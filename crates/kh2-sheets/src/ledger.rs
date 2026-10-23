//! The function ledger (`sheets/functions.json`, PLAN section 4): one row per function of the original executable
//! and its four overlays with its size, its ledger system and its status. Addresses, sizes and statuses only (RULES
//! A5): callers and callees stay in the private reverse-engineering workspace.
//!
//! This module holds what both the preflight and the `kh2 ledger` commands need: the row model, the tree scan that
//! derives `read` / `ported` / `verified` from citations, and the ledger checks of the preflight.
//!
//! Tree conventions the scan reads (FAC 2.1):
//! - `read`: the address is cited in a doc under `docs/` (`FUN_001a0180`, `0x1a0180`, `0x001a_0180`).
//! - `ported`: a doc comment line `/// orig 0x1a0180` (overlays: `/// orig ovl0 0x1f18014`) in non-test Rust code.
//! - `verified`: the address appears in the name of a `#[test]` function (`orig_1a0180_matches_the_game`, the
//!   generated oracle tests, or any test name with the address as a `_`-separated token) or in that test's doc
//!   comment.
//!
//! Test code is a file under a `tests/` or `benches/` folder, or the rest of a file from a `#[cfg(test)]` module on
//! (brace-matched). Statuses `read`/`ported`/`verified` are only ever set by the scan; `not-needed:<reason>`, the
//! system and the note are hand-edited.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::Sheet;

/// Code images: the main executable and the four overlays (all overlays load at 0x1f18000, one at a time).
pub const IMAGES: [&str; 5] = ["main", "ovl0", "ovl1", "ovl2", "ovl3"];
pub const COLUMNS: [&str; 7] = ["addr", "image", "size", "system", "status", "evidence", "note"];
/// Statuses in order. `not-needed` always carries its reason.
pub const STATUSES: [&str; 9] = [
    "unread",
    "read",
    "ported",
    "verified",
    "not-needed:sdk",
    "not-needed:debug",
    "not-needed:hw",
    "not-needed:dead",
    "not-needed:fm",
];
/// The ledger strut: the sheet renders no code, the preflight runs [`check`] on it instead of the generic row checks.
pub const STRUT: &str = "ledger";

/// (image index into [`IMAGES`], address).
pub type Key = (u8, u32);

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub addr: u32,
    pub image: u8,
    pub size: u32,
    pub system: String,
    pub status: String,
    pub evidence: String,
    pub note: String,
}

impl Row {
    pub fn key(&self) -> Key {
        (self.image, self.addr)
    }
    pub fn done(&self) -> bool {
        self.status == "verified" || self.status.starts_with("not-needed")
    }
    pub fn to_cells(&self) -> Vec<Value> {
        vec![
            Value::String(fmt_addr(self.addr)),
            Value::String(IMAGES[self.image as usize].into()),
            Value::from(self.size),
            Value::String(self.system.clone()),
            Value::String(self.status.clone()),
            Value::String(self.evidence.clone()),
            Value::String(self.note.clone()),
        ]
    }
}

/// One ledger system: id (`core.task`), ladder step (`L1`), the `sheets/systems.json` rows it belongs to, and what it
/// holds.
#[derive(Clone, Debug, PartialEq)]
pub struct SysDef {
    pub id: String,
    pub step: String,
    pub rows: Vec<String>,
    pub what: String,
}

pub fn fmt_addr(a: u32) -> String {
    format!("0x{a:x}")
}

/// `0x1a0180`, `1a0180`, `001a0180`, `0x001a_0180`.
pub fn parse_addr(s: &str) -> Option<u32> {
    let h = s.trim().trim_start_matches("0x").replace('_', "");
    if h.is_empty() || h.len() > 8 {
        return None;
    }
    u32::from_str_radix(&h, 16).ok()
}

pub fn image_index(s: &str) -> Option<u8> {
    IMAGES.iter().position(|i| *i == s).map(|i| i as u8)
}

/// The ledger systems listed in the sheet's header (`systems`: `[id, step, [systems.json ids], what]` rows).
pub fn systems(sheet: &Sheet) -> Result<Vec<SysDef>, String> {
    let rows = sheet.meta.get("systems").and_then(Value::as_array).ok_or("functions: header has no `systems` list")?;
    rows.iter()
        .map(|r| {
            let a = r.as_array().filter(|a| a.len() == 4).ok_or_else(|| format!("functions: bad systems entry {r}"))?;
            let s = |i: usize| a[i].as_str().map(String::from).ok_or_else(|| format!("functions: bad systems entry {r}"));
            let ids = a[2].as_array().ok_or_else(|| format!("functions: bad systems entry {r}"))?;
            Ok(SysDef {
                id: s(0)?,
                step: s(1)?,
                rows: ids.iter().filter_map(Value::as_str).map(String::from).collect(),
                what: s(3)?,
            })
        })
        .collect()
}

/// Decodes the rows; every malformed row is an error (the row is skipped).
pub fn rows(sheet: &Sheet) -> (Vec<Row>, Vec<String>) {
    let mut out = Vec::with_capacity(sheet.rows.len());
    let mut errs = Vec::new();
    if sheet.columns != COLUMNS {
        errs.push(format!("functions: columns must be {COLUMNS:?}, found {:?}", sheet.columns));
        return (out, errs);
    }
    for (i, r) in sheet.rows.iter().enumerate() {
        let str_at = |c: usize| r.get(c).and_then(Value::as_str);
        let row = (|| {
            if r.len() != COLUMNS.len() {
                return Err(format!("{} cells, expected {}", r.len(), COLUMNS.len()));
            }
            let addr = str_at(0).and_then(parse_addr).ok_or("addr is not a hex address")?;
            let image = str_at(1).and_then(image_index).ok_or("unknown image")?;
            let size = r[2].as_u64().filter(|&s| s > 0 && s < 1 << 24).ok_or("size is not a positive number")? as u32;
            let s = |c: usize, n: &str| str_at(c).map(String::from).ok_or(format!("{n} is not a string"));
            Ok::<Row, String>(Row {
                addr,
                image,
                size,
                system: s(3, "system")?,
                status: s(4, "status")?,
                evidence: s(5, "evidence")?,
                note: s(6, "note")?,
            })
        })();
        match row {
            Ok(row) => out.push(row),
            Err(e) => errs.push(format!("functions row {} ({}): {e}", i + 1, r.first().map(|v| v.to_string()).unwrap_or_default())),
        }
    }
    (out, errs)
}

/// Writes the rows back into the sheet (header kept).
pub fn store(sheet: &mut Sheet, rows: &[Row]) {
    sheet.columns = COLUMNS.iter().map(|c| c.to_string()).collect();
    sheet.types = ["str", "str", "u32", "str", "str", "str", "str"].iter().map(|c| c.to_string()).collect();
    sheet.rows = rows.iter().map(Row::to_cells).collect();
}

/// Where each function is cited in the tree. Paths are relative to the repository root with `/`.
#[derive(Clone, Debug, Default)]
pub struct Cites {
    /// Any mention under `docs/`.
    pub docs: BTreeMap<Key, BTreeSet<String>>,
    /// Any mention in Rust code (test code included).
    pub code: BTreeMap<Key, BTreeSet<String>>,
    /// `/// orig` doc comment lines in non-test code.
    pub orig: BTreeMap<Key, BTreeSet<String>>,
    /// `#[test]` functions (`path::name`) that carry the address in their name or doc comment.
    pub tests: BTreeMap<Key, BTreeSet<String>>,
}

/// Function starts by address (an address that starts functions in several overlays maps to all of them).
pub fn index(rows: &[Row]) -> HashMap<u32, Vec<Key>> {
    let mut m: HashMap<u32, Vec<Key>> = HashMap::new();
    for r in rows {
        m.entry(r.addr).or_default().push(r.key());
    }
    m
}

/// Resolves a cited address: a unique start, or the main executable's when several images start a function there.
/// An address shared only by overlays is ambiguous without an image and resolves to nothing.
fn resolve(idx: &HashMap<u32, Vec<Key>>, a: u32, image: Option<u8>) -> Option<Key> {
    let ks = idx.get(&a)?;
    if let Some(i) = image {
        return ks.iter().find(|k| k.0 == i).copied();
    }
    match ks.as_slice() {
        [k] => Some(*k),
        _ => ks.iter().find(|k| k.0 == 0).copied(),
    }
}

/// Hex addresses mentioned in a text: `FUN_` + 6..8 hex digits, or `0x` + 6..8 hex digits (with `_` separators).
pub fn mentions(text: &str) -> Vec<u32> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = if b[i..].starts_with(b"FUN_") && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            Some(i + 4)
        } else if b[i..].starts_with(b"0x") && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')) {
            Some(i + 2)
        } else {
            None
        };
        let Some(s) = start else {
            i += 1;
            continue;
        };
        let mut j = s;
        let mut hex = String::new();
        while j < b.len() && (b[j].is_ascii_hexdigit() || (b[j] == b'_' && !hex.is_empty())) && hex.len() <= 8 {
            if b[j] != b'_' {
                hex.push(b[j] as char);
            }
            j += 1;
        }
        let ends = j >= b.len() || !b[j].is_ascii_alphanumeric();
        if ends && (6..=8).contains(&hex.len())
            && let Ok(v) = u32::from_str_radix(&hex, 16)
        {
            out.push(v);
        }
        i = j.max(i + 1);
    }
    out
}

/// The address of a `/// orig [ovlN] 0x...` line, with its image when one is named.
pub fn orig_line(line: &str) -> Option<(Option<u8>, u32)> {
    let r = line.trim_start().strip_prefix("///")?.trim_start().strip_prefix("orig ")?.trim_start();
    let (image, r) = match r.split_once(' ') {
        Some((im, rest)) if im.starts_with("ovl") => (Some(image_index(im)?), rest.trim_start()),
        _ => (None, r),
    };
    let h = r.strip_prefix("0x")?;
    let end = h.find(|c: char| !(c.is_ascii_hexdigit() || c == '_')).unwrap_or(h.len());
    Some((image, parse_addr(&h[..end])?))
}

/// Hex tokens of a test function name (`orig_1a0180_matches_the_game`, `fun_0019f920_heals`): `_`-separated parts of
/// 6..8 hex digits with at least one decimal digit.
pub fn name_tokens(name: &str) -> Vec<u32> {
    name.split('_')
        .filter(|t| (6..=8).contains(&t.len()) && t.chars().all(|c| c.is_ascii_hexdigit()) && t.chars().any(|c| c.is_ascii_digit()))
        .filter_map(|t| u32::from_str_radix(t, 16).ok())
        .collect()
}

/// Per line of a Rust source: whether it is test code (inside a `#[cfg(test)]` item, found by brace matching that
/// skips comments, strings and character literals).
pub fn test_lines(src: &str) -> Vec<bool> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = vec![false; lines.len()];
    // Brace depth at the start of every line.
    let mut depth_at = Vec::with_capacity(lines.len());
    let mut depth: i64 = 0;
    let mut block = 0u32;
    let mut in_str = false;
    for l in &lines {
        depth_at.push(depth);
        let b = l.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if block > 0 {
                if b[i..].starts_with(b"*/") {
                    block -= 1;
                    i += 2;
                    continue;
                }
                if b[i..].starts_with(b"/*") {
                    block += 1;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if in_str {
                if c == b'\\' {
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    in_str = false;
                }
                i += 1;
                continue;
            }
            match c {
                b'/' if b.get(i + 1) == Some(&b'/') => break,
                b'/' if b.get(i + 1) == Some(&b'*') => {
                    block += 1;
                    i += 2;
                    continue;
                }
                b'"' => in_str = true,
                b'\'' => {
                    // A character literal ('{', '\n', '\''), or a lifetime ('a) which has no closing quote.
                    if b.get(i + 1) == Some(&b'\\') {
                        if let Some(k) = b[i + 2..].iter().skip(1).position(|&x| x == b'\'') {
                            i += k + 4;
                            continue;
                        }
                    } else if b.get(i + 2) == Some(&b'\'') {
                        i += 3;
                        continue;
                    }
                }
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
            i += 1;
        }
    }
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("#[cfg(test)]") {
            let d = depth_at[i];
            // The item runs until the depth comes back to `d` after having gone above it.
            let mut j = i + 1;
            let mut opened = false;
            while j < lines.len() {
                let next = depth_at.get(j + 1).copied().unwrap_or(depth);
                out[j] = true;
                if next > d {
                    opened = true;
                }
                if (opened && next <= d) || (!opened && lines[j].trim_end().ends_with(';')) {
                    break;
                }
                j += 1;
            }
            out[i] = true;
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    v.sort();
    for p in v {
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n != "target" && n != "local") {
                walk(&p, ext, out);
            }
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push(p);
        }
    }
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// Scans one Rust source (its repository-relative path decides whether it is a test file).
pub fn scan_rust(path: &str, src: &str, idx: &HashMap<u32, Vec<Key>>, c: &mut Cites) {
    let test_file = path.split('/').any(|p| p == "tests" || p == "benches");
    let tests = test_lines(src);
    let lines: Vec<&str> = src.lines().collect();
    let mut doc: Vec<&str> = Vec::new();
    let mut pending_test = false;
    for (n, l) in lines.iter().enumerate() {
        let in_test = test_file || tests[n];
        for a in mentions(l) {
            if let Some(k) = resolve(idx, a, None) {
                c.code.entry(k).or_default().insert(path.to_string());
            }
        }
        let t = l.trim_start();
        if !in_test
            && let Some((im, a)) = orig_line(t)
            && let Some(k) = resolve(idx, a, im)
        {
            c.orig.entry(k).or_default().insert(path.to_string());
        }
        if !in_test {
            continue;
        }
        if t.starts_with("///") {
            doc.push(t);
        } else if t.starts_with("#[") {
            if t.starts_with("#[test]") {
                pending_test = true;
            }
        } else if let Some(rest) = t.strip_prefix("fn ").or_else(|| t.strip_prefix("pub fn ")) {
            if pending_test {
                let name: String = rest.chars().take_while(|ch| ch.is_alphanumeric() || *ch == '_').collect();
                let mut addrs: Vec<u32> = name_tokens(&name);
                for d in &doc {
                    addrs.extend(mentions(d));
                }
                for a in addrs {
                    if let Some(k) = resolve(idx, a, None) {
                        c.tests.entry(k).or_default().insert(format!("{path}::{name}"));
                    }
                }
            }
            doc.clear();
            pending_test = false;
        } else if !t.is_empty() && !t.starts_with("//") {
            doc.clear();
            pending_test = false;
        }
    }
}

/// Scans `crates/**/*.rs` and `docs/**/*.md` of the repository at `root`.
pub fn scan_tree(root: &Path, idx: &HashMap<u32, Vec<Key>>) -> Cites {
    let mut c = Cites::default();
    let mut files = Vec::new();
    walk(&root.join("crates"), "rs", &mut files);
    for f in &files {
        if let Ok(t) = std::fs::read_to_string(f) {
            scan_rust(&rel(root, f), &t, idx, &mut c);
        }
    }
    let mut files = Vec::new();
    walk(&root.join("docs"), "md", &mut files);
    for f in &files {
        if let Ok(t) = std::fs::read_to_string(f) {
            let p = rel(root, f);
            for a in mentions(&t) {
                if let Some(k) = resolve(idx, a, None) {
                    c.docs.entry(k).or_default().insert(p.clone());
                }
            }
        }
    }
    c
}

fn first_of(s: Option<&BTreeSet<String>>) -> Option<String> {
    let s = s?;
    let f = s.iter().next()?;
    Some(if s.len() > 1 { format!("{f} +{}", s.len() - 1) } else { f.clone() })
}

/// The status and evidence the tree gives a row: verified (a test) over ported (`/// orig`) over a hand-set
/// `not-needed` over read (a doc) over unread.
pub fn derive(row: &Row, c: &Cites) -> (String, String) {
    let k = row.key();
    if let Some(e) = first_of(c.tests.get(&k)) {
        return ("verified".into(), format!("test {e}"));
    }
    if let Some(e) = first_of(c.orig.get(&k)) {
        return ("ported".into(), format!("orig {e}"));
    }
    let doc = first_of(c.docs.get(&k)).map(|e| format!("doc {e}"));
    if row.status.starts_with("not-needed") {
        return (row.status.clone(), doc.unwrap_or_default());
    }
    match doc {
        Some(d) => ("read".into(), d),
        None => ("unread".into(), String::new()),
    }
}

/// Whether a `sheets/systems.json` row is closed: a truthy `closed` cell when the sheet has that column, otherwise a
/// `validated` cell that starts with "closed".
pub fn closed_rows(systems: &Sheet) -> BTreeSet<String> {
    systems
        .rows
        .iter()
        .filter(|r| {
            let v = if systems.col("closed").is_some() { systems.get(r, "closed") } else { systems.get(r, "validated") };
            match v {
                Value::Bool(b) => *b,
                Value::String(s) if systems.col("closed").is_some() => !s.is_empty() && s != "no",
                Value::String(s) => s.starts_with("closed"),
                _ => false,
            }
        })
        .map(|r| systems.key_of(r).to_string())
        .collect()
}

/// The ledger checks of the preflight: shape, duplicate functions, known images, statuses and systems, the systems'
/// `sheets/systems.json` rows, no closed systems.json row with an unread or read function, and (with the tree's
/// citations) no `ported` or `verified` row without the code that makes it so.
pub fn check(sheet: &Sheet, systems_sheet: Option<&Sheet>, cites: Option<&Cites>) -> Vec<String> {
    let mut errs = Vec::new();
    let defs = match systems(sheet) {
        Ok(d) => d,
        Err(e) => return vec![e],
    };
    let mut ids = BTreeSet::new();
    for d in &defs {
        if !ids.insert(d.id.as_str()) {
            errs.push(format!("functions: system {} listed twice", d.id));
        }
        if let Some(s) = systems_sheet {
            for r in &d.rows {
                if !s.rows.iter().any(|row| s.key_of(row) == r) {
                    errs.push(format!("functions: system {} maps to '{r}', which is not a row of systems", d.id));
                }
            }
        }
    }
    let (rows, mut e) = rows(sheet);
    errs.append(&mut e);
    let mut seen = BTreeSet::new();
    for r in &rows {
        let at = format!("functions[{} {}]", IMAGES[r.image as usize], fmt_addr(r.addr));
        if !seen.insert(r.key()) {
            errs.push(format!("{at}: duplicate function"));
        }
        if !STATUSES.contains(&r.status.as_str()) {
            errs.push(format!("{at}: unknown status '{}'", r.status));
        }
        if !ids.contains(r.system.as_str()) {
            errs.push(format!("{at}: unknown system '{}'", r.system));
        }
        if let Some(c) = cites {
            let k = r.key();
            if r.status == "ported" && !c.orig.contains_key(&k) && !c.tests.contains_key(&k) {
                errs.push(format!("{at}: ported, but no `/// orig` line names it (run `kh2 ledger scan`)"));
            }
            if r.status == "verified" && !c.tests.contains_key(&k) {
                errs.push(format!("{at}: verified, but no test names it (run `kh2 ledger scan`)"));
            }
        }
    }
    if let Some(s) = systems_sheet {
        let closed = closed_rows(s);
        for row_id in &closed {
            let mine: BTreeSet<&str> = defs.iter().filter(|d| d.rows.iter().any(|r| r == row_id)).map(|d| d.id.as_str()).collect();
            let open: Vec<&Row> =
                rows.iter().filter(|r| mine.contains(r.system.as_str()) && (r.status == "unread" || r.status == "read")).collect();
            if !open.is_empty() {
                errs.push(format!(
                    "systems[{row_id}] is closed but {} of its functions are unread or read (first {} in {})",
                    open.len(),
                    fmt_addr(open[0].addr),
                    open[0].system
                ));
            }
        }
    }
    errs
}

/// One line for the preflight: functions, and the share of bytes verified or not needed.
pub fn summary(sheet: &Sheet) -> String {
    let (rows, _) = rows(sheet);
    let total: u64 = rows.iter().map(|r| r.size as u64).sum();
    let done: u64 = rows.iter().filter(|r| r.done()).map(|r| r.size as u64).sum();
    format!(
        "ledger: {} functions, {:.1}% of {} KB verified or not needed",
        rows.len(),
        if total > 0 { done as f64 * 100.0 / total as f64 } else { 0.0 },
        total / 1024
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static N: AtomicUsize = AtomicUsize::new(0);

    fn temp(text: &str) -> Sheet {
        let p = std::env::temp_dir().join(format!("kh2-ledger-test-{}-{}.json", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
        std::fs::write(&p, text).unwrap();
        let s = crate::load(&p).unwrap();
        std::fs::remove_file(&p).ok();
        s
    }

    fn sheet(rows: &[[&str; 7]], extra_sys: &str) -> Sheet {
        let text = format!(
            "{{\"sheet\":\"functions\",\"key\":\"addr\",\"strut\":\"ledger\",\"systems\":[[\"core\",\"L1\",[\"app\"],\"x\"],[\"magic\",\"L4\",[\"magic.row\"],\"y\"]{extra_sys}],\"columns\":{:?},\"types\":[\"str\",\"str\",\"u32\",\"str\",\"str\",\"str\",\"str\"],\"rows\":[{}]}}",
            COLUMNS,
            rows.iter()
                .map(|r| format!("[\"{}\",\"{}\",{},\"{}\",\"{}\",\"{}\",\"{}\"]", r[0], r[1], r[2], r[3], r[4], r[5], r[6]))
                .collect::<Vec<_>>()
                .join(",")
        );
        temp(&text)
    }

    fn systems_sheet(closed: &str) -> Sheet {
        let text = format!(
            "{{\"sheet\":\"systems\",\"key\":\"id\",\"strut\":\"rust_system\",\"columns\":[\"id\",\"validated\"],\"types\":[\"key\",\"str\"],\"rows\":[[\"app\",\"yes\"],[\"magic.row\",\"{closed}\"]]}}"
        );
        temp(&text)
    }

    #[test]
    fn ledger_check_finds_duplicates_unknown_statuses_and_systems() {
        let s = sheet(
            &[
                ["0x100100", "main", "16", "core", "unread", "", ""],
                ["0x100100", "main", "16", "core", "unread", "", ""],
                ["0x1f18000", "ovl0", "8", "core", "unread", "", ""],
                ["0x1f18000", "ovl1", "8", "core", "done", "", ""],
                ["0x100200", "main", "8", "nowhere", "read", "", ""],
            ],
            "",
        );
        let e = check(&s, Some(&systems_sheet("yes")), None);
        assert_eq!(e.len(), 3, "{e:?}");
        assert!(e[0].contains("main 0x100100") && e[0].contains("duplicate"));
        assert!(e[1].contains("ovl1 0x1f18000") && e[1].contains("unknown status 'done'"));
        assert!(e[2].contains("unknown system 'nowhere'"));
        // A system that maps to a row systems.json does not have.
        let s = sheet(&[["0x100100", "main", "16", "core", "unread", "", ""]], ",[\"ui\",\"L8\",[\"ui.none\"],\"z\"]");
        let e = check(&s, Some(&systems_sheet("yes")), None);
        assert_eq!(e.len(), 1);
        assert!(e[0].contains("'ui.none'"));
    }

    #[test]
    fn ledger_check_rejects_a_closed_system_with_unread_functions() {
        let rows = [["0x100100", "main", "16", "magic", "verified", "", ""], ["0x100200", "main", "8", "magic", "read", "", ""]];
        let s = sheet(&rows, "");
        assert!(check(&s, Some(&systems_sheet("yes")), None).is_empty());
        let e = check(&s, Some(&systems_sheet("closed 2026-10-22")), None);
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("systems[magic.row] is closed but 1 of its functions"));
        let rows = [["0x100100", "main", "16", "magic", "verified", "", ""], ["0x100200", "main", "8", "magic", "not-needed:dead", "", ""]];
        assert!(check(&sheet(&rows, ""), Some(&systems_sheet("closed")), None).is_empty());
    }

    #[test]
    fn ledger_check_rejects_ported_rows_without_code() {
        let s = sheet(&[["0x100100", "main", "16", "core", "ported", "", ""], ["0x100200", "main", "8", "core", "verified", "", ""]], "");
        let (rows, _) = rows(&s);
        let mut c = Cites::default();
        let e = check(&s, None, Some(&c));
        assert_eq!(e.len(), 2, "{e:?}");
        c.orig.entry(rows[0].key()).or_default().insert("a.rs".into());
        c.tests.entry(rows[1].key()).or_default().insert("t.rs::t".into());
        assert!(check(&s, None, Some(&c)).is_empty());
    }

    #[test]
    fn mentions_and_orig_lines_parse_the_conventions() {
        assert_eq!(mentions("FUN_001a0180( and 0x1a0180, 0x001a_0180; 0x33e8 x0x1a0180 0x1a0180z"), vec![0x1a0180, 0x1a0180, 0x1a0180]);
        assert_eq!(orig_line("    /// orig 0x127b98: an angle"), Some((None, 0x127b98)));
        assert_eq!(orig_line("/// orig ovl0 0x1f18014"), Some((Some(1), 0x1f18014)));
        assert_eq!(orig_line("// orig 0x127b98"), None);
        assert_eq!(orig_line("/// original 0x127b98"), None);
        assert_eq!(name_tokens("orig_127b98_matches_the_game"), vec![0x127b98]);
        assert_eq!(name_tokens("fun_0019f920_heals_facade"), vec![0x19f920]);
    }

    #[test]
    fn tree_scan_sets_statuses_from_orig_lines_tests_and_docs() {
        let rows = vec![
            Row { addr: 0x127b98, image: 0, size: 8, system: "core".into(), status: "unread".into(), evidence: String::new(), note: String::new() },
            Row { addr: 0x148550, image: 0, size: 8, system: "core".into(), status: "unread".into(), evidence: String::new(), note: String::new() },
            Row { addr: 0x239468, image: 0, size: 8, system: "core".into(), status: "not-needed:hw".into(), evidence: String::new(), note: String::new() },
            Row { addr: 0x103b18, image: 0, size: 8, system: "core".into(), status: "unread".into(), evidence: String::new(), note: String::new() },
        ];
        let idx = index(&rows);
        let mut c = Cites::default();
        let src = "/// orig 0x127b98: wraps\npub fn wrap() { let s = \"{\"; let c = '{'; }\n\
                   #[cfg(test)]\nmod tests {\n    /// orig 0x239468 (test code does not port)\n    fn helper() {}\n    #[test]\n    fn orig_148550_matches_the_game() {}\n}\n\
                   /// orig 0x103b18\nfn after() {}\n";
        scan_rust("crates/x/src/lib.rs", src, &idx, &mut c);
        let mut docs = BTreeSet::new();
        docs.insert("docs/a.md".to_string());
        c.docs.insert((0, 0x239468), docs);
        let st: Vec<(String, String)> = rows.iter().map(|r| derive(r, &c)).collect();
        assert_eq!(st[0], ("ported".into(), "orig crates/x/src/lib.rs".into()));
        assert_eq!(st[1], ("verified".into(), "test crates/x/src/lib.rs::orig_148550_matches_the_game".into()));
        assert_eq!(st[2], ("not-needed:hw".into(), "doc docs/a.md".into()));
        assert_eq!(st[3].0, "ported", "code after the test module is library code again");
        // A file under tests/ is test code throughout: a doc comment on a test verifies, an orig line does not port.
        let mut c = Cites::default();
        scan_rust("crates/x/tests/t.rs", "/// orig 0x127b98\n/// Checks FUN_00103b18.\n#[test]\nfn heals() {}\n", &idx, &mut c);
        assert!(c.orig.is_empty());
        assert_eq!(c.tests.keys().copied().collect::<Vec<_>>(), vec![(0, 0x103b18), (0, 0x127b98)]);
    }
}
