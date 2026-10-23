//! One test per register page: each writes through the guest-visible path (`Ee::write`), syncs the
//! model and asserts what the page's HLE column promises.

use super::pages::{PAGES, page_of};
use super::*;
use crate::kernel::{IntHandler, T2_CYCLES};

fn wr(m: &mut HwMachine, addr: u32, v: u32) {
    m.machine.ee.write(addr, &v.to_le_bytes());
}

fn rd(m: &mut HwMachine, addr: u32) -> u32 {
    m.machine.ee.r32(addr)
}

fn set_vsyncs(m: &mut HwMachine, n: u64) {
    m.machine.kernel.borrow_mut().clock.vsyncs = n;
}

#[test]
fn every_page_accepts_its_first_register_without_complaint() {
    let mut m = HwMachine::new();
    for p in PAGES {
        wr(&mut m, p.lo, 0);
        assert_eq!(page_of(p.lo).map(|q| q.name), Some(p.name), "{} is found by its own address", p.name);
    }
    m.sync();
    assert!(m.hw.unimplemented.is_empty(), "{:x?}", m.hw.unimplemented);
    assert_eq!(m.hw.writes as usize, PAGES.len());
    m.hw.assert_clean();
}

#[test]
fn a_register_outside_the_pages_is_logged_and_fails_the_clean_check() {
    let mut m = HwMachine::new();
    // The libdma reset of every channel is fine, a start of D0, D6 or the IPU channel is not, nor
    // an unlisted word beside INTC or an IPU command.
    for ch in 0..10 {
        wr(&mut m, pages::CHANNEL_BASE[ch], 0);
        wr(&mut m, pages::CHANNEL_BASE[ch] + 0x10, 0);
    }
    m.sync();
    assert!(m.hw.unimplemented.is_empty(), "resetting every channel is the libdma init");
    for a in [0x1000_8000, 0x1000_C400, 0x1000_F400, 0x1000_2000, 0x1000_B000] {
        wr(&mut m, a, 0x100);
    }
    m.sync();
    let addrs: Vec<u32> = m.hw.unimplemented.iter().map(|u| u.addr).collect();
    assert_eq!(addrs, [0x1000_8000, 0x1000_C400, 0x1000_F400, 0x1000_2000, 0x1000_B000]);
    assert_eq!(m.hw.unimplemented[3].movie_only, Some("IPU"));
    assert_eq!(m.hw.unimplemented[4].movie_only, Some("D3 IPU_FROM"));
    assert_eq!(m.hw.unimplemented[0].movie_only, None);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| m.hw.assert_clean()));
    assert!(r.is_err(), "assert_clean fails when a register outside the pages was written");
}

#[test]
fn timer0_counts_scan_lines_of_the_virtual_clock_and_follows_a_guest_write() {
    let mut m = HwMachine::new();
    wr(&mut m, T0_MODE, 0x83); // CUE, HBLANK
    m.machine.ee.steps += VBLANK_CYCLES;
    m.sync();
    // One VBlank period is one field: 262.5 lines.
    assert_eq!(rd(&mut m, T0_COUNT), 262);
    m.machine.ee.steps += 10 * VBLANK_CYCLES;
    m.sync();
    assert_eq!(rd(&mut m, T0_COUNT), (hblanks(11 * VBLANK_CYCLES) & 0xFFFF) as u32);
    // The game writes the counter: counting continues from the written value.
    wr(&mut m, T0_COUNT, 1000);
    m.sync();
    m.machine.ee.steps += VBLANK_CYCLES;
    m.sync();
    let n = rd(&mut m, T0_COUNT);
    assert!((1262..=1263).contains(&n), "1000 + one field of lines, got {n}");
    // A stopped timer keeps its value.
    wr(&mut m, T0_MODE, 0x03);
    m.sync();
    let held = rd(&mut m, T0_COUNT);
    m.machine.ee.steps += VBLANK_CYCLES;
    m.sync();
    assert_eq!(rd(&mut m, T0_COUNT), held);
    m.hw.assert_clean();
}

#[test]
fn timer2_overflow_flag_follows_the_virtual_clock_and_clears_with_a_one() {
    let mut m = HwMachine::new();
    wr(&mut m, 0x1000_1010, 0x80); // CUE
    // No thread runs: the clock advances over idle time, 20 seconds at the default limit.
    m.run(1).expect("no fault");
    let (overflows, vsyncs) = {
        let k = m.machine.kernel.borrow();
        (k.clock.t2_overflows, k.clock.vsyncs)
    };
    assert!((170..=180).contains(&overflows), "8.79 per second for 20 s, got {overflows}");
    assert!(vsyncs > 1100, "59.94 per second for 20 s, got {vsyncs}");
    assert_eq!(rd(&mut m, 0x1000_1010) & 0x800, 0x800, "overflow flag set");
    wr(&mut m, 0x1000_1010, 0x880); // write 1 to clear OVFF, keep CUE
    assert_eq!(rd(&mut m, 0x1000_1010), 0x80);
    assert_eq!(T2_CYCLES, 65_536 * 256 * 2);
    m.hw.assert_clean();
}

#[test]
fn gs_csr_field_toggles_per_vsync_finish_returns_after_a_clear() {
    let mut m = HwMachine::new();
    set_vsyncs(&mut m, 1);
    m.sync();
    let c = rd(&mut m, GS_CSR);
    assert_eq!(c & CSR_FIELD, CSR_FIELD, "odd vsync: FIELD set");
    assert_eq!(c & CSR_VSINT, CSR_VSINT);
    assert_eq!(c & CSR_FINISH, CSR_FINISH);
    assert_eq!(c >> 16, 0x551B, "identification kept");
    // The game clears FINISH and VSINT by writing ones, then waits for FINISH.
    wr(&mut m, GS_CSR, CSR_FINISH | CSR_VSINT);
    assert_eq!(rd(&mut m, GS_CSR) & (CSR_FINISH | CSR_VSINT), 0);
    set_vsyncs(&mut m, 2);
    m.sync();
    let c = rd(&mut m, GS_CSR);
    assert_eq!(c & CSR_FIELD, 0, "even vsync: FIELD clear");
    assert_eq!(c & CSR_FINISH, CSR_FINISH, "FINISH is back at the next sync");
    wr(&mut m, GS_CSR, CSR_VSINT);
    m.sync();
    assert_eq!(rd(&mut m, GS_CSR) & CSR_VSINT, 0, "no new vsync, no new VSINT");
    m.hw.assert_clean();
}

#[test]
fn gs_privileged_registers_and_busdir_are_recorded() {
    let mut m = HwMachine::new();
    m.machine.ee.write(0x1200_0000, &0x7F23u64.to_le_bytes()); // PMODE
    m.machine.ee.write(0x1200_00E0, &0x00FF_0000u64.to_le_bytes()); // BGCOLOR
    wr(&mut m, 0x1200_1040, 1); // BUSDIR
    m.sync();
    assert_eq!(m.hw.gs_priv.get(&0x1200_0000), Some(&0x7F23));
    assert_eq!(m.hw.gs_priv.get(&0x1200_00E0), Some(&0x00FF_0000));
    assert_eq!(rd(&mut m, 0x1200_1040), 1);
    m.hw.assert_clean();
}

#[test]
fn sio_reads_transmit_ready_and_swallows_the_fifo() {
    let mut m = HwMachine::new();
    assert_eq!(rd(&mut m, SIO_ISR) & 0x8000, 0, "the print loop waits for this flag to clear");
    for b in b"ok" {
        wr(&mut m, SIO_TXFIFO, *b as u32);
    }
    m.sync();
    assert_eq!(m.hw.sio_tx, b"ok");
    assert_eq!(rd(&mut m, SIO_ISR) & 0x8000, 0);
    m.hw.assert_clean();
}

#[test]
fn scratchpad_dma_really_copies_and_advances_the_channel() {
    let mut m = HwMachine::new();
    // D9 SPR_TO: 2 quadwords from RAM 0x100000 to scratchpad 0x70000010.
    for i in 0..8 {
        m.machine.ee.w32(0x10_0000 + 4 * i, 0xA0 + i);
    }
    wr(&mut m, 0x1000_D410, 0x10_0000);
    wr(&mut m, 0x1000_D480, 0x10);
    wr(&mut m, 0x1000_D420, 2);
    wr(&mut m, 0x1000_D400, 0x100);
    m.sync();
    assert_eq!(m.machine.ee.r32(0x7000_0010), 0xA0);
    assert_eq!(m.machine.ee.r32(0x7000_002C), 0xA7);
    assert_eq!(rd(&mut m, 0x1000_D400) & 0x100, 0, "STR clear");
    assert_eq!(rd(&mut m, 0x1000_D420), 0, "count used up");
    assert_eq!(rd(&mut m, 0x1000_D410), 0x10_0020, "MADR past the data");
    assert_eq!(rd(&mut m, 0x1000_D480), 0x30, "SADR past the data");
    // D8 SPR_FROM: the same two quadwords back to a different RAM place.
    wr(&mut m, 0x1000_D010, 0x20_0000);
    wr(&mut m, 0x1000_D080, 0x10);
    wr(&mut m, 0x1000_D020, 2);
    wr(&mut m, 0x1000_D000, 0x100);
    m.sync();
    assert_eq!(m.machine.ee.r32(0x20_0000), 0xA0);
    assert_eq!(m.machine.ee.r32(0x20_001C), 0xA7);
    assert_eq!(m.hw.dma_starts[8], 1);
    assert_eq!(m.hw.dma_starts[9], 1);
    m.hw.assert_clean();
}

#[test]
fn a_gif_transfer_completes_at_once_and_raises_the_line_only_when_unmasked() {
    let mut m = HwMachine::new();
    wr(&mut m, 0x1000_A020, 4);
    wr(&mut m, 0x1000_A000, 0x101);
    m.sync();
    assert_eq!(rd(&mut m, 0x1000_A000) & 0x100, 0, "STR clear");
    assert_eq!(rd(&mut m, 0x1000_A020), 0);
    assert_eq!(rd(&mut m, D_STAT) & 4, 4, "channel 2 status bit set");
    assert_eq!(m.machine.ee.cop0[13] & (1 << 11), 0, "mask off: no DMAC interrupt");
    // Unmask channel 2 (write 1 to toggle) and start again: the DMAC line goes up.
    wr(&mut m, D_STAT, 4 << 16 | 4);
    wr(&mut m, 0x1000_A000, 0x101);
    m.sync();
    assert_eq!(m.machine.ee.cop0[13] & (1 << 11), 1 << 11, "completion with the mask on interrupts");
    m.hw.assert_clean();
}

#[test]
fn arming_sif0_sets_no_status_bit_and_does_not_fire_its_handler() {
    let mut m = HwMachine::new();
    {
        let mut k = m.machine.kernel.borrow_mut();
        let id = k.intr.new_id();
        k.intr.dmac[5].push(IntHandler { id, addr: 0x31b6b8, arg: 0, gp: 0 });
    }
    wr(&mut m, D_STAT, 1 << (16 + 5)); // unmask channel 5, as _EnableDmac does
    wr(&mut m, 0x1000_C000, 0x184); // the sifcmd re-arm: chain mode, STR
    m.sync();
    assert_eq!(rd(&mut m, D_STAT) & (1 << 5), 0, "arming is not completion");
    assert_eq!(m.machine.ee.cop0[13] & (1 << 11), 0, "the DMAC line stays down");
    assert_eq!(m.hw.dma_starts[5], 1);
    // The IOP finishing the transfer is the kernel's raise_dmac, which does interrupt.
    {
        let mut k = m.machine.kernel.borrow_mut();
        k.raise_dmac(&mut m.machine.ee, 5);
    }
    assert_eq!(m.machine.ee.cop0[13] & (1 << 11), 1 << 11);
    m.hw.assert_clean();
}

#[test]
fn dmac_intc_and_enabler_pages_keep_their_register_rules() {
    let mut m = HwMachine::new();
    // D_STAT: status write-1-to-clear, mask write-1-to-toggle.
    m.machine.ee.hw.poke(D_STAT, 0x0003_0005);
    wr(&mut m, D_STAT, 0x0001_0004);
    assert_eq!(rd(&mut m, D_STAT), 0x0002_0001);
    // INTC.
    m.machine.ee.hw.poke(INTC_STAT, 0b1110);
    wr(&mut m, INTC_STAT, 0b0100);
    assert_eq!(rd(&mut m, INTC_STAT), 0b1010);
    wr(&mut m, 0x1000_F010, 0b0110);
    assert_eq!(rd(&mut m, 0x1000_F010), 0b0110);
    // D_ENABLER reads what D_ENABLEW stored; D_CTRL and D_PCR are stored.
    assert_eq!(rd(&mut m, 0x1000_F520), 0x1201, "reset value");
    wr(&mut m, 0x1000_F590, 0x1201 | 1 << 16);
    assert_eq!(rd(&mut m, 0x1000_F520), 0x1201 | 1 << 16);
    wr(&mut m, 0x1000_E000, 0x1);
    wr(&mut m, 0x1000_E020, 0x3FF_0000);
    assert_eq!((rd(&mut m, 0x1000_E000), rd(&mut m, 0x1000_E020)), (1, 0x3FF_0000));
    m.sync();
    m.hw.assert_clean();
}

#[test]
fn gif_and_vif_pages_read_idle_and_fifo_writes_are_accepted() {
    let mut m = HwMachine::new();
    assert_eq!(rd(&mut m, 0x1000_3020), 0, "GIF_STAT: no path busy, FIFO empty");
    assert_eq!(rd(&mut m, 0x1000_3C00), 0, "VIF1_STAT: idle, FQC 0");
    wr(&mut m, 0x1000_3000, 1); // GIF_CTRL reset
    wr(&mut m, 0x1000_3810, 0x2); // VIF0 FBRST
    wr(&mut m, 0x1000_3C10, 0x2); // VIF1 FBRST
    m.machine.ee.write(0x1000_5000, &[0u8; 16]); // VIF1 FIFO quadword
    m.machine.ee.write(0x1000_4000, &[0u8; 16]); // VIF0 FIFO quadword
    wr(&mut m, 0x1000_0810, 0x80); // T1
    wr(&mut m, 0x1000_1810, 0x80); // T3
    m.sync();
    assert_eq!(rd(&mut m, 0x1000_3000), 1);
    m.hw.assert_clean();
}

#[test]
fn the_movie_hook_answers_start_poll_and_stop() {
    let mut m = HwMachine::new();
    let mut call = |op: u64, arg: u64| m.machine.ee.call(movie::MOVIE_CALL, &[op, arg], &[], 0x01F0_0000).expect("call");
    assert_eq!(call(0, 0x1234), 0, "start");
    assert_eq!(call(5, 0), movie::STATE_PREPARED as u64, "poll: prepared");
    assert_eq!(call(1, 0), 0, "play: done");
    assert_eq!(call(5, 0), movie::STATE_FINISHED as u64, "poll: finished, so isMovieEnd 0x225790 is true");
    assert_eq!(call(6, 0), 0, "no picture");
    assert_eq!(call(7, 9), 0, "frame number");
    assert_eq!(call(0, 0x1234), 0, "the next movie");
    assert_eq!(call(5, 0), movie::STATE_PREPARED as u64);
    assert_eq!(m.movies.borrow().len(), 8);
    assert_eq!(m.movies.borrow()[..3], [(0, 0x1234), (5, 0), (1, 0)]);
}
