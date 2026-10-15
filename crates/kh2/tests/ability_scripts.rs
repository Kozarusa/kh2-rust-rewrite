//! Which BDX scripts test an ability bit (host 1.343, `FUN_00170c28`: the bit of the ability block
//! `FUN_001768a8(object) + bit / 32` that the script pushed). Read-only; skips without a disc.
//! Behind docs/abilities.md "Ability readers": it lists, per script, the bit pushed before each
//! call, so the readers of Leaf Bracer (bit 114) and the other combat bits can be found in data.

use kh2_bdx::{Insn, Program, analyze};

fn archive() -> Option<kh2_archive::Archive> {
    let path = std::env::var("KH2_ISO")
        .ok()
        .or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))
        .filter(|p| std::path::Path::new(p).exists());
    let Some(path) = path else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return None;
    };
    kh2_archive::Archive::open(&path).ok()
}

/// Every BDX on the disc (loose `.bdx` files and type 3 entries of BARs): `(label, bytes)`.
fn scripts(a: &mut kh2_archive::Archive) -> Vec<(String, Vec<u8>)> {
    let list: Vec<_> = a.entries().to_vec();
    let mut out = Vec::new();
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
    out
}

#[test]
fn ability_bit_tests_in_scripts() {
    let Some(mut a) = archive() else { return };
    let mut calls = 0;
    let mut bits = std::collections::BTreeMap::<i64, Vec<String>>::new();
    for (label, data) in scripts(&mut a) {
        let Ok(p) = Program::parse(&data) else { continue };
        let Ok(an) = analyze(&p) else { continue };
        let code: Vec<_> = an.code.iter().map(|(pc, (i, _))| (*pc, *i)).collect();
        for (n, (pc, i)) in code.iter().enumerate() {
            if !matches!(i, Insn::Host { group: 1, index: 343 }) {
                continue;
            }
            calls += 1;
            // The bit is the last value pushed before the call.
            let bit = code[..n].iter().rev().find_map(|(_, x)| if let Insn::PushInt(v) = x { Some(*v as i64) } else { None }).unwrap_or(-1);
            eprintln!("{label} pc {pc:#x}: host 1.343 bit {bit}");
            if std::env::var("KH2_ABILITY_SCRIPT_LIST").is_ok() && bits.is_empty() {
                for (pc, x) in &code[n.saturating_sub(20)..(n + 40).min(code.len())] {
                    eprintln!("    {pc:#06x} {x:?}");
                }
            }
            bits.entry(bit).or_default().push(label.clone());
        }
    }
    eprintln!("{calls} calls; bits: {:?}", bits.keys().collect::<Vec<_>>());
    assert!(calls > 0, "the scan sees the host calls the CLI scan counts");
    // docs/abilities.md "Ability readers": Leaf Bracer's bit 114 is the one the scripts read, by
    // every `CURE_*.mag` script.
    let leaf = bits.get(&114).expect("bit 114 (Leaf Bracer) is read by a script");
    assert!(leaf.iter().all(|l| l.to_ascii_uppercase().contains("CURE")), "bit 114 readers: {leaf:?}");
}
