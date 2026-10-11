//! Running a script the way the game does: an object gets one persistent thread (event 1) plus a mode
//! machine that starts one more thread per named "mode". Events from the game (damage, event start and
//! end...) run short handlers to their first `yield`.
//!
//! The mode machine here is a port of the game's per-object mode manager (FUN_001869b8 and the
//! functions around it). Parts that depend on game state go through [`Call`] (see `ModeFrozen`,
//! `ModeInterrupted`, `ModeMayStart`). Scripts can also start child threads (host 0.9 and 1.82, FUN_0016c8d8),
//! send named messages to other objects' modes (1.90 and 1.241, FUN_00187170 and FUN_00187190) and queue
//! a mode change on another object (6.58, FUN_00186f50). The cross-object parts need the host: it
//! receives the request as a [`Call`] and hands the message to every runtime with [`Runtime::receive_message`].

use crate::exe::HostTable;
use crate::host::{Arg, Call, Host, ObjectKey, Value, Vec4};
use crate::math::{self, VecAccess};
use crate::names;
use crate::program::Program;
use crate::vm::{self, Env, Mem, Outside, Shared, Stop, Thread, VmError};

/// A mode record in script memory: name pointer, priority, flags, then nine routine words.
const RECORD_SIZE: u32 = 32;
const RECORD_COUNT: u32 = 0x80;
const QUEUE_LEN: usize = 16;
/// Modes with a priority below this can be interrupted (FUN_001869b8).
const INTERRUPT_PRIORITY: i32 = 300;
const MAX_DEPTH: u32 = 8;
/// The mode queue entry flag "apply to another object" (FUN_00186f50).
const QUEUE_REMOTE: u32 = 1;
/// One game frame, the unit message delays count down in (the value behind host 0.3).
const FRAME_TIME: f32 = 1.0;

// Hook slots of a mode record (index into the u16 routine array that starts at +8).
const HOOK_MAIN: u32 = 0;
const HOOK_EXIT: u32 = 1;
const HOOK_EVENT_BEGIN: u32 = 2;
const HOOK_EVENT_END: u32 = 3;
const HOOK_DAMAGE: u32 = 4;
/// Called when a message addressed to the mode's name arrives (FUN_001871d0 reads +0x14).
const HOOK_MESSAGE: u32 = 6;
const HOOK_ENTER: u32 = 7;
/// Run by the enemy clash handler (0x1b2478 -> FUN_00187130 -> FUN_00187040 with 7, which reads the
/// halfword at record +10 + 2 * 7, slot 8) with (context, own volume, other volume).
const HOOK_CLASH: u32 = 8;
/// The script event the enemy clash handler delivers after the clash hook (0x1b2478: FUN_0016c620 with 0x13).
pub const EVENT_CLASH: u32 = 0x13;

/// A thread of an object's script together with the bookkeeping the game keeps in its thread object
/// (FUN_0016c230): the tag children are killed by, the cleanup routine run when the thread is destroyed,
/// the finished flag and the child threads started by `0.9` and `1.82`.
struct Node {
    th: Thread,
    /// Opaque non-zero id handed to scripts as the thread's handle.
    handle: u32,
    /// Offset +0x20: children with all these tag bits are killed when a new one is started with the same bits.
    tag: u32,
    /// Offset +0x24: routine (word offset) run once when the thread is destroyed, 0 for none.
    cleanup: u32,
    finished: bool,
    children: Vec<Node>,
}

/// Something a script asked for while its thread was running. The thread tree is borrowed by the run,
/// so these are carried out right after the run returns (`Core::drain`).
enum Request {
    /// Host 0.9 / 1.82: start a child of the thread with handle `root`.
    Spawn { root: u32, handle: u32, tag: u32, pc: u32, cleanup: u32, args: Vec<u32> },
    /// Host 1.112: replace the object's action thread (FUN_001764a8).
    SetAction { handle: u32, pc: u32, args: Vec<u32> },
    /// Host 1.214: drop the action thread (FUN_001764a8 with 0).
    StopAction,
    /// Host 2.20: start a detached request thread (FUN_001a4738).
    Detached { handle: u32, pc: u32, args: Vec<u32> },
}

/// Base of the script heap served to host 0.47 (`alloc`). The game hands out real heap memory; here a
/// bump allocator inside the runtime answers reads and writes before the host is asked. Clear of the
/// script regions (`vm::CODE_ADDR` and up).
pub const HEAP_BASE: u32 = 0x0900_0000;
const HEAP_SIZE: usize = 0x1_0000;

/// A message waiting for its delay to run out (offsets +0xa0, +0xa4, +0xa8 of the mode manager).
struct PendingMessage {
    name: String,
    sender: u32,
    time: f32,
}

#[derive(Default)]
struct ModeMachine {
    /// Script address of the context structure passed to mode routines (`+0x88` in the game).
    ctx: u32,
    /// Script address of the mode record table (`+0x8c`).
    table: u32,
    queue: [(u32, u32); QUEUE_LEN],
    write: usize,
    read: usize,
    /// Record of the running mode (`+0x90`), 0 if none.
    cur_rec: u32,
    /// A mode thread exists (`+0x94 != 0`).
    cur_alive: bool,
    /// The thread's storage. Empty while it runs.
    cur_thread: Option<Node>,
    /// Bumped whenever the running mode changes, to notice a switch made by the mode's own thread.
    generation: u64,
    /// Priority of the running mode (`+0x98`); 0 when idle.
    priority: i32,
    /// Bit 0: switching disabled, bit 1: a mode was started, bit 2: ask the host before running.
    flags: u32,
    /// The delayed message of the object, one slot (`+0xa0`).
    pending: Option<PendingMessage>,
}

/// State shared by everything that runs inside one object's script.
pub struct Core {
    /// The game object this script drives.
    pub obj: u32,
    modes: ModeMachine,
    /// Signatures read from the executable for functions this crate does not know.
    pub exe_table: Option<HostTable>,
    /// Signatures worked out from the scripts themselves for functions the executable's table does not
    /// have (see `signatures.rs`). Consulted last.
    pub inferred: std::collections::HashMap<(u16, u16), (usize, bool)>,
    /// Errors raised by threads (a thread that errors ends, like in the game).
    pub errors: Vec<(usize, VmError)>,
    depth: u32,
    spawned: bool,
    next_handle: u32,
    /// Handle of the object's persistent thread (event 1).
    main_handle: u32,
    /// The thread new children attach to: the game's "current root" (DAT_0035b9a0).
    cur_root: u32,
    requests: Vec<Request>,
    /// The object's action thread (offset +0xaec of the game object), started by host 1.112.
    action: Option<Node>,
    /// Detached request threads started by host 2.20 (FUN_001a4738): the game keeps them in a global
    /// list at 0x1c95518 and the task body FUN_001a4428 runs each one once per frame (FUN_0016c680),
    /// destroying it when it has ended. Here they belong to the object whose script started them.
    requests_threads: Vec<Node>,
    /// Mode threads that were stopped; destroyed when the mode machine's pass ends (FUN_0016ca48).
    deferred: Vec<Node>,
    /// The script heap behind host 0.47 / 0.48 (see [`HEAP_BASE`]); `heap_next` is the bump pointer.
    heap: Vec<u8>,
    heap_next: usize,
    /// When set, the name of every mode that starts is appended (modes that switch again within the
    /// frame show up here but not in [`Runtime::current_mode`] sampled once per frame).
    pub mode_log: Option<Vec<String>>,
}

pub struct Runtime {
    pub prog: Program,
    code: Vec<u8>,
    pub shared: Shared,
    pub core: Core,
    main: Option<Node>,
}

struct CoreEnv<'a> {
    core: &'a mut Core,
    host: &'a mut dyn Host,
    code: &'a [u8],
}

fn outside(o: Outside) -> VmError {
    match o {
        Outside::Ext(a) | Outside::Fault(a) => VmError::BadAddress(a),
    }
}

fn read32(mem: &mut Mem<'_>, host: &mut dyn Host, addr: u32) -> Result<u32, VmError> {
    match mem.read_u32(addr) {
        Ok(v) => Ok(v),
        Err(Outside::Ext(a)) => {
            let mut b = [0; 4];
            if host.read_mem(a, &mut b) { Ok(u32::from_le_bytes(b)) } else { Err(VmError::BadAddress(a)) }
        }
        Err(Outside::Fault(a)) => Err(VmError::BadAddress(a)),
    }
}

fn read_vec(mem: &mut Mem<'_>, host: &mut dyn Host, addr: u32) -> Result<Vec4, VmError> {
    match mem.read_vec4(addr) {
        Ok(v) => Ok(v),
        Err(Outside::Ext(a)) => {
            let mut b = [0; 16];
            if host.read_mem(a, &mut b) {
                Ok(std::array::from_fn(|i| f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap())))
            } else {
                Err(VmError::BadAddress(a))
            }
        }
        Err(Outside::Fault(a)) => Err(VmError::BadAddress(a)),
    }
}

/// The host with the script heap (host 0.47) in front of it: reads and writes of heap addresses never
/// reach the host. Used wherever the runtime itself reads script-owned memory (contexts, vectors).
struct HeapHost<'a> {
    heap: &'a mut Vec<u8>,
    host: &'a mut dyn Host,
}

impl Host for HeapHost<'_> {
    fn call(&mut self, c: &Call) -> Value {
        self.host.call(c)
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        match heap_span(self.heap.len(), addr, out.len()) {
            Some(r) => {
                out.copy_from_slice(&self.heap[r]);
                true
            }
            None => self.host.read_mem(addr, out),
        }
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        match heap_span(self.heap.len(), addr, data.len()) {
            Some(r) => {
                self.heap[r].copy_from_slice(data);
                true
            }
            None => self.host.write_mem(addr, data),
        }
    }
}

fn heap_span(heap_len: usize, addr: u32, len: usize) -> Option<std::ops::Range<usize>> {
    let o = addr.checked_sub(HEAP_BASE)? as usize;
    (o + len <= heap_len).then_some(o..o + len)
}

fn fl(v: u32) -> f32 {
    f32::from_bits(v)
}

/// Signature of a function this crate has named.
fn named(group: u16, index: u16) -> Option<(usize, bool)> {
    names::lookup(group, index).map(|f| (f.args as usize, f.ret))
}

fn convert(v: Value, mem: &mut Mem<'_>) -> u32 {
    match v {
        Value::None => 0,
        Value::Int(i) => i as u32,
        Value::Float(f) => f.to_bits(),
        Value::Bool(b) => b as u32,
        Value::Handle(h) => h,
        Value::Vec(v) => mem.sh.put_vec(v),
    }
}

fn strcmp_eq(mem: &mut Mem<'_>, a: u32, b: u32) -> bool {
    match (mem.read_cstr(a), mem.read_cstr(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// The random delay a broadcast message gets for one receiver (FUN_00187250): the game's second
/// generator `seed = seed * 0x10dcd` (state at 0x3716a0, separate from the script generator behind
/// host 0.16..0.18) gives a fraction of the delay. Returns the delay to pass to [`Runtime::receive_message`];
/// a delay of zero or less is delivered at once and does not use the generator.
pub fn message_delay(seed: &mut u32, delay: f32) -> f32 {
    if delay <= 0.0 {
        return delay;
    }
    *seed = seed.wrapping_mul(0x10dcd);
    *seed as f32 * 2.328_306_4e-10 * delay
}

impl ModeMachine {
    fn rec_field(mem: &mut Mem<'_>, rec: u32, off: u32) -> Result<u32, VmError> {
        mem.read_u32(rec + off).map_err(outside)
    }

    fn rec_u16(mem: &mut Mem<'_>, rec: u32, off: u32) -> Result<u32, VmError> {
        let mut b = [0u8; 2];
        mem.read(rec + off, &mut b).map_err(outside)?;
        Ok(u16::from_le_bytes(b) as u32)
    }

    /// Looks a mode up by name in the record table (FUN_001867d0).
    fn find(&self, mem: &mut Mem<'_>, name: u32) -> Result<Option<u32>, VmError> {
        if self.table == 0 {
            return Ok(None);
        }
        for i in 0..RECORD_COUNT {
            let rec = self.table + i * RECORD_SIZE;
            let n = Self::rec_field(mem, rec, 0)?;
            if n != 0 && strcmp_eq(mem, name, n) {
                return Ok(Some(rec));
            }
        }
        Ok(None)
    }

    /// As [`find`](Self::find) for a name that is not in script memory (messages from other objects).
    fn find_str(&self, mem: &mut Mem<'_>, name: &str) -> Result<Option<u32>, VmError> {
        if self.table == 0 {
            return Ok(None);
        }
        for i in 0..RECORD_COUNT {
            let rec = self.table + i * RECORD_SIZE;
            let n = Self::rec_field(mem, rec, 0)?;
            if n != 0 && mem.read_cstr(n).is_ok_and(|s| s == name) {
                return Ok(Some(rec));
            }
        }
        Ok(None)
    }

    fn hook_pc(mem: &mut Mem<'_>, rec: u32, slot: u32) -> Result<u32, VmError> {
        Self::rec_u16(mem, rec, 8 + 2 * slot)
    }
}

impl Core {
    pub fn new() -> Core {
        Core {
            obj: 0,
            modes: ModeMachine::default(),
            exe_table: None,
            inferred: Default::default(),
            errors: Vec::new(),
            depth: 0,
            spawned: false,
            next_handle: 0x1000,
            main_handle: 0,
            cur_root: 0,
            requests: Vec::new(),
            action: None,
            requests_threads: Vec::new(),
            deferred: Vec::new(),
            heap: Vec::new(),
            heap_next: 0,
            mode_log: None,
        }
    }

    /// Host 0.47: `size` bytes of zeroed heap, or 0 when the heap is exhausted (the game's allocator
    /// returns 0 too when it fails; scripts do not check). Never reused: 0.48 (`free`) only records.
    fn heap_alloc(&mut self, size: u32) -> u32 {
        if self.heap.is_empty() {
            self.heap = vec![0; HEAP_SIZE];
        }
        let size = ((size as usize) + 15) & !15;
        if self.heap_next + size > self.heap.len() {
            return 0;
        }
        let at = self.heap_next;
        self.heap_next += size;
        HEAP_BASE + at as u32
    }

    fn heap_range(&self, addr: u32, len: usize) -> Option<std::ops::Range<usize>> {
        heap_span(self.heap.len(), addr, len)
    }

    fn alloc_handle(&mut self) -> u32 {
        self.next_handle += 1;
        self.next_handle
    }

    fn new_node(&mut self, prog: &Program, sh: &mut Shared, handle: u32, pc: u32, args: &[u32]) -> Node {
        let mut th = Thread::new(prog, sh);
        th.start(pc as usize, args);
        Node { th, handle, tag: 0, cleanup: 0, finished: false, children: Vec::new() }
    }

    /// Runs a routine as a throw-away thread until its first yield (FUN_0016c550). Threads it starts
    /// become children of the object's persistent thread.
    fn run_temp(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, pc: u32, args: &[u32]) {
        if pc == 0 || !self.spawned || self.depth >= MAX_DEPTH {
            return;
        }
        let mut th = Thread::new(prog, sh);
        th.start(pc as usize, args);
        let saved = std::mem::replace(&mut self.cur_root, self.main_handle);
        self.run_thread(prog, code, sh, host, &mut th);
        self.cur_root = saved;
    }

    /// Runs a persistent thread one slice. Errors end the thread.
    fn run_thread(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, th: &mut Thread) -> Stop {
        self.depth += 1;
        let r = {
            let mut env = CoreEnv { core: &mut *self, host, code };
            vm::run_with_code(prog, code, sh, th, &mut env)
        };
        self.depth -= 1;
        match r {
            Ok(s) => s,
            Err(e) => {
                self.errors.push((th.pc, e));
                Stop::Exit
            }
        }
    }

    // ---- thread tree ----------------------------------------------------------------------

    /// Runs a thread and then its children (FUN_0016c680). `parent` is the thread the run was started
    /// from; threads the script starts attach to it (or to the thread itself at the top). Returns true
    /// when the thread has ended and has no children left.
    fn run_node(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, node: &mut Node, parent: Option<u32>) -> bool {
        let root = parent.unwrap_or(node.handle);
        self.drain(prog, code, sh, host, node);
        if !node.finished {
            let saved = std::mem::replace(&mut self.cur_root, root);
            let stop = self.run_thread(prog, code, sh, host, &mut node.th);
            self.cur_root = saved;
            if stop != Stop::Yield {
                node.finished = true;
            }
        }
        self.drain(prog, code, sh, host, node);
        let mut i = 0;
        while i < node.children.len() {
            // The game reads the next sibling before running the child: threads appended during the
            // child's run only run this frame when the child was not the last one.
            let has_next = i + 1 < node.children.len();
            let done = self.run_node(prog, code, sh, host, &mut node.children[i], Some(node.handle));
            self.drain(prog, code, sh, host, node);
            if done {
                let dead = node.children.remove(i);
                self.destroy(prog, code, sh, host, dead);
            } else {
                i += 1;
            }
            if !has_next {
                break;
            }
        }
        node.finished && node.children.is_empty()
    }

    /// Destroys a thread: its cleanup routine runs once (up to its first yield), then its children are
    /// destroyed the same way (FUN_0016c3f0).
    fn destroy(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, mut node: Node) {
        if node.cleanup != 0 {
            node.th.start(node.cleanup as usize, &[]);
            let saved = std::mem::replace(&mut self.cur_root, 0);
            self.run_thread(prog, code, sh, host, &mut node.th);
            self.cur_root = saved;
        }
        for c in std::mem::take(&mut node.children) {
            self.destroy(prog, code, sh, host, c);
        }
    }

    /// Carries out the requests addressed to `node` (children started by scripts, FUN_0016c8d8) and,
    /// for the main thread, the object-wide ones.
    fn drain(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, node: &mut Node) {
        let mut i = 0;
        while i < self.requests.len() {
            let mine = match &self.requests[i] {
                Request::Spawn { root, .. } => *root == node.handle,
                _ => false,
            };
            if !mine {
                i += 1;
                continue;
            }
            let Request::Spawn { handle, tag, pc, cleanup, args, .. } = self.requests.remove(i) else { continue };
            if tag != 0 {
                // Children whose tag holds all the bits of the new tag are killed first (FUN_0016c928).
                let mut k = 0;
                while k < node.children.len() {
                    if node.children[k].tag & tag == tag {
                        let dead = node.children.remove(k);
                        self.destroy(prog, code, sh, host, dead);
                    } else {
                        k += 1;
                    }
                }
            }
            let mut child = self.new_node(prog, sh, handle, pc, &args);
            child.tag = tag;
            child.cleanup = cleanup;
            node.children.push(child);
        }
    }

    /// Replaces or drops the action thread as requested by scripts (they cannot do it while it runs).
    fn apply_action_requests(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host) {
        let mut i = 0;
        while i < self.requests.len() {
            match &self.requests[i] {
                Request::SetAction { .. } | Request::StopAction => {}
                Request::Detached { .. } => {
                    let Request::Detached { handle, pc, args } = self.requests.remove(i) else { continue };
                    let node = self.new_node(prog, sh, handle, pc, &args);
                    self.requests_threads.push(node);
                    continue;
                }
                _ => {
                    i += 1;
                    continue;
                }
            }
            let req = self.requests.remove(i);
            if let Some(old) = self.action.take() {
                self.destroy(prog, code, sh, host, old);
            }
            if let Request::SetAction { handle, pc, args } = req {
                let node = self.new_node(prog, sh, handle, pc, &args);
                self.action = Some(node);
            }
        }
    }

    /// Runs every detached request thread one slice and destroys the ones that have ended
    /// (FUN_001a4428: `for node in list { if FUN_0016c680(node.thread) == 1 { destroy } }`; the game
    /// skips the whole pass while the pause flag 0x20 of 0x35d268 is set).
    fn run_requests(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host) {
        let mut i = 0;
        while i < self.requests_threads.len() {
            let mut node = self.requests_threads.remove(i);
            let done = self.run_node(prog, code, sh, host, &mut node, None);
            if done {
                self.destroy(prog, code, sh, host, node);
            } else {
                self.requests_threads.insert(i, node);
                i += 1;
            }
        }
    }

    // ---- mode machine --------------------------------------------------------------------

    /// Ends the running mode: exit hook, thread handed to the deferred list (FUN_001868a8).
    fn stop_mode(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host) {
        if !self.modes.cur_alive {
            return;
        }
        let rec = self.modes.cur_rec;
        let ctx = self.modes.ctx;
        let hook = self.record_hook(code, sh, rec, HOOK_EXIT);
        self.modes.cur_alive = false;
        self.modes.cur_rec = 0;
        if let Some(n) = self.modes.cur_thread.take() {
            self.deferred.push(n);
        }
        self.modes.generation += 1;
        self.run_temp(prog, code, sh, host, hook, &[ctx]);
    }

    fn record_hook(&mut self, code: &[u8], sh: &mut Shared, rec: u32, slot: u32) -> u32 {
        vm::with_shared(code, sh, |mem| ModeMachine::hook_pc(mem, rec, slot).unwrap_or(0))
    }

    /// Stops the running mode, clears the queue and priority (FUN_00186ca8). Does nothing while switching is disabled.
    pub fn reset_modes(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host) {
        if self.modes.flags & 1 == 0 {
            self.stop_mode(prog, code, sh, host);
            self.modes.priority = 0;
            self.modes.write = 0;
            self.modes.read = 0;
        }
    }

    /// Starts a mode (FUN_00186cf0): thread at the main routine, flags from the record, enter hook.
    fn start_mode(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, rec: u32) {
        self.stop_mode(prog, code, sh, host);
        let (main, prio, fl_hi, enter) = vm::with_shared(code, sh, |mem| {
            (
                ModeMachine::hook_pc(mem, rec, HOOK_MAIN).unwrap_or(0),
                ModeMachine::rec_u16(mem, rec, 4).unwrap_or(0),
                ModeMachine::rec_u16(mem, rec, 6).unwrap_or(0),
                ModeMachine::hook_pc(mem, rec, HOOK_ENTER).unwrap_or(0),
            )
        });
        let ctx = self.modes.ctx;
        if self.mode_log.is_some() {
            let name = vm::with_shared(code, sh, |mem| ModeMachine::rec_field(mem, rec, 0).ok().and_then(|n| mem.read_cstr(n).ok()));
            if let (Some(log), Some(name)) = (self.mode_log.as_mut(), name) {
                log.push(name);
            }
        }
        let handle = self.alloc_handle();
        let node = self.new_node(prog, sh, handle, main, &[ctx]);
        self.modes.cur_thread = Some(node);
        self.modes.cur_alive = true;
        self.modes.cur_rec = rec;
        self.modes.generation += 1;
        self.modes.priority = prio as i32;
        let mut f = 0;
        if fl_hi & 2 != 0 {
            f = 1;
        }
        if (fl_hi ^ 1) & 1 != 0 {
            f |= 4;
        }
        self.modes.flags = f | 2;
        self.run_temp(prog, code, sh, host, enter, &[ctx]);
    }

    /// Switches to the named mode now (FUN_00186db0).
    pub fn set_mode(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, name: u32) -> Result<(), VmError> {
        if self.modes.flags & 1 != 0 || self.modes.table == 0 {
            return Ok(());
        }
        let rec = vm::with_shared(code, sh, |mem| self.modes.find(mem, name))?;
        self.switch_to(prog, code, sh, host, rec);
        Ok(())
    }

    /// As [`set_mode`](Self::set_mode) for a name held by the host (the remote side of a queued mode change).
    pub fn set_mode_named(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, name: &str) -> Result<(), VmError> {
        if self.modes.flags & 1 != 0 || self.modes.table == 0 {
            return Ok(());
        }
        let rec = vm::with_shared(code, sh, |mem| self.modes.find_str(mem, name))?;
        self.switch_to(prog, code, sh, host, rec);
        Ok(())
    }

    fn switch_to(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, rec: Option<u32>) {
        if let Some(rec) = rec {
            self.reset_modes(prog, code, sh, host);
            self.start_mode(prog, code, sh, host, rec);
        }
    }

    /// Switches to the named mode only when its priority is at least that of the running one (FUN_00186e70).
    pub fn set_mode_priority(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, name: u32) -> Result<(), VmError> {
        if self.modes.flags & 1 != 0 || self.modes.table == 0 {
            return Ok(());
        }
        let found = vm::with_shared(code, sh, |mem| -> Result<Option<(u32, i32)>, VmError> {
            Ok(match self.modes.find(mem, name)? {
                Some(rec) => Some((rec, ModeMachine::rec_u16(mem, rec, 4)? as i32)),
                None => None,
            })
        })?;
        if let Some((rec, prio)) = found
            && self.modes.priority <= prio
        {
            self.switch_to(prog, code, sh, host, Some(rec));
        }
        Ok(())
    }

    /// Stops the running mode and puts the named one at the front of the queue, so it starts on the next
    /// pass of the mode machine (FUN_00186fa8). The priority drops to zero.
    pub fn interrupt_mode(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, name: u32) -> Result<(), VmError> {
        if self.modes.table == 0 {
            return Ok(());
        }
        if let Some(rec) = vm::with_shared(code, sh, |mem| self.modes.find(mem, name))? {
            self.stop_mode(prog, code, sh, host);
            let m = &mut self.modes;
            m.read = (m.read + QUEUE_LEN - 1) % QUEUE_LEN;
            m.queue[m.read] = (rec, 0);
            m.priority = 0;
        }
        Ok(())
    }

    /// Queues the named mode to start after the running one ends (FUN_00186ef8).
    pub fn queue_mode(&mut self, code: &[u8], sh: &mut Shared, name: u32) -> Result<(), VmError> {
        if self.modes.flags & 1 != 0 || self.modes.table == 0 {
            return Ok(());
        }
        let rec = vm::with_shared(code, sh, |mem| self.modes.find(mem, name))?;
        if let Some(rec) = rec {
            let m = &mut self.modes;
            if (m.write + QUEUE_LEN - m.read) % QUEUE_LEN < QUEUE_LEN - 1 {
                m.queue[m.write] = (rec, 0);
                m.write = (m.write + 1) % QUEUE_LEN;
            }
        }
        Ok(())
    }

    /// Queues a mode and marks the entry as one for another object (FUN_00186f50, host 6.58). The mark
    /// goes on the entry before the write position whether or not this call added one, as in the game.
    pub fn queue_mode_remote(&mut self, code: &[u8], sh: &mut Shared, name: u32) -> Result<(), VmError> {
        self.queue_mode(code, sh, name)?;
        let m = &mut self.modes;
        let at = (m.write + QUEUE_LEN - 1) % QUEUE_LEN;
        m.queue[at].1 |= QUEUE_REMOTE;
        Ok(())
    }

    /// A mode is queued or running (the test behind host function 1.10).
    pub fn mode_pending(&self) -> bool {
        self.modes.read != self.modes.write || self.modes.cur_rec != 0
    }

    pub fn current_mode_name(&self, code: &[u8], sh: &mut Shared) -> Option<String> {
        if self.modes.cur_rec == 0 {
            return None;
        }
        vm::with_shared(code, sh, |mem| {
            let n = ModeMachine::rec_field(mem, self.modes.cur_rec, 0).ok()?;
            mem.read_cstr(n).ok()
        })
    }

    /// Starts the next queued mode when the running one is done (FUN_00186900). An entry marked for
    /// another object is handed to the host instead and the next one is tried.
    fn pop_queue(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host) -> bool {
        loop {
            let m = &mut self.modes;
            if m.read == m.write || m.priority != 0 {
                return false;
            }
            let (rec, flag) = m.queue[m.read];
            m.read = (m.read + 1) % QUEUE_LEN;
            if flag & QUEUE_REMOTE == 0 {
                self.start_mode(prog, code, sh, host, rec);
                return true;
            }
            let name = vm::with_shared(code, sh, |mem| ModeMachine::rec_field(mem, rec, 0).ok().and_then(|n| mem.read_cstr(n).ok()));
            if let Some(name) = name {
                host.call(&Call::RemoteSetMode { obj: self.obj, name });
            }
        }
    }

    /// Delivers a message to the mode of that name: its message hook runs with this object's context and
    /// the sender's (FUN_001871d0). Messages for modes the object does not have are dropped.
    fn deliver(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, name: &str, sender: u32) {
        if self.modes.table == 0 {
            return;
        }
        let Ok(Some(rec)) = vm::with_shared(code, sh, |mem| self.modes.find_str(mem, name)) else { return };
        let pc = self.record_hook(code, sh, rec, HOOK_MESSAGE);
        let ctx = self.modes.ctx;
        let sender_ctx = if sender == 0 { 0 } else { sh.put_sender_ctx(sender) };
        self.run_temp(prog, code, sh, host, pc, &[ctx, sender_ctx]);
    }

    /// One game frame of the mode machine (FUN_001869b8).
    fn update_modes(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host) {
        if self.modes.table == 0 {
            return;
        }
        let obj = self.obj;
        if let Some(mut msg) = self.modes.pending.take() {
            msg.time -= FRAME_TIME;
            if msg.time <= 0.0 {
                self.deliver(prog, code, sh, host, &msg.name, msg.sender);
            } else {
                self.modes.pending = Some(msg);
            }
        }
        let mut max_prio = 0;
        self.modes.flags |= 2;
        for _ in 0..64 {
            if self.modes.flags & 1 == 0 && host.call(&Call::ModeFrozen { obj }).int() != 0 {
                break;
            }
            let started = self.pop_queue(prog, code, sh, host);
            let flags_now = self.modes.flags;
            if !started && flags_now & 2 == 0 {
                break;
            }
            self.modes.flags = flags_now & !2;
            if self.modes.cur_alive {
                if flags_now & 4 != 0 {
                    if host.call(&Call::ModeMayStart { obj }).int() == 0 {
                        break;
                    }
                    self.modes.flags &= !4;
                }
                max_prio = max_prio.max(self.modes.priority);
                if max_prio < INTERRUPT_PRIORITY && self.modes.flags & 1 == 0 && host.call(&Call::ModeInterrupted { obj }).int() != 0 {
                    break;
                }
                let Some(mut node) = self.modes.cur_thread.take() else { break };
                let gen_before = self.modes.generation;
                let finished = self.run_node(prog, code, sh, host, &mut node, None);
                if self.modes.generation == gen_before {
                    // Still the same mode: keep the thread until it has ended and its children are gone.
                    self.modes.cur_thread = Some(node);
                    if finished && self.modes.flags & 2 == 0 {
                        self.stop_mode(prog, code, sh, host);
                        self.modes.priority = 0;
                    }
                } else {
                    // The thread switched modes itself; it stays alive until the pass ends.
                    self.deferred.push(node);
                }
            }
        }
        for n in std::mem::take(&mut self.deferred) {
            self.destroy(prog, code, sh, host, n);
        }
    }

    /// Runs a hook of the running mode (FUN_00187040): event begin/end and damage.
    fn mode_hook(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, slot: u32, args: &[u32]) {
        if self.modes.cur_rec == 0 || self.modes.flags & 4 != 0 {
            return;
        }
        let pc = self.record_hook(code, sh, self.modes.cur_rec, slot);
        let ctx = self.modes.ctx;
        let args: Vec<u32> = if args.is_empty() { vec![ctx] } else { args.to_vec() };
        self.run_temp(prog, code, sh, host, pc, &args);
    }
}

impl Default for Core {
    fn default() -> Self {
        Core::new()
    }
}

impl Core {
    /// Argument count and result flag of a host function: this crate's own table first, then the
    /// executable's, then what `signatures.rs` worked out from the script.
    pub fn signature(&self, group: u16, index: u16) -> Option<(usize, bool)> {
        if let Some(f) = names::lookup(group, index) {
            return Some((f.args as usize, f.ret));
        }
        if let Some(s) = self.exe_table.as_ref().and_then(|t| t.get(group, index)) {
            return Some((s.nargs as usize, s.returns));
        }
        self.inferred.get(&(group, index)).copied()
    }
}

impl Env for CoreEnv<'_> {
    fn signature(&self, group: u16, index: u16) -> Option<(usize, bool)> {
        self.core.signature(group, index)
    }

    fn ext_read(&mut self, addr: u32, out: &mut [u8]) -> bool {
        if let Some(r) = self.core.heap_range(addr, out.len()) {
            out.copy_from_slice(&self.core.heap[r]);
            return true;
        }
        self.host.read_mem(addr, out)
    }

    fn ext_write(&mut self, addr: u32, data: &[u8]) -> bool {
        if let Some(r) = self.core.heap_range(addr, data.len()) {
            self.core.heap[r].copy_from_slice(data);
            return true;
        }
        self.host.write_mem(addr, data)
    }

    fn host_call(&mut self, p: &Program, mem: &mut Mem<'_>, group: u16, index: u16, a: &[u32]) -> Result<Option<u32>, VmError> {
        let code = self.code;
        self.dispatch(p, code, mem, group, index, a)
    }
}

impl CoreEnv<'_> {
    fn obj_of(&mut self, mem: &mut Mem<'_>, ctx: u32) -> Result<u32, VmError> {
        read32(mem, &mut HeapHost { heap: &mut self.core.heap, host: &mut *self.host }, ctx.wrapping_add(4))
    }

    fn vec(&mut self, mem: &mut Mem<'_>, ptr: u32) -> Result<Vec4, VmError> {
        read_vec(mem, &mut HeapHost { heap: &mut self.core.heap, host: &mut *self.host }, ptr)
    }

    fn cstr(&mut self, mem: &mut Mem<'_>, ptr: u32) -> Result<String, VmError> {
        mem.read_cstr(ptr).map_err(outside)
    }

    fn dispatch(&mut self, p: &Program, code: &[u8], mem: &mut Mem<'_>, g: u16, i: u16, a: &[u32]) -> Result<Option<u32>, VmError> {
        // Calls that are decoded into a typed request for the host.
        let call: Call = match (g, i) {
            // ---- group 0: math, no host involvement ----
            (0, idx) if math::is_math(idx) => {
                let mut hh = HeapHost { heap: &mut self.core.heap, host: &mut *self.host };
                let mut io = HostVec(&mut hh);
                return math::call(mem, idx, a, &mut io);
            }
            // Functions the retail executable implements as an empty return (debug prints and the like).
            (0, 0 | 1 | 2 | 8 | 24 | 25 | 26 | 60) | (1, 103 | 122 | 163 | 240 | 246) | (2, 36) => return Ok(None),
            (0, 79) => Call::ReleaseEffect { handle: a[0] },
            // The game heap (FUN_0016dea8 allocates through the allocator at 0x33e850, FUN_0016ded8 frees
            // with FUN_00104fe0): served by the runtime's own heap, see `HEAP_BASE`.
            (0, 47) => return Ok(Some(self.core.heap_alloc(a[0]))),
            (0, 48) => return Ok(None),
            // Start a thread under the thread this one runs for (FUN_0016d458): tag, routine, cleanup
            // routine, argument. Another start with the same tag bits kills the earlier thread first.
            (0, 9) => {
                let handle = self.core.alloc_handle();
                let args = if a[3] != 0 { vec![a[3]] } else { Vec::new() };
                let root = self.core.cur_root;
                self.core.requests.push(Request::Spawn { root, handle, tag: a[0], pc: a[1], cleanup: a[2], args });
                return Ok(Some(handle));
            }

            // ---- group 1 ----
            (1, 3) => Call::Player,
            (1, 4) => {
                let obj = self.obj_of(mem, a[0])?;
                Call::FaceDirection { obj, dir: self.vec(mem, a[1])? }
            }
            (1, 5) => {
                for k in 0..RECORD_COUNT * RECORD_SIZE / 4 {
                    mem.write_u32(a[0] + 4 * k, 0).map_err(outside)?;
                }
                return Ok(None);
            }
            (1, 6) => {
                self.register_mode(mem, a)?;
                return Ok(None);
            }
            (1, 7) => {
                self.core.modes.ctx = a[0];
                self.core.modes.table = a[1];
                return Ok(None);
            }
            (1, 8) => {
                let (core, host) = (&mut *self.core, &mut *self.host);
                core.set_mode(p, code, mem.sh, host, a[1])?;
                return Ok(None);
            }
            (1, 9) => {
                self.core.queue_mode(code, mem.sh, a[1])?;
                return Ok(None);
            }
            (1, 10) => return Ok(Some(self.core.mode_pending() as u32)),
            (1, 81) => {
                let (core, host) = (&mut *self.core, &mut *self.host);
                core.interrupt_mode(p, code, mem.sh, host, a[1])?;
                return Ok(None);
            }
            (1, 167) => {
                let (core, host) = (&mut *self.core, &mut *self.host);
                core.set_mode_priority(p, code, mem.sh, host, a[1])?;
                return Ok(None);
            }
            (1, 127) => {
                let (core, host) = (&mut *self.core, &mut *self.host);
                core.reset_modes(p, code, mem.sh, host);
                return Ok(None);
            }
            // Start a thread on the object's persistent thread (FUN_00172df0): context, tag, routine,
            // cleanup routine, argument.
            (1, 82) => {
                let handle = self.core.alloc_handle();
                let root = self.core.main_handle;
                self.core.requests.push(Request::Spawn { root, handle, tag: a[1], pc: a[2], cleanup: a[3], args: vec![a[4]] });
                return Ok(Some(handle));
            }
            // Messages to the modes of all objects (FUN_00187170 with the sender's context, FUN_00187190 without).
            (1, 90) => {
                let name = self.cstr(mem, a[1])?;
                Call::SendMessage { sender: self.obj_of(mem, a[0])?, name, delay: fl(a[2]) }
            }
            (1, 241) => Call::SendMessage { sender: 0, name: self.cstr(mem, a[0])?, delay: fl(a[1]) },
            // The object's action thread (offset +0xaec): routine and one optional argument after the object.
            (1, 112) => {
                let obj = self.obj_of(mem, a[0])?;
                let handle = self.core.alloc_handle();
                let args = if a[2] != 0 { vec![obj, a[2]] } else { vec![obj] };
                self.core.requests.push(Request::SetAction { handle, pc: a[1], args });
                return Ok(None);
            }
            (1, 214) => {
                self.core.requests.push(Request::StopAction);
                return Ok(None);
            }
            (1, 11) => Call::PlayMotion { obj: a[0], motion: a[1] as i32, blend: fl(a[2]) },
            (1, 12) => Call::PlayMotionChecked { obj: a[0], motion: a[1] as i32, blend: fl(a[2]) },
            (1, 14) => Call::MotionDone { obj: a[0] },
            (1, 15) => Call::MotionState { obj: a[0] },
            (1, 19) => Call::FadeOut { obj: a[0], frames: fl(a[1]) },
            (1, 20) => Call::FadeIn { obj: a[0], frames: fl(a[1]) },
            (1, 23) => Call::GlobalByte33e830,
            (1, 28) => Call::Hide { obj: self.obj_of(mem, a[0])? },
            (1, 34) => Call::EventActive,
            (1, 39) => Call::SpawnParam { obj: self.obj_of(mem, a[0])?, index: a[1] as i32 },
            (1, 55) | (1, 168) => {
                let obj = self.obj_of(mem, a[0])?;
                let dir = self.vec(mem, a[1])?;
                let (motion, blend, commit) = (a[2] as i32, fl(a[3]), a[4] != 0);
                if i == 55 { Call::StartMove { obj, dir, motion, blend, commit } } else { Call::StartMoveAlt { obj, dir, motion, blend, commit } }
            }
            (1, 56) => Call::OnScreen { obj: self.obj_of(mem, a[0])? },
            (1, 57) => Call::Airborne { obj: self.obj_of(mem, a[0])? },
            (1, 60) => Call::Busy { obj: self.obj_of(mem, a[0])? },
            (1, 61) => Call::PlayMotionAt { obj: a[0], motion: a[1] as i32, start: fl(a[2]), blend: fl(a[3]) },
            (1, 62) => Call::ObjVec860 { obj: self.obj_of(mem, a[0])? },
            (1, 64) => Call::ObjVec850 { obj: self.obj_of(mem, a[0])? },
            (1, 65) => Call::SetFlag26 { obj: self.obj_of(mem, a[0])? },
            (1, 67) => Call::MotionLength { obj: self.obj_of(mem, a[0])?, motion: a[1] as i32 },
            (1, 68) => Call::MotionEnd { obj: self.obj_of(mem, a[0])?, motion: a[1] as i32 },
            (1, 70) => Call::SetFlag { obj: self.obj_of(mem, a[0])?, bit: a[1] as i32 },
            (1, 71) => Call::ClearFlag { obj: self.obj_of(mem, a[0])?, bit: a[1] as i32 },
            (1, 78) => Call::HitStun { obj: self.obj_of(mem, a[0])? },
            (1, 79) => {
                let obj = self.obj_of(mem, a[0])?;
                Call::FaceToward { obj, dir: self.vec(mem, a[1])? }
            }
            (1, 80) => {
                let obj = self.obj_of(mem, a[0])?;
                Call::TurnToward { obj, dir: self.vec(mem, a[1])?, speed: fl(a[2]) }
            }
            (1, 87) => Call::SpawnAttached { obj: self.obj_of(mem, a[0])?, id: a[1] as i32, flag: a[2] as i32, extra: a[3] as i32 },
            (1, 89) => {
                let obj = self.obj_of(mem, a[0])?;
                Call::SetVelocity { obj, dir: self.vec(mem, a[1])?, speed: fl(a[2]) }
            }
            (1, 92) => Call::GroundMarker { obj: self.obj_of(mem, a[0])?, id: a[1] as i32 },
            (1, 104) => Call::SetFlag768 { obj: self.obj_of(mem, a[0])? },
            (1, 105) => Call::ClearFlag768 { obj: self.obj_of(mem, a[0])? },
            (1, 110) => Call::SpawnKind { obj: self.obj_of(mem, a[0])? },
            (1, 111) => {
                let obj = self.obj_of(mem, a[0])?;
                Call::SetMove { obj, dir: self.vec(mem, a[1])?, speed: fl(a[2]) }
            }
            (1, 119) => {
                let obj = self.obj_of(mem, a[0])?;
                Call::ProbeGround { obj, dir: self.vec(mem, a[1])?, a: fl(a[2]), b: fl(a[3]) }
            }
            (1, 120) => Call::TargetPosition { target: a[0] },
            (1, 121) => Call::SetTargetMode { target: a[0], obj: self.obj_of(mem, a[1])?, mode: a[2] as i32 },
            (1, 124) => Call::Flag124 { obj: self.obj_of(mem, a[0])? },
            (1, 128) => Call::MoverMethod { obj: self.obj_of(mem, a[0])?, value: fl(a[1]) },
            (1, 131) => Call::FreeTarget { handle: a[0] },
            (1, 133) => Call::ObjMethod44 { obj: self.obj_of(mem, a[0])? },
            (1, 134) => Call::ObjMethod48 { obj: self.obj_of(mem, a[0])? },
            (1, 147) => Call::Position { obj: self.obj_of(mem, a[0])? },
            (1, 171) => Call::GlobalFlag4,
            (1, 201) => Call::FacingVector { obj: self.obj_of(mem, a[0])? },
            (1, 206) => Call::GlobalFlag80,
            (1, 301) => Call::GlobalByte301,
            (1, 309) => Call::Flag309 { obj: a[0] },

            // ---- group 2 ----
            (2, 5) => Call::HitStun { obj: self.obj_of(mem, a[0])? },
            (2, 8) => Call::HitIsReal { hit: a[0] },
            (2, 9) => Call::SetObjParam { obj: self.obj_of(mem, a[0])?, a: a[1] as i32, b: a[2] as i32 },
            // A detached thread at routine a[0] with the single argument a[1] (FUN_0018e898 ->
            // FUN_001a4738: FUN_0016c838 makes the thread, FUN_001a42b8 links it into the list at
            // 0x1c95518; FUN_001a4428 runs the list once per frame). Returns the list node.
            (2, 20) => {
                let handle = self.core.alloc_handle();
                self.core.requests.push(Request::Detached { handle, pc: a[0], args: vec![a[1]] });
                return Ok(Some(handle));
            }
            (2, 23) => Call::Target { obj: self.obj_of(mem, a[0])? },
            (2, 88) => Call::LastAttacker { obj: self.obj_of(mem, a[0])? },

            // ---- group 6 ----
            (6, 58) => {
                self.core.queue_mode_remote(code, mem.sh, a[1])?;
                return Ok(None);
            }

            // Object lookups: the answer goes into the context the script passed (+4).
            (1, 114) | (1, 143) | (1, 210) => {
                let by = match i {
                    114 => ObjectKey::Entity,
                    143 => ObjectKey::SpawnGroup,
                    _ => ObjectKey::NeoStatus,
                };
                let found = self.host.call(&Call::FindObject { by, value: a[1] as i32 }).int() as u32;
                mem.write_u32(a[0].wrapping_add(4), found).map_err(outside)?;
                return Ok(None);
            }

            // 1.141 probe_ground_down (FUN_00172538 -> FUN_001a5c30(distance, 0, point, out, mask 4)): the
            // host answers the point found (`Value::Vec`, game space) or anything else for "nothing below".
            // The found point, or the point itself, goes through the second argument, which is the script's
            // own memory (MAGNET_*.mag passes a stack local), and the return is 1 or 0 (WP-F1 item 1).
            (1, 141) => {
                let from = self.vec(mem, a[0])?;
                let call = Call::Func { group: g, index: i, args: self.decode_args(mem, "vpf", a)? };
                let (found, point) = match self.host.call(&call) {
                    Value::Vec(p) => (true, p),
                    _ => (false, from),
                };
                HostVec(&mut *self.host).write_vec(mem, a[1], point)?;
                return Ok(Some(found as u32));
            }

            // Everything else that has been named is decoded by its kinds.
            _ => match names::lookup(g, i) {
                Some(f) => Call::Func { group: g, index: i, args: self.decode_args(mem, f.kinds, a)? },
                None => Call::Unknown { group: g, index: i, args: a.to_vec() },
            },
        };
        let v = self.host.call(&call);
        Ok(Some(convert(v, mem)))
    }

    /// Turns argument words into typed arguments by the kind letters of `names::HostFn::kinds`.
    fn decode_args(&mut self, mem: &mut Mem<'_>, kinds: &str, a: &[u32]) -> Result<Vec<Arg>, VmError> {
        let mut out = Vec::with_capacity(a.len());
        for (k, &w) in kinds.chars().zip(a) {
            out.push(match k {
                'O' => Arg::Obj(self.obj_of(mem, w)?),
                'o' => Arg::Obj(w),
                'i' => Arg::Int(w as i32),
                'f' => Arg::Float(fl(w)),
                'v' => Arg::Vec(self.vec(mem, w)?),
                's' => Arg::Str(self.cstr(mem, w)?),
                _ => Arg::Raw(w),
            });
        }
        Ok(out)
    }

    /// Host function 1.6: writes a mode record (FUN_00186730). `a` = table, name, flags, then the routine words.
    fn register_mode(&mut self, mem: &mut Mem<'_>, a: &[u32]) -> Result<(), VmError> {
        let (table, name, flags) = (a[0], a[1], a[2]);
        let mut slot = None;
        let mut free = None;
        for k in 0..RECORD_COUNT {
            let rec = table + k * RECORD_SIZE;
            let n = mem.read_u32(rec).map_err(outside)?;
            if n == 0 {
                free.get_or_insert(rec);
            } else if strcmp_eq(mem, name, n) {
                slot = Some(rec);
                break;
            }
        }
        let rec = match slot {
            Some(r) => r,
            None => {
                let r = free.ok_or_else(|| VmError::Host("mode table full".into()))?;
                mem.write_u32(r, name).map_err(outside)?;
                r
            }
        };
        let put16 = |mem: &mut Mem<'_>, off: u32, v: u32| mem.write(rec + off, &(v as u16).to_le_bytes()).map_err(outside);
        put16(mem, 8, a[3])?;
        put16(mem, 4, flags)?;
        put16(mem, 6, flags >> 16)?;
        for k in 0..11u32 {
            // The game copies eleven words but only passes nine routine arguments; the rest is stack residue.
            let v = a.get(4 + k as usize).copied().unwrap_or(0);
            put16(mem, 10 + 2 * k, v)?;
        }
        Ok(())
    }
}

impl Runtime {
    pub fn new(prog: Program) -> Runtime {
        let code = prog.words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let shared = Shared::new(&prog);
        let mut core = Core::new();
        core.inferred = crate::signatures::infer_only(&prog, &named, &crate::signatures::absent_from_table);
        Runtime { prog, code, shared, core, main: None }
    }

    /// Gives the runtime the signatures read from the user's executable ([`HostTable::from_exe`]). The
    /// entries the table lacks (see `signatures.rs`) are worked out again with the table's help, which
    /// resolves calls nested in arguments of other functions.
    pub fn set_host_table(&mut self, table: HostTable) {
        let known = |g: u16, i: u16| named(g, i).or_else(|| table.get(g, i).map(|s| (s.nargs as usize, s.returns)));
        self.core.inferred = crate::signatures::infer_only(&self.prog, &known, &|g, i| table.get(g, i).is_none());
        self.core.exe_table = Some(table);
    }

    /// Creates the object's script thread: runs event 0 with the object, then makes event 1 the persistent
    /// thread (FUN_00169a90). Returns false if the script has no event 1 (the thread then counts as
    /// finished from the start).
    pub fn spawn(&mut self, host: &mut dyn Host, obj: u32) -> bool {
        self.core.obj = obj;
        self.core.spawned = true;
        let handle = self.core.alloc_handle();
        self.core.main_handle = handle;
        self.event(host, 0, &[obj]);
        let entry = self.prog.entry(1);
        let mut node = self.core.new_node(&self.prog, &mut self.shared, handle, entry.unwrap_or(0), &[]);
        node.finished = entry.is_none();
        self.main = Some(node);
        entry.is_some()
    }

    /// A script thread for `obj` whose code is event `event` of the program (FUN_0016c9b8 makes the
    /// thread, FUN_0016c500 points it at the event): the way the cast start FUN_001a4798 runs a spell
    /// (event 20 with the caster, the effect file, the kind and the `magi` row). The thread becomes the
    /// runtime's persistent thread and runs one slice per [`update`](Self::update) (the magic task
    /// FUN_001a4428 runs it with FUN_0016c680) until it ends. No event 0 runs. Returns false when the
    /// program has no such event (the thread is then over at once, FUN_0016c4d8 -> FUN_0016c7c8).
    pub fn start_thread(&mut self, obj: u32, event: u32, args: &[u32]) -> bool {
        self.core.obj = obj;
        self.core.spawned = true;
        let handle = self.core.alloc_handle();
        self.core.main_handle = handle;
        let entry = self.prog.entry(event);
        let mut node = self.core.new_node(&self.prog, &mut self.shared, handle, entry.unwrap_or(0), args);
        node.finished = entry.is_none();
        self.main = Some(node);
        entry.is_some()
    }

    /// FUN_0016c500 on the persistent thread: its code becomes event `event` with `args` (FUN_0016c4d8
    /// clears the thread's flags, so an ended thread runs again; FUN_001d7bb0 sets the program counter
    /// and pushes four zero frame words and the arguments without resetting the frame or the operand
    /// stack, `Thread::restart`). The thread's children stay. The new code runs at the next
    /// [`update`](Self::update). Returns false (and ends the thread) when the program has no such
    /// event or there is no thread.
    pub fn restart_thread(&mut self, event: u32, args: &[u32]) -> bool {
        let entry = self.prog.entry(event);
        let Some(node) = self.main.as_mut() else { return false };
        match entry {
            Some(pc) => {
                node.th.restart(pc as usize, args);
                node.finished = false;
                true
            }
            None => {
                node.finished = true;
                false
            }
        }
    }

    /// The persistent thread has ended and has no children left (FUN_0016c680 returning 1, after
    /// which FUN_001a4428 destroys it).
    pub fn thread_done(&self) -> bool {
        self.main.as_ref().is_none_or(|n| n.finished && n.children.is_empty())
    }

    /// Delivers a game event: runs its handler to the first yield. Returns false when the script has no handler.
    pub fn event(&mut self, host: &mut dyn Host, id: u32, args: &[u32]) -> bool {
        let Some(pc) = self.prog.entry(id) else { return false };
        // Event 6 reaches the running mode's damage hook first (FUN_001870f8).
        if id == 6 {
            let ctx = self.core.modes.ctx;
            let hit = args.first().copied().unwrap_or(0);
            self.core.mode_hook(&self.prog, &self.code, &mut self.shared, host, HOOK_DAMAGE, &[ctx, hit]);
        }
        if id == 13 {
            self.core.mode_hook(&self.prog, &self.code, &mut self.shared, host, HOOK_EVENT_BEGIN, &[]);
        }
        if id == 14 {
            self.core.mode_hook(&self.prog, &self.code, &mut self.shared, host, HOOK_EVENT_END, &[]);
        }
        self.core.run_temp(&self.prog, &self.code, &mut self.shared, host, pc, args);
        true
    }

    /// The script's part of the enemy clash handler (0x1b2478), given the two attack volumes: when the
    /// object has a script thread (+0x484), the running mode's clash hook (slot 8, FUN_00187130) with
    /// (context, own volume, other volume), then event 0x13 with (own volume, other volume). The handler
    /// does both before its own flag 0x40 / `RefactSelf` 2 tests, so every clash reaches the script.
    /// Returns whether the script has an event 0x13 handler.
    pub fn clash(&mut self, host: &mut dyn Host, own: u32, other: u32) -> bool {
        if self.main.is_none() {
            return false;
        }
        let ctx = self.core.modes.ctx;
        self.core.mode_hook(&self.prog, &self.code, &mut self.shared, host, HOOK_CLASH, &[ctx, own, other]);
        self.event(host, EVENT_CLASH, &[own, other])
    }

    /// One game frame of the script (FUN_0016a288): the main thread and its children run, then the mode
    /// machine while the object's script has not ended. The action thread (host 1.112) runs afterwards;
    /// where the game runs it relative to this is not known, hosts that care can call
    /// [`update_script`](Self::update_script) and [`update_action`](Self::update_action) themselves.
    pub fn update(&mut self, host: &mut dyn Host) {
        self.update_script(host);
        self.update_action(host);
        self.update_requests(host);
    }

    /// The detached request threads of host 2.20 (the task FUN_001a4428 in the game, whose order
    /// relative to the object updates is not known): each runs one slice, ended ones are destroyed.
    pub fn update_requests(&mut self, host: &mut dyn Host) {
        let (prog, code) = (&self.prog, &self.code);
        self.core.apply_action_requests(prog, code, &mut self.shared, host);
        self.core.run_requests(prog, code, &mut self.shared, host);
    }

    /// Number of detached request threads (host 2.20) still running.
    pub fn request_threads(&self) -> usize {
        self.core.requests_threads.len()
    }

    /// The script part of a frame: main thread, its children, the mode machine.
    pub fn update_script(&mut self, host: &mut dyn Host) {
        let Some(mut node) = self.main.take() else { return };
        let done = self.core.run_node(&self.prog, &self.code, &mut self.shared, host, &mut node, None);
        self.main = Some(node);
        if !done {
            self.core.update_modes(&self.prog, &self.code, &mut self.shared, host);
        }
        // Requests addressed to threads that no longer exist (or to the persistent thread, which only
        // runs next frame) must not pile up: keep the latter, drop the rest.
        let main = self.core.main_handle;
        self.core.requests.retain(|r| match r {
            Request::Spawn { root, .. } => *root == main,
            _ => true,
        });
    }

    /// The action thread (FUN_00176528): runs unless the object is frozen (flag 0x40000, as for the mode
    /// machine), and is destroyed once it has ended and has no children.
    pub fn update_action(&mut self, host: &mut dyn Host) {
        let (prog, code) = (&self.prog, &self.code);
        self.core.apply_action_requests(prog, code, &mut self.shared, host);
        if let Some(mut node) = self.core.action.take() {
            if host.call(&Call::ModeFrozen { obj: self.core.obj }).int() == 0 {
                let done = self.core.run_node(prog, code, &mut self.shared, host, &mut node, None);
                if done {
                    self.core.destroy(prog, code, &mut self.shared, host, node);
                } else {
                    self.core.action = Some(node);
                }
            } else {
                self.core.action = Some(node);
            }
        }
        self.core.apply_action_requests(prog, code, &mut self.shared, host);
    }

    /// A message from another object (or from nobody, `sender` 0) addressed to a mode by name. With a
    /// delay of zero or less the mode's message hook runs now, otherwise the message waits in the
    /// object's single pending slot (a later message replaces it) and is delivered when the delay has
    /// counted down in [`update`](Self::update). Hosts broadcast a [`Call::SendMessage`] to every
    /// object and pick each receiver's delay with [`message_delay`].
    pub fn receive_message(&mut self, host: &mut dyn Host, name: &str, sender: u32, delay: f32) {
        if delay <= 0.0 {
            self.core.deliver(&self.prog, &self.code, &mut self.shared, host, name, sender);
        } else {
            self.core.modes.pending = Some(PendingMessage { name: name.to_string(), sender, time: delay });
        }
    }

    /// Switches this object to the named mode (the receiving end of [`Call::RemoteSetMode`]).
    pub fn set_mode_named(&mut self, host: &mut dyn Host, name: &str) {
        let (prog, code) = (&self.prog, &self.code);
        let _ = self.core.set_mode_named(prog, code, &mut self.shared, host, name);
    }

    /// Name of the running mode, if any.
    pub fn current_mode(&mut self) -> Option<String> {
        self.core.current_mode_name(&self.code, &mut self.shared)
    }

    /// Number of live threads of the object: persistent, mode, action and all their children. The game keeps
    /// the same count (offset +0x488) and treats the object's script as over when it reaches zero.
    pub fn live_threads(&self) -> usize {
        fn count(n: &Node) -> usize {
            usize::from(!n.finished) + n.children.iter().map(count).sum::<usize>()
        }
        let m = &self.core.modes;
        self.main.iter().chain(m.cur_thread.iter()).chain(self.core.action.iter()).chain(self.core.requests_threads.iter()).map(count).sum()
    }
}

/// Gives the math functions access to vectors in script memory or in the host's.
struct HostVec<'a>(&'a mut dyn Host);

impl VecAccess for HostVec<'_> {
    fn read_vec(&mut self, mem: &mut Mem<'_>, addr: u32) -> Result<Vec4, VmError> {
        read_vec(mem, self.0, addr)
    }

    fn write_vec(&mut self, mem: &mut Mem<'_>, addr: u32, v: Vec4) -> Result<(), VmError> {
        match mem.write_vec4(addr, v) {
            Ok(()) => Ok(()),
            Err(Outside::Ext(a)) => {
                let mut b = [0u8; 16];
                for (k, x) in v.iter().enumerate() {
                    b[k * 4..k * 4 + 4].copy_from_slice(&x.to_le_bytes());
                }
                if self.0.write_mem(a, &b) { Ok(()) } else { Err(VmError::BadAddress(a)) }
            }
            Err(Outside::Fault(a)) => Err(VmError::BadAddress(a)),
        }
    }
}
