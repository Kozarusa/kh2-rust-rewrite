//! The original's save block as one layout object (RULES C7; core runtime, WP-L1-P8).
//!
//! The save block (`0x33e860`, 0xb4e0 bytes) holds the whole saved game: the area program table, the story flags, the world-point
//! bits, the exit locks, the record bits, the munny word, the game mode, the party block (`0x33fec0`, 0x1998 bytes: members, drive
//! forms, inventory counts, experience), the world map's records and flags, the option word, the shop and moogle records, the journal
//! and the object block at `+0xacc0`. [`SaveBlock`] is those bytes with typed accessors at the original's offsets, so a routine of
//! the original that works on a part of it is ported as a function over a byte slice in the original's layout and an oracle test
//! compares the whole block with the game's own code. The party block is the part [`crate::progress::Progress`] reads and writes
//! most; it is the slice [`SaveBlock::party`] (offsets in `party::` are relative to its start, the offsets of [`at`] to the save block).
//!
//! The boot routine `FUN_00105638` (the new game's save block) and every routine below it that only writes defaults are ported here
//! as functions over `&mut [u8]`, each starting at the struct the original passes. Tables the original reads from the executable
//! (the object block's fix-up list at `0x37c7d8`, the region word at `0x349d44`) come in through [`BootEnv`] and are never kept in
//! the repo. Nothing in the app calls the whole boot yet; `Progress::new_game` runs the party half of it.
//!
//! Not here: the story flags' side effects (`progress.rs` `Story`, `FUN_001b6870` at the end of `FUN_00187d20`), the table-driven
//! `FUN_001c0cb0(0)` that gives the new game its starting gear (`Progress::apply_plrp`, passed to [`SaveBlock::init_save`] as `mid`),
//! and the memory-card layer (`kh2-save`, WP-L1-P9).

/// Size of the save block.
pub const LEN: usize = 0xb4e0;

/// Offsets in the save block (add `0x33e860` for the address in the original).
pub mod at {
    /// The four header bytes `"KH2"` + the region letter (`FUN_00105758`), `+4` the word 0x2d.
    pub const MAGIC: usize = 0x0;
    pub const HEADER_WORD: usize = 0x4;
    /// The story tables `FUN_0032c7c8` clears (from `+0xc`).
    pub const STORY_TABLES: usize = 0xc;
    /// The area program table `(map, btl, evt)` per world and area (19 worlds of 0xc0 bytes).
    pub const PROGRAMS: usize = 0xf;
    /// The story flag pages (19 of 0x20 bytes).
    pub const FLAGS: usize = 0xe50;
    /// The word at `+0x10b0` is the first of the world-point words `FUN_0032c7c8` clears.
    pub const WORLD_POINTS: usize = 0x10b0;
    pub const LOCKS: usize = 0x10b8;
    pub const RECORD_BITS: usize = 0x1550;
    /// The munny word, and the 21 words from `+0x1604` that `FUN_0014c8c8` clears.
    pub const WALLET: usize = 0x1600;
    /// The game mode word (`FUN_00105748` writes it; 0 Beginner, 1 Standard, 2 Proud).
    pub const GAME_MODE: usize = 0x1658;
    /// The party block (0x1998 bytes), set up by `FUN_00187b38`.
    pub const PARTY: usize = 0x1660;
    /// The world map's records, flags and queue (`FUN_0032c550` clears from here).
    pub const WORLD_RECORDS: usize = 0x2ff8;
    /// The option word (`FUN_0023e9a0` gives it its default).
    pub const OPTIONS: usize = 0x31b0;
    /// The words of the NEW tags (`FUN_0032c610`).
    pub const NEW_SEEN: usize = 0x31b4;
    /// The shop records: two words from `+0x370c`, the four "seen" words, the first-visit byte.
    pub const SHOP: usize = 0x370c;
    pub const SHOP_SEEN: usize = 0x3714;
    pub const SHOP_VISITS: usize = 0x3724;
    /// The moogle's synthesis record (`FUN_0032c690`).
    pub const MOOGLE: usize = 0x3728;
    /// The journal's records (`FUN_0032c6c8`).
    pub const JOURNAL: usize = 0x38f0;
    /// The block `FUN_002ee2a8` sets up (a settings record and two large tables).
    pub const CONFIG_BLOCK: usize = 0x3a20;
    /// The object block `FUN_001ddec0` sets up.
    pub const OBJECT_BLOCK: usize = 0xacc0;
}

/// Offsets in the party block (add `0x33fec0`, or [`at::PARTY`] for the save block).
pub mod party {
    /// Party member records: `FUN_00187d40(0x33fec0, member)` = `0x33fec0 + (index - 1) * 0xf4`.
    pub const MEMBER_SIZE: usize = 0xf4;
    pub const MEMBERS: usize = 13;
    /// Form records: `FUN_00187f60(0x33fec0, f)` = `0x33fec0 + 0xc3c + f * 0x28`, `f` 1..=9.
    pub const FORM_BASE: usize = 0xc3c;
    pub const FORM_SIZE: usize = 0x28;
    pub const FORMS: usize = 9;
    /// `0x340c88` + 4: the current form (`FUN_001b4c28`).
    pub const CURRENT_FORM: usize = 0xdcc;
    /// `0x340c88` + 5: the active summon (1-based `sumn` index), + 6 the summon level, + 7 the
    /// Genie form / Tinker Bell byte.
    pub const SUMMON: usize = 0xdcd;
    pub const SUMMON_LEVEL: usize = 0xdce;
    pub const SUMMON_EXTRA: usize = 0xdcf;
    /// Saved drive gauge bytes `+0xdd0..+0xdd3` and floats `+0xdd4`, `+0xdd8` (`FUN_0019f920`
    /// copies them to status `+0x1b0..+0x1b8`).
    pub const DRIVE: usize = 0xdd0;
    /// World records `0x340c9c + 4 * world` (`FUN_001a71c0`), 19 of them.
    pub const WORLDS: usize = 0xddc;
    pub const WORLD_COUNT: usize = 19;
    /// Item counts `0x340ce8`, one byte per item `Slot` (`FUN_001a1cc8`).
    pub const COUNTS: usize = 0xe28;
    pub const COUNT_SLOTS: usize = 0x140;
    /// Key-item bits `0x340e28` (items with `Flag0` bit 0), by `Slot`.
    pub const KEYS: usize = 0xf68;
    pub const KEY_WORDS: usize = 8;
    /// Experience total `0x340e48` (`FUN_001b51e0`, followed by the level-up check `FUN_001b52f8`).
    pub const EXPERIENCE: usize = 0xf88;
    /// Summon experience `0x340e4c` (`FUN_001d4ec8`).
    pub const SUMMON_EXP: usize = 0xf8c;
    /// Consumables Sora received, 8 ids (`FUN_001a2120`).
    pub const RECENT: usize = 0xf90;
    /// Four command shortcuts (`0x340e60`; `FUN_001a2120` and `FUN_001a1a18` fill empty ones).
    pub const SHORTCUTS: usize = 0xfa0;
    /// The two eight-word arrays `FUN_00187b38` clears at `+0xfac` and `+0xfcc` (the bonus flags and a second array).
    pub const BONUS: usize = 0xfac;
    pub const SECOND_FLAGS: usize = 0xfcc;
    /// Per-spell switch bits `0x340eac` (`FUN_001a4b30`, `FUN_001a4b68`, `FUN_001a4bb8`).
    pub const SPELL_SWITCH: usize = 0xfec;
    /// "Obtained once" bits `0x3414f0`, by item `Slot` (`FUN_001a18d8`).
    pub const OBTAINED: usize = 0x1630;
    pub const OBTAINED_WORDS: usize = 10;
    /// A word `FUN_00187b38` clears (party `+0x1658`); the 32 records of 8 bytes from `+0x165c` keep only bit 15 of their halfword
    /// at `+2`, and the 64 records of 8 bytes from `+0x1760` are cleared (byte 0 and word 4).
    pub const WORD_1658: usize = 0x1658;
    pub const KEEP_BIT_RECORDS: usize = 0x165c;
    pub const CLEARED_RECORDS: usize = 0x1760;
    /// The four words `FUN_00187b38` clears at `+0x1960`, the report words from `+0x197a` (`FUN_001d7640`) and the halfword at `+0x1994`.
    pub const WORDS_1960: usize = 0x1960;
    pub const REPORT: usize = 0x197a;
    pub const HALF_1994: usize = 0x1994;
    /// Length of the party block as `FUN_00187b38` touches it.
    pub const LEN: usize = 0x1998;
}

/// What the boot routine reads outside the save block.
#[derive(Clone, Debug, Default)]
pub struct BootEnv {
    /// The region word of the executable (`0x349d44`, answered by `FUN_00105cd8`): 1 gives the header letter `J`, 2 gives `U`, anything
    /// else `E` (the executable image holds 0; a running USA game has 2).
    pub region: u32,
    /// The object block's fix-up list (`0x37c7d8`, 20 words = 10 pairs of (offset, delta) read by `FUN_001ddf20` until an offset of 0).
    pub fixups: Vec<u32>,
}

/// The region word of a running USA game (`0x349d44`, set at start-up: the executable image holds 0 there). Evidence: the save header of a
/// state taken in the USA game reads `"KH2U"`, which `FUN_00105758` makes only from the value 2.
pub const REGION_USA: u32 = 2;

/// The save block: [`LEN`] bytes in the original's layout.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SaveBlock(Vec<u8>);

impl Default for SaveBlock {
    fn default() -> Self {
        SaveBlock(vec![0; LEN])
    }
}

fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// Zeroes `n` words from byte offset `o`.
fn zero_words(b: &mut [u8], o: usize, n: usize) {
    b[o..o + 4 * n].fill(0);
}

impl SaveBlock {
    /// An all-zero block (what `FUN_00310008(save, 0xb4e0)` leaves).
    pub fn new() -> SaveBlock {
        SaveBlock::default()
    }

    /// A block from bytes: shorter input is zero-padded, longer input cut.
    pub fn from_bytes(bytes: &[u8]) -> SaveBlock {
        let mut v = vec![0; LEN];
        let n = bytes.len().min(LEN);
        v[..n].copy_from_slice(&bytes[..n]);
        SaveBlock(v)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }

    pub fn u8_at(&self, o: usize) -> u8 {
        self.0[o]
    }
    pub fn set_u8(&mut self, o: usize, v: u8) {
        self.0[o] = v;
    }
    pub fn u16_at(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.0[o], self.0[o + 1]])
    }
    pub fn set_u16(&mut self, o: usize, v: u16) {
        self.0[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    pub fn u32_at(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.0[o..o + 4].try_into().unwrap())
    }
    pub fn set_u32(&mut self, o: usize, v: u32) {
        put32(&mut self.0, o, v);
    }

    /// The party block (`party::LEN` bytes from [`at::PARTY`]).
    pub fn party(&self) -> &[u8] {
        &self.0[at::PARTY..at::PARTY + party::LEN]
    }

    pub fn party_mut(&mut self) -> &mut [u8] {
        &mut self.0[at::PARTY..at::PARTY + party::LEN]
    }

    /// The header word `"KH2"` + region letter (`+0`).
    pub fn magic(&self) -> u32 {
        self.u32_at(at::MAGIC)
    }
    /// The munny word (`+0x1600`).
    pub fn wallet(&self) -> u32 {
        self.u32_at(at::WALLET)
    }
    pub fn set_wallet(&mut self, v: u32) {
        self.set_u32(at::WALLET, v);
    }
    /// The option word (`+0x31b0`).
    pub fn options(&self) -> u32 {
        self.u32_at(at::OPTIONS)
    }
    pub fn set_options(&mut self, v: u32) {
        self.set_u32(at::OPTIONS, v);
    }
    /// The game mode word (`+0x1658`).
    pub fn game_mode(&self) -> u32 {
        self.u32_at(at::GAME_MODE)
    }
    pub fn set_game_mode(&mut self, v: u32) {
        set_game_mode_word(&mut self.0, v);
    }
    /// The four "seen" words of the shops (`+0x3714`).
    pub fn shop_seen(&self) -> [u32; 4] {
        std::array::from_fn(|i| self.u32_at(at::SHOP_SEEN + 4 * i))
    }
    pub fn set_shop_seen(&mut self, w: [u32; 4]) {
        for (i, v) in w.into_iter().enumerate() {
            self.set_u32(at::SHOP_SEEN + 4 * i, v);
        }
    }
    /// The shops' first-visit byte (`+0x3724`).
    pub fn shop_visits(&self) -> u8 {
        self.u8_at(at::SHOP_VISITS)
    }
    pub fn set_shop_visits(&mut self, v: u8) {
        self.set_u8(at::SHOP_VISITS, v);
    }

    /// `FUN_00105638(0x33e860)`: the save block of a new game, in the order of the original. `mid` stands for `FUN_00187d20` (the new
    /// game's starting gear from the `plrp` rows with `Id` 0, then the world flags' boot `FUN_001b6870`); it runs after the object
    /// block and the header are set and before the world records' last byte, exactly where the original calls it. The calls with no
    /// effect on the block (`FUN_001ddff0`, the script-VM start `FUN_00194d78`) are left out.
    ///
    /// orig 0x105638
    pub fn init_save(&mut self, env: &BootEnv, mid: impl FnOnce(&mut SaveBlock)) {
        self.head(env);
        mid(self);
        self.tail();
    }

    /// The part of `FUN_00105638` before the call of `FUN_00187d20`.
    pub fn head(&mut self, env: &BootEnv) {
        clear_all(&mut self.0);
        static_init(&mut self.0, &env.fixups, 1, STATIC_INIT_TOKEN);
        let magic = header_word(env.region);
        self.set_u32(at::MAGIC, magic);
        self.set_u32(at::HEADER_WORD, 0x2d);
    }

    /// The part of `FUN_00105638` after the call of `FUN_00187d20`.
    pub fn tail(&mut self) {
        let b = &mut self.0;
        mark_world_records(&mut b[at::WORLD_RECORDS..]);
        clear_wallet_table(b);
        set_game_mode_word(b, 1);
        reset_settings(b);
    }
}

/// `FUN_00105748(save, v)`: the game mode word at save `+0x1658`.
///
/// orig 0x105748
pub fn set_game_mode_word(save: &mut [u8], v: u32) {
    put32(save, at::GAME_MODE, v);
}

/// `FUN_00310008(block, len)`: zero the bytes.
///
/// orig 0x310008
pub fn clear_all(b: &mut [u8]) {
    b.fill(0);
}

/// `FUN_00105758`: the header word, `"KH2"` and a letter chosen by the region word (`J` for 1, `U` for 2, else `E`).
///
/// orig 0x105758
pub fn header_word(region: u32) -> u32 {
    let letter = match region {
        1 => 0x4a,
        2 => 0x55,
        _ => 0x45,
    };
    letter << 24 | 0x32484b
}

/// `FUN_0032c7c8(save + 0xc)`: clears the area program table (19 rows of 0xc0 bytes from `+3`), the story flag pages (19 rows of 8 words
/// from `+0xe44`), two words at `+0x10a4` (the world-point bits), 19 pairs of words from `+0x14ac`, 7 words from `+0x1544`, 16 words from
/// `+0x1560` and 16 from `+0x15b4`, and the word at `+0x1650`.
///
/// orig 0x32c7c8
pub fn clear_story_tables(b: &mut [u8]) {
    for row in 0..19 {
        b[3 + 0xc0 * row..3 + 0xc0 * (row + 1)].fill(0);
    }
    zero_words(b, 0xe44, 19 * 8);
    put32(b, 0x10a4, 0);
    put32(b, 0x10a8, 0);
    zero_words(b, 0x14ac, 19 * 2);
    zero_words(b, 0x1544, 7);
    zero_words(b, 0x1560, 16);
    zero_words(b, 0x15b4, 16);
    put32(b, 0x1650, 0);
}

/// `FUN_0032c8f8(p)`: four zero words.
///
/// orig 0x32c8f8
pub fn clear_four_words(b: &mut [u8]) {
    zero_words(b, 0, 4);
}

/// `FUN_0032c550(save + 0x2ff8)`: clears the world records (19 words), eight words from `+0x4c` (the world flags), the queue count byte
/// at `+0x16b`, 32 bytes from `+0x16c`, four words from `+0x18c` and four from `+0x19c` (`FUN_0032c8f8`).
///
/// orig 0x32c550
pub fn clear_world_records(b: &mut [u8]) {
    zero_words(b, 0, 19);
    zero_words(b, 0x4c, 8);
    b[0x16b] = 0;
    b[0x16c..0x18c].fill(0);
    zero_words(b, 0x18c, 4);
    clear_four_words(&mut b[0x19c..]);
}

/// `FUN_002c5828(save + 0x2ff8)`: byte `+0x1ac` is set to 1.
///
/// orig 0x2c5828
pub fn mark_world_records(b: &mut [u8]) {
    b[0x1ac] = 1;
}

/// `FUN_0032c610(save + 0x31b4)`: clears the 38 words of the NEW tags' record.
///
/// orig 0x32c610
pub fn clear_new_seen(b: &mut [u8]) {
    zero_words(b, 0, 0x26);
}

/// `FUN_0032c690(save + 0x3728)`: clears the moogle's record (12 words) and the byte at `+0x1c5`.
///
/// orig 0x32c690
pub fn clear_moogle(b: &mut [u8]) {
    b[0x1c5] = 0;
    zero_words(b, 0, 12);
}

/// `FUN_0032c6c8(save + 0x38f0)`: clears the journal's record (74 words).
///
/// orig 0x32c6c8
pub fn clear_journal(b: &mut [u8]) {
    zero_words(b, 0, 0x4a);
}

/// `FUN_002ee1c8(p)`: byte 0 is set to 1.
///
/// orig 0x2ee1c8
pub fn set_first_byte(b: &mut [u8]) {
    b[0] = 1;
}

/// `FUN_002ee208(p)`: a settings record of 0x13a bytes: bytes 0, 0x8b and 0x8c are 1, bytes 1, 2 and 0x8a are 0, 16 bytes each from
/// `+0x9a` and `+0xaa` and 128 bytes from `+0xba` are 0.
///
/// orig 0x2ee208
pub fn init_settings_record(b: &mut [u8]) {
    b[1] = 0;
    b[0x8c] = 1;
    b[0] = 1;
    b[2] = 0;
    b[0x8a] = 0;
    b[0x8b] = 1;
    b[0x9a..0xaa].fill(0);
    b[0xaa..0xba].fill(0);
    b[0xba..0x13a].fill(0);
}

/// `FUN_002ee2e8(p)`: byte 0 is 1, the settings record at `+4` is set up, 0x54c0 bytes from `+0x160` and 0x1c80 from `+0x5620` are cleared.
///
/// orig 0x2ee2e8
pub fn init_config_tables(b: &mut [u8]) {
    set_first_byte(b);
    init_settings_record(&mut b[4..]);
    b[0x160..0x160 + 0x54c0].fill(0);
    b[0x5620..0x5620 + 0x1c80].fill(0);
}

/// `FUN_002ee2a8(save + 0x3a20)`: byte 0 is 1 (`FUN_002ee1b0`), the settings record at `+4` is set up (`FUN_002ee1f0`), then
/// `FUN_002ee2e8`.
///
/// orig 0x2ee2a8
pub fn init_config_block(b: &mut [u8]) {
    set_first_byte(b);
    init_settings_record(&mut b[4..]);
    init_config_tables(b);
}

/// `FUN_001ddf20(obj)`: applies the fix-up list: for each (offset, delta) pair until an offset of 0, the byte at `obj + 0x6c0 + offset`
/// gets `delta` added. The list is the 20 words the original copies from the executable at `0x37c7d8`; an offset of 0 in the first
/// pair means no fix-up at all.
///
/// orig 0x1ddf20
pub fn apply_object_fixups(b: &mut [u8], fixups: &[u32]) {
    let word = |i: usize| fixups.get(i).copied().unwrap_or(0);
    for i in 0..10 {
        let off = word(2 * i) as usize;
        if off == 0 {
            break;
        }
        if let Some(x) = b.get_mut(0x6c0 + off) {
            *x = x.wrapping_add(word(2 * i + 1) as u8);
        }
    }
}

/// `FUN_001ddfb0(obj)`: 27 records of 0x40 bytes from `+0x18`: byte 0 the row 0..8, byte 1 the column 1..3.
///
/// orig 0x1ddfb0
pub fn number_object_cells(b: &mut [u8]) {
    for row in 0..9 {
        for col in 0..3 {
            let o = 0x18 + 0x40 * (row * 3 + col);
            b[o] = row as u8;
            b[o + 1] = col as u8 + 1;
        }
    }
}

/// `FUN_001ddec0(save + 0xacc0)`: 20 words from `+0x7c0` and the words at `+0x810`, `+0x814`, `+0x818`, `+0x81c` are cleared, then the
/// fix-up list is applied (`FUN_001ddf20`) and the cells numbered (`FUN_001ddfb0`).
///
/// orig 0x1ddec0
pub fn init_object_block(b: &mut [u8], fixups: &[u32]) {
    zero_words(b, 0x7c0, 20);
    put32(b, 0x810, 0);
    put32(b, 0x814, 0);
    put32(b, 0x818, 0);
    put32(b, 0x81c, 0);
    apply_object_fixups(b, fixups);
    number_object_cells(b);
}

/// `FUN_0014c8c8()`: the 21 words from save `+0x1604` (up to `+0x1658`, the game mode word) (the munny word before them is not touched).
///
/// orig 0x14c8c8
pub fn clear_wallet_table(save: &mut [u8]) {
    zero_words(save, at::WALLET + 4, 21);
}

/// `FUN_0023e9a0()`: the halfwords at save `+0x31ac` and `+0x31ae` are cleared and the option word keeps its bits outside 0x7f6, then
/// gets 0x40b (the default Config rows).
///
/// orig 0x23e9a0
pub fn reset_options(save: &mut [u8]) {
    save[0x31ac..0x31b0].fill(0);
    let w = u32::from_le_bytes(save[at::OPTIONS..at::OPTIONS + 4].try_into().unwrap());
    put32(save, at::OPTIONS, w & 0xffff_f809 | 0x40b);
}

/// `FUN_00239f28()`: 0x870 bytes from save `+0x31a8` are cleared, then `FUN_0023e9a0`.
///
/// orig 0x239f28
pub fn reset_settings(save: &mut [u8]) {
    save[0x31a8..0x31a8 + 0x870].fill(0);
    reset_options(save);
}

// --- the party block ---------------------------------------------------------------------------------

/// `FUN_00114318(p)`: 32 bytes are cleared (a member's AI settings, record `+0xd4`).
///
/// orig 0x114318
pub fn clear_member_settings(b: &mut [u8]) {
    b[..32].fill(0);
}

/// `FUN_001875c8(rec)`: a party member record's defaults: HP, MP and their maxima 100, level 1, one armor and one accessory slot,
/// no item slots, no strength, magic, defense or AP, the 64 ability halfwords from `+0x54` and the AI settings cleared.
///
/// orig 0x1875c8
pub fn init_member(r: &mut [u8]) {
    r[9] = 0;
    r[7] = 100;
    r[0x11] = 1;
    r[4] = 100;
    r[5] = 100;
    r[6] = 100;
    for o in [0xa, 0xb, 0xc, 0xd, 0xe] {
        r[o] = 0;
    }
    r[0xf] = 1;
    r[0x10] = 1;
    r[0x12] = 0;
    r[0x54..0x54 + 128].fill(0);
    clear_member_settings(&mut r[0xd4..]);
}

/// `FUN_00187ed0(rec)`: a drive form record's defaults: halfword 0 is 0, byte 3 and byte 2 are 1, the word at `+4` is 0 and the 16
/// ability halfwords from `+8` are cleared.
///
/// orig 0x187ed0
pub fn init_form(r: &mut [u8]) {
    r[0] = 0;
    r[1] = 0;
    r[3] = 1;
    r[2] = 1;
    put32(r, 4, 0);
    r[8..40].fill(0);
}

/// `FUN_001a71a8(rec)`: a world record's defaults, the four bytes 0, 0x10, 0x10, 0x10.
///
/// orig 0x1a71a8
pub fn init_world_slot(r: &mut [u8]) {
    r[..4].copy_from_slice(&[0, 0x10, 0x10, 0x10]);
}

/// `FUN_001d7640(p)`: 13 halfwords are cleared (the report counters).
///
/// orig 0x1d7640
pub fn clear_report_words(b: &mut [u8]) {
    b[..26].fill(0);
}

/// `FUN_00187b38(0x33fec0)`: the new game's party block before the starting gear: 13 member records and 9 form records at their
/// defaults, no form, summon level 1, drive gauge 100 with bytes 3 and 3, the 19 world records at their defaults, the key bits, the
/// experience, the bonus flags and the second flag array, the spell switches, the "obtained once" bits and the other counters cleared,
/// the 32 records from `+0x165c` keep only bit 15 of their halfword at `+2`, the 64 records from `+0x1760` cleared, four words from
/// `+0x1960`, the report words from `+0x197a`, the halfword at `+0x1994` and the four shortcuts.
///
/// orig 0x187b38
pub fn init_party(b: &mut [u8]) {
    for m in 0..party::MEMBERS {
        init_member(&mut b[m * party::MEMBER_SIZE..]);
    }
    for f in 1..=party::FORMS {
        init_form(&mut b[party::FORM_BASE + f * party::FORM_SIZE..]);
    }
    b[party::CURRENT_FORM] = 0;
    b[party::DRIVE + 2] = 3;
    b[party::SUMMON_LEVEL] = 1;
    b[party::DRIVE] = 100;
    b[party::SUMMON] = 0;
    b[party::DRIVE + 1] = 3;
    for w in 0..party::WORLD_COUNT {
        init_world_slot(&mut b[party::WORLDS + 4 * w..]);
    }
    zero_words(b, party::KEYS, party::KEY_WORDS);
    put32(b, party::EXPERIENCE, 0);
    zero_words(b, party::BONUS, 8);
    zero_words(b, party::SECOND_FLAGS, 8);
    put32(b, party::SPELL_SWITCH, 0);
    zero_words(b, party::OBTAINED, party::OBTAINED_WORDS);
    put32(b, party::WORD_1658, 0);
    for k in 0..32 {
        let o = party::KEEP_BIT_RECORDS + 8 * k + 2;
        let v = u16::from_le_bytes([b[o], b[o + 1]]) & 0x8000;
        b[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    for k in 0..64 {
        let o = party::CLEARED_RECORDS + 8 * k;
        b[o] = 0;
        put32(b, o + 4, 0);
    }
    clear_four_words(&mut b[party::WORDS_1960..]);
    clear_report_words(&mut b[party::REPORT..]);
    b[party::HALF_1994..party::HALF_1994 + 2].fill(0);
    b[party::SHORTCUTS..party::SHORTCUTS + 8].fill(0);
}

/// `FUN_00187930(party)`: the same defaults as [`init_party`], written out with absolute offsets (forms from `+0xc64`, the four words at
/// `+0x1960` inline, the bonus flags cleared with the arrays at `+0xfac` and `+0xfcc`). The game has no caller of it that this port found;
/// the oracle test shows both leave the same bytes.
///
/// orig 0x187930
pub fn init_party_unrolled(b: &mut [u8]) {
    init_party(b);
}

// --- records of the party block ----------------------------------------------------------------------

/// `FUN_001b58d8(member)`: the 1-based record index of a party member id: ids 1..=15, but 14 is Sora's record (1) and 15 the sixth;
/// anything else is 0 (no record).
///
/// orig 0x1b58d8
pub fn member_index(member: i32) -> usize {
    if (member as u32).wrapping_sub(1) < 0xf {
        match member {
            0xe => 1,
            0xf => 6,
            m => m as usize,
        }
    } else {
        0
    }
}

/// `FUN_00187d40(party, member)`: the party-relative offset of a member's record, `None` for no record.
///
/// orig 0x187d40
pub fn member_offset(member: i32) -> Option<usize> {
    let i = member_index(member);
    (i != 0).then(|| (i - 1) * party::MEMBER_SIZE)
}

/// `FUN_00187f60(party, form)`: the party-relative offset of a drive form record, forms 1..=9, `None` otherwise.
///
/// orig 0x187f60
pub fn form_offset(form: i32) -> Option<usize> {
    ((form as u32).wrapping_sub(1) < 9).then(|| party::FORM_BASE + form as usize * party::FORM_SIZE)
}

/// `FUN_00282d30(form, weapon)`: the weapon halfword (`+0`) of a drive form record (forms 1..=9; nothing for another number).
///
/// orig 0x282d30
pub fn set_form_weapon(party: &mut [u8], form: i32, weapon: u16) {
    if let Some(r) = form_offset(form) {
        party[r..r + 2].copy_from_slice(&weapon.to_le_bytes());
    }
}

/// `FUN_001be7e8(n)`: bit `n & 31` of the word `(n >> 5) - 0x11` of the array at party `+0xff0` (the original indexes memory without a
/// bound; the port answers false for a word outside the party block).
///
/// orig 0x1be7e8
pub fn defeat_bit(party: &[u8], n: u32) -> bool {
    let word = (n >> 5) as i64 - 0x11;
    let o = 0xff0 + 4 * word;
    o >= 0 && (o as usize) + 4 <= party.len() && u32::from_le_bytes(party[o as usize..o as usize + 4].try_into().unwrap()) & (1 << (n & 31)) != 0
}

/// `FUN_0019d890()`: the 38 words (19 pairs) from save `+0x14b8` are cleared.
///
/// orig 0x19d890
pub fn clear_pair_table(save: &mut [u8]) {
    zero_words(save, 0x14b8, 38);
}

/// `FUN_00187648(rec, a, b)`: the two weapon halfwords of a member record.
///
/// orig 0x187648
pub fn set_member_weapons(rec: &mut [u8], a: u16, b: u16) {
    rec[0..2].copy_from_slice(&a.to_le_bytes());
    rec[2..4].copy_from_slice(&b.to_le_bytes());
}

/// `FUN_00187d88(party, member, a, b)`: [`set_member_weapons`] on the record of a member id. For an id without a record the original
/// passes the null record on and writes at address 0; the port does nothing.
///
/// orig 0x187d88
pub fn set_weapons_of_member(party: &mut [u8], member: i32, a: u16, b: u16) {
    if let Some(r) = member_offset(member) {
        set_member_weapons(&mut party[r..], a, b);
    }
}

/// `FUN_00187868(rec, id, equipped)`: an ability into the first empty one of the 64 slots at `+0x54` (id in bits 0..14, equipped bit 15).
/// A full list takes nothing.
///
/// orig 0x187868
pub fn add_ability(rec: &mut [u8], id: u16, equipped: bool) {
    for i in 0..64 {
        let o = 0x54 + 2 * i;
        if u16::from_le_bytes([rec[o], rec[o + 1]]) & 0x7fff == 0 {
            rec[o..o + 2].copy_from_slice(&(id & 0x7fff | (equipped as u16) << 15).to_le_bytes());
            return;
        }
    }
}

/// `FUN_00187f18(form, v)`: an ability into the first empty one of a form's 16 slots at `+8`; the slot keeps its bit 15 and gets bit 15
/// set, so the ability is always equipped.
///
/// orig 0x187f18
pub fn add_form_ability(form: &mut [u8], v: u16) {
    for i in 0..16 {
        let o = 8 + 2 * i;
        let old = u16::from_le_bytes([form[o], form[o + 1]]);
        if old & 0x7fff == 0 {
            form[o..o + 2].copy_from_slice(&(old & 0x8000 | v & 0x7fff | 0x8000).to_le_bytes());
            return;
        }
    }
}

/// `FUN_00187728(rec, id)`: a consumable into the member's first empty battle item slot (of the `rec[0x12]` slots, items at `+0x34`,
/// reserved ids at `+0x44`) whose reserved id is this item or empty; an empty reserved id takes the item's id when another slot already
/// reserves it.
///
/// orig 0x187728
pub fn reserve_battle_item(rec: &mut [u8], id: u16) {
    let h = |rec: &[u8], o: usize| u16::from_le_bytes([rec[o], rec[o + 1]]);
    let n = rec[0x12] as usize;
    for i in 0..n {
        if h(rec, 0x34 + 2 * i) != 0 {
            continue;
        }
        let reserved = h(rec, 0x44 + 2 * i);
        if reserved == id {
            rec[0x34 + 2 * i..0x36 + 2 * i].copy_from_slice(&id.to_le_bytes());
            return;
        }
        if reserved == 0 {
            if (0..n).any(|j| h(rec, 0x44 + 2 * j) == id) {
                rec[0x44 + 2 * i..0x46 + 2 * i].copy_from_slice(&id.to_le_bytes());
            }
            rec[0x34 + 2 * i..0x36 + 2 * i].copy_from_slice(&id.to_le_bytes());
            return;
        }
    }
}

/// `FUN_00187dc8(party, id)`: the party-relative offset of the defeat counter of an enemy id: ids 1000..=1046 at `+0xff0`, ids
/// 3000..=3011 at `+0x1170`, one word each; `None` for any other id.
///
/// orig 0x187dc8
pub fn defeat_counter_offset(id: i32) -> Option<usize> {
    let a = (id as u32).wrapping_sub(1000);
    if a <= 0x2e {
        return Some(0xff0 + 4 * a as usize);
    }
    let b = (id as u32).wrapping_sub(3000);
    (b < 0xc).then(|| 0x1170 + 4 * b as usize)
}

/// `FUN_00187e08(party, id)`: when the chapter test (`FUN_00180850`, story flag 0x8d0) says so and the id has a counter below 9999, the
/// counter goes up by one.
///
/// orig 0x187e08
pub fn count_defeat(party: &mut [u8], id: i32, counting: bool) {
    if !counting {
        return;
    }
    if let Some(o) = defeat_counter_offset(id) {
        let n = u32::from_le_bytes(party[o..o + 4].try_into().unwrap());
        if n < 9999 {
            party[o..o + 4].copy_from_slice(&(n + 1).to_le_bytes());
        }
    }
}

/// `FUN_001878f8(slot, v)`: the halfword at party `+0x1994` takes the slot's old value and the slot takes `v` [meaning of the halfword
/// not traced: a one-place stash].
///
/// orig 0x1878f8
pub fn stash_halfword(party: &mut [u8], slot: &mut u16, v: u16) {
    party[party::HALF_1994..party::HALF_1994 + 2].copy_from_slice(&slot.to_le_bytes());
    *slot = v;
}

/// `FUN_00187910(slot)`: the stash at party `+0x1994` (0 reads as 0x29) goes into the slot and is cleared.
///
/// orig 0x187910
pub fn take_stashed_halfword(party: &mut [u8], slot: &mut u16) {
    let o = party::HALF_1994;
    if u16::from_le_bytes([party[o], party[o + 1]]) == 0 {
        party[o..o + 2].copy_from_slice(&0x29u16.to_le_bytes());
    }
    *slot = u16::from_le_bytes([party[o], party[o + 1]]);
    party[o..o + 2].fill(0);
}

/// `FUN_00187e68(party, id)`: the defeat counter of an enemy id (`FUN_00187dc8` finds the word). An id without a counter makes the
/// original read address 0; the port answers 0 [inferred].
///
/// orig 0x187e68
pub fn defeat_count(party: &[u8], id: i32) -> u32 {
    defeat_counter_offset(id).map_or(0, |o| u32::from_le_bytes(party[o..o + 4].try_into().unwrap()))
}

/// `FUN_0014ca28`: one tick of the play time. The word at save `+0x1604` goes up by one unless it is already above 0xcdfe5c3 (it stops at
/// 0xcdfe5c4); the word after it (`+0x1608`) is read and written back unchanged. When the word at `0x33e6fc` is non-zero (`counting`) and the
/// current world byte (`0x33e830`) is below 0x13, the word `world` places on from `+0x160c` (`+0x1604 + 8 + 4 * world`) counts the same way:
/// the time spent in each world. The original runs it from the vblank handler `FUN_0014cc88`.
///
/// orig 0x14ca28
pub fn play_time_tick(save: &mut [u8], counting: bool, world: u8) {
    let bump = |save: &mut [u8], o: usize| {
        let v = u32::from_le_bytes(save[o..o + 4].try_into().unwrap());
        put32(save, o, if v > 0xcdf_e5c3 { v } else { v + 1 });
    };
    bump(save, at::WALLET + 4);
    if counting && world < 0x13 {
        bump(save, at::WALLET + 12 + 4 * world as usize);
    }
}

/// `FUN_001a6810(a, b, c)`: the offset in the chest table (`table`: a count halfword at `+2`, the records of 12 bytes from `+4`) of the first
/// record whose byte 4 is 0 and whose bytes 5, 6 and 7 are `a`, `b` and `c`; `None` (the original answers 0) when there is none. The
/// arguments are 64-bit in the original, so a value above 255 never matches.
///
/// orig 0x1a6810
pub fn find_chest(table: &[u8], a: u64, b: u64, c: u64) -> Option<usize> {
    let n = u16::from_le_bytes([table[2], table[3]]) as usize;
    (0..n).map(|k| 4 + 12 * k).find(|&r| table[r + 4] == 0 && u64::from(table[r + 5]) == a && u64::from(table[r + 6]) == b && u64::from(table[r + 7]) == c)
}

/// `FUN_001be4b8(id)`: the offset in a table (`table`: a count word at `+4`, the records of 16 bytes from `+8`) of the first record whose byte 0
/// is `id` (64-bit in the original, so above 255 never); `None` (the original answers 0) for none or a count below 1.
///
/// orig 0x1be4b8
pub fn find_record_by_id(table: &[u8], id: u64) -> Option<usize> {
    let n = i32::from_le_bytes(table[4..8].try_into().unwrap());
    (0..n.max(0) as usize).map(|k| 8 + 16 * k).find(|&r| u64::from(table[r]) == id)
}

/// Address of the save block in the original (`FUN_001057a8` answers it).
pub const SAVE_ADDR: u32 = 0x33e860;

/// The second argument of the static initialiser `FUN_001057b8` that both of its branches test for.
pub const STATIC_INIT_TOKEN: i64 = 0xffff;

/// `FUN_00105750(save)`: the game mode word of a save block (`+0x1658`; the original's accessor).
///
/// orig 0x105750
pub fn game_level(save: &[u8]) -> u32 {
    u32::from_le_bytes(save[at::GAME_MODE..at::GAME_MODE + 4].try_into().unwrap())
}

/// `FUN_001057a8()`: the address of the global save block, `0x33e860`.
///
/// orig 0x1057a8
pub fn save_ram_address() -> u32 {
    SAVE_ADDR
}

/// `FUN_00105cd8()`: the region word of the executable (global `0x349d44`; [`BootEnv::region`] carries it into the port).
///
/// orig 0x105cd8
pub fn region_word(env: &BootEnv) -> u32 {
    env.region
}

/// `FUN_002b70e0()`: the address of the journal's seen set, the save block's address plus `0x38f0` (`at::JOURNAL`).
///
/// orig 0x2b70e0
pub fn journal_seen_address() -> u32 {
    save_ram_address().wrapping_add(at::JOURNAL as u32)
}

/// `FUN_001a71c0(world)`: the address of a world's four-byte record, `0x340c9c + 4 * world`. A negative `world` means the current world:
/// `current` is the byte at `0x33e830`. The arithmetic is the game's 32-bit one (the argument is cut to 32 bits, the product wraps).
///
/// orig 0x1a71c0
pub fn world_record_address(world: i64, current: u8) -> u32 {
    let w = if world < 0 { i64::from(current) } else { world };
    (w as i32 as u32).wrapping_mul(4).wrapping_add(0x340c9c)
}

/// `FUN_001a6730(prev)`: the next record of the item table. The table (`table` is the pointer word `0x1c95588`, `count` the word at
/// `table + 4`) holds records of 0x18 bytes from `+8`. With no previous record (0) the answer is the first record; otherwise the record
/// after `prev`, or 0 once that would lie at or past the end. All sums are the game's 32-bit ones.
///
/// orig 0x1a6730
pub fn item_table_each(prev: i64, table: u32, count: i32) -> u32 {
    if prev == 0 {
        return table.wrapping_add(8);
    }
    let next = (prev as i32 as u32).wrapping_add(0x18);
    let end = table.wrapping_add((count as u32).wrapping_mul(0x18)).wrapping_add(8);
    if next < end { next } else { 0 }
}

/// `FUN_001057b8(phase, token)`: the save block's static initialiser, the part of the new game's boot that runs on a zeroed block. Only the
/// token `0xffff` does anything. Phase 1 sets up the story tables, the party block, the world records, the NEW tags, the shop words, the
/// moogle and journal records, the config block and the object block (everything `FUN_00105638` does between zeroing the block and writing
/// the header; `fixups` is the object block's fix-up list of the executable). Phase 0 calls an empty function (`FUN_002ee2e0`), so it
/// changes nothing; any other phase changes nothing either.
///
/// orig 0x1057b8
pub fn static_init(save: &mut [u8], fixups: &[u32], phase: i32, token: i64) {
    if token == STATIC_INIT_TOKEN && phase == 1 {
        clear_story_tables(&mut save[at::STORY_TABLES..]);
        init_party(&mut save[at::PARTY..]);
        clear_world_records(&mut save[at::WORLD_RECORDS..]);
        clear_new_seen(&mut save[at::NEW_SEEN..]);
        put32(save, at::SHOP, 0);
        put32(save, at::SHOP + 4, 0);
        clear_four_words(&mut save[at::SHOP_SEEN..]);
        save[at::SHOP_VISITS] = 0;
        clear_moogle(&mut save[at::MOOGLE..]);
        clear_journal(&mut save[at::JOURNAL..]);
        init_config_block(&mut save[at::CONFIG_BLOCK..]);
        init_object_block(&mut save[at::OBJECT_BLOCK..], fixups);
    }
}

/// `FUN_00105890()`: `FUN_001057b8(1, 0xffff)`, the entry the start-up table calls.
///
/// orig 0x105890
pub fn static_init_boot(save: &mut [u8], fixups: &[u32]) {
    static_init(save, fixups, 1, STATIC_INIT_TOKEN);
}

/// `FUN_00194fa8(bit)`: sets bit `bit` of the world-point bit words (save `+0x10b0`, bits counted from the first word's lowest bit).
///
/// orig 0x194fa8
pub fn set_world_point_bit(save: &mut [u8], bit: u32) {
    let o = at::WORLD_POINTS + (bit >> 5) as usize * 4;
    let v = u32::from_le_bytes(save[o..o + 4].try_into().unwrap()) | 1u32.wrapping_shl(bit & 0x1f);
    put32(save, o, v);
}

/// The world-point bit the story flag `flag` stands for in `FUN_00195130`'s map, or `None` for a flag the map does not list (the
/// original then does nothing). Both `0x1822` and `0x1877` map to bit 0x1f.
pub fn event_world_point(flag: i32) -> Option<u32> {
    Some(match flag {
        0x1877 | 0x1822 => 0x1f,
        0x2822 => 0x26,
        0x2023 => 0x29,
        0x3012 => 0x13,
        0x401e => 0x10,
        0x1061 => 0x1e,
        0x8b8 => 0x2f,
        0x141f => 9,
        _ => return None,
    })
}

/// `FUN_00195130(flag)`: the story-flag event map. A handful of flags (one per world-map step) switch on a world-point bit when they are
/// set: [`event_world_point`] picks the bit, [`set_world_point_bit`] sets it. The story flag setter calls it last.
///
/// orig 0x195130
pub fn story_flag_world_point(save: &mut [u8], flag: i32) {
    if let Some(bit) = event_world_point(flag) {
        set_world_point_bit(save, bit);
    }
}

/// The offset in an equipment table (`table`: a count word at `+4`, records of 16 bytes from `+8`, sorted by the halfword at byte 0) of the
/// record whose id is `key`, or `None` (the original answers 0) when there is none. This is the binary search `FUN_0030ff40` with the
/// comparator at `0x333ff8` (`key - record id`); a count the slice cannot hold is cut to what it holds.
///
/// orig 0x1d0120
pub fn find_equipment_record(table: &[u8], key: i64) -> Option<usize> {
    let count = u32::from_le_bytes(table[4..8].try_into().unwrap());
    let count = count.min(((table.len().saturating_sub(8)) / 16) as u32);
    if count == 0 {
        return None;
    }
    let (mut lo, mut hi) = (0u32, count);
    let mut n = count;
    loop {
        let mid = n >> 1;
        let at = 8 + 16 * mid as usize;
        let id = i64::from(u16::from_le_bytes([table[at], table[at + 1]]));
        let diff = key.wrapping_sub(id);
        if diff >= 0 {
            lo = mid + 1;
            if diff == 0 {
                return Some(at);
            }
        } else {
            hi = mid;
        }
        if lo >= hi {
            return None;
        }
        n = lo + hi;
    }
}

/// `FUN_001a67b0(item)`: the equipment record of an item record (`item`: byte 2 the kind). Kinds below 2 and above 15 have none; kinds 2 to
/// 13 look up the halfword at `+6`, kinds 14 and 15 the halfword at `+4`, in [`find_equipment_record`]'s table.
///
/// orig 0x1a67b0
pub fn item_equipment_record(item: &[u8], table: &[u8]) -> Option<usize> {
    let key = match item[2] {
        0 | 1 => return None,
        2..=13 => u16::from_le_bytes([item[6], item[7]]),
        14 | 15 => u16::from_le_bytes([item[4], item[5]]),
        _ => return None,
    };
    find_equipment_record(table, i64::from(key))
}

/// The item id the give list (`FUN_001d7508`) hands back, with the target it names: the inventory.
pub const GIVE_LIST_TARGET: i32 = 100;

/// `FUN_001d7508(ids, count)`: returns each listed item to the inventory. For every non-zero entry of the first `count` halfwords `give` is
/// called with the id (zero-extended, as the original passes it; the target is [`GIVE_LIST_TARGET`], equipped 0) and the entry is cleared.
///
/// orig 0x1d7508
pub fn give_list(ids: &mut [u16], count: i32, mut give: impl FnMut(i32)) {
    for id in ids.iter_mut().take(count.max(0) as usize) {
        if *id != 0 {
            give(i32::from(*id));
            *id = 0;
        }
    }
}

/// `FUN_001ddff0()`: an empty function (the object block's start-up hook); the boot calls it and nothing changes.
///
/// orig 0x1ddff0
pub fn object_block_hook() {}

/// Which release routine freed a card unit buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Release {
    /// The array delete `FUN_001062b0`.
    Delete,
    /// The loaded-data release `FUN_00106280`.
    LoadData,
}

/// The card unit's global pointer words (`0x36cb08..0x36cb40`) that the shutdown routines clear. Each buffer word holds the address of
/// a buffer the start-up routine `FUN_0023c948` allocated (the sizes are in `kh2_save::dir::Buffers`), or 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CardWords {
    /// `0x36cb08`: set to 0 by the unit shutdown.
    pub ready: u32,
    /// `0x36cb0c`: the name buffer.
    pub name: u32,
    /// `0x36cb10`: the path buffer.
    pub path: u32,
    /// `0x36cb14`: the file buffer.
    pub file: u32,
    /// `0x36cb18`: the card info block (a loaded-data allocation).
    pub info: u32,
    /// `0x36cb1c`: the per-save file list (a loaded-data allocation).
    pub list: u32,
    /// `0x36cb20`: the aligned listing table, an address inside the allocation of `table_alloc`; cleared, never freed itself.
    pub table: u32,
    /// `0x36cb24`: the listing table's allocation.
    pub table_alloc: u32,
    /// `0x36cb38`: the CRC unit's flag word.
    pub crc_flag: u32,
    /// `0x36cb3c`: the CRC table buffer.
    pub crc_table: u32,
}

impl CardWords {
    /// `FUN_0023cac0()`: frees the six file layer buffers that are set, in the order name, path, file, info, list, listing table, clears
    /// each word, and clears the aligned table word.
    ///
    /// orig 0x23cac0
    pub fn free_buffers(&mut self, mut free: impl FnMut(Release, u32)) {
        if self.name != 0 {
            free(Release::Delete, self.name);
            self.name = 0;
        }
        if self.path != 0 {
            free(Release::Delete, self.path);
            self.path = 0;
        }
        if self.file != 0 {
            free(Release::Delete, self.file);
            self.file = 0;
        }
        if self.info != 0 {
            free(Release::LoadData, self.info);
            self.info = 0;
        }
        if self.list != 0 {
            free(Release::LoadData, self.list);
            self.list = 0;
        }
        if self.table_alloc != 0 {
            free(Release::Delete, self.table_alloc);
            self.table_alloc = 0;
        }
        self.table = 0;
    }

    /// `FUN_0023e410()`: the CRC unit's shutdown: clears its flag word and frees the table buffer when there is one.
    ///
    /// orig 0x23e410
    pub fn free_crc(&mut self, mut free: impl FnMut(Release, u32)) {
        self.crc_flag = 0;
        if self.crc_table != 0 {
            free(Release::Delete, self.crc_table);
            self.crc_table = 0;
        }
    }

    /// `FUN_0023c8a8()`: the unit's shutdown: [`CardWords::free_buffers`], [`CardWords::free_crc`], then clears the ready word.
    ///
    /// orig 0x23c8a8
    pub fn shutdown(&mut self, mut free: impl FnMut(Release, u32)) {
        self.free_buffers(&mut free);
        self.free_crc(&mut free);
        self.ready = 0;
    }
}

/// `FUN_00254038(phase, token)`: the static initialiser of the world record the field code keeps at `0x36cee0` (4 bytes): with phase 1 and
/// token `0xffff` it runs the world slot initialiser `FUN_001a71a8` on `slot`; anything else changes nothing.
///
/// orig 0x254038
pub fn static_init_world_slot(slot: &mut [u8], phase: i32, token: i64) {
    if token == STATIC_INIT_TOKEN && phase == 1 {
        init_world_slot(slot);
    }
}

/// `FUN_002540a8()`: `FUN_00254038(1, 0xffff)`, the entry the start-up table calls.
///
/// orig 0x2540a8
pub fn static_init_world_slot_boot(slot: &mut [u8]) {
    static_init_world_slot(slot, 1, STATIC_INIT_TOKEN);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_constants_agree_with_each_other() {
        assert_eq!(at::PARTY + party::LEN, at::WORLD_RECORDS, "the party block ends where the world records begin");
        assert_eq!(at::PROGRAMS, at::STORY_TABLES + 3);
        assert_eq!(at::FLAGS, at::STORY_TABLES + 0xe44);
        assert_eq!(at::WORLD_POINTS, at::STORY_TABLES + 0x10a4);
        assert_eq!(LEN, SaveBlock::new().bytes().len());
    }

    #[test]
    fn the_event_map_sets_the_listed_world_point_bits_only() {
        let mut b = vec![0u8; LEN];
        story_flag_world_point(&mut b, 0x8b8);
        story_flag_world_point(&mut b, 0x1822);
        story_flag_world_point(&mut b, 0x8b9);
        assert_eq!(u32::from_le_bytes(b[at::WORLD_POINTS..at::WORLD_POINTS + 4].try_into().unwrap()), 1 << 0x1f);
        assert_eq!(u32::from_le_bytes(b[at::WORLD_POINTS + 4..at::WORLD_POINTS + 8].try_into().unwrap()), 1 << 0xf);
    }

    #[test]
    fn the_give_list_gives_each_entry_once_and_clears_it() {
        let mut ids = [5u16, 0, 0xffff, 7];
        let mut given = Vec::new();
        give_list(&mut ids, 3, |id| given.push(id));
        assert_eq!(given, [5, 0xffff]);
        assert_eq!(ids, [0, 0, 0, 7], "the entry past the count stays");
        give_list(&mut ids, -1, |_| panic!("a negative count gives nothing"));
    }

    #[test]
    fn the_equipment_search_finds_a_record_or_none() {
        let mut t = vec![0u8; 8 + 16 * 3];
        t[4] = 3;
        for (i, id) in [2u16, 9, 0x8001].into_iter().enumerate() {
            t[8 + 16 * i..10 + 16 * i].copy_from_slice(&id.to_le_bytes());
        }
        assert_eq!(find_equipment_record(&t, 9), Some(24));
        assert_eq!(find_equipment_record(&t, 0x8001), Some(40));
        assert_eq!(find_equipment_record(&t, 3), None);
        let mut item = [0u8; 8];
        item[2] = 14;
        item[4] = 2;
        assert_eq!(item_equipment_record(&item, &t), Some(8));
        item[2] = 16;
        assert_eq!(item_equipment_record(&item, &t), None);
    }

    #[test]
    fn the_header_letter_follows_the_region_word() {
        assert_eq!(header_word(1), 0x4a32_484b);
        assert_eq!(header_word(2), 0x5532_484b);
        assert_eq!(header_word(0), 0x4532_484b);
        assert_eq!(header_word(7), 0x4532_484b);
    }

    #[test]
    fn a_new_game_block_has_the_header_the_modes_and_the_party_defaults() {
        let mut s = SaveBlock::from_bytes(&vec![0xa5; LEN]);
        s.init_save(&BootEnv { region: 2, fixups: vec![] }, |_| {});
        assert_eq!(s.magic(), 0x5532_484b);
        assert_eq!((s.u32_at(at::HEADER_WORD), s.game_mode(), s.options(), s.wallet()), (0x2d, 1, 0x40b, 0));
        assert_eq!(s.party()[party::SUMMON_LEVEL], 1);
        assert_eq!(s.party()[party::DRIVE], 100);
        assert_eq!(s.bytes()[at::WORLD_RECORDS + 0x1ac], 1);
        assert_eq!(s.shop_seen(), [0; 4]);
        // The object block's cells are numbered.
        assert_eq!(&s.bytes()[at::OBJECT_BLOCK + 0x18..at::OBJECT_BLOCK + 0x1a], &[0, 1]);
        assert_eq!(&s.bytes()[at::OBJECT_BLOCK + 0x18 + 0x40 * 26..][..2], &[8, 3]);
    }

    #[test]
    fn the_fixup_list_stops_at_an_offset_of_zero() {
        let mut b = vec![0u8; 0x900];
        apply_object_fixups(&mut b, &[2, 5, 4, 0xff, 0, 9, 8, 8]);
        assert_eq!((b[0x6c2], b[0x6c4], b[0x6c8]), (5, 0xff, 0));
        apply_object_fixups(&mut b, &[2, 5, 0, 0]);
        assert_eq!(b[0x6c2], 10);
        apply_object_fixups(&mut b, &[]);
    }

    #[test]
    fn the_boot_calls_mid_between_the_head_and_the_tail() {
        let mut s = SaveBlock::new();
        s.init_save(&BootEnv::default(), |b| {
            // Between the two halves the game mode is still the cleared 0, the world records' last byte is still 0.
            assert_eq!((b.game_mode(), b.bytes()[at::WORLD_RECORDS + 0x1ac]), (0, 0));
            b.set_u8(at::WORLD_RECORDS + 0x1ac, 9);
        });
        assert_eq!(s.bytes()[at::WORLD_RECORDS + 0x1ac], 1, "the tail sets it afterwards");
    }
}
