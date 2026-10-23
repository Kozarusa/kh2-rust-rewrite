//! The ability block and its rules (moved out of `abilities.rs`, which re-exports them).

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use kh2_archive::Archive;
use kh2_tables::player::{self, Sstm};
use kh2_tables::tables as t;

use crate::data::KINGDOM_KEY;

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

/// Ability bits of the combat abilities whose readers were traced (WP-A1; the rows own these bits,
/// checked against the disc in `combat_ability_bits_are_the_rows_bits`). Word / mask in the block:
/// Leaf Bracer word 3 0x40000, Second Chance word 3 0x80000000, Once More word 4 0x1, Auto Limit
/// word 4 0x2 (`FUN_001d25e0`, a party member's block), Auto Change word 4 0x4 (`FUN_0017ce60`),
/// Auto Healing word 4 0x10 (`FUN_0017d4b8`).
pub const BIT_LEAF_BRACER: u16 = 114;

pub const BIT_SECOND_CHANCE: u16 = 127;

pub const BIT_ONCE_MORE: u16 = 128;

pub const BIT_AUTO_LIMIT: u16 = 129;

pub const BIT_AUTO_CHANGE: u16 = 130;

pub const BIT_AUTO_HEALING: u16 = 132;

/// Sora's answer to "does this hit leave me alive?" (vtable slot +0x58 of his class, `FUN_00176d78`,
/// asked by `FUN_00185a68` only for a hit that would bring HP to the minimum or below):
/// Second Chance and HP at least 2 (status +0); else Once More and a damage motion playing
/// (`FUN_00185a08`: actor +0x140 set and the motion id +0x154 in 10..=39); else the base class's
/// answer (`FUN_00185be0`, from the actor's invincibility flags +0xabc/+0xac0/+0xac4 and +0x588,
/// which the app does not model: false here).
pub fn survives_lethal_hit(second_chance: bool, once_more: bool, hp: i32, motion: Option<usize>) -> bool {
    (second_chance && hp >= 2) || (once_more && motion.is_some_and(|m| (10..40).contains(&m)))
}

/// The damage `FUN_00185a68` applies for a non-healing hit of `dmg` on HP `hp` with the status
/// minimum `min` (status +8): when `hp - dmg <= min` and the victim survives the hit
/// (`survives`), the damage becomes `hp - min - 1` (never below 0), so HP ends at `min + 1`.
pub fn lethal_hit_damage(hp: i32, min: i32, dmg: i32, survives: bool) -> i32 {
    if hp - dmg <= min && survives { (hp - min - 1).max(0) } else { dmg }
}

/// `FUN_001a0be0(status)`: HP (status +0) at or below a quarter of the maximum (status +4, shifted
/// right by 2 with the compiler's rounding toward zero for a negative value).
pub fn hp_is_low(hp: i32, max_hp: i32) -> bool {
    hp <= (if max_hp < 0 { max_hp + 3 } else { max_hp }) >> 2
}

/// Defender (`FUN_001a0610`, the generic branch): the victim's defense `status +0x18c` is lowered by
/// the block byte `+0x2c` (`defense_bonus`) too, but only while the victim's HP is low
/// (`FUN_001a0be0`). Returns the extra defense, 0 when HP is not low.
pub fn defender_bonus(block: &Block, hp: i32, max_hp: i32) -> i32 {
    if hp_is_low(hp, max_hp) { i32::from(block.defense_bonus as u8) } else { 0 }
}

/// Berserk Charge (`FUN_001a0610`, attack rows whose byte `+5` is 0, the strength stat): while the
/// attacker's MP charge runs (`FUN_001a05d0`: status `+0x1c0` above 0) and his object has bit 4 of
/// `+0x588` (taken as set for Sora [inferred, as in prize.rs for Draw]) the block's `+0x4e`
/// (`berserk_charge`) is added to the strength. Returns the amount to add.
pub fn berserk_attack(block: &Block, charging: bool) -> i32 {
    if charging { i32::from(block.berserk_charge) } else { 0 }
}

/// Berserk Charge for an attack row: only rows whose byte `+5` (`critical_adjust`) is 0 read the strength.
pub fn berserk_for_row(block: &Block, charging: bool, critical_adjust: u8) -> i32 {
    if critical_adjust == 0 { berserk_attack(block, charging) } else { 0 }
}

/// MP Rage (`FUN_00176c98`, the HP method of the player): when HP drops (negative change) and the MP
/// charge is not running, `FUN_00186368` adds `trunc(damage / maximum HP * mp_rage)` MP (block `+0x54`)
/// when that is above 0. `FUN_00186368` first refuses when the object's `+0x7f8` has bit 4 [not modelled].
pub fn mp_rage_gain(block: &Block, charging: bool, damage: i32, max_hp: i32) -> i32 {
    if charging || damage <= 0 || max_hp <= 0 {
        return 0;
    }
    ((damage as f32 / max_hp as f32) * block.mp_rage) as i32
}

/// Damage Drive (`FUN_001792f0`, the HP method of the player, run before `FUN_00176c98`): a drop of
/// HP while the status kind (`+0x1af`) is 1 (gauge mode, not in a form) feeds `FUN_001788d8` with
/// `damage / maximum HP * damage_drive` (block `+0x50`) drive units. Returns the units (0.0 for no loss).
pub fn damage_drive_units(block: &Block, damage: i32, max_hp: i32) -> f32 {
    if damage <= 0 || max_hp <= 0 {
        return 0.0;
    }
    (damage as f32 / max_hp as f32) * block.damage_drive
}

/// Leaf Bracer (Cure's script, event 20 at word 0x1e8 of every `CURE_*.mag`): when the caster has
/// bit 114 (host 1.343 = `FUN_00170c28`) and the cast's target is the caster itself (host 1.130
/// answers 0), host 1.91 (`FUN_00172fe8` -> `FUN_00175aa8`) writes 60.0 into the caster's +0xb00.
/// `FUN_00175ab0` lowers it by the frame step each tick and `FUN_00175ad8` (vtable +0x60) answers
/// "untouchable" while it is above 0, which makes the receive handler `FUN_00175340` drop every hit
/// except heals (attack types 5 and 6). Returns the frames, or None.
pub const LEAF_BRACER_FRAMES: f32 = 60.0;

pub fn leaf_bracer_frames(leaf_bracer: bool, target_is_caster: bool) -> Option<f32> {
    (leaf_bracer && target_is_caster).then_some(LEAF_BRACER_FRAMES)
}

/// Scan (block word 0 0x80000): bit 19 (item 138).
pub const BIT_SCAN: u16 = 19;

/// The target's `+0x108` flags `FUN_0019b4a8` tests: 0x4000 refuses the gauge; 0x80000 or
/// 0x10000000 make `FUN_0019a440` answer 1, which `FUN_0019a3f8` turns into "not a gauge target".
pub const TARGET_NO_GAUGE: u32 = 0x4000;

pub const TARGET_NOT_SCANNABLE: u32 = 0x8_0000 | 0x1000_0000;

/// Scan: whether the target marker binds the target's HP gauge (`FUN_0019b4a8`, run by the marker's
/// set `FUN_0019b3f0` and its retarget `FUN_0019b5e8` after the old gauge is released by
/// `FUN_0019b548`). All of: the marker's player (+0x28) has a status block (+0x48c), Scan in that
/// block, the target (+4) is in the actor list (`FUN_001b80e8`: the chain from `DAT_01d5d7a0`
/// through +0x8b4), the target's +0x108 has neither 0x80000 nor 0x10000000 (`FUN_0019a440`) nor
/// 0x4000, and the target has a status block. The gauge then shows the target's HP record of part
/// `marker +0x24` (12-byte records from the status block, `FUN_00154688(record[0], record[1])`).
pub fn scan_binds_gauge(player_status: bool, scan: bool, target_listed: bool, target_flags: u32, target_status: bool) -> bool {
    player_status && scan && target_listed && target_flags & TARGET_NOT_SCANNABLE == 0 && target_flags & TARGET_NO_GAUGE == 0 && target_status
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

    /// `FUN_001c1340`: the highest growth level set per group (0 when none). The bits come only from
    /// applied abilities (`FUN_001c0e50` applies a list slot only when its bit 15 is set), so a growth
    /// group's level is the highest *equipped* level: an unequipped LV3 under an equipped LV1 is LV1.
    pub fn growth_levels(&self, block: &Block) -> [u8; 4] {
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
    pub fn equipment_row(&self, item: u16) -> Option<&EquipRow> {
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
    pub rules: Arc<AbilityRules>,
    /// The character's ability list (`party record + 0x54`, 64 slots): the first `mirrored` entries
    /// are `Progress`'s list (its non-empty slots in slot order, kept in step by `follow_progress`),
    /// the rest the harness's additions (`KH2_ABILITY_IDS`, `KH2_ABILITIES`).
    pub learned: Vec<Slot>,
    /// How many entries of `learned` mirror `Progress` (0 without it).
    pub mirrored: usize,
    /// Equipped items: keyblade, armor, accessories. Each grants its `item_stat` ability.
    pub equipment: Vec<u16>,
    /// While a drive form is active: the form's own ability list (`FUN_001c0e50`, 16 slots). Then
    /// only the character's support abilities (kind 0) still apply.
    pub form: Option<Vec<u16>>,
    /// Ability bits set directly (`KH2_ABILITIES` bits that no ability row owns).
    pub raw_bits: Vec<u16>,
    pub level: usize,
    pub block: Block,
    /// Dodge Roll level, the fifth growth kind (growth slot 4 of the Final Mix game, docs/systems/foundations/final-mix.md).
    /// The USA ability table has no row for it, so it is not derived from the block: it comes from
    /// `Progress::dodge_roll`. Outside a form only (a form's growth is its own ability [inferred]).
    pub dodge_roll: u8,
}

impl Abilities {
    pub fn new(rules: Arc<AbilityRules>) -> Abilities {
        let mut s = Abilities { rules, learned: Vec::new(), mirrored: 0, equipment: vec![KINGDOM_KEY], form: None, raw_bits: Vec::new(), level: 1, block: Block::default(), dodge_roll: 0 };
        s.recompute();
        s
    }

    /// Sora's equipment and ability list from `progress::Progress` (the new game plus its app
    /// baseline: Kingdom Key and Guard, which `KH2_ABILITY_START=data` drops there), then the
    /// environment overrides (see the module doc). Without `Progress`: the Kingdom Key and the
    /// Guard baseline as before.
    pub fn with_defaults(rules: Arc<AbilityRules>, progress: Option<&crate::progress::Progress>) -> Abilities {
        let mut s = Abilities::new(rules);
        match progress {
            Some(p) => {
                s.follow_progress(p);
            }
            None if std::env::var("KH2_ABILITY_START").as_deref() != Ok("data") => {
                s.equip_ability(id::GUARD);
                s.dodge_roll = 1;
            }
            None => {}
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

    /// Sora's list as `Progress` holds it (record `+0x54`, the non-empty slots in slot order).
    pub fn progress_list(p: &crate::progress::Progress) -> Vec<Slot> {
        p.sora().abilities.iter().map(|&(id, equipped)| Slot { id, equipped }).collect()
    }

    /// Whether `follow_progress` would change anything (read only, so a caller can keep change
    /// detection quiet when nothing moved).
    pub fn differs_from(&self, p: &crate::progress::Progress) -> bool {
        self.learned.get(..self.mirrored) != Some(&Self::progress_list(p)[..]) || self.equipment != p.equipment() || self.dodge_roll != p.dodge_roll
    }

    /// The record is the state (`FUN_001c0e50` reads the record's list each time the status is
    /// rebuilt, `FUN_0019fb98`): Sora's ability list, his equipment and the app's Dodge Roll level are
    /// copied from `Progress` and the block is rebuilt. The harness's additions stay after the copy.
    /// So whatever writes the record (the Abilities menu's toggle `FUN_0027d8b8`, a level-up's gift
    /// `FUN_001a18d8(id, 1, 0)`, a form level-up's growth upgrade `FUN_001d3f18`, `KH2_STATE`) reaches
    /// the growth levels and the ability bits. Returns whether anything changed.
    pub fn follow_progress(&mut self, p: &crate::progress::Progress) -> bool {
        if !self.differs_from(p) {
            return false;
        }
        let list = Self::progress_list(p);
        let n = self.mirrored.min(self.learned.len());
        self.learned.splice(..n, list.iter().copied());
        self.mirrored = list.len();
        self.learned.truncate(64);
        self.equipment = p.equipment();
        self.dodge_roll = p.dodge_roll;
        self.recompute();
        true
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

    /// Growth level 0..=4 of a group: 0 High Jump, 1 Quick Run, 2 Aerial Dodge, 3 Glide, 4 Dodge Roll
    /// (the app's `dodge_roll`; hidden in a form).
    pub fn growth(&self, group: usize) -> u8 {
        if group == 4 {
            return if self.in_form() { 0 } else { self.dodge_roll };
        }
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

    pub fn name(&self, id: u16) -> String {
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
            "abilities: equipment [{}]; learned [{}]; form list {:?}; raw bits {:?}; AP {}/{}; combo {}+finisher x{}, air {}; growth (High Jump, Quick Run, Aerial Dodge, Glide, Dodge Roll) {:?}; stat block {}",
            gear.join(", "),
            learned.join(", "),
            self.form,
            self.raw_bits,
            self.ap_used(),
            self.ap_max(),
            self.ground_combo(),
            self.finishers(),
            self.air_combo(),
            [self.growth(0), self.growth(1), self.growth(2), self.growth(3), self.growth(4)],
            if d.is_empty() { "unchanged".to_string() } else { d.join("; ") },
        )
    }
}
