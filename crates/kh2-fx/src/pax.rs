//! PAX (BAR type 18) → DPX → DPD containers.
//!
//! * PAX: `"PAX_"`, debug-name offset, element count, DPX offset; then 0x50-byte elements (what to
//!   spawn, bound to which bone, with which flags and transform).
//! * DPX: effect list (DPD offset, particle-data index, effect number) and the DPD offset table.
//! * DPD (version 0x96): five offset lists (particle data, textures, shapes, models, VSF) and VRAM ranges.
//! * Particle data: a 0x110-byte header, then a program file: life, flags, the manager-level
//!   programs, and a linked list of groups. Each group is an object type running a list of
//!   programs, each with its own keyframed parameter list and work-memory offsets ("useVal").
//!
//! Layouts per OpenKH `OpenKh.Kh2/{Pax,Dpx,Dpd*,DpdPData}.cs` (Apache-2.0). The meaning of the
//! program-file header words (life, flags, start frame) and of the frame unit (0x1000 per 60 Hz
//! tick) was read from the game's effect runtime; see CREDITS.md.

use crate::rd::*;

/// One frame tick in the program file's fixed-point time unit.
pub const TICK: i32 = 0x1000;
/// "No frame": used by loop points and lifetimes that never trigger.
pub const NEVER: i32 = 0x7000_0000;

#[derive(Debug, Clone)]
pub struct Pax {
    pub elements: Vec<Element>,
    pub effects: Vec<DpxEffect>,
    pub dpds: Vec<Dpd>,
}

/// A PAX element: spawns DPX effect `effect` when the game starts effect `id`.
#[derive(Debug, Clone)]
pub struct Element {
    pub effect: u16,
    pub id: u16,
    pub group: u8,
    pub fadeout_frame: u8,
    pub bone: i16,
    pub category: u64,
    pub flags: u32,
    pub start_wait: f32,
    pub se: i32,
    pub translation: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    /// Bone pair + ratios for two-point effects (weapon trails): point = bone × (ratio × bone length, 0, 0).
    pub bone_pos: BonePos,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BonePos {
    pub a: u16,
    pub b: u16,
    pub ratio_a: f32,
    pub ratio_b: f32,
    pub adjust: u8,
}

pub mod element_flag {
    pub const BIND: u32 = 0x01;
    pub const BIND_ONLY_START: u32 = 0x02;
    pub const BIND_ONLY_POS: u32 = 0x04;
    pub const GET_COLOR: u32 = 0x08;
    pub const GET_BRIGHTNESS: u32 = 0x10;
    pub const DESTROY_WHEN_MOTION_CHANGE: u32 = 0x20;
    pub const DESTROY_FADEOUT: u32 = 0x40;
    pub const DESTROY_LOOPEND: u32 = 0x80;
    pub const BIND_SCALE: u32 = 0x100;
    pub const DESTROY_WHEN_OBJECT_LEAVES: u32 = 0x200;
    pub const DESTROY_WHEN_BIND_EFFECT_OFF: u32 = 0x400;
    pub const GET_OBJECT_FADE: u32 = 0x800;
    pub const BONE_POS: u32 = 0x1000;
    pub const BIND_CAMERA: u32 = 0x2000;
    /// Two-point effect: the game allocates a (base, tip) point pair and refreshes it from `bone_pos`.
    pub const TWO_POINT: u32 = 0x10000;
}

#[derive(Debug, Clone, Copy)]
pub struct DpxEffect {
    pub dpd: usize,
    pub particle_data: usize,
    pub number: u32,
}

#[derive(Debug, Clone)]
pub struct Dpd {
    pub particles: Vec<ParticleData>,
    pub textures: Vec<Texture>,
    pub shapes: Vec<crate::shape::Shape>,
    pub models: Vec<crate::omd::Model>,
    pub vsfs: Vec<Vsf>,
}

/// A particle-data block: header (bbox, offset, angle, scale, used resources, work sizes, sound)
/// and the program file.
#[derive(Debug, Clone)]
pub struct ParticleData {
    pub offset: [f32; 3],
    pub angle: [i32; 3],
    pub scale: [f32; 3],
    pub use_texture: [u8; 16],
    pub use_shape: [u8; 16],
    pub use_model: [u8; 16],
    pub use_vsf: [u8; 16],
    pub work_mem: i32,
    pub packet_mem: i32,
    pub chr_scale: u8,
    pub bind_pos_only: u8,
    pub non_ambient: u8,
    pub se: u16,
    pub se_vol_add: u16,
    pub se_wait: f32,
    pub bone: i32,
    pub wait: f32,
    /// Manager lifetime in frame units (`TICK` per tick).
    pub life: i32,
    /// Bit 0: looping effect (runs until told to end).
    pub flags: u32,
    /// Every program kind the groups use; the game runs per-kind manager hooks once per tick.
    pub program_kinds: Vec<u32>,
    pub groups: Vec<Group>,
}

/// An object type: objects of this group are born at `start_frame` and run `programs`.
#[derive(Debug, Clone)]
pub struct Group {
    pub flag: i32,
    pub status: i32,
    pub start_frame: i32,
    pub total_frame: i32,
    pub loop_start: i32,
    pub loop_end: i32,
    /// Object size up to the per-program cursor array (header 0xa0 + work memory).
    pub cursor_offset: i32,
    pub priority: i16,
    pub programs: Vec<Program>,
}

#[derive(Debug, Clone)]
pub struct Program {
    /// Index into the game's program table (see `ppp::NAMES`).
    pub kind: u32,
    pub param_size: usize,
    /// Parameter entries (`param_size` bytes each, the first word is the trigger frame), then the
    /// list terminator `0xFFFFF000`.
    pub params: Vec<u8>,
    /// Work-memory offsets (relative to object offset 0xa0) this program reads and writes.
    pub use_val: Vec<i32>,
}

impl Program {
    pub fn entries(&self) -> usize {
        if self.param_size == 0 { 0 } else { self.params.len().saturating_sub(4) / self.param_size }
    }
    /// Trigger frame of entry `i` (any value for the terminator).
    pub fn frame_at(&self, off: usize) -> i32 {
        i32_at(&self.params, off)
    }
}

/// An 8- or 4-bit CLUT texture uploaded to GS memory at load time.
#[derive(Debug, Clone)]
pub struct Texture {
    pub tex_dbp: i16,
    pub clut_dbp: i16,
    /// Upload buffer width in 64-pixel units, and upload rectangle origin.
    pub dbw: i16,
    pub x: i16,
    pub y: i16,
    /// GS pixel format (0x13 PSMT8, 0x14 PSMT4).
    pub format: i16,
    pub width: usize,
    pub height: usize,
    pub tex0: u64,
    /// Raw pixel indices as uploaded (8 bits, or 4 bits packed low nibble first).
    pub indices: Vec<u8>,
    /// Raw CLUT (RGBA, alpha 0..0x80) in upload order.
    pub palette: Vec<u8>,
    /// RGBA8 pixels, alpha already expanded from the PS2 0..0x80 range.
    pub rgba: Vec<u8>,
}

impl Texture {
    /// Uploads the pixels and CLUT into GS memory as the game does (BITBLT at the stored DBP).
    pub fn upload(&self, vram: &mut kh2_ps2::gs::Vram) {
        if self.format == 0x13 {
            vram.write8(self.tex_dbp as u16 as usize, self.dbw.max(1) as usize, self.x as usize, self.y as usize, self.width, self.height, &self.indices);
            vram.write32(self.clut_dbp as u16 as usize, 1, 0, 0, 16, 16, &self.palette);
        }
    }
}

/// VSF: a vertex-shape binding (`vsf_no`, model index, vertex index list).
#[derive(Debug, Clone)]
pub struct Vsf {
    pub number: i32,
    pub model: i32,
    pub indices: Vec<u16>,
}

#[derive(Debug)]
pub enum Error {
    BadMagic(&'static str),
    Truncated,
}

impl Pax {
    pub fn parse(b: &[u8]) -> Result<Pax, Error> {
        if b.get(..4) != Some(b"PAX_") {
            return Err(Error::BadMagic("PAX_"));
        }
        let n = i32_at(b, 8) as usize;
        let dpx_off = i32_at(b, 12) as usize;
        let elements = (0..n)
            .map(|i| {
                let o = 16 + i * 0x50;
                Element {
                    effect: u16_at(b, o),
                    id: u16_at(b, o + 2),
                    group: u8_at(b, o + 4),
                    fadeout_frame: u8_at(b, o + 5),
                    bone: i16_at(b, o + 6),
                    category: u64_at(b, o + 8),
                    flags: u32_at(b, o + 0x10),
                    start_wait: f32_at(b, o + 0x14),
                    se: i32_at(b, o + 0x18),
                    translation: [f32_at(b, o + 0x1c), f32_at(b, o + 0x20), f32_at(b, o + 0x24)],
                    rotation: [f32_at(b, o + 0x28), f32_at(b, o + 0x2c), f32_at(b, o + 0x30)],
                    scale: [f32_at(b, o + 0x34), f32_at(b, o + 0x38), f32_at(b, o + 0x3c)],
                    bone_pos: BonePos {
                        a: u16_at(b, o + 0x40),
                        b: u16_at(b, o + 0x42),
                        ratio_a: f32_at(b, o + 0x44),
                        ratio_b: f32_at(b, o + 0x48),
                        adjust: u8_at(b, o + 0x4c),
                    },
                }
            })
            .collect();
        let d = b.get(dpx_off..).ok_or(Error::Truncated)?;
        let count = i32_at(d, 12) as usize;
        let raw: Vec<(usize, usize, u32)> = (0..count)
            .map(|i| {
                let o = 16 + i * 32;
                (u32_at(d, o) as usize, u32_at(d, o + 4) as usize, u32_at(d, o + 8))
            })
            .collect();
        let p = 16 + count * 32;
        let nd = i32_at(d, p) as usize;
        let offs: Vec<usize> = (0..nd).map(|i| i32_at(d, p + 4 + 4 * i) as usize).collect();
        let mut dpds = Vec::with_capacity(nd);
        for (i, &o) in offs.iter().enumerate() {
            let end = offs.get(i + 1).copied().unwrap_or(d.len());
            dpds.push(Dpd::parse(d.get(o..end).ok_or(Error::Truncated)?)?);
        }
        let effects = raw
            .into_iter()
            .map(|(off, pd, number)| DpxEffect { dpd: offs.iter().position(|&o| o == off).unwrap_or(0), particle_data: pd, number })
            .collect();
        Ok(Pax { elements, effects, dpds })
    }

    /// Elements started by game effect id `id`.
    pub fn elements_for(&self, id: u16) -> impl Iterator<Item = &Element> {
        self.elements.iter().filter(move |e| e.id == id)
    }

    /// The DPX entry an element's `effect` word names: the first entry whose number (its third word)
    /// equals it (`FUN_001db5f0` -> `FUN_001da6f0` -> `FUN_001d9bf0` -> `FUN_001d9720` walks the 0x20-byte
    /// list comparing that word). The number is not always the entry's place: `00effect.bar` has 14
    /// entries numbered 0..5 and 11..18.
    pub fn dpx_effect(&self, number: u16) -> Option<&DpxEffect> {
        self.effects.iter().find(|f| f.number == number as u32)
    }
}

impl Dpd {
    /// Shape registered under `number` (the game indexes shapes by their own number).
    pub fn shape(&self, number: i32) -> Option<&crate::shape::Shape> {
        self.shapes.iter().find(|s| s.number as i32 == number)
    }
    pub fn model(&self, number: i32) -> Option<&crate::omd::Model> {
        self.models.iter().find(|m| m.number as i32 == number)
    }

    pub fn parse(b: &[u8]) -> Result<Dpd, Error> {
        if i32_at(b, 0) != 0x96 {
            return Err(Error::BadMagic("DPD 0x96"));
        }
        let mut p = 4;
        let mut lists: Vec<Vec<usize>> = Vec::new();
        for _ in 0..5 {
            let n = i32_at(b, p) as usize;
            lists.push((0..n).map(|k| i32_at(b, p + 4 + 4 * k) as usize).collect());
            p += 4 + 4 * n;
        }
        // Each block ends where the next one (in file order) starts.
        let mut starts: Vec<usize> = lists.iter().flatten().copied().collect();
        starts.push(b.len());
        starts.sort_unstable();
        let end_of = |o: usize| starts.iter().copied().find(|&s| s > o).unwrap_or(b.len());
        let slice = |o: usize| b.get(o..end_of(o)).unwrap_or(&[]);
        Ok(Dpd {
            particles: lists[0].iter().map(|&o| ParticleData::parse(slice(o))).collect::<Result<_, _>>()?,
            textures: lists[1].iter().map(|&o| Texture::parse(slice(o))).collect(),
            shapes: lists[2].iter().map(|&o| crate::shape::Shape::parse(slice(o))).collect(),
            models: lists[3].iter().map(|&o| crate::omd::Model::parse(slice(o))).collect(),
            vsfs: lists[4]
                .iter()
                .map(|&o| {
                    let s = slice(o);
                    let n = i32_at(s, 8).max(0) as usize;
                    Vsf { number: i32_at(s, 0), model: i32_at(s, 4), indices: (0..n).map(|k| u16_at(s, 12 + 2 * k)).collect() }
                })
                .collect(),
        })
    }
}

impl ParticleData {
    pub fn parse(b: &[u8]) -> Result<ParticleData, Error> {
        if b.len() < 0x120 {
            return Err(Error::Truncated);
        }
        let bytes16 = |o: usize| -> [u8; 16] { b[o..o + 16].try_into().unwrap() };
        // Program file at 0x110: life, flags, manager-program list offset, group list offset, groups...
        let f = &b[0x110..];
        let life = i32_at(f, 0);
        let flags = u32_at(f, 4);
        let mp = i32_at(f, 8) as usize;
        let program_kinds = (0..i32_at(f, mp).max(0) as usize).map(|k| u32_at(f, mp + 4 + 4 * k)).collect();
        let gl = i32_at(f, 12) as usize;
        let ngroups = i32_at(f, gl).max(0) as usize;
        let mut groups = Vec::with_capacity(ngroups);
        for k in 0..ngroups {
            let g = i32_at(f, gl + 4 + 4 * k) as usize;
            groups.push(Group::parse(f, g));
        }
        Ok(ParticleData {
            offset: [f32_at(b, 0x80), f32_at(b, 0x84), f32_at(b, 0x88)],
            angle: [i32_at(b, 0x90), i32_at(b, 0x94), i32_at(b, 0x98)],
            scale: [f32_at(b, 0xa0), f32_at(b, 0xa4), f32_at(b, 0xa8)],
            use_texture: bytes16(0xb0),
            use_shape: bytes16(0xc0),
            use_model: bytes16(0xd0),
            use_vsf: bytes16(0xe0),
            work_mem: i32_at(b, 0xf0),
            packet_mem: i32_at(b, 0xf4),
            chr_scale: b[0xf9],
            bind_pos_only: b[0xfa],
            non_ambient: b[0xfb],
            se: u16_at(b, 0xfc),
            se_vol_add: u16_at(b, 0xfe),
            se_wait: f32_at(b, 0x100),
            bone: i32_at(b, 0x104),
            wait: f32_at(b, 0x108),
            life,
            flags,
            program_kinds,
            groups,
        })
    }
}

impl Group {
    fn parse(f: &[u8], g: usize) -> Group {
        let count = i16_at(f, g + 0x26).max(0) as usize;
        let cv_off = i32_at(f, g + 4) as usize;
        let mut programs = Vec::with_capacity(count);
        for k in 0..count {
            let o = g + 0x28 + 16 * k;
            let kind = u32_at(f, o);
            let param_size = u16_at(f, o + 4) as usize;
            let entries = u16_at(f, o + 6) as usize;
            let params_ptr = i32_at(f, o + 8) as usize;
            let use_ptr = i32_at(f, o + 12) as usize;
            // Parameters are followed by the 0xFFFFF000 terminator; useVal runs to the next program's
            // parameters (or to the group's cv-off block).
            let params_end = params_ptr + entries * param_size + 4;
            let use_end = if k + 1 < count { i32_at(f, g + 0x28 + 16 * (k + 1) + 8) as usize } else { cv_off };
            let params = f.get(params_ptr..params_end).unwrap_or(&[]).to_vec();
            let use_val = (use_ptr..use_end.max(use_ptr)).step_by(4).map(|a| i32_at(f, a)).collect();
            programs.push(Program { kind, param_size, params, use_val });
        }
        Group {
            flag: i32_at(f, g + 8),
            status: i32_at(f, g + 12),
            start_frame: i32_at(f, g + 16),
            total_frame: i32_at(f, g + 20),
            loop_start: i32_at(f, g + 24),
            loop_end: i32_at(f, g + 28),
            cursor_offset: i32_at(f, g + 32),
            priority: i16_at(f, g + 36),
            programs,
        }
    }
}

impl Texture {
    pub fn parse(b: &[u8]) -> Texture {
        let format = i16_at(b, 6);
        let width = i16_at(b, 12).max(0) as usize;
        let height = i16_at(b, 14).max(0) as usize;
        let tex0 = u32_at(b, 16) as u64 | (u32_at(b, 20) as u64) << 32;
        let data = &b[b.len().min(0x20)..];
        let mut rgba = vec![0u8; width * height * 4];
        let four = format == 0x14;
        let pal_off = if four { width * height / 2 } else { width * height };
        let color = |idx: usize| -> [u8; 4] {
            let o = pal_off + idx * 4;
            let c = data.get(o..o + 4).unwrap_or(&[0, 0, 0, 0]);
            [c[0], c[1], c[2], (c[3] as u16 * 2).min(255) as u8]
        };
        for i in 0..width * height {
            let idx = if four {
                (data.get(i / 2).copied().unwrap_or(0) >> ((i & 1) * 4)) as usize & 15
            } else {
                // CSM1 palette order: swap bits 3 and 4 of the index.
                let x = data.get(i).copied().unwrap_or(0) as usize;
                (x & 0xe7) | (x & 8) << 1 | (x & 16) >> 1
            };
            rgba[i * 4..i * 4 + 4].copy_from_slice(&color(idx));
        }
        let n = if four { width * height / 2 } else { width * height };
        Texture {
            tex_dbp: i16_at(b, 0),
            clut_dbp: i16_at(b, 2),
            dbw: i16_at(b, 4),
            x: i16_at(b, 8),
            y: i16_at(b, 10),
            format,
            width,
            height,
            tex0,
            indices: data.get(..n).unwrap_or(&[]).to_vec(),
            palette: data.get(n..n + if four { 64 } else { 1024 }).unwrap_or(&[]).to_vec(),
            rgba,
        }
    }
}
