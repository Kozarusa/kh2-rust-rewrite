//! Oracle tests of the card unit's start-up routines (WP-L1-P27a): the CRC unit's init `FUN_0023e3d8`, the buffer allocation
//! `FUN_0023c948` and the unit's init `FUN_0023c830`.
//!
//! They allocate with the game's heap (`operator new` thunk `FUN_00106250`, `FUN_00106220`), which belongs to another unit. In kh2-ee
//! both thunks are hooks that hand out blocks from a bump region (starting at an address with a varied low part, so the table's
//! 64-byte alignment is exercised) that was filled with 0xaa first, and log the requested sizes; everything else runs for real
//! (the memset `FUN_00310ba4`, the size-to-KB helper `FUN_0023cd38`, the CRC table routine `FUN_0023e4c0`, the reset `FUN_0023e528`,
//! the save-block getter `FUN_001057a8`, the world-name test `FUN_0023e318`). The port's side is `Crc::new`, `crc::build_table`,
//! `Buffers::default`, `align_table` and `body::init_alt_bit`.
//!
//! Skips without a disc; fails without one when `KH2_REQUIRE_DISC=1`.

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use common::*;
use kh2_ee::Ee;
use kh2_save::body::init_alt_bit;
use kh2_save::crc::{Crc, build_table};
use kh2_save::dir::{Buffers, align_table};

const HEAP: u32 = 0x01a0_0000;
const NEW: u32 = 0x106250;
const LOAD_ALLOC: u32 = 0x106220;
const ALLOCATOR_WORD: u32 = 0x36cb08;

/// The allocation log: (size, allocator argument, address given).
type Log = Rc<RefCell<Vec<(u32, u32, u32)>>>;

fn hook_heap(ee: &mut Ee, start: u32) -> Log {
    let log: Log = Rc::default();
    let next = Rc::new(Cell::new(start));
    for entry in [NEW, LOAD_ALLOC] {
        let (l, n) = (log.clone(), next.clone());
        ee.hooks.insert(entry, move |ee: &mut Ee| {
            let (size, obj) = (ee.gpr[4] as u32, ee.gpr[5] as u32);
            let at = n.get();
            l.borrow_mut().push((size, obj, at));
            // The next block starts after this one, 4-byte aligned.
            n.set((at + size + 3) & !3);
            ee.gpr[2] = u128::from(at);
        });
    }
    log
}

/// Fills the heap region with a pattern the init routines must overwrite where they write.
fn fill_heap(o: &mut Orig) {
    o.put(HEAP, &vec![0xaa; 0x2_0000]);
}

#[test]
fn orig_23e3d8_matches_the_game() {
    let Some(mut o) = Orig::new("oracle test of 0x23e3d8", 0x23e3d8) else { return };
    let want_table = build_table();
    for n in 0..200 {
        fill_heap(&mut o);
        let start = HEAP + 4 * o.rng.below(0x300) as u32;
        let log = hook_heap(&mut o.ee, start);
        // The state words before: random, so the reset is seen.
        let w = o.rng.word() as u32;
        o.set_word(0x36cb40, w);
        let w = o.rng.word() as u32;
        o.set_word(0x36cb38, w);
        let w = o.rng.word() as u32;
        o.set_word(0x36cb3c, w);
        let allocator = o.rng.word() as u32;
        o.call(0x23e3d8, &[allocator as u64]);
        let got = log.borrow().clone();
        assert_eq!(got, vec![(0x400, allocator, start)], "case {n}: one block of 0x400 bytes from the allocator");
        assert_eq!(o.word(0x36cb38), allocator, "case {n}: allocator kept");
        assert_eq!(o.word(0x36cb3c), start, "case {n}: table pointer");
        let table: Vec<u32> = (0..256).map(|i| o.ee.r32(start + 4 * i)).collect();
        assert_eq!(table, want_table, "case {n}: the table");
        assert_eq!(o.word(0x36cb40), Crc::new().state, "case {n}: the state is reset");
    }
}

#[test]
fn orig_23c948_matches_the_game() {
    let Some(mut o) = Orig::new("oracle test of 0x23c948", 0x23c948) else { return };
    let port = Buffers::default();
    let mut aligned_seen = [0u32; 2];
    for n in 0..200 {
        fill_heap(&mut o);
        let start = HEAP + 4 * o.rng.below(0x300) as u32;
        let log = hook_heap(&mut o.ee, start);
        let allocator = o.rng.word() as u32;
        o.set_word(ALLOCATOR_WORD, allocator);
        for g in [0x36cb0c, 0x36cb10, 0x36cb14, 0x36cb18, 0x36cb1c, 0x36cb20, 0x36cb24] {
            o.set_word(g, 0);
        }
        o.call(0x23c948, &[]);
        let sizes: Vec<u32> = log.borrow().iter().map(|a| a.0).collect();
        let lens = [port.name.0.len(), port.path.len(), port.file.len(), port.info.0.len(), port.list.len(), port.table.len()];
        assert_eq!(sizes, lens.map(|l| l as u32), "case {n}: the allocation sizes");
        assert!(log.borrow().iter().all(|a| a.1 == allocator), "case {n}: every block comes from the unit's allocator");
        let at = |i: usize| log.borrow()[i].2;
        for (g, i) in [(0x36cb0c, 0), (0x36cb10, 1), (0x36cb14, 2), (0x36cb18, 3), (0x36cb1c, 4), (0x36cb24, 5)] {
            assert_eq!(o.word(g), at(i), "case {n}: pointer {g:#x}");
        }
        assert_eq!(o.word(0x36cb20), align_table(at(5)), "case {n}: aligned table start");
        aligned_seen[(at(5) & 0x3f == 0) as usize] += 1;
        let bytes = |i: usize, len: usize| o.get(at(i), len);
        assert_eq!(bytes(0, 0x20), port.name.0.to_vec(), "case {n}: name buffer");
        assert_eq!(bytes(1, 0x12), port.path.to_vec(), "case {n}: path buffer");
        assert_eq!(bytes(2, port.file.len()), port.file, "case {n}: file buffer");
        assert_eq!(bytes(3, port.info.0.len()), port.info.0, "case {n}: card info block");
        assert_eq!(bytes(4, port.list.len()), port.list, "case {n}: file list");
        assert_eq!(bytes(5, port.table.len()), port.table, "case {n}: directory table");
    }
    assert!(aligned_seen[0] > 20, "unaligned table allocations drawn: {aligned_seen:?}");
}

/// The unit's init: allocator word, CRC init, buffers, and bit 8 of the save block word at `+0x31a8` set from the world name of
/// the current world (byte `+0xc`) and story flag 0x108b. The port covers that last step (`body::init_alt_bit`); the allocation
/// halves are the two tests above.
#[test]
fn orig_23c830_matches_the_game() {
    let Some(mut o) = Orig::new("oracle test of 0x23c830", 0x23c830) else { return };
    let flag_byte = 0xe50 + (0x108b >> 10) * 0x20 + ((0x108b & 0x3ff) >> 5) * 4 + ((0x108b & 31) >> 3);
    let mut alt = [0u32; 2];
    for n in 0..400 {
        fill_heap(&mut o);
        let _log = hook_heap(&mut o.ee, HEAP);
        // A random save block with the world byte near 4 and the flag bit chosen, so both outcomes occur.
        let mut live = o.random(0x3a00);
        live[0xc] = [3, 4, 4, o.rng.word() as u8][o.rng.below(4) as usize];
        if o.rng.below(2) == 0 {
            live[flag_byte] |= 1 << (0x108b & 7);
        } else {
            live[flag_byte] &= !(1 << (0x108b & 7));
        }
        o.put(SAVE, &live);
        let allocator = o.rng.word() as u32;
        o.call(0x23c830, &[allocator as u64]);
        assert_eq!(o.word(ALLOCATOR_WORD), allocator, "case {n}: allocator word");
        let mut want = live.clone();
        init_alt_bit(&mut want);
        assert_eq!(o.get(SAVE, want.len()), want, "case {n}: the save block");
        alt[((want[0x31a9] & 1) != 0) as usize] += 1;
    }
    assert!(alt[0] > 50 && alt[1] > 50, "both outcomes of the alternate world name: {alt:?}");
}
