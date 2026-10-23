//! `SetVSyncFlag` (syscall 0x73), which the kernel module does not implement: the kernel sets a
//! flag word and stores the GS CSR (a 64-bit value) at every vertical blank [SDK]. The only caller
//! in the executable is the wait at 0x3188c0 [V], which polls INTC_STAT bit 2 and the flag and
//! returns that stored word; its caller 0x2f8ec8 returns bit 13 of it (the FIELD bit) [V]. When the game
//! has already enabled the VBlank interrupt the kernel's dispatch consumes the status bit before
//! the poll sees it, so on the real machine only the flag ends the wait.
//!
//! The flag is not written from an interrupt here: the pointers are addresses on the caller's
//! stack and the function returns right after the wait, so a write at a later blank would hit a
//! dead frame. Instead the poll loop (the flag read at 0x318920) is probed: when a vblank has
//! passed since the call, the flag is set and the CSR word stored, just before the read. The word is the CSR the hardware
//! model holds with FIELD (bit 13) following the parity of the vblank number and FINISH (bit 1)
//! set [inferred; the GS CSR model is the hardware package's].

use kh2_ee::{Ee, reg};

use super::Handle;
use crate::kernel::VBLANK_CYCLES;

pub const SYSCALL: i32 = 0x73;
/// The load of the flag in the wait loop at 0x3188c0 [V: disassembly].
pub const POLL: u32 = 0x318920;
const GS_CSR: u32 = 0x1200_1000;

/// The pointers given to the last call and the vblank number it was made in.
#[derive(Clone, Copy, Debug, Default)]
pub struct VsyncFlag {
    pub flag: u32,
    pub tick: u32,
    pub since: u64,
}

pub(super) fn install(ee: &mut Ee, iop: &Handle) {
    let h = iop.clone();
    ee.syscalls.insert(SYSCALL, move |ee: &mut Ee| {
        let mut iop = h.borrow_mut();
        let now = iop.kernel.borrow().clock.now(ee.steps) / VBLANK_CYCLES;
        iop.vsync_flag = Some(VsyncFlag { flag: ee.gpr[reg::A0] as u32, tick: ee.gpr[reg::A0 + 1] as u32, since: now });
        ee.gpr[reg::V0] = 0;
    });
    let h = iop.clone();
    ee.probes.insert(POLL, move |ee: &mut Ee| {
        let mut iop = h.borrow_mut();
        let now = iop.kernel.borrow().clock.now(ee.steps) / VBLANK_CYCLES;
        if let Some(v) = iop.vsync_flag.as_mut()
            && now > v.since
        {
            let csr = (ee.hw.peek(GS_CSR) & !(1 << 13)) | 2 | ((now as u32 & 1) << 13);
            ee.w32(v.tick, csr);
            ee.w32(v.tick + 4, 0);
            ee.w32(v.flag, 1);
            v.since = now;
        }
    });
}
