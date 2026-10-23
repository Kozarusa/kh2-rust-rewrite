//! Pad scripts: the input both sides of the rig are fed, keyed on `k` (vsyncs since the state was taken).
//!
//! ```text
//! # kh2 pad v1      k  buttons            lx  ly  rx  ry
//! 0                 -                     127 127 127 127
//! 40                -                     127 0   127 127
//! 120               -                     255 127 127 127
//! 240               start
//! end 330
//! ```
//!
//! A line holds until the next one. `buttons` is `-` or names joined by `+`: `up down left right start select l1 r1 l2 r2 l3 r3
//! triangle circle cross square`. The sticks follow in the order left x, left y, right x, right y; left out they are the centre (127).
//! `end K` ends the run at `k = K` (the last line is then not needed). `#` starts a comment.
//!
//! The original reads the pad once per game frame, which in the field is every 2 vsyncs: a field script should change only on even `k`
//! ([`PadScript::odd_changes`] lists the violations; a change at an odd `k` is seen one vsync late).
//!
//! PCSX2 side: the 6 bytes of [`Frame::bytes`] are written at the pad read (`0x167370`, see `client`). Byte 0 is Select, L3, R3, Start,
//! Up, Right, Down, Left (bits 0..7), byte 1 is L2, R2, L1, R1, Triangle, Circle, Cross, Square, both active low; then right x, right y,
//! left x, left y (centre 0x7f). Idle reads back as `ff ff 7f 7f 7f 7f` (local/specs/RIG.md 1.3).

/// Button names in bit order: bits 0..7 of byte 0, then bits 0..7 of byte 1.
pub const BUTTONS: [&str; 16] = ["select", "l3", "r3", "start", "up", "right", "down", "left", "l2", "r2", "l1", "r1", "triangle", "circle", "cross", "square"];

/// The pad at one moment: pressed buttons as a mask over [`BUTTONS`] (bit i = `BUTTONS[i]`), and the four stick axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub buttons: u16,
    pub lx: u8,
    pub ly: u8,
    pub rx: u8,
    pub ry: u8,
}

pub const CENTRE: u8 = 0x7f;

impl Default for Frame {
    fn default() -> Self {
        Frame { buttons: 0, lx: CENTRE, ly: CENTRE, rx: CENTRE, ry: CENTRE }
    }
}

impl Frame {
    /// The bytes the game's pad buffer holds: two active-low button bytes, then right x, right y, left x, left y.
    pub fn bytes(&self) -> [u8; 6] {
        let m = !self.buttons;
        [m as u8, (m >> 8) as u8, self.rx, self.ry, self.lx, self.ly]
    }

    pub fn pressed(&self, name: &str) -> bool {
        BUTTONS.iter().position(|b| *b == name).is_some_and(|i| self.buttons >> i & 1 != 0)
    }
}

/// A parsed script.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PadScript {
    /// `(k, frame)` in increasing k.
    pub entries: Vec<(u32, Frame)>,
    /// `end K`.
    pub end: Option<u32>,
}

impl PadScript {
    pub fn parse(text: &str) -> Result<PadScript, String> {
        let mut s = PadScript::default();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let at = |m: &str| format!("pad script line {}: {m}", n + 1);
            let mut w = line.split_whitespace();
            let first = w.next().unwrap_or("");
            if first == "end" {
                let k = w.next().and_then(|v| v.parse().ok()).ok_or_else(|| at("`end` needs a number"))?;
                if s.end.replace(k).is_some() {
                    return Err(at("`end` twice"));
                }
                continue;
            }
            let k: u32 = first.parse().map_err(|_| at(&format!("'{first}' is not a k")))?;
            if s.entries.last().is_some_and(|(p, _)| *p >= k) {
                return Err(at(&format!("k {k} does not increase")));
            }
            let mut f = Frame::default();
            for name in w.next().unwrap_or("-").split('+').filter(|b| *b != "-") {
                let i = BUTTONS.iter().position(|b| *b == name.to_ascii_lowercase()).ok_or_else(|| at(&format!("unknown button '{name}'")))?;
                f.buttons |= 1 << i;
            }
            let mut axes = [CENTRE; 4];
            for (i, v) in w.by_ref().take(4).enumerate() {
                axes[i] = v.parse().map_err(|_| at(&format!("'{v}' is not a stick value 0..255")))?;
            }
            if let Some(extra) = w.next() {
                return Err(at(&format!("unexpected '{extra}'")));
            }
            [f.lx, f.ly, f.rx, f.ry] = axes;
            s.entries.push((k, f));
        }
        if s.entries.is_empty() && s.end.is_none() {
            return Err("pad script: no lines".into());
        }
        Ok(s)
    }

    /// The pad at `k`: the last entry at or before it; `None` before the first entry (the real pad is left alone).
    pub fn at(&self, k: u32) -> Option<Frame> {
        let i = self.entries.partition_point(|(p, _)| *p <= k);
        i.checked_sub(1).map(|i| self.entries[i].1)
    }

    /// The last k of the run: `end`, else the last entry.
    pub fn last_k(&self) -> u32 {
        self.end.or_else(|| self.entries.last().map(|e| e.0)).unwrap_or(0)
    }

    /// The entries that change the pad at an odd `k` (the field reads every second vsync).
    pub fn odd_changes(&self) -> Vec<u32> {
        self.entries.iter().map(|e| e.0).filter(|k| k % 2 == 1).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "# kh2 pad v1\n0 - 127 127 127 127\n40 - 127 0 127 127   # up\n120 - 255 127 127 127\n200 start+cross\n204 -\nend 330\n";

    #[test]
    fn a_script_parses_and_holds_each_line_until_the_next() {
        let s = PadScript::parse(SCRIPT).unwrap();
        assert_eq!(s.entries.len(), 5);
        assert_eq!(s.end, Some(330));
        assert_eq!(s.last_k(), 330);
        assert_eq!(s.at(39).unwrap(), Frame::default());
        assert_eq!(s.at(40).unwrap().ly, 0);
        assert_eq!(s.at(119).unwrap().ly, 0);
        assert_eq!(s.at(120).unwrap().lx, 255);
        assert!(s.at(200).unwrap().pressed("start") && s.at(203).unwrap().pressed("cross"));
        assert_eq!(s.at(204).unwrap().buttons, 0);
        assert_eq!(s.at(204).unwrap().lx, 127, "sticks left out are the centre");
    }

    #[test]
    fn before_the_first_entry_the_real_pad_is_left_alone() {
        let s = PadScript::parse("10 -\nend 20").unwrap();
        assert_eq!(s.at(9), None);
        assert!(s.at(10).is_some());
    }

    #[test]
    fn the_bytes_are_the_games_pad_buffer() {
        // Idle: ffff7f7f7f7f (RIG.md 1.3). Stick up: left y 0. Start (bit 3 of byte 0): f7.
        assert_eq!(Frame::default().bytes(), [0xff, 0xff, 0x7f, 0x7f, 0x7f, 0x7f]);
        let up = Frame { ly: 0, ..Frame::default() };
        assert_eq!(up.bytes(), [0xff, 0xff, 0x7f, 0x7f, 0x7f, 0x00]);
        let start = PadScript::parse("0 start").unwrap().at(0).unwrap();
        assert_eq!(start.bytes()[0], 0xf7);
        // Cross is bit 6 of byte 1, L1 bit 2, Left bit 7 of byte 0.
        let f = PadScript::parse("0 cross+l1+left").unwrap().at(0).unwrap();
        assert_eq!(f.bytes()[1], !(1 << 6 | 1 << 2));
        assert_eq!(f.bytes()[0], !(1u8 << 7));
    }

    #[test]
    fn bad_scripts_name_the_line() {
        for (text, needle) in [("0 jump", "unknown button"), ("5 -\n5 -", "does not increase"), ("x -", "not a k"), ("0 - 1 2 3 4 5", "unexpected"), ("0 - 300", "stick value"), ("end", "needs a number"), ("# nothing", "no lines")] {
            let e = PadScript::parse(text).unwrap_err();
            assert!(e.contains(needle), "{text:?} gave {e}");
        }
    }

    #[test]
    fn odd_changes_are_listed() {
        assert_eq!(PadScript::parse("0 -\n41 up\n42 -").unwrap().odd_changes(), vec![41]);
    }
}
