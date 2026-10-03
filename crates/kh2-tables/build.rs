//! Preflight gate + strut generation. Runs on every build that touches sheets/.
//! Errors stop compilation. Unfinished rows are reported as warnings, or stop it with `KH2_STRICT=1`.

use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    println!("cargo:rerun-if-changed={}", root.join("sheets").display());
    println!("cargo:rerun-if-changed={}", root.join("crates").display());
    println!("cargo:rerun-if-env-changed=KH2_STRICT");

    let report = kh2_sheets::preflight(&root);
    let strict = std::env::var("KH2_STRICT").is_ok_and(|v| v == "1");
    if !report.ok(strict) {
        panic!("preflight failed (run `cargo run -p kh2 -- preflight` for the grid):\n{}", report.render(50));
    }
    let total: usize = report.grid.values().map(|g| g.0).sum();
    let done: usize = report.grid.values().map(|g| g.1).sum();
    println!(
        "cargo:warning=preflight PASS: {done}/{total} rows finished, {} unfinished, {} gaps",
        report.unfinished.len(),
        report.gaps.len()
    );

    let sheets = kh2_sheets::load_dir(&root.join("sheets")).expect("sheets load (already checked)");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    for (file, code) in kh2_sheets::strut::render_all(&sheets).expect("struts render (already checked)") {
        std::fs::write(out.join(file), code).unwrap();
    }
}
