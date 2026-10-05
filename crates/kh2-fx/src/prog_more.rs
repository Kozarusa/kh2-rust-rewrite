//! Three more programs, read from the game's own handlers (a local disassembly of the user's
//! executable; nothing copied). The decompiler drops the broadcast fields of the VU0 macro
//! instructions these handlers use, so the draw handler of `pppKeShpTail3X` and the model program
//! `pppSnoMdlDttNorm` were read from the disassembly instead.
//!
//! * `pppKeShpTail3X` (frame 0x1fcf70, draw 0x1fd388, init 0x1fdb90, reset 0x1fdbf0): a tail of
//!   randomly rotated sprites strung along the recent path, with colors animated by eight 16-bit
//!   channels.
//! * `pppKeMvYpEff` (frame 0x1f4628): turns the object's angles to face its own velocity.
//! * `pppSnoMdlDttNorm` (draw 0x20a7e8, init 0x20a7b8, reset 0x20a7d0): draws a model that shows
//!   the screen shifted along the model's normals (a distortion lens). The animated amplitude and
//!   the geometry are ported; see [`Batch::offsets`] for how the draw is handed over.
//!
//! Work-memory offsets in this file are relative to each program's own work area (the object's
//! `0xa0 + useVal`), like the offsets the game's handlers add to their `+0xa0` base.

use glam::{Mat4, Vec3, Vec4};

use crate::pax::Program;
use crate::prog::{cos_turn, sin_turn};
use crate::progx::{draw_frame, packed_color, pb, pf, pw, tail_pos, work};
use crate::rd::*;
use crate::vm::{Batch, Ctx, OBJ_LOCAL, Obj, Vertex};

// ---- pppKeShpTail3X -------------------------------------------------------------------------------

/// Samples in the ring of past positions (the game's fixed 0x1c).
const T3_RING: usize = 0x1c;
/// Work layout: eight 16-bit channels at +0x00, their rates at +0x10 (channels 0-3) and +0x20
/// (4-7), the rates of the rates at +0x18 and +0x28; the ring (0x10 bytes per sample) at +0x30.
const T3_RING_AT: usize = 0x30;
/// Time accumulator (u32), seed (u16) and ring head (u8).
const T3_ACC: usize = 0x1f0;
const T3_SEED: usize = 0x1f8;
const T3_HEAD: usize = 0x1fa;

fn t3_rate(k: usize) -> usize {
    if k < 4 { 0x10 + 2 * k } else { 0x20 + 2 * (k - 4) }
}

fn t3_clear(obj: &mut Obj, o: usize) {
    obj.mem[o..o + 0x30].fill(0);
}

pub fn tail3_init(obj: &mut Obj, p: &Program) {
    let o = work(p, 0);
    obj.mem[o + T3_HEAD] = 0;
    obj.set_i16(o + 0x1f4, 0);
    obj.set_i32(o + T3_ACC, 0);
    // The game seeds the per-instance random rotation from its global rand().
    obj.set_i16(o + T3_SEED, obj.seed as i16);
    t3_clear(obj, o);
}

/// A loop restart clears the channels only (not the ring, the seed or the accumulator).
pub fn tail3_reset(obj: &mut Obj, p: &Program) {
    t3_clear(obj, work(p, 0));
}

pub fn tail3_frame(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    if !ctx.running {
        return;
    }
    let o = work(p, 0);
    // Byte +0x53: positions in the world (bind applied when sampled) instead of bind-relative.
    let world = pb(p, e, 0x53) != 0;
    let v = tail_pos(ctx, obj, world);
    if obj.frame() == 0 && !obj.restart {
        for k in 0..T3_RING {
            obj.set_vec3(o + T3_RING_AT + 0x10 * k, v.truncate());
            obj.set_f32(o + T3_RING_AT + 0x10 * k + 0xc, v.w);
        }
    }
    let mut head = obj.mem[o + T3_HEAD];
    if head == 0 {
        head = T3_RING as u8;
    }
    head -= 1;
    obj.mem[o + T3_HEAD] = head;
    let at = o + T3_RING_AT + 0x10 * head as usize;
    obj.set_vec3(at, v.truncate());
    obj.set_f32(at + 0xc, v.w);
    // Rate += rate of rate, value += rate, for all eight channels.
    for k in 0..8 {
        let r = obj.i16(o + t3_rate(k)).wrapping_add(obj.i16(o + t3_rate(k) + 8));
        obj.set_i16(o + t3_rate(k), r);
        let val = obj.i16(o + 2 * k).wrapping_add(r);
        obj.set_i16(o + 2 * k, val);
    }
    // On the entry's frame its 0x30 bytes of deltas (same layout as the work area) are added.
    if p.frame_at(e) == obj.frame() {
        for i in 0..0x18 {
            let d = i16_at(&p.params, e + 0x20 + 2 * i);
            let v = obj.i16(o + 2 * i).wrapping_add(d);
            obj.set_i16(o + 2 * i, v);
        }
    }
}

/// Wrap-around successor in the ring.
fn t3_next(k: usize) -> usize {
    if k == T3_RING - 1 { 0 } else { k + 1 }
}

pub fn tail3_draw(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let shape_no = pw(p, e, 4);
    if shape_no == 0xffff {
        return;
    }
    let dpd = ctx.dpd;
    let Some(shape) = dpd.shape(shape_no) else { return };
    let o = work(p, 0);
    // Start and end colors (channels 0-3 and 4-7, 7 fractional bits dropped); the alpha channels
    // are scaled by the object's color alpha.
    let alpha = packed_color(obj, work(p, 1)).w;
    let ch = |obj: &Obj, k: usize| (obj.i16(o + 2 * k) >> 7) as f32;
    let mut color = Vec4::new(ch(obj, 0), ch(obj, 1), ch(obj, 2), ch(obj, 3) * alpha);
    let end = Vec4::new(ch(obj, 4), ch(obj, 5), ch(obj, 6), ch(obj, 7) * alpha);
    // Instances drawn (byte +0x1c), size and spacing stepping along the tail.
    let mut counter = pb(p, e, 0x1c) as i32 - 1;
    let steps = counter as f32;
    let dcol = if counter != 0 { (color - end) * (1.0 / steps) } else { Vec4::ZERO };
    let (mut size, size_end, rand_size, mut spacing) = (pf(p, e, 0xc), pf(p, e, 0x10), pf(p, e, 0x14), pf(p, e, 0x18));
    let dsize = if counter != 0 { (size - size_end) / steps } else { 0.0 };
    let dspace = if size != 0.0 { spacing * (dsize / size) } else { 0.0 };
    if spacing == 0.0 {
        counter = -1;
    }
    let acc = obj.i32(o + T3_ACC) as u32;
    let mut seed = u16_at(&obj.mem, o + T3_SEED) as u32;
    let world = pb(p, e, 0x53) != 0;
    let local = obj.mat(OBJ_LOCAL);
    let (s, view_inv) = (ctx.env.bind_scale, ctx.env.view_inv);
    let to_cam = if world { ctx.env.view } else { ctx.env.view * ctx.env.bind };
    let enabled = pb(p, e, 0x1d) != 0;
    let rotate = u16_at(&p.params, e + 0x1e) != 0;
    let wait0 = shape.frames.first().map_or(0, |f| f.wait as i32 as u32);
    let seq = shape.seq_count as u32;

    // Walk the ring from the newest sample backwards: `seg_*` is the segment the next instance
    // lies on, `along` the distance to it from the segment start and `left` the length not yet used.
    let ring = |obj: &Obj, k: usize| obj.vec3(o + T3_RING_AT + 0x10 * k);
    let start = obj.mem[o + T3_HEAD] as usize % T3_RING;
    let mut cursor = t3_next(start);
    let mut seg_start = ring(obj, start);
    let mut seg_end = ring(obj, cursor);
    let mut seg_vec = seg_end - seg_start;
    let mut seg_len = seg_vec.length();
    let mut left = seg_len;
    let mut along = spacing;
    let mut pos = seg_start;
    for _ in 0..4096 {
        if enabled {
            seed = (seed * 0x80d + 7) & 0xffff;
            // Frame from the time accumulator and the random value, size shrunk by the random value.
            let k = if wait0 == 0 || seq == 0 { 0 } else { ((acc.wrapping_add(seed) / wait0) % seq) as usize };
            if let Some(frame) = shape.frames.get(k) {
                let byte = |c: f32| (c as i32) as u8 as f32;
                let col = Vec4::new(byte(color.x), byte(color.y), byte(color.z), byte(color.w)) / 128.0;
                let sz = size * (1.0 - seed as f32 / 65535.0 * rand_size);
                let mut right = local.x_axis.truncate() * (s.x * sz);
                let mut up = local.y_axis.truncate() * (s.y * sz);
                if rotate {
                    // A random turn about the view axis from the same random value.
                    let (sn, cs) = (sin_turn(seed as i32), cos_turn(seed as i32));
                    (right, up) = (right * cs + up * sn, up * cs - right * sn);
                }
                let centre = to_cam * pos.extend(1.0);
                let cam = Mat4::from_cols(right.extend(0.0), up.extend(0.0), Vec4::ZERO, centre);
                draw_frame(ctx, frame, view_inv * cam, col);
            }
        }
        if counter == 0 {
            break;
        }
        color -= dcol;
        spacing -= dspace;
        size -= dsize;
        if spacing <= 0.0 {
            break;
        }
        // Find the segment the next instance falls on.
        let mut placed = false;
        loop {
            if spacing <= left {
                let frac = if seg_len != 0.0 { along / seg_len } else { 0.0 };
                pos = seg_start + seg_vec * frac;
                along += spacing;
                left -= spacing;
                counter -= 1;
                placed = true;
                break;
            }
            cursor = t3_next(cursor);
            if cursor == start {
                break;
            }
            seg_start = seg_end;
            seg_end = ring(obj, cursor);
            seg_vec = seg_end - seg_start;
            along -= seg_len;
            seg_len = seg_vec.length();
            left += seg_len;
        }
        if !placed {
            break;
        }
    }
    if ctx.running {
        obj.set_i32(o + T3_ACC, (acc as i32).wrapping_add(pw(p, e, 8)));
    }
}

// ---- pppKeMvYpEff ---------------------------------------------------------------------------------

/// The game's atan2 (handler 0x1f44d0): quadrant logic around a double precision `atan`, and no
/// result for (0, 0).
fn game_atan2(y: f32, x: f32) -> Option<f32> {
    use std::f64::consts::{FRAC_PI_2, PI};
    let atan = |q: f32| (q as f64).atan();
    let r = if x * y > 0.0 {
        let base = if x > 0.0 { 0.0 } else { -PI };
        base + atan(y / x)
    } else if x * y < 0.0 {
        let base = if y > 0.0 { FRAC_PI_2 } else { -FRAC_PI_2 };
        base - atan(x / y)
    } else if x != 0.0 {
        if x > 0.0 { 0.0 } else { PI }
    } else if y != 0.0 {
        if y > 0.0 { FRAC_PI_2 } else { -FRAC_PI_2 }
    } else {
        return None;
    };
    Some(r as f32)
}

/// Radians to the game's integer angle (one turn = 0x10000), truncated.
fn to_turn(rad: f32) -> i32 {
    ((rad * 32768.0) as f64 / std::f64::consts::PI) as i32
}

/// pppKeMvYpEff: work 1 holds a velocity (as a Move program leaves it); work 0 is the object's
/// angle triple. Z gets the heading `atan2(vx, -vy)` and X the pitch `atan2(-vz, hypot(vx, vy))`;
/// the Y angle is not touched. Nothing happens while the velocity is zero.
pub fn mv_yp_eff(ctx: &mut Ctx, obj: &mut Obj, p: &Program, _: usize) {
    if !ctx.running {
        return;
    }
    let (angles, velocity) = (work(p, 0), work(p, 1));
    let v = obj.vec3(velocity);
    if v.x == 0.0 && v.y == 0.0 && v.z == 0.0 {
        return;
    }
    if let Some(a) = game_atan2(v.x, -v.y) {
        obj.set_i32(angles + 8, to_turn(a));
    }
    let flat = (v.y * v.y + v.x * v.x).sqrt();
    if let Some(a) = game_atan2(-v.z, flat) {
        obj.set_i32(angles, to_turn(a));
    }
}

// ---- pppSnoMdlDttNorm -----------------------------------------------------------------------------

/// Work 0: the amplitude (value, rate, acceleration as floats; init and reset zero it); work 1: the
/// color bytes a color program packed; work 2: the model's matrix (copied there by a draw-matrix
/// program, like `pppKeDMatFr`).
pub fn sno_init(obj: &mut Obj, p: &Program) {
    let o = work(p, 0);
    obj.mem[o..o + 12].fill(0);
}

pub fn sno_draw(ctx: &mut Ctx, obj: &mut Obj, p: &Program, e: usize) {
    let (w0, w1, w2) = (work(p, 0), work(p, 1), work(p, 2));
    if ctx.running {
        // value += rate += acceleration; on the entry's frame its three floats add to all three.
        let rate = obj.f32(w0 + 4) + obj.f32(w0 + 8);
        let value = obj.f32(w0) + rate;
        obj.set_f32(w0 + 4, rate);
        obj.set_f32(w0, value);
        if p.frame_at(e) == obj.frame() {
            obj.set_f32(w0, value + pf(p, e, 8));
            obj.set_f32(w0 + 4, rate + pf(p, e, 0xc));
            let acc = obj.f32(w0 + 8) + pf(p, e, 0x10);
            obj.set_f32(w0 + 8, acc);
        }
    }
    let number = pw(p, e, 4);
    if number == 0xffff || obj.mem[w1 + 0xb] == 0 {
        return;
    }
    let dpd = ctx.dpd;
    let Some(model) = dpd.model(number) else { return };
    let tint = packed_color(obj, w1);
    // The displacement along each normal: the amplitude, optionally times the bind's scale (byte
    // +0x16), and with byte +0x15 tapered by the normal's view-space z.
    let amp = obj.f32(w0) * if pb(p, e, 0x16) != 0 { ctx.env.bind_scale.x } else { 1.0 };
    let taper = pb(p, e, 0x15) != 0;
    // Models without flag 2 use the direction from the model's origin to the vertex as the normal.
    let radial = model.flags & 2 == 0;
    // The game keeps the model matrix in camera space.
    let m_cam = ctx.env.view * obj.mat(w2);
    let (view_inv, centre) = (ctx.env.view_inv, m_cam.w_axis.truncate());
    let alpha = match pb(p, e, 0x1c) {
        0 => 0x44,
        a => a,
    };
    let mut batch = Batch { dpd: ctx.dpd_index, tex0: None, alpha, ..Default::default() };
    for blk in model.blocks.iter().filter(|b| b.kind == 1) {
        for pr in &blk.prims {
            let base = batch.verts.len() as u32;
            for k in 0..3 {
                let v = model.vertices.get(pr.tri[k] as usize).copied().unwrap_or([0; 3]);
                let pos_cam = m_cam.transform_point3(Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32));
                let dir = if radial {
                    (pos_cam - centre).normalize_or_zero()
                } else {
                    let n = model.normals.get(pr.tri_normals[k] as usize).copied().unwrap_or([0; 3]);
                    m_cam.transform_vector3(Vec3::new(n[0] as f32, n[1] as f32, n[2] as f32) / 4096.0)
                };
                let mut off = dir;
                if taper {
                    let z = dir.normalize_or_zero().z;
                    off.x *= z;
                    off.y *= z;
                }
                off *= amp;
                let c = pr.colors[k];
                batch.verts.push(Vertex {
                    pos: view_inv.transform_point3(pos_cam),
                    color: Vec4::new(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32) / 128.0 * tint,
                    uv: [0.0; 2],
                });
                batch.offsets.push(view_inv.transform_vector3(off));
            }
            batch.indices.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }
    if !batch.indices.is_empty() {
        // Distortion batches never merge with others.
        ctx.draws.push(batch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atan2_quadrants() {
        let pi = std::f32::consts::PI;
        let near = |a: Option<f32>, b: f32| (a.unwrap() - b).abs() < 1e-5;
        assert!(near(game_atan2(1.0, 1.0), pi / 4.0));
        assert!(near(game_atan2(1.0, -1.0), 3.0 * pi / 4.0));
        assert!(near(game_atan2(-1.0, -1.0), -3.0 * pi / 4.0));
        assert!(near(game_atan2(-1.0, 1.0), -pi / 4.0));
        assert!(near(game_atan2(0.0, -2.0), pi));
        assert!(near(game_atan2(3.0, 0.0), pi / 2.0));
        assert!(near(game_atan2(-3.0, 0.0), -pi / 2.0));
        assert_eq!(game_atan2(0.0, 0.0), None);
    }

    #[test]
    fn turns() {
        assert_eq!(to_turn(std::f32::consts::PI), 32768);
        assert_eq!(to_turn(-std::f32::consts::FRAC_PI_2), -16384);
    }
}
