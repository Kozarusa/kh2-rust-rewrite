//! The BDX container: a 16 byte name, three sizes, an entry table, then 16-bit code words.

use std::fmt;

/// Byte offset of the first code word. All code-relative offsets count 16-bit words from here.
pub const CODE_BASE: usize = 0x10;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Shorter than the fixed header, or the entry table runs off the end.
    Truncated,
    /// Odd length: code is made of 16-bit words.
    OddLength,
    /// An entry point lies outside the code.
    EntryOutOfRange { id: u32, offset: u32 },
    /// Code reachable from an entry point does not decode.
    BadCode { pc: usize, word: u16 },
    /// Control flow leaves the code area.
    FlowOutOfRange { pc: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated => write!(f, "truncated header"),
            Error::OddLength => write!(f, "odd length"),
            Error::EntryOutOfRange { id, offset } => write!(f, "entry {id} offset {offset:#x} outside the code"),
            Error::BadCode { pc, word } => write!(f, "undecodable word {word:#06x} at code word {pc:#x}"),
            Error::FlowOutOfRange { pc } => write!(f, "control flow leaves the code at word {pc:#x}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryPoint {
    /// Event id the game uses to start this entry (see docs/bdx.md).
    pub id: u32,
    /// Code word index of the first instruction.
    pub pc: u32,
}

#[derive(Debug, Clone)]
pub struct Program {
    /// Object or area name stored in the first 16 bytes (`m_ex020`, `00common`...).
    pub name: String,
    /// Bytes of global (per-thread-group) memory the script wants.
    pub globals: u32,
    /// Bytes of frame (local variable) stack.
    pub frame_stack: u32,
    /// Bytes of operand stack.
    pub value_stack: u32,
    pub entries: Vec<EntryPoint>,
    /// Everything after the 16 byte name, as 16-bit words. The header words are included so that
    /// word indices match the game's (entry table lives at words 6.., code follows it).
    pub words: Vec<u16>,
}

impl Program {
    pub fn parse(data: &[u8]) -> Result<Program, Error> {
        if data.len() < 0x20 {
            return Err(Error::Truncated);
        }
        if !data.len().is_multiple_of(2) {
            return Err(Error::OddLength);
        }
        let u32_at = |o: usize| data.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).ok_or(Error::Truncated);
        let name_end = data[..CODE_BASE].iter().position(|&b| b == 0).unwrap_or(CODE_BASE);
        let name = String::from_utf8_lossy(&data[..name_end]).into_owned();
        let words: Vec<u16> = data[CODE_BASE..].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        // Entry table: {id, offset} pairs from byte 0x1c, ended by a zero offset (the game's lookup stops there).
        let mut entries = Vec::new();
        let mut o = 0x1c;
        loop {
            let (id, off) = (u32_at(o)?, u32_at(o + 4)?);
            if off == 0 {
                break;
            }
            entries.push(EntryPoint { id, pc: off });
            o += 8;
        }
        let code_len = words.len() as u32;
        for e in &entries {
            if e.pc >= code_len {
                return Err(Error::EntryOutOfRange { id: e.id, offset: e.pc });
            }
        }
        Ok(Program { name, globals: u32_at(0x10)?, frame_stack: u32_at(0x14)?, value_stack: u32_at(0x18)?, entries, words })
    }

    /// First entry with this event id.
    pub fn entry(&self, id: u32) -> Option<u32> {
        self.entries.iter().find(|e| e.id == id).map(|e| e.pc)
    }

    /// Word index where the entry table ends (the terminator pair is skipped).
    pub fn table_end(&self) -> usize {
        // 0x1c is word 6; each entry is 4 words; the terminator pair adds 4 more.
        6 + 4 * self.entries.len() + 4
    }

    pub fn word(&self, pc: usize) -> Option<u16> {
        self.words.get(pc).copied()
    }

    /// Little-endian 32-bit value at a word index.
    pub fn u32_at_word(&self, pc: usize) -> Option<u32> {
        Some(self.words.get(pc)?.to_owned() as u32 | (*self.words.get(pc + 1)? as u32) << 16)
    }
}
