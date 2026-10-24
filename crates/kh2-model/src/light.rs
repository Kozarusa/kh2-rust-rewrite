//! Model lighting (render-model.md d1, L1-P26a): the matrix inverse helpers, the light block, and the two routines that turn
//! three lights and a model matrix into the light matrices a model draw uses. Pure float rules on plain arrays; nothing here
//! reads the game's memory except the global light block helpers, which work on a [`Mem`] image.
//!
//! The port is not wired into the renderer yet: every material is drawn unlit today (DEVIATIONS 2026-10-24). Wiring takes the
//! per-model call of the light matrices (the caller of 0x134960 / 0x134bd8 in the object draw, L1-P25b) and a shader that
//! multiplies the vertex normal by the three direction columns and adds the colour rows.
//!
//! Float rules follow kh2-ee (the oracle): single precision, round to nearest, results that overflow saturate to the largest
//! finite value, a divide by zero gives the largest finite value with the sign of the operands. The vector unit's divide (Q
//! register) saturates the same way. Light matrix routines use the vector unit for the cross products.

use crate::texanim::objects::Mem;

/// A 4x4 matrix as four row vectors (the layout of the original: row `i` at `+0x10 * i`).
pub type Mat4 = [[f32; 4]; 4];

/// The global light block (see [`global_light_block`]) sits at this address; its first vector is the light position, the
/// second its colour, the third the vector 0x135880 stores and 0x1358a0 reads.
pub const GLOBAL_LIGHT: u32 = 0x354e30;
/// The third vector of the global light block (0x354e30 + 0x20); the lighting routines copy it into the last row of the
/// light matrices. [inferred: an ambient colour; the routines only copy it]
pub const GLOBAL_AMBIENT: u32 = 0x354e50;

const MAX: f32 = f32::MAX;

/// The EE's float unit has no infinities: an overflowing result is the largest finite value of its sign.
pub(crate) fn sat(f: f32) -> f32 {
    if f.is_nan() {
        MAX
    } else if f.is_infinite() {
        MAX.copysign(f)
    } else {
        f
    }
}

/// Single precision divide of the EE FPU (`div.s`): a zero divisor gives the largest finite value, signed by both operands.
fn ee_div(n: f32, d: f32) -> f32 {
    sat(if d == 0.0 { MAX.copysign(n) * d.signum() } else { n / d })
}

/// The vector unit's divide into Q: a zero (or denormal) divisor gives the largest finite value with the sign of the xor of
/// both operands, otherwise the saturated quotient.
fn vu_div(n: f32, d: f32) -> f32 {
    if d.to_bits() & 0x7f80_0000 == 0 {
        f32::from_bits(((n.to_bits() ^ d.to_bits()) & 0x8000_0000) | 0x7f7f_ffff)
    } else {
        sat(n / d)
    }
}

/// orig 0x11bb28: the determinant of a 3x3 matrix given as nine floats (row major).
pub fn det3(m: &[f32; 9]) -> f32 {
    m[2] * (m[3] * m[7] - m[4] * m[6]) + (m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6]))
}

/// orig 0x11bb78: the cofactor of element (`row`, `col`) of a 4x4 matrix: the determinant of the 3x3 left after removing that
/// row and column, negated when `row + col` is odd. `row` and `col` are 0..=3.
pub fn cofactor(m: &Mat4, row: usize, col: usize) -> f32 {
    let mut sub = [0f32; 9];
    let mut k = 0;
    for (i, r) in m.iter().enumerate() {
        if i == row {
            continue;
        }
        for (j, v) in r.iter().enumerate() {
            if j != col {
                sub[k] = *v;
                k += 1;
            }
        }
    }
    let d = det3(&sub);
    if (row + col) & 1 == 0 { d } else { -d }
}

/// orig 0x11bc08: inverts a 4x4 matrix in place (adjugate over the determinant, expanded along row 0) and returns the
/// determinant. A zero determinant scales by the largest finite value, as the original's float divide does.
pub fn invert(m: &mut Mat4) -> f32 {
    let mut adj = [[0f32; 4]; 4];
    for (col, a) in adj.iter_mut().enumerate() {
        for (row, v) in a.iter_mut().enumerate() {
            *v = cofactor(m, row, col);
        }
    }
    let mut det = 0f32;
    for (k, a) in adj.iter().enumerate() {
        det += a[0] * m[0][k];
    }
    *m = adj;
    let s = ee_div(1.0, det);
    for row in m.iter_mut() {
        for v in row.iter_mut() {
            *v = sat(*v * s);
        }
    }
    det
}

// ---- the light block --------------------------------------------------------------------------------------------------

/// A light block: three vectors at `+0x00` (position), `+0x10` (colour) and `+0x20` (the vector 0x135880 sets on the global
/// block). The initialiser writes only the first two.
pub type LightBlock = [[f32; 4]; 3];

/// orig 0x134f40: initialises a light block: position (0, -1e7, 0, 1) and colour (1, 1, 1, 1). Writes eight words; the third
/// vector is left alone. [inferred: the position is a far point above the scene, y pointing down]
pub fn light_init(b: &mut LightBlock) {
    b[0] = [0.0, -10_000_000.0, 0.0, 1.0];
    b[1] = [1.0; 4];
}

/// orig 0x134f88: an empty function (called after a light block is used, a destructor with nothing to do).
pub fn light_done(_b: &mut LightBlock) {}

/// orig 0x134f90: copies a vector (16 bytes) from `src` to `dst`.
pub fn vec_copy(dst: &mut [f32; 4], src: &[f32; 4]) {
    *dst = *src;
}

/// orig 0x134fa8: copies the vector at the first argument to the second (the reverse argument order of 0x134f90): the light
/// block's position (the vector at `+0`) into `out`.
pub fn light_position(light: &LightBlock, out: &mut [f32; 4]) {
    *out = light[0];
}

/// orig 0x134fc0: stores a colour vector at `+0x10` of a light block.
pub fn light_set_colour(b: &mut LightBlock, c: &[f32; 4]) {
    b[1] = *c;
}

/// orig 0x134fe0: reads the colour vector at `+0x10` of a light block.
pub fn light_colour(b: &LightBlock, out: &mut [f32; 4]) {
    *out = b[1];
}

/// orig 0x135870: the address of the global light block.
pub fn global_light_block() -> u32 {
    GLOBAL_LIGHT
}

/// orig 0x135880: stores a vector in the global light block's third slot (0x354e50).
pub fn set_global_ambient(m: &mut Mem, v: &[f32; 4]) {
    for (i, x) in v.iter().enumerate() {
        m.wf32(GLOBAL_AMBIENT + 4 * i as u32, *x);
    }
}

/// orig 0x1358a0: reads the vector in the global light block's third slot (0x354e50).
pub fn global_ambient(m: &Mem) -> [f32; 4] {
    std::array::from_fn(|i| m.rf32(GLOBAL_AMBIENT + 4 * i as u32))
}

// ---- the light matrices -----------------------------------------------------------------------------------------------

/// The result of the light matrix routines: seven vectors (0x70 bytes). Rows 0 to 2 hold the three light directions as
/// columns (row `r`, column `k` is component `r` of light `k`; the fourth column is zero), rows 3 to 5 the three colours
/// (`w` as given, see each routine) and row 6 the ambient vector.
pub type LightMatrices = [[f32; 4]; 7];

/// One vector unit multiply, add and subtract: the result saturates.
fn vm(a: f32, b: f32) -> f32 {
    sat(a * b)
}
fn va(a: f32, b: f32) -> f32 {
    sat(a + b)
}
fn vs(a: f32, b: f32) -> f32 {
    sat(a - b)
}
/// A multiply-add into the accumulator: one saturation on the whole result, so a product that overflows on its own still
/// meets the accumulator before it saturates.
pub(crate) fn madd(acc: f32, a: f32, b: f32) -> f32 {
    sat(acc + a * b)
}
fn msub(acc: f32, a: f32, b: f32) -> f32 {
    sat(acc - a * b)
}

/// The vector unit's inverse of the model matrix as used by both routines: the cross products of the three rotation rows
/// give the adjugate, the determinant `row0 . (row1 x row2)` sums x, then y, then z, Q is its reciprocal, and the
/// translation row is `-(row3 * adjugate)` with the third component summed in the order z + x, then + y.
fn vu_inverse(m: &Mat4) -> Mat4 {
    // OPMULA/OPMSUB: (a x b) component by component, ACC first.
    let cross = |a: &[f32; 4], b: &[f32; 4]| -> [f32; 4] {
        [msub(vm(a[1], b[2]), b[1], a[2]), msub(vm(a[2], b[0]), b[2], a[0]), msub(vm(a[0], b[1]), b[0], a[1]), 0.0]
    };
    let c0 = cross(&m[1], &m[2]);
    let c1 = cross(&m[2], &m[0]);
    let c2 = cross(&m[0], &m[1]);
    let det = va(va(vm(m[0][0], c0[0]), vm(m[0][1], c0[1])), vm(m[0][2], c0[2]));
    let q = vu_div(1.0, det);
    // Transposed: row r of the inverse is (c0[r], c1[r], c2[r], 0).
    let col = |r: usize| [c0[r], c1[r], c2[r], 0.0];
    let t = &m[3];
    let tx = va(va(vm(t[0], c0[0]), vm(t[1], c0[1])), vm(t[2], c0[2]));
    let ty = va(va(vm(t[1], c1[1]), vm(t[0], c1[0])), vm(t[2], c1[2]));
    let tz = va(va(vm(t[2], c2[2]), vm(t[0], c2[0])), vm(t[1], c2[1]));
    let sc = |v: [f32; 4]| v.map(|x| vm(x, q));
    [sc(col(0)), sc(col(1)), sc(col(2)), [vm(vs(0.0, tx), q), vm(vs(0.0, ty), q), vm(vs(0.0, tz), q), m[3][3]]]
}

/// Takes a light direction into model space with the inverse above (rotation rows by x, y, z, plus the translation row), flips
/// the y and z components and normalises it. The length is the vector unit's square root, the reciprocal its divide.
fn model_space_direction(inv: &Mat4, d: &[f32; 4]) -> [f32; 3] {
    let mut v = [0f32; 3];
    for (i, o) in v.iter_mut().enumerate() {
        *o = madd(madd(madd(vm(inv[0][i], d[0]), inv[1][i], d[1]), inv[2][i], d[2]), inv[3][i], 1.0);
    }
    v[1] = -v[1];
    v[2] = -v[2];
    let len2 = va(va(vm(v[0], v[0]), vm(v[1], v[1])), vm(v[2], v[2]));
    let len = len2.abs().sqrt();
    let q = vu_div(1.0, len);
    v.map(|x| vm(x, q))
}

/// orig 0x134960: the light matrices of a model from its matrix `m`, up to three light blocks (`None` leaves that light's
/// direction column and colour row zero) and the global ambient vector (0x354e50, read last through 0x1358a0).
pub fn light_matrices(m: &Mat4, lights: [Option<&LightBlock>; 3], ambient: &[f32; 4]) -> LightMatrices {
    let mut out = [[0f32; 4]; 7];
    let inv = vu_inverse(m);
    for (k, l) in lights.iter().enumerate() {
        let Some(l) = l else { continue };
        let d = model_space_direction(&inv, &l[0]);
        for (r, v) in d.iter().enumerate() {
            out[r][k] = *v;
        }
        out[3 + k] = l[1];
    }
    out[6] = *ambient;
    out
}

/// orig 0x134bd8: the same light matrices from a packed array: three directions of three floats (`+0x00`), three colours of
/// three floats (`+0x24`) and the ambient colour of three floats (`+0x48`). The `w` of each direction fed to the transform and
/// of each colour and the ambient row is the constant 1.0 (0x378cc4), where 0x134960 takes `w` from the light blocks.
pub fn light_matrices_packed(m: &Mat4, packed: &[f32; 21]) -> LightMatrices {
    const W: f32 = 1.0;
    let mut out = [[0f32; 4]; 7];
    let inv = vu_inverse(m);
    for k in 0..3 {
        let d = [packed[3 * k], packed[3 * k + 1], packed[3 * k + 2], W];
        let v = model_space_direction(&inv, &d);
        for (r, x) in v.iter().enumerate() {
            out[r][k] = *x;
        }
        out[3 + k] = [packed[9 + 3 * k], packed[9 + 3 * k + 1], packed[9 + 3 * k + 2], W];
    }
    out[6] = [packed[18], packed[19], packed[20], W];
    out
}
