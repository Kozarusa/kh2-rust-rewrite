//! Player attack tables: which motion each attack action plays and when it may be chosen.
//!
//! The table is the `ptya` subfile of `00battle.bin`. A character's table is picked by the
//! `NeoMoveset` field of its `00objentry` row. Layout (read from the game's lookup and selection
//! routines; OpenKH leaves the subfile undocumented): a u32 version, a u32 table count, then one
//! u32 offset per table (from the start of the subfile). A table is a u32 record count followed by
//! 0x44-byte records.
//!
//! Record fields that the selection code uses:
//!
//! | offset | meaning |
//! |---|---|
//! | 0 | action key; other code asks for an action by key (guard, the default first attack, ...) |
//! | 1 | candidate group: 3 = always a candidate, 1 = candidate for the locked target by range, 2 = candidate for other threatening enemies by range, 0 = only chosen explicitly by key |
//! | 2 | alternative key (signed, -1 none): tried when this action's motion is already playing |
//! | 3 | argument of the finisher test |
//! | 4 | flags: bit 0 airborne only, bit 1 and bits 3, 4 adjust the height window, bit 2 finisher |
//! | 8 | motion ID played |
//! | 10 | motion ID queued after it (4, the fall, for air attacks) |
//! | 0x20, 0x24 | horizontal distance window (min inclusive, max exclusive) |
//! | 0x28, 0x2c | height window, Y down (upper bound inclusive, lower bound exclusive) |
//! | 0x30, 0x34 | window on the dot product of the facing/lock direction with the direction to the candidate |
//! | 0x40 | ability bit that must be set (0 none) |

use std::sync::Arc;

pub const RECORD_SIZE: usize = 0x44;

/// One action of a player's attack table.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub key: u8,
    pub group: u8,
    pub alt: i8,
    pub arg: u8,
    pub flags: u32,
    pub motion: u16,
    pub next_motion: u16,
    pub distance: [f32; 2],
    pub height: [f32; 2],
    pub angle: [f32; 2],
    pub ability: u16,
}

impl Action {
    /// The action may only be chosen while airborne (flag bit 0).
    pub fn air_only(&self) -> bool {
        self.flags & 1 != 0
    }
    /// A finisher: chosen once the combo is long enough or a finisher window is open (flag bit 2).
    pub fn finisher(&self) -> bool {
        self.flags & 4 != 0
    }
}

/// One character's attack table.
#[derive(Debug, Clone, Default)]
pub struct Moveset {
    pub actions: Vec<Action>,
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn f32_at(b: &[u8], o: usize) -> Option<f32> {
    u32_at(b, o).map(f32::from_bits)
}

impl Moveset {
    /// Table `index` of a `ptya` subfile; `None` when the index is out of range or unused.
    pub fn parse(ptya: &[u8], index: usize) -> Option<Moveset> {
        let count = u32_at(ptya, 4)? as usize;
        if index >= count {
            return None;
        }
        let at = u32_at(ptya, 8 + 4 * index)? as usize;
        if at == 0 {
            return None;
        }
        let n = u32_at(ptya, at)? as usize;
        let mut actions = Vec::with_capacity(n);
        for i in 0..n {
            let r = ptya.get(at + 4 + RECORD_SIZE * i..at + 4 + RECORD_SIZE * (i + 1))?;
            actions.push(Action {
                key: r[0],
                group: r[1],
                alt: r[2] as i8,
                arg: r[3],
                flags: u32_at(r, 4)?,
                motion: u16_at(r, 8)?,
                next_motion: u16_at(r, 10)?,
                distance: [f32_at(r, 0x20)?, f32_at(r, 0x24)?],
                height: [f32_at(r, 0x28)?, f32_at(r, 0x2c)?],
                angle: [f32_at(r, 0x30)?, f32_at(r, 0x34)?],
                ability: u16_at(r, 0x40)?,
            });
        }
        Some(Moveset { actions })
    }

    /// The first action with this key.
    pub fn by_key(&self, key: u8) -> Option<&Action> {
        self.actions.iter().find(|a| a.key == key)
    }

    /// The game's choice among the candidate actions (`candidate` marks the flagged ones): the first
    /// flagged action whose motion differs from the one playing, or whose alternative key is its own;
    /// when every flagged action is already playing, the first one's alternative.
    pub fn pick(&self, candidate: impl Fn(&Action) -> bool, playing: usize) -> Option<&Action> {
        let mut fallback = None;
        for a in self.actions.iter().filter(|a| candidate(a)) {
            if a.alt as i32 == a.key as i32 || a.motion as usize != playing {
                return Some(a);
            }
            if fallback.is_none() && a.alt >= 0 {
                fallback = self.by_key(a.alt as u8);
            }
        }
        fallback
    }
}

/// A shared handle for components.
pub type SharedMoveset = Arc<Moveset>;

#[cfg(test)]
mod tests {
    use super::*;

    fn record(key: u8, group: u8, alt: i8, flags: u32, motion: u16) -> Vec<u8> {
        let mut r = vec![0u8; RECORD_SIZE];
        r[0] = key;
        r[1] = group;
        r[2] = alt as u8;
        r[4..8].copy_from_slice(&flags.to_le_bytes());
        r[8..10].copy_from_slice(&motion.to_le_bytes());
        r
    }

    fn table(records: &[Vec<u8>]) -> Vec<u8> {
        // Version, one table at offset 12.
        let mut b = vec![];
        b.extend(2u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(12u32.to_le_bytes());
        b.extend((records.len() as u32).to_le_bytes());
        for r in records {
            b.extend(r);
        }
        b
    }

    #[test]
    fn parses_records() {
        let t = table(&[record(5, 3, 6, 0, 151), record(8, 3, 7, 5, 183)]);
        let m = Moveset::parse(&t, 0).unwrap();
        assert_eq!(m.actions.len(), 2);
        assert_eq!((m.actions[1].key, m.actions[1].motion, m.actions[1].alt), (8, 183, 7));
        assert!(m.actions[1].air_only() && m.actions[1].finisher() && !m.actions[0].finisher());
        assert!(Moveset::parse(&t, 1).is_none());
    }

    #[test]
    fn pick_prefers_other_motion_then_alternative() {
        let t = table(&[record(5, 3, 6, 0, 151), record(6, 1, -1, 0, 153)]);
        let m = Moveset::parse(&t, 0).unwrap();
        // Only key 5 is a candidate. From idle it is chosen; while it plays its alternative (key 6) is.
        assert_eq!(m.pick(|a| a.key == 5, 0).unwrap().key, 5);
        assert_eq!(m.pick(|a| a.key == 5, 151).unwrap().key, 6);
        assert!(m.pick(|a| a.key == 6, 153).is_none());
    }
}
