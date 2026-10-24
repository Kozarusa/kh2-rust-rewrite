//! Oracle tests for the core leaves between 0x102e00 and 0x14afc8 (`kh2_game_core::leaves_a`): kh2-ee runs the original from the
//! user's executable and the port on the same random globals; the return value and every global byte are compared.
//!
//! Getters and setters of one global use the `factory gen` harness (`fuzz` with at least 1,000 cases, the global randomised before
//! each case). The table lookup at 0x377e50 reads the executable's own table (not randomised) and a second test reads random
//! tables. The allocator thunks and the game-mode setter call through memory, so they have hand harnesses below that hook the
//! indirect callee (the callee is another unit) and compare the call it receives.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_factory::fuzz::*;
use kh2_game_core::leaves_a as l;

const CASES: u32 = 2000;

fn report(st: Stats) {
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// A getter of one global: no arguments, the global randomised, the result compared with the port's.
macro_rules! getter {
    ($test:ident, $addr:expr, $global:expr, $len:expr, $port:expr) => {
        #[test]
        fn $test() {
            const SPEC: Spec = Spec {
                addr: $addr,
                args: &[],
                floats: 0,
                ret: Ret::Int,
                globals: &[($global, $len, true)],
                stubs: &[],
                nulls: true,
            };
            let Some(st) = fuzz(&SPEC, CASES, |c| Out::int(($port)(c.global(0)) as i64)) else { return };
            report(st);
        }
    };
}

/// A function returning a constant address (no state read): the original, called with random argument registers that it must
/// ignore, returns the port's constant. (`fuzz` refuses a case set with one constant result, so this is a hand test.)
macro_rules! constant {
    ($test:ident, $addr:expr, $port:expr) => {
        #[test]
        fn $test() {
            let Some(mut ee) = world(stringify!($test)) else { return };
            let mut r = Rng::new($addr);
            for _ in 0..300 {
                let got = ee.call($addr, &[r.word(), r.word()], &[], STACK).expect("original") as u32;
                assert_eq!(got, $port);
            }
        }
    };
}

/// A setter of one byte: the byte is randomised, the original runs, and the byte must equal what the port leaves.
macro_rules! setter {
    ($test:ident, $addr:expr, $at:expr, $port:expr) => {
        #[test]
        fn $test() {
            let Some(mut ee) = world(stringify!($test)) else { return };
            let mut r = Rng::new($addr);
            let mut seen = std::collections::HashSet::new();
            for _ in 0..300 {
                let before = r.word() as u8;
                ee.write($at, &[before]);
                ee.call($addr, &[], &[], STACK).expect("original");
                let mut g = [before];
                ($port)(&mut g[..]);
                assert_eq!(ee.r8($at), g[0], "byte was {before:#x}");
                seen.insert(before);
            }
            assert!(seen.len() > 50, "the byte was varied");
        }
    };
}

getter!(orig_102e00_matches_the_game, 0x102e00, 0x33e73c, 4, |g: &mut [u8]| l::word_33e73c(g));
getter!(orig_102ee0_matches_the_game, 0x102ee0, 0x33e740, 1, |g: &mut [u8]| l::byte_33e740(g));
getter!(orig_103408_matches_the_game, 0x103408, 0x33e780, 4, |g: &mut [u8]| l::word_33e780(g));
getter!(orig_103418_matches_the_game, 0x103418, 0x33e774, 4, |g: &mut [u8]| l::word_33e774(g));
getter!(orig_103428_matches_the_game, 0x103428, 0x33e778, 4, |g: &mut [u8]| l::word_33e778(g));
getter!(orig_103fb0_matches_the_game, 0x103fb0, 0x33e7d0, 4, |g: &mut [u8]| l::pause_param(g));
getter!(orig_104058_matches_the_game, 0x104058, 0x33e7d4, 4, |g: &mut [u8]| l::word_33e7d4(g));
getter!(orig_104078_matches_the_game, 0x104078, 0x33e7dc, 4, |g: &mut [u8]| l::word_33e7dc_at_least_one(g));
getter!(orig_104120_matches_the_game, 0x104120, 0x33e7cc, 4, |g: &mut [u8]| l::word_33e7cc(g));
getter!(orig_104498_matches_the_game, 0x104498, 0x33e80c, 4, |g: &mut [u8]| l::word_33e80c(g));

constant!(orig_102f18_matches_the_game, 0x102f18, l::title_mode());
constant!(orig_103d68_matches_the_game, 0x103d68, l::menu_mode());
constant!(orig_104048_matches_the_game, 0x104048, l::gummi_battle_mode());
constant!(orig_104488_matches_the_game, 0x104488, l::gummi_menu_mode());
constant!(orig_14a938_matches_the_game, 0x14a938, l::block_356a20());

setter!(orig_102ef0_matches_the_game, 0x102ef0, 0x33e740, |g: &mut [u8]| l::clear_byte_33e740(g));
setter!(orig_14a220_matches_the_game, 0x14a220, 0x35639a, |g: &mut [u8]| l::soft_reset_disable(g));
setter!(orig_14a230_matches_the_game, 0x14a230, 0x35639a, |g: &mut [u8]| l::soft_reset_enable(g));

#[test]
fn orig_1041d8_matches_the_game() {
    const SPEC: Spec = Spec {
        addr: 0x1041d8,
        args: &[Arg::Range(0, 0x3f)],
        floats: 0,
        ret: Ret::Int,
        globals: &[(0x33e820, 0x40, true)],
        stubs: &[],
        nulls: true,
    };
    let Some(st) = fuzz(&SPEC, CASES, |c| {
        let i = c.int(0);
        Out::int(l::byte_33e820(c.global(0), i) as i64)
    }) else {
        return;
    };
    report(st);
}

/// The executable's own table (0x377e50, not randomised): keys around the stored ones.
#[test]
fn orig_103280_matches_the_game() {
    const SPEC: Spec = Spec {
        addr: 0x103280,
        args: &[Arg::Range(-3, 400)],
        floats: 0,
        ret: Ret::Int,
        globals: &[(0x377e50, 0x400, false)],
        stubs: &[],
        nulls: true,
    };
    let mut hits = 0u32;
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let k = c.int(0);
        let v = l::table_377e50(c.global(0), k);
        hits += (v != -1) as u32;
        Out::int(v as i64)
    }) else {
        return;
    };
    assert!(hits > 0, "some keys must hit a pair of the real table");
    report(st);
}

/// The same lookup over random tables.
#[test]
fn orig_103280_matches_the_game_on_random_tables() {
    const SPEC: Spec = Spec {
        addr: 0x103280,
        args: &[Arg::Int(&[0, 1, 2, 5, 100, 255, 0x7fff, -1, 0x10005])],
        floats: 0,
        ret: Ret::Int,
        globals: &[(0x377e50, 0x400, true)],
        stubs: &[],
        nulls: true,
    };
    let Some(st) = fuzz(&SPEC, CASES, |c| {
        let k = c.int(0);
        Out::int(l::table_377e50(c.global(0), k) as i64)
    }) else {
        return;
    };
    report(st);
}

// ---- hand harnesses: calls through memory ----

const OBJ: u32 = 0x0180_0000;
const HOOK: u32 = 0x0170_0000;

fn world(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

/// The calls a hooked address receives: (a0, a1, a2).
type Log = Rc<RefCell<Vec<(u32, u32, u32)>>>;

fn hook(ee: &mut Ee, at: u32, log: &Log) {
    let l = log.clone();
    ee.hooks.insert(at, move |ee: &mut Ee| l.borrow_mut().push((ee.gpr[4] as u32, ee.gpr[5] as u32, ee.gpr[6] as u32)));
}

/// Both entry points of the allocation thunk call slot 0 of the object's table as `slot0(object, size, 0)`.
#[test]
fn orig_106220_matches_the_game() {
    let Some(mut ee) = world("orig_106220") else { return };
    let log: Log = Rc::default();
    let mut r = Rng::new(0x1062_20);
    for n in 0..400u32 {
        let (obj, vt) = (OBJ + n * 0x40, OBJ + 0x10_0000 + n * 0x10);
        let func = HOOK + (n % 8) * 0x10;
        hook(&mut ee, func, &log);
        ee.w32(obj, vt);
        ee.w32(vt, func);
        let size = r.word();
        for entry in [0x106220u32, 0x106250] {
            log.borrow_mut().clear();
            ee.call(entry, &[size, obj as u64], &[], STACK).expect("original");
            let got = log.borrow().clone();
            assert_eq!(got.len(), 1, "case {n}: one virtual call");
            let want = l::alloc_call(size as u32, obj, |a| ee.r32(a));
            assert_eq!((func, [got[0].0, got[0].1, got[0].2]), (want.func, want.args), "case {n} entry {entry:#x}");
        }
    }
}

/// Both entry points of the free thunk: nothing for a null block, otherwise slot 1 of the table behind the word before the block.
#[test]
fn orig_106280_matches_the_game() {
    let Some(mut ee) = world("orig_106280") else { return };
    let log: Log = Rc::default();
    let mut r = Rng::new(0x1062_80);
    let (mut nulls, mut frees) = (0, 0);
    for n in 0..400u32 {
        let (hdr, vt, block) = (OBJ + 0x20_0000 + n * 0x40, OBJ + 0x30_0000 + n * 0x40, OBJ + 0x40_0000 + n * 0x40);
        let func = HOOK + 0x100 + (n % 8) * 0x10;
        hook(&mut ee, func, &log);
        ee.w32(block - 4, hdr);
        ee.w32(hdr, vt);
        ee.w32(vt + 4, func);
        // A null block is drawn often.
        let p: u64 = if r.below(4) == 0 { 0 } else { block as u64 };
        for entry in [0x106280u32, 0x1062b0] {
            log.borrow_mut().clear();
            ee.call(entry, &[p], &[], STACK).expect("original");
            let got = log.borrow().clone();
            match l::free_call(p, |a| ee.r32(a)) {
                None => {
                    nulls += 1;
                    assert!(got.is_empty(), "case {n}: null frees nothing");
                }
                Some(c) => {
                    frees += 1;
                    assert_eq!(got.len(), 1, "case {n}: one virtual call");
                    assert_eq!((c.func, c.args), (func, [got[0].0, got[0].1, 0]), "case {n} entry {entry:#x}");
                }
            }
        }
    }
    assert!(nulls > 0 && frees > 0, "both branches reached: {nulls} nulls, {frees} frees");
}

/// The task list head is 0x3990c0; a node's next link is +0xc and its flag word +0x1c. The mode objects are nodes of the real list.
const MODES: [u32; 4] = [0x33e6e0, 0x33e710, 0x33e7a0, 0x33e7e0];
const HEAD: u32 = 0x3990c0;
const MODE_WORD: u32 = 0x33e784;

#[test]
fn orig_103a48_matches_the_game() {
    let Some(mut ee) = world("orig_103a48") else { return };
    let mut r = Rng::new(0x103a48);
    let mut seen = [0u32; 6];
    for n in 0..2000u32 {
        // A chain of 1..=5 distinct nodes (mode objects and filler nodes), each with a flag word; one is always flagged 1.
        let len = 1 + r.below(5) as usize;
        let mut nodes: Vec<u32> = Vec::new();
        for i in 0..len {
            let pick = r.below(6);
            let a = if pick < 4 { MODES[pick as usize] } else { OBJ + 0x50_0000 + (n * 8 + i as u32) * 0x40 };
            if !nodes.contains(&a) {
                nodes.push(a);
            }
        }
        let flagged_at = r.below(nodes.len() as u64) as usize;
        for (i, &a) in nodes.iter().enumerate() {
            let flag = if i == flagged_at || r.below(2) == 0 { 1 } else { 2 + r.below(3) as u32 };
            ee.w32(a + 0x1c, flag);
            ee.w32(a + 0xc, nodes.get(i + 1).copied().unwrap_or(0));
        }
        ee.w32(HEAD, nodes[0]);
        let cur = r.word() as u32 & 7;
        ee.w32(MODE_WORD, cur);
        let req: i64 = if r.below(3) == 0 { (r.word() % 9) as i64 - 2 } else { -1 };
        ee.call(0x103a48, &[req as u64], &[], STACK).expect("original");
        let got = ee.r32(MODE_WORD);
        let first = nodes.iter().copied().find(|&a| ee.r32(a + 0x1c) == 1).expect("a flagged node");
        let want = l::game_mode(req, cur, first);
        assert_eq!(got, want, "case {n}: request {req}, first flagged {first:#x}, current {cur}");
        seen[if req != -1 { 5 } else { MODES.iter().position(|&m| m == first).unwrap_or(4) }] += 1;
    }
    assert!(seen.iter().all(|&s| s > 0), "every branch was reached: {seen:?}");
}
