//! Motion data: interpolated (type 0) and raw (type 1). Offsets inside a motion are relative to the
//! end of its 0x90-byte reserved area.

use glam::{Mat4, Quat, Vec3};
use kh2_model::Bone;

const RESERVED: usize = 0x90;

fn u32_at(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).unwrap_or(0)
}
fn i16_at(b: &[u8], o: usize) -> i16 {
    b.get(o..o + 2).map(|s| i16::from_le_bytes([s[0], s[1]])).unwrap_or(0)
}
fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_at(b, o))
}

/// Number of joints whose "external effector" flag (bit 7) is set.
fn joints_count_flagged(b: &[u8], joints_off: usize, n: usize) -> usize {
    (0..n).filter(|i| b.get(joints_off + 4 * i + 2).is_some_and(|f| f & 0x80 != 0)).count()
}

#[derive(Debug, Clone, Copy)]
pub struct Key {
    pub interp: u8,
    pub time: f32,
    pub value: f32,
    pub tan_in: f32,
    pub tan_out: f32,
}

#[derive(Debug, Clone)]
pub struct FCurve {
    pub joint: usize,
    pub channel: usize,
    /// Behaviour before the first key / after the last key: 0 hold, 1 extrapolate along the
    /// end tangent, 2 repeat, 3 zero.
    pub pre: u8,
    pub post: u8,
    pub keys: Vec<Key>,
}

impl FCurve {
    /// Value at key time `t` (60 Hz ticks).
    pub fn eval(&self, mut t: f32) -> f32 {
        let k = &self.keys;
        let (Some(first), Some(last)) = (k.first(), k.last()) else { return 0.0 };
        let period = last.time - first.time;
        if t <= first.time {
            match self.pre {
                0 => return first.value,
                1 => return first.value - (first.time - t) * first.tan_in,
                2 if period > 0.0 => t += ((first.time - t) / period).ceil() * period,
                2 => return first.value,
                _ => return 0.0,
            }
        } else if t >= last.time {
            match self.post {
                0 => return last.value,
                1 => return last.value + (t - last.time) * last.tan_out,
                2 if period > 0.0 => t -= ((t - last.time) / period).ceil() * period,
                2 => return last.value,
                _ => return 0.0,
            }
        }
        if k.len() == 1 {
            return first.value;
        }
        let i = k.partition_point(|x| x.time <= t).saturating_sub(1).min(k.len() - 2);
        let (a, b) = (k[i], k[i + 1]);
        let dt = b.time - a.time;
        if dt <= 0.0 || t == a.time {
            return a.value;
        }
        let s = (t - a.time) / dt;
        match a.interp {
            0 => a.value,
            1 => a.value + (b.value - a.value) * s,
            2 => {
                // Cubic Hermite; tangents are per tick.
                let (s2, s3) = (s * s, s * s * s);
                let h1 = 2.0 * s3 - 3.0 * s2 + 1.0;
                let h2 = -2.0 * s3 + 3.0 * s2;
                let h3 = s3 - 2.0 * s2 + s;
                let h4 = s3 - s2;
                h1 * a.value + h2 * b.value + (h3 * a.tan_out + h4 * b.tan_in) * dt
            }
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Interpolated {
    pub bone_count: usize,
    pub total_count: usize,
    /// (joint, channel, value)
    pub initial: Vec<(usize, usize, f32)>,
    pub forward: Vec<FCurve>,
    pub inverse: Vec<FCurve>,
    /// IK helper joints (appended after the model bones).
    pub helpers: Vec<Bone>,
    /// Parent of each IK helper (index into the full joint list), or -1.
    pub helper_parent: Vec<i32>,
    /// IK helper flags word (bit 0 terminate, 1 below, 2 enable bias, upper bits unknown).
    pub helper_flags: Vec<u32>,
    /// Evaluation order of bones + IK helpers with per-joint flags.
    pub joints: Vec<Joint>,
    /// Root position: base values plus forward F-curve indices driving its S/R/T channels.
    pub root: Option<Root>,
    pub constraints: Vec<Constraint>,
    /// (time, active) entries addressed by constraints.
    pub activations: Vec<(f32, i32)>,
    pub limiters: Vec<Limiter>,
    pub expressions: Vec<Expression>,
    pub nodes: Vec<ExprNode>,
    /// Joints flagged as external effectors.
    pub effectors: Vec<i16>,
}

#[derive(Debug, Clone, Copy)]
pub struct Joint {
    pub index: u16,
    /// bits 0-1 ik, 2 translation, 3 rotation, 4 fixed, 5 calculated, 6 calculated matrix2 rotation, 7 external effector
    pub flags: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct Root {
    /// Root motion is evaluated only when this word (the scale vector's W) is non-zero.
    pub enabled: bool,
    pub scale: Vec3,
    pub rotation: Vec3,
    pub translation: Vec3,
    /// Forward F-curve index per channel (sx, sy, sz, rx, ry, rz, tx, ty, tz), -1 = static.
    pub curves: [i32; 9],
}

#[derive(Debug, Clone, Copy)]
pub struct Constraint {
    pub kind: u8,
    pub temp_active: u8,
    pub constrained: i16,
    pub source: i16,
    pub limiter: i16,
    pub act_count: i16,
    pub act_start: i16,
}

#[derive(Debug, Clone, Copy)]
pub struct Limiter {
    pub flags: u32,
    pub damping_width: f32,
    pub damping_strength: f32,
    pub min: [f32; 4],
    pub max: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct Expression {
    pub joint: i16,
    pub channel: i16,
    pub node: i16,
}

#[derive(Debug, Clone, Copy)]
pub struct ExprNode {
    pub kind: i8,
    pub global: bool,
    pub element: i16,
    pub value: f32,
    pub car: i16,
    pub cdr: i16,
}

impl Interpolated {
    /// A curve addressed by a combined index: forward curves first, then IK-helper curves.
    pub fn curve(&self, index: i32) -> Option<&FCurve> {
        let i = usize::try_from(index).ok()?;
        self.forward.get(i).or_else(|| self.inverse.get(i - self.forward.len()))
    }

    /// Root transform at key time `tick` (60 Hz): base scale/rotation/translation with any
    /// curve-driven channels, or `None` when the motion has no root motion.
    pub fn root_at(&self, tick: f32) -> Option<RootPose> {
        let r = self.root.as_ref().filter(|r| r.enabled)?;
        let base = [r.scale.x, r.scale.y, r.scale.z, r.rotation.x, r.rotation.y, r.rotation.z, r.translation.x, r.translation.y, r.translation.z];
        let ch: [f32; 9] = std::array::from_fn(|k| self.curve(r.curves[k]).map_or(base[k], |c| c.eval(tick)));
        let rot = crate::rig::euler(Vec3::new(ch[3], ch[4], ch[5]));
        Some(RootPose { translation: Vec3::new(ch[6], ch[7], ch[8]), yaw: rot.z_axis.x.atan2(rot.z_axis.z) })
    }
}

/// Root motion sample: model-space translation and heading.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RootPose {
    pub translation: Vec3,
    pub yaw: f32,
}

#[derive(Debug, Clone)]
pub enum Data {
    Interpolated(Box<Interpolated>),
    /// Per frame, per bone model-space matrices.
    Raw { bones: usize, frames: Vec<Vec<Mat4>> },
}

#[derive(Debug, Clone)]
pub struct Motion {
    pub frame_start: f32,
    pub frame_end: f32,
    /// Local frame rate (frames per second the curves are authored at).
    pub fps: f32,
    /// Where a looping motion jumps back to.
    pub frame_return: f32,
    pub data: Data,
}

/// Per-bone local transform.
#[derive(Debug, Clone, Copy)]
pub struct Srt {
    pub scale: Vec3,
    pub rot: Vec3,
    pub trans: Vec3,
}

pub type Pose = Vec<Mat4>;

impl Motion {
    pub fn parse(file: &[u8]) -> Option<Motion> {
        let b = file.get(RESERVED..)?;
        let kind = u32_at(b, 0);
        let h = 0x10; // after the 16-byte motion header
        match kind {
            0 => {
                let bone_count = i16_at(b, h) as u16 as usize;
                let total_count = (i16_at(b, h + 2) as u16 as usize).max(bone_count);
                let ik_off = u32_at(b, h + 8) as usize;
                let joints_off = u32_at(b, h + 0x0C) as usize;
                // The helper bones and the joint table must lie inside the file.
                if total_count > bone_count && ik_off.saturating_add(64 * (total_count - bone_count)) > b.len() {
                    return None;
                }
                if total_count > 0 && joints_off.saturating_add(4 * total_count) > b.len() {
                    return None;
                }
                let (init_off, init_n) = (u32_at(b, h + 0x14) as usize, u32_at(b, h + 0x18) as usize);
                let (fwd_off, fwd_n) = (u32_at(b, h + 0x20) as usize, u32_at(b, h + 0x24) as usize);
                let (inv_off, inv_n) = (u32_at(b, h + 0x28) as usize, u32_at(b, h + 0x2C) as usize);
                let keys_off = u32_at(b, h + 0x30) as usize;
                let (times, values, tans) = (u32_at(b, h + 0x34) as usize, u32_at(b, h + 0x38) as usize, u32_at(b, h + 0x3C) as usize);
                let _ = joints_off;
                let initial = (0..init_n.min(100_000))
                    .map(|i| {
                        let o = init_off + 8 * i;
                        (i16_at(b, o) as usize, i16_at(b, o + 2) as usize, f32_at(b, o + 4))
                    })
                    .collect();
                let curves = |off: usize, n: usize| -> Vec<FCurve> {
                    (0..n.min(100_000))
                        .map(|i| {
                            let o = off + 6 * i;
                            let joint = i16_at(b, o) as usize;
                            let ch = b.get(o + 2).copied().unwrap_or(0);
                            let count = b.get(o + 3).copied().unwrap_or(0) as usize;
                            let start = i16_at(b, o + 4) as u16 as usize;
                            let keys = (0..count)
                                .map(|k| {
                                    let ko = keys_off + 8 * (start + k);
                                    let tw = i16_at(b, ko) as u16;
                                    Key {
                                        interp: (tw & 3) as u8,
                                        time: f32_at(b, times + 4 * (tw >> 2) as usize),
                                        value: f32_at(b, values + 4 * i16_at(b, ko + 2) as u16 as usize),
                                        tan_in: f32_at(b, tans + 4 * i16_at(b, ko + 4) as u16 as usize),
                                        tan_out: f32_at(b, tans + 4 * i16_at(b, ko + 6) as u16 as usize),
                                    }
                                })
                                .collect();
                            FCurve { joint, channel: (ch & 0xF) as usize, pre: (ch >> 4) & 3, post: (ch >> 6) & 3, keys }
                        })
                        .collect()
                };
                let n_help = total_count - bone_count;
                let mut helpers = Vec::with_capacity(n_help);
                let mut helper_parent = Vec::with_capacity(n_help);
                let mut helper_flags = Vec::with_capacity(n_help);
                for i in 0..n_help {
                    let o = ik_off + 64 * i;
                    let v = |k: usize| Vec3::new(f32_at(b, o + k), f32_at(b, o + k + 4), f32_at(b, o + k + 8));
                    let parent = i16_at(b, o + 4) as i32;
                    helper_parent.push(parent);
                    helper_flags.push(u32_at(b, o + 0x0C));
                    helpers.push(Bone { parent, scale: v(0x10), rotation: v(0x20), translation: v(0x30), length: f32_at(b, o + 0x1C) });
                }
                let joints = (0..total_count)
                    .map(|i| Joint { index: i16_at(b, joints_off + 4 * i) as u16, flags: b.get(joints_off + 4 * i + 2).copied().unwrap_or(0) })
                    .collect();
                let root_off = u32_at(b, h + 0x1C) as usize;
                let root = (root_off != 0).then(|| {
                    let v = |k: usize| Vec3::new(f32_at(b, root_off + k), f32_at(b, root_off + k + 4), f32_at(b, root_off + k + 8));
                    Root {
                        enabled: u32_at(b, root_off + 0x0C) != 0,
                        scale: v(0),
                        rotation: v(0x10),
                        translation: v(0x20),
                        curves: std::array::from_fn(|k| u32_at(b, root_off + 0x30 + 4 * k) as i32),
                    }
                });
                let (con_off, con_n) = (u32_at(b, h + 0x40) as usize, u32_at(b, h + 0x44) as usize);
                let constraints: Vec<Constraint> = (0..con_n.min(10_000))
                    .map(|i| {
                        let o = con_off + 12 * i;
                        Constraint {
                            kind: b.get(o).copied().unwrap_or(0),
                            temp_active: b.get(o + 1).copied().unwrap_or(0),
                            constrained: i16_at(b, o + 2),
                            source: i16_at(b, o + 4),
                            limiter: i16_at(b, o + 6),
                            act_count: i16_at(b, o + 8),
                            act_start: i16_at(b, o + 10),
                        }
                    })
                    .collect();
                let act_off = u32_at(b, h + 0x48) as usize;
                let act_n = constraints.iter().map(|c| (c.act_start.max(0) + c.act_count.max(0)) as usize).max().unwrap_or(0);
                let activations = (0..act_n).map(|i| (f32_at(b, act_off + 8 * i), u32_at(b, act_off + 8 * i + 4) as i32)).collect();
                let lim_off = u32_at(b, h + 0x4C) as usize;
                let lim_n = constraints.iter().map(|c| c.limiter + 1).max().unwrap_or(0).max(0) as usize;
                let limiters = (0..lim_n)
                    .map(|i| {
                        let o = lim_off + 0x30 * i;
                        let v4 = |k: usize| std::array::from_fn(|j| f32_at(b, o + k + 4 * j));
                        Limiter { flags: u32_at(b, o), damping_width: f32_at(b, o + 8), damping_strength: f32_at(b, o + 0x0C), min: v4(0x10), max: v4(0x20) }
                    })
                    .collect();
                let (ex_off, ex_n) = (u32_at(b, h + 0x50) as usize, u32_at(b, h + 0x54) as usize);
                let expressions = (0..ex_n.min(10_000))
                    .map(|i| {
                        let o = ex_off + 8 * i;
                        Expression { joint: i16_at(b, o), channel: i16_at(b, o + 2), node: i16_at(b, o + 6) }
                    })
                    .collect();
                let (nd_off, nd_n) = (u32_at(b, h + 0x58) as usize, u32_at(b, h + 0x5C) as usize);
                let nodes = (0..nd_n.min(100_000))
                    .map(|i| {
                        let o = nd_off + 12 * i;
                        ExprNode {
                            kind: b.get(o).copied().unwrap_or(0) as i8,
                            global: b.get(o + 1).copied().unwrap_or(0) & 1 != 0,
                            element: i16_at(b, o + 2),
                            value: f32_at(b, o + 4),
                            car: i16_at(b, o + 8),
                            cdr: i16_at(b, o + 10),
                        }
                    })
                    .collect();
                let eff_off = u32_at(b, h + 0x90) as usize;
                let effectors = if eff_off == 0 {
                    Vec::new()
                } else {
                    let n = joints_count_flagged(b, joints_off, total_count);
                    (0..n).map(|i| i16_at(b, eff_off + 2 * i)).collect()
                };
                Some(Motion {
                    frame_start: f32_at(b, h + 0x80),
                    frame_end: f32_at(b, h + 0x84),
                    fps: f32_at(b, h + 0x88),
                    frame_return: f32_at(b, h + 0x8C),
                    data: Data::Interpolated(Box::new(Interpolated {
                        bone_count,
                        total_count,
                        initial,
                        forward: curves(fwd_off, fwd_n),
                        inverse: curves(inv_off, inv_n),
                        helpers,
                        helper_parent,
                        helper_flags,
                        joints,
                        root,
                        constraints,
                        activations,
                        limiters,
                        expressions,
                        nodes,
                        effectors,
                    })),
                })
            }
            1 => {
                let bones = u32_at(b, h) as usize;
                let total = u32_at(b, h + 0x14) as usize;
                let mat_off = u32_at(b, h + 0x18) as usize;
                let frames = (0..total.min(10_000))
                    .map(|f| {
                        (0..bones)
                            .map(|j| {
                                let o = mat_off + 64 * (f * bones + j);
                                Mat4::from_cols_array(&std::array::from_fn(|k| f32_at(b, o + 4 * k)))
                            })
                            .collect()
                    })
                    .collect();
                Some(Motion {
                    frame_start: f32_at(b, h + 0x40),
                    frame_end: f32_at(b, h + 0x44),
                    fps: f32_at(b, h + 0x48),
                    frame_return: f32_at(b, h + 0x4C),
                    data: Data::Raw { bones, frames },
                })
            }
            _ => None,
        }
    }

    /// Length in seconds from frame_start to frame_end at the motion's own frame rate.
    pub fn duration(&self) -> f32 {
        (self.frame_end - self.frame_start).max(0.0) / self.fps.max(1.0)
    }

    /// Key time (60 Hz ticks) of local frame `frame`.
    pub fn tick(&self, frame: f32) -> f32 {
        frame * 60.0 / self.fps.max(1.0)
    }

    /// Model-space matrices for the model's bones at local frame `frame`, with the full control
    /// rig (IK, constraints, expressions) evaluated from a fresh state.
    pub fn pose(&self, bones: &[Bone], frame: f32) -> Pose {
        let mut st = crate::rig::RigState::default();
        self.pose_with(bones, frame, &mut st)
    }

    /// Like [`Motion::pose`] but keeps the rig's work buffers in `st` between calls (the game
    /// keeps them per object: channels the curves do not drive carry over).
    pub fn pose_with(&self, bones: &[Bone], frame: f32, st: &mut crate::rig::RigState) -> Pose {
        match &self.data {
            Data::Interpolated(ip) => {
                let tick = self.tick(frame);
                crate::rig::evaluate(ip, bones, st, tick, self.frame_start + frame, self.fps.max(1.0));
                st.fin.iter().take(bones.len()).copied().collect()
            }
            Data::Raw { .. } => self.pose_fk(bones, frame),
        }
    }

    /// Forward kinematics only (no rig): bind values, initial pose, then curves.
    pub fn pose_fk(&self, bones: &[Bone], frame: f32) -> Pose {
        match &self.data {
            Data::Raw { frames, .. } => {
                let i = (frame.max(0.0) as usize).min(frames.len().saturating_sub(1));
                let mut m = frames.get(i).cloned().unwrap_or_default();
                m.resize(bones.len(), Mat4::IDENTITY);
                m
            }
            Data::Interpolated(ip) => {
                let frame = self.tick(frame);
                let n = ip.total_count.max(bones.len());
                // Start from the model's bind values, then the initial pose, then the curves.
                let mut srt: Vec<Srt> = (0..n)
                    .map(|j| {
                        let src = bones.get(j).or_else(|| ip.helpers.get(j.wrapping_sub(ip.bone_count)));
                        src.map(|b| Srt { scale: b.scale, rot: b.rotation, trans: b.translation })
                            .unwrap_or(Srt { scale: Vec3::ONE, rot: Vec3::ZERO, trans: Vec3::ZERO })
                    })
                    .collect();
                let set = |srt: &mut Vec<Srt>, j: usize, ch: usize, v: f32| {
                    if let Some(s) = srt.get_mut(j) {
                        match ch {
                            0 => s.scale.x = v,
                            1 => s.scale.y = v,
                            2 => s.scale.z = v,
                            3 => s.rot.x = v,
                            4 => s.rot.y = v,
                            5 => s.rot.z = v,
                            6 => s.trans.x = v,
                            7 => s.trans.y = v,
                            8 => s.trans.z = v,
                            _ => {}
                        }
                    }
                };
                for &(j, ch, v) in &ip.initial {
                    set(&mut srt, j, ch, v);
                }
                for c in &ip.forward {
                    set(&mut srt, c.joint, c.channel, c.eval(frame));
                }
                for c in &ip.inverse {
                    set(&mut srt, ip.bone_count + c.joint, c.channel, c.eval(frame));
                }
                let mut world = vec![Mat4::IDENTITY; n];
                for j in 0..n {
                    let parent = if j < ip.bone_count { bones.get(j).map_or(-1, |b| b.parent) } else { ip.helper_parent[j - ip.bone_count] };
                    let s = srt[j];
                    let local = Mat4::from_scale_rotation_translation(
                        s.scale,
                        Quat::from_rotation_z(s.rot.z) * Quat::from_rotation_y(s.rot.y) * Quat::from_rotation_x(s.rot.x),
                        s.trans,
                    );
                    world[j] = match usize::try_from(parent).ok().filter(|&p| p < n && p != j) {
                        Some(p) if p < j || p >= ip.bone_count => world[p] * local,
                        _ => local,
                    };
                }
                world.truncate(bones.len());
                world
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(interp: u8, time: f32, value: f32) -> Key {
        Key { interp, time, value, tan_in: 0.0, tan_out: 0.0 }
    }

    #[test]
    fn fcurve_interpolates_and_clamps() {
        let lin = FCurve { joint: 0, channel: 6, pre: 0, post: 0, keys: vec![key(1, 0.0, 0.0), key(1, 10.0, 10.0)] };
        assert_eq!(lin.eval(-5.0), 0.0);
        assert!((lin.eval(2.5) - 2.5).abs() < 1e-6);
        assert_eq!(lin.eval(20.0), 10.0);
        let step = FCurve { joint: 0, channel: 6, pre: 0, post: 0, keys: vec![key(0, 0.0, 1.0), key(0, 10.0, 5.0)] };
        assert_eq!(step.eval(9.9), 1.0);
        // Hermite with flat tangents eases: halfway value is the midpoint, quarter is below linear.
        let herm = FCurve { joint: 0, channel: 6, pre: 0, post: 0, keys: vec![key(2, 0.0, 0.0), key(2, 10.0, 10.0)] };
        assert!((herm.eval(5.0) - 5.0).abs() < 1e-5);
        assert!(herm.eval(2.5) < 2.5);
    }
}

#[cfg(test)]
mod bound_tests {
    use super::*;

    #[test]
    fn negative_or_huge_joint_counts_are_rejected() {
        let mut f = vec![0u8; RESERVED + 0x100];
        f[RESERVED + 0x10..RESERVED + 0x12].copy_from_slice(&1i16.to_le_bytes());
        f[RESERVED + 0x12..RESERVED + 0x14].copy_from_slice(&(-1i16).to_le_bytes());
        assert!(Motion::parse(&f).is_none());
    }
}
