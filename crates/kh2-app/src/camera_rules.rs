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
/// Distance a collided camera is kept off the surface, along the surface normal (`FUN_0013f8f8`).
const WALL_MARGIN: f32 = 5.0;
/// Radius change per tick at full stick (`FUN_0013fd08`, 8 per tick).
pub const ZOOM_RATE: f32 = 8.0;

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

/// The camera's numbers, read from the user's executable (see [`Params::from_exe`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    pub rows: [Row; 4],
    /// Largest yaw change per tick when the camera swings toward its target yaw.
    pub yaw_step: f32,
    /// Auto-camera turn rate: `auto_base + auto_per_speed * (run speed - 8)` radians per tick.
    pub auto_base: f32,
    pub auto_per_speed: f32,
    /// Right-stick yaw rate per tick at full tilt.
    pub stick_yaw: f32,
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

/// Distance along a segment to the first surface, or `None`.
pub type Cast<'a> = &'a dyn Fn(Vec3, Vec3) -> Option<f32>;

/// Pulls a camera back from a surface between `look` and `pos` (`FUN_0013f8f8`): the new position
/// is the hit point pushed 5 along the surface normal. The normal is found from two more rays
/// offset sideways; the result is checked to be reachable from `look` and falls back to backing
/// off along the ray. `None` when nothing is in the way.
pub fn pull_in(look: Vec3, pos: Vec3, cast: Cast) -> Option<Vec3> {
    let seg = pos - look;
    let len = seg.length();
    if len < 1e-3 {
        return None;
    }
    let dir = seg / len;
    let t0 = cast(look, pos)?;
    let h0 = look + dir * t0;
    let back = look + dir * (t0 - WALL_MARGIN).max(0.0);
    let e1 = dir.any_orthonormal_vector();
    let e2 = dir.cross(e1);
    let probe = |e: Vec3| -> Option<Vec3> {
        let off = e * 3.0;
        cast(look + off, pos + off).map(|t| look + off + dir * t)
    };
    let n = match (probe(e1), probe(e2)) {
        (Some(h1), Some(h2)) => {
            let n = (h1 - h0).cross(h2 - h0);
            // Reject probes that hit different surfaces (a corner or an edge).
            let flat = (h1 - h0).dot(dir).abs() < 60.0 && (h2 - h0).dot(dir).abs() < 60.0;
            (flat && n.length_squared() > 1e-6).then(|| {
                let n = n.normalize();
                if n.dot(look - h0) < 0.0 { -n } else { n }
            })
        }
        _ => None,
    };
    let Some(n) = n else { return Some(back) };
    let out = h0 + n * WALL_MARGIN;
    // The pushed-out point must be on the look-at side of every surface.
    match cast(look, out) {
        None => Some(out),
        Some(_) => Some(back),
    }
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
}

/// The camera object's state (`0x355840`).
#[derive(Clone, Debug)]
pub struct Rig {
    pub row: Row,
    pub radius: f32,
    pub yaw: f32,
    pub target_yaw: f32,
    /// Smoothed look-at point and position, what is drawn.
    pub look: Vec3,
    pub pos: Vec3,
    pub fov: f32,
    wanted_look: Vec3,
    prev_wanted_look: Vec3,
    wanted_pos: Vec3,
    prev_wanted_pos: Vec3,
    wanted_fov: f32,
    prev_wanted_fov: f32,
}

impl Rig {
    /// A camera placed straight behind the subject with no smoothing (`FUN_0013f470`).
    pub fn new(row: Row, s: &Subject, cast: Cast) -> Rig {
        let yaw = behind_yaw(s.facing);
        let look = s.pos + Vec3::Y * LOOK_HEIGHT;
        let mut pos = wanted_position(look, row.radius, yaw, row.min_radius, row.height_k);
        if let Some(c) = pull_in(look, pos, cast) {
            pos = c;
        }
        Rig {
            row,
            radius: row.radius,
            yaw,
            target_yaw: yaw,
            look,
            pos,
            fov: row.fov,
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

    /// One 60 Hz tick (`FUN_00140a80`).
    pub fn tick(&mut self, p: &Params, i: &TickIn) {
        self.prev_wanted_look = self.wanted_look;
        self.prev_wanted_pos = self.wanted_pos;
        self.prev_wanted_fov = self.wanted_fov;
        let sub = &i.subject;
        // Where the camera looked last tick, for the lock-on screen tests.
        let (view_pos, view_look, view_fov) = (self.pos, self.look, self.fov);
        let chest = sub.pos + Vec3::Y * LOOK_HEIGHT;
        let mut snap_yaw = false;
        // Both cameras orbit the character's chest; the lock-on look-at point is another matter.
        let orbit_around = chest;
        match i.lock {
            None => {
                // Follow camera (`FUN_001402c8`).
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
        let mut wp = wanted_position(orbit_around, self.radius, self.yaw, self.row.min_radius, self.row.height_k);
        if let Some(c) = pull_in(orbit_around, wp, i.cast) {
            wp = c;
        }
        self.wanted_pos = wp;
        if i.warp {
            self.look = self.wanted_look;
            self.pos = self.wanted_pos;
            self.fov = self.wanted_fov;
            self.prev_wanted_look = self.wanted_look;
            self.prev_wanted_pos = self.wanted_pos;
        }
        // Smoothing (`FUN_001407b8`).
        let t = smoothing_time(sub.run_speed, i.warp);
        self.look = spring(self.look, self.wanted_look, self.prev_wanted_look, t + 0.03, 2.5, 0.5, 1.0);
        self.pos = spring(self.pos, self.wanted_pos, self.prev_wanted_pos, t, 2.5, 0.5, 1.0);
        self.fov = spring(Vec3::new(self.fov, 0.0, 0.0), Vec3::new(self.wanted_fov, 0.0, 0.0), Vec3::new(self.prev_wanted_fov, 0.0, 0.0), t, 2.5, 0.5, 0.001).x;
        // Collision on the smoothed position; a hit also becomes the wanted position.
        let from = if i.lock.is_some() { chest } else { self.look };
        if let Some(c) = pull_in(from, self.pos, i.cast) {
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
        Params { rows: [row(); 4], yaw_step: 0.17, auto_base: 0.004, auto_per_speed: 0.0002, stick_yaw: 0.05 }
    }

    fn no_wall(_: Vec3, _: Vec3) -> Option<f32> {
        None
    }

    /// An infinite wall on the plane x = 0 (both sides).
    fn wall(a: Vec3, b: Vec3) -> Option<f32> {
        let d = b - a;
        if d.x.abs() < 1e-6 {
            return None;
        }
        let t = -a.x / d.x;
        (0.0..=1.0).contains(&t).then(|| t * d.length())
    }

    fn subject(pos: Vec3) -> Subject {
        Subject { pos, facing: Vec3::NEG_Z, moved: false, run_speed: 8.0 }
    }

    fn input<'a>(s: Subject, cast: Cast<'a>) -> TickIn<'a> {
        TickIn { sticks: Sticks::default(), reset: false, subject: s, lock: None, zoom: 0.0, auto: true, warp: false, cast }
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
    fn pull_in_never_ends_beyond_the_wall() {
        // Sweep many camera positions behind a wall at x = 0, from the look-at side.
        let mut n = 0;
        for ly in [20.0f32, 170.0] {
            for lx in [30.0f32, 120.0, 400.0] {
                for ang in 0..72 {
                    let a = ang as f32 * 5.0f32.to_radians();
                    let look = Vec3::new(lx, ly, 10.0);
                    let want = look + Vec3::new(a.cos(), 0.3 * a.sin(), a.sin()) * 450.0;
                    let out = pull_in(look, want, &wall).unwrap_or(want);
                    assert!(out.x > 0.0, "ends past the wall: look {look} want {want} out {out}");
                    n += 1;
                    if want.x <= 0.0 {
                        // The wall is in the way: the camera is pulled in, not left behind it.
                        assert!(out.distance(look) < want.distance(look));
                        assert!(out.x >= 0.0);
                    } else {
                        assert!(pull_in(look, want, &wall).is_none());
                    }
                }
            }
        }
        assert!(n > 100);
    }

    #[test]
    fn pull_in_keeps_the_margin_from_the_wall() {
        let look = Vec3::new(100.0, 100.0, 0.0);
        let want = Vec3::new(-300.0, 140.0, 80.0);
        let out = pull_in(look, want, &wall).unwrap();
        assert!((out.x - WALL_MARGIN).abs() < 0.5, "{out}");
    }

    #[test]
    fn pull_in_with_nothing_in_the_way() {
        assert!(pull_in(Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0), &no_wall).is_none());
    }

    #[test]
    fn rig_settles_behind_a_standing_subject() {
        let p = params();
        let s = subject(Vec3::new(1000.0, 0.0, 1000.0));
        let rig = &mut Rig::new(row(), &s, &no_wall);
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
        let rig = &mut Rig::new(row(), &s, &no_wall);
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
        let rig = &mut Rig::new(row(), &s, &no_wall);
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
        let rig = &mut Rig::new(row(), &s, &wall);
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
        let rig = &mut Rig::new(row(), &s, &no_wall);
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
    fn exe_table_round_trip() {
        // A fake image with the table's addresses filled, to check the offsets are consistent.
        let mut exe = vec![0u8; 0x380000 - EXE_BIAS];
        let mut put = |va: usize, v: f32| exe[va - EXE_BIAS..va - EXE_BIAS + 4].copy_from_slice(&v.to_le_bytes());
        for (va, v) in [
            (0x378f34, 1.4), (0x378f38, 400.0), (0x378f3c, 350.0), (0x378f40, 200.0), (0x378f44, 500.0), (0x378f48, 0.01),
            (0x378f04, 1.0), (0x378f20, 600.0), (0x378f24, 520.0), (0x378f28, 400.0), (0x378f30, 700.0), (0x378f2c, 0.005),
            (0x378f4c, 633.0), (0x378f08, 0.17), (0x378f0c, 0.004), (0x378f10, 0.0002), (0x378f14, 0.05),
        ] {
            put(va, v);
        }
        let p = Params::from_exe(&exe).expect("table");
        assert_eq!(p.rows[0].radius, 400.0);
        assert_eq!(p.rows[2].max_radius, 633.0);
        assert_eq!(p.rows[1].fov, 1.0);
        assert!(Params::from_exe(&exe[..1000]).is_none());
        assert!(Params::from_exe(&vec![0u8; 0x380000 - EXE_BIAS]).is_none());
    }
}
