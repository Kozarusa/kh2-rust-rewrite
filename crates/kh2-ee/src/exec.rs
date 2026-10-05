//! Instruction execution. Encodings: R5900 user manual (core, MMI, COP1) and VU user manual
//! (COP2 macro instructions).

use super::{Ee, clamp, sx32};

fn words(v: u128) -> [u32; 4] {
    std::array::from_fn(|i| (v >> (32 * i)) as u32)
}
fn from_words(w: [u32; 4]) -> u128 {
    w.iter().enumerate().fold(0, |a, (i, &x)| a | (x as u128) << (32 * i))
}
fn halves(v: u128) -> [u16; 8] {
    std::array::from_fn(|i| (v >> (16 * i)) as u16)
}
fn from_halves(h: [u16; 8]) -> u128 {
    h.iter().enumerate().fold(0, |a, (i, &x)| a | (x as u128) << (16 * i))
}
fn vf_bits(v: [f32; 4]) -> u128 {
    from_words(v.map(f32::to_bits))
}
fn bits_vf(v: u128) -> [f32; 4] {
    words(v).map(f32::from_bits)
}
fn shl(x: u64, s: u32) -> u64 {
    x.checked_shl(s).unwrap_or(0)
}
fn shr(x: u64, s: u32) -> u64 {
    x.checked_shr(s).unwrap_or(0)
}
fn lo64(v: u128) -> u64 {
    v as u64
}
fn with_lo64(v: u128, x: u64) -> u128 {
    (v & !(u64::MAX as u128)) | x as u128
}
fn with_hi64(v: u128, x: u64) -> u128 {
    (v & u64::MAX as u128) | (x as u128) << 64
}

impl Ee {
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
            0x08 | 0x09 => self.set(rt, sx32((a as u32).wrapping_add(simm as u32))),
            0x0A => self.set(rt, ((a as i64) < simm as i64) as u64),
            0x0B => self.set(rt, (a < simm) as u64),
            0x0C => self.set(rt, a & imm as u64),
            0x0D => self.set(rt, a | imm as u64),
            0x0E => self.set(rt, a ^ imm as u64),
            0x0F => self.set(rt, sx32((imm as u32) << 16)),
            0x10 => self.unimpl(w, "cop0"),
            0x11 => self.cop1(w, rs, rt, rd, sa as usize, funct),
            0x12 => self.cop2(w, rs, rt, rd),
            0x14 => self.branch(a == b, bt, true),
            0x15 => self.branch(a != b, bt, true),
            0x16 => self.branch(a as i64 <= 0, bt, true),
            0x17 => self.branch(a as i64 > 0, bt, true),
            0x18 | 0x19 => self.set(rt, a.wrapping_add(simm)),
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
                let v = sx32(self.r32(addr));
                self.set(rt, v)
            }
            0x24 => {
                let v = self.r8(addr) as u64;
                self.set(rt, v)
            }
            0x25 => {
                let v = self.r16(addr) as u64;
                self.set(rt, v)
            }
            0x27 => {
                let v = self.r32(addr) as u64;
                self.set(rt, v)
            }
            0x28 => self.write(addr, &[b as u8]),
            0x29 => self.write(addr, &(b as u16).to_le_bytes()),
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
            0x2B => self.w32(addr, b as u32),
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
            0x31 => self.fpr[rt] = f32::from_bits(self.r32(addr)),
            0x36 => {
                let v = self.r128(addr & !15);
                if rt != 0 {
                    self.vf[rt] = bits_vf(v);
                }
            }
            0x37 => {
                let v = self.r64(addr);
                self.set(rt, v)
            }
            0x39 => self.w32(addr, self.fpr[rt].to_bits()),
            0x3E => {
                let v = vf_bits(self.vf[rt]);
                self.write(addr & !15, &v.to_le_bytes());
            }
            0x3F => self.write(addr, &b.to_le_bytes()),
            _ => self.unimpl(w, "opcode"),
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
            0x20 | 0x21 => self.set(rd, sx32(a32.wrapping_add(b32))),
            0x22 | 0x23 => self.set(rd, sx32(a32.wrapping_sub(b32))),
            0x24 => self.set(rd, a & b),
            0x25 => self.set(rd, a | b),
            0x26 => self.set(rd, a ^ b),
            0x27 => self.set(rd, !(a | b)),
            0x28 => self.set(rd, self.sa as u64),
            0x29 => self.sa = a32,
            0x2A => self.set(rd, ((a as i64) < (b as i64)) as u64),
            0x2B => self.set(rd, (a < b) as u64),
            0x2C | 0x2D => self.set(rd, a.wrapping_add(b)),
            0x2E | 0x2F => self.set(rd, a.wrapping_sub(b)),
            0x38 => self.set(rd, b << sa),
            0x3A => self.set(rd, b >> sa),
            0x3B => self.set(rd, ((b as i64) >> sa) as u64),
            0x3C => self.set(rd, b << (sa + 32)),
            0x3E => self.set(rd, b >> (sa + 32)),
            0x3F => self.set(rd, ((b as i64) >> (sa + 32)) as u64),
            _ => self.unimpl(w, "special"),
        }
    }

    fn divide(&mut self, a: u32, b: u32, signed: bool, pipe1: bool) {
        let (q, r) = if signed {
            let (x, y) = (a as i32, b as i32);
            if y == 0 {
                (if x < 0 { 1 } else { -1i32 } as u32, a)
            } else {
                (x.wrapping_div(y) as u32, x.wrapping_rem(y) as u32)
            }
        } else if b == 0 {
            (u32::MAX, a)
        } else {
            (a / b, a % b)
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
            _ => self.vi[r] = v,
        }
    }

    fn i_reg(&self) -> f32 {
        f32::from_bits(self.vi[21])
    }

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

    fn aset(&mut self, dest: u32, v: [f32; 4]) {
        for (i, x) in v.into_iter().enumerate() {
            if dest & (8 >> i) != 0 {
                self.vacc[i] = clamp(x);
            }
        }
    }

    fn cop2(&mut self, w: u32, fmt: usize, rt: usize, rd: usize) {
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
            match funct {
                0x00..=0x03 => self.vset(fd, dest, map(&|k| s[k] + bc)),
                0x04..=0x07 => self.vset(fd, dest, map(&|k| s[k] - bc)),
                0x08..=0x0B => self.vset(fd, dest, map(&|k| acc[k] + s[k] * bc)),
                0x0C..=0x0F => self.vset(fd, dest, map(&|k| acc[k] - s[k] * bc)),
                0x10..=0x13 => self.vset(fd, dest, map(&|k| s[k].max(bc))),
                0x14..=0x17 => self.vset(fd, dest, map(&|k| s[k].min(bc))),
                0x18..=0x1B => self.vset(fd, dest, map(&|k| s[k] * bc)),
                0x1C => self.vset(fd, dest, map(&|k| s[k] * q)),
                0x1D => self.vset(fd, dest, map(&|k| s[k].max(i))),
                0x1E => self.vset(fd, dest, map(&|k| s[k] * i)),
                0x1F => self.vset(fd, dest, map(&|k| s[k].min(i))),
                0x20 => self.vset(fd, dest, map(&|k| s[k] + q)),
                0x21 => self.vset(fd, dest, map(&|k| acc[k] + s[k] * q)),
                0x22 => self.vset(fd, dest, map(&|k| s[k] + i)),
                0x23 => self.vset(fd, dest, map(&|k| acc[k] + s[k] * i)),
                0x24 => self.vset(fd, dest, map(&|k| s[k] - q)),
                0x25 => self.vset(fd, dest, map(&|k| acc[k] - s[k] * q)),
                0x26 => self.vset(fd, dest, map(&|k| s[k] - i)),
                0x27 => self.vset(fd, dest, map(&|k| acc[k] - s[k] * i)),
                0x28 => self.vset(fd, dest, map(&|k| s[k] + t[k])),
                0x29 => self.vset(fd, dest, map(&|k| acc[k] + s[k] * t[k])),
                0x2A => self.vset(fd, dest, map(&|k| s[k] * t[k])),
                0x2B => self.vset(fd, dest, map(&|k| s[k].max(t[k]))),
                0x2C => self.vset(fd, dest, map(&|k| s[k] - t[k])),
                0x2D => self.vset(fd, dest, map(&|k| acc[k] - s[k] * t[k])),
                0x2E => self.vset(fd, dest & 0xE, [acc[0] - s[1] * t[2], acc[1] - s[2] * t[0], acc[2] - s[0] * t[1], 0.0]),
                0x2F => self.vset(fd, dest, map(&|k| s[k].min(t[k]))),
                0x30 => self.vi_write(fd, self.vi_read(fs).wrapping_add(self.vi_read(ft))),
                0x31 => self.vi_write(fd, self.vi_read(fs).wrapping_sub(self.vi_read(ft))),
                0x32 => {
                    let imm = ((((w >> 6) & 31) as i32) << 27 >> 27) as u32;
                    self.vi_write(ft, self.vi_read(fs).wrapping_add(imm))
                }
                0x34 => self.vi_write(fd, self.vi_read(fs) & self.vi_read(ft)),
                0x35 => self.vi_write(fd, self.vi_read(fs) | self.vi_read(ft)),
                _ => self.unimpl(w, "vu upper"),
            }
            return;
        }
        let op = (((w >> 6) & 31) << 2) | (funct & 3);
        let fsf = ((w >> 21) & 3) as usize;
        let ftf = ((w >> 23) & 3) as usize;
        let scale = |n: u32| (1u32 << n) as f32;
        match op {
            0x00..=0x03 => self.aset(dest, map(&|k| s[k] + bc)),
            0x04..=0x07 => self.aset(dest, map(&|k| s[k] - bc)),
            0x08..=0x0B => self.aset(dest, map(&|k| acc[k] + s[k] * bc)),
            0x0C..=0x0F => self.aset(dest, map(&|k| acc[k] - s[k] * bc)),
            0x10..=0x13 => {
                let n = [0, 4, 12, 15][(op & 3) as usize];
                let v = map(&|k| s[k].to_bits() as i32 as f32 / scale(n));
                self.vset(ft, dest, v);
            }
            0x14..=0x17 => {
                if ft != 0 {
                    let n = [0, 4, 12, 15][(op & 3) as usize];
                    for k in 0..4 {
                        if dest & (8 >> k) != 0 {
                            self.vf[ft][k] = f32::from_bits((s[k] * scale(n)) as i32 as u32);
                        }
                    }
                }
            }
            0x18..=0x1B => self.aset(dest, map(&|k| s[k] * bc)),
            0x1C => self.aset(dest, map(&|k| s[k] * q)),
            0x1D => self.vset(ft, dest, map(&|k| s[k].abs())),
            0x1E => self.aset(dest, map(&|k| s[k] * i)),
            0x1F => {
                let wv = t[3].abs();
                let mut flags = 0u32;
                for k in 0..3 {
                    if s[k] > wv {
                        flags |= 1 << (2 * k);
                    }
                    if s[k] < -wv {
                        flags |= 2 << (2 * k);
                    }
                }
                self.vi[18] = ((self.vi[18] << 6) | flags) & 0xFF_FFFF;
            }
            0x20 => self.aset(dest, map(&|k| s[k] + q)),
            0x21 => self.aset(dest, map(&|k| acc[k] + s[k] * q)),
            0x22 => self.aset(dest, map(&|k| s[k] + i)),
            0x23 => self.aset(dest, map(&|k| acc[k] + s[k] * i)),
            0x24 => self.aset(dest, map(&|k| s[k] - q)),
            0x25 => self.aset(dest, map(&|k| acc[k] - s[k] * q)),
            0x26 => self.aset(dest, map(&|k| s[k] - i)),
            0x27 => self.aset(dest, map(&|k| acc[k] - s[k] * i)),
            0x28 => self.aset(dest, map(&|k| s[k] + t[k])),
            0x29 => self.aset(dest, map(&|k| acc[k] + s[k] * t[k])),
            0x2A => self.aset(dest, map(&|k| s[k] * t[k])),
            0x2C => self.aset(dest, map(&|k| s[k] - t[k])),
            0x2D => self.aset(dest, map(&|k| acc[k] - s[k] * t[k])),
            0x2E => self.aset(dest & 0xE, [s[1] * t[2], s[2] * t[0], s[0] * t[1], 0.0]),
            0x2F | 0x3B => {} // vnop, vwaitq
            0x30 => self.vset(ft, dest, s),
            0x31 => self.vset(ft, dest, [s[1], s[2], s[3], s[0]]),
            0x38 => {
                let (n, d) = (s[fsf], t[ftf]);
                self.q = clamp(if d == 0.0 { f32::MAX.copysign(n) * d.signum() } else { n / d });
            }
            0x39 => self.q = t[ftf].abs().sqrt(),
            0x3A => {
                let d = t[ftf].abs().sqrt();
                self.q = clamp(if d == 0.0 { f32::MAX.copysign(s[fsf]) } else { s[fsf] / d });
            }
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
            _ => self.unimpl(w, "vu lower"),
        }
    }

    fn mmi(&mut self, w: u32, rs: usize, rt: usize, rd: usize, sa: u32, funct: u32) {
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
            0x08 => self.mmi0(w, rd, wa, wb, a, b, sa),
            0x09 => self.mmi2(w, rd, wa, wb, a, b, sa),
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
            0x28 => self.mmi1(w, rd, wa, wb, a, b, sa),
            0x29 => self.mmi3(w, rd, wa, wb, a, b, sa),
            // PMFHL.LW / .UW: the low or high pipe's product, as the soft-float multiply reads it.
            0x30 if sa <= 1 => {
                let (lo, hi) = (words(self.lo), words(self.hi));
                self.set128(rd, if sa == 0 { from_words([lo[0], hi[0], lo[2], hi[2]]) } else { from_words([lo[1], hi[1], lo[3], hi[3]]) });
            }
            0x34 | 0x36 | 0x37 => {
                let h = halves(b).map(|x| match funct {
                    0x34 => x << sa,
                    0x36 => x >> sa,
                    _ => ((x as i16) >> sa) as u16,
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

    #[allow(clippy::too_many_arguments)]
    fn mmi0(&mut self, w: u32, rd: usize, wa: [u32; 4], wb: [u32; 4], _a: u128, _b: u128, sa: u32) {
        let r = match sa {
            0x00 => from_words(std::array::from_fn(|k| wa[k].wrapping_add(wb[k]))),
            0x01 => from_words(std::array::from_fn(|k| wa[k].wrapping_sub(wb[k]))),
            0x02 => from_words(std::array::from_fn(|k| if (wa[k] as i32) > (wb[k] as i32) { u32::MAX } else { 0 })),
            0x03 => from_words(std::array::from_fn(|k| (wa[k] as i32).max(wb[k] as i32) as u32)),
            0x12 => from_words([wb[0], wa[0], wb[1], wa[1]]),
            0x13 => from_words([wb[0], wb[2], wa[0], wa[2]]),
            _ => return self.unimpl(w, "mmi0"),
        };
        self.set128(rd, r);
    }

    #[allow(clippy::too_many_arguments)]
    fn mmi1(&mut self, w: u32, rd: usize, wa: [u32; 4], wb: [u32; 4], a: u128, b: u128, sa: u32) {
        let r = match sa {
            0x01 => from_words(wb.map(|x| (x as i32).unsigned_abs().min(i32::MAX as u32))),
            0x02 => from_words(std::array::from_fn(|k| if wa[k] == wb[k] { u32::MAX } else { 0 })),
            0x03 => from_words(std::array::from_fn(|k| (wa[k] as i32).min(wb[k] as i32) as u32)),
            0x10 => from_words(std::array::from_fn(|k| wa[k].saturating_add(wb[k]))),
            0x12 => from_words([wb[2], wa[2], wb[3], wa[3]]),
            0x1B => {
                // qfsrv: rd = low 128 bits of (rs:rt) >> SA
                let s = self.sa & 127;
                if s == 0 { b } else { (b >> s) | (a << (128 - s)) }
            }
            _ => return self.unimpl(w, "mmi1"),
        };
        self.set128(rd, r);
    }

    #[allow(clippy::too_many_arguments)]
    fn mmi2(&mut self, w: u32, rd: usize, wa: [u32; 4], wb: [u32; 4], a: u128, b: u128, sa: u32) {
        let r = match sa {
            0x08 => self.hi,
            0x09 => self.lo,
            0x0E => (a << 64) | (b & u64::MAX as u128),
            0x12 => a & b,
            0x13 => a ^ b,
            0x1E => from_words([wb[2], wb[1], wb[0], wb[3]]),
            0x1F => from_words([wb[1], wb[2], wb[0], wb[3]]),
            _ => {
                let _ = wa;
                return self.unimpl(w, "mmi2");
            }
        };
        self.set128(rd, r);
    }

    #[allow(clippy::too_many_arguments)]
    fn mmi3(&mut self, w: u32, rd: usize, wa: [u32; 4], wb: [u32; 4], a: u128, b: u128, sa: u32) {
        match sa {
            0x08 => {
                self.hi = a;
                return;
            }
            0x09 => {
                self.lo = a;
                return;
            }
            _ => {}
        }
        let r = match sa {
            0x0E => (b & !(u64::MAX as u128)) | (a >> 64),
            0x12 => a | b,
            0x13 => !(a | b),
            0x1B => {
                let h = halves(b);
                from_halves([h[0], h[0], h[0], h[0], h[4], h[4], h[4], h[4]])
            }
            0x1E => from_words([wb[0], wb[2], wb[1], wb[3]]),
            _ => {
                let _ = wa;
                return self.unimpl(w, "mmi3");
            }
        };
        self.set128(rd, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn mmi_extend_and_copy() {
        let mut e = Ee::new();
        e.gpr[1] = from_words([1, 2, 3, 4]);
        e.gpr[2] = from_words([5, 6, 7, 8]);
        // pextlw v1(3), at(1), v0(2)
        e.exec(asm_r(0x1C, 1, 2, 3, 0x12, 0x08));
        assert_eq!(words(e.gpr[3]), [5, 1, 6, 2]);
        // pcpyld a0(4), at, v0 -> lo = v0.lo, hi = at.lo
        e.exec(asm_r(0x1C, 1, 2, 4, 0x0E, 0x09));
        assert_eq!(words(e.gpr[4]), [5, 6, 1, 2]);
        // pcpyud a1(5), at, v0 -> lo = at.hi, hi = v0.hi
        e.exec(asm_r(0x1C, 1, 2, 5, 0x0E, 0x29));
        assert_eq!(words(e.gpr[5]), [3, 4, 7, 8]);
    }
}
