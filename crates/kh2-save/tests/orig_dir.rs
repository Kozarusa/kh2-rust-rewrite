//! Oracle tests of the directory layer and the file names: the root scan into the slot table, the per-save file list and its checks, the
//! completeness count, and the name routines. kh2-ee runs the original on random directory tables built to hit every branch (names around
//! the 15-character pattern, the expected file names, `.` and `..`, right and wrong sizes) and the port runs on copies of the same bytes.
//!
//! The card library's listing count comes from the status table (`FUN_002319f8`, a getter of a global the tests set); string routines
//! (`FUN_0031218c`, `FUN_00311f34`, `FUN_00312078`, the formatting `FUN_00311c08`) run for real. Skips without a disc.

mod common;

use common::*;

/// Starts kh2-ee (`Ee`) on the user's executable with the unit's pointer globals set; `None` without a disc.
fn start(what: &str, seed: u64) -> Option<Orig> {
    let o = Orig::new(what, seed)?;
    let _: &kh2_ee::Ee = &o.ee;
    Some(o)
}

use kh2_save::dir::{self, LIST_LEN, NAME_LEN, NameBuf};
use kh2_save::names::{self, NameEnv};
use kh2_save::slot::{CardInfo, INFO_LEN, SLOTS};

const CASES: u32 = 1000;

/// A random directory table of 100 entries. `save_name` is the name of the save's own file.
fn random_table(o: &mut Orig, save_name: &[u8]) -> Vec<u8> {
    let mut t = o.random(100 * 0x40);
    for i in 0..100 {
        let kind = o.rng.below(10);
        let size = [0x3c4u32, 0x8a58, 0xb4e0, 0x3c3, 0x8a59, 0xb4e1, 0, o.rng.below(0x20000) as u32, 0x3c4, 0xb4e0][o.rng.below(10) as usize];
        let attr = [0x20u16, 0x20, 0, 0x10, 0x30, 0x8427][o.rng.below(6) as usize];
        let name: Vec<u8> = match kind {
            0 => b".".to_vec(),
            1 => b"..".to_vec(),
            2 => b"icon.sys".to_vec(),
            3 => b"kh2.ico".to_vec(),
            4 => save_name.to_vec(),
            5 | 6 => {
                // 15 characters with two digits where the slot parser looks.
                let mut n = b"BASLUS-21005-00".to_vec();
                let slot = o.rng.below(99) as u8;
                n[13] = b'0' + slot / 10;
                n[14] = b'0' + slot % 10;
                if o.rng.below(6) == 0 {
                    n[13] = o.rng.word() as u8 | 1;
                }
                if o.rng.below(8) == 0 {
                    n[14] = [b'/', b':', 0x3a, b'0', 0xff][o.rng.below(5) as usize];
                }
                n
            }
            7 => (0..o.rng.below(31) + 1).map(|_| (o.rng.word() as u8).max(1)).collect(),
            8 => (0..15).map(|_| (o.rng.word() as u8 % 0x5f) + 0x20).collect(),
            _ => b"BASLUS-21005-1".to_vec(),
        };
        let e = &mut t[i * 0x40..(i + 1) * 0x40];
        e[0x14..0x16].copy_from_slice(&attr.to_le_bytes());
        e[0x10..0x14].copy_from_slice(&size.to_le_bytes());
        e[0x20..0x40].fill(0);
        e[0x20..0x20 + name.len()].copy_from_slice(&name);
        // A directory name of 15 characters with a digit at index 13 and a slot outside 0..99 would make the original write beyond the
        // block (slot -1 hits the block's own header); the port ignores it, and no real listing has such a name, so the test leaves
        // those out.
        if attr & 0x20 != 0 && name.len() == 15 && name[13].wrapping_sub(0x30) < 10 {
            let slot = i32::from(name[13]) * 10 + i32::from(name[14]) - 0x210;
            if !(0..99).contains(&slot) {
                e[0x20 + 14] = b'0';
            }
        }
    }
    t
}

/// The count of the last listing sits in the status table of the card port in byte 0 of the info block.
fn set_count(o: &mut Orig, info: &mut CardInfo, count: i32) {
    info.0[0] = o.rng.below(2) as u8;
    o.set_word(0x1dc0be4 + u32::from(info.0[0]) * 4 * 0x20, count as u32);
}

fn random_info(o: &mut Orig) -> CardInfo {
    let mut b = o.random(INFO_LEN);
    for s in 0..SLOTS {
        let at = 8 + s * 0x20;
        let f = [0u32, 1, 2, 0x10, o.rng.word() as u32][o.rng.below(5) as usize];
        b[at..at + 4].copy_from_slice(&f.to_le_bytes());
    }
    CardInfo(b)
}

fn set_region(o: &mut Orig) -> NameEnv {
    let region = 1 + o.rng.below(4) as u32;
    o.set_word(0x349d44, region);
    o.name_env(region)
}

fn name_buf(o: &Orig) -> NameBuf {
    NameBuf(o.get(NAME, NAME_LEN).try_into().unwrap())
}

/// The table `FUN_0023e360(1, 0xffff)` fills at `0x1dc0dd8`: the two names and sizes, then the body size at `0x1dc0dec`.
fn expected_table(o: &Orig) -> ([Vec<u8>; 2], [u32; 3]) {
    let w = |a: u32| u32::from_le_bytes(o.get(a, 4).try_into().unwrap());
    ([o.cstr(w(0x1dc0dd8)), o.cstr(w(0x1dc0de0))], [w(0x1dc0ddc), w(0x1dc0de4), w(0x1dc0dec)])
}

fn check_expected(o: &Orig) {
    let (names, sizes) = expected_table(o);
    let mine = dir::expected_files();
    assert_eq!(names[0], mine[0].0.as_bytes());
    assert_eq!(names[1], mine[1].0.as_bytes());
    assert_eq!([sizes[0], sizes[1], sizes[2]], [mine[0].1, mine[1].1, kh2_save::body::BODY_LEN as u32]);
}

#[test]
fn orig_23e360_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e360", 56) else { return };
    // Only the arguments (1, 0xffff) fill the table; any other pair leaves it alone.
    o.fill(0x1dc0dd8, 0x18);
    let before = o.get(0x1dc0dd8, 0x18);
    for (a, b) in [(0u64, 0xffffu64), (2, 0xffff), (1, 0xfffe), (1, 0), (1, 0xffff_ffff)] {
        o.call(0x23e360, &[a, b]);
        assert_eq!(o.get(0x1dc0dd8, 0x18), before, "arguments ({a}, {b:#x}) change nothing");
    }
    o.call(0x23e360, &[1, 0xffff]);
    check_expected(&o);
}

#[test]
fn orig_23e3b8_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e3b8", 57) else { return };
    o.fill(0x1dc0dd8, 0x18);
    o.call(0x23e3b8, &[]);
    check_expected(&o);
}

#[test]
fn orig_23d578_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d578", 50) else { return };
    let mut marked = 0;
    for n in 0..CASES {
        let mut info = random_info(&mut o);
        let table = random_table(&mut o, b"x");
        let count = o.rng.below(100) as i32;
        set_count(&mut o, &mut info, count);
        o.put(INFO, &info.0);
        o.put(DIRTAB, &table);
        let r = o.call(0x23d578, &[]) as u32 as i32;
        let m = dir::scan_root(&mut info, &table, count);
        assert_eq!(r, m, "case {n}");
        same_bytes(&o.get(INFO, INFO_LEN), &info.0, &format!("case {n}: count {count}"));
        marked += (0..SLOTS).filter(|&s| info.flags(s) == 1).count();
    }
    assert!(marked > 500, "{marked} slots marked");
}

#[test]
fn orig_23e080_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23e080", 51) else { return };
    // The expected file table (`FUN_0023e360` through `FUN_0023e3b8`) is filled first, as the game's start-up does.
    o.call(0x23e3b8, &[]);
    let (mut yes, mut no) = (0, 0);
    for n in 0..CASES {
        let env = set_region(&mut o);
        let mut info = random_info(&mut o);
        info.0[1] = [0u8, 1, 5, 98, 0xff, 0xfe, 0xfd, 0xfc, 0xfb, 0x7f][o.rng.below(10) as usize];
        let count = o.rng.below(100) as i32;
        set_count(&mut o, &mut info, count);
        let mut name = NameBuf::default();
        let savename = names::name_for(&env, i64::from(info.0[1] as i8), "");
        let table = random_table(&mut o, savename.as_bytes());
        o.put(INFO, &info.0);
        o.put(DIRTAB, &table);
        let prefill = o.fill(NAME, NAME_LEN);
        name.0.copy_from_slice(&prefill);
        let exact = n % 2 == 0;
        let r = o.call(0x23e080, &[u64::from(exact)]) as u32 & 0xff != 0;
        let m = dir::files_complete(&env, &info, &mut name, &table, count, exact);
        assert_eq!(r, m, "case {n}: exact {exact}, count {count}, cursor {}", info.0[1]);
        assert_eq!(name_buf(&o), name, "case {n}: name buffer");
        if m {
            yes += 1;
        } else {
            no += 1;
        }
    }
    assert!(yes > 50 && no > 50, "{yes} complete, {no} not");
}

/// A random file list with `count` entries (kinds, sizes and names around the expected files).
fn random_list(o: &mut Orig, save_name: &[u8], count: usize) -> Vec<u8> {
    let mut l = o.random(LIST_LEN);
    l[..4].copy_from_slice(&(count as i32).to_le_bytes());
    for i in 0..count {
        let e = 8 + i * dir::LIST_ENTRY_LEN;
        let kind = [1u32, 2, 2, 2, 2, 0x12, 3, 0, 0x10][o.rng.below(9) as usize];
        let size = [0x3c4u32, 0x8a58, 0xb4e0, 0x3c3, 0x8a59, 0xb4e1, 0, o.rng.below(0x20000) as u32, 0x3c4, 0xb4e0][o.rng.below(10) as usize];
        let name: Vec<u8> = match o.rng.below(6) {
            0 => b"icon.sys".to_vec(),
            1 => b"kh2.ico".to_vec(),
            2 | 3 => save_name.to_vec(),
            4 => Vec::new(),
            _ => (0..o.rng.below(20) + 1).map(|_| (o.rng.word() as u8).max(1)).collect(),
        };
        l[e..e + 4].copy_from_slice(&kind.to_le_bytes());
        l[e + 4..e + 8].copy_from_slice(&size.to_le_bytes());
        l[e + 8..e + 8 + 0x20].fill(0);
        l[e + 8..e + 8 + name.len()].copy_from_slice(&name);
    }
    l
}

#[test]
fn orig_23d820_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d820", 52) else { return };
    let mut seen = std::collections::BTreeSet::new();
    for n in 0..CASES {
        let savename = b"BASLUS-21005-07";
        o.put(NAME, &[0u8; NAME_LEN]);
        o.put(NAME, savename);
        let count = o.rng.below(12) as usize;
        let mut list = random_list(&mut o, savename, count);
        o.put(LIST, &list);
        let r = o.call(0x23d820, &[]) as u32;
        let m = dir::check_list(&mut list, savename);
        assert_eq!(r, m, "case {n}");
        assert_eq!(o.get(LIST, LIST_LEN), list, "case {n}: list");
        seen.insert(m);
    }
    assert!(seen.len() > 20, "{} different results", seen.len());
}

#[test]
fn orig_23d9e0_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d9e0", 53) else { return };
    let (mut found, mut none) = (0, 0);
    for n in 0..CASES {
        let count = o.rng.below(12) as usize;
        let mut list = random_list(&mut o, b"BASLUS-21005-07", count);
        let cur = (o.rng.below(count as u64 + 1)) as i32;
        list[4..8].copy_from_slice(&cur.to_le_bytes());
        o.put(LIST, &list);
        let prefill = o.fill(NAME, NAME_LEN);
        let mut name = NameBuf(prefill.try_into().unwrap());
        let r = o.call(0x23d9e0, &[]) as u32 as i32;
        let m = dir::first_bad(&mut list, &mut name);
        assert_eq!(r, m, "case {n}");
        assert_eq!(o.get(LIST, LIST_LEN), list, "case {n}: list");
        assert_eq!(name_buf(&o), name, "case {n}: name");
        if m >= 0 {
            found += 1;
        } else {
            none += 1;
        }
    }
    assert!(found > 100 && none > 20, "{found} found, {none} none");
}

#[test]
fn orig_23d9a8_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d9a8", 55) else { return };
    for n in 0..CASES {
        let count = o.rng.below(12) as usize;
        let mut list = random_list(&mut o, b"BASLUS-21005-07", count);
        let cur = (o.rng.below(count as u64 + 1)) as i32;
        list[4..8].copy_from_slice(&cur.to_le_bytes());
        o.put(LIST, &list);
        o.call(0x23d9a8, &[]);
        dir::clear_current(&mut list);
        assert_eq!(o.get(LIST, LIST_LEN), list, "case {n}: cleared entry {cur}");
    }
}

#[test]
fn orig_23d6c8_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d6c8", 54) else { return };
    for n in 0..CASES {
        let env = set_region(&mut o);
        let mut info = random_info(&mut o);
        info.0[1] = [0u8, 1, 5, 98, 0xff, 0xfe, 0xfd, 0xfc, 0xfb, 0x7f][o.rng.below(10) as usize];
        let count = o.rng.below(100) as i32;
        set_count(&mut o, &mut info, count);
        let table = random_table(&mut o, b"x");
        let mut list = o.random(LIST_LEN);
        let prefill = o.fill(NAME, NAME_LEN);
        let mut name = NameBuf(prefill.try_into().unwrap());
        o.put(INFO, &info.0);
        o.put(DIRTAB, &table);
        o.put(LIST, &list);
        let r = o.call(0x23d6c8, &[]) as u32 as i32;
        let m = dir::build_list(&env, &info, &mut name, &mut list, &table, count);
        assert_eq!(r, m, "case {n}");
        assert_eq!(o.get(LIST, LIST_LEN), list, "case {n}: list");
        assert_eq!(name_buf(&o), name, "case {n}: name");
    }
}

// --- names ------------------------------------------------------------------------------------------------------------------

#[test]
fn orig_23cb90_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23cb90", 60) else { return };
    for n in 0..CASES {
        let env = set_region(&mut o);
        o.fill(NAME, NAME_LEN);
        let slot = o.rng.int(&[0, 9, 10, 98, 99, 100]) as i64;
        o.call(0x23cb90, &[NAME as u64, slot as u64]);
        assert_eq!(o.cstr(NAME), names::save_dir_name(&env, slot).into_bytes(), "case {n}: region {} slot {slot}", env.region);
    }
}

#[test]
fn orig_23cc38_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23cc38", 61) else { return };
    for region in 0..8u32 {
        o.set_word(0x349d44, region);
        let p = o.call(0x23cc38, &[]) as u32;
        assert_eq!(o.cstr(p), names::region_prefix(region).as_bytes(), "region {region}");
    }
}

#[test]
fn orig_23cc98_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23cc98", 62) else { return };
    for n in 0..CASES {
        let env = set_region(&mut o);
        let before = o.fill(NAME, NAME_LEN);
        let sel: i64 = [-6, -5, -4, -3, -2, -1, 0, 7, 98, 99][o.rng.below(10) as usize] + if n % 7 == 0 { o.rng.below(300) as i64 - 150 } else { 0 };
        o.call(0x23cc98, &[sel as u64]);
        let mut buf = NameBuf(before.try_into().unwrap());
        let cur = String::from_utf8_lossy(buf.bytes()).into_owned();
        if sel != -5 {
            buf.set(names::name_for(&env, sel, &cur).as_bytes());
        }
        assert_eq!(name_buf(&o), buf, "case {n}: selector {sel}");
    }
}
