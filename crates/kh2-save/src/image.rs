//! The PCSX2 memory card image (`.ps2`), an 8 MB card as the PS2 card file system lays it out. This is not part of the game (the game
//! talks to the IOP's card library, which hides it); the layout below was read from a real PCSX2 card of the user and from the public
//! descriptions of the PS2 card file system, and every field used here is checked against that card by `tests/card_image.rs`.
//!
//! - The file is 16384 pages of 512 data bytes and 16 spare bytes (8,650,752 bytes); a card without spare bytes (8,388,608 bytes) is
//!   also read. A cluster is two pages (1024 data bytes). The spare bytes hold 3 ECC bytes per 128-byte chunk and 4 zero bytes; a page
//!   that was never written keeps its spare bytes at 0xff.
//! - Page 0 is the superblock: the text `Sony PS2 Memory Card Format `, the version, page size, pages per cluster, pages per block,
//!   clusters per card, the first allocatable cluster (`alloc_offset`), the number of allocatable clusters (`alloc_end`), the root
//!   directory cluster, the indirect FAT cluster list, the bad block list, the card type and flags.
//! - The FAT: the indirect list (`ifc_list`) names clusters holding 256 FAT cluster numbers each; a FAT cluster holds 256 entries. An
//!   entry belongs to an allocatable cluster (numbered from `alloc_offset`); `0x7fffffff` is free, `0xffffffff` ends a chain, else bit 31
//!   is set and the low bits name the next cluster.
//! - A directory is a chain of clusters of 512-byte entries (two per cluster); the count of entries is in the `length` of its own `.`
//!   entry (or of its entry in the parent). An entry: halfword mode (`0x8427` directory, `0x8497` file; bit 15 set = exists), length,
//!   created time (8 bytes: 0, second, minute, hour, day, month, year halfword), first cluster, the parent entry index (in `.`), modified
//!   time, attributes, and at `+0x40` the 32-byte name.

use std::fmt;

/// Pages of a card.
pub const PAGES: usize = 16384;
/// Data bytes per page.
pub const PAGE: usize = 512;
/// Page plus spare bytes.
pub const RAW_PAGE: usize = 528;
/// File size with spare bytes.
pub const RAW_LEN: usize = PAGES * RAW_PAGE;
/// File size without spare bytes.
pub const PLAIN_LEN: usize = PAGES * PAGE;
/// Bytes per cluster.
pub const CLUSTER: usize = 1024;
/// Bytes per directory entry.
pub const ENTRY: usize = 512;
/// The magic text of the superblock.
pub const MAGIC: &[u8; 28] = b"Sony PS2 Memory Card Format ";
/// Directory entry mode of a directory.
pub const MODE_DIR: u16 = 0x8427;
/// Directory entry mode of a file.
pub const MODE_FILE: u16 = 0x8497;

/// What can go wrong reading or writing a card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The file has neither card size.
    Size(usize),
    /// The superblock text is missing.
    Magic,
    /// An inconsistency (named).
    Corrupt(&'static str),
    /// Not enough free clusters.
    Full,
    /// A name does not fit the 32-byte field.
    Name,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Size(n) => write!(f, "{n} bytes is not the size of an 8 MB card image"),
            Error::Magic => write!(f, "the superblock text is missing"),
            Error::Corrupt(w) => write!(f, "corrupt card: {w}"),
            Error::Full => write!(f, "the card is full"),
            Error::Name => write!(f, "name too long"),
        }
    }
}

impl std::error::Error for Error {}

/// A directory entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub mode: u16,
    pub length: u32,
    pub created: [u8; 8],
    /// The first cluster, relative to the allocation offset.
    pub cluster: u32,
    pub parent_index: u32,
    pub modified: [u8; 8],
    pub attr: u32,
    pub name: String,
}

impl DirEntry {
    pub fn exists(&self) -> bool {
        self.mode & 0x8000 != 0 && self.mode != 0xffff
    }

    pub fn is_dir(&self) -> bool {
        self.exists() && self.mode & 0x20 != 0
    }

    pub fn is_file(&self) -> bool {
        self.exists() && self.mode & 0x10 != 0
    }

    fn parse(b: &[u8]) -> DirEntry {
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let name = &b[0x40..0x60];
        let n = name.iter().position(|&c| c == 0).unwrap_or(32);
        DirEntry {
            mode: u16::from_le_bytes([b[0], b[1]]),
            length: u32_at(4),
            created: b[8..16].try_into().unwrap(),
            cluster: u32_at(0x10),
            parent_index: u32_at(0x14),
            modified: b[0x18..0x20].try_into().unwrap(),
            attr: u32_at(0x20),
            name: String::from_utf8_lossy(&name[..n]).into_owned(),
        }
    }

    fn to_bytes(&self) -> [u8; ENTRY] {
        let mut b = [0u8; ENTRY];
        b[0..2].copy_from_slice(&self.mode.to_le_bytes());
        b[4..8].copy_from_slice(&self.length.to_le_bytes());
        b[8..16].copy_from_slice(&self.created);
        b[0x10..0x14].copy_from_slice(&self.cluster.to_le_bytes());
        b[0x14..0x18].copy_from_slice(&self.parent_index.to_le_bytes());
        b[0x18..0x20].copy_from_slice(&self.modified);
        b[0x20..0x24].copy_from_slice(&self.attr.to_le_bytes());
        let n = self.name.as_bytes();
        b[0x40..0x40 + n.len()].copy_from_slice(n);
        b
    }
}

/// The superblock fields the reader and writer use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Superblock {
    pub page_size: u16,
    pub pages_per_cluster: u16,
    pub pages_per_block: u16,
    pub clusters_per_card: u32,
    pub alloc_offset: u32,
    pub alloc_end: u32,
    pub root_cluster: u32,
    pub ifc_list: [u32; 32],
}

const ECC_MASKS: [u8; 7] = [0x55, 0x33, 0x0f, 0x00, 0xaa, 0xcc, 0xf0];

/// The three ECC bytes of a 128-byte chunk: column parity, then the two line parities [inferred from the public descriptions of the card
/// format and checked against every page of the user's card by `tests/card_image.rs`].
pub fn ecc(chunk: &[u8]) -> [u8; 3] {
    let mut column = 0x77u8;
    let mut line0 = 0x7fu8;
    let mut line1 = 0x7fu8;
    for (i, &b) in chunk.iter().enumerate().take(128) {
        let mut m = 0u8;
        for (j, &mask) in ECC_MASKS.iter().enumerate() {
            m |= (((b & mask).count_ones() & 1) as u8) << j;
        }
        column ^= m;
        if b.count_ones() & 1 != 0 {
            line0 ^= !(i as u8);
            line1 ^= i as u8;
        }
    }
    [column, line0 & 0x7f, line1]
}

/// An 8 MB card: every page with its spare bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Card {
    raw: Vec<u8>,
}

impl fmt::Debug for Card {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Card({} bytes)", self.raw.len())
    }
}

impl Card {
    /// Reads a card image: with spare bytes (8,650,752 bytes) or without (8,388,608; the spare bytes are made).
    pub fn from_bytes(bytes: &[u8]) -> Result<Card, Error> {
        let card = match bytes.len() {
            RAW_LEN => Card { raw: bytes.to_vec() },
            PLAIN_LEN => {
                let mut c = Card { raw: vec![0xff; RAW_LEN] };
                for p in 0..PAGES {
                    c.write_page(p, bytes[p * PAGE..(p + 1) * PAGE].try_into().unwrap());
                }
                c
            }
            n => return Err(Error::Size(n)),
        };
        if &card.raw[..MAGIC.len()] != MAGIC {
            return Err(Error::Magic);
        }
        let sb = card.superblock();
        if sb.page_size != 512 || sb.pages_per_cluster != 2 || sb.clusters_per_card as usize != PAGES / 2 {
            return Err(Error::Corrupt("superblock geometry"));
        }
        Ok(card)
    }

    /// The card as a file with spare bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.raw.clone()
    }

    /// The data of a card without spare bytes.
    pub fn to_plain(&self) -> Vec<u8> {
        (0..PAGES).flat_map(|p| self.page(p).to_vec()).collect()
    }

    fn page(&self, p: usize) -> &[u8] {
        &self.raw[p * RAW_PAGE..p * RAW_PAGE + PAGE]
    }

    /// Writes a page and its spare bytes (ECC per 128-byte chunk, then four zero bytes).
    fn write_page(&mut self, p: usize, data: &[u8; PAGE]) {
        let o = p * RAW_PAGE;
        self.raw[o..o + PAGE].copy_from_slice(data);
        for c in 0..4 {
            let e = ecc(&data[c * 128..(c + 1) * 128]);
            self.raw[o + PAGE + c * 3..o + PAGE + c * 3 + 3].copy_from_slice(&e);
        }
        self.raw[o + PAGE + 12..o + PAGE + 16].fill(0);
    }

    /// The data of a cluster (absolute number).
    pub fn cluster(&self, c: usize) -> Vec<u8> {
        let mut v = self.page(2 * c).to_vec();
        v.extend_from_slice(self.page(2 * c + 1));
        v
    }

    fn set_cluster(&mut self, c: usize, data: &[u8]) {
        let mut d = data.to_vec();
        d.resize(CLUSTER, 0xff);
        self.write_page(2 * c, d[..PAGE].try_into().unwrap());
        self.write_page(2 * c + 1, d[PAGE..].try_into().unwrap());
    }

    pub fn superblock(&self) -> Superblock {
        let s = self.page(0);
        let u16_at = |o: usize| u16::from_le_bytes([s[o], s[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes(s[o..o + 4].try_into().unwrap());
        let mut ifc_list = [0u32; 32];
        for (i, w) in ifc_list.iter_mut().enumerate() {
            *w = u32_at(0x50 + 4 * i);
        }
        Superblock {
            page_size: u16_at(0x28),
            pages_per_cluster: u16_at(0x2a),
            pages_per_block: u16_at(0x2c),
            clusters_per_card: u32_at(0x30),
            alloc_offset: u32_at(0x34),
            alloc_end: u32_at(0x38),
            root_cluster: u32_at(0x3c),
            ifc_list,
        }
    }

    fn fat_location(&self, n: u32) -> (usize, usize) {
        let sb = self.superblock();
        let fat_no = (n / 256) as usize;
        let ifc = self.cluster(sb.ifc_list[fat_no / 256 % 32] as usize);
        let o = (fat_no % 256) * 4;
        let fat_cluster = u32::from_le_bytes(ifc[o..o + 4].try_into().unwrap()) as usize;
        (fat_cluster, (n % 256) as usize * 4)
    }

    /// The FAT entry of allocatable cluster `n`.
    pub fn fat(&self, n: u32) -> u32 {
        let (c, o) = self.fat_location(n);
        let d = self.cluster(c);
        u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
    }

    fn set_fat(&mut self, n: u32, v: u32) {
        let (c, o) = self.fat_location(n);
        let mut d = self.cluster(c);
        d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        self.set_cluster(c, &d);
    }

    /// The clusters of a chain from `first` (relative numbers).
    pub fn chain(&self, first: u32) -> Result<Vec<u32>, Error> {
        let sb = self.superblock();
        let mut out = vec![first];
        let mut c = first;
        loop {
            if c >= sb.alloc_end {
                return Err(Error::Corrupt("cluster beyond the allocatable area"));
            }
            let v = self.fat(c);
            if v == 0xffff_ffff {
                return Ok(out);
            }
            if v & 0x8000_0000 == 0 || out.len() > sb.alloc_end as usize {
                return Err(Error::Corrupt("broken cluster chain"));
            }
            c = v & 0x7fff_ffff;
            out.push(c);
        }
    }

    fn abs(&self, rel: u32) -> usize {
        (self.superblock().alloc_offset + rel) as usize
    }

    /// `len` bytes of the data chain starting at `first`.
    pub fn read_chain(&self, first: u32, len: usize) -> Result<Vec<u8>, Error> {
        let mut out = Vec::with_capacity(len);
        for c in self.chain(first)? {
            out.extend_from_slice(&self.cluster(self.abs(c)));
            if out.len() >= len {
                break;
            }
        }
        if out.len() < len {
            return Err(Error::Corrupt("file shorter than its chain"));
        }
        out.truncate(len);
        Ok(out)
    }

    /// The entries of the directory whose chain starts at `first` and which holds `count` entries (deleted ones included).
    fn read_entries(&self, first: u32, count: u32) -> Result<Vec<DirEntry>, Error> {
        let data = self.read_chain(first, (count as usize).div_ceil(2) * CLUSTER)?;
        Ok((0..count as usize).map(|i| DirEntry::parse(&data[i * ENTRY..(i + 1) * ENTRY])).collect())
    }

    /// The entries of the root directory (`.` and `..` included, deleted entries left out).
    pub fn root(&self) -> Result<Vec<DirEntry>, Error> {
        let first = self.superblock().root_cluster;
        let head = DirEntry::parse(&self.read_chain(first, ENTRY)?);
        Ok(self.read_entries(first, head.length)?.into_iter().filter(DirEntry::exists).collect())
    }

    /// The entries of a directory entry (`.` and `..` included, deleted entries left out).
    pub fn entries(&self, dir: &DirEntry) -> Result<Vec<DirEntry>, Error> {
        if !dir.is_dir() {
            return Err(Error::Corrupt("not a directory"));
        }
        Ok(self.read_entries(dir.cluster, dir.length)?.into_iter().filter(DirEntry::exists).collect())
    }

    /// The directory entry of a name in the root.
    pub fn find(&self, name: &str) -> Result<Option<DirEntry>, Error> {
        Ok(self.root()?.into_iter().find(|e| e.is_dir() && e.name == name))
    }

    /// The names of the directories in the root other than `.` and `..`.
    pub fn directories(&self) -> Result<Vec<String>, Error> {
        Ok(self.root()?.into_iter().filter(|e| e.is_dir() && e.name != "." && e.name != "..").map(|e| e.name).collect())
    }

    /// A file inside a root directory.
    pub fn read_file(&self, dir: &str, file: &str) -> Result<Option<Vec<u8>>, Error> {
        let Some(d) = self.find(dir)? else { return Ok(None) };
        let Some(f) = self.entries(&d)?.into_iter().find(|e| e.is_file() && e.name == file) else { return Ok(None) };
        if f.length == 0 {
            return Ok(Some(Vec::new()));
        }
        self.read_chain(f.cluster, f.length as usize).map(Some)
    }

    /// Free allocatable clusters.
    pub fn free_clusters(&self) -> u32 {
        let sb = self.superblock();
        (0..sb.alloc_end).filter(|&n| self.fat(n) & 0x8000_0000 == 0).count() as u32
    }

    fn allocate(&mut self, n: usize) -> Result<Vec<u32>, Error> {
        let sb = self.superblock();
        let free: Vec<u32> = (0..sb.alloc_end).filter(|&c| self.fat(c) & 0x8000_0000 == 0).take(n).collect();
        if free.len() < n {
            return Err(Error::Full);
        }
        for (i, &c) in free.iter().enumerate() {
            self.set_fat(c, if i + 1 < n { 0x8000_0000 | free[i + 1] } else { 0xffff_ffff });
        }
        Ok(free)
    }

    fn free_chain(&mut self, first: u32) -> Result<(), Error> {
        for c in self.chain(first)? {
            self.set_fat(c, 0x7fff_ffff);
        }
        Ok(())
    }

    /// Writes data over a fresh chain of clusters and returns the first one (`None` for no data).
    fn write_data(&mut self, data: &[u8]) -> Result<Option<u32>, Error> {
        if data.is_empty() {
            return Ok(None);
        }
        let clusters = self.allocate(data.len().div_ceil(CLUSTER))?;
        for (i, &c) in clusters.iter().enumerate() {
            let abs = self.abs(c);
            self.set_cluster(abs, &data[i * CLUSTER..data.len().min((i + 1) * CLUSTER)]);
        }
        Ok(Some(clusters[0]))
    }

    /// Removes a root directory with its files (nothing when it does not exist). The entry is marked deleted, as the card library does.
    pub fn remove_dir(&mut self, name: &str) -> Result<(), Error> {
        let sb = self.superblock();
        let root = self.chain(sb.root_cluster)?;
        let head = DirEntry::parse(&self.read_chain(sb.root_cluster, ENTRY)?);
        for i in 0..head.length as usize {
            let abs = self.abs(root[i / 2]);
            let mut cl = self.cluster(abs);
            let e = DirEntry::parse(&cl[(i % 2) * ENTRY..(i % 2 + 1) * ENTRY]);
            if e.is_dir() && e.name == name && name != "." && name != ".." {
                for f in self.entries(&e)? {
                    if f.is_file() && f.length > 0 {
                        self.free_chain(f.cluster)?;
                    }
                }
                self.free_chain(e.cluster)?;
                cl[(i % 2) * ENTRY..(i % 2) * ENTRY + 2].copy_from_slice(&(e.mode & 0x7fff).to_le_bytes());
                self.set_cluster(abs, &cl);
                return Ok(());
            }
        }
        Ok(())
    }

    /// Writes a save directory into the root: an existing directory of the name is removed first. `stamp` is the time (8 bytes as in the
    /// directory entries) given to everything written.
    pub fn write_dir(&mut self, name: &str, files: &[(&str, &[u8])], stamp: [u8; 8]) -> Result<(), Error> {
        if name.len() > 31 || files.iter().any(|(n, _)| n.len() > 31) {
            return Err(Error::Name);
        }
        let backup = self.clone();
        let r = self.write_dir_inner(name, files, stamp);
        if r.is_err() {
            *self = backup;
        }
        r
    }

    fn write_dir_inner(&mut self, name: &str, files: &[(&str, &[u8])], stamp: [u8; 8]) -> Result<(), Error> {
        self.remove_dir(name)?;
        let sb = self.superblock();
        // The root: where the new entry goes, and whether the chain must grow.
        let root_chain = self.chain(sb.root_cluster)?;
        let head = DirEntry::parse(&self.read_chain(sb.root_cluster, ENTRY)?);
        // A deleted entry's slot is reused; else the entry is appended.
        let slots = self.read_entries(sb.root_cluster, head.length)?;
        let index = slots.iter().position(|e| e.mode & 0x8000 == 0).unwrap_or(head.length as usize);
        let appended = index == head.length as usize;
        // The file data first, then the directory's own clusters.
        let mut entries = Vec::new();
        for (n, data) in files {
            let first = self.write_data(data)?;
            entries.push(DirEntry { mode: MODE_FILE, length: data.len() as u32, created: stamp, cluster: first.unwrap_or(0xffff_ffff), parent_index: 0, modified: stamp, attr: 0, name: (*n).to_string() });
        }
        let dir_entries = entries.len() + 2;
        let dir_clusters = self.allocate(dir_entries.div_ceil(2))?;
        let dot = |name: &str, parent: u32| DirEntry { mode: MODE_DIR, length: 0, created: stamp, cluster: 0, parent_index: parent, modified: stamp, attr: 0, name: name.into() };
        let mut all = vec![dot(".", index as u32), dot("..", 0)];
        all.extend(entries);
        for (ci, &c) in dir_clusters.iter().enumerate() {
            let mut cl = vec![0xffu8; CLUSTER];
            for k in 0..2 {
                if let Some(e) = all.get(ci * 2 + k) {
                    cl[k * ENTRY..(k + 1) * ENTRY].copy_from_slice(&e.to_bytes());
                }
            }
            let abs = self.abs(c);
            self.set_cluster(abs, &cl);
        }
        // The root gets the new entry, extending its chain when the last cluster is full.
        let mut chain = root_chain;
        if index / 2 >= chain.len() {
            let extra = self.allocate(1)?[0];
            let last = *chain.last().unwrap();
            self.set_fat(last, 0x8000_0000 | extra);
            let abs = self.abs(extra);
            self.set_cluster(abs, &vec![0xffu8; CLUSTER]);
            chain.push(extra);
        }
        let entry = DirEntry { mode: MODE_DIR, length: dir_entries as u32, created: stamp, cluster: dir_clusters[0], parent_index: 0, modified: stamp, attr: 0, name: name.into() };
        let abs = self.abs(chain[index / 2]);
        let mut cl = self.cluster(abs);
        cl[(index % 2) * ENTRY..(index % 2 + 1) * ENTRY].copy_from_slice(&entry.to_bytes());
        self.set_cluster(abs, &cl);
        // The root's own `.` entry counts its entries.
        let abs0 = self.abs(chain[0]);
        let mut cl0 = self.cluster(abs0);
        if appended {
            cl0[4..8].copy_from_slice(&(index as u32 + 1).to_le_bytes());
        }
        cl0[0x18..0x20].copy_from_slice(&stamp);
        self.set_cluster(abs0, &cl0);
        Ok(())
    }

    /// An empty formatted 8 MB card (the layout of the user's PCSX2 card: root directory at allocatable cluster 0, indirect list at cluster
    /// 8, FAT clusters 9 to 40, allocatable area from 41).
    pub fn format(stamp: [u8; 8]) -> Card {
        let mut c = Card { raw: vec![0xff; RAW_LEN] };
        // Every page erased, with the ECC an erased page carries.
        let erased = [0xffu8; PAGE];
        for p in 0..PAGES {
            c.write_page(p, &erased);
        }
        let mut sb = [0u8; PAGE];
        sb[..28].copy_from_slice(MAGIC);
        sb[28..35].copy_from_slice(b"1.2.0.0");
        sb[0x28..0x2a].copy_from_slice(&512u16.to_le_bytes());
        sb[0x2a..0x2c].copy_from_slice(&2u16.to_le_bytes());
        sb[0x2c..0x2e].copy_from_slice(&16u16.to_le_bytes());
        sb[0x2e..0x30].copy_from_slice(&0xff00u16.to_le_bytes());
        sb[0x30..0x34].copy_from_slice(&8192u32.to_le_bytes());
        sb[0x34..0x38].copy_from_slice(&41u32.to_le_bytes());
        sb[0x38..0x3c].copy_from_slice(&8135u32.to_le_bytes());
        sb[0x3c..0x40].copy_from_slice(&0u32.to_le_bytes());
        sb[0x40..0x44].copy_from_slice(&0x3ffu32.to_le_bytes());
        sb[0x44..0x48].copy_from_slice(&0x3feu32.to_le_bytes());
        sb[0x50..0x54].copy_from_slice(&8u32.to_le_bytes());
        sb[0xd0..0x150].fill(0xff);
        sb[0x150] = 2;
        sb[0x151] = 0x2b;
        sb[0x154..0x158].copy_from_slice(&0x400u32.to_le_bytes());
        sb[0x158..0x15c].copy_from_slice(&0x100u32.to_le_bytes());
        sb[0x15c..0x160].copy_from_slice(&8u32.to_le_bytes());
        c.write_page(0, &sb);
        // The indirect list names the 32 FAT clusters 9..=40.
        let mut ifc = vec![0xffu8; CLUSTER];
        for i in 0..32u32 {
            ifc[i as usize * 4..i as usize * 4 + 4].copy_from_slice(&(9 + i).to_le_bytes());
        }
        c.set_cluster(8, &ifc);
        // The FAT: everything free but the root directory's one cluster.
        for f in 0..32usize {
            let mut fat = vec![0u8; CLUSTER];
            for e in 0..256 {
                let n = f * 256 + e;
                let v: u32 = if n == 0 {
                    0xffff_ffff
                } else if n < 8135 {
                    0x7fff_ffff
                } else {
                    0xffff_ffff
                };
                fat[e * 4..e * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            c.set_cluster(9 + f, &fat);
        }
        let dot = |name: &str, mode: u16, len: u32| DirEntry { mode, length: len, created: stamp, cluster: 0, parent_index: 0, modified: stamp, attr: 0, name: name.into() };
        let mut root = vec![0xffu8; CLUSTER];
        root[..ENTRY].copy_from_slice(&dot(".", MODE_DIR, 2).to_bytes());
        root[ENTRY..].copy_from_slice(&dot("..", 0xa426, 0).to_bytes());
        c.set_cluster(41, &root);
        c
    }
}
