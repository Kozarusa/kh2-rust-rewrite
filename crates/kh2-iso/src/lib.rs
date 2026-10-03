//! Minimal ISO9660 (ECMA-119) reader: walks the directory tree once and reads files by LBA.
//!
//! Only what a PS2 game disc needs: 2048-byte sectors, primary volume descriptor, no Joliet/Rock Ridge.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub const SECTOR: u64 = 2048;

#[derive(Debug, Clone)]
pub struct Entry {
    /// Path relative to the disc root, `/`-separated, version suffix (`;1`) stripped.
    pub path: String,
    pub lba: u32,
    pub size: u32,
}

pub struct Iso {
    file: File,
    entries: Vec<Entry>,
}

impl Iso {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut pvd = [0u8; SECTOR as usize];
        file.seek(SeekFrom::Start(16 * SECTOR))?;
        file.read_exact(&mut pvd)?;
        if &pvd[1..6] != b"CD001" || pvd[0] != 1 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not an ISO9660 image"));
        }
        let root = &pvd[156..156 + 34];
        let mut iso = Iso { file, entries: Vec::new() };
        let (lba, size) = (le32(root, 2), le32(root, 10));
        iso.walk(lba, size, String::new(), 0)?;
        Ok(iso)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Case-insensitive lookup by disc path, e.g. `"KH2.IDX"`.
    pub fn find(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path.eq_ignore_ascii_case(path))
    }

    pub fn read(&mut self, path: &str) -> io::Result<Vec<u8>> {
        let e = self
            .find(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{path} not on disc")))?;
        self.read_at(e.lba as u64 * SECTOR, e.size as usize)
    }

    /// Raw read at a byte offset in the image.
    pub fn read_at(&mut self, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn walk(&mut self, lba: u32, size: u32, prefix: String, depth: u32) -> io::Result<()> {
        if depth > 16 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "directory tree too deep"));
        }
        let data = self.read_at(lba as u64 * SECTOR, size as usize)?;
        let mut pos = 0usize;
        while pos < data.len() {
            let n = data[pos] as usize;
            if n == 0 {
                // Records never straddle sectors: skip padding to the next sector.
                pos = (pos / SECTOR as usize + 1) * SECTOR as usize;
                continue;
            }
            if pos + n > data.len() || n < 34 {
                break;
            }
            let rec = &data[pos..pos + n];
            pos += n;
            let name_len = rec[32] as usize;
            let raw = &rec[33..(33 + name_len).min(n)];
            if raw == [0] || raw == [1] {
                continue; // "." and ".."
            }
            let name = String::from_utf8_lossy(raw);
            let name = name.split(';').next().unwrap_or_default();
            let path = format!("{prefix}{name}");
            let (elba, esize) = (le32(rec, 2), le32(rec, 10));
            if rec[25] & 2 != 0 {
                self.walk(elba, esize, format!("{path}/"), depth + 1)?;
            } else {
                self.entries.push(Entry { path, lba: elba, size: esize });
            }
        }
        Ok(())
    }
}

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
