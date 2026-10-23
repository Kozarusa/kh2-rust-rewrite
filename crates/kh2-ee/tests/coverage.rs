//! Instruction coverage over the user's own executable and its four overlays: every word inside a
//! function range must decode and execute (a fault other than `Unimplemented` counts as decoded).
//! Function ranges come from the private reverse-engineering workspace's code sheets
//! (`kh2_ps2/sheet/code`, found by walking up from this crate, or `KH2_FUNCS`); skipped without them
//! or without the disc (`KH2_ISO`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use kh2_ee::{Ee, Fault, OVERLAY_BASE};

fn sheets_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("KH2_FUNCS") {
        return Some(d.into());
    }
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..8 {
        let c = d.join("kh2_ps2").join("sheet").join("code");
        if c.is_dir() {
            return Some(c);
        }
        d = d.parent()?.to_path_buf();
    }
    None
}

/// (address, size in bytes) of every function of one sheet.
fn functions(dir: &std::path::Path, sheet: &str) -> Vec<(u32, u32)> {
    let text = std::fs::read_to_string(dir.join(format!("functions_{sheet}.json"))).expect("sheet");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json");
    v["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|r| {
            let a = r[1].as_str().unwrap();
            let a = a.rsplit("::").next().unwrap();
            (u32::from_str_radix(a, 16).unwrap(), r[3].as_u64().unwrap() as u32)
        })
        .collect()
}

fn scan(ee: &mut Ee, funcs: &[(u32, u32)], hist: &mut BTreeMap<String, (u32, u32)>) -> usize {
    let mut words = 0;
    // One-byte "functions" are data items the disassembler mislabelled (tables inside the code
    // segment), not code.
    for &(addr, size) in funcs.iter().filter(|f| f.1 >= 4) {
        for off in (0..size).step_by(4) {
            let pc = addr + off;
            let w = ee.r32(pc);
            ee.gpr = [0; 32];
            ee.pc = pc;
            words += 1;
            if let Err(Fault::Unimplemented { what, word, .. }) = ee.execute(w) {
                let key = format!("{what} op {:02x} funct {:02x} rt {:02x} sa {:02x}", w >> 26, w & 63, (w >> 16) & 31, (w >> 6) & 31);
                hist.entry(key).or_insert((0, word)).0 += 1;
            }
        }
    }
    words
}

#[test]
fn every_instruction_of_the_executable_and_overlays_decodes() {
    let (Some(dir), Ok(iso)) = (sheets_dir(), std::env::var("KH2_ISO")) else {
        eprintln!("skipped: no function sheets or no disc (set KH2_ISO)");
        return;
    };
    let mut a = kh2_archive::Archive::open(iso).expect("disc");
    let boot = a.boot_executable().to_string();
    let elf = a.iso().read(&boot).expect("executable");
    let mut hist = BTreeMap::new();
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut total = scan(&mut ee, &functions(&dir, "main"), &mut hist);
    for k in 0..4 {
        let code = a.read_overlay(k).expect("overlay");
        ee.load_overlay(&code).expect("fits");
        let f = functions(&dir, &format!("ovl{k}"));
        assert!(f.iter().all(|&(addr, _)| addr >= OVERLAY_BASE));
        total += scan(&mut ee, &f, &mut hist);
    }
    for (k, (n, w)) in &hist {
        eprintln!("unimplemented x{n}: {k} (e.g. {w:08x})");
    }
    eprintln!("{total} words scanned");
    assert!(total > 600_000);
    assert!(hist.is_empty(), "{} unimplemented encodings", hist.len());
}
