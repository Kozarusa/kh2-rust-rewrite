//! Control-rig evaluation of interpolated motions.
//!
//! KH2 motions are authored with a control rig: besides the model's bones they carry IK helper
//! joints, constraints (position / orientation / direction / up-vector / scale / limits),
//! expressions and IK chains. Joints are processed in the motion's joint-table order:
//!
//! 1. expressions write local channels of the joint;
//! 2. the joint's constraints are collected (a shared cursor walks the constraint table, which
//!    is stored in evaluation order);
//! 3. an IK chain root (one or two bones) finds its goal: the next active position constraint on
//!    its effector joint;
//! 4. world scale, translation and orientation are resolved and the world matrix is stored.
//!
//! Matrices are glam column-major with column vectors (`parent * local`).
//! Behaviour was reconstructed from observation of the game's evaluator and is checked joint by
//! joint against it (see `kh2 rig`); no game code is reproduced here.

use glam::{Mat4, Vec3, Vec4, Vec4Swizzles};

use crate::motion::{Constraint, FCurve, Interpolated, Limiter};
use kh2_model::Bone;

/// Joint flags (byte 2 of a joint-table entry).
pub mod flag {
    pub const IK_MASK: u8 = 0x03;
    /// World translation is already known (set by an IK chain): keep it.
    pub const TRANSLATION: u8 = 0x04;
    /// Rotation is absolute (not inherited from the parent).
    pub const ROTATION: u8 = 0x08;
    /// Joint was solved by a preceding IK chain.
    pub const CALCULATED: u8 = 0x20;
    /// Write the solved rotation back into the rotation channels.
    pub const EXTRACT_ROTATION: u8 = 0x40;
    /// IK effector taken from the external-effector list.
    pub const EXTERNAL_EFFECTOR: u8 = 0x80;
}

/// Per-instance rig state. Buffers persist between evaluations like the game's work areas.
#[derive(Debug, Clone, Default)]
pub struct RigState {
    pub s: Vec<Vec4>,
    pub r: Vec<Vec4>,
    pub t: Vec<Vec4>,
    /// Local matrix of each joint.
    pub local: Vec<Mat4>,
    /// World matrix without scale (parents for children).
    pub world: Vec<Mat4>,
    /// Final world matrix (axes scaled by world scale): what skinning uses.
    pub fin: Vec<Mat4>,
    pub wscale: Vec<Vec4>,
    pub active: Vec<bool>,
    parent: Vec<i32>,
    length: Vec<f32>,
    /// Joint count this state was set up for.
    joints: usize,
    /// Identity of the motion the channels were set up from (setup reruns when it changes).
    key: usize,
}

fn wrap_pi(mut a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    if a > 0.0 {
        while a > PI {
            a -= TAU;
        }
    } else {
        while a < -PI {
            a += TAU;
        }
    }
    a
}

/// Euler angles (X, then Y, then Z) of a rotation matrix, choosing the solution closest to `prev`.
pub fn extract_euler(m: &Mat4, prev: Vec3) -> Vec3 {
    use std::f32::consts::PI;
    let (m0, m1, m2) = (m.x_axis.x, m.x_axis.y, m.x_axis.z);
    let (m4, m5, m6) = (m.y_axis.x, m.y_axis.y, m.y_axis.z);
    let m10 = m.z_axis.z;
    let s = m2.clamp(-1.0, 1.0);
    let (x, y, z);
    if (m0 * m0 + m1 * m1).sqrt() > 1.907_348_6e-6 {
        z = m1.atan2(m0);
        y = (-s).asin();
        x = m6.atan2(m10);
    } else {
        x = if s > 0.0 { (-m4).atan2(m5) } else { m4.atan2(m5) } * 0.5;
        y = (-s).asin();
        z = if s < 0.0 { -x } else { x };
    }
    let d1 = Vec3::new(wrap_pi(x - prev.x), wrap_pi(y - prev.y), wrap_pi(z - prev.z));
    let d2 = Vec3::new(wrap_pi(x + PI - prev.x), wrap_pi(PI - y - prev.y), wrap_pi(z + PI - prev.z));
    prev + if d1.length_squared() <= d2.length_squared() { d1 } else { d2 }
}

/// Local rotation matrix from Euler angles (X applied first).
pub fn euler(r: Vec3) -> Mat4 {
    Mat4::from_rotation_z(r.z) * Mat4::from_rotation_y(r.y) * Mat4::from_rotation_x(r.x)
}

fn normalize(v: Vec3) -> Vec3 {
    let l = v.length();
    if l > 0.0 { v / l } else { v }
}

/// Rotation whose X axis points along `dir` and whose Y axis is `up` made perpendicular.
fn look_at(dir: Vec3, up: Vec3) -> (Vec3, Vec3, Vec3) {
    let x = normalize(dir);
    let y = normalize(dir.cross(up).cross(dir));
    let z = normalize(dir.cross(y));
    (x, y, z)
}

/// One- or two-bone IK in the parent's space. Returns the first bone's local rotation and, for
/// two bones, the second bone's rotation relative to the first.
#[allow(clippy::too_many_arguments)]
pub fn solve_ik(len1: f32, len2: f32, twist1: f32, twist2: f32, start: Vec3, goal: Vec3, bend: bool, up: Option<Vec3>, two: bool) -> (Mat4, Mat4) {
    let d = goal - start;
    let dist = d.length();
    let x = normalize(d);
    let (y, z) = match up {
        Some(u) => {
            let y = normalize(u - start);
            let z = normalize(x.cross(y));
            (normalize(z.cross(x)), z)
        }
        None => {
            let y = normalize(Vec3::new(-x.y, x.x, 0.0));
            (y, normalize(x.cross(y)))
        }
    };
    let mut frame = Mat4::from_cols(x.extend(0.0), y.extend(0.0), z.extend(0.0), Vec4::W);
    if twist1 != 0.0 {
        frame *= Mat4::from_rotation_x(twist1);
    }
    if !two {
        return (frame, Mat4::IDENTITY);
    }
    let c = ((dist * dist - len1 * len1 - len2 * len2) / (len1 * len2 * 2.0)).clamp(-1.0, 1.0);
    let s = (1.0 - c * c).max(0.0).sqrt();
    let s = if bend { -s } else { s };
    let mut second = Mat4::from_cols(Vec4::new(c, s, 0.0, 0.0), Vec4::new(-s, c, 0.0, 0.0), Vec4::Z, Vec4::W);
    if twist2 != 0.0 {
        second *= Mat4::from_rotation_x(twist2);
    }
    let a = (len1 + len2 * c) * dist;
    let b = -(len2 * s) * dist;
    let r = (a * a + b * b).sqrt();
    let (cp, sp) = (a / r, b / r);
    let first = frame * Mat4::from_cols(Vec4::new(cp, sp, 0.0, 0.0), Vec4::new(-sp, cp, 0.0, 0.0), Vec4::Z, Vec4::W);
    (first, second)
}

/// Applies a limiter to `value` (a position, or Euler angles) relative to `origin`.
pub fn limit(l: &Limiter, origin: Vec4, value: &mut Vec4) -> bool {
    let kind = l.flags & 7;
    let o = if kind == 0 || l.flags & 8 != 0 { Vec4::W } else { origin };
    let v = *value - o;
    let (w, k) = (l.damping_width, l.damping_strength);
    let soft = |e: f32| w * (1.0 - ((k - 1.0) * (e / w)).exp());
    let mut changed = false;
    if kind == 1 {
        let radius = l.min[0];
        let len = v.xyz().length();
        // Without damping the game rescales the offset to its own length, so a hard sphere
        // limit never clamps; only the damped form pulls positions in.
        let target = if k == 0.0 {
            None
        } else {
            let e = len - (radius - w);
            (e > 0.0).then(|| (radius - w) + soft(e))
        };
        if let Some(t) = target {
            let n = normalize(v.xyz());
            *value = o + (n * t).extend(v.w);
            changed = true;
        }
    } else {
        for a in 0..3 {
            let (lo, hi) = (l.flags >> (4 + 2 * a) & 1 != 0, l.flags >> (5 + 2 * a) & 1 != 0);
            if k == 0.0 {
                if lo && v[a] < l.min[a] {
                    value[a] = o[a] + l.min[a];
                    changed = true;
                } else if hi && l.max[a] < v[a] {
                    value[a] = o[a] + l.max[a];
                    changed = true;
                }
            } else if lo && (l.min[a] + w) - v[a] > 0.0 {
                value[a] = o[a] + (l.min[a] + w) + soft((l.min[a] + w) - v[a]);
                changed = true;
            } else if hi && v[a] - (l.max[a] - w) > 0.0 {
                value[a] = o[a] + (l.max[a] - w) + soft(v[a] - (l.max[a] - w));
                changed = true;
            }
        }
    }
    value.w = 1.0;
    changed
}

/// Rigid inverse (transpose of the rotation part).
fn rigid_inverse(m: &Mat4) -> Mat4 {
    let r = glam::Mat3::from_mat4(*m).transpose();
    let t = -(r * m.w_axis.xyz());
    Mat4::from_cols(r.x_axis.extend(0.0), r.y_axis.extend(0.0), r.z_axis.extend(0.0), t.extend(1.0))
}

fn point(m: &Mat4, p: Vec4) -> Vec4 {
    *m * p
}

fn scale_axes(m: &Mat4, s: Vec4) -> Mat4 {
    Mat4::from_cols(m.x_axis * s.x, m.y_axis * s.y, m.z_axis * s.z, m.w_axis)
}

impl RigState {
    fn ensure(&mut self, ip: &Interpolated, bones: &[Bone]) {
        let n = ip.total_count.max(bones.len());
        if self.joints == n && self.parent.len() == n {
            return;
        }
        *self = RigState { joints: n, ..Default::default() };
        self.s = vec![Vec4::ZERO; n];
        self.r = vec![Vec4::ZERO; n];
        self.t = vec![Vec4::ZERO; n];
        self.local = vec![Mat4::ZERO; n];
        self.world = vec![Mat4::ZERO; n];
        self.fin = vec![Mat4::ZERO; n];
        self.wscale = vec![Vec4::ZERO; n];
        self.parent = (0..n)
            .map(|j| if j < ip.bone_count { bones.get(j).map_or(-1, |b| b.parent) } else { ip.helper_parent[j - ip.bone_count] })
            .collect();
        self.length = (0..n)
            .map(|j| if j < ip.bone_count { bones.get(j).map_or(0.0, |b| b.length) } else { ip.helpers[j - ip.bone_count].length })
            .collect();
    }

    /// Motion start: channels from the model's bind pose and the motion's helpers + initial pose.
    pub fn setup(&mut self, ip: &Interpolated, bones: &[Bone]) {
        self.joints = 0;
        self.ensure(ip, bones);
        for (j, b) in bones.iter().enumerate().take(self.s.len()) {
            self.s[j] = b.scale.extend(b.length);
            self.r[j] = b.rotation.extend(0.0);
            self.t[j] = b.translation.extend(0.0);
        }
        for (i, h) in ip.helpers.iter().enumerate() {
            let j = ip.bone_count + i;
            self.s[j] = h.scale.extend(h.length);
            self.r[j] = h.rotation.extend(0.0);
            self.t[j] = h.translation.extend(0.0);
        }
        for &(j, ch, v) in &ip.initial {
            self.set_channel(j, ch, v);
        }
        self.active = ip.constraints.iter().map(|c| c.temp_active != 0).collect();
    }

    fn set_channel(&mut self, j: usize, ch: usize, v: f32) {
        let Some(slot) = (match ch {
            0..=2 => self.s.get_mut(j),
            3..=5 => self.r.get_mut(j),
            6..=8 => self.t.get_mut(j),
            _ => None,
        }) else {
            return;
        };
        slot[ch % 3] = v;
    }

    pub fn joint_count(&self) -> usize {
        self.joints
    }
}

/// Evaluation context of one frame.
struct Ctx<'a> {
    ip: &'a Interpolated,
    /// Current frame variable used by expressions.
    frame_var: f32,
    fps: f32,
}

impl Interpolated {
    /// Sets channels from the F-curves at key time `tick` (60 Hz units).
    pub fn apply_curves(&self, st: &mut RigState, tick: f32) {
        for c in &self.forward {
            st.set_channel(c.joint, c.channel, c.eval(tick));
        }
        for c in &self.inverse {
            st.set_channel(self.bone_count + c.joint, c.channel, c.eval(tick));
        }
    }

    /// Constraint activation at key time `tick`.
    pub fn apply_activations(&self, st: &mut RigState, tick: f32) {
        st.active.resize(self.constraints.len(), false);
        for (i, c) in self.constraints.iter().enumerate() {
            if c.act_count <= 0 {
                continue;
            }
            let acts = &self.activations[c.act_start.max(0) as usize..][..c.act_count as usize];
            // Before the first entry the opposite of the first state applies.
            st.active[i] = if tick < acts[0].0 {
                acts[0].1 == 0
            } else {
                let k = acts.iter().rposition(|a| a.0 <= tick).unwrap_or(0);
                acts[k].1 != 0
            };
        }
    }

    /// Runs the rig for the current channels. `frame_var` is the expression frame variable.
    pub fn solve(&self, st: &mut RigState, frame_var: f32, fps: f32) {
        let ctx = Ctx { ip: self, frame_var, fps };
        let cons = &self.constraints;
        let mut ci = 0usize; // constraint cursor
        let mut ei = 0usize; // expression cursor
        let mut fi = 0usize; // external effector cursor
        let n = st.joints;
        for entry in &self.joints {
            let j = entry.index as usize;
            if j >= n {
                continue;
            }
            let mut f = entry.flags;
            let p = st.parent[j];
            let pu = usize::try_from(p).ok().filter(|&p| p < n);

            // 1. expressions
            while let Some(e) = self.expressions.get(ei).filter(|e| e.joint as usize == j) {
                let v = eval_expr(&ctx, st, e.node);
                let ch = e.channel as usize;
                st.set_channel(j, ch, if (3..=5).contains(&ch) { v * std::f32::consts::PI / 180.0 } else { v });
                ei += 1;
            }

            // 2. constraints on this joint
            let (mut pos1, mut pos2) = (None::<usize>, None::<usize>);
            let mut orient: Option<&Constraint> = None;
            let mut scale_src: Option<usize> = None;
            let mut pos_limit: Option<(usize, &Limiter)> = None;
            let mut rot_limit: Option<&Limiter> = None;
            while let Some(c) = cons.get(ci).filter(|c| c.constrained as usize == j) {
                if st.active.get(ci).copied().unwrap_or(false) {
                    let src = c.source as u16 as usize;
                    let aux = c.limiter as u16 as usize;
                    match c.kind {
                        0 => {
                            if pos1.is_none() {
                                pos1 = Some(src);
                            } else if pos2.is_none() {
                                pos2 = Some(src);
                            }
                        }
                        2..=4 => orient = Some(c),
                        5 => {
                            pos1 = Some(src);
                            pos2 = Some(aux);
                            orient = Some(c);
                        }
                        6 => scale_src = Some(src),
                        12 => pos_limit = self.limiters.get(aux).map(|l| (src, l)),
                        13 => rot_limit = self.limiters.get(aux),
                        _ => {}
                    }
                }
                ci += 1;
            }

            // 3. IK goal: next active position constraint on the effector
            let mut goal: Option<&Constraint> = None;
            if f & flag::IK_MASK != 0 {
                let eff = if f & flag::EXTERNAL_EFFECTOR != 0 {
                    let e = self.effectors.get(fi).map_or(0, |&e| e as u16 as usize);
                    fi += 1;
                    e
                } else {
                    j + if f & flag::IK_MASK == 1 { 1 } else { 2 }
                };
                while let Some(c) = cons.get(ci) {
                    ci += 1;
                    if st.active.get(ci - 1).copied().unwrap_or(false) && c.constrained as usize == eff && (c.kind <= 1 || c.kind == 5) {
                        goal = Some(c);
                        break;
                    }
                }
                if goal.is_none() {
                    f &= !flag::IK_MASK;
                }
            }

            // 4. world scale
            match scale_src.filter(|&s| s < n) {
                Some(src) => {
                    st.wscale[j] = st.wscale[src];
                    st.s[j] = match pu {
                        None => st.wscale[j],
                        Some(p) => (st.wscale[j].xyz() / st.wscale[p].xyz()).extend(st.s[j].w),
                    };
                }
                None => st.wscale[j] = pu.map_or(st.s[j], |p| st.wscale[p] * st.s[j]),
            }

            if f & flag::CALCULATED != 0 {
                if f & flag::EXTRACT_ROTATION != 0 {
                    let r = extract_euler(&st.local[j], st.r[j].xyz());
                    st.r[j] = r.extend(st.r[j].w);
                }
                continue;
            }
            let fin_of = |st: &RigState, k: Option<usize>| k.map_or(Mat4::IDENTITY, |k| st.fin[k]);
            let world_of = |st: &RigState, k: Option<usize>| k.map_or(Mat4::IDENTITY, |k| st.world[k]);
            let lp = st.t[j].xyz().extend(1.0);

            match f & flag::IK_MASK {
                0 => {
                    let need_inv =
                        pos1.is_some() || pos2.is_some() || orient.is_some() || f & (flag::TRANSLATION | flag::ROTATION) != 0 || pos_limit.is_some() || rot_limit.is_some();
                    // Constraint targets are brought into the parent's final (scaled) space.
                    let inv = if need_inv { fin_of(st, pu).inverse() } else { Mat4::IDENTITY };
                    // translation
                    let (world_pos, local_pos);
                    if f & flag::TRANSLATION == 0 {
                        if pos1.is_none() && pos2.is_none() {
                            let mut l = lp;
                            if let Some((src, lim)) = pos_limit {
                                let origin = inv * st.fin[src.min(n - 1)].w_axis;
                                limit(lim, origin, &mut l);
                            }
                            local_pos = l;
                            world_pos = point(&fin_of(st, pu), l);
                        } else {
                            let (w, mut l) = position_constraint(st, pos1, pos2, &inv);
                            if let Some((src, lim)) = pos_limit {
                                let origin = inv * st.fin[src.min(n - 1)].w_axis;
                                limit(lim, origin, &mut l);
                                world_pos = point(&fin_of(st, pu), l);
                            } else {
                                world_pos = w;
                            }
                            local_pos = l;
                        }
                    } else {
                        world_pos = st.fin[j].w_axis;
                        local_pos = inv * world_pos;
                    }
                    let local_pos = local_pos.xyz().extend(1.0);
                    // orientation
                    match orient.map(|c| c.kind) {
                        None => {
                            if let Some(lim) = rot_limit {
                                let origin = st.r[j];
                                let mut r = st.r[j];
                                limit(lim, origin, &mut r);
                                st.r[j] = r;
                            }
                            let mut l = euler(st.r[j].xyz());
                            l.w_axis = local_pos;
                            st.local[j] = l;
                            st.world[j] = match pu {
                                Some(p) if f & flag::ROTATION == 0 => st.world[p] * l,
                                _ => l,
                            };
                        }
                        Some(k @ (3 | 5)) => {
                            let c = orient.unwrap();
                            let tgt = if k == 3 { c.source } else { c.limiter } as u16 as usize;
                            let tl = inv * st.fin[tgt.min(n - 1)].w_axis;
                            let dir = tl.xyz() - local_pos.xyz();
                            if std::env::var("KH2_RIG_DEBUG").is_ok_and(|v| v.parse() == Ok(j)) {
                                eprintln!("joint {j}: kind {k} tgt {tgt} fin[tgt] {:?} inv {inv:?} tl {tl:?} lp {local_pos:?} dir {dir:?} rx {}", st.fin[tgt].w_axis, st.r[j].x);
                            }
                            let (x, y, z) = look_at(dir, Vec3::Y);
                            let mut l = st.local[j];
                            l.x_axis = x.extend(0.0);
                            l.y_axis = y.extend(0.0);
                            l.z_axis = z.extend(0.0);
                            let mut l = l * Mat4::from_rotation_x(st.r[j].x);
                            l.w_axis = local_pos;
                            st.local[j] = l;
                            st.world[j] = match pu {
                                None => l,
                                Some(p) => st.world[p] * l,
                            };
                        }
                        Some(2) => {
                            let src = orient.unwrap().source as u16 as usize;
                            st.world[j] = st.world[src.min(n - 1)];
                        }
                        Some(_) => {}
                    }
                    st.fin[j] = scale_axes(&st.world[j], st.wscale[j]);
                    st.fin[j].w_axis = world_pos.xyz().extend(1.0);
                    if orient.is_some() || f & flag::ROTATION != 0 {
                        let pinv = rigid_inverse(&world_of(st, pu));
                        let mut l = pinv * st.world[j];
                        l.w_axis = local_pos;
                        st.local[j] = l;
                        if f & flag::EXTRACT_ROTATION != 0 {
                            let r = extract_euler(&st.local[j], st.r[j].xyz());
                            st.r[j] = r.extend(st.r[j].w);
                        }
                    }
                }
                ik => {
                    let goal = goal.unwrap();
                    let two = ik != 1;
                    if two && orient.is_some_and(|c| c.kind != 4) {
                        orient = None;
                    }
                    let world_pos = point(&fin_of(st, pu), lp);
                    let pw = world_of(st, pu);
                    let inv = rigid_inverse(&pw);
                    let start = inv * world_pos;
                    let j1 = j + 1;
                    if two {
                        while cons.get(ci).is_some_and(|c| c.constrained as usize == j1) {
                            ci += 1;
                        }
                        if j1 < n {
                            st.wscale[j1] = st.wscale[j] * st.s[j1];
                        }
                    }
                    let (g2, aux) = (goal.source as u16 as usize, (goal.kind == 5).then_some(goal.limiter as u16 as usize));
                    let (_, goal_local) = position_constraint(st, Some(g2), aux, &inv);
                    let up = orient.filter(|c| c.kind == 4).map(|c| (inv * st.fin[(c.source as u16 as usize).min(n - 1)].w_axis).xyz());
                    let len1 = st.wscale[j].x * st.length[j];
                    let (len2, tw2) = if two && j1 < n { (st.wscale[j1].x * st.length[j1], st.r[j1].x) } else { (0.0, 0.0) };
                    let (first, second) = solve_ik(len1, len2, st.r[j].x, tw2, start.xyz(), goal_local.xyz(), ik == 3, up, two);
                    let mut l = first;
                    if f & flag::EXTRACT_ROTATION != 0 {
                        let r = extract_euler(&l, st.r[j].xyz());
                        st.r[j] = r.extend(st.r[j].w);
                    }
                    l.w_axis = lp;
                    st.local[j] = l;
                    st.world[j] = pw * l;
                    st.fin[j] = scale_axes(&st.world[j], st.wscale[j]);
                    st.fin[j].w_axis = world_pos.xyz().extend(1.0);
                    let mut last = j;
                    if two && j1 < n {
                        let mut l1 = second;
                        l1.w_axis = Vec4::new(st.length[j], 0.0, 0.0, 1.0);
                        st.local[j1] = l1;
                        st.world[j1] = st.world[j] * l1;
                        st.fin[j1] = scale_axes(&st.world[j1], st.wscale[j1]);
                        st.fin[j1].w_axis = (st.fin[j] * l1.w_axis).xyz().extend(1.0);
                        last = j1;
                    }
                    let eff = goal.constrained as u16 as usize;
                    if eff < n {
                        let p = st.fin[last] * st.t[eff].xyz().extend(1.0);
                        st.fin[eff].w_axis = p.xyz().extend(1.0);
                    }
                }
            }
        }
    }
}

/// World position from one or two (averaged) source joints, and the same in `inv`'s space.
fn position_constraint(st: &RigState, a: Option<usize>, b: Option<usize>, inv: &Mat4) -> (Vec4, Vec4) {
    let n = st.fin.len();
    let at = |k: usize| st.fin[k.min(n - 1)].w_axis;
    let world = match (a, b) {
        (Some(a), Some(b)) => (at(a) + at(b)) * 0.5,
        (Some(a), None) | (None, Some(a)) => at(a),
        (None, None) => Vec4::W,
    };
    (world, *inv * world)
}

fn eval_expr(ctx: &Ctx, st: &RigState, node: i16) -> f32 {
    let nodes = &ctx.ip.nodes;
    let Some(nd) = usize::try_from(node).ok().and_then(|i| nodes.get(i)) else { return 0.0 };
    let deg = 180.0 / std::f32::consts::PI;
    let rad = std::f32::consts::PI / 180.0;
    let car = || eval_expr(ctx, st, nd.car);
    let cdr = || eval_expr(ctx, st, nd.cdr);
    let node_at = |i: i16| usize::try_from(i).ok().and_then(|i| nodes.get(i)).copied();
    let b = |c: bool| if c { 1.0 } else { 0.0 };
    let n = st.fin.len();
    let joint = |e: i16| (e as u16 as usize).min(n.saturating_sub(1));
    match nd.kind {
        0 => (car() * rad).sin(),
        1 => (car() * rad).cos(),
        2 => (car() * rad).tan(),
        3 => car().asin() * deg,
        4 => car().acos() * deg,
        5 => car().atan() * deg,
        6 => car().ln(),
        7 => car().exp(),
        8 => car().abs(),
        9 => car().powf(cdr()),
        10 => car().sqrt(),
        11 => {
            let (a, c) = (car(), cdr());
            if a < c { a } else { c }
        }
        12 => {
            let (a, c) = (car(), cdr());
            if c < a { a } else { c }
        }
        13 => {
            // average over a list
            let (mut sum, mut count) = (0.0, 0);
            let mut cur = node_at(nd.car);
            while let Some(item) = cur.filter(|c| usize::try_from(c.car).is_ok_and(|i| i < nodes.len())) {
                sum += eval_expr(ctx, st, item.car);
                count += 1;
                cur = node_at(item.cdr);
            }
            sum / count as f32
        }
        14 => {
            // (cond then else) list
            let Some(l0) = node_at(nd.car) else { return 0.0 };
            let Some(l1) = node_at(l0.cdr) else { return 0.0 };
            if eval_expr(ctx, st, l0.car) == 0.0 {
                node_at(l1.cdr).map_or(0.0, |l2| eval_expr(ctx, st, l2.car))
            } else {
                eval_expr(ctx, st, l1.car)
            }
        }
        15 | 43 => {
            let t = car() * 60.0 / ctx.fps;
            let k = nd.cdr as u16 as usize;
            let curve: Option<&FCurve> = if k < ctx.ip.forward.len() { ctx.ip.forward.get(k) } else { ctx.ip.inverse.get(k - ctx.ip.forward.len()) };
            let v = curve.map_or(0.0, |c| c.eval(t));
            if nd.kind == 43 { v * deg } else { v }
        }
        16 => (st.fin[joint(nd.car)].w_axis - st.fin[joint(nd.cdr)].w_axis).xyz().length(),
        17 => car() % cdr(),
        18 => car() + cdr(),
        19 => car() - cdr(),
        20 => car() * cdr(),
        21 => car() / cdr(),
        22 => (car() as i32).checked_rem(cdr() as i32).unwrap_or(0) as f32,
        23 => b(car() == cdr()),
        24 => {
            let (a, c) = (car(), cdr());
            b(c < a)
        }
        25 => {
            let (a, c) = (car(), cdr());
            b(c <= a)
        }
        26 => {
            let (a, c) = (car(), cdr());
            b(a < c)
        }
        27 => {
            let (a, c) = (car(), cdr());
            b(a <= c)
        }
        28 => {
            if car() == 0.0 {
                0.0
            } else {
                b(cdr() != 0.0)
            }
        }
        29 => {
            if car() != 0.0 {
                1.0
            } else {
                b(cdr() != 0.0)
            }
        }
        30 => ctx.frame_var,
        31 => nd.value,
        32..=34 => {
            let j = joint(nd.element);
            let m = if nd.global { st.fin[j] } else { st.local[j] };
            m.w_axis[(nd.kind - 32) as usize]
        }
        35..=37 => {
            let j = joint(nd.element);
            let r = if nd.global { extract_euler(&st.world[j], st.r[j].xyz()) } else { st.r[j].xyz() };
            r[(nd.kind - 35) as usize] * deg
        }
        38..=40 => {
            let j = joint(nd.element);
            let k = (nd.kind - 38) as usize;
            if nd.global { st.wscale[j][k] } else { st.s[j][k] }
        }
        _ => 0.0,
    }
}

/// One frame of a motion: setup when the motion changed, then curves at `tick`, constraint
/// activations and the rig solve. Like the game, channels the curves do not drive keep the
/// values left by earlier frames of the same motion.
pub fn evaluate(ip: &Interpolated, bones: &[Bone], st: &mut RigState, tick: f32, frame_var: f32, fps: f32) {
    let key = ip as *const Interpolated as usize;
    if st.key != key || st.joints == 0 {
        st.setup(ip, bones);
        st.key = key;
    }
    ip.apply_curves(st, tick);
    ip.apply_activations(st, tick);
    ip.solve(st, frame_var, fps);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_bone_ik_reaches_goal() {
        let start = Vec3::ZERO;
        let goal = Vec3::new(10.0, 5.0, 0.0);
        let (a, b) = solve_ik(8.0, 6.0, 0.0, 0.0, start, goal, false, None, true);
        let mut second = b;
        second.w_axis = Vec4::new(8.0, 0.0, 0.0, 1.0);
        let end = (a * second * Vec4::new(6.0, 0.0, 0.0, 1.0)).xyz();
        assert!((end - goal).length() < 1e-3, "{end:?}");
    }

    #[test]
    fn euler_round_trip() {
        let r = Vec3::new(0.3, -0.7, 1.2);
        let e = extract_euler(&euler(r), Vec3::ZERO);
        assert!((e - r).length() < 1e-4, "{e:?}");
    }
}
