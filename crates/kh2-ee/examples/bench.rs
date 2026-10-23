//! Interpreter speed: instructions per second on a synthetic loop and on the game's own vector
//! rotation (`FUN_0016cf78`, needs `KH2_ISO`), with and without a table of hooks. Run with
//! `cargo run --release -p kh2-ee --example bench`.

use std::time::Instant;

use kh2_ee::Ee;

fn report(label: &str, ee: &Ee, from: u64, t: Instant) {
    let n = ee.steps - from;
    let s = t.elapsed().as_secs_f64();
    println!("{label}: {} instructions in {:.3}s = {:.1} M/s", n, s, n as f64 / s / 1e6);
}

fn hook_none(_: &mut Ee) {}

fn synthetic(hooks: usize) {
    let mut ee = Ee::new();
    // addiu t0, zero, 0x7fff; loop: addiu t0, t0, -1; bne t0, zero, loop; nop; jr ra; nop (x 1000)
    let base = 0x0010_0000u32;
    for (i, w) in [0x2408_7fffu32, 0x2508_ffff, 0x1500_fffe, 0, 0x03e0_0008, 0].iter().enumerate() {
        ee.w32(base + 4 * i as u32, *w);
    }
    for k in 0..hooks {
        ee.hooks.insert(0x0020_0000 + 4 * k as u32 * 17, hook_none);
    }
    ee.limit = u64::MAX;
    let (from, t) = (ee.steps, Instant::now());
    for _ in 0..200 {
        ee.call(base, &[], &[], 0x01f0_0000).unwrap();
    }
    report(&format!("synthetic loop, {hooks} hooks"), &ee, from, t);
}

fn game(hooks: usize) {
    let Some(path) = std::env::var("KH2_ISO").ok().filter(|p| std::path::Path::new(p).exists()) else {
        println!("game: skipped (set KH2_ISO)");
        return;
    };
    let mut a = kh2_archive::Archive::open(&path).expect("iso");
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable");
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    for k in 0..16 {
        ee.wf32(0x354a60 + 4 * k, if k % 5 == 0 { 1.0 } else { 0.0 });
    }
    for k in 0..hooks {
        ee.hooks.insert(0x0100_0000 + 4 * k as u32 * 29, hook_none);
    }
    let (from, t) = (ee.steps, Instant::now());
    for n in 0..100_000u32 {
        for (i, x) in [1.0f32, 2.0, 3.0, 1.0].iter().enumerate() {
            ee.wf32(0x0181_0000 + 4 * i as u32, *x);
        }
        ee.w32(0x0180_0000, 0x0181_0000);
        ee.wf32(0x0180_0004, (n % 628) as f32 * 0.01);
        ee.call(0x16cf78, &[0x0180_0000], &[], 0x01ff_0000).unwrap();
    }
    report(&format!("game FUN_0016cf78 x100000, {hooks} hooks"), &ee, from, t);
}

fn main() {
    synthetic(0);
    synthetic(500);
    game(0);
    game(500);
}
