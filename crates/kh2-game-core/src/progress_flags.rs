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

/// Where the original keeps the flag pages (0x20 bytes per world, so 256 flags each; the word index of an id runs to 31, so ids above
/// 255 of a page read into the next pages, and the largest id 0xffff addresses up to `FLAGS_LEN`).
pub const FLAGS_ADDR: u32 = 0x33f6b0;
/// Bytes of memory every id 0..=0xffff can address from `FLAGS_ADDR`.
pub const FLAGS_LEN: usize = 0x860;
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
    for e in table.as_chunks::<12>().0.iter().take(14) {
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

// --- the world map's flags and queue (core.save, WP-L1-P17) ----------------------------------------------------------------

/// Offsets inside the world map's region, the bytes from `0x341858` (`save_block::at::WORLD_RECORDS` in the save block): four bytes per
/// world, then the world flag bits, the queue of flags just set and its count byte.
pub mod world_at {
    /// Eight words of world flag bits (`0x3418a4`), read by `FUN_001b6a18`, set by `FUN_001b68f8`.
    pub const FLAGS: usize = 0x4c;
    /// The queue of world flags just set (`0x3418c4`), 255 bytes; the count byte follows it.
    pub const QUEUE: usize = 0x6c;
    /// The queue's count byte (`0x3419c3`).
    pub const QUEUE_COUNT: usize = 0x16b;
}

/// `FUN_001c0950(id)`: the address of route state byte `id`, `0x3419c3 + id` (the bytes the `wldf` list ops 8..=0xb write [inferred]).
///
/// orig 0x1c0950
pub fn course_address(id: i32) -> u32 {
    (id as u32).wrapping_add(0x3419c3)
}

/// What the world flag routines reach outside the world region: the `wldf` record lookup (`FUN_001b68c0`), story flag `0x102b`
/// (`FUN_00180330`) and the action list runner (`FUN_001b6a48`).
pub trait WorldEnv {
    /// Byte `offset` of the `wldf` record with index `index` (`FUN_001b68c0` finds the record by binary search); 0 for a missing record
    /// [inferred: the game reads address `offset`].
    fn record_byte(&self, index: u32, offset: usize) -> u8;
    /// `FUN_00180330`: story flag `0x102b` is set.
    fn second_part(&self) -> bool;
    /// `FUN_001b6a48(record, hook)` for the record of `key`: runs its action list on the world region.
    fn run_list(&mut self, world: &mut [u8], key: u8, hook: bool);
}

/// orig 0x1b6a18: world flag `i` is set (bit `i & 31` of word `i >> 5` of the flag words). A word past the slice reads as clear [inferred].
pub fn world_flag_check(world: &[u8], i: u32) -> bool {
    le32(world, world_at::FLAGS + (i >> 5) as usize * 4).is_some_and(|w| w & (1u32 << (i & 31)) != 0)
}

/// orig 0x1b68f8: sets world flag `i` and queues it. Four indexes (0x25, 0x2a, 0x2d, 0x2e) are skipped while their rival flag (0x73,
/// 0x56, 0x54, 0x72) is set; a flag already set changes nothing; a record of kind 1 (byte +1 of the `wldf` record) is not queued while
/// story flag 0x102b is set. The queue count goes up first, then the byte is written at the old count, so a count of 255 writes the
/// count byte itself.
pub fn world_flag_set(world: &mut [u8], env: &impl WorldEnv, i: u32) {
    let rival = match i {
        0x25 => Some(0x73),
        0x2a => Some(0x56),
        0x2d => Some(0x54),
        0x2e => Some(0x72),
        _ => None,
    };
    if rival.is_some_and(|r| world_flag_check(world, r)) {
        return;
    }
    let o = world_at::FLAGS + (i >> 5) as usize * 4;
    let Some(w) = le32(world, o) else { return };
    let bit = 1u32 << (i & 31);
    if w & bit != 0 {
        return;
    }
    put32(world, o, w | bit);
    if env.record_byte(i, 1) != 1 || !env.second_part() {
        let n = world[world_at::QUEUE_COUNT];
        world[world_at::QUEUE_COUNT] = n.wrapping_add(1);
        world[world_at::QUEUE + n as usize] = i as u8;
    }
}

/// orig 0x1b6e20: copies the queue out (oldest first) and zeroes its count byte.
pub fn world_take_queue(world: &mut [u8]) -> Vec<u8> {
    let n = world[world_at::QUEUE_COUNT] as usize;
    world[world_at::QUEUE_COUNT] = 0;
    world[world_at::QUEUE..world_at::QUEUE + n].to_vec()
}

/// orig 0x1b6e60: takes the queue (`FUN_001b6e20`) and runs the action list of each queued flag in order with the hook argument 0
/// (`hook` false; the consuming half of `FUN_002c6bf8` passes 1). Returns the flags taken.
pub fn world_drain(world: &mut [u8], env: &mut impl WorldEnv, hook: bool) -> Vec<u8> {
    let queue = world_take_queue(world);
    for &key in &queue {
        env.run_list(world, key, hook);
    }
    queue
}

/// orig 0x1b6870: the boot of the world flags: the eight flag words and the queue count are cleared, world flag 0 is set
/// (`FUN_001b68f8(0)`) and the queue drained with hook argument 0 (`FUN_001b6e60`). Returns the flags the drain ran.
pub fn world_boot(world: &mut [u8], env: &mut impl WorldEnv) -> Vec<u8> {
    world[world_at::FLAGS..world_at::FLAGS + 32].fill(0);
    world[world_at::QUEUE_COUNT] = 0;
    world_flag_set(world, env, 0);
    world_drain(world, env, false)
}

/// orig 0x2c6d60: the order of two queued world flags in the menu's sort: byte +2 of the `wldf` record of the first minus that of the second.
pub fn world_record_order(env: &impl WorldEnv, a: u8, b: u8) -> i32 {
    i32::from(env.record_byte(u32::from(a), 2)) - i32::from(env.record_byte(u32::from(b), 2))
}

// --- the full story flag setter ------------------------------------------------------------------------------------------

/// Offsets in the save block that the setter's side effects write (the same numbers as `save_block::at`, with the per-world row sizes).
mod save_at {
    /// `0x33e86f`: the first (map, btl, evt) triple of the area program table, 0xc0 bytes per world.
    pub const PROGRAMS: usize = 0xf;
    pub const PROGRAM_WORLD: usize = 0xc0;
    /// `0x33f6b0`: the flag pages.
    pub const FLAGS: usize = 0xe50;
    /// `0x33f910`: the world-point bits (two words).
    pub const WORLD_POINTS: usize = 0x10b0;
    /// `0x33f918`: the exit-lock halfword pairs (state, message).
    pub const LOCKS: usize = 0x10b8;
    /// `0x33fdb0`: the record bits; `FUN_001a7730` writes the byte `+ world + 0x5c`.
    pub const RECORD_BITS: usize = 0x1550;
    /// `0x341858`: the world map's records; `FUN_001b4e80(world)` is `+ 4 * world`.
    pub const WORLD_RECORDS: usize = 0x2ff8;
    /// The 19 pairs of words `FUN_0019d890` clears.
    pub const PAIRS: usize = 0x14b8;
    pub const PAIRS_WORDS: usize = 38;
}

/// What the setter `FUN_0017ff60` reads from the executable's loaded tables: the per-world side-effect lists of `00progress.bin`
/// (`0x35ccc0` holds a pointer to each), the table of two bytes per exit lock that names its world point (`DAT_01c8a988`, entry `i` at
/// `+ 2 + 2 * i`), and the world point table (`DAT_01c8a984`, 0x36 entries of four bytes whose first two bytes are compared).
#[derive(Clone, Copy, Debug, Default)]
pub struct SetterTables<'a> {
    pub entries: &'a [&'a [u8]],
    pub locks: &'a [u8],
    pub points: &'a [u8],
}

/// The calls the setter makes that act outside the save block, in the original's order after its list: `FUN_001803b0` (flag),
/// `FUN_00180450`, `FUN_001805d0` (flag), `FUN_00180558`, `FUN_001d39d0`, `FUN_001d3238` (flag), `FUN_001b6da0` (flag), `FUN_001be9c8`
/// (flag), `FUN_001d5718` and, last, `FUN_00195130` (flag); `FUN_001d39d0` also runs once after each record bit set by list op 8. `addr`
/// is the callee's address in the original, `arg` the flag id (0 for the calls without one).
pub trait SetterTail {
    fn call(&mut self, save: &mut [u8], addr: u32, arg: i32);
}

fn le16(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map_or(0, |s| u16::from_le_bytes([s[0], s[1]]))
}

fn set_byte(b: &mut [u8], o: usize, v: u8) {
    if let Some(x) = b.get_mut(o) {
        *x = v;
    }
}

/// orig 0x104b90: copies three bytes to the program triple, each unless it is 0xff.
fn merge_program(dst: &mut [u8], o: usize, src: [u8; 3]) {
    for (k, &v) in src.iter().enumerate() {
        if v != 0xff {
            set_byte(dst, o + k, v);
        }
    }
}

/// orig 0x195260: the exit lock entry `idx` gets its state and message halfwords.
fn lock_set(save: &mut [u8], idx: usize, state: u16, message: u16) {
    let o = save_at::LOCKS + 4 * idx;
    if let Some(s) = save.get_mut(o..o + 4) {
        s[..2].copy_from_slice(&state.to_le_bytes());
        s[2..].copy_from_slice(&message.to_le_bytes());
    }
}

/// orig 0x195288: the exit lock entry's state is 0.
fn lock_open(save: &mut [u8], idx: usize) {
    let o = save_at::LOCKS + 4 * idx;
    if let Some(s) = save.get_mut(o..o + 2) {
        s.fill(0);
    }
}

/// `value - 1` goes to the byte `world + 0x5c` of the record bit words (`0x33fe0c + world`).
///
/// orig 0x1a7730
fn store_world_byte(save: &mut [u8], world: usize, value: u8) {
    set_byte(save, save_at::RECORD_BITS + world + 0x5c, value.wrapping_sub(1));
}

/// The world point named by lock `idx` loses its bit, as `FUN_001952a0` / `FUN_00195300` do it: the lookup `FUN_00194f68` in the point
/// table by the lock's two bytes, then the bit clear `FUN_00194fd8`.
///
/// orig 0x194f68
/// orig 0x194fd8
fn lock_clear_point(save: &mut [u8], t: &SetterTables, idx: usize) {
    let (a, b) = (t.locks.get(2 + 2 * idx).copied().unwrap_or(0), t.locks.get(3 + 2 * idx).copied().unwrap_or(0));
    let hit = t.points.chunks(4).take(0x36).position(|e| e.len() >= 2 && e[0] == a && e[1] == b);
    if let Some(i) = hit {
        let o = save_at::WORLD_POINTS + 4 * (i >> 5);
        if let Some(w) = le32(save, o) {
            put32(save, o, w & !(1u32 << (i & 31)));
        }
    }
}

/// Record bit `n` (word `n >> 5` of the words at `0x33fdb0`) set (`FUN_001a52b0`, which then calls `FUN_001d39d0`) or cleared.
///
/// orig 0x1a52b0
/// orig 0x1a52f0
fn record_bit(save: &mut [u8], n: u16, on: bool) {
    let o = save_at::RECORD_BITS + 4 * (n >> 5) as usize;
    if let Some(w) = le32(save, o) {
        let m = 1u32 << (n & 31);
        put32(save, o, if on { w | m } else { w & !m });
    }
}

/// orig 0x17ff60: the story flag setter on the save block `save` (the bytes from `0x33e860`). Sets the flag's bit, then runs the
/// side-effect list the flag's world has for it (the list starts at the entry's halfword `id & 0x3ff`; ops, each `op, count` and `count`
/// halfwords, end at op 0):
/// 1 writes `count / 2` area program triples (area byte then map, btl, evt; 0xff keeps a byte, `FUN_00104b90`), 2 sets an exit lock's
/// state and message (`FUN_00195260`), 3 opens a lock (`FUN_00195288`), 4 locks it with state 0xffff and clears its world point
/// (`FUN_001952a0`), 5 opens it and clears the point (`FUN_00195300`), 6 stores `value - 1` in the record byte of the world
/// (`FUN_001a7730`), 7 clears each flag (`FUN_00180298`), 8 sets each record bit and calls `FUN_001d39d0` (`FUN_001a52b0`), 9 clears each
/// record bit (`FUN_001a52f0`), 0xc sets each flag that is clear (recursively), 0xd stores the halfword's high and low byte in bytes 1
/// and 2 of the world's record (`FUN_001b4e80`); any other op does nothing. After the list the calls of [`SetterTail`] run, and for
/// flag 0x8d0 `FUN_0019d890` clears the pair table before the last of them. A world without an entry has an empty list [inferred: the game
/// reads the pointer table past its 19 entries].
pub fn set_flag_full(save: &mut [u8], t: &SetterTables, tail: &mut impl SetterTail, id: i32) {
    let page = (id >> 10) as isize;
    // The flag word: page * 0x20 + word * 4 from 0x33f6b0. A negative page or a word past the block is outside the layout; nothing is set.
    let word = (id as u32 & 0x3ff) >> 5;
    if let Some(o) = usize::try_from(page * 0x20 + word as isize * 4).ok().map(|d| save_at::FLAGS + d)
        && let Some(w) = le32(save, o)
    {
        put32(save, o, w | 1u32 << (id as u32 & 0x1f));
    }
    let entry = usize::try_from(page).ok().and_then(|p| t.entries.get(p)).copied().unwrap_or(&[]);
    let world = page.max(0) as usize;
    let mut p = le16(entry, (id as usize & 0x3ff) * 2) as usize;
    loop {
        let op = entry.get(p).copied().unwrap_or(0);
        if op == 0 {
            break;
        }
        let n = entry.get(p + 1).copied().unwrap_or(0) as usize;
        let h = |k: usize| le16(entry, p + 2 + 2 * k);
        match op {
            1 => {
                for k in 0..n >> 1 {
                    let r = p + 2 + 4 * k;
                    let b = |j: usize| entry.get(r + j).copied().unwrap_or(0);
                    let o = save_at::PROGRAMS + world * save_at::PROGRAM_WORLD + b(0) as usize * 3;
                    merge_program(save, o, [b(1), b(2), b(3)]);
                }
            }
            2 => lock_set(save, h(0) as usize, h(1), h(2)),
            3 => lock_open(save, h(0) as usize),
            4 => {
                let i = h(0) as usize;
                if let Some(s) = save.get_mut(save_at::LOCKS + 4 * i..save_at::LOCKS + 4 * i + 2) {
                    s.copy_from_slice(&0xffffu16.to_le_bytes());
                }
                lock_clear_point(save, t, i);
            }
            5 => {
                lock_open(save, h(0) as usize);
                lock_clear_point(save, t, h(0) as usize);
            }
            6 => store_world_byte(save, world, h(0) as u8),
            7 => {
                for k in 0..n {
                    clear_flag_in(save, h(k) as i32);
                }
            }
            8 => {
                for k in 0..n {
                    record_bit(save, h(k), true);
                    tail.call(save, 0x1d39d0, 0);
                }
            }
            9 => {
                for k in 0..n {
                    record_bit(save, h(k), false);
                }
            }
            0xc => {
                for k in 0..n {
                    let f = h(k) as i32;
                    if !check_flag(save.get(save_at::FLAGS..).unwrap_or(&[]), f) {
                        set_flag_full(save, t, tail, f);
                    }
                }
            }
            0xd => {
                let o = save_at::WORLD_RECORDS + 4 * world;
                set_byte(save, o + 1, (h(0) >> 8) as u8);
                set_byte(save, o + 2, h(0) as u8);
            }
            _ => {}
        }
        p += n * 2 + 2;
    }
    for (addr, arg) in [(0x1803b0, id), (0x180450, 0), (0x1805d0, id), (0x180558, 0), (0x1d39d0, 0), (0x1d3238, id), (0x1b6da0, id), (0x1be9c8, id), (0x1d5718, 0)] {
        tail.call(save, addr, arg);
    }
    if id == 0x8d0 {
        clear_pair_words(save);
    }
    tail.call(save, 0x195130, id);
}

fn clear_flag_in(save: &mut [u8], id: i32) {
    if let Some(f) = save.get_mut(save_at::FLAGS..) {
        clear_flag(f, id);
    }
}

/// The 38 words (19 pairs) from save `+0x14b8` are cleared (`FUN_0019d890`, ported as `save_block::clear_pair_table`).
fn clear_pair_words(save: &mut [u8]) {
    if let Some(s) = save.get_mut(save_at::PAIRS..save_at::PAIRS + 4 * save_at::PAIRS_WORDS) {
        s.fill(0);
    }
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
            Env { flags: vec![0; 0x860], ..Env::default() }
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
        let mut f = vec![0u8; 0x860];
        assert!(!check_flag(&f, 0x879));
        let (o, m) = flag_slot(0x879).unwrap();
        // Page 2 (0x879 >> 10), word (0x79 >> 5) = 3, bit 25.
        assert_eq!((o, m), (2 * 0x20 + 3 * 4, 1 << 25));
        put32(&mut f, o, m);
        assert!(check_flag(&f, 0x879));
        clear_flag(&mut f, 0x879);
        assert!(!check_flag(&f, 0x879));
        put32(&mut f, 0x85c, u32::MAX);
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
