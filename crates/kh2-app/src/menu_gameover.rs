//! The game over (docs/menus.md "Game over", docs/struggle.md "The knock-out").
//!
//! What the original does (`SLUS_210.05`): when the player's HP part reaches 0 (`FUN_00186238` -> `FUN_00186530`) the
//! player class's knock-out method `FUN_001cec00` decides, in this order:
//! * `DAT_0035d268` bit 0x20000 (host 1.317, the Struggle script's event 3): global event 0x5a, no game over (`struggle.rs`);
//! * Tinker Bell ready (`FUN_001c69b0`, Peter Pan out with byte 7 set): the revive sequence, game over type 2
//!   (`FUN_001ad8a8`, `summon.rs`), unless the form field `+0xb50` is 0xb;
//! * a manager of mission kind 2, 0x19 or 0x20 (`FUN_001b9c00`): game over type 3 (`FUN_001ad8e0`);
//! * the form field `+0xb50` in 1..5 (`FUN_001793a8`) or 0xb: `FUN_001b4d78` (the form's own end), no game over;
//! * otherwise game over type 0 (`FUN_001ad810`); with the summon state 2 `FUN_001cf298` ends the summon first.
//!
//! A manager that ends by itself (`FUN_001b9ce0`) starts type 3 for the mission kinds 2, 0x1e and 0x23, else type 1
//! (`FUN_001ad868`, only while none runs). `FUN_001ad378` builds the object: the system sound `0x13` (type 1: `0x33`;
//! `FUN_002c14a8`), the music stop `FUN_00137520`, flag 0x35d268 bit 1, and its task `FUN_001ad9a0` whose steps wait for the
//! fade and the scene (`obj/GAMEOVER.mset`, `FUN_001adcf8`), then open the pause-menu record: `FUN_001bef10(1)` (types 0,
//! 1: "Continue", "Load Game") or `FUN_001bef10(0x21)` (type 3: "Quit", "Retry"), through `FUN_0024f330` (menu kind 7, the
//! pointer list of `menu_struggle`, no cancel). The page handler `FUN_0026fda0`:
//! * code 3, 4 (type 3): `FUN_001b9c58` (events 0x8b / 0x8d to the area triggers, then their jump records);
//! * code 1 "Load Game": `FUN_00254a08(2)` leaves the list for the file screens (kind 2);
//! * code 2 "Continue": the type's callback `LAB_001adc70` (object +0x48, `FUN_0026fda0`'s default branch) starts the jump
//!   task `FUN_00104e18(FUN_001cb3b8(), 0x80000001, 1, 1, 0)`: its fade-out step restores the snapshot of the save body
//!   (`FUN_001cb208`) and enters the area record `DAT_01d69890`. `FUN_001047a8`, the start of every area, takes the snapshot
//!   (`FUN_001cafa0`) when `FUN_001cb358` allows: a manager with header flag 0x1000, or no manager and the area's map
//!   program below 0x33.
//!
//! The port: the KO decision is `route`; the snapshot is the area record and Sora's HP at each area start plus the save body
//! as a slot file (`capture_body`; "Continue" puts it back, `restore_snapshot`); the list is
//! `menu_struggle`'s pointer list over the rows of the disc's table; "Continue" jumps to the snapshot's area with the fade
//! of kind 1; "Retry" and "Quit" take the manager's trigger jumps as the Struggle's result menu does; "Load Game" opens the
//! Load screen of `menu_save` (menu kind 2, docs/menus.md 18.6): a read slot replaces the state and enters its area, a cancel
//! brings the list back. The list shows the player's model lying in the scene's motion (`model`, `obj/GAMEOVER.mset`, docs 26.6) and
//! "Continue" ends with the party refresher `FUN_001abfe8(8)` (full HP and MP, `refresh_party`, docs 26.7). The wait before the list and the
//! header group are not ported (see docs).

use bevy::prelude::*;
use kh2_image::layd;
use kh2_image::text::Style;

use crate::area::AreaRecord;
use crate::area_jump::{disc_localset, resolve_jump, Dest, JumpTask};
use crate::assets::{CurrentArea, Disc};
use crate::audio::{Audio, PlaySe};
use crate::audio_bgm::AreaMusic;
use crate::drive::DriveState;
use crate::menu::{close_brightness, dim_alpha, menu_sound, CLOSE_FRAMES, DIM_BRIGHTNESS};
use crate::menu::save::{LoadOutcome, Loaded, SaveMenu};
use crate::menu_gfx::{self, MenuScene, Node};
use crate::menu_struggle::{self as list, Action, Block, Flow, Press};
use crate::mickey;
use crate::party::PartyMember;
use crate::menu::new_tags::NewTags;
use crate::progress::{Progress, SlotInfo};
use crate::schedule::{Pause, Sim};
use crate::sora::{Sora, State};
use crate::struggle::{GameOverExits, Mission};
use crate::summon::{SummonData, SummonState};

/// The pause-menu record of types 0 and 1 (`FUN_001ad640`: `FUN_001bef10(1)`) and of type 3 (`FUN_001bef10(0x21)`).
pub const RECORD_FIELD: u16 = 1;
pub const RECORD_MISSION: u16 = 0x21;
/// The action codes of the rows: "Continue" (the callback), "Load Game" (`FUN_00254a08(2)`), "Retry" and "Quit" (`FUN_001b9c58`).
pub const CODE_CONTINUE: u16 = 2;
pub const CODE_LOAD: u16 = 1;
pub const CODE_RETRY: u16 = 3;
pub const CODE_QUIT: u16 = 4;
/// The system sounds `FUN_001ad378` plays: type 1 `0x33`, the others `0x13`.
pub const SOUND_OVER: u32 = 0x13;
pub const SOUND_OVER_TYPE_1: u32 = 0x33;
/// The area map program below which `FUN_001cb358` allows the snapshot (`DAT_0033e830` byte 3).
pub const SNAPSHOT_PROGRAM_LIMIT: u8 = 0x33;
/// Seconds Sora lies down before the list opens [inferred: the steps of `FUN_001ad9a0` wait for the fade and the scene
/// `GAMEOVER.mset`, whose lengths were not read].
pub const OPEN_AFTER: f32 = 1.5;
/// The fade kind of the jump `FUN_00104e18(.., 0x80000001, ..)` (`FUN_0013ef68(0x80000001)`: kind 1).
pub const CONTINUE_FADE_KIND: u32 = 1;

/// What `FUN_001cec00` needs to know.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoInput {
    /// `DAT_0035d268` bit 0x20000.
    pub ko_bit: bool,
    /// `FUN_001c69b0`.
    pub tinker_ready: bool,
    /// `FUN_001b9c00`: a manager of mission kind 2, 0x19 or 0x20 runs.
    pub menu_manager: bool,
    /// `FUN_001793a8`: the form field is in 1..5 (a drive form).
    pub in_form: bool,
}

/// What the knock-out does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Global event 0x5a to the area (`FUN_00197b70`); `struggle.rs`.
    MissionEvent,
    /// Game over type 2: Tinker Bell's revive (`FUN_001ad8a8`); `summon.rs`.
    TinkerRevive,
    /// `FUN_001b4d78`: the form ends; no game over.
    FormEnds,
    /// Game over type 0 or 3.
    GameOver(u8),
}

/// `FUN_001cec00`'s decision.
pub fn route(i: KoInput) -> Route {
    if i.ko_bit {
        return Route::MissionEvent;
    }
    if i.tinker_ready {
        return Route::TinkerRevive;
    }
    if i.menu_manager {
        return Route::GameOver(3);
    }
    if i.in_form {
        return Route::FormEnds;
    }
    Route::GameOver(0)
}

/// The pause-menu record a game over type opens (`FUN_001ad640`).
pub fn record_of(kind: u8) -> u16 {
    if kind == 3 { RECORD_MISSION } else { RECORD_FIELD }
}

/// What a chosen row leads to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Retry,
    Quit,
    /// "Load Game" read a slot (`menu_save` kind 2): enter the area it holds (`FUN_00103b38`, mode 2).
    Load(Loaded),
    /// "I won't give up!" of the alternate list (code 9): Mickey's rescue (`mickey.rs`).
    Mickey,
}

/// The stages of the game over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The knock-out is down; the list opens after `OPEN_AFTER` seconds.
    Waiting,
    List,
    /// "Load Game" (code 1): the Load screen (`FUN_00254a08(2)`, `menu_save`) is up; its end brings the list back or the loaded
    /// state's jump.
    Loading,
    /// The dim returns, then the outcome runs.
    Closing,
    /// The outcome's jump runs; the game over ends when the jump task is idle again.
    Jumping,
}

/// A game over in progress.
#[derive(Clone, Debug)]
pub struct Over {
    pub kind: u8,
    pub phase: Phase,
    pub waited: f32,
    pub flow: Flow,
    pub chosen: Option<Outcome>,
    /// The trigger jumps of a manager that is gone (`GameOverRequest`); with a manager the manager's are read.
    pub exits: Option<GameOverExits>,
    pub frames: u32,
    /// `FUN_001ad378`'s sound and music stop are still to run.
    pub fresh: bool,
    /// The outcome's jump task has left `Idle`.
    pub started: bool,
    /// The Load screen has been requested from `menu_save` (phase `Loading`).
    pub load_asked: bool,
    /// The alternate list (record 10, `mickey.rs`) is the one shown: `FUN_001ad520` set the byte `+0xc`.
    pub alternate: bool,
}

impl Over {
    pub fn new(kind: u8, exits: Option<GameOverExits>) -> Over {
        Over { kind, phase: Phase::Waiting, waited: 0.0, flow: Flow::default(), chosen: None, exits, frames: 0, fresh: true, started: false, load_asked: false, alternate: false }
    }

    /// The Load screen ended without a load: the list is back (the original's kind 2 closes and the list's page shows again
    /// [inferred: the wrapper `FUN_00254548` was not traced further]).
    pub fn back_to_list(&mut self) {
        self.phase = Phase::List;
        self.load_asked = false;
        self.flow.stage = list::Stage::List;
        self.flow.chosen = None;
    }

    /// The wait is over: the list opens (`FUN_0024f330`).
    pub fn open(&mut self, b: &Block) {
        self.flow = Flow::open(b);
        self.phase = Phase::List;
        self.frames = 0;
    }

    /// A press on the list. The code 1 row ("Load Game") leaves for the Load screen (`FUN_00254a08(2)`), the list waiting.
    pub fn press(&mut self, p: Press) -> Action {
        let a = self.flow.press(p);
        if let Action::Chosen(code) = a {
            if self.alternate {
                // The callback `0x1adbb8`: code 8 opens the normal list, code 9 sets step 7 (Mickey).
                match code {
                    mickey::CODE_GIVE_UP => {
                        self.alternate = false;
                        self.phase = Phase::Waiting;
                        self.waited = OPEN_AFTER;
                        self.flow.chosen = None;
                    }
                    mickey::CODE_RESCUE => {
                        self.chosen = Some(Outcome::Mickey);
                        self.phase = Phase::Closing;
                    }
                    _ => {
                        self.flow.stage = list::Stage::List;
                        self.flow.chosen = None;
                        return Action::None;
                    }
                }
                return a;
            }
            match code {
                CODE_CONTINUE => self.chosen = Some(Outcome::Continue),
                CODE_RETRY if self.kind == 3 => self.chosen = Some(Outcome::Retry),
                CODE_QUIT if self.kind == 3 => self.chosen = Some(Outcome::Quit),
                CODE_LOAD => {
                    self.phase = Phase::Loading;
                    self.load_asked = false;
                    return a;
                }
                // Any other code is not on these rows. Undo the close the list started.
                _ => {
                    self.flow.stage = list::Stage::List;
                    self.flow.chosen = None;
                    return Action::None;
                }
            }
            self.phase = Phase::Closing;
        } else if a == Action::Cancelled {
            self.phase = Phase::Closing;
        }
        a
    }
}

/// `FUN_001cafa0` / `FUN_001cb3b8`: the area record and Sora's HP / MP at the last area start that allowed the snapshot.
/// The original copies the whole save body, whose party record holds the values `FUN_0019f920` rebuilds the status from
/// (current HP = the smaller of the record's bytes +4 and +5, MP the smaller of +6 and +7). "Continue" does not give these back:
/// the party refresher `FUN_001abfe8(8)` that ends `FUN_001cb208` sets the maximum (`refreshed`, WP-GO2); the two numbers are kept
/// for the log line of the area start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub dest: Dest,
    pub hp: i32,
    pub mp: i32,
}

/// `FUN_001cb358`. `manager_bit12` is `None` without a manager, else the manager's header word +4 bit 12
/// (`FUN_001b9fb0`, `Header::flags & 0x1000`): with the bit the snapshot is allowed whatever the map program; a manager
/// without it forbids it; without a manager the map program (`DAT_0033e830 + 3`) must be below 0x33. The original also
/// requires `DAT_0035e3a0` to be 0 on the program branch; nothing in the app writes it, so it is taken as 0 [inferred].
pub fn snapshot_allowed(map_program: u8, manager_bit12: Option<bool>) -> bool {
    manager_bit12.unwrap_or(map_program < SNAPSHOT_PROGRAM_LIMIT)
}

/// What the party refresher gives a member back: full HP and full MP. `FUN_001abfe8(8)` runs `FUN_001ac130` on each of the
/// 15 members' status objects after `FUN_001cb208` has copied the save body back (and `FUN_0019fdc0` has rebuilt Sora's status
/// from his record): `FUN_001a0240(status, 0)` ends a running MP charge (`FUN_001a05d0` tests the total +0x1c0, `FUN_001a05a8`
/// clears both times) and adds the maximum to MP (`FUN_001a0210` -> `FUN_001a0180`: current + maximum, clamped to 0..maximum),
/// then `FUN_001a0100(status, status +4, 0)` adds the maximum HP to the current HP the same way (clamped to the minimum +8 and the
/// maximum +4). The result is the maximum of both, whatever the record held, and `FUN_0019ffb8` writes it back into the record
/// (`FUN_001a0030`: bytes +4..+7). The port floors the HP at 1 [guard against a loop of game overs at 0 maximum].
pub fn refreshed(max_hp: i32, max_mp: i32) -> (i32, i32) {
    (max_hp.max(1), max_mp.max(0))
}

/// The destination of "Continue": the snapshot's area record, or (no snapshot yet: all bytes 0xff in `DAT_01d69890`) the
/// current record, which `FUN_00104bd0` keeps for every 0xff byte.
pub fn continue_dest(snap: Option<Snapshot>) -> Dest {
    snap.map_or(Dest { world: crate::area_jump::KEEP, area: crate::area_jump::KEEP, entrance: crate::area_jump::KEEP, programs: [crate::area_jump::KEEP; 3] }, |s| s.dest)
}

/// `FUN_001cafa0` / `FUN_001cb118`: the copy of the save body (`0x33e860..0x349d40`, the words `0x35d278..0x35daa0` and
/// `0x35daa0..0x35e2c8`) as a slot file. Every `Progress` field except the app-only ones is one of those bytes (the party
/// block, the wallet `+0x1600`, the option word `+0x31b0`, the NEW tags' seen words `+0x31b4`, the shops' bits, the moogle record,
/// the journal's seen set, the counters, the story flags, the area program table and the lock halfwords), so the file the
/// Save menu writes is the port's body. `seen` is the NEW tags' live words (the Save menu stores them the same way).
pub fn capture_body(p: &Progress, seen: &[u32]) -> String {
    let mut stored = p.clone();
    stored.set_new_seen(seen);
    stored.slot_json(&SlotInfo::default())
}

/// `FUN_001cb208`'s copy back: the body applied to a copy of the live state, so nothing of `live` changes unless the caller
/// replaces it on `Ok` (`apply_json` puts the shared story back on a failure, LD1b).
pub fn restore_body(live: &Progress, body: &str) -> Result<Progress, String> {
    let mut next = live.clone();
    next.apply_json(body)?;
    Ok(next)
}

/// The jump record of a "Retry" / "Quit" row.
pub fn exit_jump(exits: &GameOverExits, o: Outcome) -> Option<[u16; 6]> {
    match o {
        Outcome::Retry => exits.retry,
        Outcome::Quit => exits.quit,
        Outcome::Continue | Outcome::Load(_) | Outcome::Mickey => None,
    }
}

/// `save.2ld` sequence 0 groups (the Save menu's pointer list, as `menu_struggle` draws it).
mod g {
    pub const ROW_SELECTED: usize = 77;
    pub const ROW_IDLE: usize = 79;
    pub const HAND: usize = 12;
    pub const ROW_LABEL: [f32; 2] = [70.0, 80.0];
    pub const ROW_PITCH: f32 = 26.0;
    pub const HAND_AT: [f32; 2] = [62.0, 80.0];
    pub const ROW_IDLE_COLOR: [u8; 4] = [0x9f, 0x9f, 0xaf, 0x80];
    pub const ROW_SELECTED_COLOR: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
}

const ROWS: usize = 4;

/// The game over's resource.
#[derive(Resource, Default)]
pub struct GameOver {
    pub over: Option<Over>,
    pub snapshot: Option<Snapshot>,
    /// The save body taken with the snapshot (`capture_body`); "Continue" puts it back.
    pub body: Option<String>,
    last_hp: Option<i32>,
    seen_area: Option<String>,
    table: Vec<Block>,
    tried: bool,
    loaded: bool,
    tex: usize,
    seq: usize,
    rows: Vec<(usize, usize)>,
    hand: usize,
    /// `KH2_GAMEOVER_KO` has fired.
    aid_done: bool,
    /// `KH2_GAMEOVER_GAIN` has fired.
    gain_done: bool,
    /// The outcome's jump has started; the HP it restores.
    restore_hp: Option<i32>,
    /// The shared generator's value for `FUN_001ad520`'s roll (`mickey::SEED_START` until the first roll).
    seed: Option<u32>,
    /// The scene's model panel (`model`).
    stage: model::Stage,
}

impl GameOver {
    pub fn is_open(&self) -> bool {
        self.over.as_ref().is_some_and(|o| matches!(o.phase, Phase::List | Phase::Closing))
    }
}

pub struct GameOverPlugin;

impl Plugin for GameOverPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameOver>()
            .add_systems(FixedUpdate, detect.after(Sim::SoraHurt).before(Sim::Sounds))
            .add_systems(PreUpdate, input.after(crate::menu::input).after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, (track_area, drive, model::show, model::animate, model::layer_and_tint).chain().before(crate::schedule::Frame::Popups));
    }
}

/// `KH2_GAMEOVER_KO=<seconds>` (a harness aid): once, that many seconds after the start, Sora is knocked out (HP 0,
/// `State::Down`) so the game over can be seen without waiting for an enemy.
fn ko_aid_secs() -> Option<f32> {
    static SPEC: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *SPEC.get_or_init(|| std::env::var("KH2_GAMEOVER_KO").ok()?.trim().parse().ok())
}

/// `KH2_GAMEOVER_GAIN=<seconds>` (a harness aid): once, that many seconds after the start, the wallet gains 777 munny and the
/// experience 100, so that a later "Continue" can be seen to take them back.
fn gain_aid_secs() -> Option<f32> {
    static SPEC: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    *SPEC.get_or_init(|| std::env::var("KH2_GAMEOVER_GAIN").ok()?.trim().parse().ok())
}

fn tinker_ready(sm: Option<&SummonState>, data: Option<&SummonData>) -> bool {
    // `summon::tinker_ready` (FUN_001c69b0): the summon state is not 0, byte 5 is Peter Pan's `sumn` row and byte 7 is set.
    let (Some(sm), Some(data)) = (sm, data) else { return false };
    sm.state != 0 && sm.index == data.index_of(crate::summon::cmd::PETER_PAN).unwrap_or(0) && sm.extra != 0
}

/// The player's knock-out (`FUN_001cec00`): the edge of Sora's HP to 0 decides the route once.
#[allow(clippy::too_many_arguments)]
fn detect(
    mut go: ResMut<GameOver>,
    mission: Option<Res<Mission>>,
    sm: Option<Res<SummonState>>,
    sdata: Option<Res<SummonData>>,
    drive: Option<Res<DriveState>>,
    real: Res<Time<Real>>,
    mut sora: Query<&mut Sora>,
    party: Query<&PartyMember>,
    mut progress: Option<ResMut<Progress>>,
    disc: Option<Res<Disc>>,
) {
    if let Some(at) = gain_aid_secs()
        && !go.gain_done
        && real.elapsed_secs() >= at
        && let Some(p) = progress.as_deref_mut()
    {
        go.gain_done = true;
        let json = format!("{{\"munny\": {}, \"experience\": {}}}", p.wallet() + 777, p.experience() + 100);
        let r = p.apply_json(&json);
        info!("game over: KH2_GAMEOVER_GAIN: munny {}, experience {} ({r:?})", p.wallet(), p.experience());
    }
    let Ok(mut s) = sora.single_mut() else { return };
    if let Some(at) = ko_aid_secs()
        && !go.aid_done
        && real.elapsed_secs() >= at
        && s.hp > 0
    {
        go.aid_done = true;
        info!("game over: KH2_GAMEOVER_KO: Sora is knocked out (HP {} -> 0)", s.hp);
        s.hp = 0;
        s.enter(State::Down, None);
    }
    let was = go.last_hp.replace(s.hp);
    if !ko_edge(was, s.hp) || go.over.is_some() {
        return;
    }
    let view = mission.as_deref().and_then(Mission::ko_view);
    let input = KoInput {
        ko_bit: view.is_some_and(|v| v.loses),
        tinker_ready: tinker_ready(sm.as_deref(), sdata.as_deref()),
        menu_manager: view.is_some_and(|v| v.menu_kind),
        in_form: drive.as_deref().is_some_and(|d| d.in_form && (1..=5).contains(&d.form)),
    };
    let r = route(input);
    info!("game over: Sora knocked out (FUN_001cec00): {r:?} for {input:?}");
    if let Route::GameOver(kind) = r {
        let exits = (kind == 3).then(|| view.map(|v| v.exits)).flatten();
        let mut over = Over::new(kind, exits);
        over.alternate = alternate_offer(&mut go, kind, mission.as_deref(), &party, progress.as_deref(), disc.as_deref());
        go.over = Some(over);
        s.game_over = true;
    }
}

/// `FUN_001ad378` -> `FUN_001ad520`: does this game over show the alternate list (Mickey's offer)? See `mickey.rs`.
fn alternate_offer(go: &mut GameOver, kind: u8, mission: Option<&Mission>, party: &Query<&PartyMember>, progress: Option<&Progress>, disc: Option<&Disc>) -> bool {
    let bit9 = mission.and_then(|m| m.m.as_deref()).is_some_and(|a| a.header.flags & mickey::HEADER_BIT != 0);
    if mickey::forced() {
        let yes = kind == 0;
        info!("game over: Mickey offer: KH2_MICKEY_OFFER forces the alternate list (record {}): {yes}", mickey::RECORD);
        return yes;
    }
    if kind != 0 || !bit9 {
        return false;
    }
    if !mickey::RESCUE_PORTED {
        info!("game over: Mickey offer: header bit 9 is set (the original offers the rescue here) but the rescue is not ported (mickey::RESCUE_PORTED); the normal list shows");
        return false;
    }
    let Some(chance) = disc.and_then(|d| d.0.lock().ok().and_then(|mut a| kh2_tables::player::load_sstm(&mut a).ok())).map(|s| mickey::Chance::from_sstm(&s)) else {
        info!("game over: Mickey offer: sstm is not readable; no offer");
        return false;
    };
    let mut ally_up = [false; 2];
    for m in party.iter() {
        if let Some(up) = ally_up.get_mut(usize::from(m.slot)) {
            *up = !m.ko;
        }
    }
    let saves = progress.map_or(0, Progress::mickey_save_count);
    let offer = mickey::Offer { kind, manager_bit9: bit9, ally_up, chance, saves };
    let (yes, next) = mickey::offered(offer, go.seed.unwrap_or(mickey::SEED_START));
    go.seed = Some(next);
    info!("game over: Mickey offer (FUN_001ad520): {offer:?}, chance {:.3}, roll {:.3}: {yes}", chance.after(saves), mickey::roll(next));
    yes
}

/// Sora's HP fell to 0 (`FUN_00186238` -> `FUN_00186530`): the knock-out method runs once per fall.
pub fn ko_edge(was: Option<i32>, hp: i32) -> bool {
    hp <= 0 && was.is_some_and(|h| h > 0)
}

/// `FUN_001047a8`: the area start takes the snapshot when `FUN_001cb358` allows.
fn track_area(
    mut go: ResMut<GameOver>,
    area: Res<CurrentArea>,
    rec: Res<AreaRecord>,
    mission: Option<Res<Mission>>,
    sora: Query<&Sora>,
    progress: Option<Res<Progress>>,
    tags: Option<Res<NewTags>>,
) {
    if area.loaded == go.seen_area {
        return;
    }
    go.seen_area = area.loaded.clone();
    let Some(name) = area.loaded.as_deref() else { return };
    let manager = mission.as_deref().and_then(|m| m.m.as_deref()).map(|a| a.header.flags & 0x1000 != 0);
    if !snapshot_allowed(rec.programs[0], manager) {
        info!("game over: area {name}: no snapshot (FUN_001cb358: map program {:#x}, manager header bit 12 {manager:?})", rec.programs[0]);
        return;
    }
    let (hp, mp) = sora.single().map_or((1, 0), |s| (s.hp.max(1), s.mp));
    let dest = Dest { world: rec.world, area: rec.area, entrance: rec.entrance, programs: rec.programs };
    info!("game over: area {name}: snapshot (FUN_001cafa0): world {} area {} entrance {} programs {:x?}, HP {hp}", dest.world, dest.area, dest.entrance, dest.programs);
    go.snapshot = Some(Snapshot { dest, hp, mp });
    if let Some(p) = progress.as_deref() {
        let body = capture_body(p, &tags.as_deref().map(NewTags::seen_words).unwrap_or_default());
        info!("game over: area {name}: save body copied (FUN_001cafa0): {} bytes of state", body.len());
        go.body = Some(body);
    }
}

/// "Continue" (`FUN_00104d58` -> `FUN_001cb208`): the snapshot's body replaces the state, so what was earned since the area
/// start is gone. A body that does not apply changes nothing (the jump goes on with the live state).
fn restore_snapshot(go: &GameOver, progress: Option<&mut Progress>, tags: Option<&mut NewTags>) {
    let (Some(body), Some(p)) = (go.body.as_deref(), progress) else {
        info!("game over: Continue: no save body was copied; the state stays");
        return;
    };
    match restore_body(p, body) {
        Ok(next) => {
            *p = next;
            if let Some(t) = tags {
                t.set_seen_words(p.new_seen());
            }
            info!("game over: Continue: the save body is back (FUN_001cb208): munny {}, experience {}", p.wallet(), p.experience());
        }
        Err(why) => warn!("game over: Continue: the save body does not apply ({why}); the state stays"),
    }
}

/// `FUN_001abfe8(8)` (called by `FUN_001cb208` after the body is back): every member's MP charge ends, HP and MP go to their
/// maximum (`refreshed`). Sora's maximum MP is the data's (`GameData::sora_max_mp`); a member's are his own. The members' KO flag
/// (the actor's dead bit) is not touched: the jump that follows builds them again. `FUN_001b81f8`, the other refresher, clears
/// the 0x1c bytes at `0x1d5d7c0` (`FUN_001b8218`), the Struggle's score block: the app has no such block to clear (docs/menus.md 26.7).
fn refresh_party(
    sora: &mut Query<&mut Sora>,
    sora_max_mp: Option<i32>,
    mut charge: Option<&mut crate::magic_rules::Charge>,
    members: &mut Query<&mut PartyMember>,
) {
    if let Ok(mut s) = sora.single_mut() {
        let (hp, mp) = refreshed(s.stats.max_hp, sora_max_mp.unwrap_or(s.mp));
        s.hp = hp;
        let ended = crate::magic::refill_mp(charge.as_deref_mut(), &mut s.mp, mp);
        info!("game over: party refresher (FUN_001abfe8(8)): Sora HP {hp}, MP {mp}, MP charge ended {ended}");
    }
    for mut m in members.iter_mut() {
        let (hp, mp) = refreshed(m.hp_max, m.mp_max);
        m.hp = hp;
        let m = &mut *m;
        crate::magic::refill_mp(Some(&mut m.charge), &mut m.mp, mp);
        info!("game over: party refresher (FUN_001abfe8(8)): member {} HP {hp}, MP {mp}", m.slot);
    }
}

fn load(go: &mut GameOver, disc: &Disc, scene: &mut MenuScene, images: &mut Assets<Image>) {
    go.tried = true;
    let Some(bar) = disc.read(list::FILE) else {
        warn!("game over: {} is not readable; the game over menu is off", list::FILE);
        return;
    };
    let Ok(entries) = kh2_bar::parse(&bar) else { return };
    let Some(e) = entries.iter().find(|e| e.tag == list::TAG) else {
        warn!("game over: {} has no {} entry; the game over menu is off", list::FILE, list::TAG);
        return;
    };
    go.table = list::parse_table(e.data);
    let Some(two) = disc.read("menu/us/save.2ld").and_then(|b| layd::parse_2ld(&b)) else {
        warn!("game over: menu/us/save.2ld is not readable; the game over menu is off");
        return;
    };
    let (Some(img), Some(seq)) = (two.images.first(), two.layd.seqs.first()) else { return };
    if scene.text.is_none() {
        warn!("game over: no menu text; the game over menu is off");
        return;
    }
    go.tex = scene.texs.len();
    scene.texs.push(menu_gfx::Tex::new(img, images));
    go.seq = scene.seqs.len();
    scene.seqs.push(seq.clone());
    let new = |scene: &mut MenuScene, s: Option<usize>, t: usize| {
        let mut node = Node::new(s);
        node.tex = t;
        node.visible = false;
        scene.add(node)
    };
    if !go.stage.tried {
        go.stage.read(disc);
        go.stage.setup(scene, images);
    }
    go.rows = (0..ROWS).map(|_| (new(scene, Some(go.seq), go.tex), new(scene, None, go.tex))).collect();
    go.hand = new(scene, Some(go.seq), go.tex);
    go.loaded = true;
    info!("game over: {} {}: {} rows", list::FILE, list::TAG, go.table.len());
}

fn hide_all(go: &GameOver, scene: &mut MenuScene) {
    if !go.loaded {
        return;
    }
    for &(p, t) in &go.rows {
        for i in [p, t] {
            scene.nodes[i].visible = false;
            scene.nodes[i].label = None;
        }
    }
    scene.nodes[go.hand].visible = false;
}

fn show(go: &GameOver, over: &Over, scene: &mut MenuScene) {
    for (k, &(plate, text)) in go.rows.iter().enumerate() {
        let Some(&(_, msg)) = over.flow.rows.get(k) else {
            scene.nodes[plate].visible = false;
            scene.nodes[text].visible = false;
            continue;
        };
        let selected = k == over.flow.row;
        let dy = g::ROW_PITCH * k as f32;
        scene.nodes[plate].visible = true;
        scene.nodes[plate].pos = [0.0, dy];
        let want = if selected { g::ROW_SELECTED } else { g::ROW_IDLE };
        if scene.nodes[plate].player.group != Some(want) {
            scene.set_group(plate, want);
        }
        let color = if selected { g::ROW_SELECTED_COLOR } else { g::ROW_IDLE_COLOR };
        // Bit 15 of a stored id is ignored by the text lookup (`FUN_00190c40`).
        let label = scene.text.as_ref().and_then(|t| {
            t.message(u32::from(msg & 0x7fff), Style { font: 1, size: Some(15.0), color, ..Style::default() }, [g::ROW_LABEL[0], g::ROW_LABEL[1] + dy])
        });
        scene.nodes[text].label = label;
        scene.nodes[text].visible = true;
    }
    scene.nodes[go.hand].visible = true;
    scene.nodes[go.hand].pos = [g::HAND_AT[0], g::HAND_AT[1] + g::ROW_PITCH * over.flow.row as f32];
    if scene.nodes[go.hand].player.group != Some(g::HAND) {
        scene.set_group(go.hand, g::HAND);
    }
}

/// The list shows again after the Load screen was left.
fn reshow(go: &GameOver, over: &Over, scene: &mut MenuScene) {
    scene.hidden = false;
    scene.dim = dim_alpha(DIM_BRIGHTNESS);
    hide_all(go, scene);
    if go.loaded {
        show(go, over, scene);
    }
}

/// The keys of the list (the same as the Struggle's result menu).
#[allow(clippy::too_many_arguments)]
fn input(
    mut go: ResMut<GameOver>,
    mut scene: ResMut<MenuScene>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    mut pause: ResMut<Pause>,
) {
    if go.over.as_ref().is_some_and(|o| o.phase == Phase::Loading) {
        // The Load screen owns the pad (`menu_save`); the field stays stopped.
        pause.menu = true;
        return;
    }
    if !go.is_open() {
        return;
    }
    // A menu is a scene of its own: the field stops while it is up (docs/menus.md 1.1).
    pause.menu = true;
    let pressed = |k: KeyCode| keys.just_pressed(k);
    let press = if pressed(KeyCode::ArrowUp) || pressed(KeyCode::KeyW) {
        Some(Press::Up)
    } else if pressed(KeyCode::ArrowDown) || pressed(KeyCode::KeyS) {
        Some(Press::Down)
    } else if pressed(KeyCode::KeyJ) || pressed(KeyCode::Enter) || pressed(KeyCode::NumpadEnter) {
        Some(Press::Confirm)
    } else if pressed(KeyCode::KeyK) || pressed(KeyCode::Escape) || pressed(KeyCode::Backspace) {
        Some(Press::Cancel)
    } else {
        None
    };
    keys.reset_all();
    let Some(p) = press else { return };
    let Some(mut over) = go.over.take() else { return };
    let row_code = over.flow.rows.get(over.flow.row).map(|r| r.0);
    match over.press(p) {
        Action::None => {
            if p == Press::Cancel {
                info!("game over: cancel ignored (+0x25 bit 0: list flags 0x60 cleared)");
            } else if p == Press::Confirm {
                info!("game over: row code {row_code:?} is not a row of this record; the list stays");
                menu_sound::play(&mut se, menu_sound::REFUSED);
            }
        }
        Action::Cursor => {
            menu_sound::play(&mut se, menu_sound::CURSOR);
            show(&go, &over, &mut scene);
        }
        Action::Chosen(code) => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            info!("game over: row chosen, code {code} (FUN_0026fda0)");
            over.frames = 0;
            hide_all(&go, &mut scene);
        }
        Action::Cancelled => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            over.frames = 0;
            hide_all(&go, &mut scene);
        }
    }
    go.over = Some(over);
}

#[allow(clippy::too_many_arguments)]
fn drive(
    mut go: ResMut<GameOver>,
    mut scene: ResMut<MenuScene>,
    real: Res<Time<Real>>,
    mut mission: ResMut<Mission>,
    mut task: ResMut<JumpTask>,
    rec: Res<AreaRecord>,
    disc: Res<Disc>,
    mut images: ResMut<Assets<Image>>,
    mut se: MessageWriter<PlaySe>,
    mut music: Option<ResMut<AreaMusic>>,
    audio: Option<Res<Audio>>,
    mut sora: Query<&mut Sora>,
    mut save: ResMut<SaveMenu>,
    mut pause: ResMut<Pause>,
    // Bevy takes 16 system parameters at most: the state the restore and the refreshers touch comes as one tuple.
    (mut progress, mut tags, data, mut magic, mut members): (
        Option<ResMut<Progress>>,
        Option<ResMut<NewTags>>,
        Option<Res<crate::data::GameData>>,
        Option<ResMut<crate::magic::MagicState>>,
        Query<&mut PartyMember>,
    ),
) {
    // A manager that ended by itself (`FUN_001b9ce0`).
    if let Some(r) = mission.game_over_request.take()
        && go.over.is_none()
    {
        info!("game over: the manager ended with the game-over path: type {} (FUN_001b9ce0)", r.kind);
        go.over = Some(Over::new(r.kind, Some(r.exits)));
    }
    let Some(mut over) = go.over.take() else { return };
    if over.fresh {
        over.fresh = false;
        start(over.kind, &mut se, music.as_deref_mut(), audio.as_deref());
    }
    let mut keep = true;
    match over.phase {
        Phase::Waiting => {
            over.waited += real.delta_secs();
            if over.waited >= OPEN_AFTER {
                if !go.tried {
                    load(&mut go, &disc, &mut scene, &mut images);
                }
                let key = if over.alternate { mickey::RECORD } else { record_of(over.kind) };
                if over.alternate && go.loaded && list::find(&go.table, key).is_none() {
                    // The disc has no record 10: the normal list shows [port choice].
                    warn!("game over: the alternate record {key:#x} is not in the table; the normal list shows");
                    over.alternate = false;
                    go.over = Some(over);
                    return;
                }
                match list::find(&go.table, key).cloned() {
                    Some(b) if go.loaded => {
                        over.open(&b);
                        scene.hidden = false;
                        scene.dim = dim_alpha(DIM_BRIGHTNESS);
                        hide_all(&go, &mut scene);
                        show(&go, &over, &mut scene);
                        menu_sound::play(&mut se, menu_sound::OPEN);
                        info!("game over: type {} opens record {key:#x} (FUN_0024f330 -> menu kind 7): rows {:x?}, cursor {}", over.kind, over.flow.rows, over.flow.row);
                    }
                    _ => {
                        // Without the disc's rows nothing can be shown: Sora stands up as before [port choice].
                        warn!("game over: record {key:#x} cannot open (loaded {}); Sora stands up in place", go.loaded);
                        keep = false;
                        finish(&mut go, &mut sora, false);
                    }
                }
            }
        }
        Phase::List => {
            scene.advance(real.delta_secs());
        }
        Phase::Loading => {
            pause.menu = true;
            if !over.load_asked {
                over.load_asked = true;
                if save.request_load() {
                    info!("game over: row code 1 (Load Game): the Load screen is requested (FUN_00254a08(2), menu kind 2)");
                } else {
                    info!("game over: the Load screen cannot open (no save.2ld or a menu is up); the list stays");
                    menu_sound::play(&mut se, menu_sound::REFUSED);
                    over.back_to_list();
                    reshow(&go, &over, &mut scene);
                }
            } else if let Some(out) = save.take_outcome() {
                match out {
                    LoadOutcome::Loaded(l) => {
                        // `FUN_00259e88` state 0x34 ends with `FUN_001cb118` (the snapshot of the loaded body, so a later
                        // Continue returns to this state) and `FUN_0025a158` leaves with mode 2 (`FUN_00103b38`).
                        let (hp, mp) = l.hp_mp.unwrap_or((i32::MAX, sora.single().map_or(0, |s| s.mp)));
                        go.snapshot = Some(Snapshot { dest: l.dest, hp, mp });
                        // The Load screen has put the loaded state in `Progress` and the NEW tags already.
                        if let Some(p) = progress.as_deref() {
                            go.body = Some(capture_body(p, &tags.as_deref().map(NewTags::seen_words).unwrap_or_default()));
                        }
                        over.chosen = Some(Outcome::Load(l));
                        scene.hidden = true;
                        scene.dim = 0.0;
                        hide_all(&go, &mut scene);
                        if run_outcome(&mut go, &over, &mut mission, &mut task, &rec, &disc, &mut sora, music.as_deref_mut(), audio.as_deref()) {
                            over.phase = Phase::Jumping;
                            over.started = false;
                        } else {
                            keep = false;
                        }
                    }
                    LoadOutcome::Left => {
                        info!("game over: the Load screen was left; the list is back");
                        over.back_to_list();
                        reshow(&go, &over, &mut scene);
                    }
                }
            }
        }
        Phase::Closing => {
            over.frames += scene.advance(real.delta_secs());
            scene.dim = dim_alpha(close_brightness(over.frames));
            if over.frames >= CLOSE_FRAMES {
                scene.hidden = true;
                scene.dim = 0.0;
                hide_all(&go, &mut scene);
                info!("game over: closed: back to the field (FUN_001038a0)");
                if over.chosen == Some(Outcome::Mickey) {
                    rescue(&mut go, &mut sora, progress.as_deref_mut(), music.as_deref_mut(), audio.as_deref());
                    keep = false;
                } else {
                    if over.chosen == Some(Outcome::Continue) {
                        restore_snapshot(&go, progress.as_deref_mut(), tags.as_deref_mut());
                        refresh_party(&mut sora, data.as_deref().map(|d| d.sora_max_mp), magic.as_deref_mut().map(|m| &mut m.charge), &mut members);
                    }
                    if run_outcome(&mut go, &over, &mut mission, &mut task, &rec, &disc, &mut sora, music.as_deref_mut(), audio.as_deref()) {
                        over.phase = Phase::Jumping;
                        over.started = false;
                    } else {
                        keep = false;
                    }
                }
            }
        }
        Phase::Jumping => {
            // The jump task leaves `Idle` on the tick after it was set; the game over ends when it comes back.
            if *task != JumpTask::Idle {
                over.started = true;
            } else if over.started {
                info!("game over: the jump is over: Sora is back (HP {:?})", go.restore_hp);
                keep = false;
                finish(&mut go, &mut sora, true);
            }
        }
    }
    if keep {
        go.over = Some(over);
    }
    if let Ok(mut s) = sora.single_mut() {
        s.game_over = go.over.is_some();
    }
}

/// "I won't give up!" (callback `0x1adbb8`, step 7 of `FUN_001ad9a0`): the music comes back (`FUN_00137558`) and
/// `FUN_001cdf48` makes Mickey, whose constructor counts the rescue (`FUN_001cda78`). The Mickey object (a second player
/// class), Sora's motion 0xfc, the system effect 0x11 and the retargeting `FUN_0018f188` are not ported, and what heals Sora
/// was not traced [GAP]: he stands up with full HP so the fight can go on, the only value the app has for a revive. The
/// original's value is unknown.
fn rescue(go: &mut GameOver, sora: &mut Query<&mut Sora>, progress: Option<&mut Progress>, music: Option<&mut AreaMusic>, audio: Option<&Audio>) {
    if let Some(p) = progress {
        p.count_mickey_save();
        info!("game over: Mickey's rescue: counted (report row 4 now {})", p.mickey_save_count());
    }
    if let Some(m) = music {
        crate::audio_bgm::revive_restart(m, audio);
    }
    info!("game over: Mickey's rescue: Sora stands up where he fell; the Mickey object is not ported");
    finish(go, sora, true);
}

/// `FUN_001ad378`: the sound and the music stop.
fn start(kind: u8, se: &mut MessageWriter<PlaySe>, music: Option<&mut AreaMusic>, audio: Option<&Audio>) {
    let n = if kind == 1 { SOUND_OVER_TYPE_1 } else { SOUND_OVER };
    info!("game over: type {kind}: system sound {n:#x} (FUN_002c14a8), music stops (FUN_00137520)");
    se.write(PlaySe::system(n));
    if let Some(m) = music {
        crate::audio_bgm::revive_stop(m, audio);
    }
}

/// The game over is over: Sora is not held any more.
fn finish(go: &mut GameOver, sora: &mut Query<&mut Sora>, restored: bool) {
    go.over = None;
    go.restore_hp = None;
    if let Ok(mut s) = sora.single_mut() {
        s.game_over = false;
        if restored && s.hp <= 0 {
            s.hp = s.stats.max_hp;
        }
    }
}

/// The closed list's choice: "Continue" jumps to the snapshot's area, "Retry" / "Quit" take the trigger jumps.
#[allow(clippy::too_many_arguments)]
fn run_outcome(
    go: &mut GameOver,
    over: &Over,
    mission: &mut Mission,
    task: &mut JumpTask,
    rec: &AreaRecord,
    disc: &Disc,
    sora: &mut Query<&mut Sora>,
    music: Option<&mut AreaMusic>,
    audio: Option<&Audio>,
) -> bool {
    let Some(outcome) = over.chosen else {
        finish(go, sora, false);
        return false;
    };
    let max_hp = sora.single().map_or(1, |s| s.stats.max_hp);
    let (dest, kind) = match outcome {
        Outcome::Continue => {
            // The callback `LAB_001adc70`: `FUN_00104e18(FUN_001cb3b8(), 0x80000001, 1, 1, 0)`.
            // `FUN_001cb208` ended with the party refresher (`refresh_party`, run by `drive` when the list closed): Sora is at his maximum.
            go.restore_hp = sora.single().ok().map(|s| s.hp);
            (continue_dest(go.snapshot), CONTINUE_FADE_KIND)
        }
        Outcome::Load(l) => {
            // `FUN_00103b38`, mode 2: `FUN_00104e18(record, 1, 0, 1, 0)` enters the area record of the loaded body. Sora's HP and
            // MP come from its party record (HP = min(+4, +5), MP = min(+6, +7)).
            let (hp, mp) = l.hp_mp.map_or((max_hp, sora.single().map_or(0, |s| s.mp)), |(h, m)| (h.clamp(1, max_hp.max(1)), m.max(0)));
            go.restore_hp = Some(hp);
            if let Ok(mut s) = sora.single_mut() {
                s.hp = hp;
                s.mp = mp;
            }
            (l.dest, CONTINUE_FADE_KIND)
        }
        Outcome::Mickey => {
            // Handled by `rescue` before this runs.
            finish(go, sora, true);
            return false;
        }
        Outcome::Retry | Outcome::Quit => {
            let code = if outcome == Outcome::Retry { CODE_RETRY } else { CODE_QUIT };
            // With the manager (a knock-out): `Mission::menu_exit` ends it and reads its trigger; without it the jumps the
            // request carried.
            let jump = if mission.m.is_some() {
                mission.menu_exit(code).and_then(|e| e.jump)
            } else {
                over.exits.and_then(|e| exit_jump(&e, outcome))
            };
            go.restore_hp = Some(max_hp);
            if let Ok(mut s) = sora.single_mut() {
                s.hp = max_hp;
            }
            let Some(j) = jump else {
                info!("game over: {outcome:?}: no trigger jump; Sora stands up in place");
                finish(go, sora, true);
                return false;
            };
            match exit_dest(j, (rec.world, rec.area, rec.entrance), |w, k| disc_localset(disc, w, k)) {
                Ok(d) => d,
                Err(why) => {
                    info!("game over: {outcome:?}: trigger jump {j:?}: {why}; not taken, Sora stands up in place");
                    finish(go, sora, true);
                    return false;
                }
            }
        }
    };
    if let Some(m) = music {
        // `FUN_00137558`: the held music state back, so the arrival's music runs.
        crate::audio_bgm::revive_restart(m, audio);
    }
    info!("game over: {outcome:?}: jump to world {} area {} entrance {} programs {} (fade kind {kind})", dest.world, dest.area, dest.entrance, dest.programs_text());
    *task = JumpTask::FadeOut { dest, kind, t: 0.0 };
    true
}

/// A trigger jump record `[type, world, area, entrance, key, flags]` resolved as every other trigger jump
/// (`area_jump::resolve_jump`), with the fade kind from the flags.
fn exit_dest(j: [u16; 6], cur: (u8, u8, u8), localset: impl Fn(u8, u16) -> Option<u8>) -> Result<(Dest, u32), String> {
    let jump = kh2_ard::run::Jump { kind: j[0], world: j[1], area: j[2], entrance: j[3], key: j[4], flags: j[5] };
    resolve_jump(&jump, cur, None, localset).map(|d| (d, u32::from(j[5] & 0x3fff)))
}

/// The game over scene (docs/menus.md 26.6): the model panel that the list shows.
///
/// The original (`SLUS_210.05`): `FUN_001ade58` loads `obj/GAMEOVER.mset` as resource 5000 (`FUN_0016fac8`) and `FUN_001adcf8`
/// (task step 5) stores the motion `FUN_001add40` picks in the object's word `+0x34`, the second list header word. The first
/// header word is the player's object id (`FUN_00169958(player) | 0x20000000`). When the list builds (`FUN_0026f948`) it takes
/// the camp's model object (`FUN_00295bc0`, the same object the Party page uses), puts the player's model in slot 0 at x 0, y 0
/// (`vtable +0x18`, no motion of its own), gives it that motion (`FUN_0023b0f0`), the translation z `DAT_00381b28` (-100,
/// the camp's), no rotation (`FUN_0023b3a8` / `FUN_0023b3c8`) and the list window as its alpha source (`FUN_0023b4b8`). The
/// per-frame step `FUN_0023a920` loops the motion. The motion is the third entry named `%03d0` (format string `0x37b878`) of the
/// BAR at index 0xfc (type 0) or 0xfd (types 1 and 3) of the file, with `%03d` the player's objentry byte `+7`
/// (`WeaponJoint`; Sora's row is `P_EX100`, 1): `FUN_001684c0` finds the index, `FUN_00168510` the name and `FUN_001684c0(.., 9, 0)`
/// the motion in the entry. Assets are read from the disc at run time; nothing is stored.
mod model {
    use std::sync::Arc;

    use bevy::asset::RenderAssetUsages;
    use bevy::camera::visibility::RenderLayers;
    use bevy::camera::RenderTarget;
    use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
    use bevy::prelude::*;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

    use super::{GameOver, Phase};
    use crate::assets::Disc;
    use crate::menu::preview::{focal, model_drop, origin_on_screen, PreviewCamera};
    use crate::menu_gfx::{MenuScene, Pic, Tex};
    use crate::skel::Animator;
    use crate::sora::Sora;

    /// The file of resource 5000.
    pub const FILE: &str = "obj/GAMEOVER.mset";

    /// The BAR index `FUN_001add40` asks for: 0xfc for type 0, 0xfd for types 1 and 3 (type 2 never opens the list).
    pub fn set_index(kind: u8) -> Option<usize> {
        match kind {
            0 => Some(0xfc),
            1 | 3 => Some(0xfd),
            _ => None,
        }
    }

    /// The entry name `FUN_00311c08(.., 0x37b878, objentry byte +7)` builds: `%03d0`.
    pub fn entry_name(joint: u8) -> String {
        format!("{joint:03}0")
    }

    /// `FUN_001add40`'s lookup: the bytes of the third entry named `%03d0` of the BAR at index `set` of the file
    /// (`FUN_001684c0(file, -1, set)`, then `FUN_00168510(set data, -1, name, 2)`). `None` when the index or the entry is
    /// missing or empty (the original returns 0 and shows no model).
    pub fn entry_data(file: &[u8], set: usize, joint: u8) -> Option<Vec<u8>> {
        let outer = kh2_bar::parse(file).ok()?;
        let entry = outer.get(set).filter(|e| !e.data.is_empty())?;
        let inner = kh2_bar::parse(entry.data).ok()?;
        let name = entry_name(joint);
        let found = inner.iter().filter(|e| e.tag == name).nth(2).filter(|e| !e.data.is_empty())?;
        Some(found.data.to_vec())
    }

    /// `FUN_001add40`: the motion of player model `joint` in the set at BAR index `set` of the file: the entry of `entry_data`,
    /// then its first type 9 entry (`FUN_001684c0(.., 9, 0)`), with its triggers (type 16).
    pub fn scene_motion(file: &[u8], set: usize, joint: u8) -> Option<kh2_anim::Slot> {
        let data = entry_data(file, set, joint)?;
        let anb = kh2_bar::parse(&data).ok()?;
        let motion = anb.iter().find(|x| x.kind == 9).and_then(|x| kh2_anim::Motion::parse(x.data))?;
        let triggers = anb.iter().find(|x| x.kind == 16).map(|x| kh2_anim::Triggers::parse(x.data)).unwrap_or_default();
        Some(kh2_anim::Slot { tag: entry_name(joint), motion: Some(motion), triggers })
    }

    /// The picture's size: the menu canvas (512 x 448 units) at twice the resolution, as the camp's preview.
    const TARGET: [u32; 2] = [1024, 896];
    /// Render layer of this scene's camera and model (the camp's preview uses 9).
    const LAYER: usize = 11;
    /// Where the private stage stands (Bevy units), far below any area.
    const STAGE: Vec3 = Vec3::new(0.0, -20_000.0, 0.0);
    /// The model colour 0x60 of 0x80 (`FUN_0023b448`), as the camp's preview.
    const TINT: f32 = 0x60 as f32 / 128.0;

    /// What the scene keeps between frames.
    #[derive(Default)]
    pub struct Stage {
        /// The motion at index 0xfc and at 0xfd.
        pub motions: [Option<Arc<kh2_anim::Slot>>; 2],
        pub tried: bool,
        target: Handle<Image>,
        pub node: Option<usize>,
        camera: Option<Entity>,
        root: Option<Entity>,
        /// The kind the model standing now was built for.
        kind: Option<u8>,
    }

    impl Stage {
        /// Reads the file and Sora's `WeaponJoint` (the objentry row of `P_EX100`) from the disc.
        pub fn read(&mut self, disc: &Disc) {
            self.tried = true;
            let joint = disc
                .0
                .lock()
                .ok()
                .and_then(|mut a| kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).ok())
                .and_then(|d| d.rows.iter().find(|r| r.model_name == "P_EX100").map(|r| r.weapon_joint))
                .unwrap_or(1);
            let Some(file) = disc.read(FILE) else {
                warn!("game over: {FILE} is not readable; the list shows no model");
                return;
            };
            for (i, set) in [0xfc, 0xfd].into_iter().enumerate() {
                self.motions[i] = scene_motion(&file, set, joint).map(Arc::new);
                match &self.motions[i] {
                    Some(s) => {
                        let m = s.motion.as_ref();
                        info!(
                            "game over: {FILE} set {set:#x} entry {} holds a motion of frames {:?}..{:?} (FUN_001add40)",
                            entry_name(joint),
                            m.map(|m| m.frame_start),
                            m.map(|m| m.frame_end)
                        );
                    }
                    None => warn!("game over: {FILE} set {set:#x} has no motion for {} (FUN_001add40 returns 0)", entry_name(joint)),
                }
            }
        }

        /// The picture the camera draws into, as a texture of the menu, and the node that shows it.
        pub fn setup(&mut self, scene: &mut MenuScene, images: &mut Assets<Image>) {
            let size = Extent3d { width: TARGET[0], height: TARGET[1], depth_or_array_layers: 1 };
            let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
            image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
            self.target = images.add(image);
            scene.texs.push(Tex { width: TARGET[0], height: TARGET[1], linear: self.target.clone(), nearest: self.target.clone() });
            let tex = scene.texs.len() - 1;
            let y = crate::menu_gfx::Y_BIAS;
            let mut node = crate::menu_gfx::Node::new(None);
            node.visible = false;
            node.pic = Some(Pic { tex, rect: [0.0, -y, 512.0, 448.0 - y], v_end: 1.0 });
            self.node = Some(scene.add(node));
        }
    }

    #[derive(Component)]
    pub struct OverModel {
        body: Entity,
        motion: Arc<kh2_anim::Slot>,
        state: kh2_anim::rig::RigState,
        done: bool,
    }

    /// Stands the model up while the list is up and takes it away after.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        mut commands: Commands,
        mut go: ResMut<GameOver>,
        mut scene: ResMut<MenuScene>,
        sora: Query<&Sora>,
        anims: Query<&Animator>,
        mut meshes: ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        mut images: ResMut<Assets<Image>>,
        mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    ) {
        // The alternate list (record 10) has no second header word (`FUN_001adcf8` runs for the normal list only): no model.
        let over = go.over.as_ref().map(|o| (o.kind, o.phase, o.frames, o.alternate));
        let kind = over.filter(|o| matches!(o.1, Phase::List | Phase::Closing) && !o.3).map(|o| o.0);
        let motion = kind.and_then(set_index).and_then(|s| go.stage.motions[s - 0xfc].clone());
        let want = motion.is_some() && go.stage.node.is_some() && std::env::var_os("KH2_NO_GAMEOVER_SCENE").is_none();
        let st = &mut go.stage;
        if (!want || st.kind != kind) && st.camera.is_some() {
            for e in [st.camera.take(), st.root.take()].into_iter().flatten() {
                commands.entity(e).despawn();
            }
            st.kind = None;
            if let Some(n) = st.node {
                scene.nodes[n].visible = false;
            }
            info!("game over: the model is removed");
        }
        if !want {
            return;
        }
        if st.camera.is_some() {
            // Fades with the close (the alpha of the list window, `FUN_0023b4b8`).
            let f = match over {
                Some((_, Phase::Closing, frames, _)) => 1.0 - (frames as f32 / crate::menu::CLOSE_FRAMES as f32).clamp(0.0, 1.0),
                _ => 1.0,
            };
            if let Some(n) = st.node {
                scene.nodes[n].label_fade = f;
            }
            return;
        }
        let (Some(motion), Some(rig), Some(node)) = (motion, sora.single().ok().and_then(|s| anims.get(s.body).ok()).map(|a| a.rig.clone()), st.node) else {
            return;
        };
        let (fx, fy) = focal();
        let fov = 2.0 * (224.0 / fy).atan();
        let camera = commands
            .spawn((
                Camera3d::default(),
                PreviewCamera,
                Camera { order: -2, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
                RenderTarget::Image(st.target.clone().into()),
                Projection::Perspective(PerspectiveProjection { fov, near: 0.01, far: 100.0, ..default() }),
                Transform::from_translation(STAGE),
                RenderLayers::layer(LAYER),
            ))
            .id();
        // Slot 0 at x 0, y 0: the slot centre is the canvas middle (256, 208); the model's origin lands `model_drop` below it.
        let (_, z, pitch) = origin_on_screen(0, 1, false);
        let at = Vec2::new(256.0, 208.0 + model_drop());
        let depth = z * crate::SCALE;
        let local = Vec3::new((at.x - 256.0) * depth / fy, -(at.y - 208.0) * depth / fy, -depth);
        let scale = Vec3::new(crate::SCALE * fx / fy, crate::SCALE, crate::SCALE);
        let root = commands
            .spawn((Transform { translation: STAGE + local, rotation: Quat::from_rotation_x(pitch), scale }, Visibility::default(), RenderLayers::layer(LAYER)))
            .id();
        let body = crate::skel::spawn_skinned(&mut commands, root, rig, &mut meshes, &mut materials, &mut images, &mut bindposes, 1.0);
        let slot = motion.clone();
        commands.entity(root).insert(OverModel { body, motion, state: Default::default(), done: false });
        commands.queue(move |world: &mut World| {
            if let Some(mut a) = world.get_mut::<Animator>(body) {
                // `play_external` plays a motion that is not in the rig's own set; the preview loops (`FUN_0023a920`).
                a.play_external(slot, 0.0);
                a.looping = true;
            }
        });
        scene.nodes[node].visible = true;
        scene.nodes[node].label_fade = 1.0;
        st.camera = Some(camera);
        st.root = Some(root);
        st.kind = kind;
        info!("game over: the model stands in slot 0 at menu ({:.0}, {:.0}), depth {z:.0} (FUN_0026f948)", at.x, at.y);
    }

    /// One 60 Hz step of the motion while the simulation (and with it `skel::animate`) is stopped.
    pub fn animate(mut q: Query<&mut OverModel>, mut anims: Query<&mut Animator>, mut joints: Query<&mut Transform>, real: Res<Time<Real>>, mut acc: Local<f32>) {
        *acc = (*acc + real.delta_secs()).min(0.25);
        let mut ticks = 0;
        while *acc >= 1.0 / 60.0 {
            *acc -= 1.0 / 60.0;
            ticks += 1;
        }
        for mut m in &mut q {
            let m = &mut *m;
            let Ok(mut a) = anims.get_mut(m.body) else { continue };
            let a = &mut *a;
            for _ in 0..ticks {
                a.tick();
            }
            let rig = a.rig.clone();
            let motion = m.motion.clone();
            let Some(motion) = motion.motion.as_ref() else { continue };
            let mut pose = motion.pose_with(&rig.model.bones, a.frame, &mut m.state);
            if a.root != Default::default() {
                let undo = (Mat4::from_translation(a.root.translation) * Mat4::from_rotation_y(a.root.yaw)).inverse();
                for p in &mut pose {
                    *p = undo * *p;
                }
            }
            for (j, p) in a.joints.iter().zip(&pose) {
                if let Ok(mut t) = joints.get_mut(*j) {
                    *t = Transform::from_matrix(*p);
                }
            }
            a.pose = pose;
        }
    }

    /// Once the model's meshes exist: put them on the scene's layer with the model colour.
    pub fn layer_and_tint(
        mut commands: Commands,
        mut q: Query<&mut OverModel>,
        children: Query<&Children>,
        handles: Query<&MeshMaterial3d<StandardMaterial>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
    ) {
        for mut p in &mut q {
            if p.done {
                continue;
            }
            let Ok(kids) = children.get(p.body) else { continue };
            let mut any = false;
            for k in kids.iter() {
                commands.entity(k).insert(RenderLayers::layer(LAYER));
                let Ok(h) = handles.get(k) else { continue };
                if let Some(mut m) = materials.get_mut(&h.0) {
                    let c = m.base_color.to_srgba();
                    m.base_color = Color::srgba(c.red * TINT, c.green * TINT, c.blue * TINT, c.alpha);
                    any = true;
                }
            }
            commands.entity(p.body).insert(RenderLayers::layer(LAYER));
            p.done = any;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu_struggle::{parse_table, FILE, TAG};

    fn input(f: impl FnOnce(&mut KoInput)) -> KoInput {
        let mut i = KoInput { ko_bit: false, tinker_ready: false, menu_manager: false, in_form: false };
        f(&mut i);
        i
    }

    #[test]
    fn the_knock_out_route_follows_fun_001cec00() {
        assert_eq!(route(input(|_| {})), Route::GameOver(0));
        assert_eq!(route(input(|i| i.menu_manager = true)), Route::GameOver(3));
        // Host 1.317's bit wins over everything: event 0x5a, no game over.
        assert_eq!(route(input(|i| {
            i.ko_bit = true;
            i.tinker_ready = true;
            i.menu_manager = true;
        })), Route::MissionEvent);
        // Tinker Bell's revive wins over the game over (also with a menu manager).
        assert_eq!(route(input(|i| i.tinker_ready = true)), Route::TinkerRevive);
        assert_eq!(route(input(|i| {
            i.tinker_ready = true;
            i.menu_manager = true;
        })), Route::TinkerRevive);
        // A drive form ends by itself; a menu manager still takes the game over (FUN_001b9c00 is tested first).
        assert_eq!(route(input(|i| i.in_form = true)), Route::FormEnds);
        assert_eq!(route(input(|i| {
            i.in_form = true;
            i.menu_manager = true;
        })), Route::GameOver(3));
    }

    #[test]
    fn the_record_and_the_manager_end_follow_the_type() {
        assert_eq!(record_of(0), 1);
        assert_eq!(record_of(1), 1);
        assert_eq!(record_of(3), 0x21);
        // FUN_001b9ce0: mission kinds 2, 0x1e and 0x23 start type 3, every other kind type 1.
        for k in [2u8, 0x1e, 0x23] {
            assert_eq!(crate::struggle::game_over_kind(k), 3);
        }
        for k in [0u8, 1, 3, 0x19, 0x20] {
            assert_eq!(crate::struggle::game_over_kind(k), 1);
        }
    }

    fn block(key: u16, opts: &[(u16, u16)]) -> Block {
        let mut r = vec![0u8; 0x40];
        r[0..2].copy_from_slice(&key.to_le_bytes());
        r[2] = opts.len() as u8;
        for (i, (c, m)) in opts.iter().enumerate() {
            r[4 + 4 * i..6 + 4 * i].copy_from_slice(&c.to_le_bytes());
            r[6 + 4 * i..8 + 4 * i].copy_from_slice(&m.to_le_bytes());
        }
        r[0x25] = 1;
        let mut d = vec![2, 0, 0, 0, 1, 0, 0, 0];
        d.extend(r);
        parse_table(&d).remove(0)
    }

    #[test]
    fn death_to_list_to_choice_continue_closes_and_retry_needs_type_3() {
        let b = block(1, &[(2, 0x8ab0), (1, 0x8aaf)]);
        let mut o = Over::new(0, None);
        assert_eq!(o.phase, Phase::Waiting);
        o.open(&b);
        assert_eq!(o.phase, Phase::List);
        assert_eq!(o.flow.rows, vec![(2, 0x8ab0), (1, 0x8aaf)]);
        // No cancel (+0x25 bit 0): the list cannot be left without a row.
        assert_eq!(o.press(Press::Cancel), Action::None);
        assert_eq!(o.phase, Phase::List);
        // "Load Game" leaves for the Load screen; the list waits.
        assert_eq!(o.press(Press::Down), Action::Cursor);
        assert_eq!(o.press(Press::Confirm), Action::Chosen(CODE_LOAD));
        assert_eq!((o.phase, o.chosen), (Phase::Loading, None));
        // Leaving the Load screen brings the list back, ready for another choice.
        o.back_to_list();
        assert_eq!((o.phase, o.chosen, o.flow.stage), (Phase::List, None, list::Stage::List));
        assert_eq!(o.press(Press::Confirm), Action::Chosen(CODE_LOAD), "the Load row can be chosen again");
        o.back_to_list();
        // "Continue" closes.
        assert_eq!(o.press(Press::Up), Action::Cursor);
        assert_eq!(o.press(Press::Confirm), Action::Chosen(CODE_CONTINUE));
        assert_eq!((o.phase, o.chosen), (Phase::Closing, Some(Outcome::Continue)));

        // Type 3: Quit then Retry rows.
        let b = block(0x21, &[(4, 0x8ab2), (3, 0x8ab1)]);
        let mut o = Over::new(3, Some(GameOverExits { retry: Some([2, 2, 0, 0, 0xb6, 1]), quit: Some([1, 2, 4, 55, 0, 0]) }));
        o.open(&b);
        assert_eq!(o.press(Press::Confirm), Action::Chosen(CODE_QUIT));
        assert_eq!(o.chosen, Some(Outcome::Quit));
        let e = o.exits.unwrap();
        assert_eq!(exit_jump(&e, Outcome::Quit), Some([1, 2, 4, 55, 0, 0]));
        assert_eq!(exit_jump(&e, Outcome::Retry), Some([2, 2, 0, 0, 0xb6, 1]));
        assert_eq!(exit_jump(&e, Outcome::Continue), None);

        // A type 0 list does not take the mission codes.
        let b = block(1, &[(3, 1)]);
        let mut o = Over::new(0, None);
        o.open(&b);
        assert_eq!(o.press(Press::Confirm), Action::None);
        assert_eq!(o.phase, Phase::List);
    }

    /// The alternate list (record 10): code 9 chooses Mickey's rescue and closes, code 8 gives the normal list back, a plain
    /// game over (no alternate) is not changed by either code.
    #[test]
    fn the_alternate_list_routes_code_9_to_mickey_and_code_8_to_the_normal_list() {
        let alt = block(mickey::RECORD, &[(mickey::CODE_RESCUE, 0x8ab5), (mickey::CODE_GIVE_UP, 0x8ab6)]);
        let mut o = Over::new(0, None);
        o.alternate = true;
        o.open(&alt);
        assert_eq!(o.flow.rows, vec![(9, 0x8ab5), (8, 0x8ab6)]);
        assert_eq!(o.press(Press::Confirm), Action::Chosen(mickey::CODE_RESCUE));
        assert_eq!((o.phase, o.chosen), (Phase::Closing, Some(Outcome::Mickey)));

        // "It's all over...": no outcome, the wait is over so the normal record opens at once.
        let mut o = Over::new(0, None);
        o.alternate = true;
        o.open(&alt);
        assert_eq!(o.press(Press::Down), Action::Cursor);
        assert_eq!(o.press(Press::Confirm), Action::Chosen(mickey::CODE_GIVE_UP));
        assert_eq!((o.phase, o.chosen, o.alternate), (Phase::Waiting, None, false));
        assert!(o.waited >= OPEN_AFTER);
        // The normal list then behaves as before: Continue closes with its outcome.
        o.open(&block(1, &[(2, 0x8ab0), (1, 0x8aaf)]));
        assert_eq!(o.press(Press::Confirm), Action::Chosen(CODE_CONTINUE));
        assert_eq!(o.chosen, Some(Outcome::Continue));

        // A row that is not 8 or 9 is not on the alternate record.
        let mut o = Over::new(0, None);
        o.alternate = true;
        o.open(&block(mickey::RECORD, &[(2, 1)]));
        assert_eq!(o.press(Press::Confirm), Action::None);
        assert_eq!((o.phase, o.chosen), (Phase::List, None));

        // Without the alternate, codes 8 and 9 are not game-over rows.
        let mut o = Over::new(0, None);
        o.open(&alt);
        assert_eq!(o.press(Press::Confirm), Action::None);
        assert_eq!((o.phase, o.chosen), (Phase::List, None));
        assert!(!Over::new(0, None).alternate, "a game over starts as the plain one");
    }

    /// `rescue` counts the rescue once, ends the game over and stands Sora up.
    #[test]
    fn the_rescue_counts_once_and_releases_sora() {
        use bevy::ecs::system::RunSystemOnce;
        let mut w = world_with_sora(0);
        w.insert_resource(Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default())));
        w.resource_mut::<GameOver>().over = Some(Over::new(0, None));
        w.query::<&mut Sora>().single_mut(&mut w).unwrap().game_over = true;
        w.run_system_once(|mut go: ResMut<GameOver>, mut sora: Query<&mut Sora>, mut p: ResMut<Progress>| {
            rescue(&mut go, &mut sora, Some(&mut p), None, None);
        })
        .unwrap();
        assert_eq!(w.resource::<Progress>().mickey_save_count(), 1, "report row 4 counts the rescue");
        assert!(w.resource::<GameOver>().over.is_none());
        assert_eq!(sora(&mut w), (20, false), "Sora is up with the app's revive HP and no longer held");
    }

    #[test]
    fn the_plain_knock_out_has_no_alternate() {
        use bevy::ecs::system::RunSystemOnce;
        // No manager, so no header bit 9: `alternate_offer` answers no (and does not roll) whatever the chance would be.
        let mut w = world_with_sora(10);
        w.run_system_once(detect).unwrap();
        w.query::<&mut Sora>().single_mut(&mut w).unwrap().hp = 0;
        w.run_system_once(detect).unwrap();
        let o = w.resource::<GameOver>().over.clone().expect("type 0 starts");
        assert_eq!((o.kind, o.alternate), (0, false));
        assert_eq!(w.resource::<GameOver>().seed, None, "no roll was made");
    }

    #[test]
    fn the_snapshot_rule_and_the_continue_destination() {
        assert!(snapshot_allowed(0x10, None));
        assert!(!snapshot_allowed(0x33, None), "map program 0x33 and above");
        assert!(!snapshot_allowed(0x10, Some(false)), "a manager without header bit 12");
        assert!(snapshot_allowed(0x10, Some(true)) && snapshot_allowed(0x7f, Some(true)), "bit 12 allows it whatever the program");
        let keep = continue_dest(None);
        assert_eq!((keep.world, keep.area, keep.entrance, keep.programs), (0xff, 0xff, 0xff, [0xff; 3]));
        let d = Dest { world: 2, area: 4, entrance: 7, programs: [1, 2, 3] };
        assert_eq!(continue_dest(Some(Snapshot { dest: d, hp: 9, mp: 4 })), d);
        // FUN_001abfe8(8): whatever the snapshot held, the refresher gives the maximum (never below 1 HP).
        assert_eq!(refreshed(20, 100), (20, 100));
        assert_eq!(refreshed(0, 0), (1, 0));
    }

    #[test]
    fn the_knock_out_edge_fires_once_per_fall() {
        assert!(!ko_edge(None, 0), "no earlier HP seen");
        assert!(!ko_edge(Some(10), 5));
        assert!(ko_edge(Some(10), 0));
        assert!(ko_edge(Some(1), -3));
        assert!(!ko_edge(Some(0), 0), "already down");
    }

    fn world_with_sora(hp: i32) -> bevy::ecs::world::World {
        let mut w = bevy::ecs::world::World::new();
        w.insert_resource(GameOver::default());
        w.insert_resource(Time::<Real>::default());
        w.insert_resource(Mission::default());
        w.spawn(Sora::for_test(hp, 20));
        w
    }

    /// Sora's HP and hold flag.
    fn sora(w: &mut bevy::ecs::world::World) -> (i32, bool) {
        let s = w.query::<&Sora>().single(w).unwrap();
        (s.hp, s.game_over)
    }

    /// `detect` as a system: an HP fall to 0 with no manager, no summon and no form starts type 0 once and holds Sora; a
    /// fall with Tinker Bell ready or under host 1.317's bit does not (those routes are `route`'s, tested above).
    #[test]
    fn detect_as_a_system_starts_type_0_once_and_holds_sora() {
        use bevy::ecs::system::RunSystemOnce;
        let mut w = world_with_sora(10);
        w.run_system_once(detect).unwrap();
        assert!(w.resource::<GameOver>().over.is_none(), "alive: nothing");
        w.query::<&mut Sora>().single_mut(&mut w).unwrap().hp = 0;
        w.run_system_once(detect).unwrap();
        let o = w.resource::<GameOver>().over.clone().expect("the fall starts a game over");
        assert_eq!((o.kind, o.phase, o.fresh), (0, Phase::Waiting, true));
        assert!(sora(&mut w).1, "he is held down");
        // A second tick at 0 HP is not another fall.
        w.resource_mut::<GameOver>().over = None;
        w.run_system_once(detect).unwrap();
        assert!(w.resource::<GameOver>().over.is_none());
    }

    /// `drive` as a system (needs a `Disc` resource, so skipped without `KH2_ISO`): a request from an ended manager
    /// starts type 3 with its exits and plays the start once; with no menu text the list cannot open, so after the wait
    /// Sora is released unchanged; a game over whose jump has run ends when the task is idle again and restores HP.
    #[test]
    fn drive_as_a_system_takes_the_manager_request_waits_and_ends_with_the_jump() {
        use bevy::ecs::system::RunSystemOnce;
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let mut w = world_with_sora(0);
        w.insert_resource(MenuScene::default());
        w.insert_resource(JumpTask::default());
        w.insert_resource(Disc::open(&iso));
        w.insert_resource(Assets::<Image>::default());
        w.init_resource::<SaveMenu>();
        w.init_resource::<Pause>();
        w.init_resource::<Messages<PlaySe>>();
        w.insert_resource(AreaRecord {
            world: 2,
            area: 4,
            entrance: 0,
            programs: [0; 3],
            programs_override: None,
            dest_programs: [0xff; 3],
            place_on_load: false,
            run: None,
            placed: None,
        });
        let exits = GameOverExits { retry: Some([2, 2, 0, 0, 0xb6, 1]), quit: None };
        w.resource_mut::<Mission>().game_over_request = Some(crate::struggle::GameOverRequest { kind: 3, exits });
        w.run_system_once(drive).unwrap();
        let o = w.resource::<GameOver>().over.clone().expect("the request starts a game over");
        assert_eq!((o.kind, o.phase, o.fresh, o.exits), (3, Phase::Waiting, false, Some(exits)));
        assert!(w.resource::<Mission>().game_over_request.is_none(), "taken");
        assert!(sora(&mut w).1);
        assert_eq!(w.resource::<Messages<PlaySe>>().len(), 1, "FUN_001ad378's sound plays once");
        // The wait runs out; the scene has no menu text, so the list cannot open and Sora is released.
        w.resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_secs_f32(OPEN_AFTER + 0.1));
        w.run_system_once(drive).unwrap();
        assert!(w.resource::<GameOver>().over.is_none());
        assert!(!sora(&mut w).1);
        assert_eq!(sora(&mut w).0, 0, "no list, no outcome: the stand-up of sora.rs is left to do it");
        // A game over in its jump phase ends when the task is idle again, and gives HP back.
        let mut o = Over::new(0, None);
        o.fresh = false;
        o.phase = Phase::Jumping;
        w.resource_mut::<GameOver>().over = Some(o);
        w.run_system_once(drive).unwrap();
        assert!(w.resource::<GameOver>().over.is_some(), "the jump has not started");
        *w.resource_mut::<JumpTask>() = JumpTask::FadeIn { kind: 1, t: 0.0 };
        w.run_system_once(drive).unwrap();
        assert!(w.resource::<GameOver>().over.as_ref().is_some_and(|o| o.started));
        *w.resource_mut::<JumpTask>() = JumpTask::Idle;
        w.run_system_once(drive).unwrap();
        assert!(w.resource::<GameOver>().over.is_none());
        assert_eq!(sora(&mut w).0, 20, "released with full HP");
    }

    /// The "Load Game" phase through `drive` (skipped without `KH2_ISO`, `Disc` is a resource): with the Load screen unable to
    /// open the list returns; a left screen returns it too; a read slot snapshots, hands HP and MP to Sora and starts the jump
    /// to the slot's area with fade kind 1.
    #[test]
    fn drive_load_phase_returns_the_list_or_enters_the_slots_area() {
        use bevy::ecs::system::RunSystemOnce;
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let mut w = world_with_sora(0);
        w.insert_resource(MenuScene::default());
        w.insert_resource(JumpTask::default());
        w.insert_resource(Disc::open(&iso));
        w.insert_resource(Assets::<Image>::default());
        w.init_resource::<SaveMenu>();
        w.init_resource::<Pause>();
        w.init_resource::<Messages<PlaySe>>();
        w.insert_resource(AreaRecord { world: 2, area: 4, entrance: 0, programs: [0; 3], programs_override: None, dest_programs: [0xff; 3], place_on_load: false, run: None, placed: None });
        let loading = || {
            let mut o = Over::new(0, None);
            o.fresh = false;
            o.phase = Phase::Loading;
            o
        };
        // The Save menu has no `save.2ld` here, so the request is refused and the list is back.
        w.resource_mut::<GameOver>().over = Some(loading());
        w.run_system_once(drive).unwrap();
        let o = w.resource::<GameOver>().over.clone().expect("the game over goes on");
        assert_eq!((o.phase, o.flow.stage), (Phase::List, list::Stage::List));
        assert!(w.resource::<Pause>().menu, "the field stays stopped");

        // The Load screen ended on its cancel.
        let mut o = loading();
        o.load_asked = true;
        w.resource_mut::<GameOver>().over = Some(o);
        w.resource_mut::<SaveMenu>().end_load_for_test(LoadOutcome::Left);
        w.run_system_once(drive).unwrap();
        assert_eq!(w.resource::<GameOver>().over.as_ref().map(|o| o.phase), Some(Phase::List));

        // The Load screen read a slot: tt03, HP 40, MP 25.
        let mut o = loading();
        o.load_asked = true;
        w.resource_mut::<GameOver>().over = Some(o);
        let dest = Dest::new(2, 3, 0);
        w.resource_mut::<SaveMenu>().end_load_for_test(LoadOutcome::Loaded(Loaded { dest, hp_mp: Some((40, 25)) }));
        w.run_system_once(drive).unwrap();
        assert_eq!(w.resource::<GameOver>().over.as_ref().map(|o| o.phase), Some(Phase::Jumping));
        assert!(matches!(*w.resource::<JumpTask>(), JumpTask::FadeOut { dest: d, kind: 1, .. } if d == dest), "the jump enters the slot's area");
        assert_eq!(w.resource::<GameOver>().snapshot, Some(Snapshot { dest, hp: 40, mp: 25 }), "FUN_001cb118 snapshots the loaded body");
        let (hp, mp) = w.query::<&Sora>().single(&w).map(|s| (s.hp, s.mp)).unwrap();
        assert_eq!((hp, mp), (20.min(40), 25), "HP is clamped to the maximum (20 in this test), MP from the record");
    }

    /// The disc's rows: record 1 has "Continue" (code 2) and "Load Game" (code 1), record 0x21 has "Quit" (4) and "Retry"
    /// (3), both without a cancel (skipped without `KH2_ISO`).
    #[test]
    fn the_disc_rows_of_the_game_over() {
        let Ok(iso) = std::env::var("KH2_ISO") else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let bar = a.read(FILE).expect("14mission.bar");
        let entries = kh2_bar::parse(&bar).unwrap();
        let e = entries.iter().find(|e| e.tag == TAG).expect("the slct entry");
        let t = parse_table(e.data);
        let sys = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").unwrap()).unwrap();
        let text = |m: u16| sys.text(u32::from(m & 0x7fff)).unwrap_or_default();
        let b = list::find(&t, RECORD_FIELD).expect("row 1");
        let rows = b.rows();
        assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), vec![CODE_CONTINUE, CODE_LOAD]);
        assert_eq!((text(rows[0].1).as_str(), text(rows[1].1).as_str()), ("Continue", "Load Game"));
        assert!(b.no_cancel);
        let b = list::find(&t, RECORD_MISSION).expect("row 0x21");
        let rows = b.rows();
        assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), vec![CODE_QUIT, CODE_RETRY]);
        assert_eq!((text(rows[0].1).as_str(), text(rows[1].1).as_str()), ("Quit", "Retry"));
        assert!(b.no_cancel);
        assert!(a.read("obj/GAMEOVER.mset").is_ok_and(|m| !m.is_empty()), "the scene file the task loads");
    }

    fn state() -> Progress {
        Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()))
    }

    /// Continue puts the area-start body back: munny, EXP, a story flag and the NEW tags' words earned or changed since are
    /// gone, the shared story included.
    #[test]
    fn continue_restores_the_area_start_body_and_drops_later_gains() {
        let mut p = state();
        p.apply_json(r#"{"munny": 100, "experience": 40, "story_flags": [2179]}"#).unwrap();
        let body = capture_body(&p, &[5, 6]);
        p.apply_json(r#"{"munny": 900, "experience": 700, "story_flags": [2179, 2267]}"#).unwrap();
        assert_eq!((p.wallet(), p.experience()), (900, 700));
        let go = GameOver { body: Some(body), ..Default::default() };
        let mut tags = NewTags::default();
        tags.set_seen_words(&[9, 9]);
        restore_snapshot(&go, Some(&mut p), Some(&mut tags));
        assert_eq!((p.wallet(), p.experience()), (100, 40), "gains since the area start are lost");
        assert!(p.story().set_flags().contains(&2179) && !p.story().set_flags().contains(&2267), "story flags: {:?}", p.story().set_flags());
        assert_eq!(tags.seen_words()[..2], [5, 6], "the NEW tags words come back with the body");
    }

    /// A body that does not apply changes nothing, and neither does a missing one.
    #[test]
    fn a_failed_restore_leaves_the_state_unchanged() {
        let mut p = state();
        p.apply_json(r#"{"munny": 321, "experience": 9, "story_flags": [2179]}"#).unwrap();
        let before = capture_body(&p, &[]);
        // munny applies first, then the bad key fails: the live state must keep its own values.
        let bad = r#"{"munny": 5, "experience": 6, "story_flags": [4096], "shop_seen": 3}"#;
        assert!(restore_body(&p, bad).is_err());
        let go = GameOver { body: Some(bad.into()), ..Default::default() };
        restore_snapshot(&go, Some(&mut p), None);
        restore_snapshot(&GameOver::default(), Some(&mut p), None);
        assert_eq!(capture_body(&p, &[]), before);
        assert_eq!((p.wallet(), p.experience()), (321, 9));
    }

    /// A BAR from (type, tag, data) entries: header, 16-byte entries with absolute offsets, then the data.
    fn bar(entries: &[(u16, &str, Vec<u8>)]) -> Vec<u8> {
        let mut out = vec![0u8; 16 + 16 * entries.len()];
        out[..4].copy_from_slice(b"BAR");
        out[4..8].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        for (i, (kind, tag, data)) in entries.iter().enumerate() {
            let off = out.len();
            out.extend_from_slice(data);
            let e = 16 + 16 * i;
            out[e..e + 2].copy_from_slice(&kind.to_le_bytes());
            out[e + 4..e + 8].copy_from_slice(tag.as_bytes());
            out[e + 8..e + 12].copy_from_slice(&(off as u32).to_le_bytes());
            out[e + 12..e + 16].copy_from_slice(&(data.len() as u32).to_le_bytes());
        }
        out
    }

    /// `FUN_001add40`: type 0 asks the BAR at index 0xfc, types 1 and 3 the one at 0xfd, type 2 none; the entry is named
    /// `%03d0` of the objentry byte (format string 0x37b878).
    #[test]
    fn the_scene_set_and_entry_name_follow_fun_001add40() {
        use super::model::{entry_name, set_index};
        assert_eq!((set_index(0), set_index(1), set_index(2), set_index(3)), (Some(0xfc), Some(0xfd), None, Some(0xfd)));
        assert_eq!((entry_name(1), entry_name(14), entry_name(27)), ("0010".to_string(), "0140".to_string(), "0270".to_string()));
    }

    /// The lookup takes the THIRD entry of the name (`FUN_00168510(.., 2)` counts down from 2), in the BAR at the asked
    /// index, and nothing when the index holds no data or the name is missing.
    #[test]
    fn the_scene_entry_is_the_third_of_its_name_in_the_asked_set() {
        use super::model::entry_data;
        let set = bar(&[
            (17, "0010", b"a".to_vec()),
            (17, "0010", b"b".to_vec()),
            (17, "0010", b"third".to_vec()),
            (17, "0010", b"d".to_vec()),
            (17, "0140", b"x".to_vec()),
            (17, "0140", b"y".to_vec()),
        ]);
        let other = bar(&[(17, "0010", b"p".to_vec()), (17, "0010", b"q".to_vec()), (17, "0010", b"other".to_vec())]);
        let mut entries: Vec<(u16, &str, Vec<u8>)> = (0..4).map(|_| (17u16, "DUMM", Vec::new())).collect();
        entries[2] = (20, "AK10", set);
        entries[3] = (20, "AK12", other);
        let file = bar(&entries);
        assert_eq!(entry_data(&file, 2, 1).as_deref(), Some(&b"third"[..]), "the third entry named 0010");
        assert_eq!(entry_data(&file, 3, 1).as_deref(), Some(&b"other"[..]), "the other set has its own");
        assert_eq!(entry_data(&file, 2, 14), None, "only two entries named 0140: no third");
        assert_eq!(entry_data(&file, 2, 5), None, "no entry named 0050");
        assert_eq!(entry_data(&file, 0, 1), None, "an empty index holds nothing");
        assert_eq!(entry_data(&file, 9, 1), None, "past the end");
        assert_eq!(entry_data(b"not a bar", 0, 1), None);
    }

    /// The real file: both sets hold a motion for Sora's byte (1), the two sets differ, and a byte with no entry reads nothing
    /// (skipped without `KH2_ISO`).
    #[test]
    fn the_disc_scene_has_sora_motions_in_both_sets() {
        use super::model::{scene_motion, FILE};
        let Ok(iso) = std::env::var("KH2_ISO") else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let file = a.read(FILE).expect("GAMEOVER.mset");
        let a0 = scene_motion(&file, 0xfc, 1).expect("type 0, Sora");
        let a1 = scene_motion(&file, 0xfd, 1).expect("types 1 and 3, Sora");
        let (m0, m1) = (a0.motion.as_ref().unwrap(), a1.motion.as_ref().unwrap());
        assert!(m0.frame_end > m0.frame_start && m1.frame_end > m1.frame_start, "{}..{} {}..{}", m0.frame_start, m0.frame_end, m1.frame_start, m1.frame_end);
        assert_ne!((m0.frame_end, m0.fps), (m1.frame_end, m1.fps), "the two sets are different motions");
        assert!(scene_motion(&file, 0xfc, 99).is_none(), "no entry for a model byte of 99");
    }

    /// `FUN_001abfe8(8)`: whatever the record held, HP and MP go to the maximum, a running MP charge ends, and nothing is
    /// below 1 HP.
    #[test]
    fn the_party_refresher_fills_hp_and_mp_and_ends_the_charge() {
        use bevy::ecs::system::RunSystemOnce;
        assert_eq!(refreshed(20, 100), (20, 100));
        assert_eq!(refreshed(0, -5), (1, 0));
        let mut w = world_with_sora(3);
        w.insert_resource(crate::magic::MagicState::default());
        w.resource_mut::<crate::magic::MagicState>().charge = crate::magic_rules::Charge { left: 40.0, total: 90.0 };
        w.query::<&mut Sora>().single_mut(&mut w).unwrap().mp = 7;
        w.run_system_once(|mut q: Query<&mut Sora>, mut m: Query<&mut PartyMember>, mut mg: ResMut<crate::magic::MagicState>| {
            refresh_party(&mut q, Some(100), Some(&mut mg.charge), &mut m);
        })
        .unwrap();
        let s = w.query::<&Sora>().single(&w).unwrap();
        assert_eq!((s.hp, s.mp), (s.stats.max_hp, 100));
        assert!(s.hp > 3, "HP was 3");
        assert!(!w.resource::<crate::magic::MagicState>().charge.active(), "the MP charge ended (FUN_001a05a8)");
    }
}
