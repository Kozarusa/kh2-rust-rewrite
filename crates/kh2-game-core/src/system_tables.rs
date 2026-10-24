//! System tables and the area-loading remainder (core runtime, work package L1-P19): the table setters the 03system.bin and battle.bin
//! loaders call, the 03system loader (`0x193610`), the 00objentry loader (`0x16e1e0`), the area BAR lookups, the area cost budget,
//! the spawn record stack and the group activators, the object data cache (`0x1a7808`, `0x1a78c8`), the object list destroy walk, the
//! spawn finish (`0x199b18`) and the queue allocation of the area unit.
//!
//! Layout rule (RULES C7): a global the original keeps in the executable's data is handed in as the bytes at its address (`g`), so an
//! oracle test lays the same bytes into kh2-ee and compares them back. Work that reaches other units goes through
//! [`AreaHost`](crate::area::ctl::AreaHost) as in the area control unit: `call(callee, args)` stands for a function that is another
//! unit's, and the oracle logs the call (address and argument words) on both sides. Memory the functions read through a pointer (a BAR in
//! memory, an object block) is read through the host too.
//!
//! The setters store the payload address of a BAR entry in a global; nothing reads these globals yet in the port, so nothing here is
//! wired into the app (the readers are other units: the cost consumers, the item and shop tables, the battle tables).

use crate::area::ctl::{AreaCtl, AreaHost, GroupTable, GROUP_BASE, GROUP_SIZE};

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn wr32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

// ---- the table setters: one global word each ------------------------------------------------------------------------------------

/// orig 0x1845a0: stores the table address in the word at 0x1c89700.
pub fn set_table_1845a0(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x184630: stores the table address in the word at 0x1c89704.
pub fn set_table_184630(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x193370: stores the table address in the word at 0x1c8a970 (the area info table of the area unit).
pub fn set_table_193370(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1a6800: stores the table address in the word at 0x1c95590.
pub fn set_table_1a6800(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1b03a8: stores the table address in the word at 0x1d5d640 and clears the word behind it (0x1d5d644). `g` is the 8 bytes.
pub fn set_table_1b03a8(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
    wr32(g, 4, 0);
}

/// orig 0x1b50b0: stores the table address in the word at 0x1d5d6b4.
pub fn set_table_1b50b0(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1bea98: stores the table address in the word at 0x1d5dd80.
pub fn set_table_1bea98(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1bfef0: stores the table address in the word at 0x1d5df10.
pub fn set_table_1bfef0(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1c0b30: stores the table address in the word at 0x1d5df58 (the keyed table [`keyed_row`] searches).
pub fn set_table_1c0b30(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1c0b70: stores the table address in the word at 0x1d5df5c.
pub fn set_table_1c0b70(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1c1400: stores the table address in the word at 0x1d5df60.
pub fn set_table_1c1400(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1c6cd0: stores the table address in the word at 0x1d5e378.
pub fn set_table_1c6cd0(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1d0110: stores the table address in the word at 0x1d6a920.
pub fn set_table_1d0110(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1d0248: stores the table address in the word at 0x1d6a924.
pub fn set_table_1d0248(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1d3900: stores the table address in the word at 0x1d6a98c.
pub fn set_table_1d3900(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// orig 0x1d5e38: stores the table address in the word at 0x3608f8.
pub fn set_table_1d5e38(g: &mut [u8], v: u32) {
    wr32(g, 0, v);
}

/// Stores the table address in the word at 0x1c95588 and hands the address of the table's end to [`set_table_1d0110`]: the table is a count word at +4 followed by entries of 0x18 bytes from +8, so the end is `table + count * 0x18 +
/// 8` (wrapping). `count` is the word at `table + 4`. Returns the pair of words written: (0x1c95588, 0x1d6a920).
///
/// orig 0x1a66b8
pub fn set_table_1a66b8(table: u32, count: u32) -> (u32, u32) {
    (table, table.wrapping_add(count.wrapping_mul(0x18)).wrapping_add(8))
}

/// Stores the table address in the word at 0x360768 and the payload word (+8) of the first type-2 entry of that BAR in the word at
/// 0x1d6a988. A BAR without a type-2 entry makes the original read the word at address 8; so does this.
///
/// orig 0x1d3050
pub fn set_table_1d3050(host: &mut impl AreaHost, table: u32) {
    host.write32(0x360768, table);
    let e = bar_nth_of_type(host, table, 2, 0);
    let p = host.read32(e.wrapping_add(8));
    host.write32(0x1d6a988, p);
}

// ---- keyed row search (0x1c0b40) -------------------------------------------------------------------------------------------------

/// The binary search the game runs for `0x30ff40` (halving window, 8-byte rows, signed difference of the key and the row's first word).
/// `row_word(i)` is the first word of row `i`; the result is the index of the row whose word equals the key, or `None`.
pub fn bsearch_rows(count: u32, key: u32, mut row_word: impl FnMut(u32) -> u32) -> Option<u32> {
    let mut lo = 0u32;
    let mut hi = count;
    let mut n = count;
    if count == 0 {
        return None;
    }
    loop {
        let mid = n >> 1;
        let diff = key.wrapping_sub(row_word(mid)) as i32;
        if diff >= 0 {
            lo = mid.wrapping_add(1);
            if diff == 0 {
                return Some(mid);
            }
        } else {
            hi = mid;
        }
        if lo >= hi {
            return None;
        }
        n = lo.wrapping_add(hi);
    }
}

/// The row of the keyed table at the word 0x1d5df58 whose first word equals `key`: the table has its row count at +4 and 8-byte rows from
/// +8 (sorted by the first word). Returns the row's address, 0 when there is none.
///
/// orig 0x1c0b40
pub fn keyed_row(host: &mut impl AreaHost, key: u32) -> u32 {
    let table = host.read32(0x1d5df58);
    let count = host.read32(table.wrapping_add(4));
    let base = table.wrapping_add(8);
    match bsearch_rows(count, key, |i| host.read32(base.wrapping_add(i.wrapping_mul(8)))) {
        Some(i) => base.wrapping_add(i.wrapping_mul(8)),
        None => 0,
    }
}

// ---- the bit tests (0x19de20, 0x19de48) ------------------------------------------------------------------------------------------

/// Whether bit `bit` of the bit array at `base` is set: word `bit >> 5`, bit `bit & 31`. `word(i)` reads the word at `base + i * 4`
/// (the array is another unit's block).
///
/// orig 0x19de20
pub fn bit_set(word: impl Fn(u32) -> u32, bit: u32) -> bool {
    word(bit >> 5) & (1u32 << (bit & 31)) != 0
}

/// The same test on an array whose words start 4 bytes into the block (the first word is a header).
///
/// orig 0x19de48
pub fn bit_set_after_header(word: impl Fn(u32) -> u32, bit: u32) -> bool {
    word((bit >> 5).wrapping_add(1)) & (1u32 << (bit & 31)) != 0
}

// ---- the area cost budget: the budget word at 0x1c893b8 and the used word at 0x1c893bc ----------------------------------------------

/// The EE's FPU has no infinities or NaNs: a result that overflows (or is not a number) saturates to the largest finite value. This is the
/// model the oracle interpreter (kh2-ee) uses [inferred for the hardware: denormals are not flushed here either].
fn ee_out(x: f32) -> f32 {
    if x.is_nan() {
        f32::MAX
    } else if x.is_infinite() {
        f32::MAX.copysign(x)
    } else {
        x
    }
}

fn ee_div(a: f32, b: f32) -> f32 {
    ee_out(if b == 0.0 { f32::MAX.copysign(a) * b.signum() } else { a / b })
}

fn f(g: &[u8], o: usize) -> f32 {
    f32::from_bits(rd32(g, o))
}

/// orig 0x181440: adds `cost` to the used word (`g` is the 8 bytes from 0x1c893b8: budget, used).
pub fn cost_add(g: &mut [u8], cost: f32) {
    let v = ee_out(f(g, 4) + cost);
    wr32(g, 4, v.to_bits());
}

/// orig 0x181458: the budget left, `budget - used`.
pub fn cost_left(g: &[u8]) -> f32 {
    ee_out(f(g, 0) - f(g, 4))
}

/// orig 0x181470: the share of the budget used, `used / budget`.
pub fn cost_ratio(g: &[u8]) -> f32 {
    ee_div(f(g, 4), f(g, 0))
}

/// Whether `cost` still fits: it is not above the budget left.
///
/// orig 0x181408
pub fn cost_can_add(g: &[u8], cost: f32) -> bool {
    cost <= cost_left(g)
}

// ---- the area record at 0x1c893b0 (six bytes) ---------------------------------------------------------------------------------------

/// Forgets the current area: when called with `(1, 0xffff)` all six bytes of the record at 0x1c893b0 become 0xff, any other pair does
/// nothing (`b` is the full 64-bit register).
///
/// orig 0x181498
pub fn area_record_reset_if(g: &mut [u8], a: i32, b: i64) {
    if b == 0xffff && a == 1 {
        g[..6].fill(0xff);
    }
}

/// The reset point wrapper: [`area_record_reset_if`] with `(1, 0xffff)`.
///
/// orig 0x1814d8
pub fn area_record_reset(g: &mut [u8]) {
    area_record_reset_if(g, 1, 0xffff);
}

// ---- the spawn record stack: 12-byte records from 0x1c89400, the count word at 0x35cd48 ---------------------------------------------

/// Address of the first spawn record.
pub const SPAWN_RECORDS: u32 = 0x1c89400;
/// Size of one spawn record.
pub const SPAWN_RECORD: u32 = 12;

/// Takes the next spawn record: returns its address and advances the count (`count` is the word at 0x35cd48).
///
/// orig 0x1814f8
pub fn spawn_record_push(count: &mut [u8]) -> u32 {
    let n = rd32(count, 0);
    wr32(count, 0, n.wrapping_add(1));
    SPAWN_RECORDS.wrapping_add(n.wrapping_mul(SPAWN_RECORD))
}

/// Gives the last spawn record back: the count word drops by one.
///
/// orig 0x181520
pub fn spawn_record_pop(count: &mut [u8]) {
    let n = rd32(count, 0);
    wr32(count, 0, n.wrapping_sub(1));
}

/// Fills a spawn record for a group loaded from a spawn file: word 0 the tag, word 2 the group record, word 1 the flags (2 when `alt`,
/// else 0; bit 0 stays clear, which marks the record live).
///
/// orig 0x181560
pub fn spawn_record_for_group(rec: &mut [u8], tag: u32, group: u32, alt: bool) {
    wr32(rec, 0, tag);
    wr32(rec, 8, group);
    wr32(rec, 4, 0);
    if alt {
        wr32(rec, 4, 2);
    }
}

/// Fills a spawn record that has no group yet: word 0 the tag, word 1 the flags (1, or 3 when `alt`), word 2 the group record.
///
/// orig 0x1815b0
pub fn spawn_record_for_event(rec: &mut [u8], tag: u32, group: u32, alt: bool) {
    wr32(rec, 0, tag);
    wr32(rec, 4, 1);
    wr32(rec, 8, group);
    if alt {
        wr32(rec, 4, 3);
    }
}

/// Releases the group a live spawn record (flags bit 0 clear) holds: the group's objects are dismissed (method 1 of each), then its
/// record is given back to the group table. The record's group word is an address in the group table.
///
/// orig 0x181630
pub fn spawn_record_release(rec: &[u8], groups: &mut GroupTable, ctl: &mut AreaCtl, host: &mut impl AreaHost) {
    let group = rd32(rec, 8);
    if rd32(rec, 4) & 1 == 0 && group != 0 {
        let idx = (group.wrapping_sub(GROUP_BASE) / GROUP_SIZE as u32) as usize;
        groups.recs[idx].dismiss_all(host);
        GroupTable::give_back(ctl);
    }
}

/// Calls the original's per-group pass `0x1986c8` for a live spawn record (flags bit 0 clear) with its group record.
///
/// orig 0x181688
pub fn spawn_record_pass(rec: &[u8], host: &mut impl AreaHost) {
    if rd32(rec, 4) & 1 == 0 {
        host.call(0x1986c8, &[rd32(rec, 8) as u64]);
    }
}

// ---- small leaves ------------------------------------------------------------------------------------------------------------------

/// orig 0x13ee98: the byte at 0x355830.
pub fn byte_355830(g: &[u8]) -> u8 {
    g[0]
}

/// orig 0x1af928: points the word at 0x35ee98 at the block at 0x1cb8300.
pub fn point_35ee98_at_block(g: &mut [u8]) {
    wr32(g, 0, 0x1cb8300);
}

/// orig 0x1af940: clears the word at 0x35ee98.
pub fn clear_35ee98(g: &mut [u8]) {
    wr32(g, 0, 0);
}

// ---- BAR lookups on the host's memory ---------------------------------------------------------------------------------------------

/// Size of a BAR entry; the first entry follows the 0x10-byte header (the layout of `kh2_bar::resident`).
const ENTRY: u32 = 0x10;

/// The address of the `n`-th (from 0) entry of the BAR at `hdr` whose type is `ty` (-1 for any), or 0. The walk of
/// `kh2_bar::resident::nth_of_type`, reading the BAR through the host. The count word is signed; a negative count has no entries.
pub fn bar_nth_of_type(host: &mut impl AreaHost, hdr: u32, ty: i64, mut n: i32) -> u32 {
    let count = host.read32(hdr.wrapping_add(4)) as i32;
    let mut e = hdr.wrapping_add(ENTRY);
    for _ in 0..count.max(0) {
        if ty == -1 || host.read16(e) as i64 == ty {
            n = n.wrapping_sub(1);
            if n == -1 {
                return e;
            }
        }
        e = e.wrapping_add(ENTRY);
    }
    0
}

/// The address of the `n`-th entry whose type is `ty` (-1 for any) and whose name word equals `name`, compared as the full register the
/// original compares (the word read sign-extended), or 0. The walk of `kh2_bar::resident::nth_of_name`.
pub fn bar_nth_of_name(host: &mut impl AreaHost, hdr: u32, ty: i64, name: u64, mut n: i32) -> u32 {
    let count = host.read32(hdr.wrapping_add(4)) as i32;
    let mut e = hdr.wrapping_add(ENTRY);
    for _ in 0..count.max(0) {
        if (ty == -1 || host.read16(e) as i64 == ty) && crate::area::ctl::sx(host.read32(e.wrapping_add(4))) == name {
            n = n.wrapping_sub(1);
            if n == -1 {
                return e;
            }
        }
        e = e.wrapping_add(ENTRY);
    }
    0
}

/// The name an entry is looked up by when the original builds it from four bytes in its data: each byte is loaded signed, shifted into
/// place and ORed in, so a byte of 0x80 or above floods the bits above it with ones. `b` are the four bytes in memory order.
pub fn name_from_bytes(b: [u8; 4]) -> u64 {
    let s = |i: usize| b[i] as i8 as i64;
    let sll = |v: i64, n: u32| (v as i32).wrapping_shl(n) as i64;
    (s(0) | sll(s(3), 24) | (sll(s(2), 16) | sll(s(1), 8))) as u64
}

fn host_name(host: &mut impl AreaHost, at: u32) -> u64 {
    name_from_bytes([host.read8(at), host.read8(at + 1), host.read8(at + 2), host.read8(at + 3)])
}

// ---- the area BAR (cache word 0x35cd44, place record 0x1c893b0) --------------------------------------------------------------------

/// Loads the area's spawn file when the place `place` (a pointer to six bytes) is another world or area than the one loaded (the place
/// record at 0x1c893b0): the cache buffer of the old one is freed (0x16ff18), the file name is built (0x105010 gives the stem, 0x311c08
/// formats it with the format at 0x37a7a0 into `scratch`, a 48-byte buffer), the file's size is asked (0x168250), a buffer of that size
/// rounded up to 0x800 is taken (0x16fe70) and the BAR read request is made (0x167e58); the first six bytes of `place` become the loaded
/// place. The callees are other units' (the file unit's request builders are in `kh2_bar::request`), so they go through the host.
///
/// orig 0x180870
pub fn area_bar_load(host: &mut impl AreaHost, place: u32, scratch: u32) {
    let cur = 0x1c893b0u32;
    if host.read8(cur) == host.read8(place) && host.read8(cur + 1) == host.read8(place + 1) {
        return;
    }
    let cache = host.read32(0x35cd44);
    if cache != 0 {
        host.call(0x16ff18, &[cache as u64]);
    }
    let name = host.call(0x105010, &[place as u64, 0]);
    host.call(0x311c08, &[scratch as u64, 0x37a7a0, name]);
    let size = host.call(0x168250, &[scratch as u64]) as u32;
    let buf = host.call(0x16fe70, &[crate::area::ctl::sx(size.wrapping_add(0x7ff) & 0xffff_f800)]);
    host.write32(0x35cd44, buf as u32);
    host.call(0x167e58, &[scratch as u64, buf, 0, 0]);
    let w = host.read32(place);
    host.write32(cur, w);
    let b4 = host.read8(place + 4);
    host.write8(cur + 4, b4);
    let b5 = host.read8(place + 5);
    host.write8(cur + 5, b5);
}

/// The payload word (+8) of the first entry of type 0xc and name `name` in the area BAR (the buffer at 0x35cd44), 0 when there is none.
///
/// orig 0x180950
pub fn area_bar_lookup(host: &mut impl AreaHost, name: u64) -> u32 {
    let bar = host.read32(0x35cd44);
    let e = bar_nth_of_name(host, bar, 0xc, name, 0);
    if e == 0 { 0 } else { host.read32(e.wrapping_add(8)) }
}

// ---- spawn finish (0x199b18) -------------------------------------------------------------------------------------------------------

/// The pass that gives every object `0x184d98` finds (the next object of the list with class bit 2 set) that stands inside one of the
/// group record `rec`'s objects the group's id: for each such object `obj` the 16-byte position `0x16b960` points at is copied to
/// `scratch`, each object of the group (the list at record +0xc, linked at +0x60) is asked by its method 2 whether the point is inside
/// it, and the first yes ends the search (an object no group object accepts is left alone). If the object's word at +0xaa8 (with bit 16
/// masked) is not already the group header's id (the halfword at +2), `0x197b70` is told (the header's word +0x1c and the object's serial
/// from `0x16b928`). Either way the word becomes the id.
///
/// orig 0x199b18
pub fn spawn_finish(host: &mut impl AreaHost, rec: u32, scratch: u32) {
    let mut obj: u64 = 0;
    'objects: loop {
        obj = host.call(0x184d98, &[obj]);
        if obj == 0 {
            return;
        }
        let o = obj as u32;
        let pos = host.call(0x16b960, &[obj]) as u32;
        if pos != scratch {
            for k in 0..4u32 {
                let w = host.read32(pos.wrapping_add(4 * k));
                host.write32(scratch.wrapping_add(4 * k), w);
            }
        }
        let mut member = 0u32;
        loop {
            member = if member == 0 { host.read32(rec.wrapping_add(0xc)) } else { host.read32(member.wrapping_add(0x60)) };
            if member == 0 {
                continue 'objects;
            }
            if host.vcall(member, 2, &[scratch as u64]) != 0 {
                break;
            }
        }
        let hdr = host.read32(rec.wrapping_add(8));
        let id = host.read16(hdr.wrapping_add(2)) as u32;
        if host.read32(o.wrapping_add(0xaa8)) & 0xfffe_ffff != id {
            let serial = host.call(0x16b928, &[obj]);
            let hdr = host.read32(rec.wrapping_add(8));
            let kind = host.read32(hdr.wrapping_add(0x1c));
            host.call(0x197b70, &[crate::area::ctl::sx(kind), serial]);
        }
        let hdr = host.read32(rec.wrapping_add(8));
        let id = host.read16(hdr.wrapping_add(2)) as u32;
        host.write32(o.wrapping_add(0xaa8), id);
    }
}

// ---- the list destroy walk (0x18c448) ----------------------------------------------------------------------------------------------

/// Calls method 1 of every object of the list whose head is the word at 0x1c89b58 (linked at +0xc0; the next link is read before the call),
/// then clears the byte at 0x1c89b60.
///
/// orig 0x18c448
pub fn destroy_list(host: &mut impl AreaHost) {
    let mut p = host.read32(0x1c89b58);
    while p != 0 {
        let next = host.read32(p.wrapping_add(0xc0));
        host.vcall(p, 1, &[]);
        p = next;
    }
    host.write8(0x1c89b60, 0);
}

// ---- the queue object of the area unit (word 0x359330) ----------------------------------------------------------------------------

/// Makes the queue object: the allocator object at the word 0x359368 gives 0x1340 bytes (its method 0), the constructor `0x3307c8` runs on
/// them, the word 0x359330 takes the block and the block's own method 0 runs.
///
/// orig 0x15d9b0
pub fn queue_create(host: &mut impl AreaHost) {
    let alloc = host.read32(0x359368);
    let blk = host.vcall(alloc, 0, &[0x1340]);
    host.call(0x3307c8, &[blk]);
    host.write32(0x359330, blk as u32);
    let obj = host.read32(0x359330);
    host.vcall(obj, 0, &[]);
}

/// Destroys the queue object: its method 3 runs if there is one, and the word 0x359330 is cleared.
///
/// orig 0x15daf0
pub fn queue_destroy(host: &mut impl AreaHost) {
    let obj = host.read32(0x359330);
    if obj != 0 {
        host.vcall(obj, 3, &[]);
    }
    host.write32(0x359330, 0);
}

// ---- the 00objentry loader ---------------------------------------------------------------------------------------------------------

/// Stores `v` in the first free slot (a zero word) of the three-word table at 0x1d6a930 and returns it. With no free slot the original
/// stores at address 0 (the search returns null); so does this. (The call of the empty function 0x1d13f0 the original makes in between has
/// no effect.)
///
/// orig 0x1d13f8
pub fn objentry_register(host: &mut impl AreaHost, v: u64) -> u64 {
    let mut slot = 0u32;
    for k in 0..3u32 {
        if host.read32(0x1d6a930 + 4 * k) == 0 {
            slot = 0x1d6a930 + 4 * k;
            break;
        }
    }
    host.write32(slot, v as u32);
    v
}

/// Reads the object entry file (its name is the pointer in the word at 0x35bd18) into a data buffer with `0x1af998` and registers the
/// buffer.
///
/// orig 0x16e1e0
pub fn objentry_load(host: &mut impl AreaHost) -> u64 {
    let name = host.read32(0x35bd18);
    let bar = host.call(0x1af998, &[crate::area::ctl::sx(name)]);
    objentry_register(host, bar)
}

// ---- the object data cache (the record at object +0x7a0) --------------------------------------------------------------------------

/// Offsets in the 0x18-byte data cache record: size, model id, entry pointer, and the three cache buffers it allocates.
pub const DC_SIZE: usize = 0;
pub const DC_ID: usize = 4;
pub const DC_ENTRY: usize = 8;
pub const DC_BUF_A: usize = 12;
pub const DC_BUF_B: usize = 16;
pub const DC_BUF_C: usize = 20;

/// Allocates the cache buffers of the record `rec`: buffer A for the name `0x16ed08` builds from the entry (size = the record's size
/// word), buffer B, when the model id passes neither `0x16ef80` nor `0x16efd8` and the entry's byte +0x48 has bit 0 clear, for the name
/// `0x16ed50` builds, and buffer C (size -1) for the name `0x16eda8` builds when the id does not pass `0x16ef80`.
///
/// orig 0x1a7808
pub fn data_cache_fill(host: &mut impl AreaHost, rec: &mut [u8]) {
    let sx = crate::area::ctl::sx;
    let size = sx(rd32(rec, DC_SIZE));
    let id = sx(rd32(rec, DC_ID));
    let entry = sx(rd32(rec, DC_ENTRY));
    let name = host.call(0x16ed08, &[entry, 0]);
    let a = host.call(0x16fd18, &[name, size]);
    wr32(rec, DC_BUF_A, a as u32);
    if host.call(0x16ef80, &[id]) == 0
        && host.call(0x16efd8, &[id]) == 0
        && (host.read8(rd32(rec, DC_ENTRY).wrapping_add(0x48)) ^ 1) & 1 != 0
    {
        let name = host.call(0x16ed50, &[entry, 0]);
        let b = host.call(0x16fd18, &[name, sx(rd32(rec, DC_SIZE))]);
        wr32(rec, DC_BUF_B, b as u32);
    }
    if host.call(0x16ef80, &[id]) == 0 {
        let name = host.call(0x16eda8, &[entry, id, 0]);
        if name != 0 {
            let c = host.call(0x16fd18, &[name, u64::MAX]);
            wr32(rec, DC_BUF_C, c as u32);
        }
    }
}

/// Points the record `rec` (at address `at`) at model `id` and entry `entry` (0: the model's entry from `0x16e208`): the old buffers are
/// freed (`0x1a7940`), the room is checked (`0x1a7d48`; when there is none the cache is dumped with `0x1700e0`), the id and entry are
/// stored and the buffers are allocated by [`data_cache_fill`].
///
/// orig 0x1a78c8
pub fn data_cache_set(host: &mut impl AreaHost, at: u32, rec: &mut [u8], id: u64, entry: u64) {
    host.call(0x1a7940, &[at as u64]);
    let entry = if entry == 0 { host.call(0x16e208, &[id]) } else { entry };
    if host.call(0x1a7d48, &[id, entry]) == 0 {
        host.call(0x1700e0, &[]);
    }
    wr32(rec, DC_ID, id as u32);
    wr32(rec, DC_ENTRY, entry as u32);
    data_cache_fill(host, rec);
}

// ---- the system table loaders (0x193610) ---------------------------------------------------------------------------------------------

/// One table entry the loader looks up (by type and name) and the function that gets its payload word (+8 of the entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableStep {
    /// The entry type looked up.
    pub ty: u32,
    /// Where the four bytes of the entry's name are in the executable's data.
    pub name_at: u32,
    /// The setter that gets the payload word.
    pub setter: u32,
}

const fn st(ty: u32, name_at: u32, setter: u32) -> TableStep {
    TableStep { ty, name_at, setter }
}

/// The steps on the first file (03system.bin) before its optional entry, in the order the loader runs them.
pub const SYSTEM_STEPS_A: [TableStep; 11] = [
    st(1, 0x37aed8, 0x1cc040),
    st(2, 0x37aee0, 0x1d0248),
    st(2, 0x37aee8, 0x1b6f38),
    st(2, 0x37aef0, 0x1845a0),
    st(2, 0x37aef8, 0x184630),
    st(2, 0x37af00, 0x193370),
    st(2, 0x37af08, 0x1a66b8),
    st(2, 0x37af10, 0x1a6800),
    st(2, 0x37af18, 0x1a6f48),
    st(2, 0x37af20, 0x1a7dd8),
    st(0x19, 0x37af28, 0x1b03a8),
];
/// The entry (by type only) the loader hands to `0x265ac0` (no argument) when it exists and its payload word is not zero.
pub const SYSTEM_OPTIONAL: (u32, u32) = (0x29, 0x265ac0);
/// The steps on the first file after the optional entry.
pub const SYSTEM_STEPS_B: [TableStep; 2] = [st(2, 0x37af30, 0x1c0b30), st(2, 0x37af38, 0x1d1118)];
/// The steps on the second file (00battle.bin).
pub const BATTLE_STEPS: [TableStep; 17] = [
    st(2, 0x37af40, 0x19e778),
    st(2, 0x37af48, 0x1d5e38),
    st(2, 0x37af50, 0x1d8ca0),
    st(2, 0x37af58, 0x1a41c8),
    st(2, 0x37af60, 0x1adfb0),
    st(2, 0x37af68, 0x1b50b0),
    st(2, 0x37af70, 0x1be4a8),
    st(2, 0x37af78, 0x1be818),
    st(2, 0x37af80, 0x1bea98),
    st(2, 0x37af88, 0x1bfef0),
    st(2, 0x37af90, 0x1c0b70),
    st(2, 0x37af98, 0x1d1c70),
    st(2, 0x37afa0, 0x1d4c58),
    st(2, 0x37afa8, 0x1a4c68),
    st(1, 0x37afb0, 0x1d3050),
    st(2, 0x37afb8, 0x1d3b80),
    st(2, 0x37afc0, 0x1d5910),
];

fn global(host: &mut impl AreaHost, at: u32, n: usize, f: impl FnOnce(&mut [u8])) {
    let mut b = vec![0u8; n];
    for (k, c) in b.chunks_mut(4).enumerate() {
        c.copy_from_slice(&host.read32(at + 4 * k as u32).to_le_bytes());
    }
    f(&mut b);
    for (k, c) in b.chunks(4).enumerate() {
        host.write32(at + 4 * k as u32, u32::from_le_bytes(c.try_into().unwrap()));
    }
}

/// Hands `payload` to the setter at `setter`: the ones ported in this module write their globals through the host, the others are
/// called through it.
fn run_setter(host: &mut impl AreaHost, setter: u32, payload: u32) {
    match setter {
        0x1845a0 => global(host, 0x1c89700, 4, |g| set_table_1845a0(g, payload)),
        0x184630 => global(host, 0x1c89704, 4, |g| set_table_184630(g, payload)),
        0x193370 => global(host, 0x1c8a970, 4, |g| set_table_193370(g, payload)),
        0x1a6800 => global(host, 0x1c95590, 4, |g| set_table_1a6800(g, payload)),
        0x1b03a8 => global(host, 0x1d5d640, 8, |g| set_table_1b03a8(g, payload)),
        0x1b50b0 => global(host, 0x1d5d6b4, 4, |g| set_table_1b50b0(g, payload)),
        0x1bea98 => global(host, 0x1d5dd80, 4, |g| set_table_1bea98(g, payload)),
        0x1bfef0 => global(host, 0x1d5df10, 4, |g| set_table_1bfef0(g, payload)),
        0x1c0b30 => global(host, 0x1d5df58, 4, |g| set_table_1c0b30(g, payload)),
        0x1c0b70 => global(host, 0x1d5df5c, 4, |g| set_table_1c0b70(g, payload)),
        0x1d0248 => global(host, 0x1d6a924, 4, |g| set_table_1d0248(g, payload)),
        0x1d5e38 => global(host, 0x3608f8, 4, |g| set_table_1d5e38(g, payload)),
        0x1a66b8 => {
            let count = host.read32(payload.wrapping_add(4));
            let (a, b) = set_table_1a66b8(payload, count);
            host.write32(0x1c95588, a);
            global(host, 0x1d6a920, 4, |g| set_table_1d0110(g, b));
        }
        0x1d3050 => set_table_1d3050(host, payload),
        other => {
            host.call(other, &[crate::area::ctl::sx(payload)]);
        }
    }
}

/// Loads the system tables: reads 03system.bin (the file name is the pointer in the word 0x35d270; `0x1af9e0` reads it into a data buffer,
/// whose address is kept in the word 0x1c8a978), hands the payload word of each wanted entry to its setter, then does the same with
/// 00battle.bin (name pointer in 0x35d274, buffer address kept in 0x1c8a97c). An entry that is missing makes the original read the word at
/// address 8 for the payload; so does this.
///
/// orig 0x193610
pub fn load_system_tables(host: &mut impl AreaHost) {
    fn run(host: &mut impl AreaHost, bar: u32, s: TableStep) {
        let name = host_name(host, s.name_at);
        let e = bar_nth_of_name(host, bar, s.ty as i64, name, 0);
        let payload = host.read32(e.wrapping_add(8));
        run_setter(host, s.setter, payload);
    }
    let sys_name = host.read32(0x35d270);
    let sys = host.call(0x1af9e0, &[crate::area::ctl::sx(sys_name)]);
    host.write32(0x1c8a978, sys as u32);
    for s in SYSTEM_STEPS_A {
        run(host, sys as u32, s);
    }
    let e = bar_nth_of_type(host, sys as u32, SYSTEM_OPTIONAL.0 as i64, 0);
    if e != 0 && host.read32(e.wrapping_add(8)) != 0 {
        host.call(SYSTEM_OPTIONAL.1, &[]);
    }
    for s in SYSTEM_STEPS_B {
        run(host, sys as u32, s);
    }
    let battle_name = host.read32(0x35d274);
    let battle = host.call(0x1af9e0, &[crate::area::ctl::sx(battle_name)]);
    host.write32(0x1c8a97c, battle as u32);
    for s in BATTLE_STEPS {
        run(host, battle as u32, s);
    }
}
