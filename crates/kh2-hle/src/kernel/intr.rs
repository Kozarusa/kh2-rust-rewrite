//! INTC and DMAC handler tables and interrupt delivery.
//!
//! The kernel keeps the pending state in the same registers the game reads: INTC_STAT (0x1000f000)
//! and INTC_MASK (0x1000f010), D_STAT (0x1000e010; status in the low half, the channel mask in
//! the high half), written through `Hw::poke` (the stub's own write rules apply only to guest
//! stores). A line is pending when its status bit and its mask bit are set. The core's Cause.IP2
//! (INTC) and IP3 (DMAC) bits mirror that, so [`kh2_ee::Ee`] takes the interrupt at the first
//! instruction boundary where Status allows it, and the exception handler of the machine calls
//! [`dispatch`] [SDK: the real kernel does the same from its exception vector].
//!
//! A handler is called as `handler(cause_or_channel, arg, 0)` with the gp it was registered with
//! and a private stack, returns through the call sentinel, and its status bit is cleared before
//! it runs (the real kernel acknowledges the line) [inferred].

use kh2_ee::Ee;

use super::{INT_STACK_TOP, Kernel, Shared};

pub const INTC_STAT: u32 = 0x1000_F000;
pub const INTC_MASK: u32 = 0x1000_F010;
pub const D_STAT: u32 = 0x1000_E010;

/// INTC cause numbers the game uses.
pub const CAUSE_VBLANK_START: u32 = 2;
pub const CAUSE_TIMER2: u32 = 11;
/// The DMAC channel of the SIF0 (IOP to EE) command handler.
pub const DMAC_SIF0: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntHandler {
    pub id: u32,
    pub addr: u32,
    pub arg: u32,
    pub gp: u32,
}

#[derive(Default)]
pub struct Intr {
    pub intc: [Vec<IntHandler>; 16],
    pub dmac: [Vec<IntHandler>; 16],
    next_id: u32,
}

/// One acknowledged interrupt: the handlers to run, in order.
pub struct Job {
    pub line: u32,
    pub handlers: Vec<IntHandler>,
}

impl Intr {
    pub fn new_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id
    }

    /// Inserts a handler in front of the one with id `next` (0 = at the head, -1 = at the end;
    /// an unknown id also means the end) [SDK].
    pub fn insert(list: &mut Vec<IntHandler>, h: IntHandler, next: i32) {
        let at = match next {
            0 => 0,
            n if n > 0 => list.iter().position(|x| x.id == n as u32).unwrap_or(list.len()),
            _ => list.len(),
        };
        list.insert(at, h);
    }
}

/// The lowest pending line and its handlers, acknowledged (status bit cleared); INTC before DMAC.
pub fn next_job(k: &mut Kernel, ee: &mut Ee) -> Option<Job> {
    let (stat, mask) = (ee.hw.peek(INTC_STAT), ee.hw.peek(INTC_MASK));
    let pend = stat & mask & 0x7FFF;
    if pend != 0 {
        let line = pend.trailing_zeros();
        ee.hw.poke(INTC_STAT, stat & !(1 << line));
        return Some(Job { line, handlers: k.intr.intc[line as usize].clone() });
    }
    let d = ee.hw.peek(D_STAT);
    let pend = (d & 0xFFFF) & (d >> 16);
    if pend != 0 {
        let line = pend.trailing_zeros();
        ee.hw.poke(D_STAT, d & !(1 << line));
        return Some(Job { line, handlers: k.intr.dmac[line as usize].clone() });
    }
    None
}

/// Sets Cause.IP2 and IP3 from the pending state.
pub fn refresh_lines(ee: &mut Ee) {
    let intc = ee.hw.peek(INTC_STAT) & ee.hw.peek(INTC_MASK) & 0x7FFF != 0;
    let d = ee.hw.peek(D_STAT);
    let dmac = (d & 0xFFFF) & (d >> 16) != 0;
    ee.set_interrupt_pending(2, intc);
    ee.set_interrupt_pending(3, dmac);
}

/// Runs every pending interrupt handler to completion. `guest_running` is true when a thread's
/// registers are live in `ee` (they are saved and restored around the handlers) and false in the
/// idle loop. Returns the number of handlers called. The kernel is not borrowed while guest code
/// runs, so handlers may make syscalls.
pub fn dispatch(shared: &Shared, ee: &mut Ee, guest_running: bool) -> Result<u32, kh2_ee::Fault> {
    let saved = guest_running.then(|| ee.context());
    // Status as the interrupted code had it: the exception entry set EXL, the handlers may `ei`.
    let status = ee.cop0[12] & !2;
    let mut called = 0;
    loop {
        let Some(job) = next_job(&mut shared.borrow_mut(), ee) else { break };
        for h in &job.handlers {
            shared.borrow_mut().in_isr += 1;
            ee.gpr[28] = h.gp as u128;
            let r = ee.call(h.addr, &[job.line as u64, h.arg as u64, 0], &[], INT_STACK_TOP);
            shared.borrow_mut().in_isr -= 1;
            r?;
            called += 1;
        }
    }
    if let Some(c) = saved {
        ee.set_context(&c);
        ee.cop0[12] = status;
    }
    // Cause.IP7 (the core's Count/Compare timer) has no handler in this machine.
    ee.cop0[13] &= !(1 << 15);
    refresh_lines(ee);
    Ok(called)
}
