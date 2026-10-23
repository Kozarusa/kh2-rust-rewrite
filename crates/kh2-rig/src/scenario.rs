//! Scenario files and the value list.
//!
//! # `scenarios/<system>/<name>.toml` (tracked; no game data)
//!
//! ```toml
//! [scenario]
//! name = "tt-run-06"                 # the run's name (folder under local/rig/)
//! state = "06"                       # a PCSX2 slot number ("06" = <serial>.06.p2s in the sstates folder), or a file name
//!                                    # in local/rig/states/ (never committed), or a path
//! port_area = "tt21"                 # the port's area name until the port reads the place from the state
//! pad = "tt-run-06.pad"              # the pad script (see `pad`), next to this file
//! frames = 330                       # the run ends at the first stop with k >= frames
//!
//! [seeds]                            # the port's RNG seeds (the original's state carries its own)
//! drive = 1
//! auto_limit = 1
//!
//! [trace]                            # optional: read these at every `every`-th k as well
//! every = 2
//! values = ["frame.step"]
//!
//! [[check]]                          # one per value group
//! at = [40, 42, 60]                  # checkpoints, in k
//! values = ["sora.pos", "sora.facing"]       # names from scenarios/values.toml
//! tol = { "sora.pos" = 2.0 }         # per name, in the value's unit; a missing name is compared exactly
//! informational = false              # true: reported, never fails the scenario
//! note = "free text"
//!
//! [[shot]]                           # screenshots (later, informational in v1)
//! at = 320
//! ```
//!
//! `k` is the vsync counter minus its value in the state (60 per second also in menus; the field runs one game frame every 2).
//! A checkpoint at an `at` where the original has no pad stop (an odd k in the field) is read at the next stop and
//! the trace row keeps both numbers (see `trace`).
//!
//! # `scenarios/values.toml`
//!
//! One table per name; a table that has `path` is a value, the name is its dotted table name:
//!
//! ```toml
//! [sora.pos]
//! path = [0x35c998, 0x70]    # the first is an absolute EE address; every further one is added to the pointer read at the one before
//! type = "f32"               # u8 i8 u16 i16 u32 i32 f32
//! count = 3                  # consecutive items (default 1); in the trace they are joined by ';'
//! unit = "cm"
//! note = "..."
//! ```
//!
//! A pointer that reads 0 (or points outside RAM) makes the value empty in the trace.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table, TableLike};

/// Item type of a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    F32,
}

impl Ty {
    pub fn parse(s: &str) -> Option<Ty> {
        Some(match s {
            "u8" => Ty::U8,
            "i8" => Ty::I8,
            "u16" => Ty::U16,
            "i16" => Ty::I16,
            "u32" => Ty::U32,
            "i32" => Ty::I32,
            "f32" => Ty::F32,
            _ => return None,
        })
    }

    pub fn size(self) -> usize {
        match self {
            Ty::U8 | Ty::I8 => 1,
            Ty::U16 | Ty::I16 => 2,
            Ty::U32 | Ty::I32 | Ty::F32 => 4,
        }
    }

    /// One item as text: integers in decimal, floats in the shortest form that reads back exactly.
    pub fn format(self, b: &[u8]) -> String {
        match self {
            Ty::U8 => b[0].to_string(),
            Ty::I8 => (b[0] as i8).to_string(),
            Ty::U16 => u16::from_le_bytes([b[0], b[1]]).to_string(),
            Ty::I16 => i16::from_le_bytes([b[0], b[1]]).to_string(),
            Ty::U32 => u32::from_le_bytes([b[0], b[1], b[2], b[3]]).to_string(),
            Ty::I32 => i32::from_le_bytes([b[0], b[1], b[2], b[3]]).to_string(),
            Ty::F32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]).to_string(),
        }
    }
}

/// One named value of the original's memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueDef {
    pub name: String,
    pub path: Vec<u32>,
    pub ty: Ty,
    pub count: usize,
    pub unit: String,
    pub note: String,
}

/// Reads `len` bytes at an EE address.
pub type ReadFn<'a> = &'a mut dyn FnMut(u32, usize) -> Result<Vec<u8>, String>;

/// Where pointers may point: EE RAM past the kernel area.
fn plausible(p: u32) -> bool {
    (0x0010_0000..0x0200_0000).contains(&p)
}

impl ValueDef {
    /// Reads the value: `None` when a pointer on the path is null or outside RAM, else the items joined by `;`.
    pub fn read(&self, mem: ReadFn) -> Result<Option<String>, String> {
        let mut at = self.path[0];
        for off in &self.path[1..] {
            let p = mem(at, 4)?;
            let p = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
            if !plausible(p) {
                return Ok(None);
            }
            at = p.wrapping_add(*off);
        }
        let bytes = mem(at, self.count * self.ty.size())?;
        Ok(Some(bytes.chunks_exact(self.ty.size()).map(|c| self.ty.format(c)).collect::<Vec<_>>().join(";")))
    }
}

/// The value list: names to definitions.
#[derive(Clone, Debug, Default)]
pub struct Values {
    pub defs: BTreeMap<String, ValueDef>,
}

fn int(item: &Item, what: &str) -> Result<i64, String> {
    item.as_integer().ok_or_else(|| format!("{what}: expected an integer"))
}

fn ints(item: &Item, what: &str) -> Result<Vec<i64>, String> {
    item.as_array().ok_or_else(|| format!("{what}: expected an array"))?.iter().map(|v| v.as_integer().ok_or_else(|| format!("{what}: expected integers"))).collect()
}

fn strs(item: &Item, what: &str) -> Result<Vec<String>, String> {
    item.as_array().ok_or_else(|| format!("{what}: expected an array"))?.iter().map(|v| v.as_str().map(str::to_string).ok_or_else(|| format!("{what}: expected strings"))).collect()
}

fn u32s(v: Vec<i64>, what: &str) -> Result<Vec<u32>, String> {
    v.into_iter().map(|n| u32::try_from(n).map_err(|_| format!("{what}: {n} does not fit 0..=u32"))).collect()
}

impl Values {
    pub fn parse(text: &str) -> Result<Values, String> {
        let doc: DocumentMut = text.parse().map_err(|e| format!("values: {e}"))?;
        let mut v = Values::default();
        walk(doc.as_table(), "", &mut v.defs)?;
        if v.defs.is_empty() {
            return Err("values: no entries".into());
        }
        Ok(v)
    }

    /// The `values.toml` that belongs to a scenario file: the first one in an ancestor directory.
    pub fn for_scenario(scenario: &Path) -> Result<Values, String> {
        let abs = std::fs::canonicalize(scenario).map_err(|e| format!("{}: {e}", scenario.display()))?;
        for dir in abs.ancestors().skip(1) {
            let p = dir.join("values.toml");
            if p.is_file() {
                return Values::parse(&std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?).map_err(|e| format!("{}: {e}", p.display()));
            }
        }
        Err(format!("no values.toml above {}", scenario.display()))
    }

    pub fn get(&self, name: &str) -> Result<&ValueDef, String> {
        self.defs.get(name).ok_or_else(|| format!("unknown value '{name}' (not in values.toml)"))
    }
}

fn walk(t: &dyn TableLike, prefix: &str, out: &mut BTreeMap<String, ValueDef>) -> Result<(), String> {
    for (k, item) in t.iter() {
        let Some(sub) = item.as_table_like() else { continue };
        let name = if prefix.is_empty() { k.to_string() } else { format!("{prefix}.{k}") };
        if let Some(path) = sub.get("path") {
            let path = u32s(ints(path, &format!("{name}.path"))?, &format!("{name}.path"))?;
            if path.is_empty() {
                return Err(format!("{name}: empty path"));
            }
            let ty = sub.get("type").and_then(Item::as_str).ok_or_else(|| format!("{name}: missing type"))?;
            let ty = Ty::parse(ty).ok_or_else(|| format!("{name}: unknown type '{ty}'"))?;
            let count = sub.get("count").map_or(Ok(1), |c| int(c, &format!("{name}.count")))?;
            if !(1..=64).contains(&count) {
                return Err(format!("{name}: count {count} is outside 1..=64"));
            }
            let text = |key: &str| sub.get(key).and_then(Item::as_str).unwrap_or("").to_string();
            out.insert(name.clone(), ValueDef { name, path, ty, count: count as usize, unit: text("unit"), note: text("note") });
        } else {
            walk(sub, &name, out)?;
        }
    }
    Ok(())
}

/// A group of checkpoints.
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub at: Vec<u32>,
    pub values: Vec<String>,
    pub tol: BTreeMap<String, f64>,
    pub informational: bool,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shot {
    pub at: u32,
}

/// A parsed scenario file.
#[derive(Clone, Debug, PartialEq)]
pub struct Scenario {
    pub name: String,
    pub state: String,
    pub port_area: Option<String>,
    pub pad: Option<String>,
    pub frames: u32,
    pub seeds: BTreeMap<String, i64>,
    pub every: u32,
    pub trace_values: Vec<String>,
    pub checks: Vec<Check>,
    pub shots: Vec<Shot>,
    /// The folder of the scenario file (the pad script is next to it).
    pub dir: PathBuf,
}

fn table<'a>(doc: &'a DocumentMut, name: &str) -> Option<&'a dyn TableLike> {
    doc.get(name).and_then(Item::as_table_like)
}

fn float(item: &Item, what: &str) -> Result<f64, String> {
    item.as_float().or_else(|| item.as_integer().map(|i| i as f64)).ok_or_else(|| format!("{what}: expected a number"))
}

impl Scenario {
    pub fn parse(text: &str, dir: &Path) -> Result<Scenario, String> {
        let doc: DocumentMut = text.parse().map_err(|e| format!("scenario: {e}"))?;
        let s = table(&doc, "scenario").ok_or("scenario: missing [scenario]")?;
        let req = |k: &str| s.get(k).and_then(Item::as_str).map(str::to_string).ok_or_else(|| format!("scenario: missing {k}"));
        let opt = |k: &str| s.get(k).and_then(Item::as_str).map(str::to_string);
        let frames = int(s.get("frames").ok_or("scenario: missing frames")?, "frames")?;
        let frames = u32::try_from(frames).map_err(|_| format!("scenario: frames {frames} is out of range"))?;
        let mut seeds = BTreeMap::new();
        if let Some(t) = table(&doc, "seeds") {
            for (k, v) in t.iter() {
                seeds.insert(k.to_string(), int(v, &format!("seeds.{k}"))?);
            }
        }
        let (mut every, mut trace_values) = (0, Vec::new());
        if let Some(t) = table(&doc, "trace") {
            if let Some(e) = t.get("every") {
                every = u32::try_from(int(e, "trace.every")?).map_err(|_| "trace.every is out of range".to_string())?;
            }
            if let Some(v) = t.get("values") {
                trace_values = strs(v, "trace.values")?;
            }
        }
        let mut checks = Vec::new();
        if let Some(arr) = doc.get("check").and_then(Item::as_array_of_tables) {
            for (i, c) in arr.iter().enumerate() {
                checks.push(parse_check(c, i)?);
            }
        }
        let mut shots = Vec::new();
        if let Some(arr) = doc.get("shot").and_then(Item::as_array_of_tables) {
            for c in arr.iter() {
                let at = int(c.get("at").ok_or("shot: missing at")?, "shot.at")?;
                shots.push(Shot { at: u32::try_from(at).map_err(|_| "shot.at is out of range".to_string())? });
            }
        }
        Ok(Scenario { name: req("name")?, state: req("state")?, port_area: opt("port_area"), pad: opt("pad"), frames, seeds, every, trace_values, checks, shots, dir: dir.to_path_buf() })
    }

    pub fn load(path: &Path) -> Result<Scenario, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Scenario::parse(&text, path.parent().unwrap_or(Path::new("."))).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Every value name the scenario reads.
    pub fn value_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.checks.iter().flat_map(|c| c.values.iter().cloned()).chain(self.trace_values.iter().cloned()).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Checks that every name exists in `values`, checkpoints lie within the run, and tolerances name checked values.
    pub fn validate(&self, values: &Values) -> Result<(), String> {
        for n in self.value_names() {
            values.get(&n)?;
        }
        for c in &self.checks {
            if let Some(k) = c.at.iter().find(|k| **k > self.frames) {
                return Err(format!("check at {k} is past frames = {}", self.frames));
            }
            if let Some(t) = c.tol.keys().find(|t| !c.values.contains(t)) {
                return Err(format!("tol names '{t}' which this check does not read"));
            }
        }
        if self.checks.is_empty() && self.every == 0 {
            return Err("the scenario reads nothing (no [[check]], no trace.every)".into());
        }
        Ok(())
    }

    /// The pad script file, resolved next to the scenario file.
    pub fn pad_path(&self) -> Option<PathBuf> {
        self.pad.as_ref().map(|p| self.dir.join(p))
    }
}

fn parse_check(c: &Table, i: usize) -> Result<Check, String> {
    let at_of = |k: &str| c.get(k).ok_or_else(|| format!("check #{}: missing {k}", i + 1));
    let at = u32s(ints(at_of("at")?, "check.at")?, "check.at")?;
    let values = strs(at_of("values")?, "check.values")?;
    let mut tol = BTreeMap::new();
    if let Some(t) = c.get("tol").and_then(Item::as_table_like) {
        for (k, v) in t.iter() {
            tol.insert(k.to_string(), float(v, &format!("tol.{k}"))?);
        }
    }
    Ok(Check { at, values, tol, informational: c.get("informational").and_then(Item::as_bool).unwrap_or(false), note: c.get("note").and_then(Item::as_str).unwrap_or("").to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALUES: &str = r#"
[sora.pos]
path = [0x35c998, 0x70]
type = "f32"
count = 3
unit = "cm"

[frame.step]
path = [0x356f40]
type = "f32"

[place.world]
path = [0x33e830]
type = "u8"
"#;

    fn mem(image: &[(u32, Vec<u8>)]) -> impl FnMut(u32, usize) -> Result<Vec<u8>, String> + '_ {
        move |a, n| {
            for (base, bytes) in image {
                if a >= *base && (a - base) as usize + n <= bytes.len() {
                    return Ok(bytes[(a - base) as usize..(a - base) as usize + n].to_vec());
                }
            }
            Err(format!("no memory at {a:#x}"))
        }
    }

    #[test]
    fn values_are_read_by_dotted_name_and_follow_pointers() {
        let v = Values::parse(VALUES).unwrap();
        assert_eq!(v.defs.keys().collect::<Vec<_>>(), ["frame.step", "place.world", "sora.pos"]);
        let pos = v.get("sora.pos").unwrap();
        assert_eq!((pos.path.clone(), pos.ty, pos.count, pos.unit.as_str()), (vec![0x35c998, 0x70], Ty::F32, 3, "cm"));
        let mut obj = vec![0u8; 0x80];
        for (i, f) in [650.5f32, 0.0, -250.0].iter().enumerate() {
            obj[0x70 + 4 * i..0x74 + 4 * i].copy_from_slice(&f.to_le_bytes());
        }
        let image = vec![(0x35c998, 0x0180_0000u32.to_le_bytes().to_vec()), (0x0180_0000, obj)];
        assert_eq!(pos.read(&mut mem(&image)).unwrap().as_deref(), Some("650.5;0;-250"));
        // A null pointer is an empty value, not an error.
        let null = vec![(0x35c998, vec![0; 4])];
        assert_eq!(pos.read(&mut mem(&null)).unwrap(), None);
        // An unreadable address is an error.
        assert!(pos.read(&mut mem(&[])).is_err());
        let w = v.get("place.world").unwrap();
        assert_eq!(w.read(&mut mem(&[(0x33e830, vec![2, 0x15, 99])])).unwrap().as_deref(), Some("2"));
    }

    #[test]
    fn integer_types_read_signed_and_unsigned() {
        assert_eq!(Ty::I8.format(&[0xff]), "-1");
        assert_eq!(Ty::U8.format(&[0xff]), "255");
        assert_eq!(Ty::I16.format(&[0xfe, 0xff]), "-2");
        assert_eq!(Ty::U32.format(&[0xff; 4]), "4294967295");
        assert_eq!(Ty::I32.format(&(-5i32).to_le_bytes()), "-5");
        assert_eq!(Ty::F32.format(&2.0f32.to_le_bytes()), "2");
    }

    #[test]
    fn bad_value_lists_are_rejected() {
        for (text, needle) in [("[a]\npath = []\ntype = \"u8\"", "empty path"), ("[a]\npath = [1]", "missing type"), ("[a]\npath = [1]\ntype = \"u7\"", "unknown type"), ("[a]\npath = [-1]\ntype = \"u8\"", "does not fit"), ("[a]\npath = [1]\ntype = \"u8\"\ncount = 0", "count"), ("x = 1", "no entries")] {
            let e = Values::parse(text).unwrap_err();
            assert!(e.contains(needle), "{text:?} gave {e}");
        }
    }

    const SCENARIO: &str = r#"
[scenario]
name = "demo"
state = "06"
port_area = "tt21"
pad = "demo.pad"
frames = 100

[seeds]
drive = 1

[trace]
every = 10
values = ["frame.step"]

[[check]]
at = [40, 42]
values = ["sora.pos"]
tol = { "sora.pos" = 2.0 }
note = "moves"

[[check]]
at = [60]
values = ["frame.step"]
informational = true

[[shot]]
at = 90
"#;

    #[test]
    fn a_scenario_parses_and_validates_against_the_values() {
        let s = Scenario::parse(SCENARIO, Path::new("scenarios/demo")).unwrap();
        assert_eq!((s.name.as_str(), s.state.as_str(), s.frames, s.every), ("demo", "06", 100, 10));
        assert_eq!(s.port_area.as_deref(), Some("tt21"));
        assert_eq!(s.seeds["drive"], 1);
        assert_eq!(s.checks.len(), 2);
        assert_eq!(s.checks[0].at, vec![40, 42]);
        assert_eq!(s.checks[0].tol["sora.pos"], 2.0);
        assert!(s.checks[1].informational && !s.checks[0].informational);
        assert_eq!(s.shots, vec![Shot { at: 90 }]);
        assert_eq!(s.value_names(), vec!["frame.step", "sora.pos"]);
        assert_eq!(s.pad_path().unwrap(), Path::new("scenarios/demo").join("demo.pad"));
        s.validate(&Values::parse(VALUES).unwrap()).unwrap();
    }

    #[test]
    fn validation_catches_unknown_names_late_checkpoints_and_stray_tolerances() {
        let v = Values::parse(VALUES).unwrap();
        let base = Scenario::parse(SCENARIO, Path::new(".")).unwrap();
        let mut s = base.clone();
        s.checks[0].values = vec!["nope".into()];
        assert!(s.validate(&v).unwrap_err().contains("unknown value 'nope'"));
        let mut s = base.clone();
        s.checks[0].at = vec![101];
        assert!(s.validate(&v).unwrap_err().contains("past frames"));
        let mut s = base.clone();
        s.checks[0].tol.insert("frame.step".into(), 1.0);
        assert!(s.validate(&v).unwrap_err().contains("does not read"));
        let mut s = base;
        s.checks.clear();
        s.every = 0;
        assert!(s.validate(&v).unwrap_err().contains("reads nothing"));
    }

    #[test]
    fn missing_scenario_keys_are_named() {
        for (text, needle) in [("[x]\na=1", "missing [scenario]"), ("[scenario]\nname=\"a\"\nstate=\"1\"", "missing frames"), ("[scenario]\nname=\"a\"\nframes=1", "missing state"), ("[scenario]\nstate=\"1\"\nframes=1", "missing name")] {
            let e = Scenario::parse(text, Path::new(".")).unwrap_err();
            assert!(e.contains(needle), "{text:?} gave {e}");
        }
    }
}
