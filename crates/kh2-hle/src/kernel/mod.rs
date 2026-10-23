//! The EE kernel in Rust: syscall trap, threads, semaphores, interrupt handlers and the virtual
//! clock (boot contract sections 2.1 to 2.3, decisions D3 and D4).
//!
//! [`Machine`] owns the interpreter and the shared [`Kernel`] state. Its syscall closures run when
//! the guest executes `syscall` (the interpreter has already moved past it); a blocking call saves
//! the calling thread's registers, loads the next ready thread's registers with
//! `Ee::set_context`, or, when nothing is ready, asks the run loop to stop so that [`Machine::run`]
//! can advance the clock to the next event (idle). Timer events (VBlank, timer 2, DelayThread
//! wake-ups) fire between slices of `Ee::run` that end exactly at the next event time.
//! Interrupts raise Cause.IP2/IP3 and are taken by the interpreter at an instruction boundary
//! when the guest's Status allows it; the exception handler saves the full register file, calls
//! each registered handler to the call sentinel, restores the registers and then re-checks which
//! thread should run.

mod clock;
mod intr;
mod sema;
mod syscalls;
use syscalls::complete;
mod thread;
pub mod usa;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

pub use clock::{Clock, EE_HZ, Event, T2_CYCLES, VBLANK_CYCLES};
pub use intr::{CAUSE_TIMER2, CAUSE_VBLANK_START, DMAC_SIF0, IntHandler, Intr};
pub use sema::Sema;
pub use thread::{Run, THS_DORMANT, THS_READY, THS_RUN, THS_SUSPEND, THS_WAIT, Thread, Wait};

use kh2_ee::{Ee, Exception, Fault, Stop, reg};

/// Kernel error codes [SDK, Sony `KE_*`].
pub mod err {
    pub const UNKNOWN_THID: i32 = -406;
    pub const UNKNOWN_SEMID: i32 = -407;
    pub const NOT_DORMANT: i32 = -414;
    pub const DORMANT: i32 = -413;
    pub const CAN_NOT_WAIT: i32 = -416;
    pub const RELEASE_WAIT: i32 = -418;
    pub const SEMA_ZERO: i32 = -419;
    pub const SEMA_OVF: i32 = -420;
}

/// Where a thread that returns from its entry function lands: `li v1,0x23; syscall` (ExitThread).
/// Low RAM below the executable is the kernel's on the real machine [inferred].
pub const EXIT_STUB: u32 = 0x0008_0000;
/// Top of the stack interrupt handlers run on [inferred: the real kernel uses its own].
pub const INT_STACK_TOP: u32 = 0x0009_0000;

/// The kernel's syscall table, kept in low RAM so that libkernel's search of kernel RAM finds it
/// (see `set_syscall`).
pub const SYSCALL_TABLE: u32 = 0x0007_0000;

pub type Shared = Rc<RefCell<Kernel>>;

/// What a syscall does to scheduling after its result is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Post {
    Nothing,
    /// A thread may have become ready: switch if it outranks the caller.
    Resched,
    /// The caller waits.
    Block(Wait),
    /// The caller ends (`delete` frees its slot).
    Exit { delete: bool },
    /// The caller suspends itself.
    Suspend,
}

pub struct Kernel {
    pub threads: Vec<Option<Thread>>,
    pub semas: Vec<Option<Sema>>,
    pub intr: Intr,
    pub clock: Clock,
    /// The running thread, or `None` while every thread is blocked.
    pub cur: Option<u32>,
    /// The code that runs before `SetupThread` makes thread 1 (crt0) has no thread yet.
    pub booting: bool,
    /// Interrupt handlers being run (blocking is refused inside).
    pub in_isr: u32,
    pub heap_start: u32,
    pub heap_end: u32,
    pub sif_regs: HashMap<u32, u32>,
    pub gs_imr: u32,
    pub osd_config: u32,
    next_dma_id: u32,
    seq: i64,
    front_seq: i64,
    /// Syscall counts by absolute number (both signs together).
    pub counts: BTreeMap<u32, u64>,
    /// Syscalls reached that have no implementation, in order (number, pc after the trap).
    pub unimplemented: Vec<(i32, u32)>,
    /// Every `CreateThread` and `StartThread` made, as (event, thread id), for tests and logs.
    pub trace: Vec<(&'static str, u32)>,
    /// Set by the kernel when it asked the run loop to stop for its own reasons (nothing can run,
    /// or an event earlier than the planned slice end was scheduled).
    internal_stop: bool,
    /// A non-syscall exception the guest raised (the run ends with it).
    pub exception: Option<(u32, Exception)>,
    /// A fault inside an interrupt handler; [`Machine::run`] returns it.
    pub fault: Option<Fault>,
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}

impl Kernel {
    pub fn new() -> Kernel {
        let mut sif_regs = HashMap::new();
        // Defaults for an IOP that finished booting [inferred]: SMFLAG with bit 0x40000, the IOP
        // command buffer address seen in PCSX2 (0x1e640), RPC initialised.
        sif_regs.insert(4, 0x40000);
        sif_regs.insert(0x8000_0000, 0x1E640);
        sif_regs.insert(0x8000_0002, 1);
        Kernel {
            threads: vec![None],
            semas: vec![None],
            intr: Intr::default(),
            clock: Clock::new(),
            cur: None,
            booting: false,
            in_isr: 0,
            heap_start: 0,
            heap_end: 0,
            sif_regs,
            gs_imr: 0,
            // Language field (bits 16 to 20) English, every other field 0 [inferred].
            osd_config: 1 << 16,
            next_dma_id: 0,
            seq: 0,
            front_seq: 0,
            counts: BTreeMap::new(),
            unimplemented: Vec::new(),
            trace: Vec::new(),
            internal_stop: false,
            exception: None,
            fault: None,
        }
    }

    pub fn thread(&self, id: u32) -> Option<&Thread> {
        self.threads.get(id as usize)?.as_ref()
    }

    pub fn thread_mut(&mut self, id: u32) -> Option<&mut Thread> {
        self.threads.get_mut(id as usize)?.as_mut()
    }

    pub fn sema(&self, id: u32) -> Option<&Sema> {
        self.semas.get(id as usize)?.as_ref()
    }

    /// Number of existing threads.
    pub fn thread_count(&self) -> usize {
        self.threads.iter().flatten().count()
    }

    fn free_thread_id(&mut self) -> u32 {
        match self.threads.iter().skip(1).position(|t| t.is_none()) {
            Some(i) => i as u32 + 1,
            None => {
                self.threads.push(None);
                self.threads.len() as u32 - 1
            }
        }
    }

    /// Creates a dormant thread; returns its id (the lowest free one, so the main thread is 1 and
    /// the next ones 2, 3, 4 as in the PCSX2 runs of the boot contract).
    #[allow(clippy::too_many_arguments)]
    pub fn create_thread(&mut self, entry: u32, stack: u32, stack_size: u32, gp: u32, init_prio: u32, attr: u32, option: u32) -> u32 {
        let id = self.free_thread_id();
        self.threads[id as usize] = Some(Thread {
            id,
            run: Run::Dormant,
            suspended: false,
            wait: Wait::None,
            entry,
            stack,
            stack_size,
            gp,
            init_prio,
            prio: init_prio,
            attr,
            option,
            wakeups: 0,
            seq: 0,
            ctx: thread::blank_context(),
        });
        self.trace.push(("create", id));
        id
    }

    fn next_seq(&mut self, front: bool) -> i64 {
        if front {
            self.front_seq -= 1;
            self.front_seq
        } else {
            self.seq += 1;
            self.seq
        }
    }

    /// Makes a thread ready (at the tail of its priority queue, or the head for a preempted one).
    pub fn make_ready(&mut self, tid: u32, front: bool) {
        let seq = self.next_seq(front);
        if let Some(t) = self.thread_mut(tid) {
            t.run = Run::Ready;
            t.wait = Wait::None;
            t.seq = seq;
        }
    }

    /// The runnable thread with the best (lowest) priority, oldest in its queue first.
    pub fn best_ready(&self) -> Option<u32> {
        self.threads.iter().flatten().filter(|t| t.runnable()).min_by_key(|t| (t.prio, t.seq)).map(|t| t.id)
    }

    /// Whether a ready thread outranks the running one (equal priority does not preempt).
    pub fn needs_switch(&self) -> bool {
        let Some(best) = self.best_ready().and_then(|b| self.thread(b)) else { return false };
        match self.cur.and_then(|c| self.thread(c)) {
            Some(c) => best.prio < c.prio,
            None => true,
        }
    }

    /// Starts a dormant thread: registers loaded as the SDK documents (a0 = argument, gp, sp at
    /// the top of the stack, ra at the exit stub). Does not switch.
    pub fn start_thread(&mut self, tid: u32, arg: u32) -> i32 {
        let Some(t) = self.thread_mut(tid) else { return err::UNKNOWN_THID };
        if t.run != Run::Dormant {
            return err::NOT_DORMANT;
        }
        t.ctx = thread::blank_context();
        t.ctx.pc = t.entry;
        t.ctx.npc = t.entry.wrapping_add(4);
        t.ctx.gpr[reg::A0] = arg as u128;
        t.ctx.gpr[28] = t.gp as u128;
        t.ctx.gpr[reg::SP] = t.stack.wrapping_add(t.stack_size) as u128;
        t.ctx.gpr[reg::RA] = EXIT_STUB as u128;
        t.prio = t.init_prio;
        t.suspended = false;
        t.wakeups = 0;
        self.make_ready(tid, false);
        self.trace.push(("start", tid));
        0
    }

    /// Makes `tid` the running thread. The caller's registers are saved if a thread is running.
    pub fn switch_to(&mut self, ee: &mut Ee, tid: u32) {
        if self.cur == Some(tid) {
            return;
        }
        if let Some(c) = self.cur {
            let front = self.next_seq(true);
            let ctx = ee.context();
            if let Some(t) = self.thread_mut(c) {
                t.ctx = ctx;
                if t.run == Run::Running {
                    t.run = Run::Ready;
                    t.seq = front;
                }
            }
        }
        self.load(ee, tid);
    }

    fn load(&mut self, ee: &mut Ee, tid: u32) {
        let t = self.thread_mut(tid).expect("thread exists");
        t.run = Run::Running;
        t.wait = Wait::None;
        let ctx = t.ctx.clone();
        ee.set_context(&ctx);
        self.cur = Some(tid);
    }

    /// With no running thread: runs the best ready one, or stops the run loop for idle time.
    pub fn pick_next(&mut self, ee: &mut Ee) {
        self.cur = None;
        match self.best_ready() {
            Some(b) => self.load(ee, b),
            None => {
                self.internal_stop = true;
                ee.request_stop();
            }
        }
    }

    /// The caller (the running thread, whose registers are live in `ee`) waits.
    pub fn block_current(&mut self, ee: &mut Ee, wait: Wait) {
        let Some(c) = self.cur else { return };
        let ctx = ee.context();
        if let Some(t) = self.thread_mut(c) {
            t.ctx = ctx;
            t.run = Run::Waiting;
            t.wait = wait;
        }
        self.pick_next(ee);
    }

    /// Switches to a better ready thread, if there is one (never inside an interrupt handler).
    pub fn reschedule(&mut self, ee: &mut Ee) {
        if self.in_isr == 0 && !self.booting && self.needs_switch() {
            let b = self.best_ready().expect("needs_switch");
            self.switch_to(ee, b);
        }
    }

    /// The running thread ends: dormant, or deleted.
    pub fn exit_current(&mut self, ee: &mut Ee, delete: bool) {
        let Some(c) = self.cur else { return };
        if delete {
            self.threads[c as usize] = None;
        } else if let Some(t) = self.thread_mut(c) {
            t.run = Run::Dormant;
            t.wait = Wait::None;
        }
        self.pick_next(ee);
    }

    /// The running thread suspends itself (`SuspendThread` on itself).
    pub fn suspend_current(&mut self, ee: &mut Ee) {
        let Some(c) = self.cur else { return };
        let ctx = ee.context();
        if let Some(t) = self.thread_mut(c) {
            t.ctx = ctx;
            t.run = Run::Ready;
            t.suspended = true;
        }
        self.pick_next(ee);
    }

    /// Wakes a waiting thread with `v0` as the result of its blocking call.
    pub fn wake(&mut self, tid: u32, v0: i32) {
        if let Some(t) = self.thread_mut(tid) {
            thread::set_reg32(&mut t.ctx, reg::V0, v0);
        }
        self.make_ready(tid, false);
    }

    // ---- semaphores -----------------------------------------------------------------------

    pub fn create_sema(&mut self, count: i32, max: i32, init: i32, attr: u32, option: u32) -> u32 {
        let id = match self.semas.iter().skip(1).position(|s| s.is_none()) {
            Some(i) => i as u32 + 1,
            None => {
                self.semas.push(None);
                self.semas.len() as u32 - 1
            }
        };
        self.semas[id as usize] = Some(Sema { id, count, max, init, attr, option, waiters: Vec::new() });
        id
    }

    /// `SignalSema` / `iSignalSema`: wakes the first waiter or raises the count.
    pub fn signal_sema(&mut self, id: u32) -> (i32, Post) {
        let Some(s) = self.semas.get_mut(id as usize).and_then(|s| s.as_mut()) else { return (err::UNKNOWN_SEMID, Post::Nothing) };
        if s.waiters.is_empty() {
            if s.count >= s.max {
                return (err::SEMA_OVF, Post::Nothing);
            }
            s.count += 1;
            return (id as i32, Post::Nothing);
        }
        let pick = if s.attr & Sema::SA_THPRI != 0 {
            let mut best = 0;
            for (i, w) in s.waiters.iter().enumerate() {
                let p = |t: &u32| self.threads[*t as usize].as_ref().map_or(u32::MAX, |t| t.prio);
                if p(w) < p(&s.waiters[best]) {
                    best = i;
                }
            }
            best
        } else {
            0
        };
        let tid = self.semas[id as usize].as_mut().expect("sema").waiters.remove(pick);
        self.wake(tid, id as i32);
        (id as i32, Post::Resched)
    }

    /// `WaitSema`: takes a count or blocks the caller.
    pub fn wait_sema(&mut self, id: u32) -> (i32, Post) {
        let (in_isr, cur) = (self.in_isr, self.cur);
        let Some(s) = self.semas.get_mut(id as usize).and_then(|s| s.as_mut()) else { return (err::UNKNOWN_SEMID, Post::Nothing) };
        if s.count > 0 {
            s.count -= 1;
            return (id as i32, Post::Nothing);
        }
        let Some(c) = cur.filter(|_| in_isr == 0) else { return (err::CAN_NOT_WAIT, Post::Nothing) };
        s.waiters.push(c);
        (id as i32, Post::Block(Wait::Sema(id)))
    }

    /// Takes a waiting thread out of whatever it waits on (a semaphore queue, a delay).
    pub fn terminate_wait(&mut self, tid: u32) {
        for s in self.semas.iter_mut().flatten() {
            s.waiters.retain(|w| *w != tid);
        }
        self.clock.cancel_wake(tid);
    }

    pub fn delete_sema(&mut self, id: u32) -> i32 {
        let Some(s) = self.semas.get_mut(id as usize).and_then(|s| s.take()) else { return err::UNKNOWN_SEMID };
        for w in s.waiters {
            self.wake(w, err::RELEASE_WAIT);
        }
        id as i32
    }

    /// Ends a thread that is not the running one (`TerminateThread`).
    pub fn terminate(&mut self, tid: u32) {
        self.terminate_wait(tid);
        if let Some(t) = self.thread_mut(tid) {
            t.run = Run::Dormant;
            t.wait = Wait::None;
            t.suspended = false;
        }
    }

    // ---- interrupts -----------------------------------------------------------------------

    /// Sets an INTC status bit (a device raising its line).
    pub fn raise_intc(&mut self, ee: &mut Ee, cause: u32) {
        let v = ee.hw.peek(intr::INTC_STAT) | (1 << cause);
        ee.hw.poke(intr::INTC_STAT, v);
        intr::refresh_lines(ee);
    }

    /// Sets a DMAC channel status bit.
    pub fn raise_dmac(&mut self, ee: &mut Ee, channel: u32) {
        let v = ee.hw.peek(intr::D_STAT) | (1 << channel);
        ee.hw.poke(intr::D_STAT, v);
        intr::refresh_lines(ee);
    }

    /// Schedules a timer event from inside guest execution: the current slice of the run loop
    /// was planned without it, so the loop is asked to plan again.
    pub fn schedule_now(&mut self, ee: &mut Ee, at: u64, ev: Event) {
        self.clock.schedule(at, ev);
        self.internal_stop = true;
        ee.request_stop();
    }

    /// Handles one due timer event.
    fn fire(&mut self, ee: &mut Ee, ev: Event) {
        match ev {
            Event::VBlank => self.raise_intc(ee, CAUSE_VBLANK_START),
            Event::T2Overflow => {
                // The overflow flag (MODE bit 11) the alarm handler looks at [inferred].
                let m = ee.hw.peek(0x1000_1010);
                ee.hw.poke(0x1000_1010, m | 0x800);
                self.raise_intc(ee, CAUSE_TIMER2);
            }
            Event::Wake(tid) => {
                if self.thread(tid).is_some_and(|t| t.run == Run::Waiting && t.wait == Wait::Delay) {
                    self.wake(tid, 0);
                }
            }
        }
    }
}

/// Why [`Machine::run`] returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStop {
    /// The instruction budget is used.
    Budget,
    /// A probe or hook called `Ee::request_stop`.
    Requested,
    /// The program counter reached the call sentinel.
    Returned,
    /// The guest raised an exception that is not a syscall (address error, break, ...).
    Exception { pc: u32, exc: Exception },
    /// Every thread is blocked and no timer event is pending.
    Deadlock,
}

/// The interpreter with the kernel installed.
pub struct Machine {
    pub ee: Ee,
    pub kernel: Shared,
    /// Idle cycles one [`Machine::run`] call may skip before it returns [`RunStop::Budget`] (a
    /// program that sleeps forever would otherwise never return). Default 20 seconds of virtual time.
    pub idle_limit: u64,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    pub fn new() -> Machine {
        let mut ee = Ee::new();
        let kernel: Shared = Rc::new(RefCell::new(Kernel::new()));
        // ExitThread stub for threads that return from their entry function.
        ee.w32(EXIT_STUB, 0x2403_0023);
        ee.w32(EXIT_STUB + 4, 0x0000_000C);
        ee.w32(EXIT_STUB + 8, 0);
        syscalls::install(&mut ee, &kernel);
        let s = kernel.clone();
        ee.set_exception_handler(move |ee, exc| match exc {
            Exception::Interrupt => {
                let r = intr::dispatch(&s, ee, true);
                let mut k = s.borrow_mut();
                match r {
                    Ok(_) => k.reschedule(ee),
                    Err(f) => {
                        // A handler faulted: end the run; the driver reports it.
                        k.fault = Some(f);
                        ee.request_stop();
                    }
                }
            }
            Exception::Syscall(n) => {
                // Unimplemented syscall: result 0, resume after the trap.
                let epc = ee.cop0[14];
                ee.cop0[12] &= !2;
                s.borrow_mut().unimplemented.push((n, epc.wrapping_add(4)));
                ee.gpr[reg::V0] &= !(u64::MAX as u128);
                ee.jump(epc.wrapping_add(4));
            }
            other => {
                s.borrow_mut().exception = Some((ee.cop0[14], other));
                ee.cop0[12] &= !2;
                ee.request_stop();
            }
        });
        Machine { ee, kernel, idle_limit: 20 * EE_HZ }
    }

    /// Loads the executable and starts at its entry point; there is no thread yet (crt0 creates
    /// the main thread through `SetupThread`). Returns the entry address.
    pub fn load_elf(&mut self, elf: &[u8]) -> Option<u32> {
        let entry = self.ee.load_elf(elf)?;
        self.ee.jump(entry);
        self.kernel.borrow_mut().booting = true;
        Some(entry)
    }

    /// Makes `tid` the running thread without a syscall (for hosts and tests that start a thread
    /// from outside the guest).
    pub fn run_thread(&mut self, tid: u32) {
        let mut k = self.kernel.borrow_mut();
        k.switch_to(&mut self.ee, tid);
    }

    /// Runs guest code, the clock and the interrupts for at most `max_steps` retired instructions
    /// (idle time does not count against the budget).
    pub fn run(&mut self, max_steps: u64) -> Result<RunStop, Fault> {
        let start = self.ee.steps;
        let idle_start = self.kernel.borrow().clock.skipped;
        loop {
            let used = self.ee.steps - start;
            if used >= max_steps || self.kernel.borrow().clock.skipped - idle_start >= self.idle_limit {
                return Ok(RunStop::Budget);
            }
            // Timer events that are due.
            let mut fired = false;
            loop {
                let now = self.kernel.borrow().clock.now(self.ee.steps);
                let ev = self.kernel.borrow_mut().clock.pop_due(now);
                let Some((_, ev)) = ev else { break };
                self.kernel.borrow_mut().fire(&mut self.ee, ev);
                fired = true;
            }
            if let Some((pc, exc)) = self.kernel.borrow_mut().exception.take() {
                return Ok(RunStop::Exception { pc, exc });
            }
            let idle = {
                let k = self.kernel.borrow();
                k.cur.is_none() && !k.booting
            };
            if idle {
                // Nothing is running: interrupts are taken here, then a ready thread starts or
                // the clock jumps to the next event.
                intr::dispatch(&self.kernel, &mut self.ee, false)?;
                let mut k = self.kernel.borrow_mut();
                if let Some(b) = k.best_ready() {
                    k.switch_to(&mut self.ee, b);
                    continue;
                }
                let Some(at) = k.clock.next_at() else { return Ok(RunStop::Deadlock) };
                let now = k.clock.now(self.ee.steps);
                k.clock.skipped += at.saturating_sub(now);
                continue;
            }
            if fired {
                let mut k = self.kernel.borrow_mut();
                k.reschedule(&mut self.ee);
            }
            let (now, next) = {
                let k = self.kernel.borrow();
                (k.clock.now(self.ee.steps), k.clock.next_at())
            };
            let until = next.map_or(u64::MAX, |n| n.saturating_sub(now));
            let slice = (max_steps - used).min(until).max(1);
            let stop = self.ee.run(slice)?;
            match stop {
                Stop::Steps => {}
                Stop::Returned => return Ok(RunStop::Returned),
                Stop::Requested => {
                    let mut k = self.kernel.borrow_mut();
                    if k.internal_stop {
                        k.internal_stop = false;
                    } else if let Some(f) = k.fault.take() {
                        return Err(f);
                    } else if let Some((pc, exc)) = k.exception.take() {
                        return Ok(RunStop::Exception { pc, exc });
                    } else {
                        return Ok(RunStop::Requested);
                    }
                }
            }
        }
    }
}
