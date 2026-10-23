//! Hardware register pages (boot contract section 3, HLE column), HYB-5.
//!
//! `kh2-ee` keeps a small stub of the memory-mapped registers: reads give the last written value,
//! writes are logged, a DMA start completes at once and scratchpad transfers in normal mode really
//! copy. [`HwModel`] sits on top of it. After every slice of guest execution [`HwModel::sync`]
//! drains the stub's write log and
//!
//! * checks each write against the register pages ([`pages::PAGES`]); a write outside them is
//!   recorded in [`HwModel::unimplemented`] and [`HwModel::assert_clean`] fails on it;
//! * finishes DMA starts the way the controller does: the transfer count reads 0 and the address
//!   registers advance (normal mode), the channel's status bit raises the DMAC line, and the SIF
//!   channels (5 to 7, whose other end is the IOP) get no status bit from arming. This is the fix
//!   for the arming issue HYB-3 noted: the stub sets D_STAT when a channel starts, so re-arming
//!   SIF0 from its handler would otherwise fire the registered DMAC handler again;
//! * drives the registers that follow time: GS CSR FIELD (bit 13) is the vsync count modulo two,
//!   VSINT (bit 3) is set at a vsync, FINISH (bit 1) is set at once because drawing is never
//!   rendered; timer 0 COUNT counts scan lines (HBLANK) of the virtual clock. Timer 2 overflow and
//!   INTC 11 come from the kernel clock; the SIO ISR already reads "transmit ready" (bit 15, the
//!   transmit FIFO full flag the debug print loop waits on, is 0) [V: 0x319bc8].
//!
//! Limits (all need a `kh2-ee` change, listed in the HYB-5 report): reads are not logged, so only
//! writes can be flagged; effects land at the next sync (at most [`HwMachine::slice`] steps late),
//! not at the instruction that wrote.

pub mod movie;
pub mod pages;

use std::collections::BTreeMap;

use kh2_ee::hw::HwWrite;
use kh2_ee::{Ee, Fault};

use crate::kernel::{Kernel, Machine, RunStop, VBLANK_CYCLES};

pub const INTC_STAT: u32 = 0x1000_F000;
pub const D_STAT: u32 = 0x1000_E010;
pub const GS_CSR: u32 = 0x1200_1000;
pub const T0_COUNT: u32 = 0x1000_0000;
pub const T0_MODE: u32 = 0x1000_0010;
pub const SIO_ISR: u32 = 0x1000_F130;
pub const SIO_TXFIFO: u32 = 0x1000_F180;

pub const CSR_FINISH: u32 = 1 << 1;
pub const CSR_VSINT: u32 = 1 << 3;
pub const CSR_FIELD: u32 = 1 << 13;

/// An access outside the register pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unimplemented {
    pub pc: u32,
    pub addr: u32,
    pub size: u8,
    pub value: u128,
    /// The movie-only page it fell in, if any (the movie hook should have kept the game out).
    pub movie_only: Option<&'static str>,
}

/// Scan lines (HBLANKs) since reset at `cycles` EE cycles: 525 lines per frame, two fields per
/// frame, one field per VBlank period.
pub fn hblanks(cycles: u64) -> u64 {
    cycles * 525 / (2 * VBLANK_CYCLES)
}

#[derive(Default)]
pub struct HwModel {
    pub unimplemented: Vec<Unimplemented>,
    /// Writes checked so far.
    pub writes: u64,
    /// CHCR starts per channel.
    pub dma_starts: [u64; 10],
    /// Bytes sent to the SIO transmit FIFO (debug print).
    pub sio_tx: Vec<u8>,
    /// Last value written to each GS privileged register.
    pub gs_priv: BTreeMap<u32, u128>,
    /// Vsyncs the CSR was last updated for.
    vsyncs_seen: u64,
    /// Added to the HBLANK count to give timer 0's COUNT (the game writes the counter).
    t0_offset: i64,
}

impl HwModel {
    pub fn new() -> HwModel {
        HwModel::default()
    }

    /// Panics, listing the accesses, if any register outside the pages was written.
    pub fn assert_clean(&self) {
        assert!(self.unimplemented.is_empty(), "unimplemented register access: {:x?}", self.unimplemented);
    }

    /// Applies the log of guest writes since the last call and the time-driven registers.
    pub fn sync(&mut self, ee: &mut Ee, k: &mut Kernel) {
        let now = k.clock.now(ee.steps);
        let writes: Vec<HwWrite> = ee.hw.log.drain(..).collect();
        for w in &writes {
            self.writes += 1;
            self.on_write(ee, k, now, w);
        }
        // GS CSR: FIELD alternates with the vsync count.
        let vs = k.clock.vsyncs;
        let mut csr = ee.hw.peek(GS_CSR) | CSR_FINISH;
        csr = (csr & !CSR_FIELD) | if vs & 1 == 1 { CSR_FIELD } else { 0 };
        if vs != self.vsyncs_seen {
            csr |= CSR_VSINT;
            self.vsyncs_seen = vs;
        }
        ee.hw.poke(GS_CSR, csr);
        // Timer 0 counts scan lines while enabled (CUE, MODE bit 7). Writing COUNT through the
        // stub latches it at the current step count.
        if ee.hw.peek(T0_MODE) & 0x80 != 0 {
            let count = (hblanks(now) as i64).wrapping_add(self.t0_offset) as u32 & 0xFFFF;
            ee.write(T0_COUNT, &count.to_le_bytes());
            ee.hw.log.clear();
        }
    }

    fn on_write(&mut self, ee: &mut Ee, k: &mut Kernel, now: u64, w: &HwWrite) {
        if pages::page_of(w.addr).is_none() {
            let movie_only = pages::movie_page_of(w.addr).map(|p| p.name);
            self.unimplemented.push(Unimplemented { pc: w.pc, addr: w.addr, size: w.size, value: w.value, movie_only });
            return;
        }
        match w.addr {
            T0_COUNT => self.t0_offset = (w.value as u32 & 0xFFFF) as i64 - hblanks(now) as i64,
            SIO_TXFIFO => self.sio_tx.push(w.value as u8),
            a if (0x1200_0000..0x1200_0100).contains(&a) => {
                self.gs_priv.insert(a & !7, w.value);
            }
            a => {
                if let Some(ch) = pages::channel_of_chcr(a)
                    && w.value & 0x100 != 0
                {
                    if pages::start_allowed(ch) {
                        self.dma_start(ee, k, ch, w.value as u32);
                    } else {
                        let movie_only = matches!(ch, 3 | 4).then_some(pages::PAGES.iter().find(|p| p.lo == a).map_or("", |p| p.name));
                        self.unimplemented.push(Unimplemented { pc: w.pc, addr: w.addr, size: w.size, value: w.value, movie_only });
                    }
                }
            }
        }
    }

    /// The controller's side of a started transfer; the stub has already run the data movement.
    fn dma_start(&mut self, ee: &mut Ee, k: &mut Kernel, ch: u32, chcr: u32) {
        self.dma_starts[ch as usize] += 1;
        let base = pages::CHANNEL_BASE[ch as usize];
        let qwc = ee.hw.peek(base + 0x20) & 0xFFFF;
        if chcr & 0xC == 0 {
            // Normal mode: addresses advance past the data, the count is used up.
            let adv = qwc * 16;
            let madr = ee.hw.peek(base + 0x10);
            ee.hw.poke(base + 0x10, madr.wrapping_add(adv));
            if ch == 8 || ch == 9 {
                let sadr = ee.hw.peek(base + 0x80);
                ee.hw.poke(base + 0x80, sadr.wrapping_add(adv) & 0x3FFF);
            }
        }
        ee.hw.poke(base + 0x20, 0);
        if pages::iop_channel(ch) {
            // Arming a SIF channel completes nothing (only the IOP does).
            let st = ee.hw.peek(D_STAT);
            ee.hw.poke(D_STAT, st & !(1 << ch));
        } else {
            // Completion raised the status bit in the stub; let the DMAC line see it.
            k.raise_dmac(ee, ch);
        }
    }
}

/// The interpreter, the kernel and the hardware pages together.
pub struct HwMachine {
    pub machine: Machine,
    pub hw: HwModel,
    /// Guest instructions between two syncs of the hardware model.
    pub slice: u64,
    /// Calls the movie hook answered (see [`movie`]).
    pub movies: movie::MovieLog,
}

impl Default for HwMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl HwMachine {
    /// A machine with the movie hook installed.
    pub fn new() -> HwMachine {
        let mut machine = Machine::new();
        let movies = movie::install(&mut machine.ee);
        HwMachine { machine, hw: HwModel::new(), slice: 128, movies }
    }

    /// Loads the executable and installs the hooks on its libkernel (`Machine::install_usa_hooks`).
    pub fn load_elf(&mut self, elf: &[u8]) -> Option<u32> {
        let entry = self.machine.load_elf(elf)?;
        self.machine.install_usa_hooks();
        Some(entry)
    }

    /// Syncs the hardware model now.
    pub fn sync(&mut self) {
        let mut k = self.machine.kernel.borrow_mut();
        self.hw.sync(&mut self.machine.ee, &mut k);
    }

    /// Like [`Machine::run`], with the hardware model synced every [`HwMachine::slice`] steps.
    pub fn run(&mut self, max_steps: u64) -> Result<RunStop, Fault> {
        let start = self.machine.ee.steps;
        let idle_start = self.machine.kernel.borrow().clock.skipped;
        loop {
            let used = self.machine.ee.steps - start;
            let idle = self.machine.kernel.borrow().clock.skipped - idle_start;
            if used >= max_steps || idle >= self.machine.idle_limit {
                return Ok(RunStop::Budget);
            }
            let r = self.machine.run((max_steps - used).min(self.slice))?;
            self.sync();
            if r != RunStop::Budget {
                return Ok(r);
            }
        }
    }
}

#[cfg(test)]
mod tests;
