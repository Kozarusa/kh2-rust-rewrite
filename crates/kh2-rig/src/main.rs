//! `kh2-rig`: run a scenario on the original game in PCSX2 and write its trace.
//!
//! ```text
//! kh2-rig check <scenario.toml>                 parse and validate (no emulator needed)
//! kh2-rig run <scenario.toml> [options]         run on the original; writes <out>/trace-orig.csv and meta.txt
//! kh2-rig determinism <scenario.toml> [options] run twice and compare the traces (and with --ram the whole EE RAM)
//! kh2-rig diff <a.csv> <b.csv>                  compare two traces
//! options: --unlimited  --out DIR  --id NAME (lock owner)  --attach load|bp  --ram  --lock-wait SECONDS
//! ```
//! Needs `KH2_ISO` and the patched PCSX2-MCP under `local/tools/pcsx2-mcp` (see `client`).

use kh2_rig::client::{self, Attach, Config, Options};
use kh2_rig::trace::Trace;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const USAGE: &str = "usage: kh2-rig check|run|determinism <scenario.toml> [--unlimited] [--out DIR] [--id NAME] [--attach load|bp] [--ram] [--lock-wait SECONDS]\n       kh2-rig diff <a.csv> <b.csv>";

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
    let cfg = Config::from_env(value(args, "--id"))?;
    let attach = match value(args, "--attach") {
        None | Some("load") => Attach::LoadFirst,
        Some("bp") => Attach::BreakFirst,
        Some(o) => return Err(format!("--attach {o}: expected load or bp")),
    };
    let opt = Options {
        unlimited: flag(args, "--unlimited"),
        attach,
        ram_hash: flag(args, "--ram"),
        out: value(args, "--out").map(PathBuf::from),
        lock_wait: Duration::from_secs(value(args, "--lock-wait").and_then(|s| s.parse().ok()).unwrap_or(1800)),
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
