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

/// orig 0x17bbc0: an angle wrapped to [-pi, pi] (`fmod` of the angle shifted by pi, shifted back; the two branches keep the sign).
pub fn fix_radian(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    if a > 0.0 { (a + PI) % TAU - PI } else { PI - (PI - a) % TAU }
}

/// orig 0x17bc30: `angle` moved toward `wanted` by at most `step` (the shorter way round), wrapped.
pub fn turn_angle(angle: f32, wanted: f32, step: f32) -> f32 {
    let d = wanted - angle;
    let wrapped = fix_radian(d);
    let moved = if wrapped < -step {
        angle - step
    } else if step < wrapped {
        angle + step
    } else {
        angle + wrapped
    };
    fix_radian(moved)
}

impl Motor {
    pub fn forward(&self) -> Vec3 {
        Vec3::new(self.yaw.sin(), 0.0, self.yaw.cos())
    }

    /// Turns toward `dir` by at most `rate` radians (one tick of the frame step 1.0); returns true while still turning.
    pub fn turn_to(&mut self, dir: Vec3, rate: f32) -> bool {
        self.turn_to_step(dir, rate, 1.0)
    }

    /// The facing turn of the object tick (`FUN_0016a2f0`): the wanted yaw is the direction's `atan2(x, z)` (`FUN_0016a948`), the
    /// facing moves toward it by at most `rate * step` ([`turn_angle`]). `step` is the frame step of the tick that runs. The app
    /// runs the game's logic once per vblank of a frame, so a frame of step 2 is two ticks of `step` 1.0 and the turn per frame is
    /// the same as the original's one turn of `rate * 2`.
    pub fn turn_to_step(&mut self, dir: Vec3, rate: f32, step: f32) -> bool {
        if dir.xz().length_squared() < 1e-8 {
            return false;
        }
        let target = dir.x.atan2(dir.z);
        let max = rate * step;
        let turning = fix_radian(target - self.yaw).abs() > max;
        self.yaw = turn_angle(self.yaw, target, max);
        turning
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

// ---------------------------------------------------------------------------------------------------------------------------------
// The ground mover (mover 0) and the mover table (WP-L2-P3). The mover is a shared descriptor, not a per-actor block: all its state lives
// in the actor block (+0x10 the move vector with the speed in its fourth word, +0x20 the velocity, +0x140 the motion controller). The
// functions below take the actor fields they read as plain values and answer what the original writes or calls, so the oracle tests
// (`tests/orig_l2_p3.rs`) can run the original on the same inputs and compare. Callees that are other systems (the footstep sound
// `FUN_0016baa0`, the dust `FUN_001b1c90`, the motion request `FUN_0017e2d8`, the jump launches `FUN_0017b278` / `FUN_0017b1e0`, the
// action change `FUN_0016b698`, the jump mover step `FUN_001832b8`) are not run: the result names the call and its arguments.
// ---------------------------------------------------------------------------------------------------------------------------------

/// The constants the ground mover reads from the executable (`DAT_0037a4e4` .. `DAT_0037a4ec`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundConsts {
    /// 0x37a4e4: the first argument of the motion request (the blend time).
    pub request_blend: f32,
    /// 0x37a4e8: the factor on walk plus run speed that splits walk from run.
    pub class_split: f32,
    /// 0x37a4ec: the fourth word of the displacement vector.
    pub out_w: f32,
}

impl GroundConsts {
    /// The values the USA executable holds (checked against it by `orig_l2_p3::the_ground_constants_are_the_ports`).
    pub const USA: GroundConsts = GroundConsts { request_blend: 8.0, class_split: 0.5, out_w: 1.0 };

    /// Reads the three words from the USA executable (`None` for another file).
    pub fn from_exe(elf: &[u8]) -> Option<GroundConsts> {
        use crate::reaction_rules::exe_f32;
        Some(GroundConsts { request_blend: exe_f32(elf, 0x37a4e4)?, class_split: exe_f32(elf, 0x37a4e8)?, out_w: exe_f32(elf, 0x37a4ec)? })
    }
}

/// What `FUN_0017af10` reads of the actor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroundMotionIn {
    /// Actor +0x1c: the speed word of the move vector.
    pub speed: f32,
    /// Actor +0x110 and +0x114: walk and run speed.
    pub walk: f32,
    pub run: f32,
    /// Actor +0x588: class flags (bit 0x80: a player class).
    pub class_flags: u32,
    /// Actor +0x154: the motion id last requested (motion controller +0x14).
    pub motion_id: i32,
    /// Actor +0x164: motion flags 2 (bit 1: range trigger 42 open).
    pub flags2: u32,
    /// Actor +0x160: motion flags (range triggers; bit 4 and 0x10 let a requested motion through).
    pub flags: u32,
    /// The object entry's part number (`FUN_001699a8`, entry +0x4c); 0xe means no foot effects.
    pub part_num: i16,
}

/// The foot effects `FUN_0017af10` starts before it requests the motion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FootCues {
    /// The footstep sound of kind 8 (`FUN_0016baa0`), played on the stop.
    pub stop_sound: bool,
    /// The dust kinds `FUN_001b1c90` is asked for, in order (10 and 11 on the first step, 8 and 9 on the stop).
    pub dust: Option<[u16; 2]>,
}

/// The result of `FUN_0017af10`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GroundMotion {
    /// Motion class: 0 idle, 1 walk, 2 run (also the id requested).
    pub class: u8,
    pub foot: FootCues,
    /// `FUN_0017aeb8`: the class asked of `FUN_0017e2d8`, or `None` when the gate keeps the motion.
    pub request: Option<u8>,
}

/// orig 0x17aeb8: the gate of the motion request: motion flags bit 4 always lets it through, bit 0x10 only for a class that is not 0.
pub fn motion_request_gate(flags: u32, class: u8) -> bool {
    flags & 4 != 0 || (flags & 0x10 != 0 && class != 0)
}

/// orig 0x17af10: the ground mover's tick slot. The class comes from the speed word: 0 when it is not above 0, else 1, or 2 when it
/// reaches `(walk + run) * class_split`. A player class (+0x588 bit 0x80) with the motion last requested idle starts the dust 10 and 11
/// on the first moving tick, and one in walk or run (motion 1 or 2) whose range trigger 42 is closed (+0x164 bit 1) plays the stop
/// sound and the dust 8 and 9 on the first tick without speed; the part number 0xe skips the dust. Then the motion request of
/// `FUN_0017aeb8`.
pub fn ground_motion(a: &GroundMotionIn, k: &GroundConsts) -> GroundMotion {
    let mut out = GroundMotion::default();
    let player = a.class_flags & 0x80 != 0;
    if 0.0 < a.speed {
        out.class = if a.speed < (a.walk + a.run) * k.class_split { 1 } else { 2 };
        if player && a.motion_id == 0 && a.part_num != 0xe {
            out.foot.dust = Some([10, 11]);
        }
    } else if player && a.flags2 & 1 == 0 && (a.motion_id as u32).wrapping_sub(1) < 2 {
        out.foot.stop_sound = true;
        if a.part_num != 0xe {
            out.foot.dust = Some([8, 9]);
        }
    }
    out.request = motion_request_gate(a.flags, out.class).then_some(out.class);
    out
}

/// What `FUN_0017b040` reads of the actor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GroundStepIn {
    /// Actor +0x10: the move vector (direction in xyz, speed in w).
    pub dir: [f32; 4],
    /// Actor +0x160 (bit 0x40: the step is the jump mover's with +0xf0) and +0x164 (bit 1: the step is the jump mover's with the row).
    pub flags: u32,
    pub flags2: u32,
    /// Actor +0xf0.
    pub f0: f32,
    /// The row behind actor +0x12c: the words at +0x28 and +0x2c.
    pub row_28: f32,
    pub row_2c: f32,
    /// Actor +0x810 (non-zero: the row's +0x28 is used twice).
    pub w810: u32,
}

/// The result of `FUN_0017b040`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GroundStep {
    /// The jump mover's step `FUN_001832b8(a, b, out)` answers.
    Jump(f32, f32),
    /// The displacement written to the result (xyz as `step`, the fourth word `out_w`) and the velocity written to actor +0x20.
    Walk { out: [f32; 4], velocity: [f32; 4] },
}

/// orig 0x17b040: the ground mover's displacement slot. Unless the flags hand the step to the jump mover, a speed above 0 gives the
/// displacement `normalise(xyz) * speed * frame_step` and the velocity (the displacement times the speed again; the fourth word is the
/// speed word carried through the same multiplies); no speed gives zeros for both.
pub fn ground_step(a: &GroundStepIn, k: &GroundConsts, frame_step: f32) -> GroundStep {
    if a.flags & 0x40 != 0 {
        return GroundStep::Jump(a.f0, a.f0);
    }
    if a.flags2 & 1 != 0 {
        return GroundStep::Jump(a.row_28, if a.w810 == 0 { a.row_2c } else { a.row_28 });
    }
    let w = a.dir[3];
    if 0.0 < w {
        let len = (a.dir[0] * a.dir[0] + a.dir[1] * a.dir[1] + a.dir[2] * a.dir[2]).sqrt();
        let q = 1.0 / len;
        let unit = [a.dir[0] * q, a.dir[1] * q, a.dir[2] * q];
        let s = w * frame_step;
        GroundStep::Walk { out: [unit[0] * s, unit[1] * s, unit[2] * s, k.out_w], velocity: [unit[0] * s * w, unit[1] * s * w, unit[2] * s * w, w * s * w] }
    } else {
        GroundStep::Walk { out: [0.0, 0.0, 0.0, k.out_w], velocity: [0.0; 4] }
    }
}

/// orig 0x17ade0: the launch vector of a jump: the actor's move vector (+0x10) with its second word replaced by the jump height the
/// ability lookup `FUN_0016bd98` answers; with `set_release_watch` the actor's flags +0x108 also get bit 0x10000 (the jump button can
/// cut the rise, `FUN_0017b328` turns it into the release-watch bit). Returns the vector and the new flags.
pub fn launch_vector(dir: [f32; 4], height: f32, set_release_watch: bool, flags108: u32) -> ([f32; 4], u32) {
    ([dir[0], height, dir[2], dir[3]], if set_release_watch { flags108 | 0x10000 } else { flags108 })
}

/// The calls `FUN_0017ae50` makes after the launch vector is built.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JumpStart {
    /// The vector handed to the jump mover's launch `FUN_0017b278`.
    pub vector: [f32; 4],
    /// Actor +0x108 after the launch vector (bit 0x10000 when the release watch was asked for).
    pub flags108: u32,
    /// The motion request `FUN_0017e220(0.0, 0.0, motion controller, id)`: the id the ability lookup answered (3, or 0xc9 for High Jump).
    pub motion_id: i32,
    /// Actor +0x100 and +0x104 as the function leaves them (4 and 5: the state the jump leaves the actor in [inferred: group time and a step counter]).
    pub w100: u32,
    pub w104: u32,
}

/// orig 0x17ae50: starts a jump from the move vector: the launch vector (`launch_vector`), the jump mover's launch with it, the motion
/// request for the id the ability lookup gave (blend 0.0), and actor +0x104 = 5, +0x100 = 4.
pub fn jump_start(dir: [f32; 4], height: f32, motion_id: i32, set_release_watch: bool, flags108: u32) -> JumpStart {
    let (vector, flags108) = launch_vector(dir, height, set_release_watch, flags108);
    JumpStart { vector, flags108, motion_id, w100: 4, w104: 5 }
}

/// How the jump mover's enter slots launch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MoverLaunch {
    /// `FUN_0017b278` with a vector.
    Vector([f32; 4]),
    /// `FUN_0017b1e0` with the height in the float argument register.
    Height(f32),
}

/// What an enter slot of the jump mover does: a launch, then `FUN_0016b698(actor, action)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoverEnter {
    pub launch: MoverLaunch,
    pub action: u32,
}

/// orig 0x1796a8: the jump mover's method with a vector: launches with a copy of it, then changes the action to 1.
pub fn enter_with_vector(v: [f32; 4]) -> MoverEnter {
    MoverEnter { launch: MoverLaunch::Vector(v), action: 1 }
}

/// orig 0x1796e8: the jump mover's method without a vector: launches by height (the float argument the caller left), then action 1.
pub fn enter_with_height(height: f32) -> MoverEnter {
    MoverEnter { launch: MoverLaunch::Height(height), action: 1 }
}

/// orig 0x179718: the shared zero vector. The first call clears its four words, sets the flag and registers the exit handler
/// (`FUN_0030fe80(0x17aca8)`); the answer is always four zero words. Returns the vector and the handler registered by this call.
pub fn shared_zero_vector(ready: &mut bool) -> ([f32; 4], Option<u32>) {
    let registered = if *ready {
        None
    } else {
        *ready = true;
        Some(0x17aca8)
    };
    ([0.0; 4], registered)
}

/// The pointer table of the mover descriptors in the executable (`0x35ca48`, ids 0 to 0x3f).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoverTable {
    pointers: Vec<u32>,
}

impl MoverTable {
    /// Reads the 64 pointers from the USA executable (`None` for another file).
    pub fn from_exe(elf: &[u8]) -> Option<MoverTable> {
        use crate::reaction_rules::exe_f32;
        let pointers = (0..MOVER_COUNT as u32).map(|i| exe_f32(elf, 0x35ca48 + 4 * i).map(f32::to_bits)).collect::<Option<Vec<_>>>()?;
        Some(MoverTable { pointers })
    }

    /// orig 0x179868: the descriptor address of mover `id`.
    pub fn descriptor(&self, id: usize) -> Option<u32> {
        self.pointers.get(id).copied()
    }
}

/// Number of entries in the pointer table.
pub const MOVER_COUNT: usize = 0x40;

/// One mover descriptor record: the vtable of the mover class (+0), the id (+4) and the flag word (+8; the ground mover has 0x180, the
/// jump mover 0x181: `FUN_00174c28` reads bit 0x100 of it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoverDescriptor {
    /// Address of the record in the static table at 0x1c88eb0.
    pub addr: u32,
    pub vtable: u32,
    pub id: u32,
    pub flags: u32,
}

/// orig 0x179880: the mover descriptor records the static initialiser writes at start-up (its construct call, arguments 1 and 0xffff):
/// each record's vtable, id and flag word (the flag word starts at 0 and gets the bits shown; the four records built by the base
/// constructors `FUN_002e6650`, `FUN_002e5540` and `FUN_002f0a78` carry the values those constructors write). The exit call (argument 0)
/// only puts the base vtables back and runs the destructors of the four constructor-built records; it is not ported.
pub fn mover_descriptors() -> &'static [MoverDescriptor] {
    const ROWS: [(u32, u32, u32, u32); 64] = [
    (0x01c88eb0, 0x35cbb0, 0x0, 0x180),
    (0x01c88ec0, 0x35cbe0, 0x1, 0x181),
    (0x01c88ed0, 0x35e2d0, 0x2, 0x105),
    (0x01c88ee0, 0x35cb80, 0x3, 0x100),
    (0x01c88ef0, 0x35e4a8, 0x4, 0x1c1),
    (0x01c88f00, 0x35e4d8, 0x5, 0x102),
    (0x01c88f10, 0x35e6d0, 0x6, 0x100),
    (0x01c88f20, 0x35e700, 0x7, 0x101),
    (0x01c88f30, 0x35e730, 0x8, 0x142),
    (0x01c88f40, 0x35efc0, 0x9, 0x101),
    (0x01c88f50, 0x35f050, 0xa, 0x4012a),
    (0x01c88f60, 0x35f1a0, 0xb, 0x103),
    (0x01c88f70, 0x35f2b8, 0xc, 0x101),
    (0x01c88f80, 0x35f288, 0xd, 0x102),
    (0x01c88f90, 0x35f378, 0xe, 0x311),
    (0x01c88fa0, 0x35f348, 0xf, 0x112),
    (0x01c88fb0, 0x35f4f0, 0x10, 0x100),
    (0x01c88fc0, 0x35f520, 0x11, 0x4101),
    (0x01c88fd0, 0x35f550, 0x12, 0x4101),
    (0x01c88fe0, 0x35eff0, 0x13, 0x902),
    (0x01c88ff0, 0x35f020, 0x14, 0x40123),
    (0x01c89000, 0x35fed8, 0x15, 0x16b),
    (0x01c89010, 0x35ff08, 0x16, 0x100),
    (0x01c89020, 0x35ff40, 0x17, 0x20000),
    (0x01c89030, 0x370c30, 0x18, 0x12902),
    (0x01c89040, 0x370c00, 0x18, 0x102),
    (0x01c89050, 0x3601a8, 0x1a, 0x101),
    (0x01c89060, 0x3716f8, 0x1f, 0x502),
    (0x01c89070, 0x3711b0, 0xd, 0x102),
    (0x01c89080, 0x360330, 0x1c, 0x0),
    (0x01c89090, 0x360610, 0x1d, 0x18a),
    (0x01c890a0, 0x370a18, 0x1e, 0x900),
    (0x01c89120, 0x371728, 0x20, 0x502),
    (0x01c89130, 0x371758, 0x21, 0x542),
    (0x01c89140, 0x370ac8, 0x1e, 0x100),
    (0x01c89180, 0x370a70, 0x1e, 0x100),
    (0x01c891c0, 0x3711e8, 0x1, 0x181),
    (0x01c891d0, 0x370348, 0x25, 0x102),
    (0x01c891e0, 0x3702b8, 0x26, 0x102),
    (0x01c891f0, 0x360640, 0x27, 0x102),
    (0x01c89200, 0x360710, 0x28, 0x102),
    (0x01c89210, 0x3606e0, 0x29, 0x102),
    (0x01c89220, 0x371430, 0x2a, 0x100),
    (0x01c89230, 0x371298, 0x2b, 0x1900),
    (0x01c89240, 0x371268, 0x2c, 0x1901),
    (0x01c89250, 0x371238, 0x2d, 0x1941),
    (0x01c89260, 0x3712e8, 0x2e, 0x902),
    (0x01c89270, 0x36f750, 0x2f, 0x900),
    (0x01c89280, 0x3713d8, 0x30, 0x100),
    (0x01c89290, 0x3713a8, 0x31, 0x101),
    (0x01c892a0, 0x3714c0, 0x32, 0x10a),
    (0x01c892b0, 0x371378, 0x33, 0x101),
    (0x01c892c0, 0x371460, 0x34, 0x10a),
    (0x01c892d0, 0x370d20, 0x35, 0x50902),
    (0x01c892e0, 0x370cf0, 0x18, 0x102),
    (0x01c892f0, 0x370318, 0x37, 0x102),
    (0x01c89300, 0x3702e8, 0x38, 0x102),
    (0x01c89310, 0x371490, 0x39, 0x10a),
    (0x01c89320, 0x36f8c0, 0x3a, 0x802),
    (0x01c89330, 0x36f8f0, 0x3b, 0x8900),
    (0x01c89340, 0x371788, 0x3c, 0x900),
    (0x01c89350, 0x371348, 0x3d, 0x2101),
    (0x01c89360, 0x3717e8, 0x3e, 0x900),
    (0x01c89370, 0x3717b8, 0x3f, 0x901),
    ];
    const TABLE: [MoverDescriptor; 64] = {
        let mut t = [MoverDescriptor { addr: 0, vtable: 0, id: 0, flags: 0 }; 64];
        let mut i = 0;
        while i < 64 {
            t[i] = MoverDescriptor { addr: ROWS[i].0, vtable: ROWS[i].1, id: ROWS[i].2, flags: ROWS[i].3 };
            i += 1;
        }
        t
    };
    &TABLE
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
