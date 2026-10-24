//! Oracle tests for the area and system loading remainder (work package L1-P19). kh2-ee runs each original from the user's executable
//! and the port (`system_tables`, `objlist`, `kh2_bar::request`, `kh2_ard::run`) runs on the same inputs.
//!
//! Part one, the leaves (table setters, the cost budget, the spawn record fill, bit tests), use the `factory gen` harness (`fuzz`,
//! 4,000 cases, blocks and globals random, every byte compared). Part two, below the line, are hand harnesses for the functions that
//! reach other units or follow pointers between blocks: kh2-ee runs the original and the port runs on a copy of the same memory
//! windows (random bytes), and after every case the ordered list of calls to other units (address and argument words), the return
//! value and every byte of every window are compared.
//!
//! Stand-ins (hooks), listed at each hand test: every function a unit calls that is not ported in it is a hook that logs its address
//! and arguments and returns a value drawn the same way on both sides; the port's host logs the same call. Virtual methods are hooks
//! behind one shared function table. Everything else the originals call runs for real: 0x1684c0 and 0x168510 (the BAR lookups), the
//! list insert 0x1b7440, the flag-table leaves 0x19de70 and 0x1a77e8, the group record functions 0x198660 and 0x198328, the empty
//! function 0x1d13f0.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_factory::fuzz::*;
use kh2_game_core::system_tables as t;

const CASES: u32 = 4000;

fn report(st: Stats) {
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

const fn blk(size: u32, fields: &'static [FieldSpec]) -> Block {
    Block { size, fields, subs: &[] }
}

const fn spec(addr: u32, args: &'static [Arg], ret: Ret, globals: &'static [(u32, u32, bool)]) -> Spec {
    Spec { addr, args, floats: 0, ret, globals, stubs: &[], nulls: true }
}

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

// ---- factory harness: the table setters ------------------------------------------------------------------------------------------

/// The table word at 0x1c89700: the argument is stored.
#[test]
fn orig_1845a0_matches_the_game() {
    const S: Spec = spec(0x1845a0, &[Arg::Int(&[])], Ret::None, &[(0x1c89700, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1845a0(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1c89704: the argument is stored.
#[test]
fn orig_184630_matches_the_game() {
    const S: Spec = spec(0x184630, &[Arg::Int(&[])], Ret::None, &[(0x1c89704, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_184630(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1c8a970: the argument is stored.
#[test]
fn orig_193370_matches_the_game() {
    const S: Spec = spec(0x193370, &[Arg::Int(&[])], Ret::None, &[(0x1c8a970, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_193370(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1c95590: the argument is stored.
#[test]
fn orig_1a6800_matches_the_game() {
    const S: Spec = spec(0x1a6800, &[Arg::Int(&[])], Ret::None, &[(0x1c95590, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1a6800(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d5d640: the argument is stored.
#[test]
fn orig_1b03a8_matches_the_game() {
    const S: Spec = spec(0x1b03a8, &[Arg::Int(&[])], Ret::None, &[(0x1d5d640, 8, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1b03a8(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d5d6b4: the argument is stored.
#[test]
fn orig_1b50b0_matches_the_game() {
    const S: Spec = spec(0x1b50b0, &[Arg::Int(&[])], Ret::None, &[(0x1d5d6b4, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1b50b0(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d5dd80: the argument is stored.
#[test]
fn orig_1bea98_matches_the_game() {
    const S: Spec = spec(0x1bea98, &[Arg::Int(&[])], Ret::None, &[(0x1d5dd80, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1bea98(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d5df10: the argument is stored.
#[test]
fn orig_1bfef0_matches_the_game() {
    const S: Spec = spec(0x1bfef0, &[Arg::Int(&[])], Ret::None, &[(0x1d5df10, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1bfef0(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d5df58: the argument is stored.
#[test]
fn orig_1c0b30_matches_the_game() {
    const S: Spec = spec(0x1c0b30, &[Arg::Int(&[])], Ret::None, &[(0x1d5df58, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1c0b30(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d5df5c: the argument is stored.
#[test]
fn orig_1c0b70_matches_the_game() {
    const S: Spec = spec(0x1c0b70, &[Arg::Int(&[])], Ret::None, &[(0x1d5df5c, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1c0b70(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d5df60: the argument is stored.
#[test]
fn orig_1c1400_matches_the_game() {
    const S: Spec = spec(0x1c1400, &[Arg::Int(&[])], Ret::None, &[(0x1d5df60, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1c1400(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d5e378: the argument is stored.
#[test]
fn orig_1c6cd0_matches_the_game() {
    const S: Spec = spec(0x1c6cd0, &[Arg::Int(&[])], Ret::None, &[(0x1d5e378, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1c6cd0(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d6a920: the argument is stored.
#[test]
fn orig_1d0110_matches_the_game() {
    const S: Spec = spec(0x1d0110, &[Arg::Int(&[])], Ret::None, &[(0x1d6a920, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1d0110(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d6a924: the argument is stored.
#[test]
fn orig_1d0248_matches_the_game() {
    const S: Spec = spec(0x1d0248, &[Arg::Int(&[])], Ret::None, &[(0x1d6a924, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1d0248(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x1d6a98c: the argument is stored.
#[test]
fn orig_1d3900_matches_the_game() {
    const S: Spec = spec(0x1d3900, &[Arg::Int(&[])], Ret::None, &[(0x1d6a98c, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1d3900(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The table word at 0x3608f8: the argument is stored.
#[test]
fn orig_1d5e38_matches_the_game() {
    const S: Spec = spec(0x1d5e38, &[Arg::Int(&[])], Ret::None, &[(0x3608f8, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(0) as u32;
        t::set_table_1d5e38(c.global(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

// ---- factory harness: the other leaves --------------------------------------------------------------------------------------------

/// The table address and the table's own count word: the end of the table is handed to the second setter.
#[test]
fn orig_1a66b8_matches_the_game() {
    const S: Spec = Spec {
        addr: 0x1a66b8,
        args: &[Arg::Ptr(blk(0x40, &[(4, 4, Kind::Int)]))],
        floats: 0,
        ret: Ret::None,
        globals: &[(0x1c95588, 4, true), (0x1d6a920, 4, true)],
        stubs: &[],
        nulls: false,
    };
    let Some(st) = fuzz(&S, CASES, |c| {
        let table = block_addr(c.ints[0] as usize);
        let count = rd32(c.block(0), 4);
        let (a, b) = t::set_table_1a66b8(table, count);
        c.global(0).copy_from_slice(&a.to_le_bytes());
        t::set_table_1d0110(c.global(1), b);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

#[test]
fn orig_13ee98_matches_the_game() {
    const S: Spec = spec(0x13ee98, &[], Ret::Int, &[(0x355830, 1, true)]);
    let Some(st) = fuzz(&S, CASES, |c| Out::int(t::byte_355830(c.global(0)) as i64)) else { return };
    report(st);
}

#[test]
fn orig_181440_matches_the_game() {
    const S: Spec = Spec { addr: 0x181440, args: &[], floats: 1, ret: Ret::None, globals: &[(0x1c893b8, 8, true)], stubs: &[], nulls: true };
    let Some(st) = fuzz(&S, CASES, |c| {
        let f = c.float(0);
        t::cost_add(c.global(0), f);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

#[test]
fn orig_181458_matches_the_game() {
    const S: Spec = spec(0x181458, &[], Ret::Float, &[(0x1c893b8, 8, true)]);
    let Some(st) = fuzz(&S, CASES, |c| Out::float(t::cost_left(c.global(0)))) else { return };
    report(st);
}

#[test]
fn orig_181470_matches_the_game() {
    const S: Spec = spec(0x181470, &[], Ret::Float, &[(0x1c893b8, 8, true)]);
    let Some(st) = fuzz(&S, CASES, |c| Out::float(t::cost_ratio(c.global(0)))) else { return };
    report(st);
}

#[test]
fn orig_181408_matches_the_game() {
    const S: Spec = Spec { addr: 0x181408, args: &[], floats: 1, ret: Ret::Int, globals: &[(0x1c893b8, 8, true)], stubs: &[], nulls: true };
    let mut yes = 0u32;
    let Some(st) = fuzz(&S, CASES, |c| {
        let f = c.float(0);
        let r = t::cost_can_add(c.global(0), f);
        yes += r as u32;
        Out::int(r as i64)
    }) else {
        return;
    };
    assert!(yes > 200 && yes < CASES - 200, "both outcomes are drawn: {yes} fit");
    report(st);
}

/// The area record reset: only the pair (1, 0xffff) acts.
#[test]
fn orig_181498_matches_the_game() {
    const S: Spec = spec(0x181498, &[Arg::Int(&[1, 0, 2]), Arg::Int(&[0xffff, 0, 1])], Ret::None, &[(0x1c893b0, 8, true)]);
    let mut acted = 0u32;
    let Some(st) = fuzz(&S, CASES, |c| {
        let (a, b) = (c.int(0), c.int(1) as i64);
        acted += (a == 1 && b == 0xffff) as u32;
        t::area_record_reset_if(c.global(0), a, b);
        Out::none()
    }) else {
        return;
    };
    assert!(acted > 10, "the acting pair is drawn: {acted}");
    report(st);
}

#[test]
fn orig_1814d8_matches_the_game() {
    const S: Spec = spec(0x1814d8, &[], Ret::None, &[(0x1c893b0, 8, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        t::area_record_reset(c.global(0));
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The spawn record stack: the count word at 0x35cd48 (small values and wild ones).
#[test]
fn orig_1814f8_matches_the_game() {
    const S: Spec = spec(0x1814f8, &[], Ret::Int, &[(0x35cd48, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| Out::int(t::spawn_record_push(c.global(0)) as i32 as i64)) else { return };
    report(st);
}

#[test]
fn orig_181520_matches_the_game() {
    const S: Spec = spec(0x181520, &[], Ret::None, &[(0x35cd48, 4, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        t::spawn_record_pop(c.global(0));
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The fourth argument is a char: only its low byte counts (0x100 has a clear low byte).
#[test]
fn orig_181560_matches_the_game() {
    const S: Spec = spec(0x181560, &[Arg::Ptr(blk(0x10, &[])), Arg::Int(&[]), Arg::Int(&[]), Arg::Int(&[0, 1, 0x100])], Ret::None, &[]);
    let (mut yes, mut no) = (0u32, 0u32);
    let Some(st) = fuzz(&S, CASES, |c| {
        let (a, b, f) = (c.int(1) as u32, c.int(2) as u32, c.int(3) as u8 != 0);
        if f {
            yes += 1
        } else {
            no += 1
        }
        t::spawn_record_for_group(c.block(0), a, b, f);
        Out::none()
    }) else {
        return;
    };
    assert!(yes > 200 && no > 200, "both flag values: {yes} set, {no} clear");
    report(st);
}

#[test]
fn orig_1815b0_matches_the_game() {
    const S: Spec = spec(0x1815b0, &[Arg::Ptr(blk(0x10, &[])), Arg::Int(&[]), Arg::Int(&[]), Arg::Int(&[0, 1, 0x100])], Ret::None, &[]);
    let (mut yes, mut no) = (0u32, 0u32);
    let Some(st) = fuzz(&S, CASES, |c| {
        let (a, b, f) = (c.int(1) as u32, c.int(2) as u32, c.int(3) as u8 != 0);
        if f {
            yes += 1
        } else {
            no += 1
        }
        t::spawn_record_for_event(c.block(0), a, b, f);
        Out::none()
    }) else {
        return;
    };
    assert!(yes > 200 && no > 200, "both flag values: {yes} set, {no} clear");
    report(st);
}

/// Bits 0..8191 of a 0x400-byte array.
#[test]
fn orig_19de20_matches_the_game() {
    const S: Spec = spec(0x19de20, &[Arg::Ptr(blk(0x400, &[])), Arg::Range(0, 8191)], Ret::Int, &[]);
    let mut set = 0u32;
    let Some(st) = fuzz(&S, CASES, |c| {
        let bit = c.int(1) as u32;
        let b = c.block(0).to_vec();
        let r = t::bit_set(|i| rd32(&b, 4 * i as usize), bit);
        set += r as u32;
        Out::int(r as i64)
    }) else {
        return;
    };
    assert!(set > 1000 && set < CASES - 1000, "both outcomes are drawn: {set} set");
    report(st);
}

/// The same behind a four-byte header: bits 0..8158.
#[test]
fn orig_19de48_matches_the_game() {
    const S: Spec = spec(0x19de48, &[Arg::Ptr(blk(0x400, &[])), Arg::Range(0, 8158)], Ret::Int, &[]);
    let mut set = 0u32;
    let Some(st) = fuzz(&S, CASES, |c| {
        let bit = c.int(1) as u32;
        let b = c.block(0).to_vec();
        let r = t::bit_set_after_header(|i| rd32(&b, 4 * i as usize), bit);
        set += r as u32;
        Out::int(r as i64)
    }) else {
        return;
    };
    assert!(set > 1000 && set < CASES - 1000, "both outcomes are drawn: {set} set");
    report(st);
}


// ---- hand harnesses ----------------------------------------------------------------------------------------------------------------

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_game_core::area::ctl::AreaHost;

/// The shared function table of the virtual calls (slot n at VT + 4n) and the hook addresses behind it (HV + 0x10 n).
const VT: u32 = 0x0188_0000;
const HV: u32 = 0x0189_0000;
const VSLOTS: u32 = 16;
/// The address both sides show for a pointer into the original's stack (the port's scratch buffer).
const SCRATCH: u32 = 0x01ff_f000;
const VTAG: u32 = 0x8000_0000;

fn mix(a: u64, b: u64, c: u64) -> u64 {
    let mut x = a ^ b.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ c.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 29)
}

type Ev = (u32, Vec<u32>);

/// Windows of memory laid on both sides: random bytes (or what the test puts), compared byte for byte after the case. A scratch window
/// is only the port's (the original's stack stands in for it) and is not compared.
#[derive(Clone)]
struct Mem {
    regs: Vec<(u32, Vec<u8>, bool)>,
}

impl Mem {
    fn new() -> Mem {
        Mem { regs: Vec::new() }
    }
    fn win(&mut self, r: &mut Rng, a: u32, n: u32) -> &mut Mem {
        let v = (0..n).map(|_| r.word() as u8).collect();
        self.regs.push((a, v, true));
        self
    }
    fn fixed(&mut self, a: u32, bytes: Vec<u8>) -> &mut Mem {
        self.regs.push((a, bytes, true));
        self
    }
    fn scratch(&mut self, a: u32, n: u32) -> &mut Mem {
        self.regs.push((a, vec![0; n as usize], false));
        self
    }
    fn slot(&mut self, a: u32, n: usize) -> &mut [u8] {
        for (base, d, _) in self.regs.iter_mut() {
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
    fn put(&mut self, a: u32, d: &[u8]) {
        self.slot(a, d.len()).copy_from_slice(d)
    }
    fn get(&mut self, a: u32, n: usize) -> Vec<u8> {
        self.slot(a, n).to_vec()
    }
}

/// How the stand-ins behave: which addresses are hooked (and how many argument words each logs), how many words each virtual method
/// logs after the object, what they return, and which logged arguments are pointers whose target words are logged instead.
struct Cfg {
    standins: Vec<(u32, usize)>,
    vargs: Vec<usize>,
    /// (tag, k, arguments, hash) -> v0. The tag is the callee address, or `VTAG | slot` for a virtual call.
    ret: Box<dyn Fn(u32, u32, &[u64], u64) -> u64>,
    /// (tag, argument index, words): that argument is logged as the words it points at.
    deref: Vec<(u32, usize, usize)>,
    /// Stand-ins whose last logged word is the float register f12 (the port passes it as bits after the register arguments).
    floats: Vec<u32>,
    /// Tags whose argument count varies between calls (a virtual method that some callers give one more argument): the original's log
    /// is cut to the port's count for them.
    flex: Vec<u32>,
}

impl Cfg {
    fn new(standins: &[(u32, usize)], ret: impl Fn(u32, u32, &[u64], u64) -> u64 + 'static) -> Cfg {
        Cfg { standins: standins.to_vec(), vargs: vec![0; VSLOTS as usize], ret: Box::new(ret), deref: Vec::new(), floats: Vec::new(), flex: Vec::new() }
    }
    fn v(mut self, slot: usize, extra: usize) -> Cfg {
        self.vargs[slot] = extra;
        self
    }
    fn flex(mut self, tag: u32) -> Cfg {
        self.flex.push(tag);
        self
    }
    fn deref(mut self, tag: u32, arg: usize, words: usize) -> Cfg {
        self.deref.push((tag, arg, words));
        self
    }
    fn arity(&self, tag: u32) -> usize {
        if tag & VTAG != 0 {
            1 + self.vargs[(tag & 0xff) as usize]
        } else {
            self.standins.iter().find(|s| s.0 == tag).unwrap_or_else(|| panic!("the port called {tag:#x}, which the test does not list as a stand-in")).1
        }
    }
}

struct Shared {
    log: Vec<Ev>,
    counts: HashMap<u32, u32>,
    seed: u64,
}

fn norm(v: u32) -> u32 {
    if (STACK - 0x2000..=STACK).contains(&v) { SCRATCH } else { v }
}

/// The port's side: memory windows, a call log and the same return policy.
struct Port {
    mem: Mem,
    log: Vec<Ev>,
    counts: HashMap<u32, u32>,
    seed: u64,
    cfg: Rc<Cfg>,
}

impl Port {
    fn event(&mut self, tag: u32, args: &[u64]) -> u64 {
        let n = self.cfg.arity(tag);
        if self.cfg.flex.contains(&tag) {
            assert!(args.len() <= n, "the port passes {} words to {tag:#x}, the test allows {n}", args.len());
        } else {
            assert_eq!(args.len(), n, "the port passes {} words to {tag:#x}, the test expects {n}", args.len());
        }
        let mut ev: Vec<u32> = args.iter().map(|a| norm(*a as u32)).collect();
        for &(t, i, w) in &self.cfg.deref {
            if t == tag {
                let p = ev[i];
                let words: Vec<u32> = (0..w as u32).map(|k| self.mem.r32(p + 4 * k)).collect();
                ev.splice(i..=i, words);
            }
        }
        self.log.push((tag, ev));
        let k = {
            let c = self.counts.entry(tag).or_insert(0);
            let k = *c;
            *c += 1;
            k
        };
        (self.cfg.ret)(tag, k, args, mix(self.seed, tag as u64, k as u64))
    }
}

impl AreaHost for Port {
    fn call(&mut self, callee: u32, args: &[u64]) -> u64 {
        self.event(callee, args)
    }
    fn vcall(&mut self, obj: u32, slot: u32, args: &[u64]) -> u64 {
        let a: Vec<u64> = std::iter::once(obj as u64).chain(args.iter().copied()).collect();
        self.event(VTAG | slot, &a)
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

/// kh2-ee with the stand-ins installed.
struct Dual {
    ee: Ee,
    sh: Rc<RefCell<Shared>>,
    cfg: Rc<Cfg>,
    what: &'static str,
    cases: u64,
}

impl Dual {
    fn new(what: &'static str, cfg: Cfg) -> Option<Dual> {
        let elf = kh2_factory::exe_or_skip(what)?;
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let cfg = Rc::new(cfg);
        let sh = Rc::new(RefCell::new(Shared { log: Vec::new(), counts: HashMap::new(), seed: 0 }));
        for &(addr, n) in &cfg.standins {
            let (s, c) = (sh.clone(), cfg.clone());
            ee.hooks.insert(addr, move |ee: &mut Ee| {
                let full: Vec<u64> = (4..10).map(|i| ee.gpr[i] as u64).collect();
                let mut args = full[..n].to_vec();
                if c.floats.contains(&addr) {
                    args[n - 1] = ee.fpr[12].to_bits() as u64;
                }
                Dual::hook(&s, &c, ee, addr, args);
            });
        }
        for slot in 0..VSLOTS {
            let (s, c) = (sh.clone(), cfg.clone());
            ee.w32(VT + 4 * slot, HV + 0x10 * slot);
            ee.hooks.insert(HV + 0x10 * slot, move |ee: &mut Ee| {
                let n = c.arity(VTAG | slot);
                let args: Vec<u64> = (4..4 + n).map(|i| ee.gpr[i] as u64).collect();
                Dual::hook(&s, &c, ee, VTAG | slot, args);
            });
        }
        Some(Dual { ee, sh, cfg, what, cases: 0 })
    }

    fn hook(s: &Rc<RefCell<Shared>>, c: &Rc<Cfg>, ee: &mut Ee, tag: u32, args: Vec<u64>) {
        let mut ev: Vec<u32> = args.iter().map(|a| norm(*a as u32)).collect();
        for &(t, i, w) in &c.deref {
            if t == tag {
                let p = args[i] as u32;
                let words: Vec<u32> = (0..w as u32).map(|k| ee.r32(p + 4 * k)).collect();
                ev.splice(i..=i, words);
            }
        }
        let mut s = s.borrow_mut();
        s.log.push((tag, ev));
        let k = {
            let e = s.counts.entry(tag).or_insert(0);
            let k = *e;
            *e += 1;
            k
        };
        let h = mix(s.seed, tag as u64, k as u64);
        ee.gpr[2] = (c.ret)(tag, k, &args, h) as u128;
    }

    /// Runs one case on both sides from the memory `mem`; returns the two results after comparing the call logs and every window.
    fn run<R>(&mut self, mem: &Mem, seed: u64, orig: impl FnOnce(&mut Ee) -> R, port: impl FnOnce(&mut Port) -> R) -> (R, R) {
        self.cases += 1;
        {
            let mut s = self.sh.borrow_mut();
            s.log.clear();
            s.counts.clear();
            s.seed = seed;
        }
        for (a, d, _) in &mem.regs {
            self.ee.write(*a, d);
        }
        let game = orig(&mut self.ee);
        let mut p = Port { mem: mem.clone(), log: Vec::new(), counts: HashMap::new(), seed, cfg: self.cfg.clone() };
        let mine = port(&mut p);
        let mut glog = self.sh.borrow().log.clone();
        for (g, m) in glog.iter_mut().zip(&p.log) {
            if g.0 == m.0 && self.cfg.flex.contains(&g.0) && m.1.len() < g.1.len() {
                g.1.truncate(m.1.len());
            }
        }
        if glog != p.log {
            let at = (0..glog.len().max(p.log.len())).find(|&i| glog.get(i) != p.log.get(i)).unwrap();
            panic!(
                "{} case {}: call {at} differs (game has {} calls, port {}): game {:x?}, port {:x?}",
                self.what,
                self.cases,
                glog.len(),
                p.log.len(),
                glog.get(at),
                p.log.get(at)
            );
        }
        for (base, d, cmp) in &p.mem.regs {
            if !*cmp {
                continue;
            }
            for (i, b) in d.iter().enumerate() {
                let g = self.ee.r8(base + i as u32);
                assert_eq!(g, *b, "{} case {}: window {base:#x} byte +{i:#x}: game {g:02x} port {b:02x}", self.what, self.cases);
            }
        }
        (game, mine)
    }

    fn call(&mut self, addr: u32, args: &[u64]) -> u64 {
        let what = self.what;
        self.ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("{what}: the original at {addr:#x} faulted: {f:?}"))
    }
}

fn call(ee: &mut Ee, addr: u32, args: &[u64]) -> u64 {
    ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
}

fn sx(v: u32) -> u64 {
    v as i32 as i64 as u64
}

/// Writes a vtable pointer into the object at `a` in a memory image.
fn vt(m: &mut Mem, a: u32) {
    m.w32(a, VT);
}


// ---- constants the originals read, copied once from the executable -----------------------------------------------------------------

/// Bytes of the executable's data as the interpreter holds them after `load_elf` (the name words and constants the loaders read).
#[allow(dead_code)]
fn exe_bytes(ee: &mut Ee, a: u32, n: u32) -> Vec<u8> {
    (0..n).map(|i| ee.r8(a + i)).collect()
}

// ---- tests ---------------------------------------------------------------------------------------------------------------------------

const PLACE: u32 = 0x0181_0000;

/// A flag-free constant test of three leaves that write a constant: the word starts random and ends as the original leaves it.
fn constant_writer(addr: u32, what: &'static str, f: impl Fn(&mut [u8])) {
    let Some(mut d) = Dual::new(what, Cfg::new(&[], |_, _, _, h| h)) else { return };
    let mut r = Rng::new(addr as u64);
    for _ in 0..200 {
        let mut m = Mem::new();
        m.win(&mut r, 0x35ee98, 8);
        d.run(
            &m,
            0,
            |ee| call(ee, addr, &[]),
            |p| {
                let mut w = p.mem.get(0x35ee98, 4);
                f(&mut w);
                p.mem.put(0x35ee98, &w);
                0
            },
        );
    }
}

#[test]
fn orig_1af928_matches_the_game() {
    constant_writer(0x1af928, "orig_1af928", t::point_35ee98_at_block);
}

#[test]
fn orig_1af940_matches_the_game() {
    constant_writer(0x1af940, "orig_1af940", t::clear_35ee98);
}

/// The volume test of shape 2 answers 1 whatever the registers hold.
#[test]
fn orig_334388_matches_the_game() {
    let Some(mut d) = Dual::new("orig_334388", Cfg::new(&[], |_, _, _, h| h)) else { return };
    let mut r = Rng::new(0x334388);
    for _ in 0..200 {
        let args = [r.word(), r.word(), r.word(), r.word()];
        let got = d.call(0x334388, &args) as u32;
        assert_eq!(got, kh2_ard::run::shape_always_inside());
    }
}

/// The area spawn file load: freed buffer, file name, size, rounded buffer, read request, then the place record. Stand-ins: 0x16ff18,
/// 0x105010, 0x311c08, 0x168250, 0x16fe70 and 0x167e58 (they log their arguments; the one pointer into the original's stack, the name
/// buffer, shows as one fixed address).
#[test]
fn orig_180870_matches_the_game() {
    let cfg = Cfg::new(&[(0x16ff18, 1), (0x105010, 2), (0x311c08, 3), (0x168250, 1), (0x16fe70, 1), (0x167e58, 4)], |tag, _, _, h| match tag {
        0x105010 => 0x0183_0000,
        0x168250 => h & 0x3fff,
        0x16fe70 => 0x0184_0000 + (h & 0xf) * 0x1000,
        _ => h,
    });
    let Some(mut d) = Dual::new("orig_180870", cfg) else { return };
    let mut r = Rng::new(0x180870);
    let (mut same, mut freed, mut fresh) = (0, 0, 0);
    for n in 0..1500u64 {
        let mut m = Mem::new();
        m.win(&mut r, 0x1c893b0, 8).win(&mut r, 0x35cd44, 4).win(&mut r, PLACE, 8);
        if r.below(3) == 0 {
            m.w32(0x35cd44, 0);
        }
        // The loaded place and the new one share world and area half of the time (the other bytes differ).
        let (w, a) = (m.r8(PLACE), m.r8(PLACE + 1));
        match r.below(4) {
            0 | 1 => {
                m.w8(0x1c893b0, w);
                m.w8(0x1c893b1, a);
            }
            2 => m.w8(0x1c893b0, w),
            _ => {}
        }
        let cached = m.r32(0x35cd44) != 0;
        let is_same = m.r8(0x1c893b0) == m.r8(PLACE) && m.r8(0x1c893b1) == m.r8(PLACE + 1);
        if is_same {
            same += 1;
        } else {
            fresh += 1;
            freed += cached as u32;
        }
        d.run(&m, n, |ee| call(ee, 0x180870, &[PLACE as u64]), |p| {
            t::area_bar_load(p, PLACE, SCRATCH);
            0
        });
    }
    assert!(same > 200 && fresh > 200 && freed > 100, "branches: {same} same, {fresh} loads, {freed} with a cache to free");
}

const BAR: u32 = 0x0181_0000;

/// Lays a BAR image at `BAR`: count, then entries (type, flags, name, payload word).
fn put_bar(m: &mut Mem, entries: &[(u16, u16, u32, u32)]) {
    m.w32(BAR + 4, entries.len() as u32);
    for (k, e) in entries.iter().enumerate() {
        let at = BAR + 0x10 + 0x10 * k as u32;
        m.w16(at, e.0);
        m.w16(at + 2, e.1);
        m.w32(at + 4, e.2);
        m.w32(at + 8, e.3);
        m.w32(at + 12, 0x100);
    }
}

/// The payload of the first type-0xc entry with a name: the name is the full register (a sign-extended word or a stranger).
#[test]
fn orig_180950_matches_the_game() {
    let Some(mut d) = Dual::new("orig_180950", Cfg::new(&[], |_, _, _, h| h)) else { return };
    let mut r = Rng::new(0x180950);
    let pool = [0x0000_0042u32, 0x8000_0001, 0xffff_ff80, 0x1234_5678];
    let (mut found, mut missed, mut wrong_type) = (0, 0, 0);
    for n in 0..2000u64 {
        let mut m = Mem::new();
        m.win(&mut r, 0x35cd44, 4).win(&mut r, BAR, 0x200);
        m.w32(0x35cd44, BAR);
        let count = r.below(9) as usize;
        let entries: Vec<(u16, u16, u32, u32)> = (0..count)
            .map(|_| ([0xc, 0xc, 1, 2, 0xc][r.below(5) as usize], r.word() as u16, pool[r.below(4) as usize], r.word() as u32))
            .collect();
        put_bar(&mut m, &entries);
        let name: u64 = if r.below(5) == 0 { pool[r.below(4) as usize] as u64 } else { sx(pool[r.below(4) as usize]) };
        let want = entries.iter().find(|e| e.0 == 0xc && sx(e.2) == name);
        match want {
            Some(_) => found += 1,
            None => {
                missed += 1;
                wrong_type += entries.iter().any(|e| sx(e.2) == name) as u32;
            }
        }
        let (g, p) = d.run(&m, n, |ee| call(ee, 0x180950, &[name]) as u32, |p| t::area_bar_lookup(p, name));
        assert_eq!(g, p, "case {n}: return value");
        assert_eq!(g, want.map_or(0, |e| e.3));
    }
    assert!(found > 400 && missed > 400 && wrong_type > 50, "outcomes: {found} found, {missed} missed, {wrong_type} with a name match of another type");
}

/// Both walks over a BAR through the host (`bar_nth_of_type` for 0x1684c0 and `bar_nth_of_name` for 0x168510) against the originals,
/// with every position n, any type and a negative count.
#[test]
fn orig_1684c0_matches_the_game() {
    let Some(mut d) = Dual::new("orig_1684c0", Cfg::new(&[], |_, _, _, h| h)) else { return };
    let mut r = Rng::new(0x1684c0);
    for n in 0..2000u64 {
        let mut m = Mem::new();
        m.win(&mut r, BAR, 0x200);
        let count = if r.below(30) == 0 { (-(r.below(4) as i32)) as u32 } else { r.below(12) as u32 };
        m.w32(BAR + 4, count);
        for k in 0..12u32 {
            m.w16(BAR + 0x10 + 0x10 * k, [1u16, 2, 3, 0x17][r.below(4) as usize]);
        }
        let ty: i64 = if r.below(4) == 0 { -1 } else { [1i64, 2, 3, 0x17, 9][r.below(5) as usize] };
        let idx = r.below(4) as i32;
        let (g, p) = d.run(&m, n, |ee| call(ee, 0x1684c0, &[BAR as u64, ty as u64, idx as u64]) as u32, |p| t::bar_nth_of_type(p, BAR, ty, idx));
        assert_eq!(g, p, "case {n}: entry address");
    }
}

#[test]
fn orig_168510_matches_the_game() {
    let Some(mut d) = Dual::new("orig_168510", Cfg::new(&[], |_, _, _, h| h)) else { return };
    let mut r = Rng::new(0x168510);
    let pool = [0x0000_0042u32, 0x8000_0001, 0xffff_ff80, 0x1234_5678];
    for n in 0..2000u64 {
        let mut m = Mem::new();
        m.win(&mut r, BAR, 0x200);
        let count = if r.below(30) == 0 { (-(r.below(4) as i32)) as u32 } else { r.below(12) as u32 };
        m.w32(BAR + 4, count);
        for k in 0..12u32 {
            m.w16(BAR + 0x10 + 0x10 * k, [1u16, 2, 3, 0x17][r.below(4) as usize]);
            m.w32(BAR + 0x14 + 0x10 * k, pool[r.below(4) as usize]);
        }
        let ty: i64 = if r.below(4) == 0 { -1 } else { [1i64, 2, 3, 0x17, 9][r.below(5) as usize] };
        let name: u64 = if r.below(5) == 0 { pool[r.below(4) as usize] as u64 } else { sx(pool[r.below(4) as usize]) };
        let idx = r.below(3) as i32;
        let (g, p) = d.run(&m, n, |ee| call(ee, 0x168510, &[BAR as u64, ty as u64, name, idx as u64]) as u32, |p| t::bar_nth_of_name(p, BAR, ty, name, idx));
        assert_eq!(g, p, "case {n}: entry address");
    }
}

const REC: u32 = 0x0181_0000;
const HDR: u32 = 0x0182_0000;
const MEMBERS: u32 = 0x0183_0000;
const OBJS: u32 = 0x0184_0000;

/// The spawn finish: objects the list walker offers one by one, a group of member objects that accept a point or not, the id word.
/// Stand-ins: 0x184d98 (hands out the case's object list), 0x16b960 (the position at obj +0x70), 0x16b928, 0x197b70 and virtual
/// method 2 of the members (answers by hash; its pointer argument is logged as the four words behind it).
#[test]
fn orig_199b18_matches_the_game() {
    let order: Rc<RefCell<Vec<u32>>> = Rc::new(RefCell::new(Vec::new()));
    let o2 = order.clone();
    let cfg = Cfg::new(&[(0x184d98, 1), (0x16b960, 1), (0x16b928, 1), (0x197b70, 2)], move |tag, _, args, h| match tag {
        0x184d98 => {
            let ord = o2.borrow();
            let cur = args[0] as u32;
            if cur == 0 { ord.first().copied().unwrap_or(0) as u64 } else { ord.iter().position(|&x| x == cur).and_then(|i| ord.get(i + 1)).copied().unwrap_or(0) as u64 }
        }
        0x16b960 => (args[0] as u32 + 0x70) as u64,
        tg if tg == VTAG | 2 => (h >> 5) & 1,
        _ => h,
    })
    .v(2, 1)
    .deref(VTAG | 2, 1, 4);
    let Some(mut d) = Dual::new("orig_199b18", cfg) else { return };
    let mut r = Rng::new(0x199b18);
    let (mut updates, mut already, mut skipped) = (0, 0, 0);
    for n in 0..800u64 {
        let mut m = Mem::new();
        m.win(&mut r, REC, 0x40).win(&mut r, HDR, 0x40).scratch(SCRATCH, 0x20);
        let nobj = 1 + r.below(3) as u32;
        let nmem = r.below(5) as u32;
        for k in 0..nobj {
            m.win(&mut r, OBJS + k * 0x1000, 0xb00);
        }
        for k in 0..nmem.max(1) {
            m.win(&mut r, MEMBERS + k * 0x100, 0x100);
        }
        m.w32(REC + 8, HDR);
        m.w32(REC + 0xc, if nmem == 0 { 0 } else { MEMBERS });
        for k in 0..nmem {
            vt(&mut m, MEMBERS + k * 0x100);
            let next = if k + 1 < nmem { MEMBERS + (k + 1) * 0x100 } else { 0 };
            m.w32(MEMBERS + k * 0x100 + 0x60, next);
        }
        let id = m.r16(HDR + 2) as u32;
        let objs: Vec<u32> = (0..nobj).map(|k| OBJS + k * 0x1000).collect();
        for &o in &objs {
            // The object's id word: the group's (with or without bit 16), or a stranger.
            match r.below(3) {
                0 => m.w32(o + 0xaa8, id),
                1 => m.w32(o + 0xaa8, id | 0x1_0000),
                _ => {}
            }
        }
        *order.borrow_mut() = objs;
        let (_, _) = d.run(&m, n, |ee| call(ee, 0x199b18, &[REC as u64]), |p| {
            t::spawn_finish(p, REC, SCRATCH);
            0
        });
        let log = d.sh.borrow().log.clone();
        updates += log.iter().filter(|e| e.0 == 0x197b70).count();
        already += log.iter().filter(|e| e.0 == VTAG | 2).count();
        skipped += (nmem == 0) as u32;
    }
    assert!(updates > 100 && already > 500 && skipped > 50, "branches: {updates} id updates, {already} method calls, {skipped} empty groups");
}

/// The destroy walk: method 1 of each object of the list, next link read first, and the byte cleared.
#[test]
fn orig_18c448_matches_the_game() {
    let cfg = Cfg::new(&[], |_, _, _, h| h).v(1, 0);
    let Some(mut d) = Dual::new("orig_18c448", cfg) else { return };
    let mut r = Rng::new(0x18c448);
    for n in 0..500u64 {
        let mut m = Mem::new();
        m.win(&mut r, 0x1c89b58, 4).win(&mut r, 0x1c89b60, 1);
        let count = r.below(6) as u32;
        for k in 0..count.max(1) {
            m.win(&mut r, OBJS + k * 0x100, 0x100);
        }
        m.w32(0x1c89b58, if count == 0 { 0 } else { OBJS });
        for k in 0..count {
            vt(&mut m, OBJS + k * 0x100);
            m.w32(OBJS + k * 0x100 + 0xc0, if k + 1 < count { OBJS + (k + 1) * 0x100 } else { 0 });
        }
        d.run(&m, n, |ee| call(ee, 0x18c448, &[]), |p| {
            t::destroy_list(p);
            0
        });
    }
}

const QA: u32 = 0x0181_0000;
const QB: u32 = 0x0182_0000;

/// The queue object: allocator method 0 (0x1340 bytes), the constructor 0x3307c8, the word, method 0 of the new block. Stand-in:
/// 0x3307c8; virtual method 0 returns the block for its first call.
#[test]
fn orig_15d9b0_matches_the_game() {
    let cfg = Cfg::new(&[(0x3307c8, 1)], |tag, k, _, h| if tag == VTAG && k == 0 { QB as u64 } else { h }).v(0, 1).flex(VTAG);
    let Some(mut d) = Dual::new("orig_15d9b0", cfg) else { return };
    let mut r = Rng::new(0x15d9b0);
    for n in 0..300u64 {
        let mut m = Mem::new();
        m.win(&mut r, 0x359368, 4).win(&mut r, 0x359330, 4).win(&mut r, QA, 0x100).win(&mut r, QB, 0x100);
        m.w32(0x359368, QA);
        vt(&mut m, QA);
        vt(&mut m, QB);
        d.run(&m, n, |ee| call(ee, 0x15d9b0, &[]), |p| {
            t::queue_create(p);
            0
        });
    }
}

/// The queue object is destroyed through method 3 when there is one; the word is cleared either way.
#[test]
fn orig_15daf0_matches_the_game() {
    let cfg = Cfg::new(&[], |_, _, _, h| h).v(3, 0);
    let Some(mut d) = Dual::new("orig_15daf0", cfg) else { return };
    let mut r = Rng::new(0x15daf0);
    for n in 0..300u64 {
        let mut m = Mem::new();
        m.win(&mut r, 0x359330, 4).win(&mut r, QA, 0x100);
        m.w32(0x359330, if r.below(2) == 0 { 0 } else { QA });
        vt(&mut m, QA);
        d.run(&m, n, |ee| call(ee, 0x15daf0, &[]), |p| {
            t::queue_destroy(p);
            0
        });
    }
}

/// The object entry table: first free of three words; full tables store at address 0 as the original does. The return value is the
/// argument.
#[test]
fn orig_1d13f8_matches_the_game() {
    let Some(mut d) = Dual::new("orig_1d13f8", Cfg::new(&[], |_, _, _, h| h)) else { return };
    let mut r = Rng::new(0x1d13f8);
    let (mut full, mut partial) = (0, 0);
    for n in 0..1000u64 {
        let mut m = Mem::new();
        m.win(&mut r, 0x1d6a930, 12).win(&mut r, 0, 0x100);
        for k in 0..3 {
            if r.below(2) == 0 {
                m.w32(0x1d6a930 + 4 * k, 0);
            }
        }
        let zeros = (0..3).filter(|&k| m.r32(0x1d6a930 + 4 * k) == 0).count();
        if zeros == 0 {
            full += 1;
        } else {
            partial += 1;
        }
        let v = r.word();
        let (g, p) = d.run(&m, n, |ee| call(ee, 0x1d13f8, &[v]), |p| t::objentry_register(p, v));
        assert_eq!(g, p, "case {n}: return value");
    }
    assert!(full > 50 && partial > 500, "table states: {full} full, {partial} with a free slot");
}

/// The objentry loader: reads the file into a data buffer (stand-in 0x1af998) and registers the buffer.
#[test]
fn orig_16e1e0_matches_the_game() {
    let Some(mut d) = Dual::new("orig_16e1e0", Cfg::new(&[(0x1af998, 1)], |_, _, _, h| h)) else { return };
    let mut r = Rng::new(0x16e1e0);
    for n in 0..500u64 {
        let mut m = Mem::new();
        m.win(&mut r, 0x35bd18, 4).win(&mut r, 0x1d6a930, 12).win(&mut r, 0, 0x100);
        for k in 0..3 {
            if r.below(2) == 0 {
                m.w32(0x1d6a930 + 4 * k, 0);
            }
        }
        let (g, p) = d.run(&m, n, |ee| call(ee, 0x16e1e0, &[]), |p| t::objentry_load(p));
        assert_eq!(g, p, "case {n}: return value");
    }
}


// ---- the data cache, the table loaders and the group records -------------------------------------------------------------------------

const DREC: u32 = 0x0181_0000;
const ENT: u32 = 0x0182_0000;

fn data_cache_cfg() -> Cfg {
    Cfg::new(
        &[(0x16ed08, 2), (0x16fd18, 2), (0x16ef80, 1), (0x16efd8, 1), (0x16ed50, 2), (0x16eda8, 3), (0x1a7940, 1), (0x16e208, 1), (0x1a7d48, 2), (0x1700e0, 0)],
        |tag, _, _, h| match tag {
            0x16ef80 | 0x16efd8 => (h >> 3) & 1,
            0x16eda8 => {
                if h & 1 == 0 {
                    0
                } else {
                    h
                }
            }
            0x16e208 => {
                if h & 3 == 0 {
                    0
                } else {
                    (ENT + 0x100) as u64
                }
            }
            0x1a7d48 => h & 1,
            _ => h,
        },
    )
}

fn data_cache_mem(r: &mut Rng) -> Mem {
    let mut m = Mem::new();
    m.win(r, DREC, 0x18).win(r, ENT, 0x100).win(r, ENT + 0x100, 0x100).win(r, 0, 0x100);
    m.w32(DREC + 8, [ENT, ENT + 0x100, 0][r.below(3) as usize]);
    m
}

/// The cache buffer allocation: the first buffer always, the second when the id passes neither test and the entry byte +0x48 has bit 0
/// clear, the third when the id fails the first test. Stand-ins: 0x16ed08, 0x16fd18, 0x16ef80, 0x16efd8, 0x16ed50, 0x16eda8.
#[test]
fn orig_1a7808_matches_the_game() {
    let Some(mut d) = Dual::new("orig_1a7808", data_cache_cfg()) else { return };
    let mut r = Rng::new(0x1a7808);
    let (mut b, mut c) = (0, 0);
    for n in 0..1500u64 {
        let m = data_cache_mem(&mut r);
        d.run(&m, n, |ee| call(ee, 0x1a7808, &[DREC as u64]), |p| {
            let mut rec = p.mem.get(DREC, 0x18);
            t::data_cache_fill(p, &mut rec);
            p.mem.put(DREC, &rec);
            0
        });
        let log = d.sh.borrow().log.clone();
        b += log.iter().any(|e| e.0 == 0x16ed50) as u32;
        c += log.iter().any(|e| e.0 == 0x16eda8) as u32;
    }
    assert!(b > 100 && c > 400, "branches: {b} second buffers, {c} third-buffer lookups");
}

/// Pointing the record at a model: free, entry lookup when none is given, room check (dump when full), then the allocation.
/// Stand-ins as above plus 0x1a7940, 0x16e208, 0x1a7d48 and 0x1700e0.
#[test]
fn orig_1a78c8_matches_the_game() {
    let Some(mut d) = Dual::new("orig_1a78c8", data_cache_cfg()) else { return };
    let mut r = Rng::new(0x1a78c8);
    let (mut looked_up, mut dumped) = (0, 0);
    for n in 0..1500u64 {
        let m = data_cache_mem(&mut r);
        let id = sx(r.word() as u32) >> if r.below(2) == 0 { 0 } else { 40 };
        let entry: u64 = if r.below(2) == 0 { 0 } else { [ENT as u64, (ENT + 0x100) as u64][r.below(2) as usize] };
        d.run(&m, n, |ee| call(ee, 0x1a78c8, &[DREC as u64, id, entry]), |p| {
            let mut rec = p.mem.get(DREC, 0x18);
            t::data_cache_set(p, DREC, &mut rec, id, entry);
            p.mem.put(DREC, &rec);
            0
        });
        let log = d.sh.borrow().log.clone();
        looked_up += log.iter().any(|e| e.0 == 0x16e208) as u32;
        dumped += log.iter().any(|e| e.0 == 0x1700e0) as u32;
    }
    assert!(looked_up > 300 && dumped > 300, "branches: {looked_up} entry lookups, {dumped} dumps");
}

const PA: u32 = 0x0182_0000;
const B1: u32 = 0x0181_0000;
const B2: u32 = 0x0181_1000;

/// The words the loader's setters write (address, bytes), the windows the test lays.
const LOADER_WINDOWS: &[(u32, u32)] = &[
    (0x35d270, 8),
    (0x1c89700, 8),
    (0x1c8a970, 0x10),
    (0x1c95588, 12),
    (0x1d5d640, 8),
    (0x1d5d6b4, 4),
    (0x1d5dd80, 4),
    (0x1d5df10, 4),
    (0x1d5df58, 0x10),
    (0x1d6a920, 0x70),
    (0x360768, 4),
    (0x3608f8, 4),
    (0, 0x100),
];

const NAME_REGION: (u32, u32) = (0x37aed8, 0xf4);

fn all_steps() -> Vec<t::TableStep> {
    let mut v = Vec::new();
    v.extend(t::SYSTEM_STEPS_A);
    v.extend(t::SYSTEM_STEPS_B);
    v.extend(t::BATTLE_STEPS);
    v
}

/// A BAR image of one file: the wanted entries (some missing, some preceded by a decoy of the same name and another type), noise
/// entries and, for the first file, the optional type-0x29 entry. Payloads point at blocks of the arena at `PA`.
fn build_bar(m: &mut Mem, r: &mut Rng, at: u32, steps: &[t::TableStep], names: &[u8], optional: bool, next_block: &mut u32) -> (usize, usize) {
    let mut entries: Vec<(u16, u16, u32, u32)> = Vec::new();
    let mut missing = 0;
    for s in steps {
        let o = (s.name_at - NAME_REGION.0) as usize;
        let name = t::name_from_bytes([names[o], names[o + 1], names[o + 2], names[o + 3]]) as u32;
        if r.below(16) == 0 {
            missing += 1;
            continue;
        }
        if r.below(4) == 0 {
            entries.push((if s.ty == 2 { 1 } else { 2 }, 0, name, PA + r.below(0x40) as u32 * 0x100));
        }
        entries.push((s.ty as u16, 0, name, PA + *next_block * 0x100));
        *next_block += 1;
    }
    if optional {
        match r.below(3) {
            0 => {}
            1 => entries.push((0x29, 0, r.word() as u32, 0)),
            _ => entries.push((0x29, 0, r.word() as u32, PA + 0x100)),
        }
    }
    for _ in 0..r.below(5) {
        entries.push((r.below(0x30) as u16, 0, r.word() as u32, r.word() as u32));
    }
    for i in (1..entries.len()).rev() {
        entries.swap(i, r.below(i as u64 + 1) as usize);
    }
    m.w32(at + 4, entries.len() as u32);
    for (k, e) in entries.iter().enumerate() {
        let a = at + 0x10 + 0x10 * k as u32;
        m.w16(a, e.0);
        m.w16(a + 2, e.1);
        m.w32(a + 4, e.2);
        m.w32(a + 8, e.3);
    }
    (entries.len(), missing)
}

/// The two table loaders. Stand-ins: 0x1af9e0 (hands out the two BAR images in turn), the setters that are other units' (0x1cc040,
/// 0x1b6f38, 0x1a6f48, 0x1a7dd8, 0x1d1118, 0x19e778, 0x1d8ca0, 0x1a41c8, 0x1adfb0, 0x1be4a8, 0x1be818, 0x1d1c70, 0x1d4c58, 0x1a4c68,
/// 0x1d3b80, 0x1d5910 with one argument, 0x265ac0 with none). The ported setters run for real in the original, so the globals they write
/// are compared. The name words are copied from the executable's data once.
#[test]
fn orig_193610_matches_the_game() {
    let mut standins = vec![(0x1af9e0, 1), (0x265ac0, 0)];
    for s in [0x1cc040, 0x1b6f38, 0x1a6f48, 0x1a7dd8, 0x1d1118, 0x19e778, 0x1d8ca0, 0x1a41c8, 0x1adfb0, 0x1be4a8, 0x1be818, 0x1d1c70, 0x1d4c58, 0x1a4c68, 0x1d3b80, 0x1d5910] {
        standins.push((s, 1));
    }
    let cfg = Cfg::new(&standins, |tag, k, _, h| if tag == 0x1af9e0 { [B1, B2][k as usize % 2] as u64 } else { h });
    let Some(mut d) = Dual::new("orig_193610", cfg) else { return };
    let names = exe_bytes(&mut d.ee, NAME_REGION.0, NAME_REGION.1);
    let mut r = Rng::new(0x193610);
    let (mut missing_total, mut optional_called, mut optional_idle, mut total) = (0, 0, 0, 0);
    for n in 0..600u64 {
        let mut m = Mem::new();
        for &(a, len) in LOADER_WINDOWS {
            m.win(&mut r, a, len);
        }
        m.fixed(NAME_REGION.0, names.clone());
        // An entry that is missing makes the loader read the word at address 8: it holds a valid payload address.
        m.w32(8, PA + 0x3f00);
        m.win(&mut r, B1, 0x400).win(&mut r, B2, 0x400).win(&mut r, PA, 0x4000);
        // Payload blocks: a count word at +4 and type words behind it, so that the table end and the type-2 lookup have something to read.
        for k in 0..0x40u32 {
            let a = PA + k * 0x100;
            m.w32(a + 4, r.below(6) as u32);
            for e in 0..6u32 {
                m.w16(a + 0x10 + 0x10 * e, [1u16, 2, 3, 2][r.below(4) as usize]);
            }
        }
        let steps = all_steps();
        let sys: Vec<t::TableStep> = steps[..13].to_vec();
        let battle: Vec<t::TableStep> = steps[13..].to_vec();
        let mut next = 0u32;
        let (c1, miss1) = build_bar(&mut m, &mut r, B1, &sys, &names, true, &mut next);
        let (c2, miss2) = build_bar(&mut m, &mut r, B2, &battle, &names, false, &mut next);
        assert!(c1 < 60 && c2 < 60);
        missing_total += miss1 + miss2;
        total += 1;
        d.run(&m, n, |ee| call(ee, 0x193610, &[]), |p| {
            t::load_system_tables(p);
            0
        });
        let log = d.sh.borrow().log.clone();
        optional_called += log.iter().any(|e| e.0 == 0x265ac0) as u32;
        optional_idle += !log.iter().any(|e| e.0 == 0x265ac0) as u32;
    }
    assert!(missing_total > 100 && optional_called > 100 && optional_idle > 100, "branches over {total}: {missing_total} missing entries, {optional_called} with the optional call, {optional_idle} without");
}

/// The BAR payload setter that also reads the BAR: the type-2 entry's payload word, or the word at address 8 when there is none.
#[test]
fn orig_1d3050_matches_the_game() {
    let Some(mut d) = Dual::new("orig_1d3050", Cfg::new(&[], |_, _, _, h| h)) else { return };
    let mut r = Rng::new(0x1d3050);
    let (mut found, mut none) = (0, 0);
    for n in 0..1000u64 {
        let mut m = Mem::new();
        m.win(&mut r, 0x360768, 4).win(&mut r, 0x1d6a988, 4).win(&mut r, 0, 0x100).win(&mut r, PA, 0x100);
        let count = r.below(6) as u32;
        m.w32(PA + 4, count);
        for e in 0..6u32 {
            m.w16(PA + 0x10 + 0x10 * e, [1u16, 2, 3, 1][r.below(4) as usize]);
        }
        if (0..count).any(|e| m.r16(PA + 0x10 + 0x10 * e) == 2) {
            found += 1;
        } else {
            none += 1;
        }
        d.run(&m, n, |ee| call(ee, 0x1d3050, &[PA as u64]), |p| {
            t::set_table_1d3050(p, PA);
            0
        });
    }
    assert!(found > 300 && none > 100, "outcomes: {found} with a type-2 entry, {none} without");
}

const KT: u32 = 0x0181_0000;

/// The keyed row search of the table behind the word 0x1d5df58: rows of 8 bytes sorted by their first word, searched with the game's
/// halving binary search (0x30ff40 with the comparator 0x3335c0, both run for real in the original).
#[test]
fn orig_1c0b40_matches_the_game() {
    let Some(mut d) = Dual::new("orig_1c0b40", Cfg::new(&[], |_, _, _, h| h)) else { return };
    let mut r = Rng::new(0x1c0b40);
    let (mut hit, mut miss) = (0, 0);
    for n in 0..3000u64 {
        let mut m = Mem::new();
        m.win(&mut r, 0x1d5df58, 4).win(&mut r, KT, 0x100);
        m.w32(0x1d5df58, KT);
        let count = r.below(20) as u32;
        m.w32(KT + 4, count);
        let mut ids: Vec<u32> = (0..count).map(|_| (r.below(40) as u32) * 3).collect();
        if r.below(8) != 0 {
            ids.sort();
        }
        for (k, id) in ids.iter().enumerate() {
            m.w32(KT + 8 + 8 * k as u32, *id);
        }
        let key: u32 = match r.below(5) {
            0 => r.word() as u32,
            1 | 2 if count > 0 => ids[r.below(count as u64) as usize],
            _ => (r.below(42) as u32) * 3 + r.below(2) as u32,
        };
        let (g, p) = d.run(&m, n, |ee| call(ee, 0x1c0b40, &[sx(key)]) as u32, |p| t::keyed_row(p, key));
        assert_eq!(g, p, "case {n}: row address");
        if g == 0 {
            miss += 1
        } else {
            hit += 1
        }
    }
    assert!(hit > 500 && miss > 500, "outcomes: {hit} found, {miss} missed");
}

const GRP: u32 = 0x01c8_af90;
const SREC: u32 = 0x01c8_9400;

fn group_case_mem(r: &mut Rng) -> (Mem, u32, u32) {
    let mut m = Mem::new();
    m.win(r, SREC, 0x40).win(r, GRP, 64 * 0x38).win(r, 0x35e468, 4);
    let nobj = r.below(4) as u32;
    for k in 0..nobj.max(1) {
        m.win(r, OBJS + k * 0x100, 0x100);
    }
    let idx = r.below(64) as u32;
    let group = if r.below(10) == 0 { 0 } else { GRP + idx * 0x38 };
    let rec = SREC + 12 * r.below(4) as u32;
    m.w32(rec + 8, group);
    if group != 0 {
        m.w32(group + 0xc, if nobj == 0 { 0 } else { OBJS });
        for k in 0..nobj {
            vt(&mut m, OBJS + k * 0x100);
            m.w32(OBJS + k * 0x100 + 0x60, if k + 1 < nobj { OBJS + (k + 1) * 0x100 } else { 0 });
        }
    }
    if r.below(2) == 0 {
        let f = m.r32(rec + 4) & !1;
        m.w32(rec + 4, f);
    }
    (m, rec, group)
}

/// Releasing a spawn record: when its flag bit 0 is clear and it holds a group, the group's objects are dismissed (method 1 of each)
/// and the group slot counter drops. Stand-in: virtual method 1 of the group's objects.
#[test]
fn orig_181630_matches_the_game() {
    let Some(mut d) = Dual::new("orig_181630", Cfg::new(&[], |_, _, _, h| h).v(1, 0)) else { return };
    let mut r = Rng::new(0x181630);
    let (mut released, mut idle) = (0, 0);
    for n in 0..1500u64 {
        let (m, rec, group) = group_case_mem(&mut r);
        let mut m = m;
        let live = m.r32(rec + 4) & 1 == 0 && group != 0;
        if live {
            released += 1
        } else {
            idle += 1
        }
        d.run(&m, n, |ee| call(ee, 0x181630, &[rec as u64]), |p| {
            use kh2_game_core::area::ctl::{AreaCtl, GroupRec, GroupTable};
            let bytes = p.mem.get(rec, 12);
            let mut groups = GroupTable { recs: (0..64).map(|i| GroupRec(p.mem.get(GRP + i * 0x38, 0x38).try_into().unwrap())).collect() };
            let mut ctl = AreaCtl { group_slots: p.mem.r32(0x35e468), ..AreaCtl::default() };
            t::spawn_record_release(&bytes, &mut groups, &mut ctl, p);
            for (i, g) in groups.recs.iter().enumerate() {
                p.mem.put(GRP + i as u32 * 0x38, &g.0);
            }
            p.mem.w32(0x35e468, ctl.group_slots);
            0
        });
    }
    assert!(released > 400 && idle > 400, "outcomes: {released} released, {idle} left alone");
}

/// The per-group pass of a live spawn record. Stand-in: 0x1986c8.
#[test]
fn orig_181688_matches_the_game() {
    let Some(mut d) = Dual::new("orig_181688", Cfg::new(&[(0x1986c8, 1)], |_, _, _, h| h)) else { return };
    let mut r = Rng::new(0x181688);
    for n in 0..500u64 {
        let (m, rec, _) = group_case_mem(&mut r);
        d.run(&m, n, |ee| call(ee, 0x181688, &[rec as u64]), |p| {
            let bytes = p.mem.get(rec, 12);
            t::spawn_record_pass(&bytes, p);
            0
        });
    }
}

