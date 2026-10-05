//! Running a script the way the game does: an object gets one persistent thread (event 1) plus a mode
//! machine that starts one more thread per named "mode". Events from the game (damage, event start and
//! end...) run short handlers to their first `yield`.
//!
//! The mode machine here is a port of the game's per-object mode manager (FUN_001869b8 and the
//! functions around it). Parts that depend on game state go through [`Call`] (see
//! `ModeFrozen`, `ModeInterrupted`, `ModeMayStart`). The "delayed message" feature of the manager is not ported.

use crate::exe::HostTable;
use crate::host::{Call, Host, Value, Vec4};
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

// Hook slots of a mode record (index into the u16 routine array that starts at +8).
const HOOK_MAIN: u32 = 0;
const HOOK_EXIT: u32 = 1;
const HOOK_EVENT_BEGIN: u32 = 2;
const HOOK_EVENT_END: u32 = 3;
const HOOK_DAMAGE: u32 = 4;
const HOOK_ENTER: u32 = 7;

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
    cur_thread: Option<Thread>,
    /// Bumped whenever the running mode changes, to notice a switch made by the mode's own thread.
    generation: u64,
    /// Priority of the running mode (`+0x98`); 0 when idle.
    priority: i32,
    /// Bit 0: switching disabled, bit 1: a mode was started, bit 2: ask the host before running.
    flags: u32,
}

/// State shared by everything that runs inside one object's script.
pub struct Core {
    /// The game object this script drives.
    pub obj: u32,
    modes: ModeMachine,
    /// Signatures read from the executable for functions this crate does not know.
    pub exe_table: Option<HostTable>,
    /// Errors raised by threads (a thread that errors ends, like in the game).
    pub errors: Vec<(usize, VmError)>,
    depth: u32,
    spawned: bool,
}

pub struct Runtime {
    pub prog: Program,
    code: Vec<u8>,
    pub shared: Shared,
    pub core: Core,
    main: Option<Thread>,
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

fn fl(v: u32) -> f32 {
    f32::from_bits(v)
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

    fn hook_pc(mem: &mut Mem<'_>, rec: u32, slot: u32) -> Result<u32, VmError> {
        Self::rec_u16(mem, rec, 8 + 2 * slot)
    }
}

impl Core {
    pub fn new() -> Core {
        Core { obj: 0, modes: ModeMachine::default(), exe_table: None, errors: Vec::new(), depth: 0, spawned: false }
    }

    /// Runs a routine as a throw-away thread until its first yield (FUN_0016c550).
    fn run_temp(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host, pc: u32, args: &[u32]) {
        if pc == 0 || !self.spawned || self.depth >= MAX_DEPTH {
            return;
        }
        let mut th = Thread::new(prog, sh);
        th.start(pc as usize, args);
        self.run_thread(prog, code, sh, host, &mut th);
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

    // ---- mode machine --------------------------------------------------------------------

    /// Ends the running mode: exit hook, thread dropped (FUN_001868a8).
    fn stop_mode(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host) {
        if !self.modes.cur_alive {
            return;
        }
        let rec = self.modes.cur_rec;
        let ctx = self.modes.ctx;
        let hook = self.record_hook(code, sh, rec, HOOK_EXIT);
        self.modes.cur_alive = false;
        self.modes.cur_rec = 0;
        self.modes.cur_thread = None;
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
        let mut th = Thread::new(prog, sh);
        th.start(main as usize, &[ctx]);
        self.modes.cur_thread = Some(th);
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
        if let Some(rec) = rec {
            self.reset_modes(prog, code, sh, host);
            self.start_mode(prog, code, sh, host, rec);
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

    /// Starts the next queued mode when the running one is done (FUN_00186900).
    fn pop_queue(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host) -> bool {
        let m = &mut self.modes;
        if m.read == m.write || m.priority != 0 {
            return false;
        }
        let (rec, _flag) = m.queue[m.read];
        m.read = (m.read + 1) % QUEUE_LEN;
        self.start_mode(prog, code, sh, host, rec);
        true
    }

    /// One game frame of the mode machine (FUN_001869b8).
    fn update_modes(&mut self, prog: &Program, code: &[u8], sh: &mut Shared, host: &mut dyn Host) {
        if self.modes.table == 0 {
            return;
        }
        let obj = self.obj;
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
                let Some(mut th) = self.modes.cur_thread.take() else { break };
                let gen_before = self.modes.generation;
                let stop = self.run_thread(prog, code, sh, host, &mut th);
                if self.modes.generation == gen_before {
                    // Still the same mode: keep the thread unless it ended.
                    if stop == Stop::Yield {
                        self.modes.cur_thread = Some(th);
                    } else if self.modes.flags & 2 == 0 {
                        self.stop_mode(prog, code, sh, host);
                        self.modes.priority = 0;
                    }
                }
            }
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

impl Env for CoreEnv<'_> {
    fn signature(&self, group: u16, index: u16) -> Option<(usize, bool)> {
        if let Some(f) = names::lookup(group, index) {
            return Some((f.args as usize, f.ret));
        }
        self.core.exe_table.as_ref()?.get(group, index).map(|s| (s.nargs as usize, s.returns))
    }

    fn ext_read(&mut self, addr: u32, out: &mut [u8]) -> bool {
        self.host.read_mem(addr, out)
    }

    fn ext_write(&mut self, addr: u32, data: &[u8]) -> bool {
        self.host.write_mem(addr, data)
    }

    fn host_call(&mut self, p: &Program, mem: &mut Mem<'_>, group: u16, index: u16, a: &[u32]) -> Result<Option<u32>, VmError> {
        let code = self.code;
        self.dispatch(p, code, mem, group, index, a)
    }
}

impl CoreEnv<'_> {
    fn obj_of(&mut self, mem: &mut Mem<'_>, ctx: u32) -> Result<u32, VmError> {
        read32(mem, self.host, ctx.wrapping_add(4))
    }

    fn vec(&mut self, mem: &mut Mem<'_>, ptr: u32) -> Result<Vec4, VmError> {
        read_vec(mem, self.host, ptr)
    }

    fn dispatch(&mut self, p: &Program, code: &[u8], mem: &mut Mem<'_>, g: u16, i: u16, a: &[u32]) -> Result<Option<u32>, VmError> {
        // Calls that are decoded into a typed request for the host.
        let call: Call = match (g, i) {
            // ---- group 0: math, no host involvement ----
            (0, idx) if math::is_math(idx) => {
                let mut io = HostVec(&mut *self.host);
                return math::call(mem, idx, a, &mut io);
            }
            (0, 79) => Call::ReleaseEffect { handle: a[0] },

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
            (1, 127) => {
                let (core, host) = (&mut *self.core, &mut *self.host);
                core.reset_modes(p, code, mem.sh, host);
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
            (2, 20) => Call::Request2_20 { a: a[0], b: a[1] },
            (2, 23) => Call::Target { obj: self.obj_of(mem, a[0])? },
            (2, 88) => Call::LastAttacker { obj: self.obj_of(mem, a[0])? },

            _ => Call::Unknown { group: g, index: i, args: a.to_vec() },
        };
        let v = self.host.call(&call);
        Ok(Some(convert(v, mem)))
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
        Runtime { prog, code, shared, core: Core::new(), main: None }
    }

    /// Creates the object's script thread: runs event 0 with the object, then makes event 1 the persistent
    /// thread (FUN_00169a90). Returns false if the script has no event 1.
    pub fn spawn(&mut self, host: &mut dyn Host, obj: u32) -> bool {
        self.core.obj = obj;
        self.core.spawned = true;
        self.event(host, 0, &[obj]);
        let Some(pc) = self.prog.entry(1) else { return false };
        let mut th = Thread::new(&self.prog, &mut self.shared);
        th.start(pc as usize, &[]);
        self.main = Some(th);
        true
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

    /// One game frame (FUN_0016a288): the main thread runs, then the mode machine while the main thread lives.
    pub fn update(&mut self, host: &mut dyn Host) {
        let Some(mut th) = self.main.take() else { return };
        let stop = self.core.run_thread(&self.prog, &self.code, &mut self.shared, host, &mut th);
        if stop == Stop::Yield {
            self.main = Some(th);
            self.core.update_modes(&self.prog, &self.code, &mut self.shared, host);
        }
    }

    /// Name of the running mode, if any.
    pub fn current_mode(&mut self) -> Option<String> {
        self.core.current_mode_name(&self.code, &mut self.shared)
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
