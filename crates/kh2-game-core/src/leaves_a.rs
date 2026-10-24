//! Core leaves of the executable between 0x102e00 and 0x14afc8 (WP-L1-P11a): getters and setters of the boot-time globals around
//! `0x33e6e0..0x33e830` (game modes, loader objects, pause and gummi parameters), the small table lookup at `0x377e50`, the soft-reset
//! flag, the C++ allocator thunks, and the game-mode setter.
//!
//! A getter of global state takes the bytes of the global in the original's layout: `g` starts at the address named in the doc line
//! of the function (the oracle test hands it exactly that range), so `g[0..4]` is the word at that address. Addresses the original
//! returns (the `&DAT_...` getters) are plain constants. The allocator thunks do not call anything themselves: they return the
//! [`Call`] the original makes through the virtual table, and a caller (or the oracle test) performs it.
//!
//! Not wired into the app: nothing calls these yet (the port's callers still reach the originals' behaviour by other means).

fn r32(g: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(g[at..at + 4].try_into().unwrap())
}

fn w32(g: &mut [u8], at: usize, v: u32) {
    g[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// The word at `0x33e73c`.
///
/// orig 0x102e00
pub fn word_33e73c(g: &[u8]) -> u32 {
    r32(g, 0)
}

/// The byte at `0x33e740`.
///
/// orig 0x102ee0
pub fn byte_33e740(g: &[u8]) -> u8 {
    g[0]
}

/// Clears the byte at `0x33e740` (`g` starts there).
///
/// orig 0x102ef0
pub fn clear_byte_33e740(g: &mut [u8]) {
    g[0] = 0;
}

/// The title-mode object: the address `0x33e710` itself.
///
/// orig 0x102f18
pub const fn title_mode() -> u32 {
    0x33e710
}

/// The word at `0x33e780`.
///
/// orig 0x103408
pub fn word_33e780(g: &[u8]) -> u32 {
    r32(g, 0)
}

/// The word at `0x33e774`.
///
/// orig 0x103418
pub fn word_33e774(g: &[u8]) -> u32 {
    r32(g, 0)
}

/// The word at `0x33e778` (the object allocator of the loader).
///
/// orig 0x103428
pub fn word_33e778(g: &[u8]) -> u32 {
    r32(g, 0)
}

/// The menu-mode object: the address `0x33e748` itself.
///
/// orig 0x103d68
pub const fn menu_mode() -> u32 {
    0x33e748
}

/// The pause parameter, the word at `0x33e7d0`.
///
/// orig 0x103fb0
pub fn pause_param(g: &[u8]) -> u32 {
    r32(g, 0)
}

/// The gummi battle-mode object: the address `0x33e7a0` itself.
///
/// orig 0x104048
pub const fn gummi_battle_mode() -> u32 {
    0x33e7a0
}

/// The word at `0x33e7d4`.
///
/// orig 0x104058
pub fn word_33e7d4(g: &[u8]) -> u32 {
    r32(g, 0)
}

/// The word at `0x33e7dc`, which reads 1 once asked: a zero is replaced by 1 in place first (`g` starts at `0x33e7dc`).
///
/// orig 0x104078
pub fn word_33e7dc_at_least_one(g: &mut [u8]) -> u32 {
    if r32(g, 0) == 0 {
        w32(g, 0, 1);
    }
    r32(g, 0)
}

/// The word at `0x33e7cc`.
///
/// orig 0x104120
pub fn word_33e7cc(g: &[u8]) -> u32 {
    r32(g, 0)
}

/// Byte `i` of the table at `0x33e820` (`g` starts there; the original does not bound-check `i`).
///
/// orig 0x1041d8
pub fn byte_33e820(g: &[u8], i: i32) -> u8 {
    g[i as usize]
}

/// The gummi menu-mode object: the address `0x33e7e0` itself.
///
/// orig 0x104488
pub const fn gummi_menu_mode() -> u32 {
    0x33e7e0
}

/// The word at `0x33e80c`.
///
/// orig 0x104498
pub fn word_33e80c(g: &[u8]) -> u32 {
    r32(g, 0)
}

/// The value paired with `key` in the table of 16-bit pairs at `0x377e50` (`g` starts there). The table ends at the first pair whose
/// key is negative; the whole table is walked and the LAST pair with an equal key wins; `-1` when none matches.
///
/// orig 0x103280
pub fn table_377e50(g: &[u8], key: i32) -> i16 {
    let at = |i: usize| i16::from_le_bytes([g[i * 4], g[i * 4 + 1]]);
    let val = |i: usize| i16::from_le_bytes([g[i * 4 + 2], g[i * 4 + 3]]);
    let mut out = -1i16;
    let mut i = 0usize;
    while at(i) >= 0 {
        if at(i) == key as i16 {
            out = val(i);
        }
        i += 1;
    }
    out
}

/// The soft-reset disable flag at `0x35639a` (`g` starts there) is set: soft reset is blocked.
///
/// orig 0x14a220
pub fn soft_reset_disable(g: &mut [u8]) {
    g[0] = 1;
}

/// Clears the soft-reset disable flag at `0x35639a`.
///
/// orig 0x14a230
pub fn soft_reset_enable(g: &mut [u8]) {
    g[0] = 0;
}

/// The address `0x356a20` itself (a block the area code reads through).
///
/// orig 0x14a938
pub const fn block_356a20() -> u32 {
    0x356a20
}

/// A call through a virtual table: the function address and its first arguments (the unused ones are the original's zeros).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Call {
    pub func: u32,
    pub args: [u32; 3],
}

/// Allocation through an allocator object: calls slot 0 of the object's table as `slot0(object, size_arg, 0)`. `rd` reads a word
/// of memory. The two entry points are the same code (placement-new style and the loader's allocation).
///
/// orig 0x106220
/// orig 0x106250
pub fn alloc_call(size_arg: u32, object: u32, mut rd: impl FnMut(u32) -> u32) -> Call {
    let table = rd(object);
    Call { func: rd(table), args: [object, size_arg, 0] }
}

/// Free of a block that carries its allocator in the word before it: nothing for a null pointer (a 64-bit compare), otherwise slot 1
/// of the allocator's table as `slot1(allocator, block)`. The two entry points are the same code.
///
/// orig 0x106280
/// orig 0x1062b0
pub fn free_call(block: u64, mut rd: impl FnMut(u32) -> u32) -> Option<Call> {
    if block == 0 {
        return None;
    }
    let block = block as u32;
    let owner = rd(block.wrapping_sub(4));
    let table = rd(owner);
    Some(Call { func: rd(table.wrapping_add(4)), args: [owner, block, 0] })
}

/// The game-mode word at `0x33e784` that `FUN_00103a48` writes. With `requested == -1` (a 64-bit compare) the mode is chosen from the
/// object of the first task on the task list whose `+0x1c` is 1 (`first_flagged`): 1 for the field mode (`0x33e6e0`), 4 title
/// (`0x33e710`), 2 gummi battle (`0x33e7a0`), 3 gummi menu (`0x33e7e0`), and the current word for any other object. Any other request is
/// stored as is (low 32 bits). Returns the new word.
///
/// With no flagged task the original reads an uninitialised stack word; the caller passes 0 then [inferred: no caller does that].
///
/// orig 0x103a48
pub fn game_mode(requested: i64, current: u32, first_flagged: u32) -> u32 {
    if requested != -1 {
        return requested as u32;
    }
    match first_flagged {
        0x33e6e0 => 1,
        0x33e710 => 4,
        0x33e7a0 => 2,
        0x33e7e0 => 3,
        _ => current,
    }
}
