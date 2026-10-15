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