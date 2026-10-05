//! Developer tool: runs every script on the disc on the mock world for a while and counts the errors
//! the interpreter reports, split by whether the script calls functions whose signature was inferred.
//! A wrong inferred argument count unbalances the operand stack and shows up as stack underflow,
//! overflow or an undecodable instruction. Needs KH2_ISO and KH2_NAMES.

use std::collections::BTreeMap;

use kh2_bdx::mock::{MockHost, SELF_OBJ};
use kh2_bdx::signatures::absent_from_table;
use kh2_bdx::{HostTable, Insn, Program, Runtime, VmError, analyze};

fn kind(e: &VmError) -> &'static str {
    match e {
        VmError::BadInstruction { .. } => "bad instruction",
        VmError::PcOutOfRange(_) => "pc out of range",
        VmError::OperandStackUnderflow => "stack underflow",
        VmError::OperandStackOverflow => "stack overflow",
        VmError::FrameOverflow => "frame overflow",
        VmError::DivideByZero { .. } => "divide by zero",
        VmError::BadAddress(_) => "bad address",
        VmError::UnknownHost { .. } => "unknown host",
        VmError::StepLimit => "step limit",
        VmError::Host(_) => "host",
    }
}

fn main() {
    let path = std::env::var("KH2_ISO").expect("KH2_ISO");
    let mut a = kh2_archive::Archive::open(&path).expect("iso");
    if let Ok(list) = std::fs::read_to_string(std::env::var("KH2_NAMES").unwrap_or_default()) {
        a.apply_names(list.lines()).expect("names");
    }
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable");
    let table = HostTable::from_exe(&elf).expect("table");
    let list: Vec<_> = a.entries().to_vec();
    let mut stats: BTreeMap<(bool, &str), usize> = BTreeMap::new();
    let mut scripts = [0usize; 2];
    for l in &list {
        let name = l.name.clone().unwrap_or_default();
        let Ok(d) = a.read_entry(&l.entry) else { continue };
        let mut bdx = Vec::new();
        if name.ends_with(".bdx") {
            bdx.push((name.clone(), d));
        } else if kh2_bar::is_bar(&d) {
            for e in kh2_bar::parse(&d).unwrap_or_default().iter().filter(|e| e.kind == 3 && !e.data.is_empty()) {
                bdx.push((format!("{name}:{}", e.tag), e.data.to_vec()));
            }
        }
        for (label, data) in bdx {
            let Ok(p) = Program::parse(&data) else { continue };
            let Ok(an) = analyze(&p) else { continue };
            let uses_inferred = an.code.values().any(|(i, _)| matches!(i, Insn::Host { group, index } if absent_from_table(*group, *index)));
            let mut rt = Runtime::new(p);
            rt.set_host_table(table.clone());
            let mut host = MockHost::new();
            rt.shared.rng = 7;
            rt.shared.step_limit = 20_000;
            if !rt.spawn(&mut host, SELF_OBJ) {
                continue;
            }
            for _ in 0..120 {
                rt.update(&mut host);
                host.tick();
            }
            scripts[uses_inferred as usize] += 1;
            for (pc, e) in &rt.core.errors {
                *stats.entry((uses_inferred, kind(e))).or_default() += 1;
                if uses_inferred && std::env::var("VERBOSE").is_ok() && !matches!(e, VmError::BadAddress(_) | VmError::StepLimit | VmError::DivideByZero { .. }) {
                    eprintln!("{label}: {e:?} at word {pc:#x}");
                }
            }
        }
    }
    println!("scripts without inferred calls: {}, with: {}", scripts[0], scripts[1]);
    for ((inferred, k), n) in &stats {
        println!("{} {k}: {n}", if *inferred { "with inferred   " } else { "without inferred" });
    }
}
