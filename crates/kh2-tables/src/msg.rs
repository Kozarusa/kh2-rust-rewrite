//! System message text (`msg/<lang>/sys.bar`, entry `sys`): item, ability and command names and
//! descriptions. Layout: u32 version, u32 count, then count x {u32 id, u32 offset}; each text is
//! NUL-terminated at its offset inside the entry. Table fields that name text (`Item.Name`,
//! `Cmd.MessageId`) hold the id with bit 15 set; the id in this file is the low 15 bits.

use std::collections::HashMap;

pub struct Messages {
    data: Vec<u8>,
    index: HashMap<u32, usize>,
}

impl Messages {
    /// Parses the BAR that holds the `sys` entry.
    pub fn from_bar(bar: &[u8]) -> Option<Messages> {
        let entries = kh2_bar::parse(bar).ok()?;
        let sys = kh2_bar::find(&entries, "sys")?.data;
        let n = u32::from_le_bytes(sys.get(4..8)?.try_into().ok()?) as usize;
        let mut index = HashMap::with_capacity(n);
        for i in 0..n {
            let p = 8 + 8 * i;
            let id = u32::from_le_bytes(sys.get(p..p + 4)?.try_into().ok()?);
            let off = u32::from_le_bytes(sys.get(p + 4..p + 8)?.try_into().ok()?) as usize;
            index.insert(id, off);
        }
        Some(Messages { data: sys.to_vec(), index })
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Text for a table field (bit 15 is ignored). Control codes print as `<xx>`.
    pub fn get(&self, field: u32) -> Option<String> {
        let off = *self.index.get(&(field & 0x7fff))?;
        let tail = self.data.get(off..)?;
        let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
        Some(decode(&tail[..end]))
    }
}

/// The in-game character set, as far as names need it: space, digits, upper and lower case
/// letters and a few punctuation marks. Everything else is shown as `<xx>`.
pub fn decode(b: &[u8]) -> String {
    let mut s = String::new();
    for &c in b {
        match c {
            0x01 => s.push(' '),
            0x02 => s.push('/'),
            0x21..=0x2a => s.push((b'0' + (c - 0x21)) as char),
            0x2e..=0x47 => s.push((b'A' + (c - 0x2e)) as char),
            0x9a..=0xb3 => s.push((b'a' + (c - 0x9a)) as char),
            0x4f => s.push('.'),
            0x48 => s.push(','),
            0x49 => s.push('!'),
            0x50 => s.push('?'),
            0x66 => s.push('-'),
            _ => s.push_str(&format!("<{c:02x}>")),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_letters_and_space() {
        // "Hi Ab1" in the game's code points.
        assert_eq!(decode(&[0x35, 0xa2, 0x01, 0x2e, 0x9b, 0x22]), "Hi Ab1");
        assert_eq!(decode(&[0x0b]), "<0b>");
    }

    #[test]
    fn indexes_a_synthetic_sys_entry() {
        let mut sys = Vec::new();
        sys.extend_from_slice(&1u32.to_le_bytes());
        sys.extend_from_slice(&1u32.to_le_bytes());
        sys.extend_from_slice(&760u32.to_le_bytes());
        sys.extend_from_slice(&16u32.to_le_bytes());
        sys.extend_from_slice(&[0x3d, 0xa8, 0]);
        let mut bar = b"BAR\x01".to_vec();
        bar.extend_from_slice(&1u32.to_le_bytes());
        bar.extend_from_slice(&[0; 8]);
        bar.extend_from_slice(&2u16.to_le_bytes());
        bar.extend_from_slice(&0u16.to_le_bytes());
        bar.extend_from_slice(b"sys\0");
        bar.extend_from_slice(&32u32.to_le_bytes());
        bar.extend_from_slice(&(sys.len() as u32).to_le_bytes());
        bar.extend_from_slice(&sys);
        let m = Messages::from_bar(&bar).unwrap();
        assert_eq!(m.get(760 | 0x8000).as_deref(), Some("Po"));
        assert!(m.get(1).is_none());
    }
}
