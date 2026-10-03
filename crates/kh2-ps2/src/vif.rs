//! VIF1 interpreter: executes the data-transfer subset of the VIF command set into a 16 KiB VU1
//! data memory and reports every microprogram kick (MSCAL/MSCALF/MSCNT) with a memory snapshot.

use crate::rd32;

pub const VU1_QWORDS: usize = 1024;

#[derive(Clone)]
pub struct Vif {
    pub mem: Vec<[u32; 4]>,
    cl: u32,
    wl: u32,
    mask: u32,
    row: [u32; 4],
    col: [u32; 4],
}

impl Default for Vif {
    fn default() -> Self {
        Vif { mem: vec![[0; 4]; VU1_QWORDS], cl: 1, wl: 1, mask: 0, row: [0; 4], col: [0; 4] }
    }
}

impl Vif {
    /// Runs a VIF code stream. `kick` gets VU1 memory each time a microprogram would start.
    /// Stops quietly at the end of the stream or at a truncated command.
    pub fn run(&mut self, code: &[u8], mut kick: impl FnMut(&[[u32; 4]])) {
        let mut pc = 0usize;
        while pc + 4 <= code.len() {
            let w = rd32(code, pc);
            pc += 4;
            let imm = w & 0xFFFF;
            let num = (w >> 16) & 0xFF;
            let cmd = (w >> 24) & 0x7F;
            match cmd {
                0x01 => {
                    self.cl = (imm & 0xFF).max(1);
                    self.wl = (imm >> 8).max(1);
                }
                0x14 | 0x15 | 0x17 => kick(&self.mem),
                0x20 => {
                    self.mask = rd32(code, pc);
                    pc += 4;
                }
                0x30 | 0x31 => {
                    let v = [rd32(code, pc), rd32(code, pc + 4), rd32(code, pc + 8), rd32(code, pc + 12)];
                    if cmd == 0x30 { self.row = v } else { self.col = v }
                    pc += 16;
                }
                0x4A => pc += 8 * if num == 0 { 256 } else { num as usize }, // MPG: microcode, skipped
                0x50 | 0x51 => pc += 16 * if imm == 0 { 65536 } else { imm as usize }, // DIRECT: GIF data
                c if c >= 0x60 => pc = self.unpack(code, pc, c, num, imm),
                _ => {} // NOP, OFFSET, BASE, ITOP, STMOD, MSKPATH3, MARK, FLUSH*: no effect on data
            }
        }
    }

    fn unpack(&mut self, code: &[u8], pc: usize, cmd: u32, num: u32, imm: u32) -> usize {
        let vn = ((cmd >> 2) & 3) as usize;
        let vl = (cmd & 3) as usize;
        let masked = (cmd >> 4) & 1 == 1;
        let unsigned = (imm >> 14) & 1 == 1;
        let num = if num == 0 { 256 } else { num as usize };
        let comps = vn + 1;
        let v4_5 = vn == 3 && vl == 3;
        let bits = if v4_5 { 16 } else { [32, 16, 8, 5][vl] };
        let elem_bits = if v4_5 { 16 } else { comps * bits };
        let len = (num * elem_bits).div_ceil(32) * 4;
        let data = &code[pc.min(code.len())..(pc + len).min(code.len())];
        let base = (imm & 0x3FF) as usize;

        let mut bitpos = 0usize;
        let mut read = |n: usize| -> u32 {
            let byte = bitpos / 8;
            let v = match n {
                32 => rd32(data, byte),
                16 => crate::rd16(data, byte) as u32,
                _ => *data.get(byte).unwrap_or(&0) as u32,
            };
            bitpos += n;
            if unsigned || n == 32 {
                v
            } else if n == 16 {
                v as u16 as i16 as i32 as u32
            } else {
                v as u8 as i8 as i32 as u32
            }
        };
        let (cl, wl) = (self.cl as usize, self.wl as usize);
        for j in 0..num {
            let v: [u32; 4] = if v4_5 {
                let p = read(16);
                [(p & 0x1F) << 3, ((p >> 5) & 0x1F) << 3, ((p >> 10) & 0x1F) << 3, ((p >> 15) & 1) << 7]
            } else {
                let mut v = [0u32; 4];
                for c in v.iter_mut().take(comps) {
                    *c = read(bits);
                }
                match comps {
                    1 => [v[0]; 4],
                    2 => [v[0], v[1], v[0], v[1]], // undefined on hardware; this matches PCSX2
                    3 => [v[0], v[1], v[2], 0],
                    _ => v,
                }
            };
            // Write/skip cycle: `wl` qwords written out of every `cl`.
            let dest = (base + (j / wl) * cl.max(wl) + j % wl) % VU1_QWORDS;
            let row_sel = (j % wl).min(3);
            for (c, val) in v.iter().enumerate() {
                let m = if masked { (self.mask >> (row_sel * 8 + c * 2)) & 3 } else { 0 };
                match m {
                    0 => {
                        if !(comps == 3 && c == 3) || !masked {
                            self.mem[dest][c] = *val;
                        }
                    }
                    1 => self.mem[dest][c] = self.row[c],
                    2 => self.mem[dest][c] = self.col[c],
                    _ => {}
                }
            }
        }
        pc + len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(cmd: u32, num: u32, imm: u32) -> [u8; 4] {
        ((cmd << 24) | (num << 16) | imm).to_le_bytes()
    }

    #[test]
    fn unpack_v4_32_and_kick() {
        let mut code = Vec::new();
        code.extend(word(0x01, 0, 0x0101));
        code.extend(word(0x6C, 1, 5)); // V4-32, 1 element, addr 5
        for x in [1u32, 2, 3, 4] {
            code.extend(x.to_le_bytes());
        }
        code.extend(word(0x17, 0, 0));
        let mut seen = None;
        Vif::default().run(&code, |m| seen = Some(m[5]));
        assert_eq!(seen, Some([1, 2, 3, 4]));
    }

    #[test]
    fn masked_s8_lands_in_one_lane() {
        let mut code = Vec::new();
        code.extend(word(0x20, 0, 0));
        code.extend(0xCFCF_CFCFu32.to_le_bytes()); // write Z only
        code.extend(word(0x72, 2, 0x4000 | 3)); // S-8 masked, 2 elements, unsigned, addr 3
        code.extend([7u8, 200, 0, 0]);
        let mut v = Vif::default();
        v.run(&code, |_| {});
        assert_eq!(v.mem[3], [0, 0, 7, 0]);
        assert_eq!(v.mem[4], [0, 0, 200, 0]);
    }
}
