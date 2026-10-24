//! The map object and the map model instance (WP-L1-P22, spec `docs/systems/core-runtime/collision.md`).
//!
//! The map object is the record the area loader keeps for the current map file: a file name buffer at `+0x00`, the
//! cache buffer handle at `+0x20`, then one word per table of the loaded BAR (`+0x24` to `+0x50`: the model class,
//! texture animation data, the draw tree input, the five collision files, the property table), a small entry table
//! at `+0x54`, two place picture words (`+0x64`, `+0x68`) and one 12-byte record per map model from `+0x78`
//! (`+0x78`, `+0x7c`, `+0x80`; 96 of them). The port keeps the record as its raw 0x500 bytes so the getters and the
//! clear compare byte for byte with the game's. A word that names a file is an address in the game (the caller
//! resolves it); the port never dereferences one.
//!
//! The map model instance is the 0x90-byte block `FUN_00142ba8` allocates for the drawn map (the game keeps it at
//! `DAT_00355948`); only the words the clusters here touch are named: the first word (the model handle, `+0x00`),
//! the colour bytes (`+0x54..+0x56`) and the switch byte `+0x8c`.

/// The size of the map object record.
pub const MAP_OBJECT_SIZE: usize = 0x500;

/// The size of the map model instance.
pub const MAP_MODEL_SIZE: usize = 0x90;

/// How many 12-byte model records the map object holds (`+0x78` up to `+0x4f8`: 96).
pub const MAP_MODELS: usize = 96;

/// The map object record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapObject {
    raw: Vec<u8>,
}

impl Default for MapObject {
    fn default() -> Self {
        MapObject { raw: vec![0; MAP_OBJECT_SIZE] }
    }
}

impl MapObject {
    /// A record holding `bytes` (shorter input is zero padded, longer is cut at 0x500).
    pub fn from_bytes(bytes: &[u8]) -> MapObject {
        let mut raw = vec![0; MAP_OBJECT_SIZE];
        let n = bytes.len().min(MAP_OBJECT_SIZE);
        raw[..n].copy_from_slice(&bytes[..n]);
        MapObject { raw }
    }

    /// The record's bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.raw
    }

    /// The word at byte offset `o`.
    pub fn word(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.raw[o..o + 4].try_into().unwrap())
    }

    /// Stores the word at byte offset `o`.
    pub fn set_word(&mut self, o: usize, v: u32) {
        self.raw[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    /// orig 0x1490f0: clears the first name byte and the words `+0x20` to `+0x74`, the model records from `+0x78` to
    /// `+0x4f4` and the word `+0x4f8`. The rest of the name buffer and the word `+0x4fc` stay as they were.
    pub fn clear(&mut self) {
        for o in (0x20..=0x50).step_by(4) {
            self.set_word(o, 0);
        }
        // The loop at +0x54 clears four words (a counter that runs 3, 2, 1, 0, -1 with the test `-1 < n`).
        for o in (0x54..0x64).step_by(4) {
            self.set_word(o, 0);
        }
        for o in (0x64..=0x74).step_by(4) {
            self.set_word(o, 0);
        }
        self.set_word(0x4f8, 0);
        self.raw[0] = 0;
        self.set_word(0x78, 0);
        // Per model record `o` (0x0c apart, 96 of them): its second and third words, and the next record's first
        // word unless this is the last record.
        let mut o = 0usize;
        loop {
            self.set_word(o + 0x7c, 0);
            self.set_word(o + 0x80, 0);
            if 0x480 <= o + 0xc {
                break;
            }
            self.set_word(o + 0x84, 0);
            o += 0xc;
        }
    }

    /// orig 0x1490c0: the static initialiser of the map object (the constructor list's word); it clears the record.
    pub fn init(&mut self) {
        self.clear();
    }

    /// orig 0x1490d8: the reset of the map object (the area loader reaches it through the host); it clears the record.
    pub fn reset(&mut self) {
        self.clear();
    }

    /// orig 0x1491a0: the model class word (`+0x24`).
    pub fn model_class(&self) -> u32 {
        self.word(0x24)
    }
    /// orig 0x1491a8: the word at `+0x28` (the model's texture animation data).
    pub fn texture_data(&self) -> u32 {
        self.word(0x28)
    }
    /// orig 0x1491b0: the word at `+0x2c` (the input of the draw tree build).
    pub fn draw_tree(&self) -> u32 {
        self.word(0x2c)
    }
    /// orig 0x1491b8: the word at `+0x30` (the file of collision slot 0x355934, the walking map).
    pub fn collision_main(&self) -> u32 {
        self.word(0x30)
    }
    /// orig 0x1491c0: the word at `+0x34` (the file of collision slot 0x355938).
    pub fn collision_second(&self) -> u32 {
        self.word(0x34)
    }
    /// orig 0x1491c8: the word at `+0x38` (the file of collision slot 0x35593c, the floor colour map).
    pub fn collision_colour(&self) -> u32 {
        self.word(0x38)
    }
    /// orig 0x1491d0: the word at `+0x3c` (the file of collision slot 0x355940).
    pub fn collision_aux(&self) -> u32 {
        self.word(0x3c)
    }
    /// orig 0x1491d8: the word at `+0x40` (the file of collision slot 0x355944, the floor kind object).
    pub fn collision_kinds(&self) -> u32 {
        self.word(0x40)
    }
    /// orig 0x1491e0: the property table (`+0x44`).
    pub fn prop_table(&self) -> u32 {
        self.word(0x44)
    }
    /// orig 0x1491e8: the word at `+0x48` (a table the drawn map's set-up reads).
    pub fn draw_table(&self) -> u32 {
        self.word(0x48)
    }
    /// orig 0x1491f0: the word at `+0x4c` (a second table of the drawn map's set-up).
    pub fn draw_table_b(&self) -> u32 {
        self.word(0x4c)
    }
    /// orig 0x1491f8: the word at `+0x50`.
    pub fn word_50(&self) -> u32 {
        self.word(0x50)
    }
    /// orig 0x149200: entry `n` of the table at `+0x54`.
    pub fn entry(&self, n: usize) -> u32 {
        self.word(0x54 + 4 * n)
    }
    /// orig 0x149210: the place picture word (`+0x64`).
    pub fn place_picture(&self) -> u32 {
        self.word(0x64)
    }
    /// orig 0x149218: the second place picture word (`+0x68`).
    pub fn place_picture_b(&self) -> u32 {
        self.word(0x68)
    }
    /// orig 0x149220: the first word of map model `n` (`+0x78`, 12 bytes apart).
    pub fn model_word(&self, n: usize) -> u32 {
        self.word(0x78 + 0xc * n)
    }
    /// orig 0x149238: the second word of map model `n` (`+0x7c`).
    pub fn model_word_b(&self, n: usize) -> u32 {
        self.word(0x7c + 0xc * n)
    }
    /// orig 0x149250: the third word of map model `n` (`+0x80`).
    pub fn model_word_c(&self, n: usize) -> u32 {
        self.word(0x80 + 0xc * n)
    }

    /// orig 0x149ad0: the cache buffer handle the map file lives in (`+0x20`): the word the original hands to the
    /// cache's free call.
    pub fn buffer_handle(&self) -> u32 {
        self.word(0x20)
    }

    /// orig 0x149328: the cache read request the area loader issues for the map file: the arguments after the object
    /// itself (the original calls the cache's `ReadRequest(object, 0x438, 0)`).
    pub fn read_request() -> (u32, u32) {
        (0x438, 0)
    }

    /// The file name buffer (`+0x00`, up to the cache handle at `+0x20`) up to its first zero byte.
    pub fn name(&self) -> &[u8] {
        let end = self.raw[..0x20].iter().position(|&b| b == 0).unwrap_or(0x20);
        &self.raw[..end]
    }

    /// orig 0x1492e8: stores the map file name: a string copy of `name` and its terminating zero over the name buffer
    /// (the original has no length limit; the port stops at the cache handle word `+0x20` [inferred: real names are
    /// shorter]). The original also measures the string first and uses nothing of it.
    pub fn set_name(&mut self, name: &[u8]) {
        let n = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        let n = n.min(0x1f);
        self.raw[..n].copy_from_slice(&name[..n]);
        self.raw[n] = 0;
    }

    /// orig 0x1492b8: gives the object the default file name of the current area when it has none (first name byte
    /// zero). `format` is `map_file_name` of the area.
    pub fn set_default_name(&mut self, name: &[u8]) {
        if self.raw[0] == 0 {
            self.set_name(name);
        }
    }
}

/// orig 0x149268: the map file name of an area: the executable's format string at 0x379038 (`map/%s/%s.map`) applied to
/// the language directory string `DAT_00349d40` points at and the area's name (`AREA::get_area_name`, 0x105010).
pub fn map_file_name(language: &str, area: &str) -> Vec<u8> {
    format!("map/{language}/{area}.map").into_bytes()
}

/// The map model instance block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapModel {
    raw: Vec<u8>,
}

impl Default for MapModel {
    fn default() -> Self {
        MapModel { raw: vec![0; MAP_MODEL_SIZE] }
    }
}

impl MapModel {
    /// An instance holding `bytes` (zero padded or cut at 0x90).
    pub fn from_bytes(bytes: &[u8]) -> MapModel {
        let mut raw = vec![0; MAP_MODEL_SIZE];
        let n = bytes.len().min(MAP_MODEL_SIZE);
        raw[..n].copy_from_slice(&bytes[..n]);
        MapModel { raw }
    }

    /// The instance's bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.raw
    }

    /// orig 0x143230: the first word (the model handle the instance wraps).
    pub fn handle(&self) -> u32 {
        u32::from_le_bytes(self.raw[0..4].try_into().unwrap())
    }

    /// orig 0x143208: stores the three colour bytes (`+0x54`, `+0x55`, `+0x56`).
    pub fn set_colour(&mut self, a: u8, b: u8, c: u8) {
        self.raw[0x56] = c;
        self.raw[0x54] = a;
        self.raw[0x55] = b;
    }

    /// orig 0x1430d8: clears the switch byte `+0x8c`.
    pub fn clear_switch(&mut self) {
        self.raw[0x8c] = 0;
    }

    /// orig 0x1430e8: sets the switch byte `+0x8c`. The function is real code but its only caller is the twin of
    /// 0x15edb0 at 0x15ee58, which nothing calls or points at (calls, address builds and data words searched).
    pub fn set_switch(&mut self) {
        self.raw[0x8c] = 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clear_leaves_the_name_tail_and_the_class_words() {
        let mut o = MapObject::from_bytes(&[0xff; MAP_OBJECT_SIZE]);
        o.clear();
        assert_eq!(o.raw[0], 0);
        assert_eq!(o.raw[1], 0xff, "only the first name byte is cleared");
        assert_eq!(o.word(0x04), 0xffff_ffff);
        assert_eq!(o.word(0x1c), 0xffff_ffff);
        for off in [0x20, 0x50, 0x54, 0x60, 0x64, 0x74, 0x78, 0x4f4, 0x4f8] {
            assert_eq!(o.word(off), 0, "+{off:#x}");
        }
        assert_eq!(o.word(0x4fc), 0xffff_ffff);
        assert_eq!(o.model_word_c(MAP_MODELS - 1), 0);
    }

    #[test]
    fn the_getters_read_their_offsets() {
        let mut o = MapObject::default();
        for k in 0..0x40 {
            o.set_word(0x20 + 4 * k, 0x100 + k as u32);
        }
        assert_eq!(o.model_class(), 0x101);
        assert_eq!(o.collision_main(), 0x104);
        assert_eq!(o.entry(2), 0x10f);
        assert_eq!(o.model_word_b(1), o.word(0x88));
    }

    #[test]
    fn a_name_is_set_once_and_the_default_fills_an_empty_one() {
        let mut o = MapObject::default();
        o.set_default_name(b"map/tt00.map\0");
        assert_eq!(o.name(), b"map/tt00.map");
        o.set_default_name(b"other");
        assert_eq!(o.name(), b"map/tt00.map");
        o.set_name(b"x");
        assert_eq!(o.name(), b"x");
    }

    #[test]
    fn the_model_switch_and_colour() {
        let mut m = MapModel::default();
        m.set_switch();
        assert_eq!(m.bytes()[0x8c], 1);
        m.clear_switch();
        m.set_colour(1, 2, 3);
        assert_eq!((m.bytes()[0x54], m.bytes()[0x55], m.bytes()[0x56], m.bytes()[0x8c]), (1, 2, 3, 0));
    }
}
