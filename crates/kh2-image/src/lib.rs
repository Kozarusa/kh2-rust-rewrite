//! KH2 2D UI data: images (IMGD, IMGZ), sprite sequences (SEQD), fonts and system messages, as
//! stored in the `.2dd`, `.2ld` and message/font BAR archives. Layouts per OpenKH `docs/kh2/file/type/image.md` and
//! `OpenKh.Kh2/Sequence.cs` (Apache-2.0); see CREDITS.md. Fonts and the message code were decoded from
//! the files and the game's own text code.

pub mod font;
pub mod imgd;
pub mod layd;
pub mod seqd;
pub mod text;
pub mod window;

#[cfg(test)]
mod tests;

pub(crate) fn rd32(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).unwrap_or(0)
}

pub(crate) fn rd16(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map(|s| u16::from_le_bytes(s.try_into().unwrap())).unwrap_or(0)
}
