//! Draws the pause menu's 2D sequences: the `.2ld` image and its sequences (`kh2_image::layd`) and
//! the menu's text, on a camera of their own above the HUD's. The draw path is the HUD's (docs/hud.md:
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
            .add_systems(Startup, setup_camera)
            .add_systems(Update, draw.in_set(crate::schedule::Frame::Popups));
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
    /// Draw nothing at all (the menu is closed).
    pub hidden: bool,
    /// Frames the players advance per 60 Hz tick (the clip's pace, docs/menus.md 1.6).
    pub speed: f32,
    acc: f32,
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
        if self.dim > 0.0 {
            let a = (self.dim.clamp(0.0, 1.0) * 128.0) as u8;
            let (x0, y0, x1, y1) = (-3000.0, -3000.0, 3500.0, 3500.0);
            out.push((
                WHITE_TEX,
                Quad { pos: [[x0, y0], [x1, y0], [x0, y1], [x1, y1]], uv: [[0.5; 2]; 4], color: [[0, 0, 0, a]; 4], blend: 0, nearest: true },
            ));
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
                    Quad { pos: [[x0, y0], [x1, y0], [x0, y1], [x1, y1]], uv: [[0.0, 0.0], [w, 0.0], [0.0, h], [w, h]], color: [[0x80; 4]; 4], blend: 0, nearest: false },
                ));
            }
            if let (Some(g), Some(s)) = (n.player.group, n.seq.and_then(|s| self.seqs.get(s))) {
                let mut ext = n.ext;
                ext.time = n.player.uv_time;
                for q in s.quads(g, n.player.frame(), n.origin, &ext) {
                    out.push((CAMP_TEX, q));
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

fn setup_camera(mut commands: Commands) {
    commands.spawn((
        MenuCamera,
        Camera2d,
        Camera { order: 11, clear_color: ClearColorConfig::None, ..default() },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin { min_width: SCREEN[0], min_height: SCREEN[1] },
            ..OrthographicProjection::default_2d()
        }),
        // Menu coordinates have y pointing down; geometry is built with y negated.
        Transform::from_xyz(SCREEN[0] / 2.0, -SCREEN[1] / 2.0, 100.0),
        RenderLayers::layer(MENU_LAYER),
    ));
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

fn draw(
    mut commands: Commands,
    mut scene: ResMut<MenuScene>,
    mut state: ResMut<MenuDraw>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<MenuMaterial>>,
    mut batches: Query<(&mut Mesh2d, &mut MeshMaterial2d<MenuMaterial>, &mut Visibility, &mut Transform)>,
) {
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
