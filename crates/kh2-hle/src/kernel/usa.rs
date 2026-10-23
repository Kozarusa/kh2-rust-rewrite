//! Parts of the kernel that live in the USA executable's own libkernel (addresses of
//! `SLUS_210.05`). The boot contract (section 2.3) says `DelayThread` is library code: it
//! creates a semaphore, arms an alarm on timer 2 and waits. The alarm clock is replaced by the
//! virtual clock, so the function itself is hooked.

use kh2_ee::{Ee, reg};

use super::{Event, Kernel, Machine, Post, Wait, complete};

/// `DelayThread(usec)` in libkernel [V: disassembly, contract 2.3].
pub const DELAY_THREAD: u32 = 0x319418;

/// The error `DelayThread` returns when `mfc0 Status` shows interrupts disabled [V: contract 2.3].
pub const DELAY_ERR_INT_DISABLED: i32 = 0x8000_8008u32 as i32;

/// Sleeps the calling thread for `usec` microseconds of virtual time.
pub fn delay_thread(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let usec = ee.gpr[reg::A0] as u32 as u64;
    if ee.cop0[12] & (1 << 16) == 0 || k.in_isr > 0 {
        return (DELAY_ERR_INT_DISABLED, Post::Nothing);
    }
    let Some(c) = k.cur else { return (DELAY_ERR_INT_DISABLED, Post::Nothing) };
    let at = k.clock.now(ee.steps) + super::Clock::usec_to_cycles(usec).max(1);
    k.schedule_now(ee, at, Event::Wake(c));
    (0, Post::Block(Wait::Delay))
}

impl Machine {
    /// Installs the hooks on functions of the USA executable's libkernel.
    pub fn install_usa_hooks(&mut self) {
        let s = self.kernel.clone();
        self.ee.hooks.insert(DELAY_THREAD, move |ee: &mut Ee| {
            let mut k = s.borrow_mut();
            let ra = ee.gpr[reg::RA] as u32;
            // The caller resumes at its return address.
            ee.jump(ra);
            let r = delay_thread(&mut k, ee);
            complete(&mut k, ee, r);
        });
    }
}
