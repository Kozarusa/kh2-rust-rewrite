//! Drive forms: the drive gauge, entering and leaving a form, the form timer, and the form's
//! model, motions, attack table and keyblade motion sets.
//!
//! `rules` holds the game's rules as pure functions over rows read from the disc (tested against the
//! real rows); the plugin below runs them on Sora through his public fields, because the
//! controller (`sora.rs`) is owned by another work package. What the controller still has to give
//! this module is listed in `docs/drive-forms.md` ("Hook points in sora.rs"). Where the game's own
//! code was not found the module says so (`GAP` comments) instead of guessing.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::system::SystemParam;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_anim::moveset::Moveset;
use kh2_tables::player::{self, form, DriveForm, FormLevels, Memt, Sstm};
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::combat::{Fighter, Hit};
use crate::data::{GameData, KINGDOM_KEY};
use crate::fx::EffectLib;
use crate::schedule::{Frame, Sim};
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::sora::{self, Abilities, Battle, Growth, HandSocket, Rigs, Sora, State};
use crate::SCALE;

#[path = "hud_drive.rs"]
mod hud_drive;
pub use hud_drive::DriveGauge;

/// Sora's left-hand bone (the mirror of `sora::RIGHT_HAND`, bone 178).
const LEFT_HAND: usize = 86;
/// Motion id the old body plays when a form starts (`FUN_001b4c28` asks for 0x34) and the id the new
/// form body plays on arrival (every form motion set has one at 53; the call that starts it was
/// not found, see the doc).
const MOTION_LEAVE: usize = 52;
const MOTION_ARRIVE: usize = 53;
/// Safety stop for a change motion that never reports finished.
const CHANGE_TIMEOUT: u32 = 600;

/// A form's name for logs; id 0 is base Sora (`player::form::name(0)` names the summon curve).
fn form_name(f: u8) -> &'static str {
    if f == 0 { "base Sora" } else { form::name(f) }
}

/// Pure rules. Every function names the game routine it follows.
pub mod rules {
    use super::*;

    /// Elapsed time per logic tick in the units of the game's frame unit (`DAT_00356f40`): one
    /// 60 Hz tick is 1.0 (the unit counts vertical syncs; see the doc).
    pub const FRAME_UNIT: f32 = 1.0;
    /// Ability id of Form Boost, the one ability the form timer drain reads (`FUN_001c0f40`).
    pub const FORM_BOOST_ID: u16 = 398;

    /// The `sstm` fields the drive code reads (offsets in `player::sstm_label`).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Tuning {
        pub point_units: f32,
        pub field_drain_scale: f32,
        pub form_hit_loss: f32,
        pub anti_level_basis: i32,
        pub anti_drop: i32,
        pub anti_damage: f32,
        pub final_base: f32,
        pub final_growth: f32,
        pub final_cap: f32,
        pub final_anti_drop: i32,
        pub form_boost: f32,
    }

    impl Tuning {
        pub fn from_sstm(s: &Sstm) -> Tuning {
            Tuning {
                point_units: s.f32(0x68),
                field_drain_scale: s.f32(0x6c),
                form_hit_loss: s.f32(0x74),
                anti_level_basis: s.i32(0x8c),
                anti_drop: s.i32(0x90),
                anti_damage: s.f32(0x94),
                final_base: s.f32(0x98),
                final_growth: s.f32(0x9c),
                final_cap: s.f32(0xa0),
                final_anti_drop: s.i32(0xa4),
                form_boost: s.f32(0x140),
            }
        }
    }

    /// The normal drive gauge: whole points, a fraction of the next point 0..=100, and the maximum
    /// (status +0x1b1, +0x1b0, +0x1b2). A full gauge keeps fraction 100.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Points {
        pub whole: u8,
        pub fraction: u8,
        pub max: u8,
    }

    impl Points {
        /// `FUN_001a0290`: add to the fraction, carry into points, borrow from a point when negative.
        pub fn gain(&mut self, delta: i32) {
            let mut f = self.fraction as i32 + delta;
            if delta < 0 {
                if f < 0 {
                    if self.whole == 0 {
                        f = 0;
                    } else {
                        f += 100;
                        self.whole -= 1;
                    }
                }
            } else if f > 99 {
                loop {
                    if self.whole < self.max {
                        self.whole += 1;
                    }
                    if self.max <= self.whole {
                        f = 100;
                        break;
                    }
                    f -= 100;
                    if f <= 99 {
                        break;
                    }
                }
            }
            self.fraction = f as u8;
        }

        /// `FUN_001a0320`: pay `cost` points; a full gauge or a short one loses its fraction.
        pub fn spend(&mut self, cost: u8) {
            if self.whole == self.max || self.whole < cost {
                self.fraction = 0;
            }
            self.whole = self.whole.saturating_sub(cost);
        }
    }

    /// The form timer (status +0x1b4 left, +0x1b8 full), in gauge units.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct FormTimer {
        pub left: f32,
        pub total: f32,
    }

    /// The level the timer length uses (`FUN_001a04b0`): the form's level, but for Anti a fixed
    /// `sstm[0x8c] - 2`.
    pub fn timer_level(t: &Tuning, form: u8, level: u8) -> i32 {
        if form == player::form::ANTI {
            t.anti_level_basis - 2
        } else {
            level as i32
        }
    }

    impl FormTimer {
        /// `FUN_001a0480`: one drive point is `sstm[0x68]` gauge units; the timer is that times level + 3.
        pub fn start(t: &Tuning, form: u8, level: u8) -> FormTimer {
            let v = t.point_units * (timer_level(t, form, level) + 3) as f32;
            FormTimer { left: v, total: v }
        }

        /// `FUN_001a0358`: add `delta` (negative drains), clamped to 0..=total; in Anti form a
        /// positive change is turned negative.
        pub fn add(&mut self, form: u8, delta: f32) {
            let delta = if form == player::form::ANTI && delta > 0.0 { -delta } else { delta };
            self.left = (self.left + delta).clamp(0.0, self.total);
        }

        /// `FUN_001a0420`: whole drive points left (one less while the timer is full).
        pub fn whole_points(&self, t: &Tuning) -> i32 {
            let n = (self.left / t.point_units) as i32;
            if self.left == self.total {
                n - 1
            } else {
                n
            }
        }

        /// `FUN_001ceda8` reverts when the timer is empty.
        pub fn expired(&self) -> bool {
            self.left <= 0.0
        }
    }

    /// Drain per logic tick (`FUN_00177788`): the frame unit, times `sstm[0x6c]` while the battle
    /// flag (`FUN_001903d8`) is clear, times the ability block's form multiplier.
    pub fn drain_per_tick(t: &Tuning, battle: bool, form_boost: f32) -> f32 {
        FRAME_UNIT * if battle { 1.0 } else { t.field_drain_scale } * form_boost
    }

    /// Anti form multiplies the damage Sora deals by `sstm[0x94]` (`FUN_001a0610`): in the generic
    /// damage branch, not for the percentage-of-HP and fixed types (5 and 6). The game applies it to
    /// the damage after its element and defence steps and before its last two adjustments
    /// (`FUN_001a0a18`, `FUN_001a0aa8`, not ported).
    pub fn outgoing_damage(t: &Tuning, form: u8, atk_type: u8, dmg: i32) -> i32 {
        if form == player::form::ANTI && !matches!(atk_type, 5 | 6) { (dmg as f32 * t.anti_damage) as i32 } else { dmg }
    }

    /// The form timer multiplier of the abilities the form has granted: Form Boost multiplies by
    /// `sstm[0x140]` once per copy (`FUN_001c0f40`, ability 398).
    pub fn form_boost(t: &Tuning, granted: &[u16]) -> f32 {
        granted.iter().filter(|&&a| a == FORM_BOOST_ID).fold(1.0, |m, _| m * t.form_boost)
    }

    /// Abilities the form has earned at `level`: the `ability` column of its `fmlv` rows up to the level.
    pub fn granted_abilities(levels: &FormLevels, form: u8, level: u8) -> Vec<u16> {
        levels.levels(form).filter(|r| r.level <= level && r.ability != 0).map(|r| r.ability).collect()
    }

    /// Growth ability level the form gives (`fmlv` display level; its growth group is `form - 1`:
    /// High Jump, Quick Run, Aerial Dodge, Glide). Anti has none.
    pub fn growth_level(levels: &FormLevels, form: u8, level: u8) -> u8 {
        if (form::VALOR..=form::FINAL).contains(&form) { levels.get(form, level).map_or(0, |r| r.display_level) } else { 0 }
    }

    /// Which companions a form absorbs (`FUN_001c77a8`): (Donald, Goofy).
    pub fn companions(form: u8) -> (bool, bool) {
        match form {
            1 => (false, true),
            2 => (true, false),
            3..=5 => (true, true),
            _ => (false, false),
        }
    }

    /// Whether the companions the form needs are in the party (`FUN_001c77f0` returning 0).
    pub fn companions_ok(form: u8, donald: bool, goofy: bool) -> bool {
        let (d, g) = companions(form);
        (d || g) && (!d || donald) && (!g || goofy)
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Refusal {
        UnknownForm,
        Locked,
        InForm,
        NotOwned,
        Companions,
        NotEnoughPoints { have: u8, need: u8 },
    }

    /// What decides whether the Drive command is available (`FUN_001ba568`, `FUN_001c7970`).
    #[derive(Clone, Copy, Debug)]
    pub struct EnterCtx {
        pub current_form: u8,
        pub owned: [bool; 6],
        pub points: u8,
        pub donald: bool,
        pub goofy: bool,
        /// A command lock is up (`DAT_0035d268` bit 0x40000, or a change already running).
        pub locked: bool,
    }

    /// The drive command of `chosen`, if the menu would offer it: not locked, no form active (the
    /// menu shows Revert then, `FUN_001bc578`), the form's item owned, its companions present, and
    /// `cmd.Cost <= points` (`FUN_001ba568`).
    pub fn check_enter(forms: &[DriveForm], chosen: u8, c: &EnterCtx) -> Result<DriveForm, Refusal> {
        let row = forms.iter().find(|f| f.form == chosen).ok_or(Refusal::UnknownForm)?;
        if c.locked {
            return Err(Refusal::Locked);
        }
        if c.current_form != 0 {
            return Err(Refusal::InForm);
        }
        if !c.owned.get(chosen as usize).copied().unwrap_or(false) {
            return Err(Refusal::NotOwned);
        }
        if !companions_ok(chosen, c.donald, c.goofy) {
            return Err(Refusal::Companions);
        }
        if row.cost > c.points {
            return Err(Refusal::NotEnoughPoints { have: c.points, need: row.cost });
        }
        Ok(row.clone())
    }

    /// Revert from the menu (`FUN_001bc578`): the Revert entry is marked unavailable in Anti form while
    /// the battle flag is set. A timer running out reverts regardless (`FUN_001ceda8`).
    pub fn can_revert(form: u8, battle: bool) -> bool {
        form != 0 && !(form == player::form::ANTI && battle)
    }

    /// The game's random generator (`DAT_003716a0 * 0x10dcd`, wrapping).
    #[derive(Clone, Copy, Debug)]
    pub struct Lcg(pub u32);

    impl Lcg {
        pub fn step(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(0x10dcd);
            self.0
        }
    }

    /// The Anti roll's random number, 0..=99 (`FUN_00325b38(state, 0x28f5c29)`, a division).
    pub fn percent_int(state: u32) -> i32 {
        (state / 0x028f5c29) as i32
    }

    /// The Final roll's random number, 0..100 as a float (`state * 2^-32 * 100`).
    pub fn percent_float(state: u32) -> f32 {
        state as f32 * 2.3283064e-10 * 100.0
    }

    /// Final Form's chance in percent for its counter (`FUN_001d4348`).
    pub fn final_chance(t: &Tuning, counter: i32) -> f32 {
        (t.final_base * t.final_growth.powf(counter as f32)).min(t.final_cap)
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct Counters {
        pub anti: i32,
        pub fin: i32,
    }

    /// Inputs of the form choice (`FUN_001d4180`).
    #[derive(Clone, Copy, Debug)]
    pub struct RollCtx {
        pub chosen: u8,
        /// `FUN_001c77f0(5) == 0`: the companions Anti absorbs are present.
        pub may_anti: bool,
        /// The battle flag (`FUN_001903d8`).
        pub battle: bool,
        pub final_owned: bool,
        /// Story flag 0x4842 (set later in the story).
        pub story_flag: bool,
        pub points: u8,
        pub final_cost: u8,
        /// `FUN_001b9d70`: 1 except in special battles.
        pub anti_factor: i32,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Rolled {
        pub form: u8,
        pub counters: Counters,
        /// The Final Form item is added to the inventory.
        pub gave_final: bool,
    }

    /// `FUN_001d4180` with `FUN_001d4278` (Anti roll) and `FUN_001d4348` (Final roll).
    pub fn choose_form(t: &Tuning, levels: &FormLevels, c: &RollCtx, mut k: Counters, rng: &mut Lcg) -> Rolled {
        let drop = |v: i32, by: i32| (v - by).max(0);
        if c.chosen == form::FINAL {
            k.anti = drop(k.anti, t.final_anti_drop);
            return Rolled { form: form::FINAL, counters: k, gave_final: false };
        }
        // Anti roll: only when the companions allow it; the counter rises on every roll.
        let mut anti = false;
        if c.may_anti {
            let chance = player::anti_form_chance(levels, k.anti) as i32;
            anti = percent_int(rng.step()) < chance * c.anti_factor;
            k.anti += 1;
        }
        if !anti || !c.battle {
            return Rolled { form: c.chosen, counters: k, gave_final: false };
        }
        // Final roll: needs the item not owned, its companions, the story flag and the points.
        if !c.final_owned && c.may_anti && c.story_flag && c.final_cost <= c.points {
            let chance = final_chance(t, k.fin);
            let roll = percent_float(rng.step());
            k.fin += 1;
            if roll < chance {
                k.fin = 0;
                k.anti = drop(k.anti, t.final_anti_drop);
                return Rolled { form: form::FINAL, counters: k, gave_final: true };
            }
        }
        k.anti = drop(k.anti, t.anti_drop);
        Rolled { form: form::ANTI, counters: k, gave_final: false }
    }

    /// The player's object words after the world merge (`FUN_001a6ff0`): record 0's words 8..24 are
    /// cleared, then every record that matches the world and the story flags overwrites them with
    /// its non-zero words, in table order. A record matches when its key is 0 or the world, its
    /// first flag (if any) is set and its second flag (if any) is clear (`FUN_001a6f70`).
    pub fn memt_merge(m: &Memt, world: u16, flag: impl Fn(u16) -> bool) -> [u16; 24] {
        let mut out = m.rows.first().copied().unwrap_or([0; 24]);
        out[8..].fill(0);
        for r in &m.rows {
            let ok = (r[0] == 0 || r[0] == world) && (r[1] == 0 || flag(r[1])) && (r[2] == 0 || !flag(r[2]));
            if ok {
                for k in 8..24 {
                    if r[k] != 0 {
                        out[k] = r[k];
                    }
                }
            }
        }
        out
    }

    /// Object id of a form in a merged record (`FUN_001cf2c8`: byte offset 0x16 + 2 * form).
    pub fn merged_form_object(rec: &[u16; 24], form: u8) -> Option<u16> {
        (1..=5).contains(&form).then(|| rec[11 + form as usize]).filter(|&o| o != 0)
    }

    /// `FUN_001845b0`: the weapon object for a moveset and an item's slot (the item's
    /// `Flag1 | Rank << 8`), from the `went` table: a u32 per moveset giving a word offset, then
    /// one u32 object id per slot at that offset.
    pub fn weapon_object(went: &[u8], moveset: usize, slot: u16) -> Option<u32> {
        let w = |i: usize| went.get(i * 4..i * 4 + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        let off = w(moveset)? as usize;
        if off == 0 {
            return None;
        }
        w(off + slot as usize).filter(|&o| o != 0)
    }

    /// The `wmst` table: 0x20-byte names, two per moveset (right-hand and left-hand weapon motion
    /// set, empty when the hand holds none; `FUN_00184640`).
    pub fn parse_wmst(b: &[u8]) -> Vec<[String; 2]> {
        let name = |c: &[u8]| String::from_utf8_lossy(&c[..c.iter().position(|&x| x == 0).unwrap_or(c.len())]).into_owned();
        b.as_chunks::<0x40>().0.iter().map(|c| [name(&c[..0x20]), name(&c[0x20..])]).collect()
    }
}

/// The disc data the drive system needs.
#[derive(Resource)]
pub struct DriveRules {
    pub forms: Vec<DriveForm>,
    pub levels: FormLevels,
    pub tuning: rules::Tuning,
    memt: Memt,
    objs: Vec<t::Objentry>,
    wmst: Vec<[String; 2]>,
    went: Vec<u8>,
    /// Ability item id to its ability bit (the `abilities` table).
    ability_bits: HashMap<u16, u16>,
    /// Slot of the equipped keyblade item (Kingdom Key), the `went` index of the weapon object.
    key_slot: u16,
}

/// What one form is made of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormFiles {
    pub model: String,
    pub mset: String,
    pub moveset: u16,
    /// Weapon model and the motion sets of the right and left hand (empty = no weapon in that hand).
    pub weapon_model: Option<String>,
    pub right_mset: Option<String>,
    pub left_mset: Option<String>,
}

impl DriveRules {
    pub fn load(disc: &Disc) -> Option<DriveRules> {
        let mut a = disc.0.lock().ok()?;
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).ok()?.rows;
        let cmds = kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok()?.rows;
        let fmlv = kh2_tables::load::<t::Fmlv>(&mut a, &t::FMLV).ok()?.rows;
        let objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()?.rows;
        let memt = player::load_memt(&mut a).ok()?;
        let sstm = player::load_sstm(&mut a).ok()?;
        let wmst = rules::parse_wmst(&kh2_tables::subfile(&mut a, &t::WMST).ok()?);
        let went = kh2_tables::subfile(&mut a, &t::WENT).ok()?;
        let key = items.iter().find(|i| i.id == KINGDOM_KEY)?;
        Some(DriveRules {
            forms: player::drive_forms(&items, &cmds, &memt),
            levels: FormLevels::from_rows(&fmlv),
            tuning: rules::Tuning::from_sstm(&sstm),
            memt,
            objs,
            wmst,
            went,
            ability_bits: player::abilities(&items).into_iter().map(|a| (a.id, a.bit)).collect(),
            key_slot: key.flag1 as u16 | (key.rank as u16) << 8,
        })
    }

    /// The abilities a form has earned at `level` as (id, bit) pairs, for the controller's ability block.
    pub fn granted_with_bits(&self, form: u8, level: u8) -> Vec<(u16, u16)> {
        rules::granted_abilities(&self.levels, form, level).into_iter().filter_map(|id| Some((id, *self.ability_bits.get(&id)?))).collect()
    }

    pub fn form(&self, form: u8) -> Option<&DriveForm> {
        self.forms.iter().find(|f| f.form == form)
    }

    /// Files of a form in a world (`world` selects the `_NM`, `_TR`, `_WI` variants through the
    /// memt merge; no story flag is set).
    pub fn files(&self, form: u8, world: u16) -> Option<FormFiles> {
        let rec = rules::memt_merge(&self.memt, world, |_| false);
        let obj = rules::merged_form_object(&rec, form)?;
        let o = self.objs.iter().find(|o| o.object_id == obj as u32)?;
        let names = self.wmst.get(o.neo_moveset as usize);
        let non_empty = |i: usize| names.and_then(|n| n.get(i)).filter(|s| !s.is_empty()).cloned();
        let right = non_empty(0);
        let weapon_model = right.as_ref().and_then(|_| {
            let id = rules::weapon_object(&self.went, o.neo_moveset as usize, self.key_slot)?;
            self.objs.iter().find(|w| w.object_id == id).map(|w| w.model_name.clone())
        });
        Some(FormFiles {
            model: o.model_name.clone(),
            mset: o.animation_name.trim_end_matches(".mset").to_string(),
            moveset: o.neo_moveset,
            weapon_model,
            right_mset: right,
            left_mset: non_empty(1),
        })
    }
}

/// Persistent drive state (what the game keeps in its save record and status block).
#[derive(Resource, Clone, Debug)]
pub struct DriveState {
    pub points: rules::Points,
    /// Gauge mode (status +0x1af): the timer runs only in form mode.
    pub in_form: bool,
    /// Current form (`DAT_00340c88` byte 4); set when a change starts.
    pub form: u8,
    /// Form level per form id (index 1..=5).
    pub level: [u8; 6],
    pub owned: [bool; 6],
    pub counters: rules::Counters,
    pub timer: rules::FormTimer,
    /// Times each form was entered (capped at 999).
    pub uses: [u16; 6],
    pub donald: bool,
    pub goofy: bool,
    pub rng: rules::Lcg,
    /// The battle flag is forced by `KH2_DRIVE_BATTLE`.
    pub force_battle: bool,
    ticks: u32,
}

impl DriveState {
    fn from_env(rules: &DriveRules) -> DriveState {
        let env = |k: &str| std::env::var(k).ok();
        let num = |k: &str| env(k).and_then(|v| v.trim().parse::<i32>().ok());
        let scripted = env("KH2_DRIVE").is_some();
        // New game: 0 points of 3 (FUN_001cda78). The harness starts with 9 so that any form can be
        // entered (the maximum is raised by bonuses in the game, `bons` DriveGaugeUpgrade).
        let want = num("KH2_DRIVE_POINTS").map_or(if scripted { 9 } else { 0 }, |v| v.clamp(0, 9) as u8);
        let points = rules::Points { whole: want, fraction: 0, max: want.max(3) };
        let mut owned = [false; 6];
        match env("KH2_DRIVE_OWNED") {
            Some(v) => v.split(',').filter_map(|x| x.trim().parse::<usize>().ok()).filter(|&f| f < 6).for_each(|f| owned[f] = true),
            None => owned[1..=4].fill(true),
        }
        let cap = player::form_level_cap(owned[1..=4].iter().filter(|&&o| o).count() as u32);
        let level = num("KH2_DRIVE_LEVEL").map_or(1, |v| v.clamp(1, cap as i32) as u8);
        let seed = env("KH2_DRIVE_SEED").and_then(|v| v.parse::<u32>().ok()).unwrap_or_else(|| {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos())
        });
        let party = env("KH2_DRIVE_PARTY").unwrap_or_else(|| "both".into());
        let _ = rules;
        DriveState {
            points,
            in_form: false,
            form: 0,
            level: [0, level, level, level, level, level],
            owned,
            counters: rules::Counters { anti: num("KH2_DRIVE_ANTI_COUNTER").unwrap_or(0), fin: 0 },
            timer: Default::default(),
            uses: [0; 6],
            donald: matches!(party.as_str(), "both" | "donald"),
            goofy: matches!(party.as_str(), "both" | "goofy"),
            rng: rules::Lcg(seed | 1),
            force_battle: env("KH2_DRIVE_BATTLE").is_some_and(|v| v != "0"),
            ticks: 0,
        }
    }
}

/// Requests to the drive system. Written by the keys and the harness, or by other code (a command menu).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveRequest {
    /// The Drive command for this form.
    Enter(u8),
    /// The Revert command.
    Revert,
    /// Add drive gauge fraction (hit gain, orbs, debug keys).
    Gain(i32),
}

#[derive(Resource, Default)]
pub struct DriveQueue(pub Vec<DriveRequest>);

/// Sent when the body has been swapped: `from` and `to` are form ids (0 = base Sora).
#[derive(Message, Clone, Copy, Debug)]
pub struct FormChanged {
    pub from: u8,
    pub to: u8,
}

/// The world the forms are looked up for (selects `_NM`, `_TR`, `_WI` objects). 2 is Twilight Town.
#[derive(Resource, Clone, Copy, Debug)]
pub struct DriveWorld(pub u16);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    /// The old body plays the form start motion.
    Leaving,
    /// The new body plays its arrival motion.
    Arriving,
}

#[derive(Resource, Default)]
struct Change(Option<ChangeTask>);

struct ChangeTask {
    to: u8,
    phase: Phase,
    ticks: u32,
    /// The old body plays a change motion that the swap waits for.
    motion: bool,
}

/// Loaded form models, filled on first use.
struct FormAssets {
    body: Arc<Rig>,
    right: Arc<Rig>,
    left: Option<Arc<Rig>>,
    moves: Arc<Moveset>,
    has_weapon: bool,
}

#[derive(Resource, Default)]
struct FormCache(HashMap<(u8, u16), Arc<FormAssets>>);

/// What base Sora was before the first form, for the revert.
struct Base {
    moves: Arc<Moveset>,
    growth: Growth,
}

#[derive(Resource, Default)]
struct BaseSora(Option<Base>);

/// The second weapon (left hand) of the dual-wielding forms.
#[derive(Resource, Default)]
struct LeftWeapon(Option<Entity>);

#[derive(Component)]
struct LeftSocket;

/// The drive system's own state, as one system parameter.
#[derive(SystemParam)]
struct Store<'w> {
    state: ResMut<'w, DriveState>,
    queue: ResMut<'w, DriveQueue>,
    change: ResMut<'w, Change>,
    cache: ResMut<'w, FormCache>,
    base: ResMut<'w, BaseSora>,
    left: ResMut<'w, LeftWeapon>,
    abilities: ResMut<'w, Abilities>,
    battle: Res<'w, Battle>,
}

/// The asset stores a model spawn needs.
#[derive(SystemParam)]
struct Gfx<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    bindposes: ResMut<'w, Assets<SkinnedMeshInverseBindposes>>,
}

/// Scripted harness: `KH2_DRIVE=<form>` enters that form at `KH2_DRIVE_AT` seconds (default 4),
/// `KH2_DRIVE_REVERT_AT=<s>` reverts. Keys: F5 chooses the form (1..4), F7 enters it, F8 reverts,
/// F10 gives one drive point, F11 fills the gauge.
#[derive(Resource)]
struct Harness {
    enter: Option<(f32, u8)>,
    revert_at: Option<f32>,
    chosen: u8,
}

pub struct DrivePlugin;

impl Plugin for DrivePlugin {
    fn build(&self, app: &mut App) {
        let Some(rules) = app.world().get_resource::<Disc>().and_then(DriveRules::load) else {
            warn!("drive: tables not readable; drive forms are off");
            return;
        };
        let env_f = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
        let world = std::env::var("KH2_DRIVE_WORLD")
            .ok()
            .map(|v| match v.to_ascii_lowercase().as_str() {
                // The three worlds with their own outfit records (memt keys 13, 14, 17 and their
                // object names); the other names are not needed for any record.
                "wi" => 13,
                "nm" => 14,
                "tr" => 17,
                n => n.parse().unwrap_or(2),
            })
            .unwrap_or(2);
        let enter = std::env::var("KH2_DRIVE").ok().and_then(|v| v.parse::<u8>().ok()).map(|f| (env_f("KH2_DRIVE_AT").unwrap_or(4.0), f));
        app.insert_resource(DriveState::from_env(&rules))
            .insert_resource(rules)
            .insert_resource(DriveWorld(world))
            .insert_resource(Harness { enter, revert_at: env_f("KH2_DRIVE_REVERT_AT"), chosen: enter.map_or(1, |e| e.1) })
            .init_resource::<DriveQueue>()
            .init_resource::<Change>()
            .init_resource::<FormCache>()
            .init_resource::<BaseSora>()
            .init_resource::<LeftWeapon>()
            .add_message::<FormChanged>()
            .add_plugins(hud_drive::HudDrivePlugin)
            .add_systems(Update, input.after(Frame::Pad))
            .add_systems(
                FixedUpdate,
                (
                    drive_tick.in_set(Sim::Weapon).before(sora::sync_weapon),
                    sync_left.in_set(Sim::Weapon).after(sora::sync_weapon),
                    (follow_left, weapon_visibility.after(sora::tick)).in_set(Sim::Sora),
                    log_changes.in_set(Sim::Sounds),
                ),
            );
    }
}

/// Keys and the scripted harness, as requests.
fn input(keys: Res<ButtonInput<KeyCode>>, time: Res<Time>, mut h: ResMut<Harness>, mut queue: ResMut<DriveQueue>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs());
    let crossed = |at: f32| now >= at && now - dt < at;
    if let Some((at, f)) = h.enter
        && crossed(at)
    {
        queue.0.push(DriveRequest::Enter(f));
    }
    if let Some(at) = h.revert_at
        && crossed(at)
    {
        queue.0.push(DriveRequest::Revert);
    }
    if keys.just_pressed(KeyCode::F5) {
        h.chosen = h.chosen % 4 + 1;
        info!("drive: form chosen {}", form_name(h.chosen));
    }
    if keys.just_pressed(KeyCode::F7) {
        queue.0.push(DriveRequest::Enter(h.chosen));
    }
    if keys.just_pressed(KeyCode::F8) {
        queue.0.push(DriveRequest::Revert);
    }
    if keys.just_pressed(KeyCode::F10) {
        queue.0.push(DriveRequest::Gain(100));
    }
    if keys.just_pressed(KeyCode::F11) {
        queue.0.push(DriveRequest::Gain(100 * 9));
    }
}

fn read_rig(disc: &Disc, model: &str, mset: &str) -> Option<Arc<Rig>> {
    let m = disc.read(&format!("obj/{model}.mdlx")).and_then(|d| kh2_model::mdlx(&d))?;
    let s = disc.read(&format!("obj/{mset}.mset")).and_then(|d| kh2_anim::Mset::parse(&d))?;
    let apdx = format!("obj/{model}.apdx");
    let apdx = disc.read(&apdx).is_some().then_some(apdx);
    Some(Arc::new(Rig { model: m, mset: s, apdx }))
}

fn load_assets(disc: &Disc, rules: &DriveRules, rigs: &Rigs, form: u8, world: u16) -> Option<FormAssets> {
    let f = rules.files(form, world)?;
    let body = read_rig(disc, &f.model, &f.mset)?;
    let moves = {
        let mut a = disc.0.lock().ok()?;
        kh2_tables::subfile(&mut a, &t::PTYA).ok().and_then(|b| Moveset::parse(&b, f.moveset as usize))?
    };
    let weapon = f.weapon_model.as_deref().zip(f.right_mset.as_deref());
    let right = weapon.and_then(|(m, s)| read_rig(disc, m, s));
    let left = f.weapon_model.as_deref().zip(f.left_mset.as_deref()).and_then(|(m, s)| read_rig(disc, m, s));
    let has_weapon = right.is_some();
    info!(
        "drive: loaded {} (mset {}, attack table {}, right weapon {:?}, left weapon {:?})",
        f.model, f.mset, f.moveset, f.right_mset, f.left_mset
    );
    Some(FormAssets { body, right: right.unwrap_or_else(|| rigs.keyblade.clone()), left, moves: Arc::new(moves), has_weapon })
}

/// Registers a rig's effect file with the effect library so its motion effects run.
fn register_effects(disc: &Disc, lib: &mut EffectLib, rig: &Rig) {
    let Some(name) = &rig.apdx else { return };
    if lib.objects.contains_key(name) {
        return;
    }
    let common = disc.read("effect/texcommon.dpd").and_then(|b| kh2_fx::Dpd::parse(&b).ok());
    if let Some(p) = crate::fx::load(disc, name, 100 + lib.objects.len(), common.as_ref()) {
        lib.objects.insert(name.clone(), Arc::new(p));
    }
}

/// Spawns `assets` under `root` and points Sora at the new bodies. Returns the old body.
#[allow(clippy::too_many_arguments)]
fn swap_bodies(
    commands: &mut Commands,
    root: Entity,
    s: &mut Sora,
    fighter: &mut Fighter,
    assets: &FormAssets,
    left: &mut LeftWeapon,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) {
    let old = s.body;
    let body = spawn_skinned(commands, root, assets.body.clone(), meshes, materials, images, bindposes, SCALE);
    let hand = commands.spawn((Transform::default(), Visibility::default(), ChildOf(body))).id();
    let weapon = spawn_skinned(commands, hand, assets.right.clone(), meshes, materials, images, bindposes, 1.0);
    commands.entity(hand).insert(HandSocket);
    let mut attack = vec![weapon, body];
    left.0 = assets.left.as_ref().map(|rig| {
        let socket = commands.spawn((Transform::default(), Visibility::default(), ChildOf(body), LeftSocket)).id();
        let w = spawn_skinned(commands, socket, rig.clone(), meshes, materials, images, bindposes, 1.0);
        attack.push(w);
        w
    });
    commands.entity(old).despawn();
    s.body = body;
    s.weapon = weapon;
    s.moves = assets.moves.clone();
    fighter.attack_bodies = attack;
    fighter.hurt_body = body;
    fighter.landed.clear();
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn drive_tick(
    mut commands: Commands,
    rules: Res<DriveRules>,
    data: Res<GameData>,
    disc: Res<Disc>,
    rigs: Res<Rigs>,
    world: Res<DriveWorld>,
    store: Store,
    mut growth: ResMut<Growth>,
    mut lib: Option<ResMut<EffectLib>>,
    mut hits: MessageReader<Hit>,
    mut changed: MessageWriter<FormChanged>,
    mut gauge: ResMut<DriveGauge>,
    mut sora_q: Query<(Entity, &mut Sora, &mut Fighter)>,
    mut anims: Query<&mut Animator>,
    gfx: Gfx,
) {
    let Store { mut state, mut queue, mut change, mut cache, mut base, mut left, mut abilities, battle: battle_flag } = store;
    let Gfx { mut meshes, mut materials, mut images, mut bindposes } = gfx;
    let Ok((root, mut s, mut fighter)) = sora_q.single_mut() else { return };
    let s = &mut *s;
    let st = &mut *state;
    st.ticks += 1;
    let tuning = rules.tuning;
    let battle = st.force_battle || battle_flag.active();

    // Hits Sora lands: drive gain in the normal gauge, lost time in a form (FUN_00178748; the
    // victim flags 0x10 / 0x800 it tests are not modelled, so every enemy counts as 0x10 only).
    for h in hits.read().filter(|h| h.attacker == root) {
        let Some(drain) = data.atkp(h.atkp).map(|a| a.drive_drain).filter(|&d| d != 0) else { continue };
        if st.in_form {
            st.timer.add(st.form, -(drain as f32) * tuning.form_hit_loss);
            info!("drive: landed hit (atkp {}, DriveDrain {drain}): form timer now {:.0}", h.atkp, st.timer.left);
        } else if change.0.is_none() {
            st.points.gain(drain as i32);
            info!("drive: landed hit (atkp {}, DriveDrain {drain}): gauge {} points + {}%", h.atkp, st.points.whole, st.points.fraction);
        }
    }

    // Requests.
    for req in std::mem::take(&mut queue.0) {
        match req {
            DriveRequest::Gain(n) => {
                if !st.in_form {
                    st.points.gain(n);
                    info!("drive: gain {n} -> {} points + {}%", st.points.whole, st.points.fraction);
                }
            }
            DriveRequest::Enter(chosen) => {
                let can_act = change.0.is_none() && s.hp > 0 && !matches!(s.state, State::Down | State::Blown | State::Viewer);
                let ctx = rules::EnterCtx { current_form: st.form, owned: st.owned, points: st.points.whole, donald: st.donald, goofy: st.goofy, locked: !can_act };
                // Anti is only entered by the roll; the harness may name it to look at the form.
                let forced_anti = chosen == form::ANTI;
                let row = if forced_anti { rules.form(chosen).cloned().ok_or(rules::Refusal::UnknownForm) } else { rules::check_enter(&rules.forms, chosen, &ctx) };
                let row = match row {
                    Ok(r) => r,
                    Err(e) => {
                        info!("drive: cannot enter {}: {e:?} (points {}, cost {:?})", form_name(chosen), st.points.whole, rules.form(chosen).map(|f| f.cost));
                        continue;
                    }
                };
                let roll = if forced_anti {
                    rules::Rolled { form: form::ANTI, counters: st.counters, gave_final: false }
                } else {
                    let ctx = rules::RollCtx {
                        chosen,
                        may_anti: rules::companions_ok(form::ANTI, st.donald, st.goofy),
                        battle,
                        final_owned: st.owned[form::FINAL as usize],
                        story_flag: true,
                        points: st.points.whole,
                        final_cost: rules.form(form::FINAL).map_or(u8::MAX, |f| f.cost),
                        anti_factor: 1,
                    };
                    rules::choose_form(&tuning, &rules.levels, &ctx, st.counters, &mut st.rng)
                };
                st.counters = roll.counters;
                if roll.gave_final {
                    st.owned[form::FINAL as usize] = true;
                }
                let entered = roll.form;
                let cost = rules.form(entered).map_or(row.cost, |f| f.cost);
                let before = st.points;
                st.points.spend(cost);
                st.uses[entered as usize] = (st.uses[entered as usize] + 1).min(999);
                info!(
                    "drive: enter {} (chose {}, cost {cost}, points {} -> {}, level {}, anti counter {}, final counter {})",
                    form_name(entered),
                    form_name(chosen),
                    before.whole,
                    st.points.whole,
                    st.level[entered as usize],
                    st.counters.anti,
                    st.counters.fin
                );
                if forced_anti {
                    info!("drive: Anti entered directly (harness); the game reaches it only through the roll");
                }
                // The load happens now; the swap waits for the old body's change motion.
                let key = (entered, world.0);
                if let std::collections::hash_map::Entry::Vacant(slot) = cache.0.entry(key) {
                    match load_assets(&disc, &rules, &rigs, entered, world.0) {
                        Some(a) => {
                            if let Some(l) = lib.as_deref_mut() {
                                register_effects(&disc, l, &a.body);
                                register_effects(&disc, l, &a.right);
                                if let Some(r) = &a.left {
                                    register_effects(&disc, l, r);
                                }
                            }
                            slot.insert(Arc::new(a));
                        }
                        None => {
                            warn!("drive: files of {} not loadable; the change is skipped", form_name(entered));
                            continue;
                        }
                    }
                }
                if base.0.is_none() {
                    base.0 = Some(Base { moves: s.moves.clone(), growth: *growth });
                }
                st.form = entered;
                let started = anims.get_many_mut([s.body, s.weapon]).ok().and_then(|[mut body, mut weapon]| {
                    let slot = body.rig.mset.slot(MOTION_LEAVE, kh2_anim::PlayerState::BattleWeapon)?;
                    body.restart(Some(slot), false, 4.0);
                    let armed = weapon.rig.mset.has(slot);
                    weapon.restart(armed.then_some(slot), false, 4.0);
                    Some(slot)
                });
                if let Some(slot) = started {
                    info!("drive: change motion {} (id {MOTION_LEAVE}, slot {slot}) on the old body", anims.get(s.body).map_or("-", |a| a.tag()));
                    s.state = State::FormChange;
                    s.motor.vel = Vec3::ZERO;
                    s.queued_attack = false;
                }
                change.0 = Some(ChangeTask { to: entered, phase: Phase::Leaving, ticks: 0, motion: started.is_some() });
                if started.is_none() {
                    info!("drive: the old body has no change motion; swapping at once");
                }
            }
            DriveRequest::Revert => {
                if !st.in_form || change.0.is_some() {
                    info!("drive: revert refused (form {}, changing {})", form_name(st.form), change.0.is_some());
                } else if !rules::can_revert(st.form, battle) {
                    info!("drive: revert refused: Anti form cannot be reverted by command while the battle flag is set");
                } else {
                    do_revert(&mut commands, root, s, &mut fighter, st, &base, &cache, &rigs, &mut growth, &mut abilities, &mut left, &mut changed, &mut meshes, &mut materials, &mut images, &mut bindposes, "command");
                }
            }
        }
    }

    // The change task.
    if let Some(task) = change.0.as_mut() {
        task.ticks += 1;
        s.invuln = s.invuln.max(2.0);
        let done = match task.phase {
            Phase::Leaving => !task.motion || anims.get(s.body).map_or(true, |a| a.finished) || task.ticks > CHANGE_TIMEOUT,
            // Tick 1 starts the arrival motion; it ends with the motion.
            Phase::Arriving => task.ticks > 1 && (anims.get(s.body).map_or(true, |a| a.finished) || task.ticks > CHANGE_TIMEOUT),
        };
        if done && task.phase == Phase::Leaving {
            let to = task.to;
            let key = (to, world.0);
            let Some(assets) = cache.0.get(&key).cloned() else {
                change.0 = None;
                return;
            };
            swap_bodies(&mut commands, root, s, &mut fighter, &assets, &mut left, &mut meshes, &mut materials, &mut images, &mut bindposes);
            // Per-form growth (the form's own growth ability only) and abilities (its fmlv grants).
            let level = st.level[to as usize];
            let g = rules::growth_level(&rules.levels, to, level);
            let mut next = Growth::default();
            match to {
                1 => next.high_jump = g,
                2 => next.quick_run = g,
                3 => next.aerial_dodge = g,
                4 => next.glide = g,
                _ => {}
            }
            *growth = next;
            let granted = rules::granted_abilities(&rules.levels, to, level);
            // The form's grants join the controller's ability block (Combo Plus and Air Combo Plus lengthen the chains).
            abilities.form = rules.granted_with_bits(to, level);
            st.timer =rules::FormTimer::start(&tuning, to, level);
            st.in_form = true;
            s.mp = data.sora_max_mp;
            info!(
                "drive: {} active, level {level}, timer {}/{} ({} points, drain {:.2}/tick in battle, {:.2} out of battle), growth level {g}, abilities granted {granted:?}, MP refilled to {}",
                form_name(to),
                st.timer.left,
                st.timer.total,
                st.timer.whole_points(&tuning),
                rules::drain_per_tick(&tuning, true, rules::form_boost(&tuning, &granted)),
                rules::drain_per_tick(&tuning, false, rules::form_boost(&tuning, &granted)),
                s.mp
            );
            changed.write(FormChanged { from: 0, to });
            task.phase = Phase::Arriving;
            task.ticks = 0;
            // Held in the form-change state until the arrival motion has played: the new animator
            // exists only after this system's commands apply.
            s.state = State::FormChange;
            return;
        }
        if done && task.phase == Phase::Arriving {
            change.0 = None;
            s.invuln = 0.0;
            s.state = if s.motor.grounded { State::Ground } else { State::Air };
            if !s.motor.grounded {
                s.motor.start_fall();
            }
        } else if task.phase == Phase::Arriving && task.ticks == 1 {
            let started = anims.get_many_mut([s.body, s.weapon]).ok().and_then(|[mut body, mut weapon]| {
                let slot = body.rig.mset.slot(MOTION_ARRIVE, kh2_anim::PlayerState::BattleWeapon)?;
                body.restart(Some(slot), false, 0.0);
                let armed = weapon.rig.mset.has(slot);
                weapon.restart(armed.then_some(slot), false, 0.0);
                info!("drive: arrival motion {} (id {MOTION_ARRIVE})", body.tag());
                Some(())
            });
            if started.is_none() {
                s.state = if s.motor.grounded { State::Ground } else { State::Air };
                change.0 = None;
                s.invuln = 0.0;
            }
        }
    }

    // The form timer.
    if st.in_form && change.0.is_none() {
        let granted = rules::granted_abilities(&rules.levels, st.form, st.level[st.form as usize]);
        let drain = rules::drain_per_tick(&tuning, battle, rules::form_boost(&tuning, &granted));
        st.timer.add(st.form, -drain);
        if st.ticks % 60 == 0 {
            info!("drive: {} timer {:.0}/{:.0} ({} points, battle {battle})", form_name(st.form), st.timer.left, st.timer.total, st.timer.whole_points(&tuning));
        }
        if st.timer.expired() {
            do_revert(&mut commands, root, s, &mut fighter, st, &base, &cache, &rigs, &mut growth, &mut abilities, &mut left, &mut changed, &mut meshes, &mut materials, &mut images, &mut bindposes, "timer ran out");
        }
    }

    // The HUD hook.
    let granted = rules::granted_abilities(&rules.levels, st.form, st.level[st.form as usize]);
    let drain = rules::drain_per_tick(&tuning, battle, rules::form_boost(&tuning, &granted)).max(f32::EPSILON);
    *gauge = DriveGauge {
        points: st.points.whole,
        max_points: st.points.max,
        fraction: st.points.fraction,
        form: st.form,
        form_level: if st.form == 0 { 0 } else { st.level[st.form as usize] },
        in_form: st.in_form,
        changing: change.0.is_some(),
        time_left: st.timer.left,
        time_total: st.timer.total,
        time_points: if st.in_form { st.timer.whole_points(&tuning) } else { 0 },
        seconds_left: if st.in_form { st.timer.left / drain / crate::data::LOGIC_HZ } else { 0.0 },
    };
}

/// Back to base Sora (`FUN_001b4c28(player, 0)`: the same change task with form 0; the old body
/// plays no motion). Restores the attack table, abilities and growth levels.
#[allow(clippy::too_many_arguments)]
fn do_revert(
    commands: &mut Commands,
    root: Entity,
    s: &mut Sora,
    fighter: &mut Fighter,
    st: &mut DriveState,
    base: &BaseSora,
    _cache: &FormCache,
    rigs: &Rigs,
    growth: &mut Growth,
    abilities: &mut Abilities,
    left: &mut LeftWeapon,
    changed: &mut MessageWriter<FormChanged>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    why: &str,
) {
    let Some(b) = &base.0 else { return };
    let from = st.form;
    let assets = FormAssets { body: rigs.sora.clone(), right: rigs.keyblade.clone(), left: None, moves: b.moves.clone(), has_weapon: true };
    swap_bodies(commands, root, s, fighter, &assets, left, meshes, materials, images, bindposes);
    abilities.form.clear();
    *growth = b.growth;
    st.in_form = false;
    st.form = 0;
    st.timer = Default::default();
    s.state = if s.motor.grounded { State::Ground } else { State::Air };
    if !s.motor.grounded {
        s.motor.start_fall();
    }
    info!("drive: revert from {} ({why}); gauge mode normal, points {} + {}%", form_name(from), st.points.whole, st.points.fraction);
    changed.write(FormChanged { from, to: 0 });
}

/// Logs form changes (other systems read `FormChanged` for their own effects, sounds and camera).
fn log_changes(mut changes: MessageReader<FormChanged>) {
    for c in changes.read() {
        info!("drive: form changed {} -> {}", form_name(c.from), form_name(c.to));
    }
}

/// The left weapon plays the body's slot and frame, as the right one does in `sora::sync_weapon`.
fn sync_left(sora: Query<&Sora>, left: Res<LeftWeapon>, mut anims: Query<&mut Animator>) {
    let (Ok(s), Some(l)) = (sora.single(), left.0) else { return };
    let Ok([body, mut weapon]) = anims.get_many_mut([s.body, l]) else { return };
    let slot = body.slot.filter(|&x| weapon.rig.mset.has(x));
    if weapon.slot != slot {
        weapon.restart(slot, body.looping, 0.0);
    }
    weapon.frame = body.frame;
    weapon.prev_frame = body.prev_frame;
    weapon.looping = body.looping;
    weapon.speed = body.speed;
}

/// Keeps the left socket on the left-hand bone.
fn follow_left(sora: Query<&Sora>, anims: Query<&Animator>, mut socket: Query<&mut Transform, With<LeftSocket>>) {
    let (Ok(s), Ok(mut t)) = (sora.single(), socket.single_mut()) else { return };
    if let Ok(a) = anims.get(s.body) {
        *t = Transform::from_matrix(a.pose.get(LEFT_HAND).copied().unwrap_or(Mat4::IDENTITY));
    }
}

/// The left weapon shows with the right one; a form without a weapon (Anti) never shows it.
/// Runs after the controller, which sets the right weapon's visibility from the battle state.
fn weapon_visibility(
    sora: Query<&Sora>,
    state: Res<DriveState>,
    left: Res<LeftWeapon>,
    cache: Res<FormCache>,
    world: Res<DriveWorld>,
    mut vis: Query<&mut Visibility>,
) {
    let Ok(s) = sora.single() else { return };
    let armed = state.form == 0 || cache.0.get(&(state.form, world.0)).is_none_or(|a| a.has_weapon);
    if !armed && let Ok(mut v) = vis.get_mut(s.weapon) {
        *v = Visibility::Hidden;
    }
    let shown = vis.get(s.weapon).copied().unwrap_or(Visibility::Hidden);
    if let Some(l) = left.0 && let Ok(mut v) = vis.get_mut(l) {
        *v = shown;
    }
}

#[cfg(test)]
mod tests {
    use super::rules::*;
    use super::*;

    fn rules_from_disc() -> Option<DriveRules> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        let mut a = kh2_archive::Archive::open(&iso).map_err(|e| eprintln!("skipped: {iso}: {e}")).ok()?;
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).ok()?.rows;
        let cmds = kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok()?.rows;
        let fmlv = kh2_tables::load::<t::Fmlv>(&mut a, &t::FMLV).ok()?.rows;
        let objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()?.rows;
        let memt = player::load_memt(&mut a).ok()?;
        let sstm = player::load_sstm(&mut a).ok()?;
        let wmst = parse_wmst(&kh2_tables::subfile(&mut a, &t::WMST).ok()?);
        let went = kh2_tables::subfile(&mut a, &t::WENT).ok()?;
        let key = items.iter().find(|i| i.id == KINGDOM_KEY)?;
        Some(DriveRules {
            forms: player::drive_forms(&items, &cmds, &memt),
            levels: FormLevels::from_rows(&fmlv),
            tuning: Tuning::from_sstm(&sstm),
            memt,
            objs,
            wmst,
            went,
            ability_bits: player::abilities(&items).into_iter().map(|a| (a.id, a.bit)).collect(),
            key_slot: key.flag1 as u16 | (key.rank as u16) << 8,
        })
    }

    fn ctx(points: u8) -> EnterCtx {
        EnterCtx { current_form: 0, owned: [false, true, true, true, true, false], points, donald: true, goofy: true, locked: false }
    }

    #[test]
    fn gauge_carry_borrow_and_spend() {
        let mut p = Points { whole: 0, fraction: 90, max: 3 };
        p.gain(20);
        assert_eq!((p.whole, p.fraction), (1, 10));
        p.gain(-30);
        assert_eq!((p.whole, p.fraction), (0, 80));
        p.gain(-100);
        assert_eq!((p.whole, p.fraction), (0, 0));
        // Filling the gauge leaves the fraction at 100.
        let mut p = Points { whole: 2, fraction: 50, max: 3 };
        p.gain(60);
        assert_eq!((p.whole, p.fraction), (3, 100));
        // Paying from a full gauge drops the fraction; a partial gauge keeps it.
        p.spend(3);
        assert_eq!((p.whole, p.fraction), (0, 0));
        let mut p = Points { whole: 2, fraction: 40, max: 3 };
        p.spend(1);
        assert_eq!((p.whole, p.fraction), (1, 40));
        p.spend(5);
        assert_eq!((p.whole, p.fraction), (0, 0));
    }

    #[test]
    fn timer_clamps_and_anti_inverts() {
        let t = Tuning { point_units: 100.0, field_drain_scale: 0.5, form_hit_loss: 1.0, anti_level_basis: 5, anti_drop: 5, anti_damage: 1.5, final_base: 25.0, final_growth: 2.0, final_cap: 100.0, final_anti_drop: 10, form_boost: 0.8 };
        let mut tm = FormTimer::start(&t, form::VALOR, 1);
        assert_eq!((tm.left, tm.total), (400.0, 400.0));
        assert_eq!(tm.whole_points(&t), 3);
        tm.add(form::VALOR, 50.0);
        assert_eq!(tm.left, 400.0);
        tm.add(form::VALOR, -150.0);
        assert_eq!((tm.left, tm.whole_points(&t)), (250.0, 2));
        tm.add(form::ANTI, 100.0);
        assert_eq!(tm.left, 150.0);
        tm.add(form::VALOR, -1000.0);
        assert!(tm.expired());
        // Anti's timer length ignores the form level.
        assert_eq!(FormTimer::start(&t, form::ANTI, 7).total, 100.0 * 6.0);
    }

    #[test]
    fn companions_and_entry_rules() {
        assert_eq!(companions(1), (false, true));
        assert_eq!(companions(2), (true, false));
        assert_eq!(companions(4), (true, true));
        assert!(!companions_ok(1, true, false));
        assert!(companions_ok(1, false, true));
        assert!(!companions_ok(0, true, true));
        assert!(!can_revert(form::ANTI, true));
        assert!(can_revert(form::ANTI, false) && can_revert(form::VALOR, true) && !can_revert(0, false));
    }

    #[test]
    fn roll_rules() {
        let t = Tuning { point_units: 600.0, field_drain_scale: 0.5, form_hit_loss: 1.2, anti_level_basis: 5, anti_drop: 5, anti_damage: 1.5, final_base: 25.0, final_growth: 2.0, final_cap: 100.0, final_anti_drop: 10, form_boost: 0.8 };
        assert_eq!(final_chance(&t, 0), 25.0);
        assert_eq!(final_chance(&t, 1), 50.0);
        assert_eq!(final_chance(&t, 5), 100.0);
        assert_eq!(percent_int(u32::MAX), 99);
        assert_eq!(percent_int(0), 0);
        assert!(percent_float(u32::MAX) <= 100.0 && percent_float(0) == 0.0);
        let mut rng = Lcg(1);
        assert_eq!(rng.step(), 0x10dcd);
    }

    #[test]
    fn memt_merge_follows_world_and_flags() {
        let mut base = [0u16; 24];
        base[8] = 84;
        base[12] = 85;
        let mut nm = [0u16; 24];
        nm[0] = 14;
        nm[12] = 998;
        let mut gated = [0u16; 24];
        gated[1] = 4227;
        gated[12] = 7;
        let m = Memt { version: 5, rows: vec![base, nm, gated], trailing: vec![] };
        let r = memt_merge(&m, 2, |_| false);
        assert_eq!((r[8], r[12]), (84, 85));
        let r = memt_merge(&m, 14, |_| false);
        assert_eq!((r[8], r[12]), (84, 998));
        // A record behind a story flag applies only when the flag is set.
        let r = memt_merge(&m, 2, |f| f == 4227);
        assert_eq!(r[12], 7);
        assert_eq!(merged_form_object(&r, 1), Some(7));
        assert_eq!(merged_form_object(&r, 0), None);
    }

    #[test]
    fn costs_levels_and_drain_match_the_disc() {
        let Some(r) = rules_from_disc() else { return };
        let cost = |f| r.form(f).unwrap().cost;
        // Entering needs the cost in points, the owned item and the companions.
        for f in 1..=4u8 {
            assert!(matches!(check_enter(&r.forms, f, &ctx(cost(f) - 1)), Err(Refusal::NotEnoughPoints { .. })), "form {f}");
            assert!(check_enter(&r.forms, f, &ctx(cost(f))).is_ok(), "form {f}");
        }
        let mut c = ctx(9);
        c.owned[3] = false;
        assert_eq!(check_enter(&r.forms, 3, &c), Err(Refusal::NotOwned));
        c.goofy = false;
        assert_eq!(check_enter(&r.forms, 1, &c), Err(Refusal::Companions));
        c = ctx(9);
        c.current_form = 1;
        assert_eq!(check_enter(&r.forms, 2, &c), Err(Refusal::InForm));
        // Level cap from owned forms.
        assert_eq!(player::form_level_cap(2), 4);
        assert_eq!(player::form_level_cap(4), 7);
        // Timer: one drive point is 600 units; level L lasts (L + 3) points at 1 unit per tick in
        // battle and half that rate out of it.
        let t = r.tuning;
        for lv in 1..=7u8 {
            let tm = FormTimer::start(&t, form::VALOR, lv);
            assert_eq!(tm.total, t.point_units * (lv as f32 + 3.0));
            let mut x = tm;
            let mut ticks = 0;
            while !x.expired() {
                x.add(form::VALOR, -drain_per_tick(&t, true, 1.0));
                ticks += 1;
            }
            assert_eq!(ticks as f32, (tm.total / drain_per_tick(&t, true, 1.0)).ceil());
            assert!(drain_per_tick(&t, false, 1.0) < drain_per_tick(&t, true, 1.0));
        }
        // Form Boost shortens the drain once per granted copy: Final grants it at levels 4 and 6.
        assert_eq!(form_boost(&t, &granted_abilities(&r.levels, form::FINAL, 3)), 1.0);
        assert_eq!(form_boost(&t, &granted_abilities(&r.levels, form::FINAL, 4)), t.form_boost);
        assert!(form_boost(&t, &granted_abilities(&r.levels, form::FINAL, 6)) < t.form_boost);
        // Growth abilities rise with the display level (1..=4).
        assert_eq!(growth_level(&r.levels, form::FINAL, 1), 1);
        assert_eq!(growth_level(&r.levels, form::FINAL, 7), 4);
        assert_eq!(growth_level(&r.levels, form::ANTI, 1), 0);
        // The Anti roll counter only ever drops by the tuned amounts and never below 0.
        let mut rng = Lcg(12345);
        let k = Counters { anti: 100, fin: 0 };
        let rc = RollCtx { chosen: form::FINAL, may_anti: true, battle: true, final_owned: false, story_flag: true, points: 9, final_cost: cost(4), anti_factor: 1 };
        assert_eq!(choose_form(&t, &r.levels, &rc, k, &mut rng).counters.anti, 100 - t.final_anti_drop);
        // A fresh counter has no Anti chance: choosing Valor always gives Valor.
        let rc = RollCtx { chosen: form::VALOR, ..rc };
        for _ in 0..50 {
            assert_eq!(choose_form(&t, &r.levels, &rc, Counters::default(), &mut rng).form, form::VALOR);
        }
    }

    #[test]
    fn form_files_follow_memt_objentry_and_wmst() {
        let Some(r) = rules_from_disc() else { return };
        let valor = r.files(form::VALOR, 2).unwrap();
        assert_eq!((valor.model.as_str(), valor.mset.as_str()), ("P_EX100_BTLF", "P_EX100_BTLF"));
        // Valor, Master and Final hold two keyblades, Wisdom one, Anti none.
        assert!(valor.left_mset.is_some() && valor.right_mset.is_some());
        assert!(r.files(form::WISDOM, 2).unwrap().left_mset.is_none());
        let anti = r.files(form::ANTI, 2).unwrap();
        assert!(anti.right_mset.is_none() && anti.weapon_model.is_none());
        assert_eq!(valor.weapon_model.as_deref(), Some("W_EX010"));
        // World outfits: the memt records of worlds 17, 13 and 14 swap in the _TR, _WI and _NM objects.
        assert_eq!(r.files(form::VALOR, 17).unwrap().model, "P_EX100_TR_BTLF");
        assert_eq!(r.files(form::FINAL, 13).unwrap().model, "P_EX100_WI_ULTF");
        assert_eq!(r.files(form::MASTER, 14).unwrap().model, "P_EX100_NM_TRIF");
        // The form's shared motion set and its own attack table.
        assert_eq!(r.files(form::VALOR, 17).unwrap().mset, "P_EX100_BTLF");
        assert_ne!(r.files(form::VALOR, 17).unwrap().moveset, valor.moveset);
        for f in 1..=5 {
            assert!(r.files(f, 2).is_some(), "form {f}");
        }
    }
}
