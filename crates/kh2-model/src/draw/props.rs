//! The float and byte arithmetic of the map props (WP-L1-P25c, spec `docs/systems/core-runtime/render-model.md` c6): the
//! colour scale, the motion clock, and the matrix and box maths the prop set-up and the per-frame world box use. The
//! prop block itself, its tasks and the draw tree are in `kh2_game_core::props`, which calls these.
//!
//! Prop offsets used here (the prop is the 0x2c0-byte object the map creates per prop record): `+0x1a4..+0x1a6` the base
//! colour bytes, `+0x2a8..+0x2aa` the scaled colour bytes (`0x80` is 1.0). The motion holder is the block at prop
//! `+0x110`: `+0x40` the motion table, `+0x44` the owner, `+0x48` the motion in use (0 for none), `+0x4c` its time, `+0x50`
//! its length (both floats, in the motion's own frame units). Arithmetic follows the executable: the vector unit's
//! multiply and add round to f32 one operation at a time and have no infinity ([`vu`]).

use super::frustum::{M4, V4, transform, vu};

/// The offset of the motion pointer in a motion holder.
pub const HOLDER_MOTION: usize = 0x48;
/// The offset of the motion time in a motion holder.
pub const HOLDER_TIME: usize = 0x4c;
/// The offset of the motion length in a motion holder.
pub const HOLDER_LENGTH: usize = 0x50;

/// One scaled colour channel of `0x15ff50`: the channel value (low byte only) times the base byte, shifted right by 7
/// (arithmetic, on the 32-bit product), truncated to a byte. The base `0x80` keeps the value.
pub fn colour_channel(base: u8, value: u32) -> u8 {
    ((((value & 0xff) * base as u32) as i32) >> 7) as u8
}

/// orig 0x15ff50: the prop's scaled colour. Prop block form: `+0x2a8 = (r * +0x1a4) >> 7`, `+0x2a9 = (g * +0x1a5) >> 7`,
/// `+0x2aa = (b * +0x1a6) >> 7`, each value masked to a byte first.
pub fn colour_scale(prop: &mut [u8], r: u32, g: u32, b: u32) {
    prop[0x2aa] = colour_channel(prop[0x1a6], b);
    prop[0x2a9] = colour_channel(prop[0x1a5], g);
    prop[0x2a8] = colour_channel(prop[0x1a4], r);
}

/// The three words of a motion holder the clock uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    /// The motion in use (a handle; 0 for none).
    pub motion: u32,
    /// The time of the pose, the holder's `+0x4c`.
    pub time: f32,
    /// The length, the holder's `+0x50`.
    pub length: f32,
}

/// The clock step of `0x161130` without the memory: with a motion, the time moves by `dt` and the pose of the new time
/// is taken (`sample`, the `0x1611b8` call); when the time is then beyond the length, the length is taken off it and
/// the pose of the length itself is taken (a looping motion shows its last pose once, then restarts from the stored
/// time on the next step). Without a motion nothing happens.
pub fn clock_step(c: &mut Clock, dt: f32, mut sample: impl FnMut(f32)) {
    if c.motion == 0 {
        return;
    }
    c.time += dt;
    sample(c.time);
    if c.length < c.time {
        c.time -= c.length;
        sample(c.length);
    }
}

/// orig 0x161130: the motion clock step on the holder's bytes (`+0x48` motion, `+0x4c` time, `+0x50` length); `sample`
/// stands for the pose call `0x1611b8` (a callee outside the clock: it writes the holder's matrix, not its clock).
pub fn motion_step(holder: &mut [u8], dt: f32, sample: impl FnMut(f32)) {
    let word = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let mut c = Clock {
        motion: word(holder, HOLDER_MOTION),
        time: f32::from_bits(word(holder, HOLDER_TIME)),
        length: f32::from_bits(word(holder, HOLDER_LENGTH)),
    };
    clock_step(&mut c, dt, sample);
    holder[HOLDER_TIME..HOLDER_TIME + 4].copy_from_slice(&c.time.to_bits().to_le_bytes());
}

/// The prop's own matrix from `0x15ffc0` after the rotation: the position (`prop +0x50`) is added to the translation
/// row on x, y and z (the w lane stays), then rows 0 to 2 are multiplied by the x, y and z of the scale (`prop +0x70`)
/// on all four lanes.
pub fn place_matrix(rotated: &M4, position: V4, scale: V4) -> M4 {
    let mut m = *rotated;
    for k in 0..3 {
        m[3][k] = vu(m[3][k] + position[k]);
    }
    for r in 0..3 {
        for k in 0..4 {
            m[r][k] = vu(m[r][k] * scale[r]);
        }
    }
    m
}

/// The matrix of `0x1601a8`: rows 1 and 2 of the prop's matrix scaled on all lanes by `row_scale` (the constant at
/// `0x3794e0`), then each row of the motion holder's matrix carried through the result; row 3 of the result is the
/// prop's centre.
pub fn world_matrix(object: &M4, holder: &M4, row_scale: f32) -> M4 {
    let mut a = *object;
    for r in 1..3 {
        for k in 0..4 {
            a[r][k] = vu(a[r][k] * row_scale);
        }
    }
    [transform(&a, holder[0]), transform(&a, holder[1]), transform(&a, holder[2]), transform(&a, holder[3])]
}

/// The box of `0x1601a8` before the corners: the holder's animation box, each side moved out by the owner's radius.
pub fn inflated_box(b: &[f32; 6], radius: f32) -> [f32; 6] {
    [b[0] - radius, b[1] - radius, b[2] - radius, b[3] + radius, b[4] + radius, b[5] + radius]
}

/// The corner walk of `0x160068`: each of the eight corners is carried through `m`, and the running per-axis minimum
/// and maximum are kept with strict compares (a corner below the minimum replaces it, else one above the maximum
/// replaces that; the first corner is both). Returns the carried corners and the box (min xyz, max xyz) with each axis
/// ordered smaller first as `0x1446f8` does.
pub fn transform_corners(corners: &[V4; 8], m: &M4) -> ([V4; 8], [f32; 6]) {
    let mut out = [[0.0f32; 4]; 8];
    let (mut lo, mut hi) = ([0.0f32; 4], [0.0f32; 4]);
    for (i, c) in corners.iter().enumerate() {
        let t = transform(m, *c);
        out[i] = t;
        if i == 0 {
            lo = t;
            hi = t;
        } else {
            for k in 0..3 {
                if t[k] < lo[k] {
                    lo[k] = t[k];
                } else if hi[k] < t[k] {
                    hi[k] = t[k];
                }
            }
        }
    }
    let mut b = [0.0f32; 6];
    for k in 0..3 {
        if lo[k] < hi[k] {
            b[k] = lo[k];
            b[k + 3] = hi[k];
        } else {
            b[k] = hi[k];
            b[k + 3] = lo[k];
        }
    }
    (out, b)
}
