//! Oracle tests for the frame step and timer unit at 0x14c6b0..0x14cdd0: kh2-ee runs the original on a random state of the block at
//! `0x356f18` (`kh2_game_core::frame::Vsync`), the port runs on a copy, and the whole block (float words within the EE's rounding),
//! the return value and the out-pointers are compared after every case.
//!
//! The harness is written for this unit instead of `factory gen` because the generator randomises globals as raw bytes: the game
//! speed would take float bit patterns the EE treats differently (no NaN or infinity), and a raw limit word makes the wait loops run
//! for billions of rounds. Here the limit is drawn from -5..64 (and the edges), the speed from the factory's float generator, the
//! counter small or raw. Rules as the factory's: the original runs from the user's executable, the cases come from a seeded generator
//! (a failing case number reproduces), and the first cases audit that the original writes nothing outside the block, the out-pointer
//! slot and the stack.
//!
//! Stand-in (stub): the vblank wait `FUN_0014d1a8` (a semaphore wait) is a hook that adds 1 to the counter at `0x356f38`, one vblank
//! per wait; the port's wait closure does the same.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_game_core::frame::{BASE, SIZE, Vsync, to_hms};

const WAIT: u32 = 0x14d1a8;
const OUT: u32 = 0x0180_0000;
const CASES: u32 = 4000;
/// What a port closure returns for a function without a return value (v0 is left over and not compared).
const VOID: u64 = u64::MAX;

fn machine(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.hooks.insert(WAIT, |e: &mut Ee| {
        let c = e.r32(BASE + 0x20);
        e.w32(BASE + 0x20, c.wrapping_add(1));
    });
    Some(ee)
}

/// `wide`: the limit also takes negative and edge values (the step 0x14cdd0 would wait for billions of vblanks on those; the game
/// only sets 0 and 1).
fn draw(r: &mut Rng, wide: bool) -> Vsync {
    let mut v = Vsync::default();
    for b in v.0.iter_mut() {
        *b = r.word() as u8;
    }
    let limit = match r.below(8) {
        0 if wide => [i32::MAX, i32::MIN, -1][r.below(3) as usize],
        1..=5 => r.below(3) as i32,
        _ if wide => r.below(70) as i32 - 5,
        _ => r.below(65) as i32,
    };
    v.set_limit(limit as u32);
    v.set_since_step(if r.below(4) == 0 {
        r.word() as u32
    } else {
        r.below(9) as u32
    });
    v.set_frame_type(match r.below(3) {
        0 => 0,
        1 => 1,
        _ => r.word() as u8,
    });
    v.set_speed(if r.below(3) == 0 {
        [1.0, 0.5, 2.0, 0.0][r.below(4) as usize]
    } else {
        r.float()
    });
    for o in [0x28, 0x2c, 0x30] {
        v.0[o..o + 4].copy_from_slice(&r.float().to_le_bytes());
    }
    v
}

fn feq(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a - b).abs() <= 1e-5 * (1.0 + a.abs().max(b.abs()))
}

fn same(n: u32, what: &str, game: &[u8], port: &Vsync) {
    let mut o = 0;
    while o < SIZE {
        if (0x24..0x34).contains(&o) {
            let g = f32::from_le_bytes(game[o..o + 4].try_into().unwrap());
            let p = f32::from_le_bytes(port.0[o..o + 4].try_into().unwrap());
            assert!(
                feq(g, p),
                "{what} case {n}: float +{o:#x}: game {g} port {p}"
            );
            o += 4;
        } else {
            assert_eq!(
                game[o], port.0[o],
                "{what} case {n}: byte +{o:#x}: game {:02x} port {:02x}",
                game[o], port.0[o]
            );
            o += 1;
        }
    }
}

/// Runs `addr` with args from `args` on random blocks; `port` gets the block and the args and returns v0.
fn check(
    addr: u32,
    cases: u32,
    args: impl Fn(&mut Rng) -> Vec<u64>,
    port: impl FnMut(&mut Vsync, &[u64]) -> u64,
) {
    check_with(addr, cases, false, args, port)
}

/// [`check`] with `fret`: the original returns in f0 (the port returns the float's bits) and the args are float arguments (f12..).
fn check_with(
    addr: u32,
    cases: u32,
    fret: bool,
    args: impl Fn(&mut Rng) -> Vec<u64>,
    mut port: impl FnMut(&mut Vsync, &[u64]) -> u64,
) {
    let Some(mut ee) = machine(&format!("oracle test of {addr:#x}")) else {
        return;
    };
    let mut r = Rng::new(0x6b68_3272 ^ addr as u64);
    let mut varied = false;
    let mut first: Option<Vec<u8>> = None;
    for n in 0..cases {
        let mut v = draw(&mut r, addr != 0x14cdd0);
        let a = args(&mut r);
        ee.write(BASE, &v.0);
        let before = (n < 16).then(|| ee.ram.clone());
        let fa: Vec<f32> = if fret {
            a.iter().map(|&x| f32::from_bits(x as u32)).collect()
        } else {
            vec![]
        };
        let ia: &[u64] = if fret { &[] } else { &a };
        ee.fpr[0] = 0.0;
        let v0 = ee
            .call(addr, ia, &fa, STACK)
            .unwrap_or_else(|f| panic!("{addr:#x} case {n}: the original faulted: {f:?}"));
        let game: Vec<u8> = (0..SIZE as u32).map(|i| ee.r8(BASE + i)).collect();
        if let Some(before) = before {
            let inside = |x: u32| {
                (BASE..BASE + SIZE as u32).contains(&x)
                    || (OUT..OUT + 0x20).contains(&x)
                    || (STACK - 0x10000..STACK + 0x100).contains(&x)
            };
            if let Some(x) =
                (0..ee.ram.len()).find(|&i| before[i] != ee.ram[i] && !inside(i as u32))
            {
                panic!("{addr:#x} case {n}: the original wrote {x:#x} outside the block");
            }
        }
        match &first {
            None => first = Some(game.clone()),
            Some(f) => varied |= *f != game,
        }
        let mine = port(&mut v, &a);
        if fret && mine != VOID {
            let g = ee.fpr[0];
            let p = f32::from_bits(mine as u32);
            assert!(feq(g, p), "{addr:#x} case {n}: f0 game {g} port {p}");
        } else if mine != VOID {
            assert_eq!(v0 as u32, mine as u32, "{addr:#x} case {n}: v0");
        }
        same(n, &format!("{addr:#x}"), &game, &v);
    }
    assert!(varied, "{addr:#x}: every case gave the same block");
}

fn wait(v: &mut Vsync) {
    v.set_since_step(v.since_step().wrapping_add(1));
}

fn none(_: &mut Rng) -> Vec<u64> {
    vec![]
}

fn int(r: &mut Rng) -> Vec<u64> {
    vec![r.int(&[0, 1, 2, 60]) as i64 as u64]
}

#[test]
fn orig_14cdd0_matches_the_game() {
    check(0x14cdd0, CASES, none, |v, _| {
        v.step_frame(wait);
        VOID
    });
}

#[test]
fn orig_14cbf8_matches_the_game() {
    check(0x14cbf8, CASES, none, |v, _| {
        v.fixed_step();
        VOID
    });
}

#[test]
fn orig_14cba0_matches_the_game() {
    check(
        0x14cba0,
        CASES,
        |r| vec![(r.below(70) as i64 - 5) as u64],
        |v, a| {
            v.set_frame_limit(a[0] as u32);
            VOID
        },
    );
}

#[test]
fn orig_14cd38_matches_the_game() {
    check(0x14cd38, 1000, none, |v, _| {
        v.init();
        VOID
    });
}

#[test]
fn orig_14cbe8_matches_the_game() {
    check_with(0x14cbe8, 1000, true, none, |v, _| {
        v.frame_step().to_bits() as u64
    });
}

#[test]
fn orig_14cb90_matches_the_game() {
    check(0x14cb90, 1000, int, |v, a| {
        v.set_frame_type(a[0] as u8);
        VOID
    });
}

#[test]
fn orig_14cbc0_matches_the_game() {
    check_with(
        0x14cbc0,
        1000,
        true,
        |r| vec![r.float().to_bits() as u64],
        |v, a| {
            v.set_game_speed(f32::from_bits(a[0] as u32));
            VOID
        },
    );
}

#[test]
fn orig_14cbd0_matches_the_game() {
    check(0x14cbd0, 1000, none, |v, _| {
        v.reset_speed();
        VOID
    });
}

#[test]
fn orig_14c6b0_matches_the_game() {
    check(0x14c6b0, 1000, int, |v, a| {
        v.set_count_down(a[0] as i32);
        VOID
    });
}

#[test]
fn orig_14c6e0_matches_the_game() {
    check(0x14c6e0, 1000, int, |v, a| {
        v.set_count_up(a[0] as i32);
        VOID
    });
}

#[test]
fn orig_14c710_matches_the_game() {
    check(0x14c710, 1000, none, |v, _| v.is_count_down() as u64);
}

#[test]
fn orig_14c728_matches_the_game() {
    check(0x14c728, 1000, none, |v, _| {
        v.start_timer();
        VOID
    });
}

#[test]
fn orig_14c748_matches_the_game() {
    check(0x14c748, 1000, int, |v, a| {
        v.pause_on(a[0] as u32);
        VOID
    });
}

#[test]
fn orig_14c760_matches_the_game() {
    check(0x14c760, 1000, int, |v, a| {
        v.pause_off(a[0] as u32);
        VOID
    });
}

#[test]
fn orig_14c780_matches_the_game() {
    check(0x14c780, 1000, none, |v, _| v.paused() as u64);
}

#[test]
fn orig_14c790_matches_the_game() {
    check(0x14c790, 1000, none, |v, _| v.stop_timer() as u64);
}

#[test]
fn orig_14c7a8_matches_the_game() {
    check(0x14c7a8, 1000, none, |v, _| v.timer() as u64);
}

#[test]
fn orig_14c7b8_matches_the_game() {
    check(
        0x14c7b8,
        1000,
        |r| {
            let _ = r.word();
            vec![]
        },
        |v, _| {
            v.tick_timer();
            VOID
        },
    );
}

/// `FUN_0014c7f0(frames, &h, &m, &s, &hundredths)`: the four words at `OUT` are compared.
#[test]
fn orig_14c7f0_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14c7f0") else {
        return;
    };
    let mut r = Rng::new(0x14c7f0);
    for n in 0..CASES {
        let frames = if r.below(2) == 0 {
            r.below(0xcdf_e5c4) as i32
        } else {
            r.int(&[216_000, 3600, 60])
        };
        let args = [
            frames as i64 as u64,
            OUT as u64,
            OUT as u64 + 4,
            OUT as u64 + 8,
            OUT as u64 + 12,
        ];
        ee.write(OUT, &[0xa5; 16]);
        ee.call(0x14c7f0, &args, &[], STACK)
            .unwrap_or_else(|f| panic!("case {n}: {f:?}"));
        let game = (
            ee.r32(OUT),
            ee.r32(OUT + 4),
            ee.r32(OUT + 8),
            ee.r32(OUT + 12),
        );
        assert_eq!(game, to_hms(frames as u32), "case {n}: frames {frames}");
    }
}
