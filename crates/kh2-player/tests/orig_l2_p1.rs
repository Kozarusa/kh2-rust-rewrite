//! Oracle tests for the player's heading (WP-L2-P1): kh2-ee runs the original from the user's executable and
//! `kh2_player::controller_rules` (and `kh2_game_core::motor::{fix_radian, turn_angle}`) runs on the same inputs.
//!
//! The harness is hand-written, not `factory gen`: the functions work on an actor block whose fields the generator cannot name, so
//! the actor, the pad record, the mover and the view block are laid out here at the offsets the port documents and every case is
//! compared. It follows the factory's rules: the original runs from the executable, cases come from a seeded generator (a failing
//! case number reproduces), 1,000 or more cases per function, every case is compared, nothing is skipped inside the loops.
//!
//! Stand-ins (stubs), all listed here:
//! - `FUN_001068c0` (the view block lookup) returns a fixed block address: the tests of 0x177360 and 0x1774d8.
//! - `FUN_0011b460` (matrix reset) and `FUN_0011b520` (roll about the view axis) do nothing in the test of 0x106f78: the roll is
//!   zero in every case and the reset only writes words the function overwrites or the test does not compare.
//! - `FUN_001775e8`, `FUN_001775a0`, `FUN_00177710`, `FUN_001774d8` log their call in the test of 0x1772f8 (they are the readers
//!   the function chooses between).
//! - `FUN_00140f98` (look-around mode) answers a drawn bit in the test of 0x179108.
//! - `FUN_00179108` (the vtable slot +0x84) answers a drawn bit and `FUN_00197118` does nothing in the test of 0x174bd8.
//! - `FUN_0019ad98` (free rotation) answers 0 in the test of 0x16a790.
//! Floats are compared within the rounding of the two FPUs (relative 1e-5) unless bit equal.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_game_core::motor::{fix_radian, turn_angle};
use kh2_player::controller_rules::*;

const ACT: u32 = 0x0180_0000;
const REC: u32 = 0x0181_0000;
const MOV: u32 = 0x0182_0000;
const VIEW: u32 = 0x0183_0000;
const OUT: u32 = 0x0184_0000;
const VT: u32 = 0x0185_0000;
const CASES: u32 = 2000;

fn machine(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

fn call(ee: &mut Ee, addr: u32, args: &[u64], fargs: &[f32]) -> u64 {
    ee.call(addr, args, fargs, STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
}

fn feq(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a - b).abs() <= 1e-5 * (1.0 + a.abs().max(b.abs()))
}

fn assert_f(case: u32, what: &str, game: f32, port: f32) {
    assert!(feq(game, port), "case {case}: {what}: game {game} port {port}");
}

/// A float in -scale..scale with the edge values of the factory's generator now and then.
fn rf(r: &mut Rng, scale: f32) -> f32 {
    match r.below(10) {
        0 => 0.0,
        1 => scale,
        2 => -scale,
        _ => ((r.word() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0) * scale,
    }
}

fn put4(ee: &mut Ee, addr: u32, v: [f32; 4]) {
    for (i, x) in v.iter().enumerate() {
        ee.wf32(addr + 4 * i as u32, *x);
    }
}

fn get4(ee: &mut Ee, addr: u32) -> [f32; 4] {
    [0, 1, 2, 3].map(|i| ee.f32_at(addr + 4 * i))
}

fn put_matrix(ee: &mut Ee, addr: u32, m: &[f32; 16]) {
    for (i, x) in m.iter().enumerate() {
        ee.wf32(addr + 4 * i as u32, *x);
    }
}

/// A view matrix for the tests: half the time the real builder's (a rotation), else sixteen random words.
fn some_matrix(r: &mut Rng) -> [f32; 16] {
    if r.below(2) == 0 {
        let eye = [rf(r, 500.0), rf(r, 500.0), rf(r, 500.0)];
        let look = [rf(r, 500.0), rf(r, 500.0), rf(r, 500.0)];
        view_matrix(eye, look, [0.0, 1.0, 0.0])
    } else {
        let mut m = [0.0; 16];
        for x in &mut m {
            *x = rf(r, 2.0);
        }
        m
    }
}

#[test]
fn orig_17bbc0_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x17bbc0") else { return };
    let mut r = Rng::new(0x17bbc0);
    for n in 0..CASES {
        let scale = [4.0, 20.0, 100.0][r.below(3) as usize];
        let a = rf(&mut r, scale);
        call(&mut ee, 0x17bbc0, &[], &[a]);
        assert_f(n, &format!("fix_radian({a})"), ee.fpr[0], fix_radian(a));
    }
}

#[test]
fn orig_17bc30_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x17bc30") else { return };
    let mut r = Rng::new(0x17bc30);
    for n in 0..CASES {
        let (angle, wanted) = (rf(&mut r, 7.0), rf(&mut r, 7.0));
        // The step is the turn speed times the frame step: small, and the frame step is 1 or 2.
        let step = rf(&mut r, 1.5).abs() * if r.below(2) == 0 { 1.0 } else { 2.0 };
        call(&mut ee, 0x17bc30, &[], &[angle, wanted, step]);
        assert_f(n, &format!("turn_angle({angle}, {wanted}, {step})"), ee.fpr[0], turn_angle(angle, wanted, step));
    }
}

#[test]
fn orig_17be98_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x17be98") else { return };
    let mut r = Rng::new(0x17be98);
    for n in 0..CASES {
        let a = rf(&mut r, 12.0);
        put4(&mut ee, OUT, [9.0; 4]);
        call(&mut ee, 0x17be98, &[OUT as u64], &[a]);
        let (game, port) = (get4(&mut ee, OUT), rot_to_dir(a));
        for i in 0..4 {
            assert_f(n, &format!("word {i} of rot_to_dir({a})"), game[i], port[i]);
        }
    }
}

#[test]
fn orig_16a790_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x16a790") else { return };
    ee.hooks.insert(0x19ad98, |ee: &mut Ee| ee.gpr[2] = 0);
    let mut r = Rng::new(0x16a790);
    for n in 0..CASES {
        let a = rf(&mut r, 12.0);
        put4(&mut ee, ACT + 0x550, [9.0; 4]);
        call(&mut ee, 0x16a790, &[ACT as u64], &[a]);
        let (game, port) = (get4(&mut ee, ACT + 0x550), set_rot(a));
        for i in 0..4 {
            assert_f(n, &format!("word {i} of set_rot({a})"), game[i], port[i]);
        }
    }
}

#[test]
fn orig_16a948_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x16a948") else { return };
    let mut r = Rng::new(0x16a948);
    for n in 0..CASES {
        let dir = [rf(&mut r, 3.0), rf(&mut r, 3.0), rf(&mut r, 3.0), rf(&mut r, 3.0)];
        put4(&mut ee, OUT, dir);
        ee.wf32(ACT + 0x66c, 7.0);
        call(&mut ee, 0x16a948, &[ACT as u64, OUT as u64], &[]);
        assert_f(n, &format!("wanted yaw of {dir:?}"), ee.f32_at(ACT + 0x66c), wanted_yaw(dir));
    }
}

#[test]
fn orig_106f78_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x106f78") else { return };
    ee.hooks.insert(0x11b460, |_: &mut Ee| {});
    ee.hooks.insert(0x11b520, |_: &mut Ee| {});
    let mut r = Rng::new(0x106f78);
    for n in 0..CASES {
        let eye = [rf(&mut r, 800.0), rf(&mut r, 800.0), rf(&mut r, 800.0)];
        let look = [rf(&mut r, 800.0), rf(&mut r, 800.0), rf(&mut r, 800.0)];
        // The view block's up vector is (0, 1, 0) in the game; the others are checked as well.
        let up = if r.below(2) == 0 { [0.0, 1.0, 0.0] } else { [rf(&mut r, 1.0), rf(&mut r, 1.0), rf(&mut r, 1.0)] };
        if eye == look {
            continue;
        }
        put4(&mut ee, VIEW + 0x50, [eye[0], eye[1], eye[2], 1.0]);
        put4(&mut ee, VIEW + 0x60, [look[0], look[1], look[2], 1.0]);
        put4(&mut ee, VIEW + 0x70, [up[0], up[1], up[2], 0.0]);
        ee.wf32(VIEW + 0x80, 0.0);
        call(&mut ee, 0x106f78, &[VIEW as u64], &[]);
        let port = view_matrix(eye, look, up);
        for row in 0..3 {
            let game = get4(&mut ee, VIEW + 0x10 + 0x10 * row as u32);
            for c in 0..3 {
                assert_f(n, &format!("matrix [{row}][{c}] eye {eye:?} look {look:?} up {up:?}"), game[c], port[row * 4 + c]);
            }
        }
    }
}

#[test]
fn orig_177360_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x177360") else { return };
    ee.hooks.insert(0x1068c0, |ee: &mut Ee| ee.gpr[2] = VIEW as u128);
    let mut r = Rng::new(0x177360);
    for n in 0..CASES {
        let m = some_matrix(&mut r);
        put_matrix(&mut ee, VIEW + 0x10, &m);
        let w = match r.below(5) {
            0 => 0.0,
            1 => -rf(&mut r, 1.0).abs(),
            _ => rf(&mut r, 1.0).abs(),
        };
        let stick = [rf(&mut r, 1.0), rf(&mut r, 1.0), rf(&mut r, 1.0), w];
        put4(&mut ee, REC, stick);
        put4(&mut ee, OUT, [7.0; 4]);
        call(&mut ee, 0x177360, &[OUT as u64, REC as u64], &[]);
        let (game, port) = (get4(&mut ee, OUT), stick_through_view(&m, stick));
        for i in 0..4 {
            assert_f(n, &format!("word {i} of the stick {stick:?} through {m:?}"), game[i], port[i]);
        }
    }
}

/// The actor block of the tests: the pad record behind +0xb44 holding the stick at +0x9c0, the move vector at +0x10, the walk and
/// run speeds at +0x110 and +0x114, the wanted yaw at +0x66c.
fn lay_actor(ee: &mut Ee, stick: [f32; 4], wanted: f32, walk: f32, run: f32) {
    ee.w32(ACT + 0xb44, REC);
    put4(ee, REC + 0x9c0, stick);
    put4(ee, ACT + 0x10, [5.0; 4]);
    ee.wf32(ACT + 0x110, walk);
    ee.wf32(ACT + 0x114, run);
    ee.wf32(ACT + 0x66c, wanted);
}

#[test]
fn orig_1774d8_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x1774d8") else { return };
    ee.hooks.insert(0x1068c0, |ee: &mut Ee| ee.gpr[2] = VIEW as u128);
    let mut r = Rng::new(0x1774d8);
    for n in 0..CASES {
        let m = some_matrix(&mut r);
        put_matrix(&mut ee, VIEW + 0x10, &m);
        // Magnitudes either side of the walk limit 0.8 (the stick vector's own length is capped at 1).
        let w = match r.below(6) {
            0 => 0.0,
            1 => 0.8,
            2 => 0.79999995,
            3 => 0.8000001,
            _ => rf(&mut r, 1.0).abs(),
        };
        let stick = [rf(&mut r, 1.0), rf(&mut r, 1.0), 0.0, w];
        let (wanted, walk, run) = (rf(&mut r, 4.0), 1.0 + rf(&mut r, 1.0).abs(), 5.0 + rf(&mut r, 1.0).abs());
        lay_actor(&mut ee, stick, wanted, walk, run);
        call(&mut ee, 0x1774d8, &[ACT as u64], &[]);
        let (game, port) = (get4(&mut ee, ACT + 0x10), stick_vector(stick, &m, wanted, walk, run));
        for i in 0..4 {
            assert_f(n, &format!("word {i} of the move vector for stick {stick:?} wanted {wanted}"), game[i], port[i]);
        }
    }
}

#[test]
fn orig_1772f8_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x1772f8") else { return };
    let log: Rc<RefCell<Vec<u32>>> = Rc::default();
    for a in [0x1775e8u32, 0x1775a0, 0x177710, 0x1774d8] {
        let l = log.clone();
        ee.hooks.insert(a, move |_: &mut Ee| l.borrow_mut().push(a));
    }
    let mut r = Rng::new(0x1772f8);
    for n in 0..CASES {
        let id = [2, 0x18, 0x35, 0, 1, 3, 0x17, 0x19, 0x34, 0x36][r.below(10) as usize] as u32;
        let flags = if r.below(3) == 0 { 8 } else { r.word() as u32 & !8 } | if r.below(4) == 0 { 0x180 } else { 0 };
        ee.w32(ACT + 0xc, MOV);
        ee.w32(MOV + 4, id);
        ee.w32(MOV + 8, flags);
        log.borrow_mut().clear();
        call(&mut ee, 0x1772f8, &[ACT as u64], &[]);
        let called = log.borrow().clone();
        let game = match called.as_slice() {
            [0x1775e8] => StickReader::Flag8,
            [0x1775a0] => StickReader::Id2,
            [0x177710] => StickReader::Id18Or35,
            [0x1774d8] => StickReader::View,
            other => panic!("case {n}: the original called {other:x?}"),
        };
        assert_eq!(game, stick_reader(flags, id as i32), "case {n}: flags {flags:#x} id {id:#x}");
    }
}

fn lay_gate(ee: &mut Ee, r: &mut Rng) -> ([f32; 4], u32, u32, u32) {
    let comp = |r: &mut Rng| if r.below(3) == 0 { 0.0 } else { rf(r, 2.0) };
    let w = match r.below(4) {
        0 => 0.0,
        1 => -rf(r, 1.0).abs(),
        _ => rf(r, 1.0).abs(),
    };
    let dir = [comp(r), 0.0, comp(r), w];
    let f168 = [0, 4, 8, 12, r.word() as u32][r.below(5) as usize];
    let mover = [0, 0x100, 0x180, 0x80, r.word() as u32][r.below(5) as usize];
    let f160 = [0, 0x8000_0000, 0x7fff_ffff, r.word() as u32][r.below(4) as usize];
    ee.w32(ACT + 0xc, MOV);
    ee.w32(MOV + 8, mover);
    put4(ee, ACT + 0x10, dir);
    ee.w32(ACT + 0x160, f160);
    ee.w32(ACT + 0x168, f168);
    (dir, f168, mover, f160)
}

#[test]
fn orig_174c28_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x174c28") else { return };
    let mut r = Rng::new(0x174c28);
    for n in 0..CASES {
        let (dir, f168, mover, f160) = lay_gate(&mut ee, &mut r);
        let game = call(&mut ee, 0x174c28, &[ACT as u64], &[]) & 0xff != 0;
        assert_eq!(game, stick_gate(dir, f168, mover, f160), "case {n}: dir {dir:?} flags168 {f168:#x} mover {mover:#x} flags160 {f160:#x}");
    }
}

#[test]
fn orig_179108_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x179108") else { return };
    let look = Rc::new(RefCell::new(0u128));
    let l = look.clone();
    ee.hooks.insert(0x140f98, move |ee: &mut Ee| ee.gpr[2] = *l.borrow());
    let mut r = Rng::new(0x179108);
    for n in 0..CASES {
        let (dir, f168, mover, f160) = lay_gate(&mut ee, &mut r);
        let looking = r.below(2) == 0;
        *look.borrow_mut() = looking as u128;
        let game = call(&mut ee, 0x179108, &[ACT as u64], &[]) & 0xff != 0;
        let port = stick_may_move(stick_gate(dir, f168, mover, f160), looking);
        assert_eq!(game, port, "case {n}: dir {dir:?} flags168 {f168:#x} mover {mover:#x} flags160 {f160:#x} look-around {looking}");
    }
}

#[test]
fn orig_174bd8_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x174bd8") else { return };
    let allow = Rc::new(RefCell::new(0u128));
    let a = allow.clone();
    ee.hooks.insert(0x179108, move |ee: &mut Ee| ee.gpr[2] = *a.borrow());
    ee.hooks.insert(0x197118, |_: &mut Ee| {});
    // The slot +0x84 of the actor's vtable (the pointer at actor +0) points at the stand-in for 0x179108.
    ee.w32(ACT, VT);
    ee.w32(VT + 0x84, 0x179108);
    let mut r = Rng::new(0x174bd8);
    for n in 0..CASES {
        let dir = [rf(&mut r, 3.0), rf(&mut r, 3.0), rf(&mut r, 3.0), rf(&mut r, 3.0)];
        let before = rf(&mut r, 4.0);
        let may = r.below(2) == 0;
        *allow.borrow_mut() = may as u128;
        put4(&mut ee, ACT + 0x10, dir);
        ee.wf32(ACT + 0x66c, before);
        call(&mut ee, 0x174bd8, &[ACT as u64], &[]);
        assert_f(n, &format!("wanted yaw (dir {dir:?}, allowed {may})"), ee.f32_at(ACT + 0x66c), update_wanted_yaw(before, dir, may));
    }
}

/// Not an oracle test: the drawn view and the orbit yaw. A camera held by a wall sits where the orbit yaw does not say; the
/// stick pushed straight up goes along the drawn horizontal forward, and the port's old orbit mapping would not.
#[test]
fn a_camera_held_by_a_wall_gives_the_drawn_heading_not_the_orbit_yaw() {
    // Sora at the origin; the orbit yaw says the camera sits at yaw 212 degrees, the wall holds it so that the drawn line from the
    // camera to its look-at point points along yaw 214.2 degrees (the rig's tt21 numbers).
    let orbit = 212.0f32.to_radians();
    let drawn = 214.2f32.to_radians();
    let (eye, look) = ([700.0 * drawn.sin(), 220.0, 700.0 * drawn.cos()], [0.0, 90.0, 0.0]);
    let view = view_matrix(eye, look, [0.0, 1.0, 0.0]);
    let up = stick_through_view(&view, [0.0, -1.0, 0.0, 1.0]);
    let heading = up[0].atan2(up[2]);
    // The camera looks from yaw 214.2 toward the origin, so forward is yaw 214.2 + 180 degrees.
    let want = fix_radian(drawn + std::f32::consts::PI);
    assert!((heading - want).abs() < 1e-4, "heading {heading} want {want}");
    let orbit_heading = fix_radian(orbit + std::f32::consts::PI);
    assert!((heading - orbit_heading).abs() > 0.03, "the drawn heading must differ from the orbit yaw's");
    assert!((up[3] - 1.0).abs() < 1e-6 && up[1] == 0.0);
    // Stick right goes along the drawn right (a quarter turn clockwise from forward seen from above).
    let right = stick_through_view(&view, [1.0, 0.0, 0.0, 1.0]);
    let right_heading = right[0].atan2(right[2]);
    assert!((fix_radian(right_heading - heading) + std::f32::consts::FRAC_PI_2).abs() < 1e-4, "right {right_heading} forward {heading}");
}

/// The turn is the turn speed times the frame step: a frame of step 2 turns as far as two ticks of step 1, and the facing
/// record follows the angle.
#[test]
fn the_facing_turn_scales_with_the_frame_step() {
    let start = Facing { angle: 0.1, dir: set_rot(0.1) };
    let two_ticks = start.turn(2.0, 0.2, 1.0).turn(2.0, 0.2, 1.0);
    let one_frame = start.turn(2.0, 0.2, 2.0);
    assert!((two_ticks.angle - one_frame.angle).abs() < 1e-6 && (one_frame.angle - 0.5).abs() < 1e-6, "{two_ticks:?} {one_frame:?}");
    assert!((one_frame.dir[0] - 0.5f32.sin()).abs() < 1e-6 && (one_frame.dir[2] - 0.5f32.cos()).abs() < 1e-6 && one_frame.dir[3] == one_frame.angle);
    // Close to the wanted angle it stops on it, and it takes the short way round the wrap.
    assert_eq!(start.turn(0.15, 0.2, 1.0).angle, fix_radian(0.1 + fix_radian(0.05)));
    let wrapped = Facing { angle: 3.0, dir: set_rot(3.0) }.turn(-3.0, 0.1, 1.0);
    assert!(wrapped.angle > 3.0 || wrapped.angle < -3.0, "{wrapped:?}");
}
