//! Townspeople: the class-5 objects of an area's `map` program groups (WP-E2, docs/systems/events-movies/npc-savepoints.md
//! section 3 and the E2 stage).
//!
//! The game makes an object of `00objentry` type 15 (`N_EX500_RTN`, `N_EX510_RTN`, ...) through
//! `FUN_00189858` (object class 5, vtable 0x35ced8, 0xb40 bytes): it carries two "attention"
//! followers at +0xa40 and +0xaa0 (`FUN_0018b0d8`), a bit array at +0xb04 filled from the spawn
//! record's word +0x24 (`FUN_001899d0`, bits 0..6), a parameter block at +0xb10 (host 5.33) and
//! the walk path at +0x508 (`FUN_00183c30`: a `WalkPath` of the object's own spawn group found by
//! its id, `FUN_00199cc0`). The model's own BDX (`rtn_default`) runs on it like an enemy script:
//! `NpcHost` answers the group 5 library (the NPC routine library of docs/systems/script-host/contract.md) and the path
//! hosts of group 1 (1.177 set path, 1.178 path velocity) and hands everything else to the enemy
//! host (`BevyHost`), so the per-run host-call table of `KH2_BDXLOG=1` covers these objects too.
//!
//! Spawning: when the area record's run changes, every entity of type 15 in the groups the `map`
//! program loaded is spawned at its record's point (on the collision floor under it), facing
//! `PI - rotation y` (`FUN_00198220`), as an `AreaPart` so the area switch removes it. The walk
//! paths and return parameters of its group are kept with it (the class-5 code reads them through
//! the object's group pointer +0x818).
//!
//! Log: `KH2_NPCLOG=1` prints the spawns, the path assignments and the script's mode changes;
//! `KH2_BDXLOG=1` the host calls as for enemies. `KH2_NO_NPC=1` spawns none.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_bdx::host::{Arg, Call, Host, Value};
use kh2_bdx::{Program, Runtime};
use kh2_tables::tables as t;

use crate::area::{app_yaw, game_to_app, AreaRecord};
use crate::assets::{AreaPart, Disc};
use crate::enemy_ai::AiTrace;
use crate::enemy_host::{BevyHost, Env, St};
use crate::enemy_obj::{from_game, to_game, Obj, OBJ_SIZE, PLAYER, SELF_BASE};
use crate::reaction_parts as parts;
use crate::schedule::Sim;
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::sora::Sora;
use crate::world::World;
use crate::SCALE;
pub use crate::reaction_cmd::{part_offer, PartOffer, Talk, TalkRun};
pub use crate::shared::reaction::{EXECUTE_EXAMINE, EXECUTE_TALK};

/// `00objentry` ObjectType of the townspeople (FUN_0016e8c0 case 0xf -> FUN_00189858).
pub const OBJECT_TOWNSPERSON: u8 = 15;

/// Script handles of the NPCs: past the enemies' (below 0x1000) and the friends' (0x1000..).
const HANDLE_BASE: u32 = 0x2001;

pub struct NpcPlugin;

impl Plugin for NpcPlugin {
    fn build(&self, app: &mut App) {
        use crate::hud::hud_menu::mirror_reaction;
        use crate::schedule::{sim_running, Frame};
        app.init_resource::<NpcAssets>()
            .init_resource::<NpcSpawned>()
            .init_resource::<Talk>()
            .init_resource::<crate::hud::hud_msgwin::ScriptWindow>()
            .add_plugins(crate::savepoint::SavePointPlugin)
            .add_systems(FixedUpdate, (npc_spawn, npc_update).chain().in_set(Sim::Enemy).after(crate::enemy::spawn_waves))
            // After the reaction manager's own offer and accept (`reaction_cmd`, `Sim::Effects`).
            .add_systems(FixedUpdate, (talk_offer, talk_step).chain().after(Sim::Effects).before(Sim::Sounds).run_if(sim_running))
            .add_systems(Update, talk_lock_pad.after(Frame::Pad).before(Frame::Hud))
            .add_systems(Update, (talk_show_offer.before(mirror_reaction), talk_hide_offer.after(mirror_reaction)).in_set(Frame::Hud));
    }
}

fn npclog() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_NPCLOG").is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// The townspeople's models and scripts by model name (loaded once each), the `objentry` rows and
/// the executable's host table.
#[derive(Resource, Default)]
pub(crate) struct NpcAssets {
    rigs: HashMap<String, Option<Arc<Rig>>>,
    scripts: HashMap<String, Option<Arc<Program>>>,
    objs: Vec<t::Objentry>,
    table: Option<kh2_bdx::HostTable>,
    /// The collision records of each model (BAR entry 23, the game's object +0x8d8).
    parts: HashMap<String, Option<Arc<parts::Table>>>,
    /// The player's own body volume (record type 1 of `P_EX100`), which `FUN_0018aa08` tests
    /// against a townsperson's type-5 part.
    player_body: Option<parts::Rec>,
    /// The player's record of type 0x11, the point `FUN_0016bbe8` gives for him (a bone of `P_EX100`):
    /// what a townsperson's head looks at.
    player_point: Option<parts::Rec>,
    loaded: bool,
}

impl NpcAssets {
    fn load_once(&mut self, disc: &Disc) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.table = disc.read_root("SLUS_210.05").and_then(|exe| kh2_bdx::HostTable::from_exe(&exe));
        if let Ok(mut a) = disc.0.lock() {
            self.objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default();
        }
        let player = disc.read("obj/P_EX100.mdlx").and_then(|f| parts::Table::from_mdlx(&f));
        self.player_body = player.as_ref().and_then(|t| t.first(parts::TYPE_BODY).copied());
        self.player_point = player.as_ref().and_then(|t| t.first(TYPE_POINT).copied());
    }

    fn parts(&mut self, disc: &Disc, model: &str) -> Option<Arc<parts::Table>> {
        self.parts.entry(model.to_string()).or_insert_with(|| disc.read(&format!("obj/{model}.mdlx")).and_then(|f| parts::Table::from_mdlx(&f)).map(Arc::new)).clone()
    }

    /// The object's model and its motion set: the objentry's AnimationName (a shopkeeper such as `N_EX650_TT_ITEM_RTN`
    /// shares another townsperson's set), the model's own name when that is empty.
    fn rig(&mut self, disc: &Disc, model: &str, anim: &str) -> Option<Arc<Rig>> {
        let anim = Some(anim.trim_end_matches(".mset")).filter(|a| !a.is_empty()).unwrap_or(model).to_string();
        self.rigs
            .entry(model.to_string())
            .or_insert_with(|| {
                let m = disc.read(&format!("obj/{model}.mdlx")).and_then(|d| kh2_model::mdlx(&d))?;
                let s = disc.read(&format!("obj/{anim}.mset")).and_then(|d| kh2_anim::Mset::parse(&d))?;
                let apdx = Some(format!("obj/{model}.apdx")).filter(|a| disc.read(a).is_some());
                Some(Arc::new(Rig { model: m, mset: s, apdx }))
            })
            .clone()
    }

    fn script(&mut self, disc: &Disc, model: &str) -> Option<Arc<Program>> {
        self.scripts
            .entry(model.to_string())
            .or_insert_with(|| {
                let file = disc.read(&format!("obj/{model}.mdlx"))?;
                let entry = kh2_bar::parse(&file).ok()?.into_iter().find(|e| e.kind == 3)?.data.to_vec();
                match Program::parse(&entry) {
                    Ok(p) => Some(Arc::new(p)),
                    Err(e) => {
                        warn!("npc: {model}: script not readable: {e}");
                        None
                    }
                }
            })
            .clone()
    }
}

/// Which area run the townspeople were spawned for.
#[derive(Resource, Default)]
struct NpcSpawned {
    area: Option<String>,
    serial: u32,
}

/// What the class-5 object keeps of its spawn: the group's walk paths and return parameters
/// (reached through the object's group pointer +0x818 in the game) and its own record.
pub struct NpcGroup {
    pub tag: String,
    /// The group header's kind (+0) and id (+2); host 1.170 answers the id.
    pub kind: u16,
    pub id: u16,
    pub paths: Vec<kh2_ard::WalkPath>,
    /// The return parameters (16 bytes each): for a kind-6 group the class reads them as its
    /// behaviour entries (`FUN_00199ca0` hands them out only then).
    pub returns: Vec<[u8; 0x10]>,
    /// The group's event activators: for a kind-6 group the class-5 code reads them as the
    /// "regions" a townsperson walks through (`FUN_00198750` makes a volume of every one of them,
    /// `FUN_00199d68` finds the one a point is in).
    pub activators: Vec<kh2_ard::Activator>,
}

impl NpcGroup {
    /// `FUN_00199d68(group, point)`: for a kind-6 group (header byte 0), the first of the group's
    /// volumes (in record order, `FUN_00198750` appends them) whose record halfword +0x2c is 0 and
    /// that contains the point (the volume's own test, vtable slot +8: box, cylinder or always);
    /// `None` for other groups or when no volume contains it. `p` is in the game's space.
    pub fn region_at(&self, p: [f32; 3]) -> Option<usize> {
        if self.kind & 0xff != 6 {
            return None;
        }
        self.activators.iter().position(|a| u16::from_le_bytes([a.raw[0x2c], a.raw[0x2d]]) == 0 && a.contains(p))
    }
}

/// `FUN_001d8c18(volume, point)`: the point's place across the volume along its own x axis, 0 at
/// -x extent and 1 at +x extent: the point is taken into the volume's frame (its inverse matrix at
/// +0x10, `FUN_001d8600`) and `(x + sx) / (2 sx)` is answered with `sx` the record's +0x10
/// (volume +0x50). `p` is in the game's space.
pub fn region_coord(a: &kh2_ard::Activator, p: [f32; 3]) -> f32 {
    let sx = a.scale[0];
    (a.local(p)[0] + sx) / (sx + sx)
}

/// The game's `rand()` (`FUN_003117a8`) as the class-5 code uses it; a per-object xorshift stands in
/// for the one global generator [approx: the game's sequence is not reproduced, only the ranges].
#[derive(Clone, Debug)]
pub struct Rng(pub u32);

/// `DAT_003780e0` / `DAT_003780e4`: the divisor that turns `rand()` into a fraction.
const RAND_DIV: f32 = 2_147_483_520.0;

impl Rng {
    fn next(&mut self) -> u32 {
        let mut x = self.0.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x >> 1
    }

    /// `FUN_001063b8(x)`: `x * rand() / DAT_003780e4`, a float in [0, x].
    pub fn frac(&mut self, x: f32) -> f32 {
        x * self.next() as f32 / RAND_DIV
    }

    /// `FUN_00106408(lo, hi)`: `lo + FUN_001063b8(hi - lo)`.
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.frac(hi - lo)
    }

    /// `FUN_00106308(n)`: `n * rand() / DAT_003780e0` truncated to an integer (0..n).
    pub fn below(&mut self, n: u32) -> u32 {
        (n as f32 * self.next() as f32 / RAND_DIV) as u32
    }
}

/// `FUN_0018b1e0`: the motion number of a behaviour entry's byte 1 (a jump table of 14 entries at
/// 0x37ab9c): 0 and 1 give 0, 2 gives 0x11f, 3 gives 0, 4..13 give the index plus 0x117, higher
/// values 0.
pub fn entry_motion(k: u8) -> i32 {
    match k {
        2 => 0x11f,
        4..=13 => k as i32 + 0x117,
        _ => 0,
    }
}

/// A townsperson in the world: its body and the state the script does not own.
#[derive(Component)]
pub struct Npc {
    pub body: Entity,
    pub yaw: f32,
    pub grounded: bool,
    /// The entity record (position, rotation, the spawn-record bytes).
    pub record: kh2_ard::Entity,
    /// The model's collision records (the type-0 and type-5 parts the class-5 code tests).
    pub parts: Option<Arc<parts::Table>>,
    /// The talk reader kept between talks (the object's +0x504, `FUN_001a53f8`).
    pub talk: kh2_ard::libretto::Reader,
}

/// The script runtime of one townsperson.
#[derive(Component)]
pub struct NpcBrain {
    rt: Runtime,
    obj: Obj,
    tag: String,
    flags124: bool,
    fields: NpcFields,
    started: bool,
    errors_seen: usize,
    mode: Option<String>,
    last_frame: f32,
    visible: bool,
    /// A mode the native code switches the script to before its next tick (`FUN_00186db0` from
    /// `FUN_00189ff0`: "after_talk" when the talk ends), if the script has it (`FUN_001870d8`).
    pending_mode: Option<&'static str>,
}

/// Spawns the townspeople of the loaded area's `map` program when the area run changes.
#[allow(clippy::too_many_arguments)]
fn npc_spawn(
    mut commands: Commands,
    disc: Res<Disc>,
    rec: Res<AreaRecord>,
    mut spawned: ResMut<NpcSpawned>,
    mut assets: ResMut<NpcAssets>,
    world: Option<Res<World>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut sora: Query<(&mut Transform, &mut Sora)>,
    mut talk: ResMut<Talk>,
    mut cam: ResMut<crate::camera::CamEvent>,
) {
    let Some(run) = rec.run.as_ref() else { return };
    if spawned.area.as_deref() == Some(run.name.as_str()) {
        return;
    }
    // The collision world is inserted by `load_area` (deferred: present from the frame it loads).
    let Some(world) = world else { return };
    spawned.area = Some(run.name.clone());
    // FUN_00168ba8 -> FUN_001b6408: the world's talk table is loaded with the area.
    talk.load(&disc, run.name.get(..2).unwrap_or("tt"));
    talk.offer = None;
    if let Some(end) = talk.drop_run() {
        cam.talk = Some(end);
    }
    if std::env::var("KH2_NO_NPC").is_ok_and(|v| v != "0") {
        return;
    }
    assets.load_once(&disc);
    // Harness: `KH2_NPC_WATCH=<n>` puts Sora three metres in front of the n-th townsperson spawned
    // (1-based), facing it (a test aid for screenshots, not the game's rule).
    let watch: Option<usize> = std::env::var("KH2_NPC_WATCH").ok().and_then(|v| v.parse().ok());
    // `KH2_NPC_WATCH_DIST=<cm>`: how far in front (default 300).
    let watch_dist: f32 = std::env::var("KH2_NPC_WATCH_DIST").ok().and_then(|v| v.parse().ok()).unwrap_or(300.0);
    let mut count = 0;
    // Every group the three scripts loaded (FUN_00181728 makes the objects of each; in tt02 the
    // townspeople's kind-6 groups a72_, b72_, c72_ come from `evt` programs 0x10 / 0x12, b50_ from 0xc).
    for g in run.script.groups.iter() {
        let Some(groups) = run.ard.spawns.get(&g.tag) else { continue };
        for grp in groups {
            let shared = Arc::new(NpcGroup {
                tag: g.tag.clone(),
                kind: grp.kind,
                id: grp.id,
                paths: grp.paths.clone(),
                returns: grp.returns.clone(),
                activators: grp.activators.clone(),
            });
            for e in &grp.entities {
                if !kh2_ard::run::entity_for_entrance(e, rec.entrance) {
                    continue;
                }
                let Some(row) = assets.objs.iter().find(|r| r.object_id == e.object) else { continue };
                if row.object_type != OBJECT_TOWNSPERSON {
                    continue;
                }
                let model = row.model_name.clone();
                let flags124 = row.flags & 8 != 0;
                let anim = row.animation_name.clone();
                let Some(rig) = assets.rig(&disc, &model, &anim) else {
                    warn!("npc: {model}: model or motion set not readable; not spawned");
                    continue;
                };
                let script = assets.script(&disc, &model);
                let model_parts = assets.parts(&disc, &model);
                let at = game_to_app(e.position);
                let at = world.ground(at, at.y + 100.0, 400.0).map_or(at, |g| at.with_y(g.y));
                let yaw = app_yaw(e.rotation[1]);
                spawned.serial += 1;
                let serial = spawned.serial;
                let root = commands.spawn((Transform::from_translation(at * SCALE).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default(), AreaPart)).id();
                let body = spawn_skinned(&mut commands, root, rig, &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
                let tag = format!("{model}#{serial}");
                let mut ent = commands.entity(root);
                ent.insert(Npc { body, yaw, grounded: true, record: e.clone(), parts: model_parts, talk: Default::default() });
                let scripted = script.is_some();
                if let Some(prog) = script {
                    let mut rt = Runtime::new((*prog).clone());
                    if let Some(t) = &assets.table {
                        rt.set_host_table(t.clone());
                    }
                    rt.shared.rng = 0x2545_F491u32 ^ serial.wrapping_mul(0x85EB_CA6B);
                    rt.core.mode_log = Some(Vec::new());
                    let handle = SELF_BASE + (HANDLE_BASE + serial) * OBJ_SIZE as u32;
                    ent.insert(NpcBrain {
                        rt,
                        obj: Obj::new(handle),
                        tag: tag.clone(),
                        flags124,
                        fields: NpcFields::new(e, &shared, serial),
                        started: false,
                        errors_seen: 0,
                        mode: None,
                        last_frame: 0.0,
                        visible: true,
                        pending_mode: None,
                    });
                }
                count += 1;
                if watch == Some(count) && let Ok((mut tf, mut s)) = sora.single_mut() {
                    let a = yaw;
                    let p = at + Vec3::new(a.sin(), 0.0, a.cos()) * watch_dist;
                    let p = world.ground(p, p.y + 100.0, 400.0).map_or(p, |g| p.with_y(g.y));
                    crate::sora::respawn(&mut s, p);
                    s.motor.yaw = a + std::f32::consts::PI;
                    tf.translation = p * SCALE;
                    tf.rotation = Quat::from_rotation_y(s.motor.yaw);
                    info!("npc: KH2_NPC_WATCH: Sora put in front of {tag} at ({:.0}, {:.0}, {:.0})", p.x, p.y, p.z);
                }
                info!("npc: {} group {} (kind {}, id {}) spawned {tag} (object {:#x})", run.name, g.tag, grp.kind, grp.id, e.object);
                info!(
                    "npc: {tag} at ({:.0}, {:.0}, {:.0}) yaw {yaw:.3}, spawn flags {:#x}, talk id {:#x}, part command {}, {} paths{}",
                    at.x,
                    at.y,
                    at.z,
                    NpcFields::spawn_flags(e),
                    e.talk_id(),
                    e.part_command(),
                    shared.paths.len(),
                    if scripted { "" } else { " (no script: stands)" }
                );
            }
        }
    }
    info!("npc: {}: {count} townspeople spawned from the loaded groups", run.name);
}

/// One tick of every townsperson's script: run it, carry out its motion and movement requests.
#[allow(clippy::too_many_arguments)]
pub(crate) fn npc_update(
    mut commands: Commands,
    world: Option<Res<World>>,
    trace: Option<ResMut<AiTrace>>,
    cam: Query<(&Camera, &GlobalTransform), crate::shared::ui::GameCamera>,
    sora: Query<&Sora>,
    assets: Res<NpcAssets>,
    mut q: Query<(Entity, &mut Transform, &mut Npc, &mut NpcBrain)>,
    mut anims: Query<&mut Animator>,
    mut tick: Local<u64>,
) {
    let (Some(mut trace), Ok(s)) = (trace, sora.single()) else { return };
    let paired = HashMap::new();
    let cam = cam.single().ok();
    let log = npclog();
    *tick += 1;
    let status_tick = tick.is_multiple_of(60);

    // ---- the class's own logic first (FUN_00189ae8 -> FUN_0018a130 for objects whose bit 0 is
    // clear) [inferred order: the object's update before its script's tick].
    let player_body = assets.player_body.as_ref().and_then(|r| parts::shape_of(r, s.motor.pos, s.motor.yaw));
    // `FUN_0016bbe8` for the player: his type-0x11 record, a point on a bone of his last pose [inferred
    // as for the talk camera's bone points]; without it the object's position, raised to his head.
    let player_point = {
        let rot = Quat::from_rotation_y(s.motor.yaw);
        let on_bone = |r: &parts::Rec| {
            let off = Vec3::new(r.off[0] as f32, r.off[1] as f32, r.off[2] as f32);
            let pose = anims.get(s.body).ok()?;
            Some(s.motor.pos + rot * pose.pose.get((r.joint & 0x3fff) as usize)?.transform_point3(off) + Vec3::Y * r.lift as f32)
        };
        assets.player_point.as_ref().and_then(|r| if r.joint & parts::JOINT_OBJECT != 0 { parts::shape_of(r, s.motor.pos, s.motor.yaw).map(|sh| sh.center) } else { on_bone(r) })
    };
    {
        let mut rows: Vec<_> = q.iter_mut().collect();
        let tags: Vec<String> = rows.iter().map(|r| r.3.tag.clone()).collect();
        // [inferred] the script's init (which hands the parameter block over, host 5.33) runs
        // before the class's first update: an object whose script has not started is skipped.
        let started: Vec<bool> = rows.iter().map(|r| r.3.started).collect();
        let mut minds: Vec<Mind> = rows
            .iter_mut()
            .map(|(_, tf, npc, brain)| {
                let pos = tf.translation / SCALE;
                let shape = |kind: u8| npc.parts.as_ref().and_then(|t| t.first(kind)).and_then(|r| parts::shape_of(r, pos, npc.yaw));
                Mind {
                    handle: brain.obj.handle,
                    object: npc.record.object,
                    serial: npc.record.serial,
                    pos,
                    yaw: npc.yaw,
                    body: shape(0),
                    talk: shape(parts::TYPE_REACTION),
                    fields: &mut brain.fields,
                }
            })
            .collect();
        for i in 0..minds.len() {
            if !started[i] || minds[i].fields.test_bit(0) {
                continue;
            }
            let before = minds[i].fields.region;
            let what = class_tick(&mut minds, i, player_body.as_ref());
            if log {
                match what {
                    Attention::Player => info!("npc: {} attention: the player meets the talk part: kind-4 follower (timer {:.0})", tags[i], minds[i].fields.follower.timer),
                    Attention::Pair(j) => info!("npc: {} attention: paired with {} (kind 5, timer {:.0})", tags[i], tags[j], minds[i].fields.follower.timer),
                    Attention::None => {
                        let f = &minds[i].fields.follower;
                        if minds[i].fields.region != before {
                            info!(
                                "npc: {} attention: region {:?} -> {:?}: follower kind {} mode {} motion {} timer {:.0} flag {}",
                                tags[i], before, minds[i].fields.region, f.kind, f.mode, f.motion, f.timer, f.flag
                            );
                        }
                    }
                }
            }
        }
    }
    let peers: HashMap<u32, Peer> = q
        .iter()
        .map(|(_, tf, npc, brain)| (brain.obj.handle, Peer { pos: tf.translation / SCALE, target: brain.fields.follower.target, slot: anims.get(npc.body).ok().and_then(|a| a.slot) }))
        .collect();

    for (ent, mut tf, mut npc, mut brain) in &mut q {
        let Ok(mut anim) = anims.get_mut(npc.body) else { continue };
        let rig = anim.rig.clone();
        let NpcBrain { rt, obj, tag, flags124, fields, started, errors_seen, mode, last_frame, visible, pending_mode } = &mut *brain;
        obj.motor.pos = tf.translation / SCALE;
        obj.motor.yaw = npc.yaw;
        obj.motor.grounded = npc.grounded;
        obj.on_screen = cam
            .and_then(|(c, g)| c.world_to_ndc(g, tf.translation + Vec3::Y))
            .is_none_or(|n| n.x.abs() <= 1.0 && n.y.abs() <= 1.0 && (0.0..=1.0).contains(&n.z));
        let wrapped = anim.looping && anim.frame < *last_frame;
        obj.done = anim.slot.is_some() && (anim.finished || wrapped);
        obj.slot = anim.slot;
        obj.frame = anim.frame;
        obj.blend_left = (obj.blend_left - 1.0).max(0.0);
        obj.spawn_kind = npc.record.spawn_kind as i32;
        obj.spawn_params = Some([0x20, 0x24].map(|o| i32::from_le_bytes(npc.record.raw[o..o + 4].try_into().unwrap_or_default())));
        let env = Env {
            player: s.motor.pos,
            player_yaw: s.motor.yaw,
            world: world.as_deref(),
            rig: &rig,
            flags124: *flags124,
            enmp: 0,
            player_serial: 0,
            player_motion_done: true,
            paired: &paired,
            player_model: 0,
            mission_counters: None,
            event_active: false,
        };
        let handle = obj.handle;
        let head_rec = npc.parts.as_ref().and_then(|t| t.first(look::TYPE_HEAD)).copied();
        fields.cone_part = head_rec.and_then(|rec| {
            let off = Vec3::new(rec.off[0] as f32, rec.off[1] as f32, rec.off[2] as f32);
            let m = anim.pose.get((rec.joint & 0x3fff) as usize)?;
            Some((obj.motor.pos + Quat::from_rotation_y(npc.yaw) * m.transform_point3(off) + Vec3::Y * rec.lift as f32, rec.r[0]))
        });
        let mut host = NpcHost { inner: BevyHost { obj, env: &env, trace: &mut trace, tag: tag.as_str() }, fields, peers: &peers, log };
        if !*started {
            *started = true;
            rt.spawn(&mut host, handle);
        }
        if let Some(m) = pending_mode.take() {
            // FUN_001870d8 asks whether the script registered the mode; FUN_00186db0 switches to it.
            rt.set_mode_named(&mut host, m);
            if log {
                info!("npc: {tag} native mode switch to {m:?} (FUN_00189ff0 -> FUN_00186db0): now {:?}", rt.current_mode());
            }
        }
        rt.update(&mut host);
        // Host 5.33 named a block in the script's globals: copy its nine words now.
        if let Some(at) = fields.params_ptr.take() {
            let base = rt.shared.layout.globals;
            let o = at.wrapping_sub(base) as usize;
            match rt.shared.globals.get(o..o + 36) {
                Some(b) => {
                    for (k, w) in b.chunks(4).enumerate() {
                        fields.params[k] = u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
                    }
                    if log {
                        info!("npc: {tag} parameter block (5.33): {:?}", fields.params.map(f32::from_bits));
                    }
                }
                None => warn!("npc: {tag}: host 5.33 named a block outside the script's globals ({at:#x}); parameters stay 0"),
            }
        }
        let now = rt.current_mode();
        if now != *mode {
            if log {
                info!("npc: {tag} script mode {:?} -> {:?}", mode.as_deref().unwrap_or("-"), now.as_deref().unwrap_or("-"));
            }
            *mode = now;
        }
        for entry in rt.core.mode_log.as_mut().map(std::mem::take).unwrap_or_default() {
            trace.line(tag, &format!("mode {entry}"));
        }
        if log && status_tick {
            let p = obj.motor.pos;
            info!(
                "npc: {tag} status: mode {} slot {:?} at ({:.0}, {:.0}, {:.0}) yaw {:.2} path t {}",
                mode.as_deref().unwrap_or("-"),
                obj.slot,
                p.x,
                p.y,
                p.z,
                obj.motor.yaw,
                fields.path.as_ref().map_or("-".to_string(), |p| format!("{:.2}", p.t))
            );
        }
        for (pc, err) in &rt.core.errors[*errors_seen..] {
            warn!("npc: {tag}: script error {err:?} at {pc:#06x}");
        }
        *errors_seen = rt.core.errors.len();

        // ---- the head look-at controller's commands (hosts 1.174 / 1.176): only an object whose model
        // has the head record (type 0x14) can start one (FUN_001aece8 looks it up first).
        if let Some(cmd) = fields.look_cmd.take() {
            match (cmd, head_rec.is_some()) {
                (LookCmd::Start(n), true) => fields.look.start(n),
                (LookCmd::Release(n), _) => fields.look.release(n),
                _ => {}
            }
            if log {
                info!("npc: {tag} head look-at {cmd:?}: record {}", if head_rec.is_some() { "present" } else { "absent (nothing happens)" });
            }
        }

        // ---- carry out the requests (as `enemy_ai::ai_update` does for enemies)
        let looping_slot = |s: usize| rig.mset.get(s).and_then(|s| s.motion.as_ref()).is_some_and(|m| m.frame_return < m.frame_end);
        if let Some(req) = obj.motion_req.take() {
            anim.restart(Some(req.slot), looping_slot(req.slot), req.blend);
            if let Some(s) = req.start {
                anim.frame = s;
                anim.prev_frame = s;
            }
        } else if let Some((slot, blend)) = fields.queued.filter(|_| anim.finished || anim.slot.is_none()) {
            // The queued motion (host 1.13) follows the playing one when it ends.
            fields.queued = None;
            if rig.mset.has(slot) {
                trace.line(tag, &format!("motion request slot {slot} blend {blend} (queued)"));
                anim.restart(Some(slot), looping_slot(slot), blend);
                obj.slot = Some(slot);
            }
        }
        let (root_t, root_yaw) = anim.root_delta;
        obj.motor.yaw = crate::motor::wrap_angle(obj.motor.yaw + root_yaw);
        let root_move = Quat::from_rotation_y(obj.motor.yaw - anim.root.yaw) * root_t;
        let delta = obj.move_vel + root_move.with_y(0.0);
        obj.move_vel = Vec3::ZERO;
        let before = obj.motor.pos;
        if let Some(w) = world.as_deref() {
            obj.motor.move_by(w, delta, None);
            if obj.motor.grounded {
                obj.motor.pos.y += anim.root.translation.y.max(0.0);
            }
        }
        obj.want_vel = delta;
        obj.moved = obj.motor.pos - before;
        *last_frame = anim.frame;
        npc.grounded = obj.motor.grounded;
        npc.yaw = obj.motor.yaw;
        tf.translation = obj.motor.pos * SCALE;
        tf.rotation = Quat::from_rotation_y(npc.yaw);
        // The head look-at controller's tick (FUN_001aea28 -> FUN_001aee90) after the move [inferred order].
        anim.turn = None;
        // Harness: `KH2_NO_HEADLOOK=1` leaves the head on the motion's pose (to compare shots).
        if let Some(rec) = head_rec.filter(|_| std::env::var_os("KH2_NO_HEADLOOK").is_none()) {
            let joint = (rec.joint & 0x3fff) as usize;
            let off = Vec3::new(rec.off[0] as f32, rec.off[1] as f32, rec.off[2] as f32);
            let at_head = anim.pose.get(joint).map(|m| obj.motor.pos + Quat::from_rotation_y(npc.yaw) * m.transform_point3(off) + Vec3::Y * rec.lift as f32);
            if let Some(head) = at_head {
                let input = look::Input { head, target: player_point.unwrap_or(s.motor.pos + Vec3::Y * 100.0), body_yaw: npc.yaw, limits: look::Limits::of(&rec) };
                if let Some(w) = fields.look.tick(&input) {
                    anim.turn = Some(crate::skel::JointTurn { joint, rot: fields.look.turn(w) });
                    if log && status_tick {
                        info!("npc: {tag} head look-at: yaw {:.1} deg pitch {:.1} deg weight {:.2}, head {head:.0} target {:.0} body yaw {:.2}", fields.look.yaw_off.to_degrees(), fields.look.pitch.to_degrees(), fields.look.weight, input.target, npc.yaw);
                    }
                }
            }
        }
        // Effects, messages and the player-side commands of these scripts are not carried out.
        obj.effects.clear();
        obj.outbox.clear();
        obj.player_cmds.clear();
        obj.drops.clear();
        obj.advance_fade();
        let vis = !obj.hidden && obj.alpha > 0.0;
        if vis != *visible {
            *visible = vis;
            if let Ok(mut b) = commands.get_entity(npc.body) {
                b.insert(if vis { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
        if obj.hidden {
            trace.line(tag, "hidden: despawned");
            commands.entity(ent).despawn();
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Talk (WP-E3, docs/systems/events-movies/npc-savepoints.md section 3 "Talk")
//
// The offer: the player's controller asks every object for reaction candidates (FUN_0019cb70): a
// type-5 collision part whose volume the player's type-1 body touches offers its id as a command,
// and a part with id 0x83 offers the halfword +0x2c of the object's spawn record instead (31, the
// talk, for nearly every townsperson). The gates (FUN_0019cf58), the status (FUN_001ba2c0) and the
// cone are `reaction_cmd`'s.
//
// The command: Sora's class (FUN_001cf030) hands Execute 0x1f to FUN_00177b18, whose case 0x1f does
// NOT take the generic path FUN_00177c20 (no use counter, no CmdAction, no event 0xb to the
// target's script): FUN_00177d28 clears the player's velocities (+0x20, +0x10, his target +0x9c0),
// then FUN_001a56c0 marks HUD list record 0xc (FUN_001bef48), locks the player's control (bit 0 of
// 0x1d5d7b8, FUN_001b7ce8; read by FUN_001b7d50 through FUN_00176a18) and starts the talk task
// FUN_001a5728 (FUN_001a53f8: the reader of the target's talk id +0x500 kept at +0x504).
//
// The task (FUN_001a5728): the player faces the target (FUN_0016a948), the class-5 target faces the
// player unless bit 4 of its array is set (FUN_00189f40) and plays motion 0x32 (FUN_0017e220), the
// talk camera starts (FUN_00159688, kind 4) and the party members' heads turn (FUN_001abf40); then
// the program runs (FUN_001a53b8 -> FUN_001b6578, callback FUN_001a5a60: op 1 opens window class
// FUN_001aba58 with the message and waits until the window reaches state 3). When no event started
// (FUN_00196418), the target's facing comes back (FUN_0016a828) and FUN_00189ff0 ends the talk on the
// class-5 object: its motion becomes the follower's (+0xa48) when array bit 0 is set, else 0; with
// bit 0 clear the previous follower takes kind 4, the follower is reset and +0xb00 = +0xb18 + 60
// (`DAT_0037ab84`); the script goes to its "after_talk" mode when it has one. Then the camera comes
// back (FUN_00159768), the lock is released (FUN_001b7d18) and the HUD record cleared (FUN_001befc0).

/// The window Examine opens: `FUN_001a5608` builds class `FUN_001ab7d8` (`FUN_001aaf20(win, 3, 4, message, 0, 0x100, 0x20,
/// 3)`: font 3, colour 4, at (256, 32), frame kind 3, alignment 2), class 2 of `kh2_image::window::CLASSES`.
pub const EXAMINE_WINDOW_CLASS: usize = 2;
/// The motion the talked-to object plays (FUN_001a5728: `FUN_0017e220(.., obj + 0x140, 0x32)`).
pub const MOTION_TALK: usize = 0x32;
/// The window the talk opens: FUN_001aba58 builds the values of FUN_001ab9e0 (font 3, colour 5, at
/// (256, 336), frame kind 0, the speech tail), class 3 of `kh2_image::window::CLASSES`.
pub const TALK_WINDOW_CLASS: usize = 3;
/// `DAT_0037ab84`: added to +0xb18 when a talk ends (FUN_00189ff0).
pub const TALK_END_TIMER: f32 = 60.0;

/// The record type `FUN_0016bbe8` reads for an object's point (`FUN_0016b558(obj, 0x11, -1)`).
pub const TYPE_POINT: u8 = 0x11;

/// The collision record type the talk camera reads (`FUN_0018f108`: `FUN_0016b558(target, 0x13,
/// part)`).
pub const TYPE_TALK_CAMERA: u8 = 0x13;
/// The record type `FUN_0018ee20` falls back to (`FUN_0016b558(target, 3, part)`, else the first).
pub const TYPE_TARGET: u8 = 3;

/// `FUN_0018f108` for the talk camera: the target's type-0x13 record with the talk part's index,
/// else (`FUN_0018eec0` -> `FUN_0018ee20`) its type-3 record with that index or its first type-3
/// record. `FUN_001894d0` gives the record's centre (the look-at point) and its two radii (the
/// distance and the height). `pos`/`yaw` are the target's, in the app's space. A record on a
/// bone (joint without bit 0x4000) is read through `bone(joint, offset)`, the offset's point in
/// the app's space (`FUN_0016aa18`, valid once the object's pose ran this frame: `FUN_001b7be8`
/// sets +0x7f8 bit 0x400, which `FUN_001894d0` requires; without it the record reads as zero).
/// `None` when no record exists or the bone is unknown: the game then reads the object's
/// position with an unset distance (a stack value) [GAP: not ported].
pub fn talk_shot(
    table: &parts::Table,
    part: u8,
    (pos, yaw): (Vec3, f32),
    frame: crate::camera::rules::TalkFrame,
    bone: &dyn Fn(usize, Vec3) -> Option<Vec3>,
) -> Option<(crate::camera::rules::TalkShot, u8)> {
    let rec = table
        .recs
        .iter()
        .find(|r| r.kind == TYPE_TALK_CAMERA && r.part == part)
        .or_else(|| table.recs.iter().find(|r| r.kind == TYPE_TARGET && r.part == part))
        .or_else(|| table.first(TYPE_TARGET))?;
    let look = if rec.joint & parts::JOINT_OBJECT != 0 {
        parts::shape_of(rec, pos, yaw)?.center
    } else {
        let off = Vec3::new(rec.off[0] as f32, rec.off[1] as f32, rec.off[2] as f32);
        bone((rec.joint & 0x3fff) as usize, off)? + Vec3::Y * rec.lift as f32
    };
    Some((crate::camera::rules::TalkShot { look, dist: rec.r[0] as f32, height: rec.r[1] as f32, frame }, rec.kind))
}

fn reaction_log() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_REACTIONLOG").is_ok_and(|v| v != "0"))
}

/// The townspeople's candidates for the player's slot 1 (after `reaction_cmd` filled it from the
/// enemies): the best part offer of every townsperson; an enemy offer of a higher or equal
/// priority keeps the slot [approx: FUN_0019c520 also compares the distance].
#[allow(clippy::too_many_arguments)]
pub(crate) fn talk_offer(
    mut talk: ResMut<Talk>,
    rules: Option<Res<crate::reaction_cmd::ReactionRules>>,
    slot: Option<Res<crate::reaction_cmd::ReactionOffer>>,
    assets: Res<NpcAssets>,
    sora: Query<&Sora>,
    npcs: Query<(Entity, &Transform, &Npc, &NpcBrain)>,
    anims: Query<&Animator>,
    drive: Option<Res<crate::drive::DriveState>>,
    battle: Res<crate::sora::Battle>,
) {
    use crate::reaction_cmd::{window_at, Active, Ctx};
    let before = talk.npc_offer.map(|a| (a.cmd, a.from, a.status));
    talk.offer = None;
    let mut tag = String::new();
    if let (Some(rules), Ok(s), Some(body)) = (rules.as_deref(), sora.single(), assets.player_body.as_ref())
        && talk.run.is_none()
        && s.hp > 0
        && !matches!(s.state, crate::sora::State::Down | crate::sora::State::Viewer)
    {
        let form = drive.as_deref().map_or(0, |d| d.object_form());
        let battle = battle.active() || drive.as_deref().is_some_and(|d| d.force_battle);
        let ctx = Ctx { form, battle, mp: s.mp, ..Default::default() };
        let window = anims.get(s.body).ok().and_then(|a| a.triggers().and_then(|tg| window_at(tg, a.gframe())));
        let (ppos, pyaw) = (s.motor.pos, s.motor.yaw);
        if let Some(player) = parts::shape_of(body, ppos, pyaw) {
            let mut best: Option<(PartOffer, Entity, &str)> = None;
            for (e, tf, npc, brain) in &npcs {
                let Some(table) = npc.parts.as_deref() else { continue };
                let (mut on, mut off) = (Vec::new(), Vec::new());
                if let Some((tg, f)) = anims.get(npc.body).ok().and_then(|a| a.triggers().map(|t| (t, a.gframe()))) {
                    for r in tg.active(f) {
                        match (r.id, r.params.first()) {
                            (4, Some(&p)) => on.push(p),
                            (5, Some(&p)) => off.push(p),
                            _ => {}
                        }
                    }
                }
                let Some(o) = part_offer(rules, table, npc.record.part_command(), (&on, &off), (tf.translation / SCALE, npc.yaw), &player, (ppos, pyaw), (&ctx, window)) else { continue };
                if best.is_some_and(|(b, _, _)| o.priority < b.priority || (o.priority == b.priority && b.dist <= o.dist)) {
                    continue;
                }
                best = Some((o, e, brain.tag.as_str()));
            }
            let enemy = slot.as_deref().and_then(|o| o.0).and_then(|a| rules.cmd(a.cmd)).map(|c| c.priority);
            if let Some((o, e, t)) = best.filter(|(o, _, _)| enemy.is_none_or(|p| p < o.priority)) {
                let message = rules.cmd(o.cmd).map_or(0, |c| c.message_id as u32);
                talk.offer = Some(Active { cmd: o.cmd, pos: o.part as i16, from: e, status: o.status, message });
                tag = t.to_string();
            }
        }
    }
    talk.npc_offer = talk.offer;
    if reaction_log() && talk.offer.map(|a| (a.cmd, a.from, a.status)) != before {
        match talk.offer {
            Some(a) => info!(
                "reaction: offered command {} (message {}) by {tag} {:?} from its part {} (id 0x83: spawn record +0x2c, FUN_0019cb70), status {:?}",
                a.cmd, a.message, a.from, a.pos, a.status
            ),
            None => info!("reaction: townsperson offer withdrawn"),
        }
    }
}

/// While a talk runs the player's control is locked (bit 0 of 0x1d5d7b8, FUN_001b7d50): the pad
/// reads as untouched.
fn talk_lock_pad(talk: Res<Talk>, mut pad: ResMut<crate::sora::Pad>) {
    if talk.running() {
        pad.lock();
    }
}

/// The HUD's capsule reads the player's slot 1 (`hud_menu::mirror_reaction`): the townsperson's
/// offer is put there for it when the enemies left the slot empty...
fn talk_show_offer(talk: Res<Talk>, mut slot: Option<ResMut<crate::reaction_cmd::ReactionOffer>>) {
    if let (Some(slot), Some(o)) = (slot.as_deref_mut(), talk.offer)
        && slot.0.is_none()
    {
        slot.0 = Some(o);
    }
}

/// ...and taken out again after it, so `reaction_cmd`'s own offer and accept never see it (they
/// handle the enemies' offers; see the outside-ownership note in docs/systems/events-movies/npc-savepoints.md section 3).
fn talk_hide_offer(talk: Res<Talk>, mut slot: Option<ResMut<crate::reaction_cmd::ReactionOffer>>) {
    if let (Some(slot), Some(o)) = (slot.as_deref_mut(), talk.offer)
        && slot.0 == Some(o)
    {
        slot.0 = None;
    }
}

/// The talk task (FUN_001a5728) step by step. The accept itself (FUN_0019c670 -> FUN_001cf030 ->
/// FUN_00177b18 case 0x1f) is `reaction_cmd::accept`, which leaves the offer in `Talk::accepted`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn talk_step(
    mut talk: ResMut<Talk>,
    rules: Option<Res<crate::reaction_cmd::ReactionRules>>,
    mut win: ResMut<crate::hud::hud_msgwin::ScriptWindow>,
    mut sora: Query<(&mut Transform, &mut Sora), Without<Npc>>,
    mut npcs: Query<(&mut Transform, &mut Npc, &mut NpcBrain), Without<Sora>>,
    mut anims: Query<&mut Animator>,
    mut cam: ResMut<crate::camera::CamEvent>,
) {
    use kh2_ard::libretto::Op;
    let talk = &mut *talk;
    if talk.run.is_none() {
        // Only a townsperson's Execute 0x1f is this talk (the save points take their own 0x37 in
        // `savepoint::point_use`, which runs first); everything else was answered by `reaction_cmd`.
        let Some(o) = talk.accepted.filter(|o| npcs.contains(o.from)) else { return };
        talk.accepted = None;
        let Some(c) = rules.as_deref().and_then(|r| r.cmd(o.cmd)).filter(|c| c.execute == EXECUTE_TALK || c.execute == EXECUTE_EXAMINE) else { return };
        let (Ok((mut stf, mut s)), Ok((mut ntf, mut npc, mut brain))) = (sora.single_mut(), npcs.get_mut(o.from)) else { return };
        if c.execute == EXECUTE_EXAMINE {
            // FUN_001a5510: the player faces the target (FUN_0016a948 with the target point minus his position); a
            // class-5 target's script goes to its "before_check" mode (FUN_00189eb8: FUN_001870d8 / FUN_00186db0,
            // string 0x37ab68) when it has one; no talk camera, no turn of the target, no motion 0x32.
            s.motor.vel = Vec3::ZERO;
            let to_npc = (ntf.translation / SCALE - s.motor.pos).with_y(0.0);
            if to_npc.length_squared() > 1e-6 {
                s.motor.yaw = to_npc.x.atan2(to_npc.z);
                stf.rotation = Quat::from_rotation_y(s.motor.yaw);
            }
            brain.pending_mode = Some("before_check");
            let id = npc.record.talk_id();
            let program = talk.lib.as_ref().and_then(|l| l.program(id)).unwrap_or_default();
            let ops: Vec<Op> = npc.talk.take(&program).iter().map(|c| c.kind()).collect();
            info!("reaction: accepted command {} (Execute 0x1e, Examine) from {} {:?}: FUN_001a54a8, talk id {id:#x}, ops {ops:?}", o.cmd, brain.tag, o.from);
            talk.run = Some(TalkRun { npc: o.from, tag: brain.tag.clone(), ops, at: 0, window: false, yaw: npc.yaw, camera: false, examine: true });
            talk.offer = None;
            return;
        }
        // FUN_00177d28: the player's velocities are cleared.
        s.motor.vel = Vec3::ZERO;
        // FUN_001a5728: the player faces the target (FUN_0016a948 with the target point minus his
        // position; the point is the object's position [inferred: FUN_0018efa0 of the target record]).
        let (ppos, npos) = (s.motor.pos, ntf.translation / SCALE);
        let to_npc = (npos - ppos).with_y(0.0);
        if to_npc.length_squared() > 1e-6 {
            s.motor.yaw = to_npc.x.atan2(to_npc.z);
            stf.rotation = Quat::from_rotation_y(s.motor.yaw);
        }
        // FUN_00189f40: the class-5 target faces the player unless bit 4 of its array is set.
        let saved = npc.yaw;
        if !brain.fields.test_bit(4) && to_npc.length_squared() > 1e-6 {
            npc.yaw = (-to_npc.x).atan2(-to_npc.z);
            ntf.rotation = Quat::from_rotation_y(npc.yaw);
        }
        // FUN_00159688(target record, a1): the talk camera, placed on the camera's next tick from
        // the target as it stands then [inferred: after the target turned]. a1 is 1 unless the
        // target is a class-5 object (+0x588 bit 3, every townsperson) whose array has bit 4 or
        // bit 6 (FUN_00189e90 on +0xb04 & 0x10 / 0x40): then 0, the target's own frame.
        let mut camera = false;
        {
            use crate::camera::rules::TalkFrame;
            let own_frame = brain.fields.test_bit(4) || brain.fields.test_bit(6);
            let frame = if own_frame { TalkFrame::Object { forward: Vec3::new(npc.yaw.sin(), 0.0, npc.yaw.cos()) } } else { TalkFrame::AroundPlayer };
            // A bone's point: the body's last pose (model centimetres under the root, which
            // carries the position and the facing) [inferred: one frame old, the offset in the
            // bone's frame as combat.rs reads the attack shapes].
            let pose = anims.get(npc.body).ok().map(|a| a.pose.clone());
            let rot = Quat::from_rotation_y(npc.yaw);
            let bone = |j: usize, off: Vec3| pose.as_ref()?.get(j).map(|m| npos + rot * m.transform_point3(off));
            match npc.parts.as_deref().and_then(|t| talk_shot(t, o.pos as u8, (npos, npc.yaw), frame, &bone)) {
                Some((shot, kind)) => {
                    if npclog() {
                        info!("npc: {} talk camera: record type {kind:#x} of part {} (FUN_0018f108), look {:.0}, distance {:.0}, height {:.0}", brain.tag, o.pos, shot.look, shot.dist, shot.height);
                    }
                    cam.talk = Some(crate::camera::TalkCam::Start(shot));
                    camera = true;
                }
                None => info!("npc: {} talk camera: no record of type 0x13 / 3 on the object's frame for part {}; the camera stays (gap)", brain.tag, o.pos),
            }
        }
        if let Ok(mut a) = anims.get_mut(npc.body)
            && a.rig.mset.has(MOTION_TALK)
        {
            let looping = a.rig.mset.get(MOTION_TALK).and_then(|m| m.motion.as_ref()).is_some_and(|m| m.frame_return < m.frame_end);
            a.restart(Some(MOTION_TALK), looping, 0.0);
        }
        // FUN_001a53f8 / FUN_001a53b8: the program from the kept place (FUN_001b6578).
        let id = npc.record.talk_id();
        let program = talk.lib.as_ref().and_then(|l| l.program(id)).unwrap_or_default();
        let ops: Vec<Op> = npc.talk.take(&program).iter().map(|c| c.kind()).collect();
        if reaction_log() {
            info!(
                "reaction: accepted command {} (Execute {}) from {} {:?}: talk (FUN_00177b18 case 0x1f -> FUN_001a56c0), talk id {id:#x}, ops {ops:?}; no script event 0xb (FUN_00177c20 is not called)",
                o.cmd, c.execute, brain.tag, o.from
            );
        }
        if npclog() {
            info!("npc: {} talk starts: motion {MOTION_TALK:#x}, facing the player, program {id:#x} {ops:?}", brain.tag);
        }
        talk.run = Some(TalkRun { npc: o.from, tag: brain.tag.clone(), ops, at: 0, window: false, yaw: saved, camera, examine: false });
        talk.offer = None;
        return;
    }
    let Some(run) = talk.run.as_mut() else { return };
    // A window of this run is up: wait until it is closed (the window's state 3).
    if run.window {
        if !std::mem::take(&mut win.closed) {
            return;
        }
        run.window = false;
        run.at += 1;
    }
    while let Some(op) = run.ops.get(run.at).copied() {
        match op {
            Op::Message(id) => {
                // FUN_001a5a60: window class FUN_001aba58 with the message (Examine, FUN_001a5608: class FUN_001ab7d8;
                // its wait also ends when FUN_001a5b18(0xe) answers 0 [GAP: that player state; not ported]).
                win.open = Some((id, if run.examine { EXAMINE_WINDOW_CLASS } else { TALK_WINDOW_CLASS }));
                win.closed = false;
                run.window = true;
                if npclog() {
                    info!("npc: {} talk: message {id:#x} in window class {TALK_WINDOW_CLASS} (FUN_001aba58)", run.tag);
                }
                return;
            }
            Op::Event(e) => {
                // FUN_00197b70(e, 0): 0x65 goes to FUN_0023f108 [not ported].
                info!("npc: {} talk: event {e:#x} fired (FUN_00197b70); not ported", run.tag);
            }
            other => info!("npc: {} talk: op {other:?} not ported", run.tag),
        }
        run.at += 1;
    }
    // The end (FUN_001a5728 after FUN_001a53b8; no event runs in the app, FUN_00196418 is 0).
    let run = talk.run.take().expect("run");
    if let Ok((mut ntf, mut npc, mut brain)) = npcs.get_mut(run.npc) {
        npc.yaw = run.yaw;
        ntf.rotation = Quat::from_rotation_y(npc.yaw);
        // FUN_00189ff0.
        let f = &mut brain.fields;
        let motion = if f.test_bit(0) { f.follower.motion.max(0) as usize } else { 0 };
        if !f.test_bit(0) {
            f.prev.kind = 4;
            f.follower.reset();
            f.timer_b00 = f.param(2) + TALK_END_TIMER;
        }
        brain.pending_mode = Some("after_talk");
        if let Ok(mut a) = anims.get_mut(npc.body)
            && a.rig.mset.has(motion)
        {
            let looping = a.rig.mset.get(motion).and_then(|m| m.motion.as_ref()).is_some_and(|m| m.frame_return < m.frame_end);
            a.restart(Some(motion), looping, 0.0);
        }
        if npclog() || reaction_log() {
            info!("npc: {} talk ends (FUN_00189ff0): motion {motion}, previous follower kind {}, +0xb00 {:.0}, mode after_talk", run.tag, brain.fields.prev.kind, brain.fields.timer_b00);
        }
    }
    // FUN_00159768: the camera comes back.
    if run.camera {
        cam.talk = Some(crate::camera::TalkCam::End);
    }
}

// ---------------------------------------------------------------------------------------------------
// The class-5 object's own fields

/// One of the two attention followers of a class-5 object (`FUN_0018b0d8` makes them at +0xa40
/// and +0xaa0; `FUN_0018b0f0` resets one). Offsets are inside the 0x60-byte follower.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Follower {
    /// +0: the kind (2 the walk path, 4 the player, 5 another townsperson, 6 from a return entry).
    pub kind: i32,
    /// +4: the target mode (0 none, 1 an object, 2 the player, 3 a point).
    pub mode: i32,
    /// +8: the motion of the entry (`FUN_0018b1e0` of the entry's byte 1).
    pub motion: i32,
    /// +0xc: the target object (mode 1).
    pub target: u32,
    /// +0x10: the target point (mode 3), the game's space.
    pub point: [f32; 4],
    /// +0x20: the region (a volume of the group, `FUN_0018b308`) whose crossing ends the follower;
    /// the index into the group's activators.
    pub region: Option<usize>,
    /// +0x24 (object +0xa64): a flag byte the script reads (host 5.7): turn the walk around after
    /// the action.
    pub flag: u8,
    /// +0x28: the place across the region (`FUN_001063b8(1.0)`, `DAT_0037abd4`) at which the
    /// follower's test passes.
    pub threshold: f32,
    /// +0x2c: the object's place across the region when the follower started (`FUN_001d8c18`).
    pub start: f32,
    /// +0x30 (object +0xa70): set by host 5.0 when the follower's test passed.
    pub done: u8,
    /// +0x31: forced done.
    pub forced: u8,
    /// +0x50: a timer counted down by one per frame (`FUN_0018b228`, `DAT_00356f40` = 1).
    pub timer: f32,
}

impl Follower {
    /// `FUN_0018b0f0`: clears +0x31, +0x30, the kind, the mode, the target and the region (the
    /// motion, the point, the flag and the timer keep their values).
    pub fn reset(&mut self) {
        self.forced = 0;
        self.done = 0;
        self.kind = 0;
        self.mode = 0;
        self.target = 0;
        self.region = None;
    }

    /// `FUN_0018b110`: the follower is live when it is forced or has a region.
    pub fn live(&self) -> bool {
        self.forced != 0 || self.region.is_some()
    }
}

/// The fields of the class-5 object that the group 5 hosts read and write (game offsets in the
/// doc comments), kept on `NpcBrain`.
pub struct NpcFields {
    /// +0xb04..: the bit array (`FUN_00189938` tests, `FUN_00189960` sets): bits 0..6 come from the
    /// spawn record's word +0x24 (`FUN_001899d0`).
    pub bits: [u32; 2],
    /// +0xb08: the word host 5.26 writes (the second argument's object).
    pub word_b08: u32,
    /// +0xb10..+0xb34: the nine words host 5.33 copies in (speeds and timers the class reads: +0xb10
    /// and +0xb14 the type-4 timer range, +0xb18 / +0xb24 / +0xb30 the +0xb00 timer per follower
    /// kind, +0xb1c / +0xb20 the type-5 range, +0xb28 / +0xb2c the region idle range).
    pub params: [u32; 9],
    /// +0xb00: a timer counted down per frame by `FUN_0018a130` while above 0.
    pub timer_b00: f32,
    /// +0xa40 and +0xaa0: the current and the previous attention follower.
    pub follower: Follower,
    pub prev: Follower,
    /// +0x508: the walk path the object follows, set by host 1.177 (`FUN_00183c30`).
    pub path: Option<PathRun>,
    /// The spawn group's walk paths (`FUN_00199cc0` searches them by id) and return parameters.
    pub group: Arc<NpcGroup>,
    /// The object's own spawn record (the game's +0x81c).
    pub record: kh2_ard::Entity,
    /// The block host 5.33 named, when it lies in the script's own memory (read after the tick,
    /// `NpcBrain::read_params`).
    pub params_ptr: Option<u32>,
    /// Host 1.13 on this object: the motion queued behind the playing one (slot, blend). The
    /// game's motion controller queue is not ported; the app starts it when the playing motion
    /// ends [approx].
    pub queued: Option<(usize, f32)>,
    /// The head look-at controller (+0x4a0), and the host call that has not reached it yet.
    pub look: look::Look,
    pub look_cmd: Option<LookCmd>,
    /// The object's type-0x14 part for hosts 1.172 / 1.173 (`FUN_001af650`): its point (the head
    /// bone's pose of the last tick, `FUN_001894d0`) and its cone in degrees (record halfword +0x10).
    pub cone_part: Option<(Vec3, i16)>,
    /// +0xa30: the region the object stood in when `FUN_0018a4b0` last looked.
    pub region: Option<usize>,
    /// This frame's `FUN_0018aa08(obj, 5)`: the object's first type-5 part meets the player's
    /// type-1 body volume (computed by the class pass, read by host 5.11).
    pub player_in_part: bool,
    /// The object's random numbers (the game's global `rand()`).
    pub rng: Rng,
}

/// What host 1.174 or 1.176 asked of the head look-at controller, with its frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LookCmd {
    Start(f32),
    Release(f32),
}

impl NpcFields {
    /// The spawn record's word +0x24: bits 0..6 go into the object's bit array (`FUN_001899d0`).
    pub fn spawn_flags(e: &kh2_ard::Entity) -> u32 {
        u32::from_le_bytes(e.raw[0x24..0x28].try_into().unwrap_or_default())
    }

    pub fn new(e: &kh2_ard::Entity, group: &Arc<NpcGroup>, seed: u32) -> NpcFields {
        let mut f = NpcFields {
            region: None,
            player_in_part: false,
            rng: Rng(0x9E37_79B9 ^ seed.wrapping_mul(0x85EB_CA6B)),
            bits: [0; 2],
            word_b08: 0,
            params: [0; 9],
            timer_b00: 0.0,
            follower: Follower::default(),
            prev: Follower::default(),
            path: None,
            group: group.clone(),
            record: e.clone(),
            params_ptr: None,
            queued: None,
            look: look::Look::default(),
            look_cmd: None,
            cone_part: None,
        };
        // FUN_001899d0: bits 0..6 of the record's word +0x24; with bit 0 set the follower is put in
        // kind 2 (the walk path) at once, with mode 0.
        let flags = Self::spawn_flags(e);
        for bit in 0..7 {
            if flags >> bit & 1 != 0 {
                f.set_bit(bit);
            }
        }
        if f.bits[0] & 1 != 0 {
            f.follower.kind = 2;
            f.follower.mode = 0;
            f.follower.motion = 0;
            // The behaviour entries of a kind-6 group (FUN_00199ca0): an entry whose byte 0 is 0 and
            // byte 3 is 4 names the fixed idle motion by its byte 1 (the last such entry wins).
            if group.kind & 0xff == 6 {
                for r in &group.returns {
                    if r[0] == 0 && r[3] == 4 {
                        f.follower.motion = entry_motion(r[1]);
                    }
                }
            }
        }
        f
    }

    pub fn test_bit(&self, bit: u32) -> bool {
        self.bits.get((bit >> 5) as usize).is_some_and(|w| w >> (bit & 31) & 1 != 0)
    }

    pub fn set_bit(&mut self, bit: u32) {
        if let Some(w) = self.bits.get_mut((bit >> 5) as usize) {
            *w |= 1 << (bit & 31);
        }
    }

    /// Parameter word `k` of the block at +0xb10 as a float.
    fn param(&self, k: usize) -> f32 {
        f32::from_bits(self.params[k])
    }

    /// Host 5.4 (`FUN_0018ae08`): the follower is copied to the previous slot (+0xaa0), its +0x30
    /// cleared (`FUN_0018b138(f, 0)`) and reset (`FUN_0018b0f0`); then the timer +0xb00 takes
    /// +0xb18 when the follower's kind is 4, +0xb24 when 5, else +0xb30. The kind is read after the
    /// reset cleared it (the instructions at 0x18aeb0..0x18aed0: `jal 0x18b0f0`, then `lw 0xa40`),
    /// so the game always takes +0xb30.
    fn end_follower(&mut self) {
        self.prev = self.follower.clone();
        self.follower.done = 0;
        self.follower.reset();
        let k = match self.follower.kind {
            4 => 2,
            5 => 5,
            _ => 8,
        };
        self.timer_b00 = self.param(k);
    }

    /// The previous follower takes the current one when it has a kind (`FUN_0018a130` copies
    /// +0xa40..+0xa90 to +0xaa0 before starting a kind-4 or a kind-5 follower).
    fn keep_previous(&mut self) {
        if self.follower.kind != 0 {
            self.prev = self.follower.clone();
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// The class's own attention logic (vtable slot 3, FUN_00189ae8 -> FUN_0018a130)

/// The objentry id `FUN_0016b078` treats as the player (`DAT_0035c998`).
pub const OBJECT_PLAYER: u32 = 0x236;

/// One townsperson as the class pass sees it: what the other objects' code reads of it, and its
/// own fields.
pub struct Mind<'a> {
    /// The script handle (the object's address in the app's model).
    pub handle: u32,
    /// The objentry id (`FUN_0016b078` finds objects by it).
    pub object: u32,
    /// The spawn record's halfword +0x1e (`FUN_0016b928`: `FUN_0018a918` skips objects with the
    /// same one).
    pub serial: u16,
    /// Position (app space, cm) and facing (+0x550, `FUN_0016a890`).
    pub pos: Vec3,
    pub yaw: f32,
    /// The first type-0 part (`FUN_0016b558(obj, 0, -1)` placed by `FUN_001894d0`).
    pub body: Option<parts::Shape>,
    /// The first type-5 part (the talk part).
    pub talk: Option<parts::Shape>,
    pub fields: &'a mut NpcFields,
}

/// `FUN_0016b078(id)`: the player for 0x236, else the first object (in the object list) whose
/// objentry is `id`. Only the townspeople and the player are searched here [approx: the game's
/// list also holds the enemies and other objects]; 0 when none.
fn object_by_id(minds: &[Mind], id: u32) -> u32 {
    if id == OBJECT_PLAYER {
        return PLAYER;
    }
    minds.iter().find(|m| m.object == id).map_or(0, |m| m.handle)
}

/// `FUN_0018a7a0(a, b)`: the first return entry of `a`'s group with byte 0 = 0 whose word +4 names
/// (by objentry id) the object `b`.
fn entry_naming(minds: &[Mind], a: usize, b: u32) -> Option<[u8; 0x10]> {
    minds[a].fields.group.returns.iter().find(|r| r[0] == 0 && object_by_id(minds, u32::from_le_bytes([r[4], r[5], r[6], r[7]])) == b).copied()
}

/// `FUN_0018a598(obj)`: a partner townsperson. `FUN_0018a918(obj, 0)` takes the first other
/// object (not of the same spawn record serial) whose first type-0 part meets this object's
/// (`FUN_00189208`); it must be a class-5 object (+0x588 bit 3, every townsperson) whose follower
/// is not done, and the return entries of both must name each other. Answers the partner and the
/// two entries (this one's, the partner's).
fn partner(minds: &[Mind], i: usize) -> Option<(usize, [u8; 0x10], [u8; 0x10])> {
    let me = minds[i].body?;
    // [approx] the object list order is the app's spawn order; only townspeople are in it.
    let j = minds.iter().enumerate().position(|(j, m)| j != i && m.serial != minds[i].serial && m.body.is_some_and(|b| parts::overlaps(&me, &b) == Some(true)))?;
    if minds[j].fields.follower.done != 0 {
        return None;
    }
    let mine = entry_naming(minds, i, minds[j].handle)?;
    let theirs = entry_naming(minds, j, minds[i].handle)?;
    Some((j, mine, theirs))
}

/// `FUN_0018a828(timer, obj, entry, target, flag)`: the object's follower becomes a forced kind-5
/// follower toward `target` (mode 1) with the entry's motion.
fn start_pair(f: &mut Follower, timer: f32, entry: &[u8; 0x10], target: u32, flag: u8) {
    f.reset();
    f.timer = timer;
    f.forced = 1;
    f.kind = 5;
    f.target = target;
    f.mode = 1;
    f.flag = flag;
    f.motion = entry_motion(entry[1]);
}

/// `FUN_0018b308(lo, hi, follower, position, count, entries, region)`: a weighted draw among the
/// return entries keyed to the region (byte 0 equal to the volume record's +2): a number below 100
/// (`FUN_00106308`) falls into the entry whose byte 2 (its weight) covers it; entries whose byte 1
/// is 3 take no part in the draw and the last of them gives the flag (+0x24) with the chance of
/// its byte 2. The chosen entry makes a kind-6 follower when its byte 1 is 1 (the script then
/// stops the walker for good), else a kind-2 one, with the motion of its byte 1, the target of its
/// byte 3 (1 an object by the objentry id at +4, 2 the player, 3 the point at +4..+0xc, else
/// none), the region, a random threshold across it, the place across it now and a timer from
/// `lo..hi`. Answers whether an entry was chosen.
#[allow(clippy::too_many_arguments)]
pub fn pick_entry(f: &mut Follower, rng: &mut Rng, group: &NpcGroup, region: usize, pos: [f32; 3], lo: f32, hi: f32, by_id: impl Fn(u32) -> u32) -> bool {
    let Some(vol) = group.activators.get(region) else { return false };
    let roll = rng.below(100);
    let (mut sum, mut chosen, mut flag_entry) = (0u32, None, None);
    for e in group.returns.iter().filter(|e| e[0] as u16 == vol.option) {
        if e[1] == 3 {
            flag_entry = Some(*e);
            continue;
        }
        let before = sum;
        sum += e[2] as u32;
        if before <= roll && roll < sum {
            chosen = Some(*e);
        }
    }
    let Some(e) = chosen else { return false };
    f.reset();
    f.kind = if e[1] == 1 { 6 } else { 2 };
    f.motion = entry_motion(e[1]);
    let word = |o: usize| [e[o], e[o + 1], e[o + 2], e[o + 3]];
    match e[3] {
        1 => {
            f.mode = 1;
            f.target = by_id(u32::from_le_bytes(word(4)));
        }
        2 => f.mode = 2,
        3 => {
            f.mode = 3;
            f.point = [f32::from_le_bytes(word(4)), f32::from_le_bytes(word(8)), f32::from_le_bytes(word(12)), 0.0];
        }
        _ => f.mode = 0,
    }
    f.flag = 0;
    if let Some(fe) = flag_entry
        && rng.below(100) < fe[2] as u32
    {
        f.flag = 1;
    }
    f.region = Some(region);
    f.threshold = rng.frac(1.0);
    f.start = region_coord(vol, pos);
    f.timer = rng.range(lo, hi);
    true
}

/// `FUN_0018a4b0(obj)`: the region the object stands in (its position 10 cm higher,
/// `DAT_0037ab88`) is looked up in its group (`FUN_00199d68`); when it differs from the last one
/// (+0xa30) and the follower is not done, the follower is reset and, inside a region, a return
/// entry of that region is drawn (`FUN_0018b308` with the timer range +0xb28 / +0xb2c).
fn region_step(minds: &mut [Mind], i: usize) {
    let p = to_game(minds[i].pos);
    let probe = [p[0], p[1] - 10.0, p[2]];
    let group = minds[i].fields.group.clone();
    let r = group.region_at(probe);
    if r == minds[i].fields.region {
        return;
    }
    if minds[i].fields.follower.done == 0 {
        minds[i].fields.follower.reset();
        let Some(region) = r else {
            minds[i].fields.region = None;
            return;
        };
        let ids: Vec<(u32, u32)> = minds.iter().map(|m| (m.object, m.handle)).collect();
        let by_id = |id: u32| if id == OBJECT_PLAYER { PLAYER } else { ids.iter().find(|(o, _)| *o == id).map_or(0, |(_, h)| *h) };
        let (lo, hi) = (minds[i].fields.param(6), minds[i].fields.param(7));
        let f = &mut *minds[i].fields;
        pick_entry(&mut f.follower, &mut f.rng, &group, region, [p[0], p[1], p[2]], lo, hi, by_id);
    }
    minds[i].fields.region = r;
}

/// `FUN_0018b148(follower, position)`: true when the follower is forced (+0x31); with a region
/// (+0x20) the object's place across it (`FUN_001d8c18`) against the threshold (+0x28): at or below
/// it when the follower started in the upper half (+0x2c >= 0.5, `DAT_0037ab98`), at or above it
/// otherwise; false without a region. `p` is in the game's space.
pub fn follower_test(f: &Follower, group: &NpcGroup, p: [f32; 3]) -> bool {
    if f.forced != 0 {
        return true;
    }
    let Some(vol) = f.region.and_then(|r| group.activators.get(r)) else { return false };
    let at = region_coord(vol, p);
    if f.start >= 0.5 { at <= f.threshold } else { f.threshold <= at }
}

/// What the class pass reports for the log.
#[derive(Debug, PartialEq)]
pub enum Attention {
    None,
    /// A kind-4 follower: the player came into the talk part.
    Player,
    /// A kind-5 pair with the townsperson at this index.
    Pair(usize),
}

/// `FUN_0018a130(obj)` for townsperson `i`, once per frame while bit 0 of its array is clear (the
/// object is not linked, `FUN_001beb00`, in the app). The follower's timer and +0xb00 count down;
/// then, unless the follower already looks at the player (kind 4) or the last one did and +0xb00
/// still runs: the player's body meeting the object's talk part (`FUN_0018aa08(obj, 5)`) starts a
/// forced kind-4 follower toward the player (mode 2) with a timer from +0xb10..+0xb14. Else, unless
/// the follower is of kind 4 or 5 (or the last one was 5 and +0xb00 runs), a partner
/// (`FUN_0018a598`) starts a kind-5 pair on both objects with one timer from +0xb1c..+0xb20: this
/// one's flag set, the partner's set when the two face away from each other (the dot product of
/// their facings, `FUN_0016a890`, is negative). Else, once +0xb00 ran out, the region step
/// (`FUN_0018a4b0`).
pub fn class_tick(minds: &mut [Mind], i: usize, player_body: Option<&parts::Shape>) -> Attention {
    {
        let f = &mut *minds[i].fields;
        if f.follower.timer > 0.0 {
            f.follower.timer -= 1.0;
        }
        if f.timer_b00 > 0.0 {
            f.timer_b00 -= 1.0;
        }
    }
    let meets = matches!((minds[i].talk, player_body), (Some(t), Some(b)) if parts::overlaps(&t, b) == Some(true));
    minds[i].fields.player_in_part = meets;
    let kind = minds[i].fields.follower.kind;
    if kind != 4 && (minds[i].fields.prev.kind != 4 || minds[i].fields.timer_b00 <= 0.0) && meets {
        let f = &mut *minds[i].fields;
        f.keep_previous();
        f.follower.reset();
        f.follower.timer = {
            let (lo, hi) = (f.param(0), f.param(1));
            f.rng.range(lo, hi)
        };
        f.follower.forced = 1;
        f.follower.kind = 4;
        f.follower.flag = 0;
        f.follower.mode = 2;
        return Attention::Player;
    }
    if !(kind == 4 || kind == 5) {
        if minds[i].fields.prev.kind == 5 && minds[i].fields.timer_b00 > 0.0 {
            return Attention::None;
        }
        if let Some((j, mine, theirs)) = partner(minds, i) {
            minds[i].fields.keep_previous();
            let timer = {
                let f = &mut *minds[i].fields;
                let (lo, hi) = (f.param(3), f.param(4));
                f.rng.range(lo, hi)
            };
            let facing = |m: &Mind| Vec3::new(m.yaw.sin(), 0.0, m.yaw.cos());
            let away = facing(&minds[i]).dot(facing(&minds[j])) < 0.0;
            let (hi_, hj) = (minds[i].handle, minds[j].handle);
            start_pair(&mut minds[i].fields.follower, timer, &mine, hj, 1);
            start_pair(&mut minds[j].fields.follower, timer, &theirs, hi_, away as u8);
            return Attention::Pair(j);
        }
    }
    if minds[i].fields.timer_b00 <= 0.0 {
        minds[i].fields.timer_b00 = 0.0;
        region_step(minds, i);
    }
    Attention::None
}

/// What host 5.11 and 5.6 read of the other townspeople (taken after the class pass).
#[derive(Clone, Copy, Debug)]
pub struct Peer {
    pub pos: Vec3,
    /// The follower's target (+0xa4c).
    pub target: u32,
    /// The motion last requested (+0x154).
    pub slot: Option<usize>,
}

// ---------------------------------------------------------------------------------------------------
// The walk path

/// The object's walk path as `FUN_002d3b40` builds it from a group's `WalkPath` (a 0xa0-byte record
/// at object +0x508, ctor `FUN_002d2100`): the points (record +0x80, 0x20 bytes each: the position
/// and its spline tangent, whose w holds the point's index), whether it loops (+0x10, header byte
/// +4 bit 0), the tension (+0x20: 0.5, `DAT_003837fc`, unless header bit 1), the place (+0x18), the
/// walking parameter (+0x1c, -2 until the first advance, `DAT_0038379c`), the current segment
/// (+0x30/+0x40 its start point and tangent, +0x50/+0x60 its end, +0x28 the chord between them),
/// the direction (+0x84), "turn around at the ends" (+0x85) and the lock (+0x86). The curve between
/// two points is the cubic Hermite of `FUN_002d4298` / `FUN_002d2460`. Positions are kept in the
/// app's space (the game's mirrored: lengths, the Hermite and the horizontal distances agree).
#[derive(Clone, Debug, PartialEq)]
pub struct PathRun {
    pub id: u16,
    /// The points in the app's space (centimetres).
    pub points: Vec<Vec3>,
    /// The tangent of each point (`FUN_002d32f8`).
    pub tangents: Vec<Vec3>,
    /// Record +0x10: the path closes on itself (cleared for paths of fewer than three points).
    pub loops: bool,
    /// Record +0x20: the tangents' factor.
    pub tension: f32,
    /// Record +0x18: the place on the path, segment index plus fraction.
    pub t: f32,
    /// Record +0x1c: the walking parameter (the start index of the segment plus the Hermite
    /// parameter inside it).
    pub cur: f32,
    /// The current segment's start and end point indices (the tangents' w at +0x4c and +0x6c).
    pub seg: (usize, usize),
    /// Record +0x28: the chord of the current segment.
    pub chord: f32,
    /// Record +0x84: walking from lower to higher indices.
    pub forward: bool,
    /// Record +0x85: at an end the direction turns around (`FUN_002d2328`); clear when the path
    /// is locked (+0x86, bit 2 of the class-5 bit array).
    pub bounce: bool,
}

/// `FUN_002d4298(s, out, path, a, b)`: the cubic Hermite between point `a` (position `p0`, tangent
/// `t0`) and point `b` at parameter `s`: the weights 2s^3 - 3s^2 + 1, -2s^3 + 3s^2, s^3 - 2s^2 + s
/// and s^3 - s^2 (`DAT_00383818` = 1, `DAT_0038381c` = 3, `DAT_00383820` = -2).
pub fn hermite(p0: Vec3, t0: Vec3, p1: Vec3, t1: Vec3, s: f32) -> Vec3 {
    let (s2, s3) = (s * s, s * s * s);
    p0 * (s3 + s3 - s2 * 3.0 + 1.0) + p1 * (s3 * -2.0 + s2 * 3.0) + t0 * (s3 - (s2 + s2) + s) + t1 * (s3 - s2)
}

/// `FUN_002d4040(path, &dist, p, a, b)`: the point's distance to the chord from `a` to `b` and its
/// place along it: before `a` the distance to `a` and 0; past `b` the distance to `b` and 1
/// (`DAT_00383814`); else the distance to the chord and the projection over the chord's length.
fn chord_place(p: Vec3, a: Vec3, b: Vec3) -> (f32, f32) {
    let (ab, ap) = (b - a, p - a);
    if ab.dot(ap) < 0.0 {
        return (ap.length(), 0.0);
    }
    let len = ab.length();
    let proj = if len > 0.0 { (ab / len).dot(ap).abs() } else { 0.0 };
    if proj <= len {
        let h = ap.length_squared() - proj * proj;
        (if h <= 0.0 { 0.0 } else { h.sqrt() }, if len > 0.0 { proj / len } else { 0.0 })
    } else {
        ((p - b).length(), 1.0)
    }
}

impl PathRun {
    /// `FUN_002d3b40` for a class-5 object: the points (with every height taken from the first
    /// point when bit 3 of the object's bit array is set), the loop flag and the tension from the
    /// header byte +4, the tangents (`FUN_002d32f8`); the place is set by the caller.
    pub fn new(id: u16, p: &kh2_ard::WalkPath, flatten: bool) -> PathRun {
        let first_y = p.points.first().map_or(0.0, |q| q[1]);
        let points: Vec<Vec3> = p.points.iter().map(|q| game_to_app(if flatten { [q[0], first_y, q[2]] } else { *q })).collect();
        let mut r = PathRun {
            id,
            tangents: vec![Vec3::ZERO; points.len()],
            points,
            loops: p.header[4] & 1 != 0,
            tension: if p.header[4] & 2 == 0 { 0.5 } else { 0.0 },
            t: 0.0,
            cur: -2.0,
            seg: (0, 0),
            chord: 0.0,
            forward: true,
            bounce: true,
        };
        r.make_tangents();
        r
    }

    /// `FUN_002d32f8`: with fewer than three points the one tangent is the chord, both points get
    /// it and the loop flag is cleared. Else every point gets (next - previous) * tension (indices
    /// wrapping), except the two ends of a path that does not loop: there the quadratic through the
    /// end and its two neighbours (passing the middle one at the share `f` of the first chord in the
    /// sum of both) gives the direction at the end, scaled so that its length at the middle point
    /// equals the inner rule's length there.
    fn make_tangents(&mut self) {
        let n = self.points.len();
        let p = &self.points;
        if n < 3 {
            if n == 2 {
                self.tangents = vec![p[1] - p[0]; 2];
            }
            self.loops = false;
            return;
        }
        let end = |a: Vec3, b: Vec3, c: Vec3, at_start: bool, tension: f32| {
            let (la, lb) = ((b - a).length(), (c - b).length());
            let f = la / (la + lb);
            let g = 1.0 - f;
            let m = (b - a * (g * g) - c * (f * f)) / ((f + f) * g);
            let d = if at_start { (m - a) * 2.0 } else { (c - m) * 2.0 };
            let u = a * (g * -2.0) + m * (2.0 - f * 4.0) + c * (2.0 * f);
            d * (((c - a) * tension).length() / u.length())
        };
        for i in 0..n {
            self.tangents[i] = if !self.loops && i == 0 {
                end(p[0], p[1], p[2], true, self.tension)
            } else if !self.loops && i == n - 1 {
                end(p[n - 3], p[n - 2], p[n - 1], false, self.tension)
            } else {
                (p[(i + 1) % n] - p[(i + n - 1) % n]) * self.tension
            };
        }
    }

    fn last(&self) -> usize {
        self.points.len().saturating_sub(1)
    }

    /// The Hermite of segment `i` (from point `i` to point `i + 1`, wrapping) at parameter `s`.
    pub fn eval(&self, i: usize, s: f32) -> Vec3 {
        let n = self.points.len();
        match n {
            0 => Vec3::ZERO,
            1 => self.points[0],
            _ => {
                let (a, b) = (i % n, (i + 1) % n);
                hermite(self.points[a], self.tangents[a], self.points[b], self.tangents[b], s)
            }
        }
    }

    /// The point at a place (segment index plus fraction; `FUN_002d4438` -> `FUN_002d43a0`).
    pub fn at(&self, t: f32) -> Vec3 {
        let i = t.max(0.0) as usize;
        self.eval(i, t - i as f32)
    }

    /// `FUN_002d4dc8(path, &seg, &frac, p)`: the segment whose chord is nearest to the point (two
    /// points: the one segment; else every segment, the last one only for a loop; the first of
    /// equals wins) and the place on it: the chord's share, refined, when strictly inside
    /// (`DAT_0038385c` .. `DAT_00383860`), by 33 halvings of the curve (`DAT_00383868`) toward the
    /// half whose chord is nearer. Answers (segment, fraction, chord distance).
    pub fn nearest(&self, p: Vec3) -> (usize, f32, f32) {
        let n = self.points.len();
        if n < 2 {
            return (0, 0.0, 0.0);
        }
        let count = if n == 2 { 1 } else { n - usize::from(!self.loops) };
        let mut best = (0usize, 0.0f32, 0.0f32);
        for i in 0..count {
            let (d, f) = chord_place(p, self.points[i], self.points[(i + 1) % n]);
            if i == 0 || (0.0 <= d && d < best.2) {
                best = (i, f, d);
            }
        }
        let (i, f, d) = best;
        if n == 2 || f <= 1e-4 || f32::from_bits(0x3f7f_f972) <= f {
            return (i, f, d);
        }
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        let (mut a, mut b) = (self.points[i], self.points[(i + 1) % n]);
        let mut s = hi + lo;
        for _ in 0..33 {
            s *= 0.5;
            let m = self.eval(i, s);
            if chord_place(p, a, m).0 < chord_place(p, m, b).0 {
                hi = s;
                b = m;
            } else {
                lo = s;
                a = m;
            }
            s = lo + hi;
        }
        (i, s * 0.5, d)
    }

    /// `FUN_002d4688(lo, hi, path, &frac, p, a, b)`: 33 halvings of segment `i` between the
    /// parameters `lo` and `hi`, keeping the half whose chord (from the segment's start point or the
    /// last kept point to the middle, and on to the end point or the last kept point) is nearer.
    /// Answers the fraction and the distance of the last comparison (the larger of the two).
    fn search_segment(&self, i: usize, mut lo: f32, mut hi: f32, p: Vec3) -> (f32, f32) {
        let n = self.points.len();
        let (mut a, mut b) = (self.points[i % n], self.points[(i + 1) % n]);
        let mut d = 0.0;
        let mut s = lo + hi;
        for _ in 0..33 {
            s *= 0.5;
            let m = self.eval(i, s);
            let (da, db) = (chord_place(p, a, m).0, chord_place(p, m, b).0);
            if da < db {
                d = db;
                hi = s;
                b = m;
            } else {
                d = da;
                lo = s;
                a = m;
            }
            s = lo + hi;
        }
        ((lo + hi) * 0.5, d)
    }

    /// The local search of `FUN_002d51e0` around the current place: `FUN_002d4ad8` for a loop,
    /// `FUN_002d4800` for an open path. From the place (+0x18), moved over a segment boundary when
    /// it sits on one in the walking direction (1/10000, `DAT_0038384c` / `DAT_00383850`), the rest
    /// of the current segment ahead (in the walking direction) and the next segment(s) ahead are
    /// searched (`FUN_002d4688`); the nearer wins. Answers (segment, fraction, distance) or `None`
    /// when the distance is not below `reach` (the game's -1, `DAT_00383858` / `DAT_00383848`).
    fn local_nearest(&self, p: Vec3, reach: f32) -> Option<(usize, f32, f32)> {
        let n = self.points.len() as i64;
        let mut seg = self.t.max(0.0) as i64;
        let mut frac = self.t - seg as f32;
        if self.forward && frac > f32::from_bits(0x3f7f_f972) {
            seg += 1;
            frac = 0.0;
        } else if !self.forward && frac < 1e-4 {
            seg -= 1;
            frac = 1.0;
        }
        let (lo, hi) = if self.forward { (frac, 1.0) } else { (0.0, frac) };
        if self.loops {
            let seg = seg.rem_euclid(n);
            let (f1, d1) = self.search_segment(seg as usize, lo, hi, p);
            let seg2 = if self.forward { (seg + 1) % n } else { (seg + n - 1) % n };
            let (f2, d2) = self.search_segment(seg2 as usize, 0.0, 1.0, p);
            let best = if d1 < d2 { (seg as usize, f1, d1) } else { (seg2 as usize, f2, d2) };
            return (best.2 < reach).then_some(best);
        }
        if seg < 0 {
            return Some((0, 0.0, (p - self.points[0]).length())).filter(|b| b.2 < reach);
        }
        if seg >= n - 1 {
            // [inferred] FUN_002d4800 answers the last point's distance with segment n - 1 (which
            // FUN_002d51e0 then pairs with point 0); the place is clamped to the path's end here.
            let last = (n - 1) as usize;
            return Some((last.saturating_sub(1), 1.0, (p - self.points[last]).length())).filter(|b| b.2 < reach);
        }
        let (mut lo, mut hi) = (lo, hi);
        let mut best: Option<(usize, f32, f32)> = None;
        let mut cur = seg;
        for _ in 0..2 {
            let (f, d) = self.search_segment(cur as usize, lo, hi, p);
            if best.is_none_or(|b| d < b.2) {
                best = Some((cur as usize, f, d));
            }
            let next = if self.forward { cur + 1 } else { cur - 1 };
            if next < 0 || next >= n - 1 {
                break;
            }
            cur = next;
            (lo, hi) = (0.0, 1.0);
        }
        best.filter(|b| b.2 < reach)
    }

    /// Takes segment `i` as the current one (+0x30..+0x60 and the chord +0x28).
    fn set_seg(&mut self, i: usize) {
        let n = self.points.len().max(1);
        self.seg = (i % n, (i + 1) % n);
        self.chord = (self.points[self.seg.1] - self.points[self.seg.0]).length();
    }

    /// `FUN_002d50c0(path, p)`: the place nearest to the point becomes the path's place and its
    /// segment the current one.
    pub fn place_nearest(&mut self, p: Vec3) {
        if self.points.len() < 2 {
            return;
        }
        let (i, f, _) = self.nearest(p);
        self.set_seg(i);
        self.t = i as f32 + f;
    }

    /// `FUN_002d5398(path, obj)`: a class-5 object whose bit 1 is clear starts at a random place of
    /// its path (`FUN_001063b8` of the segment count) walking a random way (`FUN_00106308(2)`);
    /// the object is put on that point (`FUN_0016a618`), which is answered. The segment of the
    /// fraction's point is the place's integer part [inferred: the argument is not in the decompile].
    pub fn random_start(&mut self, rng: &mut Rng) -> Vec3 {
        let segs = if self.loops { self.points.len() } else { self.points.len().saturating_sub(1) };
        let t = rng.frac(segs as f32);
        self.forward = rng.below(2) != 0;
        self.t = t;
        self.at(t)
    }

    /// `FUN_002d2328`: the direction flips unless the path is locked; answers the new direction.
    pub fn toggle(&mut self) -> bool {
        if self.bounce {
            self.forward = !self.forward;
        }
        self.forward
    }

    /// `FUN_002d2a98(speed, path, out, position)` for an ordinary path (+0x87 clear, no stop index
    /// +0x8c): the object is first taken onto the curve (`FUN_002d51e0` with the reach `speed * 10`:
    /// the nearest place, unless it is as far from the object as the old place within 1/10000,
    /// `DAT_003837d4`, then the old place stays) and the walking parameter starts there. Then, up
    /// to twenty times, the curve point at the parameter is the answer when it lies more than
    /// `speed` from the projected point (horizontally), else the parameter moves `speed / chord / 2`
    /// (`DAT_003837dc`) along: past the segment's end it takes the next segment (wrapping on a loop);
    /// at an end of a path that does not loop the direction turns around (+0x85, then one more
    /// pass) or the walk stops (`None`). After the passes the projected point is the answer.
    /// Answers (the point to walk to, the projected point).
    pub fn advance(&mut self, speed: f32, pos: Vec3) -> Option<(Vec3, Vec3)> {
        let n = self.points.len();
        if n < 2 {
            return None;
        }
        let old = self.t;
        let old_point = self.at(old);
        // FUN_002d51e0: the local search within `speed * 10` (`DAT_003837d0`) for more than two
        // points, else (or when it fails) the global one.
        let local = if n > 2 { self.local_nearest(pos, speed * 10.0) } else { None };
        let (i, f, _) = local.unwrap_or_else(|| self.nearest(pos));
        self.set_seg(i);
        self.t = i as f32 + f;
        let mut proj = self.eval(i, f);
        let (d_old, d_new) = ((old_point - pos).length(), (proj - pos).length());
        if (d_new - d_old).abs() < (d_old + d_new) * 1e-4 {
            self.t = old;
            proj = old_point;
        }
        self.cur = self.t;
        let last = self.last();
        for pass in 0..2 {
            let mut turned = false;
            for _ in 0..20 {
                let (s, e) = self.seg;
                let sample = hermite(self.points[s], self.tangents[s], self.points[e], self.tangents[e], self.cur - s as f32);
                if (sample - proj).with_y(0.0).length() > speed {
                    return Some((sample, proj));
                }
                let step = speed / self.chord.max(1e-6) * 0.5;
                if self.forward {
                    self.cur += step;
                    if self.cur < s as f32 + 1.0 {
                        continue;
                    }
                    let mut next = (self.cur + 1.0) as usize;
                    if next > last {
                        if !self.loops {
                            if !self.bounce {
                                return None;
                            }
                            self.toggle();
                            turned = true;
                            break;
                        }
                        next = next - last - 1;
                    }
                    self.cur = e as f32;
                    self.seg = (e, next % n);
                } else {
                    self.cur -= step;
                    if self.cur > s as f32 {
                        continue;
                    }
                    let prev = if s == 0 { last } else { s - 1 };
                    if self.cur <= 0.0 {
                        if !self.loops {
                            if !self.bounce {
                                return None;
                            }
                            self.toggle();
                            turned = true;
                            break;
                        }
                        self.cur = (last + 1) as f32;
                    } else {
                        self.cur = s as f32;
                    }
                    self.seg = (prev, s);
                }
                self.chord = (self.points[self.seg.1] - self.points[self.seg.0]).length();
            }
            if !turned || pass == 1 {
                break;
            }
        }
        Some((proj, proj))
    }
}

// ---------------------------------------------------------------------------------------------------
// The host

/// The townsperson's script host: the group 5 library and the group 1 path hosts, the rest the
/// enemy host's.
pub(crate) struct NpcHost<'a, 'b> {
    pub(crate) inner: BevyHost<'a>,
    pub(crate) fields: &'b mut NpcFields,
    /// The townspeople by script handle as the class pass left them this frame.
    pub(crate) peers: &'b HashMap<u32, Peer>,
    pub(crate) log: bool,
}

fn words(c: &Call) -> Option<(u16, u16, Vec<u32>)> {
    match c {
        Call::Unknown { group, index, args } => Some((*group, *index, args.clone())),
        Call::Func { group, index, args } => Some((
            *group,
            *index,
            args.iter()
                .map(|a| match a {
                    Arg::Float(f) => f.to_bits(),
                    a => a.int() as u32,
                })
                .collect(),
        )),
        _ => None,
    }
}

/// A 16-byte vector answer in the game's space.
fn vec_value(v: Vec3) -> Value {
    Value::Vec(to_game(v))
}

impl NpcHost<'_, '_> {
    fn trace(&mut self) -> &mut AiTrace {
        &mut *self.inner.trace
    }

    /// Host 1.177 (0x1719d0 -> `FUN_00183c30(obj, id, 0)`): the walk path with this id in the
    /// object's own spawn group replaces the one at +0x508 (`FUN_00199cc0` compares the path
    /// header's u16 id; a negative id takes the group's first path, which is what `rtn_default`
    /// asks for with -1). `FUN_002d3b40` for a class-5 object: bit 3 of its array flattens the
    /// points to the first one's height, bit 2 locks the path (+0x86; +0x85 "turn around" is its
    /// opposite); with bit 1 clear the object is put at a random place of the path walking a random
    /// way (`FUN_002d5398`), with bit 1 set it starts at the place nearest to it (`FUN_002d50c0`).
    /// Answers 1 when a path was found, 0 otherwise (the game then also tries the area's shared
    /// paths for objects with flag 0x588 bit 7, which these are not).
    fn set_path(&mut self, id: u32) -> bool {
        let pos = self.inner.obj.motor.pos;
        let paths = &self.fields.group.paths;
        let found = if (id as i32) < 0 { paths.first().cloned() } else { paths.iter().find(|p| u16::from_le_bytes([p.header[0], p.header[1]]) as u32 == id).cloned() };
        match found {
            Some(p) => {
                let mut run = PathRun::new(u16::from_le_bytes([p.header[0], p.header[1]]), &p, self.fields.test_bit(3));
                let locked = self.fields.test_bit(2);
                run.bounce = !locked;
                if self.fields.test_bit(1) {
                    run.place_nearest(pos);
                } else {
                    let at = run.random_start(&mut self.fields.rng);
                    let at = self.inner.env.world.and_then(|w| w.ground(at, at.y + 100.0, 400.0)).map_or(at, |g| at.with_y(g.y));
                    self.inner.obj.motor.pos = at;
                }
                if self.log {
                    let at = self.inner.obj.motor.pos;
                    info!(
                        "npc: {} path {} (asked {}) set: {} points, loops {}, locked {locked}, start t {:.2} forward {} at ({:.0}, {:.0}, {:.0})",
                        self.inner.tag,
                        run.id,
                        id as i32,
                        run.points.len(),
                        run.loops,
                        run.t,
                        run.forward,
                        at.x,
                        at.y,
                        at.z
                    );
                }
                self.fields.path = Some(run);
                true
            }
            None => {
                let tag = self.inner.tag;
                let msg = format!("path {id} is not in group {} ({} paths)", self.fields.group.tag, self.fields.group.paths.len());
                self.trace().note(tag, msg);
                self.fields.path = None;
                false
            }
        }
    }

    /// `FUN_002d25b0(speed, blend, out, path, position, facing)` behind hosts 5.3 and 1.178 (both
    /// with blend 1.0, `DAT_00383e80` / `DAT_003837c4`): the speed is clipped to 50
    /// (`DAT_003837b8`) and the path advanced (`FUN_002d2a98`); when the walk stops the answer is
    /// the facing. Else the unit direction from the object to the point found when that point is
    /// more than ten times the speed away (`DAT_003837c0`); nearer, the blend of it with the unit
    /// direction from the object's place on the curve, which with blend 1 is the latter alone.
    fn path_direction(&mut self, speed: f32) -> Vec3 {
        let yaw = self.inner.obj.motor.yaw;
        let facing = Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let pos = self.inner.obj.motor.pos;
        let speed = speed.min(50.0);
        let Some(path) = self.fields.path.as_mut() else { return facing };
        match path.advance(speed, pos) {
            Some((target, _)) if (target - pos).length() > speed * 10.0 => (target - pos).normalize_or(facing),
            // [approx] the game divides by a zero length when the passes end on the projected point
            // itself; the facing is answered then.
            Some((target, proj)) => (target - proj).normalize_or(facing),
            None => facing,
        }
    }

    fn group5(&mut self, index: u16, a: &[u32]) -> Option<(Value, St, &'static str)> {
        let arg = |k: usize| a.get(k).copied().unwrap_or(0);
        let me = self.inner.obj.handle;
        Some(match index {
            // 5.0 (0x2ea050 -> FUN_0018a8b0): while the follower is not done, FUN_0018b148 tests it:
            // true when it is forced (+0x31); with a region (+0x20) the object's place across it
            // (FUN_001d8c18) is compared with the threshold (+0x28): at or below it when the
            // follower started in the upper half (+0x2c >= 0.5, DAT_0037ab98), at or above it
            // otherwise; without a region false. A passing test sets +0x30 (FUN_0018b138(f, 1)).
            0 => {
                let p = to_game(self.inner.obj.motor.pos);
                let group = self.fields.group.clone();
                let f = &mut self.fields.follower;
                let pass = f.done == 0 && follower_test(f, &group, [p[0], p[1], p[2]]);
                if pass {
                    f.done = 1;
                }
                (Value::Int(pass as i32), St::Impl, "")
            }
            // 5.2 (0x2ea230 -> FUN_002d2328): flips the walk direction of the object's path (+0x84)
            // unless the path is locked (+0x86) and answers the new direction; 0 without a path.
            2 => match self.fields.path.as_mut() {
                Some(p) => {
                    let d = p.toggle();
                    (Value::Int(d as i32), St::Impl, "")
                }
                None => (Value::Int(0), St::Impl, ""),
            },
            // 5.3 (0x2ea080 -> FUN_002d25b0): the direction along the object's path for a speed (the
            // second argument, a float); nothing without a path (the answer stays what the buffer held).
            3 => {
                if self.fields.path.is_none() {
                    return Some((Value::Vec([0.0; 4]), St::Approx, "no path at +0x508: the game leaves the answer buffer as it was"));
                }
                let d = self.path_direction(f32::from_bits(arg(1)));
                (Value::Vec([d.x, -d.y, -d.z, 1.0]), St::Approx, "FUN_002d2a98 ported for the ordinary path (+0x87 clear, no stop index +0x8c); a zero direction answers the facing")
            }
            // 5.4 (0x2ea290 -> FUN_0018ae08): the follower ends (copied to +0xaa0, reset) and the
            // +0xb00 timer takes the parameter word of its kind.
            4 => {
                self.fields.end_follower();
                (Value::None, St::Impl, "")
            }
            // 5.5 (0x2ea2b0 -> FUN_0018aef8): the follower's first three words (kind, mode, motion)
            // copied to a static buffer and returned as a vector.
            5 => {
                let f = &self.fields.follower;
                (Value::Vec([f32::from_bits(f.kind as u32), f32::from_bits(f.mode as u32), f32::from_bits(f.motion as u32), 0.0]), St::Impl, "")
            }
            // 5.6 (0x2ea300 -> FUN_0018af10): the unit direction from the object (its height taken
            // as 0) to the follower's target: mode 2 the player and mode 1 the target object (when
            // it still exists, FUN_0019a3f8) at height 0, mode 3 the point with its own height; the
            // object's facing (+0x550, FUN_0016a890) when the follower is not live (FUN_0018b110:
            // forced or with a region) or has no target mode. Kind 3 answers the saved vector at
            // +0xa80 instead, a follower the class never makes here (not ported: the facing).
            6 => {
                let yaw = self.inner.obj.motor.yaw;
                let facing = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                let own = self.inner.obj.motor.pos.with_y(0.0);
                let f = &self.fields.follower;
                let target = match (f.live() && f.kind != 3, f.mode) {
                    (true, 2) => Some(self.inner.env.player.with_y(0.0)),
                    (true, 1) if f.target == PLAYER => Some(self.inner.env.player.with_y(0.0)),
                    (true, 1) if f.target == me => Some(own),
                    (true, 1) => self.peers.get(&f.target).map(|p| p.pos.with_y(0.0)),
                    (true, 3) => Some(from_game(f.point)),
                    _ => None,
                };
                match target {
                    Some(t) => (vec_value((t - own).normalize_or(facing)), St::Impl, ""),
                    None => (vec_value(facing), St::Impl, ""),
                }
            }
            // 5.7 (0x2ea350 -> FUN_0018af08): the byte at object +0xa64 (the follower's +0x24).
            7 => (Value::Int(self.fields.follower.flag as i32), St::Impl, ""),
            // 5.11 (0x2ea618 -> FUN_00189e70 -> FUN_0018b250): "the action is over". 1 unless the
            // follower is done (+0x30) and its timer (+0x50) runs; then a kind-4 follower holds (0)
            // while the player's body still meets the object's talk part (FUN_0018aa08), a kind-5
            // one while the partner (+0xc) exists, points back at this object (its +0xa4c) and does
            // not play motion 0x32 (its +0x154); every other kind holds until the timer ends.
            11 => {
                let f = &self.fields.follower;
                let v = if f.done == 0 || f.timer <= 0.0 || (f.kind == 4 && !self.fields.player_in_part) {
                    1
                } else if f.kind == 5 {
                    match self.peers.get(&f.target) {
                        Some(p) if p.target == me => (p.slot == Some(0x32)) as i32,
                        _ => 1,
                    }
                } else {
                    0
                };
                (Value::Int(v), St::Impl, "")
            }
            // 5.16 (0x2ea768 -> FUN_002d3148): with a path and a second object, decides the walk
            // direction (+0x84) from the dot products of the direction to that object and the path's
            // tangent at its nearest point; nothing without a path.
            16 => match self.fields.path.as_mut() {
                Some(p) => {
                    // The second argument is the other object's context (word +4 = the object);
                    // only the player is known to this host.
                    let other = self.inner.env.player;
                    let pos = self.inner.obj.motor.pos;
                    let to = (other - pos).with_y(0.0);
                    if to.length() >= 0.01 {
                        let (i, f, _) = p.nearest(pos);
                        let tangent = (p.eval(i, f + 0.01) - p.eval(i, f - 0.01)).with_y(0.0).normalize_or_zero();
                        if tangent.length() >= 0.01 {
                            let to = to.normalize();
                            p.forward = to.dot(tangent) >= to.dot(-tangent);
                        }
                    }
                    (Value::None, St::Approx, "the other object is taken as the player; FUN_002d3148's own rule is not read, the curve's tangent at the nearest place is compared")
                }
                None => (Value::None, St::Impl, ""),
            },
            // 5.17 (0x2ea7b0 -> FUN_00189938): bit n of the array at +0xb04.
            17 => (Value::Int(self.fields.test_bit(arg(1)) as i32), St::Impl, ""),
            // 5.26 (0x2ea978, six instructions): object +0xb08 = the second argument's object (the
            // word at +4 of its context). Only the own context is known here.
            26 => {
                let ctx = arg(1);
                let mut b = [0u8; 4];
                let v = if self.inner.read_mem(ctx.wrapping_add(4), &mut b) { u32::from_le_bytes(b) } else { 0 };
                self.fields.word_b08 = v;
                if v == 0 {
                    (Value::None, St::Approx, "the second context is not in this object's memory: +0xb08 takes 0")
                } else {
                    (Value::None, St::Impl, "")
                }
            }
            // 5.27 (0x2ea7e8 -> FUN_00189960): set bit n of the array at +0xb04.
            27 => {
                self.fields.set_bit(arg(1));
                (Value::None, St::Impl, "")
            }
            // 5.33 (0x2ea808 -> FUN_00189990): the nine words at the second argument copied to
            // object +0xb10 (the class's parameter block). `rtn_default` keeps the block in its
            // context (script globals), which the host reads after the tick (`read_params`).
            33 => {
                self.fields.params_ptr = Some(arg(1));
                (Value::None, St::Impl, "")
            }
            // 5.34 (0x2ea380 -> FUN_0016bbe8): a point of the object: the position of its part of
            // kind 0x11 (FUN_001894d0, a collision record) when it has one, else its lock-on point
            // (FUN_0016b6e0: a target record on the object). The app's townspeople have no parts:
            // the object's position.
            34 => (vec_value(self.inner.obj.motor.pos), St::Approx, "the kind-0x11 part's point (FUN_001894d0) is not read; the object's position"),
            _ => return None,
        })
    }

    /// `FUN_001af650(obj, point)` behind hosts 1.172 (the point is the player's lock-on point,
    /// `FUN_0016bbe8`) and 1.173: without a part of kind 0x14 on the object, the angle between its
    /// facing (+0x550) and the horizontal direction to the point is below `DAT_0037b950` (15
    /// degrees); with one, the part's own cone (its halfword +0x10 in degrees) and point are used.
    fn in_front(&self, point: Vec3) -> bool {
        in_cone(self.inner.obj.motor.pos, self.inner.obj.motor.yaw, point, self.fields.cone_part)
    }

    fn group1(&mut self, index: u16, a: &[u32], c: &Call) -> Option<(Value, St, &'static str)> {
        let arg = |k: usize| a.get(k).copied().unwrap_or(0);
        let me = self.inner.obj.handle;
        Some(match index {
            // 1.161 has_link (0x1706c8 -> FUN_001beb00) on this object: its +0x570 is never set here
            // (the app has no object links).
            161 if arg(0) == me => (Value::Bool(false), St::Approx, "the app has no object links: never linked"),
            // 1.222 objentry_word0 (0x170808): the first word of the object's objentry row, its id.
            222 if arg(0) == me => (Value::Int(self.fields.record.object as i32), St::Impl, ""),
            // 1.170 spawn_record_id (0x171b30): the halfword +2 of the object's group header (+0x818
            // -> +8): the spawn group's id.
            170 if arg(0) == me => (Value::Int(self.fields.group.id as i32), St::Impl, ""),
            // 1.145 obj_word_aa8 (0x1705d0) on the player: the group id of the townsperson he faces
            // for a talk [role inferred from rtn_default comparing it with 1.170]; the player side
            // of the app does not write it: no group.
            145 if arg(0) == PLAYER => (Value::Int(-1), St::Approx, "the player's +0xaa8 (the group he talks to) is not written by the app: -1, no match"),
            // 1.18 link_start on a companion object (the pets of four townspeople): the companions
            // are not spawned (1.114 answers no object), so this is not reached in practice.
            18 => (Value::None, St::Stub, "object links are not in the app"),
            // 1.172 (0x1718d0 -> FUN_001af878): the player's lock-on point is inside the object's
            // talk cone; 1.173 (0x171908 -> FUN_001af650) the same for a given point.
            172 => (Value::Bool(self.in_front(self.inner.env.player)), St::Approx, "the kind-0x14 part's point is the head bone of the last tick [inferred]; the player's lock-on point is his origin"),
            173 => match args_vec(c, 1) {
                Some(v) => (Value::Bool(self.in_front(from_game(v))), St::Approx, "the kind-0x14 part's point is the head bone of the last tick [inferred]"),
                None => (Value::Bool(false), St::Missing, "1.173 without a point"),
            },
            // 1.13 queue_motion on this object (0x1702b8 -> the motion controller's queue, which
            // `rtn_default` uses to go from the turn (129) through the idle (0) to the walk (1)).
            13 if arg(0) == me => {
                let slot = arg(1) as i32;
                self.fields.queued = usize::try_from(slot).ok().map(|s| (s, f32::from_bits(arg(2))));
                (Value::None, St::Approx, "the motion controller's queue is not ported: the queued motion starts when the playing one ends")
            }
            // 1.174 / 1.175 / 1.176 (0x171940, 0x171978, 0x1719a8 -> FUN_001aece8, FUN_001aec78,
            // FUN_001aec50): the head look-at controller at object +0x4a0 is started toward an object
            // or a point over n frames, or tweened back to zero (`look`). The townspeople's scripts use
            // 1.174 toward the player and 1.176; 1.175 (a point) is in none of them.
            174 => {
                if self.log {
                    info!("npc: {} host 1.174 {:x?}: the first argument is not the object itself ({me:#x}); the target is taken to be the player", self.inner.tag, a);
                }
                self.fields.look_cmd = Some(LookCmd::Start(f32::from_bits(arg(2))));
                (Value::None, St::Approx, "mode 2 toward the player; the joint's turn is inferred (the pose code behind FUN_001aead0 was not traced), and the enable flag (object +0x160 bit 0x4000000) is not read")
            }
            176 if arg(0) == me => {
                self.fields.look_cmd = Some(LookCmd::Release(f32::from_bits(arg(1))));
                (Value::None, St::Approx, "the weight ramps to 0 (FUN_001aec50); the joint's turn is inferred")
            }
            175 => (Value::None, St::Stub, "the look-at controller's point mode (FUN_001aec78) is not ported: the head follows the motion"),
            // 1.177 (0x1719d0 -> FUN_00183c30): set the walk path by id.
            177 => {
                let ok = self.set_path(arg(1));
                (Value::Int(ok as i32), St::Impl, "")
            }
            // 1.178 (0x171a08 -> FUN_00183e18 -> FUN_002d27c8): the direction along the path for the
            // path's own speed (record +0x24 = 20.0, DAT_003837a4), blend 1.0; zero without a path.
            178 => match self.fields.path.is_some() {
                true => {
                    let d = self.path_direction(20.0);
                    (Value::Vec([d.x, -d.y, -d.z, 1.0]), St::Approx, "FUN_002d2a98 ported for the ordinary path (+0x87 clear, no stop index +0x8c); a zero direction answers the facing")
                }
                false => (Value::Vec([0.0; 4]), St::Impl, ""),
            },
            _ => return None,
        })
    }
}

/// Argument `k` of a decoded call when it is a vector.
fn args_vec(c: &Call, k: usize) -> Option<kh2_bdx::Vec4> {
    match c {
        Call::Func { args, .. } => match args.get(k) {
            Some(Arg::Vec(v)) => Some(*v),
            _ => None,
        },
        _ => None,
    }
}

impl Host for NpcHost<'_, '_> {
    fn call(&mut self, c: &Call) -> Value {
        let answer = match c {
            // 1.114 bind_object for an entity other than the player: the companions (pets) of four
            // townspeople, which the app does not spawn.
            Call::FindObject { value, .. } if *value != kh2_ard::run::PLAYER as i32 => Some((Value::Handle(0), St::Stub, "companion objects are not spawned: no object")),
            _ => match words(c) {
                Some((5, index, a)) => self.group5(index, &a),
                Some((1, index, a)) => self.group1(index, &a, c),
                _ => None,
            },
        };
        if let Some((v, st, why)) = answer {
            let tag = self.inner.tag;
            self.trace().record(tag, c, &v, st, why);
            return v;
        }
        self.inner.call(c)
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        self.inner.read_mem(addr, out)
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        self.inner.write_mem(addr, data)
    }
}

/// `FUN_001af650`: is `point` inside the object's talk cone. The facing is +0x550 (the yaw's
/// horizontal vector). Without a type-0x14 part the direction is `point - position` flattened to the
/// ground and the test is `angle < DAT_0037b950` (15 degrees); with one it starts at the part's
/// point, is not flattened, and the test is `angle <= halfword +0x10 * DAT_0037b954` (degrees to
/// radians). The angle is `FUN_0017bf10`'s arc cosine (0 or pi outside (-1, 1)).
pub fn in_cone(pos: Vec3, yaw: f32, point: Vec3, part: Option<(Vec3, i16)>) -> bool {
    let (origin, flat) = match part {
        Some((p, _)) => (p, false),
        None => (pos, true),
    };
    let mut d = point - origin;
    if flat {
        d.y = 0.0;
    }
    if d.length() < 1e-3 {
        return true;
    }
    let d = d.normalize();
    let dot = d.x * yaw.sin() + d.z * yaw.cos();
    let angle = if dot < 1.0 && dot > -1.0 { dot.acos() } else if dot >= 1.0 { 0.0 } else { std::f32::consts::PI };
    match part {
        Some((_, deg)) => angle <= f32::from(deg) * f32::from_bits(0x3c8e_fa35),
        None => angle < f32::from_bits(0x3e86_0a91),
    }
}

/// The head look-at controller of a townsperson (docs/systems/events-movies/npc-savepoints.md section 3, "Head look-at").
///
/// The game keeps it at object +0x4a0 (0x60 bytes): +4 the mode (0 off, 1 toward a point, 2 toward an
/// object, 3 toward a direction), +0xc the joint (the halfword +6 of the object's record of type
/// 0x14), +0x10/+0x14 the weight and its per-tick step (`FUN_001abf40`, advanced by `FUN_001abf78`),
/// +0x20 the aim point, +0x30 the target object, +0x40 the last look direction, +0x50 the head's
/// yaw against the body. Hosts 1.174 / 1.176 (`FUN_001aece8`, `FUN_001aec50`) start and end it;
/// `FUN_001aea28` runs it every tick and mode 2 is `FUN_001aee90`; `FUN_001aead0` hands the aim
/// point and the eased weight to the pose code. Only objects whose model has a record of type 0x14
/// (the `*_RTN` townspeople: joint = the head bone, radii = yaw and pitch limits in degrees) turn.
pub mod look {
    use bevy::prelude::*;

    use crate::reaction_parts::Rec;

    /// The collision record type of the head (`FUN_001aee90`: `FUN_0016b558(obj, 0x14, -1)`).
    pub const TYPE_HEAD: u8 = 0x14;
    /// `DAT_0037b938`: degrees to radians for the record's two radii.
    const DEG: f32 = f32::from_bits(0x3c8e_fa35);
    /// `DAT_0037b93c`: the most the head turns toward the target in one tick (5 degrees); a smaller
    /// remaining angle is taken whole.
    pub const YAW_STEP: f32 = f32::from_bits(0x3db2_b8c2);
    /// `DAT_0037b944`: the pitch moves 3 degrees a tick, and only while it is more than 3 degrees
    /// from the target's.
    pub const PITCH_STEP: f32 = f32::from_bits(0x3d56_774f);

    /// The record's limits: `+0x10` and `+0x12` (halfwords) times `DAT_0037b938`.
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Limits {
        pub yaw: f32,
        pub pitch: f32,
    }

    impl Limits {
        pub fn of(rec: &Rec) -> Limits {
            Limits { yaw: rec.r[0] as f32 * DEG, pitch: rec.r[1] as f32 * DEG }
        }
    }

    /// One tick's inputs, in the app's space: the head part's point (`FUN_001894d0` of the record),
    /// the target's point (`FUN_0016bbe8`), the body's facing and the record's limits.
    #[derive(Clone, Copy, Debug)]
    pub struct Input {
        pub head: Vec3,
        pub target: Vec3,
        pub body_yaw: f32,
        pub limits: Limits,
    }

    /// The controller state.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Look {
        /// Mode 2 is running (+4 == 2).
        pub active: bool,
        /// +0x10: the weight, tweened between 0 and 1.
        pub weight: f32,
        /// +0x14: its change per tick.
        delta: f32,
        /// +0x50: the head's yaw against the body's facing.
        pub yaw_off: f32,
        /// The pitch of the last look direction (+0x40), up positive. Starts level: the game seeds +0x40
        /// with the target's facing vector (`FUN_0016a890`), which for an upright object is level.
        pub pitch: f32,
        /// +0x20: the aim point of the last tick.
        pub aim: Vec3,
    }

    fn wrap(a: f32) -> f32 {
        let a = (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        if a == -std::f32::consts::PI { std::f32::consts::PI } else { a }
    }

    impl Look {
        /// Host 1.174 (`FUN_001aece8`, with the record present): mode 2 toward the target, the weight
        /// from 0 to 1 over `frames` (`FUN_001abf40`: at once when `frames` is not above 0), the
        /// head's yaw against the body back to 0.
        pub fn start(&mut self, frames: f32) {
            self.active = true;
            self.yaw_off = 0.0;
            self.weight = 0.0;
            self.delta = 0.0;
            if frames <= 0.0 {
                self.weight = 1.0;
            } else {
                self.delta = 1.0 / frames;
            }
        }

        /// Host 1.176 (`FUN_001aec50`): the weight from where it is to 0 over `frames`.
        pub fn release(&mut self, frames: f32) {
            if frames <= 0.0 {
                self.weight = 0.0;
                self.delta = 0.0;
            } else {
                self.delta = -self.weight / frames;
            }
        }

        /// `FUN_001abf78` on the weight, then `FUN_001aea28`'s test: a weight of 0 ends the mode.
        fn tween(&mut self) {
            if self.delta != 0.0 {
                self.weight += self.delta;
                if self.weight <= 0.0 {
                    self.weight = 0.0;
                    self.delta = 0.0;
                } else if self.weight >= 1.0 {
                    self.weight = 1.0;
                    self.delta = 0.0;
                }
            }
            if self.weight == 0.0 {
                self.active = false;
            }
        }

        /// `FUN_001aea28` for mode 2: the tween, then `FUN_001aee90`. The head's yaw turns toward the
        /// target by at most 5 degrees a tick (a smaller angle is taken whole) and stays within the
        /// record's yaw limit of the body's facing; the pitch moves by 3 degrees a tick while it is
        /// more than 3 degrees off the target's and stays within the pitch limit of level. Returns the
        /// eased weight to apply, or `None` while the mode is off.
        pub fn tick(&mut self, i: &Input) -> Option<f32> {
            self.tween();
            if !self.active {
                return None;
            }
            let d = i.target - i.head;
            let hlen = d.x.hypot(d.z);
            // A target straight above or below has no horizontal direction to turn toward.
            if hlen > 1e-3 {
                let toward = d.x.atan2(d.z);
                let head_yaw = i.body_yaw + self.yaw_off;
                let gap = wrap(toward - head_yaw);
                let mut yaw = if gap.abs() > YAW_STEP { head_yaw + YAW_STEP.copysign(gap) } else { toward };
                let rel = wrap(yaw - i.body_yaw);
                if rel.abs() > i.limits.yaw {
                    yaw = i.body_yaw + i.limits.yaw.copysign(rel);
                }
                self.yaw_off = wrap(yaw - i.body_yaw);
            }
            let want = d.y.atan2(hlen);
            let gap = want - self.pitch;
            if gap.abs() > PITCH_STEP {
                self.pitch += PITCH_STEP.copysign(gap);
            }
            self.pitch = self.pitch.clamp(-i.limits.pitch, i.limits.pitch);
            // +0x20: the head point plus the new look direction scaled by the distance.
            let (sy, cy) = (i.body_yaw + self.yaw_off).sin_cos();
            let dir = Vec3::new(self.pitch.cos() * sy, self.pitch.sin(), self.pitch.cos() * cy);
            self.aim = i.head + dir * d.length();
            Some(self.eased())
        }

        /// `FUN_001aead0`: the weight as `(1 - cos(pi w)) / 2`.
        pub fn eased(&self) -> f32 {
            (1.0 - (std::f32::consts::PI * self.weight).cos()) * 0.5
        }

        /// The turn of the head in the character's own frame (+z forward, y up) at the eased weight
        /// [inferred: how the pose code turns the joint toward the aim point was not traced]: a yaw of
        /// `yaw_off` and a pitch of `pitch`, scaled from nothing by the weight.
        pub fn turn(&self, eased: f32) -> Quat {
            Quat::IDENTITY.slerp(Quat::from_rotation_y(self.yaw_off) * Quat::from_rotation_x(-self.pitch), eased)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const LIMITS: Limits = Limits { yaw: 50.0 * DEG, pitch: 20.0 * DEG };

        fn input(target: Vec3, body_yaw: f32) -> Input {
            Input { head: Vec3::new(0.0, 160.0, 0.0), target, body_yaw, limits: LIMITS }
        }

        /// The record's two radii (50 and 20, the townspeople's) are degrees, and the constants are
        /// the executable's: 5 degrees, 3 degrees.
        #[test]
        fn limits_and_steps_are_the_executables_degrees() {
            let rec = Rec { part: 0, id: 0, kind: TYPE_HEAD, shape: 0, joint: 0x3e, off: [7, 0, 0], lift: 0, r: [50, 20] };
            let l = Limits::of(&rec);
            assert!((l.yaw.to_degrees() - 50.0).abs() < 1e-3 && (l.pitch.to_degrees() - 20.0).abs() < 1e-3);
            assert!((YAW_STEP.to_degrees() - 5.0).abs() < 1e-4 && (PITCH_STEP.to_degrees() - 3.0).abs() < 1e-4);
        }

        /// The head turns 5 degrees a tick toward a target 40 degrees to the side, takes the last
        /// 3 degrees whole, and then holds.
        #[test]
        fn the_head_turns_five_degrees_a_tick_then_arrives() {
            let mut l = Look::default();
            l.start(0.0);
            let target = Vec3::new(40f32.to_radians().sin(), 0.0, 40f32.to_radians().cos()) * 300.0 + Vec3::Y * 160.0;
            let mut seen = Vec::new();
            for _ in 0..10 {
                l.tick(&input(target, 0.0)).unwrap();
                seen.push(l.yaw_off.to_degrees());
            }
            for (k, v) in seen.iter().take(8).enumerate() {
                assert!((v - 5.0 * (k + 1) as f32).abs() < 1e-3, "{seen:?}");
            }
            assert!((seen[8] - 40.0).abs() < 1e-3 && (seen[9] - 40.0).abs() < 1e-3, "{seen:?}");
        }

        /// A target behind the body or far to the side leaves the head at the yaw limit, either side.
        #[test]
        fn the_yaw_stops_at_the_records_limit() {
            for side in [1.0f32, -1.0] {
                let mut l = Look::default();
                l.start(0.0);
                let target = Vec3::new(side * 300.0, 160.0, -50.0);
                for _ in 0..60 {
                    l.tick(&input(target, 0.0)).unwrap();
                }
                assert!((l.yaw_off - side * LIMITS.yaw).abs() < 1e-4, "{}", l.yaw_off.to_degrees());
            }
            // The limit is against the body's facing, not the world's.
            let mut l = Look::default();
            l.start(0.0);
            for _ in 0..60 {
                l.tick(&input(Vec3::new(0.0, 160.0, 300.0), 1.0)).unwrap();
            }
            assert!((l.yaw_off + LIMITS.yaw).abs() < 1e-4, "{}", l.yaw_off.to_degrees());
        }

        /// The pitch moves 3 degrees at a time, ignores a target less than 3 degrees off level and
        /// stops at the pitch limit.
        #[test]
        fn the_pitch_steps_by_three_degrees_inside_its_limit() {
            let mut l = Look::default();
            l.start(0.0);
            let level = Vec3::new(0.0, 160.0 + 300.0 * 2f32.to_radians().tan(), 300.0);
            l.tick(&input(level, 0.0)).unwrap();
            assert_eq!(l.pitch, 0.0, "2 degrees off level is ignored");
            let up = Vec3::new(0.0, 160.0 + 300.0 * 45f32.to_radians().tan(), 300.0);
            l.tick(&input(up, 0.0)).unwrap();
            assert!((l.pitch.to_degrees() - 3.0).abs() < 1e-3);
            for _ in 0..30 {
                l.tick(&input(up, 0.0)).unwrap();
            }
            assert!((l.pitch - LIMITS.pitch).abs() < 1e-4, "{}", l.pitch.to_degrees());
            let down = Vec3::new(0.0, 160.0 - 300.0, 300.0);
            for _ in 0..60 {
                l.tick(&input(down, 0.0)).unwrap();
            }
            assert!((l.pitch + LIMITS.pitch).abs() < 1e-4);
        }

        /// The weight ramps over the host's frames, the eased weight follows `(1 - cos(pi w)) / 2`,
        /// and the mode ends when the tween back to 0 arrives (`FUN_001aea28`).
        #[test]
        fn the_weight_ramps_in_and_out_and_ends_the_mode() {
            let mut l = Look::default();
            l.start(20.0);
            let t = input(Vec3::new(0.0, 160.0, 300.0), 0.0);
            for _ in 0..10 {
                l.tick(&t).unwrap();
            }
            assert!((l.weight - 0.5).abs() < 1e-5 && (l.eased() - 0.5).abs() < 1e-5);
            for _ in 0..10 {
                l.tick(&t).unwrap();
            }
            assert!((l.weight - 1.0).abs() < 1e-5);
            l.release(20.0);
            let ticks = (1..30).find(|_| l.tick(&t).is_none()).expect("the mode ends");
            // The twentieth tick brings the weight to 0 (the twenty-first if the sum falls short).
            assert!((20..=21).contains(&ticks), "{ticks}");
            assert!(!l.active && l.weight == 0.0);
            // A turn of nothing is no turn.
            assert!(l.turn(0.0).angle_between(Quat::IDENTITY) < 1e-6);
        }

        /// Frames of 0 or less switch it on at full weight at once; a release from 0 stays at 0.
        #[test]
        fn zero_frames_act_at_once() {
            let mut l = Look::default();
            l.start(0.0);
            assert_eq!(l.weight, 1.0);
            l.release(0.0);
            assert_eq!(l.weight, 0.0);
            let mut l = Look::default();
            l.release(20.0);
            assert_eq!(l.weight, 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction_cmd::PART_FROM_RECORD;

    /// `FUN_001af650`: no part -> 15 degrees flat (strict); a part -> its own point, not flattened,
    /// at most halfword * 0.017453 (the townspeople's 50 degrees).
    #[test]
    fn cone_without_and_with_a_head_part() {
        let pos = Vec3::ZERO;
        let at = |deg: f32, r: f32, y: f32| Vec3::new(deg.to_radians().sin() * r, y, deg.to_radians().cos() * r);
        // no part: 14 degrees in, 16 out; height is ignored
        assert!(in_cone(pos, 0.0, at(14.0, 100.0, 500.0), None));
        assert!(!in_cone(pos, 0.0, at(16.0, 100.0, 0.0), None));
        // a part with 50 degrees: 40 degrees to the side is in, 55 out
        let part = Some((Vec3::new(0.0, 160.0, 0.0), 50));
        assert!(in_cone(pos, 0.0, at(40.0, 100.0, 160.0), part));
        assert!(!in_cone(pos, 0.0, at(55.0, 100.0, 160.0), part));
        // not flattened: a point straight above the head is 90 degrees from the facing
        assert!(!in_cone(pos, 0.0, Vec3::new(0.0, 460.0, 0.0), part));
        assert!(in_cone(pos, 0.0, Vec3::new(0.0, 460.0, 0.0), None));
        // the facing follows the yaw
        assert!(in_cone(pos, 1.0, at(57.0, 100.0, 160.0), part));
        // dot near +-1 clamps (FUN_0017bf10): directly behind is pi
        assert!(!in_cone(pos, 0.0, Vec3::new(0.0, 160.0, -100.0), part));
    }

    /// The disc's townspeople: the head record (type 0x14) of `N_EX510_RTN` and `N_EX520_RTN` names
    /// a bone of the model and the limits 50 and 20 degrees; `N_EX500_RTN` has none, so host 1.174
    /// does nothing for it (`FUN_001aece8` looks the record up first). The head bone stands above
    /// the root in the rest pose.
    #[test]
    fn the_head_record_names_the_head_bone_and_the_limits() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let d = Disc::open(&iso);
        for (model, joint) in [("N_EX510_RTN", 128u16), ("N_EX520_RTN", 68)] {
            let file = d.read(&format!("obj/{model}.mdlx")).expect(model);
            let t = parts::Table::from_mdlx(&file).expect("parts");
            let rec = t.first(look::TYPE_HEAD).unwrap_or_else(|| panic!("{model}: no head record"));
            assert_eq!((rec.joint, rec.r), (joint, [50, 20]), "{model}");
            let l = look::Limits::of(rec);
            assert!((l.yaw.to_degrees() - 50.0).abs() < 1e-3 && (l.pitch.to_degrees() - 20.0).abs() < 1e-3);
            let bones = kh2_model::mdlx(&file).expect("model").bones;
            let rest = kh2_model::rest_pose(&bones);
            let head = rest[joint as usize].w_axis.y;
            assert!((100.0..220.0).contains(&head), "{model}: head bone at height {head}");
        }
        let none = d.read("obj/N_EX500_RTN.mdlx").and_then(|f| parts::Table::from_mdlx(&f)).expect("N_EX500_RTN");
        assert!(none.first(look::TYPE_HEAD).is_none());
        // The player's point (FUN_0016bbe8) is his type-0x11 record, on a bone.
        let sora = d.read("obj/P_EX100.mdlx").and_then(|f| parts::Table::from_mdlx(&f)).expect("P_EX100");
        let p = sora.first(TYPE_POINT).expect("type 0x11");
        assert_eq!((p.joint, p.off), (0x8c, [3, 0, 0]));
    }

    /// FIX-R1 5: an area load that drops a running talk with the camera started asks the rig to end it;
    /// a run without the camera, or no run, asks for nothing.
    #[test]
    fn dropping_a_talk_with_the_camera_ends_the_camera() {
        let run = |camera| TalkRun { npc: Entity::PLACEHOLDER, tag: String::new(), ops: Vec::new(), at: 0, window: false, yaw: 0.0, camera, examine: false };
        let mut t = Talk { run: Some(run(true)), ..Default::default() };
        assert_eq!(t.drop_run(), Some(crate::camera::TalkCam::End));
        assert!(t.run.is_none());
        let mut t = Talk { run: Some(run(false)), ..Default::default() };
        assert_eq!(t.drop_run(), None);
        assert!(t.run.is_none());
        assert_eq!(Talk::default().drop_run(), None);
    }

    fn path(points: &[[f32; 3]], loops: bool) -> kh2_ard::WalkPath {
        let mut header = [0u8; 8];
        header[0] = 7;
        header[2] = points.len() as u8;
        header[4] = loops as u8;
        kh2_ard::WalkPath { header, points: points.to_vec() }
    }

    /// FUN_002d32f8 / FUN_002d4298: the curve passes through the points, an inner point's tangent
    /// is half the difference of its neighbours, the ends of an open path take the quadratic's
    /// direction, and the corner is rounded off the chords; FUN_002d4dc8 finds a place on the curve.
    #[test]
    fn the_walk_path_is_the_games_hermite_curve() {
        let p = path(&[[0.0, 0.0, 0.0], [400.0, 0.0, 0.0], [400.0, 0.0, -400.0]], false);
        let r = PathRun::new(7, &p, false);
        let (a, b, c) = (Vec3::ZERO, Vec3::new(400.0, 0.0, 0.0), Vec3::new(400.0, 0.0, 400.0));
        assert!((r.eval(0, 0.0) - a).length() < 1e-3 && (r.eval(0, 1.0) - b).length() < 1e-3 && (r.eval(1, 1.0) - c).length() < 1e-3);
        assert!((r.tangents[1] - (c - a) * 0.5).length() < 1e-3, "{:?}", r.tangents[1]);
        // The start's tangent comes from the quadratic through the three points: along the first
        // chord, bent away from the third point.
        assert!(r.tangents[0].x > 0.0 && r.tangents[0].z < 0.0, "{:?}", r.tangents[0]);
        // The curve leaves the chords near the corner.
        let mid = r.eval(0, 0.75);
        assert!(mid.z.abs() > 1.0, "{mid:?}");
        // A place on the curve is found again.
        let q = r.eval(1, 0.3);
        let (i, f, _) = r.nearest(q);
        assert!((r.eval(i, f) - q).length() < 1.0, "{i} {f}");
        // Header bit 1 means no tension (tangents of zero), bit 0 a loop.
        let mut h = path(&[[0.0, 0.0, 0.0], [400.0, 0.0, 0.0], [400.0, 0.0, -400.0]], true);
        h.header[4] |= 2;
        let l = PathRun::new(7, &h, false);
        assert!(l.loops && l.tension == 0.0 && l.tangents.iter().all(|t| t.length() < 1e-6));
        // Two points: the chord is the tangent and the path cannot loop.
        let two = PathRun::new(7, &path(&[[0.0, 0.0, 0.0], [400.0, 0.0, 0.0]], true), false);
        assert!(!two.loops && (two.tangents[0] - b).length() < 1e-3);
    }

    /// Walking the way FUN_002d2a98 points (a step of 8 cm toward each answer): an open path is
    /// followed to its end and turned around (FUN_002d2328), a locked one stops there, a loop wraps.
    #[test]
    fn a_walk_path_is_followed_and_turns_around_at_its_ends() {
        let walk = |r: &mut PathRun, mut pos: Vec3, steps: usize| -> (Vec3, bool, bool) {
            let (mut flipped, mut stopped) = (false, false);
            for _ in 0..steps {
                match r.advance(40.0, pos) {
                    Some((a, _)) => pos += (a - pos).with_y(0.0).normalize_or_zero() * 8.0,
                    None => {
                        stopped = true;
                        break;
                    }
                }
                if !r.forward {
                    flipped = true;
                    break;
                }
            }
            (pos, flipped, stopped)
        };
        let p = path(&[[0.0, 0.0, 0.0], [400.0, 0.0, 0.0], [400.0, 0.0, -400.0]], false);
        let mut r = PathRun::new(7, &p, false);
        r.place_nearest(Vec3::new(40.0, 0.0, 0.0));
        assert!((r.at(r.t) - Vec3::new(40.0, 0.0, 0.0)).length() < 15.0, "{}", r.t);
        let (end, flipped, _) = walk(&mut r, Vec3::new(40.0, 0.0, 0.0), 400);
        assert!(flipped && end.z > 330.0 && end.x > 380.0, "{end:?}");
        let mut locked = PathRun::new(7, &p, false);
        locked.bounce = false;
        locked.place_nearest(Vec3::ZERO);
        let (end, _, stopped) = walk(&mut locked, Vec3::ZERO, 400);
        assert!(stopped && end.z > 330.0, "{end:?}");
        let mut l = PathRun::new(7, &path(&[[0.0, 0.0, 0.0], [400.0, 0.0, 0.0], [0.0, 0.0, -400.0]], true), false);
        l.place_nearest(Vec3::ZERO);
        let (mut pos, mut wrapped, mut last_t) = (Vec3::ZERO, false, 0.0);
        for _ in 0..400 {
            if let Some((a, _)) = l.advance(40.0, pos) {
                pos += (a - pos).with_y(0.0).normalize_or_zero() * 8.0;
            }
            wrapped |= l.t < last_t - 1.0;
            last_t = l.t;
        }
        assert!(wrapped && l.forward, "t {}", l.t);
    }

    fn entity(object: u32, serial: u16) -> kh2_ard::Entity {
        kh2_ard::Entity { object, position: [0.0; 3], rotation: [0.0; 3], spawn_kind: 0, spawn_arg: 0, serial, raw: [0; 0x40] }
    }

    fn group(returns: Vec<[u8; 0x10]>, activators: Vec<kh2_ard::Activator>) -> Arc<NpcGroup> {
        Arc::new(NpcGroup { tag: "t".into(), kind: 6, id: 1, paths: Vec::new(), returns, activators })
    }

    /// The fields of a walker (bit 0 clear) with the nine floats `rtn_default` hands to host 5.33.
    fn walker(object: u32, serial: u16, g: &Arc<NpcGroup>) -> NpcFields {
        let mut f = NpcFields::new(&entity(object, serial), g, serial as u32);
        f.params = [360.0f32, 720.0, 180.0, 360.0, 720.0, 180.0, 360.0, 720.0, 180.0].map(f32::to_bits);
        f
    }

    /// FUN_0018a130: two townspeople whose kind-0 parts meet and whose return entries name each
    /// other (byte 0 = 0, the objentry id at +4) start a kind-5 pair together; the player's body
    /// meeting a talk part (type 5) then starts a kind-4 follower toward him.
    #[test]
    fn the_class_pass_pairs_townspeople_and_turns_them_to_the_player() {
        let entry = |id: u32| {
            let mut e = [0u8; 0x10];
            (e[1], e[3]) = (2, 1);
            e[4..8].copy_from_slice(&id.to_le_bytes());
            e
        };
        let (ga, gb) = (group(vec![entry(0x51f)], vec![]), group(vec![entry(0x51d)], vec![]));
        let (mut fa, mut fb) = (walker(0x51d, 26, &ga), walker(0x51f, 30, &gb));
        // The tt townspeople's parts: type 0 a sphere of 90 lifted 115, type 5 a sphere of 200 at 200 up.
        let body = |p: Vec3| Some(parts::Shape { kind: 3, center: p + Vec3::Y * 115.0, r: [90.0, 90.0] });
        let talk = |p: Vec3| Some(parts::Shape { kind: 3, center: p + Vec3::Y * 200.0, r: [200.0, 200.0] });
        let (pa, pb) = (Vec3::ZERO, Vec3::new(150.0, 0.0, 0.0));
        let mut minds = vec![
            Mind { handle: 0x100, object: 0x51d, serial: 26, pos: pa, yaw: 0.0, body: body(pa), talk: talk(pa), fields: &mut fa },
            Mind { handle: 0x200, object: 0x51f, serial: 30, pos: pb, yaw: std::f32::consts::PI, body: body(pb), talk: talk(pb), fields: &mut fb },
        ];
        let far = parts::Shape { kind: 0, center: Vec3::new(5000.0, 85.0, 0.0), r: [35.0, 55.0] };
        assert_eq!(class_tick(&mut minds, 0, Some(&far)), Attention::Pair(1));
        let (a, b) = (minds[0].fields.follower.clone(), minds[1].fields.follower.clone());
        assert_eq!((a.kind, a.mode, a.target, a.forced, a.flag, a.motion), (5, 1, 0x200, 1, 1, 0x11f));
        // The partner faces the other way (dot of the facings < 0): its flag is set too.
        assert_eq!((b.kind, b.mode, b.target, b.forced, b.flag), (5, 1, 0x100, 1, 1));
        assert!(a.timer == b.timer && (360.0..=720.0).contains(&a.timer), "{}", a.timer);
        // The partner's own pass does not pair again (its follower is of kind 5).
        assert_eq!(class_tick(&mut minds, 1, Some(&far)), Attention::None);
        assert_eq!(minds[1].fields.follower.target, 0x100);
        // Sora walks into the first one's talk part: a forced kind-4 follower toward the player;
        // the pair is kept as the previous follower.
        let near = parts::Shape { kind: 0, center: Vec3::new(0.0, 85.0, 150.0), r: [35.0, 55.0] };
        assert_eq!(class_tick(&mut minds, 0, Some(&near)), Attention::Player);
        let f = &minds[0].fields;
        assert_eq!((f.follower.kind, f.follower.mode, f.follower.forced, f.prev.kind), (4, 2, 1, 5));
        assert!(f.player_in_part);
        // While it lasts nothing else starts.
        assert_eq!(class_tick(&mut minds, 0, Some(&near)), Attention::None);
        assert_eq!(minds[0].fields.follower.kind, 4);
    }

    /// FUN_0018a4b0 / FUN_0018b308 / FUN_0018b148: walking into a kind-6 group's volume draws one of
    /// the return entries keyed to the volume's +2 by weight; the follower ends when the object
    /// crosses the volume past the random threshold; leaving the volume first resets it.
    #[test]
    fn a_region_draws_a_weighted_entry_that_ends_past_a_threshold() {
        let act = kh2_ard::Activator { shape: 0, option: 2, position: [0.0; 3], scale: [50.0, 100.0, 20.0], rotation: [0.0; 3], raw: [0; 0x40] };
        let mut look = [0u8; 0x10];
        (look[0], look[2], look[3]) = (2, 100, 2);
        let mut flag = [0u8; 0x10];
        (flag[0], flag[1], flag[2]) = (2, 3, 100);
        let mut other = [0u8; 0x10];
        (other[0], other[2]) = (1, 100);
        let g = group(vec![other, look, flag], vec![act.clone()]);
        let mut f = walker(0x51d, 1, &g);
        let place = |x: f32| Vec3::new(x, 0.0, 0.0);
        let mut minds = vec![Mind { handle: 0x100, object: 0x51d, serial: 1, pos: place(-40.0), yaw: 0.0, body: None, talk: None, fields: &mut f }];
        assert_eq!(class_tick(&mut minds, 0, None), Attention::None);
        let fl = minds[0].fields.follower.clone();
        assert_eq!((fl.kind, fl.mode, fl.region, fl.flag, fl.motion), (2, 2, Some(0), 1, 0));
        assert!((fl.start - 0.1).abs() < 1e-4, "{}", fl.start);
        assert!((360.0..=720.0).contains(&fl.timer));
        assert_eq!(minds[0].fields.region, Some(0));
        // The same volume draws nothing new.
        minds[0].fields.follower.motion = 77;
        class_tick(&mut minds, 0, None);
        assert_eq!(minds[0].fields.follower.motion, 77);
        // Started in the lower half: the test passes at or above the threshold.
        let mut t = fl.clone();
        t.threshold = 0.5;
        assert!(!follower_test(&t, &g, [-40.0, 0.0, 0.0]));
        assert!(follower_test(&t, &g, [10.0, 0.0, 0.0]));
        t.start = 0.9;
        assert!(follower_test(&t, &g, [-40.0, 0.0, 0.0]) && !follower_test(&t, &g, [10.0, 0.0, 0.0]));
        // Leaving the volume before the follower is done resets it.
        minds[0].pos = place(500.0);
        class_tick(&mut minds, 0, None);
        assert_eq!((minds[0].fields.follower.kind, minds[0].fields.follower.region, minds[0].fields.region), (0, None, None));
        // Host 5.4 always reloads +0xb00 from +0xb30 (180 here).
        minds[0].fields.end_follower();
        assert_eq!(minds[0].fields.timer_b00, 180.0);
    }

    /// The townspeople of tt02's kind-6 groups name each other as partners, and their models carry
    /// the parts the class tests (type 0 and type 5 on the object's own frame), as does Sora's body.
    #[test]
    fn tt02_townspeople_name_each_other_and_carry_their_parts() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let disc = Disc::open(&iso);
        let (area, _) = kh2_ard::Area::parse(&disc.read("ard/tt02.ard").expect("tt02.ard")).expect("area");
        let objs = disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default();
        let mut people = Vec::new();
        for g in ["a72_", "b72_", "c72_"].iter().filter_map(|t| area.spawns.get(*t)).flatten() {
            assert_eq!(g.kind & 0xff, 6);
            let e = &g.entities[0];
            let named: Vec<u32> = g.returns.iter().filter(|r| r[0] == 0 && r[3] == 1).map(|r| u32::from_le_bytes([r[4], r[5], r[6], r[7]])).collect();
            // Walking each path forward with steps of 5 cm toward the answers of the advance (speed
            // 5, rtn_default's) goes round the whole loop, the place growing segment by segment.
            for p in &g.paths {
                let mut r = PathRun::new(0, p, false);
                r.place_nearest(r.points[0]);
                let mut pos = r.at(r.t);
                let (mut back, mut laps, mut last) = (0, 0, r.t);
                for _ in 0..20000 {
                    let Some((a, _)) = r.advance(5.0, pos) else { break };
                    pos += (a - pos).with_y(0.0).normalize_or_zero() * 5.0;
                    let d = r.t - last;
                    if d < -(r.points.len() as f32) / 2.0 {
                        laps += 1;
                    } else if d < -0.01 {
                        back += 1;
                        eprintln!("{:#x}: back from {last:.3} to {:.3} at {pos:?}", e.object, r.t);
                    }
                    last = r.t;
                }
                assert!(laps >= 1 && back == 0, "{:#x}: laps {laps}, steps back {back}", e.object);
            }
            people.push((e.object, named, objs.iter().find(|r| r.object_id == e.object).map(|r| r.model_name.clone()).unwrap_or_default()));
        }
        assert_eq!(people.len(), 3);
        for (me, named, model) in &people {
            for (other, theirs, _) in people.iter().filter(|p| p.0 != *me) {
                assert!(named.contains(other) && theirs.contains(me), "{me:#x} and {other:#x} name each other");
            }
            let t = disc.read(&format!("obj/{model}.mdlx")).and_then(|f| parts::Table::from_mdlx(&f)).expect("parts");
            for kind in [0, parts::TYPE_REACTION] {
                let r = t.first(kind).expect("part");
                assert!(parts::shape_of(r, Vec3::ZERO, 0.0).is_some(), "{model} part {kind} on the object's frame");
            }
        }
        let p = disc.read("obj/P_EX100.mdlx").and_then(|f| parts::Table::from_mdlx(&f)).expect("P_EX100");
        assert!(parts::shape_of(p.first(parts::TYPE_BODY).expect("body"), Vec3::ZERO, 0.0).is_some());
    }

    /// The talk camera's input on the disc: each of tt02's four townspeople carries a collision
    /// record of type 0x13 with its talk part's index (the 0x83 part), on the object's own frame,
    /// whose radii are the camera's distance and height (FUN_0018f108 -> FUN_001894d0).
    #[test]
    fn tt02_townspeople_carry_their_talk_camera_records() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let disc = Disc::open(&iso);
        let (area, _) = kh2_ard::Area::parse(&disc.read("ard/tt02.ard").expect("tt02.ard")).expect("area");
        let objs = disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default();
        let mut seen = 0;
        for g in ["a72_", "b72_", "c72_", "b50_"].iter().filter_map(|t| area.spawns.get(*t)).flatten() {
            let e = &g.entities[0];
            let model = objs.iter().find(|r| r.object_id == e.object).map(|r| r.model_name.clone()).expect("objentry row");
            let file = disc.read(&format!("obj/{model}.mdlx")).expect("mdlx");
            let t = parts::Table::from_mdlx(&file).expect("parts");
            let rest = kh2_model::rest_pose(&kh2_model::mdlx(&file).expect("model").bones);
            let talk = t.reaction_parts().find(|r| r.id == PART_FROM_RECORD).expect("talk part");
            let cams: Vec<_> = t.recs.iter().filter(|r| r.kind == TYPE_TALK_CAMERA).collect();
            let bone = |j: usize, off: Vec3| rest.get(j).map(|m| m.transform_point3(off));
            let (shot, kind) = talk_shot(&t, talk.part, (Vec3::ZERO, 0.0), crate::camera::rules::TalkFrame::AroundPlayer, &bone).expect("talk shot");
            eprintln!("{model}: talk part {}; type 0x13 records {cams:?}; rest-pose shot {shot:?}", talk.part);
            assert_eq!(kind, TYPE_TALK_CAMERA, "{model}");
            assert!(cams.iter().all(|r| r.joint & parts::JOINT_OBJECT == 0), "{model}: on a bone");
            // The look-at point is high on the body (a head bone) and the distance is set.
            assert!(shot.dist > 0.0 && shot.look.y > 100.0, "{model}: {shot:?}");
            seen += 1;
        }
        assert_eq!(seen, 4);
    }

    /// The spawn record's word +0x24 fills bits 0..6 of the object's bit array (FUN_001899d0) and
    /// bit 0 puts the follower on the walk path (kind 2).
    #[test]
    fn spawn_flags_fill_the_bit_array() {
        let mut raw = [0u8; 0x40];
        raw[0x24] = 0x85;
        let e = kh2_ard::Entity { object: 0x4d8, position: [0.0; 3], rotation: [0.0; 3], spawn_kind: 0, spawn_arg: 0, serial: 0, raw };
        // A kind-6 group with one behaviour entry (byte 0 = 0, byte 3 = 4) naming motion index 5.
        let mut entry = [0u8; 0x10];
        entry[1] = 5;
        entry[3] = 4;
        let g = Arc::new(NpcGroup { tag: "a72_".into(), kind: 6, id: 0x191, paths: Vec::new(), returns: vec![[0; 0x10], entry], activators: Vec::new() });
        let f = NpcFields::new(&e, &g, 1);
        assert!(f.test_bit(0) && f.test_bit(2) && !f.test_bit(1));
        assert!(!f.test_bit(7), "only bits 0..6 are copied");
        assert_eq!(f.follower.kind, 2);
        assert_eq!(f.follower.motion, 5 + 0x117, "FUN_0018b1e0 of the entry's byte 1");
        assert_eq!((entry_motion(0), entry_motion(2), entry_motion(3), entry_motion(13), entry_motion(14)), (0, 0x11f, 0, 13 + 0x117, 0));
        let mut f = f;
        f.params[8] = 90f32.to_bits();
        f.end_follower();
        assert_eq!((f.prev.kind, f.follower.kind), (2, 0));
        assert_eq!(f.timer_b00, 90.0);
    }

    fn cmd_row(id: u16, priority: u8) -> crate::reaction_cmd::CmdRow {
        crate::reaction_cmd::CmdRow {
            id,
            execute: id,
            argument: 0,
            flags: 0,
            disable_form: 0,
            dist_range: 0,
            range: -1.0,
            priority,
            message_id: 1000 + id as i32,
            cmd_action: 0,
            reaction_count: 0,
            require: 0,
            cost: 0,
        }
    }

    /// FUN_0019cb70 for a townsperson: its type-5 part with id 0x83 offers the spawn record's +0x2c
    /// while the player's body touches the part; a part with its own id offers that id and the higher
    /// priority wins (FUN_0019c520); a disabled part offers nothing.
    #[test]
    fn the_0x83_part_offers_the_record_command() {
        use crate::reaction_cmd::{Ctx, ReactionRules, Status};
        let rules = ReactionRules { cmds: vec![cmd_row(31, 100), cmd_row(30, 100), cmd_row(40, 120)] };
        let sphere = |part: u8, id: u16| parts::Rec { part, id, kind: parts::TYPE_REACTION, shape: 3, joint: parts::JOINT_OBJECT, off: [0, 0, 0], lift: 200, r: [200, 200] };
        let table = parts::Table { default_mask: 1, recs: vec![sphere(0, PART_FROM_RECORD)] };
        let body = |at: Vec3| parts::Shape { kind: 3, center: at + Vec3::Y * 85.0, r: [35.0, 55.0] };
        let ctx = Ctx::default();
        let near = Vec3::new(0.0, 0.0, 150.0);
        let o = part_offer(&rules, &table, 31, (&[], &[]), (Vec3::ZERO, 0.0), &body(near), (near, std::f32::consts::PI), (&ctx, None)).expect("offer");
        assert_eq!((o.cmd, o.part, o.status), (31, 0, Status::Available));
        assert_eq!(part_offer(&rules, &table, 30, (&[], &[]), (Vec3::ZERO, 0.0), &body(near), (near, 0.0), (&ctx, None)).map(|o| o.cmd), Some(30), "the record names the command");
        let far = Vec3::new(0.0, 0.0, 400.0);
        assert_eq!(part_offer(&rules, &table, 31, (&[], &[]), (Vec3::ZERO, 0.0), &body(far), (far, 0.0), (&ctx, None)), None, "outside the part");
        assert_eq!(part_offer(&rules, &table, 31, (&[], &[0]), (Vec3::ZERO, 0.0), &body(near), (near, 0.0), (&ctx, None)), None, "a trigger 5 turns the part off");
        let two = parts::Table { default_mask: 3, recs: vec![sphere(0, PART_FROM_RECORD), sphere(1, 40)] };
        assert_eq!(part_offer(&rules, &two, 31, (&[], &[]), (Vec3::ZERO, 0.0), &body(near), (near, 0.0), (&ctx, None)).map(|o| (o.cmd, o.part)), Some((40, 1)));
    }

    /// The pages of a message: a page ends at text command 0x10 where the command walk reaches it,
    /// not at a 0x10 inside a command's arguments (0x190fa8).
    #[test]
    fn a_message_splits_into_pages_at_command_0x10() {
        use crate::hud::hud_msgwin::pages;
        // 'A' page end 'B' end.
        let p = pages(&[0x21, 0x10, 0x22, 0x00]);
        assert_eq!(p.len(), 2);
        // A wait command (0x14) whose argument is 0x10: one page.
        let one = pages(&[0x21, 0x14, 0x10, 0x00, 0x22, 0x00]);
        assert_eq!(one.len(), 1, "{one:?}");
        assert_eq!(one[0].len(), 4, "glyph, wait, glyph, end: {one:?}");
    }

    /// The tt02 townspeople's talk ids have programs in `libretto-tt.bar`, each a single message of
    /// `msg/us/tt.bar`, and their motion sets have the talk motion 0x32.
    #[test]
    fn tt02_townspeople_have_talk_programs_messages_and_the_talk_motion() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let disc = Disc::open(&iso);
        let lib = disc.read("libretto-tt.bar").and_then(|f| kh2_ard::libretto::Libretto::from_bar(&f)).expect("libretto-tt.bar");
        let msgs = disc.read("msg/us/tt.bar").and_then(|b| kh2_image::text::Messages::from_bar(&b)).expect("tt.bar");
        let (area, _) = kh2_ard::Area::parse(&disc.read("ard/tt02.ard").expect("tt02.ard")).expect("area");
        let objs = disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default();
        let mut seen = 0;
        for tag in ["a72_", "b72_", "c72_", "b50_"] {
            for g in area.spawns.get(tag).expect("group") {
                for e in &g.entities {
                    let Some(o) = objs.iter().find(|o| o.object_id == e.object && o.object_type == OBJECT_TOWNSPERSON) else { continue };
                    assert_eq!(e.part_command(), 31, "{tag} {}", o.model_name);
                    let prog = lib.program(e.talk_id()).expect("program");
                    let ops: Vec<_> = prog.iter().map(|c| c.kind()).collect();
                    let [kh2_ard::libretto::Op::Message(id)] = ops[..] else { panic!("{tag}: {ops:?}") };
                    let b = msgs.bytes(id).expect("message");
                    let pages = crate::hud::hud_msgwin::pages(b);
                    eprintln!("{tag} {} talk {:#x}: message {id:#x}, {} page(s): {:?}", o.model_name, e.talk_id(), pages.len(), pages.iter().map(|p| kh2_image::text::to_text(p).len()).collect::<Vec<_>>());
                    assert!(!pages.is_empty());
                    let mset = disc.read(&format!("obj/{}.mset", o.model_name)).and_then(|d| kh2_anim::Mset::parse(&d)).expect("mset");
                    assert!(mset.has(MOTION_TALK), "{} has motion 0x32", o.model_name);
                    seen += 1;
                }
            }
        }
        assert_eq!(seen, 4);
    }
}
