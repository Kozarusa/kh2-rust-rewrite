//! The scenario files under `scenarios/` parse, validate against `scenarios/values.toml`, and their pad scripts parse and cover the run.

use kh2_rig::client::load_scenario;
use std::path::PathBuf;

fn scenarios() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scenarios")
}

fn all() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in std::fs::read_dir(scenarios()).unwrap() {
        let dir = dir.unwrap().path();
        if dir.is_dir() {
            for f in std::fs::read_dir(&dir).unwrap() {
                let f = f.unwrap().path();
                if f.extension().is_some_and(|e| e == "toml") {
                    out.push(f);
                }
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_shipped_scenario_loads_and_validates() {
    let files = all();
    assert!(files.len() >= 2, "{files:?}");
    for f in files {
        let (sc, values, pad) = load_scenario(&f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert!(!sc.checks.is_empty() || sc.every > 0);
        let pad = pad.unwrap_or_else(|| panic!("{}: no pad script", f.display()));
        assert_eq!(pad.end, Some(sc.frames), "{}: the pad script ends where the scenario does", f.display());
        assert!(pad.odd_changes().is_empty(), "{}: pad changes at odd k {:?}", f.display(), pad.odd_changes());
        assert!(pad.entries.first().is_some_and(|e| e.0 == 0), "{}: the script starts at k 0", f.display());
        assert!(sc.value_names().iter().all(|n| values.defs.contains_key(n)));
    }
}

#[test]
fn the_first_scenario_moves_the_stick_as_documented() {
    let (sc, _, pad) = load_scenario(&scenarios().join("movement/tt-run-06.toml")).unwrap();
    let pad = pad.unwrap();
    assert_eq!((sc.name.as_str(), sc.state.as_str(), sc.frames), ("tt-run-06", "06", 330));
    assert_eq!(pad.at(39).unwrap().bytes(), [0xff, 0xff, 0x7f, 0x7f, 0x7f, 0x7f]);
    assert_eq!(pad.at(40).unwrap().bytes(), [0xff, 0xff, 0x7f, 0x7f, 0x7f, 0x00], "stick up");
    assert_eq!(pad.at(120).unwrap().bytes(), [0xff, 0xff, 0x7f, 0x7f, 0xff, 0x7f], "stick right");
    assert_eq!(pad.at(200).unwrap().buttons, 0);
}

#[test]
fn the_menu_scenario_presses_start_cross_circle_start() {
    let (_, _, pad) = load_scenario(&scenarios().join("menu/camp-open-06.toml")).unwrap();
    let pad = pad.unwrap();
    for (k, name) in [(200, "start"), (280, "cross"), (320, "circle"), (360, "start")] {
        assert!(pad.at(k).unwrap().pressed(name), "{name} at {k}");
        assert!(!pad.at(k + 4).unwrap().pressed(name));
    }
}
