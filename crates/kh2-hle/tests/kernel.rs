//! Kernel behaviour on hand-assembled guest code (no disc needed): scheduler, semaphores,
//! interrupt delivery, the virtual clock and the syscall return values of the boot contract 2.1.

use std::cell::Cell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_hle::kernel::{CAUSE_VBLANK_START, VBLANK_CYCLES};
use kh2_hle::{Machine, RunStop};

// ---- a tiny assembler ----------------------------------------------------------------------------

const ZERO: u32 = 0;
const V0: u32 = 2;
const V1: u32 = 3;
const A0: u32 = 4;
const A1: u32 = 5;
const A2: u32 = 6;
const A3: u32 = 7;
const T0: u32 = 8;
const T1: u32 = 9;
const T2: u32 = 10;
const T3: u32 = 11;
const RA: u32 = 31;

fn addiu(rt: u32, rs: u32, imm: i32) -> u32 {
    0x2400_0000 | rs << 21 | rt << 16 | (imm as u32 & 0xFFFF)
}
fn lui(rt: u32, imm: u32) -> u32 {
    0x3C00_0000 | rt << 16 | (imm & 0xFFFF)
}
fn ori(rt: u32, rs: u32, imm: u32) -> u32 {
    0x3400_0000 | rs << 21 | rt << 16 | (imm & 0xFFFF)
}
fn lw(rt: u32, off: i32, rs: u32) -> u32 {
    0x8C00_0000 | rs << 21 | rt << 16 | (off as u32 & 0xFFFF)
}
fn sw(rt: u32, off: i32, rs: u32) -> u32 {
    0xAC00_0000 | rs << 21 | rt << 16 | (off as u32 & 0xFFFF)
}
fn jr(rs: u32) -> u32 {
    rs << 21 | 8
}
fn jal(target: u32) -> u32 {
    0x0C00_0000 | (target >> 2)
}
fn beq(rs: u32, rt: u32, off: i32) -> u32 {
    0x1000_0000 | rs << 21 | rt << 16 | (off as u32 & 0xFFFF)
}
const SYSCALL: u32 = 0x0000_000C;
const EI: u32 = 0x4200_0038;
const NOP: u32 = 0;

fn li(rt: u32, v: u32) -> Vec<u32> {
    vec![lui(rt, v >> 16), ori(rt, rt, v & 0xFFFF)]
}
fn sys(n: i32) -> Vec<u32> {
    vec![addiu(V1, ZERO, n), SYSCALL]
}
/// Calls syscall `n` with the given arguments (a0..a3).
fn call(n: i32, args: &[u32]) -> Vec<u32> {
    let mut v = Vec::new();
    for (i, a) in args.iter().enumerate() {
        v.extend(li(A0 + i as u32, *a));
    }
    v.extend(sys(n));
    v
}

/// Shared event counter: each `mark(slot)` adds one to the counter at `COUNTER` and stores the new
/// value in `SLOTS + 4 * slot`, so the slots show the order in which the code ran.
const COUNTER: u32 = 0x0020_0000;
const SLOTS: u32 = COUNTER + 0x100;
fn mark(slot: u32) -> Vec<u32> {
    let mut v = li(T2, COUNTER);
    v.extend([lw(T3, 0, T2), addiu(T3, T3, 1), sw(T3, 0, T2)]);
    v.extend(li(T2, SLOTS + 4 * slot));
    v.push(sw(T3, 0, T2));
    v
}

fn put(m: &mut Machine, at: u32, code: &[u32]) {
    for (i, w) in code.iter().enumerate() {
        m.ee.w32(at + 4 * i as u32, *w);
    }
}

fn slot(m: &mut Machine, i: u32) -> u32 {
    m.ee.r32(SLOTS + 4 * i)
}

const STOP: u32 = 0x0010_F000;

/// Code that jumps to the stop address, where a probe ends the run.
fn goto_stop() -> Vec<u32> {
    let mut v = li(T2, STOP);
    v.extend([jr(T2), NOP]);
    v
}

fn machine() -> Machine {
    let mut m = Machine::new();
    put(&mut m, STOP, &[NOP, NOP]);
    m.ee.probes.insert(STOP, |ee: &mut Ee| ee.request_stop());
    m
}

/// Creates a thread whose entry is `code` at `at`, with its own stack.
fn thread(m: &mut Machine, at: u32, code: &[u32], prio: u32) -> u32 {
    put(m, at, code);
    let mut k = m.kernel.borrow_mut();
    let n = k.thread_count() as u32;
    k.create_thread(at, 0x0030_0000 + 0x1000 * n, 0x1000, 0, prio, 0, 0)
}

fn start_main(m: &mut Machine, tid: u32) {
    {
        let mut k = m.kernel.borrow_mut();
        assert_eq!(k.start_thread(tid, 0), 0);
    }
    m.run_thread(tid);
}

// ---- scheduler -----------------------------------------------------------------------------------

#[test]
fn a_higher_priority_thread_preempts_and_a_lower_one_waits_for_the_caller_to_end() {
    let mut m = machine();
    let mut b = mark(3);
    b.extend([jr(RA), NOP]);
    let mut c = mark(5);
    c.extend(goto_stop());
    // Ids are the lowest free ones: main 1, B 2, C 3.
    let (main_id, b_id, c_id) = (1, 2, 3);
    let mut main = mark(1);
    main.extend(call(0x22, &[c_id, 0])); // start C (priority 20): does not preempt
    main.extend(mark(2));
    main.extend(call(0x22, &[b_id, 0])); // start B (priority 3): preempts at once
    main.extend(mark(4));
    main.extend([jr(RA), NOP]);
    assert_eq!(thread(&mut m, 0x0010_0000, &main, 10), main_id);
    assert_eq!(thread(&mut m, 0x0010_0400, &b, 3), b_id);
    assert_eq!(thread(&mut m, 0x0010_0800, &c, 20), c_id);
    start_main(&mut m, main_id);
    assert_eq!(m.run(10_000).unwrap(), RunStop::Requested);
    let order: Vec<u32> = (1..=5).map(|i| slot(&mut m, i)).collect();
    // main marks 1 and 2, B (higher priority) runs the moment it starts, main marks 4, and C
    // (lower priority) only runs once main has ended.
    assert_eq!(order, vec![1, 2, 3, 4, 5], "marks 1..5 ran in this order: main, main, B, main, C");
    let k = m.kernel.borrow();
    assert_eq!(k.thread(main_id).unwrap().status(), kh2_hle::kernel::THS_DORMANT);
}

#[test]
fn equal_priority_threads_run_in_start_order_and_a_preempted_thread_keeps_the_head() {
    let mut ee = Ee::new();
    let mut k = kh2_hle::Kernel::new();
    let a = k.create_thread(0x1000, 0x20_0000, 0x1000, 0, 5, 0, 0);
    let b = k.create_thread(0x2000, 0x21_0000, 0x1000, 0, 5, 0, 0);
    let c = k.create_thread(0x3000, 0x22_0000, 0x1000, 0, 7, 0, 0);
    let hi = k.create_thread(0x4000, 0x23_0000, 0x1000, 0, 1, 0, 0);
    for t in [b, a, c] {
        k.start_thread(t, 0);
    }
    assert_eq!(k.best_ready(), Some(b), "first started, first run at equal priority");
    k.switch_to(&mut ee, b);
    assert_eq!(ee.pc, 0x2000, "its registers were loaded");
    assert!(!k.needs_switch(), "an equal priority thread does not preempt");
    k.start_thread(hi, 0);
    assert!(k.needs_switch());
    k.switch_to(&mut ee, hi);
    assert_eq!(k.thread(b).unwrap().status(), kh2_hle::kernel::THS_READY);
    // B was preempted: it goes before A even though A has waited longer.
    k.block_current(&mut ee, kh2_hle::kernel::Wait::Sleep);
    assert_eq!(k.cur, Some(b), "the preempted thread resumes before its equal-priority peer");
}

// ---- semaphores ----------------------------------------------------------------------------------

#[test]
fn a_semaphore_blocks_the_waiter_and_reports_the_wait_count() {
    let mut m = machine();
    m.kernel.borrow_mut().create_sema(0, 10, 0, 0, 0);
    let buf = 0x0020_0400;
    let mut main = call(0x22, &[2, 0]); // W (priority 3) starts, waits, control returns here
    main.extend(call(0x47, &[1, buf])); // ReferSemaStatus
    main.extend(mark(1));
    main.extend(call(0x42, &[1])); // SignalSema: W is woken and preempts
    main.extend(mark(3));
    main.extend(goto_stop());
    let mut w = call(0x44, &[1]); // WaitSema
    w.extend(li(T0, COUNTER + 0x10));
    w.push(sw(V0, 0, T0));
    w.extend(mark(2));
    w.extend([jr(RA), NOP]);
    let main_id = thread(&mut m, 0x0010_0000, &main, 10);
    let w_id = thread(&mut m, 0x0010_0400, &w, 3);
    assert_eq!((main_id, w_id), (1, 2));
    start_main(&mut m, main_id);
    assert_eq!(m.run(10_000).unwrap(), RunStop::Requested);
    assert_eq!(m.ee.r32(buf + 0xC), 1, "ReferSemaStatus +0xc counts the blocked thread (0x14d1a8 tests it)");
    assert_eq!(m.ee.r32(buf), 0, "count 0, max 10 at +4");
    assert_eq!(m.ee.r32(buf + 4), 10);
    assert_eq!(m.ee.r32(COUNTER + 0x10), 1, "WaitSema returns the semaphore id once signalled");
    assert_eq!([slot(&mut m, 1), slot(&mut m, 2), slot(&mut m, 3)], [1, 2, 3], "main, W, main");
    let k = m.kernel.borrow();
    assert_eq!(k.sema(1).unwrap().count, 0, "the signal went to the waiter, not the count");
}

#[test]
fn poll_and_signal_move_the_count_and_overflow_is_refused() {
    let mut m = machine();
    // ensure thread 1 exists as the caller
    let id = thread(&mut m, 0x0010_0000, &[NOP], 5);
    m.kernel.borrow_mut().start_thread(id, 0);
    m.run_thread(id);
    let s = m.kernel.borrow_mut().create_sema(1, 2, 1, 0, 0) as i32;
    let sc = |m: &mut Machine, n: i32| m.ee.call(0x0030_0000, &[s as u64], &[], 0x0040_0000).map(|_| n);
    put(&mut m, 0x0030_0000, &[addiu(V1, ZERO, 0x45), SYSCALL, jr(RA), NOP]);
    sc(&mut m, 0).unwrap();
    assert_eq!(m.ee.gpr[V0 as usize] as i32, s, "PollSema takes the count and returns the id");
    sc(&mut m, 0).unwrap();
    assert_eq!(m.ee.gpr[V0 as usize] as i32, -419, "count 0: KE_SEMA_ZERO");
    put(&mut m, 0x0030_0000, &[addiu(V1, ZERO, 0x42), SYSCALL, jr(RA), NOP]);
    for _ in 0..2 {
        sc(&mut m, 0).unwrap();
        assert_eq!(m.ee.gpr[V0 as usize] as i32, s);
    }
    sc(&mut m, 0).unwrap();
    assert_eq!(m.ee.gpr[V0 as usize] as i32, -420, "count at max: KE_SEMA_OVF");
    assert_eq!(m.kernel.borrow().sema(s as u32).unwrap().count, 2);
}

// ---- interrupts and the clock ----------------------------------------------------------------------

/// A VBlank handler that does `iSignalSema(1)` (syscall -0x43).
fn signal_handler() -> Vec<u32> {
    let mut h = call(-0x43, &[1]);
    h.extend([jr(RA), NOP]);
    h
}

#[test]
fn idle_time_advances_to_the_vblank_and_the_handler_wakes_the_waiter() {
    let mut m = machine();
    m.kernel.borrow_mut().create_sema(0, 10, 0, 0, 0);
    put(&mut m, 0x0010_0800, &signal_handler());
    let mut main = call(0x10, &[CAUSE_VBLANK_START, 0x0010_0800, u32::MAX, 0]); // AddIntcHandler(2, handler, -1, 0)
    main.extend(call(0x14, &[CAUSE_VBLANK_START])); // _EnableIntc(2)
    main.push(EI);
    main.extend(call(0x44, &[1])); // WaitSema: blocks, every thread is blocked, time advances
    main.extend(li(T0, COUNTER + 0x20));
    main.push(sw(V0, 0, T0));
    main.extend(mark(1));
    main.extend(goto_stop());
    let id = thread(&mut m, 0x0010_0000, &main, 10);
    start_main(&mut m, id);
    assert_eq!(m.run(100_000).unwrap(), RunStop::Requested);
    let k = m.kernel.borrow();
    let now = k.clock.now(m.ee.steps);
    assert!((VBLANK_CYCLES..VBLANK_CYCLES + 200).contains(&now), "woken at the first VBlank, got {now}");
    assert!(m.ee.steps < 200, "the wait cost no instructions: {}", m.ee.steps);
    assert_eq!(k.clock.vsyncs, 1);
    assert_eq!(k.clock.skipped, VBLANK_CYCLES - m.ee.steps + (now - VBLANK_CYCLES), "idle advance is what was skipped");
    drop(k);
    assert_eq!(m.ee.r32(COUNTER + 0x20), 1, "WaitSema returned the id after the handler's iSignalSema");
    assert_eq!(slot(&mut m, 1), 1);
}

#[test]
fn an_interrupt_saves_every_register_and_keeps_the_interrupt_enable_state() {
    let mut m = machine();
    // The handler clobbers temporaries and counts itself at COUNTER + 0x40.
    let mut h = li(T2, COUNTER);
    h.extend([lw(T3, 0x40, T2), addiu(T3, T3, 1), sw(T3, 0x40, T2)]);
    h.extend(li(T0, 0x5555));
    h.extend(li(T1, 0x7777));
    h.extend([addiu(V0, ZERO, 5), EI, jr(RA), NOP]); // the real VBlank handler also executes `ei`
    put(&mut m, 0x0010_0800, &h);
    let mut main = call(0x10, &[CAUSE_VBLANK_START, 0x0010_0800, u32::MAX, 0]);
    main.extend(call(0x14, &[CAUSE_VBLANK_START]));
    main.push(EI);
    main.extend(li(T0, 0));
    main.extend(li(T1, 0));
    // loop: t0 += 1; t1 += 3; b loop
    main.extend([addiu(T0, T0, 1), addiu(T1, T1, 3), beq(ZERO, ZERO, -3), NOP]);
    let id = thread(&mut m, 0x0010_0000, &main, 10);
    start_main(&mut m, id);
    assert_eq!(m.run(16_000_000).unwrap(), RunStop::Budget);
    assert_eq!(m.ee.r32(COUNTER + 0x40), 3, "three VBlanks in 16 million instructions, each ran the handler once");
    let (t0, t1) = (m.ee.gpr[T0 as usize] as u32, m.ee.gpr[T1 as usize] as u32);
    assert!(t0 > 1_000_000, "the loop kept running: {t0}");
    assert!(t1 == 3 * t0 || t1 + 3 == 3 * t0, "the handler's writes to t0 and t1 were undone: {t0} {t1}");
    assert_eq!(m.ee.cop0[12] & 2, 0, "EXL is clear after the handler");
    assert_ne!(m.ee.cop0[12] & (1 << 16), 0, "EIE as the interrupted code had it");
    assert_eq!(m.kernel.borrow().clock.vsyncs, 3);
}

#[test]
fn masked_and_unregistered_lines_run_nothing_and_dmac_channel_5_reaches_its_handler() {
    let mut m = machine();
    // Handler: stores its first argument (the channel or cause) at COUNTER + 0x50 and counts.
    let mut h = li(T2, COUNTER);
    h.extend([sw(A0, 0x50, T2), lw(T3, 0x54, T2), addiu(T3, T3, 1), sw(T3, 0x54, T2), jr(RA), NOP]);
    put(&mut m, 0x0010_0800, &h);
    let mut main = call(0x12, &[5, 0x0010_0800, u32::MAX, 0]); // AddDmacHandler(5, ...)
    main.push(EI);
    main.extend(li(T0, 0));
    main.extend([addiu(T0, T0, 1), beq(ZERO, ZERO, -2), NOP]);
    let id = thread(&mut m, 0x0010_0000, &main, 10);
    start_main(&mut m, id);
    m.run(200).unwrap();
    {
        let mut k = m.kernel.borrow_mut();
        k.raise_dmac(&mut m.ee, 5); // channel not enabled yet
    }
    m.run(200).unwrap();
    assert_eq!(m.ee.r32(COUNTER + 0x54), 0, "the DMAC mask bit is clear: no delivery");
    // _EnableDmac(5) from a call, then the pending status is delivered at the next boundary.
    put(&mut m, 0x0030_0000, &[addiu(V1, ZERO, 0x16), SYSCALL, jr(RA), NOP]);
    m.ee.call(0x0030_0000, &[5], &[], 0x0040_0000).unwrap();
    assert_eq!(m.ee.gpr[V0 as usize] as u32, 1, "_EnableDmac returns 1 when the state changed");
    m.run(200).unwrap();
    assert_eq!(m.ee.r32(COUNTER + 0x54), 1);
    assert_eq!(m.ee.r32(COUNTER + 0x50), 5, "the handler gets the channel number in a0");
    m.run(200).unwrap();
    assert_eq!(m.ee.r32(COUNTER + 0x54), 1, "the status bit was acknowledged: no second call");
    // INTC 11 raised with no handler and no mask changes nothing.
    {
        let mut k = m.kernel.borrow_mut();
        k.raise_intc(&mut m.ee, 11);
    }
    m.run(200).unwrap();
    assert_eq!(m.ee.r32(COUNTER + 0x54), 1);
}

// ---- DelayThread ----------------------------------------------------------------------------------

#[test]
fn delay_thread_sleeps_on_the_virtual_clock_while_another_thread_runs() {
    let mut m = machine();
    m.install_usa_hooks();
    let woke = Rc::new(Cell::new(0u64));
    let (w, k) = (woke.clone(), m.kernel.clone());
    m.ee.probes.insert(STOP, move |ee: &mut Ee| {
        w.set(k.borrow().clock.now(ee.steps));
        ee.request_stop();
    });
    // Main: start B (priority 20), then DelayThread(1000 microseconds).
    let mut main = call(0x22, &[2, 0]);
    main.push(EI);
    main.extend(li(A0, 1000));
    main.extend([jal(0x319418), NOP]);
    main.extend(mark(1));
    main.extend(goto_stop());
    // B counts forever.
    let b = {
        let mut v = li(T0, COUNTER + 0x60);
        v.extend([lw(T1, 0, T0), addiu(T1, T1, 1), sw(T1, 0, T0), beq(ZERO, ZERO, -4), NOP]);
        v
    };
    let id = thread(&mut m, 0x0010_0000, &main, 10);
    let b_id = thread(&mut m, 0x0010_0400, &b, 20);
    assert_eq!((id, b_id), (1, 2));
    start_main(&mut m, id);
    let r = m.run(1_000_000).unwrap();
    assert_eq!(r, RunStop::Requested);
    let wake = woke.get();
    assert!((294_912..294_912 + 100).contains(&wake), "1000 us is 294,912 cycles; woke at {wake}");
    assert!(m.ee.r32(COUNTER + 0x60) > 10_000, "the low priority thread ran during the delay");
    assert_eq!(m.kernel.borrow().clock.skipped, 0, "never idle: B kept the processor busy");
}

#[test]
fn delay_thread_alone_is_idle_time_and_costs_no_instructions() {
    let mut m = machine();
    m.install_usa_hooks();
    let mut main = vec![EI];
    main.extend(li(A0, 2000));
    main.extend([jal(0x319418), NOP]);
    main.extend(goto_stop());
    let id = thread(&mut m, 0x0010_0000, &main, 10);
    start_main(&mut m, id);
    assert_eq!(m.run(1_000).unwrap(), RunStop::Requested);
    let k = m.kernel.borrow();
    assert!(k.clock.skipped >= 589_000 && m.ee.steps < 100, "skipped {} steps {}", k.clock.skipped, m.ee.steps);
    assert_eq!(m.ee.gpr[V0 as usize] as u32, 0, "DelayThread returns 0");
}

#[test]
fn delay_thread_with_interrupts_disabled_returns_the_error_code() {
    let mut m = machine();
    m.install_usa_hooks();
    let mut main = li(A0, 1000);
    main.extend([jal(0x319418), NOP]); // EIE is clear: the library returns 0x80008008 at once
    main.extend(li(T0, COUNTER + 0x70));
    main.push(sw(V0, 0, T0));
    main.extend(goto_stop());
    let id = thread(&mut m, 0x0010_0000, &main, 10);
    start_main(&mut m, id);
    assert_eq!(m.run(1_000).unwrap(), RunStop::Requested);
    assert_eq!(m.ee.r32(COUNTER + 0x70), 0x8000_8008);
}

// ---- syscall return values -------------------------------------------------------------------------

#[test]
fn syscalls_return_what_the_boot_contract_lists() {
    let mut m = machine();
    let sys_at = 0x0030_0000;
    let run = |m: &mut Machine, n: i32, args: &[u64]| -> i32 {
        put(m, sys_at, &[addiu(V1, ZERO, n), SYSCALL, jr(RA), NOP]);
        m.ee.call(sys_at, args, &[], 0x0040_0000).expect("syscall");
        m.ee.gpr[V0 as usize] as i32
    };
    // crt0: SetupThread(gp, stack, size, args, exit) returns the initial stack pointer.
    let sp = run(&mut m, 0x3c, &[0x388ebc, 0x1fe0000, 0x20000, 0x388f00, 0x1002a8]);
    assert_eq!(sp as u32, 0x1fffd60, "near the traced sp of 0x1fffd20 after one prologue");
    assert_eq!(m.ee.r32(0x388f00), 1, "argc");
    let argv0 = m.ee.r32(0x388f04);
    assert_eq!(m.ee.r8(argv0), b'c', "argv[0] points at a path string");
    assert_eq!(run(&mut m, 0x3d, &[0x1fc0000, 0x20000]) as u32, 0x1fc0000, "SetupHeap returns the heap start");
    assert_eq!(run(&mut m, 0x3e, &[]) as u32, 0x1fe0000, "EndOfHeap");
    assert_eq!(run(&mut m, 0x2f, &[]), 1, "GetThreadId: main");
    assert_eq!(run(&mut m, 0x7f, &[]), 0x0200_0000, "GetMemorySize");

    // Handlers and masks: ids are positive; Enable returns 1 only when the state changed.
    let h = run(&mut m, 0x10, &[2, 0x14cc88, u64::MAX, 0]);
    assert!(h > 0);
    assert_eq!(run(&mut m, 0x14, &[2]), 1);
    assert_eq!(run(&mut m, 0x14, &[2]), 0);
    assert_eq!(run(&mut m, 0x16, &[5]), 1);
    assert_eq!(run(&mut m, 0x17, &[5]), 1);
    assert_eq!(run(&mut m, 0x17, &[5]), 0);
    assert!(run(&mut m, 0x12, &[5, 0x31b6b8, u64::MAX, 0]) > 0);
    assert_eq!(run(&mut m, 0x13, &[5, 1]), 0, "RemoveDmacHandler");
    assert_eq!(m.kernel.borrow().intr.intc[2][0].addr, 0x14cc88);

    // Threads: a ThreadParam is read from memory; the status block returns its values.
    let (param, status) = (0x0020_1000u32, 0x0020_1100u32);
    for (i, w) in [0, 0x319500, 0x1fffcc0 - 0x400, 0x400, 0x388ebc, 0x1e, 0x1e, 0, 0].iter().enumerate() {
        m.ee.w32(param + 4 * i as u32, *w);
    }
    let t = run(&mut m, 0x20, &[param as u64]);
    assert_eq!(t, 2, "the first CreateThread after the main thread is thread 2");
    assert_eq!(run(&mut m, 0x30, &[t as u64, status as u64]), 0, "ReferThreadStatus returns 0");
    assert_eq!(m.ee.r32(status), kh2_hle::kernel::THS_DORMANT);
    assert_eq!(m.ee.r32(status + 4), 0x319500);
    assert_eq!(m.ee.r32(status + 0x14), 0x1e);
    assert_eq!(run(&mut m, 0x22, &[t as u64, 0]), 0, "StartThread returns 0");
    assert_eq!(run(&mut m, 0x29, &[0, 0x1e]), 0, "ChangeThreadPriority returns the old priority (main starts at 0)");
    // Thread 2 has priority 0x1e, the same as main now: it waits, ready.
    assert_eq!(m.kernel.borrow().thread(2).unwrap().status(), kh2_hle::kernel::THS_READY);
    assert_eq!(run(&mut m, 0x29, &[0, 1]), 0x1e);

    // Semaphores.
    for (i, w) in [0, 1, 0, 0, 0, 0].iter().enumerate() {
        m.ee.w32(param + 4 * i as u32, *w); // count 0, max 1, init 0
    }
    let s = run(&mut m, 0x40, &[param as u64]);
    assert_eq!(s, 1);
    assert_eq!(run(&mut m, 0x42, &[s as u64]), s, "SignalSema returns the id");
    assert_eq!(run(&mut m, 0x47, &[s as u64, status as u64]), 0, "ReferSemaStatus returns 0");
    assert_eq!(m.ee.r32(status), 1);
    assert_eq!(run(&mut m, 0x44, &[s as u64]), s, "WaitSema with a count returns the id");
    assert_eq!(run(&mut m, 0x41, &[s as u64]), s, "DeleteSema returns the id");

    // Configuration, SIF and no-ops.
    assert_eq!(run(&mut m, 0x4b, &[status as u64]), 0);
    assert_eq!((m.ee.r32(status) >> 16) & 0x1f, 1, "GetOsdConfigParam: language field English");
    assert_eq!(run(&mut m, 0x64, &[0]), 0, "FlushCache");
    assert_eq!(run(&mut m, -0x68, &[2]), 0, "iFlushCache");
    assert_eq!(run(&mut m, 0x76, &[1]), -1, "sceSifDmaStat: done");
    let d1 = run(&mut m, 0x77, &[0, 1]);
    assert!(d1 != 0, "sceSifSetDma returns a non-zero id");
    assert_eq!(run(&mut m, 0x79, &[0x8000_0001, 0x1e40218]), 0, "SetReg returns the old value");
    assert_eq!(run(&mut m, 0x7a, &[0x8000_0001]), 0x1e40218);
    assert_eq!(run(&mut m, 0x7a, &[4]) & 0x40000, 0x40000, "IOP boot done bit");
    assert_eq!(run(&mut m, 0x71, &[0xff00]), 0, "GsPutIMR returns the old IMR");
    for n in [0x02, 0x6b, 0x74, 0x5a, 0x83, 0x5b, 0x78, 0x7c] {
        assert_eq!(run(&mut m, n, &[0, 0, 0, 0]), 0, "syscall {n:#x} is accepted and ignored");
    }
    let k = m.kernel.borrow();
    assert!(k.unimplemented.is_empty(), "{:?}", k.unimplemented);
}

#[test]
fn an_unknown_syscall_is_recorded_and_returns_zero() {
    let mut m = machine();
    let mut main = vec![addiu(V0, ZERO, 77)];
    main.extend(sys(0x6f));
    main.extend(li(T0, COUNTER + 0x80));
    main.push(sw(V0, 0, T0));
    main.extend(goto_stop());
    let id = thread(&mut m, 0x0010_0000, &main, 10);
    start_main(&mut m, id);
    assert_eq!(m.run(1_000).unwrap(), RunStop::Requested);
    assert_eq!(m.ee.r32(COUNTER + 0x80), 0);
    assert_eq!(m.kernel.borrow().unimplemented.len(), 1);
    assert_eq!(m.kernel.borrow().unimplemented[0].0, 0x6f);
}

#[test]
fn a_blocking_call_in_a_handler_context_is_refused() {
    let mut m = machine();
    let s = m.kernel.borrow_mut().create_sema(0, 1, 0, 0, 0);
    // A handler that calls WaitSema (illegal) gets KE_CAN_NOT_WAIT back and returns normally.
    let mut h = call(0x44, &[s]);
    h.extend(li(T0, COUNTER + 0x90));
    h.extend([sw(V0, 0, T0), jr(RA), NOP]);
    put(&mut m, 0x0010_0800, &h);
    let mut main = call(0x10, &[CAUSE_VBLANK_START, 0x0010_0800, u32::MAX, 0]);
    main.extend(call(0x14, &[CAUSE_VBLANK_START]));
    main.push(EI);
    main.extend([beq(ZERO, ZERO, -1), NOP]);
    let id = thread(&mut m, 0x0010_0000, &main, 10);
    start_main(&mut m, id);
    assert_eq!(m.run(5_000_000).unwrap(), RunStop::Budget);
    assert_eq!(m.ee.r32(COUNTER + 0x90) as i32, -416);
    let _ = (A1, A2, A3);
}
