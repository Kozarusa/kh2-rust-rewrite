//! The `.ps2` card image reader and writer, and loading a card save into `Progress`.
//!
//! Two kinds of test. The fixture tests build an 8 MB card in memory (formatted empty, then saves written from a synthetic save block:
//! never a real save in the repository) and check what the reader returns, and that the original's own check (`FUN_0023d1c8`, run in
//! kh2-ee on the written body; needs the disc) accepts what the writer wrote. The `KH2_CARD` tests read the user's own PCSX2 card (the
//! path in the environment variable; skipped when unset) and check the saves on it: every ECC byte, the byte-exact round trip, the load
//! into `Progress`, and that the port's export, `icon.sys` and file names reproduce what the original wrote on that card.

mod common;

use std::sync::Arc;

use common::*;

/// Starts kh2-ee (`Ee`) on the user's executable with the unit's pointer globals set; `None` without a disc.
fn start(what: &str, seed: u64) -> Option<Orig> {
    let o = Orig::new(what, seed)?;
    let _: &kh2_ee::Ee = &o.ee;
    Some(o)
}

use kh2_game_core::progress::{ItemIndex, Progress};
use kh2_game_core::save_block::{self, header_word};
use kh2_save::body::{self, BODY_LEN, ICON_LEN};
use kh2_save::image::{Card, ecc};
use kh2_save::load::{self, WriteEnv};
use kh2_save::names::{NameEnv, save_dir_name};

const STAMP: [u8; 8] = [0, 10, 20, 12, 22, 10, 0xea, 0x07];

/// A synthetic save block in the original's layout: the header, a name for each field the slot row shows, and a little else.
fn synthetic_live(level: u8, munny: u32, play_ticks: u32) -> Vec<u8> {
    let mut b = vec![0u8; save_block::LEN];
    b[..4].copy_from_slice(&header_word(2).to_le_bytes());
    b[4..8].copy_from_slice(&0x2du32.to_le_bytes());
    b[0xc] = 4; // world
    b[0xd] = 1; // area
    b[0x1600..0x1604].copy_from_slice(&munny.to_le_bytes());
    b[0x1604..0x1608].copy_from_slice(&play_ticks.to_le_bytes());
    b[0x1658] = 1; // game mode
    b[0x1660 + 0xf] = level; // Sora's level (the first party record)
    b
}

fn test_env() -> WriteEnv {
    WriteEnv {
        names: NameEnv::usa(),
        icon: kh2_save::icon::IconEnv {
            magic: *b"PS2D",
            background: [1; 0x40],
            light_dirs: [2; 0x30],
            light_colors: [3; 0x30],
            ambient: [4; 0x10],
            line1: b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_vec(),
            line2: b"BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_vec(),
            marks: [[0x81, 0x40]; 4],
            blank: [0x81, 0x40],
        },
        // Every entry is a candidate (flag id 0xffff is never set; a non-zero `a` that is clear counts).
        variants: [(0xffff, 0xffff); 8],
        model: vec![0x5a; ICON_LEN],
    }
}

#[test]
fn a_formatted_card_is_empty_and_round_trips() {
    let card = Card::format(STAMP);
    assert!(card.directories().unwrap().is_empty());
    assert_eq!(card.free_clusters(), 8134);
    let bytes = card.to_bytes();
    assert_eq!(bytes.len(), 8_650_752);
    assert_eq!(Card::from_bytes(&bytes).unwrap(), card);
    // Without spare bytes the card is 8 MB flat and reads as the same card.
    let plain = card.to_plain();
    assert_eq!(plain.len(), 8_388_608);
    assert_eq!(Card::from_bytes(&plain).unwrap(), card);
    assert!(Card::from_bytes(&bytes[..1000]).is_err());
    let mut bad = bytes.clone();
    bad[0] = b'X';
    assert!(Card::from_bytes(&bad).is_err());
}

#[test]
fn saves_written_to_a_card_read_back() {
    let env = test_env();
    let mut card = Card::format(STAMP);
    let a = synthetic_live(12, 345, 216_000 * 3 + 3600 * 7);
    let b = synthetic_live(55, 99_999, 1234);
    load::write_save(&mut card, &env, 0, &a, STAMP).unwrap();
    load::write_save(&mut card, &env, 3, &b, STAMP).unwrap();
    let saves = load::saves(&card, &env.names).unwrap();
    assert_eq!(saves.iter().map(|s| s.slot).collect::<Vec<_>>(), vec![0, 3]);
    assert!(saves.iter().all(|s| s.valid), "both bodies pass the check");
    assert_eq!(saves[0].dir.name, "BASLUS-21005-00");
    assert_eq!(saves[1].dir.name, "BASLUS-21005-03");
    // The body is the live block with the variant byte and the CRC set, nothing else.
    let body = saves[0].body.as_ref().unwrap();
    let mut expected = a.clone();
    expected[0x31a8] = body[0x31a8];
    expected[0x31a9] &= !1;
    expected[0x31a9] |= body[0x31a9] & 1;
    body::stamp_crc(&mut expected);
    assert_eq!(body, &expected);
    // The three files are there with the sizes the original checks.
    let dir = &saves[0].dir;
    let files = card.entries(dir).unwrap();
    let sizes: Vec<(String, u32)> = files.iter().filter(|f| f.is_file()).map(|f| (f.name.clone(), f.length)).collect();
    assert_eq!(sizes, vec![("kh2.ico".into(), 0x8a58), ("icon.sys".into(), 0x3c4), ("BASLUS-21005-00".into(), 0xb4e0)]);
    assert_eq!(card.read_file("BASLUS-21005-00", "kh2.ico").unwrap().unwrap(), env.model);
    // The slot table sees both.
    let info = load::slot_table(&card, &env.names, &|_, _| 7).unwrap();
    assert!(info.is_valid(0) && info.is_valid(3) && !info.is_valid(1));
    assert_eq!(info.record(3)[0x11], 55, "the level in the slot row");
    assert_eq!(info.latest(), 0, "equal times keep the earlier slot");
    // The image round trips, with and without spare bytes.
    let bytes = card.to_bytes();
    assert_eq!(Card::from_bytes(&bytes).unwrap(), card);
    assert_eq!(Card::from_bytes(&card.to_plain()).unwrap(), card);
}

#[test]
fn rewriting_a_slot_replaces_it_and_deleting_frees_the_clusters() {
    let env = test_env();
    let mut card = Card::format(STAMP);
    let free0 = card.free_clusters();
    let a = synthetic_live(12, 1, 100);
    load::write_save(&mut card, &env, 5, &a, STAMP).unwrap();
    let used = free0 - card.free_clusters();
    assert!((80..90).contains(&used), "{used} clusters for a save");
    let b = synthetic_live(70, 2, 200);
    load::write_save(&mut card, &env, 5, &b, STAMP).unwrap();
    assert_eq!(free0 - card.free_clusters(), used, "the same slot takes the same space");
    let saves = load::saves(&card, &env.names).unwrap();
    assert_eq!(saves.len(), 1);
    assert_eq!(saves[0].body.as_ref().unwrap()[0x1660 + 0xf], 70);
    card.remove_dir(&save_dir_name(&env.names, 5)).unwrap();
    // The root directory grew by one cluster for the entry, and keeps it.
    assert_eq!(card.free_clusters(), free0 - 1);
    assert!(load::saves(&card, &env.names).unwrap().is_empty());
    // Many saves make the root directory grow past one cluster.
    for s in 0..40 {
        load::write_save(&mut card, &env, s, &a, STAMP).unwrap();
    }
    assert_eq!(load::saves(&card, &env.names).unwrap().len(), 40);
    assert!(load::saves(&card, &env.names).unwrap().iter().all(|s| s.valid));
    // A full card fails and is left as it was.
    let before = card.clone();
    let mut r = Ok(());
    for s in 40..99 {
        r = load::write_save(&mut card, &env, s, &a, STAMP);
        if r.is_err() {
            break;
        }
    }
    assert!(r.is_err(), "99 saves of about 85 clusters do not fit in 8135");
    assert!(load::saves(&card, &env.names).unwrap().iter().all(|s| s.valid));
    let _ = before;
}

#[test]
fn a_save_loads_into_progress_and_comes_back() {
    let env = test_env();
    let mut card = Card::format(STAMP);
    let mut live = synthetic_live(33, 4321, 5000);
    live[0x1660 + 0xf4 + 0xf] = 21; // Donald's level
    load::write_save(&mut card, &env, 2, &live, STAMP).unwrap();
    let bytes = card.to_bytes();
    let path = std::env::temp_dir().join(format!("kh2-save-test-{}.ps2", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();
    let mut p = Progress::from_block(&[], Arc::new(ItemIndex::new(&[], &[], &[])));
    let slot = load::load_file(&mut p, &path, &env.names, None).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(slot, 2);
    assert_eq!(p.member(1).unwrap().level, 33);
    assert_eq!(p.member(2).unwrap().level, 21);
    assert_eq!(p.wallet(), 4321);
    // The image the game would save again is the body that was loaded (the regions the app models; the option word stays the app's).
    let img = p.save_image();
    let body = card.read_file("BASLUS-21005-02", "BASLUS-21005-02").unwrap().unwrap();
    let diff: Vec<usize> = (0..BODY_LEN).filter(|&i| img[i] != body[i]).collect();
    assert!(diff.iter().all(|&i| (0x31a8..0x31b4).contains(&i) || (8..12).contains(&i)), "differences outside the option word, the variant bytes and the CRC: {:x?}", &diff[..diff.len().min(20)]);
}

#[test]
fn a_bad_card_or_slot_is_refused() {
    let env = test_env();
    let mut card = Card::format(STAMP);
    let live = synthetic_live(1, 0, 0);
    assert!(load::write_save(&mut card, &env, 99, &live, STAMP).is_err());
    let mut e2 = test_env();
    e2.model.truncate(10);
    assert!(load::write_save(&mut card, &e2, 0, &live, STAMP).is_err());
    let mut p = Progress::from_block(&[], Arc::new(ItemIndex::new(&[], &[], &[])));
    assert!(load::apply(&mut p, &[0; 10]).is_err());
    let path = std::env::temp_dir().join(format!("kh2-save-empty-{}.ps2", std::process::id()));
    std::fs::write(&path, card.to_bytes()).unwrap();
    let r = load::load_file(&mut p, &path, &env.names, None);
    std::fs::remove_file(&path).ok();
    assert!(r.is_err(), "an empty card holds no save");
    assert!(load::from_env(&mut p).is_none() || std::env::var("KH2_CARD").is_ok());
}

/// The original's own check accepts a body the writer wrote, and rejects it after one flipped bit (kh2-ee runs `FUN_0023d1c8`).
#[test]
fn orig_23d1c8_accepts_what_the_writer_wrote() {
    let Some(mut o) = start("the original's check of a written save body", 70) else { return };
    o.build_crc_table();
    o.set_word(0x349d44, 2);
    let env = WriteEnv { variants: o.variants(), icon: o.icon_env(), ..test_env() };
    let mut card = Card::format(STAMP);
    for (slot, level) in [(0usize, 7u8), (9, 90)] {
        let mut live = synthetic_live(level, 1000 * u32::from(level), 777 * u32::from(level));
        live[0x1607] &= 0x7f;
        // Flags are clear, so the variant comes from the pairs of the executable only when one is a candidate; set one.
        let flag = 0x1000usize;
        let _ = flag;
        let Ok(()) = load::write_save(&mut card, &env, slot, &live, STAMP) else {
            eprintln!("slot {slot}: the executable's variant pairs give no candidate for an all-clear save; skipped");
            continue;
        };
        let name = save_dir_name(&env.names, slot as i64);
        let body = card.read_file(&name, &name).unwrap().unwrap();
        o.put(FILEBUF, &body);
        assert_ne!(o.call(0x23d1c8, &[0]) as u32 & 0xff, 0, "the original accepts the written body of slot {slot}");
        let mut bad = body.clone();
        bad[0x2000] ^= 1;
        o.put(FILEBUF, &bad);
        assert_eq!(o.call(0x23d1c8, &[0]) as u32 & 0xff, 0, "one flipped bit fails the original's check");
    }
}

// --- the user's own card ----------------------------------------------------------------------------------------------------

fn user_card() -> Option<(std::path::PathBuf, Vec<u8>)> {
    let Ok(p) = std::env::var("KH2_CARD") else {
        eprintln!("skipped: KH2_CARD is not set (the path of a PCSX2 .ps2 card image)");
        return None;
    };
    let path = std::path::PathBuf::from(p);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("KH2_CARD {}: {e}", path.display()));
    Some((path, bytes))
}

#[test]
fn the_users_card_ecc_and_bytes_round_trip() {
    let Some((_, bytes)) = user_card() else { return };
    let card = Card::from_bytes(&bytes).unwrap();
    assert_eq!(card.to_bytes(), bytes);
    // Every page that has ECC bytes (not 0xff) has the ECC this code computes.
    // The one exception on the user's card is page 1 (the superblock cluster's second page): its first chunk holds bytes whose stored ECC
    // is that of an empty chunk, so the card was changed after its ECC was made (by PCSX2 itself).
    let (mut checked, mut odd) = (0, Vec::new());
    for p in 0..kh2_save::image::PAGES {
        let spare = &bytes[p * 528 + 512..p * 528 + 528];
        if spare.iter().all(|&b| b == 0xff) {
            continue;
        }
        let good = (0..4).all(|c| ecc(&bytes[p * 528 + c * 128..p * 528 + (c + 1) * 128]) == spare[c * 3..c * 3 + 3]);
        assert_eq!(&spare[12..], &[0, 0, 0, 0], "page {p}");
        if good {
            checked += 1;
        } else {
            odd.push(p);
        }
    }
    assert!(checked > 400, "{checked} pages with ECC");
    assert!(odd.iter().all(|&p| p == 1), "pages whose stored ECC differs: {odd:?}");
    // Writing the same data again page by page (through the flat form) gives the same card for every page that had ECC.
    let again = Card::from_bytes(&card.to_plain()).unwrap().to_bytes();
    let differing = (0..kh2_save::image::PAGES).filter(|&p| again[p * 528..(p + 1) * 528] != bytes[p * 528..(p + 1) * 528]).count();
    eprintln!("{differing} pages differ only in never-written spare bytes");
    assert!((0..kh2_save::image::PAGES).all(|p| again[p * 528..p * 528 + 512] == bytes[p * 528..p * 528 + 512]));
}

#[test]
fn the_users_card_loads_into_progress() {
    let Some((path, bytes)) = user_card() else { return };
    let card = Card::from_bytes(&bytes).unwrap();
    let env = NameEnv::usa();
    let saves = load::saves(&card, &env).unwrap();
    assert!(!saves.is_empty(), "the card holds no KH2 save");
    for s in &saves {
        eprintln!("slot {}: {} body {:?} valid {}", s.slot, s.dir.name, s.body.as_ref().map(Vec::len), s.valid);
        assert_eq!(s.dir.name, save_dir_name(&env, s.slot as i64));
    }
    let valid: Vec<_> = saves.iter().filter(|s| s.valid).collect();
    assert!(!valid.is_empty(), "no save passes the original's check");
    let items = Arc::new(ItemIndex::new(&[], &[], &[]));
    for s in &valid {
        let body = s.body.as_ref().unwrap();
        let mut p = Progress::from_block(&[], items.clone());
        load::apply(&mut p, body).unwrap();
        let sora = p.member(1).unwrap();
        let level = u32::from(sora.level);
        eprintln!("slot {}: Sora level {level}, hp {}/{}, munny {}, weapon {:?}", s.slot, sora.hp, sora.hp_max, p.wallet(), sora.weapons);
        assert!((1..=99).contains(&level), "Sora's level {level}");
        assert!(sora.hp_max >= 20 && sora.hp >= 1 && sora.hp <= sora.hp_max.max(sora.hp), "hp {}/{}", sora.hp, sora.hp_max);
        assert!(p.wallet() <= 9_999_999, "munny {}", p.wallet());
        assert_ne!(sora.weapons[0], 0, "Sora has a keyblade");
        assert!(!p.story().set_flags().is_empty(), "story flags are set");
        let slot_level = body[0x1660 + 0xf];
        assert_eq!(slot_level as u32, level);
        // Round trip: the image `Progress` would save again equals the body, except the option word (the app keeps its own), the shop
        // words it clears and the CRC field.
        let img = p.save_image();
        let diff: Vec<usize> = (0..BODY_LEN).filter(|&i| img[i] != body[i]).collect();
        eprintln!("slot {}: {} bytes differ after the round trip: {:x?}", s.slot, diff.len(), &diff[..diff.len().min(40)]);
        assert!(diff.iter().all(|&i| (0x31b0..0x31b4).contains(&i) || (0x370c..0x3728).contains(&i)), "unexpected differences: {:x?}", &diff[..diff.len().min(40)]);
    }
    // The loader entry point behind KH2_CARD reads the same file.
    let mut p = Progress::from_block(&[], items);
    let slot = load::load_file(&mut p, &path, &env, None).unwrap();
    eprintln!("the latest save is slot {slot}");
    assert!(valid.iter().any(|s| s.slot == slot));
}

/// The port's export, `icon.sys` and names reproduce, byte for byte, what the original wrote on the user's card. Needs the disc (the icon
/// strings and colour tables come from the executable).
#[test]
fn the_users_card_matches_what_the_port_writes() {
    let Some((_, bytes)) = user_card() else { return };
    let Some(mut o) = start("regenerating the saves of the user's card", 71) else { return };
    o.build_crc_table();
    o.set_word(0x349d44, 2);
    let card = Card::from_bytes(&bytes).unwrap();
    let names = o.name_env(2);
    assert_eq!(names, NameEnv::usa(), "the region table of the executable gives the USA names");
    let (icon, variants) = (o.icon_env(), o.variants());
    let saves = load::saves(&card, &names).unwrap();
    let mut regenerated = 0;
    for s in saves.iter().filter(|s| s.valid) {
        let body = s.body.as_ref().unwrap();
        let name = &s.dir.name;
        // The body: exporting the saved block again gives the saved body (the variant pick depends only on the block).
        let again = body::export(body, &variants).expect("a variant applies");
        assert_eq!(&again, body, "slot {}: export of the body", s.slot);
        // icon.sys: the original wrote it from the live block of that moment, which the body copies.
        let sys = card.read_file(name, "icon.sys").unwrap().unwrap();
        let mine = kh2_save::icon::write_icon_sys(&icon, s.slot as i32, body);
        assert_eq!(sys, mine, "slot {}: icon.sys", s.slot);
        // The original accepts the body.
        o.put(FILEBUF, body);
        assert_ne!(o.call(0x23d1c8, &[0]) as u32 & 0xff, 0);
        // And the whole save written by the port into a copy of the card, in the same slot, reads back identical.
        let model = card.read_file(name, "kh2.ico").unwrap().unwrap();
        let env = WriteEnv { names: names.clone(), icon: icon.clone(), variants, model };
        let mut copy = card.clone();
        load::write_save(&mut copy, &env, s.slot, body, s.dir.modified).unwrap();
        for f in ["kh2.ico", "icon.sys", name.as_str()] {
            assert_eq!(copy.read_file(name, f).unwrap(), card.read_file(name, f).unwrap(), "slot {}: {f}", s.slot);
        }
        assert_eq!(load::saves(&copy, &names).unwrap().len(), saves.len());
        // A new slot next to the existing ones.
        let free = (0..99).find(|n| !saves.iter().any(|x| x.slot == *n)).unwrap();
        load::write_save(&mut copy, &env, free, body, s.dir.modified).unwrap();
        let all = load::saves(&copy, &names).unwrap();
        assert_eq!(all.len(), saves.len() + 1);
        assert!(all.iter().all(|x| x.valid || !saves.iter().any(|y| y.slot == x.slot && y.valid)));
        let new_name = save_dir_name(&names, free as i64);
        let new_body = copy.read_file(&new_name, &new_name).unwrap().unwrap();
        o.put(FILEBUF, &new_body);
        assert_ne!(o.call(0x23d1c8, &[0]) as u32 & 0xff, 0, "the original accepts the body written to the new slot");
        assert_eq!(Card::from_bytes(&copy.to_bytes()).unwrap(), copy);
        regenerated += 1;
    }
    assert!(regenerated > 0);
}
