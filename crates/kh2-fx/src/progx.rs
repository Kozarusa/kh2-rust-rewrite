//! Drawing and spawning programs: sprites (shapes), models, random values, matrix variants,
//! loop variants, child spawners (Ap/Attend), sprite tails and the MegaBirth particle system.
//! Reimplemented from the behaviour of the game's handlers (local study, nothing copied).

use glam::{Mat3, Mat4, Vec3, Vec4};

use crate::pax::Program;
use crate::prog::{euler_zyx, sin_turn};
use crate::rd::*;
use crate::shape::{Anim, Frame};
use crate::vm::{Batch, Ctx, OBJ_DRAW, OBJ_FRAME, OBJ_LOCAL, OBJ_WORK, Obj, Spawn, SpawnInit, Vertex};

pub(crate) fn work(p: &Program, k: usize) -> usize {
    (p.use_val.get(k).copied().unwrap_or(0) as usize) + OBJ_WORK
}
pub(crate) fn pb(p: &Program, e: usize, o: usize) -> u8 {
    u8_at(&p.params, e + o)
}
pub(crate) fn ph(p: &Program, e: usize, o: usize) -> i16 {
    i16_at(&p.params, e + o)
}
pub(crate) fn pw(p: &Program, e: usize, o: usize) -> i32 {
    i32_at(&p.params, e + o)
}
pub(crate) fn pf(p: &Program, e: usize, o: usize) -> f32 {
    f32_at(&p.params, e + o)
}

/// Color bytes packed by a color program (work + 8), as GS-scale floats (0x80 = 1.0).
pub(crate) fn packed_color(obj: &Obj, color_work: usize) -> Vec4 {
    let o = color_work + 8;
    Vec4::new(obj.mem[o] as f32, obj.mem[o + 1] as f32, obj.mem[o + 2] as f32, obj.mem[o + 3] as f32) / 128.0
}

/// Draws one shape frame with a world matrix and material color (FUN_0021a660): every sprite with
/// the frame's TEX0 plus its own palette offset, its texture rectangle as fractions of the texture
/// (the GS gets them as normalised ST, whatever the texture's size).
pub(crate) fn draw_frame(ctx: &mut Ctx, frame: &Frame, m: Mat4, mat: Vec4) {
    for s in &frame.sprites {
        let col = Vec4::new(s.rgba[0] as f32, s.rgba[1] as f32, s.rgba[2] as f32, s.rgba[3] as f32) / 128.0 * mat;
        if col.w <= 0.0 {
            continue;
        }
        let [u0, v0, w, h] = s.st;
        let uvs = [[u0, v0], [u0 + w, v0], [u0, v0 + h], [u0 + w, v0 + h]];
        let mut v = [Vertex { pos: Vec3::ZERO, color: col, uv: [0.0; 2] }; 4];
        for k in 0..4 {
            v[k].pos = m.transform_point3(Vec3::new(s.corners[k][0], s.corners[k][1], 0.0));
            v[k].uv = uvs[k];
        }
        batch_for(ctx, Some(s.tex0(frame.tex0)), s.alpha).quad(v);
    }
}

/// The batch for (texture, blend), appended if new.
pub(crate) fn batch_for<'a>(ctx: &'a mut Ctx, tex0: Option<u64>, alpha: u8) -> &'a mut Batch {
    let dpd = ctx.dpd_index;
    let i = match ctx.draws.iter().rposition(|b| b.dpd == dpd && b.tex0 == tex0 && b.alpha == alpha) {
        Some(i) if i + 1 == ctx.draws.len() => i,
        _ => {
            ctx.draws.push(Batch { dpd, tex0, alpha, ..Default::default() });
            ctx.draws.len() - 1
        }
    };
    &mut ctx.draws[i]
}

// ---- pppDrawShapeX ---------------------------------------------------------------------------

fn anim_at(obj: &Obj, o: usize) -> Anim {
    Anim { timer: obj.i16(o) as u16, next: obj.i16(o + 2) as u16, shown: obj.i16(o + 4) as u16 }
}
fn set_anim(obj: &mut Obj, o: usize, a: Anim) {
    obj.set_i16(o, a.timer as i16);
    obj.set_i16(o + 2, a.next as i16);
    obj.set_i16(o + 4, a.shown as i16);
}

pub fn shape_init(obj: &mut Obj, p: &Program) {
    set_anim(obj, work(p, 0), Anim::default());
}

pub fn shape_frame(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let Some(shape) = ctx.dpd.shape(pw(p, e, 4)) else { return };
    let o = work(p, 0);
    let mut a = anim_at(obj, o);
    a.step(shape, ph(p, e, 8));
    set_anim(obj, o, a);
}

pub fn shape_draw(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let Some(shape) = ctx.dpd.shape(pw(p, e, 4)) else { return };
    let a = anim_at(obj, work(p, 0));
    let Some(frame) = shape.frames.get(a.shown as usize) else { return };
    let mat = packed_color(obj, work(p, 1));
    let frame = frame.clone();
    draw_frame(ctx, &frame, obj.mat(OBJ_DRAW), mat);
}

// ---- pppDrawMdl3 / pppDrawMdlTs3 ---------------------------------------------------------------

fn draw_model(ctx: &mut Ctx, number: i32, m: Mat4, mat: Vec4, uv_off: [f32; 2]) {
    let Some(model) = ctx.dpd.model(number) else { return };
    if mat.w <= 0.0 {
        return;
    }
    let model = model.clone();
    let pos = |i: u16| {
        let v = model.vertices.get(i as usize).copied().unwrap_or([0; 3]);
        m.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32))
    };
    for b in &model.blocks {
        let textured = matches!(b.kind, 2 | 3 | 6 | 7);
        let tex0 = textured.then_some(b.tex0);
        let quad = b.kind >= 4;
        for pr in &b.prims {
            let n = if quad { 4 } else { 3 };
            let mut v = [Vertex { pos: Vec3::ZERO, color: Vec4::ZERO, uv: [0.0; 2] }; 4];
            for k in 0..n {
                let c = pr.colors[k.min(3)];
                v[k] = Vertex {
                    pos: pos(pr.indices[k]),
                    color: Vec4::new(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32) / 128.0 * mat,
                    uv: [
                        (pr.uvs[k][0] & 0xfff) as f32 / 4096.0 + uv_off[0],
                        (pr.uvs[k][1] & 0xfff) as f32 / 4096.0 + uv_off[1],
                    ],
                };
            }
            let bt = batch_for(ctx, tex0, model.alpha);
            if quad {
                bt.quad(v);
            } else {
                let base = bt.verts.len() as u32;
                bt.verts.extend_from_slice(&v[..3]);
                bt.indices.extend_from_slice(&[base, base + 1, base + 2]);
            }
        }
    }
}

pub fn mdl3_draw(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let mat = packed_color(obj, work(p, 0));
    draw_model(ctx, pw(p, e, 4), obj.mat(OBJ_DRAW), mat, [0.0; 2]);
}

/// pppDrawMdlTs3: model with a scrolling texture (u, v each with rate and acceleration).
pub fn mdl_ts3_init(obj: &mut Obj, p: &Program) {
    let o = work(p, 2);
    for k in 0..6 {
        obj.set_f32(o + 4 * k, 0.0);
    }
}

pub fn mdl_ts3_frame(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let o = work(p, 2);
    let vu = obj.f32(o + 4) + obj.f32(o + 8);
    let vv = obj.f32(o + 0x10) + obj.f32(o + 0x14);
    let u = obj.f32(o) + vu;
    let v = obj.f32(o + 0xc) + vv;
    obj.set_f32(o + 4, vu);
    obj.set_f32(o + 0x10, vv);
    obj.set_f32(o, u);
    obj.set_f32(o + 0xc, v);
    if p.frame_at(e) == obj.frame() {
        let add = |obj: &mut Obj, k: usize, x: f32| {
            let v = obj.f32(o + k) + x;
            obj.set_f32(o + k, v);
        };
        add(obj, 0, pf(p, e, 8));
        add(obj, 4, pf(p, e, 0xc));
        add(obj, 8, pf(p, e, 0x10));
        add(obj, 0xc, pf(p, e, 0x14));
        add(obj, 0x10, pf(p, e, 0x18));
        add(obj, 0x14, pf(p, e, 0x1c));
    }
}

pub fn mdl_ts3_draw(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let o = work(p, 2);
    // The game passes the offsets as integers in 1/65536 of the texture (the UV scale).
    let off = [(obj.f32(o) * 65536.0) as i32 as f32 / 65536.0, (obj.f32(o + 0xc) * 65536.0) as i32 as f32 / 65536.0];
    let mat = packed_color(obj, work(p, 0));
    draw_model(ctx, pw(p, e, 4), obj.mat(OBJ_DRAW), mat, off);
}

// ---- random values ------------------------------------------------------------------------------

/// The per-object random factor, drawn at frame 0: two uniforms summed (or one doubled), in [0, 2).
fn rand2(ctx: &mut Ctx, two: bool) -> f32 {
    let a = ctx.rand();
    let b = if two { ctx.rand() } else { a };
    a + b
}

fn rand_target(_obj: &mut Obj, p: &Program, e: usize) -> Option<usize> {
    let off = pw(p, e, 4);
    (off >= 0).then(|| off as usize + OBJ_WORK)
}

/// pppRandFV: target vector += p·r − p per component.
pub fn rand_fv(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let r_at = work(p, 0);
    if obj.frame() == 0 {
        let r = rand2(ctx, pb(p, e, 0x14) != 0);
        obj.set_f32(r_at, r);
    } else if p.frame_at(e) != obj.frame() {
        return;
    }
    let r = obj.f32(r_at);
    if let Some(t) = rand_target(obj, p, e) {
        for k in 0..3 {
            let x = pf(p, e, 8 + 4 * k);
            let v = obj.f32(t + 4 * k) + (x * r - x);
            obj.set_f32(t + 4 * k, v);
        }
    }
}

/// pppRandIV: integer vector += (int)(p·r − p).
pub fn rand_iv(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running || p.frame_at(e) != obj.frame() {
        return;
    }
    let r = rand2(ctx, pb(p, e, 0x14) != 0);
    obj.set_f32(work(p, 0), r);
    if let Some(t) = rand_target(obj, p, e) {
        for k in 0..3 {
            let x = pw(p, e, 8 + 4 * k) as f32;
            let v = obj.i32(t + 4 * k).wrapping_add((x * r - x) as i32);
            obj.set_i32(t + 4 * k, v);
        }
    }
}

/// pppRandFloat: target += p·r − p.
pub fn rand_float(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let r_at = work(p, 0);
    if obj.frame() == 0 {
        let r = rand2(ctx, pb(p, e, 0xc) != 0);
        obj.set_f32(r_at, r);
    } else if p.frame_at(e) != obj.frame() {
        return;
    }
    let r = obj.f32(r_at);
    if let Some(t) = rand_target(obj, p, e) {
        let x = pf(p, e, 8);
        let v = obj.f32(t) + (x * r - x);
        obj.set_f32(t, v);
    }
}

/// pppRandDownFV: target vector += p·r with r in (−1, 0] (or the mean of two such).
pub fn rand_down_fv(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let r_at = work(p, 0);
    if obj.frame() == 0 {
        let mut r = -ctx.rand();
        if pb(p, e, 0x14) != 0 {
            r = (r - ctx.rand()) * 0.5;
        }
        obj.set_f32(r_at, r);
    } else if p.frame_at(e) != obj.frame() {
        return;
    }
    let r = obj.f32(r_at);
    if let Some(t) = rand_target(obj, p, e) {
        for k in 0..3 {
            let v = obj.f32(t + 4 * k) + pf(p, e, 8 + 4 * k) * r;
            obj.set_f32(t + 4 * k, v);
        }
    }
}

// ---- matrix variants ----------------------------------------------------------------------------

fn rot_x(a: i32) -> Mat4 {
    euler_zyx([a, 0, 0])
}
fn rot_y(a: i32) -> Mat4 {
    euler_zyx([0, a, 0])
}
fn rot_z(a: i32) -> Mat4 {
    euler_zyx([0, 0, a])
}

/// Matrix programs with another Euler order: `order` lists axes in application order.
fn matrix_order(obj: &mut Obj, p: &Program, order: [u8; 3]) {
    let (pos, ang, scl) = (work(p, 0), work(p, 1), work(p, 2));
    let a = [obj.i32(ang), obj.i32(ang + 4), obj.i32(ang + 8)];
    let r = |axis: u8| match axis {
        0 => rot_x(a[0]),
        1 => rot_y(a[1]),
        _ => rot_z(a[2]),
    };
    let rm = r(order[2]) * r(order[1]) * r(order[0]);
    let s = obj.vec3(scl);
    let mut m = Mat4::from_cols(rm.x_axis * s.x, rm.y_axis * s.y, rm.z_axis * s.z, rm.w_axis);
    m.w_axis = obj.vec3(pos).extend(m.w_axis.w);
    obj.set_mat(OBJ_LOCAL, m);
}

pub fn matrix_yxz(_: &mut Ctx, obj: &mut Obj, p: &Program, _: usize) {
    matrix_order(obj, p, [1, 0, 2]);
}
pub fn matrix_zxy(_: &mut Ctx, obj: &mut Obj, p: &Program, _: usize) {
    matrix_order(obj, p, [2, 0, 1]);
}
pub fn matrix_yzx(_: &mut Ctx, obj: &mut Obj, p: &Program, _: usize) {
    matrix_order(obj, p, [1, 2, 0]);
}
pub fn matrix_xzy(_: &mut Ctx, obj: &mut Obj, p: &Program, _: usize) {
    matrix_order(obj, p, [0, 2, 1]);
}
pub fn matrix_zyx(_: &mut Ctx, obj: &mut Obj, p: &Program, _: usize) {
    matrix_order(obj, p, [2, 1, 0]);
}

/// pppRyjMatrixNoRot: the inverse camera rotation (faces the camera), scaled, at the position.
pub fn matrix_no_rot(ctx: &mut Ctx, obj: &mut Obj, p: &Program, _: usize) {
    let r = Mat3::from_mat4(ctx.env.view_inv);
    let s = obj.vec3(work(p, 2));
    let mut m = Mat4::from_mat3(r);
    m.x_axis *= s.x;
    m.y_axis *= s.y;
    m.z_axis *= s.z;
    m.w_axis = obj.vec3(work(p, 0)).extend(1.0);
    obj.set_mat(OBJ_LOCAL, m);
}

/// pppParMatrix: the position moves into the parent object's local space.
pub fn par_matrix(ctx: &mut Ctx, obj: &mut Obj, _: &Program, _: usize) {
    let Some(par) = ctx.parent_local else { return };
    let mut m = obj.mat(OBJ_LOCAL);
    m.w_axis = par * m.w_axis;
    obj.set_mat(OBJ_LOCAL, m);
}

/// pppDrawMatrixWood: a billboard that keeps the bind's up axis (turns about it toward the camera).
pub fn draw_matrix_wood(ctx: &mut Ctx, obj: &mut Obj, _: &Program, _: usize) {
    let env = ctx.env;
    let cb = env.view * env.bind;
    let up = cb.y_axis.truncate().normalize_or_zero();
    let x = Vec3::new(up.y, -up.x, 0.0).normalize_or_zero();
    let z = x.cross(up).normalize_or_zero();
    let w = Mat4::from_cols(x.extend(0.0), up.extend(0.0), z.extend(0.0), cb.w_axis);
    let l = obj.mat(OBJ_LOCAL);
    let s = env.bind_scale;
    let rot = w * Mat4::from_cols(l.x_axis * s.x, l.y_axis * s.y, l.z_axis * s.z, Vec4::W);
    let mut cam = rot;
    cam.w_axis = cb * l.w_axis;
    obj.set_mat(OBJ_DRAW, env.view_inv * cam);
}

/// pppKeDMatFr: camera-facing draw matrix; also copies it into work memory.
pub fn ke_dmat_fr(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    crate::prog::draw_matrix_front_pub(ctx, obj, p, e);
    let m = obj.mat(OBJ_DRAW);
    obj.set_mat(work(p, 1), m);
}

// ---- loop variants ------------------------------------------------------------------------------

/// Loop variants keep accumulating across loop restarts: the restart only takes back the first
/// entry, which is applied again at frame 0.
pub fn loop_init(obj: &mut Obj, p: &Program) {
    obj.set_i32(work(p, 0) + 0xc, 0);
}

fn loop_first(obj: &mut Obj, p: &Program) {
    let o = work(p, 0);
    if obj.i32(o + 0xc) == 0 {
        obj.set_vec3(o, Vec3::ZERO);
        obj.set_i32(o + 0xc, 1);
    }
}

pub fn point_loop(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    loop_first(obj, p);
    if p.frame_at(e) == obj.frame() {
        let o = work(p, 0);
        let v = obj.vec3(o) + Vec3::new(pf(p, e, 4), pf(p, e, 8), pf(p, e, 0xc));
        obj.set_vec3(o, v);
    }
}

pub fn point_loop_reset(obj: &mut Obj, p: &Program) {
    let o = work(p, 0);
    let v = obj.vec3(o) - Vec3::new(pf(p, 0, 4), pf(p, 0, 8), pf(p, 0, 0xc));
    obj.set_vec3(o, v);
}

pub fn angle_loop(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let o = work(p, 0);
    if obj.i32(o + 0xc) == 0 {
        for k in 0..3 {
            obj.set_i32(o + 4 * k, 0);
        }
        obj.set_i32(o + 0xc, 1);
    }
    if p.frame_at(e) == obj.frame() {
        for k in 0..3 {
            let mut v = obj.i32(o + 4 * k).wrapping_add(pw(p, e, 4 + 4 * k));
            while v > 0x10000 {
                v -= 0x10000;
            }
            obj.set_i32(o + 4 * k, v);
        }
    }
}

pub fn angle_loop_reset(obj: &mut Obj, p: &Program) {
    let o = work(p, 0);
    for k in 0..3 {
        let v = obj.i32(o + 4 * k).wrapping_sub(pw(p, 0, 4 + 4 * k));
        obj.set_i32(o + 4 * k, v);
    }
}

/// pppAngMoveLoop: like AngMove, but the rate is not applied during the loop fast-forward.
pub fn ang_move_loop(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let (o, r) = (work(p, 0), work(p, 1));
    if p.frame_at(e) == obj.frame() {
        for k in 0..3 {
            let v = obj.i32(r + 4 * k).wrapping_add(pw(p, e, 4 + 4 * k));
            obj.set_i32(r + 4 * k, v);
        }
    }
    if !ctx.fast_forward {
        for k in 0..3 {
            let v = obj.i32(o + 4 * k).wrapping_add(obj.i32(r + 4 * k));
            obj.set_i32(o + 4 * k, v);
        }
    }
}

// ---- spawners -----------------------------------------------------------------------------------

/// pppPointAp: every `interval` ticks, spawns `count` objects of group `child` at this position.
pub fn point_ap_init(obj: &mut Obj, p: &Program) {
    obj.mem[work(p, 1) + 1] = 0;
}

pub fn point_ap(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let t = work(p, 1) + 1;
    let mut left = obj.mem[t];
    if left == 0 {
        let pos = obj.vec3(work(p, 0));
        for _ in 0..pb(p, e, 0xd) {
            ctx.spawns.push(Spawn {
                group: pw(p, e, 4) as usize,
                parent: ctx.obj_id,
                init: SpawnInit::Vec3(pw(p, e, 8) as usize, pos),
            });
        }
        left = pb(p, e, 0xc);
    }
    obj.mem[t] = left.wrapping_sub(1);
}

/// Vertex list of a VSF: (model vertices, index list).
fn vsf_vertices<'a>(ctx: &'a Ctx, vsf: i16) -> Option<(&'a [[i16; 3]], &'a [u16])> {
    let v = ctx.dpd.vsfs.iter().find(|v| v.number == vsf as i32)?;
    let m = ctx.dpd.model(v.model)?;
    Some((&m.vertices, &v.indices))
}

#[derive(Clone, Copy)]
enum ApKind {
    /// pppVertexAp: vertex through this object's local matrix.
    World,
    /// pppVertexApLc: raw vertex.
    Local,
    /// pppVertexApAt: the vertex index (the child follows it with pppVertexAttend).
    Index,
}

fn vertex_ap_common(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize, kind: ApKind) {
    if !ctx.running || ctx.fast_forward {
        return;
    }
    let vsf = ph(p, e, 4);
    if vsf < 0 {
        return;
    }
    let st = work(p, 0);
    let mut wait = obj.i16(st + 2) as u16;
    if wait == 0 {
        let Some((verts, idx)) = vsf_vertices(ctx, vsf) else { return };
        let (verts, idx) = (verts.to_vec(), idx.to_vec());
        let n = idx.len().max(1);
        let local = obj.mat(OBJ_LOCAL);
        for _ in 0..pb(p, e, 6) {
            let k = match pb(p, e, 8) {
                0 => {
                    let mut c = obj.i16(st) as u16 as usize;
                    if c >= n {
                        c = 0;
                    }
                    obj.set_i16(st, (c + 1) as i16);
                    c
                }
                1 => ((ctx.rand() * n as f32) as usize).min(n - 1),
                _ => break,
            };
            let vi = idx.get(k).copied().unwrap_or(0);
            let v = verts.get(vi as usize).copied().unwrap_or([0; 3]);
            let v3 = Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32);
            let child = pw(p, e, 0xc);
            if child == -1 {
                continue;
            }
            let off = pw(p, e, 0x10) as usize;
            let init = match kind {
                ApKind::World => SpawnInit::Vec3(off, local.transform_point3(v3)),
                ApKind::Local => SpawnInit::Vec3(off, v3),
                ApKind::Index => SpawnInit::U16(off, k as u16),
            };
            ctx.spawns.push(Spawn { group: child as usize, parent: ctx.obj_id, init });
        }
        wait = pb(p, e, 7) as u16;
    }
    obj.set_i16(st + 2, wait.wrapping_sub(1) as i16);
}

pub fn vertex_ap_init(obj: &mut Obj, p: &Program) {
    let o = work(p, 0);
    obj.set_i16(o, 0);
    obj.set_i16(o + 2, 0);
}
pub fn vertex_ap(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    vertex_ap_common(ctx, obj, p, e, ApKind::World);
}
pub fn vertex_ap_lc(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    vertex_ap_common(ctx, obj, p, e, ApKind::Local);
}
pub fn vertex_ap_at(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    vertex_ap_common(ctx, obj, p, e, ApKind::Index);
}

/// pppVertexAttend: position = parent local × the VSF vertex chosen at spawn.
pub fn vertex_attend(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let vsf = ph(p, e, 0xc);
    if vsf < 0 {
        return;
    }
    let Some((verts, idx)) = vsf_vertices(ctx, vsf) else { return };
    let k = u16::from_le_bytes([obj.mem[work(p, 0)], obj.mem[work(p, 0) + 1]]) as usize;
    let vi = idx.get(k).copied().unwrap_or(0) as usize;
    let v = verts.get(vi).copied().unwrap_or([0; 3]);
    let par = ctx.parent_local.unwrap_or(Mat4::IDENTITY);
    let w = par.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32));
    obj.set_vec3(work(p, 1), w);
}

// ---- per-object heap blocks ---------------------------------------------------------------------

#[derive(Clone, Debug)]
pub enum Heap {
    Mega(Mega),
}

fn heap(obj: &mut Obj, pi: usize) -> Option<&mut Heap> {
    obj.heap.iter_mut().find(|(k, _)| *k == pi).map(|(_, h)| h)
}

// ---- pppKeShpTail2X: a chain of sprites along the recent path ------------------------------------

/// Work: [0] ring length (0x1f), [1] ring head, [2..8] shape animation, ring of positions at +0x10.
pub fn tail_init(obj: &mut Obj, p: &Program) {
    let o = work(p, 0);
    obj.mem[o] = 0x1f;
    obj.mem[o + 1] = 0;
    set_anim(obj, o + 2, Anim::default());
}

pub(crate) fn tail_pos(ctx: &Ctx, obj: &Obj, world: bool) -> Vec4 {
    let t = obj.mat(OBJ_LOCAL).w_axis;
    let mut v = if world { ctx.env.bind * t } else { t };
    v.w = 1.0;
    v
}

pub fn tail_frame(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let o = work(p, 0);
    let world = pb(p, e, 0x26) != 0;
    let n = obj.mem[o].max(1) as usize;
    if obj.frame() == 0 && !obj.restart {
        let v = tail_pos(ctx, obj, world);
        for k in 0..n {
            obj.set_vec3(o + 0x10 + 0x10 * k, v.truncate());
            obj.set_f32(o + 0x1c + 0x10 * k, v.w);
        }
    }
    let mut head = obj.mem[o + 1];
    if head == 0 {
        head = n as u8;
    }
    head -= 1;
    obj.mem[o + 1] = head;
    let v = tail_pos(ctx, obj, world);
    obj.set_vec3(o + 0x10 + 0x10 * head as usize, v.truncate());
    obj.set_f32(o + 0x1c + 0x10 * head as usize, v.w);
    if let Some(shape) = ctx.dpd.shape(pw(p, e, 4)) {
        let mut a = anim_at(obj, o + 2);
        a.step(shape, ph(p, e, 8));
        set_anim(obj, o + 2, a);
    }
}

pub fn tail_draw(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let Some(shape) = ctx.dpd.shape(pw(p, e, 4)) else { return };
    let o = work(p, 0);
    let a = anim_at(obj, o + 2);
    let Some(frame) = shape.frames.get(a.shown as usize).cloned() else { return };
    let obj_alpha = packed_color(obj, work(p, 1)).w;
    let count = u16_at(&p.params, e + 0x20) as i32;
    let steps = (count - 1).max(1) as f32;
    let c0 = Vec4::new(pb(p, e, 0x14) as f32, pb(p, e, 0x15) as f32, pb(p, e, 0x16) as f32, pb(p, e, 0x17) as f32 * obj_alpha);
    let c1 = Vec4::new(pb(p, e, 0x18) as f32, pb(p, e, 0x19) as f32, pb(p, e, 0x1a) as f32, pb(p, e, 0x1b) as f32 * obj_alpha);
    let dc = (c0 - c1) / steps;
    let (mut size, size_end, spacing0) = (pf(p, e, 0xc), pf(p, e, 0x10), pf(p, e, 0x1c));
    let dsize = (size - size_end) / steps;
    let dspace = if size != 0.0 { spacing0 * (dsize / size) } else { 0.0 };
    let n = obj.mem[o].max(1) as usize;
    let head = obj.mem[o + 1] as usize % n;
    let ring = |k: usize| obj.vec3(o + 0x10 + 0x10 * k);
    let world = pb(p, e, 0x26) != 0;
    let local = obj.mat(OBJ_LOCAL);
    let s = ctx.env.bind_scale;
    let to_cam = if world { ctx.env.view } else { ctx.env.view * ctx.env.bind };
    let mut color = c0;
    let mut spacing = spacing0;
    let mut remaining = if spacing0 == 0.0 { 1 } else { count };
    let mut pos = ring(head);
    let mut seg_from = ring(head);
    let mut idx = (head + 1) % n;
    let mut seg_to = ring(idx);
    let mut seg_len = (seg_to - seg_from).length();
    let mut along = 0.0f32;
    loop {
        let cam = Mat4::from_cols(local.x_axis * (size * s.x), local.y_axis * (size * s.y), local.z_axis, to_cam * pos.extend(1.0));
        draw_frame(ctx, &frame, ctx.env.view_inv * cam, color / 128.0);
        remaining -= 1;
        if remaining <= 0 {
            break;
        }
        color -= dc;
        spacing -= dspace;
        size -= dsize;
        if spacing <= 0.0 {
            break;
        }
        // Walk `spacing` further along the ring of past positions.
        along += spacing;
        while along > seg_len {
            along -= seg_len;
            idx = (idx + 1) % n;
            if idx == head {
                return;
            }
            seg_from = seg_to;
            seg_to = ring(idx);
            seg_len = (seg_to - seg_from).length();
        }
        pos = if seg_len > 0.0 { seg_from + (seg_to - seg_from) * (along / seg_len) } else { seg_from };
    }
}

// ---- pppRyjMegaBirth: particle emitter -----------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct Mega {
    pub particles: Vec<Particle>,
    pub emitter: Mat4,
    pub gravity: Vec3,
    pub timer: u16,
    pub vertex_cursor: u16,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Particle {
    pub pos: Vec3,
    pub dir: Vec3,
    pub anim: Anim,
    /// Ticks left; 0 = free slot; 0xffff = endless.
    pub life: u16,
    pub color_off: [i8; 4],
    pub age: u8,
    pub angle: i16,
    pub ang_vel: i16,
    pub ang_extra: i16,
    pub scale: [f32; 2],
    pub scale_vel: [f32; 2],
    pub scale_acc: [f32; 2],
    pub speed: f32,
    pub fall: f32,
    pub fade: f32,
    pub size: f32,
    pub fade_in: u8,
    pub fade_out: u8,
    pub grow_in: u8,
    pub shrink_out: u8,
    /// Birth matrix (coordinate modes 1-3).
    pub birth: Mat4,
    /// Per-particle color drift (rgba, rgba rate).
    pub color: [f32; 8],
}

pub fn mega_init(obj: &mut Obj, p: &Program) {
    let _ = p;
    obj.heap.retain(|_| true);
}

pub fn mega_destroy(obj: &mut Obj, _: &Program) {
    obj.heap.clear();
}

fn deg_to_turn(d: f32) -> f32 {
    d * 32768.0 / 180.0
}

pub fn mega_frame(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let pi = ctx.prog_index;
    if heap(obj, pi).is_none() {
        let max = u16_at(&p.params, e + 0x1c) as usize;
        let g = Vec3::new(pf(p, e, 0x98), pf(p, e, 0x9c), pf(p, e, 0xa0)).normalize_or_zero();
        obj.heap.push((pi, Heap::Mega(Mega { particles: vec![Particle::default(); max], gravity: g, ..Default::default() })));
    }
    // Emitter: the bind without rotation for odd emitter shapes, the full bind otherwise.
    let env = ctx.env;
    let emitter = if pb(p, e, 0x26) % 2 == 1 && pb(p, e, 0x26) <= 9 {
        Mat4::from_cols(
            Vec4::X * env.bind_scale.x,
            Vec4::Y * env.bind_scale.y,
            Vec4::Z * env.bind_scale.z,
            env.bind.w_axis,
        )
    } else {
        env.bind
    };
    let local = obj.mat(OBJ_LOCAL);
    let alpha = obj.mem[work(p, 1) + 0xb];
    let Some(Heap::Mega(mut mb)) = heap(obj, pi).cloned() else { return };
    mb.emitter = emitter;
    if ctx.running && pw(p, e, 4) != 0xffff {
        mb.timer = mb.timer.wrapping_add(1);
        let mut born = 0;
        let shape = ctx.dpd.shape(pw(p, e, 4)).cloned();
        for i in 0..mb.particles.len() {
            if mb.particles[i].life == 0 {
                if u16_at(&p.params, e + 0x20) <= mb.timer && born < u16_at(&p.params, e + 0x1e) {
                    born += 1;
                    let pt = birth(ctx, p, e, &mut mb, emitter, local, alpha);
                    mb.particles[i] = pt;
                }
            } else {
                let mut pt = mb.particles[i];
                update(p, e, &mut pt, mb.gravity, alpha);
                if let Some(s) = &shape {
                    pt.anim.step(s, ph(p, e, 8));
                }
                mb.particles[i] = pt;
            }
        }
        if born > 0 {
            mb.timer = 0;
        }
    }
    if let Some(h) = heap(obj, pi) {
        *h = Heap::Mega(mb);
    }
}

fn birth(ctx: &mut Ctx, p: &Program, e: usize, mb: &mut Mega, emitter: Mat4, local: Mat4, alpha: u8) -> Particle {
    let mut pt = Particle { size: 1.0, ..Default::default() };
    let kind = pb(p, e, 0x26);
    let spread = pb(p, e, 0x27) as f32;
    let axis = Vec3::new(pf(p, e, 0xbc), pf(p, e, 0xc0), pf(p, e, 0xc4));
    let radius = pf(p, e, 0xb8);
    if kind < 8 {
        let mut a = [0f32; 3];
        for x in a.iter_mut() {
            *x = ((2.0 * spread * ctx.rand() - spread) * std::f32::consts::PI) / 180.0;
        }
        if kind == 2 || kind == 3 {
            a[0] = 0.0;
            a[1] = 0.0;
        }
        let r = Mat3::from_rotation_x(a[0]) * Mat3::from_rotation_y(a[1]) * Mat3::from_rotation_z(a[2]);
        let base = Vec3::new(pf(p, e, 0x8c), pf(p, e, 0x90), pf(p, e, 0x94));
        pt.dir = (r * base * axis).normalize_or_zero();
    }
    match kind {
        4 | 5 => {
            if radius != 0.0 {
                let h = radius * 0.5;
                let mut v = [0f32; 3];
                for x in v.iter_mut() {
                    *x = match pb(p, e, 0xc8) {
                        2 => radius * ctx.rand() * ctx.rand() - h,
                        3 => {
                            let (a, b) = (ctx.rand(), ctx.rand());
                            radius - radius * a * b * 0.7 - h
                        }
                        4 => radius * ctx.rand() * ctx.rand() * ctx.rand() * ctx.rand() - h,
                        5 => {
                            let (a, b, c) = (ctx.rand(), ctx.rand(), ctx.rand());
                            radius - radius * a * b * c * 0.5 - h
                        }
                        1 => {
                            ctx.rand();
                            radius * ctx.rand() - h
                        }
                        _ => radius * ctx.rand() - h,
                    };
                }
                pt.pos = Vec3::from(v) * axis;
            }
        }
        6..=9 => {
            let vsf = ph(p, e, 0xd0);
            if vsf >= 0 {
                if let Some((verts, idx)) = vsf_vertices(ctx, vsf) {
                    let (verts, idx) = (verts.to_vec(), idx.to_vec());
                    let n = idx.len().max(1);
                    let k = if pb(p, e, 0xc8) == 0 {
                        let mut c = mb.vertex_cursor as usize;
                        if c >= n {
                            c = 0;
                        }
                        mb.vertex_cursor = (c + 1) as u16;
                        c
                    } else {
                        let f = ctx.rand() * ctx.rand();
                        mb.vertex_cursor = mb.vertex_cursor.min(n as u16);
                        ((f * n as f32) as usize).min(n - 1)
                    };
                    let v = verts.get(idx.get(k).copied().unwrap_or(0) as usize).copied().unwrap_or([0; 3]);
                    pt.pos = Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) * axis;
                    if kind >= 8 {
                        pt.dir = pt.pos.normalize_or_zero();
                    }
                }
            }
        }
        _ => {
            if radius != 0.0 {
                let r = match pb(p, e, 0xc8) {
                    1 => {
                        ctx.rand();
                        radius * ctx.rand()
                    }
                    2 => radius * ctx.rand() * ctx.rand(),
                    3 => {
                        let (a, b) = (ctx.rand(), ctx.rand());
                        radius - radius * a * b * 0.7
                    }
                    4 => {
                        let x = radius * ctx.rand() * ctx.rand();
                        x * ctx.rand() * ctx.rand()
                    }
                    5 => {
                        let (a, b, c) = (ctx.rand(), ctx.rand(), ctx.rand());
                        radius - radius * a * b * c * 0.5
                    }
                    _ => radius,
                };
                pt.pos = pt.dir * r;
            }
        }
    }
    let rb = |ctx: &mut Ctx, range: u8| -> i8 {
        let r = ctx.rand();
        ((((range as u32) << 1) as f32 * r - (range >> 1) as f32) as i32) as i8
    };
    pt.color_off = [rb(ctx, pb(p, e, 0x48)), rb(ctx, pb(p, e, 0x49)), rb(ctx, pb(p, e, 0x4a)), rb(ctx, pb(p, e, 0x4b))];
    if pb(p, e, 0x24) != 0 {
        pt.fade = alpha as f32;
        pt.fade_in = pb(p, e, 0x24);
    }
    pt.fade_out = pb(p, e, 0x25);
    if pb(p, e, 0xd3) != 0 {
        pt.size = 0.0;
        pt.grow_in = pb(p, e, 0xd3);
    }
    pt.shrink_out = pb(p, e, 0xd4);
    pt.angle = deg_to_turn(pf(p, e, 0x7c)) as i32 as i16;
    pt.ang_vel = deg_to_turn(pf(p, e, 0x80)) as i32 as i16;
    let af = pb(p, e, 0xcb);
    if af != 0 {
        let r = ctx.rand();
        pt.ang_extra = deg_to_turn(r * pf(p, e, 0x88)) as i32 as i16;
        if af & 3 == 3 {
            if ctx.rand() > 0.5 {
                pt.ang_extra = pt.ang_extra.wrapping_neg();
            }
        } else if af & 2 != 0 {
            pt.ang_extra = pt.ang_extra.wrapping_neg();
        }
        if af & 4 != 0 {
            pt.angle = pt.angle.wrapping_add(pt.ang_extra);
        }
        if af & 8 != 0 {
            pt.ang_vel = pt.ang_vel.wrapping_add(pt.ang_extra);
        }
    }
    pt.scale = [pf(p, e, 0x4c), pf(p, e, 0x50)];
    pt.scale_vel = [pf(p, e, 0x58), pf(p, e, 0x5c)];
    let sf = pb(p, e, 0xca);
    if sf != 0 {
        if sf & 0x20 == 0 {
            pt.scale_acc = [ctx.rand() * pf(p, e, 0x70), ctx.rand() * pf(p, e, 0x74)];
            if sf & 3 == 3 {
                if ctx.rand() > 0.5 {
                    pt.scale_acc[0] = -pt.scale_acc[0];
                }
                if ctx.rand() > 0.5 {
                    pt.scale_acc[1] = -pt.scale_acc[1];
                }
            } else if sf & 2 != 0 {
                pt.scale_acc = [-pt.scale_acc[0], -pt.scale_acc[1]];
            }
        } else {
            let r = ctx.rand() * pf(p, e, 0x70);
            pt.scale_acc = [r, r];
            if sf & 3 == 3 {
                if ctx.rand() > 0.5 {
                    pt.scale_acc = [-r, -r];
                }
            } else if sf & 2 != 0 {
                pt.scale_acc = [-r, -r];
            }
        }
        if sf & 4 != 0 {
            pt.scale[0] += pt.scale_acc[0];
            pt.scale[1] += pt.scale_acc[1];
        }
        if sf & 8 != 0 {
            pt.scale_vel[0] += pt.scale_acc[0];
            pt.scale_vel[1] += pt.scale_acc[1];
        }
    }
    match pb(p, e, 0xd2) {
        1 if ctx.rand() > 0.5 => pt.scale[0] = -pt.scale[0],
        2 if ctx.rand() > 0.5 => pt.scale[1] = -pt.scale[1],
        3 => {
            if ctx.rand() > 0.5 {
                pt.scale[0] = -pt.scale[0];
            }
            if ctx.rand() > 0.5 {
                pt.scale[1] = -pt.scale[1];
            }
        }
        _ => {}
    }
    pt.speed = pf(p, e, 0xa4);
    pt.fall = pf(p, e, 0xb0);
    let sr = pf(p, e, 0xac);
    if sr != 0.0 {
        pt.speed += 2.0 * sr * ctx.rand() - sr;
    }
    let fr = pf(p, e, 0xd8);
    if fr != 0.0 {
        pt.fall += 2.0 * fr * ctx.rand() - fr;
    }
    let life = ph(p, e, 0x22);
    pt.life = if life == 0 { 0xffff } else { life as u16 };
    pt.birth = match pb(p, e, 0xcc) {
        1 => emitter,
        2 => local,
        3 => emitter * local,
        _ => Mat4::IDENTITY,
    };
    if pb(p, e, 0xc9) != 0 {
        pt.color = [0.0, 0.0, 0.0, 0.0, pf(p, e, 0x28), pf(p, e, 0x2c), pf(p, e, 0x30), pf(p, e, 0x34)];
    }
    pt
}

fn update(p: &Program, e: usize, pt: &mut Particle, gravity: Vec3, alpha: u8) {
    let mut base_alpha = alpha as i32;
    if pb(p, e, 0xc9) != 0 {
        let c = &mut pt.color;
        let a = c[3] + c[7];
        c[0] += c[4];
        c[1] += c[5];
        c[2] += c[6];
        c[3] = a;
        c[4] += pf(p, e, 0x38);
        c[5] += pf(p, e, 0x3c);
        c[6] += pf(p, e, 0x40);
        c[7] += pf(p, e, 0x44);
        base_alpha = (alpha as i32 + a as i32).min(255);
    }
    pt.angle = pt.angle.wrapping_add(pt.ang_vel);
    let extra = if pb(p, e, 0xcb) & 0x10 != 0 { pt.ang_extra as f32 } else { 0.0 };
    pt.ang_vel = (pt.ang_vel as f32 + pf(p, e, 0x84) + extra) as i32 as i16;
    pt.scale[0] += pt.scale_vel[0];
    pt.scale[1] += pt.scale_vel[1];
    let (ax, ay) = if pb(p, e, 0xca) & 0x10 != 0 { (pt.scale_acc[0], pt.scale_acc[1]) } else { (0.0, 0.0) };
    pt.scale_vel[0] += pf(p, e, 0x64) + ax;
    pt.scale_vel[1] += pf(p, e, 0x68) + ay;
    pt.speed += pf(p, e, 0xa8);
    if pb(p, e, 0xce) == 0 {
        let (v0, acc) = (pf(p, e, 0xa4), pf(p, e, 0xa8));
        if (v0 > 0.0 && acc < 0.0 && pt.speed < 0.0) || (v0 < 0.0 && acc > 0.0 && pt.speed > 0.0) {
            pt.speed = 0.0;
        }
    }
    pt.fall += pf(p, e, 0xb4);
    pt.pos += pt.dir * pt.speed + gravity * pt.fall;
    if ph(p, e, 0x22) != 0 {
        pt.life = pt.life.wrapping_sub(1);
    }
    pt.age = pt.age.wrapping_add(1);
    if pt.fade_in != 0 && pt.age <= pt.fade_in {
        pt.fade -= base_alpha as f32 / pt.fade_in as f32;
    }
    if pt.fade_out != 0 && pt.life <= pt.fade_out as u16 {
        pt.fade += base_alpha as f32 / pb(p, e, 0x25).max(1) as f32;
    }
    if pt.grow_in != 0 && pt.age <= pt.grow_in {
        pt.size = pt.age as f32 / pt.grow_in as f32;
    }
    if pt.shrink_out != 0 && pt.life <= pt.shrink_out as u16 {
        pt.size = pt.life as f32 / pt.shrink_out as f32;
    }
}

pub fn mega_draw(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let pi = ctx.prog_index;
    let Some(Heap::Mega(mb)) = heap(obj, pi).cloned() else { return };
    let Some(shape) = ctx.dpd.shape(pw(p, e, 4)).cloned() else { return };
    let env = ctx.env.clone();
    let local = obj.mat(OBJ_LOCAL);
    let co = work(p, 1) + 8;
    let base = [obj.mem[co] as i32, obj.mem[co + 1] as i32, obj.mem[co + 2] as i32, obj.mem[co + 3] as i32];
    let mode = pb(p, e, 0xcc);
    let mut order: Vec<usize> = (0..mb.particles.len()).filter(|&i| mb.particles[i].life != 0).collect();
    let anchor = |pt: &Particle| -> Vec4 {
        let pos = pt.pos.extend(1.0);
        match mode {
            1 => env.view * pt.birth * local * pos,
            2 => env.view * mb.emitter * pt.birth * pos,
            3 => env.view * pt.birth * pos,
            _ => env.view * mb.emitter * local * pos,
        }
    };
    if pb(p, e, 0xf) == 0 {
        // Back to front in camera depth (game camera looks down +Z).
        order.sort_by(|&a, &b| anchor(&mb.particles[b]).z.total_cmp(&anchor(&mb.particles[a]).z));
    }
    for i in order {
        let pt = mb.particles[i];
        let Some(frame) = shape.frames.get(pt.anim.shown as usize) else { continue };
        let mut sx = pt.scale[0] * env.bind_scale.x;
        let mut sy = pt.scale[1] * env.bind_scale.y;
        if pt.size < 1.0 {
            sx *= pt.size;
            sy *= pt.size;
        }
        let mut m = Mat4::from_cols(Vec4::X * sx, Vec4::Y * sy, Vec4::Z * sx, Vec4::W);
        if pt.angle != 0 {
            let (s, c) = (sin_turn(pt.angle as i32), crate::prog::cos_turn(pt.angle as i32));
            let r = Mat4::from_cols(Vec4::new(c, s, 0.0, 0.0), Vec4::new(-s, c, 0.0, 0.0), Vec4::Z, Vec4::W);
            m *= r;
        }
        m.w_axis = anchor(&pt);
        let col = |k: usize, max: i32| (base[k] + pt.color_off[k] as i32 + pt.color[k] as i32).clamp(0, max);
        let a = ((base[3] as f32 + pt.color_off[3] as f32 - pt.fade) as i32 + pt.color[3] as i32).clamp(0, 0x7f);
        let mat = Vec4::new(col(0, 255) as f32, col(1, 255) as f32, col(2, 255) as f32, a as f32) / 128.0;
        draw_frame(ctx, frame, env.view_inv * m, mat);
    }
    let _ = OBJ_FRAME;
}

/// pppKeBornRnd5: every `interval` ticks, `count` children of `group` at VSF vertices (in order or
/// at random), each with position = local × vertex and velocity = speed × direction from the origin.
pub fn born_rnd5_init(obj: &mut Obj, p: &Program) {
    let o = work(p, 0);
    obj.mem[o + 2] = 0;
    obj.set_i16(o, 0);
}

pub fn born_rnd5(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running || ctx.fast_forward || pb(p, e, 0xd) == 0 {
        return;
    }
    let o = work(p, 0);
    let mut left = obj.mem[o + 2];
    if left == 0 {
        left = pb(p, e, 0xd);
        obj.mem[o + 2] = left;
        let group = pw(p, e, 0x10);
        if group == -1 || pw(p, e, 0x18) == -1 {
            return;
        }
        let Some((verts, idx)) = vsf_vertices(ctx, u16_at(&p.params, e + 4) as i16) else { return };
        let (verts, idx) = (verts.to_vec(), idx.to_vec());
        let n = idx.len().max(1);
        let random = pb(p, e, 0xe) != 0;
        let mut k = if random { ((ctx.rand() * n as f32) as usize).min(n - 1) } else { obj.i16(o) as u16 as usize % n };
        let local = obj.mat(OBJ_LOCAL);
        for _ in 0..pb(p, e, 0xc) {
            let v = verts.get(idx.get(k).copied().unwrap_or(0) as usize).copied().unwrap_or([0; 3]);
            let pos = local.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32));
            let vel = (pos - local.w_axis.truncate()).normalize_or_zero() * pf(p, e, 8);
            ctx.spawns.push(Spawn { group: group as usize, parent: ctx.obj_id, init: SpawnInit::Vec3(pw(p, e, 0x14) as usize, pos) });
            ctx.spawns.push(Spawn { group: usize::MAX, parent: ctx.obj_id, init: SpawnInit::Vec3(pw(p, e, 0x1c) as usize, vel) });
            k = if random { ((ctx.rand() * n as f32) as usize).min(n - 1) } else { (k + 1) % n };
        }
        obj.set_i16(o, k as i16);
    }
    obj.mem[o + 2] = obj.mem[o + 2].wrapping_sub(1);
}
