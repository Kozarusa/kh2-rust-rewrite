//! The port-side half of the rig scenario `story-forge` (L1-close.md, WP-RIG-G3): the save block after a forged story flag.
//!
//! The original side is not a PCSX2 run: the setter `FUN_0017ff60` is already proven byte for byte by `orig_save2.rs`
//! (`orig_17ff60_matches_the_game`, on the byte-layout port `progress_flags::set_flag_full`), and a forge through PCSX2 would need a
//! memory write and a code call the rig client does not have. What this test closes is the gap the oracle does not cover: the app does
//! not call `set_flag_full`; its `Progress::story().set(id)` is a separate implementation (a story of word arrays) that the state key
//! `story_flags` and every script host use. Here both run the REAL side-effect lists of the user's `00progress.bin` for every flag id
//! of the 19 pages, each from a fresh new-game story, and the story regions of the save block are compared: the flag words, the area
//! program table and the exit-lock halfwords. The ids where the two differ are the ops the app story does not port (recorded in its
//! `unported` list: 6, 8, 9 and the world-point parts of 4 and 5 touch memory this test does not compare, so they are not differences
//! here), so any difference this test reports is a real finding.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::sync::Arc;

use kh2_game_core::progress::{Story, story};
use kh2_game_core::progress_flags::{SetterTables, SetterTail, set_flag_full};
use kh2_game_core::save_block::{LEN, at};

struct NoTail;

impl SetterTail for NoTail {
    fn call(&mut self, _save: &mut [u8], _addr: u32, _arg: i32) {}
}

/// The exit locks compared: the `dsa` table has well under this many entries.
const LOCKS: usize = 256;

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

/// The story regions of the app's story as the save block lays them out: (flag words, programs of all 19 worlds, lock halfwords).
fn app_view(s: &Story) -> (Vec<u32>, Vec<u8>, Vec<u16>) {
    let mut words = vec![0u32; story::WORLDS * story::PAGE / 4];
    for id in s.set_flags() {
        let w = (id as usize >> 10) * (story::PAGE / 4) + ((id as usize & 0x3ff) >> 5);
        words[w] |= 1 << (id & 31);
    }
    let mut programs = Vec::new();
    for world in 0..story::WORLDS {
        for area in 0..(story::PROGRAMS_PER_WORLD / 3) {
            programs.extend(s.programs(world as u8, area as u8));
        }
    }
    let locks = (0..LOCKS).flat_map(|i| [s.lock_state(i), s.lock_message(i)]).collect();
    (words, programs, locks)
}

fn block_view(b: &[u8]) -> (Vec<u32>, Vec<u8>, Vec<u16>) {
    let words = (0..story::WORLDS * story::PAGE / 4).map(|w| le32(b, at::FLAGS + 4 * w)).collect();
    let mut programs = Vec::new();
    for world in 0..story::WORLDS {
        let o = at::PROGRAMS + world * story::PROGRAMS_PER_WORLD;
        programs.extend_from_slice(&b[o..o + story::PROGRAMS_PER_WORLD / 3 * 3]);
    }
    let locks = (0..LOCKS).flat_map(|i| [at::LOCKS + 4 * i, at::LOCKS + 4 * i + 2]).map(|o| u16::from_le_bytes([b[o], b[o + 1]])).collect();
    (words, programs, locks)
}

#[test]
fn a_forged_story_flag_leaves_the_same_flags_programs_and_locks_in_the_app_story_as_in_the_original_setter() {
    let Some(mut disc) = kh2_factory::disc("story-forge") else { return };
    let file = disc.read("00progress.bin").expect("00progress.bin");
    let entries: Vec<Vec<u8>> = kh2_bar::parse(&file).expect("bar").into_iter().filter(|e| e.kind == 2).take(story::WORLDS).map(|e| e.data.to_vec()).collect();
    assert_eq!(entries.len(), story::WORLDS, "the 19 world entries of the flag side effects");
    let refs: Vec<&[u8]> = entries.iter().map(|e| e.as_slice()).collect();
    let zeros = vec![0u8; 0x1000];
    let tables = SetterTables { entries: &refs, locks: &zeros, points: &zeros };
    let effects = Arc::new(entries.clone());

    let mut different = Vec::new();
    let mut with_effects = 0usize;
    // Only the ids the world's offset table lists: past it the original reads list bytes as offsets (nothing in the game sets those).
    let counts: Vec<usize> = entries.iter().map(|e| kh2_ard::progress::flag_count(e)).collect();
    let mut checked = 0usize;
    for id in 0..(story::WORLDS * 1024) as u16 {
        if usize::from(id & 0x3ff) >= counts[usize::from(id >> 10)] {
            continue;
        }
        checked += 1;
        let mut block = vec![0u8; LEN];
        set_flag_full(&mut block, &tables, &mut NoTail, i32::from(id));
        let mut app = Story::with_effects(effects.clone());
        app.set(id);
        let (a, b) = (app_view(&app), block_view(&block));
        if a.1.iter().any(|&p| p != 0) || a.2.iter().any(|&l| l != 0) || a.0.iter().map(|w| w.count_ones()).sum::<u32>() > 1 {
            with_effects += 1;
        }
        if a != b {
            if different.len() < 3 {
                let (w, p, l) = (a.0 != b.0, a.1 != b.1, a.2 != b.2);
                eprintln!("story-forge: flag {id:#x} differs in words {w}, programs {p}, locks {l}");
                let nz = |v: &[u8]| v.iter().enumerate().filter(|(_, b)| **b != 0).map(|(i, b)| (i, *b)).collect::<Vec<_>>();
                let wz = |v: &[u32]| v.iter().enumerate().filter(|(_, b)| **b != 0).map(|(i, b)| (i, *b)).collect::<Vec<_>>();
                eprintln!("  app words {:x?} orig words {:x?}
  app programs {:?}
  orig programs {:?}", wz(&a.0), wz(&b.0), nz(&a.1), nz(&b.1));
            }
            different.push(id);
        }
    }
    eprintln!("story-forge: {checked} listed flags checked, {with_effects} flags with a visible side effect; {} differ: {:x?}", different.len(), &different[..different.len().min(40)]);
    assert!(with_effects > 100, "the lists of the disc must be doing something ({with_effects} flags changed a program, lock or other flag)");
    assert!(different.is_empty(), "{} flags leave a different story block in the app than in the original setter, first: {:x?}", different.len(), &different[..different.len().min(40)]);
}
