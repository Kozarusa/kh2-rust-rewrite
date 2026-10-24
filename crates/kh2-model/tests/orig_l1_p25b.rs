//! Oracle tests of the object pass (WP-L1-P25b, `kh2_model::draw::pass`): kh2-ee runs the original from the user's own
//! executable on a scene laid out in its memory (a draw tree with its slot table, view matrices and counters, draw objects
//! with children, part alpha and distance records, a camera block, the layer buckets and the scratchpad arrays), the port
//! runs on a byte-for-byte copy of the same scene in a sparse memory, and afterwards every region of the scene, the global
//! bytes and the log of the calls both sides hand to other units are compared. Floats are compared by bits.
//!
//! Callees that belong to other units are replaced by hooks that log their arguments (and write what the unit would): the
//! frame profiler 0x1b62a8 / 0x1b62d8, the scene event test 0x224b18, the part switches 0x11c580 / 0x11c5b0, the model
//! draws 0x11cb38 / 0x11c7d8 (the second returns a draw record from a table), the light matrices 0x134bd8, the shadow pass
//! 0x148d58 / 0x148e08 (a list chosen from the object), the effect creator 0x1da840, the projection update 0x106a68 (a
//! pure stand-in on the block, the same function on both sides) and 0x1084f8. Everything else runs as the game has it: the
//! billboard, the sort key, the level of detail, the sort, the lists, the layers and the pass itself.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::{Rng, STACK};
use kh2_model::draw::pass::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

const TREE: u32 = 0x0181_0000;
const TREE_LEN: u32 = 0x5400;
const OBJ0: u32 = 0x0185_0000;
const OBJ_STRIDE: u32 = 0x200;
const OBJ_MAX: u32 = 40;
const AUX0: u32 = 0x0187_0000;
const AUX_STRIDE: u32 = 0x200;
const RECS: u32 = 0x018a_0000;
const RECS_LEN: u32 = 0x4000;
const MISC: u32 = 0x018b_0000;
const MISC_LEN: u32 = 0x1000;
const SPR: u32 = 0x7000_0000;
const SPR_LEN: u32 = 0x1800;
const BUCKETS_LEN: u32 = 0x400;
const CAM_BASE: u32 = 0x3b25f0;
const G_VIEW: u32 = 0x349e40;
const G_SEL: u32 = 0x349e44;
const G_TREE: u32 = 0x355930;
const G_MAP_OFF: u32 = 0x35594d;

// ---------------------------------------------------------------------------------------------------------------------
// the port's memory
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Default)]
struct Sparse {
    pages: HashMap<u32, Box<[u8; 4096]>>,
}

impl Sparse {
    fn page(&mut self, a: u32) -> &mut [u8; 4096] {
        self.pages.entry(a / 4096).or_insert_with(|| Box::new([0; 4096]))
    }
    fn read(&self, a: u32, n: u32) -> Vec<u8> {
        (0..n).map(|i| self.r8(a + i)).collect()
    }
    fn write(&mut self, a: u32, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.w8(a + i as u32, *v);
        }
    }
}

impl Words for Sparse {
    fn r8(&self, a: u32) -> u8 {
        self.pages.get(&(a / 4096)).map_or(0, |p| p[(a % 4096) as usize])
    }
    fn w8(&mut self, a: u32, v: u8) {
        self.page(a)[(a % 4096) as usize] = v;
    }
    fn r16(&self, a: u32) -> u16 {
        u16::from_le_bytes([self.r8(a), self.r8(a + 1)])
    }
    fn w16(&mut self, a: u32, v: u16) {
        self.write(a, &v.to_le_bytes());
    }
    fn r32(&self, a: u32) -> u32 {
        u32::from_le_bytes(std::array::from_fn(|i| self.r8(a + i as u32)))
    }
    fn w32(&mut self, a: u32, v: u32) {
        self.write(a, &v.to_le_bytes());
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// the hooks and the port's host
// ---------------------------------------------------------------------------------------------------------------------

type Log = Rc<RefCell<Vec<Vec<u32>>>>;

#[derive(Clone, Copy, Default)]
struct Params {
    scene_event: bool,
    seed: u32,
}

fn mix(a: u32, b: u32) -> u32 {
    (a ^ b.rotate_left(7)).wrapping_mul(0x9e37_79b1).rotate_left(11) ^ b.wrapping_mul(0x85eb_ca6b)
}

/// The projection update stand-in: the first 0x40 bytes become a function of the block's other words (the field of view
/// words +0x44 and +0x48 and the unwritten word +0x7c are left out: the first holds the library arctangent's result, the
/// second is stack garbage in the original), the whole fourth row is 0 (the billboard draw with a projection pointer adds a stack word it never wrote to the translation row
/// before the multiply; a zero fourth row makes the answer independent of that word).
fn proj_stand_in(blk: &mut [u8; 0x100], seed: u32) {
    let mut h = mix(seed, 0x106a68);
    for (i, b) in blk.iter().enumerate() {
        if i >= 0x4c && !(0x7c..0x80).contains(&i) {
            h = mix(h, *b as u32);
        }
    }
    for r in 0..4 {
        for c in 0..4 {
            h = mix(h, (r * 4 + c) as u32);
            let v = if r == 3 { 0.0 } else { ((h >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 3.0 };
            blk[16 * r + 4 * c..16 * r + 4 * c + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
}

/// The shadow list the stand-in collects for an object: 0 to 3 entries derived from the object's address.
fn shadow_list(seed: u32, obj: u32) -> Vec<u32> {
    let n = mix(seed, obj) % 4;
    (0..n).map(|k| obj.wrapping_add(0x10000 * (k + 1))).collect()
}

struct CaseHost {
    log: Vec<Vec<u32>>,
    params: Params,
    next_rec: u32,
}

fn words16(m: &dyn Words, a: u32) -> Vec<u32> {
    (0..16).map(|k| m.r32(a + 4 * k)).collect()
}

fn words_of(b: &[u8; 64]) -> Vec<u32> {
    b.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
}

impl PassHost for CaseHost {
    fn profile_begin(&mut self, _m: &mut dyn Words) {
        self.log.push(vec![0x1b62a8]);
    }
    fn profile_end(&mut self, _m: &mut dyn Words) {
        self.log.push(vec![0x1b62d8]);
    }
    fn scene_event_running(&mut self, _m: &mut dyn Words) -> bool {
        self.log.push(vec![0x224b18]);
        self.params.scene_event
    }
    fn show_part(&mut self, _m: &mut dyn Words, model: u32, part: u32) {
        self.log.push(vec![0x11c580, model, part]);
    }
    fn hide_part(&mut self, _m: &mut dyn Words, model: u32, part: u32) {
        self.log.push(vec![0x11c5b0, model, part]);
    }
    fn draw_model_groups(&mut self, m: &mut dyn Words, model: u32, screen: u32, matrix: u32, state: u32, list: &[u16]) {
        let mut v = vec![0x11cb38, model, screen, matrix, state];
        v.extend(words16(m, screen));
        v.extend(words16(m, matrix));
        v.extend(list.iter().map(|x| *x as u32));
        self.log.push(v);
    }
    fn draw_model(&mut self, _m: &mut dyn Words, model: u32, a1: &[u8; 64], a2: &[u8; 64], state: u32, mode: u8) -> u32 {
        let mut v = vec![0x11c7d8, model, state, mode as u32];
        v.extend(words_of(a1));
        v.extend(words_of(a2));
        self.log.push(v);
        let rec = RECS + 0x100 * self.next_rec;
        self.next_rec += 1;
        rec
    }
    fn light_matrices(&mut self, _m: &mut dyn Words, z: f32, out: u32, matrix: u32, lights: u32) {
        self.log.push(vec![0x134bd8, z.to_bits(), out, matrix, lights]);
    }
    fn shadow_collect(&mut self, _m: &mut dyn Words, tree: u32, obj: u32) -> Vec<u32> {
        self.log.push(vec![0x148d58, tree, obj]);
        shadow_list(self.params.seed, obj)
    }
    fn shadow_draw(&mut self, _m: &mut dyn Words, tree: u32, objs: &[u32]) {
        let mut v = vec![0x148e08, tree, objs.len() as u32];
        v.extend(objs);
        v.push(0);
        self.log.push(v);
    }
    fn effect_create(&mut self, _m: &mut dyn Words, at: u32) {
        self.log.push(vec![0x1da840, at]);
    }
    fn projection_update(&mut self, _m: &mut dyn Words, proj: &mut [u8; 0x100]) {
        let mut v = vec![0x106a68];
        for (i, c) in proj.chunks(4).enumerate() {
            if !(0x10..0x13).contains(&i) && i != 0x1f {
                v.push(u32::from_le_bytes(c.try_into().unwrap()));
            }
        }
        self.log.push(v);
        proj_stand_in(proj, self.params.seed);
    }
    fn init_shadow_block(&mut self, _m: &mut dyn Words, block: u32) {
        self.log.push(vec![0x1084f8, block]);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// the machine
// ---------------------------------------------------------------------------------------------------------------------

struct W {
    ee: Ee,
    ram: Sparse,
    regions: Vec<(u32, u32)>,
    globals: Vec<(u32, u8)>,
    log: Log,
    params: Rc<Cell<Params>>,
    rec_counter: Rc<Cell<u32>>,
    scissor: Rc<RefCell<Vec<u64>>>,
}

impl W {
    fn new(what: &str) -> Option<W> {
        let elf = kh2_factory::exe_or_skip(what)?;
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let params = Rc::new(Cell::new(Params::default()));
        let rec_counter = Rc::new(Cell::new(0u32));
        let scissor = Rc::new(RefCell::new(Vec::new()));
        let a = |e: &Ee, k: usize| e.gpr[reg::A0 + k] as u32;
        {
            let l = log.clone();
            ee.hooks.insert(0x1b62a8, move |_e: &mut Ee| l.borrow_mut().push(vec![0x1b62a8]));
            let l = log.clone();
            ee.hooks.insert(0x1b62d8, move |_e: &mut Ee| l.borrow_mut().push(vec![0x1b62d8]));
            let (l, p) = (log.clone(), params.clone());
            ee.hooks.insert(0x224b18, move |e: &mut Ee| {
                l.borrow_mut().push(vec![0x224b18]);
                e.gpr[reg::V0] = p.get().scene_event as u128;
            });
            for addr in [0x11c580u32, 0x11c5b0] {
                let l = log.clone();
                ee.hooks.insert(addr, move |e: &mut Ee| l.borrow_mut().push(vec![addr, a(e, 0), a(e, 1)]));
            }
            let l = log.clone();
            ee.hooks.insert(0x11cb38, move |e: &mut Ee| {
                let (model, screen, matrix, state, list) = (a(e, 0), a(e, 1), a(e, 2), a(e, 3), a(e, 4));
                let mut v = vec![0x11cb38, model, screen, matrix, state];
                v.extend((0..16).map(|k| e.r32(screen + 4 * k)));
                v.extend((0..16).map(|k| e.r32(matrix + 4 * k)));
                for k in 0..8 {
                    let g = e.r16(list + 2 * k) as u32;
                    v.push(g);
                    if g == 0xffff {
                        break;
                    }
                }
                l.borrow_mut().push(v);
            });
            let (l, rc) = (log.clone(), rec_counter.clone());
            ee.hooks.insert(0x11c7d8, move |e: &mut Ee| {
                let (model, a1, a2, state, mode) = (a(e, 0), a(e, 1), a(e, 2), a(e, 3), e.gpr[reg::A0 + 4] as u8 as u32);
                let mut v = vec![0x11c7d8, model, state, mode];
                v.extend((0..16).map(|k| e.r32(a1 + 4 * k)));
                v.extend((0..16).map(|k| e.r32(a2 + 4 * k)));
                l.borrow_mut().push(v);
                e.gpr[reg::V0] = (RECS + 0x100 * rc.get()) as u128;
                rc.set(rc.get() + 1);
            });
            let l = log.clone();
            ee.hooks.insert(0x134bd8, move |e: &mut Ee| {
                l.borrow_mut().push(vec![0x134bd8, e.fpr[12].to_bits(), a(e, 0), a(e, 1), a(e, 2)]);
            });
            let (l, p) = (log.clone(), params.clone());
            ee.hooks.insert(0x148d58, move |e: &mut Ee| {
                let (tree, obj, list, count) = (a(e, 0), a(e, 1), a(e, 2), a(e, 3));
                l.borrow_mut().push(vec![0x148d58, tree, obj]);
                let items = shadow_list(p.get().seed, obj);
                for (k, it) in items.iter().enumerate() {
                    e.w32(list + 4 * k as u32, *it);
                }
                e.w32(count, items.len() as u32);
            });
            let l = log.clone();
            ee.hooks.insert(0x148e08, move |e: &mut Ee| {
                let (tree, list, n) = (a(e, 0), a(e, 1), a(e, 2));
                let mut v = vec![0x148e08, tree, n];
                v.extend((0..=n.min(40)).map(|k| e.r32(list + 4 * k)));
                l.borrow_mut().push(v);
            });
            let l = log.clone();
            ee.hooks.insert(0x1da840, move |e: &mut Ee| l.borrow_mut().push(vec![0x1da840, a(e, 0)]));
            let (l, p) = (log.clone(), params.clone());
            ee.hooks.insert(0x106a68, move |e: &mut Ee| {
                let at = a(e, 0);
                let mut blk = [0u8; 0x100];
                for (i, b) in blk.iter_mut().enumerate() {
                    *b = e.r8(at + i as u32);
                }
                let mut v = vec![0x106a68];
                for (i, c) in blk.chunks(4).enumerate() {
                    if !(0x10..0x13).contains(&i) && i != 0x1f {
                        v.push(u32::from_le_bytes(c.try_into().unwrap()));
                    }
                }
                l.borrow_mut().push(v);
                proj_stand_in(&mut blk, p.get().seed);
                e.write(at, &blk[..0x40]);
            });
            let l = log.clone();
            ee.hooks.insert(0x1084f8, move |e: &mut Ee| l.borrow_mut().push(vec![0x1084f8, a(e, 0)]));
            ee.hooks.insert(0x134638, |_e: &mut Ee| {});
            ee.hooks.insert(0x134680, |_e: &mut Ee| {});
            let s = scissor.clone();
            ee.hooks.insert(0x134658, move |e: &mut Ee| {
                s.borrow_mut().push(e.gpr[reg::A0 + 1] as u64);
            });
        }
        Some(W { ee, ram: Sparse::default(), regions: Vec::new(), globals: Vec::new(), log, params, rec_counter, scissor })
    }

    fn reset(&mut self, params: Params) {
        self.ram = Sparse::default();
        self.regions.clear();
        self.globals.clear();
        self.log.borrow_mut().clear();
        self.scissor.borrow_mut().clear();
        self.params.set(params);
        self.rec_counter.set(0);
    }

    fn put(&mut self, a: u32, b: &[u8]) {
        self.ee.write(a, b);
        self.ram.write(a, b);
        self.regions.push((a, b.len() as u32));
    }

    fn global32(&mut self, a: u32, v: u32) {
        self.ee.w32(a, v);
        self.ram.w32(a, v);
        self.globals.push((a, 4));
    }

    fn global8(&mut self, a: u32, v: u8) {
        self.ee.write(a, &[v]);
        self.ram.w8(a, v);
        self.globals.push((a, 1));
    }

    fn call(&mut self, addr: u32, args: &[u64], floats: &[f32]) -> u64 {
        // The stack below the call starts zeroed (the original reads stack words it never wrote in one rare path).
        self.ee.write(STACK - 0x1000, &[0u8; 0x1000]);
        self.ee.call(addr, args, floats, STACK).unwrap_or_else(|f| panic!("the original faulted at {addr:#x}: {f:?}"))
    }

    fn host(&self) -> CaseHost {
        CaseHost { log: Vec::new(), params: self.params.get(), next_rec: 0 }
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

    /// Compares every region, global and the log of calls.
    fn check(&mut self, what: &str, port_log: &[Vec<u32>]) {
        for (a, len) in self.regions.clone() {
            let g = self.game_bytes(a, len);
            let p = self.ram.read(a, len);
            if g != p {
                let i = (0..len as usize).find(|&i| g[i] != p[i]).unwrap();
                panic!("{what}: region {a:#x}+{i:#x} differs: game {:02x?} port {:02x?}", &g[i..(i + 8).min(len as usize)], &p[i..(i + 8).min(len as usize)]);
            }
        }
        for (a, width) in self.globals.clone() {
            if width == 1 {
                assert_eq!(self.ee.r8(a), self.ram.r8(a), "{what}: global byte {a:#x}");
            } else {
                assert_eq!(self.ee.r32(a), self.ram.r32(a), "{what}: global {a:#x}");
            }
        }
        let game_log = self.log.borrow().clone();
        assert_eq!(game_log.len(), port_log.len(), "{what}: the number of host calls (game {:x?} port {:x?})", game_log.iter().map(|v| v[0]).collect::<Vec<_>>(), port_log.iter().map(|v| v[0]).collect::<Vec<_>>());
        for (i, (g, p)) in game_log.iter().zip(port_log).enumerate() {
            assert_eq!(g, p, "{what}: host call {i} differs (game {:x?} port {:x?})", g, p);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// the scene
// ---------------------------------------------------------------------------------------------------------------------

fn urange(r: &mut Rng, lo: f32, hi: f32) -> f32 {
    lo + (r.word() >> 40) as f32 / (1u64 << 24) as f32 * (hi - lo)
}

fn chance(r: &mut Rng, n: u64) -> bool {
    r.below(n) == 0
}

fn put32(b: &mut [u8], o: u32, v: u32) {
    b[o as usize..o as usize + 4].copy_from_slice(&v.to_le_bytes());
}

fn putf(b: &mut [u8], o: u32, v: f32) {
    put32(b, o, v.to_bits());
}

fn put16(b: &mut [u8], o: u32, v: u16) {
    b[o as usize..o as usize + 2].copy_from_slice(&v.to_le_bytes());
}

fn rand_bytes(r: &mut Rng, n: usize) -> Vec<u8> {
    (0..n).map(|_| r.word() as u8).collect()
}

struct Scene {
    tree: Vec<u8>,
    objs: Vec<u8>,
    aux: Vec<u8>,
    n: u32,
    view: u32,
    sel: u8,
    cam: Vec<u8>,
    buckets: Vec<u8>,
    spr: Vec<u8>,
    misc: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Opt {
    objects: (u64, u64),
    /// Probability (out of 8) that a flag bit is set, for the bits of the object pass.
    flag_density: u64,
}

const PASS: Opt = Opt { objects: (1, 24), flag_density: 4 };

fn obj_addr(k: u32) -> u32 {
    OBJ0 + OBJ_STRIDE * k
}

fn aux_addr(k: u32) -> u32 {
    AUX0 + AUX_STRIDE * k
}

fn rand_matrix(r: &mut Rng, b: &mut [u8], at: u32, span: f32) {
    for k in 0..16 {
        putf(b, at + 4 * k, if chance(r, 12) { r.float() } else { urange(r, -span, span) });
    }
}

fn scene(r: &mut Rng, opt: Opt) -> Scene {
    let n = (opt.objects.0 + r.below(opt.objects.1 - opt.objects.0 + 1)) as u32;
    let mut tree = rand_bytes(r, TREE_LEN as usize);
    let mut objs = rand_bytes(r, (OBJ_STRIDE * OBJ_MAX) as usize);
    let mut aux = rand_bytes(r, (AUX_STRIDE * OBJ_MAX) as usize);
    let view = r.below(3) as u32;
    // view matrix near the identity, so the object depth is about its position's z plus a shift
    let mut vm = [[0f32; 4]; 4];
    for (i, row) in vm.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x = if i == j { 1.0 } else { urange(r, -0.2, 0.2) };
        }
    }
    vm[3] = [urange(r, -100.0, 100.0), urange(r, -100.0, 100.0), urange(r, -200.0, 200.0), 1.0];
    for (i, row) in vm.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            putf(&mut tree, 0x130 + 16 * i as u32 + 4 * j as u32, *x);
        }
    }
    rand_matrix(r, &mut tree, 0x1b0, 3.0);
    for k in 0..0x40 {
        putf(&mut tree, 0x20 + 4 * k, urange(r, -2.0, 2.0));
    }
    putf(&mut tree, 0x20 + 0x4c, urange(r, 0.5, 2.0));
    putf(&mut tree, 0x20 + 0x50, urange(r, 0.5, 2.0));
    for k in 0x80 / 4..0x100 / 4 {
        putf(&mut tree, 0x20 + 4 * k, urange(r, -2.0, 2.0));
    }
    putf(&mut tree, 0x774, urange(r, -100.0, 700.0));
    tree[0x778] = chance(r, 2) as u8;
    tree[0x779] = chance(r, 4) as u8;
    put32(&mut tree, 0x536c, view);
    put32(&mut tree, 0x5360, n);
    put32(&mut tree, 0xc40, r.below(4) as u32);
    for v in 0..3 {
        put32(&mut tree, 0xc50 + 4 * v, if chance(r, 3) { 12 + r.below(6) as u32 } else { r.below(4) as u32 });
        put32(&mut tree, 0x77c + 4 * v, r.below(3) as u32);
    }
    for b in 0..16 {
        put32(&mut tree, 0xc60 + 0x370 * b + 0x360, if chance(r, 3) { 0 } else { r.below(3) as u32 });
    }
    // the parent of each object (or none)
    let mut parent = vec![u32::MAX; n as usize];
    let mut last_child: HashMap<u32, u32> = HashMap::new();
    for k in 0..n {
        if k > 0 && chance(r, 3) {
            parent[k as usize] = r.below(k as u64) as u32;
        }
    }
    let mut first_child: HashMap<u32, u32> = HashMap::new();
    let mut sibling: HashMap<u32, u32> = HashMap::new();
    for k in 0..n {
        let p = parent[k as usize];
        if p != u32::MAX {
            match last_child.get(&p) {
                Some(&l) => {
                    sibling.insert(l, k);
                }
                None => {
                    first_child.insert(p, k);
                }
            }
            last_child.insert(p, k);
        }
    }
    let dens = opt.flag_density;
    for k in 0..n {
        let o = (OBJ_STRIDE * k) as usize;
        let ob = &mut objs[o..o + OBJ_STRIDE as usize];
        put32(ob, 0, 0x0100_0000 + 0x100 * k);
        put32(ob, 0x2c, [1, 2, 3, 4, 2, 1][r.below(6) as usize]);
        put32(ob, 0x34, if chance(r, 2) { aux_addr(k) } else { 0 });
        put32(ob, 0x38, if chance(r, 3) { 0 } else { 0x0199_0000 + 0x100 * k });
        put32(ob, 0x100, if chance(r, 3) { 0 } else { 0x019a_0000 + 0x100 * k });
        rand_matrix(r, ob, 0x40, 2.0);
        rand_matrix(r, ob, 0x80, 2.0);
        putf(ob, 0xc0, urange(r, -300.0, 300.0));
        putf(ob, 0xc4, urange(r, -300.0, 300.0));
        putf(ob, 0xc8, urange(r, -100.0, 900.0));
        putf(ob, 0xcc, if chance(r, 6) { r.float() } else { 1.0 });
        ob[0xeb] = [0, 0x40, 0x7f, 0x80, 0xff, r.word() as u8, 1][r.below(7) as usize];
        put32(ob, 0xec, sibling.get(&k).map_or(0, |s| obj_addr(*s)));
        put32(ob, 0xf0, first_child.get(&k).map_or(0, |c| obj_addr(*c)));
        let mut flags = r.word() as u32 & 0x2f_ffff;
        for bit in [1u32, 4, 0x10, 0x40, 0x80, 0x100, 0x1000, 0x2000, 0x4000] {
            flags = if r.below(8) < dens || (bit == 4 && !chance(r, 4)) { flags | bit } else { flags & !bit };
        }
        put32(ob, 0xf8, flags);
        putf(ob, 0x108, if chance(r, 3) { 0.0 } else { urange(r, -50.0, 1200.0) });
        ob[0x10c] = r.word() as u8;
        ob[0x10d] = r.below(3) as u8;
        for v in 0..4 {
            ob[0x10e + v] = if chance(r, 5) { r.word() as u8 } else { r.below(3) as u8 };
        }
        put32(ob, 0x114, aux_addr(k) + 0x40);
        put16(ob, 0x118, r.word() as u16);
        put16(ob, 0x11c, k as u16);
        ob[0x11f] = if chance(r, 2) { 0 } else { r.below(6) as u8 };
        put32(ob, 0x120, if chance(r, 2) { aux_addr(k) + 0x100 } else { 0 });
    }
    // distance records: the offsets are relative to the record pointer (+0x114), which is aux + 0x40: its floats sit at
    // +0x38.. +0x64, so the record spans aux +0x78 .. +0xa4
    for k in 0..n {
        let a = (AUX_STRIDE * k) as usize + 0x40;
        let mut d = urange(r, 0.0, 300.0);
        for off in [0x38usize, 0x3c, 0x40, 0x44] {
            d += if chance(r, 8) { -urange(r, 0.0, 50.0) } else { urange(r, 0.0, 300.0) };
            putf(&mut aux, (a + off) as u32, d);
        }
        for (near, far) in [(0x60usize, 0x64usize), (0x58, 0x5c), (0x50, 0x54)] {
            let lo = urange(r, 0.0, 600.0);
            let hi = if chance(r, 10) { lo } else { lo + urange(r, 0.0, 600.0) };
            putf(&mut aux, (a + near) as u32, lo);
            putf(&mut aux, (a + far) as u32, hi);
        }
        // the matrix the projection pointer names
        let m = a - 0x40 + 0x100;
        for j in 0..16u32 {
            putf(&mut aux, m as u32 + 4 * j, urange(r, -2.0, 2.0));
        }
        putf(&mut aux, m as u32 + 0x3c, urange(r, 0.3, 2.0));
    }
    // slots
    for k in 0..n {
        let s = 0x4360 + 8 * k;
        put32(&mut tree, s, obj_addr(k));
        let mut sf = r.word() as u32 & !3;
        if !chance(r, 6) {
            sf |= 2;
        }
        if !chance(r, 4) {
            sf |= 1;
        }
        put32(&mut tree, s + 4, sf);
    }
    for (off, k) in [(0x5370u32, 0u32), (0x5374, 1)] {
        let sky = if n > 2 && chance(r, 2) { obj_addr(n - 1 - k) } else { 0 };
        put32(&mut tree, off, sky);
    }
    let mut cam = rand_bytes(r, 0x90);
    for i in 0..16 {
        putf(&mut cam, 0x10 + 4 * i, if i % 5 == 0 { 1.0 } else { urange(r, -0.3, 0.3) });
    }
    putf(&mut cam, 0x50, urange(r, -300.0, 300.0));
    putf(&mut cam, 0x54, urange(r, -300.0, 300.0));
    putf(&mut cam, 0x58, urange(r, -400.0, 100.0));
    putf(&mut cam, 0x5c, 1.0);
    let mut buckets = vec![0u8; BUCKETS_LEN as usize];
    let mut filled = 0u32;
    for i in 0..40 {
        if chance(r, 5) && n > 0 {
            put32(&mut buckets, 4 * i, obj_addr(r.below(n as u64) as u32));
            filled += 1;
        }
    }
    put32(&mut tree, 0x537c, if chance(r, 3) { r.below(4) as u32 } else { filled });
    let spr = rand_bytes(r, SPR_LEN as usize);
    let misc = rand_bytes(r, MISC_LEN as usize);
    Scene { tree, objs, aux, n, view, sel: r.below(2) as u8, cam, buckets, spr, misc }
}

impl W {
    /// Lays the scene into both machines.
    fn load(&mut self, s: &Scene) {
        self.put(TREE, &s.tree);
        self.put(OBJ0, &s.objs);
        self.put(AUX0, &s.aux);
        self.put(RECS, &vec![0u8; RECS_LEN as usize]);
        self.put(MISC, &s.misc);
        self.put(SPR, &s.spr);
        self.put(0x3b4070, &s.buckets);
        self.global32(G_VIEW, s.view);
        self.global8(G_SEL, s.sel);
        self.put(CAM_BASE + s.view * 0x90, &s.cam);
        self.global32(G_TREE, TREE);
        self.global8(G_MAP_OFF, s.n as u8);
    }
}

fn pick_obj(r: &mut Rng, s: &Scene) -> u32 {
    obj_addr(r.below(s.n.max(1) as u64) as u32)
}

fn cases(name: &str, n: u32, opt: Opt, mut each: impl FnMut(&mut W, &mut Rng, &Scene, u32)) -> Option<()> {
    let mut w = W::new(name)?;
    let mut r = Rng::new(0x25b ^ name.len() as u64 ^ (name.as_bytes()[2] as u64) << 8);
    for case in 0..n {
        let s = scene(&mut r, opt);
        let params = Params { scene_event: chance(&mut r, 4), seed: r.word() as u32 };
        w.reset(params);
        w.load(&s);
        each(&mut w, &mut r, &s, case);
    }
    Some(())
}

fn z_of(r: &mut Rng) -> f32 {
    match r.below(6) {
        0 => 0.0,
        1 => -0.0,
        _ => urange(r, -200.0, 1200.0),
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// the tests
// ---------------------------------------------------------------------------------------------------------------------

#[test]
fn orig_143100_143120_143140_143160_1431d0_15f1b0_15f1c8_148440_148478_match_the_game() {
    let Some(()) = cases("0x143100", 400, PASS, |w, _r, _s, case| {
        let which = case % 9;
        let addr = [0x143100u32, 0x143120, 0x143140, 0x143160, 0x1431d0, 0x15f1b0, 0x15f1c8, 0x148440, 0x148478][which as usize];
        w.call(addr, &[if which >= 7 { TREE as u64 } else { 0 }], &[]);
        let m = &mut w.ram;
        match which {
            0 => sky_on_global(m),
            1 => sky_off_global(m),
            2 => map_off(m),
            3 => map_on(m),
            4 => sort_by_group_on(m),
            5 => sky_on_wrapper_a(m),
            6 => sky_off_wrapper_a(m),
            7 => sky_on(m, TREE),
            _ => sky_off(m, TREE),
        }
        w.globals.push((G_MAP_OFF, 1));
        w.check(&format!("{addr:#x} case {case}"), &[]);
    }) else {
        return;
    };
}

#[test]
fn orig_147158_matches_the_game() {
    let mut ones = 0;
    let Some(()) = cases("0x147158", 3000, PASS, |w, r, s, case| {
        let obj = pick_obj(r, s);
        let z = z_of(r);
        let g = w.call(0x147158, &[TREE as u64, obj as u64], &[z]) as u32;
        let mut h = w.host();
        let p = see_through_test(&mut w.ram, &mut h, z, TREE, obj) as u32;
        assert_eq!(g, p, "case {case}: ev {} sw {} flags {:x} depth {} z {}", w.params.get().scene_event, w.ram.r8(TREE + 0x778), w.ram.r32(obj + 0xf8), w.ram.rf(TREE + 0x774), z);
        ones += p;
        w.check(&format!("0x147158 case {case}"), &h.log);
    }) else {
        return;
    };
    assert!(ones > 100, "{ones}");
}

#[test]
fn orig_147980_matches_the_game() {
    let Some(()) = cases("0x147980", 3000, PASS, |w, r, _s, case| {
        let n = r.below(30) as i32;
        let start = r.below(n as u64 + 3) as i32 - 1;
        let words: Vec<u32> = (0..n).map(|_| [1u32, 1, 2, 3, 0x7fff_ffff][r.below(5) as usize]).collect();
        let mut img = vec![0u8; 0x100];
        put32(&mut img, 0, start as u32);
        for (i, v) in words.iter().enumerate() {
            put32(&mut img, 0x10 + 4 * i as u32, *v);
        }
        w.put(MISC, &img);
        let g = w.call(0x147980, &[0, (MISC) as u64, (MISC + 0x10) as u64], &[]) as u32;
        let mut idx = start;
        let mut padded = words.clone();
        padded.resize(n.max(start + 1) as usize, 1);
        let p = if start >= n { w.ram.r32(MISC + 0x10 + 4 * start as u32) } else { next_group(&mut idx, &padded) };
        let _ = (g, p);
        if start < n {
            assert_eq!(g, p, "case {case}");
            w.ram.w32(MISC, idx as u32);
        } else {
            return;
        }
        w.check(&format!("0x147980 case {case}"), &[]);
    }) else {
        return;
    };
}

#[test]
fn orig_147b90_148920_148948_148980_149068_1490a0_match_the_game() {
    let Some(()) = cases("0x147b90", 3000, PASS, |w, r, s, case| {
        let obj = pick_obj(r, s);
        let which = case % 6;
        let mut h = w.host();
        match which {
            0 => {
                w.call(0x147b90, &[0, obj as u64], &[]);
                copy_colour(&mut w.ram, obj);
            }
            1 => {
                w.call(0x148920, &[TREE as u64, obj as u64], &[]);
                mark_visible(&mut w.ram, TREE, obj);
            }
            2 => {
                w.call(0x148948, &[TREE as u64], &[]);
                layers_reset(&mut w.ram, TREE);
            }
            3 => {
                w.call(0x148980, &[TREE as u64, obj as u64], &[]);
                layers_add(&mut w.ram, TREE, obj);
            }
            4 => {
                let (a, b) = (r.int(&[1]) as i64, [0xffffi64, 0xfffe, 0, r.word() as i64][r.below(4) as usize]);
                let (a, b) = if chance(r, 3) { (1, 0xffff) } else { (a, b) };
                w.call(0x149068, &[a as u64, b as u64], &[]);
                shadow_block_hook(&mut w.ram, &mut h, a, b);
            }
            _ => {
                w.call(0x1490a0, &[], &[]);
                shadow_block_init(&mut w.ram, &mut h);
            }
        }
        w.check(&format!("function {which} case {case}"), &h.log);
    }) else {
        return;
    };
}

#[test]
fn orig_1471d0_matches_the_game() {
    let Some(()) = cases("0x1471d0", 4000, PASS, |w, r, _s, case| {
        let n = 1 + r.below(60) as i32;
        let pool: Vec<f32> = (0..1 + r.below(12)).map(|_| urange(r, -100.0, 100.0)).collect();
        let mut img = vec![0u8; 0x400];
        for i in 0..n as u32 {
            let k = match r.below(4) {
                0 => i as f32,
                1 => -(i as f32),
                _ => pool[r.below(pool.len() as u64) as usize],
            };
            putf(&mut img, 4 * i, k);
            put32(&mut img, 0x200 + 4 * i, 0x1000 + i);
        }
        w.put(MISC, &img);
        let lo = if chance(r, 4) { r.below(n as u64) as i32 } else { 0 };
        w.call(0x1471d0, &[(MISC + 0x200) as u64, MISC as u64, lo as u64, (n - 1) as u64], &[]);
        quicksort(&mut w.ram, MISC + 0x200, MISC, lo, n - 1);
        w.check(&format!("0x1471d0 case {case}"), &[]);
    }) else {
        return;
    };
}

#[test]
fn orig_148550_1485a8_1487a8_match_the_game() {
    let Some(()) = cases("0x148550", 3000, PASS, |w, r, s, case| {
        let obj = pick_obj(r, s);
        let d = if chance(r, 5) { 0.0 } else { urange(r, -50.0, 1500.0) };
        let which = case % 3;
        let mut h = w.host();
        match which {
            0 => {
                w.call(0x148550, &[0, obj as u64], &[d]);
                lod_select(&mut w.ram, obj, d);
            }
            1 => {
                w.call(0x1485a8, &[TREE as u64, obj as u64], &[d]);
                three_part_fade(&mut w.ram, &mut h, d, obj);
            }
            _ => {
                let eye: [f32; 4] = std::array::from_fn(|k| if k == 3 { 1.0 } else { urange(r, -400.0, 400.0) });
                let mut b = vec![0u8; 16];
                for (k, f) in eye.iter().enumerate() {
                    putf(&mut b, 4 * k as u32, *f);
                }
                w.put(MISC, &b);
                let g = f32::from_bits(w.call(0x1487a8, &[TREE as u64, obj as u64, MISC as u64], &[]) as u32 ^ 0) ;
                let _ = g;
                let gf = w.ee.fpr[0];
                let p = sort_key(&mut w.ram, &mut h, TREE, obj, eye);
                assert_eq!(gf.to_bits(), p.to_bits(), "0x1487a8 case {case}: key");
            }
        }
        w.check(&format!("function {which} case {case}"), &h.log);
    }) else {
        return;
    };
}

#[test]
fn orig_147728_matches_the_game() {
    let Some(()) = cases("0x147728", 3000, PASS, |w, r, s, case| {
        let obj = pick_obj(r, s);
        w.call(0x147728, &[TREE as u64, obj as u64], &[]);
        billboard(&mut w.ram, obj);
        w.check(&format!("0x147728 case {case}"), &[]);
    }) else {
        return;
    };
}

#[test]
fn orig_147bb8_147c98_148388_1479c0_match_the_game() {
    // how often each path was reached: [see-through added, batch added, drawn at once, layered, light matrices, scene event]
    let mut hit = [0u32; 6];
    let Some(()) = cases("0x147bb8", 6000, PASS, |w, r, s, case| {
        let obj = pick_obj(r, s);
        let z = z_of(r);
        let mut h = w.host();
        let which = case % 4;
        let addr = [0x147bb8u32, 0x147c98, 0x148388, 0x1479c0][which as usize];
        if which == 1 && w.ram.r32(TREE + 0xc50 + 4 * w.ram.r32(TREE + 0x536c)) >= 16 {
            // the original never calls 0x147c98 with all batches used
            return;
        }
        let see_before = w.ram.r32(TREE + 0xc40);
        let batches_before: u32 = (0..16).map(|b| w.ram.r32(TREE + 0xc60 + 0x370 * b + 0x360)).sum();
        let layers_before = w.ram.r32(TREE + 0x537c);
        w.call(addr, &[TREE as u64, obj as u64], &[z]);
        match which {
            0 => see_through_add(&mut w.ram, z, TREE, obj),
            1 => batch_add(&mut w.ram, z, TREE, obj),
            2 => draw_or_layer(&mut w.ram, TREE, obj, &mut h),
            _ => draw_one(&mut w.ram, &mut h, z, TREE, obj),
        }
        hit[0] += (w.ram.r32(TREE + 0xc40) != see_before) as u32;
        let batches_after: u32 = (0..16).map(|b| w.ram.r32(TREE + 0xc60 + 0x370 * b + 0x360)).sum();
        hit[1] += (batches_after != batches_before) as u32;
        hit[2] += h.log.iter().filter(|v| v[0] == 0x11c7d8).count() as u32;
        hit[3] += (w.ram.r32(TREE + 0x537c) != layers_before) as u32;
        hit[4] += h.log.iter().filter(|v| v[0] == 0x134bd8).count() as u32;
        hit[5] += h.log.iter().filter(|v| v[0] == 0x224b18).count() as u32;
        w.check(&format!("{addr:#x} case {case}"), &h.log);
    }) else {
        return;
    };
    assert!(hit.iter().all(|&n| n > 100), "{hit:?}");
}

#[test]
fn orig_147e20_1484b0_1489d8_match_the_game() {
    let (mut plain, mut proj, mut sky, mut layered) = (0, 0, 0, 0);
    let Some(()) = cases("0x147e20", 6000, PASS, |w, r, s, case| {
        let obj = pick_obj(r, s);
        let mut h = w.host();
        let which = if std::env::var("ONLY0").is_ok() { 0 } else { case % 3 };
        let addr = [0x147e20u32, 0x1484b0, 0x1489d8][which as usize];
        let arg1 = if which == 2 { 0 } else { obj as u64 };
        w.call(addr, &[TREE as u64, arg1], &[]);
        match which {
            0 => draw_object(&mut w.ram, TREE, obj, &mut h),
            1 => sky_draw(&mut w.ram, TREE, obj, &mut h),
            _ => layers_draw(&mut w.ram, TREE, &mut h),
        }
        if which == 0 {
            let f = w.ram.r32(obj + 0xf8);
            if f & 0x1000 == 0 {
                plain += 1;
            } else if w.ram.r32(obj + 0x120) != 0 {
                proj += 1;
            }
        }
        sky += h.log.iter().filter(|v| v[0] == 0x11cb38).count();
        layered += h.log.iter().filter(|v| v[0] == 0x11c7d8).count();
        w.check(&format!("{addr:#x} case {case} (ptr {:#x} flags {:#x})", w.ram.r32(obj + 0x120), w.ram.r32(obj + 0xf8)), &h.log);
    }) else {
        return;
    };
    assert!(plain > 300 && proj > 300 && sky > 300 && layered > 1000, "{plain} {proj} {sky} {layered}");
}

#[test]
fn orig_147d98_matches_the_game() {
    let Some(()) = cases("0x147d98", 2000, PASS, |w, r, _s, case| {
        let x = r.word() as u16 as i16;
        let y = r.word() as u16 as i16;
        let mut img = vec![0u8; 0x40];
        put16(&mut img, 0x14, x as u16);
        put16(&mut img, 0x16, y as u16);
        w.put(MISC, &img);
        w.call(0x147d98, &[MISC as u64], &[]);
        let got = w.scissor.borrow().clone();
        assert_eq!(got, vec![billboard_scissor(x, y)], "case {case}");
    }) else {
        return;
    };
}

#[test]
fn orig_147300_matches_the_game() {
    let mut drawn = 0usize;
    let mut with_batches = 0usize;
    let mut groups = 0usize;
    let Some(()) = cases("0x147300", 3000, Opt { objects: (0, 30), flag_density: 4 }, |w, r, s, case| {
        // fade distances that equal an object's depth, or the depth plus 400, exactly (the comparisons in the fade are strict)
        for k in 0..s.n {
            let o = obj_addr(k);
            if w.ram.r32(o + 0xf8) & 0x1000 == 0 && chance(r, 3) {
                let view = std::array::from_fn(|i| std::array::from_fn(|j| w.ram.rf(TREE + 0x130 + 16 * i as u32 + 4 * j as u32)));
                let p = [w.ram.rf(o + 0xc0), w.ram.rf(o + 0xc4), w.ram.rf(o + 0xc8), 1.0];
                let z = kh2_model::draw::frustum::transform(&view, p)[2];
                let f = if chance(r, 2) { z } else { z + 400.0 };
                w.put(o + 0x108, &f.to_le_bytes());
            }
        }
        let mut h = w.host();
        w.call(0x147300, &[TREE as u64], &[]);
        object_pass(&mut w.ram, TREE, &mut h);
        drawn += h.log.iter().filter(|v| v[0] == 0x11c7d8 || v[0] == 0x11cb38).count();
        with_batches += h.log.iter().filter(|v| v[0] == 0x1da840).count();
        groups += (s.tree[0x779] != 0 && s.n > 1) as usize;
        w.check(&format!("0x147300 case {case}"), &h.log);
    }) else {
        return;
    };
    assert!(drawn > 3000, "{drawn}");
    assert!(with_batches > 300, "{with_batches}");
    assert!(groups > 300, "{groups}");
}

#[test]
fn orig_165990_matches_the_game() {
    const OCC: u32 = 0x018c_0000;
    const OUT: u32 = OCC + 0x1800;
    const COUNT: u32 = OCC + 0x1f80;
    const EYE: u32 = OCC + 0x1ef0;
    const CULL: u32 = OCC + 0x1f00;
    let mut made = 0u32;
    let Some(()) = cases("0x165990", 4000, PASS, |w, r, _s, case| {
        let mut b = rand_bytes(r, 0x2000);
        let groups = r.below(4) as u32;
        put32(&mut b, 0x4, OCC + 0x100);
        put32(&mut b, 0x8, OCC + 0x400);
        put32(&mut b, 0xc, OCC + 0x800);
        put32(&mut b, 0x10, OCC + 0x1000);
        put32(&mut b, 0x18, OCC + 0x1400);
        put32(&mut b, 0x20, groups);
        let tables = 6u32;
        for g in 0..groups {
            let first = r.below(tables as u64) as u16;
            let end = first + r.below(4) as u16;
            put16(&mut b, 0x100 + 0x20 * g + 0x1c, first);
            put16(&mut b, 0x100 + 0x20 * g + 0x1e, end.min(tables as u16 + 1));
        }
        for t in 0..tables + 1 {
            let lo = r.below(8) as u16;
            put16(&mut b, 0x400 + 0x14 * t + 0xc, lo);
            put16(&mut b, 0x400 + 0x14 * t + 0xe, lo + r.below(5) as u16);
            b[(0x400 + 0x14 * t + 0x10) as usize] = chance(r, 4) as u8 ^ 1;
        }
        for p in 0..16u32 {
            for k in 0..4 {
                put16(&mut b, 0x800 + 0x10 * p + 2 + 2 * k, r.below(20) as u16);
            }
            if chance(r, 2) {
                put16(&mut b, 0x800 + 0x10 * p + 8, 0xffff);
            }
            put16(&mut b, 0x800 + 0x10 * p + 0xa, r.below(10) as u16);
        }
        for i in 0..0x100u32 {
            putf(&mut b, 0x1000 + 4 * i, urange(r, -3.0, 3.0));
        }
        for i in 0..0x40u32 {
            putf(&mut b, 0x1400 + 4 * i, urange(r, -2.0, 2.0));
        }
        for i in 0..4 {
            putf(&mut b, 0x1ef0 + 4 * i, if i == 3 { 1.0 } else { urange(r, -4.0, 4.0) });
        }
        for i in 0..16 {
            putf(&mut b, 0x1f00 + 4 * i, urange(r, -1.5, 1.5));
        }
        put32(&mut b, 0x1f80, r.below(3) as u32);
        w.put(OCC, &b);
        w.call(0x165990, &[OCC as u64, EYE as u64, OUT as u64, COUNT as u64, CULL as u64], &[]);
        let before = w.ram.r32(COUNT);
        occluder_volumes(&mut w.ram, OCC, EYE, OUT, COUNT, CULL);
        made += w.ram.r32(COUNT) - before;
        w.check(&format!("0x165990 case {case}"), &[]);
    }) else {
        return;
    };
    assert!(made > 500, "{made}");
}
