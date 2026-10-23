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

/// The functions the runtime answers itself without a `names.rs` row (the target search 6.3 / 6.4 / 6.5 /
/// 10.55 and the static vector 6.54) take the argument counts it reads, per the executable's table.
#[test]
fn runtime_group_6_signatures_match_the_executable() {
    let Some((_, elf)) = open() else { return };
    let table = HostTable::from_exe(&elf).expect("table");
    for (g, i, n, ret) in [(6, 3, 2, false), (6, 4, 1, false), (6, 5, 7, false), (10, 55, 2, false), (6, 54, 1, true)] {
        let s = table.get(g, i).unwrap_or_else(|| panic!("{g}.{i} missing"));
        assert_eq!((s.nargs, s.returns), (n, ret), "{g}.{i}");
    }
    assert!(table.search.is_some(), "the search numbers 0x383fb4..0x383fbc, 0x37a528 / 0x37a52c");
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
    dry_run_table(data, seed, distance, frames, damage_at, spawn_param, None)
}

/// As `dry_run_with` with the executable's host table given to the runtime (as the app does), so
/// functions `names.rs` does not carry (4.29 of the Struggle opponents) have their signatures.
fn dry_run_table(data: &[u8], seed: u32, distance: f32, frames: u32, damage_at: Option<u32>, spawn_param: i32, table: Option<&HostTable>) -> Dry {
    let p = Program::parse(data).unwrap();
    analyze(&p).unwrap();
    let mut rt = Runtime::new(p);
    if let Some(t) = table {
        rt.set_host_table(t.clone());
    }
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
    let (seen, unknown) = mode_cycle_table(data, spawn_param, None);
    (seen, unknown.values().sum())
}

/// As `mode_cycle_with` with the executable's host table; the calls that reached the host undecoded
/// are counted per function.
fn mode_cycle_table(data: &[u8], spawn_param: i32, table: Option<&HostTable>) -> (std::collections::BTreeSet<String>, std::collections::BTreeMap<(u16, u16), usize>) {
    let mut seen = std::collections::BTreeSet::new();
    let mut unknown = std::collections::BTreeMap::new();
    for distance in [150.0, 250.0, 450.0, 900.0] {
        for seed in 1..=6 {
            for damage_at in [None, Some(200)] {
                let d = dry_run_table(data, seed, distance, 700, damage_at, spawn_param, table);
                assert!(d.errors.is_empty(), "seed {seed} distance {distance}: {:?}", d.errors);
                for c in d.host.calls() {
                    if let Call::Unknown { group, index, .. } = c {
                        *unknown.entry((*group, *index)).or_insert(0) += 1;
                    }
                }
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


/// The Nobody M_EX940 (object 0x139, ObjectType 0x15) of tt04's b_44 and the M_EX850 it makes (WP-EN3): a blow-class hit
/// (hit reaction 7, 9 or 11, R016a at 0x01a7) runs R0082, which makes the object of objentry 0x6bf with host 1.0, stores
/// its handle at global +0x1084 (R011d) and puts it at the ground point under M_EX940 (1.98 then 1.148). On the mock world
/// the call order and the handle it hands to 1.148 are the whole of it.
#[test]
fn m_ex940_makes_m_ex850_on_a_blow() {
    let Some((mut a, _)) = open() else { return };
    assert_enemy_files(&mut a, "M_EX940");
    let data = object_script(&mut a, "obj/M_EX940.mdlx");
    let p = Program::parse(&data).unwrap();
    analyze(&p).unwrap();
    let mut rt = Runtime::new(p);
    rt.shared.rng = 3;
    rt.core.mode_log = Some(Vec::new());
    let mut host = MockHost::new();
    host.player_position = [250.0, 0.0, 0.0, 1.0];
    assert!(rt.spawn(&mut host, SELF_OBJ));
    for _ in 0..40 {
        rt.update(&mut host);
        host.tick();
    }
    assert!(host.created.is_empty(), "nothing is made before a hit: {:?}", host.created);
    host.hit_reaction = 7;
    rt.event(&mut host, 6, &[0x500]);
    for _ in 0..5 {
        rt.update(&mut host);
        host.tick();
    }
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
    assert_eq!(host.created, vec![0x6bf], "one object, the SpawnObject1 of M_EX940's objentry row");
    let made = kh2_bdx::mock::CREATED;
    let calls: Vec<&Call> = host.calls().collect();
    let at = |from: usize, pred: &dyn Fn(&Call) -> bool| from + calls[from..].iter().position(|c| pred(c)).expect("call missing");
    let create = at(0, &|c| matches!(c, Call::Func { group: 1, index: 0, .. }));
    let ground = at(create, &|c| matches!(c, Call::Func { group: 1, index: 98, .. }));
    let place = at(create, &|c| matches!(c, Call::Func { group: 1, index: 148, args } if args.first() == Some(&kh2_bdx::Arg::Obj(made))));
    assert!(create < ground && ground < place, "create_object, then ground_position, then set_position on the made object: {create} {ground} {place}");
    // The main loop unlinks again (R033b clears global +0x1070) and a second blow makes a second object; R00b6 first hides
    // the one made before (1.28 on global +0x1084, then set_flag 6 on it) because host 1.94 calls it valid.
    for _ in 0..3 {
        rt.update(&mut host);
        host.tick();
    }
    rt.event(&mut host, 6, &[0x500]);
    rt.update(&mut host);
    assert_eq!(host.created, vec![0x6bf, 0x6bf]);
    assert!(host.calls().any(|c| matches!(c, Call::Hide { obj } if *obj == made)), "the first object is hidden");
    assert!(host.calls().any(|c| matches!(c, Call::SetFlag { obj, bit: 6 } if *obj == made)), "and flagged 6");
}

/// The object M_EX940 makes, `M_EX850` (WP-EN3; objentry 0x6bf, ObjectType 4, no `set_obj_param`): its script runs on the
/// mock world with every host function decoded, starts in `idle` (it has an `appear` mode, which nothing enters on the mock
/// world) and leaves, and never asks for a status block.
#[test]
fn m_ex850_runs_its_modes() {
    let Some((mut a, _)) = open() else { return };
    assert_enemy_files(&mut a, "M_EX850");
    let data = object_script(&mut a, "obj/M_EX850.mdlx");
    let (seen, unknown) = mode_cycle(&data);
    eprintln!("M_EX850 modes: {seen:?}");
    assert_eq!(unknown, 0, "every host function M_EX850 calls is decoded");
    for m in ["idle", "leave"] {
        assert!(seen.contains(m), "mode {m} never ran: {seen:?}");
    }
    let d = dry_run(&data, 1, 250.0, 120, None);
    assert!(d.errors.is_empty(), "{:?}", d.errors);
    assert!(!d.host.calls().any(|c| matches!(c, Call::SetObjParam { .. })), "no status block is asked for");
}

/// The Struggle opponent N_EX570_BTL (WP-6H) with the spawn parameter 1 of its tt04 groups (b_41: 1,
/// b_42: 2, b_45: 4, b_51: 5; `docs/systems/script-host/contract.md`).
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

/// The files an enemy kind needs (model with a non-empty BDX entry, motion set, effect file).
fn assert_enemy_files(a: &mut kh2_archive::Archive, model: &str) {
    for ext in ["mdlx", "mset", "apdx"] {
        assert!(a.read(&format!("obj/{model}.{ext}")).is_ok(), "obj/{model}.{ext}");
    }
    let mdlx = a.read(&format!("obj/{model}.mdlx")).unwrap();
    assert!(kh2_bar::parse(&mdlx).unwrap().iter().any(|e| e.kind == 3 && !e.data.is_empty()), "{model}: BDX entry");
}

/// The Nobody M_EX930 (WP-7D; "Dancer" from its mode names) of tt04's group b_21 (spawn kind 2 there,
/// the app's own waves -1; host 1.110 picks between the two appear paths of R0568).
#[test]
fn m_ex930_runs_its_mode_cycle() {
    let Some((mut a, _)) = open() else { return };
    assert_enemy_files(&mut a, "M_EX930");
    let data = object_script(&mut a, "obj/M_EX930.mdlx");
    let (seen, unknown) = mode_cycle(&data);
    eprintln!("M_EX930 modes: {seen:?}");
    assert_eq!(unknown, 0, "every host function M_EX930 calls is decoded");
    for m in ["appear", "mode_battle", "btl_attack", "move_target", "mode_revenge"] {
        assert!(seen.contains(m), "mode {m} never ran: {seen:?}");
    }
    assert!(seen.iter().any(|m| m.starts_with("atk_spin")), "no spin attack: {seen:?}");
    // Its status block comes from enmp row 48 (R0f13 at 0x0f3a).
    let d = dry_run(&data, 1, 250.0, 60, None);
    assert!(d.host.calls().any(|c| matches!(c, Call::SetObjParam { a: 48, .. })), "set_obj_param 48");
}

/// The Struggle opponent N_EX500_BTL (WP-7D) of tt04's group b_50. With the executable's host table
/// (as the app runs it) the only functions `names.rs` does not decode are group 4's: 4.29 (the
/// Struggle manager's score, FUN_001d6738) and 4.0 / 4.1 (empty table entries, signatures inferred).
#[test]
fn n_ex500_btl_runs_its_mode_cycle() {
    let Some((mut a, elf)) = open() else { return };
    assert_enemy_files(&mut a, "N_EX500_BTL");
    let table = HostTable::from_exe(&elf).expect("table");
    let data = object_script(&mut a, "obj/N_EX500_BTL.mdlx");
    let (seen, unknown) = mode_cycle_table(&data, 99, Some(&table));
    eprintln!("N_EX500_BTL modes: {seen:?}; undecoded calls: {unknown:?}");
    assert!(unknown.keys().all(|&(g, i)| g == 4 && [0, 1, 29].contains(&i)), "{unknown:?}");
    for m in ["start_voice", "mode_battle", "check_mode", "move_target_near", "mode_revenge"] {
        assert!(seen.contains(m), "mode {m} never ran: {seen:?}");
    }
    assert!(seen.iter().any(|m| m.starts_with("atk_")), "no attack: {seen:?}");
    let d = dry_run_table(&data, 1, 250.0, 60, None, 99, Some(&table));
    assert!(d.host.calls().any(|c| matches!(c, Call::SetObjParam { a: 150, .. })), "set_obj_param 150");
}

/// The Struggle opponent N_EX600_BTL (WP-7D) of tt04's group b_52. Undecoded by `names.rs`: group 4's
/// 4.29 / 4.0 / 4.1 (as for N_EX500_BTL) and 1.95 (FUN_00183470, the ballistic fall of its
/// `atk_air_counter` hook).
#[test]
fn n_ex600_btl_runs_its_mode_cycle() {
    let Some((mut a, elf)) = open() else { return };
    assert_enemy_files(&mut a, "N_EX600_BTL");
    let table = HostTable::from_exe(&elf).expect("table");
    let data = object_script(&mut a, "obj/N_EX600_BTL.mdlx");
    let (seen, unknown) = mode_cycle_table(&data, 99, Some(&table));
    eprintln!("N_EX600_BTL modes: {seen:?}; undecoded calls: {unknown:?}");
    assert!(unknown.keys().all(|&k| [(4, 0), (4, 1), (4, 29), (1, 95)].contains(&k)), "{unknown:?}");
    for m in ["start_voice", "mode_battle", "check_mode", "btl_attack", "mode_revenge"] {
        assert!(seen.contains(m), "mode {m} never ran: {seen:?}");
    }
    assert!(seen.iter().any(|m| m.starts_with("atk_")), "no attack: {seen:?}");
    let d = dry_run_table(&data, 1, 250.0, 60, None, 99, Some(&table));
    assert!(d.host.calls().any(|c| matches!(c, Call::SetObjParam { a: 158, .. })), "set_obj_param 158");
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

/// Goofy's `turbo` (P_EX030 R0b5c, WP-S6) on the mock world with two target parts: the block at the
/// context +0x20 is set up by 6.3 (capacity 5), 10.55 and the script's own distance (0..500) and angle (2 pi)
/// writes; every 30 frames 6.5 searches with the history on (the nearest part first, then the other one,
/// since the history skips the first) into the target record of 2.23 (the host's memory), and the host is
/// told each answer (`Host::target_found`). No script error.
#[test]
fn goofy_turbo_searches_its_targets() {
    use kh2_bdx::host::{Host, Value};
    use kh2_bdx::search::TargetPart;
    /// What `target_found` was told: the record, the part found, the parts walked.
    type Found = (u32, Option<(u32, u8)>, u8);
    struct TurboHost {
        mock: MockHost,
        found: Vec<Found>,
        asked: Vec<(i32, bool)>,
    }
    impl Host for TurboHost {
        fn call(&mut self, c: &Call) -> Value {
            match c {
                Call::MotionState { .. } => {
                    self.mock.call(c);
                    Value::Int(self.mock.motion)
                }
                Call::Func { group: 1, index: 150, .. } => {
                    self.mock.call(c);
                    Value::Bool(true)
                }
                // Goofy stays where he is, so the distances stay as laid out.
                Call::SetMove { .. } => Value::None,
                // 1.247 random_free_point: the centre (the toy world is open).
                Call::Func { group: 1, index: 247, .. } => {
                    self.mock.call(c);
                    Value::Vec(self.mock.position)
                }
                _ => self.mock.call(c),
            }
        }
        fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
            self.mock.read_mem(addr, out)
        }
        fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
            self.mock.write_mem(addr, data)
        }
        fn target_parts(&mut self, kind: i32, skip_flagged: bool) -> Option<Vec<TargetPart>> {
            self.asked.push((kind, skip_flagged));
            Some(vec![TargetPart { obj: 0x900, part: 1, point: [0.0, 0.0, 450.0, 1.0] }, TargetPart { obj: 0xa00, part: 0, point: [200.0, 0.0, 0.0, 1.0] }, TargetPart { obj: 0xb00, part: 0, point: [700.0, 0.0, 0.0, 1.0] }])
        }
        fn target_found(&mut self, record: u32, found: Option<(u32, u8)>, walked: u8) {
            self.found.push((record, found, walked));
        }
    }
    let Some((mut a, elf)) = open() else { return };
    let data = object_script(&mut a, "obj/P_EX030.mdlx");
    let mut rt = Runtime::new(Program::parse(&data).unwrap());
    rt.set_host_table(HostTable::from_exe(&elf).expect("table"));
    let mut host = TurboHost { mock: MockHost::new(), found: Vec::new(), asked: Vec::new() };
    host.mock.motion_length = 200.0;
    assert!(rt.spawn(&mut host, SELF_OBJ));
    rt.set_mode_named(&mut host, "turbo");
    for _ in 0..200 {
        rt.update(&mut host);
        host.mock.tick();
    }
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
    eprintln!("turbo: searches {:?}", host.found);
    assert!(host.asked.iter().all(|&a| a == (0, true)), "kind 0 (enemies), the 0x8000 objects skipped (10.55): {:?}", host.asked);
    let found: Vec<Option<(u32, u8)>> = host.found.iter().map(|f| f.1).collect();
    assert!(found.len() >= 6, "a search every 30 frames: {found:?}");
    assert_eq!(&found[..3], &[Some((0xa00, 0)), Some((0x900, 1)), None], "nearest within 500, then the other, then none left (history of 5)");
    assert_eq!(found[5], Some((0xa00, 0)), "after five searches 6.4 empties the history (R0c12)");
    assert!(host.found.iter().all(|f| f.0 == kh2_bdx::mock::TARGET && f.2 == 3), "the target record of 2.23, three parts walked");
    // The record in the host's memory: object, part, 0, -1 (FUN_0018f490).
    let mut rec = [0u8; 16];
    assert!(host.mock.read_mem(kh2_bdx::mock::TARGET, &mut rec));
    assert_eq!(&rec[12..16], &[0xff; 4]);
}

/// Goofy's `blade` (P_EX030 R0d30, WP-S5) on the mock world with a hand object: the child thread R0d94
/// takes the object off the hand (1.161 / 1.102), places it at its bone 3 (1.83, 1.148), flies it with
/// 1.89 and its attack 1161 (2.10) whose hit routine (2.38, R10f7) the host calls as FUN_0018cf50 does
/// (`Runtime::call_routine`); the routine sets the context's hit word and names the victim in the target
/// record (1.160), the shield waits on the target, the thread ends, the main routine links the object back
/// (1.18) and plays motion 193. No script error (the base run stopped at 0x0e14 on a 0 hand object).
#[test]
fn goofy_blade_throws_and_takes_back_the_hand_object() {
    use kh2_bdx::host::{Arg, Host, Value};
    const WEAPON: u32 = 0x600;
    const ATTACK: u32 = 0x700;
    const VICTIM: u32 = 0x800;
    struct BladeHost {
        mock: MockHost,
        linked: bool,
        callback: Option<(u32, u32)>,
    }
    impl Host for BladeHost {
        fn call(&mut self, c: &Call) -> Value {
            let word = |a: &Arg| a.int() as u32;
            match c {
                Call::Func { group: 1, index: 234, .. } => {
                    self.mock.log.push((self.mock.frame, c.clone()));
                    Value::Handle(WEAPON)
                }
                Call::Func { group: 1, index: 161, args } if args.first().map(word) == Some(WEAPON) => Value::Bool(self.linked),
                Call::Func { group: 1, index: 102, args } if args.first().map(word) == Some(WEAPON) => {
                    self.linked = false;
                    self.mock.call(c)
                }
                Call::Func { group: 1, index: 18, args } if args.first().map(word) == Some(WEAPON) => {
                    self.linked = true;
                    self.mock.call(c)
                }
                Call::Func { group: 1, index: 83, .. } => {
                    self.mock.call(c);
                    Value::Vec([10.0, -100.0, 20.0, 1.0])
                }
                Call::Func { group: 1, index: 94 | 140 | 151, .. } => {
                    self.mock.call(c);
                    Value::Bool(true)
                }
                Call::Func { group: 2, index: 10, .. } => {
                    self.mock.call(c);
                    Value::Handle(ATTACK)
                }
                Call::Func { group: 2, index: 38, args } => {
                    self.callback = Some((word(&args[1]), word(&args[2])));
                    self.mock.call(c)
                }
                _ => self.mock.call(c),
            }
        }
        fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
            self.mock.read_mem(addr, out)
        }
        fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
            self.mock.write_mem(addr, data)
        }
    }
    let Some((mut a, elf)) = open() else { return };
    let data = object_script(&mut a, "obj/P_EX030.mdlx");
    let mut rt = Runtime::new(Program::parse(&data).unwrap());
    rt.set_host_table(HostTable::from_exe(&elf).expect("table"));
    rt.core.mode_log = Some(Vec::new());
    let mut host = BladeHost { mock: MockHost::new(), linked: true, callback: None };
    assert!(rt.spawn(&mut host, SELF_OBJ));
    rt.set_mode_named(&mut host, "blade");
    let mut called = None;
    for frame in 0..400 {
        if called.is_none()
            && frame > 30
            && let Some((pc, arg)) = host.callback
        {
            rt.call_routine(&mut host, pc, &[arg, VICTIM, ATTACK]);
            called = Some(frame);
        }
        rt.update(&mut host);
        host.mock.tick();
    }
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
    let calls: Vec<&Call> = host.mock.calls().collect();
    let on_weapon = |g: u16, i: u16| calls.iter().any(|c| matches!(c, Call::Func { group, index, args } if *group == g && *index == i && args.first().is_some_and(|a| a.int() as u32 == WEAPON)));
    assert!(on_weapon(1, 102) && on_weapon(1, 148) && on_weapon(1, 18), "the hand object is taken off, placed and linked back");
    assert!(calls.iter().any(|c| matches!(c, Call::SetVelocity { obj, speed, .. } if *obj == WEAPON && *speed > 0.0)), "it flies");
    assert!(called.is_some(), "the attack's hit routine was registered (2.38)");
    assert!(calls.iter().any(|c| matches!(c, Call::Unknown { group: 1, index: 160, args } if args.get(1) == Some(&VICTIM))), "the hit routine names the victim (1.160)");
    assert!(calls.iter().any(|c| matches!(c, Call::PlayMotion { obj: SELF_OBJ, motion: 193, .. })), "the catch motion 193 after the throw");
    assert!(host.linked, "back on the hand");
    let modes = rt.core.mode_log.take().unwrap();
    eprintln!("blade: callback at frame {called:?}, modes {modes:?}");
}

/// Stitch's summon script (`obj/N_HB040_BTL`, WP-SUM4, docs/systems/drive-limits-summons/summons-stages.md stage 4) in its `appear_skip` mode on
/// the mock world, with the group 10 entries it reaches answered as the app's summon host answers them:
/// every group 10 call is decoded by its `names.rs` signature (none reaches the host as `Unknown`), the
/// battle start asks the friend AI for slots (10.12), and the main thread offers Ohana (1.165, slot 2,
/// command 519) only while the timer +0x1b4 of the player's status block (1.84 then 10.45) is above 0 and
/// the summon is in no limit (10.19).
#[test]
fn genie_summon_script_offers_the_form_limit_while_the_timer_runs() {
    use kh2_bdx::host::{Host, Value};
    const GAUGES: u32 = 0x7900_0000;
    /// The pairs of the table at 0x3548f0 as FUN_00119ca8 reads them (form command, limit command).
    fn limit_of(elf: &[u8], cmd: i32) -> i32 {
        let seg = |va: u32| -> Option<i16> {
            let u32_ = |o: usize| u32::from_le_bytes(elf[o..o + 4].try_into().unwrap());
            let (phoff, n) = (u32_(0x1c) as usize, u16::from_le_bytes([elf[0x2c], elf[0x2d]]) as usize);
            let size = u16::from_le_bytes([elf[0x2a], elf[0x2b]]) as usize;
            (0..n).map(|i| phoff + i * size).find_map(|p| {
                let (off, base, len) = (u32_(p + 4), u32_(p + 8), u32_(p + 16));
                (u32_(p) == 1 && va >= base && va + 2 <= base + len).then(|| i16::from_le_bytes([elf[(off + va - base) as usize], elf[(off + va - base) as usize + 1]]))
            })
        };
        (0..5).find_map(|k| (seg(0x3548f0 + 4 * k)? as i32 == cmd).then(|| seg(0x3548f2 + 4 * k)).flatten()).map_or(0, |v| v as i32)
    }
    struct GenieHost {
        mock: MockHost,
        timer: f32,
        in_limit: bool,
        cmd: i32,
        limit: i32,
        seen: Vec<Call>,
    }
    impl Host for GenieHost {
        fn call(&mut self, c: &Call) -> Value {
            self.seen.push(c.clone());
            match c {
                Call::Func { group: 1, index: 94, .. } => Value::Bool(true),
                Call::Func { group: 1, index: 84, .. } => Value::Handle(GAUGES),
                Call::Func { group: 10, index: 45, args } if args[0].int() as u32 == GAUGES => Value::Float(self.timer),
                Call::Func { group: 10, index: 19, .. } => Value::Bool(self.in_limit),
                Call::Func { group: 1, index: 272, .. } => Value::Bool(false),
                // 10.10: the limit of the form command at AI +0x14 (set by the first update, FUN_00119a40).
                Call::Func { group: 10, index: 10, .. } => Value::Int(if self.cmd != 0 { self.limit } else { 0 }),
                Call::Func { group: 10, .. } | Call::Func { group: 1, index: 165, .. } => Value::None,
                _ => self.mock.call(c),
            }
        }
        fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
            self.mock.read_mem(addr, out)
        }
        fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
            self.mock.write_mem(addr, data)
        }
    }
    let Some((mut a, elf)) = open() else { return };
    let table = HostTable::from_exe(&elf).expect("table");
    let data = object_script(&mut a, "obj/P_AL010.mdlx");
    let limit = limit_of(&elf, 536);
    assert_eq!(limit, 544, "Wisdom Genie's limit (the table at 0x3548f0)");
    for (timer, in_limit, cmd, offers) in [(600.0f32, false, 536, true), (0.0, false, 536, false), (600.0, true, 536, false), (600.0, false, 0, true)] {
        let mut rt = Runtime::new(Program::parse(&data).expect("P_AL010 parses"));
        rt.set_host_table(table.clone());
        let mut h = GenieHost { mock: MockHost::new(), timer, in_limit, cmd, limit, seen: Vec::new() };
        rt.spawn(&mut h, SELF_OBJ);
        rt.set_mode_named(&mut h, "appear_skip");
        for _ in 0..120 {
            h.mock.tick();
            rt.update(&mut h);
        }
        assert!(rt.core.errors.is_empty(), "timer {timer} limit {in_limit}: {:?}", rt.core.errors);
        let unknown10: Vec<_> = h.seen.iter().filter(|c| matches!(c, Call::Unknown { group: 10, .. })).collect();
        assert!(unknown10.is_empty(), "group 10 calls without a signature: {unknown10:?}");
        let want = if cmd != 0 { limit } else { 0 };
        let offered = h.seen.iter().filter(|c| matches!(c, Call::Func { group: 1, index: 165, args } if args[0].int() == 2 && args[1].int() == want)).count();
        let limits = h.seen.iter().filter(|c| matches!(c, Call::Func { group: 10, index: 10, .. })).count();
        eprintln!("timer {timer} limit {in_limit} form {cmd}: {} calls, {limits} 10.10 calls, {offered} offers of {want}", h.seen.len());
        assert!(limits > 0, "the main thread asks for the form's limit (10.10)");
        assert_eq!(offered > 0, offers, "timer {timer} limit {in_limit}: offered {offered} times");
    }
}

#[test]
fn stitch_summon_script_offers_ohana_while_the_timer_runs() {
    use kh2_bdx::host::{Host, Value};
    const GAUGES: u32 = 0x7900_0000;
    struct StitchHost {
        mock: MockHost,
        timer: f32,
        in_limit: bool,
        slots: i32,
        seen: Vec<Call>,
    }
    impl Host for StitchHost {
        fn call(&mut self, c: &Call) -> Value {
            self.seen.push(c.clone());
            match c {
                // 1.94 obj_valid: the summon and the player exist.
                Call::Func { group: 1, index: 94, .. } => Value::Bool(true),
                Call::Func { group: 1, index: 84, .. } => Value::Handle(GAUGES),
                Call::Func { group: 10, index: 45, args } if args[0].int() as u32 == GAUGES => Value::Float(self.timer),
                Call::Func { group: 10, index: 19, .. } => Value::Bool(self.in_limit),
                Call::Func { group: 1, index: 272, .. } => Value::Bool(false),
                Call::Func { group: 10, index: 12, .. } => {
                    self.slots += 1;
                    Value::Int(self.slots - 1)
                }
                Call::Func { group: 10, .. } | Call::Func { group: 1, index: 165, .. } => Value::None,
                _ => self.mock.call(c),
            }
        }
        fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
            self.mock.read_mem(addr, out)
        }
        fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
            self.mock.write_mem(addr, data)
        }
    }
    let Some((mut a, elf)) = open() else { return };
    let table = HostTable::from_exe(&elf).expect("table");
    let data = object_script(&mut a, "obj/N_HB040_BTL.mdlx");
    for (timer, in_limit, offers) in [(600.0f32, false, true), (0.0, false, false), (600.0, true, false)] {
        let mut rt = Runtime::new(Program::parse(&data).expect("N_HB040_BTL parses"));
        rt.set_host_table(table.clone());
        let mut h = StitchHost { mock: MockHost::new(), timer, in_limit, slots: 0, seen: Vec::new() };
        rt.spawn(&mut h, SELF_OBJ);
        rt.set_mode_named(&mut h, "appear_skip");
        for _ in 0..120 {
            h.mock.tick();
            rt.update(&mut h);
        }
        assert!(rt.core.errors.is_empty(), "timer {timer} limit {in_limit}: {:?}", rt.core.errors);
        let unknown10: Vec<_> = h.seen.iter().filter(|c| matches!(c, Call::Unknown { group: 10, .. })).collect();
        assert!(unknown10.is_empty(), "group 10 calls without a signature: {unknown10:?}");
        assert!(h.slots > 0, "the battle start asks the friend AI for slots (10.12)");
        let ohana = h.seen.iter().filter(|c| matches!(c, Call::Func { group: 1, index: 165, args } if args[0].int() == 2 && args[1].int() == 519)).count();
        eprintln!("timer {timer} limit {in_limit}: {} calls, {} slots, {ohana} Ohana offers", h.seen.len(), h.slots);
        assert_eq!(ohana > 0, offers, "timer {timer} limit {in_limit}: Ohana offered {ohana} times");
    }
}
/// Chicken Little's script (P_EX350) keeps a host 2.21 buffer (key 48, 12 bytes) at its global +0x14 and
/// stores its attack handle at the buffer's +4 (0x1620..0x1637). A 2.21 answered with 0 makes that store
/// (and the read at 0x1264) reach address 4: the `BadAddress(4)` of WP-SUM5's run. With the buffer of
/// FUN_001a5040 (`kh2_bdx::shared`) the script runs without an error.
#[test]
fn chicken_little_script_needs_the_shared_block() {
    use kh2_bdx::host::{Host, Value};
    use kh2_bdx::shared::SharedBlocks;
    const GAUGES: u32 = 0x7900_0000;
    struct ChickenHost {
        mock: MockHost,
        shared: Option<SharedBlocks>,
        seen: Vec<Call>,
    }
    impl Host for ChickenHost {
        fn call(&mut self, c: &Call) -> Value {
            self.seen.push(c.clone());
            match c {
                Call::Func { group: 2, index: 21, args } => match self.shared.as_mut() {
                    Some(s) => Value::Handle(s.get(args[0].int(), args[1].int())),
                    None => Value::Int(0),
                },
                Call::Func { group: 1, index: 94, .. } => Value::Bool(true),
                Call::Func { group: 1, index: 84, .. } => Value::Handle(GAUGES),
                Call::Func { group: 10, index: 45, .. } => Value::Float(600.0),
                Call::Func { group: 10, index: 19, .. } | Call::Func { group: 1, index: 272, .. } => Value::Bool(false),
                Call::Func { group: 10, .. } | Call::Func { group: 1, index: 165, .. } => Value::None,
                _ => self.mock.call(c),
            }
        }
        fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
            self.shared.as_mut().is_some_and(|s| s.read(addr, out)) || self.mock.read_mem(addr, out)
        }
        fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
            self.shared.as_mut().is_some_and(|s| s.write(addr, data)) || self.mock.write_mem(addr, data)
        }
    }
    let Some((mut a, elf)) = open() else { return };
    let table = HostTable::from_exe(&elf).expect("table");
    let data = object_script(&mut a, "obj/P_EX350.mdlx");
    for with in [false, true] {
        let mut rt = Runtime::new(Program::parse(&data).expect("P_EX350 parses"));
        rt.set_host_table(table.clone());
        let mut h = ChickenHost { mock: MockHost::new(), shared: with.then(SharedBlocks::default), seen: Vec::new() };
        rt.spawn(&mut h, SELF_OBJ);
        rt.set_mode_named(&mut h, "appear_skip");
        for _ in 0..120 {
            h.mock.tick();
            rt.update(&mut h);
        }
        let asked: Vec<_> = h.seen.iter().filter_map(|c| match c { Call::Func { group: 2, index: 21, args } => Some((args[0].int(), args[1].int())), _ => None }).collect();
        eprintln!("shared block {with}: 2.21 asked {asked:?}, errors {:?}", rt.core.errors);
        assert!(asked.contains(&(48, 12)), "the attack routine asks for key 48, 12 bytes");
        let bad4 = rt.core.errors.iter().any(|e| format!("{e:?}").contains("BadAddress(4)"));
        if with {
            assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
            assert_eq!(h.shared.as_ref().unwrap().len(), 1);
        } else {
            assert!(bad4, "without the buffer the store reaches address 4: {:?}", rt.core.errors);
        }
    }
}

/// Peter Pan's script (P_EX330) asks host 2.75 (FUN_001c69b0, Tinker Bell's revive ready); with the
/// signature row the call decodes and the script runs in `appear_skip` either way without an error.
#[test]
fn peter_pan_script_asks_whether_tinker_bell_is_ready() {
    use kh2_bdx::host::{Host, Value};
    const GAUGES: u32 = 0x7900_0000;
    struct PeterHost {
        mock: MockHost,
        ready: bool,
        seen: Vec<Call>,
    }
    impl Host for PeterHost {
        fn call(&mut self, c: &Call) -> Value {
            self.seen.push(c.clone());
            match c {
                Call::Func { group: 2, index: 75, .. } => Value::Bool(self.ready),
                // 0.49 effect_alive: the effect the thread watches lives (the loop at 0x14c8 asks 2.75 then).
                Call::Func { group: 0, index: 49, .. } => Value::Bool(true),
                Call::Func { group: 1, index: 94, .. } => Value::Bool(true),
                Call::Func { group: 1, index: 84, .. } => Value::Handle(GAUGES),
                Call::Func { group: 10, index: 45, .. } => Value::Float(600.0),
                Call::Func { group: 10, index: 19, .. } | Call::Func { group: 1, index: 272, .. } => Value::Bool(false),
                Call::Func { group: 10, .. } | Call::Func { group: 1, index: 165, .. } => Value::None,
                _ => self.mock.call(c),
            }
        }
        fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
            self.mock.read_mem(addr, out)
        }
        fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
            self.mock.write_mem(addr, data)
        }
    }
    let Some((mut a, elf)) = open() else { return };
    let table = HostTable::from_exe(&elf).expect("table");
    let data = object_script(&mut a, "obj/P_EX330.mdlx");
    for ready in [true, false] {
        let mut rt = Runtime::new(Program::parse(&data).expect("P_EX330 parses"));
        rt.set_host_table(table.clone());
        let mut h = PeterHost { mock: MockHost::new(), ready, seen: Vec::new() };
        rt.spawn(&mut h, SELF_OBJ);
        rt.set_mode_named(&mut h, "appear_skip");
        for _ in 0..120 {
            h.mock.tick();
            rt.update(&mut h);
        }
        let asked = h.seen.iter().filter(|c| matches!(c, Call::Func { group: 2, index: 75, .. })).count();
        let unknown2: Vec<_> = h.seen.iter().filter(|c| matches!(c, Call::Unknown { group: 2, .. })).collect();
        eprintln!("ready {ready}: 2.75 asked {asked} times, {} calls, errors {:?}", h.seen.len(), rt.core.errors);
        assert!(unknown2.is_empty(), "group 2 calls without a signature: {unknown2:?}");
        assert!(asked > 0, "the script asks 2.75");
        assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
    }
}

/// Peter Pan's script (P_EX330) in `appear_skip` with Tinker Bell ready: it heals the player through host 1.155
/// (`obj_method_78`: object, amount 5, part 0, flag 0) and sets the effect's second vector through host 0.40
/// (`effect_set_vec2_raw`, a vector argument). Both calls decode with their signature rows (no `Unknown`) and the
/// script runs on without an error.
#[test]
fn peter_pan_script_heals_the_player_and_sets_the_effect_vector() {
    use kh2_bdx::host::{Arg, Host, Value};
    struct PeterHost {
        mock: MockHost,
        seen: Vec<Call>,
    }
    impl Host for PeterHost {
        fn call(&mut self, c: &Call) -> Value {
            self.seen.push(c.clone());
            match c {
                Call::Func { group: 2, index: 75, .. } | Call::Func { group: 0, index: 49, .. } | Call::Func { group: 1, index: 94, .. } => Value::Bool(true),
                Call::Func { group: 1, index: 84, .. } => Value::Handle(0x7900_0000),
                Call::Func { group: 10, index: 45, .. } => Value::Float(600.0),
                Call::Func { group: 10, index: 19, .. } | Call::Func { group: 1, index: 272, .. } => Value::Bool(false),
                Call::Func { group: 1, index: 155, .. } => Value::Int(15),
                Call::Func { group: 10, .. } | Call::Func { group: 1, index: 165, .. } | Call::Func { group: 0, index: 40, .. } => Value::None,
                _ => self.mock.call(c),
            }
        }
        fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
            self.mock.read_mem(addr, out)
        }
        fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
            self.mock.write_mem(addr, data)
        }
    }
    let Some((mut a, elf)) = open() else { return };
    let table = HostTable::from_exe(&elf).expect("table");
    let data = object_script(&mut a, "obj/P_EX330.mdlx");
    let mut rt = Runtime::new(Program::parse(&data).expect("P_EX330 parses"));
    rt.set_host_table(table);
    let mut h = PeterHost { mock: MockHost::new(), seen: Vec::new() };
    rt.spawn(&mut h, SELF_OBJ);
    rt.set_mode_named(&mut h, "appear_skip");
    for _ in 0..120 {
        h.mock.tick();
        rt.update(&mut h);
    }
    let heals: Vec<_> = h.seen.iter().filter_map(|c| match c { Call::Func { group: 1, index: 155, args } => Some(args.clone()), _ => None }).collect();
    let vecs: Vec<_> = h.seen.iter().filter_map(|c| match c { Call::Func { group: 0, index: 40, args } => Some(args.clone()), _ => None }).collect();
    eprintln!("1.155: {heals:?}; 0.40 x{} first {:?}; errors {:?}", vecs.len(), vecs.first(), rt.core.errors);
    assert!(!heals.is_empty(), "the script heals through 1.155");
    assert!(heals.iter().all(|x| x.len() == 4 && x[1].int() == 5 && x[2].int() == 0 && x[3].int() == 0), "{heals:?}");
    assert!(!vecs.is_empty(), "the script sets the vector through 0.40");
    assert!(vecs.iter().all(|x| matches!(x.get(1), Some(Arg::Vec(_)))), "{vecs:?}");
    assert!(!h.seen.iter().any(|c| matches!(c, Call::Unknown { group: 0, index: 40, .. } | Call::Unknown { group: 1, index: 155, .. })));
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
}

/// Peter Pan's `appear` mode (P_EX330, R16b9) makes Tinker Bell with host 1.0 (objentry 1268) and switches the object it
/// made to its own `appear` mode with host 1.8 (`set_mode`, FUN_00170ea0 -> FUN_00186db0 on the mode machine of the object the
/// context names). The context names another object than the caller, so the runtime hands the host `RemoteSetMode` with
/// the made object's handle instead of switching its own modes.
#[test]
fn peter_pan_makes_tinker_bell_and_sets_her_mode_remotely() {
    use kh2_bdx::host::{Host, Value};
    struct H {
        mock: MockHost,
        seen: Vec<Call>,
    }
    impl Host for H {
        fn call(&mut self, c: &Call) -> Value {
            self.seen.push(c.clone());
            match c {
                Call::Func { group: 2, index: 75, .. } | Call::Func { group: 0, index: 49, .. } => Value::Bool(true),
                Call::Func { group: 1, index: 84, .. } => Value::Handle(0x7900_0000),
                Call::Func { group: 10, index: 45, .. } => Value::Float(600.0),
                Call::Func { group: 10, index: 19, .. } | Call::Func { group: 1, index: 272, .. } => Value::Bool(false),
                Call::Func { group: 10, .. } | Call::Func { group: 1, index: 165, .. } | Call::Func { group: 0, index: 40, .. } => Value::None,
                _ => self.mock.call(c),
            }
        }
        fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
            self.mock.read_mem(addr, out)
        }
        fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
            self.mock.write_mem(addr, data)
        }
    }
    let Some((mut a, elf)) = open() else { return };
    let table = HostTable::from_exe(&elf).expect("table");
    let data = object_script(&mut a, "obj/P_EX330.mdlx");
    let mut rt = Runtime::new(Program::parse(&data).expect("P_EX330 parses"));
    rt.set_host_table(table);
    let mut h = H { mock: MockHost::new(), seen: Vec::new() };
    rt.spawn(&mut h, SELF_OBJ);
    rt.set_mode_named(&mut h, "appear");
    for _ in 0..40 {
        h.mock.tick();
        rt.update(&mut h);
    }
    assert_eq!(h.mock.created, vec![1268], "Tinker Bell, objentry 1268, made once");
    let made = kh2_bdx::mock::CREATED;
    let remote: Vec<_> = h.seen.iter().filter_map(|c| match c { Call::RemoteSetMode { obj, name } => Some((*obj, name.clone())), _ => None }).collect();
    assert_eq!(remote, vec![(made, "appear".to_string())], "her mode is asked of the made object, not switched on Peter Pan");
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
}

/// Chicken Little's `whistle` mode (WP-SUM8) walks the enemies (2.4), clones the object's target record for each in range
/// (1.131: a pool record the host serves from memory) and starts a detached thread (2.20) that reads the record's object
/// (word 0) as a context and attacks it. With the host answering the walk, the record and the object queries the threads
/// run without a bad address.
#[test]
fn chicken_little_whistle_threads_run() {
    use kh2_bdx::host::{Host, Value};
    use kh2_bdx::shared::SharedBlocks;
    const GAUGES: u32 = 0x7900_0000;
    const ENEMIES: [u32; 2] = [0x7e00_0001, 0x7e00_0002];
    const RECORDS: u32 = 0x0600_0000;
    struct H {
        mock: MockHost,
        shared: SharedBlocks,
        seen: Vec<Call>,
        records: Vec<[u8; 20]>,
    }
    impl Host for H {
        fn call(&mut self, c: &Call) -> Value {
            self.seen.push(c.clone());
            match c {
                Call::Func { group: 2, index: 21, args } => Value::Handle(self.shared.get(args[0].int(), args[1].int())),
                Call::Func { group: 2, index: 4, args } => {
                    let prev = args[0].int() as u32;
                    Value::Handle(match prev {
                        0 => ENEMIES[0],
                        p if p == ENEMIES[0] => ENEMIES[1],
                        _ => 0,
                    })
                }
                Call::Position { obj } if ENEMIES.contains(obj) => Value::Vec([300.0, 0.0, 100.0, 1.0]),
                // 1.131: a record from the pool naming the first enemy.
                Call::FreeTarget { .. } => {
                    let mut r = [0u8; 20];
                    r[0..4].copy_from_slice(&ENEMIES[0].to_le_bytes());
                    r[12..16].copy_from_slice(&(-1i32).to_le_bytes());
                    self.records.push(r);
                    Value::Handle(RECORDS + 0x20 * (self.records.len() as u32 - 1))
                }
                Call::Func { group: 1, index: 94, .. } => Value::Bool(true),
                Call::Func { group: 1, index: 84, .. } => Value::Handle(GAUGES),
                Call::Func { group: 10, index: 45, .. } => Value::Float(600.0),
                Call::Func { group: 10, index: 19, .. } | Call::Func { group: 1, index: 272, .. } => Value::Bool(false),
                Call::Func { group: 10, .. } | Call::Func { group: 1, index: 165, .. } => Value::None,
                _ => self.mock.call(c),
            }
        }
        fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
            let r = addr.wrapping_sub(RECORDS);
            if let Some(rec) = self.records.get((r / 0x20) as usize) {
                let k = (r % 0x20) as usize;
                if let Some(b) = rec.get(k..k + out.len()) {
                    out.copy_from_slice(b);
                    return true;
                }
            }
            self.shared.read(addr, out) || self.mock.read_mem(addr, out)
        }
        fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
            self.shared.write(addr, data) || self.mock.write_mem(addr, data)
        }
    }
    let Some((mut a, elf)) = open() else { return };
    let table = HostTable::from_exe(&elf).expect("table");
    let data = object_script(&mut a, "obj/P_EX350.mdlx");
    let mut rt = Runtime::new(Program::parse(&data).expect("P_EX350 parses"));
    rt.set_host_table(table);
    let mut h = H { mock: MockHost::new(), shared: SharedBlocks::default(), seen: Vec::new(), records: Vec::new() };
    rt.spawn(&mut h, SELF_OBJ);
    rt.set_mode_named(&mut h, "whistle");
    for _ in 0..200 {
        h.mock.tick();
        rt.update(&mut h);
    }
    let clones = h.seen.iter().filter(|c| matches!(c, Call::FreeTarget { .. })).count();
    let attacks = h.seen.iter().filter(|c| matches!(c, Call::Func { group: 2, index: 10, .. })).count();
    eprintln!("1.131 x{clones}, attack objects {attacks}; errors {:?}", rt.core.errors);
    assert!(clones >= 1, "the whistle clones a target record per enemy in range");
    assert!(attacks >= 1, "a thread read the record's object and made its attack object");
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
}

/// Genie's "Drive!" (WP-SUM8): the generic executor raises event 11 on his script with (the player, the form command,
/// a target record). For each of the four form commands 535..538 the handler plays motion 241, queues motion 0 and
/// calls host 10.9 with the command; another command does nothing.
#[test]
fn genie_drive_event_plays_the_form_motion_and_sets_the_form() {
    use kh2_bdx::host::{Host, Value};
    struct H {
        mock: MockHost,
        seen: Vec<Call>,
    }
    impl Host for H {
        fn call(&mut self, c: &Call) -> Value {
            self.seen.push(c.clone());
            match c {
                Call::Func { group: 1, index: 94, .. } => Value::Bool(true),
                Call::Func { group: 1, index: 84, .. } => Value::Handle(0x7900_0000),
                Call::Func { group: 10, index: 45, .. } => Value::Float(600.0),
                Call::Func { group: 10, index: 19, .. } | Call::Func { group: 1, index: 272, .. } => Value::Bool(false),
                Call::Func { group: 10, index: 10, .. } => Value::Int(0),
                Call::Func { group: 10, .. } | Call::Func { group: 1, index: 165, .. } => Value::None,
                _ => self.mock.call(c),
            }
        }
        fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
            self.mock.read_mem(addr, out)
        }
        fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
            self.mock.write_mem(addr, data)
        }
    }
    let Some((mut a, elf)) = open() else { return };
    let table = HostTable::from_exe(&elf).expect("table");
    let data = object_script(&mut a, "obj/P_AL010.mdlx");
    for (cmd, handled) in [(535, true), (536, true), (537, true), (538, true), (539, false), (0, false)] {
        let mut rt = Runtime::new(Program::parse(&data).expect("P_AL010 parses"));
        rt.set_host_table(table.clone());
        let mut h = H { mock: MockHost::new(), seen: Vec::new() };
        rt.spawn(&mut h, SELF_OBJ);
        rt.set_mode_named(&mut h, "appear_skip");
        for _ in 0..40 {
            h.mock.tick();
            rt.update(&mut h);
        }
        h.seen.clear();
        assert!(rt.event(&mut h, 11, &[0x200, cmd, 0x600_0000]), "Genie's script has an event 11 handler");
        let motions: Vec<i32> = h.seen.iter().filter_map(|c| match c { Call::PlayMotion { motion, .. } => Some(*motion), _ => None }).collect();
        let forms: Vec<i32> = h.seen.iter().filter_map(|c| match c { Call::Func { group: 10, index: 9, args } => Some(args[1].int()), _ => None }).collect();
        eprintln!("command {cmd}: motions {motions:?}, 10.9 {forms:?}");
        assert_eq!(motions.contains(&241), handled, "command {cmd}");
        assert_eq!(forms, if handled { vec![cmd as i32] } else { vec![] }, "command {cmd}");
        assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
    }
}
