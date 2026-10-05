//! Tests on small hand-assembled scripts (no disc needed): header parsing, instruction semantics,
//! the call convention and host calls.

use kh2_bdx::program::{Error, Program};
use kh2_bdx::vm::{self, Env, Mem, Shared, Stop, Thread, VmError};
use kh2_bdx::{analyze, decode};

/// Word-level assembler. Offsets are word indexes counted from file byte 0x10, like the game's.
#[derive(Default)]
struct Asm {
    w: Vec<u16>,
}

#[allow(dead_code)]
impl Asm {
    fn here(&self) -> usize {
        self.w.len()
    }
    fn raw(&mut self, w: &[u16]) -> &mut Self {
        self.w.extend_from_slice(w);
        self
    }
    fn push_i(&mut self, v: i32) -> &mut Self {
        self.raw(&[0x0000, v as u16, (v >> 16) as u16])
    }
    fn push_f(&mut self, v: f32) -> &mut Self {
        let b = v.to_bits();
        self.raw(&[0x0010, b as u16, (b >> 16) as u16])
    }
    fn load_local(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0030, off])
    }
    fn store_local(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0001, off])
    }
    fn load_global(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0070, off])
    }
    fn store_global(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0041, off])
    }
    fn lea_global(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0060, off])
    }
    fn lea_code(&mut self, word_off: u16) -> &mut Self {
        self.raw(&[0x00e0, word_off])
    }
    fn host(&mut self, g: u16, i: u16) -> &mut Self {
        self.raw(&[(g << 6) | 0xa, i])
    }
    fn yield_(&mut self) -> &mut Self {
        self.raw(&[0x0009])
    }
    fn ret(&mut self) -> &mut Self {
        self.raw(&[0x0089])
    }
    fn pop(&mut self) -> &mut Self {
        self.raw(&[0x00c9])
    }
    /// Relative call; `target` is an absolute word index.
    fn call(&mut self, frame_words: u16, target: usize) -> &mut Self {
        let rel = target as i32 - (self.here() as i32 + 2);
        self.raw(&[(frame_words << 6) | 8, rel as i16 as u16])
    }
    fn jmp(&mut self, target: usize) -> &mut Self {
        let rel = target as i32 - (self.here() as i32 + 2);
        self.raw(&[0x0007, rel as i16 as u16])
    }
    fn jz_placeholder(&mut self) -> usize {
        self.raw(&[0x0047, 0]);
        self.here() - 1
    }
    fn patch_jump(&mut self, at: usize, target: usize) {
        self.w[at] = (target as i32 - (at as i32 + 1)) as i16 as u16;
    }
    fn string(&mut self, s: &str) -> u16 {
        let at = self.here() as u16;
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        if b.len() % 2 == 1 {
            b.push(0);
        }
        for c in b.chunks(2) {
            self.w.push(u16::from_le_bytes([c[0], c[1]]));
        }
        at
    }
}

/// Builds a file image: name, sizes, entry table, code. Entry `pc`s are absolute word indexes; the
/// code is placed right after the table and `build` shifts nothing, so assemble with `base()` in mind.
fn image(entries: &[(u32, u32)], code: &Asm, globals: u32) -> Vec<u8> {
    let mut b = Vec::new();
    let mut name = b"synthetic".to_vec();
    name.resize(16, 0);
    b.extend(name);
    b.extend_from_slice(&globals.to_le_bytes());
    b.extend_from_slice(&0x200u32.to_le_bytes());
    b.extend_from_slice(&0x200u32.to_le_bytes());
    for (id, pc) in entries {
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&pc.to_le_bytes());
    }
    b.extend_from_slice(&[0; 8]);
    while (b.len() - 0x10) / 2 < base(entries.len()) {
        b.push(0);
    }
    for w in &code.w {
        b.extend_from_slice(&w.to_le_bytes());
    }
    b
}

/// Word index of the first code word for a table of `n` entries (header words 0..6, entries, terminator).
fn base(n: usize) -> usize {
    6 + 4 * n + 4
}

/// An assembler positioned at the code base for `n` entries.
fn asm(n: usize) -> Asm {
    Asm { w: vec![0; base(n)] }
}

fn finish(a: Asm, entries: &[(u32, u32)], globals: u32) -> Program {
    let img = {
        // The assembler already holds the padding words; strip them because `image` re-adds them.
        let mut code = Asm::default();
        code.w = a.w[base(entries.len())..].to_vec();
        image(entries, &code, globals)
    };
    Program::parse(&img).expect("parse")
}

struct TestEnv {
    calls: Vec<(u16, u16, Vec<u32>)>,
    reply: u32,
}

impl Env for TestEnv {
    fn signature(&self, g: u16, i: u16) -> Option<(usize, bool)> {
        match (g, i) {
            (9, 1) => Some((2, true)),
            (9, 2) => Some((0, false)),
            _ => None,
        }
    }
    fn host_call(&mut self, _p: &Program, _m: &mut Mem<'_>, g: u16, i: u16, a: &[u32]) -> Result<Option<u32>, VmError> {
        self.calls.push((g, i, a.to_vec()));
        Ok((i == 1).then_some(self.reply))
    }
}

fn run_entry(p: &Program, pc: usize, args: &[u32], env: &mut TestEnv) -> (Result<Stop, VmError>, Thread, Shared) {
    let mut sh = Shared::new(p);
    let mut th = Thread::new(p, &mut sh);
    th.start(pc, args);
    let r = vm::run(p, &mut sh, &mut th, env);
    (r, th, sh)
}

#[test]
fn header_and_entries_parse() {
    let mut a = asm(2);
    a.ret();
    let p = finish(a, &[(6, 10), (1, 10)], 0x40);
    assert_eq!(p.name, "synthetic");
    assert_eq!(p.globals, 0x40);
    assert_eq!(p.entry(1), Some(10));
    assert_eq!(p.entry(6), Some(10));
    assert_eq!(p.entry(3), None);
}

#[test]
fn bad_headers_are_rejected() {
    assert_eq!(Program::parse(&[0; 8]).unwrap_err(), Error::Truncated);
    let mut odd = vec![0u8; 0x31];
    odd[0] = b'x';
    assert_eq!(Program::parse(&odd).unwrap_err(), Error::OddLength);
    // An entry pointing outside the code.
    let mut b = vec![0u8; 0x20];
    b[0x1c..0x20].copy_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&0x100u32.to_le_bytes());
    b.extend_from_slice(&[0; 8]);
    assert!(matches!(Program::parse(&b), Err(Error::EntryOutOfRange { .. })));
}

#[test]
fn instruction_words_decode() {
    let mut a = asm(1);
    a.push_i(-5).push_f(1.5).load_local(4).store_local(8).raw(&[0x0006, 0x0086, 0x0105, 0x02d5, 0x0089]);
    let p = finish(a, &[(1, base(1) as u32)], 0);
    let at = base(1);
    use kh2_bdx::Insn::*;
    assert_eq!(decode(&p, at).unwrap().0, PushInt(-5i32 as u32));
    assert_eq!(decode(&p, at + 3).unwrap().0, PushFloat(1.5));
    assert!(matches!(decode(&p, at + 6).unwrap().0, Load(_)));
    assert!(matches!(decode(&p, at + 8).unwrap().0, Store(_)));
    assert!(matches!(decode(&p, at + 10).unwrap().0, Binary { float: false, .. }));
    assert!(matches!(decode(&p, at + 11).unwrap().0, Binary { float: false, .. }));
    assert!(matches!(decode(&p, at + 12).unwrap().0, Unary { float: false, .. }));
    assert!(matches!(decode(&p, at + 13).unwrap().0, Unary { float: true, .. }));
    // Major opcodes 11..15 and float variants without an operation are not instructions.
    for w in [0x000b, 0x000f, 0x0035 /* unary on type 3 */, 0x0016 + 0x0300 /* float binary sub 12 */] {
        let mut a = asm(1);
        a.raw(&[w]);
        let p = finish(a, &[(1, base(1) as u32)], 0);
        assert!(decode(&p, base(1)).is_none(), "{w:#06x}");
    }
}

#[test]
fn arithmetic_and_stack() {
    // (7 - 3) * 2 -> host 9.1(result, 0)
    let mut a = asm(1);
    let at = a.here();
    a.push_i(7).push_i(3).raw(&[0x0046]).push_i(2).raw(&[0x0086]).push_i(0).host(9, 1).pop().ret();
    let p = finish(a, &[(1, at as u32)], 0);
    let mut env = TestEnv { calls: vec![], reply: 77 };
    let (r, th, _) = run_entry(&p, at, &[], &mut env);
    assert_eq!(r.unwrap(), Stop::Done);
    assert_eq!(env.calls, vec![(9, 1, vec![8, 0])]);
    let _ = th;
}

#[test]
fn float_ops_and_conversion() {
    let mut a = asm(1);
    let at = a.here();
    // 3.5 * 2.0 = 7.0 -> to int -> host 9.1(7, 1.0 as int bits)
    a.push_f(3.5).push_f(2.0).raw(&[0x0096]).raw(&[0x0055]).push_i(1).host(9, 1).pop().ret();
    let p = finish(a, &[(1, at as u32)], 0);
    let mut env = TestEnv { calls: vec![], reply: 0 };
    run_entry(&p, at, &[], &mut env).0.unwrap();
    assert_eq!(env.calls[0].2, vec![7, 1]);
}

#[test]
fn call_return_and_locals() {
    // main: local0 = arg; call f(local0 + 1); f stores its arg in its own local and returns arg * 2.
    let mut a = asm(1);
    let main = a.here();
    a.store_local(0).load_local(0).push_i(1).raw(&[0x0006]);
    let call_at = a.here();
    a.raw(&[0, 0]); // patched below: call with a 4-word frame
    a.host(9, 1).pop().ret();
    let f = a.here();
    a.store_local(0).load_local(0).push_i(2).raw(&[0x0086]).push_i(0).ret();
    // The callee's result is left on the operand stack: drop the dummy push before returning.
    let rel = f as i32 - (call_at as i32 + 2);
    a.w[call_at] = (4 << 6) | 8;
    a.w[call_at + 1] = rel as i16 as u16;
    let p = finish(a, &[(1, main as u32)], 0);
    let mut env = TestEnv { calls: vec![], reply: 0 };
    let (r, _, _) = run_entry(&p, main, &[20], &mut env);
    assert_eq!(r.unwrap(), Stop::Done);
    // f returned (21 * 2, 0): the host call received them in push order.
    assert_eq!(env.calls[0].2, vec![42, 0]);
}

#[test]
fn yield_resumes_where_it_stopped() {
    let mut a = asm(1);
    let at = a.here();
    a.push_i(1).yield_().push_i(2).host(9, 1).pop().ret();
    let p = finish(a, &[(1, at as u32)], 0);
    let mut env = TestEnv { calls: vec![], reply: 0 };
    let mut sh = Shared::new(&p);
    let mut th = Thread::new(&p, &mut sh);
    th.start(at, &[]);
    assert_eq!(vm::run(&p, &mut sh, &mut th, &mut env).unwrap(), Stop::Yield);
    assert!(env.calls.is_empty());
    assert_eq!(vm::run(&p, &mut sh, &mut th, &mut env).unwrap(), Stop::Done);
    assert_eq!(env.calls[0].2, vec![1, 2]);
}

#[test]
fn errors_are_reported() {
    let mut a = asm(1);
    let at = a.here();
    a.push_i(1).push_i(0).raw(&[0x00c6]); // major 6, sub 3: divide
    let p = finish(a, &[(1, at as u32)], 0);
    let mut env = TestEnv { calls: vec![], reply: 0 };
    assert!(matches!(run_entry(&p, at, &[], &mut env).0, Err(VmError::DivideByZero { .. })));
    let mut a = asm(1);
    let at = a.here();
    a.raw(&[0x0006]); // add with an empty stack
    let p = finish(a, &[(1, at as u32)], 0);
    assert_eq!(run_entry(&p, at, &[], &mut env).0, Err(VmError::OperandStackUnderflow));
    let mut a = asm(1);
    let at = a.here();
    a.host(9, 7);
    let p = finish(a, &[(1, at as u32)], 0);
    assert!(matches!(run_entry(&p, at, &[], &mut env).0, Err(VmError::UnknownHost { .. })));
    let mut a = asm(1);
    let at = a.here();
    a.jmp(at);
    let p = finish(a, &[(1, at as u32)], 0);
    let mut sh = Shared::new(&p);
    sh.step_limit = 1000;
    let mut th = Thread::new(&p, &mut sh);
    th.start(at, &[]);
    assert_eq!(vm::run(&p, &mut sh, &mut th, &mut env), Err(VmError::StepLimit));
}

#[test]
fn analysis_follows_flow_and_flags_bad_code() {
    let mut a = asm(1);
    let at = a.here();
    a.push_i(1);
    let j = a.jz_placeholder();
    a.ret();
    let target = a.here();
    a.ret();
    a.patch_jump(j, target);
    let p = finish(a, &[(1, at as u32)], 0);
    let an = analyze(&p).unwrap();
    assert!(an.code.contains_key(&target));
    // Reachable garbage is an error.
    let mut a = asm(1);
    let at = a.here();
    a.raw(&[0x000f]);
    let p = finish(a, &[(1, at as u32)], 0);
    assert!(matches!(analyze(&p), Err(Error::BadCode { .. })));
}

