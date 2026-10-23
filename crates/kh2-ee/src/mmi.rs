//! The R5900 multimedia instructions (opcode 0x1C). Encodings and semantics follow the EE core
//! instruction set manual, MMI chapter: the top-level function field selects MMI0, MMI1, MMI2 or
//! MMI3, which in turn select by the `sa` field. `PHMADH`, `PHMSBH` and `PDIVBW` are not
//! implemented: the game does not use them and their HI/LO distribution is not certain from
//! the sources at hand.

use crate::exec::{bytes, from_bytes, from_halves, from_words, halves, with_hi64, with_lo64, words};
use crate::{Ee, sx32};

fn sat_i32(x: i64) -> u32 {
    x.clamp(i32::MIN as i64, i32::MAX as i64) as i32 as u32
}
fn sat_i16(x: i32) -> u16 {
    x.clamp(i16::MIN as i32, i16::MAX as i32) as i16 as u16
}
fn sat_i8(x: i16) -> u8 {
    x.clamp(i8::MIN as i16, i8::MAX as i16) as i8 as u8
}

fn w_map(a: [u32; 4], b: [u32; 4], f: impl Fn(u32, u32) -> u32) -> u128 {
    from_words(std::array::from_fn(|k| f(a[k], b[k])))
}
fn h_map(a: [u16; 8], b: [u16; 8], f: impl Fn(u16, u16) -> u16) -> u128 {
    from_halves(std::array::from_fn(|k| f(a[k], b[k])))
}
fn b_map(a: [u8; 16], b: [u8; 16], f: impl Fn(u8, u8) -> u8) -> u128 {
    from_bytes(std::array::from_fn(|k| f(a[k], b[k])))
}

impl Ee {
    #[inline(never)]
    pub(crate) fn mmi(&mut self, w: u32, rs: usize, rt: usize, rd: usize, sa: u32, funct: u32) {
        let (a, b) = (self.gpr[rs], self.gpr[rt]);
        let (wa, wb) = (words(a), words(b));
        match funct {
            0x00 | 0x01 | 0x20 | 0x21 => {
                let pipe1 = funct >= 0x20;
                let (x, y) = (wa[0], wb[0]);
                let p = if funct & 1 == 0 { (x as i32 as i64).wrapping_mul(y as i32 as i64) as u64 } else { x as u64 * y as u64 };
                let (l, h) = if pipe1 { ((self.lo >> 64) as u32, (self.hi >> 64) as u32) } else { (self.lo as u32, self.hi as u32) };
                let s = (((h as u64) << 32) | l as u64).wrapping_add(p);
                let (nl, nh) = (sx32(s as u32), sx32((s >> 32) as u32));
                if pipe1 {
                    self.lo = with_hi64(self.lo, nl);
                    self.hi = with_hi64(self.hi, nh);
                } else {
                    self.lo = with_lo64(self.lo, nl);
                    self.hi = with_lo64(self.hi, nh);
                }
                self.set(rd, nl);
            }
            0x04 => {
                let lz = |x: u32| if (x as i32) < 0 { (!x).leading_zeros() - 1 } else { x.leading_zeros() - 1 };
                self.set(rd, lz(wa[0]) as u64 | (lz(wa[1]) as u64) << 32);
            }
            0x08 => self.mmi0(w, rd, a, b, sa),
            0x09 => self.mmi2(w, rd, a, b, sa),
            0x10 => self.set(rd, (self.hi >> 64) as u64),
            0x11 => self.hi = with_hi64(self.hi, a as u64),
            0x12 => self.set(rd, (self.lo >> 64) as u64),
            0x13 => self.lo = with_hi64(self.lo, a as u64),
            0x18 | 0x19 => {
                let p = if funct == 0x18 { (wa[0] as i32 as i64).wrapping_mul(wb[0] as i32 as i64) as u64 } else { wa[0] as u64 * wb[0] as u64 };
                self.lo = with_hi64(self.lo, sx32(p as u32));
                self.hi = with_hi64(self.hi, sx32((p >> 32) as u32));
                self.set(rd, sx32(p as u32));
            }
            0x1A | 0x1B => self.divide(wa[0], wb[0], funct == 0x1A, true),
            0x28 => self.mmi1(w, rd, a, b, sa),
            0x29 => self.mmi3(w, rd, a, b, sa),
            0x30 => self.pmfhl(w, rd, sa),
            0x31 if sa == 0 => {
                // PMTHL.LW: LO.w0 = rs.w0, HI.w0 = rs.w1, LO.w2 = rs.w2, HI.w2 = rs.w3.
                let (mut lo, mut hi) = (words(self.lo), words(self.hi));
                (lo[0], hi[0], lo[2], hi[2]) = (wa[0], wa[1], wa[2], wa[3]);
                self.lo = from_words(lo);
                self.hi = from_words(hi);
            }
            0x34 | 0x36 | 0x37 => {
                let sh = sa & 15;
                let h = halves(b).map(|x| match funct {
                    0x34 => x << sh,
                    0x36 => x >> sh,
                    _ => ((x as i16) >> sh) as u16,
                });
                self.set128(rd, from_halves(h));
            }
            0x3C | 0x3E | 0x3F => {
                let v = wb.map(|x| match funct {
                    0x3C => x << sa,
                    0x3E => x >> sa,
                    _ => ((x as i32) >> sa) as u32,
                });
                self.set128(rd, from_words(v));
            }
            _ => self.unimpl(w, "mmi"),
        }
    }

    /// PMFHL: move from HI/LO in one of five formats selected by `sa`.
    fn pmfhl(&mut self, w: u32, rd: usize, sa: u32) {
        let (lo, hi) = (words(self.lo), words(self.hi));
        let (lh, hh) = (halves(self.lo), halves(self.hi));
        let r = match sa {
            // LW: {LO0, HI0, LO2, HI2}
            0 => from_words([lo[0], hi[0], lo[2], hi[2]]),
            // UW: {LO1, HI1, LO3, HI3}
            1 => from_words([lo[1], hi[1], lo[3], hi[3]]),
            // SLW: each 64-bit (HI.w:LO.w) pair saturated to a signed 32-bit value, sign-extended.
            2 => {
                let one = |l: u32, h: u32| {
                    let v = ((h as u64) << 32 | l as u64) as i64;
                    sx32(sat_i32(v)) as u128
                };
                one(lo[0], hi[0]) | one(lo[2], hi[2]) << 64
            }
            // LH: low halfwords {LO.h0, LO.h2, HI.h0, HI.h2, LO.h4, LO.h6, HI.h4, HI.h6}
            3 => from_halves([lh[0], lh[2], hh[0], hh[2], lh[4], lh[6], hh[4], hh[6]]),
            // SH: alternate saturated words and plain low halfwords.
            4 => from_halves([
                sat_i16(lo[0] as i32),
                lh[2],
                sat_i16(lo[2] as i32),
                lh[6],
                sat_i16(hi[0] as i32),
                hh[2],
                sat_i16(hi[2] as i32),
                hh[6],
            ]),
            _ => return self.unimpl(w, "pmfhl"),
        };
        self.set128(rd, r);
    }

    fn mmi0(&mut self, w: u32, rd: usize, a: u128, b: u128, sa: u32) {
        let (wa, wb) = (words(a), words(b));
        let (ha, hb) = (halves(a), halves(b));
        let (ba, bb) = (bytes(a), bytes(b));
        let r = match sa {
            0x00 => w_map(wa, wb, u32::wrapping_add),
            0x01 => w_map(wa, wb, u32::wrapping_sub),
            0x02 => w_map(wa, wb, |x, y| if (x as i32) > (y as i32) { u32::MAX } else { 0 }),
            0x03 => w_map(wa, wb, |x, y| (x as i32).max(y as i32) as u32),
            0x04 => h_map(ha, hb, u16::wrapping_add),
            0x05 => h_map(ha, hb, u16::wrapping_sub),
            0x06 => h_map(ha, hb, |x, y| if (x as i16) > (y as i16) { u16::MAX } else { 0 }),
            0x07 => h_map(ha, hb, |x, y| (x as i16).max(y as i16) as u16),
            0x08 => b_map(ba, bb, u8::wrapping_add),
            0x09 => b_map(ba, bb, u8::wrapping_sub),
            0x0A => b_map(ba, bb, |x, y| if (x as i8) > (y as i8) { u8::MAX } else { 0 }),
            0x10 => w_map(wa, wb, |x, y| sat_i32(x as i32 as i64 + y as i32 as i64)),
            0x11 => w_map(wa, wb, |x, y| sat_i32(x as i32 as i64 - y as i32 as i64)),
            // PEXTLW: interleave the low words, rt first.
            0x12 => from_words([wb[0], wa[0], wb[1], wa[1]]),
            // PPACW: the even words of rt, then of rs.
            0x13 => from_words([wb[0], wb[2], wa[0], wa[2]]),
            0x14 => h_map(ha, hb, |x, y| sat_i16(x as i16 as i32 + y as i16 as i32)),
            0x15 => h_map(ha, hb, |x, y| sat_i16(x as i16 as i32 - y as i16 as i32)),
            // PEXTLH: interleave the low halfwords, rt first.
            0x16 => from_halves(std::array::from_fn(|k| if k % 2 == 0 { hb[k / 2] } else { ha[k / 2] })),
            // PPACH: the even halfwords of rt, then of rs.
            0x17 => from_halves(std::array::from_fn(|k| if k < 4 { hb[2 * k] } else { ha[2 * (k - 4)] })),
            0x18 => b_map(ba, bb, |x, y| sat_i8(x as i8 as i16 + y as i8 as i16)),
            0x19 => b_map(ba, bb, |x, y| sat_i8(x as i8 as i16 - y as i8 as i16)),
            // PEXTLB: interleave the low bytes, rt first.
            0x1A => from_bytes(std::array::from_fn(|k| if k % 2 == 0 { bb[k / 2] } else { ba[k / 2] })),
            // PPACB: the even bytes of rt, then of rs.
            0x1B => from_bytes(std::array::from_fn(|k| if k < 8 { bb[2 * k] } else { ba[2 * (k - 8)] })),
            // PEXT5: expand RGBA5551 in the low halfword of each rt word to 8:8:8:1 (1 in bit 31).
            0x1E => from_words(wb.map(|x| (x & 0x1F) << 3 | (x >> 5 & 0x1F) << 11 | (x >> 10 & 0x1F) << 19 | (x >> 15 & 1) << 31)),
            // PPAC5: pack the top 5 bits of each colour byte of each rt word, and the top bit.
            0x1F => from_words(wb.map(|x| (x >> 3 & 0x1F) | (x >> 11 & 0x1F) << 5 | (x >> 19 & 0x1F) << 10 | (x >> 31 & 1) << 15)),
            _ => return self.unimpl(w, "mmi0"),
        };
        self.set128(rd, r);
    }

    fn mmi1(&mut self, w: u32, rd: usize, a: u128, b: u128, sa: u32) {
        let (wa, wb) = (words(a), words(b));
        let (ha, hb) = (halves(a), halves(b));
        let (ba, bb) = (bytes(a), bytes(b));
        let r = match sa {
            0x01 => from_words(wb.map(|x| (x as i32).unsigned_abs().min(i32::MAX as u32))),
            0x02 => w_map(wa, wb, |x, y| if x == y { u32::MAX } else { 0 }),
            0x03 => w_map(wa, wb, |x, y| (x as i32).min(y as i32) as u32),
            // PADSBH: the low four halfwords subtract, the high four add.
            0x04 => from_halves(std::array::from_fn(|k| if k < 4 { ha[k].wrapping_sub(hb[k]) } else { ha[k].wrapping_add(hb[k]) })),
            0x05 => from_halves(hb.map(|x| (x as i16 as i32).unsigned_abs().min(i16::MAX as u32) as u16)),
            0x06 => h_map(ha, hb, |x, y| if x == y { u16::MAX } else { 0 }),
            0x07 => h_map(ha, hb, |x, y| (x as i16).min(y as i16) as u16),
            0x0A => b_map(ba, bb, |x, y| if x == y { u8::MAX } else { 0 }),
            0x10 => w_map(wa, wb, u32::saturating_add),
            0x11 => w_map(wa, wb, u32::saturating_sub),
            // PEXTUW: interleave the high words, rt first.
            0x12 => from_words([wb[2], wa[2], wb[3], wa[3]]),
            0x14 => h_map(ha, hb, u16::saturating_add),
            0x15 => h_map(ha, hb, u16::saturating_sub),
            // PEXTUH: interleave the high halfwords, rt first.
            0x16 => from_halves(std::array::from_fn(|k| if k % 2 == 0 { hb[4 + k / 2] } else { ha[4 + k / 2] })),
            0x18 => b_map(ba, bb, u8::saturating_add),
            0x19 => b_map(ba, bb, u8::saturating_sub),
            // PEXTUB: interleave the high bytes, rt first.
            0x1A => from_bytes(std::array::from_fn(|k| if k % 2 == 0 { bb[8 + k / 2] } else { ba[8 + k / 2] })),
            0x1B => {
                // QFSRV: rd = low 128 bits of (rs:rt) >> SA
                let s = self.sa & 127;
                if s == 0 { b } else { (b >> s) | (a << (128 - s)) }
            }
            _ => return self.unimpl(w, "mmi1"),
        };
        self.set128(rd, r);
    }

    /// Updates the two 64-bit accumulator lanes: `acc[k] = (HI.w[2k] : LO.w[2k]) op p[k]`, stored as
    /// sign-extended 32-bit halves in LO.d[k] and HI.d[k]; returns the full 64-bit results.
    fn accumulate(&mut self, p: [u64; 2], sub: bool, keep: bool) -> [u64; 2] {
        let (lo, hi) = (words(self.lo), words(self.hi));
        let mut out = [0u64; 2];
        let (mut nlo, mut nhi) = ([0u64; 2], [0u64; 2]);
        for k in 0..2 {
            let base = if keep { (hi[2 * k] as u64) << 32 | lo[2 * k] as u64 } else { 0 };
            let v = if sub { base.wrapping_sub(p[k]) } else { base.wrapping_add(p[k]) };
            out[k] = v;
            nlo[k] = sx32(v as u32);
            nhi[k] = sx32((v >> 32) as u32);
        }
        self.lo = nlo[0] as u128 | (nlo[1] as u128) << 64;
        self.hi = nhi[0] as u128 | (nhi[1] as u128) << 64;
        out
    }

    /// The halfword multiplies: products land in LO (lanes 0, 1, 4, 5) and HI (2, 3, 6, 7); the
    /// destination gets LO.w0, HI.w0, LO.w2, HI.w2.
    fn halfword_mac(&mut self, rd: usize, a: u128, b: u128, mode: i32) {
        let (ha, hb) = (halves(a), halves(b));
        let p: [i32; 8] = std::array::from_fn(|k| ha[k] as i16 as i32 * hb[k] as i16 as i32);
        let (mut lo, mut hi) = (words(self.lo), words(self.hi));
        let (li, hi_i) = ([0usize, 1, 4, 5], [2usize, 3, 6, 7]);
        for k in 0..4 {
            let (pl, ph) = (p[li[k]] as u32, p[hi_i[k]] as u32);
            match mode {
                0 => (lo[k], hi[k]) = (pl, ph),
                1 => (lo[k], hi[k]) = (lo[k].wrapping_add(pl), hi[k].wrapping_add(ph)),
                _ => (lo[k], hi[k]) = (lo[k].wrapping_sub(pl), hi[k].wrapping_sub(ph)),
            }
        }
        self.lo = from_words(lo);
        self.hi = from_words(hi);
        self.set128(rd, from_words([lo[0], hi[0], lo[2], hi[2]]));
    }

    fn mmi2(&mut self, w: u32, rd: usize, a: u128, b: u128, sa: u32) {
        let (wa, wb) = (words(a), words(b));
        let (ha, hb) = (halves(a), halves(b));
        let signed = |x: u32| x as i32 as i64;
        let r = match sa {
            // PMADDW / PMSUBW / PMULTW: signed products of words 0 and 2 into the 64-bit lanes.
            0x00 | 0x04 | 0x0C => {
                let p = [(signed(wa[0]) * signed(wb[0])) as u64, (signed(wa[2]) * signed(wb[2])) as u64];
                let o = self.accumulate(p, sa == 0x04, sa != 0x0C);
                return self.set128(rd, o[0] as u128 | (o[1] as u128) << 64);
            }
            // PSLLVW / PSRLVW: variable shifts of words 0 and 2, results sign-extended to 64 bits.
            0x02 | 0x03 => {
                let f = |x: u32, s: u32| if sa == 0x02 { x << (s & 31) } else { x >> (s & 31) };
                sx32(f(wb[0], wa[0])) as u128 | (sx32(f(wb[2], wa[2])) as u128) << 64
            }
            0x08 => self.hi,
            0x09 => self.lo,
            // PINTH: interleave the low halfwords of rt with the high halfwords of rs.
            0x0A => from_halves(std::array::from_fn(|k| if k % 2 == 0 { hb[k / 2] } else { ha[4 + k / 2] })),
            // PDIVW: signed divide of words 0 and 2; quotient to LO, remainder to HI.
            0x0D => {
                let mut lo = [0u64; 2];
                let mut hi = [0u64; 2];
                for (k, i) in [0usize, 2].into_iter().enumerate() {
                    let (x, y) = (wa[i] as i32, wb[i] as i32);
                    let (q, r) = if y == 0 { (if x < 0 { 1 } else { -1i32 }, x) } else { (x.wrapping_div(y), x.wrapping_rem(y)) };
                    lo[k] = sx32(q as u32);
                    hi[k] = sx32(r as u32);
                }
                self.lo = lo[0] as u128 | (lo[1] as u128) << 64;
                self.hi = hi[0] as u128 | (hi[1] as u128) << 64;
                return;
            }
            0x0E => (a << 64) | (b & u64::MAX as u128),
            0x10 => return self.halfword_mac(rd, a, b, 1),
            0x12 => a & b,
            0x13 => a ^ b,
            0x14 => return self.halfword_mac(rd, a, b, 2),
            // PEXEH: swap halfwords 0 and 2 of each half.
            0x1A => from_halves([hb[2], hb[1], hb[0], hb[3], hb[6], hb[5], hb[4], hb[7]]),
            // PREVH: reverse the halfwords of each half.
            0x1B => from_halves([hb[3], hb[2], hb[1], hb[0], hb[7], hb[6], hb[5], hb[4]]),
            0x1C => return self.halfword_mac(rd, a, b, 0),
            0x1E => from_words([wb[2], wb[1], wb[0], wb[3]]),
            0x1F => from_words([wb[1], wb[2], wb[0], wb[3]]),
            _ => return self.unimpl(w, "mmi2"),
        };
        self.set128(rd, r);
    }

    fn mmi3(&mut self, w: u32, rd: usize, a: u128, b: u128, sa: u32) {
        let (wa, wb) = (words(a), words(b));
        let (ha, hb) = (halves(a), halves(b));
        let r = match sa {
            // PMADDUW / PMULTUW: unsigned products of words 0 and 2.
            0x00 | 0x0C => {
                let p = [wa[0] as u64 * wb[0] as u64, wa[2] as u64 * wb[2] as u64];
                let o = self.accumulate(p, false, sa == 0x00);
                return self.set128(rd, o[0] as u128 | (o[1] as u128) << 64);
            }
            // PSRAVW: variable arithmetic shift of words 0 and 2, sign-extended to 64 bits.
            0x03 => {
                let f = |x: u32, s: u32| ((x as i32) >> (s & 31)) as u32;
                sx32(f(wb[0], wa[0])) as u128 | (sx32(f(wb[2], wa[2])) as u128) << 64
            }
            0x08 => {
                self.hi = a;
                return;
            }
            0x09 => {
                self.lo = a;
                return;
            }
            // PINTEH: interleave the even halfwords, rt first.
            0x0A => from_halves(std::array::from_fn(|k| if k % 2 == 0 { hb[k] } else { ha[k - 1] })),
            // PDIVUW: unsigned divide of words 0 and 2.
            0x0D => {
                let mut lo = [0u64; 2];
                let mut hi = [0u64; 2];
                for (k, i) in [0usize, 2].into_iter().enumerate() {
                    let (x, y) = (wa[i], wb[i]);
                    let (q, r) = (x.checked_div(y).unwrap_or(u32::MAX), x.checked_rem(y).unwrap_or(x));
                    lo[k] = sx32(q);
                    hi[k] = sx32(r);
                }
                self.lo = lo[0] as u128 | (lo[1] as u128) << 64;
                self.hi = hi[0] as u128 | (hi[1] as u128) << 64;
                return;
            }
            0x0E => (b & !(u64::MAX as u128)) | (a >> 64),
            0x12 => a | b,
            0x13 => !(a | b),
            // PEXCH: swap halfwords 1 and 2 of each half.
            0x1A => from_halves([hb[0], hb[2], hb[1], hb[3], hb[4], hb[6], hb[5], hb[7]]),
            0x1B => from_halves([hb[0], hb[0], hb[0], hb[0], hb[4], hb[4], hb[4], hb[4]]),
            0x1E => from_words([wb[0], wb[2], wb[1], wb[3]]),
            _ => return self.unimpl(w, "mmi3"),
        };
        self.set128(rd, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fault;

    fn mm(funct: u32, sa: u32, rs: u32, rt: u32, rd: u32) -> u32 {
        (0x1C << 26) | (rs << 21) | (rt << 16) | (rd << 11) | (sa << 6) | funct
    }

    /// Executes one MMI instruction with rs = register 1 and rt = register 2, result in register 3.
    fn op(funct: u32, sa: u32, rs: u128, rt: u128) -> Ee {
        let mut e = Ee::new();
        e.gpr[1] = rs;
        e.gpr[2] = rt;
        e.execute(mm(funct, sa, 1, 2, 3)).expect("decodes");
        e
    }

    fn bytes_of(v: u128) -> [u8; 16] {
        v.to_le_bytes()
    }
    fn h(v: [u16; 8]) -> u128 {
        from_halves(v)
    }
    fn w4(v: [u32; 4]) -> u128 {
        from_words(v)
    }
    fn b16(v: [u8; 16]) -> u128 {
        u128::from_le_bytes(v)
    }
    fn seq(base: u8) -> u128 {
        b16(std::array::from_fn(|i| base + i as u8))
    }

    #[test]
    fn pextlb_interleaves_low_bytes_rt_first() {
        // EE core manual, PEXTLB: rd = rs.b7 rt.b7 ... rs.b0 rt.b0 (byte 0 is rt.b0, byte 1 is rs.b0).
        let e = op(0x08, 0x1A, seq(0x10), seq(0x20));
        let r = bytes_of(e.gpr[3]);
        assert_eq!(r[..6], [0x20, 0x10, 0x21, 0x11, 0x22, 0x12]);
        assert_eq!(r[14..], [0x27, 0x17]);
    }

    #[test]
    fn pextlh_and_pextuh_interleave_halfwords() {
        let rs = h([10, 11, 12, 13, 14, 15, 16, 17]);
        let rt = h([20, 21, 22, 23, 24, 25, 26, 27]);
        assert_eq!(op(0x08, 0x16, rs, rt).gpr[3], h([20, 10, 21, 11, 22, 12, 23, 13]), "PEXTLH");
        assert_eq!(op(0x28, 0x16, rs, rt).gpr[3], h([24, 14, 25, 15, 26, 16, 27, 17]), "PEXTUH");
    }

    #[test]
    fn pextub_interleaves_high_bytes() {
        let r = bytes_of(op(0x28, 0x1A, seq(0x10), seq(0x20)).gpr[3]);
        assert_eq!(r[..4], [0x28, 0x18, 0x29, 0x19]);
        assert_eq!(r[14..], [0x2F, 0x1F]);
    }

    #[test]
    fn ppach_and_ppacb_pack_even_lanes() {
        let rs = h([10, 11, 12, 13, 14, 15, 16, 17]);
        let rt = h([20, 21, 22, 23, 24, 25, 26, 27]);
        assert_eq!(op(0x08, 0x17, rs, rt).gpr[3], h([20, 22, 24, 26, 10, 12, 14, 16]), "PPACH");
        let r = bytes_of(op(0x08, 0x1B, seq(0x10), seq(0x20)).gpr[3]);
        assert_eq!(r[..4], [0x20, 0x22, 0x24, 0x26]);
        assert_eq!(r[8..12], [0x10, 0x12, 0x14, 0x16]);
    }

    #[test]
    fn padd_psub_in_three_widths_wrap() {
        let a = w4([0x7FFF_FFFF, 1, 0xFFFF_FFFF, 5]);
        let b = w4([1, 2, 1, 5]);
        assert_eq!(op(0x08, 0x00, a, b).gpr[3], w4([0x8000_0000, 3, 0, 10]), "PADDW");
        assert_eq!(op(0x08, 0x01, a, b).gpr[3], w4([0x7FFF_FFFE, 0xFFFF_FFFF, 0xFFFF_FFFE, 0]), "PSUBW");
        let ha = h([0x7FFF, 1, 2, 3, 4, 5, 6, 0xFFFF]);
        let hb = h([1, 1, 1, 1, 1, 1, 1, 1]);
        assert_eq!(op(0x08, 0x04, ha, hb).gpr[3], h([0x8000, 2, 3, 4, 5, 6, 7, 0]), "PADDH");
        assert_eq!(op(0x08, 0x05, ha, hb).gpr[3], h([0x7FFE, 0, 1, 2, 3, 4, 5, 0xFFFE]), "PSUBH");
        let ba = b16([0xFF, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 0x80]);
        let bb = b16([1; 16]);
        assert_eq!(bytes_of(op(0x08, 0x08, ba, bb).gpr[3])[..2], [0, 2], "PADDB");
        assert_eq!(bytes_of(op(0x08, 0x09, ba, bb).gpr[3])[15], 0x7F, "PSUBB");
    }

    #[test]
    fn signed_saturating_adds() {
        let a = w4([0x7FFF_FFFF, 0x8000_0000, 5, 0]);
        let b = w4([1, 0xFFFF_FFFF, 7, 0]);
        assert_eq!(op(0x08, 0x10, a, b).gpr[3], w4([0x7FFF_FFFF, 0x8000_0000, 12, 0]), "PADDSW");
        assert_eq!(op(0x08, 0x11, a, b).gpr[3], w4([0x7FFF_FFFE, 0x8000_0001, 0xFFFF_FFFE, 0]), "PSUBSW");
        let ha = h([0x7FFF, 0x8000, 1, 2, 3, 4, 5, 6]);
        let hb = h([1, 0xFFFF, 1, 1, 1, 1, 1, 1]);
        assert_eq!(op(0x08, 0x14, ha, hb).gpr[3], h([0x7FFF, 0x8000, 2, 3, 4, 5, 6, 7]), "PADDSH");
        assert_eq!(op(0x08, 0x15, ha, hb).gpr[3], h([0x7FFE, 0x8001, 0, 1, 2, 3, 4, 5]), "PSUBSH");
        let ba = b16([0x7F, 0x80, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let bb = b16([1, 0xFF, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(bytes_of(op(0x08, 0x18, ba, bb).gpr[3])[..3], [0x7F, 0x80, 2], "PADDSB");
        assert_eq!(bytes_of(op(0x08, 0x19, ba, bb).gpr[3])[..3], [0x7E, 0x81, 0], "PSUBSB");
    }

    #[test]
    fn unsigned_saturating_adds() {
        let a = w4([0xFFFF_FFFF, 5, 1, 0]);
        let b = w4([1, 7, 2, 0]);
        assert_eq!(op(0x28, 0x10, a, b).gpr[3], w4([0xFFFF_FFFF, 12, 3, 0]), "PADDUW");
        assert_eq!(op(0x28, 0x11, a, b).gpr[3], w4([0xFFFF_FFFE, 0, 0, 0]), "PSUBUW");
        let ha = h([0xFFFF, 1, 2, 3, 4, 5, 6, 7]);
        let hb = h([1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(op(0x28, 0x14, ha, hb).gpr[3], h([0xFFFF, 3, 5, 7, 9, 11, 13, 15]), "PADDUH");
        assert_eq!(op(0x28, 0x15, ha, hb).gpr[3], h([0xFFFE, 0, 0, 0, 0, 0, 0, 0]), "PSUBUH");
        let ba = b16([0xFF, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let bb = b16([1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(bytes_of(op(0x28, 0x18, ba, bb).gpr[3])[..2], [0xFF, 3], "PADDUB");
        assert_eq!(bytes_of(op(0x28, 0x19, ba, bb).gpr[3])[..2], [0xFE, 0], "PSUBUB");
    }

    #[test]
    fn compares_and_min_max() {
        let a = w4([5, 0xFFFF_FFFF, 7, 0]);
        let b = w4([3, 1, 7, 1]);
        assert_eq!(op(0x08, 0x02, a, b).gpr[3], w4([u32::MAX, 0, 0, 0]), "PCGTW signed greater-than");
        assert_eq!(op(0x08, 0x03, a, b).gpr[3], w4([5, 1, 7, 1]), "PMAXW");
        assert_eq!(op(0x28, 0x03, a, b).gpr[3], w4([3, 0xFFFF_FFFF, 7, 0]), "PMINW");
        assert_eq!(op(0x28, 0x02, a, b).gpr[3], w4([0, 0, u32::MAX, 0]), "PCEQW");
        let ha = h([1, 0xFFFF, 3, 4, 5, 6, 7, 8]);
        let hb = h([0, 1, 3, 5, 5, 5, 5, 5]);
        assert_eq!(op(0x08, 0x06, ha, hb).gpr[3], h([0xFFFF, 0, 0, 0, 0, 0xFFFF, 0xFFFF, 0xFFFF]), "PCGTH");
        assert_eq!(op(0x08, 0x07, ha, hb).gpr[3], h([1, 1, 3, 5, 5, 6, 7, 8]), "PMAXH");
        assert_eq!(op(0x28, 0x07, ha, hb).gpr[3], h([0, 0xFFFF, 3, 4, 5, 5, 5, 5]), "PMINH");
        assert_eq!(op(0x28, 0x06, ha, hb).gpr[3], h([0, 0, 0xFFFF, 0, 0xFFFF, 0, 0, 0]), "PCEQH");
        let ba = b16([1, 0xFF, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let bb = b16([0, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(bytes_of(op(0x08, 0x0A, ba, bb).gpr[3])[..3], [0xFF, 0, 0], "PCGTB");
        assert_eq!(bytes_of(op(0x28, 0x0A, ba, bb).gpr[3])[..3], [0, 0, 0xFF], "PCEQB");
    }

    #[test]
    fn absolute_values_saturate() {
        assert_eq!(op(0x28, 0x01, 0, w4([0x8000_0000, 0xFFFF_FFFF, 5, 0])).gpr[3], w4([0x7FFF_FFFF, 1, 5, 0]), "PABSW");
        assert_eq!(op(0x28, 0x05, 0, h([0x8000, 0xFFFF, 5, 0, 0, 0, 0, 0])).gpr[3], h([0x7FFF, 1, 5, 0, 0, 0, 0, 0]), "PABSH");
    }

    #[test]
    fn padsbh_subtracts_low_adds_high() {
        let rs = h([10, 10, 10, 10, 10, 10, 10, 10]);
        let rt = h([1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(op(0x28, 0x04, rs, rt).gpr[3], h([9, 8, 7, 6, 15, 16, 17, 18]));
    }

    #[test]
    fn pext5_and_ppac5_convert_rgba5551() {
        // PEXT5: 5-bit fields to the high bits of bytes, the alpha bit to bit 31.
        let px = 0x8000 | (3 << 10) | (2 << 5) | 1;
        let e = op(0x08, 0x1E, 0, w4([px, 0, 0xFFFF, 0]));
        assert_eq!(words(e.gpr[3])[0], 1 << 3 | 2 << 11 | 3 << 19 | 1 << 31);
        assert_eq!(words(e.gpr[3])[2], 0xF8 | 0xF8 << 8 | 0xF8 << 16 | 1 << 31, "all ones expand to 0xF8 per channel");
        // PPAC5 inverts it for values that survive the truncation.
        let packed = op(0x08, 0x1F, 0, e.gpr[3]);
        assert_eq!(words(packed.gpr[3])[0], px);
        assert_eq!(words(packed.gpr[3])[2], 0xFFFF);
    }

    #[test]
    fn pmfhl_formats() {
        let mut e = Ee::new();
        e.lo = w4([0x1111_0001, 0x2222_0002, 0x3333_0003, 0x4444_0004]);
        e.hi = w4([0xAAAA_0005, 0xBBBB_0006, 0xCCCC_0007, 0xDDDD_0008]);
        let run = |e: &mut Ee, sa: u32| {
            e.execute(mm(0x30, sa, 0, 0, 5)).expect("decodes");
            e.gpr[5]
        };
        assert_eq!(run(&mut e, 0), w4([0x1111_0001, 0xAAAA_0005, 0x3333_0003, 0xCCCC_0007]), "PMFHL.LW");
        assert_eq!(run(&mut e, 1), w4([0x2222_0002, 0xBBBB_0006, 0x4444_0004, 0xDDDD_0008]), "PMFHL.UW");
        // SLW: (HI0:LO0) = 0xAAAA0005_11110001 is a negative 64-bit value below i32::MIN -> 0x80000000.
        let r = run(&mut e, 2);
        assert_eq!(r as u64, 0xFFFF_FFFF_8000_0000);
        assert_eq!((r >> 64) as u64, 0xFFFF_FFFF_8000_0000);
        e.hi = 0;
        e.lo = w4([0x1234_5678, 0, 0xFFFF_FFFF, 0]);
        let r = run(&mut e, 2);
        assert_eq!(r as u64, 0x1234_5678, "fits: low word, sign-extended");
        assert_eq!((r >> 64) as u64, 0x7FFF_FFFF, "(0:0xFFFFFFFF) is positive 2^32-1, saturates to 0x7FFFFFFF");
    }

    #[test]
    fn pmfhl_lh_and_sh() {
        let mut e = Ee::new();
        e.lo = h([0x0001, 0x0002, 0x0003, 0x0004, 0x0005, 0x0006, 0x0007, 0x0008]);
        e.hi = h([0x0011, 0x0012, 0x0013, 0x0014, 0x0015, 0x0016, 0x0017, 0x0018]);
        e.execute(mm(0x30, 3, 0, 0, 5)).unwrap();
        assert_eq!(e.gpr[5], h([0x0001, 0x0003, 0x0011, 0x0013, 0x0005, 0x0007, 0x0015, 0x0017]), "PMFHL.LH");
        // SH: words 0 and 2 saturate to signed 16 bits; the others keep their low halfword.
        e.lo = w4([0x0001_0000 + 0x7000, 0x0000_1234, 0xFFFF_8000 - 0x1000, 0x0000_5678]);
        e.hi = w4([5, 0x0000_9ABC, 0xFFFF_FFFF, 0x0000_DEF0]);
        e.execute(mm(0x30, 4, 0, 0, 5)).unwrap();
        assert_eq!(e.gpr[5], h([0x7FFF, 0x1234, 0x8000, 0x5678, 5, 0x9ABC, 0xFFFF, 0xDEF0]), "PMFHL.SH");
        // PMFHL with an undefined format is unimplemented, not silently ignored.
        assert!(matches!(e.execute(mm(0x30, 7, 0, 0, 5)), Err(Fault::Unimplemented { .. })));
    }

    #[test]
    fn pmthl_lw_scatters_a_register() {
        let mut e = Ee::new();
        e.gpr[1] = w4([1, 2, 3, 4]);
        e.lo = w4([9, 9, 9, 9]);
        e.hi = w4([8, 8, 8, 8]);
        e.execute(mm(0x31, 0, 1, 0, 0)).unwrap();
        assert_eq!(e.lo, w4([1, 9, 3, 9]));
        assert_eq!(e.hi, w4([2, 8, 4, 8]));
    }

    #[test]
    fn pmaddw_pmsubw_pmultw_use_words_0_and_2() {
        let rs = w4([3, 99, 0xFFFF_FFFE, 99]); // 3, -2
        let rt = w4([4, 99, 5, 99]);
        let e = op(0x09, 0x0C, rs, rt); // PMULTW
        assert_eq!(e.gpr[3] as u64, 12);
        assert_eq!((e.gpr[3] >> 64) as u64 as i64, -10);
        assert_eq!(e.lo as u64, 12, "LO.d0 = low word sign-extended");
        assert_eq!((e.lo >> 64) as u64 as i64, -10);
        assert_eq!((e.hi >> 64) as u64, u64::MAX, "HI.d1 = sign extension of the high word");
        // PMADDW accumulates onto (HI:LO).
        let mut e2 = Ee::new();
        e2.gpr[1] = rs;
        e2.gpr[2] = rt;
        e2.lo = 100u128 | 100u128 << 64;
        e2.execute(mm(0x09, 0x00, 1, 2, 3)).unwrap();
        assert_eq!(e2.gpr[3] as u64, 112);
        assert_eq!((e2.gpr[3] >> 64) as u64, 90);
        e2.execute(mm(0x09, 0x04, 1, 2, 3)).unwrap(); // PMSUBW subtracts the products again
        assert_eq!(e2.gpr[3] as u64, 100);
        assert_eq!((e2.gpr[3] >> 64) as u64, 100);
    }

    #[test]
    fn pmaddw_wraps_the_64_bit_accumulator_and_splits_it() {
        let mut e = Ee::new();
        e.gpr[1] = w4([0x7FFF_FFFF, 0, 0, 0]);
        e.gpr[2] = w4([0x7FFF_FFFF, 0, 0, 0]);
        e.execute(mm(0x09, 0x00, 1, 2, 3)).unwrap();
        // 0x7FFFFFFF^2 = 0x3FFF_FFFF_0000_0001: LO = 1, HI = 0x3FFFFFFF.
        assert_eq!(e.gpr[3] as u64, 0x3FFF_FFFF_0000_0001);
        assert_eq!(e.lo as u64, 1);
        assert_eq!(e.hi as u64, 0x3FFF_FFFF);
    }

    #[test]
    fn pmadduw_and_pmultuw_are_unsigned() {
        let rs = w4([0xFFFF_FFFF, 0, 2, 0]);
        let rt = w4([2, 0, 0xFFFF_FFFF, 0]);
        let e = op(0x29, 0x0C, rs, rt); // PMULTUW
        assert_eq!(e.gpr[3] as u64, 0x1_FFFF_FFFE);
        assert_eq!((e.gpr[3] >> 64) as u64, 0x1_FFFF_FFFE);
        assert_eq!(e.lo as u64 as i64, -2, "LO.d0 holds the sign-extended low word");
        assert_eq!(e.hi as u64, 1);
        let mut e2 = Ee::new();
        e2.gpr[1] = rs;
        e2.gpr[2] = rt;
        e2.lo = 1u128;
        e2.execute(mm(0x29, 0x00, 1, 2, 3)).unwrap(); // PMADDUW
        assert_eq!(e2.gpr[3] as u64, 0x1_FFFF_FFFF);
    }

    #[test]
    fn pdivw_and_pdivuw() {
        let e = op(0x09, 0x0D, w4([0xFFFF_FFF9, 0, 100, 0]), w4([2, 0, 0xFFFF_FFF6, 0])); // -7/2, 100/-10
        assert_eq!(e.lo as u64 as i64, -3);
        assert_eq!(e.hi as u64 as i64, -1);
        assert_eq!((e.lo >> 64) as u64 as i64, -10);
        assert_eq!((e.hi >> 64) as u64, 0);
        // Divide by zero: quotient is -1 (1 for a negative dividend), remainder the dividend.
        let z = op(0x09, 0x0D, w4([5, 0, 0xFFFF_FFFB, 0]), 0);
        assert_eq!(z.lo as u64 as i64, -1);
        assert_eq!((z.lo >> 64) as u64, 1);
        assert_eq!(z.hi as u64, 5);
        // i32::MIN / -1 does not trap.
        let m = op(0x09, 0x0D, w4([0x8000_0000, 0, 0, 0]), w4([0xFFFF_FFFF, 0, 1, 0]));
        assert_eq!(m.lo as u64, 0xFFFF_FFFF_8000_0000);
        assert_eq!(m.hi as u64, 0);
        let u = op(0x29, 0x0D, w4([100, 0, 7, 0]), w4([7, 0, 0, 0]));
        assert_eq!(u.lo as u64, 14);
        assert_eq!(u.hi as u64, 2);
        assert_eq!((u.lo >> 64) as u64 as i64, -1, "unsigned divide by zero: all ones");
        assert_eq!((u.hi >> 64) as u64, 7);
    }

    #[test]
    fn variable_shifts_of_two_words() {
        let rs = w4([4, 0, 33, 0]); // shift counts use the low 5 bits: 4 and 1
        let rt = w4([0x8000_0001, 0, 0x8000_0002, 0]);
        let e = op(0x09, 0x02, rs, rt); // PSLLVW
        assert_eq!(e.gpr[3] as u64, 0x10);
        assert_eq!((e.gpr[3] >> 64) as u64, 4);
        let e = op(0x09, 0x03, rs, rt); // PSRLVW
        assert_eq!(e.gpr[3] as u64, 0x0800_0000);
        assert_eq!((e.gpr[3] >> 64) as u64, 0x4000_0001);
        let e = op(0x29, 0x03, rs, rt); // PSRAVW
        assert_eq!(e.gpr[3] as u64, 0xFFFF_FFFF_F800_0000);
        assert_eq!((e.gpr[3] >> 64) as u64, 0xFFFF_FFFF_C000_0001);
    }

    #[test]
    fn halfword_multiply_accumulate_layout() {
        // PMULTH: LO.w0,w1,w2,w3 = products of halfwords 0,1,4,5; HI.w0..w3 = products of 2,3,6,7;
        // rd = {LO.w0, HI.w0, LO.w2, HI.w2}.
        let rs = h([1, 2, 3, 4, 5, 6, 7, 8]);
        let rt = h([10, 10, 10, 10, 10, 10, 10, 10]);
        let e = op(0x09, 0x1C, rs, rt);
        assert_eq!(e.lo, w4([10, 20, 50, 60]));
        assert_eq!(e.hi, w4([30, 40, 70, 80]));
        assert_eq!(e.gpr[3], w4([10, 30, 50, 70]));
        // PMADDH adds, PMSUBH subtracts; negative halfwords multiply signed.
        let mut e2 = Ee::new();
        e2.gpr[1] = h([0xFFFF, 0, 0, 0, 0, 0, 0, 0]);
        e2.gpr[2] = h([3, 0, 0, 0, 0, 0, 0, 0]);
        e2.lo = w4([100, 0, 0, 0]);
        e2.execute(mm(0x09, 0x10, 1, 2, 3)).unwrap();
        assert_eq!(words(e2.lo)[0], 97, "PMADDH: 100 + (-1 * 3)");
        assert_eq!(words(e2.gpr[3])[0], 97);
        e2.execute(mm(0x09, 0x14, 1, 2, 3)).unwrap();
        assert_eq!(words(e2.lo)[0], 100, "PMSUBH");
    }

    #[test]
    fn permutations() {
        let a = h([0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(op(0x09, 0x1A, 0, a).gpr[3], h([2, 1, 0, 3, 6, 5, 4, 7]), "PEXEH");
        assert_eq!(op(0x09, 0x1B, 0, a).gpr[3], h([3, 2, 1, 0, 7, 6, 5, 4]), "PREVH");
        assert_eq!(op(0x29, 0x1A, 0, a).gpr[3], h([0, 2, 1, 3, 4, 6, 5, 7]), "PEXCH");
        assert_eq!(op(0x29, 0x1B, 0, a).gpr[3], h([0, 0, 0, 0, 4, 4, 4, 4]), "PCPYH");
        let rs = h([10, 11, 12, 13, 14, 15, 16, 17]);
        assert_eq!(op(0x09, 0x0A, rs, a).gpr[3], h([0, 14, 1, 15, 2, 16, 3, 17]), "PINTH");
        assert_eq!(op(0x29, 0x0A, rs, a).gpr[3], h([0, 10, 2, 12, 4, 14, 6, 16]), "PINTEH");
        let wv = w4([1, 2, 3, 4]);
        assert_eq!(op(0x09, 0x1E, 0, wv).gpr[3], w4([3, 2, 1, 4]), "PEXEW");
        assert_eq!(op(0x09, 0x1F, 0, wv).gpr[3], w4([2, 3, 1, 4]), "PROT3W");
        assert_eq!(op(0x29, 0x1E, 0, wv).gpr[3], w4([1, 3, 2, 4]), "PEXCW");
    }

    #[test]
    fn halfword_shifts_use_four_bit_counts() {
        let v = h([0x8001, 2, 3, 4, 5, 6, 7, 8]);
        let mut e = Ee::new();
        e.gpr[2] = v;
        e.execute(mm(0x34, 4, 0, 2, 3)).unwrap(); // PSLLH by 4
        assert_eq!(e.gpr[3], h([0x0010, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80]));
        e.execute(mm(0x37, 4, 0, 2, 3)).unwrap(); // PSRAH by 4: arithmetic
        assert_eq!(halves(e.gpr[3])[0], 0xF800);
        e.execute(mm(0x36, 4, 0, 2, 3)).unwrap(); // PSRLH by 4: logical
        assert_eq!(halves(e.gpr[3])[0], 0x0800);
        // A shift amount with bit 4 set uses only the low four bits (no overflow panic).
        e.execute(mm(0x34, 17, 0, 2, 3)).unwrap();
        assert_eq!(halves(e.gpr[3])[1], 4);
    }

    #[test]
    fn unused_halfword_horizontals_stay_unimplemented() {
        for sa in [0x11, 0x15, 0x1D] {
            let mut e = Ee::new();
            assert!(matches!(e.execute(mm(0x09, sa, 1, 2, 3)), Err(Fault::Unimplemented { what: "mmi2", .. })), "sa {sa:#x}");
        }
    }

    #[test]
    fn mmi_move_hi_lo_pipe_one_and_the_rest_still_work() {
        let mut e = Ee::new();
        e.gpr[1] = 0x1234_5678_9ABC_DEF0_1111_2222_3333_4444;
        e.execute(mm(0x11, 0, 1, 0, 0)).unwrap(); // MTHI1
        e.execute(mm(0x10, 0, 0, 0, 4)).unwrap(); // MFHI1
        assert_eq!(e.gpr[4] as u64, 0x1111_2222_3333_4444, "MTHI1 copies the low half of rs to HI[127:64]");
        e.gpr[5] = 0x1_0000_0000 + 7;
        e.gpr[6] = 0xFFFF_FFFF_FFFF_FFF0; // low word -16
        e.execute(mm(0x1A, 0, 5, 6, 0)).unwrap(); // DIV1: 7 / -16
        assert_eq!((e.lo >> 64) as u64, 0);
        assert_eq!((e.hi >> 64) as u64, 7);
        e.gpr[7] = 0xFFFF_FFF0;
        e.execute(mm(0x04, 0, 7, 0, 8)).unwrap(); // PLZCW: leading sign bits minus one
        assert_eq!(e.gpr[8] as u32, 27);
    }
}
