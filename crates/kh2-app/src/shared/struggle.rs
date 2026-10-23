//! The mission manager and its records, the global events and the area triggers the lower crates read (moved out of `struggle.rs`, which re-exports them).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use kh2_bdx::Runtime;

use crate::shared::prize::Prize;

/// `00objentry` (ObjectType, SubType) of the Struggle orb pool object `PRIZE_TT` (`FUN_001c0400` case 8 ->
/// `FUN_001cac70`, which sets `DAT_003600c0`).
pub const ORB_POOL_TYPE: (u8, u8) = (22, 8);

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

/// The wave spawner started a group whose program has a mission command.
#[derive(Message, Clone, Debug)]
pub struct MissionStart {
    pub id: u32,
    pub name: String,
    pub orb_pool: bool,
    /// The area triggers (area-script opcode 0xc) the mission's end may meet (`FUN_00197588`).
    pub triggers: Vec<Trig>,
}

/// An area trigger as the mission end looks it up (`FUN_00197588`: the record's event and argument, -1 matches
/// any argument; `FUN_001b9780` reads the record's story flag, +8, set by trigger sub-command 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trig {
    pub event: u16,
    pub arg: u16,
    pub story_flag: Option<u16>,
    /// The trigger plays an event (sub-command 0) or jumps (sub-command 1): what the game does when it fires.
    pub plays_event: bool,
    pub jumps: bool,
    /// Sub-command 1's jump record (type, world, area, entrance, key, flags; `FUN_0019d8f0`), which
    /// `FUN_00197a60` copies for the result menu's jump (`FUN_001b9c58`).
    pub jump: Option<[u16; 6]>,
}

impl Trig {
    pub fn from_ard(t: &kh2_ard::Trigger) -> Trig {
        Trig {
            event: t.event,
            arg: t.arg,
            story_flag: t.subs.iter().find(|(op, _)| *op == 2).and_then(|(_, h)| h.first().copied()),
            plays_event: t.subs.iter().any(|(op, _)| *op == 0),
            jumps: t.subs.iter().any(|(op, _)| *op == 1),
            jump: t.jump().map(|j| [j.kind, j.world, j.area, j.entrance, j.key, j.flags]),
        }
    }

    /// `FUN_00197588(event, arg)`: same event, and the same argument or a record argument of -1.
    pub fn matches(&self, event: u32, arg: i32) -> bool {
        self.event as u32 == event && (self.arg == arg as u16 || self.arg == 0xffff)
    }
}

/// A global event (`FUN_00197b70(event, argument)`): besides the mission script it goes to every object
/// (`FUN_0016b470`, object method +0x34); the enemy scripts get it as event 10 (`enemy_ai`).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalEvent {
    pub id: u32,
    pub arg: i32,
}

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
    /// +8: the mission kind. `FUN_001b9c00` treats 2, 0x19 and 0x20 as the kinds whose end opens the menu of
    /// record 0x1a (`FUN_001b96e0`) instead of sending the end event to the area; the start applies the
    /// pause-menu record of this number (`FUN_001b8ee8` -> `FUN_001bef48`).
    pub kind: u8,
    /// +0xd: when not 0, the end passes it to `FUN_001be6b8` (the bonus reward id: `Progress::give_bonus` is the port, not wired to
    /// the end here because the header is 0 in every Struggle mission file; WP-DYN1 found that this is the way `bons` rewards reach
    /// the game besides an `enmp` BonusLevel, which is 0 in all 177 rows of the USA disc).
    pub unlock: u8,
    /// +0x10, +0x14, +0x18: interface sounds (`FUN_001b9ec8` -> `FUN_002c3318`) of the start
    /// (`FUN_001b8ee8`), the end (`FUN_001b95a0`) and the failure (`FUN_001b9818`); 0 plays nothing.
    pub se_start: u32,
    pub se_end: u32,
    pub se_fail: u32,
    pub records: Vec<Record>,
}

impl Header {
    /// `FUN_001b9c00`: the mission kinds 2, 0x19 and 0x20.
    pub fn ends_in_menu(&self) -> bool {
        matches!(self.kind, 2 | 0x19 | 0x20)
    }
}

pub fn parse_header(d: &[u8]) -> Option<Header> {
    let u16_at = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
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
    Some(Header {
        id: u16_at(2)?,
        flags: u16_at(4)?,
        kind: *d.get(8)?,
        unlock: *d.get(0xd)?,
        se_start: u32_at(0x10)?,
        se_end: u32_at(0x14)?,
        se_fail: u32_at(0x18)?,
        records,
    })
}

/// A layout record (opcodes 1, 2, 3 and 0xd; `FUN_001b9378` -> `FUN_001d3490`): an event of the mission file
/// played by the event player (`FUN_001d3538` -> `FUN_00195898`, `FUN_00195ae0`), with a picture the event shows.
/// Record bytes +4 the event's BAR index, +5 the 1-based index of the picture (image; the sequence follows,
/// `FUN_001d3390`), +6 an object the event binds (0: none). The event kind is the manager's default
/// (`FUN_001b8b10`: record 1 0xc, 2 and 0xd 0xe, 3 0xf); record 1 with a picture becomes 0xd.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub op: u16,
    pub kind: u8,
    pub entry: u8,
    pub picture: u8,
    pub object: u16,
    /// Event opcode 8 (`FUN_002245d0` -> `FUN_00226fb0`): the event frame it ends at (-1: none).
    pub end: i32,
    /// Event opcode 0x50 (`FUN_00225af0`): the event frame from which a group of the picture plays
    /// (`FUN_00225998`: the event object's picture pair +0x94 / +0x98).
    pub picture_at: Option<i32>,
    /// The record's words +0x14 and +0x18 in the manager (`FUN_001d3538`: +0x14 non-zero -> event +0x9c, +0x18
    /// non-zero -> event +0x9c = 2; `FUN_002232c8` copies +0x9c to `DAT_0036bd30`). Only the failure path writes
    /// one here: +0xb4 = 2 (`FUN_001b9818`) is record 3's +0x14.
    pub select: u32,
    pub select2: u32,
    /// Per group of the picture's sequence: the 60 Hz ticks one play takes (`last frame + 1`), `None` for a group
    /// that loops for ever (it would never end). The 2D object `FUN_00225af0` makes plays the group once and
    /// destroys itself at its end (`FUN_00225aa8` -> `FUN_00138aa8`, vtable +0x14); its destructor `FUN_00225a50`
    /// starts a thread of `FUN_00225a20`, which ends the event (`FUN_00223658(0)` -> `FUN_002235c0`) [WP-G1].
    pub group_ticks: Vec<Option<i32>>,
}

/// The layout slots of the manager: record 1 (+0x4c, the start), 2 (+0x68, the end), 3 (+0xa0, the failure),
/// 0xd (+0x84, the one after the end's wait).
pub const LAYOUT_START: usize = 0;

pub const LAYOUT_END: usize = 1;

pub const LAYOUT_FAIL: usize = 2;

pub const LAYOUT_AFTER: usize = 3;

impl Layout {
    pub fn from_record(r: &Record, entries: &[kh2_bar::Entry]) -> Option<(usize, Layout)> {
        let (slot, kind) = match r.op {
            1 => (LAYOUT_START, 0xc),
            2 => (LAYOUT_END, 0xe),
            3 => (LAYOUT_FAIL, 0xf),
            0xd => (LAYOUT_AFTER, 0xe),
            _ => return None,
        };
        let w = *r.args.first()?;
        let (entry, picture, object) = (w as u8, (w >> 8) as u8, (w >> 16) as u16);
        let kind = if kind == 0xc && picture != 0 { 0xd } else { kind };
        let cmds = entries.get(entry as usize).and_then(|e| kh2_ard::parse_event(e.data)).unwrap_or_default();
        let at = |op: u16| cmds.iter().find(|c| c.opcode == op).and_then(|c| c.data.get(0..2)).map(|b| i16::from_le_bytes([b[0], b[1]]) as i32);
        let group_ticks = (picture as usize)
            .checked_sub(1)
            .and_then(|i| kh2_image::seqd::parse(entries.get(i + 1)?.data))
            .map(|s| (0..s.groups.len()).map(|g| (s.groups[g].loops == 1).then(|| s.last_frame(g) + 1)).collect())
            .unwrap_or_default();
        Some((slot, Layout { op: r.op, kind, entry, picture, object, end: at(8).unwrap_or(-1), picture_at: at(0x50), select: 0, select2: 0, group_ticks }))
    }

    /// The ticks the picture's chosen group plays for (`None`: no picture, or a group that never ends).
    pub fn picture_ticks(&self) -> Option<i32> {
        self.group_ticks.get(self.group()).copied().flatten()
    }

    /// The picture's group (`FUN_00225af0`: `DAT_0036bd30` 1 to 5 picks that group, anything else group 1).
    pub fn group(&self) -> usize {
        let sel = if self.select != 0 { self.select } else if self.select2 != 0 { 2 } else { u32::MAX };
        if (1..=5).contains(&sel) { sel as usize } else { 1 }
    }

    /// The BAR entries (image, sequence) of its picture.
    pub fn picture_entries(&self) -> Option<(usize, usize)> {
        (self.picture > 0).then(|| (self.picture as usize - 1, self.picture as usize))
    }
}

/// One event playing (`DAT_0035e3a4`: one at a time; a new one ends the old, `FUN_00195ae0`).
#[derive(Clone, Debug, PartialEq)]
pub struct Play {
    pub what: &'static str,
    pub layout: Layout,
    /// The kind row's flags (`evtp` +2).
    pub flags: u16,
    /// Game frames since the start. The event frame is half of it (`FUN_00222728`: the counter adds the frame
    /// step, 1.0, each frame and the frame is the counter times 0.5 at 0x37ec50) [inferred: the startup states of
    /// the event object (`FUN_00195c08`, fades) take no time here].
    pub ticks: i32,
}

impl Play {
    pub fn frame(&self) -> i32 {
        self.ticks / 2
    }

    /// `FUN_00195ae0`: kind flag bit 3 clear -> `FUN_00196280`, players 0 and 1 lose control (`FUN_001b7ce8`;
    /// `FUN_001b7d50` reads it) until the end (`FUN_00196170` -> `FUN_001962a0`).
    pub fn locks_control(&self) -> bool {
        self.flags & 8 == 0
    }

    /// `FUN_00195ae0`: kind flag bit 2 clear -> `FUN_00196760` -> `FUN_0014ef48` hides the field HUD's draw tags 3
    /// to 6 (`FUN_0013da78`; tag 6 is the target gauge, `FUN_00154498`) until the end (`FUN_00196790`).
    pub fn hides_hud(&self) -> bool {
        self.flags & 4 == 0
    }

    /// The game tick at which the picture's group has played through (`2 * picture_at + ticks of the group`: the
    /// 2D object starts at event frame `picture_at`, two game ticks per event frame).
    pub fn picture_end(&self) -> Option<i32> {
        Some(2 * self.layout.picture_at? + self.layout.picture_ticks()?)
    }

    /// The event ends when its frame reaches the end frame (`FUN_00222668`), or when its picture's 2D object has
    /// played its group through and destroyed itself (`FUN_00225a50` -> `FUN_00225a20` -> `FUN_00223658(0)`,
    /// `FUN_002235c0`). The end frame (500 event frames) is 1000 game ticks, the picture's group in the Struggle's
    /// files is 90 to 300, so the picture ends the event [inferred: the destroy thread's start takes a tick or two].
    pub fn ended(&self) -> bool {
        (self.layout.end >= 0 && self.frame() >= self.layout.end) || self.picture_end().is_some_and(|e| self.ticks >= e)
    }

    /// `FUN_00224b98`: frames left (0 without an end frame).
    pub fn remaining(&self) -> i32 {
        if self.layout.end < 0 { 0 } else { self.layout.end - self.frame() }
    }

    pub fn picture_shown(&self) -> bool {
        self.layout.picture_at.is_some_and(|f| self.frame() >= f)
    }
}

/// What the end of the mission's update does (`FUN_001b96e0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finish {
    /// Mission kinds 2, 0x19, 0x20 or event 0x9c: the menu of record 0x1a opens (`FUN_001bef10(0x1a)` ->
    /// `FUN_0024f330`) and the manager stays while it is open (`Mission::menu_request`, `menu_struggle`); a row of
    /// the menu ends it (`Mission::menu_exit`).
    Menu,
    /// The event goes to the area (`FUN_00197b70`): a trigger that takes it ends the manager (`FUN_001b9518`);
    /// event 8 that no trigger takes ends it too, with the game-over path (`FUN_001b9ce0`).
    Event(u32, i32),
    /// `FUN_001b9ce0`: the manager ends and the game-over path runs (kind 2, 0x1e, 0x23: `FUN_001ad8e0(0)`, else
    /// `FUN_001ad868`): `Mission::game_over_request` hands it to `menu_gameover`.
    GameOver,
    /// The manager ends (host 4.22, `FUN_001b9908`).
    Gone,
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

/// A word of the executable at a virtual address.
pub fn exe_word(elf: &[u8], va: u32) -> Option<u32> {
    exe_bytes(elf, va).map(u32::from_le_bytes)
}

pub fn exe_bytes(elf: &[u8], va: u32) -> Option<[u8; 4]> {
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

/// One orb of the pool: a prize entry (flag bit 4 set by `FUN_001cad50`: its life does not run down) and,
/// while it is drawn in, the fighter taking it.
#[derive(Clone, Debug)]
pub struct Orb {
    pub p: Prize,
    pub taker: Option<Entity>,
    /// The entity that draws it (`show_orbs`), its sprite children and the shape frame they show.
    pub ent: Option<Entity>,
    pub kids: Vec<Entity>,
    pub shown: Option<usize>,
}

impl Orb {
    pub fn new(p: Prize) -> Orb {
        Orb { p, taker: None, ent: None, kids: Vec::new(), shown: None }
    }
}

/// The mission manager (`DAT_01d5dd20`, made by `FUN_001b9338` -> `FUN_001b8b10`), with what the app uses.
#[derive(Resource, Default)]
pub struct Mission {
    pub m: Option<Box<Active>>,
    /// Orb entities left by a manager that ended, for `show_orbs` to remove.
    pub gone: Vec<Entity>,
    /// The end's last step asked for the menu of this pause-menu record (`FUN_001bef10(0x1a)` -> `FUN_0024f330`);
    /// `menu_struggle` takes it and opens the menu while the manager stays.
    pub menu_request: Option<u16>,
    /// The manager ended with the game-over path (`FUN_001b9ce0`); `menu_gameover` takes it and opens the game over.
    pub game_over_request: Option<GameOverRequest>,
}

/// The jump records of the area triggers that answer the game-over menu's rows of mission kind 3 (`FUN_001b9c58`: code 3
/// -> event 0x8b, code 4 -> event 0x8d; `FUN_00197588(event, -1)`), read from the manager before it ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GameOverExits {
    pub retry: Option<[u16; 6]>,
    pub quit: Option<[u16; 6]>,
}

/// `FUN_001b9ce0` after the manager ended: game over of type 3 (mission kinds 2, 0x1e, 0x23: `FUN_001ad8e0(0)`) or type 1
/// (`FUN_001ad868`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameOverRequest {
    pub kind: u8,
    pub exits: GameOverExits,
}

/// What `FUN_001cec00` asks of the manager when the player is knocked out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KoView {
    /// Host 1.317's bit 0x20000 of `DAT_0035d268`: the knock-out is global event 0x5a, not a game over.
    pub loses: bool,
    /// `FUN_001b9c00`: the manager's kind is 0x02, 0x19 or 0x20 (the game over is type 3).
    pub menu_kind: bool,
    pub exits: GameOverExits,
}

/// What a row of the result menu does to the manager (`FUN_001b9c58(code)`, the pointer list's codes 3 and 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuExit {
    /// The event sent to the area: 0x8b for code 3, 0x8d for code 4, -1 for any other code.
    pub event: i32,
    /// The jump record of the area trigger that answers the event with any argument (`FUN_00197a60(buf, event)` ->
    /// `FUN_00197588(event, -1)`), handed to `FUN_0019d9d8(record, 0x40000000)`.
    pub jump: Option<[u16; 6]>,
}

impl Mission {
    /// The manager's part of the player's knock-out decision (`FUN_001cec00`); `None` without a manager.
    pub fn ko_view(&self) -> Option<KoView> {
        let a = self.m.as_deref()?;
        Some(KoView { loses: a.ko_loses, menu_kind: a.header.ends_in_menu(), exits: a.game_over_exits() })
    }
}

impl Mission {
    /// `FUN_001b9c58(code)`: code 3 -> event 0x8b (and `FUN_001d58e0` when the mission kind is 0x1e), code 4 ->
    /// event 0x8d (and `FUN_001abfe8(9)`); then the manager ends (`FUN_001b9518`) and the trigger's jump record of
    /// the event is taken. `None` when no manager runs.
    pub fn menu_exit(&mut self, code: u16) -> Option<MenuExit> {
        let a = self.m.as_deref()?;
        let event = match code {
            3 => 0x8b,
            4 => 0x8d,
            _ => -1,
        };
        if code == 3 && a.header.kind == 0x1e {
            info!("struggle: menu code 3 with mission kind 0x1e: FUN_001d58e0 (clears four words of FUN_001b8218's object, party reset 0xc) is not ported");
        }
        if code == 4 {
            info!("struggle: menu code 4: party reset kind 9 (FUN_001abfe8(9) -> FUN_001ac130 for members 1..15) is not ported");
        }
        let jump = if event < 0 { None } else { a.trigger(event as u32, -1).and_then(|t| t.jump) };
        info!("struggle: menu code {code} (FUN_001b9c58): event {event:#x}, the trigger's jump {jump:?}; the manager ends (FUN_001b9518)");
        self.end();
        Some(MenuExit { event, jump })
    }

    /// `FUN_001b9518` -> `FUN_001b90d0`: the elements and the clock go with the manager; the orbs' entities are removed.
    pub fn end(&mut self) {
        let Some(mut a) = self.m.take() else { return };
        let (p, o) = (a.counter_value(0), a.counter_value(1));
        let orbs: Vec<Entity> = a.orbs.drain(..).flat_map(|o| o.ent).chain(a.gone.drain(..)).collect();
        self.gone.extend(orbs);
        info!("struggle: mission manager ended (player {p} orbs, opponent {o})");
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

    /// An event of the mission is playing (`DAT_0035e3a4` non-zero: host 1.34 `event_active`).
    pub fn event_active(&self) -> bool {
        self.m.as_ref().is_some_and(|a| a.play.is_some())
    }

    /// The playing layout keeps the players from acting (`FUN_00196280`; read by `FUN_001b7d50`).
    pub fn control_locked(&self) -> bool {
        self.m.as_ref().and_then(|a| a.play.as_ref()).is_some_and(|p| p.locks_control())
    }

    /// The playing layout hides the field HUD (draw tags 3 to 6, `FUN_0014ef48`).
    pub fn hud_hidden(&self) -> bool {
        self.m.as_ref().and_then(|a| a.play.as_ref()).is_some_and(|p| p.hides_hud())
    }

    /// The mission stopped (`FUN_001b9940`: manager +4 bits 4 and 0x10, bit 2 of `DAT_0035d268`): the commands
    /// are refused (`FUN_001ba2c0` -> `FUN_001b70b8` answers 5), the player's method +0x60 reports busy
    /// (`FUN_001789d8` -> `FUN_00175ad8`), the counters and the clock no longer move.
    pub fn fight_stopped(&self) -> bool {
        self.m.as_ref().is_some_and(|a| a.stopped)
    }
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
    /// Entities of orbs that ended, for `show_orbs` to remove.
    pub gone: Vec<Entity>,
    /// The generator the orb launches step (the game shares 0x3716a0 with every other user; here it is the
    /// pool's own [approx]).
    pub seed: u32,
    pub rt: Option<Runtime>,
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
    /// The layout records (`LAYOUT_*`).
    pub layouts: [Option<Layout>; 4],
    /// The event kind rows (`evtp`).
    pub kinds: Vec<(u8, u16)>,
    /// The area triggers the end looks up.
    pub triggers: Vec<Trig>,
    /// +0x2d8: the update state of `FUN_001b9140` (0 wait to show, 1 running, 2 / 3 / 4 the end).
    pub state: u8,
    /// The state 3 step has run: `DAT_0035d268 |= 8` (bit 3 of the global flag word, prizes and items go to the player at
    /// once). Written only by `FUN_001b9140` case 3; see `collect_latch` for who keeps it.
    pub collect_at_once: bool,
    /// +0x2dc, +0x2e0: the end event and its argument.
    pub end_ev: u32,
    pub end_arg: i32,
    /// The event playing (`DAT_0035e3a4`).
    pub play: Option<Play>,
    /// +0xb4 = 2: the failure path ran (`FUN_001b9818`).
    pub failed: bool,
    /// Bit 0x20000 of `DAT_0035d268` (host 1.317): the player's knock-out ends the match (`FUN_001cec00` sends
    /// global event 0x5a) instead of the game-over path.
    pub ko_loses: bool,
    /// Interface sounds to play (`FUN_002c3318`).
    pub sounds: Vec<u32>,
    /// The end's last step (`FUN_001b96e0`), once reached, and the same waiting for `mission_tick`.
    pub finish: Option<Finish>,
    pub pending: Option<Finish>,
}

impl Active {
    /// The jump records of the triggers for events 0x8b and 0x8d (`FUN_00197588(event, -1)`).
    pub fn game_over_exits(&self) -> GameOverExits {
        GameOverExits { retry: self.trigger(0x8b, -1).and_then(|t| t.jump), quit: self.trigger(0x8d, -1).and_then(|t| t.jump) }
    }
}

impl Active {
    /// Host 4.29 (`FUN_001d6738`): the value of counter `i`, 0 when it does not exist.
    pub fn counter_value(&self, i: usize) -> i32 {
        self.counters.get(i).filter(|c| c.active).map_or(0, |c| c.value)
    }

    /// `FUN_001b9560`: the manager exists and is not stopped (every counter change asks it).
    pub fn running(&self) -> bool {
        !self.stopped
    }

    /// A counter change through the hosts (`FUN_001d6598` 4.5, `FUN_001d6620` 4.41, 4.6 / 4.20, 4.48).
    pub fn change(&mut self, i: usize, f: impl FnOnce(&mut Counter) -> Option<(u32, i32)>) {
        if !self.running() {
            return;
        }
        let Some(c) = self.counters.get_mut(i).filter(|c| c.active) else { return };
        if let Some((ev, slot)) = f(c) {
            self.events.push((ev, slot));
        }
    }

    /// `FUN_001b9940`: the mission stops (flags 4 and 0x10, the clock stops through `FUN_0014c790`).
    pub fn stop(&mut self) {
        self.stopped = true;
        self.clock.stop();
    }

    /// No prize objects while the header's flag 0x40 holds (`FUN_001b8d80` sets 0x200 of `DAT_0035d268`;
    /// `FUN_001a3cb0` then drops nothing unless the player's +0x588 bit 0x2000000 is set, which the app never sets).
    pub fn no_prizes(&self) -> bool {
        self.header.flags & 0x40 != 0
    }

    pub fn trigger(&self, ev: u32, arg: i32) -> Option<Trig> {
        self.triggers.iter().copied().find(|t| t.matches(ev, arg))
    }

    /// `FUN_001d3538` + `FUN_00195ae0`: plays a layout record's event (none when the record is absent). A new
    /// event ends the one playing; an event pauses the clock with bit 4 (`FUN_0014c748(4)`, `FUN_001959f0`
    /// clears it at the end).
    pub fn play_layout(&mut self, slot: usize, what: &'static str) {
        let Some(l) = self.layouts[slot].clone() else { return };
        let flags = self.kinds.iter().find(|(k, _)| *k == l.kind).map_or(0, |(_, f)| *f);
        info!(
            "struggle: layout {what} (record {}): event entry {}, kind {:#x} (flags {flags:#06x}), picture {} group {}, end frame {}, picture from frame {:?}",
            l.op, l.entry, l.kind, l.picture, l.group(), l.end, l.picture_at
        );
        self.play = Some(Play { what, layout: l, flags, ticks: 0 });
        self.clock.pause |= 4;
    }

    /// `FUN_001b8ee8`'s tail: the start sound (header +0x10) and the start layout (record 1, at once).
    pub fn begin(&mut self) {
        if self.header.se_start != 0 {
            self.sounds.push(self.header.se_start);
        }
        self.play_layout(LAYOUT_START, "start");
        self.state = 0;
    }

    /// `FUN_001b95a0(event, argument)`: the end sound (header +0x14), the stop (`FUN_001b9940`), then the end
    /// layout (record 2): with event 0x10 or 0x9c, or a trigger for the event when the header has no flag 0x40,
    /// the update waits for the event's last frames (state 2), else for its end (state 4). Without record 2 the
    /// last step runs at once.
    pub fn end_through(&mut self, ev: u32, arg: i32) {
        if self.header.unlock != 0 {
            info!("struggle: header +0xd {} (bonus reward, FUN_001be6b8) is not given by the manager", self.header.unlock);
        }
        if self.header.se_end != 0 {
            self.sounds.push(self.header.se_end);
        }
        self.stop();
        if self.layouts[LAYOUT_END].is_some() {
            let has = self.trigger(ev, -1).is_some();
            let wait_last = ev == 0x10 || ev == 0x9c || (has && self.header.flags & 0x40 == 0);
            self.play_layout(LAYOUT_END, "end");
            self.state = if wait_last { 2 } else { 4 };
            self.end_arg = arg;
            self.end_ev = ev;
            return;
        }
        self.set_finish(self.finish_step(ev, arg));
    }

    /// Host 4.3 (`FUN_001b9780`): the end with event 7, or 0x9c when the mission kind is 0x1e (`FUN_001ba100`) and
    /// the trigger for (7, argument) has a story flag that is set.
    pub fn host_end(&mut self, arg: i32) {
        let set = self.header.kind == 0x1e && self.trigger(7, arg).and_then(|t| t.story_flag).is_some_and(|f| f != 0xffff && (self.story.contains(&(f as i32)) || crate::progress::shared_story().is_some_and(|s| s.lock().test(f))));
        self.end_through(if set { 0x9c } else { 7 }, arg);
    }

    /// Host 4.11 (`FUN_001b9818`): the failure sound (header +0x18), +0xb4 = 2, the failure layout (record 3), the
    /// stop; with a trigger for event 8 or a picture on record 3 (`FUN_001b98a8`: +0xac) the end waits for the
    /// layout and then runs the last step with event 8; otherwise the manager ends at once (`FUN_001b9ce0`).
    pub fn host_fail(&mut self) {
        if self.header.se_fail != 0 {
            self.sounds.push(self.header.se_fail);
        }
        self.failed = true;
        if let Some(l) = self.layouts[LAYOUT_FAIL].as_mut() {
            l.select = 2;
        }
        self.play_layout(LAYOUT_FAIL, "failure");
        self.stop();
        let pictured = self.layouts[LAYOUT_FAIL].as_ref().is_some_and(|l| l.picture != 0);
        if self.trigger(8, -1).is_some() || pictured {
            self.end_ev = 8;
            self.end_arg = -1;
            self.state = 4;
        } else {
            self.set_finish(Finish::GameOver);
        }
    }

    pub fn set_finish(&mut self, f: Finish) {
        self.finish = Some(f);
        self.pending = Some(f);
    }

    /// `FUN_001b96e0`: the last step.
    pub fn finish_step(&self, ev: u32, arg: i32) -> Finish {
        if self.header.ends_in_menu() || ev == 0x9c { Finish::Menu } else { Finish::Event(ev, arg) }
    }

    /// One frame of the event playing (the event player) and of the manager's update (`FUN_001b9140`). The last
    /// step, once the end reaches it, waits in `pending`.
    pub fn flow_tick(&mut self) {
        if let Some(p) = self.play.as_mut() {
            p.ticks += 1;
            if p.ended() {
                info!("struggle: layout {} ended at event frame {}", p.what, p.frame());
                self.play = None;
                self.clock.pause &= !4;
            }
        }
        match self.state {
            // Wait while an event holds the player (FUN_001794f0 false), then show the elements.
            0 => {
                if self.play.as_ref().is_none_or(|p| !p.locks_control()) {
                    // [inferred] the show is skipped when manager +4 bit 1 is set; its writer was not found and it
                    // reads clear here.
                    self.show();
                    self.state = 1;
                }
            }
            2 => self.state = 3,
            // FUN_00196668: frames left of the event (none playing: 0); at 4 or less (0x37bbc8) record 0xd plays.
            3 => {
                if self.play.as_ref().is_some_and(|p| p.remaining() > 4) {
                    return;
                }
                // FUN_001b9140 case 3 sets the bit after FUN_00193f08, before it hands over to FUN_001ba140.
                self.collect_at_once = true;
                if self.end_ev == 0x9c {
                    info!("struggle: event 0x9c: sound 0x1b68 with the sound pause (FUN_002c38c8) is not ported");
                }
                if self.layouts[LAYOUT_AFTER].is_none() {
                    info!("struggle: no record 0xd; the game would play event {} of 00battle.bin (FUN_00193f08), not ported", if self.end_ev == 0x10 { 2 } else { 1 });
                }
                self.play_layout(LAYOUT_AFTER, "after");
                self.state = 4;
            }
            4 if self.play.is_none() && self.finish.is_none() => {
                let f = self.finish_step(self.end_ev, self.end_arg);
                self.set_finish(f);
            }
            _ => {}
        }
    }
    /// `FUN_001d5f00`: the elements come up; the clock record's show method `FUN_001cf748` starts the clock.
    pub fn show(&mut self) {
        self.shown = true;
        for c in self.counters.iter_mut().filter(|c| c.active) {
            c.shown = true;
        }
        if let Some(r) = self.clock_rec {
            let max = (r.max_word & 0xbfff_ffff) as i32;
            self.clock.begin(r.start, max);
            info!("struggle: clock starts: {} s{}", r.start, if r.start < 1 { " counting up" } else { " counting down" });
        }
        info!("struggle: mission HUD shown: player {} orbs, opponent {}", self.counter_value(0), self.counter_value(1));
    }
}

/// Global event a counter sends when it drops to 0 from above (`FUN_001cf9e8` -> `FUN_001cf5b0(.., 0xe)`).
pub const EV_COUNTER_EMPTY: u32 = 0xe;

/// Global event a counter sends when it reaches its maximum from below (`FUN_001cf9e8`, 0x7b).
pub const EV_COUNTER_FULL: u32 = 0x7b;

/// Global event of the player's knock-out with host 1.317's bit (`FUN_001cec00`: `FUN_00197b70(0x5a, 0)`).
pub const EV_PLAYER_KO: u32 = 0x5a;

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
