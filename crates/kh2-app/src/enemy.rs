//! Enemies (Heartless Shadow M_EX020 and Soldier M_EX010, the Nobody Dusk M_EX990): models,
//! motions, hitboxes and stats from the disc. What an enemy does runs under the game's own BDX
//! script (`enemy_ai.rs`); this module spawns the waves and holds the hit reactions of the victim
//! side (damage, flinch, push, blow).
//!
//! Waves. In an area whose battle script (`ard/<area>.ard`, entry `btl`) loads spawn groups with
//! enemies the app has, a wave is the next such group: its enemies at the places and with the
//! spawn records the group gives (tt04: the Dusks, `docs/enemies.md`). Which of the script's
//! programs the game runs depends on story progress, which the app does not have, so the groups
//! take turns in script order. Elsewhere (and with `KH2_AREA_WAVES=0`) a wave is the app's own:
//! Shadows around Sora, with a Soldier from the second wave on (not from the disc).
//!
//! Enemy motion sets use the default mapping (slot = motion id): 0 idle, 10/12 small damage
//! front/back, 16/17 blow + land. Attack motions carry attack triggers (atkp + collision group).
//! An enemy without a script (its BDX entry did not load) only stands and reacts to hits.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;

use crate::assets::{CurrentArea, Disc};
use crate::world::World;
use crate::combat::{hit_is_ineffective, Fighter, Hit, IneffectiveHit, ReactionData, ReactionStatus, Team};
use crate::data::{GameData, Stats, GRAVITY, LOGIC_HZ};
use crate::reaction_rules::{self as rules, Blow, Push, VictimAction, VictimCtx};
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::sora::{Rigs, Sora};
use crate::SCALE;

const IDLE: usize = 0;
const HURT_FRONT: usize = 10;
const BLOW: usize = 16;

pub struct Kind {
    pub name: &'static str,
    /// Row in `enmp`.
    pub enmp: u16,
    /// The object's model name (`obj/<model>.mdlx`, `.mset`, `.apdx`; its `00objentry` row).
    pub model: &'static str,
    /// The script's `dead` mode starts element 2 of the object's effect file (the death effect), so
    /// the damage code starts it when HP reaches 0 (`GameEffect::Death`).
    pub death_effect: bool,
    /// The object has a status block, so a hit can hurt it: false for an object whose `00objentry` row has `NeoStatus` 0
    /// and whose script never calls `set_obj_param` (`M_EX850`): nothing builds the block (FUN_001b2148 is the only builder).
    pub status: bool,
}

pub const SHADOW: Kind = Kind { name: "Shadow", enmp: 34, model: "M_EX020", death_effect: true, status: true };

pub const SOLDIER: Kind = Kind { name: "Soldier", enmp: 33, model: "M_EX010", death_effect: true, status: true };

/// The Dusk. Its `enmp` row is the constant its script hands to `set_obj_param` (host 2.9, the
/// call that builds the status block from the row, FUN_001b2148): 41 (BDX `M_EX990` at 0x073c).
pub const DUSK: Kind = Kind { name: "Dusk", enmp: 41, model: "M_EX990", death_effect: true, status: true };

/// The Struggle opponent `N_EX570_BTL` (object 0x678; Seifer by the community's model names), in
/// tt04's battle groups b_41, b_42, b_45 and b_51. Its script picks the `enmp` row it hands to
/// `set_obj_param` from spawn parameter 1 (BDX `N_EX570_BTL` R1864 at 0x077d): 155 for 3 and 4, else
/// 135. The kind starts with 135; host 2.9 rebuilds the stats from the row the script asks for. Its
/// `dead` mode (BDX 0x174d) plays motion 16 or 10 and waits for ever: no death effect, no fade.
pub const SEIFER: Kind = Kind { name: "Seifer", enmp: 135, model: "N_EX570_BTL", death_effect: false, status: true };

/// The Nobody `M_EX930` (object 0x138), with two Dusks in tt04's battle group b_21; "Dancer" is
/// inferred from its mode names (`atk_force_dance`, `atk_spin_*`, `atk_kick`). Its script hands
/// `enmp` row 48 to `set_obj_param` (BDX `M_EX930` R0f13 at 0x0f3a, the constant pushed at 0x04ce). Its `dead` mode (R06bd) starts element 2 of its effect file, fades out and hides, like
/// the Dusk's.
pub const DANCER: Kind = Kind { name: "Dancer", enmp: 48, model: "M_EX930", death_effect: true, status: true };

/// The Struggle opponent `N_EX500_BTL` (object 0x4d7) of tt04's battle group b_50, called by its model
/// (its name is not in the data the app reads). Its script hands `enmp` row 150 to `set_obj_param`
/// (BDX `N_EX500_BTL` 0x0736). Unlike `N_EX570_BTL` its `dead` mode (R0900) starts element 2 of its
/// effect file, fades out and hides, as the Dusk's does.
pub const N_EX500: Kind = Kind { name: "N_EX500", enmp: 150, model: "N_EX500_BTL", death_effect: true, status: true };

/// The Struggle opponent `N_EX600_BTL` (object 0x82f) of tt04's battle group b_52, called by its model.
/// Its script hands `enmp` row 158 to `set_obj_param` (BDX `N_EX600_BTL` 0x07fd); its `dead` mode
/// (R09c8) starts element 2 of its effect file, fades out and hides.
pub const N_EX600: Kind = Kind { name: "N_EX600", enmp: 158, model: "N_EX600_BTL", death_effect: true, status: true };

/// `M_EX940` (object 0x139, `ObjectType` 0x15: the second enemy class, `FUN_0016e8c0` -> `FUN_001c1ec8`), four of
/// them in tt04's b_44, called by its model. Its BDX script (5159 words; modes `atk_brush`, `atk_jump`, `atk_throw`,
/// `atk_fullpower`, `mode_battle_boss`) hands `enmp` row 49 to `set_obj_param` (host 2.9, constant 49 pushed at
/// 0x06e2 for `R124c`); its `dead` mode (R08db) starts element 2 of its effect file, fades out and hides.
pub const M_EX940: Kind = Kind { name: "M_EX940", enmp: 49, model: "M_EX940", death_effect: true, status: true };

/// `M_EX850` (object 0x6bf, `ObjectType` 4), the object `M_EX940` makes with host 1.0 when a blow-class hit lands (BDX
/// `M_EX940` R0082 at 0x0082: `create_object(1727)`, the `SpawnObject1` of its objentry row). It is nobody's group
/// member: the script places it at the ground point under `M_EX940` (1.98, 1.148) and it runs its own script (3101 words;
/// modes `appear`, `idle`, `idle_time`, `freeze`, `event` ..; its event-11 handler answers reaction commands 481, 528 and
/// 586). Its script never calls `set_obj_param` and its objentry row has `NeoStatus` 0, so nothing builds a status block
/// (FUN_001b2148 is the only builder) and `Kind::status` is false: it cannot be hurt. `enmp` is only a stand-in for the
/// numbers `spawn_enemy` asks of the data (any row: HP only has to be above 0) [inferred].
pub const M_EX850: Kind = Kind { name: "M_EX850", enmp: 49, model: "M_EX850", death_effect: false, status: false };

/// `M_EX760` (object 0x131 = 305, `ObjectType` 4, `NeoStatus` 1028, Weight 10, `SpawnLimiter` 7), the enemy of 15 areas (bb08,
/// ca09, ca13, ca19, hb02, hb08 ..), called by its model (the data gives no name). Its BDX script (7982 words; modes `appear`,
/// `btl_attack`, `atk_turn`, `atk_jump`, `atk_down`, `footwork`, `mode_revenge`, `leave`, `dead`) hands `enmp` row 36 to
/// `set_obj_param` (host 2.9 at 0x1cec, in the constructor routine R1cc5 that the init calls at 0x0c02 with 36 as its 8th
/// word); its `dead` mode (R0dbf at 0x07b2) starts element 2 of its effect file (1.87), sets flags 3 and 6, fades out and hides.
pub const M_EX760: Kind = Kind { name: "M_EX760", enmp: 36, model: "M_EX760", death_effect: true, status: true };

/// `M_EX420` (object 120, `ObjectType` 4; `00objentry` row 120: `NeoStatus` 1003, flags 4), spawned in 33 areas of the other
/// worlds (the area survey), called by its model. Its script hands `enmp` row 9 to `set_obj_param` (host 2.9: BDX `M_EX420`
/// R174d at 0x1774, the constant pushed at 0x063e by the call at 0x0656); its `dead` mode (R082e) starts element 2 of its
/// effect file, fades out and hides, like the Dusk's.
pub const M_EX420: Kind = Kind { name: "M_EX420", enmp: 9, model: "M_EX420", death_effect: true, status: true };

/// Every enemy kind the app has.
pub const KINDS: [&Kind; 11] = [&SHADOW, &SOLDIER, &DUSK, &SEIFER, &DANCER, &N_EX500, &N_EX600, &M_EX940, &M_EX850, &M_EX420, &M_EX760];

/// The rigs of the enemy kinds, by `Kind::name` (Shadow and Soldier share `sora::Rigs`' copies).
#[derive(Resource, Default)]
pub struct EnemyRigs(pub HashMap<&'static str, Arc<Rig>>);

impl EnemyRigs {
    pub fn get(&self, kind: &Kind) -> Option<Arc<Rig>> {
        self.0.get(kind.name).cloned()
    }
}

fn load_rig(disc: &Disc, model: &str) -> Option<Arc<Rig>> {
    let m = disc.read(&format!("obj/{model}.mdlx")).and_then(|d| kh2_model::mdlx(&d))?;
    let s = disc.read(&format!("obj/{model}.mset")).and_then(|d| kh2_anim::Mset::parse(&d))?;
    let apdx = Some(format!("obj/{model}.apdx")).filter(|a| disc.read(a).is_some());
    Some(Arc::new(Rig { model: m, mset: s, apdx }))
}

/// Adds the effect files of the enemy rigs that `fx::setup` does not know (it loads Sora's,
/// the keyblade's, the Shadow's and the Soldier's), so motion effects and the death effect of
/// the Dusk find its `obj/M_EX990.apdx`.
fn register_effects(disc: Res<Disc>, rigs: Res<EnemyRigs>, lib: Option<ResMut<crate::fx::EffectLib>>) {
    let Some(mut lib) = lib else { return };
    let common = disc.read("effect/texcommon.dpd").and_then(|b| kh2_fx::Dpd::parse(&b).ok());
    for rig in rigs.0.values() {
        let Some(name) = &rig.apdx else { continue };
        if lib.objects.contains_key(name) {
            continue;
        }
        let id = 2 + lib.objects.len();
        match crate::fx::load(&disc, name, id, common.as_ref()) {
            Some(p) => {
                lib.objects.insert(name.clone(), Arc::new(p));
            }
            None => warn!("{name}: no effects"),
        }
    }
}

/// One enemy of an area's spawn group: the kind, where it stands (metres, the app's space), its
/// facing and the spawn kind byte of its record (host 1.110).
#[derive(Clone)]
pub struct Member {
    pub kind: &'static Kind,
    pub at: Vec3,
    pub yaw: f32,
    pub spawn_kind: i32,
    /// The record's words at +0x20 and +0x24, which the game copies to the object's spawn
    /// parameters 1 and 2 (FUN_0016b870; host 1.39).
    pub spawn_params: [i32; 2],
    /// The record's `00objentry` id (`FUN_001699a8` answers it for the object; the form experience of a
    /// defeat, `prize::defeat_exp`, is keyed by it).
    pub object_id: u32,
    /// Index into `AreaGroup::parts`: the group record (one tag can hold several) the entity is in.
    pub part: usize,
    /// The record's byte +0x1d (the entrance for spawn kind 1, `FUN_001981a0`; the placement for kind 2).
    pub spawn_arg: u8,
    /// The record's halfword +0x1e: the serial the game finds the object by (`FUN_0016b0d8`).
    pub serial: u16,
    /// The record's halfword +0x2a: frames to wait after this entity is created by a kind-2 group
    /// (`FUN_001991a0` writes it to the group's timer).
    pub delay: u16,
    /// The record's byte +0x30: the wave of a kind-2 group from which the entity may appear (`FUN_001991a0`).
    pub wave: u8,
    /// The `00objentry` `SpawnLimiter` byte (row +0x54) of the object, which `FUN_001996f0` sums.
    pub limiter: u8,
    /// The record's halfword +0x2e: the radius (centimetres) of placement 2, which `FUN_001991a0` passes to
    /// `FUN_00199a80`.
    pub radius: u16,
    /// `00objentry` flags (+0x48) bit 3: placements 1 and 2 pick a point in the air (`FUN_001a5ee0`) instead of one
    /// on the floor (`FUN_001a5ea8`).
    pub flies: bool,
}

/// One group record of a spawn tag (BAR type 12; `FUN_00198430` builds the live group from it).
#[derive(Clone)]
pub struct GroupPart {
    /// Header +2.
    pub id: u16,
    /// Header byte +0: 1 plain / exit, 2 battle group with on-demand entities (`FUN_00198b30`).
    pub kind: u8,
    /// Kind 5 or flag bit 0: the group starts disabled and creates nothing (`FUN_00198430`, `FUN_00198920`).
    pub disabled: bool,
    pub header: [u8; 0x2c],
    pub activators: Vec<kh2_ard::Activator>,
    /// Every entity of the record (also the ones the app does not have): spawn kind, wave byte +0x30 and the
    /// `00objentry` `SpawnLimiter` byte (row +0x54), which `FUN_001996f0` sums.
    pub entities: Vec<(u8, u8, u8)>,
    /// Every entity of the record as the group's enemy counter sees it (`FUN_00198750`, `FUN_001994c8`).
    pub counted: Vec<Counted>,
}

/// One entity of a group record for the group's counter of enemies (`FUN_00198750`).
#[derive(Clone, Debug, PartialEq)]
pub struct Counted {
    /// Record byte +0x1c.
    pub spawn_kind: u8,
    /// Record byte +0x1d (the entrance of a spawn-kind-1 entity).
    pub arg: u8,
    /// Record halfword +0x1e.
    pub serial: u16,
    /// `00objentry` `ObjectType` 4 or 0x15: the entities `FUN_001994c8` counts.
    pub enemy: bool,
}

impl GroupPart {
    fn u16_at(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.header[o], self.header[o + 1]])
    }
    /// Header +0x1c: the kind-2 group's timer when it is set up (`FUN_00198430`), frames.
    pub fn start_delay(&self) -> u16 {
        self.u16_at(0x1c)
    }
    /// Header +0x20: the timer after one of its objects is removed (`FUN_001993f8`), frames.
    pub fn removal_delay(&self) -> u16 {
        self.u16_at(0x20)
    }
}

/// A spawn group of the area's battle script that holds enemies the app has.
#[derive(Clone)]
pub struct AreaGroup {
    pub program: u16,
    pub name: String,
    /// The group records of the tag, in file order.
    pub parts: Vec<GroupPart>,
    pub members: Vec<Member>,
    /// Enemies (`ObjectType` 4) of the group that the app does not have, by model name.
    pub skipped: Vec<String>,
    /// Where the group puts the player (its entity 0x236, the player's id, FUN_0016b078), metres.
    pub player: Option<Vec3>,
    /// The mission the group's btl program starts (opcode 0x15, `FUN_001b9338`): id and file name (WP-7E).
    pub mission: Option<(u32, String)>,
    /// The program also loads a group with the Struggle orb pool object (`00objentry` type 22 subtype 8,
    /// `PRIZE_TT`, made by `FUN_001c0400` -> `FUN_001cac70`) (WP-7E).
    pub orb_pool: bool,
    /// The triggers (opcode 0xc) of the area's `btl` program and of the `evt` program with the same id, which
    /// the mission's end looks up (WP-7E2) [inferred: the area record runs the `evt` program of the same number].
    pub triggers: Vec<crate::struggle::Trig>,
}

/// The current area's enemy spawn groups (see the module header).
#[derive(Resource, Default)]
pub struct AreaWaves {
    area: Option<String>,
    pub groups: Vec<AreaGroup>,
}

/// `ObjectType` of enemies in `00objentry`.
const OBJECT_ENEMY: u8 = 4;
/// `ObjectType` 0x15: the second enemy class (`FUN_0016e8c0` -> `FUN_001c1ec8`, the type-4 constructor with another
/// vtable, 0x35f580; `FUN_00198750` counts it like type 4).
const OBJECT_ENEMY_B: u8 = 0x15;
/// The player's entity id in spawn records (`00objentry` row `PLAYER`; FUN_0016b078 finds the player by it).
const PLAYER_ENTITY: u32 = 0x236;

/// The spawn groups that the area's battle script loads and that hold enemies of `KINDS`, in
/// script order. The positions are the game's (centimetres, Y down, Z mirrored: the app's space is
/// the game's turned half a turn about X); a game facing of yaw `r` (direction (sin r, 0, cos r),
/// FUN_0016a7f0) is the app's yaw `PI - r`.
pub fn area_groups(disc: &Disc, area: &str) -> Vec<AreaGroup> {
    use kh2_tables::tables as t;
    let Some(file) = disc.read(&format!("ard/{area}.ard")) else { return Vec::new() };
    let Some((ard, _)) = kh2_ard::Area::parse(&file) else { return Vec::new() };
    let rows = {
        let Ok(mut a) = disc.0.lock() else { return Vec::new() };
        kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default()
    };
    let mut out: Vec<AreaGroup> = Vec::new();
    for p in ard.scripts.get("btl").into_iter().flatten() {
        let mission = crate::struggle::mission_command(p);
        let orb_pool = p.spawns().flat_map(|n| ard.spawns.get(&n).into_iter().flatten().flat_map(|g| &g.entities).map(|e| e.object).collect::<Vec<_>>()).any(|o| {
            rows.iter().any(|r| r.object_id == o && r.object_type == crate::struggle::ORB_POOL_TYPE.0 && r.sub_type == crate::struggle::ORB_POOL_TYPE.1)
        });
        for name in p.spawns() {
            // One entry per (program, group): the mission and triggers are the loading program's.
            if out.iter().any(|g| g.name == name && g.program == p.id) {
                continue;
            }
            let triggers = [Some(p), ard.scripts.get("evt").and_then(|e| e.iter().find(|q| q.id == p.id))]
                .into_iter()
                .flatten()
                .flat_map(|q| q.commands.iter().filter(|c| c.opcode == 0xc).filter_map(|c| kh2_ard::parse_trigger(&c.args)))
                .map(|t| crate::struggle::Trig::from_ard(&t))
                .collect();
            let mut g = AreaGroup { program: p.id, name: name.clone(), parts: Vec::new(), members: Vec::new(), skipped: Vec::new(), player: None, mission: mission.clone(), orb_pool, triggers };
            for rec in ard.spawns.get(&name).into_iter().flatten() {
                let kind = (rec.kind & 0xff) as u8;
                let limiter = |o: u32| rows.iter().find(|r| r.object_id == o).map_or(0, |r| r.spawn_limiter);
                g.parts.push(GroupPart {
                    id: rec.id,
                    kind,
                    disabled: kind == 5 || rec.flags() & 1 != 0,
                    header: rec.header,
                    activators: rec.activators.clone(),
                    entities: rec.entities.iter().map(|e| (e.spawn_kind, e.raw[0x30], limiter(e.object))).collect(),
                    counted: rec
                        .entities
                        .iter()
                        .map(|e| Counted {
                            spawn_kind: e.spawn_kind,
                            arg: e.spawn_arg,
                            serial: e.serial,
                            enemy: rows.iter().any(|r| r.object_id == e.object && (r.object_type == OBJECT_ENEMY || r.object_type == OBJECT_ENEMY_B)),
                        })
                        .collect(),
                });
            }
            for (part, e) in ard.spawns.get(&name).into_iter().flatten().enumerate().flat_map(|(i, g)| g.entities.iter().map(move |e| (i, e))) {
                if e.object == PLAYER_ENTITY && g.player.is_none() {
                    let [x, y, z] = e.position;
                    g.player = Some(Vec3::new(x, -y, -z) * SCALE);
                }
                let Some(row) = rows.iter().find(|r| r.object_id == e.object) else { continue };
                match KINDS.iter().find(|k| k.model == row.model_name) {
                    Some(kind) => {
                        let [x, y, z] = e.position;
                        g.members.push(Member {
                            kind,
                            at: Vec3::new(x, -y, -z) * SCALE,
                            yaw: std::f32::consts::PI - e.rotation[1],
                            spawn_kind: e.spawn_kind as i32,
                            spawn_params: [0x20, 0x24].map(|o| i32::from_le_bytes(e.raw[o..o + 4].try_into().unwrap_or_default())),
                            object_id: e.object,
                            part,
                            spawn_arg: e.spawn_arg,
                            serial: e.serial,
                            delay: u16::from_le_bytes([e.raw[0x2a], e.raw[0x2b]]),
                            wave: e.raw[0x30],
                            limiter: row.spawn_limiter,
                            radius: u16::from_le_bytes([e.raw[0x2e], e.raw[0x2f]]),
                            flies: row.flags >> 3 & 1 != 0,
                        });
                    }
                    None if row.object_type == OBJECT_ENEMY => g.skipped.push(row.model_name.clone()),
                    None => {}
                }
            }
            if !g.members.is_empty() {
                out.push(g);
            }
        }
    }
    out
}

/// The enemy groups the area's `btl` program loaded (its opcode 0 / 9 commands as `kh2_ard::run::run_area`
/// ran them, the port of `FUN_00181148`), in run order. A btl program the script does not have loads none.
pub fn btl_groups<'a>(groups: &'a [AreaGroup], run: &kh2_ard::run::ScriptRun) -> Vec<&'a AreaGroup> {
    let mut out: Vec<&AreaGroup> = Vec::new();
    for l in run.groups.iter().filter(|l| l.script == "btl") {
        if let Some(g) = groups.iter().find(|g| g.program == l.program && g.name == l.tag)
            && !out.iter().any(|o| std::ptr::eq(*o, g))
        {
            out.push(g);
        }
    }
    out
}

/// Whether setting the group up creates this member at once (`FUN_00198750` -> `FUN_00198920`): spawn kinds 0
/// and 3, and 1 only for the current entrance (`FUN_001981a0`). Spawn kind 2 is only counted
/// (`FUN_001994c8`) and waits for its kind-2 group (`KindTwo`). A disabled group (`FUN_00198920` tests the
/// group's bit 0) and a kind-9 group (`FUN_00198750` skips the entities) create nothing.
pub fn created_at_load(m: &Member, part: &GroupPart, entrance: u8) -> bool {
    !part.disabled
        && part.kind != 9
        && match m.spawn_kind {
            0 | 3 => true,
            1 => m.spawn_arg == entrance,
            _ => false,
        }
}

/// A kind-2 group's own state (`FUN_00198430` sets it up; the group object's +0x14 timer and +0x20 wave).
#[derive(Clone, Debug, PartialEq)]
pub struct KindTwo {
    /// Index into `AreaGroup::parts`.
    pub part: usize,
    /// Frames until the group may create again (+0x14; header +0x1c at set-up).
    pub timer: f32,
    /// The wave (+0x20, 0 at set-up, at most 200).
    pub wave: u8,
    /// Serials of the members defeated for good: the table at 0x35d278 that `FUN_00198220` enters an enemy in when
    /// it is created (`FUN_00198150`: object types 4, 0xd, 0x15, 0x17) and `FUN_001993f8` takes it out of when it
    /// is removed alive, or dead with a wave byte above 99 (`FUN_00199378`). The game keeps the table for four
    /// areas (`FUN_00194188`); the app keeps it for one run of the area's scripts.
    pub killed: Vec<u16>,
}

/// The tables of the defeated that outlive an area (`FUN_00194188`, on `0x35d278`): four slots, each keyed by the
/// area byte (`0x33e831`) with an age, a counter of area starts (+0x824) and the entered serials (256 halfwords).
/// An area start (`FUN_00181148`) takes the slot of its own key whose age is at least `counter - keep` (the newest
/// such slot) and keeps its entries; with none it clears the least recent slot (a free one, key -1, first) and tags
/// it with the key. Either way the slot's age becomes the counter, which then counts up; after a hit the ages below
/// the hit's age go up by one first. `keep` is `DAT_0035cd28` (3, area script opcode 6; 0 when the map program
/// byte is above 0x32), so an area visited again within `keep` area starts finds its defeated members still down.
/// `FUN_00194330` (through `FUN_00168960`, an object system reset) clears everything and the counter.
#[derive(Clone, Debug, PartialEq)]
pub struct DefeatedTables {
    slots: [DefeatedSlot; 4],
    /// +0x824: how many area starts have stamped a slot.
    counter: i32,
    /// +0x820: the slot of the area now running.
    chosen: usize,
    /// The world byte (`0x33e830`) of the last area start (`DAT_00522178`); `None` before the first.
    world: Option<[u8; 2]>,
}

#[derive(Clone, Debug, PartialEq)]
struct DefeatedSlot {
    key: i32,
    age: i32,
    serials: Vec<u16>,
}

impl DefeatedSlot {
    fn cleared() -> DefeatedSlot {
        DefeatedSlot { key: -1, age: 0, serials: Vec::new() }
    }
}

impl Default for DefeatedTables {
    fn default() -> Self {
        DefeatedTables { slots: std::array::from_fn(|_| DefeatedSlot::cleared()), counter: 0, chosen: 0, world: None }
    }
}

impl DefeatedTables {
    /// `FUN_001689c0`: when the world differs from the last area start's (`DAT_00522178`) the object system is reset
    /// (`FUN_00168960` -> `FUN_00194318` -> `FUN_00194330`) before the area is built, so every slot is cleared. The
    /// world is the two-letter prefix of the area name. Three area-retry handlers (`FUN_002d8b98`, `FUN_002df3d8`,
    /// `FUN_002e3c78`) also call `FUN_00194330` before re-entering the same area; the app has no retry yet.
    pub fn enter_world(&mut self, world: [u8; 2]) {
        if self.world != Some(world) {
            self.clear();
            self.world = Some(world);
        }
    }

    /// `FUN_00194330`: every slot cleared and the counter back to 0.
    pub fn clear(&mut self) {
        *self = DefeatedTables::default();
    }

    /// `FUN_00194188(table, keep)` for the area byte `key`: selects the slot the area's defeated members are in.
    pub fn enter(&mut self, key: u8, keep: i32) {
        let key = i32::from(key);
        let mut best: Option<usize> = None;
        let mut victim = 0;
        for i in 0..4 {
            let s = &self.slots[i];
            if s.key == key && self.counter - keep <= s.age && best.is_none_or(|b| s.age > self.slots[b].age) {
                best = Some(i);
            }
            if s.key < 0 || (self.slots[victim].key >= 0 && s.age < self.slots[victim].age) {
                victim = i;
            }
        }
        let at = match best {
            None => {
                self.slots[victim] = DefeatedSlot::cleared();
                self.slots[victim].key = key;
                victim
            }
            Some(b) => {
                let top = self.slots[b].age;
                for s in self.slots.iter_mut().filter(|s| s.age < top) {
                    s.age += 1;
                }
                b
            }
        };
        self.slots[at].age = self.counter;
        self.counter += 1;
        self.chosen = at;
    }

    /// The serials entered for the area now running.
    pub fn serials(&self) -> &[u16] {
        &self.slots[self.chosen].serials
    }

    /// Makes the running area's entries among `members` equal `killed` (`FUN_00198220` enters a created enemy,
    /// `FUN_001993f8` / `FUN_00199858` take one out; the table holds 256 entries, and a full table takes no more).
    pub fn sync(&mut self, members: &[u16], killed: &[u16]) {
        let t = &mut self.slots[self.chosen].serials;
        t.retain(|s| !members.contains(s) || killed.contains(s));
        for s in killed {
            if !t.contains(s) && t.len() < 256 {
                t.push(*s);
            }
        }
    }
}

impl KindTwo {
    pub fn new(g: &AreaGroup, part: usize) -> KindTwo {
        KindTwo { part, timer: g.parts[part].start_delay() as f32, wave: 0, killed: Vec::new() }
    }

    /// `FUN_001993f8`: member `m` of the group is gone (here: its HP reached 0). The timer restarts at header +0x20;
    /// a member of a wave up to 99 stays defeated (`FUN_00199378`).
    pub fn removed(&mut self, g: &AreaGroup, m: &Member) {
        self.timer = g.parts[self.part].removal_delay() as f32;
        if m.wave <= 99 && !self.killed.contains(&m.serial) {
            self.killed.push(m.serial);
        }
    }

    /// `FUN_001991a0`, run on a frame the player is inside one of the group's volumes (`FUN_001989d8` ->
    /// `FUN_00198b30`). Returns the members (indices into `g.members`) to create now; `alive(i)` says whether
    /// member `i`'s object exists (`FUN_0016b0d8` by serial). Walks the entities in order: once the timer runs it
    /// stops; an absent member of spawn kind 2 whose wave byte is at most the group's wave is created and sets the
    /// timer to its delay (+0x2a); a member defeated for good (`killed`) does not come back. When the walk ends with the timer at 0, `FUN_00199618` moves to the next wave
    /// that has members, if the `SpawnLimiter` sum of the members in play (`FUN_001996f0`) is at most header +0x22;
    /// header +0x23 (not 0) is the wave at which it goes back to wave +0x24 (`FUN_001997f8`, `FUN_00199858`).
    pub fn run(&mut self, g: &AreaGroup, alive: impl Fn(usize) -> bool) -> Vec<usize> {
        let part = &g.parts[self.part];
        let mut out = Vec::new();
        // The game repeats until no wave is left to move to; the guard only bounds a cycle of waves.
        for _ in 0..256 {
            for (i, m) in g.members.iter().enumerate().filter(|(_, m)| m.part == self.part) {
                if self.timer > 0.0 {
                    return out;
                }
                if m.spawn_kind == 2 && m.wave <= self.wave && !alive(i) && !self.killed.contains(&m.serial) && !out.contains(&i) {
                    out.push(i);
                    self.timer = m.delay as f32;
                }
            }
            if self.timer > 0.0 {
                return out;
            }
            // FUN_001996f0: the SpawnLimiter bytes of the members of spawn kinds 0 and 3 and of kind 2 of the waves so
            // far, except those defeated for good (FUN_001995c8: in the table and absent). Only the members the app
            // has are summed [inferred stand-in: the game also counts the entities the app cannot create, which
            // would hold the waves back here for ever; since WP-EN2 b_44's M_EX940 are not among them].
            let sum: u32 = g
                .members
                .iter()
                .enumerate()
                .filter(|(_, m)| m.part == self.part && (matches!(m.spawn_kind, 0 | 3) || (m.spawn_kind == 2 && m.wave <= self.wave)))
                .filter(|(i, m)| !(self.killed.contains(&m.serial) && !alive(*i)))
                .map(|(_, m)| m.limiter as u32)
                .sum();
            if self.wave >= 200 || sum > part.header[0x22] as u32 {
                return out;
            }
            let mut steps = 0;
            let next = loop {
                steps += 1;
                if steps > 256 {
                    break false;
                }
                // FUN_001997f8: the wave counts up only while it is below 200 (it stays at 200 after that).
                let counted = self.wave < 200;
                if counted {
                    self.wave += 1;
                }
                let back_at = part.header[0x23];
                if counted && back_at != 0 && back_at <= self.wave {
                    self.wave = part.header[0x24];
                    // FUN_00199858: before the wave is set, each spawn-kind-2 entity whose wave byte is not below the new
                    // wave and that FUN_001995c8 finds defeated (in the table, object absent) is taken out of the table
                    // (FUN_001942c8), so it comes back in the next lap.
                    let wave = self.wave;
                    let part_ix = self.part;
                    let gone: Vec<u16> = g
                        .members
                        .iter()
                        .enumerate()
                        .filter(|(i, m)| m.part == part_ix && m.spawn_kind == 2 && m.wave >= wave && self.killed.contains(&m.serial) && !alive(*i))
                        .map(|(_, m)| m.serial)
                        .collect();
                    self.killed.retain(|s| !gone.contains(s));
                }
                // FUN_00199698
                if part.entities.iter().any(|e| e.0 == 2 && e.1 == self.wave) {
                    break true;
                }
                if self.wave >= 200 {
                    break false;
                }
            };
            if !next {
                return out;
            }
        }
        out
    }
}

/// The game's random generator `DAT_003716a0` as the placement code reads it (`FUN_001a6068`): `seed * 0x10dcd`
/// (32 bits, no increment), taken unsigned and scaled by 2^-32; and the counter `DAT_0035e9b0` that picks the
/// octant the next search starts in. The game shares one generator among all its callers and the app keeps one for
/// the placements [inferred: nothing else here depends on the sequence]; the game's seed is not known (the executable
/// holds 0), the start value is the app's.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaceRng {
    seed: u32,
    sector: u32,
}

impl Default for PlaceRng {
    fn default() -> PlaceRng {
        PlaceRng { seed: 0x2545_f491, sector: 0 }
    }
}

impl PlaceRng {
    /// One step: a value in [0, 1).
    pub fn next(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(0x10dcd);
        self.seed as f32 * (1.0 / 4_294_967_296.0)
    }

    /// `FUN_001a6068`'s octant number: `(c & 4) >> 2 | c << 1` for the counter `c`, which then counts 0..7 round.
    fn octant(&mut self) -> u32 {
        let c = self.sector;
        self.sector = (c + 1) % 8;
        (c & 4) >> 2 | c << 1
    }
}

/// A ring search (`FUN_001a6068` sets it up; `FUN_001a6198` on the floor, `FUN_001a63c8` in the air): metres, the
/// app's space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub centre: Vec3,
    /// The smallest and largest radius; the search starts at a random one in between.
    pub min: f32,
    pub max: f32,
    /// In the air: the offset range of the height (the game's `FUN_001a63c8` args); `None` on the floor.
    pub heights: Option<(f32, f32)>,
}

/// The ring search of `FUN_001a6198` / `FUN_001a63c8` (the part of `FUN_001a5ea8` / `FUN_001a5ee0` after the set-up).
/// A radius `r` is drawn between the ring's limits and an octant, then points are tried at `r` round the centre in
/// steps of 45 degrees (the angle of try `n` is the start plus `n` steps; `x = cos`, game `z = sin`, so the app's
/// `z = -sin`): on the floor, `ground(point)` is the floor found from 2 m above the point down to 12 m below
/// (`FUN_001a5c30`, 200 and 1000 cm) and the point is taken when the line from the centre to a spot 20 cm further
/// out and 1 m above that floor is `clear` (`FUN_001a5bf8`); in the air, a random height in `heights` is added to
/// the point and the same line test (to 1 m above the point) decides. After 8 tries the radius shrinks by 1 m, until
/// it falls below the minimum. Nothing found: the floor under the centre (`ground`, else the centre) or, in the air,
/// the centre. `ground` and `clear` are the collision queries.
pub fn ring_place(rng: &mut PlaceRng, ring: &Ring, ground: &dyn Fn(Vec3) -> Option<Vec3>, clear: &dyn Fn(Vec3, Vec3) -> bool) -> Vec3 {
    const STEP: f32 = std::f32::consts::FRAC_PI_4;
    let mut r = ring.min + rng.next() * (ring.max - ring.min);
    let start = rng.octant() as f32 * STEP + rng.next() * STEP;
    let mut tries = 0;
    while r >= ring.min {
        tries += 1;
        let a = start + tries as f32 * STEP;
        let dir = Vec3::new(a.cos(), 0.0, -a.sin());
        let spot = ring.centre + dir * r;
        let mut beyond = ring.centre + dir * (r + 0.2);
        match ring.heights {
            None => {
                if let Some(hit) = ground(spot) {
                    beyond.y = hit.y + 1.0;
                    if clear(ring.centre, beyond) {
                        return hit;
                    }
                }
            }
            Some((lo, hi)) => {
                let h = lo + rng.next() * (hi - lo);
                let at = spot + Vec3::Y * h;
                beyond.y = at.y + 1.0;
                if clear(ring.centre, beyond) {
                    return at;
                }
            }
        }
        if tries >= 8 {
            tries = 0;
            r -= 1.0;
        }
    }
    match ring.heights {
        None => ground(ring.centre).unwrap_or(ring.centre),
        Some(_) => ring.centre,
    }
}

/// Where a kind-2 member is created (`FUN_001991a0`, by the record byte +0x1d): 1 near the player
/// (`FUN_00199970`: a ring of 1.5 to 4.5 m round the player's position raised by 2 m, in the air 1 m up or down),
/// 2 within the record's radius of its point (`FUN_00199a80`: 0 to the radius, in the air at the point's height);
/// anything else the record's point (`FUN_001982c8`), which is `None` here. `player` is the player's position
/// (`FUN_0016b960` of `DAT_0035c998` [inferred: the object's position]).
pub fn member_ring(m: &Member, player: Vec3) -> Option<Ring> {
    match m.spawn_arg {
        1 => Some(Ring { centre: player + Vec3::Y * 2.0, min: 1.5, max: 4.5, heights: m.flies.then_some((-1.0, 1.0)) }),
        2 => Some(Ring { centre: m.at, min: 0.0, max: m.radius as f32 * SCALE, heights: m.flies.then_some((0.0, 0.0)) }),
        _ => None,
    }
}

/// The ring search on the loaded area's collision (`World`): the floor probe is `World::ground_m` from 2 m above the
/// point (at most 12 m below), the line test `World::blocked_m` [inferred approximation: the game's line test
/// (`FUN_001d1648`) meets floors too; `blocked_m` only walls].
pub fn ring_place_in(rng: &mut PlaceRng, ring: &Ring, col: &World) -> Vec3 {
    let ground = |p: Vec3| floor_probe(p, |x, z, from| col.ground_m(x, z, from));
    let clear = |a: Vec3, b: Vec3| !col.blocked_m(a, b);
    ring_place(rng, ring, &ground, &clear)
}

/// The floor probe of `FUN_001a5c30`: the floor found from 2 m above `p` (`floor(x, z, from_y)` is the collision query),
/// taken only when it is at most 12 m below `p` (the ray's 200 and 1000 cm).
fn floor_probe(p: Vec3, floor: impl Fn(f32, f32, f32) -> Option<f32>) -> Option<Vec3> {
    floor(p.x, p.z, p.y + 2.0).filter(|y| *y >= p.y - 12.0).map(|y| p.with_y(y))
}

/// `FUN_00199e98` for a kind-6 group record (`FUN_00198b30` case 6), run on a frame the player is inside one of its
/// volumes whose activator halfword +0x2c is 1 (`FUN_001989d8` skips the others; `inside(volume)` is the volume
/// test): every spawn-kind-2 entity whose object is absent (`present(serial)`, `FUN_0016b0d8`) is created at once at
/// its record's point (`FUN_001982c8`), with no timer, no wave and no table of the defeated. Returns the members
/// (indices into `g.members`). [gap: the appear fade the game starts on the new object, `FUN_001abf40` on its
/// +0x82c, is not ported.]
pub fn kind_six_creates(g: &AreaGroup, part: usize, inside: impl Fn(&kh2_ard::Activator) -> bool, present: impl Fn(u16) -> bool) -> Vec<usize> {
    let p = &g.parts[part];
    if p.kind != 6 || p.disabled || !p.activators.iter().any(|v| u16::from_le_bytes([v.raw[0x2c], v.raw[0x2d]]) == 1 && inside(v)) {
        return Vec::new();
    }
    g.members.iter().enumerate().filter(|(_, m)| m.part == part && m.spawn_kind == 2 && !present(m.serial)).map(|(i, _)| i).collect()
}

/// A group record's counter of enemies (the group object's +0x18) and the group's id (header +2).
#[derive(Clone, Debug, PartialEq)]
pub struct PartCount {
    pub id: u16,
    pub count: i32,
    pub disabled: bool,
}

/// What `FUN_00198750` counts for a group record when it is set up: every enemy-class entity (`FUN_001994c8`: object
/// types 4 and 0x15) that is not in the table of the defeated (`killed`): the spawn-kind-2 ones always (they are only
/// counted), kinds 0 and 3 when the group is not disabled (`FUN_00198920`), kind 1 when its entrance is the current
/// one (`FUN_001981a0`). A kind-9 group sets up nothing. Not ported: the two filters that skip an entity in
/// `FUN_00198750` (`FUN_001981a0` fails when `DAT_0035e3a0` is set and `FUN_0016ece0(type) >= 2000`; an object of type 0x10
/// with `FUN_002d6d28 == 0` is dropped); the Twilight Town data has neither, and no enemy-class entity is type 0x10.
pub fn counted_at_load(part: &GroupPart, entrance: u8, killed: &[u16]) -> i32 {
    if part.kind == 9 {
        return 0;
    }
    part.counted
        .iter()
        .filter(|c| c.enemy && !killed.contains(&c.serial))
        .filter(|c| match c.spawn_kind {
            2 => true,
            0 | 3 => !part.disabled,
            1 => !part.disabled && c.arg == entrance,
            _ => false,
        })
        .count() as i32
}

/// `FUN_00199518`, run when an object of spawn kind 0, 1 or 3 of group `i` is removed (`FUN_001993f8`; the object's
/// +0x588 bit 0x800 it also tests is set by the enemy classes' constructor `FUN_001b0270`, so it holds for object
/// types 4 and 0x15): the group's counter drops by 1 and, at 0, global event 3 with the group's id is raised; then,
/// when the counters of all groups are 0, event 2 with 0 (`FUN_00199580` sums them). Spawn-kind-2 members never
/// lower the counter. Returns the events in order.
pub fn removal_events(counts: &mut [PartCount], i: usize) -> Vec<(u32, i32)> {
    let mut out = Vec::new();
    if counts[i].disabled {
        return out;
    }
    counts[i].count -= 1;
    if counts[i].count == 0 {
        out.push((3, counts[i].id as i32));
    }
    if counts.iter().map(|c| c.count).sum::<i32>() == 0 {
        out.push((2, 0));
    }
    out
}

/// A group record's members the app tracks for its events: the serials of the spawn-kind 0, 1 and 3 members that
/// were alive last tick.
#[derive(Clone)]
pub struct EventPart {
    pub group: AreaGroup,
    pub part: usize,
    pub present: Vec<u16>,
}

/// The kind-2 groups of the loaded area and what they need between frames.
#[derive(Resource, Default)]
pub struct BattleGroups {
    /// The area run (`AreaRecord::run`, by address) the groups were set up for.
    run: usize,
    pub live: Vec<(AreaGroup, KindTwo, Vec<u16>)>,
    /// Every group record of the set-up groups (the events and kind-6 groups); `counts` is parallel to it.
    pub events: Vec<EventPart>,
    pub counts: Vec<PartCount>,
    /// The tables of the defeated that outlive the area (`DefeatedTables`).
    pub defeated: DefeatedTables,
    rng: PlaceRng,
}

/// What the reaction code tracks: a hit reaction in progress, or nothing (the script is in charge).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EState {
    Idle,
    Hurt,
    Blown,
}

#[derive(Component)]
pub struct Enemy {
    pub kind: &'static Kind,
    pub name: &'static str,
    pub state: EState,
    pub hp: i32,
    pub stats: Stats,
    pub vel: Vec3,
    pub grounded: bool,
    pub yaw: f32,
    pub timer: f32,
    pub body: Entity,
    /// Decaying push of a push reaction, added to the movement every tick.
    push: Option<Push>,
    /// Flight of the blow that put the enemy in `EState::Blown`, and whether it came from behind.
    blow: Option<Blow>,
    blown_back: bool,
    /// Reaction class 5 (Magnet): the spell attack object the enemy is drawn toward (its group in
    /// `magic::SpellAttacks`, actor +0xaac) and the pull's numbers (`reaction_rules::Pull`).
    pull: Option<(u32, rules::Pull)>,
    /// Hit sounds played so far; the sound alternates between two entries (actor byte +0x7fc).
    hit_sounds: u8,
    /// The spawn kind byte of the enemy's spawn record (host 1.110), -1 without one.
    pub spawn_kind: i32,
    /// Spawn parameters 1 and 2 from the record (host 1.39), `None` without a record.
    pub spawn_params: Option<[i32; 2]>,
    /// The lowest HP a hit can leave (word 2 of the HP record of the status block, which the game's
    /// HP change FUN_001a0100 clamps to; scripts set it with host 1.231). 0 unless a script sets it.
    pub min_hp: i32,
    /// The `00objentry` id of the spawn record (`Member::object_id`); 0 for the harness's own waves.
    pub object_id: u32,
    /// The group record id and the record's serial (+0x1e) of an enemy from an area group, `None` for the
    /// app's own waves.
    pub serial: Option<(u16, u16)>,
}

impl Enemy {
    pub fn alive(&self) -> bool {
        self.hp > 0
    }
}

/// The executable's numbers of the class 5 pull (`reaction_rules::PullConsts`).
#[derive(Resource)]
pub struct PullData(pub rules::PullConsts);

/// The game's shared random generator (`DAT_003716a0`, `x * 0x10dcd`) as the pull draws from it
/// (FUN_001bf920). The game shares one state with every other user; the app keeps this one for the
/// pull, started at 1 like the game's (FUN_002f5fc8) [the sequence differs from the game's].
#[derive(Default)]
pub struct PullRng(u32);

#[derive(Resource, Default)]
pub struct Waves {
    pub timer: f32,
    pub count: u32,
}

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        use crate::schedule::Sim;
        let rigs = {
            let (disc, shared) = (app.world().resource::<Disc>(), app.world().resource::<Rigs>());
            let mut map = HashMap::new();
            map.insert(SHADOW.name, shared.shadow.clone());
            map.insert(SOLDIER.name, shared.soldier.clone());
            for kind in KINDS.iter().filter(|k| ![SHADOW.name, SOLDIER.name].contains(&k.name)) {
                match load_rig(disc, kind.model) {
                    Some(r) => {
                        map.insert(kind.name, r);
                    }
                    None => warn!("{}: obj/{}.mdlx or .mset not loadable; no {} spawns", kind.name, kind.model, kind.name),
                }
            }
            EnemyRigs(map)
        };
        // The numbers of reaction class 5 (Magnet's pull), from the user's executable.
        match app.world().resource::<Disc>().read_root("SLUS_210.05").and_then(|e| rules::PullConsts::from_exe(&e)) {
            Some(p) => {
                app.insert_resource(PullData(p));
            }
            None => warn!("enemy: SLUS_210.05 not readable; reaction class 5 (pull) is not played"),
        }
        // The appear ramp of the objects a kind-6 group makes (FUN_00199e98, FUN_001abf40 / FUN_001abf78).
        match app.world().resource::<Disc>().read_root("SLUS_210.05").and_then(|e| crate::enemy_obj::AppearRamp::from_exe(&e)) {
            Some(a) => {
                app.insert_resource(a);
            }
            None => warn!("enemy: SLUS_210.05 not readable; the objects of kind-6 groups do not fade in"),
        }
        app.insert_resource(Waves::default())
            .insert_resource(rigs)
            .init_resource::<AreaWaves>()
            .init_resource::<BattleGroups>()
            .add_message::<EnemyClash>()
            .add_systems(Startup, register_effects.after(crate::fx::setup))
            .add_systems(
                FixedUpdate,
                (
                    (spawn_waves, think, separate).chain().in_set(Sim::Enemy),
                    (receive_clashes, receive_hits).chain().in_set(Sim::EnemyHurt),
                ),
            );
    }
}

/// The assets and the area a wave is made from.
type WaveSource<'a> = (Res<'a, Disc>, Res<'a, CurrentArea>, ResMut<'a, AreaWaves>, Res<'a, EnemyRigs>, Res<'a, crate::area::AreaRecord>, ResMut<'a, BattleGroups>, Res<'a, crate::area_jump::JumpTask>);

/// What a spawn record gives an enemy: spawn kind, spawn parameters, `00objentry` id, (group id, serial).
type SpawnRecord = (i32, Option<[i32; 2]>, u32, Option<(u16, u16)>);

/// Spawns one enemy: its model under a new root at `at` (metres), stats from its `enmp` row.
#[allow(clippy::too_many_arguments)]
fn spawn_enemy(
    commands: &mut Commands,
    kind: &'static Kind,
    rig: Arc<Rig>,
    at: Vec3,
    yaw: f32,
    (spawn_kind, spawn_params, object_id, serial): SpawnRecord,
    data: &GameData,
    assets: (&mut Assets<Mesh>, &mut Assets<StandardMaterial>, &mut Assets<Image>, &mut Assets<SkinnedMeshInverseBindposes>),
) -> Entity {
    let stats = data.enemy_stats(kind.enmp, 1);
    let root = commands.spawn((Transform::from_translation(at), Visibility::default())).id();
    let (meshes, materials, images, bindposes) = assets;
    let body = spawn_skinned(commands, root, rig, meshes, materials, images, bindposes, SCALE);
    commands.entity(root).insert((
        Enemy {
            kind,
            name: kind.name,
            state: EState::Idle,
            hp: stats.max_hp,
            stats,
            vel: Vec3::ZERO,
            grounded: true,
            yaw,
            timer: 0.0,
            body,
            push: None,
            blow: None,
            blown_back: false,
            pull: None,
            hit_sounds: 0,
            spawn_kind,
            spawn_params,
            min_hp: 0,
            object_id,
            serial,
        },
        crate::area_jump::AreaActor,
        Fighter { team: Team::Enemy, attack_bodies: vec![body], hurt_body: body, landed: Default::default(), hittable: false },
    ));
    root
}

/// Makes the objects host 1.0 asked for (`ObjTable::creates`, filled by `enemy_ai::ai_update`): the object of the
/// `00objentry` id, with the script of its kind, at the origin the game's place record gives (FUN_0016e8c0 gets 0x1c883a0)
/// or at the point the script's `set_position` (1.148) gave it in the same tick; facing 0 in the game's space. Its
/// brain takes the handle the call returned (`CreatedHandle`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_created(
    mut commands: Commands,
    mut table: ResMut<crate::enemy_obj::ObjTable>,
    rigs: Res<EnemyRigs>,
    data: Res<GameData>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    use crate::enemy_obj::Remote;
    for (handle, object) in std::mem::take(&mut table.creates) {
        let Some(kind) = table.kinds.get(&object).copied() else { continue };
        let Some(rig) = rigs.get(kind) else {
            warn!("create_object: {} has no rig; {handle:#x} is not made", kind.name);
            continue;
        };
        // The point the script gave it in this tick (the last `set_position`): the call is consumed here.
        let mut at = Vec3::ZERO;
        table.remote.retain(|(h, op, _)| match op {
            Remote::Position(p) if *h == handle => {
                at = *p * SCALE;
                false
            }
            _ => true,
        });
        info!("create_object: {} (objentry {object:#x}) made as {handle:#x} at {at:?}", kind.name);
        let ent = spawn_enemy(&mut commands, kind, rig, at, std::f32::consts::PI, (-1, None, object, None), &data, (&mut meshes, &mut materials, &mut images, &mut bindposes));
        commands.entity(ent).insert(crate::enemy_obj::CreatedHandle(handle));
    }
}

fn has(rig: &Rig, s: usize) -> Option<usize> {
    rig.mset.has(s).then_some(s)
}

/// The floor under a member's record point (the record's height is the game's own floor).
fn record_spot(m: &Member, col: &World) -> Vec3 {
    let y = col.ground_m(m.at.x, m.at.z, m.at.y + 1.0).unwrap_or(m.at.y);
    m.at.with_y(y)
}

/// Creates one member of an area group at `at` (metres): the record's point for the members created at load, a
/// placement (`member_ring`) for the kind-2 members that ask for one.
#[allow(clippy::too_many_arguments)]
fn spawn_member(
    commands: &mut Commands,
    m: &Member,
    part_id: u16,
    at: Vec3,
    rigs: &EnemyRigs,
    data: &GameData,
    assets: (&mut Assets<Mesh>, &mut Assets<StandardMaterial>, &mut Assets<Image>, &mut Assets<SkinnedMeshInverseBindposes>),
) -> Option<Entity> {
    let rig = rigs.get(m.kind)?;
    Some(spawn_enemy(commands, m.kind, rig, at, m.yaw, (m.spawn_kind, Some(m.spawn_params), m.object_id, Some((part_id, m.serial))), data, assets))
}

/// The area's enemies (module header). When an area run starts (`AreaRecord::run`: every area load, and a
/// type-2 jump into the loaded area), the groups its `btl` program loaded are set up: the members
/// `created_at_load` appear at once, and each kind-2 group keeps a `KindTwo` that creates its spawn-kind-2
/// members while Sora is inside one of its volumes (`FUN_001989d8` -> `FUN_00198b30` -> `FUN_001991a0`); when
/// one of its enemies is gone its timer restarts at header +0x20 (`FUN_001993f8`). F3 sets the groups up again.
/// `KH2_WAVE_GROUP=<tag>` (harness) sets up that group instead of the program's. In an area whose `btl` script
/// loads no group with enemies of `KINDS` (tt00, tt02) and with `KH2_AREA_WAVES=0`: the app's own waves, one 4 s
/// after the last enemy is gone or on F3 (not from the disc).
#[allow(clippy::too_many_arguments)]
pub fn spawn_waves(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut waves: ResMut<Waves>,
    (disc, area, mut area_waves, rigs, rec, mut battle, task): WaveSource,
    data: Res<GameData>,
    collider: Option<Res<World>>,
    mut sora: Query<(&mut Transform, &mut Sora)>,
    enemies: Query<&Enemy>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut missions: MessageWriter<crate::struggle::MissionStart>,
    mut events: MessageWriter<crate::struggle::GlobalEvent>,
    mission: Option<Res<crate::struggle::Mission>>,
) {
    let (Some(col), Ok((mut sora_tf, mut sora))) = (collider, sora.single_mut()) else { return };
    let p = *sora_tf;
    if std::env::var("KH2_NO_ENEMIES").is_ok() {
        return;
    }
    // The area's own enemy groups, read once per loaded area.
    if area_waves.area != area.loaded {
        area_waves.area = area.loaded.clone();
        area_waves.groups = area.loaded.as_deref().map(|a| area_groups(&disc, a)).unwrap_or_default();
        for g in &area_waves.groups {
            info!(
                "area groups: {} btl program {:#x} group {}: {} (not in the app: {:?})",
                area.loaded.as_deref().unwrap_or("-"),
                g.program,
                g.name,
                g.members.iter().map(|m| m.kind.name).collect::<Vec<_>>().join(", "),
                g.skipped
            );
        }
    }
    let area_on = !std::env::var("KH2_AREA_WAVES").is_ok_and(|v| v == "0");
    let forced = std::env::var("KH2_WAVE_GROUP").ok();
    if area_on && !area_waves.groups.is_empty() {
        // The area's unload (`area_jump::unload_area_actors`) removes the groups with their enemies; the next run
        // of the scripts sets them up again, also when a jump re-enters the loaded area.
        if matches!(*task, crate::area_jump::JumpTask::Loading { .. }) {
            battle.run = 0;
            battle.live.clear();
            battle.events.clear();
            battle.counts.clear();
            return;
        }
        // The run of the loaded area (`area::run_area_script`, the port of FUN_00181148).
        let Some(run) = rec.run.clone().filter(|r| area.loaded.as_deref() == Some(r.name.as_str())) else { return };
        let id = Arc::as_ptr(&run) as usize;
        if battle.run != id || keys.just_pressed(KeyCode::F3) {
            // F3 sets the groups up again from scratch (a harness key): the tables of the defeated start empty too.
            if keys.just_pressed(KeyCode::F3) {
                battle.defeated.clear();
            }
            // FUN_00181148 -> FUN_00194188(0x35d278, DAT_0035cd28): the area byte selects the table; the keep count is
            // opcode 6 of the area scripts (FUN_00180b48 case 6, default 3), 0 when the map program byte is above 0x32.
            let world = area.loaded.as_deref().and_then(|n| n.as_bytes().get(..2)).map(|w| [w[0], w[1]]).unwrap_or([0, 0]);
            battle.defeated.enter_world(world);
            let area_byte = area.loaded.as_deref().and_then(|n| n.get(2..)).and_then(|n| n.parse::<u8>().ok()).unwrap_or(0xff);
            battle.defeated.enter(area_byte, kh2_ard::run::keep_for_program(&run.script, rec.programs[0]));
            battle.run = id;
            // FUN_00168ba8 -> FUN_001a5030: the battle set-up empties the keyed buffers of host 2.21.
            crate::enemy_host::clear_shared_blocks();
            battle.live.clear();
            battle.events.clear();
            battle.counts.clear();
            // Harness: `KH2_WAVE_GROUP=<tag>` sets up that group instead of the program's and, when the group holds
            // the player's entity, puts Sora there first (the record's place for the player; that the game moves
            // the player there when such a battle starts is [inferred]).
            let chosen: Vec<AreaGroup> = match forced.as_deref() {
                Some(f) => area_waves.groups.iter().find(|g| g.name == f).cloned().into_iter().collect(),
                None => btl_groups(&area_waves.groups, &run.script).into_iter().cloned().collect(),
            };
            if chosen.is_empty() {
                info!("area {}: btl program {:#x} loads no group with enemies of the app (FUN_00181148); no enemies", run.name, rec.programs[1]);
            }
            for g in &chosen {
                // Harness: `KH2_WAVE_AT_GROUP=1` puts Sora at the centre of a forced group that has no player entry
                // (b_00, b_21: the group stands about 22 m from his start point). A test aid, not the game's rule.
                let centre = (forced.is_some() && g.player.is_none() && std::env::var("KH2_WAVE_AT_GROUP").is_ok_and(|v| v == "1"))
                    .then(|| g.members.iter().map(|m| m.at).sum::<Vec3>() / g.members.len() as f32);
                if let (Some(_), Some(at)) = (&forced, g.player.or(centre)) {
                    let y = col.ground_m(at.x, at.z, at.y + 1.0).unwrap_or(at.y);
                    crate::sora::respawn(&mut sora, at.with_y(y) / SCALE);
                    sora_tf.translation = at.with_y(y);
                    info!("area group {}: Sora placed at the group's player entry {:?}", g.name, sora_tf.translation);
                }
                let mut now = 0;
                for m in g.members.iter().filter(|m| created_at_load(m, &g.parts[m.part], rec.entrance)) {
                    spawn_member(&mut commands, m, g.parts[m.part].id, record_spot(m, &col), &rigs, &data, (&mut meshes, &mut materials, &mut images, &mut bindposes));
                    now += 1;
                }
                let twos: Vec<usize> = (0..g.parts.len()).filter(|&k| g.parts[k].kind == 2 && !g.parts[k].disabled && g.members.iter().any(|m| m.part == k && m.spawn_kind == 2)).collect();
                info!(
                    "area {}: btl program {:#x} group {}{}: {now} enemies at load (FUN_00198750), {} on demand by kind-2 group {:x?}",
                    run.name,
                    g.program,
                    g.name,
                    if forced.is_some() { " (KH2_WAVE_GROUP)" } else { "" },
                    g.members.iter().filter(|m| m.spawn_kind == 2 && twos.contains(&m.part)).count(),
                    twos.iter().map(|&k| g.parts[k].id).collect::<Vec<_>>()
                );
                for k in twos {
                    let mut two = KindTwo::new(g, k);
                    two.killed = battle.defeated.serials().iter().copied().filter(|s| g.members.iter().any(|m| m.part == k && m.serial == *s)).collect();
                    info!("area group {} kind-2 group {:#x}: timer {} frames (header +0x1c), {} after a removal (+0x20), limit {} (+0x22)", g.name, g.parts[k].id, two.timer, g.parts[k].removal_delay(), g.parts[k].header[0x22]);
                    // The members created at load are there (FUN_00198750 enters them as it creates them).
                    let there = g.members.iter().filter(|m| m.part == k && created_at_load(m, &g.parts[k], rec.entrance)).map(|m| m.serial).collect();
                    battle.live.push((g.clone(), two, there));
                }
                // The group records' counters of enemies (FUN_00198750 -> FUN_001994c8) for the group events.
                for (k, part) in g.parts.iter().enumerate() {
                    let count = counted_at_load(part, rec.entrance, &[]);
                    if count > 0 {
                        info!("area group {} group record {:#x} (kind {}): {count} enemies counted (FUN_001994c8)", g.name, part.id, part.kind);
                    }
                    battle.counts.push(PartCount { id: part.id, count, disabled: part.disabled });
                    battle.events.push(EventPart { group: g.clone(), part: k, present: Vec::new() });
                }
                // The program's mission command (opcode 0x15) creates the mission manager (WP-7E, struggle.rs).
                if let Some((id, name)) = &g.mission {
                    missions.write(crate::struggle::MissionStart { id: *id, name: name.clone(), orb_pool: g.orb_pool, triggers: g.triggers.clone() });
                }
            }
            // The members created here are entities from the next tick on: the groups' walk starts then.
            return;
        }
        // The kind-2 groups, every tick (FUN_001989d8: the timer runs down by the frame step whether or not the
        // player is inside; `DAT_00356f40` [inferred: one 60 Hz frame per frame]).
        let frames = 60.0 / LOGIC_HZ;
        let pos = sora.motor.pos;
        let at = [pos.x, -pos.y, -pos.z];
        let player = sora_tf.translation;
        let b = &mut *battle;
        // FUN_00190668: no group creates while an event plays (0x35e3a4) or the fight is stopped (0x35d268 bit 2). Not
        // modelled: the latch at 0x35fec8 (private to the mission) and bit 0x100 of 0x35d268 (no writer found).
        let blocked = mission.as_deref().is_some_and(|m| m.event_active() || m.fight_stopped());
        for (g, two, prev) in b.live.iter_mut() {
            let part = &g.parts[two.part];
            let pid = part.id;
            let live: Vec<u16> = enemies.iter().filter(|e| e.alive()).filter_map(|e| e.serial.filter(|s| s.0 == pid).map(|s| s.1)).collect();
            // FUN_001993f8: a member that was there and is gone.
            let tp = two.part;
            for m in g.members.iter().filter(|m| m.part == tp && prev.contains(&m.serial) && !live.contains(&m.serial)) {
                two.removed(g, m);
                info!("area group {} kind-2 group {pid:#x}: {} (serial {:#x}, wave {}) gone; timer {} frames{}", g.name, m.kind.name, m.serial, m.wave, two.timer, if two.killed.contains(&m.serial) { ", defeated for good" } else { "" });
            }
            let part = &g.parts[two.part];
            let members: Vec<u16> = g.members.iter().filter(|m| m.part == tp).map(|m| m.serial).collect();
            // FUN_001989d8: while FUN_00190668 holds, the timer stands and nothing is created.
            if !blocked && two.timer > 0.0 {
                two.timer -= frames;
            }
            let mut made = Vec::new();
            if !blocked && part.activators.iter().any(|v| v.contains(at)) {
                let alive = |i: usize| enemies.iter().any(|e| e.alive() && e.serial == Some((pid, g.members[i].serial)));
                for i in two.run(g, alive) {
                    let m = &g.members[i];
                    let (spot, how) = match member_ring(m, player) {
                        Some(ring) => (ring_place_in(&mut b.rng, &ring, &col), if m.spawn_arg == 1 { "near the player (FUN_00199970)" } else { "within its radius (FUN_00199a80)" }),
                        None => (record_spot(m, &col), "at the record's point"),
                    };
                    info!(
                        "area group {} kind-2 group {pid:#x}: {} (serial {:#x}, wave {}) created {how} at {:.2?}; next in {} frames, group wave {}",
                        g.name, m.kind.name, m.serial, m.wave, spot, m.delay, two.wave
                    );
                    spawn_member(&mut commands, m, pid, spot, &rigs, &data, (&mut meshes, &mut materials, &mut images, &mut bindposes));
                    made.push(m.serial);
                }
            }
            *prev = live.into_iter().chain(made).collect();
            b.defeated.sync(&members, &two.killed);
        }
        // Kind-6 groups (FUN_00199e98): see `kind_six_creates`. The object counts as present from its creation to its
        // removal, so a defeated enemy that is still lying there does not come back yet.
        for ep in &b.events {
            let pid = ep.group.parts[ep.part].id;
            for i in kind_six_creates(&ep.group, ep.part, |v| v.contains(at), |s| enemies.iter().any(|e| e.serial == Some((pid, s)))) {
                let m = &ep.group.members[i];
                info!("area group {} kind-6 group {pid:#x}: {} (serial {:#x}) created at the record's point (FUN_00199e98)", ep.group.name, m.kind.name, m.serial);
                // The object fades in: FUN_00199e98 starts the ramp on +0x82c unless +0x7f8 bit 0x10 is set (a new object's is clear).
                if let Some(ent) = spawn_member(&mut commands, m, pid, record_spot(m, &col), &rigs, &data, (&mut meshes, &mut materials, &mut images, &mut bindposes)) {
                    commands.entity(ent).insert(crate::enemy_obj::Appearing);
                }
            }
        }
        // Group events: an object of spawn kind 0, 1 or 3 removed (here: its HP reached 0) lowers its group record's
        // counter; 0 raises event 3 with the group's id, all counters 0 event 2 (FUN_001993f8 -> FUN_00199518).
        for i in 0..b.events.len() {
            let (pid, part_idx) = (b.events[i].group.parts[b.events[i].part].id, b.events[i].part);
            let now: Vec<u16> = enemies
                .iter()
                .filter(|e| e.alive())
                .filter_map(|e| e.serial.filter(|s| s.0 == pid).map(|s| s.1))
                .filter(|s| b.events[i].group.members.iter().any(|m| m.part == part_idx && m.serial == *s && matches!(m.spawn_kind, 0 | 1 | 3)))
                .collect();
            let gone: Vec<u16> = b.events[i].present.iter().copied().filter(|s| !now.contains(s)).collect();
            b.events[i].present = now;
            for serial in gone {
                for (id, arg) in removal_events(&mut b.counts, i) {
                    let hit = b.events[i].group.triggers.iter().find(|t| t.matches(id, arg)).copied();
                    info!(
                        "area group {} group record {pid:#x}: member serial {serial:#x} removed: global event {id} ({arg}) (FUN_00199518){}",
                        b.events[i].group.name,
                        hit.map_or(String::new(), |t| format!("; area trigger {t:?} matches (its effects are not applied here)"))
                    );
                    events.write(crate::struggle::GlobalEvent { id, arg });
                }
            }
        }
        return;
    }
    if enemies.iter().any(|e| e.alive()) && !keys.just_pressed(KeyCode::F3) {
        waves.timer = 0.0;
        return;
    }
    waves.timer += time.delta_secs();
    if waves.timer < 4.0 && !keys.just_pressed(KeyCode::F3) {
        return;
    }
    waves.timer = 0.0;
    waves.count += 1;
    let n = 3 + (waves.count % 3) as usize;
    // Candidate points around Sora with floor near his height and a clear path to him.
    let knee = Vec3::Y * 0.5;
    let spots: Vec<Vec3> = (0..24)
        .flat_map(|k| [4.5f32, 6.5].map(move |r| (k, r)))
        .filter_map(|(k, r)| {
            let ang = k as f32 / 24.0 * std::f32::consts::TAU + waves.count as f32 * 0.7;
            let (x, z) = (p.translation.x + ang.cos() * r, p.translation.z + ang.sin() * r);
            let y = col.ground_m(x, z, p.translation.y + 1.5)?;
            let at = Vec3::new(x, y, z);
            ((y - p.translation.y).abs() < 1.5 && !col.blocked_m(p.translation + knee, at + knee)).then_some(at)
        })
        .collect();
    let step = (spots.len() / n.max(1)).max(1);
    for (i, at) in spots.iter().step_by(step).take(n).enumerate() {
        let (x, y, z, i) = (at.x, at.y, at.z, i as u32);
        // Mostly Shadows, with a Soldier in every wave after the first.
        let kind: &'static Kind = if waves.count > 1 && i == 0 { &SOLDIER } else { &SHADOW };
        let Some(rig) = rigs.get(kind) else { continue };
        let yaw = { let d = p.translation - Vec3::new(x, y, z); f32::atan2(d.x, d.z) };
        spawn_enemy(&mut commands, kind, rig, Vec3::new(x, y, z), yaw, (-1, None, 0, None), &data, (&mut meshes, &mut materials, &mut images, &mut bindposes));
    }
}

/// What the class-5 pull reads: the executable's numbers and the attack objects of the spells and of
/// the party members (their places by group).
type PullSources<'w> = (Option<Res<'w, PullData>>, Option<Res<'w, crate::magic::SpellAttacks>>, Option<Res<'w, crate::friend_ai::MemberAttacks>>);

/// The hit reactions of the enemies (`Hurt`, `Blown`) and the movement they need. A scripted enemy
/// (`AiBrain`) moves and animates itself in `enemy_ai.rs` whenever no reaction plays.
#[allow(clippy::too_many_arguments)]
pub fn think(
    time: Res<Time>,
    data: Res<GameData>,
    collider: Option<Res<World>>,
    reaction: Option<Res<ReactionData>>,
    sora: Query<&Transform, (With<Sora>, Without<Enemy>)>,
    mut q: Query<(Entity, &mut Transform, &mut Enemy, Has<crate::enemy_ai::AiBrain>)>,
    mut anims: Query<&mut Animator>,
    (pull_data, spells, member_attacks): PullSources,
) {
    let (Some(col), Ok(_)) = (collider, sora.single()) else { return };
    let dt = time.delta_secs().min(0.05);
    for (me, mut tf, mut e, scripted) in &mut q {
        let e = &mut *e;
        if scripted && !matches!(e.state, EState::Hurt | EState::Blown) {
            continue;
        }
        let Ok(mut a) = anims.get_mut(e.body) else { continue };
        let rig = a.rig.clone();
        e.timer += dt;
        let mut gravity = GRAVITY;
        match e.state {
            EState::Idle => {
                e.vel = Vec3::new(0.0, e.vel.y, 0.0);
                a.play(has(&rig, IDLE), true, 0.2);
            }
            EState::Hurt if e.pull.is_some() => {
                // Reaction class 5: the pull mover (FUN_003332f0 -> FUN_001bfc10 for the velocity,
                // FUN_001bfba8 each frame: the facing again, or the end once the attack object is
                // finished, FUN_0018ccf8 clearing +0xaac). The pull's velocity is the whole movement,
                // height included [inferred: the blow mover's velocity method adds its own gravity
                // (FUN_0019e4d8), this one adds none].
                let (group, p) = e.pull.expect("pull");
                let at = spells.as_ref().and_then(|s| s.places.get(&group).copied()).or_else(|| member_attacks.as_ref().and_then(|m| m.places.get(&group).copied()));
                match (at, pull_data.as_ref()) {
                    (Some(at), Some(pd)) => {
                        let centre = tf.translation + spells.as_ref().and_then(|s| s.centres.get(&me).copied()).unwrap_or(Vec3::ZERO);
                        let v = p.velocity(at / SCALE, centre / SCALE, &pd.0);
                        e.vel = v * (SCALE * LOGIC_HZ);
                        gravity = 0.0;
                        let face = p.facing(at / SCALE, centre / SCALE, &pd.0).with_y(0.0);
                        if face != Vec3::ZERO {
                            e.yaw = face.x.atan2(face.z);
                        }
                        // `KH2_REACTIONLOG=1`: the pull every 10 ticks (distance to the attack, game units).
                        if std::env::var("KH2_REACTIONLOG").is_ok_and(|v| v != "0") && (e.timer * LOGIC_HZ).round() as i32 % 10 == 0 {
                            info!("{} pull: {:.0} units from attack {group} (hold {:.0}), velocity {:.1?} units/tick, at {:.2?}", e.name, (at - centre).length() / SCALE, p.hold, v, tf.translation);
                        }
                    }
                    _ => {
                        // FUN_001bfba8 with +0xaac cleared: the default mover again and motion 0 with
                        // the blend of 0x37bd28.
                        let blend = pull_data.as_ref().map_or(0.0, |pd| pd.0.end_blend);
                        info!("{} pull ends: attack {group} finished (FUN_001bfba8), motion 0 blend {blend} at {:?}", e.name, tf.translation);
                        e.pull = None;
                        e.state = EState::Idle;
                        e.timer = 0.0;
                        a.restart(has(&rig, IDLE), true, blend);
                    }
                }
            }
            EState::Hurt => {
                e.vel *= Vec3::new(0.85, 1.0, 0.85);
                if a.finished || e.timer > 1.2 {
                    e.state = EState::Idle;
                    e.timer = 0.0;
                }
            }
            EState::Blown => {
                // The flight of a blow (FUN_0019e4d8) is in units per tick; the mover takes m/s and
                // gets the vertical speed directly, so it adds no gravity of its own.
                if let (Some(b), Some(r)) = (e.blow.as_mut(), reaction.as_ref()) {
                    let (h, vy) = b.step(&r.defaults, &r.consts);
                    let k = SCALE * LOGIC_HZ;
                    e.vel = (b.dir * h * k).with_y(vy * k);
                    gravity = 0.0;
                }
                if e.grounded && e.timer > 0.25 {
                    info!("{} lands from a blow after {:.2} s at {:?}", e.name, e.timer, tf.translation);
                    e.vel = Vec3::ZERO;
                    e.blow = None;
                    e.state = EState::Hurt;
                    e.timer = 0.0;
                    a.restart(has(&rig, BLOW + if e.blown_back { 3 } else { 1 }).or(has(&rig, BLOW + 1)), false, 0.05);
                }
            }
        }
        // A push (hit reaction) moves the enemy on top of whatever else it does this tick.
        let mut push_v = Vec3::ZERO;
        if let (Some(p), Some(r)) = (e.push.as_mut(), reaction.as_ref()) {
            push_v = p.step(&r.consts) * (SCALE * LOGIC_HZ);
            if p.finished() {
                e.push = None;
            }
        }
        e.vel += push_v;
        let mut g = e.grounded;
        col.step_m(&mut tf.translation, &mut e.vel, dt, gravity, data.fall_max(), &mut g);
        e.grounded = g;
        e.vel -= push_v;
        tf.rotation = Quat::from_rotation_y(e.yaw);
    }
}

/// The direction body `i` is pushed from body `j` at horizontal offset `d` (length `l`). Two bodies on the same point have no
/// direction: each takes the opposite one by index, so they part (both taking +X moved them together for ever: M_EX850, made
/// on M_EX940's point, left the arena with it).
fn pair_dir(d: Vec3, l: f32, i: usize, j: usize) -> Vec3 {
    if l > 1e-4 {
        d / l
    } else if i < j {
        Vec3::X
    } else {
        -Vec3::X
    }
}

/// Keeps bodies from overlapping: enemies push apart from each other and from Sora.
pub fn separate(sora: Query<&Transform, (With<Sora>, Without<Enemy>)>, mut q: Query<(&mut Transform, &Enemy)>) {
    const ENEMY_R: f32 = 0.45;
    const SORA_R: f32 = 0.4;
    let sp = sora.single().map(|t| t.translation).ok();
    let pos: Vec<Vec3> = q.iter().map(|(t, _)| t.translation).collect();
    for (i, (mut t, e)) in q.iter_mut().enumerate() {
        if !e.alive() {
            continue;
        }
        let mut push = Vec3::ZERO;
        for (j, o) in pos.iter().enumerate() {
            let d = (t.translation - *o).with_y(0.0);
            let l = d.length();
            if i != j && l < 2.0 * ENEMY_R {
                push += pair_dir(d, l, i, j) * (2.0 * ENEMY_R - l) * 0.5;
            }
        }
        if let Some(sp) = sp {
            let d = (t.translation - sp).with_y(0.0);
            let l = d.length();
            if l < ENEMY_R + SORA_R {
                push += if l > 1e-4 { d / l } else { Vec3::Z } * (ENEMY_R + SORA_R - l);
            }
        }
        t.translation += push;
    }
}

/// The `atkp` row a hit names and its base damage on `to`, before the form and ability multipliers:
/// a spell hit carries the exact `SubId` of the row its attack object used (`Hit::row`, so SubId 2 is
/// its own row with its own Power), any other hit the default lookup (`GameData::hit_row`).
fn row_damage<'a>(data: &'a GameData, h: &Hit, from: &Stats, to: &Stats) -> Option<(&'a kh2_tables::tables::Atkp, i32)> {
    let atk = data.hit_row(h)?;
    Some((atk, data.damage(atk, from, to)))
}

/// A party member's stats for the damage formula, from its saved record (`Progress::member`: strength,
/// magic, defense, level, maximum HP). [inferred] The equipment pass of the status rebuild is not applied:
/// the hand-0 weapon's attack and magic (`item_stat`) are not added, unlike Sora's keyblade in
/// `GameData::sora_stats`.
pub(crate) fn member_stats(progress: Option<&crate::progress::Progress>, neo: u16) -> Option<Stats> {
    let m = progress?.member(neo)?;
    Some(Stats {
        level: m.level as i32,
        max_hp: m.hp_max as i32,
        strength: m.strength as i32,
        magic: m.magic as i32,
        defense: m.defense as i32,
        max_taken: i32::MAX,
        min_taken: 1,
        max_dealt: i32::MAX,
        min_dealt: 0,
        weakness: [100; 7],
    })
}

/// `member_stats` with the strength, magic and defense of the rebuilt status (`PartyData::member_status`:
/// the lvup row, the record and the equipment) when the party data is loaded.
pub(crate) fn member_stats_rebuilt(progress: Option<&crate::progress::Progress>, party: Option<&crate::party::PartyData>, neo: u16) -> Option<Stats> {
    let mut s = member_stats(progress, neo)?;
    if let (Some(party), Some(rec)) = (party, progress.and_then(|p| p.member(neo))) {
        (s.strength, s.magic, s.defense) = party.member_status(neo, &rec);
    }
    Some(s)
}

/// Sora's and the party's hits on enemies: damage, then the victim reaction of the attack row
/// (`reaction_rules::victim_outcome`): nothing, a push, a flinch, a flinch with a push, or a blow.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn receive_hits(
    mut hits: MessageReader<Hit>,
    data: Res<GameData>,
    reaction: Option<Res<ReactionData>>,
    sora: Query<&Sora>,
    mut q: Query<(&mut Enemy, &mut Fighter, Option<&ReactionStatus>)>,
    mut anims: Query<&mut Animator>,
    mut popups: MessageWriter<crate::hud::DamagePopup>,
    mut sounds: MessageWriter<crate::audio::PlaySe>,
    mut ineffective: MessageWriter<IneffectiveHit>,
    mut ai_hits: MessageWriter<crate::enemy_ai::AiHit>,
    mut game: MessageWriter<crate::fx::GameEffect>,
    drive: (Option<Res<crate::drive::DriveRules>>, Option<Res<crate::drive::DriveState>>),
    (abilities, magic): (Option<Res<crate::abilities::Abilities>>, Option<Res<crate::magic::MagicState>>),
    (pull_data, spells, mut rng, member_attacks): (Option<Res<PullData>>, Option<Res<crate::magic::SpellAttacks>>, Local<PullRng>, Option<Res<crate::friend_ai::MemberAttacks>>),
    (members, progress, mut form_exp, party): (Query<&crate::party::PartyMember>, Option<Res<crate::progress::Progress>>, MessageWriter<crate::prize::FormExpGain>, Option<Res<crate::party::PartyData>>),
    mut defeated: MessageWriter<crate::progress::EnemyDefeated>,
) {
    let sora_from = sora.single().map(|s| (s.stats, s.hits)).ok();
    let Some(reaction) = reaction else { return };
    for h in hits.read() {
        // The attacker's stats: Sora's, or the party member's own record (WP-F1 item 5; the game's damage
        // formula FUN_001a0610 reads the attacker's status block +0x188 / +0x18a, rebuilt from the saved
        // record by FUN_0019fd30 -> FUN_0019f920). A member's hit carries no combo count.
        let from = match members.get(h.attacker) {
            Ok(m) => member_stats_rebuilt(progress.as_deref(), party.as_deref(), m.neo).map(|s| (s, 0)),
            Err(_) => sora_from,
        };
        let (Ok((mut e, mut f, status)), Some((from, combo_hits))) = (q.get_mut(h.target), from) else { continue };
        if !e.alive() {
            continue;
        }
        // Berserk Charge (FUN_001a0610): for rows whose byte +5 is 0 (the strength stat) the attacker's
        // strength gets the block's +0x4e while his MP charge runs; Sora only [inferred, as berserk_attack].
        let mut from = from;
        if let (Some(ab), Some(mg)) = (&abilities, &magic)
            && members.get(h.attacker).is_err()
            && let Some(row) = data.hit_row(h)
        {
            let extra = crate::abilities::berserk_for_row(ab.block(), mg.charge.active(), row.critical_adjust);
            if extra > 0 && std::env::var_os("KH2_ABILITYLOG").is_some() {
                info!("abilities: Berserk Charge: strength {} + {extra} for atkp {}", from.strength, h.atkp);
            }
            from.strength += extra;
        }
        let Some((atk, mut dmg)) = row_damage(&data, h, &from, &e.stats) else { continue };
        let useless = hit_is_ineffective(atk, &e.stats);
        // Anti form multiplies the damage Sora deals (FUN_001a0610), then the attacker's ability
        // block adds its element and combo/reaction boosts (FUN_001a0a18, FUN_001a0aa8).
        if let (Some(rules), Some(state)) = (&drive.0, &drive.1) {
            dmg = crate::drive::rules::outgoing_damage(&rules.tuning, state.form, atk.r#type, dmg);
        }
        if let Some(ab) = &abilities {
            dmg = crate::abilities::attacker_bonuses(ab.block(), combo_hits, atk, dmg);
        }
        // FUN_001a0100: the new HP is held between word 2 (`min_hp`) and the maximum.
        let was_alive = e.hp > 0;
        e.hp = (e.hp - dmg).max(e.min_hp);
        // Form experience (WP-F1 item 7): the enemies' hit method (0x1849d8) calls FUN_001d4078 for the hit
        // (Valor, the attacker the player: +0x588 bit 0x80, in the object list per FUN_0019a3f8), and the
        // defeat method FUN_00184a30 calls FUN_001d40e0(object id) when the player defeated it.
        if members.get(h.attacker).is_err()
            && let Some(state) = &drive.1
        {
            let form = state.object_form();
            let hit_exp = crate::prize::valor_hit_exp(form);
            let defeat = if was_alive && e.hp <= 0 { crate::prize::defeat_exp(form, e.object_id) } else { 0 };
            for amount in [hit_exp, defeat].into_iter().filter(|&a| a > 0) {
                info!("{} hit by Sora in form {form}: form experience +{amount} (FUN_001d4078 / FUN_001d40e0, object {:#x})", e.name, e.object_id);
                form_exp.write(crate::prize::FormExpGain { form, amount });
            }
        }
        // The victim's `dead` method FUN_001b25d0 pays the experience (progress::pay_defeats).
        if was_alive && e.hp <= 0 {
            defeated.write(crate::progress::EnemyDefeated { victim: h.target, enmp: e.kind.enmp, by_player: members.get(h.attacker).is_err() });
        }
        if e.hp == 0 && e.kind.death_effect {
            // The damage method starts the death at the moment HP reaches 0 (FUN_00186530 -> the
            // `dead` mode); a second request for the same victim is ignored by `fx`.
            game.write(crate::fx::GameEffect::Death { victim: h.target });
        }
        popups.write(crate::hud::DamagePopup { at: h.point, amount: dmg });
        if useless {
            ineffective.write(IneffectiveHit { attacker: h.attacker, target: h.target, atkp: h.atkp, point: h.point });
        }
        let Ok(mut a) = anims.get_mut(e.body) else { continue };
        let rig = a.rig.clone();
        // The victim's hit sound (FUN_001b2378): not for ineffective hits; the sound alternates
        // between two entries per victim (the actor byte at +0x7fc).
        if !useless && let Some(se) = crate::audio::PlaySe::hit(atk.enemy_reaction, e.hit_sounds, rig.apdx.clone()) {
            sounds.write(se);
            e.hit_sounds = e.hit_sounds.wrapping_add(1);
        }
        let facing = Vec3::new(e.yaw.sin(), 0.0, e.yaw.cos());
        let from_front = facing.dot(h.dir) < 0.0;
        let ctx = VictimCtx {
            status: status.map_or(0, |x| x.0),
            grounded: e.grounded,
            attacker_flag_4000: false,
            from_front,
            hit_part_plain: true,
        };
        let out = rules::victim_outcome(&rules::HitRow::from_atkp(atk), useless, &ctx, &reaction.patn, &reaction.defaults, &reaction.consts);
        ai_hits.write(crate::enemy_ai::AiHit { target: h.target, real: rules::reaction_class(out.effective) >= 2, reaction: out.effective });
        // Dying is not part of the reaction rules: the last hit always launches.
        let d = &reaction.defaults;
        let action = match out.action {
            x @ VictimAction::Blow { .. } => x,
            // An ineffective hit has reaction 0 (flag 4): no flinch, push or blow.
            _ if e.hp == 0 && !useless => VictimAction::Blow { motion: rules::blow_motion(from_front), up: d.blow_up, horiz: d.blow_speed, face_attacker: from_front },
            x => x,
        };
        info!(
            "{} hit: atkp {} damage {dmg} hp {} reaction {} effective {} ineffective {useless} {} -> {action:?} (atkp row SubId {})",
            e.name,
            h.atkp,
            e.hp,
            atk.enemy_reaction,
            out.effective,
            if from_front { "front" } else { "back" },
            atk.sub_id
        );
        let away = h.dir.with_y(0.0).normalize_or_zero();
        let push = |strength: f32| Push::new(away, strength, 0.0, &reaction.consts);
        let first = |ids: &[usize]| ids.iter().find_map(|&i| has(&rig, i));
        // Any other reaction replaces the pull's mover (FUN_0016b698 switches the mover).
        if !matches!(action, VictimAction::None) {
            e.pull = None;
        }
        // Reaction class 5 (FUN_00185668 -> FUN_001bf920): the victim keeps the attack object and is
        // pulled toward it (see `think`). `victim_outcome` leaves the class to the caller since the
        // pull needs the attack object; the app has it for spell attacks (`magic::SpellAttacks`) and the
        // party members' script attack objects (`friend_ai::MemberAttacks`, Goofy's `tornado`).
        if matches!(action, VictimAction::NotDecoded { class: 5 }) {
            let group = spells.as_ref().and_then(|s| s.hit_group(h.target, h.atkp)).or_else(|| member_attacks.as_ref().and_then(|m| m.hit_group(h.target, h.atkp)));
            match (group, pull_data.as_ref()) {
                (Some(group), Some(pd)) => {
                    if rng.0 == 0 {
                        rng.0 = 1;
                    }
                    rng.0 = rng.0.wrapping_mul(0x10dcd);
                    let p = rules::Pull::new(atk.knockback_strength1, atk.knockback_strength2, atk.unknown, from_front, rng.0, &pd.0);
                    e.pull = Some((group, p));
                    e.push = None;
                    e.vel = Vec3::ZERO;
                    e.timer = 0.0;
                    e.state = EState::Hurt;
                    // FUN_0017e220(0, 0, controller, 0x14 / 0x15): blend 0 [the motion loops at its own
                    // return frame while the pull lasts, inferred].
                    let slot = has(&rig, p.motion as usize);
                    let running = a.slot;
                    a.restart(slot.or(running), true, 0.0);
                    info!(
                        "{} pulled (reaction class 5, FUN_001bf920): attack {group}, hold {:.1} (kb {} .. {}), swirl {}, motion {:#x}{}",
                        e.name,
                        p.hold,
                        atk.knockback_strength1,
                        atk.knockback_strength2,
                        p.swirl,
                        p.motion,
                        if slot.is_none() { " (not in the set: the running motion stays)" } else { "" }
                    );
                }
                _ => info!("{} reaction class 5 without a spell attack object or the executable's numbers: no pull", e.name),
            }
        }
        match action {
            VictimAction::None | VictimAction::NotDecoded { .. } => {}
            VictimAction::Push { strength } => e.push = push(strength),
            VictimAction::Flinch { motion, fallback } | VictimAction::FlinchPush { motion, fallback, .. } => {
                if let VictimAction::FlinchPush { strength, .. } = action {
                    e.push = push(strength);
                }
                e.vel = Vec3::new(0.0, e.vel.y, 0.0);
                e.timer = 0.0;
                e.state = EState::Hurt;
                a.restart(first(&[motion as usize, fallback as usize]), false, 0.03);
            }
            VictimAction::Class14 { motion, strength, face_attacker } => {
                let to = if face_attacker { -away } else { away };
                e.yaw = to.x.atan2(to.z);
                e.push = push(strength);
                e.vel = Vec3::new(0.0, e.vel.y, 0.0);
                e.timer = 0.0;
                e.state = EState::Hurt;
                a.restart(first(&[motion as usize, HURT_FRONT]), false, 0.03);
            }
            VictimAction::Blow { motion, up, horiz, face_attacker } => {
                // Faces the attacker when hit from the front, launched along the hit direction.
                let to = if face_attacker { -away } else { away };
                e.yaw = to.x.atan2(to.z);
                e.blow = Some(Blow::new(away, up, horiz, d));
                e.push = None;
                e.blown_back = !from_front;
                e.vel = Vec3::ZERO;
                e.grounded = false;
                e.timer = 0.0;
                e.state = EState::Blown;
                a.restart(first(&[motion as usize, BLOW, HURT_FRONT]), false, 0.03);
            }
        }
        if !matches!(action, VictimAction::None | VictimAction::NotDecoded { .. } | VictimAction::Push { .. }) || (matches!(action, VictimAction::NotDecoded { class: 5 }) && e.pull.is_some()) {
            f.landed.clear();
        }
    }
}

/// An enemy attack volume met a player attack volume (`clash.rs`) and the enemy-class handler of the
/// game (0x1b2478) reacts: the row's `ReflectedMotion` with a recoil push (FUN_00185d30, power
/// `ReflectHitBack`, negative for the default). `ReflectAction` (a script action id) is not carried:
/// the table behind FUN_0019e870 is not decoded. Written for every clash of an enemy volume, also when
/// the handler's own part does nothing (`motion` `None`): the script part of 0x1b2478 (clash hook and
/// event 0x13, `enemy_ai.rs`) runs before its flag 0x40 / `RefactSelf` 2 tests.
#[derive(Message, Clone, Copy, Debug)]
pub struct EnemyClash {
    pub enemy: Entity,
    pub motion: Option<u8>,
    pub power: f32,
    /// The attack rows of the enemy's own volume and of the other one.
    pub own_atkp: i16,
    pub other_atkp: i16,
}

/// Plays the clash recoil: the motion, then a push backwards along the enemy's own facing
/// (FUN_001857b0). A motion the enemy's set does not have is ignored.
pub fn receive_clashes(
    mut clashes: MessageReader<EnemyClash>,
    reaction: Option<Res<ReactionData>>,
    mut q: Query<&mut Enemy>,
    mut anims: Query<&mut Animator>,
) {
    let Some(reaction) = reaction else { return };
    for c in clashes.read() {
        let (Ok(mut e), Some(m)) = (q.get_mut(c.enemy), c.motion) else { continue };
        if !e.alive() {
            continue;
        }
        let Ok(mut a) = anims.get_mut(e.body) else { continue };
        let rig = a.rig.clone();
        let Some(slot) = has(&rig, m as usize) else { continue };
        let facing = Vec3::new(e.yaw.sin(), 0.0, e.yaw.cos());
        e.push = Push::new(-facing, rules::recoil_power(c.power, &reaction.consts), 0.0, &reaction.consts);
        e.vel = Vec3::new(0.0, e.vel.y, 0.0);
        e.timer = 0.0;
        e.state = EState::Hurt;
        a.restart(Some(slot), false, 0.0);
        info!("{} clash: recoil motion {m}", e.name);
    }
}

#[cfg(test)]
mod tests {
    /// Two enemies on the same point part: each is pushed the opposite way (by index), and the pushes of the pair cancel as
    /// they separate. Apart, the direction is the plain one away from the other body.
    #[test]
    fn bodies_on_one_point_are_pushed_apart() {
        assert_eq!(pair_dir(Vec3::ZERO, 0.0, 2, 5), Vec3::X);
        assert_eq!(pair_dir(Vec3::ZERO, 0.0, 5, 2), -Vec3::X);
        assert_eq!(pair_dir(Vec3::new(0.0, 0.0, -2.0), 2.0, 1, 0), Vec3::new(0.0, 0.0, -1.0));
    }

    use super::*;

    fn stats(strength: i32, magic: i32) -> Stats {
        Stats { level: 1, max_hp: 1000, strength, magic, defense: 0, max_taken: i32::MAX, min_taken: 1, max_dealt: i32::MAX, min_dealt: 0, weakness: [100; 7] }
    }

    /// al00's `btl` program 0xa loads b_20, which holds ten `M_EX420` (object 120, a kind of `KINDS`; the group's other
    /// members are kinds the app lacks). Needs `KH2_ISO`.
    #[test]
    fn al00_btl_program_a_holds_ten_m_ex420() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let groups = area_groups(&disc, "al00");
        let ard = disc.read("ard/al00.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("al00.ard");
        let run = kh2_ard::run::run_area(&ard, [0, 0xa, 0], 0);
        let b20 = btl_groups(&groups, &run).into_iter().find(|g| g.name == "b_20").expect("program 0xa loads b_20");
        assert_eq!(b20.members.iter().filter(|m| m.kind.name == M_EX420.name).count(), 10);
        assert!(KINDS.iter().any(|k| k.model == "M_EX420" && k.enmp == 9));
    }

    /// tt04's `btl` programs pick their groups (disc): program 0 (a new game) is not in the script and loads none;
    /// program 1 (story flag 0x883) loads b_00, whose three Dusks are all spawn kind 2 of a kind-2 group (none at
    /// load, created on demand); program 0x50 loads b_44, whose kind-2 group has Dusks of both kinds. Needs `KH2_ISO`.
    #[test]
    fn tt04_btl_programs_choose_their_groups() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let groups = area_groups(&disc, "tt04");
        let ard = disc.read("ard/tt04.ard").and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a).expect("tt04.ard");
        let pick = |btl: u8| {
            let run = kh2_ard::run::run_area(&ard, [0, btl, 0], 0);
            btl_groups(&groups, &run).into_iter().cloned().collect::<Vec<_>>()
        };
        let split = |g: &AreaGroup| {
            let now = g.members.iter().filter(|m| created_at_load(m, &g.parts[m.part], 0)).count();
            let later = g.members.iter().filter(|m| m.spawn_kind == 2 && g.parts[m.part].kind == 2).count();
            (now, later)
        };
        for btl in [0u8, 1, 4, 5, 7, 0x1b, 0x1c, 0x4c, 0x4d, 0x4e, 0x4f, 0x50, 0xb5, 0xb6, 0xb7, 0xb8] {
            for g in pick(btl) {
                let (now, later) = split(&g);
                eprintln!(
                    "tt04 btl {btl:#x}: {} parts {:?} at load {now}, on demand {later}, mission {:?}",
                    g.name,
                    g.parts.iter().map(|p| (p.id, p.kind, p.start_delay(), p.removal_delay(), p.header[0x22], p.header[0x23], p.header[0x24])).collect::<Vec<_>>(),
                    g.mission
                );
                for m in &g.members {
                    eprintln!("    {} part {} sk {} arg {} serial {:#x} delay {} wave {}", m.kind.name, m.part, m.spawn_kind, m.spawn_arg, m.serial, m.delay, m.wave);
                }
            }
        }
        assert!(pick(0).is_empty(), "tt04 has no btl program 0");
        let one = pick(1);
        assert_eq!(one.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["b_00"]);
        assert_eq!(split(&one[0]), (0, 3), "b_00: three spawn-kind-2 Dusks of a kind-2 group");
        let b44 = pick(0x50).into_iter().find(|g| g.name == "b_44").expect("program 0x50 loads b_44");
        let (now, later) = split(&b44);
        assert!(now > 0 && later > 0, "b_44: {now} at load, {later} on demand");
        assert!(b44.members.iter().all(|m| m.kind.name == DUSK.name || m.kind.name == M_EX940.name));
        assert_eq!(b44.members.iter().filter(|m| m.kind.name == M_EX940.name).count(), 4, "b_44 holds four M_EX940 (object type 0x15)");
        // Placement 1 (near the player, FUN_00199970) is the record byte +0x1d of the three Dusks of wave 1.
        let near: Vec<u16> = b44.members.iter().filter(|m| m.spawn_kind == 2 && m.spawn_arg == 1).map(|m| m.serial).collect();
        assert_eq!(near, [0x24, 0x25, 0x101]);
        assert!(b44.members.iter().filter(|m| m.spawn_arg == 1).all(|m| m.kind.name == DUSK.name && m.wave == 1));
        // Every enemy-class entity of the group is counted (FUN_001994c8): the 3 spawn-kind-3 Dusks at load, the 4+4
        // spawn-kind-2 entities (type 4 and 0x15) in the record.
        let counts: Vec<i32> = b44.parts.iter().map(|p| counted_at_load(p, 0, &[])).collect();
        eprintln!("b_44 counters at load: {counts:?}");
        assert_eq!(counts.iter().sum::<i32>(), 3 + 8);
    }

    /// `M_EX760` (object 0x131) is an enemy kind of the app: bb08's btl groups b_20 and b_21 hold it, its `00objentry` row
    /// is type 4 with `NeoStatus` 1028 and its script's `set_obj_param` row is `enmp` 36 (checked in the disc's table).
    #[test]
    fn m_ex760_is_an_enemy_kind_that_bb08_spawns() {
        assert!(KINDS.iter().any(|k| std::ptr::eq(*k, &M_EX760)));
        assert_eq!((M_EX760.enmp, M_EX760.model, M_EX760.status, M_EX760.death_effect), (36, "M_EX760", true, true));
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().expect("archive lock");
            kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).expect("objentry").rows
        };
        let row = rows.iter().find(|r| r.model_name == "M_EX760").expect("objentry row of M_EX760");
        assert_eq!((row.object_id, row.object_type, row.neo_status), (305, OBJECT_ENEMY, 1028));
        let groups = area_groups(&disc, "bb08");
        for name in ["b_20", "b_21"] {
            let g = groups.iter().find(|g| g.name == name).unwrap_or_else(|| panic!("bb08 has {name}"));
            assert!(!g.members.is_empty() && g.members.iter().all(|m| m.kind.name == M_EX760.name), "{name} holds only M_EX760");
            assert!(g.skipped.is_empty(), "{name}: every enemy of the group is a kind of the app, not {:?}", g.skipped);
        }
    }

    fn member(part: usize, spawn_kind: i32, serial: u16, delay: u16, wave: u8) -> Member {
        Member { kind: &DUSK, at: Vec3::ZERO, yaw: 0.0, spawn_kind, spawn_params: [0; 2], object_id: 0x13e, part, spawn_arg: 0, serial, delay, wave, limiter: 1, radius: 0, flies: false }
    }

    /// `FUN_001991a0`: a kind-2 group creates one absent spawn-kind-2 member at a time and waits its delay; members
    /// of a later wave come once the defeats of the waves before allow it (`FUN_00199618`), a defeated member of a wave
    /// up to 99 does not come back, kinds 0/3 are left to the set-up.
    #[test]
    fn a_kind_two_group_creates_one_member_per_delay_and_moves_on_waves() {
        let mut header = [0u8; 0x2c];
        header[0x1c] = 30; // start delay
        header[0x20] = 90; // after a removal
        header[0x22] = 2; // SpawnLimiter sum allowed before the next wave
        let entities = vec![(3, 0, 1), (2, 0, 1), (2, 0, 1), (2, 1, 1)];
        let part = GroupPart { id: 0x8a, kind: 2, disabled: false, header, activators: Vec::new(), entities, counted: Vec::new() };
        let g = AreaGroup {
            program: 0x50,
            name: "b_t".into(),
            parts: vec![part],
            members: vec![member(0, 3, 1, 0, 0), member(0, 2, 2, 10, 0), member(0, 2, 3, 20, 0), member(0, 2, 4, 5, 1)],
            skipped: Vec::new(),
            player: None,
            mission: None,
            orb_pool: false,
            triggers: Vec::new(),
        };
        assert!(created_at_load(&g.members[0], &g.parts[0], 0));
        assert!(!created_at_load(&g.members[1], &g.parts[0], 0));
        let mut two = KindTwo::new(&g, 0);
        assert_eq!(two.timer, 30.0);
        let mut alive = [true, false, false, false];
        assert!(two.run(&g, |i| alive[i]).is_empty(), "the start delay runs");
        two.timer = 0.0;
        assert_eq!(two.run(&g, |i| alive[i]), [1]);
        assert_eq!(two.timer, 10.0);
        alive[1] = true;
        two.timer = 0.0;
        assert_eq!(two.run(&g, |i| alive[i]), [2]);
        alive[2] = true;
        two.timer = 0.0;
        // Wave 0's sum is 1 + 1 + 1 = 3 > 2: no next wave, nothing to create.
        assert!(two.run(&g, |i| alive[i]).is_empty());
        assert_eq!(two.wave, 0);
        // Member 2 is defeated: the timer restarts at +0x20 and it stays defeated (wave 0 <= 99, FUN_00199378).
        alive[2] = false;
        two.removed(&g, &g.members[2]);
        assert_eq!((two.timer, two.killed.clone()), (90.0, vec![3]));
        assert!(two.run(&g, |i| alive[i]).is_empty(), "the removal delay runs");
        two.timer = 0.0;
        // It does not come back; the sum drops to 2, so the walk moves to wave 1 and creates its member.
        assert_eq!(two.run(&g, |i| alive[i]), [3]);
        assert_eq!(two.wave, 1);
        // A member of a wave above 99 comes back after its defeat.
        let mut g2 = g.clone();
        g2.members[1].wave = 100;
        let mut two = KindTwo { part: 0, timer: 0.0, wave: 100, killed: Vec::new() };
        two.removed(&g2, &g2.members[1]);
        assert!(two.killed.is_empty());
    }

    /// Review finding 1 (`FUN_00199858`): when the waves wrap back, the members defeated for good in the lap before
    /// leave the table of the defeated, so they come again; one still alive stays out.
    #[test]
    fn a_wave_wrap_takes_the_defeated_members_out_of_the_table() {
        let mut header = [0u8; 0x2c];
        header[0x22] = 9; // SpawnLimiter sum allowed
        header[0x23] = 2; // go back at wave 2
        header[0x24] = 0; // to wave 0
        let entities = vec![(2, 0, 1), (2, 1, 1)];
        let part = GroupPart { id: 0x8a, kind: 2, disabled: false, header, activators: Vec::new(), entities, counted: Vec::new() };
        let g = AreaGroup {
            program: 0x50,
            name: "b_t".into(),
            parts: vec![part],
            members: vec![member(0, 2, 1, 10, 0), member(0, 2, 2, 10, 1)],
            skipped: Vec::new(),
            player: None,
            mission: None,
            orb_pool: false,
            triggers: Vec::new(),
        };
        let mut two = KindTwo { part: 0, timer: 0.0, wave: 0, killed: vec![1, 2] };
        // Both are defeated, so nothing is created on the way up; at wave 2 the walk goes back to wave 0 and takes both out
        // of the table, and the first is created in the new lap.
        assert_eq!(two.run(&g, |_| false), [0]);
        assert!(two.killed.is_empty(), "wave 0 <= wave byte: both were taken out");
        // A member that is still there stays in the table (FUN_001995c8 needs the object absent).
        let mut two = KindTwo { part: 0, timer: 0.0, wave: 0, killed: vec![1, 2] };
        assert_eq!(two.run(&g, |i| i == 1), [0]);
        assert_eq!(two.killed, [2]);
    }

    /// A member's hit takes the rebuilt status: Donald's magic is the lvup row plus record plus staff, above the
    /// bare record's. Needs `KH2_ISO`.
    #[test]
    fn member_hit_uses_the_rebuilt_magic() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let party = crate::party::PartyData::load(&disc);
        let p = crate::progress::Progress::start(&disc).expect("progress");
        let bare = member_stats(Some(&p), 2).expect("bare");
        let built = member_stats_rebuilt(Some(&p), Some(&party), 2).expect("rebuilt");
        assert!(built.magic > bare.magic, "{} vs {}", built.magic, bare.magic);
        assert_eq!(member_stats_rebuilt(Some(&p), None, 2).expect("fallback").magic, bare.magic);
    }

    /// A spell hit whose `Hit::row` names SubId 2 deals the damage of that row (its own Power), not the
    /// default row of the same `atkp` id (`receive_hits` takes both from `row_damage`). Fire is atkp 502:
    /// its rows carry the hit-by-hit variants (`magic_rules::variant`). Needs `KH2_ISO`.
    #[test]
    fn sub_id_2_hit_uses_its_own_atkp_row() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let data = GameData::load(&Disc::open(&iso));
        // Every id that has a SubId 0 row and a SubId 2 row with a different Power.
        let ids: Vec<u16> = data
            .atkp
            .iter()
            .filter(|b| b.sub_id == 2 && data.atkp.iter().any(|a| a.id == b.id && a.sub_id == 0 && a.power != b.power && a.r#type == b.r#type))
            .map(|b| b.id)
            .collect();
        eprintln!("atkp ids with distinct SubId 0 and SubId 2 rows: {ids:?}");
        assert!(!ids.is_empty());
        let (from, to) = (stats(200, 200), stats(0, 0));
        let mut checked = 0;
        for id in ids {
            let base = data.atkp.iter().find(|a| a.id == id && a.sub_id == 0).unwrap();
            let own = data.atkp.iter().find(|a| a.id == id && a.sub_id == 2).unwrap();
            let hit = |row| Hit { attacker: Entity::PLACEHOLDER, target: Entity::PLACEHOLDER, atkp: id as i16, dir: Vec3::X, point: Vec3::ZERO, row };
            let (row, dmg) = row_damage(&data, &hit(Some(2)), &from, &to).unwrap();
            assert_eq!(row.sub_id, 2, "atkp {id}: the hit names SubId 2");
            assert_eq!(dmg, data.damage(own, &from, &to), "atkp {id}: damage comes from the SubId 2 row");
            let (dflt, dflt_dmg) = row_damage(&data, &hit(None), &from, &to).unwrap();
            assert_eq!(dflt.sub_id, base.sub_id, "atkp {id}: no row named means the default lookup");
            assert_eq!(dflt_dmg, data.damage(base, &from, &to));
            if dmg != dflt_dmg {
                checked += 1;
            }
            if id == crate::magic::ATKP_FIRE {
                eprintln!("fire (atkp {id}): SubId 2 Power {} -> damage {dmg}; default SubId {} Power {} -> damage {dflt_dmg}", own.power, dflt.sub_id, base.power);
            }
        }
        assert!(checked > 0, "at least one id deals different damage through its SubId 2 row");
    }

    /// `FUN_001a6068`: the generator is `seed * 0x10dcd` read unsigned and scaled by 2^-32; the octant number
    /// counts 0..7 round as `(c & 4) >> 2 | c << 1`.
    #[test]
    fn place_rng_follows_the_games_generator() {
        let mut r = PlaceRng::default();
        // Literal values of seed * 0x10dcd (mod 2^32) / 2^32 from the start seed 0x2545f491: 0x5b01351d, 0x24c70139, 0x8fa4dfa5.
        let seq: Vec<f32> = (0..3).map(|_| r.next()).collect();
        for (v, want) in seq.iter().zip([0.355_487_17_f32, 0.143_661_58, 0.561_109_5]) {
            assert!((v - want).abs() < 1e-6, "{v} vs {want}");
        }
        assert_eq!(r.seed, 0x8fa4_dfa5);
        let mut r = PlaceRng::default();
        let octants: Vec<u32> = (0..9).map(|_| r.octant()).collect();
        assert_eq!(octants, [0, 2, 4, 6, 9, 11, 13, 15, 0]);
    }

    /// The floor probe of `ring_place_in` (`FUN_001a5c30`): the collision is asked from 2 m above the point and a floor more than
    /// 12 m below the point is not taken.
    #[test]
    fn the_floor_probe_asks_from_2_m_above_and_takes_a_floor_up_to_12_m_below() {
        let p = Vec3::new(1.0, 5.0, -2.0);
        let asked = std::cell::Cell::new((0.0, 0.0, 0.0));
        let at = |y: f32| {
            move |x: f32, z: f32, from: f32| {
                let _ = (x, z, from);
                Some(y)
            }
        };
        assert_eq!(floor_probe(p, |x, z, from| {
            asked.set((x, z, from));
            Some(-7.0)
        }), Some(Vec3::new(1.0, -7.0, -2.0)), "12 m below is still taken");
        assert_eq!(asked.get(), (1.0, -2.0, 7.0));
        assert_eq!(floor_probe(p, at(-7.01)), None, "past 12 m below");
        assert_eq!(floor_probe(p, |_, _, _| None), None);
        assert_eq!(floor_probe(p, at(6.0)), Some(Vec3::new(1.0, 6.0, -2.0)), "a floor above the point (a ceiling's underside is the query's business)");
    }

    /// Review finding 9 (`FUN_001997f8`): the wave counts up only from below 200, and the wrap test uses the new wave, so a
    /// wrap at wave 200 still happens on the step from 199 and a group with no wave left stops at 200.
    #[test]
    fn the_wave_counter_stops_at_200_and_wraps_there() {
        let part = |back_at: u8, back_to: u8, entities: Vec<(u8, u8, u8)>| {
            let mut header = [0u8; 0x2c];
            header[0x22] = 9;
            header[0x23] = back_at;
            header[0x24] = back_to;
            GroupPart { id: 0x8a, kind: 2, disabled: false, header, activators: Vec::new(), entities, counted: Vec::new() }
        };
        let group = |p: GroupPart, members: Vec<Member>| AreaGroup { program: 0x50, name: "b_t".into(), parts: vec![p], members, skipped: Vec::new(), player: None, mission: None, orb_pool: false, triggers: Vec::new() };
        // No wave to move to: the walk ends at 200 and returns nothing; a second run does not move it.
        let g = group(part(0, 0, vec![]), vec![]);
        let mut two = KindTwo { part: 0, timer: 0.0, wave: 198, killed: Vec::new() };
        assert!(two.run(&g, |_| false).is_empty());
        assert_eq!(two.wave, 200);
        assert!(two.run(&g, |_| false).is_empty());
        assert_eq!(two.wave, 200);
        // A wrap at 200 (header +0x23) is taken on the step from 199 and goes back to wave 5, where the only entity is (its
        // member is alive, so nothing is created): every lap of the walk ends on wave 5 again, never above 200.
        let g = group(part(200, 5, vec![(2, 5, 1)]), vec![member(0, 2, 7, 10, 5)]);
        let mut two = KindTwo { part: 0, timer: 0.0, wave: 199, killed: Vec::new() };
        assert!(two.run(&g, |_| true).is_empty());
        assert_eq!(two.wave, 5);
    }

    fn flat(p: Vec3) -> Option<Vec3> {
        Some(p.with_y(0.0))
    }

    /// `FUN_001a6198`: on open floor the first try is taken, at the drawn radius (between the limits) from the
    /// centre, on the floor.
    #[test]
    fn ring_search_on_open_floor_takes_the_first_try() {
        let ring = Ring { centre: Vec3::new(3.0, 2.0, -4.0), min: 1.5, max: 4.5, heights: None };
        let mut rng = PlaceRng::default();
        let mut seen = (f32::MAX, 0.0f32);
        for _ in 0..200 {
            let p = ring_place(&mut rng, &ring, &flat, &|_, _| true);
            let d = Vec2::new(p.x - 3.0, p.z + 4.0).length();
            assert!((1.5 - 1e-3..=4.5 + 1e-3).contains(&d), "distance {d}");
            assert_eq!(p.y, 0.0);
            seen = (seen.0.min(d), seen.1.max(d));
        }
        assert!(seen.0 < 2.0 && seen.1 > 4.0, "the radius covers the ring: {seen:?}");
    }

    /// With floor only close to the centre the radius shrinks by 1 m after 8 tries until a floor is found
    /// (the loop of `FUN_001a6198`), and the point is never beyond the floor.
    #[test]
    fn ring_search_shrinks_until_it_finds_floor() {
        let ring = Ring { centre: Vec3::ZERO, min: 0.5, max: 4.5, heights: None };
        let near = |p: Vec3| (Vec2::new(p.x, p.z).length() <= 1.6).then(|| p.with_y(0.0));
        let mut rng = PlaceRng::default();
        for _ in 0..100 {
            let p = ring_place(&mut rng, &ring, &near, &|_, _| true);
            let d = Vec2::new(p.x, p.z).length();
            assert!(d <= 1.6 + 1e-3, "{d}");
        }
    }

    /// A line test that refuses everything, or no floor anywhere: the floor under the centre (else the centre).
    #[test]
    fn ring_search_falls_back_to_the_centre() {
        let ring = Ring { centre: Vec3::new(1.0, 2.0, 1.0), min: 1.5, max: 4.5, heights: None };
        let mut rng = PlaceRng::default();
        assert_eq!(ring_place(&mut rng, &ring, &flat, &|_, _| false), Vec3::new(1.0, 0.0, 1.0));
        assert_eq!(ring_place(&mut rng, &ring, &|_| None, &|_, _| true), ring.centre);
        let air = Ring { heights: Some((-1.0, 1.0)), ..ring };
        assert_eq!(ring_place(&mut rng, &air, &flat, &|_, _| false), ring.centre);
    }

    /// The line test decides which directions are used: with every line toward +x blocked no point lies at +x.
    #[test]
    fn ring_search_respects_the_line_test() {
        let ring = Ring { centre: Vec3::new(0.0, 2.0, 0.0), min: 1.5, max: 4.5, heights: None };
        let mut rng = PlaceRng::default();
        for _ in 0..200 {
            let p = ring_place(&mut rng, &ring, &flat, &|_, to| to.x < 0.0);
            assert!(p.x < 0.0, "{p:?}");
        }
    }

    /// `FUN_001a63c8`: in the air the height is the centre's plus a random offset in the range.
    #[test]
    fn ring_search_in_the_air_adds_a_height() {
        let ring = Ring { centre: Vec3::new(0.0, 3.0, 0.0), min: 1.5, max: 4.5, heights: Some((-1.0, 1.0)) };
        let mut rng = PlaceRng::default();
        let mut ys = Vec::new();
        for _ in 0..100 {
            let p = ring_place(&mut rng, &ring, &|_| None, &|_, _| true);
            let d = Vec2::new(p.x, p.z).length();
            assert!((1.5 - 1e-3..=4.5 + 1e-3).contains(&d) && (2.0..=4.0).contains(&p.y), "{p:?}");
            ys.push(p.y);
        }
        assert!(ys.iter().any(|y| *y < 2.8) && ys.iter().any(|y| *y > 3.2));
        let still = Ring { heights: Some((0.0, 0.0)), ..ring };
        assert_eq!(ring_place(&mut rng, &still, &|_| None, &|_, _| true).y, 3.0);
    }

    /// `FUN_001991a0`: placement byte 1 rings the player (centre raised 2 m), 2 the record's point with the record's
    /// radius; other values use the point.
    #[test]
    fn member_rings_follow_the_placement_byte() {
        let mut m = member(0, 2, 1, 0, 0);
        m.at = Vec3::new(5.0, 0.0, 6.0);
        m.radius = 800;
        assert_eq!(member_ring(&m, Vec3::ZERO), None);
        m.spawn_arg = 1;
        assert_eq!(member_ring(&m, Vec3::new(1.0, 0.0, 1.0)), Some(Ring { centre: Vec3::new(1.0, 2.0, 1.0), min: 1.5, max: 4.5, heights: None }));
        m.flies = true;
        assert_eq!(member_ring(&m, Vec3::ZERO).unwrap().heights, Some((-1.0, 1.0)));
        m.spawn_arg = 2;
        assert_eq!(member_ring(&m, Vec3::ZERO), Some(Ring { centre: Vec3::new(5.0, 0.0, 6.0), min: 0.0, max: 8.0, heights: Some((0.0, 0.0)) }));
        m.spawn_arg = 0;
        assert_eq!(member_ring(&m, Vec3::ZERO), None);
    }

    fn counted(spawn_kind: u8, arg: u8, serial: u16, enemy: bool) -> Counted {
        Counted { spawn_kind, arg, serial, enemy }
    }

    fn part_of(kind: u8, disabled: bool, counted: Vec<Counted>) -> GroupPart {
        GroupPart { id: 0x8a, kind, disabled, header: [0; 0x2c], activators: Vec::new(), entities: Vec::new(), counted }
    }

    /// `FUN_00198750` / `FUN_001994c8`: the counter holds the enemy-class entities, spawn kind 2 always, 0 and 3 in
    /// an enabled group, 1 for the current entrance; the table of the defeated and a kind-9 group count nothing.
    #[test]
    fn the_group_counter_counts_enemy_entities() {
        let cs = vec![counted(3, 0, 1, true), counted(0, 0, 2, true), counted(2, 0, 3, true), counted(1, 4, 4, true), counted(1, 5, 5, true), counted(3, 0, 6, false)];
        assert_eq!(counted_at_load(&part_of(2, false, cs.clone()), 4, &[]), 4);
        assert_eq!(counted_at_load(&part_of(2, false, cs.clone()), 4, &[1, 3]), 2);
        assert_eq!(counted_at_load(&part_of(2, true, cs.clone()), 4, &[]), 1, "a disabled group counts only its kind-2 entities");
        assert_eq!(counted_at_load(&part_of(9, false, cs), 4, &[]), 0);
    }

    /// `FUN_00199518`: the last removal of a group raises event 3 with its id; when every group is empty event 2
    /// follows; a disabled group raises nothing.
    #[test]
    fn emptied_groups_raise_events_3_and_2() {
        let mut c = vec![PartCount { id: 0x10, count: 2, disabled: false }, PartCount { id: 0x11, count: 1, disabled: false }, PartCount { id: 0x12, count: 5, disabled: true }];
        assert_eq!(removal_events(&mut c, 0), vec![]);
        assert_eq!(removal_events(&mut c, 0), vec![(3, 0x10)]);
        assert_eq!(removal_events(&mut c, 2), vec![]);
        assert_eq!(c[2].count, 5);
        // The disabled group's five still count in the sum (FUN_00199580 adds every group): no event 2 yet.
        assert_eq!(removal_events(&mut c, 1), vec![(3, 0x11)]);
        c[2].count = 0;
        c[0].count = 1;
        assert_eq!(removal_events(&mut c, 0), vec![(3, 0x10), (2, 0)]);
    }

    /// A kind-6 group creates every absent spawn-kind-2 member at once while the player is in a volume whose
    /// activator halfword +0x2c is 1; it creates nothing outside, in another group kind, or when disabled.
    #[test]
    fn a_kind_six_group_creates_its_absent_members_at_once() {
        let mut vol = kh2_ard::Activator { shape: 2, option: 0, position: [0.0; 3], scale: [1.0; 3], rotation: [0.0; 3], raw: [0; 0x40] };
        let mut other = vol.clone();
        vol.raw[0x2c] = 1;
        other.raw[0x2c] = 2;
        let mut part = part_of(6, false, Vec::new());
        part.activators = vec![other.clone(), vol.clone()];
        let g = AreaGroup {
            program: 1,
            name: "k6".into(),
            parts: vec![part],
            members: vec![member(0, 2, 1, 0, 0), member(0, 2, 2, 30, 5), member(0, 3, 3, 0, 0), member(0, 2, 4, 0, 0)],
            skipped: Vec::new(),
            player: None,
            mission: None,
            orb_pool: false,
            triggers: Vec::new(),
        };
        let only_vol = |v: &kh2_ard::Activator| v.raw[0x2c] == 1;
        // Wave bytes and delays are ignored; the spawn-kind-3 member is the set-up's; present ones are skipped.
        assert_eq!(kind_six_creates(&g, 0, only_vol, |s| s == 2), vec![0, 3]);
        assert_eq!(kind_six_creates(&g, 0, |v| v.raw[0x2c] == 2, |_| false), Vec::<usize>::new(), "a volume of another type does not count");
        assert_eq!(kind_six_creates(&g, 0, |_| false, |_| false), Vec::<usize>::new(), "the player is outside");
        let mut off = g.clone();
        off.parts[0].disabled = true;
        assert_eq!(kind_six_creates(&off, 0, only_vol, |_| false), Vec::<usize>::new());
        let mut two = g.clone();
        two.parts[0].kind = 2;
        assert_eq!(kind_six_creates(&two, 0, only_vol, |_| false), Vec::<usize>::new(), "a kind-2 group has its own walk");
    }

    /// Survey (disc, WP-EN2): no Twilight Town area script has a trigger on the group events 2 and 3, and no group
    /// of kind 3, 4 or 6 holds an enemy-class entity, so the events and kind-6 creation have no user in tt00..tt41.
    /// Needs `KH2_ISO`.
    #[test]
    fn no_twilight_town_data_uses_group_events_or_kind_six_enemies() {
        use kh2_tables::tables as t;
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let rows = {
            let mut a = disc.0.lock().unwrap();
            kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default()
        };
        let enemy = |o: u32| rows.iter().any(|r| r.object_id == o && (r.object_type == OBJECT_ENEMY || r.object_type == OBJECT_ENEMY_B));
        let (mut areas, mut kind6, mut triggers) = (0, 0, 0);
        for n in 0..=41 {
            let name = format!("tt{n:02}");
            let Some(ard) = disc.read(&format!("ard/{name}.ard")).and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a) else { continue };
            areas += 1;
            for p in ard.scripts.values().flatten() {
                for c in p.commands.iter().filter(|c| c.opcode == 0xc) {
                    if let Some(tr) = kh2_ard::parse_trigger(&c.args)
                        && (tr.event == 2 || tr.event == 3)
                    {
                        triggers += 1;
                    }
                }
            }
            for (tag, groups) in &ard.spawns {
                for g in groups {
                    if matches!(g.kind & 0xff, 3 | 4 | 6) {
                        kind6 += (g.kind & 0xff == 6) as usize;
                        assert!(!g.entities.iter().any(|e| enemy(e.object)), "{name} group {tag}: kind {} holds an enemy-class entity", g.kind & 0xff);
                    }
                }
            }
        }
        eprintln!("{areas} tt areas, {kind6} kind-6 groups, {triggers} triggers on events 2 or 3");
        assert!(areas > 30 && kind6 > 100);
        assert_eq!(triggers, 0);
    }

    /// Area script opcode 6 (`DAT_0035cd28`, `FUN_00180b48` case 6) on the real Twilight Town area scripts: which values the
    /// programs write, and the keep count `FUN_00181148` ends with for every map program id (0 above 0x32). Needs `KH2_ISO`.
    #[test]
    fn twilight_town_area_scripts_keep_values() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let (mut areas, mut writers, mut non_default, mut with_writer) = (0, 0, 0, Vec::new());
        for n in 0..=41 {
            let name = format!("tt{n:02}");
            let Some(ard) = disc.read(&format!("ard/{name}.ard")).and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a) else { continue };
            areas += 1;
            let mut values = Vec::new();
            for (script, progs) in &ard.scripts {
                for p in progs {
                    for c in p.commands.iter().filter(|c| c.opcode == 6) {
                        writers += 1;
                        values.push((script.clone(), p.id, c.args.first().copied().unwrap_or(0)));
                    }
                }
            }
            let mut keeps = std::collections::BTreeSet::new();
            let map_ids: Vec<u16> = ard.scripts.get("map").map(|ps| ps.iter().map(|p| p.id).collect()).unwrap_or_default();
            for id in map_ids.iter().copied().chain([0xffu16]).filter(|i| *i <= 0xff) {
                let run = kh2_ard::run::run_area(&ard, [id as u8, 0, 0], 0);
                keeps.insert(kh2_ard::run::keep_for_program(&run, id as u8));
            }
            {
                let run = kh2_ard::run::run_area(&ard, [0, 0, 0], 0);
                assert_eq!((run.keep, kh2_ard::run::keep_for_program(&run, 0x32), kh2_ard::run::keep_for_program(&run, 0x33)), (3, 3, 0), "{name}");
            }
            // Every (map, btl) pair that has an opcode 6 writer: the keep count the area ends with.
            for (script, id, v) in &values {
                let programs = if script == "btl" { [0, *id as u8, 0] } else { [*id as u8, 0, 0] };
                let run = kh2_ard::run::run_area(&ard, programs, 0);
                assert_eq!(run.keep, *v as i32, "{name} {script} program {id}");
                with_writer.push(format!("{name} {script} {id} -> {}", run.keep));
            }
            non_default += (!values.is_empty()) as usize;
            eprintln!("{name}: opcode 6 writes {values:?}; keep over map programs {keeps:?}");
        }
        eprintln!("{areas} tt areas, {writers} opcode 6 commands, {non_default} areas with one: {with_writer:?}");
        assert!(areas > 30);
        // Printed table: only tt07 (btl 21, 22) and tt33 (btl 1) write opcode 6, each with 0; every other program leaves the
        // default 3, and a map program byte above 0x32 gives 0 whatever the script says.
        assert_eq!((writers, non_default), (3, 2));
        assert_eq!(with_writer, ["tt07 btl 21 -> 0", "tt07 btl 22 -> 0", "tt33 btl 1 -> 0"]);
    }

    /// `FUN_001689c0`: a change of world resets the tables; a move inside the world does not. The world is the area's
    /// two-letter prefix [inferred].
    #[test]
    fn a_change_of_world_clears_the_tables_of_the_defeated() {
        let mut t = DefeatedTables::default();
        t.enter_world(*b"tt");
        t.enter(4, 3);
        t.sync(&[1, 2], &[1]);
        t.enter_world(*b"tt");
        t.enter(4, 3);
        assert_eq!(t.serials(), [1], "same world: kept");
        t.enter_world(*b"hb");
        t.enter(4, 3);
        assert!(t.serials().is_empty(), "other world: cleared");
    }

    /// `FUN_00194188` on the table at 0x35d278: an area visited again within `keep` area starts finds its slot and
    /// the serials entered in it; with keep 0, after the window or after four other areas it comes up cleared.
    #[test]
    fn the_table_of_the_defeated_outlives_the_area_for_a_few_area_starts() {
        let mut t = DefeatedTables::default();
        t.enter(4, 3);
        t.sync(&[1, 2], &[1]);
        assert_eq!(t.serials(), [1]);
        t.enter(5, 3);
        assert!(t.serials().is_empty(), "another area has its own slot");
        t.enter(4, 3);
        assert_eq!(t.serials(), [1], "area 4 again, one area start later: its slot is found with its entries");
        t.sync(&[1, 2], &[2]);
        assert_eq!(t.serials(), [2], "a member taken out of the killed list leaves the table");

        let mut t = DefeatedTables::default();
        t.enter(4, 0);
        t.sync(&[1], &[1]);
        t.enter(4, 0);
        assert!(t.serials().is_empty(), "keep 0 (map program above 0x32): never found again");

        let mut t = DefeatedTables::default();
        t.enter(4, 3);
        t.sync(&[1], &[1]);
        for a in [5, 6, 7] {
            t.enter(a, 3);
        }
        t.enter(4, 3);
        assert!(t.serials().is_empty(), "three area starts later the age window is over (counter 4 - 3 > age 0)");

        let mut t = DefeatedTables::default();
        t.enter(4, 3);
        t.sync(&[1], &[1]);
        for a in [5, 6, 7, 8] {
            t.enter(a, 3);
        }
        t.enter(4, 3);
        assert!(t.serials().is_empty(), "the fifth area took the least recent of the four slots");

        let mut t = DefeatedTables::default();
        t.enter(4, 3);
        t.sync(&[1], &[1]);
        t.clear();
        t.enter(4, 3);
        assert!(t.serials().is_empty(), "FUN_00194330 cleared every slot");
    }

    /// The table holds 256 serials (`FUN_00194080` writes halfword indices 0..=255): a full table takes no more.
    #[test]
    fn the_table_of_the_defeated_holds_256_serials() {
        let mut t = DefeatedTables::default();
        t.enter(4, 3);
        let all: Vec<u16> = (1..=300).collect();
        t.sync(&all, &all);
        assert_eq!(t.serials().len(), 256);
    }
}
