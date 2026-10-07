//! The party members' own BDX scripts (stage 1 of docs/party.md section 9: "the member's BDX attached
//! and put in `idle`"). Each member's model file carries a script (BAR type 3 of `obj/<model>.mdlx`,
//! `kh2 bdx P_EX020`): its init event registers the modes (`appear`, `idle`, `idle_time`, `footwork`,
//! `mode_battle`, the spells ...) and its main event only yields; the native friend AI starts modes by
//! name (docs/party.md section 4, action records +0x20). Stage 1 starts the script when the member
//! appears and switches it to `idle`, then runs it every tick through the enemy host (`BevyHost`).
//!
//! The script runs through `friend_host::FriendHost`: its motion requests go through the player-type
//! remap FUN_0017da30 (the member's motion state row, `PartyMember::motion_row`) before the enemy host
//! plays them, and the group-10 entries the members' scripts reach are answered there. The requested
//! slot is handed to `party.rs` (`PartyMember::script_slot`), which plays it while the member stands:
//! `idle` (BDX `P_EX020` R0489 -> R0499) faces the target and asks for motion 0. Movement, effects
//! and the other requests of the script are not carried out (the locomotion owns the body).
//!
//! Log: `KH2_PARTYLOG=1` prints the mode changes; `KH2_BDXLOG=1` the host calls as for enemies.

use std::collections::HashMap;

use bevy::prelude::*;
use kh2_bdx::{Program, Runtime};
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::enemy_ai::AiTrace;
use crate::enemy_host::{BevyHost, Env};
use crate::friend_host::{FriendFields, FriendHost};
use crate::enemy_obj::{Obj, OBJ_SIZE, SELF_BASE};
use crate::party::PartyMember;
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::Sora;
use crate::world::World;

/// Script handles of the members: past the enemies' range (`enemy_ai` uses serials below 0x1000).
const HANDLE_BASE: u32 = 0x1000;

/// The mode the stage puts a new member's script in.
const START_MODE: &str = "idle";

/// The scripts by model name (`None`: the model has no readable script) and the executable's host table.
#[derive(Resource, Default)]
struct FriendScripts {
    by_model: HashMap<String, Option<(Program, bool)>>,
    table: Option<kh2_bdx::HostTable>,
    objs: Vec<t::Objentry>,
    loaded: bool,
}

impl FriendScripts {
    fn get(&mut self, disc: &Disc, model: &str) -> Option<(Program, bool)> {
        if !self.loaded {
            self.loaded = true;
            self.table = disc.read_root("SLUS_210.05").and_then(|exe| kh2_bdx::HostTable::from_exe(&exe));
            if let Ok(mut a) = disc.0.lock() {
                self.objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default();
            }
        }
        let flags124 = self.objs.iter().find(|r| r.model_name == model).is_some_and(|r| r.flags & 8 != 0);
        self.by_model
            .entry(model.to_string())
            .or_insert_with(|| {
                let file = disc.read(&format!("obj/{model}.mdlx"))?;
                let entry = kh2_bar::parse(&file).ok()?.into_iter().find(|e| e.kind == 3)?.data.to_vec();
                match Program::parse(&entry) {
                    Ok(p) => Some((p, flags124)),
                    Err(e) => {
                        warn!("party: {model}: script not readable: {e}");
                        None
                    }
                }
            })
            .clone()
    }
}

/// A member's script runtime.
#[derive(Component)]
pub struct FriendBrain {
    rt: Runtime,
    obj: Obj,
    tag: String,
    flags124: bool,
    started: bool,
    errors_seen: usize,
    mode: Option<String>,
    fields: FriendFields,
}

/// Registers the systems (called by `party::PartyPlugin`, which runs them after its own tick).
pub(crate) fn register(app: &mut App) {
    app.init_resource::<FriendScripts>().add_systems(FixedUpdate, (friend_attach, friend_update).chain().after(crate::party::PartyTick).in_set(Sim::Enemy));
}

fn friend_attach(mut commands: Commands, disc: Res<Disc>, mut scripts: ResMut<FriendScripts>, q: Query<(Entity, &PartyMember), Without<FriendBrain>>) {
    for (ent, m) in &q {
        let Some((prog, flags124)) = scripts.get(&disc, &m.model) else { continue };
        info!("party: {} script {} ({} words) attached", m.model, prog.name, prog.words.len());
        let mut rt = Runtime::new(prog);
        if let Some(t) = &scripts.table {
            rt.set_host_table(t.clone());
        }
        rt.shared.rng = 0x51ed_270b ^ (m.slot as u32).wrapping_mul(0x85eb_ca6b);
        rt.core.mode_log = Some(Vec::new());
        let handle = SELF_BASE + (HANDLE_BASE + m.slot as u32) * OBJ_SIZE as u32;
        commands.entity(ent).insert(FriendBrain {
            rt,
            obj: Obj::new(handle),
            tag: format!("{}#{}", m.model, m.slot),
            flags124,
            started: false,
            errors_seen: 0,
            mode: None,
            fields: FriendFields::default(),
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn friend_update(
    world: Option<Res<World>>,
    trace: Option<ResMut<AiTrace>>,
    sora: Query<&Sora>,
    data: Res<crate::party::PartyData>,
    mut q: Query<(&mut PartyMember, &mut FriendBrain)>,
    anims: Query<&Animator>,
) {
    let (Some(mut trace), Ok(s)) = (trace, sora.single()) else { return };
    let log = std::env::var("KH2_PARTYLOG").is_ok_and(|v| !v.is_empty() && v != "0");
    let paired = HashMap::new();
    for (mut m, mut brain) in &mut q {
        let Ok(anim) = anims.get(m.body) else { continue };
        let rig = anim.rig.clone();
        let FriendBrain { rt, obj, tag, flags124, started, errors_seen, mode, fields } = &mut *brain;
        fields.order = data.order(m.motion_row);
        fields.free = m.free;
        obj.motor.pos = m.position();
        obj.motor.yaw = m.yaw();
        obj.motor.grounded = true;
        obj.slot = anim.slot;
        obj.frame = anim.frame;
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
        };
        let handle = obj.handle;
        let mut host = FriendHost { inner: BevyHost { obj, env: &env, trace: &mut trace, tag: tag.as_str() }, fields };
        if !*started {
            *started = true;
            rt.spawn(&mut host, handle);
            rt.set_mode_named(&mut host, START_MODE);
        }
        rt.update(&mut host);
        let now = rt.current_mode();
        if now != *mode {
            if log {
                info!("party: {tag} script mode {:?} -> {:?}", mode.as_deref().unwrap_or("-"), now.as_deref().unwrap_or("-"));
            }
            *mode = now;
        }
        if let Some(req) = obj.motion_req.take() {
            if log && m.script_slot != Some(req.slot) {
                info!("party: {tag} script asked for slot {} (FUN_0017da30 row {}): played while the member stands", req.slot, m.motion_row);
            }
            m.script_slot = Some(req.slot);
        }
        // Nothing else the script asked for is carried out.
        obj.move_vel = Vec3::ZERO;
        obj.effects.clear();
        obj.outbox.clear();
        obj.player_cmds.clear();
        obj.drops.clear();
        for entry in rt.core.mode_log.as_mut().map(std::mem::take).unwrap_or_default() {
            if log {
                info!("party: {tag} script mode {entry}");
            }
        }
        for (pc, err) in &rt.core.errors[*errors_seen..] {
            warn!("party: {tag}: script error {err:?} at {pc:#06x}");
        }
        *errors_seen = rt.core.errors.len();
    }
}
