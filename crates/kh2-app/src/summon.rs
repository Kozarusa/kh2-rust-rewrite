//! Summons, stage 2 of docs/summons.md section 6: the gauge and the command without the summon
//! object. The rules are pure functions in `drive::rules` (the summon uses the drive gauge under mode
//! byte 3); this module keeps the summon's own state and runs the sequence on the player:
//!
//! - Summon (`Execute` 72, `FUN_001cf030` -> `FUN_001c6168`): the cost in drive points (`FUN_001a0320`),
//!   the active summon index (`DAT_00340c88` byte 5), the sequence bit (`0x35feb8` bit 0), the summon
//!   state (`DAT_01d5e368`) 2, Peter Pan's Tinker Bell byte (byte 7), the use counter; then the arrival
//!   wait (`FUN_001c6260`): `sstm[0x88]` frames (the arrival without its event), and gauge mode 3
//!   (`FUN_001a0480(status, 3, summon level)`).
//! - Mode 3 each tick: the drain (`FUN_00177788` with the ability block's Summon Boost factor), the
//!   whole segments lost feed the summon experience (`FUN_001a0358` -> `FUN_001d4ec8`), and hits Sora
//!   lands add `DriveDrain x sstm[0x70]` to the timer while it is not empty (`FUN_00178748` ->
//!   `FUN_001788d8`).
//! - End: the timer at 0 (`FUN_001ceda8` -> `FUN_001cf298`) or Dismiss (`FUN_001cf030` case 316): the
//!   leave (`FUN_001c67b0`: state 3, byte 5 = 0, then the leave thread clears `0x35feb8` bit 0) and
//!   gauge mode 1.
//!
//! Not in this stage (docs/summons.md section 6, stages 3 to 6): the summon object (spawn, script,
//! `appear_skip` / `leave`), the companions' fade out and return (`FUN_001c7740`, `FUN_001c79c8`;
//! the arrival therefore does not wait for `FUN_0017d470`), the player's motions 0x8a / 0x82 and
//! flags +0x108, system effect 0x13, actor stop id 2, the arrival event and its config option, Genie's
//! form byte (`FUN_001c6a88`), the summon face panel, summon limits.
//!
//! Harness: `KH2_SUMMON=<sumn index 1..4 or command id>` asks for that summon at `KH2_SUMMON_AT`
//! seconds (default 4); `KH2_DISMISS_AT=<s>` asks for Dismiss. The command menu (`items.rs`) issues
//! the same requests: the Summon entry is on the second deck, which `H` (`KH2_KEYS` `t:H`) swaps in
//! (the original's d-pad Left at the deck). `KH2_SUMMON_PARTY=1` takes both party slots as filled
//! when no companion is spawned (harness only: the original refuses Summon then, `FUN_001c78f0`).
//! Logs: `summon start`, `summon: gauge mode 3`, `summon: timer` once a second, `summon: level up`,
//! `summon end (timer)` / `summon end (dismiss)`.

use bevy::prelude::*;
use kh2_tables::tables::{self as t, Cmd};

use crate::abilities::Abilities;
use crate::assets::Disc;
use crate::combat::Hit;
use crate::data::GameData;
use crate::drive::rules::{self, GaugeMode};
use crate::drive::{DriveRules, DriveState};
use crate::progress::Progress;
use crate::schedule::Sim;
use crate::sora::{Battle, Sora};

/// Command ids (immediates of the code).
pub mod cmd {
    /// Dismiss (`FUN_001ceda8` looks it up with `FUN_001b6f48(0x13c)`).
    pub const DISMISS: u16 = 0x13c;
    /// Peter Pan's command: byte 7 = 1 (Tinker Bell ready, `FUN_001c6168`).
    pub const PETER_PAN: u16 = 0x132;
    /// Genie's command: byte 7 = a random owned drive form (`FUN_001c6a88`, not ported).
    pub const GENIE: u16 = 0x59;
}

/// The disc data the summons read.
#[derive(Resource)]
pub struct SummonData {
    /// The `sumn` rows (1-based index = row + 1).
    pub rows: Vec<t::Sumn>,
    pub cmds: Vec<Cmd>,
    pub tuning: rules::SummonTuning,
    /// Command names for the log (`msg/us/sys.bar`).
    names: Vec<(u16, String)>,
}

impl SummonData {
    fn load(disc: &Disc) -> Option<SummonData> {
        let mut a = disc.0.lock().ok()?;
        let rows = kh2_tables::load::<t::Sumn>(&mut a, &t::SUMN).ok()?.rows;
        let cmds = kh2_tables::load::<Cmd>(&mut a, &t::CMD).ok()?.rows;
        let sstm = kh2_tables::player::load_sstm(&mut a).ok()?;
        let msg = kh2_tables::player::load_messages(&mut a);
        let names = rows
            .iter()
            .map(|r| r.command)
            .chain([cmd::DISMISS])
            .map(|c| {
                let n = cmds.iter().find(|x| x.id == c).and_then(|x| msg.as_ref()?.get(x.message_id as u32)).unwrap_or_else(|| format!("command {c}"));
                (c, n)
            })
            .collect();
        Some(SummonData { rows, cmds, tuning: rules::SummonTuning::from_sstm(&sstm), names })
    }

    pub fn name(&self, c: u16) -> String {
        self.names.iter().find(|n| n.0 == c).map_or_else(|| format!("command {c}"), |n| n.1.clone())
    }

    /// `FUN_001d4ca0`: the 1-based row index of a summon command.
    pub fn index_of(&self, c: u16) -> Option<u8> {
        self.rows.iter().position(|r| r.command == c).map(|i| i as u8 + 1)
    }

    fn cmd(&self, c: u16) -> Option<&Cmd> {
        self.cmds.iter().find(|x| x.id == c)
    }
}

/// The summon's state (the game keeps it in `DAT_00340c88` bytes 5..7, `DAT_00340e4c`,
/// `DAT_01d5e368`, the words `0x35feb8` / `0x35fec0` and the two object slots at 0x1d5e360).
#[derive(Resource, Clone, Debug, Default)]
pub struct SummonState {
    /// The active summon, a 1-based `sumn` index (byte 5), 0 for none.
    pub index: u8,
    /// Summon level (byte 6) and experience (`DAT_00340e4c`).
    pub exp: rules::SummonExp,
    /// Genie's form / Tinker Bell byte (byte 7).
    pub extra: u8,
    /// `DAT_01d5e368`: 0 none, 1 the arrival event plays, 2 active, 3 leaving.
    pub state: u8,
    /// `0x35feb8` bit 0: a summon sequence runs (set at the start, cleared by the leave thread).
    pub sequence: bool,
    /// The arrival wait left in frames while `0x35fec0` bit 3 is up (`FUN_001c6578` .. `FUN_001c65b8`).
    pub arrival: Option<f32>,
    /// The summon objects (0x1d5e360 / 0x1d5e364); always empty in this stage (no object).
    pub slots: [Option<Entity>; 2],
    /// Each summon command's use counter (+0x34184a + 2 x index, capped at 999).
    pub uses: [u16; 4],
    /// The SUMMONS list (`FUN_001d4d48`): every `sumn` command whose charm is owned, in row order.
    pub commands: Vec<u16>,
    /// The summon level cap (`FUN_001d4e68`).
    pub cap: u8,
    /// `FUN_001c78f0` this tick: both party slots hold members in the party.
    pub party_ok: bool,
    /// The mode-3 tick counter (for the once-a-second log line).
    ticks: u32,
}

impl SummonState {
    /// The summon timer is non-zero (what limit.rs calls the summon gauge, status +0x1b4 in mode 3).
    pub fn gauge(&self, drive: &DriveState) -> bool {
        drive.gauge_mode == GaugeMode::Summon && drive.timer.left > 0.0
    }

    /// Dismiss is available (`FUN_001ba2c0` case 0x13c). The limit task test (`FUN_001b3c00`) reads
    /// no task: limit.rs keeps none that this module can see.
    pub fn dismiss_ok(&self) -> bool {
        rules::dismiss_available(self.state, false)
    }

    /// A summon command is available (`FUN_001ba568` with `FUN_001c7970`).
    pub fn command_ok(&self, drive: &DriveState, c: &Cmd) -> bool {
        rules::summon_available(&rules::SummonCtx {
            busy: self.sequence || self.arrival.is_some(),
            form: drive.object_form(),
            party_ok: self.party_ok,
            locked: false,
            points: drive.points.whole,
            cost: c.cost,
        })
    }
}

/// Requests to the summon system, from the command menu or the harness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummonRequest {
    /// A summon command (its id).
    Summon(u16),
    Dismiss,
}

#[derive(Resource, Default)]
pub struct SummonQueue(pub Vec<SummonRequest>);

/// The scripted harness.
#[derive(Resource, Default)]
struct Harness {
    summon: Option<(f32, u16)>,
    dismiss_at: Option<f32>,
    force_party: bool,
}

/// What the HUD's drive gauge showed before mode 3 (put back when the mode leaves 3).
#[derive(Resource, Default)]
struct HudSaved(Option<crate::hud::hud_gauge::DriveGauge>);

pub struct SummonPlugin;

impl Plugin for SummonPlugin {
    fn build(&self, app: &mut App) {
        let Some(data) = app.world().get_resource::<Disc>().and_then(SummonData::load) else {
            warn!("summon: tables not readable; summons are off");
            return;
        };
        crate::progress::ensure(app);
        let progress = app.world().get_resource::<Progress>();
        let exp = progress.map_or(rules::SummonExp { level: 1, exp: 0 }, |p| rules::SummonExp { level: p.summon_level(), exp: p.summon_exp() as i32 });
        let env_f = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
        let summon = std::env::var("KH2_SUMMON").ok().and_then(|v| v.trim().parse::<u16>().ok()).map(|v| {
            // 1..=4 names a `sumn` row, anything else a command id.
            let c = if (1..=4).contains(&v) { data.rows.get(v as usize - 1).map_or(v, |r| r.command) } else { v };
            (env_f("KH2_SUMMON_AT").unwrap_or(4.0), c)
        });
        let harness = Harness { summon, dismiss_at: env_f("KH2_DISMISS_AT"), force_party: std::env::var("KH2_SUMMON_PARTY").is_ok_and(|v| v == "1") };
        info!(
            "summon: {} rows ({}), level {} exp {}, wait {} frames (with event {}), gauge change factor {}",
            data.rows.len(),
            data.rows.iter().map(|r| format!("{} charm {}", data.name(r.command), r.item)).collect::<Vec<_>>().join(", "),
            exp.level,
            exp.exp,
            data.tuning.wait,
            data.tuning.wait_event,
            data.tuning.gain
        );
        app.insert_resource(data)
            .insert_resource(SummonState { exp, ..Default::default() })
            .insert_resource(harness)
            .init_resource::<SummonQueue>()
            .init_resource::<HudSaved>()
            .add_systems(Update, (keys.before(crate::magic::harness_keys), harness_requests, hud_feed.before(crate::hud::hud_gauge::update)))
            .add_systems(FixedUpdate, tick.in_set(Sim::Sounds));
    }
}

/// `KH2_KEYS` entries `t:H`: the key the command menu reads as d-pad Left (the deck swap).
fn keys(time: Res<Time>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    let Ok(spec) = std::env::var("KH2_KEYS") else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    for item in spec.split(',') {
        let mut parts = item.split(':');
        let (Some(at), Some("H")) = (parts.next(), parts.next()) else { continue };
        let hold: f32 = parts.next().and_then(|h| h.parse().ok()).unwrap_or(0.05);
        let Ok(at) = at.parse::<f32>() else { continue };
        if t >= at && t - dt < at {
            keys.press(KeyCode::KeyH);
        } else if t >= at + hold && t - dt < at + hold {
            keys.release(KeyCode::KeyH);
        }
    }
}

fn harness_requests(time: Res<Time>, h: Res<Harness>, mut queue: ResMut<SummonQueue>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs());
    let crossed = |at: f32| now >= at && now - dt < at;
    if let Some((at, c)) = h.summon
        && crossed(at)
    {
        queue.0.push(SummonRequest::Summon(c));
    }
    if let Some(at) = h.dismiss_at
        && crossed(at)
    {
        queue.0.push(SummonRequest::Dismiss);
    }
}

/// The leave (`FUN_001c67b0`) and gauge mode 1 (`FUN_001cf298` / `FUN_001cf030` case 316).
fn leave(sm: &mut SummonState, drive: &mut DriveState, why: &str, name: &str) {
    if sm.state == 0 {
        return;
    }
    sm.state = 3;
    let index = std::mem::take(&mut sm.index);
    sm.arrival = None;
    drive.gauge_mode = GaugeMode::Normal;
    info!(
        "summon end ({why}): {name} (index {index}), timer {:.0}/{:.0}, gauge mode 1, drive points {} + {}%, summon level {} exp {}",
        drive.timer.left, drive.timer.total, drive.points.whole, drive.points.fraction, sm.exp.level, sm.exp.exp
    );
}

#[allow(clippy::too_many_arguments)]
fn tick(
    data: Res<SummonData>,
    rules_d: Res<DriveRules>,
    game: Res<GameData>,
    progress: Option<Res<Progress>>,
    abilities: Option<Res<Abilities>>,
    battle: Res<Battle>,
    h: Res<Harness>,
    mut sm: ResMut<SummonState>,
    mut queue: ResMut<SummonQueue>,
    mut drive: ResMut<DriveState>,
    mut hits: MessageReader<Hit>,
    sora: Query<(Entity, &Sora)>,
    party: Query<&crate::party::PartyMember>,
) {
    let Ok((root, s)) = sora.single() else {
        hits.clear();
        return;
    };
    let sm = &mut *sm;
    let st = &mut *drive;
    let tuning = rules_d.tuning;
    // The SUMMONS list and the level cap from the charms owned (FUN_001d4d48, FUN_001d4e68).
    let owned = |item: u16| progress.as_ref().is_some_and(|p| p.item_count(item) > 0);
    sm.commands = data.rows.iter().filter(|r| owned(r.item)).map(|r| r.command).collect();
    sm.cap = rules::summon_level_cap(data.rows.iter().filter(|r| owned(r.item)).count() as u32);
    // FUN_001c78f0: party slots 0 and 1 hold members (the app's members are always in the party).
    let slot = |n: u8| party.iter().any(|m| m.slot == n);
    sm.party_ok = (slot(0) && slot(1)) || h.force_party;
    // The leave thread (0x1c6810) once the slots are empty: its cleanup clears `0x35feb8` bit 0. The
    // state goes back to 0 when the last summon object is destroyed (FUN_001c5f80 / FUN_001c6038);
    // with no object in this stage the thread's end stands in for that [inferred].
    if sm.state == 3 && sm.slots.iter().all(|o| o.is_none()) {
        sm.state = 0;
        sm.sequence = false;
        info!("summon: leave done (summon state 0, the sequence bit cleared)");
    }

    for req in std::mem::take(&mut queue.0) {
        match req {
            SummonRequest::Summon(c) => {
                let (Some(row), Some(index)) = (data.cmd(c), data.index_of(c)) else {
                    info!("summon: command {c} is not a summon command");
                    continue;
                };
                // The menu offers only usable entries; the harness gets the same tests, with the
                // command's `Flags` bit 1 (no use while airborne, FUN_001b70b8).
                let airborne = row.flags >> 1 & 1 != 0 && !s.motor.grounded;
                if !owned(data.rows[index as usize - 1].item) || airborne || !sm.command_ok(st, row) {
                    info!(
                        "summon: {} refused (charm owned {}, airborne {airborne}, sequence {}, arrival {}, party {}, form {}, points {} cost {})",
                        data.name(c),
                        owned(data.rows[index as usize - 1].item),
                        sm.sequence,
                        sm.arrival.is_some(),
                        sm.party_ok,
                        st.object_form(),
                        st.points.whole,
                        row.cost
                    );
                    continue;
                }
                let before = st.points;
                st.points.spend(row.cost);
                sm.index = index;
                sm.sequence = true;
                sm.state = 2;
                if c == cmd::PETER_PAN {
                    sm.extra = 1;
                }
                let u = &mut sm.uses[index as usize - 1];
                *u = (*u + 1).min(999);
                sm.arrival = Some(data.tuning.wait);
                info!(
                    "summon start: {} (command {c}, index {index}, cost {}, drive points {} -> {}, summon level {} exp {} cap {}); arrival wait {} frames (sstm 0x88, no arrival event){}",
                    data.name(c),
                    row.cost,
                    before.whole,
                    st.points.whole,
                    sm.exp.level,
                    sm.exp.exp,
                    sm.cap,
                    data.tuning.wait,
                    if c == cmd::GENIE { "; Genie's form byte (FUN_001c6a88) not ported" } else { "" }
                );
            }
            SummonRequest::Dismiss => {
                if sm.dismiss_ok() {
                    let name = data.rows.get((sm.index as usize).wrapping_sub(1)).map_or_else(String::new, |r| data.name(r.command));
                    leave(sm, st, "dismiss", &name);
                } else {
                    info!("summon: Dismiss refused (summon state {})", sm.state);
                }
            }
        }
    }

    // The arrival wait (FUN_001c6260, the path without the event), then gauge mode 3.
    if let Some(w) = sm.arrival.as_mut() {
        *w -= rules::FRAME_UNIT;
        if *w <= 0.0 {
            sm.arrival = None;
            let (timer, segments) = rules::summon_timer(&tuning, sm.exp.level);
            st.timer = timer;
            st.segments = segments;
            st.gauge_mode = GaugeMode::Summon;
            sm.ticks = 0;
            info!("summon: gauge mode 3: timer {:.0}/{:.0}, {segments} segments (summon level {})", timer.left, timer.total, sm.exp.level);
        }
    }

    if st.gauge_mode != GaugeMode::Summon {
        hits.clear();
        return;
    }
    let mut segments_lost = 0;
    // Hits Sora lands while the timer is not empty (FUN_00178748 -> FUN_001788d8 mode 3; the victim
    // flags 0x10 / 0x800 are not modelled, as in drive.rs).
    for h in hits.read().filter(|h| h.attacker == root) {
        let Some(d) = game.atkp(h.atkp).map(|a| a.drive_drain).filter(|&d| d != 0) else { continue };
        if st.timer.left == 0.0 {
            continue;
        }
        let gain = rules::summon_gauge_change(&data.tuning, d as f32);
        segments_lost += st.timer.change(&tuning, GaugeMode::Summon, st.form, gain);
        info!("summon: landed hit (atkp {}, DriveDrain {d}): timer +{gain:.1} -> {:.0}", h.atkp, st.timer.left);
    }
    // The drain (FUN_00177788; no event runs in the app).
    let factor = rules::mode_drain_factor(GaugeMode::Summon, 1.0, abilities.as_ref().map_or(1.0, |a| a.block().summon_drain)).unwrap_or(1.0);
    let battle_on = st.force_battle || battle.active();
    let drain = rules::drain_per_tick(&tuning, battle_on, factor);
    segments_lost += st.timer.change(&tuning, GaugeMode::Summon, st.form, -drain);
    // The experience (FUN_001d4ec8; the minigame flag of FUN_001b9bd8 is never set in the app).
    if segments_lost > 0 {
        let before = sm.exp;
        let reached = sm.exp.add(&rules_d.levels, sm.cap, segments_lost, false);
        info!("summon: {segments_lost} segment(s) used: experience {} -> {} (level {})", before.exp, sm.exp.exp, sm.exp.level);
        for l in reached {
            info!("summon: level up to {l} (notice 0xc5bd not shown: FUN_001d4fa0 not ported)");
        }
    }
    sm.ticks += 1;
    if sm.ticks.is_multiple_of(crate::data::LOGIC_HZ as u32) {
        let (digit, fill, seg) = rules::segment_display(st.timer.left, st.timer.total, st.segments);
        info!(
            "summon: timer {:.0}/{:.0} ({} whole segments, HUD digit {digit} fill {fill}/{seg}), drain {drain:.2}/tick (battle {battle_on}, Summon Boost factor {factor}), level {} exp {}",
            st.timer.left,
            st.timer.total,
            st.timer.whole_points(&tuning),
            sm.exp.level,
            sm.exp.exp
        );
    }
    // The timer at 0 (FUN_001ceda8: summon state 2 and Dismiss accepted -> FUN_001cf298). The game
    // tests +0x1b4 in any mode; only mode 3 is tested here: during the arrival the player is busy
    // (+0x108 bit 4, FUN_0019abb0) [inferred].
    if st.timer.left <= 0.0
        && sm.state == 2
        && sm.dismiss_ok()
        && let Some(dismiss) = data.cmd(cmd::DISMISS)
        && !(dismiss.flags >> 1 & 1 != 0 && !s.motor.grounded)
    {
        let name = data.rows.get((sm.index as usize).wrapping_sub(1)).map_or_else(String::new, |r| data.name(r.command));
        leave(sm, st, "timer", &name);
    }
}

/// Mode 3 on the HUD's drive gauge (`FUN_00155bf0`: the digit of the segment and the fill inside it;
/// the art set of mode 3, `DriveMode::FormB`). Outside mode 3 the gauge is left as it was.
fn hud_feed(drive: Option<Res<DriveState>>, mut gauge: ResMut<crate::hud::hud_gauge::DriveGauge>, mut saved: ResMut<HudSaved>) {
    use crate::hud::hud_gauge::{DriveGauge, DriveMode};
    let Some(st) = drive else { return };
    if st.gauge_mode == GaugeMode::Summon {
        if saved.0.is_none() {
            saved.0 = Some(gauge.clone());
        }
        let (digit, fill, seg) = rules::segment_display(st.timer.left, st.timer.total, st.segments);
        *gauge = DriveGauge {
            enabled: true,
            mode: DriveMode::FormB,
            points: digit.min(9) as u8,
            max: st.points.max,
            fraction: 0,
            time_fill: Some(if seg > 0 { fill as f32 / seg as f32 } else { 0.0 }),
        };
    } else if let Some(g) = saved.0.take() {
        *gauge = g;
    }
}
