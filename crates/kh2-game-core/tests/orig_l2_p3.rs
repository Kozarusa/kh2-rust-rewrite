//! Oracle tests for the ground mover and the mover table (WP-L2-P3): kh2-ee runs the original from the user's executable and
//! `kh2_game_core::motor` runs on the same inputs.
//!
//! The harness is hand-written, not `factory gen`: the functions work on an actor block whose fields the generator cannot name, so the
//! actor is laid out here at the offsets the port documents and every case is compared. It follows the factory's rules: the original
//! runs from the executable, cases come from a seeded generator (a failing case number reproduces), 2,000 cases per function, every
//! case is compared, nothing is skipped inside the loops.
//!
//! Actor block of the tests: `+0x10` move vector, `+0x1c` its speed word, `+0x20` velocity, `+0xf0` a float, `+0x100`/`+0x104` the
//! words `FUN_0017ae50` leaves, `+0x108` flags, `+0x110`/`+0x114` walk and run speed, `+0x12c` the row pointer, `+0x154` the motion id
//! last requested, `+0x160`/`+0x164` motion flags, `+0x588` class flags, `+0x810` a word.
//!
//! Stand-ins (stubs), all listed here, record their arguments and do nothing else:
//! - `FUN_0016baa0` (footstep sound), `FUN_001b1c90` (dust), `FUN_0017e2d8` (motion request): the tests of 0x17af10 and 0x17aeb8.
//!   `FUN_001699a8` (part number) answers the case's value there.
//! - `FUN_0016bd98` (jump height and motion id lookup) answers the case's values, `FUN_0017b278` (jump launch with a vector),
//!   `FUN_0017e220` (motion request): the tests of 0x17ade0 and 0x17ae50.
//! - `FUN_0017b278`, `FUN_0017b1e0` (jump launches) and `FUN_0016b698` (action change): the tests of 0x1796a8 and 0x1796e8.
//! - `FUN_001832b8` (the jump mover's step) in the test of 0x17b040.
//! - `FUN_0030fe80` (exit handler registry) in the test of 0x179718.
//! The test of 0x179880 runs the whole initialiser, base constructors included.
//! Floats are compared within the rounding of the two FPUs (relative 1e-5) unless bit equal.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_game_core::motor::*;

const ACT: u32 = 0x0180_0000;
const ROW: u32 = 0x0181_0000;
const OUT: u32 = 0x0182_0000;
const VEC: u32 = 0x0183_0000;
const LOCAL: u32 = 0x0184_0000;
const FRAME_STEP: u32 = 0x356f40;
const CASES: u32 = 2000;

fn machine(what: &str) -> Option<(Ee, Vec<u8>)> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some((ee, elf))
}

fn call(ee: &mut Ee, addr: u32, args: &[u64], fargs: &[f32]) -> u64 {
    ee.call(addr, args, fargs, STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
}

fn feq(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a == 0.0 && b == 0.0) || (a - b).abs() <= 1e-5 * (1.0 + a.abs().max(b.abs()))
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

/// A float in 0..scale, never zero.
fn rp(r: &mut Rng, scale: f32) -> f32 {
    0.001 + rf(r, scale).abs()
}

fn put4(ee: &mut Ee, addr: u32, v: [f32; 4]) {
    for (i, x) in v.iter().enumerate() {
        ee.wf32(addr + 4 * i as u32, *x);
    }
}

fn get4(ee: &mut Ee, addr: u32) -> [f32; 4] {
    [0, 1, 2, 3].map(|i| ee.f32_at(addr + 4 * i))
}

fn assert_v4(case: u32, what: &str, game: [f32; 4], port: [f32; 4]) {
    for i in 0..4 {
        assert_f(case, &format!("word {i} of {what}"), game[i], port[i]);
    }
}

fn consts(elf: &[u8]) -> GroundConsts {
    GroundConsts::from_exe(elf).expect("the ground mover constants")
}

/// What the stand-ins saw: (function, first argument, float argument 12 bits).
type Log = Rc<RefCell<Vec<(u32, u32, u32)>>>;

#[test]
fn orig_17aeb8_matches_the_game() {
    let Some((mut ee, _)) = machine("oracle test of 0x17aeb8") else { return };
    let log: Log = Rc::default();
    let l = log.clone();
    ee.hooks.insert(0x17e2d8, move |ee: &mut Ee| l.borrow_mut().push((0x17e2d8, ee.gpr[5] as u32, ee.fpr[12].to_bits())));
    let mut r = Rng::new(0x17aeb8);
    for n in 0..CASES {
        let flags = [0, 4, 0x10, 0x14, 8, 0xffff_ffff, r.word() as u32][r.below(7) as usize];
        let class = r.below(3) as u8;
        ee.w32(ACT + 0x160, flags);
        log.borrow_mut().clear();
        call(&mut ee, 0x17aeb8, &[0, ACT as u64, class as u64], &[]);
        let game = log.borrow().clone();
        let port = motion_request_gate(flags, class);
        assert_eq!(game.len(), port as usize, "case {n}: flags {flags:#x} class {class}");
        if let Some(&(_, c, _)) = game.first() {
            assert_eq!(c, class as u32, "case {n}");
        }
    }
}

#[test]
fn orig_17af10_matches_the_game() {
    let Some((mut ee, elf)) = machine("oracle test of 0x17af10") else { return };
    let k = consts(&elf);
    let log: Log = Rc::default();
    let part = Rc::new(RefCell::new(0u16));
    for a in [0x16baa0u32, 0x1b1c90, 0x17e2d8] {
        let l = log.clone();
        ee.hooks.insert(a, move |ee: &mut Ee| l.borrow_mut().push((a, ee.gpr[5] as u32, ee.fpr[12].to_bits())));
    }
    let p = part.clone();
    ee.hooks.insert(0x1699a8, move |ee: &mut Ee| ee.gpr[2] = *p.borrow() as u128);
    let mut r = Rng::new(0x17af10);
    for n in 0..CASES {
        let walk = 1.0 + rf(&mut r, 3.0).abs();
        let run = walk + 1.0 + rf(&mut r, 8.0).abs();
        let split = (walk + run) * k.class_split;
        let speed = match r.below(8) {
            0 => 0.0,
            1 => -rf(&mut r, 3.0).abs(),
            2 => walk,
            3 => run,
            4 => split,
            _ => rf(&mut r, 16.0),
        };
        let a = GroundMotionIn {
            speed,
            walk,
            run,
            class_flags: if r.below(3) == 0 { r.word() as u32 & !0x80 } else { r.word() as u32 | 0x80 },
            motion_id: [0, 1, 2, 3, -1, 151, r.word() as i32][r.below(7) as usize],
            flags2: [0, 1, r.word() as u32][r.below(3) as usize],
            flags: [0, 4, 0x10, 0x14, 8, r.word() as u32][r.below(6) as usize],
            part_num: [0xe, 0, 1, r.word() as i16][r.below(4) as usize],
        };
        ee.wf32(ACT + 0x1c, a.speed);
        ee.wf32(ACT + 0x110, a.walk);
        ee.wf32(ACT + 0x114, a.run);
        ee.w32(ACT + 0x588, a.class_flags);
        ee.w32(ACT + 0x154, a.motion_id as u32);
        ee.w32(ACT + 0x164, a.flags2);
        ee.w32(ACT + 0x160, a.flags);
        *part.borrow_mut() = a.part_num as u16;
        log.borrow_mut().clear();
        call(&mut ee, 0x17af10, &[0, ACT as u64], &[]);
        let game = log.borrow().clone();
        let port = ground_motion(&a, &k);
        let mut want: Vec<(u32, u32, u32)> = Vec::new();
        if port.foot.stop_sound {
            want.push((0x16baa0, 8, 0));
        }
        if let Some(d) = port.foot.dust {
            want.extend(d.map(|kind| (0x1b1c90u32, kind as u32, 0)));
        }
        if let Some(c) = port.request {
            want.push((0x17e2d8, c as u32, k.request_blend.to_bits()));
        }
        // The effect calls take the actor and the kind (second argument register); the dust and sound hooks see no float argument.
        let game: Vec<(u32, u32, u32)> = game.into_iter().map(|(f, a1, f12)| if f == 0x17e2d8 { (f, a1, f12) } else { (f, a1, 0) }).collect();
        assert_eq!(game, want, "case {n}: {a:?} -> {port:?}");
    }
}

#[test]
fn orig_17b040_matches_the_game() {
    let Some((mut ee, elf)) = machine("oracle test of 0x17b040") else { return };
    let k = consts(&elf);
    let log: Log = Rc::default();
    let l = log.clone();
    ee.hooks.insert(0x1832b8, move |ee: &mut Ee| {
        l.borrow_mut().push((ee.gpr[4] as u32, ee.fpr[12].to_bits(), ee.fpr[13].to_bits()));
        ee.gpr[2] = ee.gpr[4];
    });
    let mut r = Rng::new(0x17b040);
    for n in 0..CASES {
        let comp = |r: &mut Rng| if r.below(3) == 0 { 0.0 } else { rf(r, 2.0) };
        let mut dir = [comp(&mut r), comp(&mut r), comp(&mut r), match r.below(5) { 0 => 0.0, 1 => -rf(&mut r, 20.0).abs(), _ => rp(&mut r, 20.0) }];
        // A zero direction with a speed has no unit vector (the stick never gives one): give it a direction.
        if dir[3] > 0.0 && dir[0] == 0.0 && dir[1] == 0.0 && dir[2] == 0.0 {
            dir[2] = 1.0;
        }
        let a = GroundStepIn {
            dir,
            flags: [0, 0x40, 0x41, r.word() as u32][r.below(4) as usize] & if r.below(3) == 0 { !0 } else { !0x40 },
            flags2: [0, 1, r.word() as u32][r.below(3) as usize],
            f0: rf(&mut r, 50.0),
            row_28: rf(&mut r, 50.0),
            row_2c: rf(&mut r, 50.0),
            w810: [0, 1, r.word() as u32][r.below(3) as usize],
        };
        let step = [1.0, 2.0, rp(&mut r, 3.0)][r.below(3) as usize];
        put4(&mut ee, ACT + 0x10, a.dir);
        put4(&mut ee, ACT + 0x20, [7.0; 4]);
        ee.w32(ACT + 0x160, a.flags);
        ee.w32(ACT + 0x164, a.flags2);
        ee.wf32(ACT + 0xf0, a.f0);
        ee.w32(ACT + 0x12c, ROW);
        ee.wf32(ROW + 0x28, a.row_28);
        ee.wf32(ROW + 0x2c, a.row_2c);
        ee.w32(ACT + 0x810, a.w810);
        ee.wf32(FRAME_STEP, step);
        put4(&mut ee, OUT, [9.0; 4]);
        log.borrow_mut().clear();
        let ret = call(&mut ee, 0x17b040, &[OUT as u64, 0, ACT as u64], &[]);
        assert_eq!(ret as u32, OUT, "case {n}: the result address");
        let game = log.borrow().clone();
        match ground_step(&a, &k, step) {
            GroundStep::Jump(x, y) => {
                assert_eq!(game, vec![(OUT, x.to_bits(), y.to_bits())], "case {n}: {a:?}");
            }
            GroundStep::Walk { out, velocity } => {
                assert!(game.is_empty(), "case {n}: the original called the jump step for {a:?}");
                assert_v4(n, &format!("the displacement of {a:?} step {step}"), get4(&mut ee, OUT), out);
                assert_v4(n, &format!("the velocity of {a:?}"), get4(&mut ee, ACT + 0x20), velocity);
            }
        }
    }
}

#[test]
fn orig_17ade0_matches_the_game() {
    let Some((mut ee, _)) = machine("oracle test of 0x17ade0") else { return };
    let case = Rc::new(RefCell::new((0f32, 0u32)));
    let c = case.clone();
    ee.hooks.insert(0x16bd98, move |ee: &mut Ee| {
        let (h, id) = *c.borrow();
        ee.w32(ee.gpr[5] as u32, id);
        ee.fpr[0] = h;
    });
    let mut r = Rng::new(0x17ade0);
    for n in 0..CASES {
        let dir = [rf(&mut r, 3.0), rf(&mut r, 3.0), rf(&mut r, 3.0), rf(&mut r, 3.0)];
        let (height, id) = (rf(&mut r, 400.0), [3u32, 0xc9][r.below(2) as usize]);
        let (set, flags) = (r.below(2) == 0, [0u32, 0x10000, r.word() as u32][r.below(3) as usize]);
        *case.borrow_mut() = (height, id);
        put4(&mut ee, ACT + 0x10, dir);
        ee.w32(ACT + 0x108, flags);
        put4(&mut ee, OUT, [9.0; 4]);
        let ret = call(&mut ee, 0x17ade0, &[OUT as u64, ACT as u64, LOCAL as u64, set as u64], &[]);
        assert_eq!(ret as u32, OUT, "case {n}");
        let (v, f) = launch_vector(dir, height, set, flags);
        assert_v4(n, &format!("the launch vector of {dir:?} height {height}"), get4(&mut ee, OUT), v);
        assert_eq!(ee.r32(ACT + 0x108), f, "case {n}: flags {flags:#x} set {set}");
    }
}

#[test]
fn orig_17ae50_matches_the_game() {
    let Some((mut ee, _)) = machine("oracle test of 0x17ae50") else { return };
    let case = Rc::new(RefCell::new((0f32, 0u32)));
    let c = case.clone();
    ee.hooks.insert(0x16bd98, move |ee: &mut Ee| {
        let (h, id) = *c.borrow();
        ee.w32(ee.gpr[5] as u32, id);
        ee.fpr[0] = h;
    });
    // (function, actor argument, vector or request words): the launch sees the vector, the request the id and both floats.
    let seen: Rc<RefCell<Vec<(u32, u32, [u32; 4])>>> = Rc::default();
    let s = seen.clone();
    ee.hooks.insert(0x17b278, move |ee: &mut Ee| {
        let at = ee.gpr[5] as u32;
        let w = [0, 1, 2, 3].map(|i| ee.r32(at + 4 * i));
        s.borrow_mut().push((0x17b278, ee.gpr[4] as u32, w));
    });
    let s = seen.clone();
    ee.hooks.insert(0x17e220, move |ee: &mut Ee| {
        s.borrow_mut().push((0x17e220, ee.gpr[4] as u32, [ee.gpr[5] as u32, ee.fpr[12].to_bits(), ee.fpr[13].to_bits(), 0]));
    });
    let mut r = Rng::new(0x17ae50);
    for n in 0..CASES {
        let dir = [rf(&mut r, 3.0), rf(&mut r, 3.0), rf(&mut r, 3.0), rf(&mut r, 3.0)];
        let (height, id) = (rf(&mut r, 400.0), [3u32, 0xc9][r.below(2) as usize]);
        let (set, flags) = (r.below(2) == 0, [0u32, 0x10000, r.word() as u32][r.below(3) as usize]);
        *case.borrow_mut() = (height, id);
        put4(&mut ee, ACT + 0x10, dir);
        ee.w32(ACT + 0x108, flags);
        ee.w32(ACT + 0x100, 77);
        ee.w32(ACT + 0x104, 77);
        seen.borrow_mut().clear();
        call(&mut ee, 0x17ae50, &[0, ACT as u64, set as u64], &[]);
        let j = jump_start(dir, height, id as i32, set, flags);
        let want = vec![(0x17b278, ACT, j.vector.map(f32::to_bits)), (0x17e220, ACT + 0x140, [j.motion_id as u32, 0f32.to_bits(), 0f32.to_bits(), 0])];
        assert_eq!(*seen.borrow(), want, "case {n}: {dir:?} height {height} id {id}");
        assert_eq!((ee.r32(ACT + 0x100), ee.r32(ACT + 0x104), ee.r32(ACT + 0x108)), (j.w100, j.w104, j.flags108), "case {n}");
    }
}

#[test]
fn orig_1796a8_matches_the_game() {
    let Some((mut ee, _)) = machine("oracle test of 0x1796a8") else { return };
    let seen: Rc<RefCell<Vec<(u32, u32, [u32; 4])>>> = Rc::default();
    let s = seen.clone();
    ee.hooks.insert(0x17b278, move |ee: &mut Ee| {
        let at = ee.gpr[5] as u32;
        let w = [0, 1, 2, 3].map(|i| ee.r32(at + 4 * i));
        s.borrow_mut().push((0x17b278, ee.gpr[4] as u32, w));
    });
    let s = seen.clone();
    ee.hooks.insert(0x16b698, move |ee: &mut Ee| s.borrow_mut().push((0x16b698, ee.gpr[4] as u32, [ee.gpr[5] as u32, 0, 0, 0])));
    let mut r = Rng::new(0x1796a8);
    for n in 0..CASES {
        let v = [rf(&mut r, 9.0), rf(&mut r, 9.0), rf(&mut r, 9.0), rf(&mut r, 9.0)];
        put4(&mut ee, VEC, v);
        seen.borrow_mut().clear();
        call(&mut ee, 0x1796a8, &[0, ACT as u64, VEC as u64], &[]);
        let e = enter_with_vector(v);
        let MoverLaunch::Vector(lv) = e.launch else { panic!() };
        let want = vec![(0x17b278, ACT, lv.map(f32::to_bits)), (0x16b698, ACT, [e.action, 0, 0, 0])];
        assert_eq!(*seen.borrow(), want, "case {n}: {v:?}");
    }
}

#[test]
fn orig_1796e8_matches_the_game() {
    let Some((mut ee, _)) = machine("oracle test of 0x1796e8") else { return };
    let seen: Rc<RefCell<Vec<(u32, u32, [u32; 4])>>> = Rc::default();
    let s = seen.clone();
    ee.hooks.insert(0x17b1e0, move |ee: &mut Ee| s.borrow_mut().push((0x17b1e0, ee.gpr[4] as u32, [ee.fpr[12].to_bits(), 0, 0, 0])));
    let s = seen.clone();
    ee.hooks.insert(0x16b698, move |ee: &mut Ee| s.borrow_mut().push((0x16b698, ee.gpr[4] as u32, [ee.gpr[5] as u32, 0, 0, 0])));
    let mut r = Rng::new(0x1796e8);
    for n in 0..CASES {
        let h = rf(&mut r, 300.0);
        seen.borrow_mut().clear();
        call(&mut ee, 0x1796e8, &[0, ACT as u64], &[h]);
        let e = enter_with_height(h);
        let MoverLaunch::Height(eh) = e.launch else { panic!() };
        let want = vec![(0x17b1e0, ACT, [eh.to_bits(), 0, 0, 0]), (0x16b698, ACT, [e.action, 0, 0, 0])];
        assert_eq!(*seen.borrow(), want, "case {n}: height {h}");
    }
}

#[test]
fn orig_179718_matches_the_game() {
    let Some((mut ee, _)) = machine("oracle test of 0x179718") else { return };
    let seen: Rc<RefCell<Vec<u32>>> = Rc::default();
    let s = seen.clone();
    ee.hooks.insert(0x30fe80, move |ee: &mut Ee| s.borrow_mut().push(ee.gpr[4] as u32));
    for pass in 0..2u32 {
        // The flag byte at 0x1c89380 starts 0; the four words at 0x1c89390 hold junk the first call must clear.
        ee.write(0x1c89380, &[0]);
        for i in 0..4 {
            ee.w32(0x1c89390 + 4 * i, 0xdead_beef);
        }
        let mut ready = false;
        for n in 0..3 {
            put4(&mut ee, OUT, [9.0; 4]);
            seen.borrow_mut().clear();
            let ret = call(&mut ee, 0x179718, &[OUT as u64], &[]);
            let (vec, registered) = shared_zero_vector(&mut ready);
            assert_eq!(ret as u32, OUT, "pass {pass} call {n}");
            assert_eq!(get4(&mut ee, OUT).map(f32::to_bits), vec.map(f32::to_bits), "pass {pass} call {n}");
            assert_eq!(seen.borrow().clone(), registered.into_iter().collect::<Vec<_>>(), "pass {pass} call {n}: the exit handler");
            assert_eq!(ee.r8(0x1c89380) != 0, ready, "pass {pass} call {n}: the flag");
        }
    }
}

#[test]
fn orig_179868_matches_the_game() {
    let Some((mut ee, elf)) = machine("oracle test of 0x179868") else { return };
    let table = MoverTable::from_exe(&elf).expect("the pointer table");
    let known: Vec<u32> = mover_descriptors().iter().map(|d| d.addr).collect();
    for id in 0..MOVER_COUNT {
        let game = call(&mut ee, 0x179868, &[id as u64], &[]) as u32;
        assert_eq!(Some(game), table.descriptor(id), "id {id}");
        assert!(known.contains(&game), "id {id}: pointer {game:#x} is not a descriptor of the table");
    }
    assert_eq!(table.descriptor(MOVER_COUNT), None);
}

#[test]
fn orig_179880_matches_the_game() {
    let Some((mut ee, elf)) = machine("oracle test of 0x179880") else { return };
    let table = MoverTable::from_exe(&elf).expect("the pointer table");
    // The static initialiser, construct call (arguments 1 and 0xffff); it runs the base constructors, nothing is stubbed.
    call(&mut ee, 0x179880, &[1, 0xffff], &[]);
    for d in mover_descriptors() {
        let game = (ee.r32(d.addr), ee.r32(d.addr + 4), ee.r32(d.addr + 8));
        assert_eq!(game, (d.vtable, d.id, d.flags), "record at {:#x}", d.addr);
    }
    // Every pointer of the table lands on one of the records.
    for id in 0..MOVER_COUNT {
        let p = table.descriptor(id).unwrap();
        assert!(mover_descriptors().iter().any(|d| d.addr == p), "table entry {id}");
    }
    // The records do not overlap and are in address order.
    for w in mover_descriptors().windows(2) {
        assert!(w[0].addr + 0x10 <= w[1].addr, "{:#x} {:#x}", w[0].addr, w[1].addr);
    }
}

/// Not an oracle test: the numbers the ground mover gives a run, with the frame step of the field (2.0) and Roxas's run speed 8 of the
/// rig's state .06: 16 centimetres per game frame, the class 2 (run).
#[test]
fn a_full_stick_run_moves_sixteen_centimetres_per_frame() {
    let k = GroundConsts { request_blend: 0.0, class_split: 0.5, out_w: 1.0 };
    let a = GroundStepIn { dir: [0.0, 0.0, -1.0, 8.0], ..Default::default() };
    let GroundStep::Walk { out, velocity } = ground_step(&a, &k, 2.0) else { panic!() };
    assert_eq!((out, velocity[2]), ([0.0, 0.0, -16.0, 1.0], -128.0), "actor +0x20 holds the displacement times the speed");
    let m = ground_motion(&GroundMotionIn { speed: 8.0, walk: 4.0, run: 8.0, flags: 4, ..Default::default() }, &k);
    assert_eq!((m.class, m.request), (2, Some(2)));
    let m = ground_motion(&GroundMotionIn { speed: 4.0, walk: 4.0, run: 8.0, flags: 0x10, ..Default::default() }, &k);
    assert_eq!((m.class, m.request), (1, Some(1)));
    let m = ground_motion(&GroundMotionIn { speed: 0.0, walk: 4.0, run: 8.0, flags: 0x10, ..Default::default() }, &k);
    assert_eq!((m.class, m.request), (0, None), "idle is requested only through bit 4");
}

#[test]
fn the_ground_constants_are_the_ports() {
    let Some((_, elf)) = machine("constants of the ground mover") else { return };
    assert_eq!(consts(&elf), GroundConsts::USA);
}
