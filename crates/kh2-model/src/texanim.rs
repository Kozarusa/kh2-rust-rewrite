//! Texture animation: the footer of a model texture container (BAR type 7) and the game's
//! players for it. Footer layout per OpenKH `docs/kh2/file/raw-texture.md` (Apache-2.0), checked
//! against the executable's readers; the runtime is a port of the game's own routines:
//!
//! * The footer starts right after the palette (`palette offset + 4 × colour count`,
//!   FUN_00132238) and is a run of tagged blocks `tag, u32 size, body`: `TEXA` (sprite animation
//!   of one texture region), `UVSC` (UV scroll speeds), `_DMY` (padding) and `_KN5` (end).
//!   FUN_00132438 accepts exactly these four tags; FUN_001323b0 walks them; FUN_0011e208 makes
//!   one player object per `TEXA` (0x290 bytes, vtable 0x354da0) and per `UVSC` (0x2c bytes,
//!   vtable 0x354dd0) when a model's textures are set up.
//! * Both players advance by the field frame delta `DAT_00356f40` (1.0 per 60 Hz field,
//!   FUN_0014cbf8 / FUN_0014cdd0) times their own speed (+0x18, initialised to 1.0 from
//!   `DAT_00378cb0` by FUN_001324a0): FUN_0011e7f8 calls each object's update (vtable +0x14).
//! * TEXA (FUN_00132950 start, FUN_00132a80 update, FUN_00132fc0 current sprite): a frame table
//!   of `ctrl, min, max, sprite…` entries; `ctrl & 15` is 0 show sprite, 1 show none, 2 jump by
//!   `ctrl >> 4` entries, 3 stop. A frame lasts a random whole number of fields in `[min, max]`.
//!   While a sprite shows, the draw code re-uploads the base image and then the sprite's pixels
//!   at `(x, y)` in the texture (FUN_0011f560 maps, FUN_00121a78 models, upload FUN_001095f0).
//! * UVSC (FUN_00133518 update, FUN_00133628 / FUN_001336d8 offsets): two integer accumulators
//!   grow by `speed × dt` each field; the draw code turns them into a 12-bit texture-coordinate
//!   offset for every mesh whose flags name that UVSC entry.

use crate::texture::{self, i32_at};

const TEXA: &[u8; 4] = b"TEXA";
const UVSC: &[u8; 4] = b"UVSC";
const KN5: &[u8; 4] = b"_KN5";
const DMY: &[u8; 4] = b"_DMY";

fn u16_at(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map_or(0, |s| u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap()))
}

/// One sprite animation (`TEXA` block) of a texture container.
#[derive(Debug, Clone)]
pub struct Texa {
    /// GS-info (texture) index whose image the sprites are copied into (body +2).
    pub texture: usize,
    /// Destination rectangle in that texture's pixels (body +0x10 dsax, +0x12 dsay, +0x14, +0x16).
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Every sprite image as RGBA8 (`width × height`), through the target texture's CLUT.
    pub sprites: Vec<Vec<u8>>,
    /// The game uploads the sprite at (x, y) of the GS buffer at the texture's transfer DBP with
    /// the TEX0 buffer width (FUN_001095f0). That is (x, y) of the decoded image when the texture
    /// starts at its transfer (TBP0 == DBP) and the buffer is as wide as the PSM's page rows the
    /// decoder assumes; false means the region would land elsewhere and is not applied.
    pub in_place: bool,
    /// The block's body (after tag and size); every offset in it is relative to its start.
    body: Vec<u8>,
}

/// One UV scroll entry (`UVSC` block): the UV-scroll index meshes refer to and its speeds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Uvsc {
    pub index: u32,
    pub speed_u: f32,
    pub speed_v: f32,
}

/// Everything in a texture container's footer.
#[derive(Debug, Clone, Default)]
pub struct Animations {
    pub texa: Vec<Texa>,
    pub uvsc: Vec<Uvsc>,
}

/// One entry of a TEXA frame table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub ctrl: u16,
    pub min: u16,
    pub max: u16,
    pub sprite: u16,
}

impl Frame {
    pub fn code(&self) -> u16 {
        self.ctrl & 15
    }
    /// Jump distance in entries (`ctrl >> 4`, sign-extended).
    pub fn jump(&self) -> i32 {
        (self.ctrl as i16 >> 4) as i32
    }
}

/// Byte offset of the footer in a texture container (FUN_00132238), if it has one.
pub fn footer_offset(b: &[u8]) -> Option<usize> {
    if b.len() < 36 || i32_at(b, 0) != 0 {
        return None;
    }
    let o = i32_at(b, 32) + 4 * i32_at(b, 4);
    (o + 4 <= b.len()).then_some(o)
}

/// Parses the footer of a texture container (the BAR type 7 data). Containers without one
/// (or with a first tag the game does not know) have no animations.
pub fn parse(b: &[u8]) -> Animations {
    let mut out = Animations::default();
    let Some(mut o) = footer_offset(b) else { return out };
    // FUN_001323b0 / FUN_00132360: stop at _KN5 or at any tag other than the four known ones.
    while let Some(tag) = b.get(o..o + 4) {
        if tag == KN5 || !(tag == TEXA || tag == UVSC || tag == DMY) {
            break;
        }
        let size = u32_at(b, o + 4) as usize;
        let body = b.get(o + 8..(o + 8 + size).min(b.len())).unwrap_or(&[]);
        if tag == TEXA {
            if let Some(t) = Texa::parse(b, body) {
                out.texa.push(t);
            }
        } else if tag == UVSC {
            out.uvsc.push(Uvsc { index: u32_at(body, 0), speed_u: f32::from_bits(u32_at(body, 4)), speed_v: f32::from_bits(u32_at(body, 8)) });
        }
        o += 8 + size;
    }
    out
}

impl Texa {
    fn parse(container: &[u8], body: &[u8]) -> Option<Texa> {
        let texture = u16_at(body, 2) as usize;
        let g = texture::gs_info(container, texture)?;
        let (width, height) = (u16_at(body, 0x14) as usize, u16_at(body, 0x16) as usize);
        let bpp = u16_at(body, 6) as usize;
        let size = (width * bpp >> 3) * height;
        let base = u32_at(body, 0x20) as usize;
        let sprites = (0..u16_at(body, 0xe) as usize)
            .map(|k| {
                let src = body.get(base + k * size..).unwrap_or(&[]);
                texture::pixels_to_rgba(container, &g, src, width, height)
            })
            .collect();
        let rows = match g.psm {
            0x13 | 0x14 => g.tbw.div_ceil(2) == g.width.div_ceil(128),
            _ => g.tbw * 64 == g.width,
        };
        Some(Texa {
            texture,
            in_place: g.tbp0 == g.dbp && rows,
            x: u16_at(body, 0x10) as u32,
            y: u16_at(body, 0x12) as u32,
            width: width as u32,
            height: height as u32,
            sprites,
            body: body.to_vec(),
        })
    }

    /// Bits per sprite pixel (body +6).
    pub fn bpp(&self) -> u16 {
        u16_at(&self.body, 6)
    }
    /// Number of animations (body +0xc).
    pub fn anim_count(&self) -> u16 {
        u16_at(&self.body, 0xc)
    }
    /// Bytes per frame-table entry: 8 plus the extra halfwords of body +4 (FUN_00132a80).
    pub fn entry_size(&self) -> usize {
        u16_at(&self.body, 4) as usize * 2 + 8
    }

    /// Animation index for a request (FUN_001327f8): with `mask` 0, `value` is the index itself;
    /// otherwise `value & mask` is a slot looked up in the slot table (body +8 first slot, +0xa
    /// last slot, table at body +0x18, one u16 per slot holding index + 1, 0 = none).
    pub fn anim_index(&self, mask: u32, value: u32) -> Option<usize> {
        let b = &self.body;
        let count = u16_at(b, 0xc) as u32;
        if mask == 0 {
            return (value < count).then_some(value as usize);
        }
        let v = value & mask;
        let (lo, hi) = (u16_at(b, 8) as u32, u16_at(b, 0xa) as u32);
        if v < lo || v > hi {
            return None;
        }
        let e = u16_at(b, (v - lo) as usize * 2 + u32_at(b, 0x18) as usize) as u32;
        (e != 0 && e <= count).then(|| (e - 1) as usize)
    }

    /// Byte offset (in the body) of animation `index`'s first frame (animation table at +0x1c).
    pub fn anim_start(&self, index: usize) -> Option<usize> {
        let o = u32_at(&self.body, u32_at(&self.body, 0x1c) as usize + 4 * index) as usize;
        (o + 8 <= self.body.len()).then_some(o)
    }

    /// The animation that plays unless the game asks for another (body +0x24, FUN_001328a8).
    pub fn default_anim(&self) -> Option<usize> {
        self.anim_index(0, u32_at(&self.body, 0x24))
    }

    /// The frame entry at byte offset `at`.
    pub fn frame(&self, at: usize) -> Option<Frame> {
        let b = &self.body;
        (at + 8 <= b.len()).then(|| Frame { ctrl: u16_at(b, at), min: u16_at(b, at + 2), max: u16_at(b, at + 4), sprite: u16_at(b, at + 6) })
    }

    /// The frames of animation `index` in play order up to its first jump or stop (inclusive).
    pub fn frames(&self, index: usize) -> Vec<Frame> {
        let mut out = Vec::new();
        let mut at = self.anim_start(index);
        while let Some(f) = at.and_then(|a| self.frame(a)) {
            out.push(f);
            if f.code() >= 2 || out.len() >= 4096 {
                break;
            }
            at = at.map(|a| a + self.entry_size());
        }
        out
    }
}

/// The game's random frame length: `min + rand_n(max + 1 - min)` (FUN_00106380 → FUN_00106308:
/// `n × rand() / 2147483520.0` as an unsigned integer). `rand` yields 31-bit values like the
/// game's `rand()` (FUN_003117a8).
pub fn frame_length(min: u16, max: u16, rand: &mut impl FnMut() -> u32) -> f32 {
    let n = (max as i32 + 1) - min as i32;
    let n = if n < 0 { n as u32 as f32 } else { n as f32 };
    let r = fixuns(n * rand() as f32 / 2_147_483_520.0);
    (min as u32).wrapping_add(r) as f32
}

/// `__fixunssfsi` (FUN_00326940): negative and NaN give 0, too large gives all ones, else truncation.
pub fn fixuns(f: f32) -> u32 {
    if f.is_nan() || f <= 0.0 {
        0
    } else if f >= 4_294_967_296.0 {
        u32::MAX
    } else {
        f as u32
    }
}

/// A TEXA player: the fields of the game's 0x290-byte object that its update uses.
#[derive(Debug, Clone, Default)]
pub struct TexaPlayer {
    /// +0x14: 1 = just started (skip one advance), 2 = stop at the next jump, 4 = `request` was
    /// set by a slot request this field (FUN_00133058) instead of the default.
    pub flags: u32,
    /// +0x274: the animation to play (its first frame's offset); the default animation at setup.
    pub request: Option<usize>,
    /// +0x278: the playing animation's first frame.
    pub anim: Option<usize>,
    /// +0x27c: the current frame entry (None = nothing shows).
    pub frame: Option<usize>,
    /// +0x280: current frame length, +0x284: time in it (fields).
    pub length: f32,
    pub time: f32,
    /// +0x18: speed scale (1.0).
    pub speed: f32,
}

impl TexaPlayer {
    /// A player set up like FUN_0011e208 + FUN_001328a8: default animation requested, nothing started.
    pub fn new(t: &Texa) -> TexaPlayer {
        TexaPlayer { request: t.default_anim().and_then(|i| t.anim_start(i)), speed: 1.0, ..Default::default() }
    }

    /// FUN_00132950: start `anim` unless it already plays.
    pub fn start(&mut self, t: &Texa, anim: Option<usize>, rand: &mut impl FnMut() -> u32) {
        if self.anim == anim {
            return;
        }
        match anim.and_then(|a| t.frame(a).map(|f| (a, f))) {
            Some((a, f)) => {
                self.flags |= 1;
                self.length = frame_length(f.min, f.max, rand);
                self.time = 0.0;
                self.anim = Some(a);
            }
            None => self.anim = None,
        }
        self.frame = anim;
    }

    /// FUN_00133058: play the animation a slot names (mask 0xffff), overriding the default this field.
    pub fn play_slot(&mut self, t: &Texa, slot: u32, rand: &mut impl FnMut() -> u32) {
        if let Some(a) = t.anim_index(0xffff, slot).and_then(|i| t.anim_start(i)) {
            self.flags |= 4;
            self.start(t, Some(a), rand);
        }
    }

    /// FUN_00132a80 (an unlinked object): one field update by `dt` fields.
    pub fn update(&mut self, t: &Texa, dt: f32, rand: &mut impl FnMut() -> u32) {
        if self.flags & 4 == 0 {
            let r = self.request;
            self.start(t, r, rand);
        } else {
            self.flags &= !4;
        }
        let step = t.entry_size() as i64;
        let Some(mut at) = self.frame else { return };
        let Some(f) = t.frame(at) else {
            self.frame = None;
            return;
        };
        if f.code() == 3 {
            return;
        }
        if f.code() == 2 {
            at = (at as i64 + f.jump() as i64 * step) as usize;
            self.frame = Some(at);
        }
        if self.flags & 1 != 0 {
            self.flags &= !1;
            return;
        }
        self.time += dt * self.speed;
        while self.length <= self.time {
            self.time -= self.length;
            at += step as usize;
            self.frame = Some(at);
            let Some(f) = t.frame(at) else {
                self.frame = None;
                return;
            };
            match f.code() {
                2 => {
                    if self.flags & 2 != 0 {
                        self.frame = None;
                        return;
                    }
                    at = (at as i64 + f.jump() as i64 * step) as usize;
                    self.frame = Some(at);
                }
                3 => return,
                _ => {}
            }
            let Some(f) = t.frame(at) else {
                self.frame = None;
                return;
            };
            self.length = frame_length(f.min, f.max, rand);
        }
    }

    /// FUN_00132fc0: the sprite to show now (the current entry's code is 0), if any. A sprite
    /// index beyond the TEXA's sprite count shows nothing (the game would read past the data).
    pub fn sprite(&self, t: &Texa) -> Option<usize> {
        let f = t.frame(self.frame?)?;
        (f.code() == 0 && (f.sprite as usize) < t.sprites.len()).then_some(f.sprite as usize)
    }
}

/// A UVSC player: the fields of the game's 0x2c-byte object that its update uses.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UvscPlayer {
    /// +0x14: 1 = skip one advance, 2 = stopped.
    pub flags: u32,
    /// +0x24 / +0x28: the U and V accumulators.
    pub acc_u: i32,
    pub acc_v: i32,
    /// +0xc: time run (fields).
    pub time: f32,
}

impl UvscPlayer {
    /// FUN_00133518: one field update by `dt` fields (speed scale +0x18 is 1.0).
    pub fn update(&mut self, s: &Uvsc, dt: f32) {
        if self.flags & 2 != 0 {
            return;
        }
        if self.flags & 1 != 0 {
            self.flags &= !1;
            return;
        }
        let d = dt * 1.0;
        self.acc_u = self.acc_u.wrapping_add((s.speed_u * d) as i32);
        self.time += d;
        self.acc_v = self.acc_v.wrapping_add((s.speed_v * d) as i32);
    }

    /// FUN_00133628 / FUN_001336d8: the 12-bit texture-coordinate offset (4096 = one texture
    /// width) for an accumulator, a CLAMP span (`MINU + 1` or `MINV + 1`) and the texture size.
    pub fn offset(acc: i32, span: u32, size: u32) -> u32 {
        let k = span as f32 / size as f32;
        (fixuns(acc as u32 as f32 * k) >> 16) & 0xfff
    }

    /// The U and V offsets as fractions of the texture (0..1), for a texture with `uv_span`.
    pub fn uv(&self, span: [u32; 2], width: u32, height: u32) -> [f32; 2] {
        [Self::offset(self.acc_u, span[0], width) as f32 / 4096.0, Self::offset(self.acc_v, span[1], height) as f32 / 4096.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A TEXA body with one animation: show sprite 0 for 2 fields, sprite 1 for 3, jump back 2.
    fn toy() -> Texa {
        let mut b = vec![0u8; 0x60];
        let w16 = |b: &mut Vec<u8>, o: usize, v: u16| b[o..o + 2].copy_from_slice(&v.to_le_bytes());
        let w32 = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        w16(&mut b, 0xc, 1);
        w16(&mut b, 0xe, 2);
        w32(&mut b, 0x1c, 0x28);
        w32(&mut b, 0x28, 0x30);
        for (k, (c, mn, mx, s)) in [(0u16, 2u16, 2u16, 0u16), (0, 3, 3, 1), (2 | ((-2i16 as u16) << 4), 0, 0, 0)].iter().enumerate() {
            let o = 0x30 + 8 * k;
            w16(&mut b, o, *c);
            w16(&mut b, o + 2, *mn);
            w16(&mut b, o + 4, *mx);
            w16(&mut b, o + 6, *s);
        }
        Texa { texture: 0, in_place: true, x: 0, y: 0, width: 1, height: 1, sprites: vec![vec![0; 4], vec![0; 4]], body: b }
    }

    #[test]
    fn texa_player_follows_the_frame_table_and_loops() {
        let t = toy();
        let mut rand = || 0u32;
        let mut p = TexaPlayer::new(&t);
        let mut seen = Vec::new();
        for _ in 0..12 {
            p.update(&t, 1.0, &mut rand);
            seen.push(p.sprite(&t));
        }
        // The first field starts the animation without advancing time, so sprite 0 shows for
        // 2 fields in all, sprite 1 for 3, then the jump entry leads straight back to sprite 0.
        let s = |v: usize| Some(v);
        assert_eq!(seen, vec![s(0), s(0), s(1), s(1), s(1), s(0), s(0), s(1), s(1), s(1), s(0), s(0)]);
    }

    #[test]
    fn frame_length_is_uniform_in_min_max() {
        let mut r = || 0u32;
        assert_eq!(frame_length(5, 5, &mut r), 5.0);
        let mut r = || 0x7fff_0000u32;
        assert_eq!(frame_length(5, 9, &mut r), 9.0);
        // The game's float rounding lets the very top of rand()'s range reach max + 1.
        let mut r = || 0x7fff_ffffu32;
        assert_eq!(frame_length(5, 9, &mut r), 10.0);
        let mut r = || 0x4000_0000u32;
        assert_eq!(frame_length(0, 3, &mut r), 2.0);
    }

    #[test]
    fn uv_offset_is_the_accumulator_scaled_by_span_over_size() {
        // span == size: the accumulator's bits 16..27.
        assert_eq!(UvscPlayer::offset(0x0123_4567, 128, 128), 0x123);
        // Negative accumulators read as unsigned (the conversion at 0x133670).
        assert_eq!(UvscPlayer::offset(-65536, 64, 64), 0xfff);
        let s = Uvsc { index: 0, speed_u: 1000.5, speed_v: -10.0 };
        let mut p = UvscPlayer::default();
        p.update(&s, 1.0);
        p.update(&s, 2.0);
        assert_eq!((p.acc_u, p.acc_v, p.time), (1000 + 2001, -30, 3.0));
    }
}
