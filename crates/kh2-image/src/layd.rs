//! `.2ld` layout archives: the images and layouts of the menu screens (docs/menus.md section 1.3.1).
//!
//! A `.2ld` is a BAR of two entries: type 29 `IMGZ` (the images) and type 28 `LAYD` (the layout).
//! The menu's loader (`FUN_0024f558`, `FUN_0024f908`) names an animation group of one of the embedded
//! sequences, never a pixel; the layers below say which group sits where.
//!
//! **IMGZ** (the offset-table form, not the back-to-back form of [`crate::imgd::split_imgz`]): magic,
//! 0x100, 0x10, the image count, then one (offset, size) pair per image at 0x10; each image is an
//! ordinary IMGD.
//!
//! **LAYD**: magic, 0x100, then three (count, offset) pairs, all u32: the **layers** (0x18 bytes: kind,
//! SEQD index, animation group, 0, x, y), the **layouts** (0x14 bytes: u16 first layer, u16 layer
//! count, zeros) and the **SEQD offsets** (each SEQD runs to the next offset or the end).

use crate::imgd::{self, Image};
use crate::seqd::{self, Seqd};

/// One layer: an animation group of one embedded sequence, at a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layer {
    pub kind: u32,
    /// Index of the embedded SEQD.
    pub seq: usize,
    /// Animation group in that SEQD.
    pub group: usize,
    pub x: i32,
    pub y: i32,
}

/// A parsed LAYD.
#[derive(Debug, Clone, Default)]
pub struct Layd {
    pub layers: Vec<Layer>,
    /// (first layer, layer count) per layout.
    pub layouts: Vec<(usize, usize)>,
    /// The embedded sequences, in file order.
    pub seqs: Vec<Seqd>,
}

impl Layd {
    /// The layers of layout `n` (empty when there is no such layout).
    pub fn layout(&self, n: usize) -> &[Layer] {
        match self.layouts.get(n) {
            Some(&(first, count)) => self.layers.get(first..first + count).unwrap_or(&[]),
            None => &[],
        }
    }
}

fn u32_at(b: &[u8], o: usize) -> Option<usize> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()) as usize)
}

/// Reads the LAYD entry. `None` when the magic is wrong, a table runs past the data or an embedded
/// sequence does not parse.
pub fn parse_layd(b: &[u8]) -> Option<Layd> {
    if !b.starts_with(b"LAYD") {
        return None;
    }
    let (n_l, o_l, n_p, o_p, n_s, o_s) = (u32_at(b, 8)?, u32_at(b, 12)?, u32_at(b, 16)?, u32_at(b, 20)?, u32_at(b, 24)?, u32_at(b, 28)?);
    let mut layers = Vec::new();
    for i in 0..n_l {
        let o = o_l + 0x18 * i;
        layers.push(Layer { kind: u32_at(b, o)? as u32, seq: u32_at(b, o + 4)?, group: u32_at(b, o + 8)?, x: u32_at(b, o + 16)? as u32 as i32, y: u32_at(b, o + 20)? as u32 as i32 });
    }
    let mut layouts = Vec::new();
    for i in 0..n_p {
        let o = o_p + 0x14 * i;
        let (first, count) = (b.get(o..o + 2)?, b.get(o + 2..o + 4)?);
        layouts.push((u16::from_le_bytes(first.try_into().ok()?) as usize, u16::from_le_bytes(count.try_into().ok()?) as usize));
    }
    let mut offs = Vec::new();
    for i in 0..n_s {
        offs.push(u32_at(b, o_s + 4 * i)?);
    }
    let mut seqs = Vec::new();
    for (i, &o) in offs.iter().enumerate() {
        let end = offs.get(i + 1).copied().unwrap_or(b.len());
        seqs.push(seqd::parse(b.get(o..end)?)?);
    }
    Some(Layd { layers, layouts, seqs })
}

/// The IMGDs of an IMGZ in the offset-table form (empty when it is not one).
pub fn split_imgz(b: &[u8]) -> Vec<&[u8]> {
    if !b.starts_with(b"IMGZ") {
        return Vec::new();
    }
    let Some(n) = u32_at(b, 12) else { return Vec::new() };
    (0..n)
        .filter_map(|i| {
            let (o, s) = (u32_at(b, 0x10 + 8 * i)?, u32_at(b, 0x14 + 8 * i)?);
            b.get(o..o.checked_add(s)?)
        })
        .collect()
}

/// A whole `.2ld`: its decoded images and its layout.
#[derive(Debug, Clone)]
pub struct TwoLd {
    pub images: Vec<Image>,
    pub layd: Layd,
}

/// Reads a `.2ld` file (the BAR bytes). `None` when it lacks the IMGZ or the LAYD entry, or an image
/// does not decode.
pub fn parse_2ld(data: &[u8]) -> Option<TwoLd> {
    let entries = kh2_bar::parse(data).ok()?;
    let imgz = entries.iter().find(|e| e.data.starts_with(b"IMGZ"))?;
    let layd = entries.iter().find(|e| e.data.starts_with(b"LAYD"))?;
    let images = split_imgz(imgz.data).into_iter().map(imgd::decode).collect::<Option<Vec<_>>>()?;
    Some(TwoLd { images, layd: parse_layd(layd.data)? })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(b: &mut Vec<u8>, v: u32) {
        b.extend_from_slice(&v.to_le_bytes());
    }

    #[test]
    fn rejects_other_data() {
        assert!(parse_layd(b"SEQD0000").is_none());
        assert!(parse_layd(b"LAYD").is_none());
        assert!(split_imgz(b"IMGD").is_empty());
        assert!(parse_2ld(&[]).is_none());
    }

    #[test]
    fn layers_and_layouts_are_read_by_the_documented_offsets() {
        // header 0x20, two layers at 0x20, one layout at 0x50, no sequences.
        let mut b = b"LAYD".to_vec();
        for v in [0x100, 2, 0x20, 1, 0x50, 0, 0x64] {
            put(&mut b, v);
        }
        for (kind, seq, group, x, y) in [(1u32, 0u32, 7u32, 5i32, -6i32), (2, 0, 9, 0, 0)] {
            for v in [kind, seq, group, 0, x as u32, y as u32] {
                put(&mut b, v);
            }
        }
        b.extend_from_slice(&[0, 0, 2, 0]);
        b.resize(0x64, 0);
        let l = parse_layd(&b).expect("parses");
        assert_eq!(l.layers.len(), 2);
        assert_eq!(l.layers[0], Layer { kind: 1, seq: 0, group: 7, x: 5, y: -6 });
        assert_eq!(l.layouts, vec![(0, 2)]);
        assert_eq!(l.layout(0).len(), 2);
        assert!(l.layout(1).is_empty());
    }
}
