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
        let size = ((width * bpp) >> 3) * height;
        let base = u32_at(body, 0x20) as usize;
        if width > 1024 || height > 1024 {
            return None;
        }
        let count = body.len().saturating_sub(base).checked_div(size).map_or(0, |fit| (u16_at(body, 0xe) as usize).min(fit));
        let sprites = (0..count)
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
    /// orig 0x1327f8
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
    /// orig 0x1328a8
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
    /// orig 0x132950
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
    /// orig 0x133058
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
    /// orig 0x132fc0
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
    /// orig 0x133628
    /// orig 0x1336d8
    pub fn offset(acc: i32, span: u32, size: u32) -> u32 {
        let k = span as f32 / size as f32;
        (fixuns(acc as u32 as f32 * k) >> 16) & 0xfff
    }

    /// The U and V offsets as fractions of the texture (0..1), for a texture with `uv_span`.
    pub fn uv(&self, span: [u32; 2], width: u32, height: u32) -> [f32; 2] {
        [Self::offset(self.acc_u, span[0], width) as f32 / 4096.0, Self::offset(self.acc_v, span[1], height) as f32 / 4096.0]
    }
}

/// The game's animation objects on their own byte layouts (RULE C7), so that an oracle can run the original and the port on
/// one memory image and compare every byte. Pointers are the game's addresses; the allocator and the random number source
/// are the caller's ([`Host`]). The player types above are the same rules on Rust types for the app.
///
/// Layouts (owner = the model unit's animation owner, object = an animation object of the list at owner +0x38):
/// * owner: +0x10 allocator object, +0x34 the texture container group, +0x38 first animation object.
/// * object: +0 vtable, +4 container it was made from, +8 kind (1 TEXA, 2 UVSC), +0xc run time (UVSC), +0x10 footer block, +0x14
///   flags, +0x18 speed, +0x1c next object, +0x20 linked object, then per kind: TEXA +0x30 four 0x90-byte GS records, +0x270
///   record selector, +0x274 requested frame table, +0x278 playing frame table, +0x27c current frame, +0x280 frame length,
///   +0x284 time in it (0x290 bytes); UVSC +0x24 and +0x28 the U and V offsets (0x2c bytes).
pub mod objects {
    /// The three vtables (base, TEXA, UVSC) and the model unit's static object, as addresses in the executable.
    pub const VT_ANIM: u32 = 0x354d70;
    pub const VT_TEXA: u32 = 0x354da0;
    pub const VT_UVSC: u32 = 0x354dd0;
    pub const MODEL_UNIT: u32 = 0x354df8;
    /// Footer tags as little-endian words: `TEXA`, `UVSC`, `_KN5`, `_DMY`.
    pub const TAG_TEXA: u32 = 0x4158_4554;
    pub const TAG_UVSC: u32 = 0x4353_5655;
    pub const TAG_KN5: u32 = 0x354e_4b5f;
    pub const TAG_DMY: u32 = 0x594d_445f;

    /// A window of the game's memory: `bytes` hold the addresses `base..base + bytes.len()`.
    #[derive(Clone, Debug)]
    pub struct Mem {
        pub base: u32,
        pub bytes: Vec<u8>,
    }

    impl Mem {
        pub fn new(base: u32, size: usize) -> Mem {
            Mem { base, bytes: vec![0; size] }
        }
        fn at(&self, a: u32, n: usize) -> usize {
            let o = a.wrapping_sub(self.base) as usize;
            assert!(o.saturating_add(n) <= self.bytes.len(), "address {a:#x} is outside the image at {:#x}", self.base);
            o
        }
        pub fn r8(&self, a: u32) -> u8 {
            self.bytes[self.at(a, 1)]
        }
        pub fn r16(&self, a: u32) -> u16 {
            let o = self.at(a, 2);
            u16::from_le_bytes([self.bytes[o], self.bytes[o + 1]])
        }
        pub fn r32(&self, a: u32) -> u32 {
            let o = self.at(a, 4);
            u32::from_le_bytes(self.bytes[o..o + 4].try_into().unwrap())
        }
        pub fn rf32(&self, a: u32) -> f32 {
            f32::from_bits(self.r32(a))
        }
        pub fn w8(&mut self, a: u32, v: u8) {
            let o = self.at(a, 1);
            self.bytes[o] = v;
        }
        pub fn w16(&mut self, a: u32, v: u16) {
            let o = self.at(a, 2);
            self.bytes[o..o + 2].copy_from_slice(&v.to_le_bytes());
        }
        pub fn w32(&mut self, a: u32, v: u32) {
            let o = self.at(a, 4);
            self.bytes[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        pub fn wf32(&mut self, a: u32, v: f32) {
            self.w32(a, v.to_bits());
        }
    }

    /// What the animation objects need from the rest of the game: the allocator object the owner holds (its vtable slots 0 and
    /// 1: allocate `size` bytes, free a block) and the random number source (0x106380).
    pub trait Host {
        fn alloc(&mut self, m: &mut Mem, allocator: u32, size: u32) -> u32;
        fn free(&mut self, m: &mut Mem, allocator: u32, block: u32);
        /// 0x106380: `lo + random(hi - lo)`, the frame length in fields.
        fn rand_between(&mut self, m: &mut Mem, lo: u32, hi: u32) -> i32;
    }

    /// The conversion the game applies to the result of 0x106380 before it stores a frame length.
    fn length_of(v: i32) -> f32 {
        if v < 0 { (v as u32) as f32 } else { v as f32 }
    }

    // ---- container and footer walk ----------------------------------------------------------------------------------

    /// Address of the footer of a texture container (past the palette), following the `-1` group headers to the first
    /// plain container; 0 when there is none. A chain longer than 4096 headers (the original would not return) gives 0.
    /// orig 0x132238
    pub fn footer_start(m: &Mem, mut p: u32) -> u32 {
        for _ in 0..4096 {
            if p == 0 {
                return 0;
            }
            if m.r16(p) as i16 != -1 {
                if m.r16(p) != 0 {
                    return 0;
                }
                return p.wrapping_add(m.r32(p.wrapping_add(0x20))).wrapping_add(m.r32(p.wrapping_add(4)).wrapping_mul(4));
            }
            let count = m.r32(p.wrapping_add(4));
            p = p.wrapping_add(m.r32(p.wrapping_add(count.wrapping_mul(4)).wrapping_add(4)));
        }
        0
    }

    /// Entry `index` of a container group (`-1` header: base plus the offset in its table), or the container itself when
    /// `p` is a plain container (first halfword 0); 0 otherwise.
    /// orig 0x132288
    pub fn section(m: &Mem, p: u32, index: u32) -> u32 {
        if p != 0 {
            let head = m.r16(p) as i16;
            if head == -1 {
                return p.wrapping_add(m.r32(p.wrapping_add(index.wrapping_mul(4)).wrapping_add(8)));
            }
            if head == 0 {
                return p;
            }
        }
        0
    }

    /// A word at any alignment (the original assembles it from bytes when the address is odd).
    /// orig 0x1322e0
    pub fn word_at(m: &Mem, a: u32) -> u32 {
        m.r32(a)
    }

    /// The size word of a footer block (its second word), at any alignment.
    /// orig 0x132320
    pub fn block_size(m: &Mem, block: u32) -> u32 {
        m.r32(block.wrapping_add(4))
    }

    /// The block after `block`, or 0 when what follows is not a known tag.
    /// orig 0x132360
    pub fn next_block(m: &Mem, block: u32) -> u32 {
        let next = block.wrapping_add(block_size(m, block)).wrapping_add(8);
        if is_known_tag(m, next) { next } else { 0 }
    }

    /// Where a block's body (after tag and size) starts.
    /// orig 0x1323a0
    pub fn block_body(block: u32) -> u32 {
        block.wrapping_add(8)
    }

    /// The first block from `block` on whose tag is `tag`, stopping at the `_KN5` end block or at a word that is no tag; 0 if
    /// none.
    /// orig 0x1323b0
    pub fn find_block(m: &Mem, mut block: u32, tag: u32) -> u32 {
        while is_known_tag(m, block) && word_at(m, block) != TAG_KN5 {
            if word_at(m, block) == tag {
                return block;
            }
            block = next_block(m, block);
        }
        0
    }

    /// Whether `block` starts with one of the four tags a footer holds (`TEXA`, `UVSC`, `_DMY`, `_KN5`); false for null.
    /// orig 0x132438
    pub fn is_known_tag(m: &Mem, block: u32) -> bool {
        block != 0 && matches!(word_at(m, block), TAG_TEXA | TAG_UVSC | TAG_KN5 | TAG_DMY)
    }

    // ---- the base object --------------------------------------------------------------------------------------------

    /// Sets up the fields every animation object shares: container +4, kind 0, run time 0, no footer block, flags 0, speed
    /// 1.0, no next object, no link, and the base vtable.
    /// orig 0x1324a0
    pub fn base_init(m: &mut Mem, obj: u32, container: u32) {
        m.w32(obj + 4, container);
        m.w32(obj + 8, 0);
        m.w32(obj + 0xc, 0);
        m.wf32(obj + 0x18, 1.0);
        m.w32(obj + 0x10, 0);
        m.w32(obj, VT_ANIM);
        m.w32(obj + 0x14, 0);
        m.w32(obj + 0x1c, 0);
        m.w32(obj + 0x20, 0);
    }

    /// orig 0x132520
    pub fn base_drop(m: &mut Mem, obj: u32) {
        m.w32(obj, VT_ANIM);
    }

    /// Destructor entry of the base vtable.
    /// orig 0x132530
    pub fn base_destroy(m: &mut Mem, obj: u32) {
        m.w32(obj, VT_ANIM);
    }

    /// Destructor that also frees the block: the allocator is the word before the block (0x106280 calls its slot 1).
    /// orig 0x132540
    pub fn base_delete<H: Host>(m: &mut Mem, h: &mut H, obj: u32) {
        m.w32(obj, VT_ANIM);
        free_block(m, h, obj);
    }

    /// 0x106280: gives a block back to the allocator recorded in the word before it; null does nothing.
    fn free_block<H: Host>(m: &mut Mem, h: &mut H, block: u32) {
        if block != 0 {
            let allocator = m.r32(block.wrapping_sub(4));
            h.free(m, allocator, block);
        }
    }

    /// orig 0x132560
    pub fn container_of(m: &Mem, obj: u32) -> u32 {
        m.r32(obj + 4)
    }

    /// orig 0x132568
    pub fn kind_of(m: &Mem, obj: u32) -> u32 {
        m.r32(obj + 8)
    }

    /// Stores the footer block (slot 2 of the base and UVSC vtables).
    /// orig 0x132578
    pub fn set_block(m: &mut Mem, obj: u32, block: u32) {
        m.w32(obj + 0x10, block);
    }

    /// orig 0x132580
    pub fn next_of(m: &Mem, obj: u32) -> u32 {
        m.r32(obj + 0x1c)
    }

    /// orig 0x132588
    pub fn set_next(m: &mut Mem, obj: u32, next: u32) {
        m.w32(obj + 0x1c, next);
    }

    /// orig 0x132590
    pub fn link_of(m: &Mem, obj: u32) -> u32 {
        m.r32(obj + 0x20)
    }

    /// orig 0x132598
    pub fn set_link(m: &mut Mem, obj: u32, link: u32) {
        m.w32(obj + 0x20, link);
    }

    /// Stores the speed (a float argument; the object is the integer argument).
    /// orig 0x1325a0
    pub fn set_speed(m: &mut Mem, obj: u32, speed: f32) {
        m.wf32(obj + 0x18, speed);
    }

    /// Slot 8 of the base and UVSC vtables: two objects can be linked only if they are the same one (a 64-bit compare).
    /// orig 0x1325b0
    pub fn same_object(a: u64, b: u64) -> bool {
        a == b
    }

    /// The speed word.
    /// orig 0x133948
    pub fn speed_bits(m: &Mem, obj: u32) -> u32 {
        m.r32(obj + 0x18)
    }

    /// The first object from `from` on (up to, not including, `stop`) that answers the slot-3 query `(a, b, c)` with a
    /// non-zero value; 0 if none, or when `from` is null or `stop`.
    /// orig 0x1325c0
    pub fn find_answering(m: &Mem, from: u32, stop: u32, a: u32, b: u32, c: u32) -> u32 {
        let mut p = from;
        if p == 0 || p == stop {
            return 0;
        }
        while !query(m, p, a, b, c) {
            p = next_of(m, p);
            if p == 0 || p == stop {
                return 0;
            }
        }
        p
    }

    // ---- virtual calls ----------------------------------------------------------------------------------------------

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Class {
        Base,
        Texa,
        Uvsc,
    }

    fn class(m: &Mem, obj: u32) -> Class {
        match m.r32(obj) {
            VT_ANIM => Class::Base,
            VT_TEXA => Class::Texa,
            VT_UVSC => Class::Uvsc,
            other => panic!("object {obj:#x} has the unknown vtable {other:#x}"),
        }
    }

    /// Slot 0 (destructor without free).
    pub fn destroy(m: &mut Mem, obj: u32) {
        match class(m, obj) {
            Class::Base => base_destroy(m, obj),
            Class::Texa => texa_destroy(m, obj),
            Class::Uvsc => uvsc_destroy(m, obj),
        }
    }

    /// Slot 2: attach the footer block.
    pub fn attach_block(m: &mut Mem, obj: u32, block: u32) {
        match class(m, obj) {
            Class::Texa => texa_attach(m, obj, block),
            _ => set_block(m, obj, block),
        }
    }

    /// Slot 3: the query `(kind, mask, value)`. The base class's slot is the game's "pure virtual" handler.
    pub fn query(m: &Mem, obj: u32, a: u32, b: u32, c: u32) -> bool {
        match class(m, obj) {
            Class::Texa => texa_query(m, obj, a, b, c) != 0,
            Class::Uvsc => uvsc_query(m, obj, a as i32 as i64 as u64, b, c) != 0,
            Class::Base => panic!("pure virtual call: slot 3 of the base object {obj:#x}"),
        }
    }

    /// Slot 4: the request `(mask, value)` (a UVSC object restarts, ignoring them).
    pub fn request<H: Host>(m: &mut Mem, h: &mut H, obj: u32, mask: u32, value: u32) {
        match class(m, obj) {
            Class::Texa => texa_request(m, h, obj, mask, value),
            Class::Uvsc => uvsc_restart(m, obj),
            Class::Base => panic!("pure virtual call: slot 4 of the base object {obj:#x}"),
        }
    }

    /// Slot 5: advance by `dt` fields.
    pub fn update<H: Host>(m: &mut Mem, h: &mut H, obj: u32, dt: f32) {
        match class(m, obj) {
            Class::Texa => texa_update(m, h, obj, dt),
            Class::Uvsc => uvsc_update(m, obj, dt),
            Class::Base => panic!("pure virtual call: slot 5 of the base object {obj:#x}"),
        }
    }

    /// Slot 8: whether `obj` may take `other` as its link.
    pub fn may_link(m: &Mem, obj: u32, other: u32) -> bool {
        match class(m, obj) {
            Class::Texa => texa_may_link(m, obj, other),
            _ => same_object(obj as u64, other as u64),
        }
    }

    // ---- TEXA objects -----------------------------------------------------------------------------------------------

    /// Sets up a TEXA object: base fields, the TEXA vtable, four GS records, record selector 0, kind 1, nothing requested or
    /// playing.
    /// orig 0x1326e8
    pub fn texa_construct(m: &mut Mem, obj: u32, container: u32) {
        base_init(m, obj, container);
        m.w32(obj, VT_TEXA);
        for k in 0..4u32 {
            gs_record_init(m, obj + 0x40 + 0x90 * k);
        }
        m.w32(obj + 0x270, 0);
        m.w32(obj + 8, 1);
        m.w32(obj + 0x274, 0);
        m.w32(obj + 0x278, 0);
        m.w32(obj + 0x27c, 0);
    }

    /// Clears one GS record's register image and sets its register numbers (0x1081f8); a 64-bit register word is written whole.
    /// orig 0x1081f8
    pub fn gs_record_init(m: &mut Mem, p: u32) {
        let w64 = |m: &mut Mem, a: u32, v: u64| {
            m.w32(a, v as u32);
            m.w32(a + 4, (v >> 32) as u32);
        };
        let r64 = |m: &Mem, a: u32| m.r32(a) as u64 | (m.r32(a + 4) as u64) << 32;
        w64(m, p + 0x40, 0);
        for o in [0, 4, 8, 0xc, 0x50, 0x54, 0x58, 0x5c] {
            m.w32(p + o, 0);
        }
        let v = r64(m, p) & 0x0fff_ffff_ffff_8000 | 0x1000_0000_0000_0004;
        w64(m, p, v);
        w64(m, p + 0x28, 0x51);
        let v = r64(m, p + 0x50) & 0xf3ff_ffff_ffff_ffff | 0x0800_0000_0000_8000;
        w64(m, p + 0x50, v);
        w64(m, p + 0x18, 0x50);
        let v = r64(m, p + 8) & 0xffff_ffff_ffff_fff0 | 0xe;
        w64(m, p + 8, v);
        w64(m, p + 0x48, 0x53);
        w64(m, p + 0x38, 0x52);
    }

    /// Destructor entry of the TEXA vtable (slot 0): the vtable falls back to the base one.
    /// orig 0x132798
    pub fn texa_destroy(m: &mut Mem, obj: u32) {
        m.w32(obj, VT_TEXA);
        base_drop(m, obj);
    }

    /// Destructor-and-free entry of the TEXA vtable (slot 1).
    /// orig 0x1327c0
    pub fn texa_delete<H: Host>(m: &mut Mem, h: &mut H, obj: u32) {
        m.w32(obj, VT_TEXA);
        base_drop(m, obj);
        free_block(m, h, obj);
    }

    /// The TEXA body of an object (after tag and size), or 0 without a footer block.
    /// orig 0x132e98
    pub fn data_of(m: &Mem, obj: u32) -> u32 {
        let block = m.r32(obj + 0x10);
        if block != 0 { block_body(block) } else { 0 }
    }

    /// Animation index for a request: with mask 0 the value itself if below the animation count, otherwise `value & mask`
    /// looked up in the slot table; `u32::MAX` for none.
    pub fn anim_index_of(m: &Mem, obj: u32, mask: u32, value: u32) -> u32 {
        let d = data_of(m, obj);
        if d == 0 {
            return u32::MAX;
        }
        let count = m.r16(d + 0xc) as u32;
        if mask == 0 {
            return if count <= value { u32::MAX } else { value };
        }
        let v = value & mask;
        let (lo, hi) = (m.r16(d + 8) as u32, m.r16(d + 10) as u32);
        if lo <= v && v <= hi {
            let e = m.r16((v - lo).wrapping_mul(2).wrapping_add(d).wrapping_add(m.r32(d + 0x18))) as u32;
            if e == 0 || count < e {
                return u32::MAX;
            }
            return e - 1;
        }
        u32::MAX
    }

    /// Slot 2 of the TEXA vtable: stores the footer block, then requests the default animation.
    pub fn texa_attach(m: &mut Mem, obj: u32, block: u32) {
        set_block(m, obj, block);
        let d = data_of(m, obj);
        if d == 0 {
            return;
        }
        let idx = anim_index_of(m, obj, 0, m.r32(d.wrapping_add(0x24)));
        if (idx as i32) >= 0 {
            let off = m.r32(idx.wrapping_mul(4).wrapping_add(d).wrapping_add(m.r32(d + 0x1c)));
            m.w32(obj + 0x274, d.wrapping_add(off));
        }
    }

    /// Slot 3 of the TEXA vtable: with kind 1, whether the animation index for `(mask, value)` exists.
    /// orig 0x132910
    pub fn texa_query(m: &Mem, obj: u32, kind: u32, mask: u32, value: u32) -> u32 {
        if kind as i32 == 1 { (!anim_index_of(m, obj, mask, value)) >> 31 } else { 0 }
    }

    /// Starts the frame table at `table` unless it is already the playing one: a random first frame length, time 0, flag 1.
    pub fn texa_start<H: Host>(m: &mut Mem, h: &mut H, obj: u32, table: u32) {
        if m.r32(obj + 0x278) != table {
            if table == 0 {
                m.w32(obj + 0x278, 0);
            } else {
                let f = m.r32(obj + 0x14) | 1;
                m.w32(obj + 0x14, f);
                let (lo, hi) = (m.r16(table + 2) as u32, m.r16(table + 4) as u32 + 1);
                let v = h.rand_between(m, lo, hi);
                m.wf32(obj + 0x280, length_of(v));
                m.w32(obj + 0x284, 0);
                m.w32(obj + 0x278, table);
            }
            m.w32(obj + 0x27c, table);
        }
    }

    /// Slot 4 of the TEXA vtable: starts the animation a request `(mask, value)` names.
    /// orig 0x1329e0
    pub fn texa_request<H: Host>(m: &mut Mem, h: &mut H, obj: u32, mask: u32, value: u32) {
        let d = data_of(m, obj);
        if d != 0 {
            let idx = anim_index_of(m, obj, mask, value);
            if (idx as i32) >= 0 {
                let off = m.r32(idx.wrapping_mul(4).wrapping_add(d).wrapping_add(m.r32(d + 0x1c)));
                texa_start(m, h, obj, d.wrapping_add(off));
            }
        }
    }

    /// Plays the animation slot `slot` names (mask 0xffff) and marks the table as chosen by a request (flag 4).
    pub fn texa_play_slot<H: Host>(m: &mut Mem, h: &mut H, obj: u32, slot: u32) {
        let d = data_of(m, obj);
        if d != 0 {
            let idx = anim_index_of(m, obj, 0xffff, slot);
            if (idx as i32) >= 0 {
                let off = m.r32(idx.wrapping_mul(4).wrapping_add(d).wrapping_add(m.r32(d + 0x1c)));
                let f = m.r32(obj + 0x14) | 4;
                m.w32(obj + 0x14, f);
                texa_start(m, h, obj, d.wrapping_add(off));
            }
        }
    }

    /// Slot 5 of the TEXA vtable: one field update by `dt`. A linked object copies the state of its link instead.
    /// orig 0x132a80
    pub fn texa_update<H: Host>(m: &mut Mem, h: &mut H, obj: u32, dt: f32) {
        let d = data_of(m, obj);
        if d == 0 {
            return;
        }
        if m.r32(obj + 0x14) & 4 == 0 {
            let table = m.r32(obj + 0x274);
            texa_start(m, h, obj, table);
        } else {
            let f = m.r32(obj + 0x14) & !4;
            m.w32(obj + 0x14, f);
        }
        let link = m.r32(obj + 0x20);
        if link != 0 {
            let cur = m.r32(link + 0x27c);
            if cur == 0 {
                m.w32(obj + 0x27c, 0);
            } else {
                let v = m.r32(obj + 0x278).wrapping_add(cur.wrapping_sub(m.r32(link + 0x278)));
                m.w32(obj + 0x27c, v);
            }
            let flags = m.r32(link + 0x14);
            m.w32(obj + 0x14, flags);
            let (len, time) = (m.r32(link + 0x280), m.r32(link + 0x284));
            m.w32(obj + 0x280, len);
            m.w32(obj + 0x284, time);
            return;
        }
        let mut at = m.r32(obj + 0x27c);
        if at == 0 {
            return;
        }
        let code = m.r16(at) & 0xf;
        if code == 3 {
            return;
        }
        let step = (m.r16(d + 4) as u32).wrapping_mul(2).wrapping_add(8);
        let jump = |m: &Mem, at: u32| ((m.r16(at) as i16 as i32) >> 4) as u32;
        if code == 2 {
            at = at.wrapping_add(jump(m, at).wrapping_mul(step));
            m.w32(obj + 0x27c, at);
        }
        let flags = m.r32(obj + 0x14);
        if flags & 1 != 0 {
            m.w32(obj + 0x14, flags & !1);
            return;
        }
        let mut len = m.rf32(obj + 0x280);
        let mut time = m.rf32(obj + 0x284) + dt * m.rf32(obj + 0x18);
        m.wf32(obj + 0x284, time);
        while len <= time {
            m.wf32(obj + 0x284, time - len);
            let at = m.r32(obj + 0x27c).wrapping_add(step);
            m.w32(obj + 0x27c, at);
            match m.r16(at) & 0xf {
                2 => {
                    if m.r32(obj + 0x14) & 2 != 0 {
                        m.w32(obj + 0x27c, 0);
                        return;
                    }
                    m.w32(obj + 0x27c, at.wrapping_add(jump(m, at).wrapping_mul(step)));
                }
                3 => return,
                _ => {}
            }
            let at = m.r32(obj + 0x27c);
            let (lo, hi) = (m.r16(at + 2) as u32, m.r16(at + 4) as u32 + 1);
            let v = h.rand_between(m, lo, hi);
            len = length_of(v);
            time = m.rf32(obj + 0x284);
            m.wf32(obj + 0x280, len);
        }
    }

    /// Slot 6 of the TEXA vtable: forgets the playing table and frame.
    /// orig 0x132c90
    pub fn texa_reset(m: &mut Mem, obj: u32) {
        m.w32(obj + 0x278, 0);
        m.w32(obj + 0x27c, 0);
    }

    /// Slot 7 of the TEXA vtable: whether a frame is showing (it has a footer block and a current frame).
    /// orig 0x132ca0
    pub fn texa_running(m: &Mem, obj: u32) -> bool {
        data_of(m, obj) != 0 && m.r32(obj + 0x27c) != 0
    }

    /// Slot 8 of the TEXA vtable: whether `obj` may take `other` as its link: the same object, or two objects of one kind
    /// whose footers have the same texture, size, animation count and slot range and agree on which slots name an animation.
    /// orig 0x132ce0
    pub fn texa_may_link(m: &Mem, obj: u32, other: u32) -> bool {
        if obj == other {
            return true;
        }
        if m.r32(obj + 8) != kind_of(m, other) {
            return false;
        }
        let (a, b) = (data_of(m, obj), data_of(m, other));
        if a == 0 || b == 0 {
            return false;
        }
        let same = |o: u32| m.r16(a + o) == m.r16(b + o);
        if !(same(0) && same(2) && same(0xc) && same(8) && same(0xa) && same(0x10) && same(0x12) && same(0x14) && same(0x16)) {
            return false;
        }
        let (count, lo, hi) = (m.r16(a + 0xc), m.r16(a + 8) as u32, m.r16(a + 0xa) as u32);
        if lo <= hi {
            let mut s = lo;
            loop {
                let ea = m.r16(a.wrapping_add((s - lo) * 2).wrapping_add(m.r32(a + 0x18)));
                let eb = m.r16(b.wrapping_add((s - lo) * 2).wrapping_add(m.r32(b + 0x18)));
                let a_names = !(ea == 0 || count < ea);
                let b_names = !(eb == 0 || count < eb);
                if a_names != b_names {
                    return false;
                }
                s = (s + 1) & 0xffff;
                if s > hi {
                    break;
                }
            }
        }
        true
    }

    /// Address of the GS record the selector (+0x270) names.
    /// orig 0x132ec8
    pub fn gs_record(m: &Mem, obj: u32) -> u32 {
        obj.wrapping_add(m.r32(obj + 0x270).wrapping_mul(0x90)).wrapping_add(0x30)
    }

    /// Flips the record selector between 0 and 2.
    /// orig 0x132ee8
    pub fn flip_record(m: &mut Mem, obj: u32) {
        let v = if m.r32(obj + 0x270) != 0 { 0 } else { 2 };
        m.w32(obj + 0x270, v);
    }

    /// The footer's destination x, y, width, height (body +0x10..0x16); `u32::MAX` without a footer block.
    /// orig 0x132f00
    pub fn texa_x(m: &Mem, obj: u32) -> u32 {
        field16(m, obj, 0x10)
    }
    /// orig 0x132f30
    pub fn texa_y(m: &Mem, obj: u32) -> u32 {
        field16(m, obj, 0x12)
    }
    /// orig 0x132f60
    pub fn texa_width(m: &Mem, obj: u32) -> u32 {
        field16(m, obj, 0x14)
    }
    /// orig 0x132f90
    pub fn texa_height(m: &Mem, obj: u32) -> u32 {
        field16(m, obj, 0x16)
    }
    fn field16(m: &Mem, obj: u32, off: u32) -> u32 {
        let d = data_of(m, obj);
        if d != 0 { m.r16(d + off) as u32 } else { u32::MAX }
    }

    /// Writes into `out` the TEXA objects of the list from `list` that show a frame of texture `texture` (and belong to
    /// `filter` unless it is 0); returns how many. `out` may be 0 to only count.
    /// orig 0x1331c8
    pub fn collect_for_texture(m: &mut Mem, list: u32, texture: u32, out: u32, filter: u32) -> i32 {
        let mut n = 0;
        let mut p = list;
        let mut dst = out;
        while p != 0 {
            if kind_of(m, p) == 1 {
                let d = data_of(m, p);
                if (filter == 0 || m.r32(p + 4) == filter) && d != 0 && m.r16(d + 2) as u32 == texture && m.r32(p + 0x27c) != 0 {
                    if out != 0 {
                        m.w32(dst, p);
                    }
                    dst = dst.wrapping_add(4);
                    n += 1;
                }
            }
            p = next_of(m, p);
        }
        n
    }

    /// For every texture 0..`textures` writes at `out` a (count, byte offset) pair, then after the pairs the object lists of
    /// each texture one after the other; returns the total. At most 8 objects per texture (the original's stack buffer).
    /// orig 0x1332a8
    pub fn build_texture_lists(m: &mut Mem, list: u32, out: u32, textures: u32, filter: u32) -> i32 {
        let mut dst = out.wrapping_add(textures.wrapping_mul(8));
        let mut total = 0i32;
        for t in 0..textures {
            let mut found = Vec::new();
            let mut p = list;
            while p != 0 {
                if kind_of(m, p) == 1 {
                    let d = data_of(m, p);
                    if (filter == 0 || m.r32(p + 4) == filter) && d != 0 && m.r16(d + 2) as u32 == t && m.r32(p + 0x27c) != 0 {
                        found.push(p);
                    }
                }
                p = next_of(m, p);
            }
            assert!(found.len() <= 8, "more than 8 objects for one texture overflows the original's buffer");
            let at = out.wrapping_add(t * 8);
            m.w32(at, found.len() as u32);
            m.w32(at + 4, (textures * 2).wrapping_add(total as u32).wrapping_mul(4));
            total += found.len() as i32;
            for f in found {
                m.w32(dst, f);
                dst = dst.wrapping_add(4);
            }
        }
        total
    }

    // ---- UVSC objects -----------------------------------------------------------------------------------------------

    /// Sets up a UVSC object: base fields, no accumulated offset, kind 2, the UVSC vtable.
    /// orig 0x1333c8
    pub fn uvsc_construct(m: &mut Mem, obj: u32, container: u32) {
        base_init(m, obj, container);
        m.w32(obj + 0x28, 0);
        m.w32(obj + 0x24, 0);
        m.w32(obj + 8, 2);
        m.w32(obj, VT_UVSC);
    }

    /// Destructor entry of the UVSC vtable (slot 0).
    /// orig 0x133430
    pub fn uvsc_destroy(m: &mut Mem, obj: u32) {
        m.w32(obj, VT_UVSC);
        base_drop(m, obj);
    }

    /// Destructor-and-free entry of the UVSC vtable (slot 1).
    /// orig 0x133458
    pub fn uvsc_delete<H: Host>(m: &mut Mem, h: &mut H, obj: u32) {
        m.w32(obj, VT_UVSC);
        base_drop(m, obj);
        free_block(m, h, obj);
    }

    /// The UVSC body of an object (after tag and size), or 0 without a footer block.
    /// orig 0x1335f8
    pub fn uvsc_data(m: &Mem, obj: u32) -> u32 {
        data_of(m, obj)
    }

    /// Slot 3 of the UVSC vtable: with kind 2, whether the body's first word under `mask` equals `value`.
    /// orig 0x133490
    pub fn uvsc_query(m: &Mem, obj: u32, kind: u64, mask: u32, value: u32) -> u32 {
        let d = uvsc_data(m, obj);
        if kind == 2 && d != 0 && (m.r32(d) & mask) == value { 1 } else { 0 }
    }

    /// Slot 4 of the UVSC vtable: restart (run time and offsets to 0, skip the next advance).
    pub fn uvsc_restart(m: &mut Mem, obj: u32) {
        let f = m.r32(obj + 0x14) | 1;
        m.w32(obj + 0xc, 0);
        m.w32(obj + 0x24, 0);
        m.w32(obj + 0x14, f);
        m.w32(obj + 0x28, 0);
    }

    /// Slot 5 of the UVSC vtable: one field update by `dt`.
    /// orig 0x133518
    pub fn uvsc_update(m: &mut Mem, obj: u32, dt: f32) {
        let d = uvsc_data(m, obj);
        if d == 0 {
            return;
        }
        let flags = m.r32(obj + 0x14);
        if flags & 2 == 0 {
            if flags & 1 == 0 {
                let dt = dt * m.rf32(obj + 0x18);
                let u = (m.r32(obj + 0x24) as i32).wrapping_add((m.rf32(d + 4) * dt) as i32);
                m.w32(obj + 0x24, u as u32);
                let t = m.rf32(obj + 0xc) + dt;
                m.wf32(obj + 0xc, t);
                let v = (m.r32(obj + 0x28) as i32).wrapping_add((m.rf32(d + 8) * dt) as i32);
                m.w32(obj + 0x28, v as u32);
            } else {
                m.w32(obj + 0x14, flags & !1);
            }
        }
    }

    /// Slot 7 of the UVSC vtable: whether the object runs (it has a footer block and is not stopped).
    /// orig 0x1335d8
    pub fn uvsc_running(m: &Mem, obj: u32) -> bool {
        m.r32(obj + 0x10) != 0 && m.r32(obj + 0x14) & 2 == 0
    }

    /// Slot 6 of the UVSC vtable: stop.
    pub fn uvsc_stop(m: &mut Mem, obj: u32) {
        let f = m.r32(obj + 0x14) | 2;
        m.w32(obj + 0x14, f);
    }

    // ---- the owner's list -------------------------------------------------------------------------------------------

    /// Destroys and frees every object of the list starting at `list`.
    /// orig 0x11e108
    pub fn free_list<H: Host>(m: &mut Mem, h: &mut H, owner: u32, mut list: u32) {
        while list != 0 {
            let next = next_of(m, list);
            destroy(m, list);
            let allocator = m.r32(owner + 0x10);
            h.free(m, allocator, list);
            list = next;
        }
    }

    /// Loads a container group: remembers it at owner +0x34, makes the objects of the first container (replacing the
    /// owner's list), then those of every further one, and links objects of the first container to objects of the others.
    /// orig 0x11e018
    pub fn load_group<H: Host>(m: &mut Mem, h: &mut H, owner: u32, group: u32) {
        m.w32(owner + 0x34, group);
        let first = footer_start(m, section(m, group, 0));
        replace_list(m, h, owner, first);
        let g = m.r32(owner + 0x34);
        if g != 0 && m.r16(g) as i16 == -1 && 1 < m.r32(g + 4) {
            let mut i = 1;
            loop {
                let f = footer_start(m, section(m, g, i));
                attach_footer(m, h, owner, f);
                let a = footer_start(m, section(m, m.r32(owner + 0x34), 0));
                let b = footer_start(m, section(m, m.r32(owner + 0x34), i));
                link_up(m, owner, a, b);
                i += 1;
                if i >= m.r32(m.r32(owner + 0x34) + 4) {
                    break;
                }
            }
        }
    }

    /// Frees the owner's objects and makes those of one footer.
    /// orig 0x11e1c8
    pub fn replace_list<H: Host>(m: &mut Mem, h: &mut H, owner: u32, footer: u32) {
        let list = m.r32(owner + 0x38);
        free_list(m, h, owner, list);
        m.w32(owner + 0x38, 0);
        attach_footer(m, h, owner, footer);
    }

    /// Appends one object per `TEXA` block and then one per `UVSC` block of a footer to the owner's list.
    /// orig 0x11e208
    pub fn attach_footer<H: Host>(m: &mut Mem, h: &mut H, owner: u32, footer: u32) {
        if footer == 0 {
            return;
        }
        let mut tail = m.r32(owner + 0x38);
        while tail != 0 && next_of(m, tail) != 0 {
            tail = next_of(m, tail);
        }
        for (tag, size) in [(TAG_TEXA, 0x290), (TAG_UVSC, 0x2c)] {
            let mut block = find_block(m, footer, tag);
            while block != 0 {
                let allocator = m.r32(owner + 0x10);
                let obj = h.alloc(m, allocator, size);
                if tag == TAG_TEXA {
                    texa_construct(m, obj, footer);
                } else {
                    uvsc_construct(m, obj, footer);
                }
                attach_block(m, obj, block);
                if tail == 0 {
                    m.w32(owner + 0x38, obj);
                } else {
                    set_next(m, tail, obj);
                }
                let after = next_block(m, block);
                block = find_block(m, after, tag);
                tail = obj;
            }
        }
    }

    /// Re-links the objects of two footers `a` and `b`: first drops links between the two, then lets each object of `a`
    /// that a later object of `b` accepts (slot 8) become that object's link, and the same with the roles swapped.
    /// orig 0x11e3b8
    pub fn link_up(m: &mut Mem, owner: u32, a: u32, b: u32) {
        if a == 0 || b == 0 || a == b || m.r32(owner + 0x38) == 0 {
            return;
        }
        let mut p = m.r32(owner + 0x38);
        while p != 0 {
            let id = container_of(m, p);
            if id == a || id == b {
                let link = link_of(m, p);
                if link != 0 {
                    let lid = container_of(m, link);
                    if lid == a || lid == b {
                        set_link(m, p, 0);
                    }
                }
            }
            p = next_of(m, p);
        }
        for (first, second) in [(a, b), (b, a)] {
            let mut p = m.r32(owner + 0x38);
            while p != 0 {
                if container_of(m, p) == first {
                    let mut q = next_of(m, p);
                    while q != 0 {
                        if container_of(m, q) == second && may_link(m, q, p) {
                            set_link(m, q, p);
                        }
                        q = next_of(m, q);
                    }
                }
                p = next_of(m, p);
            }
        }
    }

    /// The next object after `after` (the first when `after` is 0) that answers the query `(a, b, c)`; 0 if none.
    /// orig 0x11e598
    pub fn next_answering(m: &Mem, owner: u32, after: u32, a: u32, b: u32, c: u32) -> u32 {
        let mut p = if after == 0 { m.r32(owner + 0x38) } else { next_of(m, after) };
        while p != 0 {
            if query(m, p, a, b, c) {
                return p;
            }
            p = next_of(m, p);
        }
        0
    }

    /// Sends the request `(b, c)` to every object that answers the query `(a, b, c)`.
    /// orig 0x11e638
    pub fn request_all<H: Host>(m: &mut Mem, h: &mut H, owner: u32, a: u32, b: u32, c: u32) {
        let mut p = 0;
        loop {
            p = next_answering(m, owner, p, a, b, c);
            if p == 0 {
                break;
            }
            request(m, h, p, b, c);
        }
    }

    /// Sets the speed word of every object that answers the query `(a, b, c)`.
    /// orig 0x11e760
    pub fn speed_all(m: &mut Mem, speed: f32, owner: u32, a: u32, b: u32, c: u32) {
        let mut p = 0;
        loop {
            p = next_answering(m, owner, p, a, b, c);
            if p == 0 {
                break;
            }
            set_speed(m, p, speed);
        }
    }

    /// Advances every object of the owner's list by `dt` fields.
    /// orig 0x11e7f8
    pub fn update_all<H: Host>(m: &mut Mem, h: &mut H, owner: u32, dt: f32) {
        let mut p = m.r32(owner + 0x38);
        while p != 0 {
            update(m, h, p, dt);
            p = next_of(m, p);
        }
    }

    /// Asks every TEXA object whose slot table names `slot` to play it.
    pub fn request_slot<H: Host>(m: &mut Mem, h: &mut H, owner: u32, slot: u32) {
        let mut p = m.r32(owner + 0x38);
        while p != 0 {
            if query(m, p, 1, 0xffff, slot) {
                texa_play_slot(m, h, p, slot);
            }
            p = next_of(m, p);
        }
    }

    /// The actor's request for animation slot `slot`: forwarded to the owner at actor +0x670.
    /// orig 0x192c80
    pub fn actor_request_slot<H: Host>(m: &mut Mem, h: &mut H, actor: u32, slot: u32) {
        let owner = m.r32(actor + 0x670);
        request_slot(m, h, owner, slot);
    }

    // ---- model unit leaves ------------------------------------------------------------------------------------------

    /// Swaps the model's two matrix slots (+0x2c with +0x30, +0x108 with +0x10c).
    /// orig 0x11d1f8
    pub fn matrix_swap(m: &mut Mem, model: u32) {
        let (a, b) = (m.r32(model + 0x2c), m.r32(model + 0x10c));
        let (c, d) = (m.r32(model + 0x30), m.r32(model + 0x108));
        m.w32(model + 0x2c, c);
        m.w32(model + 0x10c, d);
        m.w32(model + 0x30, a);
        m.w32(model + 0x108, b);
    }

    /// The address of the model unit's static object.
    /// orig 0x134728
    pub fn model_unit_address() -> u32 {
        MODEL_UNIT
    }

    /// With `(1, 0xffff)` resets the static object (0x1337a0 → 0x133860: nine words zeroed and the first byte, the other
    /// three bytes of its first word kept); with `(0, 0xffff)` nothing happens (0x1337c0).
    /// orig 0x134738
    pub fn model_unit_command(m: &mut Mem, what: i32, id: u64) {
        if id == 0xffff && what == 1 {
            let p = MODEL_UNIT;
            m.w32(p + 0x1c, 0);
            m.w8(p, 0);
            for o in [4, 8, 0xc, 0x10, 0x18, 0x20, 0x14] {
                m.w32(p + o, 0);
            }
        }
    }

    /// orig 0x1347b0
    pub fn model_unit_reset(m: &mut Mem) {
        model_unit_command(m, 1, 0xffff);
    }

    /// orig 0x1347d0
    pub fn model_unit_idle(m: &mut Mem) {
        model_unit_command(m, 0, 0xffff);
    }

    /// Does nothing (the second command of 0x134738).
    /// orig 0x1337c0
    pub fn nothing() {}

    /// Sets the model unit's matrix block: three quads cleared, the second quad's first three words 1.0, quad 5 four 1.0
    /// words copied down to quads 4 and 3, and quad 6 from the executable's constant quad `init` (0x354e50).
    /// orig 0x134898
    pub fn matrix_block_init(m: &mut Mem, p: u32, init: [u32; 4]) {
        for w in 0..12 {
            m.w32(p + w * 4, 0);
        }
        let one = 1.0f32.to_bits();
        for w in [0, 8, 4] {
            m.w32(p + 16 + w, one);
        }
        for w in 0..4 {
            m.w32(p + 0x50 + w * 4, one);
        }
        for w in 0..4 {
            let v = m.r32(p + 0x50 + w * 4);
            m.w32(p + 0x40 + w * 4, v);
        }
        for w in 0..4 {
            let v = m.r32(p + 0x40 + w * 4);
            m.w32(p + 0x30 + w * 4, v);
        }
        for (w, v) in init.iter().enumerate() {
            m.w32(p + 0x60 + w as u32 * 4, *v);
        }
    }

    // ---- range table dispatch ---------------------------------------------------------------------------------------

    /// Calls `cb(entry, arg)` for each entry of a range table whose `[lo, hi)` contains `value` (`hi` 0xffff means no upper
    /// limit). Table: byte 0 the entry count; from +4 entries `lo: u16, hi: u16, ..`, each (byte 5 of the entry + 3)
    /// halfwords long.
    /// orig 0x188bd8
    pub fn range_dispatch(m: &mut Mem, value: f32, table: u32, arg: u32, mut cb: impl FnMut(&mut Mem, u32, u32)) {
        let mut entry = table.wrapping_add(4);
        let mut count = m.r8(table) as i32;
        let mut i = 0;
        if count != 0 {
            loop {
                let (lo, hi) = (m.r16(entry), m.r16(entry + 2));
                let hit = (lo as f32) <= value && (hi == 0xffff || value < hi as f32);
                if hit {
                    cb(m, entry, arg);
                    count = m.r8(table) as i32;
                }
                i += 1;
                entry = entry.wrapping_add((m.r8(entry + 5) as u32 + 3) * 2);
                if i >= count {
                    break;
                }
            }
        }
    }

    /// The same dispatch with the game's event handler (0x188030) as the callback.
    /// orig 0x188010
    pub fn range_dispatch_events(m: &mut Mem, value: f32, table: u32, arg: u32, handler: impl FnMut(&mut Mem, u32, u32)) {
        range_dispatch(m, value, table, arg, handler);
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

    /// A host for the layout tests: a bump allocator, a counter-based random source.
    struct Toy(u32);
    impl objects::Host for Toy {
        fn alloc(&mut self, m: &mut objects::Mem, _: u32, size: u32) -> u32 {
            let at = self.0;
            self.0 += (size + 15) & !15;
            m.w32(at - 4, 0);
            at
        }
        fn free(&mut self, _: &mut objects::Mem, _: u32, _: u32) {}
        fn rand_between(&mut self, _: &mut objects::Mem, lo: u32, _: u32) -> i32 {
            lo as i32
        }
    }

    /// A plain container with a footer of one TEXA block (one animation of one frame that shows sprite 0 for 2 fields and
    /// jumps back) and one UVSC block, loaded into an owner: the list holds the TEXA object then the UVSC object, and updates
    /// advance the frame table and the UV offset.
    #[test]
    fn objects_load_a_container_and_update() {
        use objects::*;
        let mut m = Mem::new(0x1000, 0x4000);
        let c = 0x1100u32;
        m.w32(c + 0x20, 0x40);
        // Footer at c + 0x40: TEXA block, UVSC block, _KN5.
        let texa = c + 0x40;
        m.w32(texa, TAG_TEXA);
        m.w32(texa + 4, 0x50);
        let d = texa + 8;
        m.w16(d + 2, 1);
        m.w16(d + 0xc, 1);
        m.w32(d + 0x1c, 0x30);
        m.w32(d + 0x30, 0x38);
        let f = d + 0x38;
        for (k, (ctrl, mn, mx)) in [(0u16, 2u16, 2u16), (2 | (((-1i16) << 4) as u16), 0, 0)].iter().enumerate() {
            m.w16(f + 8 * k as u32, *ctrl);
            m.w16(f + 8 * k as u32 + 2, *mn);
            m.w16(f + 8 * k as u32 + 4, *mx);
        }
        let uvsc = texa + 8 + 0x50;
        m.w32(uvsc, TAG_UVSC);
        m.w32(uvsc + 4, 12);
        m.wf32(uvsc + 8 + 4, 2.5);
        m.w32(uvsc + 20, TAG_KN5);
        assert_eq!(footer_start(&m, c), texa);
        assert_eq!(find_block(&m, texa, TAG_UVSC), uvsc);
        assert_eq!(next_block(&m, texa), uvsc);
        let owner = 0x1000u32 + 0x80;
        m.w32(owner + 0x10, 0x1040);
        let mut h = Toy(0x2000);
        load_group(&mut m, &mut h, owner, c);
        let first = m.r32(owner + 0x38);
        let second = next_of(&m, first);
        assert_eq!((kind_of(&m, first), kind_of(&m, second), next_of(&m, second)), (1, 2, 0));
        assert_eq!(m.r32(first + 0x274), f);
        update_all(&mut m, &mut h, owner, 1.0);
        assert!(texa_running(&m, first));
        assert_eq!(m.r32(first + 0x27c), f);
        update_all(&mut m, &mut h, owner, 1.0);
        update_all(&mut m, &mut h, owner, 1.0);
        // After 2 fields of the first entry the second entry jumps back to the first.
        assert_eq!(m.r32(first + 0x27c), f);
        update_all(&mut m, &mut h, owner, 1.0);
        assert_eq!(m.r32(second + 0x24) as i32, 4 * 2);
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
