//! Reader for PCSX2 save states (`.p2s`, version 2.8.2).
//!
//! A `.p2s` is a zip archive. The entries that matter here: `eeMemory.bin` (the 32 MiB of EE RAM from address 0, so every address is the
//! one the executable uses) and `PCSX2 Savestate Version.id`. The user setting `SavestateCompressionType = 2` stores them with zstd
//! (zip method 93); the version and screenshot entries are stored (method 0). Only those two methods are read; anything else is an
//! error that names the method.
//!
//! The zip reader is the central-directory kind (no streaming, no zip64): a state is a few tens of MiB, read whole.

use std::io::Read;
use std::path::Path;

/// The EE RAM entry.
pub const EE_MEMORY: &str = "eeMemory.bin";
/// Size of the EE RAM in a state.
pub const EE_SIZE: usize = 0x0200_0000;

const SIG_EOCD: u32 = 0x0605_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_LOCAL: u32 = 0x0403_4b50;
const METHOD_STORED: u16 = 0;
const METHOD_ZSTD: u16 = 93;

/// One entry of the zip's central directory.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub method: u16,
    pub compressed: usize,
    pub size: usize,
    local_header: usize,
}

/// A zip archive held in memory.
pub struct Zip {
    data: Vec<u8>,
    entries: Vec<Entry>,
}

fn u16_at(d: &[u8], o: usize) -> Result<u16, String> {
    d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(|| format!("zip: truncated at {o:#x}"))
}

fn u32_at(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| format!("zip: truncated at {o:#x}"))
}

impl Zip {
    /// Parses the central directory of `data`.
    pub fn parse(data: Vec<u8>) -> Result<Zip, String> {
        // The end-of-central-directory record sits in the last 64 KiB + 22 bytes.
        let lo = data.len().saturating_sub(0x1_0000 + 22);
        let eocd = (lo..data.len().saturating_sub(21)).rev().find(|&i| u32_at(&data, i) == Ok(SIG_EOCD)).ok_or("zip: no end-of-central-directory record (not a .p2s?)")?;
        let count = usize::from(u16_at(&data, eocd + 10)?);
        let dir_size = u32_at(&data, eocd + 12)? as usize;
        let dir_at = u32_at(&data, eocd + 16)? as usize;
        if count == 0xffff || dir_size == u32::MAX as usize || dir_at == u32::MAX as usize {
            return Err("zip: zip64 archives are not supported".into());
        }
        let mut entries = Vec::with_capacity(count);
        let mut o = dir_at;
        for _ in 0..count {
            if u32_at(&data, o)? != SIG_CENTRAL {
                return Err(format!("zip: bad central directory entry at {o:#x}"));
            }
            let method = u16_at(&data, o + 10)?;
            let compressed = u32_at(&data, o + 20)? as usize;
            let size = u32_at(&data, o + 24)? as usize;
            let nlen = usize::from(u16_at(&data, o + 28)?);
            let elen = usize::from(u16_at(&data, o + 30)?);
            let clen = usize::from(u16_at(&data, o + 32)?);
            let local_header = u32_at(&data, o + 42)? as usize;
            let name = data.get(o + 46..o + 46 + nlen).ok_or("zip: truncated entry name")?;
            entries.push(Entry { name: String::from_utf8_lossy(name).into_owned(), method, compressed, size, local_header });
            o += 46 + nlen + elen + clen;
        }
        Ok(Zip { data, entries })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The bytes of the entry `name`, decompressed.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        let e = self.entries.iter().find(|e| e.name == name).ok_or_else(|| format!("zip: no entry '{name}'"))?;
        if u32_at(&self.data, e.local_header)? != SIG_LOCAL {
            return Err(format!("zip: bad local header of '{name}'"));
        }
        let nlen = usize::from(u16_at(&self.data, e.local_header + 26)?);
        let elen = usize::from(u16_at(&self.data, e.local_header + 28)?);
        let at = e.local_header + 30 + nlen + elen;
        let raw = self.data.get(at..at + e.compressed).ok_or_else(|| format!("zip: '{name}' runs past the end of the file"))?;
        let out = match e.method {
            METHOD_STORED => raw.to_vec(),
            METHOD_ZSTD => {
                let mut dec = ruzstd::decoding::StreamingDecoder::new(raw).map_err(|e| format!("zstd: {e}"))?;
                let mut out = Vec::with_capacity(e.size);
                dec.read_to_end(&mut out).map_err(|e| format!("zstd: {name}: {e}"))?;
                out
            }
            m => return Err(format!("zip: '{name}' uses compression method {m}; only stored (0) and zstd (93) are read")),
        };
        if out.len() != e.size {
            return Err(format!("zip: '{name}' is {} bytes, the directory says {}", out.len(), e.size));
        }
        Ok(out)
    }
}

/// A save state: the zip plus the EE memory read out of it.
pub struct P2s {
    /// `PCSX2 Savestate Version.id`, the version string after the leading u32 (for example `v2.8.2`), if present.
    pub version: Option<String>,
    /// The 32 MiB of EE RAM.
    pub ee: Vec<u8>,
}

impl P2s {
    pub fn from_bytes(data: Vec<u8>) -> Result<P2s, String> {
        let zip = Zip::parse(data)?;
        let version = zip.read("PCSX2 Savestate Version.id").ok().and_then(|v| v.get(4..).map(|s| String::from_utf8_lossy(s.split(|b| *b == 0).next().unwrap_or(&[])).into_owned()));
        let ee = zip.read(EE_MEMORY)?;
        if ee.len() != EE_SIZE {
            return Err(format!("{EE_MEMORY} is {:#x} bytes, expected {EE_SIZE:#x}", ee.len()));
        }
        Ok(P2s { version, ee })
    }

    pub fn open(path: &Path) -> Result<P2s, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        P2s::from_bytes(data).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Test helpers: a zip builder with stored and zstd (raw block) entries, so no game data is needed.
#[cfg(test)]
pub(crate) mod testzip {
    /// A zstd frame holding `data` as raw blocks (valid for any size: blocks of at most 128 KiB). The frame header carries no size.
    pub fn zstd_raw(data: &[u8]) -> Vec<u8> {
        let mut f = vec![0x28, 0xb5, 0x2f, 0xfd];
        // Frame header descriptor 0: no checksum, no dictionary, window descriptor follows; window 1 MiB (exponent 0 mantissa 0 = 1 KiB..).
        f.push(0x00);
        f.push(0x58); // window descriptor: exponent 11 -> 2 MiB window, plenty for 128 KiB blocks
        let blocks: Vec<&[u8]> = if data.is_empty() { vec![&[][..]] } else { data.chunks(0x1_0000).collect() };
        for (i, b) in blocks.iter().enumerate() {
            let last = u32::from(i + 1 == blocks.len());
            let hdr = last | ((b.len() as u32) << 3);
            f.extend_from_slice(&hdr.to_le_bytes()[..3]);
            f.extend_from_slice(b);
        }
        f
    }

    /// `(name, method, payload as stored, uncompressed size)` entries to a zip file.
    pub fn build(entries: &[(&str, u16, Vec<u8>, usize)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, method, payload, size) in entries {
            let at = out.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0]);
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&0u32.to_le_bytes()); // crc (not checked)
            out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            out.extend_from_slice(&(*size as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(payload);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&[20, 0, 20, 0, 0, 0]);
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0; 4]);
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            central.extend_from_slice(&(*size as u32).to_le_bytes());
            central.extend_from_slice(&(name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&at.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let dir_at = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&dir_at.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::testzip::*;
    use super::*;

    #[test]
    fn a_stored_zip_returns_its_entries() {
        let z = Zip::parse(build(&[("a.txt", 0, b"hello".to_vec(), 5), ("dir/b.bin", 0, vec![1, 2, 3], 3)])).unwrap();
        assert_eq!(z.entries().len(), 2);
        assert_eq!(z.read("a.txt").unwrap(), b"hello");
        assert_eq!(z.read("dir/b.bin").unwrap(), vec![1, 2, 3]);
        assert!(z.read("missing").unwrap_err().contains("no entry"));
    }

    #[test]
    fn a_zstd_entry_is_decoded() {
        let payload: Vec<u8> = (0..200_000u32).map(|i| (i * 7 + i / 300) as u8).collect();
        let z = Zip::parse(build(&[("big", METHOD_ZSTD, zstd_raw(&payload), payload.len())])).unwrap();
        assert_eq!(z.read("big").unwrap(), payload);
    }

    #[test]
    fn unsupported_methods_and_size_mismatches_are_errors() {
        let z = Zip::parse(build(&[("d", 8, vec![0; 4], 9), ("s", 0, vec![1; 4], 5)])).unwrap();
        assert!(z.read("d").unwrap_err().contains("method 8"));
        assert!(z.read("s").unwrap_err().contains("directory says 5"));
    }

    #[test]
    fn garbage_is_not_a_zip() {
        assert!(Zip::parse(vec![0; 100]).is_err());
        assert!(Zip::parse(Vec::new()).is_err());
    }

    #[test]
    fn a_state_needs_a_full_size_ee_image() {
        let short = build(&[(EE_MEMORY, 0, vec![0; 16], 16)]);
        assert!(P2s::from_bytes(short).err().unwrap().contains("expected"));
        let mut id = vec![0, 0, 0x59, 0x9a];
        id.extend_from_slice(b"v2.8.2");
        let ok = build(&[("PCSX2 Savestate Version.id", 0, id.clone(), id.len()), (EE_MEMORY, METHOD_ZSTD, zstd_raw(&vec![0u8; EE_SIZE]), EE_SIZE)]);
        let s = P2s::from_bytes(ok).unwrap();
        assert_eq!(s.version.as_deref(), Some("v2.8.2"));
        assert_eq!(s.ee.len(), EE_SIZE);
    }
}
