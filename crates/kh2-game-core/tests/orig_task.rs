//! Oracle tests for the task manager at 0x1022b0..0x102a28: kh2-ee runs the original on random task lists and the port
//! (`kh2_game_core::task`) runs on the same operations; the list order, every block byte, the events (finalizers, thread and block
//! frees, task runs) and the running-task word are compared after every case.
//!
//! The harness is written for this unit instead of `factory gen` because the inputs are linked lists the generator cannot lay out
//! (prev links, tail, handles that change under frees). It follows the same rules: the original runs from the user's executable, a
//! case counts only when everything agrees, cases are drawn from a seeded generator (a failing case number reproduces), and the
//! first cases audit that the original writes nothing outside the arenas below.
//!
//! Stand-ins (stubs), all documented here and nowhere else:
//! - The manager's vtable slot 0 (`0x32c4b0`, allocate a task) and slot 1 (`0x32c4d8`, free a task, thread or manager) are hooks: the
//!   first hands out zeroed blocks at `TASKS + n * 0x1000`, the second logs the freed pointer. The allocator behind them is another unit.
//! - The allocator object that `FUN_001024a0` allocates the manager from is a fake object whose vtable slot 0 is a hook handing out
//!   `MGRS + n * 0x1000`.
//! - `FUN_00102340` (run one task) is a hook that logs the task and then, from a seeded plan the port repeats, leaves the task alone,
//!   sets its function to 0, sets another task's function to 0, creates a task (through the original `FUN_00102778`), or kills the
//!   tasks of another class bits value (through the original `FUN_00102678`); the port's host does the same.
//! - Finalizers are three hooked addresses that log the task they get.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_game_core::task::*;

const ALLOC: u32 = 0x0170_0000;
const VT: u32 = 0x0170_0100;
const ALLOC_FN: u32 = 0x0170_0200;
const MGRS: u32 = 0x0180_0000;
const TASKS: u32 = 0x0190_0000;
const ARENA_LEN: u32 = 0x0010_0000;
const FINS: [u32; 3] = [0x0171_0000, 0x0171_0010, 0x0171_0020];
const THREADS: [u32; 3] = [0x0200_0010, 0x0200_0020, 0x0200_0030];
const RUNNING: u32 = 0x33e6c0;
const NESTED_STACK: u32 = 0x01ef_0000;
const CAP: u32 = 24;

/// Event kinds: finalizer (function, task), thread free, task free, manager free, task run.
const FIN: u32 = 1;
const THREAD: u32 = 2;
const TASK_FREE: u32 = 3;
const MGR_FREE: u32 = 4;
const RUN: u32 = 5;
type Ev = (u32, u32, u32);

struct Act {
    kind: u64,
    idx: usize,
    bits: u32,
    prio: i32,
    func: u32,
}

fn draw(r: &mut Rng) -> Act {
    Act { kind: r.below(10), idx: r.word() as usize, bits: r.below(16) as u32, prio: r.below(7) as i32 - 3, func: r.below(4) as u32 }
}

struct Plan {
    rng: Rng,
    created: u32,
}

#[derive(Default)]
struct Shared {
    next_task: u32,
    next_mgr: u32,
    last_task: u32,
    last_mgr: u32,
    log: Vec<Ev>,
    plan: Option<Plan>,
}

fn sx(v: u32) -> u64 {
    v as i32 as i64 as u64
}

fn ee_list(ee: &mut Ee, mgr: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut p = ee.r32(mgr + 8);
    while p != 0 && out.len() < 1000 {
        out.push(p);
        p = ee.r32(p + 0x38);
    }
    out
}

fn install_hooks(ee: &mut Ee, sh: &Rc<RefCell<Shared>>) {
    let s = sh.clone();
    ee.hooks.insert(0x32c4b0, move |ee: &mut Ee| {
        assert_eq!(ee.gpr[5] as u32, 0x48, "the manager allocates tasks of 0x48 bytes");
        let a = {
            let mut s = s.borrow_mut();
            let a = TASKS + s.next_task * 0x1000;
            s.next_task += 1;
            s.last_task = a;
            a
        };
        ee.write(a, &[0u8; 0x1000]);
        ee.gpr[2] = a as u128;
    });
    let s = sh.clone();
    ee.hooks.insert(ALLOC_FN, move |ee: &mut Ee| {
        assert_eq!(ee.gpr[5] as u32, 0x120, "the manager block is 0x120 bytes");
        let a = {
            let mut s = s.borrow_mut();
            let a = MGRS + s.next_mgr * 0x1000;
            s.next_mgr += 1;
            s.last_mgr = a;
            a
        };
        ee.write(a, &[0u8; 0x1000]);
        ee.gpr[2] = a as u128;
    });
    let s = sh.clone();
    ee.hooks.insert(0x32c4d8, move |ee: &mut Ee| {
        let (owner, p) = (ee.gpr[4] as u32, ee.gpr[5] as u32);
        let kind = if (TASKS..TASKS + ARENA_LEN).contains(&p) {
            TASK_FREE
        } else if p == owner {
            MGR_FREE
        } else {
            THREAD
        };
        s.borrow_mut().log.push((kind, p, 0));
    });
    for f in FINS {
        let s = sh.clone();
        ee.hooks.insert(f, move |ee: &mut Ee| {
            s.borrow_mut().log.push((FIN, f, ee.gpr[4] as u32));
        });
    }
    let s = sh.clone();
    ee.hooks.insert(0x102340, move |ee: &mut Ee| {
        let task = ee.gpr[4] as u32;
        let act = {
            let mut s = s.borrow_mut();
            s.log.push((RUN, task, 0));
            s.plan.as_mut().map(|p| (draw(&mut p.rng), p.created < CAP))
        };
        let Some((a, may_create)) = act else { return };
        let owner = ee.r32(task + 0x24);
        let list = ee_list(ee, owner);
        let other = list[a.idx % list.len()];
        let create = |ee: &mut Ee| {
            if may_create {
                s.borrow_mut().plan.as_mut().unwrap().created += 1;
                ee.call(0x102778, &[owner as u64, a.bits as u64, sx(a.prio as u32), a.func as u64], &[], NESTED_STACK).unwrap();
            }
        };
        match a.kind {
            4 | 5 => ee.w32(task, 0),
            6 => ee.w32(other, 0),
            7 => create(ee),
            8 => {
                let (b, mine) = (ee.r32(other + 0x28), ee.r32(task + 0x28));
                if b != mine {
                    ee.call(0x102678, &[owner as u64, sx(0xffff_ffff), sx(b)], &[], NESTED_STACK).unwrap();
                }
            }
            9 => {
                ee.w32(task, 0);
                create(ee);
            }
            _ => {}
        }
    });
}

/// The port's side: its blocks, its handle to address map, its events, and the plan for the run hook.
struct Port {
    tasks: Tasks,
    map: HashMap<u32, u32>,
    next_task: u32,
    next_mgr: u32,
    log: Vec<Ev>,
    plan: Option<Plan>,
}

impl Port {
    fn new() -> Port {
        Port { tasks: Tasks::new(), map: HashMap::new(), next_task: 0, next_mgr: 0, log: Vec::new(), plan: None }
    }
    fn tr(&self, h: u32) -> u32 {
        if h == 0 { 0 } else { self.map.get(&h).copied().unwrap_or(h) }
    }
}

struct Host<'a>(&'a mut Port);

fn bind_task(p: &mut Port, h: u32) {
    p.map.insert(h, TASKS + p.next_task * 0x1000);
    p.next_task += 1;
}

impl TaskHost for Host<'_> {
    fn run(&mut self, tasks: &mut Tasks, task: u32) {
        let t = self.0.tr(task);
        self.0.log.push((RUN, t, 0));
        let Some(p) = self.0.plan.as_mut() else { return };
        let a = draw(&mut p.rng);
        let may_create = p.created < CAP;
        let owner = tasks.task(task).unwrap().owner();
        let list = tasks.list(owner);
        let other = list[a.idx % list.len()];
        let create = |me: &mut Host, tasks: &mut Tasks| {
            if may_create {
                me.0.plan.as_mut().unwrap().created += 1;
                let h = tasks.create_task(owner, a.bits, a.prio, a.func);
                bind_task(me.0, h);
            }
        };
        match a.kind {
            4 | 5 => tasks.task_mut(task).unwrap().set_function(0),
            6 => tasks.task_mut(other).unwrap().set_function(0),
            7 => create(self, tasks),
            8 => {
                let (b, mine) = (tasks.task(other).unwrap().bits(), tasks.task(task).unwrap().bits());
                if b != mine {
                    tasks.kill(owner, 0xffff_ffff, b, self);
                }
            }
            9 => {
                tasks.task_mut(task).unwrap().set_function(0);
                create(self, tasks);
            }
            _ => {}
        }
    }
    fn finalize(&mut self, _tasks: &mut Tasks, task: u32, finalizer: u32) {
        let t = self.0.tr(task);
        self.0.log.push((FIN, finalizer, t));
    }
    fn free_thread(&mut self, thread: u32) {
        self.0.log.push((THREAD, thread, 0));
    }
    fn task_freed(&mut self, task: u32) {
        let t = self.0.tr(task);
        self.0.log.push((TASK_FREE, t, 0));
    }
    fn manager_freed(&mut self, manager: u32) {
        let m = self.0.tr(manager);
        self.0.log.push((MGR_FREE, m, 0));
    }
}

struct World {
    ee: Ee,
    sh: Rc<RefCell<Shared>>,
    port: Port,
    /// Events seen by kind, over all cases.
    tally: [u64; 6],
    /// Live tasks compared, over all cases.
    compared: u64,
}

impl World {
    fn new(what: &str) -> Option<World> {
        let elf = kh2_factory::exe_or_skip(what)?;
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let sh = Rc::new(RefCell::new(Shared::default()));
        install_hooks(&mut ee, &sh);
        ee.w32(ALLOC, VT);
        ee.w32(VT, ALLOC_FN);
        Some(World { ee, sh, port: Port::new(), tally: [0; 6], compared: 0 })
    }

    fn reset(&mut self) {
        *self.sh.borrow_mut() = Shared::default();
        self.port = Port::new();
        self.ee.w32(RUNNING, 0);
    }

    fn call(&mut self, addr: u32, args: &[u64]) -> u32 {
        self.ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}")) as u32
    }

    fn manager(&mut self) -> u32 {
        let e = self.call(0x1024a0, &[ALLOC as u64]);
        let _ = e;
        let em = self.sh.borrow().last_mgr;
        let h = self.port.tasks.create_manager(ALLOC);
        self.port.map.insert(h, MGRS + self.port.next_mgr * 0x1000);
        self.port.next_mgr += 1;
        assert_eq!(self.port.map[&h], em, "manager addresses run in step");
        h
    }

    fn task(&mut self, mgr: u32, bits: u32, prio: i32, func: u32) -> u32 {
        let m = self.port.map[&mgr];
        let e = self.call(0x102778, &[m as u64, bits as u64, sx(prio as u32), func as u64]);
        let h = self.port.tasks.create_task(mgr, bits, prio, func);
        bind_task(&mut self.port, h);
        assert_eq!(e, self.port.map[&h], "the original returns the new task");
        h
    }

    /// Sets the finalizer and the thread word of a task on both sides.
    fn set_extras(&mut self, h: u32, fin: u32, thread: u32) {
        let a = self.port.map[&h];
        self.ee.w32(a + 0x30, fin);
        self.ee.w32(a + 0x34, thread);
        let t = self.port.tasks.task_mut(h).unwrap();
        t.set_finalizer(fin);
        t.set_thread(thread);
    }

    /// 1 or 2 managers with random lists; returns the manager handles. Priorities repeat on purpose.
    fn build(&mut self, r: &mut Rng, max: u64) -> Vec<u32> {
        let mut ms = Vec::new();
        for _ in 0..1 + r.below(2) {
            let m = self.manager();
            for _ in 0..r.below(max + 1) {
                let bits = match r.below(4) {
                    0 => 0,
                    1 => 1 << r.below(4),
                    2 => r.below(16) as u32,
                    _ => r.word() as u32,
                };
                let prio = match r.below(12) {
                    0 => i32::MIN,
                    1 => i32::MAX,
                    _ => r.below(7) as i32 - 3,
                };
                let func = if r.below(5) == 0 { 0 } else { 1 + r.below(4) as u32 };
                let h = self.task(m, bits, prio, func);
                let fin = if r.below(3) == 0 { FINS[r.below(3) as usize] } else { 0 };
                let thread = if r.below(3) == 0 { THREADS[r.below(3) as usize] } else { 0 };
                self.set_extras(h, fin, thread);
            }
            ms.push(m);
        }
        ms
    }

    fn pick_task(&mut self, r: &mut Rng, m: u32) -> Option<u32> {
        let l = self.port.tasks.list(m);
        (!l.is_empty()).then(|| l[r.below(l.len() as u64) as usize])
    }

    /// A (mask, value) pair: sometimes matching the class bits of a listed task.
    fn mask_value(&mut self, r: &mut Rng, m: u32) -> (u32, u32) {
        let mask = match r.below(5) {
            0 => 0,
            1 => 0xffff_ffff,
            2 => 1 << r.below(4),
            3 => r.below(16) as u32,
            _ => r.word() as u32,
        };
        let value = match (r.below(3), self.pick_task(r, m)) {
            (0, _) | (_, None) => r.word() as u32 & mask,
            (1, Some(t)) => self.port.tasks.task(t).unwrap().bits() & mask,
            _ => r.word() as u32,
        };
        (mask, value)
    }

    /// Sets the manager's resume word on both sides to 0, a listed task, or a wild value.
    fn set_resume(&mut self, r: &mut Rng, m: u32) {
        let v = match (r.below(3), self.pick_task(r, m)) {
            (0, _) | (_, None) => 0,
            (_, Some(t)) => t,
        };
        let (ma, va) = (self.port.map[&m], self.port.tr(v));
        self.ee.w32(ma + 0x110, va);
        self.port.tasks.manager_mut(m).unwrap().set_resume(v);
    }

    fn set_running(&mut self, v: u32) {
        self.ee.w32(RUNNING, v);
        self.port.tasks.running = v;
    }

    fn start_plan(&mut self, seed: u64) {
        self.sh.borrow_mut().plan = Some(Plan { rng: Rng::new(seed), created: 0 });
        self.port.plan = Some(Plan { rng: Rng::new(seed), created: 0 });
    }

    /// Compares everything the original can have touched.
    fn verify(&mut self, case: u32) {
        let tr = |p: &Port, b: &mut [u8], offs: &[usize]| {
            for &o in offs {
                let v = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
                b[o..o + 4].copy_from_slice(&p.tr(v).to_le_bytes());
            }
        };
        for m in self.port.tasks.live_managers() {
            let a = self.port.map[&m];
            let mut mine = self.port.tasks.manager(m).unwrap().0;
            tr(&self.port, &mut mine, &[8, 0xc, 0x10, 0x110]);
            let game: Vec<u8> = (0..MANAGER_SIZE as u32).map(|i| self.ee.r8(a + i)).collect();
            if let Some(o) = (0..MANAGER_SIZE).find(|&o| game[o] != mine[o]) {
                panic!("case {case}: manager {a:#x} byte +{o:#x}: game {:02x} port {:02x}", game[o], mine[o]);
            }
        }
        for t in self.port.tasks.live_tasks() {
            let a = self.port.map[&t];
            let mut mine = self.port.tasks.task(t).unwrap().0;
            tr(&self.port, &mut mine, &[0x24, 0x38, 0x3c]);
            let game: Vec<u8> = (0..TASK_SIZE as u32).map(|i| self.ee.r8(a + i)).collect();
            if let Some(o) = (0..TASK_SIZE).find(|&o| game[o] != mine[o]) {
                panic!("case {case}: task {a:#x} byte +{o:#x}: game {:02x} port {:02x}", game[o], mine[o]);
            }
            self.compared += 1;
        }
        let run = self.ee.r32(RUNNING);
        assert_eq!(run, self.port.tr(self.port.tasks.running), "case {case}: the running-task word");
        let game = std::mem::take(&mut self.sh.borrow_mut().log);
        assert_eq!(game, self.port.log, "case {case}: events (kind, a, b) game vs port");
        for e in &game {
            self.tally[e.0 as usize] += 1;
        }
        self.port.log.clear();
    }
}

/// Runs `cases` seeded cases of `body`, each on fresh state, verified after the body; the first cases audit the original's writes.
fn each(w: &mut World, cases: u32, seed: u64, mut body: impl FnMut(&mut World, &mut Rng, u32)) {
    for n in 0..cases {
        w.reset();
        let mut r = Rng::new(seed + n as u64);
        let before = (n < 8).then(|| w.ee.ram.clone());
        body(w, &mut r, n);
        w.verify(n);
        if let Some(before) = before {
            let inside = |a: u32| {
                (MGRS..MGRS + ARENA_LEN).contains(&a)
                    || (TASKS..TASKS + ARENA_LEN).contains(&a)
                    || (NESTED_STACK - 0x10000..NESTED_STACK + 0x100).contains(&a)
                    || (STACK - 0x10000..STACK + 0x100).contains(&a)
                    || (ALLOC..ALLOC + 0x1000).contains(&a)
                    || (RUNNING..RUNNING + 4).contains(&a)
            };
            if let Some(a) = (0..w.ee.ram.len()).find(|&i| before[i] != w.ee.ram[i] && !inside(i as u32)) {
                panic!("case {n}: the original wrote {a:#x}, outside the arenas, the stacks and the running word");
            }
        }
    }
}

const CASES: u32 = 3000;

#[test]
fn orig_1022b0_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x1022b0") else { return };
    each(&mut w, CASES, 1, |w, r, _| {
        // A block full of garbage: init must leave the words it does not write alone.
        let mut b = [0u8; TASK_SIZE];
        for x in b.iter_mut() {
            *x = r.word() as u8;
        }
        let (mgr, bits, prio, func) = (r.word() as u32 & 0x7fff_ffff, r.word() as u32, r.word() as i32, r.word() as u32);
        let at = TASKS + 0x1000 * 100;
        w.ee.write(at, &b);
        w.call(0x1022b0, &[at as u64, sx(mgr), sx(bits), sx(prio as u32), sx(func)]);
        let mut t = Task(b);
        t.init(mgr, bits, prio, func);
        let game: Vec<u8> = (0..TASK_SIZE as u32).map(|i| w.ee.r8(at + i)).collect();
        assert_eq!(game, t.0.to_vec());
        w.tally[0] += 1;
    });
    assert!(w.tally[0] >= CASES as u64);
}

#[test]
fn orig_1024a0_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x1024a0") else { return };
    each(&mut w, CASES, 2, |w, r, _| {
        for _ in 0..1 + r.below(3) {
            w.manager();
        }
    });
    assert!(w.compared == 0);
    assert!(w.port.tasks.live_managers().len() <= 3);
}

#[test]
fn orig_102778_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x102778") else { return };
    each(&mut w, CASES, 3, |w, r, _| {
        let ms = w.build(r, 12);
        let m = ms[r.below(ms.len() as u64) as usize];
        for _ in 0..3 {
            w.task(m, r.below(16) as u32, r.below(7) as i32 - 3, 1 + r.below(4) as u32);
        }
    });
    assert!(w.compared > CASES as u64);
}

#[test]
fn orig_102518_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x102518") else { return };
    each(&mut w, CASES, 4, |w, r, _| {
        let ms = w.build(r, 12);
        let m = ms[r.below(ms.len() as u64) as usize];
        w.set_resume(r, m);
        // A task made by hand (init) and put into the list by the insert alone.
        let prio = match r.below(8) {
            0 => i32::MIN,
            1 => i32::MAX,
            _ => r.below(7) as i32 - 3,
        };
        let bits = r.below(16) as u32;
        let ma = w.port.map[&m];
        let ea = {
            let mut s = w.sh.borrow_mut();
            let a = TASKS + s.next_task * 0x1000;
            s.next_task += 1;
            a
        };
        w.ee.write(ea, &[0u8; 0x1000]);
        w.call(0x1022b0, &[ea as u64, ma as u64, sx(bits), sx(prio as u32), 9]);
        let h = w.port.tasks.new_task(m, bits, prio, 9);
        bind_task(&mut w.port, h);
        assert_eq!(w.port.map[&h], ea);
        w.call(0x102518, &[ma as u64, ea as u64]);
        w.port.tasks.insert(m, h);
    });
    assert!(w.compared > CASES as u64);
}

#[test]
fn orig_1022d8_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x1022d8") else { return };
    each(&mut w, CASES, 5, |w, r, _| {
        let ms = w.build(r, 8);
        let m = ms[0];
        let Some(t) = w.pick_task(r, m) else { return };
        let a = w.port.map[&t];
        w.call(0x1022d8, &[a as u64]);
        free_port(w, |tasks, host| tasks.release(t, host));
    });
}

#[test]
fn orig_1025e8_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x1025e8") else { return };
    each(&mut w, CASES, 6, |w, r, _| {
        let ms = w.build(r, 10);
        let m = ms[0];
        w.set_resume(r, m);
        let Some(t) = w.pick_task(r, m) else { return };
        let (ma, ta) = (w.port.map[&m], w.port.map[&t]);
        w.call(0x1025e8, &[ma as u64, ta as u64]);
        free_port(w, |tasks, host| tasks.unlink_and_free(m, t, host));
    });
    assert!(w.tally[TASK_FREE as usize] > 100);
    assert!(w.tally[FIN as usize] > 20 && w.tally[THREAD as usize] > 20);
}

#[test]
fn orig_102660_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x102660") else { return };
    each(&mut w, CASES, 7, |w, r, _| {
        let ms = w.build(r, 10);
        let m = ms[0];
        w.set_resume(r, m);
        let Some(t) = w.pick_task(r, m) else { return };
        let (ma, ta) = (w.port.map[&m], w.port.map[&t]);
        w.call(0x102660, &[ma as u64, ta as u64]);
        free_port(w, |tasks, host| tasks.finish(m, t, host));
    });
    assert!(w.tally[TASK_FREE as usize] > 100);
}

/// Runs a port operation that frees things with a host that logs into the port's event list.
fn free_port(w: &mut World, f: impl FnOnce(&mut Tasks, &mut Host)) {
    let mut tasks = std::mem::take(&mut w.port.tasks);
    f(&mut tasks, &mut Host(&mut w.port));
    w.port.tasks = tasks;
}

#[test]
fn orig_102678_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x102678") else { return };
    each(&mut w, CASES, 8, |w, r, _| {
        let ms = w.build(r, 12);
        let m = ms[0];
        w.set_resume(r, m);
        let (mask, value) = w.mask_value(r, m);
        let ma = w.port.map[&m];
        w.call(0x102678, &[ma as u64, sx(mask), sx(value)]);
        free_port(w, |tasks, host| tasks.kill(m, mask, value, host));
    });
    assert!(w.tally[TASK_FREE as usize] > 1000);
}

#[test]
fn orig_102720_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x102720") else { return };
    each(&mut w, CASES, 9, |w, r, _| {
        let ms = w.build(r, 12);
        let m = ms[0];
        let ma = w.port.map[&m];
        w.call(0x102720, &[ma as u64]);
        free_port(w, |tasks, host| tasks.kill_all(m, host));
        assert_eq!(w.port.tasks.count(m), 0);
    });
    assert!(w.tally[TASK_FREE as usize] > 1000);
}

#[test]
fn orig_102740_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x102740") else { return };
    each(&mut w, CASES, 10, |w, r, _| {
        let ms = w.build(r, 12);
        let m = ms[0];
        let ma = w.port.map[&m];
        w.call(0x102740, &[ma as u64]);
        free_port(w, |tasks, host| tasks.destroy_manager(m, host));
    });
    assert!(w.tally[MGR_FREE as usize] >= CASES as u64);
    assert!(w.tally[TASK_FREE as usize] > 1000);
}

/// One pass on both sides with a seeded plan; the running word starts as garbage that must come back.
fn pass_case(w: &mut World, r: &mut Rng, n: u32, all: bool) {
    let ms = w.build(r, 12);
    let m = ms[0];
    let garbage = (r.word() as u32) | 0x4000_0000;
    w.set_running(garbage);
    let (mask, value) = if all { (0, 0) } else { w.mask_value(r, m) };
    let seed = 1000 + n as u64;
    w.start_plan(seed);
    let ma = w.port.map[&m];
    if all {
        w.call(0x1029d8, &[ma as u64]);
    } else {
        w.call(0x1028a0, &[ma as u64, sx(mask), sx(value)]);
    }
    free_port(w, |tasks, host| if all { tasks.pass_all(m, host) } else { tasks.pass(m, mask, value, host) });
    assert_eq!(w.port.tasks.running, garbage);
}

#[test]
fn orig_1028a0_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x1028a0") else { return };
    each(&mut w, CASES, 11, |w, r, n| pass_case(w, r, n, false));
    assert!(w.tally[RUN as usize] > 3000);
    assert!(w.tally[TASK_FREE as usize] > 1000);
    assert!(w.tally[FIN as usize] > 100);
}

#[test]
fn orig_1029d8_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x1029d8") else { return };
    each(&mut w, CASES, 12, |w, r, n| pass_case(w, r, n, true));
    assert!(w.tally[RUN as usize] > 3000);
    assert!(w.tally[TASK_FREE as usize] > 1000);
}

#[test]
fn orig_1029f8_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x1029f8") else { return };
    each(&mut w, CASES, 13, |w, r, _| {
        let ms = w.build(r, 12);
        for m in ms {
            let ma = w.port.map[&m];
            let game = w.call(0x1029f8, &[ma as u64]);
            assert_eq!(game, w.port.tasks.count(m));
        }
    });
    assert!(w.compared > 1000);
}

#[test]
fn orig_102a28_matches_the_game() {
    let Some(mut w) = World::new("oracle test of 0x102a28") else { return };
    let (mut yes, mut no) = (0, 0);
    each(&mut w, CASES, 14, |w, r, _| {
        let ms = w.build(r, 12);
        for m in ms {
            let ma = w.port.map[&m];
            let f = r.below(7) as u32;
            let game = w.call(0x102a28, &[ma as u64, f as u64]);
            let mine = w.port.tasks.contains_function(m, f);
            assert_eq!(game, mine as u32, "function {f}");
            if mine { yes += 1 } else { no += 1 }
        }
    });
    assert!(yes > 100 && no > 100, "{yes} found, {no} not found");
}
