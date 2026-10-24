//! `kh2-rig`: run a scenario on the original game in PCSX2 and write its trace.
//!
//! ```text
//! kh2-rig check <scenario.toml>                 parse and validate (no emulator needed)
//! kh2-rig run <scenario.toml> [options]         run on the original; writes <out>/trace-orig.csv and meta.txt
//! kh2-rig determinism <scenario.toml> [options] run twice and compare the traces (and with --ram the whole EE RAM)
//! kh2-rig diff <a.csv> <b.csv>                  compare two traces
//! kh2-rig port-state <scenario.toml> <out.json>   the scenario's state as one KH2_STATE file for the port (prints the .p2s path)
//! kh2-rig plan <scenario.toml>                  key=value lines for tools/rig.ps1 (area, pad, frames, seeds, trace names and checkpoints)
//! kh2-rig compare <scenario.toml> <orig.csv> <port.csv> [--out DIR]   pass/fail table by the scenario's checks (values.csv, report.txt)
//! options: --unlimited  --out DIR  --id NAME (lock owner)  --attach load|bp  --ram  --lock-wait SECONDS  --no-shots (skip [[shot]] captures)
//! ```
//! Needs `KH2_ISO` and the patched PCSX2-MCP under `local/tools/pcsx2-mcp` (see `client`).

use kh2_rig::client::{self, Attach, Config, Options};
use kh2_rig::trace::Trace;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "usage: kh2-rig check|run|determinism <scenario.toml> [--unlimited] [--out DIR] [--id NAME] [--attach bp|load] [--ram] [--lock-wait SECONDS] [--no-shots]\n       kh2-rig diff <a.csv> <b.csv>
       kh2-rig compare <scenario.toml> <orig.csv> <port.csv> [--out DIR]
       kh2-rig frames <scenario.toml> <run-dir>
       kh2-rig port-state <scenario.toml> <out.json>
       kh2-rig plan <scenario.toml>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn run(args: &[String]) -> Result<bool, String> {
    let cmd = args.first().ok_or(USAGE)?.as_str();
    if cmd == "diff" {
        let load = |i: usize| -> Result<Trace, String> {
            let p = args.get(i).ok_or(USAGE)?;
            Trace::parse(&std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?)
        };
        let d = load(1)?.diff(&load(2)?);
        d.iter().for_each(|l| println!("{l}"));
        println!("{} differences", d.len());
        return Ok(d.is_empty());
    }
    if cmd == "compare" {
        let load = |i: usize| -> Result<Trace, String> {
            let p = args.get(i).ok_or(USAGE)?;
            Trace::parse(&std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?).map_err(|e| format!("{p}: {e}"))
        };
        let (sc, _, _) = client::load_scenario(&PathBuf::from(args.get(1).ok_or(USAGE)?))?;
        let report = kh2_rig::report::compare(&sc, &load(2)?, &load(3)?);
        if let Some(dir) = value(args, "--out").map(PathBuf::from) {
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            std::fs::write(dir.join("values.csv"), report.csv()).map_err(|e| e.to_string())?;
            std::fs::write(dir.join("report.txt"), report.text()).map_err(|e| e.to_string())?;
        }
        print!("{}", report.text());
        return Ok(report.passed());
    }
    if cmd == "frames" {
        let (sc, _, _) = client::load_scenario(&PathBuf::from(args.get(1).ok_or(USAGE)?))?;
        if sc.shots.is_empty() {
            return Err(format!("{}: the scenario has no [[shot]]", sc.name));
        }
        let dir = PathBuf::from(args.get(2).ok_or(USAGE)?);
        let report = kh2_rig::frames::compare_dir(&sc, &dir)?;
        std::fs::write(dir.join("frames.csv"), report.csv()).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("frames.txt"), report.text()).map_err(|e| e.to_string())?;
        print!("{}", report.text());
        return Ok(report.passed());
    }
    if cmd == "port-state" {
        let (sc, _, _) = client::load_scenario(&PathBuf::from(args.get(1).ok_or(USAGE)?))?;
        let cfg = Config::from_env(None)?;
        let state = cfg.resolve_state(&sc.state)?;
        let ee = kh2_rig::p2s::P2s::open(&state)?.ee;
        let name = state.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let out = PathBuf::from(args.get(2).ok_or(USAGE)?);
        std::fs::write(&out, kh2_rig::save::merged_json(&ee, &name, None)?).map_err(|e| format!("{}: {e}", out.display()))?;
        println!("{}", state.display());
        return Ok(true);
    }
    let path = PathBuf::from(args.get(1).ok_or(USAGE)?);
    let (sc, values, pad) = client::load_scenario(&path)?;
    if let Some(p) = &pad {
        let odd = p.odd_changes();
        if !odd.is_empty() {
            eprintln!("warning: the pad script changes at odd k {odd:?}; the field reads the pad every second vsync");
        }
    }
    if cmd == "check" {
        println!("{}: ok ({} checks, {} values, {} frames)", sc.name, sc.checks.len(), sc.value_names().len(), sc.frames);
        return Ok(true);
    }
    if cmd == "plan" {
        // The checkpoints the port records: every k when the scenario traces every stop, else the union of the checks' k.
        let at = if sc.every > 0 {
            "every".to_string()
        } else {
            let mut ks: Vec<u32> = sc.checks.iter().flat_map(|c| c.at.iter().copied()).collect();
            ks.sort_unstable();
            ks.dedup();
            ks.iter().map(u32::to_string).collect::<Vec<_>>().join(",")
        };
        println!("name={}
area={}
pad={}
frames={}
state={}
names={}
at={at}
shots={}
shot_names={}", sc.name, sc.port_area.as_deref().unwrap_or(""), sc.pad_path().map_or(String::new(), |p| p.display().to_string()), sc.frames, sc.state, sc.value_names().join(","), sc.shots.iter().map(|s| s.at.to_string()).collect::<Vec<_>>().join(","), sc.shots.iter().map(|s| s.name.clone()).collect::<Vec<_>>().join(","));
        for (k, v) in &sc.seeds {
            println!("seed.{k}={v}");
        }
        return Ok(true);
    }
    let cfg = Config::from_env(value(args, "--id"))?;
    let attach = match value(args, "--attach") {
        None | Some("bp") => Attach::BreakFirst,
        Some("load") => Attach::LoadFirst,
        Some(o) => return Err(format!("--attach {o}: expected load or bp")),
    };
    let opt = Options {
        unlimited: flag(args, "--unlimited"),
        attach,
        ram_hash: flag(args, "--ram"),
        out: value(args, "--out").map(PathBuf::from),
        lock_wait: Duration::from_secs(value(args, "--lock-wait").and_then(|s| s.parse().ok()).unwrap_or(1800)),
        no_shots: flag(args, "--no-shots"),
    };
    let out = opt.out.clone().unwrap_or_else(|| cfg.local.join("rig").join(&sc.name));
    match cmd {
        "run" => {
            let o = client::run_original(&cfg, &sc, &values, pad.as_ref(), &opt)?;
            client::write_outcome(&out, &sc, &o)?;
            println!("{}: {} rows, {} pad-1 stops (k {}..{}) in {:.1} s ({:.0} stops/s); wrote {}", sc.name, o.trace.rows.len(), o.stops, o.first_k, o.last_k, o.seconds, f64::from(o.stops) / o.seconds.max(1e-9), out.join("trace-orig.csv").display());
            Ok(true)
        }
        "determinism" => {
            let d = client::determinism(&cfg, &sc, &values, pad.as_ref(), &opt)?;
            d.iter().for_each(|l| println!("{l}"));
            println!("determinism {}: {}", sc.name, if d.is_empty() { "IDENTICAL".to_string() } else { format!("{} DIFFERENCES", d.len()) });
            Ok(d.is_empty())
        }
        _ => Err(USAGE.into()),
    }
}
