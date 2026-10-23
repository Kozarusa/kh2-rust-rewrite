//! Oracle tests of the file unit helpers (`core.file`) against the original run in kh2-ee on the user's executable:
//! the BAR entry lookups 0x168428, 0x168460, 0x1684c0, 0x168510, the three fix-up passes 0x168570, 0x168690, 0x1687b8
//! and the request queue bookkeeping 0x1682e0, 0x168268, 0x167d20, 0x167da8, 0x167e58, 0x167ee0, 0x1683e0, 0x168408,
//! 0x168240. Callees that belong to other subsystems (the payload loaders, the name helpers, the allocator, the memory
//! clear) are replaced by recording hooks in the original; the port reports the same calls through its host callback,
//! and the logs are compared. Skips without a disc (fails with `KH2_REQUIRE_DISC=1`).
//! Not in these tests: 0x167cc0, 0x167f88, 0x168010, 0x168110, 0x1681a0 and the one-call wrappers (thread, CD and callback
//! code the port replaces by direct disc reads).

use kh2_bar::request::{self, Queue, Request};
use kh2_bar::resident;
use kh2_ee::Ee;
use kh2_factory::fuzz::Rng;
use std::cell::RefCell;
use std::rc::Rc;

const STACK: u32 = 0x01ff_0000;
const IMG: u32 = 0x0180_0000;
const IMG2: u32 = 0x0188_0000;
const IMG_LEN: usize = 0x4000;
const HEAD: u32 = 0x35b940;
const TAIL: u32 = 0x35b944;
const REC: u32 = 0x0181_0000;
const NAME: u32 = 0x0182_0000;
const OLD_HEAD: u32 = 0x0181_8000;
const FAKE_ALLOC: u32 = 0x01f0_0000;
const ALLOC_OBJ: u32 = 0x0183_0000;
const VTABLE: u32 = 0x0183_0100;

type Log = Rc<RefCell<Vec<(u32, u32, u32)>>>;

fn ee() -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip("oracle test of the file unit")?;
    let mut e = Ee::new();
    e.load_elf(&elf).expect("elf");
    Some(e)
}

fn put16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// A BAR header with random entries (small type and name pools so lookups find several).
fn header(rng: &mut Rng) -> Vec<u8> {
    let n = rng.below(14) as usize;
    let mut b = vec![0u8; 0x10 + 0x10 * n + 0x40];
    for x in b.iter_mut() {
        *x = rng.word() as u8;
    }
    put32(&mut b, 4, n as u32);
    for k in 0..n {
        put16(&mut b, 0x10 + 0x10 * k, [0, 1, 4, 7, 9, 0x22, 0x14, 0x12][rng.below(8) as usize]);
        put32(&mut b, 0x14 + 0x10 * k, [0u32, 1, 2, 0xffff_ffff, 0x6f_6e65][rng.below(5) as usize]);
    }
    b
}

fn ty_arg(rng: &mut Rng) -> u32 {
    [0, 1, 4, 7, 9, 0x22, 0x14, 0x12, 0xffff_ffff, 0x10000, 3][rng.below(11) as usize]
}

fn name_arg(rng: &mut Rng) -> i32 {
    [0u32, 1, 2, 0xffff_ffff, 0x6f_6e65, 9][rng.below(6) as usize] as i32
}

#[test]
fn orig_168428_matches_the_game() {
    let Some(mut ee) = ee() else { return };
    let mut rng = Rng::new(0x168428);
    let mut hits = 0;
    for n in 0..3000 {
        let h = header(&mut rng);
        let ty = ty_arg(&mut rng);
        ee.write(IMG, &h);
        let g = ee.call(0x168428, &[IMG as u64, sx(ty)], &[], STACK).expect("original") as u32 as i32;
        hits += (g > 0) as u32;
        assert_eq!(g, resident::count_of_type(&h, IMG, IMG, ty), "case {n}: type {ty:#x} {h:x?}");
    }
    assert!(hits > 100, "the cases must find entries");
}

#[test]
fn orig_168460_matches_the_game() {
    let Some(mut ee) = ee() else { return };
    let mut rng = Rng::new(0x168460);
    let out = IMG2;
    let mut hits = 0;
    for n in 0..3000 {
        let h = header(&mut rng);
        let ty = ty_arg(&mut rng);
        let name = name_arg(&mut rng);
        ee.write(IMG, &h);
        ee.w32(out, 0x5555);
        ee.call(0x168460, &[IMG as u64, out as u64, sx(ty), sx(name as u32)], &[], STACK).expect("original");
        let g = ee.r32(out) as i32;
        hits += (g > 0) as u32;
        assert_eq!(g, resident::count_of_name(&h, IMG, IMG, ty, name), "case {n}: type {ty:#x} name {name:#x} {h:x?}");
    }
    assert!(hits > 100, "the cases must find entries");
}

#[test]
fn orig_1684c0_matches_the_game() {
    let Some(mut ee) = ee() else { return };
    let mut rng = Rng::new(0x1684c0);
    let mut found = 0;
    for n in 0..3000 {
        let h = header(&mut rng);
        let ty = ty_arg(&mut rng);
        let nth = rng.below(7) as i32 - 1;
        ee.write(IMG, &h);
        let g = ee.call(0x1684c0, &[IMG as u64, sx(ty), sx(nth as u32)], &[], STACK).expect("original") as u32;
        let p = resident::nth_of_type(&h, IMG, IMG, ty, nth);
        found += p.is_some() as u32;
        assert_eq!(g, p.unwrap_or(0), "case {n}: type {ty:#x} nth {nth} {h:x?}");
    }
    assert!(found > 100, "the cases must find entries");
}

#[test]
fn orig_168510_matches_the_game() {
    let Some(mut ee) = ee() else { return };
    let mut rng = Rng::new(0x168510);
    let mut found = 0;
    for n in 0..3000 {
        let h = header(&mut rng);
        let ty = ty_arg(&mut rng);
        let name = name_arg(&mut rng);
        let nth = rng.below(5) as i32 - 1;
        ee.write(IMG, &h);
        let g = ee.call(0x168510, &[IMG as u64, sx(ty), sx(name as u32), sx(nth as u32)], &[], STACK).expect("original") as u32;
        let p = resident::nth_of_name(&h, IMG, IMG, ty, name, nth);
        found += p.is_some() as u32;
        assert_eq!(g, p.unwrap_or(0), "case {n}: type {ty:#x} name {name:#x} nth {nth} {h:x?}");
    }
    assert!(found > 100, "the cases must find entries");
}

/// A random file image: a BAR whose entries are payloads, some of them nested BARs (offsets relative to their own
/// header, as the game's files have them), some with size zero, some with the no-fix-up flag.
fn tree(rng: &mut Rng, depth: u32) -> Vec<u8> {
    let n = rng.below(5) as usize;
    let mut b = vec![0u8; 0x10 + 0x10 * n];
    put32(&mut b, 0, 0x0152_4142);
    put32(&mut b, 4, n as u32);
    for k in 0..n {
        let o = 0x10 + 0x10 * k;
        let ty = [1u16, 4, 7, 8, 9, 0x11, 0x12, 0x14, 0x1e, 0x22, 0x2e, 3, 0x20][rng.below(13) as usize];
        let mut flags = if rng.below(5) == 0 { 1 } else { rng.below(2) as u16 * 2 };
        if matches!(ty, 1 | 0x11 | 0x14 | 0x1e | 0x2e) && depth >= 3 {
            flags |= 1;
        }
        put16(&mut b, o, ty);
        put16(&mut b, o + 2, flags);
        put32(&mut b, o + 4, rng.word() as u32);
        let payload = if rng.below(6) == 0 {
            Vec::new()
        } else if matches!(ty, 1 | 0x11 | 0x14 | 0x1e | 0x2e) && depth < 3 && flags & 1 == 0 {
            tree(rng, depth + 1)
        } else {
            (0..8 + rng.below(24) as usize).map(|_| rng.word() as u8).collect()
        };
        while !b.len().is_multiple_of(16) {
            b.push(0);
        }
        let at = b.len() as u32;
        put32(&mut b, o + 8, at);
        put32(&mut b, o + 0xc, payload.len() as u32);
        b.extend(payload);
    }
    b
}

const CALLEES: [u32; 15] = [
    0x11f0a8, 0x1320a0, 0x13aa68, 0x1d97a0, 0x127af0, 0x1dbe30, 0x11f198, 0x132168, 0x1d9848, 0x127b78, 0x1dbe50, 0x2c3838, 0x11f170,
    0x132160, 0x127b50,
];

fn hook_all(ee: &mut Ee, log: &Log) {
    for &a in &CALLEES {
        let l = log.clone();
        ee.hooks.insert(a, move |e: &mut Ee| l.borrow_mut().push((a, e.gpr[4] as u32, 0)));
    }
}

fn dump(ee: &mut Ee, at: u32) -> Vec<u8> {
    (0..IMG_LEN as u32).map(|i| ee.r8(at + i)).collect()
}

/// Runs `cases` random images through attach, then (per `pass`) rebase or release, comparing memory and loader calls.
fn passes(pass: u8) {
    let Some(mut ee) = ee() else { return };
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    hook_all(&mut ee, &log);
    let mut rng = Rng::new(0x168570 + pass as u64);
    let mut calls = 0usize;
    for n in 0..2000 {
        let t = tree(&mut rng, 0);
        let mut port = vec![0u8; IMG_LEN];
        port[..t.len()].copy_from_slice(&t);
        ee.write(IMG, &port);
        let mut plog: Vec<(u32, u32, u32)> = Vec::new();
        let mut host = |c: u32, a: u32| plog.push((c, a, 0));
        log.borrow_mut().clear();
        ee.call(0x168570, &[IMG as u64], &[], STACK).expect("original");
        resident::attach(&mut port, IMG, IMG, &mut host);
        if pass == 0 {
            assert_eq!(dump(&mut ee, IMG), port, "case {n}: memory\n{t:x?}");
            assert_eq!(*log.borrow(), plog, "case {n}: calls\n{t:x?}");
            calls += plog.len();
            continue;
        }
        // The later passes start from the attached image; rebase also moves it first.
        log.borrow_mut().clear();
        let mut plog2: Vec<(u32, u32, u32)> = Vec::new();
        let mut host2 = |c: u32, a: u32| plog2.push((c, a, 0));
        if pass == 1 {
            let moved = dump(&mut ee, IMG);
            ee.write(IMG2, &moved);
            ee.call(0x168690, &[IMG2 as u64], &[], STACK).expect("original");
            resident::rebase(&mut port, IMG2, IMG2, &mut host2);
            assert_eq!(dump(&mut ee, IMG2), port, "case {n}: memory\n{t:x?}");
        } else {
            ee.call(0x1687b8, &[IMG as u64], &[], STACK).expect("original");
            resident::release(&mut port, IMG, IMG, &mut host2);
            assert_eq!(dump(&mut ee, IMG), port, "case {n}: memory\n{t:x?}");
        }
        assert_eq!(*log.borrow(), plog2, "case {n}: calls\n{t:x?}");
        calls += plog2.len();
    }
    assert!(calls > 200, "the cases must reach the loaders ({calls})");
}

#[test]
fn orig_168570_matches_the_game() {
    passes(0);
}

#[test]
fn orig_168690_matches_the_game() {
    passes(1);
}

#[test]
fn orig_1687b8_matches_the_game() {
    passes(2);
}

/// The request queue: random list state before, one builder call, then the record, the list words and the helper calls.
fn queue_case(which: u32) {
    let Some(mut ee) = ee() else { return };
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    for (a, tag) in [(0x31218cu32, 0u32), (0x312078, 1), (0x310ba4, 2)] {
        let l = log.clone();
        ee.hooks.insert(a, move |e: &mut Ee| {
            let args = match tag {
                0 => (e.gpr[4] as u32, 0),
                1 => (e.gpr[4] as u32, e.gpr[5] as u32),
                _ => (e.gpr[4] as u32, e.gpr[6] as u32),
            };
            l.borrow_mut().push((a, args.0, args.1));
        });
    }
    ee.hooks.insert(FAKE_ALLOC, |e: &mut Ee| e.gpr[2] = REC as u128);
    ee.w32(0x33e69c, ALLOC_OBJ);
    ee.w32(ALLOC_OBJ, VTABLE);
    ee.w32(VTABLE, FAKE_ALLOC);
    let mut rng = Rng::new(0x167d20 + which as u64);
    for n in 0..2000 {
        let rec: Vec<u8> = (0..request::RECORD).map(|_| rng.word() as u8).collect();
        let (head, tail) = if rng.below(2) == 0 { (0, 0) } else { (OLD_HEAD, OLD_HEAD + 0x100 * rng.below(4) as u32) };
        let (p2, p3, p5, p6) = (rng.word() as u32, rng.word() as u32, rng.word() as u32, rng.word() as u32);
        let flag = rng.word() as u16;
        let kind = rng.int(&[0, 1, 2]);
        ee.write(REC, &rec);
        ee.w32(HEAD, head);
        ee.w32(TAIL, tail);
        log.borrow_mut().clear();
        let call = match which {
            0 => ee.call(0x1682e0, &[REC as u64, kind as u32 as u64, NAME as u64], &[], STACK),
            1 => ee.call(0x168268, &[REC as u64, kind as u32 as u64, NAME as u64], &[], STACK),
            2 => ee.call(0x167d20, &[NAME as u64, p2 as u64, p3 as u64, p5 as u64], &[], STACK),
            3 => ee.call(0x167da8, &[NAME as u64, p2 as u64, p3 as u64, flag as u64, p5 as u64, p6 as u64], &[], STACK),
            4 => ee.call(0x167e58, &[NAME as u64, p2 as u64, p3 as u64, p5 as u64], &[], STACK),
            _ => ee.call(0x167ee0, &[NAME as u64, p2 as u64, p3 as u64], &[], STACK),
        };
        call.expect("original");
        let (g_rec, g_head, g_tail) = ((0..request::RECORD as u32).map(|i| ee.r8(REC + i)).collect::<Vec<u8>>(), ee.r32(HEAD), ee.r32(TAIL));
        let g_log = log.borrow().clone();
        // The port.
        let mut q = Queue { head, tail };
        let mut r = Request(rec.clone().try_into().unwrap());
        let mut plog: Vec<(u32, u32, u32)> = Vec::new();
        let mut names = |c: u32| plog.push(if c == 0x31218c { (c, NAME, 0) } else { (c, REC + 8, NAME) });
        match which {
            0 | 1 => request::enqueue(&mut q, REC, &mut r, kind, &mut names),
            2 => request::plain(&mut q, REC, &mut r, p2, p3, p5, &mut names),
            3 => request::at_offset(&mut q, REC, &mut r, p2, p3, flag, p5, p6, &mut names),
            4 => request::bar(&mut q, REC, &mut r, p2, p3, p5, &mut names),
            _ => {
                let mut clears = Vec::new();
                request::special(&mut q, REC, &mut r, p2, p3, &mut |a, len| clears.push((a, len)), &mut names);
                assert_eq!(clears, [(0x1f18000, 0xa8000)]);
                // The original logs the clear first (a0 = address, a2 = size).
                plog.insert(0, (0x310ba4, 0x1f18000, 0xa8000));
            }
        }
        assert_eq!(g_rec, r.0.to_vec(), "case {n}: record");
        assert_eq!((g_head, g_tail), (q.head, q.tail), "case {n}: list words");
        assert_eq!(g_log, plog, "case {n}: helper calls");
    }
}

#[test]
fn orig_1682e0_matches_the_game() {
    queue_case(0);
}

#[test]
fn orig_168268_matches_the_game() {
    queue_case(1);
}

#[test]
fn orig_167d20_matches_the_game() {
    queue_case(2);
}

#[test]
fn orig_167da8_matches_the_game() {
    queue_case(3);
}

#[test]
fn orig_167e58_matches_the_game() {
    queue_case(4);
}

#[test]
fn orig_167ee0_matches_the_game() {
    queue_case(5);
}

#[test]
fn orig_1683e0_matches_the_game() {
    let Some(mut ee) = ee() else { return };
    let mut rng = Rng::new(0x1683e0);
    for n in 0..2000 {
        let (head, tail) = (rng.word() as u32, rng.word() as u32);
        let a = [1i32, 0, 2, -1][rng.below(4) as usize];
        let b = [0xffffi64, 0, 1, 0x1_0000, -1, 0xfffe][rng.below(6) as usize];
        ee.w32(HEAD, head);
        ee.w32(TAIL, tail);
        ee.call(0x1683e0, &[a as u32 as u64, b as u64], &[], STACK).expect("original");
        let mut q = Queue { head, tail };
        request::reset_if(&mut q, a, b);
        assert_eq!((ee.r32(HEAD), ee.r32(TAIL)), (q.head, q.tail), "case {n}: a {a} b {b:#x}");
    }
}

#[test]
fn orig_168408_matches_the_game() {
    let Some(mut ee) = ee() else { return };
    let mut rng = Rng::new(0x168408);
    for n in 0..200 {
        let (head, tail) = (rng.word() as u32, rng.word() as u32);
        ee.w32(HEAD, head);
        ee.w32(TAIL, tail);
        ee.call(0x168408, &[], &[], STACK).expect("original");
        let mut q = Queue { head, tail };
        request::reset(&mut q);
        assert_eq!((ee.r32(HEAD), ee.r32(TAIL)), (q.head, q.tail), "case {n}");
        assert_eq!(q, Queue::default());
    }
}

#[test]
fn orig_168240_matches_the_game() {
    let Some(mut ee) = ee() else { return };
    let mut rng = Rng::new(0x168240);
    for n in 0..200 {
        let head = if rng.below(2) == 0 { 0 } else { rng.word() as u32 };
        ee.w32(HEAD, head);
        let g = ee.call(0x168240, &[], &[], STACK).expect("original") as u32 & 0xff;
        let q = Queue { head, tail: 0 };
        assert_eq!(g, request::is_reading(&q) as u32, "case {n}: head {head:#x}");
    }
}

/// A 32-bit argument as the EE ABI passes it: sign-extended into the 64-bit register.
fn sx(v: u32) -> u64 {
    v as i32 as i64 as u64
}
