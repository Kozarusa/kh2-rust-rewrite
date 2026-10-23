//! The drive rules and state the lower crates read (moved out of `drive.rs`, which re-exports them).

use bevy::prelude::*;
use kh2_tables::player::{self, form, DriveForm, FormLevels, Memt, Sstm};
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::data::KINGDOM_KEY;
use crate::progress::Progress;

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
    // Summons (docs/systems/drive-limits-summons/summons-spec.md sections 1 and 2). The summon uses the same gauge fields as the form
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
    // Its caller (the summon object, the summon limit task) comes in a later stage of docs/systems/drive-limits-summons/contract.md.
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
    // Its caller (the summon object, the summon limit task) comes in a later stage of docs/systems/drive-limits-summons/contract.md.
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
    pub memt: Memt,
    pub objs: Vec<t::Objentry>,
    pub items: Vec<t::Item>,
    pub wmst: Vec<[String; 2]>,
    pub went: Vec<u8>,
    /// The equipped keyblade item (Kingdom Key; the party record's weapon word, hand 0 of every
    /// player object through `FUN_0016ee98`).
    pub key_item: u16,
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
    pub fn item_slot(&self, item: u16) -> Option<u16> {
        self.items.iter().find(|i| i.id == item).map(|i| i.flag1 as u16 | (i.rank as u16) << 8)
    }

    /// Files of a player object: `obj` is its objentry id; `hand1_item` the form record's weapon
    /// word (0 for the base body, which only has hand 0: `FUN_00176f88`). Each hand's weapon is
    /// the moveset's `went` entry for the item (`FUN_0016ef28`, `FUN_001845b0`) and its motion set
    /// the `wmst` name of that hand (`FUN_00184640`).
    pub fn object_files(&self, obj: u16, outfit: u8, hand1_item: u16) -> Option<FormFiles> {
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
    pub fn merged(&self, world: u16) -> [u16; 24] {
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
    pub ticks: u32,
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
    pub fn from_env(rules: &DriveRules, progress: Option<&Progress>) -> DriveState {
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

/// The world the player objects are looked up for (the `memt` merge key: 13, 14 and 17 select the
/// `_WI`, `_NM`, `_TR` objects; 2 is Twilight Town) and its outfit index (word 8 + outfit of the
/// merged record for the base body; a new game has 0).
#[derive(Resource, Clone, Copy, Debug)]
pub struct DriveWorld {
    pub id: u16,
    pub outfit: u8,
}
