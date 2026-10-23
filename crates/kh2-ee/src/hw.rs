//! A small stub of the EE's memory-mapped hardware: just enough for game code that touches the
//! timers, IPU, GIF, VIF, DMA controller, interrupt controller, serial port or GS privileged
//! registers to run in a test.
//!
//! Model (a deliberate simplification, not an emulation):
//! - Every register reads back what was last written, starting from its documented reset value
//!   (mostly 0). Writes are logged in [`Hw::log`].
//! - The write-one-to-clear and write-one-to-toggle registers behave as documented (INTC_STAT,
//!   INTC_MASK, D_STAT, the timer flag bits, GS CSR).
//! - The four timers count while enabled: one tick per two executed instructions (BUSCLK is half
//!   the core clock and an instruction is taken as one core cycle; [inferred] model), divided by
//!   the CLKS setting. Compare and overflow flags and interrupts are not raised.
//! - A DMA channel started by writing STR to its CHCR completes at once: STR reads back 0 and the
//!   channel's D_STAT bit is set. The scratchpad channels (8 and 9) in normal mode really copy;
//!   chain mode and all other channels move nothing (the data is the job of a real DMA model).
//! - The IPU runs every command to completion at once: the busy bits (IPU_CMD bit 31, IPU_CTRL bit
//!   31) read 0, the FIFO counts in IPU_CTRL bits 0-7 read 0 (empty), and writing IPU_CTRL with RST
//!   (bit 30) resets the unit: RST reads back 0 and IPU_CTRL, IPU_CMD, IPU_BP and IPU_TOP clear
//!   (EE user's manual, IPU chapter; PCSX2 resets the same registers). Bitstream decoding is not
//!   modelled, so code that waits for the end of a stream (IPU_CTRL.ECD) spins.
//! - The VU0/VU1 code and data memories (0x11000000..0x11010000) are plain RAM.
//!
//! Sources: the EE user's manual register chapters (timers, DMAC, INTC, IPU, GIF), the GS user's
//! manual (privileged registers); the GS CSR identification bits follow PCSX2's documented value
//! [research source, behaviour only].

use std::collections::{HashMap, VecDeque};

/// First and one past the last address (after masking to 29 bits) of each mapped window.
pub const IO_BASE: u32 = 0x1000_0000;
pub const IO_END: u32 = 0x1001_0000;
pub const VU_BASE: u32 = 0x1100_0000;
pub const VU_END: u32 = 0x1101_0000;
pub const GS_BASE: u32 = 0x1200_0000;
pub const GS_END: u32 = 0x1200_2000;

const INTC_STAT: u32 = 0x1000_F000;
const INTC_MASK: u32 = 0x1000_F010;
const D_STAT: u32 = 0x1000_E010;
const D_ENABLER: u32 = 0x1000_F520;
const D_ENABLEW: u32 = 0x1000_F590;
const GS_CSR: u32 = 0x1200_1000;
const IPU_CMD: u32 = 0x1000_2000;
const IPU_CTRL: u32 = 0x1000_2010;
const IPU_BP: u32 = 0x1000_2020;
const IPU_TOP: u32 = 0x1000_2030;

/// One logged register write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HwWrite {
    pub pc: u32,
    pub addr: u32,
    /// Size in bytes (1, 2, 4, 8 or 16).
    pub size: u8,
    pub value: u128,
}

#[derive(Clone, Copy, Default)]
struct Timer {
    /// Counter value at `since`.
    count: u32,
    /// Instruction count at which `count` was latched.
    since: u64,
}

/// A transfer the memory system must perform after a CHCR write (it needs RAM and scratchpad).
#[derive(Clone, Copy, Debug)]
pub(crate) struct DmaKick {
    pub channel: u32,
    pub madr: u32,
    pub sadr: u32,
    pub qwc: u32,
}

pub struct Hw {
    regs: HashMap<u32, u32>,
    timers: [Timer; 4],
    vu: Vec<u8>,
    /// Register writes, oldest first, capped at [`Hw::LOG_CAP`] entries.
    pub log: VecDeque<HwWrite>,
}

impl Default for Hw {
    fn default() -> Self {
        Self::new()
    }
}

/// The documented reset value of the register at `addr` (word aligned), 0 where the manuals give
/// none or the register is not modelled.
fn reset_value(addr: u32) -> u32 {
    match addr {
        D_ENABLER => 0x1201,
        // GS CSR: revision 0x1B and id 0x55 in the top half, FIFO field = empty.
        GS_CSR => 0x551B_4000,
        // GS IMR: every interrupt masked.
        0x1200_1010 => 0x7F00,
        _ => 0,
    }
}

fn timer_of(addr: u32) -> Option<(usize, u32)> {
    // T0 0x10000000, T1 0x10000800, T2 0x10001000, T3 0x10001800; registers at +0 COUNT, +0x10 MODE, +0x20 COMP, +0x30 HOLD.
    if !(0x1000_0000..0x1000_2000).contains(&addr) {
        return None;
    }
    let off = addr - 0x1000_0000;
    let n = (off >> 11) as usize;
    let reg = off & 0x7FF;
    (reg & 0xF == 0 && reg <= 0x30).then_some((n, reg))
}

/// Core clocks per timer tick for a CLKS setting (BUSCLK, /16, /256, HBLNK).
fn timer_divisor(mode: u32) -> u64 {
    match mode & 3 {
        0 => 2,
        1 => 32,
        2 => 512,
        // One scan line of NTSC video is about 9370 bus clocks [inferred from 15.734 kHz].
        _ => 2 * 9370,
    }
}

impl Hw {
    pub const LOG_CAP: usize = 1 << 16;

    pub fn new() -> Hw {
        Hw { regs: HashMap::new(), timers: [Timer::default(); 4], vu: vec![0; (VU_END - VU_BASE) as usize], log: VecDeque::new() }
    }

    /// Whether `addr` (masked to 29 bits) is inside a mapped window.
    pub fn maps(addr: u32) -> bool {
        (IO_BASE..IO_END).contains(&addr) || (VU_BASE..VU_END).contains(&addr) || (GS_BASE..GS_END).contains(&addr)
    }

    /// The last value written to the word register at `addr`, or its reset value. Timer counts
    /// read as latched at the last write.
    pub fn peek(&self, addr: u32) -> u32 {
        let addr = addr & !3;
        self.regs.get(&addr).copied().unwrap_or_else(|| reset_value(addr))
    }

    /// Sets a word register without logging (to prepare what the code will read).
    pub fn poke(&mut self, addr: u32, value: u32) {
        self.regs.insert(addr & !3, value);
    }

    /// VU0/VU1 code and data memory as bytes from 0x11000000.
    pub fn vu_mem(&mut self) -> &mut [u8] {
        &mut self.vu
    }

    fn timer_count(&self, n: usize, steps: u64) -> u32 {
        let t = self.timers[n];
        let mode = self.peek(0x1000_0010 + 0x800 * n as u32);
        if mode & 0x80 == 0 {
            return t.count & 0xFFFF;
        }
        (t.count.wrapping_add(((steps - t.since) / timer_divisor(mode)) as u32)) & 0xFFFF
    }

    fn read_word(&self, addr: u32, steps: u64) -> u32 {
        if let Some((n, 0)) = timer_of(addr) {
            return self.timer_count(n, steps);
        }
        if addr == D_ENABLER && let Some(v) = self.regs.get(&D_ENABLEW) {
            return *v;
        }
        let v = self.peek(addr);
        // Commands complete at once: never busy.
        if addr == IPU_CMD { v & 0x7FFF_FFFF } else { v }
    }

    pub(crate) fn read(&self, addr: u32, out: &mut [u8], steps: u64) {
        if (VU_BASE..VU_END).contains(&addr) {
            let o = (addr - VU_BASE) as usize;
            match self.vu.get(o..o + out.len()) {
                Some(s) => out.copy_from_slice(s),
                None => out.fill(0),
            }
            return;
        }
        for (i, b) in out.iter_mut().enumerate() {
            let a = addr + i as u32;
            *b = (self.read_word(a & !3, steps) >> (8 * (a & 3))) as u8;
        }
    }

    fn log_write(&mut self, w: HwWrite) {
        if self.log.len() >= Self::LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(w);
    }

    /// Writes `bytes` at `addr`; returns a DMA transfer to perform, if the write started one.
    pub(crate) fn write(&mut self, pc: u32, addr: u32, bytes: &[u8], steps: u64) -> Option<DmaKick> {
        if (VU_BASE..VU_END).contains(&addr) {
            let o = (addr - VU_BASE) as usize;
            if let Some(s) = self.vu.get_mut(o..o + bytes.len()) {
                s.copy_from_slice(bytes);
            }
            return None;
        }
        let mut v = 0u128;
        for (i, b) in bytes.iter().enumerate() {
            v |= (*b as u128) << (8 * i);
        }
        self.log_write(HwWrite { pc, addr, size: bytes.len() as u8, value: v });
        let mut kick = None;
        let mut i = 0;
        while i < bytes.len() {
            let a = addr + i as u32;
            let (word_addr, shift) = (a & !3, 8 * (a & 3));
            let n = (4 - (a & 3) as usize).min(bytes.len() - i);
            let mut part = 0u32;
            for (k, b) in bytes[i..i + n].iter().enumerate() {
                part |= (*b as u32) << (8 * k);
            }
            let mask = if n == 4 { u32::MAX } else { ((1u32 << (8 * n)) - 1) << shift };
            // Sub-word writes merge into the register's current value.
            let word = if n == 4 { part } else { (self.read_word(word_addr, steps) & !mask) | ((part << shift) & mask) };
            if let Some(k) = self.write_word(word_addr, word, steps) {
                kick = Some(k);
            }
            i += n;
        }
        kick
    }

    fn write_word(&mut self, addr: u32, value: u32, steps: u64) -> Option<DmaKick> {
        if let Some((n, reg)) = timer_of(addr) {
            // Latch the running count before anything changes.
            let now = self.timer_count(n, steps);
            self.timers[n] = Timer { count: now, since: steps };
            match reg {
                0x00 => self.timers[n].count = value & 0xFFFF,
                0x10 => {
                    // Bits 10 (EQUF) and 11 (OVFF) clear when written as 1; the rest is stored.
                    let old = self.peek(addr);
                    let flags = old & value & 0xC00;
                    self.regs.insert(addr, (value & !0xC00) | (old & 0xC00 & !flags));
                }
                _ => {
                    self.regs.insert(addr, value);
                }
            }
            return None;
        }
        match addr {
            INTC_STAT => {
                let old = self.peek(addr);
                self.regs.insert(addr, old & !value);
            }
            INTC_MASK => {
                let old = self.peek(addr);
                self.regs.insert(addr, old ^ (value & 0x7FFF));
            }
            D_STAT => {
                // Bits 0-9 are status (write 1 to clear), bits 16-25 are masks (write 1 to toggle).
                let old = self.peek(addr);
                let status = old & 0x3FF & !(value & 0x3FF);
                let mask = (old & 0x03FF_0000) ^ (value & 0x03FF_0000);
                self.regs.insert(addr, status | mask);
            }
            D_ENABLEW => {
                self.regs.insert(addr, value);
            }
            IPU_CTRL => {
                if value & (1 << 30) != 0 {
                    for r in [IPU_CMD, IPU_CTRL, IPU_BP, IPU_TOP] {
                        self.regs.insert(r, 0);
                    }
                } else {
                    // BUSY, RST and the FIFO counts are not stored.
                    self.regs.insert(addr, value & !(0xC000_0000 | 0xFF));
                }
            }
            GS_CSR => {
                // Bits 0-4 are event flags (write 1 to clear); the identification stays.
                let old = self.peek(addr);
                self.regs.insert(addr, old & !(value & 0x1F));
            }
            _ => {
                self.regs.insert(addr, value);
                if let Some(ch) = dma_channel_of_chcr(addr)
                    && value & 0x100 != 0
                {
                    // Completes at once: STR reads back 0, the channel's status bit is set.
                    self.regs.insert(addr, value & !0x100);
                    let st = self.peek(D_STAT);
                    self.regs.insert(D_STAT, st | 1 << ch);
                    let base = addr & !0xFF;
                    let chain = value & 0xC != 0;
                    if (ch == 8 || ch == 9) && !chain {
                        return Some(DmaKick {
                            channel: ch,
                            madr: self.peek(base + 0x10),
                            sadr: self.peek(base + 0x80),
                            qwc: self.peek(base + 0x20) & 0xFFFF,
                        });
                    }
                }
            }
        }
        None
    }
}

/// The channel number if `addr` is a channel's CHCR register.
fn dma_channel_of_chcr(addr: u32) -> Option<u32> {
    const CHCR: [u32; 10] = [
        0x1000_8000,
        0x1000_9000,
        0x1000_A000,
        0x1000_B000,
        0x1000_B400,
        0x1000_C000,
        0x1000_C400,
        0x1000_C800,
        0x1000_D000,
        0x1000_D400,
    ];
    CHCR.iter().position(|&a| a == addr).map(|p| p as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rd(h: &Hw, a: u32, steps: u64) -> u32 {
        let mut b = [0u8; 4];
        h.read(a, &mut b, steps);
        u32::from_le_bytes(b)
    }

    fn wr(h: &mut Hw, a: u32, v: u32, steps: u64) {
        h.write(0x100000, a, &v.to_le_bytes(), steps);
    }

    #[test]
    fn registers_read_back_and_writes_are_logged() {
        let mut h = Hw::new();
        assert_eq!(rd(&h, 0x1000_3000, 0), 0, "GIF_CTRL resets to 0");
        wr(&mut h, 0x1000_3000, 0x5, 0);
        assert_eq!(rd(&h, 0x1000_3000, 0), 5);
        assert_eq!(h.log.back().copied(), Some(HwWrite { pc: 0x100000, addr: 0x1000_3000, size: 4, value: 5 }));
        // A byte write merges into the word.
        h.write(0x100004, 0x1000_3001, &[0xAB], 0);
        assert_eq!(rd(&h, 0x1000_3000, 0), 0xAB05);
        assert_eq!(h.log.back().unwrap().size, 1);
    }

    #[test]
    fn intc_stat_clears_and_mask_toggles_with_ones() {
        let mut h = Hw::new();
        h.poke(INTC_STAT, 0b1110);
        wr(&mut h, INTC_STAT, 0b0100, 0);
        assert_eq!(rd(&h, INTC_STAT, 0), 0b1010, "EE user's manual, INTC_STAT: write 1 clears");
        wr(&mut h, INTC_MASK, 0b0011, 0);
        assert_eq!(rd(&h, INTC_MASK, 0), 0b0011);
        wr(&mut h, INTC_MASK, 0b0110, 0);
        assert_eq!(rd(&h, INTC_MASK, 0), 0b0101, "INTC_MASK: write 1 toggles");
    }

    #[test]
    fn dmac_status_and_mask_halves() {
        let mut h = Hw::new();
        h.poke(D_STAT, 0x0003_0005);
        wr(&mut h, D_STAT, 0x0001_0004, 0);
        // Status bit 2 cleared; mask bit 16 toggled off.
        assert_eq!(rd(&h, D_STAT, 0), 0x0002_0001);
    }

    #[test]
    fn timers_count_while_enabled() {
        let mut h = Hw::new();
        assert_eq!(rd(&h, 0x1000_0000, 1000), 0, "disabled: stays at 0");
        wr(&mut h, 0x1000_0010, 0x80, 1000); // CUE, BUSCLK
        assert_eq!(rd(&h, 0x1000_0000, 1000), 0);
        assert_eq!(rd(&h, 0x1000_0000, 1100), 50, "one tick per two instructions");
        wr(&mut h, 0x1000_0010, 0x81, 1100); // /16
        assert_eq!(rd(&h, 0x1000_0000, 1100), 50, "latched before the mode change");
        assert_eq!(rd(&h, 0x1000_0000, 1100 + 320), 60);
        wr(&mut h, 0x1000_0000, 0xFFFF, 1420);
        assert_eq!(rd(&h, 0x1000_0000, 1420 + 64), 1, "16-bit wrap");
    }

    #[test]
    fn dma_start_completes_at_once_and_sets_status() {
        let mut h = Hw::new();
        wr(&mut h, 0x1000_A000, 0x105, 0);
        assert_eq!(rd(&h, 0x1000_A000, 0) & 0x100, 0, "STR reads back clear");
        assert_eq!(rd(&h, D_STAT, 0) & 4, 4, "channel 2 (GIF) status bit");
    }

    #[test]
    fn ipu_reset_clears_and_the_unit_is_never_busy() {
        let mut h = Hw::new();
        wr(&mut h, IPU_CMD, 0x8000_0001, 0);
        assert_eq!(rd(&h, IPU_CMD, 0) >> 31, 0, "a command is done as soon as it is written");
        wr(&mut h, IPU_CTRL, 0x00C0_00FF | 1 << 31, 0);
        assert_eq!(rd(&h, IPU_CTRL, 0), 0x00C0_0000, "FIFO counts and BUSY are read-only");
        h.poke(IPU_BP, 0x1234);
        wr(&mut h, IPU_CTRL, 1 << 30, 0);
        assert_eq!((rd(&h, IPU_CTRL, 0), rd(&h, IPU_BP, 0), h.peek(IPU_CMD)), (0, 0, 0), "RST resets the unit and clears itself");
    }

    #[test]
    fn gs_csr_identifies_and_clears_flags() {
        let mut h = Hw::new();
        assert_eq!(rd(&h, GS_CSR, 0) >> 16, 0x551B);
        h.poke(GS_CSR, 0x551B_4003);
        wr(&mut h, GS_CSR, 0x2, 0);
        assert_eq!(rd(&h, GS_CSR, 0), 0x551B_4001);
    }

    #[test]
    fn vu_memory_is_plain_storage() {
        let mut h = Hw::new();
        h.write(0, 0x1100_4010, &[1, 2, 3, 4], 0);
        let mut b = [0u8; 4];
        h.read(0x1100_4010, &mut b, 0);
        assert_eq!(b, [1, 2, 3, 4]);
        assert!(h.log.is_empty(), "VU memory writes are not register writes");
    }
}
