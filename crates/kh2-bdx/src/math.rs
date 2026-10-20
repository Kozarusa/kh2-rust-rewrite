//! Host function group 0: vector arithmetic, random numbers and the frame constant. These touch no game
//! state; the game implements them natively with the console's vector unit, and here they are plain arithmetic.

use crate::vm::{self, Mem, VmError};

fn fl(v: u32) -> f32 {
    f32::from_bits(v)
}

/// Scale of the random float: 2^-32.
const RAND_SCALE: f32 = 2.328_306_4e-10;

/// Coefficients of the polynomial the game's vector unit routine uses for sine (order of terms: z^4, z^3, z^2, z).
const SIN_POLY: [f32; 4] = [f32::from_bits(0x362e_9c14), f32::from_bits(0xb94f_b21f), f32::from_bits(0x3c08_873e), f32::from_bits(0xbe2a_aaa4)];
const HALF_PI: f32 = f32::from_bits(0x3fc9_0fdb);

/// Sine and cosine of an angle in -pi..pi the way the game's rotation helper gets them: a polynomial at
/// pi/2 - |b| gives cos b, the sine follows from the square root of 1 - cos^2 with the sign of b.
fn game_sin_cos(b: f32) -> (f32, f32) {
    let x = if b < 0.0 { HALF_PI + b } else { HALF_PI - b };
    let z = x * x;
    let mut t = SIN_POLY.map(|c| c * x * z);
    for x in t.iter_mut().take(3) {
        *x *= z;
    }
    for x in t.iter_mut().take(2) {
        *x *= z;
    }
    t[0] *= z;
    // Terms are added in this order: x, z*c4, z^2*c3, z^3*c2, z^4*c1.
    let cos = (((x + t[3]) + t[2]) + t[1]) + t[0];
    let q = (1.0 - cos * cos).abs().sqrt();
    (if b < 0.0 { -q } else { q }, cos)
}

/// Reads and writes the vectors a math function is pointed at (script memory, or host memory through the caller).
pub trait VecAccess {
    fn read_vec(&mut self, mem: &mut Mem<'_>, addr: u32) -> Result<[f32; 4], VmError>;
    fn write_vec(&mut self, mem: &mut Mem<'_>, addr: u32, v: [f32; 4]) -> Result<(), VmError>;
}

/// Rotation about the Y axis: x' = x cos + z sin, z' = -x sin + z cos. The angle (radians) is wrapped
/// into -pi..pi with the game's own routine first; an angle of exactly zero leaves the vector alone.
pub fn rotate_y(v: [f32; 4], ang: f32) -> [f32; 4] {
    let pi = std::f32::consts::PI;
    let wrapped = if ang > 0.0 { ((ang + pi) % std::f32::consts::TAU) - pi } else { pi - ((pi - ang) % std::f32::consts::TAU) };
    if wrapped == 0.0 {
        return v;
    }
    let (s, c) = game_sin_cos(wrapped);
    [vm::ee_clamp(v[0] * c + v[2] * s), v[1], vm::ee_clamp(-v[0] * s + v[2] * c), v[3]]
}

/// Rotation about the X axis (host 0.68, FUN_0016cf78 -> FUN_0017c398 with the matrix of FUN_0011b490 -> FUN_002fb580): y' = y cos - z sin,
/// z' = y sin + z cos, the angle wrapped like `rotate_y`'s. The sign is the one the oracle test `vec_rotate_x_matches_the_game` finds in the game.
pub fn rotate_x(v: [f32; 4], ang: f32) -> [f32; 4] {
    let pi = std::f32::consts::PI;
    let wrapped = if ang > 0.0 { ((ang + pi) % std::f32::consts::TAU) - pi } else { pi - ((pi - ang) % std::f32::consts::TAU) };
    if wrapped == 0.0 {
        return v;
    }
    let (s, c) = game_sin_cos(wrapped);
    [v[0], vm::ee_clamp(v[1] * c - v[2] * s), vm::ee_clamp(v[1] * s + v[2] * c), v[3]]
}

/// Length of the xyz part, with the vector unit's saturating arithmetic.
fn length(v: [f32; 4]) -> f32 {
    let sq = |a: f32| vm::ee_clamp(a * a);
    vm::ee_clamp(vm::ee_clamp(sq(v[0]) + sq(v[1])) + sq(v[2])).sqrt()
}

/// True for the indexes of group 0 that this module implements.
pub fn is_math(index: u16) -> bool {
    matches!(index, 3 | 4 | 5 | 6 | 7 | 13 | 16 | 17 | 18 | 23 | 35 | 36 | 37 | 68)
}

/// Runs math function `index` of group 0. Vector results come back as a pointer into the scratch buffers.
pub fn call(mem: &mut Mem<'_>, index: u16, a: &[u32], io: &mut dyn VecAccess) -> Result<Option<u32>, VmError> {
    let f = |x: f32| Ok(Some(x.to_bits()));
    match index {
        // The per-frame time unit: scripts count frames, one frame is 1.0.
        3 => f(1.0),
        4 | 5 => {
            let (x, y) = (io.read_vec(mem, a[0])?, io.read_vec(mem, a[1])?);
            let sign = if index == 4 { 1.0 } else { -1.0 };
            let v = std::array::from_fn(|k| vm::ee_clamp(x[k] + sign * y[k]));
            Ok(Some(mem.sh.put_vec(v)))
        }
        6 => {
            let v = io.read_vec(mem, a[0])?;
            f(length(v))
        }
        7 => {
            // Normalises xyz in place and stores the length in w.
            let v = io.read_vec(mem, a[0])?;
            let len = length(v);
            let q = vm::ee_clamp(1.0 / len);
            io.write_vec(mem, a[0], [v[0] * q, v[1] * q, v[2] * q, len])?;
            f(len)
        }
        13 => {
            let v = io.read_vec(mem, a[0])?;
            Ok(Some(mem.sh.put_vec(rotate_y(v, fl(a[1])))))
        }
        68 => {
            let v = io.read_vec(mem, a[0])?;
            Ok(Some(mem.sh.put_vec(rotate_x(v, fl(a[1])))))
        }
        16 => {
            let n = a[0];
            let seed = mem.sh.next_rand();
            if n == 0 {
                return Err(VmError::DivideByZero { pc: 0 });
            }
            let q = (1u64 << 32).div_ceil(n as u64);
            Ok(Some((seed as u64 / q) as u32))
        }
        17 => {
            let seed = mem.sh.next_rand();
            f(seed as f32 * RAND_SCALE * fl(a[0]))
        }
        18 => {
            let seed = mem.sh.next_rand();
            let (lo, hi) = (fl(a[0]), fl(a[1]));
            f(lo + seed as f32 * RAND_SCALE * (hi - lo))
        }
        // Absolute value: the game clears the sign bit.
        23 => Ok(Some(a[0] & 0x7fff_ffff)),
        // Multiply all four lanes by a scalar, in place.
        35 => {
            let v = io.read_vec(mem, a[0])?;
            let s = fl(a[1]);
            io.write_vec(mem, a[0], std::array::from_fn(|k| vm::ee_clamp(v[k] * s)))?;
            Ok(None)
        }
        36 => {
            let v = io.read_vec(mem, a[0])?;
            let s = fl(a[1]);
            Ok(Some(mem.sh.put_vec(std::array::from_fn(|k| vm::ee_clamp(v[k] * s)))))
        }
        // Divide all four lanes by a scalar: the vector unit's reciprocal (a division by zero saturates) times each lane.
        37 => {
            let v = io.read_vec(mem, a[0])?;
            let q = vm::ee_clamp(1.0 / fl(a[1]));
            Ok(Some(mem.sh.put_vec(std::array::from_fn(|k| vm::ee_clamp(v[k] * q)))))
        }
        _ => Err(VmError::UnknownHost { group: 0, index }),
    }
}
