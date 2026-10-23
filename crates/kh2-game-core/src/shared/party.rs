//! The party data the lower crates read (moved out of `friend_ai.rs` and `party_battle.rs`, which re-export it).

use std::collections::HashMap;

use bevy::prelude::*;

/// The members' live attack objects as the class-5 pull (FUN_00185668 -> FUN_001bf920, Goofy's `tornado`
/// atkp 1188) needs them, like `magic::SpellAttacks` for the spells: the victim keeps the attack object
/// (+0xaac) and is pulled toward its position until it is finished (FUN_0018ccf8).
#[derive(Resource, Default)]
pub struct MemberAttacks {
    /// The position of every live attack object of the members by key (`MemberAttacks::key`), Bevy space.
    pub places: HashMap<u32, Vec3>,
    /// This tick's hits of the members' attack objects: (victim, atkp, key).
    pub hits: Vec<(Entity, i16, u32)>,
}

impl MemberAttacks {
    /// The key of a member's attack object: bit 31 (apart from the spells' hit groups), the slot and the
    /// object's handle.
    pub fn key(slot: u8, handle: u32) -> u32 {
        0x8000_0000 | (slot as u32) << 24 | (handle & 0x00ff_ffff)
    }

    /// The attack object behind this tick's hit on `target` with `atkp`.
    pub fn hit_group(&self, target: Entity, atkp: i16) -> Option<u32> {
        self.hits.iter().find(|h| h.0 == target && h.1 == atkp).map(|h| h.2)
    }
}

/// What `FUN_00186238` (the HP method of a party object, +0x588 bit 0x4000 set, through FUN_00176c98)
/// did with a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HpChange {
    /// The KO bit was cleared by a heal (FUN_0017cee8): motion 0x37 when the member lay in 0x36.
    pub revived: bool,
    /// HP reached 0 by this change (FUN_00186530 -> the KO method +0x5c, FUN_0017ce60).
    pub knocked_out: bool,
    /// The amount applied (after FUN_0017cee8's scale); 0 when the change was refused.
    pub applied: i32,
}

/// `FUN_00186238` for a party object on its HP (member index 0): no change at a zero maximum; a heal
/// (`delta >= 1`) on a KO'd object first revives it (FUN_0017cee8: the KO bit cleared, the amount
/// scaled by `revive_scale`, which is 1.0 (`DAT_0037a560`) with the ability block's +0x10 bit 8 and
/// `sstm` +0x54 without); while KO'd nothing else changes; otherwise FUN_001a0100 adds the amount held
/// between the minimum (+8) and the maximum (+4), and an HP of 0 knocks the object out (FUN_00186530,
/// only when the KO bit is clear).
pub fn hp_change(hp: &mut i32, max: i32, min: i32, ko: &mut bool, delta: i32, revive_scale: f32) -> HpChange {
    let mut out = HpChange::default();
    if max == 0 {
        return out;
    }
    let mut delta = delta;
    if delta >= 1 && *ko {
        *ko = false;
        out.revived = true;
        delta = (delta as f32 * revive_scale) as i32;
    }
    if *ko {
        return out;
    }
    let sum = *hp + delta;
    let new = if sum < min { min } else if sum > max { max } else { sum };
    out.applied = new - *hp;
    *hp = new;
    if new == 0 {
        *ko = true;
        out.knocked_out = true;
    }
    out
}
