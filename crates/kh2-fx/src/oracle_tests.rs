//! Runs the game's own handlers of the three ported programs (`pppKeShpTail3X` frame 0x1fcf70 and
//! draw 0x1fd388, `pppKeMvYpEff` 0x1f4628, `pppSnoMdlDttNorm` draw 0x20a7e8 and its model draw
//! 0x20ab40) in the `kh2-ee` interpreter on the user's executable and compares them with this
//! crate's ports on the same objects, parameters and matrices. The routines that would draw or
//! allocate packets (0x21a660 shape draws, the packet allocator 0x20c110 and the DMA kick 0x1e4a88)
//! are replaced by hooks that record their arguments. Skipped without a disc (KH2_ISO or
//! local/iso.txt).
//!
//! The Sno model draw's clip and back-face tests read the VU0 status and MAC flags (`cfc2`); the
//! interpreter sets them as the hardware does. The shape draw uses PEXTLB and PEXTLH, which the
//! interpreter executes itself.

use std::cell::RefCell;

use glam::{EulerRot, Mat4, Quat, Vec3, Vec4};
use kh2_ee::{Ee, reg};

use crate::pax::{Dpd, Program};
use crate::prog_more as m;
use crate::projection::Projection;
use crate::shape::Shape;
use crate::vm::{Ctx, Env, OBJ_FRAME, OBJ_LOCAL, OBJ_WORK, Obj};

const OBJ: u32 = 0x01f0_0000;
const PROG: u32 = 0x01f0_1000;
const USEVAL: u32 = 0x01f0_1100;
const ENTRY: u32 = 0x01f0_1200;
const ZERO: u32 = 0x01f0_1400;
const RES: u32 = 0x01f0_1500;
const SHAPETAB: u32 = 0x01f0_1600;
const SHAPE: u32 = 0x01f0_2000;
const SCRATCH: u32 = 0x01f0_3000;
const STACK: u32 = 0x01ff_0000;
const OBJ_SIZE: usize = 0x800;

#[derive(Clone, Debug)]
struct DrawCall {
    frame_ptr: u32,
    color: [u8; 4],
    matrix: [f32; 16],
}

thread_local! {
    static CALLS: RefCell<Vec<DrawCall>> = const { RefCell::new(Vec::new()) };
    /// Sno: (amplitude, taper flag byte as a word, alpha register argument, the projection's z
    /// translation at 0x1db1038 as the draw sees it) of each model draw.
    static SNO: RefCell<Vec<(f32, u32, u32, f32)>> = const { RefCell::new(Vec::new()) };
}

fn hook_shape_draw(ee: &mut Ee) {
    let a0 = ee.gpr[reg::A0] as u32;
    let a1 = ee.gpr[reg::A0 + 1] as u32;
    let color = [ee.r8(a1 + 4), ee.r8(a1 + 5), ee.r8(a1 + 6), ee.r8(a1 + 7)];
    let mp = ee.r32(a1 + 0x10);
    let mut matrix = [0f32; 16];
    for (i, x) in matrix.iter_mut().enumerate() {
        *x = ee.f32_at(mp + 4 * i as u32);
    }
    CALLS.with(|c| c.borrow_mut().push(DrawCall { frame_ptr: a0, color, matrix }));
    ee.gpr[reg::V0] = 0;
}

fn hook_model_draw(ee: &mut Ee) {
    let a0 = ee.gpr[reg::A0] as u32;
    let a1 = ee.gpr[reg::A0 + 1] as u32;
    let rec = (ee.f32_at(a0 + 0x20), ee.r32(a0 + 0x24), a1 as u32, ee.f32_at(0x1db1038));
    SNO.with(|c| c.borrow_mut().push(rec));
}

fn hook_alloc(ee: &mut Ee) {
    ee.gpr[reg::V0] = SCRATCH as u128;
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

fn write_mat(ee: &mut Ee, at: u32, m: &Mat4) {
    for (i, x) in m.to_cols_array().iter().enumerate() {
        ee.wf32(at + 4 * i as u32, *x);
    }
}

/// A raw DPD shape of three frames (waits 7, 5, 6) with one sprite each, in the layout the game
/// and `Shape::parse` read.
fn raw_shape() -> Vec<u8> {
    let mut b = vec![0u8; 0x200];
    let a = 0x10;
    b[a + 4..a + 6].copy_from_slice(&3i16.to_le_bytes());
    b[a + 6..a + 8].copy_from_slice(&3i16.to_le_bytes());
    for (k, wait) in [7i16, 5, 6].into_iter().enumerate() {
        let v = 0x60 + 0x40 * k;
        let e = a + 0x10 + 8 * k;
        b[e..e + 2].copy_from_slice(&((v - a) as i16).to_le_bytes());
        b[e + 2..e + 4].copy_from_slice(&wait.to_le_bytes());
        b[v + 2..v + 4].copy_from_slice(&1i16.to_le_bytes());
    }
    b
}

fn dpd_with(shape: &[u8]) -> Dpd {
    Dpd { particles: vec![], textures: vec![], shapes: vec![Shape::parse(shape)], models: vec![], vsfs: vec![] }
}

struct Scene {
    ee: Ee,
    env: Env,
}

/// The executable loaded, the engine's globals the handlers read set from `env`, the draw hooks in.
fn scene(elf: &[u8], bind: Mat4, view: Mat4) -> Scene {
    let mut ee = Ee::new();
    ee.load_elf(elf).expect("elf");
    let env = Env {
        bind,
        view,
        view_inv: view.inverse(),
        bind_scale: Vec3::new(bind.x_axis.truncate().length(), bind.y_axis.truncate().length(), bind.z_axis.truncate().length()),
        ..Default::default()
    };
    ee.w32(0x1db0e20, ZERO);
    ee.w32(0x1db0e24, RES);
    ee.w32(0x1db0e28, OBJ);
    write_mat(&mut ee, 0x1db0e30, &bind);
    write_mat(&mut ee, 0x1db0e70, &view);
    write_mat(&mut ee, 0x1db0f80, &(view * bind));
    write_mat(&mut ee, 0x1db1000, &Mat4::IDENTITY);
    ee.wf32(0x362b00, env.bind_scale.x);
    ee.wf32(0x362b04, env.bind_scale.y);
    ee.wf32(0x362b08, env.bind_scale.z);
    ee.hooks.insert(0x21a660, hook_shape_draw);
    ee.hooks.insert(0x20ab40, hook_model_draw);
    ee.hooks.insert(0x20c110, hook_alloc);
    ee.hooks.insert(0x20c1f0, hook_nothing);
    Scene { ee, env }
}

fn put_program(ee: &mut Ee, p: &Program) {
    ee.w32(PROG + 0xc, USEVAL);
    for (i, u) in p.use_val.iter().enumerate() {
        ee.w32(USEVAL + 4 * i as u32, *u as u32);
    }
    ee.write(ENTRY, &p.params);
}

fn sync_obj(ee: &mut Ee, obj: &Obj) {
    ee.write(OBJ, &obj.mem);
}

fn run(ee: &mut Ee, handler: u32) {
    ee.call(handler, &[OBJ as u64, ENTRY as u64, PROG as u64], &[], STACK).unwrap_or_else(|f| panic!("{handler:#x}: {f:?}"));
}

fn ee_bytes(ee: &Ee, at: u32, n: usize) -> Vec<u8> {
    ee.ram[at as usize..at as usize + n].to_vec()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 2e-3 * (1.0 + a.abs().max(b.abs()))
}

fn pseudo(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
    (*seed >> 8) as f32 / (1u32 << 24) as f32
}

/// One Tail3X parameter entry (0x58 bytes) and the list terminator.
fn tail3_entry(world: bool, count: u8, spacing: f32, rotate: u16) -> Vec<u8> {
    let mut e = vec![0u8; 0x5c];
    let w32 = |e: &mut Vec<u8>, o: usize, v: u32| e[o..o + 4].copy_from_slice(&v.to_le_bytes());
    let h = |e: &mut Vec<u8>, o: usize, v: i16| e[o..o + 2].copy_from_slice(&v.to_le_bytes());
    w32(&mut e, 0, 0); // frame
    w32(&mut e, 4, 0); // shape index
    w32(&mut e, 8, 0x200); // time accumulator step
    w32(&mut e, 0xc, 0.5f32.to_bits());
    w32(&mut e, 0x10, 2.0f32.to_bits());
    w32(&mut e, 0x14, 0.4f32.to_bits());
    w32(&mut e, 0x18, spacing.to_bits());
    e[0x1c] = count;
    e[0x1d] = 1;
    h(&mut e, 0x1e, rotate as i16);
    // start color, end color, rates, rates of rates (all deltas applied on frame 0)
    for (i, v) in [0x2e00i16, 0x1c80, 0x0300, 0x4000, 0x0880, 0x1100, 0x3180, 0x0800].into_iter().enumerate() {
        h(&mut e, 0x20 + 2 * i, v);
    }
    for (i, v) in [30i16, -20, 5, 40].into_iter().enumerate() {
        h(&mut e, 0x30 + 2 * i, v); // rates 0-3
        h(&mut e, 0x38 + 2 * i, 1 - i as i16); // rates of rates 0-3
    }
    for (i, v) in [-10i16, 15, -6, 25].into_iter().enumerate() {
        h(&mut e, 0x40 + 2 * i, v);
        h(&mut e, 0x48 + 2 * i, 2 * i as i16 - 3);
    }
    e[0x53] = world as u8;
    e[0x54] = 1;
    w32(&mut e, 0x58, 0xffff_f000);
    e
}

fn tail3_check(world: bool, count: u8, spacing: f32, rotate: u16) {
    let Some(elf) = exe() else { return };
    let bind = Mat4::from_scale_rotation_translation(Vec3::new(1.2, 0.8, 1.5), Quat::from_euler(EulerRot::ZYX, 0.3, -0.5, 0.7), Vec3::new(100.0, -20.0, 50.0));
    let view = Mat4::from_rotation_translation(Quat::from_euler(EulerRot::ZYX, -0.2, 0.4, 0.1), Vec3::new(10.0, -30.0, 400.0));
    let Scene { mut ee, env } = scene(&elf, bind, view);
    let raw = raw_shape();
    let dpd = dpd_with(&raw);
    ee.write(SHAPE, &raw);
    ee.w32(RES + 0x14, SHAPETAB);
    ee.w32(SHAPETAB, SHAPE + 0x10);
    ee.w32(SHAPETAB + 4, 0);
    let p = Program { kind: 157, param_size: 0x58, params: tail3_entry(world, count, spacing, rotate), use_val: vec![0x20, 0x300] };
    put_program(&mut ee, &p);

    let mut obj = Obj {
        group: 0,
        mem: vec![0; OBJ_SIZE],
        cursors: vec![0],
        fresh: false,
        restart: false,
        parent: None,
        ring: Vec::new(), trail_prims: Vec::new(),
        heap: Vec::new(),
        seed: 0,
    };
    let (w0, w1) = (OBJ_WORK + 0x20, OBJ_WORK + 0x300);
    obj.mem[w1 + 8..w1 + 12].copy_from_slice(&[80, 100, 120, 100]);
    obj.set_i16(w0 + 0x1f8, 0x1357);
    let mut rs = 7u32;
    let mut rng = 0u32;
    let mut compared = 0;
    for tick in 0..14 {
        obj.set_i32(OBJ_FRAME, tick * 0x1000);
        // The object's local matrix: a turning, scaling axes pair and a moving position.
        let a = tick as f32 * 0.21;
        let mut local = Mat4::from_cols(
            Vec4::new(a.cos() * 1.1, a.sin() * 1.1, 0.0, 0.0),
            Vec4::new(-a.sin() * 0.9, a.cos() * 0.9, 0.0, 0.0),
            Vec4::Z,
            Vec4::new(tick as f32 * 17.0 + pseudo(&mut rs) * 3.0, (tick as f32 * 0.7).sin() * 40.0, tick as f32 * 9.0, 1.0),
        );
        local.w_axis.w = 1.0;
        obj.set_mat(OBJ_LOCAL, local);
        sync_obj(&mut ee, &obj);

        // Frame handler.
        let mut draws = Vec::new();
        let mut spawns = Vec::new();
        {
            let mut ctx = Ctx {
                env: &env,
                dpd: &dpd,
                dpd_index: 0,
                draws: &mut draws,
                rng: &mut rng,
                running: true,
                fast_forward: false,
                obj_id: 0,
                prog_index: 0,
                parent_local: None,
                spawns: &mut spawns,
            };
            m::tail3_frame(&mut ctx, &mut obj, &p, 0);
        }
        run(&mut ee, 0x1fcf70);
        let o = w0 as u32;
        let game = ee_bytes(&ee, OBJ + o, 0x200);
        assert_eq!(&game[0x1fa..0x1fb], &obj.mem[w0 + 0x1fa..w0 + 0x1fb], "tick {tick}: ring head");
        assert_eq!(&game[..0x30], &obj.mem[w0..w0 + 0x30], "tick {tick}: channels, rates and rates of rates");
        for k in 0..0x1c * 4 {
            let at = 0x30 + 4 * k;
            let (g, r) = (f32::from_le_bytes(game[at..at + 4].try_into().unwrap()), obj.f32(w0 + at));
            assert!(near(g, r), "tick {tick}: ring float {k}: game {g} port {r}");
        }
        sync_obj(&mut ee, &obj);

        // Draw handler.
        let expect = m::tail3_instances(&env, &dpd, &obj, &p, 0);
        {
            let mut ctx = Ctx {
                env: &env,
                dpd: &dpd,
                dpd_index: 0,
                draws: &mut draws,
                rng: &mut rng,
                running: true,
                fast_forward: false,
                obj_id: 0,
                prog_index: 0,
                parent_local: None,
                spawns: &mut spawns,
            };
            m::tail3_draw(&mut ctx, &mut obj, &p, 0);
        }
        CALLS.with(|c| c.borrow_mut().clear());
        run(&mut ee, 0x1fd388);
        let calls = CALLS.with(|c| c.borrow().clone());
        assert_eq!(calls.len(), expect.len(), "tick {tick}: sprite count");
        compared += calls.len();
        for (i, (g, x)) in calls.iter().zip(&expect).enumerate() {
            let at = format!("tick {tick} sprite {i}");
            assert_eq!(g.frame_ptr, SHAPE + 0x10 + 0x50 + 0x40 * x.frame as u32, "{at}: shape frame");
            assert_eq!(g.color, x.color, "{at}: color bytes");
            let want = [x.right.extend(0.0), x.up.extend(0.0), Vec4::ZERO, x.centre];
            for (r, w) in want.iter().enumerate() {
                for c in 0..4 {
                    // The matrix goes through the (identity) projection: row r, component c.
                    let (gv, wv) = (g.matrix[4 * r + c], w[c]);
                    assert!(near(gv, wv), "{at}: matrix [{r}][{c}] game {gv} port {wv}");
                }
            }
        }
        assert_eq!(&ee_bytes(&ee, OBJ + o + 0x1f0, 4), &obj.mem[w0 + 0x1f0..w0 + 0x1f4], "tick {tick}: time accumulator");
        sync_obj(&mut ee, &obj);
    }
    // A run that compared nothing would prove nothing.
    assert!(compared >= 14, "only {compared} sprites compared");
    eprintln!("tail3 (world {world}, count {count}, spacing {spacing}): {compared} sprites match the game's draw calls");
}

#[test]
fn tail3_matches_the_game_local_space() {
    tail3_check(false, 30, 1.5, 0x1234);
}

#[test]
fn tail3_matches_the_game_world_space() {
    tail3_check(true, 12, 2.5, 0);
}

#[test]
fn tail3_matches_the_game_without_spacing() {
    tail3_check(false, 5, 0.0, 0x20);
}

#[test]
fn mv_yp_eff_matches_the_game() {
    let Some(elf) = exe() else { return };
    let Scene { mut ee, env } = scene(&elf, Mat4::IDENTITY, Mat4::IDENTITY);
    let p = Program { kind: 47, param_size: 4, params: vec![0, 0, 0, 0, 0, 0xf0, 0xff, 0xff], use_val: vec![0x40, 0x60] };
    put_program(&mut ee, &p);
    let dpd = dpd_with(&raw_shape());
    let mut rs = 99u32;
    let mut cases: Vec<Vec3> = (0..40).map(|_| Vec3::new(pseudo(&mut rs) * 20.0 - 10.0, pseudo(&mut rs) * 20.0 - 10.0, pseudo(&mut rs) * 20.0 - 10.0)).collect();
    // The axes and the zero cases.
    cases.extend([Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z, Vec3::ZERO, Vec3::new(0.0, 0.0, 3.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 5.0, 1.0)]);
    for v in cases {
        let mut obj = Obj { group: 0, mem: vec![0; OBJ_SIZE], cursors: vec![0], fresh: false, restart: false, parent: None, ring: Vec::new(), trail_prims: Vec::new(), heap: Vec::new(), seed: 0 };
        let (w0, w1) = (OBJ_WORK + 0x40, OBJ_WORK + 0x60);
        obj.set_vec3(w1, v);
        // Angles start at a marker so "not touched" shows.
        for k in 0..3 {
            obj.set_i32(w0 + 4 * k, 0x1111_0000 + k as i32);
        }
        sync_obj(&mut ee, &obj);
        run(&mut ee, 0x1f4628);
        let (mut draws, mut spawns, mut rng) = (Vec::new(), Vec::new(), 0u32);
        let mut ctx = Ctx {
            env: &env,
            dpd: &dpd,
            dpd_index: 0,
            draws: &mut draws,
            rng: &mut rng,
            running: true,
            fast_forward: false,
            obj_id: 0,
            prog_index: 0,
            parent_local: None,
            spawns: &mut spawns,
        };
        m::mv_yp_eff(&mut ctx, &mut obj, &p, 0);
        for k in 0..3 {
            let g = i32::from_le_bytes(ee_bytes(&ee, OBJ + w0 as u32 + 4 * k as u32, 4).try_into().unwrap());
            let r = obj.i32(w0 + 4 * k);
            assert!((g as i64 - r as i64).abs() <= 1, "velocity {v:?}: angle {k}: game {g:#x} port {r:#x}");
        }
    }
}

/// `pppKeDMat` (handler 0x1fa130) against the port: the game leaves view * bind * local in the program's work
/// slot 1 and projection * that in the draw matrix (+0x50); the port keeps both in world space, so the view (and
/// here an identity projection) put on the port's matrices must give the game's.
#[test]
fn ke_dmat_matches_the_game() {
    let Some(elf) = exe() else { return };
    let mut rs = 7u32;
    for case in 0..12 {
        let mut r = || pseudo(&mut rs) * 2.0 - 1.0;
        let bind = Mat4::from_scale_rotation_translation(Vec3::new(1.0 + r().abs(), 1.0, 1.0 + r().abs()), Quat::from_euler(EulerRot::XYZ, r(), r(), r()), Vec3::new(r() * 300.0, r() * 50.0, r() * 300.0));
        let view = Mat4::look_at_rh(Vec3::new(r() * 500.0, 150.0 + r() * 50.0, -600.0), Vec3::new(r() * 20.0, 100.0, 0.0), Vec3::Y);
        let Scene { mut ee, env } = scene(&elf, bind, view);
        let p = Program { kind: 0, param_size: 4, params: vec![0; 8], use_val: vec![0x48, 0x50 + 0x10 * case] };
        put_program(&mut ee, &p);
        let local = Mat4::from_scale_rotation_translation(Vec3::splat(0.5 + r().abs()), Quat::from_euler(EulerRot::ZYX, r(), r(), r()), Vec3::new(r() * 40.0, r() * 40.0, r() * 40.0));
        let mut obj = Obj { group: 0, mem: vec![0; OBJ_SIZE], cursors: vec![0], fresh: false, restart: false, parent: None, ring: Vec::new(), trail_prims: Vec::new(), heap: Vec::new(), seed: 0 };
        obj.set_mat(OBJ_LOCAL, local);
        sync_obj(&mut ee, &obj);
        run(&mut ee, 0x1fa130);
        let dpd = dpd_with(&raw_shape());
        let (mut draws, mut spawns, mut rng) = (Vec::new(), Vec::new(), 0u32);
        let mut ctx = Ctx { env: &env, dpd: &dpd, dpd_index: 0, draws: &mut draws, rng: &mut rng, running: true, fast_forward: false, obj_id: 0, prog_index: 0, parent_local: None, spawns: &mut spawns };
        crate::prog::ke_dmat(&mut ctx, &mut obj, &p, 0);
        let slot = OBJ_WORK + p.use_val[1] as usize;
        for (what, at, port) in [("work slot", slot, view * obj.mat(slot)), ("draw", crate::vm::OBJ_DRAW, view * obj.mat(crate::vm::OBJ_DRAW))] {
            let g = ee_bytes(&ee, OBJ + at as u32, 64);
            let game: Vec<f32> = g.chunks(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect();
            for (k, (a, b)) in game.iter().zip(port.to_cols_array()).enumerate() {
                assert!(near(*a, b), "case {case} {what} [{k}]: game {a} port {b}");
            }
        }
    }
}

#[test]
fn sno_amplitude_matches_the_game() {
    let Some(elf) = exe() else { return };
    let bind = Mat4::from_scale_rotation_translation(Vec3::new(1.7, 1.0, 1.0), Quat::IDENTITY, Vec3::ZERO);
    // The handler reads work 2 as a camera-space matrix (the port applies the view to it, so the
    // view is the identity here and both see the same model depth).
    let Scene { mut ee, mut env } = scene(&elf, bind, Mat4::IDENTITY);
    // Model table entry 3: a model whose first word has flag 2 clear.
    ee.w32(RES + 0x10, SHAPETAB);
    ee.w32(SHAPETAB + 3 * 0x24 + 0x1c, SHAPE);
    ee.w32(SHAPETAB + 3 * 0x24 + 0x18, 0);
    // A projection whose z translation (0x1db1038) is 250.
    let proj = Mat4::from_cols(Vec4::new(300.0, 0.0, 0.0, 0.0), Vec4::new(0.0, 300.0, 0.0, 0.0), Vec4::new(2048.0, 2048.0, -3.0, 1.0), Vec4::new(0.0, 0.0, 250.0, 0.0));
    write_mat(&mut ee, 0x1db1000, &proj);
    env.proj = Some(proj);
    let mut e = vec![0u8; 0x24];
    let w32 = |e: &mut Vec<u8>, o: usize, v: u32| e[o..o + 4].copy_from_slice(&v.to_le_bytes());
    w32(&mut e, 0, 0);
    w32(&mut e, 4, 3);
    w32(&mut e, 8, (-14.4f32).to_bits());
    w32(&mut e, 0xc, 36.2f32.to_bits());
    w32(&mut e, 0x10, (-12.0f32).to_bits());
    e[0x14] = 1;
    e[0x15] = 1;
    e[0x16] = 1;
    w32(&mut e, 0x18, (-80.0f32).to_bits());
    e[0x1c] = 0x48;
    e.extend_from_slice(&0xffff_f000u32.to_le_bytes());
    let p = Program { kind: 238, param_size: 0x20, params: e, use_val: vec![0x40, 0x60, 0x80] };
    put_program(&mut ee, &p);
    // The port needs a model to make a batch (its depth factor is checked below).
    let mut raw = raw_model(2, &[[50, 0, 0], [0, 50, 0], [0, 0, 50]], &[[4096, 0, 0], [0, 4096, 0], [0, 0, 4096]], &[[0, 1, 2]]);
    raw[0..2].copy_from_slice(&3i16.to_le_bytes());
    let dpd = Dpd { particles: vec![], textures: vec![], shapes: vec![], models: vec![crate::omd::Model::parse(&raw)], vsfs: vec![] };
    let mut obj = Obj { group: 0, mem: vec![0; OBJ_SIZE], cursors: vec![0], fresh: false, restart: false, parent: None, ring: Vec::new(), trail_prims: Vec::new(), heap: Vec::new(), seed: 0 };
    let (w0, w1, w2) = (OBJ_WORK + 0x40, OBJ_WORK + 0x60, OBJ_WORK + 0x80);
    obj.mem[w1 + 8..w1 + 12].copy_from_slice(&[128, 128, 128, 90]);
    obj.set_mat(w2, Mat4::from_translation(Vec3::new(1.0, 2.0, 300.0)));
    let mut rng = 0u32;
    for tick in 0..10 {
        obj.set_i32(OBJ_FRAME, tick * 0x1000);
        sync_obj(&mut ee, &obj);
        SNO.with(|c| c.borrow_mut().clear());
        run(&mut ee, 0x20a7e8);
        let (mut draws, mut spawns) = (Vec::new(), Vec::new());
        let mut ctx = Ctx {
            env: &env,
            dpd: &dpd,
            dpd_index: 0,
            draws: &mut draws,
            rng: &mut rng,
            running: true,
            fast_forward: false,
            obj_id: 0,
            prog_index: 0,
            parent_local: None,
            spawns: &mut spawns,
        };
        m::sno_draw(&mut ctx, &mut obj, &p, 0);
        let game = ee_bytes(&ee, OBJ + w0 as u32, 12);
        for k in 0..3 {
            let g = f32::from_le_bytes(game[4 * k..4 * k + 4].try_into().unwrap());
            assert!(near(g, obj.f32(w0 + 4 * k)), "tick {tick}: amplitude state {k}: game {g} port {}", obj.f32(w0 + 4 * k));
        }
        let rec = SNO.with(|c| c.borrow().clone());
        assert_eq!(rec.len(), 1, "tick {tick}: one model draw");
        // The amplitude the game hands to the model draw is the value times the bind's x scale
        // (byte +0x16 set); the taper flag word is byte +0x15; the blend argument is byte +0x1c.
        assert!(near(rec[0].0, obj.f32(w0) * env.bind_scale.x), "tick {tick}: amplitude passed {} port {}", rec[0].0, obj.f32(w0) * env.bind_scale.x);
        assert_eq!(rec[0].1 & 0xff, 1);
        assert_eq!(rec[0].2 & 0xff, 0x48);
        // The projection's z translation the draw sees is biased by entry +0x18 (-80) times 1e7
        // over the model's depth (300), and restored afterwards; the port's depth factor is the
        // unbiased over the biased value.
        assert_eq!(rec[0].3, m::f64_to_f32_trunc(250.0f64 - (-80.0f64 * 1.0e7 / 300.0)), "tick {tick}: biased z translation");
        assert_eq!(ee.f32_at(0x1db1038), 250.0);
        assert_eq!(draws.len(), 1, "tick {tick}: the port draws the model");
        assert!(near(draws[0].depth_scale, 250.0 / rec[0].3), "tick {tick}: depth factor {} vs {}", draws[0].depth_scale, 250.0 / rec[0].3);
        assert!(draws[0].depth_test);
        sync_obj(&mut ee, &obj);
    }
}

/// The game's camera setup (FUN_001078f0 with the disc's constants, then FUN_00106c48 for the
/// field of view and FUN_00106a68 for the matrix) against [`Projection`]: the 4 x 4 at 0x3b21f0
/// for several horizontal fields of view, and the z row.
#[test]
fn projection_matches_the_game() {
    let Some(elf) = exe() else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    // The three hardware setup calls at the start of FUN_001078f0 do nothing here.
    for a in [0x1067f8, 0x106818, 0x106858] {
        ee.hooks.insert(a, hook_nothing);
    }
    ee.call(0x1078f0, &[], &[], STACK).unwrap_or_else(|f| panic!("FUN_001078f0: {f:?}"));
    let p = Projection::from_exe(&elf).expect("projection constants");
    eprintln!("projection constants on the disc: {p:?}");
    const CAM: u32 = 0x3b21f0;
    for hfov in [1.5f32, 1.0, 0.7, 2.2] {
        ee.call(0x106c48, &[CAM as u64, 0], &[hfov], STACK).unwrap_or_else(|f| panic!("FUN_00106c48: {f:?}"));
        ee.call(0x106a68, &[CAM as u64], &[], STACK).unwrap_or_else(|f| panic!("FUN_00106a68: {f:?}"));
        let want = p.matrix(hfov);
        let got = Mat4::from_cols_array(&std::array::from_fn(|i| ee.f32_at(CAM + 4 * i as u32)));
        for (i, (g, w)) in got.to_cols_array().iter().zip(want.to_cols_array()).enumerate() {
            // Element 11 (the z row's w) is 0 in the game's stored matrix; the model draw adds the
            // w = z term itself (the draw reads rows 0, 1 and 3 packed into one register and row 2
            // separately, 0x20acf0..0x20ad20), so the port keeps 1 there.
            if i == 11 {
                continue;
            }
            assert!((g - w).abs() <= 1e-4 * (1.0 + w.abs()), "hfov {hfov}: element {i}: game {g} port {w}\n{got:?}\n{want:?}");
        }
        assert!(near(ee.f32_at(CAM + 0x40), p.focal(hfov)), "focal length");
        assert!(near(ee.f32_at(CAM + 0x48), p.vfov(hfov)), "vertical field of view");
    }
    let (p22, p32) = p.z_row();
    assert!(near(ee.f32_at(CAM + 0x60), p22) && near(ee.f32_at(CAM + 0x54), p32), "z row {p22} {p32}");
}

// ---- pppSnoMdlDttNorm geometry ----------------------------------------------------------------------

thread_local! {
    /// Per emitted triangle: the texture coordinates of its three vertices and their screen xyz words.
    static TRIS: RefCell<Vec<([[f32; 2]; 3], [[u32; 3]; 3])>> = const { RefCell::new(Vec::new()) };
    static BUMP: RefCell<u32> = const { RefCell::new(0) };
}

fn hook_bump(ee: &mut Ee) {
    let size = ee.gpr[reg::A0] as u32;
    let at = BUMP.with(|b| {
        let mut b = b.borrow_mut();
        let at = SCRATCH + *b;
        *b += (size + 15) & !15;
        at
    });
    ee.gpr[reg::V0] = at as u128;
}

fn hook_kick(ee: &mut Ee) {
    let a0 = ee.gpr[reg::A0] as u32;
    if ee.r32(a0) != 0x7000_000d {
        return;
    }
    let uv = |ee: &mut Ee, at: u32| [ee.f32_at(a0 + at), ee.f32_at(a0 + at + 4)];
    let xyz = |ee: &mut Ee, at: u32| [ee.r32(a0 + at), ee.r32(a0 + at + 4), ee.r32(a0 + at + 8)];
    let t = ([uv(ee, 0x50), uv(ee, 0x80), uv(ee, 0xb0)], [xyz(ee, 0x70), xyz(ee, 0xa0), xyz(ee, 0xd0)]);
    TRIS.with(|c| c.borrow_mut().push(t));
}

fn hook_zero(ee: &mut Ee) {
    ee.gpr[reg::V0] = 0;
}

/// A raw OMD model (DPD model packet: 0x10 header, then the OMD) of triangles with normals.
fn raw_model(flags: u32, verts: &[[i16; 3]], normals: &[[i16; 3]], tris: &[[u16; 3]]) -> Vec<u8> {
    let mut b = vec![0u8; 0x600];
    let omd = 0x10;
    let put32 = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    put32(&mut b, omd, flags);
    put32(&mut b, omd + 4, 0x40);
    put32(&mut b, omd + 8, 0x200);
    put32(&mut b, omd + 12, 0x300);
    b[omd + 0x10..omd + 0x12].copy_from_slice(&(tris.len() as i16).to_le_bytes());
    b[omd + 0x12..omd + 0x14].copy_from_slice(&(verts.len() as i16).to_le_bytes());
    let blk = omd + 0x40;
    b[blk + 1] = 1;
    b[blk + 2..blk + 4].copy_from_slice(&(tris.len() as i16).to_le_bytes());
    for (i, t) in tris.iter().enumerate() {
        let q = blk + 0x10 + 0x1c * i;
        for c in 0..3 {
            b[q + 4 * c..q + 4 * c + 4].copy_from_slice(&[0x60 + 8 * i as u8, 0x70 + 4 * c as u8, 0x80, 0x80]);
            b[q + 0xc + 2 * c..q + 0xe + 2 * c].copy_from_slice(&t[c].to_le_bytes());
            b[q + 0x14 + 2 * c..q + 0x16 + 2 * c].copy_from_slice(&t[c].to_le_bytes());
        }
    }
    b[blk + 0x10 + 0x1c * tris.len() + 1] = 0xff;
    for (i, v) in verts.iter().enumerate() {
        for c in 0..3 {
            b[omd + 0x200 + 6 * i + 2 * c..omd + 0x202 + 6 * i + 2 * c].copy_from_slice(&v[c].to_le_bytes());
        }
    }
    for (i, n) in normals.iter().enumerate() {
        for c in 0..3 {
            b[omd + 0x300 + 6 * i + 2 * c..omd + 0x302 + 6 * i + 2 * c].copy_from_slice(&n[c].to_le_bytes());
        }
    }
    b
}

/// The game's model draw (FUN_0020ab40, run in the interpreter) against `sno_draw`: for a small
/// model with stored normals that differ from the radial directions, every vertex's texture
/// coordinate moves by the screen displacement the port computes (the model is placed so that the
/// game's back-face and clip tests accept every triangle; those tests are compared separately).
#[test]
fn sno_geometry_matches_the_game() {
    let Some(elf) = exe() else { return };
    let r = 100i16;
    let verts: Vec<[i16; 3]> = vec![[r, 0, 0], [-r, 0, 0], [0, r, 0], [0, -r, 0], [0, 0, r], [0, 0, -r]];
    let normals: Vec<[i16; 3]> = verts
        .iter()
        .map(|v| {
            let d = Vec3::new(v[0] as f32 + 30.0, v[1] as f32 + 20.0, v[2] as f32 + 10.0).normalize();
            [(d.x * 4096.0) as i16, (d.y * 4096.0) as i16, (d.z * 4096.0) as i16]
        })
        .collect();
    let tris: Vec<[u16; 3]> = vec![[0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4], [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5]];
    // A scaled, turned model in camera space (the game keeps its draw matrix there).
    let m_cam = Mat4::from_scale_rotation_translation(Vec3::new(0.8, 0.8, 0.8), Quat::from_euler(EulerRot::ZYX, 0.4, 0.3, -0.2), Vec3::new(5.0, -8.0, 400.0));
    let mut raw = raw_model(2, &verts, &normals, &tris);
    raw[0..2].copy_from_slice(&3i16.to_le_bytes());
    let dpd = Dpd { particles: vec![], textures: vec![], shapes: vec![], models: vec![crate::omd::Model::parse(&raw)], vsfs: vec![] };
    let mut compared = 0;
    for (e15, e16) in [(0u8, 0u8), (1, 0), (0, 1), (1, 1)] {
        let mut game = Vec::new();
        for amp in [0.0f32, 12.0] {
            let Scene { mut ee, .. } = scene(&elf, Mat4::IDENTITY, Mat4::IDENTITY);
            ee.hooks.remove(&0x20ab40);
            ee.hooks.insert(0x20c110, hook_bump);
            ee.hooks.insert(0x1e4a88, hook_kick);
            ee.hooks.insert(0x2f8f60, hook_zero);
            BUMP.with(|b| *b.borrow_mut() = 0);
            TRIS.with(|c| c.borrow_mut().clear());
            ee.write(SHAPE, &raw);
            ee.w32(RES + 0x10, SHAPETAB);
            ee.w32(SHAPETAB + 3 * 0x24 + 0x1c, SHAPE + 0x10);
            // The disc's projection with a 300-pixel focal length (x and y scaled, w = z, screen centre
            // 2048), so that the game's clip and back-face tests, which read the flags, accept the model.
            let (p22, p32) = Projection::from_exe(&elf).expect("projection constants").z_row();
            let proj = [300.0f32, 0.0, 0.0, 0.0, 0.0, 300.0, 0.0, 0.0, 2048.0, 2048.0, p22, 1.0, 0.0, 0.0, p32, 0.0];
            for (i, x) in proj.iter().enumerate() {
                ee.wf32(0x1db1000 + 4 * i as u32, *x);
            }
            let p = sno_program(e15, e16);
            put_program(&mut ee, &p);
            let mut obj = sno_obj(amp, &m_cam);
            sync_obj(&mut ee, &obj);
            run(&mut ee, 0x20a7e8);
            game.push(TRIS.with(|c| c.borrow().clone()));
            let _ = &mut obj;
        }
        assert_eq!(game[0].len(), tris.len());
        // The port's batch for the same amplitude: camera-space positions and displacements.
        let p = sno_program(e15, e16);
        let mut obj = sno_obj(12.0, &m_cam);
        let env = Env::default();
        let (mut draws, mut spawns, mut rng) = (Vec::new(), Vec::new(), 0u32);
        let mut ctx = Ctx {
            env: &env,
            dpd: &dpd,
            dpd_index: 0,
            draws: &mut draws,
            rng: &mut rng,
            running: true,
            fast_forward: false,
            obj_id: 0,
            prog_index: 0,
            parent_local: None,
            spawns: &mut spawns,
        };
        m::sno_draw(&mut ctx, &mut obj, &p, 0);
        assert_eq!(draws.len(), 1);
        let b = &draws[0];
        assert_eq!(b.verts.len(), 3 * tris.len());
        let screen = |v: Vec3| Vec3::new(300.0 * v.x / v.z, 300.0 * v.y / v.z, 0.0);
        for (t, (g0, ga)) in game[0].iter().zip(&game[1]).enumerate() {
            for k in 0..3 {
                let i = 3 * t + k;
                let (pos, off) = (b.verts[i].pos, b.offsets[i]);
                let want = (screen(pos + off) - screen(pos)) / 1024.0;
                let got = [ga.0[k][0] - g0.0[k][0], ga.0[k][1] - g0.0[k][1]];
                assert!(
                    (got[0] - want.x).abs() < 3e-5 && (got[1] - want.y).abs() < 3e-5,
                    "e15 {e15} e16 {e16} tri {t} vertex {k}: game {got:?} port {:?} (offset {off:?})",
                    [want.x, want.y]
                );
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 4 * 24);
}

fn sno_program(e15: u8, e16: u8) -> Program {
    sno_program_full(e15, e16, 0.0, 0)
}

fn sno_program_full(e15: u8, e16: u8, e18: f32, e17: u8) -> Program {
    let mut e = vec![0u8; 0x20];
    e[4..8].copy_from_slice(&3u32.to_le_bytes());
    e[0x15] = e15;
    e[0x16] = e16;
    e[0x17] = e17;
    e[0x18..0x1c].copy_from_slice(&e18.to_le_bytes());
    e[0x1c] = 0x44;
    e.extend_from_slice(&0xffff_f000u32.to_le_bytes());
    Program { kind: 238, param_size: 0x20, params: e, use_val: vec![0x40, 0x60, 0x80] }
}

/// The octahedron of [`sno_geometry_matches_the_game`] with the model flag word `flags`.
fn octahedron(flags: u32) -> (Vec<u8>, usize) {
    let r = 100i16;
    let verts: Vec<[i16; 3]> = vec![[r, 0, 0], [-r, 0, 0], [0, r, 0], [0, -r, 0], [0, 0, r], [0, 0, -r]];
    let normals: Vec<[i16; 3]> = verts
        .iter()
        .map(|v| {
            let d = Vec3::new(v[0] as f32 + 30.0, v[1] as f32 + 20.0, v[2] as f32 + 10.0).normalize();
            [(d.x * 4096.0) as i16, (d.y * 4096.0) as i16, (d.z * 4096.0) as i16]
        })
        .collect();
    let tris: Vec<[u16; 3]> = vec![[0, 2, 4], [2, 1, 4], [1, 3, 4], [3, 0, 4], [2, 0, 5], [1, 2, 5], [3, 1, 5], [0, 3, 5]];
    let mut raw = raw_model(flags, &verts, &normals, &tris);
    raw[0..2].copy_from_slice(&3i16.to_le_bytes());
    (raw, tris.len())
}

/// One run of the game's model draw with the VU flag model installed: the triangles it kicks
/// (texture coordinates and screen xyz words), in order.
fn sno_game_tris(elf: &[u8], raw: &[u8], proj: &Mat4, p: &Program, amp: f32, m_cam: &Mat4) -> Vec<([[f32; 2]; 3], [[u32; 3]; 3])> {
    let Scene { mut ee, .. } = scene(elf, Mat4::IDENTITY, Mat4::IDENTITY);
    ee.hooks.remove(&0x20ab40);
    ee.hooks.insert(0x20c110, hook_bump);
    ee.hooks.insert(0x1e4a88, hook_kick);
    ee.hooks.insert(0x2f8f60, hook_zero);
    BUMP.with(|b| *b.borrow_mut() = 0);
    TRIS.with(|c| c.borrow_mut().clear());
    ee.write(SHAPE, raw);
    ee.w32(RES + 0x10, SHAPETAB);
    ee.w32(SHAPETAB + 3 * 0x24 + 0x1c, SHAPE + 0x10);
    write_mat(&mut ee, 0x1db1000, proj);
    put_program(&mut ee, p);
    let obj = sno_obj(amp, m_cam);
    sync_obj(&mut ee, &obj);
    run(&mut ee, 0x20a7e8);
    TRIS.with(|c| c.borrow().clone())
}

/// The model draw's clip test, back-face test and depth bias against the port, with the VU0
/// status and MAC flags the interpreter sets: models in front of the camera with
/// and without the cull flag, models partly outside the 4096-pixel window or in front of the
/// near plane, and a biased z row. The triangles the game kicks must be the port's, in order, with
/// the same screen x, y and z words (12.4 fixed) and the same displacement.
#[test]
fn sno_clip_and_cull_match_the_game() {
    let Some(elf) = exe() else { return };
    let disc = Projection::from_exe(&elf).expect("projection constants");
    let (p22, p32) = disc.z_row();
    // The disc's z row with a 300-pixel focal length.
    let proj = Mat4::from_cols(Vec4::new(300.0, 0.0, 0.0, 0.0), Vec4::new(0.0, 300.0, 0.0, 0.0), Vec4::new(2048.0, 2048.0, p22, 1.0), Vec4::new(0.0, 0.0, p32, 0.0));
    let rot = Quat::from_euler(EulerRot::ZYX, 0.4, 0.3, -0.2);
    let centred = Mat4::from_scale_rotation_translation(Vec3::splat(0.8), rot, Vec3::new(5.0, -8.0, 400.0));
    // Vertices from x 500 to 700 and z 0 to 200: the right side passes 4096 pixels, the nearest
    // vertex sits in front of the near plane (w <= 1).
    let clipped = Mat4::from_translation(Vec3::new(600.0, 0.0, 100.0));
    // (name, model flags, matrix, entry +0x18, entry +0x17, expect some rejection)
    let cases = [
        ("front, not culled", 2u32, centred, 0.0f32, 0u8, false),
        ("front, culled", 0, centred, -80.0, 0, true),
        ("front, culled, biased, no depth test", 0, centred, 25.0, 1, true),
        ("clipped, not culled", 2, clipped, 0.0, 0, true),
        ("clipped, culled", 0, clipped, -80.0, 0, true),
    ];
    let mut compared = 0;
    for (name, flags, m_cam, e18, e17, rejects) in cases {
        let (raw, ntris) = octahedron(flags);
        let mut kicked = 0;
        let dpd = Dpd { particles: vec![], textures: vec![], shapes: vec![], models: vec![crate::omd::Model::parse(&raw)], vsfs: vec![] };
        for (e15, e16) in [(0u8, 0u8), (1, 1)] {
            let p = sno_program_full(e15, e16, e18, e17);
            let g0 = sno_game_tris(&elf, &raw, &proj, &p, 0.0, &m_cam);
            let ga = sno_game_tris(&elf, &raw, &proj, &p, 12.0, &m_cam);
            assert_eq!(g0.len(), ga.len(), "{name}: the amplitude changes no triangle's fate");
            let mut obj = sno_obj(12.0, &m_cam);
            let env = Env { proj: Some(proj), ..Env::default() };
            let (mut draws, mut spawns, mut rng) = (Vec::new(), Vec::new(), 0u32);
            let mut ctx = Ctx { env: &env, dpd: &dpd, dpd_index: 0, draws: &mut draws, rng: &mut rng, running: true, fast_forward: false, obj_id: 0, prog_index: 0, parent_local: None, spawns: &mut spawns };
            m::sno_draw(&mut ctx, &mut obj, &p, 0);
            let port = draws.first().map_or(0, |b| b.indices.len() / 3);
            kicked = port;
            assert_eq!(g0.len(), port, "{name} (e15 {e15} e16 {e16}): triangles the game kicks vs the port's");
            if rejects {
                assert!(port < ntris, "{name}: nothing rejected");
            } else {
                assert_eq!(port, ntris, "{name}: everything drawn");
            }
            let Some(b) = draws.first() else { continue };
            assert_eq!(b.depth_test, e17 == 0, "{name}: depth test");
            let screen = |v: Vec3| {
                let c = proj * v.extend(1.0);
                c.truncate() / c.w
            };
            for (t, (g0, ga)) in g0.iter().zip(&ga).enumerate() {
                for k in 0..3 {
                    let i = 3 * t + k;
                    let (pos, off) = (b.verts[i].pos, b.offsets[i]);
                    let want = (screen(pos + off) - screen(pos)) / 1024.0;
                    let got = [ga.0[k][0] - g0.0[k][0], ga.0[k][1] - g0.0[k][1]];
                    assert!((got[0] - want.x).abs() < 3e-5 && (got[1] - want.y).abs() < 3e-5, "{name} tri {t} vertex {k}: displacement game {got:?} port {:?}", [want.x, want.y]);
                    // Screen x, y as the game converts them (12.4 fixed, truncated), and z from the
                    // biased z row (p32 / depth factor).
                    let s = screen(pos);
                    let (gx, gy, gz) = (g0.1[k][0] as i32 as f32 / 16.0, g0.1[k][1] as i32 as f32 / 16.0, g0.1[k][2] as i32 as f32 / 16.0);
                    assert!((gx - s.x).abs() < 0.07 && (gy - s.y).abs() < 0.07, "{name} tri {t} vertex {k}: screen game ({gx}, {gy}) port ({}, {})", s.x, s.y);
                    let z = p22 + (p32 / b.depth_scale) / pos.z;
                    assert!((gz - z).abs() <= 0.07 + 1e-5 * z.abs(), "{name} tri {t} vertex {k}: screen z game {gz} port {z} (factor {})", b.depth_scale);
                    compared += 1;
                }
            }
        }
        eprintln!("{name}: {kicked} of {ntris} triangles kicked by the game and the port");
    }
    assert!(compared >= 60, "only {compared} vertices compared");
}

/// What the disc holds for the Sno program and for the elements the app binds: the entries'
/// bytes +0x17 (depth test off) and +0x18 (depth bias) over the known users, and the element
/// flags and start waits of the files the app loads. Prints; asserts only that something was found.
#[test]
fn sno_entries_and_element_flags_on_the_disc() {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()));
    let Some(path) = path.filter(|p| std::path::Path::new(p).exists()) else { return };
    let mut a = kh2_archive::Archive::open(&path).expect("iso");
    let files = ["00effect.bar", "obj/P_EX100.apdx", "obj/P_EX120.apdx", "obj/M_EX600.apdx", "obj/N_BB050_BTL.apdx", "obj/B_EX130.apdx", "obj/B_EX140.apdx", "obj/N_HB630.apdx", "obj/W_EX010.apdx", "obj/M_EX020.apdx", "obj/M_EX010.apdx"];
    let (mut entries, mut elements) = (0, 0);
    for name in files {
        let Ok(file) = a.read(name) else { continue };
        let pax = match kh2_bar::parse(&file) {
            Ok(bar) => bar.iter().find(|e| e.kind == 18).and_then(|e| crate::Pax::parse(e.data).ok()),
            Err(_) => crate::Pax::parse(&file).ok(),
        };
        let Some(pax) = pax else { continue };
        for (di, dpd) in pax.dpds.iter().enumerate() {
            for (pi, pd) in dpd.particles.iter().enumerate() {
                for g in &pd.groups {
                    for prog in g.programs.iter().filter(|p| p.kind == 238) {
                        for i in 0..prog.entries() {
                            let o = i * prog.param_size;
                            let e18 = f32::from_le_bytes(prog.params[o + 0x18..o + 0x1c].try_into().unwrap());
                            eprintln!("{name} dpd {di} pdata {pi}: Sno entry model {} +0x14 {} +0x15 {} +0x16 {} +0x17 {} +0x18 {e18} alpha {:#x}", u16::from_le_bytes([prog.params[o + 4], prog.params[o + 5]]), prog.params[o + 0x14], prog.params[o + 0x15], prog.params[o + 0x16], prog.params[o + 0x17], prog.params[o + 0x1c]);
                            entries += 1;
                        }
                    }
                }
            }
        }
        for el in &pax.elements {
            if el.flags & 0x1 == 0 || el.flags & 0x2_0106 != 0 || el.start_wait != 0.0 || el.bone < 0 || (el.bone as u16) & 0xc000 != 0 {
                eprintln!("{name}: element id {} effect {} bone {:#x} flags {:#x} start wait {} category {:#x}", el.id, el.effect, el.bone, el.flags, el.start_wait, el.category);
            }
            elements += 1;
        }
    }
    assert!(entries > 0 && elements > 0, "{entries} Sno entries, {elements} elements");
}

fn sno_obj(amp: f32, m_cam: &Mat4) -> Obj {
    let mut obj = Obj { group: 0, mem: vec![0; OBJ_SIZE], cursors: vec![0], fresh: false, restart: false, parent: None, ring: Vec::new(), trail_prims: Vec::new(), heap: Vec::new(), seed: 0 };
    let (w0, w1, w2) = (OBJ_WORK + 0x40, OBJ_WORK + 0x60, OBJ_WORK + 0x80);
    obj.mem[w1 + 8..w1 + 12].copy_from_slice(&[128, 128, 128, 128]);
    obj.set_f32(w0, amp);
    // The entry is for frame 0; the object is at frame 1, so the amplitude stays as set.
    obj.set_i32(OBJ_FRAME, 0x1000);
    obj.set_mat(w2, *m_cam);
    obj
}

// ---- shape draw: texture coordinates and palette offset (FUN_0021a660) ----------------------------

const PKT: u32 = 0x01e0_0000;
const SHAPE_REC: u32 = 0x01f0_4000;
const SHAPE_MAT: u32 = 0x01f0_4100;
const SHAPE_FRAME: u32 = 0x01f0_5000;
const SPLICE: u32 = 0x01f0_6000;

/// A raw DPD shape of one frame: `quad` selects the free-quad sprite layout (frame flag 8). Each
/// sprite is (corners or rectangle in 12.4, texture rectangle in 4.12, color record).
fn raw_one_frame_shape(quad: bool, sprites: &[([i16; 8], [i16; 4], [u8; 8])]) -> Vec<u8> {
    let mut b = vec![0u8; 0x400];
    let a = 0x10;
    b[a + 4..a + 6].copy_from_slice(&1i16.to_le_bytes());
    b[a + 6..a + 8].copy_from_slice(&1i16.to_le_bytes());
    let v = 0x40;
    b[a + 0x10..a + 0x12].copy_from_slice(&((v - a) as i16).to_le_bytes());
    b[a + 0x12..a + 0x14].copy_from_slice(&1i16.to_le_bytes());
    b[v..v + 2].copy_from_slice(&(if quad { 8u16 } else { 0 }).to_le_bytes());
    b[v + 2..v + 4].copy_from_slice(&(sprites.len() as u16).to_le_bytes());
    let size = if quad { 24 } else { 16 };
    let colors = 0x20 + sprites.len() * size;
    b[v + 4..v + 8].copy_from_slice(&(colors as u32).to_le_bytes());
    // TEX0: TBP 0x2a00, TBW 4, PSMT8, 256 x 512, TCC, CBP 0x2e34, CLD 2.
    b[v + 0x10..v + 0x18].copy_from_slice(&0x4005_c686_6131_2a00u64.to_le_bytes());
    for (i, (pos, st, col)) in sprites.iter().enumerate() {
        let s = v + 0x20 + i * size;
        let n = if quad { 8 } else { 4 };
        for k in 0..n {
            b[s + 2 * k..s + 2 * k + 2].copy_from_slice(&pos[k].to_le_bytes());
        }
        for k in 0..4 {
            b[s + 2 * (n + k)..s + 2 * (n + k) + 2].copy_from_slice(&st[k].to_le_bytes());
        }
        b[v + colors + 8 * i..v + colors + 8 * i + 8].copy_from_slice(col);
    }
    b
}

/// FUN_0021a660, the shape draw behind every sprite program, run in the interpreter on sprite and
/// free-quad frames: the texture coordinates it writes into the GS packet (ST, normalised floats,
/// PRIM 0x56 / 0x54 with FST clear) are the sprite's raw values / 4096, the same as the port's
/// `Sprite::st`, for every texture size; the TEX0 it writes per sprite is the frame's with the
/// color record's byte 7 added to CBP, as `Sprite::tex0`. The port draws the same rectangles.
#[test]
fn shape_draw_texture_coordinates_match_the_game() {
    let Some(elf) = exe() else { return };
    let mut checked = 0;
    for quad in [false, true] {
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let sprites: Vec<([i16; 8], [i16; 4], [u8; 8])> = vec![
            ([-0x100, -0xf0, 0xf0, 0x110, -0x80, 0x120, 0x60, 0x90], [528, 16, 480, 464], [0x80, 0x40, 0x20, 0x80, 0x48, 0, 0, 0]),
            ([-0x40, -0x40, 0x40, 0x40, -0x30, 0x50, 0x48, 0x38], [3 * 64, 2 * 64, 26 * 64, 26 * 64], [0x60, 0x70, 0x80, 0x40, 0x44, 0, 0, 3]),
            ([0, -0x200, 0x200, 0, -0x200, 0, 0x10, 0x200], [4000, 64, 90, 4000], [0x10, 0x20, 0x30, 0x80, 0x42, 0x20, 0, 1]),
        ];
        let raw = raw_one_frame_shape(quad, &sprites);
        let shape = Shape::parse(&raw);
        let frame = &shape.frames[0];
        ee.write(SHAPE_FRAME, &raw[0x40..]);
        // The record: flags 0x80 (draw into the caller's packet memory at +0x14), color 0x80s, the
        // matrix (an identity with the screen centre and the GS z as the translation).
        ee.write(SHAPE_REC, &[0; 0x40]);
        ee.write(SHAPE_REC + 2, &0x80u16.to_le_bytes());
        ee.write(SHAPE_REC + 4, &[0x80, 0x80, 0x80, 0x80]);
        ee.w32(SHAPE_REC + 0x10, SHAPE_MAT);
        ee.w32(SHAPE_REC + 0x14, PKT);
        let m = Mat4::from_cols(Vec4::X, Vec4::Y, Vec4::Z, Vec4::new(2048.0, 2048.0, 4_194_304.0, 1.0));
        write_mat(&mut ee, SHAPE_MAT, &m);
        ee.w32(0x360b58, 0);
        ee.w32(0x1db11f0, 0x7fff_ffff);
        ee.w32(0x1db11e8, 0);
        ee.w32(0x1db11e4, PKT);
        for k in 0..256u32 {
            ee.w32(0x1dbec98 + 16 * k, SPLICE);
        }
        ee.call(0x21a660, &[SHAPE_FRAME as u64, SHAPE_REC as u64, 0x1000], &[], STACK).unwrap_or_else(|f| panic!("FUN_0021a660: {f:?}"));
        // The port's quads.
        let mut draws = Vec::new();
        let (mut rng, mut spawns) = (0u32, Vec::new());
        let env = Env::default();
        let dpd = dpd_with(&raw);
        {
            let mut ctx = Ctx { env: &env, dpd: &dpd, dpd_index: 0, draws: &mut draws, rng: &mut rng, running: true, fast_forward: false, obj_id: 0, prog_index: 0, parent_local: None, spawns: &mut spawns };
            crate::progx::draw_frame(&mut ctx, frame, Mat4::IDENTITY, Vec4::ONE);
        }
        let quads: Vec<(u64, [[f32; 2]; 4])> = draws.iter().flat_map(|b| b.verts.chunks(4).map(move |v| (b.tex0.unwrap(), [v[0].uv, v[1].uv, v[2].uv, v[3].uv]))).collect();
        assert_eq!(quads.len(), sprites.len(), "quad {quad}: the port draws every sprite");
        let size = if quad { 0xe0 } else { 0x70 };
        let mut at = PKT + 0x50;
        for (i, (s, (tex0, uv))) in frame.sprites.iter().zip(&quads).enumerate() {
            let tex = ee.r64(at);
            assert_eq!(tex, s.tex0(frame.tex0), "quad {quad} sprite {i}: TEX0 (palette offset {})", s.clut_add);
            assert_eq!(*tex0, tex, "quad {quad} sprite {i}: the port's batch texture");
            assert_eq!(ee.r64(at + 0x10) & 0xff, s.alpha as u64, "quad {quad} sprite {i}: ALPHA");
            // ST of the vertices the game writes, against the port's corners (TL, TR, BL, BR).
            let st = |ee: &mut Ee, o: u32| [ee.f32_at(at + o), ee.f32_at(at + o + 4)];
            let game: Vec<([f32; 2], [f32; 2])> = if quad {
                [0x20, 0x50, 0x80, 0xb0].iter().zip(uv).map(|(&o, p)| (st(&mut ee, o), *p)).collect()
            } else {
                vec![(st(&mut ee, 0x20), uv[0]), (st(&mut ee, 0x50), uv[3])]
            };
            for (k, (g, p)) in game.iter().enumerate() {
                assert!((g[0] - p[0]).abs() < 1e-6 && (g[1] - p[1]).abs() < 1e-6, "quad {quad} sprite {i} vertex {k}: game ST {g:?} port {p:?}");
            }
            eprintln!("quad {quad} sprite {i}: game ST {:?}, TEX0 {tex:#x}", game.iter().map(|x| x.0).collect::<Vec<_>>());
            checked += 1;
            at += size;
        }
    }
    assert_eq!(checked, 6);
}
// ---- weapon trail (pppRyjDrawShipolyBone: FUN_00200a20 -> FUN_002002f8) -----------------------------

const TRAIL_MODTAB: u32 = 0x01f0_7000;
const TRAIL_OMD: u32 = 0x01f0_7100;
const TRAIL_VBUF: u32 = 0x01f0_9000;
const TRAIL_POINTS: u32 = 0x01f0_9800;
const TRAIL_RING: u32 = 0x01f0_a000;

fn hook_trail_ring(ee: &mut Ee) {
    ee.gpr[reg::V0] = TRAIL_RING as u128;
}

/// A raw DPD model (0x10 header, then the OMD) of `segs` type-6 quads, as the trail uses it.
fn raw_trail_model(segs: usize) -> Vec<u8> {
    let mut b = vec![0u8; 0x40 + 0x28 * segs + 0x20];
    let omd = 0x10;
    b[omd + 4..omd + 8].copy_from_slice(&0x20u32.to_le_bytes());
    b[omd + 0x10..omd + 0x12].copy_from_slice(&(segs as i16).to_le_bytes());
    b[omd + 0x12..omd + 0x14].copy_from_slice(&((2 * segs + 2) as i16).to_le_bytes());
    b[omd + 0x16] = 0x48;
    let blk = omd + 0x20;
    b[blk + 1] = 6;
    b[blk + 2..blk + 4].copy_from_slice(&(segs as i16).to_le_bytes());
    for i in 0..segs {
        let q = blk + 0x10 + 0x28 * i;
        // File values that the game overwrites (colors 0x40, uvs 0x123).
        for k in 0..16 {
            b[q + k] = 0x40;
        }
        for k in 0..8 {
            b[q + 0x18 + 2 * k..q + 0x1a + 2 * k].copy_from_slice(&0x123u16.to_le_bytes());
        }
    }
    b[blk + 0x10 + 0x28 * segs + 1] = 0xff;
    b
}

/// The weapon trail against the game's FUN_00200a20 (called at the start of the draw handler
/// 0x200d58), which allocates and fills the ring, resets the model's colors and indices and runs
/// FUN_002002f8: the spline through the ring, the narrowing (entry +0xf), the stationary tail count,
/// the alphas and texture coordinates of the first pass, and the ring head. Over more ticks than the
/// ring holds, so the values kept after the first pass are compared too; the first ticks hold the
/// points still. Every vertex, alpha byte and texture coordinate is compared.
fn trail_check(n: u8, segs: usize, taper: u8, divs: [u8; 3]) {
    let Some(elf) = exe() else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let raw = raw_trail_model(segs);
    let dpd = Dpd { particles: vec![], textures: vec![], shapes: vec![], models: vec![crate::omd::Model::parse(&raw)], vsfs: vec![] };
    ee.w32(0x1db0e20, ZERO);
    ee.w32(0x1db0e24, RES);
    ee.w32(0x1db11dc, 0);
    ee.w32(RES + 0x10, TRAIL_MODTAB);
    ee.write(TRAIL_MODTAB, &[0; 0x24]);
    ee.w32(TRAIL_MODTAB, (2 * segs + 2) as u32);
    ee.w32(TRAIL_MODTAB + 8, segs as u32);
    ee.w32(TRAIL_MODTAB + 0x14, TRAIL_VBUF);
    ee.w32(TRAIL_MODTAB + 0x1c, TRAIL_OMD);
    ee.write(TRAIL_OMD, &raw[0x10..]);
    ee.w32(0x3625a0, TRAIL_POINTS);
    ee.w32(0x3625a4, 1);
    ee.hooks.insert(0x1e4cb0, hook_trail_ring);
    let mut params = vec![0u8; 0x18];
    params[0xd] = 0;
    params[0xe] = n;
    params[0xf] = taper;
    params[0x10..0x13].copy_from_slice(&divs);
    params[0x14..0x18].copy_from_slice(&0xffff_f000u32.to_le_bytes());
    let p = Program { kind: 200, param_size: 0x14, params, use_val: vec![0x20, 0x40] };
    put_program(&mut ee, &p);
    let mut obj = Obj { group: 0, mem: vec![0; OBJ_SIZE], cursors: vec![0], fresh: false, restart: false, parent: None, ring: Vec::new(), trail_prims: Vec::new(), heap: Vec::new(), seed: 0 };
    let c = OBJ_WORK + 0x20 + 8;
    obj.mem[c..c + 4].copy_from_slice(&[0x80; 4]);
    crate::prog::trail_init(&mut obj, &p);
    sync_obj(&mut ee, &obj);
    ee.call(0x200fd8, &[OBJ as u64, PROG as u64], &[], STACK).unwrap_or_else(|f| panic!("init: {f:?}"));
    let mut rng = 0u32;
    let mut compared = 0;
    for tick in 0..(3 * n as usize + 4) {
        // Held still for three ticks, then a swing: the base turns slowly, the tip sweeps an arc.
        let t = tick.saturating_sub(3) as f32;
        let a = Vec3::new(10.0 + t * 1.5, -100.0 + (t * 0.3).sin() * 8.0, 20.0 - t);
        let b = a + Vec3::new((t * 0.45).cos() * 90.0, -60.0 + (t * 0.45).sin() * 70.0, (t * 0.2).sin() * 30.0);
        for (k, v) in [a, b].iter().enumerate() {
            let at = TRAIL_POINTS + 16 * k as u32;
            ee.wf32(at, v.x);
            ee.wf32(at + 4, v.y);
            ee.wf32(at + 8, v.z);
            ee.wf32(at + 12, 1.0);
        }
        ee.call(0x200a20, &[OBJ as u64, ENTRY as u64, PROG as u64], &[], STACK).unwrap_or_else(|f| panic!("tick {tick}: FUN_00200a20: {f:?}"));
        let env = Env { two_point: Some([a, b]), ..Default::default() };
        let (mut draws, mut spawns) = (Vec::new(), Vec::new());
        {
            let mut ctx = Ctx { env: &env, dpd: &dpd, dpd_index: 0, draws: &mut draws, rng: &mut rng, running: true, fast_forward: false, obj_id: 0, prog_index: 0, parent_local: None, spawns: &mut spawns };
            crate::prog::trail_draw(&mut ctx, &mut obj, &p, 0);
        }
        let w1 = OBJ_WORK + 0x40;
        assert_eq!(ee.r32(OBJ + w1 as u32), obj.i32(w1) as u32, "tick {tick}: ring head");
        assert_eq!(ee.r8(OBJ + w1 as u32 + 5), obj.mem[w1 + 5], "tick {tick}: first-pass flag");
        assert_eq!(draws.len(), 1, "tick {tick}: one batch");
        let verts = &draws[0].verts;
        for i in 0..segs {
            let q = TRAIL_OMD + 0x20 + 0x10 + 0x28 * i as u32;
            for k in 0..4 {
                let v = verts[4 * i + k];
                // Corner k of quad i is vertex 2i + k.
                let vi = (2 * i + k) as u32;
                let g = [ee.r16(TRAIL_VBUF + 6 * vi) as i16, ee.r16(TRAIL_VBUF + 6 * vi + 2) as i16, ee.r16(TRAIL_VBUF + 6 * vi + 4) as i16];
                let port = v.pos - a;
                assert!(
                    (port.x - g[0] as f32).abs() < 0.01 && (port.y - g[1] as f32).abs() < 0.01 && (port.z - g[2] as f32).abs() < 0.01,
                    "tick {tick} vertex {vi}: game {g:?} port {port:?}"
                );
                let alpha = ee.r8(q + 4 * k as u32 + 3);
                assert_eq!((v.color.w * 128.0).round() as u8, alpha, "tick {tick} prim {i} corner {k}: alpha");
                assert_eq!(v.color.x * 128.0, 255.0, "tick {tick} prim {i} corner {k}: red");
                let (gu, gv) = (ee.r16(q + 0x18 + 4 * k as u32), ee.r16(q + 0x1a + 4 * k as u32));
                assert_eq!(v.uv, [(gu & 0xfff) as f32 / 4096.0, (gv & 0xfff) as f32 / 4096.0], "tick {tick} prim {i} corner {k}: uv");
                compared += 1;
            }
        }
    }
    eprintln!("trail (ring {n}, {segs} segments, taper {taper}, divisions {divs:?}): {compared} corners match the game");
}

#[test]
fn trail_matches_the_game() {
    trail_check(8, 16, 0, [0, 0, 0]);
}

#[test]
fn trail_narrowing_matches_the_game() {
    trail_check(6, 10, 1, [1, 2, 1]);
}