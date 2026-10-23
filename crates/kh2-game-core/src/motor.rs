//! Character movement at the game's 60 Hz logic rate, in game units (centimetres, Y up).
//!
//! Behaviour follows the game's object movement (studied in the local decompilation; nothing
//! copied):
//! * Ground: velocity is the input direction times walk or run speed, with no acceleration;
//!   facing turns toward the input by the `prty` turn speed per tick and snaps when within it.
//! * Jump of height h: rise time T = sqrt(2h/g) with g = 20/49 per tick², height eased as
//!   h·sin(sin(t/T·π/2)·π/2), then a parabola ½g(t-T)²; each tick's drop is capped at FallMax.
//!   Falls continue the same parabola. Releasing jump between tick 6 and T-5 swaps the rest of
//!   the rise for a 5.1-unit bump; a ceiling ends the rise.
//! * Air: velocity → 0.9·velocity + 0.1·input velocity, or ×0.95 without input.
//! * Root motion of the playing motion is added to the displacement, rotated by facing.

use bevy::prelude::*;

use crate::world::World;

/// Gravity of the jump parabola (units per tick²).
pub const G: f32 = 20.0 / 49.0;
/// Air control blend and drag.
const AIR_KEEP: f32 = 0.9;
const AIR_DRAG: f32 = 0.95;
/// Height of the bump that replaces the rest of a rise when jump is released early.
const SHORT_HOP: f32 = 250.0 / 49.0;

/// Capsule of a character (Sora's body shape: radius 35, see the model's collision shapes).
#[derive(Clone, Copy, Debug)]
pub struct Capsule {
    pub radius: f32,
    pub height: f32,
    /// Ledges up to this height are stepped onto.
    pub step: f32,
}

impl Default for Capsule {
    fn default() -> Self {
        Capsule { radius: 35.0, height: 150.0, step: 40.0 }
    }
}

/// The analytic jump of the game's jump action.
#[derive(Clone, Copy, Debug, Default)]
pub struct Jump {
    /// Ticks since the jump (or fall) started.
    pub t: f32,
    /// Rise time T (0 for a fall).
    pub rise: f32,
    /// Height at the start.
    pub base: f32,
    pub height: f32,
    /// Jump button still held (short hop allowed).
    pub hold: bool,
    /// Releasing the jump button may cut the rise short. Set for the jump press and the Aerial
    /// Dodge (they set actor +0x108 bit 0x10000, which the jump mover's start FUN_0017b328 turns
    /// into the release-watch bit 0x20000); clear for an attack's leap (FUN_00176050 launches the
    /// mover without it, so FUN_0017b328 clears 0x20000 and FUN_00174cf8 never sets the cut bit
    /// 0x40000).
    pub release_cuts: bool,
    /// Fall motion not yet started.
    pub fall_motion_pending: bool,
}

impl Jump {
    /// A jump of `h` from height `y`.
    pub fn up(h: f32, y: f32) -> Jump {
        Jump { t: 0.0, rise: (2.0 * h / G).sqrt(), base: y, height: h, hold: true, release_cuts: true, fall_motion_pending: true }
    }

    /// A fall as if it had started `drop` above `y` (0 = start falling now).
    pub fn fall(drop: f32, y: f32) -> Jump {
        Jump { t: (2.0 * drop / G).sqrt(), rise: 0.0, base: y + drop, height: 0.0, hold: false, release_cuts: false, fall_motion_pending: true }
    }

    /// A fall from `y` that is already moving down at `speed` units per tick (the jump mover
    /// restarted by FUN_0016a2f0: clock `+0xd0` = speed / g, no rise, base so that the curve
    /// passes through the current height).
    pub fn falling_at(speed: f32, y: f32) -> Jump {
        let v = speed.max(0.0);
        Jump::fall(v * v / (2.0 * G), y)
    }

    pub fn target_y(&self) -> f32 {
        use std::f32::consts::FRAC_PI_2;
        if self.t < self.rise {
            self.base + self.height * ((self.t / self.rise * FRAC_PI_2).sin() * FRAC_PI_2).sin()
        } else {
            let d = self.t - self.rise;
            self.base + self.height - 0.5 * G * d * d
        }
    }

    pub fn rising(&self) -> bool {
        self.t < self.rise
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Motor {
    pub pos: Vec3,
    /// Facing (radians; 0 = +Z).
    pub yaw: f32,
    /// Horizontal velocity per tick.
    pub vel: Vec3,
    pub grounded: bool,
    pub jump: Jump,
    /// The last tick hit a ceiling while rising.
    pub ceiling: bool,
    pub capsule: Capsule,
    /// Ground under the character (when grounded).
    pub ground: Option<crate::world::Ground>,
}

pub fn wrap_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (a + PI).rem_euclid(TAU) - PI
}

impl Motor {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos())
    }

    /// Turns toward `dir` by at most `rate` radians; returns true while still turning.
    pub fn turn_to(&mut self, dir: Vec3, rate: f32) -> bool {
        if dir.xz().length_squared() < 1e-8 {
            return false;
        }
        let target = dir.x.atan2(dir.z);
        let d = wrap_angle(target - self.yaw);
        if d > rate {
            self.yaw = wrap_angle(self.yaw + rate);
            true
        } else if d < -rate {
            self.yaw = wrap_angle(self.yaw - rate);
            true
        } else {
            self.yaw = target;
            false
        }
    }

    /// Ground action: instant velocity along the input.
    pub fn ground_velocity(&mut self, dir: Vec3, speed: f32) {
        self.vel = if speed > 0.0 { dir.with_y(0.0).normalize_or_zero() * speed } else { Vec3::ZERO };
    }

    /// Air action horizontal control.
    pub fn air_velocity(&mut self, dir: Vec3, speed: f32) {
        self.vel = if speed > 0.0 {
            self.vel * AIR_KEEP + dir.with_y(0.0).normalize_or_zero() * speed * (1.0 - AIR_KEEP)
        } else {
            self.vel * AIR_DRAG
        };
    }

    pub fn start_jump(&mut self, h: f32) {
        self.jump = Jump::up(h, self.pos.y);
        self.grounded = false;
        self.ground = None;
    }

    /// A launch of the jump mover (its method +8, FUN_001796a8 -> FUN_0017b278 / FUN_0017b1e0): an
    /// attack's leap (FUN_00176050) or the air wall recoil (FUN_00185978). `h` at or above 0 is a
    /// new jump of `h` from here (FUN_0017b1e0: base +0xd8 0, rise +0xdc -h, clock 0, rise time
    /// FUN_0017bee0); below 0 it is a fall already under way, as if it had started `-h` above
    /// (clock sqrt(h x -2 / g), DAT_0037a4f0 = -2, DAT_0037a4f4 = g; no rise). The jump button cannot
    /// cut either short (FUN_0017b328 clears the release watch because the launch did not set
    /// +0x108 bit 0x10000).
    pub fn start_leap(&mut self, h: f32) {
        if h >= 0.0 {
            self.start_jump(h);
            self.jump.release_cuts = false;
        } else {
            self.jump = Jump::fall(-h, self.pos.y);
            self.grounded = false;
            self.ground = None;
        }
    }

    pub fn start_fall(&mut self) {
        self.jump = Jump::fall(0.0, self.pos.y);
        self.grounded = false;
        self.ground = None;
    }

    /// Jump-action control: short hop on early release, ceilings end the rise.
    /// Returns true when the fall motion should start now.
    pub fn jump_control(&mut self, jump_held: bool) -> bool {
        let j = &mut self.jump;
        if !jump_held {
            j.hold = false;
        }
        let mut fall_motion = false;
        if self.ceiling && j.t > 6.0 && j.t < j.rise {
            fall_motion = std::mem::take(&mut j.fall_motion_pending);
            *j = Jump { fall_motion_pending: false, ..Jump::fall(0.0, self.pos.y) };
        } else if j.release_cuts && !j.hold && j.t > 6.0 && j.t < j.rise - 5.0 {
            fall_motion = std::mem::take(&mut j.fall_motion_pending);
            *j = Jump { fall_motion_pending: false, ..Jump::up(SHORT_HOP, self.pos.y) };
            j.hold = false;
        } else if j.t + 5.0 >= j.rise {
            fall_motion = std::mem::take(&mut j.fall_motion_pending);
        }
        fall_motion
    }

    /// Vertical displacement of the jump action for this tick (advances the jump clock).
    pub fn jump_dy(&mut self, fall_max: f32) -> f32 {
        self.jump.t += 1.0;
        (self.jump.target_y() - self.pos.y).max(-fall_max)
    }

    /// Moves by `delta` against the world. Grounded characters follow floors and step up and
    /// down; airborne ones land when they reach a floor. Returns true on landing.
    pub fn move_by(&mut self, world: &World, delta: Vec3, airborne_dy: Option<f32>) -> bool {
        let c = self.capsule;
        self.ceiling = false;
        let start = self.pos;
        let horizontal = delta.with_y(0.0);
        world.slide(&mut self.pos, horizontal, c.radius, c.step, c.height);
        match airborne_dy {
            None => {
                // Follow the floor: step up to `step`, down as far as the tick's run speed allows.
                let down = c.step + horizontal.length() + 2.0;
                match world.ground(self.pos, self.pos.y + c.step, c.step + down) {
                    Some(g) => {
                        self.pos.y = g.y;
                        self.grounded = true;
                        self.ground = Some(g);
                    }
                    None => {
                        self.grounded = false;
                        self.ground = None;
                        self.start_fall();
                    }
                }
                false
            }
            Some(dy) => {
                let y0 = self.pos.y;
                if dy > 0.0 {
                    if let Some(cy) = world.ceiling(self.pos, y0 + c.height - 1.0, dy + 1.0) {
                        self.pos.y = (cy - c.height).max(y0);
                        self.ceiling = true;
                    } else {
                        self.pos.y += dy;
                    }
                    false
                } else {
                    let to = y0 + dy;
                    match world.ground(self.pos, y0 + c.step.min(10.0), (y0 - to) + c.step.min(10.0)) {
                        Some(g) if g.y >= to => {
                            self.pos.y = g.y;
                            self.grounded = true;
                            self.ground = Some(g);
                            // The jump base follows ground corrections (the game shifts its
                            // base height by the collision's vertical correction).
                            true
                        }
                        _ => {
                            self.pos.y = to;
                            let _ = start;
                            false
                        }
                    }
                }
            }
        }
    }
}

/// A sphere of radius `r` swept from `a` to `b` (game units) against the walls of the map: the
/// first wall-like triangle (normal more than 60 degrees from vertical, the same class the
/// capsule slides along) within `r` of the segment, as (contact point, normal). COCT walls are
/// one-sided, as in `World::slide`.
///
/// This is the shape of the game's probe (FUN_0019a8e8 -> FUN_00143840 -> FUN_00164138: the
/// actor's collision sphere stepped along its delta in radius-sized steps against the polygons
/// in the swept box). Polygons are filtered by attribute word: `(attr & 0xf) != 4` and `attr & mask`
/// (the handler of mask index 0: flag 0x10, PARTY_COLLIDE). That the handler's third argument is
/// the polygon word, and that the probe's mask index is 0, are inferences (the handler table is
/// assembly and the decompiler dropped the argument); walls are told from floors by the normal.
pub fn sweep_wall(world: &World, a: Vec3, b: Vec3, r: f32, mask: u32) -> Option<(Vec3, Vec3)> {
    if !(a.is_finite() && b.is_finite()) {
        return None;
    }
    let lo = a.min(b) - Vec3::splat(r);
    let hi = a.max(b) + Vec3::splat(r);
    let ab = b - a;
    let len2 = ab.length_squared();
    let mut best: Option<(f32, Vec3, Vec3)> = None;
    for (i, t) in world.tris.iter().enumerate() {
        if !world.group_enabled(i) {
            continue;
        }
        // Polygon filter of FUN_00141b50: the attribute nibble is not 4 and the handler of the
        // probe's mask index (0 for a party member, FUN_0032e3b8 table at 0x378f7c: index 0 and 6
        // test flag 0x10) accepts the word. Maps without COCT flags (flags 0) never match.
        if t.flags & mask == 0 || t.flags & 0xf == 4 || t.n.y.abs() >= crate::world::FLOOR_NY || t.hi.x < lo.x || t.lo.x > hi.x || t.hi.y < lo.y || t.lo.y > hi.y || t.hi.z < lo.z || t.lo.z > hi.z {
            continue;
        }
        // Closest points between the segment and the triangle (a few alternating projections).
        let mut s = 0.5f32;
        let mut q = closest_point_triangle(a + ab * s, t);
        for _ in 0..4 {
            s = if len2 > 1e-6 { ((q - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
            q = closest_point_triangle(a + ab * s, t);
        }
        let p = a + ab * s;
        if world.coct && t.n.dot(p - t.v[0]) < 0.0 {
            continue;
        }
        let d = q.distance(p);
        if d <= r && best.is_none_or(|(bd, _, _)| d < bd) {
            best = Some((d, q, t.n));
        }
    }
    best.map(|(_, q, n)| (q, n))
}

fn closest_point_triangle(p: Vec3, t: &crate::world::Tri) -> Vec3 {
    // Ericson, Real-Time Collision Detection, 5.1.5.
    let [a, b, c] = t.v;
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Host 0.45 (FUN_0016d098): `a + b / |b.xyz| x step x DAT_00356f40` on all four lanes (the frame step 1.0).
pub fn advance_vec(a: [f32; 4], b: [f32; 4], step: f32) -> [f32; 4] {
    let len = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    let q = if len > 0.0 { step / len } else { 0.0 };
    std::array::from_fn(|k| a[k] + b[k] * q)
}

/// Host 0.46 (FUN_0016d138 -> FUN_0017c0f8): `a` turned toward `b` by at most `rate` (times the frame step
/// 1.0). Both are normalised by their xyz length; the angle between them is FUN_0017bf10's (acos of the dot,
/// 0 at or above 1.0 `DAT_0037a53c`, pi at or below -1.0 `DAT_0037a540` / `DAT_0037a544`), held at `rate`;
/// FUN_0017bf88 builds the rotation by that angle about the unit `a x b` (rows `x^2 t + c, xy t + z s, xz t -
/// y s` .., `t = 1 - c`), and the result is `a` through it with w 0. A zero cross product gives a zero axis
/// (the vector unit's division by zero times 0): the result is `a x cos(angle)`.
pub fn turn_vec(a: [f32; 4], b: [f32; 4], rate: f32) -> [f32; 4] {
    let unit = |v: [f32; 4]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 0.0 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0; 3] }
    };
    let (u, w) = (unit(a), unit(b));
    let dot = u[0] * w[0] + u[1] * w[1] + u[2] * w[2];
    let angle = if dot >= 1.0 {
        0.0
    } else if dot <= -1.0 {
        std::f32::consts::PI
    } else {
        dot.acos()
    };
    let step = angle.min(rate);
    let k = unit([u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0], 0.0]);
    let (s, c) = step.sin_cos();
    let t = 1.0 - c;
    let (x, y, z) = (k[0], k[1], k[2]);
    let rows = [[x * x * t + c, x * y * t + z * s, x * z * t - y * s], [x * y * t - z * s, y * y * t + c, y * z * t + x * s], [x * z * t + y * s, y * z * t - x * s, z * z * t + c]];
    let r: [f32; 3] = std::array::from_fn(|j| rows[0][j] * u[0] + rows[1][j] * u[1] + rows[2][j] * u[2]);
    [r[0], r[1], r[2], 0.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_finds_a_wall_ahead_and_ignores_floors() {
        // A floor square and a wall across x = 100 facing -x, from render-style meshes.
        let tri = |v: [Vec3; 3], flags: u32| {
            let n = (v[1] - v[0]).cross(v[2] - v[0]).normalize();
            crate::world::Tri { v, n, flags, material: 0, lo: v[0].min(v[1]).min(v[2]), hi: v[0].max(v[1]).max(v[2]) }
        };
        let w = |x: f32, flags: u32| tri([Vec3::new(x, -200.0, -400.0), Vec3::new(x, -200.0, 400.0), Vec3::new(x, 600.0, 0.0)], flags);
        let floor = tri([Vec3::new(-500.0, 0.0, -500.0), Vec3::new(500.0, 0.0, 500.0), Vec3::new(500.0, 0.0, -500.0)], 0x10);
        let mut world = World::default();
        world.tris = vec![floor, w(100.0, 0x10), w(300.0, 0x10 | 4), w(-100.0, 0x80)];
        let start = Vec3::new(0.0, 40.0, 0.0);
        // 35 + 30 of reach along +x from x = 0: the sphere's front reaches x = 100.
        let hit = sweep_wall(&world, start, start + Vec3::X * 65.0, 35.0, 0x10);
        assert!(hit.is_some_and(|(q, _)| (q.x - 100.0).abs() < 1e-3), "{hit:?}");
        // Short of it by a unit: nothing.
        assert!(sweep_wall(&world, start, start + Vec3::X * 64.0, 35.0, 0x10).is_none());
        // Facing away: nothing, and the floor never counts.
        assert!(sweep_wall(&world, start, start - Vec3::X * 65.0, 35.0, 0x10).is_none());
        // Attribute 4 and walls without the party flag do not count.
        assert!(sweep_wall(&world, Vec3::new(240.0, 40.0, 0.0), Vec3::new(305.0, 40.0, 0.0), 35.0, 0x10).is_none());
        assert!(sweep_wall(&world, Vec3::new(-40.0, 40.0, 0.0), Vec3::new(-105.0, 40.0, 0.0), 35.0, 0x10).is_none());
        assert!(sweep_wall(&world, Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, 10.0, 60.0), 35.0, 0x10).is_none());
    }

    #[test]
    fn jump_reaches_its_height_at_the_rise_time() {
        let mut j = Jump::up(185.0, 0.0);
        assert!((j.rise - 30.1).abs() < 0.1, "{}", j.rise);
        j.t = j.rise;
        assert!((j.target_y() - 185.0).abs() < 1e-3);
        j.t = j.rise + 10.0;
        assert!((j.target_y() - (185.0 - 0.5 * G * 100.0)).abs() < 1e-3);
    }

    #[test]
    fn a_leap_is_not_cut_by_the_jump_button_and_a_fall_keeps_its_speed() {
        let mut m = Motor::default();
        m.start_leap(185.0);
        for _ in 0..8 {
            m.jump.t += 1.0;
            m.jump_control(false);
        }
        assert_eq!(m.jump.height, 185.0, "the leap's rise was cut");
        let mut j = Motor::default();
        j.start_jump(185.0);
        for _ in 0..8 {
            j.jump.t += 1.0;
            j.jump_control(false);
        }
        assert!(j.jump.height < 10.0, "a released jump still hops short");
        // A fall that already moves down 6 units a tick drops about 6.2 on its next tick.
        let f = Jump::falling_at(6.0, 100.0);
        let mut n = f;
        n.t += 1.0;
        let drop = f.target_y() - n.target_y();
        assert!((f.target_y() - 100.0).abs() < 1e-3 && (drop - (6.0 + 0.5 * G)).abs() < 1e-3, "{drop}");
    }

    #[test]
    fn a_negative_launch_is_a_fall_under_way() {
        // FUN_0017b1e0 with a height below 0: no rise, clock sqrt(2|h|/g), base |h| above.
        let mut m = Motor { pos: Vec3::new(0.0, 100.0, 0.0), ..Motor::default() };
        m.start_leap(-30.0);
        assert!(!m.grounded && m.jump.rise == 0.0 && !m.jump.rising());
        assert!((m.jump.target_y() - 100.0).abs() < 1e-3);
        let dy = m.jump_dy(1.0e6);
        // Already moving down at sqrt(2 g 30) a tick.
        assert!(dy < -(2.0 * G * 30.0).sqrt() + 0.01, "{dy}");
    }

    #[test]
    fn fall_continues_the_parabola() {
        let j = Jump::fall(0.0, 100.0);
        assert_eq!(j.target_y(), 100.0);
        let j2 = Jump::fall(50.0, 100.0);
        assert!((j2.target_y() - 100.0).abs() < 1e-3);
    }
}
