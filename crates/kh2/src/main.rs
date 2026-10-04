//! `kh2` - command line for kh2-rs. Everything it reads comes from the user's own disc at runtime;
//! anything it writes goes to `local/` (git-ignored), except `validate`, which writes checkmarks
//! (status text, never game data) into `sheets/`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use kh2_archive::Archive;

mod oracle;
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
            if args.iter().any(|x| x == "--shapes") {
                for s in &m.shapes {
                    println!(
                        "group {:3} kind {:2} shape {} bone {:?} off {:?} r {} h {} parts {} arg {}",
                        s.group, s.kind, s.shape, s.bone, s.offset, s.radius, s.height, s.parts, s.argument
                    );
                }
            }
            if args.iter().any(|x| x == "--bones") {
                for (i, (b, m)) in m.bones.iter().zip(kh2_model::rest_pose(&m.bones)).enumerate() {
                    let t = m.w_axis;
                    println!("bone {i:3} parent {:3} rest ({:7.1} {:7.1} {:7.1})", b.parent, t.x, t.y, t.z);
                }
            }
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
        "motion" => {
            let mut a = open(args)?;
            let name = args.first().ok_or("motion needs an mset name")?.clone();
            let slot: usize = args.get(1).and_then(|s| s.parse().ok()).ok_or("motion needs a slot")?;
            let m = kh2_anim::Mset::parse(&a.read(&name).map_err(|e| e.to_string())?).ok_or("not an MSET")?;
            let s = m.get(slot).ok_or("empty slot")?;
            let mo = s.motion.as_ref().ok_or("no motion")?;
            println!("{} slot {slot}: frames {}..{} ret {} @ {} fps", s.tag, mo.frame_start, mo.frame_end, mo.frame_return, mo.fps);
            if let kh2_anim::motion::Data::Interpolated(ip) = &mo.data {
                let times = ip.forward.iter().chain(&ip.inverse).flat_map(|c| c.keys.iter().map(|k| k.time));
                let (lo, hi) = times.fold((f32::MAX, f32::MIN), |(a, b), t| (a.min(t), b.max(t)));
                println!("key times {lo}..{hi}");
                if let Some(r) = &ip.root {
                    println!("root base T {:?} R {:?} S {:?} curves {:?}", r.translation, r.rotation, r.scale, r.curves);
                    let (t0, t1) = (ip.root_translation(mo.frame_start), ip.root_translation(mo.frame_end));
                    println!("root T at start {t0:?} end {t1:?} delta {:?}", t1 - t0);
                    for f in [0.25f32, 0.5, 0.75] {
                        let fr = mo.frame_start + (mo.frame_end - mo.frame_start) * f;
                        println!("  root T @{fr:.1} {:?} yaw {:.3}", ip.root_translation(fr), ip.root_yaw(fr));
                    }
                }
                let ikj: Vec<String> = ip.joints.iter().filter(|j| j.flags != 0).map(|j| format!("{}:{:#04x}", j.index, j.flags)).collect();
                println!("joints {} (ik-flagged: {})", ip.joints.len(), ikj.join(" "));
                for (i, (p, f)) in ip.helper_parent.iter().zip(&ip.helper_flags).enumerate() {
                    let h = &ip.helpers[i];
                    println!("  helper {} parent {p} flags {f:#x} T {:?} R {:?}", ip.bone_count + i, h.translation, h.rotation);
                }
                for c in &ip.constraints {
                    let acts: Vec<String> = (0..c.act_count.max(0))
                        .filter_map(|k| ip.activations.get((c.act_start + k) as usize).map(|(t, a)| format!("{t}:{a}")))
                        .collect();
                    println!("  constraint kind {:2} {} <- {} lim {} temp {} acts [{}]", c.kind, c.constrained, c.source, c.limiter, c.temp_active, acts.join(" "));
                }
                println!("limiters {} expressions {} nodes {} effectors {:?}", ip.limiters.len(), ip.expressions.len(), ip.nodes.len(), ip.effectors);
                for e in &ip.expressions {
                    println!("  expr joint {} ch {} node {}", e.joint, e.channel, e.node);
                }
                if args.iter().any(|x| x == "--nodes") {
                    for (i, nd) in ip.nodes.iter().enumerate() {
                        println!("  node {i:3} kind {:3} global {} element {:4} value {:10.4} car {:4} cdr {:4}", nd.kind, nd.global as u8, nd.element, nd.value, nd.car, nd.cdr);
                    }
                    for (i, l) in ip.limiters.iter().enumerate() {
                        println!("  limiter {i} flags {:#x} width {} strength {} min {:?} max {:?}", l.flags, l.damping_width, l.damping_strength, l.min, l.max);
                    }
                    for c in &ip.forward {
                        if c.pre != 0 || c.post != 0 {
                            println!("  curve joint {} ch {} pre {} post {} keys {}", c.joint, c.channel, c.pre, c.post, c.keys.len());
                        }
                    }
                }
            }
        }
        "rig-sweep" => {
            // Every slot of an MSET at several frames: native rig vs the game's evaluator.
            let mut a = open(args)?;
            let [model, mset] = [0, 1].map(|i| args.get(i).cloned().unwrap_or_default());
            let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).ok_or("executable not on disc")?;
            let mdlx = a.read(&model).map_err(|e| e.to_string())?;
            let msetb = a.read(&mset).map_err(|e| e.to_string())?;
            let bones = oracle::bone_records(&mdlx).ok_or("no bones")?;
            let m = kh2_model::mdlx(&mdlx).ok_or("model")?;
            let set = kh2_anim::Mset::parse(&msetb).ok_or("not an MSET")?;
            let mut o = oracle::Oracle::new(&elf)?;
            let (mut worst, mut checked, mut bad) = (0f32, 0, 0);
            for (slot, s) in set.slots.iter().enumerate() {
                let Some(s) = s else { continue };
                let Some(mo) = &s.motion else { continue };
                if !matches!(mo.data, kh2_anim::motion::Data::Interpolated(_)) {
                    continue;
                }
                let Some(mf) = oracle::motion_file(&msetb, slot) else { continue };
                let mut slot_err = 0f32;
                let mut fault = None;
                for k in 0..4 {
                    let f = mo.frame_start + (mo.frame_end - mo.frame_start) * k as f32 / 3.0;
                    match o.pose(&bones, &mf, f) {
                        Ok(game) => {
                            let native = mo.pose(&m.bones, f);
                            for (g, n) in game.bones.iter().zip(&native) {
                                let d = (glam::Mat4::from_cols_array(g).w_axis - n.w_axis).truncate().length();
                                if d.is_finite() {
                                    slot_err = slot_err.max(d);
                                } else {
                                    slot_err = f32::INFINITY;
                                }
                            }
                        }
                        Err(e) => fault = Some(e),
                    }
                }
                checked += 1;
                worst = worst.max(slot_err);
                if slot_err > 0.1 || fault.is_some() {
                    bad += 1;
                    println!("slot {slot:4} {} err {slot_err:.3} {}", s.tag, fault.unwrap_or_default());
                }
            }
            println!("{checked} motions checked, {bad} over 0.1, worst {worst:.4}");
        }
        "rig" => {
            let mut a = open(args)?;
            let [model, mset] = [0, 1].map(|i| args.get(i).cloned().unwrap_or_default());
            let slot: usize = args.get(2).and_then(|s| s.parse().ok()).ok_or("rig MDLX MSET SLOT [FRAME]")?;
            let frame: f32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).ok_or("executable not on disc")?;
            let mdlx = a.read(&model).map_err(|e| e.to_string())?;
            let msetb = a.read(&mset).map_err(|e| e.to_string())?;
            let bones = oracle::bone_records(&mdlx).ok_or("no bones")?;
            let mf = oracle::motion_file(&msetb, slot).ok_or("no motion in slot")?;
            let mut o = oracle::Oracle::new(&elf)?;
            if std::env::var("KH2_ORACLE_DEBUG").is_ok() {
                println!("rot x0.5 {:?}", o.rotation([0.5, 0.0, 0.0])?);
                println!("rot y0.5 {:?}", o.rotation([0.0, 0.5, 0.0])?);
            }
            let t0 = std::time::Instant::now();
            let game = o.pose(&bones, &mf, frame)?;
            println!("game: {} bones, {} helpers, {} instructions, {:?}", game.bones.len(), game.helpers.len(), game.steps, t0.elapsed());
            let m = kh2_model::mdlx(&mdlx).ok_or("model")?;
            let native = kh2_anim::Motion::parse(&mf).ok_or("motion")?.pose(&m.bones, frame);
            let mut errs: Vec<(f32, usize)> = game
                .bones
                .iter()
                .zip(&native)
                .enumerate()
                .map(|(i, (g, n))| {
                    let gm = glam::Mat4::from_cols_array(g);
                    let d = (gm.w_axis - n.w_axis).truncate().length()
                        + (0..3).map(|k| (gm.col(k) - n.col(k)).truncate().length()).sum::<f32>();
                    (d, i)
                })
                .collect();
            errs.sort_by(|a, b| b.0.total_cmp(&a.0));
            println!("max error {:.4}, bones over 0.01: {}", errs[0].0, errs.iter().filter(|e| e.0 > 0.01).count());
            // First divergence in evaluation order, helpers included.
            let mo = kh2_anim::Motion::parse(&mf).ok_or("motion")?;
            if let kh2_anim::motion::Data::Interpolated(ip) = &mo.data {
                let mut st = kh2_anim::rig::RigState::default();
                mo.pose_with(&m.bones, frame, &mut st);
                let mut shown = 0;
                for (order, jt) in ip.joints.iter().enumerate() {
                    let j = jt.index as usize;
                    let g = if j < ip.bone_count { game.bones.get(j) } else { game.helpers.get(j - ip.bone_count) };
                    let (Some(g), Some(n)) = (g, st.fin.get(j)) else { continue };
                    let gm = glam::Mat4::from_cols_array(g);
                    let dt = (gm.w_axis - n.w_axis).truncate().length();
                    let dr = (0..3).map(|k| (gm.col(k) - n.col(k)).truncate().length()).sum::<f32>();
                    let gw = glam::Mat4::from_cols_array(&game.unscaled[j]);
                    let dw = (gw.w_axis - st.world[j].w_axis).truncate().length();
                    if dw > 0.05 && shown < 8 {
                        shown += 1;
                        println!("  order {order:3} joint {j:3} unscaled T differs: game {:?} native {:?}", gw.w_axis.truncate(), st.world[j].w_axis.truncate());
                    }
                    let watch = std::env::var("KH2_RIG_JOINTS").unwrap_or_default();
                    let watched = watch.split(',').any(|w| w.parse() == Ok(j));
                    if ((dt > 0.05 || dr > 0.01) && shown < 8) || watched {
                        shown += 1;
                        println!(
                            "  order {order:3} joint {j:3} flags {:#04x} parent {:4} dT {dt:8.3} dR {dr:6.3}\n    game T {:?} X {:?}\n    nat  T {:?} X {:?}",
                            jt.flags,
                            if j < ip.bone_count { m.bones[j].parent } else { ip.helper_parent[j - ip.bone_count] },
                            gm.w_axis.truncate(),
                            gm.x_axis.truncate(),
                            n.w_axis.truncate(),
                            n.x_axis.truncate()
                        );
                    }
                }
            }
            let verbose = args.iter().any(|x| x == "-v");
            for &(d, i) in errs.iter().take(if verbose { errs.len() } else { 12 }) {
                let g = glam::Mat4::from_cols_array(&game.bones[i]);
                println!(
                    "  bone {i:3} err {d:9.4} game T {:8.2} {:8.2} {:8.2} native T {:8.2} {:8.2} {:8.2}",
                    g.w_axis.x, g.w_axis.y, g.w_axis.z, native[i].w_axis.x, native[i].w_axis.y, native[i].w_axis.z
                );
            }
        }
        "coct" => {
            let mut a = open(args)?;
            let name = args.first().ok_or("coct needs a map file name")?.clone();
            let c = kh2_map::Coct::from_map(&a.read(&name).map_err(|e| e.to_string())?).ok_or("no COCT")?;
            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
            let mut by_flags = std::collections::BTreeMap::new();
            let mut bad_plane = 0;
            for p in &c.polygons {
                *by_flags.entry(p.flags).or_insert(0usize) += 1;
                for v in &p.verts[..if p.quad { 4 } else { 3 }] {
                    for k in 0..3 {
                        lo[k] = lo[k].min(v[k]);
                        hi[k] = hi[k].max(v[k]);
                    }
                    // vertices must lie on their plane
                    let dist = p.normal[0] * v[0] + p.normal[1] * v[1] + p.normal[2] * v[2] + p.d;
                    if dist.abs() > 1.0 {
                        bad_plane += 1;
                    }
                }
            }
            println!("{name}: {} polygons, bounds {lo:?} .. {hi:?}, vertices off-plane: {bad_plane}", c.polygons.len());
            for (f, n) in by_flags {
                let (mut floor, mut wall, mut ceil) = (0, 0, 0);
                for p in c.polygons.iter().filter(|p| p.flags == f) {
                    match p.normal[1] {
                        y if y > 0.5 => floor += 1,
                        y if y < -0.5 => ceil += 1,
                        _ => wall += 1,
                    }
                }
                println!("  flags {f:#010x} ({f:032b}): {n:5}  floor {floor} wall {wall} ceiling {ceil}");
            }
        }
        "mset" => {
            let mut a = open(args)?;
            let name = args.first().ok_or("mset needs a file name")?.clone();
            let filter = args.get(1).cloned().unwrap_or_default();
            let m = kh2_anim::Mset::parse(&a.read(&name).map_err(|e| e.to_string())?).ok_or("not an MSET")?;
            println!("{name}: {} slots, {} with motion", m.slots.len(), m.slots.iter().flatten().filter(|s| s.motion.is_some()).count());
            for (i, s) in m.slots.iter().enumerate() {
                let Some(s) = s else { continue };
                let Some(mo) = &s.motion else { continue };
                if !s.tag.contains(filter.as_str()) {
                    continue;
                }
                let kind = match &mo.data {
                    kh2_anim::motion::Data::Interpolated(ip) => format!("interp bones {}+{} curves {}", ip.bone_count, ip.total_count - ip.bone_count, ip.forward.len()),
                    kh2_anim::motion::Data::Raw { bones, frames } => format!("raw bones {bones} frames {}", frames.len()),
                };
                let rt: Vec<String> = s.triggers.ranges.iter().map(|r| format!("{}[{}-{}]{:?}", r.id, r.start, r.end, r.params)).collect();
                let ft: Vec<String> = s.triggers.frames.iter().map(|r| format!("{}@{}{:?}", r.id, r.frame, r.params)).collect();
                println!(
                    "{i:4} id{:3}.{} {} f{}..{} ret{} {}fps {kind} | R {} | F {}",
                    i / 4, i % 4, s.tag, mo.frame_start, mo.frame_end, mo.frame_return, mo.fps, rt.join(" "), ft.join(" ")
                );
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
