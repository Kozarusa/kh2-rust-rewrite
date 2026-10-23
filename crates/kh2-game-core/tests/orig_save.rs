//! Oracle tests for the save block ports of `save_block.rs` and the party-block rules of `progress.rs` (core.save, WP-L1-P8). kh2-ee runs
//! the original from the user's executable on random blocks and the port runs on copies of the same bytes; every byte of every block
//! and global the function may touch is compared (the harness of `kh2_factory::fuzz`, 1,000 to 4,000 cases per function).
//!
//! The new game's save block (`FUN_00105638`) is compared whole: the original runs on a save block full of random bytes with a random
//! region word and a random fix-up list, the port builds the block from nothing, and all 0xb4e0 bytes must agree.
//!
//! Stand-ins, all documented here and nowhere else:
//! - `FUN_00105638` runs with two callees replaced by `jr ra`: `FUN_00194d78` (starts the script VM; touches no save byte) and
//!   `FUN_001b6870` (the world flags' boot, `Story::world_boot`; the port's `Progress::new_game` does not run it either). The starting gear
//!   (`FUN_001c0cb0(0)`, inside `FUN_00187d20`) runs for real on the executable's `item` and `plrp` tables laid out in memory.
//! - `FUN_00187868` and the other record functions run on a pointer to a random record; the ones that take the party block as `g_save.party`
//!   have it as a randomised global.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::sync::Arc;

use kh2_ee::Ee;
use kh2_factory::fuzz::*;
use kh2_game_core::progress::{ItemIndex, Progress};
use kh2_game_core::progress_flags::{FLAGS_ADDR, FLAGS_LEN, flag_8d0};
use kh2_game_core::save_block::{self as sb, BootEnv, LEN, SaveBlock, at, party};
use kh2_tables::tables::{self as t, Cmd, Item, Magc, Plrp};

const SAVE: i32 = 0x33e860;
const PARTY: i32 = 0x33fec0;
const SAVE_GLOBAL: (u32, u32, bool) = (SAVE as u32, LEN as u32, true);
const PARTY_GLOBAL: (u32, u32, bool) = (PARTY as u32, party::LEN as u32, true);
const FIXUPS: u32 = 0x37c7d8;
const REGION: u32 = 0x349d44;

/// Runs one function that takes a pointer to a struct of `size` bytes (random bytes before the call) over 1,000 cases.
fn struct_case(addr: u32, size: u32, mut port: impl FnMut(&mut [u8])) {
    let args: &'static [Arg] = Box::leak(Box::new([Arg::Ptr(Block { size, fields: &[], subs: &[] })]));
    let spec = Spec { addr, args, floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&spec, 1000, |c| {
        port(c.block(0));
        Out::none()
    }) else {
        return;
    };
    eprintln!("{addr:#x}: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_1875c8_matches_the_game() {
    struct_case(0x1875c8, 0x100, sb::init_member);
}

#[test]
fn orig_187ed0_matches_the_game() {
    struct_case(0x187ed0, 0x40, sb::init_form);
}

#[test]
fn orig_1a71a8_matches_the_game() {
    struct_case(0x1a71a8, 0x10, sb::init_world_slot);
}

#[test]
fn orig_1d7640_matches_the_game() {
    struct_case(0x1d7640, 0x40, sb::clear_report_words);
}

#[test]
fn orig_32c8f8_matches_the_game() {
    struct_case(0x32c8f8, 0x20, sb::clear_four_words);
}

#[test]
fn orig_114318_matches_the_game() {
    struct_case(0x114318, 0x40, sb::clear_member_settings);
}

#[test]
fn orig_32c7c8_matches_the_game() {
    struct_case(0x32c7c8, 0x1700, sb::clear_story_tables);
}

#[test]
fn orig_32c550_matches_the_game() {
    struct_case(0x32c550, 0x200, sb::clear_world_records);
}

#[test]
fn orig_2c5828_matches_the_game() {
    struct_case(0x2c5828, 0x200, sb::mark_world_records);
}

#[test]
fn orig_32c610_matches_the_game() {
    struct_case(0x32c610, 0xa0, sb::clear_new_seen);
}

#[test]
fn orig_32c690_matches_the_game() {
    struct_case(0x32c690, 0x200, sb::clear_moogle);
}

#[test]
fn orig_32c6c8_matches_the_game() {
    struct_case(0x32c6c8, 0x140, sb::clear_journal);
}

#[test]
fn orig_2ee1c8_matches_the_game() {
    struct_case(0x2ee1c8, 0x20, sb::set_first_byte);
}

#[test]
fn orig_2ee208_matches_the_game() {
    struct_case(0x2ee208, 0x150, sb::init_settings_record);
}

#[test]
fn orig_2ee2e8_matches_the_game() {
    struct_case(0x2ee2e8, 0x7300, sb::init_config_tables);
}

#[test]
fn orig_1ddfb0_matches_the_game() {
    struct_case(0x1ddfb0, 0x700, sb::number_object_cells);
}

/// `FUN_002ee2a8` calls `FUN_002ee1b0` and `FUN_002ee1f0` without arguments of their own: the first writes the byte at its `a0`, the
/// second sets up the record at `a0 + 4`, which the port reads as the block pointer and the pointer plus four.
#[test]
fn orig_2ee2a8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x2ee2a8, args: &[Arg::Ptr(Block { size: 0x7300, fields: &[], subs: &[] })], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        sb::init_config_block(c.block(0));
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x2ee2a8: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_310008_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x310008, args: &[Arg::Ptr(Block { size: 0x300, fields: &[], subs: &[] }), Arg::Range(0, 0x2ff)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let n = c.int(1) as usize;
        sb::clear_all(&mut c.block(0)[..n]);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x310008: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// `FUN_001ddf20` reads its fix-up list from the executable (`0x37c7d8`, left as the executable has it, passed to the port as bytes).
#[test]
fn orig_1ddf20_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1ddf20, args: &[Arg::Ptr(Block { size: 0x1000, fields: &[], subs: &[] })], floats: 0, ret: Ret::None, globals: &[(FIXUPS, 0x50, false)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let table = words(c.global(0));
        sb::apply_object_fixups(c.block(0), &table);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x1ddf20: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_1ddec0_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1ddec0, args: &[Arg::Ptr(Block { size: 0x1000, fields: &[], subs: &[] })], floats: 0, ret: Ret::None, globals: &[(FIXUPS, 0x50, false)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let table = words(c.global(0));
        sb::init_object_block(c.block(0), &table);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x1ddec0: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

fn words(b: &[u8]) -> Vec<u32> {
    b.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
}

#[test]
fn orig_105758_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x105758, args: &[], floats: 0, ret: Ret::Int, globals: &[(REGION, 4, true)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let region = u32::from_le_bytes(c.global(0)[..4].try_into().unwrap());
        Out::int(sb::header_word(region) as i32 as i64)
    }) else {
        return;
    };
    eprintln!("0x105758: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_105748_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x105748, args: &[Arg::Ptr(Block { size: 0x1700, fields: &[], subs: &[] }), Arg::Int(&[0, 1, 2])], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let v = c.int(1) as u32;
        sb::set_game_mode_word(c.block(0), v);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x105748: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// Runs one function that works on the global save block (`g_save`, randomised before each of 1,000 cases).
fn save_case(addr: u32, mut port: impl FnMut(&mut [u8])) {
    let args: &'static [Arg] = &[];
    let globals: &'static [(u32, u32, bool)] = &[SAVE_GLOBAL];
    let spec = Spec { addr, args, floats: 0, ret: Ret::None, globals, stubs: &[], nulls: false };
    let Some(st) = fuzz(&spec, 1000, |c| {
        port(c.global(0));
        Out::none()
    }) else {
        return;
    };
    eprintln!("{addr:#x}: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_14c8c8_matches_the_game() {
    save_case(0x14c8c8, sb::clear_wallet_table);
}

#[test]
fn orig_23e9a0_matches_the_game() {
    save_case(0x23e9a0, sb::reset_options);
}

#[test]
fn orig_239f28_matches_the_game() {
    save_case(0x239f28, sb::reset_settings);
}

#[test]
fn orig_19d890_matches_the_game() {
    save_case(0x19d890, sb::clear_pair_table);
}

/// `FUN_00282d30` reads `g_save.party`: the party block is a randomised global.
#[test]
fn orig_282d30_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x282d30, args: &[Arg::Range(-3, 12), Arg::Range(0, 0xffff)], floats: 0, ret: Ret::None, globals: &[PARTY_GLOBAL], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 2000, |c| {
        let (form, weapon) = (c.int(0), c.int(1) as u16);
        sb::set_form_weapon(c.global(0), form, weapon);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x282d30: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// `FUN_001be7e8` reads `g_save.party` too; bit numbers from 0 to 0x2fff stay inside the block.
#[test]
fn orig_1be7e8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1be7e8, args: &[Arg::Range(0, 0x2fff)], floats: 0, ret: Ret::Int, globals: &[PARTY_GLOBAL], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let n = c.int(0) as u32;
        Out::int(sb::defeat_bit(c.global(0), n) as i64)
    }) else {
        return;
    };
    eprintln!("0x1be7e8: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// `FUN_00187b38` on the party block as a randomised global, and `FUN_00187930` (the same defaults with absolute offsets).
#[test]
fn orig_187b38_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x187b38, args: &[Arg::Range(PARTY, PARTY)], floats: 0, ret: Ret::None, globals: &[PARTY_GLOBAL], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        sb::init_party(c.global(0));
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x187b38: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_187930_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x187930, args: &[Arg::Range(PARTY, PARTY)], floats: 0, ret: Ret::None, globals: &[PARTY_GLOBAL], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        sb::init_party_unrolled(c.global(0));
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x187930: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

// --- record lookups ----------------------------------------------------------------------------------

#[test]
fn orig_1b58d8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1b58d8, args: &[Arg::Range(-3, 20)], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| Out::int(sb::member_index(c.int(0)) as i64)) else { return };
    eprintln!("0x1b58d8: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_187d40_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x187d40, args: &[Arg::Range(PARTY, PARTY), Arg::Range(-3, 20)], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| Out::int(sb::member_offset(c.int(1)).map_or(0, |o| PARTY as i64 + o as i64))) else { return };
    eprintln!("0x187d40: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_187f60_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x187f60, args: &[Arg::Range(PARTY, PARTY), Arg::Range(-3, 14)], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| Out::int(sb::form_offset(c.int(1)).map_or(0, |o| PARTY as i64 + o as i64))) else { return };
    eprintln!("0x187f60: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_187dc8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x187dc8, args: &[Arg::Range(PARTY, PARTY), Arg::Int(&[1000, 1046, 3000, 3011])], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 2000, |c| Out::int(sb::defeat_counter_offset(c.int(1)).map_or(0, |o| PARTY as i64 + o as i64))) else { return };
    eprintln!("0x187dc8: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// A field list of `N` halfwords from `base`, so the harness draws small values (empty slots) often.
const fn halves<const N: usize>(base: u32) -> [FieldSpec; N] {
    let mut a = [(0, 0, Kind::Int); N];
    let mut i = 0;
    while i < N {
        a[i] = (base + 2 * i as u32, 2, Kind::Int);
        i += 1;
    }
    a
}

#[test]
fn orig_187e08_matches_the_game() {
    // The counters are drawn small often; the flag array is a randomised global (story flag 0x8d0 is a coin toss).
    static COUNTERS: [FieldSpec; 59] = {
        let mut a = [(0, 0, Kind::Int); 59];
        let mut i = 0;
        while i < 47 {
            a[i] = (0xff0 + 4 * i as u32, 4, Kind::Int);
            i += 1;
        }
        while i < 59 {
            a[i] = (0x1170 + 4 * (i as u32 - 47), 4, Kind::Int);
            i += 1;
        }
        a
    };
    static SPEC: Spec = Spec {
        addr: 0x187e08,
        args: &[Arg::Ptr(Block { size: party::LEN as u32, fields: &COUNTERS, subs: &[] }), Arg::Int(&[1000, 1046, 3000, 3011])],
        floats: 0,
        ret: Ret::None,
        globals: &[(FLAGS_ADDR, FLAGS_LEN as u32, true)],
        stubs: &[],
        nulls: false,
    };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let id = c.int(1);
        let counting = flag_8d0(c.global(0));
        sb::count_defeat(c.block(0), id, counting);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x187e08: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_187868_matches_the_game() {
    static ABILITIES: [FieldSpec; 64] = halves(0x54);
    static SPEC: Spec = Spec { addr: 0x187868, args: &[Arg::Ptr(Block { size: 0x100, fields: &ABILITIES, subs: &[] }), Arg::Range(0, 0xffff), Arg::Range(0, 1)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let (id, eq) = (c.int(1) as u16, c.int(2) != 0);
        sb::add_ability(c.block(0), id, eq);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x187868: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_187f18_matches_the_game() {
    static ABILITIES: [FieldSpec; 16] = halves(8);
    static SPEC: Spec = Spec { addr: 0x187f18, args: &[Arg::Ptr(Block { size: 0x40, fields: &ABILITIES, subs: &[] }), Arg::Range(0, 0xffff)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let v = c.int(1) as u16;
        sb::add_form_ability(c.block(0), v);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x187f18: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_187728_matches_the_game() {
    static SLOTS: [FieldSpec; 18] = {
        let mut a = [(0x12, 1, Kind::Int); 18];
        let mut i = 0;
        while i < 8 {
            a[1 + i] = (0x34 + 2 * i as u32, 2, Kind::Int);
            a[9 + i] = (0x44 + 2 * i as u32, 2, Kind::Int);
            i += 1;
        }
        a
    };
    static SPEC: Spec = Spec { addr: 0x187728, args: &[Arg::Ptr(Block { size: 0x300, fields: &SLOTS, subs: &[] }), Arg::Range(0, 3)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let id = c.int(1) as u16;
        sb::reserve_battle_item(c.block(0), id);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x187728: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_187648_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x187648, args: &[Arg::Ptr(Block { size: 0x10, fields: &[], subs: &[] }), Arg::Range(0, 0xffff), Arg::Range(0, 0xffff)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let (a, b) = (c.int(1) as u16, c.int(2) as u16);
        sb::set_member_weapons(c.block(0), a, b);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x187648: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// `FUN_00187d88` with the party block as a pointer; a member id without a record is left out (the original writes at address 0 then).
#[test]
fn orig_187d88_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x187d88, args: &[Arg::Ptr(Block { size: party::LEN as u32, fields: &[], subs: &[] }), Arg::Range(1, 15), Arg::Range(0, 0xffff), Arg::Range(0, 0xffff)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 2000, |c| {
        let (m, a, b) = (c.int(1), c.int(2) as u16, c.int(3) as u16);
        sb::set_weapons_of_member(c.block(0), m, a, b);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x187d88: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// The two stash functions work on `g_save.party` +0x1994 (a randomised global) and a halfword the caller points at.
#[test]
fn orig_1878f8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1878f8, args: &[Arg::Ptr(Block { size: 0x10, fields: &[], subs: &[] }), Arg::Range(0, 0xffff)], floats: 0, ret: Ret::None, globals: &[(PARTY as u32 + party::HALF_1994 as u32, 2, true)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let v = c.int(1) as u16;
        let mut p = vec![0u8; party::LEN];
        p[party::HALF_1994..party::HALF_1994 + 2].copy_from_slice(&c.global(0)[..2]);
        let mut slot = u16::from_le_bytes([c.block(0)[0], c.block(0)[1]]);
        sb::stash_halfword(&mut p, &mut slot, v);
        c.block(0)[..2].copy_from_slice(&slot.to_le_bytes());
        c.global(0)[..2].copy_from_slice(&p[party::HALF_1994..party::HALF_1994 + 2]);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x1878f8: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_187910_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x187910, args: &[Arg::Ptr(Block { size: 0x10, fields: &[], subs: &[] })], floats: 0, ret: Ret::None, globals: &[(PARTY as u32 + party::HALF_1994 as u32, 2, true)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let mut p = vec![0u8; party::LEN];
        p[party::HALF_1994..party::HALF_1994 + 2].copy_from_slice(&c.global(0)[..2]);
        let mut slot = u16::from_le_bytes([c.block(0)[0], c.block(0)[1]]);
        sb::take_stashed_halfword(&mut p, &mut slot);
        c.block(0)[..2].copy_from_slice(&slot.to_le_bytes());
        c.global(0)[..2].copy_from_slice(&p[party::HALF_1994..party::HALF_1994 + 2]);
        Out::none()
    }) else {
        return;
    };
    eprintln!("0x187910: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

// --- the tables the starting gear and the item rules read -------------------------------------------

const STACK: u32 = 0x01ff_0000;
const ITEM_PTR: u32 = 0x01c9_5588;
const PLRP_PTR: u32 = 0x01d5_df5c;
const CMD_PTR: u32 = 0x01d5_d798;
const MAGC_PTR: u32 = 0x01c9_5520;

struct Tables {
    items: Vec<Item>,
    cmds: Vec<Cmd>,
    magc: Vec<Magc>,
    plrp: Vec<Plrp>,
    ee: Ee,
}

impl Tables {
    fn index(&self) -> Arc<ItemIndex> {
        Arc::new(ItemIndex::new(&self.items, &self.cmds, &self.magc))
    }
}

/// The disc's tables decoded for the port and laid out in a kh2-ee machine for the original, or `None` after a skip.
fn tables(what: &str) -> Option<Tables> {
    let mut a = kh2_factory::disc(what)?;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    let items = kh2_tables::load::<Item>(&mut a, &t::ITEM).ok()?.rows;
    let cmds = kh2_tables::load::<Cmd>(&mut a, &t::CMD).ok()?.rows;
    let magc = kh2_tables::load::<Magc>(&mut a, &t::MAGC).ok()?.rows;
    let plrp = kh2_tables::load::<Plrp>(&mut a, &t::PLRP).ok()?.rows;
    let blob = |a: &mut kh2_archive::Archive, d: &kh2_tables::TableDef| kh2_tables::subfile(a, d).expect("table");
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    for (ptr, at, data) in [
        (ITEM_PTR, 0x0180_0000, blob(&mut a, &t::ITEM)),
        (PLRP_PTR, 0x0181_0000, blob(&mut a, &t::PLRP)),
        (CMD_PTR, 0x0182_0000, blob(&mut a, &t::CMD)),
        (MAGC_PTR, 0x0183_0000, blob(&mut a, &t::MAGC)),
    ] {
        ee.write(at, &data);
        ee.w32(ptr, at);
    }
    Some(Tables { items, cmds, magc, plrp, ee })
}

fn read(ee: &mut Ee, at: u32, n: usize) -> Vec<u8> {
    (0..n as u32).map(|o| ee.r8(at + o)).collect()
}

fn first_diff(a: &[u8], b: &[u8]) -> String {
    a.iter().zip(b).enumerate().filter(|(_, (x, y))| x != y).map(|(o, (x, y))| format!("+{o:#x}: game {x:#x} port {y:#x}")).take(8).collect::<Vec<_>>().join("; ")
}

/// The whole new-game save block. The original runs on a save block of random bytes, a random region word and a random fix-up list
/// (offsets 1..=0x3f, deltas any byte, ended by an offset of 0 after 0 to 9 pairs); the port builds the block from the same inputs.
#[test]
fn orig_105638_matches_the_game() {
    let Some(mut tb) = tables("oracle test of 0x105638") else { return };
    let ix = tb.index();
    let ee = &mut tb.ee;
    ee.hooks.insert(0x194d78, |e| e.gpr[kh2_ee::reg::V0] = 0);
    ee.hooks.insert(0x1b6870, |e| e.gpr[kh2_ee::reg::V0] = 0);
    let mut rng = Rng::new(0x105638);
    let mut steps = 0;
    for case in 0..40 {
        let junk: Vec<u8> = (0..LEN).map(|_| rng.word() as u8).collect();
        ee.write(SAVE as u32, &junk);
        let region = [sb::REGION_USA, 0, 1, 2, 3][rng.below(5) as usize];
        ee.w32(REGION, region);
        let pairs = rng.below(10) as usize;
        let mut table = vec![0u32; 20];
        for k in 0..pairs {
            table[2 * k] = 1 + rng.below(0x3f) as u32;
            table[2 * k + 1] = rng.word() as u8 as u32;
        }
        for (i, w) in table.iter().enumerate() {
            ee.w32(FIXUPS + 4 * i as u32, *w);
        }
        let s0 = ee.steps;
        ee.call(0x105638, &[SAVE as u64], &[], STACK).unwrap_or_else(|f| panic!("case {case}: the original faulted: {f:?}"));
        steps += ee.steps - s0;
        let game = read(ee, SAVE as u32, LEN);
        let port = Progress::new_game_with(ix.clone(), &tb.plrp, &BootEnv { region, fixups: table.clone() });
        let image = port.save_image();
        assert!(game == image, "case {case} (region {region}, {pairs} fix-up pairs): the save block differs: {}", first_diff(&game, &image));
        // The SaveBlock of the port built without Progress agrees in every byte the starting gear does not touch.
        let mut bare = SaveBlock::new();
        bare.init_save(&BootEnv { region, fixups: table }, |_| {});
        assert_eq!(&bare.bytes()[..at::PARTY], &game[..at::PARTY], "case {case}: bytes before the party block");
        assert_eq!(&bare.bytes()[at::WORLD_RECORDS..], &game[at::WORLD_RECORDS..], "case {case}: bytes after the party block");
    }
    eprintln!("0x105638: 40 whole-block cases equal, {steps} interpreter steps");
}

/// A random party block with plausible content for the item-slot rules: record fields random, item ids drawn from the table (or 0), the
/// stock counts small numbers so the "no more than 99" refusal and the empty stock both happen.
fn plausible_party(rng: &mut Rng, ids: &[u16]) -> Vec<u8> {
    let mut b: Vec<u8> = (0..party::LEN).map(|_| rng.word() as u8).collect();
    for m in 0..party::MEMBERS {
        let r = m * party::MEMBER_SIZE;
        b[r + 0x12] = rng.below(9) as u8;
        for i in 0..8 {
            for base in [0x34usize, 0x44] {
                let v = if rng.below(3) == 0 { 0 } else { ids[rng.below(ids.len() as u64) as usize] };
                b[r + base + 2 * i..r + base + 2 * i + 2].copy_from_slice(&v.to_le_bytes());
            }
        }
    }
    for o in 0..party::COUNT_SLOTS {
        b[party::COUNTS + o] = [0, 0, 1, 2, 98, 99, 100, 255][rng.below(8) as usize];
    }
    b
}

/// `FUN_00187658` (a member's number of battle item slots; a shrinking list gives its items back to the stock through `FUN_001a18d8`)
/// and `FUN_001877c8` (empty slots that reserve an item take one from the stock through `FUN_001a1cc8` and `FUN_001a1c30`) on random
/// party blocks, member ids and slot counts.
fn item_slot_rule(what: &str, addr: u32, seed: u64, run: impl Fn(&mut Progress, u16, u8)) {
    let Some(mut tb) = tables(what) else { return };
    let ix = tb.index();
    let ids: Vec<u16> = ix.rows.iter().map(|r| r.id).collect();
    let ee = &mut tb.ee;
    let mut rng = Rng::new(seed);
    for case in 0..1500 {
        let bytes = plausible_party(&mut rng, &ids);
        ee.write(PARTY as u32, &bytes);
        let mut port = Progress::from_block(&bytes, ix.clone());
        let member = 1 + rng.below(13) as u16;
        let n = rng.below(9) as u8;
        let rec = PARTY as u32 + (member as u32 - 1) * party::MEMBER_SIZE as u32;
        ee.call(addr, &[rec as u64, n as u64], &[], STACK).unwrap_or_else(|f| panic!("case {case}: {f:?}"));
        run(&mut port, member, n);
        let game = read(ee, PARTY as u32, party::LEN);
        assert!(game == port.block(), "case {case} (member {member}, {n} slots): {}", first_diff(&game, port.block()));
    }
    eprintln!("{addr:#x}: 1500 cases equal");
}

#[test]
fn orig_187658_matches_the_game() {
    item_slot_rule("oracle test of 0x187658", 0x187658, 0x187658, |p, member, n| {
        assert!(p.set_battle_item_slots(member, n));
    });
}

#[test]
fn orig_1877c8_matches_the_game() {
    item_slot_rule("oracle test of 0x1877c8", 0x1877c8, 0x1877c8, |p, member, _| p.refill_battle_items(member));
}

// --- the layout object on a real state ---------------------------------------------------------------

/// The save block of the user's state `.06` (a PCSX2 state read by `kh2 state from-p2s`'s reader) goes through the layout object bit for
/// bit: `SaveBlock` gives its bytes back, its accessors read what the raw bytes hold, and `Progress` loaded from the same image gives the
/// image back through `save_image` (the option word aside: the app keeps its own default, as it did before the migration). Skipped
/// without the state file.
#[test]
fn the_users_state_round_trips_through_the_layout_object() {
    let dir = std::env::var_os("KH2_SSTATES").map(std::path::PathBuf::from).or_else(|| std::env::var_os("USERPROFILE").map(|u| std::path::PathBuf::from(u).join("Documents/PCSX2/sstates")));
    let Some(path) = dir.map(|d| d.join("SLUS-21005 (DA0535FD).06.p2s")).filter(|p| p.exists()) else {
        eprintln!("skipped: no PCSX2 state .06 (set KH2_SSTATES)");
        return;
    };
    let state = kh2_rig::p2s::P2s::open(&path).expect("reads the state");
    let image = &state.ee[SAVE as usize..][..LEN];
    let u32_at = |a: usize| u32::from_le_bytes(image[a..a + 4].try_into().unwrap());

    let block = SaveBlock::from_bytes(image);
    assert_eq!(block.bytes(), image, "the layout object holds the image");
    assert_eq!(block.party(), &image[at::PARTY..at::PARTY + party::LEN]);
    assert_eq!((block.magic(), block.wallet(), block.options(), block.game_mode()), (u32_at(at::MAGIC), u32_at(at::WALLET), u32_at(at::OPTIONS), u32_at(at::GAME_MODE)));
    assert_eq!(block.shop_seen(), std::array::from_fn(|i| u32_at(at::SHOP_SEEN + 4 * i)));
    assert_eq!(block.magic(), sb::header_word(sb::REGION_USA), "a USA save carries the USA header word");
    let mut again = SaveBlock::new();
    again.bytes_mut().copy_from_slice(image);
    assert_eq!(again, block);

    let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
    p.apply_json(&kh2_rig::save::rig_json(&state.ee, "state.06.p2s", None).unwrap()).expect("the rig file loads");
    let mut expected = image.to_vec();
    // The app keeps its default option word and clamps the munny to its limit; everything else comes back.
    expected[at::OPTIONS..at::OPTIONS + 4].copy_from_slice(&p.option_word().to_le_bytes());
    expected[at::WALLET..at::WALLET + 4].copy_from_slice(&p.wallet().to_le_bytes());
    expected[at::GAME_MODE..at::GAME_MODE + 4].copy_from_slice(&p.game_mode().to_le_bytes());
    let got = p.save_image();
    assert!(got == expected, "the image Progress gives back differs from the state: {}", first_diff(&expected, &got));
    assert_eq!(p.block(), block.party());
}
