//! Port factory (PLAN R12): classify the original's functions by a static scan, write a brief and an oracle fuzz test
//! for one function, and run that test: kh2-ee executes the original on generated inputs laid out in its memory, the
//! port runs on copies of the same inputs, and returns, written blocks and floats are compared.
//!
//! Everything here reads the user's own executable at run time (from the disc, or a local copy named by `KH2_EXE`);
//! nothing of the game is stored in the repository.

pub mod fuzz;
pub mod scan;

use std::path::{Path, PathBuf};

/// The repository root (this crate sits in `crates/kh2-factory`).
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The private reverse-engineering workspace: `KH2_PS2`, else the first `kh2_ps2` folder next to the repository or
/// next to one of its parents (a worktree under `.claude/worktrees/` finds the main checkout's neighbour).
pub fn ps2_root() -> PathBuf {
    if let Ok(p) = std::env::var("KH2_PS2") {
        return PathBuf::from(p);
    }
    let root = repo_root().canonicalize().unwrap_or_else(|_| repo_root());
    root.ancestors()
        .skip(1)
        .map(|a| a.join("kh2_ps2"))
        .find(|p| p.is_dir())
        .unwrap_or_else(|| repo_root().join("..").join("kh2_ps2"))
}

fn iso_path() -> Option<String> {
    let p = std::env::var("KH2_ISO")
        .ok()
        .or_else(|| std::fs::read_to_string(repo_root().join("local").join("iso.txt")).ok().map(|s| s.trim().to_string()))?;
    Path::new(&p).exists().then_some(p)
}

/// Gate mode (FAC-4): with `KH2_REQUIRE_DISC=1` a test that finds no disc fails instead of passing vacuously.
pub fn require_disc() -> bool {
    std::env::var("KH2_REQUIRE_DISC").is_ok_and(|v| v == "1")
}

/// What a disc test does when the disc is missing: prints why it skips, or panics in gate mode.
pub fn skip(what: &str) {
    if require_disc() {
        panic!("{what}: no disc image (set KH2_ISO or local/iso.txt), and KH2_REQUIRE_DISC=1 makes that a failure");
    }
    eprintln!("skipped: {what}: no disc image (set KH2_ISO or local/iso.txt)");
}

/// The user's disc (`KH2_ISO` or `local/iso.txt`), or `None` after [`skip`]: the one helper disc tests share.
pub fn disc(what: &str) -> Option<kh2_archive::Archive> {
    let a = iso_path().and_then(|p| kh2_archive::Archive::open(p).ok());
    if a.is_none() {
        skip(what);
    }
    a
}

/// The main executable: `KH2_EXE` (a local copy) or read from the disc image (`KH2_ISO` or `local/iso.txt`).
/// `None` without either; a test should then call [`skip`] (or use [`exe_or_skip`]).
pub fn exe() -> Option<Vec<u8>> {
    if let Ok(p) = std::env::var("KH2_EXE")
        && let Ok(b) = std::fs::read(p)
    {
        return Some(b);
    }
    let mut a = kh2_archive::Archive::open(iso_path()?).ok()?;
    ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())
}

/// [`exe`] for tests: `None` after [`skip`] when there is no executable.
pub fn exe_or_skip(what: &str) -> Option<Vec<u8>> {
    let e = exe();
    if e.is_none() {
        skip(what);
    }
    e
}

/// The loadable bytes of an ELF image as (base address, bytes) segments, plus overlays added by the caller.
pub struct Image {
    pub segs: Vec<(u32, Vec<u8>)>,
    /// End of the initialised data of the main executable (loads below it read the executable's own tables).
    pub data_end: u32,
}

impl Image {
    pub fn from_elf(elf: &[u8]) -> Option<Image> {
        let u16_ = |o: usize| Some(u16::from_le_bytes(elf.get(o..o + 2)?.try_into().ok()?) as usize);
        let u32_ = |o: usize| Some(u32::from_le_bytes(elf.get(o..o + 4)?.try_into().ok()?));
        let (phoff, phent, phnum) = (u32_(0x1c)? as usize, u16_(0x2a)?, u16_(0x2c)?);
        let mut segs = Vec::new();
        let mut data_end = 0;
        for i in 0..phnum {
            let p = phoff + i * phent;
            if u32_(p)? != 1 {
                continue;
            }
            let (off, vaddr, filesz) = (u32_(p + 4)? as usize, u32_(p + 8)?, u32_(p + 16)? as usize);
            data_end = data_end.max(vaddr + filesz as u32);
            segs.push((vaddr, elf.get(off..off + filesz)?.to_vec()));
        }
        Some(Image { segs, data_end })
    }

    pub fn word(&self, a: u32) -> Option<u32> {
        self.segs.iter().find_map(|(b, d)| {
            let o = a.checked_sub(*b)? as usize;
            Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
        })
    }
}

/// One row of the function sheets of the reverse-engineering workspace (address, size, callers, callees, system).
#[derive(Clone, Debug)]
pub struct FnRow {
    pub id: String,
    pub image: String,
    pub addr: u32,
    pub size: u32,
    pub callers: Vec<String>,
    pub callees: Vec<String>,
    pub system: Option<String>,
}

pub fn sheet(image: &str) -> Option<Vec<FnRow>> {
    let p = ps2_root().join("sheet").join("code").join(format!("functions_{image}.json"));
    let j: serde_json::Value = serde_json::from_slice(&std::fs::read(p).ok()?).ok()?;
    let cols: Vec<&str> = j["columns"].as_array()?.iter().filter_map(|c| c.as_str()).collect();
    let ci = |n: &str| cols.iter().position(|c| *c == n);
    let (id, addr, size, callers, callees, system) = (ci("id")?, ci("addr")?, ci("size")?, ci("callers")?, ci("callees")?, ci("system")?);
    let refs = |v: &serde_json::Value| v.as_str().map(|s| s.split_whitespace().map(String::from).collect()).unwrap_or_default();
    j["rows"]
        .as_array()?
        .iter()
        .map(|r| {
            let a = r[addr].as_str()?;
            let a = a.rsplit("::").next()?;
            Some(FnRow {
                id: r[id].as_str()?.to_string(),
                image: image.to_string(),
                addr: u32::from_str_radix(a, 16).ok()?,
                size: r[size].as_u64()? as u32,
                callers: refs(&r[callers]),
                callees: refs(&r[callees]),
                system: r[system].as_str().map(String::from),
            })
        })
        .collect()
}

/// Function addresses cited in the tree (`FUN_00xxxxxx`, `0x00xxxxxx`, `0x1a0180`, `0x001a_0180`) that are function
/// starts, split into those cited in code (`crates/`) and in docs (`docs/`).
pub fn citations(starts: &std::collections::HashSet<u32>) -> (std::collections::HashSet<u32>, std::collections::HashSet<u32>) {
    use std::collections::HashSet;
    fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != "target") {
                    walk(&p, ext, out);
                }
            } else if p.extension().is_some_and(|x| x == ext) {
                out.push(p);
            }
        }
    }
    let grab = |text: &str, set: &mut HashSet<u32>| {
        let b = text.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let start = if b[i..].starts_with(b"FUN_") {
                Some(i + 4)
            } else if b[i..].starts_with(b"0x") && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
                Some(i + 2)
            } else {
                None
            };
            if let Some(s) = start {
                let mut j = s;
                let mut hex = String::new();
                while j < b.len() && (b[j].is_ascii_hexdigit() || b[j] == b'_') && hex.len() < 9 {
                    if b[j] != b'_' {
                        hex.push(b[j] as char);
                    }
                    j += 1;
                }
                if (6..=8).contains(&hex.len())
                    && let Ok(v) = u32::from_str_radix(&hex, 16)
                    && starts.contains(&v)
                {
                    set.insert(v);
                }
                i = j.max(i + 1);
            } else {
                i += 1;
            }
        }
    };
    let root = repo_root();
    let (mut code, mut docs) = (HashSet::new(), HashSet::new());
    let mut files = Vec::new();
    walk(&root.join("crates"), "rs", &mut files);
    for f in &files {
        if let Ok(t) = std::fs::read_to_string(f) {
            grab(&t, &mut code);
        }
    }
    let mut files = Vec::new();
    walk(&root.join("docs"), "md", &mut files);
    for f in &files {
        if let Ok(t) = std::fs::read_to_string(f) {
            grab(&t, &mut docs);
        }
    }
    (code, docs)
}

pub mod layout;
pub mod ported;
