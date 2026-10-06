//! The Struggle (WP-7E): the game's mission manager (`DAT_01d5dd20`) reduced to what the Struggle matches of
//! tt04 use: the two orb counters, the match clock, the mission script that ties them together, and the end.
//! See docs/struggle.md for the rules and every address.
//!
//! What runs here, in the game's terms:
//! - the mission file `msn/us/<name>.bar` (loaded by `FUN_001b8d80`): its header records (`FUN_001d8d18` with
//!   `FUN_001b9378`) make the counters (opcode 5) and the clock record (opcode 4); its BDX entry (type 3) is the
//!   mission script, run as a system script (`FUN_001954f8`: event 2 now, event 3 as its thread) that gets every
//!   global event as event 10 with (event, argument) (`FUN_00197b70` -> `FUN_00195718` -> `FUN_00195690`);
//! - the orbs: every reaction a fighter takes knocks orbs out of its counter (`FUN_001caeb0`, the end of the
//!   reaction dispatcher `FUN_00184e80`, which the player's method +0x68 `0x179078` also reaches) into the orb
//!   pool of `PRIZE_TT` (`DAT_003600c0`), whose pickup (`FUN_001cadd8`) adds one to the picker's counter;
//! - the end: the script reacts to a full counter (global event 0x7b) or the clock's end (0x4a) and sends the
//!   result as global event 159 (the player won) or 160 (he lost) through host 1.41.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use kh2_bdx::{Call, Host, Program, Runtime, Value};

use crate::assets::Disc;
use crate::combat::{Hit, ReactionData, ReactionStatus};
use crate::data::GameData;
use crate::enemy::Enemy;
use crate::enemy_ai::AiHit;
use crate::prize::{self, Consts, Phase, Prize};
use crate::reaction_rules as rules;
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::{Sora, State};
use crate::world::World;
use crate::SCALE;

/// `00objentry` (ObjectType, SubType) of the Struggle orb pool object `PRIZE_TT` (`FUN_001c0400` case 8 ->
/// `FUN_001cac70`, which sets `DAT_003600c0`).
pub const ORB_POOL_TYPE: (u8, u8) = (22, 8);
/// The orb kinds of the pool (table 0x37c238, 16 bytes each, read by `FUN_001cad50`).
const ORB_KINDS: u32 = 0x37c238;
/// Global events the Struggle script sends through host 1.41 (`FUN_001727b0` -> `FUN_00197b70`).
pub const EV_PLAYER_WON: u32 = 159;
pub const EV_PLAYER_LOST: u32 = 160;

/// A btl program's mission command (area-script opcode 0x15, `FUN_00180b48` -> `FUN_001b9338(id, name)`).
pub fn mission_command(p: &kh2_ard::Program) -> Option<(u32, String)> {
    let c = p.commands.iter().find(|c| c.opcode == 0x15)?;
    let id = *c.args.first()?;
    let name: String = c.args[1..].iter().flat_map(|w| w.to_le_bytes()).take_while(|&b| b != 0).map(|b| b as char).collect();
    Some((id, name))
}

pub struct StrugglePlugin;

impl Plugin for StrugglePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MissionStart>().add_message::<GlobalEvent>().init_resource::<Mission>();
        let consts = app.world().get_resource::<Disc>().and_then(|d| d.read_root("SLUS_210.05")).and_then(|elf| orb_consts(&elf));
        if consts.is_none() {
            warn!("struggle: the orb constants are not readable from SLUS_210.05; no orb pool");
        }
        app.insert_resource(OrbLib(consts)).add_systems(
            FixedUpdate,
            (
                start_mission.in_set(Sim::Enemy).after(crate::enemy::spawn_waves),
                sora_orb_loss.in_set(Sim::SoraHurt).before(crate::sora::receive_hits),
                (enemy_orb_loss, mission_tick, run_orbs).chain().in_set(Sim::Effects),
            ),
        );
    }
}

/// The wave spawner started a group whose program has a mission command.
#[derive(Message, Clone, Debug)]
pub struct MissionStart {
    pub id: u32,
    pub name: String,
    pub orb_pool: bool,
}

/// A global event (`FUN_00197b70(event, argument)`): besides the mission script it goes to every object
/// (`FUN_0016b470`, object method +0x34); the enemy scripts get it as event 10 (`enemy_ai`).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalEvent {
    pub id: u32,
    pub arg: i32,
}

// ---------------------------------------------------------------------------------------------------
// The mission file

/// One header record (`u16 opcode, u16 count, count words`; the list starts at +0x1c and ends at opcode 0).
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub op: u16,
    pub args: Vec<u32>,
}

/// The mission header (BAR entry type 2 of the mission file, `FUN_001b8d80` keeps it at manager +8).
#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    /// +2: the mission id (the opcode-0x15 argument).
    pub id: u16,
    /// +4: flags (`FUN_001b8d80`: 0x40 sets bit 0x200 of `DAT_0035d268`, no prizes; 0x80 bit 0x400, no items).
    pub flags: u16,
    pub records: Vec<Record>,
}

pub fn parse_header(d: &[u8]) -> Option<Header> {
    let u16_at = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let mut records = Vec::new();
    let mut p = 0x1c;
    while let (Some(op), Some(n)) = (u16_at(p), u16_at(p + 2)) {
        if op == 0 {
            break;
        }
        let args = (0..n as usize).map(|k| d.get(p + 4 + 4 * k..p + 8 + 4 * k).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))).collect::<Option<Vec<_>>>()?;
        records.push(Record { op, args });
        p += 4 + 4 * n as usize;
    }
    Some(Header { id: u16_at(2)?, flags: u16_at(4)?, records })
}

/// A HUD element record (opcodes 4 to 9, `FUN_001d6110`): words +4, +8, +0xc go to the element's init
/// method (vtable +0x14), +0x10 is the 1-based BAR index of its picture (the image; the sequence follows,
/// `FUN_001d3390`), +0x12 its screen side (+0x20 of the element, the HUD's group choice), +0x13 its slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Element {
    pub op: u16,
    pub start: i32,
    pub max_word: u32,
    pub third: i32,
    pub picture: u16,
    pub side: u8,
    pub slot: u8,
}

impl Element {
    pub fn from_record(r: &Record) -> Option<Element> {
        let w = *r.args.get(3)?;
        Some(Element { op: r.op, start: r.args[0] as i32, max_word: r.args[1], third: r.args[2] as i32, picture: w as u16, side: (w >> 16) as u8, slot: (w >> 24) as u8 })
    }

    /// The BAR entries (image, sequence) of its picture (`FUN_001d3390(.., index - 1)`), none for index 0.
    pub fn picture_entries(&self) -> Option<(usize, usize)> {
        (self.picture > 0).then(|| (self.picture as usize - 1, self.picture as usize))
    }
}

// ---------------------------------------------------------------------------------------------------
// Orbs (FUN_001caeb0, FUN_001cad50, FUN_001cadd8)

/// Orbs a reaction knocks out of the victim's counter (`FUN_001caeb0`): by the reaction's class
/// (`FUN_0018b700`): 3 for classes 0, 2 and 3, 15 for 4, 5 and 14, none otherwise; never more than the counter holds.
pub fn orbs_knocked_out(reaction: u8, counter: i32) -> i32 {
    let n = match rules::reaction_class(reaction) {
        0 | 2 | 3 => 3,
        4 | 5 | 14 => 15,
        _ => 0,
    };
    n.min(counter)
}

/// The counter a fighter's orbs belong to: the player's (+0x588 bit 0x80) is slot 0, any other's slot 1
/// (`FUN_001caeb0` and `FUN_001cadd8` both take `(+0x588 & 0x80) == 0` as the slot).
pub fn side_of(is_player: bool) -> usize {
    usize::from(!is_player)
}

/// The orb kind a victim's orbs take (`FUN_001caeb0`: 0 for the player, 2 for any other).
pub fn orb_kind(is_player: bool) -> u8 {
    if is_player { 0 } else { 2 }
}

/// The prize constants with the orb kinds of table 0x37c238 in place of the prize kinds.
pub fn orb_consts(elf: &[u8]) -> Option<Consts> {
    let mut c = Consts::from_exe(elf)?;
    let mut entries = Vec::new();
    for k in 0..4u32 {
        let e = ORB_KINDS + 16 * k;
        let w = |o: u32| {
            let b = exe_bytes(elf, e + o)?;
            Some(u32::from_le_bytes(b))
        };
        entries.push(prize::KindEntry { id: w(0)? as u8, radius: f32::from_bits(w(4)?), bounce_se: w(8)?, pickup_se: w(12)? });
    }
    for (k, e) in entries.into_iter().enumerate() {
        c.kinds[k] = e;
    }
    Some(c)
}

fn exe_bytes(elf: &[u8], va: u32) -> Option<[u8; 4]> {
    let u16_at = |o: usize| elf.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |o: usize| elf.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let (phoff, phentsize, phnum) = (u32_at(0x1c)? as usize, u16_at(0x2a)?, u16_at(0x2c)?);
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        let (kind, off, vaddr, filesz) = (u32_at(p)?, u32_at(p + 4)?, u32_at(p + 8)?, u32_at(p + 16)?);
        if kind == 1 && va >= vaddr && va - vaddr + 4 <= filesz {
            let s = off as usize + (va - vaddr) as usize;
            return elf.get(s..s + 4).and_then(|b| b.try_into().ok());
        }
    }
    None
}

/// The orb constants (None: the executable was not readable).
#[derive(Resource)]
pub struct OrbLib(pub Option<Consts>);

/// One orb of the pool: a prize entry (flag bit 4 set by `FUN_001cad50`: its life does not run down) and,
/// while it is drawn in, the fighter taking it.
#[derive(Clone, Debug)]
pub struct Orb {
    pub p: Prize,
    pub taker: Option<Entity>,
}

/// A fighter that may take orbs (`FUN_00184d98`: the objects with +0x588 bit 2), in object-list order.
#[derive(Clone, Copy, Debug)]
pub struct Taker {
    pub ent: Entity,
    pub player: bool,
    /// Game space (cm, Y down, Z mirrored): the position (`FUN_0016b960`) and the centre (`FUN_0016b7d0`).
    pub feet: Vec3,
    pub centre: Vec3,
    /// The Draw ability's reach added for the player (+0x588 bit 4).
    pub extra: f32,
}

/// One frame of an orb: the prize update without the life count; at the pickup check the first fighter in
/// reach takes it (`FUN_001cadd8`, which returns after the first). Returns (alive, bounce sound, taker side).
pub fn orb_tick(o: &mut Orb, c: &Consts, sweep: impl Fn(Vec3, Vec3) -> prize::Sweep, takers: &[Taker]) -> (bool, Option<u32>, Option<usize>) {
    if o.p.phase == Phase::Absorb {
        let centre = takers.iter().find(|t| Some(t.ent) == o.taker).map(|t| t.centre);
        return (centre.is_some_and(|ce| prize::absorb_step(&mut o.p, c, ce)), None, None);
    }
    let mut bounce = None;
    if o.p.phase == Phase::Fall {
        bounce = prize::fall_physics(&mut o.p, c, &sweep);
    }
    let mut side = None;
    if prize::shrink_or_check(&mut o.p, c, &sweep) && o.p.pickable {
        if let Some(t) = takers.iter().find(|t| prize::in_reach(c, &o.p, t.feet, t.extra)) {
            prize::start_absorb(&mut o.p, c, t.centre);
            o.taker = Some(t.ent);
            side = Some(side_of(t.player));
        }
    }
    (true, bounce, side)
}

// ---------------------------------------------------------------------------------------------------
// The mission state

/// The mission manager (`DAT_01d5dd20`, made by `FUN_001b9338` -> `FUN_001b8b10`), with what the app uses.
#[derive(Resource, Default)]
pub struct Mission {
    pub m: Option<Box<Active>>,
}

pub struct Active {
    pub id: u32,
    pub name: String,
    pub header: Header,
    /// The mission file (its pictures are read by `hud_mission`).
    pub file: Arc<Vec<u8>>,
    pub elements: Vec<Element>,
    /// +0xe8: the three counters.
    pub counters: [Counter; 3],
    /// The clock record (opcode 4, +0xbc) and the global clock it drives.
    pub clock_rec: Option<Element>,
    pub clock: Clock,
    /// +4 bit 0: the HUD elements are shown (`FUN_001d5f00`); bit 2: stopped (`FUN_001b9940`, which
    /// `FUN_001b9560` tests for every counter change).
    pub shown: bool,
    pub stopped: bool,
    /// The latch at 0x35fec8 (hosts 4.54 set, 4.55 test, `FUN_001b99a0` clears).
    pub latch: bool,
    pub orb_pool: bool,
    pub orbs: Vec<Orb>,
    /// The generator the orb launches step (the game shares 0x3716a0 with every other user; here it is the
    /// pool's own [approx]).
    pub seed: u32,
    rt: Option<Runtime>,
    /// Global events waiting for delivery.
    pub events: Vec<(u32, i32)>,
    /// Story flags (hosts 0.14 / 0.15) and record bits (0.69 / 0.89, the array at 0x33fdb0) the script set this
    /// session: the app has no save data, all start clear.
    pub story: HashSet<i32>,
    pub records: HashSet<i32>,
    /// Host 4.42's tallies (`FUN_001d7560`: bytes at `PTR_DAT_0037c5f0` + index * 3 + which, held at 99).
    pub tallies: HashMap<(i32, i32), u8>,
    /// How the match ended (`time`, `win`, `lose`) and its result, once known.
    pub end_cause: Option<&'static str>,
    pub result: Option<&'static str>,
}

impl Active {
    /// Host 4.29 (`FUN_001d6738`): the value of counter `i`, 0 when it does not exist.
    pub fn counter_value(&self, i: usize) -> i32 {
        self.counters.get(i).filter(|c| c.active).map_or(0, |c| c.value)
    }

    /// `FUN_001b9560`: the manager exists and is not stopped (every counter change asks it).
    fn running(&self) -> bool {
        !self.stopped
    }

    /// A counter change through the hosts (`FUN_001d6598` 4.5, `FUN_001d6620` 4.41, 4.6 / 4.20, 4.48).
    fn change(&mut self, i: usize, f: impl FnOnce(&mut Counter) -> Option<(u32, i32)>) {
        if !self.running() {
            return;
        }
        let Some(c) = self.counters.get_mut(i).filter(|c| c.active) else { return };
        if let Some((ev, slot)) = f(c) {
            self.events.push((ev, slot));
        }
    }

    /// `FUN_001b9940`: the mission stops (flags 4 and 0x10, the clock stops through `FUN_0014c790`).
    fn stop(&mut self) {
        self.stopped = true;
        self.clock.stop();
    }

    /// No prize objects while the header's flag 0x40 holds (`FUN_001b8d80` sets 0x200 of `DAT_0035d268`;
    /// `FUN_001a3cb0` then drops nothing unless the player's +0x588 bit 0x2000000 is set, which the app never sets).
    pub fn no_prizes(&self) -> bool {
        self.header.flags & 0x40 != 0
    }
}

impl Mission {
    /// The counters as host 4.29 sees them, None without a manager.
    pub fn counters(&self) -> Option<[i32; 3]> {
        self.m.as_ref().map(|a| [0, 1, 2].map(|i| a.counter_value(i)))
    }

    pub fn no_prizes(&self) -> bool {
        self.m.as_ref().is_some_and(|a| a.no_prizes())
    }
}

/// `FUN_001b8b10` + `FUN_001b8d80` + `FUN_001b8ee8` for a mission file: the header, its HUD elements, the
/// counters with their start values, the script.
pub fn build(id: u32, name: &str, file: Vec<u8>, orb_pool: bool, table: Option<&kh2_bdx::HostTable>) -> Result<Active, String> {
    let entries = kh2_bar::parse(&file).map_err(|e| format!("{name}: {e:?}"))?;
    let header = entries.iter().find(|e| e.kind == 2).and_then(|e| parse_header(e.data)).ok_or_else(|| format!("{name}: no header (BAR type 2)"))?;
    let rt = match entries.iter().find(|e| e.kind == 3) {
        Some(e) => {
            let prog = Program::parse(e.data).map_err(|err| format!("{name}: script: {err}"))?;
            let mut rt = Runtime::new(prog);
            if let Some(t) = table {
                rt.set_host_table(t.clone());
            }
            Some(rt)
        }
        None => None,
    };
    drop(entries);
    let mut counters: [Counter; 3] = Default::default();
    let mut clock_rec = None;
    let mut elements = Vec::new();
    for r in &header.records {
        let Some(e) = Element::from_record(r) else { continue };
        match r.op {
            // FUN_001d64d0: counter `slot` (method 0 = FUN_001cf870 -> FUN_001cf508(slot), method +0x14 = FUN_001cf8a0).
            5 => {
                if let Some(c) = counters.get_mut(e.slot as usize) {
                    c.active = true;
                    c.slot = e.slot as i32;
                    c.init(e.start, e.max_word);
                }
            }
            // FUN_001d61c8: the clock record at +0xbc (FUN_001cf528: start, maximum, third word).
            4 => clock_rec = Some(e),
            _ => {}
        }
        if (4..=9).contains(&r.op) {
            elements.push(e);
        }
    }
    Ok(Active {
        id,
        name: name.to_string(),
        header,
        file: Arc::new(file),
        elements,
        counters,
        clock_rec,
        clock: Clock::default(),
        shown: false,
        stopped: false,
        latch: false,
        orb_pool,
        orbs: Vec::new(),
        seed: 1,
        rt,
        events: Vec::new(),
        story: HashSet::new(),
        records: HashSet::new(),
        tallies: HashMap::new(),
        end_cause: None,
        result: None,
    })
}

// ---------------------------------------------------------------------------------------------------
// The mission script's host

/// The integer arguments of a call (named or not).
fn ints(c: &Call) -> Option<(u16, u16, Vec<i32>)> {
    match c {
        Call::Func { group, index, args } => Some((*group, *index, args.iter().map(|a| a.int()).collect())),
        Call::Unknown { group, index, args } => Some((*group, *index, args.iter().map(|&a| a as i32).collect())),
        _ => None,
    }
}

struct MissionHost<'a> {
    a: &'a mut Active,
    /// Calls the host does not know, for the log (once each).
    unknown: &'a mut HashSet<(u16, u16)>,
}

impl Host for MissionHost<'_> {
    fn call(&mut self, c: &Call) -> Value {
        let Some((g, i, args)) = ints(c) else {
            return Value::Int(0);
        };
        let arg = |k: usize| args.get(k).copied().unwrap_or(0);
        let a = &mut *self.a;
        match (g, i) {
            // Debug prints (empty in the retail build).
            (0, 0..=2) | (0, 8) | (0, 24..=26) => Value::None,
            // 0.14 / 0.15: story flags (FUN_0017ff60 / FUN_001802e0).
            (0, 14) => {
                a.story.insert(arg(0));
                Value::None
            }
            (0, 15) => Value::Bool(a.story.contains(&arg(0))),
            // 0.69 (0x16e088 -> FUN_001a52b0) / 0.89 (0x16e0b8 -> FUN_001a5328): a bit of the array at 0x33fdb0.
            (0, 69) => {
                a.records.insert(arg(0));
                Value::None
            }
            (0, 89) => Value::Bool(a.records.contains(&arg(0))),
            // 1.41 (0x1727b0): the global event (event, argument).
            (1, 41) => {
                a.events.push((arg(0) as u32, arg(1)));
                Value::None
            }
            // 1.317 (0x173b70): bit 0x20000 of DAT_0035d268 (meaning not traced).
            (1, 317) => Value::None,
            // 4.3 (FUN_001b9780), 4.16 (FUN_001b97f0), 4.11 (FUN_001b9818), 4.22 (0x19e910): the mission ends
            // with result event 7 (or 0x9c), 0x10, the failure path (+0xb4 = 2), or global event 0xf.
            (4, 3) | (4, 16) | (4, 11) | (4, 22) => {
                let what = match i {
                    3 => "event 7 (FUN_001b9780)",
                    16 => "event 0x10 (FUN_001b97f0)",
                    11 => "the failure path (FUN_001b9818)",
                    _ => "global event 0xf (0x19e910)",
                };
                info!("struggle: mission {} ends through host 4.{i}: {what}; result layouts and the area event are not in the app", a.name);
                a.stop();
                if i == 22 {
                    a.events.push((0xf, arg(0)));
                }
                Value::None
            }
            // 4.29 (FUN_001d6738): a counter's value.
            (4, 29) => Value::Int(a.counter_value(arg(0).max(0) as usize)),
            // 4.5 (FUN_001d6598): set; 4.41 (FUN_001d6620): add; 4.6 / 4.20: one up / down; 4.48: one digit.
            (4, 5) => {
                a.change(arg(1).max(0) as usize, |c| c.set(arg(0)));
                Value::None
            }
            (4, 41) => {
                a.change(arg(1).max(0) as usize, |c| c.add(arg(0)));
                Value::None
            }
            (4, 6) | (4, 20) => {
                a.change(arg(0).max(0) as usize, |c| c.step(i == 6));
                Value::None
            }
            (4, 48) => {
                a.change(arg(2).max(0) as usize, |c| c.set_digit(arg(0), arg(1)));
                Value::None
            }
            // 4.26 (FUN_001d6340): the clock's frames.
            (4, 26) => Value::Int(a.clock.shown),
            // 4.42 (FUN_001d7598 -> FUN_001d7560): a tally byte + 1, held at 99.
            (4, 42) => {
                let t = a.tallies.entry((arg(0), arg(1))).or_insert(0);
                if *t < 99 {
                    *t += 1;
                }
                Value::Int(*t as i32)
            }
            // 4.54 / 4.55 (0x19e9b0 / 0x19e9d0): the latch at 0x35fec8.
            (4, 54) => {
                a.latch = true;
                Value::None
            }
            (4, 55) => Value::Bool(a.latch),
            _ => {
                if self.unknown.insert((g, i)) {
                    warn!("struggle: mission script calls host {g}.{i}, which the app does not map (answered 0)");
                }
                Value::Int(0)
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Systems

/// Script table, unknown-call memory.
#[derive(Default)]
struct ScriptEnv {
    table: Option<kh2_bdx::HostTable>,
    unknown: HashSet<(u16, u16)>,
    loaded: bool,
}

fn start_mission(disc: Res<Disc>, mut starts: MessageReader<MissionStart>, mut mission: ResMut<Mission>, mut env: Local<ScriptEnv>) {
    let Some(s) = starts.read().last().cloned() else { return };
    // The same group again (the wave spawner repeats a forced group): the manager stays.
    if mission.m.as_ref().is_some_and(|a| a.id == s.id) {
        return;
    }
    if !env.loaded {
        env.loaded = true;
        env.table = disc.read_root("SLUS_210.05").and_then(|e| kh2_bdx::HostTable::from_exe(&e));
    }
    let path = format!("msn/us/{}.bar", s.name);
    let Some(file) = disc.read(&path) else {
        warn!("struggle: {path} is not on the disc; no mission");
        return;
    };
    match build(s.id, &s.name, file, s.orb_pool, env.table.as_ref()) {
        Ok(mut a) => {
            // FUN_001954f8: event 2 runs at once, event 3 becomes the script's thread.
            let mut rt = a.rt.take();
            if let Some(rt) = rt.as_mut() {
                let mut host = MissionHost { a: &mut a, unknown: &mut env.unknown };
                rt.event(&mut host, 2, &[]);
                rt.start_thread(0, 3, &[]);
            }
            a.rt = rt;
            info!(
                "struggle: mission {} ({:#x}) from {path}: flags {:#06x}, counters {:?}, clock {:?}, orb pool {}, script {}",
                a.name,
                a.id,
                a.header.flags,
                a.counters.iter().filter(|c| c.active).map(|c| format!("slot {} {}/{}", c.slot, c.value, c.max)).collect::<Vec<_>>(),
                a.clock_rec.map(|c| format!("{} s", c.start)),
                a.orb_pool,
                if a.rt.is_some() { "yes" } else { "no" }
            );
            mission.m = Some(Box::new(a));
        }
        Err(e) => warn!("struggle: {e}"),
    }
}

/// Orbs a reaction knocks out (`FUN_001caeb0`) into the pool, at the victim's centre.
fn knock_out(a: &mut Active, c: &Consts, player: bool, reaction: u8, centre_game: Vec3) -> i32 {
    if !a.orb_pool {
        return 0;
    }
    let side = side_of(player);
    let n = orbs_knocked_out(reaction, a.counter_value(side));
    if n <= 0 {
        return 0;
    }
    a.change(side, |k| k.add(-n));
    // FUN_001cad50: n orbs of the victim's kind, each launched by FUN_001a3b28 (three generator steps).
    for _ in 0..n {
        let mut v = [0u32; 3];
        for x in &mut v {
            a.seed = a.seed.wrapping_mul(0x10dcd);
            *x = a.seed;
        }
        a.orbs.push(Orb { p: prize::launch(c, orb_kind(player), v, centre_game), taker: None });
    }
    n
}

/// App space (Y up) and game space (Y down, Z mirrored).
fn flip(v: Vec3) -> Vec3 {
    Vec3::new(v.x, -v.y, -v.z)
}

/// The centre (`FUN_0016b7d0`: the body's first shape of kind 1) of a body, app cm.
fn centre_of(bodies: &Query<(&Animator, &GlobalTransform)>, body: Entity, feet: Vec3) -> Vec3 {
    bodies.get(body).ok().and_then(|(a, g)| prize::shape_point(a, g, 1)).unwrap_or(feet)
}

/// Hits on Sora: the reaction they give him (as `sora::receive_hits` works it out, which runs right after)
/// knocks his orbs out. Skipped where `receive_hits` skips the reaction (invulnerable, down, guarded).
#[allow(clippy::too_many_arguments)]
fn sora_orb_loss(
    mut hits: MessageReader<Hit>,
    data: Res<GameData>,
    reaction: Option<Res<ReactionData>>,
    lib: Res<OrbLib>,
    mut mission: ResMut<Mission>,
    sora: Query<(Entity, &Transform, &Sora, Option<&ReactionStatus>)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
) {
    let (Some(a), Some(reaction), Some(c)) = (mission.m.as_deref_mut(), reaction, lib.0.as_ref()) else {
        hits.clear();
        return;
    };
    let Ok((me, tf, s, status)) = sora.single() else { return };
    for h in hits.read().filter(|h| h.target == me) {
        if s.invuln > 0.0 || matches!(s.state, State::Down | State::Viewer | State::FormChange) {
            continue;
        }
        let from_front = s.motor.forward().dot(-h.dir) > 0.0;
        // [approx] the guard's repel window (the guard motion before its blend-to-idle range) is taken as the
        // whole guard state.
        if s.state == State::Guard && from_front {
            continue;
        }
        let Some(atk) = data.atkp(h.atkp) else { continue };
        let useless = crate::combat::hit_is_ineffective(atk, &s.stats);
        let ctx = rules::VictimCtx { status: status.map_or(0, |x| x.0), grounded: s.motor.grounded, attacker_flag_4000: false, from_front, hit_part_plain: true };
        let out = rules::victim_outcome(&rules::HitRow::from_atkp(atk), useless, &ctx, &reaction.patn, &reaction.defaults, &reaction.consts);
        let feet = tf.translation / SCALE;
        let centre = centre_of(&bodies, s.body, feet);
        let n = knock_out(a, c, true, out.effective, flip(centre));
        if n > 0 {
            info!("struggle: Sora hit (reaction {}): {n} orbs knocked out, player {} orbs, opponent {}", out.effective, a.counter_value(0), a.counter_value(1));
        }
    }
}

/// Hits on the opponents (the reaction `enemy::receive_hits` reports): their orbs are knocked out.
fn enemy_orb_loss(
    mut hits: MessageReader<AiHit>,
    lib: Res<OrbLib>,
    mut mission: ResMut<Mission>,
    enemies: Query<(&Transform, &Enemy)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
) {
    let (Some(a), Some(c)) = (mission.m.as_deref_mut(), lib.0.as_ref()) else {
        hits.clear();
        return;
    };
    for h in hits.read() {
        let Ok((tf, e)) = enemies.get(h.target) else { continue };
        let feet = tf.translation / SCALE;
        let centre = centre_of(&bodies, e.body, feet);
        let n = knock_out(a, c, false, h.reaction, flip(centre));
        if n > 0 {
            info!("struggle: {} hit (reaction {}): {n} orbs knocked out, player {} orbs, opponent {}", e.name, h.reaction, a.counter_value(0), a.counter_value(1));
        }
    }
}

/// The mission's frame: the HUD elements come up when the mission starts (`FUN_001b9140` state 0 ->
/// `FUN_001d5f00`; the clock record's show method `FUN_001cf748` starts the clock), the clock ticks, the
/// script runs, and the global events of the frame are delivered.
fn mission_tick(mut mission: ResMut<Mission>, mut out: MessageWriter<GlobalEvent>, mut unknown: Local<HashSet<(u16, u16)>>) {
    let Some(a) = mission.m.as_deref_mut() else { return };
    if !a.shown {
        a.shown = true;
        for c in a.counters.iter_mut().filter(|c| c.active) {
            c.shown = true;
        }
        if let Some(r) = a.clock_rec {
            let max = (r.max_word & 0xbfff_ffff) as i32;
            a.clock.begin(r.start, max);
            info!("struggle: clock starts: {} s{}", r.start, if r.start < 1 { " counting up" } else { " counting down" });
        }
        info!("struggle: mission HUD shown: player {} orbs, opponent {}", a.counter_value(0), a.counter_value(1));
    }
    if a.clock.tick() {
        a.events.push((EV_CLOCK_END, 0));
    }
    let mut rt = a.rt.take();
    if let Some(rt) = rt.as_mut() {
        let mut host = MissionHost { a, unknown: &mut unknown };
        rt.update_script(&mut host);
    }
    // Events raised while delivering are delivered in the same frame (the game's calls nest).
    let mut guard = 0;
    while !a.events.is_empty() && guard < 16 {
        guard += 1;
        for (id, arg) in std::mem::take(&mut a.events) {
            note_event(a, id, arg);
            if let Some(rt) = rt.as_mut() {
                let mut host = MissionHost { a, unknown: &mut unknown };
                rt.event(&mut host, 10, &[id, arg as u32]);
            }
            out.write(GlobalEvent { id, arg });
        }
    }
    a.rt = rt;
}

/// The log lines of the events the Struggle cares about.
fn note_event(a: &mut Active, id: u32, arg: i32) {
    let (p, o) = (a.counter_value(0), a.counter_value(1));
    let cause = match id {
        EV_COUNTER_FULL => Some(if arg == 0 { "win" } else { "lose" }),
        EV_CLOCK_END => Some("time"),
        _ => None,
    };
    if let Some(cause) = cause
        && a.end_cause.is_none()
    {
        a.end_cause = Some(cause);
        info!("struggle: end {cause}: player {p} orbs, opponent {o}");
    }
    match id {
        EV_COUNTER_EMPTY => info!("struggle: counter {arg} is empty (global event 0xe)"),
        EV_PLAYER_WON | EV_PLAYER_LOST => {
            let r = if id == EV_PLAYER_WON { "win" } else { "lose" };
            a.result = Some(r);
            info!("struggle: result {r} (global event {id}): player {p} orbs, opponent {o}");
        }
        _ => {}
    }
}

/// The orb pool's frame (`FUN_001c4d50` walk; update `FUN_001c5288` / rest, pickup `FUN_001cadd8`).
#[allow(clippy::too_many_arguments)]
fn run_orbs(
    lib: Res<OrbLib>,
    mut mission: ResMut<Mission>,
    world: Option<Res<World>>,
    abilities: Option<Res<crate::abilities::Abilities>>,
    sora: Query<(Entity, &Transform, &Sora)>,
    enemies: Query<(Entity, &Transform, &Enemy)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
    mut sounds: MessageWriter<crate::audio::PlaySe>,
) {
    let (Some(a), Some(c)) = (mission.m.as_deref_mut(), lib.0.as_ref()) else { return };
    if a.orbs.is_empty() {
        return;
    }
    let mut takers = Vec::new();
    if let Ok((ent, tf, s)) = sora.single()
        && s.hp > 0
    {
        let feet = tf.translation / SCALE;
        let extra = abilities.as_ref().map_or(0.0, |x| x.block().draw_radius);
        takers.push(Taker { ent, player: true, feet: flip(feet), centre: flip(centre_of(&bodies, s.body, feet)), extra });
    }
    for (ent, tf, e) in &enemies {
        if e.alive() {
            let feet = tf.translation / SCALE;
            takers.push(Taker { ent, player: false, feet: flip(feet), centre: flip(centre_of(&bodies, e.body, feet)), extra: 0.0 });
        }
    }
    let w = world.as_deref();
    let mut picked = Vec::new();
    let mut i = 0;
    while i < a.orbs.len() {
        let o = &mut a.orbs[i];
        let radius = c.kinds.get(o.p.kind as usize).map_or(0.0, |k| k.radius);
        let (alive, bounce, side) = orb_tick(o, c, |s, v| prize::sweep(w, radius, s, v), &takers);
        if let Some(se) = bounce {
            sounds.write(crate::audio::PlaySe::number(se, None).at(flip(o.p.pos) * SCALE));
        }
        if let Some(side) = side {
            picked.push((side, o.p.kind, o.taker));
        }
        if alive {
            i += 1;
        } else {
            a.orbs.swap_remove(i);
        }
    }
    for (side, kind, taker) in picked {
        // FUN_001c5808's sound at the taker's centre, then FUN_001d6620(1, side).
        if let (Some(se), Some(t)) = (c.kinds.get(kind as usize).map(|k| k.pickup_se), takers.iter().find(|t| Some(t.ent) == taker)) {
            sounds.write(crate::audio::PlaySe::number(se, None).at(flip(t.centre) * SCALE));
        }
        a.change(side, |k| k.add(1));
        if std::env::var("KH2_STRUGGLELOG").is_ok_and(|v| v != "0") {
            info!("struggle: orb taken by {}: player {} orbs, opponent {}", if side == 0 { "Sora" } else { "the opponent" }, a.counter_value(0), a.counter_value(1));
        }
    }
}

/// Global event a counter sends when it drops to 0 from above (`FUN_001cf9e8` -> `FUN_001cf5b0(.., 0xe)`).
pub const EV_COUNTER_EMPTY: u32 = 0xe;
/// Global event a counter sends when it reaches its maximum from below (`FUN_001cf9e8`, 0x7b).
pub const EV_COUNTER_FULL: u32 = 0x7b;
/// Global event the clock sends when it runs out (`FUN_0014c560`: `FUN_00197b70(0x4a, 0)`).
pub const EV_CLOCK_END: u32 = 0x4a;
/// Highest value of a clock that counts up (`FUN_0014c560`, 0x34bbf frames: one hour less a frame).
pub const CLOCK_UP_LIMIT: i32 = 0x34bbf;

/// One mission counter (class with vtable 0x360588, three of them at manager +0xe8, 0x38 bytes each).
/// Field names follow the object's words: +0xc flags, +0x10 slot, +0x14 start, +0x18 maximum, +0x2c value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Counter {
    /// +0xc bit 0: the counter exists (a header record of opcode 5 created it, `FUN_001cf508`).
    pub active: bool,
    /// +0xc bit 2: shown (its HUD is up, `FUN_001cf588`); the events below are sent only while it is set.
    pub shown: bool,
    /// +0x10: the slot passed with the events (-1: no events, `FUN_001cf5b0`).
    pub slot: i32,
    /// +0x14: the start value (`FUN_001cf528` param 2).
    pub start: i32,
    /// +0x18: the maximum (`FUN_001cf528` param 3 without bit 0x40000000).
    pub max: i32,
    /// +0xc bit 1: the HUD shows "value / maximum" (bit 0x40000000 of the maximum word).
    pub show_max: bool,
    /// +0x2c: the value.
    pub value: i32,
}

impl Counter {
    /// `FUN_001cf528` + `FUN_001cf8a0` (the record's init method, vtable +0x14): start value, maximum word
    /// (bit 0x40000000 = show the maximum; 0 leaves the maximum as it was), third word; value = start.
    pub fn init(&mut self, start: i32, max_word: u32) {
        self.start = start;
        if max_word != 0 {
            self.max = (max_word & 0xbfff_ffff) as i32;
            self.show_max = max_word & 0x4000_0000 != 0;
        }
        self.value = start;
    }

    /// `FUN_001cf9e8`: the value held in 0..=max; reaching 0 from above sends event 0xe, reaching the maximum
    /// from below sends 0x7b (both with the slot, only while shown and with a slot). Returns the event.
    pub fn set(&mut self, v: i32) -> Option<(u32, i32)> {
        let mut ev = None;
        let nv = if v < self.max {
            if v < 1 {
                if self.value >= 1 {
                    ev = Some(EV_COUNTER_EMPTY);
                }
                0
            } else {
                v
            }
        } else {
            if self.value < self.max {
                ev = Some(EV_COUNTER_FULL);
            }
            self.max
        };
        self.value = nv;
        ev.filter(|_| self.slot != -1 && self.shown).map(|e| (e, self.slot))
    }

    /// Host 4.41 (`FUN_001d6698`): value + n.
    pub fn add(&mut self, n: i32) -> Option<(u32, i32)> {
        self.set(self.value + n)
    }

    /// Host 4.6 / 4.20 (`FUN_0019eb68` / `FUN_0019eb88`): value + 1 / value - 1.
    pub fn step(&mut self, up: bool) -> Option<(u32, i32)> {
        self.add(if up { 1 } else { -1 })
    }

    /// Host 4.48 (`FUN_0019ec58`): replaces decimal digit `digit` (0 = units) of the value with `d`.
    pub fn set_digit(&mut self, digit: i32, d: i32) -> Option<(u32, i32)> {
        let p = 10i32.pow(digit.max(0) as u32);
        self.set(self.value + (d - (self.value / p) % 10) * p)
    }
}

/// The match clock (the global timer at 0x356f18..0x356f30, `FUN_0014c6b0`.. `FUN_0014c7f0`; ticked by
/// `FUN_0014c560`). Times in frames (60 per second).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clock {
    /// 0x356f18: frames counted while no pause bit is set (`FUN_0014c7b8`).
    pub frames: i32,
    /// 0x356f1c: pause bits (4.13 sets, 4.24 clears; bit 2 belongs to the clock's own HUD record).
    pub pause: u32,
    /// 0x356f20: `frames` when the clock started.
    pub started_at: i32,
    /// 0x356f24: the length (countdown) or the limit (count up, 0 = none).
    pub limit: i32,
    /// 0x356f28: the time shown.
    pub shown: i32,
    /// 0x356f2c: 0 stopped, 1 started (counts from the next tick), 2 running.
    pub state: u8,
    /// 0x356f30: counts down.
    pub down: bool,
}

impl Clock {
    /// `FUN_0014c6b0(s)`: a countdown of `s` seconds.
    pub fn set_countdown(&mut self, secs: i32) {
        self.limit = secs * 60;
        self.down = true;
        self.shown = secs * 60;
        self.state = 0;
    }

    /// `FUN_0014c6e0(s)`: counting up to `s` seconds.
    pub fn set_count_up(&mut self, secs: i32) {
        self.down = false;
        self.limit = secs * 60;
        self.state = 0;
        self.shown = 0;
    }

    /// `FUN_0014c728`: start.
    pub fn start(&mut self) {
        self.started_at = self.frames;
        self.state = 1;
    }

    /// `FUN_001cf7d8` (the clock record's start, host 4.7): a positive time counts down from it, otherwise the
    /// clock counts up to `max`; then it starts and pause bit 2 clears.
    pub fn begin(&mut self, secs: i32, max: i32) {
        if secs < 1 {
            self.set_count_up(max);
        } else {
            self.set_countdown(secs);
        }
        self.start();
        self.pause &= !2;
    }

    /// `FUN_0014c790`: stops and returns the time shown.
    pub fn stop(&mut self) -> i32 {
        self.state = 0;
        self.shown
    }

    /// One frame: `FUN_0014c7b8` (the frame count, while no pause bit is set) and then `FUN_0014c560`.
    /// Returns true when the clock ran out (the game then sends global event 0x4a with 0).
    /// [inferred] the count steps before the clock reads it; both run once per frame.
    pub fn tick(&mut self) -> bool {
        if self.pause != 0 {
            return false;
        }
        self.frames += 1;
        match self.state {
            1 => {
                self.state = 2;
                false
            }
            2 if self.down => {
                let left = self.limit - (self.frames - self.started_at);
                self.shown = left;
                if left > 0 {
                    return false;
                }
                self.shown = 0;
                self.state = 0;
                true
            }
            2 => {
                let t = self.frames - self.started_at;
                if t > CLOCK_UP_LIMIT {
                    self.shown = CLOCK_UP_LIMIT;
                    return false;
                }
                self.shown = t;
                if self.limit > 0 && t >= self.limit {
                    self.shown = self.limit;
                    self.state = 0;
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// `FUN_0014c7f0`: hours, minutes, seconds and hundredths of a frame count (the clock HUD `FUN_00150d40`
    /// draws minutes, seconds and the hundredths; the factor 100/60 is the float at 0x379168 [inferred: read
    /// from the executable by the test]).
    pub fn split(frames: i32, frac_scale: f32) -> (i32, i32, i32, i32) {
        let h = frames / 0x34bc0;
        let rest = frames % 0x34bc0;
        let m = rest / 0xe10;
        let s = (rest % 0xe10) / 0x3c;
        let f = ((rest % 0xe10 % 0x3c) as f32 * frac_scale) as i32;
        (h, m, s, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc() -> Option<Disc> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        if iso.is_none() {
            eprintln!("skipped: no disc (set KH2_ISO)");
        }
        iso.map(|p| Disc::open(&p))
    }

    /// A Struggle mission from the disc with the manager shown, as the first frame leaves it.
    fn struggle(disc: &Disc, name: &str) -> Active {
        let table = disc.read_root("SLUS_210.05").and_then(|e| kh2_bdx::HostTable::from_exe(&e));
        let file = disc.read(&format!("msn/us/{name}.bar")).expect("mission file");
        let mut a = build(0x128, name, file, true, table.as_ref()).expect("mission");
        let mut rt = a.rt.take().expect("script");
        let mut unknown = HashSet::new();
        rt.event(&mut MissionHost { a: &mut a, unknown: &mut unknown }, 2, &[]);
        rt.start_thread(0, 3, &[]);
        // The thread's first frame (it clears its state and waits).
        rt.update_script(&mut MissionHost { a: &mut a, unknown: &mut unknown });
        a.rt = Some(rt);
        a
    }

    /// Delivers one global event to the script and runs a few frames; returns the events it raised.
    fn deliver(a: &mut Active, id: u32, arg: i32) -> Vec<(u32, i32)> {
        let mut rt = a.rt.take().unwrap();
        let mut unknown = HashSet::new();
        rt.event(&mut MissionHost { a: &mut *a, unknown: &mut unknown }, 10, &[id, arg as u32]);
        for _ in 0..3 {
            rt.update_script(&mut MissionHost { a: &mut *a, unknown: &mut unknown });
        }
        a.rt = Some(rt);
        assert!(unknown.is_empty(), "unmapped host calls: {unknown:?}");
        std::mem::take(&mut a.events)
    }

    #[test]
    fn the_struggle_missions_hold_two_orb_counters_and_a_minute() {
        let Some(disc) = disc() else { return };
        for name in ["TT04_MS00", "TT04_MS01", "TT04_MS02"] {
            let a = struggle(&disc, name);
            let live: Vec<(i32, i32, i32)> = a.counters.iter().filter(|c| c.active).map(|c| (c.slot, c.value, c.max)).collect();
            eprintln!("{name}: flags {:#06x}, counters {live:?}, clock {:?}, elements {:?}", a.header.flags, a.clock_rec, a.elements);
            assert_eq!(live, vec![(0, 100, 200), (1, 100, 200)], "{name}");
            assert_eq!(a.clock_rec.map(|c| c.start), Some(60), "{name}");
            assert!(a.no_prizes(), "{name}: header flag 0x40");
            // Pictures: counters at BAR entries 12/13 and 14/15, the clock at 16/17.
            assert_eq!(a.elements.iter().map(|e| e.picture_entries()).collect::<Vec<_>>(), vec![Some((12, 13)), Some((14, 15)), Some((16, 17))]);
        }
    }

    #[test]
    fn the_struggle_script_decides_by_a_full_counter_or_by_time() {
        let Some(disc) = disc() else { return };
        for name in ["TT04_MS00", "TT04_MS01", "TT04_MS02"] {
            // The player's counter full (0x7b with slot 0): he wins (159), then the mission ends (4.3).
            let mut a = struggle(&disc, name);
            assert_eq!(deliver(&mut a, EV_COUNTER_FULL, 0), vec![(EV_PLAYER_WON, 0)], "{name}");
            assert!(a.stopped && a.latch, "{name}: ended through 4.54 / 4.3");
            // The opponent's full: he loses (160).
            let mut a = struggle(&disc, name);
            assert_eq!(deliver(&mut a, EV_COUNTER_FULL, 1), vec![(EV_PLAYER_LOST, 0)], "{name}");
            // Time: more orbs wins; a tie loses.
            for (p, o, ev) in [(120, 80, EV_PLAYER_WON), (80, 120, EV_PLAYER_LOST), (100, 100, EV_PLAYER_LOST)] {
                let mut a = struggle(&disc, name);
                a.counters[0].value = p;
                a.counters[1].value = o;
                assert_eq!(deliver(&mut a, EV_CLOCK_END, 0), vec![(ev, 0)], "{name} {p}:{o}");
            }
            // An empty counter decides nothing.
            let mut a = struggle(&disc, name);
            assert_eq!(deliver(&mut a, EV_COUNTER_EMPTY, 1), vec![], "{name}");
            assert!(!a.stopped);
        }
    }

    #[test]
    fn reactions_knock_out_3_or_15_orbs() {
        assert_eq!(orbs_knocked_out(0, 100), 3);
        assert_eq!(orbs_knocked_out(2, 100), 3);
        assert_eq!(orbs_knocked_out(6, 100), 3);
        assert_eq!(orbs_knocked_out(4, 100), 15);
        assert_eq!(orbs_knocked_out(5, 100), 15);
        assert_eq!(orbs_knocked_out(14, 100), 15);
        assert_eq!(orbs_knocked_out(1, 100), 0);
        assert_eq!(orbs_knocked_out(12, 100), 0);
        assert_eq!(orbs_knocked_out(4, 7), 7);
        assert_eq!(orbs_knocked_out(3, 0), 0);
        assert_eq!((side_of(true), side_of(false)), (0, 1));
        assert_eq!((orb_kind(true), orb_kind(false)), (0, 2));
    }

    #[test]
    fn mission_header_records_decode() {
        let mut d = vec![0u8; 0x1c];
        d[2..4].copy_from_slice(&0x128u16.to_le_bytes());
        d[4..6].copy_from_slice(&0x0d61u16.to_le_bytes());
        for w in [0x0004_0005u32, 100, 200, 0, 0x0101_000f, 0] {
            d.extend_from_slice(&w.to_le_bytes());
        }
        let h = parse_header(&d).unwrap();
        assert_eq!((h.id, h.flags, h.records.len()), (0x128, 0x0d61, 1));
        let e = Element::from_record(&h.records[0]).unwrap();
        assert_eq!(e, Element { op: 5, start: 100, max_word: 200, third: 0, picture: 0xf, side: 1, slot: 1 });
        assert_eq!(e.picture_entries(), Some((14, 15)));
    }

    #[test]
    fn an_orb_rests_and_goes_to_the_first_fighter_in_reach() {
        let Some(elf) = disc().and_then(|d| d.read_root("SLUS_210.05")) else { return };
        let c = orb_consts(&elf).unwrap();
        assert_eq!((c.kinds[0].id, c.kinds[2].id), (0, 2));
        let floor = |s: Vec3, v: Vec3| {
            let e = s + v;
            if e.y >= 0.0 { prize::Sweep { end: e.with_y(0.0), on_floor: true } } else { prize::Sweep { end: e, on_floor: false } }
        };
        let mut o = Orb { p: prize::launch(&c, 2, [0x8000_0000; 3], Vec3::new(0.0, -100.0, 0.0)), taker: None };
        let far = Taker { ent: Entity::from_raw_u32(1).unwrap(), player: true, feet: Vec3::new(1000.0, 0.0, 0.0), centre: Vec3::new(1000.0, -90.0, 0.0), extra: 0.0 };
        let near = Taker { ent: Entity::from_raw_u32(2).unwrap(), player: false, feet: Vec3::ZERO, centre: Vec3::new(0.0, -90.0, 0.0), extra: 0.0 };
        let mut taken = None;
        for _ in 0..600 {
            let (alive, _, side) = orb_tick(&mut o, &c, floor, &[far, near]);
            assert!(alive, "an orb's life does not run down");
            if side.is_some() {
                taken = side;
                break;
            }
        }
        assert_eq!(taken, Some(1), "the opponent standing on it takes it for slot 1");
        assert_eq!(o.p.phase, Phase::Absorb);
    }

    fn counter(max: i32, v: i32) -> Counter {
        Counter { active: true, shown: true, slot: 1, max, value: v, ..Counter::default() }
    }

    #[test]
    fn a_counter_holds_its_range_and_reports_empty_and_full() {
        let mut c = counter(100, 5);
        assert_eq!(c.add(-3), None);
        assert_eq!(c.value, 2);
        assert_eq!(c.add(-7), Some((EV_COUNTER_EMPTY, 1)));
        assert_eq!(c.value, 0);
        // Already empty: no second event.
        assert_eq!(c.step(false), None);
        assert_eq!(c.add(150), Some((EV_COUNTER_FULL, 1)));
        assert_eq!(c.value, 100);
        assert_eq!(c.step(true), None);
        assert_eq!(c.value, 100);
        // Hidden counters and counters without a slot send nothing.
        let mut h = Counter { shown: false, ..counter(10, 3) };
        assert_eq!(h.set(0), None);
        let mut n = Counter { slot: -1, ..counter(10, 3) };
        assert_eq!(n.set(0), None);
        assert_eq!(n.value, 0);
    }

    #[test]
    fn a_counter_digit_is_replaced_in_place() {
        let mut c = counter(99999, 1234);
        c.set_digit(1, 7);
        assert_eq!(c.value, 1274);
        c.set_digit(0, 0);
        assert_eq!(c.value, 1270);
    }

    #[test]
    fn counter_init_reads_the_maximum_word() {
        let mut c = Counter::default();
        c.init(100, 0x4000_0000 | 200);
        assert_eq!((c.value, c.max, c.show_max), (100, 200, true));
        c.init(30, 0);
        assert_eq!((c.value, c.max), (30, 200));
    }

    #[test]
    fn the_countdown_runs_out_after_its_length() {
        let mut k = Clock::default();
        k.begin(2, 0);
        let mut ended = None;
        for f in 1..=200 {
            if k.tick() {
                ended = Some(f);
                break;
            }
        }
        // The start frame is taken when the clock starts; the tick that makes it run already counts.
        assert_eq!(ended, Some(120));
        assert_eq!(k.shown, 0);
        assert_eq!(k.state, 0);
        // A paused clock does not move.
        let mut p = Clock::default();
        p.begin(1, 0);
        p.pause = 1;
        for _ in 0..100 {
            assert!(!p.tick());
        }
        assert_eq!(p.shown, 60);
    }

    #[test]
    fn the_count_up_clock_ends_at_its_limit() {
        let mut k = Clock::default();
        k.begin(0, 1);
        let n = (1..100).find(|_| k.tick());
        assert_eq!(n, Some(60));
        assert_eq!(k.shown, 60);
        assert_eq!(Clock::split(60 * 75 + 30, 100.0 / 60.0), (0, 1, 15, 50));
    }
}
