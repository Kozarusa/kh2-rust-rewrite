//! Writes `$OUT_DIR/registry.rs`, which `enemy.rs` includes: one `#[path] pub mod` line per file of `src/kinds/` (sorted
//! by file name) inside `pub mod kinds`, and `KINDS`, every kind the app has, in the same order. A new kind is one new
//! file; no shared list is edited.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let dir = Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir")).join("src").join("kinds");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut stems: Vec<String> = std::fs::read_dir(&dir)
        .expect("src/kinds")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_string))
        .collect();
    stems.sort();
    let mut out = String::from("/// The enemy kinds, one module per file of `src/kinds/`.\npub mod kinds {\n");
    for s in &stems {
        let path = dir.join(format!("{s}.rs"));
        println!("cargo:rerun-if-changed={}", path.display());
        let _ = writeln!(out, "    #[path = {:?}]\n    pub mod {s};", path.to_string_lossy().replace('\\', "/"));
    }
    out.push_str("}\n\n/// Every enemy kind the app has (the files of `src/kinds/`, by file name).\npub static KINDS: &[&Kind] = &[");
    for (i, s) in stems.iter().enumerate() {
        let _ = write!(out, "{}&kinds::{s}::KIND", if i == 0 { "" } else { ", " });
    }
    out.push_str("];\n");
    std::fs::write(Path::new(&std::env::var("OUT_DIR").expect("out dir")).join("registry.rs"), out).expect("registry.rs");
}
