//! The Spreadsheet Method (see docs/METHOD.md).
//!
//! * `sheets/*.json` are the source of truth. Columns are variables, rows are objects.
//! * A cell is **checked** when it is not `null`. A row with any unchecked cell is unfinished.
//! * Every row generates code (its *strut*), chosen by the sheet's `strut` field.
//! * [`preflight`] overlaps all sheets before anything compiles; `kh2-tables/build.rs` runs it, so
//!   `cargo build` cannot skip it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

pub mod ledger;
pub mod strut;

#[derive(Debug, Clone)]
pub struct Sheet {
    pub name: String,
    pub key: String,
    pub strut: String,
    pub columns: Vec<String>,
    pub types: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    /// All header fields (doc, hand, ...), preserved on save.
    pub meta: Map<String, Value>,
    pub path: PathBuf,
}

impl Sheet {
    pub fn col(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c == name)
    }

    pub fn key_of<'a>(&self, row: &'a [Value]) -> &'a str {
        self.col(&self.key).and_then(|i| row.get(i)).and_then(Value::as_str).unwrap_or("?")
    }

    /// Unchecked (null) columns of a row.
    pub fn unchecked(&self, row: &[Value]) -> Vec<&str> {
        self.columns.iter().zip(row).filter(|(_, v)| v.is_null()).map(|(c, _)| c.as_str()).collect()
    }

    pub fn get<'a>(&self, row: &'a [Value], col: &str) -> &'a Value {
        self.col(col).and_then(|i| row.get(i)).unwrap_or(&Value::Null)
    }

    /// Writes the sheet back: header on one line, one row per line (grep- and diff-friendly).
    pub fn save(&self) -> std::io::Result<()> {
        let mut head = self.meta.clone();
        head.insert("columns".into(), self.columns.clone().into());
        head.insert("types".into(), self.types.clone().into());
        let h = Value::Object(head).to_string();
        let mut out = format!("{},\"rows\":[\n", &h[..h.len() - 1]);
        for (i, r) in self.rows.iter().enumerate() {
            out.push_str(&Value::Array(r.clone()).to_string());
            out.push_str(if i + 1 < self.rows.len() { ",\n" } else { "\n" });
        }
        out.push_str("]}\n");
        std::fs::write(&self.path, out)
    }
}

pub fn load(path: &Path) -> Result<Sheet, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("{}: invalid JSON: {e}", path.display()))?;
    let mut meta = v.as_object().cloned().ok_or_else(|| format!("{}: not an object", path.display()))?;
    let take_strs = |meta: &mut Map<String, Value>, k: &str| -> Vec<String> {
        meta.remove(k)
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|x| x.as_str().unwrap_or_default().to_string())
            .collect()
    };
    let columns = take_strs(&mut meta, "columns");
    let types = take_strs(&mut meta, "types");
    let rows = meta
        .remove("rows")
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.as_array().cloned().unwrap_or_default())
        .collect();
    let s = |k: &str| meta.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    Ok(Sheet { name: s("sheet"), key: s("key"), strut: s("strut"), columns, types, rows, path: path.into(), meta })
}

pub fn load_dir(dir: &Path) -> Result<BTreeMap<String, Sheet>, String> {
    let mut out = BTreeMap::new();
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    for p in paths {
        let s = load(&p)?;
        out.insert(s.name.clone(), s);
    }
    Ok(out)
}

#[derive(Debug, Default)]
pub struct Report {
    /// Block the build.
    pub errors: Vec<String>,
    /// (sheet, row key, unchecked columns)
    pub unfinished: Vec<(String, String, Vec<String>)>,
    /// Overlap gaps that are not cell-level (e.g. a crate that does not exist yet).
    pub gaps: Vec<String>,
    pub warnings: Vec<String>,
    /// sheet -> (rows, finished)
    pub grid: BTreeMap<String, (usize, usize)>,
    /// The function ledger's one-line summary (`sheets/functions.json`), when the sheet exists.
    pub ledger: Option<String>,
}

impl Report {
    pub fn ok(&self, strict: bool) -> bool {
        self.errors.is_empty() && (!strict || (self.unfinished.is_empty() && self.gaps.is_empty()))
    }

    pub fn render(&self, detail: usize) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "{:12} {:>5} {:>5}  unchecked", "sheet", "rows", "done");
        for (name, (rows, done)) in &self.grid {
            let mut cols: BTreeMap<&str, usize> = BTreeMap::new();
            for (_, _, c) in self.unfinished.iter().filter(|u| &u.0 == name) {
                for x in c {
                    *cols.entry(x).or_default() += 1;
                }
            }
            let cols: Vec<String> = cols.iter().map(|(c, n)| format!("{c}:{n}")).collect();
            let _ = writeln!(s, "{name:12} {rows:>5} {done:>5}  {}", cols.join(" "));
        }
        for g in self.gaps.iter().take(detail) {
            let _ = writeln!(s, "gap: {g}");
        }
        if self.gaps.len() > detail {
            let _ = writeln!(s, "gap: ... {} more", self.gaps.len() - detail);
        }
        for w in &self.warnings {
            let _ = writeln!(s, "warning: {w}");
        }
        if let Some(l) = &self.ledger {
            let _ = writeln!(s, "{l}");
        }
        for e in self.errors.iter().take(detail.max(20)) {
            let _ = writeln!(s, "ERROR: {e}");
        }
        if self.errors.len() > detail.max(20) {
            let _ = writeln!(s, "ERROR: ... {} more", self.errors.len() - detail.max(20));
        }
        let total: usize = self.grid.values().map(|g| g.0).sum();
        let done: usize = self.grid.values().map(|g| g.1).sum();
        let _ = writeln!(
            s,
            "preflight: {} - {} errors, {done}/{total} rows finished, {} gaps",
            if self.errors.is_empty() { "PASS" } else { "FAIL" },
            self.errors.len(),
            self.gaps.len()
        );
        s
    }
}

/// Overlaps every sheet with every other sheet. `root` is the repository root.
pub fn preflight(root: &Path) -> Report {
    let mut r = Report::default();
    let sheets = match load_dir(&root.join("sheets")) {
        Ok(s) => s,
        Err(e) => {
            r.errors.push(e);
            return r;
        }
    };
    let keys: BTreeMap<&str, BTreeSet<&str>> =
        sheets.iter().map(|(n, s)| (n.as_str(), s.rows.iter().map(|row| s.key_of(row)).collect())).collect();
    let unfinished_rows: BTreeSet<(String, String)> = sheets
        .values()
        .filter(|s| s.strut != ledger::STRUT)
        .flat_map(|s| {
            s.rows.iter().filter(|row| row.iter().any(Value::is_null)).map(move |row| (s.name.clone(), s.key_of(row).to_string()))
        })
        .collect();

    for s in sheets.values() {
        if s.strut == ledger::STRUT {
            // The function ledger has its own checks (its rows are statuses, never unfinished cells).
            let (rows, _) = ledger::rows(s);
            let cites = ledger::scan_tree(root, &ledger::index(&rows));
            r.errors.extend(ledger::check(s, sheets.get("systems"), Some(&cites)));
            r.ledger = Some(ledger::summary(s));
            continue;
        }
        // Shape.
        if s.columns.len() != s.types.len() {
            r.errors.push(format!("{}: {} columns but {} types", s.name, s.columns.len(), s.types.len()));
        }
        if s.col(&s.key).is_none() {
            r.errors.push(format!("{}: key column '{}' missing", s.name, s.key));
            continue;
        }
        if !strut::KNOWN.contains(&s.strut.as_str()) {
            r.errors.push(format!("{}: unknown strut '{}'", s.name, s.strut));
        }
        let mut seen = BTreeSet::new();
        let mut done = 0;
        for row in &s.rows {
            let k = s.key_of(row);
            if row.len() != s.columns.len() {
                r.errors.push(format!("{}[{k}]: {} cells, expected {}", s.name, row.len(), s.columns.len()));
                continue;
            }
            if !seen.insert(k) {
                r.errors.push(format!("{}[{k}]: duplicate key", s.name));
            }
            let miss = s.unchecked(row);
            if miss.is_empty() {
                done += 1;
            } else {
                r.unfinished.push((s.name.clone(), k.into(), miss.iter().map(|c| c.to_string()).collect()));
            }
            // Overlap: typed cells against other sheets and the workspace.
            for (c, t) in s.columns.iter().zip(&s.types) {
                let v = s.get(row, c);
                if v.is_null() {
                    continue;
                }
                if let Some(target) = t.strip_prefix("refs:").or_else(|| t.strip_prefix("ref:")) {
                    let vals: Vec<&str> = match v {
                        Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
                        Value::String(x) => x.split_whitespace().collect(),
                        _ => vec![],
                    };
                    for x in vals {
                        match keys.get(target) {
                            None => r.errors.push(format!("{}.{c}: target sheet '{target}' does not exist", s.name)),
                            Some(ks) if !ks.contains(x) => {
                                r.errors.push(format!("{}[{k}].{c}: '{x}' is not a row of {target}", s.name))
                            }
                            _ if unfinished_rows.contains(&(target.to_string(), x.to_string())) => {
                                r.gaps.push(format!("{}[{k}] -> {target}[{x}] is unfinished", s.name))
                            }
                            _ => {}
                        }
                    }
                } else if t == "crate" {
                    let name = v.as_str().unwrap_or_default();
                    if !root.join("crates").join(name).join("Cargo.toml").exists() {
                        r.gaps.push(format!("{}[{k}]: crate '{name}' not created yet", s.name));
                    }
                } else if t == "layout"
                    && let Err(e) = strut::parse_layout(v.as_str().unwrap_or_default())
                {
                    r.errors.push(format!("{}[{k}].{c}: {e}", s.name));
                }
            }
            for c in s.columns.iter().filter(|c| c.to_ascii_lowercase().starts_with("unk")) {
                r.warnings.push(format!("{}.{c}: column meaning unidentified", s.name));
            }
        }
        if let Some(layout) = s.col("layout") {
            for row in &s.rows {
                if let Some(l) = row.get(layout).and_then(Value::as_str) {
                    for f in l.split_whitespace().filter(|f| f.to_ascii_lowercase().starts_with("unk")) {
                        r.warnings.push(format!("{}[{}]: field {f} unidentified", s.name, s.key_of(row)));
                    }
                }
            }
        }
        r.grid.insert(s.name.clone(), (s.rows.len(), done));
    }
    // Every strut must render (dry run).
    if let Err(e) = strut::render_all(&sheets) {
        r.errors.push(e);
    }
    r
}
