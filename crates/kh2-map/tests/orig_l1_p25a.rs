//! Oracle tests of the map draw tree (WP-L1-P25a, `kh2_map::draw_tree`): kh2-ee runs the original from the user's own
//! executable and the port runs on a byte-for-byte copy of the same memory image; afterwards every region of the
//! image (the tree object, the map file's node and group tables, the draw objects, the output list, the scratch words)
//! and the log of the calls the port hands to its host are compared with the original's.
//!
//! The tree is built in memory by this file (a node table with up to eight children per node, a group table, draw
//! objects linked into node lists and registered in the slot table, clip matrices that put some boxes inside the
//! view, some across it and some outside), as `orig_l1_p22.rs` does for the collision unit. Callees that belong to
//! other units are replaced by hooks that log their arguments, and the port's [`Host`] logs the same:
//! * 0x15dff8 and 0x15e4f8 (the constructors of the see-through effect and the translucent batches) and the batch
//!   destructors a batch vtable names,
//! * 0x165990 (the occluder volume builder; the hook writes volumes the case chose), 0x16aa08 and 0x16b960 (the
//!   player and its position), 0x11c268 and 0x11e7f8 (the map model's pose and texture update), 0x11cb38 (the map
//!   model draw), 0x147300 (the object pass).
//!
//! Nothing else is stubbed: the clip tests, the matrix products, the library routine that projects a point (0x2fbc10) and
//! the box helpers run as the game has them. Skips without a disc (KH2_ISO or local/iso.txt); fails without one when
//! KH2_REQUIRE_DISC=1.

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::{Rng, STACK};
use kh2_map::draw_tree::*;
use kh2_map::geom::V4;
use kh2_model::draw::frustum::M4;
use std::cell::RefCell;
use std::rc::Rc;

const TREE: u32 = 0x0181_0000;
const TREE_LEN: u32 = 0x5400;
const FILE: u32 = 0x0183_0000;
const FILE_LEN: u32 = 0x4000;
const OBJ0: u32 = 0x0185_0000;
const OBJ_STRIDE: u32 = 0x200;
const OBJ_COUNT: u32 = 48;
const OBJ_LEN: u32 = OBJ_COUNT * OBJ_STRIDE;
const POINTS: u32 = 0x0187_0000;
const POINTS_LEN: u32 = 0x1000;
const LIST: u32 = 0x0188_0000;
const LIST_LEN: u32 = 0x1000;
const SCRATCH: u32 = 0x0189_0000;
const SCRATCH_LEN: u32 = 0x400;
const VIEWS: u32 = 0x3b21f0;
const VIEWS_LEN: u32 = 0xb00;
const NODES_OFF: u32 = 0x100;
const FAKE_VTABLE: u32 = SCRATCH + 0x100;
const DESTROY_HOOK: u32 = 0x0189_0f00;
const MODEL: u32 = 0x0189_0200;
const OCC_OBJ: u32 = 0x0189_0300;
const PORT_LIST: u32 = 0x01ee_0000;
const PLAYER_POS: u32 = SCRATCH + 0x80;

const G_TREE: u32 = 0x355930;
const G_OCC: u32 = 0x355940;
const G_MODEL: u32 = 0x355948;
const G_FLAGS: u32 = 0x35594c;
const G_VIEW_INDEX: u32 = 0x349e40;
const G_VIEW_BYTE: u32 = 0x349e44;
const G_STEP: u32 = 0x356f40;

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

fn r32(b: &[u8], o: u32) -> u32 {
    u32::from_le_bytes(b[o as usize..o as usize + 4].try_into().unwrap())
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

    /// A new case: the port's memory is empty again, the regions forgotten.
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

    fn call(&mut self, addr: u32, args: &[u64]) -> u64 {
        self.ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("the original faulted at {addr:#x}: {f:?}"))
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

    /// Compares every region and global of the game with the port's.
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

/// The calls both sides log.
type Log = Rc<RefCell<Vec<Vec<u32>>>>;

fn new_log() -> Log {
    Rc::new(RefCell::new(Vec::new()))
}

/// A hook that logs a0..a(n-1) and returns `ret`.
fn hook_log(ee: &mut Ee, addr: u32, n: usize, log: &Log, ret: u64) {
    let l = log.clone();
    ee.hooks.insert(addr, move |e: &mut Ee| {
        l.borrow_mut().push(std::iter::once(addr).chain((0..n).map(|k| e.gpr[reg::A0 + k] as u32)).collect());
        e.gpr[reg::V0] = ret as u128;
    });
}

/// What a case chose for the callees the port reaches through its host.
#[derive(Default)]
struct CaseHost {
    log: Vec<Vec<u32>>,
    volumes: Vec<u8>,
    volume_count: u32,
    player: bool,
}

impl Host for CaseHost {
    fn construct_see_through(&mut self, _ram: &mut Ram, at: u32) {
        self.log.push(vec![0x15dff8, at]);
    }
    fn construct_batch(&mut self, _ram: &mut Ram, at: u32) {
        self.log.push(vec![0x15e4f8, at]);
    }
    fn destroy_batch(&mut self, _ram: &mut Ram, at: u32) {
        self.log.push(vec![DESTROY_HOOK, at]);
    }
    fn destroy_member(&mut self, _ram: &mut Ram, at: u32, which: u32) {
        self.log.push(vec![which, at]);
    }
    fn occluder_volumes(&mut self, ram: &mut Ram, occluder: u32, eye: V4, planes: u32, count: u32, cull: u32) {
        let e: Vec<u32> = eye.iter().map(|f| f.to_bits()).collect();
        self.log.push([vec![0x165990, occluder], e, vec![planes, count, cull]].concat());
        ram.write(planes, &self.volumes);
        ram.w32(count, self.volume_count);
    }
    fn player_position(&mut self, ram: &mut Ram) -> Option<V4> {
        self.log.push(vec![0x16aa08]);
        if !self.player {
            return None;
        }
        self.log.push(vec![0x16b960]);
        Some(ram.v4(PLAYER_POS))
    }
    fn model_pose_reset(&mut self, _ram: &mut Ram, model: u32) {
        self.log.push(vec![0x11c268, model]);
    }
    fn model_texture_update(&mut self, _ram: &mut Ram, step: u32, model: u32) {
        self.log.push(vec![0x11e7f8, model, step]);
    }
    fn draw_map_model(&mut self, ram: &mut Ram, model: u32, m: &M4, a: u32, b: u32, list: u32) {
        let mat: Vec<u32> = m.iter().flatten().map(|f| f.to_bits()).collect();
        let mut entries = Vec::new();
        for k in 0..2049 {
            let v = ram.r16(list + 2 * k) as u32;
            entries.push(v);
            if v == 0xffff {
                break;
            }
        }
        self.log.push([vec![0x11cb38, model], mat, vec![a, b, 0], entries].concat());
    }
    fn object_pass(&mut self, _ram: &mut Ram, tree: u32) {
        self.log.push(vec![0x147300, tree]);
    }
}

/// Installs the logging hooks on the game and returns the shared log of what they saw.
fn install_hooks(w: &mut W, volumes: Vec<u8>, volume_count: u32, player: bool) -> Log {
    let log = new_log();
    hook_log(&mut w.ee, 0x15dff8, 1, &log, 0);
    hook_log(&mut w.ee, 0x15e4f8, 1, &log, 0);
    // the batch destructor named by the fake vtable: logs its own address as the host does
    {
        let l = log.clone();
        w.ee.hooks.insert(DESTROY_HOOK, move |e: &mut Ee| l.borrow_mut().push(vec![DESTROY_HOOK, e.gpr[reg::A0] as u32]));
    }
    hook_log(&mut w.ee, 0x32e530, 1, &log, 0);
    hook_log(&mut w.ee, 0x32e520, 1, &log, 0);
    hook_log(&mut w.ee, 0x11c268, 1, &log, 0);
    {
        // the step is a float argument (f12), the model handle is a0
        let l = log.clone();
        w.ee.hooks.insert(0x11e7f8, move |e: &mut Ee| l.borrow_mut().push(vec![0x11e7f8, e.gpr[reg::A0] as u32, e.fpr[12].to_bits()]));
    }
    hook_log(&mut w.ee, 0x147300, 1, &log, 0);
    {
        let l = log.clone();
        w.ee.hooks.insert(0x165990, move |e: &mut Ee| {
            let eye = e.read::<16>(e.gpr[reg::A0 + 1] as u32);
            let eyew: Vec<u32> = (0..4).map(|k| u32::from_le_bytes(eye[4 * k..4 * k + 4].try_into().unwrap())).collect();
            let (a0, planes, count, cull) = (e.gpr[reg::A0] as u32, e.gpr[reg::A0 + 2] as u32, e.gpr[reg::A0 + 3] as u32, e.gpr[reg::A0 + 4] as u32);
            l.borrow_mut().push([vec![0x165990, a0], eyew, vec![planes, count, cull]].concat());
            e.write(planes, &volumes);
            e.w32(count, volume_count);
        });
    }
    {
        let l = log.clone();
        w.ee.hooks.insert(0x16aa08, move |e: &mut Ee| {
            l.borrow_mut().push(vec![0x16aa08]);
            e.gpr[reg::V0] = if player { 0x1234_5000 } else { 0 };
        });
        let l = log.clone();
        w.ee.hooks.insert(0x16b960, move |e: &mut Ee| {
            l.borrow_mut().push(vec![0x16b960]);
            e.gpr[reg::V0] = PLAYER_POS as u128;
        });
    }
    {
        let l = log.clone();
        w.ee.hooks.insert(0x11cb38, move |e: &mut Ee| {
            let a1 = e.gpr[reg::A0 + 1] as u32;
            let mat: Vec<u32> = (0..16).map(|k| e.r32(a1 + 4 * k)).collect();
            let (a0, a2, a3, a4, a5) = (e.gpr[reg::A0] as u32, e.gpr[reg::A0 + 2] as u32, e.gpr[reg::A0 + 3] as u32, e.gpr[reg::A0 + 4] as u32, e.gpr[reg::A0 + 5] as u32);
            // the group list the call is given (entries up to the terminator), not its address (the game's is on its stack)
            let mut list = Vec::new();
            for k in 0..2049 {
                let v = e.r16(a4 + 2 * k) as u32;
                list.push(v);
                if v == 0xffff {
                    break;
                }
            }
            l.borrow_mut().push([vec![0x11cb38, a0], mat, vec![a2, a3, a5], list].concat());
        });
    }
    log
}

// ---------------------------------------------------------------------------------------------------------------------
// The scene generator
// ---------------------------------------------------------------------------------------------------------------------

/// What a case should look like.
#[derive(Clone, Copy)]
struct Opt {
    nodes: (u64, u64),
    groups: (u64, u64),
    objects: (u64, u64),
    /// Share of objects that are registered in the slot table (out of 8).
    registered: u64,
    /// Root box is the huge one of the build (otherwise the scene box).
    huge_root: bool,
    /// Flag bits an object may draw from.
    flag_bits: &'static [u32],
}

const WALK: Opt = Opt { nodes: (1, 13), groups: (0, 18), objects: (0, 20), registered: 8, huge_root: false, flag_bits: &[1, 2, 4, 8, 0x10, 0x20, 0x800, 0x1000] };

struct Scene {
    tree: Vec<u8>,
    file: Vec<u8>,
    objs: Vec<u8>,
    points: Vec<u8>,
    nodes: u32,
    groups: u32,
    objects: u32,
}

fn sub_box(r: &mut Rng, p: &[f32; 6]) -> [f32; 6] {
    let mut o = [0.0; 6];
    for k in 0..3 {
        let a = urange(r, p[k], p[k + 3]);
        let b = urange(r, p[k], p[k + 3]);
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        match r.below(6) {
            0 => {
                o[k] = p[k];
                o[k + 3] = p[k + 3];
            }
            _ => {
                o[k] = lo;
                o[k + 3] = hi;
            }
        }
    }
    o
}

const SCENE_BOX: [f32; 6] = [-120.0, -120.0, -20.0, 120.0, 120.0, 320.0];

/// A perspective-like clip matrix (the point times matrix form: x' = fx x, y' = fy y, z' = a z + b, w' = z).
fn perspective(r: &mut Rng) -> M4 {
    let (fx, fy) = (urange(r, 0.4, 2.0), urange(r, 0.4, 2.0));
    let (a, b) = (urange(r, 1.0, 4.0), urange(r, -800.0, -10.0));
    [[fx, 0.0, 0.0, 0.0], [0.0, fy, 0.0, 0.0], [0.0, 0.0, a, 1.0], [0.0, 0.0, b, 0.0]]
}

fn put_m(b: &mut [u8], o: u32, m: &M4) {
    for (i, row) in m.iter().enumerate() {
        wf(b, o + 16 * i as u32, row);
    }
}

fn object_addr(i: u32) -> u32 {
    OBJ0 + OBJ_STRIDE * i
}

fn scene(r: &mut Rng, o: Opt) -> Scene {
    let n = o.nodes.0 + r.below(o.nodes.1 - o.nodes.0 + 1);
    let g = o.groups.0 + r.below(o.groups.1 - o.groups.0 + 1);
    let k = o.objects.0 + r.below(o.objects.1 - o.objects.0 + 1);
    let (n, g, k) = (n as u32, g as u32, k as u32);
    let mut tree = vec![0u8; TREE_LEN as usize];
    for b in tree.iter_mut().take(0x5380) {
        *b = r.word() as u8;
    }
    let mut file = vec![0u8; FILE_LEN as usize];
    let mut objs = vec![0u8; OBJ_LEN as usize];
    for b in objs.iter_mut() {
        *b = r.word() as u8;
    }
    let mut points = vec![0u8; POINTS_LEN as usize];
    // file header and tables
    let groups_off = NODES_OFF + n * 0x30;
    let third_off = groups_off + g * 0x1c;
    w32(&mut file, 0x14, NODES_OFF);
    w32(&mut file, 0x18, n * 0x30);
    w32(&mut file, 0x1c, groups_off);
    w32(&mut file, 0x20, g * 0x1c);
    w32(&mut file, 0x24, third_off);
    w32(&mut file, 0x28, 0x34 * 3);
    // tree header
    w32(&mut tree, off::FILE, FILE);
    w32(&mut tree, off::NODES, FILE + NODES_OFF);
    w32(&mut tree, off::GROUPS, FILE + groups_off);
    w32(&mut tree, off::THIRD, FILE + third_off);
    w32(&mut tree, off::NODE_COUNT, n);
    w32(&mut tree, off::GROUP_COUNT, g);
    w32(&mut tree, off::THIRD_COUNT, 3);
    w32(&mut tree, off::LIST, LIST);
    tree[off::SEE_THROUGH as usize] = r.below(2) as u8;
    // clip matrices: the cull view and the clip-to-GS view
    put_m(&mut tree, off::CLIP_CULL, &perspective(r));
    put_m(&mut tree, off::CLIP_GS, &perspective(r));
    w32(&mut tree, off::VOLUME_COUNT, 0);
    // nodes: parents, children, boxes
    let mut children: Vec<Vec<u32>> = vec![Vec::new(); n as usize];
    let mut boxes = vec![SCENE_BOX; n as usize];
    boxes[0] = if o.huge_root { [-1e8, -1e8, -1e8, 1e8, 1e8, 1e8] } else { SCENE_BOX };
    for j in 1..n {
        loop {
            let p = r.below(j as u64) as usize;
            if children[p].len() < 8 {
                children[p].push(j);
                let pb = if p == 0 { SCENE_BOX } else { boxes[p] };
                boxes[j as usize] = sub_box(r, &pb);
                break;
            }
        }
    }
    let mut next_group = 0u32;
    for i in 0..n {
        let at = NODES_OFF + i * 0x30;
        for c in 0..8usize {
            let v = children[i as usize].get(c).map_or(-1i16, |&x| x as i16);
            w16(&mut file, at + 2 * c as u32, v as u16);
            if v == -1 {
                // the original stops at the first -1; later entries may hold anything
                if r.below(3) == 0 && c + 1 < 8 {
                    w16(&mut file, at + 2 * (c as u32 + 1), r.below(n as u64) as u16);
                }
                break;
            }
        }
        wf(&mut file, at + 0x10, &boxes[i as usize]);
        let take = if g == 0 { 0 } else { r.below((g - next_group.min(g)) as u64 + 1) as u32 };
        let first = next_group.min(g);
        let end = (first + take).min(g);
        w16(&mut file, at + 0x28, first as u16);
        w16(&mut file, at + 0x2a, end as u16);
        next_group = end;
        w32(&mut file, at + 0x2c, 0);
    }
    for gi in 0..g {
        let at = groups_off + gi * 0x1c;
        file[at as usize] = if r.below(4) == 0 { 0 } else { pick(r, &[1u8, 1, 1, 2, 0xff]) };
        file[at as usize + 1] = if r.below(10) == 0 { pick(r, &[0x80u8, 0xff]) } else { r.below(8) as u8 };
        let b = sub_box(r, &SCENE_BOX);
        wf(&mut file, at + 4, &b);
    }
    // draw objects, lists, slots
    let mut slot = 0u32;
    for oi in 0..k.min(OBJ_COUNT) {
        let a = object_addr(oi);
        let ob = (OBJ_STRIDE * oi) as usize;
        let node = r.below(n as u64) as u32;
        let mut f = 0u32;
        for &bit in o.flag_bits {
            if r.below(2) == 0 {
                f |= bit;
            }
        }
        w32(&mut objs, ob as u32 + 0xf8, f);
        objs[ob + 0xeb] = pick(r, &[0u8, 0x20, 0x80, 0xff, 0x7f, 0x01]);
        let bx = sub_box(r, &boxes[node as usize]);
        wf(&mut objs, ob as u32 + 0xd0, &bx);
        // eight corner points sometimes
        if r.below(3) == 0 && (oi as usize) * 128 + 128 <= points.len() {
            w32(&mut objs, ob as u32 + 0xfc, POINTS + oi * 128);
            let c = crate_corners(&bx, r);
            for (pi, p) in c.iter().enumerate() {
                wf(&mut points, oi * 128 + 16 * pi as u32, p);
            }
        } else {
            w32(&mut objs, ob as u32 + 0xfc, 0);
        }
        w16(&mut objs, ob as u32 + 0x118, node as u16);
        w16(&mut objs, ob as u32 + 0x11a, r.below(4) as u16);
        // list link: head insertion into the node's list
        let head = r32(&file, NODES_OFF + node * 0x30 + 0x2c);
        w32(&mut objs, ob as u32 + 0x110, head);
        w32(&mut file, NODES_OFF + node * 0x30 + 0x2c, a);
        // children chain stays empty unless a test links them
        w32(&mut objs, ob as u32 + 0xec, 0);
        w32(&mut objs, ob as u32 + 0xf0, 0);
        if r.below(8) < o.registered {
            w16(&mut objs, ob as u32 + 0x11c, slot as u16);
            w32(&mut tree, off::SLOTS + 8 * slot, a);
            w32(&mut tree, off::SLOTS + 8 * slot + 4, r.word() as u32 & pick(r, &[0xffff_ffffu32, 3, 0]));
            slot += 1;
        } else {
            w16(&mut objs, ob as u32 + 0x11c, 0x1ff - oi as u16);
        }
    }
    w32(&mut tree, off::SLOT_COUNT, slot);
    Scene { tree, file, objs, points, nodes: n, groups: g, objects: k.min(OBJ_COUNT) }
}

fn crate_corners(b: &[f32; 6], r: &mut Rng) -> [[f32; 4]; 8] {
    let mut c = [
        [b[0], b[1], b[2], 1.0],
        [b[0], b[4], b[2], 1.0],
        [b[3], b[4], b[2], 1.0],
        [b[3], b[1], b[2], 1.0],
        [b[0], b[1], b[5], 1.0],
        [b[0], b[4], b[5], 1.0],
        [b[3], b[4], b[5], 1.0],
        [b[3], b[1], b[5], 1.0],
    ];
    if r.below(3) == 0 {
        for p in c.iter_mut() {
            p[0] += urange(r, -5.0, 5.0);
        }
    }
    c
}

/// Puts a scene into both machines (regions: tree, file, objects, points, list, scratch) and sets the tree global.
fn install(w: &mut W, r: &mut Rng, s: &Scene) {
    w.put(TREE, &s.tree);
    w.put(FILE, &s.file);
    w.put(OBJ0, &s.objs);
    w.put(POINTS, &s.points);
    let mut list = vec![0u8; LIST_LEN as usize];
    for b in list.iter_mut() {
        *b = r.word() as u8;
    }
    w.put(LIST, &list);
    let mut scratch = vec![0u8; SCRATCH_LEN as usize];
    for b in scratch.iter_mut() {
        *b = r.word() as u8;
    }
    w.put(SCRATCH, &scratch);
    w.global(G_TREE, TREE);
}

// ---------------------------------------------------------------------------------------------------------------------
// c4: group show and hide, and the build
// ---------------------------------------------------------------------------------------------------------------------

const GROUPS: Opt = Opt { nodes: (1, 4), groups: (0, 30), objects: (0, 2), registered: 4, huge_root: false, flag_bits: &[1] };

/// orig 0x145b40 and 0x145b80: hide or show every group numbered b.
fn group_one(addr: u32, show: bool) {
    let Some(mut w) = W::new(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    for n in 0..800 {
        w.reset();
        let s = scene(&mut r, GROUPS);
        install(&mut w, &mut r, &s);
        let b = pick(&mut r, &[0u8, 1, 2, 3, 4, 7, 8, 0x80, 0xff]);
        let high = r.below(3) << 8;
        w.call(addr, &[TREE as u64, b as u64 | high]);
        let t = DrawTree(TREE);
        if show {
            t.group_show(&mut w.ram, b);
        } else {
            t.group_hide(&mut w.ram, b);
        }
        w.check(&format!("{addr:#x} case {n} group {b}"));
    }
}

#[test]
fn orig_145b40_matches_the_game() {
    group_one(0x145b40, false);
}

#[test]
fn orig_145b80_matches_the_game() {
    group_one(0x145b80, true);
}

/// orig 0x145aa8: all groups cleared, then the groups of the mask's set bits shown (64-bit mask, bits up to 63).
#[test]
fn orig_145aa8_matches_the_game() {
    let Some(mut w) = W::new("0x145aa8") else { return };
    let mut r = Rng::new(0x145aa8);
    for n in 0..800 {
        w.reset();
        let s = scene(&mut r, GROUPS);
        install(&mut w, &mut r, &s);
        let mask = match r.below(5) {
            0 => 0,
            1 => u64::MAX,
            2 => 1u64 << r.below(64),
            _ => r.word() & r.word(),
        };
        w.call(0x145aa8, &[TREE as u64, mask]);
        DrawTree(TREE).show_mask(&mut w.ram, mask);
        w.check(&format!("0x145aa8 case {n} mask {mask:#x}"));
    }
}

/// orig 0x142f78 and 0x143028 with the collision globals null: the drawn half of the opcode (the collision half is
/// proven in orig_l1_p22.rs and `switch_group` calls it).
#[test]
fn orig_142f78_matches_the_game_on_the_drawn_half() {
    let Some(mut w) = W::new("0x142f78") else { return };
    let mut r = Rng::new(0x142f78);
    for n in 0..600 {
        w.reset();
        let s = scene(&mut r, GROUPS);
        install(&mut w, &mut r, &s);
        for slot in [0x355934u32, 0x355938, 0x35593c, 0x355940, 0x355944] {
            w.global(slot, 0);
        }
        let b = r.below(8) as u8;
        let show = r.below(2) == 0;
        w.call(if show { 0x143028 } else { 0x142f78 }, &[b as u64]);
        let mut coll = kh2_map::mapcoll::MapCollision::default();
        switch_group(&mut w.ram, &mut coll, b, show);
        w.check(&format!("case {n} group {b} show {show}"));
    }
}

/// orig 0x145748 and 0x1459d0: the build and the release. The constructors, the destructors and the member
/// destructors are hooked and logged; the whole tree object and the file's node 0 box are compared.
#[test]
fn orig_145748_matches_the_game() {
    let Some(mut w) = W::new("0x145748") else { return };
    let mut r = Rng::new(0x145748);
    for n in 0..500 {
        w.reset();
        let s = scene(&mut r, GROUPS);
        install(&mut w, &mut r, &s);
        let log = install_hooks(&mut w, Vec::new(), 0, false);
        log.borrow_mut().clear();
        let mask = if r.below(3) == 0 { r.word() } else { r.word() & r.word() & r.word() };
        let flag = pick(&mut r, &[0u8, 1, 2, 0xff]);
        // the build overwrites the tree; start from a tree with junk
        let mut host = CaseHost::default();
        w.call(0x145748, &[TREE as u64, FILE as u64, flag as u64, mask]);
        DrawTree(TREE).build(&mut w.ram, FILE, flag, mask, &mut host);
        assert_eq!(*log.borrow(), host.log, "case {n}: calls");
        assert!(host.log.len() == 17, "17 constructor calls");
        w.check(&format!("0x145748 case {n} flag {flag} mask {mask:#x}"));
    }
}

#[test]
fn orig_1459d0_matches_the_game() {
    let Some(mut w) = W::new("0x1459d0") else { return };
    let mut r = Rng::new(0x1459d0);
    for n in 0..300 {
        w.reset();
        let mut s = scene(&mut r, GROUPS);
        // the batches' first words name a table whose first entry is the destructor hook
        w32(&mut s.tree, off::SEE_THROUGH_EFFECT, 0x1111_1111);
        for i in 0..16 {
            w32(&mut s.tree, off::BATCHES + off::BATCH_SIZE * i, FAKE_VTABLE);
        }
        install(&mut w, &mut r, &s);
        let mut sc = vec![0u8; SCRATCH_LEN as usize];
        w32(&mut sc, FAKE_VTABLE - SCRATCH, DESTROY_HOOK);
        w.put(SCRATCH, &sc);
        let log = install_hooks(&mut w, Vec::new(), 0, false);
        log.borrow_mut().clear();
        let mut host = CaseHost::default();
        w.call(0x1459d0, &[TREE as u64]);
        DrawTree(TREE).release(&mut w.ram, &mut host);
        assert_eq!(*log.borrow(), host.log, "case {n}: calls");
        assert_eq!(host.log.len(), 18);
        w.check(&format!("0x1459d0 case {n}"));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// c3: the per-frame matrices, the setters, the volumes and the lists
// ---------------------------------------------------------------------------------------------------------------------

/// Random view buffers (projection and camera blocks of the first views) and the view globals.
fn install_views(w: &mut W, r: &mut Rng) {
    let mut b = vec![0u8; VIEWS_LEN as usize];
    for k in 0..VIEWS_LEN / 4 {
        let f = if r.below(10) == 0 { pick(r, &[0.0f32, 1.0, -1.0, -0.0]) } else { urange(r, -50.0, 50.0) };
        w32(&mut b, 4 * k, f.to_bits());
    }
    w.put(VIEWS, &b);
    w.global(G_VIEW_INDEX, r.below(4) as u32);
    w.global8(G_VIEW_BYTE, r.below(2) as u8);
}

/// orig 0x145bc8: the per-frame matrices (projection and camera copies and the three clip matrices).
#[test]
fn orig_145bc8_matches_the_game() {
    let Some(mut w) = W::new("0x145bc8") else { return };
    let mut r = Rng::new(0x145bc8);
    for n in 0..800 {
        w.reset();
        let s = scene(&mut r, GROUPS);
        install(&mut w, &mut r, &s);
        install_views(&mut w, &mut r);
        w.call(0x145bc8, &[TREE as u64]);
        DrawTree(TREE).frame_matrices(&mut w.ram);
        w.check(&format!("0x145bc8 case {n}"));
    }
}

/// orig 0x145f88, 0x145f90, 0x145f98: the list, model and shadow list setters.
#[test]
fn orig_145f88_matches_the_game() {
    setter(0x145f88, |t, ram, v| t.set_list(ram, v));
}

#[test]
fn orig_145f90_matches_the_game() {
    setter(0x145f90, |t, ram, v| t.set_model(ram, v));
}

#[test]
fn orig_145f98_matches_the_game() {
    setter(0x145f98, |t, ram, v| t.set_shadow_list(ram, v));
}

fn setter(addr: u32, f: fn(DrawTree, &mut Ram, u32)) {
    let Some(mut w) = W::new(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    for n in 0..200 {
        w.reset();
        let s = scene(&mut r, GROUPS);
        install(&mut w, &mut r, &s);
        let v = r.word() as u32;
        w.call(addr, &[TREE as u64, v as u64]);
        f(DrawTree(TREE), &mut w.ram, v);
        w.check(&format!("{addr:#x} case {n}"));
    }
}

/// Random occluder volumes (count and planes) written into the tree: planes with unit normals and negative offsets so
/// the scene is sometimes inside them.
fn volumes(r: &mut Rng, count: u32) -> Vec<u8> {
    let mut b = vec![0u8; 0x50 * 16];
    for v in 0..count {
        for p in 0..5u32 {
            let axis = r.below(3) as usize;
            let mut n = [urange(r, -0.3, 0.3), urange(r, -0.3, 0.3), urange(r, -0.3, 0.3)];
            n[axis] = if r.below(2) == 0 { 1.0 } else { -1.0 };
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            let d = -urange(r, 20.0, 300.0);
            wf(&mut b, 0x50 * v + 16 * p, &[n[0] / len, n[1] / len, n[2] / len, d]);
        }
    }
    b
}

/// orig 0x145fa0: is the box in any volume?
#[test]
fn orig_145fa0_matches_the_game() {
    let Some(mut w) = W::new("0x145fa0") else { return };
    let mut r = Rng::new(0x145fa0);
    let mut yes = 0;
    for n in 0..1500 {
        w.reset();
        let mut s = scene(&mut r, GROUPS);
        let count = r.below(5) as u32;
        let v = volumes(&mut r, count);
        s.tree[off::VOLUMES as usize..off::VOLUMES as usize + v.len()].copy_from_slice(&v);
        w32(&mut s.tree, off::VOLUME_COUNT, if r.below(30) == 0 { 0xffff_fff0 } else { count });
        install(&mut w, &mut r, &s);
        let b = sub_box(&mut r, &SCENE_BOX);
        let at = SCRATCH + 0x300;
        let mut bb = [0u8; 24];
        wf(&mut bb, 0, &b);
        w.put(at, &bb);
        let g = w.call(0x145fa0, &[TREE as u64, at as u64]) as u32;
        let p = DrawTree(TREE).in_volume(&w.ram, at) as u32;
        assert_eq!(g, p, "case {n}");
        yes += p;
        w.check(&format!("0x145fa0 case {n}"));
    }
    assert!(yes > 100, "inside cases {yes}");
}

/// orig 0x146018: appends a value to the list and bumps the count.
#[test]
fn orig_146018_matches_the_game() {
    let Some(mut w) = W::new("0x146018") else { return };
    let mut r = Rng::new(0x146018);
    for n in 0..300 {
        w.reset();
        let s = scene(&mut r, GROUPS);
        install(&mut w, &mut r, &s);
        let c0 = r.below(100) as i32;
        w.ram.w32(SCRATCH, c0 as u32);
        w.ee.w32(SCRATCH, c0 as u32);
        let v = r.word() as u16;
        w.call(0x146018, &[TREE as u64, SCRATCH as u64, v as u64]);
        let mut count = c0;
        DrawTree(TREE).list_push(&mut w.ram, &mut count, v);
        w.ram.w32(SCRATCH, count as u32);
        w.check(&format!("0x146018 case {n}"));
    }
}

/// orig 0x146040: a group that crossed the cull test is tested against the clip-to-GS matrix.
#[test]
fn orig_146040_matches_the_game() {
    let Some(mut w) = W::new("0x146040") else { return };
    let mut r = Rng::new(0x146040);
    let mut seen = [0u32; 3];
    for n in 0..1500 {
        w.reset();
        let s = scene(&mut r, Opt { groups: (1, 12), ..WALK });
        install(&mut w, &mut r, &s);
        let c0 = r.below(10) as i32;
        w.ram.w32(SCRATCH, c0 as u32);
        w.ee.w32(SCRATCH, c0 as u32);
        let g = r.below(s.groups as u64) as u16;
        w.call(0x146040, &[TREE as u64, SCRATCH as u64, g as u64]);
        let mut count = c0;
        DrawTree(TREE).list_group_clipped(&mut w.ram, &mut count, g);
        seen[(count - c0) as usize] += 1;
        w.ram.w32(SCRATCH, count as u32);
        w.check(&format!("0x146040 case {n}"));
    }
    assert!(seen[0] > 50 && seen[1] > 50, "{seen:?}");
}

// ---------------------------------------------------------------------------------------------------------------------
// the cull walk
// ---------------------------------------------------------------------------------------------------------------------

/// orig 0x1463b0 (the walk), 0x1462a8 (the inside walk), 0x146108 (a node's objects visible), 0x146168 (a node's objects tested).
fn walk_test(addr: u32, f: fn(DrawTree, &mut Ram, u32, i32, &mut i32)) {
    let Some(mut w) = W::new(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64 ^ 0x77);
    let (mut listed, mut clipped) = (0, 0);
    for n in 0..1200 {
        w.reset();
        let huge = r.below(3) == 0;
        let s = scene(&mut r, Opt { huge_root: huge, ..WALK });
        let mut tree = s.tree.clone();
        // occluder volumes in half of the cases
        let count = if r.below(2) == 0 { 0 } else { r.below(4) as u32 };
        let v = volumes(&mut r, count);
        tree[off::VOLUMES as usize..off::VOLUMES as usize + v.len()].copy_from_slice(&v);
        w32(&mut tree, off::VOLUME_COUNT, count);
        let s = Scene { tree, ..s };
        install(&mut w, &mut r, &s);
        let node = if addr == 0x1463b0 || addr == 0x1462a8 { 0 } else { r.below(s.nodes as u64) as i32 };
        let c0 = r.below(5) as i32;
        w.ram.w32(SCRATCH, c0 as u32);
        w.ee.w32(SCRATCH, c0 as u32);
        let node_addr = FILE + NODES_OFF + node as u32 * 0x30;
        let arg1 = if addr == 0x146108 || addr == 0x146168 { node_addr as u64 } else { node as u64 };
        w.call(addr, &[TREE as u64, arg1, SCRATCH as u64]);
        let mut count = c0;
        f(DrawTree(TREE), &mut w.ram, node_addr, node, &mut count);
        w.ram.w32(SCRATCH, count as u32);
        w.check(&format!("{addr:#x} case {n}"));
        listed += (count - c0).max(0);
        let list = w.ram.read(LIST, 0x1000);
        clipped += list.chunks(2).take(count as usize).filter(|c| c[1] & 0x80 != 0).count();
    }
    if addr == 0x1463b0 {
        assert!(listed > 500 && clipped > 20, "listed {listed} clipped {clipped}");
    }
}

#[test]
fn orig_1463b0_matches_the_game() {
    walk_test(0x1463b0, |t, ram, _, node, count| t.walk(ram, node, count));
}

#[test]
fn orig_1462a8_matches_the_game() {
    walk_test(0x1462a8, |t, ram, _, node, count| t.walk_inside(ram, node, count));
}

#[test]
fn orig_146108_matches_the_game() {
    walk_test(0x146108, |t, ram, node_addr, _, _| t.node_objects_visible(ram, node_addr));
}

#[test]
fn orig_146168_matches_the_game() {
    walk_test(0x146168, |t, ram, node_addr, _, _| t.node_objects(ram, node_addr));
}

/// orig 0x1470f0 and 0x147128: an object marked visible (the clip mark variant sets bit 2 of the object's word +0x08).
fn mark_test(addr: u32, clip: bool) {
    let Some(mut w) = W::new(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    for n in 0..500 {
        w.reset();
        let s = scene(&mut r, WALK);
        if s.objects == 0 {
            continue;
        }
        install(&mut w, &mut r, &s);
        let o = object_addr(r.below(s.objects as u64) as u32);
        w.call(addr, &[TREE as u64, o as u64]);
        if clip {
            DrawTree(TREE).visible_clipped(&mut w.ram, o);
        } else {
            DrawTree(TREE).visible(&mut w.ram, o);
        }
        w.check(&format!("{addr:#x} case {n}"));
    }
}

#[test]
fn orig_1470f0_matches_the_game() {
    mark_test(0x1470f0, false);
}

#[test]
fn orig_147128_matches_the_game() {
    mark_test(0x147128, true);
}

// ---------------------------------------------------------------------------------------------------------------------
// object registration and move
// ---------------------------------------------------------------------------------------------------------------------

/// Chains child objects (+0xf0 first child, +0xec sibling) under some objects of the scene.
fn link_children(r: &mut Rng, s: &mut Scene) {
    let k = s.objects;
    if k < 3 {
        return;
    }
    // objects k-1 and k-2 become children chains under object 0 or 1
    let parent = r.below(2) as u32;
    let kids: Vec<u32> = (2..k).filter(|_| r.below(3) == 0).collect();
    let mut prev: Option<u32> = None;
    for &c in &kids {
        let ca = object_addr(c);
        match prev {
            None => w32(&mut s.objs, OBJ_STRIDE * parent + 0xf0, ca),
            Some(p) => w32(&mut s.objs, OBJ_STRIDE * p + 0xec, ca),
        }
        w32(&mut s.objs, OBJ_STRIDE * c + 0xec, 0);
        w32(&mut s.objs, OBJ_STRIDE * c + 0xf0, 0);
        prev = Some(c);
    }
}

/// orig 0x146c40, 0x146c80, 0x146cf8: the slot and list clears.
fn clear_test(addr: u32, f: fn(DrawTree, &mut Ram)) {
    let Some(mut w) = W::new(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    for n in 0..500 {
        w.reset();
        let s = scene(&mut r, WALK);
        install(&mut w, &mut r, &s);
        w.call(addr, &[TREE as u64]);
        f(DrawTree(TREE), &mut w.ram);
        w.check(&format!("{addr:#x} case {n}"));
    }
}

#[test]
fn orig_146c40_matches_the_game() {
    clear_test(0x146c40, |t, ram| t.reset_object_lists(ram));
}

#[test]
fn orig_146c80_matches_the_game() {
    clear_test(0x146c80, |t, ram| t.clear_object_lists(ram));
}

#[test]
fn orig_146cf8_matches_the_game() {
    clear_test(0x146cf8, |t, ram| t.clear_visible(ram));
}

/// orig 0x146d40: registers an object in the next slot and at the head of its node's list.
#[test]
fn orig_146d40_matches_the_game() {
    let Some(mut w) = W::new("0x146d40") else { return };
    let mut r = Rng::new(0x146d40);
    for n in 0..500 {
        w.reset();
        let s = scene(&mut r, WALK);
        if s.objects == 0 {
            continue;
        }
        install(&mut w, &mut r, &s);
        let o = object_addr(r.below(s.objects as u64) as u32);
        w.call(0x146d40, &[TREE as u64, o as u64]);
        DrawTree(TREE).register(&mut w.ram, o);
        w.check(&format!("0x146d40 case {n}"));
    }
}

/// orig 0x146d88: stores a sky object (the third argument is 64 bits wide).
#[test]
fn orig_146d88_matches_the_game() {
    let Some(mut w) = W::new("0x146d88") else { return };
    let mut r = Rng::new(0x146d88);
    for n in 0..300 {
        w.reset();
        let s = scene(&mut r, GROUPS);
        install(&mut w, &mut r, &s);
        let o = r.word() as u32;
        let which = pick(&mut r, &[0i64, 1, 2, -1, 0x1_0000_0000, 7]);
        w.call(0x146d88, &[TREE as u64, o as u64, which as u64]);
        DrawTree(TREE).set_sky(&mut w.ram, o, which);
        w.check(&format!("0x146d88 case {n}"));
    }
}

/// orig 0x146da0 and its wrapper 0x143240: adds an object whose flags have bit 4 or 8.
#[test]
fn orig_146da0_matches_the_game() {
    add_test(0x146da0);
}

#[test]
fn orig_143240_matches_the_game() {
    add_test(0x143240);
}

fn add_test(addr: u32) {
    let Some(mut w) = W::new(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    for n in 0..600 {
        w.reset();
        let s = scene(&mut r, WALK);
        if s.objects == 0 {
            continue;
        }
        install(&mut w, &mut r, &s);
        let o = object_addr(r.below(s.objects as u64) as u32);
        if addr == 0x143240 {
            w.call(addr, &[o as u64]);
            tree_add_object(&mut w.ram, o);
        } else {
            w.call(addr, &[TREE as u64, o as u64]);
            DrawTree(TREE).add_object(&mut w.ram, o);
        }
        w.check(&format!("{addr:#x} case {n}"));
    }
}

/// orig 0x146df0: the cull decision of an object (alpha byte, bit 0x10, bit 0x800).
#[test]
fn orig_146df0_matches_the_game() {
    let Some(mut w) = W::new("0x146df0") else { return };
    let mut r = Rng::new(0x146df0);
    let mut seen = [0u32; 2];
    for n in 0..600 {
        w.reset();
        let s = scene(&mut r, WALK);
        if s.objects == 0 {
            continue;
        }
        install(&mut w, &mut r, &s);
        let o = object_addr(r.below(s.objects as u64) as u32);
        let g = w.call(0x146df0, &[TREE as u64, o as u64]) as u32;
        let p = cull_state(&mut w.ram, o);
        assert_eq!(g, p, "case {n}");
        seen[p as usize] += 1;
        w.check(&format!("0x146df0 case {n}"));
    }
    assert!(seen[0] > 50 && seen[1] > 50, "{seen:?}");
}

/// orig 0x146f68: the deepest node holding the object's box; 0x147058 searches again from the object's own node;
/// 0x143260 searches from the root and stores the node and depth.
#[test]
fn orig_146f68_matches_the_game() {
    let Some(mut w) = W::new("0x146f68") else { return };
    let mut r = Rng::new(0x146f68);
    let (mut deep, mut zero) = (0, 0);
    for n in 0..1200 {
        w.reset();
        let s = scene(&mut r, Opt { nodes: (2, 13), objects: (1, 8), ..WALK });
        install(&mut w, &mut r, &s);
        let o = object_addr(r.below(s.objects as u64) as u32);
        let start = r.below(s.nodes as u64) as i32;
        let depth = r.below(4) as i32;
        let best0 = if r.below(2) == 0 { depth } else { r.below(5) as i32 };
        w.ee.w32(SCRATCH, best0 as u32);
        w.ram.w32(SCRATCH, best0 as u32);
        let g = w.call(0x146f68, &[TREE as u64, start as u64, o as u64, depth as u64, SCRATCH as u64]) as i32;
        let mut best = best0;
        let p = DrawTree(TREE).find_node(&w.ram, start, o, depth, &mut best);
        w.ram.w32(SCRATCH, best as u32);
        assert_eq!(g, p, "case {n}");
        if p > 0 {
            deep += 1;
        } else {
            zero += 1;
        }
        w.check(&format!("0x146f68 case {n}"));
    }
    assert!(deep > 200 && zero > 50, "{deep} {zero}");
}

#[test]
fn orig_147058_matches_the_game() {
    let Some(mut w) = W::new("0x147058") else { return };
    let mut r = Rng::new(0x147058);
    for n in 0..1200 {
        w.reset();
        let s = scene(&mut r, Opt { nodes: (2, 13), objects: (1, 8), ..WALK });
        install(&mut w, &mut r, &s);
        let o = object_addr(r.below(s.objects as u64) as u32);
        w.call(0x147058, &[TREE as u64, o as u64]);
        DrawTree(TREE).search_node(&mut w.ram, o);
        w.check(&format!("0x147058 case {n}"));
    }
}

#[test]
fn orig_143260_matches_the_game() {
    let Some(mut w) = W::new("0x143260") else { return };
    let mut r = Rng::new(0x143260);
    for n in 0..1200 {
        w.reset();
        let s = scene(&mut r, Opt { nodes: (2, 13), objects: (1, 8), ..WALK });
        install(&mut w, &mut r, &s);
        let o = object_addr(r.below(s.objects as u64) as u32);
        w.call(0x143260, &[o as u64]);
        tree_find_node(&mut w.ram, o);
        w.check(&format!("0x143260 case {n}"));
    }
}

/// orig 0x146e40 and its wrapper 0x1432b0: moves an object (and its children) to the node that holds its box.
#[test]
fn orig_146e40_matches_the_game() {
    move_test(0x146e40);
}

#[test]
fn orig_1432b0_matches_the_game() {
    move_test(0x1432b0);
}

fn move_test(addr: u32) {
    let Some(mut w) = W::new(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    for n in 0..1500 {
        w.reset();
        let mut s = scene(&mut r, Opt { nodes: (2, 13), objects: (1, 14), registered: 3, ..WALK });
        link_children(&mut r, &mut s);
        install(&mut w, &mut r, &s);
        let o = object_addr(r.below(s.objects.min(2) as u64) as u32);
        let top = r.below(2) as u8;
        if addr == 0x1432b0 {
            w.call(addr, &[o as u64]);
            tree_move_object(&mut w.ram, o);
        } else {
            w.call(addr, &[TREE as u64, o as u64, top as u64]);
            DrawTree(TREE).move_object(&mut w.ram, o, top);
        }
        w.check(&format!("{addr:#x} case {n}"));
    }
}

/// orig 0x1432d8: the sky wrapper.
#[test]
fn orig_1432d8_matches_the_game() {
    let Some(mut w) = W::new("0x1432d8") else { return };
    let mut r = Rng::new(0x1432d8);
    for n in 0..200 {
        w.reset();
        let s = scene(&mut r, GROUPS);
        install(&mut w, &mut r, &s);
        let o = r.word() as u32;
        let which = pick(&mut r, &[0i64, 1, 2, -1]);
        w.call(0x1432d8, &[o as u64, which as u64]);
        tree_set_sky(&mut w.ram, o, which);
        w.check(&format!("0x1432d8 case {n}"));
    }
}

/// orig 0x143300: the per-frame list clear task (the pose reset and the texture update are hooked and logged).
#[test]
fn orig_143300_matches_the_game() {
    let Some(mut w) = W::new("0x143300") else { return };
    let mut r = Rng::new(0x143300);
    for n in 0..500 {
        w.reset();
        let s = scene(&mut r, WALK);
        install(&mut w, &mut r, &s);
        let mut inst = vec![0u8; 0x100];
        for b in inst.iter_mut() {
            *b = r.word() as u8;
        }
        if r.below(2) == 0 {
            inst[0x8c] = 0;
        }
        w32(&mut inst, 0, 0x0cde_0000 + n);
        w.put(MODEL, &inst);
        w.global(G_MODEL, MODEL);
        w.global(G_STEP, r.word() as u32);
        let log = install_hooks(&mut w, Vec::new(), 0, false);
        log.borrow_mut().clear();
        let mut host = CaseHost::default();
        w.call(0x143300, &[]);
        clear_object_lists_task(&mut w.ram, &mut host);
        assert_eq!(*log.borrow(), host.log, "case {n}");
        w.check(&format!("0x143300 case {n}"));
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// the occluders, the player box and the pass
// ---------------------------------------------------------------------------------------------------------------------

/// The model instance block, the occluder global and the four flag bytes of one case; returns the volumes the occluder
/// hook will write and their count. `flags` are the bytes `0x35594c..f` (player box, map off, freeze, occluders).
fn install_pass_state(w: &mut W, r: &mut Rng, occluder: bool, flags: [u8; 4]) -> (Vec<u8>, u32) {
    let mut inst = vec![0u8; 0x100];
    for b in inst.iter_mut() {
        *b = r.word() as u8;
    }
    w32(&mut inst, 0, 0x0cde_0001);
    w.put(MODEL, &inst);
    w.global(G_MODEL, MODEL);
    w.global(G_OCC, if occluder { OCC_OBJ } else { 0 });
    for (k, f) in flags.iter().enumerate() {
        w.global8(G_FLAGS + k as u32, *f);
    }
    let count = r.below(4) as u32;
    (volumes(r, count), count)
}

/// orig 0x143f78: the occluder volumes (the builder 0x165990 is hooked).
#[test]
fn orig_143f78_matches_the_game() {
    let Some(mut w) = W::new("0x143f78") else { return };
    let mut r = Rng::new(0x143f78);
    let mut built = 0;
    for n in 0..600 {
        w.reset();
        let s = scene(&mut r, WALK);
        install(&mut w, &mut r, &s);
        install_views(&mut w, &mut r);
        let occluder = r.below(3) != 0;
        let flags = [0, 0, 0, r.below(3).min(1) as u8];
        let (v, count) = install_pass_state(&mut w, &mut r, occluder, flags);
        let log = install_hooks(&mut w, v.clone(), count, false);
        log.borrow_mut().clear();
        let mut host = CaseHost { volumes: v, volume_count: count, ..Default::default() };
        w.call(0x143f78, &[]);
        DrawTree(TREE).build_occluders(&mut w.ram, &mut host);
        assert_eq!(*log.borrow(), host.log, "case {n}");
        built += host.log.len();
        w.check(&format!("0x143f78 case {n}"));
    }
    assert!(built > 100, "built {built}");
}

/// orig 0x143ff8: the player's screen rectangle (0x16aa08 and 0x16b960 are hooked, the projection routine 0x2fbc10 runs).
#[test]
fn orig_143ff8_matches_the_game() {
    let Some(mut w) = W::new("0x143ff8") else { return };
    let mut r = Rng::new(0x143ff8);
    let mut with_player = 0;
    for n in 0..1500 {
        w.reset();
        let mut s = scene(&mut r, GROUPS);
        // a view matrix and a screen matrix with a usable w
        put_m(&mut s.tree, off::VIEW_MATRIX, &perspective(&mut r));
        let mut m = perspective(&mut r);
        for row in m.iter_mut() {
            for v in row.iter_mut() {
                *v *= 400.0;
            }
        }
        put_m(&mut s.tree, off::CLIP_SCREEN, &m);
        install(&mut w, &mut r, &s);
        let mut scratch = w.ram.read(SCRATCH, SCRATCH_LEN);
        wf(&mut scratch, PLAYER_POS - SCRATCH, &[urange(&mut r, -100.0, 100.0), urange(&mut r, 0.0, 200.0), urange(&mut r, -10.0, 400.0), 1.0]);
        w.put(SCRATCH, &scratch);
        let wanted = r.below(8) != 0;
        let (v, count) = install_pass_state(&mut w, &mut r, false, [wanted as u8, 0, 0, 0]);
        let player = r.below(5) != 0;
        let log = install_hooks(&mut w, v.clone(), count, player);
        log.borrow_mut().clear();
        let mut host = CaseHost { volumes: v, volume_count: count, player, ..Default::default() };
        w.call(0x143ff8, &[]);
        DrawTree(TREE).player_box(&mut w.ram, &mut host);
        assert_eq!(*log.borrow(), host.log, "case {n}");
        with_player += (wanted && player) as u32;
        w.check(&format!("0x143ff8 case {n}"));
    }
    assert!(with_player > 800, "with player {with_player}");
}

/// orig 0x144168: the map draw task end to end (every callee outside the package is hooked and logged; the order of
/// the logged calls is the order of the pass). The game's list lives on its stack and the port's at another address:
/// the tree word that names it is set to 0 on both sides before the comparison, and the group list is compared through
/// the entries the model draw is given.
#[test]
fn orig_144168_matches_the_game() {
    let Some(mut w) = W::new("0x144168") else { return };
    let mut r = Rng::new(0x144168);
    let (mut drawn, mut frozen, mut listed) = (0, 0, 0);
    for n in 0..800 {
        w.reset();
        let huge = r.below(3) == 0;
        let mut s = scene(&mut r, Opt { huge_root: huge, ..WALK });
        put_m(&mut s.tree, off::CLIP_SCREEN, &perspective(&mut r));
        install(&mut w, &mut r, &s);
        // the views: random buffers, then the active view's blocks made usable perspective-like matrices
        install_views(&mut w, &mut r);
        let idx = w.ram.r32(G_VIEW_INDEX);
        let mut region = w.ram.read(VIEWS, VIEWS_LEN);
        put_m(&mut region, 0x100 * idx, &perspective(&mut r));
        put_m(&mut region, 0x100 * idx + 0x80, &perspective(&mut r));
        put_m(&mut region, 0x100 * idx + 0xc0, &perspective(&mut r));
        put_m(&mut region, 0x400 + 0x90 * idx + 0x10, &perspective(&mut r));
        w.put(VIEWS, &region);
        let occluder = r.below(2) == 0;
        let flags = [r.below(2) as u8, (r.below(4) == 0) as u8, (r.below(8) == 0) as u8, r.below(2) as u8];
        let (v, count) = install_pass_state(&mut w, &mut r, occluder, flags);
        let player = r.below(2) == 0;
        let mut scratch = w.ram.read(SCRATCH, SCRATCH_LEN);
        wf(&mut scratch, PLAYER_POS - SCRATCH, &[urange(&mut r, -100.0, 100.0), urange(&mut r, 0.0, 200.0), urange(&mut r, -10.0, 400.0), 1.0]);
        w.put(SCRATCH, &scratch);
        frozen += (flags[2] == 1) as u32;
        let log = install_hooks(&mut w, v.clone(), count, player);
        log.borrow_mut().clear();
        let mut host = CaseHost { volumes: v, volume_count: count, player, ..Default::default() };
        w.call(0x144168, &[]);
        DrawTree(TREE).map_pass(&mut w.ram, PORT_LIST, &mut host);
        assert_eq!(*log.borrow(), host.log, "case {n}");
        for l in host.log.iter().filter(|l| l[0] == 0x11cb38) {
            drawn += 1;
            listed += l.len() - 21;
        }
        w.ee.w32(TREE + off::LIST, 0);
        w.ram.w32(TREE + off::LIST, 0);
        w.check(&format!("0x144168 case {n}"));
    }
    assert!(drawn > 100 && frozen > 20 && listed > 500, "drawn {drawn} frozen {frozen} listed {listed}");
}
