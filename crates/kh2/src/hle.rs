//! `kh2 hle run`: the user's executable on the headless reference runtime (kh2-hle), driven by a
//! pad script, with memory dumps at frame boundaries.
//!
//! ```text
//! kh2 hle run [--pad FILE] [--dump ADDR[:LEN],...] [--frames N] [--every K] [--steps M]
//! ```
//! `--pad` is a script of `vsync buttons lx ly rx ry` lines (RIG.md 2.2, port 0); `--dump` prints
//! the words at each address (default 4 bytes) every `--every` main-loop passes (default 60);
//! `--frames` is the number of passes to run (default 600); `--steps` caps the instructions
//! (default 20 billion). The run ends with a summary: overlays requested, syscall counts of the
//! last frames, anything the HLE does not implement.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_archive::Archive;
use kh2_hle::bringup::{BootEvent, Hle};
use kh2_hle::iop::Disc;
use kh2_hle::iop::pad::PadScript;

/// The user's disc as the cdvd servers see it, with a log of the archive files read.
pub struct ArchiveDisc {
    pub arc: Archive,
    /// (first sector on the disc, name) of every named archive file, sorted.
    files: Vec<(u32, String)>,
    pub log: Rc<RefCell<Vec<String>>>,
}

impl ArchiveDisc {
    pub fn new(mut arc: Archive) -> ArchiveDisc {
        let img = arc.iso().find("KH2.IMG").map_or(0, |e| e.lba);
        let mut files: Vec<(u32, String)> =
            arc.entries().iter().map(|l| (img + l.entry.offset, l.name.clone().unwrap_or_else(|| format!("{:08x}", l.entry.hash32)))).collect();
        files.sort();
        files.dedup_by_key(|f| f.0);
        ArchiveDisc { arc, files, log: Rc::default() }
    }
}

impl Disc for ArchiveDisc {
    fn read_sectors(&mut self, lsn: u32, count: u32) -> Option<Vec<u8>> {
        let i = self.files.partition_point(|f| f.0 <= lsn);
        if i > 0 && self.files[i - 1].0 == lsn {
            self.log.borrow_mut().push(self.files[i - 1].1.clone());
        }
        self.arc.iso().read_at(lsn as u64 * 2048, count as usize * 2048).ok()
    }

    fn find(&mut self, path: &str) -> Option<(u32, u32)> {
        self.arc.iso().find(path).map(|e| (e.lba, e.size))
    }
}

fn take(args: &mut Vec<String>, flag: &str) -> Result<Option<String>, String> {
    let Some(i) = args.iter().position(|a| a == flag) else { return Ok(None) };
    let v = args.get(i + 1).cloned().ok_or(format!("{flag} needs a value"))?;
    args.drain(i..i + 2);
    Ok(Some(v))
}

fn num(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let r = match s.strip_prefix("0x") {
        Some(h) => u64::from_str_radix(h, 16),
        None => s.parse(),
    };
    r.map_err(|_| format!("bad number {s}"))
}

/// One `--dump` item: `ADDR[:LEN]`, or `*PTR+OFF[:LEN]` for the bytes at OFF past the word at PTR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dump {
    pub addr: u32,
    pub deref: Option<u32>,
    pub len: usize,
}

/// `ITEM,...` with hex or decimal numbers (see [`Dump`]).
pub fn parse_dumps(s: &str) -> Result<Vec<Dump>, String> {
    s.split(',')
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let (a, n) = p.split_once(':').unwrap_or((p, "4"));
            let len = num(n)? as usize;
            match a.trim().strip_prefix('*') {
                Some(r) => {
                    let (ptr, off) = r.split_once('+').unwrap_or((r, "0"));
                    Ok(Dump { addr: num(ptr)? as u32, deref: Some(num(off)? as u32), len })
                }
                None => Ok(Dump { addr: num(a)? as u32, deref: None, len }),
            }
        })
        .collect()
}

pub fn run(args: &mut Vec<String>, arc: Archive) -> Result<(), String> {
    let sub = if args.is_empty() { String::new() } else { args.remove(0) };
    if sub != "run" {
        return Err("usage: kh2 hle run [--pad FILE] [--dump ADDR[:LEN],...] [--frames N] [--every K] [--steps M]".into());
    }
    let pad = match take(args, "--pad")? {
        Some(p) => PadScript::parse(&std::fs::read_to_string(&p).map_err(|e| format!("{p}: {e}"))?)?,
        None => PadScript::default(),
    };
    let dumps = parse_dumps(&take(args, "--dump")?.unwrap_or_default())?;
    let frames = take(args, "--frames")?.map(|v| num(&v)).transpose()?.unwrap_or(600) as usize;
    let every = take(args, "--every")?.map(|v| num(&v)).transpose()?.unwrap_or(60).max(1) as usize;
    let max_steps = take(args, "--steps")?.map(|v| num(&v)).transpose()?.unwrap_or(20_000_000_000);
    let mut arc = arc;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| arc.iso().read(p).ok()).ok_or("no SLUS_210.05 on the disc")?;
    let disc = ArchiveDisc::new(arc);
    let files = disc.log.clone();
    let mut h = Hle::new(&elf, Some(Box::new(disc))).ok_or("the executable did not load")?;
    h.iop.borrow_mut().pad = pad;
    // `--callers ADDR`: the return addresses calls to ADDR came from, per report (for finding what waits).
    let callers: Option<Rc<RefCell<std::collections::BTreeMap<u32, u64>>>> = match take(args, "--callers")? {
        Some(a) => {
            let m: Rc<RefCell<std::collections::BTreeMap<u32, u64>>> = Rc::default();
            let m2 = m.clone();
            h.hw.machine.ee.probes.insert(num(&a)? as u32, move |ee: &mut kh2_ee::Ee| *m2.borrow_mut().entry(ee.gpr[31] as u32).or_default() += 1);
            Some(m)
        }
        None => None,
    };
    let counts: Vec<(u32, Rc<std::cell::Cell<u64>>)> = parse_dumps(&take(args, "--count")?.unwrap_or_default())?
        .into_iter()
        .map(|d| {
            let a = d.addr;
            let c = Rc::new(std::cell::Cell::new(0u64));
            let c2 = c.clone();
            h.hw.machine.ee.probes.insert(a, move |_: &mut kh2_ee::Ee| c2.set(c2.get() + 1));
            (a, c)
        })
        .collect();
    let start = std::time::Instant::now();
    let show_syscalls = args.iter().any(|a| a == "--syscalls");
    let show_order = args.iter().any(|a| a == "--boot-order");
    args.retain(|a| a != "--syscalls" && a != "--boot-order");
    let mut last_counts = h.hw.machine.kernel.borrow().counts.clone();
    let mut last_frames = 0usize;
    let mut done = 0usize;
    let mut overlays_seen = 0;
    while done < frames {
        let n = every.min(frames - done);
        let before = h.hw.machine.ee.steps;
        let r = h.run_frames(n, max_steps.saturating_sub(before)).map_err(|f| format!("fault: {f:?}"))?;
        done = h.frame_count();
        let (vs, steps) = (h.hw.machine.kernel.borrow().clock.vsyncs, h.hw.machine.ee.steps);
        let pl = h.place();
        let mut line = format!("frame {done} vsync {vs} steps {steps} place {:02x}{:02x}{:02x}", pl.world, pl.area, pl.entrance);
        if let Some(p) = pl.player {
            line.push_str(&format!(" player ({:.1}, {:.1}, {:.1})", p[0], p[1], p[2]));
        }
        for d in &dumps {
            let ee = &mut h.hw.machine.ee;
            let (a, label) = match d.deref {
                Some(off) => (ee.r32(d.addr).wrapping_add(off), format!("*{:#x}+{off:#x}", d.addr)),
                None => (d.addr, format!("{:#x}", d.addr)),
            };
            let b: Vec<u8> = (0..d.len as u32).map(|i| ee.r8(a.wrapping_add(i))).collect();
            let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
            line.push_str(&format!(" {label}={hex}"));
        }
        for (a, c) in &counts {
            line.push_str(&format!(" #{a:#x}={}", c.get()));
        }
        println!("{line}");
        if show_syscalls {
            let now = h.hw.machine.kernel.borrow().counts.clone();
            let n = (done - last_frames).max(1) as f64;
            let per: Vec<String> =
                now.iter().map(|(k, v)| (k, v - last_counts.get(k).copied().unwrap_or(0))).filter(|e| e.1 > 0).map(|(k, d)| format!("{k:#x}:{:.2}", d as f64 / n)).collect();
            println!("  syscalls per frame {}", per.join(" "));
            last_counts = now;
            last_frames = done;
        }
        if let Some(m) = &callers {
            let mut v: Vec<(u32, u64)> = m.borrow().iter().map(|(a, n)| (*a, *n)).collect();
            v.sort_by_key(|e| std::cmp::Reverse(e.1));
            let top: Vec<String> = v.iter().take(12).map(|(a, n)| format!("{a:#x}:{n}")).collect();
            println!("  callers {}", top.join(" "));
            m.borrow_mut().clear();
        }
        for (_, e) in h.events.borrow().iter().skip(overlays_seen) {
            if let BootEvent::Overlay(name) = e {
                println!("  overlay {name}");
            }
        }
        overlays_seen = h.events.borrow().len();
        for f in files.borrow_mut().drain(..) {
            println!("  file {f}");
        }
        if !matches!(r, kh2_hle::RunStop::Requested) {
            println!("stopped: {r:?}");
            break;
        }
    }
    if show_order {
        for e in h.boot_order() {
            match e {
                BootEvent::Init(a) => println!("  init {a:#x}"),
                BootEvent::Bind(s) => println!("  bind {s:#x}"),
                BootEvent::Module(m) => println!("  module {m}"),
                BootEvent::Overlay(o) => println!("  overlay {o}"),
            }
        }
    }
    let k = h.hw.machine.kernel.borrow();
    let iop = h.iop.borrow();
    println!("{} frames, {} vsyncs, {} instructions, {:.1} s", h.frame_count(), k.clock.vsyncs, h.hw.machine.ee.steps, start.elapsed().as_secs_f64());
    println!("unimplemented syscalls: {:x?}", k.unimplemented);
    println!("unknown rpc (server, fno): {:x?}", iop.unknown);
    println!("unimplemented registers: {:x?}", h.hw.hw.unimplemented);
    let movies = h.hw.movies.borrow();
    println!("movie calls: {} (starts: {})", movies.len(), movies.iter().filter(|c| c.0 == 0).count());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_items_parse_plain_and_pointer_forms() {
        let d = parse_dumps("0x356f34,0x33e830:3,*0x35c998+0x70:12").unwrap();
        assert_eq!(d[0], Dump { addr: 0x356f34, deref: None, len: 4 });
        assert_eq!(d[1], Dump { addr: 0x33e830, deref: None, len: 3 });
        assert_eq!(d[2], Dump { addr: 0x35c998, deref: Some(0x70), len: 12 });
        assert!(parse_dumps("zz").is_err());
        assert!(parse_dumps("").unwrap().is_empty());
    }
}
