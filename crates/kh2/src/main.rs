//! `kh2` - command line for kh2-rs. Everything it reads comes from the user's own disc at runtime;
//! anything it writes goes to `local/` (git-ignored), except `validate`, which writes checkmarks
//! (status text, never game data) into `sheets/`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kh2_archive::Archive;
use serde_json::Value;

const USAGE: &str = "\
kh2 <command> [--iso PATH]

  set-iso PATH          remember the disc image path (local/iso.txt)
  fetch-names           download OpenKH's KH2 file-name list (Apache-2.0) to local/kh2idx.txt
  info                  disc + archive summary
  ls [FILTER]           list archive files (names need fetch-names)
  extract NAME [DIR]    write one archive file to DIR (default local/extract/)
  bar NAME              list the subfiles of a BAR archive
  table NAME [LIMIT]    print a decoded gameplay table
  validate              decode every modelled table and write checkmarks into sheets/
  preflight [--strict]  overlap all sheets and print the checklist

ISO path: --iso, else $KH2_ISO, else local/iso.txt";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn local() -> PathBuf {
    let p = root().join("local");
    std::fs::create_dir_all(&p).ok();
    p
}

fn iso_path(args: &mut Vec<String>) -> Result<PathBuf, String> {
    if let Some(i) = args.iter().position(|a| a == "--iso") {
        let p = args.get(i + 1).cloned().ok_or("--iso needs a path")?;
        args.drain(i..i + 2);
        return Ok(p.into());
    }
    if let Ok(p) = std::env::var("KH2_ISO") {
        return Ok(p.into());
    }
    std::fs::read_to_string(local().join("iso.txt"))
        .map(|s| PathBuf::from(s.trim()))
        .map_err(|_| "no disc image configured: run `kh2 set-iso <path to your ISO>`".to_string())
}

fn open(args: &mut Vec<String>) -> Result<Archive, String> {
    let p = iso_path(args)?;
    let mut a = Archive::open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    if let Ok(names) = std::fs::read_to_string(local().join("kh2idx.txt")) {
        a.apply_names(names.lines()).map_err(|e| e.to_string())?;
    }
    Ok(a)
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    match run(&mut args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &mut Vec<String>) -> Result<(), String> {
    let cmd = if args.is_empty() { "help".to_string() } else { args.remove(0) };
    match cmd.as_str() {
        "set-iso" => {
            let p = PathBuf::from(args.first().ok_or("set-iso needs a path")?);
            Archive::open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            std::fs::write(local().join("iso.txt"), p.canonicalize().unwrap().display().to_string())
                .map_err(|e| e.to_string())?;
            println!("ok: {}", p.display());
        }
        "fetch-names" => {
            let dst = local().join("kh2idx.txt");
            let url = "https://raw.githubusercontent.com/OpenKH/OpenKh/master/OpenKh.Kh2/resources/kh2idx.txt";
            let st = std::process::Command::new("curl")
                .args(["-fsSL", "-o"])
                .arg(&dst)
                .arg(url)
                .status()
                .map_err(|e| format!("curl: {e}"))?;
            if !st.success() {
                return Err(format!("download failed: {url}"));
            }
            println!("saved {} (OpenKH, Apache-2.0; not committed)", dst.display());
        }
        "info" => {
            let mut a = open(args)?;
            let named = a.entries().iter().filter(|e| e.name.is_some()).count();
            let disc = a.iso().entries().len();
            println!("disc: {} files, boot {}", disc, kh2_archive::SUPPORTED_BOOT);
            println!("archive: {} entries ({} named), {} overlays", a.entries().len(), named, a.overlays().len());
            if named == 0 {
                println!("hint: `kh2 fetch-names` to name entries");
            }
        }
        "ls" => {
            let a = open(args)?;
            let filter = args.first().map(|s| s.to_lowercase()).unwrap_or_default();
            for l in a.entries() {
                let name = l.name.clone().unwrap_or_else(|| format!("<{:08x}{:04x}>", l.entry.hash32, l.entry.hash16));
                if name.to_lowercase().contains(&filter) {
                    println!("{name:48} {:>10} {}", l.entry.length, l.parent);
                }
            }
        }
        "extract" => {
            let mut a = open(args)?;
            let name = args.first().ok_or("extract needs a file name")?.clone();
            let dir = args.get(1).map(PathBuf::from).unwrap_or_else(|| local().join("extract"));
            let data = a.read(&name).map_err(|e| e.to_string())?;
            let dst = dir.join(&name);
            std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(&dst, &data).map_err(|e| e.to_string())?;
            println!("{} ({} bytes)", dst.display(), data.len());
        }
        "bar" => {
            let mut a = open(args)?;
            let name = args.first().ok_or("bar needs a file name")?.clone();
            let data = a.read(&name).map_err(|e| e.to_string())?;
            for e in kh2_bar::parse(&data).map_err(|e| format!("{e:?}"))? {
                println!("{:4} type={:3} {:>8} bytes", e.tag, e.kind, e.data.len());
            }
        }
        "table" => {
            let mut a = open(args)?;
            let name = args.first().ok_or("table needs a name")?.clone();
            let limit: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
            let t = decode(&mut a, &name)?;
            println!("{name}: version {} rows {} (bytes {}/{})", t.version, t.rows.len(), t.used, t.len);
            println!("{}", t.fields.join("\t"));
            for r in t.rows.iter().take(limit) {
                println!("{}", r.join("\t"));
            }
        }
        "model" => {
            let mut a = open(args)?;
            let name = args.first().ok_or("model needs a file name")?.clone();
            let data = a.read(&name).map_err(|e| e.to_string())?;
            let m = if name.ends_with(".map") {
                kh2_model::map_file(&data).map(|f| f.map)
            } else {
                kh2_model::mdlx(&data)
            }
            .ok_or("not a supported model")?;
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            for p in m.meshes.iter().flat_map(|x| &x.positions) {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            let tris: usize = m.meshes.iter().map(|x| x.indices.len() / 3).sum();
            println!("{name}: {} meshes, {tris} triangles, {} bones, {} textures", m.meshes.len(), m.bones.len(), m.textures.len());
            println!("bounds {lo:?} .. {hi:?}");
            if args.iter().any(|x| x == "--textures") {
                let dir = local().join("tex").join(name.replace('/', "_"));
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                for (i, t) in m.textures.iter().enumerate() {
                    let f = std::fs::File::create(dir.join(format!("{i:03}.png"))).map_err(|e| e.to_string())?;
                    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), t.width, t.height);
                    enc.set_color(png::ColorType::Rgba);
                    enc.set_depth(png::BitDepth::Eight);
                    enc.write_header().and_then(|mut w| w.write_image_data(&t.rgba)).map_err(|e| e.to_string())?;
                }
                println!("textures -> {}", dir.display());
            }
        }
        "validate" => validate(args)?,
        "preflight" => {
            let r = kh2_sheets::preflight(&root());
            print!("{}", r.render(40));
            if !r.ok(args.iter().any(|a| a == "--strict")) {
                return Err("preflight failed".into());
            }
        }
        _ => println!("{USAGE}"),
    }
    Ok(())
}

fn decode(a: &mut Archive, name: &str) -> Result<kh2_tables::DynTable, String> {
    let def = kh2_tables::tables::ALL.iter().find(|d| d.name == name).ok_or(format!("unknown table {name}"))?;
    let blob = kh2_tables::subfile(a, def).map_err(|e| e.to_string())?;
    kh2_tables::tables::decode_dyn(name, &blob)
        .ok_or_else(|| format!("{name}: no layout in sheets/tables.json yet"))?
        .map_err(|e| e.to_string())
}

/// Decodes every modelled table from the disc and records the result as the row's `validated`
/// checkmark. Also checks the file-system systems. Only status text is written, never values.
fn validate(args: &mut Vec<String>) -> Result<(), String> {
    let mut a = open(args)?;
    let root = root();
    let mut tables = kh2_sheets::load(&root.join("sheets/tables.json"))?;
    let vcol = tables.col("validated").ok_or("tables.json has no validated column")?;
    let mut pass = 0;
    for row in tables.rows.iter_mut() {
        let name = row[0].as_str().unwrap_or_default().to_string();
        let def = kh2_tables::tables::ALL.iter().find(|d| d.name == name);
        let result = match def.and_then(|d| d.record_size.map(|_| d)) {
            None => Value::Null,
            Some(_) => match decode(&mut a, &name) {
                Ok(t) => {
                    pass += 1;
                    let extra = if t.used < t.len { format!(", {} trailing bytes", t.len - t.used) } else { String::new() };
                    Value::String(format!("v{} {} rows, size ok{extra}", t.version, t.rows.len()))
                }
                Err(e) => {
                    println!("FAIL {name}: {e}");
                    Value::Null
                }
            },
        };
        println!("{name:10} {}", result.as_str().unwrap_or("-"));
        row[vcol] = result;
    }
    tables.save().map_err(|e| e.to_string())?;
    let validated: std::collections::BTreeSet<String> = tables
        .rows
        .iter()
        .filter(|r| !r[vcol].is_null())
        .map(|r| r[0].as_str().unwrap_or_default().to_string())
        .collect();

    let mut systems = kh2_sheets::load(&root.join("sheets/systems.json"))?;
    let (icol, vcol) = (systems.col("implemented").unwrap(), systems.col("validated").unwrap());
    let named = a.entries().iter().filter(|e| e.name.is_some()).count();
    let overlays_ok = (0..a.overlays().len()).all(|k| a.read_overlay(k).is_ok());
    let battle = a.read("00battle.bin").map_err(|e| e.to_string())?;
    let bar_n = kh2_bar::parse(&battle).map(|e| e.len()).unwrap_or(0);
    let tcol = systems.col("tables").unwrap();
    for row in systems.rows.iter_mut() {
        // A data system is checked only when every table it owns decodes from the disc.
        let owned: Vec<String> = row[tcol]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
            .unwrap_or_default();
        if !owned.is_empty() {
            let ok = owned.iter().filter(|t| validated.contains(*t)).count();
            let done = ok == owned.len();
            row[icol] = if done { Value::String("yes".into()) } else { Value::Null };
            row[vcol] = if done { Value::String(format!("{ok}/{} tables decode", owned.len())) } else { Value::Null };
            if !done {
                println!("{:14} {ok}/{} tables validated", row[0].as_str().unwrap_or_default(), owned.len());
            }
            continue;
        }
        let status = match row[0].as_str().unwrap_or_default() {
            "fs.iso" => Some(format!("{} disc files", a.iso().entries().len())),
            "fs.archive" if overlays_ok => Some(format!("{} entries, {named} named, overlays decompress", a.entries().len())),
            "fs.bar" if bar_n > 0 => Some(format!("00battle.bin: {bar_n} subfiles")),
            _ => None,
        };
        if let Some(s) = status {
            if row[icol].is_null() {
                row[icol] = Value::String("yes".into());
            }
            row[vcol] = Value::String(s);
        }
    }
    systems.save().map_err(|e| e.to_string())?;
    println!("validated {pass} tables; sheets updated");
    Ok(())
}
