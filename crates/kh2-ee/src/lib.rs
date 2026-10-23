//! Emotion Engine interpreter: the R5900 integer core (64/128-bit GPRs, the full MMI set), the COP0
//! system registers, the COP1 FPU and COP2 (VU0) macro mode with its flag registers. There is no
//! operating system, DMA engine or VU microprogram execution: the crate exists to run routines of
//! the user's own game executable (loaded from their disc at run time) with prepared memory, so
//! native ports can be checked against the original behaviour bit for bit.
//!
//! Instruction semantics follow the public R5900 and VU user manuals; behaviour of corner cases
//! (FPU clamping, LWR sign extension) follows the conventions documented by PCSX2.
//!
//! # Calling and running
//!
//! * [`Ee::call`] runs one function to its return (the return address is the sentinel
//!   [`SENTINEL`]) and returns `v0`. It may be used again from inside a hook; the return address,
//!   stack pointer and program counter of the interrupted code are restored afterwards.
//! * [`Ee::run`] runs from the current program counter for a number of instructions, for a host
//!   scheduler that interleaves several threads and events: it returns [`Stop::Steps`] when the
//!   budget is used, [`Stop::Returned`] when the sentinel is reached and [`Stop::Requested`] when
//!   a handler called [`Ee::request_stop`]. [`Ee::context`] and [`Ee::set_context`] save and load
//!   the register file of a thread; [`Ee::jump`] sets the program counter.
//!
//! # Handlers
//!
//! `ee.hooks`, `ee.probes` and `ee.syscalls` hold Rust closures keyed by address (or syscall
//! number); see [`HookTable`] and [`SyscallTable`].
//!
//! * A hook replaces the function at its address: when the program counter reaches the address the
//!   closure runs, then execution resumes at `ra` (unless the closure moved the program counter
//!   with [`Ee::jump`] or [`Ee::set_context`]).
//! * A probe runs before the instruction at its address, which then executes normally.
//! * Handlers cost nothing for addresses without one: the run loop tests one bit of a bitmap per
//!   instruction and only then looks in the tables.
//!
//! # Exceptions and interrupts
//!
//! Synchronous exceptions ([`Exception`]) are `syscall`, `break`, address errors (misaligned
//! halfword, word and doubleword accesses and instruction fetches), integer overflow, traps and
//! reserved instructions. Delivery order for a `syscall`: a closure in `ee.syscalls` for the
//! number in `v1` (signed, as the SDK passes it) runs with the program counter already past the
//! instruction. Otherwise, and for every other exception, the COP0 registers are updated as the
//! hardware does (EPC, Cause.ExcCode and BD, Status.EXL, BadVAddr) and
//! [`Ee::set_exception_handler`]'s closure runs; with no handler a synchronous exception ends the
//! run with [`Fault::Exception`]. The handler resumes by setting the program counter (typically
//! from `cop0[14]`, as `eret` would).
//!
//! Interrupts: [`Ee::set_interrupt_pending`] sets or clears a Cause.IP bit (2 and 3 are the INTC and
//! DMAC lines, 7 is the Count/Compare timer, which the core raises itself). A pending interrupt is
//! taken between instructions when Status has IE and EIE set, EXL and ERL clear, and the matching
//! IM bit set, exactly as `ei` and `mtc0` leave it. With an exception handler installed it
//! receives [`Exception::Interrupt`]; without one the core jumps to the vector (0x80000200 when
//! Cause.IV is set, else 0x80000180; 0xBFC00400 and 0xBFC00380 with Status.BEV) where a hook can
//! stand in for the kernel. The state a program starts in has IE and every IM bit set and EIE
//! clear, so nothing is taken until the program executes `ei`. `eret`,
//! `ei`, `di` and `mfc0`/`mtc0` work on the registers in `ee.cop0`. Count advances by one per
//! executed instruction ([inferred] model; the hardware counts bus cycles).
//!
//! An exception or interrupt in the delay slot of a branch or jump, taken or not, records EPC at
//! the branch and sets Cause.BD, as the hardware does; the handler's `eret` re-runs the branch. A
//! branch-likely that is not taken nullifies its slot, so nothing can happen there. A fetch address
//! error after a bad `jr` target is raised on the fetch itself: EPC and BadVAddr are the bad
//! address and BD is clear.
//!
//! # Instruction set
//!
//! The whole of the R5900 core, MMI (except `PHMADH`, `PHMSBH` and `PDIVBW`, which the game does
//! not use and which stay [`Fault::Unimplemented`]), COP1 (single precision), COP0 and the COP2
//! macro instructions of VU0 execute, including the MAC, status and clip flags, the R register
//! and VU0 data memory access. `vcallms`/`vcallmsr` (microprograms) do not. VU0 denormal results
//! are stored unflushed unless [`Ee::flush_denormals`] is set (the flags are set either way);
//! denormal operands are never flushed. A scan of every function of the executable and the four
//! overlays (`tests/coverage.rs`, needs the private function sheets) finds no word that fails to
//! decode.
//!
//! # Hardware registers
//!
//! The timers, IPU, GIF, VIF, DMA controller, INTC and the GS privileged registers answer reads
//! with their documented reset values and log writes: see [`hw`]. The TLB is not modelled
//! (addresses are masked as before); `tlbr`, `tlbwi`, `tlbwr` and `tlbp` update the COP0 registers
//! only.

pub mod hw;
mod cop0;
mod exec;
mod hooks;
mod mmi;
mod vu;

pub use hooks::{Handler, Hook, HookTable, SyscallTable};

pub const RAM: usize = 32 << 20;
const SPR_BASE: u32 = 0x7000_0000;
const SPR: usize = 16 << 10;
/// Return address used by [`Ee::call`]; reaching it ends the call.
pub const SENTINEL: u32 = 0xFFFF_FFF0;
/// Where the four code overlays are loaded (they share one window; see [`Ee::load_overlay`]).
pub const OVERLAY_BASE: u32 = 0x01F1_8000;

/// A synchronous exception or an interrupt, as passed to the exception handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exception {
    /// A pending, enabled interrupt (Cause.ExcCode 0).
    Interrupt,
    /// `syscall` with the number found in `v1` (ExcCode 8).
    Syscall(i32),
    /// `break` with its 20-bit code (ExcCode 9). gcc emits `break 7` behind a divide by zero.
    Break(u32),
    /// A misaligned access or fetch (ExcCode 4 for loads and fetches, 5 for stores).
    AddressError { addr: u32, store: bool },
    /// Signed overflow in `add`, `addi`, `sub`, `dadd`, `daddi`, `dsub` (ExcCode 12).
    Overflow,
    /// A conditional trap instruction (`teq` and the like) that fired (ExcCode 13).
    Trap,
    /// An opcode that is not defined (ExcCode 10).
    ReservedInstruction(u32),
}

impl Exception {
    pub(crate) fn code(self) -> u32 {
        match self {
            Exception::Interrupt => 0,
            Exception::AddressError { store: false, .. } => 4,
            Exception::AddressError { store: true, .. } => 5,
            Exception::Syscall(_) => 8,
            Exception::Break(_) => 9,
            Exception::ReservedInstruction(_) => 10,
            Exception::Overflow => 12,
            Exception::Trap => 13,
        }
    }
}

#[derive(Debug)]
pub enum Fault {
    Unimplemented { pc: u32, word: u32, what: &'static str },
    Memory { pc: u32, addr: u32 },
    Runaway { pc: u32 },
    /// A synchronous exception with no handler installed. `pc` is the faulting instruction.
    Exception { pc: u32, exc: Exception },
    /// A handler was reached again while it was still running.
    Reentrant { pc: u32 },
    /// Internal marker (never returned): the instruction completed but the run loop must look at
    /// timers and interrupts before the next one.
    #[doc(hidden)]
    Reschedule,
}

/// Why [`Ee::run`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// The program counter reached [`SENTINEL`].
    Returned,
    /// The instruction budget is used up.
    Steps,
    /// A handler called [`Ee::request_stop`].
    Requested,
}

/// The register file of one thread (everything a context switch saves).
#[derive(Clone)]
pub struct Context {
    pub gpr: [u128; 32],
    pub hi: u128,
    pub lo: u128,
    pub sa: u32,
    pub pc: u32,
    pub npc: u32,
    pub fpr: [f32; 32],
    pub facc: f32,
    pub fcr31: u32,
    pub vf: [[f32; 4]; 32],
    pub vi: [u32; 32],
    pub vacc: [f32; 4],
    pub q: f32,
}

type ExceptionHandler = Box<dyn FnMut(&mut Ee, Exception)>;

pub struct Ee {
    pub ram: Vec<u8>,
    pub spr: Vec<u8>,
    pub gpr: [u128; 32],
    pub hi: u128,
    pub lo: u128,
    pub sa: u32,
    pub pc: u32,
    npc: u32,
    pub fpr: [f32; 32],
    pub facc: f32,
    pub fcr31: u32,
    /// VU0 vector registers. `vf[0]` is the constant (0, 0, 0, 1).
    pub vf: [[f32; 4]; 32],
    /// VU0 integer and control registers: 1..15 integers, 16 status flag, 17 MAC flag, 18 clip
    /// flag, 20 R, 21 I, 22 Q (mirrors `q`), 28/31 CMSAR.
    pub vi: [u32; 32],
    pub vacc: [f32; 4],
    pub q: f32,
    /// Replace the function at an address with a closure (see the crate docs).
    pub hooks: HookTable,
    /// Debug: run before the instruction at an address executes (the call continues).
    pub probes: HookTable,
    /// Host kernel: closures keyed by syscall number.
    pub syscalls: SyscallTable,
    /// COP0 registers (Status 12, Cause 13, EPC 14, Compare 11, ...; Count is read through `mfc0`).
    pub cop0: [u32; 32],
    /// The memory-mapped hardware stub.
    pub hw: hw::Hw,
    /// The CPCOND0 line that `bc0t` and `bc0f` test: true while the DMA channels selected in
    /// D_PCR are idle, which is always the case here.
    pub cpcond0: bool,
    /// Store denormal VU0 results as zero, as the hardware does (the U and Z flags are set either way).
    pub flush_denormals: bool,
    pub steps: u64,
    /// The most instructions one [`Ee::call`] may execute before it fails with [`Fault::Runaway`].
    pub limit: u64,
    fault: Option<Fault>,
    npc_next: Option<u32>,
    skip_delay: bool,
    /// The executing instruction is a branch or jump (taken or not).
    branched: bool,
    /// Address of the branch whose delay slot is the instruction at `pc`, if it is one.
    slot_of: Option<u32>,
    /// Debug: log writes into [lo, hi).
    pub watch: Option<(u32, u32)>,
    special: hooks::Special,
    seen: (u64, u64),
    exception_handler: Option<ExceptionHandler>,
    /// A handler moved the program counter, so a hook must not return to `ra`.
    redirected: bool,
    stop: bool,
    /// Step count at which the run loop must look at timers and interrupts again.
    next_event: u64,
    depth: u32,
    count_base: u32,
    count_since: u64,
    timer_due: u64,
}

/// Register names (ABI).
pub mod reg {
    pub const V0: usize = 2;
    pub const V1: usize = 3;
    pub const A0: usize = 4;
    pub const T0: usize = 8;
    pub const SP: usize = 29;
    pub const RA: usize = 31;
}

fn clamp(f: f32) -> f32 {
    // The EE FPU and VUs have no infinities or NaNs: overflow saturates to the largest normal.
    if f.is_nan() {
        f32::MAX
    } else if f.is_infinite() {
        f32::MAX.copysign(f)
    } else {
        f
    }
}

fn sx32(v: u32) -> u64 {
    v as i32 as i64 as u64
}

impl Default for Ee {
    fn default() -> Self {
        Self::new()
    }
}

impl Ee {
    pub fn new() -> Ee {
        let mut e = Ee {
            ram: vec![0; RAM],
            spr: vec![0; SPR],
            gpr: [0; 32],
            hi: 0,
            lo: 0,
            sa: 0,
            pc: 0,
            npc: 4,
            fpr: [0.0; 32],
            facc: 0.0,
            fcr31: 0,
            vf: [[0.0; 4]; 32],
            vi: [0; 32],
            vacc: [0.0; 4],
            q: 0.0,
            hooks: HookTable::default(),
            probes: HookTable::default(),
            syscalls: SyscallTable::default(),
            cop0: cop0::reset_state(),
            hw: hw::Hw::new(),
            cpcond0: true,
            flush_denormals: false,
            steps: 0,
            limit: 50_000_000,
            fault: None,
            npc_next: None,
            skip_delay: false,
            branched: false,
            slot_of: None,
            watch: None,
            special: hooks::Special::new(),
            seen: (u64::MAX, u64::MAX),
            exception_handler: None,
            redirected: false,
            stop: false,
            next_event: 0,
            depth: 0,
            count_base: 0,
            count_since: 0,
            timer_due: u64::MAX,
        };
        e.vf[0] = [0.0, 0.0, 0.0, 1.0];
        e.vi[20] = 0x3F80_0000;
        e
    }

    /// Loads the PT_LOAD segments of an ELF32 executable. Returns the entry point.
    pub fn load_elf(&mut self, elf: &[u8]) -> Option<u32> {
        let u16_ = |o: usize| -> Option<usize> { Some(u16::from_le_bytes(elf.get(o..o + 2)?.try_into().ok()?) as usize) };
        let u32_ = |o: usize| -> Option<u32> { Some(u32::from_le_bytes(elf.get(o..o + 4)?.try_into().ok()?)) };
        if elf.get(..4)? != b"\x7fELF" {
            return None;
        }
        let entry = u32_(0x18)?;
        let phoff = u32_(0x1C)? as usize;
        let phentsize: usize = u16_(0x2A)?;
        let phnum: usize = u16_(0x2C)?;
        for i in 0..phnum {
            let p = phoff + i * phentsize;
            if u32_(p)? != 1 {
                continue;
            }
            let (off, vaddr, filesz, memsz) = (u32_(p + 4)? as usize, u32_(p + 8)?, u32_(p + 16)? as usize, u32_(p + 20)? as usize);
            let dst = (vaddr & 0x01FF_FFFF) as usize;
            self.ram.get_mut(dst..dst + memsz)?.fill(0);
            self.ram[dst..dst + filesz].copy_from_slice(elf.get(off..off + filesz)?);
        }
        Some(entry)
    }

    /// Copies a code overlay (a decompressed `OVL.IMG` entry) to [`OVERLAY_BASE`]. The four
    /// overlays share the window, so loading one replaces the previous; bytes of a longer
    /// previous overlay beyond the new one's end are kept, as the game's loader keeps them.
    /// Returns the end address, or `None` if it does not fit in RAM.
    pub fn load_overlay(&mut self, code: &[u8]) -> Option<u32> {
        let dst = OVERLAY_BASE as usize;
        self.ram.get_mut(dst..dst + code.len())?.copy_from_slice(code);
        Some(OVERLAY_BASE + code.len() as u32)
    }

    // ---- memory -------------------------------------------------------------------------

    fn mem_fault(&mut self, addr: u32) {
        if self.fault.is_none() {
            self.fault = Some(Fault::Memory { pc: self.pc, addr });
        }
    }

    #[inline(always)]
    pub fn read<const N: usize>(&mut self, addr: u32) -> [u8; N] {
        let a = (addr & 0x1FFF_FFFF) as usize;
        if a + N <= RAM {
            return self.ram[a..a + N].try_into().unwrap();
        }
        self.read_slow::<N>(addr)
    }

    #[cold]
    fn read_slow<const N: usize>(&mut self, addr: u32) -> [u8; N] {
        if (SPR_BASE..SPR_BASE + SPR as u32).contains(&addr) {
            let o = (addr - SPR_BASE) as usize;
            if let Some(s) = self.spr.get(o..o + N) {
                return s.try_into().unwrap();
            }
        }
        let a = addr & 0x1FFF_FFFF;
        if hw::Hw::maps(a) {
            let mut out = [0u8; N];
            self.hw.read(a, &mut out, self.steps);
            return out;
        }
        self.mem_fault(addr);
        [0; N]
    }

    #[inline(always)]
    pub fn write(&mut self, addr: u32, bytes: &[u8]) {
        let a = (addr & 0x1FFF_FFFF) as usize;
        if a + bytes.len() <= RAM && self.watch.is_none() {
            self.ram[a..a + bytes.len()].copy_from_slice(bytes);
            return;
        }
        self.write_slow(addr, bytes);
    }

    #[cold]
    fn write_slow(&mut self, addr: u32, bytes: &[u8]) {
        if let Some((lo, hi)) = self.watch
            && addr < hi
            && addr + bytes.len() as u32 > lo
        {
            eprintln!("watch: pc {:08x} writes {:08x} <- {:02x?}", self.pc, addr, bytes);
        }
        let a = (addr & 0x1FFF_FFFF) as usize;
        if a + bytes.len() <= RAM {
            self.ram[a..a + bytes.len()].copy_from_slice(bytes);
            return;
        }
        if (SPR_BASE..SPR_BASE + SPR as u32).contains(&addr) {
            let o = (addr - SPR_BASE) as usize;
            if let Some(s) = self.spr.get_mut(o..o + bytes.len()) {
                s.copy_from_slice(bytes);
                return;
            }
        }
        let a = addr & 0x1FFF_FFFF;
        if hw::Hw::maps(a) {
            if let Some(k) = self.hw.write(self.pc, a, bytes, self.steps) {
                self.scratchpad_dma(k);
            }
            return;
        }
        self.mem_fault(addr);
    }

    /// Scratchpad DMA in normal mode: channel 8 copies scratchpad to memory, channel 9 memory to
    /// scratchpad, `qwc` quadwords between `sadr` and `madr`.
    fn scratchpad_dma(&mut self, k: hw::DmaKick) {
        let (n, spr_at, mem_at) = ((k.qwc as usize) * 16, (k.sadr & 0x3FFF) as usize, (k.madr & 0x1FFF_FFFF) as usize);
        if spr_at + n > SPR || mem_at + n > RAM {
            return;
        }
        if k.channel == 8 {
            self.ram[mem_at..mem_at + n].copy_from_slice(&self.spr[spr_at..spr_at + n]);
        } else {
            self.spr[spr_at..spr_at + n].copy_from_slice(&self.ram[mem_at..mem_at + n]);
        }
    }

    pub fn r8(&mut self, a: u32) -> u8 {
        self.read::<1>(a)[0]
    }
    pub fn r16(&mut self, a: u32) -> u16 {
        u16::from_le_bytes(self.read(a))
    }
    pub fn r32(&mut self, a: u32) -> u32 {
        u32::from_le_bytes(self.read(a))
    }
    pub fn r64(&mut self, a: u32) -> u64 {
        u64::from_le_bytes(self.read(a))
    }
    pub fn r128(&mut self, a: u32) -> u128 {
        u128::from_le_bytes(self.read(a))
    }
    pub fn f32_at(&mut self, a: u32) -> f32 {
        f32::from_bits(self.r32(a))
    }
    pub fn w32(&mut self, a: u32, v: u32) {
        self.write(a, &v.to_le_bytes())
    }
    pub fn wf32(&mut self, a: u32, v: f32) {
        self.w32(a, v.to_bits())
    }

    // ---- registers ------------------------------------------------------------------------

    fn g(&self, r: usize) -> u64 {
        self.gpr[r] as u64
    }
    fn g32(&self, r: usize) -> u32 {
        self.gpr[r] as u32
    }
    /// Writes the low 64 bits; the upper 64 bits of the register are preserved.
    fn set(&mut self, r: usize, v: u64) {
        if r != 0 {
            self.gpr[r] = (self.gpr[r] & !(u64::MAX as u128)) | v as u128;
        }
    }
    fn set128(&mut self, r: usize, v: u128) {
        if r != 0 {
            self.gpr[r] = v;
        }
    }
    pub fn arg(&mut self, i: usize, v: u64) {
        // EE ABI: integer arguments in a0-a3 then t0-t3.
        let r = if i < 4 { reg::A0 + i } else { reg::T0 + i - 4 };
        self.set(r, v);
    }

    /// Sets the program counter (no delay slot pending). A hook or syscall handler that calls this
    /// decides where execution continues.
    pub fn jump(&mut self, pc: u32) {
        self.pc = pc;
        self.npc = pc.wrapping_add(4);
        self.slot_of = None;
        self.redirected = true;
    }

    /// The thread state: registers, program counters and the VU0 register file.
    pub fn context(&self) -> Context {
        Context {
            gpr: self.gpr,
            hi: self.hi,
            lo: self.lo,
            sa: self.sa,
            pc: self.pc,
            npc: self.npc,
            fpr: self.fpr,
            facc: self.facc,
            fcr31: self.fcr31,
            vf: self.vf,
            vi: self.vi,
            vacc: self.vacc,
            q: self.q,
        }
    }

    /// Loads a saved thread state; a hook or syscall handler that does this switches threads.
    pub fn set_context(&mut self, c: &Context) {
        self.gpr = c.gpr;
        self.hi = c.hi;
        self.lo = c.lo;
        self.sa = c.sa;
        self.pc = c.pc;
        self.npc = c.npc;
        self.fpr = c.fpr;
        self.facc = c.facc;
        self.fcr31 = c.fcr31;
        self.vf = c.vf;
        self.vi = c.vi;
        self.vacc = c.vacc;
        self.q = c.q;
        self.slot_of = None;
        self.redirected = true;
    }

    /// Makes [`Ee::run`] return [`Stop::Requested`] after the current instruction or handler.
    pub fn request_stop(&mut self) {
        self.stop = true;
        self.next_event = 0;
    }

    /// Installs the closure that receives exceptions the syscall table does not take.
    pub fn set_exception_handler<F: FnMut(&mut Ee, Exception) + 'static>(&mut self, f: F) {
        self.exception_handler = Some(Box::new(f));
    }

    pub fn clear_exception_handler(&mut self) {
        self.exception_handler = None;
    }

    // ---- calls --------------------------------------------------------------------------

    /// Calls `addr` with integer args (a0..), float args (f12..), and a stack at `sp`.
    /// Returns v0; f0 holds a float result.
    pub fn call(&mut self, addr: u32, args: &[u64], fargs: &[f32], sp: u32) -> Result<u64, Fault> {
        let saved = (self.depth > 0).then(|| (self.pc, self.npc, self.gpr[reg::RA], self.gpr[reg::SP], self.slot_of));
        // A handler that runs inside this call may move the program counter; that must not leak
        // into the hook that made the call.
        let redirected = self.redirected;
        for (i, &a) in args.iter().enumerate() {
            self.arg(i, a);
        }
        for (i, &f) in fargs.iter().enumerate() {
            self.fpr[12 + i] = f;
        }
        self.set(reg::SP, sp as u64);
        self.set(reg::RA, SENTINEL as u64);
        self.pc = addr;
        self.npc = addr.wrapping_add(4);
        self.npc_next = None;
        self.slot_of = None;
        let end = self.steps.saturating_add(self.limit).saturating_add(1);
        self.depth += 1;
        let r = self.run_loop(end, true);
        self.depth -= 1;
        self.redirected = redirected;
        if let Some((pc, npc, ra, sp, slot)) = saved {
            self.pc = pc;
            self.npc = npc;
            self.slot_of = slot;
            self.gpr[reg::RA] = ra;
            self.gpr[reg::SP] = sp;
        }
        r?;
        Ok(self.g(reg::V0))
    }

    /// Runs from the current program counter for at most `max_steps` instructions.
    pub fn run(&mut self, max_steps: u64) -> Result<Stop, Fault> {
        let end = self.steps.saturating_add(max_steps);
        let redirected = self.redirected;
        self.depth += 1;
        let r = self.run_loop(end, false);
        self.depth -= 1;
        self.redirected = redirected;
        r
    }

    fn sync_special(&mut self) {
        let seen = (self.hooks.generation, self.probes.generation);
        if seen != self.seen {
            self.special.rebuild([&self.hooks, &self.probes]);
            self.seen = seen;
        }
    }

    fn run_loop(&mut self, end: u64, runaway_is_error: bool) -> Result<Stop, Fault> {
        self.stop = false;
        self.sync_special();
        // Force a first look at timers and interrupts.
        self.next_event = 0;
        loop {
            if self.steps >= self.next_event {
                if self.stop {
                    self.stop = false;
                    return Ok(Stop::Requested);
                }
                if self.steps >= end {
                    return if runaway_is_error { Err(Fault::Runaway { pc: self.pc }) } else { Ok(Stop::Steps) };
                }
                self.service_events(end)?;
            }
            // Run instructions until the next timer or limit deadline, a handler, or a fault.
            let mut left = self.next_event.saturating_sub(self.steps).max(1);
            loop {
                let pc = self.pc;
                let mut asked = false;
                if self.special.has(pc) {
                    if pc == SENTINEL {
                        return Ok(Stop::Returned);
                    }
                    if self.run_special(pc)? {
                        self.next_event = 0;
                        break;
                    }
                    // A probe ran; it may have asked for a look at the events (it cannot undo that).
                    asked = self.next_event == 0;
                }
                if self.step_inner() {
                    self.handle_fault()?;
                    self.next_event = 0;
                    break;
                }
                left -= 1;
                if left == 0 || asked {
                    break;
                }
            }
        }
    }

    /// Runs the hook or probe at `pc`. Returns true if a hook ran (the instruction is not
    /// executed).
    fn run_special(&mut self, pc: u32) -> Result<bool, Fault> {
        self.sync_special();
        if let Some(h) = self.hooks.get(pc) {
            self.redirected = false;
            self.invoke(&h)?;
            self.sync_special();
            if !self.redirected {
                let ra = self.g32(reg::RA);
                self.pc = ra;
                self.npc = ra.wrapping_add(4);
                self.slot_of = None;
            }
            return Ok(true);
        }
        if let Some(p) = self.probes.get(pc) {
            self.invoke(&p)?;
            self.sync_special();
        }
        Ok(false)
    }

    fn invoke(&mut self, h: &Handler) -> Result<(), Fault> {
        match h.try_borrow_mut() {
            Ok(mut f) => {
                f(self);
                Ok(())
            }
            Err(_) => Err(Fault::Reentrant { pc: self.pc }),
        }
    }

    fn handle_fault(&mut self) -> Result<(), Fault> {
        match self.fault.take() {
            Some(Fault::Exception { exc, .. }) => self.dispatch_exception(exc),
            Some(Fault::Reschedule) | None => Ok(()),
            Some(f) => Err(f),
        }
    }

    /// Ends the current run-loop chunk after the running instruction, so that timers and
    /// interrupts are looked at before the next one.
    pub(crate) fn resched(&mut self) {
        self.next_event = 0;
        if self.fault.is_none() {
            self.fault = Some(Fault::Reschedule);
        }
    }

    /// Executes one instruction word as if it sat at `pc` (no fetch), for decoder tests and
    /// scans. Returns the fault it raised, if any; a taken branch is left in `npc`.
    pub fn execute(&mut self, word: u32) -> Result<(), Fault> {
        self.npc_next = None;
        self.skip_delay = false;
        self.branched = false;
        self.exec(word);
        match self.fault.take() {
            Some(Fault::Reschedule) | None => Ok(()),
            Some(f) => Err(f),
        }
    }

    fn unimpl(&mut self, word: u32, what: &'static str) {
        if self.fault.is_none() {
            self.fault = Some(Fault::Unimplemented { pc: self.pc, word, what });
        }
    }

    /// Raises a synchronous exception at the current instruction.
    pub(crate) fn raise(&mut self, exc: Exception) {
        if self.fault.is_none() {
            self.fault = Some(Fault::Exception { pc: self.pc, exc });
        }
    }

    /// An unconditional jump (`j`, `jal`, `jr`, `jalr`): always taken, with a delay slot.
    pub(crate) fn jump_to(&mut self, target: u32) {
        self.npc_next = Some(target);
        self.branched = true;
    }

    fn branch(&mut self, cond: bool, target: u32, likely: bool) {
        self.branched = true;
        if cond {
            self.npc_next = Some(target);
        } else if likely {
            // Branch-likely not taken: the delay slot is nullified.
            self.skip_delay = true;
        }
    }

    #[inline(always)]
    fn fetch(&mut self, pc: u32) -> u32 {
        let a = (pc & 0x1FFF_FFFF) as usize;
        if pc & 3 == 0 && a + 4 <= RAM {
            return u32::from_le_bytes(self.ram[a..a + 4].try_into().unwrap());
        }
        if pc & 3 != 0 {
            self.cop0[8] = pc;
            self.raise(Exception::AddressError { addr: pc, store: false });
            return 0;
        }
        self.r32(pc)
    }

    /// Executes the instruction at `pc`.
    pub fn step(&mut self) {
        self.step_inner();
    }

    /// Executes one instruction; returns true if a fault or exception is pending (the program
    /// counter then stays on the faulting instruction, except for [`Fault::Reschedule`]).
    #[inline(always)]
    fn step_inner(&mut self) -> bool {
        self.steps += 1;
        let (pc, old_npc, old_slot) = (self.pc, self.npc, self.slot_of);
        // A failed fetch leaves a fault and the word 0 (a nop); the check after `exec` catches it.
        let w = self.fetch(pc);
        self.npc_next = None;
        self.skip_delay = false;
        self.branched = false;
        self.exec(w);
        let npc = self.npc;
        // A branch-likely that falls through nullifies its slot, so no exception can occur there.
        self.slot_of = (self.branched && !self.skip_delay).then_some(pc);
        if self.skip_delay {
            self.pc = npc.wrapping_add(4);
            self.npc = self.pc.wrapping_add(4);
        } else {
            let next = self.npc_next.unwrap_or(npc.wrapping_add(4));
            self.pc = npc;
            self.npc = next;
        }
        if self.fault.is_some() {
            return self.after_fault(pc, old_npc, old_slot);
        }
        false
    }

    #[cold]
    fn after_fault(&mut self, pc: u32, npc: u32, slot: Option<u32>) -> bool {
        // eret and other completed instructions keep the new program counter; a real fault leaves
        // the program counter on the instruction.
        if !matches!(self.fault, Some(Fault::Reschedule)) {
            self.pc = pc;
            self.npc = npc;
            self.slot_of = slot;
        }
        true
    }
}
