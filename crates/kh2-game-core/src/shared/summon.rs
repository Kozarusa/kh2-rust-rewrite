//! The summon state, its requests and the revive thread the lower crates read (moved out of `summon.rs`, which re-exports them).

use bevy::prelude::*;
use kh2_tables::tables::Cmd;

use crate::shared::drive::rules::{self as drive_rules, GaugeMode};
use crate::shared::drive::DriveState;

/// The summon's state (the game keeps it in `DAT_00340c88` bytes 5..7, `DAT_00340e4c`,
/// `DAT_01d5e368`, the words `0x35feb8` / `0x35fec0` and the two object slots at 0x1d5e360).
#[derive(Resource, Clone, Debug, Default)]
pub struct SummonState {
    /// The active summon, a 1-based `sumn` index (byte 5), 0 for none.
    pub index: u8,
    /// Summon level (byte 6) and experience (`DAT_00340e4c`).
    pub exp: drive_rules::SummonExp,
    /// Genie's form / Tinker Bell byte (byte 7).
    pub extra: u8,
    /// `DAT_01d5e368`: 0 none, 1 the arrival event plays, 2 active, 3 leaving.
    pub state: u8,
    /// `0x35feb8` bit 0: a summon sequence runs (set at the start, cleared by the leave thread).
    pub sequence: bool,
    /// The arrival wait left in frames while the arrival thread (0x1c6260) runs.
    pub arrival: Option<f32>,
    /// The arrival plays as an event (`FUN_001c65f0`).
    pub event: bool,
    /// `DAT_01d5e370`: the summons whose arrival event has played (bit = index).
    pub seen: u32,
    /// `0x35fec0` bit 3 with actor stop id 2 on the actors of `STOP_MASK` (`FUN_001c6578` ..
    /// `FUN_001c65b8`).
    pub stop: bool,
    /// The player's +0x108 bits the sequence sets: 0x100 (no hit reaches him, `FUN_00186638`), 0x4
    /// (busy, `FUN_0019a530`), 0x1 (from mode 3 to the end of the arrival) [meanings inferred].
    pub player_flags: u32,
    /// The summon objects (0x1d5e360 / 0x1d5e364).
    pub slots: [Option<Entity>; 2],
    /// The NeoStatus of each summon object (what `FUN_00175ba0` finds a summon limit's member by).
    pub present: Vec<u16>,
    /// Each summon command's use counter (+0x34184a + 2 x index, capped at 999).
    pub uses: [u16; 4],
    /// The SUMMONS list (`FUN_001d4d48`): every `sumn` command whose charm is owned, in row order.
    pub commands: Vec<u16>,
    /// The summon level cap (`FUN_001d4e68`).
    pub cap: u8,
    /// `FUN_001c78f0` this tick: both party slots hold members in the party.
    pub party_ok: bool,
    /// The limit task exists (`FUN_001b3c00`: `DAT_01d5d6b0` != 0), from limit.rs.
    pub limit_task: bool,
    /// `DAT_003716a0` and `DAT_0035e9b0` (the game's LCG and the free-spot sector counter).
    pub rng: u32,
    pub sector: i32,
    /// The form command of "Drive!" waiting to reach Genie's script as event 11.
    pub pending_form: Option<u16>,
    /// The enemy the player's latest hit landed on (a stand-in for `0x34e9c4`, FUN_00112850 at his attack's start).
    pub player_hit: Option<Entity>,
    /// Objects the summon scripts made with host 1.0 (`ai::SummonAi::makes`), waiting for `act` to build them: handle and
    /// `00objentry` id.
    pub kid_makes: Vec<(u32, u32)>,
    /// What the scripts asked of those objects, with the ticks each call has waited for its object.
    pub kid_ops: Vec<(u32, KidOp, u8)>,
    /// The serial of the last object made (their handles are `KID_BASE + serial`).
    pub kid_serial: u32,
    /// The leave thread (0x1c6810) runs.
    pub leaving: bool,
    /// Tinker Bell's revive sequence (`FUN_001ad378` type 2 and its thread `FUN_001ad9a0`).
    pub revive: Revive,
    /// What the sequence did this tick, for `revive_effects` (music, sound, log).
    pub revive_acts: Vec<ReviveAct>,
    /// The mode-3 tick counter (for the once-a-second log line).
    pub ticks: u32,
}

impl SummonState {
    /// The summon timer is non-zero (what limit.rs calls the summon gauge, status +0x1b4 in mode 3).
    pub fn gauge(&self, drive: &DriveState) -> bool {
        drive.gauge_mode == GaugeMode::Summon && drive.timer.left > 0.0
    }

    /// Genie is out: the state is 2 and `FUN_001c6940` (the player-type id of the object at `DAT_01d5e360`) answers his
    /// NeoStatus 0xc9 (`FUN_001bc6e0`'s test for the "Drive!" entry). The summon objects' NeoStatus is looked up among
    /// all of them, not only the first slot [inferred: Genie is a single-summon object].
    pub fn genie_out(&self) -> bool {
        self.state == 2 && self.present.contains(&GENIE_NEO)
    }

    /// Dismiss is available (`FUN_001ba2c0` case 0x13c: the state is 2 and `FUN_001b3c00` finds no
    /// limit task).
    pub fn dismiss_ok(&self) -> bool {
        drive_rules::dismiss_available(self.state, self.limit_task)
    }

    /// A summon command is available (`FUN_001ba568` with `FUN_001c7970`).
    pub fn command_ok(&self, drive: &DriveState, c: &Cmd) -> bool {
        drive_rules::summon_available(&drive_rules::SummonCtx {
            busy: self.sequence || self.arrival.is_some(),
            form: drive.object_form(),
            party_ok: self.party_ok,
            locked: false,
            points: drive.points.whole,
            cost: c.cost,
        })
    }
}

impl SummonState {
    /// The revive's camera event holds the pad (`Revive::pad_locked`).
    pub fn revive_pad_locked(&self) -> bool {
        self.revive.pad_locked()
    }
}

/// Requests to the summon system, from the command menu, limit.rs or the harness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SummonRequest {
    /// A summon command (its id).
    Summon(u16),
    Dismiss,
    /// A change of the mode-3 timer through `FUN_001788d8` (already multiplied by `sstm[0x70]`).
    Gauge(f32),
    /// Genie's "Drive!" list: the form command chosen (535..538, `Execute` handled by the generic executor FUN_00177c20 ->
    /// FUN_0016b420, which raises event 11 on the first summon object's script with (the player, the command, a target
    /// record)). The command menu issues it (`items.rs`: the second deck's "Drive!" list while Genie is out, `SummonState::genie_out`);
    /// the harness also pushes it (`KH2_GENIE_FORM`).
    Form(u16),
}

#[derive(Resource, Default)]
pub struct SummonQueue(pub Vec<SummonRequest>);

/// What Tinker Bell's revive sequence does, in the order the game does it (`FUN_001ad378` type 2, then the thread
/// `FUN_001ad9a0`, then `FUN_001adeb8`). The acts are carried out by `revive_effects`; the state machine
/// itself needs neither audio nor a world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviveAct {
    /// `DAT_0035d268` bit 1 set (`FUN_001ad378`); cleared again by `FlagsClear`.
    FlagsSet,
    /// `FUN_00137520`: the music state is kept and cleared (0x354ebc -> 0x354ebe), the channel ramps `FUN_002c1a08` and the
    /// BGM stream stop `FUN_00136a68` (0x1e0).
    MusicStop,
    /// `FUN_002c14a8(0x13, 0)`: system sound 0x13 (type 2).
    Sound(u32),
    /// `FUN_00195898` / `FUN_00195a78` / `FUN_00195ae0` build the revive's "window", which is a camera event (kind 0xf) and not a text
    /// window: the word FUN_00195a78 stores is the player object and nothing reads it, so there is no message text. The event itself
    /// is timed by `Revive` (`window_open`, `pad_locked`); the act only logs.
    WindowSkipped,
    /// `FUN_00138708(2)`: `DAT_00354f70` bit 2 set (cleared by `FUN_001ad790`, `FieldReleased`).
    FieldHeld,
    /// Step 1 of the thread: `FUN_002c38b8` sets `DAT_0036f598`. Its only reader is `FUN_002c3228`, the sound engine's request-slot
    /// allocator (`FUN_002c36b8` / `FUN_002c37b8`): while it is set no new positional sound gets a slot. It is not a pad lock.
    SoundGate,
    /// `FUN_002c38c8` clears `DAT_0036f598` again.
    SoundGateOff,
    /// `FUN_00183fe0`: the player is put on the floor point under him.
    PlayerToFloor,
    /// `FUN_001adeb8`: byte 7 cleared (`FUN_001c6a10`) and Tinker Bell made.
    TinkerMade,
    /// `FUN_001adeb8`: `DAT_0035d268` bit 1 cleared.
    FlagsClear,
    /// `FUN_001adeb8` -> `FUN_00137558`: the kept music state back, the area's music started again.
    MusicRestart,
    /// The thread ends (`FUN_001ad960` -> `FUN_001ad790`): `DAT_00354f70` bit 2 cleared.
    FieldReleased,
}

/// Ticks the revive's camera event lasts: its end frame 0x15 is 21 event frames, an event frame being two game ticks
/// (docs/systems/drive-limits-summons/contract.md, WP-RV2). The window wait (`FUN_00196418`) ends then and the pad is given back.
pub const REVIVE_EVENT_TICKS: u32 = 42;

/// Game tick of the event at which its screen fade starts (event frame 0xb, command 0x12 mode 2).
pub const REVIVE_FADE_START: u32 = 22;

/// Length of that fade in game ticks (the counter `FUN_0014a710` sets is twice the command's length 0x32).
pub const REVIVE_FADE_TICKS: u32 = 100;

/// The command's length (+6 of the command, 0x32).
const REVIVE_FADE_LENGTH: u32 = 0x32;

/// The revive thread (`FUN_001ad9a0`, state word +8) with the object's fields it needs.
#[derive(Debug, Clone, Default)]
pub struct Revive {
    /// `Some(step)` while the thread runs: 0 the first tick, 1 the sound gate, 2 the waits.
    pub step: Option<u8>,
    /// `00objentry` id of Tinker Bell, made when the thread is done.
    pub tinker: u32,
    /// Game ticks since the revive began; the camera event and its fade run on this clock.
    pub clock: u32,
}

impl Revive {
    /// The thread runs.
    pub fn running(&self) -> bool {
        self.step.is_some()
    }

    /// `DAT_0036f598` is set (step 1 on, until the waits are over).
    pub fn sounds_held(&self) -> bool {
        self.step.is_some_and(|s| s >= 2)
    }

    /// The revive's camera event is alive (`FUN_00196418`: `DAT_0035e3a4 != 0`): the first 42 ticks.
    pub fn window_open(&self) -> bool {
        self.running() && self.clock < REVIVE_EVENT_TICKS
    }

    /// The event's `FUN_00196280` took the pad from the players; `FUN_00196170` gives it back when the event ends.
    pub fn pad_locked(&self) -> bool {
        self.window_open()
    }

    /// The fade object 0x356b30 has reached state 4 (the finished fade out, white): 100 ticks after tick 22, tick 122.
    pub fn fade_black(&self) -> bool {
        self.clock >= REVIVE_FADE_START + REVIVE_FADE_TICKS
    }

    /// The white fade's alpha byte, 0 to 0x80, while the revive runs (`revive_fade::alpha_after`, the game's counter). Nothing in the
    /// revive lifts the fade afterwards (docs/systems/drive-limits-summons/contract.md, WP-RV5): the original leaves it at 0x80 until another fade-in call, which the
    /// app does not model, so once the thread ends this is 0 again [conservative].
    pub fn fade_alpha(&self) -> u32 {
        if !self.running() {
            return 0;
        }
        alpha_after(self.clock.saturating_sub(REVIVE_FADE_START), REVIVE_FADE_LENGTH)
    }

    /// `FUN_001ad378` with type 2, started by `FUN_001cec00` -> `FUN_001ad8a8` when Tinker Bell is ready and the player is
    /// down: everything it does before the thread's first tick.
    pub fn begin(&mut self, tinker: u32) -> Vec<ReviveAct> {
        self.step = Some(0);
        self.tinker = tinker;
        self.clock = 0;
        vec![ReviveAct::FlagsSet, ReviveAct::MusicStop, ReviveAct::Sound(0x13), ReviveAct::WindowSkipped, ReviveAct::FieldHeld]
    }

    /// One tick of the thread (`FUN_001ad9a0`): case 0 moves on, case 1 sets the sound gate and falls into case 2, which waits
    /// while the camera event exists (`FUN_00196418`, 42 ticks) or the fade object 0x356b30 is not black (+0xfc != 4, tick 122),
    /// then clears the gate, puts the player on the floor and finishes with `FUN_001adeb8` (type 2).
    pub fn tick(&mut self) -> Vec<ReviveAct> {
        let mut acts = Vec::new();
        if self.step.is_some() {
            self.clock += 1;
        }
        match self.step {
            None => {}
            Some(0) => self.step = Some(1),
            Some(1) => {
                acts.push(ReviveAct::SoundGate);
                self.step = Some(2);
                self.wait(&mut acts);
            }
            Some(_) => self.wait(&mut acts),
        }
        acts
    }

    pub fn wait(&mut self, acts: &mut Vec<ReviveAct>) {
        if self.window_open() || !self.fade_black() {
            return;
        }
        acts.extend([
            ReviveAct::SoundGateOff,
            ReviveAct::PlayerToFloor,
            ReviveAct::TinkerMade,
            ReviveAct::FlagsClear,
            ReviveAct::MusicRestart,
            ReviveAct::FieldReleased,
        ]);
        self.step = None;
    }
}

/// A call a summon's script makes on an object it made with host 1.0 (the object exists a tick later: the calls wait
/// for it, `super::objects`).
#[derive(Clone, Debug, PartialEq)]
pub enum KidOp {
    /// Host 1.148 `set_position` (FUN_0016a618), centimetres, the app's space.
    Position(bevy::prelude::Vec3),
    /// Host 1.8 `set_mode` (FUN_00186db0 on the other object's mode machine).
    Mode(String),
    /// Host 1.28 `hide`.
    Hide,
    /// Hosts 1.70 / 1.71.
    SetFlag(i32),
    ClearFlag(i32),
}

/// The NeoStatus of Genie (`summon::genie::NEO`), which `SummonState::genie_out` looks for.
pub const GENIE_NEO: u16 = 201;

/// The fade alpha that is full: 0x80 is 1.0 on the GS, and the end value `FUN_0014a818` stores.
pub const FULL: u32 = 0x80;

/// The alpha byte after `calls` runs of `FUN_0014a818` in state 3 (the fade out), for a fade of `length` (the command's +6):
/// `FUN_0014a710` makes the counter `2 * length` and the ratio `128 / counter` (`DAT_00379130`); each run takes `DAT_00356f44` (1.0) off
/// the counter and stores `0x80 - (char)(int)(ratio * counter)`, and when the counter is 0 or less it stores 0x80 and the state becomes 4.
/// Single-precision, as the game computes it.
pub fn alpha_after(calls: u32, length: u32) -> u32 {
    let counter0 = (length.max(1) * 2) as f32;
    let ratio = 128.0_f32 / counter0;
    let counter = counter0 - calls as f32;
    if calls == 0 {
        return 0;
    }
    if counter <= 0.0 {
        return FULL;
    }
    FULL - ((ratio * counter) as i32).clamp(0, FULL as i32) as u32
}
