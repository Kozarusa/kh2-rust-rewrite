//! Oracle tests for the global object list (0x1b7440 insert, 0x1b7c88 walker, 0x1b80e8 membership, 0x19a3f8, 0x19a440 leave flag,
//! 0x16b928 serial, 0x16b0d8 lookup by serial): kh2-ee runs the original on random object sets and the port
//! (`kh2_game_core::objlist`) runs on the same ones; after every insert the two list words and every object block byte are
//! compared (links and the spawn record handle translated through the handle map), and after the last insert every query function
//! of the unit is compared over all objects, a non-member and null.
//!
//! The harness is written for this unit instead of `factory gen` because the inputs are linked lists the generator cannot lay out
//! (a list head in a global, links between blocks). It follows the same rules: the original runs from the user's executable with no
//! stand-ins (every callee of these functions is ported here), cases come from a seeded generator (a failing case number
//! reproduces), and a guard block after the arenas audits that the original writes nothing outside them.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_game_core::objlist::*;

const OBJS: u32 = 0x0180_0000;
const RECS: u32 = 0x0190_0000;
const GUARD: u32 = 0x01a0_0000;
const HEAD: u32 = 0x01d5_d7a0;
const TAIL: u32 = 0x01d5_d7a4;
const REC_BASE: u32 = 0x0f00_0000;
const CASES: u64 = 2000;

fn obj_addr(slot: usize) -> u32 {
    OBJS + slot as u32 * 0x1000
}

struct Side {
    list: ObjList,
    /// Serial of each spawn record, by record number.
    serials: Vec<u16>,
}

impl Side {
    /// Handle to the original's address.
    fn tr(&self, h: u32) -> u32 {
        match ObjList::slot_of(h) {
            Some(s) if h != 0 => obj_addr(s),
            _ => h,
        }
    }
    fn rec_tr(h: u32) -> u32 {
        if h == 0 { 0 } else { RECS + (h - REC_BASE) / 0x1000 * 0x100 }
    }
}

fn call(ee: &mut Ee, addr: u32, args: &[u64]) -> u64 {
    ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
}

fn sx(v: i32) -> u64 {
    v as i64 as u64
}

fn compare(ee: &mut Ee, side: &Side, case: u64, what: &str) {
    assert_eq!(ee.r32(HEAD), side.tr(side.list.head), "case {case} {what}: head word");
    assert_eq!(ee.r32(TAIL), side.tr(side.list.tail), "case {case} {what}: tail word");
    for h in side.list.live() {
        let a = side.tr(h);
        let mut mine = side.list.get(h).unwrap().0.to_vec();
        for (o, v) in [(OFF_LINK, side.tr(side.list.get(h).unwrap().link())), (OFF_SPAWN, Side::rec_tr(side.list.get(h).unwrap().spawn()))] {
            mine[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        let game: Vec<u8> = (0..0x1000u32).map(|i| ee.r8(a + i)).collect();
        if let Some(o) = (0..0x1000).find(|&o| game[o] != mine[o]) {
            panic!("case {case} {what}: object {a:#x} byte +{o:#x}: game {:02x} port {:02x}", game[o], mine[o]);
        }
    }
}

fn run() {
    let Some(elf) = kh2_factory::exe_or_skip("object list oracle") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut r = Rng::new(0x0b1157);
    let (mut inserts, mut ties, mut leaving, mut found) = (0u64, 0u64, 0u64, 0u64);
    for case in 0..CASES {
        ee.write(GUARD, &[0xa5; 0x1000]);
        ee.w32(HEAD, 0);
        ee.w32(TAIL, 0);
        let mut side = Side { list: ObjList::new(), serials: Vec::new() };
        let count = 1 + r.below(16) as usize;
        let prio_span = [1u64, 3, 6, 256][r.below(4) as usize];
        let mut order = Vec::new();
        for n in 0..count + 1 {
            // The last object is created but never inserted: a non-member for the queries.
            let h = side.list.alloc();
            assert_eq!(h, ObjList::handle_of(n));
            let a = obj_addr(n);
            ee.write(a, &[0u8; 0x1000]);
            let prio = if prio_span == 256 { r.word() as u8 } else { r.below(prio_span) as u8 + if r.below(4) == 0 { 0x7e } else { 0 } };
            let flags = match r.below(6) {
                0 => 0x8_0000,
                1 => 0x1000_0000,
                2 => r.word() as u32,
                _ => 0,
            };
            let blk = side.list.get_mut(h).unwrap();
            blk.set_priority(prio);
            blk.set_flags(flags);
            ee.write(a + OFF_PRIORITY as u32, &[prio]);
            ee.w32(a + OFF_FLAGS as u32, flags);
            if r.below(4) != 0 {
                let serial = r.below(5) as u16 | if r.below(8) == 0 { 0x8000 } else { 0 };
                let idx = side.serials.len();
                side.serials.push(serial);
                let rh = REC_BASE + idx as u32 * 0x1000;
                side.list.get_mut(h).unwrap().set_spawn(rh);
                ee.w32(a + OFF_SPAWN as u32, Side::rec_tr(rh));
                ee.write(RECS + idx as u32 * 0x100, &[0u8; 0x100]);
                ee.write(RECS + idx as u32 * 0x100 + 0x1e, &serial.to_le_bytes());
            }
            if n < count {
                if order.iter().any(|&(p, _)| p == prio) {
                    ties += 1;
                }
                order.push((prio, h));
                call(&mut ee, 0x1b7440, &[a as u64]);
                side.list.insert(h);
                inserts += 1;
                compare(&mut ee, &side, case, "after insert");
            }
        }
        let non_member = ObjList::handle_of(count);
        let mut probes = side.list.live();
        probes.push(0);
        let walked = side.list.walk();
        assert_eq!(walked.len(), count, "case {case}: the walk reaches every inserted object");
        for &h in &probes {
            let a = side.tr(h);
            let ret = |ee: &mut Ee, f: u32| call(ee, f, &[a as u64]) as u32;
            assert_eq!(ret(&mut ee, 0x1b80e8) != 0, side.list.contains(h), "case {case}: membership of {a:#x}");
            if h != 0 {
                assert_eq!(ret(&mut ee, 0x19a440) != 0, side.list.get(h).unwrap().is_leave(), "case {case}: is_leave {a:#x}");
                assert_eq!(ret(&mut ee, 0x19a3f8) != 0, side.list.is_live(h), "case {case}: is_live {a:#x}");
                let ser = (call(&mut ee, 0x16b928, &[a as u64]) & 0xffff) as u16;
                assert_eq!(ser, side.list.serial_of(h, &|rh| side.serials[((rh - REC_BASE) / 0x1000) as usize]), "case {case}: serial {a:#x}");
            }
            let nx = ret(&mut ee, 0x1b7c88);
            assert_eq!(nx, side.tr(side.list.next_live(h)), "case {case}: next_live after {a:#x}");
            if nx != 0 && h != 0 {
                found += 1;
            }
        }
        // The membership walk also answers for a block that was never linked, and the leave flag counts only for members.
        let na = side.tr(non_member);
        assert_eq!(call(&mut ee, 0x1b80e8, &[na as u64]) as u32 != 0, side.list.contains(non_member), "case {case}: non-member");
        assert_eq!(call(&mut ee, 0x19a3f8, &[na as u64]) as u32 != 0, side.list.is_live(non_member), "case {case}: non-member live");
        for s in 0..6i32 {
            let want = side.list.find_by_serial(s as u16, &|rh| side.serials[((rh - REC_BASE) / 0x1000) as usize]);
            let got = call(&mut ee, 0x16b0d8, &[sx(s)]) as u32;
            assert_eq!(got, side.tr(want), "case {case}: lookup of serial {s}");
        }
        leaving += probes.iter().filter(|&&h| h != 0 && side.list.get(h).unwrap().is_leave()).count() as u64;
        let guard: Vec<u8> = (0..0x1000u32).map(|i| ee.r8(GUARD + i)).collect();
        assert!(guard.iter().all(|&b| b == 0xa5), "case {case}: the original wrote outside its arenas");
    }
    eprintln!("object list: {CASES} cases, {inserts} inserts ({ties} priority ties), {leaving} leaving objects, {found} walker hits");
    assert!(ties > 500 && leaving > 500 && found > 1000, "the generator exercises ties, leaving objects and the walker");
}

/// One generated run compares every function of the unit; each address has its own test name so the ledger marks it verified.
#[test]
fn orig_1b7440_matches_the_game() {
    run();
}

#[test]
fn orig_1b7c88_matches_the_game() {
    run();
}

#[test]
fn orig_1b80e8_matches_the_game() {
    run();
}

#[test]
fn orig_19a3f8_matches_the_game() {
    run();
}

#[test]
fn orig_19a440_matches_the_game() {
    run();
}

#[test]
fn orig_16b928_matches_the_game() {
    run();
}

#[test]
fn orig_16b0d8_matches_the_game() {
    run();
}
