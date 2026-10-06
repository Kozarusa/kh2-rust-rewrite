//! DPD shapes: animated sprite sets. A 0x10-byte packet header (number, texture/CLUT base,
//! packet size), an animation header (frame count) and frames ("sequences": shape offset, wait,
//! flag 0x80 = loop at the end). Each frame is a VU1 sprite list: a 0x20-byte header (flags, sprite
//! count, TEX0), the sprites (axis-aligned rectangles, or free quads when flag 0x08 is set), then one
//! color/blend record per sprite.
//!
//! Layout per OpenKH `OpenKh.Kh2/Dpd.Shape.cs` (Apache-2.0). Positions are 12.4 fixed point.
//! Texture coordinates are fractions of the texture in 1/4096 (4.12 fixed point): the shape draw
//! FUN_0021a660 converts them with `vitof12` and hands them to the GS as normalised ST floats (the
//! packet's PRIM 0x56 has FST clear), so they do not depend on the texture's size. Byte 7 of a
//! sprite's color record is added to the CBP field of the frame's TEX0 (a per-sprite palette).

use crate::rd::*;

#[derive(Clone, Debug)]
pub struct Shape {
    pub number: i16,
    pub frames: Vec<Frame>,
    /// Frame count the animation runs through (the header's sequence count).
    pub seq_count: usize,
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub wait: i16,
    pub flag: u8,
    pub tex0: u64,
    pub sprites: Vec<Sprite>,
}

#[derive(Clone, Copy, Debug)]
pub struct Sprite {
    /// Corners: top-left, top-right, bottom-left, bottom-right (local units).
    pub corners: [[f32; 2]; 4],
    /// The raw texture rectangle divided by 16 (s, t, width, height), the unit this parser used
    /// before WP-FIX-9; kept for its readers. The game's reading is [`Sprite::st`].
    pub uv: [f32; 4],
    /// Texture rectangle as fractions of the texture: s, t, width, height (raw / 4096), the
    /// normalised ST FUN_0021a660 hands to the GS.
    pub st: [f32; 4],
    pub rgba: [u8; 4],
    /// GS ALPHA register value (0x48 additive, 0x44 blend).
    pub alpha: u8,
    pub alpha_fix: u8,
    /// Added to the CBP field (bits 37..50) of the frame's TEX0 for this sprite (color record
    /// byte 7; FUN_0021a660 adds it with `paddw` before the TEX0 write).
    pub clut_add: u8,
}

impl Sprite {
    /// The TEX0 value the sprite is drawn with: the frame's, with the sprite's palette offset.
    pub fn tex0(&self, frame_tex0: u64) -> u64 {
        frame_tex0.wrapping_add((self.clut_add as u64) << 37)
    }
}

/// One raw texture coordinate (4.12 fixed point) as a fraction of the texture.
pub fn st(raw: i16) -> f32 {
    raw as f32 / 4096.0
}

impl Shape {
    pub fn parse(b: &[u8]) -> Shape {
        let a = 0x10;
        let count = i16_at(b, a + 4).max(0) as usize;
        let seq_count = i16_at(b, a + 6).max(0) as usize;
        let frames = (0..count)
            .map(|k| {
                let e = a + 0x10 + k * 8;
                let v = a + i16_at(b, e).max(0) as usize;
                let vflag = i16_at(b, v);
                let n = i16_at(b, v + 2).max(0) as usize;
                let tex0 = u64_at(b, v + 0x10);
                let quad = vflag & 8 != 0;
                let size = if quad { 24 } else { 16 };
                let s0 = v + 0x20;
                let c0 = s0 + n * size;
                let sprites = (0..n)
                    .map(|j| {
                        let s = s0 + j * size;
                        let h = |k: usize| i16_at(b, s + 2 * k) as f32 / 16.0;
                        let (corners, uv_at) = if quad {
                            ([[h(0), h(1)], [h(2), h(3)], [h(4), h(5)], [h(6), h(7)]], 8)
                        } else {
                            ([[h(0), h(1)], [h(2), h(1)], [h(0), h(3)], [h(2), h(3)]], 4)
                        };
                        let t = |k: usize| st(i16_at(b, s + 2 * (uv_at + k)));
                        let c = c0 + j * 8;
                        Sprite {
                            corners,
                            uv: [h(uv_at), h(uv_at + 1), h(uv_at + 2), h(uv_at + 3)],
                            st: [t(0), t(1), t(2), t(3)],
                            rgba: [u8_at(b, c), u8_at(b, c + 1), u8_at(b, c + 2), u8_at(b, c + 3)],
                            alpha: u8_at(b, c + 4),
                            alpha_fix: u8_at(b, c + 5),
                            clut_add: u8_at(b, c + 7),
                        }
                    })
                    .collect();
                Frame { wait: i16_at(b, e + 2), flag: u8_at(b, e + 4), tex0, sprites }
            })
            .collect();
        Shape { number: i16_at(b, 0), frames, seq_count }
    }
}

/// Sprite-animation state as the game keeps it in work memory: timer, next frame, shown frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Anim {
    pub timer: u16,
    pub next: u16,
    pub shown: u16,
}

impl Anim {
    /// One tick at `speed` (the game's step; frames end when the timer reaches their wait).
    pub fn step(&mut self, shape: &Shape, speed: i16) {
        let cur = self.next;
        let shown = self.shown as usize;
        self.shown = cur;
        let Some(f) = shape.frames.get(shown) else { return };
        let t = self.timer.wrapping_add(speed as u16);
        self.timer = t;
        if f.wait as i32 <= t as i32 {
            self.timer = t.wrapping_sub(f.wait as u16);
            self.next = cur + 1;
            if shape.seq_count as i64 <= (cur + 1) as i64 {
                if f.flag & 0x80 != 0 {
                    self.next = 0;
                    self.timer = 0;
                } else {
                    self.next = cur;
                    self.timer = 0;
                }
            }
        }
    }
}
