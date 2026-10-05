//! Runs this crate's interpreter and the game's own interpreter (FUN_001d7cb0, executed in the kh2-ee
//! R5900 emulator on the user's executable) on the same scripts and compares them instruction by
//! instruction: program counter, operand stack depth, frame pointer and top of stack after every
//! step, plus every host call and its arguments. Host functions of groups 1 and 2 (and the non-arithmetic ones of group 0) are replaced on both
//! sides by the same scripted replies; the arithmetic functions of group 0 (vectors, random numbers) run natively on the game's side.
//!
//! Skips when no disc image is configured (KH2_ISO). KH2_BDX_ORACLE_ALL=1 checks every script on the disc.

use std::cell::RefCell;
use std::collections::HashMap;

use kh2_bdx::exe::HostTable;
use kh2_bdx::insn::Insn;
use kh2_bdx::math::{self, VecAccess};
use kh2_bdx::program::Program;
use kh2_bdx::vm::{self, Env, Mem, Shared, Stop, Thread, TraceState, VmError};
use kh2_ee::Ee;

const PROG: u32 = 0x0140_0000;
const THREAD: u32 = 0x0150_0000;
const GLOB: u32 = 0x0151_0000;
const FRAME: u32 = 0x0152_0000;
const VALS: u32 = 0x0153_0000;
const HOSTBUF: u32 = 0x0154_0000;
const ARGMEM: u32 = 0x0155_0000;
const STACK: u32 = 0x01ff_0000;
const CB_ADDR: u32 = 0x0000_3000;
const GROUP_TABLE: u32 = 0x35bce8;
const SEED_ADDR: u32 = 0x35b9a8;
const IDENTITY: u32 = 0x354a60;
const SEED: u32 = 0x1234_5679;

#[derive(Debug, Clone, PartialEq)]
enum Canon {
    Raw(u32),
    Code(u32),
    Global(u32),
    Frame(u32),
    Scratch,
}

type Step = (usize, usize, usize, Option<Canon>);

#[derive(Default)]
struct EeState {
    n: u32,
    by_addr: HashMap<u32, (u16, u16, usize, bool)>,
    steps: Vec<Step>,
    calls: Vec<(u16, u16, Vec<Canon>)>,
    code_len: u32,
}

thread_local! {
    static ST: RefCell<EeState> = RefCell::new(EeState::default());
}

fn canon_ee(v: u32, st: &EeState) -> Canon {
    if (PROG + 0x10..PROG + 0x10 + st.code_len).contains(&v) {
        Canon::Code(v - PROG - 0x10)
    } else if (GLOB..GLOB + 0x4000).contains(&v) {
        Canon::Global(v - GLOB)
    } else if (FRAME..FRAME + 0x1000).contains(&v) {
        Canon::Frame(v - FRAME)
    } else if (0x522100..0x522300).contains(&v) {
        Canon::Scratch
    } else {
        Canon::Raw(v)
    }
}

/// The regions of this crate's interpreter sit where the game's interpreter has them, so only the
/// result buffers of the vector functions (static memory in the game) need a common name.
fn canon_vm(v: u32, _th: u32, code_len: u32) -> Canon {
    let l = layout();
    if (l.code..l.code + code_len).contains(&v) {
        Canon::Code(v - l.code)
    } else if (l.globals..l.globals + 0x4000).contains(&v) {
        Canon::Global(v - l.globals)
    } else if (l.frames..l.frames + 0x1000).contains(&v) {
        Canon::Frame(v - l.frames)
    } else if (l.scratch..l.scratch + 0x100).contains(&v) {
        Canon::Scratch
    } else {
        Canon::Raw(v)
    }
}

fn layout() -> vm::Layout {
    vm::Layout { code: PROG + 0x10, globals: GLOB, frames: FRAME, frame_stride: 0x1_0000, scratch: 0x0522_2000 }
}

/// Scripted reply of a host function of group 1 or 2.
fn policy(g: u16, i: u16, n: u32) -> u32 {
    if matches!((g, i), (1, 62) | (1, 64) | (1, 120) | (1, 147) | (1, 201)) {
        return HOSTBUF + 16 * (n % 8);
    }
    [0, 0x3f80_0000, 3, 99, 0, 2][((n + i as u32) % 6) as usize]
}

fn hostbuf_bytes() -> Vec<u8> {
    let mut b = Vec::new();
    for k in 0..8u32 {
        for x in [k as f32 * 10.0 + 1.0, 2.0, 3.0 + k as f32, 0.0] {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }
    b
}

fn host_hook(ee: &mut Ee) {
    let pc = ee.pc;
    let a0 = ee.gpr[kh2_ee::reg::A0] as u32;
    let (g, i, nargs, ret) = ST.with(|s| s.borrow().by_addr[&pc]);
    let args: Vec<u32> = (0..nargs as u32).map(|k| ee.r32(a0 + 4 * k)).collect();
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let n = s.n;
        s.n += 1;
        let canon: Vec<Canon> = args.iter().map(|&v| canon_ee(v, &s)).collect();
        s.calls.push((g, i, canon));
        if ret {
            drop(s);
            let r = policy(g, i, n);
            ee.w32(a0, r);
        }
    });
}

fn cb_hook(ee: &mut Ee) {
    let th = ee.gpr[kh2_ee::reg::A0] as u32;
    let pc = ee.r32(th + 4) as usize;
    let fp = ee.r32(th + 8);
    let vsp = ee.r32(th + 12);
    let depth = (vsp.wrapping_sub(VALS) / 4) as usize;
    let top = if depth > 0 && depth < 0x1000 { Some(ee.r32(vsp - 4)) } else { None };
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let t = top.map(|v| canon_ee(v, &s));
        s.steps.push((pc, depth, fp.wrapping_sub(FRAME) as usize, t));
    });
}

struct Io<'a>(&'a mut TestExt);

struct TestExt(Vec<u8>);

impl VecAccess for Io<'_> {
    fn read_vec(&mut self, mem: &mut Mem<'_>, addr: u32) -> Result<[f32; 4], VmError> {
        match mem.read_vec4(addr) {
            Ok(v) => Ok(v),
            Err(vm::Outside::Ext(a)) => {
                let o = a.wrapping_sub(EXT) as usize;
                let b = self.0.0.get(o..o + 16).ok_or(VmError::BadAddress(a))?;
                Ok(std::array::from_fn(|k| f32::from_le_bytes(b[4 * k..4 * k + 4].try_into().unwrap())))
            }
            Err(_) => Err(VmError::BadAddress(addr)),
        }
    }

    fn write_vec(&mut self, mem: &mut Mem<'_>, addr: u32, v: [f32; 4]) -> Result<(), VmError> {
        match mem.write_vec4(addr, v) {
            Ok(()) => Ok(()),
            Err(vm::Outside::Ext(a)) => {
                let o = a.wrapping_sub(EXT) as usize;
                let b = self.0.0.get_mut(o..o + 16).ok_or(VmError::BadAddress(a))?;
                for k in 0..4 {
                    b[4 * k..4 * k + 4].copy_from_slice(&v[k].to_le_bytes());
                }
                Ok(())
            }
            Err(_) => Err(VmError::BadAddress(addr)),
        }
    }
}

/// Memory outside the script that both sides can reach: host reply buffers and the argument words' targets.
const EXT: u32 = 0x0154_0000;
const EXT_LEN: usize = 0x2_0000;

fn ext_initial() -> Vec<u8> {
    let mut b = vec![0u8; EXT_LEN];
    let h = hostbuf_bytes();
    b[..h.len()].copy_from_slice(&h);
    b
}

struct TestEnv {
    ext: Vec<u8>,
    table: HostTable,
    n: u32,
    code_len: u32,
    steps: Vec<Step>,
    calls: Vec<(u16, u16, Vec<Canon>)>,
    /// The latest math call with its raw arguments and the vectors they pointed at (for failure reports).
    last_math: Vec<(usize, String)>,
}

impl Env for TestEnv {
    fn signature(&self, g: u16, i: u16) -> Option<(usize, bool)> {
        self.table.get(g, i).map(|s| (s.nargs as usize, s.returns))
    }

    fn host_call(&mut self, _p: &Program, mem: &mut Mem<'_>, g: u16, i: u16, a: &[u32]) -> Result<Option<u32>, VmError> {
        if g == 0 && math::is_math(i) {
            let mut seen = String::new();
            for &w in a {
                if let Ok(v) = mem.read_vec4(w) {
                    seen += &format!(" vec@{w:#x}={v:?}");
                }
            }
            self.last_math.push((self.steps.len(), format!("0.{i} args {a:x?}{seen}")));
            let mut ext = TestExt(std::mem::take(&mut self.ext));
            let r = math::call(mem, i, a, &mut Io(&mut ext));
            self.ext = ext.0;
            return r;
        }
        let ret = self.table.get(g, i).unwrap().returns;
        let n = self.n;
        self.n += 1;
        let cl = self.code_len;
        self.calls.push((g, i, a.iter().map(|&v| canon_vm(v, 0, cl)).collect()));
        Ok(ret.then(|| policy(g, i, n)))
    }

    fn ext_read(&mut self, addr: u32, out: &mut [u8]) -> bool {
        let o = addr.wrapping_sub(EXT) as usize;
        match self.ext.get(o..o + out.len()) {
            Some(b) => {
                out.copy_from_slice(b);
                true
            }
            None => false,
        }
    }

    fn ext_write(&mut self, addr: u32, data: &[u8]) -> bool {
        let o = addr.wrapping_sub(EXT) as usize;
        match self.ext.get_mut(o..o + data.len()) {
            Some(b) => {
                b.copy_from_slice(data);
                true
            }
            None => false,
        }
    }

    fn trace(&mut self, s: &TraceState, _i: &Insn) {
        let t = s.top.map(|v| canon_vm(v, s.thread, self.code_len));
        self.steps.push((s.pc, s.depth, s.fp, t));
    }
}

fn open_disc() -> Option<(kh2_archive::Archive, Vec<u8>)> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    // File names (OpenKH's list, fetched by `kh2 fetch-names`) let the test find scripts inside archives.
    if let Ok(names) = std::fs::read_to_string("../../local/kh2idx.txt") {
        a.apply_names(names.lines()).ok()?;
    }
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    Some((a, elf))
}

/// Every BDX the archive can name: object scripts (type 3 entries of MDLX files) and 00common.bdx.
fn disc_scripts(a: &mut kh2_archive::Archive, all: bool) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for name in ["obj/M_EX020.mdlx", "obj/M_EX010.mdlx"] {
        if let Ok(d) = a.read(name)
            && let Some(e) = kh2_bar::parse(&d).unwrap().into_iter().find(|e| e.kind == 3) {
                out.push((name.to_string(), e.data.to_vec()));
            }
    }
    if let Ok(d) = a.read("00common.bdx") {
        out.push(("00common.bdx".into(), d));
    }
    // Other scripts: BAR entries of type 3 in files whose names are known.
    let list: Vec<_> = a.entries().to_vec();
    let mut k = 0;
    for l in &list {
        let Some(name) = &l.name else { continue };
        if !(name.ends_with(".mdlx") || name.ends_with(".ard") || name.ends_with(".bar") || name.ends_with(".mag")) {
            continue;
        }
        let Ok(d) = a.read_entry(&l.entry) else { continue };
        if !kh2_bar::is_bar(&d) {
            continue;
        }
        for e in kh2_bar::parse(&d).unwrap_or_default().iter().filter(|e| e.kind == 3 && !e.data.is_empty()) {
            k += 1;
            if all || k % 25 == 0 {
                out.push((format!("{name}:{}", e.tag), e.data.to_vec()));
            }
        }
    }
    out
}

struct Report {
    scripts: usize,
    entries: usize,
    steps: usize,
    host_calls: usize,
    slices: usize,
}

fn compare_script(ee: &mut Ee, make: &dyn Fn() -> Ee, table: &HostTable, name: &str, data: &[u8], rep: &mut Report) -> Result<(), String> {
    let prog = Program::parse(data).map_err(|e| format!("{name}: {e}"))?;
    let code_len = (prog.words.len() * 2) as u32;
    rep.scripts += 1;
    let an = kh2_bdx::analyze(&prog).map_err(|e| format!("{name}: {e}"))?;
    let mut roots: Vec<(String, u32)> = prog.entries.iter().map(|e| (format!("event {}", e.id), e.pc)).collect();
    roots.extend(an.routines.iter().map(|&pc| (format!("routine {pc:#x}"), pc as u32)));
    for (root, pc) in roots {
        let entry = kh2_bdx::EntryPoint { id: 0, pc };
        rep.entries += 1;
        // --- game side set-up ---
        ee.write(PROG, data);
        ee.write(GLOB, &vec![0u8; 0x4000]);
        ee.write(FRAME, &vec![0u8; 0x1000]);
        ee.write(VALS, &vec![0u8; 0x1000]);
        ee.write(EXT, &ext_initial());
        ee.w32(SEED_ADDR, SEED);
        ee.w32(THREAD, PROG);
        ee.w32(THREAD + 4, entry.pc);
        ee.w32(THREAD + 8, FRAME + 16);
        let args = [ARGMEM + 0x10, ARGMEM + 0x20, ARGMEM + 0x30];
        for (k, a) in args.iter().enumerate() {
            ee.w32(VALS + 4 * k as u32, *a);
        }
        ee.w32(THREAD + 12, VALS + 4 * args.len() as u32);
        ee.w32(THREAD + 16, GLOB);
        ST.with(|s| {
            let mut s = s.borrow_mut();
            s.n = 0;
            s.code_len = code_len;
        });
        // --- this crate's side ---
        let mut sh = Shared::new(&prog);
        sh.layout = layout();
        sh.rng = SEED;
        let mut th = Thread::new(&prog, &mut sh);
        th.start(entry.pc as usize, &args);
        let mut env = TestEnv { ext: ext_initial(), table: table.clone(), n: 0, code_len, steps: Vec::new(), calls: Vec::new(), last_math: Vec::new() };
        for slice in 0..12 {
            ST.with(|s| {
                let mut s = s.borrow_mut();
                s.steps.clear();
                s.calls.clear();
            });
            env.steps.clear();
            env.calls.clear();
            env.last_math.clear();
            let game = ee.call(0x1d7cb0, &[THREAD as u64, GROUP_TABLE as u64, CB_ADDR as u64, 0], &[], STACK);
            if game.is_err() {
                // A processor exception leaves the emulator mid-instruction; start over for the next run.
                *ee = make();
            }
            let mine = vm::run(&prog, &mut sh, &mut th, &mut env);
            let (gsteps, gcalls) = ST.with(|s| {
                let s = s.borrow();
                (s.steps.clone(), s.calls.clone())
            });
            // The game raises a processor exception where this crate reports an error; compare up to there.
            let label = format!("{name} {root} slice {slice}");
            let game_stop = match game {
                Ok(v) => Some(v as u32),
                Err(_) => None,
            };
            let n = env.steps.len();
            if n == 0 {
                return Err(format!("{label}: no steps"));
            }
            for k in 1..n {
                if gsteps.get(k - 1) != Some(&env.steps[k]) {
                    let calls = |c: &[(u16, u16, Vec<Canon>)]| c.iter().take(8).map(|(g, i, _)| format!("{g}.{i}")).collect::<Vec<_>>().join(" ");
                    return Err(format!(
                        "{label}: step {k} differs\n  game {:?}\n  this {:?}\n  previous this {:?}\n  game calls {}\n  this calls {}\n  last math call {}",
                        gsteps.get(k - 1),
                        env.steps[k],
                        env.steps[k - 1],
                        calls(&gcalls),
                        calls(&env.calls),
                        env.last_math.iter().filter(|(n, _)| *n == k).map(|(_, m)| m.as_str()).collect::<Vec<_>>().join(" | ")
                    ));
                }
            }
            rep.steps += n;
            let stopped_cleanly = mine.is_ok();
            if stopped_cleanly {
                if gsteps.len() != n {
                    return Err(format!("{label}: game ran {} steps, this crate {}", gsteps.len(), n));
                }
                let code = match mine.as_ref().unwrap() {
                    Stop::Yield => 1,
                    Stop::Exit => 2,
                    Stop::Done => 3,
                };
                if game_stop != Some(code) {
                    return Err(format!("{label}: game result {game_stop:?}, this crate {code}"));
                }
            }
            let common = gcalls.len().min(env.calls.len());
            for k in 0..common {
                // Several table entries can share one native function; they are the same call to the game.
                let key = |c: &(u16, u16, Vec<Canon>)| (table.get(c.0, c.1).map(|s| s.address), c.2.clone());
                if key(&gcalls[k]) != key(&env.calls[k]) {
                    return Err(format!("{label}: host call {k} differs\n  game {:?}\n  this {:?}", gcalls[k], env.calls[k]));
                }
            }
            rep.host_calls += common;
            rep.slices += 1;
            if !matches!(mine, Ok(Stop::Yield)) {
                break;
            }
        }
    }
    Ok(())
}

fn make_ee(elf: &[u8], table: &HostTable) -> Ee {
    let mut ee = Ee::new();
    ee.load_elf(elf).expect("elf");
    ee.limit = 300_000_000;
    // The game fills its identity matrix constant in at start-up; the file has zeros there.
    for k in 0..16u32 {
        ee.wf32(IDENTITY + 4 * k, if k % 5 == 0 { 1.0 } else { 0.0 });
    }
    for g in 0..=11u16 {
        for i in 0..700u16 {
            if g == 0 && math::is_math(i) {
                continue;
            }
            if let Some(sig) = table.get(g, i) {
                ST.with(|s| s.borrow_mut().by_addr.insert(sig.address, (g, i, sig.nargs as usize, sig.returns)));
                ee.hooks.insert(sig.address, host_hook);
            }
        }
    }
    ee.hooks.insert(CB_ADDR, cb_hook);
    ee
}

#[test]
fn interpreter_matches_the_games() {
    let Some((mut a, elf)) = open_disc() else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return;
    };
    let table = HostTable::from_exe(&elf).expect("host table");
    let make = || make_ee(&elf, &table);
    let mut ee = make();
    let all = std::env::var("KH2_BDX_ORACLE_ALL").is_ok();
    let scripts = disc_scripts(&mut a, all);
    let mut rep = Report { scripts: 0, entries: 0, steps: 0, host_calls: 0, slices: 0 };
    let mut failures = Vec::new();
    let only = std::env::var("KH2_BDX_ORACLE_ONLY").ok();
    for (name, data) in &scripts {
        if data.is_empty() || only.as_ref().is_some_and(|o| !name.contains(o.as_str())) {
            continue;
        }
        if let Err(e) = compare_script(&mut ee, &make, &table, name, data, &mut rep) {
            failures.push(e);
        }
    }
    eprintln!(
        "oracle: {} scripts, {} entries, {} slices, {} interpreter steps and {} host calls compared, {} failures",
        rep.scripts,
        rep.entries,
        rep.slices,
        rep.steps,
        rep.host_calls,
        failures.len()
    );
    for f in failures.iter().take(5) {
        eprintln!("{f}");
    }
    assert!(failures.is_empty(), "{} scripts differ from the game's interpreter", failures.len());
}
