//! Oracle tests of the save file layer: the CRC, the save body checks, export and import, the sizes, the slot table of the card info block
//! and `icon.sys`. kh2-ee runs the original from the user's executable on random buffers (see `common/mod.rs`) and the port runs on
//! copies of the same bytes; every buffer the function may touch is compared.
//!
//! Stand-ins: `FUN_0019e178` (the place id of an area, a table lookup the port does not own) returns 0 in `FUN_0023d370`, and the port's
//! place function returns 0. Everything else runs for real. Skips without a disc; fails without one when `KH2_REQUIRE_DISC=1`.

mod common;

use common::*;

/// Starts kh2-ee (`Ee`) on the user's executable with the unit's pointer globals set; `None` without a disc.
fn start(what: &str, seed: u64) -> Option<Orig> {
    let o = Orig::new(what, seed)?;
    let _: &kh2_ee::Ee = &o.ee;
    Some(o)
}

use kh2_save::body::{self, BODY_LEN};
use kh2_save::crc::{self, Crc};
use kh2_save::icon;
use kh2_save::slot::{CardInfo, INFO_LEN, SLOTS};

const CASES: u32 = 1000;

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

#[test]
fn orig_23e4c0_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e4c0", 1) else { return };
    for _ in 0..3 {
        o.fill(TABLE, 0x400);
        o.call(0x23e4c0, &[]);
        let game = o.get(TABLE, 0x400);
        let mine: Vec<u8> = crc::build_table().iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(game, mine, "the table of the original and the port differ (255 rounds, not 8)");
    }
    // The common CRC-32 table differs from this one at its first nonzero word.
    assert_ne!(crc::build_table()[1], 0x7707_3096, "the table is not the common CRC-32 table");
}

#[test]
fn orig_23e528_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e528", 2) else { return };
    for _ in 0..CASES {
        let s = o.rng.word() as u32;
        o.set_word(0x36cb40, s);
        o.call(0x23e528, &[]);
        let mut c = Crc { state: s };
        c.reset();
        assert_eq!(o.word(0x36cb40), c.state);
    }
}

#[test]
fn orig_23e538_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e538", 3) else { return };
    o.build_crc_table();
    for n in 0..CASES {
        let state = o.rng.word() as u32;
        let len1 = o.rng.below(300) as usize;
        let len2 = o.rng.below(300) as usize;
        let data = o.fill(FILEBUF, 600);
        o.set_word(0x36cb40, state);
        let mut c = Crc { state };
        // Two chained calls, the second continuing where the first stopped.
        let r1 = o.call(0x23e538, &[FILEBUF as u64, len1 as u64]) as u32;
        let m1 = c.update(&data[..len1]);
        assert_eq!(r1, m1, "case {n}: first return");
        let r2 = o.call(0x23e538, &[(FILEBUF + 300) as u64, len2 as u64]) as u32;
        let m2 = c.update(&data[300..300 + len2]);
        assert_eq!(r2, m2, "case {n}: second return");
        assert_eq!(o.word(0x36cb40), c.state, "case {n}: state");
    }
}

/// A random body whose header is right, wrong in one field, or random.
fn random_body(o: &mut Orig, region: u32) -> Vec<u8> {
    let mut b = o.random(BODY_LEN);
    let mode = o.rng.below(4);
    if mode < 3 {
        b[..4].copy_from_slice(&kh2_game_core::save_block::header_word(region).to_le_bytes());
        b[4..8].copy_from_slice(&0x2du32.to_le_bytes());
        let c = body::body_crc(&b);
        b[8..12].copy_from_slice(&c.to_le_bytes());
        match mode {
            1 => b[o.rng.below(4) as usize] ^= 1 << o.rng.below(8),
            2 => {
                let k = [4usize, 5, 8, 9, 10, 11][o.rng.below(6) as usize];
                b[k] ^= 1 << o.rng.below(8);
                if k >= 8 && o.rng.below(2) == 0 {
                    // Only a byte covered by the CRC changes: the CRC must then differ.
                    b[0x100] ^= 0x10;
                }
            }
            _ => {}
        }
    }
    b
}

#[test]
fn orig_23d180_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d180", 4) else { return };
    o.build_crc_table();
    for n in 0..CASES {
        let b = o.random(BODY_LEN);
        o.put(FILEBUF, &b);
        o.put(OTHER, &b);
        let s = o.rng.word() as u32;
        o.set_word(0x36cb40, s);
        let r = o.call(0x23d180, &[if n % 2 == 0 { 0 } else { OTHER as u64 }]) as u32;
        assert_eq!(r, body::body_crc(&b), "case {n}");
    }
}

#[test]
fn orig_23d1c8_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d1c8", 5) else { return };
    o.build_crc_table();
    let mut valid = 0;
    for n in 0..CASES {
        let region = o.rng.below(5) as u32;
        o.set_word(0x349d44, region);
        let b = random_body(&mut o, region);
        o.put(FILEBUF, &b);
        o.put(OTHER, &b);
        let r = o.call(0x23d1c8, &[if n % 2 == 0 { 0 } else { OTHER as u64 }]) as u32 & 0xff;
        let m = body::is_valid(&b, region);
        valid += u32::from(m);
        assert_eq!(r != 0, m, "case {n}: region {region}");
    }
    assert!(valid > 100 && valid < CASES - 100, "the cases cover both outcomes: {valid} valid");
}

#[test]
fn orig_23cd38_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23cd38", 6) else { return };
    for _ in 0..CASES {
        let n = o.rng.int(&[0, 1, 0x3ff, 0x400, 0xb4e0]);
        let r = o.call(0x23cd38, &[n as i64 as u64]) as u32;
        assert_eq!(r, body::kb(n), "n {n:#x}");
    }
}

#[test]
fn orig_23d120_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d120", 7) else { return };
    assert_eq!(o.call(0x23d120, &[]) as u32 as i32, body::space_needed());
    assert_eq!(body::space_needed(), 90);
}

#[test]
fn orig_23dd60_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23dd60", 8) else { return };
    let mut kinds: Vec<i64> = vec![-1, 0, 1, 2, 3, 100, i64::MIN, i64::MAX];
    for _ in 0..200 {
        kinds.push(o.rng.word() as i64);
    }
    for k in kinds {
        assert_eq!(o.call(0x23dd60, &[k as u64]) as u32, body::file_size(k), "kind {k}");
    }
}

#[test]
fn orig_23d230_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d230", 9) else { return };
    o.build_crc_table();
    let pairs = o.variants();
    let (mut done, mut alt) = (0, 0);
    for n in 0..CASES {
        let mut save = o.random(BODY_LEN);
        // The seed (play time) is not negative: the original indexes its candidate list with it.
        save[0x1607] &= 0x7f;
        if n % 3 == 0 {
            // World 4 and the flag of the alternate name.
            save[0xc] = 4;
            let w = 0xe50 + (0x108b >> 10) * 0x20 + ((0x108b & 0x3ff) >> 5) * 4;
            save[w + ((0x108b & 31) >> 3)] |= 1 << (0x108b & 7);
        }
        o.put(SAVE, &save);
        let buf = o.fill(FILEBUF, BODY_LEN);
        o.put(OTHER, &buf);
        let arg = if n % 2 == 0 { 0 } else { OTHER as u64 };
        let target = if n % 2 == 0 { FILEBUF } else { OTHER };
        if o.ee.call(0x23d230, &[arg], &[], kh2_factory::fuzz::STACK).is_err() {
            // No candidate: the original traps.
            o.ee.cop0[12] &= !2;
            assert!(body::export(&save, &pairs).is_none(), "case {n}: the port finds a variant where the original traps");
            continue;
        }
        let game = o.get(target, BODY_LEN);
        let mine = body::export(&save, &pairs).unwrap_or_else(|| panic!("case {n}: the port finds no variant"));
        assert_eq!(game, mine, "case {n}");
        assert_eq!(o.get(SAVE, BODY_LEN), save, "case {n}: the live block is unchanged");
        done += 1;
        alt += (mine[0x31a9] & 1) as u32;
    }
    assert!(done > 800 && alt > 50, "{done} exported, {alt} with the alternate name");
}

#[test]
fn orig_23d2b8_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d2b8", 10) else { return };
    for n in 0..200 {
        let save = o.fill(SAVE, BODY_LEN);
        let b = o.fill(FILEBUF, BODY_LEN);
        let c = o.fill(OTHER, BODY_LEN);
        let (arg, src) = if n % 2 == 0 { (0, b) } else { (OTHER as u64, c) };
        o.call(0x23d2b8, &[arg]);
        let mut mine = save;
        body::import(&mut mine, &src);
        assert_eq!(o.get(SAVE, BODY_LEN), mine, "case {n}");
    }
}

#[test]
fn orig_23e318_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e318", 11) else { return };
    let mut set = 0;
    for n in 0..CASES {
        let mut save = o.random(BODY_LEN);
        if n % 2 == 0 {
            let w = 0xe50 + (0x108b >> 10) * 0x20 + ((0x108b & 0x3ff) >> 5) * 4;
            save[w + ((0x108b & 31) >> 3)] |= 1 << (0x108b & 7);
        }
        o.put(SAVE, &save);
        let world = o.rng.int(&[4]);
        let r = o.call(0x23e318, &[world as i64 as u64]) as u32 & 0xff;
        let m = body::alt_world_name(world, &save[0xe50..]);
        set += u32::from(m);
        assert_eq!(r != 0, m, "case {n}: world {world}");
    }
    assert!(set > 20);
}

#[test]
fn orig_23e258_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e258", 12) else { return };
    let pairs = o.variants();
    let mut seen = [0u32; 8];
    for n in 0..CASES {
        let mut save = o.random(BODY_LEN);
        save[0x1607] &= 0x7f;
        o.put(SAVE, &save);
        let seed = u32_at(&save, 0x1604) as i32;
        let Ok(r) = o.ee.call(0x23e258, &[], &[], kh2_factory::fuzz::STACK) else {
            o.ee.cop0[12] &= !2;
            assert!(body::icon_variant(&pairs, &save[0xe50..], seed).is_none(), "case {n}");
            continue;
        };
        let m = body::icon_variant(&pairs, &save[0xe50..], seed).unwrap_or_else(|| panic!("case {n}: no variant"));
        assert_eq!(r as u32, u32::from(m), "case {n}");
        seen[m as usize] += 1;
    }
    assert!(seen.iter().filter(|&&c| c > 0).count() >= 3, "several variants appear: {seen:?}");
}

#[test]
fn orig_23dcf8_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23dcf8", 13) else { return };
    o.build_crc_table();
    let mut same = 0;
    for n in 0..300 {
        let a = o.fill(FILEBUF, body::ICON_LEN);
        let mut b = if n % 2 == 0 { a.clone() } else { o.random(body::ICON_LEN) };
        if n % 4 == 0 {
            b[o.rng.below(body::ICON_LEN as u64) as usize] ^= 1;
        }
        o.put(OTHER, &b);
        let r = o.call(0x23dcf8, &[OTHER as u64]) as u32 & 0xff;
        let m = body::icon_matches(&b, &a);
        same += u32::from(m);
        assert_eq!(r != 0, m, "case {n}");
    }
    assert!(same > 50);
}

#[test]
fn orig_23e598_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e598", 14) else { return };
    for _ in 0..100 {
        let save = o.fill(SAVE, BODY_LEN);
        let v = o.rng.word() as u32;
        o.call(0x23e598, &[v as u64]);
        let mut mine = save;
        body::set_munny(&mut mine, v);
        assert_eq!(o.get(SAVE, BODY_LEN), mine);
    }
}

// --- the slot table ---------------------------------------------------------------------------------------------------------

/// A random card info block: random flag words from the bits the functions test, random times, the cursor and free space in range.
fn random_info(o: &mut Orig) -> CardInfo {
    let mut b = o.random(INFO_LEN);
    for s in 0..SLOTS {
        let flags = [0u32, 1, 2, 3, 0x10, 0x11, 0x13, 0x20, 0x40, 0x30, 0x12, o.rng.word() as u32][o.rng.below(12) as usize];
        let at = 8 + s * 0x20;
        b[at..at + 4].copy_from_slice(&flags.to_le_bytes());
        if o.rng.below(3) == 0 {
            // Equal times to the minute: the order is decided by the later fields, or the tie keeps the earlier slot.
            let t = [0u8, 5, 6, 7, 8, 9, 0xea, 0x07];
            b[at + 0x18..at + 0x20].copy_from_slice(&t);
            if o.rng.below(2) == 0 {
                b[at + 0x18 + 1 + o.rng.below(5) as usize] = o.rng.below(3) as u8 + 5;
            }
        }
    }
    b[1] = if o.rng.below(4) == 0 { o.rng.word() as u8 & 0x7f } else { o.rng.below(99) as u8 };
    let free = [-2i16, -1, 0, 5, 89, 90, 91, 200, o.rng.word() as i16][o.rng.below(9) as usize];
    b[2..4].copy_from_slice(&free.to_le_bytes());
    CardInfo(b)
}

fn place_zero(_: u8, _: u8) -> u32 {
    0
}

#[test]
fn orig_23d370_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d370", 20) else { return };
    o.stub(0x19e178);
    for n in 0..CASES {
        let info = random_info(&mut o);
        o.put(INFO, &info.0);
        let body = o.fill(FILEBUF, BODY_LEN);
        let other = o.fill(OTHER, BODY_LEN);
        let slot = o.rng.below(SLOTS as u64) as usize;
        let bits = [0u32, 1, 2, 3, 0x10, 0x11, 0x13, 0x40, 0x41, 0x50, o.rng.word() as u32 & 0x7f][o.rng.below(11) as usize];
        let (arg, src) = if n % 2 == 0 { (0, body) } else { (OTHER as u64, other) };
        o.call(0x23d370, &[slot as u64, arg, bits as u64]);
        let mut mine = info;
        mine.describe(slot, &src, bits, &place_zero);
        assert_eq!(o.get(INFO, INFO_LEN), mine.0, "case {n}: slot {slot} bits {bits:#x}");
    }
}

#[test]
fn orig_23dd80_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23dd80", 21) else { return };
    o.stub(0x19e178);
    for n in 0..CASES {
        let info = random_info(&mut o);
        o.put(INFO, &info.0);
        let body = o.fill(FILEBUF, BODY_LEN);
        let slot: i8 = if o.rng.below(3) == 0 { -(o.rng.below(100) as i8) - 1 } else { o.rng.below(99) as i8 };
        // A negative slot is the cursor (a byte below 99 here).
        let mut info = info;
        if slot < 0 {
            info.0[1] = o.rng.below(99) as u8;
            o.put(INFO, &info.0);
        }
        o.call(0x23dd80, &[slot as i64 as u64]);
        info.describe_saved(slot, &body, &place_zero);
        assert_eq!(o.get(INFO, INFO_LEN), info.0, "case {n}: slot {slot}");
    }
}

#[test]
fn orig_23d520_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d520", 22) else { return };
    for _ in 0..300 {
        let mut info = random_info(&mut o);
        o.put(INFO, &info.0);
        let slot = o.rng.below(SLOTS as u64) as usize;
        o.call(0x23d520, &[slot as u64]);
        info.clear(slot);
        assert_eq!(o.get(INFO, INFO_LEN), info.0);
    }
}

#[test]
fn orig_23d698_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d698", 23) else { return };
    for _ in 0..CASES {
        let mut info = random_info(&mut o);
        let slot: i64 = if o.rng.below(3) == 0 { -1 - o.rng.below(5) as i64 } else { o.rng.below(99) as i64 };
        if slot < 0 {
            // A negative slot is the cursor.
            info.0[1] = o.rng.below(99) as u8;
        }
        o.put(INFO, &info.0);
        let bits = o.rng.word() as u32;
        o.call(0x23d698, &[slot as u64, bits as u64]);
        info.or_bits(slot, bits);
        assert_eq!(o.get(INFO, INFO_LEN), info.0);
    }
}

#[test]
fn orig_23de68_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23de68", 24) else { return };
    for n in 0..CASES {
        let mut info = random_info(&mut o);
        o.put(INFO, &info.0);
        let table = o.fill(DIRTAB, 0x40);
        let slot = o.rng.below(98) as usize;
        let clear = n % 2 == 0;
        o.call(0x23de68, &[slot as u64, u64::from(clear)]);
        info.set_time(slot, clear, &table);
        assert_eq!(o.get(INFO, INFO_LEN), info.0, "case {n}");
    }
}

#[test]
fn orig_23ded0_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23ded0", 25) else { return };
    let mut non_zero = 0;
    for n in 0..CASES {
        let info = random_info(&mut o);
        o.put(INFO, &info.0);
        let r = o.call(0x23ded0, &[]) as u32 as usize;
        assert_eq!(r, info.latest(), "case {n}");
        non_zero += usize::from(r != 0);
    }
    assert!(non_zero > 300);
}

#[test]
fn orig_23da78_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23da78", 26) else { return };
    for n in 0..CASES {
        let info = random_info(&mut o);
        o.put(INFO, &info.0);
        let r = o.call(0x23da78, &[]) as i64;
        assert_eq!(r as i32, info.next_free_dir(), "case {n}: cursor {}", info.0[1]);
    }
}

#[test]
fn orig_23dde0_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23dde0", 27) else { return };
    for n in 0..CASES {
        let mut info = random_info(&mut o);
        o.put(INFO, &info.0);
        o.call(0x23dde0, &[]);
        info.mark_no_space();
        assert_eq!(o.get(INFO, INFO_LEN), info.0, "case {n}");
    }
}

#[test]
fn orig_23ddb0_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23ddb0", 28) else { return };
    for n in 0..CASES {
        let mut info = random_info(&mut o);
        info.0[1] = o.rng.below(99) as u8;
        o.put(INFO, &info.0);
        let slot: i64 = if n % 3 == 0 { -1 } else { o.rng.below(99) as i64 };
        let r = o.call(0x23ddb0, &[slot as u64]) as u32;
        assert_eq!(r != 0, info.is_valid(slot), "case {n}");
        assert!(r <= 1);
    }
}

#[test]
fn orig_23e060_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e060", 29) else { return };
    for n in 0..CASES {
        let info = random_info(&mut o);
        o.put(INFO, &info.0);
        let slot = o.rng.below(99) as usize;
        assert_eq!(o.call(0x23e060, &[slot as u64]) as u32 != 0, info.is_empty_writable(slot), "case {n}");
    }
}

#[test]
fn orig_23e018_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e018", 32) else { return };
    let mut any = 0;
    for n in 0..CASES {
        let mut info = random_info(&mut o);
        if n % 3 == 0 {
            for s in 0..SLOTS {
                info.clear(s);
            }
        }
        o.put(INFO, &info.0);
        let m = info.any_valid();
        any += usize::from(m);
        assert_eq!(o.call(0x23e018, &[]) as u32 != 0, m, "case {n}");
    }
    assert!(any > 100);
}

#[test]
fn orig_23d348_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d348", 30) else { return };
    for _ in 0..100 {
        let mut info = random_info(&mut o);
        o.put(INFO, &info.0);
        o.call(0x23d348, &[]);
        info.clear_records();
        assert_eq!(o.get(INFO, INFO_LEN), info.0);
    }
}

/// One accessor of the card info block against the port, over random blocks and values.
fn accessor_case(addr: u32, run: fn(&mut Orig, &mut CardInfo, u64)) {
    let Some(mut o) = start(&format!("oracle test of {addr:#x}"), u64::from(addr)) else { return };
    for n in 0..200 {
        let mut info = random_info(&mut o);
        o.put(INFO, &info.0);
        let v = o.rng.word();
        run(&mut o, &mut info, v);
        same_bytes(&o.get(INFO, INFO_LEN), &info.0, &format!("case {n}"));
    }
}

#[test]
fn orig_23d2f8_matches_the_game() {
    accessor_case(0x23d2f8, |o, i, v| {
        o.call(0x23d2f8, &[v & 0xff]);
        i.set_port(v as u8);
    });
}

#[test]
fn orig_23d308_matches_the_game() {
    accessor_case(0x23d308, |o, i, _| assert_eq!(o.call(0x23d308, &[]) as u8, i.port()));
}

#[test]
fn orig_23d318_matches_the_game() {
    accessor_case(0x23d318, |o, i, v| {
        o.call(0x23d318, &[v & 0xffff]);
        i.set_free_kb(v as u16);
    });
}

#[test]
fn orig_23d328_matches_the_game() {
    accessor_case(0x23d328, |o, i, v| {
        o.call(0x23d328, &[v & 0xff]);
        i.set_cursor(v as u8);
    });
}

#[test]
fn orig_23d338_matches_the_game() {
    accessor_case(0x23d338, |o, i, _| assert_eq!(o.call(0x23d338, &[]) as u8, i.cursor()));
}

#[test]
fn orig_23dfe8_matches_the_game() {
    accessor_case(0x23dfe8, |o, i, v| {
        o.call(0x23dfe8, &[v & 1]);
        i.set_flag4(v & 1 != 0);
    });
}

#[test]
fn orig_23e000_matches_the_game() {
    accessor_case(0x23e000, |o, i, _| assert_eq!(o.call(0x23e000, &[]) as u32 & 0xff != 0, i.flag4()));
}

/// The buffer getters return the unit's own buffers (their addresses are the pointer globals the tests set): the port has no buffers of
/// its own to hand out, so the test checks that the original returns the pointer it was given.
#[test]
fn orig_23e350_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e350", 33) else { return };
    assert_eq!(o.call(0x23e350, &[]) as u32, INFO);
}

#[test]
fn orig_23dd50_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23dd50", 34) else { return };
    assert_eq!(o.call(0x23dd50, &[]) as u32, FILEBUF);
}

#[test]
fn orig_23cd28_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23cd28", 35) else { return };
    assert_eq!(o.call(0x23cd28, &[]) as u32, NAME);
}

// --- icon.sys ---------------------------------------------------------------------------------------------------------------

fn random_live(o: &mut Orig) -> Vec<u8> {
    let mut save = o.random(BODY_LEN);
    // The game mode indexes the four marks; the player level and play time stay random.
    save[0x1658..0x165c].copy_from_slice(&(o.rng.below(4) as u32).to_le_bytes());
    if o.rng.below(2) == 0 {
        // Real play times: up to a few hundred hours at 60 Hz.
        let t = o.rng.below(216_000 * 300) as u32;
        save[0x1604..0x1608].copy_from_slice(&t.to_le_bytes());
    }
    save
}

#[test]
fn orig_23ce40_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23ce40", 40) else { return };
    let env = o.icon_env();
    for n in 0..CASES {
        let save = random_live(&mut o);
        o.put(SAVE, &save);
        let buf = o.fill(FILEBUF, 0x44);
        let num = o.rng.below(120) as i32 - 5;
        o.call(0x23ce40, &[FILEBUF as u64, num as i64 as u64]);
        let mut mine = buf;
        icon::write_title(&env, &mut mine, num, &save);
        same_bytes(&o.get(FILEBUF, 0x44), &mine, &format!("case {n}: number {num} ticks {}", i32::from_le_bytes(save[0x1604..0x1608].try_into().unwrap())));
    }
}

#[test]
fn orig_23cd48_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23cd48", 41) else { return };
    let env = o.icon_env();
    for n in 0..CASES {
        let save = random_live(&mut o);
        o.put(SAVE, &save);
        let _ = o.fill(FILEBUF, ICON_SYS_LEN_PLUS);
        let num = o.rng.below(99) as i32;
        let (arg, at) = if n % 2 == 0 { (0, FILEBUF) } else { (OTHER as u64, OTHER) };
        if arg != 0 {
            o.fill(OTHER, ICON_SYS_LEN_PLUS);
        }
        o.call(0x23cd48, &[arg, num as u64]);
        let mine = icon::write_icon_sys(&env, num, &save);
        same_bytes(&o.get(at, mine.len()), &mine, &format!("case {n}: slot {num}"));
    }
}

const ICON_SYS_LEN_PLUS: usize = 0x3c4 + 0x40;
