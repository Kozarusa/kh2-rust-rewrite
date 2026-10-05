//! Single-precision sine and cosine as the game computes them. The routines are the usual SunPro
//! kernels (argument reduction by multiples of pi/2, then a polynomial on the remainder), but the game's
//! constants differ from the published ones in the last bit in several places, so they are listed here as
//! read from the executable. Arguments beyond about 2^7 * pi/2 (never seen in scripts) use a double
//! precision fallback instead of the game's multi-word reduction.

const fn c(bits: u32) -> f32 {
    f32::from_bits(bits)
}

// Kernel sine: S1..S6.
const S1: f32 = c(0xbe2a_aaaa);
const S2: f32 = c(0x3c08_8889);
const S3: f32 = c(0xb950_0d01);
const S4: f32 = c(0x3638_ef1a);
const S5: f32 = c(0xb2d7_2f34);
const S6: f32 = c(0x2f2e_c9d2);
// Kernel cosine: C1..C6.
const C1: f32 = c(0x3d2a_aaaa);
const C2: f32 = c(0xbab6_0b60);
const C3: f32 = c(0x37d0_0d00);
const C4: f32 = c(0xb493_f27b);
const C5: f32 = c(0x310f_74f5);
const C6: f32 = c(0xad47_d74e);
// Reduction: pi/2 split into pieces.
const PIO2_1: f32 = c(0x3fc9_0f80);
const PIO2_1T: f32 = c(0x3735_4442);
const PIO2_2: f32 = c(0x3735_43ff);
const PIO2_2T: f32 = c(0x2e85_a308);
const PIO2_3: f32 = c(0x2e85_a300);
const PIO2_3T: f32 = c(0x248d_3131);
const INV_PIO2: f32 = c(0x3f22_f984);

fn abs_bits(x: f32) -> u32 {
    x.to_bits() & 0x7fff_ffff
}

fn kernel_sin(x: f32, y: f32, tail: bool) -> f32 {
    if abs_bits(x) <= 0x31ff_ffff && (x as i32) == 0 {
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = z * (z * (z * (z * S6 + S5) + S4) + S3) + S2;
    if !tail {
        x + v * (z * r + S1)
    } else {
        x - ((z * (y * 0.5 - v * r) - y) - v * S1)
    }
}

fn kernel_cos(x: f32, y: f32) -> f32 {
    let ax = abs_bits(x);
    if ax <= 0x31ff_ffff && (x as i32) == 0 {
        return 1.0;
    }
    let z = x * x;
    let r = z * (z * (z * (z * (z * (z * C6 + C5) + C4) + C3) + C2) + C1);
    if ax < 0x3e99_999a {
        1.0 - (z * 0.5 - (z * r - x * y))
    } else {
        let qx = if ax >= 0x3f48_0001 { 0.28125 } else { f32::from_bits(ax - 0x0100_0000) };
        (1.0 - qx) - ((z * 0.5 - qx) - (z * r - x * y))
    }
}

/// Splits x into n * pi/2 + (y0 + y1). Returns (n, y0, y1), or None for arguments that need the long reduction.
fn rem_pio2(x: f32) -> Option<(i32, f32, f32)> {
    let ax = abs_bits(x);
    let neg = x.to_bits() >> 31 != 0;
    if ax < 0x3f49_0fd9 {
        return Some((0, x, 0.0));
    }
    if ax < 0x4016_cbe4 {
        // |x| < 3 pi/4: n = +-1.
        let (mut z, n) = if !neg { (x - PIO2_1, 1) } else { (x + PIO2_1, -1) };
        let sgn = if neg { -1.0 } else { 1.0 };
        let (y0, y1);
        if x.to_bits() & 0x7fff_fff0 != 0x3fc9_0fd0 {
            y0 = z - sgn * PIO2_1T;
            y1 = (z - y0) - sgn * PIO2_1T;
        } else {
            z -= sgn * PIO2_2;
            y0 = z - sgn * PIO2_2T;
            y1 = (z - y0) - sgn * PIO2_2T;
        }
        return Some((n, y0, y1));
    }
    if ax <= 0x4349_0f80 {
        let t = x.abs();
        let n = (t * INV_PIO2 + 0.5) as i32;
        let fnn = n as f32;
        let mut r = t - fnn * PIO2_1;
        let mut w = fnn * PIO2_1T;
        let mut y0;
        let table_hit = (1..32).contains(&n) && (ax & 0xffff_ff00) != npio2_hw(n);
        if table_hit {
            y0 = r - w;
        } else {
            let j = (ax >> 23) as i32;
            y0 = r - w;
            let i = j - ((y0.to_bits() >> 23) & 0xff) as i32;
            if i > 8 {
                let t2 = r;
                w = fnn * PIO2_2;
                r = t2 - w;
                w = fnn * PIO2_2T - ((t2 - r) - w);
                y0 = r - w;
                let i = j - ((y0.to_bits() >> 23) & 0xff) as i32;
                if i > 25 {
                    let t3 = r;
                    w = fnn * PIO2_3;
                    r = t3 - w;
                    w = fnn * PIO2_3T - ((t3 - r) - w);
                    y0 = r - w;
                }
            }
        }
        let y1 = (r - y0) - w;
        return Some(if neg { (-n, -y0, -y1) } else { (n, y0, y1) });
    }
    None
}

/// High 24 bits of n * pi/2 as a float (the game's table for n = 1..31).
fn npio2_hw(n: i32) -> u32 {
    ((n as f64 * std::f64::consts::FRAC_PI_2) as f32).to_bits() & 0xffff_ff00
}

pub fn sin(x: f32) -> f32 {
    if abs_bits(x) < 0x3f49_0fd9 {
        return kernel_sin(x, 0.0, false);
    }
    match rem_pio2(x) {
        Some((n, y0, y1)) => match n & 3 {
            0 => kernel_sin(y0, y1, true),
            1 => kernel_cos(y0, y1),
            2 => -kernel_sin(y0, y1, true),
            _ => -kernel_cos(y0, y1),
        },
        None => (x as f64).sin() as f32,
    }
}

pub fn cos(x: f32) -> f32 {
    if abs_bits(x) < 0x3f49_0fd9 {
        return kernel_cos(x, 0.0);
    }
    match rem_pio2(x) {
        Some((n, y0, y1)) => match n & 3 {
            0 => kernel_cos(y0, y1),
            1 => -kernel_sin(y0, y1, true),
            2 => -kernel_cos(y0, y1),
            _ => kernel_sin(y0, y1, true),
        },
        None => (x as f64).cos() as f32,
    }
}
