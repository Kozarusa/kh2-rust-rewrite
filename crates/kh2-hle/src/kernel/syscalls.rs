//! The syscall table. Numbers, wrappers and the values the game expects back are from the boot
//! contract section 2.1 (runs B and F of the PCSX2 trace); semantics of the rest follow the SDK
//! kernel documentation [SDK]. A negative number is the interrupt-context variant of the same
//! call (`iSignalSema` is -0x43, `iFlushCache` -0x68): both signs are registered for each number.
//!
//! The calling convention is the EE ABI: arguments in a0..a3 then t0..t3, result in v0.
//! Numbers listed as "reached" below were seen in a run of the game; the others are the small
//! remaining set of thread and semaphore calls an SDK program may make, with SDK semantics.

use kh2_ee::{Ee, reg};

use super::intr::{D_STAT, INTC_MASK, IntHandler, Intr, refresh_lines};
use super::{Kernel, Post, Run, SYSCALL_TABLE, Shared, Wait, err};

type Handler = fn(&mut Kernel, &mut Ee) -> (i32, Post);

/// Argument `i` of the current call (a0..a3, t0..t3).
fn arg(ee: &Ee, i: usize) -> u32 {
    let r = if i < 4 { reg::A0 + i } else { reg::T0 + i - 4 };
    ee.gpr[r] as u32
}

fn ok(v: i32) -> (i32, Post) {
    (v, Post::Nothing)
}

/// Stores the result in v0 and applies the scheduling consequence.
pub(super) fn complete(k: &mut Kernel, ee: &mut Ee, r: (i32, Post)) {
    ee.gpr[reg::V0] = (ee.gpr[reg::V0] & !(u64::MAX as u128)) | (r.0 as i64 as u64) as u128;
    match r.1 {
        Post::Nothing => {}
        Post::Resched => k.reschedule(ee),
        Post::Block(w) => k.block_current(ee, w),
        Post::Exit { delete } => k.exit_current(ee, delete),
        Post::Suspend => k.suspend_current(ee),
    }
}

fn register(ee: &mut Ee, kernel: &Shared, abs: u32, f: Handler) {
    for n in [abs as i32, -(abs as i32)] {
        let s = kernel.clone();
        ee.syscalls.insert(n, move |ee: &mut Ee| {
            let mut k = s.borrow_mut();
            *k.counts.entry(abs).or_default() += 1;
            let r = f(&mut k, ee);
            complete(&mut k, ee, r);
        });
    }
}

pub(super) fn install(ee: &mut Ee, kernel: &Shared) {
    let table: &[(u32, Handler)] = &[
        (0x02, |_, _| ok(0)),  // SetGsCrt: nothing to display (args 1, 2, 0 select NTSC interlaced field mode)
        (0x10, add_intc),      // AddIntcHandler / AddIntcHandler2
        (0x11, remove_intc),   // RemoveIntcHandler
        (0x12, add_dmac),      // AddDmacHandler
        (0x13, remove_dmac),   // RemoveDmacHandler
        (0x14, enable_intc),   // _EnableIntc
        (0x15, disable_intc),  // _DisableIntc
        (0x16, enable_dmac),   // _EnableDmac
        (0x17, disable_dmac),  // _DisableDmac
        (0x20, create_thread), // CreateThread
        (0x21, delete_thread), // DeleteThread
        (0x22, start_thread),  // StartThread
        (0x23, |_, _| (0, Post::Exit { delete: false })), // ExitThread
        (0x24, |_, _| (0, Post::Exit { delete: true })),  // ExitDeleteThread
        (0x25, terminate_thread),
        (0x29, change_priority),
        (0x2d, release_wait),
        (0x2f, |k, _| ok(k.cur.unwrap_or(0) as i32)), // GetThreadId
        (0x30, refer_thread),
        (0x32, sleep_thread),
        (0x33, wakeup_thread),
        (0x35, cancel_wakeup),
        (0x37, suspend_thread),
        (0x39, resume_thread),
        (0x3c, setup_thread), // RFU060 SetupThread
        (0x3d, setup_heap),   // RFU061 SetupHeap
        (0x3e, |k, _| ok(k.heap_end as i32)), // EndOfHeap
        (0x40, create_sema),
        (0x41, delete_sema),
        (0x42, |k, ee| k.signal_sema(arg(ee, 0))),
        (0x43, |k, ee| k.signal_sema(arg(ee, 0))), // iSignalSema
        (0x44, |k, ee| k.wait_sema(arg(ee, 0))),
        (0x45, poll_sema),
        (0x46, poll_sema), // iPollSema
        (0x47, refer_sema),
        (0x48, refer_sema), // iReferSemaStatus
        (0x4a, set_osd),
        (0x4b, get_osd),
        // Slots libkernel installs for its own kernel-mode routines (D4): the routines are not run,
        // their effect is done here (see `kernel_copy` and `kernel_find`).
        (0x5a, kernel_copy),
        (0x83, kernel_find),
        (0x5b, get_entry), // GetEntryAddress
        (0x64, |_, _| ok(0)), // FlushCache
        (0x68, |_, _| ok(0)), // iFlushCache
        (0x6b, |_, _| ok(0)), // sceSifStopDma
        (0x71, gs_put_imr),
        (0x74, set_syscall),
        (0x76, |_, _| ok(-1)), // sceSifDmaStat: transfers are complete
        (0x77, sif_set_dma),
        (0x78, |_, _| ok(0)), // sceSifSetDChain
        (0x79, sif_set_reg),
        (0x7a, sif_get_reg),
        (0x7c, |_, _| ok(0)), // Deci2Call: swallowed
        (0x7f, |_, _| ok(0x0200_0000)), // GetMemorySize: 32 MB
    ];
    for (n, f) in table {
        register(ee, kernel, *n, *f);
    }
}

// ---- interrupt handlers ----------------------------------------------------------------------

fn add_handler(k: &mut Kernel, ee: &mut Ee, dmac: bool) -> (i32, Post) {
    let (line, addr, next, a) = (arg(ee, 0), arg(ee, 1), arg(ee, 2) as i32, arg(ee, 3));
    if line >= 16 {
        return ok(-1);
    }
    let id = k.intr.new_id();
    let h = IntHandler { id, addr, arg: a, gp: ee.gpr[28] as u32 };
    let list = if dmac { &mut k.intr.dmac[line as usize] } else { &mut k.intr.intc[line as usize] };
    Intr::insert(list, h, next);
    ok(id as i32)
}

fn add_intc(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    add_handler(k, ee, false)
}

fn add_dmac(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    add_handler(k, ee, true)
}

fn remove_handler(k: &mut Kernel, ee: &mut Ee, dmac: bool) -> (i32, Post) {
    let (line, id) = (arg(ee, 0), arg(ee, 1));
    if line >= 16 {
        return ok(-1);
    }
    let list = if dmac { &mut k.intr.dmac[line as usize] } else { &mut k.intr.intc[line as usize] };
    list.retain(|h| h.id != id);
    ok(0)
}

fn remove_intc(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    remove_handler(k, ee, false)
}

fn remove_dmac(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    remove_handler(k, ee, true)
}

/// Sets or clears a mask bit; returns 1 if the state changed.
fn set_mask(ee: &mut Ee, reg_addr: u32, bit: u32, on: bool) -> (i32, Post) {
    let m = ee.hw.peek(reg_addr);
    let new = if on { m | bit } else { m & !bit };
    ee.hw.poke(reg_addr, new);
    refresh_lines(ee);
    ok((new != m) as i32)
}

fn enable_intc(_: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let c = arg(ee, 0) & 31;
    set_mask(ee, INTC_MASK, 1 << c, true)
}

fn disable_intc(_: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let c = arg(ee, 0) & 31;
    set_mask(ee, INTC_MASK, 1 << c, false)
}

fn enable_dmac(_: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let c = arg(ee, 0) & 15;
    set_mask(ee, D_STAT, 1 << (16 + c), true)
}

fn disable_dmac(_: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let c = arg(ee, 0) & 15;
    set_mask(ee, D_STAT, 1 << (16 + c), false)
}

// ---- threads ----------------------------------------------------------------------------------

fn create_thread(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let p = arg(ee, 0);
    let (entry, stack, size, gp) = (ee.r32(p + 4), ee.r32(p + 8), ee.r32(p + 0xC), ee.r32(p + 0x10));
    let (prio, attr, option) = (ee.r32(p + 0x14), ee.r32(p + 0x1C), ee.r32(p + 0x20));
    ok(k.create_thread(entry, stack, size, gp, prio, attr, option) as i32)
}

fn delete_thread(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let tid = arg(ee, 0);
    match k.thread(tid).map(|t| t.run) {
        None => ok(err::UNKNOWN_THID),
        Some(Run::Dormant) => {
            k.threads[tid as usize] = None;
            ok(tid as i32)
        }
        Some(_) => ok(err::NOT_DORMANT),
    }
}

fn start_thread(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let (tid, a) = (arg(ee, 0), arg(ee, 1));
    match k.start_thread(tid, a) {
        0 => (0, Post::Resched),
        e => ok(e),
    }
}

fn terminate_thread(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let tid = arg(ee, 0);
    if k.thread(tid).is_none() {
        return ok(err::UNKNOWN_THID);
    }
    if k.cur == Some(tid) {
        return ok(err::NOT_DORMANT);
    }
    k.terminate(tid);
    ok(tid as i32)
}

fn target(k: &Kernel, ee: &Ee, i: usize) -> u32 {
    match arg(ee, i) {
        0 => k.cur.unwrap_or(0),
        t => t,
    }
}

fn change_priority(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let (tid, prio) = (target(k, ee, 0), arg(ee, 1));
    match k.thread_mut(tid) {
        None => ok(err::UNKNOWN_THID),
        Some(t) => {
            let old = t.prio;
            t.prio = prio;
            (old as i32, Post::Resched)
        }
    }
}

fn release_wait(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let tid = arg(ee, 0);
    match k.thread(tid) {
        Some(t) if t.run == Run::Waiting => {
            k.terminate_wait(tid);
            k.wake(tid, err::RELEASE_WAIT);
            (tid as i32, Post::Resched)
        }
        Some(_) => ok(-415),
        None => ok(err::UNKNOWN_THID),
    }
}

fn refer_thread(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let (tid, p) = (target(k, ee, 0), arg(ee, 1));
    let Some(t) = k.thread(tid) else { return ok(err::UNKNOWN_THID) };
    let words =
        [t.status(), t.entry, t.stack, t.stack_size, t.gp, t.init_prio, t.prio, t.attr, t.option, t.wait_type(), t.wait_id(), t.wakeups];
    for (i, w) in words.iter().enumerate() {
        ee.w32(p + 4 * i as u32, *w);
    }
    ok(0)
}

fn sleep_thread(k: &mut Kernel, _: &mut Ee) -> (i32, Post) {
    let Some(c) = k.cur.filter(|_| k.in_isr == 0) else { return ok(err::CAN_NOT_WAIT) };
    let t = k.thread_mut(c).expect("running thread");
    if t.wakeups > 0 {
        t.wakeups -= 1;
        return ok(0);
    }
    (0, Post::Block(Wait::Sleep))
}

fn wakeup_thread(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let tid = target(k, ee, 0);
    match k.thread(tid) {
        None => ok(err::UNKNOWN_THID),
        Some(t) if t.run == Run::Waiting && t.wait == Wait::Sleep => {
            k.wake(tid, 0);
            (tid as i32, Post::Resched)
        }
        Some(_) => {
            k.thread_mut(tid).expect("thread").wakeups += 1;
            ok(tid as i32)
        }
    }
}

fn cancel_wakeup(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let tid = target(k, ee, 0);
    match k.thread_mut(tid) {
        None => ok(err::UNKNOWN_THID),
        Some(t) => ok(std::mem::take(&mut t.wakeups) as i32),
    }
}

fn suspend_thread(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let tid = target(k, ee, 0);
    if k.thread(tid).is_none_or(|t| t.run == Run::Dormant) {
        return ok(err::UNKNOWN_THID);
    }
    if k.cur == Some(tid) {
        return (0, Post::Suspend);
    }
    k.thread_mut(tid).expect("thread").suspended = true;
    ok(0)
}

fn resume_thread(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let tid = arg(ee, 0);
    match k.thread_mut(tid) {
        Some(t) if t.suspended => {
            t.suspended = false;
            (0, Post::Resched)
        }
        Some(_) => ok(-412),
        None => ok(err::UNKNOWN_THID),
    }
}

/// RFU060 SetupThread(gp, stack, stack size, args, exit routine): crt0's first call. Makes the
/// calling code thread 1 (the main thread) and returns its initial stack pointer, 0x2a0 below the
/// top of the stack [inferred from the first frames of the traced run, which sat at 0x1fffd20 for
/// a stack ending at 0x2000000]. The argument block gets argc 1 and a path as argv[0] [inferred].
fn setup_thread(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let (gp, stack, size, args) = (arg(ee, 0), arg(ee, 1), arg(ee, 2), arg(ee, 3));
    let (stack, top) = if stack == u32::MAX { (0x0200_0000 - size, 0x0200_0000) } else { (stack, stack.wrapping_add(size)) };
    let sp = top - 0x2A0;
    if args != 0 {
        let path = b"cdrom0:\\SLUS_210.05;1\0";
        let at = top - 0x100;
        ee.write(at, path);
        ee.w32(args, 1);
        ee.w32(args + 4, at);
        ee.w32(args + 8, 0);
    }
    if k.thread(1).is_none() {
        let id = k.create_thread(0, stack, size, gp, 0, 0, 0);
        debug_assert_eq!(id, 1, "the main thread is thread 1");
    }
    let t = k.thread_mut(1).expect("main thread");
    t.stack = stack;
    t.stack_size = size;
    t.gp = gp;
    t.run = Run::Running;
    k.cur = Some(1);
    k.booting = false;
    ok(sp as i32)
}

/// RFU061 SetupHeap(start, size): returns the start and remembers the end for EndOfHeap.
fn setup_heap(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let (start, size) = (arg(ee, 0), arg(ee, 1));
    k.heap_start = start;
    k.heap_end = start.wrapping_add(size);
    ok(start as i32)
}

// ---- semaphores -------------------------------------------------------------------------------

fn create_sema(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let p = arg(ee, 0);
    let (max, init, attr, option) = (ee.r32(p + 4) as i32, ee.r32(p + 8) as i32, ee.r32(p + 0x10), ee.r32(p + 0x14));
    ok(k.create_sema(init, max, init, attr, option) as i32)
}

fn delete_sema(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let r = k.delete_sema(arg(ee, 0));
    (r, if r > 0 { Post::Resched } else { Post::Nothing })
}

fn poll_sema(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let id = arg(ee, 0);
    match k.semas.get_mut(id as usize).and_then(|s| s.as_mut()) {
        None => ok(err::UNKNOWN_SEMID),
        Some(s) if s.count > 0 => {
            s.count -= 1;
            ok(id as i32)
        }
        Some(_) => ok(err::SEMA_ZERO),
    }
}

fn refer_sema(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let (id, p) = (arg(ee, 0), arg(ee, 1));
    let Some(s) = k.sema(id) else { return ok(err::UNKNOWN_SEMID) };
    let words = [s.count as u32, s.max as u32, s.init as u32, s.waiters.len() as u32, s.attr, s.option];
    for (i, w) in words.iter().enumerate() {
        ee.w32(p + 4 * i as u32, *w);
    }
    ok(0)
}

// ---- configuration and SIF ----------------------------------------------------------------------

fn set_osd(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let p = arg(ee, 0);
    k.osd_config = ee.r32(p);
    ok(0)
}

fn get_osd(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let p = arg(ee, 0);
    ee.w32(p, k.osd_config);
    ok(0)
}

fn gs_put_imr(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let old = k.gs_imr;
    k.gs_imr = arg(ee, 0);
    ok(old as i32)
}

/// sceSifSetDma(descriptors, count): the transfer completes at once (D2); returns a non-zero id.
fn sif_set_dma(k: &mut Kernel, _: &mut Ee) -> (i32, Post) {
    k.next_dma_id += 1;
    ok(k.next_dma_id as i32)
}

/// sceSifSetReg(reg, value): returns the old value. Register 4 (SMFLAG) bits accumulate [inferred].
fn sif_set_reg(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let (r, v) = (arg(ee, 0), arg(ee, 1));
    let old = k.sif_regs.get(&r).copied().unwrap_or(0);
    k.sif_regs.insert(r, if r == 4 { old | v } else { v });
    ok(old as i32)
}

fn sif_get_reg(k: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    ok(k.sif_regs.get(&arg(ee, 0)).copied().unwrap_or(0) as i32)
}

// ---- the kernel's syscall table (D4) --------------------------------------------------------------

/// SetSyscall(number, handler): libkernel replaces kernel slots with routines of its own and later
/// finds the table by searching kernel RAM for those addresses (0x321cd8). The table is kept in low
/// RAM, which this machine reserves for the kernel, so that search finds it; the new handlers are
/// never run (D4). Returns the previous entry.
fn set_syscall(_: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let (n, h) = (arg(ee, 0) & 0x3FF, arg(ee, 1));
    let at = SYSCALL_TABLE + 4 * n;
    let old = ee.r32(at);
    ee.w32(at, h);
    ok(old as i32)
}

/// GetEntryAddress(number): the table entry.
fn get_entry(_: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let n = arg(ee, 0) & 0x3FF;
    ok(ee.r32(SYSCALL_TABLE + 4 * n) as i32)
}

/// Slot 0x5a, libkernel's kernel-mode copy routine 0x321c18: copies `a2` bytes from `a1` to `a0`
/// (either may be a kernel address 0x8xxxxxxx, which this machine maps onto low RAM). [V args from
/// the traced run; the routine's body is a plain copy, 0x321c08 is its caller]
fn kernel_copy(_: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let (dst, src, n) = (arg(ee, 0), arg(ee, 1), arg(ee, 2).min(0x1_0000));
    for i in 0..n {
        let b = ee.r8(src.wrapping_add(i));
        ee.write(dst.wrapping_add(i), &[b]);
    }
    ok(dst as i32)
}

/// Slot 0x83, libkernel's kernel-mode search routine 0x321c50: the address of the first word equal
/// to `a2` in [a0, a1), or 0. (The routine reads before it tests the bound; same here.)
fn kernel_find(_: &mut Kernel, ee: &mut Ee) -> (i32, Post) {
    let (mut p, end, v) = (arg(ee, 0), arg(ee, 1), arg(ee, 2));
    while p < end {
        if ee.r32(p) == v {
            return ok(p as i32);
        }
        p = p.wrapping_add(4);
    }
    ok(0)
}
