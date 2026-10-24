//! The directory scans of the file layer: the root listing into the slot table, the per-save file list and its checks.
//!
//! The IOP library's listing is a table of 0x40-byte entries (`DAT_0036cb20`): `+8` the modify time (8 bytes), `+0x10` the size, halfword
//! `+0x14` the attributes (0x20 = directory), `+0x20` the name (32 bytes). The count of the last listing comes from the card status table
//! (`FUN_002319f8`); here it is an argument.

use crate::body::{BODY_LEN, ICON_LEN, ICON_SYS_LEN, kb, space_needed};
use crate::names::{NameEnv, name_for};
use crate::slot::{CardInfo, RECORDS};

/// Length of a listing entry.
pub const ENTRY_LEN: usize = 0x40;
/// Entries the directory table holds (0x1900 bytes).
pub const TABLE_ENTRIES: usize = 100;
/// Length of the per-save file list (`DAT_0036cb1c`).
pub const LIST_LEN: usize = 0xf80;
/// Length of a file list entry.
pub const LIST_ENTRY_LEN: usize = 0x28;
/// Length of the name buffer (`DAT_0036cb0c`).
pub const NAME_LEN: usize = 0x20;

/// The unit's buffers as `FUN_0023c948` allocates them: the name buffer, the path buffer, the file buffer (the larger of the icon and the body
/// file, in whole KB), the card info block (byte `+0` 0xff, halfword `+2` 0xffff), the file list and the directory table.
///
/// orig 0x23c948
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Buffers {
    pub name: NameBuf,
    pub path: [u8; 0x12],
    pub file: Vec<u8>,
    pub info: CardInfo,
    pub list: Vec<u8>,
    pub table: Vec<u8>,
}

impl Default for Buffers {
    fn default() -> Self {
        let kb_len = kb(ICON_LEN as i32).max(kb(BODY_LEN as i32)) as usize * 1024;
        Buffers { name: NameBuf::default(), path: [0; 0x12], file: vec![0; kb_len], info: CardInfo::default(), list: vec![0; LIST_LEN], table: vec![0; TABLE_ENTRIES * ENTRY_LEN] }
    }
}

/// The directory table's start: the allocation rounded up to a multiple of 0x40 (the listing needs a 64-byte aligned buffer).
///
/// orig 0x23c948
pub fn align_table(allocated: u32) -> u32 {
    if allocated & 0x3f == 0 { allocated } else { allocated - (allocated & 0x3f) + 0x40 }
}

/// The name buffer: a NUL-terminated string in 0x20 bytes. Setting a name copies it with its NUL and leaves the rest as it was.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NameBuf(pub [u8; NAME_LEN]);

impl Default for NameBuf {
    fn default() -> Self {
        NameBuf([0; NAME_LEN])
    }
}

impl NameBuf {
    /// Copies `s` and a NUL into the buffer (what the string copy routine `FUN_00312078` does).
    pub fn set(&mut self, s: &[u8]) {
        let n = s.len().min(NAME_LEN - 1);
        self.0[..n].copy_from_slice(&s[..n]);
        self.0[n] = 0;
    }

    /// The string up to the first NUL.
    pub fn bytes(&self) -> &[u8] {
        cstr(&self.0)
    }
}

/// The bytes of a NUL-terminated string.
pub fn cstr(b: &[u8]) -> &[u8] {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    &b[..n]
}

/// The two expected files besides the body (the body is named like its directory) with their sizes: the table `FUN_0023e360(1, 0xffff)`
/// fills at `0x1dc0dd8` (called through `FUN_0023e3b8`), whose last word holds the body size.
///
/// orig 0x23e360, 0x23e3b8
pub fn expected_files() -> [(&'static str, u32); 2] {
    [("icon.sys", ICON_SYS_LEN as u32), ("kh2.ico", ICON_LEN as u32)]
}

fn entry(table: &[u8], i: usize) -> &[u8] {
    &table[i * ENTRY_LEN..(i + 1) * ENTRY_LEN]
}

fn e_name(e: &[u8]) -> &[u8] {
    cstr(&e[0x20..0x40])
}

fn e_attr(e: &[u8]) -> u16 {
    u16::from_le_bytes([e[0x14], e[0x15]])
}

fn e_size(e: &[u8]) -> u32 {
    u32::from_le_bytes(e[0x10..0x14].try_into().unwrap())
}

/// Puts the name for the info block's cursor into the name buffer (`FUN_0023cc98(info[1])`, the cursor read as a signed byte).
fn name_from_cursor(env: &NameEnv, info: &CardInfo, buf: &mut NameBuf) {
    let cur = name_for(env, i64::from(info.cursor() as i8), std::str::from_utf8(buf.bytes()).unwrap_or(""));
    if info.cursor() as i8 != -5 {
        buf.set(cur.as_bytes());
    }
}

/// After a root listing: clears the 99 records and the cursor, then for every directory entry whose name is 15 characters with a digit at
/// index 13 marks the slot `name[13] * 10 + name[14] - 0x210` as existing and copies the modify time. Returns the entry count.
///
/// The digit test is an unsigned compare of `name[13] - 0x30` against 10 (the oracle: a byte below `0x30` does not pass), and `name[14]` is
/// not checked at all. A name whose two characters give a slot below 0 or above 98 would write outside the block in the original (slot -1
/// lands on the block's own header); the port ignores it [inferred: no listing of a real card holds one].
///
/// orig 0x23d578
pub fn scan_root(info: &mut CardInfo, table: &[u8], count: i32) -> i32 {
    info.clear_records();
    info.set_cursor(0);
    for i in 0..count.max(0) as usize {
        let e = entry(table, i);
        let name = e_name(e);
        if e_attr(e) & 0x20 != 0 && name.len() == 0xf && name[13].wrapping_sub(0x30) < 10 {
            let slot = i32::from(name[13]) * 10 + i32::from(name[14]) - 0x210;
            if (0..99).contains(&slot) {
                let o = RECORDS + slot as usize * 0x20;
                info.0[o..o + 4].copy_from_slice(&1u32.to_le_bytes());
                info.0[o + 0x18..o + 0x20].copy_from_slice(&e[8..16]);
            }
        }
    }
    count
}

/// After listing one save directory: builds the file list. Entries named `.` or `..` are skipped (left as they were), the others get kind
/// 1 (directory) or 2 (file), the size and the name. The name buffer is first set for the cursor. Returns the entry count.
///
/// orig 0x23d6c8
pub fn build_list(env: &NameEnv, info: &CardInfo, name: &mut NameBuf, list: &mut [u8], table: &[u8], count: i32) -> i32 {
    name_from_cursor(env, info, name);
    list[..4].fill(0);
    list[..4].copy_from_slice(&count.to_le_bytes());
    for i in 0..count.max(0) as usize {
        let e = entry(table, i);
        let n = e_name(e);
        if n != b".." && n != b"." {
            let o = 8 + i * LIST_ENTRY_LEN;
            let kind: u32 = if e_attr(e) & 0x20 == 0 { 2 } else { 1 };
            list[o..o + 4].copy_from_slice(&kind.to_le_bytes());
            list[o + 4..o + 8].copy_from_slice(&e_size(e).to_le_bytes());
            list[o + 8..o + 8 + n.len()].copy_from_slice(n);
            list[o + 8 + n.len()] = 0;
        }
    }
    count
}

fn l_count(list: &[u8]) -> i32 {
    i32::from_le_bytes(list[..4].try_into().unwrap())
}

fn l_kind(list: &[u8], i: usize) -> u32 {
    let o = 8 + i * LIST_ENTRY_LEN;
    u32::from_le_bytes(list[o..o + 4].try_into().unwrap())
}

fn l_set_kind(list: &mut [u8], i: usize, v: u32) {
    let o = 8 + i * LIST_ENTRY_LEN;
    list[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

fn l_size(list: &[u8], i: usize) -> u32 {
    let o = 8 + i * LIST_ENTRY_LEN + 4;
    u32::from_le_bytes(list[o..o + 4].try_into().unwrap())
}

fn l_name(list: &[u8], i: usize) -> &[u8] {
    let o = 8 + i * LIST_ENTRY_LEN + 8;
    cstr(&list[o..o + 0x20])
}

/// Checks the file list against the three expected files. Returns bits: 1 `icon.sys`, 2 `kh2.ico`, 4 the body file (named like the
/// directory, `name`), each also with 0x100 / 0x200 / 0x400 when its size is wrong, and 0x10 for any other file. A wrong or unknown file
/// gets bit 0x10 in its entry's kind word.
///
/// orig 0x23d820
pub fn check_list(list: &mut [u8], name: &[u8]) -> u32 {
    let mut acc = 0u32;
    for i in 0..l_count(list).max(0) as usize {
        if l_kind(list, i) & 2 == 0 || l_name(list, i).is_empty() {
            continue;
        }
        let n = l_name(list, i).to_vec();
        let size = l_size(list, i);
        let (bit, want) = if n == b"icon.sys" {
            (1, Some(ICON_SYS_LEN as u32))
        } else if n == b"kh2.ico" {
            (2, Some(ICON_LEN as u32))
        } else if n == name {
            (4, Some(BODY_LEN as u32))
        } else {
            (0x10, None)
        };
        acc |= bit;
        if want.is_some_and(|w| w == size) {
            continue;
        }
        if want.is_some() {
            acc |= bit << 8;
        }
        let k = l_kind(list, i) | 0x10;
        l_set_kind(list, i, k);
    }
    acc
}

/// Clears the current entry of the file list.
///
/// orig 0x23d9a8
pub fn clear_current(list: &mut [u8]) {
    let cur = i32::from_le_bytes(list[4..8].try_into().unwrap()) as usize;
    let o = cur * LIST_ENTRY_LEN + 8;
    list[o..o + LIST_ENTRY_LEN].fill(0);
}

/// The first entry marked bad (kind bit 0x10): it becomes the current entry and its name goes into the name buffer. `-1` when none.
///
/// orig 0x23d9e0
pub fn first_bad(list: &mut [u8], name: &mut NameBuf) -> i32 {
    for i in 0..l_count(list).max(0) as usize {
        if l_kind(list, i) & 0x10 != 0 {
            list[4..8].copy_from_slice(&(i as i32).to_le_bytes());
            let n = l_name(list, i).to_vec();
            name.set(&n);
            return i as i32;
        }
    }
    -1
}

/// Counts the three expected files in the listing of the save directory and adds up their KB. `exact`: the sizes must be equal; else
/// they must be at least the expected ones. Exact wants all three; otherwise all three, or a total of at least the space a save needs.
/// The name buffer is first set for the cursor.
///
/// orig 0x23e080
pub fn files_complete(env: &NameEnv, info: &CardInfo, name: &mut NameBuf, table: &[u8], count: i32, exact: bool) -> bool {
    name_from_cursor(env, info, name);
    let mut found = 0;
    let mut sum = 0u32;
    for i in 0..count.max(0) as usize {
        let e = entry(table, i);
        let n = e_name(e);
        if n == b".." || n == b"." {
            continue;
        }
        let size = e_size(e);
        sum = sum.wrapping_add(kb(size as i32));
        let ok = |want: u32| if exact { size == want } else { size >= want };
        if n == name.bytes() {
            if ok(BODY_LEN as u32) {
                found += 1;
            }
        } else {
            // The two expected files, both tried for every other name.
            if n == b"icon.sys" && ok(ICON_SYS_LEN as u32) {
                found += 1;
            }
            if n == b"kh2.ico" && ok(ICON_LEN as u32) {
                found += 1;
            }
        }
    }
    if exact { found == 3 } else { !(found != 3 && (sum as i32) < space_needed()) }
}
