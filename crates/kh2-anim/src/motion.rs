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
    pub keys: Vec<Key>,
}

impl FCurve {
    pub fn eval(&self, t: f32) -> f32 {
        let k = &self.keys;
        match k.len() {
            0 => 0.0,
            1 => k[0].value,
            _ => {
                if t <= k[0].time {
                    return k[0].value;
                }
                if t >= k[k.len() - 1].time {
                    return k[k.len() - 1].value;
                }
                let i = k.partition_point(|x| x.time <= t).saturating_sub(1).min(k.len() - 2);
                let (a, b) = (k[i], k[i + 1]);
                let dt = b.time - a.time;
                if dt <= 0.0 {
                    return a.value;
                }
                let s = (t - a.time) / dt;
                match a.interp {
                    0 => a.value,
                    1 => a.value + (b.value - a.value) * s,
                    _ => {
                        // Cubic Hermite; tangents are per frame.
                        let (s2, s3) = (s * s, s * s * s);
                        let h1 = 2.0 * s3 - 3.0 * s2 + 1.0;
                        let h2 = -2.0 * s3 + 3.0 * s2;
                        let h3 = s3 - 2.0 * s2 + s;
                        let h4 = s3 - s2;
                        h1 * a.value + h2 * b.value + (h3 * a.tan_out + h4 * b.tan_in) * dt
                    }
                }
            }
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
}

#[derive(Debug, Clone)]
pub enum Data {
    Interpolated(Interpolated),
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
                let bone_count = i16_at(b, h) as usize;
                let total_count = (i16_at(b, h + 2) as usize).max(bone_count);
                let ik_off = u32_at(b, h + 8) as usize;
                let joints_off = u32_at(b, h + 0x0C) as usize;
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
                            FCurve { joint, channel: (ch & 0xF) as usize, keys }
                        })
                        .collect()
                };
                let n_help = total_count - bone_count;
                let mut helpers = Vec::with_capacity(n_help);
                let mut helper_parent = Vec::with_capacity(n_help);
                for i in 0..n_help {
                    let o = ik_off + 64 * i;
                    let v = |k: usize| Vec3::new(f32_at(b, o + k), f32_at(b, o + k + 4), f32_at(b, o + k + 8));
                    let parent = i16_at(b, o + 4) as i32;
                    helper_parent.push(parent);
                    helpers.push(Bone { parent, scale: v(0x10), rotation: v(0x20), translation: v(0x30) });
                }
                Some(Motion {
                    frame_start: f32_at(b, h + 0x80),
                    frame_end: f32_at(b, h + 0x84),
                    fps: f32_at(b, h + 0x88),
                    frame_return: f32_at(b, h + 0x8C),
                    data: Data::Interpolated(Interpolated {
                        bone_count,
                        total_count,
                        initial,
                        forward: curves(fwd_off, fwd_n),
                        inverse: curves(inv_off, inv_n),
                        helpers,
                        helper_parent,
                    }),
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

    /// Model-space matrices for the model's bones at `frame` (local frame units).
    pub fn pose(&self, bones: &[Bone], frame: f32) -> Pose {
        match &self.data {
            Data::Raw { frames, .. } => {
                let i = (frame.max(0.0) as usize).min(frames.len().saturating_sub(1));
                let mut m = frames.get(i).cloned().unwrap_or_default();
                m.resize(bones.len(), Mat4::IDENTITY);
                m
            }
            Data::Interpolated(ip) => {
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
        let lin = FCurve { joint: 0, channel: 6, keys: vec![key(1, 0.0, 0.0), key(1, 10.0, 10.0)] };
        assert_eq!(lin.eval(-5.0), 0.0);
        assert!((lin.eval(2.5) - 2.5).abs() < 1e-6);
        assert_eq!(lin.eval(20.0), 10.0);
        let step = FCurve { joint: 0, channel: 6, keys: vec![key(0, 0.0, 1.0), key(0, 10.0, 5.0)] };
        assert_eq!(step.eval(9.9), 1.0);
        // Hermite with flat tangents eases: halfway value is the midpoint, quarter is below linear.
        let herm = FCurve { joint: 0, channel: 6, keys: vec![key(2, 0.0, 0.0), key(2, 10.0, 10.0)] };
        assert!((herm.eval(5.0) - 5.0).abs() < 1e-5);
        assert!(herm.eval(2.5) < 2.5);
    }
}
