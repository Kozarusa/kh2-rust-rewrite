//! Host function signatures read from the user's own game executable at runtime.
//!
//! The game keeps a table of group pointers; each group is an array of `{function address, flags}`
//! where the low 16 bits of the flags are the argument count and bit 30 says the function returns a
//! value. The scripts do not carry argument counts, so the interpreter needs them to pop the right
//! number of words. Nothing from the table is stored in this repository beyond what docs/bdx.md
//! describes for the functions that were analysed.

use std::collections::HashMap;

/// Address of the group pointer table in the USA executable (SLUS_210.05).
const GROUP_TABLE: u32 = 0x35bce8;
const MAX_GROUPS: u32 = 12;
const TEXT_START: u32 = 0x10_0000;
const TEXT_END: u32 = 0x34_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signature {
    /// Address of the native function in the executable.
    pub address: u32,
    pub nargs: u16,
    pub returns: bool,
}

#[derive(Debug, Default, Clone)]
pub struct HostTable {
    entries: HashMap<(u16, u16), Signature>,
}

impl HostTable {
    pub fn get(&self, group: u16, index: u16) -> Option<Signature> {
        self.entries.get(&(group, index)).copied()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Reads the static tables out of an ELF32 image of the USA executable. Groups that the game fills
    /// in at run time (for example group 3) are absent.
    pub fn from_exe(elf: &[u8]) -> Option<HostTable> {
        let u16_ = |o: usize| Some(u16::from_le_bytes(elf.get(o..o + 2)?.try_into().ok()?) as usize);
        let u32_ = |o: usize| Some(u32::from_le_bytes(elf.get(o..o + 4)?.try_into().ok()?));
        if elf.get(..4)? != b"\x7fELF" {
            return None;
        }
        let phoff = u32_(0x1c)? as usize;
        let (phentsize, phnum) = (u16_(0x2a)?, u16_(0x2c)?);
        let mut segs = Vec::new();
        for i in 0..phnum {
            let p = phoff + i * phentsize;
            if u32_(p)? == 1 {
                segs.push((u32_(p + 8)?, u32_(p + 4)? as usize, u32_(p + 16)?));
            }
        }
        let word = |va: u32| -> Option<u32> {
            let (base, off, _) = *segs.iter().find(|(b, _, s)| va >= *b && va + 4 <= *b + *s)?;
            u32_(off + (va - base) as usize)
        };
        let mut starts: Vec<u32> = (0..MAX_GROUPS).filter_map(|g| word(GROUP_TABLE + 4 * g)).filter(|&p| (0x30_0000..0x40_0000).contains(&p)).collect();
        starts.sort_unstable();
        let mut entries = HashMap::new();
        for g in 0..MAX_GROUPS {
            let Some(base) = word(GROUP_TABLE + 4 * g).filter(|p| starts.contains(p)) else { continue };
            let limit = starts.iter().copied().find(|&s| s > base).unwrap_or(base + 0x2000);
            let mut a = base;
            let mut i = 0u16;
            while a + 8 <= limit {
                let (func, flags) = (word(a)?, word(a + 4)?);
                let valid = (func == 0 && flags == 0) || ((TEXT_START..TEXT_END).contains(&func) && flags & 0xbfff_0000 == 0);
                if !valid {
                    break;
                }
                if func != 0 {
                    entries.insert((g as u16, i), Signature { address: func, nargs: (flags & 0xffff) as u16, returns: flags & 0x4000_0000 != 0 });
                }
                a += 8;
                i += 1;
            }
        }
        Some(HostTable { entries })
    }
}
