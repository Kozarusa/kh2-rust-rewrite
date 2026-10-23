//! COP2: VU0 in macro mode. Encodings and semantics follow the VU user's manual (macro
//! instructions of VU0) and the EE core manual's COP2 chapter.
//!
//! Flags (VU manual, "MAC flag" and "status flag"): every FMAC result lane of an add, subtract,
//! multiply, multiply-add or outer-product instruction updates the MAC flag (`vi[17]`): bits 15..12
//! O (overflow) x y z w, 11..8 U (underflow) x y z w, 7..4 S (sign) x y z w, 3..0 Z (zero) x y z w;
//! lanes outside the destination mask read 0. The status flag (`vi[16]`) takes in bits 3..0 Z S U O
//! of "any lane" (bit 0 Z, bit 1 S, bit 2 U, bit 3 O); bits 9..6 hold the same four as sticky bits
//! that only a `ctc2` clears; bits 4 and 5 are I (invalid) and D (divide by zero) of the last
//! `vdiv`, `vsqrt` or `vrsqrt`, with sticky copies in bits 10 and 11. Overflow clamps to the
//! largest finite value with the sign kept; a denormal result sets U and Z and, when
//! [`Ee::flush_denormals`] is on (off by default: the hardware flushes, but the ports compared
//! against this interpreter compute in plain `f32`, and the script oracle feeds raw integers
//! through these instructions), is stored as a signed zero. `vmax`, `vmini`, `vabs`, `vmove`, the conversions and the integer ops leave the MAC and
//! status flags alone. Denormal operands are not flushed [limitation]. VU0 data memory (4 KB at
//! 0x11004000) is what the `vlqi`, `vsqi`, `vlqd`, `vsqd`, `vilwr` and `viswr` instructions
//! address, one quadword per integer-register unit.

use crate::exec::{bits_vf, vf_bits};
use crate::{Ee, clamp, sx32};

const STATUS: usize = 16;
const MAC: usize = 17;
const CLIP: usize = 18;
const R: usize = 20;
const VU0_DATA: usize = 0x4000;

/// Where an FMAC result goes.
enum Target {
    Vf(usize),
    Acc,
}

/// Z S U O flags (bits 0..3) of one lane and the value stored for it.
fn fmac_lane(v: f32, flush: bool) -> (f32, u32) {
    let bits = v.to_bits();
    let (exp, sign) = ((bits >> 23) & 0xFF, bits >> 31 != 0);
    let s = if sign { 2 } else { 0 };
    if exp == 0xFF {
        (f32::from_bits((bits & 0x8000_0000) | 0x7F7F_FFFF), 8 | s)
    } else if exp == 0 {
        let u = if bits & 0x7F_FFFF != 0 { 4 } else { 0 };
        (if flush { f32::from_bits(bits & 0x8000_0000) } else { v }, 1 | u | s)
    } else {
        (v, s)
    }
}

impl Ee {
    fn vi_read(&self, r: usize) -> u32 {
        match r {
            0 => 0,
            1..=15 => self.vi[r] & 0xFFFF,
            22 => self.q.to_bits(),
            _ => self.vi[r],
        }
    }

    fn vi_write(&mut self, r: usize, v: u32) {
        match r {
            0 => {}
            1..=15 => self.vi[r] = v & 0xFFFF,
            22 => self.q = f32::from_bits(v),
            STATUS => self.vi[r] = v & 0xFFF,
            MAC => self.vi[r] = v & 0xFFFF,
            CLIP => self.vi[r] = v & 0xFF_FFFF,
            R => self.vi[r] = (v & 0x7F_FFFF) | 0x3F80_0000,
            _ => self.vi[r] = v,
        }
    }

    fn i_reg(&self) -> f32 {
        f32::from_bits(self.vi[21])
    }

    /// Writes the lanes of `v` selected by `dest` into `vf[fd]` (clamped, no flags).
    fn vset(&mut self, fd: usize, dest: u32, v: [f32; 4]) {
        if fd == 0 {
            return;
        }
        for (i, x) in v.into_iter().enumerate() {
            if dest & (8 >> i) != 0 {
                self.vf[fd][i] = clamp(x);
            }
        }
    }

    /// Stores an FMAC result in the destination lanes and sets the MAC and status flags.
    fn fmac(&mut self, dest: u32, target: Target, r: [f32; 4]) {
        let mut mac = 0u32;
        for (k, x) in r.into_iter().enumerate() {
            if dest & (8 >> k) == 0 {
                continue;
            }
            let (v, f) = fmac_lane(x, self.flush_denormals);
            mac |= (f & 1) << (3 - k) | (f >> 1 & 1) << (7 - k) | (f >> 2 & 1) << (11 - k) | (f >> 3 & 1) << (15 - k);
            match target {
                Target::Vf(0) => {}
                Target::Vf(fd) => self.vf[fd][k] = v,
                Target::Acc => self.vacc[k] = v,
            }
        }
        self.vi[MAC] = mac;
        let any = |m: u32| (m != 0) as u32;
        let f4 = any(mac & 0xF) | any(mac & 0xF0) << 1 | any(mac & 0xF00) << 2 | any(mac & 0xF000) << 3;
        self.vi[STATUS] = (self.vi[STATUS] & !0xF) | f4 | f4 << 6 | (self.vi[STATUS] & 0x3C0);
    }

    /// Sets Q and the I and D flags for a divide of `n` by `d`.
    fn vu_divide(&mut self, n: f32, d: f32) {
        self.vi[STATUS] &= !0x30;
        if d.to_bits() & 0x7F80_0000 == 0 {
            // Divisor zero (or denormal): 0/0 is invalid, x/0 is a divide by zero.
            let flag = if n.to_bits() & 0x7F80_0000 == 0 { 0x10 | 0x400 } else { 0x20 | 0x800 };
            self.vi[STATUS] |= flag;
            self.q = f32::from_bits(((n.to_bits() ^ d.to_bits()) & 0x8000_0000) | 0x7F7F_FFFF);
        } else {
            self.q = fmac_lane(clamp(n / d), self.flush_denormals).0;
        }
    }

    fn vu_sqrt(&mut self, d: f32) {
        self.vi[STATUS] &= !0x30;
        if d < 0.0 {
            self.vi[STATUS] |= 0x10 | 0x400;
        }
        self.q = d.abs().sqrt();
    }

    fn vu_rsqrt(&mut self, n: f32, d: f32) {
        self.vi[STATUS] &= !0x30;
        if d.to_bits() & 0x7F80_0000 == 0 {
            let flag = if n.to_bits() & 0x7F80_0000 == 0 { 0x10 | 0x400 } else { 0x20 | 0x800 };
            self.vi[STATUS] |= flag;
            self.q = f32::from_bits(((n.to_bits() ^ d.to_bits()) & 0x8000_0000) | 0x7F7F_FFFF);
        } else {
            if d < 0.0 {
                self.vi[STATUS] |= 0x10 | 0x400;
            }
            self.q = fmac_lane(clamp(n / d.abs().sqrt()), self.flush_denormals).0;
        }
    }

    /// The next value of the R register (VU manual, VRNEXT): a 23-bit shift register with taps at
    /// bits 22 and 4, the exponent fixed at 1.0.
    fn r_next(&mut self) -> u32 {
        let r = self.vi[R];
        let (x, y) = ((r >> 4) & 1, (r >> 22) & 1);
        self.vi[R] = (((r << 1) ^ x ^ y) & 0x7F_FFFF) | 0x3F80_0000;
        self.vi[R]
    }

    fn vu0_quad(&mut self, index: u32) -> usize {
        VU0_DATA + (index as usize & 0xFF) * 16
    }

    fn vu0_load(&mut self, index: u32) -> [f32; 4] {
        let at = self.vu0_quad(index);
        let m = self.hw.vu_mem();
        bits_vf(u128::from_le_bytes(m[at..at + 16].try_into().unwrap()))
    }

    fn vu0_store(&mut self, index: u32, dest: u32, v: [f32; 4]) {
        let at = self.vu0_quad(index);
        let m = self.hw.vu_mem();
        for (k, x) in v.into_iter().enumerate() {
            if dest & (8 >> k) != 0 {
                m[at + 4 * k..at + 4 * k + 4].copy_from_slice(&x.to_bits().to_le_bytes());
            }
        }
    }

    #[inline(never)]
    pub(crate) fn cop2(&mut self, w: u32, fmt: usize, rt: usize, rd: usize) {
        if w & (1 << 25) == 0 {
            match fmt {
                0x01 => self.set128(rt, vf_bits(self.vf[rd])),
                0x02 => self.set(rt, sx32(self.vi_read(rd))),
                0x05 => {
                    if rd != 0 {
                        self.vf[rd] = bits_vf(self.gpr[rt])
                    }
                }
                0x06 => self.vi_write(rd, self.g32(rt)),
                0x08 => {
                    // BC2F/T/FL/TL test VBS0, the "VU0 microprogram running" bit of VPU_STAT. Macro
                    // mode never starts one, so the bit is whatever the program wrote to vi[30].
                    let busy = self.vi[30] & 1 != 0;
                    let bt = self.pc.wrapping_add(4).wrapping_add(((w as u16 as i16 as i32) << 2) as u32);
                    match rt {
                        0 => self.branch(!busy, bt, false),
                        1 => self.branch(busy, bt, false),
                        2 => self.branch(!busy, bt, true),
                        3 => self.branch(busy, bt, true),
                        _ => self.unimpl(w, "bc2"),
                    }
                }
                _ => self.unimpl(w, "cop2"),
            }
            return;
        }
        let dest = (w >> 21) & 15;
        let ft = ((w >> 16) & 31) as usize;
        let fs = ((w >> 11) & 31) as usize;
        let fd = ((w >> 6) & 31) as usize;
        let funct = w & 63;
        let (s, t) = (self.vf[fs], self.vf[ft]);
        let acc = self.vacc;
        let bc = t[(funct & 3) as usize];
        let (q, i) = (self.q, self.i_reg());
        let map = |f: &dyn Fn(usize) -> f32| -> [f32; 4] { std::array::from_fn(f) };
        if funct < 0x3C {
            let v = Target::Vf(fd);
            match funct {
                0x00..=0x03 => self.fmac(dest, v, map(&|k| s[k] + bc)),
                0x04..=0x07 => self.fmac(dest, v, map(&|k| s[k] - bc)),
                0x08..=0x0B => self.fmac(dest, v, map(&|k| acc[k] + s[k] * bc)),
                0x0C..=0x0F => self.fmac(dest, v, map(&|k| acc[k] - s[k] * bc)),
                0x10..=0x13 => self.vset(fd, dest, map(&|k| s[k].max(bc))),
                0x14..=0x17 => self.vset(fd, dest, map(&|k| s[k].min(bc))),
                0x18..=0x1B => self.fmac(dest, v, map(&|k| s[k] * bc)),
                0x1C => self.fmac(dest, v, map(&|k| s[k] * q)),
                0x1D => self.vset(fd, dest, map(&|k| s[k].max(i))),
                0x1E => self.fmac(dest, v, map(&|k| s[k] * i)),
                0x1F => self.vset(fd, dest, map(&|k| s[k].min(i))),
                0x20 => self.fmac(dest, v, map(&|k| s[k] + q)),
                0x21 => self.fmac(dest, v, map(&|k| acc[k] + s[k] * q)),
                0x22 => self.fmac(dest, v, map(&|k| s[k] + i)),
                0x23 => self.fmac(dest, v, map(&|k| acc[k] + s[k] * i)),
                0x24 => self.fmac(dest, v, map(&|k| s[k] - q)),
                0x25 => self.fmac(dest, v, map(&|k| acc[k] - s[k] * q)),
                0x26 => self.fmac(dest, v, map(&|k| s[k] - i)),
                0x27 => self.fmac(dest, v, map(&|k| acc[k] - s[k] * i)),
                0x28 => self.fmac(dest, v, map(&|k| s[k] + t[k])),
                0x29 => self.fmac(dest, v, map(&|k| acc[k] + s[k] * t[k])),
                0x2A => self.fmac(dest, v, map(&|k| s[k] * t[k])),
                0x2B => self.vset(fd, dest, map(&|k| s[k].max(t[k]))),
                0x2C => self.fmac(dest, v, map(&|k| s[k] - t[k])),
                0x2D => self.fmac(dest, v, map(&|k| acc[k] - s[k] * t[k])),
                0x2E => self.fmac(dest & 0xE, v, [acc[0] - s[1] * t[2], acc[1] - s[2] * t[0], acc[2] - s[0] * t[1], 0.0]),
                0x2F => self.vset(fd, dest, map(&|k| s[k].min(t[k]))),
                0x30 => self.vi_write(fd, self.vi_read(fs).wrapping_add(self.vi_read(ft))),
                0x31 => self.vi_write(fd, self.vi_read(fs).wrapping_sub(self.vi_read(ft))),
                0x32 => {
                    let imm = ((((w >> 6) & 31) as i32) << 27 >> 27) as u32;
                    self.vi_write(ft, self.vi_read(fs).wrapping_add(imm))
                }
                0x34 => self.vi_write(fd, self.vi_read(fs) & self.vi_read(ft)),
                0x35 => self.vi_write(fd, self.vi_read(fs) | self.vi_read(ft)),
                // VCALLMS and VCALLMSR start a microprogram; the game does not use them.
                _ => self.unimpl(w, "vu upper"),
            }
            return;
        }
        let op = (((w >> 6) & 31) << 2) | (funct & 3);
        let fsf = ((w >> 21) & 3) as usize;
        let ftf = ((w >> 23) & 3) as usize;
        let scale = |n: u32| (1u32 << n) as f32;
        let a = Target::Acc;
        match op {
            0x00..=0x03 => self.fmac(dest, a, map(&|k| s[k] + bc)),
            0x04..=0x07 => self.fmac(dest, a, map(&|k| s[k] - bc)),
            0x08..=0x0B => self.fmac(dest, a, map(&|k| acc[k] + s[k] * bc)),
            0x0C..=0x0F => self.fmac(dest, a, map(&|k| acc[k] - s[k] * bc)),
            0x10..=0x13 => {
                let n = [0, 4, 12, 15][(op & 3) as usize];
                let v = map(&|k| s[k].to_bits() as i32 as f32 / scale(n));
                self.vset(ft, dest, v);
            }
            0x14..=0x17 => {
                if ft != 0 {
                    let n = [0, 4, 12, 15][(op & 3) as usize];
                    for (k, sk) in s.iter().enumerate() {
                        if dest & (8 >> k) != 0 {
                            self.vf[ft][k] = f32::from_bits((*sk * scale(n)) as i32 as u32);
                        }
                    }
                }
            }
            0x18..=0x1B => self.fmac(dest, a, map(&|k| s[k] * bc)),
            0x1C => self.fmac(dest, a, map(&|k| s[k] * q)),
            0x1D => self.vset(ft, dest, map(&|k| s[k].abs())),
            0x1E => self.fmac(dest, a, map(&|k| s[k] * i)),
            0x1F => {
                let wv = t[3].abs();
                let mut flags = 0u32;
                for (k, sk) in s.iter().enumerate().take(3) {
                    if *sk > wv {
                        flags |= 1 << (2 * k);
                    }
                    if *sk < -wv {
                        flags |= 2 << (2 * k);
                    }
                }
                self.vi[CLIP] = ((self.vi[CLIP] << 6) | flags) & 0xFF_FFFF;
            }
            0x20 => self.fmac(dest, a, map(&|k| s[k] + q)),
            0x21 => self.fmac(dest, a, map(&|k| acc[k] + s[k] * q)),
            0x22 => self.fmac(dest, a, map(&|k| s[k] + i)),
            0x23 => self.fmac(dest, a, map(&|k| acc[k] + s[k] * i)),
            0x24 => self.fmac(dest, a, map(&|k| s[k] - q)),
            0x25 => self.fmac(dest, a, map(&|k| acc[k] - s[k] * q)),
            0x26 => self.fmac(dest, a, map(&|k| s[k] - i)),
            0x27 => self.fmac(dest, a, map(&|k| acc[k] - s[k] * i)),
            0x28 => self.fmac(dest, a, map(&|k| s[k] + t[k])),
            0x29 => self.fmac(dest, a, map(&|k| acc[k] + s[k] * t[k])),
            0x2A => self.fmac(dest, a, map(&|k| s[k] * t[k])),
            0x2C => self.fmac(dest, a, map(&|k| s[k] - t[k])),
            0x2D => self.fmac(dest, a, map(&|k| acc[k] - s[k] * t[k])),
            0x2E => self.fmac(dest & 0xE, a, [s[1] * t[2], s[2] * t[0], s[0] * t[1], 0.0]),
            0x2F | 0x3B => {} // vnop, vwaitq
            0x30 => self.vset(ft, dest, s),
            0x31 => self.vset(ft, dest, [s[1], s[2], s[3], s[0]]),
            // VLQI / VSQI / VLQD / VSQD: quadword access through an auto-incremented integer register.
            0x34 => {
                let v = self.vu0_load(self.vi_read(fs));
                self.vset(ft, dest, v);
                self.vi_write(fs, self.vi_read(fs).wrapping_add(1));
            }
            0x35 => {
                self.vu0_store(self.vi_read(ft), dest, s);
                self.vi_write(ft, self.vi_read(ft).wrapping_add(1));
            }
            0x36 => {
                self.vi_write(fs, self.vi_read(fs).wrapping_sub(1));
                let v = self.vu0_load(self.vi_read(fs));
                self.vset(ft, dest, v);
            }
            0x37 => {
                self.vi_write(ft, self.vi_read(ft).wrapping_sub(1));
                self.vu0_store(self.vi_read(ft), dest, s);
            }
            0x38 => self.vu_divide(s[fsf], t[ftf]),
            0x39 => self.vu_sqrt(t[ftf]),
            0x3A => self.vu_rsqrt(s[fsf], t[ftf]),
            0x3C => self.vi_write(ft, s[fsf].to_bits() & 0xFFFF),
            0x3D => {
                let v = f32::from_bits(self.vi_read(fs) as u16 as i16 as i32 as u32);
                if ft != 0 {
                    for k in 0..4 {
                        if dest & (8 >> k) != 0 {
                            self.vf[ft][k] = v;
                        }
                    }
                }
            }
            // VILWR / VISWR: one 16-bit field of a quadword, the field being the first set dest bit.
            0x3E | 0x3F => {
                let lane = (0..4).find(|k| dest & (8 >> k) != 0);
                let Some(k) = lane else { return };
                let at = self.vu0_quad(self.vi_read(fs)) + 4 * k;
                if op == 0x3E {
                    let m = self.hw.vu_mem();
                    let v = u32::from_le_bytes(m[at..at + 4].try_into().unwrap());
                    self.vi_write(ft, v & 0xFFFF);
                } else {
                    let v = self.vi_read(ft);
                    self.hw.vu_mem()[at..at + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
            // VRNEXT / VRGET / VRINIT / VRXOR: the random-number register.
            0x40 | 0x41 => {
                let r = if op == 0x40 { self.r_next() } else { self.vi[R] };
                if ft != 0 {
                    for k in 0..4 {
                        if dest & (8 >> k) != 0 {
                            self.vf[ft][k] = f32::from_bits(r);
                        }
                    }
                }
            }
            0x42 => self.vi[R] = (s[fsf].to_bits() & 0x7F_FFFF) | 0x3F80_0000,
            0x43 => self.vi[R] = ((s[fsf].to_bits() ^ self.vi[R]) & 0x7F_FFFF) | 0x3F80_0000,
            _ => self.unimpl(w, "vu lower"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fault;

    /// A COP2 macro instruction word: special1 form.
    fn s1(dest: u32, ft: u32, fs: u32, fd: u32, funct: u32) -> u32 {
        (0x12 << 26) | (1 << 25) | (dest << 21) | (ft << 16) | (fs << 11) | (fd << 6) | funct
    }
    /// Special2 form: `op` is the 7-bit opcode (bits 10..6 in the high five).
    fn s2(dest: u32, ft: u32, fs: u32, op: u32) -> u32 {
        (0x12 << 26) | (1 << 25) | (dest << 21) | (ft << 16) | (fs << 11) | ((op >> 2) << 6) | (0x3C | (op & 3))
    }

    fn vu() -> Ee {
        Ee::new()
    }

    #[test]
    fn mac_zero_and_sign_flags_per_lane() {
        // VU manual, VMUL: MAC bits Z (3..0, x at bit 3) and S (7..4) follow the result lanes.
        let mut e = vu();
        e.vf[1] = [1.0, -2.0, 0.0, 3.0];
        e.vf[2] = [1.0, 1.0, 5.0, -1.0];
        e.execute(s1(0xF, 2, 1, 3, 0x2A)).unwrap(); // VMUL.xyzw vf3, vf1, vf2
        assert_eq!(e.vf[3], [1.0, -2.0, 0.0, -3.0]);
        // x positive, y negative (S bit 6), z zero (Z bit 1), w negative (S bit 4).
        assert_eq!(e.vi[17], 1 << 6 | 1 << 1 | 1 << 4);
        // Status: Z and S set (bits 0, 1) with their sticky copies (bits 6, 7).
        assert_eq!(e.vi[16], 0b11 | 0b11 << 6);
    }

    #[test]
    fn mac_only_covers_the_destination_lanes() {
        let mut e = vu();
        e.vf[1] = [0.0, 0.0, 0.0, 0.0];
        e.vf[2] = [1.0; 4];
        e.execute(s1(0b1000, 2, 1, 3, 0x2A)).unwrap(); // VMUL.x: only x is written and flagged
        assert_eq!(e.vi[17], 1 << 3, "Zx only");
        e.vf[1] = [1.0; 4];
        e.execute(s1(0b0001, 2, 1, 3, 0x2A)).unwrap(); // VMUL.w, nonzero
        assert_eq!(e.vi[17], 0, "each instruction replaces the MAC flag");
        assert_eq!(e.vi[16] & 0xF, 0, "status bits 3..0 follow the last result");
        assert_eq!(e.vi[16] >> 6 & 0xF, 1, "sticky Z stays");
    }

    #[test]
    fn overflow_clamps_and_sets_o_and_s() {
        let mut e = vu();
        e.vf[1] = [f32::MAX, -f32::MAX, 1.0, 1.0];
        e.vf[2] = [2.0, 2.0, 1.0, 1.0];
        e.execute(s1(0xF, 2, 1, 3, 0x2A)).unwrap();
        assert_eq!(e.vf[3][0], f32::MAX);
        assert_eq!(e.vf[3][1], -f32::MAX);
        assert_eq!(e.vi[17] >> 12, 0b1100, "Ox and Oy");
        assert_eq!(e.vi[17] >> 4 & 0xF, 0b0100, "Sy");
        assert_eq!(e.vi[16] & 0xF, 0b1010, "status: S and O (bit 1 and bit 3), no Z or U");
    }

    #[test]
    fn denormal_results_flush_to_zero_with_u_and_z() {
        let mut e = vu();
        e.flush_denormals = true;
        // 1e-20 * 1e-20 = 1e-40, below the smallest normal float.
        e.vf[1] = [1e-20, -1e-20, 1.0, 1.0];
        e.vf[2] = [1e-20, 1e-20, 1.0, 1.0];
        e.execute(s1(0xF, 2, 1, 3, 0x2A)).unwrap();
        assert_eq!(e.vf[3][0].to_bits(), 0);
        assert_eq!(e.vf[3][1].to_bits(), 0x8000_0000, "signed zero");
        assert_eq!(e.vi[17] >> 8 & 0xF, 0b1100, "Ux Uy");
        assert_eq!(e.vi[17] & 0xF, 0b1100, "Zx Zy");
        assert_eq!(e.vi[17] >> 4 & 0xF, 0b0100, "Sy");
        // With the flush off the flags are the same and the denormal is kept.
        let mac = e.vi[17];
        e.flush_denormals = false;
        e.execute(s1(0xF, 2, 1, 3, 0x2A)).unwrap();
        assert_ne!(e.vf[3][0].to_bits(), 0);
        assert_eq!(e.vi[17], mac);
    }

    #[test]
    fn accumulate_forms_update_flags_too_but_max_does_not() {
        let mut e = vu();
        e.vf[1] = [0.0; 4];
        e.vf[2] = [1.0; 4];
        e.execute(s2(0xF, 2, 1, 0x2A)).unwrap(); // VMULA
        assert_eq!(e.vi[17] & 0xF, 0xF, "VMULA of zeros: all Z");
        e.vi[17] = 0;
        e.vf[1] = [1.0; 4];
        e.execute(s1(0xF, 2, 1, 3, 0x2B)).unwrap(); // VMAX
        assert_eq!(e.vi[17], 0, "VMAX leaves the MAC flag alone");
    }

    #[test]
    fn fmac_to_vf0_still_sets_flags() {
        let mut e = vu();
        e.vf[1] = [0.0; 4];
        e.vf[2] = [1.0; 4];
        e.execute(s1(0xF, 2, 1, 0, 0x2A)).unwrap();
        assert_eq!(e.vf[0], [0.0, 0.0, 0.0, 1.0], "vf0 is not written");
        assert_eq!(e.vi[17] & 0xF, 0xF);
    }

    #[test]
    fn cfc2_ctc2_see_the_flags() {
        // CFC2 rt, vi17 reads the MAC flag; CTC2 vi16 clears the sticky bits.
        let mut e = vu();
        e.vf[1] = [0.0; 4];
        e.vf[2] = [1.0; 4];
        e.execute(s1(0xF, 2, 1, 3, 0x2A)).unwrap();
        e.execute((0x12 << 26) | (2 << 21) | (4 << 16) | (17 << 11)).unwrap(); // cfc2 a0, vi17
        assert_eq!(e.gpr[4] as u32, 0xF);
        e.gpr[5] = 0;
        e.execute((0x12 << 26) | (6 << 21) | (5 << 16) | (16 << 11)).unwrap(); // ctc2 a1, vi16
        assert_eq!(e.vi[16], 0);
        e.gpr[5] = 0xFFFF_FFFF;
        e.execute((0x12 << 26) | (6 << 21) | (5 << 16) | (16 << 11)).unwrap();
        assert_eq!(e.vi[16], 0xFFF, "the status flag is 12 bits wide");
    }

    #[test]
    fn divide_flags_and_results() {
        let mut e = vu();
        let div = |e: &mut Ee, n: f32, d: f32| {
            e.vf[1] = [n, 0.0, 0.0, 0.0];
            e.vf[2] = [d, 0.0, 0.0, 0.0];
            e.execute(s2(0, 2, 1, 0x38)).unwrap(); // VDIV Q, vf1x, vf2x
        };
        div(&mut e, 6.0, 3.0);
        assert_eq!(e.q, 2.0);
        assert_eq!(e.vi[16] & 0x30, 0);
        div(&mut e, 1.0, 0.0);
        assert_eq!(e.q.to_bits(), 0x7F7F_FFFF, "x/0 gives the largest finite value");
        assert_eq!(e.vi[16] & 0x30, 0x20, "D flag");
        assert_ne!(e.vi[16] & 0x800, 0, "sticky D");
        div(&mut e, -1.0, 0.0);
        assert_eq!(e.q.to_bits(), 0xFF7F_FFFF, "sign follows the operands");
        div(&mut e, 0.0, 0.0);
        assert_eq!(e.vi[16] & 0x30, 0x10, "I flag for 0/0, D cleared");
        assert_ne!(e.vi[16] & 0x400, 0, "sticky I");
        div(&mut e, 8.0, 2.0);
        assert_eq!(e.vi[16] & 0x30, 0, "the next divide clears I and D");
        assert_ne!(e.vi[16] & 0xC00, 0, "but not the sticky bits");
    }

    #[test]
    fn sqrt_and_rsqrt() {
        let mut e = vu();
        e.vf[1] = [9.0, 0.0, 0.0, 0.0];
        e.vf[2] = [16.0, 0.0, 0.0, 0.0];
        e.execute(s2(0, 2, 1, 0x39)).unwrap(); // VSQRT Q, vf2x
        assert_eq!(e.q, 4.0);
        e.execute(s2(0, 2, 1, 0x3A)).unwrap(); // VRSQRT Q, vf1x, vf2x
        assert_eq!(e.q, 9.0 / 4.0);
        e.vf[2] = [-16.0, 0.0, 0.0, 0.0];
        e.execute(s2(0, 2, 1, 0x39)).unwrap();
        assert_eq!(e.q, 4.0, "square root of the absolute value");
        assert_eq!(e.vi[16] & 0x30, 0x10, "I flag for a negative operand");
        e.vf[2] = [0.0; 4];
        e.execute(s2(0, 2, 1, 0x3A)).unwrap();
        assert_eq!(e.vi[16] & 0x30, 0x20, "rsqrt of zero: D");
        assert_eq!(e.q.to_bits(), 0x7F7F_FFFF);
    }

    #[test]
    fn random_register_ops() {
        // VU manual: VRINIT loads R from the low 23 bits with the exponent forced to 1.0;
        // VRGET reads it; VRNEXT advances the shift register (taps 22 and 4) and reads it;
        // VRXOR xors a vector field in.
        let mut e = vu();
        e.vf[1] = [f32::from_bits(0x0012_3456), 0.0, 0.0, 0.0];
        e.execute(s2(0, 0, 1, 0x42)).unwrap(); // VRINIT R, vf1x
        assert_eq!(e.vi[20], 0x3F92_3456);
        e.execute(s2(0xF, 3, 0, 0x41)).unwrap(); // VRGET.xyzw vf3, R
        assert_eq!(e.vf[3].map(f32::to_bits), [0x3F92_3456; 4]);
        // VRNEXT: R = ((R << 1) ^ bit4 ^ bit22) & 0x7FFFFF | 0x3F800000. 0x123456: bit 4 = 1, bit 22 = 0.
        e.execute(s2(0b1000, 4, 0, 0x40)).unwrap(); // VRNEXT.x vf4, R
        let expect = (((0x12_3456u32 << 1) ^ 1) & 0x7F_FFFF) | 0x3F80_0000;
        assert_eq!(e.vi[20], expect);
        assert_eq!(e.vf[4][0].to_bits(), expect, "the new value is delivered");
        assert_eq!(e.vf[4][1], 0.0, "only the dest lane is written");
        // VRXOR: R = (vf1x ^ R) & 0x7FFFFF | 0x3F800000.
        e.execute(s2(0, 0, 1, 0x43)).unwrap();
        assert_eq!(e.vi[20], (((0x12_3456u32 ^ expect) & 0x7F_FFFF) | 0x3F80_0000));
        // The sequence has a long period and is deterministic: 100 steps from the same seed agree.
        let seq = |mut e: Ee| -> Vec<u32> {
            e.vi[20] = 0x3F80_0001;
            (0..100)
                .map(|_| {
                    e.execute(s2(0b1000, 4, 0, 0x40)).unwrap();
                    e.vf[4][0].to_bits()
                })
                .collect()
        };
        let (a, b) = (seq(vu()), seq(vu()));
        assert_eq!(a, b);
        assert!(a.windows(2).all(|p| p[0] != p[1]));
        assert!(a.iter().all(|r| r >> 23 == 0x7F), "exponent stays at 1.0");
    }

    #[test]
    fn ctc2_to_r_keeps_the_exponent() {
        let mut e = vu();
        e.gpr[4] = 0xFFFF_FFFF;
        e.execute((0x12 << 26) | (6 << 21) | (4 << 16) | (20 << 11)).unwrap(); // ctc2 a0, vi20
        assert_eq!(e.vi[20], 0x3FFF_FFFF);
    }

    #[test]
    fn quadword_access_with_auto_increment() {
        let mut e = vu();
        e.vi[1] = 5; // vi1 = quadword index
        e.vf[1] = [1.0, 2.0, 3.0, 4.0];
        // VSQI.xyzw vf1, (vi1++): fs = source vf (bits 11-15), it = address register (bits 16-20).
        e.execute(s2(0xF, 1, 1, 0x35)).unwrap();
        assert_eq!(e.vi[1], 6, "post-increment");
        // VLQI.xyzw vf2, (vi5++) with vi5 pointing back at index 5.
        e.vi[5] = 5;
        e.execute(s2(0xF, 2, 5, 0x34)).unwrap();
        assert_eq!(e.vf[2], [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(e.vi[5], 6);
        // VLQD pre-decrements: vf3.xy = mem[5].xy again.
        e.execute(s2(0b1100, 3, 5, 0x36)).unwrap();
        assert_eq!(e.vi[5], 5);
        assert_eq!(e.vf[3], [1.0, 2.0, 0.0, 0.0], "only x and y written");
        // The store went to VU0 data memory, visible on the bus at 0x11004050.
        assert_eq!(e.r32(0x1100_4050), 1.0f32.to_bits());
        assert_eq!(e.r32(0x1100_405C), 4.0f32.to_bits());
        // VSQD pre-decrements the address register and stores only the dest lanes.
        e.vi[2] = 8;
        e.vf[1] = [9.0, 8.0, 7.0, 6.0];
        e.execute(s2(0b0001, 2, 1, 0x37)).unwrap();
        assert_eq!(e.vi[2], 7);
        assert_eq!(e.r32(0x1100_4000 + 7 * 16 + 12), 6.0f32.to_bits());
        assert_eq!(e.r32(0x1100_4000 + 7 * 16), 0, "other lanes untouched");
    }

    #[test]
    fn vilwr_viswr_move_sixteen_bit_fields() {
        let mut e = vu();
        e.vi[1] = 3; // quadword 3
        e.vi[2] = 0xBEEF;
        // VISWR.y vi2, (vi1): ft = value register (bits 16-20), fs = address register (bits 11-15).
        e.execute(s2(0b0100, 2, 1, 0x3F)).unwrap();
        assert_eq!(e.r32(0x1100_4000 + 3 * 16 + 4), 0xBEEF);
        e.vi[3] = 0;
        e.execute(s2(0b0100, 3, 1, 0x3E)).unwrap(); // VILWR.y vi3, (vi1)
        assert_eq!(e.vi[3], 0xBEEF);
    }

    #[test]
    fn macro_branches_on_vu0_idle() {
        // BC2F is taken while no microprogram runs (VBS0 clear), BC2T is not.
        let bc2f = (0x12 << 26) | (8 << 21) | 4;
        let bc2t = (0x12 << 26) | (8 << 21) | (1 << 16) | 4;
        let mut e = vu();
        e.pc = 0x1000;
        e.npc = 0x1004;
        e.execute(bc2f).unwrap();
        assert_eq!(e.npc_next, Some(0x1004 + 16));
        e.execute(bc2t).unwrap();
        assert_eq!(e.npc_next, None);
        e.vi[30] = 1;
        e.execute(bc2t).unwrap();
        assert_eq!(e.npc_next, Some(0x1004 + 16));
    }

    #[test]
    fn vcallms_stays_unimplemented() {
        let mut e = vu();
        assert!(matches!(e.execute(s1(0, 0, 0, 0, 0x38)), Err(Fault::Unimplemented { .. })));
    }

    #[test]
    fn vu_macro_mul_add() {
        let mut e = Ee::new();
        e.vf[1] = [1.0, 2.0, 3.0, 4.0];
        e.vf[2] = [10.0, 20.0, 30.0, 40.0];
        // vmulax.xyzw ACC, vf1, vf2x  (special2 op 0x18: fd field 6, funct 0x3C)
        e.exec((0x12 << 26) | (1 << 25) | (15 << 21) | (2 << 16) | (1 << 11) | (6 << 6) | 0x3C);
        assert_eq!(e.vacc, [10.0, 20.0, 30.0, 40.0]);
        // vmaddy.xyzw vf3, vf1, vf2y : fd = acc + vf1*20
        e.exec((0x12 << 26) | (1 << 25) | (15 << 21) | (2 << 16) | (1 << 11) | (3 << 6) | 0x09);
        assert_eq!(e.vf[3], [30.0, 60.0, 90.0, 120.0]);
        // vdiv Q, vf1x, vf2y  -> 1/20
        e.exec((0x12 << 26) | (1 << 25) | (1 << 23) | (2 << 16) | (1 << 11) | (0x0E << 6) | 0x3C);
        assert!((e.q - 0.05).abs() < 1e-7);
    }
}
