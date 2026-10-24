//! Oracle tests of the map props (WP-L1-P25c, `kh2_game_core::props` and `kh2_model::draw::props`): kh2-ee runs the original from
//! the user's own executable and the port runs on a byte-for-byte copy of the same memory image; afterwards every region of
//! the image and the log of the calls the port hands to its host are compared with the original's, bit for bit.
//!
//! Two shapes of test:
//! * the factory fuzz of the two pure functions, `0x15ff50` (colour scale) and `0x161130` (motion clock step, with the
//!   pose call `0x1611b8` stubbed as the spec says);
//! * hand harnesses for the rest: a prop block, its owner object (the model data), the motion objects, the model table
//!   and the draw tree are built in memory by this file with random contents. The callees that belong to other units are
//!   hooks that log their arguments and answer from a pure function of the case's salt and the arguments; the port's
//!   [`PropHost`] logs the same and answers from the same functions. Hooked: the owner's virtual methods (`+0xc`, `+0x10`,
//!   `+0x24`, `+0x28`, `+0x38`), the motion's (`+0x8`, `+0x14`, `+0x18`), the prop's own two (offsets 0 and 4), the model
//!   allocator's free, `0x330bd0`, `0x11e018`, `0x11e8c0`, `0x11e638`, `0x11e760`, `0x11e7f8`, `0x11b568`, `0x143438`,
//!   `0x1684c0`, `0x11c2a0`, `0x11c268`, `0x16aa08`, `0x16b960`, `0x102498` and `0x102778` (the task create, which hands out
//!   a zeroed block the test then compares with the port's task fields).
//!
//! Nothing else is stubbed: the matrix and box helpers (`0x11b460`, `0x1443e0`, `0x144af8`, `0x144a50`, `0x144820`,
//! `0x1446d8`, `0x1446f8`), the draw tree functions (`0x143240`, `0x143260`, `0x1432b0`, `0x1432d8` and what they call) and
//! the vector unit code of the prop run as the game has them.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::*;
use kh2_game_core::props::{self as pr, InitArgs, PropHost};
use kh2_game_core::task::Tasks;
use kh2_map::draw_tree::Ram;
use kh2_model::draw::props as pure;
use std::cell::RefCell;
use std::rc::Rc;

// ---------------------------------------------------------------------------------------------------------------------
// The factory tests
// ---------------------------------------------------------------------------------------------------------------------

macro_rules! oracle {
    ($name:ident, $spec:expr, |$c:ident| $body:expr) => {
        #[test]
        fn $name() {
            let spec: Spec = $spec;
            let Some(st) = fuzz(&spec, 4000, |$c| $body) else { return };
            eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
        }
    };
}

const fn spec(addr: u32, args: &'static [Arg], floats: u8, ret: Ret, globals: &'static [(u32, u32, bool)], stubs: &'static [u32]) -> Spec {
    Spec { addr, args, floats, ret, globals, stubs, nulls: true }
}

oracle!(
    orig_15ff50_matches_the_game,
    spec(
        0x15ff50,
        &[
            Arg::Ptr(Block { size: 0x2b0, fields: &[(0x1a4, 1, Kind::Int), (0x1a5, 1, Kind::Int), (0x1a6, 1, Kind::Int), (0x2a8, 1, Kind::Int), (0x2a9, 1, Kind::Int), (0x2aa, 1, Kind::Int)], subs: &[] }),
            Arg::Int(&[0x80, 0xff, 0x100]),
            Arg::Int(&[0x80, 0xff, 0x100]),
            Arg::Int(&[0x80, 0xff, 0x100])
        ],
        0,
        Ret::None,
        &[],
        &[]
    ),
    |c| {
        let (r, g, b) = (c.int(1) as u32, c.int(2) as u32, c.int(3) as u32);
        pure::colour_scale(c.block(0), r, g, b);
        Out::none()
    }
);

// The holder block: +0x48 the motion (a pointer the original only tests against 0), +0x4c the time, +0x50 the length. The
// time and the length are compared as floats here (the harness's tolerance); the hand test below compares the words exactly.
oracle!(
    orig_161130_matches_the_game,
    spec(
        0x161130,
        &[Arg::Ptr(Block { size: 0x58, fields: &[(0x48, 4, Kind::Int), (0x4c, 4, Kind::Float), (0x50, 4, Kind::Float)], subs: &[] })],
        1,
        Ret::None,
        &[],
        &[0x1611b8]
    ),
    |c| {
        let dt = c.float(0);
        pure::motion_step(c.block(0), dt, |_| {});
        Out::none()
    }
);

// ---------------------------------------------------------------------------------------------------------------------
// The machine
// ---------------------------------------------------------------------------------------------------------------------

fn urange(r: &mut Rng, lo: f32, hi: f32) -> f32 {
    lo + (r.word() >> 40) as f32 / (1u64 << 24) as f32 * (hi - lo)
}

fn pick<T: Copy>(r: &mut Rng, s: &[T]) -> T {
    s[r.below(s.len() as u64) as usize]
}

fn w32(b: &mut [u8], o: u32, v: u32) {
    b[o as usize..o as usize + 4].copy_from_slice(&v.to_le_bytes());
}

fn w16(b: &mut [u8], o: u32, v: u16) {
    b[o as usize..o as usize + 2].copy_from_slice(&v.to_le_bytes());
}

fn wf(b: &mut [u8], o: u32, v: &[f32]) {
    for (k, f) in v.iter().enumerate() {
        w32(b, o + 4 * k as u32, f.to_bits());
    }
}

/// The tests' machine: the original in kh2-ee, the port's memory next to it, and the regions both are compared over.
struct W {
    ee: Ee,
    ram: Ram,
    regions: Vec<(u32, u32)>,
    globals: Vec<(u32, u8)>,
}

impl W {
    fn new(what: &str) -> Option<W> {
        let elf = kh2_factory::exe_or_skip(what)?;
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        Some(W { ee, ram: Ram::new(), regions: Vec::new(), globals: Vec::new() })
    }

    fn reset(&mut self) {
        self.ram = Ram::new();
        self.regions.clear();
        self.globals.clear();
    }

    fn put(&mut self, a: u32, b: &[u8]) {
        self.ee.write(a, b);
        self.ram.write(a, b);
        self.regions.push((a, b.len() as u32));
    }

    fn global(&mut self, a: u32, v: u32) {
        self.ee.w32(a, v);
        self.ram.w32(a, v);
        self.globals.push((a, 4));
    }

    fn global8(&mut self, a: u32, v: u8) {
        self.ee.write(a, &[v]);
        self.ram.w8(a, v);
        self.globals.push((a, 1));
    }

    /// Copies bytes the executable's image holds (a constant table) into the port's memory.
    fn seed(&mut self, a: u32, len: u32) {
        for i in 0..len {
            let b = self.ee.r8(a + i);
            self.ram.w8(a + i, b);
        }
    }

    fn call(&mut self, addr: u32, args: &[u64], fargs: &[f32]) -> u64 {
        self.ee.call(addr, args, fargs, STACK).unwrap_or_else(|f| panic!("the original faulted at {addr:#x}: {f:?}"))
    }

    fn game_bytes(&mut self, a: u32, n: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(n as usize);
        let mut at = a;
        while (out.len() as u32) < n {
            out.extend_from_slice(&self.ee.read::<16>(at));
            at += 16;
        }
        out.truncate(n as usize);
        out
    }

    fn check(&mut self, what: &str) {
        for (a, len) in self.regions.clone() {
            let g = self.game_bytes(a, len);
            let p = self.ram.read(a, len);
            if g != p {
                let i = (0..len as usize).find(|&i| g[i] != p[i]).unwrap();
                panic!("{what}: region {a:#x}+{:#x} differs: game {:02x?} port {:02x?}", i, &g[i..(i + 8).min(len as usize)], &p[i..(i + 8).min(len as usize)]);
            }
        }
        for (a, width) in self.globals.clone() {
            if width == 1 {
                assert_eq!(self.ee.r8(a), self.ram.r8(a), "{what}: global byte {a:#x}");
            } else {
                assert_eq!(self.ee.r32(a), self.ram.r32(a), "{what}: global {a:#x}");
            }
        }
    }
}

type Log = Rc<RefCell<Vec<Vec<u32>>>>;

// Memory map (all in the machine's RAM, above the executable).
const PROP: u32 = 0x0181_0000;
const PROP2: u32 = 0x0181_1000;
const PROP_LEN: u32 = 0x300;
const OWNER: u32 = 0x0182_0000;
const OWN_VT: u32 = 0x0182_0100;
const OWN_DATA: u32 = 0x0182_0200;
const MOTION: u32 = 0x0183_0000;
const MOT_VT: u32 = 0x0183_0100;
const LOOKUP: u32 = 0x0183_0200;
const PROP_VT: u32 = 0x0184_0000;
const TREE: u32 = 0x0185_0000;
const TREE_LEN: u32 = 0x5400;
const NODES: u32 = 0x0186_0000;
const MODEL_TABLE: u32 = 0x0187_0000;
const MODEL_TABLE_LEN: u32 = 0x200;
const ALLOC: u32 = 0x0188_0000;
const ALLOC_VT: u32 = 0x0188_0100;
const TASKBLK: u32 = 0x0189_0000;
const MAT: u32 = 0x018a_0000;
const ROT_ALT: u32 = 0x018b_0000;
const VEC_A: u32 = 0x018d_0000;
const VEC_B: u32 = 0x018d_0010;
const VEC_C: u32 = 0x018d_0020;
const PLAYER_OBJ: u32 = 0x018f_0000;
const PLAYER_POS: u32 = 0x018f_0100;
const MGR_GAME: u32 = 0x0123_4560;
const HOOKS: u32 = 0x01c0_0000;

const fn hk(k: u32) -> u32 {
    HOOKS + 0x10 * k
}
const H_PROP0: u32 = hk(1);
const H_PROP1: u32 = hk(2);
const H_RADIUS: u32 = hk(3);
const H_BOUNDS: u32 = hk(4);
const H_INSTANCE: u32 = hk(5);
const H_RELEASE: u32 = hk(6);
const H_FLAGS: u32 = hk(7);
const H_MLEN: u32 = hk(8);
const H_MBOX: u32 = hk(9);
const H_MMAT: u32 = hk(10);
const H_FREE: u32 = hk(11);

// Game globals.
const G_TREE: u32 = 0x355930;
const G_STEP: u32 = 0x356f40;

fn drv(salt: u64, id: u32, a: &[u32]) -> Rng {
    let mut k = salt ^ ((id as u64) << 40) ^ 0x5bd1_e995;
    for x in a {
        k = k.wrapping_mul(0x0000_0100_0000_01b3) ^ *x as u64;
    }
    Rng::new(k)
}

// What the hooked callees answer: one pure function per callee, used by the hook and by the port's host.
fn d_radius(s: u64, owner: u32) -> f32 {
    urange(&mut drv(s, 1, &[owner]), 0.0, 25.0)
}

fn d_bounds(s: u64, id: u32, who: u32) -> (V4, V4) {
    let mut r = drv(s, id, &[who]);
    let lo = [urange(&mut r, -90.0, 10.0), urange(&mut r, -90.0, 10.0), urange(&mut r, -90.0, 10.0), urange(&mut r, 0.5, 1.5)];
    let hi = [urange(&mut r, -10.0, 90.0), urange(&mut r, -10.0, 90.0), urange(&mut r, -10.0, 90.0), urange(&mut r, 0.5, 1.5)];
    (lo, hi)
}

fn d_instance(s: u64, owner: u32, arg: u32) -> u32 {
    0x0190_0000 + (drv(s, 3, &[owner, arg]).word() as u32 & 0xff0)
}

fn d_flags(s: u64, owner: u32) -> u64 {
    pick(&mut drv(s, 4, &[owner]), &[0u64, 1, 1, 2, 3, 0x100, 0x1_0000_0000, 0x1_0000_0001])
}

fn d_mlen(s: u64, motion: u32) -> i32 {
    drv(s, 5, &[motion]).below(300) as i32 - 4
}

fn d_mmat(s: u64, motion: u32, time: f32) -> Option<M4> {
    let mut r = drv(s, 6, &[motion, time.to_bits()]);
    if r.below(5) == 0 {
        return None;
    }
    Some(rand_m4(&mut r))
}

fn d_rotate(s: u64, a0: u32, a1: u32) -> (M4, bool) {
    let mut r = drv(s, 7, &[a0, a1]);
    (rand_m4(&mut r), r.below(4) == 0)
}

fn d_floor(s: u64, point: u32, rec_first: u32) -> [u8; 3] {
    let w = drv(s, 8, &[point, rec_first]).word();
    [w as u8, (w >> 8) as u8, (w >> 16) as u8]
}

fn d_lookup(s: u64, table: u32, kind: u32, index: u32) -> u32 {
    if drv(s, 9, &[table, kind, index]).below(4) == 0 { 0 } else { LOOKUP }
}

fn d_player(s: u64) -> Option<V4> {
    let mut r = drv(s, 10, &[]);
    if r.below(4) == 0 {
        return None;
    }
    Some([urange(&mut r, -50.0, 50.0), urange(&mut r, -120.0, 120.0), urange(&mut r, -50.0, 50.0), 1.0])
}

type V4 = [f32; 4];
type M4 = [V4; 4];

fn rand_m4(r: &mut Rng) -> M4 {
    let mut m = [[0.0f32; 4]; 4];
    for (i, row) in m.iter_mut().enumerate() {
        for (k, f) in row.iter_mut().enumerate() {
            *f = if i == 3 { if k == 3 { 1.0 } else { urange(r, -120.0, 120.0) } } else { urange(r, -2.0, 2.0) };
        }
    }
    m
}

fn m4_bytes(m: &M4) -> Vec<u8> {
    m.iter().flatten().flat_map(|f| f.to_bits().to_le_bytes()).collect()
}

fn v4_bytes(v: &V4) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_bits().to_le_bytes()).collect()
}

/// The port's host: logs what the hooks log and answers from the same functions.
struct TestHost {
    log: Log,
    salt: u64,
}

impl TestHost {
    fn push(&self, v: Vec<u32>) {
        self.log.borrow_mut().push(v);
    }
}

impl PropHost for TestHost {
    fn prop_call(&mut self, _ram: &mut Ram, prop: u32, slot_offset: u32) {
        self.push(vec![if slot_offset == 0 { H_PROP0 } else { H_PROP1 }, prop]);
    }
    fn construct_draw_object(&mut self, _ram: &mut Ram, at: u32) {
        self.push(vec![0x330bd0, at]);
    }
    fn owner_instance(&mut self, _ram: &mut Ram, owner: u32, arg: u32) -> u32 {
        self.push(vec![H_INSTANCE, owner, arg]);
        d_instance(self.salt, owner, arg)
    }
    fn owner_release(&mut self, _ram: &mut Ram, owner: u32, arg: u32, instance: u32) {
        self.push(vec![H_RELEASE, owner, arg, instance]);
    }
    fn owner_radius(&mut self, _ram: &mut Ram, owner: u32) -> f32 {
        self.push(vec![H_RADIUS, owner]);
        d_radius(self.salt, owner)
    }
    fn owner_bounds(&mut self, _ram: &mut Ram, owner: u32) -> (V4, V4) {
        self.push(vec![H_BOUNDS, owner]);
        d_bounds(self.salt, 11, owner)
    }
    fn owner_flags(&mut self, _ram: &mut Ram, owner: u32) -> u64 {
        self.push(vec![H_FLAGS, owner]);
        d_flags(self.salt, owner)
    }
    fn bind_instance(&mut self, _ram: &mut Ram, instance: u32, value: u32) {
        self.push(vec![0x11e018, instance, value]);
    }
    fn call_11e8c0(&mut self, _ram: &mut Ram, instance: u32, a1: u32) {
        self.push(vec![0x11e8c0, instance, a1]);
    }
    fn call_11e638(&mut self, _ram: &mut Ram, instance: u32, a1: u32, a2: u32, a3: u32) {
        self.push(vec![0x11e638, instance, a1, a2, a3]);
    }
    fn call_11e760(&mut self, _ram: &mut Ram, instance: u32, a1: u64) {
        self.push(vec![0x11e760, instance, a1 as u32, (a1 >> 32) as u32, 0, 0]);
    }
    fn texture_update(&mut self, _ram: &mut Ram, step: f32, instance: u32) {
        self.push(vec![0x11e7f8, instance, step.to_bits()]);
    }
    fn rotate_xyz(&mut self, ram: &mut Ram, matrix: u32, rotation: u32) -> u32 {
        self.push(vec![0x11b568, matrix, rotation]);
        let (m, elsewhere) = d_rotate(self.salt, matrix, rotation);
        let at = if elsewhere { ROT_ALT } else { matrix };
        ram.write(at, &m4_bytes(&m));
        at
    }
    fn floor_facts(&mut self, ram: &mut Ram, point: u32, record: &mut [u8; 0x40]) {
        let word = |o: usize| u32::from_le_bytes(record[o..o + 4].try_into().unwrap());
        let mut l = vec![0x143438, point];
        for o in [0, 4, 8, 0xc, 0x10, 0x14, 0x18, 0x24, 0x28, 0x2c, 0x30] {
            l.push(word(o));
        }
        l.push(record[0x34] as u32);
        self.push(l);
        let c = d_floor(self.salt, point, ram.r32(point));
        record[0x20..0x23].copy_from_slice(&c);
    }
    fn motion_lookup(&mut self, _ram: &mut Ram, table: u32, kind: u32, index: i64) -> u32 {
        self.push(vec![0x1684c0, table, kind, index as u32, (index >> 32) as u32]);
        d_lookup(self.salt, table, kind, index as u32)
    }
    fn motion_length(&mut self, _ram: &mut Ram, motion: u32) -> i32 {
        self.push(vec![H_MLEN, motion]);
        d_mlen(self.salt, motion)
    }
    fn motion_bounds(&mut self, _ram: &mut Ram, motion: u32) -> (V4, V4) {
        self.push(vec![H_MBOX, motion]);
        d_bounds(self.salt, 12, motion)
    }
    fn motion_matrix(&mut self, _ram: &mut Ram, motion: u32, time: f32) -> Option<M4> {
        self.push(vec![H_MMAT, motion, time.to_bits()]);
        d_mmat(self.salt, motion, time)
    }
    fn pose_motion(&mut self, _ram: &mut Ram, instance: u32, matrix: u32, motion: u32, time: f32) {
        self.push(vec![0x11c2a0, instance, matrix, motion, 0, 0, 0, time.to_bits()]);
    }
    fn pose_base(&mut self, _ram: &mut Ram, instance: u32, matrix: u32) {
        self.push(vec![0x11c268, instance, matrix, 0]);
    }
    fn player_position(&mut self, _ram: &mut Ram) -> Option<V4> {
        self.push(vec![0x16aa08]);
        let p = d_player(self.salt)?;
        self.push(vec![0x16b960, PLAYER_OBJ]);
        Some(p)
    }
    fn dead_prop(&mut self, _ram: &mut Ram, task: u32) {
        self.push(vec![0x102498, task]);
    }
    fn free_prop(&mut self, _ram: &mut Ram, allocator: u32, prop: u32) {
        self.push(vec![H_FREE, allocator, prop]);
    }
}

fn hook(ee: &mut Ee, addr: u32, f: impl FnMut(&mut Ee) + 'static) {
    ee.hooks.insert(addr, f);
}

fn a(e: &Ee, k: usize) -> u32 {
    e.gpr[reg::A0 + k] as u32
}

/// Installs the logging hooks on the game for one case.
fn install_hooks(w: &mut W, log: &Log, s: u64) {
    let ee = &mut w.ee;
    // The prop's own methods: log the prop.
    for h in [H_PROP0, H_PROP1] {
        let l = log.clone();
        hook(ee, h, move |e: &mut Ee| l.borrow_mut().push(vec![h, a(e, 0)]));
    }
    {
        let l = log.clone();
        hook(ee, H_RADIUS, move |e: &mut Ee| {
            l.borrow_mut().push(vec![H_RADIUS, a(e, 0)]);
            e.fpr[0] = d_radius(s, a(e, 0));
        });
    }
    {
        let l = log.clone();
        hook(ee, H_BOUNDS, move |e: &mut Ee| {
            l.borrow_mut().push(vec![H_BOUNDS, a(e, 0)]);
            let (lo, hi) = d_bounds(s, 11, a(e, 0));
            e.write(a(e, 1), &v4_bytes(&lo));
            e.write(a(e, 2), &v4_bytes(&hi));
        });
    }
    {
        let l = log.clone();
        hook(ee, H_INSTANCE, move |e: &mut Ee| {
            l.borrow_mut().push(vec![H_INSTANCE, a(e, 0), a(e, 1)]);
            e.gpr[reg::V0] = d_instance(s, a(e, 0), a(e, 1)) as u128;
        });
    }
    {
        let l = log.clone();
        hook(ee, H_RELEASE, move |e: &mut Ee| l.borrow_mut().push(vec![H_RELEASE, a(e, 0), a(e, 1), a(e, 2)]));
    }
    {
        let l = log.clone();
        hook(ee, H_FLAGS, move |e: &mut Ee| {
            l.borrow_mut().push(vec![H_FLAGS, a(e, 0)]);
            e.gpr[reg::V0] = d_flags(s, a(e, 0)) as u128;
        });
    }
    {
        let l = log.clone();
        hook(ee, H_MLEN, move |e: &mut Ee| {
            l.borrow_mut().push(vec![H_MLEN, a(e, 0)]);
            e.gpr[reg::V0] = d_mlen(s, a(e, 0)) as i64 as u64 as u128;
        });
    }
    {
        let l = log.clone();
        hook(ee, H_MBOX, move |e: &mut Ee| {
            l.borrow_mut().push(vec![H_MBOX, a(e, 0)]);
            let (lo, hi) = d_bounds(s, 12, a(e, 0));
            e.write(a(e, 1), &v4_bytes(&lo));
            e.write(a(e, 2), &v4_bytes(&hi));
        });
    }
    {
        let l = log.clone();
        hook(ee, H_MMAT, move |e: &mut Ee| {
            let t = e.fpr[12];
            l.borrow_mut().push(vec![H_MMAT, a(e, 0), t.to_bits()]);
            match d_mmat(s, a(e, 0), t) {
                Some(m) => {
                    e.write(a(e, 1), &m4_bytes(&m));
                    e.gpr[reg::V0] = 0;
                }
                None => e.gpr[reg::V0] = 1,
            }
        });
    }
    {
        let l = log.clone();
        hook(ee, H_FREE, move |e: &mut Ee| l.borrow_mut().push(vec![H_FREE, a(e, 0), a(e, 1)]));
    }
    for (addr, n) in [(0x330bd0u32, 1usize), (0x11e018, 2), (0x11e8c0, 2), (0x11e638, 4), (0x102498, 1)] {
        let l = log.clone();
        hook(ee, addr, move |e: &mut Ee| l.borrow_mut().push(std::iter::once(addr).chain((0..n).map(|k| a(e, k))).collect()));
    }
    {
        let l = log.clone();
        hook(ee, 0x11e760, move |e: &mut Ee| {
            let a1 = e.gpr[reg::A0 + 1] as u64;
            l.borrow_mut().push(vec![0x11e760, a(e, 0), a1 as u32, (a1 >> 32) as u32, a(e, 2), a(e, 3)]);
        });
    }
    {
        let l = log.clone();
        hook(ee, 0x11e7f8, move |e: &mut Ee| l.borrow_mut().push(vec![0x11e7f8, a(e, 0), e.fpr[12].to_bits()]));
    }
    {
        let l = log.clone();
        hook(ee, 0x11b568, move |e: &mut Ee| {
            let (a0, a1) = (a(e, 0), a(e, 1));
            l.borrow_mut().push(vec![0x11b568, a0, a1]);
            let (m, elsewhere) = d_rotate(s, a0, a1);
            let at = if elsewhere { ROT_ALT } else { a0 };
            e.write(at, &m4_bytes(&m));
            e.gpr[reg::V0] = at as u128;
        });
    }
    {
        let l = log.clone();
        hook(ee, 0x143438, move |e: &mut Ee| {
            let (point, rec) = (a(e, 0), a(e, 1));
            let mut v = vec![0x143438, point];
            for o in [0u32, 4, 8, 0xc, 0x10, 0x14, 0x18, 0x24, 0x28, 0x2c, 0x30] {
                v.push(e.r32(rec + o));
            }
            v.push(e.r8(rec + 0x34) as u32);
            l.borrow_mut().push(v);
            let c = d_floor(s, point, e.r32(point));
            e.write(rec + 0x20, &c);
        });
    }
    {
        let l = log.clone();
        hook(ee, 0x1684c0, move |e: &mut Ee| {
            let (t, k, i) = (a(e, 0), a(e, 1), e.gpr[reg::A0 + 2] as u64);
            l.borrow_mut().push(vec![0x1684c0, t, k, i as u32, (i >> 32) as u32]);
            e.gpr[reg::V0] = d_lookup(s, t, k, i as u32) as u128;
        });
    }
    {
        let l = log.clone();
        hook(ee, 0x11c2a0, move |e: &mut Ee| {
            let t = e.fpr[12];
            let t0 = e.gpr[reg::T0] as u32;
            let t1 = e.gpr[reg::T0 + 1] as u32;
            l.borrow_mut().push(vec![0x11c2a0, a(e, 0), a(e, 1), a(e, 2), a(e, 3), t0, t1, t.to_bits()]);
        });
    }
    {
        let l = log.clone();
        hook(ee, 0x11c268, move |e: &mut Ee| l.borrow_mut().push(vec![0x11c268, a(e, 0), a(e, 1), a(e, 2)]));
    }
    {
        let l = log.clone();
        hook(ee, 0x16aa08, move |e: &mut Ee| {
            l.borrow_mut().push(vec![0x16aa08]);
            e.gpr[reg::V0] = if d_player(s).is_some() { PLAYER_OBJ as u128 } else { 0 };
        });
    }
    {
        let l = log.clone();
        hook(ee, 0x16b960, move |e: &mut Ee| {
            l.borrow_mut().push(vec![0x16b960, a(e, 0)]);
            let p = d_player(s).expect("the position is asked for only with a player");
            e.write(PLAYER_POS, &v4_bytes(&p));
            e.gpr[reg::V0] = PLAYER_POS as u128;
        });
    }
    {
        let l = log.clone();
        hook(ee, 0x102778, move |e: &mut Ee| {
            l.borrow_mut().push(vec![0x102778, a(e, 0), a(e, 1), a(e, 2), a(e, 3)]);
            e.gpr[reg::V0] = TASKBLK as u128;
        });
    }
}

/// One case's world: the machines, the logs, the salt, the port's tasks.
struct Cx {
    w: W,
    game_log: Log,
    port_log: Log,
    salt: u64,
    tasks: Tasks,
    manager: u32,
}

impl Cx {
    fn new(what: &str) -> Option<Cx> {
        Some(Cx { w: W::new(what)?, game_log: Rc::new(RefCell::new(Vec::new())), port_log: Rc::new(RefCell::new(Vec::new())), salt: 0, tasks: Tasks::new(), manager: 0 })
    }

    /// A new case: empty port memory, new salt, hooks, the static objects every test shares.
    fn begin(&mut self, r: &mut Rng) {
        self.w.reset();
        self.game_log.borrow_mut().clear();
        self.port_log.borrow_mut().clear();
        self.salt = r.word();
        self.tasks = Tasks::new();
        self.manager = self.tasks.create_manager(0);
        let (log, s) = (self.game_log.clone(), self.salt);
        install_hooks(&mut self.w, &log, s);
        self.w.seed(pr::UNIT_TABLE, 0x40);
        self.w.seed(pr::ROW_SCALE_WORD, 4);
        let mut ov = vec![0u8; 0x300];
        w32(&mut ov, 0, OWN_VT);
        w32(&mut ov, 4, OWN_DATA);
        for b in ov.iter_mut().skip(8) {
            *b = r.word() as u8;
        }
        self.w.put(OWNER, &ov);
        let mut vt = vec![0u8; 0x40];
        for (o, h) in [(0xc, H_RADIUS), (0x10, H_BOUNDS), (0x24, H_INSTANCE), (0x28, H_RELEASE), (0x38, H_FLAGS)] {
            w32(&mut vt, o, h);
        }
        self.w.put(OWN_VT, &vt);
        let mut od = vec![0u8; 0x10];
        w32(&mut od, 0, if r.below(2) == 0 { 0xffff_ffff } else { r.word() as u32 & 0xffff });
        self.w.put(OWN_DATA, &od);
        let mut mo = vec![0u8; 0x10];
        w32(&mut mo, 0, MOT_VT);
        self.w.put(MOTION, &mo);
        let mut mvt = vec![0u8; 0x40];
        for (o, h) in [(0x8, H_MLEN), (0x14, H_MBOX), (0x18, H_MMAT)] {
            w32(&mut mvt, o, h);
        }
        self.w.put(MOT_VT, &mvt);
        let mut lk = vec![0u8; 0x10];
        w32(&mut lk, 8, if r.below(5) == 0 { 0 } else { MOTION });
        self.w.put(LOOKUP, &lk);
        let mut pv = vec![0u8; 0x10];
        w32(&mut pv, 0, H_PROP0);
        w32(&mut pv, 4, H_PROP1);
        self.w.put(PROP_VT, &pv);
        let mut al = vec![0u8; 0x10];
        w32(&mut al, 0, ALLOC_VT);
        self.w.put(ALLOC, &al);
        let mut avt = vec![0u8; 0x10];
        w32(&mut avt, 4, H_FREE);
        self.w.put(ALLOC_VT, &avt);
        self.w.put(TASKBLK, &[0u8; 0x48]);
        self.w.put(ROT_ALT, &[0u8; 0x40]);
        let mut mt = vec![0u8; MODEL_TABLE_LEN as usize];
        for m in 0..3u32 {
            w32(&mut mt, 0x78 + 12 * m, OWNER);
            w32(&mut mt, 0x7c + 12 * m, r.word() as u32 & 0xffff);
            w32(&mut mt, 0x80 + 12 * m, if r.below(4) == 0 { 0 } else { 0x0191_0000 + (r.word() as u32 & 0xff0) });
        }
        self.w.put(MODEL_TABLE, &mt);
        self.w.global(pr::MODEL_TABLE_WORD, MODEL_TABLE);
        self.w.global(pr::OWNER_ARG_WORD, ALLOC);
        self.w.global(0x520a30, MGR_GAME);
    }

    fn host(&self) -> TestHost {
        TestHost { log: self.port_log.clone(), salt: self.salt }
    }

    /// Both tails of the case: the task block the game was handed against the port's task fields, memory, then the logs.
    fn end(&mut self, what: &str) {
        self.w.check(what);
        assert_eq!(*self.game_log.borrow(), *self.port_log.borrow(), "{what}: the calls to the host differ");
    }

    fn game(&mut self, addr: u32, args: &[u64], fargs: &[f32]) -> u64 {
        self.w.call(addr, args, fargs)
    }
}

fn cases(name: &str, n: u32, mut body: impl FnMut(&mut Cx, &mut Rng, u32)) {
    let Some(mut cx) = Cx::new(name) else { return };
    let mut r = Rng::new(name.bytes().fold(0x25c0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64)));
    for i in 0..n {
        cx.begin(&mut r);
        body(&mut cx, &mut r, i);
        cx.end(&format!("{name} case {i}"));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------------------------------

/// A prop block with random bytes everywhere and sane floats where the code reads floats.
fn prop_bytes(r: &mut Rng) -> Vec<u8> {
    let mut b: Vec<u8> = (0..PROP_LEN).map(|_| r.word() as u8).collect();
    w32(&mut b, 0, PROP_VT);
    w32(&mut b, 4, OWNER);
    let m = rand_m4(r);
    wf(&mut b, 0x10, &m.iter().flatten().copied().collect::<Vec<_>>());
    wf(&mut b, 0x50, &[urange(r, -200.0, 200.0), urange(r, -200.0, 200.0), urange(r, -200.0, 200.0), 1.0]);
    wf(&mut b, 0x60, &[urange(r, -3.0, 3.0), urange(r, -3.0, 3.0), urange(r, -3.0, 3.0), 0.0]);
    wf(&mut b, 0x70, &[urange(r, -2.0, 2.0), urange(r, -2.0, 2.0), urange(r, -2.0, 2.0), 1.0]);
    for i in 0..8u32 {
        wf(&mut b, 0x90 + 16 * i, &[urange(r, -100.0, 100.0), urange(r, -100.0, 100.0), urange(r, -100.0, 100.0), 1.0]);
    }
    let h = rand_m4(r);
    wf(&mut b, 0x110, &h.iter().flatten().copied().collect::<Vec<_>>());
    w32(&mut b, 0x110 + 0x40, if r.below(4) == 0 { 0 } else { 0x0191_0000 });
    w32(&mut b, 0x110 + 0x44, PROP);
    w32(&mut b, 0x110 + 0x48, if r.below(3) == 0 { 0 } else { MOTION });
    wf(&mut b, 0x110 + 0x4c, &[urange(r, 0.0, 60.0), urange(r, 1.0, 60.0)]);
    wf(&mut b, 0x164, &[urange(r, -90.0, 0.0), urange(r, -90.0, 0.0), urange(r, -90.0, 0.0), urange(r, 0.0, 90.0), urange(r, 0.0, 90.0), urange(r, 0.0, 90.0)]);
    let pm = rand_m4(r);
    wf(&mut b, 0x200, &pm.iter().flatten().copied().collect::<Vec<_>>());
    wf(&mut b, 0x280, &[urange(r, -50.0, 50.0), urange(r, -50.0, 50.0), urange(r, -50.0, 50.0), 1.0]);
    wf(&mut b, 0x290, &[urange(r, -100.0, -1.0), urange(r, -100.0, -1.0), urange(r, -100.0, -1.0), urange(r, 1.0, 100.0), urange(r, 1.0, 100.0), urange(r, 1.0, 100.0)]);
    w32(&mut b, 0x180, r.word() as u32 & 3 | (r.word() as u32 & 0xf0));
    b
}

/// A small tree: `n` nodes, a node table behind tree `+4`, a slot count, the global word set.
fn put_tree(cx: &mut Cx, r: &mut Rng, n: u32) {
    let mut t: Vec<u8> = (0..TREE_LEN).map(|_| r.word() as u8).collect();
    w32(&mut t, 4, NODES);
    w32(&mut t, 0x5360, r.below(100) as u32);
    let mut nodes = vec![0u8; (n * 0x30) as usize];
    for i in 0..n {
        let at = i * 0x30;
        let mut kids = Vec::new();
        for j in (i + 1)..n {
            if kids.len() < 3 && r.below(2) == 0 {
                kids.push(j);
            }
        }
        for c in 0..8u32 {
            let v = kids.get(c as usize).map_or(-1i16, |&x| x as i16);
            w16(&mut nodes, at + 2 * c, v as u16);
            if v == -1 {
                break;
            }
        }
        let s = if i == 0 { 5000.0 } else { urange(r, 300.0, 2000.0) };
        let lo = [-s * urange(r, 0.4, 1.0), -s * urange(r, 0.4, 1.0), -s * urange(r, 0.4, 1.0)];
        let hi = [s * urange(r, 0.4, 1.0), s * urange(r, 0.4, 1.0), s * urange(r, 0.4, 1.0)];
        wf(&mut nodes, at + 0x10, &[lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]]);
        w32(&mut nodes, at + 0x2c, 0);
    }
    cx.w.put(TREE, &t);
    cx.w.put(NODES, &nodes);
    cx.w.global(G_TREE, TREE);
}

fn node_count(r: &mut Rng) -> u32 {
    1 + r.below(5) as u32
}

// ---------------------------------------------------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------------------------------------------------

#[test]
fn the_empty_functions_change_nothing() {
    cases("empty functions", 20, |cx, r, _| {
        let p = prop_bytes(r);
        cx.w.put(PROP, &p);
        for addr in [0x15f488, 0x15ffb8, 0x160ff8] {
            cx.game(addr, &[PROP as u64, 0x1234_5678], &[]);
        }
        pr::empty_15f488();
        pr::empty_15ffb8();
        pr::empty_160ff8();
    });
}

/// orig 0x161130 on a holder in memory (with the pose call), bit for bit.
#[test]
fn orig_161130_matches_the_game_on_the_holder_in_memory() {
    cases("0x161130 holder", 600, |cx, r, _| {
        let mut p = prop_bytes(r);
        let (t, len) = (urange(r, -5.0, 70.0), urange(r, 0.0, 70.0));
        wf(&mut p, 0x110 + 0x4c, &[t, len]);
        cx.w.put(PROP, &p);
        let (u1, u2) = (urange(r, 0.0, 40.0), urange(r, -3.0, 40.0));
        let dt = pick(r, &[0.0f32, 1.0, 0.5, 2.0, u1, u2]);
        cx.game(0x161130, &[(PROP + 0x110) as u64], &[dt]);
        let mut h = cx.host();
        pr::motion_step(&mut cx.w.ram, &mut h, PROP + 0x110, dt);
    });
}

#[test]
fn orig_160fb0_matches_the_game() {
    cases("0x160fb0", 200, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        let owner = r.word() as u32;
        cx.game(0x160fb0, &[(PROP + 0x110) as u64, owner as u64], &[]);
        pr::bind_holder(&mut cx.w.ram, PROP + 0x110, owner);
    });
}

#[test]
fn orig_1611a0_matches_the_game() {
    cases("0x1611a0", 100, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        cx.game(0x1611a0, &[(PROP + 0x110) as u64], &[]);
        pr::holder_unit(&mut cx.w.ram, PROP + 0x110);
    });
}

#[test]
fn orig_161000_and_15ffa0_match_the_game() {
    cases("0x161000 0x15ffa0", 100, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        cx.game(0x161000, &[(PROP + 0x110) as u64], &[]);
        pr::reset_time(&mut cx.w.ram, PROP + 0x110);
        cx.w.put(PROP2, &prop_bytes(r));
        cx.game(0x15ffa0, &[PROP2 as u64], &[]);
        pr::reset_prop_time(&mut cx.w.ram, PROP2);
    });
}

#[test]
fn orig_161008_matches_the_game() {
    cases("0x161008", 100, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        let t = r.word() as u32;
        cx.game(0x161008, &[(PROP + 0x110) as u64, t as u64], &[]);
        pr::set_motion_table(&mut cx.w.ram, PROP + 0x110, t);
    });
}

#[test]
fn orig_161010_matches_the_game() {
    let mut found = 0;
    cases("0x161010", 600, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        let index = pick(r, &[-1i64, 0, 1, 2, 7, 100, -100, i32::MAX as i64, i32::MIN as i64]);
        let v0 = cx.game(0x161010, &[(PROP + 0x110) as u64, index as u64], &[]);
        let mut h = cx.host();
        let mine = pr::motion_lookup(&mut cx.w.ram, &mut h, PROP + 0x110, index);
        assert_eq!(v0 as u32, mine, "the motion returned");
        found += (mine != 0) as u32;
    });
    assert!(found > 20 || kh2_factory::exe().is_none(), "{found} lookups found a motion");
}

#[test]
fn orig_161238_matches_the_game() {
    cases("0x161238", 300, |cx, r, _| {
        let mut p = prop_bytes(r);
        w32(&mut p, 0x110 + 0x48, MOTION);
        cx.w.put(PROP, &p);
        cx.game(0x161238, &[(PROP + 0x110) as u64], &[]);
        let mut h = cx.host();
        pr::motion_box(&mut cx.w.ram, &mut h, PROP + 0x110);
    });
}

#[test]
fn orig_1611b8_matches_the_game() {
    cases("0x1611b8", 500, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        let t = urange(r, -2.0, 80.0);
        cx.game(0x1611b8, &[(PROP + 0x110) as u64], &[t]);
        let mut h = cx.host();
        pr::motion_pose(&mut cx.w.ram, &mut h, PROP + 0x110, t);
    });
}

#[test]
fn orig_1610b0_matches_the_game() {
    let (mut started, mut none) = (0, 0);
    cases("0x1610b0", 600, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        let index = pick(r, &[-1i64, 0, 1, 2, 9, 50]);
        let t = urange(r, 0.0, 80.0);
        cx.game(0x1610b0, &[(PROP + 0x110) as u64, index as u64], &[t]);
        let mut h = cx.host();
        pr::motion_start(&mut cx.w.ram, &mut h, t, PROP + 0x110, index);
        if cx.w.ram.r32(PROP + 0x110 + 0x48) != 0 {
            started += 1;
        } else {
            none += 1;
        }
    });
    assert!((started > 30 && none > 30) || kh2_factory::exe().is_none(), "{started} started, {none} without a motion");
}

#[test]
fn orig_161060_matches_the_game() {
    cases("0x161060", 300, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        cx.game(0x161060, &[(PROP + 0x110) as u64], &[]);
        let mut h = cx.host();
        pr::motion_stop(&mut cx.w.ram, &mut h, PROP + 0x110);
    });
}

#[test]
fn orig_15fef8_matches_the_game() {
    cases("0x15fef8", 400, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        let u = urange(r, 0.0, 5.0);
        let step = pick(r, &[0.0f32, 1.0, 2.0, u]);
        cx.w.global(G_STEP, step.to_bits());
        cx.game(0x15fef8, &[PROP as u64], &[]);
        let mut h = cx.host();
        pr::sky_player(&mut cx.w.ram, &mut h, PROP);
    });
}

#[test]
fn orig_160408_matches_the_game() {
    cases("0x160408", 100, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        let a1 = r.word();
        cx.game(0x160408, &[PROP as u64, a1], &[]);
        let mut h = cx.host();
        pr::prop_instance_call(&mut cx.w.ram, &mut h, PROP, a1);
    });
}

#[test]
fn orig_160068_matches_the_game() {
    cases("0x160068", 600, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        let m = rand_m4(r);
        cx.w.put(MAT, &m4_bytes(&m));
        cx.game(0x160068, &[PROP as u64, MAT as u64], &[]);
        let m = cx.w.ram.m4(MAT);
        pr::corners_box(&mut cx.w.ram, PROP, &m);
    });
}

#[test]
fn orig_1601a8_matches_the_game() {
    cases("0x1601a8", 600, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        cx.game(0x1601a8, &[PROP as u64], &[]);
        let mut h = cx.host();
        pr::world_box(&mut cx.w.ram, &mut h, PROP);
    });
}

#[test]
fn orig_160388_matches_the_game() {
    let (mut set, mut kept) = (0, 0);
    cases("0x160388", 600, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        cx.game(0x160388, &[PROP as u64], &[]);
        let mut h = cx.host();
        pr::prop_centre(&mut cx.w.ram, &mut h, PROP);
        let after = cx.w.ram.rf(PROP + 0x284);
        // The centre's height is the box centre's unless the player's height replaced it.
        if d_player(cx.salt).is_some_and(|p| p[1].to_bits() == after.to_bits()) {
            set += 1;
        } else {
            kept += 1;
        }
    });
    assert!(kh2_factory::exe().is_none() || set > 0 && kept > 0, "{set} cases replaced the height, {kept} did not");
}

#[test]
fn orig_15ffc0_matches_the_game() {
    cases("0x15ffc0", 600, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        cx.game(0x15ffc0, &[PROP as u64], &[]);
        let mut h = cx.host();
        pr::prop_matrix(&mut cx.w.ram, &mut h, PROP);
    });
}

#[test]
fn orig_160428_matches_the_game() {
    let (mut moving, mut fixed, mut centre) = (0, 0, 0);
    cases("0x160428", 800, |cx, r, _| {
        let mut p = prop_bytes(r);
        let n = node_count(r);
        w16(&mut p, 0x2d8, r.below(n as u64) as u16);
        w16(&mut p, 0x2da, r.below(3) as u16);
        // no children chained to the draw object
        w32(&mut p, 0x2ac, 0);
        w32(&mut p, 0x2b0, 0);
        let draw_flags = (r.word() as u32 & 1) | pick(r, &[0u32, 4, 8, 4 | 8]) | pick(r, &[0u32, 0x10]) | pick(r, &[0u32, 0x1000]);
        w32(&mut p, 0x2b8, draw_flags);
        p[0x2ab] = pick(r, &[0u8, 0x20, 0x7f, 0x80, 0xff]);
        cx.w.put(PROP, &p);
        put_tree(cx, r, n);
        cx.game(0x160428, &[PROP as u64], &[]);
        let mut h = cx.host();
        pr::prop_frame(&mut cx.w.ram, &mut h, PROP);
        moving += (p[0x180] & 2 != 0) as u32;
        fixed += (p[0x180] & 2 == 0) as u32;
        centre += (draw_flags & 1) as u32;
    });
    assert!(kh2_factory::exe().is_none() || moving > 100 && fixed > 100 && centre > 100, "{moving} moving, {fixed} fixed, {centre} with the centre");
}

/// Writes the port's task fields into the block the game's `0x102778` hook handed out, so the region compare covers them, and
/// adds the create call to the port's log the way the game's hook logs it.
fn sync_task(cx: &mut Cx, t: u32) {
    let task = cx.tasks.task(t).expect("the task").clone();
    cx.w.ram.w32(TASKBLK + 4, task.arg());
    cx.w.ram.w32(TASKBLK + 0x30, task.finalizer());
    assert_eq!(task.owner(), cx.manager);
    cx.port_log.borrow_mut().push(vec![0x102778, MGR_GAME, task.bits(), task.priority() as u32, task.function()]);
}

#[test]
fn orig_160588_matches_the_game() {
    cases("0x160588", 100, |cx, r, _| {
        cx.w.put(PROP, &prop_bytes(r));
        let prio = { let w = r.word(); pick(r, &[0i32, 1, 300, 5000, -5, w as i32]) };
        cx.game(0x160588, &[PROP as u64, prio as i64 as u64], &[]);
        let t = pr::create_prop_task(&mut cx.tasks, cx.manager, PROP, prio);
        sync_task(cx, t);
    });
}

#[test]
fn orig_1604a8_matches_the_game() {
    let (mut dead, mut ran, mut held) = (0, 0, 0);
    cases("0x1604a8", 600, |cx, r, _| {
        let p = prop_bytes(r);
        cx.w.put(PROP, &p);
        cx.w.put(PROP2, &prop_bytes(r));
        let mut tb = vec![0u8; 0x48];
        w32(&mut tb, 4, PROP);
        cx.w.put(TASKBLK, &tb);
        let flag = { let w = r.word(); pick(r, &[0u32, 0x20, 0x21, 0xffff_ffdf, w as u32]) };
        cx.w.global(pr::FLAG_WORD, flag);
        let hold = pick(r, &[0u8, 0, 0, 1, 0xff]);
        cx.w.global8(pr::HOLD_BYTE, hold);
        let (head, tail) = if r.below(3) == 0 { (0, 0) } else { (PROP2, PROP2) };
        cx.w.global(pr::TASK_LIST_HEAD, head);
        cx.w.global(pr::TASK_LIST_TAIL, tail);
        cx.game(0x1604a8, &[TASKBLK as u64], &[]);
        let t = cx.tasks.create_task(cx.manager, 0, 0, pr::TASK_FUNCTION);
        cx.tasks.task_mut(t).unwrap().set_arg(PROP);
        let mut h = cx.host();
        // The port's task has its own handle; the dead-prop report names the task, the game's is the block address.
        pr::prop_task(&mut cx.w.ram, &mut h, &cx.tasks, t);
        let mut log = cx.port_log.borrow_mut();
        for e in log.iter_mut() {
            if e[0] == 0x102498 {
                e[1] = TASKBLK;
            }
        }
        if p[0x180] & 1 != 0 {
            dead += 1;
        } else if flag & 0x20 != 0 || hold != 0 {
            held += 1;
        } else {
            ran += 1;
        }
    });
    assert!(kh2_factory::exe().is_none() || dead > 50 && ran > 50 && held > 50, "{dead} dead, {ran} ran, {held} held");
}

#[test]
fn orig_15fdc0_and_15fe50_match_the_game() {
    for (addr, deleting) in [(0x15fdc0u32, false), (0x15fe50, true)] {
        cases(&format!("{addr:#x}"), 200, |cx, r, _| {
            cx.w.put(PROP, &prop_bytes(r));
            cx.game(addr, &[PROP as u64], &[]);
            let mut h = cx.host();
            if deleting {
                pr::prop_delete(&mut cx.w.ram, &mut h, PROP);
            } else {
                pr::prop_drop(&mut cx.w.ram, &mut h, PROP);
            }
        });
    }
}

/// The constructor arguments of a case; the vectors are put in game memory for the original and handed by value to the port.
fn init_args(cx: &mut Cx, r: &mut Rng, flags: u32) -> InitArgs {
    let v = |r: &mut Rng, s: f32| [urange(r, -s, s), urange(r, -s, s), urange(r, -s, s), 1.0];
    let position = v(r, 200.0);
    let rotation = v(r, 3.0);
    let mut scale = v(r, 2.0);
    if r.below(3) == 0 {
        scale[r.below(3) as usize] = 1.0;
    }
    cx.w.put(VEC_A, &v4_bytes(&position));
    cx.w.put(VEC_B, &v4_bytes(&rotation));
    cx.w.put(VEC_C, &v4_bytes(&scale));
    InitArgs {
        tint: r.word() as u32,
        position,
        rotation,
        scale,
        priority: { let w = r.word(); pick(r, &[0i32, 100, 3000, -2, w as i32]) },
        motion: pick(r, &[-1i32, -1, 0, 1, 3, 8]),
        start: pick(r, &[0i32, 1, 5, 30, -3]),
        extra: r.word() as u32,
        flags,
    }
}

fn run_init(addr: u32, cases_n: u32) {
    let (mut floors, mut motions, mut owner_m1, mut multi, mut centred, mut negative) = (0, 0, 0, 0, 0, 0);
    cases(&format!("{addr:#x}"), cases_n, |cx, r, _| {
        let n = node_count(r);
        let mut p = prop_bytes(r);
        w16(&mut p, 0x2d8, r.below(n as u64) as u16);
        cx.w.put(PROP, &p);
        cx.w.put(PROP2, &prop_bytes(r));
        put_tree(cx, r, n);
        cx.w.put(TASKBLK, &[0u8; 0x48]);
        let (head, tail) = if r.below(3) == 0 { (0, 0) } else { (PROP2, PROP2) };
        cx.w.global(pr::LIST_HEAD, head);
        cx.w.global(pr::LIST_TAIL, tail);
        let flags = { let w = r.word(); pick(r, &[0u32, 1, 2, 3, w as u32 & 3]) };
        let a = init_args(cx, r, flags);
        let model = r.below(3) as u32;
        let stack = STACK;
        cx.w.ee.w32(stack, a.start as u32);
        cx.w.ee.w32(stack + 8, a.extra);
        cx.w.ee.w32(stack + 16, a.flags);
        cx.game(
            addr,
            &[PROP as u64, a.tint as u64, model as u64, VEC_A as u64, VEC_B as u64, VEC_C as u64, a.priority as i64 as u64, a.motion as i64 as u64],
            &[],
        );
        let mut h = cx.host();
        let tasks = &mut cx.tasks;
        pr::prop_init(&mut cx.w.ram, &mut h, tasks, cx.manager, PROP, model, &a);
        let t = cx.tasks.list(cx.manager)[0];
        sync_task(cx, t);
        floors += (flags & 2 != 0) as u32;
        centred += (flags & 1 != 0) as u32;
        motions += (cx.w.ram.r32(PROP + 0x110 + 0x48) != 0) as u32;
        owner_m1 += (cx.w.ram.r32(PROP + 0x2d4) == a.tint) as u32;
        multi += (cx.w.ram.r32(PROP + 0x2b8) & 0x100 != 0) as u32;
        negative += (cx.w.ram.r32(PROP + 0x1c8) & 8 != 0) as u32;
    });
    let ok = kh2_factory::exe().is_none() || (floors > 50 && motions > 50 && owner_m1 > 50 && multi > 50 && centred > 50 && negative > 50);
    assert!(ok, "{floors} floor colours, {motions} motions, {owner_m1} tints, {multi} multi, {centred} centres, {negative} mirrored");
}

#[test]
fn orig_15f490_matches_the_game() {
    run_init(0x15f490, 400);
}

#[test]
fn orig_15f8e0_matches_the_game() {
    run_init(0x15f8e0, 400);
}

#[test]
fn orig_15f3a0_matches_the_game() {
    let (mut sky0, mut sky1) = (0, 0);
    cases("0x15f3a0", 400, |cx, r, _| {
        let n = node_count(r);
        let mut p = prop_bytes(r);
        w16(&mut p, 0x2d8, r.below(n as u64) as u16);
        cx.w.put(PROP, &p);
        cx.w.put(PROP2, &prop_bytes(r));
        put_tree(cx, r, n);
        cx.w.put(TASKBLK, &[0u8; 0x48]);
        let (head, tail) = if r.below(3) == 0 { (0, 0) } else { (PROP2, PROP2) };
        cx.w.global(pr::LIST_HEAD, head);
        cx.w.global(pr::LIST_TAIL, tail);
        let a = init_args(cx, r, 0);
        let model = r.below(3) as u32;
        let sky = pick(r, &[0i64, 1, 2, -1, 0x1_0000_0000]);
        cx.game(
            0x15f3a0,
            &[PROP as u64, model as u64, VEC_A as u64, VEC_B as u64, VEC_C as u64, a.priority as i64 as u64, sky as u64],
            &[],
        );
        let mut h = cx.host();
        let tasks = &mut cx.tasks;
        pr::sky_prop_init(&mut cx.w.ram, &mut h, tasks, cx.manager, PROP, model, a.position, a.rotation, a.scale, a.priority, sky);
        let t = cx.tasks.list(cx.manager)[0];
        sync_task(cx, t);
        if sky == 0 {
            sky0 += 1;
        } else {
            sky1 += 1;
        }
    });
    assert!(kh2_factory::exe().is_none() || sky0 > 50 && sky1 > 50, "{sky0} first skies, {sky1} others");
}
