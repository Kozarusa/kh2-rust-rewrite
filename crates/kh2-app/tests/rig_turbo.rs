//! A turbo run of `scenarios/movement/tt-run-06` from the user's state writes one trace row per (k, name) for k = 0 ..= frames.
//! Skipped without the disc (`KH2_ISO`) or the user's state 06.

use std::path::PathBuf;

#[test]
fn a_turbo_run_of_tt_run_06_writes_the_expected_trace() {
    let (Some(iso), Some(profile)) = (std::env::var_os("KH2_ISO").map(PathBuf::from), std::env::var_os("USERPROFILE")) else { return };
    let states = std::env::var_os("KH2_SSTATES").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(profile).join("Documents/PCSX2/sstates"));
    let state = states.join("SLUS-21005 (DA0535FD).06.p2s");
    if !iso.exists() || !state.exists() {
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sc = kh2_rig::scenario::Scenario::load(&root.join("scenarios/movement/tt-run-06.toml")).unwrap();
    let ee = kh2_rig::p2s::P2s::open(&state).unwrap().ee;
    let dir = std::env::temp_dir().join(format!("kh2-rig-turbo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let json = dir.join("state.json");
    std::fs::write(&json, kh2_rig::save::merged_json(&ee, "state.06.p2s", None).unwrap()).unwrap();
    let trace = dir.join("trace-port.csv");
    let names = ["sora.pos", "sora.facing", "frame.step"];
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_kh2-game"))
        .arg(sc.port_area.as_deref().unwrap())
        .current_dir(&dir)
        .env("KH2_TURBO", "1")
        .env("KH2_STATE", &json)
        .env("KH2_PAD", sc.pad_path().unwrap())
        .env("KH2_TRACE", names.join(","))
        .env("KH2_TRACE_AT", "every")
        .env("KH2_TRACE_FILE", &trace)
        .env("KH2_DRIVE_SEED", "1")
        .env("KH2_AUTO_LIMIT_SEED", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "{status}");
    let text = std::fs::read_to_string(&trace).unwrap();
    let t = kh2_rig::trace::Trace::parse(&text).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(t.rows.len(), (sc.frames as usize + 1) * names.len(), "one row per k in 0..={} and name", sc.frames);
    assert_eq!(t.get(0, "sora.pos").unwrap().value, "650;0;250", "the state's position");
    assert!(t.get(sc.frames, "sora.pos").is_some_and(|r| !r.value.is_empty()));
}
