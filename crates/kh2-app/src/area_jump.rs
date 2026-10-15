//! Exit volumes and the area jump (WP-E1, docs/events.md section 2).
//!
//! Every frame the game tests the player against the activator volumes of the loaded spawn
//! groups (`FUN_001989d8`) and hands a hit to `FUN_00198b30`, which for a kind-1 group (an exit,
//! `FUN_00198e10`) acts by the volume's option: option 0 is the walk-in volume, options 1 and 2
//! the facing prompt. A walk-in fires event 1 with the group id (`FUN_00198fa0`); a trigger on that
//! event takes the exit over (an event, or its own jump record), else `FUN_00104e18` starts the
//! jump task `FUN_00104d58`: fade out, unload, apply the destination, restart the area thread, and
//! on the next area start `FUN_0013efe8` runs the matching fade-in.
//!
//! Fade kinds (`FUN_0013ef68` out, `FUN_0013efe8` in): an exit uses kind 1 when the world changes
//! and 0 otherwise (`FUN_00198fa0`). Kind 1 is the screen fader at 0x3564d0: `FUN_0014a7c8(fader,
//! 0xf, 0)` out and `FUN_0014a7f0(fader, 0xf, 0)` in; `FUN_0014a710` makes the duration `2 x 15`
//! ticks and the colour black (`+0xf4` 0) or white. Kind 0 is the 2D layout in slot 2 of the table
//! at 0x355118 (`FUN_0015d058` plays its animation 1 out, `FUN_0015d1a8` animation 0 in plus a
//! 2-tick fader fade-in): the keyhole wipe of the reference video, drawn as a HUD node (`Keyhole`).
//! When the layout cannot be loaded, kind 0 falls back to the 30-tick black fade.
//!
//! Gate (`FUN_001d88a0(volume, header +0x1e)`): 0 passes; 1 and 2 need `FUN_001d8908` (the player
//! is a controlled object, not busy by actor flags `+0x108` 0x80000 / 0x10000000 [inferred: an
//! action or event], and moving: `FUN_001b7e00`, `+0x160` bits 4, 8, 0x10 [inferred: walk, run,
//! dash]) and `FUN_001d8958`: the exit's direction `(sin a, 0, cos a)` for `a = DAT_0037c714 - ry`
//! must make a dot product of at least `DAT_0037b358` with both the player's facing and his
//! movement direction (`FUN_0019b110`, speed `+0x1c` above 0). Other values fail. The two
//! constants are read from the executable. Exits also do nothing in battle state 2 unless their
//! gate is 0 (`FUN_00198e10`, `FUN_001903c8`): not ported (the app has no battle state resource
//! here). The lock `FUN_00195358` is ported as `ExitLocks` (the `dsa` table of `00progress.bin`
//! and the save-block halfwords at 0x33f918, which the story flags' ops 2..5 write in
//! `progress::Story`, WP-E5; 0 in a new game; the world-0x10 branch `FUN_001d32c0` is not ported).

use bevy::prelude::*;
use kh2_ard::run::{Exit, Jump};
use kh2_ard::{Activator, Trigger};

use crate::area::{self, AreaRecord};
use crate::assets::{CurrentArea, Disc};
use crate::schedule::Sim;
use crate::sora::Sora;

pub struct AreaJumpPlugin;

impl Plugin for AreaJumpPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Exits>()
            .init_resource::<JumpTask>()
            .init_resource::<LastExit>()
            .init_resource::<ExitConsts>()
            .init_resource::<ExitLocks>()
            .init_resource::<Keyhole>()
            .add_systems(Startup, (load_consts, spawn_fader))
            .add_systems(FixedUpdate, (tick, unload_area_actors).chain().after(Sim::Sora).before(Sim::Enemy).run_if(crate::schedule::sim_running))
            .add_systems(Update, (load_keyhole, draw_fader).in_set(crate::schedule::Frame::Hud));
    }
}

/// `2 x 15` ticks: `FUN_0014a7c8(fader, 0xf, ..)` and `FUN_0014a710` (`param << 1`). The fader's
/// full alpha is `DAT_00379130` = 128.0 (0x80 is opaque on the PS2); the app's alpha runs 0..1.
pub const FADE_TICKS: f32 = 30.0;

/// The executable's constants the gate test reads.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Default)]
pub struct ExitConsts {
    /// `DAT_0037c714`: the exit direction's angle is this minus the volume's rotation y.
    pub angle_base: Option<f32>,
    /// `DAT_0037b358`: the smallest dot product that passes.
    pub min_dot: Option<f32>,
}

const EXE_BIAS: usize = 0xff000;

fn load_consts(disc: Res<Disc>, mut consts: ResMut<ExitConsts>, mut locks: ResMut<ExitLocks>, progress: Option<Res<crate::progress::Progress>>) {
    if let Some(table) = disc.read("00progress.bin").and_then(|f| kh2_bar::parse(&f).ok()?.into_iter().find(|e| e.tag == kh2_ard::progress::LOCK_TABLE).map(|e| e.data.to_vec())) {
        let n = table.get(..2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize);
        locks.table = table;
        locks.story = progress.map(|p| p.story_handle());
        if area::arealog() {
            let set: Vec<String> = locks.story.as_ref().map(|s| s.lock()).map_or(Vec::new(), |s| (0..n).filter(|&i| s.lock_state(i) != 0).map(|i| format!("{i} ({:?}) {:#x}", locks.entry(i), s.lock_state(i))).collect());
            info!("exits: lock table {} with {n} areas; lock halfwords set (0x33f918, story flag ops 2..5): [{}]", kh2_ard::progress::LOCK_TABLE, set.join(", "));
        }
    }
    let Some(exe) = disc.read_root("SLUS_210.05") else { return };
    let f = |va: usize| exe.get(va - EXE_BIAS..va - EXE_BIAS + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    *consts = ExitConsts { angle_base: f(0x37c714), min_dot: f(0x37b358) };
    if area::arealog() {
        info!("exits: gate constants angle base {:?} min dot {:?}", consts.angle_base, consts.min_dot);
    }
}

/// The exit lock (`FUN_00195358`): the `dsa` table of `00progress.bin` (`DAT_01c8a988`) and one
/// lock halfword per entry (the save block's `0x33f918 + index * 4`), kept by the story
/// (`progress::Story::lock_state`): the story flags' side effects ops 2..5 write it, so a
/// `KH2_STATE` file with `story_flags` locks and unlocks exits [inferred: a new game starts with
/// them 0; the new-game initialiser was not read].
#[derive(Resource, Default, Debug)]
pub struct ExitLocks {
    pub table: Vec<u8>,
    /// The app's story (none: every halfword reads 0).
    pub story: Option<crate::progress::StoryHandle>,
}

impl ExitLocks {
    /// `FUN_00195358(world, area)` != 0. The world-0x10 branch (`FUN_001d32e0`, story flags) is
    /// not ported: the app has only Twilight Town.
    pub fn locked(&self, world: u8, area: u8) -> bool {
        let story = self.story.as_ref().map(|s| s.lock());
        kh2_ard::progress::exit_locked(&self.table, |i| story.as_ref().map_or(0, |s| s.lock_state(i)), world, area, false)
    }

    /// The (world, area) of `dsa` entry `i`.
    pub fn entry(&self, i: usize) -> Option<(u8, u8)> {
        Some((*self.table.get(2 + 2 * i)?, *self.table.get(3 + 2 * i)?))
    }
}

/// One exit group of the loaded area (`FUN_00198e10`'s view of a kind-1 group with activators).
#[derive(Debug, Clone, PartialEq)]
pub struct ExitGroup {
    pub tag: String,
    pub id: u16,
    pub exit: Exit,
    /// The activators `FUN_00198750` turned into volumes.
    pub volumes: Vec<Activator>,
    /// Group flag bit 0 (+4): the walk-in fired; the group does nothing afterwards (`FUN_001989d8`
    /// skips it).
    pub fired: bool,
    /// Kind 5 or flag bit 0 of header +1: disabled until a signal shows it (not ported: stays).
    pub disabled: bool,
    /// The prompt was logged for the current stay in the prompt volume.
    pub prompting: bool,
    /// App-only: a jump from this group failed (its destination is not on the disc or not in the
    /// app), `fired` was cleared, and the group waits until Sora is outside all its volumes, so a
    /// failed exit does not fire again every tick.
    pub held: bool,
}

/// The exit groups of the loaded area.
#[derive(Resource, Default)]
pub struct Exits {
    pub area: Option<String>,
    pub groups: Vec<ExitGroup>,
    /// The id of the group whose walk-in started the running jump (cleared again if it fails).
    pub jumping: Option<u16>,
    /// A trigger's event is playing and its jump waits for the end (`FUN_00196428`).
    pub after_event: Option<AfterEvent>,
}

/// The jump a trigger's event leaves behind. `FUN_00196428` copies the trigger's jump record into the event
/// object (+0xc..+0x28); when the object's states reach 9 after the event (`FUN_00195c08` -> `FUN_00196170`)
/// it hands the record to `FUN_0019d9d8(record, 0x80000000)`, the same jump a trigger without an event takes,
/// with bit 31 or-ed into the fade flags (`FUN_00104e18` records the kind without fading out: the
/// event's own end left the screen as it is) [inferred for the meaning of bit 31, as `jump_fade_kind`'s 0x4000].
/// A record whose word 0 is 0 (`FUN_0019d9c8`) is no jump: then `FUN_00196170` gives control and the field HUD
/// back itself (`FUN_001962a0`, `FUN_00196790`).
#[derive(Debug, Clone, PartialEq)]
pub struct AfterEvent {
    pub jump: Option<Jump>,
    /// The exit group whose walk-in fired the trigger.
    pub group: u16,
    /// The event player was seen playing it.
    pub seen_playing: bool,
    /// Ticks waited without the event playing.
    pub waited: u32,
}

impl AfterEvent {
    pub fn new(jump: Option<Jump>, group: u16) -> AfterEvent {
        AfterEvent { jump, group, seen_playing: false, waited: 0 }
    }

    /// One tick with the event player `playing` or not: `true` once the event is over and the jump is due.
    /// An event that never starts (the area has no such entry: `FUN_00180a28` gives 0 and `FUN_00195ae0`
    /// starts no player) does not hold the jump: after `START_TICKS` the jump is due [inferred: the object's
    /// states then run without a player].
    pub fn step(&mut self, playing: bool) -> bool {
        if playing {
            self.seen_playing = true;
            return false;
        }
        if self.seen_playing {
            return true;
        }
        self.waited += 1;
        self.waited > Self::START_TICKS
    }

    /// Ticks the event player takes at most to start a requested event (it takes requests every tick).
    pub const START_TICKS: u32 = 2;
}

/// The area a jump to `dest` loads: `None` (with the reason) when the app has no such area or the
/// disc lacks its `ard/<name>.ard` or `map/us/<name>.map` (`has` answers whether a disc file
/// exists). The caller then stays: the fade is lifted and Sora keeps his place.
pub fn jump_target(dest: &Dest, has: impl Fn(&str) -> bool) -> Result<String, String> {
    let Some(name) = area::area_name(dest.world, dest.area) else { return Err(format!("{} is not in the app", dest_name(dest))) };
    for f in [format!("ard/{name}.ard"), format!("map/us/{name}.map")] {
        if !has(&f) {
            return Err(format!("{name}: {f} is not on the disc"));
        }
    }
    Ok(name)
}

/// The exit groups of a run: every loaded group that is an exit, with its volumes.
pub fn exit_groups(run: &area::AreaRun) -> Vec<ExitGroup> {
    let mut out = Vec::new();
    for g in &run.script.groups {
        for grp in run.ard.spawns.get(&g.tag).into_iter().flatten() {
            let Some(exit) = grp.exit() else { continue };
            out.push(ExitGroup { tag: g.tag.clone(), id: grp.id, exit, volumes: grp.volumes().cloned().collect(), fired: false, disabled: grp.starts_disabled(), prompting: false, held: false });
        }
    }
    out
}

/// A destination: the six bytes `FUN_00104e18` receives (world, area, entrance, then the map, btl and evt
/// program bytes). A byte of 0xff means "keep the record's" (`FUN_00104bd0`; `FUN_00104b90` for the programs,
/// where it keeps the story table's byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dest {
    pub world: u8,
    pub area: u8,
    pub entrance: u8,
    pub programs: [u8; 3],
}

/// The "keep" byte of a destination.
pub const KEEP: u8 = 0xff;

impl Dest {
    /// A destination without program bytes (exits and type-1 jumps).
    pub fn new(world: u8, area: u8, entrance: u8) -> Dest {
        Dest { world, area, entrance, programs: [KEEP; 3] }
    }

    /// `FUN_00104bd0`: the bytes that are 0xff become the record's current world, area and entrance
    /// (`cur`); the program bytes stay as they are (0xff keeps the table's bytes).
    pub fn apply(self, cur: (u8, u8, u8)) -> Dest {
        let k = |b: u8, c: u8| if b == KEEP { c } else { b };
        Dest { world: k(self.world, cur.0), area: k(self.area, cur.1), entrance: k(self.entrance, cur.2), programs: self.programs }
    }

    /// The program bytes as the log shows them: `0xb5`, or `-` for a byte that keeps the table's.
    pub fn programs_text(&self) -> String {
        let b: Vec<String> = self.programs.iter().map(|p| if *p == KEEP { "-".to_string() } else { format!("{p:#x}") }).collect();
        format!("({})", b.join(", "))
    }
}

/// The destination of the last exit's walk-in: the six bytes at `0x33e840` that `FUN_00198fa0` writes
/// (the exit's world, area and entrance, programs 0xff) before it asks the trigger or starts the jump, and
/// that a type-3 jump record jumps to (`FUN_0019d9d8`). Initially all 0xff (`FUN_00105420`), which keeps the
/// current record. Other writers of `0x33e840`, `FUN_00172250` and `FUN_001722c0`, have no direct caller in
/// the executable (docs/events.md section 4, gaps).
#[derive(Resource, Debug, Default, PartialEq, Eq)]
pub struct LastExit(pub Option<Dest>);

impl LastExit {
    /// `FUN_00198fa0`: remembers an exit's destination (the programs are 0xff).
    pub fn record(&mut self, dest: Dest) {
        self.0 = Some(Dest::new(dest.world, dest.area, dest.entrance));
    }
}

/// What a trigger's jump record (`FUN_0019d9d8`, parsed by `FUN_0019d8f0`) jumps to, with `cur` the record's
/// world, area and entrance, `last` the remembered exit and `localset(world, key)` the area `07localset.bin`
/// gives. Type 1: the record's own world, area, entrance. Type 2: `FUN_00198080(key, world)`: a negative world
/// is the current one; the area is the first pair of the table with the key, entrance the record's, and map,
/// btl and evt programs all the key; an unknown key leaves world and area at the current ones (every byte 0xff
/// then `FUN_00104bd0`). Type 3: the remembered exit (`0x33e840`). Types 4, 5 and 6 are not ported: `Err`.
pub fn resolve_jump(j: &Jump, cur: (u8, u8, u8), last: Option<Dest>, localset: impl Fn(u8, u16) -> Option<u8>) -> Result<Dest, String> {
    match j.kind {
        1 => Ok(Dest::new(j.world as u8, j.area as u8, j.entrance as u8).apply(cur)),
        2 => {
            let world = if (j.world as i16) < 0 { cur.0 } else { j.world as u8 };
            let entrance = j.entrance as u8;
            Ok(match localset(world, j.key) {
                Some(area) => Dest { world, area, entrance, programs: [j.key as u8; 3] },
                None => Dest::new(KEEP, KEEP, entrance),
            }
            .apply(cur))
        }
        3 => Ok(last.unwrap_or(Dest::new(KEEP, KEEP, KEEP)).apply(cur)),
        4 => Err("type 4 (a world-map case decided by story flags 0x1071, 0x1072, 0x140f, 0x1822, 0x200b, 0x8cd, 0x3834, 0x1c1c; FUN_00149df0): the app has no world map".to_string()),
        5 => Err("type 5 (opens menu 0xe, FUN_0024f0b8): not ported".to_string()),
        6 => Err("type 6 (FUN_00149f38): not ported".to_string()),
        k => Err(format!("jump type {k}: FUN_0019d9d8 does nothing for it")),
    }
}

/// `FUN_00198080` on the disc's `07localset.bin`: the area the story program `key` belongs to in `world`.
pub fn disc_localset(disc: &Disc, world: u8, key: u16) -> Option<u8> {
    let entry = kh2_ard::localset::world_entry(&disc.read("07localset.bin")?, world)?;
    kh2_ard::localset::area_for(&entry, key)
}

/// The jump task (`FUN_00104d58`): ticks count the fade.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub enum JumpTask {
    #[default]
    Idle,
    FadeOut { dest: Dest, kind: u32, t: f32 },
    /// The destination is set on `CurrentArea`; waits until it is loaded and Sora is placed.
    Loading { dest: Dest, kind: u32 },
    FadeIn { kind: u32, t: f32 },
}

/// An actor that belongs to the loaded area: the area's unload removes it (the enemies of `enemy.rs`).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct AreaActor;

/// The area's unload in the jump task (`FUN_00104d58` -> `FUN_00104ce0`, after the fade-out, before the
/// destination is applied): `FUN_00168ca0` -> `FUN_001b8800` frees every object of the list at `0x1d5d7e0`, and
/// `FUN_00168ce8` -> `FUN_00181af8` the loaded spawn groups [inferred: those are the area's actors; the objects'
/// own teardown was not read]. Here: the task's change to `Loading` despawns every `AreaActor`, once per jump.
/// The party and Sora stay (the app keeps them across areas).
pub fn unload_area_actors(task: Res<JumpTask>, mut done: Local<bool>, actors: Query<Entity, With<AreaActor>>, mut commands: Commands) {
    match *task {
        JumpTask::Loading { dest, .. } => {
            if !*done {
                *done = true;
                let n = actors.iter().count();
                for e in &actors {
                    commands.entity(e).despawn();
                }
                info!("jump: area unload (FUN_00104ce0): {n} area actors removed before {}", dest_name(&dest));
            }
        }
        _ => *done = false,
    }
}

/// The layout table of fade kind 0 (`FUN_0013a8a8`, 0x355110): the world's `field2d/<lang>/
/// <world>0field.2dd` (`FUN_0013a7e8`, format string at 0x378d78, read to 0x3c32c0) gives slot `i`
/// the `i`-th BAR entry of type 0x18 (the image, 0x355118 + 8i, `FUN_0013a980`) and of type 0x19
/// (the sequence, 0x35511c + 8i, `FUN_0013a998`). `FUN_0015d058` plays slot 2's group 1 (out),
/// `FUN_0015d1a8` its group 0 (in). Returns the (image, sequence) entry indices of `slot`.
pub fn layout_slot(kinds: &[u32], slot: usize) -> Option<(usize, usize)> {
    let nth = |k: u32| kinds.iter().enumerate().filter(|e| *e.1 == k).nth(slot).map(|e| e.0);
    Some((nth(0x18)?, nth(0x19)?))
}

/// Slot of the keyhole layout in that table (`FUN_0013a980(2)`, `FUN_0013a998(2)`).
pub const KEYHOLE_SLOT: usize = 2;

/// The fader's alpha (0..1) `t` ticks into a fade of `FADE_TICKS`.
pub fn fade_alpha(out: bool, t: f32) -> f32 {
    let k = (t / FADE_TICKS).clamp(0.0, 1.0);
    if out { k } else { 1.0 - k }
}

/// The 2-tick screen fader of the keyhole wipe (`FUN_0014a7c8(fader, 1, 0)` out after the layout's
/// group ended, `FUN_0014a7f0(fader, 1, 0)` in alongside group 0): `2 x 1` ticks.
pub const WIPE_FADER_TICKS: f32 = 2.0;

/// App-only guard: a wipe whose group never ends stops waiting after this many ticks.
pub const WIPE_MAX_TICKS: f32 = 600.0;

/// The keyhole wipe: fade kind 0's layout (slot 2 of the world's field file) as one HUD node.
#[derive(Resource, Default, Debug)]
pub struct Keyhole {
    /// The HUD node (`None` when the layout did not load: kind 0 then uses the black fade).
    pub node: Option<usize>,
    tried: bool,
    /// The running jump plays the wipe (set when its fade starts).
    wiping: bool,
    /// The fade-out tick at which group 1 ended and the 2-tick fader to black started.
    black_from: Option<f32>,
}

/// The fader's alpha (0..1) during the keyhole wipe: out, black once group 1 has ended
/// (`black_from`) over 2 ticks; in, from black over the first 2 ticks.
pub fn wipe_alpha(out: bool, t: f32, black_from: Option<f32>) -> f32 {
    if out {
        black_from.map_or(0.0, |b| ((t - b) / WIPE_FADER_TICKS).clamp(0.0, 1.0))
    } else {
        1.0 - (t / WIPE_FADER_TICKS).clamp(0.0, 1.0)
    }
}

/// One fade-out tick of the wipe at tick `t` (`FUN_0015cf58`): records when the group ended and
/// answers whether the fade is done (the fader has reached black).
pub fn wipe_out_step(t: f32, group_ended: bool, black_from: &mut Option<f32>) -> bool {
    if black_from.is_none() && (group_ended || t >= WIPE_MAX_TICKS) {
        *black_from = Some(t);
    }
    black_from.is_some_and(|b| t - b >= WIPE_FADER_TICKS)
}

/// One fade-in tick (`FUN_0015d0e8`): done when group 0 has ended and the fader is idle.
pub fn wipe_in_done(t: f32, group_ended: bool) -> bool {
    t >= WIPE_FADER_TICKS && (group_ended || t >= WIPE_MAX_TICKS)
}

/// The index in `SeqFile::from_bar`'s list (every BAR entry pair that decodes as an image followed
/// by a sequence, in order) of the pair starting at image entry `image`; `ok(j)` answers whether
/// entries `j`, `j + 1` decode as such a pair.
pub fn pair_index(image: usize, ok: impl Fn(usize) -> bool) -> Option<usize> {
    ok(image).then(|| (0..image).filter(|&j| ok(j)).count())
}

/// Finds the keyhole layout once the HUD has its files. The HUD loads the world's
/// `field2d/us/<world>0field.2dd` first (`SeqFile::from_bar`), so slot 2's pair is one of its
/// files; the match is checked by image size and sequence before a hidden root node at (0, 0) is
/// added on it.
fn load_keyhole(disc: Res<Disc>, area: Res<CurrentArea>, scene: Option<ResMut<crate::hud::HudScene>>, mut key: ResMut<Keyhole>) {
    let Some(mut scene) = scene else { return };
    if key.tried || scene.files.is_empty() {
        return;
    }
    key.tried = true;
    let world = area.name.get(..2).unwrap_or("tt");
    let name = format!("field2d/us/{world}0field.2dd");
    let found = (|| {
        let data = disc.read(&name)?;
        let bar = kh2_bar::parse(&data).ok()?;
        let kinds: Vec<u32> = bar.iter().map(|e| e.kind as u32).collect();
        let (i, s) = layout_slot(&kinds, KEYHOLE_SLOT)?;
        if s != i + 1 {
            return None;
        }
        let ok = |j: usize| j + 1 < bar.len() && kh2_image::imgd::decode(bar[j].data).is_some() && kh2_image::seqd::parse(bar[j + 1].data).is_some();
        let idx = pair_index(i, ok)?;
        let img = kh2_image::imgd::decode(bar[i].data)?;
        let seq = kh2_image::seqd::parse(bar[s].data)?;
        let f = scene.files.get(idx)?;
        (f.width == img.width && f.height == img.height && f.seq.groups.len() == seq.groups.len() && seq.groups.len() >= 2).then_some(idx)
    })();
    let Some(file) = found else {
        warn!("keyhole: slot {KEYHOLE_SLOT} layout of {name} not among the HUD's files; fade kind 0 uses the black fade");
        return;
    };
    let mut node = crate::hud::HudNode::new(file, None);
    node.visible = false;
    key.node = Some(scene.add(node));
    if area::arealog() {
        info!("keyhole: slot {KEYHOLE_SLOT} of {name} is HUD file {file}; node {:?}", key.node);
    }
}

/// Starts group `group` of the wipe on its node and shows it.
fn play_wipe(scene: &mut Option<ResMut<crate::hud::HudScene>>, key: &Keyhole, group: usize) -> bool {
    let (Some(scene), Some(n)) = (scene.as_mut(), key.node) else { return false };
    scene.set_group(n, group);
    scene.nodes[n].visible = true;
    true
}

fn wipe_ended(scene: &Option<ResMut<crate::hud::HudScene>>, key: &Keyhole) -> bool {
    match (scene.as_ref(), key.node) {
        (Some(s), Some(n)) => s.nodes[n].player.finished(),
        _ => true,
    }
}

/// The player as the gate and prompt tests see him (app axes, centimetres).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Walker {
    pub pos: Vec3,
    pub facing: Vec3,
    /// Movement this tick.
    pub vel: Vec3,
}

/// The exit direction of a volume in the app's axes (`FUN_001d8958`: `(sin a, 0, cos a)` in the
/// game's space for `a = angle_base - rotation y`; the axis flip mirrors Z).
pub fn exit_direction(v: &Activator, angle_base: f32) -> Vec3 {
    let a = angle_base - v.rotation[1];
    area::game_to_app([a.sin(), 0.0, a.cos()])
}

/// `FUN_001d88a0(volume, gate)` for the player `w`. Gate 0 passes; 1 and 2 need a moving player
/// whose facing and movement both point along the exit direction (dot >= `min_dot`); the rest fail.
pub fn gate_passes(gate: u8, v: &Activator, w: &Walker, c: &ExitConsts) -> bool {
    match gate {
        0 => true,
        1 | 2 => {
            let (Some(base), Some(min)) = (c.angle_base, c.min_dot) else { return false };
            let moving = w.vel.xz().length_squared() > 1e-8;
            let dir = exit_direction(v, base);
            moving && w.facing.dot(dir) >= min && w.vel.normalize_or_zero().dot(dir) >= min
        }
        _ => false,
    }
}

/// `FUN_00199058`: the prompt shows while the player faces the volume's centre (dot >= 0).
pub fn faces_volume(v: &Activator, w: &Walker) -> bool {
    let to = (area::game_to_app(v.position) - w.pos).normalize_or_zero();
    w.facing.dot(to) >= 0.0
}

/// What a walk-in does (`FUN_00198fa0` and the trigger on event 1).
#[derive(Debug, Clone, PartialEq)]
pub enum WalkIn {
    /// `FUN_00104e18` with the exit's destination and fade kind.
    Jump { dest: Dest, kind: u32 },
    /// A trigger with an event tag takes the exit: the event plays, the jump waits for its end.
    Event { tag: String, arg: u16, jump: Option<Jump> },
    /// A trigger without an event jumps by its own record (`FUN_0019d9d8`).
    TriggerJump(Jump),
    /// A trigger with neither (its other sub-commands only).
    TriggerOnly,
}

/// An event walk-in (`WalkIn::Event`): requests the event and sets the jump that follows it. Two exit groups
/// firing in one tick would request two events (the event player drops the second while the first plays) and
/// overwrite `after`; only the first one is taken. Returns whether this call took it.
fn start_event_walk_in(after: &mut Option<AfterEvent>, jump: Option<Jump>, id: u16, request: impl FnOnce()) -> bool {
    if after.is_some() {
        return false;
    }
    request();
    *after = Some(AfterEvent::new(jump, id));
    true
}

/// Decides a walk-in of `g` in `world`: the first trigger matching `(1, group id)` takes it over
/// (`FUN_00197b70` answers non-zero), else the exit's own destination with fade kind 1 when the
/// world changes and 0 otherwise.
pub fn walk_in(g: &ExitGroup, triggers: &[Trigger], world: u8) -> WalkIn {
    if let Some(t) = triggers.iter().find(|t| t.matches(1, g.id)) {
        return match (t.event_tag(), t.jump()) {
            (Some((tag, arg)), jump) => WalkIn::Event { tag, arg, jump },
            (None, Some(j)) => WalkIn::TriggerJump(j),
            (None, None) => WalkIn::TriggerOnly,
        };
    }
    let (w, a) = g.exit.destination(world);
    WalkIn::Jump { dest: Dest::new(w, a, g.exit.entrance), kind: (w != world) as u32 }
}

/// The fade kind of a trigger's jump record: the flags word, where bit 0x4000 means "record the
/// kind only" (`FUN_0013ef68` with bit 31 set) and the kind is the low bits.
pub fn jump_fade_kind(j: &Jump) -> (u32, bool) {
    ((j.flags & 0x3fff) as u32, j.flags & 0x4000 != 0)
}

/// `KH2_JUMP=<type>:<key>:<entrance>` (hex) as a jump record: the world is the current one (-1), a type-1 key is
/// the area, the fade kind 0.
fn harness_jump() -> Option<Jump> {
    let v = std::env::var("KH2_JUMP").ok()?;
    let mut f = v.split(':').map(|x| u16::from_str_radix(x.trim().trim_start_matches("0x"), 16).ok());
    let kind = f.next()??;
    let key = f.next().flatten().unwrap_or(0);
    let entrance = f.next().flatten().unwrap_or(0xffff);
    Some(Jump { kind, world: 0xffff, area: if kind == 1 { key } else { 0xffff }, entrance, key, flags: 0 })
}

fn dest_name(d: &Dest) -> String {
    area::area_name(d.world, d.area).unwrap_or_else(|| format!("world {} area {}", d.world, d.area))
}

/// Tests Sora against the exit volumes and runs the jump task.
#[allow(clippy::too_many_arguments)]
fn tick(
    mut exits: ResMut<Exits>,
    mut task: ResMut<JumpTask>,
    mut rec: ResMut<AreaRecord>,
    mut area: ResMut<CurrentArea>,
    consts: Res<ExitConsts>,
    locks: Res<ExitLocks>,
    disc: Res<Disc>,
    sora: Query<&Sora>,
    mut refused: Local<u32>,
    mut key: ResMut<Keyhole>,
    mut scene: Option<ResMut<crate::hud::HudScene>>,
    player: Option<Res<crate::intro::event_player::EventPlayer>>,
    mut forced: Local<u32>,
    mut last: ResMut<LastExit>,
) {
    // The jump task.
    match *task {
        JumpTask::Idle => {}
        JumpTask::FadeOut { dest, kind, t } => {
            let ended = wipe_ended(&scene, &key);
            let k = &mut *key;
            let done = if k.wiping { wipe_out_step(t + 1.0, ended, &mut k.black_from) } else { t + 1.0 >= FADE_TICKS };
            if !done {
                *task = JumpTask::FadeOut { dest, kind, t: t + 1.0 };
            } else {
                // FUN_00104c68 -> FUN_00104bd0: the destination becomes the record; the area thread
                // restarts with it (here: `CurrentArea.name`, which `assets::load_area` picks up).
                let name = match jump_target(&dest, |f| disc.read(f).is_some()) {
                    Ok(name) => name,
                    Err(why) => {
                        // Staying: lift the fade, keep Sora where he is, and let the exit fire again
                        // once he has left its volumes (`ExitGroup::held`).
                        warn!("jump: {why}; staying");
                        if let Some(id) = exits.jumping.take()
                            && let Some(g) = exits.groups.iter_mut().find(|g| g.id == id)
                        {
                            g.fired = false;
                            g.held = true;
                        }
                        key.wiping = key.wiping && play_wipe(&mut scene, &key, 0);
                        *task = JumpTask::FadeIn { kind, t: 0.0 };
                        return;
                    }
                };
                exits.jumping = None;
                rec.world = dest.world;
                rec.area = dest.area;
                rec.entrance = dest.entrance;
                rec.place_on_load = true;
                rec.placed = None;
                // FUN_00104b90: the destination's program bytes over the story table's, at the next area start. The
                // game restarts the area thread on every jump; the app reloads only a different area, so a jump
                // that carries program bytes into the loaded area runs its scripts again [inferred].
                rec.dest_programs = dest.programs;
                if area.loaded.as_deref() == Some(name.as_str()) && dest.programs != [KEEP; 3] {
                    rec.run = None;
                }
                area.name = name;
                info!("jump: area {} entrance {} (fade kind {kind} out done); programs {}", area.name, dest.entrance, dest.programs_text());
                *task = JumpTask::Loading { dest, kind };
            }
            return;
        }
        JumpTask::Loading { dest, kind } => {
            let name = dest_name(&dest);
            if area.loaded.as_deref() == Some(name.as_str()) && rec.placed.as_deref() == Some(name.as_str()) {
                // FUN_0015d1a8: group 0 of the layout and a 2-tick fader from black.
                key.wiping = key.wiping && play_wipe(&mut scene, &key, 0);
                info!("jump: {name} loaded; fade kind {kind} in{}", if key.wiping { " (keyhole wipe group 0, 2-tick fader from black)" } else if kind == 0 { " (black fade: no keyhole layout)" } else { "" });
                *task = JumpTask::FadeIn { kind, t: 0.0 };
            }
            return;
        }
        JumpTask::FadeIn { kind, t } => {
            let done = if key.wiping { wipe_in_done(t + 1.0, wipe_ended(&scene, &key)) } else { t + 1.0 >= FADE_TICKS };
            if done && key.wiping {
                key.wiping = false;
                if let (Some(s), Some(n)) = (scene.as_mut(), key.node) {
                    s.nodes[n].visible = false;
                }
                info!("jump: keyhole wipe in done after {} ticks", t + 1.0);
            }
            *task = if done { JumpTask::Idle } else { JumpTask::FadeIn { kind, t: t + 1.0 } };
            return;
        }
    }
    // A trigger's event: the exits wait while it plays, then its jump is taken (FUN_00196170).
    if let Some(ae) = exits.after_event.as_mut() {
        if !ae.step(player.as_ref().is_some_and(|p| p.playing())) {
            return;
        }
        let ae = exits.after_event.take().unwrap();
        match ae.jump {
            Some(j) => match resolve_jump(&j, (rec.world, rec.area, rec.entrance), last.0, |w, k| disc_localset(&disc, w, k)) {
                Ok(dest) => {
                    let (kind, _) = jump_fade_kind(&j);
                    info!("jump after the event (FUN_00196170 -> FUN_0019d9d8 with bit 31): type {} key {:#x} -> {} entrance {} programs {} (fade kind {kind}, no fade-out)", j.kind, j.key, dest_name(&dest), dest.entrance, dest.programs_text());
                    exits.jumping = Some(ae.group);
                    key.black_from = None;
                    key.wiping = false;
                    // Bit 31: no fade-out; the task goes straight to the load.
                    *task = JumpTask::FadeOut { dest, kind, t: FADE_TICKS - 1.0 };
                    return;
                }
                Err(why) => info!("jump after the event: {why}; not ported"),
            },
            None => info!("event over: the trigger has no jump record (FUN_0019d9c8 = 0)"),
        }
    }
    // The exit groups of the loaded area.
    let Some(run) = rec.run.clone() else { return };
    if exits.area.as_deref() != Some(run.name.as_str()) {
        exits.area = Some(run.name.clone());
        exits.groups = exit_groups(&run);
        if area::arealog() {
            for g in &exits.groups {
                let (w, a) = g.exit.destination(rec.world);
                info!("exits {}: {} id {:#x} -> world {w} area {a} entrance {} gate {}{} volumes {:?}", run.name, g.tag, g.id, g.exit.entrance, g.exit.gate, if g.disabled { " disabled" } else { "" }, g.volumes.iter().map(|v| (v.shape, v.option)).collect::<Vec<_>>());
                for v in &g.volumes {
                    let dir = consts.angle_base.map(|b| exit_direction(v, b));
                    info!("exits {}: {} volume option {} at app {:.0} half size {:?} rotation y {:.3} exit direction {dir:?}", run.name, g.tag, v.option, area::game_to_app(v.position), v.scale, v.rotation[1]);
                }
            }
        }
    }
    let Ok(s) = sora.single() else { return };
    let w = Walker { pos: s.motor.pos, facing: s.motor.forward(), vel: s.motor.vel };
    let world = rec.world;
    let mut start: Option<(Dest, u32, u16)> = None;
    let mut after: Option<AfterEvent> = None;
    // Harness: `KH2_WALKIN=<group id, hex>` fires that exit group's walk-in once `KH2_WALKIN_AT` seconds (default 3)
    // of simulation have passed, as if Sora had entered its option-0 volume past the gate (the decision and
    // everything after it are the real path).
    *forced += 1;
    let force = std::env::var("KH2_WALKIN").ok().and_then(|v| u16::from_str_radix(v.trim_start_matches("0x"), 16).ok()).filter(|_| {
        let at = std::env::var("KH2_WALKIN_AT").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(3.0);
        *forced == (at * 60.0) as u32
    });
    // Harness: `KH2_JUMP=<type>:<key>:<entrance>` (hex; a type-1 jump's key is the area in the current world) runs a
    // trigger jump record through `resolve_jump` and the real fade and load path at `KH2_JUMP_AT` seconds (default 3).
    if let Some(j) = harness_jump().filter(|_| *forced == (std::env::var("KH2_JUMP_AT").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(3.0) * 60.0) as u32) {
        info!("jump: forced by KH2_JUMP: record {j:?}");
        match resolve_jump(&j, (rec.world, rec.area, rec.entrance), last.0, |w, k| disc_localset(&disc, w, k)) {
            Ok(dest) => {
                info!("jump: type {} key {:#x} -> {} entrance {} programs {}", j.kind, j.key, dest_name(&dest), dest.entrance, dest.programs_text());
                start = Some((dest, jump_fade_kind(&j).0, 0xffff));
            }
            Err(why) => info!("jump: {why}; not ported"),
        }
    }
    for g in exits.groups.iter_mut().filter(|g| !g.fired && !g.disabled) {
        let p = [w.pos.x, -w.pos.y, -w.pos.z];
        if force == Some(g.id) {
            info!("exit {} id {:#x}: walk-in forced by KH2_WALKIN", g.tag, g.id);
            let (dw, da) = g.exit.destination(world);
            let dest = Dest::new(dw, da, g.exit.entrance);
            g.fired = true;
            last.record(dest);
            match walk_in(g, &run.script.triggers, world) {
                WalkIn::Event { tag, arg, jump } => {
                    info!("exit {} id {:#x} -> {} entrance {}: trigger (1, {:#x}) plays event {tag} arg {arg}, then the jump {jump:?} (FUN_00196428)", g.tag, g.id, dest_name(&dest), dest.entrance, g.id);
                    start_event_walk_in(&mut after, jump, g.id, || crate::intro::event_player::request(&tag, arg));
                }
                WalkIn::Jump { dest, kind } => start = Some((dest, kind, g.id)),
                other => info!("exit {}: forced walk-in {other:?} not handled by the harness", g.tag),
            }
            continue;
        }
        if g.held {
            g.held = g.volumes.iter().any(|v| v.contains(p));
            continue;
        }
        let mut in_prompt = false;
        // FUN_00198e10 asks FUN_00195358 about the destination before acting on any volume.
        let locked = {
            let (dw, da) = g.exit.destination(world);
            locks.locked(dw, da)
        };
        for v in &g.volumes {
            if !v.contains(p) {
                continue;
            }
            match v.option {
                0 => {
                    if locked || !gate_passes(g.exit.gate, v, &w, &consts) {
                        *refused += 1;
                        if area::arealog() && *refused % 30 == 1 {
                            let dir = consts.angle_base.map(|b| exit_direction(v, b)).unwrap_or_default();
                            info!("exit {} walk-in refused (locked {locked}, gate {}): pos {:.0} facing . dir {:.2}, velocity {:.2} . dir {:.2}", g.tag, g.exit.gate, w.pos, w.facing.dot(dir), w.vel, w.vel.normalize_or_zero().dot(dir));
                        }
                        continue;
                    }
                    let (dw, da) = g.exit.destination(world);
                    let dest = Dest::new(dw, da, g.exit.entrance);
                    g.fired = true;
                    // FUN_00198fa0 stores the destination at 0x33e840 before it asks the trigger.
                    last.record(dest);
                    match walk_in(g, &run.script.triggers, world) {
                        WalkIn::Jump { dest, kind } => {
                            info!("exit -> {} entrance {} (group {} id {:#x}, gate {}, fade kind {kind})", dest_name(&dest), dest.entrance, g.tag, g.id, g.exit.gate);
                            start = Some((dest, kind, g.id));
                        }
                        WalkIn::Event { tag, arg, jump } => {
                            info!("exit {} id {:#x} -> {} entrance {}: trigger (1, {:#x}) plays event {tag} arg {arg}, then the jump {jump:?} (FUN_00196428)", g.tag, g.id, dest_name(&dest), dest.entrance, g.id);
                            start_event_walk_in(&mut after, jump, g.id, || crate::intro::event_player::request(&tag, arg));
                        }
                        WalkIn::TriggerJump(j) => {
                            let (kind, instant) = jump_fade_kind(&j);
                            match resolve_jump(&j, (rec.world, rec.area, rec.entrance), last.0, |w, k| disc_localset(&disc, w, k)) {
                                Ok(dest) => {
                                    info!("exit {} id {:#x}: trigger jump type {} key {:#x} -> {} entrance {} programs {} (fade kind {kind}{})", g.tag, g.id, j.kind, j.key, dest_name(&dest), dest.entrance, dest.programs_text(), if instant { ", instant" } else { "" });
                                    start = Some((dest, kind, g.id));
                                }
                                Err(why) => info!("exit {} id {:#x}: trigger jump {why}; not ported", g.tag, g.id),
                            }
                        }
                        WalkIn::TriggerOnly => info!("exit {} id {:#x}: trigger (1, {:#x}) without event or jump: not ported", g.tag, g.id, g.id),
                    }
                    break;
                }
                1 | 2 if faces_volume(v, &w) => {
                    in_prompt = true;
                    if !g.prompting {
                        let (dw, da) = g.exit.destination(world);
                        info!("exit prompt {}{} (group {} option {}; the name window {} is not drawn)", dest_name(&Dest::new(dw, da, g.exit.entrance)), if locked { " locked" } else { "" }, g.tag, v.option, if locked { "FUN_0015acc8" } else { "FUN_0015ad80" });
                    }
                }
                _ => {}
            }
        }
        g.prompting = in_prompt;
    }
    if after.is_some() {
        exits.after_event = after;
    }
    if let Some((dest, kind, id)) = start {
        exits.jumping = Some(id);
        if kind > 1 {
            info!("jump: fade kind {kind} not ported (FUN_0013f290 / FUN_0013f3b0): black fade stand-in");
        }
        // FUN_0015d058: group 1 of the layout; the fader follows once it has ended.
        key.black_from = None;
        key.wiping = kind == 0 && play_wipe(&mut scene, &key, 1);
        if kind == 0 {
            info!("jump: fade kind 0 out ({})", if key.wiping { "keyhole wipe group 1, then a 2-tick fader to black".to_string() } else { format!("black fade, {FADE_TICKS} ticks: no keyhole layout") });
        }
        *task = JumpTask::FadeOut { dest, kind, t: 0.0 };
    }
}

#[derive(Component)]
struct FadeNode;

/// The black full-screen node the fade draws with (over the HUD).
fn spawn_fader(mut commands: Commands) {
    commands.spawn((
        FadeNode,
        Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        GlobalZIndex(1000),
    ));
}

/// The one fader node serves two owners: the area jump (always black) and a playing event
/// (`EventPlayer::screen_fade`, black or white, `FUN_0014a7c8` / `FUN_0014a710`). The stronger alpha wins;
/// on a tie the jump's black stays. Returns (alpha, white).
fn combined_fade(jump_alpha: f32, event: Option<(f32, bool)>) -> (f32, bool) {
    match event {
        Some((a, white)) if a > jump_alpha => (a, white),
        _ => (jump_alpha, false),
    }
}

fn draw_fader(
    task: Res<JumpTask>,
    key: Res<Keyhole>,
    player: Option<Res<crate::intro::event_player::EventPlayer>>,
    mut nodes: Query<&mut BackgroundColor, With<FadeNode>>,
) {
    let a = match *task {
        JumpTask::Idle => 0.0,
        JumpTask::FadeOut { t, .. } if key.wiping => wipe_alpha(true, t, key.black_from),
        JumpTask::FadeOut { t, .. } => fade_alpha(true, t),
        JumpTask::Loading { .. } => 1.0,
        JumpTask::FadeIn { t, .. } if key.wiping => wipe_alpha(false, t, None),
        JumpTask::FadeIn { t, .. } => fade_alpha(false, t),
    };
    let (a, white) = combined_fade(a, player.and_then(|p| p.screen_fade()));
    let c = if white { 1.0 } else { 0.0 };    for mut n in &mut nodes {
        n.0 = Color::srgba(c, c, c, a);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn volume(option: u16, position: [f32; 3], ry: f32) -> Activator {
        Activator { shape: 0, option, position, scale: [100.0, 100.0, 100.0], rotation: [0.0, ry, 0.0], raw: [0; 0x40] }
    }

    fn group(gate: u8, area: u8, id: u16, volumes: Vec<Activator>) -> ExitGroup {
        ExitGroup { tag: "m_10".into(), id, exit: Exit { area, entrance: 1, gate }, volumes, fired: false, disabled: false, prompting: false, held: false }
    }

    #[test]
    fn the_event_fade_and_the_jump_fade_share_the_node() {
        assert_eq!(combined_fade(0.0, None), (0.0, false));
        assert_eq!(combined_fade(0.4, None), (0.4, false));
        assert_eq!(combined_fade(0.0, Some((0.7, true))), (0.7, true));
        assert_eq!(combined_fade(0.0, Some((0.7, false))), (0.7, false));
        assert_eq!(combined_fade(1.0, Some((1.0, true))), (1.0, false), "a tie keeps the jump's black");
        assert_eq!(combined_fade(0.9, Some((0.3, true))), (0.9, false));
    }

    /// EN1: the jump's unload removes the area's actors once, when the task reaches `Loading`; other
    /// entities (Sora, the party) stay; a task that never loads removes nothing.
    #[test]
    fn a_jump_clears_the_area_enemies() {
        use bevy::ecs::system::RunSystemOnce;
        let mut w = bevy::ecs::world::World::new();
        let actors: Vec<Entity> = (0..3).map(|_| w.spawn(AreaActor).id()).collect();
        let other = w.spawn_empty().id();
        let dest = Dest::new(area::TWILIGHT_TOWN, 4, 0);
        w.insert_resource(JumpTask::FadeOut { dest, kind: 0, t: 0.0 });
        w.run_system_once(unload_area_actors).unwrap();
        assert!(actors.iter().all(|&e| w.get_entity(e).is_ok()), "the fade-out keeps them");
        w.insert_resource(JumpTask::Loading { dest, kind: 0 });
        w.run_system_once(unload_area_actors).unwrap();
        assert!(actors.iter().all(|&e| w.get_entity(e).is_err()), "the unload removes the area actors");
        assert!(w.get_entity(other).is_ok(), "other entities stay");
    }

    /// WP-E5 item 4: the lock halfwords are the story's; a flag whose op 2 writes entry 1 locks
    /// tt03 (any non-zero state, `FUN_00195358`), one whose op 3 writes 0 opens it again, op 4
    /// (0xffff) locks. Without a story every exit is open, as before.
    #[test]
    fn story_flag_ops_lock_and_unlock_exits() {
        // dsa: two entries, (2, 4) and (2, 3).
        let table = vec![2, 0, 2, 4, 2, 3];
        let mut locks = ExitLocks { table, story: None };
        assert!(!locks.locked(2, 3));
        // World 2 entry with 3 flags: 0x800 op 2 (entry 1, state 1, message 0x4321), 0x801 op 3
        // (entry 1), 0x802 op 4 (entry 1).
        let mut e = vec![6, 0, 15, 0, 20, 0];
        e.extend_from_slice(&[2, 3, 1, 0, 1, 0, 0x21, 0x43, 0]);
        e.extend_from_slice(&[3, 1, 1, 0, 0]);
        e.extend_from_slice(&[4, 1, 1, 0, 0]);
        let story = crate::progress::StoryHandle::new(crate::progress::Story::with_effects(std::sync::Arc::new(vec![Vec::new(), Vec::new(), e])));
        locks.story = Some(story.clone());
        assert!(!locks.locked(2, 3) && !locks.locked(2, 4));
        story.lock().set(0x800);
        assert!(locks.locked(2, 3) && !locks.locked(2, 4));
        assert_eq!(story.lock().lock_message(1), 0x4321);
        story.lock().set(0x801);
        assert!(!locks.locked(2, 3));
        story.lock().set(0x802);
        assert!(locks.locked(2, 3));
        assert_eq!(locks.entry(1), Some((2, 3)));
    }

    #[test]
    fn a_jump_to_a_missing_area_stays() {
        let d = |area| Dest::new(2, area, 1);
        let all = |_: &str| true;
        assert_eq!(jump_target(&d(3), all).as_deref(), Ok("tt03"));
        // Not in the app (world 0x12, area 100's mapping): no area name.
        assert!(jump_target(&Dest::new(0x12, 1, 0), all).is_err());
        // A Twilight Town number whose files the disc lacks.
        let only_tt03 = |f: &str| f.contains("tt03");
        assert_eq!(jump_target(&d(3), only_tt03).as_deref(), Ok("tt03"));
        let e = jump_target(&d(99), only_tt03).unwrap_err();
        assert!(e.contains("ard/tt99.ard"), "{e}");
        let no_map = |f: &str| f.starts_with("ard/");
        assert!(jump_target(&d(3), no_map).unwrap_err().contains("map/us/tt03.map"));
    }

    /// Fade kind 0's layout on the user's disc: slot 2 of `tt0field.2dd` is an image and a
    /// sequence (the same `SEQD` format the HUD draws) with the groups 0 (in) and 1 (out) that
    /// `FUN_0015d1a8` / `FUN_0015d058` play. Both groups must end (the tasks `FUN_0015cf58` /
    /// `FUN_0015d0e8` wait for the player to leave its playing states and the group's halfword +4
    /// to be non-zero). Prints the groups' lengths and their first and last frames' extents.
    #[test]
    fn the_keyhole_layout_is_slot_two_of_the_world_field_file() {
        assert_eq!(layout_slot(&[0x18, 0x19, 0x18, 0x19, 0x18, 0x19], 2), Some((4, 5)));
        assert_eq!(layout_slot(&[0x18, 0x18, 0x18, 0x19, 0x19, 0x19], 2), Some((2, 5)));
        assert_eq!(layout_slot(&[0x18, 0x19], 2), None);
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let file = disc.read("field2d/us/tt0field.2dd").expect("tt0field.2dd");
        let bar = kh2_bar::parse(&file).expect("BAR");
        let kinds: Vec<u32> = bar.iter().map(|e| e.kind as u32).collect();
        eprintln!("tt0field.2dd entries: {:?}", bar.iter().map(|e| (e.tag, e.kind)).collect::<Vec<_>>());
        let (i, s) = layout_slot(&kinds, KEYHOLE_SLOT).expect("slot 2");
        eprintln!("keyhole slot 2: image entry {i} {:?}, sequence entry {s} {:?}", bar[i].tag, bar[s].tag);
        let img = kh2_image::imgd::decode(bar[i].data).expect("image decodes");
        let seq = kh2_image::seqd::parse(bar[s].data).expect("sequence parses");
        eprintln!("keyhole image {}x{}, {} groups", img.width, img.height, seq.groups.len());
        for g in 0..2 {
            let grp = &seq.groups[g];
            let last = seq.last_frame(g);
            let ext = kh2_image::seqd::Ext::default();
            let bbox = |f: i32| {
                let qs = seq.quads(g, f, [0.0, 0.0], &ext);
                let xs = qs.iter().flat_map(|q| q.pos.iter().map(|p| p[0]));
                let ys = qs.iter().flat_map(|q| q.pos.iter().map(|p| p[1]));
                (qs.len(), xs.clone().fold(f32::MAX, f32::min), ys.clone().fold(f32::MAX, f32::min), xs.fold(f32::MIN, f32::max), ys.fold(f32::MIN, f32::max))
            };
            eprintln!("keyhole group {g}: loops {} loop start {} last frame {last}; frame 0 quads/bbox {:?}; last frame {:?}", grp.loops, grp.loop_start, bbox(0), bbox(last));
            assert!(grp.loops != 0, "group {g} must end");
            assert!(last > 0);
        }
        // The HUD's `SeqFile::from_bar` list holds this pair at the index `load_keyhole` uses.
        let ok = |j: usize| j + 1 < bar.len() && kh2_image::imgd::decode(bar[j].data).is_some() && kh2_image::seqd::parse(bar[j + 1].data).is_some();
        let idx = pair_index(i, ok).expect("slot 2 pair decodes");
        eprintln!("keyhole: HUD file index {idx}");
        assert_eq!(s, i + 1);
    }

    /// The wipe (`FUN_0015cf58` / `FUN_0015d0e8`): out waits for group 1 to end, then the fader
    /// reaches black in 2 ticks; in fades from black in 2 ticks and ends with group 0.
    #[test]
    fn the_keyhole_wipe_waits_for_its_group_then_fades_in_two_ticks() {
        let mut b = None;
        assert!(!wipe_out_step(1.0, false, &mut b));
        assert_eq!(wipe_alpha(true, 1.0, b), 0.0);
        assert!(!wipe_out_step(71.0, false, &mut b));
        assert!(!wipe_out_step(72.0, true, &mut b));
        assert_eq!(b, Some(72.0));
        assert_eq!(wipe_alpha(true, 73.0, b), 0.5);
        assert!(wipe_out_step(74.0, true, &mut b));
        assert_eq!(wipe_alpha(true, 74.0, b), 1.0);
        assert_eq!(wipe_alpha(false, 0.0, None), 1.0);
        assert_eq!(wipe_alpha(false, 1.0, None), 0.5);
        assert_eq!(wipe_alpha(false, 2.0, None), 0.0);
        assert!(!wipe_in_done(1.0, true));
        assert!(!wipe_in_done(40.0, false));
        assert!(wipe_in_done(72.0, true));
        // The guard: a group that never ends.
        let mut b = None;
        assert!(!wipe_out_step(WIPE_MAX_TICKS, false, &mut b));
        assert!(wipe_out_step(WIPE_MAX_TICKS + 2.0, false, &mut b));
        // from_bar's index: pairs that decode before the image entry.
        assert_eq!(pair_index(4, |j| j % 2 == 0), Some(2));
        assert_eq!(pair_index(4, |j| j == 4 || j == 0), Some(1));
        assert_eq!(pair_index(4, |j| j != 4), None);
    }

    #[test]
    fn the_fade_takes_fifteen_times_two_ticks_each_way() {
        assert_eq!(FADE_TICKS, 30.0);
        assert_eq!(fade_alpha(true, 0.0), 0.0);
        assert!((fade_alpha(true, 15.0) - 0.5).abs() < 1e-6);
        assert_eq!(fade_alpha(true, 30.0), 1.0);
        assert_eq!(fade_alpha(true, 99.0), 1.0);
        assert_eq!(fade_alpha(false, 0.0), 1.0);
        assert_eq!(fade_alpha(false, 30.0), 0.0);
    }

    #[test]
    fn exit_gates_pass_zero_and_need_a_walker_along_the_exit_for_one_and_two() {
        // The executable's values (DAT_0037c714 = -PI/2, DAT_0037b358 = 0.7) as the test reads them.
        let c = ExitConsts { angle_base: Some(-FRAC_PI_2), min_dot: Some(0.7) };
        // A volume with rotation y 0: the exit direction is (sin(-PI/2), 0, cos(-PI/2)) = -X in the
        // game's space, which is -X in the app too.
        let v = volume(0, [0.0; 3], 0.0);
        assert!((exit_direction(&v, -FRAC_PI_2) - Vec3::NEG_X).length() < 1e-6);
        // Rotation y PI/2 turns it to (sin(-PI), 0, cos(-PI)) = -Z in the game = +Z here.
        assert!((exit_direction(&volume(0, [0.0; 3], FRAC_PI_2), -FRAC_PI_2) - Vec3::Z).length() < 1e-6);
        let along = Walker { pos: Vec3::ZERO, facing: Vec3::NEG_X, vel: Vec3::NEG_X * 5.0 };
        let still = Walker { vel: Vec3::ZERO, ..along };
        let sideways = Walker { facing: Vec3::NEG_X, vel: Vec3::Z * 5.0, ..along };
        let away = Walker { facing: Vec3::X, vel: Vec3::X * 5.0, ..along };
        for gate in [1u8, 2] {
            assert!(gate_passes(gate, &v, &along, &c));
            assert!(!gate_passes(gate, &v, &still, &c));
            assert!(!gate_passes(gate, &v, &sideways, &c));
            assert!(!gate_passes(gate, &v, &away, &c));
        }
        assert!(gate_passes(0, &v, &away, &c));
        assert!(!gate_passes(3, &v, &along, &c));
        // Without the executable's constants a gated exit fails closed.
        assert!(!gate_passes(1, &v, &along, &ExitConsts::default()));
        // 45 degrees off (cos 0.707) still passes 0.7; 60 degrees (0.5) does not.
        let d45 = Vec3::new(-1.0, 0.0, 1.0).normalize();
        assert!(gate_passes(1, &v, &Walker { pos: Vec3::ZERO, facing: d45, vel: d45 }, &c));
        let d60 = Vec3::new(-0.5, 0.0, 0.866);
        assert!(!gate_passes(1, &v, &Walker { pos: Vec3::ZERO, facing: d60, vel: d60 }, &c));
    }

    #[test]
    fn a_walk_in_jumps_by_the_exit_unless_a_trigger_on_event_one_takes_it() {
        let g = group(2, 3, 0x20, vec![volume(0, [0.0; 3], 0.0)]);
        // No trigger: the exit's own destination in the same world, fade kind 0.
        assert_eq!(walk_in(&g, &[], 2), WalkIn::Jump { dest: Dest::new(2, 3, 1), kind: 0 });
        // Area 100 is world 0x12 area 1: a world change, fade kind 1.
        let far = group(0, 100, 0x9, vec![]);
        assert_eq!(walk_in(&far, &[], 2), WalkIn::Jump { dest: Dest::new(0x12, 1, 1), kind: 1 });
        // A trigger (1, 0x20) with an event tag "110" and a jump record.
        let ev = Trigger { event: 1, arg: 0x20, subs: vec![(0, vec![2, 0x3131, 0x30]), (1, vec![0, 1, 2, 8, 0, 0, 0x4001])] };
        match walk_in(&g, &[ev.clone()], 2) {
            WalkIn::Event { tag, arg, jump: Some(j) } => {
                assert_eq!((tag.as_str(), arg), ("110", 2));
                assert_eq!((j.kind, j.world, j.area, j.entrance), (1, 2, 8, 0));
                assert_eq!(jump_fade_kind(&j), (1, true));
            }
            other => panic!("{other:?}"),
        }
        // A trigger for another group id does not match; one with arg -1 matches every group.
        let other = Trigger { event: 1, arg: 0x21, ..ev.clone() };
        assert!(matches!(walk_in(&g, &[other], 2), WalkIn::Jump { .. }));
        let any = Trigger { event: 1, arg: 0xffff, subs: vec![(1, vec![0, 1, 2, 5, 3, 0, 0])] };
        assert_eq!(walk_in(&g, &[any], 2), WalkIn::TriggerJump(Jump { kind: 1, world: 2, area: 5, entrance: 3, key: 0, flags: 0 }));
        // The prompt volume: facing the centre or not.
        let pv = volume(1, [0.0, 0.0, 500.0], 0.0);
        let toward = Walker { pos: Vec3::ZERO, facing: Vec3::NEG_Z, vel: Vec3::ZERO };
        assert!(faces_volume(&pv, &toward));
        assert!(!faces_volume(&pv, &Walker { facing: Vec3::Z, ..toward }));
    }

    /// WP-E6b: the jump a trigger's event leaves waits while the event plays and is due on the first tick it
    /// no longer plays (`FUN_00196170`); an event that never starts does not hold it longer than
    /// `AfterEvent::START_TICKS`.
    #[test]
    fn the_jump_after_a_trigger_event_waits_for_the_events_end() {
        let j = Jump { kind: 1, world: 2, area: 8, entrance: 0, key: 0, flags: 0x4001 };
        let mut ae = AfterEvent::new(Some(j), 0x20);
        assert!(!ae.step(false), "the request is taken on the next tick");
        for _ in 0..500 {
            assert!(!ae.step(true));
        }
        assert!(ae.step(false), "the event is over: the jump is due");
        let mut never = AfterEvent::new(None, 0x20);
        let due: Vec<bool> = (0..4).map(|_| never.step(false)).collect();
        assert_eq!(due, vec![false, false, true, true]);
    }

    /// FIX-R1 6: two exit groups with an event walk-in in one tick request one event and keep the first jump.
    #[test]
    fn only_the_first_event_walk_in_of_a_tick_is_taken() {
        let j1 = Jump { kind: 1, world: 2, area: 8, entrance: 0, key: 0, flags: 0 };
        let j2 = Jump { kind: 1, world: 2, area: 9, entrance: 1, key: 0, flags: 0 };
        let mut after = None;
        let mut requests = 0;
        assert!(start_event_walk_in(&mut after, Some(j1), 0x20, || requests += 1));
        assert!(!start_event_walk_in(&mut after, Some(j2), 0x21, || requests += 1));
        assert_eq!(requests, 1);
        let a = after.expect("the first walk-in");
        assert_eq!((a.group, a.jump), (0x20, Some(j1)));
    }

    /// WP-E6b: every Twilight Town trigger on event 1 (an exit's walk-in) that names an event, with its jump
    /// record and whether the area file holds the event.
    #[test]
    fn twilight_town_walk_in_triggers_with_events() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let mut found = 0;
        for n in 0..64u8 {
            let Some(name) = area::area_name(2, n) else { continue };
            let Some(file) = disc.read(&format!("ard/{name}.ard")) else { continue };
            let Some((ard, _)) = kh2_ard::Area::parse(&file) else { continue };
            for (tag, progs) in &ard.scripts {
                for p in progs {
                    for c in p.commands.iter().filter(|c| c.opcode == 0xc) {
                        let Some(t) = kh2_ard::parse_trigger(&c.args) else { continue };
                        if let Some((ev, arg)) = t.event_tag() {
                            found += (t.event == 1) as usize;
                            eprintln!("{name} {tag} program {:#x}: trigger ({:#x}, {:#x}) event {ev} arg {arg} (in the area: {}) jump {:?}", p.id, t.event, t.arg, ard.events.contains_key(&ev), t.jump());
                        }
                    }
                }
            }
        }
        eprintln!("walk-in triggers with an event: {found}");
        assert!(found > 0, "Twilight Town has trigger (1, ..) walk-ins that play an event");
    }

    fn jump(kind: u16, world: u16, area: u16, entrance: u16, key: u16) -> Jump {
        Jump { kind, world, area, entrance, key, flags: 0 }
    }

    /// WP-JT2 A: a type-2 record goes to the area the table gives for the key, with map, btl and evt programs all
    /// the key and the record's entrance (`FUN_0019d8f0` -> `FUN_00198080`); a negative world is the current one;
    /// an unknown key keeps the current world and area (every byte 0xff, `FUN_00104bd0`).
    #[test]
    fn a_type_two_jump_goes_to_the_keys_area_with_all_three_programs() {
        let table = |w: u8, k: u16| (w == 2 && (k == 0xb5 || k == 0x4d)).then_some(4u8);
        let cur = (2, 3, 7);
        let d = resolve_jump(&jump(2, 0xffff, 0xffff, 0, 0xb5), cur, None, table).unwrap();
        assert_eq!(d, Dest { world: 2, area: 4, entrance: 0, programs: [0xb5; 3] });
        assert_eq!(d.programs_text(), "(0xb5, 0xb5, 0xb5)");
        let d = resolve_jump(&jump(2, 2, 0, 5, 0x4d), cur, None, table).unwrap();
        assert_eq!((d.world, d.area, d.entrance, d.programs), (2, 4, 5, [0x4d; 3]));
        // Unknown key: stays in the current area at the record's entrance, the table's programs.
        let d = resolve_jump(&jump(2, 0xffff, 0xffff, 1, 0x01), cur, None, table).unwrap();
        assert_eq!(d, Dest { world: 2, area: 3, entrance: 1, programs: [KEEP; 3] });
        // An entrance of 0xff keeps the current one.
        assert_eq!(resolve_jump(&jump(2, 0xffff, 0xffff, 0xffff, 0xb5), cur, None, table).unwrap().entrance, 7);
    }

    /// Type 1 keeps its own bytes (0xff keeps the current record's); type 3 goes where the last exit pointed, and
    /// before any exit has fired (all 0xff) stays where it is; the exit's walk-in is what records the target.
    #[test]
    fn a_type_three_jump_returns_to_the_last_exits_target() {
        let none = |_: u8, _: u16| None;
        let cur = (2, 4, 2);
        assert_eq!(resolve_jump(&jump(1, 2, 8, 0, 0), cur, None, none).unwrap(), Dest::new(2, 8, 0));
        assert_eq!(resolve_jump(&jump(1, 0xffff, 9, 0xffff, 0), cur, None, none).unwrap(), Dest::new(2, 9, 2));
        let mut last = LastExit::default();
        assert_eq!(resolve_jump(&jump(3, 0, 0, 0, 0), cur, last.0, none).unwrap(), Dest::new(2, 4, 2), "no exit yet: the record stays");
        // An exit to tt03 entrance 1 fires (the programs of the remembered record are 0xff, whatever the exit passed).
        last.record(Dest { world: 2, area: 3, entrance: 1, programs: [9, 9, 9] });
        assert_eq!(last.0, Some(Dest::new(2, 3, 1)));
        assert_eq!(resolve_jump(&jump(3, 0, 0, 0, 0), cur, last.0, none).unwrap(), Dest::new(2, 3, 1));
        // The next exit replaces it (world 0x12 area 1 for the exit area 100).
        last.record(Dest::new(0x12, 1, 0));
        assert_eq!(resolve_jump(&jump(3, 0, 0, 0, 0), cur, last.0, none).unwrap(), Dest::new(0x12, 1, 0));
    }

    /// Types 4, 5, 6 (and an unknown one) are not ported and say why.
    #[test]
    fn jump_types_four_to_six_are_not_ported() {
        let none = |_: u8, _: u16| None;
        for k in [4u16, 5, 6, 7] {
            let e = resolve_jump(&jump(k, 0, 0, 0, 0), (2, 4, 0), None, none).unwrap_err();
            assert!(!e.is_empty(), "type {k}");
        }
        assert!(resolve_jump(&jump(4, 0, 0, 0, 0), (2, 4, 0), None, none).unwrap_err().contains("0x1071"));
    }

    /// WP-JT2 A on the disc: the localset table the app reads gives key 0xb5 and 0x4d tt04.
    #[test]
    fn the_disc_localset_gives_the_prologue_keys_tt04() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        assert_eq!(disc_localset(&disc, 2, 0xb5), Some(4));
        assert_eq!(disc_localset(&disc, 2, 0x4d), Some(4));
        let d = resolve_jump(&jump(2, 0xffff, 0xffff, 0, 0xb5), (2, 3, 0), None, |w, k| disc_localset(&disc, w, k)).unwrap();
        assert_eq!(dest_name(&d), "tt04");
        eprintln!("type 2 key 0xb5 from tt03 -> {} entrance {} programs {}", dest_name(&d), d.entrance, d.programs_text());
    }
}
