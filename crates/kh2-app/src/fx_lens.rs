//! The lens pass (WP-I3): `pppSnoMdlDttNorm` batches whose texture is the display buffer.
//!
//! The PS2 draws the field into a draw buffer (GS page 0xd8, 512 x 416) and shows a separate display buffer
//! (page 0, 512 x 448; `FUN_0014dcd0` sets both up and the copy packet `FUN_0014dfe8` that moves each finished
//! frame across). A lens entry with byte +0x14 set takes the display buffer as its texture (handler 0x20a7e8:
//! `DAT_00360b04`, `DAT_00360b08`, `DAT_00360b1c`), so while the scene is being drawn it shows the picture of the
//! previous frame, HUD included, shifted along the model's normals and tinted, blended over the new scene
//! (GS ALPHA 0x44: `Cs * As + Cd * (1 - As)`). Frame after frame the ghost of what was on screen is carried
//! forward and shifted again: the smear of the Seifer intro's cut (docs/systems/player-camera/camera-event.md "The intro camera").
//!
//! The app does the same with two passes:
//! - after the HUD camera has drawn (the last thing in the picture), its main texture is copied into a history
//!   texture (`lens_history`, Core2d post-process, cameras with `LensHistory`);
//! - in the 3D camera's post-process (`lens_scene`, before the HUD draws, cameras with `LensScene`) the scene is
//!   copied to the other main texture and the lens triangles are drawn over it, sampling the history texture of
//!   the previous program frame at the shifted screen position (blend `SrcAlpha`, `OneMinusSrcAlpha`).
//!
//! The history the lens reads is the picture of the previous effect frame: the textures swap when the effects
//! run a new program frame (`LensDraws::frame`, 30 a second) or while no lens is drawn, so at 60 rendered frames a
//! second both frames of a field frame show the same ghost, as on the console. The screen position of a vertex
//! and of its shifted point are projected per vertex and the texture position is interpolated linearly across
//! the triangle, as the GS does with the UV the model draw (0x20ab40) writes per vertex. [inferred] The display
//! buffer's 448 lines and the draw buffer's 416 share their top line (the copy offset is not modelled); the GS
//! alpha factor above 1.0 (As > 0x80) is clamped to 1.0 by the blend.

use bevy::asset::embedded_asset;
use bevy::core_pipeline::{Core2d, Core2dSystems, Core3d, Core3dSystems, tonemapping::tonemapping};
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_resource::binding_types::{sampler, texture_2d};
use bevy::render::render_resource::{
    AddressMode, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, BlendComponent, BlendFactor, BlendOperation, BlendState,
    Buffer, BufferInitDescriptor, BufferUsages, CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode, FragmentState, LoadOp,
    Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType,
    SamplerDescriptor, ShaderStages, StoreOp, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType, TextureUsages,
    TextureView, TextureViewDescriptor, VertexFormat, VertexState, VertexStepMode,
};
use bevy::mesh::VertexBufferLayout;
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::Shader;
use std::collections::HashMap;

use crate::SCALE;

/// The game's world axes relative to Bevy's (Y and Z negated); the same flip as `fx::FLIP`.
const FLIP: Mat4 = Mat4::from_cols(Vec4::X, Vec4::NEG_Y, Vec4::NEG_Z, Vec4::W);

/// One lens draw: a triangle list (three vertices a triangle), in Bevy view space (metres) for an element in
/// camera space, else in Bevy world space.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LensBatch {
    pub camera_space: bool,
    /// Where each vertex is drawn.
    pub pos: Vec<Vec3>,
    /// The point whose screen position is the texture position (the vertex moved by its displacement).
    pub shifted: Vec<Vec3>,
    /// The tint (the vertex colour, gamma as `fx::gs_color`) and the blend weight in alpha.
    pub color: Vec<[f32; 4]>,
}

impl LensBatch {
    /// From a distortion batch of the effect runtime (fx space, game centimetres); `view` is the fx world to game
    /// camera matrix the effect ran with.
    pub fn of(b: &kh2_fx::Batch, view: &Mat4, camera_space: bool) -> LensBatch {
        let place = |p: Vec3| if camera_space { FLIP.transform_point3(view.transform_point3(p)) * SCALE } else { FLIP.transform_point3(p) * SCALE };
        let mut out = LensBatch { camera_space, ..Default::default() };
        for &i in &b.indices {
            let (Some(v), Some(o)) = (b.verts.get(i as usize), b.offsets.get(i as usize)) else { continue };
            out.pos.push(place(v.pos));
            out.shifted.push(place(v.pos + *o));
            let g = |c: f32| c.max(0.0).powf(2.2);
            out.color.push([g(v.color.x), g(v.color.y), g(v.color.z), v.color.w]);
        }
        out
    }
}

/// The lens batches of the running effects (written by `fx::tick`) and the effect frame they belong to.
#[derive(Resource, ExtractResource, Clone, Default)]
pub struct LensDraws {
    /// Counts program frames that drew a lens.
    pub frame: u64,
    pub batches: Vec<LensBatch>,
}

/// The 3D camera whose picture the lens is drawn over.
#[derive(Component, ExtractComponent, Clone, Copy, Default)]
pub struct LensScene;

/// A 2D camera drawn over the 3D picture (the HUD's, the menu's): the picture after the last of them is the
/// display buffer of the next frame (each copies in its turn, so the last one's copy stays).
#[derive(Component, ExtractComponent, Clone, Copy, Default)]
pub struct LensHistory;

pub struct LensPlugin;

impl Plugin for LensPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "fx_lens.wgsl");
        app.init_resource::<LensDraws>()
            .add_plugins((ExtractResourcePlugin::<LensDraws>::default(), ExtractComponentPlugin::<LensScene>::default(), ExtractComponentPlugin::<LensHistory>::default()))
            .add_systems(Update, mark_history_camera);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_systems(RenderStartup, init)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
            .add_systems(Core3d, lens_scene.in_set(Core3dSystems::PostProcess).before(tonemapping))
            .add_systems(Core2d, lens_history.in_set(Core2dSystems::PostProcess));
    }
}

fn mark_history_camera(mut commands: Commands, hud: Query<Entity, (With<Camera2d>, Without<LensHistory>)>) {
    for e in &hud {
        commands.entity(e).insert(LensHistory);
    }
}

#[derive(Resource)]
struct LensPipeline {
    layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    shader: Handle<Shader>,
    /// (copy, lens) by target format.
    ids: HashMap<TextureFormat, (CachedRenderPipelineId, CachedRenderPipelineId)>,
}

/// The two history textures, the one the lens reads this frame, and this frame's vertices.
#[derive(Resource, Default)]
struct LensGpu {
    history: Option<[(Texture, TextureView); 2]>,
    format: Option<TextureFormat>,
    /// Index of the texture holding the previous effect frame's picture (the lens reads it, the copy writes
    /// the other).
    prev: usize,
    seen: u64,
    vertices: Option<(Buffer, u32)>,
}

fn init(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>) {
    let layout = BindGroupLayoutDescriptor::new(
        "kh2_lens_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (texture_2d(TextureSampleType::Float { filterable: true }), sampler(SamplerBindingType::Filtering))),
    );
    // The GS CLAMP register of the draw is 8 (region clamp): outside the buffer the edge repeats [inferred].
    let sampler = device.create_sampler(&SamplerDescriptor {
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    let shader = assets.load(concat!("embedded://", env!("CARGO_CRATE_NAME"), "/fx_lens.wgsl"));
    commands.insert_resource(LensPipeline { layout, sampler, shader, ids: HashMap::new() });
    commands.insert_resource(LensGpu::default());
}

impl LensPipeline {
    fn ids(&mut self, cache: &PipelineCache, format: TextureFormat) -> (CachedRenderPipelineId, CachedRenderPipelineId) {
        let (layout, shader) = (self.layout.clone(), self.shader.clone());
        *self.ids.entry(format).or_insert_with(|| {
            let desc = |label: &'static str, vs: &'static str, fs: &'static str, buffers: Vec<VertexBufferLayout>, blend: Option<BlendState>| RenderPipelineDescriptor {
                label: Some(label.into()),
                layout: vec![layout.clone()],
                vertex: VertexState { shader: shader.clone(), entry_point: Some(vs.into()), buffers, ..default() },
                fragment: Some(FragmentState {
                    shader: shader.clone(),
                    entry_point: Some(fs.into()),
                    targets: vec![Some(ColorTargetState { format, blend, write_mask: ColorWrites::ALL })],
                    ..default()
                }),
                ..default()
            };
            let copy = cache.queue_render_pipeline(desc("kh2_lens_copy", "copy_vertex", "copy_fragment", Vec::new(), None));
            let vertex = VertexBufferLayout::from_vertex_formats(VertexStepMode::Vertex, [VertexFormat::Float32x4, VertexFormat::Float32x2, VertexFormat::Float32x4]);
            let blend = BlendState {
                color: BlendComponent { src_factor: BlendFactor::SrcAlpha, dst_factor: BlendFactor::OneMinusSrcAlpha, operation: BlendOperation::Add },
                alpha: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add },
            };
            let lens = cache.queue_render_pipeline(desc("kh2_lens_draw", "lens_vertex", "lens_fragment", vec![vertex], Some(blend)));
            (copy, lens)
        })
    }
}

/// The display buffer's size (GS page 0, 512 x 448; `FUN_0014dcd0`). The history texture has this size, not the
/// window's: every pass of the feedback (copy down, bilinear sample, copy down again) blurs by a texel of the
/// console's buffer, which is what makes the original's ghost a smooth stretched blur rather than separate copies
/// (docs/systems/player-camera/camera-event.md "The intro camera", WP-FX13).
const HISTORY_SIZE: Extent3d = Extent3d { width: 512, height: 448, depth_or_array_layers: 1 };

/// Per vertex: clip position (4 floats), texture position (2), tint and weight (4).
const VERTEX_FLOATS: usize = 10;

/// The vertices of the lens batches for the 3D camera `view`: each vertex's clip position, and the screen
/// position of its shifted point as the texture position (0..1, y down).
fn vertices(draws: &LensDraws, view: &ExtractedView) -> Vec<f32> {
    let view_from_world = view.world_from_view.to_matrix().inverse();
    let mut out = Vec::new();
    for b in &draws.batches {
        let to_view = |p: Vec3| if b.camera_space { p } else { view_from_world.transform_point3(p) };
        for ((p, s), c) in b.pos.iter().zip(&b.shifted).zip(&b.color) {
            let clip = view.clip_from_view * to_view(*p).extend(1.0);
            let sc = view.clip_from_view * to_view(*s).extend(1.0);
            // A shifted point behind the camera has no screen position: the vertex's own is taken.
            let ndc = if sc.w > 1e-6 { sc.truncate().truncate() / sc.w } else if clip.w > 1e-6 { clip.truncate().truncate() / clip.w } else { Vec2::ZERO };
            out.extend_from_slice(&[clip.x, clip.y, 0.0, clip.w, 0.5 + 0.5 * ndc.x, 0.5 - 0.5 * ndc.y, c[0], c[1], c[2], c[3]]);
        }
    }
    out
}

#[allow(clippy::type_complexity)]
fn prepare(
    device: Res<RenderDevice>,
    cache: Res<PipelineCache>,
    draws: Option<Res<LensDraws>>,
    mut pipeline: ResMut<LensPipeline>,
    mut gpu: ResMut<LensGpu>,
    views: Query<(&ViewTarget, &ExtractedView, Has<LensScene>, Has<LensHistory>)>,
) {
    let empty = LensDraws::default();
    let draws = draws.as_deref().unwrap_or(&empty);
    // A new effect frame (or no lens at all): the picture copied last becomes the one the lens reads.
    if draws.batches.is_empty() || draws.frame != gpu.seen {
        gpu.prev ^= 1;
        gpu.seen = draws.frame;
    }
    gpu.vertices = None;
    for (target, view, scene, history) in &views {
        if !scene && !history {
            continue;
        }
        let format = target.main_texture_format();
        pipeline.ids(&cache, format);
        if history {
            let size = HISTORY_SIZE;
            if gpu.history.is_none() || gpu.format != Some(format) {
                let make = || {
                    let t = device.create_texture(&TextureDescriptor {
                        label: Some("kh2_lens_history"),
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format,
                        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    let v = t.create_view(&TextureViewDescriptor::default());
                    (t, v)
                };
                gpu.history = Some([make(), make()]);
                gpu.format = Some(format);
            }
        }
        if scene && !draws.batches.is_empty() {
            let data = vertices(draws, view);
            if !data.is_empty() {
                let bytes: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();
                let buffer = device.create_buffer_with_data(&BufferInitDescriptor { label: Some("kh2_lens_vertices"), contents: &bytes, usage: BufferUsages::VERTEX });
                gpu.vertices = Some((buffer, (data.len() / VERTEX_FLOATS) as u32));
            }
        }
    }
}

/// Core3d post-process of the 3D camera: the scene, then the lens over it (reading last frame's picture).
fn lens_scene(view: ViewQuery<(&ViewTarget, &LensScene)>, gpu: Res<LensGpu>, pipeline: Res<LensPipeline>, cache: Res<PipelineCache>, mut ctx: RenderContext) {
    let (target, _) = view.into_inner();
    let (Some((buffer, count)), Some(history)) = (&gpu.vertices, &gpu.history) else { return };
    if gpu.format != Some(target.main_texture_format()) {
        return;
    }
    let Some(&(copy_id, lens_id)) = pipeline.ids.get(&target.main_texture_format()) else { return };
    let (Some(copy), Some(lens)) = (cache.get_render_pipeline(copy_id), cache.get_render_pipeline(lens_id)) else { return };
    let layout = cache.get_bind_group_layout(&pipeline.layout);
    let post = target.post_process_write();
    let scene_group = ctx.render_device().create_bind_group("kh2_lens_scene", &layout, &BindGroupEntries::sequential((post.source, &pipeline.sampler)));
    let history_group = ctx.render_device().create_bind_group("kh2_lens_history", &layout, &BindGroupEntries::sequential((&history[gpu.prev].1, &pipeline.sampler)));
    let pass = RenderPassDescriptor {
        label: Some("kh2_lens"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations { load: LoadOp::Clear(Default::default()), store: StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    };
    let mut pass = ctx.command_encoder().begin_render_pass(&pass);
    pass.set_pipeline(copy);
    pass.set_bind_group(0, &scene_group, &[]);
    pass.draw(0..3, 0..1);
    pass.set_pipeline(lens);
    pass.set_bind_group(0, &history_group, &[]);
    pass.set_vertex_buffer(0, (**buffer).slice(..));
    pass.draw(0..*count, 0..1);
}

/// Core2d post-process of the 2D cameras (the HUD's last): the finished picture becomes the next frame's
/// display buffer.
fn lens_history(view: ViewQuery<(&ViewTarget, &LensHistory)>, gpu: Res<LensGpu>, pipeline: Res<LensPipeline>, cache: Res<PipelineCache>, mut ctx: RenderContext) {
    let (target, _) = view.into_inner();
    let Some(history) = &gpu.history else { return };
    if gpu.format != Some(target.main_texture_format()) {
        return;
    }
    let Some(&(copy_id, _)) = pipeline.ids.get(&target.main_texture_format()) else { return };
    let Some(copy) = cache.get_render_pipeline(copy_id) else { return };
    let layout = cache.get_bind_group_layout(&pipeline.layout);
    let group = ctx.render_device().create_bind_group("kh2_lens_copy", &layout, &BindGroupEntries::sequential((target.main_texture_view(), &pipeline.sampler)));
    let pass = RenderPassDescriptor {
        label: Some("kh2_lens_history"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &history[gpu.prev ^ 1].1,
            depth_slice: None,
            resolve_target: None,
            ops: Operations { load: LoadOp::Clear(Default::default()), store: StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    };
    let mut pass = ctx.command_encoder().begin_render_pass(&pass);
    pass.set_pipeline(copy);
    pass.set_bind_group(0, &group, &[]);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The history is the console's display buffer, not the window.
    #[test]
    fn the_history_is_the_display_buffer_size() {
        assert_eq!((HISTORY_SIZE.width, HISTORY_SIZE.height), (512, 448));
    }

    /// A vertex in front of the camera lands at its projected screen position; its texture position is the
    /// screen position of the shifted point (y down), so a shift to the right moves the sample right.
    #[test]
    fn lens_vertices_take_the_shifted_point_as_texture_position() {
        let proj = Mat4::perspective_infinite_reverse_rh(1.0, 1.0, 0.01);
        let view = ExtractedView {
            retained_view_entity: bevy::render::view::RetainedViewEntity::new(bevy::render::sync_world::MainEntity::from(Entity::PLACEHOLDER), None, 0),
            clip_from_view: proj,
            world_from_view: GlobalTransform::IDENTITY,
            clip_from_world: None,
            target_format: TextureFormat::Rgba8UnormSrgb,
            viewport: UVec4::new(0, 0, 100, 100),
            color_grading: default(),
            invert_culling: false,
        };
        let b = LensBatch { camera_space: true, pos: vec![Vec3::new(0.0, 0.0, -1.0)], shifted: vec![Vec3::new(0.5, 0.0, -1.0)], color: vec![[1.0, 1.0, 1.0, 0.5]] };
        let v = vertices(&LensDraws { frame: 1, batches: vec![b] }, &view);
        assert_eq!(v.len(), VERTEX_FLOATS);
        assert!((v[0] / v[3]).abs() < 1e-6 && (v[1] / v[3]).abs() < 1e-6, "the vertex is drawn at the centre");
        assert!(v[4] > 0.5 && (v[5] - 0.5).abs() < 1e-6, "the sample moves right with the shift: {v:?}");
        assert_eq!(v[9], 0.5);
    }

    /// A camera-space distortion batch keeps its geometry relative to the game camera: a vertex 5 cm in front of
    /// the game camera (z forward) is 0.05 m in front of the Bevy view (-z).
    #[test]
    fn a_camera_space_lens_sits_in_front_of_the_view() {
        let mut b = kh2_fx::Batch::default();
        b.verts.push(kh2_fx::Vertex { pos: Vec3::new(0.0, 0.0, 5.0), color: Vec4::ONE, uv: [0.0; 2] });
        b.offsets.push(Vec3::new(1.0, 0.0, 0.0));
        b.indices.push(0);
        let l = LensBatch::of(&b, &Mat4::IDENTITY, true);
        assert!((l.pos[0] - Vec3::new(0.0, 0.0, -0.05)).length() < 1e-6, "{:?}", l.pos);
        assert!((l.shifted[0] - Vec3::new(0.01, 0.0, -0.05)).length() < 1e-6, "{:?}", l.shifted);
    }
}
