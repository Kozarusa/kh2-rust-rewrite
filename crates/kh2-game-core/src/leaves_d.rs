//! Core leaves left after the L1-P11 packages (work package L1-P24): a 32-byte clear, a virtual call of slot 0, the object observer
//! list (a count at +0xc and up to 32 object pointers from +0x10), the soft-pause flag cluster, a word store, the linked record
//! init at the list head 0x1d5dd50, a save bit test and a byte compare callback. Blocks and globals are byte slices in the
//! original's layout (RULES C7); a global read or written by a leaf is passed as the bytes at its address. Nothing here is wired into
//! the app: the callers (the observer users, the menu and event code that pause polling, the 0x333ff8 search) live in other systems.
//! The oracle tests are in `tests/orig_leaves_d.rs`.

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn wr32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// orig 0x11bef0: stores the zero quadword twice, clearing the 32 bytes at the pointer.
pub fn zero_32(b: &mut [u8]) {
    b[..32].fill(0);
}

/// orig 0x11f170: calls slot 0 of the object's function table with the object as the only argument. `rd` reads a 32-bit word of the
/// game's memory; the result is the address of the function that is called (the callee is another unit, so the port does not run it).
pub fn slot0_target(obj: u32, mut rd: impl FnMut(u32) -> u32) -> u32 {
    let table = rd(obj);
    rd(table)
}

// ---- the object observer: count at +0xc, 32 object pointers from +0x10 -------------------------------------------------------------

/// Number of object pointer slots the clear below zeroes (and so the capacity of the list [inferred from the clear]).
pub const OBSERVER_SLOTS: usize = 32;

/// orig 0x135cd0: clears the count at +0xc and all 32 pointer slots from +0x10.
pub fn observer_clear(o: &mut [u8]) {
    wr32(o, 0xc, 0);
    o[0x10..0x10 + OBSERVER_SLOTS * 4].fill(0);
}

/// orig 0x135eb0: the entry count (the word at +0xc).
pub fn observer_count(o: &[u8]) -> u32 {
    rd32(o, 0xc)
}

/// orig 0x135eb8: the index of the first slot (of the first `count`, signed) that holds `obj`, or -1. A count above
/// [`OBSERVER_SLOTS`] reads past the list in the original; the port reads zero words there [inferred: the game never holds such a count].
pub fn observer_index_of(o: &[u8], obj: u32) -> i32 {
    let count = rd32(o, 0xc) as i32;
    for i in 0..count.max(0) as usize {
        let slot = o.get(0x10 + i * 4..0x14 + i * 4).map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap()));
        if slot == obj {
            return i as i32;
        }
    }
    -1
}

/// orig 0x135f30: entry `n` taken modulo the count (a negative `n` counts from the end), or 0 for an empty list. The original wraps by
/// repeated addition or subtraction, so a count below zero never ends there; the port defines only a positive count [inferred]. Slots
/// past the list read as zero as in [`observer_index_of`].
pub fn observer_entry_wrapped(o: &[u8], n: i32) -> u32 {
    let count = rd32(o, 0xc) as i32;
    if count == 0 {
        return 0;
    }
    let i = (n as i64).rem_euclid(count as i64) as usize;
    o.get(0x10 + i * 4..0x14 + i * 4).map_or(0, |s| u32::from_le_bytes(s.try_into().unwrap()))
}

// ---- the soft-pause flag cluster (the byte at 0x356c40) ----------------------------------------------------------------------------

/// Address of the soft-pause byte the three leaves below share.
pub const SOFT_PAUSE: u32 = 0x356c40;
/// The argument 0x14a988 passes to its callee 0x14a818.
pub const SOFT_PAUSE_QUEUE: u32 = 0x3564d0;

/// orig 0x14aa58: clears the soft-pause byte (`g` is the byte at 0x356c40).
pub fn soft_pause_clear(g: &mut [u8]) {
    g[0] = 0;
}

/// orig 0x14aa48: sets the soft-pause byte.
pub fn soft_pause_set(g: &mut [u8]) {
    g[0] = 1;
}

/// orig 0x14a988: whether the queue at 0x3564d0 is polled (calls 0x14a818 with that address): only while the soft-pause byte is zero.
pub fn soft_pause_polls_queue(g: &[u8]) -> bool {
    g[0] == 0
}

// ---- word store, record list, save bit, byte compare ----------------------------------------------------------------------------

/// orig 0x17d980: stores the word at the pointer.
pub fn store_word(b: &mut [u8], v: u32) {
    wr32(b, 0, v);
}

/// Head and tail words of the record list (0x1d5dd50 and 0x1d5dd54).
pub const LIST_HEAD: u32 = 0x01d5_dd50;
pub const LIST_TAIL: u32 = LIST_HEAD + 4;
/// Offset of the next-record link in a list record.
pub const RECORD_NEXT: usize = 0x50;

/// orig 0x1bdfc0: initialises the record at `rec_addr` (bytes `rec`) with the owner word, zeroes +4..+0x30 and the words at +0x30,
/// +0x38, +0x40 and +0x48, and appends it to the list: with an empty tail it becomes head and tail, otherwise the old tail's link
/// (+0x50, written through `write32(address, value)`) points at it and it becomes the tail. The new record's link (+0x50) ends zero.
pub fn record_init(rec: &mut [u8], rec_addr: u32, owner: u32, head: &mut u32, tail: &mut u32, mut write32: impl FnMut(u32, u32)) {
    wr32(rec, 0, owner);
    rec[4..0x30].fill(0);
    for o in [0x30, 0x38, 0x40, 0x48] {
        wr32(rec, o, 0);
    }
    if *tail != 0 {
        if *tail != rec_addr {
            write32(*tail + RECORD_NEXT as u32, rec_addr);
        }
        *tail = rec_addr;
    } else {
        *head = rec_addr;
        *tail = rec_addr;
    }
    wr32(rec, RECORD_NEXT, 0);
}

/// orig 0x1be0b8: calls 0x3331e0 on the list head block (0x1d5dd50) with its own argument passed on as the second argument; returns
/// the two arguments of that call (the callee is another unit).
pub const fn list_call_args(arg: u64) -> (u32, u64) {
    (LIST_HEAD, arg)
}

/// orig 0x2f7ed8: tests bit `n` of the bit array at save block +0xb4d8 (`bits` starts there): word `n >> 5`, bit `n & 31`.
pub fn save_bit(bits: &[u8], n: u32) -> bool {
    rd32(bits, (n >> 5) as usize * 4) & (1 << (n & 31)) != 0
}

/// orig 0x334100: the compare callback of the 0x333ff8 search: the key minus the byte the second pointer addresses.
pub fn byte_compare(key: i32, byte: u8) -> i32 {
    key.wrapping_sub(i32::from(byte))
}
