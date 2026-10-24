//! Oracle tests for the field camera (WP-L2-P2): kh2-ee runs the original from the user's executable on a camera object laid out at
//! `0x355840` (the game's own address, so the task `FUN_00140be0` runs on it too), and `kh2_game_core::camera::rules` runs on the same
//! values through the port's `Rig`.
//!
//! The harness is hand-written, not `factory gen`: the functions work on the camera object, a pad record and a player block whose
//! fields the generator cannot name. It follows the factory's rules: the original runs from the executable, cases come from a seeded
//! generator (a failing case number reproduces), 2,000 or more cases per function, every case is compared, nothing is skipped inside
//! the loops.
//!
//! Layout of the camera object (docs/systems/player-camera/camera.md): `+0x00` smoothed look-at point, `+0x10` smoothed position,
//! `+0x20` smoothed (field of view, roll), `+0x30/+0x34` this tick's field of view and roll, `+0x38/+0x39` warp and reset flags,
//! `+0x3a` auto-camera, `+0x40` mode, `+0x44` the player, `+0x48` radius, `+0x4c/+0x50` yaw and target yaw, `+0x60/+0x70` wanted
//! position now and last tick, `+0x80/+0x90` wanted look-at point, `+0xa0/+0xb0` wanted (fov, roll), `+0xc8` default radius, `+0xd0`
//! and `+0xd4` radius range, `+0xd8` height factor, `+0xdc/+0xe0` default fov and roll. The pad record: `+0` and `+8` the logical
//! button words, `+0x2c` right stick magnitude, `+0x30/+0x34` left stick x and y, `+0x3c` left stick magnitude. The player block:
//! `+0x40` of its vtable (may the pad drive it), `+0x70` its position, `+0x114` its run speed.
//!
//! The port keeps the app's axes (Y up, z mirrored): a game vector `(x, y, z)` is the port's `(x, -y, -z)` and a game yaw is `PI`
//! minus the port's yaw. Comparisons that involve a yaw (sine, cosine, the angle wrap) are within 1e-3 cm plus 5e-4 of the radius (positions: the game rotates with its own sine polynomial `FUN_002fb460`, the port with libm) or
//! 4e-6 rad (angles);
//! everything else is compared bit for bit (a zero and a negative zero are one value).
//!
//! Stand-ins (stubs), all listed here. Hooks that answer from the case: `FUN_001678e8` (the pad record), `FUN_0016b960` (the player's
//! position address), `FUN_001831f0` (moved since last tick), `FUN_0016a890` (the player's facing), `FUN_0019ae30` and `FUN_0019ae68`
//! (player state flags; the port does not model the second), `FUN_0023ee68`-based stick tests `FUN_00157f98`, `FUN_00157fe8`,
//! `FUN_00158040`, `FUN_00158090` (the logical stick bits: they answer from the case's right stick, up and down for the zoom, left and
//! right for the yaw [inferred: the port decides by the stick's direction, the original by the logical bits the pad layer derives]),
//! `FUN_001580e8` (the camera config), the vtable slot `+0x40` of the player (controllable). Hooks that do nothing: the collisions of the
//! wanted position `FUN_0013fbc0` and of the smoothed one (`FUN_0013f8f8` answers "no hit", `FUN_00140398`, `FUN_0015a028`), the hand-off
//! of the finished view (`FUN_00106878`, `FUN_00106c48`, `FUN_001068c0`, `FUN_00107160`), and the event camera tests (`FUN_0015c998` and
//! `FUN_00106848` answer 0). The port's `cast` finds no wall and its blocker list is empty.
//!
//! Not covered: the camera modes other than the follow camera (the mode switch of `FUN_00140a80` is tested with mode 0), the script
//! flag at `+0x39`, Select (the look-around) and the event camera.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::f32::consts::PI;
use std::rc::Rc;

use bevy::math::{Vec2, Vec3};
use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_game_core::camera::rules::*;

const CAM: u32 = 0x355840;
const PAD: u32 = 0x0181_0000;
const PLAYER: u32 = 0x0182_0000;
const VT: u32 = 0x0183_0000;
const STUB: u32 = 0x0183_1000;
const OUT: u32 = 0x0184_0000;
const FRAME_STEP: u32 = 0x356f40;
const CASES: u32 = 2500;

/// What the hooks answer in the current case.
#[derive(Default)]
struct Env {
    moved: bool,
    facing: [f32; 4],
    right: Vec2,
    right_mag: f32,
    manual: bool,
    may_look: bool,
}

struct Machine {
    ee: Ee,
    env: Rc<RefCell<Env>>,
    params: Params,
}

fn machine(what: &str) -> Option<Machine> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let params = Params::from_exe(&elf).expect("the camera table");
    let env: Rc<RefCell<Env>> = Rc::default();
    ee.hooks.insert(0x1678e8, |ee: &mut Ee| ee.gpr[2] = PAD as u128);
    ee.hooks.insert(0x16b960, |ee: &mut Ee| ee.gpr[2] = (PLAYER + 0x70) as u128);
    let e = env.clone();
    ee.hooks.insert(0x1831f0, move |ee: &mut Ee| ee.gpr[2] = e.borrow().moved as u128);
    let e = env.clone();
    ee.hooks.insert(0x16a890, move |ee: &mut Ee| {
        let dst = ee.gpr[4] as u32;
        for (i, v) in e.borrow().facing.iter().enumerate() {
            ee.wf32(dst + 4 * i as u32, *v);
        }
        ee.gpr[2] = dst as u128;
    });
    let e = env.clone();
    ee.hooks.insert(0x19ae30, move |ee: &mut Ee| ee.gpr[2] = e.borrow().may_look as u128);
    ee.hooks.insert(0x19ae68, |ee: &mut Ee| ee.gpr[2] = 0);
    // The logical stick bits, from the right stick of the case: up/down zoom, left/right turn (stick left is the game's yaw increase).
    let e = env.clone();
    ee.hooks.insert(0x157f98, move |ee: &mut Ee| ee.gpr[2] = (e.borrow().right_mag > 0.0 && e.borrow().right.y < 0.0) as u128);
    let e = env.clone();
    ee.hooks.insert(0x157fe8, move |ee: &mut Ee| ee.gpr[2] = (e.borrow().right_mag > 0.0 && e.borrow().right.y > 0.0) as u128);
    let e = env.clone();
    ee.hooks.insert(0x158040, move |ee: &mut Ee| ee.gpr[2] = (e.borrow().right_mag > 0.0 && e.borrow().right.x < 0.0) as u128);
    let e = env.clone();
    ee.hooks.insert(0x158090, move |ee: &mut Ee| ee.gpr[2] = (e.borrow().right_mag > 0.0 && e.borrow().right.x > 0.0) as u128);
    let e = env.clone();
    ee.hooks.insert(0x1580e8, move |ee: &mut Ee| ee.gpr[2] = e.borrow().manual as u128);
    ee.hooks.insert(STUB, |ee: &mut Ee| ee.gpr[2] = 1);
    for a in [0x13fbc0u32, 0x140398, 0x15a028, 0x106878, 0x106c48, 0x1068c0, 0x107160] {
        ee.hooks.insert(a, |_: &mut Ee| {});
    }
    for a in [0x13f8f8u32, 0x15c998, 0x106848] {
        ee.hooks.insert(a, |ee: &mut Ee| ee.gpr[2] = 0);
    }
    // The static initialiser `FUN_00141018` copies the executable's camera constants into the RAM copies the functions read
    // (the yaw limit, the stick rates, the auto-camera rate; the identity matrix) and constructs the object once; the cases overwrite what they use.
    // `FUN_0011bd58` builds the identity matrix `FUN_0011b460` copies.
    call(&mut ee, 0x11bd58, &[1, 0xffff], &[]);
    call(&mut ee, 0x141018, &[1, 0xffff], &[]);
    ee.w32(PLAYER, VT);
    ee.w32(VT + 0x40, STUB);
    ee.w32(CAM + 0x44, PLAYER);
    Some(Machine { ee, env, params })
}

fn call(ee: &mut Ee, addr: u32, args: &[u64], fargs: &[f32]) -> u64 {
    ee.call(addr, args, fargs, STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
}

/// A float in -scale..scale with the edge values now and then.
fn rf(r: &mut Rng, scale: f32) -> f32 {
    match r.below(10) {
        0 => 0.0,
        1 => scale,
        2 => -scale,
        _ => ((r.word() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0) * scale,
    }
}

/// A float in -scale..scale that is never zero (a facing exactly along an axis has two behind-yaws a full turn apart: the sign of
/// the zero picks one, and the port's mirror picks the other).
fn nz(r: &mut Rng, scale: f32) -> f32 {
    (0.001 + rp(r, scale)) * if r.below(2) == 0 { 1.0 } else { -1.0 }
}

/// A float in 0..scale.
fn rp(r: &mut Rng, scale: f32) -> f32 {
    rf(r, scale).abs()
}

fn same(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a == 0.0 && b == 0.0)
}

fn assert_same(case: u32, what: &str, game: f32, port: f32) {
    assert!(same(game, port), "case {case}: {what}: game {game} ({:#x}) port {port} ({:#x})", game.to_bits(), port.to_bits());
}

fn assert_near(case: u32, what: &str, game: f32, port: f32, tol: f32) {
    assert!((game - port).abs() <= tol, "case {case}: {what}: game {game} port {port} (tolerance {tol})");
}

/// The port's vector for a game vector, and back (the same operation).
fn gp(v: [f32; 3]) -> Vec3 {
    Vec3::new(v[0], -v[1], -v[2])
}

fn put3(ee: &mut Ee, addr: u32, v: [f32; 3]) {
    for (i, x) in v.iter().enumerate() {
        ee.wf32(addr + 4 * i as u32, *x);
    }
    ee.wf32(addr + 12, 1.0);
}

fn get3(ee: &mut Ee, addr: u32) -> [f32; 3] {
    [0, 1, 2].map(|i| ee.f32_at(addr + 4 * i))
}

fn assert_vec(case: u32, what: &str, game: [f32; 3], port: Vec3) {
    let port = [port.x, -port.y, -port.z];
    for i in 0..3 {
        assert_same(case, &format!("{what}[{i}]"), game[i], port[i]);
    }
}

fn assert_vec_near(case: u32, what: &str, game: [f32; 3], port: Vec3, tol: f32) {
    let port = [port.x, -port.y, -port.z];
    for i in 0..3 {
        assert_near(case, &format!("{what}[{i}]"), game[i], port[i], tol);
    }
}

/// Angles equal modulo a full turn.
fn assert_angle(case: u32, what: &str, game: f32, port_game_yaw: f32) {
    let d = wrap(game - port_game_yaw);
    assert!(d.abs() <= 4e-6, "case {case}: {what}: game {game} port {port_game_yaw} (difference {d})");
}

/// How far a position that goes through the game's sine and cosine may sit from the port's (libm): the game rotates with its own
/// polynomial (`FUN_002fb460`), its sine is a series and its cosine the root of 1 - sine squared (`FUN_002fb628`), so the cosine is poor where it is small (yaw near
/// a quarter turn): the error reaches the square root of the single-precision epsilon (3.5e-4) of the radius at a quarter turn, 5e-4 allowed. The rotation is not ported (gap).
fn pos_tol(radius: f32) -> f32 {
    1e-3 + 5e-4 * radius
}

/// The game's yaw for the port's.
fn gy(yaw_port: f32) -> f32 {
    PI - yaw_port
}

/// A camera row with its numbers in the original's field order: (default radius, min, max, height factor, default fov).
fn some_row(r: &mut Rng) -> Row {
    let min = 30.0 + rp(r, 200.0);
    let radius = min + rp(r, 300.0);
    let max = radius + rp(r, 400.0);
    Row { fov: 0.5 + rp(r, 1.5), radius, lock_radius: radius, min_radius: min, max_radius: max, height_k: rf(r, 0.002) }
}

fn lay_row(ee: &mut Ee, row: &Row) {
    ee.wf32(CAM + 0xc8, row.radius);
    ee.wf32(CAM + 0xd0, row.min_radius);
    ee.wf32(CAM + 0xd4, row.max_radius);
    ee.wf32(CAM + 0xd8, row.height_k);
    ee.wf32(CAM + 0xdc, row.fov);
    ee.wf32(CAM + 0xe0, 0.0);
}

/// A port rig whose fields are the camera object's: a rig made behind a standing subject, then every public field overwritten.
struct Case {
    row: Row,
    radius: f32,
    yaw_game: f32,
    target_game: f32,
    look: [f32; 3],
    pos: [f32; 3],
    wanted_look: [f32; 3],
    prev_look: [f32; 3],
    wanted_pos: [f32; 3],
    prev_pos: [f32; 3],
    fov: f32,
    wanted_fov: f32,
    prev_fov: f32,
}

fn some_case(r: &mut Rng) -> Case {
    let row = some_row(r);
    let v3 = |r: &mut Rng| [rf(r, 1500.0), rf(r, 600.0), rf(r, 1500.0)];
    let radius = row.min_radius + rp(r, row.max_radius - row.min_radius);
    Case {
        row,
        radius: if r.below(8) == 0 { row.radius } else { radius },
        yaw_game: rf(r, 3.1),
        target_game: rf(r, 3.1),
        look: v3(r),
        pos: v3(r),
        wanted_look: v3(r),
        prev_look: v3(r),
        wanted_pos: v3(r),
        prev_pos: v3(r),
        fov: 0.5 + rp(r, 1.5),
        wanted_fov: row.fov,
        prev_fov: 0.5 + rp(r, 1.5),
    }
}

fn lay_case(ee: &mut Ee, c: &Case) {
    lay_row(ee, &c.row);
    ee.wf32(CAM + 0x48, c.radius);
    ee.wf32(CAM + 0x4c, c.yaw_game);
    ee.wf32(CAM + 0x50, c.target_game);
    put3(ee, CAM, c.look);
    put3(ee, CAM + 0x10, c.pos);
    put3(ee, CAM + 0x80, c.wanted_look);
    put3(ee, CAM + 0x90, c.prev_look);
    put3(ee, CAM + 0x60, c.wanted_pos);
    put3(ee, CAM + 0x70, c.prev_pos);
    ee.wf32(CAM + 0x20, c.fov);
    ee.wf32(CAM + 0x24, 0.0);
    ee.wf32(CAM + 0x28, 0.0);
    ee.wf32(CAM + 0x30, c.row.fov);
    ee.wf32(CAM + 0x34, 0.0);
    ee.wf32(CAM + 0xa0, c.wanted_fov);
    ee.wf32(CAM + 0xa4, 0.0);
    ee.wf32(CAM + 0xa8, 0.0);
    ee.wf32(CAM + 0xb0, c.prev_fov);
    ee.wf32(CAM + 0xb4, 0.0);
    ee.wf32(CAM + 0xb8, 0.0);
    ee.write(CAM + 0x38, &[0, 0, 0]);
    ee.w32(CAM + 0x40, 0);
}

fn rig_of(p: &Params, c: &Case) -> Rig {
    let subject = Subject { pos: Vec3::ZERO, facing: Vec3::Z, moved: false, run_speed: 8.0, eye_height: 0.0, can_look: true };
    let mut rig = Rig::new(p, c.row, &subject, &|_, _| None);
    rig.row = c.row;
    rig.radius = c.radius;
    rig.yaw = gy(c.yaw_game);
    rig.target_yaw = gy(c.target_game);
    rig.look = gp(c.look);
    rig.pos = gp(c.pos);
    rig.fov = c.fov;
    rig.roll = 0.0;
    rig.wanted_look = gp(c.wanted_look);
    rig.prev_wanted_look = gp(c.prev_look);
    rig.wanted_pos = gp(c.wanted_pos);
    rig.prev_wanted_pos = gp(c.prev_pos);
    rig.wanted_fov = c.wanted_fov;
    rig.prev_wanted_fov = c.prev_fov;
    rig
}

fn set_step(ee: &mut Ee, r: &mut Rng) -> f32 {
    let step = if r.below(2) == 0 { 1.0 } else { 2.0 };
    ee.wf32(FRAME_STEP, step);
    step
}

#[test]
fn orig_13f740_matches_the_game() {
    let Some(mut m) = machine("oracle test of 0x13f740") else { return };
    let mut r = Rng::new(0x13f740);
    for n in 0..CASES {
        let pos = [rf(&mut r, 3000.0), rf(&mut r, 1000.0), rf(&mut r, 3000.0)];
        put3(&mut m.ee, PLAYER + 0x70, pos);
        put3(&mut m.ee, CAM + 0x80, [9.0; 3]);
        call(&mut m.ee, 0x13f740, &[CAM as u64], &[]);
        let game = get3(&mut m.ee, CAM + 0x80);
        assert_vec(n, &format!("look-at point of {pos:?}"), game, look_point(gp(pos)));
    }
}

#[test]
fn orig_13f600_matches_the_game() {
    let Some(mut m) = machine("oracle test of 0x13f600") else { return };
    let mut r = Rng::new(0x13f600);
    for n in 0..CASES {
        let v3 = |r: &mut Rng| [rf(r, 1500.0), rf(r, 600.0), rf(r, 1500.0)];
        let (cur, new, prev) = (v3(&mut r), if r.below(6) == 0 { [0.0; 3] } else { v3(&mut r) }, v3(&mut r));
        let new = if r.below(8) == 0 { cur } else { new };
        let t = 0.05 + rp(&mut r, 0.1);
        let (k, c, l) = (2.5 + rf(&mut r, 1.0), 0.5 + rf(&mut r, 0.5), if r.below(2) == 0 { 1.0 } else { 0.001 });
        put3(&mut m.ee, OUT, cur);
        put3(&mut m.ee, OUT + 0x10, new);
        put3(&mut m.ee, OUT + 0x20, prev);
        put3(&mut m.ee, OUT + 0x30, [7.0; 3]);
        call(&mut m.ee, 0x13f600, &[(OUT + 0x30) as u64, OUT as u64, (OUT + 0x10) as u64, (OUT + 0x20) as u64], &[t, k, c, l]);
        let game = get3(&mut m.ee, OUT + 0x30);
        assert_vec(n, &format!("spring of {cur:?} {new:?} {prev:?} t {t} k {k} c {c} l {l}"), game, spring(gp(cur), gp(new), gp(prev), t, k, c, l));
    }
}

#[test]
fn orig_140768_matches_the_game() {
    let Some(mut m) = machine("oracle test of 0x140768") else { return };
    let mut r = Rng::new(0x140768);
    for n in 0..CASES {
        let c = some_case(&mut r);
        lay_case(&mut m.ee, &c);
        let mut rig = rig_of(&m.params, &c);
        call(&mut m.ee, 0x140768, &[CAM as u64], &[]);
        rig.remember_wanted();
        assert_vec(n, "last tick's wanted look-at point", get3(&mut m.ee, CAM + 0x90), rig.prev_wanted_look);
        assert_vec(n, "last tick's wanted position", get3(&mut m.ee, CAM + 0x70), rig.prev_wanted_pos);
        assert_same(n, "last tick's wanted fov", m.ee.f32_at(CAM + 0xb0), rig.prev_wanted_fov);
    }
}

#[test]
fn orig_13fd08_matches_the_game() {
    let Some(mut m) = machine("oracle test of 0x13fd08") else { return };
    let mut r = Rng::new(0x13fd08);
    for n in 0..CASES {
        let c = some_case(&mut r);
        lay_case(&mut m.ee, &c);
        let step = set_step(&mut m.ee, &mut r);
        let (right, mag) = (Vec2::new(rf(&mut r, 1.0), rf(&mut r, 1.0)), rp(&mut r, 1.0));
        *m.env.borrow_mut() = Env { right, right_mag: mag, ..Env::default() };
        m.ee.wf32(PAD + 0x2c, mag);
        call(&mut m.ee, 0x13fd08, &[CAM as u64, PAD as u64], &[step]);
        let mut rig = rig_of(&m.params, &c);
        rig.stick_zoom(&Sticks { left: Vec2::ZERO, left_mag: 0.0, right, right_mag: mag }, step);
        assert_same(n, &format!("radius (stick {right:?} {mag}, step {step}, range {}..{}, from {})", c.row.min_radius, c.row.max_radius, c.radius), m.ee.f32_at(CAM + 0x48), rig.radius);
    }
}

#[test]
fn orig_140068_matches_the_game() {
    let Some(mut m) = machine("oracle test of 0x140068") else { return };
    let mut r = Rng::new(0x140068);
    for n in 0..CASES {
        let mut c = some_case(&mut r);
        if r.below(4) == 0 {
            c.radius = c.row.radius + rf(&mut r, 1.2);
        }
        lay_case(&mut m.ee, &c);
        let step = set_step(&mut m.ee, &mut r);
        call(&mut m.ee, 0x140068, &[CAM as u64], &[step]);
        let mut rig = rig_of(&m.params, &c);
        rig.relax_radius(c.row.radius, step);
        assert_same(n, &format!("radius (step {step}, default {}, from {})", c.row.radius, c.radius), m.ee.f32_at(CAM + 0x48), rig.radius);
    }
}

/// A pad record with the sticks of a case: left x, y, magnitude and the right stick's magnitude.
fn lay_pad(ee: &mut Ee, left: Vec2, left_mag: f32, right_mag: f32, r3: bool) {
    ee.wf32(PAD + 0x30, left.x);
    ee.wf32(PAD + 0x34, left.y);
    ee.wf32(PAD + 0x3c, left_mag);
    ee.wf32(PAD + 0x2c, right_mag);
    ee.w32(PAD, 0);
    ee.w32(PAD + 4, 0);
    ee.w32(PAD + 8, if r3 { 0x20000 } else { 0 });
    ee.w32(PAD + 12, 0);
}

/// Left stick components that sit around the 0.9 limit now and then.
fn left_stick(r: &mut Rng) -> (Vec2, f32) {
    let y = match r.below(8) {
        0 => 0.9,
        1 => 0.89999998,
        2 => 0.90000004,
        _ => rf(r, 1.0),
    };
    let mag = if r.below(5) == 0 { 0.0 } else { rp(r, 1.0) };
    (Vec2::new(rf(r, 1.0), y), mag)
}

#[test]
fn orig_140128_matches_the_game() {
    let Some(mut m) = machine("oracle test of 0x140128") else { return };
    let mut r = Rng::new(0x140128);
    for n in 0..CASES {
        let mut c = some_case(&mut r);
        if r.below(3) == 0 {
            c.radius = c.row.radius + rf(&mut r, 3.0);
        }
        lay_case(&mut m.ee, &c);
        let step = set_step(&mut m.ee, &mut r);
        let (left, left_mag) = left_stick(&mut r);
        let right_mag = if r.below(3) == 0 { rp(&mut r, 1.0) } else { 0.0 };
        let moved = r.below(4) != 0;
        let run_speed = if r.below(2) == 0 { 8.0 } else { 6.0 + rp(&mut r, 6.0) };
        let facing = [nz(&mut r, 1.0), 0.0, nz(&mut r, 1.0), 0.0];
        *m.env.borrow_mut() = Env { moved, facing, ..Env::default() };
        lay_pad(&mut m.ee, left, left_mag, right_mag, false);
        m.ee.wf32(PLAYER + 0x114, run_speed);
        call(&mut m.ee, 0x140128, &[CAM as u64, PAD as u64], &[step]);
        let mut rig = rig_of(&m.params, &c);
        let sticks = Sticks { left, left_mag, right: Vec2::ZERO, right_mag };
        rig.auto_camera(&m.params, &sticks, gp([facing[0], facing[1], facing[2]]), moved, run_speed, step);
        let what = format!("(left {left:?} {left_mag}, right {right_mag}, moved {moved}, run {run_speed}, step {step}, facing {facing:?})");
        assert_angle(n, &format!("yaw {what}"), m.ee.f32_at(CAM + 0x4c), gy(rig.yaw));
        assert_angle(n, &format!("target yaw {what}"), m.ee.f32_at(CAM + 0x50), gy(rig.target_yaw));
        assert_same(n, &format!("radius {what}"), m.ee.f32_at(CAM + 0x48), rig.radius);
    }
}

#[test]
fn orig_13f7a0_matches_the_game() {
    let Some(mut m) = machine("oracle test of 0x13f7a0") else { return };
    let mut r = Rng::new(0x13f7a0);
    for n in 0..CASES {
        let mut c = some_case(&mut r);
        if r.below(4) == 0 {
            c.target_game = c.yaw_game + rf(&mut r, 0.2);
        }
        c.wanted_fov = c.row.fov;
        lay_case(&mut m.ee, &c);
        let step = set_step(&mut m.ee, &mut r);
        let snap = r.below(3) == 0;
        call(&mut m.ee, 0x13f7a0, &[CAM as u64, (CAM + 0x80) as u64, snap as u64], &[]);
        let mut rig = rig_of(&m.params, &c);
        let wp = rig.place_wanted(&m.params, rig.wanted_look, snap, false, step);
        let what = format!("(yaw {} -> {}, radius {}, snap {snap}, step {step})", c.yaw_game, c.target_game, c.radius);
        assert_angle(n, &format!("yaw {what}"), m.ee.f32_at(CAM + 0x4c), gy(rig.yaw));
        assert_vec_near(n, &format!("wanted position {what}"), get3(&mut m.ee, CAM + 0x60), wp, pos_tol(c.radius));
        assert_vec_near(n, "wanted position field", get3(&mut m.ee, CAM + 0x60), rig.wanted_pos, pos_tol(c.radius));
        if snap {
            assert_vec_near(n, &format!("smoothed position {what}"), get3(&mut m.ee, CAM + 0x10), rig.pos, pos_tol(c.radius));
            assert_vec(n, "smoothed look-at point", get3(&mut m.ee, CAM), rig.look);
            assert_vec_near(n, "last tick's wanted position", get3(&mut m.ee, CAM + 0x70), rig.prev_wanted_pos, pos_tol(c.radius));
            assert_same(n, "smoothed fov", m.ee.f32_at(CAM + 0x20), rig.fov);
        } else {
            assert_vec(n, "smoothed position untouched", get3(&mut m.ee, CAM + 0x10), rig.pos);
        }
    }
}

/// The smoothing branch of `FUN_001407b8` with the collisions stubbed: the second argument 0 (a normal tick: the spring time grows
/// with the player's run speed) and 1 (a forced tick: 0.07).
#[test]
fn the_smoothing_of_1407b8_matches_the_game() {
    let Some(mut m) = machine("oracle test of the smoothing of 0x1407b8") else { return };
    let mut r = Rng::new(0x1407b8);
    for n in 0..CASES {
        let c = some_case(&mut r);
        lay_case(&mut m.ee, &c);
        let forced = r.below(3) == 0;
        let run_speed = if r.below(2) == 0 { 8.0 } else { 4.0 + rp(&mut r, 10.0) };
        m.ee.wf32(PLAYER + 0x114, run_speed);
        call(&mut m.ee, 0x1407b8, &[CAM as u64, forced as u64], &[]);
        let mut rig = rig_of(&m.params, &c);
        rig.smooth(smoothing_time(run_speed, forced));
        let what = format!("(forced {forced}, run {run_speed})");
        assert_vec(n, &format!("smoothed look-at point {what}"), get3(&mut m.ee, CAM), rig.look);
        assert_vec(n, &format!("smoothed position {what}"), get3(&mut m.ee, CAM + 0x10), rig.pos);
        assert_same(n, &format!("smoothed fov {what}"), m.ee.f32_at(CAM + 0x20), rig.fov);
        assert_same(n, &format!("smoothed roll {what}"), m.ee.f32_at(CAM + 0x24), rig.roll);
    }
}

/// What a case of the follow camera feeds: the pad, the player and the flags.
struct Follow {
    pos: [f32; 3],
    facing: [f32; 4],
    left: Vec2,
    left_mag: f32,
    right: Vec2,
    right_mag: f32,
    r3: bool,
    manual: bool,
    moved: bool,
    run_speed: f32,
    step: f32,
}

fn some_follow(r: &mut Rng) -> Follow {
    let (left, left_mag) = left_stick(r);
    let right_mag = if r.below(2) == 0 { rp(r, 1.0) } else { 0.0 };
    Follow {
        pos: [rf(r, 2000.0), rf(r, 300.0), rf(r, 2000.0)],
        facing: [nz(r, 1.0), 0.0, nz(r, 1.0), 0.0],
        left,
        left_mag,
        right: Vec2::new(rf(r, 1.0), rf(r, 1.0)),
        right_mag,
        r3: r.below(6) == 0,
        manual: r.below(5) == 0,
        moved: r.below(4) != 0,
        run_speed: if r.below(2) == 0 { 8.0 } else { 4.0 + rp(r, 10.0) },
        step: if r.below(2) == 0 { 1.0 } else { 2.0 },
    }
}

fn lay_follow(m: &mut Machine, f: &Follow) {
    *m.env.borrow_mut() = Env { moved: f.moved, facing: f.facing, right: f.right, right_mag: f.right_mag, manual: f.manual, may_look: true };
    lay_pad(&mut m.ee, f.left, f.left_mag, f.right_mag, f.r3);
    put3(&mut m.ee, PLAYER + 0x70, f.pos);
    m.ee.wf32(PLAYER + 0x114, f.run_speed);
    m.ee.wf32(FRAME_STEP, f.step);
    m.ee.write(CAM + 0x3a, &[1]);
}

fn port_input<'a>(f: &Follow, cast: Cast<'a>) -> TickIn<'a> {
    TickIn {
        sticks: Sticks { left: f.left, left_mag: f.left_mag, right: f.right, right_mag: f.right_mag },
        reset: f.r3,
        select: false,
        subject: Subject {
            pos: gp(f.pos),
            facing: gp([f.facing[0], f.facing[1], f.facing[2]]),
            moved: f.moved,
            run_speed: f.run_speed,
            eye_height: 0.0,
            can_look: true,
        },
        lock: None,
        zoom: 0.0,
        auto: !f.manual,
        warp: false,
        step: f.step,
        cast,
        blockers: &[],
    }
}

/// One tick of the follow camera without the collision of the wanted position (stubbed): the stick turn and zoom, the reset, the
/// auto-camera, the look-at point and the wanted position.
#[test]
fn the_follow_tick_of_1402c8_matches_the_game() {
    let Some(mut m) = machine("oracle test of 0x1402c8") else { return };
    let mut r = Rng::new(0x1402c8);
    let no_wall = |_: Vec3, _: Vec3| None;
    for n in 0..CASES {
        let mut c = some_case(&mut r);
        if r.below(4) == 0 {
            c.target_game = c.yaw_game + rf(&mut r, 0.2);
        }
        lay_case(&mut m.ee, &c);
        let f = some_follow(&mut r);
        lay_follow(&mut m, &f);
        call(&mut m.ee, 0x1402c8, &[CAM as u64], &[]);
        let mut rig = rig_of(&m.params, &c);
        let wp = rig.follow_tick(&m.params, &port_input(&f, &no_wall));
        let what = format!("(left {:?} {}, right {:?} {}, r3 {}, manual {}, moved {}, run {}, step {})", f.left, f.left_mag, f.right, f.right_mag, f.r3, f.manual, f.moved, f.run_speed, f.step);
        assert_angle(n, &format!("yaw {what}"), m.ee.f32_at(CAM + 0x4c), gy(rig.yaw));
        assert_angle(n, &format!("target yaw {what}"), m.ee.f32_at(CAM + 0x50), gy(rig.target_yaw));
        assert_same(n, &format!("radius {what}"), m.ee.f32_at(CAM + 0x48), rig.radius);
        assert_vec(n, &format!("wanted look-at point {what}"), get3(&mut m.ee, CAM + 0x80), rig.wanted_look);
        assert_vec_near(n, &format!("wanted position {what}"), get3(&mut m.ee, CAM + 0x60), wp, pos_tol(c.radius));
        assert_vec_near(n, &format!("smoothed position {what}"), get3(&mut m.ee, CAM + 0x10), rig.pos, pos_tol(c.radius));
        assert_vec(n, &format!("smoothed look-at point {what}"), get3(&mut m.ee, CAM), rig.look);
    }
}

/// The task `FUN_00140be0` (`FUN_00140a80` on the global camera object) in mode 0, with the collisions and the view hand-off stubbed.
#[test]
fn the_camera_task_of_140be0_matches_the_game_in_follow_mode() {
    let Some(mut m) = machine("oracle test of 0x140be0 and 0x140a80") else { return };
    let mut r = Rng::new(0x140be0);
    let no_wall = |_: Vec3, _: Vec3| None;
    for n in 0..CASES {
        let mut c = some_case(&mut r);
        if r.below(4) == 0 {
            c.target_game = c.yaw_game + rf(&mut r, 0.2);
        }
        lay_case(&mut m.ee, &c);
        let f = some_follow(&mut r);
        lay_follow(&mut m, &f);
        call(&mut m.ee, 0x140be0, &[], &[]);
        let mut rig = rig_of(&m.params, &c);
        rig.tick(&m.params, &port_input(&f, &no_wall));
        let what = format!("(left {:?} {}, right {:?} {}, r3 {}, manual {}, moved {}, run {}, step {})", f.left, f.left_mag, f.right, f.right_mag, f.r3, f.manual, f.moved, f.run_speed, f.step);
        assert_angle(n, &format!("yaw {what}"), m.ee.f32_at(CAM + 0x4c), gy(rig.yaw));
        assert_same(n, &format!("radius {what}"), m.ee.f32_at(CAM + 0x48), rig.radius);
        assert_vec(n, &format!("wanted look-at point {what}"), get3(&mut m.ee, CAM + 0x80), rig.wanted_look);
        assert_vec_near(n, &format!("wanted position {what}"), get3(&mut m.ee, CAM + 0x60), rig.wanted_pos, pos_tol(c.radius));
        assert_vec_near(n, &format!("last tick's wanted position {what}"), get3(&mut m.ee, CAM + 0x70), rig.prev_wanted_pos, pos_tol(c.radius));
        assert_vec_near(n, &format!("smoothed position {what}"), get3(&mut m.ee, CAM + 0x10), rig.pos, pos_tol(c.radius));
        assert_vec_near(n, &format!("smoothed look-at point {what}"), get3(&mut m.ee, CAM), rig.look, pos_tol(c.radius));
        assert_same(n, &format!("smoothed fov {what}"), m.ee.f32_at(CAM + 0x20), rig.fov);
    }
}
