//! COP0: system registers, exceptions, interrupts and the Count/Compare timer.
//!
//! Register layout (EE core user's manual, COP0 chapter): Index 0, Random 1, EntryLo0 2, EntryLo1 3,
//! Context 4, PageMask 5, Wired 6, BadVAddr 8, Count 9, EntryHi 10, Compare 11, Status 12 (IE bit 0,
//! EXL 1, ERL 2, KSU 3-4, IM2/IM3 bits 10-11, IM7 bit 15, EIE bit 16, BEV bit 22), Cause 13 (ExcCode
//! bits 2-6, IP bits 8-15, IV bit 23, BD bit 31), EPC 14, PRId 15, Config 16, ErrorEPC 30.

use crate::{Ee, Exception, Fault};

const STATUS: usize = 12;
const CAUSE: usize = 13;
const EPC: usize = 14;
const COMPARE: usize = 11;

/// COP0 state a program finds at start: kernel mode, IE set and every IM bit set but EIE clear
/// (so nothing is taken until the program executes `ei`) [inferred; the BIOS hands over a state
/// like this], PRId 0x2E20 and Config 0x440 as PCSX2 reports them.
pub(crate) fn reset_state() -> [u32; 32] {
    let mut c = [0u32; 32];
    c[1] = 47;
    c[STATUS] = 0x0000_FF01;
    c[15] = 0x0000_2E20;
    c[16] = 0x0000_0440;
    c
}

impl Ee {
    /// The Count register: one per executed instruction since it was last written.
    pub fn count(&self) -> u32 {
        self.count_base.wrapping_add((self.steps - self.count_since) as u32)
    }

    fn recompute_timer(&mut self) {
        let diff = self.cop0[COMPARE].wrapping_sub(self.count());
        self.timer_due = self.steps + if diff == 0 { 1 << 32 } else { diff as u64 };
        self.next_event = 0;
    }

    /// Sets or clears Cause.IP bit `line` (0 to 7; 2 is the INTC line, 3 the DMAC line). The
    /// interrupt is taken at the next instruction boundary if Status allows it.
    pub fn set_interrupt_pending(&mut self, line: u8, on: bool) {
        let bit = 1u32 << (8 + (line & 7));
        if on {
            self.cop0[CAUSE] |= bit;
        } else {
            self.cop0[CAUSE] &= !bit;
        }
        self.next_event = 0;
    }

    fn interrupt_ready(&self) -> bool {
        let s = self.cop0[STATUS];
        s & 1 != 0 && s & (1 << 16) != 0 && s & 6 == 0 && self.cop0[CAUSE] & s & 0xFF00 != 0
    }

    /// Looks at the Count/Compare timer and the pending interrupts; called by the run loop when
    /// `next_event` is reached.
    pub(crate) fn service_events(&mut self, end: u64) -> Result<(), Fault> {
        if self.steps >= self.timer_due {
            self.cop0[CAUSE] |= 1 << 15;
            self.recompute_timer();
        }
        if self.interrupt_ready() {
            self.dispatch_exception(Exception::Interrupt)?;
        }
        self.next_event = end.min(self.timer_due);
        Ok(())
    }

    /// Records an exception in the COP0 registers as the hardware does.
    fn enter_exception(&mut self, exc: Exception) {
        // A taken branch leaves the program counter on its delay slot with the target pending.
        let in_delay = self.npc != self.pc.wrapping_add(4);
        let epc = if in_delay { self.pc.wrapping_sub(4) } else { self.pc };
        let mut cause = self.cop0[CAUSE] & !(0x1F << 2) & !(1 << 31);
        cause |= exc.code() << 2;
        if in_delay {
            cause |= 1 << 31;
        }
        self.cop0[CAUSE] = cause;
        if self.cop0[STATUS] & 2 == 0 {
            self.cop0[EPC] = epc;
        }
        self.cop0[STATUS] |= 2;
        if let Exception::AddressError { addr, .. } = exc {
            self.cop0[8] = addr;
        }
    }

    pub(crate) fn dispatch_exception(&mut self, exc: Exception) -> Result<(), Fault> {
        if let Exception::Syscall(n) = exc
            && let Some(h) = self.syscalls.get(n)
        {
            // The handler runs as the kernel would, then execution continues after the syscall.
            self.pc = self.npc;
            self.npc = self.npc.wrapping_add(4);
            self.redirected = false;
            self.invoke(&h)?;
            self.sync_special();
            return Ok(());
        }
        let fault_pc = self.pc;
        self.enter_exception(exc);
        if let Some(mut h) = self.exception_handler.take() {
            h(self, exc);
            if self.exception_handler.is_none() {
                self.exception_handler = Some(h);
            }
            return Ok(());
        }
        if exc == Exception::Interrupt {
            let bev = self.cop0[STATUS] & (1 << 22) != 0;
            let base = if bev { 0xBFC0_0200u32 } else { 0x8000_0000 };
            let off = if self.cop0[CAUSE] & (1 << 23) != 0 { 0x200 } else { 0x180 };
            self.jump(base + off);
            return Ok(());
        }
        Err(Fault::Exception { pc: fault_pc, exc })
    }

    fn mfc0(&self, r: usize) -> u32 {
        match r {
            9 => self.count(),
            // Random cycles down from 47 to Wired.
            1 => {
                let wired = (self.cop0[6] & 63).min(47) as u64;
                (47 - (self.steps % (48 - wired))) as u32
            }
            _ => self.cop0[r],
        }
    }

    fn mtc0(&mut self, r: usize, v: u32) {
        match r {
            // Read-only: Random, BadVAddr, PRId.
            1 | 8 | 15 => {}
            9 => {
                self.count_base = v;
                self.count_since = self.steps;
                self.recompute_timer();
            }
            COMPARE => {
                self.cop0[COMPARE] = v;
                // Writing Compare clears the timer interrupt.
                self.cop0[CAUSE] &= !(1 << 15);
                self.recompute_timer();
            }
            STATUS => {
                self.cop0[STATUS] = v;
                self.next_event = 0;
            }
            CAUSE => {
                // Only the software interrupt bits IP0 and IP1 and IV can be written.
                let keep = self.cop0[CAUSE] & !0x0080_0300;
                self.cop0[CAUSE] = keep | (v & 0x0080_0300);
                self.next_event = 0;
            }
            _ => self.cop0[r] = v,
        }
    }

    /// Opcode 0x10: `mfc0`, `mtc0`, `bc0*` and the CO functions (`tlb*`, `eret`, `ei`, `di`).
    pub(crate) fn cop0_op(&mut self, w: u32, rs: usize, rt: usize, rd: usize, funct: u32) {
        if rs & 0x10 != 0 {
            match funct {
                // TLB instructions: the TLB is not modelled, the registers are left alone.
                0x01 | 0x02 | 0x06 | 0x08 => {}
                0x18 => {
                    // eret: no delay slot; ERL selects ErrorEPC.
                    let target = if self.cop0[STATUS] & 4 != 0 {
                        self.cop0[STATUS] &= !4;
                        self.cop0[30]
                    } else {
                        self.cop0[STATUS] &= !2;
                        self.cop0[EPC]
                    };
                    self.npc = target;
                    self.npc_next = Some(target.wrapping_add(4));
                    self.next_event = 0;
                }
                0x38 => {
                    self.cop0[STATUS] |= 1 << 16;
                    self.next_event = 0;
                }
                0x39 => self.cop0[STATUS] &= !(1 << 16),
                _ => self.unimpl(w, "cop0 co"),
            }
            return;
        }
        match rs {
            0x00 => {
                let v = self.mfc0(rd);
                self.set(rt, crate::sx32(v));
            }
            0x04 => {
                let v = self.g32(rt);
                self.mtc0(rd, v);
            }
            0x08 => {
                let bt = self.pc.wrapping_add(4).wrapping_add(((w as u16 as i16 as i32) << 2) as u32);
                match rt {
                    0 => self.branch(!self.cpcond0, bt, false),
                    1 => self.branch(self.cpcond0, bt, false),
                    2 => self.branch(!self.cpcond0, bt, true),
                    3 => self.branch(self.cpcond0, bt, true),
                    _ => self.unimpl(w, "bc0"),
                }
            }
            _ => self.unimpl(w, "cop0"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Stop;

    const BASE: u32 = 0x0010_0000;

    fn mfc0(rt: u32, rd: u32) -> u32 {
        (0x10 << 26) | (rt << 16) | (rd << 11)
    }
    fn mtc0(rt: u32, rd: u32) -> u32 {
        (0x10 << 26) | (4 << 21) | (rt << 16) | (rd << 11)
    }
    const EI: u32 = 0x4200_0038;
    const DI: u32 = 0x4200_0039;
    const ERET: u32 = 0x4200_0018;
    const NOP: u32 = 0;
    const JR_RA: u32 = 0x03E0_0008;

    fn load(e: &mut Ee, at: u32, code: &[u32]) {
        for (i, w) in code.iter().enumerate() {
            e.w32(at + 4 * i as u32, *w);
        }
    }

    #[test]
    fn mfc0_mtc0_move_registers() {
        // R5900 manual, COP0: MFC0 reads PRId (0x2E20), MTC0 writes Compare; BadVAddr and PRId ignore writes.
        let mut e = Ee::new();
        e.gpr[8] = 0x1234;
        load(&mut e, BASE, &[mtc0(8, 11), mfc0(2, 11), mfc0(3, 15), mtc0(8, 15), mfc0(4, 15), JR_RA, NOP]);
        e.call(BASE, &[], &[], 0x01F0_0000).unwrap();
        assert_eq!(e.gpr[2] as u64, 0x1234);
        assert_eq!(e.gpr[3] as u64, 0x2E20);
        assert_eq!(e.gpr[4] as u64, 0x2E20, "PRId is read-only");
        assert_eq!(e.cop0[11], 0x1234);
    }

    #[test]
    fn ei_and_di_set_and_clear_eie() {
        let mut e = Ee::new();
        assert_eq!(e.cop0[12] & (1 << 16), 0);
        load(&mut e, BASE, &[EI, JR_RA, NOP]);
        e.call(BASE, &[], &[], 0x01F0_0000).unwrap();
        assert_ne!(e.cop0[12] & (1 << 16), 0, "EE core manual: EI sets Status.EIE");
        load(&mut e, BASE, &[DI, JR_RA, NOP]);
        e.call(BASE, &[], &[], 0x01F0_0000).unwrap();
        assert_eq!(e.cop0[12] & (1 << 16), 0, "DI clears it");
    }

    #[test]
    fn bc0_tests_cpcond0() {
        // bc0t +2 (taken when CPCOND0 is true), then v0 = 1 skipped, v0 = 2 executed.
        let bc0t = (0x10 << 26) | (8 << 21) | (1 << 16) | 2;
        let bc0f = (0x10 << 26) | (8 << 21) | 2;
        for (word, expect) in [(bc0t, 0u64), (bc0f, 1)] {
            let mut e = Ee::new();
            // The branch skips `v0 = 1` when taken.
            load(&mut e, BASE, &[word, NOP, 0x2402_0001, JR_RA, NOP]);
            e.call(BASE, &[], &[], 0x01F0_0000).unwrap();
            // With CPCOND0 true, BC0T is taken and BC0F falls through.
            assert_eq!(e.gpr[2] as u64, expect, "{word:08x}");
        }
    }

    #[test]
    fn count_advances_one_per_instruction_and_compare_raises_ip7() {
        let mut e = Ee::new();
        // Compare = 100, interrupts masked at the core (EIE clear): IP7 shows up in Cause only.
        e.gpr[8] = 100;
        load(&mut e, BASE, &[mtc0(8, 11), NOP, NOP, 0x1000_FFFF, NOP]); // mtc0, then branch-to-self loop
        e.pc = BASE;
        e.npc = BASE + 4;
        let r = e.run(300).unwrap();
        assert_eq!(r, Stop::Steps);
        assert_ne!(e.cop0[13] & (1 << 15), 0, "Count passed Compare: Cause.IP7 set");
        assert!(e.count() >= 300 - 5);
        // Writing Compare clears it again.
        load(&mut e, BASE + 0x100, &[mtc0(8, 11), JR_RA, NOP]);
        e.gpr[8] = 5000;
        e.call(BASE + 0x100, &[], &[], 0x01F0_0000).unwrap();
        assert_eq!(e.cop0[13] & (1 << 15), 0);
    }

    #[test]
    fn interrupt_is_taken_when_enabled_and_eret_returns() {
        // Main code at BASE: ei; then a counting loop. Handler (a hook at the vector) clears the
        // pending line and returns with eret semantics (jump to EPC).
        let mut e = Ee::new();
        e.cop0[13] |= 1 << 23; // Cause.IV: interrupts use the 0x200 vector
        load(&mut e, BASE, &[EI, 0x2508_0001, 0x1000_FFFE, NOP]); // ei; loop: addiu t0,t0,1; b loop; nop
        let taken = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let t2 = taken.clone();
        e.hooks.insert(0x8000_0200, move |ee: &mut Ee| {
            t2.set(t2.get() + 1);
            let epc = ee.cop0[14];
            ee.set_interrupt_pending(2, false);
            ee.cop0[12] &= !2; // what eret does
            ee.jump(epc);
        });
        e.pc = BASE;
        e.npc = BASE + 4;
        e.run(10).unwrap();
        assert_eq!(taken.get(), 0, "nothing pending yet");
        e.set_interrupt_pending(2, true);
        e.run(10).unwrap();
        assert_eq!(taken.get(), 1, "INTC line 2 taken once: IE, EIE and IM2 are set");
        assert_eq!(e.cop0[13] >> 2 & 0x1F, 0, "ExcCode Int");
        let t = e.gpr[8] as u32;
        e.run(10).unwrap();
        assert!(e.gpr[8] as u32 > t, "the loop resumed");
        assert_eq!(taken.get(), 1);
    }

    #[test]
    fn masked_or_disabled_interrupts_are_not_taken() {
        for (status, name) in [(0x0000_FF01u32, "EIE clear"), (0x0001_0001 | 0x0000_0001, "IM clear"), (0x0001_FF03, "EXL set")] {
            let mut e = Ee::new();
            e.cop0[12] = if name == "IM clear" { 0x0001_0001 } else { status };
            load(&mut e, BASE, &[0x1000_FFFF, NOP]);
            e.set_interrupt_pending(2, true);
            e.pc = BASE;
            e.npc = BASE + 4;
            let r = e.run(50);
            assert!(matches!(r, Ok(Stop::Steps)), "{name}: {r:?}");
            assert_eq!(e.cop0[14], 0, "{name}: no exception recorded");
        }
    }

    #[test]
    fn exception_in_a_delay_slot_records_the_branch() {
        // R5900 manual, exceptions: EPC points at the branch and Cause.BD is set when the
        // exception happens in its delay slot.
        let mut e = Ee::new();
        let seen = std::rc::Rc::new(std::cell::RefCell::new(None));
        let s2 = seen.clone();
        e.set_exception_handler(move |ee: &mut Ee, exc| {
            *s2.borrow_mut() = Some((exc, ee.cop0[14], ee.cop0[13]));
            ee.request_stop();
        });
        // b +2 (to BASE + 12) with a `break 5` delay slot.
        load(&mut e, BASE, &[0x1000_0002, (5 << 6) | 0x0D, NOP, NOP]);
        e.pc = BASE;
        e.npc = BASE + 4;
        assert_eq!(e.run(10).unwrap(), Stop::Requested);
        let (exc, epc, cause) = seen.borrow().unwrap();
        assert_eq!(exc, Exception::Break(5));
        assert_eq!(epc, BASE);
        assert_ne!(cause & (1 << 31), 0, "BD");
        assert_eq!(cause >> 2 & 0x1F, 9, "ExcCode Bp");
        assert_ne!(e.cop0[12] & 2, 0, "EXL");
    }

    #[test]
    fn eret_returns_to_epc_and_clears_exl() {
        let mut e = Ee::new();
        e.cop0[12] = 0x0000_0003; // EXL set
        e.cop0[14] = BASE + 0x40;
        load(&mut e, BASE, &[ERET, 0x2402_0009]);
        load(&mut e, BASE + 0x40, &[0x2402_0007, JR_RA, NOP]);
        e.gpr[31] = crate::SENTINEL as u128;
        e.pc = BASE;
        e.npc = BASE + 4;
        assert_eq!(e.run(10).unwrap(), Stop::Returned);
        assert_eq!(e.gpr[2] as u64, 7, "execution continued at EPC, not after the eret");
        assert_eq!(e.cop0[12] & 2, 0, "EXL cleared");
    }
}
