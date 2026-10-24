//! Hand tests of the start-up pieces of the file layer that the oracle cannot run (they allocate with the game's heap): the buffers
//! `FUN_0023c948` makes and the save block bit `FUN_0023c830` sets.

use kh2_game_core::progress_flags::check_flag;
use kh2_save::body::init_alt_bit;
use kh2_save::dir::{Buffers, align_table};

/// orig 0x23c948
#[test]
fn buffers_have_the_sizes_the_original_allocates() {
    let b = Buffers::default();
    assert_eq!(b.name.0.len(), 0x20);
    assert_eq!(b.path.len(), 0x12);
    assert_eq!(b.file.len(), 0xb800, "the larger of 0x8a58 and 0xb4e0 in whole KB");
    assert_eq!(b.info.0.len(), 0xc68);
    assert_eq!((b.info.0[0], b.info.0[2], b.info.0[3]), (0xff, 0xff, 0xff));
    assert_eq!(b.info.free_kb(), -1);
    assert_eq!(b.list.len(), 0xf80);
    assert_eq!(b.table.len(), 0x1900);
    assert!(b.name.0.iter().chain(&b.path).chain(&b.file).chain(&b.list).chain(&b.table).all(|&x| x == 0));
    for (a, want) in [(0x1000u32, 0x1000u32), (0x1001, 0x1040), (0x103f, 0x1040), (0x1040, 0x1040), (0x1020, 0x1040)] {
        assert_eq!(align_table(a), want, "{a:#x}");
    }
}

/// orig 0x23c830
#[test]
fn start_up_sets_bit_8_of_the_variant_word_from_the_world_name() {
    let flag_byte = 0xe50 + (0x108b >> 10) * 0x20 + ((0x108b & 0x3ff) >> 5) * 4 + ((0x108b & 31) >> 3);
    for (world, flag, old, new) in [(4u8, true, 0xabcd_ef12u32, 0xabcd_ef12 | 0x100), (4, false, 0xabcd_ef12, 0xabcd_ee12), (3, true, 0xabcd_ef12, 0xabcd_ee12), (4, true, 0, 0x100)] {
        let mut live = vec![0u8; 0xb4e0];
        live[0xc] = world;
        if flag {
            live[flag_byte] |= 1 << (0x108b & 7);
        }
        assert_eq!(check_flag(&live[0xe50..], 0x108b), flag);
        live[0x31a8..0x31ac].copy_from_slice(&old.to_le_bytes());
        init_alt_bit(&mut live);
        assert_eq!(u32::from_le_bytes(live[0x31a8..0x31ac].try_into().unwrap()), new, "world {world} flag {flag}");
    }
}
