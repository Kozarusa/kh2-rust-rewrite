//! `icon.sys` (0x3c4 bytes, the PS2 browser header) and its two-line title.

use crate::body::ICON_SYS_LEN;
use kh2_game_core::save_block::{at, member_offset};

/// What the icon routines read from the executable: the magic text, the colour blocks and the title strings. Never kept in the
/// repository; the callers read them at run time (`0x37f570`, `0x37f590`, `0x37f5d0`, `0x37f600`, `0x37f630`, the strings behind
/// `0x36caf0` and `0x36caf4`, the four mark strings behind `0x36caf8` and the blank behind `0x36cb04`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconEnv {
    pub magic: [u8; 4],
    pub background: [u8; 0x40],
    pub light_dirs: [u8; 0x30],
    pub light_colors: [u8; 0x30],
    pub ambient: [u8; 0x10],
    /// The game name line (Shift-JIS), copied whole; the template line is cut to the same length.
    pub line1: Vec<u8>,
    /// The template of the second line (file number, level, play time).
    pub line2: Vec<u8>,
    /// The two-byte marks by game mode (`FUN_00105750` indexes the table).
    pub marks: [[u8; 2]; 4],
    /// The full-width blank that replaces leading zeros of the hours.
    pub blank: [u8; 2],
}

/// The title (`FUN_0023ce40`): `title` is 0x40 bytes. `live` is the save block the title describes; `n` the file number (1-based).
///
/// Mode marks beyond the table are left out (the original indexes past it).
///
/// orig 0x23ce40
pub fn write_title(env: &IconEnv, title: &mut [u8], n: i32, live: &[u8]) {
    title[..0x40].fill(0);
    let len = env.line1.len();
    title[..len].copy_from_slice(&env.line1);
    let t = len;
    title[t..t + len].copy_from_slice(&env.line2[..len.min(env.line2.len())]);
    let mode = u32::from_le_bytes(live[at::GAME_MODE..at::GAME_MODE + 4].try_into().unwrap()) as usize;
    if let Some(m) = env.marks.get(mode) {
        title[0x3e..0x40].copy_from_slice(m);
    }
    let add = |title: &mut [u8], o: usize, v: i32| {
        if v != 0 {
            title[o] = title[o].wrapping_add(v as u8);
        }
    };
    add(title, t + 3, n / 10);
    add(title, t + 5, n % 10);
    let who = if live[0x165c] & 1 == 0 { 1 } else { 0xe };
    let level = i32::from(live[at::PARTY + member_offset(who).unwrap_or(0) + 0xf]);
    add(title, t + 0xd, level / 10);
    add(title, t + 0xf, level % 10);
    // The play time is divided as an unsigned number (checked by the oracle on negative words): 216000 ticks of 60 Hz make an hour.
    let ticks = u32::from_le_bytes(live[0x1604..0x1608].try_into().unwrap());
    let hours = (ticks / 0x34bc0) as i32;
    let minutes = ((ticks % 0x34bc0) / 0xe10) as i32;
    let hundreds = hours / 100;
    if hundreds == 0 {
        title[t + 0x12..t + 0x14].copy_from_slice(&env.blank);
    } else {
        add(title, t + 0x13, hundreds);
    }
    let tens = (hours % 100) / 10;
    if tens == 0 && hundreds == 0 {
        title[t + 0x14..t + 0x16].copy_from_slice(&env.blank);
    } else {
        title[t + 0x15] = title[t + 0x15].wrapping_add(tens as u8);
    }
    add(title, t + 0x17, (hours % 100) % 10);
    add(title, t + 0x1b, minutes / 10);
    add(title, t + 0x1d, minutes % 10);
}

/// `icon.sys` for file number `n` (0-based; the title shows `n + 1`).
///
/// orig 0x23cd48
pub fn write_icon_sys(env: &IconEnv, n: i32, live: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; ICON_SYS_LEN];
    b[..4].copy_from_slice(&env.magic);
    b[6..8].copy_from_slice(&0x20u16.to_le_bytes());
    b[0xc..0x10].copy_from_slice(&0x60u32.to_le_bytes());
    b[0x10..0x50].copy_from_slice(&env.background);
    b[0x50..0x80].copy_from_slice(&env.light_dirs);
    b[0x80..0xb0].copy_from_slice(&env.light_colors);
    b[0xb0..0xc0].copy_from_slice(&env.ambient);
    write_title(env, &mut b[0xc0..0x100], n + 1, live);
    for o in [0x104, 0x144, 0x184] {
        b[o..o + 7].copy_from_slice(b"kh2.ico");
    }
    b
}
