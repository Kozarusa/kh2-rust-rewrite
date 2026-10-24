//! Oracle tests for the core leaves of `kh2_game_core::leaves_d` (work package L1-P24). kh2-ee runs each original from the user's
//! executable. Leaves that read or write only their blocks and globals use the `factory gen` harness (`fuzz`, 4,000 cases, blocks
//! and globals random, every byte compared). The observer list functions need small counts (the original loops over the count), the
//! virtual call, the record list and the queue poll call other units or follow pointers between blocks, so they have hand harnesses
//! below: the callee address is hooked and the call it receives is compared with what the port says it would make.
//!
//! Stand-ins: 0x14a818 (called by 0x14a988), 0x3331e0 (called by 0x1be0b8) and the slot-0 target of 0x11f170 are hooked to log their
//! arguments; every other original runs for real.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_factory::fuzz::*;
use kh2_game_core::leaves_d as l;

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

// ---- factory harness ----------------------------------------------------------------------------------------------------------

#[test]
fn orig_11bef0_matches_the_game() {
    const S: Spec = spec(0x11bef0, &[Arg::Ptr(blk(0x40, &[]))], Ret::None, &[]);
    let Some(st) = fuzz(&S, CASES, |c| {
        l::zero_32(c.block(0));
        Out::none()
    }) else {
        return;
    };
    report(st);
}

#[test]
fn orig_135cd0_matches_the_game() {
    const S: Spec = spec(0x135cd0, &[Arg::Ptr(blk(0x98, &[(0xc, 4, Kind::Int)]))], Ret::None, &[]);
    let Some(st) = fuzz(&S, CASES, |c| {
        l::observer_clear(c.block(0));
        Out::none()
    }) else {
        return;
    };
    report(st);
}

#[test]
fn orig_135eb0_matches_the_game() {
    const S: Spec = spec(0x135eb0, &[Arg::Ptr(blk(0x98, &[(0xc, 4, Kind::Int)]))], Ret::Int, &[]);
    let Some(st) = fuzz(&S, CASES, |c| Out::int(l::observer_count(c.block(0)) as i32 as i64)) else { return };
    report(st);
}

#[test]
fn orig_14aa58_matches_the_game() {
    const S: Spec = spec(0x14aa58, &[], Ret::None, &[(0x356c40, 1, true), (0x356c41, 7, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        l::soft_pause_clear(c.global(0));
        Out::none()
    }) else {
        return;
    };
    report(st);
}

#[test]
fn orig_14aa48_matches_the_game() {
    const S: Spec = spec(0x14aa48, &[], Ret::None, &[(0x356c40, 1, true), (0x356c41, 7, true)]);
    let Some(st) = fuzz(&S, CASES, |c| {
        l::soft_pause_set(c.global(0));
        Out::none()
    }) else {
        return;
    };
    report(st);
}

#[test]
fn orig_17d980_matches_the_game() {
    const S: Spec = spec(0x17d980, &[Arg::Ptr(blk(0x10, &[])), Arg::Int(&[0, -1])], Ret::None, &[]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let v = c.int(1) as u32;
        l::store_word(c.block(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// Bits 0..8191 of the save block's bit array (256 words at 0x349d38, randomised).
#[test]
fn orig_2f7ed8_matches_the_game() {
    const S: Spec = spec(0x2f7ed8, &[Arg::Range(0, 8191)], Ret::Int, &[(0x349d38, 0x400, true)]);
    let mut set = 0u32;
    let Some(st) = fuzz(&S, CASES, |c| {
        let n = c.int(0) as u32;
        let b = l::save_bit(c.global(0), n);
        set += b as u32;
        Out::int(b as i64)
    }) else {
        return;
    };
    assert!(set > 1000 && set < CASES - 1000, "both outcomes are drawn: {set} set");
    report(st);
}

/// The first argument is the key (compared with neighbours of 0 and 0xff), the second points at a byte.
#[test]
fn orig_334100_matches_the_game() {
    const S: Spec = spec(0x334100, &[Arg::Int(&[0, 0xff, 0x80]), Arg::Ptr(blk(0x10, &[(0, 1, Kind::Int)]))], Ret::Int, &[]);
    let Some(st) = fuzz(&S, CASES, |c| {
        let key = c.int(0);
        let byte = c.block(0)[0];
        Out::int(l::byte_compare(key, byte) as i64)
    }) else {
        return;
    };
    report(st);
}

// ---- hand harnesses -------------------------------------------------------------------------------------------------------------

const OBJ: u32 = 0x0180_0000;
const HOOK: u32 = 0x0170_0000;

fn world(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

/// The calls a hooked address receives: (a0, a1).
type Log = Rc<RefCell<Vec<(u64, u64)>>>;

fn hook(ee: &mut Ee, at: u32, log: &Log) {
    let l = log.clone();
    ee.hooks.insert(at, move |ee: &mut Ee| l.borrow_mut().push((ee.gpr[4] as u64, ee.gpr[5] as u64)));
}

fn put_observer(ee: &mut Ee, at: u32, count: i32, slots: &[u32]) {
    ee.write(at, &[0xa5; 0x98]);
    ee.w32(at + 0xc, count as u32);
    for (i, &s) in slots.iter().enumerate() {
        ee.w32(at + 0x10 + 4 * i as u32, s);
    }
}

fn observer_bytes(ee: &mut Ee, at: u32) -> Vec<u8> {
    (0..0x98).map(|i| ee.r8(at + i)).collect()
}

/// A list with 0..=32 entries drawn from a pool of five pointers (so duplicates occur and the first match must win), the slots past
/// the count holding stale pointers; the search key is a pool member or a stranger, and a negative count is tried now and then.
#[test]
fn orig_135eb8_matches_the_game() {
    let Some(mut ee) = world("orig_135eb8") else { return };
    let mut r = Rng::new(0x135eb8);
    let pool = [0u32, 0x0200_1000, 0x0200_2000, 0x0200_3000, 0x0200_4000];
    let (mut found, mut missed) = (0, 0);
    for n in 0..4000u32 {
        let count = if r.below(16) == 0 { -(r.below(5) as i32) } else { r.below(33) as i32 };
        let slots: Vec<u32> = (0..32).map(|_| pool[r.below(5) as usize]).collect();
        let at = OBJ + (n % 8) * 0x100;
        put_observer(&mut ee, at, count, &slots);
        let key = if r.below(6) == 0 { 0x0200_9000 } else { pool[r.below(5) as usize] };
        let got = ee.call(0x135eb8, &[at as u64, key as u64], &[], STACK).expect("original") as u32 as i32;
        let want = l::observer_index_of(&observer_bytes(&mut ee, at), key);
        assert_eq!(got, want, "case {n}: count {count}, key {key:#x}");
        if want >= 0 {
            found += 1;
        } else {
            missed += 1;
        }
    }
    assert!(found > 500 && missed > 500, "both outcomes: {found} found, {missed} missed");
}

/// Entry n modulo the count: n from -100 to 100 and counts 0..=32 (a zero count must give 0); the slots hold distinct values.
#[test]
fn orig_135f30_matches_the_game() {
    let Some(mut ee) = world("orig_135f30") else { return };
    let mut r = Rng::new(0x135f30);
    let (mut zero_count, mut wrapped_low, mut wrapped_high) = (0, 0, 0);
    for n in 0..4000u32 {
        let count = r.below(33) as i32;
        let slots: Vec<u32> = (0..32).map(|i| 0x0300_0000 + 0x100 * i + r.below(16) as u32).collect();
        let at = OBJ + (n % 8) * 0x100;
        put_observer(&mut ee, at, count, &slots);
        let idx = r.below(201) as i32 - 100;
        let got = ee.call(0x135f30, &[at as u64, idx as i64 as u64], &[], STACK).expect("original") as u32;
        let want = l::observer_entry_wrapped(&observer_bytes(&mut ee, at), idx);
        assert_eq!(got, want, "case {n}: count {count}, index {idx}");
        zero_count += (count == 0) as u32;
        wrapped_low += (count > 0 && idx < 0) as u32;
        wrapped_high += (count > 0 && idx >= count) as u32;
    }
    assert!(zero_count > 20 && wrapped_low > 500 && wrapped_high > 500, "all branches: {zero_count} {wrapped_low} {wrapped_high}");
}

/// The tail of the original's slot-0 call: the hooked function receives the object as its argument.
#[test]
fn orig_11f170_matches_the_game() {
    let Some(mut ee) = world("orig_11f170") else { return };
    let log: Log = Rc::default();
    let mut r = Rng::new(0x11f170);
    for n in 0..400u32 {
        let (obj, vt) = (OBJ + n * 0x40, OBJ + 0x10_0000 + n * 0x10);
        let func = HOOK + (n % 8) * 0x10;
        hook(&mut ee, func, &log);
        ee.w32(obj, vt);
        ee.w32(vt, func);
        ee.w32(vt + 4, HOOK + 0x1000 + r.below(8) as u32 * 0x10);
        log.borrow_mut().clear();
        ee.call(0x11f170, &[obj as u64], &[], STACK).expect("original");
        let got = log.borrow().clone();
        assert_eq!(got.len(), 1, "case {n}: one virtual call");
        let want = l::slot0_target(obj, |a| ee.r32(a));
        assert_eq!(want, func, "case {n}");
        assert_eq!(got[0].0, obj as u64, "case {n}: the object is the argument");
    }
}

/// 0x14a988 calls 0x14a818 with the queue at 0x3564d0 only while the soft-pause byte at 0x356c40 is zero.
#[test]
fn orig_14a988_matches_the_game() {
    let Some(mut ee) = world("orig_14a988") else { return };
    let log: Log = Rc::default();
    hook(&mut ee, 0x14a818, &log);
    let mut r = Rng::new(0x14a988);
    let (mut polled, mut skipped) = (0, 0);
    for n in 0..400u32 {
        let byte = [0u8, 1, 2, 0x80, 0xff, r.word() as u8][r.below(6) as usize];
        ee.write(l::SOFT_PAUSE, &[byte]);
        log.borrow_mut().clear();
        ee.call(0x14a988, &[], &[], STACK).expect("original");
        let got = log.borrow().clone();
        if l::soft_pause_polls_queue(&[byte]) {
            polled += 1;
            assert_eq!(got.len(), 1, "case {n}: byte {byte}");
            assert_eq!(got[0].0, l::SOFT_PAUSE_QUEUE as u64, "case {n}");
        } else {
            skipped += 1;
            assert!(got.is_empty(), "case {n}: byte {byte} polls nothing");
        }
    }
    assert!(polled > 0 && skipped > 0);
}

/// 0x1be0b8 tail-calls 0x3331e0(0x1d5dd50, its own argument); the full 64-bit argument is passed on.
#[test]
fn orig_1be0b8_matches_the_game() {
    let Some(mut ee) = world("orig_1be0b8") else { return };
    let log: Log = Rc::default();
    hook(&mut ee, 0x3331e0, &log);
    let mut r = Rng::new(0x1be0b8);
    for n in 0..400u32 {
        let arg = if n < 4 { [0, 1, u64::MAX, 1 << 63][n as usize] } else { r.word() };
        log.borrow_mut().clear();
        ee.call(0x1be0b8, &[arg], &[], STACK).expect("original");
        let got = log.borrow().clone();
        assert_eq!(got.len(), 1, "case {n}: one call");
        let (a0, a1) = l::list_call_args(arg);
        assert_eq!(got[0], (a0 as u64, a1), "case {n}");
    }
}

/// The record init: garbage records, the list empty or ending at a record of its own (sometimes the new record itself). The new
/// record (0x58 bytes), the old tail record and the head and tail words are compared.
#[test]
fn orig_1bdfc0_matches_the_game() {
    let Some(mut ee) = world("orig_1bdfc0") else { return };
    let mut r = Rng::new(0x1bdfc0);
    let (mut empty, mut appended, mut selfie) = (0, 0, 0);
    for n in 0..2000u32 {
        let rec = OBJ + (n % 4) * 0x100;
        let old = OBJ + 0x10_0000 + (n % 4) * 0x100;
        let garbage = |r: &mut Rng| -> Vec<u8> { (0..0x58).map(|_| r.word() as u8).collect() };
        let (rec_bytes, old_bytes) = (garbage(&mut r), garbage(&mut r));
        let (head0, tail0) = match r.below(8) {
            0 => (0, 0),
            1 => (rec, rec),
            _ => (old, old),
        };
        let (head0, tail0) = if r.below(2) == 0 { (head0, tail0) } else { (r.word() as u32 & 0x00ff_fff0, tail0) };
        ee.write(rec, &rec_bytes);
        ee.write(old, &old_bytes);
        ee.w32(l::LIST_HEAD, head0);
        ee.w32(l::LIST_TAIL, tail0);
        let owner = r.word() as u32;
        ee.call(0x1bdfc0, &[rec as u64, owner as u64], &[], STACK).expect("original");
        let got: Vec<u8> = (0..0x58).map(|i| ee.r8(rec + i)).collect();
        let got_old: Vec<u8> = (0..0x58).map(|i| ee.r8(old + i)).collect();
        let got_list = (ee.r32(l::LIST_HEAD), ee.r32(l::LIST_TAIL));

        let (mut mine, mut mine_old) = (rec_bytes.clone(), old_bytes.clone());
        let (mut head, mut tail) = (head0, tail0);
        l::record_init(&mut mine, rec, owner, &mut head, &mut tail, |addr, v| {
            assert_eq!(addr, old + l::RECORD_NEXT as u32, "case {n}: only the old tail is linked");
            mine_old[l::RECORD_NEXT..l::RECORD_NEXT + 4].copy_from_slice(&v.to_le_bytes());
        });
        assert_eq!(got, mine, "case {n}: record");
        assert_eq!(got_old, mine_old, "case {n}: old tail record");
        assert_eq!(got_list, (head, tail), "case {n}: head and tail");
        match tail0 {
            0 => empty += 1,
            t if t == rec => selfie += 1,
            _ => appended += 1,
        }
    }
    assert!(empty > 50 && appended > 500 && selfie > 50, "all branches: {empty} {appended} {selfie}");
}

// ---- unit tests (no disc): the documented edges ---------------------------------------------------------------------------------

#[test]
fn observer_wrap_edges() {
    let mut o = vec![0u8; 0x98];
    for (i, w) in [10u32, 20, 30].into_iter().enumerate() {
        o[0x10 + i * 4..0x14 + i * 4].copy_from_slice(&w.to_le_bytes());
    }
    o[0xc] = 3;
    assert_eq!(l::observer_entry_wrapped(&o, -1), 30);
    assert_eq!(l::observer_entry_wrapped(&o, 3), 10);
    assert_eq!(l::observer_entry_wrapped(&o, -4), 30);
    assert_eq!(l::observer_index_of(&o, 20), 1);
    assert_eq!(l::observer_index_of(&o, 99), -1);
    o[0xc] = 0;
    assert_eq!(l::observer_entry_wrapped(&o, 5), 0);
    l::observer_clear(&mut o);
    assert!(o[0xc..0x90].iter().all(|&b| b == 0));
}
