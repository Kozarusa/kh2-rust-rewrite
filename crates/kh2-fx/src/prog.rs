//! Particle program handlers, reimplemented from the behaviour of the game's own (read in a
//! local decompilation of the user's executable; nothing copied).
//!
//! Handler arguments mirror the game's: the object, the program (its parameters and useVal work
//! offsets) and the byte offset of the program's current parameter entry. Work offsets are
//! relative to object offset 0xa0. Value programs act on their entry only on the exact frame it
//! names; "Move"/"Accele" programs also integrate every tick.

use glam::{Mat4, Vec3, Vec4};

use crate::pax::Program;
use crate::rd::*;
use crate::vm::{Batch, Ctx, OBJ_DRAW, OBJ_LOCAL, OBJ_WORK, Obj, Vertex};

pub type Frame = fn(&mut Ctx, &mut Obj, &Program, usize);
pub type Init = fn(&mut Obj, &Program);

#[derive(Clone, Copy, Default)]
pub struct Handlers {
    pub frame: Option<Frame>,
    pub draw: Option<Frame>,
    pub init: Option<Init>,
    pub reset: Option<Init>,
    pub destroy: Option<Init>,
    /// The game leaves these out of the loop-restart fast-forward.
    pub skip_in_fast_forward: bool,
}

pub fn handlers(kind: u32) -> Handlers {
    use crate::prog_more as m;
    use crate::progx as x;
    let both = |frame: Frame, init: Init| Handlers { frame: Some(frame), init: Some(init), reset: Some(init), ..Default::default() };
    let frame = |frame: Frame| Handlers { frame: Some(frame), ..Default::default() };
    match crate::ppp::name(kind) {
        "pppPoint" | "pppScale" => both(point, zero_v3_0),
        "pppMove" | "pppAccele" | "pppSclMove" | "pppSclAccele" => both(mover, zero_v3_1),
        "pppAngle" => both(angle, zero_v3_0),
        "pppAngMove" | "pppAngAccele" => both(ang_move, zero_v3_1),
        "pppColor" => both(color, zero_c4_0),
        "pppColMove" | "pppColAccele" => both(col_move, zero_c4_1),
        "pppMatrix" | "pppMatrixXYZ" | "pppMatrixLoop" => frame(matrix_xyz),
        "pppMatrixYXZ" => frame(x::matrix_yxz),
        "pppMatrixZXY" => frame(x::matrix_zxy),
        "pppMatrixYZX" => frame(x::matrix_yzx),
        "pppMatrixXZY" => frame(x::matrix_xzy),
        "pppMatrixZYX" => frame(x::matrix_zyx),
        "pppMatrixScl" => frame(matrix_scl),
        "pppRyjMatrixNoRot" => frame(x::matrix_no_rot),
        "pppParMatrix" => frame(x::par_matrix),
        "pppDrawMatrix" | "pppDrawMatrixLoop" => frame(draw_matrix),
        "pppDrawMatrixFront" => frame(draw_matrix_front),
        "pppDrawMatrixWood" => frame(x::draw_matrix_wood),
        "pppKeDMatFr" => frame(x::ke_dmat_fr),
        "pppKeDMat" => frame(ke_dmat),
        "pppRandFV" => frame(x::rand_fv),
        "pppRandIV" => frame(x::rand_iv),
        "pppRandFloat" => frame(x::rand_float),
        "pppRandDownFV" => frame(x::rand_down_fv),
        "pppPointLoop" | "pppScaleLoop" => Handlers {
            frame: Some(x::point_loop),
            init: Some(x::loop_init),
            reset: Some(x::point_loop_reset),
            ..Default::default()
        },
        "pppAngleLoop" => Handlers {
            frame: Some(x::angle_loop),
            init: Some(x::loop_init),
            reset: Some(x::angle_loop_reset),
            ..Default::default()
        },
        "pppAngMoveLoop" => both(x::ang_move_loop, zero_v3_1),
        "pppDrawShapeX" | "pppDrawShape" => Handlers {
            frame: Some(x::shape_frame),
            draw: Some(x::shape_draw),
            init: Some(x::shape_init),
            reset: Some(x::shape_init),
            ..Default::default()
        },
        "pppDrawMdl3" | "pppDrawMdlLoopZ" | "pppDrawMdl" | "pppDrawMdl2" => Handlers { draw: Some(x::mdl3_draw), ..Default::default() },
        "pppDrawMdlTs3" => Handlers {
            frame: Some(x::mdl_ts3_frame),
            draw: Some(x::mdl_ts3_draw),
            init: Some(x::mdl_ts3_init),
            reset: Some(x::mdl_ts3_init),
            ..Default::default()
        },
        "pppKeShpTail2X" => Handlers {
            frame: Some(x::tail_frame),
            draw: Some(x::tail_draw),
            init: Some(x::tail_init),
            ..Default::default()
        },
        "pppKeShpTail3X" => Handlers {
            frame: Some(m::tail3_frame),
            draw: Some(m::tail3_draw),
            init: Some(m::tail3_init),
            reset: Some(m::tail3_reset),
            ..Default::default()
        },
        "pppKeMvYpEff" => frame(m::mv_yp_eff),
        "pppSnoMdlDttNorm" => Handlers { draw: Some(m::sno_draw), init: Some(m::sno_init), reset: Some(m::sno_init), ..Default::default() },
        "pppPointAp" => Handlers { frame: Some(x::point_ap), init: Some(x::point_ap_init), ..Default::default() },
        "pppVertexAp" => Handlers { frame: Some(x::vertex_ap), init: Some(x::vertex_ap_init), ..Default::default() },
        "pppVertexApLc" => Handlers { frame: Some(x::vertex_ap_lc), init: Some(x::vertex_ap_init), ..Default::default() },
        "pppVertexApAt" => Handlers { frame: Some(x::vertex_ap_at), init: Some(x::vertex_ap_init), ..Default::default() },
        "pppVertexAttend" => frame(x::vertex_attend),
        "pppKeBornRnd5" => Handlers { frame: Some(x::born_rnd5), init: Some(x::born_rnd5_init), ..Default::default() },
        "pppRyjDrawShipolyBone" => Handlers { draw: Some(trail_draw), init: Some(trail_init), ..Default::default() },
        "pppRyjMegaBirth" => Handlers {
            frame: Some(x::mega_frame),
            draw: Some(x::mega_draw),
            init: Some(x::mega_init),
            destroy: Some(x::mega_destroy),
            skip_in_fast_forward: true,
            ..Default::default()
        },
        "pppRyjMegaBirthModel" | "pppEiDrawShipoly" => Handlers { skip_in_fast_forward: true, ..Default::default() },
        // Depth-bias matrices for the VU1 programs and the shared-matrix pointer: no visible effect here.
        "pppKeZCrct" | "pppKeZCrctShp" | "pppSMatrix" => Handlers::default(),
        _ => Handlers::default(),
    }
}

/// Program kinds without a port yet.
pub fn unported(kind: u32) -> bool {
    let h = handlers(kind);
    h.frame.is_none() && h.draw.is_none() && !matches!(crate::ppp::name(kind), "pppKeZCrct" | "pppKeZCrctShp" | "pppSMatrix" | "pppDummyFunc")
}

fn work(p: &Program, k: usize) -> usize {
    (p.use_val.get(k).copied().unwrap_or(0) as usize) + OBJ_WORK
}

/// The entry applies this tick.
fn hit(obj: &Obj, p: &Program, e: usize) -> bool {
    p.frame_at(e) == obj.frame()
}

fn pf(p: &Program, e: usize, k: usize) -> f32 {
    f32_at(&p.params, e + 4 + 4 * k)
}
fn pi(p: &Program, e: usize, k: usize) -> i32 {
    i32_at(&p.params, e + 4 + 4 * k)
}
fn ps(p: &Program, e: usize, k: usize) -> i16 {
    i16_at(&p.params, e + 4 + 2 * k)
}

fn zero_v3_0(obj: &mut Obj, p: &Program) {
    obj.set_vec3(work(p, 0), Vec3::ZERO);
}
fn zero_v3_1(obj: &mut Obj, p: &Program) {
    obj.set_vec3(work(p, 1), Vec3::ZERO);
}
fn zero_c4(obj: &mut Obj, o: usize) {
    for k in 0..4 {
        obj.set_i16(o + 2 * k, 0);
    }
}
fn zero_c4_0(obj: &mut Obj, p: &Program) {
    zero_c4(obj, work(p, 0));
}
fn zero_c4_1(obj: &mut Obj, p: &Program) {
    zero_c4(obj, work(p, 1));
}

/// pppPoint / pppScale: add the entry's vector to the value on its frame.
fn point(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if ctx.running && hit(obj, p, e) {
        let o = work(p, 0);
        let v = obj.vec3(o) + Vec3::new(pf(p, e, 0), pf(p, e, 1), pf(p, e, 2));
        obj.set_vec3(o, v);
    }
}

/// pppMove / pppAccele / pppSclMove / pppSclAccele: the entry adds to the rate (work 1) on its
/// frame; every tick the rate adds to the value (work 0).
fn mover(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let (o, r) = (work(p, 0), work(p, 1));
    if hit(obj, p, e) {
        let v = obj.vec3(r) + Vec3::new(pf(p, e, 0), pf(p, e, 1), pf(p, e, 2));
        obj.set_vec3(r, v);
    }
    let v = obj.vec3(o) + obj.vec3(r);
    obj.set_vec3(o, v);
}

fn add_i3(obj: &mut Obj, o: usize, d: [i32; 3]) {
    for (k, dk) in d.iter().enumerate() {
        let v = obj.i32(o + 4 * k).wrapping_add(*dk);
        obj.set_i32(o + 4 * k, v);
    }
}

/// pppAngle: integer angles (0x10000 = one turn).
fn angle(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if ctx.running && hit(obj, p, e) {
        add_i3(obj, work(p, 0), [pi(p, e, 0), pi(p, e, 1), pi(p, e, 2)]);
    }
}

fn ang_move(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let (o, r) = (work(p, 0), work(p, 1));
    if hit(obj, p, e) {
        add_i3(obj, r, [pi(p, e, 0), pi(p, e, 1), pi(p, e, 2)]);
    }
    let d = [obj.i32(r), obj.i32(r + 4), obj.i32(r + 8)];
    add_i3(obj, o, d);
}

fn add_c4(obj: &mut Obj, o: usize, d: [i16; 4]) {
    for (k, dk) in d.iter().enumerate() {
        let v = obj.i16(o + 2 * k).wrapping_add(*dk);
        obj.set_i16(o + 2 * k, v);
    }
}

/// pppColor: RGBA as i16 (0x4000 = 0x80 after the >>7); every tick the tinted color is packed to
/// bytes at work+8.
fn color(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let o = work(p, 0);
    if ctx.running && hit(obj, p, e) {
        add_c4(obj, o, [ps(p, e, 0), ps(p, e, 1), ps(p, e, 2), ps(p, e, 3)]);
    }
    let t = ctx.env.tint.to_array();
    for k in 0..4 {
        let f = (obj.i16(o + 2 * k) as f32 * t[k]).min(32767.0);
        obj.mem[o + 8 + k] = ((f as i32 as u32) >> 7) as u8;
    }
}

fn col_move(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let (o, r) = (work(p, 0), work(p, 1));
    if hit(obj, p, e) {
        add_c4(obj, r, [ps(p, e, 0), ps(p, e, 1), ps(p, e, 2), ps(p, e, 3)]);
    }
    let d = [obj.i16(r), obj.i16(r + 2), obj.i16(r + 4), obj.i16(r + 6)];
    add_c4(obj, o, d);
}

/// The game's integer-angle rotation: 4096-entry sine table indexed by angle >> 4.
pub fn sin_turn(a: i32) -> f32 {
    let i = ((a as u32) >> 4) & 0xfff;
    (i as f32 * (std::f32::consts::TAU / 4096.0)).sin()
}
pub fn cos_turn(a: i32) -> f32 {
    sin_turn(a.wrapping_add(0x4000))
}

/// Rz · Ry · Rx from integer angles (column vectors).
pub fn euler_zyx(a: [i32; 3]) -> Mat4 {
    let (sx, cx) = (sin_turn(a[0]), cos_turn(a[0]));
    let (sy, cy) = (sin_turn(a[1]), cos_turn(a[1]));
    let (sz, cz) = (sin_turn(a[2]), cos_turn(a[2]));
    let rx = Mat4::from_cols(Vec4::X, Vec4::new(0.0, cx, sx, 0.0), Vec4::new(0.0, -sx, cx, 0.0), Vec4::W);
    let ry = Mat4::from_cols(Vec4::new(cy, 0.0, -sy, 0.0), Vec4::Y, Vec4::new(sy, 0.0, cy, 0.0), Vec4::W);
    let rz = Mat4::from_cols(Vec4::new(cz, sz, 0.0, 0.0), Vec4::new(-sz, cz, 0.0, 0.0), Vec4::Z, Vec4::W);
    rz * ry * rx
}

/// pppMatrix / pppMatrixXYZ: local = Rz·Ry·Rx(angle), columns scaled, translation = position.
fn matrix_xyz(_: &mut Ctx, obj: &mut Obj, p: &Program, _: usize) {
    let (pos, ang, scl) = (work(p, 0), work(p, 1), work(p, 2));
    let r = euler_zyx([obj.i32(ang), obj.i32(ang + 4), obj.i32(ang + 8)]);
    let s = obj.vec3(scl);
    let mut m = Mat4::from_cols(r.x_axis * s.x, r.y_axis * s.y, r.z_axis * s.z, r.w_axis);
    m.w_axis = obj.vec3(pos).extend(m.w_axis.w);
    obj.set_mat(OBJ_LOCAL, m);
}

/// pppMatrixScl: local = diag(scale) with translation = position.
fn matrix_scl(_: &mut Ctx, obj: &mut Obj, p: &Program, _: usize) {
    let s = obj.vec3(work(p, 1));
    let m = Mat4::from_cols(Vec4::X * s.x, Vec4::Y * s.y, Vec4::Z * s.z, obj.vec3(work(p, 0)).extend(1.0));
    obj.set_mat(OBJ_LOCAL, m);
}

/// pppDrawMatrix: draw = bind · local (the game also applies camera and projection; we keep world space).
fn draw_matrix(ctx: &mut Ctx, obj: &mut Obj, _: &Program, _: usize) {
    let m = ctx.env.bind * obj.mat(OBJ_LOCAL);
    obj.set_mat(OBJ_DRAW, m);
}

/// pppKeDMat (handler 0x1fa130, table entry 0x364bb0): the draw matrix is the bind times the local matrix (the game:
/// view * bind (0x1db0f80) times the object's +0x10), copied into the program's work slot 1 (the use value at +4 of
/// the use list), then the projection (0x1db1000) goes on the draw matrix. As for `pppKeDMatFr`, this crate keeps
/// both in world space (`pppSnoMdlDttNorm` puts the view on the copy).
pub(crate) fn ke_dmat(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    draw_matrix(ctx, obj, p, e);
    let m = obj.mat(OBJ_DRAW);
    obj.set_mat(work(p, 1), m);
}

/// pppDrawMatrixFront: camera-facing. In camera space the axes are the local axes scaled by the
/// bind scale and the origin is the bound position; back to world space for us.
pub(crate) fn draw_matrix_front_pub(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    draw_matrix_front(ctx, obj, p, e);
}

fn draw_matrix_front(ctx: &mut Ctx, obj: &mut Obj, _: &Program, _: usize) {
    let l = obj.mat(OBJ_LOCAL);
    let s = ctx.env.bind_scale;
    let t = ctx.env.view * ctx.env.bind * l.w_axis;
    let cam = Mat4::from_cols(l.x_axis * s.x, l.y_axis * s.y, l.z_axis * s.z, t);
    obj.set_mat(OBJ_DRAW, ctx.env.view_inv * cam);
}

// ---- pppRyjDrawShipolyBone: the weapon trail -------------------------------------------------

/// Trail state in work memory at useVal[1]: ring head (u32 at +0), ring size (u8 at +4),
/// "first pass" flag (u8 at +5, set until the ring wraps once). The ring itself is in `obj.ring`.
pub(crate) fn trail_init(obj: &mut Obj, p: &Program) {
    let o = work(p, 1);
    obj.set_i32(o, 0);
    obj.set_i32(o + 8, 0);
    obj.mem[o + 4] = 0;
    obj.mem[o + 5] = 1;
    obj.ring.clear();
    obj.trail_prims.clear();
}

/// Natural cubic spline through unit-spaced knots, as FUN_00200000 solves it: the second
/// derivatives over 6 (`m`), from `m[i-1] + 4 m[i] + m[i+1] = d[i] - d[i-1]` with `d[i] = p[i+1] - p[i]`
/// and `m[0] = m[n-1] = 0`, by forward elimination (`c[0] = 4`, `c[i] = 4 - 1 / c[i-1]`) and back
/// substitution, in the game's order of operations.
fn spline(p: &[Vec3]) -> Vec<Vec3> {
    let n = p.len();
    let mut m = vec![Vec3::ZERO; n];
    if n < 3 {
        return m;
    }
    let d: Vec<Vec3> = (0..n - 1).map(|i| p[i + 1] - p[i]).collect();
    let mut c = vec![0f32; n];
    m[1] = d[1] - d[0] - m[0];
    c[0] = 4.0;
    for i in 1..n - 2 {
        let r = 1.0 / c[i - 1];
        m[i + 1] = (d[i + 1] - d[i]) - m[i] * r;
        c[i] = 4.0 - r;
    }
    let last = m[n - 1];
    m[n - 2] -= last;
    for i in (1..n - 1).rev() {
        m[i] = (m[i] - m[i + 1]) / c[i - 1];
    }
    m
}

/// FUN_002001b8: the spline at `t` (knot `t as usize`, fraction `t - knot`), in the game's
/// Horner order. At the last knot the fraction is 0 and the knot itself comes back.
fn spline_at(p: &[Vec3], m: &[Vec3], t: f32) -> Vec3 {
    let i = t as usize;
    let f = t - i as f32;
    if i + 1 >= p.len() {
        return p[p.len() - 1];
    }
    let (p0, p1, m0, m1) = (p[i], p[i + 1], m[i], m[i + 1]);
    let e = m1 - m0;
    let s = m0 * 2.0 + m1;
    let g = (p1 - p0) - s;
    let h = e * f + m0 * 3.0;
    let h = h * f + g;
    h * f + p0
}

pub(crate) fn trail_draw(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let Some(points) = ctx.env.two_point else { return };
    let model_no = i32_at(&p.params, e + 4);
    if model_no == 0xffff {
        return;
    }
    let Some(model) = ctx.dpd.model(model_no) else { return };
    let st = work(p, 1);
    let ring_size = u8_at(&p.params, e + 0xe).max(2) as usize;
    if obj.ring.is_empty() {
        obj.mem[st + 4] = ring_size as u8;
        obj.ring = vec![points; ring_size];
    }
    let n = obj.mem[st + 4] as usize;
    let head = obj.i32(st) as usize % n;
    obj.ring[head] = points;
    let segs = model.prim_count;
    let mut verts = vec![[0i16; 3]; model.vertex_count.max(2 * segs + 2)];
    let base = points[0];
    for edge in 0..2 {
        let pts: Vec<Vec3> = (0..n).map(|k| obj.ring[(head + n - k) % n][edge] - base).collect();
        let m = spline(&pts);
        for i in 0..=segs {
            let v = spline_at(&pts, &m, i as f32 / segs as f32 * (n - 1) as f32);
            verts[2 * i + edge] = [v.x as i16, v.y as i16, v.z as i16];
        }
    }
    // FUN_00200a20 rewrites the model in place every tick: the color bytes r, g, b of every
    // primitive to 0xff and the quad indices to 2i..2i+3. The alpha bytes and the texture
    // coordinates are only written by FUN_002002f8 while the ring fills for the first time and keep
    // their last values after that (the model is the game's own copy, written in place); the port
    // keeps them per object in `trail_prims`.
    let mut prims = Vec::new();
    let mut tex0 = 0;
    let mut kind = 6;
    for b in &model.blocks {
        tex0 = b.tex0;
        kind = b.kind;
        for (i, pr) in b.prims.iter().enumerate() {
            let mut q = *pr;
            for c in 0..4 {
                q.colors[c] = [0xff, 0xff, 0xff, q.colors[c][3]];
            }
            let s = (2 * i) as u16;
            q.indices = [s, s + 1, s + 2, s + 3];
            if let Some((alpha, uvs)) = obj.trail_prims.get(prims.len()) {
                for (col, a) in q.colors.iter_mut().zip(alpha) {
                    col[3] = *a;
                }
                q.uvs = *uvs;
            }
            prims.push(q);
        }
    }
    let first_pass = obj.mem[st + 5] != 0;
    // The stationary tail (trailing points equal to their neighbour) is only counted on the first
    // pass; afterwards the whole trail counts as visible (FUN_002002f8).
    let mut still = 0usize;
    if first_pass {
        let mut i = segs;
        while i > 0 && verts[2 * i] == verts[2 * i - 2] {
            still += 1;
            i -= 1;
        }
    }
    // Entry byte +0xf: the trail narrows to its second edge toward the end. Vertex 2i of the first
    // edge becomes `first * t + second * (1 - t)` (FUN_002fb308) with t going from 1 at the newest
    // sample down by 1 / visible per sample, not below 0.
    if u8_at(&p.params, e + 0xf) != 0 {
        let visible = segs as i32 - still as i32;
        let step = if visible > 0 { 1.0 / visible as f32 } else { 1.0 };
        let mut t = 1.0f32;
        for i in 0..=segs {
            let (a, b) = (verts[2 * i], verts[2 * i + 1]);
            let f = |k: usize| (a[k] as f32 * t + b[k] as f32 * (1.0 - t)) as i32 as i16;
            verts[2 * i] = [f(0), f(1), f(2)];
            t -= step;
            if t <= 0.0 {
                t = 0.0;
            }
        }
    }
    if first_pass {
        let visible = segs as i32 - still as i32;
        let (mut u, u_end, v_top, v_bottom) = uv_range(&p.params[e..]);
        let (du, da) = if visible < 1 { (0, 0.0) } else { ((u_end - u) / visible, 128.0 / visible as f32) };
        if visible < 1 {
            u = 0;
        }
        let mut a = if visible < 1 { 0.0 } else { 128.0 };
        for (i, q) in prims.iter_mut().enumerate() {
            if (i as i32) < visible {
                let a0 = a as i32 as u8;
                let a1 = (((a as i32) & 0xff) as f32 - da) as i32 as u8;
                q.colors[0][3] = a0;
                q.colors[1][3] = a0;
                q.colors[2][3] = a1;
                q.colors[3][3] = a1;
            } else {
                for c in 0..4 {
                    q.colors[c][3] = 0;
                }
            }
            a = (a - da).max(0.0);
            if kind == 6 {
                let next = u + du;
                q.uvs = [[u as u16, v_bottom], [u as u16, v_top], [(next - 1) as u16, v_bottom], [(next - 1) as u16, v_top]];
                u = next.min(u_end);
            }
        }
        obj.trail_prims = prims.iter().map(|q| ([q.colors[0][3], q.colors[1][3], q.colors[2][3], q.colors[3][3]], q.uvs)).collect();
    }
    if ctx.running && u8_at(&p.params, e + 0xd) == 0 {
        let h = obj.i32(st) + 1;
        if h as usize >= n {
            obj.set_i32(st, 0);
            obj.mem[st + 5] = 0;
        } else {
            obj.set_i32(st, h);
        }
    }
    // Material color from the color program (work 0 holds the i16 color; bytes at +8).
    let co = work(p, 0) + 8;
    let mat = Vec4::new(obj.mem[co] as f32, obj.mem[co + 1] as f32, obj.mem[co + 2] as f32, obj.mem[co + 3] as f32) / 128.0;
    if mat.w <= 0.0 {
        return;
    }
    let mut batch = Batch { dpd: ctx.dpd_index, tex0: Some(tex0), alpha: model.alpha, ..Default::default() };
    for q in &prims {
        let corner = |k: usize| {
            let v = verts.get(q.indices[k] as usize).copied().unwrap_or([0; 3]);
            Vertex {
                pos: base + Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32),
                color: Vec4::new(q.colors[k][0] as f32, q.colors[k][1] as f32, q.colors[k][2] as f32, q.colors[k][3] as f32) / 128.0
                    * mat,
                uv: [(q.uvs[k][0] & 0xfff) as f32 / 4096.0, (q.uvs[k][1] & 0xfff) as f32 / 4096.0],
            }
        };
        batch.quad([corner(0), corner(1), corner(2), corner(3)]);
    }
    ctx.draws.push(batch);
}

/// U span and V edges in 1/4096 from the trail's texture-division parameters (+0x10..+0x12).
fn uv_range(e: &[u8]) -> (i32, i32, u16, u16) {
    let div = |b: u8| match b {
        1 => 0x800,
        2 => 0x400,
        _ => 0x1000,
    };
    let mut u_end = div(u8_at(e, 0x10));
    let v_end = div(u8_at(e, 0x11));
    let off = match u8_at(e, 0x12) {
        0 => 0,
        1 => 0x400,
        2 => 0x800,
        _ => 0xc00,
    };
    u_end += off;
    (off + 0x10, u_end - 0x10, 0x10, (v_end - 0x10) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spline_interpolates_knots() {
        let p = [Vec3::ZERO, Vec3::new(1.0, 2.0, 0.0), Vec3::new(3.0, 1.0, 1.0), Vec3::new(4.0, 4.0, 2.0)];
        let m = spline(&p);
        for (i, k) in p.iter().enumerate() {
            assert!((spline_at(&p, &m, i as f32) - *k).length() < 1e-4);
        }
        assert_eq!(m[0], Vec3::ZERO);
        assert_eq!(m[3], Vec3::ZERO);
    }

    #[test]
    fn integer_angles() {
        // 0x4000 = a quarter turn about Z: X maps to Y.
        let r = euler_zyx([0, 0, 0x4000]);
        assert!((r.transform_vector3(Vec3::X) - Vec3::Y).length() < 1e-3);
        assert!((sin_turn(0x8000)).abs() < 1e-3);
    }
}
