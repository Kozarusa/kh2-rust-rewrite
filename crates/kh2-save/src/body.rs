//! The save body (0xb4e0 bytes, a copy of the save block `0x33e860`): header, CRC, size rules, export and import.

use crate::crc::Crc;
use kh2_game_core::progress_flags::check_flag;
use kh2_game_core::save_block::{LEN, at, header_word};

/// Length of the body file.
pub const BODY_LEN: usize = LEN;
/// Length of `icon.sys`.
pub const ICON_SYS_LEN: usize = 0x3c4;
/// Length of `kh2.ico`.
pub const ICON_LEN: usize = 0x8a58;
/// The version word at body `+4`.
pub const VERSION: u32 = 0x2d;

/// Bytes to KB, rounded up.
///
/// orig 0x23cd38
pub fn kb(n: i32) -> u32 {
    (n as u32).wrapping_add(0x3ff) >> 10
}

/// The free space in KB a new save needs: the three files rounded up to KB, plus 13, in steps of 10.
///
/// orig 0x23d120
pub fn space_needed() -> i32 {
    let sum = kb(ICON_SYS_LEN as i32) + kb(ICON_LEN as i32) + kb(BODY_LEN as i32);
    ((sum as i32 + 13) / 10) * 10
}

/// The size of a save file by kind: 0 `icon.sys`, 1 `kh2.ico`, anything else the body.
///
/// orig 0x23dd60
pub fn file_size(kind: i64) -> u32 {
    match kind {
        0 => ICON_SYS_LEN as u32,
        1 => ICON_LEN as u32,
        _ => BODY_LEN as u32,
    }
}

/// The CRC of a body: bytes 0..8, then 0xc..0xb4e0 (the CRC field at `+8` is skipped).
///
/// orig 0x23d180
pub fn body_crc(body: &[u8]) -> u32 {
    let mut c = Crc::new();
    c.update(&body[..8]);
    c.update(&body[0xc..BODY_LEN])
}

/// Whether a body header is valid: the CRC at `+8` matches, the magic is the one `FUN_00105758` makes for the region word, and the
/// version is 0x2d.
///
/// orig 0x23d1c8
pub fn is_valid(body: &[u8], region: u32) -> bool {
    let w = |o: usize| u32::from_le_bytes(body[o..o + 4].try_into().unwrap());
    body_crc(body) == w(8) && w(0) == header_word(region) && w(4) == VERSION
}

/// Sets the CRC field of a body.
pub fn stamp_crc(body: &mut [u8]) {
    let c = body_crc(body);
    body[8..12].copy_from_slice(&c.to_le_bytes());
}

/// 1 when world 4 is the current one and story flag 0x108b is set (the alternate world name of the slot row). `flags` are the story
/// flag pages (the save block from `+0xe50`).
///
/// orig 0x23e318
pub fn alt_world_name(world: i32, flags: &[u8]) -> bool {
    world == 4 && check_flag(flags, 0x108b)
}

/// The start-up half of `FUN_0023c830` that touches the save block: bit 8 of the word at `+0x31a8` is set from [`alt_world_name`] of the
/// current world (the byte at `+0xc`) and the other bits are kept. (The rest of that function allocates the unit's buffers: see
/// [`crate::dir::Buffers`].)
///
/// orig 0x23c830
pub fn init_alt_bit(live: &mut [u8]) {
    let alt = alt_world_name(i32::from(live[0xc]), &live[at::FLAGS..]);
    let w = u32::from_le_bytes(live[0x31a8..0x31ac].try_into().unwrap());
    let w = (w & 0xffff_feff) | (u32::from(alt) << 8);
    live[0x31a8..0x31ac].copy_from_slice(&w.to_le_bytes());
}

/// The eight flag pairs at `0x37f640` that choose the icon variant (read from the executable by the caller): `(a, b)` per entry.
pub type VariantPairs = [(u16, u16); 8];

/// The index of the icon variant: a random pick among the entries whose condition holds (`seed` is the save block's word at `+0x1604`).
/// An entry is a candidate when `a` is 0 or flag `a` is set and flag `b` is set, or when `a` is non-zero and flag `a` is clear. `None`
/// when there is no candidate (the original traps) or the seed is negative (the original then indexes below its candidate list).
///
/// orig 0x23e258
pub fn icon_variant(pairs: &VariantPairs, flags: &[u8], seed: i32) -> Option<u8> {
    let mut list = Vec::with_capacity(8);
    for (i, &(a, b)) in pairs.iter().enumerate() {
        let ok = if a == 0 || check_flag(flags, i32::from(a)) { check_flag(flags, i32::from(b)) } else { true };
        if ok {
            list.push(i as u8);
        }
    }
    if list.is_empty() || seed < 0 {
        return None;
    }
    Some(list[seed as usize % list.len()])
}

/// The body of a save: a copy of the live save block with the icon variant byte at `+0x31a8`, bit 8 of the 64-bit word there set from
/// [`alt_world_name`], and the CRC at `+8`. `None` where [`icon_variant`] has none.
///
/// orig 0x23d230
pub fn export(live: &[u8], pairs: &VariantPairs) -> Option<Vec<u8>> {
    let mut b = live[..BODY_LEN].to_vec();
    let flags = &live[at::FLAGS..];
    let seed = u32::from_le_bytes(live[0x1604..0x1608].try_into().unwrap()) as i32;
    b[0x31a8] = icon_variant(pairs, flags, seed)?;
    let alt = alt_world_name(i32::from(b[0xc]), flags);
    let mut w = u64::from_le_bytes(b[0x31a8..0x31b0].try_into().unwrap());
    w = (w & !0x100) | (u64::from(alt) << 8);
    b[0x31a8..0x31b0].copy_from_slice(&w.to_le_bytes());
    stamp_crc(&mut b);
    Some(b)
}

/// The import is a plain copy of the body over the live save block.
///
/// orig 0x23d2b8
pub fn import(live: &mut [u8], body: &[u8]) {
    live[..BODY_LEN].copy_from_slice(&body[..BODY_LEN]);
}

/// The munny word (`FUN_0023e598`).
///
/// orig 0x23e598
pub fn set_munny(live: &mut [u8], v: u32) {
    live[at::WALLET..at::WALLET + 4].copy_from_slice(&v.to_le_bytes());
}

/// Whether two buffers hold the same `kh2.ico` by CRC over their first 0x8a58 bytes (the compare of `FUN_0023dcf8`: the argument against
/// the file buffer).
///
/// orig 0x23dcf8
pub fn icon_matches(icon: &[u8], buffer: &[u8]) -> bool {
    let mut a = Crc::new();
    let mut b = Crc::new();
    a.update(&icon[..ICON_LEN]) == b.update(&buffer[..ICON_LEN])
}
