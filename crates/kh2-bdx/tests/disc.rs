//! Tests against the user's disc (skipped without KH2_ISO): every script parses and analyses, the
//! signature table in names.rs agrees with the executable's, and Shadow and Soldier run on the mock world.

use kh2_bdx::host::Call;
use kh2_bdx::mock::{MockHost, SELF_OBJ};
use kh2_bdx::{HostTable, Program, Runtime, analyze};

fn open() -> Option<(kh2_archive::Archive, Vec<u8>)> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    Some((a, elf))
}

fn object_script(a: &mut kh2_archive::Archive, name: &str) -> Vec<u8> {
    let d = a.read(name).expect("object file");
    kh2_bar::parse(&d).unwrap().into_iter().find(|e| e.kind == 3).expect("BDX entry").data.to_vec()
}

#[test]
fn signatures_match_the_executable() {
    let Some((_, elf)) = open() else { return };
    let table = HostTable::from_exe(&elf).expect("table");
    for f in kh2_bdx::names::HOST_FUNCS {
        let s = table.get(f.group, f.index).unwrap_or_else(|| panic!("{}.{} {} missing", f.group, f.index, f.name));
        assert_eq!((s.nargs as u8, s.returns), (f.args, f.ret), "{}.{} {}", f.group, f.index, f.name);
    }
}

#[test]
fn shadow_and_soldier_parse_and_run() {
    let Some((mut a, _)) = open() else { return };
    for name in ["obj/M_EX020.mdlx", "obj/M_EX010.mdlx"] {
        let data = object_script(&mut a, name);
        let p = Program::parse(&data).unwrap();
        analyze(&p).unwrap();
        let mut rt = Runtime::new(p);
        rt.shared.rng = 12345;
        let mut host = MockHost::new();
        assert!(rt.spawn(&mut host, SELF_OBJ));
        let mut modes = Vec::new();
        for _ in 0..400 {
            rt.update(&mut host);
            host.tick();
            let m = rt.current_mode();
            if m.is_some() && modes.last() != m.as_ref() {
                modes.push(m.unwrap());
            }
        }
        assert!(rt.core.errors.is_empty(), "{name}: {:?}", rt.core.errors);
        // The enemy starts, settles into battle and goes after the player.
        assert!(modes.iter().any(|m| m.starts_with("appear")), "{name}: {modes:?}");
        assert!(host.calls().any(|c| matches!(c, Call::PlayMotion { .. } | Call::PlayMotionChecked { .. })), "{name}");
        assert!(host.calls().any(|c| matches!(c, Call::TurnToward { .. } | Call::FaceToward { .. })), "{name}");
    }
}

/// What one enemy script does on the mock world: every mode it entered (in order of first appearance)
/// and the host calls it made, for a player `distance` away, a random seed, and an optional damage event.
struct Dry {
    modes: Vec<String>,
    host: MockHost,
    errors: Vec<(usize, kh2_bdx::VmError)>,
}

fn dry_run(data: &[u8], seed: u32, distance: f32, frames: u32, damage_at: Option<u32>) -> Dry {
    dry_run_with(data, seed, distance, frames, damage_at, 99)
}

/// As `dry_run` with the answer of host 1.39 (spawn parameter) chosen.
fn dry_run_with(data: &[u8], seed: u32, distance: f32, frames: u32, damage_at: Option<u32>, spawn_param: i32) -> Dry {
    let p = Program::parse(data).unwrap();
    analyze(&p).unwrap();
    let mut rt = Runtime::new(p);
    rt.shared.rng = seed;
    rt.core.mode_log = Some(Vec::new());
    let mut host = MockHost::new();
    host.player_position = [distance, 0.0, 0.0, 1.0];
    host.spawn_param = spawn_param;
    assert!(rt.spawn(&mut host, SELF_OBJ));
    for frame in 0..frames {
        if damage_at == Some(frame) {
            rt.event(&mut host, 6, &[0x500]);
        }
        rt.update(&mut host);
        host.tick();
    }
    Dry { modes: rt.core.mode_log.take().unwrap(), host, errors: rt.core.errors.clone() }
}

/// Every mode the script enters with the player at several distances, several random seeds, with and
/// without being hit, and how many calls reached the host undecoded.
fn mode_cycle(data: &[u8]) -> (std::collections::BTreeSet<String>, usize) {
    mode_cycle_with(data, 99)
}

fn mode_cycle_with(data: &[u8], spawn_param: i32) -> (std::collections::BTreeSet<String>, usize) {
    let mut seen = std::collections::BTreeSet::new();
    let mut unknown = 0;
    for distance in [150.0, 250.0, 450.0, 900.0] {
        for seed in 1..=6 {
            for damage_at in [None, Some(200)] {
                let d = dry_run_with(data, seed, distance, 700, damage_at, spawn_param);
                assert!(d.errors.is_empty(), "seed {seed} distance {distance}: {:?}", d.errors);
                unknown += d.host.calls().filter(|c| matches!(c, Call::Unknown { .. })).count();
                seen.extend(d.modes);
            }
        }
    }
    (seen, unknown)
}

#[test]
fn shadow_runs_its_whole_mode_cycle() {
    let Some((mut a, _)) = open() else { return };
    let data = object_script(&mut a, "obj/M_EX020.mdlx");
    let (seen, unknown) = mode_cycle(&data);
    eprintln!("shadow modes: {seen:?}");
    assert_eq!(unknown, 0, "every host function Shadow calls is decoded");
    // (mode_revenge needs the attacker flag, host 1.309, which the mock leaves clear.)
    for m in ["appear_shadow", "mode_battle", "battle_idle", "move_target", "btl_attack", "btl_short", "btl_long"] {
        assert!(seen.contains(m), "mode {m} never ran: {seen:?}");
    }
    assert!(seen.iter().any(|m| m.starts_with("atk_craw")), "no claw attack: {seen:?}");
    let d = dry_run(&data, 1, 6000.0, 400, None);
    assert!(d.errors.is_empty(), "{:?}", d.errors);
    assert!(d.modes.iter().any(|m| m == "leave"), "{:?}", d.modes);
}

#[test]
fn soldier_runs_its_whole_mode_cycle() {
    let Some((mut a, _)) = open() else { return };
    let data = object_script(&mut a, "obj/M_EX010.mdlx");
    let (seen, unknown) = mode_cycle(&data);
    eprintln!("soldier modes: {seen:?}");
    assert_eq!(unknown, 0, "every host function Soldier calls is decoded");
    for m in ["appear", "mode_battle", "idle_time", "atk_craw", "atk_kick", "atk_jump", "move_target", "move_roll", "move_step", "move_back", "btl_attack", "btl_short", "btl_long", "rvg_short", "rvg_long", "mode_revenge", "fight"] {
        assert!(seen.contains(m), "mode {m} never ran: {seen:?}");
    }
    // Far away from the player it gives up and leaves (fades out and hides).
    let d = dry_run(&data, 1, 6000.0, 300, None);
    assert!(d.errors.is_empty(), "{:?}", d.errors);
    assert!(d.modes.iter().any(|m| m == "leave"), "{:?}", d.modes);
    assert!(d.host.calls().any(|c| matches!(c, Call::Hide { .. })));
}

#[test]
fn dusk_runs_its_mode_cycle() {
    let Some((mut a, _)) = open() else { return };
    let data = object_script(&mut a, "obj/M_EX990.mdlx");
    let (seen, unknown) = mode_cycle(&data);
    eprintln!("dusk modes: {seen:?}");
    assert_eq!(unknown, 0, "every host function the Dusk calls is decoded");
    for m in ["appear", "mode_battle", "btl_attack", "move_target", "mode_revenge"] {
        assert!(seen.contains(m), "mode {m} never ran: {seen:?}");
    }
    assert!(seen.iter().any(|m| m.starts_with("atk_")), "no attack: {seen:?}");
}


/// The Struggle opponent N_EX570_BTL (WP-6H) with the spawn parameter 1 of its tt04 groups (b_41: 1,
/// b_42: 2, b_45: 4, b_51: 5; `docs/enemies.md`).
#[test]
fn n_ex570_btl_runs_its_mode_cycle() {
    let Some((mut a, _)) = open() else { return };
    let data = object_script(&mut a, "obj/N_EX570_BTL.mdlx");
    let mut all = std::collections::BTreeSet::new();
    for param in [1, 2, 4, 5] {
        let (seen, unknown) = mode_cycle_with(&data, param);
        eprintln!("N_EX570_BTL spawn parameter {param}: modes {seen:?}");
        assert_eq!(unknown, 0, "every host function N_EX570_BTL calls is decoded");
        all.extend(seen);
    }
    for m in ["start_voice", "mode_battle", "btl_attack", "mode_revenge"] {
        assert!(all.contains(m), "mode {m} never ran: {all:?}");
    }
    assert!(all.iter().any(|m| m.starts_with("atk_")), "no attack: {all:?}");
}

/// Every BDX on the disc (object scripts, mission and area scripts, 00common.bdx).
fn all_scripts(a: &mut kh2_archive::Archive) -> Vec<(String, Vec<u8>)> {
    if let Ok(names) = std::fs::read_to_string("../../local/kh2idx.txt") {
        a.apply_names(names.lines()).unwrap();
    }
    let list: Vec<_> = a.entries().to_vec();
    let mut out = Vec::new();
    for l in &list {
        let name = l.name.clone().unwrap_or_default();
        let Ok(d) = a.read_entry(&l.entry) else { continue };
        if name.ends_with(".bdx") {
            out.push((name, d));
        } else if kh2_bar::is_bar(&d) {
            for e in kh2_bar::parse(&d).unwrap_or_default().iter().filter(|e| e.kind == 3 && !e.data.is_empty()) {
                out.push((format!("{name}:{}", e.tag), e.data.to_vec()));
            }
        }
    }
    out
}

#[test]
fn every_host_call_on_the_disc_has_a_signature() {
    let Some((mut a, elf)) = open() else { return };
    if std::fs::metadata("../../local/kh2idx.txt").is_err() {
        eprintln!("skipped: no file name list (kh2 fetch-names)");
        return;
    }
    let table = HostTable::from_exe(&elf).expect("table");
    let (mut scripts, mut calls, mut inferred_used) = (0, 0, std::collections::BTreeSet::new());
    for (name, data) in all_scripts(&mut a) {
        let p = Program::parse(&data).unwrap_or_else(|e| panic!("{name}: {e}"));
        let an = analyze(&p).unwrap();
        let mut rt = Runtime::new(p);
        rt.set_host_table(table.clone());
        scripts += 1;
        for (insn, _) in an.code.values() {
            if let kh2_bdx::Insn::Host { group, index } = insn {
                calls += 1;
                assert!(rt.core.signature(*group, *index).is_some(), "{name}: host {group}.{index} has no signature");
                if table.get(*group, *index).is_none() {
                    inferred_used.insert((*group, *index));
                }
            }
        }
    }
    eprintln!("{scripts} scripts, {calls} host calls, {} functions came from inference", inferred_used.len());
    // Only the entries the table lacks are ever inferred.
    assert!(inferred_used.iter().all(|&(g, i)| kh2_bdx::signatures::absent_from_table(g, i)), "{inferred_used:?}");
}

#[test]
fn every_script_on_the_disc_parses() {
    let Some((mut a, _)) = open() else { return };
    if let Ok(names) = std::fs::read_to_string("../../local/kh2idx.txt") {
        a.apply_names(names.lines()).unwrap();
    }
    let list: Vec<_> = a.entries().to_vec();
    let (mut ok, mut empty) = (0, 0);
    for l in &list {
        let Ok(d) = a.read_entry(&l.entry) else { continue };
        if !kh2_bar::is_bar(&d) {
            continue;
        }
        for e in kh2_bar::parse(&d).unwrap_or_default().iter().filter(|e| e.kind == 3) {
            if e.data.is_empty() {
                empty += 1;
                continue;
            }
            let p = Program::parse(e.data).unwrap_or_else(|er| panic!("{:?} {}: {er}", l.name, e.tag));
            analyze(&p).unwrap_or_else(|er| panic!("{:?} {}: {er}", l.name, e.tag));
            ok += 1;
        }
    }
    eprintln!("{ok} scripts parse, {empty} empty");
    if std::fs::metadata("../../local/kh2idx.txt").is_ok() {
        assert!(ok > 1000);
    }
}
