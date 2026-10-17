//! Limits: the data rows, the conditions the game tests before a limit command is offered and run,
//! and as much of the activation as is possible without companions. See docs/limits-reactions.md.
//!
//! Game rules ported (addresses in `SLUS_210.05`; the notes in docs/player-data.md describe the rest):
//! - Row usable (FUN_001d1ce0): the row's world is 0 or the current world; the player's command
//!   lock bit is clear (`Flags` of the player's status, +0x8 & 0x20); each non-zero member byte among
//!   `Character`, `Summon`, `Group` is a party member that is present and not downed (FUN_00175ba0 +
//!   FUN_0019a3f8; a member id 2 or 3 is refused while the form field is 6); when the `Limit` item is a
//!   type 19 ability, the member `Character` must have its ability bit set.
//! - Command available (FUN_001b3e38, Execute 81): the row exists and is usable, no global command
//!   lock; a normal limit needs MP >= the command's `Cost` byte (FUN_001b3f00), else status 2 (not
//!   enough MP); a summon limit (`Summon` 200..=204) needs its summon object present (FUN_00175ba0 by
//!   NeoStatus, summon.rs) and a non-zero summon gauge.
//! - Execution (FUN_001b28a0): spend MP (normal) or the summon timer (`FUN_001788d8(-Cost)` through
//!   summon.rs), create the limit task (`LimitTask`, read by Dismiss's test FUN_001b3c00) with the
//!   command's `Time` as its duration, load the limit's resource BAR `limit/us/<FileName>`, and put
//!   the participants into the limit state. The app runs the first three steps (the BAR is listed,
//!   not played) and writes `LimitStarted`; the task body, the script, the effects and the
//!   participants' motions are gaps.
//!
//! - Script events (FUN_001b2e08, FUN_001b3118, FUN_001b28a0): see [`LimitBar`] and [`start_events`]; the app sends them as
//!   [`LimitScriptEvent`] messages and the receiving script (a summon's) is run by summon.rs.
//!
//! The app has no companions, so a normal limit is refused with "member absent" unless the harness
//! names a party (`KH2_LIMIT_PARTY=2,3`). That path exists to exercise the rules, not as gameplay.

use bevy::prelude::*;
use kh2_tables::tables as t;
use kh2_tables::player;

use crate::assets::Disc;
use crate::progress::Progress;
use crate::schedule::Sim;
use crate::sora::Sora;

/// Item type of an ability (`item.Type`).
const ITEM_ABILITY: u8 = 19;

/// The fields of a `limt` row the rules read, joined with its command and ability item.
#[derive(Clone, Debug, PartialEq)]
pub struct LimitRow {
    pub id: u8,
    /// The three participant bytes (`Character`, `Summon`, `Group`).
    pub members: [u8; 3],
    pub file: String,
    pub command: u16,
    pub item: u16,
    pub world: u16,
    /// From the command row: MP cost byte and duration.
    pub cost: u8,
    pub time: u16,
    pub message: u32,
    /// The ability bit of `item` when it is a type 19 item.
    pub ability_bit: Option<u16>,
}

impl LimitRow {
    pub fn is_summon(&self) -> bool {
        player::is_summon_limit(self.members[1])
    }
}

/// Rows of the disc's limit table with their command and ability data.
pub fn rows(limt: &[t::Limt], cmds: &[t::Cmd], items: &[t::Item]) -> Vec<LimitRow> {
    limt.iter()
        .map(|l| {
            let c = cmds.iter().find(|c| c.id == l.command);
            let it = items.iter().find(|i| i.id == l.limit);
            LimitRow {
                id: l.id,
                members: [l.character, l.summon, l.group],
                file: l.file_name.clone(),
                command: l.command,
                item: l.limit,
                world: l.world,
                cost: c.map_or(0, |c| c.cost),
                time: c.map_or(0, |c| c.time),
                message: c.map_or(0, |c| c.message_id as u32),
                ability_bit: it.filter(|i| i.r#type == ITEM_ABILITY).map(|i| i.flag1 as u16 | (i.rank as u16) << 8),
            }
        })
        .collect()
}

/// What the tests read about the party and the world.
#[derive(Clone, Debug, Default)]
pub struct Party {
    /// Member ids (1 Sora, 2 Donald, 3 Goofy, 5 Auron ...) that are in the party and not downed.
    pub present: Vec<u8>,
    /// Ability bits of the member a row's `Character` names.
    pub abilities: u128,
    /// The player's form field (6 is a special non-drive mode).
    pub form: u8,
    pub world: u16,
    /// The player's command lock (FUN_001d1ce0 reads status flag 0x20).
    pub locked: bool,
    pub mp: i32,
    /// The summon gauge (status +0x1b4) is non-zero.
    pub summon_gauge: bool,
    /// The NeoStatus of the summon objects present (`FUN_00175ba0` finds a summon row's member among
    /// the party slots by it; summon.rs).
    pub summons: Vec<u16>,
    /// `FUN_001b3e38` answers 5 (unavailable) while a limit task exists (`FUN_001b3c00`) or the word `0x35fec0` is non-zero (a summon's
    /// arrival or leave sequence holds bit 3, `FUN_001c6578` .. `FUN_001c65b8`). The harness paths leave it false.
    pub busy: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    WrongWorld,
    Locked,
    /// A participant byte names a member that is not in the party (or is downed).
    MemberAbsent(u8),
    /// The type 19 ability item is not learned (the bit).
    AbilityMissing(u16),
    /// Status 2 of FUN_001b3e38.
    NotEnoughMp,
    SummonGaugeEmpty,
    /// Status 5 of `FUN_001b3e38` for a limit task or the word `0x35fec0` ([`Party::busy`]).
    Busy,
}

/// FUN_001d1ce0: is the row usable now.
pub fn usable(r: &LimitRow, p: &Party) -> Result<(), Refusal> {
    if r.world != 0 && r.world != p.world {
        return Err(Refusal::WrongWorld);
    }
    if p.locked {
        return Err(Refusal::Locked);
    }
    for &m in r.members.iter().filter(|&&m| m != 0) {
        if p.form == 6 && (m == 2 || m == 3) {
            return Err(Refusal::MemberAbsent(m));
        }
        // Summon limits name a summon (200..=204): FUN_00175ba0 finds the summon object by NeoStatus.
        if player::is_summon_limit(m) {
            if !p.summons.contains(&(m as u16)) {
                return Err(Refusal::MemberAbsent(m));
            }
            continue;
        }
        if m != 1 && !p.present.contains(&m) {
            return Err(Refusal::MemberAbsent(m));
        }
    }
    if let Some(bit) = r.ability_bit
        && (bit >= 128 || p.abilities >> bit & 1 == 0)
    {
        return Err(Refusal::AbilityMissing(bit));
    }
    Ok(())
}

/// FUN_001b3e38 for Execute 81: usable, then MP (normal) or the summon gauge (summon limit).
pub fn available(r: &LimitRow, p: &Party) -> Result<(), Refusal> {
    usable(r, p)?;
    if p.busy {
        return Err(Refusal::Busy);
    }
    if r.is_summon() {
        if !p.summon_gauge {
            return Err(Refusal::SummonGaugeEmpty);
        }
    } else if p.mp < mp_needed(r) {
        return Err(Refusal::NotEnoughMp);
    }
    Ok(())
}

/// The MP a normal limit needs and spends. Every normal limit's `Cost` byte on this disc is 255.
/// Read literally (FUN_001b3e38 compares the byte, FUN_001b9ff0 only replaces it in a special
/// battle) no MP value reaches it, so the notes in docs/player-data.md call it "all MP". The app
/// follows that reading [inferred]: any MP is enough and the limit spends all of it. Whether the
/// game's MP unit makes 255 reachable was not checked (the MP block at status +0x180 was not traced).
pub fn mp_needed(r: &LimitRow) -> i32 {
    if r.cost == 255 { 1 } else { r.cost as i32 }
}

/// The first usable row of a command (FUN_001d2040 walks the table in order).
pub fn row_for_command(rows: &[LimitRow], command: u16, p: &Party) -> Option<usize> {
    rows.iter().position(|r| r.command == command && usable(r, p).is_ok())
}

/// Why the party-limit offer, and the start of an accepted one, are refused in a state the app can model. `FUN_001d2460` and
/// `FUN_0019c670` / `FUN_0019ae90` (the controller's own gate) read words the app does not have; these are the ones it does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StartGates {
    /// Sora is KO or the Viewer (`FUN_0019ae90`: `+0x7f8` bit 4 is the KO bit; the Viewer stands for the states `FUN_0019a440` tests).
    pub sora_down: bool,
    /// The pause menu is open or closing: the game's menu holds the commands (`DAT_0035d268` bit 2, the global command lock, which
    /// turns every command without Flags bit 5 into status 5, `FUN_001b70b8`) [inferred: that the menu sets it].
    pub menu_open: bool,
    /// An event plays (`FUN_001ad378` sets `DAT_0035d268` bit 2 at its start) [inferred: that this is the app's `event_active`].
    pub event: bool,
    /// A summon sequence runs: `FUN_001d2460` refuses when the word `0x35feb8` is non-zero and `DAT_01d5e368` is not 2, so the
    /// arrival and the leave refuse and an active summon (state 2) does not.
    pub summon_sequence: bool,
}

/// What a gate refused with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    SoraDown,
    MenuOpen,
    EventPlaying,
    SummonSequence,
}

impl StartGates {
    pub fn check(&self) -> Result<(), Gate> {
        if self.sora_down {
            Err(Gate::SoraDown)
        } else if self.summon_sequence {
            Err(Gate::SummonSequence)
        } else if self.menu_open {
            Err(Gate::MenuOpen)
        } else if self.event {
            Err(Gate::EventPlaying)
        } else {
            Ok(())
        }
    }
}

/// What the gates read, as one system parameter.
pub type GateParams<'w> = (Option<Res<'w, crate::menu::Menu>>, Option<Res<'w, crate::intro::event_player::EventPlayer>>, Option<Res<'w, crate::summon::SummonState>>);

/// The gates from the app's state.
pub fn gates_of(g: &GateParams, sora_down: bool) -> StartGates {
    StartGates {
        sora_down,
        menu_open: g.0.as_deref().is_some_and(|m| m.phase != crate::menu::Phase::Closed),
        event: g.1.as_deref().is_some_and(|e| e.playing()),
        summon_sequence: g.2.as_deref().is_some_and(|s| s.sequence && s.state != 2),
    }
}

/// Sora cannot take a command: KO (no HP), down, or the Viewer.
pub fn sora_is_down(s: &Sora) -> bool {
    s.hp <= 0 || matches!(s.state, crate::sora::State::Down | crate::sora::State::Viewer)
}

/// The ability item of Auto Limit (ability bit 129 of the member's own block, `abilities::BIT_AUTO_LIMIT`).
pub const AUTO_LIMIT_ITEM: u16 = 417;

/// `DAT_0037c414` (500.0 in the executable): how near, horizontally, a partner must stand to the player for `FUN_001d25e0`.
pub const AUTO_LIMIT_RANGE: f32 = 500.0;

/// One party member in the field as `FUN_001d25e0` sees him (`FUN_0017d400` of a row's `Character`).
#[derive(Clone, Debug, Default)]
pub struct Partner {
    /// The party member id (`limt` `Character`: 2 Donald, 3 Goofy ...).
    pub member: u8,
    /// Position in game units (the app's axes; only x and z are read).
    pub pos: Vec3,
    /// The ability items he has equipped (his own ability block).
    pub items: Vec<u16>,
}

impl Partner {
    /// Bit 129 of his own block (`FUN_001768a8(member) + 0x10 & 2`).
    pub fn has_auto_limit(&self) -> bool {
        self.items.contains(&AUTO_LIMIT_ITEM)
    }
}

/// What the rows' tests see for one partner: the base party with the ability bit of the row's limit item as that member holds it.
pub fn party_for(base: &Party, partner: &Partner, row: &LimitRow) -> Party {
    let mut p = base.clone();
    p.abilities = match row.ability_bit {
        Some(b) if b < 128 && partner.items.contains(&row.item) => 1u128 << b,
        _ => 0,
    };
    p
}

/// A row `FUN_001d25e0` would offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoCandidate {
    /// Index into the limit table.
    pub row: usize,
    /// The row's command (`+0x30` of the `limt` row, `FUN_001b6f48`).
    pub command: u16,
    pub member: u8,
    /// `FUN_001ba2c0` answered 2 (not enough MP): still a candidate (the test is `< 4`), but the offer cannot be accepted.
    pub no_mp: bool,
}

/// The candidate walk of `FUN_001d25e0`, in table order: a row whose `Character` is not 1 and is usable (`FUN_001d1ce0`), whose
/// partner has Auto Limit and stands within [`AUTO_LIMIT_RANGE`] (horizontal), and whose command's status for the player is below 4
/// (`FUN_001ba2c0`: Execute 81 is `FUN_001b3e38`, 5 when no usable row exists for the command's gauge or MP is not what the rule
/// wants; 2 for too little MP; then the command's Flags and DisableForm test, `flags_ok`, which makes it 5).
/// `FUN_001d1ea0` (the limit's file is loaded, the party members idle) is not modelled [approx]: it is taken as true.
pub fn auto_limit_candidates(rows: &[LimitRow], partners: &[Partner], player: Vec3, base: &Party, flags_ok: impl Fn(u16) -> bool) -> Vec<AutoCandidate> {
    let mut out = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        let member = r.members[0];
        if member <= 1 {
            continue;
        }
        let Some(partner) = partners.iter().find(|p| p.member == member) else { continue };
        let party = party_for(base, partner, r);
        if usable(r, &party).is_err() || !partner.has_auto_limit() {
            continue;
        }
        let d = Vec3::new(partner.pos.x - player.x, 0.0, partner.pos.z - player.z).length();
        if d > AUTO_LIMIT_RANGE {
            continue;
        }
        // FUN_001b3e38 asks FUN_001d2040 for the command's first usable row; none answers 0 (available).
        let status_row = row_for_command(rows, r.command, &party).map_or(r, |j| &rows[j]);
        let no_mp = match available(status_row, &party) {
            Ok(()) => false,
            Err(Refusal::NotEnoughMp) => true,
            Err(_) => continue,
        };
        if !flags_ok(r.command) {
            continue;
        }
        out.push(AutoCandidate { row: i, command: r.command, member, no_mp });
    }
    out
}

/// The choice at the end of `FUN_001d25e0`: `wanted` (the command the offer already shows) wins at once, with no draw; none
/// answers nothing; otherwise the generator steps once (`DAT_003716a0 *= 0x10dcd`) and the index is the new word divided by
/// `(2^32 + n - 1) / n` (`FUN_00325b38` twice, unsigned: the call site zero-extends the word with `dsll32` / `dsrl32`).
pub fn auto_limit_pick(commands: &[u16], wanted: u16, seed: &mut u32) -> Option<usize> {
    if let Some(i) = commands.iter().position(|&c| c == wanted) {
        return Some(i);
    }
    let n = commands.len() as u64;
    if n == 0 {
        return None;
    }
    *seed = seed.wrapping_mul(0x10dcd);
    let step = ((1u64 << 32) + n - 1) / n;
    Some((u64::from(*seed) / step) as usize)
}

/// The generator `FUN_001d25e0` draws from (`DAT_003716a0`); the app keeps its own copy for this pick. `KH2_AUTO_LIMIT_SEED=<n>` fixes it.
#[derive(Resource, Debug)]
pub struct LimitRng(pub u32);

impl Default for LimitRng {
    fn default() -> Self {
        let clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.subsec_nanos() | 1);
        LimitRng(std::env::var("KH2_AUTO_LIMIT_SEED").ok().and_then(|v| v.parse().ok()).unwrap_or(clock))
    }
}

/// The player accepted a limit offer (Execute 81 through the player class, `FUN_001cf030` case 0x51 -> `FUN_001b28a0`). `party` is
/// what the offer's tests saw, so the start sees the same members and abilities.
#[derive(Message, Clone, Debug)]
pub struct StartLimit {
    pub command: u16,
    pub party: Party,
}

#[derive(Resource)]
pub struct LimitRules {
    pub rows: Vec<LimitRow>,
    /// Commands with Execute 81 and no `limt` row: the sub-commands of a running limit (Stitch's 525 to 527 ...).
    pub sub_commands: Vec<u16>,
}

/// The Execute 81 commands that no `limt` row names (`FUN_001d2040` finds no row, so `FUN_001b28a0` sends event 23).
pub fn sub_commands(limt: &[t::Limt], cmds: &[t::Cmd]) -> Vec<u16> {
    cmds.iter().filter(|c| c.execute == 81 && !limt.iter().any(|l| l.command == c.id)).map(|c| c.id).collect()
}

/// The party the rules see. The app has only Sora; `KH2_LIMIT_PARTY=2,3` (and `KH2_LIMIT_ABILITIES`,
/// a comma list of ability bits) pretends the others are there so the rules can be exercised.
#[derive(Resource, Default)]
pub struct LimitParty(pub Party);

/// Sent when a limit starts. A limit task (script, effects, participants) would hang off this.
#[allow(dead_code)]
#[derive(Message, Clone, Debug)]
pub struct LimitStarted {
    pub command: u16,
    pub file: String,
    /// The command's `Time`, in the units the game's limit task takes.
    pub time: u16,
}

/// Entry kinds of a limit's resource BAR (`limit/us/<FileName>`) that the limit task looks up (`FUN_001684c0(bar, kind, 0)`).
pub const KIND_SCRIPT: u16 = 3;
/// `FUN_001b2e08` stores the entry's data pointer at task `+8`; non-zero means the limit has its own object set ("LIMI").
pub const KIND_PARTY_SET: u16 = 0x14;

/// What the limit task reads of the BAR's entry list: which of its optional parts exist.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LimitBar {
    /// A type 3 entry: the limit's own script, started as a thread at task creation (`FUN_001b2e08`, `FUN_0016c9b8`).
    pub script: bool,
    /// A type 0x14 entry (task `+8` non-zero, `FUN_001b3bc0` false): the limit brings its own object set. The task then sets
    /// bit 0 of the word `0x35feb8` (`FUN_001c6bd0`, cleared again by `FUN_001b32d8`) and the first participant's script is
    /// not asked to run; without it `FUN_001b3118` starts a thread on that participant's script instead.
    pub party_set: bool,
}

impl LimitBar {
    pub fn from_kinds(kinds: &[u16]) -> Self {
        LimitBar { script: kinds.contains(&KIND_SCRIPT), party_set: kinds.contains(&KIND_PARTY_SET) }
    }
}

/// Who a limit script event goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitReceiver {
    /// The thread of the limit BAR's own type 3 script (see [`event_receiver`] for when it is the one task `+0x34` names).
    LimitScript,
    /// A thread on the script of the first participant byte above 1 (task `+0x10`): a summon (200 to 204) for a summon limit.
    Participant(u8),
}

/// The one argument of a limit script event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitArg {
    /// Event 22 (0x16): the limit task object (word `+0xc` the player, `+0x10` the first participant).
    Task,
    /// Event 23 (0x17): the command id of a sub-command (`FUN_001b28a0` first branch).
    Command(u16),
}

/// A script event of the limit task (event 22 at the start, event 23 for sub-commands). summon.rs delivers it to the summon.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LimitScriptEvent {
    pub event: u8,
    pub to: LimitReceiver,
    pub arg: LimitArg,
}

/// The first participant byte above 1 (`FUN_001b2c10` stores `FUN_0017d400(byte)` of each such byte at task `+0x10..`).
pub fn first_participant(r: &LimitRow) -> Option<u8> {
    r.members.iter().copied().find(|&m| m > 1)
}

/// The events the task creation sends, in order (`FUN_001b2c10`): `FUN_001b2e08` starts the BAR's type 3 script with event 22
/// and the task as argument; then, when the BAR has no type 0x14 entry, `FUN_001b3118` starts a thread on the first
/// participant's script (`+0x484`) for event 22 with the same argument. [inferred] That a participant without a script
/// (`+0x484` zero) is skipped is read from the guard in `FUN_001b3118`; the app checks it in summon.rs.
pub fn start_events(bar: &LimitBar, r: &LimitRow) -> Vec<LimitScriptEvent> {
    let mut ev = Vec::new();
    if bar.script {
        ev.push(LimitScriptEvent { event: 22, to: LimitReceiver::LimitScript, arg: LimitArg::Task });
    }
    if !bar.party_set
        && let Some(m) = first_participant(r)
    {
        ev.push(LimitScriptEvent { event: 22, to: LimitReceiver::Participant(m), arg: LimitArg::Task });
    }
    ev
}

/// The thread task `+0x34` names after creation, which event 23 goes to: `FUN_001b2e08` stores the type 3 script's thread there,
/// and `FUN_001b3118` (no type 0x14 entry) overwrites it with the participant's. None when neither exists.
pub fn event_receiver(bar: &LimitBar, r: &LimitRow) -> Option<LimitReceiver> {
    start_events(bar, r).last().map(|e| e.to)
}

/// `FUN_001b28a0` first branch: Execute 81 for a command without a usable `limt` row sends event 23 with the command id to
/// the running limit's thread (`FUN_0016c620(task + 0x34, 0x17, [id], 1, 0)`). With no limit task the original reads
/// `DAT_01d5d6b0 + 0x34` of a null pointer [GAP: what the interpreter does with that thread word was not read]; the app sends nothing.
pub fn sub_command_event(thread: Option<LimitReceiver>, command: u16) -> Option<LimitScriptEvent> {
    thread.map(|to| LimitScriptEvent { event: 23, to, arg: LimitArg::Command(command) })
}

/// The receiver of event 23 for the running limit.
#[derive(Resource, Default, Debug)]
pub struct LimitThread(pub Option<LimitReceiver>);

/// The limit task (`DAT_01d5d6b0`, created by `FUN_001b2c10` in `FUN_001cf030` case 0x51): the command
/// and the ticks left of its duration. `FUN_001b3c00` (Dismiss's test) reads whether it exists.
#[derive(Resource, Default, Debug)]
pub struct LimitTask(pub Option<(u16, u32)>);

/// The running limit's hit count: the original's gauge object at limit task `+0x68` (`FUN_001b2e08` sets its range 0..999 and
/// raises its record marker, `+0x38` of the gauge = task `+0xa0`, to the stored best with `FUN_001d00c0`). `FUN_001cffa8` adds to
/// the count and raises the marker to it; when the task ends `FUN_001b32d8` stores the marker with `FUN_001d25b0` under the
/// limit's `limt` `Id`. [inferred] That the count is the limit's hit count: the callers of `FUN_001cffa8` are virtual and the
/// limit task body is not ported. WP-CT1 found the caller: the enemies' hit-receive method `FUN_001849d8` runs `FUN_001b21f0`, which
/// calls `FUN_001b3dc8` while a limit task exists (`FUN_001b3c00`) and the hit record is not ineffective (`FUN_0018ba88`: flag 4 clear,
/// and a reaction-5 hit needs damage), and that adds 1 (`FUN_001cffa8(0, task + 0x68, 1)`). So the count is the effective hits that
/// enemies receive while the limit runs; [`count_limit_hits`] does that from the app's hit messages. (`KH2_LIMIT_HITS=<n>` also adds `n`
/// at a limit's start, a debug aid.)
#[derive(Resource, Default, Debug)]
pub struct LimitHits {
    /// The `limt` `Id` of the running limit (the best-count slot).
    pub slot: u8,
    /// The record marker (task `+0xa0`), at most 999.
    pub marker: u32,
    count: u32,
}

impl LimitHits {
    /// A limit starts: the count is 0 and the marker starts at the saved best (`FUN_001d00c0` in `FUN_001b2e08`).
    pub fn start(&mut self, slot: u8, best: u16) {
        *self = LimitHits { slot, marker: u32::from(best), count: 0 };
    }

    /// `FUN_001cffa8`: adds to the count (clamped to the gauge's 0..999) and raises the marker to it.
    pub fn add(&mut self, n: i32) {
        self.count = (self.count as i32 + n).clamp(0, 999) as u32;
        self.marker = self.marker.max(self.count);
    }

    /// `FUN_001b32d8` end: stores the marker as the limit's best count (`FUN_001d25b0` never lowers it).
    pub fn finish(&self, progress: &mut Progress) {
        progress.raise_limit_best(self.slot, self.marker as u16);
    }
}

/// How many of the enemy hits of a frame the limit counts: every hit, less the ones the victim's receiver marked ineffective
/// (`FUN_0018ba88`; each ineffective message cancels one hit on the same target). [inferred] A reaction-5 hit with no damage is not
/// removed: the app's `AiHit` does not carry the damage.
pub fn counted_hits(hits: &[Entity], ineffective: &[Entity]) -> i32 {
    let mut left: Vec<Entity> = ineffective.to_vec();
    let mut n = 0;
    for h in hits {
        if let Some(i) = left.iter().position(|e| e == h) {
            left.swap_remove(i);
        } else {
            n += 1;
        }
    }
    n
}

/// `FUN_001b3dc8` through the enemies' receive method: while a limit runs, each effective hit on an enemy adds 1 to the limit's count.
fn count_limit_hits(
    task: Res<LimitTask>,
    mut count: ResMut<LimitHits>,
    mut hits: MessageReader<crate::enemy_ai::AiHit>,
    mut ineffective: MessageReader<crate::combat::IneffectiveHit>,
) {
    let landed: Vec<Entity> = hits.read().map(|h| h.target).collect();
    let useless: Vec<Entity> = ineffective.read().map(|h| h.target).collect();
    if task.0.is_some() {
        count.add(counted_hits(&landed, &useless));
    }
}

#[derive(Resource, Default)]
struct Selected {
    next: usize,
    scripted: Option<u16>,
    armed: bool,
    /// L was pressed since the last tick (latched in `Update`, consumed by the 60 Hz tick).
    pressed: bool,
    /// `KH2_LIMIT_SUB=<command id>`: a sub-command sent as event 23 `SUB_DELAY` ticks after a limit starts (a harness aid).
    sub: Option<u16>,
    /// Ticks since the running limit started.
    since: u32,
}

fn read_key(keys: Res<ButtonInput<KeyCode>>, mut sel: ResMut<Selected>) {
    if keys.just_pressed(KeyCode::KeyL) {
        sel.pressed = true;
    }
}

pub struct LimitPlugin;

impl Plugin for LimitPlugin {
    fn build(&self, app: &mut App) {
        let rows = app.world().get_resource::<Disc>().and_then(|d| {
            let mut a = d.0.lock().ok()?;
            let limt = kh2_tables::load::<t::Limt>(&mut a, &t::LIMT).ok()?.rows;
            let cmds = kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok()?.rows;
            let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).ok()?.rows;
            Some((rows(&limt, &cmds, &items), sub_commands(&limt, &cmds)))
        });
        let Some((rows, sub_commands)) = rows else {
            warn!("limit: tables not readable; limits are off");
            return;
        };
        let env = |k: &str| std::env::var(k).ok();
        let list = |k: &str| env(k).map(|v| v.split(',').filter_map(|x| x.trim().parse::<u16>().ok()).collect::<Vec<_>>()).unwrap_or_default();
        let party = Party {
            present: list("KH2_LIMIT_PARTY").into_iter().map(|m| m as u8).collect(),
            abilities: list("KH2_LIMIT_ABILITIES").into_iter().filter(|&b| b < 128).fold(0, |a, b| a | 1u128 << b),
            world: 2,
            ..Default::default()
        };
        app.insert_resource(LimitRules { rows, sub_commands })
            .insert_resource(LimitParty(party))
            .insert_resource(Selected { scripted: env("KH2_LIMIT").and_then(|v| v.parse().ok()), sub: env("KH2_LIMIT_SUB").and_then(|v| v.parse().ok()), ..Default::default() })
            .init_resource::<LimitTask>()
            .init_resource::<LimitHits>()
            .init_resource::<LimitThread>()
            .add_message::<LimitStarted>()
            .add_message::<LimitScriptEvent>()
            .add_message::<StartLimit>()
            .init_resource::<LimitRng>()
            .add_systems(Update, read_key)
            .add_systems(FixedUpdate, (try_limit.in_set(Sim::Sora), count_limit_hits));
    }
}

/// What the limit rules read from and send to the summon system (summon.rs).
type SummonSide<'w> = (
    Option<Res<'w, crate::summon::SummonState>>,
    Option<Res<'w, crate::drive::DriveState>>,
    Option<Res<'w, crate::summon::SummonData>>,
    Option<ResMut<'w, crate::summon::SummonQueue>>,
);

/// Ticks after a limit's start at which `KH2_LIMIT_SUB` sends its sub-command.
const SUB_DELAY: u32 = 120;

/// `FUN_001b28a0` first branch for a command without a limit row.
fn send_sub_command(rules: &LimitRules, command: u16, thread: Option<LimitReceiver>, out: &mut MessageWriter<LimitScriptEvent>) {
    if !rules.sub_commands.contains(&command) {
        info!("limit: command {command} has no limit row and is not an Execute 81 command");
        return;
    }
    match sub_command_event(thread, command) {
        Some(e) => {
            info!("limit: sub-command {command}: script event 23 to {:?} (FUN_001b28a0 first branch)", e.to);
            out.write(e);
        }
        None => info!("limit: sub-command {command} with no running limit: the original sends event 23 to a null thread, the app sends nothing"),
    }
}

/// Key `L` tries the next limit command in table order; `KH2_LIMIT=<command id>` tries that one once
/// at about 4 s. Both print the decision (every limit) and run an available limit.
#[allow(clippy::too_many_arguments)]
fn try_limit(
    time: Res<Time>,
    rules: Res<LimitRules>,
    mut party: ResMut<LimitParty>,
    mut sel: ResMut<Selected>,
    disc: Res<Disc>,
    mut sora: Query<&mut Sora>,
    mut out: MessageWriter<LimitStarted>,
    mut active: ResMut<LimitTask>,
    mut hits: ResMut<LimitHits>,
    mut thread: ResMut<LimitThread>,
    mut script_events: MessageWriter<LimitScriptEvent>,
    mut progress: Option<ResMut<Progress>>,
    summon: SummonSide,
    mut requests: MessageReader<StartLimit>,
    gate_params: GateParams,
) {
    let (sm, drive, summon_data, mut summon_queue) = summon;
    if let Some((cmd, left)) = active.0.as_mut() {
        *left = left.saturating_sub(1);
        if *left == 0 {
            info!("limit: command {cmd} ended (duration over)");
            active.0 = None;
            thread.0 = None;
            if let Some(p) = progress.as_deref_mut() {
                hits.finish(p);
                info!("limit: best count of slot {} is now {} (FUN_001b32d8 -> FUN_001d25b0)", hits.slot, p.limit_best(hits.slot));
            }
        }
    }
    let mut request = requests.read().last().cloned();
    let Ok(mut s) = sora.single_mut() else { return };
    // The start is refused in the states the offer is (the player may have been hit, opened the menu or entered an event since).
    if request.is_some()
        && let Err(g) = gates_of(&gate_params, sora_is_down(&s)).check()
    {
        info!("limit: start refused ({g:?}: FUN_001d2460 / FUN_0019ae90 gate)");
        request = None;
    }
    if active.0.is_some() {
        sel.since += 1;
        if sel.since == SUB_DELAY
            && let Some(c) = sel.sub
        {
            send_sub_command(&rules, c, thread.0, &mut script_events);
        }
    }
    let command = if let Some(r) = request.filter(|_| active.0.is_none()) {
        // An accepted Auto Limit offer: the party the offer saw runs the rules (the original starts it through FUN_001b28a0).
        party.0 = r.party;
        Some(r.command)
    } else if let Some(c) = sel.scripted.filter(|_| !sel.armed && time.elapsed_secs() >= 4.0) {
        sel.armed = true;
        Some(c)
    } else if std::mem::take(&mut sel.pressed) {
        let mut cmds: Vec<u16> = Vec::new();
        for r in &rules.rows {
            if !cmds.contains(&r.command) {
                cmds.push(r.command);
            }
        }
        let c = cmds.get(sel.next % cmds.len().max(1)).copied();
        sel.next += 1;
        c
    } else {
        None
    };
    let Some(command) = command else { return };
    party.0.mp = s.mp;
    // The summon gauge: the mode-3 timer (status +0x1b4) is non-zero; a summon row's member is the
    // summon object of its NeoStatus (FUN_001d1ce0 -> FUN_00175ba0), from summon.rs.
    party.0.summon_gauge = match (&sm, &drive) {
        (Some(sm), Some(d)) => sm.gauge(d),
        _ => false,
    };
    party.0.summons = sm.as_ref().map_or_else(Vec::new, |s| s.present.clone());
    let Some(i) = rules.rows.iter().position(|r| r.command == command) else {
        send_sub_command(&rules, command, thread.0, &mut script_events);
        return;
    };
    // The first usable row of the command wins (FUN_001d2040); otherwise report the first row's refusal.
    let i = row_for_command(&rules.rows, command, &party.0).unwrap_or(i);
    let r = &rules.rows[i];
    let desc = format!("limit {} command {command} (participant bytes {:?}, file {}, cost {}, time {})", r.id, r.members, r.file, r.cost, r.time);
    match available(r, &party.0) {
        Err(e) => info!("limit: {desc} refused: {e:?} (party {:?})", party.0.present),
        Ok(()) => {
            // A summon limit spends the summon timer (FUN_001cf030 case 0x51: FUN_001788d8(-Cost), so
            // `Cost x sstm[0x70]`, through summon.rs); a normal one MP.
            if r.is_summon() {
                if let (Some(d), Some(q)) = (summon_data.as_deref(), summon_queue.as_deref_mut()) {
                    let c = crate::drive::rules::summon_limit_cost(&d.tuning, r.cost);
                    q.0.push(crate::summon::SummonRequest::Gauge(c));
                    info!("limit: summon limit cost {} x {} = {c} to the summon timer", r.cost, d.tuning.gain);
                }
            } else {
                s.mp = if r.cost == 255 { 0 } else { (s.mp - r.cost as i32).max(0) };
            }
            let entries = disc
                .read(&format!("limit/us/{}", r.file))
                .and_then(|b| kh2_bar::parse(&b).ok().map(|es| es.iter().map(|e| format!("{}:{}", e.tag, e.kind)).collect::<Vec<_>>().join(" ")))
                .unwrap_or_else(|| "(resource not found)".into());
            info!("limit: {desc} started, MP now {}; resource BAR entries {entries}; the task body is not ported", s.mp);
            active.0 = Some((command, (r.time as u32).max(1)));
            sel.since = 0;
            let bar = disc.read(&format!("limit/us/{}", r.file)).and_then(|b| kh2_bar::parse(&b).ok().map(|es| LimitBar::from_kinds(&es.iter().map(|e| e.kind).collect::<Vec<_>>()))).unwrap_or_default();
            thread.0 = event_receiver(&bar, r);
            for e in start_events(&bar, r) {
                info!("limit: script event {} ({:?}) to {:?} (FUN_001b2e08 / FUN_001b3118)", e.event, e.arg, e.to);
                script_events.write(e);
            }
            hits.start(r.id, progress.as_deref().map_or(0, |p| p.limit_best(r.id)));
            if let Some(n) = std::env::var("KH2_LIMIT_HITS").ok().and_then(|v| v.parse::<i32>().ok()) {
                hits.add(n);
            }
            out.write(LimitStarted { command, file: r.file.clone(), time: r.time });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The record marker follows the count; the end stores it as the slot's best and never lowers a better one.
    #[test]
    fn the_end_of_a_limit_raises_the_slots_best() {
        let mut p = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        p.raise_limit_best(4, 6);
        let mut h = LimitHits::default();
        h.start(4, p.limit_best(4));
        h.add(3);
        h.finish(&mut p);
        assert_eq!(p.limit_best(4), 6, "a lower count keeps the best");
        h.start(4, p.limit_best(4));
        h.add(5);
        h.add(4);
        assert_eq!(h.marker, 9);
        h.finish(&mut p);
        assert_eq!(p.limit_best(4), 9);
        assert_eq!(p.limit_best(5), 0, "another slot is untouched");
        h.start(5, 0);
        h.add(5000);
        assert_eq!(h.marker, 999, "the gauge ends at 999");
    }

    /// `FUN_001b3dc8` counts a hit only while the limit task exists, and `FUN_0018ba88` leaves out an ineffective one.
    #[test]
    fn limit_hits_count_effective_enemy_hits_only() {
        let (a, b) = (Entity::from_raw_u32(1).unwrap(), Entity::from_raw_u32(2).unwrap());
        assert_eq!(counted_hits(&[a, a, b], &[]), 3);
        assert_eq!(counted_hits(&[a, a, b], &[a]), 2, "one ineffective message cancels one hit on that target");
        assert_eq!(counted_hits(&[b], &[a]), 1);
        let mut app = App::new();
        app.init_resource::<LimitTask>().init_resource::<LimitHits>().add_message::<crate::enemy_ai::AiHit>().add_message::<crate::combat::IneffectiveHit>().add_systems(Update, count_limit_hits);
        let send = |app: &mut App, n: usize| {
            for _ in 0..n {
                app.world_mut().write_message(crate::enemy_ai::AiHit { target: a, real: true, reaction: 2 });
            }
            app.update();
        };
        send(&mut app, 2);
        assert_eq!(app.world().resource::<LimitHits>().marker, 0, "no limit task: nothing counts");
        app.world_mut().resource_mut::<LimitHits>().start(4, 1);
        app.world_mut().resource_mut::<LimitTask>().0 = Some((100, 50));
        send(&mut app, 3);
        assert_eq!(app.world().resource::<LimitHits>().marker, 3);
        app.world_mut().write_message(crate::combat::IneffectiveHit { attacker: b, target: a, atkp: 1, point: Vec3::ZERO });
        send(&mut app, 1);
        assert_eq!(app.world().resource::<LimitHits>().marker, 3, "an ineffective hit adds nothing");
        let mut p = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        app.world().resource::<LimitHits>().finish(&mut p);
        assert_eq!(p.limit_best(4), 3);
    }

    /// `FUN_001b2c10` / `FUN_001b2e08` / `FUN_001b3118`: the type 3 script gets event 22 first; without a type 0x14 entry the first
    /// participant above 1 gets it too and becomes the receiver of event 23.
    #[test]
    fn start_events_follow_the_bar_parts() {
        let summon = row([1, 201, 0], 0, 50, None);
        let partner = row([5, 1, 0], 0, 255, None);
        let script_only = LimitBar { script: true, party_set: false };
        let both = LimitBar { script: true, party_set: true };
        let none = LimitBar::default();
        let task = |to| LimitScriptEvent { event: 22, to, arg: LimitArg::Task };
        assert_eq!(start_events(&script_only, &summon), vec![task(LimitReceiver::LimitScript), task(LimitReceiver::Participant(201))]);
        assert_eq!(event_receiver(&script_only, &summon), Some(LimitReceiver::Participant(201)), "FUN_001b3118 overwrites task +0x34");
        assert_eq!(start_events(&both, &partner), vec![task(LimitReceiver::LimitScript)]);
        assert_eq!(event_receiver(&both, &partner), Some(LimitReceiver::LimitScript));
        assert_eq!(start_events(&none, &partner), vec![task(LimitReceiver::Participant(5))]);
        assert!(start_events(&none, &row([1, 0, 0], 0, 0, None)).is_empty(), "Sora alone: no participant above 1");
        assert_eq!(event_receiver(&none, &row([1, 0, 0], 0, 0, None)), None);
        assert_eq!(first_participant(&row([1, 3, 2], 0, 0, None)), Some(3));
        assert_eq!(LimitBar::from_kinds(&[3, 20, 18]), both);
        assert_eq!(LimitBar::from_kinds(&[18, 24]), none);
    }

    /// Event 23 goes to the running limit's thread with the command id as its only argument; no limit, no thread, no event.
    #[test]
    fn sub_commands_become_event_23_for_the_running_limit() {
        assert_eq!(sub_command_event(None, 525), None);
        assert_eq!(
            sub_command_event(Some(LimitReceiver::Participant(201)), 525),
            Some(LimitScriptEvent { event: 23, to: LimitReceiver::Participant(201), arg: LimitArg::Command(525) })
        );
    }

    /// The real limit files: every non-summon limit has the type 0x14 entry and a type 3 script, so its event 22 goes to its own script;
    /// the summon limits have no type 0x14 entry, so the summon's script gets event 22 and later event 23. The Execute 81 commands
    /// without a `limt` row are the sub-commands (Stitch's 525 to 527, Never Land's 469).
    #[test]
    fn real_limit_bars_and_sub_commands() {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let d = Disc::open(&iso);
        let (rows, subs) = {
            let mut a = d.0.lock().unwrap();
            let limt = kh2_tables::load::<t::Limt>(&mut a, &t::LIMT).unwrap().rows;
            let cmds = kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).unwrap().rows;
            let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).unwrap().rows;
            (rows(&limt, &cmds, &items), sub_commands(&limt, &cmds))
        };
        for r in &rows {
            let bytes = d.read(&format!("limit/us/{}", r.file)).unwrap_or_else(|| panic!("limit/us/{} missing", r.file));
            let kinds: Vec<u16> = kh2_bar::parse(&bytes).unwrap().iter().map(|e| e.kind).collect();
            let bar = LimitBar::from_kinds(&kinds);
            eprintln!("LIMIT id {} cmd {} {} summon {} bar {:?} events {:?} receiver {:?}", r.id, r.command, r.file, r.is_summon(), bar, start_events(&bar, r), event_receiver(&bar, r));
            assert_eq!(bar.script, !r.is_summon(), "{}: only the partner limits have the type 3 script", r.file);
            assert_eq!(bar.party_set, !r.is_summon(), "{}: the type 0x14 entry marks the partner limits", r.file);
        }
        eprintln!("SUB-COMMANDS {subs:?}");
        for c in [469u16, 525, 526, 527] {
            assert!(subs.contains(&c), "{c} is an Execute 81 command without a limit row");
        }
        assert!(rows.iter().all(|r| !subs.contains(&r.command)));
    }

    fn partner(member: u8, x: f32, items: &[u16]) -> Partner {
        Partner { member, pos: Vec3::new(x, 7.0, 0.0), items: items.to_vec() }
    }

    fn donald_row(command: u16, item: u16, bit: u16) -> LimitRow {
        LimitRow { id: command as u8, members: [2, 0, 0], file: "x.bar".into(), command, item, world: 0, cost: 255, time: 10, message: 0, ability_bit: Some(bit) }
    }

    fn base(present: &[u8], mp: i32) -> Party {
        Party { present: present.to_vec(), world: 2, mp, ..Default::default() }
    }

    /// `FUN_001d25e0`'s range test: the horizontal distance (y is zeroed) is compared with 500.0 (`DAT_0037c414`) with `<=`.
    #[test]
    fn auto_limit_needs_the_partner_within_500_horizontally() {
        let rows = vec![donald_row(300, 40, 5)];
        let items = [AUTO_LIMIT_ITEM, 40];
        let ok = |x: f32, y: f32| {
            let mut p = partner(2, x, &items);
            p.pos.y = y;
            !auto_limit_candidates(&rows, &[p], Vec3::ZERO, &base(&[2], 10), |_| true).is_empty()
        };
        assert!(ok(500.0, 0.0), "exactly 500.0 still qualifies");
        assert!(!ok(500.5, 0.0));
        assert!(ok(499.0, 9000.0), "the height is not part of the distance");
        assert!(ok(-300.0, 0.0), "either side");
    }

    /// The gates of the candidate walk: Auto Limit in the partner's own block, the limit item in it, the partner present and living,
    /// `Character` not 1, MP (status 2 stays a candidate), the command flags (status 5 is not).
    #[test]
    fn auto_limit_offer_gating() {
        let rows = vec![donald_row(300, 40, 5), LimitRow { members: [1, 0, 0], ..donald_row(301, 40, 5) }];
        let run = |items: &[u16], party: &Party, flags: bool| auto_limit_candidates(&rows, &[partner(2, 10.0, items)], Vec3::ZERO, party, |_| flags);
        let full = [AUTO_LIMIT_ITEM, 40];
        let got = run(&full, &base(&[2], 10), true);
        assert_eq!(got, vec![AutoCandidate { row: 0, command: 300, member: 2, no_mp: false }], "row 1 names Sora: never offered");
        assert!(run(&[40], &base(&[2], 10), true).is_empty(), "no Auto Limit");
        assert!(run(&[AUTO_LIMIT_ITEM], &base(&[2], 10), true).is_empty(), "the limit ability itself is missing");
        assert!(run(&full, &base(&[3], 10), true).is_empty(), "Donald is not in the party (or is down)");
        assert!(run(&full, &base(&[2], 10), false).is_empty(), "the command's flags refuse it");
        let poor = run(&full, &base(&[2], 0), true);
        assert_eq!(poor.len(), 1);
        assert!(poor[0].no_mp, "status 2 is below 4: still a candidate, but not acceptable");
        let mut f6 = base(&[2], 10);
        f6.form = 6;
        assert!(run(&full, &f6, true).is_empty(), "member 2 is refused in form field 6");
        assert!(auto_limit_candidates(&rows, &[], Vec3::ZERO, &base(&[2], 10), |_| true).is_empty(), "no partner object");
    }

    /// The pick: the wanted command wins without a draw; otherwise one step of the generator and `word / ceil(2^32 / n)`.
    /// Literal values: 0x2545f491 * 0x10dcd = 0x5b01351d, which falls in bucket 1 of 3, 1 of 4 and 2 of 7.
    #[test]
    fn auto_limit_pick_rule_with_a_fixed_seed() {
        let cmds = [10u16, 20, 30, 40];
        let mut seed = 0x2545_f491u32;
        assert_eq!(auto_limit_pick(&cmds, 30, &mut seed), Some(2), "the wanted command wins at once");
        assert_eq!(seed, 0x2545_f491, "and does not step the generator");
        assert_eq!(auto_limit_pick(&cmds, 0, &mut seed), Some(1));
        assert_eq!(seed, 0x5b01_351d);
        let mut seed = 0x2545_f491u32;
        assert_eq!(auto_limit_pick(&cmds[..3], 0, &mut seed), Some(1));
        let mut seed = 0x2545_f491u32;
        assert_eq!(auto_limit_pick(&[1, 2, 3, 4, 5, 6, 7], 0, &mut seed), Some(2));
        let mut seed = 0xdead_beefu32;
        assert_eq!(auto_limit_pick(&cmds, 0, &mut seed), Some(3), "0xe2c30863 is in the top quarter");
        let mut seed = 5;
        assert_eq!(auto_limit_pick(&[], 0, &mut seed), None);
        assert_eq!(seed, 5, "no candidate: no draw");
        assert_eq!(auto_limit_pick(&[77], 0, &mut seed), Some(0));
        assert_eq!(seed, 5u32.wrapping_mul(0x10dcd), "one candidate still draws once");
        for s0 in [1u32, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff] {
            for n in 1..=9usize {
                let mut s = s0;
                let i = auto_limit_pick(&vec![0u16; n], 1, &mut s).unwrap();
                assert!(i < n, "seed {s0:#x} n {n}: index {i}");
            }
        }
    }

    /// The pick against the game: the two calls of `FUN_001d25e0` (`FUN_00325b38((n - 1) + 2^32, n)`, then `FUN_00325b38(word, step)`)
    /// run in kh2-ee for many generator states and candidate counts; the index, and the generator step, equal the port's.
    #[test]
    fn auto_limit_pick_matches_the_games_divide() {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable");
        let mut ee = kh2_ee::Ee::new();
        ee.load_elf(&elf).expect("elf");
        let stack = 0x01ff_0000;
        let mut seeds: Vec<u32> = (0..200u32).map(|i| i.wrapping_mul(0x0083_1f27).wrapping_add(0x1234_5678)).collect();
        seeds.extend([1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff, 0x2545_f491]);
        let mut checked = 0;
        for &s0 in &seeds {
            for n in 1..=8u64 {
                let word = s0.wrapping_mul(0x10dcd);
                // The call site: `dsll32` / `dsrl32` zero-extend the word (0x1d27a0, 0x1d27a4); the step's operands are `(n - 1) | 1 << 32` and `n`.
                let step = ee.call(0x325b38, &[(n - 1) + (1 << 32), n], &[], stack).expect("step");
                let game = ee.call(0x325b38, &[u64::from(word), step], &[], stack).expect("index");
                let mut seed = s0;
                let port = auto_limit_pick(&vec![0u16; n as usize], 1, &mut seed).unwrap();
                assert_eq!(seed, word);
                assert_eq!(port as u64, game, "seed {s0:#x} n {n}");
                checked += 1;
            }
        }
        eprintln!("auto limit pick: {checked} cases equal the game's divide");
    }

    /// The gates of the offer and of the start: Sora down, a summon's arrival or leave (`0x35feb8` non-zero with `DAT_01d5e368`
    /// not 2), the menu and an event. An active summon (state 2) is not a gate of `FUN_001d2460`.
    #[test]
    fn start_gates_refuse_down_menu_event_and_summon_sequence() {
        use bevy::ecs::system::RunSystemOnce;
        let read = |w: &mut World| w.run_system_once(|g: GateParams| gates_of(&g, false)).unwrap();
        let mut w = World::new();
        assert_eq!(read(&mut w).check(), Ok(()), "no resources: nothing gates");
        w.init_resource::<crate::menu::Menu>();
        w.init_resource::<crate::intro::event_player::EventPlayer>();
        w.init_resource::<crate::summon::SummonState>();
        assert_eq!(read(&mut w).check(), Ok(()), "closed menu, no event, no summon");
        w.resource_mut::<crate::menu::Menu>().phase = crate::menu::Phase::Closing;
        assert_eq!(read(&mut w).check(), Err(Gate::MenuOpen));
        w.resource_mut::<crate::menu::Menu>().phase = crate::menu::Phase::Closed;
        {
            let mut sm = w.resource_mut::<crate::summon::SummonState>();
            sm.sequence = true;
            sm.state = 1;
        }
        assert_eq!(read(&mut w).check(), Err(Gate::SummonSequence), "the arrival");
        w.resource_mut::<crate::summon::SummonState>().state = 3;
        assert_eq!(read(&mut w).check(), Err(Gate::SummonSequence), "the leave");
        w.resource_mut::<crate::summon::SummonState>().state = 2;
        assert_eq!(read(&mut w).check(), Ok(()), "an active summon: FUN_001d2460 exempts state 2");
        assert_eq!(StartGates { sora_down: true, ..Default::default() }.check(), Err(Gate::SoraDown));
        assert_eq!(StartGates { event: true, ..Default::default() }.check(), Err(Gate::EventPlaying));
        assert_eq!(StartGates { sora_down: true, menu_open: true, ..Default::default() }.check(), Err(Gate::SoraDown), "KO first");
    }

    /// `FUN_001b3e38` answers 5 while a limit task exists or `0x35fec0` is set: not a candidate, not startable.
    #[test]
    fn a_busy_party_is_not_available() {
        let r = row([2, 1, 0], 0, 255, None);
        let mut p = party(&[2], 0, 10);
        assert_eq!(available(&r, &p), Ok(()));
        p.busy = true;
        assert_eq!(available(&r, &p), Err(Refusal::Busy));
        let rows = vec![donald_row(300, 40, 5)];
        let mut b = base(&[2], 10);
        b.busy = true;
        let items = [AUTO_LIMIT_ITEM, 40];
        assert!(auto_limit_candidates(&rows, &[partner(2, 10.0, &items)], Vec3::ZERO, &b, |_| true).is_empty());
    }

    fn row(members: [u8; 3], world: u16, cost: u8, bit: Option<u16>) -> LimitRow {
        LimitRow { id: 1, members, file: "x.bar".into(), command: 100, item: 0, world, cost, time: 10, message: 0, ability_bit: bit }
    }

    fn party(present: &[u8], abilities: u128, mp: i32) -> Party {
        Party { present: present.to_vec(), abilities, world: 2, mp, ..Default::default() }
    }

    #[test]
    fn members_world_ability_and_mp() {
        let r = row([2, 3, 0], 0, 50, Some(7));
        assert_eq!(usable(&r, &party(&[], 1 << 7, 100)), Err(Refusal::MemberAbsent(2)));
        assert_eq!(usable(&r, &party(&[2], 1 << 7, 100)), Err(Refusal::MemberAbsent(3)));
        assert_eq!(usable(&r, &party(&[2, 3], 0, 100)), Err(Refusal::AbilityMissing(7)));
        assert_eq!(available(&r, &party(&[2, 3], 1 << 7, 49)), Err(Refusal::NotEnoughMp));
        assert_eq!(available(&r, &party(&[2, 3], 1 << 7, 50)), Ok(()));
        let w = row([2, 0, 0], 5, 0, None);
        assert_eq!(usable(&w, &party(&[2], 0, 0)), Err(Refusal::WrongWorld));
        let mut locked = party(&[2], 0, 0);
        locked.locked = true;
        assert_eq!(usable(&row([2, 0, 0], 0, 0, None), &locked), Err(Refusal::Locked));
        let mut f6 = party(&[2], 0, 0);
        f6.form = 6;
        assert_eq!(usable(&row([2, 0, 0], 0, 0, None), &f6), Err(Refusal::MemberAbsent(2)));
    }

    #[test]
    fn all_mp_cost_needs_some_mp() {
        let r = row([0, 0, 0], 0, 255, None);
        assert_eq!(available(&r, &party(&[], 0, 0)), Err(Refusal::NotEnoughMp));
        assert_eq!(available(&r, &party(&[], 0, 1)), Ok(()));
    }

    #[test]
    fn summon_limits_need_the_gauge_not_a_partner() {
        let r = row([0, 201, 0], 0, 0, None);
        let mut p = party(&[], 0, 0);
        // FUN_00175ba0: the summon object of NeoStatus 201 must be present.
        assert_eq!(available(&r, &p), Err(Refusal::MemberAbsent(201)));
        p.summons = vec![200];
        assert_eq!(available(&r, &p), Err(Refusal::MemberAbsent(201)));
        p.summons = vec![201];
        assert_eq!(available(&r, &p), Err(Refusal::SummonGaugeEmpty));
        p.summon_gauge = true;
        assert_eq!(available(&r, &p), Ok(()));
    }

    fn disc_rows() -> Option<Vec<LimitRow>> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        let d = Disc::open(&iso);
        let mut a = d.0.lock().ok()?;
        let limt = kh2_tables::load::<t::Limt>(&mut a, &t::LIMT).ok()?.rows;
        let cmds = kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok()?.rows;
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).ok()?.rows;
        Some(rows(&limt, &cmds, &items))
    }

    /// Real rows: Sora alone can use none of the normal limits; the table is non-empty and every
    /// row's file exists in the limit directory.
    #[test]
    fn real_rows_need_companions() {
        let Some(rows) = disc_rows() else { return };
        assert!(!rows.is_empty());
        let alone = party(&[], u128::MAX, 100);
        for r in &rows {
            eprintln!("LIMIT id {} members {:?} cmd {} item {} bit {:?} world {} cost {} time {} file {} alone {:?}", r.id, r.members, r.command, r.item, r.ability_bit, r.world, r.cost, r.time, r.file, usable(r, &alone));
        }
        // Every row that names a partner is refused for Sora alone; the one row that needs nobody is the
        // Trinity variant with no partner bytes (file trinity_zz.bar, command 620).
        for r in rows.iter().filter(|r| !r.is_summon()) {
            let alone_ok = usable(r, &alone).is_ok();
            assert_eq!(alone_ok, r.members[0] == 1 && r.members[1] == 0 && r.members[2] == 0, "limit {} command {}", r.id, r.command);
        }
    }
}
