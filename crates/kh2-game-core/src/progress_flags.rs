//! The story-flag primitives of the original on its own byte layouts (RULES C7): flag pages, the small gates built on them, the
//! follow-up checks that run after a flag is set, the Re:COM-style entry table and the save-flag bits. Every function takes the
//! memory it works on as a byte slice that starts at the original's address, so the factory's oracle tests
//! (`tests/orig_story.rs`) compare these ports with the original byte for byte.
//!
//! Tables the original reads from its executable (the pairs of `story_chain`, the triples of `record_bits_for_flag`) are passed in
//! as raw bytes: the app reads them from the user's disc at run time and no table value lives in the repo (RULES A3).
//!
//! `Story` (progress.rs) keeps the same flag bits in its own words and runs the side-effect lists; the functions here are the
//! original's small routines around that setter. `set_flag` of the [`StoryEnv`] is the full setter `FUN_0017ff60`, which this file
//! does not port.

/// Where the original keeps the flag pages (0x20 bytes per world).
pub const FLAGS_ADDR: u32 = 0x33f6b0;
/// Where the original keeps the save-flag bits (`FUN_001a7780` and `FUN_001a77b8` index words from here).
pub const SAVE_FLAGS_ADDR: u32 = 0x33fe20;
/// Entries of the Re:COM-style table and the size of one entry in bytes.
pub const RECOM_ENTRIES: usize = 8;
pub const RECOM_ENTRY: usize = 12;

fn le32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn put32(b: &mut [u8], o: usize, v: u32) {
    if let Some(s) = b.get_mut(o..o + 4) {
        s.copy_from_slice(&v.to_le_bytes());
    }
}

/// Byte offset of the word that holds flag `id` inside the flag pages, and its bit mask: page `id >> 10` (arithmetic shift), word
/// `(id & 0x3ff) >> 5`. `None` for a negative page (the original would read the memory before the pages).
fn flag_slot(id: i32) -> Option<(usize, u32)> {
    let page = usize::try_from(id >> 10).ok()?;
    Some((page * 0x20 + ((id as u32 & 0x3ff) >> 5) as usize * 4, 1u32 << (id as u32 & 0x1f)))
}

/// orig 0x1802e0: flag `id` is set in the pages `flags` (the bytes from 0x33f6b0). Id 0xffff is always clear. A word past the end of
/// the slice reads as clear [inferred: the game reads whatever memory follows the pages; no caller passes such an id].
pub fn check_flag(flags: &[u8], id: i32) -> bool {
    if id == 0xffff {
        return false;
    }
    flag_slot(id).and_then(|(o, m)| le32(flags, o).map(|w| w & m != 0)).unwrap_or(false)
}

/// orig 0x180298: clears flag `id` in the pages, with none of the setter's side effects. A word past the end of the slice is left alone.
pub fn clear_flag(flags: &mut [u8], id: i32) {
    if let Some((o, m)) = flag_slot(id)
        && let Some(w) = le32(flags, o)
    {
        put32(flags, o, w & !m);
    }
}

/// orig 0x180330: the flag 0x102b is set (the second-part-of-the-game switch several gates and the world flags read).
pub fn flag_102b(flags: &[u8]) -> bool {
    check_flag(flags, 0x102b)
}

/// orig 0x180850: the flag 0x8d0 is set (the chapter switch the story setter also tests by value).
pub fn flag_8d0(flags: &[u8]) -> bool {
    check_flag(flags, 0x8d0)
}

/// The shape shared by the Twilight Town gates: open (true) unless flag 0x817 is set while every one of `others` is clear.
fn gate(flags: &[u8], others: &[i32]) -> bool {
    !(check_flag(flags, 0x817) && others.iter().all(|&f| !check_flag(flags, f)))
}

/// orig 0x180638: the gate on flag 0x81c.
pub fn gate_81c(flags: &[u8]) -> bool {
    gate(flags, &[0x81c])
}

/// orig 0x180680: the gate on flag 0x821 (the battle gate; docs/systems/ui/items.md).
pub fn gate_821(flags: &[u8]) -> bool {
    gate(flags, &[0x821])
}

/// orig 0x1806c8: the gate on flag 0x849 (the command-control gate).
pub fn gate_849(flags: &[u8]) -> bool {
    gate(flags, &[0x849])
}

/// orig 0x180710: the gate on flag 0x834 (the camp-menu gate).
pub fn gate_834(flags: &[u8]) -> bool {
    gate(flags, &[0x834])
}

/// orig 0x180758: the gate on flag 0x8aa (the drive gate).
pub fn gate_8aa(flags: &[u8]) -> bool {
    gate(flags, &[0x8aa])
}

/// orig 0x1807a0: open unless flag 0x817 is set while both 0x142b and 0x2001 are clear.
pub fn gate_142b_2001(flags: &[u8]) -> bool {
    gate(flags, &[0x142b, 0x2001])
}

/// orig 0x1807f8: open unless flag 0x817 is set while both 0x142b and 0x2018 are clear.
pub fn gate_142b_2018(flags: &[u8]) -> bool {
    gate(flags, &[0x142b, 0x2018])
}

/// What the story code around the setter reaches outside the flag pages: the setter `FUN_0017ff60` (bit, side-effect list, follow-up
/// calls), the item lookup `FUN_001a1cc8`, the item removal `FUN_001a1ba8` and the third drive-gauge byte (`0x340c92`).
pub trait StoryEnv {
    /// `FUN_001802e0`.
    fn flag(&self, id: i32) -> bool;
    /// `FUN_0017ff60`: the full setter. The ports below only decide which flag to set.
    fn set_flag(&mut self, id: i32);
    /// `FUN_001a1cc8(item)`: how many of the item the player holds (a byte; for key items 0 or 1).
    fn owned_count(&self, item: i32) -> u32;
    /// `FUN_001a1ba8(item, member)`: removes the item (member 100 = the shared stock).
    fn remove_item(&mut self, item: i32, member: i32);
    /// The byte at 0x340c92.
    fn drive_gauge_2(&self) -> u8;
}

/// orig 0x180350: when flag `target` is clear and flag `condition` is set, sets `target` through the setter and answers true.
pub fn set_when(env: &mut impl StoryEnv, target: i32, condition: i32) -> bool {
    if !env.flag(target) && env.flag(condition) {
        env.set_flag(target);
        true
    } else {
        false
    }
}

/// orig 0x1803b0: the follow-up for a just-set flag `id`. The five ids 0x2409, 0x2411, 0x2419, 0x2421 and 0x2428 remove item 0x20
/// from the shared stock (`remove_item(0x20, 100)`). Any other flag, when the player holds item 0x20, tries the five
/// (target, condition) pairs of `pairs` in order through [`set_when`]. `pairs` is the executable's 40 bytes at 0x37a650 (five pairs
/// of words); the jump table at 0x37a678 sends exactly the five ids above to the removal.
pub fn story_chain(env: &mut impl StoryEnv, pairs: &[u8], id: i32) {
    if matches!(id, 0x2409 | 0x2411 | 0x2419 | 0x2421 | 0x2428) {
        env.remove_item(0x20, 100);
        return;
    }
    if env.owned_count(0x20) != 0 {
        for k in 0..5 {
            let target = le32(pairs, 8 * k).unwrap_or(0) as i32;
            let condition = le32(pairs, 8 * k + 4).unwrap_or(0) as i32;
            set_when(env, target, condition);
        }
    }
}

/// orig 0x180450: the staged flags 0x2c26, 0x2c27, 0x2c28 and 0x2c2d. Only the first stage whose predecessor chain allows it is
/// looked at: while 0x2c26 is clear it is set when 0x2c31 is set and item 0x57 is held; else while 0x2c27 is clear it is set when
/// 0x2c34 is set and the drive-gauge byte is at least 5; else while 0x2c28 is clear it is set when 0x2c37 is set and at least two of
/// item 0x57 are held; else, with 0x2c2d clear, it is set when 0x2c3a is set and at least three of item 0x17 are held.
pub fn staged_flags(env: &mut impl StoryEnv) {
    let set = if !env.flag(0x2c26) {
        (env.flag(0x2c31) && env.owned_count(0x57) > 0).then_some(0x2c26)
    } else if !env.flag(0x2c27) {
        (env.flag(0x2c34) && env.drive_gauge_2() >= 5).then_some(0x2c27)
    } else if !env.flag(0x2c28) {
        (env.flag(0x2c37) && env.owned_count(0x57) >= 2).then_some(0x2c28)
    } else {
        (!env.flag(0x2c2d) && env.flag(0x2c3a) && env.owned_count(0x17) >= 3).then_some(0x2c2d)
    };
    if let Some(id) = set {
        env.set_flag(id);
    }
}

/// orig 0x180558: when flag 0x102b is set and 0x1807 and 0x1809 are clear and 0x1822 is set, sets 0x1809 if flag 0x1865 is set, else 0x1807.
pub fn second_part_flags(env: &mut impl StoryEnv) {
    if env.flag(0x102b) && !env.flag(0x1807) && !env.flag(0x1809) && env.flag(0x1822) {
        env.set_flag(if env.flag(0x1865) { 0x1809 } else { 0x1807 });
    }
}

/// orig 0x1805d0: for a just-set flag `id`, calls `set_bit(group, bit)` once for every entry of `table` whose first word equals `id`.
/// `table` is the executable's 0xa8 bytes at 0x37a6f8 (fourteen entries of three words: flag, group, bit); `set_bit` is
/// `FUN_0019d7e0`, which sets bit `bit` of the words at `0x33fd18 + 8 * group`.
pub fn record_bits_for_flag(table: &[u8], id: i32, mut set_bit: impl FnMut(i32, i32)) {
    for e in table.chunks_exact(12).take(14) {
        let w = |k: usize| u32::from_le_bytes([e[4 * k], e[4 * k + 1], e[4 * k + 2], e[4 * k + 3]]) as i32;
        if w(0) == id {
            set_bit(w(1), w(2));
        }
    }
}

/// orig 0x1a7780: clears save-flag bit `n` (word `n >> 5` of the bits at 0x33fe20, bit `n & 0x1f`). A word past the slice is left alone.
pub fn save_flag_reset(bits: &mut [u8], n: u32) {
    let o = (n >> 5) as usize * 4;
    if let Some(w) = le32(bits, o) {
        put32(bits, o, w & !(1u32 << (n & 0x1f)));
    }
}

/// orig 0x1a77b8: save-flag bit `n` is set. A word past the slice reads as clear [inferred].
pub fn save_flag_check(bits: &[u8], n: u32) -> bool {
    le32(bits, (n >> 5) as usize * 4).is_some_and(|w| w & (1u32 << (n & 0x1f)) != 0)
}

/// orig 0x1a77e8: resets a 0x18-byte record: word 0 to -1, words 1, 3, 4 and 5 to 0 (word 2 is kept).
pub fn reset_record(rec: &mut [u8]) {
    put32(rec, 20, 0);
    put32(rec, 0, 0xffff_ffff);
    put32(rec, 4, 0);
    put32(rec, 12, 0);
    put32(rec, 16, 0);
}

// --- the eight-entry table (the original's RECOM flag block: 0x60 bytes, entries of `id`, `aux`, a switch byte) -------------

/// orig 0x19de70: clears the first word of every entry.
pub fn recom_clear(t: &mut [u8]) {
    for k in 0..RECOM_ENTRIES {
        put32(t, k * RECOM_ENTRY, 0);
    }
}

/// orig 0x19dea0: the index of the first entry whose first word is 0, or `None` when all eight are taken.
pub fn recom_first_free(t: &[u8]) -> Option<usize> {
    (0..RECOM_ENTRIES).find(|&k| le32(t, k * RECOM_ENTRY) == Some(0))
}

/// orig 0x19ded0: the index of the first entry with first word `id` and second word `aux`, or `None`.
pub fn recom_find(t: &[u8], id: i32, aux: i32) -> Option<usize> {
    (0..RECOM_ENTRIES).find(|&k| le32(t, k * RECOM_ENTRY) == Some(id as u32) && le32(t, k * RECOM_ENTRY + 4) == Some(aux as u32))
}

fn recom_put(t: &mut [u8], id: i32, aux: i32, on: u8) {
    // The original writes through whatever the two searches return: with no match and no free entry that is a null pointer.
    // The port does nothing then [inferred: the game never fills the table; the oracle test keeps one entry free].
    if let Some(k) = recom_find(t, id, aux).or_else(|| recom_first_free(t)) {
        put32(t, k * RECOM_ENTRY, id as u32);
        put32(t, k * RECOM_ENTRY + 4, aux as u32);
        if let Some(b) = t.get_mut(k * RECOM_ENTRY + 8) {
            *b = on;
        }
    }
}

/// orig 0x19df08: stores (`id`, `aux`) with the switch byte 1, in the matching entry or else the first free one.
pub fn recom_switch_on(t: &mut [u8], id: i32, aux: i32) {
    recom_put(t, id, aux, 1);
}

/// orig 0x19df68: the same with the switch byte 0.
pub fn recom_switch_off(t: &mut [u8], id: i32, aux: i32) {
    recom_put(t, id, aux, 0);
}

/// orig 0x19dfc0: frees the entry matching (`id`, `aux`) by zeroing its first word.
pub fn recom_remove(t: &mut [u8], id: i32, aux: i32) {
    if let Some(k) = recom_find(t, id, aux) {
        put32(t, k * RECOM_ENTRY, 0);
    }
}

fn recom_has(t: &[u8], id: i32, aux: i32, on: bool) -> bool {
    (0..RECOM_ENTRIES).any(|k| {
        let o = k * RECOM_ENTRY;
        le32(t, o) == Some(id as u32)
            && (le32(t, o + 4) == Some(u32::MAX) || le32(t, o + 4) == Some(aux as u32))
            && t.get(o + 8).is_some_and(|&b| (b != 0) == on)
    })
}

/// orig 0x19dfe8: some entry with first word `id`, second word -1 (any) or `aux`, and a nonzero switch byte.
pub fn recom_is_on(t: &[u8], id: i32, aux: i32) -> bool {
    recom_has(t, id, aux, true)
}

/// orig 0x19e038: some entry with first word `id`, second word -1 (any) or `aux`, and a zero switch byte.
pub fn recom_is_off(t: &[u8], id: i32, aux: i32) -> bool {
    recom_has(t, id, aux, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Env {
        flags: Vec<u8>,
        sets: Vec<i32>,
        removed: Vec<(i32, i32)>,
        items: Vec<(i32, u32)>,
        gauge: u8,
    }

    impl Env {
        fn new() -> Env {
            Env { flags: vec![0; 0x800], ..Env::default() }
        }
        fn with(mut self, ids: &[i32]) -> Env {
            for &i in ids {
                let (o, m) = flag_slot(i).unwrap();
                let w = le32(&self.flags, o).unwrap();
                put32(&mut self.flags, o, w | m);
            }
            self
        }
    }

    impl StoryEnv for Env {
        fn flag(&self, id: i32) -> bool {
            check_flag(&self.flags, id)
        }
        fn set_flag(&mut self, id: i32) {
            self.sets.push(id);
            let (o, m) = flag_slot(id).unwrap();
            let w = le32(&self.flags, o).unwrap();
            put32(&mut self.flags, o, w | m);
        }
        fn owned_count(&self, item: i32) -> u32 {
            self.items.iter().find(|i| i.0 == item).map_or(0, |i| i.1)
        }
        fn remove_item(&mut self, item: i32, member: i32) {
            self.removed.push((item, member));
        }
        fn drive_gauge_2(&self) -> u8 {
            self.gauge
        }
    }

    #[test]
    fn flags_are_pages_of_words_and_ffff_is_always_clear() {
        let mut f = vec![0u8; 0x800];
        assert!(!check_flag(&f, 0x879));
        let (o, m) = flag_slot(0x879).unwrap();
        // Page 2 (0x879 >> 10), word (0x79 >> 5) = 3, bit 25.
        assert_eq!((o, m), (2 * 0x20 + 3 * 4, 1 << 25));
        put32(&mut f, o, m);
        assert!(check_flag(&f, 0x879));
        clear_flag(&mut f, 0x879);
        assert!(!check_flag(&f, 0x879));
        put32(&mut f, 0x7fc, u32::MAX);
        assert!(!check_flag(&f, 0xffff));
        assert!(check_flag(&f, 0xfffe));
        assert!(!check_flag(&f, -1) && !check_flag(&f, 0x10000));
    }

    #[test]
    fn a_gate_is_closed_only_with_817_set_and_every_other_flag_clear() {
        let f = |ids: &[i32]| Env::new().with(ids).flags;
        assert!(gate_81c(&f(&[])));
        assert!(!gate_81c(&f(&[0x817])));
        assert!(gate_81c(&f(&[0x817, 0x81c])));
        assert!(!gate_142b_2001(&f(&[0x817])));
        assert!(gate_142b_2001(&f(&[0x817, 0x2001])));
        assert!(gate_142b_2018(&f(&[0x817, 0x142b])));
    }

    #[test]
    fn set_when_needs_the_target_clear_and_the_condition_set() {
        let mut e = Env::new().with(&[0x20]);
        assert!(set_when(&mut e, 0x10, 0x20));
        assert!(!set_when(&mut e, 0x10, 0x20));
        assert!(!set_when(&mut e, 0x11, 0x21));
        assert_eq!(e.sets, [0x10]);
    }

    #[test]
    fn staged_flags_advance_one_stage_at_a_time() {
        let mut e = Env::new().with(&[0x2c31]);
        e.items = vec![(0x57, 1)];
        staged_flags(&mut e);
        assert_eq!(e.sets, [0x2c26]);
        // Stage 2 needs 0x2c34 and a gauge byte of 5.
        let mut e = Env::new().with(&[0x2c26, 0x2c34]);
        e.gauge = 4;
        staged_flags(&mut e);
        assert!(e.sets.is_empty());
        e.gauge = 5;
        staged_flags(&mut e);
        assert_eq!(e.sets, [0x2c27]);
    }

    #[test]
    fn the_chain_removes_the_item_for_five_flags_and_otherwise_tries_the_pairs() {
        let mut pairs = Vec::new();
        for k in 0..5u32 {
            pairs.extend_from_slice(&(0x100 + k).to_le_bytes());
            pairs.extend_from_slice(&(0x200 + k).to_le_bytes());
        }
        let mut e = Env::new();
        story_chain(&mut e, &pairs, 0x2411);
        assert_eq!(e.removed, [(0x20, 100)]);
        let mut e = Env::new().with(&[0x202, 0x204]);
        e.items = vec![(0x20, 1)];
        story_chain(&mut e, &pairs, 5);
        assert_eq!(e.sets, [0x102, 0x104]);
        let mut e = Env::new().with(&[0x202]);
        story_chain(&mut e, &pairs, 5);
        assert!(e.sets.is_empty());
    }

    #[test]
    fn the_eight_entry_table_stores_finds_and_frees() {
        let mut t = [0u8; 0x60];
        assert_eq!(recom_first_free(&t), Some(0));
        recom_switch_on(&mut t, 5, 7);
        recom_switch_off(&mut t, 6, 1);
        assert_eq!((recom_find(&t, 5, 7), recom_find(&t, 6, 1), recom_first_free(&t)), (Some(0), Some(1), Some(2)));
        assert!(recom_is_on(&t, 5, 7) && !recom_is_on(&t, 6, 1) && recom_is_off(&t, 6, 1));
        // Switching an existing pair reuses its entry.
        recom_switch_off(&mut t, 5, 7);
        assert_eq!(recom_find(&t, 5, 7), Some(0));
        assert!(recom_is_off(&t, 5, 7));
        recom_remove(&mut t, 5, 7);
        assert_eq!(recom_first_free(&t), Some(0));
        recom_clear(&mut t);
        assert_eq!(recom_find(&t, 6, 1), None);
    }

    #[test]
    fn save_flag_bits_and_the_record_reset() {
        let mut b = [0u8; 0x200];
        put32(&mut b, 4, 1 << 3);
        assert!(save_flag_check(&b, 35) && !save_flag_check(&b, 34));
        save_flag_reset(&mut b, 35);
        assert!(!save_flag_check(&b, 35));
        let mut r = [0x77u8; 0x18];
        reset_record(&mut r);
        assert_eq!(le32(&r, 0), Some(u32::MAX));
        assert_eq!((le32(&r, 4), le32(&r, 8), le32(&r, 12)), (Some(0), Some(0x7777_7777), Some(0)));
    }

    #[test]
    fn record_bits_follow_the_matching_entries() {
        let mut t = Vec::new();
        for (f, g, b) in [(9u32, 1u32, 2u32), (4, 5, 6), (9, 7, 8)] {
            for v in [f, g, b] {
                t.extend_from_slice(&v.to_le_bytes());
            }
        }
        let mut got = Vec::new();
        record_bits_for_flag(&t, 9, |g, b| got.push((g, b)));
        assert_eq!(got, [(1, 2), (7, 8)]);
    }
}
