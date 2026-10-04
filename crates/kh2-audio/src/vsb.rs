//! VSB: a bank of voice lines (`"IopVoice"`). Header: magic, u32 count at 0xC, then `count`
//! (offset, size) pairs from 0x10 (0xFFFFFFFF = empty); offsets are relative to the end of that
//! table. Each line is a Sony VAG file (`"VAGp"`, big-endian header: size at 0xC, sample rate at
//! 0x10, ADPCM from 0x30).

use crate::adpcm::{self, Sample};

#[derive(Clone, Debug)]
pub struct Vsb {
    data: Vec<u8>,
    lines: Vec<Option<(usize, usize)>>,
}

impl Vsb {
    pub fn parse(b: &[u8]) -> Option<Vsb> {
        if b.get(..8) != Some(b"IopVoice") {
            return None;
        }
        let u32_at = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
        let count = u32_at(0xc)? as usize;
        let base = 0x10 + 8 * count;
        let mut lines = Vec::with_capacity(count);
        for i in 0..count {
            let (off, size) = (u32_at(0x10 + 8 * i)?, u32_at(0x14 + 8 * i)?);
            lines.push((off != 0xffff_ffff && size != 0xffff_ffff).then_some((base + off as usize, size as usize)));
        }
        Some(Vsb { data: b.to_vec(), lines })
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Line `index` decoded, with its sample rate.
    pub fn line(&self, index: usize) -> Option<(Sample, u32)> {
        let (o, size) = (*self.lines.get(index)?)?;
        let vag = self.data.get(o..(o + size).min(self.data.len()))?;
        if vag.get(..4) != Some(b"VAGp") {
            return None;
        }
        let rate = u32::from_be_bytes(vag.get(0x10..0x14)?.try_into().ok()?);
        Some((adpcm::decode(vag, 0x30), rate))
    }
}
