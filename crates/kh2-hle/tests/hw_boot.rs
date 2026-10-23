//! The executable's boot on the Rust kernel with the hardware pages: no register access outside
//! the contract (HYB-boot section 3). Skipped without the disc.
//!
//! Without the IOP services the boot waits in the IOP reboot call at 0x166ee8, so this test covers
//! the boot up to that call and the idle time after it. The title-screen loop with the IOP joined
//! (600 frames, the VBlank syscall rates of section 2.1) is in `tests/bringup.rs`.

use kh2_ee::Ee;
use kh2_hle::RunStop;
use kh2_hle::hw::HwMachine;

#[test]
fn boot_to_the_iop_boot_call_touches_only_listed_registers_and_idles_cleanly_after() {
    let Some(elf) = kh2_factory::exe_or_skip("hw boot") else { return };
    let mut m = HwMachine::new();
    m.load_elf(&elf).expect("elf");
    m.machine.ee.probes.insert(0x166ee8, |ee: &mut Ee| ee.request_stop());
    let r = m.run(400_000_000).expect("no fault");
    assert_eq!(r, RunStop::Requested, "reached the IOP boot call");
    // libdma's init (main call 4) wrote every channel's registers and D_CTRL, D_STAT, D_PCR.
    assert!(m.hw.writes >= 50, "the libdma reset and VIF0 reset are about 58 register writes, got {}", m.hw.writes);
    assert!(m.hw.dma_starts.iter().all(|&n| n == 0), "no transfer yet: {:?}", m.hw.dma_starts);
    m.hw.assert_clean();
    // Past the call, with no IOP the boot ends up waiting; 3 virtual seconds of that stay clean.
    m.machine.ee.probes.remove(&0x166ee8);
    let before = m.machine.kernel.borrow().clock.vsyncs;
    m.machine.idle_limit = 3 * kh2_hle::kernel::EE_HZ;
    m.run(100_000_000).expect("no fault");
    let k = m.machine.kernel.borrow();
    assert!(k.clock.vsyncs > before, "the virtual clock kept running");
    assert!(k.unimplemented.is_empty(), "no unimplemented syscall: {:?}", k.unimplemented);
    drop(k);
    m.hw.assert_clean();
}
