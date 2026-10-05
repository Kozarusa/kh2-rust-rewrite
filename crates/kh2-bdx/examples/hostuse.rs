//! Developer tool: lists the host functions that scripts call, with the signature and the native
//! address the executable's table gives them. Needs the ISO (KH2_ISO or local/iso.txt).
//!
//! `cargo run -p kh2-bdx --example hostuse -- M_EX020 M_EX010` lists the named objects' scripts;
//! `-- --all` covers every BDX on the disc (use counts over all scripts). `KH2_NAMES` points at the
//! file-name list so that `obj/NAME.mdlx` lookups and standalone `.bdx` files resolve.

use std::collections::BTreeMap;

use kh2_bdx::{HostTable, Insn, Program, analyze, names};

fn scripts(a: &mut kh2_archive::Archive, args: &[String]) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    if args.first().map(String::as_str) == Some("--all") {
        let list: Vec<_> = a.entries().to_vec();
        for l in &list {
            let name = l.name.clone().unwrap_or_else(|| format!("<{:08x}>", l.entry.hash32));
            let Ok(data) = a.read_entry(&l.entry) else { continue };
            if name.ends_with(".bdx") {
                out.push((name, data));
            } else if kh2_bar::is_bar(&data)
                && let Ok(es) = kh2_bar::parse(&data)
            {
                for e in es.iter().filter(|e| e.kind == 3) {
                    out.push((format!("{name}:{}", e.tag), e.data.to_vec()));
                }
            }
        }
    } else {
        for name in args {
            let data = a.read(&format!("obj/{}.mdlx", name.to_uppercase())).expect("object");
            let bdx = kh2_bar::parse(&data).unwrap().into_iter().find(|e| e.kind == 3).expect("BDX entry").data.to_vec();
            out.push((name.clone(), bdx));
        }
    }
    out
}

fn main() {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("local/iso.txt").ok().map(|s| s.trim().to_string()));
    let Some(path) = path else { return eprintln!("set KH2_ISO") };
    let mut a = kh2_archive::Archive::open(&path).expect("iso");
    // Optional file-name list (OpenKH's kh2idx.txt, fetched by `kh2 fetch-names`); without it only BAR contents are found.
    let names_path = std::env::var("KH2_NAMES").unwrap_or_else(|_| "local/kh2idx.txt".into());
    if let Ok(list) = std::fs::read_to_string(names_path) {
        a.apply_names(list.lines()).expect("names");
    }
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable");
    let table = HostTable::from_exe(&elf).expect("host table");
    let args: Vec<String> = std::env::args().skip(1).collect();
    // KH2_DUMPTABLE=1 prints every entry of the executable's table, used or not.
    if std::env::var("KH2_DUMPTABLE").is_ok() {
        for g in 0..12u16 {
            for i in 0..400u16 {
                if let Some(s) = table.get(g, i) {
                    println!("{g}.{i:<4} args {} ret {} fn {:#x} {}", s.nargs, s.returns as u8, s.address, names::host_name(g, i).unwrap_or("-"));
                }
            }
        }
        return;
    }
    let mut uses: BTreeMap<(u16, u16), BTreeMap<String, usize>> = BTreeMap::new();
    // KH2_PREFIX=obj/M_,obj/B_ keeps only scripts whose label starts with one of the prefixes; KH2_SKIP=_RTN drops matches.
    let prefixes: Vec<String> = std::env::var("KH2_PREFIX").map(|v| v.split(',').map(str::to_string).collect()).unwrap_or_default();
    let skip = std::env::var("KH2_SKIP").ok();
    for (name, bdx) in scripts(&mut a, &args) {
        if bdx.is_empty() || (!prefixes.is_empty() && !prefixes.iter().any(|p| name.starts_with(p.as_str()))) || skip.as_ref().is_some_and(|s| name.contains(s.as_str())) {
            continue;
        }
        let Ok(p) = Program::parse(&bdx) else { continue };
        let Ok(an) = analyze(&p) else { continue };
        for (i, _) in an.code.values() {
            if let Insn::Host { group, index } = i {
                *uses.entry((*group, *index)).or_default().entry(name.clone()).or_default() += 1;
            }
        }
    }
    // KH2_WHO=3.35 lists the scripts that call one function.
    if let Ok(w) = std::env::var("KH2_WHO") {
        let (g, i) = w.split_once('.').expect("KH2_WHO=group.index");
        if let Some(per) = uses.get(&(g.parse().unwrap(), i.parse().unwrap())) {
            for (n, c) in per {
                println!("{n} x{c}");
            }
        }
        return;
    }
    let all = args.first().map(String::as_str) == Some("--all");
    for ((g, i), per) in &uses {
        let sig_text = match table.get(*g, *i) {
            Some(s) => format!("args {} ret {} fn {:#x}", s.nargs, s.returns as u8, s.address),
            None => "NOT IN STATIC TABLE".to_string(),
        };
        let n = names::host_name(*g, *i).unwrap_or("-");
        let total: usize = per.values().sum();
        if all {
            println!("{g}.{i:<4} {sig_text:<34} {n:<22} x{total} in {} scripts", per.len());
        } else {
            println!("{g}.{i:<4} {sig_text:<34} {n:<22} {per:?}");
        }
    }
}
