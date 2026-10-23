//! BAR: KH2's generic container. Header `"BAR\x01"`, u32 count, 8 bytes padding, then
//! `count` x {u16 type, u16 duplicate, char tag[4], u32 offset, u32 size}. Per OpenKH `docs/kh2/file/type/bar.md`.

pub mod request;
pub mod resident;

#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    pub kind: u16,
    pub tag: &'a str,
    pub data: &'a [u8],
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    NotBar,
    Truncated,
}

pub fn is_bar(data: &[u8]) -> bool {
    data.starts_with(b"BAR\x01")
}

pub fn parse(data: &[u8]) -> Result<Vec<Entry<'_>>, Error> {
    if !is_bar(data) {
        return Err(Error::NotBar);
    }
    let u16_at = |o: usize| data.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or(Error::Truncated);
    let u32_at = |o: usize| {
        data.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize).ok_or(Error::Truncated)
    };
    let n = u32_at(4)?;
    (0..n)
        .map(|i| {
            let o = 16 + 16 * i;
            let tag_bytes = data.get(o + 4..o + 8).ok_or(Error::Truncated)?;
            let end = tag_bytes.iter().position(|&b| b == 0).unwrap_or(4);
            let tag = std::str::from_utf8(&tag_bytes[..end]).map_err(|_| Error::Truncated)?;
            let (off, size) = (u32_at(o + 8)?, u32_at(o + 12)?);
            Ok(Entry { kind: u16_at(o)?, tag, data: data.get(off..off + size).ok_or(Error::Truncated)? })
        })
        .collect()
}

/// First entry with this tag (tags can repeat; the game's lookup also matches the type).
pub fn find<'a>(entries: &[Entry<'a>], tag: &str) -> Option<Entry<'a>> {
    entries.iter().copied().find(|e| e.tag == tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_bar() {
        let mut d = b"BAR\x01".to_vec();
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&[0; 8]);
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        d.extend_from_slice(b"abcd");
        d.extend_from_slice(&32u32.to_le_bytes());
        d.extend_from_slice(&3u32.to_le_bytes());
        d.extend_from_slice(b"xyz");
        let e = parse(&d).unwrap();
        assert_eq!((e[0].kind, e[0].tag, e[0].data), (2, "abcd", &b"xyz"[..]));
        assert!(parse(b"nope").is_err());
    }
}
