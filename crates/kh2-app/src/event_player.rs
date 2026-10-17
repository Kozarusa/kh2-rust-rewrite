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
    /// The model's mesh groups (`mesh_groups`).
    groups: Option<Vec<u8>>,
    /// Mesh draw flags opcode 0xd set, applied once the body's meshes exist.
    parts: Option<Vec<u8>>,
    /// The place opcode 2 gave the actor last (the object's +0x540 position and its rotation).
    place: Option<Transform>,
    /// The motion slot's kept place (row +0x20, `FUN_0016b960` at the row's first step, +0x30 = 1) and whether
    /// that first step is still to come.
    kept: Option<Vec3>,
    first: bool,
    /// Object +0x10c bit 0x80, set by opcode 0x4d (`hide_by_tag`) while the object was in the list.
    hidden: bool,
}

/// The model resource tag (resource +0x28, `FUN_0016e430` -> `FUN_0016f9c0`) the event's own actors carry:
/// `FUN_00229198` (load record 0x38) and opcode 0x2c make them with the objentry id or'ed with 0x40000000, and
/// `FUN_0016ef80` answers yes for such an id unless its objentry type is 7 or 0x16, which gives -200.
pub const EVENT_ACTOR_TAG: i32 = -200;

/// The limit opcode 0x4d passes (`FUN_00226e80`: `FUN_001b7fa8(2000)` / `FUN_001b8040(2000)`).
pub const HIDE_TAG_LIMIT: i32 = 2000;

/// The tag `FUN_0016e430` gives an object's model resource when its caller passes -1: -200 for an object the event
/// made (`event_made`, id with 0x40000000; objentry types 7 and 0x16 excepted), else the word of the table at
/// 0x37a278 for the objentry `ObjectType` (`words`, 24 of them).
///
/// Unported exceptions of the original, both on the object list walk of opcode 0x4d: `FUN_0016ef80` answers yes
/// only when `FUN_0016e208` finds the objentry row, so an id with no row gets the table word [inferred: read from
/// the row's type here, which the caller already has]; and `FUN_001b7fa8` / `FUN_001b8040` skip every object with
/// object flag +0x588 bit 0x20 (not set by anything the app has: its event actors never carry it, so no actor is
/// skipped). The app's event actors are all `EventActor`s, none of type 7 or 0x16.
#[cfg(test)]
pub fn resource_tag(words: &[i32], object_type: u8, event_made: bool) -> Option<i32> {
    if event_made && !matches!(object_type, 7 | 0x16) {
        return Some(EVENT_ACTOR_TAG);
    }
    words.get(object_type as usize).copied()
}

/// `FUN_001b7fa8` / `FUN_001b8040`: whether opcode 0x4d touches an object with this resource tag (method +0x3c,
/// `FUN_0016bc68`, is the same in every object class vtable; the compare is signed).
pub fn op_4d_touches(tag: i32) -> bool {
    tag < HIDE_TAG_LIMIT
}

/// Opcode 0x4d on the event's own actors: `FUN_001b7fa8` sets (+6 = 0), `FUN_001b8040` clears (+6 not 0) object
/// +0x10c bit 0x80 on every object in the list at that moment whose tag it touches. Actors the event makes later
/// are not changed. Returns the new hidden state of an object with this tag that already existed.
pub fn op_4d_hidden(was: bool, tag: i32, show: bool) -> bool {
    if op_4d_touches(tag) { !show } else { was }
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
    /// The event's kind row (`FUN_00195898`), `None` when the table has none.
    row: Option<KindRow>,
    /// The screen fader the event object drives (`start_fade`, then `setup_fade`); while the start fade runs
    /// the timeline is held (states 0..3).
    fade: Option<ScreenFade>,
    /// `DAT_0037b054`.
    c054: f32,
}

/// `SLUS_210.05` file offset of a virtual address (as `area_jump`).
const EXE_BIAS: usize = 0xff000;

#[derive(Resource, Default)]
pub struct EventPlayer {
    play: Option<Play>,
    /// The event kind rows (`03system.bin` `evtp`, `kind_rows`).
    kinds: Option<Vec<KindRow>>,
    /// `DAT_0037b050`, `DAT_0037b054` (the fade length factors of `FUN_00195d28`, `FUN_00195ff8`).
    consts: Option<(f32, f32)>,
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

    /// The screen fader's state while an event drives it: (alpha 0..1, white). For the app's full-screen fade
    /// node (`area_jump::draw_fader`).
    pub fn screen_fade(&self) -> Option<(f32, bool)> {
        self.play.as_ref().and_then(|p| p.fade).map(|f| (f.alpha(), f.white))
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
#[cfg(test)]
pub fn kind_flags(kinds: &[(u8, u16)], arg: u16) -> Option<u16> {
    let k = (arg & 0x3f) as u8;
    kinds.iter().find(|(x, _)| *x == k).map(|(_, f)| *f)
}

/// One `evtp` row of `03system.bin` (8 bytes after an 8-byte head, `FUN_001d1128`): +0 the kind, +1 the fade
/// kind (`FUN_00195d28` at the start, `FUN_00195ff8` after the setup: 1 black, 2 white, 3 the keyhole layout
/// `FUN_0015d058`, 0 none), +2 the flags, +4 the length of the start fade (a float, times `DAT_0037b050`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KindRow {
    pub kind: u8,
    pub fade: u8,
    pub flags: u16,
    pub fade_len: f32,
}

/// The `evtp` rows (same table as `struggle::event_kinds`, with the fade fields).
pub fn kind_rows(sys: &[u8]) -> Vec<KindRow> {
    let Ok(entries) = kh2_bar::parse(sys) else { return Vec::new() };
    let Some(e) = entries.iter().find(|e| e.tag == "evtp") else { return Vec::new() };
    e.data.get(8..).unwrap_or_default().as_chunks::<8>().0.iter().map(|r| KindRow { kind: r[0], fade: r[1], flags: u16::from_le_bytes([r[2], r[3]]), fade_len: f32::from_le_bytes([r[4], r[5], r[6], r[7]]) }).collect()
}

/// `FUN_0014a710`: a fade of `frames` (0 counts as 1) lasts `2 x frames` ticks (one tick a game frame here, as
/// `area_jump::FADE_TICKS`).
pub fn fader_ticks(frames: i32) -> f32 {
    (frames.max(0) as u32).max(1) as f32 * 2.0
}

/// The screen fader at 0x3564d0 (`FUN_0014a918`) as the event object drives it: `out` is mode 3
/// (`FUN_0014a7c8`, to the colour), else mode 1 (`FUN_0014a7f0`, from it); `white` is `+0xf4` (`FUN_0014a710`:
/// 0 black, else white).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenFade {
    pub out: bool,
    pub white: bool,
    pub ticks: f32,
    pub t: f32,
}

impl ScreenFade {
    /// The colour's alpha, 0..1 (full is `DAT_00379130` = 128.0 on the PS2, as `area_jump::fade_alpha`).
    pub fn alpha(&self) -> f32 {
        let k = (self.t / self.ticks).clamp(0.0, 1.0);
        if self.out { k } else { 1.0 - k }
    }
    pub fn done(&self) -> bool {
        self.t >= self.ticks
    }
}

/// The event object's fades (`FUN_00195c08` states). State 0 (`FUN_00195d28`): fade kind 1 or 2 fades the
/// screen fader out over `(int)(row +4 * DAT_0037b050)` frames (2: white); 3 plays the keyhole layout
/// (`FUN_0015d058`, not driven from here [GAP]); states 1 to 3 hold the timeline until the fade is over
/// (`FUN_001966f8`: fader `+0xfc` != 4), then set the object's bit 2 that the timeline thread waits for
/// (`FUN_00223a88` -> `FUN_00196320`). The thread then calls `FUN_00196348(2 * DAT_0036bd0c)` (opcode 0x10,
/// `FUN_00226fa0`): state 5, 6 (`FUN_00195ff8`) fades the fader in over `(int)(+0x90 * DAT_0037b054)` frames
/// for kinds 1..3 (2: white). Returns the start fade, or `None` (kind 0 and 3 here).
pub fn start_fade(row: Option<&KindRow>, c050: f32) -> Option<ScreenFade> {
    let r = row?;
    matches!(r.fade, 1 | 2).then(|| ScreenFade { out: true, white: r.fade == 2, ticks: fader_ticks((r.fade_len * c050) as i32), t: 0.0 })
}

/// The fade-in after the setup (`FUN_00195ff8`): `op10` is opcode 0x10's +4 (`DAT_0036bd0c`, 0 when absent),
/// the object's +0x90 is twice it, times `DAT_0037b054`.
pub fn setup_fade(row: Option<&KindRow>, op10: i16, c054: f32) -> Option<ScreenFade> {
    let r = row?;
    matches!(r.fade, 1..=3).then(|| ScreenFade { out: false, white: r.fade == 2, ticks: fader_ticks((2.0 * op10 as f32 * c054) as i32), t: 0.0 })
}

/// The draw group of every mesh of a skeletal `.mdlx` (the model block, type 3: +0x1c the mesh count, 0x20-byte
/// mesh headers from +0x20): bits 6..10 of the header's flags word, the field `FUN_00120fd0` matches. In file
/// order (as `kh2_model::mdlx` and `skel::spawn_skinned` keep them).
pub fn mesh_groups(file: &[u8]) -> Option<Vec<u8>> {
    let entries = kh2_bar::parse(file).ok()?;
    let b = entries.iter().find(|e| e.kind == 4)?.data.get(kh2_model::RESERVED..)?;
    let w = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    if w(0)? != 3 {
        return None;
    }
    (0..w(0x1c)?.min(1000) as usize).map(|c| w(0x20 + 0x20 * c).map(|f| (f >> 6 & 0x1f) as u8)).collect()
}

/// Opcode 0xd (`FUN_00228b48` -> `FUN_0011c550` -> the skeletal model class's method +0x34, `FUN_00120fd0`,
/// vtable 0x354b98): every mesh's draw flag (instance +0x104, one byte a mesh, 1 at creation, `FUN_00120b48`)
/// loses its low four bits (`FUN_0011eda0`), then each u16 of the list up to 0xffff ORs 1 (bit 15 clear) or 2
/// (bit 15 set) into the meshes whose group is `value & 0x7fff`. The draw (`FUN_00121a78`) skips a mesh whose
/// flag has no low bit set. Returns the flags (0 hidden).
pub fn mesh_flags(groups: &[u8], list: &[u16]) -> Vec<u8> {
    let mut flags = vec![0u8; groups.len()];
    for &v in list.iter().take_while(|&&v| v != 0xffff) {
        let bit = if v & 0x8000 == 0 { 1 } else { 2 };
        for (f, &g) in flags.iter_mut().zip(groups) {
            if g as u16 == v & 0x7fff {
                *f |= bit;
            }
        }
    }
    flags
}

/// `FUN_0022ca18`'s first step of a looping motion row (+0x30 = 1, +0x1a not -1): the row keeps the object's
/// place (`FUN_0016b960`) and clears the flag.
pub fn keep_place(first: &mut bool, kept: &mut Option<Vec3>, place: Option<Transform>, loops: bool) {
    if *first && loops {
        *first = false;
        *kept = place.map(|t| t.translation);
    }
}

/// At a loop (`FUN_0016a618`): the object's position becomes the kept place; its rotation stays.
pub fn put_back(kept: Option<Vec3>, place: Option<Transform>) -> Option<Transform> {
    Some(Transform { translation: kept?, ..place? })
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
    let rows = player.kinds.get_or_insert_with(|| disc.read("03system.bin").map(|s| kind_rows(&s)).unwrap_or_default());
    let row = rows.iter().find(|r| r.kind == (arg & 0x3f) as u8).copied();
    // FUN_001d1128 answers no row for a kind the table lacks [inferred: then nothing is held back, flags 0xffff].
    let flags = row.map(|r| r.flags);
    let (c050, c054) = *player.consts.get_or_insert_with(|| {
        let exe = disc.read_root("SLUS_210.05");
        let f = |va: usize| exe.as_ref().and_then(|e| e.get(va - EXE_BIAS..va - EXE_BIAS + 4)).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        (f(0x37b050).unwrap_or(0.0), f(0x37b054).unwrap_or(0.0))
    });
    let sf = start_fade(row.as_ref(), c050);
    let fade = sf.or_else(|| setup_fade(row.as_ref(), ev.end_fade.unwrap_or(0), c054));
    info!(
        "event: kind {:#x} fade kind {} start fade {} (FUN_00195d28, row +4 {:?} x DAT_0037b050 {c050}); after the setup {} (FUN_00195ff8, op 0x10 {:?} x 2 x DAT_0037b054 {c054})",
        arg & 0x3f,
        row.map_or(-1, |r| r.fade as i32),
        sf.map_or(if row.is_some_and(|r| r.fade == 3) { "keyhole layout FUN_0015d058: not driven [GAP]".to_string() } else { "none".to_string() }, |f| format!("out to {} over {} ticks", if f.white { "white" } else { "black" }, f.ticks)),
        row.map(|r| r.fade_len),
        setup_fade(row.as_ref(), ev.end_fade.unwrap_or(0), c054).map_or("no fade".to_string(), |f| format!("in from {} over {} ticks", if f.white { "white" } else { "black" }, f.ticks)),
        ev.end_fade
    );
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
        "event: {tag} load blocks name {} voice(s) {:?}, {} effect(s) {:?}, layouts {:?} (FUN_0022c070, FUN_00227df0, FUN_00226100: not played, not drawn); op 0xf {} (FUN_00226f50); setup fade-in {:?} frames (op 0x10, FUN_00196348 -> FUN_00195ff8)",
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
    player.play = Some(Play { area: run.name.clone(), tag, cmds, ev, cursor: 0, ticks: 0, actors: HashMap::new(), lines: Vec::new(), motions: HashMap::new(), flags, windows: WindowSlots::default(), row, fade, c054 });
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
    children: Query<&Children>,
    mesh_q: Query<(), With<Mesh3d>>,
) {
    let player = &mut *player;
    let Some(p) = player.play.as_mut() else { return };
    // The event object's fades (`start_fade`, `setup_fade`): the timeline waits while the start fade runs.
    if let Some(mut fd) = p.fade {
        fd.t += 1.0;
        p.fade = Some(fd);
        if fd.out {
            if !fd.done() {
                return;
            }
            p.fade = setup_fade(p.row.as_ref(), p.ev.end_fade.unwrap_or(0), p.c054);
            info!("event: start fade over after {} ticks (FUN_001966f8); the timeline runs; {}", fd.ticks, p.fade.map_or("no fade-in".to_string(), |f| format!("fade-in from {} over {} ticks (FUN_00196348 -> FUN_00195ff8)", if f.white { "white" } else { "black" }, f.ticks)));
        } else if fd.done() {
            p.fade = None;
            if eventlog() {
                info!("event: setup fade-in over after {} ticks (event frame {:.1})", fd.ticks, p.ticks as f32 * 0.5);
            }
        }
    }
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
                            let groups = model.as_ref().and_then(|model| disc.read(&format!("obj/{model}.mdlx"))).and_then(|f| mesh_groups(&f));
                            slot.insert(Spawned { root, body, motion: None, clock: None, groups, parts: None, place: None, kept: None, first: false, hidden: false });
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
                        sp.first = true;
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
                        commands.entity(s.root).insert((tf, if s.hidden { Visibility::Hidden } else { Visibility::Inherited }));
                        if let Some(s) = p.actors.get_mut(&pl.index) {
                            s.place = Some(tf);
                        }
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
                // FUN_00228b48: actor +8 (cmd +8 = operand 4), the list at cmd +0xa (operand 6) up to 0xffff.
                let index = le16(d, 4);
                let list: Vec<u16> = (6..d.len().saturating_sub(1)).step_by(2).map(|o| le16(d, o) as u16).take_while(|&v| v != 0xffff).collect();
                let note = match p.actors.get_mut(&index) {
                    Some(s) => match s.groups.as_deref() {
                        Some(g) => {
                            let flags = mesh_flags(g, &list);
                            let shown = flags.iter().filter(|&&f| f != 0).count();
                            s.parts = Some(flags);
                            format!("{shown} of {} meshes drawn (groups {:?})", g.len(), g)
                        }
                        None => "the model's mesh groups are not readable".to_string(),
                    },
                    None => "the actor has no object yet".to_string(),
                };
                if log {
                    info!("event: frame {fi} (cmd frame {at}): op 0xd actor {index} draws the mesh groups {list:x?} (FUN_00228b48 -> FUN_00120fd0): {note}");
                }
            }
            0x4d => {
                // FUN_00226e80: +6 = 0 hides (FUN_001b7fa8(2000)), else shows (FUN_001b8040(2000)) every listed object
                // whose model resource tag is below 2000. The event's actors carry -200; the field's Sora, party and
                // townspeople 2300..2700 (not touched). Objects of ObjectType 5, 6, 8, 10, 11, 13, 18, 23 (tags 1030..1110)
                // are touched too: the app draws none of them in an event [see docs/events.md].
                let show = le16(d, 2) != 0;
                let mut n = 0;
                for s in p.actors.values_mut() {
                    s.hidden = op_4d_hidden(s.hidden, EVENT_ACTOR_TAG, show);
                    // An actor opcode 2 has not placed yet stays out of sight (the app's own rule: it has no place).
                    if s.place.is_some() {
                        commands.entity(s.root).insert(if s.hidden { Visibility::Hidden } else { Visibility::Inherited });
                    }
                    n += 1;
                }
                info!(
                    "event: frame {fi} (cmd frame {at}): op 0x4d {} {n} event actor(s) (resource tag {EVENT_ACTOR_TAG} < {HIDE_TAG_LIMIT}; FUN_00226e80 -> FUN_001b7fa8 / FUN_001b8040: +0x10c bit 0x80)",
                    if show { "shows" } else { "hides" }
                );
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
        // Opcode 0xd's draw flags on the body's mesh entities (`spawn_skinned` keeps the file's mesh order).
        if let Some(flags) = s.parts.as_ref()
            && let Ok(kids) = children.get(s.body)
        {
            let parts: Vec<Entity> = kids.iter().filter(|e| mesh_q.contains(*e)).collect();
            if parts.len() == flags.len() {
                for (e, f) in parts.iter().zip(flags) {
                    commands.entity(*e).insert(if *f & 0xf != 0 { Visibility::Inherited } else { Visibility::Hidden });
                }
                s.parts = None;
            } else if !parts.is_empty() {
                info!("event: op 0xd: {} mesh entities for {} model meshes: not applied", parts.len(), flags.len());
                s.parts = None;
            }
        }
        if let Some((slot, blend, start)) = s.motion.take() {
            match anims.get_mut(s.body) {
                Ok(mut a) => {
                    a.play_external(slot, blend);
                    a.frame += start;
                }
                Err(_) => s.motion = Some((slot, blend, start)),
            }
        }
        // FUN_0022ca18 (every frame, from the setup reader's frame path): the motion slots' loops. A looping row
        // keeps the object's place at its first step (`FUN_0016b960`: +0x540 unless `FUN_001beb00`, the object
        // has no +0x570 link, which the app's event actors never have) and puts it back at every loop
        // (`FUN_0016a618`: +0x540 = the kept place, w = 1, `FUN_001921e8`).
        if s.motion.is_none()
            && let Some((clock, slot)) = s.clock.as_mut()
        {
            keep_place(&mut s.first, &mut s.kept, s.place, clock.loop_end != -1);
            if let Some(at) = clock.tick()
                && let Ok(mut a) = anims.get_mut(s.body)
            {
                a.play_external(slot.clone(), 0.0);
                a.frame += at;
                if let Some(tf) = put_back(s.kept, s.place) {
                    s.place = Some(tf);
                    commands.entity(s.root).insert(tf);
                }
                if log {
                    info!("event: frame {fi}: motion {} loops back to frame {at} (FUN_0022ca18, {}..{}); place put back {:?} (FUN_0016a618)", slot.tag, clock.loop_start, clock.loop_end, s.kept.map(|k| k / SCALE));
                }
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
        // The screen fader the event drives (drawn by `area_jump::draw_fader` once it reads this).
        if let Some((a, white)) = player.screen_fade()
            && eventlog()
            && a >= 1.0
        {
            info!("event: screen fader fully {} (FUN_0014a918)", if white { "white" } else { "black" });
        }
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

    /// WP-E6e: the event object's fades. Fade kind 1 fades the screen fader out to black at the start
    /// (`FUN_00195d28`) over `(int)(row +4 * DAT_0037b050)` frames = twice that in ticks (`FUN_0014a710`), 2 to
    /// white; after the setup `FUN_00195ff8` fades in over `(int)(2 * op 0x10 * DAT_0037b054)` frames (kinds 1..3);
    /// kind 0 has neither, kind 3 (the keyhole) no fader fade-out. On the disc: the rows and the two factors.
    #[test]
    fn the_event_object_fades_out_then_in_by_its_kind_row() {
        let row = |fade| KindRow { kind: 2, fade, flags: 0, fade_len: 30.0 };
        let s = start_fade(Some(&row(1)), 0.5).expect("kind 1 fades out");
        assert!(s.out && !s.white);
        assert_eq!(s.ticks, 30.0);
        assert_eq!(s.alpha(), 0.0);
        assert!(start_fade(Some(&row(2)), 0.5).unwrap().white);
        assert_eq!(start_fade(Some(&row(3)), 0.5), None);
        assert_eq!(start_fade(Some(&row(0)), 0.5), None);
        assert_eq!(start_fade(None, 0.5), None);
        let i = setup_fade(Some(&row(1)), 20, 0.5).expect("kind 1 fades in");
        assert!(!i.out && !i.white);
        assert_eq!(i.ticks, 40.0, "20 frames of op 0x10: +0x90 = 40, x 0.5 = 20 frames, 40 ticks");
        assert_eq!(i.alpha(), 1.0);
        assert!(setup_fade(Some(&row(2)), 20, 0.5).unwrap().white);
        assert!(!setup_fade(Some(&row(3)), 20, 0.5).unwrap().white);
        assert_eq!(setup_fade(Some(&row(0)), 20, 0.5), None);
        assert_eq!(fader_ticks(0), 2.0, "FUN_0014a710 makes 0 frames 1");
        let half = ScreenFade { t: 20.0, ..i };
        assert_eq!(half.alpha(), 0.5);
        assert!(!half.done() && ScreenFade { t: 40.0, ..i }.done());
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let rows = kind_rows(&disc.read("03system.bin").expect("03system.bin"));
        let old = crate::struggle::event_kinds(&disc.read("03system.bin").unwrap());
        assert_eq!(rows.iter().map(|r| (r.kind, r.flags)).collect::<Vec<_>>(), old, "same rows as struggle::event_kinds");
        let exe = disc.read_root("SLUS_210.05").expect("SLUS_210.05");
        let f = |va: usize| f32::from_le_bytes(exe[va - EXE_BIAS..va - EXE_BIAS + 4].try_into().unwrap());
        eprintln!("evtp rows: {rows:?}; DAT_0037b050 {} DAT_0037b054 {}", f(0x37b050), f(0x37b054));
        let r2 = rows.iter().find(|r| r.kind == 2).expect("kind 2 (tt02 102's trigger argument)");
        eprintln!("kind 2: start {:?}, after the setup of event 102 {:?}", start_fade(Some(r2), f(0x37b050)), setup_fade(Some(r2), 20, f(0x37b054)));
    }

    /// WP-E6e: a looping motion row keeps the place at its first step only (later placements do not move the
    /// kept place) and puts the position back at a loop, keeping the rotation (`FUN_0022ca18`, `FUN_0016a618`).
    #[test]
    fn a_looping_motion_puts_its_actor_back_where_the_loop_began() {
        let a = Transform::from_xyz(1.0, 2.0, 3.0);
        let b = Transform::from_xyz(9.0, 9.0, 9.0).with_rotation(Quat::from_rotation_y(1.0));
        let (mut first, mut kept) = (true, None);
        keep_place(&mut first, &mut kept, Some(a), false);
        assert!(first && kept.is_none(), "a row without a loop keeps nothing");
        keep_place(&mut first, &mut kept, Some(a), true);
        keep_place(&mut first, &mut kept, Some(b), true);
        assert_eq!(kept, Some(a.translation));
        let back = put_back(kept, Some(b)).unwrap();
        assert_eq!(back.translation, a.translation);
        assert_eq!(back.rotation, b.rotation);
        assert_eq!(put_back(None, Some(b)), None);
    }

    /// WP-E6e: opcode 0xd draws only the listed mesh groups (`FUN_00120fd0`): bit 15 of a value ORs 2 instead of
    /// 1, the list ends at 0xffff, unlisted groups are not drawn. On the disc: every op 0xd of the tt02..tt04
    /// events reaches its actor's model (the tt lists name every group but 1).
    #[test]
    fn opcode_0xd_draws_only_the_listed_mesh_groups() {
        let groups = [0u8, 0, 2, 3, 3, 5];
        assert_eq!(mesh_flags(&groups, &[0, 3, 0xffff, 2]), vec![1, 1, 0, 1, 1, 0]);
        assert_eq!(mesh_flags(&groups, &[0x8002, 5]), vec![0, 0, 2, 0, 0, 1]);
        assert_eq!(mesh_flags(&groups, &[]), vec![0; 6]);
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let objs = disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default();
        let (mut cmds, mut checked, mut unknown) = (0, 0, Vec::new());
        for area in ["tt02", "tt03", "tt04"] {
            let (ard, _) = kh2_ard::Area::parse(&disc.read(&format!("ard/{area}.ard")).expect("area")).expect("area");
            for (tag, bytes) in &ard.events {
                let Some(list) = kh2_ard::parse_event(bytes) else { continue };
                let e = Event::parse(&list);
                for c in list.iter().filter(|c| c.opcode == 0xd) {
                    cmds += 1;
                    let d = &c.data;
                    let Some(a) = e.actors.iter().find(|a| a.index == le16(d, 4)) else { continue };
                    let Some(model) = objs.iter().find(|r| r.object_id == a.object as u32).map(|r| r.model_name.clone()) else { continue };
                    let Some(g) = disc.read(&format!("obj/{model}.mdlx")).and_then(|f| mesh_groups(&f)) else { continue };
                    let list: Vec<u16> = (6..d.len().saturating_sub(1)).step_by(2).map(|o| le16(d, o) as u16).take_while(|&v| v != 0xffff).collect();
                    checked += 1;
                    for v in &list {
                        if !g.contains(&((v & 0x7fff) as u8)) {
                            unknown.push(format!("{area} {tag} {model} {v:#x} (groups {g:?})"));
                        }
                    }
                    if checked <= 3 {
                        eprintln!("{area} {tag} actor {} {model}: groups {g:?}, list {list:x?} -> {:?}", a.index, mesh_flags(&g, &list));
                    }
                }
            }
        }
        // The lists name groups 0 and 2..0x1f (all but 1): a group the model lacks matches no mesh (no-op).
        eprintln!("op 0xd: {cmds} commands, {checked} with a readable model, {} listed values match no mesh", unknown.len());
        assert!(checked > 100, "{checked}");
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

    /// Opcode 0x4d (`FUN_00226e80`): +6 = 0 sets object bit 0x80 on every object whose model resource tag is below
    /// 2000 (signed), else clears it. The tag of the event's own actors is -200 (`FUN_0016ef80`), the table of
    /// `FUN_0016e430` (0x37a278) gives the rest by ObjectType.
    #[test]
    fn opcode_0x4d_hides_and_shows_the_objects_whose_tag_is_below_2000() {
        assert!(op_4d_touches(-200) && op_4d_touches(1999) && !op_4d_touches(2000) && !op_4d_touches(2300));
        // hide, hide again (stays), show
        let h = op_4d_hidden(false, EVENT_ACTOR_TAG, false);
        assert!(h);
        assert!(op_4d_hidden(h, EVENT_ACTOR_TAG, false));
        assert!(!op_4d_hidden(h, EVENT_ACTOR_TAG, true));
        // an object with a tag of 2000 or more keeps its state either way
        assert!(!op_4d_hidden(false, 2300, false));
        assert!(op_4d_hidden(true, 2300, true));
        // the event's own actors: -200 for every type but 7 and 0x16
        let words = [2300, 2400, 2500, 2600, 2700, 1090, 1090, 2100];
        assert_eq!(resource_tag(&words, 1, true), Some(-200));
        assert_eq!(resource_tag(&words, 7, true), Some(2100));
        let mut long = [0i32; 24];
        long[0x16] = 2200;
        assert_eq!(resource_tag(&long, 0x16, true), Some(2200), "type 0x16 keeps the table word like 7");
        assert_eq!(resource_tag(&words, 5, false), Some(1090));
        assert_eq!(resource_tag(&words, 1, false), Some(2400));
        assert_eq!(resource_tag(&words, 40, false), None);
    }

    /// On the disc: the table at 0x37a278 of the executable (read here, never copied) puts ObjectTypes 5, 6, 8, 10,
    /// 11, 13, 18 and 23 below 2000 and the player, friends, townspeople, enemies and save points above. Lists what
    /// the tt areas spawn of the hidden types and every opcode 0x4d of tt02..tt04 (docs/events.md).
    #[test]
    fn opcode_0x4d_touches_the_object_types_the_table_puts_below_2000() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let exe = disc.read_root("SLUS_210.05").expect("executable");
        let words: Vec<i32> = (0..24).map(|i| {
            let o = 0x37a278 - EXE_BIAS + 4 * i;
            i32::from_le_bytes([exe[o], exe[o + 1], exe[o + 2], exe[o + 3]])
        }).collect();
        let hit: Vec<u8> = (0..24u8).filter(|&t| op_4d_touches(resource_tag(&words, t, false).unwrap())).collect();
        eprintln!("table {words:?}: touched object types {hit:?}");
        assert_eq!(hit, vec![5, 6, 8, 10, 11, 13, 18, 23]);
        for t in [0u8, 1, 4, 7, 15, 19] {
            assert!(!hit.contains(&t), "type {t}");
        }
        let objs = disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default();
        let mut spawned = std::collections::BTreeMap::new();
        let (mut cmds, mut hides, mut shows) = (0, 0, 0);
        for area in ["tt00", "tt01", "tt02", "tt03", "tt04"] {
            let Some(file) = disc.read(&format!("ard/{area}.ard")) else { continue };
            let (ard, _) = kh2_ard::Area::parse(&file).expect("area");
            for (_, _, _, e) in ard.spawned() {
                if let Some(o) = objs.iter().find(|o| o.object_id == e.object).filter(|o| hit.contains(&o.object_type)) {
                    *spawned.entry((o.object_type, o.model_name.clone())).or_insert(0) += 1;
                }
            }
            for (tag, bytes) in &ard.events {
                for c in kh2_ard::parse_event(bytes).unwrap_or_default().iter().filter(|c| c.opcode == 0x4d) {
                    cmds += 1;
                    if le16(&c.data, 2) == 0 { hides += 1 } else { shows += 1 }
                    eprintln!("{area} event {tag}: op 0x4d {:02x?}", c.data);
                }
            }
        }
        eprintln!("area spawns of the touched types: {spawned:?}; op 0x4d: {cmds} commands, {hides} hide, {shows} show");
        assert!(cmds > 0);
    }

    /// Every `.ard` area file on the disc, scanned for opcode 0x4d in its events (docs/events.md section 6).
    #[test]
    fn opcode_0x4d_on_the_whole_disc() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let list = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/kh2idx.txt")).unwrap_or_default();
        let names: Vec<String> = list.lines().filter(|n| n.starts_with("ard/") && n.ends_with(".ard")).map(str::to_string).collect();
        if names.is_empty() {
            eprintln!("skipped: the disc's area names are not known (local/kh2idx.txt)");
            return;
        }
        let (mut files, mut found) = (0, Vec::new());
        for n in &names {
            let Some(file) = disc.read(n) else { continue };
            let Some((ard, _)) = kh2_ard::Area::parse(&file) else { continue };
            files += 1;
            for (tag, bytes) in &ard.events {
                for c in kh2_ard::parse_event(bytes).unwrap_or_default().iter().filter(|c| c.opcode == 0x4d) {
                    found.push(format!("{n} event {tag} frame {} +6 {}", le16(&c.data, 0), le16(&c.data, 2)));
                }
            }
        }
        eprintln!("{files} area files, op 0x4d: {found:?}");
        assert!(files > 20, "{files}");
        assert!(found.iter().any(|f| f.starts_with("ard/tt01.ard event 101 ")) && found.iter().any(|f| f.ends_with("+6 1")), "{found:?}");
    }

    /// The real `step` path: an event actor that opcode 0x4d (+6 = 0) hid stays hidden when opcode 2 places it,
    /// and a later 0x4d with +6 = 1 shows it. Needs the disc (the actor's model).
    #[test]
    fn an_actor_hidden_by_opcode_0x4d_stays_hidden_after_its_placement() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let mut a = Vec::new();
        a.extend(h(0x5a));
        a.extend(h(0));
        a.extend(b"p_ex110 ");
        let mut m = vec![0u8; 14];
        m.extend(b"anm_tt/ETT99901E  ");
        let mut hide = vec![0u8; 4];
        hide[0..2].copy_from_slice(&h(1));
        let mut show = vec![0u8; 4];
        show[0..2].copy_from_slice(&h(3));
        show[2..4].copy_from_slice(&h(1));
        let mut place = vec![0u8; 36];
        place[0x22..0x24].copy_from_slice(&h(2));
        let cmds = vec![cmd(0xf, &[0; 4]), cmd(1, &a), cmd(0x2c, &m), cmd(0x4d, &hide), cmd(2, &place), cmd(0x4d, &show), cmd(8, &h(1000))];
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        app.init_asset::<Mesh>().init_asset::<StandardMaterial>().init_asset::<Image>().init_asset::<SkinnedMeshInverseBindposes>();
        app.insert_resource(Disc::open(&iso)).init_resource::<CamEvent>().init_resource::<EventPlayer>();
        let ev = Event::parse(&cmds);
        app.world_mut().resource_mut::<EventPlayer>().play = Some(Play { area: "tt02".into(), tag: "t".into(), cmds, ev, cursor: 0, ticks: 0, actors: HashMap::new(), lines: Vec::new(), motions: HashMap::new(), flags: 0xffff, windows: WindowSlots::default(), row: None, fade: None, c054: 0.0 });
        app.add_systems(Update, step);
        let vis = |app: &mut App| {
            let mut q = app.world_mut().query_filtered::<&Visibility, With<EventActor>>();
            q.iter(app.world()).copied().collect::<Vec<_>>()
        };
        let mut seen = Vec::new();
        for _ in 0..8 {
            app.update();
            seen.push(vis(&mut app));
        }
        // ticks 1..8 are event frames 0.5..4: the actor appears at frame 0 (hidden), 0x4d hides at frame 1, the
        // placement at frame 2 must not show it, the show at frame 3 does.
        assert_eq!(seen[0], vec![Visibility::Hidden], "made, not placed");
        assert_eq!(seen[3], vec![Visibility::Hidden], "placed after the hide: still hidden");
        assert_eq!(seen[7], vec![Visibility::Inherited], "shown by the later 0x4d");
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