//! Instruction execution: the R5900 integer core and the COP1 FPU. Encodings: R5900 user manual
//! (core, COP1). MMI is in `mmi.rs`, COP2 in `vu.rs`, COP0 in `cop0.rs`.

use super::{Ee, Exception, clamp, sx32};

pub(crate) fn words(v: u128) -> [u32; 4] {
    std::array::from_fn(|i| (v >> (32 * i)) as u32)
}
pub(crate) fn from_words(w: [u32; 4]) -> u128 {
    w.iter().enumerate().fold(0, |a, (i, &x)| a | (x as u128) << (32 * i))
}
pub(crate) fn halves(v: u128) -> [u16; 8] {
    std::array::from_fn(|i| (v >> (16 * i)) as u16)
}
pub(crate) fn from_halves(h: [u16; 8]) -> u128 {
    h.iter().enumerate().fold(0, |a, (i, &x)| a | (x as u128) << (16 * i))
}
pub(crate) fn bytes(v: u128) -> [u8; 16] {
    v.to_le_bytes()
}
pub(crate) fn from_bytes(b: [u8; 16]) -> u128 {
    u128::from_le_bytes(b)
}
pub(crate) fn vf_bits(v: [f32; 4]) -> u128 {
    from_words(v.map(f32::to_bits))
}
pub(crate) fn bits_vf(v: u128) -> [f32; 4] {
    words(v).map(f32::from_bits)
}
fn shl(x: u64, s: u32) -> u64 {
    x.checked_shl(s).unwrap_or(0)
}
fn shr(x: u64, s: u32) -> u64 {
    x.checked_shr(s).unwrap_or(0)
}
pub(crate) fn lo64(v: u128) -> u64 {
    v as u64
}
pub(crate) fn with_lo64(v: u128, x: u64) -> u128 {
    (v & !(u64::MAX as u128)) | x as u128
}
pub(crate) fn with_hi64(v: u128, x: u64) -> u128 {
    (v & u64::MAX as u128) | (x as u128) << 64
}

impl Ee {
    /// Raises an address error and returns true if `addr` is not aligned to `mask + 1`.
    #[inline(always)]
    fn misaligned(&mut self, addr: u32, mask: u32, store: bool) -> bool {
        if addr & mask == 0 {
            return false;
        }
        self.cop0[8] = addr;
        self.raise(Exception::AddressError { addr, store });
        true
    }

    pub(crate) fn exec(&mut self, w: u32) {
        let op = w >> 26;
        let rs = ((w >> 21) & 31) as usize;
        let rt = ((w >> 16) & 31) as usize;
        let rd = ((w >> 11) & 31) as usize;
        let sa = (w >> 6) & 31;
        let funct = w & 63;
        let imm = w as u16;
        let simm = imm as i16 as i64 as u64;
        let addr = self.g32(rs).wrapping_add(imm as i16 as i32 as u32);
        let pc4 = self.pc.wrapping_add(4);
        let bt = pc4.wrapping_add(((imm as i16 as i32) << 2) as u32);
        let (a, b) = (self.g(rs), self.g(rt));
        match op {
            0x00 => self.special(w, rs, rt, rd, sa, funct),
            0x01 => {
                let s = a as i64;
                match rt {
                    0x00 => self.branch(s < 0, bt, false),
                    0x01 => self.branch(s >= 0, bt, false),
                    0x02 => self.branch(s < 0, bt, true),
                    0x03 => self.branch(s >= 0, bt, true),
                    // Trap on compare with an immediate: tgei tgeiu tlti tltiu teqi tnei.
                    0x08 => self.trap(s >= simm as i64),
                    0x09 => self.trap(a >= simm),
                    0x0A => self.trap(s < simm as i64),
                    0x0B => self.trap(a < simm),
                    0x0C => self.trap(a == simm),
                    0x0E => self.trap(a != simm),
                    0x10..=0x13 => {
                        let cond = if rt & 1 == 0 { s < 0 } else { s >= 0 };
                        self.set(31, sx32(self.pc.wrapping_add(8)));
                        self.branch(cond, bt, rt >= 0x12);
                    }
                    0x18 => self.sa = ((a as u32 & 15) ^ (imm as u32 & 15)) * 8,
                    0x19 => self.sa = ((a as u32 & 7) ^ (imm as u32 & 7)) * 16,
                    _ => self.unimpl(w, "regimm"),
                }
            }
            0x02 => self.npc_next = Some((pc4 & 0xF000_0000) | ((w & 0x03FF_FFFF) << 2)),
            0x03 => {
                self.set(31, sx32(self.pc.wrapping_add(8)));
                self.npc_next = Some((pc4 & 0xF000_0000) | ((w & 0x03FF_FFFF) << 2));
            }
            0x04 => self.branch(a == b, bt, false),
            0x05 => self.branch(a != b, bt, false),
            0x06 => self.branch(a as i64 <= 0, bt, false),
            0x07 => self.branch(a as i64 > 0, bt, false),
            0x08 => match (a as u32 as i32).checked_add(simm as i32) {
                Some(v) => self.set(rt, sx32(v as u32)),
                None => self.raise(Exception::Overflow),
            },
            0x09 => self.set(rt, sx32((a as u32).wrapping_add(simm as u32))),
            0x0A => self.set(rt, ((a as i64) < simm as i64) as u64),
            0x0B => self.set(rt, (a < simm) as u64),
            0x0C => self.set(rt, a & imm as u64),
            0x0D => self.set(rt, a | imm as u64),
            0x0E => self.set(rt, a ^ imm as u64),
            0x0F => self.set(rt, sx32((imm as u32) << 16)),
            0x10 => self.cop0_op(w, rs, rt, rd, funct),
            0x11 => self.cop1(w, rs, rt, rd, sa as usize, funct),
            0x12 => self.cop2(w, rs, rt, rd),
            0x14 => self.branch(a == b, bt, true),
            0x15 => self.branch(a != b, bt, true),
            0x16 => self.branch(a as i64 <= 0, bt, true),
            0x17 => self.branch(a as i64 > 0, bt, true),
            0x18 => match (a as i64).checked_add(simm as i64) {
                Some(v) => self.set(rt, v as u64),
                None => self.raise(Exception::Overflow),
            },
            0x19 => self.set(rt, a.wrapping_add(simm)),
            0x1A | 0x1B => {
                let k = addr & 7;
                let m = self.r64(addr & !7);
                let v = if op == 0x1A {
                    let s = (7 - k) * 8;
                    (b & shl(1, s).wrapping_sub(1)) | shl(m, s)
                } else {
                    let s = k * 8;
                    (b & !shr(u64::MAX, s)) | shr(m, s)
                };
                self.set(rt, v);
            }
            0x1C => self.mmi(w, rs, rt, rd, sa, funct),
            0x1E => {
                let v = self.r128(addr & !15);
                self.set128(rt, v);
            }
            0x1F => {
                let v = self.gpr[rt];
                self.write(addr & !15, &v.to_le_bytes());
            }
            0x20 => {
                let v = self.r8(addr) as i8 as i64 as u64;
                self.set(rt, v)
            }
            0x21 => {
                if self.misaligned(addr, 1, false) {
                    return;
                }
                let v = self.r16(addr) as i16 as i64 as u64;
                self.set(rt, v)
            }
            0x22 | 0x26 => {
                let k = addr & 3;
                let m = self.r32(addr & !3) as u64;
                let old = b as u32 as u64;
                if op == 0x22 {
                    let s = (3 - k) * 8;
                    self.set(rt, sx32(((old & ((1u64 << s) - 1)) | (m << s)) as u32));
                } else {
                    let s = k * 8;
                    let v = ((old & !(0xFFFF_FFFFu64 >> s)) | (m >> s)) as u32;
                    // A full-word LWR sign-extends; a partial one keeps the upper half.
                    self.set(rt, if k == 0 { sx32(v) } else { (b & !0xFFFF_FFFF) | v as u64 });
                }
            }
            0x23 => {
                if self.misaligned(addr, 3, false) {
                    return;
                }
                let v = sx32(self.r32(addr));
                self.set(rt, v)
            }
            0x24 => {
                let v = self.r8(addr) as u64;
                self.set(rt, v)
            }
            0x25 => {
                if self.misaligned(addr, 1, false) {
                    return;
                }
                let v = self.r16(addr) as u64;
                self.set(rt, v)
            }
            0x27 => {
                if self.misaligned(addr, 3, false) {
                    return;
                }
                let v = self.r32(addr) as u64;
                self.set(rt, v)
            }
            0x28 => self.write(addr, &[b as u8]),
            0x29 => {
                if self.misaligned(addr, 1, true) {
                    return;
                }
                self.write(addr, &(b as u16).to_le_bytes())
            }
            0x2A | 0x2E => {
                let k = addr & 3;
                let m = self.r32(addr & !3) as u64;
                let r = b as u32 as u64;
                let v = if op == 0x2A {
                    let s = (3 - k) * 8;
                    (m & !(0xFFFF_FFFFu64 >> s) & 0xFFFF_FFFF) | (r >> s)
                } else {
                    let s = k * 8;
                    (m & ((1u64 << s) - 1)) | ((r << s) & 0xFFFF_FFFF)
                };
                self.w32(addr & !3, v as u32);
            }
            0x2B => {
                if self.misaligned(addr, 3, true) {
                    return;
                }
                self.w32(addr, b as u32)
            }
            0x2C | 0x2D => {
                let k = addr & 7;
                let m = self.r64(addr & !7);
                let v = if op == 0x2C {
                    let s = (7 - k) * 8;
                    (m & !shr(u64::MAX, s)) | shr(b, s)
                } else {
                    let s = k * 8;
                    (m & shl(1, s).wrapping_sub(1)) | shl(b, s)
                };
                self.write(addr & !7, &v.to_le_bytes());
            }
            0x2F | 0x33 => {} // cache, pref
            0x31 => {
                if self.misaligned(addr, 3, false) {
                    return;
                }
                self.fpr[rt] = f32::from_bits(self.r32(addr))
            }
            0x36 => {
                let v = self.r128(addr & !15);
                if rt != 0 {
                    self.vf[rt] = bits_vf(v);
                }
            }
            0x37 => {
                if self.misaligned(addr, 7, false) {
                    return;
                }
                let v = self.r64(addr);
                self.set(rt, v)
            }
            0x39 => {
                if self.misaligned(addr, 3, true) {
                    return;
                }
                self.w32(addr, self.fpr[rt].to_bits())
            }
            0x3E => {
                let v = vf_bits(self.vf[rt]);
                self.write(addr & !15, &v.to_le_bytes());
            }
            0x3F => {
                if self.misaligned(addr, 7, true) {
                    return;
                }
                self.write(addr, &b.to_le_bytes())
            }
            _ => self.unimpl(w, "opcode"),
        }
    }

    fn trap(&mut self, cond: bool) {
        if cond {
            self.raise(Exception::Trap);
        }
    }

    fn special(&mut self, w: u32, rs: usize, rt: usize, rd: usize, sa: u32, funct: u32) {
        let (a, b) = (self.g(rs), self.g(rt));
        let (a32, b32) = (a as u32, b as u32);
        match funct {
            0x00 => self.set(rd, sx32(b32 << sa)),
            0x02 => self.set(rd, sx32(b32 >> sa)),
            0x03 => self.set(rd, sx32(((b32 as i32) >> sa) as u32)),
            0x04 => self.set(rd, sx32(b32 << (a32 & 31))),
            0x06 => self.set(rd, sx32(b32 >> (a32 & 31))),
            0x07 => self.set(rd, sx32(((b32 as i32) >> (a32 & 31)) as u32)),
            0x08 => self.npc_next = Some(a32),
            0x09 => {
                self.set(rd, sx32(self.pc.wrapping_add(8)));
                self.npc_next = Some(a32);
            }
            0x0A => {
                if b == 0 {
                    self.set(rd, a)
                }
            }
            0x0B => {
                if b != 0 {
                    self.set(rd, a)
                }
            }
            // The syscall number is in v1 (the SDK passes negative numbers for interrupt context).
            0x0C => self.raise(Exception::Syscall(self.g32(super::reg::V1) as i32)),
            0x0D => self.raise(Exception::Break((w >> 6) & 0xF_FFFF)),
            0x0F => {} // sync
            0x10 => self.set(rd, lo64(self.hi)),
            0x11 => self.hi = with_lo64(self.hi, a),
            0x12 => self.set(rd, lo64(self.lo)),
            0x13 => self.lo = with_lo64(self.lo, a),
            0x14 => self.set(rd, b << (a32 & 63)),
            0x16 => self.set(rd, b >> (a32 & 63)),
            0x17 => self.set(rd, ((b as i64) >> (a32 & 63)) as u64),
            0x18 | 0x19 => {
                let p = if funct == 0x18 { (a32 as i32 as i64).wrapping_mul(b32 as i32 as i64) as u64 } else { a32 as u64 * b32 as u64 };
                self.lo = with_lo64(self.lo, sx32(p as u32));
                self.hi = with_lo64(self.hi, sx32((p >> 32) as u32));
                self.set(rd, sx32(p as u32));
            }
            0x1A | 0x1B => self.divide(a32, b32, funct == 0x1A, false),
            0x20 => match (a32 as i32).checked_add(b32 as i32) {
                Some(v) => self.set(rd, sx32(v as u32)),
                None => self.raise(Exception::Overflow),
            },
            0x21 => self.set(rd, sx32(a32.wrapping_add(b32))),
            0x22 => match (a32 as i32).checked_sub(b32 as i32) {
                Some(v) => self.set(rd, sx32(v as u32)),
                None => self.raise(Exception::Overflow),
            },
            0x23 => self.set(rd, sx32(a32.wrapping_sub(b32))),
            0x24 => self.set(rd, a & b),
            0x25 => self.set(rd, a | b),
            0x26 => self.set(rd, a ^ b),
            0x27 => self.set(rd, !(a | b)),
            0x28 => self.set(rd, self.sa as u64),
            0x29 => self.sa = a32,
            0x2A => self.set(rd, ((a as i64) < (b as i64)) as u64),
            0x2B => self.set(rd, (a < b) as u64),
            0x2C => match (a as i64).checked_add(b as i64) {
                Some(v) => self.set(rd, v as u64),
                None => self.raise(Exception::Overflow),
            },
            0x2D => self.set(rd, a.wrapping_add(b)),
            0x2E => match (a as i64).checked_sub(b as i64) {
                Some(v) => self.set(rd, v as u64),
                None => self.raise(Exception::Overflow),
            },
            0x2F => self.set(rd, a.wrapping_sub(b)),
            // tge tgeu tlt tltu teq tne
            0x30 => self.trap(a as i64 >= b as i64),
            0x31 => self.trap(a >= b),
            0x32 => self.trap((a as i64) < b as i64),
            0x33 => self.trap(a < b),
            0x34 => self.trap(a == b),
            0x36 => self.trap(a != b),
            0x38 => self.set(rd, b << sa),
            0x3A => self.set(rd, b >> sa),
            0x3B => self.set(rd, ((b as i64) >> sa) as u64),
            0x3C => self.set(rd, b << (sa + 32)),
            0x3E => self.set(rd, b >> (sa + 32)),
            0x3F => self.set(rd, ((b as i64) >> (sa + 32)) as u64),
            _ => self.unimpl(w, "special"),
        }
    }

    pub(crate) fn divide(&mut self, a: u32, b: u32, signed: bool, pipe1: bool) {
        let (q, r) = if signed {
            let (x, y) = (a as i32, b as i32);
            if y == 0 {
                (if x < 0 { 1 } else { -1i32 } as u32, a)
            } else {
                (x.wrapping_div(y) as u32, x.wrapping_rem(y) as u32)
            }
        } else if let Some(q) = a.checked_div(b) {
            (q, a % b)
        } else {
            (u32::MAX, a)
        };
        if pipe1 {
            self.lo = with_hi64(self.lo, sx32(q));
            self.hi = with_hi64(self.hi, sx32(r));
        } else {
            self.lo = with_lo64(self.lo, sx32(q));
            self.hi = with_lo64(self.hi, sx32(r));
        }
    }

    fn cop1(&mut self, w: u32, fmt: usize, ft: usize, fs: usize, fd: usize, funct: u32) {
        const C: u32 = 1 << 23;
        match fmt {
            0x00 => self.set(ft, sx32(self.fpr[fs].to_bits())),
            0x02 => self.set(ft, sx32(if fs == 31 { self.fcr31 } else { 0x2E30 })),
            0x04 => self.fpr[fs] = f32::from_bits(self.g32(ft)),
            0x06 => {
                if fs == 31 {
                    self.fcr31 = self.g32(ft)
                }
            }
            0x08 => {
                let c = self.fcr31 & C != 0;
                let bt = self.pc.wrapping_add(4).wrapping_add(((w as u16 as i16 as i32) << 2) as u32);
                match ft {
                    0 => self.branch(!c, bt, false),
                    1 => self.branch(c, bt, false),
                    2 => self.branch(!c, bt, true),
                    3 => self.branch(c, bt, true),
                    _ => self.unimpl(w, "bc1"),
                }
            }
            0x10 => {
                let (s, t) = (self.fpr[fs], self.fpr[ft]);
                let cmp = |e: &mut Ee, c: bool| e.fcr31 = if c { e.fcr31 | C } else { e.fcr31 & !C };
                match funct {
                    0x00 => self.fpr[fd] = clamp(s + t),
                    0x01 => self.fpr[fd] = clamp(s - t),
                    0x02 => self.fpr[fd] = clamp(s * t),
                    0x03 => self.fpr[fd] = clamp(if t == 0.0 { f32::MAX.copysign(s) * t.signum() } else { s / t }),
                    0x04 => self.fpr[fd] = t.abs().sqrt(),
                    0x05 => self.fpr[fd] = s.abs(),
                    0x06 => self.fpr[fd] = s,
                    0x07 => self.fpr[fd] = -s,
                    0x16 => self.fpr[fd] = clamp(s / t.abs().sqrt()),
                    0x18 => self.facc = clamp(s + t),
                    0x19 => self.facc = clamp(s - t),
                    0x1A => self.facc = clamp(s * t),
                    0x1C => self.fpr[fd] = clamp(self.facc + s * t),
                    0x1D => self.fpr[fd] = clamp(self.facc - s * t),
                    0x1E => self.facc = clamp(self.facc + s * t),
                    0x1F => self.facc = clamp(self.facc - s * t),
                    0x24 => self.fpr[fd] = f32::from_bits(s as i32 as u32),
                    0x28 => self.fpr[fd] = s.max(t),
                    0x29 => self.fpr[fd] = s.min(t),
                    0x30 => cmp(self, false),
                    0x32 => cmp(self, s == t),
                    0x34 => cmp(self, s < t),
                    0x36 => cmp(self, s <= t),
                    _ => self.unimpl(w, "cop1.s"),
                }
            }
            0x14 if funct == 0x20 => self.fpr[fd] = self.fpr[fs].to_bits() as i32 as f32,
            _ => self.unimpl(w, "cop1"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Fault;

    fn asm_r(op: u32, rs: u32, rt: u32, rd: u32, sa: u32, funct: u32) -> u32 {
        (op << 26) | (rs << 21) | (rt << 16) | (rd << 11) | (sa << 6) | funct
    }
    fn asm_i(op: u32, rs: u32, rt: u32, imm: u16) -> u32 {
        (op << 26) | (rs << 21) | (rt << 16) | imm as u32
    }

    fn run(code: &[u32]) -> Ee {
        let mut e = Ee::new();
        let base = 0x0010_0000u32;
        for (i, w) in code.iter().enumerate() {
            e.w32(base + 4 * i as u32, *w);
        }
        // jr ra; nop
        e.w32(base + 4 * code.len() as u32, asm_r(0, 31, 0, 0, 0, 8));
        e.w32(base + 4 * code.len() as u32 + 4, 0);
        e.call(base, &[], &[], 0x01F0_0000).unwrap();
        e
    }

    /// Runs `code` and returns the fault it ended with.
    fn run_fault(code: &[u32]) -> Fault {
        let mut e = Ee::new();
        let base = 0x0010_0000u32;
        for (i, w) in code.iter().enumerate() {
            e.w32(base + 4 * i as u32, *w);
        }
        e.w32(base + 4 * code.len() as u32, asm_r(0, 31, 0, 0, 0, 8));
        e.call(base, &[], &[], 0x01F0_0000).unwrap_err()
    }

    #[test]
    fn integer_and_branch_delay() {
        // addiu v0, zero, 5; beq zero, zero, +2; addiu v0, v0, 1 (delay slot); addiu v0, v0, 100; addiu v0, v0, 10
        let e = run(&[asm_i(9, 0, 2, 5), asm_i(4, 0, 0, 2), asm_i(9, 2, 2, 1), asm_i(9, 2, 2, 100), asm_i(9, 2, 2, 10)]);
        assert_eq!(e.gpr[2] as u64, 16);
    }

    #[test]
    fn branch_likely_nullifies() {
        // addiu v0,zero,1; beql zero,v0,+1 (not taken); addiu v0,v0,7 (nullified); addiu v0,v0,2
        let e = run(&[asm_i(9, 0, 2, 1), asm_i(0x14, 0, 2, 1), asm_i(9, 2, 2, 7), asm_i(9, 2, 2, 2)]);
        assert_eq!(e.gpr[2] as u64, 3);
    }

    #[test]
    fn syscall_without_handler_faults_with_the_number() {
        // R5900 manual, SYSCALL: the exception code is Sys; the SDK puts the number in v1.
        let f = run_fault(&[asm_i(9, 0, 3, 0xFFFE), asm_r(0, 0, 0, 0, 0, 0x0C)]); // v1 = -2
        assert!(matches!(f, Fault::Exception { exc: Exception::Syscall(-2), .. }), "{f:?}");
    }

    #[test]
    fn syscall_table_runs_the_host_handler_and_continues() {
        use std::cell::Cell;
        use std::rc::Rc;
        let mut e = Ee::new();
        let seen = Rc::new(Cell::new(0u64));
        let s2 = seen.clone();
        // The handler reads a0 and returns a0 + 1 in v0; the program counter is already past the syscall.
        e.syscalls.insert(0x3C, move |ee: &mut Ee| {
            s2.set(ee.gpr[4] as u64);
            ee.gpr[2] = ee.gpr[4] + 1;
        });
        let base = 0x0010_0000u32;
        for (i, w) in [asm_i(9, 0, 3, 0x3C), asm_i(9, 0, 4, 41), asm_r(0, 0, 0, 0, 0, 0x0C), asm_i(9, 2, 2, 100), asm_r(0, 31, 0, 0, 0, 8), 0].iter().enumerate() {
            e.w32(base + 4 * i as u32, *w);
        }
        let v0 = e.call(base, &[], &[], 0x01F0_0000).unwrap();
        assert_eq!(seen.get(), 41);
        assert_eq!(v0, 142, "the instruction after the syscall ran with the handler's v0");
    }

    #[test]
    fn syscall_handler_may_switch_context() {
        use crate::Stop;
        let mut e = Ee::new();
        let base = 0x0010_0000u32;
        // Thread A: v1 = 7; syscall; (never resumed here). Thread B at base + 0x100: v0 = 99; jr ra.
        for (i, w) in [asm_i(9, 0, 3, 7), asm_r(0, 0, 0, 0, 0, 0x0C), asm_i(9, 0, 2, 1), asm_r(0, 31, 0, 0, 0, 8), 0].iter().enumerate() {
            e.w32(base + 4 * i as u32, *w);
        }
        for (i, w) in [asm_i(9, 0, 2, 99), asm_r(0, 31, 0, 0, 0, 8), 0].iter().enumerate() {
            e.w32(base + 0x100 + 4 * i as u32, *w);
        }
        e.syscalls.insert(7, move |ee: &mut Ee| {
            let mut c = ee.context();
            c.pc = 0x0010_0100;
            c.npc = 0x0010_0104;
            ee.set_context(&c);
        });
        e.gpr[31] = crate::SENTINEL as u128;
        e.pc = base;
        e.npc = base + 4;
        assert_eq!(e.run(100).unwrap(), Stop::Returned);
        assert_eq!(e.gpr[2] as u64, 99, "execution went to the other thread");
    }

    #[test]
    fn break_reports_its_code() {
        // gcc's divide-by-zero check is `break 7` (code in bits 25:6).
        let f = run_fault(&[asm_r(0, 0, 0, 0, 0, 0x0D) | (7 << 6)]);
        assert!(matches!(f, Fault::Exception { exc: Exception::Break(7), .. }), "{f:?}");
    }

    #[test]
    fn misaligned_accesses_raise_address_errors() {
        // R5900 manual, load/store: lw/sw need 4, lh/sh 2, ld/sd 8 byte alignment; lq/sq ignore the low bits.
        let cases = [
            (asm_i(0x23, 4, 2, 0), false), // lw
            (asm_i(0x2B, 4, 2, 0), true),  // sw
            (asm_i(0x21, 4, 2, 0), false), // lh
            (asm_i(0x29, 4, 2, 0), true),  // sh
            (asm_i(0x37, 4, 2, 0), false), // ld
            (asm_i(0x3F, 4, 2, 0), true),  // sd
        ];
        for (word, store) in cases {
            let mut e = Ee::new();
            let base = 0x0010_0000u32;
            e.w32(base, word);
            let r = e.call(base, &[0x0020_0001], &[], 0x01F0_0000);
            let Err(Fault::Exception { exc: Exception::AddressError { addr, store: s }, pc }) = r else { panic!("{word:08x}: {r:?}") };
            assert_eq!((addr, s, pc), (0x0020_0001, store, base), "{word:08x}");
            assert_eq!(e.cop0[8], 0x0020_0001, "BadVAddr");
        }
        // Aligned halfword and quadword accesses are fine; lq with a low-bit address truncates.
        let mut e = Ee::new();
        e.w32(0x0020_0000, 0xAABB_CCDD);
        e.w32(0x0010_0000, asm_i(0x1E, 4, 2, 0));
        e.w32(0x0010_0004, asm_r(0, 31, 0, 0, 0, 8));
        e.call(0x0010_0000, &[0x0020_0007], &[], 0x01F0_0000).unwrap();
        assert_eq!(e.gpr[2] as u32, 0xAABB_CCDD);
    }

    #[test]
    fn misaligned_jump_target_raises_an_address_error_on_fetch() {
        let mut e = Ee::new();
        let base = 0x0010_0000u32;
        e.w32(base, asm_r(0, 4, 0, 0, 0, 8)); // jr a0
        let r = e.call(base, &[0x0010_0102], &[], 0x01F0_0000);
        assert!(matches!(r, Err(Fault::Exception { exc: Exception::AddressError { addr: 0x0010_0102, store: false }, .. })), "{r:?}");
    }

    #[test]
    fn signed_overflow_traps_but_unsigned_forms_wrap() {
        // add/addi/sub raise Overflow (ExcCode 12); addu/addiu/subu wrap.
        let mut e = Ee::new();
        let base = 0x0010_0000u32;
        e.w32(base, asm_r(0, 4, 5, 2, 0, 0x20)); // add v0, a0, a1
        let r = e.call(base, &[0x7FFF_FFFF, 1], &[], 0x01F0_0000);
        assert!(matches!(r, Err(Fault::Exception { exc: Exception::Overflow, .. })), "{r:?}");
        e.w32(base, asm_r(0, 4, 5, 2, 0, 0x21)); // addu
        e.w32(base + 4, asm_r(0, 31, 0, 0, 0, 8));
        e.call(base, &[0x7FFF_FFFF, 1], &[], 0x01F0_0000).unwrap();
        assert_eq!(e.gpr[2] as u64, 0xFFFF_FFFF_8000_0000);
        e.w32(base, asm_i(8, 4, 2, 1)); // addi v0, a0, 1
        let r = e.call(base, &[0x7FFF_FFFF], &[], 0x01F0_0000);
        assert!(matches!(r, Err(Fault::Exception { exc: Exception::Overflow, .. })), "{r:?}");
        e.w32(base, asm_r(0, 4, 5, 2, 0, 0x22)); // sub
        let r = e.call(base, &[0x8000_0000u64 as u32 as i32 as i64 as u64, 1], &[], 0x01F0_0000);
        assert!(matches!(r, Err(Fault::Exception { exc: Exception::Overflow, .. })), "{r:?}");
    }

    #[test]
    fn conditional_traps_fire_on_their_condition() {
        // teq a0, a1 (funct 0x34) and tne (0x36).
        let mut e = Ee::new();
        let base = 0x0010_0000u32;
        e.w32(base, asm_r(0, 4, 5, 0, 0, 0x34));
        e.w32(base + 4, asm_r(0, 31, 0, 0, 0, 8));
        assert!(e.call(base, &[3, 4], &[], 0x01F0_0000).is_ok());
        let r = e.call(base, &[3, 3], &[], 0x01F0_0000);
        assert!(matches!(r, Err(Fault::Exception { exc: Exception::Trap, .. })), "{r:?}");
    }

    #[test]
    fn closures_capture_state_and_hooks_return_to_ra() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let mut e = Ee::new();
        let log = Rc::new(RefCell::new(Vec::new()));
        let l2 = log.clone();
        // The hook at 0x100800 stands in for a function: it logs a0 and returns a0 * 2.
        e.hooks.insert(0x0010_0800, move |ee: &mut Ee| {
            l2.borrow_mut().push(ee.gpr[4] as u64);
            ee.gpr[2] = ee.gpr[4] * 2;
        });
        let base = 0x0010_0000u32;
        // addiu sp,sp,-16; sw ra,0(sp); jal 0x100800; nop; lw ra,0(sp); addiu sp,sp,16; jr ra; nop
        for (i, w) in [asm_i(9, 29, 29, 0xFFF0), asm_i(0x2B, 29, 31, 0), (3 << 26) | (0x0010_0800 >> 2), 0, asm_i(0x23, 29, 31, 0), asm_i(9, 29, 29, 16), asm_r(0, 31, 0, 0, 0, 8), 0]
            .iter()
            .enumerate()
        {
            e.w32(base + 4 * i as u32, *w);
        }
        assert_eq!(e.call(base, &[21], &[], 0x01F0_0000).unwrap(), 42);
        assert_eq!(*log.borrow(), vec![21]);
        // Removing a hook takes effect on the next call.
        e.hooks.remove(&0x0010_0800);
        e.w32(0x0010_0800, asm_r(0, 31, 0, 0, 0, 8));
        e.call(base, &[5], &[], 0x01F0_0000).unwrap();
        assert_eq!(log.borrow().len(), 1);
    }

    #[test]
    fn nested_call_from_a_hook_leaves_the_outer_return_intact() {
        let mut e = Ee::new();
        let base = 0x0010_0000u32;
        // Inner function at 0x100400: v0 = a0 + 1.
        e.w32(0x0010_0400, asm_i(9, 4, 2, 1));
        e.w32(0x0010_0404, asm_r(0, 31, 0, 0, 0, 8));
        e.hooks.insert(0x0010_0800, |ee: &mut Ee| {
            let a = ee.gpr[4] as u64;
            let r = ee.call(0x0010_0400, &[a], &[], 0x01EF_0000).unwrap();
            ee.gpr[2] = r as u128 * 10;
        });
        for (i, w) in [asm_i(9, 29, 29, 0xFFF0), asm_i(0x2B, 29, 31, 0), (3 << 26) | (0x0010_0800 >> 2), 0, asm_i(0x23, 29, 31, 0), asm_i(9, 29, 29, 16), asm_r(0, 31, 0, 0, 0, 8), 0]
            .iter()
            .enumerate()
        {
            e.w32(base + 4 * i as u32, *w);
        }
        assert_eq!(e.call(base, &[4], &[], 0x01F0_0000).unwrap(), 50);
    }

    #[test]
    fn a_hook_reaching_itself_is_reported() {
        let mut e = Ee::new();
        e.w32(0x0010_0400, asm_r(0, 31, 0, 0, 0, 8));
        e.hooks.insert(0x0010_0400, |ee: &mut Ee| {
            let _ = ee.call(0x0010_0400, &[], &[], 0x01EF_0000);
        });
        // The inner call fails with Reentrant (ignored by the closure); the outer call completes.
        assert!(e.call(0x0010_0400, &[], &[], 0x01F0_0000).is_ok());
    }

    #[test]
    fn probes_run_before_the_instruction_and_the_run_continues() {
        use std::cell::Cell;
        use std::rc::Rc;
        let mut e = Ee::new();
        let seen = Rc::new(Cell::new(0u64));
        let s2 = seen.clone();
        e.probes.insert(0x0010_0004, move |ee: &mut Ee| s2.set(ee.gpr[2] as u64));
        let out = {
            let base = 0x0010_0000u32;
            for (i, w) in [asm_i(9, 0, 2, 7), asm_i(9, 2, 2, 1), asm_r(0, 31, 0, 0, 0, 8), 0].iter().enumerate() {
                e.w32(base + 4 * i as u32, *w);
            }
            e.call(base, &[], &[], 0x01F0_0000).unwrap()
        };
        assert_eq!(seen.get(), 7, "the probe saw the state before the addiu");
        assert_eq!(out, 8);
    }

    #[test]
    fn hardware_register_accesses_work_and_are_logged() {
        let mut e = Ee::new();
        let base = 0x0010_0000u32;
        // lui t0,0x1000; ori t0,t0,0x3000 (GIF_CTRL); li t1,5; sw t1,0(t0); lw v0,0(t0); jr ra
        for (i, w) in [asm_i(0x0F, 0, 8, 0x1000), asm_i(0x0D, 8, 8, 0x3000), asm_i(9, 0, 9, 5), asm_i(0x2B, 8, 9, 0), asm_i(0x23, 8, 2, 0), asm_r(0, 31, 0, 0, 0, 8), 0]
            .iter()
            .enumerate()
        {
            e.w32(base + 4 * i as u32, *w);
        }
        assert_eq!(e.call(base, &[], &[], 0x01F0_0000).unwrap(), 5);
        let w = e.hw.log.back().unwrap();
        assert_eq!((w.addr, w.value, w.size), (0x1000_3000, 5, 4));
        // The uncached alias 0xB0003000 reaches the same register.
        e.w32(0xB000_3000, 9);
        assert_eq!(e.hw.peek(0x1000_3000), 9);
    }

    #[test]
    fn scratchpad_dma_copies_in_normal_mode() {
        let mut e = Ee::new();
        for i in 0..8u32 {
            e.w32(0x0020_0000 + 4 * i, 0x1000 + i);
        }
        // SPR_TO (channel 9): MADR 0x200000, SADR 0x100, QWC 2, CHCR STR (normal mode).
        e.w32(0x1000_D410, 0x0020_0000);
        e.w32(0x1000_D480, 0x100);
        e.w32(0x1000_D420, 2);
        e.w32(0x1000_D400, 0x100);
        assert_eq!(e.r32(0x7000_0100), 0x1000);
        assert_eq!(e.r32(0x7000_011C), 0x1007);
        // SPR_FROM (channel 8) copies back to another place.
        e.w32(0x1000_D010, 0x0030_0000);
        e.w32(0x1000_D080, 0x100);
        e.w32(0x1000_D020, 2);
        e.w32(0x1000_D000, 0x100);
        assert_eq!(e.r32(0x0030_001C), 0x1007);
        assert_eq!(e.r32(0x1000_D000) & 0x100, 0, "the channel is done");
    }

    #[test]
    fn overlay_loads_at_the_window_and_is_callable() {
        let mut e = Ee::new();
        // v0 = a0 + 5; jr ra; nop
        let code: Vec<u8> = [asm_i(9, 4, 2, 5), asm_r(0, 31, 0, 0, 0, 8), 0].iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(e.load_overlay(&code), Some(crate::OVERLAY_BASE + 12));
        assert_eq!(e.call(crate::OVERLAY_BASE, &[37], &[], 0x01F0_0000).unwrap(), 42);
        assert_eq!(e.load_overlay(&vec![0; 64 << 20]), None);
    }
}
