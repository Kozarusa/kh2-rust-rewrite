//! Just enough PS2 hardware behaviour to turn KH2 asset bytes into vertices and pixels.
//!
//! * [`vif`]: VIF1 command stream → VU1 data memory (the game's models are VIF programs).
//! * [`dma`]: DMA source-chain walking (cnt/ref/next/ret/end tags) to assemble a VIF stream.
//! * [`vpu`]: the VU1 memory layout KH2's microprograms consume (strip nodes, colors, vertices, weights).
//! * [`gs`]: GS local-memory swizzling, for textures uploaded as PSMCT32 but sampled as PSMT8/PSMT4.
//!
//! Behaviour per Sony's public VIF/GS descriptions as documented by the PS2 homebrew community,
//! KH2 specifics per OpenKH (Apache-2.0); see CREDITS.md.

pub mod dma;
pub mod gs;
pub mod vif;
pub mod vpu;

pub(crate) fn rd32(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).unwrap_or(0)
}

pub(crate) fn rd16(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map(|s| u16::from_le_bytes(s.try_into().unwrap())).unwrap_or(0)
}
