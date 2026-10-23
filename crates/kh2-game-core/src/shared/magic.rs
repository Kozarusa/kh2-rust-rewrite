
//! The spell state the lower crates read (moved out of `magic.rs`, which re-exports it).

use std::collections::HashMap;

use bevy::prelude::*;

use crate::magic_rules::Charge;

/// `atkp` id of Fire's attack objects (FIRE_*.mag event 21: `attack_create(ctx, 502, -1, kind)`).
pub const ATKP_FIRE: u16 = 502;

/// Ticks a cast request waits for Sora to accept it (app rule: the game keeps a pending command
/// and retries it while the playing motion has no window, the retry limit was not traced).
pub const REQUEST_TICKS: u32 = 20;

pub fn log_on() -> bool {
    log_level() > 0
}

/// `KH2_MAGIC_LOG`: 1 the casts, MP and hits; 2 also every live attack each tick (the timeline the
/// hand port and the interpreter are compared by).
pub fn log_level() -> u32 {
    static ON: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_MAGIC_LOG").map_or(0, |v| v.parse().unwrap_or(1)))
}

/// The spell state other systems read and write: the MP charge (abilities, drive, items, the party, the
/// prizes, the save point, the summons, the menus) and the command the menu issued. The rest of the
/// casting state is `magic::MagicRun`, which only the spell system touches.
#[derive(Resource, Default)]
pub struct MagicState {
    pub charge: Charge,
    /// A spell command waiting for Sora (command id, ticks left).
    pub request: Option<(u16, u32)>,
}

/// The spell attack objects as the victims' side reads them (enemy.rs: the pull of reaction class 5,
/// which follows the attack object the victim keeps at +0xaac). Written by `run` every tick.
#[derive(Resource, Default)]
pub struct SpellAttacks {
    /// The position of every live attack object (method +8), by group, Bevy space. A group that is
    /// gone has been finished (FUN_0018ccf8 clears the victims' +0xaac then).
    pub places: HashMap<u32, Vec3>,
    /// This tick's spell hits: (victim, atkp, attack group).
    pub hits: Vec<(Entity, i16, u32)>,
    /// Each enemy's body centre (FUN_0016b7d0: the centre of its first shape of type 1, else its
    /// position) relative to its root, Bevy space.
    pub centres: HashMap<Entity, Vec3>,
}

impl SpellAttacks {
    /// The attack object behind this tick's spell hit on `target` with `atkp`.
    pub fn hit_group(&self, target: Entity, atkp: i16) -> Option<u32> {
        self.hits.iter().find(|h| h.0 == target && h.1 == atkp).map(|h| h.2)
    }
}

impl MagicState {
    /// Menu glue (WP-7C): the command menu (`items.rs`, FUN_001bc0c8) issued a spell command; Sora
    /// takes it when a window lets him (see `run`). The menu only issues commands whose status is
    /// below 4 (FUN_001bb930).
    pub fn request_cast(&mut self, cmd: u16) {
        self.request = Some((cmd, REQUEST_TICKS));
        if log_on() {
            info!("magic: menu: cast command {cmd} selected");
        }
    }
}
