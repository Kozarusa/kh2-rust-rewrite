//! The `kh2 state ...` command line (called by the `kh2` binary's `state` arm).

use crate::p2s::P2s;
use crate::save;
use std::path::{Path, PathBuf};

pub const USAGE: &str = "usage: kh2 state from-p2s <file.p2s> [out.json] [--with-rig-keys]
  Reads the EE memory of a PCSX2 save state and writes a KH2_STATE file the app loads (party block, munny, record bits,
  story flags, world map bytes). Without out.json the file goes to stdout. The keys the app does not read yet (place, Sora,
  the rig clock, the whole save block) go to <out>.rig.json, or into the same file with --with-rig-keys.";

/// `kh2 state <sub> ...`; `args` are the words after `state`.
pub fn state(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("from-p2s") => from_p2s(&args[1..]),
        _ => Err(USAGE.to_string()),
    }
}

fn from_p2s(args: &[String]) -> Result<(), String> {
    let merge = args.iter().any(|a| a == "--with-rig-keys");
    let paths: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let input = PathBuf::from(paths.first().ok_or(USAGE)?.as_str());
    let state = P2s::open(&input)?;
    let name = input.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    match paths.get(1) {
        Some(out) => {
            let out = Path::new(out.as_str());
            let (main, side) = if merge { (save::merged_json(&state.ee, &name, None)?, None) } else { (save::state_json(&state.ee, &name)?, Some(save::rig_json(&state.ee, &name, None)?)) };
            std::fs::write(out, main).map_err(|e| format!("{}: {e}", out.display()))?;
            println!("wrote {}", out.display());
            if let Some(side) = side {
                let side_path = out.with_extension("rig.json");
                std::fs::write(&side_path, side).map_err(|e| format!("{}: {e}", side_path.display()))?;
                println!("wrote {} (place, sora, rig clock, save block)", side_path.display());
            }
        }
        None => print!("{}", if merge { save::merged_json(&state.ee, &name, None)? } else { save::state_json(&state.ee, &name)? }),
    }
    let x = save::Extract::read(&state.ee)?;
    eprintln!("state {name}: world {} area {:#x} entrance {}, vsync {}{}", x.world, x.area, x.entrance, x.vsync, x.sora.map_or(", no player object".to_string(), |s| format!(", Sora at ({}, {}, {})", s.pos[0], s.pos[1], s.pos[2])));
    Ok(())
}
