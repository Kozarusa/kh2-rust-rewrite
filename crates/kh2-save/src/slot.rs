//! The card info block (`DAT_0036cb18`, 0xc68 bytes) and its slot table of 99 records.
//!
//! Layout: `+0` the card port, `+1` the cursor slot (a signed byte), halfword `+2` the free space in KB (negative: unknown), byte `+4`
//! a flag, then 99 records of 0x20 bytes from `+8`. A record: `+0` a flag word (bit 0 a directory exists, bit 1 set by the caller, bit 4
//! a valid save is described, bit 5 empty and writable, bit 6 empty without space), then for a described save `+4` the place id,
//! `+8` the word at body `+0x1604`, `+0xc` the munny word at body `+0x1600`, `+0x10` the game mode byte (`+0x1658`), `+0x11` Sora's
//! level, `+0x12` the world (0xff above 0x12), `+0x13` the icon variant byte (`+0x31a8`), `+0x14` its bit 8, and at `+0x18` the
//! directory entry's 8-byte modify time.

use crate::body::space_needed;
use kh2_game_core::save_block::{at, member_offset};

/// Length of the card info block.
pub const INFO_LEN: usize = 0xc68;
/// Number of save slots.
pub const SLOTS: usize = 99;
/// Offset of the first record.
pub const RECORDS: usize = 8;
/// Length of a record.
pub const RECORD_LEN: usize = 0x20;

/// Record flag bits.
pub mod flag {
    pub const EXISTS: u32 = 1;
    pub const MARK: u32 = 2;
    pub const VALID: u32 = 0x10;
    pub const EMPTY: u32 = 0x20;
    pub const NO_SPACE: u32 = 0x40;
}

/// The card info block as bytes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CardInfo(pub Vec<u8>);

impl Default for CardInfo {
    /// What `FUN_0023c948` leaves: zeroed, byte `+0` 0xff, halfword `+2` 0xffff.
    fn default() -> Self {
        let mut v = vec![0u8; INFO_LEN];
        v[0] = 0xff;
        v[2] = 0xff;
        v[3] = 0xff;
        CardInfo(v)
    }
}

/// The place id of an area and world (`FUN_0019e178`, a table lookup the port does not own): the caller supplies it.
pub type PlaceFn<'a> = &'a dyn Fn(u8, u8) -> u32;

impl CardInfo {
    pub fn from_bytes(b: &[u8]) -> CardInfo {
        let mut v = vec![0; INFO_LEN];
        let n = b.len().min(INFO_LEN);
        v[..n].copy_from_slice(&b[..n]);
        CardInfo(v)
    }

    fn rec(slot: usize) -> usize {
        RECORDS + slot * RECORD_LEN
    }

    fn word(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.0[o..o + 4].try_into().unwrap())
    }

    fn set_word(&mut self, o: usize, v: u32) {
        self.0[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// The flag word of a record.
    pub fn flags(&self, slot: usize) -> u32 {
        self.word(Self::rec(slot))
    }

    /// The record's bytes (0x20).
    pub fn record(&self, slot: usize) -> &[u8] {
        &self.0[Self::rec(slot)..Self::rec(slot) + RECORD_LEN]
    }

    /// `slot` as the functions that accept a negative number read it: the cursor byte for a negative one.
    fn resolve(&self, slot: i64) -> usize {
        if slot < 0 { self.0[1] as i8 as i64 as usize } else { slot as usize }
    }

    /// orig 0x23d2f8
    pub fn set_port(&mut self, v: u8) {
        self.0[0] = v;
    }

    /// orig 0x23d308
    pub fn port(&self) -> u8 {
        self.0[0]
    }

    /// orig 0x23d318
    pub fn set_free_kb(&mut self, v: u16) {
        self.0[2..4].copy_from_slice(&v.to_le_bytes());
    }

    /// The free space halfword (signed: negative means unknown).
    pub fn free_kb(&self) -> i16 {
        i16::from_le_bytes([self.0[2], self.0[3]])
    }

    /// orig 0x23d328
    pub fn set_cursor(&mut self, v: u8) {
        self.0[1] = v;
    }

    /// orig 0x23d338
    pub fn cursor(&self) -> u8 {
        self.0[1]
    }

    /// orig 0x23dfe8
    pub fn set_flag4(&mut self, v: bool) {
        self.0[4] = u8::from(v);
    }

    /// orig 0x23e000
    pub fn flag4(&self) -> bool {
        self.0[4] != 0
    }

    /// Clears all 99 records.
    ///
    /// orig 0x23d348
    pub fn clear_records(&mut self) {
        self.0[RECORDS..RECORDS + 0xc60].fill(0);
    }

    /// Clears one record's flag word.
    ///
    /// orig 0x23d520
    pub fn clear(&mut self, slot: usize) {
        self.set_word(Self::rec(slot), 0);
    }

    /// ORs bits into a record's flag word (a negative slot is the cursor).
    ///
    /// orig 0x23d698
    pub fn or_bits(&mut self, slot: i64, bits: u32) {
        let s = self.resolve(slot);
        let o = Self::rec(s);
        let w = self.word(o) | bits;
        self.set_word(o, w);
    }

    /// Describes a body in a record. `bits`: 1 and 2 set those flags; bit 4 describes a valid save (the other fields from `body`), else
    /// bit 6 marks an empty slot without space and anything else an empty writable one.
    ///
    /// orig 0x23d370
    pub fn describe(&mut self, slot: usize, body: &[u8], bits: u32, place: PlaceFn) {
        let r = Self::rec(slot);
        if bits & 1 != 0 {
            let w = self.word(r) | 1;
            self.set_word(r, w);
        }
        if bits & 2 != 0 {
            let w = self.word(r) | 2;
            self.set_word(r, w);
        }
        if bits & 0x10 == 0 {
            let w = self.word(r) | if bits & 0x40 == 0 { 0x20 } else { 0x40 };
            self.set_word(r, w);
            return;
        }
        let w = self.word(r) | 0x10;
        self.set_word(r, w);
        let bw = |o: usize| u32::from_le_bytes(body[o..o + 4].try_into().unwrap());
        self.set_word(r + 4, place(body[0xd], body[0xc]));
        self.set_word(r + 8, bw(0x1604));
        self.set_word(r + 0xc, bw(0x1600));
        self.0[r + 0x10] = body[0x1658];
        let who = if bw(0x165c) & 1 == 0 { 1 } else { 0xe };
        let off = member_offset(who).unwrap_or(0);
        self.0[r + 0x11] = body[at::PARTY + off + 0xf];
        self.0[r + 0x12] = if body[0xc] > 0x12 { 0xff } else { body[0xc] };
        self.0[r + 0x13] = body[0x31a8];
        self.0[r + 0x14] = body[0x31a9] & 1;
    }

    /// Describes a body after a save (`bits` 0x13), for the slot or the cursor.
    ///
    /// orig 0x23dd80
    pub fn describe_saved(&mut self, slot: i8, body: &[u8], place: PlaceFn) {
        let s = if slot < 0 { self.0[1] as i8 } else { slot };
        self.describe(s as usize, body, 0x13, place);
    }

    /// Sets a record's modify time: from the first directory entry's time (`table` is the directory table, the time at entry +8), or
    /// all 0xff when `clear` is set.
    ///
    /// orig 0x23de68
    pub fn set_time(&mut self, slot: usize, clear: bool, table: &[u8]) {
        let o = Self::rec(slot) + 0x18;
        if clear {
            self.0[o..o + 8].fill(0xff);
        } else {
            self.0[o..o + 8].copy_from_slice(&table[8..16]);
        }
    }

    /// The slot with a valid save whose modify time is the latest (year, month, day, hour, minute, second; ties keep the earlier slot), or
    /// 0 when no slot has one.
    ///
    /// orig 0x23ded0
    pub fn latest(&self) -> usize {
        let mut best: Option<(usize, [u8; 8])> = None;
        for s in 0..SLOTS {
            if self.flags(s) & flag::VALID == 0 {
                continue;
            }
            let r = Self::rec(s) + 0x18;
            let t: [u8; 8] = self.0[r..r + 8].try_into().unwrap();
            let key = |t: &[u8; 8]| (u16::from_le_bytes([t[6], t[7]]), t[5], t[4], t[3], t[2], t[1]);
            if best.is_none_or(|(_, b)| key(&t) > key(&b)) {
                best = Some((s, t));
            }
        }
        best.map_or(0, |(s, _)| s)
    }

    /// The first slot from the cursor whose record has bit 0 set and bit 1 clear... as read: `((flags ^ 1) & 1) == 0 && flags & 2 == 0`,
    /// that is bit 0 set and bit 1 clear. `-1` when none below 99.
    ///
    /// orig 0x23da78
    pub fn next_free_dir(&self) -> i32 {
        let mut s = self.0[1] as i8 as i32;
        // A negative cursor would read before the block in the original; no caller sets one [inferred], the port reports none.
        if s < 0 {
            return -1;
        }
        while s < SLOTS as i32 {
            let f = self.flags(s as usize);
            if f & 1 != 0 && f & 2 == 0 {
                return s;
            }
            s += 1;
        }
        -1
    }

    /// When the free space is known and below the space a save needs, every empty record (flag word 0) becomes 0x40.
    ///
    /// orig 0x23dde0
    pub fn mark_no_space(&mut self) {
        let f = self.free_kb();
        if f >= 0 && i32::from(f) < space_needed() {
            for s in 0..SLOTS {
                if self.flags(s) == 0 {
                    self.set_word(Self::rec(s), 0x40);
                }
            }
        }
    }

    /// Whether the slot (negative: the cursor) holds a valid save.
    ///
    /// orig 0x23ddb0
    pub fn is_valid(&self, slot: i64) -> bool {
        let s = self.resolve(slot);
        self.flags(s) >> 4 & 1 != 0
    }

    /// Whether the slot is empty and writable.
    ///
    /// orig 0x23e060
    pub fn is_empty_writable(&self, slot: usize) -> bool {
        self.flags(slot) >> 5 & 1 != 0
    }

    /// Whether any slot holds a valid save.
    ///
    /// orig 0x23e018
    pub fn any_valid(&self) -> bool {
        (0..SLOTS).any(|s| self.flags(s) & flag::VALID != 0)
    }
}
