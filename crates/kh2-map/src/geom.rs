//! Boxes and vector helpers of the collision unit (ledger clusters L1:coct and L1:geom, spec
//! `docs/systems/core-runtime/collision.md` section 9.6).
//!
//! The game computes with the VU0 macro unit: a "vector" is four floats (x, y, z, w) and most
//! operations run on all four lanes, so the w lane is kept wherever the original stores it. The
//! dot product idiom of the original is `vmul` followed by two broadcast adds: `(x*x' + y*y') + z*z'`
//! in that order; the helpers below keep that association (float addition is not associative).
//! Boxes come in two forms: the float box (min xyz then max xyz) and the quantized box of six
//! 16-bit integers that the map tree stores (`FUN_00144d68`).

/// A vector register: x, y, z, w.
pub type V4 = [f32; 4];
/// A float box: min x, y, z, then max x, y, z.
pub type FBox = [f32; 6];
/// A quantized box in the same order.
pub type QBox = [i16; 6];

/// `vsub` on all four lanes.
pub fn sub(a: V4, b: V4) -> V4 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2], a[3] - b[3]]
}

/// `vadd` on all four lanes.
pub fn add(a: V4, b: V4) -> V4 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
}

/// `vmulbc`: every lane times one scalar.
pub fn scale(a: V4, s: f32) -> V4 {
    [a[0] * s, a[1] * s, a[2] * s, a[3] * s]
}

/// `vmulbc` with the destination mask xyz: the three direction lanes times a scalar, w left as it was (the sphere
/// solver's multiplies use this form, so a plane keeps its distance in w).
pub fn mul_xyz(a: V4, s: f32) -> V4 {
    [a[0] * s, a[1] * s, a[2] * s, a[3]]
}

/// `normalize4` with the divide applied to the three direction lanes only (`vmulq` with the mask xyz): w is kept.
pub fn normalize_xyz(v: V4) -> V4 {
    let len = dot3(v, v).sqrt();
    let q = vu_div(1.0, len);
    mul_xyz(v, q)
}

/// The dot of the first three lanes in the original's order: `(x*x' + y*y') + z*z'`.
pub fn dot3(a: V4, b: V4) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}

/// `vopmula` followed by `vopmsub`: the cross product `q x e` of the first three lanes (the w lane is not
/// written by the pair and is returned as 0).
pub fn cross_qe(q: V4, e: V4) -> V4 {
    [q[1] * e[2] - e[1] * q[2], q[2] * e[0] - e[2] * q[0], q[0] * e[1] - e[0] * q[1], 0.0]
}

/// `vnorm` as the original builds it: `v * (1 / sqrt(dot3(v, v)))` on all four lanes, the reciprocal being
/// the VU divide with the zero divisor clamped to the largest finite float.
pub fn normalize4(v: V4) -> V4 {
    let len = dot3(v, v).sqrt();
    let q = vu_div(1.0, len);
    scale(v, q)
}

/// A 4x4 matrix as four row vectors (the placement of a collision object, `oct +0x50` and its inverse `+0x90`).
pub type M4 = [V4; 4];

/// The identity matrix.
pub const M4_IDENTITY: M4 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// A point times a matrix as the original builds it: `vmulabc` with the x lane, `vmaddabc` with y and z, `vmaddbc` with
/// w, i.e. `((row0 * x + row1 * y) + row2 * z) + row3 * w` lane by lane.
pub fn xf(m: &M4, v: V4) -> V4 {
    let mut a = scale(m[0], v[0]);
    a = add(a, scale(m[1], v[1]));
    a = add(a, scale(m[2], v[2]));
    add(a, scale(m[3], v[3]))
}

/// The VU divide: an infinite quotient reads as the largest finite float of its sign (no NaN exists).
pub fn vu_div(n: f32, d: f32) -> f32 {
    let q = n / d;
    if q.is_nan() {
        return 0.0;
    }
    q.clamp(f32::from_bits(0xff7f_ffff), f32::from_bits(0x7f7f_ffff))
}

/// orig 0x1443a8: a box from six floats.
pub fn fbox(x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32) -> FBox {
    [x0, y0, z0, x1, y1, z1]
}

/// orig 0x1446f8: the box of two corners in either order (per axis the smaller first; the second corner
/// on a tie).
pub fn fbox_corners(a: [f32; 3], b: [f32; 3]) -> FBox {
    let mut o = [0.0; 6];
    for k in 0..3 {
        if a[k] < b[k] {
            o[k] = a[k];
            o[k + 3] = b[k];
        } else {
            o[k] = b[k];
            o[k + 3] = a[k];
        }
    }
    o
}

/// orig 0x144778: the union of two float boxes (the second box wins ties).
pub fn fbox_union(a: &FBox, b: &FBox) -> FBox {
    let mut o = [0.0; 6];
    for k in 0..3 {
        o[k] = if a[k] < b[k] { a[k] } else { b[k] };
        o[k + 3] = if b[k + 3] < a[k + 3] { a[k + 3] } else { b[k + 3] };
    }
    o
}

/// orig 0x144c28: do two float boxes overlap (closed intervals on all three axes)?
pub fn fbox_overlap(a: &FBox, b: &FBox) -> bool {
    b[0] <= a[3] && a[0] <= b[3] && b[1] <= a[4] && a[1] <= b[4] && b[2] <= a[5] && a[2] <= b[5]
}

/// orig 0x144548: the bounds of a polygon's first `n` (3 or 4) vertices. The three-vertex form and the
/// four-vertex form of the original are the same min/max walk.
pub fn fbox_of_polygon(v: &[V4; 4], n: usize) -> FBox {
    let mut o = [0.0; 6];
    for k in 0..3 {
        let (mut lo, mut hi) = (v[0][k], v[0][k]);
        for p in v.iter().take(n.min(4)).skip(1) {
            let f = p[k];
            if f < lo {
                lo = f;
            } else if hi < f {
                hi = f;
            }
        }
        o[k] = lo;
        o[k + 3] = hi;
    }
    o
}

/// The per-level offsets and scales of the quantized form (the executable's two eight-float tables at
/// 0x355e60 and 0x355e70; levels 0 to 3 are the ones a collision file's header word +0xc takes).
const LEVEL_OFFSET: [f32; 4] = [1.0, 10.0, 20.0, 100.0];
const LEVEL_SCALE: [f32; 4] = [1.0, f32::from_bits(0x3dcc_cccc), f32::from_bits(0x3d4c_cccc), f32::from_bits(0x3c23_d70a)];

/// One coordinate of `fbox_to_qbox`. `upper` selects the max-side clamp; the original clamps each side
/// only on its own end (a value past the other end wraps when truncated to 16 bits).
fn quantize(v: f32, level: usize, upper: bool) -> i16 {
    let v = if level != 0 { v * LEVEL_SCALE[level] } else { v };
    let r = if upper {
        if v >= 32768.0 { 32767.0 } else { v }
    } else if v <= -32768.0 {
        -32767.0
    } else {
        v
    };
    (r as i32) as i16
}

/// orig 0x144d68: the float box to the quantized box at `level` (0 to 3): the minimum corner loses the
/// level offset, the maximum corner gains it, both are scaled (level 0 is unscaled) and clamped.
pub fn fbox_to_qbox(b: &FBox, level: usize) -> QBox {
    let off = LEVEL_OFFSET[level];
    let mut o = [0i16; 6];
    for k in 0..3 {
        let lo = b[k] - off;
        let hi = b[k + 3] + off;
        o[k] = quantize(lo, level, false);
        o[k + 3] = quantize(hi, level, true);
    }
    o
}

/// 0x378fdc and 0x378fe4: the w lane of the box centre and of the corner list (1.0).
const BOX_W: f32 = 1.0;
/// 0x378fe0: the half factor of the box centre (0.5).
const HALF: f32 = 0.5;
/// 0x378fe8: the w lane of the negated corner list (1.0).
const BOX_NEG_W: f32 = 1.0;

/// orig 0x144368: the empty box (all six floats zero).
pub fn fbox_clear() -> FBox {
    [0.0; 6]
}

/// orig 0x1443e0: the box constructor taking two corners; it forwards its arguments to `fbox_corners` (0x1446f8).
pub fn fbox_new(a: [f32; 3], b: [f32; 3]) -> FBox {
    fbox_corners(a, b)
}

/// orig 0x144410: the box union wrapper; it forwards its arguments to `fbox_union` (0x144778).
pub fn fbox_merge(a: &FBox, b: &FBox) -> FBox {
    fbox_union(a, b)
}

/// orig 0x144500: the polygon bounds wrapper; it forwards its arguments to `fbox_of_polygon` (0x144548).
pub fn fbox_around(v: &[V4; 4], n: usize) -> FBox {
    fbox_of_polygon(v, n)
}

/// orig 0x144530: the box of two points stored one after the other (`fbox_corners` of the two).
pub fn fbox_of_points(p: &[V4; 2]) -> FBox {
    fbox_corners([p[0][0], p[0][1], p[0][2]], [p[1][0], p[1][1], p[1][2]])
}

/// orig 0x1446d8: the box set from six floats (the same store as `fbox`, 0x1443a8).
pub fn fbox_set(x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32) -> FBox {
    fbox(x0, y0, z0, x1, y1, z1)
}

/// orig 0x144820: the centre of a box, `min + (max - min) * 0.5` per axis (`vmulbc` by the half factor), with the
/// w lane the constant 1.0.
pub fn fbox_centre(b: &FBox) -> V4 {
    let d = [b[3] - b[0], b[4] - b[1], b[5] - b[2]];
    [b[0] + d[0] * HALF, b[1] + d[1] * HALF, b[2] + d[2] * HALF, BOX_W]
}

/// orig 0x144a50: the eight corners of a box as vectors (w lane 1.0): the low z face walking (min x, min y),
/// (min x, max y), (max x, max y), (max x, min y), then the high z face in the same order.
pub fn fbox_corners8(b: &FBox) -> [V4; 8] {
    let (x0, y0, z0, x1, y1, z1) = (b[0], b[1], b[2], b[3], b[4], b[5]);
    let w = BOX_W;
    [
        [x0, y0, z0, w],
        [x0, y1, z0, w],
        [x1, y1, z0, w],
        [x1, y0, z0, w],
        [x0, y0, z1, w],
        [x0, y1, z1, w],
        [x1, y1, z1, w],
        [x1, y0, z1, w],
    ]
}

/// orig 0x144af8: the same eight corners in the negated form the original writes next to them: x as is, and for y
/// and z the negated value of the opposite end (the first corner is `(min x, -max y, -max z)`; the corners then walk
/// like `fbox_corners8` over (-y, -z)); w lane 1.0 (0x378fe8).
pub fn fbox_corners8_negated(b: &FBox) -> [V4; 8] {
    let w = BOX_NEG_W;
    [
        [b[0], -b[4], -b[5], w],
        [b[0], -b[1], -b[5], w],
        [b[3], -b[1], -b[5], w],
        [b[3], -b[4], -b[5], w],
        [b[0], -b[4], -b[2], w],
        [b[0], -b[1], -b[2], w],
        [b[3], -b[1], -b[2], w],
        [b[3], -b[4], -b[2], w],
    ]
}

/// orig 0x1448c0: the box of the eight corners of `b` carried through the matrix `m` (the corners of
/// `fbox_corners8`, each through `xf`): the per-axis minimum and maximum of the results, built with strict
/// comparisons from the first corner, then handed to `fbox_corners`.
pub fn fbox_transform(b: &FBox, m: &M4) -> FBox {
    let mut lo = [0.0f32; 3];
    let mut hi = [0.0f32; 3];
    for (i, c) in fbox_corners8(b).iter().enumerate() {
        let t = xf(m, *c);
        for k in 0..3 {
            if i == 0 {
                lo[k] = t[k];
                hi[k] = t[k];
            } else if t[k] < lo[k] {
                lo[k] = t[k];
            } else if hi[k] < t[k] {
                hi[k] = t[k];
            }
        }
    }
    fbox_corners(lo, hi)
}

/// orig 0x144ca8: does box `a` contain box `b` (closed intervals on all three axes)?
pub fn fbox_contains(a: &FBox, b: &FBox) -> bool {
    a[0] <= b[0] && b[3] <= a[3] && a[1] <= b[1] && b[4] <= a[4] && a[2] <= b[2] && b[5] <= a[5]
}

/// orig 0x144d48: the empty quantized box (six zero shorts).
pub fn qbox_clear() -> QBox {
    [0; 6]
}

/// orig 0x145000: the quantized box back to floats at `level` (0 to 3): the shorts as they are at level 0, else
/// multiplied by the level's entry of the table at 0x355e60 (the same table `fbox_to_qbox` subtracts and adds).
pub fn qbox_to_fbox(q: &QBox, level: usize) -> FBox {
    if level == 0 {
        return q.map(|v| v as f32);
    }
    let s = LEVEL_OFFSET[level];
    q.map(|v| v as f32 * s)
}

/// orig 0x145498: do two quantized boxes overlap (closed intervals)?
pub fn qbox_overlap(a: &QBox, b: &QBox) -> bool {
    b[0] <= a[3] && a[0] <= b[3] && b[1] <= a[4] && a[1] <= b[4] && b[2] <= a[5] && a[2] <= b[5]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_keeps_the_original_association() {
        // (1e8 + 1) - 1e8 loses the 1 in f32, so the order is observable.
        let a = [1e8, 1.0, -1e8, 0.0];
        let b = [1.0, 1.0, 1.0, 0.0];
        assert_eq!(dot3(a, b), 0.0);
    }

    #[test]
    fn quantized_boxes_clamp_each_end() {
        let q = fbox_to_qbox(&[-1.0e6, 0.0, 0.0, 1.0e6, 10.0, 10.0], 0);
        assert_eq!((q[0], q[3]), (-32767, 32767));
        assert_eq!((q[1], q[4]), (-1, 11));
    }

    #[test]
    fn overlap_is_closed() {
        assert!(qbox_overlap(&[0, 0, 0, 5, 5, 5], &[5, 5, 5, 9, 9, 9]));
        assert!(!qbox_overlap(&[0, 0, 0, 5, 5, 5], &[6, 0, 0, 9, 9, 9]));
        assert!(fbox_overlap(&[0.0; 6], &[0.0; 6]));
    }
}
