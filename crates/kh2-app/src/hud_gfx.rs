//! Draws the game's 2D HUD sequences. The game's HUD objects (a sequence player plus a position,
//! a parent and a few overrides) become `Node`s; every frame their quads are built from the
//! sequence data (`kh2-image`), clipped where the game clips them, and drawn on a dedicated 2D
//! camera that shows the game's 512x448 screen scaled into the window.

use std::collections::HashMap;

use bevy::asset::{embedded_asset, RenderAssetUsages};
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::ScalingMode;
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Mesh2d, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d, RenderPipelineDescriptor,
    SpecializedMeshPipelineError, TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin, MeshMaterial2d};
use kh2_image::seqd::{Ext, Player, Quad, Seqd};

use super::hud_text::{Label, TextSys};
use crate::assets::Disc;

/// Render layer of the HUD camera and everything it draws.
pub const HUD_LAYER: usize = 7;
/// The game's screen, in HUD units.
pub const SCREEN: [f32; 2] = [512.0, 448.0];
/// The HUD draw code places everything at GS y = 0x7300 + 16 * y while the screen origin is 0x7200
/// (a 448-line frame centred on 2048), so HUD y = 0 is 16 lines below the top of the picture.
/// Inferred from those two constants; the draw-environment setup itself was not found.
pub const Y_BIAS: f32 = 16.0;
/// Logic step of the HUD players.
const STEP: f32 = 1.0 / 60.0;

pub struct HudGfxPlugin;

impl Plugin for HudGfxPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "hud_quad.wgsl");
        app.add_plugins(Material2dPlugin::<HudMaterial>::default())
            .init_resource::<HudScene>()
            .init_resource::<HudDraw>()
            .add_systems(Startup, setup_camera)
            .add_systems(Update, draw.in_set(crate::schedule::Frame::Popups));
    }
}

/// Two-triangle-list material: texture x vertex colour, with the GS blend presets.
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
#[bind_group_data(HudKey)]
pub struct HudMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    /// 0 = normal alpha, 1 = additive, 2 = subtractive.
    pub blend: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct HudKey {
    blend: u32,
}

impl From<&HudMaterial> for HudKey {
    fn from(m: &HudMaterial) -> HudKey {
        HudKey { blend: m.blend }
    }
}

impl Material2d for HudMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path("embedded://kh2_game/hud_quad.wgsl".into())
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let (src, dst, op) = match key.bind_group_data.blend {
            1 => (BlendFactor::SrcAlpha, BlendFactor::One, BlendOperation::Add),
            2 => (BlendFactor::SrcAlpha, BlendFactor::One, BlendOperation::ReverseSubtract),
            _ => return Ok(()),
        };
        if let Some(target) = descriptor.fragment.as_mut().and_then(|f| f.targets.get_mut(0)).and_then(|t| t.as_mut()) {
            target.blend = Some(BlendState {
                color: BlendComponent { src_factor: src, dst_factor: dst, operation: op },
                alpha: BlendComponent::OVER,
            });
        }
        Ok(())
    }
}

/// One loaded `.2dd`-style pair: an image and the sequence that draws from it.
pub struct SeqFile {
    pub seq: Seqd,
    pub width: u32,
    pub height: u32,
    pub linear: Handle<Image>,
    pub nearest: Handle<Image>,
}

impl SeqFile {
    pub fn new(img: &kh2_image::imgd::Image, seq: Seqd, images: &mut Assets<Image>) -> SeqFile {
        let mut make = |filter: ImageFilterMode| {
            let mut image = Image::new(
                Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 },
                TextureDimension::D2,
                img.rgba.clone(),
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                mag_filter: filter,
                min_filter: filter,
                ..ImageSamplerDescriptor::linear()
            });
            images.add(image)
        };
        SeqFile { seq, width: img.width, height: img.height, linear: make(ImageFilterMode::Linear), nearest: make(ImageFilterMode::Nearest) }
    }

    /// Reads the `(IMGD, SEQD)` BAR entry pairs of a `.2dd`.
    pub fn from_bar(data: &[u8], images: &mut Assets<Image>) -> Vec<SeqFile> {
        let Ok(entries) = kh2_bar::parse(data) else { return Vec::new() };
        entries
            .windows(2)
            .filter_map(|w| {
                let img = kh2_image::imgd::decode(w[0].data)?;
                let seq = kh2_image::seqd::parse(w[1].data)?;
                Some(SeqFile::new(&img, seq, images))
            })
            .collect()
    }
}

/// Which screen edge a root node keeps its distance from. The original lays the HUD out on its 512 wide
/// screen (4:3). On a wider window the extra width is split in two: `Left` nodes move out to the left
/// edge, `Right` nodes to the right edge, so their distance from that edge stays what the 4:3 layout
/// has; `Center` nodes (messages, banners, the area card, enemy gauges) stay where they are.
/// This is the user's widescreen anchoring (WP-FIX-12), not a rule of the original; `KH2_HUD_43=1`
/// turns it off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Edge {
    #[default]
    Center,
    Left,
    Right,
}

/// One HUD object: a sequence player at a position, optionally positioned from a parent.
pub struct Node {
    pub file: usize,
    pub player: Player,
    pub parent: Option<usize>,
    /// Position of a root node.
    pub pos: [f32; 2],
    /// Screen edge a root node is anchored to (children follow their parent).
    pub edge: Edge,
    /// Offset added to this node's own drawing position (and passed on to its children).
    pub off: [f32; 2],
    pub ext: Ext,
    pub visible: bool,
    /// Clip to the wedge from `sweep_start` clockwise by this many radians (a ring gauge).
    pub sweep: Option<f32>,
    /// Where the wedge starts: radians clockwise from the west direction (0 for Sora's ring, south
    /// for the party rings; see `hud_party`).
    pub sweep_start: f32,
    /// Show only this fraction of the node's width, from its left edge (a gauge fill; stands in for
    /// the game's rotating mask where that was not ported).
    pub reveal: Option<f32>,
    /// Hold the node's sequence at this frame (a harness aid for screenshots of short animations).
    pub freeze: Option<f32>,
    /// Text drawn on top of the node's sequence, from the node's origin.
    pub label: Option<Label>,
    /// Alpha multiplier for the label (the node's fade).
    pub label_fade: f32,
    /// Logic ticks the label has been drawn for: the text engine's typewriter budget (the game's
    /// message object counts its draws at +0x1c). Reset when a new label is set.
    pub label_time: f32,
    /// Follow-up group (the game's object field +0x17c, -1 when unset): when a group that plays a
    /// limited number of times has finished, the object switches to this group and steps it once
    /// (0x138aa8). Set by `HudScene::set_group_then`; a looping group never hands over.
    pub next: Option<usize>,
    /// Last anchor point the group's anchor animation produced.
    anchor: [f32; 2],
    base: [f32; 2],
    origin: [f32; 2],
}

impl Node {
    pub fn new(file: usize, parent: Option<usize>) -> Node {
        Node {
            file,
            player: Player::default(),
            parent,
            pos: [0.0; 2],
            edge: Edge::Center,
            off: [0.0; 2],
            ext: Ext::default(),
            visible: true,
            sweep: None,
            sweep_start: 0.0,
            reveal: None,
            freeze: None,
            label: None,
            label_fade: 1.0,
            label_time: 0.0,
            next: None,
            anchor: [0.0; 2],
            base: [0.0; 2],
            origin: [0.0; 2],
        }
    }
}

#[derive(Resource, Default)]
pub struct HudScene {
    pub files: Vec<SeqFile>,
    pub nodes: Vec<Node>,
    /// Fonts and messages (None when the disc has none).
    pub text: Option<TextSys>,
    /// HUD units the window shows beyond the 4:3 screen on each side (0 at 4:3, or with `KH2_HUD_43=1`).
    pub margin: f32,
    acc: f32,
}

impl HudScene {
    pub fn add(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    /// Starts `group` on a node (restarting it from frame 0, as the game's group switch does).
    pub fn set_group(&mut self, node: usize, group: usize) {
        let n = &mut self.nodes[node];
        if let Some(f) = self.files.get(n.file) {
            n.player.set_group(&f.seq, group);
        }
    }

    /// Starts `group` and makes `next` its follow-up (the game writes +0x17c after the switch, as
    /// 0x157040 does for the circle and 0x153ee8 for the face).
    pub fn set_group_then(&mut self, node: usize, group: usize, next: usize) {
        self.set_group(node, group);
        self.nodes[node].next = Some(next);
    }

    /// Advances all players by real time at the game's 60 Hz logic rate.
    pub(crate) fn advance(&mut self, dt: f32) {
        self.acc = (self.acc + dt).min(0.25);
        while self.acc >= STEP {
            self.acc -= STEP;
            for n in &mut self.nodes {
                match n.freeze {
                    Some(f) => n.player.seek(f),
                    None => n.player.tick(1.0),
                }
                // 0x138aa8: a finished group hands over to the follow-up, which is stepped once.
                if n.player.finished()
                    && let (Some(g), Some(f)) = (n.next.take(), self.files.get(n.file))
                {
                    n.player.set_group(&f.seq, g);
                    n.player.tick(1.0);
                }
                if n.label.is_some() && n.visible {
                    n.label_time += 1.0;
                }
            }
        }
    }

    /// Resolves positions (parents first: a node's parent always has a lower index).
    fn resolve(&mut self) {
        for i in 0..self.nodes.len() {
            let base = match self.nodes[i].parent {
                Some(p) => {
                    let (pn, file) = (&self.nodes[p], self.files.get(self.nodes[p].file));
                    let anchor = match (pn.player.group, file) {
                        (Some(g), Some(f)) => f.seq.anchor(g, pn.player.frame(), &pn.ext).map(|a| [a[0] as f32, a[1] as f32]),
                        _ => None,
                    };
                    let anchor = anchor.unwrap_or(pn.anchor);
                    let (pb, po) = (pn.base, pn.off);
                    self.nodes[p].anchor = anchor;
                    [pb[0] + anchor[0] + po[0], pb[1] + anchor[1] + po[1]]
                }
                None => {
                    let n = &self.nodes[i];
                    let dx = match n.edge {
                        Edge::Left => -self.margin,
                        Edge::Right => self.margin,
                        Edge::Center => 0.0,
                    };
                    [n.pos[0] + dx, n.pos[1]]
                }
            };
            let n = &mut self.nodes[i];
            n.base = base;
            n.origin = [base[0] + n.off[0], base[1] + n.off[1]];
        }
    }

    /// All quads to draw, in draw order, with the texture file they use.
    fn quads(&self) -> Vec<Drawn> {
        let mut out = Vec::new();
        for n in &self.nodes {
            if !n.visible {
                continue;
            }
            if let (Some(g), Some(f)) = (n.player.group, self.files.get(n.file)) {
                let mut ext = n.ext;
                ext.time = n.player.uv_time;
                let qs = f.seq.quads(g, n.player.frame(), n.origin, &ext);
                let max_x = n.reveal.map(|r| {
                    let xs = qs.iter().flat_map(|q| q.pos.iter().map(|p| p[0]));
                    let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
                    lo + (hi - lo) * r.clamp(0.0, 1.0)
                });
                for q in qs {
                    out.push((n.file, q, n.origin, n.sweep.map(|s| (n.sweep_start, s)), max_x));
                }
            }
            if let (Some(label), Some(text)) = (&n.label, &self.text) {
                for (file, q) in text.quads(label, n.origin, n.label_fade, n.label_time) {
                    out.push((file, q, n.origin, None, None));
                }
            }
        }
        out
    }
}

/// One quad to draw: texture file, the quad, its node's origin, the ring wedge (start, sweep) and
/// the x limit.
type Drawn = (usize, Quad, [f32; 2], Option<(f32, f32)>, Option<f32>);

#[derive(Component)]
pub struct HudCamera;

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        HudCamera,
        Camera2d,
        Camera { order: 10, clear_color: ClearColorConfig::None, ..default() },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin { min_width: SCREEN[0], min_height: SCREEN[1] },
            ..OrthographicProjection::default_2d()
        }),
        // HUD coordinates have y pointing down; geometry is built with y negated.
        Transform::from_xyz(SCREEN[0] / 2.0, -SCREEN[1] / 2.0, 100.0),
        RenderLayers::layer(HUD_LAYER),
    ));
}

/// HUD units shown beyond the 512 wide screen on each side for a window of `size` pixels. The HUD camera
/// fits the 512x448 box (`ScalingMode::AutoMin`), so a wider window shows more to the left and right.
pub fn side_margin(size: Vec2) -> f32 {
    if size.x <= 0.0 || size.y <= 0.0 {
        return 0.0;
    }
    let scale = (size.x / SCREEN[0]).min(size.y / SCREEN[1]);
    ((size.x / scale - SCREEN[0]) / 2.0).max(0.0)
}

/// `KH2_HUD_43=1`: keep the original's 4:3 layout in the middle of a wide window.
fn layout_43() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_HUD_43").is_ok_and(|v| v != "0"))
}

/// `KH2_HUDLOG=<seconds>`: true on the frame that passes that time.
fn log_now(time: &Time) -> bool {
    let at: f32 = std::env::var("KH2_HUDLOG").ok().and_then(|v| v.parse().ok()).unwrap_or(-1.0);
    at >= 0.0 && time.elapsed_secs() - time.delta_secs() < at && time.elapsed_secs() >= at
}

/// Converts a point of the 3D camera's viewport (window pixels) to HUD units.
pub fn viewport_to_hud(vp: Vec2, size: Vec2) -> Vec2 {
    let scale = (size.x / SCREEN[0]).min(size.y / SCREEN[1]);
    (vp - size / 2.0) / scale + Vec2::new(SCREEN[0] / 2.0, SCREEN[1] / 2.0 - Y_BIAS)
}

#[derive(Clone, Copy)]
struct Vert {
    pos: [f32; 2],
    uv: [f32; 2],
    col: [f32; 4],
}

fn lerp_vert(a: Vert, b: Vert, t: f32) -> Vert {
    let l = |x: f32, y: f32| x + (y - x) * t;
    Vert {
        pos: [l(a.pos[0], b.pos[0]), l(a.pos[1], b.pos[1])],
        uv: [l(a.uv[0], b.uv[0]), l(a.uv[1], b.uv[1])],
        col: [l(a.col[0], b.col[0]), l(a.col[1], b.col[1]), l(a.col[2], b.col[2]), l(a.col[3], b.col[3])],
    }
}

/// Keeps the part of a convex polygon where `dot(p - c, n) >= 0`.
fn clip(poly: &[Vert], c: [f32; 2], n: [f32; 2]) -> Vec<Vert> {
    let d = |v: &Vert| (v.pos[0] - c[0]) * n[0] + (v.pos[1] - c[1]) * n[1];
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let (da, db) = (d(&a), d(&b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            out.push(lerp_vert(a, b, da / (da - db)));
        }
    }
    out
}

/// The wedge of a ring gauge: from `start` (radians clockwise from west), clockwise on screen, by
/// `sweep` radians, around `c`. Returns convex pieces (HUD y points down, so clockwise on screen
/// is a rotation by +angle). This stands in for the game's depth-buffer masks (0x13e010): a
/// square with a corner on the ring's centre, turned by the gauge's angle, plus a fixed rectangle,
/// written to the depth buffer so the ring is hidden under them; the visible remainder is the
/// wedge computed here (docs/hud.md, "Party panels").
fn wedge(poly: &[Vert], c: [f32; 2], start: f32, sweep: f32) -> Vec<Vec<Vert>> {
    use std::f32::consts::PI;
    if sweep <= 0.0 {
        return Vec::new();
    }
    // Direction at angle a clockwise from west: (-cos a, -sin a). The half-plane that holds the
    // directions up to 180 degrees before it is n = (-sin a, cos a).
    let edge = |a: f32| [-a.sin(), a.cos()];
    if sweep >= 2.0 * PI {
        return vec![poly.to_vec()];
    }
    // The half-plane of the directions from `start` clockwise to `start + pi`.
    let first = clip(poly, c, edge(start + PI));
    if sweep <= PI {
        return vec![clip(&first, c, edge(start + sweep))];
    }
    // More than half a turn: that half, plus the other half cut at the sweep edge.
    let second = clip(&clip(poly, c, edge(start)), c, edge(start + sweep));
    vec![first, second]
}

fn gamma22(v: f32) -> f32 {
    v.max(0.0).powf(2.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Vert> {
        [[-10.0, -10.0], [10.0, -10.0], [10.0, 10.0], [-10.0, 10.0]].iter().map(|&pos| Vert { pos, uv: pos, col: [1.0; 4] }).collect()
    }

    fn inside(pieces: &[Vec<Vert>], p: [f32; 2]) -> bool {
        pieces.iter().filter(|q| q.len() >= 3).any(|q| {
            let s: Vec<f32> = (0..q.len())
                .map(|i| {
                    let (a, b) = (q[i].pos, q[(i + 1) % q.len()].pos);
                    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
                })
                .collect();
            s.iter().all(|&v| v >= -1e-4) || s.iter().all(|&v| v <= 1e-4)
        })
    }

    #[test]
    fn wedge_runs_clockwise_from_west() {
        use std::f32::consts::PI;
        // HUD y points down: north is -y. A quarter turn from west reaches north.
        let q = wedge(&square(), [0.0, 0.0], 0.0, PI / 2.0);
        assert!(inside(&q, [-5.0, -5.0]));
        assert!(!inside(&q, [5.0, -5.0]));
        assert!(!inside(&q, [-5.0, 5.0]));
        let half = wedge(&square(), [0.0, 0.0], 0.0, PI);
        assert!(inside(&half, [5.0, -5.0]) && !inside(&half, [5.0, 5.0]));
        // 225 degrees: past east, into the south-east quadrant but not as far as south.
        let more = wedge(&square(), [0.0, 0.0], 0.0, PI * 1.25);
        assert!(inside(&more, [9.0, 2.0]));
        assert!(!inside(&more, [2.0, 9.0]));
        assert!(!inside(&more, [-9.0, 2.0]));
        assert!(wedge(&square(), [0.0, 0.0], 0.0, 0.0).is_empty());
        // 270 degrees from west ends at south: the south-west quadrant stays out, everything else is in.
        let three = wedge(&square(), [0.0, 0.0], 0.0, 1.5 * PI);
        assert!(!inside(&three, [-9.0, 9.0]));
        assert!(inside(&three, [2.0, 9.0]) && inside(&three, [-9.0, -2.0]));
        assert!(inside(&wedge(&square(), [0.0, 0.0], 0.0, 2.0 * PI), [-9.0, 9.0]));
    }

    #[test]
    fn wedge_can_start_elsewhere() {
        use std::f32::consts::PI;
        // The party HP ring: from south clockwise. A quarter turn covers the south-west quadrant only.
        let q = wedge(&square(), [0.0, 0.0], 1.5 * PI, PI / 2.0);
        assert!(inside(&q, [-5.0, 5.0]));
        assert!(!inside(&q, [-5.0, -5.0]) && !inside(&q, [5.0, 5.0]) && !inside(&q, [5.0, -5.0]));
        // The party MP ring: from south counter-clockwise by a quarter is the wedge that starts at
        // east and runs clockwise to south: the south-east quadrant.
        let m = wedge(&square(), [0.0, 0.0], PI, PI / 2.0);
        assert!(inside(&m, [5.0, 5.0]));
        assert!(!inside(&m, [-5.0, 5.0]) && !inside(&m, [5.0, -5.0]));
        // Three quarters from south: south-west, north-west and north-east.
        let t = wedge(&square(), [0.0, 0.0], 1.5 * PI, 1.5 * PI);
        assert!(inside(&t, [-5.0, 5.0]) && inside(&t, [-5.0, -5.0]) && inside(&t, [5.0, -5.0]));
        assert!(!inside(&t, [5.0, 5.0]));
    }

    #[test]
    fn side_margin_splits_the_extra_width() {
        // 16:9: the 448 tall box is 796.4 wide, 142.2 more on each side; 4:3 and taller windows have none.
        assert!((side_margin(Vec2::new(1280.0, 720.0)) - 142.2).abs() < 0.1);
        assert!(side_margin(Vec2::new(1024.0, 768.0)) < 1.0);
        assert_eq!(side_margin(Vec2::new(600.0, 800.0)), 0.0);
        assert_eq!(side_margin(Vec2::ZERO), 0.0);
    }

    #[test]
    fn edge_anchored_roots_move_to_the_screen_edges() {
        let mut scene = HudScene { margin: 100.0, ..HudScene::default() };
        let mut left = Node::new(0, None);
        (left.edge, left.pos) = (Edge::Left, [28.0, 300.0]);
        let mut right = Node::new(0, None);
        (right.edge, right.pos) = (Edge::Right, [450.0, 400.0]);
        let mut mid = Node::new(0, None);
        mid.pos = [256.0, 100.0];
        let mut child = Node::new(0, Some(1));
        child.off = [-10.0, 5.0];
        for n in [left, right, mid, child] {
            scene.add(n);
        }
        scene.resolve();
        assert_eq!(scene.nodes[0].origin, [-72.0, 300.0]);
        assert_eq!(scene.nodes[1].origin, [550.0, 400.0]);
        assert_eq!(scene.nodes[2].origin, [256.0, 100.0]);
        // A child keeps its parent's shift (anchor 0 with no sequence loaded).
        assert_eq!(scene.nodes[3].origin, [540.0, 405.0]);
    }

    #[test]
    fn viewport_maps_the_centre_to_the_screen_centre() {
        let c = viewport_to_hud(Vec2::new(640.0, 360.0), Vec2::new(1280.0, 720.0));
        assert_eq!((c.x, c.y), (256.0, 224.0 - Y_BIAS));
        // The 4:3 box is fitted by height: the box's right edge is at 640 + 448 * 1.607...
        let r = viewport_to_hud(Vec2::new(640.0 + 256.0 * 720.0 / 448.0, 360.0), Vec2::new(1280.0, 720.0));
        assert!((r.x - 512.0).abs() < 1e-3);
    }
}

#[derive(Resource, Default)]
pub struct HudDraw {
    pool: Vec<Entity>,
    materials: HashMap<(usize, u32, bool), Handle<HudMaterial>>,
    /// The side margin last logged (`hud: layout ...`).
    logged_margin: Option<f32>,
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    time: Res<Time>,
    mut scene: ResMut<HudScene>,
    mut state: ResMut<HudDraw>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<HudMaterial>>,
    mut batches: Query<(&mut Mesh2d, &mut MeshMaterial2d<HudMaterial>, &mut Visibility, &mut Transform)>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    scene.margin = match windows.single() {
        Ok(w) if !layout_43() => side_margin(Vec2::new(w.width(), w.height())),
        _ => 0.0,
    };
    if state.logged_margin.is_none_or(|m| (m - scene.margin).abs() > 0.5) {
        state.logged_margin = Some(scene.margin);
        if scene.margin > 0.0 {
            info!("hud: edge layout, left group at the left edge, right group at the right edge ({:.0} HUD units of margin per side)", scene.margin);
        } else {
            info!("hud: 4:3 layout (no side margin)");
        }
    }
    scene.advance(time.delta_secs());
    scene.resolve();
    // `KH2_HUDHIDE=3,9`: hide nodes by index (for isolating a drawing problem).
    if let Ok(list) = std::env::var("KH2_HUDHIDE") {
        for i in list.split(',').filter_map(|v| v.trim().parse::<usize>().ok()) {
            if let Some(n) = scene.nodes.get_mut(i) {
                n.visible = false;
            }
        }
    }
    // `KH2_HUDLOG=1`: one dump of every node a few seconds in, for checking layouts.
    if log_now(&time) {
        for (i, n) in scene.nodes.iter().enumerate() {
            let bbox = scene.files.get(n.file).zip(n.player.group).map(|(f, g)| {
                let qs = f.seq.quads(g, n.player.frame(), n.origin, &n.ext);
                let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                for q in &qs {
                    for p in q.pos {
                        x0 = x0.min(p[0]);
                        x1 = x1.max(p[0]);
                        y0 = y0.min(p[1]);
                        y1 = y1.max(p[1]);
                    }
                }
                (qs.len(), x0, y0, x1, y1)
            });
            info!(target: "kh2_hud", "node {i} file {} group {:?} frame {} vis {} origin {:?} scale {:?} sweep {:?} quads/bbox {:?}", n.file, n.player.group, n.player.frame(), n.visible, n.origin, n.ext.scale, n.sweep, bbox);
        }
    }
    // Consecutive quads with the same texture, blend and filter share a draw call.
    struct Batch {
        key: (usize, u32, bool),
        pos: Vec<[f32; 3]>,
        uv: Vec<[f32; 2]>,
        col: Vec<[f32; 4]>,
    }
    let mut out: Vec<Batch> = Vec::new();
    for (file, q, origin, sweep, max_x) in scene.quads() {
        let Some(f) = scene.files.get(file) else { continue };
        let key = (file, q.blend, q.nearest);
        // Quad corners are TL, TR, BL, BR; as a polygon: TL, TR, BR, BL.
        let poly: Vec<Vert> = [0usize, 1, 3, 2]
            .iter()
            .map(|&i| Vert {
                pos: q.pos[i],
                uv: [q.uv[i][0] / f.width as f32, q.uv[i][1] / f.height as f32],
                // 0x80 is 1.0; the product is taken in gamma space on the PS2, so convert to linear.
                col: [gamma22(q.color[i][0] as f32 / 128.0), gamma22(q.color[i][1] as f32 / 128.0), gamma22(q.color[i][2] as f32 / 128.0), q.color[i][3] as f32 / 128.0],
            })
            .collect();
        let mut pieces = match sweep {
            Some((start, s)) => wedge(&poly, origin, start, s),
            None => vec![poly],
        };
        if let Some(mx) = max_x {
            // Keep x <= mx.
            pieces = pieces.iter().map(|p| clip(p, [mx, 0.0], [-1.0, 0.0])).collect();
        }
        if out.last().map(|b| b.key) != Some(key) {
            out.push(Batch { key, pos: Vec::new(), uv: Vec::new(), col: Vec::new() });
        }
        let b = out.last_mut().unwrap();
        for p in pieces.iter().filter(|p| p.len() >= 3) {
            for k in 1..p.len() - 1 {
                for v in [p[0], p[k], p[k + 1]] {
                    b.pos.push([v.pos[0], -(v.pos[1] + Y_BIAS), 0.0]);
                    b.uv.push(v.uv);
                    b.col.push(v.col);
                }
            }
        }
    }
    if log_now(&time) {
        for (i, b) in out.iter().enumerate() {
            let xs = b.pos.iter().map(|p| p[0]);
            let ys = b.pos.iter().map(|p| p[1]);
            info!(target: "kh2_hud", "batch {i} key {:?} verts {} x {:.0}..{:.0} y {:.0}..{:.0}", b.key, b.pos.len(), xs.clone().fold(f32::MAX, f32::min), xs.fold(f32::MIN, f32::max), ys.clone().fold(f32::MAX, f32::min), ys.fold(f32::MIN, f32::max));
            if b.pos.len() <= 24 {
                for k in 0..b.pos.len() {
                    info!(target: "kh2_hud", "  v{k} pos {:?} uv {:?} col {:?}", b.pos[k], b.uv[k], b.col[k]);
                }
            }
        }
    }
    let layers = RenderLayers::layer(HUD_LAYER);
    while state.pool.len() < out.len() {
        let e = commands
            .spawn((
                Mesh2d(Handle::default()),
                MeshMaterial2d::<HudMaterial>(Handle::default()),
                Transform::default(),
                Visibility::Hidden,
                NoFrustumCulling,
                layers.clone(),
            ))
            .id();
        state.pool.push(e);
    }
    for (i, entity) in state.pool.clone().into_iter().enumerate() {
        let Ok((mut mesh2d, mut mat, mut vis, mut tf)) = batches.get_mut(entity) else { continue };
        let Some(b) = out.get(i) else {
            *vis = Visibility::Hidden;
            continue;
        };
        let (file, blend, nearest) = b.key;
        let handle = state
            .materials
            .entry(b.key)
            .or_insert_with(|| {
                let f = &scene.files[file];
                materials.add(HudMaterial { texture: if nearest { f.nearest.clone() } else { f.linear.clone() }, blend })
            })
            .clone();
        if mat.0 != handle {
            mat.0 = handle;
        }
        // A new mesh every frame: updating one in place with a changing vertex count left stale
        // triangles in the draw (the old handle is freed when the component is replaced).
        mesh2d.0 = meshes.add(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, VertexAttributeValues::Float32x3(b.pos.clone()))
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, VertexAttributeValues::Float32x2(b.uv.clone()))
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(b.col.clone())),
        );
        // Later batches draw on top.
        tf.translation.z = i as f32 * 0.01;
        *vis = if b.pos.is_empty() { Visibility::Hidden } else { Visibility::Inherited };
    }
}

/// Loads a `.2dd`-style file from the disc into scene files.
pub fn load_files(disc: &Disc, name: &str, images: &mut Assets<Image>) -> Vec<SeqFile> {
    disc.read(name).map(|d| SeqFile::from_bar(&d, images)).unwrap_or_default()
}
