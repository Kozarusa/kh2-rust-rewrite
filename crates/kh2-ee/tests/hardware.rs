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

// ---- more of the hardware-touching functions (FAC.md section 2) ---------------------------------
//
// Each test runs a game function against the stub and checks the register writes it makes (in the
// log, in order) and the state the stub leaves. Expected values come from the function's own code
// and the EE manuals' register definitions (reset values, write-1-to-clear, STR clearing).

const GIF_CH: u32 = 0x1000_A000;
const VIF1_CH: u32 = 0x1000_9000;
const IPU_FROM_CH: u32 = 0x1000_B000;
const IPU_TO_CH: u32 = 0x1000_B400;
const D_STAT: u32 = 0x1000_E010;
const BUF: u32 = 0x0100_0000;

type Log = Vec<(u32, u8, u128)>;

fn writes(ee: &Ee) -> Log {
    ee.hw.log.iter().map(|w| (w.addr, w.size, w.value)).collect()
}

fn run(addr: u32, args: &[u64], prep: impl FnOnce(&mut Ee)) -> Option<(Ee, u64)> {
    let mut ee = loaded()?;
    prep(&mut ee);
    let v0 = ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("{addr:#x}: {f:?}"));
    Some((ee, v0))
}

fn spins(ee: &mut Ee, addr: u32, args: &[u64]) -> bool {
    ee.limit = 5_000;
    matches!(ee.call(addr, args, &[], STACK), Err(kh2_ee::Fault::Runaway { .. }))
}

/// FUN_001e4a88 starts a GIF chain: QWC 0, TADR = address | 0x80000000, D_STAT bit 1 cleared,
/// CHCR = 0x145 (chain, from memory, tag transfer, STR).
#[test]
fn gif_chain_start_writes_tadr_and_completes() {
    let Some((ee, _)) = run(0x001e_4a88, &[BUF as u64], |_| {}) else { return };
    assert_eq!(writes(&ee), vec![(GIF_CH + 0x20, 4, 0), (GIF_CH + 0x30, 4, 0x8100_0000), (D_STAT, 4, 2), (GIF_CH, 4, 0x145)]);
    assert_eq!(ee.hw.peek(GIF_CH) & 0x100, 0, "STR reads back clear");
    assert_eq!(ee.hw.peek(D_STAT) & 0x3FF, 4, "channel 2 status only");
}

/// FUN_001e4bc0 does the same for VIF1 after waiting for the VIF1 and GIF channels and VIF1_STAT
/// to go idle (idle at reset), and spins while the GIF channel runs.
#[test]
fn vif1_chain_start_waits_for_idle_hardware() {
    let Some((ee, _)) = run(0x001e_4bc0, &[BUF as u64], |_| {}) else { return };
    assert_eq!(writes(&ee), vec![(VIF1_CH + 0x20, 4, 0), (VIF1_CH + 0x30, 4, BUF as u128), (D_STAT, 4, 2), (VIF1_CH, 4, 0x145)]);
    assert_eq!(ee.hw.peek(D_STAT) & 0x3FF, 2, "channel 1 status");
    let Some(mut busy) = loaded() else { return };
    busy.hw.poke(GIF_CH, 0x100);
    assert!(spins(&mut busy, 0x001e_4bc0, &[BUF as u64]));
    assert!(busy.hw.log.is_empty(), "nothing written while busy");
}

/// FUN_001e4af0 waits for VIF1_STAT bits 0-1 to clear, then starts a transfer on the VIF1 channel.
#[test]
fn vif1_wait_and_start_ends_with_the_channel_started() {
    let Some((ee, _)) = run(0x001e_4af0, &[BUF as u64], |_| {}) else { return };
    let w = writes(&ee);
    assert_eq!(w.last().unwrap().0, VIF1_CH);
    assert_eq!(w.last().unwrap().2 & 0x105, 0x105);
    assert_eq!(ee.hw.peek(VIF1_CH) & 0x100, 0);
    let Some(mut busy) = loaded() else { return };
    busy.hw.poke(0x1000_3C00, 1);
    assert!(spins(&mut busy, 0x001e_4af0, &[0]));
}

/// FUN_002f8ae8 sends a GIF packet: QWC = (tag & 0x7fff) + 1, MADR = address, CHCR = 0x101; it
/// returns 0 when the channel is idle and polls while it is busy.
#[test]
fn gif_packet_kick_sets_qwc_from_the_tag() {
    let Some((ee, v0)) = run(0x002f_8ae8, &[BUF as u64], |ee| ee.w32(BUF, 0x10)) else { return };
    assert_eq!(v0, 0);
    assert_eq!(writes(&ee), vec![(GIF_CH + 0x20, 4, 0x11), (GIF_CH + 0x10, 4, BUF as u128), (GIF_CH, 4, 0x101)]);
    let Some(mut busy) = loaded() else { return };
    busy.hw.poke(GIF_CH, 0x100);
    assert!(spins(&mut busy, 0x002f_8ae8, &[BUF as u64]));
}

/// FUN_002fa8a0 clears bit 8 of the word it is given, holding the DMAC enable (D_ENABLEW bit 16)
/// around the access; D_ENABLER resets to 0x1201 so the writes are 0x11201 then 0x1201.
#[test]
fn dma_enable_wrapper_clears_bit_8_and_restores_the_enable() {
    let Some((mut ee, v0)) = run(0x002f_a8a0, &[BUF as u64], |ee| ee.w32(BUF, 0x1FF)) else { return };
    assert_eq!(v0 as u32, 0x1FF, "returns the old value");
    assert_eq!(ee.r32(BUF), 0xFF);
    assert_eq!(writes(&ee), vec![(0x1000_F590, 4, 0x11201), (0x1000_F590, 4, 0x1201)]);
    assert_eq!(ee.hw.peek(0x1000_F520), 0x1201, "D_ENABLER reads what D_ENABLEW last held");
}

/// FUN_0030f818 and FUN_0030f890 start the IPU_FROM and IPU_TO channels under the same wrapper.
#[test]
fn ipu_channel_starts_complete_and_leave_the_dmac_enabled() {
    let Some((ee, _)) = run(0x0030_f818, &[0x100], |_| {}) else { return };
    assert_eq!(writes(&ee), vec![(0x1000_F590, 4, 0x11201), (IPU_FROM_CH, 4, 0x100), (0x1000_F590, 4, 0x1201)]);
    assert_eq!(ee.hw.peek(D_STAT) & 0x3FF, 8, "channel 3");
    let Some((ee, _)) = run(0x0030_f890, &[0x100], |_| {}) else { return };
    assert_eq!(writes(&ee), vec![(0x1000_F590, 4, 0x11201), (IPU_TO_CH, 4, 0x100), (0x1000_F590, 4, 0x1201)]);
    assert_eq!(ee.hw.peek(D_STAT) & 0x3FF, 16, "channel 4");
}

/// FUN_0030db78 programs IPU_FROM: MADR (scratchpad addresses map to 0x80000000), QWC = bytes / 16.
#[test]
fn ipu_from_transfer_programs_madr_and_qwc() {
    let Some((ee, _)) = run(0x0030_db78, &[BUF as u64, 0x100], |_| {}) else { return };
    assert_eq!(writes(&ee), vec![(IPU_FROM_CH + 0x10, 4, BUF as u128), (IPU_FROM_CH + 0x20, 4, 0x10), (IPU_FROM_CH, 4, 0x100)]);
    let Some((ee, _)) = run(0x0030_db78, &[0x7000_1800, 0x20], |_| {}) else { return };
    assert_eq!(writes(&ee)[0], (IPU_FROM_CH + 0x10, 4, 0x8000_1800));
}

/// FUN_00304a78 sets IPU_CTRL bit 23 and fills the decoder's buffer pointers (mode 0 uses the
/// scratchpad).
#[test]
fn ipu_buffer_setup_sets_ctrl_and_scratchpad_pointers() {
    let Some((mut ee, _)) = run(0x0030_4a78, &[BUF as u64], |_| {}) else { return };
    assert_eq!(writes(&ee), vec![(0x1000_2010, 4, 0x80_0000)]);
    assert_eq!(ee.r32(BUF + 0x830), 0x7000_3600);
    assert_eq!(ee.r32(BUF + 0x5a0), 0x7000_0000);
    assert_eq!(ee.r32(BUF + 0x5a4), 0x7000_1800);
}

/// FUN_00305888 resets the IPU (FUN_00304e80 writes RST) and then sets bit 23 of IPU_CTRL. RST
/// clears itself on the hardware, so the final value is 0x800000; the first stub kept RST and
/// the register read 0x40800000.
#[test]
fn ipu_reset_leaves_only_the_bit_the_function_sets() {
    let Some((ee, v0)) = run(0x0030_5888, &[BUF as u64], |ee| ee.w32(BUF + 0x40, BUF + 0x2000)) else { return };
    assert_eq!(v0, 1);
    assert_eq!(ee.hw.peek(0x1000_2010), 0x80_0000);
    assert!(writes(&ee).contains(&(0x1000_2010, 4, 0x4000_0000)), "RST was written");
}

/// FUN_00319bc8 sends one byte to the serial port TX FIFO once SIO_ISR bit 15 (busy) is clear.
#[test]
fn serial_send_writes_one_byte_when_not_busy() {
    let Some((ee, v0)) = run(0x0031_9bc8, &[0x41], |_| {}) else { return };
    assert_eq!(v0, 0x41);
    assert_eq!(writes(&ee), vec![(0x1000_F180, 1, 0x41)]);
    let Some(mut busy) = loaded() else { return };
    busy.hw.poke(0x1000_F130, 0x8000);
    assert!(spins(&mut busy, 0x0031_9bc8, &[0x41]));
}

/// FUN_002f8840 writes the GS display registers from a five-doubleword block (PMODE, SMODE2,
/// DISPFB2, DISPLAY2, BGCOLOR); they are 64-bit writes in the log.
#[test]
fn gs_display_setup_writes_the_privileged_registers() {
    let Some((ee, _)) = run(0x002f_8840, &[BUF as u64], |ee| {
        for (i, v) in [0x65u64, 0x1, 0x2, 0x3, 0x4].iter().enumerate() {
            ee.write(BUF + 8 * i as u32, &v.to_le_bytes());
        }
    }) else {
        return;
    };
    assert_eq!(writes(&ee), vec![(0x1200_0000, 8, 0x65), (0x1200_0020, 8, 1), (0x1200_0090, 8, 2), (0x1200_00a0, 8, 3), (0x1200_00e0, 8, 4)]);
}

/// FUN_00134318 finishes with GIF_MODE = 0 after the DMA idle wait.
#[test]
fn gif_idle_wait_ends_by_clearing_gif_mode() {
    let Some((ee, v0)) = run(0x0013_4318, &[], |_| {}) else { return };
    assert_eq!(v0, 0);
    assert_eq!(writes(&ee), vec![(0x1000_3010, 4, 0)]);
}

/// FUN_002f83b0 resets VIF1 (FBRST = 1, ERR = 2), sets a bit in the VU0 status control register,
/// pushes two quadwords through the VIF1 FIFO and sets GIF_CTRL = 1.
#[test]
fn vif1_reset_pushes_two_quadwords_and_resets_the_gif() {
    let Some((ee, _)) = run(0x002f_83b0, &[], |_| {}) else { return };
    let w = writes(&ee);
    assert_eq!(&w[..2], &[(0x1000_3C10, 4, 1), (0x1000_3C20, 4, 2)]);
    assert_eq!(w.iter().filter(|x| x.0 == 0x1000_5000 && x.1 == 16).count(), 2, "two 128-bit FIFO writes");
    assert_eq!(*w.last().unwrap(), (0x1000_3000, 4, 1));
}

/// FUN_002fbdf8 resets VIF0 (MARK 0, ERR 0, FBRST 1) and pushes two quadwords through its FIFO.
#[test]
fn vif0_reset_clears_mark_and_err_and_pushes_two_quadwords() {
    let Some((ee, _)) = run(0x002f_bdf8, &[], |_| {}) else { return };
    let w = writes(&ee);
    assert_eq!(&w[..3], &[(0x1000_3830, 4, 0), (0x1000_3820, 4, 0), (0x1000_3810, 4, 1)]);
    assert_eq!(w.iter().filter(|x| x.0 == 0x1000_4000 && x.1 == 16).count(), 2);
}

/// FUN_002fa368 programs D_CTRL, D_PCR, D_SQWC, D_RBOR and D_RBSR from a block; a channel index
/// above 9 in the first byte returns -1 without touching the hardware.
#[test]
fn dmac_setup_validates_then_programs_the_controller() {
    let Some((ee, v0)) = run(0x002f_a368, &[BUF as u64], |ee| ee.write(BUF, &[10])) else { return };
    assert_eq!((v0 as u32, ee.hw.log.len()), (0xFFFF_FFFF, 0));
    let Some((ee, v0)) = run(0x002f_a368, &[BUF as u64], |ee| {
        ee.write(BUF + 4, &[0, 0, 0xCD, 0xAB]);
        ee.w32(BUF + 8, 0x1111);
        ee.w32(BUF + 12, 0x2222);
        ee.w32(BUF + 16, 0x3333);
    }) else {
        return;
    };
    assert_eq!(v0, 0);
    let w = writes(&ee);
    assert_eq!(w.iter().map(|x| x.0).collect::<Vec<_>>(), vec![0x1000_E000, 0x1000_E020, 0x1000_E030, 0x1000_E040, 0x1000_E050]);
    assert_eq!((w[2].2, w[3].2, w[4].2), (0x1111, 0x2222, 0x3333));
}

/// FUN_0030f908 saves the IPU and DMA state into a block; it needs IPU_CTRL's FIFO counts to read
/// 0 (empty) to leave its wait loop.
#[test]
fn ipu_state_save_reads_the_channel_registers_back() {
    let Some((mut ee, _)) = run(0x0030_f908, &[BUF as u64], |ee| {
        ee.hw.poke(IPU_TO_CH + 0x10, 0x1234);
        ee.hw.poke(IPU_TO_CH + 0x30, 0x5678);
        ee.hw.poke(IPU_TO_CH + 0x20, 9);
    }) else {
        return;
    };
    assert_eq!((ee.r32(BUF), ee.r32(BUF + 4), ee.r32(BUF + 8)), (0x1234, 0x5678, 9));
    assert_eq!(ee.r32(BUF + 32) & 0xFF, 0, "FIFO counts read empty");
}
