//! Motion triggers: frame ranges and single frames that switch gameplay state during a motion
//! (hitboxes, combo windows, gravity, turning, effects). IDs per OpenKH `docs/kh2/file/anb/anb.md`.

#[derive(Debug, Clone)]
pub struct RangeTrigger {
    pub start: i16,
    pub end: i16,
    pub id: u8,
    pub params: Vec<i16>,
}

#[derive(Debug, Clone)]
pub struct FrameTrigger {
    pub frame: i16,
    pub id: u8,
    pub params: Vec<i16>,
}

#[derive(Debug, Clone, Default)]
pub struct Triggers {
    pub ranges: Vec<RangeTrigger>,
    pub frames: Vec<FrameTrigger>,
}

/// Range trigger IDs used by kh2-rs.
pub mod range {
    pub const GROUNDED: u8 = 0;
    pub const AIR: u8 = 1;
    pub const BLEND_TO_IDLE: u8 = 2;
    pub const NO_GRAVITY: u8 = 3;
    pub const ATTACK: u8 = 10;
    pub const ALLOW_COMBO: u8 = 11;
    pub const WEAPON_SWING: u8 = 12;
    pub const TURN_TO_TARGET: u8 = 22;
    pub const NO_GRAVITY_KEEP_KINETICS: u8 = 24;
    pub const INVINCIBLE: u8 = 27;
    pub const TURN_TO_LOCK: u8 = 28;
    pub const ATTACK_COMBO: u8 = 33;
    pub const ALLOW_MOVEMENT: u8 = 41;
    pub const KEEP_MOMENTUM: u8 = 42;
    pub const ALLOW_FINISHER: u8 = 50;
}

fn i16_at(b: &[u8], o: usize) -> i16 {
    b.get(o..o + 2).map(|s| i16::from_le_bytes([s[0], s[1]])).unwrap_or(0)
}

impl Triggers {
    pub fn parse(b: &[u8]) -> Triggers {
        let (nr, nf) = (*b.first().unwrap_or(&0) as usize, *b.get(1).unwrap_or(&0) as usize);
        let foff = i16_at(b, 2) as usize * 2; // offset in shorts
        let mut t = Triggers::default();
        let mut p = 4;
        for _ in 0..nr {
            let n = *b.get(p + 5).unwrap_or(&0) as usize;
            t.ranges.push(RangeTrigger {
                start: i16_at(b, p),
                end: i16_at(b, p + 2),
                id: *b.get(p + 4).unwrap_or(&0),
                params: (0..n).map(|k| i16_at(b, p + 6 + 2 * k)).collect(),
            });
            p += 6 + 2 * n;
        }
        let mut p = if foff >= 4 { foff } else { p };
        for _ in 0..nf {
            let n = *b.get(p + 3).unwrap_or(&0) as usize;
            t.frames.push(FrameTrigger {
                frame: i16_at(b, p),
                id: *b.get(p + 2).unwrap_or(&0),
                params: (0..n).map(|k| i16_at(b, p + 4 + 2 * k)).collect(),
            });
            p += 4 + 2 * n;
        }
        t
    }

    /// Range triggers active at `frame` (end inclusive; end -1 means "until the motion ends").
    pub fn active(&self, frame: f32) -> impl Iterator<Item = &RangeTrigger> {
        self.ranges.iter().filter(move |r| frame >= r.start as f32 && (r.end < 0 || frame <= r.end as f32))
    }

    pub fn is_active(&self, frame: f32, id: u8) -> bool {
        self.active(frame).any(|r| r.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_range_triggers() {
        // Header: 1 range trigger, 0 frame triggers. Range: frames 15-23, id 10 (attack), params [126, 1].
        let mut b = vec![1u8, 0, 0, 0];
        for v in [15i16, 23] {
            b.extend(v.to_le_bytes());
        }
        b.extend([10u8, 2]);
        for v in [126i16, 1] {
            b.extend(v.to_le_bytes());
        }
        let t = Triggers::parse(&b);
        assert!(t.frames.is_empty());
        assert_eq!(t.ranges.len(), 1);
        assert_eq!((t.ranges[0].start, t.ranges[0].end, t.ranges[0].id), (15, 23, 10));
        assert_eq!(t.ranges[0].params, vec![126, 1]);
        assert!(t.is_active(20.0, 10) && !t.is_active(24.0, 10));
    }
}
