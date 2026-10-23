//! Draws the pause menu's 2D sequences: the `.2ld` image and its sequences (`kh2_image::layd`) and
//! the menu's text, on a camera of their own above the HUD's. The draw path is the HUD's (docs/systems/ui/contract.md:
//! a node plays an animation group, its quads are batched by texture and blend, vertex colours are
//! multiplied in gamma space) minus the parts the menu does not use (ring wedges, reveals); it is a
//! copy because `hud_gfx.rs` is private to the HUD module, which other work is editing. The shader is
//! the HUD's embedded `hud_quad.wgsl`.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
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

use crate::menu_text::{Label, MenuText};

/// Render layer of the menu camera and everything it draws (the HUD's is 7).
pub const MENU_LAYER: usize = 8;
/// Render layer of the dim: its own camera (order 11) between the HUD's (10) and the HUD's ring panels (12, WP-MN6), so
/// the field and the HUD are dimmed but the party rings stay bright (clip 10) and the menu frame (13) is over both.
pub const DIM_LAYER: usize = 9;
/// The game's screen, in menu units (the same canvas as the HUD's).
pub const SCREEN: [f32; 2] = [512.0, 448.0];
/// Menu draws go through the same 2D draw path as the HUD, so the same 16 line offset applies
/// (`hud_gfx::Y_BIAS`; the menu's own draw environment was not read, [inferred]).
pub const Y_BIAS: f32 = 16.0;
/// Logic step of the menu's players, in 60 Hz ticks of real time.
const STEP: f32 = 1.0 / 60.0;

pub struct MenuGfxPlugin;

impl Plugin for MenuGfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<MenuMaterial>::default())
            .init_resource::<MenuScene>()
            .init_resource::<MenuDraw>()
            .add_systems(Startup, (setup_camera, window_size))
            .add_systems(Update, (draw, draw_dim).in_set(crate::schedule::Frame::Popups));
    }
}

/// Texture x vertex colour, with the GS blend presets (a copy of `hud_gfx::HudMaterial`).
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
#[bind_group_data(MenuKey)]
pub struct MenuMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
    /// 0 = normal alpha, 1 = additive, 2 = subtractive.
    pub blend: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct MenuKey {
    blend: u32,
}

impl From<&MenuMaterial> for MenuKey {
    fn from(m: &MenuMaterial) -> MenuKey {
        MenuKey { blend: m.blend }
    }
}

impl Material2d for MenuMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(concat!("embedded://", env!("CARGO_CRATE_NAME"), "/hud_quad.wgsl").into())
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

/// One texture the menu draws from, in a linear and a nearest-neighbour sampling.
pub struct Tex {
    pub width: u32,
    pub height: u32,
    pub linear: Handle<Image>,
    pub nearest: Handle<Image>,
}

impl Tex {
    pub fn new(img: &kh2_image::imgd::Image, images: &mut Assets<Image>) -> Tex {
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
        Tex { width: img.width, height: img.height, linear: make(ImageFilterMode::Linear), nearest: make(ImageFilterMode::Nearest) }
    }
}

/// A whole texture drawn as one rectangle: `rect` (x0, y0, x1, y1) is relative to the node's origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pic {
    pub tex: usize,
    pub rect: [f32; 4],
    /// The part of the texture's height that is drawn (1.0: all of it).
    pub v_end: f32,
}

/// One menu object: a sequence player at a position, optionally positioned from a parent's anchor.
pub struct Node {
    /// Index into `MenuScene::seqs` (`None`: a text-only node).
    pub seq: Option<usize>,
    /// Texture of `MenuScene::texs` the sequence draws from (`CAMP_TEX`; the PAUSE band's image otherwise).
    pub tex: usize,
    pub player: Player,
    pub parent: Option<usize>,
    /// Position of a root node.
    pub pos: [f32; 2],
    /// Offset added to this node's own drawing position.
    pub off: [f32; 2],
    pub ext: Ext,
    pub visible: bool,
    pub label: Option<Label>,
    /// Draw the label from the node's anchor plus this offset instead of from its origin (the row
    /// objects' text starts at the row group's anchor point, `FUN_00139048` with the group offset
    /// flag cleared by `FUN_00139888`).
    pub label_anchor: Option<[f32; 2]>,
    /// A plain rectangle (x0, y0, x1, y1 in menu units) with its four corner colours, drawn first.
    pub fill: Option<([f32; 4], [[u8; 4]; 4])>,
    /// A picture from another texture of `MenuScene::texs` (the item pictures, `itempic/item-NNN.imd`).
    pub pic: Option<Pic>,
    /// Alpha multiplier of the label.
    pub label_fade: f32,
    /// A frame part (header band, side rails, bottom panels, backdrop) that reaches the 4:3 canvas edges: its box at
    /// rest (x0, y0, x1, y1). The sides of it that touch the canvas edges are carried out to the window's edges
    /// (`stretch_quads`, docs/systems/ui/menus-pause-m1.md 9.1).
    pub stretch: Option<[f32; 4]>,
    /// Follow-up group: when a group that plays a limited number of times has finished, the node
    /// switches to this group and steps it once (`hud_gfx::HudScene`, 0x138aa8).
    pub next: Option<usize>,
    anchor: [f32; 2],
    base: [f32; 2],
    origin: [f32; 2],
}

impl Node {
    pub fn new(seq: Option<usize>) -> Node {
        Node {
            seq,
            tex: CAMP_TEX,
            player: Player::default(),
            parent: None,
            pos: [0.0; 2],
            off: [0.0; 2],
            ext: Ext::default(),
            visible: true,
            label: None,
            label_anchor: None,
            fill: None,
            pic: None,
            label_fade: 1.0,
            stretch: None,
            next: None,
            anchor: [0.0; 2],
            base: [0.0; 2],
            origin: [0.0; 2],
        }
    }
}

/// Texture index of the `camp.2ld` image, and of the plain white pixel the dim is drawn with.
pub const CAMP_TEX: usize = 0;
pub const WHITE_TEX: usize = 1;

/// What the menu draws this frame.
#[derive(Resource, Default)]
pub struct MenuScene {
    pub texs: Vec<Tex>,
    pub seqs: Vec<Seqd>,
    pub nodes: Vec<Node>,
    pub text: Option<MenuText>,
    /// Alpha of the black the field is dimmed with (0 = none).
    pub dim: f32,
    /// Alpha of a white fade over the field, drawn even while `hidden` (the knock-out event's screen fade, `FUN_0014a710` flag 1:
    /// white, docs/systems/ui/menus-gameover.md 26.8). It replaces the black dim while it is above the dim's alpha.
    pub fade: f32,
    /// Draw nothing at all (the menu is closed).
    pub hidden: bool,
    /// Frames the players advance per 60 Hz tick (the clip's pace, docs/systems/ui/menus-pause.md 1.6).
    pub speed: f32,
    acc: f32,
    /// The window's visible area in menu units (x0, y0, x1, y1), set by `draw` (zero until the first frame: the canvas).
    pub view: [f32; 4],
}

impl MenuScene {
    pub fn add(&mut self, node: Node) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    /// The `x` and `y` fields of a group of sequence `seq` (the plate's extent for the row groups: the NEW tag sits
    /// there, `FUN_00138ed0(tag, FUN_00237618(row + 0x18), FUN_00237638(row + 0x18))`).
    pub fn extent(&self, seq: usize, group: usize) -> [f32; 2] {
        self.seqs.get(seq).and_then(|s| s.groups.get(group)).map_or([0.0; 2], |g| [g.x as f32, g.y as f32])
    }

    /// Starts `group` on a node (restarting it from frame 0, as the game's group switch does).
    pub fn set_group(&mut self, node: usize, group: usize) {
        let n = &mut self.nodes[node];
        n.next = None;
        if let Some(s) = n.seq.and_then(|s| self.seqs.get(s)) {
            n.player.set_group(s, group);
        }
    }

    /// Starts `group` and makes `next` its follow-up (the game writes +0x17c after the switch).
    pub fn set_group_then(&mut self, node: usize, group: usize, next: usize) {
        self.set_group(node, group);
        self.nodes[node].next = Some(next);
    }

    /// Advances all players by real time at 60 Hz, `speed` frames per tick. Returns the number of
    /// ticks taken.
    pub fn advance(&mut self, dt: f32) -> u32 {
        self.acc = (self.acc + dt).min(0.25);
        let mut ticks = 0;
        let speed = if self.speed > 0.0 { self.speed } else { 1.0 };
        while self.acc >= STEP {
            self.acc -= STEP;
            ticks += 1;
            for n in &mut self.nodes {
                n.player.tick(speed);
                if n.player.finished()
                    && let (Some(g), Some(s)) = (n.next.take(), n.seq.and_then(|s| self.seqs.get(s)))
                {
                    n.player.set_group(s, g);
                    n.player.tick(speed);
                }
            }
        }
        ticks
    }

    /// The visible area in menu units (the canvas until `draw` has measured the window).
    pub fn view(&self) -> [f32; 4] {
        if self.view == [0.0; 4] { CANVAS } else { self.view }
    }

    /// The quads node `i` draws now with the window-edge carry applied (`stretch_quads`), for the harness log.
    pub fn node_quads_drawn(&self, i: usize) -> Vec<Quad> {
        let mut qs = self.node_quads(i);
        if let Some(rest) = self.nodes[i].stretch {
            stretch_quads(&mut qs, rest, self.view());
        }
        qs
    }

    /// The quads node `i` draws now (its sequence group at the current frame), for logs and tests.
    pub fn node_quads(&self, i: usize) -> Vec<Quad> {
        let n = &self.nodes[i];
        match (n.player.group, n.seq.and_then(|s| self.seqs.get(s))) {
            (Some(g), Some(s)) if n.visible => {
                let mut ext = n.ext;
                ext.time = n.player.uv_time;
                s.quads(g, n.player.frame(), n.origin, &ext)
            }
            _ => Vec::new(),
        }
    }

    /// Resolves positions (parents first: a node's parent always has a lower index).
    pub fn resolve(&mut self) {
        for i in 0..self.nodes.len() {
            let base = match self.nodes[i].parent {
                Some(p) => {
                    let pn = &self.nodes[p];
                    let anchor = match (pn.player.group, pn.seq.and_then(|s| self.seqs.get(s))) {
                        (Some(g), Some(s)) => s.anchor(g, pn.player.frame(), &pn.ext).map(|a| [a[0] as f32, a[1] as f32]),
                        _ => None,
                    };
                    let anchor = anchor.unwrap_or(pn.anchor);
                    let (pb, po) = (pn.base, pn.off);
                    self.nodes[p].anchor = anchor;
                    [pb[0] + anchor[0] + po[0], pb[1] + anchor[1] + po[1]]
                }
                None => self.nodes[i].pos,
            };
            let n = &mut self.nodes[i];
            n.base = base;
            n.origin = [base[0] + n.off[0], base[1] + n.off[1]];
        }
    }

    /// All quads to draw, in draw order, with the texture each uses.
    fn quads(&self) -> Vec<(usize, Quad)> {
        let mut out = Vec::new();
        if self.hidden {
            return out;
        }
        for n in &self.nodes {
            if !n.visible {
                continue;
            }
            if let Some(([x0, y0, x1, y1], color)) = n.fill {
                out.push((WHITE_TEX, Quad { pos: [[x0, y0], [x1, y0], [x0, y1], [x1, y1]], uv: [[0.5; 2]; 4], color, blend: 0, nearest: true }));
            }
            if let (Some(p), Some(t)) = (n.pic, n.pic.and_then(|p| self.texs.get(p.tex))) {
                let [x0, y0, x1, y1] = [n.origin[0] + p.rect[0], n.origin[1] + p.rect[1], n.origin[0] + p.rect[2], n.origin[1] + p.rect[3]];
                let (w, h) = (t.width as f32, t.height as f32 * p.v_end);
                out.push((
                    p.tex,
                    Quad { pos: [[x0, y0], [x1, y0], [x0, y1], [x1, y1]], uv: [[0.0, 0.0], [w, 0.0], [0.0, h], [w, h]], color: [[0x80, 0x80, 0x80, (128.0 * n.label_fade.clamp(0.0, 1.0)) as u8]; 4], blend: 0, nearest: false },
                ));
            }
            if let (Some(g), Some(s)) = (n.player.group, n.seq.and_then(|s| self.seqs.get(s))) {
                let mut ext = n.ext;
                ext.time = n.player.uv_time;
                let mut qs = s.quads(g, n.player.frame(), n.origin, &ext);
                if let Some(rest) = n.stretch {
                    stretch_quads(&mut qs, rest, self.view());
                }
                for q in qs {
                    out.push((n.tex, q));
                }
            }
            if let (Some(label), Some(text)) = (&n.label, &self.text) {
                let mut at = n.origin;
                if let Some(o) = n.label_anchor {
                    let anchor = match (n.player.group, n.seq.and_then(|s| self.seqs.get(s))) {
                        (Some(g), Some(s)) => s.anchor(g, n.player.frame(), &n.ext).map(|a| [a[0] as f32, a[1] as f32]),
                        _ => None,
                    };
                    let a = anchor.unwrap_or(n.anchor);
                    at = [at[0] + a[0] + o[0], at[1] + a[1] + o[1]];
                }
                out.extend(text.quads(label, at, n.label_fade));
            }
        }
        out
    }
}

#[derive(Component)]
pub struct MenuCamera;

/// `KH2_WINDOW=<w>x<h>` (harness aid): the window's size, to look at the menu's edges at other aspects.
fn window_size(mut win: Query<&mut Window, With<bevy::window::PrimaryWindow>>) {
    let Some((w, h)) = std::env::var("KH2_WINDOW").ok().and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)))) else {
        return;
    };
    if let Ok(mut win) = win.single_mut() {
        win.resolution.set(w, h);
        info!("menu: window size {w}x{h} (KH2_WINDOW)");
    }
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        MenuCamera,
        Camera2d,
        Camera { order: 13, clear_color: ClearColorConfig::None, ..default() },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin { min_width: SCREEN[0], min_height: SCREEN[1] },
            ..OrthographicProjection::default_2d()
        }),
        // Menu coordinates have y pointing down; geometry is built with y negated.
        Transform::from_xyz(SCREEN[0] / 2.0, -SCREEN[1] / 2.0, 100.0),
        RenderLayers::layer(MENU_LAYER),
    ));
    // The dim: a black sprite on a camera of its own (see `DIM_LAYER`).
    commands.spawn((
        Camera2d,
        Camera { order: 11, clear_color: ClearColorConfig::None, ..default() },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin { min_width: SCREEN[0], min_height: SCREEN[1] },
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(SCREEN[0] / 2.0, -SCREEN[1] / 2.0, 100.0),
        RenderLayers::layer(DIM_LAYER),
    ));
    commands.spawn((
        MenuDim,
        Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, 0.0), Vec2::splat(6500.0)),
        Transform::from_xyz(SCREEN[0] / 2.0, -SCREEN[1] / 2.0, 0.0),
        Visibility::Hidden,
        RenderLayers::layer(DIM_LAYER),
    ));
}

/// The black the field is dimmed with (`MenuScene::dim`, the same alpha the old dim quad had).
#[derive(Component)]
struct MenuDim;

/// The colour and alpha of the dim sprite: the white fade when it is the stronger, else the black dim (none while hidden).
pub fn dim_paint(hidden: bool, dim: f32, fade: f32) -> (f32, f32) {
    let dim = if hidden { 0.0 } else { dim.clamp(0.0, 1.0) };
    let fade = fade.clamp(0.0, 1.0);
    if fade > dim { (fade, 1.0) } else { (dim, 0.0) }
}

fn draw_dim(scene: Res<MenuScene>, mut dim: Query<(&mut Sprite, &mut Visibility), With<MenuDim>>) {
    let (a, c) = dim_paint(scene.hidden, scene.dim, scene.fade);
    for (mut s, mut v) in &mut dim {
        s.color = Color::srgba(c, c, c, a);
        *v = if a > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
    }
}

#[derive(Clone, Copy)]
struct Vert {
    pos: [f32; 2],
    uv: [f32; 2],
    col: [f32; 4],
}

fn gamma22(v: f32) -> f32 {
    v.max(0.0).powf(2.2)
}

#[derive(Resource, Default)]
pub struct MenuDraw {
    pool: Vec<Entity>,
    materials: HashMap<(usize, u32, bool), Handle<MenuMaterial>>,
}

/// The 4:3 canvas in menu units (x0, y0, x1, y1): 512 x 448 with the 16 line offset.
pub const CANVAS: [f32; 4] = [0.0, -Y_BIAS, SCREEN[0], SCREEN[1] - Y_BIAS];

/// How close to a canvas edge a quad side must lie (in its rest place) to be carried to the window's edge: 16 units
/// (the bottom panels end at 420 of 432, the header band starts at -4) [inferred: the classification; the HD
/// remaster's frames show these parts at the screen edges].
const EDGE: f32 = 16.0;

/// Carries the sides of a frame part's quads that lie at the 4:3 canvas edges out to the window's edges. `rest` is
/// the part's box at rest; the quads' box now may be moved by the part's own slide, and the carried sides move with
/// it (so the enter and leave slides keep their shape). Per quad: its left side when it starts within `EDGE` of the
/// canvas's left edge (or beyond it) and the part reaches into the canvas, likewise right, top and bottom. A side
/// already beyond the window's edge stays where it is.
pub fn stretch_quads(qs: &mut [Quad], rest: [f32; 4], view: [f32; 4]) {
    if qs.is_empty() {
        return;
    }
    let mut now = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in qs.iter().flat_map(|q| q.pos) {
        now = [now[0].min(p[0]), now[1].min(p[1]), now[2].max(p[0]), now[3].max(p[1])];
    }
    // The part's slide (now against rest) and whether it reaches into the canvas at all on each axis.
    let slide = [now[0] - rest[0], now[1] - rest[1]];
    let spans_x = rest[2] > CANVAS[0] + EDGE && rest[0] < CANVAS[2] - EDGE;
    let spans_y = rest[3] > CANVAS[1] + EDGE && rest[1] < CANVAS[3] - EDGE;
    let pic_shift = picture_shifts(qs, slide, view, [spans_x, spans_y]);
    for (idx, q) in qs.iter_mut().enumerate() {
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in q.pos {
            b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
        }
        // Only a quad whose texture is already drawn stretched along the axis (a band or a flat fill: its texture span is
        // at most half its size) is carried; a picture (the Mickey watermark) keeps its shape and is translated below.
        let span = |i: usize| {
            let (lo, hi) = q.uv.iter().fold((f32::MAX, f32::MIN), |(lo, hi), u| (lo.min(u[i]), hi.max(u[i])));
            hi - lo
        };
        let (band_x, band_y) = (span(0) <= 0.5 * (b[2] - b[0]), span(1) <= 0.5 * (b[3] - b[1]));
        let left = spans_x && band_x && b[0] - slide[0] <= CANVAS[0] + EDGE;
        let right = spans_x && band_x && b[2] - slide[0] >= CANVAS[2] - EDGE;
        let top = spans_y && band_y && b[1] - slide[1] <= CANVAS[1] + EDGE;
        let bottom = spans_y && band_y && b[3] - slide[1] >= CANVAS[3] - EDGE;
        let shift = pic_shift[idx].unwrap_or([0.0; 2]);
        for p in q.pos.iter_mut() {
            p[0] += shift[0];
            p[1] += shift[1];
            if left && p[0] <= b[0] + 0.5 {
                p[0] = p[0].min(view[0] + slide[0]);
            }
            if right && p[0] >= b[2] - 0.5 {
                p[0] = p[0].max(view[2] + slide[0]);
            }
            if top && p[1] <= b[1] + 0.5 {
                p[1] = p[1].min(view[1] + slide[1]);
            }
            if bottom && p[1] >= b[3] - 0.5 {
                p[1] = p[1].max(view[3] + slide[1]);
            }
        }
    }
}


/// The translation of each picture quad of a frame part (a quad whose texture is not stretched along an axis, such as the
/// Mickey watermark): a group of touching picture quads that reaches one canvas edge of an axis (within `EDGE` of it, not of
/// the opposite one) moves, as a whole and without a change of size, by the distance of the window's edge to the canvas
/// edge, so it keeps its distance to the screen's corner. The group is judged at the part's rest place (`slide` removed).
fn picture_shifts(qs: &[Quad], slide: [f32; 2], view: [f32; 4], spans: [bool; 2]) -> Vec<Option<[f32; 2]>> {
    let boxes: Vec<[f32; 4]> = qs
        .iter()
        .map(|q| {
            let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            for p in q.pos {
                b = [b[0].min(p[0] - slide[0]), b[1].min(p[1] - slide[1]), b[2].max(p[0] - slide[0]), b[3].max(p[1] - slide[1])];
            }
            b
        })
        .collect();
    let mut out = vec![None; qs.len()];
    let tied = bar_parts(qs, &boxes);
    for axis in 0..2 {
        if !spans[axis] {
            continue;
        }
        let pic: Vec<usize> = (0..qs.len())
            .filter(|&i| {
                !tied[i] && {
                    let (lo, hi) = qs[i].uv.iter().fold((f32::MAX, f32::MIN), |(lo, hi), u| (lo.min(u[axis]), hi.max(u[axis])));
                    hi - lo > 0.5 * (boxes[i][axis + 2] - boxes[i][axis])
                }
            })
            .collect();
        // Group the touching picture quads (their boxes overlap or meet within one unit).
        let mut group: Vec<usize> = (0..qs.len()).collect();
        for &i in &pic {
            for &j in &pic {
                let (a, b) = (boxes[i], boxes[j]);
                if a[0] <= b[2] + 1.0 && b[0] <= a[2] + 1.0 && a[1] <= b[3] + 1.0 && b[1] <= a[3] + 1.0 {
                    let (gi, gj) = (group[i], group[j]);
                    let keep = gi.min(gj);
                    for g in group.iter_mut() {
                        if *g == gi || *g == gj {
                            *g = keep;
                        }
                    }
                }
            }
        }
        for &i in &pic {
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for &j in pic.iter().filter(|&&j| group[j] == group[i]) {
                lo = lo.min(boxes[j][axis]);
                hi = hi.max(boxes[j][axis + 2]);
            }
            let (c0, c1) = (CANVAS[axis], CANVAS[axis + 2]);
            let d = match (lo <= c0 + EDGE, hi >= c1 - EDGE) {
                (false, true) => view[axis + 2] - c1,
                (true, false) => view[axis] - c0,
                _ => continue,
            };
            out[i].get_or_insert([0.0; 2])[axis] = d;
        }
    }
    out
}

/// How far apart (units) two pieces of a bar may lie and still belong to it: Jiminy's icon stands 5 units left of the bar's cap.
const BAR_GAP: f32 = 8.0;

/// The quads of bars made of pieces: a chain of touching quads (boxes meet within `BAR_GAP`) that holds a piece drawn
/// stretched along one axis only (a flat body between two rounded caps, such as the help bar with Jiminy's icon). Such a
/// chain is one object whose pieces must keep their places to each other (and to the text drawn on it), so its
/// pictures are not translated by `picture_shifts`: a cap moved to the window's edge while the body stays tore the bar
/// apart (user report 2026-10-16) [inferred: the remaster's rule for these bars is not known].
fn bar_parts(qs: &[Quad], boxes: &[[f32; 4]]) -> Vec<bool> {
    let n = qs.len();
    let spans = |i: usize| {
        let b = boxes[i];
        let mut flat = [false; 2];
        for (axis, f) in flat.iter_mut().enumerate() {
            let (lo, hi) = qs[i].uv.iter().fold((f32::MAX, f32::MIN), |(lo, hi), u| (lo.min(u[axis]), hi.max(u[axis])));
            *f = hi - lo <= 0.5 * (b[axis + 2] - b[axis]);
        }
        flat
    };
    let flat: Vec<[bool; 2]> = (0..n).map(spans).collect();
    // Only quads with a picture along some axis join a chain (a flat fill on both axes is a panel, not a part).
    let member: Vec<bool> = flat.iter().map(|f| !(f[0] && f[1])).collect();
    let mut group: Vec<usize> = (0..n).collect();
    for i in (0..n).filter(|&i| member[i]) {
        for j in (0..n).filter(|&j| member[j]) {
            let (a, b) = (boxes[i], boxes[j]);
            if a[0] <= b[2] + BAR_GAP && b[0] <= a[2] + BAR_GAP && a[1] <= b[3] + BAR_GAP && b[1] <= a[3] + BAR_GAP {
                let (gi, gj) = (group[i], group[j]);
                let keep = gi.min(gj);
                for g in group.iter_mut() {
                    if *g == gi || *g == gj {
                        *g = keep;
                    }
                }
            }
        }
    }
    (0..n).map(|i| member[i] && (0..n).any(|j| member[j] && group[j] == group[i] && flat[j][0] != flat[j][1])).collect()
}

/// The part of the menu canvas the window shows (the menu camera's `AutoMin` fit around the canvas middle).
pub fn window_view(w: f32, h: f32) -> [f32; 4] {
    if w <= 0.0 || h <= 0.0 {
        return CANVAS;
    }
    let s = (w / SCREEN[0]).min(h / SCREEN[1]);
    let (cx, cy) = ((CANVAS[0] + CANVAS[2]) / 2.0, (CANVAS[1] + CANVAS[3]) / 2.0);
    [cx - w / s / 2.0, cy - h / s / 2.0, cx + w / s / 2.0, cy + h / s / 2.0]
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut scene: ResMut<MenuScene>,
    mut state: ResMut<MenuDraw>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MenuMaterial>>,
    mut batches: Query<(&mut Mesh2d, &mut MeshMaterial2d<MenuMaterial>, &mut Visibility, &mut Transform)>,
) {
    if let Ok(w) = window.single() {
        scene.view = window_view(w.width(), w.height());
    }
    scene.resolve();
    struct Batch {
        key: (usize, u32, bool),
        pos: Vec<[f32; 3]>,
        uv: Vec<[f32; 2]>,
        col: Vec<[f32; 4]>,
    }
    let mut out: Vec<Batch> = Vec::new();
    for (file, q) in scene.quads() {
        let Some(t) = scene.texs.get(file) else { continue };
        let key = (file, q.blend, q.nearest);
        // Quad corners are TL, TR, BL, BR; as a polygon: TL, TR, BR, BL.
        let poly: Vec<Vert> = [0usize, 1, 3, 2]
            .iter()
            .map(|&i| Vert {
                pos: q.pos[i],
                uv: [q.uv[i][0] / t.width as f32, q.uv[i][1] / t.height as f32],
                // 0x80 is 1.0; the product is taken in gamma space on the PS2, so convert to linear.
                col: [gamma22(q.color[i][0] as f32 / 128.0), gamma22(q.color[i][1] as f32 / 128.0), gamma22(q.color[i][2] as f32 / 128.0), q.color[i][3] as f32 / 128.0],
            })
            .collect();
        if out.last().map(|b| b.key) != Some(key) {
            out.push(Batch { key, pos: Vec::new(), uv: Vec::new(), col: Vec::new() });
        }
        let b = out.last_mut().unwrap();
        for k in 1..poly.len() - 1 {
            for v in [poly[0], poly[k], poly[k + 1]] {
                b.pos.push([v.pos[0], -(v.pos[1] + Y_BIAS), 0.0]);
                b.uv.push(v.uv);
                b.col.push(v.col);
            }
        }
    }
    let layers = RenderLayers::layer(MENU_LAYER);
    while state.pool.len() < out.len() {
        let e = commands
            .spawn((
                Mesh2d(Handle::default()),
                MeshMaterial2d::<MenuMaterial>(Handle::default()),
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
                let t = &scene.texs[file];
                materials.add(MenuMaterial { texture: if nearest { t.nearest.clone() } else { t.linear.clone() }, blend })
            })
            .clone();
        if mat.0 != handle {
            mat.0 = handle;
        }
        // A new mesh every frame, as the HUD does (an in-place update with a changing vertex count
        // left stale triangles in its draw).
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

/// A 1x1 white texture (the dim quad samples it).
pub fn white_tex(images: &mut Assets<Image>) -> Tex {
    Tex::new(&kh2_image::imgd::Image { width: 1, height: 1, rgba: vec![255; 4], swizzled: false }, images)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(x0: f32, y0: f32, x1: f32, y1: f32, uv: [f32; 4]) -> Quad {
        let [u0, v0, u1, v1] = uv;
        Quad { pos: [[x0, y0], [x1, y0], [x0, y1], [x1, y1]], uv: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]], color: [[0x80; 4]; 4], blend: 0, nearest: true }
    }

    /// The frame parts of `camp.2ld` (boxes from the `KH2_MENU_QUADS` dump of the frame group 39) at 16:9: the header
    /// band and the bottom panels reach the window's edges, the rounded caps and the Mickey picture keep their places.
    #[test]
    fn frame_parts_reach_the_window_edges() {
        let view = window_view(1280.0, 720.0);
        assert!((view[0] + 142.2).abs() < 0.1 && (view[2] - 654.2).abs() < 0.1 && view[1] == -16.0 && view[3] == 432.0, "{view:?}");
        assert_eq!(window_view(1024.0, 896.0), CANVAS, "an 8:7 window shows the canvas");
        let rest = [-94.0, -4.0, 539.0, 420.0];
        let band = [24.5, 215.5, 24.5, 215.5];
        let mut qs = vec![
            quad(55.0, -4.0, 525.0, 55.0, band),
            quad(-94.0, -4.0, 19.0, 55.0, band),
            quad(143.0, 338.0, 539.0, 420.0, band),
            quad(-6.0, 338.0, 107.0, 420.0, band),
            quad(476.0, 341.0, 523.0, 417.0, [106.0, 140.0, 153.0, 216.0]),
            quad(429.0, 341.0, 476.0, 417.0, [106.0, 140.0, 153.0, 216.0]),
            quad(24.0, 59.0, 35.0, 70.0, [22.0, 191.0, 33.0, 202.0]),
        ];
        stretch_quads(&mut qs, rest, view);
        let b = |q: &Quad| [q.pos[0][0], q.pos[0][1], q.pos[3][0], q.pos[3][1]];
        assert_eq!(b(&qs[0]), [55.0, view[1], view[2], 55.0], "the header band runs to the right edge and the top");
        assert_eq!(b(&qs[1]), [view[0], view[1], 19.0, 55.0], "and to the left edge");
        assert_eq!(b(&qs[2]), [143.0, 338.0, view[2], view[3]], "the help panel to the right and the bottom");
        assert_eq!(b(&qs[3]), [view[0], 338.0, 107.0, view[3]], "the Munny panel to the left and the bottom");
        let (dx, dy) = (view[2] - CANVAS[2], view[3] - CANVAS[3]);
        assert_eq!(b(&qs[4]), [476.0 + dx, 341.0 + dy, 523.0 + dx, 417.0 + dy], "the Mickey picture keeps its shape and sticks to the corner");
        assert_eq!(b(&qs[5]), [429.0 + dx, 341.0 + dy, 476.0 + dx, 417.0 + dy], "its other half moves with it (no tear)");
        assert_eq!(b(&qs[6]), [24.0, 59.0, 35.0, 70.0], "a corner piece stays");
        // A picture away from the canvas edges stays; one at the left edge moves to the window's left edge only.
        let mut pics = vec![quad(200.0, 100.0, 247.0, 176.0, [106.0, 140.0, 153.0, 216.0]), quad(-4.0, 200.0, 43.0, 276.0, [106.0, 140.0, 153.0, 216.0])];
        stretch_quads(&mut pics, rest, view);
        assert_eq!(b(&pics[0]), [200.0, 100.0, 247.0, 176.0], "a mid-canvas picture stays");
        assert_eq!(b(&pics[1]), [-4.0 + view[0], 200.0, 43.0 + view[0], 276.0], "a picture at the left edge moves left only");
        // A part sliding in (100 to the right of its rest place) keeps its shape: the carried edge moves with it.
        let mut moving = vec![quad(155.0, -4.0, 625.0, 55.0, band)];
        stretch_quads(&mut moving, [55.0, -4.0, 525.0, 55.0], view);
        assert_eq!(b(&moving[0])[2], view[2] + 100.0);
    }

    /// The journal's help bar (boxes from the `KH2_JOURNAL_QUADS2` dump of the layer group): the left cap, the flat body, the
    /// right cap and Jiminy's icon (which bobs to within 16 units of the canvas's left edge) stay together at wide and tall
    /// windows, while a lone picture group at an edge (the Mickey watermark) still moves.
    #[test]
    fn a_bar_of_pieces_is_not_torn_apart() {
        let pic = [1.0, 63.0, 15.0, 85.0];
        let make = || {
            vec![
                quad(47.0, 387.0, 61.0, 409.0, pic),
                quad(61.0, 387.0, 488.0, 409.0, [15.5, 63.0, 15.5, 85.0]),
                quad(488.0, 387.0, 502.0, 409.0, pic),
                quad(15.8, 388.0, 41.8, 419.0, [43.0, 63.0, 69.0, 94.0]),
            ]
        };
        let rest = [15.8, 341.0, 523.0, 419.0];
        for view in [window_view(2560.0, 1080.0), window_view(1000.0, 1000.0)] {
            let mut qs = make();
            stretch_quads(&mut qs, rest, view);
            let before = make();
            for k in 0..4 {
                assert_eq!(qs[k].pos, before[k].pos, "bar piece {k} keeps its place at {view:?}");
            }
        }
        let mut qs = vec![quad(476.0, 341.0, 523.0, 417.0, [106.0, 140.0, 153.0, 216.0])];
        stretch_quads(&mut qs, rest, window_view(1000.0, 1000.0));
        assert!(qs[0].pos[0][1] > 341.0, "the lone picture still moves to the corner");
    }
}
