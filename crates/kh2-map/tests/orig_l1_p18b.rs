//! Oracle tests of the three polygon mask handlers of the walker's table at 0x378f7c, run in kh2-ee on the user's executable:
//! `kh2_map::oct::accepts_bit_0x40` (orig 0x32e3f0), `accepts_bit_0x80` (0x32e420) and `accepts_all` (0x32e44c). Each case
//! lays an attribute word in guest memory, passes its address as the third argument (as the dispatcher 0x32e3b8 does) and
//! compares the answer. The table itself is read at run time: the handler of each kind must be the one `kind_accepts` uses.
//! Skips without a disc (fails with `KH2_REQUIRE_DISC=1`).

use kh2_ee::Ee;
use kh2_factory::fuzz::Rng;
use kh2_map::oct::{accepts_all, accepts_bit_0x40, accepts_bit_0x80, kind_accepts};

const STACK: u32 = 0x01ff_0000;
const ATTR: u32 = 0x0180_0000;
const TABLE: u32 = 0x378f7c;

fn attr(rng: &mut Rng) -> u32 {
    match rng.below(4) {
        0 => 1 << rng.below(32),
        1 => rng.word() as u32 & !(1 << rng.below(32)),
        _ => rng.word() as u32,
    }
}

/// Runs the handler at `addr` over many attributes against `port`; returns how many answered yes.
fn check(addr: u32, port: impl Fn(u32) -> bool, seed: u64) -> u32 {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of the polygon mask handlers") else { return 0 };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut rng = Rng::new(seed);
    let mut yes = 0;
    for n in 0..2000 {
        let a = attr(&mut rng);
        ee.w32(ATTR, a);
        let r = ee.call(addr, &[rng.word(), rng.word(), ATTR as u64], &[], STACK).unwrap_or_else(|f| panic!("case {n}: {f:?}"));
        let game = r & 0xff != 0;
        assert_eq!(game, port(a), "case {n}: attribute {a:#x}");
        yes += game as u32;
    }
    assert!(yes > 100 && yes < 1900, "yes {yes}");
    yes
}

#[test]
fn orig_32e3f0_matches_the_game() {
    check(0x32e3f0, accepts_bit_0x40, 0x32e3f0);
}

#[test]
fn orig_32e420_matches_the_game() {
    check(0x32e420, accepts_bit_0x80, 0x32e420);
}

#[test]
fn orig_32e44c_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x32e44c") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut rng = Rng::new(0x32e44c);
    for n in 0..2000 {
        let a = attr(&mut rng);
        ee.w32(ATTR, a);
        let r = ee.call(0x32e44c, &[rng.word(), rng.word(), ATTR as u64], &[], STACK).unwrap_or_else(|f| panic!("case {n}: {f:?}"));
        assert_eq!(r & 0xffff_ffff, 1, "case {n}: attribute {a:#x}");
        assert!(accepts_all(a));
    }
}

/// The table at 0x378f7c holds the handler addresses `kind_accepts` stands for: kind 2 the bit-0x40 handler, kind 4 the
/// bit-0x80 one, kind 15 the accept-all one; the dispatcher (0x32e3b8) answers yes itself for a kind of 16 or more.
#[test]
fn the_handler_table_names_the_ported_handlers() {
    let Some(elf) = kh2_factory::exe_or_skip("the polygon handler table") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let table: Vec<u32> = (0..16).map(|k| ee.r32(TABLE + 4 * k)).collect();
    eprintln!("handler table {table:x?}");
    assert_eq!(table[2], 0x32e3f0);
    assert_eq!(table[4], 0x32e420);
    assert_eq!(table[15], 0x32e44c);
    // Every table entry is a handler that agrees with `kind_accepts` over single-bit and random attributes.
    let mut rng = Rng::new(0x378f7c);
    for (kind, &h) in table.iter().enumerate() {
        for _ in 0..200 {
            let a = attr(&mut rng);
            ee.w32(ATTR, a);
            let r = ee.call(h, &[0, 0, ATTR as u64], &[], STACK).expect("handler");
            assert_eq!(r & 0xff != 0, kind_accepts(kind as u32, a), "kind {kind} handler {h:#x} attribute {a:#x}");
            assert!(kind_accepts(16 + kind as u32, a), "kinds of 16 or more pass");
        }
    }
}
