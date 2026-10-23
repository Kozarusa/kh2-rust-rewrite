//! The register pages game code touches (boot contract section 3) and the DMA channel map.
//!
//! A write to a register outside these pages is "unimplemented": the boot contract lists the
//! pages from an instruction scan of the executable and four overlays, so an access elsewhere
//! means a mode the scan did not cover and the contract needs extending (HYB-boot section 10).

/// One page of registers and the HLE behaviour that answers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub name: &'static str,
    /// First address of the page.
    pub lo: u32,
    /// One past the last address.
    pub hi: u32,
    /// The HLE column of the contract.
    pub hle: &'static str,
}

const fn page(name: &'static str, lo: u32, hi: u32, hle: &'static str) -> Page {
    Page { name, lo, hi, hle }
}

/// Pages in address order. The IPU, its FIFO and the DMA channels 3 and 4 are movie-only
/// (`MOVIE_ONLY`); VU memory is plain storage in `kh2-ee` and never logged.
pub const PAGES: &[Page] = &[
    page("T0", 0x1000_0000, 0x1000_0040, "COUNT follows the HBLANK count of the virtual clock"),
    page("T1", 0x1000_0800, 0x1000_0840, "stored"),
    page("T2", 0x1000_1000, 0x1000_1040, "stored; overflow flag and INTC 11 come from the kernel clock"),
    page("T3", 0x1000_1800, 0x1000_1840, "stored"),
    page("GIF", 0x1000_3000, 0x1000_30B0, "idle: STAT reads 0"),
    page("VIF0", 0x1000_3800, 0x1000_3900, "accepted"),
    page("VIF0 FIFO", 0x1000_4000, 0x1000_4010, "swallowed"),
    page("VIF1", 0x1000_3C00, 0x1000_3D00, "idle: STAT reads 0"),
    page("VIF1 FIFO", 0x1000_5000, 0x1000_5010, "swallowed"),
    page("D1 VIF1", 0x1000_9000, 0x1000_9100, "completes at once"),
    page("D2 GIF", 0x1000_A000, 0x1000_A100, "completes at once"),
    page("D5 SIF0", 0x1000_C000, 0x1000_C100, "idle; arming sets no status bit"),
    page("D8 SPR_FROM", 0x1000_D000, 0x1000_D100, "real copy"),
    page("D9 SPR_TO", 0x1000_D400, 0x1000_D500, "real copy"),
    page("DMAC", 0x1000_E000, 0x1000_E070, "D_CTRL stored; D_STAT write-1-to-clear status, toggled mask"),
    page("INTC", 0x1000_F000, 0x1000_F020, "STAT write-1-to-clear, MASK toggled"),
    page("SIO", 0x1000_F100, 0x1000_F1C0, "ISR reads transmit ready; TXFIFO swallowed"),
    page("D_ENABLER", 0x1000_F520, 0x1000_F524, "reads D_ENABLEW"),
    page("D_ENABLEW", 0x1000_F590, 0x1000_F594, "stored"),
    page("GS PMODE", 0x1200_0000, 0x1200_0008, "recorded"),
    page("GS SMODE2", 0x1200_0020, 0x1200_0028, "recorded"),
    page("GS DISPFB/DISPLAY", 0x1200_0070, 0x1200_00B0, "recorded"),
    page("GS EXTDATA..BGCOLOR", 0x1200_00C0, 0x1200_00F0, "recorded"),
    page("GS CSR", 0x1200_1000, 0x1200_1018, "FIELD per vsync, FINISH set, VSINT per vsync"),
    page("GS BUSDIR", 0x1200_1040, 0x1200_1048, "recorded"),
];

/// Pages that only a movie reaches; the movie hook keeps game code out of them.
pub const MOVIE_ONLY: &[Page] = &[
    page("IPU", 0x1000_2000, 0x1000_2040, "movie"),
    page("IPU FIFO", 0x1000_7000, 0x1000_7020, "movie"),
    page("D3 IPU_FROM", 0x1000_B000, 0x1000_B100, "movie"),
    page("D4 IPU_TO", 0x1000_B400, 0x1000_B500, "movie"),
];

/// The page an address is in, or `None` for an access outside the contract.
pub fn page_of(addr: u32) -> Option<&'static Page> {
    PAGES.iter().find(|p| (p.lo..p.hi).contains(&addr))
}

/// The movie-only page an address is in.
pub fn movie_page_of(addr: u32) -> Option<&'static Page> {
    MOVIE_ONLY.iter().find(|p| (p.lo..p.hi).contains(&addr))
}

/// Base address of each DMA channel's registers (CHCR at +0, MADR +0x10, QWC +0x20, TADR +0x30,
/// ASR0 +0x40, ASR1 +0x50, SADR +0x80) [EE user's manual].
pub const CHANNEL_BASE: [u32; 10] =
    [0x1000_8000, 0x1000_9000, 0x1000_A000, 0x1000_B000, 0x1000_B400, 0x1000_C000, 0x1000_C400, 0x1000_C800, 0x1000_D000, 0x1000_D400];

/// The channel whose CHCR is at `addr`.
pub fn channel_of_chcr(addr: u32) -> Option<u32> {
    CHANNEL_BASE.iter().position(|&b| b == addr).map(|c| c as u32)
}

/// Channels whose other end is the IOP: only a SIF event completes them, never the CHCR write.
pub fn iop_channel(ch: u32) -> bool {
    (5..=7).contains(&ch)
}
