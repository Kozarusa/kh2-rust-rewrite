//! The interpreter: a stack machine over 32-bit words. Port of the game's interpreter loop
//! (FUN_001d7cb0 in the USA executable): same operand encodings, same stack discipline, same run
//! results. Host functions are reached through the [`Env`] trait.

use crate::insn::{BinaryOp, Insn, Operand, UnaryOp, Base, decode};
use crate::program::Program;

/// Where the program image appears in the script's address space (`code + 2 * word offset`).
pub const CODE_ADDR: u32 = 0x1000_0000;
/// Global memory of the object that owns the script.
pub const GLOBAL_ADDR: u32 = 0x2000_0000;
/// Frame memory of thread `id` starts at `FRAME_ADDR + id * FRAME_STRIDE`.
pub const FRAME_ADDR: u32 = 0x3000_0000;
pub const FRAME_STRIDE: u32 = 0x1_0000;
/// Small result buffers for host functions that return a pointer to a vector.
pub const SCRATCH_ADDR: u32 = 0x6000_0000;
const SCRATCH_SLOTS: usize = 16;
/// Number of distinct frame regions (thread ids wrap at this value).
const THREAD_IDS: u32 = 0x400;

/// Extra bytes the game adds to the frame and operand stacks on top of the sizes in the header.
const STACK_SLACK: usize = 0x100;
/// The game's host-function argument buffer holds 16 words.
const MAX_ARGS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    /// The word at `pc` is not a valid instruction (the game ends the thread with result 5).
    BadInstruction { pc: usize, word: u16 },
    /// The program counter left the code.
    PcOutOfRange(usize),
    OperandStackUnderflow,
    OperandStackOverflow,
    FrameOverflow,
    /// Integer division or remainder by zero (the game traps).
    DivideByZero { pc: usize },
    /// Address outside the script's memory and not handled by the host.
    BadAddress(u32),
    /// Host function not known to the signature table.
    UnknownHost { group: u16, index: u16 },
    /// Too many instructions in one run (guards against scripts that never yield).
    StepLimit,
    /// Host function reported a failure.
    Host(String),
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for VmError {}

/// Result of running a thread until it stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// `yield`: continue next update (the game's result 1).
    Yield,
    /// `exit` (result 2).
    Exit,
    /// Returned from the outermost frame (result 3).
    Done,
}

/// Where the regions of script memory sit in the 32-bit address space the script sees. The defaults put
/// them far apart; tests that compare against the game's interpreter place them like the game's.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub code: u32,
    pub globals: u32,
    pub frames: u32,
    pub frame_stride: u32,
    pub scratch: u32,
}

impl Default for Layout {
    fn default() -> Layout {
        Layout { code: CODE_ADDR, globals: GLOBAL_ADDR, frames: FRAME_ADDR, frame_stride: FRAME_STRIDE, scratch: SCRATCH_ADDR }
    }
}

/// Memory shared by all threads of one script instance.
pub struct Shared {
    pub layout: Layout,
    pub globals: Vec<u8>,
    scratch: Vec<u8>,
    scratch_next: usize,
    /// State of the script random generator (`seed = seed * 0x10dcd`).
    pub rng: u32,
    pub next_thread_id: u32,
    /// Instructions executed per `run` before giving up.
    pub step_limit: u64,
}

impl Shared {
    pub fn new(p: &Program) -> Shared {
        Shared {
            layout: Layout::default(),
            globals: vec![0; p.globals as usize],
            scratch: vec![0; (SCRATCH_SLOTS + 1) * 16],
            scratch_next: 0,
            rng: 0,
            next_thread_id: 0,
            step_limit: 1_000_000,
        }
    }

    /// Advances the generator and returns the new state.
    pub fn next_rand(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(0x10dcd);
        self.rng
    }

    /// Builds a stand-in for the context structure of another object (the game passes a pointer into the
    /// sender's own script memory with a message) in a slot the vector results never reuse: word 1 is the
    /// object, which is the only field scripts read from a sender. Returns its script address.
    pub fn put_sender_ctx(&mut self, obj: u32) -> u32 {
        let base = SCRATCH_SLOTS * 16;
        self.scratch[base..base + 16].fill(0);
        self.scratch[base + 4..base + 8].copy_from_slice(&obj.to_le_bytes());
        self.layout.scratch + base as u32
    }

    /// Stores a 16-byte vector in the next scratch slot and returns its script address.
    pub fn put_vec(&mut self, v: [f32; 4]) -> u32 {
        let slot = self.scratch_next;
        self.scratch_next = (slot + 1) % SCRATCH_SLOTS;
        for (i, x) in v.iter().enumerate() {
            self.scratch[slot * 16 + i * 4..][..4].copy_from_slice(&x.to_le_bytes());
        }
        self.layout.scratch + (slot * 16) as u32
    }
}

/// One thread of execution: its own frame memory and operand stack.
#[derive(Debug)]
pub struct Thread {
    pub id: u32,
    pub pc: usize,
    frame: Vec<u8>,
    fp: usize,
    values: Vec<u32>,
    value_cap: usize,
}

impl Thread {
    pub fn new(p: &Program, sh: &mut Shared) -> Thread {
        // Ids pick the thread's frame region (`frames + id * stride`). A script that runs for a long time
        // creates many short threads (every event hook is one), so ids wrap instead of growing out of the
        // address space; a thousand live threads at once do not occur.
        let id = sh.next_thread_id;
        sh.next_thread_id = (id + 1) % THREAD_IDS;
        Thread {
            id,
            pc: 0,
            frame: vec![0; p.frame_stack as usize + STACK_SLACK],
            fp: 0,
            values: Vec::new(),
            value_cap: (p.value_stack as usize + STACK_SLACK) / 4,
        }
    }

    /// Points the thread at an entry and pushes its arguments (first argument pushed first), like the game's start routine.
    /// Four zero words go on the frame stack so that returning from the first routine ends the thread.
    pub fn start(&mut self, pc: usize, args: &[u32]) {
        self.pc = pc;
        self.frame[..16].fill(0);
        self.fp = 16;
        self.values.clear();
        self.values.extend_from_slice(args);
    }

    /// Points a thread that has already run at a new entry (FUN_001d7bb0 on a live thread, as
    /// FUN_0016c500 -> FUN_0016c4d8 does to restart a thread at another event): the frame pointer and
    /// the operand stack are not reset. Four zero words go on the frame stack above the current frame
    /// and the arguments on top of the operand stack, so the new routine runs above whatever the old
    /// one left, and returning from it ends the thread. On a fresh thread this is [`start`](Self::start).
    pub fn restart(&mut self, pc: usize, args: &[u32]) {
        self.pc = pc;
        let fp = self.fp;
        if fp + 16 <= self.frame.len() {
            self.frame[fp..fp + 16].fill(0);
        }
        self.fp = fp + 16;
        self.values.extend_from_slice(args);
    }

    /// Script address of the thread's frame memory.
    pub fn frame_addr(&self, layout: &Layout) -> u32 {
        layout.frames + self.id * layout.frame_stride
    }
}

/// Typed access to the script's own memory. Anything outside it is left to the host.
pub struct Mem<'a> {
    code: &'a [u8],
    pub sh: &'a mut Shared,
    frame: &'a mut Vec<u8>,
    frame_addr: u32,
}

pub enum Outside {
    /// The address is not in script memory; the host may know it.
    Ext(u32),
    /// Write to the read-only program image, or access running past a region.
    Fault(u32),
}

/// Runs `f` with a view of the shared memory only (no frame), for code that has to look at mode
/// records and the like outside any thread.
pub fn with_shared<R>(code: &[u8], sh: &mut Shared, f: impl FnOnce(&mut Mem<'_>) -> R) -> R {
    let mut frame = Vec::new();
    let mut mem = Mem { code, sh, frame: &mut frame, frame_addr: u32::MAX };
    f(&mut mem)
}

impl<'a> Mem<'a> {
    pub fn code(&self) -> &'a [u8] {
        self.code
    }
}

impl Mem<'_> {
    fn region(&mut self, addr: u32, len: usize) -> Result<&mut [u8], Outside> {
        let pick = |base: u32, buf: &mut [u8]| -> Option<Result<(), ()>> {
            if addr >= base && ((addr - base) as usize) < buf.len() {
                Some(if (addr - base) as usize + len <= buf.len() { Ok(()) } else { Err(()) })
            } else {
                None
            }
        };
        if let Some(r) = pick(self.frame_addr, self.frame) {
            return r.map(|_| &mut self.frame[(addr - self.frame_addr) as usize..][..len]).map_err(|_| Outside::Fault(addr));
        }
        if let Some(r) = pick(self.sh.layout.globals, &mut self.sh.globals) {
            return r.map(|_| &mut self.sh.globals[(addr - self.sh.layout.globals) as usize..][..len]).map_err(|_| Outside::Fault(addr));
        }
        if let Some(r) = pick(self.sh.layout.scratch, &mut self.sh.scratch) {
            return r.map(|_| &mut self.sh.scratch[(addr - self.sh.layout.scratch) as usize..][..len]).map_err(|_| Outside::Fault(addr));
        }
        Err(Outside::Ext(addr))
    }

    pub fn read(&mut self, addr: u32, out: &mut [u8]) -> Result<(), Outside> {
        let code_base = self.sh.layout.code;
        if addr >= code_base && ((addr - code_base) as usize) < self.code.len() {
            let o = (addr - code_base) as usize;
            return match self.code.get(o..o + out.len()) {
                Some(s) => {
                    out.copy_from_slice(s);
                    Ok(())
                }
                None => Err(Outside::Fault(addr)),
            };
        }
        let r = self.region(addr, out.len())?;
        out.copy_from_slice(r);
        Ok(())
    }

    pub fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), Outside> {
        let code_base = self.sh.layout.code;
        if addr >= code_base && ((addr - code_base) as usize) < self.code.len() {
            return Err(Outside::Fault(addr));
        }
        let r = self.region(addr, data.len())?;
        r.copy_from_slice(data);
        Ok(())
    }

    pub fn read_u32(&mut self, addr: u32) -> Result<u32, Outside> {
        let mut b = [0; 4];
        self.read(addr, &mut b)?;
        Ok(u32::from_le_bytes(b))
    }

    pub fn write_u32(&mut self, addr: u32, v: u32) -> Result<(), Outside> {
        self.write(addr, &v.to_le_bytes())
    }

    pub fn read_vec4(&mut self, addr: u32) -> Result<[f32; 4], Outside> {
        let mut b = [0; 16];
        self.read(addr, &mut b)?;
        Ok(std::array::from_fn(|i| f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap())))
    }

    pub fn write_vec4(&mut self, addr: u32, v: [f32; 4]) -> Result<(), Outside> {
        let mut b = [0; 16];
        for (i, x) in v.iter().enumerate() {
            b[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
        }
        self.write(addr, &b)
    }

    /// NUL-terminated string (at most 255 bytes).
    pub fn read_cstr(&mut self, addr: u32) -> Result<String, Outside> {
        let mut s = Vec::new();
        for i in 0..255 {
            let mut b = [0u8];
            self.read(addr + i, &mut b)?;
            if b[0] == 0 {
                break;
            }
            s.push(b[0]);
        }
        Ok(String::from_utf8_lossy(&s).into_owned())
    }
}

/// Interpreter state at the start of an instruction.
#[derive(Debug, Clone, Copy)]
pub struct TraceState {
    pub thread: u32,
    pub pc: usize,
    /// Words on the operand stack.
    pub depth: usize,
    /// Frame pointer as an offset into the frame memory.
    pub fp: usize,
    pub top: Option<u32>,
}

/// What the interpreter needs from its surroundings: host functions and memory it does not own.
pub trait Env {
    /// Argument count and whether a result is pushed for a host function.
    fn signature(&self, group: u16, index: u16) -> Option<(usize, bool)>;
    /// Runs a host function. `args[0]` is the first argument pushed. Returns the value to push, if the
    /// function has a result.
    fn host_call(&mut self, p: &Program, mem: &mut Mem<'_>, group: u16, index: u16, args: &[u32]) -> Result<Option<u32>, VmError>;
    /// Memory outside the script (game objects). Return false when the address is unknown.
    fn ext_read(&mut self, _addr: u32, _out: &mut [u8]) -> bool {
        false
    }
    fn ext_write(&mut self, _addr: u32, _data: &[u8]) -> bool {
        false
    }
    /// Called before every instruction (tracing).
    fn trace(&mut self, _state: &TraceState, _insn: &Insn) {}
}

fn f(v: u32) -> f32 {
    f32::from_bits(v)
}

/// The console's float unit has no infinities or NaNs: overflow saturates to the largest normal number.
pub fn ee_clamp(x: f32) -> f32 {
    if x.is_nan() {
        f32::MAX
    } else if x.is_infinite() {
        f32::MAX.copysign(x)
    } else {
        x
    }
}

/// The interpreter's own pi and 2 pi: the game's constants are one unit in the last place below the nearest floats.
const PI: f32 = f32::from_bits(0x4049_0fda);
const TWO_PI: f32 = f32::from_bits(0x40c9_0fda);

/// Wraps an angle in radians the way the interpreter does before sin and cos.
pub fn wrap_angle(x: f32) -> f32 {
    ((x + PI) % TWO_PI) - PI
}

fn unary_int(op: UnaryOp, x: u32) -> u32 {
    let i = x as i32;
    match op {
        UnaryOp::Convert => (i as f32).to_bits(),
        UnaryOp::Neg => i.wrapping_neg() as u32,
        UnaryOp::Not => !x,
        UnaryOp::IsZero => (x == 0) as u32,
        UnaryOp::Abs => i.wrapping_abs() as u32,
        UnaryOp::Lt0 => (i < 0) as u32,
        UnaryOp::Le0 => (i <= 0) as u32,
        UnaryOp::Ne0 => (x != 0) as u32,
        UnaryOp::Ge0 => (i >= 0) as u32,
        UnaryOp::Gt0 => (i > 0) as u32,
        UnaryOp::Eq0 => (x == 0) as u32,
    }
}

fn unary_float(op: UnaryOp, x: u32) -> u32 {
    let v = f(x);
    match op {
        UnaryOp::Convert => (v as i32) as u32,
        UnaryOp::Neg => (-v).to_bits(),
        UnaryOp::Abs => v.abs().to_bits(),
        UnaryOp::Lt0 => (v < 0.0) as u32,
        UnaryOp::Le0 => (v <= 0.0) as u32,
        UnaryOp::Eq0 => (v == 0.0) as u32,
        UnaryOp::Ne0 => (v != 0.0) as u32,
        UnaryOp::Ge0 => (v >= 0.0) as u32,
        UnaryOp::Gt0 => (v > 0.0) as u32,
        UnaryOp::Not | UnaryOp::IsZero => 0,
    }
}

fn binary_int(op: BinaryOp, a: u32, b: u32, pc: usize) -> Result<u32, VmError> {
    let (ia, ib) = (a as i32, b as i32);
    Ok(match op {
        BinaryOp::Add => a.wrapping_add(b),
        BinaryOp::Sub => a.wrapping_sub(b),
        BinaryOp::Mul => a.wrapping_mul(b),
        BinaryOp::Div => {
            if b == 0 {
                return Err(VmError::DivideByZero { pc });
            }
            ia.wrapping_div(ib) as u32
        }
        BinaryOp::Rem => {
            if b == 0 {
                return Err(VmError::DivideByZero { pc });
            }
            ia.wrapping_rem(ib) as u32
        }
        BinaryOp::And => a & b,
        BinaryOp::Or => a | b,
        BinaryOp::Xor => a ^ b,
        BinaryOp::Shl => a << (b & 0x1f),
        BinaryOp::Shr => (ia >> (b & 0x1f)) as u32,
        BinaryOp::LogAnd => (a != 0 && b != 0) as u32,
        BinaryOp::LogOr => (a != 0 || b != 0) as u32,
    })
}

fn binary_float(op: BinaryOp, a: u32, b: u32) -> u32 {
    let (x, y) = (f(a), f(b));
    ee_clamp(match op {
        BinaryOp::Add => x + y,
        BinaryOp::Sub => x - y,
        BinaryOp::Mul => x * y,
        BinaryOp::Div => x / y,
        BinaryOp::Rem => x % y,
        _ => 0.0,
    })
    .to_bits()
}

/// Runs `th` until it yields, exits, or returns from its outermost frame.
pub fn run(p: &Program, sh: &mut Shared, th: &mut Thread, env: &mut dyn Env) -> Result<Stop, VmError> {
    let code: Vec<u8> = p.words.iter().flat_map(|w| w.to_le_bytes()).collect();
    run_with_code(p, &code, sh, th, env)
}

/// As [`run`], with the program image supplied (avoids rebuilding it on every call).
pub fn run_with_code(p: &Program, code: &[u8], sh: &mut Shared, th: &mut Thread, env: &mut dyn Env) -> Result<Stop, VmError> {
    let mut budget = sh.step_limit;
    loop {
        if budget == 0 {
            return Err(VmError::StepLimit);
        }
        budget -= 1;
        let pc = th.pc;
        let (insn, len) = decode(p, pc).ok_or_else(|| match p.word(pc) {
            Some(word) => VmError::BadInstruction { pc, word },
            None => VmError::PcOutOfRange(pc),
        })?;
        env.trace(&TraceState { thread: th.id, pc, depth: th.values.len(), fp: th.fp, top: th.values.last().copied() }, &insn);
        th.pc = pc + len;
        macro_rules! pop {
            () => {
                th.values.pop().ok_or(VmError::OperandStackUnderflow)?
            };
        }
        macro_rules! push {
            ($v:expr) => {{
                if th.values.len() >= th.value_cap {
                    return Err(VmError::OperandStackOverflow);
                }
                th.values.push($v)
            }};
        }
        match insn {
            Insn::PushInt(v) => push!(v),
            Insn::PushFloat(v) => push!(v.to_bits()),
            Insn::Lea(o) => {
                let a = operand_addr(code, sh, th, o, env)?;
                push!(a)
            }
            Insn::Load(o) => {
                let a = operand_addr(code, sh, th, o, env)?;
                let v = read_u32(code, sh, th, env, a)?;
                push!(v)
            }
            Insn::Store(o) => {
                let a = operand_addr(code, sh, th, o, env)?;
                let v = pop!();
                write_u32(code, sh, th, env, a, v)?;
            }
            Insn::CopyTo { dst, size } => {
                let src = pop!();
                let a = operand_addr(code, sh, th, dst, env)?;
                copy(code, sh, th, env, a, src, size as usize)?;
            }
            Insn::LoadField(off) => {
                let base = pop!();
                let v = read_u32(code, sh, th, env, base.wrapping_add(off as u32))?;
                push!(v)
            }
            Insn::StoreIndirect => {
                let v = pop!();
                let a = pop!();
                write_u32(code, sh, th, env, a, v)?;
            }
            Insn::CopyIndirect { size } => {
                let src = pop!();
                let dst = pop!();
                copy(code, sh, th, env, dst, src, size as usize)?;
            }
            Insn::Unary { float, op } => {
                let x = pop!();
                push!(if float { unary_float(op, x) } else { unary_int(op, x) })
            }
            Insn::Binary { float, op } => {
                let b = pop!();
                let a = pop!();
                push!(if float { binary_float(op, a, b) } else { binary_int(op, a, b, pc)? })
            }
            Insn::Jump(t) => th.pc = t,
            Insn::JumpIfZero(t) => {
                if pop!() == 0 {
                    th.pc = t
                }
            }
            Insn::JumpIfNonZero(t) => {
                if pop!() != 0 {
                    th.pc = t
                }
            }
            Insn::Call { frame_words, target } => {
                let n = frame_words as usize * 4;
                if th.fp + n > th.frame.len() || n < 8 {
                    return Err(VmError::FrameOverflow);
                }
                th.fp += n;
                th.frame[th.fp - 4..th.fp].copy_from_slice(&(th.pc as u32).to_le_bytes());
                th.frame[th.fp - 8..th.fp - 4].copy_from_slice(&(frame_words as u32).to_le_bytes());
                th.pc = target;
            }
            Insn::Yield => return Ok(Stop::Yield),
            Insn::Exit => return Ok(Stop::Exit),
            Insn::Ret => {
                let ret = u32::from_le_bytes(th.frame[th.fp - 4..th.fp].try_into().unwrap()) as usize;
                let n = u32::from_le_bytes(th.frame[th.fp - 8..th.fp - 4].try_into().unwrap()) as usize;
                th.pc = ret;
                th.fp = th.fp.checked_sub(n * 4).ok_or(VmError::FrameOverflow)?;
                if ret == 0 {
                    return Ok(Stop::Done);
                }
            }
            Insn::Pop => {
                pop!();
            }
            Insn::Dup => {
                let v = pop!();
                push!(v);
                push!(v)
            }
            Insn::Sin => {
                let x = f(pop!());
                push!(crate::trig::sin(wrap_angle(x)).to_bits())
            }
            Insn::Cos => {
                let x = f(pop!());
                push!(crate::trig::cos(wrap_angle(x)).to_bits())
            }
            Insn::DegToRad => {
                let x = f(pop!());
                push!(((((x * PI) / 180.0) + PI) % TWO_PI - PI).to_bits())
            }
            Insn::RadToDeg => {
                let x = f(pop!());
                push!(((x * 180.0) / PI).to_bits())
            }
            Insn::Host { group, index } => {
                let (n, ret) = env.signature(group, index).ok_or(VmError::UnknownHost { group, index })?;
                if n > MAX_ARGS {
                    return Err(VmError::UnknownHost { group, index });
                }
                let mut args = [0u32; MAX_ARGS];
                for i in (0..n).rev() {
                    args[i] = pop!();
                }
                let frame_addr = sh.layout.frames + th.id * sh.layout.frame_stride;
                let mut mem = Mem { code, sh, frame: &mut th.frame, frame_addr };
                let r = env.host_call(p, &mut mem, group, index, &args[..n])?;
                if ret {
                    push!(r.unwrap_or(0))
                }
            }
        }
    }
}

fn operand_addr(code: &[u8], sh: &mut Shared, th: &mut Thread, o: Operand, env: &mut dyn Env) -> Result<u32, VmError> {
    let off = o.offset as u32;
    let l = sh.layout;
    Ok(match o.base {
        Base::Local => l.frames + th.id * l.frame_stride + th.fp as u32 + off,
        Base::Global => l.globals + off,
        Base::LocalPtr => {
            let slot = l.frames + th.id * l.frame_stride + th.fp as u32;
            read_u32(code, sh, th, env, slot)?.wrapping_add(off)
        }
        Base::Code => l.code + off * 2,
        Base::Absolute => off,
    })
}

fn with_mem<R>(
    code: &[u8],
    sh: &mut Shared,
    th: &mut Thread,
    f: impl FnOnce(&mut Mem<'_>) -> Result<R, Outside>,
) -> Result<R, Outside> {
    let frame_addr = sh.layout.frames + th.id * sh.layout.frame_stride;
    let mut mem = Mem { code, sh, frame: &mut th.frame, frame_addr };
    f(&mut mem)
}

fn read_u32(code: &[u8], sh: &mut Shared, th: &mut Thread, env: &mut dyn Env, addr: u32) -> Result<u32, VmError> {
    match with_mem(code, sh, th, |m| m.read_u32(addr)) {
        Ok(v) => Ok(v),
        Err(Outside::Ext(a)) => {
            let mut b = [0; 4];
            if env.ext_read(a, &mut b) { Ok(u32::from_le_bytes(b)) } else { Err(VmError::BadAddress(a)) }
        }
        Err(Outside::Fault(a)) => Err(VmError::BadAddress(a)),
    }
}

fn write_u32(code: &[u8], sh: &mut Shared, th: &mut Thread, env: &mut dyn Env, addr: u32, v: u32) -> Result<(), VmError> {
    match with_mem(code, sh, th, |m| m.write_u32(addr, v)) {
        Ok(()) => Ok(()),
        Err(Outside::Ext(a)) => {
            if env.ext_write(a, &v.to_le_bytes()) { Ok(()) } else { Err(VmError::BadAddress(a)) }
        }
        Err(Outside::Fault(a)) => Err(VmError::BadAddress(a)),
    }
}

fn copy(code: &[u8], sh: &mut Shared, th: &mut Thread, env: &mut dyn Env, dst: u32, src: u32, n: usize) -> Result<(), VmError> {
    let mut buf = vec![0u8; n];
    match with_mem(code, sh, th, |m| m.read(src, &mut buf)) {
        Ok(()) => {}
        Err(Outside::Ext(a)) => {
            if !env.ext_read(a, &mut buf) {
                return Err(VmError::BadAddress(a));
            }
        }
        Err(Outside::Fault(a)) => return Err(VmError::BadAddress(a)),
    }
    match with_mem(code, sh, th, |m| m.write(dst, &buf)) {
        Ok(()) => Ok(()),
        Err(Outside::Ext(a)) => {
            if env.ext_write(a, &buf) { Ok(()) } else { Err(VmError::BadAddress(a)) }
        }
        Err(Outside::Fault(a)) => Err(VmError::BadAddress(a)),
    }
}
