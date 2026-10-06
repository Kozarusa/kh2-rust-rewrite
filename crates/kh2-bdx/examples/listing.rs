//! Developer tool: lists an object's script with extra routine starts the flow walk cannot find on
//! its own (a routine number passed through parameters to a thread-starting host function).
//! Needs the ISO (KH2_ISO or local/iso.txt).
//!
//! `cargo run -p kh2-bdx --example listing -- M_EX010 5a` lists Soldier's script and follows the
//! routine at word 0x5a (the body of its reaction command, started through host 2.20).

use kh2_bdx::{Program, analyze_with, listing};

fn main() {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("local/iso.txt").ok().map(|s| s.trim().to_string()));
    let Some(path) = path else { return eprintln!("set KH2_ISO") };
    let mut a = kh2_archive::Archive::open(&path).expect("iso");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = args.first() else { return eprintln!("usage: listing NAME [ROOT_HEX...]") };
    let data = a.read(&format!("obj/{}.mdlx", name.to_uppercase())).expect("object");
    let bdx = kh2_bar::parse(&data).unwrap().into_iter().find(|e| e.kind == 3).expect("BDX entry").data.to_vec();
    let prog = Program::parse(&bdx).expect("script");
    let roots: Vec<usize> = args[1..].iter().map(|r| usize::from_str_radix(r, 16).expect("hex root")).collect();
    let an = analyze_with(&prog, &roots).expect("analysis");
    print!("{}", listing(&prog, &an));
}
