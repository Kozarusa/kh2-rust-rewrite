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

use crate::abilities::Abilities;
use crate::assets::Disc;
use crate::combat::{Fighter, Hit};
use crate::data::{GameData, KINGDOM_KEY};
use crate::fx::EffectLib;
use crate::schedule::{Frame, Sim};
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::sora::{self, Battle, Rigs, Sora, State, WeaponCue};
use crate::SCALE;

#[path = "hud_drive.rs"]
mod hud_drive;
pub use hud_drive::DriveGauge;

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
    /// `item.Type` of Sora's keyblades (the type `FUN_001a20b0(2)` looks for).
    pub const KEYBLADE_TYPE: u8 = 2;

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
    /// flag (`FUN_001903d8`) is clear, times the ability block's form multiplier (`+0x3c`,
    /// `abilities::Block::form_drain`: Form Boost per copy, the player's support copies included).
    pub fn drain_per_tick(t: &Tuning, battle: bool, form_drain: f32) -> f32 {
        FRAME_UNIT * if battle { 1.0 } else { t.field_drain_scale } * form_drain
    }

    /// The form's second keyblade (`FUN_001a2010`, run when a form item is added to the inventory,
    /// `FUN_001a1a18` type 21): for Valor, Master and Final, while the form record's weapon word
    /// is 0, `FUN_001a20b0(2)` takes the first keyblade item (type 2) in table order that the
    /// inventory holds, removes one copy and stores its id in the record. Wisdom and Anti keep 0.
    /// `inventory` is a multiset of item ids; returns the item taken, 0 for none.
    pub fn take_form_keyblade(items: &[t::Item], inventory: &mut Vec<u16>, form: u8) -> u16 {
        if !matches!(form, 1 | 3 | 4) {
            return 0;
        }
        for it in items.iter().filter(|i| i.r#type == KEYBLADE_TYPE) {
            if let Some(pos) = inventory.iter().position(|&x| x == it.id) {
                inventory.remove(pos);
                return it.id;
            }
        }
        0
    }

    /// Anti form multiplies the damage Sora deals by `sstm[0x94]` (`FUN_001a0610`): in the generic
    /// damage branch, not for the percentage-of-HP and fixed types (5 and 6). The game applies it to
    /// the damage after its element and defence steps and before its last two adjustments
    /// (`FUN_001a0a18`, `FUN_001a0aa8`, not ported).
    pub fn outgoing_damage(t: &Tuning, form: u8, atk_type: u8, dmg: i32) -> i32 {
        if form == player::form::ANTI && !matches!(atk_type, 5 | 6) { (dmg as f32 * t.anti_damage) as i32 } else { dmg }
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

    /// Object id of the base body in a merged record (`FUN_001cf2c8` with form 0: byte offset
    /// 0x10 + 2 * outfit, the outfit being the low 5 bits of byte 0 of the world's save block at
    /// `0x340c9c + 4 * world`, `FUN_001a71e8`). A new game has 0 there (inferred: nothing in the
    /// paths read writes it before play); the app takes it from `KH2_DRIVE_OUTFIT`.
    pub fn merged_base_object(rec: &[u16; 24], outfit: u8) -> Option<u16> {
        (outfit < 4).then(|| rec[8 + outfit as usize]).filter(|&o| o != 0)
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
    items: Vec<t::Item>,
    wmst: Vec<[String; 2]>,
    went: Vec<u8>,
    /// The equipped keyblade item (Kingdom Key; the party record's weapon word, hand 0 of every
    /// player object through `FUN_0016ee98`).
    key_item: u16,
}

/// One weapon of a player object: its model and, when the moveset's `wmst` row names one for the
/// hand, its motion set (a weapon without one stays in its rest pose, `FUN_00184190`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponFiles {
    pub model: String,
    pub mset: Option<String>,
}

/// What one player object (base Sora or a form) is made of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormFiles {
    pub model: String,
    pub mset: String,
    pub moveset: u16,
    /// The outfit index used (base body only).
    pub outfit: u8,
    /// The weapons by hand (0 right, 1 left); `None` when the hand holds nothing.
    pub hands: [Option<WeaponFiles>; 2],
}

impl FormFiles {
    /// The weapons in hand order with their hand index.
    pub fn weapons(&self) -> impl Iterator<Item = (usize, &WeaponFiles)> {
        self.hands.iter().enumerate().filter_map(|(h, w)| Some((h, w.as_ref()?)))
    }
}

impl DriveRules {
    pub fn load(disc: &Disc) -> Option<DriveRules> {
        let mut a = disc.0.lock().ok()?;
        Self::from_archive(&mut a)
    }

    pub fn from_archive(a: &mut kh2_archive::Archive) -> Option<DriveRules> {
        let items = kh2_tables::load::<t::Item>(a, &t::ITEM).ok()?.rows;
        let cmds = kh2_tables::load::<t::Cmd>(a, &t::CMD).ok()?.rows;
        let fmlv = kh2_tables::load::<t::Fmlv>(a, &t::FMLV).ok()?.rows;
        let objs = kh2_tables::load::<t::Objentry>(a, &t::OBJENTRY).ok()?.rows;
        let memt = player::load_memt(a).ok()?;
        let sstm = player::load_sstm(a).ok()?;
        let wmst = rules::parse_wmst(&kh2_tables::subfile(a, &t::WMST).ok()?);
        let went = kh2_tables::subfile(a, &t::WENT).ok()?;
        items.iter().find(|i| i.id == KINGDOM_KEY)?;
        Some(DriveRules {
            forms: player::drive_forms(&items, &cmds, &memt),
            levels: FormLevels::from_rows(&fmlv),
            tuning: rules::Tuning::from_sstm(&sstm),
            memt,
            objs,
            items,
            wmst,
            went,
            key_item: KINGDOM_KEY,
        })
    }

    pub fn form(&self, form: u8) -> Option<&DriveForm> {
        self.forms.iter().find(|f| f.form == form)
    }

    pub fn items(&self) -> &[t::Item] {
        &self.items
    }

    /// The `went` slot of an item: its `Flag1 | Rank << 8` word (`FUN_001845b0` reads item `+4`).
    fn item_slot(&self, item: u16) -> Option<u16> {
        self.items.iter().find(|i| i.id == item).map(|i| i.flag1 as u16 | (i.rank as u16) << 8)
    }

    /// Files of a player object: `obj` is its objentry id; `hand1_item` the form record's weapon
    /// word (0 for the base body, which only has hand 0: `FUN_00176f88`). Each hand's weapon is
    /// the moveset's `went` entry for the item (`FUN_0016ef28`, `FUN_001845b0`) and its motion set
    /// the `wmst` name of that hand (`FUN_00184640`).
    fn object_files(&self, obj: u16, outfit: u8, hand1_item: u16) -> Option<FormFiles> {
        let o = self.objs.iter().find(|o| o.object_id == obj as u32)?;
        let ms = o.neo_moveset as usize;
        let mset_of = |hand: usize| self.wmst.get(ms).and_then(|n| n.get(hand)).filter(|s| !s.is_empty()).cloned();
        let weapon = |item: u16, hand: usize| -> Option<WeaponFiles> {
            let id = rules::weapon_object(&self.went, ms, self.item_slot(item)?)?;
            let w = self.objs.iter().find(|w| w.object_id == id)?;
            Some(WeaponFiles { model: w.model_name.clone(), mset: mset_of(hand) })
        };
        Some(FormFiles {
            model: o.model_name.clone(),
            mset: o.animation_name.trim_end_matches(".mset").to_string(),
            moveset: o.neo_moveset,
            outfit,
            hands: [weapon(self.key_item, 0), (hand1_item != 0).then(|| weapon(hand1_item, 1)).flatten()],
        })
    }

    /// The merged `memt` record of a world (`FUN_001a6ff0`; no story flag is set).
    fn merged(&self, world: u16) -> [u16; 24] {
        rules::memt_merge(&self.memt, world, |_| false)
    }

    /// Files of the base body in a world: the merged record's outfit word (`FUN_001cf2c8`, form 0).
    pub fn base_files(&self, world: &DriveWorld) -> Option<FormFiles> {
        let obj = rules::merged_base_object(&self.merged(world.id), world.outfit)?;
        self.object_files(obj, world.outfit, 0)
    }

    /// Files of a form in a world (`world` selects the `_NM`, `_TR`, `_WI` variants through the
    /// memt merge), with the form's second keyblade item (`DriveState::form_weapon`).
    pub fn files(&self, form: u8, world: u16, hand1_item: u16) -> Option<FormFiles> {
        let obj = rules::merged_form_object(&self.merged(world), form)?;
        self.object_files(obj, 0, hand1_item)
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
    /// The forms' second keyblade (form record word 0, `FUN_0016ee98` hand 1), set by
    /// `rules::take_form_keyblade` when the form is obtained; 0 for none.
    pub form_weapon: [u16; 6],
    /// Spare keyblades in the inventory (item ids, one per copy) after the forms took theirs.
    pub inventory: Vec<u16>,
    ticks: u32,
}

impl DriveState {
    /// The form field of the current player object (`+0xb50`): the entered form once the body has
    /// been swapped, else 0 (the old object stays the base one through the change).
    pub fn object_form(&self) -> u8 {
        if self.in_form { self.form } else { 0 }
    }

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
        // The forms' second keyblades come from the inventory when the form items are obtained
        // (the harness obtains the owned forms in id order). The app has no inventory: the
        // default gives one spare copy of the equipped keyblade per form that takes one, which
        // keeps the Kingdom Key in both hands; `KH2_DRIVE_INVENTORY=` (empty) is the new-game
        // state with no spare keyblade (the forms then hold one), `=id,id` any list.
        let mut inventory: Vec<u16> = match env("KH2_DRIVE_INVENTORY") {
            Some(v) => v.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
            None => (1..=4u8).filter(|&f| owned[f as usize] && matches!(f, 1 | 3 | 4)).map(|_| rules.key_item).collect(),
        };
        let mut form_weapon = [0u16; 6];
        for f in 1..=4u8 {
            if owned[f as usize] {
                form_weapon[f as usize] = rules::take_form_keyblade(rules.items(), &mut inventory, f);
            }
        }
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
            form_weapon,
            inventory,
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

/// The world the player objects are looked up for (the `memt` merge key: 13, 14 and 17 select the
/// `_WI`, `_NM`, `_TR` objects; 2 is Twilight Town) and its outfit index (word 8 + outfit of the
/// merged record for the base body; a new game has 0).
#[derive(Resource, Clone, Copy, Debug)]
pub struct DriveWorld {
    pub id: u16,
    pub outfit: u8,
}

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
    /// The weapons with their hand (0 right, 1 left), in hand order.
    weapons: Vec<(usize, Arc<Rig>)>,
    moves: Arc<Moveset>,
}

#[derive(Resource, Default)]
struct FormCache(HashMap<(u8, u16), Arc<FormAssets>>);

/// What base Sora was before the first form, for the revert (the body and keyblade rigs are
/// `Rigs`, which `sora::setup` fills from the same merge).
struct Base {
    moves: Arc<Moveset>,
}

#[derive(Resource, Default)]
struct BaseSora(Option<Base>);

/// The drive system's own state, as one system parameter.
#[derive(SystemParam)]
struct Store<'w> {
    state: ResMut<'w, DriveState>,
    queue: ResMut<'w, DriveQueue>,
    change: ResMut<'w, Change>,
    cache: ResMut<'w, FormCache>,
    base: ResMut<'w, BaseSora>,
    /// The ability system (absent when its tables are unreadable: then no form list applies).
    abilities: Option<ResMut<'w, Abilities>>,
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
        let outfit = std::env::var("KH2_DRIVE_OUTFIT").ok().and_then(|v| v.parse::<u8>().ok()).unwrap_or(0).min(3);
        let enter = std::env::var("KH2_DRIVE").ok().and_then(|v| v.parse::<u8>().ok()).map(|f| (env_f("KH2_DRIVE_AT").unwrap_or(4.0), f));
        let state = DriveState::from_env(&rules);
        info!("drive: form keyblades (hand 1) {:?}, inventory left {:?}", &state.form_weapon[1..=4], state.inventory);
        app.insert_resource(state)
            .insert_resource(rules)
            .insert_resource(DriveWorld { id: world, outfit })
            .insert_resource(Harness { enter, revert_at: env_f("KH2_DRIVE_REVERT_AT"), chosen: enter.map_or(1, |e| e.1) })
            .init_resource::<DriveQueue>()
            .init_resource::<Change>()
            .init_resource::<FormCache>()
            .init_resource::<BaseSora>()
            .add_message::<FormChanged>()
            .add_plugins(hud_drive::HudDrivePlugin)
            .add_systems(Update, input.after(Frame::Pad))
            .add_systems(FixedUpdate, (drive_tick.in_set(Sim::Weapon).before(sora::sync_weapon), log_changes.in_set(Sim::Sounds)));
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

/// A model with its motion set (`obj/<model>.mdlx`, `obj/<mset>.mset`) and its effect file.
pub(crate) fn read_rig(disc: &Disc, model: &str, mset: &str) -> Option<Arc<Rig>> {
    let m = disc.read(&format!("obj/{model}.mdlx")).and_then(|d| kh2_model::mdlx(&d))?;
    let s = disc.read(&format!("obj/{mset}.mset")).and_then(|d| kh2_anim::Mset::parse(&d))?;
    let apdx = format!("obj/{model}.apdx");
    let apdx = disc.read(&apdx).is_some().then_some(apdx);
    Some(Arc::new(Rig { model: m, mset: s, apdx }))
}

/// A weapon: its model with the hand's motion set, or with no motions at all when the `wmst` row
/// names none (the game then loads none and the weapon keeps its rest pose, `FUN_00184190`).
pub(crate) fn weapon_rig(disc: &Disc, w: &WeaponFiles) -> Option<Arc<Rig>> {
    match &w.mset {
        Some(s) => read_rig(disc, &w.model, s),
        None => {
            let m = disc.read(&format!("obj/{}.mdlx", w.model)).and_then(|d| kh2_model::mdlx(&d))?;
            let apdx = format!("obj/{}.apdx", w.model);
            let apdx = disc.read(&apdx).is_some().then_some(apdx);
            Some(Arc::new(Rig { model: m, mset: kh2_anim::Mset { slots: Vec::new() }, apdx }))
        }
    }
}

fn load_assets(disc: &Disc, rules: &DriveRules, form: u8, world: u16, hand1_item: u16) -> Option<FormAssets> {
    let f = rules.files(form, world, hand1_item)?;
    let body = read_rig(disc, &f.model, &f.mset)?;
    let moves = {
        let mut a = disc.0.lock().ok()?;
        kh2_tables::subfile(&mut a, &t::PTYA).ok().and_then(|b| Moveset::parse(&b, f.moveset as usize))?
    };
    let mut weapons = Vec::new();
    for (hand, w) in f.weapons() {
        match weapon_rig(disc, w) {
            Some(r) => weapons.push((hand, r)),
            None => warn!("drive: weapon {} of hand {hand} not loadable; the hand stays empty", w.model),
        }
    }
    let hand = |h: usize| f.hands[h].as_ref().map_or("none".to_string(), |w| format!("{} ({})", w.model, w.mset.as_deref().unwrap_or("no motion set")));
    info!("drive: loaded {} (mset {}, attack table {}, hand 0 {}, hand 1 {}; hand 1 item {hand1_item})", f.model, f.mset, f.moveset, hand(0), hand(1));
    Some(FormAssets { body, weapons, moves: Arc::new(moves) })
}

/// Registers a rig's effect file with the effect library so its motion effects run.
pub(crate) fn register_effects(disc: &Disc, lib: &mut EffectLib, rig: &Rig) {
    let Some(name) = &rig.apdx else { return };
    if lib.objects.contains_key(name) {
        return;
    }
    let common = disc.read("effect/texcommon.dpd").and_then(|b| kh2_fx::Dpd::parse(&b).ok());
    if let Some(p) = crate::fx::load(disc, name, 100 + lib.objects.len(), common.as_ref()) {
        lib.objects.insert(name.clone(), Arc::new(p));
    }
}

/// Spawns `assets` under `root` (the body, then its weapons on their hand sockets) and points Sora
/// and his fighter at the new entities; the old body goes with its sockets and weapons.
#[allow(clippy::too_many_arguments)]
fn swap_bodies(
    commands: &mut Commands,
    root: Entity,
    s: &mut Sora,
    fighter: &mut Fighter,
    assets: &FormAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) {
    let old = s.body;
    let body = spawn_skinned(commands, root, assets.body.clone(), meshes, materials, images, bindposes, SCALE);
    let weapons = sora::spawn_weapons(commands, body, &assets.weapons, meshes, materials, images, bindposes);
    commands.entity(old).despawn();
    s.body = body;
    s.weapons = weapons.clone();
    s.moves = assets.moves.clone();
    fighter.attack_bodies = weapons.into_iter().chain([body]).collect();
    fighter.hurt_body = body;
    fighter.landed.clear();
}

/// Starts motion `id` (battle-weapon variant) on the body and, where their motion sets have the
/// slot, on the weapons. Returns the slot.
fn start_motion(s: &Sora, anims: &mut Query<&mut Animator>, id: usize, blend: f32) -> Option<usize> {
    let slot = {
        let mut body = anims.get_mut(s.body).ok()?;
        let slot = body.rig.mset.slot(id, kh2_anim::PlayerState::BattleWeapon)?;
        body.restart(Some(slot), false, blend);
        slot
    };
    let mut cue = WeaponCue::default();
    cue.restart(Some(slot), false, blend);
    cue.apply(&s.weapons, anims);
    Some(slot)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn drive_tick(
    mut commands: Commands,
    rules: Res<DriveRules>,
    data: Res<GameData>,
    disc: Res<Disc>,
    rigs: Res<Rigs>,
    world: Res<DriveWorld>,
    store: Store,
    mut lib: Option<ResMut<EffectLib>>,
    mut hits: MessageReader<Hit>,
    mut changed: MessageWriter<FormChanged>,
    mut gauge: ResMut<DriveGauge>,
    mut sora_q: Query<(Entity, &mut Sora, &mut Fighter)>,
    mut anims: Query<&mut Animator>,
    gfx: Gfx,
) {
    let Store { mut state, mut queue, mut change, mut cache, mut base, mut abilities, battle: battle_flag } = store;
    let Gfx { mut meshes, mut materials, mut images, mut bindposes } = gfx;
    let Ok((root, mut s, mut fighter)) = sora_q.single_mut() else { return };
    let s = &mut *s;
    let st = &mut *state;
    st.ticks += 1;
    let tuning = rules.tuning;
    let battle = st.force_battle || battle_flag.active();
    // The block's form multiplier (+0x3c): Form Boost per copy, 1.0 without the ability system.
    let form_drain = |ab: &Option<ResMut<Abilities>>| ab.as_deref().map_or(1.0, |a| a.block().form_drain);

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
                let key = (entered, world.id);
                if let std::collections::hash_map::Entry::Vacant(slot) = cache.0.entry(key) {
                    match load_assets(&disc, &rules, entered, world.id, st.form_weapon[entered as usize]) {
                        Some(a) => {
                            if let Some(l) = lib.as_deref_mut() {
                                register_effects(&disc, l, &a.body);
                                for (_, w) in &a.weapons {
                                    register_effects(&disc, l, w);
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
                    base.0 = Some(Base { moves: s.moves.clone() });
                }
                st.form = entered;
                let started = start_motion(s, &mut anims, MOTION_LEAVE, 4.0);
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
                    do_revert(&mut commands, root, s, &mut fighter, st, &base, &rigs, &mut abilities, &mut changed, &mut meshes, &mut materials, &mut images, &mut bindposes, "command");
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
            let key = (to, world.id);
            let Some(assets) = cache.0.get(&key).cloned() else {
                change.0 = None;
                return;
            };
            swap_bodies(&mut commands, root, s, &mut fighter, &assets, &mut meshes, &mut materials, &mut images, &mut bindposes);
            // The form record's ability list (FUN_001c0e50 applies its 16 slots, and only the
            // player's support abilities beside them): the fmlv grants up to the level and the
            // growth ability of the form's chain at the display level (FUN_001d3da8 keeps that
            // slot at the current level). The ability system derives the block from it.
            let level = st.level[to as usize];
            let g = rules::growth_level(&rules.levels, to, level);
            let mut list = rules::granted_abilities(&rules.levels, to, level);
            let block = match abilities.as_deref_mut() {
                Some(ab) => {
                    if let Some(id) = (g > 0).then(|| ab.rules().growth_ability(to as usize - 1, g)).flatten() {
                        list.push(id);
                    }
                    ab.set_form(Some(list.clone()));
                    let d = ab.block().deltas();
                    if d.is_empty() { "unchanged".to_string() } else { d.join("; ") }
                }
                None => "ability system off".to_string(),
            };
            st.timer = rules::FormTimer::start(&tuning, to, level);
            st.in_form = true;
            // FUN_001a04b0 at the timer start: HP raised by the maximum (FUN_001a0140 -> the
            // player's method +0x78 = FUN_001792f0 -> FUN_00176c98 -> FUN_00186238, clamped by
            // FUN_001a0100) and MP set to its maximum (FUN_001a0240 -> FUN_001a0210).
            let (hp_before, mp_before) = (s.hp, s.mp);
            s.hp = s.stats.max_hp;
            s.mp = data.sora_max_mp;
            let fd = form_drain(&abilities);
            info!(
                "drive: {} active, level {level}, timer {}/{} ({} points, drain {:.2}/tick in battle, {:.2} out of battle), form list {list:?}, stat block {block}, HP {hp_before} -> {}, MP {mp_before} -> {}",
                form_name(to),
                st.timer.left,
                st.timer.total,
                st.timer.whole_points(&tuning),
                rules::drain_per_tick(&tuning, true, fd),
                rules::drain_per_tick(&tuning, false, fd),
                s.hp,
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
            let started = start_motion(s, &mut anims, MOTION_ARRIVE, 0.0);
            if started.is_some() {
                info!("drive: arrival motion {} (id {MOTION_ARRIVE})", anims.get(s.body).map_or("-", |a| a.tag()));
            }
            if started.is_none() {
                s.state = if s.motor.grounded { State::Ground } else { State::Air };
                change.0 = None;
                s.invuln = 0.0;
            }
        }
    }

    // The form timer.
    if st.in_form && change.0.is_none() {
        let drain = rules::drain_per_tick(&tuning, battle, form_drain(&abilities));
        st.timer.add(st.form, -drain);
        if st.ticks % 60 == 0 {
            info!("drive: {} timer {:.0}/{:.0} ({} points, battle {battle}, drain {drain:.2}/tick)", form_name(st.form), st.timer.left, st.timer.total, st.timer.whole_points(&tuning));
        }
        if st.timer.expired() {
            do_revert(&mut commands, root, s, &mut fighter, st, &base, &rigs, &mut abilities, &mut changed, &mut meshes, &mut materials, &mut images, &mut bindposes, "timer ran out");
        }
    }

    // The HUD hook.
    let drain = rules::drain_per_tick(&tuning, battle, form_drain(&abilities)).max(f32::EPSILON);
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
/// plays no motion). Restores the base body with its one keyblade, the attack table and the
/// player's own ability list (the growth levels follow it).
#[allow(clippy::too_many_arguments)]
fn do_revert(
    commands: &mut Commands,
    root: Entity,
    s: &mut Sora,
    fighter: &mut Fighter,
    st: &mut DriveState,
    base: &BaseSora,
    rigs: &Rigs,
    abilities: &mut Option<ResMut<Abilities>>,
    changed: &mut MessageWriter<FormChanged>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    why: &str,
) {
    let Some(b) = &base.0 else { return };
    let from = st.form;
    let assets = FormAssets { body: rigs.sora.clone(), weapons: vec![(0, rigs.keyblade.clone())], moves: b.moves.clone() };
    swap_bodies(commands, root, s, fighter, &assets, meshes, materials, images, bindposes);
    if let Some(ab) = abilities.as_deref_mut() {
        ab.set_form(None);
    }
    st.in_form = false;
    st.form = 0;
    st.timer = Default::default();
    s.state = if s.motor.grounded { State::Ground } else { State::Air };
    if !s.motor.grounded {
        s.motor.start_fall();
    }
    info!(
        "drive: revert from {} ({why}); gauge mode normal, points {} + {}%, stat block {}",
        form_name(from),
        st.points.whole,
        st.points.fraction,
        abilities.as_deref().map_or("ability system off".to_string(), |a| {
            let d = a.block().deltas();
            if d.is_empty() { "unchanged".to_string() } else { d.join("; ") }
        })
    );
    changed.write(FormChanged { from, to: 0 });
}

/// Logs form changes (other systems read `FormChanged` for their own effects, sounds and camera).
fn log_changes(mut changes: MessageReader<FormChanged>) {
    for c in changes.read() {
        info!("drive: form changed {} -> {}", form_name(c.from), form_name(c.to));
    }
}

#[cfg(test)]
mod tests {
    use super::rules::*;
    use super::*;

    fn archive() -> Option<kh2_archive::Archive> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        kh2_archive::Archive::open(&iso).map_err(|e| eprintln!("skipped: {iso}: {e}")).ok()
    }

    fn rules_from_disc() -> Option<DriveRules> {
        DriveRules::from_archive(&mut archive()?)
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
    fn anti_scales_generic_damage_only() {
        let t = Tuning { point_units: 600.0, field_drain_scale: 0.5, form_hit_loss: 1.2, anti_level_basis: 5, anti_drop: 5, anti_damage: 1.5, final_base: 25.0, final_growth: 2.0, final_cap: 100.0, final_anti_drop: 10, form_boost: 0.8 };
        assert_eq!(outgoing_damage(&t, form::ANTI, 0, 10), 15);
        assert_eq!(outgoing_damage(&t, form::ANTI, 5, 10), 10);
        assert_eq!(outgoing_damage(&t, form::VALOR, 0, 10), 10);
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
        // Form Boost shortens the drain once per granted copy (the block's +0x3c): Final grants it
        // at levels 4 and 6, and the player's own support copy counts too.
        let ab_rules = Arc::new(crate::abilities::AbilityRules::load(&mut archive().unwrap()).unwrap());
        let drain_at = |level: u8, own: &[u16]| {
            let mut a = crate::abilities::Abilities::new(ab_rules.clone());
            for id in own {
                a.equip_ability(*id);
            }
            a.set_form(Some(granted_abilities(&r.levels, form::FINAL, level)));
            a.block().form_drain
        };
        assert_eq!(drain_at(3, &[]), 1.0);
        assert_eq!(drain_at(4, &[]), t.form_boost);
        assert!(drain_at(6, &[]) < t.form_boost);
        assert!(drain_at(3, &[crate::abilities::id::FORM_BOOST]) == t.form_boost);
        assert!(drain_per_tick(&t, true, drain_at(4, &[])) < drain_per_tick(&t, true, 1.0));
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
    fn explore_memt_went_wmst() {
        let Some(r) = rules_from_disc() else { return };
        for world in [2u16, 13, 14, 17] {
            let rec = memt_merge(&r.memt, world, |_| false);
            eprintln!("world {world}: words 8..24 {:?}", &rec[8..]);
            for f in 0..=5u8 {
                let obj = if f == 0 { rec[8] } else { rec[11 + f as usize] };
                let Some(o) = r.objs.iter().find(|o| o.object_id == obj as u32) else { continue };
                let names = r.wmst.get(o.neo_moveset as usize);
                let w0 = weapon_object(&r.went, o.neo_moveset as usize, r.key_slot);
                let w0n = w0.and_then(|id| r.objs.iter().find(|w| w.object_id == id)).map(|w| (w.model_name.clone(), w.animation_name.clone(), w.weapon_joint));
                eprintln!(
                    "  form {f}: obj {obj} {} mset {} moveset {} joint {} wmst {:?} went(key) {:?} -> {:?}",
                    o.model_name, o.animation_name, o.neo_moveset, o.weapon_joint, names, w0, w0n
                );
            }
        }
        // Every went row of the forms' movesets: which slots are filled.
        let w = |i: usize| r.went.get(i * 4..i * 4 + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0);
        eprintln!("went words 0..16: {:?}", (0..16).map(w).collect::<Vec<_>>());
        eprintln!("wmst rows: {}", r.wmst.len());
        for (i, n) in r.wmst.iter().enumerate().filter(|(_, n)| !n[0].is_empty() || !n[1].is_empty()) {
            eprintln!("  wmst {i}: {n:?}");
        }
    }

    #[test]
    fn form_files_follow_memt_objentry_went_and_wmst() {
        let Some(r) = rules_from_disc() else { return };
        let key = KINGDOM_KEY;
        let valor = r.files(form::VALOR, 2, key).unwrap();
        assert_eq!((valor.model.as_str(), valor.mset.as_str()), ("P_EX100_BTLF", "P_EX100_BTLF"));
        // Hand 0 is the equipped keyblade through the moveset's went row; hand 1 the form record's
        // item, with the wmst motion set of each hand.
        let hands = |f: &FormFiles| f.hands.clone().map(|w| w.map(|w| (w.model, w.mset)));
        assert_eq!(hands(&valor), [Some(("W_EX010".into(), Some("W_EX010_BTLF_R".into()))), Some(("W_EX010".into(), Some("W_EX010_BTLF_L".into())))]);
        // Without a form keyblade the left hand is empty.
        assert_eq!(hands(&r.files(form::VALOR, 2, 0).unwrap())[1], None);
        // Wisdom: one keyblade; Anti: none (its moveset's went row and wmst row are empty).
        assert_eq!(hands(&r.files(form::WISDOM, 2, key).unwrap())[1], None);
        assert!(hands(&r.files(form::WISDOM, 2, key).unwrap())[0].is_some());
        assert_eq!(hands(&r.files(form::ANTI, 2, key).unwrap()), [None, None]);
        // World outfits: the memt records of worlds 17, 13 and 14 swap in the _TR, _WI and _NM objects.
        assert_eq!(r.files(form::VALOR, 17, key).unwrap().model, "P_EX100_TR_BTLF");
        assert_eq!(r.files(form::FINAL, 13, key).unwrap().model, "P_EX100_WI_ULTF");
        assert_eq!(r.files(form::MASTER, 14, key).unwrap().model, "P_EX100_NM_TRIF");
        // The form's shared motion set and its own attack table.
        assert_eq!(r.files(form::VALOR, 17, key).unwrap().mset, "P_EX100_BTLF");
        assert_ne!(r.files(form::VALOR, 17, key).unwrap().moveset, valor.moveset);
        for f in 1..=5 {
            assert!(r.files(f, 2, key).is_some(), "form {f}");
        }
    }

    #[test]
    fn base_body_follows_the_memt_merge() {
        let Some(r) = rules_from_disc() else { return };
        let world = |id: u16| DriveWorld { id, outfit: 0 };
        let base = r.base_files(&world(2)).unwrap();
        assert_eq!((base.model.as_str(), base.mset.as_str()), ("P_EX100", "P_EX100"));
        // Hand 0 only: the keyblade with the base motion set; nothing in hand 1.
        assert_eq!(base.hands[0].as_ref().map(|w| (w.model.as_str(), w.mset.as_deref())), Some(("W_EX010", Some("W_EX010"))));
        assert!(base.hands[1].is_none());
        // The outfit worlds replace the base body too (word 8 of their memt records), sharing P_EX100's motion set.
        for (id, model) in [(17u16, "P_EX100_TR"), (13, "P_EX100_WI"), (14, "P_EX100_NM")] {
            let f = r.base_files(&world(id)).unwrap();
            assert_eq!(f.model, model, "world {id}");
            assert_eq!(f.mset, "P_EX100", "world {id}");
            assert_ne!(f.moveset, base.moveset, "world {id}: its own attack table row");
            assert!(f.hands[0].is_some(), "world {id}");
        }
        // Outfits 1..3 of Twilight Town are the story-flag records, not merged here: empty words.
        assert!(r.base_files(&DriveWorld { id: 2, outfit: 1 }).is_none());
    }

    #[test]
    fn form_keyblade_comes_from_the_inventory() {
        let Some(r) = rules_from_disc() else { return };
        let items = r.items();
        // The first keyblade in table order with a copy in the inventory, taken once per form that
        // dual-wields (Valor, Master, Final); Wisdom and Anti take none.
        let mut inv = vec![KINGDOM_KEY, KINGDOM_KEY];
        assert_eq!(take_form_keyblade(items, &mut inv, form::VALOR), KINGDOM_KEY);
        assert_eq!(take_form_keyblade(items, &mut inv, form::WISDOM), 0);
        assert_eq!(inv.len(), 1);
        assert_eq!(take_form_keyblade(items, &mut inv, form::MASTER), KINGDOM_KEY);
        assert_eq!(take_form_keyblade(items, &mut inv, form::FINAL), 0);
        assert!(inv.is_empty());
        // Table order, not inventory order: the lowest keyblade id in the table wins.
        let mut keys: Vec<u16> = items.iter().filter(|i| i.r#type == KEYBLADE_TYPE).map(|i| i.id).take(2).collect();
        assert_eq!(keys.len(), 2);
        keys.reverse();
        let first = keys[1];
        assert_eq!(take_form_keyblade(items, &mut keys, form::FINAL), first);
        // An item that is not a keyblade is never taken.
        let mut other = vec![1u16];
        assert_eq!(take_form_keyblade(items, &mut other, form::VALOR), 0);
    }
}
