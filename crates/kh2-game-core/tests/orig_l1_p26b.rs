//! Oracle tests for the screen state routines (`kh2_game_core::screen`, L1-P26b): the fog setters and pass word, the monochrome
//! test, the clear block setters and getters, and the three routines that only forward to the GS and loader code of other units
//! (the event freeze, the frame store of the area fade and the clear task). kh2-ee runs the original from the user's executable
//! and the port on the same inputs; globals and blocks are compared byte for byte (the fog factors with exact float bits), and a
//! forwarding routine's callees are hooked so the registers of each call can be compared with the port's `Call` records.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_factory::fuzz::*;
use kh2_game_core::screen as s;

const CASES: u32 = 2000;

fn report(st: Stats) {
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

fn world(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

/// A block of plain bytes.
const BYTES: Block = Block { size: 0x300, fields: &[], subs: &[] };
const UNIT: Block = Block { size: 0x200, fields: &[], subs: &[] };

#[test]
fn orig_10a5e0_matches_the_game() {
    const SPEC: Spec = Spec {
        addr: 0x10a5e0,
        args: &[Arg::Int(&[0, 1, 0x80, 0xff, 0x100]), Arg::Int(&[0, 1, 0x80, 0xff, 0x100]), Arg::Int(&[0, 1, 0x80, 0xff, 0x100])],
        floats: 0,
        ret: Ret::None,
        globals: &[(0x34e0d0, 3, true)],
        stubs: &[],
        nulls: true,
    };
    let Some(st) = fuzz(&SPEC, CASES, |c| {
        let (r, g, b) = (c.int(0) as u8, c.int(1) as u8, c.int(2) as u8);
        s::set_fog_colour(c.global(0), r, g, b);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// The fog range: exact float bits over ordinary ranges, equal and reversed distances, and a zero span (the divide saturates).
#[test]
fn orig_10a630_matches_the_game() {
    let Some(mut ee) = world("orig_10a630") else { return };
    let mut r = Rng::new(0x10a630);
    let mut zero_span = 0u32;
    for n in 0..4000u32 {
        let (near, far) = match n % 8 {
            0 => {
                let v = r.float();
                (v, v)
            }
            1 => (0.0, r.float()),
            2 => (r.float(), 0.0),
            3 => (5000.0, 1.0e6),
            4 => (1000.0, 1.0e6),
            5 => (1.0e30, -1.0e30),
            _ => (r.float() * 100.0, r.float() * 100.0),
        };
        let mut before = [0u8; 16];
        for b in before.iter_mut() {
            *b = r.word() as u8;
        }
        ee.write(0x34e0b4, &before);
        ee.call(0x10a630, &[], &[near, far], STACK).expect("original");
        let got: Vec<u8> = (0..16).map(|i| ee.r8(0x34e0b4 + i)).collect();
        let mut port = before;
        s::set_fog_range(&mut port, near, far);
        assert_eq!(got, port.to_vec(), "case {n}: near {near} far {far}");
        zero_span += (near == far) as u32;
    }
    assert!(zero_span > 400, "zero spans drawn: {zero_span}");
}

#[test]
fn orig_10a6b0_matches_the_game() {
    const SPEC: Spec = Spec {
        addr: 0x10a6b0,
        args: &[],
        floats: 2,
        ret: Ret::None,
        globals: &[(0x34e0c8, 8, true)],
        stubs: &[],
        nulls: true,
    };
    let Some(st) = fuzz(&SPEC, CASES, |c| {
        let (a, b) = (c.float(0), c.float(1));
        s::set_fog_limits(c.global(0), a, b);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

#[test]
fn orig_10a9b0_matches_the_game() {
    const SPEC: Spec = Spec {
        addr: 0x10a9b0,
        args: &[],
        floats: 0,
        ret: Ret::Int,
        globals: &[(0x34e0d4, 4, true)],
        stubs: &[],
        nulls: true,
    };
    let Some(st) = fuzz(&SPEC, CASES, |c| Out::int(s::fog_pass(c.global(0)) as i32 as i64)) else { return };
    report(st);
}

#[test]
fn orig_10ac98_matches_the_game() {
    let Some(mut ee) = world("orig_10ac98") else { return };
    let mut r = Rng::new(0x10ac98);
    for _ in 0..300 {
        let got = ee.call(0x10ac98, &[r.word(), r.word()], &[], STACK).expect("original") as u32;
        assert_eq!(got, s::fog_packet_block());
    }
}

/// The monochrome test: the word is 1 only for the value one.
#[test]
fn orig_13ab98_matches_the_game() {
    let Some(mut ee) = world("orig_13ab98") else { return };
    let mut r = Rng::new(0x13ab98);
    let (mut yes, mut no) = (0, 0);
    for n in 0..1000u32 {
        let v: u32 = match n % 4 {
            0 => 1,
            1 => r.below(4) as u32,
            2 => 1 | (1 << (1 + r.below(31))),
            _ => r.word() as u32,
        };
        ee.w32(0x355158, v);
        let got = ee.call(0x13ab98, &[], &[], STACK).expect("original") as u32 & 0xff;
        let want = s::is_monochrome(&v.to_le_bytes());
        assert_eq!(got != 0, want, "word {v:#x}");
        assert!(got <= 1, "a boolean");
        if want {
            yes += 1;
        } else {
            no += 1;
        }
    }
    assert!(yes > 200 && no > 200, "both answers reached: {yes} {no}");
}

#[test]
fn orig_14da88_matches_the_game() {
    const SPEC: Spec = Spec {
        addr: 0x14da88,
        args: &[Arg::Ptr(UNIT), Arg::Int(&[0, 0xff, 0x100]), Arg::Int(&[0, 0xff, 0x100]), Arg::Int(&[0, 0xff, 0x100])],
        floats: 0,
        ret: Ret::None,
        globals: &[],
        stubs: &[],
        nulls: false,
    };
    let Some(st) = fuzz(&SPEC, CASES, |c| {
        let (r, g, b) = (c.int(1) as u8, c.int(2) as u8, c.int(3) as u8);
        s::clear_set_colour(c.block(0), r, g, b);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

#[test]
fn orig_14daa8_matches_the_game() {
    const SPEC: Spec = Spec {
        addr: 0x14daa8,
        args: &[Arg::Ptr(UNIT), Arg::Ptr(BYTES), Arg::Ptr(BYTES), Arg::Ptr(BYTES)],
        floats: 0,
        ret: Ret::None,
        globals: &[],
        stubs: &[],
        nulls: false,
    };
    let Some(st) = fuzz(&SPEC, CASES, |c| {
        let (r, g, b) = s::clear_colour(c.block(0));
        c.block(1)[0] = r;
        c.block(2)[0] = g;
        c.block(3)[0] = b;
        Out::none()
    }) else {
        return;
    };
    report(st);
}

#[test]
fn orig_14dac8_matches_the_game() {
    const SPEC: Spec = Spec {
        addr: 0x14dac8,
        args: &[Arg::Ptr(UNIT), Arg::Int(&[0, 1, 0x100])],
        floats: 0,
        ret: Ret::None,
        globals: &[],
        stubs: &[],
        nulls: false,
    };
    let Some(st) = fuzz(&SPEC, CASES, |c| {
        let v = c.int(1) as u8;
        s::clear_set_switch(c.block(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

/// Through the frame unit: the original computes the clear block address with 0x14e370 (not stubbed) and stores the colour.
#[test]
fn orig_14de68_matches_the_game() {
    const SPEC: Spec = Spec {
        addr: 0x14de68,
        args: &[Arg::Int(&[0, 0xff, 0x100]), Arg::Int(&[0, 0xff, 0x100]), Arg::Int(&[0, 0xff, 0x100])],
        floats: 0,
        ret: Ret::None,
        globals: &[(0x358000, 0x600, true)],
        stubs: &[],
        nulls: true,
    };
    let Some(st) = fuzz(&SPEC, CASES, |c| {
        let (r, g, b) = (c.int(0) as u8, c.int(1) as u8, c.int(2) as u8);
        s::clear_set_colour_of_unit(c.global(0), r, g, b);
        Out::none()
    }) else {
        return;
    };
    report(st);
}

// ---- forwarding routines: the callees are hooked and their registers compared ---------------------------------------------

type Log = Rc<RefCell<Vec<(u32, [u32; 8])>>>;

/// Hooks `at` so that a call to it is logged (address and a0..a3, t0..t3, low 32 bits each) and does nothing else.
fn hook(ee: &mut Ee, at: u32, log: &Log) {
    let l = log.clone();
    ee.hooks.insert(at, move |e: &mut Ee| {
        let a: [u32; 8] = std::array::from_fn(|i| e.gpr[4 + i] as u32);
        l.borrow_mut().push((at, a));
    });
}

fn as_pairs(calls: &[s::Call]) -> Vec<(u32, [u32; 8])> {
    calls.iter().map(|c| (c.target, c.args)).collect()
}

/// The event freeze: two copies through 0x166038 with the base from the packed word and the halves of the record's height.
#[test]
fn orig_14db58_matches_the_game() {
    let Some(mut ee) = world("orig_14db58") else { return };
    let log: Log = Rc::default();
    hook(&mut ee, 0x166038, &log);
    let mut r = Rng::new(0x14db58);
    let unit = ARENA;
    for n in 0..2000u32 {
        let mut bytes = [0u8; 16];
        for b in bytes.iter_mut() {
            *b = r.word() as u8;
        }
        if n % 3 == 0 {
            // Typical sizes: width 640 or 512, height 224 or 448.
            bytes[4..6].copy_from_slice(&[640u16, 512, 256][r.below(3) as usize].to_le_bytes());
            bytes[6..8].copy_from_slice(&[224u16, 448, 7][r.below(3) as usize].to_le_bytes());
        }
        ee.write(unit, &bytes);
        let dst = r.word() as u32 & 0x00ff_fff0;
        let mode = r.int(&[0, 1, 0x3ff, 0x400, 0x7ff, 0x800, -1]);
        log.borrow_mut().clear();
        ee.call(0x14db58, &[unit as u64, dst as u64, mode as i64 as u64], &[], STACK).expect("original");
        let want = s::freeze_event(&bytes, dst, mode);
        // The copy routine takes seven arguments (a0..a3, t0..t2); t3 is not loaded.
        let mut got = log.borrow().clone();
        for g in got.iter_mut() {
            g.1[7] = 0;
        }
        assert_eq!(got, as_pairs(&want), "case {n}: dst {dst:#x} mode {mode:#x} record {bytes:02x?}");
    }
}

/// The frame store of the area fade: four calls, the first with the sign-extended base, the width in 64ths and the record's
/// halfwords. The cache flush and the two GS routines receive only the arguments the original loads; the other registers hold
/// whatever the caller left, so only the arguments the port names are compared.
#[test]
fn orig_14dbe8_matches_the_game() {
    let Some(mut ee) = world("orig_14dbe8") else { return };
    let log: Log = Rc::default();
    for at in [0x2f9460, 0x318620, 0x2f8f60, 0x2f9720] {
        hook(&mut ee, at, &log);
    }
    let mut r = Rng::new(0x14dbe8);
    let rec = ARENA;
    for n in 0..2000u32 {
        let mut bytes = [0u8; 16];
        for b in bytes.iter_mut() {
            *b = r.word() as u8;
        }
        if n % 3 == 0 {
            bytes[0..2].copy_from_slice(&[0u16, 0x4c0, 0x3ff, 0x400, 0x7ff][r.below(5) as usize].to_le_bytes());
        }
        ee.write(rec, &bytes);
        let dst = r.word() as u32 & 0x00ff_fff0;
        log.borrow_mut().clear();
        ee.call(0x14dbe8, &[rec as u64, dst as u64], &[], STACK).expect("original");
        let want = s::store_frame(&bytes, dst);
        let got = log.borrow().clone();
        assert_eq!(got.len(), want.len(), "case {n}: four calls");
        let used = [8usize, 1, 2, 2];
        for (i, ((gt, ga), w)) in got.iter().zip(want.iter()).enumerate() {
            assert_eq!(*gt, w.target, "case {n}: call {i} target");
            assert_eq!(ga[..used[i]], w.args[..used[i]], "case {n}: call {i} arguments (record {bytes:02x?})");
        }
    }
}

/// The clear task: the clear packet send always, the background load only when the byte is 1.
#[test]
fn orig_14dc70_matches_the_game() {
    let Some(mut ee) = world("orig_14dc70") else { return };
    let log: Log = Rc::default();
    hook(&mut ee, 0x14e348, &log);
    hook(&mut ee, 0x14ddc0, &log);
    let mut r = Rng::new(0x14dc70);
    let (mut loads, mut skips) = (0, 0);
    for n in 0..600u32 {
        let v = if n % 2 == 0 { 1 } else { r.word() as u8 };
        ee.write(0x3584e0, &[v]);
        log.borrow_mut().clear();
        ee.call(0x14dc70, &[], &[], STACK).expect("original");
        let want = s::clear_task(v);
        let got = log.borrow().clone();
        assert_eq!(got.len(), want.len(), "byte {v}");
        for ((gt, ga), w) in got.iter().zip(want.iter()) {
            assert_eq!(*gt, w.target);
            assert_eq!(ga[0], w.args[0], "byte {v}: first argument of {gt:#x}");
        }
        if v == 1 {
            loads += 1;
        } else {
            skips += 1;
        }
    }
    assert!(loads > 200 && skips > 200, "both paths reached: {loads} {skips}");
}
