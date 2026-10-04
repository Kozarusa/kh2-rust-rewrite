//! Game data read from the disc at startup: attack parameters, enemy and player stats, level
//! curves and Sora's movement constants. Nothing here is hard-coded game data.

use bevy::prelude::*;
use kh2_tables::tables::{self as t, Atkp, Enmp, Lvpm, Lvup, PrefPlyr, PrefPrty, PrefSstm};

use crate::assets::Disc;

/// KH2 movement values are per frame at the game's 60 Hz logic rate, in centimetres.
pub const LOGIC_HZ: f32 = 60.0;
/// Gravity in m/s². Not stored in the data files; tuned so a 185-unit jump lasts ~0.68 s.
pub const GRAVITY: f32 = 32.0;
/// Character index of Sora in `prty`/`plyr` and in `lvup`.
const SORA_PREF: usize = 1;
/// Kingdom Key item id.
pub const KINGDOM_KEY: u16 = 41;

#[derive(Resource)]
pub struct GameData {
    pub atkp: Vec<Atkp>,
    pub enmp: Vec<Enmp>,
    pub lvpm: Vec<Lvpm>,
    pub sora_levels: Vec<Lvup>,
    pub sora_max_hp: i32,
    pub sora_max_mp: i32,
    pub weapon_attack: i32,
    pub weapon_magic: i32,
    pub prty: PrefPrty,
    pub plyr: PrefPlyr,
    pub sstm: PrefSstm,
}

#[derive(Clone, Copy, Debug)]
pub struct Stats {
    pub level: i32,
    pub max_hp: i32,
    pub strength: i32,
    pub magic: i32,
    pub defense: i32,
    /// Damage this unit can take per hit (enemies: % of HP from `enmp`).
    pub max_taken: i32,
    pub min_taken: i32,
    /// Damage this unit can deal per hit before resistances (enemies: `lvpm` Max/MinStrength).
    pub max_dealt: i32,
    pub min_dealt: i32,
    /// Physical, fire, blizzard, thunder, dark, light, general — percent.
    pub weakness: [i32; 7],
}

fn pick<T>(v: Vec<T>, i: usize) -> T {
    v.into_iter().nth(i).expect("pref row")
}

fn load<T: kh2_tables::Record>(disc: &Disc, def: &kh2_tables::TableDef) -> Vec<T> {
    let mut a = disc.0.lock().unwrap();
    kh2_tables::load::<T>(&mut a, def).map(|d| d.rows).unwrap_or_else(|e| panic!("{}: {e}", def.name))
}

impl GameData {
    pub fn load(disc: &Disc) -> GameData {
        let lvup_blob = {
            let mut a = disc.0.lock().unwrap();
            kh2_tables::subfile(&mut a, &t::LVUP).expect("lvup")
        };
        let sora_levels = kh2_tables::rt::decode_lvup::<Lvup>(&lvup_blob).and_then(|c| c.into_iter().next()).unwrap_or_default();
        let plrp: Vec<t::Plrp> = load(disc, &t::PLRP);
        let sora = plrp.iter().find(|p| p.character == 1);
        let items: Vec<t::Item> = load(disc, &t::ITEM);
        let stats: Vec<t::ItemStat> = load(disc, &t::ITEM_STAT);
        let kk = items.iter().find(|i| i.id == KINGDOM_KEY).and_then(|i| stats.iter().find(|s| s.id == i.stat_entry));
        GameData {
            atkp: load(disc, &t::ATKP),
            enmp: load(disc, &t::ENMP),
            lvpm: load(disc, &t::LVPM),
            sora_levels,
            sora_max_hp: sora.map_or(20, |p| p.hp as i32),
            sora_max_mp: sora.map_or(100, |p| p.mp as i32),
            weapon_attack: kk.map_or(0, |s| s.attack as i32),
            weapon_magic: kk.map_or(0, |s| s.magic as i32),
            prty: pick(load(disc, &t::PREF_PRTY), SORA_PREF),
            plyr: pick(load(disc, &t::PREF_PLYR), SORA_PREF),
            sstm: pick(load(disc, &t::PREF_SSTM), 0),
        }
    }

    /// `atkp` entry by id (sub-id 0, lowest level variant first, as the game's lookup starts there).
    pub fn atkp(&self, id: i16) -> Option<&Atkp> {
        let id = id as u16;
        self.atkp.iter().find(|a| a.id == id && a.sub_id >> 8 == 0).or_else(|| self.atkp.iter().find(|a| a.id == id))
    }

    pub fn sora_stats(&self, level: i32) -> Stats {
        let l = self.sora_levels.get((level - 1).max(0) as usize);
        Stats {
            level,
            max_hp: self.sora_max_hp,
            strength: l.map_or(2, |l| l.strength as i32) + self.weapon_attack,
            magic: l.map_or(6, |l| l.magic as i32) + self.weapon_magic,
            defense: l.map_or(2, |l| l.defense as i32),
            max_taken: i32::MAX,
            min_taken: 1,
            max_dealt: i32::MAX,
            min_dealt: 0,
            weakness: [100; 7],
        }
    }

    /// Enemy stats exactly as the game initialises them (HP per part = ceil(lvpm.HpMultiplier ×
    /// enmp.Health / 100); damage caps are per-ten-thousand of HP; strength caps from lvpm).
    pub fn enemy_stats(&self, enmp_id: u16, world_level: i32) -> Stats {
        let e = self.enmp.iter().find(|e| e.id == enmp_id).or_else(|| self.enmp.first()).expect("enmp");
        let level = if e.level == 0 { world_level } else { e.level as i32 };
        let lv = &self.lvpm[((level - 1).max(0) as usize).min(self.lvpm.len() - 1)];
        let hp = (lv.hp_multiplier as i32 * e.health[0] as i32 + 99) / 100;
        Stats {
            level,
            max_hp: hp.max(1),
            strength: lv.strength as i32,
            magic: lv.strength as i32,
            defense: lv.defense as i32,
            max_taken: (hp * e.max_damage as i32 + 9999) / 10000,
            min_taken: (hp * e.min_damage as i32 + 9999) / 10000,
            max_dealt: lv.max_strength as i32,
            min_dealt: lv.min_strength as i32,
            weakness: [
                e.physical_weakness as i32,
                e.fire_weakness as i32,
                e.ice_weakness as i32,
                e.thunder_weakness as i32,
                e.dark_weakness as i32,
                e.light_weakness as i32,
                e.general_weakness as i32,
            ],
        }
    }

    /// The game's per-hit damage (behaviour of its damage routine, studied in the decompilation).
    pub fn damage(&self, atk: &Atkp, from: &Stats, to: &Stats) -> i32 {
        let stat = if atk.critical_adjust == 1 { from.magic } else { from.strength };
        let power = atk.power as i32;
        let dmg = match atk.r#type {
            5 => (to.max_hp * stat * power + 9999) / 10000,
            6 => (stat * power + 99) / 100,
            _ => {
                let cap_hi = to.max_taken.min(if from.max_dealt > 0 { from.max_dealt } else { i32::MAX });
                let cap_lo = to.min_taken.max(from.min_dealt);
                let base = (stat - to.defense).min(cap_hi).max(cap_lo);
                let elem = to.weakness[(atk.element as usize).min(6)] as i64;
                ((base as i64 * power as i64 * elem * to.weakness[6] as i64 + 999_999) / 1_000_000) as i32
            }
        };
        dmg.max(0)
    }

    pub fn run_speed(&self) -> f32 {
        self.prty.run_speed * LOGIC_HZ * crate::SCALE
    }
    pub fn walk_speed(&self) -> f32 {
        self.prty.walk_speed * LOGIC_HZ * crate::SCALE
    }
    pub fn jump_velocity(&self) -> f32 {
        (2.0 * GRAVITY * self.prty.jump_height * crate::SCALE).sqrt()
    }
    pub fn turn_rate(&self) -> f32 {
        self.prty.turn_speed * LOGIC_HZ
    }
    pub fn fall_max(&self) -> f32 {
        self.sstm.fall_max * LOGIC_HZ * crate::SCALE
    }
}
