//! Effects: PAX elements run by the `kh2-fx` particle program runtime, one tick per 60 Hz frame,
//! drawn as dynamic meshes (one per texture/blend batch).
//!
//! Effects run in the game's own world space ("fx space"): Bevy world flipped to Y-down/Z-back
//! (the game negates Y and Z when it reads bone matrices) and in game units (centimetres), so
//! effect parameters such as velocities and sprite axes keep their authored meaning. The camera
//! space is the PS2's: X right, Y down, Z forward.
//!
//! Effect textures live in GS memory: each DPD's textures are uploaded over the common effect
//! textures (`effect/texcommon.dpd`) into an emulated VRAM, and draws sample it by TEX0, so
//! textures that share VRAM resolve exactly as on the console.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use kh2_fx::pax::element_flag;
use kh2_fx::{Batch, Dpd, Element, Env, Pax};
use kh2_ps2::gs::Vram;

use crate::SCALE;
use crate::assets::Disc;
use crate::skel::Animator;

/// The game's world axes relative to Bevy's (Y and Z negated).
const FLIP: Mat4 = Mat4::from_cols(Vec4::X, Vec4::NEG_Y, Vec4::NEG_Z, Vec4::W);

/// A PAX with its DPDs shared and their GS memory images.
pub struct LoadedPax {
    pub id: usize,
    pub pax: Pax,
    pub dpds: Vec<Arc<Dpd>>,
    pub vram: Vec<Arc<Vram>>,
    /// The object file this came from (`obj/<name>.apdx`); `None` for the shared system effects.
    pub apdx: Option<String>,
}

#[derive(Resource)]
pub struct EffectLib {
    pub keyblade: Arc<LoadedPax>,
    pub system: Arc<LoadedPax>,
    /// Per-object effect files by name (`obj/<name>.apdx`).
    pub objects: HashMap<String, Arc<LoadedPax>>,
}

impl EffectLib {
    pub fn for_rig(&self, rig: &crate::skel::Rig) -> Option<&Arc<LoadedPax>> {
        self.objects.get(rig.apdx.as_ref()?)
    }
}

/// Decoded textures and materials, by (pax id, dpd, TEX0) and (texture, GS ALPHA).
#[derive(Resource, Default)]
pub struct FxCache {
    textures: HashMap<(usize, usize, u64), Handle<Image>>,
    materials: HashMap<(Option<Handle<Image>>, u8), Handle<StandardMaterial>>,
}

/// Range trigger: weapon swing, show the trail (keyblade motion set).
pub const TRIGGER_TRAIL: u8 = 12;
/// Game effect id of the weapon trail.
pub const EFFECT_TRAIL: u16 = 10;

/// What an effect is bound to.
#[derive(Clone, Copy)]
pub enum Bind {
    /// A bone of an animated model (the element's bone).
    Owner(Entity),
    /// A fixed world matrix (Bevy space), e.g. a hit point.
    Fixed(Mat4),
}

#[derive(Component)]
pub struct RunningEffect {
    pub lib: Arc<LoadedPax>,
    pub element: Element,
    pub effect: kh2_fx::Effect,
    pub dpd: usize,
    pub bind: Bind,
    pub game_id: u16,
    /// One child mesh entity per draw batch.
    parts: Vec<(Entity, Handle<Mesh>)>,
}

impl RunningEffect {
    fn owner(&self) -> Option<Entity> {
        match self.bind {
            Bind::Owner(e) => Some(e),
            Bind::Fixed(_) => None,
        }
    }
}

pub fn load(disc: &Disc, name: &str, id: usize, common: Option<&Dpd>) -> Option<LoadedPax> {
    let file = disc.read(name)?;
    let pax = match kh2_bar::parse(&file) {
        Ok(entries) => Pax::parse(entries.iter().find(|e| e.kind == 18)?.data).ok()?,
        Err(_) => Pax::parse(&file).ok()?,
    };
    let vram = pax
        .dpds
        .iter()
        .map(|d| {
            let mut v = Vram::default();
            if let Some(c) = common {
                for t in &c.textures {
                    t.upload(&mut v);
                }
            }
            for t in &d.textures {
                t.upload(&mut v);
            }
            Arc::new(v)
        })
        .collect();
    let dpds = pax.dpds.iter().cloned().map(Arc::new).collect();
    Some(LoadedPax { id, pax, dpds, vram, apdx: name.ends_with(".apdx").then(|| name.to_string()) })
}

pub struct FxPlugin;

impl Plugin for FxPlugin {
    fn build(&self, app: &mut App) {
        use crate::schedule::Sim;
        app.add_message::<StartEffect>().add_systems(Startup, setup).add_systems(
            FixedUpdate,
            (
                (weapon_trails, motion_effects, start_requests, hit_sparks, fx_test, effect_sounds).chain().in_set(Sim::Effects),
                tick.in_set(Sim::EffectTick),
            ),
        );
    }
}

pub fn setup(mut commands: Commands, disc: Res<Disc>, rigs: Res<crate::sora::Rigs>) {
    let common = disc.read("effect/texcommon.dpd").and_then(|b| Dpd::parse(&b).ok());
    let system = Arc::new(load(&disc, "00effect.bar", 1, common.as_ref()).expect("00effect.bar: no effects"));
    let mut objects = HashMap::new();
    for rig in [&rigs.sora, &rigs.keyblade, &rigs.shadow, &rigs.soldier] {
        if let Some(name) = &rig.apdx {
            let id = 2 + objects.len();
            if let Some(p) = load(&disc, name, id, common.as_ref()) {
                objects.insert(name.clone(), Arc::new(p));
            }
        }
    }
    let keyblade = objects.get("obj/W_EX010.apdx").cloned().expect("obj/W_EX010.apdx: no effects");
    commands.insert_resource(EffectLib { keyblade, system, objects });
    commands.init_resource::<FxCache>();
}

/// Request to start an object effect (element id of the owner's own effect file).
#[derive(Message, Clone, Copy, Debug)]
pub struct StartEffect {
    pub owner: Entity,
    pub id: u16,
}

pub fn start_requests(mut commands: Commands, lib: Res<EffectLib>, anims: Query<&Animator>, mut req: MessageReader<StartEffect>) {
    for r in req.read() {
        let Ok(a) = anims.get(r.owner) else { continue };
        if let Some(pax) = lib.for_rig(&a.rig) {
            start(&mut commands, pax, r.id, Bind::Owner(r.owner));
        }
    }
}

/// Motion frame trigger 1 ("effect caster"): starts the element id it names from the object's own
/// effect file, bound to that object.
pub fn motion_effects(mut commands: Commands, lib: Res<EffectLib>, anims: Query<(Entity, &Animator)>) {
    for (e, a) in &anims {
        let (Some(t), Some(m)) = (a.triggers(), a.motion()) else { continue };
        if t.frames.is_empty() {
            continue;
        }
        let Some(pax) = lib.for_rig(&a.rig) else { continue };
        let g = |f: f32| f * crate::data::LOGIC_HZ / m.fps.max(1.0);
        let (from, to) = (g(a.prev_frame), g(a.frame));
        if from == to {
            continue;
        }
        for ft in t.crossed(from, to).filter(|f| f.id == kh2_anim::triggers::frame::EFFECT_CASTER) {
            if let Some(&id) = ft.params.first() {
                start(&mut commands, pax, id as u16, Bind::Owner(e));
            }
        }
    }
}

/// Starts every element of `lib` registered for game effect `id`.
pub fn start(commands: &mut Commands, lib: &Arc<LoadedPax>, id: u16, bind: Bind) {
    for el in lib.pax.elements_for(id) {
        let Some(fx) = lib.pax.effects.get(el.effect as usize) else { continue };
        let Some(effect) = kh2_fx::Effect::new(lib.dpds[fx.dpd].clone(), fx.dpd, fx.particle_data) else { continue };
        // `KH2_FXONLY=<id,id,...>` runs only those effect IDs (isolating an effect in a screenshot).
        if let Ok(only) = std::env::var("KH2_FXONLY")
            && !only.split(',').any(|v| v.trim().parse::<u16>().ok() == Some(id))
        {
            continue;
        }
        commands.spawn((
            Transform::default(),
            Visibility::default(),
            RunningEffect { lib: lib.clone(), element: el.clone(), effect, dpd: fx.dpd, bind, game_id: id, parts: Vec::new() },
        ));
    }
}

/// Weapon trails: run effect 10 while the keyblade's motion has its trail range trigger.
pub fn weapon_trails(
    mut commands: Commands,
    lib: Res<EffectLib>,
    sora: Query<&crate::sora::Sora>,
    anims: Query<&Animator>,
    mut running: Query<&mut RunningEffect>,
) {
    let Ok(s) = sora.single() else { return };
    let Ok(weapon) = anims.get(s.weapon) else { return };
    let on = weapon.trigger(TRIGGER_TRAIL);
    let mut have = false;
    for mut r in running.iter_mut().filter(|r| r.owner() == Some(s.weapon) && r.game_id == EFFECT_TRAIL) {
        if !on {
            r.effect.end();
        } else if !r.effect.is_ending() {
            have = true;
        }
    }
    if on && !have {
        start(&mut commands, &lib.keyblade, EFFECT_TRAIL, Bind::Owner(s.weapon));
    }
}

/// Hit sparks, as the game starts them: `EffectOnHit` (plus a random offset up to `RandomEffect`)
/// picks a category bit in the attacker's weapon PAX, together with the target type (1: normal
/// target); the first element whose category has all those bits gives the effect id to start at
/// the hit point.
pub fn hit_sparks(
    mut commands: Commands,
    lib: Res<EffectLib>,
    data: Res<crate::data::GameData>,
    mut hits: MessageReader<crate::combat::Hit>,
    mut walls: MessageReader<crate::combat::WallHit>,
    mut seed: Local<u32>,
) {
    let all = hits.read().map(|h| (h.atkp, h.point)).chain(walls.read().map(|w| (w.atkp, w.point)));
    for (atkp, point) in all {
        let Some(atk) = data.atkp(atkp) else { continue };
        if atk.effect_on_hit == 0 {
            continue;
        }
        let mut bit = atk.effect_on_hit as u32 - 1;
        if atk.random_effect > 0 {
            // The game's linear congruential step (x * 0x10dcd).
            *seed = seed.wrapping_mul(0x10dcd).wrapping_add(1);
            bit += (*seed >> 16) % atk.random_effect as u32;
        }
        let mask: u64 = 1 | 1u64 << (32 + bit);
        let Some(id) = lib.keyblade.pax.elements.iter().find(|e| e.category & mask == mask).map(|e| e.id) else { continue };
        if std::env::var_os("KH2_FXLOG").is_some() {
            info!("hit atkp {} effect {} -> keyblade element id {id} at {:?}", atkp, atk.effect_on_hit, point);
        }
        start(&mut commands, &lib.keyblade, id, Bind::Fixed(Mat4::from_translation(point)));
    }
}

/// An effect element that names a sound effect (`se` >= 0) plays it when the effect starts.
pub fn effect_sounds(started: Query<&RunningEffect, Added<RunningEffect>>, mut out: MessageWriter<crate::audio::PlaySe>) {
    for r in &started {
        if r.element.se >= 0 {
            out.write(crate::audio::PlaySe::number(r.element.se as u32, r.lib.apdx.clone()));
        }
    }
}

/// One effect tick for every running effect, after the poses of this tick are known.
#[allow(clippy::too_many_arguments)]
pub fn tick(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut RunningEffect)>,
    owners: Query<&Animator>,
    locals: Query<(&Transform, Option<&ChildOf>)>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<FxCache>,
) {
    let view_inv = camera.single().map(|c| game_camera(c.to_matrix())).unwrap_or(Mat4::IDENTITY);
    for (e, mut r) in &mut effects {
        let el = r.element.clone();
        let mut env = Env { view: view_inv.inverse(), view_inv, ..Default::default() };
        match r.bind {
            Bind::Owner(owner) => {
                let Ok(anim) = owners.get(owner) else {
                    commands.entity(e).despawn();
                    continue;
                };
                let body = GlobalTransform::from(world_now(owner, &locals));
                // Bone numbers with 0x4000 set name special object matrices; the root for now.
                let bone = if el.bone & 0x4000 != 0 { body.to_matrix() } else { anim.bone_world(&body, el.bone.max(0) as usize) };
                // The element's offset and rotation are in the game's Y-down frame: the same flip that
                // takes the bone's world placement into fx space applies to its local axes (as for
                // `Bind::Fixed`). Root-bound elements put their centre at negative Y (above the feet).
                env.bind = fx_space(bone) * FLIP * element_matrix(&el);
                if el.flags & element_flag::TWO_POINT != 0 {
                    let p = |b: u16, ratio: f32| {
                        let len = anim.rig.model.bones.get(b as usize).map_or(0.0, |x| x.length);
                        fx_space(anim.bone_world(&body, b as usize)).transform_point3(Vec3::new(ratio * len, 0.0, 0.0))
                    };
                    env.two_point = Some([p(el.bone_pos.a, el.bone_pos.ratio_a), p(el.bone_pos.b, el.bone_pos.ratio_b)]);
                }
            }
            Bind::Fixed(m) => {
                // A placement in metres: rotation and scale stay, the translation becomes centimetres.
                let (sc, r, t) = m.to_scale_rotation_translation();
                env.bind = FLIP * Mat4::from_scale_rotation_translation(sc, r, t / SCALE) * FLIP * element_matrix(&el);
            }
        }
        env.bind_scale = Vec3::new(env.bind.x_axis.truncate().length(), env.bind.y_axis.truncate().length(), env.bind.z_axis.truncate().length());
        let draws = r.effect.tick(&env);
        if r.effect.done {
            commands.entity(e).despawn();
            continue;
        }
        if std::env::var_os("KH2_FXLOG").is_some() {
            let verts: usize = draws.iter().map(|b| b.verts.len()).sum();
            info!("fx {} objs {} batches {} verts {verts}", r.game_id, r.effect.alive_objects(), draws.len());
            for b in &draws {
                if let Some(v) = b.verts.first() {
                    info!(
                        "   batch tex0 {:?} alpha {:#x} n {} v0 pos {:?} col {:?} uv {:?} bind {:?}",
                        b.tex0.map(|t| format!("{t:#x}")),
                        b.alpha,
                        b.verts.len(),
                        FLIP.transform_point3(v.pos) * SCALE,
                        v.color,
                        v.uv,
                        FLIP.transform_point3(env.bind.w_axis.truncate()) * SCALE
                    );
                }
            }
        }
        // One child mesh per batch; spare children are hidden (empty meshes upset the renderer).
        while r.parts.len() < draws.len() {
            let mesh = meshes.add(build_mesh(&draws[r.parts.len()]));
            let child = commands
                .spawn((Mesh3d(mesh.clone()), Transform::default(), Visibility::Hidden, bevy::light::NotShadowCaster, ChildOf(e)))
                .id();
            r.parts.push((child, mesh));
        }
        for (i, (child, mesh)) in r.parts.clone().into_iter().enumerate() {
            match draws.get(i).filter(|b| !b.indices.is_empty()) {
                Some(b) => {
                    if let Some(mut m) = meshes.get_mut(&mesh) {
                        *m = build_mesh(b);
                    }
                    let tex = b.tex0.map(|t| texture(&mut cache, &mut images, &r.lib, b.dpd, t));
                    let mat = material(&mut cache, &mut materials, tex, b.alpha);
                    commands.entity(child).insert((MeshMaterial3d(mat), Visibility::Inherited));
                }
                None => {
                    commands.entity(child).insert(Visibility::Hidden);
                }
            }
        }
    }
}

fn texture(cache: &mut FxCache, images: &mut Assets<Image>, lib: &LoadedPax, dpd: usize, tex0: u64) -> Handle<Image> {
    cache
        .textures
        .entry((lib.id, dpd, tex0))
        .or_insert_with(|| {
            let (w, h, rgba) = lib.vram[dpd].texture8(tex0);
            let mut img = Image::new(
                Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
                TextureDimension::D2,
                rgba,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
            images.add(img)
        })
        .clone()
}

/// Unlit material for a GS ALPHA blend: (A − B)·C + D with A/B/D picking Cs, Cd or 0.
fn material(cache: &mut FxCache, materials: &mut Assets<StandardMaterial>, tex: Option<Handle<Image>>, alpha: u8) -> Handle<StandardMaterial> {
    cache
        .materials
        .entry((tex.clone(), alpha))
        .or_insert_with(|| {
            let (a, b, d) = (alpha & 3, (alpha >> 2) & 3, (alpha >> 6) & 3);
            let mode = match (a, b, d) {
                (0, 2, 1) => AlphaMode::Add,
                (0, 1, 1) => AlphaMode::Blend,
                (2, 0, 1) => AlphaMode::Multiply,
                (0, 2, 2) => AlphaMode::Blend,
                _ => AlphaMode::Blend,
            };
            materials.add(StandardMaterial {
                base_color_texture: tex,
                unlit: true,
                cull_mode: None,
                double_sided: true,
                alpha_mode: mode,
                ..default()
            })
        })
        .clone()
}

/// The element's own placement on its bone (translation, rotation, scale).
fn element_matrix(el: &Element) -> Mat4 {
    let s = Vec3::from(el.scale);
    let s = if s == Vec3::ZERO { Vec3::ONE } else { s };
    let r = Quat::from_euler(EulerRot::ZYX, el.rotation[2], el.rotation[1], el.rotation[0]);
    Mat4::from_scale_rotation_translation(s, r, Vec3::from(el.translation))
}

/// World matrix from this tick's local transforms (GlobalTransform is only propagated after
/// the fixed-rate systems, one frame late for anything that moved this tick).
pub fn world_now(e: Entity, locals: &Query<(&Transform, Option<&ChildOf>)>) -> Mat4 {
    let mut m = Mat4::IDENTITY;
    let mut cur = Some(e);
    while let Some(x) = cur {
        let Ok((t, parent)) = locals.get(x) else { break };
        m = t.to_matrix() * m;
        cur = parent.map(|p| p.parent());
    }
    m
}

/// Bevy world matrix (metres) → fx space (game world, centimetres).
fn fx_space(m: Mat4) -> Mat4 {
    FLIP * Mat4::from_scale(Vec3::splat(1.0 / SCALE)) * m
}

/// Game camera (X right, Y down, Z forward, centimetres) → fx world, from the Bevy camera.
fn game_camera(cam: Mat4) -> Mat4 {
    let (_, r, t) = cam.to_scale_rotation_translation();
    FLIP * Mat4::from_rotation_translation(r, t / SCALE) * FLIP
}

fn build_mesh(b: &Batch) -> Mesh {
    let back = |p: Vec3| (FLIP.transform_point3(p) * SCALE).to_array();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, b.verts.iter().map(|v| back(v.pos)).collect::<Vec<_>>())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, b.verts.iter().map(|v| v.uv).collect::<Vec<_>>())
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_COLOR,
            VertexAttributeValues::Float32x4(b.verts.iter().map(|v| v.color.to_array()).collect()),
        )
        .with_inserted_indices(Indices::U32(b.indices.clone()))
}

/// `KH2_FXTEST=<id>`: starts system effect `id` 1.2 m in front of Sora at chest height once a second
/// (visual checks without needing a fight).
pub fn fx_test(mut commands: Commands, lib: Res<EffectLib>, sora: Query<&Transform, With<crate::sora::Sora>>, mut ticks: Local<u32>) {
    let Some(spec) = std::env::var("KH2_FXTEST").ok() else { return };
    // "<name>:<id>" (e.g. "M_EX020:4"), "k<id>" for the keyblade, else a system effect id.
    let (pax, id) = if let Some((name, n)) = spec.split_once(':') {
        let Some(p) = lib.objects.get(&format!("obj/{name}.apdx")) else { return };
        (p, n.parse::<u16>().unwrap_or(0))
    } else {
        match spec.strip_prefix('k') {
            Some(n) => (&lib.keyblade, n.parse::<u16>().unwrap_or(0)),
            None => (&lib.system, spec.parse::<u16>().unwrap_or(0)),
        }
    };
    *ticks += 1;
    if *ticks % 60 != 1 {
        return;
    }
    let Ok(t) = sora.single() else { return };
    let off = std::env::var("KH2_FXTEST_AT")
        .ok()
        .map(|v| v.split(',').filter_map(|x| x.parse::<f32>().ok()).collect::<Vec<_>>())
        .filter(|v| v.len() == 3)
        .map_or(t.rotation * Vec3::Z * 1.2 + Vec3::Y, |v| Vec3::new(v[0], v[1], v[2]));
    let at = t.translation + off;
    start(&mut commands, pax, id, Bind::Fixed(Mat4::from_translation(at)));
}
