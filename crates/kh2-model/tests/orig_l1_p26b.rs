//! Oracle test of the projection builder 0x135028 (L1-P26b). kh2-ee runs the original from the user's executable on generated
//! lights and boxes; the port (`kh2_model::projection::project`) runs on copies of the same inputs, and both output matrices,
//! the static block and the first-use registration call are compared bit for bit (no tolerance: the executable's float
//! operations are the interpreter's, and the port uses the same single precision rules).
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::{ARENA, Rng, STACK};
use kh2_model::light::Mat4;
use kh2_model::projection::{IDENTITY, STATICS, STATICS_SIZE, project};

const LIGHT: u32 = ARENA;
const CORNERS: u32 = ARENA + 0x1000;
const OUT_A: u32 = ARENA + 0x2000;
const OUT_B: u32 = ARENA + 0x3000;

fn f32_in(r: &mut Rng, scale: f32) -> f32 {
    ((r.word() >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0) * scale
}

fn identity() -> Mat4 {
    std::array::from_fn(|i| std::array::from_fn(|j| if i == j { 1.0 } else { 0.0 }))
}

fn mat_at(e: &mut Ee, a: u32) -> Mat4 {
    std::array::from_fn(|i| std::array::from_fn(|j| e.f32_at(a + 16 * i as u32 + 4 * j as u32)))
}

fn bits(m: &Mat4) -> Vec<u32> {
    m.iter().flatten().map(|x| x.to_bits()).collect()
}

/// A random box, sometimes degenerate (a light inside it, flat boxes, corners behind the light), seen from a random light.
fn make_case(r: &mut Rng, n: u32) -> ([f32; 4], [[f32; 4]; 8]) {
    let scale = [1.0f32, 10.0, 300.0, 5000.0][r.below(4) as usize];
    let lo: [f32; 3] = std::array::from_fn(|_| f32_in(r, scale));
    let ext: [f32; 3] = std::array::from_fn(|_| (f32_in(r, scale).abs() + 0.01) * if n % 7 == 0 { 0.0 } else { 1.0 });
    let corners: [[f32; 4]; 8] = std::array::from_fn(|i| {
        let mut c = [0f32; 4];
        for k in 0..3 {
            c[k] = if i >> k & 1 == 1 { lo[k] + ext[k] } else { lo[k] };
        }
        c[3] = 1.0;
        c
    });
    let mut light = [f32_in(r, scale * 2.0), f32_in(r, scale * 2.0), f32_in(r, scale * 2.0), 1.0];
    match n % 11 {
        // A light on an axis through the box's centre, a light at the centre, and a light with a round number for the
        // dot products to tie.
        1 => {
            let c: [f32; 3] = std::array::from_fn(|k| lo[k] + ext[k] / 2.0);
            light = [c[0], c[1] + scale, c[2], 1.0];
        }
        2 => {
            let c: [f32; 3] = std::array::from_fn(|k| lo[k] + ext[k] / 2.0);
            light = [c[0], c[1], c[2], 1.0];
        }
        3 => light = [0.0, -1.0e7, 0.0, 1.0],
        _ => {}
    }
    (light, corners)
}

#[test]
fn orig_135028_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("orig_135028") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let regs: Rc<RefCell<Vec<u32>>> = Rc::default();
    let l = regs.clone();
    ee.hooks.insert(0x30fe80, move |e: &mut Ee| l.borrow_mut().push(e.gpr[reg::A0] as u32));
    let mut r = Rng::new(0x135028);
    let (mut first_use, mut reused, mut kinds) = (0u32, 0u32, [0u32; 5]);
    for n in 0..3000u32 {
        let (light, corners) = make_case(&mut r, n);
        // The identity matrix the game stores at 0x354a60: the identity, except in some cases.
        let mut ident = identity();
        if n % 5 == 4 {
            ident = std::array::from_fn(|_| std::array::from_fn(|_| f32_in(&mut r, 3.0)));
        }
        // The static block: guard clear (first use) or set with a random or the genuine axis table.
        let mut statics = [0u8; STATICS_SIZE];
        for b in statics.iter_mut() {
            *b = r.word() as u8;
        }
        match n % 3 {
            0 => statics[0] = 0,
            1 => {
                statics[0] = 1;
                let axes = [[1.0f32, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
                for (v, a) in axes.iter().enumerate() {
                    for (k, x) in a.iter().enumerate() {
                        statics[0x10 + 16 * v + 4 * k..0x10 + 16 * v + 4 * k + 4].copy_from_slice(&x.to_le_bytes());
                    }
                }
            }
            _ => {
                statics[0] = 1;
                for v in 0..12 {
                    let x = f32_in(&mut r, 2.0);
                    statics[0x10 + 4 * v..0x10 + 4 * v + 4].copy_from_slice(&x.to_le_bytes());
                }
            }
        }
        let kind = [0i64, 1, 2, 3, -1][(r.below(5)) as usize];
        kinds[(kind + 1) as usize] += 1;
        // Game side.
        ee.write(STATICS, &statics);
        for (i, row) in ident.iter().enumerate() {
            for (j, x) in row.iter().enumerate() {
                ee.wf32(IDENTITY + 16 * i as u32 + 4 * j as u32, *x);
            }
        }
        for (j, x) in light.iter().enumerate() {
            ee.wf32(LIGHT + 4 * j as u32, *x);
        }
        for (i, c) in corners.iter().enumerate() {
            for (j, x) in c.iter().enumerate() {
                ee.wf32(CORNERS + 16 * i as u32 + 4 * j as u32, *x);
            }
        }
        regs.borrow_mut().clear();
        ee.call(0x135028, &[LIGHT as u64, CORNERS as u64, OUT_A as u64, OUT_B as u64, kind as u64], &[], STACK).expect("original");
        let got = (mat_at(&mut ee, OUT_A), mat_at(&mut ee, OUT_B));
        let got_statics: Vec<u8> = (0..STATICS_SIZE as u32).map(|i| ee.r8(STATICS + i)).collect();
        let calls = regs.borrow().clone();
        // Port side.
        let mut ps = statics;
        let ((a, b), built) = project(&mut ps, &ident, &light, &corners, kind);
        assert_eq!(bits(&got.0), bits(&a), "case {n}: first matrix (kind {kind}, light {light:?})");
        assert_eq!(bits(&got.1), bits(&b), "case {n}: second matrix (kind {kind}, light {light:?})");
        assert_eq!(got_statics, ps.to_vec(), "case {n}: static block");
        assert_eq!(calls, if built.registered { vec![0x135980] } else { vec![] }, "case {n}: registration call");
        if built.registered {
            first_use += 1;
        } else {
            reused += 1;
        }
    }
    assert!(first_use > 500 && reused > 500, "both paths reached: {first_use} first uses, {reused} reuses");
    assert!(kinds.iter().all(|&k| k > 100), "every size class reached: {kinds:?}");
}
