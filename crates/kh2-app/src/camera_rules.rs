//! The original game's field camera as pure functions (model and sources in `docs/camera.md`).
//!
//! Everything here works in the game's units (centimetres) in map-model space, Y up, at the
//! game's 60 Hz tick: each rule is one tick of the original's camera object (`FUN_00140a80`). The
//! original keeps Y pointing down; heights below are already converted to Y up. Yaw is the angle
//! of the camera's offset from its look-at point: the offset is `(sin yaw, ., cos yaw) * radius`,
//! and "behind the character" is `atan2(-fx, -fz)` for a facing `(fx, ., fz)`.

use bevy::math::{Vec2, Vec3, Vec3Swizzles};
use std::f32::consts::{PI, TAU};

/// Look-at point above the character's feet (`FUN_0013f740`: position y minus 170).
pub const LOOK_HEIGHT: f32 = 170.0;
/// The camera sits this far below the look-at point before the radius term (`FUN_0013f7a0`).
const BASE_DROP: f32 = 150.0;
/// Radius change per tick at full stick (`FUN_0013fd08`, 8 per tick).
pub const ZOOM_RATE: f32 = 8.0;
/// Most dynamic blockers the game keeps (`FUN_0015a358` allocates 0x800 bytes of 0x20-byte
/// entries, `FUN_0015a3e8` fills at most 0x40).
pub const MAX_BLOCKERS: usize = 64;

/// One row of the camera parameter table (`FUN_00140d48` copies a row into the camera object).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Row {
    /// Horizontal field of view for the game's 4:3 picture, radians.
    pub fov: f32,
    /// Default orbit radius, and the radius the camera relaxes to while locked on.
    pub radius: f32,
    pub lock_radius: f32,
    /// Zoom range of the right stick.
    pub min_radius: f32,
    pub max_radius: f32,
    /// Height curve: the camera rises by `height_k * (radius - min_radius)^2`.
    pub height_k: f32,
}

/// Look-around (mode 1) constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LookParams {
    /// Look-at point this far ahead of the eye (`FUN_00140418`, `DAT_00378e74`).
    pub distance: f32,
    /// Pitch range (`FUN_00140418`, `DAT_004842f0`/`DAT_004842f4` from 0x378f18/0x378f1c).
    pub pitch_min: f32,
    pub pitch_max: f32,
    /// Player fade: value at entry, ticks to fade out, value at exit, ticks to fade in
    /// (`FUN_0013fe20`, `FUN_00140f28`: 0x378e58, 0x378e54, 0x378f00, 0x378e78).
    pub fade_from: f32,
    pub fade_out_ticks: f32,
    pub fade_to: f32,
    pub fade_in_ticks: f32,
    /// The player's turn per tick while looking, in the game's facing sign: right stick left
    /// (pad bit 0x100000) and right (0x200000) (`FUN_001779e8`: 0x37a4c8, 0x37a4cc).
    pub turn_left: f32,
    pub turn_right: f32,
}

/// Overhead camera (mode 5, area camera type 1) constants (`FUN_00158eb0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverheadParams {
    /// Floor probe above and below the player (`FUN_00158d88`, 0x37939c, 0x3793a0).
    pub probe_up: f32,
    pub probe_down: f32,
    /// The look-at point sits this far above the floor hit (0x3793c8).
    pub raise: f32,
    /// Ease distance of the look-at point (0x3793cc).
    pub ease: f32,
    /// Camera offset from the look-at point, in world axes (0x3793a8, 0x3793b0; Y up here).
    pub offset: Vec3,
    /// The Select view: fixed position and look-at point (0x3793b4/b8, 0x3793bc/c0) and fov
    /// (0x3793c4).
    pub fixed_pos: Vec3,
    pub fixed_look: Vec3,
    pub fixed_fov: f32,
    /// Field of view of the following view (`DAT_005204b0` from 0x3793d0).
    pub fov: f32,
}

/// Spring constants of a scripted (event) camera (`FUN_0015cb70`, `FUN_0015cd20`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventParams {
    pub k: f32,
    pub c: f32,
    pub t_look: f32,
    pub rest: f32,
    pub t_pos: f32,
    pub t_fov: f32,
    pub k_fov: f32,
    pub c_fov: f32,
    pub rest_fov: f32,
}

/// Talk camera (mode 4) constants (`FUN_00159688`, `FUN_001592d8`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TalkParams {
    /// Field of view of the talk view (`DAT_00520500`, set from 0x3793e4 by the static
    /// initialiser `FUN_00159808` -> `FUN_001597a0`).
    pub fov: f32,
    /// Ticks of the player's fade-in when a talk ends the look-around (`FUN_00140f28(DAT_003793d8)`).
    pub look_fade_ticks: f32,
}

/// The camera's numbers, read from the user's executable (see [`Params::from_exe`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    pub rows: [Row; 4],
    /// Largest yaw change per tick when the camera swings toward its target yaw.
    pub yaw_step: f32,
    /// Auto-camera turn rate: `auto_base + auto_per_speed * (run speed - 8)` radians per tick.
    pub auto_base: f32,
    pub auto_per_speed: f32,
    /// Right-stick yaw rate per tick at full tilt; the look-around pitch uses the same number
    /// (`FUN_00141018` copies 0x378f14 to both `DAT_004842e8` and `DAT_004842ec`).
    pub stick_yaw: f32,
    /// Distance a collided camera is kept off the surface along its normal (`FUN_0013f8f8`,
    /// `DAT_00378e48`), and the larger margin for callers that ask for it (`DAT_00378e44`).
    pub wall_margin: f32,
    pub wall_margin_far: f32,
    pub look: LookParams,
    pub overhead: OverheadParams,
    pub event: EventParams,
    pub talk: TalkParams,
}

/// Virtual address of the first byte of the main executable image; file offset = VA - 0xff000.
const EXE_BIAS: usize = 0xff000;

impl Params {
    /// Reads the table from the USA main executable (`SLUS_210.05`). The game builds its table in
    /// a static initialiser (`FUN_00141018`) from constants stored at these addresses.
    pub fn from_exe(exe: &[u8]) -> Option<Params> {
        let f = |va: usize| -> Option<f32> {
            let o = va.checked_sub(EXE_BIAS)?;
            exe.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        };
        let row = |v: [usize; 6]| -> Option<Row> {
            Some(Row { fov: f(v[0])?, radius: f(v[1])?, lock_radius: f(v[2])?, min_radius: f(v[3])?, max_radius: f(v[4])?, height_k: f(v[5])? })
        };
        // The game's vectors are (x, y, z) with Y down; the app is Y up: y and z negated.
        let v = |x: usize, y: usize| -> Option<Vec3> { Some(Vec3::new(f(x)?, -f(y)?, 0.0)) };
        let p = Params {
            rows: [
                row([0x378f34, 0x378f38, 0x378f3c, 0x378f40, 0x378f44, 0x378f48])?,
                row([0x378f04, 0x378f20, 0x378f24, 0x378f28, 0x378f30, 0x378f2c])?,
                row([0x378f04, 0x378f20, 0x378f24, 0x378f28, 0x378f4c, 0x378f2c])?,
                row([0x378f04, 0x378f20, 0x378f24, 0x378f28, 0x378f30, 0x378f2c])?,
            ],
            yaw_step: f(0x378f08)?,
            auto_base: f(0x378f0c)?,
            auto_per_speed: f(0x378f10)?,
            stick_yaw: f(0x378f14)?,
            wall_margin: f(0x378e48)?,
            wall_margin_far: f(0x378e44)?,
            look: LookParams {
                distance: f(0x378e74)?,
                pitch_min: f(0x378f18)?,
                pitch_max: f(0x378f1c)?,
                fade_from: f(0x378e58)?,
                fade_out_ticks: f(0x378e54)?,
                fade_to: f(0x378f00)?,
                fade_in_ticks: f(0x378e78)?,
                turn_left: f(0x37a4c8)?,
                turn_right: f(0x37a4cc)?,
            },
            overhead: OverheadParams {
                probe_up: f(0x37939c)?,
                probe_down: f(0x3793a0)?,
                raise: f(0x3793c8)?,
                ease: f(0x3793cc)?,
                offset: v(0x3793a8, 0x3793b0)?,
                fixed_pos: v(0x3793b4, 0x3793b8)?,
                fixed_look: v(0x3793bc, 0x3793c0)?,
                fixed_fov: f(0x3793c4)?,
                fov: f(0x3793d0)?,
            },
            event: EventParams {
                k: f(0x379470)?,
                c: f(0x379474)?,
                t_look: f(0x379478)?,
                rest: f(0x37947c)?,
                t_pos: f(0x379480)?,
                t_fov: f(0x379484)?,
                k_fov: f(0x379488)?,
                c_fov: f(0x37948c)?,
                rest_fov: f(0x379490)?,
            },
            talk: TalkParams { fov: f(0x3793e4)?, look_fade_ticks: f(0x3793d8)? },
        };
        p.sane().then_some(p)
    }

    /// Guards against reading a different executable: every number finite and in a plausible range.
    fn sane(&self) -> bool {
        self.rows.iter().all(|r| {
            r.fov > 0.3
                && r.fov < 3.0
                && r.radius > 50.0
                && r.max_radius < 5000.0
                && r.min_radius > 0.0
                && r.min_radius <= r.radius
                && r.radius <= r.max_radius
                && r.lock_radius > 0.0
                && r.height_k.is_finite()
        }) && self.yaw_step > 0.0
            && self.stick_yaw > 0.0
            && self.auto_base.is_finite()
            && self.auto_per_speed.is_finite()
            && self.wall_margin > 0.0
            && self.wall_margin_far >= self.wall_margin
            && self.look.distance > 0.0
            && self.look.pitch_min < 0.0
            && self.look.pitch_max > 0.0
            && self.look.fade_out_ticks > 0.0
            && self.look.fade_in_ticks > 0.0
            && self.look.turn_left.abs() < 1.0
            && self.look.turn_right.abs() < 1.0
            && self.overhead.probe_down > 0.0
            && self.overhead.ease > 0.0
            && self.overhead.fov > 0.3
            && self.overhead.fov < 3.0
            && self.event.t_look > 0.0
            && self.event.t_pos > 0.0
            && self.event.t_fov > 0.0
            && self.talk.fov > 0.3
            && self.talk.fov < 3.0
            && self.talk.look_fade_ticks > 0.0
    }
}

/// Angle wrapped into [-pi, pi) (`FUN_0017bbc0`).
pub fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// Moves `cur` toward `target` by at most `step` along the short way round (`FUN_0017bc30`).
pub fn approach_angle(cur: f32, target: f32, step: f32) -> f32 {
    let d = wrap(target - cur);
    wrap(if d < -step {
        cur - step
    } else if d > step {
        cur + step
    } else {
        cur + d
    })
}

/// Yaw that puts the camera behind a character facing `facing` (`FUN_0013fc28`).
pub fn behind_yaw(facing: Vec3) -> f32 {
    (-facing.x).atan2(-facing.z)
}

/// The spring used for the look-at point, the position and the field of view (`FUN_0013f600`).
/// `cur` is the smoothed value, `new` this tick's wanted value, `prev` last tick's wanted value:
/// it moves along the line from `new` to `cur` by `t * (k * (l - len) + c * u.d)`, where `len` is
/// their distance, `u` the unit vector from `new` to `cur` and `d = (prev - new) * t`.
pub fn spring(cur: Vec3, new: Vec3, prev: Vec3, t: f32, k: f32, c: f32, l: f32) -> Vec3 {
    let d1 = cur - new;
    let len = d1.length();
    if len == 0.0 {
        return cur;
    }
    let u = d1 / len;
    let d2 = (prev - new) * t;
    cur + u * ((k * (l - len) + c * u.dot(d2)) * t)
}

/// Where the camera wants to be: on a circle of `radius` around the look-at point, at a height
/// that follows a parabola of the radius (`FUN_0013f7a0`).
pub fn wanted_position(look: Vec3, radius: f32, yaw: f32, min_radius: f32, height_k: f32) -> Vec3 {
    let d = radius - min_radius;
    Vec3::new(look.x + radius * yaw.sin(), look.y - BASE_DROP + height_k * d * d, look.z + radius * yaw.cos())
}

/// Auto-camera: yaw step per tick while the character runs (`FUN_00140128`). `left` is the left
/// stick direction (x right, y down like the pad), `mag` its magnitude.
pub fn auto_turn_step(p: &Params, left: Vec2, mag: f32, run_speed: f32) -> f32 {
    let rate = p.auto_base + p.auto_per_speed * (run_speed - 8.0);
    (left.y.abs() + 1.0) * (2.0 * (left.x.abs() + 1.0)) * mag * rate
}

/// The player's turn this tick while the look-around runs (`FUN_001779e8`, the player's update,
/// gated by `FUN_00140f98` (camera mode 1) and the player's free-move state `FUN_001b7e00`): the
/// right stick's left bit adds `turn_left` to the game's facing, else its right bit adds
/// `turn_right`. Returned in the app's yaw (the game's facing `r` is the app's `PI - r`), so the
/// sign flips: a push to the left turns the facing, and with it the view, to the left.
pub fn look_turn(p: &Params, s: &Sticks) -> f32 {
    if s.right_mag <= 0.0 {
        0.0
    } else if s.right.x < 0.0 {
        -p.look.turn_left
    } else if s.right.x > 0.0 {
        -p.look.turn_right
    } else {
        0.0
    }
}

/// Spring time constant per tick; it grows with the character's run speed (`FUN_001407b8`).
pub fn smoothing_time(run_speed: f32, warp: bool) -> f32 {
    if warp {
        0.07
    } else {
        (run_speed - 8.0) * 0.0006 + 0.07
    }
}

/// Screen-edge tests of the lock-on camera (`FUN_00159908`): bit 1 above the safe area, 2 below,
/// 4 left, 8 right, 0x10 behind the camera. The safe area is x 32..480, y 8..408 of the game's
/// 512x416 picture.
pub fn screen_flags(p: Vec3, cam_pos: Vec3, cam_look: Vec3, hfov: f32) -> u32 {
    let fwd = (cam_look - cam_pos).normalize_or_zero();
    let right = fwd.cross(Vec3::Y).normalize_or_zero();
    let up = right.cross(fwd);
    let rel = p - cam_pos;
    let z = rel.dot(fwd);
    if z < 1.0 {
        return 0x10;
    }
    let t = (hfov * 0.5).tan();
    // Vertical field of view is 3/4 of the horizontal tangent (a 4:3 picture, `FUN_00106c48`).
    let sx = 256.0 + 256.0 / t * rel.dot(right) / z;
    let sy = 208.0 - 208.0 / (0.75 * t) * rel.dot(up) / z;
    let mut f = 0;
    if sx < 32.0 {
        f |= 4;
    }
    if sx > 480.0 {
        f |= 8;
    }
    if sy < 8.0 {
        f |= 1;
    }
    if sy > 408.0 {
        f |= 2;
    }
    f
}

/// A camera ray's first surface: the point, the polygon's unit normal and its attribute word
/// (the COCT flags word; the game reads it through `FUN_00143e40` and keeps the low four bits).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub point: Vec3,
    pub normal: Vec3,
    pub attr: u32,
}

/// First surface on a segment, or `None`.
pub type Cast<'a> = &'a dyn Fn(Vec3, Vec3) -> Option<Hit>;

/// A dynamic blocker: a sphere an object registers for the camera (`FUN_001894d0` fills the
/// list entry: centre at +0x10 from a joint of the object, radius at +4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blocker {
    pub center: Vec3,
    pub radius: f32,
}

/// Where a segment `a..b` enters a sphere, if it does (`FUN_00162ce0`): the closest point of the
/// line to the centre is found from the segment's midpoint and half extent, the ends are checked
/// when the closest point lies beyond them, and the entry point is the closest point moved back
/// toward `a` by the chord half-length.
pub fn sphere_entry(center: Vec3, radius: f32, a: Vec3, b: Vec3) -> Option<Vec3> {
    let m = (a + b) * 0.5;
    let h = b - m;
    let c = center - m;
    let hh = h.dot(h);
    if hh < f32::MIN_POSITIVE {
        return None;
    }
    let cc = c.dot(c);
    let hc = h.dot(c);
    let r2 = radius * radius;
    let d2 = cc - hc * hc / hh;
    if r2 < d2 {
        return None;
    }
    let q = m + h * (hc / hh);
    if hc > hh {
        if (center - b).length_squared() > r2 {
            return None;
        }
    } else if hc < -hh && (center - a).length_squared() > r2 {
        return None;
    }
    Some(q - h * ((r2 - d2) / hh).sqrt())
}

/// Clips a segment `a..b` against the blocker list (`FUN_0015a418`): the end moves to the nearest
/// sphere entry point. A sphere that already contains `a` (`FUN_00162c88`) is skipped; the
/// original then fails to advance its list pointer and re-tests that same sphere for the rest of
/// the loop, so every later entry is ignored. That quirk is kept.
pub fn blockers_clip(list: &[Blocker], a: Vec3, b: Vec3) -> Option<Vec3> {
    let mut end = b;
    let mut best = (b - a).length();
    let mut hit = false;
    let mut i = 0;
    for _ in 0..list.len().min(MAX_BLOCKERS) {
        let s = &list[i];
        if (a - s.center).length() <= s.radius {
            // The list pointer stays put in the original.
            continue;
        }
        if let Some(p) = sphere_entry(s.center, s.radius, a, b) {
            let d = (p - a).length();
            if d < best {
                best = d;
                end = p;
                hit = true;
            }
        }
        i += 1;
    }
    hit.then_some(end)
}

/// Pulls a camera point back from what is between `from` and `to` (`FUN_0013f8f8`). The
/// blocker list is tested first when given; then the map's camera collision. On a hit the result
/// is the hit point pushed `margin` along the polygon's normal. A polygon whose attribute's low
/// four bits are 5 makes a second cast from `from` to the point at `to`'s horizontal position
/// and the hit's height: the point becomes that cast's hit, or that end point itself when it
/// finds nothing (the first polygon's normal is kept), and the margin is always the small one. `far` asks for the larger margin (no field mode does).
pub fn collide(p: &Params, from: Vec3, to: Vec3, cast: Cast, blockers: Option<&[Blocker]>, far: bool) -> Option<Vec3> {
    let mut end = to;
    let mut blocked = false;
    if let Some(list) = blockers
        && let Some(e) = blockers_clip(list, from, to)
    {
        end = e;
        blocked = true;
    }
    let Some(h) = cast(from, end) else { return blocked.then_some(end) };
    let mut point = h.point;
    let mut second = false;
    if h.attr & 0xf == 5 {
        let end2 = Vec3::new(to.x, point.y, to.z);
        // The cast overwrites its end point only on a hit, so a miss leaves `end2` (read from
        // `FUN_0013f8f8`: `afStack_110` is the cast's in/out end point).
        point = cast(from, end2).map_or(end2, |h2| h2.point);
        second = true;
    }
    let margin = if second || !far { p.wall_margin } else { p.wall_margin_far };
    Some(point + h.normal * margin)
}

/// What the camera follows.
#[derive(Clone, Copy, Debug)]
pub struct Subject {
    /// Feet position.
    pub pos: Vec3,
    /// Facing direction (xz used).
    pub facing: Vec3,
    /// Moved horizontally since the last tick (`FUN_001831f0`).
    pub moved: bool,
    /// Current run speed in units per tick (`player + 0x114`; 8 for Sora without Quick Run).
    pub run_speed: f32,
    /// Height of the eye above the feet for the look-around (`prty` row field +0x30,
    /// `SubjectiveOffset`, through `FUN_0016be40`).
    pub eye_height: f32,
    /// The player's state allows the look-around (`FUN_0019ae30`: state flag 0x80, not in an
    /// event or on a ride).
    pub can_look: bool,
}

/// Pad sticks as the game reads them: directions are unit vectors (x right, y down), magnitudes
/// 0..1.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sticks {
    pub left: Vec2,
    pub left_mag: f32,
    pub right: Vec2,
    pub right_mag: f32,
}

/// Everything one tick needs.
pub struct TickIn<'a> {
    pub sticks: Sticks,
    /// Camera reset button (R3) pressed this tick.
    pub reset: bool,
    /// Select pressed this tick (the look-around, and the overhead camera's second view).
    pub select: bool,
    pub subject: Subject,
    /// Lock-on target point, when locked on.
    pub lock: Option<Vec3>,
    /// Zoom added directly (mouse wheel and drag), before the range clamp.
    pub zoom: f32,
    /// Auto-camera allowed (the game turns it off while R2 is held).
    pub auto: bool,
    /// The character jumped to a new place: skip smoothing.
    pub warp: bool,
    pub cast: Cast<'a>,
    /// Dynamic blockers registered this tick (`FUN_0015a3e8` rebuilds the list every call).
    pub blockers: &'a [Blocker],
}

/// The camera object's mode (`0x40`). The numbers are the game's. Mode 2 (lock-on,
/// `FUN_0015a0f0`) is not a variant: the rig takes it from `TickIn::lock`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Follow camera (`FUN_001402c8`).
    Follow = 0,
    /// Look-around from the player's eye (`FUN_00140678`).
    Look = 1,
    /// Talk camera (`FUN_00159648`; started by `FUN_00159688`, ended by `FUN_00159768`).
    Talk = 4,
    /// Overhead camera of area camera type 1 (`FUN_00158eb0`).
    Overhead = 5,
    /// Always-behind camera of area camera type 3 (`FUN_001581a0`).
    Behind = 8,
}

/// Area camera type from the area script's camera opcode (`DAT_0035cd38`), mapped to a table
/// row and a mode (`FUN_00177128`): 0 picks row 1 for an indoor area (arif bit 1) else row 0;
/// 1 is row 0 with the overhead camera; 2 is row 2; 3 is row 3 with the always-behind camera.
pub fn area_setup(camera_type: u8, indoor: bool) -> (usize, Mode) {
    match camera_type {
        1 => (0, Mode::Overhead),
        2 => (2, Mode::Follow),
        3 => (3, Mode::Behind),
        _ => (usize::from(indoor), Mode::Follow),
    }
}

/// Where the talk camera is placed relative to its look-at point (`FUN_001592d8`'s two branches,
/// chosen by the second argument of `FUN_00159688`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TalkFrame {
    /// Argument 1: on the line from the look-at point through the player, `dist` beyond the
    /// look-at point and `height` above it (yaw `atan2` of the player minus the look-at point,
    /// `FUN_003003e8`, rotating `(0, 0, dist)` by `FUN_0011b4d8`).
    AroundPlayer,
    /// Argument 0: in the talked-to object's own frame (its matrix at +0x6b0): the look-at point
    /// taken into the object's frame, moved by `-dist` along its local z and `height` up, and
    /// back to the world. `forward` is the object's local +z in the app's space [inferred: the
    /// matrix is the object's facing rotation, as `reaction_parts::shape_of` reads it].
    Object { forward: Vec3 },
}

/// The talk camera's input (`FUN_0018f108` of the target record copied to `0x358b18`): the
/// look-at point (the centre of the target's collision record of type 0x13 with the talk part's
/// index, `FUN_001894d0`), the distance and height (that record's two radii, +0x10 and +0x12).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TalkShot {
    pub look: Vec3,
    pub dist: f32,
    pub height: f32,
    pub frame: TalkFrame,
}

/// `FUN_001592d8`: the talk camera's wanted position for a player standing at `player` (feet,
/// `FUN_0016b960` of the followed object).
pub fn talk_pose(shot: &TalkShot, player: Vec3) -> Vec3 {
    let up = Vec3::Y * shot.height;
    match shot.frame {
        TalkFrame::AroundPlayer => {
            // Game (x, z) = app (x, -z); atan2(x, z) and the rotation of (0, 0, d) give the
            // horizontal unit vector toward the player; atan2(0, 0) = 0 is game +z = app -z.
            let d = (player - shot.look).with_y(0.0);
            let dir = if d.length_squared() > 0.0 { d.normalize() } else { Vec3::NEG_Z };
            shot.look + dir * shot.dist + up
        }
        TalkFrame::Object { forward } => shot.look - forward.with_y(0.0).normalize_or(Vec3::Z) * shot.dist + up,
    }
}

/// A value stepping linearly toward a target (`FUN_001abf40` sets it, `FUN_001abf78` advances it
/// and clamps to 0..1). The game keeps one on the player (+0x834) and multiplies it into the
/// model's draw factor at +0x82c (`FUN_00193270`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    pub value: f32,
    pub step: f32,
}

impl Fade {
    /// `FUN_001abf40(from, to, ticks)`: start at `from` and reach `to` after `ticks`; with no
    /// ticks the value is `to` at once.
    pub fn set(from: f32, to: f32, ticks: f32) -> Fade {
        if ticks <= 0.0 { Fade { value: to, step: 0.0 } } else { Fade { value: from, step: (to - from) / ticks } }
    }

    /// One tick (`FUN_001abf78`): the step stops at 0 or 1.
    pub fn advance(&mut self) {
        if self.step == 0.0 {
            return;
        }
        let v = self.value + self.step;
        if v <= 0.0 {
            *self = Fade { value: 0.0, step: 0.0 };
        } else if v >= 1.0 {
            *self = Fade { value: 1.0, step: 0.0 };
        } else {
            self.value = v;
        }
    }
}

/// What an event (cutscene) camera hands the field camera: a finished view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub look: Vec3,
    pub pos: Vec3,
    pub fov: f32,
    pub roll: f32,
}

/// A scripted camera's smoothing state (`0x520660..0x5206ec`): the springs the event player runs
/// on the position, the look-at point and (fov, roll) it is given each frame (`FUN_0015cb70`,
/// `FUN_0015cd20`). `start` seeds every value from the field camera (`FUN_0015ca08`), `frame`
/// is one event frame (`FUN_0015ce00`), and the field camera takes the final view back with
/// [`Rig::handover`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventCamera {
    look: Vec3,
    look_new: Vec3,
    look_prev: Vec3,
    pos: Vec3,
    pos_new: Vec3,
    pos_prev: Vec3,
    fr: Vec3,
    fr_new: Vec3,
    fr_prev: Vec3,
}

// No map script drives an event camera yet (hook for the script runner); tested below.
#[allow(dead_code)]
impl EventCamera {
    /// `FUN_0015ca08`: everything starts at the field camera's smoothed values.
    pub fn start(rig: &Rig) -> EventCamera {
        let fr = Vec3::new(rig.fov, rig.roll, 0.0);
        EventCamera { look: rig.look, look_new: rig.look, look_prev: rig.look, pos: rig.pos, pos_new: rig.pos, pos_prev: rig.pos, fr, fr_new: fr, fr_prev: fr }
    }

    /// One frame of the event's camera track (`FUN_0015ce00`): the wanted values of last frame
    /// become the previous ones (`FUN_0015cb08`); with `collide` the position is pulled in
    /// against the map (and the blockers when `blockers` is given); with `smooth` the three
    /// springs run, else the values are used as they are. Returns the view to draw.
    #[allow(clippy::too_many_arguments)] // the event track entry's own fields
    pub fn frame(&mut self, p: &Params, pos: Vec3, look: Vec3, fov: f32, roll: f32, collide_map: Option<(Cast, Option<&[Blocker]>)>, smooth: bool) -> View {
        self.look_prev = self.look_new;
        self.pos_prev = self.pos_new;
        self.fr_prev = self.fr_new;
        let mut pos = pos;
        if let Some((cast, blockers)) = collide_map
            && let Some(c) = collide(p, look, pos, cast, blockers, false)
        {
            pos = c;
        }
        self.look_new = look;
        self.pos_new = pos;
        self.fr_new = Vec3::new(fov, roll, 0.0);
        if smooth {
            let e = &p.event;
            self.look = spring(self.look, self.look_new, self.look_prev, e.t_look, e.k, e.c, e.rest);
            self.pos = spring(self.pos, self.pos_new, self.pos_prev, e.t_pos, e.k, e.c, e.rest);
            self.fr = spring(self.fr, self.fr_new, self.fr_prev, e.t_fov, e.k_fov, e.c_fov, e.rest_fov);
        } else {
            self.look = look;
            self.pos = pos;
            self.fr = self.fr_new;
        }
        View { look: self.look, pos: self.pos, fov: self.fr.x, roll: self.fr.y }
    }

    pub fn view(&self) -> View {
        View { look: self.look, pos: self.pos, fov: self.fr.x, roll: self.fr.y }
    }
}

/// The camera object's state (`0x355840`).
#[derive(Clone, Debug)]
pub struct Rig {
    pub row: Row,
    pub mode: Mode,
    pub radius: f32,
    pub yaw: f32,
    pub target_yaw: f32,
    /// Smoothed look-at point and position, what is drawn.
    pub look: Vec3,
    pub pos: Vec3,
    pub fov: f32,
    pub roll: f32,
    /// Look-around yaw and pitch (`0xc0`, `0xc4`), mode 1 only.
    pub look_yaw: f32,
    pub look_pitch: f32,
    /// The player's draw fade the look-around drives (`player + 0x834`). The app's model does
    /// not read it yet; the value is exposed for that hook.
    pub fade: Fade,
    /// Overhead camera: Select switched to the fixed view (`DAT_00358b08`).
    pub overhead_fixed: bool,
    /// The talk camera's input (`0x358b18..0x358b28`), while mode 4 runs.
    pub talk: Option<TalkShot>,
    /// `DAT_00358b10`: the talk view is placed on the next mode-4 tick.
    talk_new: bool,
    /// The warp flag (`0x38`, `FUN_00140e28`) set for the next tick.
    warp_next: bool,
    /// A dynamic blocker clipped one of this tick's collided positions (output, for the log).
    pub blocked: bool,
    wanted_look: Vec3,
    prev_wanted_look: Vec3,
    wanted_pos: Vec3,
    prev_wanted_pos: Vec3,
    wanted_fov: f32,
    prev_wanted_fov: f32,
}

impl Rig {
    /// A camera placed straight behind the subject with no smoothing (`FUN_0013f470`).
    pub fn new(p: &Params, row: Row, s: &Subject, cast: Cast) -> Rig {
        let yaw = behind_yaw(s.facing);
        let look = s.pos + Vec3::Y * LOOK_HEIGHT;
        let mut pos = wanted_position(look, row.radius, yaw, row.min_radius, row.height_k);
        if let Some(c) = collide(p, look, pos, cast, None, false) {
            pos = c;
        }
        Rig {
            row,
            mode: Mode::Follow,
            radius: row.radius,
            yaw,
            target_yaw: yaw,
            look,
            pos,
            fov: row.fov,
            roll: 0.0,
            look_yaw: yaw + PI,
            look_pitch: 0.0,
            fade: Fade { value: 1.0, step: 0.0 },
            overhead_fixed: false,
            talk: None,
            talk_new: false,
            warp_next: false,
            blocked: false,
            wanted_look: look,
            prev_wanted_look: look,
            wanted_pos: pos,
            prev_wanted_pos: pos,
            wanted_fov: row.fov,
            prev_wanted_fov: row.fov,
        }
    }

    /// Sets the yaw directly (the harness and the mouse).
    pub fn set_yaw(&mut self, yaw: f32) {
        self.yaw = wrap(yaw);
        self.target_yaw = self.yaw;
    }

    /// Elevation of the camera above the look-at point, radians.
    pub fn pitch(&self) -> f32 {
        let d = self.pos - self.look;
        d.y.atan2(d.xz().length().max(1e-3))
    }

    /// The field camera takes over from an event camera (`FUN_0015c8f8` then `FUN_001406e8`):
    /// it is placed behind the subject, runs one forced tick (no collision, warp smoothing), and
    /// its smoothed values are then replaced by the event's final view, so the next ticks spring
    /// from where the cutscene left the camera.
    pub fn handover(&mut self, p: &Params, s: &Subject, v: View) {
        let fresh = Rig::new(p, self.row, s, &|_, _| None);
        let (mode, fixed) = (self.mode, self.overhead_fixed);
        *self = fresh;
        self.mode = if mode == Mode::Look { Mode::Follow } else { mode };
        self.overhead_fixed = fixed;
        self.look = v.look;
        self.pos = v.pos;
        self.fov = v.fov;
        self.roll = v.roll;
    }

    /// `FUN_00159688`: the talk camera starts. The look-around ends (`FUN_00140f28` with
    /// `DAT_003793d8` ticks of fade-in), the mode is 4, the shot is kept and the view is placed on
    /// the next tick (`DAT_00358b10`). The follow camera's yaw and radius are left as they are.
    pub fn start_talk(&mut self, p: &Params, shot: TalkShot) {
        if self.mode == Mode::Look {
            self.fade = Fade::set(0.0, p.look.fade_to, p.talk.look_fade_ticks);
        }
        self.mode = Mode::Talk;
        self.talk = Some(shot);
        self.talk_new = true;
    }

    /// `FUN_00159768` -> the task `FUN_00159728`: the warp flag is set (`FUN_00140e28`) and the
    /// mode goes back to 0, or 2 when a lock-on is held (`FUN_00140ce8`). The rig has no mode 2
    /// state (it takes the lock-on from `TickIn::lock` every tick), so the mode is set to
    /// `Follow` here and the next tick picks the lock-on view when the input holds one. The
    /// next tick snaps to the follow camera's wanted view.
    pub fn end_talk(&mut self) {
        if self.mode == Mode::Talk {
            self.mode = Mode::Follow;
        }
        self.talk = None;
        self.talk_new = false;
        self.warp_next = true;
    }

    /// Mode 4 (`FUN_00159648`): on the first tick the view is placed (`FUN_001592d8`: wanted
    /// look-at, position, fov `DAT_00520500` and roll 0, then `FUN_0013f560` copies them to the
    /// last-tick and drawn values: a cut); every tick the wanted position is collided with the
    /// blockers and the map (`FUN_0013fbc0`).
    fn talk_tick(&mut self, p: &Params, i: &TickIn) {
        let Some(shot) = self.talk else { return };
        if std::mem::take(&mut self.talk_new) {
            self.wanted_look = shot.look;
            self.wanted_pos = talk_pose(&shot, i.subject.pos);
            self.wanted_fov = p.talk.fov;
            self.roll = 0.0;
            self.prev_wanted_look = self.wanted_look;
            self.prev_wanted_pos = self.wanted_pos;
            self.prev_wanted_fov = self.wanted_fov;
            self.look = self.wanted_look;
            self.pos = self.wanted_pos;
            self.fov = self.wanted_fov;
        }
        self.blocked |= blockers_clip(i.blockers, self.wanted_look, self.wanted_pos).is_some();
        if let Some(c) = collide(p, self.wanted_look, self.wanted_pos, i.cast, Some(i.blockers), false) {
            self.wanted_pos = c;
        }
    }

    fn clamp_radius(&mut self) {
        self.radius = self.radius.clamp(self.row.min_radius, self.row.max_radius);
    }

    /// Right stick horizontal turns the camera, at `stick_yaw` per tick times the stick's
    /// magnitude (`FUN_0013fe20`). Stick left moves the camera to its left.
    fn stick_turn(&mut self, p: &Params, s: &Sticks) {
        if s.right_mag > 0.0 && s.right.x != 0.0 {
            let dir = s.right.x.signum();
            self.yaw = wrap(self.yaw + dir * p.stick_yaw * s.right_mag);
            self.target_yaw = self.yaw;
        }
    }

    /// Right stick vertical zooms, up in and down out, `ZOOM_RATE` per tick at full tilt, inside
    /// the row's range (`FUN_0013fd08`).
    fn stick_zoom(&mut self, s: &Sticks) {
        if s.right_mag > 0.0 {
            if s.right.y < 0.0 {
                self.radius = (self.radius - s.right_mag * ZOOM_RATE).max(self.row.min_radius);
            } else if s.right.y > 0.0 {
                self.radius = (self.radius + s.right_mag * ZOOM_RATE).min(self.row.max_radius);
            }
        }
    }

    /// Zoom from the mouse wheel and drag, which the pad does not have.
    fn direct_zoom(&mut self, z: f32) {
        if z != 0.0 {
            self.radius += z;
            self.clamp_radius();
        }
    }

    /// Radius drifts back to its default while the auto-camera turns (`FUN_00140068`).
    fn relax_radius(&mut self, to: f32) {
        if (self.radius - to).abs() < 1.0 {
            return;
        }
        if self.radius <= to {
            self.radius = (self.radius + 0.5).min(self.row.max_radius);
        } else {
            self.radius = (self.radius - 0.5).max(self.row.min_radius);
        }
    }

    /// Select in the follow camera while the player may look around (`FUN_0013fe20`): mode 1,
    /// pitch 0, and the player starts fading out.
    fn enter_look(&mut self, p: &Params) {
        self.mode = Mode::Look;
        self.look_yaw = self.yaw + PI;
        self.look_pitch = 0.0;
        self.fade = Fade::set(p.look.fade_from, 0.0, p.look.fade_out_ticks);
    }

    /// Leaving the look-around (`FUN_00140f28`): the player fades back in and the mode is 0.
    fn leave_look(&mut self, p: &Params) {
        self.fade = Fade::set(0.0, p.look.fade_to, p.look.fade_in_ticks);
        self.mode = Mode::Follow;
    }

    /// Look-around tick (`FUN_00140530`). The camera sits at the eye; its yaw is the player's
    /// facing (the game sets `0xc0` from `FUN_0013fc28` plus pi every tick, so turning is the
    /// player's job); the right stick's vertical tilts within the pitch range; the look-at point
    /// is `distance` ahead (`FUN_00140418`).
    fn look_tick(&mut self, p: &Params, i: &TickIn) {
        let s = &i.subject;
        if i.reset {
            self.look_pitch = 0.0;
        }
        let eye = s.pos + Vec3::Y * s.eye_height;
        self.wanted_pos = eye;
        self.yaw = behind_yaw(s.facing);
        self.target_yaw = self.yaw;
        self.look_yaw = self.yaw + PI;
        if i.sticks.right_mag > 0.0 && i.sticks.right.y != 0.0 {
            // Stick up (y < 0) raises the pitch.
            self.look_pitch -= i.sticks.right.y * i.sticks.right_mag * p.stick_yaw;
        }
        self.look_yaw = wrap(self.look_yaw);
        self.look_pitch = wrap(self.look_pitch).clamp(p.look.pitch_min, p.look.pitch_max);
        let (sy, cy) = self.look_yaw.sin_cos();
        let (sp, cp) = self.look_pitch.sin_cos();
        self.wanted_look = eye + Vec3::new(sy * cp, sp, cy * cp) * p.look.distance;
    }

    /// Overhead camera tick (`FUN_00158eb0`): the look-at point eases toward a point above the
    /// floor under the player, the camera sits at a fixed world offset from it, and Select swaps
    /// to a fixed view. Nothing is smoothed (`FUN_0013f560` copies the wanted values).
    fn overhead_tick(&mut self, p: &Params, i: &TickIn) {
        let o = &p.overhead;
        if i.select {
            self.overhead_fixed = !self.overhead_fixed;
        }
        let (look, pos, fov) = if self.overhead_fixed {
            (o.fixed_look, o.fixed_pos, o.fixed_fov)
        } else {
            let s = &i.subject;
            // A vertical probe from above the player to far below (`FUN_00158d88`); the player's
            // own position when it finds nothing.
            let above = s.pos + Vec3::Y * o.probe_up;
            let below = s.pos - Vec3::Y * o.probe_down;
            let floor = (i.cast)(above, below).map_or(s.pos, |h| h.point);
            let target = floor + Vec3::Y * o.raise;
            let d = target - self.look;
            let len = d.length();
            let f = len.clamp(0.0, o.ease);
            let look = self.look + d * (f / o.ease);
            (look, look + o.offset, o.fov)
        };
        self.wanted_look = look;
        self.wanted_pos = pos;
        self.wanted_fov = fov;
        self.prev_wanted_look = look;
        self.prev_wanted_pos = pos;
        self.prev_wanted_fov = fov;
        self.look = look;
        self.pos = pos;
        self.fov = fov;
        self.roll = 0.0;
    }

    /// One 60 Hz tick (`FUN_00140a80`).
    pub fn tick(&mut self, p: &Params, i: &TickIn) {
        self.blocked = false;
        // The warp flag a talk's end set (`FUN_00140e48` after the mode update snaps the drawn
        // values to the wanted ones).
        let warped;
        let i = if std::mem::take(&mut self.warp_next) {
            warped = TickIn { warp: true, ..*i };
            &warped
        } else {
            i
        };
        self.prev_wanted_look = self.wanted_look;
        self.prev_wanted_pos = self.wanted_pos;
        self.prev_wanted_fov = self.wanted_fov;
        self.fade.advance();
        let sub = &i.subject;
        // Where the camera looked last tick, for the lock-on screen tests.
        let (view_pos, view_look, view_fov) = (self.pos, self.look, self.fov);
        let chest = sub.pos + Vec3::Y * LOOK_HEIGHT;
        let mut snap_yaw = false;
        if self.mode == Mode::Overhead {
            self.overhead_tick(p, i);
            return;
        }
        if self.mode == Mode::Talk {
            self.talk_tick(p, i);
            self.smooth_and_collide(p, i, chest, false);
            return;
        }
        // Lock-on starts by ending the look-around (`FUN_0015a180` calls `FUN_00140f28`).
        // Select or a state that forbids it ends it too (`FUN_00140530`'s other branch: fade the
        // player in, which sets the mode to 0 (`DAT_00355880` is the mode field, `0x355840 +
        // 0x40`), turn the yaw behind the player, and frame this tick like the follow camera).
        let mut select = i.select;
        if self.mode == Mode::Look && (i.lock.is_some() || !sub.can_look || i.select) {
            self.leave_look(p);
            if i.lock.is_none() {
                self.yaw = behind_yaw(sub.facing);
                self.target_yaw = self.yaw;
            }
            select = false;
        }
        if self.mode == Mode::Look {
            self.look_tick(p, i);
            self.wanted_fov = self.row.fov;
        } else {
            match i.lock {
                None => {
                    if self.mode == Mode::Behind {
                        // `FUN_00158130`: R3 resets the radius, else the stick zooms; the yaw is
                        // always behind the player.
                        if i.reset {
                            self.radius = self.row.radius;
                        } else {
                            self.stick_zoom(&i.sticks);
                        }
                        self.direct_zoom(i.zoom);
                        self.target_yaw = behind_yaw(sub.facing);
                    } else {
                        // Follow camera (`FUN_001402c8`).
                        if select && sub.can_look {
                            self.enter_look(p);
                            self.look_tick(p, i);
                            self.wanted_fov = self.row.fov;
                            self.smooth_and_collide(p, i, chest, false);
                            return;
                        }
                        if i.reset {
                            self.target_yaw = behind_yaw(sub.facing);
                            self.radius = self.row.radius;
                            self.pos = self.wanted_pos;
                            snap_yaw = i.sticks.left_mag != 0.0;
                        } else {
                            self.stick_turn(p, &i.sticks);
                        }
                        // Zoom runs whether or not the camera was reset.
                        self.stick_zoom(&i.sticks);
                        self.direct_zoom(i.zoom);
                        if !snap_yaw && i.auto && sub.moved && i.sticks.left_mag != 0.0 && i.sticks.right_mag == 0.0 && i.sticks.left.y < 0.9 {
                            let step = auto_turn_step(p, i.sticks.left, i.sticks.left_mag, sub.run_speed);
                            self.yaw = approach_angle(self.yaw, behind_yaw(sub.facing), step);
                            self.target_yaw = self.yaw;
                            self.relax_radius(self.row.radius);
                        }
                    }
                    self.wanted_look = chest;
                }
                Some(target) => {
                    // Lock-on camera (`FUN_0015a0f0`).
                    if i.reset {
                        // Reset button: face along the player-to-look-at line, default lock radius.
                        let d = sub.pos - self.wanted_look;
                        self.target_yaw = d.x.atan2(d.z);
                        self.radius = self.row.lock_radius;
                    } else {
                        self.stick_turn(p, &i.sticks);
                    }
                    self.stick_zoom(&i.sticks);
                    self.direct_zoom(i.zoom);
                    self.lock_frame(i, target, chest, (view_pos, view_look, view_fov));
                }
            }
            self.wanted_fov = self.row.fov;
            // Wanted position (`FUN_0013f7a0`): the yaw swings toward its target, then the circle.
            self.yaw = if snap_yaw || i.warp { self.target_yaw } else { approach_angle(self.yaw, self.target_yaw, p.yaw_step) };
            let mut wp = wanted_position(chest, self.radius, self.yaw, self.row.min_radius, self.row.height_k);
            // The wanted position is tested with the blockers (`FUN_0013fbc0` passes 1).
            self.blocked |= blockers_clip(i.blockers, chest, wp).is_some();
            if let Some(c) = collide(p, chest, wp, i.cast, Some(i.blockers), false) {
                wp = c;
            }
            self.wanted_pos = wp;
        }
        self.smooth_and_collide(p, i, chest, i.lock.is_some());
    }

    /// The common tail (`FUN_001407b8`): smoothing, then the mode's collision of the smoothed
    /// position (modes 0, 3, 6, 8: pull the position in, which also becomes the wanted one;
    /// mode 1: pull the look-at point in; mode 2: from the chest).
    fn smooth_and_collide(&mut self, p: &Params, i: &TickIn, chest: Vec3, lock: bool) {
        if i.warp {
            self.look = self.wanted_look;
            self.pos = self.wanted_pos;
            self.fov = self.wanted_fov;
            self.prev_wanted_look = self.wanted_look;
            self.prev_wanted_pos = self.wanted_pos;
        }
        let t = smoothing_time(i.subject.run_speed, i.warp);
        self.look = spring(self.look, self.wanted_look, self.prev_wanted_look, t + 0.03, 2.5, 0.5, 1.0);
        self.pos = spring(self.pos, self.wanted_pos, self.prev_wanted_pos, t, 2.5, 0.5, 1.0);
        self.fov = spring(Vec3::new(self.fov, 0.0, 0.0), Vec3::new(self.wanted_fov, 0.0, 0.0), Vec3::new(self.prev_wanted_fov, 0.0, 0.0), t, 2.5, 0.5, 0.001).x;
        if self.mode == Mode::Look {
            // `FUN_00140398`: a wall between the eye and the look-at point pulls the look-at point
            // to it. The cast's direction (eye to look-at) is inferred: the decompiled call's end
            // point is not recoverable and the game's polygon test is front-face only.
            if let Some(h) = (i.cast)(self.pos, self.look) {
                self.look = h.point;
            }
            return;
        }
        // Mode 4 is not in `FUN_001407b8`'s collision cases.
        if self.mode == Mode::Talk {
            return;
        }
        let from = if lock { chest } else { self.look };
        self.blocked |= blockers_clip(i.blockers, from, self.pos).is_some();
        if let Some(c) = collide(p, from, self.pos, i.cast, Some(i.blockers), false) {
            self.pos = c;
            self.wanted_pos = c;
        }
    }

    /// Lock-on framing (`FUN_00159da8`): look-at point between the character and the target,
    /// swing behind the character when either leaves the safe area.
    fn lock_frame(&mut self, i: &TickIn, target: Vec3, chest: Vec3, view: (Vec3, Vec3, f32)) {
        let sub = &i.subject;
        let look = chest + (target - chest) * 0.3;
        let prev_look = self.wanted_look;
        let body = sub.pos + Vec3::Y * 100.0;
        let ft = screen_flags(target, view.0, view.1, view.2);
        let fb = screen_flags(body, view.0, view.1, view.2);
        // A point below the bottom edge pulls the camera in by 2 per tick.
        for f in [ft, fb] {
            if f & 1 == 0 && f & 2 != 0 {
                self.radius -= 2.0;
            }
        }
        self.clamp_radius();
        let d = body - look;
        let len = d.length();
        // The game's Y points down: its `y <= 0.5` test is `-y >= -0.5` here.
        let steep_below = len > 0.0 && d.y / len < -0.5;
        if len <= 300.0 || !steep_below {
            if !(ft & 1 == 0 && ft & 2 != 0) && !(fb & 1 == 0 && fb & 2 != 0) {
                // Relax the radius toward the lock radius, one unit per tick (`FUN_0013ffc8`).
                if (self.radius - self.row.lock_radius).abs() >= 1.0 {
                    if self.radius <= self.row.lock_radius {
                        self.radius = (self.radius + 1.0).min(self.row.max_radius);
                    } else {
                        self.radius = (self.radius - 1.0).max(self.row.min_radius);
                    }
                }
            }
            let flags = if ft != 0 { ft } else { fb };
            if flags != 0 {
                let off = if flags & 4 != 0 {
                    0.349_066
                } else if flags & 8 != 0 {
                    -0.349_066
                } else {
                    0.0
                };
                let to = sub.pos - prev_look;
                // Camera goes along the line from the old look-at point through the character.
                self.target_yaw = wrap(to.x.atan2(to.z) - off);
            }
            self.wanted_look = look;
        } else {
            self.wanted_look = chest;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> Row {
        Row { fov: 1.2, radius: 400.0, lock_radius: 350.0, min_radius: 200.0, max_radius: 500.0, height_k: 0.01 }
    }

    fn params() -> Params {
        Params {
            rows: [row(); 4],
            yaw_step: 0.17,
            auto_base: 0.004,
            auto_per_speed: 0.0002,
            stick_yaw: 0.05,
            wall_margin: 5.0,
            wall_margin_far: 30.0,
            look: LookParams {
                distance: 350.0,
                pitch_min: -1.22,
                pitch_max: 1.22,
                fade_from: 1.0,
                fade_out_ticks: 30.0,
                fade_to: 1.0,
                fade_in_ticks: 30.0,
                turn_left: -0.05,
                turn_right: 0.05,
            },
            overhead: OverheadParams {
                probe_up: 100.0,
                probe_down: 10000.0,
                raise: 150.0,
                ease: 200.0,
                offset: Vec3::new(600.0, 500.0, 0.0),
                fixed_pos: Vec3::new(51.0, 1492.0, 0.0),
                fixed_look: Vec3::new(256.0, 1777.0, 0.0),
                fixed_fov: 1.5,
                fov: 1.05,
            },
            event: EventParams { k: 2.5, c: 0.5, t_look: 0.1, rest: 1.0, t_pos: 0.07, t_fov: 0.07, k_fov: 2.5, c_fov: 0.5, rest_fov: 0.001 },
            talk: TalkParams { fov: 1.0, look_fade_ticks: 30.0 },
        }
    }

    fn no_wall(_: Vec3, _: Vec3) -> Option<Hit> {
        None
    }

    /// An infinite wall on the plane x = 0, hit from either side, attribute 1.
    fn wall(a: Vec3, b: Vec3) -> Option<Hit> {
        plane_x0(a, b, 1)
    }

    fn plane_x0(a: Vec3, b: Vec3, attr: u32) -> Option<Hit> {
        let d = b - a;
        if d.x.abs() < 1e-6 {
            return None;
        }
        let t = -a.x / d.x;
        (0.0..=1.0).contains(&t).then(|| Hit { point: a + d * t, normal: Vec3::new(a.x.signum(), 0.0, 0.0), attr })
    }

    fn subject(pos: Vec3) -> Subject {
        Subject { pos, facing: Vec3::NEG_Z, moved: false, run_speed: 8.0, eye_height: 150.0, can_look: true }
    }

    fn input<'a>(s: Subject, cast: Cast<'a>) -> TickIn<'a> {
        TickIn { sticks: Sticks::default(), reset: false, select: false, subject: s, lock: None, zoom: 0.0, auto: true, warp: false, cast, blockers: &[] }
    }

    #[test]
    fn wrap_and_approach() {
        assert!((wrap(3.0 * PI) - -PI).abs() < 1e-4 || (wrap(3.0 * PI) - PI).abs() < 1e-4);
        assert!((wrap(0.5) - 0.5).abs() < 1e-6);
        // Short way round across the +-pi seam.
        let a = approach_angle(3.0, -3.0, 0.1);
        assert!(a > 3.0 || a < -3.1, "{a}");
        assert!((approach_angle(0.0, 0.05, 0.1) - 0.05).abs() < 1e-6);
        assert!((approach_angle(0.0, 1.0, 0.1) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn behind_is_opposite_the_facing() {
        for a in [0.0f32, 0.7, 2.0, -1.3, 3.0] {
            let f = Vec3::new(a.sin(), 0.0, a.cos());
            let y = behind_yaw(f);
            let off = Vec3::new(y.sin(), 0.0, y.cos());
            assert!((off + f).length() < 1e-4, "{a}");
        }
    }

    #[test]
    fn spring_converges_to_a_fixed_target() {
        let target = Vec3::new(100.0, 50.0, -30.0);
        let mut cur = Vec3::new(-500.0, 0.0, 200.0);
        for _ in 0..200 {
            cur = spring(cur, target, target, 0.07, 2.5, 0.5, 1.0);
        }
        // Settles at the rest length from the target (1 unit), not exactly on it.
        assert!(cur.distance(target) < 1.5, "{}", cur.distance(target));
    }

    #[test]
    fn spring_lags_a_moving_target_by_a_bounded_amount() {
        let mut target = Vec3::new(0.0, 0.0, 0.0);
        let mut cur = target;
        let mut lag = 0.0;
        for _ in 0..600 {
            let prev = target;
            target.z += 8.0;
            cur = spring(cur, target, prev, 0.07, 2.5, 0.5, 1.0);
            lag = cur.distance(target);
        }
        assert!(lag > 10.0 && lag < 80.0, "{lag}");
        assert!(cur.z < target.z, "camera trails behind the mover");
    }

    #[test]
    fn height_follows_the_radius_parabola() {
        let look = Vec3::new(0.0, 170.0, 0.0);
        let near = wanted_position(look, 200.0, 0.0, 200.0, 0.01);
        let far = wanted_position(look, 500.0, 0.0, 200.0, 0.01);
        assert!((near.y - (170.0 - 150.0)).abs() < 1e-4);
        assert!((far.y - (170.0 - 150.0 + 0.01 * 300.0 * 300.0)).abs() < 1e-3);
        assert!((near.z - 200.0).abs() < 1e-4, "yaw 0 puts the camera on +z");
    }

    #[test]
    fn screen_flags_centre_and_edges() {
        let cam = Vec3::new(0.0, 100.0, 400.0);
        let look = Vec3::new(0.0, 100.0, 0.0);
        assert_eq!(screen_flags(Vec3::new(0.0, 100.0, 0.0), cam, look, 1.2), 0);
        assert_ne!(screen_flags(Vec3::new(-900.0, 100.0, 0.0), cam, look, 1.2) & 4, 0, "off the left edge (x < 0 when looking down -z)");
        assert_ne!(screen_flags(Vec3::new(900.0, 100.0, 0.0), cam, look, 1.2) & 8, 0);
        assert_ne!(screen_flags(Vec3::new(0.0, 900.0, 0.0), cam, look, 1.2) & 1, 0);
        assert_ne!(screen_flags(Vec3::new(0.0, -900.0, 0.0), cam, look, 1.2) & 2, 0);
        assert_eq!(screen_flags(Vec3::new(0.0, 100.0, 900.0), cam, look, 1.2), 0x10);
    }

    #[test]
    fn collide_keeps_the_margin_along_the_normal() {
        let p = params();
        let look = Vec3::new(100.0, 100.0, 0.0);
        let want = Vec3::new(-300.0, 140.0, 80.0);
        let out = collide(&p, look, want, &wall, None, false).unwrap();
        assert!((out.x - 5.0).abs() < 1e-3, "{out}");
        // The hit point keeps the ray's y and z.
        let t = 100.0 / 400.0;
        assert!((out.y - (100.0 + 40.0 * t)).abs() < 1e-3 && (out.z - 80.0 * t).abs() < 1e-3, "{out}");
        assert!(collide(&p, Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0), &no_wall, None, false).is_none());
        // The far margin only without a second cast.
        let far = collide(&p, look, want, &wall, None, true).unwrap();
        assert!((far.x - 30.0).abs() < 1e-3, "{far}");
    }

    #[test]
    fn attribute_five_casts_again_at_the_hit_height() {
        let p = params();
        // A slope: hit at height 200 wherever the ray crosses x = 0, attribute 5; the second cast
        // ends at (to.x, 200, to.z) and hits the plane again (x = 0), so the point keeps the ray's
        // z at the crossing but its y is the hit height.
        let slope = |a: Vec3, b: Vec3| plane_x0(a, b, 5).map(|h| Hit { point: Vec3::new(h.point.x, 200.0, h.point.z), ..h });
        let look = Vec3::new(100.0, 100.0, 0.0);
        let want = Vec3::new(-300.0, 140.0, 80.0);
        let out = collide(&p, look, want, &slope, None, true).unwrap();
        assert!((out.y - 200.0).abs() < 1e-3, "{out}");
        // Second cast: margin stays the small one even when the far margin was asked for.
        assert!((out.x - 5.0).abs() < 1e-3, "{out}");
        // Attribute 5 polygon whose second cast misses: the first hit stands.
        let once = |a: Vec3, b: Vec3| if (b.y - 140.0).abs() > 1e-3 { None } else { plane_x0(a, b, 5) };
        let out = collide(&p, look, want, &once, None, false).unwrap();
        // The second cast finds nothing, so the point is its end (to.x, hit y = 110, to.z)
        // pushed 5 along the first polygon's normal.
        assert!((out - Vec3::new(-295.0, 110.0, 80.0)).length() < 1e-3, "{out}");
    }

    #[test]
    fn sphere_entry_points() {
        let c = Vec3::new(0.0, 0.0, 0.0);
        let e = sphere_entry(c, 10.0, Vec3::new(-50.0, 0.0, 0.0), Vec3::new(50.0, 0.0, 0.0)).unwrap();
        assert!((e - Vec3::new(-10.0, 0.0, 0.0)).length() < 1e-3, "{e}");
        assert!(sphere_entry(c, 10.0, Vec3::new(-50.0, 20.0, 0.0), Vec3::new(50.0, 20.0, 0.0)).is_none(), "misses above");
        assert!(sphere_entry(c, 10.0, Vec3::new(-50.0, 0.0, 0.0), Vec3::new(-20.0, 0.0, 0.0)).is_none(), "stops short");
        // The end inside the sphere still counts.
        let e = sphere_entry(c, 10.0, Vec3::new(-50.0, 0.0, 0.0), Vec3::new(5.0, 0.0, 0.0)).unwrap();
        assert!((e.x + 10.0).abs() < 1e-3, "{e}");
    }

    #[test]
    fn blockers_clip_nearest_and_the_stall_quirk() {
        let a = Vec3::ZERO;
        let b = Vec3::new(100.0, 0.0, 0.0);
        let near = Blocker { center: Vec3::new(40.0, 0.0, 0.0), radius: 5.0 };
        let far = Blocker { center: Vec3::new(80.0, 0.0, 0.0), radius: 5.0 };
        let e = blockers_clip(&[far, near], a, b).unwrap();
        assert!((e.x - 35.0).abs() < 1e-3, "{e}");
        // A sphere around the start stalls the walk: the one after it is never tested.
        let around = Blocker { center: a, radius: 3.0 };
        assert!(blockers_clip(&[around, near], a, b).is_none());
        assert!(blockers_clip(&[near, around], a, b).is_some());
    }

    #[test]
    fn rig_settles_behind_a_standing_subject() {
        let p = params();
        let s = subject(Vec3::new(1000.0, 0.0, 1000.0));
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        for _ in 0..120 {
            rig.tick(&p, &input(s, &no_wall));
        }
        let to_cam = rig.pos - (s.pos + Vec3::Y * LOOK_HEIGHT);
        assert!((to_cam.xz().length() - 400.0).abs() < 3.0, "{}", to_cam.xz().length());
        assert!(to_cam.z > 0.0, "facing -z, so the camera is on +z");
    }

    #[test]
    fn rig_follows_a_runner_and_swings_behind_when_he_turns() {
        let p = params();
        let mut s = subject(Vec3::ZERO);
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        // Run to the right (+x): the auto-camera turns the camera behind him.
        s.facing = Vec3::X;
        s.moved = true;
        let mut inp = input(s, &no_wall);
        inp.sticks.left = Vec2::new(1.0, 0.0);
        inp.sticks.left_mag = 1.0;
        for _ in 0..600 {
            s.pos.x += 8.0;
            inp.subject = s;
            rig.tick(&p, &inp);
        }
        let want = behind_yaw(Vec3::X);
        assert!(wrap(rig.yaw - want).abs() < 0.05, "yaw {} want {}", rig.yaw, want);
        assert!(rig.pos.x < s.pos.x, "camera trails behind the runner");
    }

    #[test]
    fn stick_turns_and_zooms_within_range() {
        let p = params();
        let s = subject(Vec3::ZERO);
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        let mut inp = input(s, &no_wall);
        inp.sticks.right = Vec2::new(1.0, 0.0);
        inp.sticks.right_mag = 1.0;
        let y0 = rig.yaw;
        rig.tick(&p, &inp);
        assert!((wrap(rig.yaw - y0) - p.stick_yaw).abs() < 1e-5, "stick right moves the camera to its right");
        inp.sticks.right = Vec2::new(0.0, -1.0);
        for _ in 0..100 {
            rig.tick(&p, &inp);
        }
        assert_eq!(rig.radius, row().min_radius);
        inp.sticks.right = Vec2::new(0.0, 1.0);
        for _ in 0..100 {
            rig.tick(&p, &inp);
        }
        assert_eq!(rig.radius, row().max_radius);
    }

    #[test]
    fn camera_never_ends_up_behind_a_wall_while_following() {
        let p = params();
        let mut s = subject(Vec3::new(300.0, 0.0, 0.0));
        s.facing = Vec3::X;
        let rig = &mut Rig::new(&p, row(), &s, &wall);
        // He faces +x and backs toward the wall at x = 0, so the camera behind him is pushed into it.
        let mut closest = f32::MAX;
        for k in 0..300 {
            s.pos.x = 450.0 - k as f32 * 1.4;
            s.moved = true;
            let inp = input(s, &wall);
            rig.tick(&p, &inp);
            assert!(rig.pos.x > 0.0 && rig.look.x > 0.0, "tick {k}: pos {} look {}", rig.pos, rig.look);
            closest = closest.min(rig.pos.distance(s.pos + Vec3::Y * LOOK_HEIGHT));
        }
        // At the end he stands 30 from the wall; the camera was squeezed in close, not left at 400.
        assert!(closest < 60.0, "{closest}");
    }

    #[test]
    fn lock_on_looks_toward_the_target_and_comes_in_to_the_lock_radius() {
        let p = params();
        let s = subject(Vec3::ZERO);
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        let target = Vec3::new(0.0, 100.0, -300.0);
        let mut inp = input(s, &no_wall);
        inp.lock = Some(target);
        for _ in 0..300 {
            rig.tick(&p, &inp);
        }
        // The look-at point sits between the character's chest and the target.
        assert!(rig.look.z < 0.0 && rig.look.z > -300.0, "{}", rig.look);
        assert!((rig.radius - row().lock_radius).abs() <= 1.0, "{}", rig.radius);
    }

    #[test]
    fn blockers_stop_the_wanted_position() {
        let p = params();
        let s = subject(Vec3::ZERO);
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        // A sphere on the camera's circle behind him (he faces -z, the camera sits on +z).
        // The wanted position is (0, 420, 400); the ray from the chest (0, 170, 0) is at
        // (0, 357.5, 300) three quarters of the way.
        let list = [Blocker { center: Vec3::new(0.0, 357.5, 300.0), radius: 50.0 }];
        let mut inp = input(s, &no_wall);
        inp.blockers = &list;
        for _ in 0..200 {
            rig.tick(&p, &inp);
        }
        assert!(rig.pos.z < 270.0 && rig.pos.z > 150.0, "{}", rig.pos);
    }

    #[test]
    fn look_around_enters_on_select_and_tilts_with_the_stick() {
        let p = params();
        let mut s = subject(Vec3::new(0.0, 0.0, 0.0));
        s.facing = Vec3::NEG_Z;
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        let mut inp = input(s, &no_wall);
        inp.select = true;
        rig.tick(&p, &inp);
        assert_eq!(rig.mode, Mode::Look);
        assert!((rig.fade.value - 1.0).abs() < 0.05 && rig.fade.step < 0.0, "fading out: {:?}", rig.fade);
        inp.select = false;
        for _ in 0..120 {
            rig.tick(&p, &inp);
        }
        // Camera at the eye, looking along the facing (-z), 350 ahead.
        let eye = Vec3::new(0.0, 150.0, 0.0);
        assert!(rig.pos.distance(eye) < 2.0, "{}", rig.pos);
        assert!(rig.look.distance(eye + Vec3::NEG_Z * 350.0) < 3.0, "{}", rig.look);
        assert_eq!(rig.fade.value, 0.0);
        // Stick up raises the look-at point; the pitch stops at the limit.
        inp.sticks.right = Vec2::new(0.0, -1.0);
        inp.sticks.right_mag = 1.0;
        for _ in 0..200 {
            rig.tick(&p, &inp);
        }
        assert!((rig.look_pitch - p.look.pitch_max).abs() < 1e-5, "{}", rig.look_pitch);
        assert!(rig.look.y > eye.y + 300.0, "{}", rig.look);
        // Select again leaves: back behind him, fading in.
        inp.sticks.right_mag = 0.0;
        inp.select = true;
        rig.tick(&p, &inp);
        assert_eq!(rig.mode, Mode::Follow);
        assert!(rig.fade.step > 0.0);
        inp.select = false;
        for _ in 0..200 {
            rig.tick(&p, &inp);
        }
        assert!(rig.pos.z > 300.0, "{}", rig.pos);
        assert_eq!(rig.fade.value, 1.0);
        // Not allowed in a state without the flag.
        s.can_look = false;
        inp.subject = s;
        inp.select = true;
        rig.tick(&p, &inp);
        assert_eq!(rig.mode, Mode::Follow);
    }

    #[test]
    fn look_around_pulls_the_look_point_to_a_wall() {
        let p = params();
        let mut s = subject(Vec3::new(100.0, 0.0, 0.0));
        s.facing = Vec3::NEG_X;
        let rig = &mut Rig::new(&p, row(), &s, &wall);
        let mut inp = input(s, &wall);
        inp.select = true;
        rig.tick(&p, &inp);
        inp.select = false;
        for _ in 0..120 {
            rig.tick(&p, &inp);
        }
        assert!(rig.look.x.abs() < 1e-2, "look-at point sits on the wall: {}", rig.look);
    }

    #[test]
    fn look_around_starts_only_from_select() {
        let p = params();
        let mut s = subject(Vec3::ZERO);
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        let mut inp = input(s, &no_wall);
        // Every other control, moving, resetting, zooming and locking on: still the follow camera.
        for k in 0..400 {
            s.moved = k % 2 == 0;
            s.pos.x += if s.moved { 8.0 } else { 0.0 };
            inp.subject = s;
            inp.sticks = Sticks { left: Vec2::new(1.0, 0.0), left_mag: (k % 3) as f32 * 0.5, right: Vec2::new(0.0, if k % 5 == 0 { -1.0 } else { 1.0 }), right_mag: (k % 4) as f32 / 3.0 };
            inp.reset = k % 7 == 0;
            inp.zoom = if k % 11 == 0 { 40.0 } else { 0.0 };
            inp.lock = (k % 13 == 0).then_some(Vec3::new(0.0, 100.0, -300.0));
            rig.tick(&p, &inp);
            assert_ne!(rig.mode, Mode::Look, "tick {k}");
        }
        inp = input(s, &no_wall);
        inp.select = true;
        rig.tick(&p, &inp);
        assert_eq!(rig.mode, Mode::Look);
        // Holding it does not toggle: only a new press (the harness and the key give one tick).
        inp.select = false;
        for _ in 0..60 {
            rig.tick(&p, &inp);
        }
        assert_eq!(rig.mode, Mode::Look);
    }

    #[test]
    fn look_turn_is_the_right_stick_and_turns_left_for_left() {
        let p = params();
        let mut sticks = Sticks::default();
        assert_eq!(look_turn(&p, &sticks), 0.0);
        // The left stick never turns.
        sticks.left = Vec2::new(-1.0, 0.0);
        sticks.left_mag = 1.0;
        assert_eq!(look_turn(&p, &sticks), 0.0);
        // Right stick left: the game's facing gets `turn_left` (negative), the app's yaw the opposite.
        sticks.right = Vec2::new(-1.0, 0.0);
        sticks.right_mag = 1.0;
        let d = look_turn(&p, &sticks);
        assert_eq!(d, -p.look.turn_left);
        // An app yaw that grows turns the facing toward the view's left: right = (-fz, 0, fx).
        let a = 0.3f32;
        let f0 = Vec3::new(a.sin(), 0.0, a.cos());
        let f1 = Vec3::new((a + d).sin(), 0.0, (a + d).cos());
        let left = -Vec3::new(-f0.z, 0.0, f0.x);
        assert!((f1 - f0).dot(left) > 0.0);
        // Up and down only tilt (`look_tick`), they do not turn; a diagonal turns at the full rate.
        sticks.right = Vec2::new(0.0, -1.0);
        assert_eq!(look_turn(&p, &sticks), 0.0);
        sticks.right = Vec2::new(0.707, -0.707);
        assert_eq!(look_turn(&p, &sticks), -p.look.turn_right);
        // The look-around's view follows the turned facing (yaw = behind the facing every tick).
        let mut s = subject(Vec3::ZERO);
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        let mut inp = input(s, &no_wall);
        inp.select = true;
        rig.tick(&p, &inp);
        inp.select = false;
        let yaw = (s.facing.x).atan2(s.facing.z) + 20.0 * -p.look.turn_left;
        s.facing = Vec3::new(yaw.sin(), 0.0, yaw.cos());
        inp.subject = s;
        rig.tick(&p, &inp);
        assert!(wrap(rig.look_yaw - yaw).abs() < 1e-4, "{} {}", rig.look_yaw, yaw);
    }

    #[test]
    fn overhead_camera_follows_the_floor_point_and_select_swaps_the_view() {
        let p = params();
        let mut s = subject(Vec3::new(0.0, 0.0, 0.0));
        let floor = |a: Vec3, b: Vec3| {
            let d = b - a;
            (d.y.abs() > 1e-6 && (a.y / -d.y) >= 0.0 && (a.y / -d.y) <= 1.0).then(|| Hit { point: Vec3::new(a.x, 0.0, a.z), normal: Vec3::Y, attr: 1 })
        };
        let rig = &mut Rig::new(&p, row(), &s, &floor);
        rig.mode = Mode::Overhead;
        let mut inp = input(s, &floor);
        for _ in 0..300 {
            rig.tick(&p, &inp);
        }
        let want = Vec3::new(0.0, 150.0, 0.0);
        assert!(rig.look.distance(want) < 2.0, "{}", rig.look);
        assert!(rig.pos.distance(want + p.overhead.offset) < 2.0, "{}", rig.pos);
        assert_eq!(rig.fov, p.overhead.fov);
        // A far target is reached in one step, a near one eases in.
        s.pos = Vec3::new(1000.0, 0.0, 0.0);
        inp.subject = s;
        rig.tick(&p, &inp);
        assert!(rig.look.distance(Vec3::new(1000.0, 150.0, 0.0)) < 1e-2, "{}", rig.look);
        s.pos = Vec3::new(1100.0, 0.0, 0.0);
        inp.subject = s;
        rig.tick(&p, &inp);
        assert!((rig.look.x - 1050.0).abs() < 1e-2, "half of a 100 gap: {}", rig.look);
        inp.select = true;
        rig.tick(&p, &inp);
        assert_eq!(rig.pos, p.overhead.fixed_pos);
        assert_eq!(rig.look, p.overhead.fixed_look);
        assert_eq!(rig.fov, p.overhead.fixed_fov);
    }

    #[test]
    fn behind_camera_ignores_the_stick_turn_and_tracks_the_facing() {
        let p = params();
        let mut s = subject(Vec3::ZERO);
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        rig.mode = Mode::Behind;
        let mut inp = input(s, &no_wall);
        inp.sticks.right = Vec2::new(1.0, 0.0);
        inp.sticks.right_mag = 1.0;
        let y0 = rig.yaw;
        for _ in 0..30 {
            rig.tick(&p, &inp);
        }
        assert!((wrap(rig.yaw - y0)).abs() < 1e-5, "{}", rig.yaw);
        s.facing = Vec3::X;
        inp.subject = s;
        inp.sticks.right_mag = 0.0;
        for _ in 0..120 {
            rig.tick(&p, &inp);
        }
        assert!(wrap(rig.yaw - behind_yaw(Vec3::X)).abs() < 1e-4, "{}", rig.yaw);
    }

    #[test]
    fn area_types_pick_rows_and_modes() {
        assert_eq!(area_setup(0, false), (0, Mode::Follow));
        assert_eq!(area_setup(0, true), (1, Mode::Follow));
        assert_eq!(area_setup(1, true), (0, Mode::Overhead));
        assert_eq!(area_setup(2, false), (2, Mode::Follow));
        assert_eq!(area_setup(3, false), (3, Mode::Behind));
    }

    #[test]
    fn event_camera_springs_and_hands_over() {
        let p = params();
        let s = subject(Vec3::ZERO);
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        let mut ev = EventCamera::start(rig);
        assert_eq!(ev.view().pos, rig.pos);
        let (pos, look) = (Vec3::new(2000.0, 500.0, 0.0), Vec3::new(2000.0, 100.0, -300.0));
        let mut v = ev.view();
        for _ in 0..400 {
            v = ev.frame(&p, pos, look, 1.0, 0.1, None, true);
        }
        assert!(v.pos.distance(pos) < 1.5 && v.look.distance(look) < 1.5, "{v:?}");
        assert!((v.fov - 1.0).abs() < 0.01 && (v.roll - 0.1).abs() < 0.01, "{v:?}");
        // Without smoothing the values are used as given.
        let v = ev.frame(&p, Vec3::ZERO, Vec3::X, 0.9, 0.0, None, false);
        assert_eq!(v.pos, Vec3::ZERO);
        // Handover: the field camera starts from the event's view and springs back behind him.
        rig.handover(&p, &s, v);
        assert_eq!(rig.pos, Vec3::ZERO);
        for _ in 0..300 {
            rig.tick(&p, &input(s, &no_wall));
        }
        assert!(rig.pos.z > 390.0, "{}", rig.pos);
    }

    /// `FUN_001592d8` argument 1: the camera on the line from the look-at point through the
    /// player, `dist` out and `height` up; argument 0: behind the object along its local z.
    #[test]
    fn talk_pose_places_the_camera_past_the_player_or_in_the_objects_frame() {
        let look = Vec3::new(100.0, 150.0, 200.0);
        let shot = TalkShot { look, dist: 300.0, height: 40.0, frame: TalkFrame::AroundPlayer };
        let pos = talk_pose(&shot, Vec3::new(100.0, 0.0, 300.0));
        assert!((pos - Vec3::new(100.0, 190.0, 500.0)).length() < 1e-3, "{pos}");
        // The direction is horizontal whatever the heights, and its length is the distance.
        let pos = talk_pose(&shot, Vec3::new(400.0, -80.0, -200.0));
        let off = (pos - look).with_y(0.0);
        assert!((off.length() - 300.0).abs() < 1e-3 && (pos.y - 190.0).abs() < 1e-3);
        assert!(off.normalize().dot(Vec3::new(300.0, 0.0, -400.0).normalize()) > 0.9999);
        // A player on the look-at point: atan2(0, 0) = 0, game +z, the app's -z.
        let pos = talk_pose(&shot, look);
        assert!((pos - Vec3::new(100.0, 190.0, -100.0)).length() < 1e-3, "{pos}");
        let obj = TalkShot { frame: TalkFrame::Object { forward: Vec3::X }, ..shot };
        let pos = talk_pose(&obj, Vec3::ZERO);
        assert!((pos - Vec3::new(-200.0, 190.0, 200.0)).length() < 1e-3, "{pos}");
    }

    /// The talk camera cuts to its view on the first tick, holds it while the player stands
    /// (the sticks do nothing), and its end snaps back to the follow camera of before.
    #[test]
    fn talk_camera_cuts_in_holds_and_warps_back() {
        let p = params();
        let s = subject(Vec3::ZERO);
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        for _ in 0..60 {
            rig.tick(&p, &input(s, &no_wall));
        }
        let (before_pos, before_look, yaw, radius) = (rig.pos, rig.look, rig.yaw, rig.radius);
        let shot = TalkShot { look: Vec3::new(0.0, 160.0, -200.0), dist: 350.0, height: 30.0, frame: TalkFrame::AroundPlayer };
        rig.start_talk(&p, shot);
        let mut i = input(s, &no_wall);
        i.sticks = Sticks { right: Vec2::new(1.0, 0.0), right_mag: 1.0, ..Sticks::default() };
        rig.tick(&p, &i);
        let want = talk_pose(&shot, s.pos);
        assert_eq!(rig.mode, Mode::Talk);
        assert!(rig.pos.distance(want) < 1e-3 && rig.look.distance(shot.look) < 1e-3, "cut: {} {}", rig.pos, rig.look);
        assert!((rig.fov - p.talk.fov).abs() < 1e-6 && rig.roll == 0.0);
        for _ in 0..120 {
            rig.tick(&p, &i);
        }
        assert!(rig.pos.distance(want) < 1e-2 && (rig.fov - p.talk.fov).abs() < 1e-4, "held: {}", rig.pos);
        assert_eq!((rig.yaw, rig.radius), (yaw, radius), "the follow camera's yaw and radius are kept");
        // A wall between the look-at point and the view pulls the wanted position in.
        let wall_shot = TalkShot { look: Vec3::new(-100.0, 160.0, 0.0), dist: 400.0, height: 0.0, frame: TalkFrame::AroundPlayer };
        let s2 = subject(Vec3::new(100.0, 0.0, 0.0));
        rig.start_talk(&p, wall_shot);
        for _ in 0..200 {
            rig.tick(&p, &input(s2, &wall));
        }
        assert!(rig.pos.x < 0.0 && rig.pos.x > -10.0, "pulled in front of the wall: {}", rig.pos);
        rig.start_talk(&p, shot);
        rig.tick(&p, &input(s, &no_wall));
        rig.end_talk();
        rig.tick(&p, &input(s, &no_wall));
        assert_eq!(rig.mode, Mode::Follow);
        assert!(rig.pos.distance(before_pos) < 1e-2 && rig.look.distance(before_look) < 1e-2, "warped back: {} {}", rig.pos, rig.look);
        assert!((rig.fov - row().fov).abs() < 1e-4);
        // From the look-around the talk ends it and the player fades back in.
        let rig = &mut Rig::new(&p, row(), &s, &no_wall);
        rig.tick(&p, &TickIn { select: true, ..input(s, &no_wall) });
        assert_eq!(rig.mode, Mode::Look);
        rig.start_talk(&p, shot);
        assert_eq!(rig.mode, Mode::Talk);
        assert!(rig.fade.step > 0.0 && (rig.fade.step - p.look.fade_to / p.talk.look_fade_ticks).abs() < 1e-6);
    }

    #[test]
    fn exe_table_round_trip() {
        // A fake image with the table's addresses filled, to check the offsets are consistent.
        let mut exe = vec![0u8; 0x380000 - EXE_BIAS];
        let mut put = |va: usize, v: f32| exe[va - EXE_BIAS..va - EXE_BIAS + 4].copy_from_slice(&v.to_le_bytes());
        for (va, v) in [
            (0x378f34, 1.4), (0x378f38, 400.0), (0x378f3c, 350.0), (0x378f40, 200.0), (0x378f44, 500.0), (0x378f48, 0.01),
            (0x378f04, 1.0), (0x378f20, 600.0), (0x378f24, 520.0), (0x378f28, 400.0), (0x378f30, 700.0), (0x378f2c, 0.005),
            (0x378f4c, 633.0), (0x378f08, 0.17), (0x378f0c, 0.004), (0x378f10, 0.0002), (0x378f14, 0.05),
            (0x378e48, 5.0), (0x378e44, 30.0), (0x378e74, 350.0), (0x378f18, -1.2), (0x378f1c, 1.2), (0x378e58, 1.0),
            (0x378e54, 30.0), (0x378f00, 1.0), (0x378e78, 30.0), (0x37939c, 100.0), (0x3793a0, 10000.0), (0x3793c8, 150.0),
            (0x3793cc, 200.0), (0x3793a8, 600.0), (0x3793b0, -500.0), (0x3793b4, 51.0), (0x3793b8, -1492.0), (0x3793bc, 256.0),
            (0x3793c0, -1777.0), (0x3793c4, 1.5), (0x3793d0, 1.05), (0x379470, 2.5), (0x379474, 0.5), (0x379478, 0.1),
            (0x37947c, 1.0), (0x379480, 0.07), (0x379484, 0.07), (0x379488, 2.5), (0x37948c, 0.5), (0x379490, 0.001),
            (0x37a4c8, -0.05), (0x37a4cc, 0.05), (0x3793e4, 1.04), (0x3793d8, 30.0),
        ] {
            put(va, v);
        }
        let p = Params::from_exe(&exe).expect("table");
        assert_eq!(p.rows[0].radius, 400.0);
        assert_eq!(p.rows[2].max_radius, 633.0);
        assert_eq!(p.rows[1].fov, 1.0);
        assert_eq!(p.overhead.offset, Vec3::new(600.0, 500.0, 0.0), "Y flipped to up");
        assert_eq!(p.look.distance, 350.0);
        assert_eq!((p.look.turn_left, p.look.turn_right), (-0.05, 0.05));
        assert_eq!(p.talk, TalkParams { fov: 1.04, look_fade_ticks: 30.0 });
        assert!(Params::from_exe(&exe[..1000]).is_none());
        assert!(Params::from_exe(&vec![0u8; 0x380000 - EXE_BIAS]).is_none());
    }
}
