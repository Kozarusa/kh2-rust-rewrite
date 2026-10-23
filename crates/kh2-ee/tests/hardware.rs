//! Game functions that touch memory-mapped hardware run against the register stub. Skipped without
//! a disc (`KH2_ISO`).

use kh2_ee::Ee;

const STACK: u32 = 0x01ff_0000;

fn loaded() -> Option<Ee> {
    let path = std::env::var("KH2_ISO").ok().filter(|p| std::path::Path::new(p).exists())?;
    let mut a = kh2_archive::Archive::open(path).ok()?;
    let boot = a.boot_executable().to_string();
    let elf = a.iso().read(&boot).ok()?;
    let mut ee = Ee::new();
    ee.load_elf(&elf)?;
    Some(ee)
}

/// FUN_001e35f0 starts the profiling timer: it ORs 0x83 into T0_MODE (0x10000010) and zeroes
/// T0_COUNT (0x10000000); 0x83 selects the horizontal-blank clock, which barely moves in a test, so
/// the test switches to the bus clock (mode 0x80) to see the count run (one tick per two
/// instructions).
#[test]
fn the_profiling_timer_starts_and_counts() {
    let Some(mut ee) = loaded() else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return;
    };
    ee.call(0x001e_35f0, &[], &[], STACK).expect("timer start");
    assert_eq!(ee.hw.peek(0x1000_0010) & 0x83, 0x83);
    let writes: Vec<_> = ee.hw.log.iter().map(|w| (w.addr, w.value)).collect();
    assert_eq!(writes, vec![(0x1000_0010, 0x83), (0x1000_0000, 0)]);
    ee.w32(0x1000_0010, 0x80);
    // 1000 instructions later the counter has advanced by about 500 ticks.
    ee.steps += 1000;
    let c = ee.r32(0x1000_0000);
    assert!((499..=501).contains(&c), "count {c}");
}

/// FUN_00134260 waits for the VIF1 DMA channel (CHCR bit 8 and the BC0 condition), VIF1_STAT, the
/// VU0 status in a COP2 control register and GIF_STAT to go idle. With the stub's idle registers it
/// returns at once; with VIF1_STAT busy it would spin until the runaway limit.
#[test]
fn the_dma_idle_wait_returns_on_idle_hardware_and_spins_on_busy() {
    let Some(mut ee) = loaded() else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return;
    };
    let v0 = ee.call(0x0013_4260, &[], &[], STACK).expect("idle wait");
    assert_eq!(v0, 0);
    assert!(ee.steps < 100, "returned within {} instructions", ee.steps);
    ee.hw.poke(0x1000_3C00, 0x1); // VIF1_STAT: VPS busy
    ee.limit = 10_000;
    assert!(matches!(ee.call(0x0013_4260, &[], &[], STACK), Err(kh2_ee::Fault::Runaway { .. })));
}
