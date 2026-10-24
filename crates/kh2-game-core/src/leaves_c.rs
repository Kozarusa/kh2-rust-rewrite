//! Pure leaves of the original executable between 0x236a81 and 0x2f7ed8 (work package L1-P11c). Blocks and globals are byte slices
//! in the original's layout (RULES C7); a global read or written by a leaf is passed as the bytes at its address. Nothing here is
//! wired into the app: the callers (menus, minigames, the world map, sound) live in other systems.
//! The oracle tests are in `tests/orig_leaves_c.rs`.

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn wr32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn rdf(b: &[u8], o: usize) -> f32 {
    f32::from_bits(rd32(b, o))
}
fn wrf(b: &mut [u8], o: usize, v: f32) {
    wr32(b, o, v.to_bits());
}

/// Draw-object offsets used by the four vector setters and the colour setter below.
const VEC: usize = 0x130;
const DIRTY_VEC: usize = 0xa9;
const DIRTY_COL: usize = 0xaa;

fn vec_dirty(o: &mut [u8]) {
    let nonzero = rdf(o, VEC) != 1.0 || rdf(o, VEC + 4) != 1.0 || rdf(o, VEC + 8) != 1.0;
    o[DIRTY_VEC] = nonzero as u8;
}

/// orig 0x236ae0: stores the three floats at object +0x130/+0x134/+0x138 and sets the flag at +0xa9 unless all three equal the
/// executable's constant at 0x37f40c to 0x37f418 (1.0).
pub fn set_vec3(o: &mut [u8], x: f32, y: f32, z: f32) {
    wrf(o, VEC, x);
    wrf(o, VEC + 4, y);
    wrf(o, VEC + 8, z);
    vec_dirty(o);
}

/// orig 0x236b28: stores the first component (+0x130) and recomputes the flag at +0xa9.
pub fn set_vec_x(o: &mut [u8], x: f32) {
    wrf(o, VEC, x);
    vec_dirty(o);
}

/// orig 0x236b70: stores the second component (+0x134) and recomputes the flag at +0xa9.
pub fn set_vec_y(o: &mut [u8], y: f32) {
    wrf(o, VEC + 4, y);
    vec_dirty(o);
}

/// orig 0x236bb8: stores the third component (+0x138) and recomputes the flag at +0xa9.
pub fn set_vec_z(o: &mut [u8], z: f32) {
    wrf(o, VEC + 8, z);
    vec_dirty(o);
}

/// orig 0x236c00: stores four floats at object +0x114..+0x120 (a colour a, r, g, b, 1.0 meaning unchanged [inferred]); when all equal the constant 1.0 (0x37f41c) clears the
/// flag at +0xaa, otherwise sets it and packs the bytes into +0xcc as `a | b << 24 | g << 16 | r << 8` after scaling by 128.0 (0x37f420). The float to unsigned conversion is the SDK soft-float leaf at 0x326940
/// (truncation, negative to 0, too large to 0xffffffff), modelled as Rust's saturating cast.
pub fn set_color(o: &mut [u8], a: f32, r: f32, g: f32, b: f32) {
    wrf(o, 0x114, a);
    wrf(o, 0x118, r);
    wrf(o, 0x11c, g);
    wrf(o, 0x120, b);
    if a == 1.0 && r == 1.0 && g == 1.0 && b == 1.0 {
        o[DIRTY_COL] = 0;
        return;
    }
    o[DIRTY_COL] = 1;
    let cv = |f: f32| (f * 128.0) as u32;
    let packed = cv(a) | cv(b) << 24 | cv(g) << 16 | cv(r) << 8;
    wr32(o, 0xcc, packed);
}

/// orig 0x23b880: sets the byte at 0x36ca71 (`g` is the byte at that address).
pub fn set_flag_36ca71(g: &mut [u8]) {
    g[0] = 1;
}

/// orig 0x23b8a0: pushes a value onto the two-deep stack at 0x36ca74 (counter byte), 0x36ca78 (current), 0x36ca7c (previous). The
/// same value again only counts up.
pub fn push_36ca78(g: &mut [u8], v: u32) {
    if rd32(g, 4) == v {
        g[0] = g[0].wrapping_add(1);
        return;
    }
    if rd32(g, 4) != 0 {
        let cur = rd32(g, 4);
        wr32(g, 8, cur);
    }
    wr32(g, 4, v);
}

/// orig 0x23b8d8: undoes one push (see [`push_36ca78`]): a non-zero counter just counts down; at zero the previous value becomes
/// current and the previous slot clears.
pub fn pop_36ca78(g: &mut [u8]) {
    if g[0] != 0 {
        g[0] = g[0].wrapping_sub(1);
        return;
    }
    let prev = rd32(g, 8);
    wr32(g, 4, prev);
    wr32(g, 8, 0);
}

/// orig 0x23c820: whether the saved-region stack count at 0x36cadc is positive (`g` holds that word).
pub fn region_stack_nonempty(g: &[u8]) -> bool {
    (rd32(g, 0) as i32) > 0
}

/// orig 0x239d88: clears the word at 0x36ca18 (`g` holds it).
pub fn clear_36ca18(g: &mut [u8]) {
    wr32(g, 0, 0);
}

/// orig 0x25a848: clears the byte at 0x36cf32 (`g` holds it).
pub fn clear_36cf32(g: &mut [u8]) {
    g[0] = 0;
}

/// orig 0x2a1370: clears the byte at 0x36d3c8 (`g` holds it).
pub fn clear_36d3c8(g: &mut [u8]) {
    g[0] = 0;
}

/// orig 0x2f7b80: clears the byte at 0x3719f0 (`g` holds it).
pub fn clear_3719f0(g: &mut [u8]) {
    g[0] = 0;
}

/// orig 0x2c4300: clears the two words at 0x36f680 and 0x36f684.
pub fn clear_36f680(g: &mut [u8]) {
    g[..8].fill(0);
}

/// orig 0x295b68: copies three words of the menu-mode block at 0x36d1d4: +0x00 to +0x04, +0x0c to +0x10, +0x14 to +0x18 (`g` is
/// the 0x1c bytes from 0x36d1d4).
pub fn copy_menu_modes(g: &mut [u8]) {
    let (a, b, c) = (rd32(g, 0), rd32(g, 0xc), rd32(g, 0x14));
    wr32(g, 4, a);
    wr32(g, 0x10, b);
    wr32(g, 0x18, c);
}

/// orig 0x2d5da8: sets bits in a word: with a zero mask the bits of `v` are ORed in; otherwise only the bits under `mask` are replaced
/// by those of `v`.
pub fn set_masked_bits(w: &mut [u8], v: u32, mask: u32) {
    let cur = rd32(w, 0);
    let new = if mask == 0 { cur | v } else { cur & !mask | v & mask };
    wr32(w, 0, new);
}

/// orig 0x2d56d8: whether the pointer at 0x36fd60 (the running minigame) is set (`g` holds it).
pub fn minigame_present(g: &[u8]) -> bool {
    rd32(g, 0) != 0
}

/// orig 0x2df3c8: the byte at object +0xc0 with its low bit flipped.
pub fn flip_byte_c0(o: &[u8]) -> u8 {
    o[0xc0] ^ 1
}

/// orig 0x2ea008: the word at +8 of the record an object's first word points to (`rec` is that record).
pub fn first_ptr_word8(rec: &[u8]) -> u32 {
    rd32(rec, 8)
}

/// orig 0x2c3a70: the halfword at +8 of a record, sign-extended.
pub fn half_at_8(b: &[u8]) -> i32 {
    i16::from_le_bytes(b[8..10].try_into().unwrap()) as i32
}

/// orig 0x2c3ef8: in a table of 27 records of three words (key, ?, flag) sets the flag word of the first record whose key equals
/// `key` to 1.
pub fn mark_record(t: &mut [u8], key: u32) {
    for i in 0..27 {
        if rd32(t, i * 12) == key {
            wr32(t, i * 12 + 8, 1);
            return;
        }
    }
}

/// orig 0x2f7ec0: address of entry `i` of the table the pointer at 0x371a00 names (entries of 0x40 bytes after an 8-byte head);
/// `g` holds the pointer.
pub fn table_entry_addr(g: &[u8], i: i32) -> u32 {
    rd32(g, 0).wrapping_add((i as u32).wrapping_mul(0x40)).wrapping_add(8)
}

/// orig 0x2bdce8: always returns 0 (a stub in the audio unit).
pub fn always_zero() -> u64 {
    0
}

/// orig 0x2ee2e0: an empty function.
pub fn empty() {}
