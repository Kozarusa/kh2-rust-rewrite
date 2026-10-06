//! Against the user's disc (skipped without KH2_ISO or local/iso.txt): every area file on the disc parses, and the enemy the app's areas spawn most (after Shadow and Soldier) is the Dusk.
//! `cargo test -p kh2-ard --test disc -- --nocapture` prints the list (docs/enemies.md).

use std::collections::BTreeMap;

use kh2_tables::tables as t;

fn open() -> Option<kh2_archive::Archive> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    // The `ard/` files live in the world index 000tt.idx, which is listed once its name is known;
    // the whole name list (when fetched) also names the entries for the listing test.
    let names = std::fs::read_to_string("../../local/kh2idx.txt").ok();
    let list: Vec<&str> = names.as_deref().map(|l| l.lines().collect()).unwrap_or_default();
    a.apply_names(if list.is_empty() { vec!["000tt.idx"] } else { list }).ok()?;
    Some(a)
}

/// `ObjectType` of enemies in `00objentry` (Shadow, Soldier and the Dusk all have it).
const ENEMY: u8 = 4;

#[test]
fn the_app_areas_spawn_the_dusk_most() {
    let Some(mut a) = open() else { return };
    let objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).expect("objentry").rows;
    let name = |id: u32| objs.iter().find(|o| o.object_id == id).map_or(("?".to_string(), 0), |o| (o.model_name.clone(), o.object_type));
    let mut enemies: BTreeMap<String, usize> = BTreeMap::new();
    for area in ["tt00", "tt02", "tt04"] {
        let file = a.read(&format!("ard/{area}.ard")).expect("area file");
        let (ard, bad) = kh2_ard::Area::parse(&file).expect("BAR");
        assert!(bad.is_empty(), "{area}: entries that do not parse: {bad:?}");
        let mut per: BTreeMap<(String, u8), usize> = BTreeMap::new();
        for (script, prog, group, e) in ard.spawned() {
            let (model, kind) = name(e.object);
            if kind == ENEMY {
                eprintln!("{area} {script} program {prog:#x} group {group}: {model} (object {:#x}, spawn kind {})", e.object, e.spawn_kind);
                *enemies.entry(model.clone()).or_default() += 1;
            }
            *per.entry((model, kind)).or_default() += 1;
        }
        eprintln!("{area}: {per:?}");
    }
    eprintln!("enemies (ObjectType {ENEMY}) over tt00, tt02, tt04: {enemies:?}");
    let pick = enemies.iter().filter(|(m, _)| !matches!(m.as_str(), "M_EX020" | "M_EX010")).max_by_key(|(_, n)| **n).map(|(m, _)| m.as_str());
    assert_eq!(pick, Some("M_EX990"));
    // Its model, motion set and BDX script are on the disc.
    let mdlx = a.read("obj/M_EX990.mdlx").expect("model");
    assert!(kh2_bar::parse(&mdlx).unwrap().iter().any(|e| e.kind == 3 && !e.data.is_empty()), "BDX entry");
    assert!(a.read("obj/M_EX990.mset").is_ok());
}

#[test]
fn every_area_file_on_the_disc_parses() {
    let Some(mut a) = open() else { return };
    let names: Vec<String> = a.entries().iter().filter_map(|l| l.name.clone()).filter(|n| n.starts_with("ard/") && n.ends_with(".ard")).collect();
    let (mut parsed, mut not_bar, mut failed) = (0, 0, Vec::new());
    for n in &names {
        let file = a.read(n).expect("area file");
        let Some((_, bad)) = kh2_ard::Area::parse(&file) else {
            not_bar += 1;
            continue;
        };
        if bad.is_empty() {
            parsed += 1;
        } else {
            failed.push(format!("{n}: {bad:?}"));
        }
    }
    eprintln!("{parsed} area files parse, {not_bar} are not BAR files, {} fail: {:?}", failed.len(), failed.iter().take(10).collect::<Vec<_>>());
    // Three type-12 entries outside Twilight Town hold something else (their second word is not a
    // group count: po10 `m_10` reads 0x180001); they are not spawn files of this layout.
    let known = ["ard/al08.ard: [\"m_10\", \"m_11\"]", "ard/po10.ard: [\"m_10\"]"];
    assert!(failed.iter().all(|f| known.contains(&f.as_str())), "{failed:?}");
}
