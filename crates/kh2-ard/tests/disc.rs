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

/// Twilight Town's area files (`ard/ttNN.ard`, the ones on the disc), in area order.
fn tt_areas(a: &mut kh2_archive::Archive) -> Vec<(String, kh2_ard::Area)> {
    (0..64)
        .filter_map(|n| {
            let name = format!("tt{n:02}");
            let file = a.read(&format!("ard/{name}.ard")).ok()?;
            let (ard, bad) = kh2_ard::Area::parse(&file)?;
            assert!(bad.is_empty(), "{name}: {bad:?}");
            Some((name, ard))
        })
        .collect()
}

/// docs/events.md section 1: the opcodes of every Twilight Town area script, with counts per area
/// and in total, and the trigger definitions (opcode 0xc) decoded.
#[test]
fn twilight_town_area_script_opcodes() {
    let Some(mut a) = open() else { return };
    let areas = tt_areas(&mut a);
    assert!(areas.len() > 30, "tt area files found: {}", areas.len());
    let mut total: BTreeMap<u16, usize> = BTreeMap::new();
    for (name, ard) in &areas {
        let mut per: BTreeMap<u16, usize> = BTreeMap::new();
        for (tag, progs) in &ard.scripts {
            for p in progs {
                for c in &p.commands {
                    assert!(c.opcode < kh2_ard::OPCODES, "{name} {tag} {:#x}: opcode {:#x}", p.id, c.opcode);
                    *per.entry(c.opcode).or_default() += 1;
                    *total.entry(c.opcode).or_default() += 1;
                    if c.opcode == 0xc {
                        let t = kh2_ard::parse_trigger(&c.args).unwrap_or_else(|| panic!("{name} {tag} {:#x}: trigger {:x?}", p.id, c.args));
                        eprintln!("{name} {tag} program {:#x}: trigger ({:#x}, {:#x}) {:x?}", p.id, t.event, t.arg, t.subs);
                    }
                }
            }
        }
        let progs: Vec<String> = ard.scripts.iter().map(|(t, p)| format!("{t} {:x?}", p.iter().map(|p| p.id).collect::<Vec<_>>())).collect();
        eprintln!("{name}: programs {progs:?}; opcodes {:?}", per.iter().map(|(o, n)| format!("{o:#x} {} x{n}", kh2_ard::opcode_name(*o))).collect::<Vec<_>>());
    }
    eprintln!("opcode table over {} tt area files:", areas.len());
    for (o, n) in &total {
        eprintln!("  {o:#04x} {:<20} {n}", kh2_ard::opcode_name(*o));
    }
    assert!(total.contains_key(&kh2_ard::OP_SPAWN));
}

/// docs/events.md section 2: every spawn group of Twilight Town with its kind, the exits (kind 1:
/// header +0x1c area, +0x1d entrance), activator volumes, walk paths and signals; and the BAR
/// entries of type 3 (scripts) and 0x16 (events) of each area file.
#[test]
fn twilight_town_groups_exits_and_events() {
    let Some(mut a) = open() else { return };
    let objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).expect("objentry").rows;
    let name = |id: u32| objs.iter().find(|o| o.object_id == id).map_or(("?".to_string(), 0), |o| (o.model_name.clone(), o.object_type));
    let mut kinds: BTreeMap<u16, usize> = BTreeMap::new();
    for (area, ard) in &tt_areas(&mut a) {
        let others: Vec<String> = ard.entries.iter().filter(|(_, k, _)| !matches!(*k, kh2_ard::SCRIPT_KIND | kh2_ard::SPAWN_KIND)).map(|(t, k, n)| format!("{t}:{k:#x}:{n}")).collect();
        eprintln!("{area}: other entries {others:?}");
        // The exits: groups with activators whose header names an area (+0x1c) and entrance (+0x1d).
        let exits: Vec<String> = ard
            .spawns
            .iter()
            .flat_map(|(tag, gs)| gs.iter().map(move |g| (tag, g)))
            .filter(|(_, g)| g.kind & 0xff == 1 && !g.activators.is_empty())
            .map(|(tag, g)| format!("{tag} -> tt{:02} entrance {} (+0x1e {}) options {:?}", g.header[0x1c], g.header[0x1d], g.header[0x1e], g.activators.iter().map(|v| v.option).collect::<Vec<_>>()))
            .collect();
        eprintln!("{area} exits: {exits:?}");
        for (tag, groups) in &ard.spawns {
            for g in groups {
                *kinds.entry(g.kind).or_default() += 1;
                let ents: Vec<String> = g.entities.iter().map(|e| {
                    let (m, k) = name(e.object);
                    format!("{m}/{k}/sk{}", e.spawn_kind)
                }).collect();
                let h = &g.header;
                eprintln!(
                    "{area} {tag} group id {:#x} kind {:#x} flags {:#x} hdr+0x1c {:02x?} ents {ents:?} acts {} paths {} returns {} signals {:?}",
                    g.id, g.kind, g.flags(), &h[0x0e..0x2c], g.activators.len(), g.paths.len(), g.returns.len(),
                    g.signals.iter().map(|s| (s.event, s.arg, s.action)).collect::<Vec<_>>()
                );
                for v in &g.activators {
                    eprintln!("    activator shape {} option {} at {:?} scale {:?} rot {:?} +0x28 {:02x?}", v.shape, v.option, v.position, v.scale, v.rotation, &v.raw[0x28..0x40]);
                }
                for p in &g.paths {
                    eprintln!("    path hdr {:02x?} points {}", p.header, p.points.len());
                }
            }
        }
    }
    eprintln!("group kinds over tt: {kinds:?}");
}

/// docs/events.md section 6: every Twilight Town event (type 0x16 entry) splits into commands
/// `u16 size, u16 opcode` to its last byte or a size-0 end; the opcodes used, with counts.
#[test]
fn twilight_town_events_split_into_commands() {
    let Some(mut a) = open() else { return };
    let mut ops: BTreeMap<u16, usize> = BTreeMap::new();
    let mut events = 0;
    for (area, ard) in &tt_areas(&mut a) {
        for (tag, d) in &ard.events {
            let cmds = kh2_ard::parse_event(d).unwrap_or_else(|| panic!("{area} event {tag}"));
            let used: usize = cmds.iter().map(|c| c.data.len() + 4).sum();
            events += 1;
            for c in &cmds {
                *ops.entry(c.opcode).or_default() += 1;
            }
            if area == "tt02" && tag == "102" || area == "tt04" && tag == "110" {
                for c in &cmds {
                    eprintln!("{area} {tag}: op {:#x} {:02x?}", c.opcode, &c.data[..c.data.len().min(40)]);
                }
            }
            eprintln!("{area} event {tag}: {} commands, {used} of {} bytes", cmds.len(), d.len());
        }
    }
    eprintln!("{events} tt events; opcodes {ops:x?}");
    // 07localset.bin: per world (type-2 entries in order) u16 count, then (u8 program, u8 area) pairs.
    let file = a.read("07localset.bin").expect("07localset.bin");
    let bar = kh2_bar::parse(&file).expect("BAR");
    let tt = bar.iter().filter(|e| e.kind == 2).nth(2).expect("tt").data;
    let n = u16::from_le_bytes([tt[0], tt[1]]) as usize;
    let pairs: Vec<(u8, u8)> = (0..n).map(|k| (tt[2 + 2 * k], tt[3 + 2 * k])).collect();
    eprintln!("07localset tt: {n} (program, area) pairs {pairs:x?}");
}

/// docs/events.md sections 5 and 7: the Twilight Town story flags (world 2 of `00progress.bin`)
/// whose side effects write area program bytes, and the music the area scripts set (opcode 0x10)
/// next to the `arif` pairs of `03system.bin`.
#[test]
fn twilight_town_story_flags_and_music() {
    let Some(mut a) = open() else { return };
    let file = a.read("00progress.bin").expect("00progress.bin");
    let bar = kh2_bar::parse(&file).expect("BAR");
    eprintln!("00progress.bin entries: {:?}", bar.iter().map(|e| (e.tag, e.kind, e.data.len())).collect::<Vec<_>>());
    let worlds: Vec<_> = bar.iter().filter(|e| e.kind == 2).collect();
    let tt = worlds[2].data;
    let n = kh2_ard::progress::flag_count(tt);
    eprintln!("tt flag entry: {} bytes, {n} flags", tt.len());
    let mut ops: BTreeMap<u8, usize> = BTreeMap::new();
    for f in 0..n as u16 {
        let effects = kh2_ard::progress::effects(tt, f).unwrap_or_else(|| panic!("flag {f:#x}"));
        for e in &effects {
            *ops.entry(e.op).or_default() += 1;
        }
        if !effects.is_empty() {
            eprintln!("flag {:#x}: {:?}", 0x800 + f as u32, effects.iter().map(|e| if e.op == 1 { format!("programs {:x?}", e.programs()) } else { format!("op {} {:02x?}", e.op, e.data) }).collect::<Vec<_>>());
        }
    }
    eprintln!("tt flag side-effect ops: {ops:?}");
    // Music: opcode 0x10 per area script program, and the arif pairs.
    let sys = a.read("03system.bin").expect("03system.bin");
    let arif = kh2_bar::parse(&sys).unwrap().into_iter().find(|e| e.tag == "arif").expect("arif").data;
    let u16_at = |o: usize| u16::from_le_bytes([arif[o], arif[o + 1]]);
    let (count, off) = (u16_at(8 + 4 * 2) as usize, u16_at(8 + 4 * 2 + 2) as usize);
    for (name, ard) in &tt_areas(&mut a) {
        let index: usize = name[2..].parse().unwrap();
        let pairs: Vec<(u16, u16)> = if index < count { (0..8).map(|k| (u16_at(off + 0x40 * index + 0x10 + 4 * k), u16_at(off + 0x40 * index + 0x12 + 4 * k))).collect() } else { Vec::new() };
        let mut set = Vec::new();
        for (tag, progs) in &ard.scripts {
            for p in progs {
                for c in p.commands.iter().filter(|c| c.opcode == 0x10) {
                    set.push(format!("{tag} {:#x}: {} {}", p.id, c.args[0] & 0xffff, c.args[0] >> 16));
                }
            }
        }
        eprintln!("{name}: arif pairs {pairs:?}; opcode 0x10 {set:?}");
    }
}

/// WP-E1c (docs/events.md section 2, "Lock"): the exit-lock table `dsa` of `00progress.bin`
/// (FUN_0017fd10 -> FUN_001951f8; read by FUN_00195208) and the Twilight Town story flags whose
/// side effects (ops 2..5: FUN_00195260, FUN_00195288, FUN_001952a0, FUN_00195300) write a lock
/// halfword. Every entry index a flag names is inside the table.
#[test]
fn the_exit_lock_table_and_the_flags_that_write_it() {
    let Some(mut a) = open() else { return };
    let file = a.read("00progress.bin").expect("00progress.bin");
    let bar = kh2_bar::parse(&file).expect("BAR");
    let dsa = bar.iter().find(|e| e.tag == kh2_ard::progress::LOCK_TABLE).expect("dsa entry").data;
    let count = u16::from_le_bytes([dsa[0], dsa[1]]) as usize;
    let pairs: Vec<(u8, u8)> = (0..count).map(|i| (dsa[2 + 2 * i], dsa[3 + 2 * i])).collect();
    eprintln!("dsa: {} bytes, {count} lockable areas: {pairs:?}", dsa.len());
    let tt: Vec<String> = pairs.iter().enumerate().filter(|(_, p)| p.0 == 2).map(|(i, p)| format!("{i}: tt{:02}", p.1)).collect();
    eprintln!("dsa Twilight Town entries: {tt:?}");
    for (i, &(w, ar)) in pairs.iter().enumerate() {
        assert_eq!(kh2_ard::progress::lock_index(dsa, w, ar), pairs.iter().position(|p| *p == (w, ar)), "entry {i}");
    }
    let worlds: Vec<_> = bar.iter().filter(|e| e.kind == 2).collect();
    let tt = worlds[2].data;
    for f in 0..kh2_ard::progress::flag_count(tt) as u16 {
        for e in kh2_ard::progress::effects(tt, f).unwrap_or_default().iter().filter(|e| (2..=5).contains(&e.op)) {
            let words = e.flags();
            eprintln!("flag {:#x}: lock op {} halfwords {words:x?} -> {:?}", 0x800 + f as u32, e.op, words.first().and_then(|&i| pairs.get(i as usize)));
            assert!(words.first().is_some_and(|&i| (i as usize) < count), "flag {f:#x} op {} names {words:?}", e.op);
        }
    }
}

/// WP-E1 (docs/events.md section 9): every Twilight Town area run with programs (0, 0, 0) through
/// `kh2_ard::run::run_area` for each entrance a player entity names: the groups loaded, where the
/// player starts (FUN_001981a0), the exits of the loaded groups and the triggers on event 1 (an
/// exit touched) that would take an exit over. tt04 has a start for entrance 1 and an exit to tt03.
#[test]
fn twilight_town_entrances_and_exits_with_program_zero() {
    let Some(mut a) = open() else { return };
    let mut tt04 = (false, false);
    for (area, ard) in &tt_areas(&mut a) {
        let mut entrances: Vec<u8> = ard.spawns.values().flatten().flat_map(|g| &g.entities).filter(|e| e.object == kh2_ard::run::PLAYER && e.spawn_kind == 1).map(|e| e.spawn_arg).collect();
        entrances.sort();
        entrances.dedup();
        let run = kh2_ard::run::run_area(ard, [0, 0, 0], entrances.first().copied().unwrap_or(0));
        let groups: Vec<String> = run.groups.iter().map(|g| format!("{} {}{}", g.script, g.tag, if g.alt { "*" } else { "" })).collect();
        eprintln!("{area}: programs (0, 0, 0) groups {groups:?} music {:?} unported {:x?} triggers {}", run.music, run.unported, run.triggers.len());
        for e in &entrances {
            let r = kh2_ard::run::run_area(ard, [0, 0, 0], *e);
            match kh2_ard::run::player_start(ard, &r, *e) {
                Some((tag, p)) => eprintln!("{area}: entrance {e} start {tag} at {:?} ry {:.3}", p.position, p.rotation[1]),
                None => eprintln!("{area}: entrance {e}: no start in the program-0 groups"),
            }
            if area == "tt04" && *e == 1 {
                tt04.0 |= kh2_ard::run::player_start(ard, &r, 1).is_some();
            }
        }
        for g in &run.groups {
            for grp in ard.spawns.get(&g.tag).into_iter().flatten() {
                let Some(x) = grp.exit() else { continue };
                let (w, dst) = x.destination(2);
                let opts: Vec<(u16, u16)> = grp.volumes().map(|v| (v.shape, v.option)).collect();
                let takeover: Vec<String> = run.triggers.iter().filter(|t| t.matches(1, grp.id)).map(|t| format!("{:?} {:?}", t.event_tag(), t.jump())).collect();
                eprintln!("{area}: exit {} id {:#x} -> world {w} tt{dst:02} entrance {} gate {} disabled {} volumes (shape, option) {opts:?} takeover {takeover:?}", g.tag, grp.id, x.entrance, x.gate, grp.starts_disabled());
                if area == "tt04" && dst == 3 {
                    tt04.1 = true;
                }
            }
        }
    }
    assert!(tt04.0, "tt04 has no player start for entrance 1 in its program-0 groups");
    assert!(tt04.1, "tt04 has no exit to tt03 in its program-0 groups");
}
/// docs/events.md section 3, "Talk" (WP-E3): `libretto-tt.bar` parses, its records are sorted by
/// id (the game finds them by binary search, `FUN_001b64b0`), every program reads to its op 0, and
/// every Twilight Town townsperson (objentry type 15) with a talk id (spawn record +0x28) has a
/// program. Prints each townsperson's talk id, part command (+0x2c) and program.
#[test]
fn twilight_town_talk_programs() {
    let Some(mut a) = open() else { return };
    let file = a.read("libretto-tt.bar").expect("libretto-tt.bar");
    let lib = kh2_ard::libretto::Libretto::from_bar(&file).expect("type-2 entry");
    assert!(!lib.records.is_empty());
    assert!(lib.records.windows(2).all(|w| w[0].0 < w[1].0), "records sorted by id");
    let mut ops: BTreeMap<u16, usize> = BTreeMap::new();
    for &(id, n, _) in &lib.records {
        if n == 0 {
            continue;
        }
        let p = lib.program(id).unwrap_or_else(|| panic!("talk {id:#x}: program does not read"));
        for c in &p {
            *ops.entry(c.op).or_default() += 1;
        }
    }
    eprintln!("libretto-tt: {} records, ops {ops:?}", lib.records.len());
    let objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).expect("objentry").rows;
    let (mut talkers, mut missing) = (0, Vec::new());
    let mut commands: BTreeMap<u16, usize> = BTreeMap::new();
    for (area, ard) in tt_areas(&mut a) {
        for (tag, groups) in &ard.spawns {
            for g in groups {
                for e in &g.entities {
                    let Some(o) = objs.iter().find(|o| o.object_id == e.object) else { continue };
                    if o.object_type != 15 {
                        continue;
                    }
                    *commands.entry(e.part_command()).or_default() += 1;
                    let id = e.talk_id();
                    if id == 0 {
                        continue;
                    }
                    talkers += 1;
                    match lib.program(id) {
                        Some(p) => eprintln!("{area} {tag} {} (object {:#x}): talk {id:#x}, part command {}, {:?}", o.model_name, e.object, e.part_command(), p.iter().map(|c| c.kind()).collect::<Vec<_>>()),
                        None => missing.push(format!("{area} {tag} {} talk {id:#x}", o.model_name)),
                    }
                }
            }
        }
    }
    eprintln!("townspeople with a talk id: {talkers}; part commands (+0x2c) over all townspeople: {commands:?}; without a program: {missing:?}");
    assert!(talkers > 0);
}
/// WP-E6: Twilight Town's short events (tt01 `100`, `200`, `400`, `600`) command by command, and
/// the area-script triggers that name an event tag (the tags the event player is handed).
#[test]
fn twilight_town_tt01_short_events() {
    let Some(mut a) = open() else { return };
    let file = a.read("ard/tt01.ard").expect("tt01");
    let (ard, _) = kh2_ard::Area::parse(&file).expect("BAR");
    let sizes: Vec<(String, usize)> = ard.events.iter().map(|(t, d)| (t.clone(), d.len())).collect();
    eprintln!("tt01 events (tag, bytes): {sizes:?}");
    for (tag, progs) in &ard.scripts {
        for p in progs {
            for c in p.commands.iter().filter(|c| c.opcode == 0xc) {
                if let Some(t) = kh2_ard::parse_trigger(&c.args) {
                    eprintln!("tt01 {tag} program {:#x}: trigger ({:#x}, {:#x}) {:x?}", p.id, t.event, t.arg, t.subs);
                }
            }
        }
    }
    for tag in ["100", "200", "400", "600"] {
        let d = ard.events.get(tag).unwrap_or_else(|| panic!("tt01 holds event {tag}"));
        let cmds = kh2_ard::parse_event(d).expect("event");
        assert!(!cmds.is_empty(), "tt01 event {tag} has commands");
        // Opcode 8 is the end marker the timeline thread waits for (docs/events.md, FUN_00222668).
        assert!(cmds.iter().any(|c| c.opcode == 8), "tt01 event {tag} has its end opcode 8");
        for c in &cmds {
            eprintln!("tt01 {tag}: op {:#x} ({} bytes) {:02x?}", c.opcode, c.data.len(), c.data);
        }
    }
    // An event with actors, motions and a camera (tt02 102), for the layouts of opcodes 1, 2, 0x24, 0x2c.
    let file = a.read("ard/tt02.ard").expect("tt02");
    let (ard, _) = kh2_ard::Area::parse(&file).expect("BAR");
    let cmds = kh2_ard::parse_event(&ard.events["102"]).expect("event");
    for c in cmds.iter().filter(|c| c.opcode != 0x13) {
        eprintln!("tt02 102: op {:#x} ({} bytes) {:02x?}", c.opcode, c.data.len(), &c.data[..c.data.len().min(64)]);
    }
}