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
//! What is NOT here: running one task (`FUN_00102340`, with the coroutine threads of 0x102340..0x102460 and 0x105440..0x105618), the
//! creation of a coroutine task (`FUN_00102808`) and the allocator. The pass hands each task to a [`TaskHost`], which stands for
//! `FUN_00102340`; the frame-order package wires a real host. Nothing in the app uses this module yet.

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
