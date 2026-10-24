//! Frustum, plane and box tests of the draw unit (WP-L1-P25a, spec `docs/systems/core-runtime/render-model.md` c2).
//!
//! The game does this arithmetic on the VU0 macro unit and the EE FPU, which have no infinities and no NaN: a result
//! that overflows reads as the largest finite float of its sign ([`vu`]). A "vector" is four floats, a matrix four
//! rows; the point times matrix product is `((row0 * x + row1 * y) + row2 * z) + row3 * w` (`vmulabc`, `vmaddabc`,
//! `vmaddbc`), the dot product of the box and plane tests is `(x*x' + y*y') + z*z'` (`vmul` and two broadcast adds).
//! Each add and multiply rounds to f32 on its own, as the emulator and the hardware do.
//!
//! Boxes are six floats: min x, y, z, then max x, y, z. The clip matrix of the visibility tests carries a point into
//! clip space; `VCLIP` of the product against `|w|` gives one bit per side.

/// A vector register: x, y, z, w.
pub type V4 = [f32; 4];
/// A 4x4 matrix as four row vectors.
pub type M4 = [V4; 4];
/// A float box: min xyz then max xyz.
pub type FBox = [f32; 6];
/// A plane: normal xyz and the offset w, so the signed distance of p is `n . p + w`.
pub type Plane = V4;

/// The result of the box test against a clip matrix: all eight corners outside one side of the view volume.
pub const CLIP_OUTSIDE: u32 = 0;
/// All eight corners inside the view volume.
pub const CLIP_INSIDE: u32 = 1;
/// The box crosses at least one side.
pub const CLIP_CROSSES: u32 = 2;

/// The VU and FPU result rule: no infinity or NaN, an overflow reads as the largest finite float of its sign.
pub fn vu(x: f32) -> f32 {
    if x.is_nan() {
        f32::MAX
    } else if x.is_infinite() {
        f32::MAX.copysign(x)
    } else {
        x
    }
}

/// `acc + a * b` as the accumulate form rounds it (the product to f32, the sum to f32, one clamp at the end).
fn madd(acc: f32, a: f32, b: f32) -> f32 {
    vu(acc + a * b)
}

/// A point times a matrix, the original's `vmulabc`/`vmaddabc`/`vmaddbc` chain on all four lanes.
pub fn transform(m: &M4, v: V4) -> V4 {
    let mut r = [0.0; 4];
    for (k, out) in r.iter_mut().enumerate() {
        let mut a = vu(m[0][k] * v[0]);
        a = madd(a, m[1][k], v[1]);
        a = madd(a, m[2][k], v[2]);
        *out = madd(a, m[3][k], v[3]);
    }
    r
}

/// The product the per-frame matrix set-up builds: row `k` of the result is `transform(a, b[k])` (the left operand
/// is the matrix applied to each row vector of the right one).
pub fn matrix_product(a: &M4, b: &M4) -> M4 {
    [transform(a, b[0]), transform(a, b[1]), transform(a, b[2]), transform(a, b[3])]
}

/// The dot product of the first three lanes: `vmul`, then the y and z lanes added to x one after the other.
pub fn dot3(a: V4, b: V4) -> f32 {
    let p = [vu(a[0] * b[0]), vu(a[1] * b[1]), vu(a[2] * b[2])];
    vu(vu(p[0] + p[1]) + p[2])
}

/// The eight corners of a box (w lane 1.0), the order of `0x144a50`: the low z face walking (min x, min y), (min x,
/// max y), (max x, max y), (max x, min y), then the high z face the same way. The collision unit's box helpers hold
/// the same routine (`kh2_map::geom::fbox_corners8`); it is repeated here because that crate sits above this one.
fn box_corners(b: &FBox) -> [V4; 8] {
    let (x0, y0, z0, x1, y1, z1) = (b[0], b[1], b[2], b[3], b[4], b[5]);
    [
        [x0, y0, z0, 1.0],
        [x0, y1, z0, 1.0],
        [x1, y1, z0, 1.0],
        [x1, y0, z0, 1.0],
        [x0, y0, z1, 1.0],
        [x0, y1, z1, 1.0],
        [x1, y1, z1, 1.0],
        [x1, y0, z1, 1.0],
    ]
}

/// The six clip bits of `VCLIP` for a vector `q` against `|q.w|`: bit 0 x above, bit 1 x below, bits 2 and 3 for y,
/// bits 4 and 5 for z (above is `>`, below is `<`, both strict).
pub fn clip_bits(q: V4) -> u32 {
    let w = q[3].abs();
    let mut bits = 0;
    for k in 0..3 {
        if q[k] > w {
            bits |= 1 << (2 * k);
        }
        if q[k] < -w {
            bits |= 2 << (2 * k);
        }
    }
    bits
}

/// orig 0x146598: the 64-byte matrix copy (the original skips it when source and destination are the same block).
pub fn matrix_copy(src: &M4) -> M4 {
    *src
}

/// orig 0x1465c8: the clip-code test of eight points against a clip matrix. Each point is carried through `m` and
/// its six clip bits taken; `and` starts at 0x3f and keeps the bits all points share, `or` collects every bit. After
/// each point: `and == 0 && or != 0` returns [`CLIP_CROSSES`] at once (two points are outside different sides, or
/// one is outside and one is not); after the eighth point [`CLIP_INSIDE`] when `and == 0` (no bit set in some point
/// and none shared: every corner inside), else [`CLIP_OUTSIDE`].
pub fn clip_points(m: &M4, points: &[V4; 8]) -> u32 {
    let (mut and, mut or) = (0x3fu32, 0u32);
    for (i, p) in points.iter().enumerate() {
        let bits = clip_bits(transform(m, *p)) & 0x3f;
        and &= bits;
        or |= bits;
        if and == 0 && or != 0 {
            return CLIP_CROSSES;
        }
        if i == 7 {
            return (and == 0) as u32;
        }
    }
    unreachable!()
}

/// orig 0x146648: the clip-code test of a float box (its eight corners from `0x144a50`, then [`clip_points`]).
pub fn clip_box(m: &M4, b: &FBox) -> u32 {
    clip_points(m, &box_corners(b))
}

/// orig 0x163070: the plane through three points. The normal is `(b - a) x (c - a)` scaled by `1 / sqrt(n . n)` (the
/// VU divide of 1.0, a zero length reads as the largest finite float), `w` is `-(n . a)`.
pub fn plane_from_points(a: V4, b: V4, c: V4) -> Plane {
    let e1: V4 = std::array::from_fn(|k| vu(b[k] - a[k]));
    let e2: V4 = std::array::from_fn(|k| vu(c[k] - a[k]));
    let n = [
        vu(vu(e1[1] * e2[2]) - e2[1] * e1[2]),
        vu(vu(e1[2] * e2[0]) - e2[2] * e1[0]),
        vu(vu(e1[0] * e2[1]) - e2[0] * e1[1]),
    ];
    let len = vu(vu(vu(n[0] * n[0]) + vu(n[1] * n[1])) + vu(n[2] * n[2])).abs().sqrt();
    let q = vu_divide(1.0, len);
    let n = [vu(n[0] * q), vu(n[1] * q), vu(n[2] * q)];
    let d = dot3([n[0], n[1], n[2], 0.0], a);
    [n[0], n[1], n[2], -d]
}

/// The VU divide (`VDIV`): a divisor with a zero exponent (zero or denormal) gives the largest finite float, signed
/// like the quotient would be; any other quotient is clamped like every VU result.
pub fn vu_divide(n: f32, d: f32) -> f32 {
    if d.to_bits() & 0x7f80_0000 == 0 {
        return f32::from_bits(((n.to_bits() ^ d.to_bits()) & 0x8000_0000) | 0x7f7f_ffff);
    }
    vu(n / d)
}

/// orig 0x1469c8: the five planes of an occluder volume from the eye and the four corners of a quad. Plane 0 is the
/// quad (c0, c1, c2); planes 1 to 4 pass through the eye and the quad's edges: (eye, c2, c3), (eye, c0, c1), (eye,
/// c3, c0), (eye, c1, c2).
pub fn occluder_planes(eye: V4, c: &[V4; 4]) -> [Plane; 5] {
    [
        plane_from_points(c[0], c[1], c[2]),
        plane_from_points(eye, c[2], c[3]),
        plane_from_points(eye, c[0], c[1]),
        plane_from_points(eye, c[3], c[0]),
        plane_from_points(eye, c[1], c[2]),
    ]
}

/// The sign bit of a float as the original's integer test `value < 0` reads it (so -0.0 counts as negative).
fn sign_set(f: f32) -> bool {
    (f.to_bits() as i32) < 0
}

/// orig 0x146b20: is the box on the negative side of all five planes? Per plane the corner that lies furthest along
/// the normal is chosen (the box's max on an axis whose normal component has its sign bit clear, the min otherwise);
/// when that corner's `n . p + w` is zero or positive the box is not inside and the answer is false. All five
/// planes passed: true (the box is inside the volume, which faces outward [inferred]).
pub fn box_inside_planes(planes: &[Plane; 5], b: &FBox) -> bool {
    for pl in planes {
        let p = [
            if sign_set(pl[0]) { b[0] } else { b[3] },
            if sign_set(pl[1]) { b[1] } else { b[4] },
            if sign_set(pl[2]) { b[2] } else { b[5] },
            0.0,
        ];
        let d = vu(dot3(p, *pl) + pl[3]);
        if 0.0 <= d {
            return false;
        }
    }
    true
}

/// orig 0x146bc8: are all eight points on the negative side of all five planes? False as soon as one point has
/// `n . p + w >= 0` on any plane (planes walked in order, the points of a plane in order).
pub fn points_inside_planes(planes: &[Plane; 5], pts: &[V4; 8]) -> bool {
    for pl in planes {
        for p in pts {
            let d = vu(dot3(*p, *pl) + pl[3]);
            if 0.0 <= d {
                return false;
            }
        }
    }
    true
}
