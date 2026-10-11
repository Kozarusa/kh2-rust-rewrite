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
use crate::enemy_obj::{from_game, to_game, Obj, OBJ_SIZE, SELF_BASE};
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
    pub paths: Vec<kh2_ard::WalkPath>,
    pub returns: Vec<[u8; 0x10]>,
}

/// A townsperson in the world: its body and the state the script does not own.
#[derive(Component)]
pub struct Npc {
    pub model: String,
    pub body: Entity,
    pub yaw: f32,
    pub grounded: bool,
    pub group: Arc<NpcGroup>,
    /// The entity record (position, rotation, the spawn-record bytes).
    pub record: kh2_ard::Entity,
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
    let mut count = 0;
    for g in run.script.groups.iter().filter(|g| g.script == "map") {
        let Some(groups) = run.ard.spawns.get(&g.tag) else { continue };
        for grp in groups {
            let shared = Arc::new(NpcGroup { tag: g.tag.clone(), paths: grp.paths.clone(), returns: grp.returns.clone() });
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
                let at = game_to_app(e.position);
                let at = world.ground(at, at.y + 100.0, 400.0).map_or(at, |g| at.with_y(g.y));
                let yaw = app_yaw(e.rotation[1]);
                spawned.serial += 1;
                let serial = spawned.serial;
                let root = commands.spawn((Transform::from_translation(at * SCALE).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default(), AreaPart)).id();
                let body = spawn_skinned(&mut commands, root, rig, &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
                let tag = format!("{model}#{serial}");
                let mut ent = commands.entity(root);
                ent.insert(Npc { model: model.clone(), body, yaw, grounded: true, group: shared.clone(), record: e.clone() });
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
                        fields: NpcFields::new(e, &shared),
                        started: false,
                        errors_seen: 0,
                        mode: None,
                        last_frame: 0.0,
                        visible: true,
                    });
                }
                count += 1;
                info!(
                    "npc: {} group {} spawned {tag} (object {:#x}) at ({:.0}, {:.0}, {:.0}) yaw {yaw:.3}, spawn flags {:#x}, {} walk paths{}",
                    run.name,
                    g.tag,
                    e.object,
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
    info!("npc: {}: {count} townspeople spawned from the map program's groups", run.name);
}

/// One tick of every townsperson's script: run it, carry out its motion and movement requests.
#[allow(clippy::too_many_arguments)]
fn npc_update(
    mut commands: Commands,
    world: Option<Res<World>>,
    trace: Option<ResMut<AiTrace>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    sora: Query<&Sora>,
    mut q: Query<(Entity, &mut Transform, &mut Npc, &mut NpcBrain)>,
    mut anims: Query<&mut Animator>,
) {
    let (Some(mut trace), Ok(s)) = (trace, sora.single()) else { return };
    let paired = HashMap::new();
    let cam = cam.single().ok();
    let log = npclog();
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
        fields.tick();
        let mut host = NpcHost { inner: BevyHost { obj, env: &env, trace: &mut trace, tag: tag.as_str() }, fields, log };
        if !*started {
            *started = true;
            rt.spawn(&mut host, handle);
        }
        rt.update(&mut host);
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
        for (pc, err) in &rt.core.errors[*errors_seen..] {
            warn!("npc: {tag}: script error {err:?} at {pc:#06x}");
        }
        *errors_seen = rt.core.errors.len();

        // ---- carry out the requests (as `enemy_ai::ai_update` does for enemies)
        if let Some(req) = obj.motion_req.take() {
            let looping = rig.mset.get(req.slot).and_then(|s| s.motion.as_ref()).is_some_and(|m| m.frame_return < m.frame_end);
            anim.restart(Some(req.slot), looping, req.blend);
            if let Some(s) = req.start {
                anim.frame = s;
                anim.prev_frame = s;
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
    /// +0x24 (object +0xa64): a flag byte the script reads (host 5.7).
    pub flag: u8,
    /// +0x30 (object +0xa70): set by host 5.0 when the follower's test passed.
    pub done: u8,
    /// +0x31: forced done.
    pub forced: u8,
    /// +0x50: a timer counted down by one per frame (`FUN_0018b228`, `DAT_00356f40` = 1).
    pub timer: f32,
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
}

impl NpcFields {
    /// The spawn record's word +0x24: bits 0..6 go into the object's bit array (`FUN_001899d0`).
    pub fn spawn_flags(e: &kh2_ard::Entity) -> u32 {
        u32::from_le_bytes(e.raw[0x24..0x28].try_into().unwrap_or_default())
    }

    pub fn new(e: &kh2_ard::Entity, group: &Arc<NpcGroup>) -> NpcFields {
        let mut f = NpcFields {
            bits: [0; 2],
            word_b08: 0,
            params: [0; 9],
            timer_b00: 0.0,
            follower: Follower::default(),
            prev: Follower::default(),
            path: None,
            group: group.clone(),
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

    /// The per-frame count-downs of the class (`FUN_0018a130`: +0xb00; `FUN_0018b228`: the
    /// follower's +0x50), one frame each.
    fn tick(&mut self) {
        if self.timer_b00 > 0.0 {
            self.timer_b00 -= 1.0;
        }
        if self.follower.timer > 0.0 {
            self.follower.timer -= 1.0;
        }
    }

    /// Host 5.4 (`FUN_0018ae08`): the follower is copied to the previous slot and reset, and the
    /// timer +0xb00 takes the parameter word of its kind (+0xb18 for kind 4, +0xb24 for 5, +0xb30
    /// otherwise).
    fn end_follower(&mut self) {
        self.prev = self.follower.clone();
        self.follower.done = 0;
        self.follower.forced = 0;
        self.follower.kind = 0;
        self.follower.mode = 0;
        self.follower.target = 0;
        self.follower.timer = 0.0;
        let w = match self.prev.kind {
            4 => self.params[2],
            5 => self.params[5],
            _ => self.params[8],
        };
        self.timer_b00 = f32::from_bits(w);
    }
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

    /// Moves the place on the path `dist` centimetres along the current direction and returns the
    /// point there; `None` when a non-looping path's end stops the walk (the game then answers
    /// the facing instead of a direction). At an end of a non-looping path the direction turns
    /// around when `bounce` is set, as `FUN_002d2a98` does through `FUN_002d2328`.
    pub fn advance(&mut self, dist: f32) -> Option<Vec3> {
        let segs = self.seg_count() as f32;
        if segs <= 0.0 {
            return None;
        }
        let mut left = dist.max(0.0);
        let mut guard = 0;
        while left > 0.0 && guard < 64 {
            guard += 1;
            let i = if self.forward { self.t.floor() } else { (self.t.ceil() - 1.0).max(0.0) };
            let i = if self.loops { i.rem_euclid(segs) } else { i.clamp(0.0, segs - 1.0) };
            let a = self.at(i);
            let b = self.at(i + 1.0);
            let len = (b - a).length().max(1e-3);
            let f = self.t - i;
            let (room, dir) = if self.forward { ((1.0 - f) * len, 1.0) } else { (f * len, -1.0) };
            if left <= room {
                self.t += dir * left / len;
                left = 0.0;
            } else {
                left -= room;
                self.t = if self.forward { i + 1.0 } else { i };
                if self.loops {
                    self.t = self.t.rem_euclid(segs);
                } else if (self.forward && self.t >= segs) || (!self.forward && self.t <= 0.0) {
                    if !self.bounce {
                        return None;
                    }
                    self.toggle();
                }
            }
        }
        Some(self.at(self.t))
    }
}

// ---------------------------------------------------------------------------------------------------
// The host

/// The townsperson's script host: the group 5 library and the group 1 path hosts, the rest the
/// enemy host's.
pub(crate) struct NpcHost<'a, 'b> {
    pub(crate) inner: BevyHost<'a>,
    pub(crate) fields: &'b mut NpcFields,
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
    /// header's u16 id); the follower starts at the point of the path nearest to the object
    /// (`FUN_002d50c0`). Answers 1 when a path was found, 0 otherwise (the game then also tries the
    /// area's shared paths for objects with flag 0x588 bit 7, which these are not).
    fn set_path(&mut self, id: u32) -> bool {
        let pos = self.inner.obj.motor.pos;
        let found = self.fields.group.paths.iter().find(|p| u16::from_le_bytes([p.header[0], p.header[1]]) as u32 == id).cloned();
        match found {
            Some(p) => {
                let run = PathRun::new(id as u16, &p, pos);
                if self.log {
                    info!("npc: {} path {id} set: {} points, loops {}, start t {:.2}", self.inner.tag, run.points.len(), run.loops, run.t);
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
        match path.advance(speed.min(50.0)) {
            Some(target) => (target - pos).normalize_or(facing),
            None => facing,
        }
    }

    fn group5(&mut self, index: u16, a: &[u32]) -> Option<(Value, St, &'static str)> {
        let arg = |k: usize| a.get(k).copied().unwrap_or(0);
        let me = self.inner.obj.handle;
        Some(match index {
            // 5.0 (0x2ea050 -> FUN_0018a8b0): while the follower is not done, FUN_0018b148 tests it:
            // true when it is forced (+0x31), or when it has a region (+0x20) and the object's
            // position inside it compared with the follower's threshold passes; a passing test sets
            // +0x30 (done). Without a region the test fails. The app has no regions: a follower
            // without one never passes, as in the game.
            0 => {
                let f = &mut self.fields.follower;
                if f.done != 0 {
                    (Value::Int(0), St::Impl, "")
                } else if f.forced != 0 {
                    f.done = 1;
                    (Value::Int(1), St::Impl, "")
                } else {
                    (Value::Int(0), St::Approx, "the follower's region test (FUN_001d8c18 on +0x20) is not ported: no region, the test fails as the game's does without one")
                }
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
            // 5.6 (0x2ea300 -> FUN_0018af10): the unit direction from the object to the follower's
            // target: mode 1 the target object, 2 the player, 3 the point; the object's facing when
            // the follower is not live (FUN_0018b110: forced or with a region) or has no target.
            6 => {
                let yaw = self.inner.obj.motor.yaw;
                let facing = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                let pos = self.inner.obj.motor.pos;
                let f = &self.fields.follower;
                let live = f.forced != 0;
                let target = match (live, f.mode) {
                    (true, 2) => Some(self.inner.env.player),
                    (true, 3) => Some(from_game(f.point)),
                    (true, 1) if f.target == me => Some(pos),
                    _ => None,
                };
                match target {
                    Some(t) => (vec_value((t - pos).normalize_or(facing)), St::Approx, "mode 1 targets other than self are not known to this host; a follower without a region counts as not live"),
                    None => (vec_value(facing), St::Approx, "the follower is not live (no region, not forced) or has no target: the facing"),
                }
            }
            // 5.7 (0x2ea350 -> FUN_0018af08): the byte at object +0xa64 (the follower's +0x24).
            7 => (Value::Int(self.fields.follower.flag as i32), St::Impl, ""),
            // 5.11 (0x2ea618 -> FUN_00189e70 -> FUN_0018b250): 1 unless the follower is done (+0x30)
            // with a positive timer (+0x50) and (kind 4) the player's part 5 still meets the object's,
            // or (kind 5) the partner object still exists, points back at this object and plays
            // motion 0x32; then 0. Without the part test (the app has no NPC collision parts) kind 4
            // keeps 1 as the game does when the parts do not meet.
            11 => {
                let f = &self.fields.follower;
                let v = if f.done != 0 && f.timer > 0.0 && f.kind == 5 { 0 } else { 1 };
                (Value::Int(v), St::Approx, "the part test of kind 4 (FUN_0018aa08) and the partner's motion test of kind 5 are not ported")
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
            // object +0xb10 (the class's parameter block).
            33 => {
                let at = arg(1);
                let mut all = true;
                for k in 0..9u32 {
                    let mut b = [0u8; 4];
                    if self.inner.read_mem(at.wrapping_add(4 * k), &mut b) {
                        self.fields.params[k as usize] = u32::from_le_bytes(b);
                    } else {
                        all = false;
                    }
                }
                if all {
                    (Value::None, St::Impl, "")
                } else {
                    (Value::None, St::Missing, "5.33 with a block outside the object's memory")
                }
            }
            // 5.34 (0x2ea380 -> FUN_0016bbe8): a point of the object: the position of its part of
            // kind 0x11 (FUN_001894d0, a collision record) when it has one, else its lock-on point
            // (FUN_0016b6e0: a target record on the object). The app's townspeople have no parts:
            // the object's position.
            34 => (vec_value(self.inner.obj.motor.pos), St::Approx, "the kind-0x11 part's point (FUN_001894d0) is not read; the object's position"),
            _ => return None,
        })
    }

    fn group1(&mut self, index: u16, a: &[u32]) -> Option<(Value, St, &'static str)> {
        let arg = |k: usize| a.get(k).copied().unwrap_or(0);
        Some(match index {
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

impl Host for NpcHost<'_, '_> {
    fn call(&mut self, c: &Call) -> Value {
        if let Some((g, index, a)) = words(c) {
            let answer = match g {
                5 => self.group5(index, &a),
                1 => self.group1(index, &a),
                _ => None,
            };
            if let Some((v, st, why)) = answer {
                let tag = self.inner.tag;
                self.trace().record(tag, c, &v, st, why);
                return v;
            }
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
        assert_eq!(r.advance(40.0), Some(Vec3::new(50.0, 0.0, 0.0)));
        // Past the corner: 50 to the corner, then 30 along the second segment (app +Z).
        assert_eq!(r.advance(80.0), Some(Vec3::new(100.0, 0.0, 30.0)));
        // Past the end: the rest of the way back.
        let end = r.advance(100.0).unwrap();
        assert!(!r.forward);
        assert!((end - Vec3::new(100.0, 0.0, 70.0)).length() < 1e-3, "{end:?}");
        // A locked path stops at its end instead.
        let mut locked = PathRun::new(7, &p, Vec3::ZERO);
        locked.bounce = false;
        assert_eq!(locked.advance(500.0), None);
        // A loop wraps.
        let mut l = PathRun::new(7, &path(&[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [0.0, 0.0, -100.0]], true), Vec3::ZERO);
        let q = l.advance(100.0 + 100.0 * 2f32.sqrt() + 100.0 + 10.0).unwrap();
        assert!((q - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-2, "{q:?}");
    }

    /// The spawn record's word +0x24 fills bits 0..6 of the object's bit array (FUN_001899d0) and
    /// bit 0 puts the follower on the walk path (kind 2).
    #[test]
    fn spawn_flags_fill_the_bit_array() {
        let mut raw = [0u8; 0x40];
        raw[0x24] = 0x85;
        let e = kh2_ard::Entity { object: 0x4d8, position: [0.0; 3], rotation: [0.0; 3], spawn_kind: 0, spawn_arg: 0, serial: 0, raw };
        let g = Arc::new(NpcGroup { tag: "a_00".into(), paths: Vec::new(), returns: Vec::new() });
        let f = NpcFields::new(&e, &g);
        assert!(f.test_bit(0) && f.test_bit(2) && !f.test_bit(1));
        assert!(!f.test_bit(7), "only bits 0..6 are copied");
        assert_eq!(f.follower.kind, 2);
        let mut f = f;
        f.params[8] = 90f32.to_bits();
        f.end_follower();
        assert_eq!((f.prev.kind, f.follower.kind), (2, 0));
        assert_eq!(f.timer_b00, 90.0);
    }
}
