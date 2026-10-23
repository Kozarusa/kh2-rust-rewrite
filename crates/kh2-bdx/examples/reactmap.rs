//! Developer tool: which scripts handle which reaction commands. For every BDX on the disc that has an
//! event 11 handler (the script event a reaction command is delivered as, `docs/systems/drive-limits-summons/contract.md`)
//! prints the command ids the handler compares its second argument with (a constant pushed in front of
//! a subtraction, as the Soldier's handler does with 34) and the ids the script offers itself through
//! host 1.165 / 1.237 / 1.325. Needs the ISO (KH2_ISO or local/iso.txt) and the file-name list
//! (`kh2 fetch-names`, local/kh2idx.txt) to find scripts inside archives.
//!
//! `cargo run -p kh2-bdx --example reactmap`

use kh2_bdx::{Program, analyze, reaction};

fn scripts(a: &mut kh2_archive::Archive) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let list: Vec<_> = a.entries().to_vec();
    for l in &list {
        let name = l.name.clone().unwrap_or_else(|| format!("<{:08x}>", l.entry.hash32));
        let Ok(data) = a.read_entry(&l.entry) else { continue };
        if name.ends_with(".bdx") {
            out.push((name, data));
        } else if kh2_bar::is_bar(&data)
            && let Ok(es) = kh2_bar::parse(&data)
        {
            for e in es.iter().filter(|e| e.kind == 3 && !e.data.is_empty()) {
                out.push((format!("{name}:{}", e.tag), e.data.to_vec()));
            }
        }
    }
    out
}

/// Object id to model name, from `00objentry.bin` (rows of 0x60 bytes after an 8-byte header: the id
/// word at +0, the model name at +8).
fn objentry_models(a: &mut kh2_archive::Archive) -> std::collections::HashMap<u32, String> {
    let mut out = std::collections::HashMap::new();
    let Some(l) = a.entries().iter().find(|l| l.name.as_deref() == Some("00objentry.bin")).cloned() else { return out };
    let Ok(d) = a.read_entry(&l.entry) else { return out };
    for r in d.get(8..).unwrap_or_default().chunks_exact(0x60) {
        let id = u32::from_le_bytes([r[0], r[1], r[2], r[3]]);
        let n = &r[8..0x28];
        let n = &n[..n.iter().position(|&b| b == 0).unwrap_or(n.len())];
        out.insert(id, String::from_utf8_lossy(n).into_owned());
    }
    out
}

/// The entities of a spawn list (BAR type 12 of an area file): a word 2, the group count, then per
/// group a 0x2c-byte header (halfword counts at +4 entities, +6 event activators, +8 walk paths,
/// +0xa return parameters, +0xc signals) followed by entities and activators (0x40 bytes each), walk
/// paths (8 bytes plus 12 per point, the count at +2), return parameters (0x10) and signals (8). The
/// sizes were fitted to the disc: 3,449 of the 3,456 lists end exactly where this walk ends; `None`
/// for the others.
fn spawn_entities(d: &[u8]) -> Option<Vec<&[u8]>> {
    let h = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let n = d.get(4..8).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)?;
    let mut o = 8;
    let mut out = Vec::new();
    for _ in 0..n {
        let (e, act, w, r, s) = (h(o + 4)?, h(o + 6)?, h(o + 8)?, h(o + 10)?, h(o + 12)?);
        o += 0x2c;
        for i in 0..e {
            out.push(d.get(o + i * 0x40..o + (i + 1) * 0x40)?);
        }
        o += (e + act) * 0x40;
        for _ in 0..w {
            o += 8 + h(o + 2)? * 12;
        }
        o += r * 0x10 + s * 8;
    }
    (o == d.len()).then_some(out)
}

fn main() {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("local/iso.txt").ok().map(|s| s.trim().to_string()));
    let Some(path) = path else { return eprintln!("set KH2_ISO") };
    let mut a = kh2_archive::Archive::open(&path).expect("iso");
    let names_path = std::env::var("KH2_NAMES").unwrap_or_else(|_| "local/kh2idx.txt".into());
    if let Ok(list) = std::fs::read_to_string(names_path) {
        a.apply_names(list.lines()).expect("names");
    }
    // The native source: reaction parts (collision records of type 5, BAR entry 23) in object files.
    let list: Vec<_> = a.entries().to_vec();
    let mut from_spawn = std::collections::BTreeSet::new();
    for l in &list {
        let Some(name) = &l.name else { continue };
        if !name.ends_with(".mdlx") {
            continue;
        }
        let Ok(data) = a.read_entry(&l.entry) else { continue };
        let Ok(es) = kh2_bar::parse(&data) else { continue };
        let Some(coll) = es.iter().find(|e| e.kind == 23) else { continue };
        let count = coll.data.get(0..4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
        let mut ids: Vec<(u16, usize)> = Vec::new();
        let mut type14 = 0;
        for i in 0..count {
            let Some(r) = coll.data.get(0x40 + 0x14 * i..0x40 + 0x14 * (i + 1)) else { break };
            if r[4] == 14 {
                type14 += 1;
            }
            if r[4] == 5 {
                let id = u16::from_le_bytes([r[2], r[3]]);
                match ids.iter_mut().find(|(c, _)| *c == id) {
                    Some(e) => e.1 += 1,
                    None => ids.push((id, 1)),
                }
            }
        }
        if ids.iter().any(|&(c, _)| c == 0x83)
            && let Some(m) = name.strip_prefix("obj/").and_then(|n| n.strip_suffix(".mdlx"))
        {
            from_spawn.insert(m.to_string());
        }
        if !ids.is_empty() {
            println!("PARTS {name}	{ids:?}");
        }
        if type14 > 0 {
            println!("TYPE14 {name}	{type14}");
        }
    }
    // Part id 0x83 takes the command from the halfword at +0x2c of the object's spawn record (+0x81c,
    // FUN_0019cb70; FUN_0016b870 stores the record). The records are the 0x40-byte entities of the area's
    // spawn lists (BAR entries of type 12 in `ard/*.ard`).
    let models = objentry_models(&mut a);
    for l in &list {
        let Some(name) = &l.name else { continue };
        if !name.ends_with(".ard") {
            continue;
        }
        let Ok(data) = a.read_entry(&l.entry) else { continue };
        let Ok(es) = kh2_bar::parse(&data) else { continue };
        for e in es.iter().filter(|e| e.kind == 12 && !e.data.is_empty()) {
            let Some(ents) = spawn_entities(&e.data) else {
                println!("SPAWN-UNPARSED {name}:{}", e.tag);
                continue;
            };
            for ent in ents {
                let obj = u32::from_le_bytes([ent[0], ent[1], ent[2], ent[3]]);
                let cmd = u16::from_le_bytes([ent[0x2c], ent[0x2d]]);
                let model = models.get(&obj).cloned().unwrap_or_else(|| format!("<obj {obj}>"));
                if cmd != 0 {
                    let part = if from_spawn.contains(&model) { "part 0x83" } else { "no part 0x83" };
                    println!("SPAWN {name}:{}	{model}	command {cmd}	{part}", e.tag);
                }
            }
        }
    }
    for (name, bdx) in scripts(&mut a) {
        let Ok(p) = Program::parse(&bdx) else { continue };
        let Ok(an) = analyze(&p) else { continue };
        let offered = reaction::offered(&an);
        let handler = reaction::handled(&p);
        // Offer calls whose command id is not a constant (computed or read from memory).
        let mut computed = Vec::new();
        for (pc, (i, _)) in &an.code {
            if let kh2_bdx::Insn::Host { group: 1, index } = i {
                let (n, which): (usize, &[usize]) = match index {
                    165 => (4, &[1]),
                    237 => (5, &[0, 1, 2, 3]),
                    325 => (4, &[2]),
                    _ => continue,
                };
                let c = kh2_bdx::call_constants(&an, *pc, n);
                if which.iter().any(|&k| c[k].is_none()) {
                    computed.push(format!("1.{index}@{pc:#x}"));
                }
            }
        }
        if handler.is_none() && offered.is_empty() && computed.is_empty() {
            continue;
        }
        println!("{name}\thandles {}\toffers {offered:?}\tcomputed {computed:?}", handler.map_or("-".to_string(), |h| format!("{h:?}")));
    }
}
