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
    /// Fall motion not yet started.
    pub fall_motion_pending: bool,
}

impl Jump {
    /// A jump of `h` from height `y`.
    pub fn up(h: f32, y: f32) -> Jump {
        Jump { t: 0.0, rise: (2.0 * h / G).sqrt(), base: y, height: h, hold: true, fall_motion_pending: true }
    }

    /// A fall as if it had started `drop` above `y` (0 = start falling now).
    pub fn fall(drop: f32, y: f32) -> Jump {
        Jump { t: (2.0 * drop / G).sqrt(), rise: 0.0, base: y + drop, height: 0.0, hold: false, fall_motion_pending: true }
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
        } else if !j.hold && j.t > 6.0 && j.t < j.rise - 5.0 {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn fall_continues_the_parabola() {
        let j = Jump::fall(0.0, 100.0);
        assert_eq!(j.target_y(), 100.0);
        let j2 = Jump::fall(50.0, 100.0);
        assert!((j2.target_y() - 100.0).abs() < 1e-3);
    }
}
