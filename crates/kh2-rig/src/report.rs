//! Original versus port: joins the two traces of a scenario by `(k, name)`, applies the tolerances of the scenario's `[[check]]` tables
//! and prints the pass/fail table (`values.csv`) and the plain-text report (`kh2-rig compare`, called by `tools/rig.ps1`).
//!
//! * A value is the items joined by `;`. When both sides parse as numbers the difference is the largest absolute difference of an
//!   item; a check without a tolerance for the name is exact (difference 0). Any other value is compared as text.
//! * A row is `Pass`, `Fail` or `Info` (the check is `informational`: shown, never counted). A row where either side has no value
//!   (no row, or an empty value: a null pointer in the original, a value the port lacks) fails unless both are empty.
//! * Scenarios whose trace holds `menu.scene_state` for every stop also get the menu open tick: the first k where it is 1 on each
//!   side, compared exactly (the `Start` menu open tick of the port against the original's).

use crate::scenario::Scenario;
use crate::trace::Trace;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    Info,
}

impl Status {
    pub fn word(self) -> &'static str {
        match self {
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
            Status::Info => "INFO",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub k: u32,
    pub name: String,
    pub orig: Option<String>,
    pub port: Option<String>,
    /// The largest item difference; None for a text comparison or a missing side.
    pub diff: Option<f64>,
    pub tol: f64,
    pub status: Status,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    pub scenario: String,
    pub rows: Vec<Row>,
}

fn numbers(v: &str) -> Option<Vec<f64>> {
    if v.is_empty() {
        return None;
    }
    v.split(';').map(|x| x.parse::<f64>().ok()).collect()
}

fn judge(orig: Option<&str>, port: Option<&str>, tol: f64) -> (Option<f64>, bool) {
    match (orig, port) {
        (Some(o), Some(p)) if o.is_empty() && p.is_empty() => (None, true),
        (Some(o), Some(p)) => match (numbers(o), numbers(p)) {
            (Some(a), Some(b)) if a.len() == b.len() => {
                let d = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max);
                (Some(d), d <= tol)
            }
            _ => (None, o == p),
        },
        _ => (None, false),
    }
}

/// The first k at which `menu.scene_state` is 1 in `t`.
pub fn open_tick(t: &Trace) -> Option<u32> {
    t.rows.iter().filter(|r| r.name == "menu.scene_state" && r.value == "1").map(|r| r.k).min()
}

pub fn compare(sc: &Scenario, orig: &Trace, port: &Trace) -> Report {
    let find = |t: &Trace| -> BTreeMap<(u32, String), String> { t.rows.iter().map(|r| ((r.k, r.name.clone()), r.value.clone())).collect() };
    let (o, p) = (find(orig), find(port));
    let mut rows = Vec::new();
    for c in &sc.checks {
        for &k in &c.at {
            for name in &c.values {
                let key = (k, name.clone());
                let (ov, pv) = (o.get(&key), p.get(&key));
                let tol = c.tol.get(name).copied().unwrap_or(0.0);
                let (diff, ok) = judge(ov.map(String::as_str), pv.map(String::as_str), tol);
                let status = if c.informational { Status::Info } else if ok { Status::Pass } else { Status::Fail };
                rows.push(Row { k, name: name.clone(), orig: ov.cloned(), port: pv.cloned(), diff, tol, status });
            }
        }
    }
    let (ot, pt) = (open_tick(orig), open_tick(port));
    if ot.is_some() || pt.is_some() {
        let show = |t: Option<u32>| t.map(|v| v.to_string());
        let diff = ot.zip(pt).map(|(a, b)| f64::from(a.abs_diff(b)));
        let status = if ot.is_some() && ot == pt { Status::Pass } else { Status::Fail };
        rows.push(Row { k: ot.or(pt).unwrap_or(0), name: "menu.open_tick".into(), orig: show(ot), port: show(pt), diff, tol: 0.0, status });
    }
    Report { scenario: sc.name.clone(), rows }
}

impl Report {
    pub fn counted(&self) -> (usize, usize) {
        let counted: Vec<&Row> = self.rows.iter().filter(|r| r.status != Status::Info).collect();
        (counted.iter().filter(|r| r.status == Status::Pass).count(), counted.len())
    }

    pub fn passed(&self) -> bool {
        let (p, n) = self.counted();
        p == n
    }

    /// `scenario,k,name,original,port,diff,tol,status` rows.
    pub fn csv(&self) -> String {
        let mut s = String::from("scenario,k,name,original,port,diff,tol,status\n");
        for r in &self.rows {
            let d = r.diff.map_or(String::new(), |d| format!("{d}"));
            s.push_str(&format!("{},{},{},{},{},{d},{},{}\n", self.scenario, r.k, r.name, r.orig.as_deref().unwrap_or(""), r.port.as_deref().unwrap_or(""), r.tol, r.status.word()));
        }
        s
    }

    /// The plain-text report: a line per row, the first failing k per name, and the last line `RIG <scenario> PASS|FAIL n/m`.
    pub fn text(&self) -> String {
        let mut s = format!("rig scenario {}\n\n", self.scenario);
        for r in &self.rows {
            let d = r.diff.map_or(String::new(), |d| format!(" diff {d:.4} tol {}", r.tol));
            s.push_str(&format!("{} k {} {}: original {} port {}{d}\n", r.status.word(), r.k, r.name, r.orig.as_deref().map_or("(none)", |v| if v.is_empty() { "(empty)" } else { v }), r.port.as_deref().map_or("(none)", |v| if v.is_empty() { "(empty)" } else { v })));
        }
        let mut first: BTreeMap<&str, u32> = BTreeMap::new();
        for r in self.rows.iter().filter(|r| r.status == Status::Fail) {
            let e = first.entry(r.name.as_str()).or_insert(r.k);
            *e = (*e).min(r.k);
        }
        s.push_str("\nfirst failing k per value:");
        if first.is_empty() {
            s.push_str(" none");
        }
        for (n, k) in &first {
            s.push_str(&format!(" {n}@{k}"));
        }
        let (p, n) = self.counted();
        s.push_str(&format!("\n\nRIG {} {} {p}/{n}\n", self.scenario, if self.passed() { "PASS" } else { "FAIL" }));
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn scenario() -> Scenario {
        let text = r#"
[scenario]
name = "t"
state = "06"
frames = 10
[[check]]
at = [1, 2]
values = ["sora.pos", "menu.page"]
tol = { "sora.pos" = 2.0 }
[[check]]
at = [1]
values = ["frame.step"]
informational = true
"#;
        Scenario::parse(text, Path::new(".")).unwrap()
    }

    fn trace(rows: &[(u32, &str, &str)]) -> Trace {
        let mut t = Trace::default();
        for (k, n, v) in rows {
            t.push(*k, n, Some((*v).to_string()), *k);
        }
        t
    }

    #[test]
    fn tolerance_exactness_missing_values_and_info_rows() {
        let orig = trace(&[(1, "sora.pos", "100;0;50"), (2, "sora.pos", "100;0;50"), (1, "menu.page", "1"), (2, "menu.page", "1"), (1, "frame.step", "2")]);
        let port = trace(&[(1, "sora.pos", "101.5;0;50"), (2, "sora.pos", "103;0;50"), (1, "menu.page", "1"), (1, "frame.step", "1")]);
        let r = compare(&scenario(), &orig, &port);
        let st = |k: u32, n: &str| r.rows.iter().find(|x| x.k == k && x.name == n).unwrap();
        assert_eq!(st(1, "sora.pos").status, Status::Pass, "1.5 cm is within 2");
        assert_eq!(st(2, "sora.pos").status, Status::Fail, "3 cm is not");
        assert_eq!(st(2, "sora.pos").diff, Some(3.0));
        assert_eq!(st(1, "menu.page").status, Status::Pass, "no tolerance: exact, equal");
        assert_eq!(st(2, "menu.page").status, Status::Fail, "the port has no row");
        assert_eq!(st(1, "frame.step").status, Status::Info, "informational rows are never counted");
        assert_eq!(r.counted(), (2, 4));
        assert!(!r.passed());
        let text = r.text();
        assert!(text.contains("FAIL k 2 sora.pos: original 100;0;50 port 103;0;50 diff 3.0000 tol 2"), "{text}");
        assert!(text.contains("first failing k per value: menu.page@2 sora.pos@2"), "{text}");
        assert!(text.ends_with("RIG t FAIL 2/4\n"), "{text}");
        assert!(r.csv().contains("t,2,sora.pos,100;0;50,103;0;50,3,2,FAIL\n"));
    }

    #[test]
    fn both_empty_passes_and_an_empty_side_fails_and_the_open_tick_is_compared() {
        assert_eq!(judge(Some(""), Some(""), 0.0), (None, true), "both are a null pointer");
        assert_eq!(judge(Some("1"), Some(""), 0.0).1, false);
        assert_eq!(judge(Some("abc"), Some("abc"), 0.0).1, true);
        let orig = trace(&[(202, "menu.scene_state", "0"), (203, "menu.scene_state", "1"), (204, "menu.scene_state", "1")]);
        let port = trace(&[(203, "menu.scene_state", "0"), (205, "menu.scene_state", "1")]);
        assert_eq!((open_tick(&orig), open_tick(&port)), (Some(203), Some(205)));
        let sc = Scenario::parse("[scenario]\nname = \"m\"\nstate = \"06\"\nframes = 300\n", Path::new(".")).unwrap();
        let r = compare(&sc, &orig, &port);
        let row = r.rows.last().unwrap();
        assert_eq!((row.name.as_str(), row.status, row.diff), ("menu.open_tick", Status::Fail, Some(2.0)));
        assert_eq!(compare(&sc, &orig, &orig).rows.last().unwrap().status, Status::Pass);
    }
}
