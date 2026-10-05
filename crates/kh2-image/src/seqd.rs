//! SEQD sprite sequences: the game's 2D animation format for HUD elements and fonts.
//!
//! File layout per OpenKH `OpenKh.Kh2/Sequence.cs` (Apache-2.0): a 0x30-byte header (magic, a
//! version word, then offset/count pairs for sprites, sprite parts, sprite groups, animations,
//! animation groups, and a UV-enable word) followed by the five tables. The evaluation rules
//! (frame stepping, interpolation, transform order, colour modulation, blend selection) were read
//! from the game's own sequence player in a local decompilation; see docs/hud.md for addresses.

use crate::{rd16, rd32};

/// A rectangle of the group's image with four corner colours (`0x80` per channel = unchanged).
#[derive(Debug, Clone, Copy, Default)]
pub struct Sprite {
    pub palette: i32,
    pub u0: i32,
    pub v0: i32,
    pub u1: i32,
    pub v1: i32,
    pub u_scroll: f32,
    pub v_scroll: f32,
    /// Top-left, top-right, bottom-left, bottom-right (RGBA bytes, alpha last).
    pub colors: [[u8; 4]; 4],
}

/// One sprite placed inside a sprite group, as a screen rectangle relative to the group origin.
#[derive(Debug, Clone, Copy, Default)]
pub struct Part {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub sprite: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SpriteGroup {
    pub start: usize,
    pub count: usize,
}

/// One keyframed placement of a sprite group (start and end values over a frame range).
#[derive(Debug, Clone, Default)]
pub struct Anim {
    pub flags: u32,
    pub group: usize,
    pub frame_start: i32,
    pub frame_end: i32,
    pub translate_x: [i32; 2],
    pub translate_y: [i32; 2],
    pub pivot_x: [i32; 2],
    pub pivot_y: [i32; 2],
    pub rot_x: [f32; 2],
    pub rot_y: [f32; 2],
    pub rot_z: [f32; 2],
    pub scale: [f32; 2],
    pub scale_x: [f32; 2],
    pub scale_y: [f32; 2],
    /// Bezier control values (start side, end side) for the time curve; unused when linear.
    pub curve_start: f32,
    pub curve_end: f32,
    pub bounce_x: [f32; 2],
    pub bounce_y: [f32; 2],
    pub bounce_x_count: u16,
    pub bounce_y_count: u16,
    /// 0 = normal alpha blend, 1 = additive, 2 = subtractive (GS ALPHA register presets).
    pub blend: u32,
    pub color_start: [u8; 4],
    pub color_end: [u8; 4],
}

pub mod flag {
    /// Linear time curve (otherwise the cubic Bezier from `curve_*`).
    pub const LINEAR: u32 = 0x1;
    /// Nearest-neighbour texture sampling (otherwise bilinear).
    pub const NEAREST: u32 = 0x4;
    /// Bounce uses cosine instead of sine.
    pub const BOUNCE_COS: u32 = 0x8;
    pub const NO_BOUNCE: u32 = 0x10;
    pub const NO_ROTATION: u32 = 0x20;
    pub const NO_SCALE: u32 = 0x40;
    /// Colour does not interpolate (uses the start colour).
    pub const COLOR_STEP: u32 = 0x80;
    pub const NO_COLOR: u32 = 0x400;
    pub const NO_PIVOT: u32 = 0x4000;
    /// The last frame itself is not drawn.
    pub const EXCLUSIVE_END: u32 = 0x8000;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AnimGroup {
    pub first: usize,
    pub count: usize,
    /// Number of plays (0 = loop forever).
    pub loops: u16,
    pub flags: u16,
    /// Frame a looping group restarts from.
    pub loop_start: i32,
    /// Explicit last frame (0 = derived from the animations).
    pub frame_end: i32,
    pub x: i32,
    pub y: i32,
    pub size: i32,
    pub unk1c: i32,
    pub unk20: i32,
}

#[derive(Debug, Clone, Default)]
pub struct Seqd {
    pub sprites: Vec<Sprite>,
    pub parts: Vec<Part>,
    pub sprite_groups: Vec<SpriteGroup>,
    pub anims: Vec<Anim>,
    pub groups: Vec<AnimGroup>,
}

/// A drawn rectangle: corners in order top-left, top-right, bottom-left, bottom-right.
#[derive(Debug, Clone, Copy)]
pub struct Quad {
    pub pos: [[f32; 2]; 4],
    /// Texel coordinates in the image.
    pub uv: [[f32; 2]; 4],
    pub color: [[u8; 4]; 4],
    pub blend: u32,
    pub nearest: bool,
}

/// Values a game object can impose on a sequence while it plays (the game's "external" scale and
/// colour). Defaults leave the sequence unchanged.
#[derive(Debug, Clone, Copy)]
pub struct Ext {
    /// Overall, x and y scale multipliers applied on top of the animation's own.
    pub scale: [f32; 3],
    /// Colour multiplier per channel (R, G, B, A), 1.0 = unchanged.
    pub color: [f32; 4],
    /// Alpha fade factor applied to every vertex alpha (1.0 = unchanged).
    pub fade: f32,
    /// Steps since the player started; drives sprite UV scrolling.
    pub time: f32,
    /// Extra rotation (x, y, z radians) added to every animation's own; `None` = unused.
    pub rot: Option<[f32; 3]>,
}

impl Default for Ext {
    fn default() -> Ext {
        Ext { scale: [1.0; 3], color: [1.0; 4], fade: 1.0, time: 0.0, rot: None }
    }
}

/// The animation's rotation at `t` with the object's extra rotation applied the way the game does:
/// added to the keyframes, or used alone when the animation disables its own rotation.
fn rotation(a: &Anim, t: f32, ext: &Ext) -> Option<[f32; 3]> {
    let own = (a.flags & flag::NO_ROTATION == 0).then(|| [lerp(a.rot_x, t), lerp(a.rot_y, t), lerp(a.rot_z, t)]);
    match (own, ext.rot) {
        (Some(r), Some(e)) => Some([r[0] + e[0], r[1] + e[1], r[2] + e[2]]),
        (Some(r), None) => Some(r),
        (None, e) => e,
    }
}

/// Rows of the 2x2 matrix the game builds from the three angles: (m1c, m20, m24, m28) so that
/// x' = x * m1c + y * m24 and y' = x * m20 + y * m28.
fn matrix(r: Option<[f32; 3]>) -> (f32, f32, f32, f32) {
    match r {
        Some([rx, ry, rz]) => {
            let (s1, c1, s2, c2, s3, c3) = (rx.sin(), rx.cos(), ry.sin(), ry.cos(), rz.sin(), rz.cos());
            (c2 * c3, c2 * -s3, c1 * s3 + s1 * s2 * c3, c1 * c3 + s1 * s2 * -s3)
        }
        None => (1.0, 0.0, 0.0, 1.0),
    }
}

fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(rd32(b, o))
}

fn i32_at(b: &[u8], o: usize) -> i32 {
    rd32(b, o) as i32
}

fn col(b: &[u8], o: usize) -> [u8; 4] {
    [b.get(o).copied().unwrap_or(0), b.get(o + 1).copied().unwrap_or(0), b.get(o + 2).copied().unwrap_or(0), b.get(o + 3).copied().unwrap_or(0)]
}

pub fn is_seqd(b: &[u8]) -> bool {
    b.starts_with(b"SEQD")
}

pub fn parse(b: &[u8]) -> Option<Seqd> {
    if !is_seqd(b) || b.len() < 0x30 {
        return None;
    }
    let sec = |i: usize| (rd32(b, 8 + 8 * i) as usize, rd32(b, 12 + 8 * i) as usize);
    let (n_sp, o_sp) = sec(0);
    let (n_pt, o_pt) = sec(1);
    let (n_sg, o_sg) = sec(2);
    let (n_an, o_an) = sec(3);
    let (n_ag, o_ag) = sec(4);
    let need = |n: usize, o: usize, size: usize| o.checked_add(n.checked_mul(size)?).filter(|&e| e <= b.len());
    need(n_sp, o_sp, 0x2c)?;
    need(n_pt, o_pt, 0x14)?;
    need(n_sg, o_sg, 4)?;
    need(n_an, o_an, 0x90)?;
    need(n_ag, o_ag, 0x24)?;
    let sprites = (0..n_sp)
        .map(|i| {
            let o = o_sp + i * 0x2c;
            Sprite {
                palette: i32_at(b, o),
                u0: i32_at(b, o + 4),
                v0: i32_at(b, o + 8),
                u1: i32_at(b, o + 12),
                v1: i32_at(b, o + 16),
                u_scroll: f32_at(b, o + 20),
                v_scroll: f32_at(b, o + 24),
                colors: [col(b, o + 28), col(b, o + 32), col(b, o + 36), col(b, o + 40)],
            }
        })
        .collect();
    let parts = (0..n_pt)
        .map(|i| {
            let o = o_pt + i * 0x14;
            Part { left: i32_at(b, o), top: i32_at(b, o + 4), right: i32_at(b, o + 8), bottom: i32_at(b, o + 12), sprite: rd32(b, o + 16) as usize }
        })
        .collect();
    let sprite_groups = (0..n_sg).map(|i| SpriteGroup { start: rd16(b, o_sg + i * 4) as usize, count: rd16(b, o_sg + i * 4 + 2) as usize }).collect();
    let pair = |o: usize| [f32_at(b, o), f32_at(b, o + 4)];
    let ipair = |o: usize| [i32_at(b, o), i32_at(b, o + 4)];
    let anims = (0..n_an)
        .map(|i| {
            let o = o_an + i * 0x90;
            Anim {
                flags: rd32(b, o),
                group: rd32(b, o + 4) as usize,
                frame_start: i32_at(b, o + 8),
                frame_end: i32_at(b, o + 12),
                translate_x: ipair(o + 16),
                translate_y: ipair(o + 24),
                pivot_x: ipair(o + 32),
                pivot_y: ipair(o + 40),
                rot_x: pair(o + 48),
                rot_y: pair(o + 56),
                rot_z: pair(o + 64),
                scale: pair(o + 72),
                scale_x: pair(o + 80),
                scale_y: pair(o + 88),
                curve_start: f32_at(b, o + 100),
                curve_end: f32_at(b, o + 108),
                bounce_x: pair(o + 112),
                bounce_y: pair(o + 120),
                bounce_x_count: rd16(b, o + 128),
                bounce_y_count: rd16(b, o + 130),
                blend: rd32(b, o + 132),
                color_start: col(b, o + 136),
                color_end: col(b, o + 140),
            }
        })
        .collect();
    let groups = (0..n_ag)
        .map(|i| {
            let o = o_ag + i * 0x24;
            AnimGroup {
                first: rd16(b, o) as usize,
                count: rd16(b, o + 2) as usize,
                loops: rd16(b, o + 4),
                flags: rd16(b, o + 6),
                loop_start: i32_at(b, o + 8),
                frame_end: i32_at(b, o + 12),
                x: i32_at(b, o + 16),
                y: i32_at(b, o + 20),
                size: i32_at(b, o + 24),
                unk1c: i32_at(b, o + 28),
                unk20: i32_at(b, o + 32),
            }
        })
        .collect();
    Some(Seqd { sprites, parts, sprite_groups, anims, groups })
}

fn lerp(p: [f32; 2], t: f32) -> f32 {
    p[0] + (p[1] - p[0]) * t
}

/// Integer lerp the way the game does it: start + (int)((end - start) * t).
fn ilerp(p: [i32; 2], t: f32) -> i32 {
    p[0] + ((p[1] - p[0]) as f32 * t) as i32
}

fn blend_color(a: [u8; 4], b: [u8; 4], t: f32) -> [u8; 4] {
    std::array::from_fn(|i| (a[i] as i32 + ((b[i] as i32 - a[i] as i32) as f32 * t) as i32).clamp(0, 255) as u8)
}

/// Per-channel (a * b) >> 7, clamped; 0x80 is the identity.
fn modulate(c: [u8; 4], m: [u8; 4]) -> [u8; 4] {
    std::array::from_fn(|i| ((c[i] as u32 * m[i] as u32) >> 7).min(255) as u8)
}

/// Frame playback of one animation group (the game's sequence player): `set_group` restarts it,
/// `tick` advances it by one logic step.
#[derive(Debug, Clone, Default)]
pub struct Player {
    pub group: Option<usize>,
    /// Frame counter in logic steps (fractional when the speed is not 1).
    time: f32,
    /// 0 = idle, 1 = just set, 2 = playing, 3 = finished (holds the last frame).
    state: u8,
    plays_left: u16,
    loop_start: f32,
    /// One past the last frame.
    end: f32,
    /// Steps since the player was created; drives UV scrolling.
    pub uv_time: f32,
}

impl Player {
    pub fn set_group(&mut self, seq: &Seqd, group: usize) {
        let Some(g) = seq.groups.get(group) else { return };
        self.group = Some(group);
        self.plays_left = g.loops;
        self.loop_start = g.loop_start as f32;
        self.end = (seq.last_frame(group) + 1) as f32;
        self.time = 0.0;
        self.state = 1;
    }

    /// Advances by `step` logic frames (1.0 = one 60 Hz tick at normal speed).
    pub fn tick(&mut self, step: f32) {
        self.uv_time += step;
        match self.state {
            1 => {
                self.time = 0.0;
                self.state = 2;
            }
            2 => {
                self.time += step;
                if self.time >= self.end {
                    self.time = self.loop_start;
                    if self.plays_left != 0 {
                        self.plays_left -= 1;
                        if self.plays_left == 0 {
                            self.state = 3;
                            self.time = self.end - 1.0;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Jumps to `frame` of the current group and keeps playing from there (a harness aid).
    pub fn seek(&mut self, frame: f32) {
        if self.group.is_some() {
            self.time = frame.clamp(0.0, (self.end - 1.0).max(0.0));
            self.state = 2;
        }
    }

    /// The integer frame the group is evaluated at (the game truncates its float counter).
    pub fn frame(&self) -> i32 {
        self.time as i32
    }

    pub fn finished(&self) -> bool {
        self.state == 3
    }
}

impl Seqd {
    /// Where the group's anchor animation (the first one flagged `0x2` that covers `frame`) puts
    /// its origin: translation, bounce and the rotated, scaled pivot. Child gauges are placed
    /// relative to this point. `None` when no flagged animation covers the frame.
    pub fn anchor(&self, group: usize, frame: i32, ext: &Ext) -> Option<[i32; 2]> {
        let g = self.groups.get(group)?;
        for a in self.anims.iter().skip(g.first).take(g.count) {
            if a.flags & 2 == 0 || frame < a.frame_start || frame > a.frame_end {
                continue;
            }
            let span = (a.frame_end - a.frame_start) as f32;
            let mut t = if span != 0.0 { (frame - a.frame_start) as f32 / span } else { 1.0 };
            if a.flags & flag::LINEAR == 0 {
                let u = 1.0 - t;
                t = 1.0 - (u * 3.0 * t * t * (1.0 - a.curve_end) + u * u * 3.0 * t * (1.0 - a.curve_start) + u * u * u);
            }
            let mut tx = ilerp(a.translate_x, t);
            let mut ty = ilerp(a.translate_y, t);
            if a.flags & flag::NO_BOUNCE == 0 {
                let f = if a.flags & flag::BOUNCE_COS == 0 { f32::sin } else { f32::cos };
                let amp_x = a.bounce_x[0] + (a.bounce_x[1] - a.bounce_x[0]) * t;
                let amp_y = lerp(a.bounce_y, t);
                tx += (amp_x * f(t * std::f32::consts::PI * a.bounce_x_count as f32)) as i32;
                ty += (amp_y * f(t * std::f32::consts::PI * a.bounce_y_count as f32)) as i32;
            }
            let (sx, sy) = if a.flags & flag::NO_SCALE == 0 { (lerp(a.scale_x, t), lerp(a.scale_y, t)) } else { (1.0, 1.0) };
            let (px, py) = (ilerp(a.pivot_x, t) as f32 * sx, ilerp(a.pivot_y, t) as f32 * sy);
            let (m1c, m20, m24, m28) = matrix(rotation(a, t, ext));
            let (ox, oy) = (px * m1c + py * m24, px * m20 + py * m28);
            return Some([tx + ox as i32, ty + oy as i32]);
        }
        None
    }

    /// Number of frames the animation group plays (the game's group length), counting frame 0.
    pub fn group_len(&self, group: usize) -> i32 {
        let Some(g) = self.groups.get(group) else { return 0 };
        let mut len = 0;
        for a in self.anims.iter().skip(g.first).take(g.count) {
            if len < a.frame_end {
                len = a.frame_end;
                if a.flags & flag::EXCLUSIVE_END != 0 {
                    len = a.frame_end - 1;
                }
            }
        }
        len + 1
    }

    /// Last playable frame: the group length minus one, capped by the group's explicit end frame.
    pub fn last_frame(&self, group: usize) -> i32 {
        let end = self.group_len(group) - 1;
        match self.groups.get(group) {
            Some(g) if g.frame_end != 0 && g.frame_end < end => g.frame_end,
            _ => end,
        }
    }

    /// The rectangles drawn by `group` at integer `frame`, placed at `origin` (x, y).
    pub fn quads(&self, group: usize, frame: i32, origin: [f32; 2], ext: &Ext) -> Vec<Quad> {
        let Some(g) = self.groups.get(group) else { return Vec::new() };
        let mut out = Vec::new();
        for a in self.anims.iter().skip(g.first).take(g.count) {
            if frame < a.frame_start || frame > a.frame_end || (frame == a.frame_end && a.flags & flag::EXCLUSIVE_END != 0) {
                continue;
            }
            self.anim_quads(a, frame, origin, ext, &mut out);
        }
        out
    }

    fn anim_quads(&self, a: &Anim, frame: i32, origin: [f32; 2], ext: &Ext, out: &mut Vec<Quad>) {
        let span = (a.frame_end - a.frame_start) as f32;
        let mut t = if span != 0.0 { (frame - a.frame_start) as f32 / span } else { 1.0 };
        if a.flags & flag::LINEAR == 0 {
            let u = 1.0 - t;
            t = 1.0 - (u * 3.0 * t * t * (1.0 - a.curve_end) + u * u * 3.0 * t * (1.0 - a.curve_start) + u * u * u);
        }
        let mut tx = ilerp(a.translate_x, t);
        let mut ty = ilerp(a.translate_y, t);
        if a.flags & flag::NO_BOUNCE == 0 {
            let f = if a.flags & flag::BOUNCE_COS == 0 { f32::sin } else { f32::cos };
            let amp_x = a.bounce_x[0] + (a.bounce_x[1] - a.bounce_x[0]) * t;
            let amp_y = lerp(a.bounce_y, t);
            let wx = f(t * std::f32::consts::PI * a.bounce_x_count as f32);
            let wy = f(t * std::f32::consts::PI * a.bounce_y_count as f32);
            tx += (amp_x * wx) as i32;
            ty += (amp_y * wy) as i32;
        }
        let (px, py) = if a.flags & flag::NO_PIVOT == 0 { (ilerp(a.pivot_x, t), ilerp(a.pivot_y, t)) } else { (0, 0) };
        let (sx, sy) = if a.flags & flag::NO_SCALE == 0 {
            let s = lerp(a.scale, t);
            (s * lerp(a.scale_x, t) * ext.scale[0] * ext.scale[1], s * lerp(a.scale_y, t) * ext.scale[0] * ext.scale[2])
        } else {
            (ext.scale[0] * ext.scale[1], ext.scale[0] * ext.scale[2])
        };
        let (m1c, m20, m24, m28) = matrix(rotation(a, t, ext));
        let modc = if a.flags & flag::NO_COLOR != 0 {
            None
        } else {
            let mut c = if a.flags & flag::COLOR_STEP != 0 { a.color_start } else { blend_color(a.color_start, a.color_end, t) };
            for (v, m) in c.iter_mut().zip(ext.color) {
                *v = (*v as f32 * m).round().clamp(0.0, 255.0) as u8;
            }
            Some(c)
        };
        let Some(sg) = self.sprite_groups.get(a.group) else { return };
        for p in self.parts.iter().skip(sg.start).take(sg.count) {
            let Some(sp) = self.sprites.get(p.sprite) else { continue };
            let corner = |x: i32, y: i32| {
                let (x, y) = ((x + px) as f32 * sx, (y + py) as f32 * sy);
                [origin[0] + tx as f32 + x * m1c + y * m24, origin[1] + ty as f32 + x * m20 + y * m28]
            };
            let pos = [corner(p.left, p.top), corner(p.right, p.top), corner(p.left, p.bottom), corner(p.right, p.bottom)];
            let (mut du, mut dv) = (0.0, 0.0);
            if sp.u_scroll != 0.0 || sp.v_scroll != 0.0 {
                // The game scrolls UVs by speed x time x image size in 1/16 texel units, wrapped at 512 texels.
                du = (((sp.u_scroll * ext.time * 256.0 * 16.0) as i32) & 0x1fff) as f32 / 16.0;
                dv = (((sp.v_scroll * ext.time * 256.0 * 16.0) as i32) & 0x1fff) as f32 / 16.0;
            }
            // A zero-width (or -height) sprite stretches one texel row or column: the game's +0.5
            // texel offset puts the sample in the middle of it.
            let (u0, u1) = if sp.u0 == sp.u1 { (sp.u0 as f32 + 0.5, sp.u1 as f32 + 0.5) } else { (sp.u0 as f32, sp.u1 as f32) };
            let (v0, v1) = if sp.v0 == sp.v1 { (sp.v0 as f32 + 0.5, sp.v1 as f32 + 0.5) } else { (sp.v0 as f32, sp.v1 as f32) };
            let (u0, u1, v0, v1) = (u0 + du, u1 + du, v0 + dv, v1 + dv);
            let mut color = sp.colors;
            if let Some(m) = modc {
                for c in color.iter_mut() {
                    *c = modulate(*c, m);
                }
            }
            if ext.fade != 1.0 {
                for c in color.iter_mut() {
                    c[3] = ((c[3] as f32 * ext.fade) as u32).min(255) as u8;
                }
            }
            out.push(Quad { pos, uv: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]], color, blend: a.blend, nearest: a.flags & flag::NEAREST != 0 });
        }
    }
}
