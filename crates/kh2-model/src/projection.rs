//! The projected-shadow projection builder (render-model.md d2, L1-P26b): from a light position and the eight corners of a box it
//! builds two 4x4 matrices that take model-space points into the shadow texture's space, one for the texture size class and
//! one for the same class with the size folded into the scale. The caller is the shadow record builder (0x11dc40 in
//! `draw::shadow`), which hooked this routine out in its oracle.
//!
//! The routine runs on the vector unit in macro mode; every operation here follows the interpreter's rules (single precision,
//! results that overflow saturate to the largest finite value, the vector unit's divide for the normalisations and the FPU's for
//! the scale factors). Its two quirks are kept: the search for the axis least aligned with the view direction never looks past
//! the first axis (the table address is advanced by the best index, not the loop index, so every pass tests the first axis again
//! and the first axis is always chosen), and the first call initialises a three-vector axis table in the game's data (the flag at
//! 0x3c3200) and registers a destructor (callee 0x30fe80 with the address 0x135980).
//!
//! Not wired into the app: the renderer has no shadows (DEVIATIONS 2026-10-24). Wiring is the shadow record builder's, see
//! `draw::shadow`; this routine only turns a box and a light into the two matrices that builder stores.

use crate::light::Mat4;

/// The static block of the routine: the guard byte at 0x3c3200 and the axis table at 0x3c3210 (three vectors).
pub const STATICS: u32 = 0x3c3200;
/// Bytes of the static block this routine touches (0x3c3200 to 0x3c323f).
pub const STATICS_SIZE: usize = 0x40;
/// The matrix the identity initialiser 0x11b460 copies, at 0x354a60 (filled in by the game at run time; the identity).
pub const IDENTITY: u32 = 0x354a60;

fn sat(f: f32) -> f32 {
    if f.is_nan() {
        f32::MAX
    } else if f.is_infinite() {
        f32::MAX.copysign(f)
    } else {
        f
    }
}

fn vm(a: f32, b: f32) -> f32 {
    sat(a * b)
}
fn va(a: f32, b: f32) -> f32 {
    sat(a + b)
}
fn vs(a: f32, b: f32) -> f32 {
    sat(a - b)
}
fn madd(acc: f32, a: f32, b: f32) -> f32 {
    sat(acc + a * b)
}
fn msub(acc: f32, a: f32, b: f32) -> f32 {
    sat(acc - a * b)
}

/// `div.s`: a zero divisor gives the largest finite value, signed by both operands.
fn ee_div(n: f32, d: f32) -> f32 {
    sat(if d == 0.0 { f32::MAX.copysign(n) * d.signum() } else { n / d })
}

/// The vector unit's divide into Q.
fn vu_div(n: f32, d: f32) -> f32 {
    if d.to_bits() & 0x7f80_0000 == 0 {
        f32::from_bits(((n.to_bits() ^ d.to_bits()) & 0x8000_0000) | 0x7f7f_ffff)
    } else {
        sat(n / d)
    }
}

fn rf(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32::from_le_bytes(b[o..o + 4].try_into().unwrap()))
}
fn wf(b: &mut [u8], o: usize, v: f32) {
    b[o..o + 4].copy_from_slice(&v.to_bits().to_le_bytes());
}

/// Sum of the squares of x, y and z the way the routine adds them: `x*x + y*y`, then `+ z*z`.
fn sumsq(v: &[f32; 4]) -> f32 {
    va(va(vm(v[0], v[0]), vm(v[1], v[1])), vm(v[2], v[2]))
}

/// The three-component dot product the routine builds from a lane multiply and two broadcast adds.
fn dot3(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    va(va(vm(a[0], b[0]), vm(a[1], b[1])), vm(a[2], b[2]))
}

/// Scales x, y and z of `v` to length one with the vector unit's square root and divide; w is kept. Returns the vector and the
/// square root the routine saw (the length).
fn normalise(v: [f32; 4]) -> ([f32; 4], f32) {
    let len = sumsq(&v).abs().sqrt();
    let q = vu_div(1.0, va(0.0, len));
    ([vm(v[0], q), vm(v[1], q), vm(v[2], q), v[3]], len)
}

/// The outer product the routine builds from `vopmula` and `vopmsub`: `a x b` on x, y and z, w zero.
fn cross(a: &[f32; 4], b: &[f32; 4]) -> [f32; 4] {
    [
        msub(vm(a[1], b[2]), b[1], a[2]),
        msub(vm(a[2], b[0]), b[2], a[0]),
        msub(vm(a[0], b[1]), b[0], a[1]),
        0.0,
    ]
}

/// A row vector times a matrix: the accumulate chain row 0 * x, row 1 * y, row 2 * z, row 3 * w.
fn vec_mat(v: &[f32; 4], m: &Mat4) -> [f32; 4] {
    std::array::from_fn(|k| madd(madd(madd(vm(m[0][k], v[0]), m[1][k], v[1]), m[2][k], v[2]), m[3][k], v[3]))
}

/// `a * b` row by row: row `i` of the result is row `i` of `a` times `b`.
fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    std::array::from_fn(|i| vec_mat(&a[i], b))
}

/// What one call does besides computing: it initialised the static axis table and registered a destructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Built {
    /// True when this call took the first-use path (the original called 0x30fe80 with 0x135980).
    pub registered: bool,
}

/// The two matrices the routine writes through its third and fourth arguments.
pub type Pair = (Mat4, Mat4);

/// The projection of a box seen from a light. `statics` is the routine's static block (the guard byte at 0x3c3200
/// and the axis table at 0x3c3210, as `STATICS_SIZE` bytes from 0x3c3200); on the first use the table is filled with the unit
/// axes and the guard set. `ident` is the matrix at 0x354a60 (the identity the game stores there). `light` is the light position
/// (the first vector of the light block), `corners` the eight box corners (w 1.0) and `kind` the texture size class (0: 64,
/// 1: 128, 2: 256, otherwise 0). Returns the two matrices (third and fourth argument of the original) and whether the first-use
/// path ran.
///
/// orig 0x135028
pub fn project(statics: &mut [u8], ident: &Mat4, light: &[f32; 4], corners: &[[f32; 4]; 8], kind: i64) -> (Pair, Built) {
    let mut registered = false;
    if statics[0] == 0 {
        for (o, v) in [(0x14usize, 0.0f32), (0x18, 0.0), (0x1c, 0.0), (0x24, 1.0), (0x2c, 0.0), (0x28, 0.0), (0x30, 0.0), (0x10, 1.0), (0x3c, 0.0), (0x38, 1.0), (0x34, 0.0), (0x20, 0.0)] {
            wf(statics, o, v);
        }
        statics[0] = 1;
        registered = true;
    }
    // The centre of the box: the sum of the corners (in order) times 0.125.
    let mut centre = corners[0];
    for c in &corners[1..] {
        centre = std::array::from_fn(|k| va(centre[k], c[k]));
    }
    let centre: [f32; 4] = centre.map(|x| vm(x, 0.125));
    // The view direction: centre minus light, normalised.
    let d: [f32; 4] = std::array::from_fn(|k| vs(centre[k], light[k]));
    let (n, dist) = normalise(d);
    // The axis least aligned with it: the table address is advanced by the best offset, so only the first axis is ever tried.
    let table: [[f32; 4]; 3] = std::array::from_fn(|v| std::array::from_fn(|k| rf(statics, 0x10 + 16 * v + 4 * k)));
    let abs_dot = |v: &[f32; 4]| -> f32 {
        let x = dot3(&n, v);
        if x < 0.0 { -x } else { x }
    };
    let mut best = abs_dot(&table[0]);
    let mut at = 0usize;
    let mut t = 1usize;
    loop {
        let x = abs_dot(&table[at]);
        if x < best {
            best = x;
            at = t;
        }
        t += 1;
        if t >= 3 {
            break;
        }
    }
    let axis = table[at];
    let v1 = normalise(cross(&axis, &n)).0;
    let v2 = normalise(cross(&n, &v1)).0;
    // The view matrix: the three basis vectors as columns, the light position moved to the translation row.
    let row3: [f32; 4] = [va(ident[3][0], light[0]), va(ident[3][1], light[1]), va(ident[3][2], light[2]), ident[3][3]];
    let basis = [v1, v2, n];
    let cols: [[f32; 4]; 3] = std::array::from_fn(|c| [basis[0][c], basis[1][c], basis[2][c], 0.0]);
    let mut tr = [0f32; 4];
    for (k, o) in tr.iter_mut().enumerate().take(3) {
        *o = vs(0.0, madd(madd(vm(cols[0][k], row3[0]), cols[1][k], row3[1]), cols[2][k], row3[2]));
    }
    tr[3] = row3[3];
    let view: Mat4 = [cols[0], cols[1], cols[2], tr];
    // The largest tangent of the box seen through the view: |x * dist / z| and |y * dist / z| over the corners.
    let mut max_x = 0f32;
    let mut max_y = 0f32;
    for c in corners {
        let s = vec_mat(c, &view);
        let a = ee_div(vm(s[0], dist), s[2]);
        let b = ee_div(vm(s[1], dist), s[2]);
        let a = if a < 0.0 { -a } else { a };
        let b = if b < 0.0 { -b } else { b };
        if max_x < a {
            max_x = a;
        }
        if max_y < b {
            max_y = b;
        }
    }
    // The perspective stage: the distance on the diagonal, w from z.
    let mut p = *ident;
    p[0][0] = dist;
    p[1][1] = dist;
    p[2][2] = 0.0;
    p[2][3] = 1.0;
    p[3][2] = 1.0;
    p[3][3] = 0.0;
    let diag: [f32; 4] = std::array::from_fn(|k| vs(corners[6][k], corners[0][k]));
    let far = va(dist, sumsq(&diag).abs().sqrt());
    let size = match kind {
        1 => 0x80,
        0 => 0x40,
        2 => 0x100,
        _ => 0,
    } as f32;
    let (two_x, two_y) = (va(max_x, max_x), va(max_y, max_y));
    let (half_x, half_y) = (vs(size, 2.0), vs(size, 2.0));
    let denom = vs(far, 1.0);
    let f21 = ee_div(va(far, -16770000.0), denom);
    let f20 = ee_div(vm(far, 16769999.0), denom);
    let finish = |sx: f32, sy: f32, bias: f32| -> Mat4 {
        let mut q = *ident;
        q[0][0] = sx;
        q[1][1] = sy;
        q[2][2] = f20;
        q[3][0] = bias;
        q[3][1] = bias;
        q[3][2] = f21;
        mat_mul(&view, &mat_mul(&p, &q))
    };
    let first = finish(ee_div(half_x, two_x), ee_div(half_y, two_y), 2048.0);
    let second = finish(ee_div(half_x, vm(two_x, size)), ee_div(half_y, vm(two_y, size)), 0.5);
    ((first, second), Built { registered })
}
