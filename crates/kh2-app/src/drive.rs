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
use crate::progress::Progress;
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
        /// The charge's re-arm time (`sstm[0xd4]`, `FUN_001ce240`).
        pub charge_wait: f32,
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
                charge_wait: s.f32(0xd4),
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

    /// The drive-gauge charge of Mickey's player class (reaction commands 455 and 457, `FUN_001ce518`
    /// sets it, `FUN_001ce240` runs it each tick; both are in the class table at 0x360360 that
    /// `FUN_001cda78` / `FUN_001cdb98` build, not in Sora's at 0x360410, whose command handler
    /// `FUN_001cf030` sends 455 and 457 to the generic path `FUN_00177b18`): `total` is the timer at +0xb68, `wait` the second timer at
    /// +0xb6c. While `total` is above 0, each tick where the player's motion has reached its end
    /// (`FUN_0017f400`: no blend pending and the motion's frame is at or past its length) takes the
    /// frame unit off `wait`; at 0 or below one gauge unit is added (`FUN_001a0290`) and `wait` is
    /// re-armed by `sstm[0xd4]`. `total` always drops by the frame unit; reaching 0 clears it and
    /// asks for motion 5 (`finished`).
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Charge {
        pub total: f32,
        pub wait: f32,
    }

    /// What one tick of the charge asks for.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct ChargeStep {
        /// Gauge units to add through `Points::gain`.
        pub units: i32,
        /// The timer ran out: play motion 5.
        pub finished: bool,
        /// The charge motion was replaced by another motion while the timer ran: its end callback
        /// (`LAB_001ce710`) cleared the timer, with no motion 5.
        pub cancelled: bool,
    }

    /// Motion 0xc1 of command 455 and the blend `FUN_001ce518` starts it with (`DAT_0037c33c`, 8.0).
    pub const CHARGE_MOTION: u16 = 0xc1;
    pub const CHARGE_BLEND: f32 = 8.0;
    /// The motion asked for when the timer runs out (`FUN_001ce240`: motion 5 with no blend, then
    /// motion 0 queued behind it, `FUN_0017e378`).
    pub const CHARGE_END_MOTION: u16 = 5;

    /// The charge as the player object runs it: the timers and the end callback `FUN_001ce518` hangs
    /// on motion 0xc1 (controller +0x194, player +0x2d4 = `LAB_001ce710`, two instructions that store
    /// 0 to +0xb68). The motion controller calls that callback and clears it whenever a new motion
    /// starts (`FUN_0017dd38` -> `FUN_0017fac8`), so anything that replaces the charge motion ends the
    /// charge with no gain and no motion 5. Command 457 starts no motion and hangs no callback.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct ChargeRun {
        pub charge: Charge,
        /// The end callback is armed on the charge motion.
        pub hooked: bool,
    }

    impl ChargeRun {
        /// `FUN_001ce518` for 455 or 457: returns the motion to start (0xc1 for 455).
        pub fn accept(&mut self, ticks: f32, motion: Option<u16>) -> Option<u16> {
            self.charge.start(ticks, motion.is_some());
            if motion.is_some() {
                self.hooked = true;
            }
            motion
        }

        /// One tick of `FUN_001ce240`. `replaced`: a motion other than the charge motion started
        /// since the last tick (the callback ran); `motion_at_end`: `FUN_0017f400` on the player's
        /// controller.
        pub fn tick(&mut self, t: &Tuning, replaced: bool, motion_at_end: bool) -> ChargeStep {
            if self.hooked && replaced {
                self.hooked = false;
                let was = self.charge.total > 0.0;
                self.charge.total = 0.0;
                return ChargeStep { cancelled: was, ..Default::default() };
            }
            let step = self.charge.tick(t, motion_at_end);
            if step.finished {
                // Motion 5 replaces the charge motion: the callback runs and is cleared.
                self.hooked = false;
            }
            step
        }

        pub fn running(&self) -> bool {
            self.charge.total > 0.0
        }
    }

    impl Charge {
        /// Command 455 (`ticks` 120, wait reset to 0) or 457 (`ticks` 30, `wait` kept).
        pub fn start(&mut self, ticks: f32, reset_wait: bool) {
            self.total = ticks;
            if reset_wait {
                self.wait = 0.0;
            }
        }

        pub fn tick(&mut self, t: &Tuning, motion_at_end: bool) -> ChargeStep {
            let mut step = ChargeStep::default();
            if self.total <= 0.0 {
                return step;
            }
            if motion_at_end {
                self.wait -= FRAME_UNIT;
                if self.wait <= 0.0 {
                    step.units = 1;
                    self.wait += t.charge_wait;
                }
            }
            self.total -= FRAME_UNIT;
            if self.total <= 0.0 {
                self.total = 0.0;
                step.finished = true;
            }
            step
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
    /// paths read writes it before play: `FUN_00187b38` sets it to 0); the app takes it from
    /// `progress::Progress::outfit` or `KH2_DRIVE_OUTFIT`.
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

    // ---------------------------------------------------------------------------------------------
    // Summons (docs/summons.md sections 1 and 2). The summon uses the same gauge fields as the form
    // timer (status +0x1b3 segment count, +0x1b4 timer, +0x1b8 its maximum) under mode byte 3.
    // ---------------------------------------------------------------------------------------------

    /// The gauge mode byte (status +0x1af): 1 normal drive points, 2 form timer, 3 summon timer
    /// (4, locked, is not used by the app).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum GaugeMode {
        #[default]
        Normal = 1,
        Form = 2,
        Summon = 3,
    }

    /// The `sstm` fields only the summon code reads (the segment size is `Tuning::point_units`,
    /// the field drain scale `Tuning::field_drain_scale`).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct SummonTuning {
        /// `sstm[0x70]`: multiplier on every gauge change made through FUN_001788d8 in modes 2 and 3.
        pub gain: f32,
        /// `sstm[0x84]` / `sstm[0x88]`: the arrival wait in frames with / without the arrival event
        /// (FUN_001c6260 loads both with `lwc1` at 0x1c62c4 and 0x1c6548).
        pub wait_event: f32,
        pub wait: f32,
    }

    impl SummonTuning {
        pub fn from_sstm(s: &Sstm) -> SummonTuning {
            SummonTuning { gain: s.f32(0x70), wait_event: s.f32(0x84), wait: s.f32(0x88) }
        }
    }

    /// `FUN_001a0480(status, 3, level)`: the summon timer and its segment count. Segments = level + 3,
    /// timer = maximum = `sstm[0x68]` x (level + 3): `FormTimer::start` with form 0 (base Sora's
    /// level rule, no Anti special case).
    pub fn summon_timer(t: &Tuning, level: u8) -> (FormTimer, u8) {
        (FormTimer::start(t, 0, level), level.wrapping_add(3))
    }

    /// The drain multiplier of a gauge mode (`FUN_00177788`): the ability block's `+0x3c` (Form
    /// Boost) in mode 2, `+0x68` (Summon Boost) in mode 3; mode 1 does not drain (`None`).
    pub fn mode_drain_factor(mode: GaugeMode, form_drain: f32, summon_drain: f32) -> Option<f32> {
        match mode {
            GaugeMode::Normal => None,
            GaugeMode::Form => Some(form_drain),
            GaugeMode::Summon => Some(summon_drain),
        }
    }

    /// The whole segments the timer lost between two values (`FUN_001a0358` in mode 3: FUN_001a0420
    /// before minus after). Negative for a gain; the experience add ignores those.
    pub fn segments_drained(t: &Tuning, before: &FormTimer, after: &FormTimer) -> i32 {
        before.whole_points(t) - after.whole_points(t)
    }

    impl FormTimer {
        /// `FUN_001a0358` with the player's form field `form` and the gauge mode: the change (Anti
        /// turns a gain into a loss), clamped to 0..=total; in mode 3 the whole segments lost, which
        /// the caller passes to the summon experience (`FUN_001d4ec8`). 0 in the other modes.
        pub fn change(&mut self, t: &Tuning, mode: GaugeMode, form: u8, delta: f32) -> i32 {
            let before = *self;
            self.add(form, delta);
            if mode == GaugeMode::Summon { segments_drained(t, &before, self) } else { 0 }
        }
    }

    /// The summon level cap (`FUN_001d4e68`): 2 + the number of `sumn` rows whose charm count is above
    /// 0 (`FUN_001a1cc8`), and 7 when that sum is 6 (all four charms). The same shape as the form cap.
    pub fn summon_level_cap(charms_owned: u32) -> u8 {
        match 2 + charms_owned {
            6 => 7,
            n => n.min(255) as u8,
        }
    }

    /// The summon level (`DAT_00340c88` byte 6) and experience (`DAT_00340e4c`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct SummonExp {
        pub level: u8,
        pub exp: i32,
    }

    impl SummonExp {
        /// `FUN_001d4ec8(gained)`: nothing for a gain of 0 or less or while the minigame flag of
        /// `FUN_001b9bd8` is set; else the experience grows and, while the level is below `cap`
        /// (`FUN_001d3cf8`) and the `fmlv` row of form 0 at the current level exists
        /// (`FUN_001d3b90(0, level)`, a binary search on the key `form << 4 | level`) and its `Exp` is
        /// reached, the level rises and that `Exp` is taken off (`FUN_001d4fa0` then shows the notice).
        /// When no row is returned (cap reached or no row) the experience is set to 0. Returns the
        /// levels reached, in order.
        pub fn add(&mut self, levels: &FormLevels, cap: u8, gained: i32, minigame: bool) -> Vec<u8> {
            let mut reached = Vec::new();
            if gained <= 0 || minigame {
                return reached;
            }
            self.exp = self.exp.wrapping_add(gained);
            loop {
                let row = if self.level < cap { levels.get(player::form::SUMMON_CURVE, self.level) } else { None };
                let Some(row) = row else {
                    self.exp = 0;
                    break;
                };
                if self.exp < row.exp {
                    break;
                }
                self.level = self.level.wrapping_add(1);
                self.exp -= row.exp;
                reached.push(self.level);
            }
            reached
        }
    }

    /// Damage to a summon drains the player's timer instead of the summon's HP (the summon class's HP
    /// change method, vtable 0x35fe18 slot +0x78 = `FUN_001c60f8`): a negative change `d` changes the
    /// timer by `sstm[0x68] x d / max_hp` (the summon's status +4); a positive one does nothing.
    /// Returns the timer change (`None` for no change).
    // Its caller (the summon object, the summon limit task) comes in a later stage of docs/summons.md.
    #[allow(dead_code)]
    pub fn summon_damage_to_timer(t: &Tuning, d: i32, summon_max_hp: i32) -> Option<f32> {
        (d < 0).then(|| t.point_units * d as f32 / summon_max_hp as f32)
    }

    /// A gauge change through `FUN_001788d8` in mode 3 (hits Sora lands, summon limits): the amount
    /// times `sstm[0x70]`, added to the timer by `FUN_001a0358`.
    pub fn summon_gauge_change(st: &SummonTuning, amount: f32) -> f32 {
        amount * st.gain
    }

    /// A summon limit's cost (`FUN_001b28a0`: `FUN_001788d8(-Cost)`): the timer loses `Cost x sstm[0x70]`.
    // Its caller (the summon object, the summon limit task) comes in a later stage of docs/summons.md.
    #[allow(dead_code)]
    pub fn summon_limit_cost(st: &SummonTuning, cost: u8) -> f32 {
        summon_gauge_change(st, -(cost as f32))
    }

    /// The digit and fill the HUD shows for modes 2 and 3 (`FUN_00155bf0`): segment size =
    /// int(max / count); digit = int(timer) / size; when that equals the count the digit is count - 1
    /// with a full fill, else the fill is int(timer) mod size. Returns (digit, fill, size); all 0 when
    /// the size is 0.
    pub fn segment_display(left: f32, total: f32, count: u8) -> (u32, i32, i32) {
        let seg = if count == 0 { 0 } else { (total / count as f32) as i32 };
        if seg == 0 {
            return (0, 0, 0);
        }
        let digit = (left as i32 / seg) as u32;
        if digit == count as u32 { (digit - 1, seg, seg) } else { (digit, left as i32 % seg, seg) }
    }

    /// What decides whether a summon command (`Execute` 72) is available (`FUN_001ba2c0` case 0x48).
    #[derive(Clone, Copy, Debug)]
    pub struct SummonCtx {
        /// `0x35feb8` bit 0 (a summon or limit sequence) or `0x35fec0` bit 3 (the summon arrival).
        pub busy: bool,
        /// The player's form field `+0xb50` (6 and 11 refuse).
        pub form: u8,
        /// `FUN_001c78f0`: both party slots hold valid members in the party.
        pub party_ok: bool,
        /// `DAT_0035d268` bit 0x40000.
        pub locked: bool,
        /// Whole drive points (status +0x1b1) and the command's `Cost`.
        pub points: u8,
        pub cost: u8,
    }

    /// `FUN_001ba568` + `FUN_001c7970` for a summon command (not the menu entry, which passes at once
    /// in `FUN_001b7098`). The current battle record's bit 14 (`FUN_001ba0c0`) is not modelled (no
    /// battle records in the app).
    pub fn summon_available(c: &SummonCtx) -> bool {
        !c.busy && c.party_ok && c.form != 6 && c.form != 11 && !c.locked && c.cost <= c.points
    }

    /// Dismiss (command 316) is available only while the summon state is 2 and no limit task exists
    /// (`FUN_001b3c00`: `DAT_01d5d6b0 != 0`).
    pub fn dismiss_available(summon_state: u8, limit_task: bool) -> bool {
        summon_state == 2 && !limit_task
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
            // A hand whose `wmst` name is empty loads no motion set (`FUN_00184640` returns 0 and
            // `FUN_00184190` skips the animation setup): such a weapon has nothing to animate and is
            // not drawn [inferred]. Anti's row is the one such case (his went row is filled).
            let mset = mset_of(hand)?;
            Some(WeaponFiles { model: w.model_name.clone(), mset: Some(mset) })
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
    /// The gauge mode byte (status +0x1af). Mode 2 follows `in_form`; mode 3 (the summon timer in
    /// `timer`) is set and cleared by `summon.rs`.
    pub gauge_mode: rules::GaugeMode,
    /// The gauge's segment count (status +0x1b3), written by the mode set `FUN_001a0480`.
    pub segments: u8,
    /// A summon sequence holds the drive commands (`0x35feb8` bit 0 or `0x35fec0` bit 3, which
    /// `FUN_001ba568` tests for Drive and Summon alike); `summon.rs` keeps it.
    pub summon_busy: bool,
    ticks: u32,
}

/// The form record's ability list at `level` (what `FUN_001c0e50` applies from its 16 slots): the `fmlv`
/// grants up to the level plus the growth ability of the form's chain at the display level (the slot
/// `FUN_001d3da8` -> `FUN_001d3f18` keeps at the current level; 0x37c556).
pub fn form_ability_list(levels: &FormLevels, ab: &crate::abilities::AbilityRules, form: u8, level: u8) -> Vec<u16> {
    let g = rules::growth_level(levels, form, level);
    let mut list = rules::granted_abilities(levels, form, level);
    if let Some(id) = (g > 0).then(|| ab.growth_ability(form as usize - 1, g)).flatten() {
        list.push(id);
    }
    list
}

/// A form level-up while the player is in that form (`FUN_001d3da8`: the record's list is upgraded in
/// place, then `FUN_0019fb98` rebuilds the status block from it): the form list the ability system applies
/// is rebuilt at the new level, which recomputes the block. Returns the new list, or `None` when the
/// player is not in `form` (the list then applies from the next entry, as before).
pub fn refresh_form_list(state: &DriveState, levels: &FormLevels, ab: &mut Abilities, form: u8) -> Option<Vec<u16>> {
    if state.object_form() != form || form == 0 {
        return None;
    }
    let level = *state.level.get(form as usize)?;
    let list = form_ability_list(levels, ab.rules(), form, level);
    ab.set_form(Some(list.clone()));
    Some(list)
}

impl DriveState {
    /// The form field of the current player object (`+0xb50`): the entered form once the body has
    /// been swapped, else 0 (the old object stays the base one through the change).
    pub fn object_form(&self) -> u8 {
        if self.in_form { self.form } else { 0 }
    }

    /// The owned forms, their levels, their second keyblades and the spare keyblades come from
    /// `progress::Progress` (the form items, the form records, the inventory); `KH2_DRIVE_OWNED`,
    /// `KH2_DRIVE_LEVEL` and `KH2_DRIVE_INVENTORY` replace them. Without `Progress` the old
    /// defaults apply (forms 1..4 owned at level 1, one spare Kingdom Key per form that takes one).
    fn from_env(rules: &DriveRules, progress: Option<&Progress>) -> DriveState {
        let env = |k: &str| std::env::var(k).ok();
        let num = |k: &str| env(k).and_then(|v| v.trim().parse::<i32>().ok());
        let scripted = env("KH2_DRIVE").is_some();
        // New game: 0 points of 3 (FUN_001cda78). The harness starts with 9 so that any form can be
        // entered (the maximum is raised by bonuses in the game, `bons` DriveGaugeUpgrade).
        let want = num("KH2_DRIVE_POINTS").map_or(if scripted { 9 } else { 0 }, |v| v.clamp(0, 9) as u8);
        let points = rules::Points { whole: want, fraction: 0, max: want.max(3) };
        let mut owned = [false; 6];
        let env_owned = env("KH2_DRIVE_OWNED");
        match (&env_owned, progress) {
            (Some(v), _) => v.split(',').filter_map(|x| x.trim().parse::<usize>().ok()).filter(|&f| f < 6).for_each(|f| owned[f] = true),
            (None, Some(p)) => (1..=5u8).for_each(|f| owned[f as usize] = p.owns_form(f)),
            (None, None) => owned[1..=4].fill(true),
        }
        let cap = player::form_level_cap(owned[1..=4].iter().filter(|&&o| o).count() as u32);
        let saved = |f: u8| progress.map_or(1, |p| p.form_level(f).clamp(1, 7));
        let level: [u8; 6] = match num("KH2_DRIVE_LEVEL") {
            Some(v) => {
                let l = v.clamp(1, cap as i32) as u8;
                [0, l, l, l, l, l]
            }
            None => [0, saved(1), saved(2), saved(3), saved(4), saved(5)],
        };
        let seed = env("KH2_DRIVE_SEED").and_then(|v| v.parse::<u32>().ok()).unwrap_or_else(|| {
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos())
        });
        let party = env("KH2_DRIVE_PARTY").unwrap_or_else(|| "both".into());
        // The forms' second keyblades come from the inventory when the form items are obtained
        // (`FUN_001a2010`). With `Progress` (and neither env var) that already happened there:
        // the form records hold them and the inventory keeps the rest. Otherwise the harness
        // obtains the owned forms in id order from `KH2_DRIVE_INVENTORY` (`=` empty: no spare
        // keyblade, `=id,id` any list) or, by default, one spare copy of the equipped keyblade per
        // form that takes one, which keeps the Kingdom Key in both hands.
        let env_inventory = env("KH2_DRIVE_INVENTORY");
        let mut form_weapon = [0u16; 6];
        let inventory: Vec<u16> = match (progress, env_owned.is_none() && env_inventory.is_none()) {
            (Some(p), true) => {
                for f in 1..=4u8 {
                    if owned[f as usize] {
                        form_weapon[f as usize] = p.form(f).weapon;
                    }
                }
                p.keyblades()
            }
            _ => {
                let mut inventory: Vec<u16> = match env_inventory {
                    Some(v) => v.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
                    None => (1..=4u8).filter(|&f| owned[f as usize] && matches!(f, 1 | 3 | 4)).map(|_| rules.key_item).collect(),
                };
                for f in 1..=4u8 {
                    if owned[f as usize] {
                        form_weapon[f as usize] = rules::take_form_keyblade(rules.items(), &mut inventory, f);
                    }
                }
                inventory
            }
        };
        DriveState {
            points,
            in_form: false,
            form: 0,
            level,
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
            gauge_mode: rules::GaugeMode::Normal,
            segments: 0,
            summon_busy: false,
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
        crate::progress::ensure(app);
        let progress = app.world().get_resource::<Progress>();
        // The outfit is the world record's byte (`Progress::outfit`); `KH2_DRIVE_OUTFIT` wins.
        let outfit = std::env::var("KH2_DRIVE_OUTFIT")
            .ok()
            .and_then(|v| v.parse::<u8>().ok())
            .unwrap_or_else(|| progress.map_or(0, |p| p.outfit(world as usize)))
            .min(3);
        let enter = std::env::var("KH2_DRIVE").ok().and_then(|v| v.parse::<u8>().ok()).map(|f| (env_f("KH2_DRIVE_AT").unwrap_or(4.0), f));
        let state = DriveState::from_env(&rules, progress);
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
            .insert_resource(ChargeHook { run: Default::default(), slot: None, on_sora: std::env::var("KH2_CHARGE_ON_SORA").is_ok_and(|v| v != "0") })
            .add_systems(
                FixedUpdate,
                (
                    charge_tick.in_set(Sim::Sora).before(sora::tick),
                    drive_tick.in_set(Sim::Weapon).before(sora::sync_weapon),
                    log_changes.in_set(Sim::Sounds),
                ),
            );
    }
}

/// The charge of commands 455 and 457 (`rules::ChargeRun`) on the player object. Only Mickey's
/// player class runs it (see `rules::Charge`); the app's player is Sora, whose class does not, so
/// by default the command is logged and dropped. `KH2_CHARGE_ON_SORA=1` (harness only, NOT the
/// original's behaviour for Sora) runs Mickey's handler on Sora's body to exercise the wiring:
/// Sora's motion 0xc1 is his own A342, not Mickey's A122.
#[derive(Resource)]
struct ChargeHook {
    run: rules::ChargeRun,
    /// The body slot the charge motion plays in (the callback is on that motion).
    slot: Option<usize>,
    on_sora: bool,
}

#[allow(clippy::too_many_arguments)]
fn charge_tick(
    mut cmds: MessageReader<crate::reaction_cmd::PlayerReactionCmd>,
    rules: Res<DriveRules>,
    battle: Res<Battle>,
    mut state: ResMut<DriveState>,
    mut hook: ResMut<ChargeHook>,
    mut sora_q: Query<&mut Sora>,
    mut anims: Query<&mut Animator>,
) {
    use crate::reaction_cmd::PlayerReactionCmd;
    let Ok(mut s) = sora_q.single_mut() else {
        cmds.clear();
        return;
    };
    let pstate = if battle.active() { kh2_anim::PlayerState::BattleWeapon } else { kh2_anim::PlayerState::FieldNoWeapon };
    // A motion other than the charge motion is on the body: the callback ran (FUN_0017fac8).
    let replaced = anims.get(s.body).map_or(true, |b| hook.slot.is_none() || b.slot != hook.slot);
    let at_end = anims.get(s.body).is_ok_and(|b| b.motion().is_some_and(|m| b.frame >= m.frame_end));
    let ChargeHook { run, slot, on_sora } = &mut *hook;
    if run.running() {
        let step = run.tick(&rules.tuning, replaced, at_end);
        if step.units != 0 {
            let before = state.points;
            state.points.gain(step.units);
            info!(
                "drive: charge +{} gauge unit (FUN_001a0290): {} points + {}% -> {} + {}% (timer {:.0}, wait {:.0})",
                step.units, before.whole, before.fraction, state.points.whole, state.points.fraction, run.charge.total, run.charge.wait
            );
        }
        if step.cancelled {
            *slot = None;
            info!("drive: charge cancelled: the charge motion was replaced ({}), its end callback cleared the timer", anims.get(s.body).map_or("-", |b| b.tag()));
        }
        if step.finished {
            *slot = None;
            let mut cue = WeaponCue::default();
            if let Ok(mut body) = anims.get_mut(s.body) {
                let end = body.rig.mset.slot(rules::CHARGE_END_MOTION as usize, pstate);
                body.restart(end, false, 0.0);
                cue.restart(end, false, 0.0);
                info!("drive: charge timer ran out: motion {} ({}) with motion 0 queued behind it", rules::CHARGE_END_MOTION, body.tag());
            }
            cue.apply(&s.weapons, &mut anims);
            // Motion 5 then motion 0: the ground state plays the landing motion out, then idle.
            if s.state == State::Reaction {
                let next = if s.motor.grounded { State::Ground } else { State::Air };
                s.enter(next, None);
            }
        }
    }
    for c in cmds.read() {
        let PlayerReactionCmd::Charge { ticks, motion } = *c else { continue };
        if !*on_sora {
            info!(
                "drive: charge command (timer {ticks}, motion {motion:?}) not run: Sora's command handler FUN_001cf030 has no charge (FUN_001ce518 is Mickey's class); KH2_CHARGE_ON_SORA=1 runs it on Sora as a harness"
            );
            continue;
        }
        let want = run.accept(ticks, motion);
        info!("drive: charge started (harness on Sora): timer {ticks}, wait {:.0}, motion {want:?}", run.charge.wait);
        let Some(id) = want else { continue };
        let mut cue = WeaponCue::default();
        if let Ok(mut body) = anims.get_mut(s.body) {
            let sl = body.rig.mset.slot(id as usize, pstate);
            body.restart(sl, false, rules::CHARGE_BLEND);
            cue.restart(sl, false, rules::CHARGE_BLEND);
            *slot = sl;
            let which = if id == rules::CHARGE_MOTION { "command 455's" } else { "unexpected" };
            info!("drive: charge motion {id:#x} ({which}; {}, slot {sl:?}), blend {}", body.tag(), rules::CHARGE_BLEND);
        }
        cue.apply(&s.weapons, &mut anims);
        if slot.is_some() {
            // No input while the motion plays (the app's controller would replace it with locomotion).
            s.enter(State::Reaction, None);
        } else {
            run.hooked = false;
        }
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
            // The world variants of the keyblade (`W_EX010_TR`, `_WI`, `_NM`) are named by the object table
            // but are not on this disc (checked: no `obj/W_EX010_{TR,WI,NM}.mdlx`). The original's behaviour
            // for the missing file is not known; the app keeps the plain keyblade model with the hand's
            // motion set [inferred] rather than dropping the weapon.
            None => match rules.files(form, 2, hand1_item).and_then(|d| d.hands[hand].clone()) {
                Some(plain) if plain.model != w.model => match weapon_rig(disc, &WeaponFiles { model: plain.model.clone(), mset: w.mset.clone() }) {
                    Some(r) => {
                        warn!("drive: weapon {} of hand {hand} is not on the disc; using {}", w.model, plain.model);
                        weapons.push((hand, r));
                    }
                    None => warn!("drive: weapon {} of hand {hand} not loadable; the hand stays empty", w.model),
                },
                _ => warn!("drive: weapon {} of hand {hand} not loadable; the hand stays empty", w.model),
            },
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
fn drive_tick(
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
    mut magic: Option<ResMut<crate::magic::MagicState>>,
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
        } else if st.gauge_mode == rules::GaugeMode::Summon {
            // Mode 3: `summon.rs` applies the hit to the summon timer (FUN_001788d8).
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
                // A summon sequence or mode 3 holds Drive (FUN_001ba568: `0x35feb8` / `0x35fec0`).
                let can_act = change.0.is_none() && !st.summon_busy && st.gauge_mode != rules::GaugeMode::Summon && s.hp > 0 && !matches!(s.state, State::Down | State::Blown | State::Viewer);
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
            let mut list = rules::granted_abilities(&rules.levels, to, level);
            let block = match abilities.as_deref_mut() {
                Some(ab) => {
                    list = form_ability_list(&rules.levels, ab.rules(), to, level);
                    ab.set_form(Some(list.clone()));
                    let d = ab.block().deltas();
                    if d.is_empty() { "unchanged".to_string() } else { d.join("; ") }
                }
                None => "ability system off".to_string(),
            };
            st.timer = rules::FormTimer::start(&tuning, to, level);
            st.segments = (rules::timer_level(&tuning, to, level) + 3) as u8;
            st.in_form = true;
            st.gauge_mode = rules::GaugeMode::Form;
            // FUN_001a04b0 at the timer start: HP raised by the maximum (FUN_001a0140 -> the
            // player's method +0x78 = FUN_001792f0 -> FUN_00176c98 -> FUN_00186238, clamped by
            // FUN_001a0100) and MP set to its maximum (FUN_001a0240: a running MP charge ends first).
            let (hp_before, mp_before) = (s.hp, s.mp);
            s.hp = s.stats.max_hp;
            crate::magic::refill_mp(magic.as_deref_mut().map(|m| &mut m.charge), &mut s.mp, data.sora_max_mp);
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
    st.gauge_mode = rules::GaugeMode::Normal;
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

    /// A Master Form level-up while in the form (`FUN_001d3da8` -> `FUN_0019fb98`) rebuilds the form list
    /// the ability system applies at the new level; out of the form, or in another form, nothing changes.
    #[test]
    fn a_form_level_up_in_the_form_rebuilds_its_list() {
        let Some(r) = rules_from_disc() else { return };
        let ab_rules = Arc::new(crate::abilities::AbilityRules::load(&mut archive().unwrap()).unwrap());
        let mut ab = crate::abilities::Abilities::new(ab_rules);
        let mut st = DriveState::from_env(&r, None);
        st.level[form::MASTER as usize] = 2;
        assert_eq!(refresh_form_list(&st, &r.levels, &mut ab, form::MASTER), None, "not in a form");
        st.in_form = true;
        st.form = form::VALOR;
        assert_eq!(refresh_form_list(&st, &r.levels, &mut ab, form::MASTER), None, "in another form");
        st.form = form::MASTER;
        let at2 = form_ability_list(&r.levels, ab.rules(), form::MASTER, 2);
        assert_eq!(refresh_form_list(&st, &r.levels, &mut ab, form::MASTER).as_deref(), Some(&at2[..]));
        assert_eq!(ab.form_list(), &at2[..]);
        st.level[form::MASTER as usize] = 7;
        let at7 = refresh_form_list(&st, &r.levels, &mut ab, form::MASTER).expect("list");
        eprintln!("Master Form list at 2: {at2:x?}, at 7: {at7:x?}");
        assert!(at7.len() > at2.len(), "level 7 has more grants than level 2");
        assert_eq!(ab.form_list(), &at7[..]);
    }

    fn ctx(points: u8) -> EnterCtx {
        EnterCtx { current_form: 0, owned: [false, true, true, true, true, false], points, donald: true, goofy: true, locked: false }
    }

    #[test]
    fn charge_counts_down_and_adds_a_unit_per_wait() {
        let t = Tuning { point_units: 600.0, field_drain_scale: 0.5, form_hit_loss: 1.2, anti_level_basis: 5, anti_drop: 5, anti_damage: 1.5, final_base: 25.0, final_growth: 2.0, final_cap: 100.0, final_anti_drop: 10, form_boost: 0.8, charge_wait: 10.0 };
        let mut c = Charge::default();
        c.start(120.0, true);
        // The motion is never at its end: the timer runs out with no gain.
        let mut units = 0;
        let mut ticks = 0;
        loop {
            ticks += 1;
            let s = c.tick(&t, false);
            units += s.units;
            if s.finished {
                break;
            }
        }
        assert_eq!((ticks, units), (120, 0));
        // At its end every tick: one unit on the first tick (wait 0), then one per 10 ticks.
        c.start(120.0, true);
        let mut units = 0;
        for _ in 0..119 {
            units += c.tick(&t, true).units;
        }
        assert_eq!(units, 12);
        assert!(c.tick(&t, true).finished && c.total == 0.0);
        // Command 457 keeps the wait.
        c.wait = 4.0;
        c.start(30.0, false);
        assert_eq!(c.wait, 4.0);
    }

    #[test]
    fn charge_run_wiring_455_457_and_the_end_callback() {
        let t = Tuning { point_units: 600.0, field_drain_scale: 0.5, form_hit_loss: 1.2, anti_level_basis: 5, anti_drop: 5, anti_damage: 1.5, final_base: 25.0, final_growth: 2.0, final_cap: 100.0, final_anti_drop: 10, form_boost: 0.8, charge_wait: 4.0 };
        let mut pts = Points { whole: 0, fraction: 98, max: 3 };
        let mut r = ChargeRun::default();
        // 455: motion 0xc1, callback armed. The motion plays 15 ticks, then holds at its end.
        assert_eq!(r.accept(120.0, Some(CHARGE_MOTION)), Some(CHARGE_MOTION));
        assert!(r.hooked && r.running());
        let mut units = 0;
        let mut fin = 0;
        for tick in 1..=120 {
            let s = r.tick(&t, false, tick > 15);
            pts.gain(s.units);
            units += s.units;
            if s.finished {
                fin = tick;
            }
        }
        // 105 ticks at the end: a unit at the first (wait 0 -> -1), then one every 4 ticks.
        assert_eq!((units, fin), (27, 120));
        assert_eq!((pts.whole, pts.fraction), (1, 25));
        assert!(!r.hooked && !r.running());
        // 457 while 455's motion still holds: the wait is kept, the hook stays.
        r.accept(120.0, Some(CHARGE_MOTION));
        for _ in 0..10 {
            r.tick(&t, false, true);
        }
        let wait = r.charge.wait;
        assert_eq!(r.accept(30.0, None), None);
        assert!(r.hooked && r.charge.total == 30.0 && r.charge.wait == wait);
        // Another motion replaces the charge motion: cleared, no gain, no motion 5.
        let s = r.tick(&t, true, true);
        assert_eq!(s, ChargeStep { units: 0, finished: false, cancelled: true });
        assert!(!r.running() && !r.hooked);
        assert_eq!(r.tick(&t, false, true), ChargeStep::default());
        // 457 alone hangs no callback: a replaced motion does not stop it.
        let mut r = ChargeRun::default();
        r.accept(30.0, None);
        assert!(!r.tick(&t, true, false).cancelled && r.running());
    }

    /// The object `FUN_001cdf48` spawns with the charge class (`FUN_001cdb98`) is word 17 of the
    /// merged `memt` record (`FUN_001ce080` reads +0x22): its model is the one whose motion set
    /// offers 457 (`P_EX200`, docs/limits-reactions.md), not Sora's.
    #[test]
    fn charge_class_object_is_memt_word_17() {
        let Some(r) = rules_from_disc() else { return };
        let rec = r.merged(2);
        let obj = rec[17];
        let name = r.objs.iter().find(|o| o.object_id == obj as u32).map(|o| o.model_name.clone()).unwrap_or_default();
        eprintln!("memt word 17 (world 2): object {obj} model {name}");
        assert_eq!(name, "P_EX200");
    }

    #[test]
    fn charge_wait_is_read_from_the_disc() {
        let Some(r) = rules_from_disc() else { return };
        eprintln!("charge_wait {}", r.tuning.charge_wait);
        assert!(r.tuning.charge_wait > 0.0);
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
        let t = Tuning { point_units: 100.0, field_drain_scale: 0.5, form_hit_loss: 1.0, anti_level_basis: 5, anti_drop: 5, anti_damage: 1.5, final_base: 25.0, final_growth: 2.0, final_cap: 100.0, final_anti_drop: 10, form_boost: 0.8, charge_wait: 0.0 };
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
        let t = Tuning { point_units: 600.0, field_drain_scale: 0.5, form_hit_loss: 1.2, anti_level_basis: 5, anti_drop: 5, anti_damage: 1.5, final_base: 25.0, final_growth: 2.0, final_cap: 100.0, final_anti_drop: 10, form_boost: 0.8, charge_wait: 0.0 };
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
        let t = Tuning { point_units: 600.0, field_drain_scale: 0.5, form_hit_loss: 1.2, anti_level_basis: 5, anti_drop: 5, anti_damage: 1.5, final_base: 25.0, final_growth: 2.0, final_cap: 100.0, final_anti_drop: 10, form_boost: 0.8, charge_wait: 0.0 };
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
                let w0 = weapon_object(&r.went, o.neo_moveset as usize, r.item_slot(r.key_item).unwrap_or(0));
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
        // Wisdom: one keyblade (its form record holds no second item, `take_form_keyblade` gives 0);
        // Anti: none (its went row names W_EX010 but its wmst row is empty, so no motion set loads).
        assert_eq!(hands(&r.files(form::WISDOM, 2, 0).unwrap())[1], None);
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
        // The outfit index picks the word: Twilight Town's merged record has objects in words 9 and 10
        // (the Roxas outfits) and nothing in word 11 here.
        let o1 = r.base_files(&DriveWorld { id: 2, outfit: 1 }).unwrap();
        assert_ne!(o1.model, base.model);
        assert!(r.base_files(&DriveWorld { id: 2, outfit: 4 }).is_none());
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

    // ---- Summons (docs/summons.md stage 1) ---------------------------------------------------

    fn tuning_from_disc() -> Option<(Tuning, SummonTuning, FormLevels, Vec<t::Sumn>)> {
        let mut a = archive()?;
        let sstm = player::load_sstm(&mut a).ok()?;
        let fmlv = kh2_tables::load::<t::Fmlv>(&mut a, &t::FMLV).ok()?.rows;
        let sumn = kh2_tables::load::<t::Sumn>(&mut a, &t::SUMN).ok()?.rows;
        Some((Tuning::from_sstm(&sstm), SummonTuning::from_sstm(&sstm), FormLevels::from_rows(&fmlv), sumn))
    }

    #[test]
    fn summon_rules_on_the_disc_rows() {
        let Some((t, st, levels, sumn)) = tuning_from_disc() else { return };
        assert_eq!(sumn.len(), 4);
        // FUN_001a0480: level + 3 segments of sstm[0x68] units each.
        for level in 0..=7u8 {
            let (timer, segs) = summon_timer(&t, level);
            assert_eq!(segs, level + 3);
            assert_eq!(timer.left, t.point_units * (level as f32 + 3.0));
            assert_eq!(timer.total, timer.left);
            // A full timer shows one whole segment fewer (FUN_001a0420); the HUD shows count - 1 full.
            assert_eq!(timer.whole_points(&t), level as i32 + 2);
            assert_eq!(segment_display(timer.left, timer.total, segs), (segs as u32 - 1, t.point_units as i32, t.point_units as i32));
        }
        // The mode-3 drain factor is the ability block's summon factor; mode 1 does not drain.
        assert_eq!(mode_drain_factor(GaugeMode::Summon, 0.8, 0.5), Some(0.5));
        assert_eq!(mode_drain_factor(GaugeMode::Form, 0.8, 0.5), Some(0.8));
        assert_eq!(mode_drain_factor(GaugeMode::Normal, 0.8, 0.5), None);
        // The cap: 2 + charms, 7 with all four.
        assert_eq!((0..=4).map(summon_level_cap).collect::<Vec<_>>(), vec![2, 3, 4, 5, 7]);
        // Segments drained: leaving a full timer costs nothing (full shows one fewer), a drain across
        // a boundary counts one, a gain counts negative, and nothing counts outside mode 3.
        let (full, _) = summon_timer(&t, 1);
        let mut x = full;
        assert_eq!(x.change(&t, GaugeMode::Summon, 0, -1.0), 0);
        let mut y = x;
        assert_eq!(y.change(&t, GaugeMode::Summon, 0, -t.point_units), 1);
        assert_eq!(segments_drained(&t, &x, &y), 1);
        assert_eq!(y.change(&t, GaugeMode::Summon, 0, t.point_units), -1);
        assert_eq!(y.change(&t, GaugeMode::Form, 0, -t.point_units), 0);
        // The experience over the form-0 rows (FUN_001d4ec8).
        let rows: Vec<_> = levels.levels(player::form::SUMMON_CURVE).collect();
        assert!(!rows.is_empty(), "fmlv has form-0 rows");
        let need = levels.get(0, 1).map(|r| r.exp).expect("form 0 level 1");
        let mut e = SummonExp { level: 1, exp: 0 };
        assert!(e.add(&levels, 7, need - 1, false).is_empty());
        assert_eq!(e, SummonExp { level: 1, exp: need - 1 });
        assert_eq!(e.add(&levels, 7, 2, false), vec![2]);
        assert_eq!(e.exp, 1);
        assert!(e.add(&levels, 7, 0, false).is_empty() && e.add(&levels, 7, -5, false).is_empty() && e.add(&levels, 7, 99, true).is_empty());
        assert_eq!(e.exp, 1, "no gain for 0, a negative drop or the minigame flag");
        let mut capped = SummonExp { level: 3, exp: 0 };
        assert!(capped.add(&levels, 3, 1_000_000, false).is_empty());
        assert_eq!(capped, SummonExp { level: 3, exp: 0 }, "at the cap the experience is set to 0");
        // Damage to the summon: a hit of its whole max HP costs one segment; heals do nothing.
        assert_eq!(summon_damage_to_timer(&t, -500, 500), Some(-t.point_units));
        assert_eq!(summon_damage_to_timer(&t, 10, 500), None);
        // FUN_001788d8 in mode 3 and the limit cost: times sstm[0x70].
        assert_eq!(summon_gauge_change(&st, 4.0), 4.0 * st.gain);
        assert_eq!(summon_limit_cost(&st, 100), -100.0 * st.gain);
        assert!(st.wait > 0.0 && st.wait_event > 0.0);
        eprintln!(
            "summon: segment {} units, field drain scale {}, change factor {}, wait {} / {} (event) frames, form-0 rows {:?}",
            t.point_units,
            t.field_drain_scale,
            st.gain,
            st.wait,
            st.wait_event,
            rows.iter().map(|r| (r.level, r.exp)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn summon_availability_and_dismiss() {
        let ok = SummonCtx { busy: false, form: 0, party_ok: true, locked: false, points: 3, cost: 3 };
        assert!(summon_available(&ok));
        assert!(!summon_available(&SummonCtx { busy: true, ..ok }));
        assert!(!summon_available(&SummonCtx { party_ok: false, ..ok }));
        assert!(!summon_available(&SummonCtx { form: 6, ..ok }));
        assert!(!summon_available(&SummonCtx { form: 11, ..ok }));
        assert!(!summon_available(&SummonCtx { locked: true, ..ok }));
        assert!(!summon_available(&SummonCtx { points: 2, ..ok }));
        assert!(dismiss_available(2, false));
        assert!(!dismiss_available(1, false) && !dismiss_available(3, false) && !dismiss_available(2, true));
    }

    /// The oracle (PLAYBOOK R2): the game's FUN_001a0480, FUN_001a0358 (with FUN_001a0420 and the
    /// experience add FUN_001d4ec8 -> FUN_001d3cf8 -> FUN_001d4e68 / FUN_001d3b90), FUN_00155bf0 and
    /// FUN_001c60f8 in kh2-ee on the user's executable, against the rules above. The `fmlv`, `sumn` and
    /// `item` tables are laid out in free memory behind their pointers (`DAT_01d6a990`, `DAT_01d6a9b8`,
    /// `0x01c95588`); the charm counts go into the inventory (0x340ce8, or the bits at 0x340e28). The
    /// level-up notice (FUN_001d4fa0) and the message call (FUN_001b5528) are replaced by `jr ra` stubs:
    /// they only show the notice. The player object behind status +0x24c is a zeroed block (form
    /// field +0xb50 = 0).
    #[test]
    fn summon_gauge_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const SSTM: u32 = 0x0180_0000;
        const STATUS: u32 = 0x0181_0000;
        const OBJECT: u32 = 0x0182_0000;
        const SUMMON: u32 = 0x0183_0000;
        const SUMMON_STATUS: u32 = 0x0184_0000;
        const PANEL: u32 = 0x0185_0000;
        const OUT: u32 = 0x0186_0000;
        const FMLV_AT: u32 = 0x0187_0000;
        const SUMN_AT: u32 = 0x0188_0000;
        const ITEM_AT: u32 = 0x0189_0000;
        let Some(mut a) = archive() else { return };
        let Some((t, st, levels, sumn)) = tuning_from_disc() else { return };
        let sstm = player::load_sstm(&mut a).expect("sstm");
        let fmlv_blob = kh2_tables::subfile(&mut a, &t::FMLV).expect("fmlv");
        let sumn_blob = kh2_tables::subfile(&mut a, &t::SUMN).expect("sumn");
        let item_blob = kh2_tables::subfile(&mut a, &t::ITEM).expect("item");
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable");
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        for (i, w) in sstm.words.iter().enumerate() {
            ee.w32(SSTM + 4 * i as u32, *w);
        }
        ee.w32(0x0036_0130, SSTM);
        ee.write(FMLV_AT, &fmlv_blob);
        ee.w32(0x01d6_a990, FMLV_AT);
        ee.write(SUMN_AT, &sumn_blob);
        ee.w32(0x01d6_a9b8, SUMN_AT);
        ee.write(ITEM_AT, &item_blob);
        ee.w32(0x01c9_5588, ITEM_AT);
        // No battle record: the minigame flag of FUN_001b9bd8 reads clear.
        ee.w32(0x01d5_dd20, 0);
        for stub in [0x001d_4fa0u32, 0x001b_5528] {
            ee.w32(stub, 0x03e0_0008);
            ee.w32(stub + 4, 0);
        }
        // The player object behind DAT_0035c998 (FUN_001c60f8 changes its status).
        ee.write(OBJECT, &[0u8; 0xc00]);
        ee.w32(OBJECT + 0x48c, STATUS);
        ee.w32(0x0035_c998, OBJECT);
        // A charm's inventory count (FUN_001a1cc8: the byte at 0x340ce8 + record +0x12, or the bit at
        // 0x340e28 for a record with flag bit 0 at +3).
        let set_charm = |ee: &mut Ee, item: u16, on: bool| {
            let rec = ee.call(0x001a_66e8, &[item as u64], &[], STACK).expect("FUN_001a66e8") as u32;
            assert_ne!(rec, 0, "charm {item} has an item record");
            let slot = ee.r16(rec + 0x12) as u32;
            if ee.r8(rec + 3) & 1 == 0 {
                ee.write(0x0034_0ce8 + slot, &[on as u8]);
            } else {
                let w = 0x0034_0e28 + (slot >> 5) * 4;
                let v = (ee.r32(w) & !(1 << (slot & 31))) | (on as u32) << (slot & 31);
                ee.w32(w, v);
            }
        };
        let mut checked = 0;
        for owned in 0..=4usize {
            for (i, r) in sumn.iter().enumerate() {
                set_charm(&mut ee, r.item, i < owned);
            }
            let cap = ee.call(0x001d_4e68, &[], &[], STACK).expect("FUN_001d4e68") as u8;
            assert_eq!(cap, summon_level_cap(owned as u32), "cap with {owned} charms");
            for level in 0..=7u8 {
                ee.write(STATUS, &[0u8; 0x200]);
                ee.w32(STATUS + 0x24c, OBJECT);
                ee.call(0x001a_0480, &[STATUS as u64, 3, level as u64], &[], STACK).expect("FUN_001a0480");
                let (timer, segs) = summon_timer(&t, level);
                let set = (ee.r8(STATUS + 0x1af), ee.r8(STATUS + 0x1b3), ee.f32_at(STATUS + 0x1b4), ee.f32_at(STATUS + 0x1b8));
                assert_eq!(set, (3, segs, timer.left, timer.total), "mode set at level {level}");
                for start_exp in [0i32, 1, 7, 29] {
                    let mut port = timer;
                    let mut exp = SummonExp { level, exp: start_exp };
                    ee.wf32(STATUS + 0x1b4, timer.left);
                    ee.write(0x0034_0c8e, &[level]);
                    ee.w32(0x0034_0e4c, start_exp as u32);
                    let steps = [-1.0f32, -(t.point_units * 0.5), -t.point_units, 7.5, -(t.point_units * 1.75), t.point_units * 3.0, -(t.point_units * 2.0), -9999.0];
                    for d in steps {
                        ee.call(0x001a_0358, &[STATUS as u64], &[d], STACK).expect("FUN_001a0358");
                        let drop = port.change(&t, GaugeMode::Summon, 0, d);
                        exp.add(&levels, cap, drop, false);
                        let game = (ee.f32_at(STATUS + 0x1b4), ee.r8(0x0034_0c8e), ee.r32(0x0034_0e4c) as i32);
                        assert_eq!(game, (port.left, exp.level, exp.exp), "charms {owned} level {level} exp {start_exp} step {d}");
                        // FUN_00155bf0 on the same block (panel +0x34 -> object, object +0x48c -> status).
                        ee.w32(PANEL + 0x34, OBJECT);
                        let digit = ee.call(0x0015_5bf0, &[PANEL as u64, OUT as u64, (OUT + 4) as u64], &[], STACK).expect("FUN_00155bf0") as u32;
                        let shown = (digit, ee.r32(OUT) as i32, ee.r32(OUT + 4) as i32);
                        assert_eq!(shown, segment_display(port.left, port.total, segs), "HUD at {}", port.left);
                        checked += 1;
                    }
                }
            }
        }
        // FUN_001c60f8: damage to the summon object changes the player's timer.
        ee.write(SUMMON, &[0u8; 0x600]);
        ee.w32(SUMMON + 0x48c, SUMMON_STATUS);
        for max_hp in [40i32, 213, 1000] {
            for d in [-1i32, -40, -250, 5] {
                ee.write(STATUS, &[0u8; 0x200]);
                ee.w32(STATUS + 0x24c, OBJECT);
                ee.call(0x001a_0480, &[STATUS as u64, 3, 2], &[], STACK).expect("FUN_001a0480");
                ee.w32(SUMMON_STATUS + 4, max_hp as u32);
                let ret = ee.call(0x001c_60f8, &[SUMMON as u64, d as i64 as u64], &[], STACK).expect("FUN_001c60f8") as i32;
                assert_eq!(ret, max_hp, "the method returns the summon's max HP");
                let mut port = summon_timer(&t, 2).0;
                if let Some(c) = summon_damage_to_timer(&t, d, max_hp) {
                    port.add(0, c);
                }
                assert_eq!(ee.f32_at(STATUS + 0x1b4), port.left, "damage {d} of max HP {max_hp}");
                checked += 1;
            }
        }
        eprintln!("oracle: FUN_001a0480, FUN_001a0358 + FUN_001d4ec8, FUN_001d4e68, FUN_00155bf0, FUN_001c60f8 equal in {checked} cases (gain factor {})", st.gain);
    }
}
