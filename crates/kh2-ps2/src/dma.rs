//! DMA source-chain walking. A tag is 16 bytes: u16 QWC, bits 28..30 = ID, u32 ADDR, then two
//! VIF codes that are transferred ahead of the tag's data. All addresses are relative to `base`.

use crate::{rd16, rd32};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub qwc: usize,
    pub id: u32,
    pub addr: usize,
}

pub fn tag_at(b: &[u8], o: usize) -> Tag {
    Tag { qwc: rd16(b, o) as usize, id: (rd32(b, o) >> 28) & 7, addr: (rd32(b, o + 4) & 0x7FFF_FFFF) as usize }
}

/// Follows a chain starting at `start` and returns the VIF byte stream it would transfer.
/// IDs: 0 refe, 1 cnt, 2 next, 3 ref, 4 refs, 5 call (treated as next), 6 ret, 7 end.
pub fn chain_to_vif(b: &[u8], start: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut p = start;
    for _ in 0..100_000 {
        if p + 16 > b.len() {
            break;
        }
        let t = tag_at(b, p);
        out.extend_from_slice(&b[p + 8..p + 16]);
        let n = t.qwc * 16;
        let inline = || b.get(p + 16..p + 16 + n).unwrap_or(&[]);
        match t.id {
            1 => {
                out.extend_from_slice(inline());
                p += 16 + n;
            }
            2 | 5 => {
                out.extend_from_slice(inline());
                p = t.addr;
            }
            3 | 4 => {
                out.extend_from_slice(b.get(t.addr..t.addr + n).unwrap_or(&[]));
                p += 16;
            }
            0 => {
                out.extend_from_slice(b.get(t.addr..t.addr + n).unwrap_or(&[]));
                break;
            }
            _ => {
                out.extend_from_slice(inline());
                break;
            }
        }
    }
    out
}
