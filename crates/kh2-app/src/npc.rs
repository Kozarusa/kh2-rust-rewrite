//! Townspeople: the class-5 objects of an area's `map` program groups (WP-E2, docs/events.md
//! section 3 and the E2 stage).
//!
//! The game makes an object of `00objentry` type 15 (`N_EX500_RTN`, `N_EX510_RTN`, ...) through
//! `FUN_00189858` (object class 5, vtable 0x35ced8, 0xb40 bytes): it carries two "attention"
//! followers at +0xa40 and +0xaa0 (`FUN_0018b0d8`), a bit array at +0xb04 filled from the spawn
//! record's word +0x24 (`FUN_001899d0`, bits 0..6), a parameter block at +0xb10 (host 5.33) and
//! the walk path at +0x508 (`FUN_00183c30`: a `WalkPath` of the object's own spawn group found by
//! its id, `FUN_00199cc0`). The model's own BDX (`rtn_default`) runs on it like an enemy script:
//! `NpcHost` answers the group 5 library (the NPC routine library of docs/bdx.md) and the path
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

/// `00objentry` ObjectType of the townspeople (FUN_0016e8c0 case 0xf -> FUN_00189858).
pub const OBJECT_TOWNSPERSON: u8 = 15;

/// Script handles of the NPCs: past the enemies' (below 0x1000) and the friends' (0x1000..).
const HANDLE_BASE: u32 = 0x2000;

pub struct NpcPlugin;

impl Plugin for NpcPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NpcAssets>()
            .init_resource::<NpcSpawned>()
            .add_systems(FixedUpdate, (npc_spawn, npc_update).chain().in_set(Sim::Enemy).after(crate::enemy::spawn_waves));
    }
}

fn npclog() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_NPCLOG").is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// The townspeople's models and scripts by model name (loaded once each), the `objentry` rows and
/// the executable's host table.
#[derive(Resource, Default)]
struct NpcAssets {
    rigs: HashMap<String, Option<Arc<Rig>>>,
    scripts: HashMap<String, Option<Arc<Program>>>,
    objs: Vec<t::Objentry>,
    table: Option<kh2_bdx::HostTable>,
    /// The collision records of each model (BAR entry 23, the game's object +0x8d8).
    parts: HashMap<String, Option<Arc<parts::Table>>>,
    /// The player's own body volume (record type 1 of `P_EX100`), which `FUN_0018aa08` tests
    /// against a townsperson's type-5 part.
    player_body: Option<parts::Rec>,
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
        self.player_body = disc.read("obj/P_EX100.mdlx").and_then(|f| parts::Table::from_mdlx(&f)).and_then(|t| t.first(parts::TYPE_BODY).copied());
    }

    fn parts(&mut self, disc: &Disc, model: &str) -> Option<Arc<parts::Table>> {
        self.parts.entry(model.to_string()).or_insert_with(|| disc.read(&format!("obj/{model}.mdlx")).and_then(|f| parts::Table::from_mdlx(&f)).map(Arc::new)).clone()
    }

    fn rig(&mut self, disc: &Disc, model: &str) -> Option<Arc<Rig>> {
        self.rigs
            .entry(model.to_string())
            .or_insert_with(|| {
                let m = disc.read(&format!("obj/{model}.mdlx")).and_then(|d| kh2_model::mdlx(&d))?;
                let s = disc.read(&format!("obj/{model}.mset")).and_then(|d| kh2_anim::Mset::parse(&d))?;
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
) {
    let Some(run) = rec.run.as_ref() else { return };
    if spawned.area.as_deref() == Some(run.name.as_str()) {
        return;
    }
    // The collision world is inserted by `load_area` (deferred: present from the frame it loads).
    let Some(world) = world else { return };
    spawned.area = Some(run.name.clone());
    if std::env::var("KH2_NO_NPC").is_ok_and(|v| v != "0") {
        return;
    }
    assets.load_once(&disc);
    // Harness: `KH2_NPC_WATCH=<n>` puts Sora three metres in front of the n-th townsperson spawned
    // (1-based), facing it (a test aid for screenshots, not the game's rule).
    let watch: Option<usize> = std::env::var("KH2_NPC_WATCH").ok().and_then(|v| v.parse().ok());
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
                let Some(rig) = assets.rig(&disc, &model) else {
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
                ent.insert(Npc { body, yaw, grounded: true, record: e.clone(), parts: model_parts });
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
                    });
                }
                count += 1;
                if watch == Some(count) && let Ok((mut tf, mut s)) = sora.single_mut() {
                    let a = yaw;
                    let p = at + Vec3::new(a.sin(), 0.0, a.cos()) * 300.0;
                    let p = world.ground(p, p.y + 100.0, 400.0).map_or(p, |g| p.with_y(g.y));
                    crate::sora::respawn(&mut s, p);
                    s.motor.yaw = a + std::f32::consts::PI;
                    tf.translation = p * SCALE;
                    tf.rotation = Quat::from_rotation_y(s.motor.yaw);
                    info!("npc: KH2_NPC_WATCH: Sora put in front of {tag} at ({:.0}, {:.0}, {:.0})", p.x, p.y, p.z);
                }
                info!("npc: {} group {} (kind {}, id {}) spawned {tag} (object {:#x})", run.name, g.tag, grp.kind, grp.id, e.object);
                info!(
                    "npc: {tag} at ({:.0}, {:.0}, {:.0}) yaw {yaw:.3}, spawn flags {:#x}, {} paths{}",
                    at.x,
                    at.y,
                    at.z,
                    NpcFields::spawn_flags(e),
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
fn npc_update(
    mut commands: Commands,
    world: Option<Res<World>>,
    trace: Option<ResMut<AiTrace>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
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
    {
        let mut rows: Vec<_> = q.iter_mut().collect();
        let tags: Vec<String> = rows.iter().map(|r| r.3.tag.clone()).collect();
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
            if minds[i].fields.test_bit(0) {
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
        let NpcBrain { rt, obj, tag, flags124, fields, started, errors_seen, mode, last_frame, visible } = &mut *brain;
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
        let mut host = NpcHost { inner: BevyHost { obj, env: &env, trace: &mut trace, tag: tag.as_str() }, fields, peers: &peers, log };
        if !*started {
            *started = true;
            rt.spawn(&mut host, handle);
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
    /// +0xa30: the region the object stood in when `FUN_0018a4b0` last looked.
    pub region: Option<usize>,
    /// This frame's `FUN_0018aa08(obj, 5)`: the object's first type-5 part meets the player's
    /// type-1 body volume (computed by the class pass, read by host 5.11).
    pub player_in_part: bool,
    /// The object's random numbers (the game's global `rand()`).
    pub rng: Rng,
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
/// at object +0x508): the points in order, whether it loops (header byte +4 bit 0) and the
/// follower's place on it. The game runs a cubic spline through the points (`FUN_002d32f8`
/// tangents, `FUN_002d4298` evaluation) and advances a parameter by the asked speed
/// (`FUN_002d2a98`); this port follows the straight segments between the points [approx: the
/// spline is not ported, a townsperson cuts the corners the game rounds].
#[derive(Clone, Debug, PartialEq)]
pub struct PathRun {
    pub id: u16,
    /// The points in the app's space (centimetres).
    pub points: Vec<Vec3>,
    /// Header byte +4 bit 0 (`FUN_002d3b40` -> record +0x10): the path closes on itself.
    pub loops: bool,
    /// Record +0x18: the place on the path, segment index plus fraction.
    pub t: f32,
    /// Record +0x84: walking from lower to higher indices.
    pub forward: bool,
    /// Record +0x85: at an end the direction turns around (`FUN_002d2328` is called by the
    /// advance); clear for objects whose flag 0x588 bit 3 and bit 2 of the bit array are set.
    pub bounce: bool,
}

impl PathRun {
    pub fn new(id: u16, p: &kh2_ard::WalkPath, start: Vec3) -> PathRun {
        let points: Vec<Vec3> = p.points.iter().map(|q| game_to_app(*q)).collect();
        let mut r = PathRun { id, points, loops: p.header[4] & 1 != 0, t: 0.0, forward: true, bounce: true };
        r.t = r.nearest(start);
        r
    }

    fn seg_count(&self) -> usize {
        match self.points.len() {
            0 | 1 => 0,
            n if self.loops => n,
            n => n - 1,
        }
    }

    /// The point at parameter `t` (segment index plus fraction).
    pub fn at(&self, t: f32) -> Vec3 {
        let n = self.points.len();
        if n == 0 {
            return Vec3::ZERO;
        }
        if n == 1 {
            return self.points[0];
        }
        let segs = self.seg_count() as f32;
        let t = if self.loops { t.rem_euclid(segs) } else { t.clamp(0.0, segs) };
        let i = (t.floor() as usize).min(self.seg_count().saturating_sub(1));
        let f = t - i as f32;
        let a = self.points[i];
        let b = self.points[(i + 1) % n];
        a.lerp(b, f)
    }

    /// The parameter of the point nearest to `p` (`FUN_002d50c0` -> `FUN_002d4dc8` at the path's
    /// start: the closest segment and the fraction along it; the vertical difference counts too).
    pub fn nearest(&self, p: Vec3) -> f32 {
        let mut best = (f32::INFINITY, 0.0);
        for i in 0..self.seg_count() {
            let a = self.points[i];
            let b = self.points[(i + 1) % self.points.len()];
            let ab = b - a;
            let f = if ab.length_squared() > 0.0 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
            let d = (a + ab * f - p).length_squared();
            if d < best.0 {
                best = (d, i as f32 + f);
            }
        }
        best.1
    }

    /// `FUN_002d2328`: at an end the direction flips unless the path is locked; returns the new
    /// direction flag.
    pub fn toggle(&mut self) -> bool {
        if self.bounce {
            self.forward = !self.forward;
        }
        self.forward
    }

    /// The look-ahead of `FUN_002d2a98(speed, path, out, position)`: up to twenty times the point
    /// at the place is taken; when it lies more than `speed` from the object (horizontally,
    /// `DAT_003837dc` halves the step) it is the answer, else the place moves on by half of
    /// `speed` over the segment's length. At an end of a path that does not loop the direction
    /// turns around when `bounce` is set (`FUN_002d2328`), else the walk stops (`None`: the game
    /// then answers the facing instead of a direction). After twenty steps the answer is the
    /// object's own position.
    pub fn advance(&mut self, speed: f32, pos: Vec3) -> Option<Vec3> {
        let segs = self.seg_count() as f32;
        if segs <= 0.0 {
            return None;
        }
        for _ in 0..20 {
            let p = self.at(self.t);
            if (p - pos).with_y(0.0).length() > speed {
                return Some(p);
            }
            let i = if self.forward { self.t.floor() } else { (self.t.ceil() - 1.0).max(0.0) };
            let i = if self.loops { i.rem_euclid(segs) } else { i.clamp(0.0, segs - 1.0) };
            let len = (self.at(i + 1.0) - self.at(i)).length().max(1e-3);
            let step = speed / len * 0.5;
            if self.forward {
                self.t += step;
                if self.loops {
                    self.t = self.t.rem_euclid(segs);
                } else if self.t >= segs {
                    self.t = segs;
                    if !self.bounce {
                        return None;
                    }
                    self.toggle();
                }
            } else {
                self.t -= step;
                if self.loops {
                    self.t = self.t.rem_euclid(segs);
                } else if self.t <= 0.0 {
                    self.t = 0.0;
                    if !self.bounce {
                        return None;
                    }
                    self.toggle();
                }
            }
        }
        Some(pos)
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
    /// asks for with -1); the follower starts at the point of the path nearest to the object
    /// (`FUN_002d50c0`). Answers 1 when a path was found, 0 otherwise (the game then also tries the
    /// area's shared paths for objects with flag 0x588 bit 7, which these are not).
    fn set_path(&mut self, id: u32) -> bool {
        let pos = self.inner.obj.motor.pos;
        let paths = &self.fields.group.paths;
        let found = if (id as i32) < 0 { paths.first().cloned() } else { paths.iter().find(|p| u16::from_le_bytes([p.header[0], p.header[1]]) as u32 == id).cloned() };
        match found {
            Some(p) => {
                let run = PathRun::new(u16::from_le_bytes([p.header[0], p.header[1]]), &p, pos);
                if self.log {
                    info!("npc: {} path {} (asked {}) set: {} points, loops {}, start t {:.2}", self.inner.tag, run.id, id as i32, run.points.len(), run.loops, run.t);
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

    /// `FUN_002d25b0(speed, blend, out, path, position, facing)` behind hosts 5.3 and 1.178: the
    /// place on the path moves `speed` (at most 50) along it and the answer is the unit direction
    /// from the object to the new place; when the advance stops (the end of a path that does not
    /// turn around) the answer is the object's facing. With the new place less than ten times the
    /// speed away the game blends the direction from the object's current and its recorded
    /// position by `blend`; both are the same point here [approx].
    fn path_direction(&mut self, speed: f32) -> Vec3 {
        let yaw = self.inner.obj.motor.yaw;
        let facing = Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let pos = self.inner.obj.motor.pos;
        let Some(path) = self.fields.path.as_mut() else { return facing };
        match path.advance(speed.min(50.0), pos) {
            Some(target) => (target - pos).normalize_or(facing),
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
                (Value::Vec([d.x, -d.y, -d.z, 1.0]), St::Approx, "the path's straight segments are followed, not the game's cubic spline (FUN_002d2a98)")
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
                        let t = p.nearest(pos);
                        let tangent = (p.at(t + 0.01) - p.at(t - 0.01)).with_y(0.0).normalize_or_zero();
                        if tangent.length() >= 0.01 {
                            let to = to.normalize();
                            p.forward = to.dot(tangent) >= to.dot(-tangent);
                        }
                    }
                    (Value::None, St::Approx, "the other object is taken as the player; the tangent is the polyline's, not the spline's")
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
        let pos = self.inner.obj.motor.pos;
        let yaw = self.inner.obj.motor.yaw;
        let d = (point - pos).with_y(0.0);
        if d.length() < 1e-3 {
            return true;
        }
        let d = d.normalize();
        // FUN_0017bf10: acos of the dot product when it is inside (-1, 1), else 0 or pi.
        let dot = (d.x * yaw.sin() + d.z * yaw.cos()).clamp(-1.0, 1.0);
        dot.acos().abs() < f32::from_bits(0x3e86_0a91)
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
            172 => (Value::Bool(self.in_front(self.inner.env.player)), St::Approx, "the kind-0x14 part's cone and point are not read: a 15 degree cone about the facing (DAT_0037b950) toward the player's origin"),
            173 => match args_vec(c, 1) {
                Some(v) => (Value::Bool(self.in_front(from_game(v))), St::Approx, "the kind-0x14 part's cone is not read: a 15 degree cone about the facing (DAT_0037b950)"),
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
            // or a point over n frames, or tweened back to zero. The app has no look-at controller:
            // the head stays with the motion.
            174 | 175 | 176 => (Value::None, St::Stub, "the look-at controller at +0x4a0 (FUN_001abf40 tweens, applied by FUN_00189dc8) is not ported: the head follows the motion"),
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
                    (Value::Vec([d.x, -d.y, -d.z, 1.0]), St::Approx, "the path's straight segments are followed, not the game's cubic spline (FUN_002d2a98)")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn path(points: &[[f32; 3]], loops: bool) -> kh2_ard::WalkPath {
        let mut header = [0u8; 8];
        header[0] = 7;
        header[2] = points.len() as u8;
        header[4] = if loops { 1 } else { 2 };
        kh2_ard::WalkPath { header, points: points.to_vec() }
    }

    /// A walk path is followed segment by segment; a path that does not loop turns around at its
    /// ends (FUN_002d2328 flips +0x84), a looping one wraps.
    #[test]
    fn a_walk_path_is_followed_and_turns_around_at_its_ends() {
        // Game space: Y down, Z mirrored; the points lie along the game's +X.
        let p = path(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 0.0, -100.0]], false);
        let mut r = PathRun::new(7, &p, Vec3::new(10.0, 0.0, 0.0));
        assert!((r.t - 0.1).abs() < 1e-4, "{}", r.t);
        // The look-ahead: the first sampled place (steps of half the speed over the segment's
        // length: 0.3, 0.5, 0.7) that lies more than 40 cm from the object.
        let q = r.advance(40.0, Vec3::new(10.0, 0.0, 0.0)).unwrap();
        assert!((q - Vec3::new(70.0, 0.0, 0.0)).length() < 1e-3 && (r.t - 0.7).abs() < 1e-4, "{q:?} t {}", r.t);
        // Walking to each answer in turn reaches the end (app (100, 0, 100)) and turns around.
        let mut pos = q;
        let mut flipped = None;
        for _ in 0..40 {
            pos = r.advance(40.0, pos).expect("a bouncing path never stops");
            if !r.forward {
                flipped = Some(pos);
                break;
            }
        }
        let back = flipped.expect("the end turns the walk around");
        assert!(back.x > 99.0 && back.z < 100.0, "{back:?}");
        // A locked path stops at its end instead.
        let mut locked = PathRun::new(7, &p, Vec3::ZERO);
        locked.bounce = false;
        let mut pos = Vec3::ZERO;
        let mut stopped = false;
        for _ in 0..40 {
            match locked.advance(40.0, pos) {
                Some(q) => pos = q,
                None => {
                    stopped = true;
                    break;
                }
            }
        }
        assert!(stopped && pos.z > 50.0, "{pos:?}");
        // A loop wraps and keeps going forward.
        let mut l = PathRun::new(7, &path(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 0.0, -100.0]], true), Vec3::ZERO);
        let (mut pos, mut wrapped, mut last_t) = (Vec3::ZERO, false, 0.0);
        for _ in 0..60 {
            pos = l.advance(40.0, pos).unwrap();
            wrapped |= l.t < last_t;
            last_t = l.t;
        }
        assert!(wrapped && l.forward && (0.0..3.0).contains(&l.t), "t {}", l.t);
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
}
