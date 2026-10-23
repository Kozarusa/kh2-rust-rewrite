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
use kh2_audio::footstep::DustRules;
use kh2_fx::pax::element_flag;
use kh2_fx::{Batch, Dpd, Element, Env, Pax};
use kh2_ps2::gs::Vram;

use crate::SCALE;
use crate::assets::Disc;
use crate::combat::{Fighter, Team};
use crate::shared::enemy::Enemy;
use crate::skel::Animator;
use crate::shared::sora::Sora;

/// The lens pass: distortion batches that read the display buffer (WP-I3).
#[path = "fx_lens.rs"]
pub mod lens;

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
/// Element id of the weapon's own effect that `FUN_00184498` starts when the weapon appears
/// (`FUN_0016a570(weapon, 0, 1, 0, 0)` = `FUN_001db868(weapon + 0x80, 0, 1, 1, 0, weapon)`: the first element
/// with id 0 of the weapon object's effect file, bound to the weapon).
pub const EFFECT_WEAPON_APPEAR: u16 = 0;
/// Effect started in place of the hit spark when a hit is ineffective (FUN_0018d090 starts element
/// id 0x12 of the system file `00effect.bar` at the hit point).
pub const EFFECT_INEFFECTIVE: u16 = 0x12;
/// Element id of the Heartless death effect in the dying object's own effect file (the `dead`
/// mode of the Shadow and Soldier scripts runs `spawn_attached(obj, 2, 1, 0)`).
pub const EFFECT_DEATH: u16 = 2;
/// Low category bits of an effect element that the victim-side `DamageEffect` lookup requires.
const CATEGORY_DAMAGE: u64 = 0x200;

/// Effects the game's damage and death code starts, as requests (see `docs/systems/combat-effects/effects-damage.md` for
/// the hook points in `combat.rs` and `enemy.rs`). `fx.rs` itself turns every `combat::Hit` into
/// `Damage` or `Ineffective`. `enemy::receive_hits` sends `Death` when HP reaches 0 (the Heartless
/// scripts' `dead` mode asks for it too, in `enemy_ai`); a second `Death` for the same victim is ignored.
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

/// The effect file of the attacking object of a spell hit, sent with the `combat::Hit` by `magic.rs`.
/// FUN_0018d090 takes the hit spark from the attacking object's own effect file; a spell's attack
/// object belongs to the spell's script, whose effect file is the `.mag` PAX the cast loaded
/// (FUN_001a45c0; the script anchors its attacks to it, host 2.30). `hit_sparks` pairs it with the
/// hit of the same target and `atkp` id.
#[derive(Message, Clone)]
pub struct SpellHit {
    pub target: Entity,
    pub atkp: i16,
    pub pax: Arc<LoadedPax>,
}

/// Marks a victim whose death effect has started.
#[derive(Component)]
pub struct DeathFx;

/// Screen-distortion material for `pppSnoMdlDttNorm` batches: the triangles show the scene drawn
/// so far (the transmission copy of the main pass) at the screen position of the vertex shifted by
/// its displacement, tinted by the vertex color. The displacement rides in the UV attributes (UV 0
/// x, y and UV 1 x), the game's depth bias as a factor on the view-space depth in UV 1 y.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(DistortKey)]
pub struct DistortMaterial {
    /// Unused; a material needs at least one binding.
    #[uniform(0)]
    pub unused: Vec4,
    /// The GS depth test of the draw: ZTST GREATER (true) or ALWAYS (entry byte +0x17 set).
    pub depth_test: bool,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct DistortKey {
    depth_test: bool,
}

impl From<&DistortMaterial> for DistortKey {
    fn from(m: &DistortMaterial) -> DistortKey {
        DistortKey { depth_test: m.depth_test }
    }
}

impl Material for DistortMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path(concat!("embedded://", env!("CARGO_CRATE_NAME"), "/fx_distort.wgsl").into())
    }
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(concat!("embedded://", env!("CARGO_CRATE_NAME"), "/fx_distort.wgsl").into())
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
        key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        // The game's back-face test is done by the program itself (the console draws both sides
        // otherwise), and the lens does not hide effects drawn after it (no depth write; the GS
        // packet's TEST register has AFAIL = FB_ONLY).
        descriptor.primitive.cull_mode = None;
        if let Some(ds) = descriptor.depth_stencil.as_mut() {
            ds.depth_write_enabled = Some(false);
            if !key.bind_group_data.depth_test {
                ds.depth_compare = Some(bevy::render::render_resource::CompareFunction::Always);
            }
        }
        Ok(())
    }
}

/// The two distortion materials: with the depth test (ZTST GREATER) and without (ALWAYS).
#[derive(Resource)]
pub struct DistortHandle {
    tested: Handle<DistortMaterial>,
    always: Handle<DistortMaterial>,
}

/// The game's projection constants, read from the executable on the disc; `None` when the
/// executable could not be read (effects then skip the screen clip test).
#[derive(Resource)]
pub struct FxProjection(pub Option<kh2_fx::Projection>);

/// What an effect is bound to.
#[derive(Clone, Copy)]
pub enum Bind {
    /// A bone of an animated model (the element's bone).
    Owner(Entity),
    /// A fixed world matrix (Bevy space), e.g. a hit point.
    Fixed(Mat4),
}

/// Element flag 0x20000: the effect is placed in camera space every tick (FUN_001dc138 -> bind record +0x5c bit 2,
/// FUN_001dd0d0 -> FUN_001dc978).
pub const ELEMENT_IN_CAMERA_SPACE: u32 = 0x20000;

/// Step of the bind record's clock per 60 Hz tick (inference: the game adds its frame step DAT_00356f40,
/// 2.0 per update in its own units, which this port takes as one 60 Hz tick).
pub const EFFECT_TIME_STEP: f32 = 1.0;
/// 60 Hz ticks per particle-program frame (WP-I3, measured): the effect library runs one program frame per
/// field frame (the draw `FUN_001e3d00` advances the instance by one `FUN_001e6240`; the update `FUN_001e3898`
/// adds one more only for each 2.0 (`DAT_0037ce98`) of frame step beyond the first, a catch-up for slow frames),
/// and a field frame is two vblanks (step 2.0, `FUN_0014cdd0`). So a program frame takes two of the app's ticks:
/// an effect life of N frames lasts N / 30 s. Test `kh2-fx/tests/clock_oracle.rs`.
pub const TICKS_PER_PROGRAM_FRAME: u32 = 2;
/// Added to a bound element's start wait (DAT_0037c778, FUN_001db5f0).
pub const BOUND_WAIT_EXTRA: f32 = 0.1;

#[derive(Component)]
pub struct RunningEffect {
    pub lib: Arc<LoadedPax>,
    pub element: Element,
    pub effect: kh2_fx::Effect,
    #[allow(dead_code)] // index of the effect's data block; no reader yet
    pub dpd: usize,
    pub bind: Bind,
    pub game_id: u16,
    /// One child mesh entity per draw batch.
    parts: Vec<(Entity, Handle<Mesh>)>,
    /// The bind record's clock (+0x4c): `EFFECT_TIME_STEP` per tick from 0.
    time: f32,
    /// Ticks since the instance started; the programs run on every `TICKS_PER_PROGRAM_FRAME`-th, from the first.
    beat: u32,
    /// The display-buffer lens batches of the last program frame (`lens::LensDraws`).
    lens: Vec<lens::LensBatch>,
    /// The element's start wait (+0x14, plus `BOUND_WAIT_EXTRA` for a bound element): the instance
    /// starts and its sound plays once the clock reaches it (FUN_001db5f0, FUN_001dd0d0).
    wait: f32,
    se_played: bool,
    /// The bind sampled on the first tick, kept for `BIND_ONLY_START` elements.
    bind_once: Option<Mat4>,
    /// Two-point positions of the last read of the owner.
    two_point: Option<[Vec3; 2]>,
    /// The last bind, kept once the owner is gone (the record is marked orphaned and no longer
    /// updated, FUN_001dcd08).
    last_bind: Option<Mat4>,
    orphan: bool,
    /// A fade-out running: (level, change per tick); the effect goes once the level reaches 0
    /// (FUN_001da080 for `DESTROY_FADEOUT`).
    fade: Option<(f32, f32)>,
}

impl RunningEffect {
    fn new(lib: Arc<LoadedPax>, element: Element, effect: kh2_fx::Effect, dpd: usize, bind: Bind, game_id: u16) -> RunningEffect {
        let wait = element.start_wait + if element.flags & element_flag::BIND != 0 { BOUND_WAIT_EXTRA } else { 0.0 };
        RunningEffect { lib, element, effect, dpd, bind, game_id, parts: Vec::new(), time: 0.0, beat: 0, lens: Vec::new(), wait, se_played: false, bind_once: None, two_point: None, last_bind: None, orphan: false, fade: None }
    }
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
        app.add_plugins((MaterialPlugin::<DistortMaterial>::default(), lens::LensPlugin))
            .add_message::<StartEffect>()
            .add_message::<StartSystemEffect>()
            .add_message::<StartBoundSystemEffect>()
            .add_message::<StopEffect>()
            .add_message::<GameEffect>()
            .add_message::<SpellHit>()
            .add_message::<FootDust>()
            .init_resource::<AreaFx>()
            .init_resource::<SparkRing>()
            .add_systems(Startup, setup)
            .add_systems(FixedUpdate, weapon_appear.after(weapon_trails).run_if(crate::schedule::sim_running))
            .add_systems(
                FixedUpdate,
                (
                    (
                        weapon_trails,
                        motion_effects,
                        start_requests,
                        start_system_requests,
                        start_bound_system_requests,
                        stop_requests,
                        load_area_fx,
                        foot_dust,
                        hit_sparks,
                        fx_kill,
                        game_effects,
                        fx_test,
                        fx_count,
                    )
                        .chain()
                        .in_set(Sim::Effects),
                    tick.in_set(Sim::EffectTick),
                ),
            );
    }
}

pub fn setup(mut commands: Commands, disc: Res<Disc>, rigs: Res<crate::shared::sora::Rigs>, mut distort: ResMut<Assets<DistortMaterial>>) {
    commands.insert_resource(DistortHandle {
        tested: distort.add(DistortMaterial { unused: Vec4::ZERO, depth_test: true }),
        always: distort.add(DistortMaterial { unused: Vec4::ZERO, depth_test: false }),
    });
    let proj = disc.read_root("SLUS_210.05").and_then(|exe| kh2_fx::Projection::from_exe(&exe));
    if proj.is_none() {
        warn!("fx: could not read the projection constants from SLUS_210.05; effects skip the screen clip test");
    }
    commands.insert_resource(FxProjection(proj));
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

/// Request to start an element of the system effect file `00effect.bar` at a fixed place (what
/// `FUN_001db868(&DAT_01c954d8, id, 1, 1, 10000, 0)` does, as the clash spark `FUN_0018d560` uses it).
/// `at` is a world matrix in Bevy space; its translation replaces the element's own translation.
#[derive(Message, Clone, Copy, Debug)]
pub struct StartSystemEffect {
    pub id: u16,
    pub at: Mat4,
}

pub fn start_system_requests(mut commands: Commands, lib: Res<EffectLib>, mut ring: ResMut<SparkRing>, mut req: MessageReader<StartSystemEffect>) {
    for r in req.read() {
        if std::env::var_os("KH2_FXLOG").is_some() {
            info!("system effect {:#x} at {:?}", r.id, r.at.w_axis.truncate());
        }
        // Every system effect started this way is a clash spark (FUN_0018d560), which FUN_001c09f8 registers.
        let spark = start_placed(&mut commands, &lib.system, r.id, Bind::Fixed(r.at), Placement { replace_translation: true, ..default() });
        ring_push(&mut commands, &mut ring, spark);
    }
}

/// The handle of an owner-bound system effect, what `FUN_001db868` returns and its caller keeps to end
/// the effect later (the player's pickup glow at +0xae4, `FUN_00175c28`). The caller takes a fresh one with
/// `FxHandle::new` and sends it with `StartBoundSystemEffect`; `fx` puts it on the running effect.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FxHandle(pub u32);

impl FxHandle {
    /// A handle no other effect carries.
    pub fn new() -> FxHandle {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        FxHandle(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

impl Default for FxHandle {
    fn default() -> FxHandle {
        FxHandle::new()
    }
}

/// Request to start an element of the system effect file `00effect.bar` bound to an owner, what
/// `FUN_001db868(&DAT_01c954d8, id, .., owner)` does with an owner (sixth argument, the record's +0x48):
/// the element follows the owner's bone as any bound effect (`tick`, FUN_001dcd08) and is not filed in
/// the spark ring (only the spark callers of `FUN_001c09f8` are). `owner` is a model entity (it carries
/// the `Animator`). The effect ends by itself, with its owner as its element flags say, or on `StopEffect`.
/// The middle arguments of `FUN_001db868` (the pickup glow passes 0, 1, 0x2714) are not modelled, as
/// for `StartSystemEffect`.
#[derive(Message, Clone, Copy, Debug)]
pub struct StartBoundSystemEffect {
    pub id: u16,
    pub owner: Entity,
    pub handle: FxHandle,
}

/// Request to end the effect carrying `handle`: `kill` false is `FUN_001d9f48` (the record's +8 bit 0 and
/// the instance's loop-end byte +0x85, `FUN_001e0298`: the effect stops emitting and goes when its
/// particles are done, as `effect.end()`); `kill` true is `FUN_001da028` (gone at once). A handle with no
/// running effect is ignored (the effect already ended or never started: no element with that id).
#[derive(Message, Clone, Copy, Debug)]
pub struct StopEffect {
    pub handle: FxHandle,
    pub kill: bool,
}

pub fn start_bound_system_requests(mut commands: Commands, lib: Res<EffectLib>, mut req: MessageReader<StartBoundSystemEffect>) {
    for r in req.read() {
        let e = start_placed(&mut commands, &lib.system, r.id, Bind::Owner(r.owner), Placement::default());
        if std::env::var_os("KH2_FXLOG").is_some() {
            info!("system effect {:#x} bound to {:?} (handle {}): {}", r.id, r.owner, r.handle.0, if e.is_some() { "started" } else { "no element" });
        }
        if let Some(e) = e {
            commands.entity(e).insert(r.handle);
        }
    }
}

pub fn stop_requests(mut commands: Commands, mut effects: Query<(Entity, &FxHandle, &mut RunningEffect)>, mut req: MessageReader<StopEffect>) {
    for r in req.read() {
        for (e, h, mut fx) in &mut effects {
            if *h != r.handle {
                continue;
            }
            if std::env::var_os("KH2_FXLOG").is_some() {
                info!("effect {:#x} (handle {}) {}", fx.game_id, h.0, if r.kill { "killed" } else { "told to end" });
            }
            if r.kill {
                commands.entity(e).despawn();
            } else {
                fx.effect.end();
            }
        }
    }
}

/// First id handed to the effect file of an area (`AreaFx`); the system file is 1, the object files
/// count up from 2, so the area files stay clear of both and of each other (the texture cache keys
/// on the id).
const AREA_FX_ID_BASE: usize = 1000;

/// A footstep dust request, what `FUN_001b1c90(actor, kind)` is asked for: by a motion's trigger 19
/// (the kind is its first parameter) and by the ground mover `FUN_0017af10` (kinds 8 and 9 with the
/// stop step, 10 and 11 when movement starts). `crates/kh2-app/src/audio.rs` sends them from the same
/// place that plays the footstep sounds.
#[derive(Message, Clone, Copy, Debug)]
pub struct FootDust {
    pub kind: u16,
    /// The actor's position in Bevy world space (metres), at its feet.
    pub at: Vec3,
    /// The actor's facing in Bevy world space.
    pub facing: Vec3,
    /// `FUN_00183730`'s ground material of the actor (selects the dust class).
    pub material: u8,
    /// Who asked, for `KH2_FXLOG`.
    pub why: &'static str,
}

/// The effect file of the current area: the PAX of its map file (`map/us/<area>.map`, BAR type 18),
/// which the game keeps as the area's own effect manager (`FUN_0015e6c0` hands the map's PAX, map
/// +0x50, to `FUN_001db4d0(0x35a7f0)`; `FUN_001b1c90` looks its dust up there), and the dust tables
/// read from the executable.
#[derive(Resource, Default)]
pub struct AreaFx {
    area: Option<String>,
    pub pax: Option<Arc<LoadedPax>>,
    pub rules: Option<DustRules>,
}

/// Loads the area's own effect file whenever a new area has finished loading.
pub fn load_area_fx(disc: Res<Disc>, area: Res<crate::assets::CurrentArea>, mut fx: ResMut<AreaFx>, mut serial: Local<usize>) {
    let Some(loaded) = area.loaded.clone() else { return };
    if fx.area.as_deref() == Some(loaded.as_str()) {
        return;
    }
    fx.area = Some(loaded.clone());
    if fx.rules.is_none() {
        fx.rules = disc.read_root("SLUS_210.05").and_then(|exe| DustRules::from_exe(&exe));
        if fx.rules.is_none() {
            warn!("fx: could not read the footstep dust tables from SLUS_210.05; no footstep dust");
        }
    }
    let common = disc.read("effect/texcommon.dpd").and_then(|b| Dpd::parse(&b).ok());
    *serial += 1;
    fx.pax = load(&disc, &format!("map/us/{loaded}.map"), AREA_FX_ID_BASE + *serial, common.as_ref()).map(Arc::new);
    if std::env::var_os("KH2_FXLOG").is_some() {
        info!("{loaded}: area effect file {}", fx.pax.as_ref().map_or("none".to_string(), |p| format!("{} elements", p.pax.elements.len())));
    }
}

/// Starts the footstep dust (`FUN_001b1c90`): the ground material gives a dust class (a byte table
/// of the executable; class 0 starts nothing), the effect is the first element of the area's own
/// effect file whose category holds the class bit and bit `kind + 0x20` (`FUN_001db9e0`), started at
/// the ground point under the foot (a vertical probe from 50 above to 100 below the foot, started
/// only when it finds ground) and turned to the horizontal direction from the foot to the toe. Kinds
/// above 0x13 start at the actor's own position and facing with no probe. The foot is not looked up
/// by its bone: the foot point is the actor's position and the direction its facing (`[inferred]`:
/// the game takes both from the foot bones `0x8028` / `0x8029` and `0x802c` / `0x802d`, whose
/// mapping to the model's bones was not decoded, `FUN_0016aa18`).
pub fn foot_dust(
    mut commands: Commands,
    area: Res<AreaFx>,
    world: Option<Res<crate::world::World>>,
    mut req: MessageReader<FootDust>,
) {
    let log = std::env::var_os("KH2_FXLOG").is_some();
    for r in req.read() {
        let (Some(pax), Some(rules)) = (area.pax.as_ref(), area.rules.as_ref()) else {
            if log {
                info!("foot dust kind {} ({}): no area effect file", r.kind, r.why);
            }
            continue;
        };
        let Some(mask) = rules.mask(r.material, r.kind) else {
            if log {
                info!("foot dust kind {} ({}): material {} has no dust class", r.kind, r.why, r.material);
            }
            continue;
        };
        let Some(id) = element_by_category(pax, mask) else {
            if log {
                info!("foot dust kind {} ({}): material {} class {} mask {mask:#x}: no element", r.kind, r.why, r.material, rules.class(r.material));
            }
            continue;
        };
        let at = if DustRules::at_foot(r.kind) {
            // FUN_001a5ce8: a vertical line through the foot point from `probe_above` above it to
            // `probe_below` below it (centimetres); no ground on it, no dust.
            let Some(w) = world.as_ref() else { continue };
            let cm = r.at / crate::SCALE;
            let Some(g) = w.ground(cm, cm.y + rules.probe_above, rules.probe_above + rules.probe_below) else {
                if log {
                    info!("foot dust kind {} ({}): no ground under the foot", r.kind, r.why);
                }
                continue;
            };
            Vec3::new(cm.x, g.y, cm.z) * crate::SCALE
        } else {
            r.at
        };
        let facing = r.facing.with_y(0.0).try_normalize().unwrap_or(Vec3::Z);
        if log {
            info!(
                "foot dust kind {} ({}): material {} class {} -> element id {id} at {at:?} facing {facing:?}",
                r.kind,
                r.why,
                r.material,
                rules.class(r.material)
            );
        }
        start_placed(&mut commands, pax, id, Bind::Fixed(Mat4::from_translation(at)), Placement::hit(Some(facing)));
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

/// The DPX effect an element names: the entry whose number (its third word) equals the element's `effect`
/// (`FUN_001d9720` walks the DPX list comparing that word; `FUN_001db5f0` -> `FUN_001da6f0` -> `FUN_001d9bf0`). The
/// number is not always the entry's place: `00effect.bar` has 14 entries numbered up to 17.
pub fn dpx_effect(pax: &Pax, number: u16) -> Option<&kh2_fx::pax::DpxEffect> {
    pax.effects.iter().find(|f| f.number == number as u32)
}

/// Starts the first element of `lib` with game effect id `id` (FUN_001db868 returns after the
/// first match; no file on the disc repeats an id, so this is the only element either way).
pub fn start_placed(commands: &mut Commands, lib: &Arc<LoadedPax>, id: u16, bind: Bind, place: Placement) -> Option<Entity> {
    let el = lib.pax.elements_for(id).next()?;
    let fx = dpx_effect(&lib.pax, el.effect)?;
    let effect = kh2_fx::Effect::new(lib.dpds[fx.dpd].clone(), fx.dpd, fx.particle_data)?;
    // `KH2_FXONLY=<id,id,...>` runs only those effect IDs (isolating an effect in a screenshot).
    if let Ok(only) = std::env::var("KH2_FXONLY")
        && !only.split(',').any(|v| v.trim().parse::<u16>().ok() == Some(id))
    {
        return None;
    }
    let mut element = el.clone();
    if place.replace_translation {
        element.translation = [0.0; 3];
    }
    if let Some(r) = place.rotation {
        element.rotation = r;
    }
    Some(commands.spawn((Transform::default(), Visibility::default(), RunningEffect::new(lib.clone(), element, effect, fx.dpd, bind, id))).id())
}

/// Slots of the game's ring of recent spark effects (`DAT_01d5df30`, 8 words, index `DAT_01d5df50`).
pub const SPARK_RING_SLOTS: usize = 8;
/// The fade time `FUN_001c09f8` hands to `FUN_001da080` for the effect it pushes out (`DAT_0037bd90`, 4.0).
pub const SPARK_EVICT_FADE: f32 = 4.0;

/// The ring `FUN_001c09f8` keeps of the last 8 spark effects: the hit spark and the "ineffective" effect
/// of `FUN_0018d090`, the wall spark of `FUN_0018ca78` and the clash spark of `FUN_0018d560`. A new entry
/// takes the slot of the oldest, and the effect that was in it is faded out (`FUN_001da080(4.0, effect)`).
/// This is the only thing that ends a looping spark such as system effect 0xd, which has no owner and no
/// end of its own (docs/systems/combat-effects/hit-reaction.md "Lifetime of the clash spark").
#[derive(Resource, Default)]
pub struct SparkRing {
    slots: [Option<Entity>; SPARK_RING_SLOTS],
    next: usize,
}

impl SparkRing {
    /// Registers a spark; returns the effect it pushes out of the ring, if the slot held one.
    pub fn push(&mut self, e: Entity) -> Option<Entity> {
        let old = self.slots[self.next].replace(e);
        self.next = (self.next + 1) % SPARK_RING_SLOTS;
        old
    }
}

/// Marks a spark pushed out of the ring: `tick` fades it out over `SPARK_EVICT_FADE` frames.
#[derive(Component)]
pub struct Evicted;

/// Marks a spark registered in the ring. [inferred] A looping one (system effect 0xd) also ends after its own
/// life plus the fade: the decompiled rule only ends it when 8 further sparks follow, but the user sees a lone
/// clash spark stay (docs/systems/combat-effects/hit-reaction.md "Lifetime of the clash spark").
#[derive(Component)]
pub struct RingSpark;

/// Whether a looping ring spark has run its program life (`pd.life`, `TICK` per program frame, each
/// `TICKS_PER_PROGRAM_FRAME` ticks) on the clock `time` (ticks).
pub fn ring_spark_expired(time: f32, life: i32) -> bool {
    time >= ((life / kh2_fx::pax::TICK) as u32 * TICKS_PER_PROGRAM_FRAME) as f32
}

/// `FUN_001c09f8` for a freshly started spark (`None`: no element, nothing to register).
pub fn ring_push(commands: &mut Commands, ring: &mut SparkRing, spark: Option<Entity>) {
    let Some(e) = spark else { return };
    commands.entity(e).try_insert(RingSpark);
    if let Some(old) = ring.push(e) {
        if std::env::var_os("KH2_FXLOG").is_some() {
            info!("spark ring: {e:?} takes the slot of {old:?}, which fades out");
        }
        commands.entity(old).try_insert(Evicted);
    }
}

/// The weapons that went from hidden to shown between two ticks (`now[i] && !prev[i]`). A weapon
/// that is new in the list (the first tick) counts as hidden before, so a keyblade that is out at
/// the start still shows its effect once, as the game's weapon does when it first becomes visible.
pub fn weapons_appeared(prev: &[bool], now: &[bool]) -> Vec<usize> {
    (0..now.len()).filter(|&i| now[i] && !prev.get(i).copied().unwrap_or(false)).collect()
}

/// The keyblade's appearance effect: when Sora's weapon turns from hidden to shown (the controller
/// shows it with the battle stand-in, `sora::tick`), start element 0 of the weapon's own effect file
/// bound to the weapon, as the weapon's update `FUN_001843b0` does through `FUN_00184498` the moment the
/// owner's motion variant is a weapon one (0 or 3) while the weapon was still hidden (flag 0x100 of
/// +0x10c, set by the constructor `FUN_00184190`). Log with `KH2_FXLOG`.
pub fn weapon_appear(mut commands: Commands, lib: Res<EffectLib>, sora: Query<&crate::shared::sora::Sora>, vis: Query<&Visibility>, mut prev: Local<Vec<bool>>) {
    let Ok(s) = sora.single() else { return };
    appear_step(&mut commands, &lib, &s.weapons, &vis, &mut prev);
}

/// One tick of `weapon_appear` for a list of weapon entities.
fn appear_step(commands: &mut Commands, lib: &EffectLib, weapons: &[Entity], vis: &Query<&Visibility>, prev: &mut Vec<bool>) {
    let now: Vec<bool> = weapons.iter().map(|w| vis.get(*w).is_ok_and(|v| *v != Visibility::Hidden)).collect();
    for i in weapons_appeared(prev, &now) {
        let started = start_placed(commands, &lib.keyblade, EFFECT_WEAPON_APPEAR, Bind::Owner(weapons[i]), Placement::default());
        if std::env::var_os("KH2_FXLOG").is_some() {
            info!("weapon appear: weapon {i} shown, effect element {EFFECT_WEAPON_APPEAR} (FUN_00184498): {}", if started.is_some() { "started" } else { "no such element" });
        }
    }
    *prev = now;
}

/// Weapon trails: run effect 10 while the keyblade's motion has its trail range trigger.
pub fn weapon_trails(
    mut commands: Commands,
    lib: Res<EffectLib>,
    sora: Query<&crate::shared::sora::Sora>,
    anims: Query<&Animator>,
    mut running: Query<&mut RunningEffect>,
) {
    let Ok(s) = sora.single() else { return };
    for &w in &s.weapons {
        let Ok(weapon) = anims.get(w) else { continue };
        let on = weapon.trigger(TRIGGER_TRAIL);
        let mut have = false;
        for mut r in running.iter_mut().filter(|r| r.owner() == Some(w) && r.game_id == EFFECT_TRAIL) {
            if !on {
                r.effect.end();
            } else if !r.effect.is_ending() {
                have = true;
            }
        }
        if on && !have {
            start(&mut commands, &lib.keyblade, EFFECT_TRAIL, Bind::Owner(w));
        }
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
pub(crate) fn element_by_category(pax: &LoadedPax, mask: u64) -> Option<u16> {
    pax.pax.elements.iter().find(|e| e.category & mask == mask).map(|e| e.id)
}

/// Hit effects, as FUN_0018d090 starts them for every landed hit. An ineffective hit sends
/// `GameEffect::Ineffective` and nothing else. Otherwise `EffectOnHit` (plus a random offset up to
/// `RandomEffect`) picks a category bit, together with the target type 1 (a normal target), in the
/// attacker's own effect file (Sora's weapon for his attacks, the enemy's file for its claws, the
/// spell's `.mag` PAX for a spell's attack: `SpellHit`); the first element whose category has all
/// those bits is started at the hit point, turned along the hit. The row is the hit's own
/// (`GameData::hit_row`: a spell hit names its variant row). `DamageEffect` then becomes a
/// `GameEffect::Damage` for the victim. Wall hits only spawn the spark.
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
    mut spells: MessageReader<SpellHit>,
    mut game: MessageWriter<GameEffect>,
    mut ring: ResMut<SparkRing>,
    mut seed: Local<u32>,
) {
    let mut spells: Vec<SpellHit> = spells.read().cloned().collect();
    // (attacker's effect file, atkp row, point, direction, the hit if it is one)
    let mut all = Vec::new();
    for h in hits.read() {
        let Some(atk) = data.hit_row(h) else { continue };
        // The spell's file for a spell hit (each `SpellHit` answers one hit).
        let spell = h.row.and_then(|_| spells.iter().position(|s| s.target == h.target && s.atkp == h.atkp)).map(|i| spells.remove(i).pax);
        let victim = enemies.get(h.target).map(|e| e.stats).ok().or_else(|| sora.get(h.target).map(|s| s.stats).ok());
        if ineffective(atk, victim) {
            game.write(GameEffect::Ineffective { point: h.point, dir: h.dir });
            continue;
        }
        if atk.damage_effect != 0 {
            game.write(GameEffect::Damage { victim: h.target, effect: atk.damage_effect, point: h.point });
        }
        let pax = spell.or_else(|| {
            fighters.get(h.attacker).ok().and_then(|f| match f.team {
                // Sora's keyblade, or a party member's own weapon (its `attack_bodies` are the body, then
                // the hand objects; `party::spawn_party` registers the weapon's effect file); a member
                // whose weapon has no effect file falls back to the keyblade's [WP-F1 item 5].
                Team::Player if sora.get(h.attacker).is_ok() => Some(lib.keyblade.clone()),
                Team::Player => f.attack_bodies.get(1).and_then(|b| anims.get(*b).ok()).and_then(|a| lib.for_rig(&a.rig)).cloned().or_else(|| Some(lib.keyblade.clone())),
                Team::Enemy => f.attack_bodies.first().and_then(|b| anims.get(*b).ok()).and_then(|a| lib.for_rig(&a.rig)).cloned(),
            })
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
            info!("hit atkp {} SubId {} effect {} -> element id {id} of {:?} (effect file {}) at {:?}", atk.id, atk.sub_id, atk.effect_on_hit, pax.apdx, pax.id, point);
        }
        let place = if dir.is_some() { Placement::hit(dir) } else { Placement::default() };
        let spark = start_placed(&mut commands, &pax, id, Bind::Fixed(Mat4::from_translation(point)), place);
        ring_push(&mut commands, &mut ring, spark);
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
    mut ring: ResMut<SparkRing>,
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
                if std::env::var_os("KH2_FXLOG").is_some() {
                    info!("ineffective hit: system effect {EFFECT_INEFFECTIVE:#x} at {point:?}");
                }
                let spark = start_placed(&mut commands, &lib.system, EFFECT_INEFFECTIVE, Bind::Fixed(Mat4::from_translation(point)), Placement::hit(Some(dir)));
                ring_push(&mut commands, &mut ring, spark);
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

/// `KH2_FXKILL=<seconds>[,<seconds>...]`: at each of those times every living enemy is defeated
/// (HP 0, the death effect requested as `enemy::receive_hits` does), to look at the death effect
/// without a fight. A time with no living enemy waits for the next wave.
pub fn fx_kill(time: Res<Time>, mut enemies: Query<(Entity, &mut Enemy)>, mut done: Local<usize>, mut out: MessageWriter<GameEffect>) {
    let Ok(spec) = std::env::var("KH2_FXKILL") else { return };
    let Some(at) = spec.split(',').filter_map(|v| v.trim().parse::<f32>().ok()).nth(*done) else { return };
    if time.elapsed_secs() < at || !enemies.iter().any(|(_, e)| e.alive()) {
        return;
    }
    *done += 1;
    for (ent, mut e) in &mut enemies {
        if e.alive() {
            e.hp = 0;
            out.write(GameEffect::Death { victim: ent });
        }
    }
}

/// One effect tick for every running effect, after the poses of this tick are known.
#[allow(clippy::too_many_arguments)]
pub fn tick(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut RunningEffect)>,
    evicted: Query<(), With<Evicted>>,
    ringed: Query<(), With<RingSpark>>,
    owners: Query<&Animator>,
    sora: Query<&Sora>,
    locals: Query<(&Transform, Option<&ChildOf>)>,
    camera: Query<(&GlobalTransform, &Projection), crate::menu::preview::GameCamera>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<FxCache>,
    distort: Res<DistortHandle>,
    projection: Res<FxProjection>,
    mut lens_draws: ResMut<lens::LensDraws>,
    mut se: MessageWriter<crate::audio::PlaySe>,
) {
    let view_inv = camera.single().map(|(c, _)| game_camera(c.to_matrix())).unwrap_or(Mat4::IDENTITY);
    // The game's projection for the camera's field of view (camera.rs makes the vertical field from the
    // game's horizontal one at 4:3: fov = 2 atan(0.75 tan(hfov / 2))).
    let mut gone = Vec::new();
    let proj = projection.0.and_then(|p| {
        let (_, Projection::Perspective(pp)) = camera.single().ok()? else { return None };
        let hfov = 2.0 * ((pp.fov * 0.5).tan() / 0.75).atan();
        Some(p.matrix(hfov))
    });
    for (e, mut r) in &mut effects {
        // A spark pushed out of the ring fades out over `SPARK_EVICT_FADE` frames (FUN_001da080).
        let pd = r.effect.particle_data();
        let lone_loop_over = pd.flags & 1 != 0 && ring_spark_expired(r.time, pd.life) && ringed.contains(e);
        if r.fade.is_none() && (evicted.contains(e) || lone_loop_over) {
            r.fade = Some((1.0, -1.0 / SPARK_EVICT_FADE));
        }
        let el = r.element.clone();
        // The bind record's clock (FUN_001dd0d0 adds the frame step to +0x4c at its end): an element with
        // a start wait does not run, draw or sound until the clock reaches it (inference for the unit: one
        // per 60 Hz tick, as the effect's own clock).
        if r.time < r.wait {
            r.time += EFFECT_TIME_STEP;
            continue;
        }
        // The element's sound plays once, from where the effect is on its first tick (below, once the bind is known).
        let play_se = !r.se_played && el.se >= 0;
        r.se_played = true;
        r.time += EFFECT_TIME_STEP;
        // One program frame per field frame (`TICKS_PER_PROGRAM_FRAME` ticks): between them the last frame's
        // draws stay and only the fade steps (FUN_001da1e8 moves it by the frame step every update).
        r.beat += 1;
        if (r.beat - 1) % TICKS_PER_PROGRAM_FRAME != 0 {
            if let Some((level, step)) = r.fade {
                if level + step <= 0.0 {
                    gone.push(e);
                commands.entity(e).despawn();
                    continue;
                }
                r.fade = Some((level + step, step));
            }
            continue;
        }
        let mut env = Env { view: view_inv.inverse(), view_inv, proj, ..Default::default() };
        // Element flag 0x20000 (WP-I2): the bind record sits in camera space (FUN_001dc138 sets its +0x5c bit 2;
        // FUN_001dd0d0 then runs FUN_001dc978 every tick: the element's own translation, rotation and scale put
        // through the camera's inverse view, FUN_001068c0(1)), so the effect stays in front of the camera.
        if el.flags & ELEMENT_IN_CAMERA_SPACE != 0 {
            env.bind = view_inv * element_matrix(&el);
        } else {
        match r.bind {
            Bind::Owner(owner) => {
                // The bind record follows its owner every tick (FUN_001dd0d0 -> FUN_001dcd08 for mode 1):
                // the owner's bone matrix is read again unless the element is not bound (flag 1 clear:
                // read once when created) or binds only at its start (flag 2: not read again once the
                // instance runs). `bone` numbers with 0x4000 name object frames (FUN_0016aa18, see
                // `object_bone`: the root, the body centre, the weapon's blade tip, a shape's centre).
                let frozen = r.bind_once.is_some() && (el.flags & element_flag::BIND == 0 || el.flags & element_flag::BIND_ONLY_START != 0);
                let mut now = if frozen { r.bind_once } else { None };
                if now.is_none() && !r.orphan {
                    if let Ok(anim) = owners.get(owner) {
                        let body = GlobalTransform::from(world_now(owner, &locals));
                        let bone = if el.bone & 0x4000 != 0 {
                            // A party member's weapons by hand (Sora's list); other owners are their own weapon.
                            let party = sora.iter().find(|s| s.body == owner);
                            let weapons: Vec<Option<ObjectFrame>> =
                                party.map_or(Vec::new(), |s| s.weapons.iter().map(|w| owners.get(*w).ok().map(|a| ObjectFrame::of(a, world_now(*w, &locals)))).collect());
                            object_bone(el.bone as u16, &ObjectFrame::of(anim, body.to_matrix()), &weapons, party.is_some())
                        } else {
                            anim.bone_world(&body, el.bone.max(0) as usize)
                        };
                        // The element's offset and rotation are in the game's Y-down frame: the same flip that
                        // takes the bone's world placement into fx space applies to its local axes (as for
                        // `Bind::Fixed`). Root-bound elements put their centre at negative Y (above the feet).
                        let m = fx_space(bone) * FLIP * element_matrix(&el);
                        if el.flags & element_flag::TWO_POINT != 0 {
                            let p = |b: u16, ratio: f32| {
                                let len = anim.rig.model.bones.get(b as usize).map_or(0.0, |x| x.length);
                                fx_space(anim.bone_world(&body, b as usize)).transform_point3(Vec3::new(ratio * len, 0.0, 0.0))
                            };
                            r.two_point = Some([p(el.bone_pos.a, el.bone_pos.ratio_a), p(el.bone_pos.b, el.bone_pos.ratio_b)]);
                        }
                        now = Some(m);
                        if r.bind_once.is_none() {
                            r.bind_once = Some(m);
                        }
                    } else {
                        // The owner is gone: the record is marked orphaned (FUN_001dcd08 sets bit 0x10 of
                        // its flags) and keeps its last matrix. Elements with flag 0x200 (destroy when the
                        // object leaves) or 0x400 end now (FUN_001dd0d0 -> FUN_001dd060); the others run on.
                        r.orphan = true;
                        if std::env::var_os("KH2_FXLOG").is_some() {
                            info!("fx {}: owner gone, effect keeps its last bind (element flags {:#x})", r.game_id, el.flags);
                        }
                        if el.flags & (element_flag::DESTROY_WHEN_OBJECT_LEAVES | element_flag::DESTROY_WHEN_BIND_EFFECT_OFF) != 0 {
                            destroy(&mut r);
                        }
                    }
                }
                let Some(m) = now.or(r.last_bind) else {
                    gone.push(e);
                commands.entity(e).despawn();
                    continue;
                };
                r.last_bind = Some(m);
                env.bind = m;
                env.two_point = r.two_point;
            }
            Bind::Fixed(m) => {
                // A placement in metres: rotation and scale stay, the translation becomes centimetres.
                let (sc, rot, t) = m.to_scale_rotation_translation();
                env.bind = FLIP * Mat4::from_scale_rotation_translation(sc, rot, t / SCALE) * FLIP * element_matrix(&el);
            }
        }
        }
        // A fade-out (element flag 0x40: FUN_001da080 over the element's fadeout frames) scales the
        // effect's alpha down to nothing, then the effect is gone.
        if let Some((level, step)) = r.fade {
            let level = level + step;
            if level <= 0.0 {
                gone.push(e);
                commands.entity(e).despawn();
                continue;
            }
            r.fade = Some((level, step));
            env.tint.w = level;
        }
        env.bind_scale = Vec3::new(env.bind.x_axis.truncate().length(), env.bind.y_axis.truncate().length(), env.bind.z_axis.truncate().length());
        if play_se {
            se.write(effect_sound_request(el.se as u32, r.lib.apdx.clone(), el.flags, &env.bind));
        }
        let draws = r.effect.tick(&env);
        if r.effect.done {
            gone.push(e);
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
        // Lens batches that read the display buffer (last frame's picture, HUD included) are drawn by the lens
        // pass (`lens.rs`), not as meshes. A camera-space element keeps its geometry in view space, so the pass
        // places it in front of the camera as it is drawn.
        let camera_space = el.flags & ELEMENT_IN_CAMERA_SPACE != 0;
        r.lens = draws.iter().filter(|b| !b.offsets.is_empty() && b.display_buffer).map(|b| lens::LensBatch::of(b, &env.view, camera_space)).collect();
        if !r.lens.is_empty() {
            lens_draws.frame = lens_draws.frame.wrapping_add(1);
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
            match draws.get(i).filter(|b| !b.indices.is_empty() && !(b.display_buffer && !b.offsets.is_empty())) {
                Some(b) => {
                    if let Some(mut m) = meshes.get_mut(&mesh) {
                        *m = build_mesh(b);
                    }
                    if b.offsets.is_empty() {
                        let tex = b.tex0.map(|t| texture(&mut cache, &mut images, &r.lib, b.dpd, t));
                        let mat = material(&mut cache, &mut materials, tex, b.alpha);
                        commands.entity(child).remove::<MeshMaterial3d<DistortMaterial>>().insert((MeshMaterial3d(mat), Visibility::Inherited));
                    } else {
                        let handle = if b.depth_test { distort.tested.clone() } else { distort.always.clone() };
                        commands.entity(child).remove::<MeshMaterial3d<StandardMaterial>>().insert((MeshMaterial3d(handle), Visibility::Inherited));
                    }
                }
                None => {
                    commands.entity(child).insert(Visibility::Hidden);
                }
            }
        }
    }
    // What the lens pass draws this frame: every running effect's lens of its last program frame.
    lens_draws.batches = effects.iter().filter(|(e, _)| !gone.contains(e)).flat_map(|(_, r)| r.lens.iter().cloned()).collect();
}

/// FUN_001dd060: starts the end of an element's effect once. Element flag 0x80 stops it like a
/// loop end (FUN_001d9f48); flag 0x40 fades it out over its `fadeout_frame` frames (FUN_001da080);
/// with neither the effect runs to its own end.
fn destroy(r: &mut RunningEffect) {
    if r.fade.is_some() {
        return;
    }
    if r.element.flags & element_flag::DESTROY_LOOPEND != 0 {
        r.effect.end();
    } else if r.element.flags & element_flag::DESTROY_FADEOUT != 0 {
        let frames = (r.element.fadeout_frame as f32).max(1.0);
        r.fade = Some((1.0, -1.0 / frames));
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

/// Shape type FUN_0016b7d0 looks for (the object's own body volume).
pub const SHAPE_BODY: u8 = 1;
/// Shape type of the 0x4009 point (FUN_0016aa18 case 0x4009).
pub const SHAPE_POINT_9: u8 = 9;
/// Shape type of a weapon's blade (FUN_0016ae90: its joint is the weapon's bone, its x offset the
/// weapon's length).
pub const SHAPE_WEAPON: u8 = 15;
/// The part of the weapon length 0x4007 / 0x4008 move along the blade (DAT_0037a1bc).
pub const WEAPON_MIDDLE: f32 = 0.5;

/// What FUN_0016aa18 reads of an object for the bone numbers with 0x4000, in Bevy world space: its
/// matrix, the centres of its first type-1 and type-9 shapes, and its blade (the bone matrix of its
/// first type-15 shape with that shape's x offset, the weapon's length).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectFrame {
    pub matrix: Mat4,
    pub body_centre: Option<Vec3>,
    pub point_9: Option<Vec3>,
    pub blade: Option<(Mat4, f32)>,
}

/// The centre of a shape (FUN_001894d0): its offset in its joint's frame, lifted by its +0xe (the
/// game's y is down, so up in Bevy space); the same point `combat::hurt_spheroid` takes.
pub fn shape_point(sh: &kh2_model::collision::Shape, joint: Mat4) -> Vec3 {
    joint.transform_point3(Vec3::from(sh.offset)) + Vec3::Y * (sh.lift * SCALE)
}

impl ObjectFrame {
    /// The frames of an animated object whose world matrix is `matrix`. A shape is the first record of
    /// its type (FUN_0016b558 with -1) [inferred: the first record is the one the enable mask passes,
    /// FUN_0019a6a0; the records' part masks are not modelled].
    pub fn of(anim: &Animator, matrix: Mat4) -> ObjectFrame {
        let body = GlobalTransform::from(matrix);
        let joint = |sh: &kh2_model::collision::Shape| match sh.bone {
            Some(b) => anim.bone_world(&body, b),
            None => matrix,
        };
        let first = |kind: u8| anim.rig.model.shapes.iter().find(|s| s.kind == kind);
        ObjectFrame {
            matrix,
            body_centre: first(SHAPE_BODY).map(|sh| shape_point(sh, joint(sh))),
            point_9: first(SHAPE_POINT_9).map(|sh| shape_point(sh, joint(sh))),
            // FUN_0016ae90: FUN_0016aa18 on the shape's joint, and the length `(float)` of its +8.
            blade: first(SHAPE_WEAPON).map(|sh| (joint(sh), sh.offset[0])),
        }
    }
}

/// FUN_0016aa18 for the bone numbers with bit 0x4000 (the object's own frames), as a world matrix in
/// Bevy space:
/// - 0x4001: the object's matrix moved to the centre of its first type-1 shape (FUN_0016b7d0; without
///   one, the object's position);
/// - 0x4003 / 0x4004: the blade of the weapon of hand 0 / 1 moved along the blade's x axis by the
///   weapon's length (FUN_0016ae50 -> FUN_0016ae90, then `row3 += row0 * length`); 0x4005 / 0x4006
///   the blade unmoved; 0x4007 / 0x4008 moved by half the length. The weapon of a party member is its
///   weapon of that hand (FUN_00175df8: the list at +0xaf8); any other object is its own weapon
///   (FUN_0016ae90 on itself, both hands alike);
/// - 0x4009: the object's matrix moved to the centre of its first type-9 shape (without one, unmoved);
/// - 0x4000, 0x4002 and any other number: the object's matrix (+0x40 / +0x6b0, which the app does
///   not tell apart).
///
/// `weapons` holds the party member's weapons by hand (`None` for an empty hand); `party` says the
/// owner is a party member (+0x588 & 4). A missing weapon or blade gives the object's matrix (the game
/// would read through a null record there).
pub fn object_bone(bone: u16, owner: &ObjectFrame, weapons: &[Option<ObjectFrame>], party: bool) -> Mat4 {
    let at = |p: Option<Vec3>| {
        let mut m = owner.matrix;
        if let Some(p) = p {
            m.w_axis = p.extend(1.0);
        }
        m
    };
    match bone {
        0x4001 => at(owner.body_centre),
        0x4003..=0x4008 => {
            let hand = ((bone - 0x4003) & 1) as usize;
            let weapon = if party { weapons.get(hand).copied().flatten() } else { Some(*owner) };
            let Some((m, len)) = weapon.and_then(|w| w.blade) else { return owner.matrix };
            let along = match bone {
                0x4003 | 0x4004 => len,
                0x4005 | 0x4006 => 0.0,
                _ => len * WEAPON_MIDDLE,
            };
            m * Mat4::from_translation(Vec3::new(along, 0.0, 0.0))
        }
        0x4009 => at(owner.point_9),
        _ => owner.matrix,
    }
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

/// Where an effect's sound is placed: the bind's translation (fx space, the flipped game frame in
/// centimetres) in world metres. The request copies the position once (FUN_002c2760), so it is the
/// place at the effect's first tick.
pub fn effect_sound_position(bind: &Mat4) -> Vec3 {
    FLIP.transform_point3(bind.w_axis.truncate()) * SCALE
}

/// The request of an element's sound (FUN_001dd0d0 -> FUN_002c32b8, after the bind update and before the
/// destroy test, so an effect that ends on its first tick still sounds). A camera-space element
/// (flag 0x20000) keeps the unplaced request [inferred: its record holds view-space coordinates, which
/// the request's camera transform FUN_002c27b0 would apply a second time].
pub fn effect_sound_request(se: u32, apdx: Option<String>, flags: u32, bind: &Mat4) -> crate::audio::PlaySe {
    let req = crate::audio::PlaySe::number(se, apdx);
    if flags & ELEMENT_IN_CAMERA_SPACE != 0 { req } else { req.at(effect_sound_position(bind)) }
}

/// Game camera (X right, Y down, Z forward, centimetres) → fx world, from the Bevy camera.
fn game_camera(cam: Mat4) -> Mat4 {
    let (_, r, t) = cam.to_scale_rotation_translation();
    FLIP * Mat4::from_rotation_translation(r, t / SCALE) * FLIP
}

/// A vertex color of the effect (0x80 = 1.0) for the linear framebuffer. The GS multiplies the texel
/// by the vertex color in display (gamma) space (`(Ct * Cf) >> 7`); the app multiplies in linear space,
/// so the color channels go through 2.2 first (the HUD's `gamma22` convention). Without it an orange
/// vertex color such as Fire's (0x9e, 0x4b, 0x1e) is drawn pale tan. Alpha is the blend weight (As)
/// and stays linear.
fn gs_color(c: Vec4) -> [f32; 4] {
    let g = |v: f32| v.max(0.0).powf(2.2);
    [g(c.x), g(c.y), g(c.z), c.w]
}

fn build_mesh(b: &Batch) -> Mesh {
    let back = |p: Vec3| (FLIP.transform_point3(p) * SCALE).to_array();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, b.verts.iter().map(|v| back(v.pos)).collect::<Vec<_>>())
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_COLOR,
            VertexAttributeValues::Float32x4(b.verts.iter().map(|v| gs_color(v.color)).collect()),
        )
        .with_inserted_indices(Indices::U32(b.indices.clone()));
    if b.offsets.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, b.verts.iter().map(|v| v.uv).collect::<Vec<_>>());
    } else {
        // Distortion batch: the displacement (in metres) rides in UV 0 (x, y) and UV 1 (x = z),
        // the depth factor in UV 1 y.
        let off: Vec<[f32; 3]> = b.offsets.iter().map(|o| (FLIP.transform_vector3(*o) * SCALE).to_array()).collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, off.iter().map(|o| [o[0], o[1]]).collect::<Vec<_>>());
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, off.iter().map(|o| [o[2], b.depth_scale]).collect::<Vec<_>>());
    }
    mesh
}

/// `KH2_FXCOUNT=1`: once a second, how many effects are running (and which ids), to see leftovers.
pub fn fx_count(time: Res<Time>, effects: Query<&RunningEffect>, mut last: Local<u32>) {
    if std::env::var_os("KH2_FXCOUNT").is_none() {
        return;
    }
    let sec = time.elapsed_secs() as u32;
    if sec == *last {
        return;
    }
    *last = sec;
    let mut ids: Vec<u16> = effects.iter().map(|r| r.game_id).collect();
    ids.sort_unstable();
    info!("fx count: t={sec}s running effects {} ids {ids:?}", ids.len());
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

/// Shape kind of the lock-on point (target shapes, as `sora::target_points`).
pub const TARGET_SHAPE: u8 = 3;

/// The first shape of `kind` of a body, on its bone (`FUN_0016b558` + `FUN_001894d0`), in app cm.
pub fn body_shape_point(anim: &Animator, body: &GlobalTransform, kind: u8) -> Option<Vec3> {
    let s = anim.rig.model.shapes.iter().find(|s| s.kind == kind && (kind != TARGET_SHAPE || s.argument & 1 == 0))?;
    let m = match s.bone {
        Some(b) => anim.bone_world(body, b),
        None => body.to_matrix(),
    };
    let mut p = m.transform_point3(Vec3::from(s.offset)) / SCALE;
    p.y += s.lift;
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WP-AU6: the element sound request the effect tick writes. A world element is placed at its bind
    /// (so it falls with the camera distance), a camera-space element stays unplaced.
    #[test]
    fn effect_sound_requests_are_placed_unless_the_element_is_camera_space() {
        let bind = Mat4::from_translation(Vec3::new(300.0, -100.0, 50.0));
        let world = effect_sound_request(29, Some("obj/X.apdx".into()), 0, &bind);
        assert_eq!((world.bank, world.index), (0, 29));
        assert_eq!(world.apdx.as_deref(), Some("obj/X.apdx"));
        let pos = world.pos.expect("a world element is placed");
        assert!((pos - effect_sound_position(&bind)).length() < 1e-4);
        let cam = effect_sound_request(29, None, ELEMENT_IN_CAMERA_SPACE, &bind);
        assert_eq!(cam.pos, None);
        let far = crate::audio::placement(world.pos, Some(&Transform::IDENTITY));
        assert!(far.0 < 0x3fff, "a placed effect sound is not at full volume: {:#x}", far.0);
        assert_eq!(crate::audio::placement(cam.pos, Some(&Transform::IDENTITY)).0, 0x3fff);
    }

    /// WP-AU5: the sound of a fixed-placed effect plays where the effect is, in world metres.
    #[test]
    fn effect_sound_is_placed_at_the_bind() {
        let at = Vec3::new(3.0, 1.5, -2.0);
        let (sc, rot, t) = Mat4::from_translation(at).to_scale_rotation_translation();
        let bind = FLIP * Mat4::from_scale_rotation_translation(sc, rot, t / SCALE) * FLIP;
        let p = effect_sound_position(&bind);
        assert!((p - at).length() < 1e-3, "{p:?}");
    }

    /// Which effect files carry clash-spark elements (category bit 0x10 plus a target-bit): the lookup
    /// of `FUN_0018d6b0`, which runs before the party handler's system spark. Skips without a disc.
    #[test]
    fn clash_spark_elements_by_file() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        for name in ["obj/W_EX010.apdx", "obj/P_EX100.apdx", "obj/M_EX010.apdx", "obj/M_EX020.apdx"] {
            let Some(p) = load(&disc, name, 99, None) else {
                eprintln!("{name}: not loadable");
                continue;
            };
            let sparks: Vec<(u16, u64)> = p.pax.elements.iter().filter(|e| e.category & 0x10 != 0).map(|e| (e.id, e.category)).collect();
            let low: std::collections::BTreeSet<u64> = p.pax.elements.iter().map(|e| e.category & 0xffff_ffff).collect();
            eprintln!("{name}: {} elements, clash-category (0x10) elements: {:x?}, low category words present: {:x?}", p.pax.elements.len(), sparks, low);
            assert!(sparks.is_empty(), "{name}: has 0x10 elements, so FUN_0018d6b0 would spark first and the party handler's system spark would be skipped");
        }
    }

    /// The footstep dust of `FUN_001b1c90` on the disc: the area effect file of tt02 (the PAX of its
    /// map) holds dust elements for the floor materials with a dust class, and `foot_dust` starts
    /// the one the mask selects at the ground under the foot (a probe from 50 above to 100 below),
    /// and nothing where there is no ground there or the material has no dust class.
    #[test]
    fn foot_dust_starts_the_area_effect_at_the_ground_under_the_foot() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rules = DustRules::from_exe(&disc.read_root("SLUS_210.05").expect("exe")).expect("dust tables");
        let mut pick = None;
        for area in ["tt25", "tt02", "tt04", "tt05", "tt01", "tt10", "tt00"] {
            let Some(pax) = load(&disc, &format!("map/us/{area}.map"), AREA_FX_ID_BASE, None) else { continue };
            let cats: std::collections::BTreeSet<u64> = pax.pax.elements.iter().map(|e| e.category).collect();
            eprintln!("{area}: {} elements, categories {cats:x?}", pax.pax.elements.len());
            if pick.is_none() && pax.pax.elements.iter().any(|e| e.category != 0) {
                pick = Some((area, pax));
            }
        }
        let Some((area, pax)) = pick else {
            eprintln!("no tt area has a categorised dust element in its map effect file: nothing to start");
            return;
        };
        let pax = Arc::new(pax);
        // Which (material, kind) pairs of the area have an element.
        let mut found = Vec::new();
        for material in 0..32u8 {
            for kind in 0..=0x13u16 {
                if let Some(mask) = rules.mask(material, kind)
                    && let Some(id) = element_by_category(&pax, mask)
                {
                    found.push((material, kind, id));
                }
            }
        }
        eprintln!("{area} dust elements (material, kind, id): {found:?}");
        if found.is_empty() {
            eprintln!("no tt area's map effect file has an element whose category selects footstep dust (checked every material and kind 0..=0x13): nothing to start");
            return;
        }
        let &(material, kind, id) = found.iter().find(|f| f.1 == 8).or(found.first()).expect("a dust element");
        // A floor under the origin, 20 m square.
        let floor = kh2_model::Mesh {
            positions: vec![[-1000.0, 0.0, -1000.0], [0.0, 0.0, 1000.0], [1000.0, 0.0, -1000.0]],
            indices: vec![0, 1, 2],
            ..Default::default()
        };
        let mut app = App::new();
        app.add_message::<FootDust>()
            .insert_resource(AreaFx { area: Some(area.into()), pax: Some(pax), rules: Some(rules) })
            .insert_resource(crate::world::World::from_meshes(&[floor]))
            .add_systems(Update, foot_dust);
        let request = |at: Vec3, material: u8, kind: u16| FootDust { kind, at, facing: Vec3::X, material, why: "test" };
        let running = |app: &mut App| app.world_mut().query::<&RunningEffect>().iter(app.world()).map(|r| r.game_id).collect::<Vec<_>>();
        // 30 cm above the floor: the probe finds it and the element starts.
        app.world_mut().write_message(request(Vec3::new(1.0, 0.3, 2.0), material, kind));
        app.update();
        assert_eq!(running(&mut app), vec![id], "material {material} kind {kind}");
        // 2 m above the floor the probe (50 up, 100 down) finds nothing: no dust.
        app.world_mut().write_message(request(Vec3::new(1.0, 2.0, 2.0), material, kind));
        // A material without a dust class: no dust.
        app.world_mut().write_message(request(Vec3::new(1.0, 0.3, 2.0), 0, kind));
        app.update();
        assert_eq!(running(&mut app), vec![id], "only the first request started an effect");
    }

    /// Which areas carry footstep dust: the map effect file of tt25 holds class-2 elements (ground
    /// material 3) for the walk and run kinds 0 to 3, al02 for the start-to-move kinds 10 and 11
    /// (class 2), al09 for the stop kinds 8 and 9 (class 9, material 13), while the maps of tt02,
    /// tt04 and tt05 hold none, so the original starts no dust on their floors either
    /// (`FUN_001db9e0` finds no element there). Skipped without the disc.
    #[test]
    fn dust_elements_follow_the_area_effect_files_on_the_disc() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rules = DustRules::from_exe(&disc.read_root("SLUS_210.05").expect("exe")).expect("dust tables");
        assert_eq!((rules.class(3), rules.class(13)), (2, 9), "materials 3 and 13 are dust classes 2 and 9");
        let ids = |area: &str, material: u8| {
            let pax = load(&disc, &format!("map/us/{area}.map"), AREA_FX_ID_BASE, None).expect("map effect file");
            (0..=0x13u16).map(|k| rules.mask(material, k).and_then(|m| element_by_category(&pax, m))).collect::<Vec<_>>()
        };
        let has = |area: &str, material: u8, kinds: &[usize]| {
            let found = ids(area, material);
            kinds.iter().all(|&k| found[k].is_some())
        };
        assert!(has("tt25", 3, &[0, 1, 2, 3]), "tt25 material 3 kinds 0 to 3");
        assert!(has("al02", 3, &[10, 11]), "al02 material 3 kinds 10 and 11");
        assert!(has("al09", 13, &[8, 9]), "al09 material 13 kinds 8 and 9");
        for area in ["tt02", "tt04", "tt05"] {
            for material in 0..32u8 {
                assert!(ids(area, material).iter().all(Option::is_none), "{area} material {material}");
            }
        }
    }

    #[test]
    fn hit_rotation_follows_the_game_axes() {
        // Straight along Bevy +Z is game -Z: yaw atan2(0, -1) = pi, no pitch.
        let r = hit_rotation(Vec3::Z);
        assert!((r[1].abs() - std::f32::consts::PI).abs() < 1e-5 && r[0].abs() < 1e-5 && r[2] == 0.0);
        // Along +X: yaw pi/2 in both frames.
        let r = hit_rotation(Vec3::X);
        assert!((r[1] - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        // Straight up in Bevy is down in the game: pitch atan2(1, 0) = pi/2 about X.
        let r = hit_rotation(Vec3::Y);
        assert!((r[0] + std::f32::consts::FRAC_PI_2).abs() < 1e-5 || (r[0] - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }

    /// FUN_0016aa18's object frames: 0x4003 is the hand-0 weapon's blade moved along its x axis by the
    /// length, 0x4005 the blade itself, 0x4007 half way; 0x4001 / 0x4009 the object's matrix moved to
    /// its type-1 / type-9 shape centres; an object that is not a party member is its own weapon.
    #[test]
    fn object_bones_follow_the_game() {
        let rot = Mat4::from_rotation_y(0.5) * Mat4::from_rotation_x(0.3);
        let blade_m = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0)) * rot;
        let sword = ObjectFrame { matrix: Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0)), body_centre: None, point_9: None, blade: Some((blade_m, -78.0)) };
        let owner = ObjectFrame {
            matrix: Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)) * Mat4::from_rotation_y(1.0),
            body_centre: Some(Vec3::new(10.0, 0.85, 0.0)),
            point_9: Some(Vec3::new(10.0, 1.05, 0.0)),
            blade: None,
        };
        let hands = [Some(sword), None];
        let pos = |m: Mat4| m.w_axis.truncate();
        // The tip: the blade's own x axis times the length (negative: toward -x).
        let tip = object_bone(0x4003, &owner, &hands, true);
        assert!((pos(tip) - (blade_m.w_axis.truncate() - 78.0 * blade_m.x_axis.truncate())).length() < 1e-4);
        assert_eq!(object_bone(0x4005, &owner, &hands, true), blade_m);
        let mid = object_bone(0x4007, &owner, &hands, true);
        assert!((pos(mid) - (blade_m.w_axis.truncate() - 39.0 * blade_m.x_axis.truncate())).length() < 1e-4);
        // Hand 1 is empty: the object's matrix.
        assert_eq!(object_bone(0x4004, &owner, &hands, true), owner.matrix);
        // 0x4001 and 0x4009 keep the rotation and take the shape centre.
        let b = object_bone(0x4001, &owner, &hands, true);
        assert_eq!(pos(b), Vec3::new(10.0, 0.85, 0.0));
        assert_eq!(b.x_axis, owner.matrix.x_axis);
        assert_eq!(pos(object_bone(0x4009, &owner, &hands, true)), Vec3::new(10.0, 1.05, 0.0));
        assert_eq!(object_bone(0x4000, &owner, &hands, true), owner.matrix);
        assert_eq!(object_bone(0x4002, &owner, &hands, true), owner.matrix);
        // Not a party member: its own blade, both hands alike; without one, its own matrix.
        let enemy = ObjectFrame { blade: Some((blade_m, 10.0)), ..owner };
        assert_eq!(object_bone(0x4003, &enemy, &[], false), object_bone(0x4004, &enemy, &[], false));
        assert_eq!(object_bone(0x4003, &owner, &[], false), owner.matrix);
        // No shape of the type: the object's own place.
        let bare = ObjectFrame { body_centre: None, point_9: None, ..owner };
        assert_eq!(object_bone(0x4001, &bare, &[], true), owner.matrix);
    }

    /// The disc's data behind the cast effects: the keyblade has a type-15 shape on a bone with a
    /// length along -x; Sora has type-1 and type-9 shapes on his own matrix; the charge and cast
    /// elements (0 and 1) of the four level-1 spells are bound to 0x4003, the weapon of hand 0.
    #[test]
    fn cast_effects_bind_to_the_blade_on_the_disc() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let kb = kh2_model::mdlx(&disc.read("obj/W_EX010.mdlx").expect("keyblade")).expect("model");
        let blade = kb.shapes.iter().find(|s| s.kind == SHAPE_WEAPON).expect("type-15 shape");
        eprintln!("keyblade blade: bone {:?} length {}", blade.bone, blade.offset[0]);
        assert!(blade.bone.is_some() && blade.offset[0] < 0.0, "a bone and a length along -x");
        let sora = kh2_model::mdlx(&disc.read("obj/P_EX100.mdlx").expect("sora")).expect("model");
        for kind in [SHAPE_BODY, SHAPE_POINT_9] {
            let s = sora.shapes.iter().find(|s| s.kind == kind).expect("shape");
            eprintln!("sora type {kind}: bone {:?} lift {}", s.bone, s.lift);
            assert!(s.bone.is_none() && s.lift > 0.0, "on the object's matrix, above its feet");
        }
        for name in ["magic/FIRE_1.mag", "magic/BLIZZARD_1.mag", "magic/THUNDER_1.mag", "magic/CURE_1.mag"] {
            let p = load(&disc, name, 1, None).expect(name);
            for id in [0u16, 1] {
                let e = p.pax.elements_for(id).next().expect("element");
                assert_eq!(e.bone, 0x4003, "{name} element {id}");
            }
        }
    }

    /// `FUN_001c09f8`: the ring holds the last 8 sparks; the 9th takes the slot of the 1st and pushes it out,
    /// the 10th the 2nd, and so on, in order.
    #[test]
    fn spark_ring_pushes_out_the_oldest_after_eight() {
        let mut w = World::new();
        let ents: Vec<Entity> = (0..12).map(|_| w.spawn_empty().id()).collect();
        let mut ring = SparkRing::default();
        for (i, e) in ents.iter().enumerate() {
            let out = ring.push(*e);
            if i < SPARK_RING_SLOTS {
                assert_eq!(out, None, "push {i}: the ring still has a free slot");
            } else {
                assert_eq!(out, Some(ents[i - SPARK_RING_SLOTS]), "push {i}: the oldest spark leaves");
            }
        }
    }

    /// `ring_push` marks exactly the pushed-out spark `Evicted` (which `tick` fades over 4 frames).
    #[test]
    fn ring_push_marks_the_pushed_out_spark() {
        let mut app = App::new();
        let ents: Vec<Entity> = (0..9).map(|_| app.world_mut().spawn_empty().id()).collect();
        let mut ring = SparkRing::default();
        let mut commands = app.world_mut().commands();
        for e in &ents {
            ring_push(&mut commands, &mut ring, Some(*e));
        }
        ring_push(&mut commands, &mut ring, None);
        app.world_mut().flush();
        let marked: Vec<bool> = ents.iter().map(|e| app.world().entity(*e).contains::<Evicted>()).collect();
        assert_eq!(marked, [true, false, false, false, false, false, false, false, false]);
    }

    /// [inferred] rule `ring_spark_expired`: a looping ring spark is faded once its program life is over (a
    /// 60-frame life, two ticks a program frame, starts the 4-tick fade at tick 120, gone at 123). Since WP-7Bc the clash spark 0xd itself is
    /// not looping (its DPX entry is found by number, `dpx_effect`): it ends by itself within its 24-frame life,
    /// so the rule does not touch it. Skips the disc part without a disc.
    #[test]
    fn lone_clash_spark_ends_by_itself() {
        let life = 60 * kh2_fx::pax::TICK;
        let start = (0..1000).find(|t| ring_spark_expired(*t as f32, life)).expect("expires");
        let mut level = 1.0f32;
        let mut gone = None;
        for t in start..start + 20 {
            level -= 1.0 / SPARK_EVICT_FADE;
            if level <= 0.0 {
                gone = Some(t);
                break;
            }
        }
        assert_eq!(start, 120);
        assert_eq!(gone, Some(123), "a looping ring spark is gone about 124 ticks after it started");
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let common = disc.read("effect/texcommon.dpd").and_then(|b| Dpd::parse(&b).ok());
        let lib = load(&disc, "00effect.bar", 1, common.as_ref()).expect("00effect.bar");
        let el = lib.pax.elements_for(0xd).next().expect("element");
        let fx = dpx_effect(&lib.pax, el.effect).expect("dpx effect");
        let mut e = kh2_fx::Effect::new(lib.dpds[fx.dpd].clone(), fx.dpd, fx.particle_data).expect("effect");
        assert_eq!(e.particle_data().flags & 1, 0, "0xd does not loop");
        let life = e.particle_data().life / kh2_fx::pax::TICK;
        let done = (0..600).find(|_| {
            e.tick(&kh2_fx::Env::default());
            e.done
        });
        assert_eq!((life, done), (24, Some(23)), "0xd ends within its life");
    }

    /// `FUN_001db5f0` -> `FUN_001d9720` finds an element's DPX entry by the entry's number, not its place. In
    /// `00effect.bar` the 14 entries are numbered 0..5 and 11..18, so the elements 0xb..0x13 resolve only by
    /// number (by place 0xb, 0xc and 0xd got the entries numbered 16, 17 and 18, and 0xe..0x13 nothing). With
    /// the game's lookup the guard-bounce sparks 0xb, 0xc and the clash spark 0xd all end by themselves within
    /// half a second; 0x13 (the summon arrival) is the looping 60-frame entry. Skips without a disc.
    #[test]
    fn system_elements_resolve_by_dpx_number() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let common = disc.read("effect/texcommon.dpd").and_then(|b| Dpd::parse(&b).ok());
        let lib = load(&disc, "00effect.bar", 1, common.as_ref()).expect("00effect.bar");
        let run = |id: u16, ticks: usize| {
            let el = lib.pax.elements_for(id).next().expect("element");
            let fx = dpx_effect(&lib.pax, el.effect).expect("dpx effect");
            let mut e = kh2_fx::Effect::new(lib.dpds[fx.dpd].clone(), fx.dpd, fx.particle_data).expect("effect");
            let looping = e.particle_data().flags & 1 != 0;
            (looping, (0..ticks).find(|_| {
                e.tick(&kh2_fx::Env::default());
                e.done
            }))
        };
        for el in &lib.pax.elements {
            let f = dpx_effect(&lib.pax, el.effect).expect("every system element has its DPX entry");
            eprintln!("element {:#x} flags {:#x}: DPX number {} = entry {} (particle data {})", el.id, el.flags, el.effect, lib.pax.effects.iter().position(|x| x.number == f.number).unwrap_or(99), f.particle_data);
        }
        let numbers: Vec<u32> = lib.pax.effects.iter().map(|f| f.number).collect();
        assert_eq!(numbers, [0, 1, 2, 3, 4, 5, 11, 12, 13, 14, 15, 16, 17, 18]);
        assert_eq!(run(0xb, 600), (false, Some(18)), "0xb ends by itself");
        assert_eq!(run(0xc, 600), (false, Some(14)), "0xc ends by itself");
        assert_eq!(run(0xd, 600), (false, Some(23)), "0xd ends by itself");
        assert_eq!(run(0x13, 600), (true, None), "0x13 loops");
        // The other files the app starts effects from: is the number the place there (then nothing changed)?
        for name in ["obj/W_EX010.apdx", "obj/P_EX100.apdx", "obj/M_EX010.apdx", "obj/M_EX020.apdx", "magic/FIRE_1.mag", "magic/BLIZZARD_1.mag", "magic/THUNDER_1.mag", "magic/CURE_1.mag", "map/us/tt02.map", "obj/PRIZE.mdlx"] {
            if let Some(p) = load(&disc, name, 99, None) {
                let same = p.pax.effects.iter().enumerate().all(|(i, f)| f.number == i as u32);
                eprintln!("{name}: {} DPX entries, numbers equal places: {same}", p.pax.effects.len());
            }
        }
    }

    /// The owner-bound system effect (`FUN_001db868` with an owner, the pickup glow's system effect 6):
    /// it starts bound to the owner, carries its handle, is not filed in the spark ring; `StopEffect`
    /// ends only the effect with that handle (`FUN_001d9f48`: loop end, the effect runs out), and a kill
    /// (`FUN_001da028`) removes it at once. Skips without a disc.
    #[test]
    fn a_bound_system_effect_carries_its_handle_and_stops_on_request() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let common = disc.read("effect/texcommon.dpd").and_then(|b| Dpd::parse(&b).ok());
        let system = Arc::new(load(&disc, "00effect.bar", 1, common.as_ref()).expect("00effect.bar"));
        let el = system.pax.elements_for(6).next().expect("system element 6");
        let pd = dpx_effect(&system.pax, el.effect).map(|f| system.dpds[f.dpd].particles[f.particle_data as usize].flags);
        eprintln!("system element 6: bone {:#x} flags {:#x} effect {} sound {} program flags {pd:?}", el.bone, el.flags, el.effect, el.se);
        let ids: Vec<u16> = system.pax.elements.iter().map(|e| e.id).collect();
        eprintln!("system element ids: {ids:x?}");
        let e10 = system.pax.elements_for(0x10).next().expect("element 0x10");
        let f10 = dpx_effect(&system.pax, e10.effect);
        eprintln!(
            "system element 0x10: bone {:#x} flags {:#x} effect {} (of {}) {:?}",
            e10.bone,
            e10.flags,
            e10.effect,
            system.pax.effects.len(),
            f10.map(|f| (f.dpd, f.particle_data, kh2_fx::Effect::new(system.dpds[f.dpd].clone(), f.dpd, f.particle_data).is_some()))
        );
        // FUN_001d9720 finds the DPX entry by its number: the level-up effect 0x10 and the ineffective-hit effect
        // 0x12 name numbers past the entry count, and both resolve and build.
        let numbers: Vec<u32> = system.pax.effects.iter().map(|f| f.number).collect();
        eprintln!("system DPX entry numbers: {numbers:?}");
        for id in [6u16, 0x10, 0x12] {
            let el = system.pax.elements_for(id).next().expect("element");
            let f = dpx_effect(&system.pax, el.effect).unwrap_or_else(|| panic!("element {id:#x}: DPX number {}", el.effect));
            assert!(kh2_fx::Effect::new(system.dpds[f.dpd].clone(), f.dpd, f.particle_data).is_some(), "element {id:#x} builds");
        }
        let mut app = App::new();
        app.add_message::<StartBoundSystemEffect>()
            .add_message::<StopEffect>()
            .init_resource::<SparkRing>()
            .insert_resource(EffectLib { keyblade: system.clone(), system, objects: HashMap::new() })
            .add_systems(Update, (start_bound_system_requests, stop_requests).chain());
        let owner = app.world_mut().spawn_empty().id();
        let (glow, other) = (FxHandle::new(), FxHandle::new());
        assert_ne!(glow, other);
        app.world_mut().write_message(StartBoundSystemEffect { id: 6, owner, handle: glow });
        app.world_mut().write_message(StartBoundSystemEffect { id: 6, owner, handle: other });
        app.update();
        let state = |app: &mut App| {
            let mut v: Vec<(u32, u16, bool, bool, bool)> = app
                .world_mut()
                .query::<(&FxHandle, &RunningEffect, Has<RingSpark>)>()
                .iter(app.world())
                .map(|(h, r, ring)| (h.0, r.game_id, matches!(r.bind, Bind::Owner(o) if o == owner), r.effect.is_ending(), ring))
                .collect();
            v.sort();
            v
        };
        assert_eq!(state(&mut app), vec![(glow.0, 6, true, false, false), (other.0, 6, true, false, false)]);
        app.world_mut().write_message(StopEffect { handle: glow, kill: false });
        app.update();
        assert_eq!(state(&mut app), vec![(glow.0, 6, true, true, false), (other.0, 6, true, false, false)], "only the glow's effect ends");
        app.world_mut().write_message(StopEffect { handle: other, kill: true });
        app.update();
        assert_eq!(state(&mut app), vec![(glow.0, 6, true, true, false)], "the killed effect is gone at once");
        // No element with the id: nothing starts (FUN_001db868 returns 0).
        app.world_mut().write_message(StartBoundSystemEffect { id: 0x7fff, owner, handle: FxHandle::new() });
        app.update();
        assert_eq!(state(&mut app).len(), 1);
    }

    #[derive(Resource)]
    struct TestWeapons(Vec<Entity>);

    /// Only a weapon that turns from hidden to shown starts its appearance effect (a new list entry counts
    /// as hidden before).
    #[test]
    fn weapons_appeared_reports_hidden_to_shown_only() {
        assert_eq!(weapons_appeared(&[false, false], &[true, false]), vec![0]);
        assert_eq!(weapons_appeared(&[true, false], &[true, true]), vec![1]);
        assert_eq!(weapons_appeared(&[true], &[true]), Vec::<usize>::new());
        assert_eq!(weapons_appeared(&[true], &[false]), Vec::<usize>::new());
        assert_eq!(weapons_appeared(&[], &[true]), vec![0]);
    }

    /// The keyblade's own effect file has an element 0 that builds, and the system starts it bound to the weapon
    /// when its visibility turns on, once. Skips without a disc.
    #[test]
    fn the_keyblade_appearance_effect_starts_when_the_weapon_is_shown() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let common = disc.read("effect/texcommon.dpd").and_then(|b| Dpd::parse(&b).ok());
        let kb = Arc::new(load(&disc, "obj/W_EX010.apdx", 2, common.as_ref()).expect("W_EX010.apdx"));
        let ids: Vec<u16> = kb.pax.elements.iter().map(|e| e.id).collect();
        eprintln!("keyblade element ids: {ids:?}");
        let el = kb.pax.elements_for(EFFECT_WEAPON_APPEAR).next().expect("keyblade element 0");
        let f = dpx_effect(&kb.pax, el.effect).expect("DPX entry");
        eprintln!("keyblade element 0: bone {:#x} flags {:#x} effect {} sound {}", el.bone, el.flags, el.effect, el.se);
        assert!(kh2_fx::Effect::new(kb.dpds[f.dpd].clone(), f.dpd, f.particle_data).is_some(), "element 0 builds");
        let mut app = App::new();
        app.insert_resource(EffectLib { keyblade: kb.clone(), system: kb, objects: HashMap::new() }).add_systems(Update, |mut c: Commands, lib: Res<EffectLib>, w: Res<TestWeapons>, vis: Query<&Visibility>, mut prev: Local<Vec<bool>>| appear_step(&mut c, &lib, &w.0, &vis, &mut prev));
        let w = app.world_mut().spawn(Visibility::Hidden).id();
        app.insert_resource(TestWeapons(vec![w]));
        let count = |app: &mut App| app.world_mut().query::<&RunningEffect>().iter(app.world()).filter(|r| r.game_id == EFFECT_WEAPON_APPEAR).count();
        app.update();
        assert_eq!(count(&mut app), 0, "hidden: nothing");
        *app.world_mut().get_mut::<Visibility>(w).unwrap() = Visibility::Inherited;
        app.update();
        assert_eq!(count(&mut app), 1, "shown: one effect");
        app.update();
        assert_eq!(count(&mut app), 1, "still shown: no second one");
    }
}
