//! Oracle tests for the texture-animation rules, the animation object lists and the model unit leaves (L1-P23). kh2-ee runs
//! the original from the user's executable and the port (`kh2_model::texanim::objects`) runs on a byte-for-byte copy of the
//! same memory image; afterwards the return value and every byte of the image are compared, and for the first cases the whole machine is audited for writes outside the image and the stack.
//!
//! The harness is written for this unit instead of `factory gen` because the inputs are linked object lists, container groups
//! with internal offsets and vtable calls, which the generator cannot lay out. It follows the same rules: nothing of the
//! original is stubbed except
//! * the allocator object the owner holds (slots 0 and 1 are small MIPS routines in the image: bump allocation with the allocator
//!   stored before the block, and a log of freed blocks) and the port's `Host` does the same,
//! * the random number source 0x106380 (hooked; both sides take the same pure function of a counter kept in the image),
//! * the event handler 0x188030 (hooked to log its two arguments), and for 0x188bd8 the callback is a logging routine in the
//!   image.
//!
//! Cases come from a seeded generator (a failing case number reproduces).
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::{Rng, STACK};
use kh2_model::texanim::objects::*;

const BASE: u32 = 0x0180_0000;
const SIZE: usize = 0x2_0000;
const ALLOC: u32 = BASE + 0x100;
const ALLOC_VT: u32 = BASE + 0x200;
const ALLOC_STUB: u32 = BASE + 0x300;
const FREE_STUB: u32 = BASE + 0x340;
const CB_STUB: u32 = BASE + 0x380;
const RNG: u32 = BASE + 0x1f0;
const CB_LOG: u32 = BASE + 0x500;
const ACTOR: u32 = BASE + 0x1000;
const OWNER: u32 = BASE + 0x1700;
const DATA: u32 = BASE + 0x2000;
const HEAP: u32 = BASE + 0x8000;
const AUDIT: u32 = 8;
const CASES: u32 = 1500;

// ---- a tiny assembler for the stub routines -------------------------------------------------------------------------

fn lw(rt: u32, off: i32, rs: u32) -> u32 {
    0x8c00_0000 | rs << 21 | rt << 16 | (off as u32 & 0xffff)
}
fn sw(rt: u32, off: i32, rs: u32) -> u32 {
    0xac00_0000 | rs << 21 | rt << 16 | (off as u32 & 0xffff)
}
fn addiu(rt: u32, rs: u32, imm: i32) -> u32 {
    0x2400_0000 | rs << 21 | rt << 16 | (imm as u32 & 0xffff)
}
fn rtype(rs: u32, rt: u32, rd: u32, sa: u32, f: u32) -> u32 {
    rs << 21 | rt << 16 | rd << 11 | sa << 6 | f
}
const JR_RA: u32 = 0x03e0_0008;
const V0: u32 = 2;
const A0: u32 = 4;
const A1: u32 = 5;
const T0: u32 = 8;
const T1: u32 = 9;
const T2: u32 = 10;

fn put_code(m: &mut Mem, at: u32, words: &[u32]) {
    for (i, w) in words.iter().enumerate() {
        m.w32(at + 4 * i as u32, *w);
    }
}

/// A fresh image: the allocator object with its two routines, the logging callback, the owner and an actor pointing at it.
fn fresh() -> Mem {
    let mut m = Mem::new(BASE, SIZE);
    m.w32(ALLOC, ALLOC_VT);
    m.w32(ALLOC + 8, HEAP);
    m.w32(ALLOC_VT, ALLOC_STUB);
    m.w32(ALLOC_VT + 4, FREE_STUB);
    // alloc(a0 = allocator, a1 = size): block = bump + 0x10, allocator stored before it, bump = block + size rounded to 16.
    put_code(
        &mut m,
        ALLOC_STUB,
        &[
            lw(V0, 8, A0),
            addiu(V0, V0, 0x10),
            sw(A0, -4, V0),
            addiu(T0, A1, 15),
            addiu(T1, 0, -16),
            rtype(T0, T1, T0, 0, 0x24),
            rtype(T0, V0, T0, 0, 0x21),
            sw(T0, 8, A0),
            JR_RA,
            0,
        ],
    );
    // free(a0 = allocator, a1 = block): appends the block to the log at allocator +0x10, count at +0xc.
    put_code(
        &mut m,
        FREE_STUB,
        &[lw(T0, 12, A0), rtype(0, T0, T1, 2, 0), rtype(T1, A0, T1, 0, 0x21), sw(A1, 0x10, T1), addiu(T0, T0, 1), sw(T0, 12, A0), JR_RA, 0],
    );
    // callback(a0, a1): appends (a0, a1) to the log at CB_LOG (count first, pairs from +8).
    put_code(
        &mut m,
        CB_STUB,
        &[
            0x3c08_0000 | (CB_LOG >> 16),
            0x3508_0000 | (CB_LOG & 0xffff),
            lw(T1, 0, T0),
            rtype(0, T1, T2, 3, 0),
            rtype(T2, T0, T2, 0, 0x21),
            sw(A0, 8, T2),
            sw(A1, 12, T2),
            addiu(T1, T1, 1),
            sw(T1, 0, T0),
            JR_RA,
            0,
        ],
    );
    m.w32(OWNER + 0x10, ALLOC);
    m.w32(ACTOR + 0x670, OWNER);
    m
}

fn mix(c: u32, lo: u32, hi: u32) -> u32 {
    (c.wrapping_mul(0x9e37_79b1) ^ lo.wrapping_mul(0x85eb_ca6b) ^ hi.wrapping_mul(0xc2b2_ae35)).rotate_left(13).wrapping_mul(0x27d4_eb2f) >> 3
}

/// The stand-in for 0x106380: `lo + (mixed counter) % (hi - lo)`, the counter kept in the image.
fn pick(c: u32, lo: u32, hi: u32) -> i32 {
    let n = (hi as i32).wrapping_sub(lo as i32);
    let r = if n > 0 { (mix(c, lo, hi) % n as u32) as i32 } else { 0 };
    (lo as i32).wrapping_add(r)
}

/// The port's side of the stand-ins.
struct TestHost;

impl Host for TestHost {
    fn alloc(&mut self, m: &mut Mem, allocator: u32, size: u32) -> u32 {
        let block = m.r32(allocator + 8) + 0x10;
        m.w32(block - 4, allocator);
        m.w32(allocator + 8, block + ((size + 15) & !15));
        block
    }
    fn free(&mut self, m: &mut Mem, allocator: u32, block: u32) {
        let n = m.r32(allocator + 0xc);
        m.w32(allocator + 0x10 + 4 * n, block);
        m.w32(allocator + 0xc, n + 1);
    }
    fn rand_between(&mut self, m: &mut Mem, lo: u32, hi: u32) -> i32 {
        let c = m.r32(RNG);
        m.w32(RNG, c + 1);
        pick(c, lo, hi)
    }
}

fn log_call(m: &mut Mem, entry: u32, arg: u32) {
    let n = m.r32(CB_LOG);
    m.w32(CB_LOG + 8 + n * 8, entry);
    m.w32(CB_LOG + 12 + n * 8, arg);
    m.w32(CB_LOG, n + 1);
}

// ---- the comparison rig ---------------------------------------------------------------------------------------------

struct Call {
    ints: Vec<u64>,
    floats: Vec<f32>,
}

fn call(ints: &[u32], floats: &[f32]) -> Call {
    Call { ints: ints.iter().map(|&v| v as i32 as i64 as u64).collect(), floats: floats.to_vec() }
}

#[derive(Clone, Copy, PartialEq)]
enum Ret {
    None,
    I32,
}

struct Run {
    changed: u32,
    distinct: std::collections::HashSet<u64>,
}

/// First difference between two images, byte for byte (kh2-ee's float unit rounds like the host, so the port's float operations
/// give the same bits).
fn first_diff(game: &[u8], port: &[u8]) -> Option<usize> {
    (0..game.len()).find(|&o| game[o] != port[o])
}

fn rig(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.hooks.insert(0x106380, |e: &mut Ee| {
        let (lo, hi) = (e.gpr[reg::A0] as u32, e.gpr[reg::A0 + 1] as u32);
        let c = e.r32(RNG);
        e.w32(RNG, c + 1);
        e.gpr[reg::V0] = pick(c, lo, hi) as i64 as u64 as u128;
    });
    ee.hooks.insert(0x188030, |e: &mut Ee| {
        let (a, b) = (e.gpr[reg::A0] as u32, e.gpr[reg::A0 + 1] as u32);
        let n = e.r32(CB_LOG);
        e.w32(CB_LOG + 8 + n * 8, a);
        e.w32(CB_LOG + 12 + n * 8, b);
        e.w32(CB_LOG, n + 1);
    });
    Some(ee)
}

/// Runs `cases` generated cases of the original at `addr` against the port. `build` builds the image and the call; `port`
/// runs the same call on the port's copy and returns what v0 would hold. Returns `None` without a disc.
fn oracle(
    what: &str,
    addr: u32,
    cases: u32,
    ret: Ret,
    outside_ok: &[(u32, u32)],
    mut build: impl FnMut(&mut Rng) -> (Mem, Call),
    port: impl Fn(&mut Mem, &Call) -> u64,
) -> Option<Run> {
    let mut ee = rig(what)?;
    let mut rng = Rng::new(addr as u64 ^ 0x5eed);
    let mut run = Run { changed: 0, distinct: Default::default() };
    for n in 0..cases {
        let (img, c) = build(&mut rng);
        let mut pm = img.clone();
        ee.write(img.base, &img.bytes);
        let before = (n < AUDIT).then(|| ee.ram.clone());
        let g = match ee.call(addr, &c.ints, &c.floats, STACK) {
            Ok(v) => v,
            Err(f) => panic!("{what} case {n}: the original at {addr:#x} faulted: {f:?} (ints {:x?})", c.ints),
        };
        if let Some(before) = before {
            let inside = |a: u32| {
                (img.base..img.base + img.bytes.len() as u32).contains(&a)
                    || (STACK - 0x10000..STACK + 0x100).contains(&a)
                    || outside_ok.iter().any(|&(s, l)| (s..s + l).contains(&a))
            };
            if let Some(a) = (0..ee.ram.len()).find(|&i| before[i] != ee.ram[i] && !inside(i as u32)) {
                panic!("{what} case {n}: the original wrote {a:#x}, outside the image");
            }
        }
        let p = port(&mut pm, &c);
        match ret {
            Ret::None => {}
            Ret::I32 => assert_eq!(g as u32, p as u32, "{what} case {n}: v0 game {:#x} port {:#x} (ints {:x?} floats {:?})", g as u32, p as u32, c.ints, c.floats),
        }
        let lo = img.base as usize;
        let game = &ee.ram[lo..lo + img.bytes.len()];
        if let Some(o) = first_diff(game, &pm.bytes) {
            panic!(
                "{what} case {n}: image byte {:#x} game {:02x} port {:02x} (ints {:x?} floats {:?}); input had {:02x}",
                img.base as usize + o,
                game[o],
                pm.bytes[o],
                c.ints,
                c.floats,
                img.bytes[o]
            );
        }
        if game != img.bytes.as_slice() {
            run.changed += 1;
        }
        if ret != Ret::None {
            run.distinct.insert(g as u32 as u64);
        }
    }
    eprintln!("{what}: {cases} cases equal, {} changed the image, {} distinct results", run.changed, run.distinct.len());
    assert!(cases <= 20 || run.changed > 0 || run.distinct.len() > 1, "{what}: every case gave the same result: the test checks nothing");
    Some(run)
}

// ---- generators -----------------------------------------------------------------------------------------------------

fn chance(r: &mut Rng, n: u64) -> bool {
    r.below(n) == 0
}
fn choose<T: Copy>(r: &mut Rng, v: &[T]) -> T {
    v[r.below(v.len() as u64) as usize]
}
fn fill(m: &mut Mem, r: &mut Rng, from: u32, len: u32) {
    for o in 0..len {
        m.w8(from + o, r.word() as u8);
    }
}
fn float(r: &mut Rng) -> f32 {
    match r.below(3) {
        0 => choose(r, &[0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 7.25]),
        _ => (r.below(8000) as f32 - 2000.0) / 1000.0,
    }
}

fn put16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn pad4(b: &mut Vec<u8>) {
    while !b.len().is_multiple_of(4) {
        b.push(0);
    }
}

/// The fields of a TEXA body that two objects must share to be linked, and the slot table.
#[derive(Clone)]
struct TexaSpec {
    h0: u16,
    tex: u16,
    extra: u16,
    lo: u16,
    hi: u16,
    count: u16,
    xywh: [u16; 4],
    slots: Vec<u16>,
}

impl TexaSpec {
    fn random(r: &mut Rng) -> TexaSpec {
        let count = r.below(4) as u16;
        let lo = r.below(4) as u16;
        let hi = lo + r.below(4) as u16;
        let slots = (lo..=hi)
            .map(|_| match r.below(4) {
                0 => 0,
                1 => count + 1,
                _ if count > 0 => 1 + r.below(count as u64) as u16,
                _ => 0,
            })
            .collect();
        TexaSpec { h0: r.word() as u16, tex: r.below(3) as u16, extra: r.below(3) as u16, lo, hi, count, xywh: [r.below(64) as u16, r.below(64) as u16, 16, 8], slots }
    }
    fn mutate(&mut self, r: &mut Rng) {
        match r.below(7) {
            0 => self.h0 ^= 1,
            1 => self.tex += 1,
            2 => self.count = self.count.wrapping_add(1),
            3 => {
                let i = r.below(self.slots.len() as u64) as usize;
                self.slots[i] = self.slots[i].wrapping_add(1) % 5;
            }
            4 => self.xywh[r.below(4) as usize] += 1,
            5 => self.lo = self.lo.saturating_sub(1),
            _ => {}
        }
    }
    fn body(&self, r: &mut Rng) -> Vec<u8> {
        let mut b = vec![0u8; 0x28];
        for (o, v) in [(0, self.h0), (2, self.tex), (4, self.extra), (6, 4), (8, self.lo), (0xa, self.hi), (0xc, self.count), (0xe, 3)] {
            put16(&mut b, o, v);
        }
        for (i, v) in self.xywh.iter().enumerate() {
            put16(&mut b, 0x10 + 2 * i, *v);
        }
        let slot_off = b.len();
        for s in &self.slots {
            b.extend_from_slice(&s.to_le_bytes());
        }
        pad4(&mut b);
        let anim_off = b.len();
        b.extend(std::iter::repeat_n(0, self.count as usize * 4));
        for a in 0..self.count as usize {
            pad4(&mut b);
            let at = b.len();
            put32(&mut b, anim_off + 4 * a, at as u32);
            let n = 1 + r.below(3) as usize;
            let entry = |b: &mut Vec<u8>, ctrl: u16, mn: u16, mx: u16, sprite: u16| {
                for v in [ctrl, mn, mx, sprite] {
                    b.extend_from_slice(&v.to_le_bytes());
                }
                for _ in 0..self.extra {
                    b.extend_from_slice(&(r_word_dummy() as u16).to_le_bytes());
                }
            };
            for _ in 0..n {
                let mn = 1 + r.below(3) as u16;
                entry(&mut b, if chance(r, 5) { 1 } else { 0 }, mn, mn + r.below(4) as u16, r.below(3) as u16);
            }
            if chance(r, 4) {
                entry(&mut b, 3, 1, 1, 0);
            } else {
                let jump = (-(n as i16)) << 4;
                entry(&mut b, jump as u16 | 2, 1 + r.below(2) as u16, 2 + r.below(3) as u16, 0);
            }
        }
        pad4(&mut b);
        let sprite_off = b.len();
        b.extend_from_slice(&[0; 8]);
        put32(&mut b, 0x18, slot_off as u32);
        put32(&mut b, 0x1c, anim_off as u32);
        put32(&mut b, 0x20, sprite_off as u32);
        let default = if self.count > 0 && !chance(r, 6) { r.below(self.count as u64 + 1) as u32 } else { r.below(5) as u32 };
        put32(&mut b, 0x24, default);
        b
    }
}

fn r_word_dummy() -> u32 {
    0x5a5a
}

fn block(tag: u32, body: &[u8]) -> Vec<u8> {
    let mut v = tag.to_le_bytes().to_vec();
    v.extend_from_slice(&(body.len() as u32).to_le_bytes());
    v.extend_from_slice(body);
    v
}

fn uvsc_body(r: &mut Rng) -> Vec<u8> {
    let mut b = vec![0u8; 12];
    put32(&mut b, 0, r.below(4) as u32);
    put32(&mut b, 4, (float(r) * 6.0).to_bits());
    put32(&mut b, 8, (float(r) * 6.0).to_bits());
    b
}

/// Where the generator put things (addresses in the image).
#[derive(Default, Clone)]
struct Tree {
    group: u32,
    groups: Vec<u32>,
    plains: Vec<u32>,
    footers: Vec<u32>,
    blocks: Vec<u32>,
    end: u32,
}

struct Builder<'a> {
    m: &'a mut Mem,
    at: u32,
    t: Tree,
    /// When set, every TEXA block is made from this spec (the footers then have the same shape and can be linked).
    twin: Option<TexaSpec>,
}

impl Builder<'_> {
    fn put(&mut self, bytes: &[u8]) -> u32 {
        let at = self.at;
        for (i, b) in bytes.iter().enumerate() {
            self.m.w8(at + i as u32, *b);
        }
        self.at += bytes.len() as u32;
        while !self.at.is_multiple_of(4) {
            self.at += 1;
        }
        at
    }

    /// A footer: up to 4 blocks of TEXA, UVSC or `_DMY`, ended by `_KN5` or by a word that is no tag.
    fn footer(&mut self, r: &mut Rng, pool: &mut Vec<TexaSpec>) -> u32 {
        let start = self.at;
        for _ in 0..r.below(5) {
            let bytes = match r.below(5) {
                0 | 1 => {
                    let spec = if let Some(t) = &self.twin {
                        t.clone()
                    } else if !pool.is_empty() && chance(r, 2) {
                        let mut s = pool[r.below(pool.len() as u64) as usize].clone();
                        if chance(r, 3) {
                            s.mutate(r);
                        }
                        s
                    } else {
                        TexaSpec::random(r)
                    };
                    pool.push(spec.clone());
                    block(TAG_TEXA, &spec.body(r))
                }
                2 | 3 => block(TAG_UVSC, &uvsc_body(r)),
                _ => block(TAG_DMY, &vec![0x77; 4 * r.below(3) as usize]),
            };
            let at = self.put(&bytes);
            self.t.blocks.push(at);
        }
        let end = if chance(r, 6) { block(0x1234_5678, &[]) } else { block(TAG_KN5, &vec![0; 4 * r.below(2) as usize]) };
        let at = self.put(&end);
        self.t.blocks.push(at);
        start
    }

    /// A plain container: 0x40-byte header, `c` palette words, then the footer.
    fn plain(&mut self, r: &mut Rng, pool: &mut Vec<TexaSpec>) -> u32 {
        let base = self.at;
        let c = r.below(4) as u32;
        let mut head = vec![0u8; 0x40 + 4 * c as usize];
        for b in head.iter_mut().skip(8) {
            *b = r.word() as u8;
        }
        put16(&mut head, 0, 0);
        put32(&mut head, 4, c);
        put32(&mut head, 0x20, 0x40);
        self.put(&head);
        let f = self.footer(r, pool);
        assert_eq!(f, base + 0x40 + 4 * c);
        self.t.plains.push(base);
        self.t.footers.push(f);
        base
    }

    /// A group: `-1` header, the section count, a table of offsets from the group, then the sections (the last one may be a
    /// group itself).
    fn group(&mut self, r: &mut Rng, n: u32, depth: u32, pool: &mut Vec<TexaSpec>) -> u32 {
        let base = self.at;
        let mut head = vec![0u8; 8 + 4 * n as usize];
        put16(&mut head, 0, 0xffff);
        put32(&mut head, 4, n);
        self.put(&head);
        for i in 0..n {
            let at = if i + 1 == n && depth < 2 && chance(r, 4) {
                let k = 1 + r.below(3) as u32;
                self.group(r, k, depth + 1, pool)
            } else {
                self.plain(r, pool)
            };
            self.m.w32(base + 8 + 4 * i, at - base);
        }
        self.t.groups.push(base);
        base
    }
}

/// An image with one random group built at DATA.
fn scene_data(r: &mut Rng) -> (Mem, Tree) {
    let mut m = fresh();
    let mut pool = Vec::new();
    let twin = chance(r, 3).then(|| TexaSpec::random(r));
    let mut b = Builder { m: &mut m, at: DATA, t: Tree::default(), twin };
    let n = 1 + r.below(3) as u32;
    let g = b.group(r, n, 0, &mut pool);
    let mut t = b.t.clone();
    t.group = g;
    t.end = b.at;
    assert!(t.end < HEAP - 0x100, "the generated data overran the data area");
    (m, t)
}

/// An image with objects loaded from a random group (and sometimes a second load over the first), their state then scrambled.
fn scene(r: &mut Rng) -> (Mem, Tree) {
    let (mut m, t) = scene_data(r);
    let mut h = TestHost;
    load_group(&mut m, &mut h, OWNER, t.group);
    if chance(r, 4) {
        // A second group appended section by section (a footer attached to the existing list).
        let f = *t.footers.last().unwrap();
        attach_footer(&mut m, &mut h, OWNER, f);
    }
    scramble(&mut m, r);
    (m, t)
}

fn objects(m: &Mem) -> Vec<u32> {
    let mut v = Vec::new();
    let mut p = m.r32(OWNER + 0x38);
    while p != 0 {
        v.push(p);
        p = next_of(m, p);
    }
    v
}

/// Randomises the run-time state of every object of the owner's list.
fn scramble(m: &mut Mem, r: &mut Rng) {
    let objs = objects(m);
    for &o in &objs {
        if kind_of(m, o) == 1 {
            let d = data_of(m, o);
            let flags = choose(r, &[0, 0, 1, 2, 4, 3, 5, 6, 7]);
            m.w32(o + 0x14, flags);
            m.wf32(o + 0x18, choose(r, &[1.0, 1.0, 0.5, 2.0, 1.5]));
            let count = m.r16(d + 0xc) as u32;
            let step = m.r16(d + 4) as u32 * 2 + 8;
            let table = |m: &Mem, r: &mut Rng| {
                if count > 0 && !chance(r, 5) {
                    let a = r.below(count as u64) as u32;
                    d + m.r32(d + m.r32(d + 0x1c) + 4 * a)
                } else {
                    0
                }
            };
            let req = table(m, r);
            m.w32(o + 0x274, req);
            let play = if chance(r, 4) { req } else { table(m, r) };
            m.w32(o + 0x278, play);
            let frame = if play == 0 || chance(r, 8) {
                0
            } else {
                let mut entries = vec![play];
                while m.r16(*entries.last().unwrap()) & 0xf < 2 {
                    let n = entries.last().unwrap() + step;
                    entries.push(n);
                }
                choose(r, &entries)
            };
            m.w32(o + 0x27c, frame);
            m.wf32(o + 0x280, choose(r, &[1.0, 2.0, 3.0, 4.0, 5.0]));
            m.wf32(o + 0x284, float(r).abs() * 2.0);
            m.w32(o + 0x270, choose(r, &[0, 2, 0]));
        } else {
            m.w32(o + 0x14, choose(r, &[0, 0, 1, 2, 3]));
            m.wf32(o + 0x18, choose(r, &[1.0, 2.0, 0.5, 1.0]));
            m.wf32(o + 0xc, float(r).abs());
            m.w32(o + 0x24, r.word() as u32 & 0x00ff_ffff);
            m.w32(o + 0x28, r.word() as u32 & 0x00ff_ffff);
        }
    }
    // Links: a TEXA object may follow another TEXA object (never itself).
    let texa: Vec<u32> = objs.iter().copied().filter(|&o| kind_of(m, o) == 1).collect();
    for &o in &texa {
        let l = if texa.len() > 1 && chance(r, 3) { choose(r, &texa) } else { 0 };
        m.w32(o + 0x20, if l == o { 0 } else { l });
    }
}

fn some_object(m: &Mem, r: &mut Rng) -> u32 {
    let v = objects(m);
    if v.is_empty() || chance(r, 8) { 0 } else { choose(r, &v) }
}

// ---- model unit leaves ----------------------------------------------------------------------------------------------

/// Orig 0x11d1f8: swaps two pairs of words of a model.
#[test]
fn matrix_swap_matches_the_game() {
    let g = |r: &mut Rng| {
        let mut m = fresh();
        let model = HEAP + 0x100 + 16 * r.below(32) as u32;
        fill(&mut m, r, model, 0x110);
        (m, call(&[model], &[]))
    };
    oracle("0x11d1f8 matrix swap", 0x11d1f8, CASES, Ret::None, &[], g, |m, c| {
        matrix_swap(m, c.ints[0] as u32);
        0
    });
}

/// Orig 0x134728 (address of the static object), 0x134738 (command), 0x1347b0 and 0x1347d0 (the two commands) and 0x1337c0
/// (no effect): the static object at 0x354df8 is laid out with random bytes in the executable's data and compared after.
#[test]
fn model_unit_static_commands_match_the_game() {
    let stat = |r: &mut Rng| {
        let mut m = Mem::new(MODEL_UNIT, 0x30);
        for o in 0..0x30 {
            m.w8(MODEL_UNIT + o, r.word() as u8);
        }
        m
    };
    let g = |r: &mut Rng| {
        let m = stat(r);
        let what = choose(r, &[0, 1, 1, 2, -1, 0x10000]);
        let id = choose(r, &[0xffff, 0xffff, 0xfffe, 0, 1, 0x1ffff]);
        (m, call(&[what as u32, id], &[]))
    };
    let out = [(0u32, 0u32)];
    oracle("0x134738 model unit command", 0x134738, CASES, Ret::None, &out, g, |m, c| {
        model_unit_command(m, c.ints[0] as i32, c.ints[1]);
        0
    });
    oracle("0x1347b0 model unit reset", 0x1347b0, 200, Ret::None, &out, |r| (stat(r), call(&[], &[])), |m, _| {
        model_unit_reset(m);
        0
    });
    oracle("0x1347d0 model unit idle", 0x1347d0, 20, Ret::None, &out, |r| (stat(r), call(&[], &[])), |m, _| {
        model_unit_idle(m);
        nothing();
        0
    });
    // 0x134728 returns the static object's address, 0x1337c0 does nothing.
    let Some(run) = oracle("0x134728 model unit address", 0x134728, 20, Ret::I32, &out, |r| (stat(r), call(&[], &[])), |_, _| model_unit_address() as u64) else { return };
    assert_eq!(run.distinct.len(), 1);
    if let Some(run) = oracle("0x1337c0 nothing", 0x1337c0, 20, Ret::None, &out, |r| (stat(r), call(&[], &[])), |_, _| 0) {
        assert_eq!(run.changed, 0);
    }
}

/// Orig 0x134898: the matrix block of the model unit (three quads cleared, ones, the copy chain, the constant quad). The
/// constant quad at 0x354e50 is randomised in the executable's data and given to the port as its `init` argument.
#[test]
fn matrix_block_init_matches_the_game() {
    let Some(mut ee) = rig("0x134898 matrix block") else { return };
    let mut rng = Rng::new(0x134898);
    for n in 0..CASES {
        let mut m = fresh();
        let p = HEAP + 0x100 + 16 * rng.below(32) as u32;
        fill(&mut m, &mut rng, p, 0x80);
        let init = [rng.word() as u32, rng.word() as u32, rng.word() as u32, rng.word() as u32];
        ee.write(m.base, &m.bytes);
        for (i, w) in init.iter().enumerate() {
            ee.w32(0x354e50 + 4 * i as u32, *w);
        }
        ee.call(0x134898, &[p as u64], &[], STACK).expect("the original faulted");
        let mut pm = m.clone();
        matrix_block_init(&mut pm, p, init);
        let game = &ee.ram[m.base as usize..m.base as usize + SIZE];
        if let Some(o) = (0..SIZE).find(|&o| game[o] != pm.bytes[o]) {
            panic!("case {n}: byte {:#x} game {:02x} port {:02x}", BASE as usize + o, game[o], pm.bytes[o]);
        }
        assert_ne!(game, m.bytes.as_slice(), "case {n}: nothing changed");
    }
}

// ---- footer walk ----------------------------------------------------------------------------------------------------

fn any_struct(t: &Tree, r: &mut Rng) -> u32 {
    match r.below(8) {
        0 => 0,
        1 => t.group,
        2 => choose(r, &t.groups),
        3 | 4 => choose(r, &t.plains),
        5 => choose(r, &t.footers),
        _ => choose(r, &t.blocks),
    }
}

/// An address inside the generated data (never null), at any alignment half of the time.
fn any_addr(t: &Tree, r: &mut Rng) -> u32 {
    if chance(r, 2) { choose(r, &t.blocks) } else { DATA + r.below((t.end - DATA) as u64) as u32 }
}

/// Orig 0x132238 (footer start of a container or group), 0x132288 (section of a group), 0x1322e0 and 0x132320 (words at any
/// alignment), 0x132360 (next block), 0x1323a0 (body), 0x1323b0 (find a tag) and 0x132438 (is a known tag).
#[test]
fn footer_walk_matches_the_game() {
    let scene = |r: &mut Rng| scene_data(r);
    oracle("0x132238 footer start", 0x132238, CASES, Ret::I32, &[], |r| {
        let (m, t) = scene(r);
        let p = if chance(r, 6) { choose(r, &t.blocks) } else if chance(r, 8) { 0 } else { let (g, p) = (choose(r, &t.groups), choose(r, &t.plains)); choose(r, &[t.group, t.group, g, p]) };
        (m, call(&[p], &[]))
    }, |m, c| footer_start(m, c.ints[0] as u32) as u64);
    oracle("0x132288 section", 0x132288, CASES, Ret::I32, &[], |r| {
        let (m, t) = scene(r);
        let p = any_struct(&t, r);
        let n = if p != 0 && m.r16(p) == 0xffff { m.r32(p + 4).max(1) } else { 3 };
        (m, call(&[p, r.below(n as u64) as u32], &[]))
    }, |m, c| section(m, c.ints[0] as u32, c.ints[1] as u32) as u64);
    oracle("0x1322e0 word at", 0x1322e0, CASES, Ret::I32, &[], |r| {
        let (m, t) = scene(r);
        (m, call(&[any_addr(&t, r)], &[]))
    }, |m, c| word_at(m, c.ints[0] as u32) as u64);
    oracle("0x132320 block size", 0x132320, CASES, Ret::I32, &[], |r| {
        let (m, t) = scene(r);
        (m, call(&[any_addr(&t, r)], &[]))
    }, |m, c| block_size(m, c.ints[0] as u32) as u64);
    oracle("0x132360 next block", 0x132360, CASES, Ret::I32, &[], |r| {
        let (m, t) = scene(r);
        (m, call(&[choose(r, &t.blocks)], &[]))
    }, |m, c| next_block(m, c.ints[0] as u32) as u64);
    oracle("0x1323a0 block body", 0x1323a0, 100, Ret::I32, &[], |r| {
        let (m, t) = scene(r);
        (m, call(&[any_addr(&t, r)], &[]))
    }, |_, c| block_body(c.ints[0] as u32) as u64);
    oracle("0x1323b0 find block", 0x1323b0, CASES, Ret::I32, &[], |r| {
        let (m, t) = scene(r);
        let tag = choose(r, &[TAG_TEXA, TAG_UVSC, TAG_KN5, TAG_DMY, 0x1234_5678, 0]);
        let p = if chance(r, 8) { 0 } else { choose(r, &t.blocks) };
        let p = if chance(r, 2) && !t.footers.is_empty() { choose(r, &t.footers) } else { p };
        (m, call(&[p, tag], &[]))
    }, |m, c| find_block(m, c.ints[0] as u32, c.ints[1] as u32) as u64);
    oracle("0x132438 is known tag", 0x132438, CASES, Ret::I32, &[], |r| {
        let (mut m, t) = scene(r);
        let p = if chance(r, 8) { 0 } else { any_addr(&t, r) };
        if p != 0 && chance(r, 3) {
            let w = choose(r, &[TAG_TEXA, TAG_UVSC, TAG_KN5, TAG_DMY, TAG_TEXA + 1, TAG_UVSC ^ 0x8000_0000, 0xffff_ffff, 0]);
            for i in 0..4 {
                m.w8(p + i, (w >> (8 * i)) as u8);
            }
        }
        (m, call(&[p], &[]))
    }, |m, c| is_known_tag(m, c.ints[0] as u32) as u64);
}

// ---- the base object ------------------------------------------------------------------------------------------------

fn obj_scene(r: &mut Rng) -> (Mem, u32) {
    let mut m = fresh();
    let obj = HEAP + 0x100 + 16 * r.below(64) as u32;
    fill(&mut m, r, obj - 16, 0x2d0);
    m.w32(obj - 4, ALLOC);
    (m, obj)
}

/// Orig 0x1324a0 (base fields), 0x132520 and 0x132530 (vtable), 0x132540 (vtable and free), the field accessors 0x132560,
/// 0x132568, 0x132578, 0x132580, 0x132588, 0x132590 and 0x132598, 0x1325a0 (speed), 0x1325b0 (64-bit identity), 0x133948 (speed word).
#[test]
fn base_object_matches_the_game() {
    let o = |r: &mut Rng| obj_scene(r);
    oracle("0x1324a0 base init", 0x1324a0, CASES, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob, r.word() as u32], &[])) }, |m, c| {
        base_init(m, c.ints[0] as u32, c.ints[1] as u32);
        0
    });
    oracle("0x132520 base drop", 0x132520, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| {
        base_drop(m, c.ints[0] as u32);
        0
    });
    oracle("0x132530 base destroy", 0x132530, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| {
        base_destroy(m, c.ints[0] as u32);
        0
    });
    oracle("0x132540 base delete", 0x132540, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| {
        base_delete(m, &mut TestHost, c.ints[0] as u32);
        0
    });
    oracle("0x132560 container", 0x132560, 300, Ret::I32, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| container_of(m, c.ints[0] as u32) as u64);
    oracle("0x132568 kind", 0x132568, 300, Ret::I32, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| kind_of(m, c.ints[0] as u32) as u64);
    oracle("0x132578 set block", 0x132578, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob, r.word() as u32], &[])) }, |m, c| {
        set_block(m, c.ints[0] as u32, c.ints[1] as u32);
        0
    });
    oracle("0x132580 next", 0x132580, 300, Ret::I32, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| next_of(m, c.ints[0] as u32) as u64);
    oracle("0x132588 set next", 0x132588, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob, r.word() as u32], &[])) }, |m, c| {
        set_next(m, c.ints[0] as u32, c.ints[1] as u32);
        0
    });
    oracle("0x132590 link", 0x132590, 300, Ret::I32, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| link_of(m, c.ints[0] as u32) as u64);
    oracle("0x132598 set link", 0x132598, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob, r.word() as u32], &[])) }, |m, c| {
        set_link(m, c.ints[0] as u32, c.ints[1] as u32);
        0
    });
    // The speed setter takes the object in a0 and the speed as a float argument.
    oracle("0x1325a0 set speed", 0x1325a0, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[float(r)])) }, |m, c| {
        set_speed(m, c.ints[0] as u32, c.floats[0]);
        0
    });
    oracle("0x133948 speed", 0x133948, 300, Ret::I32, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| speed_bits(m, c.ints[0] as u32) as u64);
    // The identity test compares the whole 64-bit registers.
    oracle("0x1325b0 same object", 0x1325b0, 600, Ret::I32, &[], |r| {
        let a = r.word();
        let b = match r.below(3) {
            0 => a,
            1 => a ^ (1 << (32 + r.below(30))),
            _ => r.word(),
        };
        (fresh(), Call { ints: vec![a, b], floats: vec![] })
    }, |_, c| same_object(c.ints[0], c.ints[1]) as u64);
}

/// Orig 0x1325c0: the first object from a start that answers the slot-3 query, up to a stop object.
#[test]
fn find_answering_matches_the_game() {
    oracle("0x1325c0 find answering", 0x1325c0, CASES, Ret::I32, &[], |r| {
        let (m, _) = scene(r);
        let from = some_object(&m, r);
        let stop = if chance(r, 2) { some_object(&m, r) } else { 0 };
        let (a, b, c) = query_args(r);
        (m, call(&[from, stop, a, b, c], &[]))
    }, |m, c| find_answering(m, c.ints[0] as u32, c.ints[1] as u32, c.ints[2] as u32, c.ints[3] as u32, c.ints[4] as u32) as u64);
}

fn query_args(r: &mut Rng) -> (u32, u32, u32) {
    // (kind, mask, value): kind 1 asks TEXA objects for an animation slot, kind 2 asks UVSC objects for an index.
    let kind = choose(r, &[1, 1, 2, 2, 0, 3]);
    let mask = choose(r, &[0, 0xffff, 0xffff, 0xff, 3, 0xffff_ffff]);
    let value = if chance(r, 3) { r.below(4) as u32 } else { r.below(8) as u32 };
    (kind, mask, value)
}

// ---- construction and destruction -----------------------------------------------------------------------------------

/// Orig 0x1326e8 (TEXA object, with the four GS records 0x1081f8), 0x132798 and 0x1327c0 (its destructors), 0x1333c8 (UVSC
/// object), 0x133430 and 0x133458 (its destructors).
#[test]
fn constructors_and_destructors_match_the_game() {
    let o = |r: &mut Rng| obj_scene(r);
    oracle("0x1326e8 texa construct", 0x1326e8, 600, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob, r.word() as u32], &[])) }, |m, c| {
        texa_construct(m, c.ints[0] as u32, c.ints[1] as u32);
        0
    });
    oracle("0x1333c8 uvsc construct", 0x1333c8, 600, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob, r.word() as u32], &[])) }, |m, c| {
        uvsc_construct(m, c.ints[0] as u32, c.ints[1] as u32);
        0
    });
    oracle("0x132798 texa destroy", 0x132798, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| {
        texa_destroy(m, c.ints[0] as u32);
        0
    });
    oracle("0x1327c0 texa delete", 0x1327c0, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| {
        texa_delete(m, &mut TestHost, c.ints[0] as u32);
        0
    });
    oracle("0x133430 uvsc destroy", 0x133430, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| {
        uvsc_destroy(m, c.ints[0] as u32);
        0
    });
    oracle("0x133458 uvsc delete", 0x133458, 300, Ret::None, &[], |r| { let (m, ob) = o(r); (m, call(&[ob], &[])) }, |m, c| {
        uvsc_delete(m, &mut TestHost, c.ints[0] as u32);
        0
    });
}

// ---- TEXA objects ---------------------------------------------------------------------------------------------------

fn texa_of(m: &Mem, r: &mut Rng) -> u32 {
    let v: Vec<u32> = objects(m).into_iter().filter(|&o| kind_of(m, o) == 1).collect();
    if v.is_empty() { 0 } else { choose(r, &v) }
}

/// A scene whose first call argument is a TEXA object when there is one (else any object).
fn texa_scene(r: &mut Rng) -> (Mem, u32) {
    loop {
        let (m, _) = scene(r);
        let o = texa_of(&m, r);
        if o != 0 {
            return (m, o);
        }
    }
}

fn uvsc_scene(r: &mut Rng) -> (Mem, u32) {
    loop {
        let (m, _) = scene(r);
        let v: Vec<u32> = objects(&m).into_iter().filter(|&o| kind_of(&m, o) == 2).collect();
        if !v.is_empty() {
            let o = choose(r, &v);
            return (m, o);
        }
    }
}

/// Orig 0x132e98 (the body of the footer block), 0x132f00, 0x132f30, 0x132f60 and 0x132f90 (destination x, y, width, height), 0x132ec8
/// (the GS record named by the selector), 0x132ee8 (flip the selector), 0x132c90 (reset), 0x132ca0 (running).
#[test]
fn texa_accessors_match_the_game() {
    let s = |r: &mut Rng| {
        let (mut m, o) = texa_scene(r);
        if chance(r, 5) {
            m.w32(o + 0x10, 0);
        }
        if chance(r, 4) {
            m.w32(o + 0x270, r.word() as u32 & 3);
        }
        (m, o)
    };
    oracle("0x132e98 data", 0x132e98, 400, Ret::I32, &[], |r| { let (m, o) = s(r); (m, call(&[o], &[])) }, |m, c| data_of(m, c.ints[0] as u32) as u64);
    oracle("0x132f00 x", 0x132f00, 400, Ret::I32, &[], |r| { let (m, o) = s(r); (m, call(&[o], &[])) }, |m, c| texa_x(m, c.ints[0] as u32) as u64);
    oracle("0x132f30 y", 0x132f30, 400, Ret::I32, &[], |r| { let (m, o) = s(r); (m, call(&[o], &[])) }, |m, c| texa_y(m, c.ints[0] as u32) as u64);
    oracle("0x132f60 width", 0x132f60, 400, Ret::I32, &[], |r| { let (m, o) = s(r); (m, call(&[o], &[])) }, |m, c| texa_width(m, c.ints[0] as u32) as u64);
    oracle("0x132f90 height", 0x132f90, 400, Ret::I32, &[], |r| { let (m, o) = s(r); (m, call(&[o], &[])) }, |m, c| texa_height(m, c.ints[0] as u32) as u64);
    oracle("0x132ec8 record", 0x132ec8, 400, Ret::I32, &[], |r| { let (m, o) = s(r); (m, call(&[o], &[])) }, |m, c| gs_record(m, c.ints[0] as u32) as u64);
    oracle("0x132ee8 flip record", 0x132ee8, 400, Ret::None, &[], |r| { let (m, o) = s(r); (m, call(&[o], &[])) }, |m, c| {
        flip_record(m, c.ints[0] as u32);
        0
    });
    oracle("0x132c90 reset", 0x132c90, 400, Ret::None, &[], |r| { let (m, o) = s(r); (m, call(&[o], &[])) }, |m, c| {
        texa_reset(m, c.ints[0] as u32);
        0
    });
    oracle("0x132ca0 running", 0x132ca0, 400, Ret::I32, &[], |r| { let (m, o) = s(r); (m, call(&[o], &[])) }, |m, c| texa_running(m, c.ints[0] as u32) as u64);
}

/// Orig 0x132910 (the slot-3 query of a TEXA object; the animation index 0x1327f8 inside it), 0x1329e0 (request and start; the
/// start 0x132950 and the random frame length 0x106380 inside it).
#[test]
fn texa_query_and_request_match_the_game() {
    oracle("0x132910 texa query", 0x132910, CASES, Ret::I32, &[], |r| {
        let (m, o) = texa_scene(r);
        let (k, a, b) = query_args(r);
        (m, call(&[o, k, a, b], &[]))
    }, |m, c| texa_query(m, c.ints[0] as u32, c.ints[1] as u32, c.ints[2] as u32, c.ints[3] as u32) as u64);
    oracle("0x1329e0 texa request", 0x1329e0, CASES, Ret::None, &[], |r| {
        let (m, o) = texa_scene(r);
        let (_, a, b) = query_args(r);
        (m, call(&[o, a, b], &[]))
    }, |m, c| {
        texa_request(m, &mut TestHost, c.ints[0] as u32, c.ints[1] as u32, c.ints[2] as u32);
        0
    });
}

/// Orig 0x132a80: the per-field update of a TEXA object (frame table walk, jumps, stop, linked copy), with the callees
/// 0x132950 (start) and 0x106380 (random length, replaced as described above).
#[test]
fn texa_update_matches_the_game() {
    let run = oracle("0x132a80 texa update", 0x132a80, 3 * CASES, Ret::None, &[], |r| {
        let (m, o) = texa_scene(r);
        let f = float(r).abs() * 3.0;
        let dt = choose(r, &[0.0, 0.5, 1.0, 1.0, 1.0, 2.0, 3.5, 6.0, f]);
        (m, call(&[o], &[dt]))
    }, |m, c| {
        texa_update(m, &mut TestHost, c.ints[0] as u32, c.floats[0]);
        0
    });
    if let Some(run) = run {
        assert!(run.changed > CASES, "too few updates changed the image: {}", run.changed);
    }
}

/// Orig 0x132ce0: whether a TEXA object may take another as its link (same footer shape and slot validity).
#[test]
fn texa_may_link_matches_the_game() {
    let Some(run) = oracle("0x132ce0 may link", 0x132ce0, 3 * CASES, Ret::I32, &[], |r| {
        let (m, a) = texa_scene(r);
        let b = match r.below(6) {
            0 => a,
            1 => choose(r, &objects(&m)),
            _ => texa_of(&m, r),
        };
        (m, call(&[a, b], &[]))
    }, |m, c| texa_may_link(m, c.ints[0] as u32, c.ints[1] as u32) as u64) else { return };
    assert!(run.distinct.len() == 2, "both answers must occur: {:?}", run.distinct);
}

/// Orig 0x1331c8 (the objects of the list that show a frame of one texture) and 0x1332a8 (the lists for all textures).
#[test]
fn texture_lists_match_the_game() {
    let out = BASE + 0x700;
    oracle("0x1331c8 collect", 0x1331c8, CASES, Ret::I32, &[], |r| {
        let (m, _) = scene(r);
        let head = m.r32(OWNER + 0x38);
        let filter = if chance(r, 2) || head == 0 { 0 } else { m.r32(head + 4) };
        let out_arg = if chance(r, 6) { 0 } else { out };
        (m, call(&[head, r.below(3) as u32, out_arg, 8, filter], &[]))
    }, |m, c| collect_for_texture(m, c.ints[0] as u32, c.ints[1] as u32, c.ints[2] as u32, c.ints[4] as u32) as u64);
    oracle("0x1332a8 build lists", 0x1332a8, CASES, Ret::I32, &[], |r| {
        let m = loop {
            let (m, _) = scene(r);
            let mut probe = m.clone();
            if (0..4).all(|t| collect_for_texture(&mut probe, m.r32(OWNER + 0x38), t, 0, 0) <= 8) {
                break m;
            }
        };
        let head = m.r32(OWNER + 0x38);
        let filter = if chance(r, 2) || head == 0 { 0 } else { m.r32(head + 4) };
        (m, call(&[head, out, 0, 1 + r.below(4) as u32, filter], &[]))
    }, |m, c| build_texture_lists(m, c.ints[0] as u32, c.ints[1] as u32, c.ints[3] as u32, c.ints[4] as u32) as u64);
}

// ---- UVSC objects ---------------------------------------------------------------------------------------------------

/// Orig 0x133490 (slot-3 query of a UVSC object), 0x133518 (update), 0x1335d8 (running), 0x1335f8 (body).
#[test]
fn uvsc_matches_the_game() {
    oracle("0x133490 uvsc query", 0x133490, CASES, Ret::I32, &[], |r| {
        let (m, o) = uvsc_scene(r);
        let (k, a, b) = query_args(r);
        let m_ = choose(r, &[a, 0xff, 0xffff_ffff, 3]);
        (m, call(&[o, k, m_, if chance(r, 2) { b } else { r.below(4) as u32 }], &[]))
    }, |m, c| uvsc_query(m, c.ints[0] as u32, c.ints[1], c.ints[2] as u32, c.ints[3] as u32) as u64);
    let run = oracle("0x133518 uvsc update", 0x133518, 3 * CASES, Ret::None, &[], |r| {
        let (m, o) = uvsc_scene(r);
        let f = float(r).abs() * 3.0;
        let dt = choose(r, &[0.0, 0.5, 1.0, 1.0, 2.0, 3.5, f]);
        (m, call(&[o], &[dt]))
    }, |m, c| {
        uvsc_update(m, c.ints[0] as u32, c.floats[0]);
        0
    });
    if let Some(run) = run {
        assert!(run.changed > CASES / 2, "too few updates changed the image: {}", run.changed);
    }
    let s = |r: &mut Rng| {
        let (mut m, o) = uvsc_scene(r);
        if chance(r, 5) {
            m.w32(o + 0x10, 0);
        }
        (m, call(&[o], &[]))
    };
    oracle("0x1335d8 uvsc running", 0x1335d8, 400, Ret::I32, &[], s, |m, c| uvsc_running(m, c.ints[0] as u32) as u64);
    oracle("0x1335f8 uvsc data", 0x1335f8, 400, Ret::I32, &[], s, |m, c| uvsc_data(m, c.ints[0] as u32) as u64);
}

// ---- the owner's list -----------------------------------------------------------------------------------------------

/// Orig 0x11e018 (load a container group), 0x11e1c8 (replace the list by one footer's objects), 0x11e208 (append a footer's
/// objects), 0x11e108 (free a list) and 0x11e3b8 (link two footers' objects); the constructors, the allocator and the vtable
/// slots run inside.
#[test]
fn owner_list_building_matches_the_game() {
    // The owner's list may hold objects from an earlier load.
    let prior = |r: &mut Rng| {
        let (mut m, t) = scene_data(r);
        if chance(r, 2) {
            load_group(&mut m, &mut TestHost, OWNER, t.group);
            // Reset the allocator and the counters as a second game phase would not: keep them, the original sees the same.
        }
        (m, t)
    };
    let Some(run) = oracle("0x11e018 load group", 0x11e018, 2 * CASES, Ret::None, &[], |r| {
        let (m, t) = prior(r);
        let g = match r.below(10) {
            0 => 0,
            1 => choose(r, &t.plains),
            _ => t.group,
        };
        (m, call(&[OWNER, g], &[]))
    }, |m, c| {
        load_group(m, &mut TestHost, c.ints[0] as u32, c.ints[1] as u32);
        0
    }) else { return };
    assert!(run.changed > CASES, "too few loads changed the image");
    oracle("0x11e1c8 replace list", 0x11e1c8, CASES, Ret::None, &[], |r| {
        let (m, t) = prior(r);
        let f = if chance(r, 6) { 0 } else { choose(r, &t.footers) };
        (m, call(&[OWNER, f], &[]))
    }, |m, c| {
        replace_list(m, &mut TestHost, c.ints[0] as u32, c.ints[1] as u32);
        0
    });
    oracle("0x11e208 attach footer", 0x11e208, CASES, Ret::None, &[], |r| {
        let (m, t) = prior(r);
        let f = if chance(r, 6) { 0 } else { choose(r, &t.footers) };
        (m, call(&[OWNER, f], &[]))
    }, |m, c| {
        attach_footer(m, &mut TestHost, c.ints[0] as u32, c.ints[1] as u32);
        0
    });
    oracle("0x11e108 free list", 0x11e108, CASES, Ret::None, &[], |r| {
        let (m, _) = scene(r);
        let from = if chance(r, 4) { some_object(&m, r) } else { m.r32(OWNER + 0x38) };
        (m, call(&[OWNER, from], &[]))
    }, |m, c| {
        free_list(m, &mut TestHost, c.ints[0] as u32, c.ints[1] as u32);
        0
    });
    let Some(run) = oracle("0x11e3b8 link up", 0x11e3b8, 2 * CASES, Ret::None, &[], |r| {
        let (m, t) = scene(r);
        let pick_f = |r: &mut Rng| match r.below(10) {
            0 => 0,
            1 => 0x1234,
            _ => choose(r, &t.footers),
        };
        let a = pick_f(r);
        let b = if chance(r, 8) { a } else { pick_f(r) };
        (m, call(&[OWNER, a, b], &[]))
    }, |m, c| {
        link_up(m, c.ints[0] as u32, c.ints[1] as u32, c.ints[2] as u32);
        0
    }) else { return };
    assert!(run.changed > 100, "too few link changes: {}", run.changed);
}

/// Orig 0x11e598 (next object answering a query), 0x11e638 (request to all answering), 0x11e760 (speed to all answering),
/// 0x11e7f8 (update all), 0x192c80 (the actor's slot request; the owner-level request 0x11e850 and the slot play 0x133058 inside).
#[test]
fn owner_list_use_matches_the_game() {
    oracle("0x11e598 next answering", 0x11e598, CASES, Ret::I32, &[], |r| {
        let (m, _) = scene(r);
        let after = some_object(&m, r);
        let (a, b, c) = query_args(r);
        (m, call(&[OWNER, after, a, b, c], &[]))
    }, |m, c| next_answering(m, c.ints[0] as u32, c.ints[1] as u32, c.ints[2] as u32, c.ints[3] as u32, c.ints[4] as u32) as u64);
    oracle("0x11e638 request all", 0x11e638, CASES, Ret::None, &[], |r| {
        let (m, _) = scene(r);
        let (a, b, c) = query_args(r);
        (m, call(&[OWNER, a, b, c], &[]))
    }, |m, c| {
        request_all(m, &mut TestHost, c.ints[0] as u32, c.ints[1] as u32, c.ints[2] as u32, c.ints[3] as u32);
        0
    });
    oracle("0x11e760 speed all", 0x11e760, CASES, Ret::None, &[], |r| {
        let (m, _) = scene(r);
        let (a, b, c) = query_args(r);
        let speed = choose(r, &[1.0f32, 2.0, 0.5, 3.25]);
        (m, call(&[OWNER, a, b, c], &[speed]))
    }, |m, c| {
        speed_all(m, c.floats[0], c.ints[0] as u32, c.ints[1] as u32, c.ints[2] as u32, c.ints[3] as u32);
        0
    });
    let Some(run) = oracle("0x11e7f8 update all", 0x11e7f8, 3 * CASES, Ret::None, &[], |r| {
        let (m, _) = scene(r);
        let f = float(r).abs() * 3.0;
        let dt = choose(r, &[0.0, 0.5, 1.0, 1.0, 2.0, 3.5, f]);
        (m, call(&[OWNER], &[dt]))
    }, |m, c| {
        update_all(m, &mut TestHost, c.ints[0] as u32, c.floats[0]);
        0
    }) else { return };
    assert!(run.changed > CASES, "too few updates changed the image");
    oracle("0x192c80 actor slot request", 0x192c80, CASES, Ret::None, &[], |r| {
        let (m, _) = scene(r);
        (m, call(&[ACTOR, r.below(6) as u32], &[]))
    }, |m, c| {
        actor_request_slot(m, &mut TestHost, c.ints[0] as u32, c.ints[1] as u32);
        0
    });
}

// ---- range table dispatch -------------------------------------------------------------------------------------------

/// A range table at DATA: entry count, then entries `lo, hi, kind, extra count, extra halfwords`.
fn table_scene(r: &mut Rng) -> (Mem, u32, f32) {
    let mut m = fresh();
    let n = r.below(6) as u32;
    m.w8(DATA, n as u8);
    let mut at = DATA + 4;
    let mut edges = vec![0.0f32, 1.0];
    for _ in 0..n {
        let lo = r.below(200) as u16;
        let hi = if chance(r, 3) { 0xffff } else { lo + r.below(60) as u16 };
        let extra = r.below(4) as u8;
        m.w16(at, lo);
        m.w16(at + 2, hi);
        m.w8(at + 4, r.below(0x36) as u8);
        m.w8(at + 5, extra);
        for i in 0..extra as u32 {
            m.w16(at + 6 + 2 * i, r.word() as u16);
        }
        edges.push(lo as f32);
        edges.push(if hi == 0xffff { 300.0 } else { hi as f32 });
        at += 6 + 2 * extra as u32;
    }
    let v = if chance(r, 2) { choose(r, &edges) } else { r.below(3000) as f32 / 10.0 };
    (m, DATA, v)
}

/// Orig 0x188bd8 (call a callback for every table entry whose range holds the value) and 0x188010 (the same with the
/// event handler 0x188030, hooked to log its arguments).
#[test]
fn range_dispatch_matches_the_game() {
    let Some(run) = oracle("0x188bd8 range dispatch", 0x188bd8, 2 * CASES, Ret::None, &[], |r| {
        let (m, t, v) = table_scene(r);
        (m, call(&[t, CB_STUB, r.word() as u32], &[v]))
    }, |m, c| {
        range_dispatch(m, c.floats[0], c.ints[0] as u32, c.ints[2] as u32, log_call);
        0
    }) else { return };
    assert!(run.changed > CASES / 2, "too few dispatches called back");
    oracle("0x188010 range dispatch events", 0x188010, 2 * CASES, Ret::None, &[], |r| {
        let (m, t, v) = table_scene(r);
        (m, call(&[t, r.word() as u32], &[v]))
    }, |m, c| {
        range_dispatch_events(m, c.floats[0], c.ints[0] as u32, c.ints[1] as u32, log_call);
        0
    });
}
