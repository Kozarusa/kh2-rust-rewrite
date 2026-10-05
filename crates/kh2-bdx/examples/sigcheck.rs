//! Developer tool: checks the signature inference (`signatures.rs`) against the executable's table.
//! Hides a share of the table's functions, infers them from every script on the disc and counts how
//! often the answer matches. `HIDE=n` hides every n-th function (default 2). Also lists what the
//! inference makes of the entries the table lacks. Needs KH2_ISO and, for names, KH2_NAMES.

use std::collections::BTreeMap;

use kh2_bdx::signatures::{absent_from_table, infer_only};
use kh2_bdx::{HostTable, Insn, Program, analyze, names};

fn main() {
    let path = std::env::var("KH2_ISO").expect("KH2_ISO");
    let mut a = kh2_archive::Archive::open(&path).expect("iso");
    if let Ok(list) = std::fs::read_to_string(std::env::var("KH2_NAMES").unwrap_or_default()) {
        a.apply_names(list.lines()).expect("names");
    }
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable");
    let table = HostTable::from_exe(&elf).expect("host table");
    let hide: u32 = std::env::var("HIDE").ok().and_then(|v| v.parse().ok()).unwrap_or(2);
    let hidden = |g: u16, i: u16| table.get(g, i).is_some() && names::lookup(g, i).is_none() && (g as u32 * 1000 + i as u32).is_multiple_of(hide);
    let known = |g: u16, i: u16| {
        if hidden(g, i) {
            return None;
        }
        names::lookup(g, i).map(|f| (f.args as usize, f.ret)).or_else(|| table.get(g, i).map(|s| (s.nargs as usize, s.returns)))
    };
    let mut scripts: Vec<(String, Vec<u8>)> = Vec::new();
    let list: Vec<_> = a.entries().to_vec();
    for l in &list {
        let name = l.name.clone().unwrap_or_default();
        let Ok(data) = a.read_entry(&l.entry) else { continue };
        if name.ends_with(".bdx") {
            scripts.push((name, data));
        } else if kh2_bar::is_bar(&data)
            && let Ok(es) = kh2_bar::parse(&data)
        {
            for e in es.iter().filter(|e| e.kind == 3 && !e.data.is_empty()) {
                scripts.push((format!("{name}:{}", e.tag), e.data.to_vec()));
            }
        }
    }
    // (script, function) outcomes
    let (mut right, mut wrong, mut missed) = (0, 0, 0);
    // function -> (scripts it was wrong in, signature in the table, last inferred signature)
    type Sig = (usize, bool);
    let mut wrong_by_fn: BTreeMap<(u16, u16), (usize, Sig, Sig)> = BTreeMap::new();
    let mut absent: BTreeMap<(u16, u16), BTreeMap<(usize, bool), usize>> = BTreeMap::new();
    for (name, data) in &scripts {
        let Ok(p) = Program::parse(data) else { continue };
        let Ok(an) = analyze(&p) else { continue };
        let used: std::collections::BTreeSet<(u16, u16)> = an.code.values().filter_map(|(i, _)| if let Insn::Host { group, index } = i { Some((*group, *index)) } else { None }).collect();
        let got = infer_only(&p, &known, &|g, i| hidden(g, i));
        for &(g, i) in used.iter().filter(|&&(g, i)| hidden(g, i)) {
            let truth = table.get(g, i).map(|s| (s.nargs as usize, s.returns)).unwrap();
            match got.get(&(g, i)) {
                Some(&s) if s == truth => right += 1,
                Some(&s) => {
                    wrong += 1;
                    let e = wrong_by_fn.entry((g, i)).or_insert((0, truth, s));
                    e.0 += 1;
                    if std::env::var("VERBOSE").is_ok() {
                        eprintln!("{name}: {g}.{i} inferred {s:?}, table {truth:?}");
                    }
                }
                None => missed += 1,
            }
        }
        // The entries the table lacks.
        let got = infer_only(&p, &|g, i| known(g, i).filter(|_| !hidden(g, i)), &absent_from_table);
        for &(g, i) in used.iter().filter(|&&(g, i)| absent_from_table(g, i)) {
            if let Some(&s) = got.get(&(g, i)) {
                *absent.entry((g, i)).or_default().entry(s).or_default() += 1;
            } else {
                *absent.entry((g, i)).or_default().entry((usize::MAX, false)).or_default() += 1;
            }
        }
    }
    println!("hidden functions (every {hide}th): {right} right, {wrong} wrong, {missed} not inferred, over {} scripts", scripts.len());
    for ((g, i), (n, truth, got)) in &wrong_by_fn {
        println!("  wrong {g}.{i}: {n} scripts, table {truth:?}, inferred {got:?}");
    }
    println!("entries the table lacks (signature: number of scripts):");
    for ((g, i), sigs) in &absent {
        println!("  {g}.{i}: {sigs:?}");
    }
}
