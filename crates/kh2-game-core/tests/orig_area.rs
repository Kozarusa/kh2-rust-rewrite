//! Oracle tests for the area control unit (`kh2_game_core::area::ctl`): the place records at 0x33e828.., the jump task and record,
//! the area allocator, the group records and the glue that starts and ends an area. kh2-ee runs the original from the user's
//! executable and the port runs on the same bytes; after every case the globals, the save block's place bytes, the arenas and the
//! ordered list of calls the unit makes to other units are compared (the call arguments by their low 32 bits).
//!
//! The harness is written for this unit instead of `factory gen` because the inputs are global records, linked objects and a
//! call-sequence glue the generator cannot lay out. It follows the same rules: the original runs from the user's executable, cases
//! come from a seeded generator (a failing case number reproduces), every compared case counts, and no byte of the windows may differ.
//!
//! Stand-ins (stubs), all documented here and nowhere else. Every function the unit calls that is not ported in it is a stand-in
//! that logs its address and arguments and returns a value drawn the same way on both sides (the list is read from the port's
//! source: every `host.call(0x...` and `host.vcall`); the port's host logs the same call. Specific returns: the task creators
//! 0x102778 and 0x102bc8 return fixed blocks, 0x1684c0 a fixed file entry, 0x1a71c0 a fixed party cell, 0x16fe70 a base address in
//! the allocator arena, 0x14a918 a fader block whose state word turns 4 at a chosen yield (0x102438), 0x102c30 and 0x102f18 mode
//! objects with a virtual table whose methods are stand-ins too. The story flag test 0x1802e0 answers by flag number and case.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_game_core::area::ctl::*;

const TASK: u32 = 0x0180_0000;
const TASK_NEW: u32 = TASK + 0x100;
const TASK_NEW2: u32 = TASK + 0x200;
const LS: u32 = 0x0181_0000;
const LS_ENTRY: u32 = LS;
const LS_DATA: u32 = LS + 0x100;
const HDR: u32 = 0x0182_0000;
const FAD: u32 = 0x0184_0000;
const PTY: u32 = 0x0185_0000;
const ALC: u32 = 0x0186_0000;
const MODE: u32 = 0x0187_0000;
const VT: u32 = 0x0188_0000;
const HV: u32 = 0x0189_0000;
const CASES: u64 = 400;

/// Windows of the original's memory the unit touches, and the arenas the harness lays out.
const REGIONS: &[(u32, u32)] = &[
    (0x33e690, 0x20),
    (0x33e6e0, 0x20),
    (0x33e820, 0x1c00),
    (0x356390, 0x20),
    (0x359360, 0x20),
    (0x35cd00, 0x50),
    (0x35d260, 0x10),
    (0x35e460, 0x20),
    (0x3b2180, 0x100),
    (0x522170, 0x20),
    (0x1c8af80, 0xe30),
    (0x1c8a960, 0x40),
    (0x35e3a0, 0x10),
    (0x356f40, 0x10),
    (0x37b300, 0x20),
    (0x35c990, 0x10),
    (0, 0x100),
    (TASK, 0x1000),
    (LS, 0x1000),
    (HDR, 0x10000),
    (FAD, 0x100),
    (PTY, 0x100),
    (ALC, 0x4000),
    (MODE, 0x100),
    (VT, 0x200),
];

type Ev = (u32, Vec<u32>);

#[derive(Clone)]
struct Mem {
    regs: Vec<(u32, Vec<u8>)>,
}

impl Mem {
    fn random(r: &mut Rng) -> Mem {
        let regs = REGIONS
            .iter()
            .map(|&(a, n)| {
                let mut v = vec![0u8; n as usize];
                for b in v.iter_mut() {
                    *b = r.word() as u8;
                }
                (a, v)
            })
            .collect();
        Mem { regs }
    }
    fn slot(&mut self, a: u32, n: usize) -> &mut [u8] {
        for (base, d) in self.regs.iter_mut() {
            if a >= *base && (a - *base) as usize + n <= d.len() {
                let o = (a - *base) as usize;
                return &mut d[o..o + n];
            }
        }
        panic!("the port touched {a:#x} (+{n}), outside every window")
    }
    fn r8(&mut self, a: u32) -> u8 {
        self.slot(a, 1)[0]
    }
    fn r16(&mut self, a: u32) -> u16 {
        u16::from_le_bytes(self.slot(a, 2).try_into().unwrap())
    }
    fn r32(&mut self, a: u32) -> u32 {
        u32::from_le_bytes(self.slot(a, 4).try_into().unwrap())
    }
    fn w8(&mut self, a: u32, v: u8) {
        self.slot(a, 1)[0] = v
    }
    fn w16(&mut self, a: u32, v: u16) {
        self.slot(a, 2).copy_from_slice(&v.to_le_bytes())
    }
    fn w32(&mut self, a: u32, v: u32) {
        self.slot(a, 4).copy_from_slice(&v.to_le_bytes())
    }
    fn arr6(&mut self, a: u32) -> Dest6 {
        self.slot(a, 6).try_into().unwrap()
    }
    fn put(&mut self, a: u32, d: &[u8]) {
        self.slot(a, d.len()).copy_from_slice(d)
    }
}

fn load_ctl(m: &mut Mem) -> AreaCtl {
    AreaCtl {
        active: m.r8(0x33e828),
        cur: m.arr6(0x33e830),
        pending: m.arr6(0x33e838),
        last_exit: m.arr6(0x33e840),
        prev: m.arr6(0x33e848),
        allocator: m.r32(0x33e850),
        task: m.r32(0x33e854),
        task_mgr: m.r32(0x33e6e4),
        gate: m.r8(0x3b2180),
        saved: m.arr6(0x3b2188),
        poll_stop: m.r8(0x356398),
        poll_paused: m.r8(0x356399),
        layout_alloc: m.r32(0x359368),
        type4_memory: m.r32(0x1c8bd90),
        party_ready: m.r8(0x1c8bd98),
        party_word: m.r32(0x1c8bda0),
        localset_file: m.r32(0x1c8af88),
        group_slots: m.r32(0x35e468),
        areainfo: m.r32(0x1c8a970),
        areainfo_has: m.r8(0x1c8a974),
        areainfo_hold: m.r8(0x1c8a975),
        areainfo_file: m.r32(0x1c8a978),
    }
}

fn store_ctl(c: &AreaCtl, m: &mut Mem) {
    m.w8(0x33e828, c.active);
    m.put(0x33e830, &c.cur);
    m.put(0x33e838, &c.pending);
    m.put(0x33e840, &c.last_exit);
    m.put(0x33e848, &c.prev);
    m.w32(0x33e850, c.allocator);
    m.w32(0x33e854, c.task);
    m.w32(0x33e6e4, c.task_mgr);
    m.w8(0x3b2180, c.gate);
    m.put(0x3b2188, &c.saved);
    m.w8(0x356398, c.poll_stop);
    m.w8(0x356399, c.poll_paused);
    m.w32(0x359368, c.layout_alloc);
    m.w32(0x1c8bd90, c.type4_memory);
    m.w8(0x1c8bd98, c.party_ready);
    m.w32(0x1c8bda0, c.party_word);
    m.w32(0x1c8af88, c.localset_file);
    m.w32(0x35e468, c.group_slots);
    m.w32(0x1c8a970, c.areainfo);
    m.w8(0x1c8a974, c.areainfo_has);
    m.w8(0x1c8a975, c.areainfo_hold);
    m.w32(0x1c8a978, c.areainfo_file);
}

const SAVE: u32 = 0x33e860;
const SAVE_LEN: usize = 0x1700;

fn load_save(m: &mut Mem) -> Vec<u8> {
    m.slot(SAVE, SAVE_LEN).to_vec()
}

fn store_save(m: &mut Mem, s: &[u8]) {
    m.put(SAVE, s);
}

fn mix(a: u64, b: u64, c: u64) -> u64 {
    let mut x = a ^ b.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ c.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 29)
}

/// What the stand-ins return; the same object drives both sides.
struct Pol {
    seed: u64,
    counts: HashMap<u32, u32>,
    alloc_base: u32,
    yield_at: u32,
    yields: u32,
}

impl Pol {
    fn new(seed: u64) -> Pol {
        Pol { seed, counts: HashMap::new(), alloc_base: ALC + 0x1000, yield_at: 1, yields: 0 }
    }
    fn ret(&mut self, addr: u32, args: &[u64]) -> u64 {
        let k = {
            let c = self.counts.entry(addr).or_insert(0);
            let k = *c;
            *c += 1;
            k
        };
        match addr {
            0x102778 => TASK_NEW as u64,
            0x102bc8 => TASK_NEW2 as u64,
            0x1684c0 => LS_ENTRY as u64,
            0x1a71c0 => PTY as u64,
            0x16fe70 => self.alloc_base as u64,
            0x14a918 => FAD as u64,
            0x198220 => HDR as u64 + 0xe000,
            0x16e208 => {
                if mix(self.seed, addr as u64, k as u64) & 3 == 0 {
                    0
                } else {
                    HDR as u64 + 0xe900
                }
            }
            0x16b858 => ((mix(self.seed, addr as u64, k as u64) % 5) as i64 - 1) as u64,
            0x16ece0 => 1990 + mix(self.seed, addr as u64, k as u64) % 20,
            0x195358 => (mix(self.seed, addr as u64, k as u64) % 4 == 0) as u64,
            0x1d88a0 => mix(self.seed, addr as u64, k as u64) % 2,
            0x102c30 => MODE as u64,
            0x102f18 => MODE as u64 + 0x40,
            0x1802e0 => (mix(self.seed, args[0] as u32 as u64, 1) >> 7) & 1,
            0x1039d0 | 0x2f4c40 => ((mix(self.seed, addr as u64, k as u64) >> 9) % 3 == 0) as u64,
            _ => mix(self.seed, addr as u64, k as u64) % 3,
        }
    }
}

struct Port {
    mem: Mem,
    pol: Pol,
    log: Vec<Ev>,
}

impl Port {
    fn new(mem: Mem, seed: u64) -> Port {
        Port { mem, pol: Pol::new(seed), log: Vec::new() }
    }
}

impl AreaHost for Port {
    fn call(&mut self, callee: u32, args: &[u64]) -> u64 {
        self.log.push((callee, args.iter().map(|a| *a as u32).collect()));
        if callee == 0x102438 {
            self.pol.yields += 1;
            if self.pol.yields == self.pol.yield_at {
                self.mem.w32(FAD + 0xfc, 4);
            }
        }
        self.pol.ret(callee, args)
    }
    fn vcall(&mut self, obj: u32, slot: u32, args: &[u64]) -> u64 {
        let mut a = vec![obj];
        a.extend(args.iter().map(|x| *x as u32));
        self.log.push((0x8000_0000 | slot, a));
        let v: Vec<u64> = std::iter::once(obj as u64).chain(args.iter().copied()).collect();
        self.pol.ret(0x8000_0000 | slot, &v)
    }
    fn read8(&mut self, a: u32) -> u8 {
        self.mem.r8(a)
    }
    fn read16(&mut self, a: u32) -> u16 {
        self.mem.r16(a)
    }
    fn read32(&mut self, a: u32) -> u32 {
        self.mem.r32(a)
    }
    fn write8(&mut self, a: u32, v: u8) {
        self.mem.w8(a, v)
    }
    fn write32(&mut self, a: u32, v: u32) {
        self.mem.w32(a, v)
    }
}

struct Shared {
    log: Vec<Ev>,
    pol: Pol,
}

struct Rig {
    ee: Ee,
    sh: Rc<RefCell<Shared>>,
    cases: u64,
    calls: u64,
    yield_at: u32,
    alloc_base: u32,
    /// How often each branch of interest ran, by name (stand-in calls by address, and `hit`).
    tally: HashMap<String, u64>,
}

/// The addresses the port calls through its host, read from its source.
fn externs() -> Vec<u32> {
    let src = include_str!("../src/area.rs");
    let mut out = Vec::new();
    for pat in ["host.call(0x", ".call(0x"] {
        for part in src.split(pat).skip(1) {
            let hex: String = part.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
            if let Ok(v) = u32::from_str_radix(&hex, 16) {
                out.push(v);
            }
        }
    }
    // Calls made through a loop over an address list.
    for part in src.split("for c in [").skip(1) {
        let list = part.split(']').next().unwrap_or("");
        for t in list.split(',') {
            if let Some(h) = t.trim().strip_prefix("0x")
                && let Ok(v) = u32::from_str_radix(h, 16)
            {
                out.push(v);
            }
        }
    }
    // The area start's chained list of getters.
    out.sort();
    out.dedup();
    out
}

/// Addresses that are ported in the unit and must run natively.
const NATIVE: &[u32] = &[
    0x1047a8, 0x104b60, 0x104b90, 0x104bd0, 0x104c68, 0x104ce0, 0x104d58, 0x104e18, 0x104f88, 0x104f98, 0x104fa8, 0x104fe0, 0x105010, 0x105070, 0x105098, 0x105188, 0x1051e0,
    0x1051f0, 0x105250, 0x1052a0, 0x1052e0, 0x105420, 0x105d60, 0x105de0, 0x105df8, 0x105e28, 0x105ea8, 0x105f18, 0x105f40, 0x105f68, 0x105f98, 0x105fa8, 0x105fb8, 0x105fd8,
    0x149b08, 0x149b40, 0x149ba0, 0x149bb8, 0x149cb8, 0x149ce0, 0x149d40, 0x149d78, 0x149db8, 0x149df0, 0x149eb0, 0x149f38, 0x149f70, 0x149fa8, 0x149fb8, 0x149fc8, 0x149fd8,
    0x14a000, 0x14a038, 0x14a060, 0x14a1e0, 0x1688a0, 0x168928, 0x168960, 0x168ae0, 0x168b10, 0x168ba8, 0x168ca0, 0x168ce8, 0x1982c8, 0x198300, 0x198328, 0x198340, 0x1985f8,
    0x198660, 0x198fa0, 0x199698, 0x19a308, 0x19a3b8, 0x19a3d8, 0x19d8c0, 0x19d8d0, 0x19d8f0, 0x19d9c8, 0x19d9d8, 0x198080, 0x105d48, 0x102498,
];

impl Rig {
    fn new(what: &str) -> Option<Rig> {
        let elf = kh2_factory::exe_or_skip(what)?;
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let sh = Rc::new(RefCell::new(Shared { log: Vec::new(), pol: Pol::new(0) }));
        for addr in externs() {
            assert!(!NATIVE.contains(&addr), "{addr:#x} is ported in the unit but also called through the host");
            let s = sh.clone();
            ee.hooks.insert(addr, move |ee: &mut Ee| {
                let mut a: Vec<u32> = (4..10).map(|i| ee.gpr[i] as u32).collect();
                if addr == 0x198220 {
                    let p = a[2];
                    a = vec![a[0], a[1], ee.r32(p), ee.r32(p + 4), ee.r32(p + 8), ee.r32(p + 12)];
                }
                if addr == 0x1abf40 {
                    a = vec![a[0], ee.fpr[12].to_bits(), ee.fpr[13].to_bits(), ee.fpr[14].to_bits()];
                }
                if addr == 0x199058 {
                    let p = a[1];
                    a = vec![a[0], ee.r8(p) as u32, ee.r8(p + 1) as u32, ee.r8(p + 2) as u32, a[2], a[3], a[4]];
                }
                let full: Vec<u64> = (4..10).map(|i| ee.gpr[i] as u64).collect();
                let mut s = s.borrow_mut();
                s.log.push((addr, a));
                if addr == 0x102438 {
                    s.pol.yields += 1;
                    if s.pol.yields == s.pol.yield_at {
                        ee.w32(FAD + 0xfc, 4);
                    }
                }
                let v = s.pol.ret(addr, &full);
                ee.gpr[2] = v as i64 as u128;
            });
        }
        for slot in 0..4u32 {
            let s = sh.clone();
            let addr = HV + slot * 0x10;
            ee.hooks.insert(addr, move |ee: &mut Ee| {
                let a: Vec<u32> = (4..8).map(|i| ee.gpr[i] as u32).collect();
                let full: Vec<u64> = (4..8).map(|i| ee.gpr[i] as u64).collect();
                let mut s = s.borrow_mut();
                s.log.push((0x8000_0000 | slot, a));
                let v = s.pol.ret(0x8000_0000 | slot, &full);
                ee.gpr[2] = v as i64 as u128;
            });
        }
        for slot in 0..4u32 {
            ee.w32(VT + slot * 4, HV + slot * 0x10);
        }
        Some(Rig { ee, sh, cases: 0, calls: 0, yield_at: 1, alloc_base: ALC + 0x1000, tally: HashMap::new() })
    }

    /// Lays the memory image and the stand-in policy of one case.
    fn port(&self, mem: Mem, seed: u64) -> Port {
        let mut p = Port::new(mem, seed);
        p.pol.yield_at = self.yield_at;
        p.pol.alloc_base = self.alloc_base;
        p
    }

    fn begin(&mut self, mem: &Mem, seed: u64, yield_at: u32, alloc_base: u32) {
        self.yield_at = yield_at;
        self.alloc_base = alloc_base;
        let mut s = self.sh.borrow_mut();
        s.log.clear();
        s.pol = Pol::new(seed);
        s.pol.yield_at = yield_at;
        s.pol.alloc_base = alloc_base;
        drop(s);
        for (a, d) in &mem.regs {
            self.ee.write(*a, d);
        }
    }

    fn call(&mut self, addr: u32, args: &[u64]) -> u64 {
        self.ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
    }

    /// Compares the port's memory and call log with the original's.
    fn finish(&mut self, port: &Port, case: u64, what: &str) {
        let g = self.sh.borrow().log.clone();
        let p = &port.log;
        assert_eq!(g.len(), p.len(), "{what} case {case}: number of calls
 game {g:x?}
 port {p:x?}");
        for (k, (ge, pe)) in g.iter().zip(p.iter()).enumerate() {
            assert!(ge.0 == pe.0 && ge.1.len() >= pe.1.len() && ge.1[..pe.1.len()] == pe.1[..], "{what} case {case}: call {k}
 game {:x?}
 port {:x?}
 all game {g:x?}
 all port {p:x?}", ge, pe);
        }
        for (a, d) in &port.mem.regs {
            for (i, &b) in d.iter().enumerate() {
                let gb = self.ee.r8(a + i as u32);
                if gb != b {
                    panic!("{what} case {case}: byte {:#x}: game {gb:02x} port {b:02x}
 calls {g:x?}", a + i as u32);
                }
            }
        }
        for (c, a) in p {
            *self.tally.entry(format!("{c:x}")).or_insert(0) += 1;
            if *c == 0x1684c0 {
                *self.tally.entry(format!("1684c0 world {:x}", a[2])).or_insert(0) += 1;
            }
        }
        self.cases += 1;
        self.calls += p.len() as u64;
    }

    fn hit(&mut self, k: &str) {
        *self.tally.entry(k.to_string()).or_insert(0) += 1;
    }

    /// Every named branch must have run at least `min` times.
    fn require(&self, what: &str, names: &[&str], min: u64) {
        for n in names {
            let c = self.tally.get(*n).copied().unwrap_or(0);
            assert!(c >= min, "{what}: branch {n} ran {c} times, fewer than {min}; the generator does not reach it");
        }
    }
}

fn place(r: &mut Rng, m: &mut Mem, a: u32) {
    // A place record with a world below 19, an area below 64 and free program bytes.
    let w = r.below(19) as u8;
    let ar = r.below(64) as u8;
    m.put(a, &[w, ar, r.word() as u8, r.below(8) as u8, r.below(8) as u8, r.below(0x30) as u8]);
}

/// A destination: each byte 0xff about half of the time.
fn dest(r: &mut Rng, m: &mut Mem, a: u32) {
    let mut d = [0u8; 6];
    d[0] = if r.below(3) == 0 { 0xff } else { r.below(19) as u8 };
    d[1] = if r.below(3) == 0 { 0xff } else { r.below(64) as u8 };
    d[2] = if r.below(3) == 0 { 0xff } else { r.word() as u8 };
    for b in d.iter_mut().skip(3) {
        *b = if r.below(2) == 0 { 0xff } else { r.below(0x30) as u8 };
    }
    m.put(a, &d);
}

fn prep(r: &mut Rng) -> (Mem, u64) {
    let mut m = Mem::random(r);
    place(r, &mut m, 0x33e830);
    place(r, &mut m, 0x33e848);
    let seed = r.word();
    // The mode objects and the allocator object have the virtual table.
    m.w32(MODE, VT);
    m.w32(MODE + 0x40, VT);
    for slot in 0..4u32 {
        m.w32(VT + slot * 4, HV + slot * 0x10);
    }
    (m, seed)
}

fn exe_table(ee: &mut Ee, base: u32, n: u32) -> Vec<u32> {
    (0..n).map(|i| ee.r32(base + 4 * i)).collect()
}

fn exe_cstr(ee: &mut Ee, p: u32) -> String {
    let mut s = String::new();
    for i in 0..64 {
        let b = ee.r8(p + i);
        if b == 0 {
            break;
        }
        s.push(b as char);
    }
    s
}

fn map_table(ee: &mut Ee) -> [(u32, u32); 15] {
    let mut t = [(0u32, 0u32); 15];
    for (i, e) in t.iter_mut().enumerate() {
        *e = (ee.r32(0x3790a8 + 8 * i as u32), ee.r32(0x3790a8 + 8 * i as u32 + 4));
    }
    t
}

fn prefixes(ee: &mut Ee) -> Vec<String> {
    (0..19).map(|i| {
        let p = ee.r32(0x349dc0 + 4 * i);
        exe_cstr(ee, p)
    }).collect()
}

fn sxl(v: u32) -> u64 {
    sx(v)
}

// ---------------------------------------------------------------------------------------------------------------------------------

/// Records, names and the small leaves.
fn records(rig: &mut Rig, r: &mut Rng, case: u64) {
    let (mut m, seed) = prep(r);
    let which = r.below(18);
    let mut ctlx = None;
    let (a0, a1, a2, a3) = (TASK + 0x400, TASK + 0x420, TASK + 0x440, TASK + 0x460);
    dest(r, &mut m, a1);
    place(r, &mut m, a0);
    place(r, &mut m, a2);
    m.put(a3, &[b't', b't', 0]);
    // The 32-bit arguments of the reset family.
    let rst_a = if r.below(3) == 0 { 1 } else { r.below(3) as i32 - 1 };
    let rst_b: i64 = if r.below(3) == 0 { 0xffff } else { [0, 1, 0xfffe, 0x1_0000, -1][r.below(5) as usize] };
    let mut jr = JumpRec::default();
    let jbytes: Vec<u8> = (0..0x20).map(|_| r.word() as u8).collect();
    jr.0.copy_from_slice(&jbytes);
    if r.below(3) == 0 {
        jr.0[0..4].copy_from_slice(&0u32.to_le_bytes());
    }
    m.put(TASK + 0x500, &jr.0);
    let world = r.below(19) as i32;
    let idx = r.below(4) as i32;
    let sbuf_arg = if r.below(2) == 0 { 0 } else { TASK + 0x600 };
    rig.begin(&m, seed, 1, ALC + 0x1000);
    let mut port = rig.port(m.clone(), seed);
    let mut ctl = load_ctl(&mut port.mem);
    let mut save = load_save(&mut port.mem);
    let what;
    match which {
        0 => {
            what = "0x104b90";
            rig.call(0x104b90, &[a0 as u64, a1 as u64]);
            let mut d = port.mem.slot(a0, 3).to_vec();
            let s = port.mem.slot(a1, 3).to_vec();
            merge_programs(&mut d, &s);
            port.mem.put(a0, &d);
        }
        1 => {
            what = "0x104bd0";
            rig.call(0x104bd0, &[0x33e830, a1 as u64]);
            let d = port.mem.arr6(a1);
            apply_destination(&mut ctl.cur, &d, &save);
        }
        2 => {
            what = "0x105188";
            rig.call(0x105188, &[a1 as u64]);
            let d = port.mem.arr6(a1);
            ctl.commit(&d, &mut save);
        }
        3 => {
            what = "0x105070";
            if r.below(3) == 0 {
                let x = port.mem.arr6(a0);
                rig.ee.write(a2, &x);
                port.mem.put(a2, &x);
            }
            let g = rig.call(0x105070, &[a0 as u64, a2 as u64]) as u8 != 0;
            let (x, y) = (port.mem.arr6(a0), port.mem.arr6(a2));
            assert_eq!(g, same_area(&x, &y), "case {case}: same_area");
            rig.hit(if g { "same_area true" } else { "same_area false" });
        }
        4 => {
            what = "0x1052e0";
            rig.call(0x1052e0, &[sxl(rst_a as u32), rst_b as u64]);
            ctl.reset(rst_a, rst_b);
            rig.hit(if rst_a == 1 && rst_b == 0xffff { "reset fires" } else { "reset skips" });
        }
        5 => {
            what = "0x105420";
            rig.call(0x105420, &[]);
            ctl.reset_all();
        }
        6 => {
            what = "0x104f88";
            rig.call(0x104f88, &[]);
            ctl.clear_task();
        }
        7 => {
            what = "0x104f98";
            let g = rig.call(0x104f98, &[]) as u32;
            assert_eq!(g, ctl.get_allocator(), "case {case}: allocator");
        }
        8 => {
            what = "0x1051e0";
            let g = rig.call(0x1051e0, &[]) as u8 != 0;
            assert_eq!(g, ctl.jump_running(), "case {case}: jump_running");
        }
        9 => {
            what = "0x105010";
            let p = rig.call(0x105010, &[a0 as u64, sbuf_arg as u64]) as u32;
            let pre = prefixes(&mut rig.ee);
            let rec = port.mem.arr6(a0);
            let want = area_name(&pre[rec[0] as usize], rec[1]);
            let buf = if sbuf_arg == 0 { 0x3b2190 } else { sbuf_arg };
            assert_eq!(p, buf, "case {case}: returned buffer");
            assert_eq!(exe_cstr(&mut rig.ee, p), want, "case {case}: area name");
            let bytes = (want.clone() + "\0").into_bytes();
            port.mem.put(buf, &bytes);
        }
        10 => {
            what = "0x105098";
            let p = rig.call(0x105098, &[a0 as u64]) as u32;
            let pre = prefixes(&mut rig.ee);
            let rec = port.mem.arr6(a0);
            let want = area_text(&pre[rec[0] as usize], &rec);
            assert_eq!(p, 0x3b2198, "case {case}: buffer");
            assert_eq!(exe_cstr(&mut rig.ee, p), want, "case {case}: area text");
            // The name was formatted at 0x3b2190 first, then the text over 0x3b2198.
            let name = area_name(&pre[rec[0] as usize], rec[1]);
            let mut bytes = (name + "\0").into_bytes();
            let at = 0x3b2190;
            port.mem.put(at, &bytes.split_off(0));
            port.mem.put(0x3b2198, &(want + "\0").into_bytes());
        }
        11 => {
            what = "0x105d60";
            let pre = prefixes(&mut rig.ee);
            let strs: [&str; 5] = ["tt", "zz", "eh", "xx", "tttt"];
            let s = if r.below(2) == 0 { pre[r.below(19) as usize].clone() } else { strs[r.below(5) as usize].to_string() };
            rig.ee.write(TASK + 0x700, &(s.clone() + "\0").into_bytes());
            port.mem.put(TASK + 0x700, &(s.clone() + "\0").into_bytes());
            let g = rig.call(0x105d60, &[(TASK + 0x700) as u64]) as u32;
            let names: Vec<&str> = pre.iter().map(|x| x.as_str()).collect();
            assert_eq!(g, world_of_prefix(&s, &names), "case {case}: world of prefix {s}");
        }
        12 => {
            what = "0x149db8";
            let g = rig.call(0x149db8, &[sxl(world as u32)]) as u32;
            let t = map_table(&mut rig.ee);
            assert_eq!(g, world_map_entrance(&t, world as u32), "case {case}: map entrance");
        }
        13 => {
            what = "0x105de0";
            let g = rig.call(0x105de0, &[sxl(idx as u32)]) as u32;
            let t = exe_table(&mut rig.ee, 0x378040, 4);
            assert_eq!(g, table_word(&t, idx), "case {case}: table word");
            let g2 = rig.call(0x105d48, &[sxl(world as u32)]) as u32;
            let t2 = exe_table(&mut rig.ee, 0x349dc0, 19);
            assert_eq!(g2, table_word(&t2, world), "case {case}: world name pointer");
        }
        14 => {
            what = "0x19d8c0";
            rig.call(0x19d8c0, &[]);
            clear_type4_memory(&mut ctl);
        }
        15 => {
            what = "0x149fa8";
            rig.call(0x149fa8, &[]);
            poll_set_stop(&mut ctl);
            rig.call(0x149fb8, &[]);
            poll_clear_stop(&mut ctl);
            rig.call(0x149fa8, &[]);
            poll_set_stop(&mut ctl);
            let g = rig.call(0x149fc8, &[]) as u8;
            assert_eq!(g, poll_stop_set(&ctl), "case {case}: poll flag");
        }
        16 => {
            what = "0x19d8d0";
            rig.call(0x19d8d0, &[(TASK + 0x500) as u64]);
            jr.init();
            port.mem.put(TASK + 0x500, &jr.0);
            let g = rig.call(0x19d9c8, &[(TASK + 0x500) as u64]) as u8 != 0;
            assert_eq!(g, jr.is_jump(), "case {case}: is_jump");
            ctlx = Some(());
        }
        _ => {
            what = "0x105ce8";
            let g = rig.call(0x105ce8, &[]) as u8 != 0;
            let region = rig.ee.r32(REGION_WORD);
            assert_eq!(g, region_is_not_pal_or_ntsc_j(region), "case {case}: region test");
            let g2 = rig.call(0x105d10, &[]) as u32;
            assert_eq!(g2, region_row(region), "case {case}: region row");
        }
    }
    let _ = ctlx;
    store_ctl(&ctl, &mut port.mem);
    store_save(&mut port.mem, &save);
    rig.finish(&port, case, what);
}

/// Each scenario runs once per test process; the test names below share it.
fn run_records() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(run_records_once);
}

fn run_records_once() {
    let Some(mut rig) = Rig::new("area records oracle") else { return };
    let ee: &Ee = &rig.ee;
    assert!(!ee.hooks.is_empty(), "the stand-ins are installed");
    let mut r = Rng::new(0xa4ea_0001);
    for case in 0..CASES * 6 {
        records(&mut rig, &mut r, case);
    }
    rig.require("area records", &["same_area true", "same_area false", "reset fires", "reset skips"], 20);
    eprintln!("area records: {} cases, {} stand-in calls", rig.cases, rig.calls);
}

#[test]
fn orig_104b90_matches_the_game() {
    run_records();
}

#[test]
fn orig_104bd0_matches_the_game() {
    run_records();
}

#[test]
fn orig_105188_matches_the_game() {
    run_records();
}

#[test]
fn orig_105070_matches_the_game() {
    run_records();
}

#[test]
fn orig_1052e0_matches_the_game() {
    run_records();
}

#[test]
fn orig_105420_matches_the_game() {
    run_records();
}

#[test]
fn orig_104f88_matches_the_game() {
    run_records();
}

#[test]
fn orig_104f98_matches_the_game() {
    run_records();
}

#[test]
fn orig_1051e0_matches_the_game() {
    run_records();
}

#[test]
fn orig_105010_matches_the_game() {
    run_records();
}

#[test]
fn orig_105098_matches_the_game() {
    run_records();
}

#[test]
fn orig_105d60_matches_the_game() {
    run_records();
}

#[test]
fn orig_149db8_matches_the_game() {
    run_records();
}

#[test]
fn orig_105de0_matches_the_game() {
    run_records();
}

#[test]
fn orig_105d48_matches_the_game() {
    run_records();
}

#[test]
fn orig_19d8c0_matches_the_game() {
    run_records();
}

#[test]
fn orig_149fa8_matches_the_game() {
    run_records();
}

#[test]
fn orig_149fb8_matches_the_game() {
    run_records();
}

#[test]
fn orig_149fc8_matches_the_game() {
    run_records();
}

#[test]
fn orig_19d8d0_matches_the_game() {
    run_records();
}

#[test]
fn orig_19d9c8_matches_the_game() {
    run_records();
}

#[test]
fn orig_105ce8_matches_the_game() {
    run_records();
}

#[test]
fn orig_105d10_matches_the_game() {
    run_records();
}


/// The area allocator and its virtual calls.
fn allocator(rig: &mut Rig, r: &mut Rng, case: u64) {
    let (mut m, seed) = prep(r);
    let which = r.below(8);
    let obj = ALC + 0x100;
    let obj2 = ALC + 0x200;
    m.w32(obj, VT);
    m.w32(0x33e850, if r.below(5) != 0 { obj } else { 0 });
    let base = ALC + 0x1000 + r.below(0x400) as u32;
    let sizes = [0u32, 0x40, 0x4f, 0x50, 0x51, 0x60, 0x1000, 0x10_0000, 0xffff_fff0, 0xffff_ffff];
    let size = if r.below(3) == 0 { sizes[r.below(sizes.len() as u64) as usize] } else { 0x40 + r.below(0x200) as u32 };
    let (a, b) = (r.word() as u32, r.word() as u32);
    rig.begin(&m, seed, 1, base);
    let mut port = rig.port(m.clone(), seed);
    let mut ctl = load_ctl(&mut port.mem);
    let save = load_save(&mut port.mem);
    let what;
    match which {
        0 => {
            what = "0x105f68";
            rig.call(0x105f68, &[obj2 as u64]);
            allocator_reset(&mut port, obj2);
        }
        1 => {
            what = "0x105f18";
            if r.below(2) == 0 {
                rig.call(0x105f18, &[obj2 as u64, a as u64, b as u64]);
            } else {
                rig.call(0x105f40, &[obj2 as u64, a as u64, b as u64]);
            }
            allocator_construct(&mut port, obj2, a, b);
        }
        2 => {
            what = "0x105f98";
            rig.call(0x105f98, &[obj2 as u64]);
            allocator_set_vtable(&mut port, obj2);
            rig.call(0x105fa8, &[(obj2 + 0x40) as u64]);
            allocator_set_vtable(&mut port, obj2 + 0x40);
        }
        3 => {
            what = "0x105fb8";
            rig.call(0x105fb8, &[obj2 as u64]);
            allocator_destroy(&mut port, obj2);
        }
        4 => {
            what = "0x105fd8";
            let g = rig.call(0x105fd8, &[base as u64, size as u64]) as u32;
            let w = create_memory_allocator(&mut port, base, size);
            assert_eq!(g, w, "case {case}: allocator object");
        }
        5 => {
            what = "0x105250";
            rig.call(0x105250, &[size as u64, a as u64]);
            ctl.create_allocator(&mut port, size, a);
        }
        6 => {
            what = "0x1052a0";
            rig.call(0x1052a0, &[]);
            ctl.destroy_allocator(&mut port);
        }
        _ => {
            what = "0x104fa8";
            if ctl.allocator != 0 {
                let g = rig.call(0x104fa8, &[a as u64]);
                let w = ctl.alloc(&mut port, a as u64);
                assert_eq!(g as u32, w as u32, "case {case}: alloc result");
                let g = rig.call(0x104fe0, &[b as u64]);
                let w = ctl.free(&mut port, b as u64);
                assert_eq!(g as u32, w as u32, "case {case}: free result");
            }
        }
    }
    store_ctl(&ctl, &mut port.mem);
    store_save(&mut port.mem, &save);
    rig.finish(&port, case, what);
}

/// Each scenario runs once per test process; the test names below share it.
fn run_allocator() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(run_allocator_once);
}

fn run_allocator_once() {
    let Some(mut rig) = Rig::new("area allocator oracle") else { return };
    let ee: &Ee = &rig.ee;
    assert!(!ee.hooks.is_empty(), "the stand-ins are installed");
    let mut r = Rng::new(0xa4ea_0002);
    for case in 0..CASES * 3 {
        allocator(&mut rig, &mut r, case);
    }
    rig.require("area allocator", &["108ff0", "1090a8", "80000000", "80000001", "106280"], 1);
    eprintln!("area allocator: {} cases, {} stand-in calls", rig.cases, rig.calls);
}

#[test]
fn orig_105f68_matches_the_game() {
    run_allocator();
}

#[test]
fn orig_105f18_matches_the_game() {
    run_allocator();
}

#[test]
fn orig_105f40_matches_the_game() {
    run_allocator();
}

#[test]
fn orig_105f98_matches_the_game() {
    run_allocator();
}

#[test]
fn orig_105fa8_matches_the_game() {
    run_allocator();
}

#[test]
fn orig_105fb8_matches_the_game() {
    run_allocator();
}

#[test]
fn orig_105fd8_matches_the_game() {
    run_allocator();
}

#[test]
fn orig_105250_matches_the_game() {
    run_allocator();
}

#[test]
fn orig_1052a0_matches_the_game() {
    run_allocator();
}

#[test]
fn orig_104fa8_matches_the_game() {
    run_allocator();
}

#[test]
fn orig_104fe0_matches_the_game() {
    run_allocator();
}


fn put_localset(r: &mut Rng, m: &mut Mem) {
    // The file entry and its data: count, then a u16 per pair (program byte, area byte).
    m.w32(LS_ENTRY + 8, LS_DATA);
    let count = r.below(9) as u16;
    m.w16(LS_DATA, count);
    for k in 0..count as u32 {
        let prog = match r.below(5) {
            0 => 0x40,
            1 => 0x7a,
            _ => r.below(0x30) as u8,
        };
        m.w8(LS_DATA + 2 + 2 * k, prog);
        m.w8(LS_DATA + 3 + 2 * k, r.below(64) as u8);
    }
}

/// Jumps: the begin and task functions, exits, the world-map jump, the title task, the local-set lookup and the record parser and player.
fn jumps(rig: &mut Rig, r: &mut Rng, case: u64) {
    let (mut m, seed) = prep(r);
    let which = r.below(13);
    m.w8(0x3b2180, if r.below(2) == 0 { 0 } else { r.word() as u8 | 1 });
    put_localset(r, &mut m);
    let save_world = r.below(19) as u8;
    m.w8(SAVE + 0xc, save_world);
    if r.below(2) == 0 {
        m.w32(0x1c8bd90, r.below(20) as u32);
    }
    let d = TASK + 0x600;
    dest(r, &mut m, d);
    let fade = match r.below(5) {
        0 => 0,
        1 => 1,
        2 => 0x4000,
        _ => r.word() as u32,
    };
    let wait = [0, 0, 1, 2, 3, -1, 7][r.below(7) as usize];
    let flag = r.word() as u8;
    let reset = [0, 0, 1, -1, 5][r.below(5) as usize];
    // The task block of the jump task and the exit object.
    let task = TASK + 0x300;
    m.w32(task + TASK_WAIT, [0u32, 1, 2, 3, 0xffff_fffe][r.below(5) as usize]);
    if r.below(3) == 0 {
        m.w32(task + TASK_RESET, 0);
    }
    m.w8(task + TASK_FLAG, if r.below(2) == 0 { 0 } else { 1 });
    let obj = TASK + 0x340;
    m.w32(obj + 8, TASK + 0x380);
    let world = [-1i32, 0, 1, 5, 0x12, 0xff, 3][r.below(7) as usize];
    let key: i64 = match r.below(6) {
        0 => -1,
        1 => 0x100 + r.below(8) as i64,
        2 => 0x40,
        3 => 0x7a,
        _ => r.below(0x30) as i64,
    };
    let lw: i64 = [-1i64, 0, 2, 0x12, 5][r.below(5) as usize];
    // Trigger shorts for the parser.
    let ps = TASK + 0x980;
    let kind = [0i16, 1, 1, 2, 2, 2, 3, 4, 5, 6, 7][r.below(11) as usize];
    let shorts: [i16; 6] = [kind, [-1i16, 0, 2, 0x12, 7][r.below(5) as usize], r.word() as i16, r.word() as i16, key as i16, r.word() as i16];
    for (k, s) in shorts.iter().enumerate() {
        m.w16(ps + 2 * k as u32, *s as u16);
    }
    // The jump record for the player.
    let rec = TASK + 0x500;
    let mut jr = JumpRec::default();
    for b in jr.0.iter_mut() {
        *b = r.word() as u8;
    }
    jr.set_kind(kind as u32 % 8);
    jr.set_party([-1, 0, 3, 0x20][r.below(4) as usize]);
    jr.set_reset([0, 0, 2, -3][r.below(4) as usize]);
    jr.set_menu(r.below(5) as i32);
    let dd0 = [r.below(19) as u8, r.below(64) as u8, r.word() as u8];
    jr.0[4..10].copy_from_slice(&[dd0[0], dd0[1], dd0[2], 0xff, r.word() as u8, 0xff]);
    m.put(rec, &jr.0);
    let extra = if r.below(3) == 0 { r.word() as u32 } else { 0 };
    let yield_at = 1 + r.below(3) as u32;
    if r.below(3) == 0 {
        m.w32(FAD + 0xfc, 4);
    } else if m.r32(FAD + 0xfc) == 4 {
        m.w32(FAD + 0xfc, 3);
    }
    rig.begin(&m, seed, yield_at, ALC + 0x1000);
    let mut port = rig.port(m.clone(), seed);
    let mut ctl = load_ctl(&mut port.mem);
    let mut save = load_save(&mut port.mem);
    let table = map_table(&mut rig.ee);
    let what;
    match which {
        0 => {
            what = "0x104e18";
            rig.call(0x104e18, &[d as u64, sxl(fade), sxl(wait as u32), flag as u64, sxl(reset as u32)]);
            let dd = port.mem.arr6(d);
            ctl.begin_jump(&mut port, &dd, fade, wait, flag, reset);
        }
        1 => {
            what = "0x104d58";
            rig.call(0x104d58, &[task as u64]);
            ctl.jump_task(&mut port, &mut save, task);
        }
        2 => {
            what = "0x104c68";
            rig.call(0x104c68, &[d as u64]);
            let dd = port.mem.arr6(d);
            ctl.start_thread(&mut port, &dd, &mut save);
        }
        3 => {
            what = "0x198fa0";
            rig.call(0x198fa0, &[obj as u64, d as u64]);
            let dd = port.mem.arr6(d);
            let mut flags = port.mem.r32(obj + 4);
            let id = port.mem.r16(TASK + 0x382);
            ctl.exit_walk_in(&mut port, &mut flags, id, &dd);
            port.mem.w32(obj + 4, flags);
        }
        4 => {
            what = "0x149df0";
            rig.call(0x149df0, &[sxl(world as u32), fade as u64]);
            ctl.to_world_map(&mut port, &mut save, &table, world as i64, fade);
        }
        5 => {
            what = "0x149f38";
            rig.call(0x149f38, &[]);
            ctl.title_start(&mut port);
        }
        6 => {
            what = "0x149eb0";
            rig.call(0x149eb0, &[(TASK + 0x900) as u64]);
            ctl.title_task(&mut port, sx(TASK + 0x900));
        }
        7 => {
            what = "0x198080";
            let out = TASK + 0x940;
            rig.call(0x198080, &[out as u64, key as u64, lw as u64]);
            let o = localset(&mut port, &ctl, key as u64, lw);
            port.mem.put(out, &o);
        }
        8 => {
            what = "0x19d8f0";
            rig.call(0x19d8f0, &[rec as u64, ps as u64]);
            let mut jr = JumpRec(port.mem.slot(rec, 0x20).try_into().unwrap());
            parse_jump(&mut port, &ctl, &mut jr, &shorts);
            port.mem.put(rec, &jr.0);
        }
        9 | 10 => {
            what = "0x19d9d8";
            rig.call(0x19d9d8, &[rec as u64, extra as u64]);
            let mut jr = JumpRec(port.mem.slot(rec, 0x20).try_into().unwrap());
            play_jump(&mut ctl, &mut port, &mut save, &table, &mut jr, extra);
            port.mem.put(rec, &jr.0);
        }
        11 => {
            what = "0x105e28";
            rig.call(0x105e28, &[]);
            let t = exe_table(&mut rig.ee, 0x3780b8, 4);
            ctl.leave_to_map_entrance(&mut port, &mut save, &t);
            rig.call(0x105ea8, &[]);
            ctl.leave_to_map(&mut port, &mut save);
        }
        _ => {
            what = "0x105df8";
            let g = rig.call(0x105df8, &[]) as u32;
            let t = exe_table(&mut rig.ee, 0x3780b8, 4);
            let w = ctl.entrance_word(&mut port, &t);
            assert_eq!(g, w, "case {case}: entrance word");
        }
    }
    store_ctl(&ctl, &mut port.mem);
    store_save(&mut port.mem, &save);
    rig.finish(&port, case, what);
}

/// Each scenario runs once per test process; the test names below share it.
fn run_jumps() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(run_jumps_once);
}

fn run_jumps_once() {
    let Some(mut rig) = Rig::new("area jump oracle") else { return };
    let ee: &Ee = &rig.ee;
    assert!(!ee.hooks.is_empty(), "the stand-ins are installed");
    let mut r = Rng::new(0xa4ea_0003);
    for case in 0..CASES * 13 {
        jumps(&mut rig, &mut r, case);
    }
    rig.require(
        "area jumps",
        &["1abfe8", "24f300", "1d73d0", "1684c0", "197b70", "1cb208", "102808", "24f0b8", "1a71c0", "13ef68", "1c6bd0", "1684c0 world 2", "1684c0 world 12", "102438", "14a7c8", "1039d0", "2f4c40"],
        20,
    );
    eprintln!("area jumps: {} cases, {} stand-in calls", rig.cases, rig.calls);
}

#[test]
fn orig_104e18_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_104d58_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_104c68_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_198fa0_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_149df0_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_149f38_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_149eb0_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_198080_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_19d8f0_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_19d9d8_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_105e28_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_105ea8_matches_the_game() {
    run_jumps();
}

#[test]
fn orig_105df8_matches_the_game() {
    run_jumps();
}


/// The glue: area start and end, the init and exit call runs, the poll task and the small getters.
fn glue(rig: &mut Rig, r: &mut Rng, case: u64) {
    let (mut m, seed) = prep(r);
    let which = r.below(24);
    if r.below(2) == 0 {
        let cur = m.arr6(0x33e830);
        m.put(0x33e848, &cur);
    }
    m.w32(0x33e850, ALC + 0x100);
    m.w32(ALC + 0x100, VT);
    let task = TASK + 0x300;
    let mode = r.word() as i64 % 20;
    if r.below(2) == 0 {
        m.w32(0x522178, 0xd);
    }
    let word = r.word();
    rig.begin(&m, seed, 1, ALC + 0x1000);
    let mut port = rig.port(m.clone(), seed);
    let mut ctl = load_ctl(&mut port.mem);
    let save = load_save(&mut port.mem);
    let fx = (port.mem.r32(0x35cd3c), port.mem.r32(0x35cd40));
    let vis = port.mem.r32(0x35cd10) as u64 | (port.mem.r32(0x35cd14) as u64) << 32;
    let flags = port.mem.r32(0x35d268);
    let what;
    match which {
        0 => {
            what = "0x1047a8";
            rig.call(0x1047a8, &[]);
            ctl.area_start(&mut port, fx, vis);
        }
        1 => {
            what = "0x104b60";
            rig.call(0x104b60, &[task as u64]);
            ctl.start_task(&mut port, task, fx, vis);
        }
        2 => {
            what = "0x104ce0";
            rig.call(0x104ce0, &[]);
            ctl.unload(&mut port);
        }
        3 => {
            what = "0x1688a0";
            rig.call(0x1688a0, &[]);
            game_init(&mut port);
        }
        4 => {
            what = "0x168928";
            rig.call(0x168928, &[]);
            boot_reset(&mut port, &mut ctl);
        }
        5 => {
            what = "0x168960";
            let md = if r.below(2) == 0 { 0xd } else { mode };
            rig.call(0x168960, &[md as u64]);
            mode_exit(&mut port, md);
        }
        6 => {
            what = "0x168ae0";
            rig.call(0x168ae0, &[word]);
            area_read(&mut port, word);
        }
        7 => {
            what = "0x168b10";
            rig.call(0x168b10, &[]);
            area_data_read(&mut port, &ctl, flags);
        }
        8 => {
            what = "0x168ba8";
            rig.call(0x168ba8, &[]);
            area_systems_init(&mut port, &ctl);
        }
        9 => {
            what = "0x168ca0";
            rig.call(0x168ca0, &[]);
            area_exit(&mut port);
        }
        10 => {
            what = "0x168ce8";
            rig.call(0x168ce8, &[]);
            area_finalize(&mut port);
        }
        11 => {
            what = "0x149b08";
            rig.call(0x149b08, &[]);
            display_read_data(&mut port);
        }
        12 => {
            what = "0x149b40";
            rig.call(0x149b40, &[word]);
            display_visibility(&mut port, &ctl, word);
        }
        13 => {
            what = "0x149ba0";
            rig.call(0x149ba0, &[]);
            display_init(&mut port);
        }
        14 => {
            what = "0x149bb8";
            rig.call(0x149bb8, &[]);
            display_start(&mut port, &mut ctl);
        }
        15 => {
            what = "0x149cb8";
            rig.call(0x149cb8, &[]);
            display_exit_a(&mut port, &mut ctl);
        }
        16 => {
            what = "0x149ce0";
            rig.call(0x149ce0, &[]);
            display_exit_b(&mut port);
        }
        17 => {
            what = "0x149d40";
            let (a, b) = (if r.below(2) == 0 { 1 } else { r.below(3) as i32 }, if r.below(2) == 0 { 0xffff } else { r.below(3) as i64 });
            rig.call(0x149d40, &[sxl(a as u32), b as u64]);
            display_reset(&mut port, a, b);
            rig.call(0x149d78, &[]);
            display_reset_all(&mut port);
        }
        18 => {
            what = "0x149f70";
            rig.call(0x149f70, &[]);
            field_effect_start(&mut port, &ctl);
        }
        19 => {
            what = "0x149fd8";
            let g = rig.call(0x149fd8, &[]) as u8 != 0;
            assert_eq!(g, camp_open(&mut port), "case {case}: camp_open");
            let g = rig.call(0x14a000, &[]) as u8 != 0;
            assert_eq!(g, camp_idle(&mut port), "case {case}: camp_idle");
            let g = rig.call(0x14a038, &[]) as u8 != 0;
            assert_eq!(g, pause_released(&mut port), "case {case}: pause_released");
        }
        20 => {
            what = "0x14a060";
            rig.call(0x14a060, &[]);
            pause_step(&mut port);
        }
        21 => {
            what = "0x14a1e0";
            rig.call(0x14a1e0, &[]);
            poll_start(&mut port, &mut ctl);
        }
        22 => {
            what = "0x1051f0";
            let (a, b) = (0x33e830u32, TASK + 0x700);
            let g = rig.call(0x1051f0, &[a as u64, b as u64]) as u8 != 0;
            assert_eq!(g, ctl.more_to_play(&mut port, a, b), "case {case}: more_to_play");
        }
        _ => {
            what = "0x102498";
            rig.call(0x102498, &[task as u64]);
            end_task(&mut port, task);
        }
    }
    store_ctl(&ctl, &mut port.mem);
    store_save(&mut port.mem, &save);
    rig.finish(&port, case, what);
}

/// Each scenario runs once per test process; the test names below share it.
fn run_glue() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(run_glue_once);
}

fn run_glue_once() {
    let Some(mut rig) = Rig::new("area glue oracle") else { return };
    let ee: &Ee = &rig.ee;
    assert!(!ee.hooks.is_empty(), "the stand-ins are installed");
    let mut r = Rng::new(0xa4ea_0004);
    for case in 0..CASES * 24 {
        glue(&mut rig, &mut r, case);
    }
    rig.require("area glue", &["15ade8", "13d6d8", "1b9550", "1490d8"], 1);
    eprintln!("area glue: {} cases, {} stand-in calls", rig.cases, rig.calls);
}

#[test]
fn orig_1047a8_matches_the_game() {
    run_glue();
}

#[test]
fn orig_104b60_matches_the_game() {
    run_glue();
}

#[test]
fn orig_104ce0_matches_the_game() {
    run_glue();
}

#[test]
fn orig_1688a0_matches_the_game() {
    run_glue();
}

#[test]
fn orig_168928_matches_the_game() {
    run_glue();
}

#[test]
fn orig_168960_matches_the_game() {
    run_glue();
}

#[test]
fn orig_168ae0_matches_the_game() {
    run_glue();
}

#[test]
fn orig_168b10_matches_the_game() {
    run_glue();
}

#[test]
fn orig_168ba8_matches_the_game() {
    run_glue();
}

#[test]
fn orig_168ca0_matches_the_game() {
    run_glue();
}

#[test]
fn orig_168ce8_matches_the_game() {
    run_glue();
}

#[test]
fn orig_149b08_matches_the_game() {
    run_glue();
}

#[test]
fn orig_149b40_matches_the_game() {
    run_glue();
}

#[test]
fn orig_149ba0_matches_the_game() {
    run_glue();
}

#[test]
fn orig_149bb8_matches_the_game() {
    run_glue();
}

#[test]
fn orig_149cb8_matches_the_game() {
    run_glue();
}

#[test]
fn orig_149ce0_matches_the_game() {
    run_glue();
}

#[test]
fn orig_149d40_matches_the_game() {
    run_glue();
}

#[test]
fn orig_149d78_matches_the_game() {
    run_glue();
}

#[test]
fn orig_149f70_matches_the_game() {
    run_glue();
}

#[test]
fn orig_149fd8_matches_the_game() {
    run_glue();
}

#[test]
fn orig_14a000_matches_the_game() {
    run_glue();
}

#[test]
fn orig_14a038_matches_the_game() {
    run_glue();
}

#[test]
fn orig_14a060_matches_the_game() {
    run_glue();
}

#[test]
fn orig_14a1e0_matches_the_game() {
    run_glue();
}

#[test]
fn orig_1051f0_matches_the_game() {
    run_glue();
}

#[test]
fn orig_102498_matches_the_game() {
    run_glue();
}


fn load_groups(m: &mut Mem) -> GroupTable {
    let mut t = GroupTable::default();
    let raw = m.slot(GROUP_BASE, GROUP_COUNT * GROUP_SIZE).to_vec();
    for (i, rec) in t.recs.iter_mut().enumerate() {
        rec.0.copy_from_slice(&raw[i * GROUP_SIZE..(i + 1) * GROUP_SIZE]);
    }
    t
}

fn store_groups(t: &GroupTable, m: &mut Mem) {
    for (i, rec) in t.recs.iter().enumerate() {
        m.put(GROUP_BASE + (i * GROUP_SIZE) as u32, &rec.0);
    }
}

/// A group header at `base` with small section counts and a few signals.
fn build_header(r: &mut Rng, m: &mut Mem, base: u32) {
    let (n4, n6, n8, n10, ns) = (r.below(4) as u16, r.below(3) as u16, r.below(3) as u16, r.below(3) as u16, r.below(4) as u16);
    m.w8(base, [2u8, 5, 1, 0, 7][r.below(5) as usize]);
    m.w8(base + 1, r.below(4) as u8);
    m.w16(base + 2, if r.below(5) == 0 { 0x8123 } else { r.below(300) as u16 });
    m.w16(base + 4, n4);
    m.w16(base + 6, n6);
    m.w16(base + 8, n8);
    m.w16(base + 10, n10);
    m.w16(base + 0xc, ns);
    m.w16(base + 0x1c, r.word() as u16);
    let mut p = base + 0x2c + (n4 as u32 + n6 as u32) * 0x40;
    for _ in 0..n8 {
        let c = r.below(3) as u16;
        m.w16(p + 2, c);
        p += c as u32 * 0xc + 8;
    }
    p += n10 as u32 * 0x10;
    for i in 0..ns as u32 {
        let ev: i16 = [0x4a, 1, 2, 0x14, -3][r.below(5) as usize];
        m.w16(p + i * 8, ev as u16);
        m.w16(p + i * 8 + 2, r.word() as u16);
        m.w8(p + i * 8 + 4, r.below(3) as u8);
    }
    // Entities for the kind-2 search: kind byte at +0x48 + i * 0x40 and the value at +0x5c + i * 0x40.
    for i in 0..n4 as u32 {
        m.w8(base + 0x48 + i * 0x40, [0u8, 2, 2, 1][r.below(4) as usize]);
        m.w8(base + 0x5c + i * 0x40, r.below(3) as u8);
    }
}

/// Group records, their objects and the slot counter.
fn groups(rig: &mut Rig, r: &mut Rng, case: u64) {
    let (mut m, seed) = prep(r);
    let which = r.below(9);
    // Records: clean list words, a few with a chain of objects (each links at +0x60 and has the virtual table at +0).
    let mut next_obj = HDR + 0x2000;
    for i in 0..GROUP_COUNT as u32 {
        let rec = GROUP_BASE + i * GROUP_SIZE as u32;
        let chain = if r.below(5) == 0 { r.below(4) } else { 0 };
        let mut head = 0;
        for _ in 0..chain {
            m.w32(next_obj, VT);
            m.w32(next_obj + 0x60, head);
            head = next_obj;
            next_obj += 0x100;
        }
        m.w32(rec + 0xc, head);
    }
    let hdr = HDR + 0x400;
    build_header(r, &mut m, hdr);
    let rec0 = GROUP_BASE + r.below(GROUP_COUNT as u64) as u32 * GROUP_SIZE as u32;
    m.w32(rec0 + 8, hdr);
    let kind = r.word() as u32;
    let unit = TASK + 0x800;
    let id = r.below(4);
    rig.begin(&m, seed, 1, ALC + 0x1000);
    let mut port = rig.port(m.clone(), seed);
    let mut ctl = load_ctl(&mut port.mem);
    let save = load_save(&mut port.mem);
    let mut table = load_groups(&mut port.mem);
    let ri = ((rec0 - GROUP_BASE) / GROUP_SIZE as u32) as usize;
    let what;
    match which {
        0 => {
            what = "0x198300";
            let g = rig.call(0x198300, &[]) as u32;
            let w = GroupTable::take(&mut ctl);
            assert_eq!(g, w, "case {case}: take");
        }
        1 => {
            what = "0x198328";
            rig.call(0x198328, &[]);
            GroupTable::give_back(&mut ctl);
        }
        2 => {
            what = "0x198340";
            rig.call(0x198340, &[rec0 as u64, kind as u64, hdr as u64]);
            table.recs[ri].init(&mut port, kind, hdr);
        }
        3 => {
            what = "0x1985f8";
            if r.below(2) == 0 {
                rig.call(0x1985f8, &[rec0 as u64]);
            } else {
                rig.call(0x198660, &[rec0 as u64]);
            }
            table.recs[ri].dismiss_all(&mut port);
        }
        4 => {
            what = "0x199698";
            let g = rig.call(0x199698, &[rec0 as u64, id]) as u8 != 0;
            let w = table.recs[ri].has_kind2_entity(&mut port, id);
            assert_eq!(g, w, "case {case}: has_kind2_entity");
            rig.hit(if g { "kind2 true" } else { "kind2 false" });
        }
        5 => {
            what = "0x19a308";
            let (a, b) = (if r.below(2) == 0 { 1 } else { 0 }, if r.below(4) == 0 { r.below(3) as i64 } else { 0xffff });
            rig.call(0x19a308, &[a, b as u64]);
            table.reset(&mut port, a as i32, b);
        }
        6 => {
            what = "0x19a3b8";
            rig.call(0x19a3b8, &[]);
            table.clear_all(&mut port);
        }
        7 => {
            what = "0x19a3d8";
            rig.call(0x19a3d8, &[]);
            table.dismiss_every_group(&mut port);
        }
        _ => {
            what = "0x1982c8";
            rig.call(0x1982c8, &[unit as u64, id]);
            unit_appear(&mut port, unit, id);
        }
    }
    store_ctl(&ctl, &mut port.mem);
    store_groups(&table, &mut port.mem);
    store_save(&mut port.mem, &save);
    rig.finish(&port, case, what);
}

/// Each scenario runs once per test process; the test names below share it.
fn run_groups() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(run_groups_once);
}

fn run_groups_once() {
    let Some(mut rig) = Rig::new("area groups oracle") else { return };
    let ee: &Ee = &rig.ee;
    assert!(!ee.hooks.is_empty(), "the stand-ins are installed");
    let mut r = Rng::new(0xa4ea_0005);
    for case in 0..CASES * 9 {
        groups(&mut rig, &mut r, case);
    }
    rig.require("area groups", &["1975e0", "197838", "197870", "80000001", "198220", "kind2 true", "kind2 false"], 20);
    eprintln!("area groups: {} cases, {} stand-in calls", rig.cases, rig.calls);
}

#[test]
fn orig_198300_matches_the_game() {
    run_groups();
}

#[test]
fn orig_198328_matches_the_game() {
    run_groups();
}

#[test]
fn orig_198340_matches_the_game() {
    run_groups();
}

#[test]
fn orig_1985f8_matches_the_game() {
    run_groups();
}

#[test]
fn orig_198660_matches_the_game() {
    run_groups();
}

#[test]
fn orig_199698_matches_the_game() {
    run_groups();
}

#[test]
fn orig_19a308_matches_the_game() {
    run_groups();
}

#[test]
fn orig_19a3b8_matches_the_game() {
    run_groups();
}

#[test]
fn orig_19a3d8_matches_the_game() {
    run_groups();
}

#[test]
fn orig_1982c8_matches_the_game() {
    run_groups();
}


/// The player-like object, its activator block, the spawn record and the character data the group handlers read.
const OBJ: u32 = HDR + 0xe000;
const ACT: u32 = HDR + 0xe400;
const SPN: u32 = HDR + 0xe800;
const CHA: u32 = HDR + 0xe900;
const AI: u32 = ALC + 0x3000;

fn lay_objects(r: &mut Rng, m: &mut Mem) {
    m.w32(OBJ, VT);
    m.w32(OBJ + 0x6c, ACT);
    m.w32(OBJ + 0x81c, if r.below(4) == 0 { 0 } else { SPN });
    m.w32(OBJ + 0x588, if r.below(2) == 0 { 0x800 } else { r.word() as u32 });
    m.w32(OBJ + 0x7f8, r.word() as u32);
    m.w16(ACT + 2, r.below(4) as u16);
    m.w16(ACT + 0x2c, r.below(3) as u16);
    m.w8(ACT + 0x2e, r.below(3) as u8);
    m.w8(ACT + 0x2f, r.below(3) as u8);
    m.w8(SPN + 0x30, [0u8, 50, 99, 100, 200][r.below(5) as usize]);
    m.w8(CHA + 4, [4u8, 0x15, 3, 7][r.below(4) as usize]);
    m.w8(CHA + 0x54, r.below(20) as u8);
    m.w32(0x35c998, OBJ);
    m.w8(0x35e3a0, r.below(2) as u8);
    // The frame step and the repeat float are finite numbers.
    m.w32(0x356f40, [0.0f32, 1.0, 0.5, 2.0][r.below(4) as usize].to_bits());
    for i in 0..GROUP_COUNT as u32 {
        let rec = GROUP_BASE + i * GROUP_SIZE as u32;
        m.w32(rec + 0x14, [0.0f32, 1.0, 0.5, 3.0, -1.0][r.below(5) as usize].to_bits());
    }
}

/// More group handlers: the entity filter, the preload, the appear paths, the kind dispatch and the frame pass.
fn groups2(rig: &mut Rig, r: &mut Rng, case: u64) {
    let (mut m, seed) = prep(r);
    let which = r.below(26);
    let mut next_obj = HDR + 0x2000;
    for i in 0..GROUP_COUNT as u32 {
        let rec = GROUP_BASE + i * GROUP_SIZE as u32;
        let chain = if r.below(3) == 0 { r.below(4) } else { 0 };
        let mut head = 0;
        for _ in 0..chain {
            m.w32(next_obj, VT);
            m.w32(next_obj + 0x60, head);
            m.w32(next_obj + 0x6c, ACT);
            head = next_obj;
            next_obj += 0x100;
        }
        m.w32(rec + 0xc, head);
        // Flags: the frame pass clears bits 3 and 4; vary the others.
        m.w32(rec + 4, [0u32, 1, 8, 0x10, 0x19, 2][r.below(6) as usize]);
    }
    lay_objects(r, &mut m);
    m.w8(0x3b2180, r.below(2) as u8);
    let hdr = HDR + 0x400;
    build_header(r, &mut m, hdr);
    m.w8(hdr, [0u8, 1, 2, 3, 4, 5, 6, 6, 7, 8, 9, 10, 11][r.below(13) as usize]);
    m.w8(hdr + 0x1c, [0x64u8, 0x65, 0x12, 0][r.below(4) as usize]);
    let idx = r.below(GROUP_COUNT as u64) as usize;
    let rec0 = GROUP_BASE + idx as u32 * GROUP_SIZE as u32;
    m.w32(rec0 + 8, hdr);
    if (18..24).contains(&which) && r.below(2) == 0 {
        m.w32(rec0 + 4, 0);
        m.w8(hdr, if which < 22 { 1 } else { [1u8, 2, 3, 6, 7, 8, 9, 10][r.below(8) as usize] });
        m.w16(ACT + 2, r.below(3) as u16);
    }
    // A unit record: object id, kind at +0x1c, entrance at +0x1d.
    let unit = HDR + 0x8000;
    m.w8(unit + 0x1c, r.below(3) as u8);
    let cur2 = m.r8(0x33e832);
    m.w8(unit + 0x1d, if r.below(2) == 0 { cur2 } else { r.word() as u8 });
    // Slots in use, each with a header of its own.
    let slots = r.below(6) as u32;
    m.w32(0x35e468, slots);
    for i in 0..slots + 1 {
        let rec = GROUP_BASE + i * GROUP_SIZE as u32;
        if i != idx as u32 || which < 8 {
            let h = HDR + 0x6000 + i * 0x40;
            m.w32(rec + 8, h);
            m.w16(h + 2, r.below(4) as u16);
        }
    }
    // The used group's header has its entities' serial and kinds already (build_header); the wanted id for the finders.
    let want = r.below(5);
    let gid: i64 = if r.below(4) == 0 { -1 } else { r.below(3) as i64 };
    // Initialise the record from its header the way the area load does (the port's `init` is verified on its own), then
    // vary the flags, the list head and the byte at +0x20.
    {
        let head0 = m.r32(rec0 + 0xc);
        let flags0 = m.r32(rec0 + 4);
        let init_kind = r.word() as u32;
        let mut tmp = rig.port(m.clone(), seed);
        let mut t = load_groups(&mut tmp.mem);
        t.recs[idx].init(&mut tmp, init_kind, hdr);
        store_groups(&t, &mut tmp.mem);
        m = tmp.mem;
        m.w32(rec0 + 0xc, head0);
        m.w32(rec0 + 4, flags0);
        m.w8(rec0 + 0x20, r.word() as u8);
    }
    let obj_arg = if r.below(4) == 0 { 0 } else { OBJ as u64 };
    let who = r.word() % 3;
    let kind2 = r.word() as u32;
    rig.begin(&m, seed, 1, ALC + 0x1000);
    let mut port = rig.port(m.clone(), seed);
    let mut ctl = load_ctl(&mut port.mem);
    let mut save = load_save(&mut port.mem);
    let mut table = load_groups(&mut port.mem);
    let what;
    match which {
        0 => {
            what = "0x1981a0";
            let g = rig.call(0x1981a0, &[unit as u64]) as u8 != 0;
            assert_eq!(g, entity_wanted(&mut port, &ctl, unit), "case {case}: entity_wanted");
            rig.hit(if g { "wanted true" } else { "wanted false" });
        }
        1 => {
            what = "0x1986c8";
            rig.call(0x1986c8, &[rec0 as u64]);
            table.preload(&mut port, &ctl, idx);
        }
        2 => {
            what = "0x198920";
            rig.call(0x198920, &[rec0 as u64, unit as u64]);
            table.appear_entity(&mut port, &ctl, idx, unit);
        }
        3 => {
            what = "0x199378";
            let g = rig.call(0x199378, &[rec0 as u64, OBJ as u64]) as u8 != 0;
            assert_eq!(g, unit_pinned(&mut port, OBJ), "case {case}: unit_pinned");
            rig.call(0x199398, &[rec0 as u64, OBJ as u64]);
            table.empty_check(&mut port, idx, OBJ);
        }
        4 => {
            what = "0x1993f8";
            rig.call(0x1993f8, &[rec0 as u64, OBJ as u64]);
            table.unit_left(&mut port, idx, OBJ);
        }
        5 => {
            what = "0x199580";
            let g = rig.call(0x199580, &[]) as u32 as i32;
            assert_eq!(g, table.used_total(&ctl), "case {case}: used_total");
        }
        6 => {
            what = "0x1995c8";
            let g = rig.call(0x1995c8, &[rec0 as u64, who]) as u8 != 0;
            assert_eq!(g, serial_free(&mut port, who), "case {case}: serial_free");
        }
        7 => {
            what = "0x1996f0";
            let g = rig.call(0x1996f0, &[rec0 as u64]) as u32 as i32;
            assert_eq!(g, table.entity_cost(&mut port, idx), "case {case}: entity_cost");
        }
        8 => {
            what = "0x199930";
            rig.call(0x199930, &[rec0 as u64]);
            table.fire_once(&mut port, idx);
        }
        9 => {
            what = "0x199bf8";
            let g = rig.call(0x199bf8, &[want]) as u32;
            assert_eq!(g, table.find(&mut port, &ctl, want), "case {case}: find");
        }
        10 => {
            what = "0x199c48";
            rig.call(0x199c48, &[want]);
            table.disable(&mut port, &ctl, want);
            rig.call(0x199c70, &[want ^ 1]);
            table.enable(&mut port, &ctl, want ^ 1);
        }
        11 => {
            what = "0x199ca0";
            let g = rig.call(0x199ca0, &[rec0 as u64]) as u32;
            assert_eq!(g, table.routine_param(&mut port, idx), "case {case}: routine_param");
        }
        12 => {
            what = "0x199cc0";
            let g = rig.call(0x199cc0, &[rec0 as u64, gid as u64]) as u32;
            assert_eq!(g, table.find_signal_block(&mut port, idx, gid), "case {case}: find_signal_block");
        }
        13 => {
            what = "0x199d18";
            let g = rig.call(0x199d18, &[rec0 as u64, want]) as u32;
            assert_eq!(g, table.find_section_block(&mut port, idx, want), "case {case}: find_section_block");
        }
        14 => {
            what = "0x199d68";
            let g = rig.call(0x199d68, &[rec0 as u64, who]) as u32;
            assert_eq!(g, table.first_accepting(&mut port, idx, who), "case {case}: first_accepting");
        }
        15 => {
            what = "0x199e10";
            rig.call(0x199e10, &[]);
            table.roll_over(&mut port, &ctl);
        }
        16 => {
            what = "0x199e98";
            rig.call(0x199e98, &[rec0 as u64]);
            table.appear_on_demand(&mut port, idx);
        }
        17 => {
            what = "0x199f70";
            rig.call(0x199f70, &[rec0 as u64, OBJ as u64]);
            GroupTable::switch_parts(&mut port, OBJ);
        }
        18 | 19 => {
            what = "0x198e10";
            rig.call(0x198e10, &[rec0 as u64, OBJ as u64]);
            table.exit_check(&mut port, &mut ctl, &mut save, idx, OBJ);
        }
        20 | 21 => {
            what = "0x198b30";
            let g = rig.call(0x198b30, &[rec0 as u64, obj_arg]) as u32 != 0;
            let w = table.dispatch(&mut port, &mut ctl, &mut save, idx, obj_arg);
            assert_eq!(g, w, "case {case}: dispatch answer");
        }
        22 | 23 => {
            what = "0x1989d8";
            rig.call(0x1989d8, &[rec0 as u64, who]);
            table.frame_pass(&mut port, &mut ctl, &mut save, idx, who);
        }
        24 => {
            what = "0x19dd78";
            rig.call(0x19dd78, &[(TASK + 0x700) as u64]);
            clear_pair(&mut port, TASK + 0x700);
        }
        _ => {
            what = "0x198430";
            rig.call(0x198430, &[rec0 as u64, kind2 as u64, hdr as u64]);
            table.recs[idx].init(&mut port, kind2, hdr);
        }
    }
    store_ctl(&ctl, &mut port.mem);
    store_groups(&table, &mut port.mem);
    store_save(&mut port.mem, &save);
    rig.finish(&port, case, what);
}

/// Each scenario runs once per test process; the test names below share it.
fn run_groups2() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(run_groups2_once);
}

fn run_groups2_once() {
    let Some(mut rig) = Rig::new("area group handlers oracle") else { return };
    let ee: &Ee = &rig.ee;
    assert!(!ee.hooks.is_empty(), "the stand-ins are installed");
    let mut r = Rng::new(0xa4ea_0006);
    for case in 0..CASES * 26 {
        groups2(&mut rig, &mut r, case);
    }
    rig.require(
        "area group handlers",
        &["wanted true", "wanted false", "16e638", "198220", "1994c8", "199518", "1942c8", "199058", "1d88a0", "195358", "1991a0", "199e68", "19a000", "15e9a8", "15e928", "1abf40", "80000002", "102778"],
        5,
    );
    eprintln!("area group handlers: {} cases, {} stand-in calls", rig.cases, rig.calls);
}

#[test]
fn orig_1981a0_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_1986c8_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_198920_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199378_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199398_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_1993f8_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199580_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_1995c8_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_1996f0_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199930_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199bf8_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199c48_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199c70_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199ca0_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199cc0_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199d18_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199d68_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199e10_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199e98_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_199f70_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_198e10_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_198b30_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_1989d8_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_19dd78_matches_the_game() {
    run_groups2();
}

#[test]
fn orig_198430_matches_the_game() {
    run_groups2();
}


/// The area info table and its readers.
fn areainfo(rig: &mut Rig, r: &mut Rng, case: u64) {
    let (mut m, seed) = prep(r);
    let which = r.below(14);
    // The table: world count at +4, a row per world at +8 + 4 * world (count, offset), entries of 0x40 bytes.
    let nw = r.below(4) as u32;
    m.w32(0x1c8a970, AI);
    m.w8(0x1c8a974, r.below(2) as u8);
    m.w8(0x1c8a975, r.below(2) as u8);
    m.w32(AI + 4, nw);
    let mut off = 0x40u32;
    for w in 0..4u32 {
        let n = r.below(4) as u16;
        m.w16(AI + 8 + 4 * w, n);
        m.w16(AI + 10 + 4 * w, off as u16);
        off += n as u32 * 0x40;
    }
    for e in 0..16u32 {
        let a = AI + 0x40 + e * 0x40;
        m.w16(a + 0x32, if r.below(3) == 0 { 0 } else { r.word() as u16 });
    }
    // The current place is a small one so the rows are hit.
    m.put(0x33e830, &[r.below(4) as u8, r.below(4) as u8, r.word() as u8, 0, 0, 0]);
    let area: i64 = [-1i64, 0, 1, 2, 3][r.below(5) as usize];
    let world: i32 = [-1i32, 0, 1, 2, 3][r.below(5) as usize];
    rig.begin(&m, seed, 1, ALC + 0x1000);
    let mut port = rig.port(m.clone(), seed);
    let mut ctl = load_ctl(&mut port.mem);
    let save = load_save(&mut port.mem);
    let what;
    match which {
        0 => {
            what = "0x1933a0";
            let g = rig.call(0x1933a0, &[area as u64, sxl(world as u32)]) as u32;
            let w = areainfo_entry(&mut port, &ctl, area, world);
            assert_eq!(g, w, "case {case}: areainfo_entry");
            rig.hit(if w != 0 { "areainfo found" } else { "areainfo missing" });
        }
        1 => {
            what = "0x193410";
            let bit = r.below(40) as u32;
            let g = rig.call(0x193410, &[area as u64, sxl(world as u32), bit as u64]) as u8 != 0;
            assert_eq!(g, areainfo_bit(&mut port, &ctl, area, world, bit), "case {case}: areainfo_bit");
        }
        2 => {
            what = "0x193458";
            let g = rig.call(0x193458, &[area as u64, sxl(world as u32)]) as u8 != 0;
            assert_eq!(g, areainfo_bit0(&mut port, &ctl, area, world), "case {case}: bit0");
            let g = rig.call(0x1934a0, &[area as u64, sxl(world as u32)]) as u8 != 0;
            assert_eq!(g, areainfo_bit2(&mut port, &ctl, area, world), "case {case}: bit2");
        }
        3 => {
            what = "0x193478";
            let g = rig.call(0x193478, &[]) as u8 != 0;
            assert_eq!(g, areainfo_bit1(&mut port, &ctl), "case {case}: bit1");
            let g = rig.call(0x1934c0, &[]) as u8 != 0;
            assert_eq!(g, areainfo_no_shadow(&mut port, &ctl), "case {case}: no shadow");
            let g = rig.call(0x1934e8, &[]) as u8 != 0;
            assert_eq!(g, areainfo_bit4(&mut port, &ctl), "case {case}: bit4");
        }
        4 => {
            what = "0x1935e8";
            let g = rig.call(0x1935e8, &[]) as u8;
            assert_eq!(g, areainfo_byte34(&mut port, &ctl), "case {case}: byte34");
        }
        5 => {
            what = "0x193510";
            rig.call(0x193510, &[]);
            areainfo_apply(&mut port, &ctl, &save);
        }
        6 => {
            what = "0x193560";
            rig.call(0x193560, &[]);
            areainfo_scan(&mut port, &mut ctl);
        }
        7 => {
            what = "0x193380";
            rig.call(0x193380, &[]);
            areainfo_init(&mut port, &mut ctl, &save);
        }
        8 => {
            what = "0x1935b0";
            let g = rig.call(0x1935b0, &[]) as u8 != 0;
            assert_eq!(g, areainfo_ready(&ctl), "case {case}: ready");
            rig.hit(if g { "ready true" } else { "ready false" });
        }
        9 => {
            what = "0x1935d8";
            rig.call(0x1935d8, &[]);
            areainfo_hold(&mut ctl);
        }
        10 => {
            what = "0x193ef8";
            let g = rig.call(0x193ef8, &[]) as u32;
            assert_eq!(g, areainfo_file(&ctl), "case {case}: file");
        }
        _ => {
            what = "0x193510";
            rig.call(0x193510, &[]);
            areainfo_apply(&mut port, &ctl, &save);
        }
    }
    store_ctl(&ctl, &mut port.mem);
    store_save(&mut port.mem, &save);
    rig.finish(&port, case, what);
}

/// Each scenario runs once per test process; the test names below share it.
fn run_areainfo() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(run_areainfo_once);
}

fn run_areainfo_once() {
    let Some(mut rig) = Rig::new("area info oracle") else { return };
    let ee: &Ee = &rig.ee;
    assert!(!ee.hooks.is_empty(), "the stand-ins are installed");
    let mut r = Rng::new(0xa4ea_0007);
    for case in 0..CASES * 14 {
        areainfo(&mut rig, &mut r, case);
    }
    rig.require("area info", &["areainfo found", "areainfo missing", "ready true", "ready false", "1a7700", "1a1cc8"], 5);
    eprintln!("area info: {} cases, {} stand-in calls", rig.cases, rig.calls);
}

#[test]
fn orig_1933a0_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_193410_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_193458_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_193478_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_1934a0_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_1934c0_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_1934e8_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_1935e8_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_193510_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_193560_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_193380_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_1935b0_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_1935d8_matches_the_game() {
    run_areainfo();
}

#[test]
fn orig_193ef8_matches_the_game() {
    run_areainfo();
}

