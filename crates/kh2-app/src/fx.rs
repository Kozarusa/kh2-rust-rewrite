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

use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{Material, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use kh2_fx::pax::element_flag;
use kh2_fx::{Batch, Dpd, Element, Env, Pax};
use kh2_ps2::gs::Vram;

use crate::SCALE;
use crate::assets::Disc;
use crate::combat::{Fighter, Team};
use crate::enemy::Enemy;
use crate::skel::Animator;
use crate::sora::Sora;

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
/// Effect started in place of the hit spark when a hit is ineffective (FUN_0018d090 starts element
/// id 0x12 of the system file `00effect.bar` at the hit point).
pub const EFFECT_INEFFECTIVE: u16 = 0x12;
/// Element id of the Heartless death effect in the dying object's own effect file (the `dead`
/// mode of the Shadow and Soldier scripts runs `spawn_attached(obj, 2, 1, 0)`).
pub const EFFECT_DEATH: u16 = 2;
/// Low category bits of an effect element that the victim-side `DamageEffect` lookup requires.
const CATEGORY_DAMAGE: u64 = 0x200;

/// Effects the game's damage and death code starts, as requests (see `docs/effects-damage.md` for
/// the hook points in `combat.rs` and `enemy.rs`). `fx.rs` itself turns every `combat::Hit` into
/// `Damage` or `Ineffective`, and starts `Death` for an enemy whose HP reached 0 until the enemy
/// code sends it; a second `Death` for the same victim is ignored.
#[derive(Message, Clone, Copy, Debug)]
pub enum GameEffect {
    /// Victim-side `DamageEffect` of a landed hit: the first element of the victim's own effect file
    /// whose category has bit `32 + effect - 1` and the 0x200 flag, started at the hit point.
    Damage { victim: Entity, effect: u8, point: Vec3 },
    /// The hit did no damage because the victim's multipliers for the element and in general
    /// multiply to 0: system effect 0x12 at the hit point, turned to face along the hit.
    Ineffective { point: Vec3, dir: Vec3 },
    /// The victim's HP reached 0: element 2 of its own effect file, bound to it.
    Death { victim: Entity },
}

/// Marks a victim whose death effect has started.
#[derive(Component)]
pub struct DeathFx;

/// Screen-distortion material for `pppSnoMdlDttNorm` batches: the triangles show the scene drawn
/// so far (the transmission copy of the main pass) at the screen position of the vertex shifted by
/// its displacement, tinted by the vertex color. The displacement rides in the UV attributes.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct DistortMaterial {
    /// Unused; a material needs at least one binding.
    #[uniform(0)]
    pub unused: Vec4,
}

impl Material for DistortMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path("embedded://kh2_game/fx_distort.wgsl".into())
    }
    // The transmission copy of the scene is only readable from the transmissive phase, which takes
    // opaque materials: the shader blends by itself.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }
    fn reads_view_transmission_texture(&self) -> bool {
        true
    }
    fn specialize(
        _: &bevy::pbr::MaterialPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _: &bevy::mesh::MeshVertexBufferLayoutRef,
        _: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        // Like the other effect materials: both sides of every triangle are drawn (the console
        // does not cull), and the lens does not hide effects drawn after it.
        descriptor.primitive.cull_mode = None;
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = Some(false);
        }
        Ok(())
    }
}

#[derive(Resource)]
pub struct DistortHandle(Handle<DistortMaterial>);

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
        embedded_asset!(app, "fx_distort.wgsl");
        app.add_plugins(MaterialPlugin::<DistortMaterial>::default())
            .add_message::<StartEffect>()
            .add_message::<GameEffect>()
            .add_systems(Startup, setup)
            .add_systems(
                FixedUpdate,
                (
                    (
                        weapon_trails,
                        motion_effects,
                        start_requests,
                        hit_sparks,
                        fx_kill,
                        enemy_deaths,
                        game_effects,
                        fx_test,
                        effect_sounds,
                    )
                        .chain()
                        .in_set(Sim::Effects),
                    tick.in_set(Sim::EffectTick),
                ),
            );
    }
}

pub fn setup(mut commands: Commands, disc: Res<Disc>, rigs: Res<crate::sora::Rigs>, mut distort: ResMut<Assets<DistortMaterial>>) {
    commands.insert_resource(DistortHandle(distort.add(DistortMaterial { unused: Vec4::ZERO })));
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
    start_placed(commands, lib, id, bind, Placement::default());
}

/// What the game overwrites in a freshly created effect before it first runs: for a hit effect the
/// hit point replaces the element's translation, and the hit spark and effect 0x12 also get a
/// rotation made from the hit direction (FUN_001a22a0, FUN_0018d090, FUN_0017be00).
#[derive(Clone, Copy, Default)]
pub struct Placement {
    pub replace_translation: bool,
    pub rotation: Option<[f32; 3]>,
}

impl Placement {
    /// A placement at a hit point (with `Bind::Fixed`), optionally turned along the hit.
    pub fn hit(dir: Option<Vec3>) -> Placement {
        Placement { replace_translation: true, rotation: dir.map(hit_rotation) }
    }
}

/// The game's angles (pitch about X, yaw about Y, 0) for a hit direction given in Bevy world space
/// (FUN_0017be00: pitch = atan2(-y, hypot(x, z)), yaw = atan2(x, z), in the game's own axes, which
/// are Bevy's with Y and Z negated).
pub fn hit_rotation(dir: Vec3) -> [f32; 3] {
    let g = Vec3::new(dir.x, -dir.y, -dir.z);
    [(-g.y).atan2((g.x * g.x + g.z * g.z).sqrt()), g.x.atan2(g.z), 0.0]
}

pub fn start_placed(commands: &mut Commands, lib: &Arc<LoadedPax>, id: u16, bind: Bind, place: Placement) {
    for el in lib.pax.elements_for(id) {
        let Some(fx) = lib.pax.effects.get(el.effect as usize) else { continue };
        let Some(effect) = kh2_fx::Effect::new(lib.dpds[fx.dpd].clone(), fx.dpd, fx.particle_data) else { continue };
        // `KH2_FXONLY=<id,id,...>` runs only those effect IDs (isolating an effect in a screenshot).
        if let Ok(only) = std::env::var("KH2_FXONLY")
            && !only.split(',').any(|v| v.trim().parse::<u16>().ok() == Some(id))
        {
            continue;
        }
        let mut element = el.clone();
        if place.replace_translation {
            element.translation = [0.0; 3];
        }
        if let Some(r) = place.rotation {
            element.rotation = r;
        }
        commands.spawn((
            Transform::default(),
            Visibility::default(),
            RunningEffect { lib: lib.clone(), element, effect, dpd: fx.dpd, bind, game_id: id, parts: Vec::new() },
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

/// Whether a hit does nothing at all: the victim's multiplier for the attack's element times its
/// general multiplier is 0 (FUN_0018b618 sets hit-record flag 4; FUN_0018d090 then starts effect
/// 0x12 instead of the spark and the damage effect). `KH2_FXINEFFECTIVE=1` makes every hit count
/// as ineffective, to look at effect 0x12.
fn ineffective(atk: &kh2_tables::tables::Atkp, victim: Option<crate::data::Stats>) -> bool {
    std::env::var_os("KH2_FXINEFFECTIVE").is_some()
        || victim.is_some_and(|s| s.weakness[(atk.element as usize).min(6)] * s.weakness[6] == 0)
}

/// The first element of `pax` whose category has all of `mask` (FUN_001db9e0: the lookup behind
/// every category-selected effect).
fn element_by_category(pax: &LoadedPax, mask: u64) -> Option<u16> {
    pax.pax.elements.iter().find(|e| e.category & mask == mask).map(|e| e.id)
}

/// Hit effects, as FUN_0018d090 starts them for every landed hit. An ineffective hit sends
/// `GameEffect::Ineffective` and nothing else. Otherwise `EffectOnHit` (plus a random offset up to
/// `RandomEffect`) picks a category bit, together with the target type 1 (a normal target), in the
/// attacker's own effect file (Sora's weapon for his attacks, the enemy's file for its claws); the
/// first element whose category has all those bits is started at the hit point, turned along the
/// hit. `DamageEffect` then becomes a `GameEffect::Damage` for the victim. Wall hits only spawn the
/// spark.
#[allow(clippy::too_many_arguments)]
pub fn hit_sparks(
    mut commands: Commands,
    lib: Res<EffectLib>,
    data: Res<crate::data::GameData>,
    fighters: Query<&Fighter>,
    anims: Query<&Animator>,
    enemies: Query<&Enemy>,
    sora: Query<&Sora>,
    mut hits: MessageReader<crate::combat::Hit>,
    mut walls: MessageReader<crate::combat::WallHit>,
    mut game: MessageWriter<GameEffect>,
    mut seed: Local<u32>,
) {
    // (attacker's effect file, atkp row, point, direction, the hit if it is one)
    let mut all = Vec::new();
    for h in hits.read() {
        let Some(atk) = data.atkp(h.atkp) else { continue };
        let victim = enemies.get(h.target).map(|e| e.stats).ok().or_else(|| sora.get(h.target).map(|s| s.stats).ok());
        if ineffective(atk, victim) {
            game.write(GameEffect::Ineffective { point: h.point, dir: h.dir });
            continue;
        }
        if atk.damage_effect != 0 {
            game.write(GameEffect::Damage { victim: h.target, effect: atk.damage_effect, point: h.point });
        }
        let pax = fighters.get(h.attacker).ok().and_then(|f| match f.team {
            Team::Player => Some(lib.keyblade.clone()),
            Team::Enemy => f.attack_bodies.first().and_then(|b| anims.get(*b).ok()).and_then(|a| lib.for_rig(&a.rig)).cloned(),
        });
        if let Some(pax) = pax {
            all.push((pax, atk, h.point, Some(h.dir)));
        }
    }
    for w in walls.read() {
        if let Some(atk) = data.atkp(w.atkp) {
            all.push((lib.keyblade.clone(), atk, w.point, None));
        }
    }
    for (pax, atk, point, dir) in all {
        if atk.effect_on_hit == 0 {
            continue;
        }
        let mut bit = atk.effect_on_hit as u32 - 1;
        if atk.random_effect > 0 {
            // The game's linear congruential step (x * 0x10dcd).
            *seed = seed.wrapping_mul(0x10dcd).wrapping_add(1);
            bit += (*seed >> 16) % atk.random_effect as u32;
        }
        let Some(id) = element_by_category(&pax, 1 | 1u64 << (32 + bit)) else { continue };
        if std::env::var_os("KH2_FXLOG").is_some() {
            info!("hit atkp {} effect {} -> element id {id} of {:?} at {:?}", atk.id, atk.effect_on_hit, pax.apdx, point);
        }
        let place = if dir.is_some() { Placement::hit(dir) } else { Placement::default() };
        start_placed(&mut commands, &pax, id, Bind::Fixed(Mat4::from_translation(point)), place);
    }
}

/// Starts the effects the game's damage and death code asks for.
#[allow(clippy::too_many_arguments)]
pub fn game_effects(
    mut commands: Commands,
    lib: Res<EffectLib>,
    mut req: MessageReader<GameEffect>,
    fighters: Query<&Fighter>,
    anims: Query<&Animator>,
    mut dead: Local<std::collections::HashSet<Entity>>,
) {
    // The victim's body (the model that carries the effect file's bones) and its effect file.
    let victim_pax = |victim: Entity| {
        let body = fighters.get(victim).ok()?.hurt_body;
        let pax = lib.for_rig(&anims.get(body).ok()?.rig)?;
        Some((body, pax))
    };
    for r in req.read() {
        match *r {
            GameEffect::Ineffective { point, dir } => {
                start_placed(&mut commands, &lib.system, EFFECT_INEFFECTIVE, Bind::Fixed(Mat4::from_translation(point)), Placement::hit(Some(dir)));
            }
            GameEffect::Damage { victim, effect, point } => {
                let Some((_, pax)) = victim_pax(victim) else { continue };
                let mask = 1u64 << (32 + (effect.max(1) as u32 - 1)) | CATEGORY_DAMAGE;
                let Some(id) = element_by_category(pax, mask) else { continue };
                if std::env::var_os("KH2_FXLOG").is_some() {
                    info!("damage effect {effect} -> element id {id} of {:?}", pax.apdx);
                }
                // The game starts it one unit above the hit point (its y is down, so minus one).
                let at = point + Vec3::Y * 0.01;
                start_placed(&mut commands, pax, id, Bind::Fixed(Mat4::from_translation(at)), Placement::hit(None));
            }
            GameEffect::Death { victim } => {
                let Some((body, pax)) = victim_pax(victim) else { continue };
                if !dead.insert(victim) {
                    continue;
                }
                if std::env::var_os("KH2_FXLOG").is_some() {
                    info!("death effect: element id {EFFECT_DEATH} of {:?}", pax.apdx);
                }
                commands.entity(victim).insert(DeathFx);
                start(&mut commands, pax, EFFECT_DEATH, Bind::Owner(body));
            }
        }
    }
}

/// Until the enemy code sends `GameEffect::Death` itself: an enemy whose HP is 0 dies.
pub fn enemy_deaths(enemies: Query<(Entity, &Enemy), Without<DeathFx>>, mut out: MessageWriter<GameEffect>) {
    for (e, enemy) in &enemies {
        if enemy.hp <= 0 {
            out.write(GameEffect::Death { victim: e });
        }
    }
}

/// `KH2_FXKILL=<seconds>`: at that time every living enemy is defeated (HP 0, state dying), to
/// look at the death effect without a fight.
pub fn fx_kill(time: Res<Time>, mut enemies: Query<&mut Enemy>, mut done: Local<bool>) {
    let Some(at) = std::env::var("KH2_FXKILL").ok().and_then(|v| v.parse::<f32>().ok()) else { return };
    if *done || time.elapsed_secs() < at || !enemies.iter().any(|e| e.alive()) {
        return;
    }
    *done = true;
    for mut e in &mut enemies {
        e.hp = 0;
        e.state = crate::enemy::EState::Dying;
        e.timer = 0.0;
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
    distort: Res<DistortHandle>,
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
                    if b.offsets.is_empty() {
                        let tex = b.tex0.map(|t| texture(&mut cache, &mut images, &r.lib, b.dpd, t));
                        let mat = material(&mut cache, &mut materials, tex, b.alpha);
                        commands.entity(child).remove::<MeshMaterial3d<DistortMaterial>>().insert((MeshMaterial3d(mat), Visibility::Inherited));
                    } else {
                        commands
                            .entity(child)
                            .remove::<MeshMaterial3d<StandardMaterial>>()
                            .insert((MeshMaterial3d(distort.0.clone()), Visibility::Inherited));
                    }
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
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, b.verts.iter().map(|v| back(v.pos)).collect::<Vec<_>>())
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_COLOR,
            VertexAttributeValues::Float32x4(b.verts.iter().map(|v| v.color.to_array()).collect()),
        )
        .with_inserted_indices(Indices::U32(b.indices.clone()));
    if b.offsets.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, b.verts.iter().map(|v| v.uv).collect::<Vec<_>>());
    } else {
        // Distortion batch: the displacement (in metres) rides in UV 0 (x, y) and UV 1 (z).
        let off: Vec<[f32; 3]> = b.offsets.iter().map(|o| (FLIP.transform_vector3(*o) * SCALE).to_array()).collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, off.iter().map(|o| [o[0], o[1]]).collect::<Vec<_>>());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, off.iter().map(|o| [o[2], 0.0]).collect::<Vec<_>>());
    }
    mesh
}

/// `KH2_FXTEST=<id>`: starts system effect `id` 1.2 m in front of Sora at chest height once a second
/// (`KH2_FXTEST_EVERY=<ticks>` changes the interval; visual checks without needing a fight).
/// `<name>:<id>` starts element `id` of `obj/<name>.apdx`
/// (any object's effect file, loaded on first use), `k<id>` one of the keyblade's. With
/// `KH2_FXTEST_BIND=sora` the effect is bound to Sora's body (so effects that follow a moving
/// owner, like tails, can be seen while he walks) instead of placed in front of him.
#[allow(clippy::too_many_arguments)]
pub fn fx_test(
    mut commands: Commands,
    lib: Res<EffectLib>,
    disc: Res<Disc>,
    sora: Query<(&Transform, &Sora)>,
    mut ticks: Local<u32>,
    mut extra: Local<HashMap<String, Option<Arc<LoadedPax>>>>,
) {
    let Some(spec) = std::env::var("KH2_FXTEST").ok() else { return };
    // "<name>:<id>" (e.g. "M_EX020:4"), "k<id>" for the keyblade, else a system effect id.
    let (pax, id) = if let Some((name, n)) = spec.split_once(':') {
        let key = format!("obj/{name}.apdx");
        let p = match lib.objects.get(&key) {
            Some(p) => Some(p.clone()),
            None => {
                if !extra.contains_key(&key) {
                    let common = disc.read("effect/texcommon.dpd").and_then(|b| Dpd::parse(&b).ok());
                    let loaded = load(&disc, &key, 100 + extra.len(), common.as_ref()).map(Arc::new);
                    extra.insert(key.clone(), loaded);
                }
                extra.get(&key).cloned().flatten()
            }
        };
        let Some(p) = p else { return };
        (p, n.parse::<u16>().unwrap_or(0))
    } else {
        match spec.strip_prefix('k') {
            Some(n) => (lib.keyblade.clone(), n.parse::<u16>().unwrap_or(0)),
            None => (lib.system.clone(), spec.parse::<u16>().unwrap_or(0)),
        }
    };
    *ticks += 1;
    let every = std::env::var("KH2_FXTEST_EVERY").ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(60).max(1);
    if *ticks % every != 1 % every {
        return;
    }
    let Ok((t, s)) = sora.single() else { return };
    if std::env::var("KH2_FXTEST_BIND").is_ok_and(|v| v == "sora") {
        start(&mut commands, &pax, id, Bind::Owner(s.body));
        return;
    }
    // `KH2_FXTEST_DIST=<m>`: distance in front of Sora (negative: behind him, toward the camera).
    let dist = std::env::var("KH2_FXTEST_DIST").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.2);
    let off = std::env::var("KH2_FXTEST_AT")
        .ok()
        .map(|v| v.split(',').filter_map(|x| x.parse::<f32>().ok()).collect::<Vec<_>>())
        .filter(|v| v.len() == 3)
        .map_or(t.rotation * Vec3::Z * dist + Vec3::Y, |v| Vec3::new(v[0], v[1], v[2]));
    let at = t.translation + off;
    start(&mut commands, &pax, id, Bind::Fixed(Mat4::from_translation(at)));
}
