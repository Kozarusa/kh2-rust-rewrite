//! Final Mix disc checks behind `docs/final-mix.md` sections 1 to 4: disc identity, the archive
//! layout, file lists by kind, the tables the app reads, text and voice folders. Read-only; prints
//! counts and names only (nothing from either disc is stored). Skips with a note unless both disc
//! images are configured: USA (`KH2_ISO` or `local/iso.txt`) and Final Mix (`KH2_ISO_FM` or
//! `local/iso_fm.txt`). The Final Mix disc is opened with the explicit opt-in
//! (`Archive::open_edition(.., true)`); the default `Archive::open` keeps refusing it.
//!
//! Run: `$env:KH2_ISO_FM = '<path to the Final Mix image>'; cargo test -p kh2 --test fm_disc -- --nocapture --test-threads 1`

use std::collections::{BTreeMap, HashMap};

use kh2_archive::{Archive, Error};

fn path(var: &str, file: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .or_else(|| std::fs::read_to_string(format!("../../local/{file}")).ok().map(|s| s.trim().to_string()))
        .filter(|p| std::path::Path::new(p).exists())
}

fn paths() -> Option<(String, String)> {
    match (path("KH2_ISO", "iso.txt"), path("KH2_ISO_FM", "iso_fm.txt")) {
        (Some(u), Some(f)) => Some((u, f)),
        _ => {
            eprintln!("skipped: needs both disc images (KH2_ISO and KH2_ISO_FM)");
            None
        }
    }
}

fn names() -> Option<String> {
    let n = std::fs::read_to_string("../../local/kh2idx.txt").ok();
    if n.is_none() {
        eprintln!("skipped: no name list (local/kh2idx.txt; `cargo run -p kh2 -- fetch-names`)");
    }
    n
}

/// FNV-1a over the bytes: equality of two files without keeping either.
fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325, |h, &x| (h ^ x as u64).wrapping_mul(0x0100_0000_01b3))
}

#[test]
fn fm_disc_identity_and_archive_layout() {
    let Some((usa, fm)) = paths() else { return };
    // The default open refuses the Final Mix disc; the opt-in opens it unchanged.
    assert!(matches!(Archive::open_edition(&fm, false), Err(Error::UnsupportedDisc(_))));
    let mut u = Archive::open(&usa).expect("USA disc");
    let mut f = Archive::open_edition(&fm, true).expect("Final Mix disc");
    assert_eq!(u.boot_executable(), "SLUS_210.05");
    assert_eq!(f.boot_executable(), "SLPM_666.75");
    for (label, a) in [("USA", &mut u), ("FM", &mut f)] {
        let boot = a.boot_executable().to_string();
        let exe = a.iso().read(&boot).expect("boot executable");
        let cnf = String::from_utf8_lossy(&a.iso().read("SYSTEM.CNF").unwrap()).replace(['\r', '\n'], " ");
        let files: Vec<String> = a.iso().entries().iter().map(|e| format!("{} {}", e.path, e.size)).collect();
        println!("{label}: SYSTEM.CNF '{}'", cnf.trim());
        println!("{label}: executable {boot}, {} bytes", exe.len());
        println!("{label}: {} disc files: {}", files.len(), files.join(", "));
        println!("{label}: KH2.IDX {} entries, OVL.IDX {} overlays", a.entries().len(), a.overlays().len());
    }
    // The LZ format and IDX layout are shared: the system tables decompress on both discs.
    for name in ["00battle.bin", "03system.bin", "00objentry.bin", "00progress.bin"] {
        let (a, b) = (u.read(name).expect(name), f.read(name).expect(name));
        println!("{name}: USA {} bytes, FM {} bytes", a.len(), b.len());
    }
    // FM's OVL.IMG also holds the movies (large, uncompressed); only the compressed code overlays
    // are read here.
    let code: Vec<usize> = (0..f.overlays().len()).filter(|&k| f.overlays()[k].compressed).collect();
    let ovl_ok = code.iter().filter(|&&k| f.read_overlay(k).is_ok()).count();
    println!("FM OVL.IDX: {} entries, {} compressed code overlays ({ovl_ok} decompress)", f.overlays().len(), code.len());
    let sizes: Vec<String> = f.overlays().iter().map(|e| format!("{}{}", e.length, if e.compressed { "c" } else { "" })).collect();
    println!("FM OVL.IDX lengths: {}", sizes.join(" "));
    assert_eq!(ovl_ok, code.len());
}

/// Folder kind of a file name for the comparison table.
fn kind(name: &str) -> String {
    let parts: Vec<&str> = name.split('/').collect();
    match parts.as_slice() {
        [one] => one.rsplit('.').next().unwrap_or(one).to_string(),
        [top, sub, _, ..] if ["msg", "voice", "menu", "map", "movie", "event"].contains(top) => format!("{top}/{sub}"),
        [top, ..] => top.to_string(),
        [] => String::new(),
    }
}

#[test]
fn fm_file_lists_by_kind() {
    let Some((usa, fm)) = paths() else { return };
    let Some(list) = names() else { return };
    let mut u = Archive::open(&usa).unwrap();
    let mut f = Archive::open_edition(&fm, true).unwrap();
    let nu = u.apply_names(list.lines()).unwrap();
    let nf = f.apply_names(list.lines()).unwrap();
    println!("entries with world sub-indexes: USA {} ({nu} named), FM {} ({nf} named)", u.entries().len(), f.entries().len());
    let key = |l: &kh2_archive::Listed| (l.entry.hash32, l.entry.hash16);
    let label = |l: &kh2_archive::Listed| l.name.clone().unwrap_or_else(|| format!("{:08x}/{:04x}", l.entry.hash32, l.entry.hash16));
    let fm: HashMap<_, _> = f.entries().iter().map(|l| (key(l), l.clone())).collect();
    let us: HashMap<_, _> = u.entries().iter().map(|l| (key(l), l.clone())).collect();
    // kind -> [added, removed, size changed, same size]
    let mut table: BTreeMap<String, [usize; 4]> = BTreeMap::new();
    let mut changed_tables = Vec::new();
    for (k, l) in &us {
        let n = label(l);
        let row = table.entry(kind(&n)).or_default();
        match fm.get(k) {
            None => row[1] += 1,
            Some(o) if o.entry.length != l.entry.length => {
                row[2] += 1;
                if (n.ends_with(".bin") || n.ends_with(".bar")) && !n.contains('/') {
                    changed_tables.push(format!("{n} {}->{}", l.entry.length, o.entry.length));
                }
            }
            Some(_) => row[3] += 1,
        }
    }
    for (k, l) in &fm {
        if !us.contains_key(k) {
            table.entry(kind(&label(l))).or_default()[0] += 1;
        }
    }
    println!("{:24} {:>6} {:>7} {:>7} {:>9}", "kind", "added", "removed", "changed", "same size");
    for (k, r) in &table {
        println!("{k:24} {:6} {:7} {:7} {:9}", r[0], r[1], r[2], r[3]);
    }
    changed_tables.sort();
    println!("top-level .bin/.bar with a new size: {}", changed_tables.join(", "));
    let added_top: Vec<String> = fm.iter().filter(|(k, l)| !us.contains_key(k) && !label(l).contains('/')).map(|(_, l)| label(l)).collect();
    println!("top-level files only on FM: {}", added_top.join(", "));
    // The USA language folders are replaced on FM.
    let has = |m: &HashMap<_, kh2_archive::Listed>, p: &str| m.values().filter(|l| label(l).starts_with(p)).count();
    for p in ["msg/us/", "msg/jp/", "voice/us/", "voice/fm/", "voice/fj/", "menu/us/", "menu/fm/", "map/us/", "map/jp/", "movie/us/"] {
        println!("{p:10} USA {:5}  FM {:5}", has(&us, p), has(&fm, p));
    }
    assert_eq!(has(&fm, "msg/us/"), 0);
    assert!(has(&fm, "msg/jp/") > 0);
}

#[test]
fn fm_voice_battle_banks_are_the_usa_files() {
    let Some((usa, fm)) = paths() else { return };
    let Some(list) = names() else { return };
    let mut u = Archive::open(&usa).unwrap();
    let mut f = Archive::open_edition(&fm, true).unwrap();
    u.apply_names(list.lines()).unwrap();
    f.apply_names(list.lines()).unwrap();
    let banks: Vec<String> =
        f.entries().iter().filter_map(|l| l.name.clone()).filter(|n| n.starts_with("voice/us/battle/")).collect();
    let mut same = 0;
    for n in &banks {
        let (a, b) = (u.read(n).expect(n), f.read(n).expect(n));
        if fnv(&a) == fnv(&b) && a.len() == b.len() {
            same += 1;
        }
    }
    println!("FM voice/us/battle banks: {}, byte-identical to the USA disc: {same}", banks.len());
    assert!(!banks.is_empty());
    // Event voices: the USA folder has no counterpart of the same size and name on FM.
    let event = |a: &Archive, p: &str| -> HashMap<String, u32> {
        a.entries()
            .iter()
            .filter_map(|l| Some((l.name.clone()?, l.entry.length)))
            .filter(|(n, _)| n.starts_with(p))
            .map(|(n, len)| (n[p.len()..].to_string(), len))
            .collect()
    };
    let us_ev = event(&u, "voice/us/event/");
    for p in ["voice/fm/event/", "voice/fj/event/"] {
        let other = event(&f, p);
        let named = us_ev.keys().filter(|k| other.contains_key(*k)).count();
        let same_len = us_ev.iter().filter(|(k, l)| other.get(*k) == Some(l)).count();
        println!("USA voice/us/event {} files; FM {p} {} files; same name {named}; same name and size {same_len}", us_ev.len(), other.len());
    }
}

#[test]
fn fm_tables_the_app_reads() {
    let Some((usa, fm)) = paths() else { return };
    let mut u = Archive::open(&usa).unwrap();
    let mut f = Archive::open_edition(&fm, true).unwrap();
    let describe = |a: &mut Archive, def: &kh2_tables::TableDef| -> String {
        let blob = match kh2_tables::subfile(a, def) {
            Ok(b) => b,
            Err(e) => return format!("missing ({e})"),
        };
        match kh2_tables::tables::decode_dyn(def.name, &blob) {
            Some(Ok(t)) => format!("v{} {} rows, {}/{} bytes", t.version, t.rows.len(), t.used, t.len),
            Some(Err(e)) => format!("{} bytes, does not decode: {e}", blob.len()),
            None => format!("{} bytes (no layout)", blob.len()),
        }
    };
    let (mut same, mut differ) = (0, 0);
    for def in kh2_tables::tables::ALL.iter().filter(|d| d.record_size.is_some()) {
        let (a, b) = (describe(&mut u, def), describe(&mut f, def));
        if a == b { same += 1 } else { differ += 1 }
        println!("{:12} {}:{:6} USA {a:40} FM {b}", def.name, def.archive, def.tag);
    }
    println!("tables with the same shape on both discs: {same}; different: {differ}");
    assert!(differ > 0);
}
