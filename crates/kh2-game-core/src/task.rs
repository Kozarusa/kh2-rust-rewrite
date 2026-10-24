//! The game's task manager, ported on the original's byte layout (core runtime, WP-L1-P2; docs/systems/core-runtime/runtime.md
//! "Task manager").
//!
//! A manager (0x120 bytes) keeps a doubly linked list of tasks (0x48 bytes each) sorted by priority: the lowest priority number runs
//! first and a new task goes behind every task of equal or lower number. One pass over the list runs every task whose class bits
//! match `(bits & mask) == value`, and a task whose function word became 0 during its run is unlinked and freed. Both block types are
//! plain byte arrays with typed accessors at the original's offsets (RULES C7), so an oracle test can compare them byte by byte with
//! the original in kh2-ee. Pointer fields hold handles, 0 for null; the owner of the blocks is [`Tasks`].
//!
//! Handles: a task is `TASK_BASE + slot * 0x1000`, a manager `MANAGER_BASE + slot * 0x1000` (the FAC-8 handle form; every block fits
//! the stride). The slot policy (lowest free slot first) is the port's own: the original's allocator is a separate unit (0x105fd8,
//! not ported here), and the oracle test translates handles to the addresses its stand-in allocator gave.
//!
//! The runner (`FUN_00102340`, [`Tasks::run_task`]), the coroutine create (`FUN_00102808`), yield and wait (`FUN_00102438`,
//! `FUN_00102460`), the game modes (`FUN_00101e98`..`FUN_00102268`, [`modes`]) and the field mode block (`FUN_00102a60`..`FUN_00102c30`,
//! [`field`]) came with WP-L1-P20. What is NOT here: the allocator behind the manager blocks, and the context switch itself
//! (`FUN_00105440`, `FUN_001054a8`, `FUN_00105510`: register save and load, hardware), which a [`RunHost`] stands for. The pass hands
//! each task to a [`TaskHost`] ([`Runner`] makes one from a [`RunHost`]); the frame-order package wires a real host. Nothing in
//! the app uses this module yet.

/// Size of a task block.
pub const TASK_SIZE: usize = 0x48;
/// Size of a manager block.
pub const MANAGER_SIZE: usize = 0x120;
/// First task handle; slot `n` is `TASK_BASE + n * HANDLE_STRIDE`.
pub const TASK_BASE: u32 = 0x0900_0000;
/// First manager handle.
pub const MANAGER_BASE: u32 = 0x0a00_0000;
/// Distance between two handles of one kind.
pub const HANDLE_STRIDE: u32 = 0x1000;
/// The word the original stores at manager +0: the address of its vtable in the executable (`FUN_001024a0`). Slot 0 allocates, slot 1
/// frees (called with the manager as the first argument).
pub const MANAGER_VTABLE: u32 = 0x0033_e6d0;

fn get(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn put(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// A task block (0x48 bytes). Fields not listed (+0x4..+0x23, +0x3c..) are only used by the coroutine code of the original.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Task(pub [u8; TASK_SIZE]);

impl Default for Task {
    fn default() -> Self {
        Task([0; TASK_SIZE])
    }
}

impl Task {
    /// The code address of the task's function; 0 means the task is finished and is freed after its pass step.
    pub fn function(&self) -> u32 {
        get(&self.0, 0)
    }
    pub fn set_function(&mut self, v: u32) {
        put(&mut self.0, 0, v)
    }
    /// The word at +4: a coroutine task keeps its argument here (the game mode of a mode task, `FUN_00102070`); other users keep other
    /// state in the same bytes.
    pub fn arg(&self) -> u32 {
        get(&self.0, 4)
    }
    pub fn set_arg(&mut self, v: u32) {
        put(&mut self.0, 4, v)
    }
    /// The manager that owns the task (its free routine releases the task).
    pub fn owner(&self) -> u32 {
        get(&self.0, 0x24)
    }
    pub fn set_owner(&mut self, v: u32) {
        put(&mut self.0, 0x24, v)
    }
    /// Class bits the pass mask selects on.
    pub fn bits(&self) -> u32 {
        get(&self.0, 0x28)
    }
    pub fn set_bits(&mut self, v: u32) {
        put(&mut self.0, 0x28, v)
    }
    /// List priority, compared as a signed number.
    pub fn priority(&self) -> i32 {
        get(&self.0, 0x2c) as i32
    }
    pub fn set_priority(&mut self, v: i32) {
        put(&mut self.0, 0x2c, v as u32)
    }
    /// A function called with the task when it is freed, 0 for none.
    pub fn finalizer(&self) -> u32 {
        get(&self.0, 0x30)
    }
    pub fn set_finalizer(&mut self, v: u32) {
        put(&mut self.0, 0x30, v)
    }
    /// The coroutine thread block, 0 for a plain call; released with the task.
    pub fn thread(&self) -> u32 {
        get(&self.0, 0x34)
    }
    pub fn set_thread(&mut self, v: u32) {
        put(&mut self.0, 0x34, v)
    }
    pub fn next(&self) -> u32 {
        get(&self.0, 0x38)
    }
    pub fn set_next(&mut self, v: u32) {
        put(&mut self.0, 0x38, v)
    }
    pub fn prev(&self) -> u32 {
        get(&self.0, 0x3c)
    }
    pub fn set_prev(&mut self, v: u32) {
        put(&mut self.0, 0x3c, v)
    }

    /// The fields `FUN_001022b0` writes, with `mgr` the owner: function, owner, bits, priority are set, the finalizer, the thread and
    /// the words at +0x40 and +0x44 are cleared. The links and the other words stay as they were.
    ///
    /// orig 0x1022b0
    pub fn init(&mut self, mgr: u32, bits: u32, priority: i32, function: u32) {
        put(&mut self.0, 0x24, mgr);
        put(&mut self.0, 0, function);
        put(&mut self.0, 0x28, bits);
        put(&mut self.0, 0x2c, priority as u32);
        put(&mut self.0, 0x44, 0);
        put(&mut self.0, 0x30, 0);
        put(&mut self.0, 0x34, 0);
        put(&mut self.0, 0x40, 0);
    }
}

/// A manager block (0x120 bytes).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Manager(pub [u8; MANAGER_SIZE]);

impl Default for Manager {
    fn default() -> Self {
        Manager([0; MANAGER_SIZE])
    }
}

impl Manager {
    /// The allocator object the manager takes its tasks from (opaque to this module).
    pub fn allocator(&self) -> u32 {
        get(&self.0, 4)
    }
    /// First task of the list.
    pub fn head(&self) -> u32 {
        get(&self.0, 8)
    }
    pub fn set_head(&mut self, v: u32) {
        put(&mut self.0, 8, v)
    }
    /// Last task of the list.
    pub fn tail(&self) -> u32 {
        get(&self.0, 0xc)
    }
    pub fn set_tail(&mut self, v: u32) {
        put(&mut self.0, 0xc, v)
    }
    /// The task a pass is running now (set around each run, 0 between runs).
    pub fn current(&self) -> u32 {
        get(&self.0, 0x10)
    }
    pub fn set_current(&mut self, v: u32) {
        put(&mut self.0, 0x10, v)
    }
    /// The task the running pass visits next; list edits made during a run keep it valid.
    pub fn resume(&self) -> u32 {
        get(&self.0, 0x110)
    }
    pub fn set_resume(&mut self, v: u32) {
        put(&mut self.0, 0x110, v)
    }

    /// A fresh manager block: the vtable word, the allocator, an empty list.
    ///
    /// orig 0x1024a0
    pub fn init(&mut self, allocator: u32) {
        put(&mut self.0, 4, allocator);
        put(&mut self.0, 0x110, 0);
        put(&mut self.0, 0, MANAGER_VTABLE);
        put(&mut self.0, 0xc, 0);
        put(&mut self.0, 8, 0);
        put(&mut self.0, 0x10, 0);
    }
}

/// What the task manager calls out to. The original's `FUN_00102340` (run one task) is the only required part; the default
/// methods ignore the release events.
pub trait TaskHost {
    /// Runs one task: the original's `FUN_00102340`. The task is the running task (`Tasks::running`) and its manager's `current`.
    /// It may change the list (create tasks, set functions to 0, kill); it must not free the task it runs.
    fn run(&mut self, tasks: &mut Tasks, task: u32);
    /// The task's finalizer address is about to be called with the task (`FUN_001022d8`).
    fn finalize(&mut self, _tasks: &mut Tasks, _task: u32, _finalizer: u32) {}
    /// The task's thread block is released.
    fn free_thread(&mut self, _thread: u32) {}
    /// The task block itself is released (the last step of the free).
    fn task_freed(&mut self, _task: u32) {}
    /// A manager block is released (`FUN_00102740`).
    fn manager_freed(&mut self, _manager: u32) {}
}

/// A host that runs nothing: every task stays as it is.
pub struct NoHost;

impl TaskHost for NoHost {
    fn run(&mut self, _tasks: &mut Tasks, _task: u32) {}
}

/// The blocks of all managers and tasks, and the one global of the unit (the task a pass is running, `DAT_0033e6c0`).
#[derive(Default)]
pub struct Tasks {
    tasks: Vec<Option<Task>>,
    managers: Vec<Option<Manager>>,
    threads: Vec<Option<Thread>>,
    /// The jump-buffer stack of the runner (`0x3b21d8`, index `0x33e858`): the entries are the owner's buffer addresses, newest last.
    jmp: Vec<u32>,
    /// The running task (the original keeps it in the word at 0x33e6c0 and restores the old value after each run).
    pub running: u32,
}

fn slot_of(h: u32, base: u32) -> Option<usize> {
    let d = h.checked_sub(base)?;
    (d % HANDLE_STRIDE == 0).then_some((d / HANDLE_STRIDE) as usize)
}

fn alloc_slot<T>(v: &mut Vec<Option<T>>, x: T) -> usize {
    match v.iter().position(Option::is_none) {
        Some(i) => {
            v[i] = Some(x);
            i
        }
        None => {
            v.push(Some(x));
            v.len() - 1
        }
    }
}

impl Tasks {
    pub fn new() -> Tasks {
        Tasks::default()
    }

    /// The task block of a handle, `None` for null, a foreign handle or a freed task.
    pub fn task(&self, h: u32) -> Option<&Task> {
        self.tasks.get(slot_of(h, TASK_BASE)?)?.as_ref()
    }
    pub fn task_mut(&mut self, h: u32) -> Option<&mut Task> {
        self.tasks.get_mut(slot_of(h, TASK_BASE)?)?.as_mut()
    }
    /// The manager block of a handle.
    pub fn manager(&self, h: u32) -> Option<&Manager> {
        self.managers.get(slot_of(h, MANAGER_BASE)?)?.as_ref()
    }
    pub fn manager_mut(&mut self, h: u32) -> Option<&mut Manager> {
        self.managers.get_mut(slot_of(h, MANAGER_BASE)?)?.as_mut()
    }
    /// Handles of all live tasks (linked or not), in slot order.
    pub fn live_tasks(&self) -> Vec<u32> {
        (0..self.tasks.len()).filter(|&i| self.tasks[i].is_some()).map(|i| TASK_BASE + i as u32 * HANDLE_STRIDE).collect()
    }
    /// Handles of all live managers.
    pub fn live_managers(&self) -> Vec<u32> {
        (0..self.managers.len()).filter(|&i| self.managers[i].is_some()).map(|i| MANAGER_BASE + i as u32 * HANDLE_STRIDE).collect()
    }

    fn tk(&self, h: u32) -> &Task {
        self.task(h).unwrap_or_else(|| panic!("task handle {h:#x} is not live"))
    }
    fn tk_mut(&mut self, h: u32) -> &mut Task {
        self.task_mut(h).unwrap_or_else(|| panic!("task handle {h:#x} is not live"))
    }
    fn mg(&self, h: u32) -> &Manager {
        self.manager(h).unwrap_or_else(|| panic!("manager handle {h:#x} is not live"))
    }
    fn mg_mut(&mut self, h: u32) -> &mut Manager {
        self.manager_mut(h).unwrap_or_else(|| panic!("manager handle {h:#x} is not live"))
    }

    /// Makes a manager over `allocator` (an opaque word) with an empty list and returns its handle. The original allocates the block
    /// from the allocator and runs the init below (the allocator itself, `FUN_001024f8` -> `FUN_00105fd8`, is not ported).
    ///
    /// orig 0x1024a0
    pub fn create_manager(&mut self, allocator: u32) -> u32 {
        let mut m = Manager::default();
        m.init(allocator);
        MANAGER_BASE + alloc_slot(&mut self.managers, m) as u32 * HANDLE_STRIDE
    }

    /// Allocates a task block of manager `mgr` and fills it (`FUN_00102778` before the insert, and `FUN_001022b0`). The task is not
    /// in the list yet.
    ///
    /// orig 0x1022b0
    pub fn new_task(&mut self, mgr: u32, bits: u32, priority: i32, function: u32) -> u32 {
        let mut t = Task::default();
        t.init(mgr, bits, priority, function);
        TASK_BASE + alloc_slot(&mut self.tasks, t) as u32 * HANDLE_STRIDE
    }

    /// Creates a task and puts it into the manager's list, returning its handle.
    ///
    /// orig 0x102778
    pub fn create_task(&mut self, mgr: u32, bits: u32, priority: i32, function: u32) -> u32 {
        let t = self.new_task(mgr, bits, priority, function);
        self.insert(mgr, t);
        t
    }

    /// Puts `task` into the list behind the last task whose priority is not above its own (so equal priorities keep creation order).
    /// When the pass's next task is the one the new task lands in front of, the new task becomes the next one.
    ///
    /// orig 0x102518
    pub fn insert(&mut self, mgr: u32, task: u32) {
        let prio = self.tk(task).priority();
        let head = self.mg(mgr).head();
        let (mut prev, mut cur) = (0u32, head);
        // Walk to the first task with a priority above the new one; `prev` is its predecessor (the last task when none is above).
        while cur != 0 && self.tk(cur).priority() <= prio {
            prev = cur;
            cur = self.tk(cur).next();
        }
        if cur != 0 && self.mg(mgr).resume() == cur {
            self.mg_mut(mgr).set_resume(task);
        }
        if prev != 0 {
            if prev == self.mg(mgr).tail() {
                self.tk_mut(prev).set_next(task);
                let tail = self.mg(mgr).tail();
                let t = self.tk_mut(task);
                t.set_next(0);
                t.set_prev(tail);
                self.mg_mut(mgr).set_tail(task);
            } else {
                let after = self.tk(prev).next();
                let t = self.tk_mut(task);
                t.set_prev(prev);
                t.set_next(after);
                self.tk_mut(after).set_prev(task);
                self.tk_mut(prev).set_next(task);
            }
            return;
        }
        let head = self.mg(mgr).head();
        if head == 0 {
            let m = self.mg_mut(mgr);
            m.set_tail(task);
            m.set_head(task);
            let t = self.tk_mut(task);
            t.set_prev(0);
            t.set_next(0);
            return;
        }
        self.tk_mut(head).set_prev(task);
        let t = self.tk_mut(task);
        t.set_prev(0);
        t.set_next(head);
        self.mg_mut(mgr).set_head(task);
    }

    /// The next task after `from` (the first when `from` is 0) whose `bits & mask` equals `value`, or 0.
    fn next_match(&self, mgr: u32, from: u32, mask: u32, value: u32) -> u32 {
        let mut p = from;
        loop {
            p = if p == 0 { self.mg(mgr).head() } else { self.tk(p).next() };
            if p == 0 || self.tk(p).bits() & mask == value {
                return p;
            }
        }
    }

    /// Releases a task that is already unlinked: its finalizer is called with it, then its thread block and the task block are freed.
    ///
    /// orig 0x1022d8
    pub fn release(&mut self, task: u32, host: &mut impl TaskHost) {
        let (finalizer, thread) = {
            let t = self.tk(task);
            (t.finalizer(), t.thread())
        };
        if finalizer != 0 {
            host.finalize(self, task, finalizer);
        }
        if thread != 0 {
            host.free_thread(thread);
            if let Some(i) = slot_of(thread, THREAD_BASE)
                && let Some(s) = self.threads.get_mut(i)
            {
                *s = None;
            }
        }
        host.task_freed(task);
        if let Some(i) = slot_of(task, TASK_BASE) {
            self.tasks[i] = None;
        }
    }

    /// Unlinks a task from its manager's list and frees it. A pass that was going to visit the task next goes to the task after it.
    ///
    /// orig 0x1025e8
    pub fn unlink_and_free(&mut self, mgr: u32, task: u32, host: &mut impl TaskHost) {
        let (next, prev) = (self.tk(task).next(), self.tk(task).prev());
        if task == self.mg(mgr).resume() {
            // `task` is never 0 here; the original's branch for 0 (the head) cannot be reached by its callers.
            self.mg_mut(mgr).set_resume(next);
        }
        if prev == 0 {
            self.mg_mut(mgr).set_head(next);
        } else {
            self.tk_mut(prev).set_next(next);
        }
        if next == 0 {
            self.mg_mut(mgr).set_tail(prev);
        } else {
            self.tk_mut(next).set_prev(prev);
        }
        let t = self.tk_mut(task);
        t.set_prev(0);
        t.set_next(0);
        self.release(task, host);
    }

    /// The same call as [`Tasks::unlink_and_free`] under the name the original's callers use (`FUN_00102660` forwards its arguments).
    ///
    /// orig 0x102660
    pub fn finish(&mut self, mgr: u32, task: u32, host: &mut impl TaskHost) {
        self.unlink_and_free(mgr, task, host)
    }

    /// Frees every task of the list whose `bits & mask` equals `value`, in list order.
    ///
    /// orig 0x102678
    pub fn kill(&mut self, mgr: u32, mask: u32, value: u32, host: &mut impl TaskHost) {
        let mut cur = self.next_match(mgr, 0, mask, value);
        while cur != 0 {
            // The next match is found before the current task goes away.
            let next = self.next_match(mgr, cur, mask, value);
            self.unlink_and_free(mgr, cur, host);
            cur = next;
        }
    }

    /// Frees every task of the list.
    ///
    /// orig 0x102720
    pub fn kill_all(&mut self, mgr: u32, host: &mut impl TaskHost) {
        self.kill(mgr, 0, 0, host)
    }

    /// Frees every task, then the manager block itself.
    ///
    /// orig 0x102740
    pub fn destroy_manager(&mut self, mgr: u32, host: &mut impl TaskHost) {
        self.kill_all(mgr, host);
        host.manager_freed(mgr);
        if let Some(i) = slot_of(mgr, MANAGER_BASE) {
            self.managers[i] = None;
        }
    }

    /// One pass over the list: every task whose `bits & mask` equals `value` is run through the host in list order, and a task whose
    /// function is 0 afterwards is unlinked and freed. The next task is fixed before each run and kept in the manager's `resume`
    /// word, which `insert` and `unlink_and_free` keep valid when the run edits the list. The running task is saved and restored
    /// around each run.
    ///
    /// orig 0x1028a0
    pub fn pass(&mut self, mgr: u32, mask: u32, value: u32, host: &mut impl TaskHost) {
        let mut cur = self.next_match(mgr, 0, mask, value);
        while cur != 0 {
            let next = self.next_match(mgr, cur, mask, value);
            self.mg_mut(mgr).set_resume(next);
            let saved = self.running;
            self.running = cur;
            self.mg_mut(mgr).set_current(cur);
            host.run(self, cur);
            self.running = saved;
            self.mg_mut(mgr).set_current(0);
            // A task the host freed from under the pass is treated as still running (the original would read freed memory).
            let finished = self.task(cur).is_some_and(|t| t.function() == 0);
            if finished {
                self.unlink_and_free(mgr, cur, host);
            }
            cur = self.mg(mgr).resume();
            if cur != 0 && self.tk(cur).bits() & mask != value {
                cur = self.next_match(mgr, cur, mask, value);
                self.mg_mut(mgr).set_resume(cur);
            }
        }
    }

    /// A pass over every task (mask 0, value 0).
    ///
    /// orig 0x1029d8
    pub fn pass_all(&mut self, mgr: u32, host: &mut impl TaskHost) {
        self.pass(mgr, 0, 0, host)
    }

    /// The number of tasks in the list.
    ///
    /// orig 0x1029f8
    pub fn count(&self, mgr: u32) -> u32 {
        let mut n = 0;
        let mut p = self.mg(mgr).head();
        while p != 0 {
            n += 1;
            p = self.tk(p).next();
        }
        n
    }

    /// Whether any task of the list has this function word.
    ///
    /// orig 0x102a28
    pub fn contains_function(&self, mgr: u32, function: u32) -> bool {
        let mut p = self.mg(mgr).head();
        while p != 0 {
            if self.tk(p).function() == function {
                return true;
            }
            p = self.tk(p).next();
        }
        false
    }

    /// The handles of the list in order (a helper for tests and tools; the original has no such function).
    pub fn list(&self, mgr: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut p = self.mg(mgr).head();
        while p != 0 {
            out.push(p);
            p = self.tk(p).next();
        }
        out
    }
}

/// Which manager a creation site puts its task on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum On {
    /// The root manager `DAT_0033e69c` (the boot thread's, every pass of the main loop).
    Root,
    /// The field manager `DAT_0033e6e4`.
    Field,
    /// Another global manager word (the address of the word).
    Global(u32),
    /// A manager read from an object or a pointer at run time.
    Dynamic,
}

/// One call of the task creator `FUN_00102778(manager, bits, priority, function)` with constant arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Site {
    /// The function that makes the call.
    pub caller: u32,
    pub on: On,
    pub bits: u32,
    pub priority: i32,
    /// The task's function.
    pub function: u32,
}

const fn site(caller: u32, on: On, bits: u32, priority: i32, function: u32) -> Site {
    Site { caller, on, bits, priority, function }
}

/// The frame task `FUN_0014ccf8` and its priority, the highest number of any task: it runs last in every pass of the root manager and
/// ends the game frame with the frame step (`frame::Vsync::step_frame`).
pub const FRAME_TASK: u32 = 0x14ccf8;
pub const FRAME_PRIORITY: i32 = 800_000;

/// Every call of `FUN_00102778` whose bits, priority and function are constants in the executable, in address order (WP-L1-P4).
/// `tests/orig_task_sites.rs` reads each from the call's argument registers in the user's executable. The calls that pass the
/// priority through (the wrappers `FUN_00102808`, `FUN_00102bc8`, `FUN_00102e78`, `FUN_001039e0`, `FUN_00104420`, and the callers
/// `FUN_001394e8`, `FUN_0013a140`, `FUN_00160588`, `FUN_001db278`, `FUN_001dbf60`, `FUN_001ddd10`,
/// `FUN_002248f0`, `FUN_002d58f8`, `FUN_002f5fe8`) are not in the table: their priority is an argument of their own callers.
///
/// What the root manager's tasks are, where known: `0x14c560` the count-down timer (priority 1, first), `0x14dc70` the screen clear
/// (100000) [inferred: it calls the GS clear helpers], `0x167038` the pad read (110000) [inferred: its unit holds the stick scale
/// `FUN_00167198`], the field tasks at 300000 and 330000, `0x2c4958` and `0x2c0e90` (380000, 500010) [not identified], `0x168010`
/// (400000) [not identified], the game-speed task `0x14cb08` and the frame task `0x14ccf8` (800000, last). In the app the same order
/// stands as: the pad (`Update`, before the next vblank's logic), the logic (`FixedUpdate` inside `schedule::Vblank`), then the frame step
/// that ends it (`schedule::FrameClock`).
pub const SITES: &[Site] = &[
    site(0x103678, On::Root, 0, 330_000, 0x1036b8),
    site(0x104e18, On::Field, 0, 300_000, 0x104d58),
    site(0x112168, On::Global(0x35f224), 0, 4, 0x111d18),
    site(0x112168, On::Global(0x35f224), 0, 6, 0x112018),
    site(0x112168, On::Dynamic, 1, 0x5014, 0x112100),
    site(0x13a068, On::Dynamic, 0, 0x19258, 0x139570),
    site(0x13ef00, On::Field, 0, 300_000, 0x13eea8),
    site(0x13f148, On::Dynamic, 0, 300_000, 0x13f0b0),
    site(0x13f1a0, On::Dynamic, 1, 300_000, 0x13f100),
    site(0x13f290, On::Dynamic, 0, 300_000, 0x13f1f8),
    site(0x13f2e8, On::Dynamic, 1, 300_000, 0x13f248),
    site(0x13f3b0, On::Dynamic, 0, 300_000, 0x13f340),
    site(0x140c00, On::Dynamic, 1, 26_000, 0x140be0),
    site(0x14a1e0, On::Root, 0, 0x1c908, 0x14a080),
    site(0x14aa90, On::Root, 0, 330_000, 0x14a988),
    site(0x14c678, On::Root, 0, 1, 0x14c560),
    site(0x14cb50, On::Root, 0, 800_000, 0x14cb08),
    site(0x14cd78, On::Root, 0, FRAME_PRIORITY, FRAME_TASK),
    site(0x14dcd0, On::Root, 0, 100_000, 0x14dc70),
    site(0x15d1a8, On::Dynamic, 1, 120_000, 0x15d0e8),
    // The sub-manager `DAT_00520a30` of 0x160d80 gets 0x1605d8 at 28000 on the straight path; the other branch passes 44000.
    site(0x160d80, On::Global(0x520a30), 0, 28_000, 0x1605d8),
    site(0x160d80, On::Dynamic, 0, 44_000, 0x160650),
    site(0x160d80, On::Dynamic, 0, 71_000, 0x1606a0),
    site(0x166fd0, On::Root, 0, 110_000, 0x167038),
    site(0x1681a0, On::Root, 0, 400_000, 0x168010),
    site(0x187370, On::Global(0x35f224), 0, 0, 0x187250),
    site(0x1b4360, On::Global(0x35f224), 0, 1, 0x1b4b68),
    site(0x1b44c8, On::Global(0x35f224), 0, 1, 0x1b4b68),
    site(0x1b72f0, On::Dynamic, 0, 5, 0x1b7730),
    site(0x1dfff8, On::Dynamic, 1, 0x5014, 0x1dff50),
    site(0x239950, On::Dynamic, 0, 15_000, 0x239ac8),
    site(0x2c0e00, On::Root, 0, 0x7a12a, 0x2c0e90),
    site(0x2c4870, On::Root, 0, 380_000, 0x2c4958),
    site(0x2cc468, On::Field, 0, 300_000, 0x2cc418),
    site(0x2f4930, On::Dynamic, 0, 2000, 0x2f4ba8),
    site(0x2f49e0, On::Dynamic, 0, 2000, 0x2f4ba8),
];

// ---------------------------------------------------------------------------------------------------------------------------------
// WP-L1-P20: coroutine threads, the task runner, game modes and the field mode block.

/// What a unit asks of the rest of the game when it only needs to call original functions by address: the arguments are the low
/// registers, the return value is v0. Every implementor of [`crate::area::ctl::AreaHost`] is one.
pub trait CallHost {
    fn call(&mut self, callee: u32, args: &[u64]) -> u64;
}

impl<T: crate::area::ctl::AreaHost + ?Sized> CallHost for T {
    fn call(&mut self, callee: u32, args: &[u64]) -> u64 {
        crate::area::ctl::AreaHost::call(self, callee, args)
    }
}

/// Size of the header of a coroutine thread block; the thread's stack follows it (`FUN_00102808` allocates `stack + 0x100`).
pub const THREAD_HEADER: usize = 0x100;
/// First thread handle; slot `n` is `THREAD_BASE + n * HANDLE_STRIDE`.
pub const THREAD_BASE: u32 = 0x0b00_0000;

/// The header of a coroutine thread block (0x100 bytes). Bytes below +0xf0 hold the saved registers of the context switch (hardware,
/// not ported); the four fields at the end are the task runner's.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Thread(pub [u8; THREAD_HEADER]);

impl Default for Thread {
    fn default() -> Self {
        Thread([0; THREAD_HEADER])
    }
}

impl Thread {
    /// +0xf0 (a byte): 0 until the function has been started once.
    pub fn first_run(&self) -> u8 {
        self.0[0xf0]
    }
    pub fn set_first_run(&mut self, v: u8) {
        self.0[0xf0] = v
    }
    /// +0xf4: frames the thread still waits (`FUN_00102438` sets it, the runner counts it down), compared as a signed number.
    pub fn wait(&self) -> i32 {
        get(&self.0, 0xf4) as i32
    }
    pub fn set_wait(&mut self, v: i32) {
        put(&mut self.0, 0xf4, v as u32)
    }
    /// +0xf8: time the thread still waits, in frame steps (`FUN_00102460` sets it, the runner reduces it by the frame step).
    pub fn timer(&self) -> f32 {
        f32::from_bits(get(&self.0, 0xf8))
    }
    pub fn set_timer(&mut self, v: f32) {
        put(&mut self.0, 0xf8, v.to_bits())
    }
    /// +0xfc: the stack size the thread was made with.
    pub fn stack(&self) -> u32 {
        get(&self.0, 0xfc)
    }
    pub fn set_stack(&mut self, v: u32) {
        put(&mut self.0, 0xfc, v)
    }
}

/// How a coroutine's run ended when the runner switched into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    /// The function returned: the task is finished (the runner sets its function to 0).
    Returned,
    /// The thread yielded (`FUN_00102438` or `FUN_00102460`, then the switch back).
    Yielded,
}

/// What the runner `FUN_00102340` needs from the game: the function calls and the two context switches (hardware: the original saves
/// and loads registers with `FUN_00105440`, `FUN_001054a8` and `FUN_00105510`; here the host runs the function to its next yield).
/// The finalizer and free events are those of [`TaskHost`].
pub trait RunHost {
    /// A task without a thread: its function is called with the task.
    fn call(&mut self, tasks: &mut Tasks, function: u32, task: u32);
    /// The first run of a coroutine task: the function runs on the thread until it returns or yields.
    fn start(&mut self, tasks: &mut Tasks, function: u32, task: u32) -> Switch;
    /// A later run: the thread continues behind its last yield until it returns or yields again.
    fn resume(&mut self, tasks: &mut Tasks, task: u32) -> Switch;
    fn finalize(&mut self, _tasks: &mut Tasks, _task: u32, _finalizer: u32) {}
    fn free_thread(&mut self, _thread: u32) {}
    fn task_freed(&mut self, _task: u32) {}
    fn manager_freed(&mut self, _manager: u32) {}
}

/// A [`TaskHost`] that runs each task through [`Tasks::run_task`] with the frame step `step`.
pub struct Runner<'a, H: RunHost> {
    pub host: &'a mut H,
    pub step: f32,
}

impl<H: RunHost> TaskHost for Runner<'_, H> {
    fn run(&mut self, tasks: &mut Tasks, task: u32) {
        tasks.run_task(task, self.step, self.host)
    }
    fn finalize(&mut self, tasks: &mut Tasks, task: u32, finalizer: u32) {
        self.host.finalize(tasks, task, finalizer)
    }
    fn free_thread(&mut self, thread: u32) {
        self.host.free_thread(thread)
    }
    fn task_freed(&mut self, task: u32) {
        self.host.task_freed(task)
    }
    fn manager_freed(&mut self, manager: u32) {
        self.host.manager_freed(manager)
    }
}

impl Tasks {
    /// The thread block of a handle.
    pub fn thread(&self, h: u32) -> Option<&Thread> {
        self.threads.get(slot_of(h, THREAD_BASE)?)?.as_ref()
    }
    pub fn thread_mut(&mut self, h: u32) -> Option<&mut Thread> {
        self.threads.get_mut(slot_of(h, THREAD_BASE)?)?.as_mut()
    }
    /// The top of the runner's jump-buffer stack (`FUN_00105618`), 0 when no task is running through the runner.
    ///
    /// orig 0x105618
    pub fn jump_top(&self) -> u32 {
        self.jmp.last().copied().unwrap_or(0)
    }

    /// Creates a coroutine task: a thread block of `stack + 0x100` bytes, then the task (`FUN_00102778`), then the thread's stack size
    /// at +0xfc, the first-run byte at +0xf0, the wait counter at +0xf4 and the wait time at +0xf8 cleared, and the thread in the
    /// task's word +0x34. Returns the task.
    ///
    /// orig 0x102808
    pub fn create_coroutine(&mut self, mgr: u32, bits: u32, priority: i32, function: u32, stack: u32) -> u32 {
        let mut th = Thread::default();
        th.set_stack(stack);
        let h = THREAD_BASE + alloc_slot(&mut self.threads, th) as u32 * HANDLE_STRIDE;
        let task = self.create_task(mgr, bits, priority, function);
        self.tk_mut(task).set_thread(h);
        task
    }

    /// The task's thread waits `frames` runs of the runner (the original then switches back out of the thread).
    ///
    /// orig 0x102438
    pub fn yield_frames(&mut self, task: u32, frames: u32) {
        let th = self.tk(task).thread();
        self.thread_mut(th).unwrap_or_else(|| panic!("task {task:#x} has no thread")).set_wait(frames as i32);
    }

    /// The task's thread waits `time` frame steps less the current step `step` (the original then switches back out of the thread).
    ///
    /// orig 0x102460
    pub fn wait_time(&mut self, task: u32, time: f32, step: f32) {
        let th = self.tk(task).thread();
        self.thread_mut(th).unwrap_or_else(|| panic!("task {task:#x} has no thread")).set_timer(crate::pad::ee::sub(time, step));
    }

    /// Runs one task of a pass. Without a thread its function is called. With one, the runner pushes the owner's jump buffer
    /// (`owner + 0x20`) and then: on the first run the function starts on the thread; later runs count the wait down by one frame
    /// while it is above 0, then reduce the wait time by `step` while it is above 0.0, and otherwise continue the thread. A function
    /// that returned finishes the task (its function word becomes 0). Only the context switch is hardware.
    ///
    /// orig 0x102340
    pub fn run_task(&mut self, task: u32, step: f32, host: &mut impl RunHost) {
        let (function, thread, owner) = {
            let t = self.tk(task);
            (t.function(), t.thread(), t.owner())
        };
        if thread == 0 {
            host.call(self, function, task);
            return;
        }
        self.jmp.push(owner.wrapping_add(0x20));
        let th = self.thread_mut(thread).unwrap_or_else(|| panic!("thread handle {thread:#x} is not live"));
        let switch = if th.first_run() == 0 {
            th.set_first_run(1);
            Some(host.start(self, function, task))
        } else if th.wait() > 0 {
            let w = th.wait();
            th.set_wait(w - 1);
            None
        } else if th.timer() > 0.0 {
            let t = th.timer();
            th.set_timer(crate::pad::ee::sub(t, step));
            None
        } else {
            Some(host.resume(self, task))
        };
        if switch == Some(Switch::Returned)
            && let Some(t) = self.task_mut(task)
        {
            t.set_function(0);
        }
        self.jmp.pop();
    }

    /// The manager of `FUN_001024f8`: an allocator over `region` (`size` bytes, `FUN_00105fd8`, a call of the host), then the manager.
    ///
    /// orig 0x1024f8
    pub fn create_manager_over(&mut self, host: &mut impl CallHost, region: u32, size: u32) -> u32 {
        let allocator = host.call(0x105fd8, &[region as u64, size as u64]) as u32;
        self.create_manager(allocator)
    }
}

/// The game modes: objects of 0x30 bytes in a singly linked list (newest first, head word `0x3990c0`, tail word `0x3990c4`). A mode
/// runs as a coroutine task of the root manager (`FUN_00102070` makes it, `FUN_00101e98` is its body) and owns a task manager for its
/// own tasks. Fields (offsets from the object): +0 the virtual table (slot at +4 enters the mode, +8 exits it), +4 the manager, +0xc the
/// next mode, +0x14 the class bits (two modes with a common bit cannot run together), +0x18 the task priority, +0x1c the state (0 off,
/// 1 running, 3 asked to stop), +0x20 and +0x24 callback addresses, +0x28 the thread's stack size.
pub mod modes {
    use super::*;
    use std::collections::BTreeMap;

    pub const MODE_SIZE: usize = 0x30;
    /// The head and tail words of the list and the thread body's address.
    pub const HEAD_WORD: u32 = 0x3990c0;
    pub const TAIL_WORD: u32 = 0x3990c4;
    pub const BODY: u32 = 0x101e98;

    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct Mode(pub [u8; MODE_SIZE]);

    impl Default for Mode {
        fn default() -> Self {
            Mode([0; MODE_SIZE])
        }
    }

    macro_rules! field {
        ($get:ident, $set:ident, $off:expr, $doc:literal) => {
            #[doc = $doc]
            pub fn $get(&self) -> u32 {
                get(&self.0, $off)
            }
            pub fn $set(&mut self, v: u32) {
                put(&mut self.0, $off, v)
            }
        };
    }

    impl Mode {
        field!(vtable, set_vtable, 0, "+0: the address of the virtual table.");
        field!(manager, set_manager, 4, "+4: the mode's own task manager (0 while off).");
        field!(next, set_next, 0xc, "+0xc: the next mode of the list.");
        field!(mask, set_mask, 0x14, "+0x14: class bits.");
        field!(priority, set_priority, 0x18, "+0x18: priority of the mode's task.");
        field!(state, set_state, 0x1c, "+0x1c: 0 off, 1 running, 3 asked to stop.");
        field!(callback20, set_callback20, 0x20, "+0x20: a callback address, 0 for none.");
        field!(callback24, set_callback24, 0x24, "+0x24: a callback address, 0 for none.");
        field!(stack, set_stack, 0x28, "+0x28: the thread's stack size.");
    }

    /// What the mode thread needs from the game besides the list: the mode's own manager and the two virtual methods of the mode.
    pub trait ModeEnv {
        /// One pass over every task of the manager (`FUN_001029d8`).
        fn pass_all(&mut self, mgr: u32);
        /// Frees every task of the manager (`FUN_00102720`).
        fn kill_all(&mut self, mgr: u32);
        /// The number of tasks of the manager (`FUN_001029f8`, a 64-bit result compared as a signed number).
        fn count(&mut self, mgr: u32) -> i64;
        /// Frees the manager (`FUN_00102740`).
        fn destroy(&mut self, mgr: u32);
        /// Virtual method 1 of the mode (the table slot at +4), called with the mode and the thread's task.
        fn enter(&mut self, mode: u32, task: u32);
        /// Virtual method 2 (the slot at +8).
        fn exit(&mut self, mode: u32, task: u32);
    }

    /// Where the mode thread stands (the resume points of `FUN_00101e98`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub enum Phase {
        /// Waiting for the modes with a common class bit to leave the list, then entering.
        #[default]
        Claim,
        /// The mode runs: its manager is passed every frame while the state is 1, until the state is 3.
        Running,
        /// The mode asked to stop: its manager is passed until it has no tasks left.
        Drain,
    }

    /// What one step of the thread ends with.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Step {
        /// The thread yields with this frame count (`FUN_00102438(task, n)`).
        Yield(u32),
        /// The function returned.
        Done,
    }

    /// The thread body `FUN_00101e98` as a state machine: [`ModeThread::step`] runs it to its next yield.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct ModeThread {
        pub phase: Phase,
    }

    /// The mode objects and the list words.
    #[derive(Clone, Default, Debug)]
    pub struct Modes {
        blocks: BTreeMap<u32, Mode>,
        /// `DAT_003990c0`, the first mode of the list.
        pub head: u32,
        /// `DAT_003990c4`, the last mode of the list.
        pub tail: u32,
    }

    impl Modes {
        pub fn new() -> Modes {
            Modes::default()
        }
        /// Puts a mode object at `id` (an address of the original; any distinct non-zero number in the port).
        pub fn insert(&mut self, id: u32, m: Mode) {
            self.blocks.insert(id, m);
        }
        pub fn mode(&self, id: u32) -> &Mode {
            self.blocks.get(&id).unwrap_or_else(|| panic!("mode {id:#x} is not known"))
        }
        pub fn mode_mut(&mut self, id: u32) -> &mut Mode {
            self.blocks.get_mut(&id).unwrap_or_else(|| panic!("mode {id:#x} is not known"))
        }
        /// The list from the head.
        pub fn list(&self) -> Vec<u32> {
            let mut out = Vec::new();
            let mut p = self.head;
            while p != 0 {
                out.push(p);
                p = self.mode(p).next();
            }
            out
        }

        /// The static init of the list words: only the call `(1, 0xffff)` of the constructor list does anything, it empties the list.
        ///
        /// orig 0x102240
        pub fn static_init(&mut self, a: u64, b: u64) {
            if b == 0xffff && a == 1 {
                self.head = 0;
                self.tail = 0;
            }
        }

        /// The constructor-list wrapper: `static_init(1, 0xffff)`.
        ///
        /// orig 0x102268
        pub fn static_init_wrapper(&mut self) {
            self.static_init(1, 0xffff)
        }

        /// Writes the running modes (state 1), newest first, into `out` and returns how many.
        ///
        /// orig 0x102170
        pub fn running(&self, out: &mut Vec<u32>) -> i32 {
            let mut n = 0;
            for m in self.list() {
                if self.mode(m).state() == 1 {
                    out.push(m);
                    n += 1;
                }
            }
            n
        }

        /// The newest running mode, 0 for none.
        ///
        /// orig 0x102200
        pub fn first_running(&self) -> u32 {
            self.list().into_iter().find(|&m| self.mode(m).state() == 1).unwrap_or(0)
        }

        /// Calls the mode's callback at +0x20 with `arg` when it has one.
        ///
        /// orig 0x102120
        pub fn call_callback20(&self, mode: u32, arg: u64, host: &mut impl CallHost) {
            let cb = self.mode(mode).callback20();
            if cb != 0 {
                host.call(cb, &[arg]);
            }
        }

        /// Calls the mode's callback at +0x24 with `arg` when it has one.
        ///
        /// orig 0x102148
        pub fn call_callback24(&self, mode: u32, arg: u64, host: &mut impl CallHost) {
            let cb = self.mode(mode).callback24();
            if cb != 0 {
                host.call(cb, &[arg]);
            }
        }

        /// Starts a mode: unless it is already asked to stop (state 3), every mode of the list that shares a class bit with it is
        /// asked to stop (state 3, and [`disable`] runs for each) and the mode's thread is made on the root manager `root`.
        /// Returns the thread's task, with the mode in its word +4.
        ///
        /// orig 0x102070
        pub fn start(&mut self, mode: u32, tasks: &mut Tasks, root: u32, host: &mut impl CallHost) -> Option<u32> {
            if self.mode(mode).state() == 3 {
                return None;
            }
            let mask = self.mode(mode).mask();
            for m in self.list() {
                if self.mode(m).mask() & mask != 0 {
                    self.mode_mut(m).set_state(3);
                    disable(host);
                }
            }
            let (prio, stack) = (self.mode(mode).priority() as i32, self.mode(mode).stack());
            let task = tasks.create_coroutine(root, mask, prio, BODY, stack);
            tasks.task_mut(task).expect("the new task").set_arg(mode);
            Some(task)
        }

        /// Whether a mode of the list shares a class bit with `mode`.
        fn conflicts(&self, mode: u32) -> bool {
            let mask = self.mode(mode).mask();
            self.list().into_iter().any(|m| self.mode(m).mask() & mask != 0)
        }

        /// Puts the mode at the head of the list.
        fn push_front(&mut self, mode: u32) {
            if self.head == 0 {
                self.head = mode;
                self.tail = mode;
                self.mode_mut(mode).set_next(0);
            } else {
                let h = self.head;
                self.mode_mut(mode).set_next(h);
                self.head = mode;
            }
        }

        /// Takes the mode out of the list.
        fn unlink(&mut self, mode: u32) {
            let prev = if mode == self.head {
                self.head = self.mode(mode).next();
                0
            } else {
                let mut p = self.head;
                while self.mode(p).next() != mode {
                    p = self.mode(p).next();
                }
                let n = self.mode(mode).next();
                self.mode_mut(p).set_next(n);
                p
            };
            if mode == self.tail {
                self.tail = prev;
            }
            self.mode_mut(mode).set_next(0);
        }
    }

    /// The mode switch-off: soft reset off (`FUN_00167878`, `FUN_0014a220`).
    ///
    /// orig 0x1021c0
    pub fn disable(host: &mut impl CallHost) {
        host.call(0x167878, &[]);
        host.call(0x14a220, &[]);
    }

    /// The mode switch-on: soft reset on (`FUN_001678b0`, `FUN_0014a230`).
    ///
    /// orig 0x1021e0
    pub fn enable(host: &mut impl CallHost) {
        host.call(0x1678b0, &[]);
        host.call(0x14a230, &[]);
    }

    impl ModeThread {
        /// Runs the thread body `FUN_00101e98` of `task` (its mode is the word +4) to its next yield or its end.
        ///
        /// The body: waits (yield 0) while a mode of the list shares a class bit; puts the mode into the list with state 1; calls the
        /// mode's enter method; then every frame passes the mode's manager while the state is 1, until the state is 3; frees all its
        /// tasks and calls the exit method; passes the manager each frame until it has no task left; frees the manager and takes the
        /// mode out of the list (state 0, manager 0).
        ///
        /// orig 0x101e98
        pub fn step(&mut self, task: u32, mode: u32, modes: &mut Modes, env: &mut impl ModeEnv) -> Step {
            if self.phase == Phase::Claim {
                if modes.conflicts(mode) {
                    return Step::Yield(0);
                }
                modes.mode_mut(mode).set_state(1);
                modes.push_front(mode);
                env.enter(mode, task);
                self.phase = Phase::Running;
            }
            let mgr = modes.mode(mode).manager();
            if self.phase == Phase::Running {
                let s = modes.mode(mode).state();
                if s != 3 {
                    if s == 1 {
                        env.pass_all(mgr);
                    }
                    return Step::Yield(0);
                }
                env.kill_all(mgr);
                env.exit(mode, task);
                self.phase = Phase::Drain;
            }
            if env.count(mgr) > 0 {
                env.pass_all(mgr);
                return Step::Yield(0);
            }
            env.destroy(mgr);
            let m = modes.mode_mut(mode);
            m.set_manager(0);
            m.set_state(0);
            modes.unlink(mode);
            Step::Done
        }
    }
}

/// The field mode: the field's task manager and the start and exit functions of the field mode block (`0x33e6e0..0x33e710`).
pub mod field {
    use super::*;
    use crate::area::ctl::{AreaCtl, AreaHost};

    /// Addresses of the words of the block.
    pub const BLOCK: u32 = 0x33e6e0;
    pub const START_WORD: u32 = 0x33e700;
    pub const EXIT_WORD: u32 = 0x33e704;
    pub const NEXT_WORD: u32 = 0x33e70c;
    pub const SAVE: u32 = 0x33e860;
    /// The code addresses the table init stores: the start function and the exit function.
    pub const START_FN: u32 = 0x102a60;
    pub const EXIT_FN: u32 = 0x102b50;

    /// The address of the field mode block.
    ///
    /// orig 0x102c30
    pub const fn mode_block() -> u32 {
        BLOCK
    }

    /// Stores the start function, the exit function and a cleared "next" word into the block's table.
    ///
    /// orig 0x102b98
    pub fn table_init(host: &mut impl AreaHost) {
        host.write32(EXIT_WORD, EXIT_FN);
        host.write32(START_WORD, START_FN);
        host.write32(NEXT_WORD, 0);
    }

    /// The three audio resets of the field mode (`FUN_002c10b0`, `FUN_00136b90`, `FUN_00136c40`).
    ///
    /// orig 0x102b28
    pub fn audio_reset(host: &mut impl CallHost) {
        host.call(0x2c10b0, &[]);
        host.call(0x136b90, &[]);
        host.call(0x136c40, &[]);
    }

    /// Starts the field mode: pad button set 1, the field's task manager (an allocator over 0x3990d0, 0x10000 bytes; stored in the
    /// control record, `0x33e6e4`), the area-state clear, the audio reset, the field task and the other unit starts, then the area
    /// thread: the area given by the save block's place bytes, or the Gummi thread when the "next" word is set (and that word is
    /// cleared). The previous place's world byte becomes 0xff.
    ///
    /// orig 0x102a60
    pub fn start(host: &mut impl AreaHost, tasks: &mut Tasks, ctl: &mut AreaCtl) {
        host.call(0x167908, &[1]);
        ctl.task_mgr = tasks.create_manager_over(host, 0x3990d0, 0x10000);
        host.call(0x104f88, &[]);
        audio_reset(host);
        host.call(0x149f70, &[]);
        host.call(0x1078f0, &[]);
        host.call(0x168928, &[]);
        let next = host.read32(NEXT_WORD);
        if next == 0 {
            let w = host.read8(SAVE + 12) as u64;
            let a = host.read8(SAVE + 13) as u64;
            let e = host.read8(SAVE + 14) as u64;
            host.call(0x104ca8, &[w, a, e]);
        } else {
            host.call(0x104c68, &[next as u64]);
            host.write32(NEXT_WORD, 0);
        }
        host.call(0x223278, &[]);
        host.call(0x13ee88, &[]);
        host.call(0x13ef68, &[1]);
        ctl.prev[0] = 0xff;
    }

    /// The field mode's exit task: the field music and area data are released (`FUN_00136be0`, `FUN_00136c90`, `FUN_00104ce0`,
    /// `FUN_002c10b0`), then the task waits one frame.
    ///
    /// orig 0x102b50
    pub fn exit(host: &mut impl CallHost, tasks: &mut Tasks, task: u32) {
        host.call(0x136be0, &[]);
        host.call(0x136c90, &[]);
        host.call(0x104ce0, &[]);
        host.call(0x2c10b0, &[]);
        tasks.yield_frames(task, 1);
    }

    /// Makes a coroutine task on the field manager.
    ///
    /// orig 0x102bf8
    pub fn create_coroutine(tasks: &mut Tasks, ctl: &AreaCtl, bits: u32, priority: i32, function: u32, stack: u32) -> u32 {
        tasks.create_coroutine(ctl.task_mgr, bits, priority, function, stack)
    }
}

/// The camp and pause poll task.
pub mod camp {
    use crate::area::ctl::{self, AreaCtl, AreaHost};
    use crate::frame::Vsync;

    /// The task body, called once per pass of the root manager. It returns at once when the stop flag (`0x356398`) is set. While the
    /// camp menu is open it keeps the game stopped: the first frame it sees the menu it notes whether the timer was already paused
    /// (`FUN_0014c780`), calls `FUN_0023e860(1)`, pauses the timer twice with bit 1 unless it was paused, and calls
    /// `FUN_002f5f08(1)`; every frame of the menu it steps the pad and the frame (`FUN_001674f8`, the frame step `FUN_0014cdd0`)
    /// itself. When the menu closes it sets the pause flag (`0x356399`) and steps frames until the pause is released
    /// (`FUN_002fee00(1) == 2`), then clears the flag, lifts the timer pause if it had set it, calls `FUN_002f5f08(0)` and
    /// `FUN_0023e8b0`, and returns. With no menu and no pause pending it returns without stepping.
    ///
    /// orig 0x14a080
    pub fn poll_task(host: &mut impl AreaHost, ctl: &mut AreaCtl, v: &mut Vsync) {
        let mut menu = false;
        let mut active = false;
        let mut was_paused = 0u64;
        loop {
            if ctl::poll_stop_set(ctl) != 0 {
                return;
            }
            if ctl::camp_open(host) {
                menu = true;
            }
            if ctl::camp_idle(host) {
                menu = false;
            }
            if !active && menu {
                was_paused = v.paused() as u64;
                host.call(0x23e860, &[1]);
                if was_paused == 0 {
                    v.pause_on(1);
                    v.pause_on(1);
                }
                active = true;
                host.call(0x2f5f08, &[1]);
            }
            let mut closed = !active;
            if active {
                if menu {
                    ctl::pause_step(host);
                    continue;
                }
                ctl.poll_paused = 1;
                active = false;
                closed = true;
            }
            if closed {
                if menu {
                    ctl::pause_step(host);
                    continue;
                }
                if ctl.poll_paused == 0 {
                    return;
                }
                loop {
                    if ctl::pause_released(host) {
                        ctl.poll_paused = 0;
                        if was_paused == 0 {
                            v.pause_off(1);
                        }
                        host.call(0x2f5f08, &[0]);
                        host.call(0x23e8b0, &[]);
                        return;
                    }
                    ctl::pause_step(host);
                    if ctl.poll_paused == 0 {
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(t: &Tasks, m: u32) -> Vec<(i32, u32)> {
        t.list(m).iter().map(|&h| (t.task(h).unwrap().priority(), t.task(h).unwrap().bits())).collect()
    }

    #[test]
    fn insert_keeps_priority_order_and_creation_order_among_equals() {
        let mut t = Tasks::new();
        let m = t.create_manager(0x1234);
        for (bits, p) in [(1, 5), (2, 3), (3, 5), (4, 9), (5, 3), (6, -2)] {
            t.create_task(m, bits, p, 0x100);
        }
        assert_eq!(names(&t, m), vec![(-2, 6), (3, 2), (3, 5), (5, 1), (5, 3), (9, 4)]);
        assert_eq!(t.count(m), 6);
        let l = t.list(m);
        assert_eq!(t.manager(m).unwrap().tail(), *l.last().unwrap());
        assert_eq!(t.task(l[3]).unwrap().prev(), l[2]);
    }

    struct Finishes(Vec<u32>);

    impl TaskHost for Finishes {
        fn run(&mut self, tasks: &mut Tasks, task: u32) {
            self.0.push(tasks.task(task).unwrap().bits());
            if tasks.task(task).unwrap().bits() % 2 == 0 {
                tasks.task_mut(task).unwrap().set_function(0);
            }
        }
    }

    #[test]
    fn a_pass_runs_matching_tasks_in_order_and_frees_the_finished_ones() {
        let mut t = Tasks::new();
        let m = t.create_manager(0);
        for b in 1..=6u32 {
            t.create_task(m, b, b as i32, 0x100);
        }
        let mut h = Finishes(Vec::new());
        // Only odd bits match (mask 1, value 1): tasks 1, 3, 5.
        t.pass(m, 1, 1, &mut h);
        assert_eq!(h.0, vec![1, 3, 5]);
        assert_eq!(t.count(m), 6);
        let mut h = Finishes(Vec::new());
        t.pass_all(m, &mut h);
        assert_eq!(h.0, vec![1, 2, 3, 4, 5, 6]);
        // The even ones finished and are gone.
        assert_eq!(names(&t, m), vec![(1, 1), (3, 3), (5, 5)]);
        assert_eq!(t.running, 0);
        assert!(t.contains_function(m, 0x100));
        assert!(!t.contains_function(m, 0));
    }

    #[test]
    fn kill_by_bits_and_destroy_free_everything() {
        let mut t = Tasks::new();
        let m = t.create_manager(0);
        for b in 0..8u32 {
            t.create_task(m, b, 1, 7);
        }
        t.kill(m, 4, 4, &mut NoHost);
        assert_eq!(t.count(m), 4);
        t.destroy_manager(m, &mut NoHost);
        assert!(t.live_tasks().is_empty() && t.live_managers().is_empty());
    }

    #[test]
    fn a_task_created_in_front_of_the_next_one_runs_next() {
        struct Spawner(bool, Vec<u32>);
        impl TaskHost for Spawner {
            fn run(&mut self, tasks: &mut Tasks, task: u32) {
                let b = tasks.task(task).unwrap().bits();
                self.1.push(b);
                if !self.0 {
                    self.0 = true;
                    // Priority between the running task (1) and the next one (3): runs before the next one.
                    tasks.create_task(tasks.task(task).unwrap().owner(), 99, 2, 1);
                }
            }
        }
        let mut t = Tasks::new();
        let m = t.create_manager(0);
        t.create_task(m, 1, 1, 1);
        t.create_task(m, 3, 3, 1);
        let mut h = Spawner(false, Vec::new());
        t.pass_all(m, &mut h);
        assert_eq!(h.1, vec![1, 99, 3]);
    }
}
