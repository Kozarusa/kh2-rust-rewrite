//! The enemy component, its kind and state, the script brain and the collision-group queues the lower crates read (moved out of `enemy.rs`, `enemy_ai.rs` and `enemy_host.rs`, which re-export them).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use bevy::prelude::*;
use kh2_bdx::Runtime;

use crate::data::Stats;
use crate::reaction_rules::{self as rules, Blow, Push};
use crate::shared::enemy_obj::{Obj, PLAYER};

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

/// The motion set a model's `00objentry` row names (`AnimationName`, without ".mset"); the model's own name when the
/// row is missing or names nothing. `M_EX610_RAW` names `M_EX610.mset` and has no `obj/M_EX610_RAW.mset`.
pub fn mset_name(animation_name: Option<&str>, model: &str) -> String {
    animation_name.map(|n| n.trim_end_matches(".mset")).filter(|n| !n.is_empty()).unwrap_or(model).to_string()
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
    pub push: Option<Push>,
    /// Flight of the blow that put the enemy in `EState::Blown`, and whether it came from behind.
    pub blow: Option<Blow>,
    pub blown_back: bool,
    /// Reaction class 5 (Magnet): the spell attack object the enemy is drawn toward (its group in
    /// `magic::SpellAttacks`, actor +0xaac) and the pull's numbers (`reaction_rules::Pull`).
    pub pull: Option<(u32, rules::Pull)>,
    /// Hit sounds played so far; the sound alternates between two entries (actor byte +0x7fc).
    pub hit_sounds: u8,
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

/// The BDX runtime of one enemy. Its presence hands the enemy's behaviour to the script.
#[derive(Component)]
pub struct AiBrain {
    pub rt: Runtime,
    pub obj: Obj,
    pub tag: String,
    pub enmp: u16,
    pub flags124: bool,
    pub paired: Arc<HashMap<usize, Vec<(String, f32)>>>,
    pub player_model: u8,
    pub started: bool,
    pub dead_sent: bool,
    pub errors_seen: usize,
    /// Animator frame after the previous tick (a looping motion wrapped when it goes down).
    pub last_frame: f32,
    pub in_air: bool,
    pub visible: bool,
    pub applied_alpha: f32,
    pub original_modes: HashMap<AssetId<StandardMaterial>, AlphaMode>,
}

impl AiBrain {
    /// The `enmp` row the enemy's status block stands on: its kind's row, or the one the script's `set_obj_param`
    /// (host 2.9) rebuilt it from. The defeat payout reads it.
    pub fn enmp(&self) -> u16 {
        self.enmp
    }
}

/// The bytes of the object's own target record (host 2.23, `TARGET`): the player, position index 0
/// [inferred], 0, -1, 0 (the layout of FUN_0018ed40 + FUN_0018f490).
pub fn target_record() -> [u8; 20] {
    let mut r = [0u8; 20];
    r[0..4].copy_from_slice(&PLAYER.to_le_bytes());
    r[12..16].copy_from_slice(&(-1i32).to_le_bytes());
    r
}

/// Collision group switches the scripts asked for (hosts 1.36 and 1.37), in call order: (group, on). `area::run_area_script` drains them
/// into `World::enabled_groups`.
pub static GROUP_EDITS: LazyLock<Mutex<Vec<(u8, bool)>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Queues one switch of a collision mesh group (`FUN_0015e9a8` when `on`, `FUN_0015e928` otherwise). Bits above 63 are not in the 64-bit mask.
pub fn queue_group_edit(group: u32, on: bool) {
    push_group_edit(&mut GROUP_EDITS.lock().unwrap_or_else(|e| e.into_inner()), group, on);
}

pub fn push_group_edit(queue: &mut Vec<(u8, bool)>, group: u32, on: bool) {
    if group < 64 {
        queue.push((group as u8, on));
    }
}

/// The area mask `DAT_0035cd18` (area opcode 0xb) the parameterless hosts 1.46 / 1.47 walk; `area::run_area_script` stores the loaded area's.
pub static AREA_MASK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn set_area_mask(word: u64) {
    AREA_MASK.store(word, std::sync::atomic::Ordering::Relaxed);
}

/// Queues the switch of every set bit of `word`, lowest bit first (`FUN_0019d4c0` / `FUN_0019d520` loop 0 to 63).
pub fn push_mask_edits(queue: &mut Vec<(u8, bool)>, word: u64, on: bool) {
    (0..64u32).filter(|b| word >> b & 1 != 0).for_each(|b| push_group_edit(queue, b, on));
}

/// The queued switches, oldest first; the queue is empty afterwards.
pub fn take_group_edits() -> Vec<(u8, bool)> {
    std::mem::take(&mut *GROUP_EDITS.lock().unwrap_or_else(|e| e.into_inner()))
}
