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
