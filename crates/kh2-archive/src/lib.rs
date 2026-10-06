//! KH2 archive layer: `KH2.IDX` + `KH2.IMG` (and the per-world `000xx.idx` sub-indexes stored inside it),
//! plus the code overlays in `OVL.IDX` + `OVL.IMG`.
//!
//! Format and algorithms per OpenKH (Apache-2.0): `docs/kh2/file/type/idx.md`, `OpenKh.Kh2/Idx.cs`,
//! `OpenKh.Kh2/Img.cs`. Reimplemented from the documented behaviour; see CREDITS.md.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::Path;

use kh2_iso::{Iso, SECTOR};

/// The only edition this crate is validated against.
pub const SUPPORTED_BOOT: &str = "SLUS_210.05";

/// Set to `1` to let [`Archive::open`] accept a disc whose boot executable is not [`SUPPORTED_BOOT`]
/// (for research on another edition, e.g. Final Mix `SLPM_666.75`; see `docs/final-mix.md`).
/// Without it other discs are refused, because table layouts and executable addresses differ.
pub const ALLOW_OTHER_DISC_ENV: &str = "KH2_ALLOW_OTHER_DISC";

/// The boot executable named by a `SYSTEM.CNF` (`BOOT2 = cdrom0:\SLUS_210.05;1` gives `SLUS_210.05`).
pub fn boot_name(cnf: &str) -> Option<String> {
    let line = cnf.lines().find(|l| l.trim_start().to_ascii_uppercase().starts_with("BOOT2"))?;
    let value = line.split_once('=')?.1.trim();
    let name = value.rsplit(['\\', ':']).next()?.split(';').next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Whether a disc with this `SYSTEM.CNF` may be opened: the USA release always, any other disc only
/// with the explicit opt-in.
pub fn check_disc(cnf: &str, allow_other: bool) -> Result<String> {
    if cnf.contains(SUPPORTED_BOOT) {
        return Ok(SUPPORTED_BOOT.to_string());
    }
    match boot_name(cnf) {
        Some(b) if allow_other => Ok(b),
        _ => Err(Error::UnsupportedDisc(cnf.lines().next().unwrap_or_default().to_string())),
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    UnsupportedDisc(String),
    NotFound(String),
    Corrupt(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io: {e}"),
            Error::UnsupportedDisc(s) => write!(f, "unsupported disc: {s} (expected {SUPPORTED_BOOT})"),
            Error::NotFound(s) => write!(f, "not in archive: {s}"),
            Error::Corrupt(s) => write!(f, "corrupt data: {s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// CRC-32 (poly 0x04C11DB7, MSB-first, init !0, inverted) of the file name. Primary IDX key.
pub fn hash32(name: &[u8]) -> u32 {
    let mut c: u32 = !0;
    for &b in name {
        c ^= (b as u32) << 24;
        for _ in 0..8 {
            c = if c & 0x8000_0000 != 0 { (c << 1) ^ 0x04C1_1DB7 } else { c << 1 };
        }
    }
    !c
}

/// CRC-16/CCITT (poly 0x1021, init !0, inverted) over the name **reversed**. Secondary IDX key.
pub fn hash16(name: &[u8]) -> u16 {
    let mut s: u16 = !0;
    for &b in name.iter().rev() {
        s ^= (b as u16) << 8;
        for _ in 0..8 {
            s = if s & 0x8000 != 0 { (s << 1) ^ 0x1021 } else { s << 1 };
        }
    }
    !s
}

/// KH2 backwards LZ. The stream is read from its end: trailing zero padding, a key byte, the
/// big-endian decompressed size, then tokens. `key, dist, len` copies `len + 3` bytes from
/// `dist` bytes ahead in the output (output is also filled from the end); `key, 0` is a literal key.
pub fn decompress(src: &[u8]) -> Result<Vec<u8>> {
    let mut i = src.len().checked_sub(1).ok_or(Error::Corrupt("empty LZ stream"))? as isize;
    let mut key = 0u8;
    while i > 0 {
        key = src[i as usize];
        i -= 1;
        if key != 0 {
            break;
        }
    }
    if i < 4 {
        return Err(Error::Corrupt("LZ stream too short"));
    }
    let at = |k: isize| src[k as usize] as usize;
    let size = at(i) | at(i - 1) << 8 | at(i - 2) << 16 | at(i - 3) << 24;
    i -= 4;
    let mut dst = vec![0u8; size];
    let mut d = size as isize - 1;
    while d >= 0 && i >= 0 {
        let b = src[i as usize];
        i -= 1;
        if b == key && i >= 0 {
            let dist = src[i as usize] as isize;
            i -= 1;
            if dist > 0 && i >= 0 {
                let n = src[i as usize] as usize + 3;
                i -= 1;
                for _ in 0..n {
                    if d < 0 {
                        break;
                    }
                    let from = (d + dist) as usize;
                    dst[d as usize] = *dst.get(from).ok_or(Error::Corrupt("LZ back-reference out of range"))?;
                    d -= 1;
                }
                continue;
            }
        }
        dst[d as usize] = b;
        d -= 1;
    }
    Ok(dst)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdxEntry {
    pub hash32: u32,
    pub hash16: u16,
    pub compressed: bool,
    pub streamed: bool,
    /// Stored size in sectors (meaningful when compressed).
    pub sectors: u32,
    /// Offset in sectors from the start of the IMG.
    pub offset: u32,
    /// Decompressed length in bytes.
    pub length: u32,
}

impl IdxEntry {
    pub fn key(&self) -> (u32, u16) {
        (self.hash32, self.hash16)
    }
}

pub fn parse_idx(data: &[u8]) -> Result<Vec<IdxEntry>> {
    let rd = |o: usize| -> Result<u32> {
        data.get(o..o + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .ok_or(Error::Corrupt("IDX truncated"))
    };
    let n = rd(0)? as usize;
    (0..n)
        .map(|k| {
            let o = 4 + 16 * k;
            let h = rd(o + 4)?;
            let bd = (h >> 16) as u16;
            Ok(IdxEntry {
                hash32: rd(o)?,
                hash16: h as u16,
                compressed: bd & 0x4000 != 0,
                streamed: bd & 0x8000 != 0,
                sectors: (bd & 0x3FFF) as u32 + 1,
                offset: rd(o + 8)?,
                length: rd(o + 12)?,
            })
        })
        .collect()
}

/// Where an entry was listed: the main index or a world sub-index (by its file name).
#[derive(Debug, Clone)]
pub struct Listed {
    pub entry: IdxEntry,
    pub parent: String,
    pub name: Option<String>,
}

pub struct Archive {
    iso: Iso,
    boot: String,
    img_lba: u64,
    ovl_lba: u64,
    ovl: Vec<IdxEntry>,
    entries: Vec<Listed>,
    by_key: HashMap<(u32, u16), usize>,
}

impl Archive {
    /// Opens the user's disc. Refuses discs other than the validated USA release, unless
    /// `KH2_ALLOW_OTHER_DISC=1` is set ([`ALLOW_OTHER_DISC_ENV`]).
    pub fn open(iso_path: impl AsRef<Path>) -> Result<Self> {
        let allow = std::env::var(ALLOW_OTHER_DISC_ENV).is_ok_and(|v| v.trim() == "1");
        Self::open_edition(iso_path, allow)
    }

    /// Opens a disc; `allow_other_disc` is the explicit opt-in for editions other than the USA
    /// release (research tests pass `true`; nothing else in the repository does).
    pub fn open_edition(iso_path: impl AsRef<Path>, allow_other_disc: bool) -> Result<Self> {
        let mut iso = Iso::open(iso_path)?;
        let cnf = String::from_utf8_lossy(&iso.read("SYSTEM.CNF")?).into_owned();
        let boot = check_disc(&cnf, allow_other_disc)?;
        let lba = |iso: &Iso, p: &str| iso.find(p).map(|e| e.lba as u64).ok_or_else(|| Error::NotFound(p.into()));
        let img_lba = lba(&iso, "KH2.IMG")?;
        let ovl_lba = lba(&iso, "OVL.IMG")?;
        let ovl = parse_idx(&iso.read("OVL.IDX")?)?;
        let main = parse_idx(&iso.read("KH2.IDX")?)?;
        let mut a = Archive { iso, boot, img_lba, ovl_lba, ovl, entries: Vec::new(), by_key: HashMap::new() };
        for e in main {
            a.push(e, "KH2.IDX");
        }
        Ok(a)
    }

    fn push(&mut self, entry: IdxEntry, parent: &str) {
        if self.by_key.contains_key(&entry.key()) {
            return;
        }
        self.by_key.insert(entry.key(), self.entries.len());
        self.entries.push(Listed { entry, parent: parent.to_string(), name: None });
    }

    /// Applies a name list (one file name per line) and expands every named `*.idx` sub-index.
    /// Returns how many entries are named afterwards.
    pub fn apply_names<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) -> Result<usize> {
        let mut table = HashMap::new();
        for n in names {
            let n = n.trim();
            if !n.is_empty() && !n.starts_with('#') {
                table.insert((hash32(n.as_bytes()), hash16(n.as_bytes())), n.to_string());
            }
        }
        let mut subs = Vec::new();
        for l in &mut self.entries {
            if let Some(n) = table.get(&l.entry.key()) {
                l.name = Some(n.clone());
                if n.ends_with(".idx") {
                    subs.push((l.entry, n.clone()));
                }
            }
        }
        for (e, n) in subs {
            for s in parse_idx(&self.read_entry(&e)?)? {
                self.push(s, &n);
                let last = self.entries.len() - 1;
                if self.entries[last].name.is_none() {
                    self.entries[last].name = table.get(&s.key()).cloned();
                }
            }
        }
        Ok(self.entries.iter().filter(|l| l.name.is_some()).count())
    }

    pub fn entries(&self) -> &[Listed] {
        &self.entries
    }

    pub fn iso(&mut self) -> &mut Iso {
        &mut self.iso
    }

    /// The disc's boot executable from `SYSTEM.CNF` (`SLUS_210.05` for the USA release).
    pub fn boot_executable(&self) -> &str {
        &self.boot
    }

    /// Reads a file by its in-game name, e.g. `"00battle.bin"`. Works without a name list.
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let key = (hash32(name.as_bytes()), hash16(name.as_bytes()));
        let i = *self.by_key.get(&key).ok_or_else(|| Error::NotFound(name.to_string()))?;
        let e = self.entries[i].entry;
        self.read_entry(&e)
    }

    pub fn read_entry(&mut self, e: &IdxEntry) -> Result<Vec<u8>> {
        read_from(&mut self.iso, self.img_lba, e)
    }

    pub fn overlays(&self) -> &[IdxEntry] {
        &self.ovl
    }

    pub fn read_overlay(&mut self, index: usize) -> Result<Vec<u8>> {
        let e = *self.ovl.get(index).ok_or_else(|| Error::NotFound(format!("overlay {index}")))?;
        read_from(&mut self.iso, self.ovl_lba, &e)
    }
}

fn read_from(iso: &mut Iso, base_lba: u64, e: &IdxEntry) -> Result<Vec<u8>> {
    let off = (base_lba + e.offset as u64) * SECTOR;
    if e.compressed {
        let raw = iso.read_at(off, e.sectors as usize * SECTOR as usize)?;
        let out = decompress(&raw)?;
        if out.len() != e.length as usize {
            return Err(Error::Corrupt("decompressed size differs from IDX length"));
        }
        Ok(out)
    } else {
        Ok(iso.read_at(off, e.length as usize)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_match_reference_values() {
        // Cross-checked against the OpenKH algorithm: these keys resolve "00battle.bin" in KH2.IDX.
        assert_eq!(hash32(b"00battle.bin"), 0x2029_c445);
        assert_eq!(hash16(b"00battle.bin"), 0x176f);
    }

    #[test]
    fn decompress_literal_and_backref() {
        // Output "ABABAB": literals B,A at the end, then copy 4 bytes (1+3) from distance 2.
        // Stream is read backwards, so tokens are stored in reverse order.
        let key = 0xFF;
        let mut s = vec![1u8, 2, key]; // len-3=1, dist=2, key  (read as key, dist, len)
        s.extend_from_slice(b"AB"); // read as 'B' then 'A'
        s.extend_from_slice(&[0, 0, 0, 6, key]); // size (big-endian, read backwards) + key
        assert_eq!(decompress(&s).unwrap(), b"ABABAB");
    }

    #[test]
    fn other_discs_need_the_explicit_opt_in() {
        let usa = "BOOT2 = cdrom0:\\SLUS_210.05;1\r\nVER = 1.00\r\nVMODE = NTSC\r\n";
        let fm = "BOOT2 = cdrom0:\\SLPM_666.75;1\r\nVER = 1.01\r\nVMODE = NTSC\r\n";
        assert_eq!(boot_name(usa).as_deref(), Some("SLUS_210.05"));
        assert_eq!(boot_name(fm).as_deref(), Some("SLPM_666.75"));
        assert_eq!(check_disc(usa, false).unwrap(), "SLUS_210.05");
        assert_eq!(check_disc(usa, true).unwrap(), "SLUS_210.05");
        assert!(matches!(check_disc(fm, false), Err(Error::UnsupportedDisc(_))));
        assert_eq!(check_disc(fm, true).unwrap(), "SLPM_666.75");
        assert!(check_disc("garbage", true).is_err());
    }

    #[test]
    fn decompress_rejects_garbage() {
        assert!(decompress(&[]).is_err());
        assert!(decompress(&[0, 0, 0]).is_err());
    }
}
