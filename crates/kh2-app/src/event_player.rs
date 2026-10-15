//! The event player (WP-E6): an area's event (BAR type 0x16 of `ard/<area>.ard`) played on its frame counter.
//! See docs/events.md section 6 ("The event player") and its E6 status.
//!
//! What the game does (`SLUS_210.05`): the setup reader `FUN_002245d0` walks the command list once (actors of
//! opcode 1 into the 0x60-entry table `DAT_0036bbcc` through `FUN_00229120` / `FUN_00224548`, the end frame of
//! opcode 8 into `DAT_0036bcf8` (`FUN_00226fb0`), the load blocks of opcode 0x24 into the 64-entry list
//! `DAT_0036bbd8` (`FUN_00222a48`), the camera keys of opcode 0x13), then sets the ready flag `DAT_0036bcf4`.
//! The timeline thread `FUN_00223a88` then runs the list again in file order with ONE cursor: each timeline
//! command first waits until the frame counter `DAT_0036bcf0` reaches its frame (`FUN_00224a90`), so a command
//! whose frame has passed runs at once. The counter is the game frame count times 0.5 (`FUN_00222728`:
//! `DAT_0036bcec += DAT_00356f40` (1.0) a frame, times `DAT_0037ec50` (0.5)); while a movie plays it is the
//! movie's frame instead (`FUN_002f5ed0` / `FUN_002f5eb8(7)`). The event ends when the counter reaches the end
//! frame (`FUN_00222668` -> `FUN_002235c0`, state 4).
//!
//! Ported here: the setup lists, the counter, the cursor, actors (made at their first motion command 0x2c,
//! `FUN_0022cd90` -> `FUN_0016e8c0` with the objentry id, placed by opcode 2, `FUN_002293b8`), motions (opcode
//! 0x2c plays an `.anb` that a 0x24 load block names, `anm/` + world + `/` + model + `/` + name + `.anb`,
//! `FUN_0022cbb0`), subtitle lines (opcode 0x15, `FUN_00227478`), message windows (opcode 0x2d, `FUN_002275a8`,
//! opened only), the camera (the cut table of opcode 6 and the keys of opcode 0x13 through `intro::Track`).
//! Everything else is logged as not ported (`KH2_EVENTLOG=1`).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_ard::EventCommand;
use kh2_image::text::Style;
use kh2_tables::tables as t;

use super::Track;
use crate::area::{app_yaw, game_to_app, AreaRecord};
use crate::assets::{AreaPart, Disc};
use crate::camera::rules::View;
use crate::camera::CamEvent;
use crate::hud::{HudNode, HudScene, Label};
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::SCALE;

fn le16(d: &[u8], o: usize) -> i16 {
    d.get(o..o + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]))
}

fn le32f(d: &[u8], o: usize) -> f32 {
    d.get(o..o + 4).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn cstr(d: &[u8], o: usize) -> String {
    let s = d.get(o..).unwrap_or(&[]);
    let n = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    String::from_utf8_lossy(&s[..n]).into_owned()
}

/// Opcode 1 (`FUN_00229120`): an actor row of `DAT_0036bbcc` (0x28 bytes): +4 the `00objentry` id (row word 0,
/// what `FUN_0022cd90` makes the object from and `FUN_002290e8` finds the model name by), +6 the actor index (row
/// word 1, what opcodes 2 and 0x2c name), +8 the model name (row +0x17).
#[derive(Clone, Debug, PartialEq)]
pub struct Actor {
    pub object: u16,
    pub index: i16,
    pub name: String,
}

/// Opcode 2 (`FUN_002293b8`): at frame +0x26, actor +0x24 is put at the position +8..+0x14 (`FUN_0016a700`, the
/// game's space) and turned by the rotation +0x14..+0x20 in degrees (`FUN_002292b0`: times `DAT_0037f0c0` =
/// pi / 180; with x and z zero only the yaw, `FUN_0016a7f0`). Only when +4 is 0. +6 is not read there.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub frame: i32,
    pub index: i16,
    pub gate: i16,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

/// Opcode 0x2c (`FUN_0022cd90`): at frame +4, actor +0xe plays the motion named at +0x12 (a record a load block
/// made, found by model and name, `FUN_002242a0`), through `FUN_0017e1c8(2 * (+0x10), 2 * (+8), ..)` =
/// `FUN_0017dd38(blend, start frame, ..)`: +0x10 the cross-fade and +8 the start, both in event frames.
/// `FUN_0022c9b0` copies +0xa / +0xc into the actor's motion slot (`DAT_0036bbc8` row +0x18 / +0x1a), which
/// `FUN_0022ca18` reads as a loop (`MotionClock`). +6 is read by neither [GAP].
#[derive(Clone, Debug, PartialEq)]
pub struct Motion {
    pub frame: i32,
    pub index: i16,
    pub start: i16,
    pub blend: i16,
    pub name: String,
    /// +0xa: the frame the loop goes back to (event frames).
    pub loop_start: i16,
    /// +0xc: the frame past which the motion loops (event frames); -1: no loop.
    pub loop_end: i16,
}

impl Motion {
    pub fn read(d: &[u8]) -> Motion {
        Motion { frame: le16(d, 0) as i32, index: le16(d, 10), start: le16(d, 4), blend: le16(d, 12), name: cstr(d, 14), loop_start: le16(d, 6), loop_end: le16(d, 8) }
    }
}

/// An actor's motion slot (`DAT_0036bbc8`, 64 rows of 0x40 bytes, one per actor: `FUN_0022c928` finds the
/// actor's row, `FUN_0022c970` a free one), as `FUN_0022c9b0` fills it and `FUN_0022ca18` steps it every frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionClock {
    /// Row +0x10: starts at 2 * (+8 - +0x10) and counts up by `DAT_00356f40` (1.0) a frame.
    pub counter: f32,
    pub loop_start: i16,
    pub loop_end: i16,
}

impl MotionClock {
    pub fn new(m: &Motion) -> MotionClock {
        MotionClock { counter: 2.0 * (m.start as i32 - m.blend as i32) as f32, loop_start: m.loop_start, loop_end: m.loop_end }
    }

    /// One frame of `FUN_0022ca18`: with a loop (+0x1a not -1) and the counter past 2 * end, the counter goes
    /// to 2 * start + the overshoot and the motion is started again there with no cross-fade
    /// (`FUN_0017e1c8(0, frame, ..)`); the frame is answered. Then the counter counts one frame.
    pub fn tick(&mut self) -> Option<f32> {
        let mut out = None;
        if self.loop_end != -1 {
            let end = 2.0 * self.loop_end as f32;
            if end < self.counter {
                self.counter = 2.0 * self.loop_start as f32 + (self.counter - end);
                out = Some(self.counter);
            }
        }
        self.counter += 1.0;
        out
    }
}

/// Opcode 0x15 (`FUN_00227478`): at frame +4 the subtitle line of message +8 opens under key +6 (+0xa 0) or
/// the line with that key and message closes (else). Class 0x1aae60, up to 32 lines at `0x1dc00e0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Line {
    pub frame: i32,
    pub key: i16,
    pub message: u16,
    pub open: bool,
}

/// Opcode 0x2d (`FUN_002275a8`): at frame +4, +0xa 0 opens message +8 under key +6 in window kind +0xc
/// (0: class 0x1ab6a8, 1: 0x1ab8e0), 1 closes it and waits, 2 waits until the window reaches state 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub frame: i32,
    pub key: i16,
    pub message: u16,
    pub action: i16,
    pub kind: i16,
}

/// The message windows opcode 0x2d opened (the slots at `0x1dc00e0` of `FUN_002275a8`, up to 32, keyed by
/// (key, message id)) and the counter hold of its action 2.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WindowSlots {
    open: Vec<(i16, u16)>,
    /// `DAT_0036b9c9` (`FUN_00222528` sets, `FUN_00222538` clears): while set `FUN_00222728` does not advance
    /// the event's counter.
    pub hold: bool,
}

impl WindowSlots {
    /// Action 0: the window is in a slot (a full table of 32 drops it).
    pub fn open(&mut self, key: i16, message: u16) {
        if self.open.len() < 32 {
            self.open.push((key, message));
        }
    }

    /// Action 1: find the slot, close the window (the caller sends vtable +0x20), clear the slot, so no wait follows
    /// for it. Returns whether the slot was found.
    pub fn close(&mut self, key: i16, message: u16) -> bool {
        let n = self.open.len();
        self.open.retain(|&s| s != (key, message));
        self.open.len() != n
    }

    /// Action 2: with the slot found, the counter is held until the window waits (state 3). No slot, no hold.
    pub fn wait(&mut self, key: i16, message: u16) -> bool {
        self.hold = self.open.contains(&(key, message));
        self.hold
    }

    /// One tick: whether the counter advances. A held counter lets go once the window is at state 3 (`waiting`);
    /// `closed` also lets it go [inferred: a window that never opened, or was closed, cannot reach state 3 and
    /// the original's wait would not end on its own].
    pub fn tick(&mut self, waiting: bool, closed: bool) -> bool {
        if self.hold && (waiting || closed) {
            self.hold = false;
        }
        !self.hold
    }
}

/// An event's commands sorted into what the player uses.
#[derive(Clone, Debug, Default)]
pub struct Event {
    /// Opcode 0 +0xa: the event's name (`FUN_00223910`, `tt_event_102`).
    pub name: String,
    /// Opcode 8: the end frame (-1: none).
    pub end: i32,
    pub actors: Vec<Actor>,
    /// Opcode 0x24 sub-records 0x25 (`FUN_00222a88` -> `FUN_0022cc88`): (objentry id, actor index, motion name).
    pub loads: Vec<(u16, i16, String)>,
    pub places: Vec<Place>,
    pub motions: Vec<Motion>,
    pub lines: Vec<Line>,
    pub windows: Vec<Window>,
    /// Opcode 0x44 (`FUN_00225860`): (frame, name) of `movie/<world>/<name>.pss`.
    pub movies: Vec<(i32, String)>,
    /// Opcode 0x24 sub-records 0x26 (`FUN_0022c070`): voice names, `voice/<world>/event/<name>.vag`
    /// (`FUN_0022bff8`), loaded into a file slot of kind 3 (`FUN_00224200`).
    pub voices: Vec<String>,
    /// Sub-records 0x39 (`FUN_00227df0`): (+4, name +6) of `event/effect/<name>.pax` (`FUN_00227d80`), a file
    /// slot of kind 2 with +4 at slot +0xc.
    pub effects: Vec<(i16, String)>,
    /// Sub-records 0x3e (`FUN_00226100`): `event/layout/<world>/<name>.2ld` (`FUN_002260b0`), kept at
    /// `DAT_01dc0078`.
    pub layouts: Vec<String>,
    /// Opcode 0xf (`FUN_00226f50`): the setup reader waits for the event object (`FUN_001962c0`), sets
    /// `DAT_0036bd10` = 1 (the timeline thread `FUN_00223a88` waits for it) and makes the tables (`FUN_00222cc0`).
    /// Without it the timeline never runs.
    pub begins: bool,
    /// Opcode 0x10 (`FUN_00226fa0`): `DAT_0036bd0c` = +4; the thread hands 2 * it to the event object +0x90
    /// (`FUN_00196348`), the length of the end fade (`FUN_00195ff8`: fader `FUN_0014a918` for kind row +1 of 1..3,
    /// length +0x90 * 0.5 (`DAT_0037b054`)).
    pub end_fade: Option<i16>,
    pub track: Track,
    /// Opcode counts of the commands the player does not port.
    pub other: BTreeMap<u16, usize>,
}

/// The opcodes `FUN_00223a88`'s switch runs; the others fall through without waiting (among them the setup
/// reader's 1, 8, 0xf, 0x10, 0x13, 0x1d, 0x1e, 0x23, 0x24, 0x2a, 0x2f, 0x32, 0x46, 0x4c, 0x4e; 0x2e and 0x44 are
/// in both switches).
pub const TIMELINE: &[u16] = &[
    2, 6, 9, 0xa, 0xb, 0xc, 0xd, 0xe, 0x12, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1f, 0x20, 0x21, 0x22, 0x27, 0x28, 0x29, 0x2b,
    0x2c, 0x2d, 0x2e, 0x30, 0x31, 0x33, 0x34, 0x35, 0x36, 0x37, 0x3a, 0x3b, 0x3c, 0x3d, 0x3f, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x47, 0x48,
    0x49, 0x4a, 0x4b, 0x4d, 0x4f, 0x50, 0x51, 0x52, 0x53,
];

/// The frame a timeline command waits for (`FUN_00224a90`): opcode 2 at +0x26, opcodes 6 and 0x1f none
/// (`FUN_0022a970`, `FUN_0022a4c0` take no frame), every other timeline command its first operand (+4)
/// [V for 0x12, 0x15, 0x2c, 0x2d, 0x44; inferred for the rest, which are not ported].
pub fn wait_frame(c: &EventCommand) -> Option<i32> {
    match c.opcode {
        2 => Some(le16(&c.data, 0x22) as i32),
        6 | 0x1f => None,
        o if TIMELINE.contains(&o) => Some(le16(&c.data, 0) as i32),
        _ => None,
    }
}

impl Event {
    pub fn parse(cmds: &[EventCommand]) -> Event {
        let mut e = Event { end: -1, track: Track::parse(cmds), ..Event::default() };
        for c in cmds {
            let d = &c.data;
            match c.opcode {
                0 => e.name = cstr(d, 6),
                8 => e.end = le16(d, 0) as i32,
                1 => e.actors.push(Actor { object: le16(d, 0) as u16, index: le16(d, 2), name: cstr(d, 4) }),
                0x24 => {
                    // +4 count, +6 / +8 the frames the block loads in, sub-records from +0xc: u16 kind, u16 size.
                    let n = le16(d, 0).max(0) as usize;
                    let mut p = 8;
                    for _ in 0..n {
                        let (kind, size) = (le16(d, p), le16(d, p + 2) as u16 as usize);
                        if size < 4 || p + size > d.len() {
                            break;
                        }
                        match kind {
                            0x25 => e.loads.push((le16(d, p + 4) as u16, le16(d, p + 6), cstr(d, p + 10))),
                            0x26 => e.voices.push(cstr(d, p + 4)),
                            0x39 => e.effects.push((le16(d, p + 4), cstr(d, p + 6))),
                            0x3e => e.layouts.push(cstr(d, p + 4)),
                            _ => {}
                        }
                        p += size;
                    }
                }
                2 => e.places.push(Place {
                    frame: le16(d, 0x22) as i32,
                    index: le16(d, 0x20),
                    gate: le16(d, 0),
                    pos: [le32f(d, 4), le32f(d, 8), le32f(d, 12)],
                    rot: [le32f(d, 16), le32f(d, 20), le32f(d, 24)],
                }),
                0x2c => e.motions.push(Motion::read(d)),
                0xf => e.begins = true,
                0x10 => e.end_fade = Some(le16(d, 0)),
                0x15 => e.lines.push(Line { frame: le16(d, 0) as i32, key: le16(d, 2), message: le16(d, 4) as u16, open: le16(d, 6) == 0 }),
                0x2d => e.windows.push(Window { frame: le16(d, 0) as i32, key: le16(d, 2), message: le16(d, 4) as u16, action: le16(d, 6), kind: le16(d, 8) }),
                0x44 => e.movies.push((le16(d, 0) as i32, cstr(d, 2))),
                6 | 0x13 | 0x31 | 0x46 | 0x4c | 0x3c => {}
                o => *e.other.entry(o).or_default() += 1,
            }
        }
        e
    }

    /// The model name of an objentry id as the actor table gives it (`FUN_002290e8`).
    pub fn model_of(&self, object: u16) -> Option<&str> {
        self.actors.iter().find(|a| a.object == object).map(|a| a.name.as_str())
    }
}

/// `FUN_0022cbb0`: the `.anb` path of motion `name` (`anm_<world>/<file>`) for model `model`: `anm/` + the
/// name's prefix past its first four characters + `/` + model + the rest + `.anb`.
pub fn anb_path(name: &str, model: &str) -> Option<String> {
    let slash = name.find('/')?;
    Some(format!("anm/{}/{}{}.anb", name.get(4..slash)?, model, &name[slash..]))
}

/// `DAT_00349d40`, the string the event's file formats take: the region (`us`) [inferred: the USA disc has
/// `voice/us/event/`, `event/layout/us/`, `movie/us/` and no `tt` folders there].
pub const REGION: &str = "us";

/// The files a load block's sub-records 0x26, 0x39 and 0x3e name, for region `world` (`DAT_00349d40`):
/// `FUN_0022bff8` (format 0x37f178 `voice/%s/event/` + name + 0x37f188 `.vag`), `FUN_00227d80` (0x37f090
/// `event/effect/` + name + 0x37f0a0 `.pax`), `FUN_002260b0` (0x37f038 `event/layout/%s/%s.2ld`).
pub fn voice_path(world: &str, name: &str) -> String {
    format!("voice/{world}/event/{name}.vag")
}
pub fn effect_path(name: &str) -> String {
    format!("event/effect/{name}.pax")
}
pub fn layout_path(world: &str, name: &str) -> String {
    format!("event/layout/{world}/{name}.2ld")
}

/// One step of the timeline cursor (`FUN_00223a88`) at frame `f`: the commands from `cursor` that run now,
/// stopping at the first whose frame has not come. Setup and unknown opcodes are passed over.
pub fn due(cmds: &[EventCommand], cursor: &mut usize, f: f32) -> Vec<usize> {
    let mut out = Vec::new();
    while let Some(c) = cmds.get(*cursor) {
        if let Some(w) = wait_frame(c)
            && (w as f32) > f
        {
            break;
        }
        if TIMELINE.contains(&c.opcode) {
            out.push(*cursor);
        }
        *cursor += 1;
    }
    out
}

// ---------------------------------------------------------------------------------------------------
// In the app

/// Events handed to the player (the area exits' triggers, `area_jump.rs`): (area the request was made in, event
/// tag, trigger argument).
static REQUESTS: Mutex<Vec<(String, String, u16)>> = Mutex::new(Vec::new());
/// The area `start` last saw loaded: a request is stamped with it, so a request made in one area never plays
/// in the next one (the event's tag is an entry of the area file that asked for it).
static LOADED: Mutex<String> = Mutex::new(String::new());

/// Asks the player to play event `tag` of the loaded area (a trigger with an event tag, `FUN_00196428`).
pub fn request(tag: &str, arg: u16) {
    let area = LOADED.lock().map(|a| a.clone()).unwrap_or_default();
    if let Ok(mut q) = REQUESTS.lock() {
        q.push((area, tag.to_string(), arg));
    }
}

/// The first request of `queue` that belongs to area `loaded`; requests of other areas are dropped (logged).
pub fn take_request(queue: &mut Vec<(String, String, u16)>, loaded: &str) -> Option<(String, u16)> {
    while !queue.is_empty() {
        let (area, tag, arg) = queue.remove(0);
        if area == loaded {
            return Some((tag, arg));
        }
        info!("event: {tag} was requested in {area:?}, {loaded} is loaded: dropped");
    }
    None
}

pub fn eventlog() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_EVENTLOG").is_ok_and(|v| v != "0"))
}

pub struct EventPlayerPlugin;

impl Plugin for EventPlayerPlugin {
    fn build(&self, app: &mut App) {
        use crate::schedule::{sim_running, Frame, Sim};
        app.init_resource::<EventPlayer>()
            .add_systems(FixedUpdate, (start, step).chain().in_set(Sim::EffectTick).run_if(sim_running))
            .add_systems(Update, (lock_pad.after(Frame::Pad).before(Frame::Hud), subtitles.in_set(Frame::Hud)));
    }
}

struct Spawned {
    root: Entity,
    body: Entity,
    /// The motion to start: (motion, cross-fade, start frame), in game frames.
    motion: Option<(Arc<kh2_anim::Slot>, f32, f32)>,
    /// The actor's motion slot (`FUN_0022ca18`) and the motion it restarts at a loop.
    clock: Option<(MotionClock, Arc<kh2_anim::Slot>)>,
}

struct Play {
    area: String,
    tag: String,
    cmds: Vec<EventCommand>,
    ev: Event,
    cursor: usize,
    ticks: u32,
    actors: HashMap<i16, Spawned>,
    /// Open subtitle lines (key, message), in open order.
    lines: Vec<(i16, u16)>,
    motions: HashMap<String, Option<Arc<kh2_anim::Slot>>>,
    /// The event kind row's flags (`evtp` +2 of kind `arg & 0x3f`, `FUN_00195898` -> `FUN_001d1128`).
    flags: u16,
    /// The windows opcode 0x2d opened and the counter hold of its action 2.
    windows: WindowSlots,
}

#[derive(Resource, Default)]
pub struct EventPlayer {
    play: Option<Play>,
    /// The event kind rows (`03system.bin` `evtp`, `struggle::event_kinds`).
    kinds: Option<Vec<(u8, u16)>>,
    /// HUD nodes the field-HUD hide turned off (shown again at the end).
    hidden: Vec<usize>,
    objs: Option<Vec<t::Objentry>>,
    rigs: HashMap<String, Option<Arc<Rig>>>,
    /// `KH2_EVENT` was taken.
    env_done: bool,
    /// The HUD node that draws the subtitle lines.
    node: Option<usize>,
}

impl EventPlayer {
    /// An event is playing (`DAT_0035e3a4` set: host 1.34 `event_active`).
    pub fn playing(&self) -> bool {
        self.play.is_some()
    }

    /// `FUN_00195ae0`: kind flag bit 3 clear -> `FUN_00196280` (`FUN_001b7ce8(0)`, `(1)`: players 0 and 1 lose
    /// control) until `FUN_00196170` -> `FUN_001962a0` gives it back.
    pub fn locks_control(&self) -> bool {
        self.play.as_ref().is_some_and(|p| kind_locks_control(p.flags))
    }

    /// `FUN_00195ae0`: kind flag bit 2 clear -> `FUN_00196760` -> `FUN_0014ef48` hides the field HUD (draw tags 3
    /// to 6, `FUN_0013da78`) until `FUN_00196170` -> `FUN_00196790`.
    pub fn hides_hud(&self) -> bool {
        self.play.as_ref().is_some_and(|p| kind_hides_hud(p.flags))
    }
}

/// Kind flag bit 3 clear: the event takes the players' control (`FUN_00195ae0` -> `FUN_00196280`).
pub fn kind_locks_control(flags: u16) -> bool {
    flags & 8 == 0
}

/// Kind flag bit 2 clear: the event hides the field HUD (`FUN_00195ae0` -> `FUN_00196760`).
pub fn kind_hides_hud(flags: u16) -> bool {
    flags & 4 == 0
}

/// The flags of event kind `arg & 0x3f` (`FUN_00195898`: `FUN_001d1128` searches the `evtp` rows by their kind
/// byte, `FUN_0030ff40`). `None` when the table has no such kind.
pub fn kind_flags(kinds: &[(u8, u16)], arg: u16) -> Option<u16> {
    let k = (arg & 0x3f) as u8;
    kinds.iter().find(|(x, _)| *x == k).map(|(_, f)| *f)
}

/// Starts a requested event: `request` (a trigger) or the harness `KH2_EVENT=<tag>` (the loaded area's event,
/// `KH2_EVENT_AT=<app seconds>`, default 2).
fn start(time: Res<Time<Real>>, rec: Res<AreaRecord>, disc: Res<Disc>, mut player: ResMut<EventPlayer>) {
    let Some(run) = rec.run.as_ref() else { return };
    if let Ok(mut a) = LOADED.lock()
        && *a != run.name
    {
        *a = run.name.clone();
    }
    let mut next = REQUESTS.lock().ok().and_then(|mut q| take_request(&mut q, &run.name));
    if next.is_none() && !player.env_done {
        let at = std::env::var("KH2_EVENT_AT").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(2.0);
        if let Ok(tag) = std::env::var("KH2_EVENT")
            && time.elapsed_secs() >= at
        {
            player.env_done = true;
            // The argument: `KH2_EVENT_ARG`, else the one the loaded scripts' trigger for this tag passes (tt02
            // `102`: arg 2), else 0.
            let arg = std::env::var("KH2_EVENT_ARG").ok().and_then(|v| v.parse::<u16>().ok()).or_else(|| run.script.triggers.iter().find_map(|t| t.event_tag().filter(|(e, _)| *e == tag).map(|(_, a)| a))).unwrap_or(0);
            next = Some((tag, arg));
        }
    }
    let Some((tag, arg)) = next else { return };
    if player.play.is_some() {
        info!("event: {tag} requested while an event plays: dropped");
        return;
    }
    let Some(bytes) = run.ard.events.get(&tag) else {
        warn!("event: {} has no event {tag}", run.name);
        return;
    };
    let Some(cmds) = kh2_ard::parse_event(bytes) else {
        warn!("event: {} {tag} does not parse", run.name);
        return;
    };
    let ev = Event::parse(&cmds);
    let kinds = player.kinds.get_or_insert_with(|| disc.read("03system.bin").map(|s| crate::struggle::event_kinds(&s)).unwrap_or_default());
    // FUN_001d1128 answers no row for a kind the table lacks [inferred: then nothing is held back, flags 0xffff].
    let flags = kind_flags(kinds, arg);
    info!(
        "event: kind {:#x} flags {}: control {}, field HUD {} (FUN_00195ae0; HUD list record 0xc FUN_001bef48 and DAT_00356f1c bit 4 FUN_0014c748 are the pause-menu record and the play clock, not ported)",
        arg & 0x3f,
        flags.map_or("none (no evtp row)".to_string(), |f| format!("{f:#06x}")),
        if flags.is_some_and(kind_locks_control) { "locked" } else { "kept" },
        if flags.is_some_and(kind_hides_hud) { "hidden" } else { "kept" }
    );
    let flags = flags.unwrap_or(0xffff);
    let world = REGION;
    info!(
        "event: {tag} load blocks name {} voice(s) {:?}, {} effect(s) {:?}, layouts {:?} (FUN_0022c070, FUN_00227df0, FUN_00226100: not played, not drawn); op 0xf {} (FUN_00226f50); end fade {:?} frames (op 0x10, FUN_00195ff8: not ported)",
        ev.voices.len(),
        ev.voices.iter().take(3).map(|v| voice_path(world, v)).collect::<Vec<_>>(),
        ev.effects.len(),
        ev.effects.iter().map(|(_, e)| effect_path(e)).collect::<Vec<_>>(),
        ev.layouts.iter().map(|l| layout_path(world, l)).collect::<Vec<_>>(),
        if ev.begins { "present: the timeline runs" } else { "absent: the timeline never runs" },
        ev.end_fade
    );
    info!(
        "event: {} {tag} ({}, arg {arg}) starts: {} commands, end frame {}, {} actor(s) {:?}, {} motion load(s), {} placement(s), {} motion(s), {} subtitle command(s), {} window command(s), camera {} record(s) {} cut(s), movies {:?}; not ported opcodes {:x?}",
        run.name,
        ev.name,
        cmds.len(),
        ev.end,
        ev.actors.len(),
        ev.actors.iter().map(|a| (a.index, a.name.as_str())).collect::<Vec<_>>(),
        ev.loads.len(),
        ev.places.len(),
        ev.motions.len(),
        ev.lines.len(),
        ev.windows.len(),
        ev.track.cams.len(),
        ev.track.cuts.len(),
        ev.movies,
        ev.other
    );
    player.play = Some(Play { area: run.name.clone(), tag, cmds, ev, cursor: 0, ticks: 0, actors: HashMap::new(), lines: Vec::new(), motions: HashMap::new(), flags, windows: WindowSlots::default() });
}

fn view(s: super::Shot) -> View {
    View { look: s.look, pos: s.pos, fov: s.fov, roll: s.roll }
}

/// One game frame of the event: the counter, the timeline cursor, the camera, the end.
#[allow(clippy::too_many_arguments)]
fn step(
    mut commands: Commands,
    disc: Res<Disc>,
    mut player: ResMut<EventPlayer>,
    mut cam: ResMut<CamEvent>,
    mut anims: Query<&mut Animator>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut msg: Option<ResMut<crate::hud::hud_msgwin::ScriptWindow>>,
) {
    let player = &mut *player;
    let Some(p) = player.play.as_mut() else { return };
    // Opcode 0x2d action 2 (`FUN_00222528`): the counter stands still until the window is at state 3 (`FUN_00222728`).
    if let Some(m) = msg.as_ref()
        && !p.windows.tick(m.waiting, m.closed)
    {
        return;
    }
    p.ticks += 1;
    // FUN_00222728: the counter is the frame count times 0.5 (no movie is played, so never the movie's frame).
    let f = p.ticks as f32 * 0.5;
    let fi = f.floor() as i32;
    let log = eventlog();
    // FUN_00223a88 waits for DAT_0036bd10, which only opcode 0xf sets (FUN_00226f50).
    let due_now = if p.ev.begins { due(&p.cmds, &mut p.cursor, f) } else { Vec::new() };
    for i in due_now {
        let c = &p.cmds[i];
        let d = &c.data;
        let at = wait_frame(c).map_or("-".to_string(), |w| w.to_string());
        match c.opcode {
            0x2c => {
                let m = Motion::read(d);
                let Some(actor) = p.ev.actors.iter().find(|a| a.index == m.index).cloned() else {
                    info!("event: frame {fi} (cmd frame {at}): op 0x2c actor {} is not in the actor table", m.index);
                    continue;
                };
                // FUN_0022cd90: the actor's object is made at its first motion (FUN_0016e8c0, objentry id).
                if let std::collections::hash_map::Entry::Vacant(slot) = p.actors.entry(m.index) {
                    let objs = player.objs.get_or_insert_with(|| disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default());
                    let model = objs.iter().find(|r| r.object_id == actor.object as u32).map(|r| r.model_name.clone());
                    let rig = model.as_ref().and_then(|model| {
                        player
                            .rigs
                            .entry(model.clone())
                            .or_insert_with(|| {
                                let m = disc.read(&format!("obj/{model}.mdlx")).and_then(|d| kh2_model::mdlx(&d))?;
                                let mset = disc.read(&format!("obj/{model}.mset")).and_then(|d| kh2_anim::Mset::parse(&d)).unwrap_or(kh2_anim::Mset { slots: Vec::new() });
                                Some(Arc::new(Rig { model: m, mset, apdx: None }))
                            })
                            .clone()
                    });
                    match rig {
                        Some(rig) => {
                            let root = commands.spawn((Transform::default(), Visibility::Hidden, AreaPart, EventActor)).id();
                            let body = spawn_skinned(&mut commands, root, rig, &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
                            slot.insert(Spawned { root, body, motion: None, clock: None });
                            info!("event: frame {fi}: actor {} {} (object {:#x}, model {}) made (op 0x2c, FUN_0016e8c0)", m.index, actor.name, actor.object, model.unwrap_or_default());
                        }
                        None => {
                            warn!("event: actor {} {} (object {:#x}): model {model:?} not readable", m.index, actor.name, actor.object);
                            continue;
                        }
                    }
                }
                // FUN_0022cd90: the model name by the actor's objentry id (FUN_002290e8), then the record by name.
                let path = anb_path(&m.name, p.ev.model_of(actor.object).unwrap_or(&actor.name));
                let slot = p
                    .motions
                    .entry(m.name.clone() + "|" + &actor.name)
                    .or_insert_with(|| {
                        let path = path.clone()?;
                        // The `anm/<world>/` files of talking motions are under `anm/us/<world>/` on the USA disc
                        // [inferred: the region's lip-sync copy; the loader's remap was not read].
                        let alt = path.replacen("anm/", "anm/us/", 1);
                        let file = disc.read(&path).or_else(|| disc.read(&alt))?;
                        let bar = kh2_bar::parse(&file).ok()?;
                        let motion = bar.iter().find(|x| x.kind == 9).and_then(|x| kh2_anim::Motion::parse(x.data));
                        let triggers = bar.iter().find(|x| x.kind == 16).map(|x| kh2_anim::Triggers::parse(x.data)).unwrap_or_default();
                        Some(Arc::new(kh2_anim::Slot { tag: m.name.clone(), motion, triggers }))
                    })
                    .clone();
                let Some(sp) = p.actors.get_mut(&m.index) else { continue };
                match slot {
                    Some(slot) => {
                        // FUN_0017dd38(blend = 2 * +0x10, start = 2 * +8): game frames. Applied below (a body
                        // made this tick is in the world from the next).
                        sp.motion = Some((slot.clone(), 2.0 * m.blend as f32, 2.0 * m.start as f32));
                        // FUN_0022c9b0: the actor's motion slot gets the loop (+0xa, +0xc) and the counter.
                        sp.clock = Some((MotionClock::new(&m), slot));
                        if log {
                            let lp = if m.loop_end == -1 { String::new() } else { format!(", loops {}..{} (FUN_0022ca18)", m.loop_start, m.loop_end) };
                            info!("event: frame {fi} (cmd frame {at}): op 0x2c actor {} {} plays {} (start {}, blend {}{lp})", m.index, actor.name, path.unwrap_or_default(), m.start, m.blend);
                        }
                    }
                    None => info!("event: frame {fi}: op 0x2c actor {} motion {:?} not found", m.index, path),
                }
            }
            2 => {
                let pl = Place { frame: le16(d, 0x22) as i32, index: le16(d, 0x20), gate: le16(d, 0), pos: [le32f(d, 4), le32f(d, 8), le32f(d, 12)], rot: [le32f(d, 16), le32f(d, 20), le32f(d, 24)] };
                if pl.gate != 0 {
                    continue;
                }
                match p.actors.get(&pl.index) {
                    Some(s) => {
                        let at_app = game_to_app(pl.pos);
                        let tf = Transform::from_translation(at_app * SCALE).with_rotation(Quat::from_rotation_y(app_yaw(pl.rot[1].to_radians())));
                        commands.entity(s.root).insert((tf, Visibility::Inherited));
                        if log {
                            info!("event: frame {fi} (cmd frame {at}): op 2 actor {} at ({:.0}, {:.0}, {:.0}) yaw {:.0} deg", pl.index, at_app.x, at_app.y, at_app.z, pl.rot[1]);
                        }
                    }
                    None => info!("event: frame {fi}: op 2 actor {} has no object yet", pl.index),
                }
            }
            0x15 => {
                let l = Line { frame: le16(d, 0) as i32, key: le16(d, 2), message: le16(d, 4) as u16, open: le16(d, 6) == 0 };
                if l.open {
                    if p.lines.len() < 32 {
                        p.lines.push((l.key, l.message));
                    }
                } else {
                    p.lines.retain(|&(k, m)| (k, m) != (l.key, l.message));
                }
                if log {
                    info!("event: frame {fi} (cmd frame {at}): op 0x15 subtitle {} key {} message {:#x}", if l.open { "opens" } else { "closes" }, l.key, l.message);
                }
            }
            0x2d => {
                let w = Window { frame: le16(d, 0) as i32, key: le16(d, 2), message: le16(d, 4) as u16, action: le16(d, 6), kind: le16(d, 8) };
                let mut note = "";
                match w.action {
                    0 => {
                        p.windows.open(w.key, w.message);
                        if let Some(m) = msg.as_mut() {
                            m.open = Some((w.message as u32, w.kind.clamp(0, 1) as usize));
                            m.closed = false;
                            m.waiting = false;
                        }
                    }
                    1 => {
                        // FUN_002275a8: the slot's window gets the close method (vtable +0x20), the slot is cleared.
                        if p.windows.close(w.key, w.message) {
                            if let Some(m) = msg.as_mut() {
                                m.close = true;
                            }
                            note = " (closed, no wait)";
                        } else {
                            note = " (no such window open)";
                        }
                    }
                    2 => {
                        note = if p.windows.wait(w.key, w.message) { " (counter held until the window is at state 3)" } else { " (no such window open: no wait)" };
                    }
                    _ => note = " (action not known)",
                }
                if log || w.action != 0 {
                    info!("event: frame {fi} (cmd frame {at}): op 0x2d window kind {} key {} message {:#x} action {}{}", w.kind, w.key, w.message, w.action, note);
                } else {
                    info!("event: frame {fi} (cmd frame {at}): op 0x2d window kind {} key {} message {:#x} action {}", w.kind, w.key, w.message, w.action);
                }
            }
            6 => {
                if log {
                    info!("event: frame {fi}: op 6 camera cut {} frames {}..{}", le16(d, 0), le16(d, 2), le16(d, 4));
                }
            }
            0xd => {
                if log {
                    info!("event: frame {fi} (cmd frame {at}): op 0xd actor {} model instance (+0x670) method +0x34 with the table at +0xa {:?}: not ported [GAP: the method, FUN_00228b48 -> FUN_0011c550]", le16(d, 4), (0..8).map(|k| le16(d, 6 + 2 * k)).collect::<Vec<_>>());
                }
            }
            0x44 => info!("event: frame {fi} (cmd frame {at}): op 0x44 movie movie/{REGION}/{}.pss not played (no movie player) [GAP]", cstr(d, 2)),
            o => {
                if log {
                    info!("event: frame {fi} (cmd frame {at}): op {o:#x} not ported {:02x?}", &d[..d.len().min(16)]);
                }
            }
        }
    }
    for s in p.actors.values_mut() {
        if let Some((slot, blend, start)) = s.motion.take() {
            match anims.get_mut(s.body) {
                Ok(mut a) => {
                    a.play_external(slot, blend);
                    a.frame += start;
                }
                Err(_) => s.motion = Some((slot, blend, start)),
            }
        }
        // FUN_0022ca18 (every frame, from the setup reader's frame path): the motion slots' loops. The position
        // put back at a loop (`FUN_0016a618` with the place `FUN_0016b960` kept at the slot's first frame, unless
        // `FUN_001beb00`) is not ported: the app's event actors take no root motion [inferred].
        if s.motion.is_none()
            && let Some((clock, slot)) = s.clock.as_mut()
            && let Some(at) = clock.tick()
            && let Ok(mut a) = anims.get_mut(s.body)
        {
            a.play_external(slot.clone(), 0.0);
            a.frame += at;
            if log {
                info!("event: frame {fi}: motion {} loops back to frame {at} (FUN_0022ca18, {}..{})", slot.tag, clock.loop_start, clock.loop_end);
            }
        }
    }
    if p.ev.track.has_camera()
        && let Some(s) = p.ev.track.at(f)
    {
        cam.track = Some((view(s), p.ev.track.collide.is_some()));
    }
    if p.ev.end >= 0 && fi >= p.ev.end || p.ev.end < 0 && p.cursor >= p.cmds.len() {
        let p = player.play.take().unwrap();
        for s in p.actors.values() {
            commands.entity(s.root).despawn();
        }
        info!("event: {} {} over at frame {fi} (end frame {}, FUN_00222668 -> FUN_002235c0): {} actor(s) removed, control back to the player", p.area, p.tag, p.ev.end, p.actors.len());
    }
}

/// An object an event made (removed when the event ends).
#[derive(Component)]
pub struct EventActor;

/// The field-HUD hide of an event whose kind has flag bit 2 clear (`FUN_00195ae0` -> `FUN_00196760` ->
/// `FUN_0014ef48` -> `FUN_0013da78`: draw tags 3 to 6), lifted when it ends (`FUN_00196170` -> `FUN_00196790`):
/// every visible HUD node but the subtitle line is turned off while the event plays and the ones turned off
/// here are shown again after [inferred: which of the app's HUD modules are the tags 3 to 5 was not traced, as
/// in `hud_mission::field_hud_hide`]. Runs at the end of the HUD chain (`hud.rs`), so it sees what the HUD
/// modules showed this frame.
pub(crate) fn field_hud_hide(mut player: ResMut<EventPlayer>, mut scene: ResMut<HudScene>) {
    let player = &mut *player;
    if player.hides_hud() {
        for (i, n) in scene.nodes.iter_mut().enumerate() {
            if n.visible && Some(i) != player.node {
                n.visible = false;
                if !player.hidden.contains(&i) {
                    player.hidden.push(i);
                }
            }
        }
    } else if !player.hidden.is_empty() {
        let n = player.hidden.len();
        for i in player.hidden.drain(..) {
            if let Some(node) = scene.nodes.get_mut(i) {
                node.visible = true;
            }
        }
        if eventlog() {
            info!("event: field HUD back ({n} node(s), FUN_00196790)");
        }
    }
}

/// The pad is the event's while it plays when the kind takes control (`kind_locks_control`).
/// With `KH2_EVENTLOG=1` Sora's place is logged while the event plays and six seconds after its end (shows
/// that the pad drives him again).
fn lock_pad(
    time: Res<Time<Real>>,
    player: Res<EventPlayer>,
    mut pad: ResMut<crate::sora::Pad>,
    sora: Query<&Transform, With<crate::sora::Sora>>,
    mut ended: Local<Option<(f32, Vec3, bool)>>,
    mut began: Local<Option<Vec3>>,
) {
    let at = sora.single().map(|t| t.translation / SCALE).unwrap_or_default();
    if player.playing() {
        if player.locks_control() {
            pad.lock();
        }
        began.get_or_insert(at);
        *ended = Some((time.elapsed_secs(), at, false));
        return;
    }
    if let Some((t, end, logged)) = ended.as_mut()
        && !*logged
        && time.elapsed_secs() - *t >= 6.0
    {
        *logged = true;
        let during = began.take().map_or(0.0, |b| (*end - b).length());
        if eventlog() {
            info!("event: Sora moved {during:.0} cm while the event played; 6 s after its end he is at ({:.0}, {:.0}, {:.0}), {:.0} cm from where the event left him", at.x, at.y, at.z, (at - *end).length());
        }
    }
}

/// The subtitle lines (class 0x1aae60, docs/hud.md: font 4, colour 3, at (256, 365), alignment 4 [inferred
/// there]): the open lines' messages, the latest at the bottom.
fn subtitles(mut player: ResMut<EventPlayer>, mut scene: ResMut<HudScene>) {
    let lines: Vec<u16> = player.play.as_ref().map(|p| p.lines.iter().map(|l| l.1).collect()).unwrap_or_default();
    let node = match player.node {
        Some(n) => n,
        None => {
            if lines.is_empty() {
                return;
            }
            let mut n = HudNode::new(usize::MAX, None);
            n.pos = [256.0, 365.0];
            let id = scene.add(n);
            player.node = Some(id);
            id
        }
    };
    let label = scene.text.as_ref().and_then(|t| {
        let mut tokens = Vec::new();
        for (i, id) in lines.iter().enumerate() {
            if i > 0 {
                tokens.push(kh2_image::text::Token::Newline);
            }
            tokens.extend(t.lookup(*id as u32)?);
        }
        let style = Style { font: 4, color: t.color(3), align: 4, world: t.world, ..Style::default() };
        (!tokens.is_empty()).then_some(Label { tokens, style, offset: [0.0, 0.0] })
    });
    let n = &mut scene.nodes[node];
    let changed = n.label.as_ref().map(|l| &l.tokens) != label.as_ref().map(|l| &l.tokens);
    if changed {
        n.label = label;
        n.label_time = 0.0;
    }
    n.visible = n.label.is_some();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(opcode: u16, data: &[u8]) -> EventCommand {
        EventCommand { opcode, data: data.to_vec() }
    }

    fn h(v: i16) -> [u8; 2] {
        v.to_le_bytes()
    }

    #[test]
    fn opcode_2d_close_clears_the_slot_and_wait_holds_the_counter() {
        let mut w = WindowSlots::default();
        w.open(1, 0x4cc6);
        // Action 2 on an open window holds the counter until the window is at state 3.
        assert!(w.wait(1, 0x4cc6));
        assert!(!w.tick(false, false));
        assert!(!w.tick(false, false));
        assert!(w.tick(true, false), "state 3 lets the counter go");
        assert!(!w.hold);
        assert!(w.tick(false, false));
        // Action 1 closes without waiting: the slot is gone, so a wait after it finds nothing.
        assert!(w.close(1, 0x4cc6));
        assert!(!w.wait(1, 0x4cc6));
        assert!(w.tick(false, false));
        assert!(!w.close(1, 0x4cc6));
        // A window that was closed (or never opened) does not hold the counter for ever.
        w.open(2, 7);
        assert!(w.wait(2, 7));
        assert!(w.tick(false, true));
    }

    #[test]
    fn the_timeline_cursor_waits_for_each_commands_frame_in_file_order() {
        let line = |frame: i16, key: i16, open: bool| {
            let mut d = Vec::new();
            d.extend(h(frame));
            d.extend(h(key));
            d.extend(h(0x4cc6));
            d.extend(h(!open as i16));
            cmd(0x15, &d)
        };
        // Setup commands (1, 8) pass; 0x15 at 10 waits; a later command with an earlier frame runs right after.
        let cmds = vec![cmd(8, &h(40)), line(10, 0, true), line(5, 1, true), line(30, 0, false)];
        let mut cur = 0;
        assert!(due(&cmds, &mut cur, 9.5).is_empty());
        assert_eq!(cur, 1, "the cursor stops at the first command whose frame has not come");
        assert_eq!(due(&cmds, &mut cur, 10.0), vec![1, 2]);
        assert_eq!(due(&cmds, &mut cur, 29.5), Vec::<usize>::new());
        assert_eq!(due(&cmds, &mut cur, 30.0), vec![3]);
        let e = Event::parse(&cmds);
        assert_eq!(e.end, 40);
        assert_eq!(e.lines[0], Line { frame: 10, key: 0, message: 0x4cc6, open: true });
        assert!(!e.lines[2].open);
    }

    #[test]
    fn actors_placements_and_motions_read_their_fields() {
        let mut a = Vec::new();
        a.extend(h(0x5a));
        a.extend(h(0));
        a.extend(b"p_ex110\0");
        let mut m = vec![0u8; 14];
        m[0..2].copy_from_slice(&h(37));
        m[4..6].copy_from_slice(&h(3));
        m[10..12].copy_from_slice(&h(0));
        m[12..14].copy_from_slice(&h(5));
        m.extend(b"anm_tt/ETT99901E\0\0");
        let mut p = vec![0u8; 36];
        p[4..8].copy_from_slice(&7.0f32.to_le_bytes());
        p[12..16].copy_from_slice(&339.0f32.to_le_bytes());
        p[20..24].copy_from_slice(&(-170.0f32).to_le_bytes());
        p[0x20..0x22].copy_from_slice(&h(0));
        p[0x22..0x24].copy_from_slice(&h(37));
        let e = Event::parse(&[cmd(1, &a), cmd(0x2c, &m), cmd(2, &p)]);
        assert_eq!(e.actors, vec![Actor { object: 0x5a, index: 0, name: "p_ex110".into() }]);
        assert_eq!(e.motions[0], Motion { frame: 37, index: 0, start: 3, blend: 5, name: "anm_tt/ETT99901E".into(), loop_start: 0, loop_end: 0 });
        assert_eq!(e.places[0], Place { frame: 37, index: 0, gate: 0, pos: [7.0, 0.0, 339.0], rot: [0.0, -170.0, 0.0] });
        assert_eq!(wait_frame(&cmd(2, &p)), Some(37));
        assert_eq!(e.model_of(0x5a), Some("p_ex110"));
        assert_eq!(anb_path("anm_tt/ETT99901E", "p_ex110").as_deref(), Some("anm/tt/p_ex110/ETT99901E.anb"));
    }

    /// WP-E6b: the kind row is found by its kind byte (`arg & 0x3f`); bit 3 clear takes control, bit 2 clear
    /// hides the field HUD. On the disc: the Struggle start kinds 0xd..0xf (`struggle.rs`) and the row of kind 0
    /// that `KH2_EVENT` (arg 0) uses.
    #[test]
    fn the_event_kind_row_decides_control_and_the_field_hud() {
        let kinds = [(0u8, 0x0000u16), (0xd, 0x60), (0xe, 0x44), (0xf, 0x64)];
        assert_eq!(kind_flags(&kinds, 0x8d), Some(0x60), "bit 7 of the argument is not part of the kind");
        assert_eq!(kind_flags(&kinds, 0x11), None);
        assert!(kind_locks_control(0x60) && kind_hides_hud(0x60));
        assert!(kind_locks_control(0x44) && !kind_hides_hud(0x44));
        assert!(!kind_locks_control(0x08) && kind_hides_hud(0x08));
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let rows = crate::struggle::event_kinds(&disc.read("03system.bin").expect("03system.bin"));
        eprintln!("evtp rows (kind, flags): {:x?}", rows);
        assert_eq!(kind_flags(&rows, 0xd), Some(0x60));
        let k0 = kind_flags(&rows, 0);
        eprintln!("kind 0: {k0:x?}");
    }

    /// The disc's tt01 short events (`100`, `200`, `400`, `600`) are movies with subtitle lines: opcode 0x44
    /// names the movie, every 0x15 opens and closes in pairs, the end frame lies past the last line.
    #[test]
    fn the_tt01_short_events_are_movies_with_subtitle_lines() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let (ard, _) = kh2_ard::Area::parse(&disc.read("ard/tt01.ard").expect("tt01")).expect("area");
        for (tag, movie) in [("100", "opn"), ("200", "me2"), ("400", "me4"), ("600", "me6")] {
            let bytes = &ard.events[tag];
            assert!(bytes.len() < 600, "{tag}: {} bytes", bytes.len());
            let e = Event::parse(&kh2_ard::parse_event(bytes).expect("event"));
            assert_eq!(e.name, format!("tt_event_{tag}"));
            assert_eq!(e.movies, vec![(0, movie.to_string())]);
            assert!(e.actors.is_empty() && e.places.is_empty() && e.motions.is_empty() && !e.track.has_camera());
            let opens = e.lines.iter().filter(|l| l.open).count();
            assert_eq!(opens * 2, e.lines.len(), "{tag}: lines open and close in pairs");
            assert!(e.lines.iter().all(|l| l.frame < e.end), "{tag}");
            eprintln!("tt01 {tag}: movie {movie}, {opens} subtitle lines, end frame {}, other {:x?}", e.end, e.other);
        }
    }

    /// tt02 `102` (the first Roxas morning event): its actors, their motions found on the disc, placements
    /// and the camera cuts.
    #[test]
    fn the_tt02_event_102_actors_have_their_models_and_motions_on_the_disc() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let (ard, _) = kh2_ard::Area::parse(&disc.read("ard/tt02.ard").expect("tt02")).expect("area");
        let e = Event::parse(&kh2_ard::parse_event(&ard.events["102"]).expect("event"));
        assert_eq!(e.actors.len(), 9);
        assert_eq!(e.actors[0], Actor { object: 0x5a, index: 0, name: "p_ex110".into() });
        assert!(e.track.has_camera() && e.track.cuts.len() == 23);
        let mut found = 0;
        for m in &e.motions {
            let a = e.actors.iter().find(|a| a.index == m.index).expect("actor of a motion");
            let path = anb_path(&m.name, &a.name).expect("path");
            if disc.read(&path).or_else(|| disc.read(&path.replacen("anm/", "anm/us/", 1))).is_some() {
                found += 1;
            } else {
                eprintln!("not on the disc: {path}");
            }
        }
        eprintln!("tt02 102: {} motions, {found} files found; end frame {}; other {:x?}", e.motions.len(), e.end, e.other);
        assert_eq!(found, e.motions.len());
    }
    /// The prologue's "Defeat Seifer!" (TT04_MS500, group b_45): the start event of the mission file (layout
    /// record 1) and every other event entry (BAR type 0x16) of it, with their actors and placements.
    #[test]
    fn the_seifer_mission_events_have_no_actors() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let file = disc.read("msn/us/TT04_MS500.bar").expect("TT04_MS500");
        let entries = kh2_bar::parse(&file).expect("BAR");
        let mut events = 0;
        for (i, e) in entries.iter().enumerate() {
            eprintln!("TT04_MS500 entry {i}: {} kind {:#x} {} bytes", e.tag, e.kind, e.data.len());
            let Some(cmds) = kh2_ard::parse_event(e.data).filter(|c| c.first().is_some_and(|c| c.opcode == 0)) else { continue };
            events += 1;
            let ev = Event::parse(&cmds);
            eprintln!(
                "  event {}: end {}, actors {:?}, places {:?}, motions {}, lines {}, cuts {}, other {:x?}",
                ev.name, ev.end, ev.actors, ev.places, ev.motions.len(), ev.lines.len(), ev.track.cuts.len(), ev.other
            );
            assert!(ev.actors.is_empty() && ev.places.is_empty() && ev.motions.is_empty(), "{:?}", ev.actors);
        }
        assert!(events >= 1);
    }

    #[test]
    fn a_request_made_in_another_area_is_dropped() {
        let mut q = vec![("tt03".to_string(), "605".to_string(), 5), ("tt02".to_string(), "102".to_string(), 2)];
        assert_eq!(take_request(&mut q, "tt02"), Some(("102".to_string(), 2)), "tt03's 605 is dropped in tt02");
        assert!(q.is_empty());
        let mut q = vec![("tt03".to_string(), "605".to_string(), 5)];
        assert_eq!(take_request(&mut q, "tt04"), None);
        assert!(q.is_empty());
    }

    /// `FUN_0022ca18`: the counter starts at 2 * (start - blend); past 2 * end it goes to 2 * loop start plus the
    /// overshoot and the motion restarts there; -1 never loops.
    #[test]
    fn motion_slots_loop_past_their_end_frame() {
        let m = Motion { frame: 0, index: 0, start: 3, blend: 1, name: String::new(), loop_start: 0, loop_end: 5 };
        let mut c = MotionClock::new(&m);
        assert_eq!(c.counter, 4.0);
        let mut restarts = Vec::new();
        for _ in 0..20 {
            if let Some(f) = c.tick() {
                restarts.push(f);
            }
        }
        // 4, 5, .., 10 counts; 11 > 10 goes to 0 + 1 = 1; then 1..10, 11 -> 1 again.
        assert_eq!(restarts, vec![1.0, 1.0]);
        let mut n = MotionClock::new(&Motion { loop_end: -1, ..m });
        assert!((0..100).all(|_| n.tick().is_none()));
        assert_eq!(voice_path(REGION, "tt10203or"), "voice/us/event/tt10203or.vag");
        assert_eq!(effect_path("tt_event_318"), "event/effect/tt_event_318.pax");
        assert_eq!(layout_path(REGION, "tt_telop"), "event/layout/us/tt_telop.2ld");
    }

    /// The Twilight Town events (tt02, tt03, tt04): every one has opcode 0xf; the files their load blocks name
    /// (voices 0x26, effects 0x39, layouts 0x3e) are on the disc; some motions loop (0x2c +0xc not -1).
    #[test]
    fn twilight_town_event_load_records_name_files_on_the_disc() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let (mut events, mut voices, mut effects, mut layouts, mut loops, mut missing) = (0, 0, 0, 0, 0, Vec::new());
        for area in ["tt02", "tt03", "tt04"] {
            let (ard, _) = kh2_ard::Area::parse(&disc.read(&format!("ard/{area}.ard")).expect("area")).expect("area");
            for (tag, bytes) in &ard.events {
                let Some(cmds) = kh2_ard::parse_event(bytes) else { continue };
                let e = Event::parse(&cmds);
                events += 1;
                assert!(e.begins, "{area} {tag} has no opcode 0xf");
                let mut check = |path: String| {
                    if disc.read(&path).is_none() {
                        missing.push(path);
                    }
                };
                for v in &e.voices {
                    check(voice_path(REGION, v));
                }
                for (_, x) in &e.effects {
                    check(effect_path(x));
                }
                for l in &e.layouts {
                    check(layout_path(REGION, l));
                }
                voices += e.voices.len();
                effects += e.effects.len();
                layouts += e.layouts.len();
                loops += e.motions.iter().filter(|m| m.loop_end != -1).count();
            }
        }
        eprintln!("{events} events: {voices} voices, {effects} effects, {layouts} layouts, {loops} looping motions; not on the disc: {missing:?}");
        assert!(voices > 0 && effects > 0 && layouts > 0 && loops > 0);
        assert!(missing.is_empty(), "{missing:?}");
    }
}