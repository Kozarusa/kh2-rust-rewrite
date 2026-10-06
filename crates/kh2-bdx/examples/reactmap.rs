//! Developer tool: which scripts handle which reaction commands. For every BDX on the disc that has an
//! event 11 handler (the script event a reaction command is delivered as, `docs/limits-reactions.md`)
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
        for i in 0..count {
            let Some(r) = coll.data.get(0x40 + 0x14 * i..0x40 + 0x14 * (i + 1)) else { break };
            if r[4] == 5 {
                let id = u16::from_le_bytes([r[2], r[3]]);
                match ids.iter_mut().find(|(c, _)| *c == id) {
                    Some(e) => e.1 += 1,
                    None => ids.push((id, 1)),
                }
            }
        }
        if !ids.is_empty() {
            println!("PARTS {name}	{ids:?}");
        }
    }
    for (name, bdx) in scripts(&mut a) {
        let Ok(p) = Program::parse(&bdx) else { continue };
        let Ok(an) = analyze(&p) else { continue };
        let offered = reaction::offered(&an);
        let handler = reaction::handled(&p);
        if handler.is_none() && offered.is_empty() {
            continue;
        }
        println!("{name}\thandles {}\toffers {offered:?}", handler.map_or("-".to_string(), |h| format!("{h:?}")));
    }
}
