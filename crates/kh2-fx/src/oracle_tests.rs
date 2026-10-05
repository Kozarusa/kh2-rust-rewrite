//! Runs the game's own handlers of the three ported programs (`pppKeShpTail3X` frame 0x1fcf70 and
//! draw 0x1fd388, `pppKeMvYpEff` 0x1f4628, `pppSnoMdlDttNorm` draw 0x20a7e8 and its model draw
//! 0x20ab40) in the `kh2-ee` interpreter on the user's executable and compares them with this
//! crate's ports on the same objects, parameters and matrices. The routines that would draw or
//! allocate packets (0x21a660 shape draws, the packet allocator 0x20c110 and the DMA kick 0x1e4a88)
//! are replaced by hooks that record their arguments. Skipped without a disc (KH2_ISO or
//! local/iso.txt).
//!
//! What is not covered: the interpreter has no VU0 flag registers, so the back-face and clip tests
//! of the Sno model draw never reject there; the port's culling is the reading of the disassembly.

use std::cell::RefCell;

use glam::{EulerRot, Mat4, Quat, Vec3, Vec4};
use kh2_ee::{Ee, reg};

use crate::pax::{Dpd, Program};
use crate::prog_more as m;
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
    /// Sno: (amplitude, taper flag byte as a word, alpha register argument) of each model draw.
    static SNO: RefCell<Vec<(f32, u32, u32)>> = const { RefCell::new(Vec::new()) };
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
    let rec = (ee.f32_at(a0 + 0x20), ee.r32(a0 + 0x24), a1 as u32);
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
        ring: Vec::new(),
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
        let mut obj = Obj { group: 0, mem: vec![0; OBJ_SIZE], cursors: vec![0], fresh: false, restart: false, parent: None, ring: Vec::new(), heap: Vec::new(), seed: 0 };
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

#[test]
fn sno_amplitude_matches_the_game() {
    let Some(elf) = exe() else { return };
    let bind = Mat4::from_scale_rotation_translation(Vec3::new(1.7, 1.0, 1.0), Quat::IDENTITY, Vec3::ZERO);
    let Scene { mut ee, env } = scene(&elf, bind, Mat4::from_translation(Vec3::new(0.0, 0.0, 300.0)));
    // Model table entry 3: a model whose first word has flag 2 clear.
    ee.w32(RES + 0x10, SHAPETAB);
    ee.w32(SHAPETAB + 3 * 0x24 + 0x1c, SHAPE);
    ee.w32(SHAPETAB + 3 * 0x24 + 0x18, 0);
    ee.wf32(0x1db1038, 250.0);
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
    let dpd = dpd_with(&raw_shape());
    let mut obj = Obj { group: 0, mem: vec![0; OBJ_SIZE], cursors: vec![0], fresh: false, restart: false, parent: None, ring: Vec::new(), heap: Vec::new(), seed: 0 };
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
        // The view matrix's z translation is restored after the draw (the game biases it during it).
        assert_eq!(ee.f32_at(0x1db1038), 250.0);
        sync_obj(&mut ee, &obj);
    }
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
/// coordinate moves by the screen displacement the port computes (the interpreter has no VU flag
/// registers, so the game's back-face and clip tests never reject here; those are not compared).
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
            // Projection rows: x and y scaled, w = z.
            let proj = [300.0f32, 0.0, 0.0, 0.0, 0.0, 300.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
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
    let mut e = vec![0u8; 0x20];
    e[4..8].copy_from_slice(&3u32.to_le_bytes());
    e[0x15] = e15;
    e[0x16] = e16;
    e[0x1c] = 0x44;
    e.extend_from_slice(&0xffff_f000u32.to_le_bytes());
    Program { kind: 238, param_size: 0x20, params: e, use_val: vec![0x40, 0x60, 0x80] }
}

fn sno_obj(amp: f32, m_cam: &Mat4) -> Obj {
    let mut obj = Obj { group: 0, mem: vec![0; OBJ_SIZE], cursors: vec![0], fresh: false, restart: false, parent: None, ring: Vec::new(), heap: Vec::new(), seed: 0 };
    let (w0, w1, w2) = (OBJ_WORK + 0x40, OBJ_WORK + 0x60, OBJ_WORK + 0x80);
    obj.mem[w1 + 8..w1 + 12].copy_from_slice(&[128, 128, 128, 128]);
    obj.set_f32(w0, amp);
    // The entry is for frame 0; the object is at frame 1, so the amplitude stays as set.
    obj.set_i32(OBJ_FRAME, 0x1000);
    obj.set_mat(w2, *m_cam);
    obj
}
