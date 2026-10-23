//! The trace file both sides of the rig write: `k,name,value,stop` rows.
//!
//! * `k`: the checkpoint (vsyncs since the state); for a value read by `trace.every` it is the stop's own k.
//! * `name`: a name from `scenarios/values.toml`.
//! * `value`: the items of the value joined by `;` (empty when a pointer on its path was null).
//! * `stop`: the k of the pad stop the read happened at. It equals `k` when the original had a stop at the checkpoint; in the field
//!   (a stop every second vsync) a checkpoint at an odd k is read at the next stop, so `stop` is `k + 1`.
//!
//! The port writes the same format (RIG-3), so a comparison is a join on `(k, name)`.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub k: u32,
    pub name: String,
    pub value: String,
    pub stop: u32,
}

pub const HEADER: &str = "k,name,value,stop";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace {
    pub rows: Vec<Row>,
}

impl Trace {
    pub fn push(&mut self, k: u32, name: &str, value: Option<String>, stop: u32) {
        self.rows.push(Row { k, name: name.to_string(), value: value.unwrap_or_default(), stop });
    }

    pub fn to_csv(&self) -> String {
        let mut s = format!("{HEADER}\n");
        for r in &self.rows {
            s.push_str(&format!("{},{},{},{}\n", r.k, r.name, r.value, r.stop));
        }
        s
    }

    pub fn parse(text: &str) -> Result<Trace, String> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(HEADER) {
            return Err(format!("trace: first line is not '{HEADER}'"));
        }
        let mut t = Trace::default();
        for (n, l) in lines.enumerate() {
            if l.trim().is_empty() {
                continue;
            }
            let c: Vec<&str> = l.split(',').collect();
            let num = |s: &str| s.parse::<u32>().map_err(|_| format!("trace line {}: '{s}' is not a number", n + 2));
            if c.len() != 4 {
                return Err(format!("trace line {}: expected 4 columns", n + 2));
            }
            t.rows.push(Row { k: num(c[0])?, name: c[1].to_string(), value: c[2].to_string(), stop: num(c[3])? });
        }
        Ok(t)
    }

    /// The value of `(k, name)`, if the trace has it.
    pub fn get(&self, k: u32, name: &str) -> Option<&Row> {
        self.rows.iter().find(|r| r.k == k && r.name == name)
    }

    /// Rows of `self` that differ from `other` (missing in `other`, or another value), as text lines. Empty = identical.
    pub fn diff(&self, other: &Trace) -> Vec<String> {
        let theirs: BTreeMap<(u32, &str), &str> = other.rows.iter().map(|r| ((r.k, r.name.as_str()), r.value.as_str())).collect();
        let mut out = Vec::new();
        for r in &self.rows {
            match theirs.get(&(r.k, r.name.as_str())) {
                None => out.push(format!("k {} {}: missing in the other trace", r.k, r.name)),
                Some(v) if *v != r.value => out.push(format!("k {} {}: {} vs {v}", r.k, r.name, r.value)),
                _ => {}
            }
        }
        let mine: BTreeMap<(u32, &str), ()> = self.rows.iter().map(|r| ((r.k, r.name.as_str()), ())).collect();
        for r in &other.rows {
            if !mine.contains_key(&(r.k, r.name.as_str())) {
                out.push(format!("k {} {}: missing in this trace", r.k, r.name));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Trace {
        let mut t = Trace::default();
        t.push(40, "sora.pos", Some("650;0;250".into()), 40);
        t.push(41, "sora.pos", Some("636.54;0;241.34".into()), 42);
        t.push(42, "cam.pos", None, 42);
        t
    }

    #[test]
    fn csv_round_trips() {
        let t = sample();
        let csv = t.to_csv();
        assert!(csv.starts_with("k,name,value,stop\n40,sora.pos,650;0;250,40\n"));
        assert_eq!(Trace::parse(&csv).unwrap(), t);
        assert_eq!(t.get(42, "cam.pos").unwrap().value, "");
        assert_eq!(t.get(41, "sora.pos").unwrap().stop, 42);
    }

    #[test]
    fn diff_lists_changed_and_missing_rows() {
        let a = sample();
        assert!(a.diff(&a).is_empty());
        let mut b = sample();
        b.rows[0].value = "651;0;250".into();
        b.rows.pop();
        b.push(50, "x", Some("1".into()), 50);
        let d = a.diff(&b);
        assert_eq!(d.len(), 3, "{d:?}");
        assert!(d[0].contains("650;0;250 vs 651;0;250"));
        assert!(d.iter().any(|l| l.contains("cam.pos") && l.contains("missing in the other")));
        assert!(d.iter().any(|l| l.contains("k 50 x") && l.contains("missing in this")));
    }

    #[test]
    fn a_bad_file_is_rejected() {
        assert!(Trace::parse("nope\n").is_err());
        assert!(Trace::parse("k,name,value,stop\n1,a,b\n").unwrap_err().contains("4 columns"));
        assert!(Trace::parse("k,name,value,stop\nx,a,b,1\n").unwrap_err().contains("not a number"));
    }
}
