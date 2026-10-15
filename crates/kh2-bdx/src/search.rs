//! The target search of host functions 6.3, 6.4, 6.5 and 10.55 (party members' scripts: Goofy's
//! `tornado` and `turbo`). The game keeps the search state in a block the script owns (its own memory);
//! the runtime reads and writes that block itself and asks the host only for the objects (docs/party.md
//! section 17).
//!
//! The block (FUN_002f01a0 lays it out):
//!
//! | offset | what |
//! |---|---|
//! | +0 | the history's capacity (the argument of 6.3) |
//! | +4 (byte) | the history's next slot (FUN_002f02b0 advances it modulo the capacity) |
//! | +5 (byte) | the target parts the last search walked (FUN_002f04a8 counts every part of every object that passed) |
//! | +4 bit 0x10000 | skip objects with +0x108 bit 0x8000 (10.55, FUN_002f0758) |
//! | +8 | the entries in the history (at most the capacity) |
//! | +0xc | the object kind: 0 = +0x588 bit 0x10, 1 / 2 = +0x588 bit 4, else any (FUN_002f0218) |
//! | +0x10 / +0x14 | the distances a part's point must lie between |
//! | +0x18 | the angle around the direction given (no test unless below `gate`, 0x383fbc) |
//! | +0x1c + 8 i | the history: the object and the part id of an earlier choice |

use crate::host::Vec4;

/// The executable's numbers the search reads: the block's default distance and angle (0x383fb4,
/// 0x383fb8), the angle below which the direction is tested (0x383fbc) and FUN_0017bbc0's pi and 2 pi
/// (0x37a528, 0x37a52c). Read from the user's executable ([`crate::HostTable::from_exe`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchConsts {
    pub max: f32,
    pub angle: f32,
    pub gate: f32,
    pub wrap_pi: f32,
    pub wrap_two_pi: f32,
}

impl SearchConsts {
    /// Addresses of the five numbers, in field order.
    pub const ADDRS: [u32; 5] = [0x383fb4, 0x383fb8, 0x383fbc, 0x37a528, 0x37a52c];
}

/// One target part of an object the host walked (FUN_001b7c88's list, FUN_0016b4e0 type 3): the object's
/// handle, the part's id (byte 0 of its collision record) and its point (FUN_001894d0, game space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetPart {
    pub obj: u32,
    pub part: u8,
    pub point: Vec4,
}

/// The block's fields (see the module table).
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub cap: i32,
    pub next: u8,
    pub walked: u8,
    /// Bytes +6 and +7 of the word at +4 (bit 0 of +6 is 10.55's flag).
    pub flags: u16,
    pub count: i32,
    pub kind: i32,
    pub min: f32,
    pub max: f32,
    pub angle: f32,
    /// The history entries 0..capacity (only the first `count` are tested).
    pub history: Vec<(u32, i32)>,
}

/// Size in bytes of a block with `cap` history entries.
pub fn block_size(cap: i32) -> usize {
    0x1c + 8 * cap.clamp(0, 64) as usize
}

impl Block {
    /// FUN_002f01a0 (6.3): capacity `cap`, the next slot, the walked count and the history count 0, the
    /// kind and the near distance 0, the far distance and the angle from the executable.
    pub fn new(cap: i32, k: &SearchConsts) -> Block {
        Block { cap, next: 0, walked: 0, flags: 0, count: 0, kind: 0, min: 0.0, max: k.max, angle: k.angle, history: vec![(0, 0); cap.clamp(0, 64) as usize] }
    }

    /// The block from its bytes (at least [`block_size`] of the capacity in the first word).
    pub fn read(b: &[u8]) -> Block {
        let w = |o: usize| b.get(o..o + 4).map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap()));
        let cap = w(0) as i32;
        let n = cap.clamp(0, 64) as usize;
        Block {
            cap,
            next: b.get(4).copied().unwrap_or(0),
            walked: b.get(5).copied().unwrap_or(0),
            flags: (w(4) >> 16) as u16,
            count: w(8) as i32,
            kind: w(0xc) as i32,
            min: f32::from_bits(w(0x10)),
            max: f32::from_bits(w(0x14)),
            angle: f32::from_bits(w(0x18)),
            history: (0..n).map(|i| (w(0x1c + 8 * i), w(0x20 + 8 * i) as i32)).collect(),
        }
    }

    /// The block's bytes ([`block_size`] of its capacity).
    pub fn bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(block_size(self.cap));
        b.extend_from_slice(&self.cap.to_le_bytes());
        b.extend_from_slice(&[self.next, self.walked]);
        b.extend_from_slice(&self.flags.to_le_bytes());
        b.extend_from_slice(&self.count.to_le_bytes());
        b.extend_from_slice(&self.kind.to_le_bytes());
        for f in [self.min, self.max, self.angle] {
            b.extend_from_slice(&f.to_bits().to_le_bytes());
        }
        for &(o, p) in &self.history {
            b.extend_from_slice(&o.to_le_bytes());
            b.extend_from_slice(&p.to_le_bytes());
        }
        b
    }

    /// 10.55 (FUN_002f0758): skip objects with +0x108 bit 0x8000 from now on.
    pub fn skips_flagged(&self) -> bool {
        self.flags & 1 != 0
    }

    /// FUN_002f01d8: the part is in the first `count` history entries.
    fn in_history(&self, obj: u32, part: i32) -> bool {
        self.history.iter().take(self.count.max(0) as usize).any(|&(o, p)| o == obj && p == part)
    }

    /// FUN_002f02f8 for one part (FUN_002f0438 with the record found): a part not yet in the history goes
    /// into the next slot, the count grows up to the capacity and the slot advances (FUN_002f02b0). The
    /// object's validity test (FUN_0019a3f8) passes for every object the list walk returned.
    fn remember(&mut self, obj: u32, part: i32) {
        if self.in_history(obj, part) || self.cap <= 0 {
            return;
        }
        if let Some(e) = self.history.get_mut(self.next as usize) {
            *e = (obj, part);
        }
        if self.count < self.cap {
            self.count += 1;
        }
        let mut n = self.next.wrapping_add(1);
        while self.cap <= n as i32 {
            n = n.wrapping_sub(self.cap as u8);
        }
        self.next = n;
    }
}

/// FUN_0017bbc0: an angle wrapped into -pi..pi with the executable's pi and 2 pi and the C library's
/// `fmodf` (FUN_00300418).
pub fn wrap(x: f32, k: &SearchConsts) -> f32 {
    if x > 0.0 { (x + k.wrap_pi) % k.wrap_two_pi - k.wrap_pi } else { k.wrap_pi - (k.wrap_pi - x) % k.wrap_two_pi }
}

/// FUN_002f04a8 after the object tests: walks the target parts in the list's order, counts them into
/// byte +5, skips those in the history (when `history`), measures the distance from `point` to each
/// part's point (without its height when `flat`), keeps those between +0x10 and +0x14 and, when +0x18 is
/// below the gate, those whose ground direction differs from `dir`'s by less than +0x18 (atan2 of x and
/// z, FUN_003003e8; the difference wrapped by FUN_0017bbc0), and answers the nearest (or, without
/// `nearest`, the farthest; the first of equals). With `history` the answer goes into the history.
/// `None`: the record is left cleared (FUN_0018ed40).
pub fn search(block: &mut Block, parts: &[TargetPart], point: Vec4, dir: Vec4, nearest: bool, flat: bool, history: bool, k: &SearchConsts) -> Option<(u32, u8)> {
    block.walked = 0;
    let mut best: Option<(u32, u8)> = None;
    let mut best_d = 0.0f32;
    for p in parts {
        block.walked = block.walked.wrapping_add(1);
        if history && block.in_history(p.obj, p.part as i32) {
            continue;
        }
        let mut d: [f32; 4] = std::array::from_fn(|i| p.point[i] - point[i]);
        if flat {
            d[1] = 0.0;
        }
        let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if !(block.min <= dist && dist <= block.max) {
            continue;
        }
        if block.angle < k.gate {
            let turn = wrap(dir[0].atan2(dir[2]) - d[0].atan2(d[2]), k).abs();
            if block.angle <= turn {
                continue;
            }
        }
        let v = if nearest { dist } else { -dist };
        if best.is_none() || v < best_d {
            best = Some((p.obj, p.part));
            best_d = v;
        }
    }
    if history && let Some((o, p)) = best {
        block.remember(o, p as i32);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k() -> SearchConsts {
        let pi = std::f32::consts::PI;
        SearchConsts { max: 1000.0, angle: pi, gate: pi, wrap_pi: pi, wrap_two_pi: 2.0 * pi }
    }

    fn part(obj: u32, part: u8, x: f32, z: f32) -> TargetPart {
        TargetPart { obj, part, point: [x, 0.0, z, 1.0] }
    }

    /// The nearest part in range, the farthest without `nearest`; the history skips earlier answers and
    /// fills its slots round the capacity.
    #[test]
    fn search_nearest_farthest_and_history() {
        let k = k();
        let mut b = Block::new(2, &k);
        b.max = 500.0;
        let parts = [part(1, 0, 300.0, 0.0), part(2, 4, 100.0, 0.0), part(3, 0, 600.0, 0.0)];
        let o = [0.0, 0.0, 0.0, 1.0];
        let fwd = [0.0, 0.0, 1.0, 0.0];
        assert_eq!(search(&mut b, &parts, o, fwd, true, true, false, &k), Some((2, 4)));
        assert_eq!(b.walked, 3);
        assert_eq!(search(&mut b, &parts, o, fwd, false, true, false, &k), Some((1, 0)));
        assert_eq!(search(&mut b, &parts, o, fwd, true, true, true, &k), Some((2, 4)));
        assert_eq!(search(&mut b, &parts, o, fwd, true, true, true, &k), Some((1, 0)));
        assert_eq!((b.count, b.next), (2, 0));
        assert_eq!(search(&mut b, &parts, o, fwd, true, true, true, &k), None);
        // The bytes round-trip.
        assert_eq!(Block::read(&b.bytes()), b);
    }

    /// Below the gate only parts within the angle of the direction are kept.
    #[test]
    fn search_angle_limits_to_the_direction() {
        let k = k();
        let mut b = Block::new(0, &k);
        b.angle = 0.5;
        let parts = [part(1, 0, 100.0, 0.0), part(2, 0, 0.0, 200.0)];
        assert_eq!(search(&mut b, &parts, [0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0], true, true, false, &k), Some((2, 0)));
        assert_eq!(search(&mut b, &parts, [0.0, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 0.0], true, true, false, &k), Some((1, 0)));
    }
}
