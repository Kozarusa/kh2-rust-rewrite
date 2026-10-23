//! The run and handler API on hand-assembled code (no disc needed).

use std::cell::Cell;
use std::rc::Rc;

use kh2_ee::{Ee, SENTINEL, Stop};

const BASE: u32 = 0x0010_0000;
const JR_RA: u32 = 0x03E0_0008;

fn load(e: &mut Ee, at: u32, code: &[u32]) {
    for (i, w) in code.iter().enumerate() {
        e.w32(at + 4 * i as u32, *w);
    }
}

#[test]
fn run_stops_on_budget_and_resumes_where_it_left_off() {
    let mut e = Ee::new();
    // loop: addiu t0, t0, 1; b loop; nop
    load(&mut e, BASE, &[0x2508_0001, 0x1000_FFFE, 0]);
    e.pc = BASE;
    e.jump(BASE);
    assert_eq!(e.run(30).unwrap(), Stop::Steps);
    assert_eq!(e.steps, 30);
    let first = e.gpr[8] as u32;
    assert_eq!(first, 10, "one addiu per three instructions");
    assert_eq!(e.run(30).unwrap(), Stop::Steps);
    assert_eq!(e.gpr[8] as u32, 20);
}

#[test]
fn request_stop_from_a_hook_ends_run() {
    let mut e = Ee::new();
    // or s0, ra, zero; addiu t0, t0, 1; jal 0x100; nop; addiu t0, t0, 1; jr s0; nop
    load(&mut e, BASE, &[0x03E0_8025, 0x2508_0001, 0x0C00_0000 | (BASE + 0x100) >> 2, 0, 0x2508_0001, 0x0200_0008, 0]);
    load(&mut e, BASE + 0x100, &[JR_RA, 0]);
    let hits = Rc::new(Cell::new(0));
    let h = hits.clone();
    e.hooks.insert(BASE + 0x100, move |ee: &mut Ee| {
        h.set(h.get() + 1);
        ee.request_stop();
    });
    e.gpr[31] = SENTINEL as u128;
    e.jump(BASE);
    assert_eq!(e.run(1000).unwrap(), Stop::Requested);
    assert_eq!(hits.get(), 1);
    assert_eq!(e.gpr[8] as u32, 1, "stopped before the second addiu");
    assert_eq!(e.run(1000).unwrap(), Stop::Returned, "resuming finishes the function");
    assert_eq!(e.gpr[8] as u32, 2);
}

#[test]
fn a_hook_can_install_another_hook_and_it_takes_effect_at_once() {
    let mut e = Ee::new();
    // Calls 0x100 twice; the first call's hook installs a hook at 0x200 (called next) and the
    // second function (0x200) is plain code that would return 1 without it.
    load(&mut e, BASE, &[0x27BD_FFF0, 0xAFBF_0000, 0x0C00_0000 | (BASE + 0x100) >> 2, 0, 0x0C00_0000 | (BASE + 0x200) >> 2, 0, 0x8FBF_0000, 0x27BD_0010, JR_RA, 0]);
    load(&mut e, BASE + 0x100, &[JR_RA, 0]);
    load(&mut e, BASE + 0x200, &[0x2402_0001, JR_RA, 0]);
    e.hooks.insert(BASE + 0x100, |ee: &mut Ee| {
        ee.hooks.insert(BASE + 0x200, |ee: &mut Ee| ee.gpr[2] = 77);
    });
    assert_eq!(e.call(BASE, &[], &[], 0x01F0_0000).unwrap(), 77);
}

#[test]
fn context_switch_round_trip() {
    let mut e = Ee::new();
    e.gpr[8] = 5;
    e.vf[3] = [1.0, 2.0, 3.0, 4.0];
    e.jump(BASE);
    let a = e.context();
    e.gpr[8] = 9;
    e.vf[3] = [0.0; 4];
    e.jump(BASE + 0x40);
    e.set_context(&a);
    assert_eq!((e.gpr[8] as u32, e.pc, e.vf[3]), (5, BASE, [1.0, 2.0, 3.0, 4.0]));
}

#[test]
fn watchers_and_probes_on_the_same_page_do_not_disturb_other_addresses() {
    let mut e = Ee::new();
    load(&mut e, BASE, &[0x2402_0001, 0x2442_0001, 0x2442_0001, JR_RA, 0]);
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    e.probes.insert(BASE + 8, move |_: &mut Ee| s.set(s.get() + 1));
    assert_eq!(e.call(BASE, &[], &[], 0x01F0_0000).unwrap(), 3);
    assert_eq!(seen.get(), 1, "only the probed address fires");
}
