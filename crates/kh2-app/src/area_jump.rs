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
//! 2-tick fader fade-in): the keyhole wipe of the reference video. That layout is not decoded, so
//! kind 0 is drawn as the same black fade, labelled in the log as a stand-in.
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
//! and the save-block halfwords at 0x33f918, all 0 in the app until E5 runs the story flags'
//! ops 2..5; the world-0x10 branch `FUN_001d32c0` is not ported).

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
            .init_resource::<ExitConsts>()
            .init_resource::<ExitLocks>()
            .add_systems(Startup, (load_consts, spawn_fader))
            .add_systems(FixedUpdate, tick.after(Sim::Sora).before(Sim::Enemy))
            .add_systems(Update, draw_fader.in_set(crate::schedule::Frame::Hud));
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

fn load_consts(disc: Res<Disc>, mut consts: ResMut<ExitConsts>, mut locks: ResMut<ExitLocks>) {
    if let Some(table) = disc.read("00progress.bin").and_then(|f| kh2_bar::parse(&f).ok()?.into_iter().find(|e| e.tag == kh2_ard::progress::LOCK_TABLE).map(|e| e.data.to_vec())) {
        let n = table.get(..2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize);
        locks.words = vec![0; n];
        locks.table = table;
        if area::arealog() {
            info!("exits: lock table {} with {n} areas, every lock halfword 0 (no save block 0x33f918 in the app; flag ops 2..5 are E5)", kh2_ard::progress::LOCK_TABLE);
        }
    }
    if area::arealog()
        && let Some(f) = disc.read("field2d/us/tt0field.2dd")
        && let Ok(bar) = kh2_bar::parse(&f)
    {
        let kinds: Vec<u32> = bar.iter().map(|e| e.kind as u32).collect();
        info!("exits: fade kind 0 layout = slot {KEYHOLE_SLOT} of tt0field.2dd (image, sequence entries {:?}); not drawn, black stand-in", layout_slot(&kinds, KEYHOLE_SLOT));
    }
    let Some(exe) = disc.read_root("SLUS_210.05") else { return };
    let f = |va: usize| exe.get(va - EXE_BIAS..va - EXE_BIAS + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    *consts = ExitConsts { angle_base: f(0x37c714), min_dot: f(0x37b358) };
    if area::arealog() {
        info!("exits: gate constants angle base {:?} min dot {:?}", consts.angle_base, consts.min_dot);
    }
}

/// The exit lock (`FUN_00195358`): the `dsa` table of `00progress.bin` (`DAT_01c8a988`) and one
/// lock halfword per entry (the save block's `0x33f918 + index * 4`). The app has no save block
/// there, so the halfwords stay 0 (open) until the story-flag side effects ops 2..5 are ported
/// (E5) [inferred: a new game starts with them 0; the new-game initialiser was not read].
#[derive(Resource, Default, Debug)]
pub struct ExitLocks {
    pub table: Vec<u8>,
    pub words: Vec<u16>,
}

impl ExitLocks {
    /// `FUN_00195358(world, area)` != 0. The world-0x10 branch (`FUN_001d32e0`, story flags) is
    /// not ported: the app has only Twilight Town.
    pub fn locked(&self, world: u8, area: u8) -> bool {
        kh2_ard::progress::exit_locked(&self.table, |i| self.words.get(i).copied().unwrap_or(0), world, area, false)
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

/// A destination (the six bytes `FUN_00104e18` receives, without the program overrides).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dest {
    pub world: u8,
    pub area: u8,
    pub entrance: u8,
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
    WalkIn::Jump { dest: Dest { world: w, area: a, entrance: g.exit.entrance }, kind: (w != world) as u32 }
}

/// The fade kind of a trigger's jump record: the flags word, where bit 0x4000 means "record the
/// kind only" (`FUN_0013ef68` with bit 31 set) and the kind is the low bits.
pub fn jump_fade_kind(j: &Jump) -> (u32, bool) {
    ((j.flags & 0x3fff) as u32, j.flags & 0x4000 != 0)
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
) {
    // The jump task.
    match *task {
        JumpTask::Idle => {}
        JumpTask::FadeOut { dest, kind, t } => {
            if t + 1.0 < FADE_TICKS {
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
                area.name = name;
                info!("jump: area {} entrance {} (fade kind {kind} out done)", area.name, dest.entrance);
                *task = JumpTask::Loading { dest, kind };
            }
            return;
        }
        JumpTask::Loading { dest, kind } => {
            let name = dest_name(&dest);
            if area.loaded.as_deref() == Some(name.as_str()) && rec.placed.as_deref() == Some(name.as_str()) {
                info!("jump: {name} loaded; fade kind {kind} in{}", if kind == 0 { " (keyhole layout 0x355118 slot 2 decoded, not drawn: black stand-in)" } else { "" });
                *task = JumpTask::FadeIn { kind, t: 0.0 };
            }
            return;
        }
        JumpTask::FadeIn { kind, t } => {
            *task = if t + 1.0 < FADE_TICKS { JumpTask::FadeIn { kind, t: t + 1.0 } } else { JumpTask::Idle };
            return;
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
            }
        }
    }
    let Ok(s) = sora.single() else { return };
    let w = Walker { pos: s.motor.pos, facing: s.motor.forward(), vel: s.motor.vel };
    let world = rec.world;
    let mut start: Option<(Dest, u32, u16)> = None;
    for g in exits.groups.iter_mut().filter(|g| !g.fired && !g.disabled) {
        let p = [w.pos.x, -w.pos.y, -w.pos.z];
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
                        continue;
                    }
                    let (dw, da) = g.exit.destination(world);
                    let dest = Dest { world: dw, area: da, entrance: g.exit.entrance };
                    g.fired = true;
                    match walk_in(g, &run.script.triggers, world) {
                        WalkIn::Jump { dest, kind } => {
                            info!("exit -> {} entrance {} (group {} id {:#x}, gate {}, fade kind {kind})", dest_name(&dest), dest.entrance, g.tag, g.id, g.exit.gate);
                            start = Some((dest, kind, g.id));
                        }
                        WalkIn::Event { tag, arg, jump } => info!("exit {} id {:#x} -> {} entrance {}: trigger (1, {:#x}) plays event {tag} arg {arg} then {jump:?}: event not played (E6)", g.tag, g.id, dest_name(&dest), dest.entrance, g.id),
                        WalkIn::TriggerJump(j) => {
                            let (kind, instant) = jump_fade_kind(&j);
                            if j.kind == 1 {
                                let dest = Dest { world: j.world as u8, area: j.area as u8, entrance: j.entrance as u8 };
                                info!("exit {} id {:#x}: trigger jump type 1 -> {} entrance {} (fade kind {kind}{})", g.tag, g.id, dest_name(&dest), dest.entrance, if instant { ", instant" } else { "" });
                                start = Some((dest, kind, g.id));
                            } else {
                                info!("exit {} id {:#x}: trigger jump type {} key {:#x} not ported (E5)", g.tag, g.id, j.kind, j.key);
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
                        info!("exit prompt {}{} (group {} option {}; the name window {} is not drawn)", dest_name(&Dest { world: dw, area: da, entrance: g.exit.entrance }), if locked { " locked" } else { "" }, g.tag, v.option, if locked { "FUN_0015acc8" } else { "FUN_0015ad80" });
                    }
                }
                _ => {}
            }
        }
        g.prompting = in_prompt;
    }
    if let Some((dest, kind, id)) = start {
        exits.jumping = Some(id);
        if kind > 1 {
            info!("jump: fade kind {kind} not ported (FUN_0013f290 / FUN_0013f3b0): black fade stand-in");
        } else if kind == 0 {
            info!("jump: fade kind 0 out (keyhole layout 0x355118 slot 2 decoded, not drawn: black stand-in, {FADE_TICKS} ticks)");
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

fn draw_fader(task: Res<JumpTask>, mut nodes: Query<&mut BackgroundColor, With<FadeNode>>) {
    let a = match *task {
        JumpTask::Idle => 0.0,
        JumpTask::FadeOut { t, .. } => fade_alpha(true, t),
        JumpTask::Loading { .. } => 1.0,
        JumpTask::FadeIn { t, .. } => fade_alpha(false, t),
    };
    for mut c in &mut nodes {
        c.0 = Color::srgba(0.0, 0.0, 0.0, a);
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
    fn a_jump_to_a_missing_area_stays() {
        let d = |area| Dest { world: 2, area, entrance: 1 };
        let all = |_: &str| true;
        assert_eq!(jump_target(&d(3), all).as_deref(), Ok("tt03"));
        // Not in the app (world 0x12, area 100's mapping): no area name.
        assert!(jump_target(&Dest { world: 0x12, area: 1, entrance: 0 }, all).is_err());
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
        assert_eq!(walk_in(&g, &[], 2), WalkIn::Jump { dest: Dest { world: 2, area: 3, entrance: 1 }, kind: 0 });
        // Area 100 is world 0x12 area 1: a world change, fade kind 1.
        let far = group(0, 100, 0x9, vec![]);
        assert_eq!(walk_in(&far, &[], 2), WalkIn::Jump { dest: Dest { world: 0x12, area: 1, entrance: 1 }, kind: 1 });
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
}
