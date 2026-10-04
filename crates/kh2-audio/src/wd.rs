//! WD: Square's PS2 instrument bank ("Wave Data"). Header: `"WD"`, u16 bank id, u32 sample
//! section size, u32 instrument count, u32 region count; at 0x20 one offset per instrument
//! (0xFFFFFFFF ends the list); each instrument is a run of 0x20-byte regions (key ranges); the
//! ADPCM sample section follows the last region.
//!
//! Layout per VGMTrans `src/main/formats/SquarePS2/WD.cpp` (zlib license); see CREDITS.md.

use crate::adpcm::{self, Sample};

#[derive(Clone, Debug)]
pub struct Wd {
    pub id: u16,
    pub instruments: Vec<Instrument>,
    /// The ADPCM sample section.
    pub samples: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Instrument {
    pub regions: Vec<Region>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Region {
    pub stereo: bool,
    pub key_low: u8,
    pub key_high: u8,
    /// Byte offset into the sample section.
    pub sample: usize,
    pub loop_start: usize,
    pub adsr1: u16,
    pub adsr2: u16,
    /// Fine tune in cents (−50..+50).
    pub fine_cents: f32,
    /// Key at which the sample plays at its recorded rate.
    pub unity_key: i32,
    pub volume: f32,
    /// −1 (left) .. 1 (right).
    pub pan: f32,
    /// Highest velocity this region answers to (byte 0x15).
    pub velocity_high: u8,
    /// The pitch word the SSL driver adds to the note (u32 at region +0x10).
    pub tune: u32,
    /// Raw volume (0x16) and pan (0x17) bytes.
    pub volume_raw: u8,
    pub pan_raw: u8,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).unwrap_or(0)
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).unwrap_or(0)
}

/// Fine-tune table: 0x10000 · 2^(i/(12·256))-style steps over one semitone (VGMTrans).
fn fine_cents(v: u8) -> f32 {
    // 256 steps spanning one semitone, centred: (ratio − 1) mapped to cents, minus 50.
    let ratio = 2f32.powf(v as f32 / (256.0 * 12.0));
    1200.0 * ratio.log2() - 50.0
}

impl Wd {
    pub fn parse(b: &[u8]) -> Option<Wd> {
        if b.get(..2) != Some(b"WD") {
            return None;
        }
        let id = u16_at(b, 2);
        let sample_size = u32_at(b, 4) as usize;
        let mut n = u32_at(b, 8) as usize;
        let region_count = u32_at(b, 0xc) as usize;
        let offs: Vec<usize> = (0..n).map(|i| u32_at(b, 0x20 + 4 * i)).take_while(|&o| o != 0xffff_ffff).map(|o| o as usize).collect();
        n = offs.len();
        let sample_off = offs.first().copied().unwrap_or(0x20) + region_count * 0x20;
        let samples = b.get(sample_off..(sample_off + sample_size).min(b.len())).unwrap_or(&[]).to_vec();
        let mut instruments = Vec::with_capacity(n);
        for i in 0..n {
            let end = if i + 1 < n { offs[i + 1] } else { sample_off };
            let mut regions: Vec<Region> = Vec::new();
            let mut o = offs[i];
            while o + 0x20 <= end {
                let flags = b[o + 1];
                let pan_raw = b[o + 0x17];
                let pan = match pan_raw {
                    255 => 1.0,
                    128 => -1.0,
                    192 => 0.0,
                    p if p > 127 => (p as f32 - 128.0) / 127.0 * 2.0 - 1.0,
                    _ => 0.0,
                };
                let mut r = Region {
                    stereo: b[o] & 1 != 0,
                    key_low: 0,
                    key_high: b[o + 0x14],
                    sample: (u32_at(b, o + 4) & !0xf) as usize,
                    loop_start: u32_at(b, o + 8) as usize,
                    adsr1: u16_at(b, o + 0xc),
                    adsr2: u16_at(b, o + 0xe),
                    fine_cents: fine_cents(b[o + 0x12]),
                    unity_key: 0x3a - b[o + 0x13] as i32,
                    volume: b[o + 0x16] as f32 / 127.0,
                    pan,
                    velocity_high: b[o + 0x15],
                    tune: u32_at(b, o + 0x10),
                    volume_raw: b[o + 0x16],
                    pan_raw,
                };
                r.key_low = if flags & 1 != 0 {
                    0
                } else if let Some(prev) = regions.last() {
                    if r.key_high == prev.key_high { prev.key_low } else { prev.key_high.saturating_add(1) }
                } else {
                    0
                };
                if flags & 2 != 0 {
                    r.key_high = 0x7f;
                }
                regions.push(r);
                o += 0x20;
            }
            instruments.push(Instrument { regions });
        }
        Some(Wd { id, instruments, samples })
    }

    /// The region of `program` that covers `key`.
    pub fn region(&self, program: usize, key: u8) -> Option<&Region> {
        let inst = self.instruments.get(program)?;
        inst.regions.iter().find(|r| key >= r.key_low && key <= r.key_high).or_else(|| inst.regions.last())
    }

    pub fn sample(&self, r: &Region) -> Sample {
        adpcm::decode(&self.samples, r.sample)
    }
}
