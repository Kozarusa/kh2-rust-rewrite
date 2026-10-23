//! The harness generated oracle tests run on. A [`Spec`] says how to call one original function (its integer and
//! float arguments, the blocks its pointer arguments point at with their typed fields, the globals it reads or writes,
//! the callees replaced by stubs); [`fuzz`] generates cases, runs the original in kh2-ee and the port on copies of the
//! same inputs, and compares the return value, every block and every global byte by byte (float fields with a
//! tolerance). The port sees blocks as byte slices in the original's layout (RULES C7).

use kh2_ee::{Ee, Fault, reg};

/// Where blocks are laid out in the interpreter's memory (one 64 KB slot each).
pub const ARENA: u32 = 0x0180_0000;
pub const SLOT: u32 = 0x0001_0000;
pub const STACK: u32 = 0x01ff_0000;
const GUARD: u8 = 0xa5;
/// Cases whose whole-memory writes are audited.
const AUDIT: u32 = 32;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Int,
    Float,
    /// A pointer field: the harness points it at the sub-block of the same field, or leaves it zero.
    Ptr,
}

/// One typed field of a block: offset, width in bytes, kind.
pub type FieldSpec = (u32, u8, Kind);

#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub size: u32,
    pub fields: &'static [FieldSpec],
    /// Sub-blocks behind pointer fields: (field offset, block).
    pub subs: &'static [(u32, Block)],
}

#[derive(Clone, Copy, Debug)]
pub enum Arg {
    /// A 32-bit integer (sign-extended into the 64-bit register as the EE ABI passes it); the constants the function
    /// compares against are tried with their neighbours.
    Int(&'static [i32]),
    /// A 32-bit integer drawn from lo..=hi (an index the original does not bound-check).
    Range(i32, i32),
    /// A pointer to a block.
    Ptr(Block),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ret {
    None,
    /// Low 32 bits of v0.
    Int,
    /// All 64 bits of v0.
    Int64,
    /// f0.
    Float,
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub addr: u32,
    pub args: &'static [Arg],
    pub floats: u8,
    pub ret: Ret,
    /// Global ranges (address, length, randomise): compared after the call; randomised before it when the flag is set,
    /// otherwise left as the executable's image has them.
    pub globals: &'static [(u32, u32, bool)],
    /// Callees replaced by a stub that returns 0 (the port must model them the same way).
    pub stubs: &'static [u32],
    /// Whether pointer fields are sometimes null (false when the original reads through them unconditionally).
    pub nulls: bool,
}

/// One case's inputs, and after the call the outputs the port wrote.
#[derive(Clone, Debug)]
pub struct Case {
    /// Integer arguments; a pointer argument holds the index of its block.
    pub ints: Vec<u64>,
    pub floats: Vec<f32>,
    /// One per pointer argument, in argument order, then the sub-blocks in depth-first order.
    pub blocks: Vec<Vec<u8>>,
    pub globals: Vec<Vec<u8>>,
}

impl Case {
    pub fn int(&self, i: usize) -> i32 {
        self.ints[i] as i32
    }
    pub fn float(&self, i: usize) -> f32 {
        self.floats[i]
    }
    pub fn block(&mut self, i: usize) -> &mut [u8] {
        &mut self.blocks[i]
    }
    /// The sub-block a pointer field of block `i` holds (its handle), or `None` for null.
    pub fn deref(&self, i: usize, field: usize) -> Option<usize> {
        let h = u32::from_le_bytes(self.blocks[i][field..field + 4].try_into().unwrap());
        (h != 0).then(|| h as usize - 1)
    }
    pub fn global(&mut self, i: usize) -> &mut [u8] {
        &mut self.globals[i]
    }
}

/// What the port returns.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Out {
    pub v0: u64,
    pub f0: f32,
}

impl Out {
    pub fn int(v: i64) -> Out {
        Out { v0: v as u64, f0: 0.0 }
    }
    pub fn float(f: f32) -> Out {
        Out { v0: 0, f0: f }
    }
    pub fn none() -> Out {
        Out::default()
    }
}

/// A small deterministic generator (xorshift64*), so a failing case number reproduces.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }
    pub fn word(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.word() % n.max(1)
    }
    pub fn int(&mut self, consts: &[i32]) -> i32 {
        match self.below(10) {
            0..=2 => self.below(40) as i32 - 8,
            3..=5 if !consts.is_empty() => consts[self.below(consts.len() as u64) as usize].wrapping_add(self.below(3) as i32 - 1),
            6 => [0, 1, -1, i32::MAX, i32::MIN, 0xff, 0xffff, 0x8000][self.below(8) as usize],
            _ => self.word() as i32,
        }
    }
    pub fn float(&mut self) -> f32 {
        match self.below(10) {
            0..=1 => [0.0, 1.0, -1.0, 0.5, -0.5, 2.0, 100.0, -100.0, 1e-4, std::f32::consts::PI][self.below(10) as usize],
            _ => {
                let scale = [1.0f32, 10.0, 1000.0, 1e5][self.below(4) as usize];
                ((self.word() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0) * scale
            }
        }
    }
}

/// Fills a block tree depth-first; returns the block's index. Pointer fields with a sub-block hold the sub-block's
/// handle (index + 1), or 0 (null) in one case of eight.
fn fill(rng: &mut Rng, b: &Block, out: &mut Vec<Vec<u8>>, nulls: bool) -> usize {
    let mut bytes: Vec<u8> = (0..b.size).map(|_| rng.word() as u8).collect();
    for &(off, w, k) in b.fields {
        let o = off as usize;
        let Some(dst) = bytes.get_mut(o..o + w as usize) else { continue };
        match k {
            Kind::Float => {
                for c in dst.chunks_mut(4) {
                    let f = rng.float().to_le_bytes();
                    let n = c.len();
                    c.copy_from_slice(&f[..n]);
                }
            }
            Kind::Int => {
                let v = rng.int(&[]).to_le_bytes();
                for (i, c) in dst.iter_mut().enumerate() {
                    *c = if i < 4 { v[i] } else { 0 };
                }
            }
            Kind::Ptr => dst.fill(0),
        }
    }
    let idx = out.len();
    out.push(bytes);
    for (field, sub) in b.subs {
        let s = fill(rng, sub, out, nulls);
        let h = if nulls && rng.below(8) == 0 { 0 } else { s as u32 + 1 };
        let f = *field as usize;
        out[idx][f..f + 4].copy_from_slice(&h.to_le_bytes());
    }
    idx
}

/// The block specs in the depth-first order of [`fill`].
fn flatten<'a>(b: &'a Block, out: &mut Vec<&'a Block>) {
    out.push(b);
    for (_, sub) in b.subs {
        flatten(sub, out);
    }
}

pub fn block_addr(i: usize) -> u32 {
    ARENA + i as u32 * SLOT
}

/// Writes every block at its slot, with the handles in pointer fields turned into the slots' addresses.
fn lay(ee: &mut Ee, flat: &[&Block], blocks: &[Vec<u8>]) {
    for (i, b) in flat.iter().enumerate() {
        let at = block_addr(i);
        ee.write(at, &blocks[i]);
        ee.write(at + b.size, &[GUARD; 0x100]);
        for (field, _) in b.subs {
            let f = *field as usize;
            let h = u32::from_le_bytes(blocks[i][f..f + 4].try_into().unwrap());
            ee.w32(at + field, if h == 0 { 0 } else { block_addr(h as usize - 1) });
        }
    }
}

/// Reads every block back, with pointer fields that point at a slot turned back into handles.
fn read_back(ee: &mut Ee, flat: &[&Block]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for (i, b) in flat.iter().enumerate() {
        let at = block_addr(i);
        let mut bytes: Vec<u8> = (0..b.size).map(|k| ee.r8(at + k)).collect();
        if let Some(o) = (0..0x100).find(|&k| ee.r8(at + b.size + k) != GUARD) {
            panic!("the original wrote past the block of {:#x} bytes at {at:#x} (+{:#x}): the spec's block size is too small", b.size, b.size + o);
        }
        for &(off, _, k) in b.fields {
            let o = off as usize;
            if k == Kind::Ptr && o + 4 <= bytes.len() {
                let a = u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
                let h = if a == 0 {
                    0
                } else if a >= ARENA && (a - ARENA).is_multiple_of(SLOT) && ((a - ARENA) / SLOT) < flat.len() as u32 {
                    (a - ARENA) / SLOT + 1
                } else {
                    a
                };
                bytes[o..o + 4].copy_from_slice(&h.to_le_bytes());
            }
        }
        out.push(bytes);
    }
    out
}

fn kinds(b: &Block, out: &mut Vec<Vec<Kind>>) {
    let mut k = vec![Kind::Int; b.size as usize];
    for &(off, w, kind) in b.fields {
        for i in off..(off + w as u32).min(b.size) {
            k[i as usize] = kind;
        }
    }
    out.push(k);
    for (_, sub) in b.subs {
        kinds(sub, out);
    }
}

fn hook_zero(e: &mut Ee) {
    e.gpr[reg::V0] = 0;
}

fn feq(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a - b).abs() <= 1e-5 * (1.0 + a.abs().max(b.abs()))
}

fn fault(f: &Fault) -> String {
    match f {
        Fault::Unimplemented { pc, word, what } => format!("unimplemented {what} {word:08x} at {pc:08x}"),
        Fault::Memory { pc, addr } => format!("bad address {addr:08x} at {pc:08x}"),
        Fault::Runaway { pc } => format!("runaway at {pc:08x}"),
    }
}

/// One case's outcome in the original: return values, blocks, globals.
type Outcome = (Out, Vec<Vec<u8>>, Vec<Vec<u8>>);

/// Measured on the last run: interpreter steps and cases.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub cases: u32,
    pub steps: u64,
    pub secs: f64,
}

/// Runs `cases` generated cases of `spec` against `port`. Returns `None` (and prints a note) without a disc, or panics
/// without one in gate mode (`KH2_REQUIRE_DISC=1`).
/// Panics with the case, its inputs and the first difference when the port disagrees with the original.
pub fn fuzz(spec: &Spec, cases: u32, port: impl FnMut(&mut Case) -> Out) -> Option<Stats> {
    let elf = crate::exe_or_skip(&format!("oracle test of {:#x}", spec.addr))?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(fuzz_with(&mut ee, spec, cases, 0x6b68_3272, port))
}

pub fn fuzz_with(ee: &mut Ee, spec: &Spec, cases: u32, seed: u64, mut port: impl FnMut(&mut Case) -> Out) -> Stats {
    for &s in spec.stubs {
        ee.hooks.insert(s, hook_zero);
    }
    let image: Vec<Vec<u8>> = spec.globals.iter().map(|&(a, n, _)| (0..n).map(|i| ee.r8(a + i)).collect()).collect();
    let mut kind = Vec::new();
    let mut flat = Vec::new();
    for a in spec.args {
        if let Arg::Ptr(b) = a {
            kinds(b, &mut kind);
            flatten(b, &mut flat);
        }
    }
    let mut rng = Rng::new(seed);
    let t0 = std::time::Instant::now();
    let steps0 = ee.steps;
    let mut first: Option<Outcome> = None;
    let mut varied = false;
    for n in 0..cases {
        // Inputs.
        let mut case = Case { ints: Vec::new(), floats: Vec::new(), blocks: Vec::new(), globals: Vec::new() };
        let mut regs = Vec::new();
        for a in spec.args {
            match a {
                Arg::Int(c) => {
                    let v = rng.int(c) as i64 as u64;
                    case.ints.push(v);
                    regs.push(v);
                }
                Arg::Range(lo, hi) => {
                    let v = (*lo as i64 + rng.below((*hi as i64 - *lo as i64 + 1) as u64) as i64) as u64;
                    case.ints.push(v);
                    regs.push(v);
                }
                Arg::Ptr(b) => {
                    let i = fill(&mut rng, b, &mut case.blocks, spec.nulls);
                    case.ints.push(i as u64);
                    regs.push(block_addr(i) as u64);
                }
            }
        }
        case.floats = (0..spec.floats).map(|_| rng.float()).collect();
        for (i, &(_, len, random)) in spec.globals.iter().enumerate() {
            case.globals.push(if random { (0..len).map(|_| rng.word() as u8).collect() } else { image[i].clone() });
        }
        // The original.
        lay(ee, &flat, &case.blocks);
        for (i, &(a, _, _)) in spec.globals.iter().enumerate() {
            ee.write(a, &case.globals[i]);
        }
        ee.fpr[0] = 0.0;
        // The first cases also check that the original writes nothing outside its blocks, the spec's globals and the stack.
        let before = (n < AUDIT).then(|| ee.ram.clone());
        let v0 = ee.call(spec.addr, &regs, &case.floats, STACK).unwrap_or_else(|f| panic!("case {n}: the original faulted: {}", fault(&f)));
        if let Some(before) = before {
            let inside = |a: u32| {
                (ARENA..ARENA + flat.len() as u32 * SLOT).contains(&a)
                    || (STACK - 0x10000..STACK + 0x100).contains(&a)
                    || spec.globals.iter().any(|&(g, len, _)| (g..g + len).contains(&a))
            };
            if let Some(a) = (0..ee.ram.len()).find(|&i| before[i] != ee.ram[i] && !inside(i as u32)) {
                panic!("case {n}: the original wrote {a:#x}, which the spec does not list (add it to `globals` or a block)");
            }
        }
        let game = Out { v0, f0: ee.fpr[0] };
        let gblocks = read_back(ee, &flat);
        let gglobals: Vec<Vec<u8>> = spec.globals.iter().map(|&(a, len, _)| (0..len).map(|i| ee.r8(a + i)).collect()).collect();
        match &first {
            None => first = Some((game, gblocks.clone(), gglobals.clone())),
            Some((o, b, g)) => varied |= o.v0 != game.v0 || o.f0.to_bits() != game.f0.to_bits() || *b != gblocks || *g != gglobals,
        }
        // The port.
        let input = case.clone();
        let mine = port(&mut case);
        let show = || format!("case {n}: ints {:x?} floats {:?}", input.ints.iter().map(|&v| v as u32).collect::<Vec<_>>(), input.floats);
        match spec.ret {
            Ret::None => {}
            Ret::Int => assert_eq!(game.v0 as u32, mine.v0 as u32, "{}: v0 game {:#x} port {:#x}", show(), game.v0 as u32, mine.v0 as u32),
            Ret::Int64 => assert_eq!(game.v0, mine.v0, "{}: v0", show()),
            Ret::Float => assert!(feq(game.f0, mine.f0), "{}: f0 game {} port {}", show(), game.f0, mine.f0),
        }
        for (b, (g, m)) in gblocks.iter().zip(&case.blocks).enumerate() {
            let mut o = 0;
            while o < g.len() {
                let k = kind.get(b).and_then(|k| k.get(o)).copied().unwrap_or(Kind::Int);
                if k == Kind::Float && o % 4 == 0 && o + 4 <= g.len() {
                    let (x, y) = (f32::from_le_bytes(g[o..o + 4].try_into().unwrap()), f32::from_le_bytes(m[o..o + 4].try_into().unwrap()));
                    assert!(feq(x, y), "{}: block {b} float at +{o:#x}: game {x} port {y}", show());
                    o += 4;
                } else {
                    assert_eq!(g[o], m[o], "{}: block {b} byte +{o:#x}: game {:02x} port {:02x}", show(), g[o], m[o]);
                    o += 1;
                }
            }
        }
        for (i, (g, m)) in gglobals.iter().zip(&case.globals).enumerate() {
            if let Some(o) = (0..g.len()).find(|&o| g[o] != m[o]) {
                panic!("{}: global {:#x} byte +{o:#x}: game {:02x} port {:02x}", show(), spec.globals[i].0, g[o], m[o]);
            }
        }
    }
    assert!(
        varied,
        "every case gave the same result: the test checks nothing (widen the inputs or the compared outputs)"
    );
    Stats { cases, steps: ee.steps - steps0, secs: t0.elapsed().as_secs_f64() }
}
