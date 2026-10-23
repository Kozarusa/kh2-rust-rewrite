//! The party data the lower crates read (moved out of `friend_ai.rs` and `party_battle.rs`, which re-export it).

use std::collections::HashMap;

use bevy::prelude::*;
use kh2_tables::tables as t;

use crate::assets::Disc;

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

/// The party member's NeoStatus (`PartyMember::neo`) on the member's root entity, for the crates below the party
/// crate (the enemy's hit on a member's attack reads whose hit it was). Inserted with the `PartyMember` component
/// by `party::spawn_party` and never changed.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemberNeo(pub u16);

/// The tables a member's status is rebuilt from (`lvup` of 00battle.bin, `item` and `item_stat`), loaded once with
/// the party data (`PartyData` holds the same value) and inserted as a resource for the crates below the party crate.
#[derive(Resource, Clone, Default)]
pub struct MemberStatus {
    /// `lvup` (00battle.bin): the level tables by character (FUN_001b50c0), the first Sora's.
    pub lvup: Vec<Vec<t::Lvup>>,
    pub items: Vec<t::Item>,
    /// `item_stat` rows (a weapon's attack and magic, `Item::stat_entry`).
    pub stats: Vec<t::ItemStat>,
}

impl MemberStatus {
    /// Reads `lvup`, `item` and `item_stat` from the disc (empty tables when unreadable).
    pub fn load(disc: &Disc) -> MemberStatus {
        let lvup = match disc.0.lock() {
            Ok(mut a) => kh2_tables::subfile(&mut a, &t::LVUP).ok().and_then(|b| kh2_tables::rt::decode_lvup::<t::Lvup>(&b)).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        let (items, stats) = match disc.0.lock() {
            Ok(mut a) => (
                kh2_tables::load::<t::Item>(&mut a, &t::ITEM).map(|d| d.rows).unwrap_or_default(),
                kh2_tables::load::<t::ItemStat>(&mut a, &t::ITEM_STAT).map(|d| d.rows).unwrap_or_default(),
            ),
            Err(_) => (Vec::new(), Vec::new()),
        };
        MemberStatus { lvup, items, stats }
    }

    /// A member's status numbers +0x188 (strength), +0x18a (magic), +0x18c (defense) as FUN_0019f920
    /// rebuilds them from the saved record (`PartyData::member_status` names the rule).
    pub fn member_status(&self, neo: u16, rec: &crate::progress::Member) -> (i32, i32, i32) {
        let row = self.lvup_row(neo, rec.level);
        let (mut s, mut m, mut d) = row.map_or((0, 0, 0), |r| (r.strength as i32, r.magic as i32, r.defense as i32));
        s += rec.strength as i32;
        m += rec.magic as i32;
        d += rec.defense as i32;
        for item in rec.weapons.iter().chain(&rec.armor).chain(&rec.accessories).filter(|&&i| i != 0) {
            if let Some(st) = self.items.iter().find(|i| i.id == *item).and_then(|i| self.stats.iter().find(|x| x.id == i.stat_entry)) {
                s += st.attack as i32;
                m += st.magic as i32;
                d += st.defense as i32;
            }
        }
        (s, m, d)
    }

    /// The `lvup` row `FUN_001b50c0` picks for a member (table by `FUN_001b58d8`, row of the level held at the table's end).
    pub fn lvup_row(&self, neo: u16, level: u8) -> Option<&t::Lvup> {
        let k = match neo {
            14 => 1,
            15 => 6,
            n @ 1..=13 => n as usize,
            _ => 1,
        };
        let table = self.lvup.get(k - 1).filter(|t| !t.is_empty()).or_else(|| self.lvup.first());
        table.and_then(|t| t.get((level.max(1) as usize).min(t.len()) - 1))
    }

    /// The AP and resistances of a member's status block (`PartyData::member_ap_resistances` names the rule).
    pub fn member_ap_resistances(&self, neo: u16, rec: &crate::progress::Member) -> (i32, [i32; 7]) {
        let mut ap = self.lvup_row(neo, rec.level).map_or(0, |r| i32::from(r.ap)) + i32::from(rec.ap);
        let rows: Vec<[i32; 8]> = rec
            .weapons
            .iter()
            .chain(&rec.armor)
            .chain(&rec.accessories)
            .filter(|&&i| i != 0)
            .filter_map(|item| self.items.iter().find(|i| i.id == *item).and_then(|i| self.stats.iter().find(|x| x.id == i.stat_entry)))
            .map(|s| [s.ability_points, s.unknown08, s.fire_resistance, s.ice_resistance, s.lightning_resistance, s.dark_resistance, s.unknown0d, s.general_resistance].map(i32::from))
            .collect();
        ap += rows.iter().map(|r| r[0]).sum::<i32>();
        (ap, crate::data::fold_resistances(rows.iter()))
    }
}
