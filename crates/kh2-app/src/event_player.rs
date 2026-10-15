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
use crate::hud::{HudNode, HudScene};
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
/// +6 and +0xa/+0xc are not read by it [GAP: who reads them].
#[derive(Clone, Debug, PartialEq)]
pub struct Motion {
    pub frame: i32,
    pub index: i16,
    pub start: i16,
    pub blend: i16,
    pub name: String,
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
                        if kind == 0x25 {
                            e.loads.push((le16(d, p + 4) as u16, le16(d, p + 6), cstr(d, p + 10)));
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
                0x2c => e.motions.push(Motion { frame: le16(d, 0) as i32, index: le16(d, 10), start: le16(d, 4), blend: le16(d, 12), name: cstr(d, 14) }),
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

/// Events handed to the player (the area exits' triggers, `area_jump.rs`): (event tag, trigger argument).
static REQUESTS: Mutex<Vec<(String, u16)>> = Mutex::new(Vec::new());

/// Asks the player to play event `tag` of the loaded area (a trigger with an event tag, `FUN_00196428`).
pub fn request(tag: &str, arg: u16) {
    if let Ok(mut q) = REQUESTS.lock() {
        q.push((tag.to_string(), arg));
    }
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
}

#[derive(Resource, Default)]
pub struct EventPlayer {
    play: Option<Play>,
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
}

/// Starts a requested event: `request` (a trigger) or the harness `KH2_EVENT=<tag>` (the loaded area's event,
/// `KH2_EVENT_AT=<app seconds>`, default 2).
fn start(time: Res<Time<Real>>, rec: Res<AreaRecord>, mut player: ResMut<EventPlayer>) {
    let Some(run) = rec.run.as_ref() else { return };
    let mut next = REQUESTS.lock().ok().and_then(|mut q| (!q.is_empty()).then(|| q.remove(0)));
    if next.is_none() && !player.env_done {
        let at = std::env::var("KH2_EVENT_AT").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(2.0);
        if let Ok(tag) = std::env::var("KH2_EVENT")
            && time.elapsed_secs() >= at
        {
            player.env_done = true;
            next = Some((tag, 0));
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
    player.play = Some(Play { area: run.name.clone(), tag, cmds, ev, cursor: 0, ticks: 0, actors: HashMap::new(), lines: Vec::new(), motions: HashMap::new() });
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
    p.ticks += 1;
    // FUN_00222728: the counter is the frame count times 0.5 (no movie is played, so never the movie's frame).
    let f = p.ticks as f32 * 0.5;
    let fi = f.floor() as i32;
    let log = eventlog();
    for i in due(&p.cmds, &mut p.cursor, f) {
        let c = &p.cmds[i];
        let d = &c.data;
        let at = wait_frame(c).map_or("-".to_string(), |w| w.to_string());
        match c.opcode {
            0x2c => {
                let m = Motion { frame: le16(d, 0) as i32, index: le16(d, 10), start: le16(d, 4), blend: le16(d, 12), name: cstr(d, 14) };
                let Some(actor) = p.ev.actors.iter().find(|a| a.index == m.index).cloned() else {
                    info!("event: frame {fi} (cmd frame {at}): op 0x2c actor {} is not in the actor table", m.index);
                    continue;
                };
                // FUN_0022cd90: the actor's object is made at its first motion (FUN_0016e8c0, objentry id).
                if !p.actors.contains_key(&m.index) {
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
                            p.actors.insert(m.index, Spawned { root, body, motion: None });
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
                        sp.motion = Some((slot, 2.0 * m.blend as f32, 2.0 * m.start as f32));
                        if log {
                            info!("event: frame {fi} (cmd frame {at}): op 0x2c actor {} {} plays {} (start {}, blend {})", m.index, actor.name, path.unwrap_or_default(), m.start, m.blend);
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
                if w.action == 0
                    && let Some(m) = msg.as_mut()
                {
                    m.open = Some((w.message as u32, w.kind.clamp(0, 1) as usize));
                    m.closed = false;
                }
                info!("event: frame {fi} (cmd frame {at}): op 0x2d window kind {} key {} message {:#x} action {}{}", w.kind, w.key, w.message, w.action, if w.action == 0 { "" } else { " (close / wait not ported)" });
            }
            6 => {
                if log {
                    info!("event: frame {fi}: op 6 camera cut {} frames {}..{}", le16(d, 0), le16(d, 2), le16(d, 4));
                }
            }
            0x44 => info!("event: frame {fi} (cmd frame {at}): op 0x44 movie movie/{}/{}.pss not played (no movie player) [GAP]", &p.area[..2.min(p.area.len())], cstr(d, 2)),
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

/// The pad is the event's while it plays [inferred: the field's player task does not run in the event state].
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
        pad.lock();
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
        // The HUD's label type is private to `hud.rs`: the label is made from the system table's fallback
        // message 0x8adc (the one 0x190c40 falls back to) and given the lines' text.
        let mut label = t.message(0x8adc, style, [0.0, 0.0])?;
        label.tokens = tokens;
        (!label.tokens.is_empty()).then_some(label)
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
        assert_eq!(e.motions[0], Motion { frame: 37, index: 0, start: 3, blend: 5, name: "anm_tt/ETT99901E".into() });
        assert_eq!(e.places[0], Place { frame: 37, index: 0, gate: 0, pos: [7.0, 0.0, 339.0], rot: [0.0, -170.0, 0.0] });
        assert_eq!(wait_frame(&cmd(2, &p)), Some(37));
        assert_eq!(e.model_of(0x5a), Some("p_ex110"));
        assert_eq!(anb_path("anm_tt/ETT99901E", "p_ex110").as_deref(), Some("anm/tt/p_ex110/ETT99901E.anb"));
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
}