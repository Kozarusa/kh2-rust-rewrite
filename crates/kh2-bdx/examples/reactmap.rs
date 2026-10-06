//! Developer tool: which scripts handle which reaction commands. For every BDX on the disc that has an
//! event 11 handler (the script event a reaction command is delivered as, `docs/limits-reactions.md`)
//! prints the command ids the handler compares its second argument with (a constant pushed in front of
//! a subtraction, as the Soldier's handler does with 34) and the ids the script offers itself through
//! host 1.165 / 1.237 / 1.325. Needs the ISO (KH2_ISO or local/iso.txt) and the file-name list
//! (`kh2 fetch-names`, local/kh2idx.txt) to find scripts inside archives.
//!
//! `cargo run -p kh2-bdx --example reactmap`

use std::collections::BTreeSet;

use kh2_bdx::{BinaryOp, Insn, Program, analyze, call_constants, decode};

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

/// Constants the handler at word `pc` subtracts from a value it loaded (walks straight code and both
/// ways of every jump, at most 400 instructions).
fn handled(p: &Program, pc: usize) -> BTreeSet<i32> {
    let mut seen = BTreeSet::new();
    let mut work = vec![pc];
    let mut found = BTreeSet::new();
    let mut budget = 400;
    while let Some(mut at) = work.pop() {
        while budget > 0 && seen.insert(at) {
            budget -= 1;
            let Some((i, len)) = decode(p, at) else { break };
            match i {
                Insn::Jump(t) => {
                    work.push(t);
                    break;
                }
                Insn::JumpIfZero(t) | Insn::JumpIfNonZero(t) => work.push(t),
                Insn::Ret | Insn::Exit => break,
                Insn::Binary { float: false, op: BinaryOp::Sub } => {
                    // the instruction in front of the subtraction is the constant
                    if let Some((Insn::PushInt(v), _)) = at.checked_sub(3).and_then(|b| decode(p, b)) {
                        found.insert(v as i32);
                    }
                }
                _ => {}
            }
            at += len;
        }
    }
    found
}

fn main() {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("local/iso.txt").ok().map(|s| s.trim().to_string()));
    let Some(path) = path else { return eprintln!("set KH2_ISO") };
    let mut a = kh2_archive::Archive::open(&path).expect("iso");
    let names_path = std::env::var("KH2_NAMES").unwrap_or_else(|_| "local/kh2idx.txt".into());
    if let Ok(list) = std::fs::read_to_string(names_path) {
        a.apply_names(list.lines()).expect("names");
    }
    for (name, bdx) in scripts(&mut a) {
        let Ok(p) = Program::parse(&bdx) else { continue };
        let Ok(an) = analyze(&p) else { continue };
        let mut offered: BTreeSet<i32> = BTreeSet::new();
        for (pc, (i, _)) in &an.code {
            if let Insn::Host { group: 1, index } = i {
                let (n, which): (usize, &[usize]) = match index {
                    165 => (4, &[1]),
                    237 => (5, &[0, 1, 2, 3]),
                    325 => (4, &[2]),
                    _ => continue,
                };
                let c = call_constants(&an, *pc, n);
                for &k in which {
                    if let Some(v) = c[k] {
                        offered.insert(v as i32);
                    }
                }
            }
        }
        let handler = p.entry(11).map(|pc| handled(&p, pc as usize));
        if handler.is_none() && offered.is_empty() {
            continue;
        }
        println!("{name}\thandles {}\toffers {offered:?}", handler.map_or("-".to_string(), |h| format!("{h:?}")));
    }
}
