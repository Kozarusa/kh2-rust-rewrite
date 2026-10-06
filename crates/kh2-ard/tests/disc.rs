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

/// The next enemy after the Dusk (WP-6H): `N_EX570_BTL`, object 0x678, in four tt04 battle groups.
/// Prints each group's records (the words the game copies into the object, FUN_0016b870: +0x20 and
/// +0x24 become spawn parameters 1 and 2, host 1.39) and checks the files the app reads are there.
#[test]
fn the_second_enemy_is_n_ex570_btl() {
    let Some(mut a) = open() else { return };
    let file = a.read("ard/tt04.ard").expect("area file");
    let (ard, _) = kh2_ard::Area::parse(&file).expect("BAR");
    let groups = ard.groups_with("btl", 0x678);
    eprintln!("N_EX570_BTL groups: {groups:?}");
    assert_eq!(groups.iter().map(|(_, g)| g.as_str()).collect::<Vec<_>>(), ["b_41", "b_42", "b_51", "b_45"]);
    for (_, g) in &groups {
        for e in ard.spawns[g].iter().flat_map(|g| &g.entities) {
            let w = |o: usize| u32::from_le_bytes(e.raw[o..o + 4].try_into().unwrap());
            eprintln!(
                "{g}: object {:#x} at {:?} rot {:?} kind {} arg {} serial {:#x} param1 {} param2 {} +0x28 {:#x} +0x30 {:02x?}",
                e.object, e.position, e.rotation, e.spawn_kind, e.spawn_arg, e.serial, w(0x20), w(0x24), w(0x28) & 0xffff, &e.raw[0x30..0x40]
            );
        }
    }
    for f in ["obj/N_EX570_BTL.mdlx", "obj/N_EX570_BTL.mset", "obj/N_EX570_BTL.apdx"] {
        assert!(a.read(f).is_ok(), "{f}");
    }
    let mdlx = a.read("obj/N_EX570_BTL.mdlx").unwrap();
    assert!(kh2_bar::parse(&mdlx).unwrap().iter().any(|e| e.kind == 3 && !e.data.is_empty()), "BDX entry");
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
