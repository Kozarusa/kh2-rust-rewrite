//! Oracle test of the area-script start `FUN_00181148` (WP-L1-P27a): the variables it sets before the three scripts run and the defaults it
//! applies after them, and the three calls it ends with (`FUN_00194188` twice with the cache counts, `FUN_001be828` with the level).
//! The script bodies themselves are the interpreter `FUN_00180b48`, which `orig_180b48.rs` compares command by command.
//!
//! Here the area file has no scripts: the BAR lookups `FUN_00168510` answer 0 (so no program runs) in the original and the port is given an
//! empty `Area`. The script variables (`0x35cd10..0x35cd44`, `0x35d268`, `0x1c893b8..0x1c893d4`) are random before the call, so every
//! byte the start writes is seen; the three program bytes are random (the cache counts become 0 above 0x32). `FUN_00181b68` (it resets
//! the area-data lists) is `jr ra`.
//!
//! What is not covered: a start that finds programs. The program lookup `FUN_00180a68` and the interpreter are compared in
//! `orig_180b48.rs` and by the port's `run_area_with` tests, not against the original's whole start.
//!
//! Skips without a disc; fails without one when `KH2_REQUIRE_DISC=1`.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ard::run::{ScriptContext, run_area_with};
use kh2_ard::Area;
use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::{Rng, STACK};

const PROGRAMS: u32 = 0x0181_0000;
const VARS: [(u32, u32); 3] = [(0x35cd10, 0x34), (0x35d268, 4), (0x1c893b8, 0x1c)];

#[test]
fn orig_181148_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x181148") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let log: Rc<RefCell<Vec<(u32, u32)>>> = Rc::default();
    ee.hooks.insert(0x168510, |e: &mut Ee| e.gpr[reg::V0] = 0);
    ee.hooks.insert(0x181b68, |e: &mut Ee| e.gpr[reg::V0] = 0);
    let l = log.clone();
    ee.hooks.insert(0x194188, move |e: &mut Ee| l.borrow_mut().push((e.gpr[4] as u32, e.gpr[5] as u32)));
    let l = log.clone();
    ee.hooks.insert(0x1be828, move |e: &mut Ee| l.borrow_mut().push((0, e.gpr[4] as u32)));
    let mut rng = Rng::new(0x181148);
    let mut high = 0;
    for n in 0..2000 {
        let raw: Vec<Vec<u8>> = VARS.iter().map(|&(_, len)| (0..len).map(|_| rng.word() as u8).collect()).collect();
        for (i, &(addr, _)) in VARS.iter().enumerate() {
            ee.write(addr, &raw[i]);
        }
        let programs = [rng.word() as u8, rng.word() as u8, rng.word() as u8];
        ee.write(PROGRAMS, &programs);
        log.borrow_mut().clear();
        ee.call(0x181148, &[PROGRAMS as u64], &[], STACK).unwrap_or_else(|f| panic!("case {n}: {f:?}"));
        let game: Vec<Vec<u8>> = VARS.iter().map(|&(a, len)| (0..len).map(|i| ee.r8(a + i)).collect()).collect();
        let game_log = log.borrow().clone();

        // The port: the random word and the entrance do not matter without programs.
        let mut ctx = ScriptContext::default();
        let run = run_area_with(&Area::default(), programs, 0, &mut ctx);
        let v = &run.vars;
        let mut a = vec![0u8; 0x34];
        a[0x00..0x08].copy_from_slice(&v.map_visibility.to_le_bytes());
        a[0x08..0x10].copy_from_slice(&v.word_35cd18.to_le_bytes());
        a[0x10..0x14].copy_from_slice(&v.heap_size.to_le_bytes());
        a[0x14..0x18].copy_from_slice(&v.heap_base.to_le_bytes());
        a[0x18..0x1c].copy_from_slice(&v.cache_a.to_le_bytes());
        a[0x1c..0x20].copy_from_slice(&v.cache_b.to_le_bytes());
        a[0x20..0x24].copy_from_slice(&v.word_35cd30.to_le_bytes());
        a[0x24..0x28].copy_from_slice(&v.word_35cd34.to_le_bytes());
        a[0x28..0x2c].copy_from_slice(&v.camera_type.to_le_bytes());
        a[0x2c..0x30].copy_from_slice(&v.effect_heap[0].to_le_bytes());
        a[0x30..0x34].copy_from_slice(&v.effect_heap[1].to_le_bytes());
        let mut c = vec![0u8; 0x1c];
        c[0x00..0x04].copy_from_slice(&v.capacity.to_le_bytes());
        c[0x08..0x10].copy_from_slice(&v.show.to_le_bytes());
        c[0x10..0x18].copy_from_slice(&v.hide.to_le_bytes());
        c[0x18..0x1c].copy_from_slice(&v.level.to_le_bytes());
        assert_eq!(game[0], a, "case {n}: variables at 0x35cd10 (programs {programs:x?})");
        assert_eq!(game[1], raw[1], "case {n}: 0x35d268 is not reset by the start");
        assert_eq!(game[2], c, "case {n}: variables at 0x1c893b8");
        let want = vec![(0x35d278, v.cache_a), (0x35daa0, v.cache_b), (0, v.level)];
        assert_eq!(game_log, want, "case {n}: the calls at the end");
        high += (programs[0] > 0x32) as u32;
    }
    assert!(high > 500 && 2000 - high > 200, "both sides of the 0x32 rule drawn: {high}");
}
