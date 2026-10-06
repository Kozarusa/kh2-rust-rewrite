//! Sora's abilities: the set he has equipped and the numbers the game derives from it.
//!
//! The game keeps an "ability block" in the character's battle status (`status + 0x1cc`): a bit set
//! of the applied abilities plus the stat fields that some abilities change (combo length, boosts,
//! drain multipliers, ...). `FUN_001c0d88` resets it, `FUN_001c0e50` applies the equipped abilities
//! and `FUN_001c0f40` applies one; `FUN_001c1340` derives the growth levels. `Block` is that block
//! and `Block::apply` is `FUN_001c0f40`. Everything is read from the disc: the ability rows (`item`
//! type 19), the equipment rows (`item_stat`), the step sizes (`sstm`) and the AP curve (`lvup`).
//! `docs/abilities.md` has the verified facts, the API for `sora.rs` / `drive.rs` and the gaps.
//!
//! Environment: `KH2_ABILITIES=bit,bit,...` (ability bits, as the attack table gates them: each is
//! equipped through the ability row that owns the bit, so `21` really adds a combo hit),
//! `KH2_ABILITY_IDS=id,...` (item ids), `KH2_ABILITY_START=data` (no Guard baseline, see the doc),
//! `KH2_ABILITYLOG=1` (log the active abilities and the stat block deltas).
//!
//! Readers: the controller's view (`sora::Abilities`, `sora::Growth`) is written by
//! `sync_controller` whenever the block changes; the drive code sets and clears the form list
//! and reads `form_drain`; `attacker_bonuses` is the damage routine's use of the boosts.

// The menu queries (`ap_used`, `ap_max`, `equipment_stats`) wait for a menu (docs/abilities.md).
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use kh2_archive::Archive;
use kh2_tables::player::{self, Sstm};
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::data::KINGDOM_KEY;
use crate::schedule::Sim;
use crate::sora::{self, Growth, GrowthOverride};

/// Ability bits the damage routine tests in the block's fourth word (`FUN_001a0aa8`: `+0x1d8`
/// bits 0x40, 0x80, 0x100): Combo Boost, Air Combo Boost, Reaction Boost. The ability rows own
/// these bits (checked against the disc in the tests).
pub const BIT_COMBO_BOOST: u16 = 102;
pub const BIT_AIR_COMBO_BOOST: u16 = 103;
pub const BIT_REACTION_BOOST: u16 = 104;

/// The two steps of the damage routine that read the attacker's ability block, after the
/// generic formula and Anti's multiplier (`FUN_001a0610` calls them last):
///
/// - `FUN_001a0a18`: the attack row's `Element` 1, 2, 3 (Fire, Blizzard, Thunder) scales the
///   damage by the block's percent for that element (`+0x40`, `+0x42`, `+0x44`), rounded up;
///   other elements are unchanged.
/// - `FUN_001a0aa8`: with the Combo Boost bit and the row's `Kind` bit 0, the damage grows by
///   `combo_boost` percent per combo hit (`FUN_00176be8`: the actor's hit counter `+0xb04`
///   capped at the block's ground length); else with the Air Combo Boost bit and `Kind` bit 1
///   the same with the air length (`FUN_00176c40`); else with the Reaction Boost bit and `Kind`
///   bit 2 by `reaction_boost` percent. Only the first matching case applies. Each addition is
///   rounded up.
///
/// `hits` is Sora's combo counter at the time of the hit.
pub fn attacker_bonuses(block: &Block, hits: u8, atk: &t::Atkp, dmg: i32) -> i32 {
    let pct = |d: i32, p: i32| (d * p + 99) / 100;
    let dmg = match atk.element {
        1 => pct(dmg, block.fire as i32),
        2 => pct(dmg, block.blizzard as i32),
        3 => pct(dmg, block.thunder as i32),
        _ => dmg,
    };
    let kind = atk.kind;
    let capped = |max: i8| if (hits as i64) < max as i64 { hits as i32 } else { max as i32 };
    if block.bit(BIT_COMBO_BOOST) && kind & 1 != 0 {
        dmg + pct(dmg, block.combo_boost as i32 * capped(block.ground_combo))
    } else if block.bit(BIT_AIR_COMBO_BOOST) && kind >> 1 & 1 != 0 {
        dmg + pct(dmg, block.air_combo_boost as i32 * capped(block.air_combo))
    } else if block.bit(BIT_REACTION_BOOST) && kind >> 2 & 1 != 0 {
        dmg + pct(dmg, block.reaction_boost as i32)
    } else {
        dmg
    }
}

/// Ability ids that `FUN_001c0f40` has a stat effect for (item ids of type 19).
pub mod id {
    pub const GUARD: u16 = 82;
    pub const COMBO_PLUS: u16 = 162;
    pub const AIR_COMBO_PLUS: u16 = 163;
    pub const COMBO_BOOST: u16 = 390;
    pub const AIR_COMBO_BOOST: u16 = 391;
    pub const REACTION_BOOST: u16 = 392;
    pub const FINISHING_PLUS: u16 = 393;
    pub const NEGATIVE_COMBO: u16 = 394;
    pub const BERSERK_CHARGE: u16 = 395;
    pub const DAMAGE_DRIVE: u16 = 396;
    pub const DRIVE_BOOST: u16 = 397;
    pub const FORM_BOOST: u16 = 398;
    pub const SUMMON_BOOST: u16 = 399;
    pub const COMBINATION_BOOST: u16 = 400;
    pub const EXPERIENCE_BOOST: u16 = 401;
    pub const DRAW: u16 = 405;
    pub const JACKPOT: u16 = 406;
    pub const LUCKY_LUCKY: u16 = 407;
    pub const FIRE_BOOST: u16 = 408;
    pub const BLIZZARD_BOOST: u16 = 409;
    pub const THUNDER_BOOST: u16 = 410;
    pub const ITEM_BOOST: u16 = 411;
    pub const MP_RAGE: u16 = 412;
    pub const MP_HASTE: u16 = 413;
    pub const DEFENDER: u16 = 414;
    pub const HYPER_HEALING: u16 = 419;
    pub const MP_HASTERA: u16 = 421;
    pub const MP_HASTEGA: u16 = 422;
}

/// The `sstm` fields the stat effects read (byte offsets in the tuning record, `FUN_001c0f40`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tuning {
    pub berserk_charge: i16,
    pub mp_haste: f32,
    pub mp_hastera: f32,
    pub mp_hastega: f32,
    pub draw: f32,
    pub combo_boost: i16,
    pub reaction_boost: i16,
    pub damage_drive: f32,
    pub drive_boost: f32,
    pub form_boost: f32,
    pub defender: i8,
    pub element_boost: i16,
    pub mp_rage: f32,
    pub hyper_healing: f32,
    pub combination_boost: f32,
    pub jackpot: f32,
    pub lucky_lucky: f32,
    pub item_boost: i16,
    pub summon_boost: f32,
}

impl Tuning {
    pub fn from_sstm(s: &Sstm) -> Tuning {
        // The game reads the shorts and the char as the low bytes of the little-endian word.
        let s16 = |o| s.i32(o) as i16;
        Tuning {
            berserk_charge: s16(0x11c),
            mp_haste: s.f32(0x120),
            mp_hastera: s.f32(0x124),
            mp_hastega: s.f32(0x128),
            draw: s.f32(0x12c),
            combo_boost: s16(0x130),
            reaction_boost: s16(0x134),
            damage_drive: s.f32(0x138),
            drive_boost: s.f32(0x13c),
            form_boost: s.f32(0x140),
            defender: s.i32(0x148) as i8,
            element_boost: s16(0x14c),
            mp_rage: s.f32(0x150),
            hyper_healing: s.f32(0x154),
            combination_boost: s.f32(0x158),
            jackpot: s.f32(0x15c),
            lucky_lucky: s.f32(0x160),
            item_boost: s16(0x164),
            summon_boost: s.f32(0x16c),
        }
    }
}

/// What the rules need to know about one ability item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub id: u16,
    /// `Flag1 | Rank << 8`: the bit this ability sets in the block (and the attack table's gate).
    pub bit: u16,
    pub ap: u8,
    /// 0 support, 1 party only, 2 action, 3 growth (`StatEntry` low byte).
    pub kind: u8,
}

/// Equipment row (`item_stat`) as `FUN_001a0c08` uses it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EquipRow {
    pub ability: u16,
    pub strength: u8,
    pub magic: u8,
    pub defense: u8,
    pub ap: u8,
}

/// The ability block (`status + 0x1cc`) with the defaults of `FUN_001c0d88`.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// Bit set of applied abilities (bits 0..256).
    pub bits: [u32; 8],
    /// Growth levels 0..=4 for High Jump, Quick Run, Aerial Dodge, Glide (0 = not learned).
    pub growth: [u8; 4],
    pub ground_combo: i8,
    pub air_combo: i8,
    pub finishers: i8,
    pub exp_multiplier: i8,
    pub defense_bonus: i8,
    pub draw_radius: f32,
    pub mp_regen: f32,
    pub drive_gain_charging: f32,
    pub form_drain: f32,
    pub fire: u16,
    pub blizzard: u16,
    pub thunder: u16,
    pub combo_boost: u16,
    pub air_combo_boost: u16,
    pub reaction_boost: u16,
    pub item_boost: u16,
    pub berserk_charge: u16,
    pub damage_drive: f32,
    pub mp_rage: f32,
    pub hyper_healing: f32,
    pub limit_duration: f32,
    pub jackpot: f32,
    pub lucky_lucky: f32,
    pub summon_drain: f32,
}

impl Default for Block {
    fn default() -> Block {
        Block {
            bits: [0; 8],
            growth: [0; 4],
            ground_combo: 3,
            air_combo: 3,
            finishers: 1,
            exp_multiplier: 1,
            defense_bonus: 0,
            draw_radius: 0.0,
            mp_regen: 1.0,
            drive_gain_charging: 1.0,
            form_drain: 1.0,
            fire: 100,
            blizzard: 100,
            thunder: 100,
            combo_boost: 0,
            air_combo_boost: 0,
            reaction_boost: 0,
            item_boost: 0,
            berserk_charge: 0,
            damage_drive: 0.0,
            mp_rage: 0.0,
            hyper_healing: 1.0,
            limit_duration: 1.0,
            jackpot: 0.0,
            lucky_lucky: 0.0,
            summon_drain: 1.0,
        }
    }
}

impl Block {
    pub fn bit(&self, bit: u16) -> bool {
        bit < 256 && self.bits[bit as usize >> 5] >> (bit & 31) & 1 != 0
    }

    pub fn set_bit(&mut self, bit: u16) {
        if bit < 256 {
            self.bits[bit as usize >> 5] |= 1 << (bit & 31);
        }
    }

    /// `FUN_001c0f40(block, id)`: the stat effect of one ability, then its bit. `bit` is the bit of
    /// the ability's item row (`None` when the id has no row: the game then reads a null row).
    pub fn apply(&mut self, id: u16, bit: Option<u16>, t: &Tuning) {
        use self::id as a;
        match id {
            0 => return,
            a::COMBO_PLUS => self.ground_combo = self.ground_combo.wrapping_add(1),
            a::AIR_COMBO_PLUS => self.air_combo = self.air_combo.wrapping_add(1),
            a::FINISHING_PLUS => self.finishers = self.finishers.wrapping_add(1),
            a::NEGATIVE_COMBO => {
                self.ground_combo = self.ground_combo.wrapping_sub(1);
                self.air_combo = self.air_combo.wrapping_sub(1);
            }
            a::COMBO_BOOST => self.combo_boost = self.combo_boost.wrapping_add(t.combo_boost as u16),
            a::AIR_COMBO_BOOST => self.air_combo_boost = self.air_combo_boost.wrapping_add(t.combo_boost as u16),
            a::REACTION_BOOST => self.reaction_boost = self.reaction_boost.wrapping_add(t.reaction_boost as u16),
            a::BERSERK_CHARGE => self.berserk_charge = self.berserk_charge.wrapping_add(t.berserk_charge as u16),
            a::DAMAGE_DRIVE => self.damage_drive += t.damage_drive,
            a::DRIVE_BOOST => self.drive_gain_charging += t.drive_boost,
            a::FORM_BOOST => self.form_drain *= t.form_boost,
            a::SUMMON_BOOST => self.summon_drain *= t.summon_boost,
            a::COMBINATION_BOOST => self.limit_duration *= t.combination_boost,
            a::EXPERIENCE_BOOST => self.exp_multiplier = self.exp_multiplier.wrapping_add(1),
            a::DRAW => self.draw_radius += t.draw,
            a::JACKPOT => self.jackpot += t.jackpot,
            a::LUCKY_LUCKY => self.lucky_lucky += t.lucky_lucky,
            a::FIRE_BOOST => self.fire = self.fire.wrapping_add(t.element_boost as u16),
            a::BLIZZARD_BOOST => self.blizzard = self.blizzard.wrapping_add(t.element_boost as u16),
            a::THUNDER_BOOST => self.thunder = self.thunder.wrapping_add(t.element_boost as u16),
            a::ITEM_BOOST => self.item_boost = self.item_boost.wrapping_add(t.item_boost as u16),
            a::MP_RAGE => self.mp_rage += t.mp_rage,
            a::MP_HASTE => self.mp_regen += t.mp_haste,
            a::MP_HASTERA => self.mp_regen += t.mp_hastera,
            a::MP_HASTEGA => self.mp_regen += t.mp_hastega,
            a::DEFENDER => self.defense_bonus = self.defense_bonus.wrapping_add(t.defender),
            a::HYPER_HEALING => self.hyper_healing *= t.hyper_healing,
            _ => {}
        }
        if let Some(b) = bit {
            self.set_bit(b);
        }
    }

    /// Stat fields that differ from the defaults, as `name: default -> value` for the log.
    pub fn deltas(&self) -> Vec<String> {
        let d = Block::default();
        let mut out = Vec::new();
        macro_rules! diff {
            ($($f:ident),*) => {$(
                if self.$f != d.$f {
                    out.push(format!("{}: {} -> {}", stringify!($f), d.$f, self.$f));
                }
            )*};
        }
        diff!(
            ground_combo, air_combo, finishers, exp_multiplier, defense_bonus, draw_radius, mp_regen,
            drive_gain_charging, form_drain, fire, blizzard, thunder, combo_boost, air_combo_boost,
            reaction_boost, item_boost, berserk_charge, damage_drive, mp_rage, hyper_healing,
            limit_duration, jackpot, lucky_lucky, summon_drain
        );
        if self.growth != d.growth {
            out.push(format!("growth (High Jump, Quick Run, Aerial Dodge, Glide): {:?}", self.growth));
        }
        out
    }
}

/// Everything read from the disc that the rules need.
#[derive(Debug)]
pub struct AbilityRules {
    pub abilities: BTreeMap<u16, Info>,
    pub equip: BTreeMap<u16, EquipRow>,
    /// Item id -> `item_stat` row for equipment (`item.StatEntry`).
    pub items: BTreeMap<u16, u16>,
    pub tuning: Tuning,
    /// Ability bits of the growth abilities per group (High Jump, Quick Run, Aerial Dodge, Glide),
    /// level 1..=4 in order. Derived from the kind 3 abilities sorted by bit.
    pub growth_bits: [[u16; 4]; 4],
    /// Total AP of `lvup` row `level - 1` (cumulative).
    pub ap_by_level: Vec<u16>,
}

impl AbilityRules {
    pub fn load(a: &mut Archive) -> Option<AbilityRules> {
        let items = kh2_tables::load::<t::Item>(a, &t::ITEM).ok()?.rows;
        let stats = kh2_tables::load::<t::ItemStat>(a, &t::ITEM_STAT).ok()?.rows;
        let sstm = player::load_sstm(a).ok()?;
        let blob = kh2_tables::subfile(a, &t::LVUP).ok()?;
        let lvup = kh2_tables::rt::decode_lvup::<t::Lvup>(&blob).and_then(|c| c.into_iter().next()).unwrap_or_default();
        let abilities: BTreeMap<u16, Info> = player::abilities(&items)
            .into_iter()
            .map(|x| (x.id, Info { id: x.id, bit: x.bit, ap: x.ap_cost, kind: x.kind }))
            .collect();
        let equip = stats
            .iter()
            .map(|s| (s.id, EquipRow { ability: s.ability, strength: s.attack, magic: s.magic, defense: s.defense, ap: s.ability_points }))
            .collect();
        let items = items.iter().map(|i| (i.id, i.stat_entry)).collect();
        let mut bits: Vec<u16> = abilities.values().filter(|i| i.kind == 3).map(|i| i.bit).collect();
        bits.sort_unstable();
        bits.dedup();
        let mut growth_bits = [[0u16; 4]; 4];
        for (n, b) in bits.iter().take(16).enumerate() {
            growth_bits[n / 4][n % 4] = *b;
        }
        let ap_by_level = lvup.iter().map(|l| l.ap as u16).collect();
        Some(AbilityRules { abilities, equip, items, tuning: Tuning::from_sstm(&sstm), growth_bits, ap_by_level })
    }

    /// The ability item that owns an ability bit (the lowest id when several share it).
    pub fn id_of_bit(&self, bit: u16) -> Option<u16> {
        self.abilities.values().find(|i| i.bit == bit).map(|i| i.id)
    }

    /// The growth ability item of `group` (0 High Jump, 1 Quick Run, 2 Aerial Dodge, 3 Glide) at
    /// `level` 1..=4: what a drive form's chain (0x37c556) grants at its display level, found
    /// through the growth bits of the kind 3 rows.
    pub fn growth_ability(&self, group: usize, level: u8) -> Option<u16> {
        let bit = *self.growth_bits.get(group)?.get(level.checked_sub(1)? as usize)?;
        (bit != 0).then(|| self.id_of_bit(bit)).flatten()
    }

    /// `FUN_001c1340`: the highest growth level set per group (0 when none).
    fn growth_levels(&self, block: &Block) -> [u8; 4] {
        let mut out = [0u8; 4];
        for (g, levels) in self.growth_bits.iter().enumerate() {
            for (l, &bit) in levels.iter().enumerate() {
                if bit != 0 && block.bit(bit) {
                    out[g] = l as u8 + 1;
                }
            }
        }
        out
    }

    /// Item id of the equipment's own ability (`item_stat.Ability`), 0 if none.
    fn equipment_row(&self, item: u16) -> Option<&EquipRow> {
        self.items.get(&item).and_then(|e| self.equip.get(e))
    }
}

/// One entry of the 64-slot ability list: the ability and whether it is equipped (bit 15).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    pub id: u16,
    pub equipped: bool,
}

/// Sora's abilities and the block derived from them.
#[derive(Resource)]
pub struct Abilities {
    rules: Arc<AbilityRules>,
    /// The character's ability list (`party record + 0x54`, 64 slots).
    pub learned: Vec<Slot>,
    /// Equipped items: keyblade, armor, accessories. Each grants its `item_stat` ability.
    pub equipment: Vec<u16>,
    /// While a drive form is active: the form's own ability list (`FUN_001c0e50`, 16 slots). Then
    /// only the character's support abilities (kind 0) still apply.
    form: Option<Vec<u16>>,
    /// Ability bits set directly (`KH2_ABILITIES` bits that no ability row owns).
    raw_bits: Vec<u16>,
    pub level: usize,
    block: Block,
}

impl Abilities {
    pub fn new(rules: Arc<AbilityRules>) -> Abilities {
        let mut s = Abilities { rules, learned: Vec::new(), equipment: vec![KINGDOM_KEY], form: None, raw_bits: Vec::new(), level: 1, block: Block::default() };
        s.recompute();
        s
    }

    /// The start of the story as far as the data goes, plus the app baseline (Guard equipped) and the
    /// environment overrides (see the module doc).
    pub fn with_defaults(rules: Arc<AbilityRules>) -> Abilities {
        let mut s = Abilities::new(rules);
        if std::env::var("KH2_ABILITY_START").as_deref() != Ok("data") {
            s.equip_ability(id::GUARD);
        }
        let list = |name: &str| -> Vec<u32> {
            std::env::var(name).unwrap_or_default().split(',').filter_map(|v| v.trim().parse().ok()).collect()
        };
        for id in list("KH2_ABILITY_IDS") {
            s.equip_ability(id as u16);
        }
        for bit in list("KH2_ABILITIES") {
            match s.rules.id_of_bit(bit as u16) {
                Some(id) => s.equip_ability(id),
                None => s.raw_bits.push(bit as u16),
            }
        }
        s.recompute();
        s
    }

    pub fn rules(&self) -> &AbilityRules {
        &self.rules
    }

    /// Learns and equips an ability (a second copy is a second slot, as in the game).
    pub fn equip_ability(&mut self, id: u16) {
        if self.learned.len() < 64 {
            self.learned.push(Slot { id, equipped: true });
            self.recompute();
        }
    }

    pub fn unequip_ability(&mut self, id: u16) {
        if let Some(s) = self.learned.iter_mut().find(|s| s.id == id && s.equipped) {
            s.equipped = false;
            self.recompute();
        }
    }

    /// Drive forms: `Some(list)` while a form is active (the form's granted abilities, including its
    /// growth ability), `None` on revert.
    pub fn set_form(&mut self, abilities: Option<Vec<u16>>) {
        self.form = abilities;
        self.recompute();
    }

    /// A form record is active (`FUN_001c0e50` then applies only the player's support abilities).
    pub fn in_form(&self) -> bool {
        self.form.is_some()
    }

    /// The form's list as set (for logs).
    pub fn form_list(&self) -> &[u16] {
        self.form.as_deref().unwrap_or(&[])
    }

    /// `FUN_0019f920` + `FUN_001c0e50`: rebuild the block from equipment, the ability list and the form.
    pub fn recompute(&mut self) {
        let r = self.rules.clone();
        let mut b = Block::default();
        let bit_of = |id: u16| r.abilities.get(&id).map(|i| i.bit);
        for item in &self.equipment {
            if let Some(e) = r.equipment_row(*item) {
                b.apply(e.ability, bit_of(e.ability), &r.tuning);
            }
        }
        let in_form = self.form.is_some();
        for s in self.learned.iter().filter(|s| s.equipped) {
            if !in_form || r.abilities.get(&s.id).is_none_or(|i| i.kind == 0) {
                b.apply(s.id, bit_of(s.id), &r.tuning);
            }
        }
        for id in self.form.iter().flatten() {
            b.apply(*id, bit_of(*id), &r.tuning);
        }
        b.growth = r.growth_levels(&b);
        for bit in &self.raw_bits {
            b.set_bit(*bit);
        }
        self.block = b;
    }

    pub fn block(&self) -> &Block {
        &self.block
    }

    /// Is the ability bit set (the attack table's gate; bit 0 means no gate).
    pub fn has_bit(&self, bit: u16) -> bool {
        bit == 0 || self.block.bit(bit)
    }

    /// Is this ability applied (by item id).
    pub fn has(&self, id: u16) -> bool {
        self.rules.abilities.get(&id).is_some_and(|i| self.block.bit(i.bit))
    }

    /// The low 128 ability bits in the format of `Sora::abilities` / `moveset::Chain::abilities`.
    pub fn bits(&self) -> u128 {
        (0..4).fold(0u128, |m, w| m | (self.block.bits[w] as u128) << (32 * w))
    }

    /// Attacks before the finisher on the ground (`+0x28`), never below 0.
    pub fn ground_combo(&self) -> u8 {
        self.block.ground_combo.max(0) as u8
    }

    /// Attacks before the finisher in the air (`+0x29`).
    pub fn air_combo(&self) -> u8 {
        self.block.air_combo.max(0) as u8
    }

    /// Finishers per chain (`+0x2a`).
    pub fn finishers(&self) -> u8 {
        self.block.finishers.max(0) as u8
    }

    /// Growth level 0..=4 of a group: 0 High Jump, 1 Quick Run, 2 Aerial Dodge, 3 Glide.
    pub fn growth(&self, group: usize) -> u8 {
        self.block.growth.get(group).copied().unwrap_or(0)
    }

    /// Total AP: the `lvup` curve at the level plus the equipment (`status + 0x18e`).
    pub fn ap_max(&self) -> u16 {
        let base = self.rules.ap_by_level.get(self.level.saturating_sub(1)).copied().unwrap_or(0);
        let gear: u16 = self.equipment.iter().filter_map(|i| self.rules.equipment_row(*i)).map(|e| e.ap as u16).sum();
        base + gear
    }

    /// AP spent by the equipped abilities (the cost is the high byte of `StatEntry`; that the menu
    /// sums them this way is inferred).
    pub fn ap_used(&self) -> u16 {
        self.learned.iter().filter(|s| s.equipped).filter_map(|s| self.rules.abilities.get(&s.id)).map(|i| i.ap as u16).sum()
    }

    /// Strength, magic and defense the equipment adds (`FUN_001a0c08`).
    pub fn equipment_stats(&self) -> (i32, i32, i32) {
        self.equipment.iter().filter_map(|i| self.rules.equipment_row(*i)).fold((0, 0, 0), |a, e| {
            (a.0 + e.strength as i32, a.1 + e.magic as i32, a.2 + e.defense as i32)
        })
    }

    fn name(&self, id: u16) -> String {
        match self.rules.abilities.get(&id) {
            Some(i) => format!("{id}(bit {}, AP {})", i.bit, i.ap),
            None => id.to_string(),
        }
    }

    /// The `KH2_ABILITYLOG` report.
    pub fn report(&self) -> String {
        let learned: Vec<String> = self.learned.iter().map(|s| format!("{}{}", self.name(s.id), if s.equipped { "" } else { " (not equipped)" })).collect();
        let gear: Vec<String> = self
            .equipment
            .iter()
            .map(|i| match self.rules.equipment_row(*i) {
                Some(e) if e.ability != 0 => format!("item {i} grants {}", self.name(e.ability)),
                _ => format!("item {i}"),
            })
            .collect();
        let d = self.block.deltas();
        format!(
            "abilities: equipment [{}]; learned [{}]; form list {:?}; raw bits {:?}; AP {}/{}; combo {}+finisher x{}, air {}; growth {:?}; stat block {}",
            gear.join(", "),
            learned.join(", "),
            self.form,
            self.raw_bits,
            self.ap_used(),
            self.ap_max(),
            self.ground_combo(),
            self.finishers(),
            self.air_combo(),
            self.block.growth,
            if d.is_empty() { "unchanged".to_string() } else { d.join("; ") },
        )
    }
}

fn log_on() -> bool {
    std::env::var_os("KH2_ABILITYLOG").is_some()
}

fn log_changes(a: Res<Abilities>) {
    if log_on() {
        info!("{}", a.report());
    }
}

/// The growth levels of the block as the controller's resource.
pub fn growth_of(a: &Abilities) -> Growth {
    Growth { high_jump: a.growth(0), quick_run: a.growth(1), aerial_dodge: a.growth(2), glide: a.growth(3) }
}

/// Writes the block into the controller's view (`sora::Abilities`: the ability bits and the combo
/// lengths) and the growth levels (`sora::Growth`) whenever the abilities change. In a form the
/// block's growth is the form's own ability; outside one `KH2_GROWTH` overrides it. Runs between
/// the drive code's swap (`Sim::Weapon`) and the controller (`Sim::Sora`), so a form's list applies
/// on the tick the body is swapped.
fn sync_controller(a: Res<Abilities>, over: Option<Res<GrowthOverride>>, mut view: ResMut<sora::Abilities>, growth: Option<ResMut<Growth>>) {
    *view = sora::Abilities::from_block(&a);
    let from_block = growth_of(&a);
    let next = match over.and_then(|o| o.0) {
        Some(o) if !a.in_form() => o,
        _ => from_block,
    };
    if let Some(mut g) = growth
        && *g != next
    {
        *g = next;
    }
    if log_on() {
        info!("abilities: controller view: combo {}+finisher x{}, air {}, growth {:?}, bits {:#x}", view.max_ground, view.max_finishers, view.max_air, next, view.bits);
    }
}

/// Registers `Abilities`. Needs `Disc`, so add it after `AssetsPlugin`.
pub struct AbilitiesPlugin;

impl Plugin for AbilitiesPlugin {
    fn build(&self, app: &mut App) {
        let rules = {
            let disc = app.world().resource::<Disc>();
            let mut a = disc.0.lock().unwrap();
            AbilityRules::load(&mut a)
        };
        let Some(rules) = rules else {
            warn!("abilities: tables not readable; the ability system is off");
            return;
        };
        let abilities = Abilities::with_defaults(Arc::new(rules));
        app.insert_resource(sora::Abilities::from_block(&abilities))
            .insert_resource(abilities)
            .add_systems(Update, log_changes.run_if(resource_changed::<Abilities>))
            .add_systems(FixedUpdate, sync_controller.after(Sim::Weapon).before(Sim::Sora).run_if(resource_changed::<Abilities>));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Option<Arc<AbilityRules>> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        let mut a = kh2_archive::Archive::open(&iso).map_err(|e| eprintln!("skipped: {iso}: {e}")).ok()?;
        AbilityRules::load(&mut a).map(Arc::new)
    }

    fn with(r: &Arc<AbilityRules>, ids: &[u16]) -> Abilities {
        let mut a = Abilities::new(r.clone());
        a.equipment.clear();
        a.recompute();
        for id in ids {
            a.equip_ability(*id);
        }
        a
    }

    #[test]
    fn defaults_match_the_block_initialiser() {
        let Some(r) = rules() else { return };
        let a = with(&r, &[]);
        assert_eq!(a.block(), &Block::default());
        assert_eq!((a.ground_combo(), a.air_combo(), a.finishers()), (3, 3, 1));
        assert_eq!(a.bits(), 0);
    }

    #[test]
    fn combo_abilities_change_the_lengths() {
        let Some(r) = rules() else { return };
        let a = with(&r, &[id::COMBO_PLUS, id::AIR_COMBO_PLUS, id::FINISHING_PLUS]);
        assert_eq!((a.ground_combo(), a.air_combo(), a.finishers()), (4, 4, 2));
        // The bits are the attack table's gates: Combo Plus 21, Air Combo Plus 22, Finishing Plus 105.
        assert!(a.has_bit(21) && a.has_bit(22) && a.has_bit(105));
        let two = with(&r, &[id::COMBO_PLUS, id::COMBO_PLUS, id::NEGATIVE_COMBO]);
        assert_eq!((two.ground_combo(), two.air_combo()), (4, 2));
        assert!(two.has(id::NEGATIVE_COMBO));
    }

    #[test]
    fn every_stat_ability_changes_the_block() {
        let Some(r) = rules() else { return };
        let all = [
            id::COMBO_PLUS, id::AIR_COMBO_PLUS, id::FINISHING_PLUS, id::NEGATIVE_COMBO, id::COMBO_BOOST,
            id::AIR_COMBO_BOOST, id::REACTION_BOOST, id::BERSERK_CHARGE, id::DAMAGE_DRIVE, id::DRIVE_BOOST,
            id::FORM_BOOST, id::SUMMON_BOOST, id::COMBINATION_BOOST, id::EXPERIENCE_BOOST, id::DRAW, id::JACKPOT,
            id::LUCKY_LUCKY, id::FIRE_BOOST, id::BLIZZARD_BOOST, id::THUNDER_BOOST, id::ITEM_BOOST, id::MP_RAGE,
            id::MP_HASTE, id::MP_HASTERA, id::MP_HASTEGA, id::DEFENDER, id::HYPER_HEALING,
        ];
        let base = Block::default();
        for id in all {
            let a = with(&r, &[id]);
            let mut want_bit = a.block().clone();
            want_bit.bits = [0; 8];
            assert_ne!(want_bit, base, "ability {id} must change a stat field");
            assert!(a.has(id), "ability {id} must set its bit");
            eprintln!("ability {id}: {}", a.block().deltas().join("; "));
        }
        // Steps come from sstm: Form Boost multiplies the drain by less than 1.
        let a = with(&r, &[id::FORM_BOOST, id::FORM_BOOST]);
        assert!(a.block().form_drain < 1.0 && a.block().form_drain > 0.0);
        let hy = with(&r, &[id::MP_HASTE]);
        assert!(hy.block().mp_regen > 1.0);
    }

    #[test]
    fn growth_levels_follow_the_highest_ability() {
        let Some(r) = rules() else { return };
        // Growth ids 94..=109 are four groups of four levels (items verified against the disc).
        let a = with(&r, &[94, 95, 98, 106, 107, 108, 109]);
        assert_eq!((a.growth(0), a.growth(1), a.growth(2), a.growth(3)), (2, 1, 0, 4));
        assert_eq!(with(&r, &[]).growth(0), 0);
        assert_eq!(r.growth_bits[0], [2, 3, 4, 5]);
        assert_eq!(r.growth_bits[3], [14, 15, 16, 17]);
    }

    #[test]
    fn form_hides_action_and_growth_abilities_of_the_player() {
        let Some(r) = rules() else { return };
        let mut a = with(&r, &[id::GUARD, 94, id::COMBO_PLUS]);
        assert!(a.has(id::GUARD) && a.growth(0) == 1 && a.ground_combo() == 4);
        // In a form only support (kind 0) abilities of the player stay; the form's own list applies.
        a.set_form(Some(vec![id::AIR_COMBO_PLUS, 95]));
        assert!(!a.has(id::GUARD));
        assert_eq!(a.growth(0), 2);
        assert_eq!((a.ground_combo(), a.air_combo()), (4, 4));
        a.set_form(None);
        assert!(a.has(id::GUARD) && a.growth(0) == 1);
    }

    #[test]
    fn kingdom_key_grants_its_own_ability_and_ap_is_counted() {
        let Some(r) = rules() else { return };
        let a = Abilities::new(r.clone());
        eprintln!("{}", a.report());
        eprintln!("{}", with(&r, &[id::COMBO_PLUS, id::FORM_BOOST, 94, 95]).report());
        let row =r.equipment_row(KINGDOM_KEY).expect("Kingdom Key row");
        assert_ne!(row.ability, 0);
        assert!(a.has(row.ability));
        let (s, m, _) = a.equipment_stats();
        assert_eq!((s, m), (row.strength as i32, row.magic as i32));
        let mut b = with(&r, &[id::COMBO_PLUS, id::GUARD]);
        assert_eq!(b.ap_used(), r.abilities[&id::COMBO_PLUS].ap as u16 + r.abilities[&id::GUARD].ap as u16);
        b.unequip_ability(id::GUARD);
        assert!(!b.has(id::GUARD));
        assert_eq!(b.ap_max(), r.ap_by_level[0]);
    }

    #[test]
    fn damage_boosts_follow_the_routine() {
        let Some(r) = rules() else { return };
        // The bits the damage routine tests are the rows' bits.
        assert_eq!(r.abilities[&id::COMBO_BOOST].bit, BIT_COMBO_BOOST);
        assert_eq!(r.abilities[&id::AIR_COMBO_BOOST].bit, BIT_AIR_COMBO_BOOST);
        assert_eq!(r.abilities[&id::REACTION_BOOST].bit, BIT_REACTION_BOOST);
        let atk = |element: u8, kind: u8| t::Atkp { element, kind, ..Default::default() };
        let none = with(&r, &[]);
        assert_eq!(attacker_bonuses(none.block(), 3, &atk(1, 7), 100), 100);
        // Fire Boost scales a Fire attack only.
        let fire = with(&r, &[id::FIRE_BOOST]);
        let f = fire.block().fire as i32;
        assert!(f > 100);
        assert_eq!(attacker_bonuses(fire.block(), 1, &atk(1, 0), 100), (100 * f + 99) / 100);
        assert_eq!(attacker_bonuses(fire.block(), 1, &atk(2, 0), 100), 100);
        // Combo Boost: per combo hit, capped at the ground length, only for rows with Kind bit 0.
        let cb = with(&r, &[id::COMBO_BOOST]);
        let step = cb.block().combo_boost as i32;
        assert!(step > 0);
        assert_eq!(attacker_bonuses(cb.block(), 2, &atk(0, 1), 100), 100 + (100 * step * 2 + 99) / 100);
        assert_eq!(attacker_bonuses(cb.block(), 9, &atk(0, 1), 100), 100 + (100 * step * 3 + 99) / 100);
        assert_eq!(attacker_bonuses(cb.block(), 2, &atk(0, 2), 100), 100);
        // Reaction Boost: a flat percent for Kind bit 2; Combo Boost takes precedence when both match.
        let rb = with(&r, &[id::REACTION_BOOST, id::COMBO_BOOST]);
        let rstep = rb.block().reaction_boost as i32;
        assert_eq!(attacker_bonuses(rb.block(), 1, &atk(0, 4), 100), 100 + (100 * rstep + 99) / 100);
        assert_eq!(attacker_bonuses(rb.block(), 1, &atk(0, 5), 100), 100 + (100 * step + 99) / 100);
        // The form chain's growth abilities by group and level.
        assert_eq!(r.growth_ability(0, 1), Some(94));
        assert_eq!(r.growth_ability(3, 4), Some(109));
        assert_eq!(r.growth_ability(0, 0), None);
    }

    #[test]
    fn env_bits_equip_the_ability_that_owns_the_bit() {
        let Some(r) = rules() else { return };
        assert_eq!(r.id_of_bit(21), Some(id::COMBO_PLUS));
        assert_eq!(r.id_of_bit(100), Some(272));
        assert_eq!(r.id_of_bit(1), Some(id::GUARD));
    }
}
