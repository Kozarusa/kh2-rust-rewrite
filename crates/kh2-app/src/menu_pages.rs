//! The pause menu's pages. Stage M1 has page 0, the top list (docs/systems/ui/menus-pause.md 2.1): its entry table,
//! which rows are shown, and where a row leads. Every table is read from the user's executable at
//! run time; nothing of it is copied here.
//!
//! The entry table (0x3808a8, eight entries of 0x24 bytes; `FUN_00253d38` decides the shown rows):
//! +0 and +8 four u16 animation group ids each (the row's idle and its selected group set), +0x12
//! the name message, +0x14 the help message (bit 15 of both ignored), +0x1c up to four story-flag
//! ids that must all be set (a negative byte ends the list), +0x20 up to four that must all be
//! clear. A flag id is bit n of the word array at 0x33fdb0 (`FUN_001a5328`). Entry 1 (Abilities) is
//! not decided by flags: it is shown when any party member's first ability slot is not empty.

use std::collections::BTreeSet;

/// Address of the entry table, the size of an entry and the count (`FUN_00251100` loops to 8).
pub const TABLE: u32 = 0x0038_08a8;
pub const ENTRY_SIZE: u32 = 0x24;
pub const ENTRIES: usize = 8;

/// The page ids of `FUN_00251848`'s jump table at 0x380794 (`docs/systems/ui/menus-pause.md` 1.4; a disc test reads
/// the table and checks them). `None`: the entry leaves the camp for the Journal scene (kind 3).
pub const PAGE_OF_ENTRY: [Option<u16>; ENTRIES] = [Some(0x01), Some(0x0f), Some(0x19), Some(0x14), Some(0x17), None, None, Some(0x24)];

/// The executable's loadable segments, for reading its tables by virtual address.
pub struct Exe {
    data: Vec<u8>,
    /// (virtual address, file offset, file size, memory size) of each PT_LOAD segment.
    segs: Vec<(u32, usize, usize, usize)>,
}

impl Exe {
    pub fn load(data: Vec<u8>) -> Option<Exe> {
        let rd = |o: usize| data.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
        let rd16 = |o: usize| data.get(o..o + 2).map(|s| u16::from_le_bytes(s.try_into().unwrap()) as usize);
        if !data.starts_with(b"\x7fELF") {
            return None;
        }
        let (phoff, phentsize, phnum) = (rd(0x1c)? as usize, rd16(0x2a)?, rd16(0x2c)?);
        let mut segs = Vec::new();
        for i in 0..phnum {
            let o = phoff + i * phentsize;
            if rd(o)? == 1 {
                segs.push((rd(o + 8)?, rd(o + 4)? as usize, rd(o + 16)? as usize, rd(o + 20)? as usize));
            }
        }
        Some(Exe { data, segs })
    }

    pub fn u8(&self, va: u32) -> Option<u8> {
        for &(v, off, fsz, msz) in &self.segs {
            if va >= v && ((va - v) as usize) < msz {
                let k = (va - v) as usize;
                return Some(if k < fsz { *self.data.get(off + k)? } else { 0 });
            }
        }
        None
    }

    pub fn u16(&self, va: u32) -> Option<u16> {
        Some(u16::from(self.u8(va)?) | u16::from(self.u8(va + 1)?) << 8)
    }
}

/// One entry of the top list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The row's idle group set: (enter, leave, loop, unused); the row starts in the first and, when
    /// it finishes, goes on to the third (`FUN_00296b48` writes the third as the follow-up group).
    pub idle: [u16; 4],
    /// The same for the selected row.
    pub selected: [u16; 4],
    pub name: u32,
    pub help: u32,
    /// Story flags that must all be set / all be clear.
    pub need: Vec<u16>,
    pub deny: Vec<u16>,
}

impl Entry {
    /// Entry `i` of the table.
    pub fn read(exe: &Exe, i: usize) -> Option<Entry> {
        let at = TABLE + ENTRY_SIZE * i as u32;
        let groups = |o: u32| -> Option<[u16; 4]> { Some([exe.u16(at + o)?, exe.u16(at + o + 2)?, exe.u16(at + o + 4)?, exe.u16(at + o + 6)?]) };
        let flags = |o: u32| -> Option<Vec<u16>> {
            let mut out = Vec::new();
            for k in 0..4 {
                let b = exe.u8(at + o + k)? as i8;
                if b < 0 {
                    break;
                }
                out.push(b as u16);
            }
            Some(out)
        };
        Some(Entry {
            idle: groups(0)?,
            selected: groups(8)?,
            name: u32::from(exe.u16(at + 0x12)? & 0x7fff),
            help: u32::from(exe.u16(at + 0x14)? & 0x7fff),
            need: flags(0x1c)?,
            deny: flags(0x20)?,
        })
    }
}

/// The whole table.
pub fn read_table(exe: &Exe) -> Option<Vec<Entry>> {
    (0..ENTRIES).map(|i| Entry::read(exe, i)).collect()
}

/// The story flags the menu tests (a set of flag ids). The app has no story: its baseline is a
/// player who has everything the list gates on (docs/systems/core-runtime/progress.md), so the default sets 0x70..0x73.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flags(pub BTreeSet<u16>);

impl Default for Flags {
    fn default() -> Flags {
        Flags((0x70..=0x73).collect())
    }
}

impl Flags {
    /// `KH2_MENU_FLAGS=0x70,0x72,...` replaces the set (a harness aid for looking at the other rows).
    pub fn from_env() -> Flags {
        match std::env::var("KH2_MENU_FLAGS") {
            Ok(v) => Flags(v.split(',').filter_map(|t| u16::from_str_radix(t.trim().trim_start_matches("0x"), 16).ok()).collect()),
            Err(_) => Flags::default(),
        }
    }

    pub fn has(&self, id: u16) -> bool {
        self.0.contains(&id)
    }
}

/// The entries the list shows, as indices into the table, in order (`FUN_00253d38`): all of an
/// entry's `need` flags set and none of its `deny` flags; entry 1 (Abilities) by `has_ability`.
pub fn visible(entries: &[Entry], flags: &Flags, has_ability: bool) -> Vec<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|&(i, e)| if i == 1 { has_ability } else { e.need.iter().all(|&f| flags.has(f)) && e.deny.iter().all(|&f| !flags.has(f)) })
        .map(|(i, _)| i)
        .collect()
}

/// The page a row opens (`None`: the Journal scene), and what M1 does with it: nothing yet.
pub fn page_of(entry: usize) -> Option<u16> {
    PAGE_OF_ENTRY.get(entry).copied().flatten()
}

/// The two numbers of the Munny plate and the clock of the Time plate, as the plates show them.
/// Munny has up to six digit slots (`FUN_0029db30` fills six), Time shows hours and two-digit
/// minutes (`FUN_0029e060` fills nine slots; the footage shows `5:37`) [inferred: the footage].
pub fn digits(n: u32, slots: usize) -> Vec<u8> {
    let s = n.to_string();
    let s = if s.len() > slots { "9".repeat(slots) } else { s };
    s.bytes().map(|b| b - b'0').collect()
}

/// Hours and minutes of `seconds` of play time.
pub fn clock(seconds: u64) -> (u32, u32) {
    ((seconds / 3600).min(999) as u32, ((seconds / 60) % 60) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(need: &[u16], deny: &[u16]) -> Entry {
        Entry { idle: [0; 4], selected: [0; 4], name: 0, help: 0, need: need.to_vec(), deny: deny.to_vec() }
    }

    /// The table of docs/systems/ui/menus-pause.md 2.1 with its flag rules, built by hand: Items needs 0x70, Customize
    /// 0x72, Party 0x71, the Jiminy Journal 0x73, the Roxas Journal needs 0x73 clear.
    fn table() -> Vec<Entry> {
        vec![
            entry(&[0x70], &[]),
            entry(&[], &[]),
            entry(&[0x72], &[]),
            entry(&[0x71], &[]),
            entry(&[], &[]),
            entry(&[0x73], &[]),
            entry(&[], &[0x73]),
            entry(&[], &[]),
        ]
    }

    #[test]
    fn the_baseline_flags_show_every_row_but_the_roxas_journal() {
        let rows = visible(&table(), &Flags::default(), true);
        assert_eq!(rows, vec![0, 1, 2, 3, 4, 5, 7]);
    }

    #[test]
    fn flag_0x71_clear_hides_party() {
        let mut f = Flags::default();
        f.0.remove(&0x71);
        let rows = visible(&table(), &f, true);
        assert!(!rows.contains(&3), "Party (entry 3) is hidden: {rows:?}");
        assert_eq!(rows, vec![0, 1, 2, 4, 5, 7]);
        // The Roxas prologue list of the footage: no flags at all.
        let rows = visible(&table(), &Flags(BTreeSet::new()), true);
        assert_eq!(rows, vec![1, 4, 6, 7], "Abilities, Status, the Roxas Journal, Config");
    }

    #[test]
    fn abilities_follow_the_party_not_a_flag() {
        assert!(!visible(&table(), &Flags::default(), false).contains(&1));
        assert!(visible(&table(), &Flags(BTreeSet::new()), true).contains(&1));
    }

    #[test]
    fn rows_lead_to_the_pages_of_the_doc() {
        assert_eq!(page_of(0), Some(1));
        assert_eq!(page_of(3), Some(0x14));
        assert_eq!(page_of(5), None);
        assert_eq!(page_of(7), Some(0x24));
    }

    #[test]
    fn plate_digits_and_clock() {
        assert_eq!(digits(615, 6), vec![6, 1, 5]);
        assert_eq!(digits(0, 6), vec![0]);
        assert_eq!(digits(12_345_678, 6), vec![9; 6]);
        assert_eq!(clock(5 * 3600 + 37 * 60 + 59), (5, 37));
    }

    /// The real table from the user's executable (skipped without `KH2_ISO`): the flag rules and the
    /// pages are the ones the doc states.
    #[test]
    fn the_executables_table_hides_party_with_flag_0x71_clear() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let Some(elf) = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()) else { return };
        let exe = Exe::load(elf).expect("an ELF");
        let t = read_table(&exe).expect("the entry table");
        assert_eq!(t.len(), ENTRIES);
        assert_eq!(t[0].need, vec![0x70]);
        assert_eq!(t[2].need, vec![0x72]);
        assert_eq!(t[3].need, vec![0x71]);
        assert_eq!(t[5].need, vec![0x73]);
        assert_eq!(t[6].deny, vec![0x73]);
        let mut f = Flags::default();
        assert_eq!(visible(&t, &f, true), vec![0, 1, 2, 3, 4, 5, 7]);
        f.0.remove(&0x71);
        let rows = visible(&t, &f, true);
        assert!(!rows.contains(&3), "Party hidden: {rows:?}");
        // The row order of the footage with Roxas: Items.. no flags: Abilities, Status, Journal, Config.
        assert_eq!(visible(&t, &Flags(BTreeSet::new()), true), vec![1, 4, 6, 7]);
        // The jump table at 0x380794 leads where PAGE_OF_ENTRY says.
        let word = |va: u32| u32::from(exe.u16(va).unwrap()) | u32::from(exe.u16(va + 2).unwrap()) << 16;
        let page = |target: u32| {
            let w0 = word(target);
            let w = if w0 >> 16 == 0x1000 { word(target + 4) } else { w0 };
            (w >> 26 == 9 && (w >> 16) & 31 == 16).then_some((w & 0xffff) as u16)
        };
        for (i, want) in PAGE_OF_ENTRY.iter().enumerate() {
            if let Some(p) = want {
                assert_eq!(page(word(0x0038_0794 + 4 * i as u32)), Some(*p), "entry {i}");
            }
        }
    }
}
