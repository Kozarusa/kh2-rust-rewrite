//! Pins the address constants of `kh2_rig::save` and the pad button order: against the app's own source (`crates/kh2-game-core/src/progress.rs`
//! and the other files that name the same addresses) and against the user's save states (skipped when absent).

use kh2_rig::p2s::P2s;
use kh2_rig::pad::{BUTTONS, Frame};
use kh2_rig::save::*;
use std::path::PathBuf;

fn app_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kh2-app/src")
}

/// The game crates' source folders: the app and its core crate (with the core's `shared` types).
fn game_srcs() -> [PathBuf; 3] {
    let core = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kh2-game-core/src");
    [app_src(), core.join("shared"), core]
}

fn progress_source() -> String {
    std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kh2-game-core/src/progress.rs")).expect("crates/kh2-game-core/src/progress.rs")
}

/// The value of `pub const NAME` inside `pub mod <module>` of `src` (a plain hex or decimal literal, underscores allowed).
fn app_const(src: &str, module: &str, name: &str) -> u64 {
    let at = src.find(&format!("pub mod {module} {{")).unwrap_or_else(|| panic!("no pub mod {module}"));
    let body = &src[at..];
    let end = body.find("\n}\n").unwrap_or(body.len());
    let line = body[..end].lines().find(|l| l.trim_start().starts_with(&format!("pub const {name}:"))).unwrap_or_else(|| panic!("no {module}::{name}"));
    let v = line.split('=').nth(1).unwrap().trim().trim_end_matches(';').replace('_', "");
    match v.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16).unwrap(),
        None => v.parse().unwrap(),
    }
}

/// Every `.rs` file of the app, lowercased and lossy (one file holds binary bytes).
fn app_text() -> String {
    let mut all = String::new();
    for e in game_srcs().iter().flat_map(|d| std::fs::read_dir(d).unwrap()) {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "rs") {
            all.push_str(&String::from_utf8_lossy(&std::fs::read(p).unwrap()).to_lowercase());
        }
    }
    all
}

fn states() -> Vec<(String, P2s)> {
    let Some(dir) = std::env::var_os("KH2_SSTATES").map(PathBuf::from).or_else(|| std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("Documents/PCSX2/sstates"))) else { return Vec::new() };
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<_> = rd.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "p2s")).collect();
    out.sort();
    out.into_iter().filter_map(|p| Some((p.file_name()?.to_string_lossy().into_owned(), P2s::open(&p).ok()?))).collect()
}

#[test]
fn save_block_and_party_block_match_the_app() {
    let src = progress_source();
    assert_eq!(u64::from(SAVE_BLOCK), app_const(&src, "addr", "SAVE"));
    assert_eq!(SAVE_BLOCK_LEN as u64, app_const(&src, "addr", "SAVE_LEN"));
    assert_eq!(u64::from(PARTY_BLOCK), app_const(&src, "addr", "PARTY"));
    assert_eq!(PARTY_BLOCK_LEN as u64, app_const(&src, "off", "LEN"));
    // The party block is save + 0x1660 (progress.rs), the wallet + 0x1600, the game mode + 0x1658, the record bits + 0x1550.
    assert_eq!(PARTY_BLOCK, SAVE_BLOCK + 0x1660);
    assert_eq!(WALLET, SAVE_BLOCK + 0x1600);
    assert_eq!(GAME_MODE, SAVE_BLOCK + 0x1658);
    assert_eq!(RECORD_BITS, SAVE_BLOCK + 0x1550);
}

#[test]
fn story_tables_match_the_app_and_tile_without_gaps() {
    let src = progress_source();
    assert_eq!(u64::from(STORY_FLAGS), app_const(&src, "story", "FLAGS"));
    assert_eq!(STORY_PAGE as u64, app_const(&src, "story", "PAGE"));
    assert_eq!(STORY_WORLDS as u64, app_const(&src, "story", "WORLDS"));
    assert_eq!(u64::from(STORY_PROGRAMS), app_const(&src, "story", "PROGRAMS"));
    assert_eq!(STORY_PROGRAMS_PER_WORLD as u64, app_const(&src, "story", "PROGRAMS_PER_WORLD"));
    assert_eq!(u64::from(EXIT_LOCKS), app_const(&src, "story", "LOCKS"));
    assert_eq!(u64::from(WORLD_STATE), app_const(&src, "world", "RECORDS"));
    // The program table ends one byte before the flag pages (the table starts at an odd address: map byte first), the flag pages end
    // where the world-point words begin (progress.rs: "ends at 0x33f910"), the two world-point words where the exit locks begin.
    assert_eq!(STORY_PROGRAMS + (STORY_WORLDS * STORY_PROGRAMS_PER_WORLD) as u32, STORY_FLAGS - 1);
    assert_eq!(STORY_FLAGS + (STORY_WORLDS * STORY_PAGE) as u32, WORLD_POINTS);
    assert_eq!(WORLD_POINTS + 8, EXIT_LOCKS);
    assert_eq!((RECORD_BITS - EXIT_LOCKS) % 4, 0);
    // 23 record words end at the per-world array 0x33fe0c, which the core-runtime doc names.
    assert_eq!(RECORD_BITS + 4 * RECORD_WORDS as u32, 0x0033_fe0c);
    let doc = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/systems/core-runtime/progress.md")).unwrap().to_lowercase();
    assert!(doc.contains("33fe0c") && doc.contains("33fdb0"), "docs/systems/core-runtime/progress.md names the record array");
}

#[test]
fn addresses_the_app_names_in_comments_agree() {
    let (src, all) = (progress_source().to_lowercase(), app_text());
    for (what, addr, text) in [("world points", WORLD_POINTS, &src), ("record bits", RECORD_BITS, &src), ("story flags", STORY_FLAGS, &src), ("exit locks", EXIT_LOCKS, &all), ("frame step", FRAME_STEP, &all), ("player pointer", PLAYER_POINTER, &all)] {
        let h = format!("{addr:x}");
        assert!(text.contains(&h), "{what}: the app never mentions {h}");
    }
}

#[test]
fn the_pad_button_order_is_the_dualshock_two_layout() {
    assert_eq!(BUTTONS, ["select", "l3", "r3", "start", "up", "right", "down", "left", "l2", "r2", "l1", "r1", "triangle", "circle", "cross", "square"]);
    // Idle is ff ff 7f 7f 7f 7f (RIG.md 1.3); each pressed button clears exactly its own bit, byte 0 first.
    assert_eq!(Frame::default().bytes(), [0xff, 0xff, 0x7f, 0x7f, 0x7f, 0x7f]);
    for (i, name) in BUTTONS.iter().enumerate() {
        let f = Frame { buttons: 1 << i, ..Frame::default() };
        let b = f.bytes();
        let want = if i < 8 { [0xff ^ (1u8 << i), 0xff] } else { [0xff, 0xff ^ (1u8 << (i - 8))] };
        assert_eq!([b[0], b[1]], want, "{name}");
        assert!(f.pressed(name));
    }
    // Stick byte order: right x, right y, left x, left y.
    let f = Frame { lx: 1, ly: 2, rx: 3, ry: 4, buttons: 0 };
    assert_eq!(f.bytes()[2..], [3, 4, 1, 2]);
}

#[test]
fn the_users_states_agree_with_the_constants() {
    let all = states();
    if all.is_empty() {
        eprintln!("skipped: no states in the sstates folder");
        return;
    }
    // (file suffix, program bytes at the state's own world and area) for states where the table entry is set.
    let programs: [(&str, [u8; 3]); 4] = [(".06.p2s", [2, 0, 0]), (".04.p2s", [4, 0, 0]), (".07.p2s", [1, 1, 0]), (".08.p2s", [1, 1, 0])];
    let mut checked_programs = 0;
    for (name, s) in &all {
        let m = Ee(&s.ee);
        let x = Extract::read(&s.ee).unwrap_or_else(|e| panic!("{name}: {e}"));
        // The frame step is 1.0 (menus) or 2.0 (field).
        assert!(x.frame_step == 1.0 || x.frame_step == 2.0, "{name}: frame step {}", x.frame_step);
        // The party block starts with Sora's record: hp = max hp, mp 100/100, level 1..=99 (record +0x4, +0x5, +0x6, +0x7, +0xf).
        let p = m.bytes(PARTY_BLOCK, 0x10).unwrap();
        assert!(p[4] > 0 && p[4] == p[5] && p[6] == 100 && p[7] == 100 && (1..=99).contains(&p[0xf]), "{name}: party record {p:02x?}");
        assert!(m.bytes(PARTY_BLOCK, PARTY_BLOCK_LEN).is_some());
        assert!(m.u32(WALLET).unwrap() < 10_000_000, "{name}: wallet");
        // Exit locks: entry 61 is (0xffff, 0) in every state; a set state halfword has a message halfword (entries 97..=138).
        let l61 = m.u32(EXIT_LOCKS + 4 * 61).unwrap();
        assert_eq!((l61 & 0xffff, l61 >> 16), (0xffff, 0), "{name}: lock 61");
        for i in 97..=138u32 {
            let w = m.u32(EXIT_LOCKS + 4 * i).unwrap();
            assert!(w & 0xffff == 0 || w >> 16 != 0, "{name}: lock {i} has a state but no message ({w:#x})");
        }
        // World points: only the first 0x37 point bits are ever set (the second word's bits 0x37 - 32 and up are clear).
        let w1 = m.u32(WORLD_POINTS + 4).unwrap();
        assert_eq!(w1 >> (0x37 - 32), 0, "{name}: world point word 1 {w1:#x}");
        // A played state has record bits set.
        if x.world != 255 && !name.ends_with(".01.p2s") {
            assert!((0..RECORD_WORDS as u32).any(|i| m.u32(RECORD_BITS + 4 * i).unwrap() != 0), "{name}: record bits all zero");
        }
        // The area program table at the state's own world and area.
        if let Some((_, want)) = programs.iter().find(|(suffix, _)| name.ends_with(suffix)) {
            let at = STORY_PROGRAMS + u32::from(x.world) * STORY_PROGRAMS_PER_WORLD as u32 + u32::from(x.area) * 3;
            assert_eq!(m.bytes(at, 3).unwrap(), want, "{name}: program entry of world {} area {}", x.world, x.area);
            checked_programs += 1;
        }
    }
    eprintln!("{} states checked, {checked_programs} program entries", all.len());
}
