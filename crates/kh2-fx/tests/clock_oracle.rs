//! The effect clock, measured on the game's own instance update `FUN_001e3898` and instance draw `FUN_001e3d00`
//! (run in `kh2-ee` on the user's executable; skipped without a disc, KH2_ISO or local/iso.txt).
//!
//! Every field frame the effect manager updates each instance (`FUN_001daa20` -> `FUN_001e3898`, after handing the
//! library the field's frame step `DAT_00356f40` through `FUN_001e4620` -> `DAT_00360b00`) and draws it
//! (`FUN_001db020` -> `FUN_001e3d00`). A particle-program frame is `FUN_001e6240` (the instance time +0x58 gains
//! 0x1000, the objects step, the life is checked). The draw runs one after drawing; the update adds the step to the
//! instance's accumulator (+0x98) and runs one more for every `DAT_0037ce98` (2.0) beyond the first, a catch-up
//! for slow frames. The step is 1.0 per vblank (`FUN_0014cdd0`); the field takes two vblanks a frame (step 2.0,
//! docs/menus.md 1.6.1). This test counts the frames. Callees that would touch the rest of the engine are hooks:
//! `FUN_001e61d0` (the programs) and `FUN_001e6240` count their calls, `FUN_001e17f8`, `FUN_001e52d8`, `FUN_001e1230`
//! and `FUN_001e01a8` do nothing; the instance's draw-list flags (+0x91) are clear, so no packet is built; the
//! reads of the hardware timer `REG_RCNT0_COUNT` (0x10000000, profiling counters only) are pointed at RAM.

use std::cell::Cell;

use kh2_ee::{Ee, reg};

thread_local! {
    static RUNS: Cell<u32> = const { Cell::new(0) };
    static FRAMES: Cell<u32> = const { Cell::new(0) };
}

fn hook_run(_: &mut Ee) {
    RUNS.with(|c| c.set(c.get() + 1));
}

fn hook_frame(ee: &mut Ee) {
    FRAMES.with(|c| c.set(c.get() + 1));
    ee.gpr[reg::V0] = 0;
}

fn hook_nothing(_: &mut Ee) {}

fn exe() -> Option<Vec<u8>> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())
}

const INSTANCE: u32 = 0x01f0_0000;
const STEP: u32 = 0x0036_0b00;

/// (program frames, program runs) after `calls` frames (update `FUN_001e3898` with frame step `step`, then draw
/// `FUN_001e3d00`).
fn count(ee: &mut Ee, step: f32, calls: u32) -> (u32, u32) {
    for a in (INSTANCE..INSTANCE + 0x200).step_by(4) {
        ee.w32(a, 0);
    }
    RUNS.with(|c| c.set(0));
    FRAMES.with(|c| c.set(0));
    for _ in 0..calls {
        ee.wf32(STEP, step);
        ee.call(0x001e_3898, &[INSTANCE as u64, 0], &[], 0x01ff_0000).expect("FUN_001e3898");
        ee.call(0x001e_3d00, &[INSTANCE as u64], &[], 0x01ff_0000).expect("FUN_001e3d00");
    }
    (FRAMES.with(|c| c.get()), RUNS.with(|c| c.get()))
}

/// The particle programs advance one frame (0x1000) per frame drawn, plus one per 2.0 of step beyond the first
/// 2.0: at the field's step of 2.0 (two vblanks, 30 frames a second) one per field frame, so an effect's life of
/// N frames lasts N / 30 s, not N / 60 s; a slow frame of four vblanks (step 4.0) catches up with two.
#[test]
fn the_effect_program_runs_one_frame_per_two_vblanks() {
    let Some(elf) = exe() else {
        eprintln!("skipped: no disc (set KH2_ISO)");
        return;
    };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    assert_eq!(ee.f32_at(0x0037_ce98), 2.0, "the frame threshold DAT_0037ce98");
    ee.hooks.insert(0x001e_61d0, hook_run);
    ee.hooks.insert(0x001e_6240, hook_frame);
    ee.hooks.insert(0x001e_17f8, hook_nothing);
    ee.hooks.insert(0x001e_52d8, hook_nothing);
    ee.hooks.insert(0x001e_1230, hook_nothing);
    ee.hooks.insert(0x001e_01a8, hook_nothing);
    // `lui rt, 0x1000` (the timer's page) -> `lui rt, 0x0100` (RAM), only inside FUN_001e3898 and FUN_001e3d00.
    let mut patched = 0;
    for pc in (0x001e_3898..0x001e_4218).step_by(4) {
        let w = ee.r32(pc);
        if w >> 26 == 0x0f && w & 0xffff == 0x1000 {
            ee.w32(pc, (w & 0xffff_0000) | 0x0100);
            patched += 1;
        }
    }
    assert!(patched > 0, "no timer reads found");
    let field = count(&mut ee, 2.0, 30);
    let menu = count(&mut ee, 1.0, 60);
    let slow = count(&mut ee, 4.0, 15);
    eprintln!("FUN_001e3898: step 2.0 x 30 calls -> {field:?}, step 1.0 x 60 -> {menu:?}, step 4.0 x 15 -> {slow:?} (frames, runs)");
    assert_eq!(field, (30, 30), "one program frame per field frame (two vblanks)");
    assert_eq!(menu.0, 60, "a 60 Hz frame (step 1.0) still draws, so it steps once");
    assert_eq!(slow.0, 30, "a four-vblank frame catches up with a second program frame");
}
