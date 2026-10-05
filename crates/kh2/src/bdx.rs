//! `kh2 bdx`: disassemble, scan and dry-run the game's BDX scripts.

use std::collections::BTreeMap;

use kh2_bdx::{Call, Insn, Program, Runtime, analyze, listing, mock::MockHost};

/// Every BDX on the disc: `(label, bytes)`.
fn collect(a: &mut kh2_archive::Archive) -> Vec<(String, Vec<u8>)> {
    let list: Vec<_> = a.entries().to_vec();
    let mut out = Vec::new();
    for l in &list {
        let name = l.name.clone().unwrap_or_else(|| format!("<{:08x}>", l.entry.hash32));
        let Ok(data) = a.read_entry(&l.entry) else { continue };
        if name.ends_with(".bdx") {
            out.push((name, data));
        } else if kh2_bar::is_bar(&data)
            && let Ok(es) = kh2_bar::parse(&data)
        {
            for e in es.iter().filter(|e| e.kind == 3) {
                out.push((format!("{name}:{}", e.tag), e.data.to_vec()));
            }
        }
    }
    out
}

fn resolve(a: &mut kh2_archive::Archive, arg: &str) -> Result<Vec<u8>, String> {
    let candidates: Vec<String> = if arg.contains('/') || arg.contains('.') {
        vec![arg.to_string()]
    } else {
        vec![format!("obj/{}.mdlx", arg.to_uppercase()), format!("obj/{}.mdlx", arg)]
    };
    for c in &candidates {
        if let Ok(data) = a.read(c) {
            if kh2_bar::is_bar(&data) {
                let es = kh2_bar::parse(&data).map_err(|e| format!("{c}: {e:?}"))?;
                return es.iter().find(|e| e.kind == 3).map(|e| e.data.to_vec()).ok_or_else(|| format!("{c}: no BDX entry (type 3)"));
            }
            return Ok(data);
        }
    }
    Err(format!("not in the archive: {arg} (tried {candidates:?}); run `kh2 fetch-names` if names are missing"))
}

pub fn run(a: &mut kh2_archive::Archive, args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) == Some("--scan") {
        return scan(a);
    }
    let name = args.first().ok_or("bdx needs a name (or --scan)")?;
    let data = resolve(a, name)?;
    if data.is_empty() {
        return Err(format!("{name}: empty BDX entry (the object has no AI script)"));
    }
    let prog = Program::parse(&data).map_err(|e| format!("{name}: {e}"))?;
    if args.get(1).map(String::as_str) == Some("--run") {
        let frames: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(120);
        return dry_run(prog, frames);
    }
    let an = analyze(&prog).map_err(|e| format!("{name}: {e}"))?;
    print!("{}", listing(&prog, &an));
    Ok(())
}

fn dry_run(prog: Program, frames: u32) -> Result<(), String> {
    let mut rt = Runtime::new(prog);
    rt.shared.rng = 12345;
    let mut host = MockHost::new();
    if !rt.spawn(&mut host, kh2_bdx::mock::SELF_OBJ) {
        return Err("script has no main event (1)".into());
    }
    let mut mode = None;
    let mut printed = 0;
    for frame in 0..frames {
        rt.update(&mut host);
        let now = rt.current_mode();
        if now != mode {
            println!("{frame:4}  == mode {}", now.as_deref().unwrap_or("(none)"));
            mode = now;
        }
        // Calls that only read the world or repeat every frame are left out of the listing.
        for (f, call) in &host.log[printed..] {
            match call {
                Call::Position { .. }
                | Call::TargetPosition { .. }
                | Call::Target { .. }
                | Call::Player
                | Call::FacingVector { .. }
                | Call::ObjVec850 { .. }
                | Call::ObjVec860 { .. }
                | Call::ModeFrozen { .. }
                | Call::ModeInterrupted { .. }
                | Call::ModeMayStart { .. }
                | Call::SpawnParam { .. }
                | Call::MotionDone { .. }
                | Call::MotionLength { .. }
                | Call::HitStun { .. }
                | Call::Busy { .. }
                | Call::Flag124 { .. }
                | Call::OnScreen { .. } => {}
                c => println!("{f:4}    {c:?}"),
            }
        }
        printed = host.log.len();
        host.tick();
    }
    println!("mode now: {:?}; script errors: {:?}", rt.current_mode(), rt.core.errors);
    Ok(())
}

fn scan(a: &mut kh2_archive::Archive) -> Result<(), String> {
    let files = collect(a);
    println!("{} BDX entries", files.len());
    let (mut empty, mut ok) = (0, 0);
    let mut fails = Vec::new();
    let mut ops: BTreeMap<String, usize> = BTreeMap::new();
    let mut host: BTreeMap<(u16, u16), (usize, usize)> = BTreeMap::new();
    let mut events: BTreeMap<u32, usize> = BTreeMap::new();
    for (label, data) in &files {
        if data.is_empty() {
            empty += 1;
            continue;
        }
        let r = Program::parse(data).and_then(|p| analyze(&p).map(|an| (p, an)));
        let (p, an) = match r {
            Ok(x) => x,
            Err(e) => {
                fails.push(format!("{label}: {e}"));
                continue;
            }
        };
        ok += 1;
        for e in &p.entries {
            *events.entry(e.id).or_default() += 1;
        }
        let mut seen = std::collections::BTreeSet::new();
        for (i, _) in an.code.values() {
            let dbg = format!("{i:?}");
            let kind = dbg.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("?").to_string();
            *ops.entry(kind).or_default() += 1;
            if let Insn::Host { group, index } = i {
                host.entry((*group, *index)).or_default().0 += 1;
                seen.insert((*group, *index));
            }
        }
        for k in seen {
            host.entry(k).or_default().1 += 1;
        }
    }
    println!("parsed {ok}, empty {empty}, failed {}", fails.len());
    for f in &fails {
        println!("  FAIL {f}");
    }
    println!("instruction kinds: {ops:?}");
    println!("entry events (id: scripts): {events:?}");
    let named = host.keys().filter(|k| kh2_bdx::names::host_name(k.0, k.1).is_some()).count();
    println!("{} distinct host functions called, {named} named in this crate", host.len());
    let mut rows: Vec<_> = host.iter().collect();
    rows.sort_by_key(|(_, (uses, _))| std::cmp::Reverse(*uses));
    for ((g, i), (uses, files)) in rows {
        println!("host {g}.{i:<4} {uses:6} uses in {files:4} scripts  {}", kh2_bdx::names::host_name(*g, *i).unwrap_or("-"));
    }
    Ok(())
}
