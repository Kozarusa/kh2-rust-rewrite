//! Core leaves of the original between 0x14afc9 and 0x236a80 (WP-L1-P11b): getters, setters, small initialisers and vector
//! helpers, each a few instructions. Every function works on the bytes of the block or global it touches, in the original's
//! layout (RULES C7): a block argument is a byte slice, a global is the slice of its bytes (the address is in the doc line),
//! and a constant the original reads from the executable's data (a float such as the scale at 0x3792c8) is a parameter, so no
//! table value lives here. A pointer the original follows is the slice it points at (`None` for null). Nothing here is wired
//! into the app; the oracle tests are `tests/orig_leaves_b.rs`.

fn r32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn w32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn rf(b: &[u8], o: usize) -> f32 {
    f32::from_bits(r32(b, o))
}
fn wf(b: &mut [u8], o: usize, v: f32) {
    w32(b, o, v.to_bits());
}
fn w64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}

// ---- rectangle (GS packet owner) setters --------------------------------------------------------------------------

/// orig 0x14b350: stores the 64-bit GS PRIM word at rect +0xc0.
pub fn rect_set_gs_prim(rect: &mut [u8], v: u64) {
    w64(rect, 0xc0, v);
}
/// orig 0x14b378: stores the 64-bit GS FRAME word at rect +0x40.
pub fn rect_set_gs_frame(rect: &mut [u8], v: u64) {
    w64(rect, 0x40, v);
}
/// orig 0x14b388: stores the 64-bit GS TEST word at rect +0x80.
pub fn rect_set_gs_test(rect: &mut [u8], v: u64) {
    w64(rect, 0x80, v);
}
/// orig 0x14b398: stores the 64-bit GS TEX0 word at rect +0x60.
pub fn rect_set_gs_tex0(rect: &mut [u8], v: u64) {
    w64(rect, 0x60, v);
}
/// orig 0x14b3a0: stores the 64-bit GS TEX1 word at rect +0x70.
pub fn rect_set_gs_tex1(rect: &mut [u8], v: u64) {
    w64(rect, 0x70, v);
}

// ---- address helpers and constants ---------------------------------------------------------------------------------

/// orig 0x14e370: the address 0x2f0 bytes into an object (the original's callers pass the object at 0x358000).
pub fn offset_2f0(base: i32) -> i32 {
    base.wrapping_add(0x2f0)
}
/// orig 0x15ea40: the address of the map's pax record, 0x35a7f0.
pub fn map_pax_addr() -> u32 {
    0x35a7f0
}
/// orig 0x1b3d68: the word at 0x1d5d6b0 plus 0x18 (`word` is that global).
pub fn global_1d5d6b0_plus_18(word: u32) -> i32 {
    (word as i32).wrapping_add(0x18)
}
/// orig 0x1a9420: the address of entry `i` (0x20 bytes each) of the font table at 0x1c95650.
pub fn font_entry_addr(i: i32) -> i32 {
    0x1c95650i32.wrapping_add(i.wrapping_mul(0x20))
}
/// orig 0x1ddff8: `base + (a * 3 + b - 1) * 0x40`, the address of cell (a, b) in a table of 3-wide rows of 0x40 bytes.
pub fn cell_addr(base: i32, a: i32, b: i32) -> i32 {
    base.wrapping_add(a.wrapping_mul(3).wrapping_add(b).wrapping_sub(1).wrapping_mul(0x40))
}
/// orig 0x2302c8: the image record 0x20 bytes into the object the global at 0x36c200 points at (`word` is that global).
pub fn musical_target_image(word: u32) -> i32 {
    (word as i32).wrapping_add(0x20)
}
/// orig 0x1cf330: the Sora object pointer, the global word at 0x360400 (`word`), returned as it is.
pub fn sora_object(word: u32) -> u32 {
    word
}
/// orig 0x1d13f0: an empty function (returns at once).
pub fn nop_1d13f0() {}

// ---- single-byte flag globals ---------------------------------------------------------------------------------------

/// orig 0x15c9a8: clears the byte at 0x359038.
pub fn clear_byte_359038(g: &mut u8) {
    *g = 0;
}
/// orig 0x15eda0: clears the map's pax-ready byte at 0x35a80d.
pub fn map_clear_pax_ready(g: &mut u8) {
    *g = 0;
}
/// orig 0x1612a0: clears the camera's kick byte at 0x35a872 (`setNoKick`).
pub fn camera_set_no_kick(g: &mut u8) {
    *g = 0;
}
/// orig 0x1b62d8: clears the byte at 0x35f1d1.
pub fn clear_byte_35f1d1(g: &mut u8) {
    *g = 0;
}
/// orig 0x1d3a58: clears the byte at 0x360770.
pub fn clear_byte_360770(g: &mut u8) {
    *g = 0;
}
/// orig 0x15ea30: the map's pax-exists byte at 0x35a80c (`byte` is that global), returned as it is.
pub fn map_pax_exists(byte: u8) -> u8 {
    byte
}
/// orig 0x222058: the byte at 0x36bcd9, returned as it is.
pub fn byte_36bcd9(byte: u8) -> u8 {
    byte
}
/// orig 0x224b50: true when the byte at 0x33e830 (the event mode) is 0xb (`byte` is that global).
pub fn event_is_lm(byte: u8) -> bool {
    byte == 0xb
}

// ---- word globals -------------------------------------------------------------------------------------------------

/// orig 0x2236c0: sets the event read-wait word at 0x36b9cc to 0x4c4b40.
pub fn event_read_wait_init(g: &mut [u8]) {
    w32(g, 0, 0x4c4b40);
}
/// orig 0x223780: clears the three event words at 0x36ba20, 0x36ba24 and 0x36ba28 (`g` is the 12 bytes from 0x36ba20).
pub fn event_clear_3(g: &mut [u8]) {
    g[..12].fill(0);
}
/// orig 0x1dd950: stores `v` at 0x360a70 and clears the word at 0x360a74 (`g` is the 8 bytes from 0x360a70).
pub fn set_360a70(g: &mut [u8], v: u32) {
    w32(g, 0, v);
    w32(g, 4, 0);
}
/// orig 0x16c088: stores `v` at 0x522180 and clears the words at 0x522188 and 0x52218c (`g` is the 16 bytes from 0x522180;
/// the word at 0x522184 is left alone).
pub fn assert_list_init(g: &mut [u8], v: u32) {
    w32(g, 0, v);
    w32(g, 0xc, 0);
    w32(g, 8, 0);
}
/// orig 0x15a210: the target camera takes a new target: the first four words of `target` go to 0x358b70..0x358b7c, the fifth
/// to 0x358b80, and the byte at 0x358b40 becomes 1 (`cam` is the 0x44 bytes from 0x358b40).
pub fn target_camera_change(target: &[u8], cam: &mut [u8]) {
    cam[0x30..0x40].copy_from_slice(&target[..0x10]);
    cam[0] = 1;
    cam[0x40..0x44].copy_from_slice(&target[0x10..0x14]);
}
/// orig 0x15c6d0: the four colour bytes at 0x520600, 0x520601, 0x520602, 0x520603 (`g` is those four bytes).
pub fn set_colour_520600(g: &mut [u8], a0: u8, a1: u8, a2: u8, a3: u8) {
    g[0] = a0;
    g[1] = a1;
    g[3] = a3;
    g[2] = a2;
}
/// orig 0x16f260: stores `v` in slot `i` of the word table at 0x1c88358 (`table` is the table; no bound check).
pub fn store_slot_1c88358(table: &mut [u8], i: i32, v: u32) {
    w32(table, (i as usize) * 4, v);
}
/// orig 0x16ff18: the fixed-cache cursor steps: the pointer word at 0x35bd84 advances by `table[idx]` (the table at
/// 0x1c88340) and the index word at 0x35bd90 goes up by one.
pub fn fix_free(ptr: &mut u32, idx: &mut u32, table: &[u8]) {
    let off = r32(table, (*idx as usize).wrapping_mul(4));
    *idx = idx.wrapping_add(1);
    *ptr = ptr.wrapping_add(off);
}
/// orig 0x18b6c8: pushes the 48-byte-slot number of the object at `addr` (its distance from 0x1c89a70, divided by 48 with the
/// multiplicative inverse of 3 after a shift by 4) onto the queue at 0x1c89b30; `count` is the word at 0x1c89b40.
pub fn slot_queue_push(entries: &mut [u8], count: &mut u32, addr: i32) {
    let n = (addr.wrapping_sub(0x1c89a70) >> 4).wrapping_mul(-0x5555_5555);
    w32(entries, (*count as usize) * 4, n as u32);
    *count = count.wrapping_add(1);
}
/// orig 0x1adfd0: clears the three 12-byte slots at 0x1cb8290: the first word 0, the other two 0xffffffff (`g` is the 36 bytes).
pub fn slot_table_clear(g: &mut [u8]) {
    for s in 0..3 {
        w32(g, s * 12, 0);
        w32(g, s * 12 + 4, 0xffff_ffff);
        w32(g, s * 12 + 8, 0xffff_ffff);
    }
}
/// orig 0x1ae388: the index of the first of the three slots at 0x1cb8290 whose second word equals `v`, or -1 (`g` is the 36 bytes).
pub fn slot_table_find(g: &[u8], v: i32) -> i32 {
    for s in 0..3 {
        if r32(g, s * 12 + 4) as i32 == v {
            return s as i32;
        }
    }
    -1
}
/// orig 0x1a6b30: true when the two ring words at 0x1c95598 and 0x1c9559c are equal (`g` is those 8 bytes).
pub fn ring_is_empty(g: &[u8]) -> bool {
    r32(g, 4) == r32(g, 0)
}
/// orig 0x1a6b50: the ring distance `(word@0x1c95598 - word@0x1c9559c + 8)` as the remainder after division by 8, with C's sign
/// rule (`g` is the 8 bytes from 0x1c95598).
pub fn ring_distance(g: &[u8]) -> i32 {
    (r32(g, 0) as i32).wrapping_sub(r32(g, 4) as i32).wrapping_add(8).wrapping_rem(8)
}

// ---- object field getters and setters ---------------------------------------------------------------------------------

/// orig 0x155b78: whether the drive gauge's player has drive-meter sequence number 4: `status` is the block at actor +0x48c of
/// the actor the gauge (+0x34) points at, `None` when that pointer is null; the byte tested is status +0x1af.
pub fn drive_meter_seq_is_4(status: Option<&[u8]>) -> bool {
    status.is_some_and(|s| s[0x1af] == 4)
}
/// orig 0x157dd8: the party gauge's group number, gauge +0x2c plus +0x30 plus 3.
pub fn gauge_party_group(gauge: &[u8]) -> i32 {
    (r32(gauge, 0x2c) as i32).wrapping_add(r32(gauge, 0x30) as i32).wrapping_add(3)
}
/// orig 0x16ecf8: true when the byte at +0x28 is not zero.
pub fn flag_28_set(obj: &[u8]) -> bool {
    obj[0x28] != 0
}
/// orig 0x19b380: clears the word at +0x28 (the lock-on's player target).
pub fn lockon_reset_player(obj: &mut [u8]) {
    w32(obj, 0x28, 0);
}
/// orig 0x1b3f68: stores three words at +0, +4 and +8.
pub fn set_words_3(obj: &mut [u8], a: u32, b: u32, c: u32) {
    w32(obj, 8, c);
    w32(obj, 0, a);
    w32(obj, 4, b);
}
/// orig 0x1d3710: clears three words at +0, +4 and +8.
pub fn clear_words_3(obj: &mut [u8]) {
    w32(obj, 8, 0);
    w32(obj, 0, 0);
    w32(obj, 4, 0);
}
/// orig 0x1b60b8: advances the write cursor at +8 by `n` 16-byte units.
pub fn cursor_advance(obj: &mut [u8], n: i32) {
    let c = r32(obj, 8).wrapping_add((n as u32).wrapping_mul(0x10));
    w32(obj, 8, c);
}
/// orig 0x1bb338: clears the four words at +0x974 and the word at +0x84c.
pub fn clear_slots_974(obj: &mut [u8]) {
    for i in 0..4 {
        w32(obj, 0x974 + i * 4, 0);
    }
    w32(obj, 0x84c, 0);
}
/// orig 0x1c40b8: stores the address 0x1c3eb0 (a handler function) at +0x54.
pub fn set_handler_54(obj: &mut [u8]) {
    w32(obj, 0x54, 0x1c3eb0);
}
/// orig 0x1da138: effect (obj +0x30 and +0x34 take `a` and `b`) and flag 0x40000 set in the flags word at +8.
pub fn effect_set_param_40000(obj: &mut [u8], a: u32, b: u32) {
    w32(obj, 0x30, a);
    w32(obj, 0x34, b);
    let f = r32(obj, 8) | 0x40000;
    w32(obj, 8, f);
}
/// orig 0x1da158: the same with flag 0x80000.
pub fn effect_set_param_80000(obj: &mut [u8], a: u32, b: u32) {
    w32(obj, 0x30, a);
    w32(obj, 0x34, b);
    let f = r32(obj, 8) | 0x80000;
    w32(obj, 8, f);
}
/// orig 0x1da338: sets flag 0x800000 (no draw) in the flags word at +8.
pub fn effect_set_no_draw(obj: &mut [u8]) {
    let f = r32(obj, 8) | 0x80_0000;
    w32(obj, 8, f);
}
/// orig 0x1da350: clears flag 0x800000 in the flags word at +8.
pub fn effect_reset_no_draw(obj: &mut [u8]) {
    let f = r32(obj, 8) & 0xff7f_ffff;
    w32(obj, 8, f);
}
/// orig 0x1db410: the pax record's constructor: words +0, +4, +8, +0xc, +0x10 and +0x18 become 0.
pub fn pax_init(obj: &mut [u8]) {
    for o in [0xc, 0, 4, 8, 0x18, 0x10] {
        w32(obj, o, 0);
    }
}
/// orig 0x2331d0: the image record's constructor: +0x194, +0x38, +0x3c cleared and the table word 0x36c9c8 stored at +0.
pub fn image_init(obj: &mut [u8]) {
    w32(obj, 0x194, 0);
    w32(obj, 0x38, 0);
    w32(obj, 0, 0x36c9c8);
    w32(obj, 0x3c, 0);
}
/// orig 0x233200: stores the table word 0x36c9c8 at +0.
pub fn image_set_table(obj: &mut [u8]) {
    w32(obj, 0, 0x36c9c8);
}
/// orig 0x233998: the 24-byte record's constructor: table word 0x36c9b8 at +0, the other five words cleared.
pub fn record_init_233998(obj: &mut [u8]) {
    w32(obj, 0x14, 0);
    w32(obj, 4, 0);
    w32(obj, 0, 0x36c9b8);
    w32(obj, 8, 0);
    w32(obj, 0xc, 0);
    w32(obj, 0x10, 0);
}
/// orig 0x234310: clears the word at +0x50.
pub fn clear_word_50(obj: &mut [u8]) {
    w32(obj, 0x50, 0);
}
/// orig 0x234a40: stores `v` at +0x9c.
pub fn set_word_9c(obj: &mut [u8], v: u32) {
    w32(obj, 0x9c, v);
}
/// orig 0x2369d0: true when the word at +0xf8 is not zero (sequence draw mode).
pub fn sequence_draw_mode(obj: &[u8]) -> bool {
    r32(obj, 0xf8) != 0
}
/// orig 0x224448: reads the word at +0 (the event's integer argument).
pub fn event_get_int(arg: &[u8]) -> u32 {
    r32(arg, 0)
}
/// orig 0x18cd90: sets the mode byte at +0x2c (3 becomes 4) and the mask byte at +0x2d, `!(1 << mode | 1)`.
pub fn set_mode_and_mask(obj: &mut [u8], mode: u32) {
    let mode = if mode == 3 { 4 } else { mode };
    obj[0x2c] = mode as u8;
    obj[0x2d] = !((1u32.wrapping_shl(mode & 31)) as u8 | 1);
}
/// orig 0x18dc80: true when byte +4 of the record the object's +0x28 points at is 2 or 3 (`rec` is that record).
pub fn kind_is_2_or_3(rec: &[u8]) -> bool {
    rec[4].wrapping_sub(2) < 2
}
/// orig 0x1b5670: maps a code to a slot: 2 to 1, 1 to 0, 3 to 2, anything else to 3.
pub fn code_to_slot(code: i32) -> u32 {
    match code {
        2 => 1,
        1 => 0,
        3 => 2,
        _ => 3,
    }
}
/// orig 0x1a9438: the byte at `idx` in the buffer the object's +0x1c points at (`buf`).
pub fn byte_at_ptr_1c(buf: &[u8], idx: i32) -> u8 {
    buf[idx as usize]
}
/// orig 0x1a9448: the byte at `idx` in the buffer the global word at 0x1c958b4 points at (`buf`).
pub fn byte_at_global_buf(buf: &[u8], idx: i32) -> u8 {
    buf[idx as usize]
}
/// orig 0x191078: the little-endian halfword from the two bytes at `off` in `base`, returned sign-extended (the original returns it
/// in a 32-bit register as a signed halfword).
pub fn halfword_at(base: &[u8], off: usize) -> i16 {
    (u16::from(base[off]) | u16::from(base[off + 1]) << 8) as i16
}
/// orig 0x233c38: `obj[8] + table[idx * 8]`, with `table` the array the object's +0xc points at.
pub fn base_plus_table_entry(obj: &[u8], table: &[u8], idx: i32) -> i32 {
    (r32(obj, 8) as i32).wrapping_add(r32(table, (idx as usize) * 8) as i32)
}
/// orig 0x1d8f90: pops from a stack of word entries: `obj[8]` (the count) goes down by one and the result is
/// `obj[0xc] + obj[0x14] * entries[count]`, `entries` being the array the object's +0x10 points at.
pub fn stack_pop_scaled(obj: &mut [u8], entries: &[u8]) -> i32 {
    let n = r32(obj, 8).wrapping_sub(1);
    w32(obj, 8, n);
    (r32(obj, 0xc) as i32).wrapping_add((r32(obj, 0x14) as i32).wrapping_mul(r32(entries, (n as i32 as usize).wrapping_mul(4)) as i32))
}
/// orig 0x19cff8: the collision draw byte: the record's byte +0x1e, replaced by 0xca when that byte is below 0xca, the halfword
/// +0x22 is not zero, the owner's flags (`flags`, owner +0x160) have 0x200000 and the owner's id (owner +0x50c) equals the halfword.
pub fn collision_draw_byte(rec: &[u8], flags: u32, owner_id: u32) -> u8 {
    let b = rec[0x1e];
    let h = u16::from(rec[0x22]) | u16::from(rec[0x23]) << 8;
    if b < 0xca && h != 0 && flags & 0x20_0000 != 0 && owner_id == u32::from(h) { 0xca } else { b }
}
/// orig 0x19d5e8: the blur record's defaults: words 0, 4 and 0x14 are 0; 0xc, 0x10 and 0x18 are 1; +8 is 0x70.
pub fn blur_init(obj: &mut [u8]) {
    w32(obj, 0, 0);
    w32(obj, 0x18, 1);
    w32(obj, 8, 0x70);
    w32(obj, 4, 0);
    w32(obj, 0xc, 1);
    w32(obj, 0x10, 1);
    w32(obj, 0x14, 0);
}
/// orig 0x236a80: stores `v` at +0x12c and sets the byte at +0xa8 to whether any of the floats at +0x124, +0x128 and `v` is not zero.
pub fn set_extent_z(obj: &mut [u8], v: f32) {
    wf(obj, 0x12c, v);
    obj[0xa8] = u8::from(rf(obj, 0x124) != 0.0 || rf(obj, 0x128) != 0.0 || v != 0.0);
}
/// orig 0x2349d0: stores three floats at +0x7c, +0x80 and +0x84 and the products `a * b` at +0x2c and `a * c` at +0x30.
pub fn set_scale3(obj: &mut [u8], a: f32, b: f32, c: f32) {
    wf(obj, 0x7c, a);
    wf(obj, 0x80, b);
    wf(obj, 0x84, c);
    wf(obj, 0x2c, a * b);
    wf(obj, 0x30, a * c);
}
/// orig 0x191b58: resets a ratio record: +0xc cleared; +4, +8 and +0x14 take the float `init` (the word at 0x37ae48).
pub fn ratio_reset(obj: &mut [u8], init: u32) {
    w32(obj, 0xc, 0);
    w32(obj, 0x14, init);
    w32(obj, 4, init);
    w32(obj, 8, init);
}
/// orig 0x191b78: +4 takes the value of +0x14 and +8 takes the float `init2` (the word at 0x37ae4c).
pub fn ratio_restore(obj: &mut [u8], init2: u32) {
    let v = r32(obj, 0x14);
    w32(obj, 4, v);
    w32(obj, 8, init2);
}
/// orig 0x191b98: the product of the floats at +4 and +8.
pub fn ratio_product(obj: &[u8]) -> f32 {
    rf(obj, 4) * rf(obj, 8)
}
/// orig 0x2308d8: scales the alpha (top byte) of an RGBA word by `scale`, saturating at 0xff; a scale equal to `unity` (the
/// float at 0x37f2fc) returns the word as it is. The scaled alpha is the float product truncated to an integer; a negative
/// result is read unsigned, so it saturates to 0xff too.
pub fn scale_alpha(scale: f32, rgba: u32, unity: f32) -> u32 {
    if scale == unity {
        return rgba;
    }
    let a = ((rgba >> 24) as f32 * scale) as i32 as u32;
    let a = if a < 0x100 { a } else { 0xff };
    (rgba & 0x00ff_ffff) | (a << 24)
}

// ---- lists ---------------------------------------------------------------------------------------------------------------

/// orig 0x16c808: appends `node` to the singly linked list whose head and tail are the words at 0x522188 and 0x52218c; the
/// link is the word at node +0x28. `link(addr, value)` stores a link word (a store of `value` at `addr` + 0x28).
pub fn list_append_522188(head: &mut u32, tail: &mut u32, node: u32, mut link: impl FnMut(u32, u32)) {
    if *tail != 0 {
        link(*tail, node);
        link(node, 0);
        *tail = node;
        return;
    }
    *head = node;
    *tail = node;
    link(node, 0);
}
/// orig 0x17eab8: appends `node` to a motion's sync list: `m` +0x98 is the tail and +0x94 the head; the link is the word at
/// node +0x9c. `link(addr, value)` stores a link word at `addr` + 0x9c.
pub fn motion_sync_append(m: &mut [u8], node: u32, mut link: impl FnMut(u32, u32)) {
    let tail = r32(m, 0x98);
    if tail != 0 {
        link(tail, node);
        link(node, 0);
        w32(m, 0x98, node);
        return;
    }
    w32(m, 0x98, node);
    w32(m, 0x94, node);
    link(node, 0);
}

// ---- text and vectors -----------------------------------------------------------------------------------------------------

/// orig 0x1919c8: converts `count` 16-bit text codes to the byte string the draw code reads, into `dst`: 0 becomes the byte 1; a
/// code below 0xe0 becomes `code + 0x20`; any other code becomes two bytes, `((code - 0xe0) >> 8) + 0x19` and `(code - 0xe0)`
/// low byte. A zero byte ends the string. Returns the number of bytes written before that terminator.
pub fn text_codes_to_bytes(src: &[u16], count: i32, dst: &mut [u8]) -> usize {
    let mut n = 0;
    for &u in &src[..count.max(0) as usize] {
        if u == 0 {
            dst[n] = 1;
        } else if u < 0xe0 {
            dst[n] = (u as u8).wrapping_add(0x20);
        } else {
            let v = u.wrapping_sub(0xe0);
            dst[n] = ((v >> 8) as u8).wrapping_add(0x19);
            n += 1;
            dst[n] = v as u8;
        }
        n += 1;
    }
    dst[n] = 0;
    n
}

// ---- vector helpers (the original uses the VU0 macro mode; the port computes the same in f32) -----------------------------------

/// orig 0x1530c0: stores the four bytes as floats in the vector and multiplies it by the reciprocal of the float whose bits are
/// `divisor` (the float at 0x3792c8, 255.0 in the executable); returns the divisor (it is left in the float return register).
pub fn colour_bytes_to_unit(v: &mut [u8], r: u8, g: u8, b: u8, a: u8, divisor: f32) -> f32 {
    let q = 1.0 / divisor;
    for (i, x) in [r, g, b, a].into_iter().enumerate() {
        wf(v, i * 4, f32::from(x) * q);
    }
    divisor
}
/// orig 0x167078: masks the four 64-bit words at +0 with `mask` and clears the vectors at +0x20 and +0x30 and the word at +0x40.
pub fn mask_block_167078(b: &mut [u8], mask: u64) {
    for i in 0..4 {
        let v = u64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap()) & mask;
        w64(b, i * 8, v);
    }
    w32(b, 0x40, 0);
    b[0x20..0x40].fill(0);
}
/// orig 0x1b5a18: adds the vector `vec` to each of the entries at `buf` +0x20, +0x40, and so on; the entry count is the word at +0.
pub fn add_vec_to_entries(buf: &mut [u8], vec: &[u8]) {
    let n = r32(buf, 0) as i32;
    for i in 0..n.max(0) as usize {
        let o = (i + 1) * 0x20;
        for k in 0..4 {
            let s = rf(buf, o + k * 4) + rf(vec, k * 4);
            wf(buf, o + k * 4, s);
        }
    }
}
/// orig 0x21fcd0: copies the x, y and z floats of the vector at `src` over `dst`; its w float stays.
pub fn vec3_copy(dst: &mut [u8], src: &[u8]) {
    dst[..12].copy_from_slice(&src[..12]);
}
/// orig 0x21ffb8: the length of the vector in its first three floats (returned in the float register).
pub fn vec3_length(v: &[u8]) -> f32 {
    let (x, y, z) = (rf(v, 0), rf(v, 4), rf(v, 8));
    (x * x + y * y + z * z).sqrt()
}
/// orig 0x1824e0: copies the vector `src` to the vector at obj +0x10, scales its x, y and z floats to length 1 (a zero vector
/// is scaled by the largest finite float, as the VU's divide-by-zero clamp gives) and stores `w` over its w float (obj +0x1c).
/// The original also returns a VU register that no caller of this port needs; it is not modelled. The original skips the copy when
/// `src` is the vector itself; callers of the port with that case need not copy.
pub fn normalize_into(obj: &mut [u8], src: &[u8], w: f32) {
    obj[0x10..0x20].copy_from_slice(&src[..16]);
    let (x, y, z) = (rf(obj, 0x10), rf(obj, 0x14), rf(obj, 0x18));
    let len = (x * x + y * y + z * z).sqrt();
    let q = if len == 0.0 { f32::MAX } else { 1.0 / len };
    wf(obj, 0x10, x * q);
    wf(obj, 0x14, y * q);
    wf(obj, 0x18, z * q);
    wf(obj, 0x1c, w);
}
/// orig 0x15db30: sets the byte at +4 of the object the global at 0x359330 points at (`obj`), when that pointer is not null.
pub fn global_object_set_flag_4(obj: Option<&mut [u8]>) {
    if let Some(o) = obj {
        o[4] = 1;
    }
}
