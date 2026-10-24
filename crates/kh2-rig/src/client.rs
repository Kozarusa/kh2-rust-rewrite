//! The PCSX2 rig client: runs a scenario on the original game and writes the trace.
//!
//! How a run goes (measured in local/specs/RIG.md section 1):
//! 1. The state (`.p2s`) is copied to the rig's own PINE slot 0 file in PCSX2's sstates folder (the user's numbered slots are never
//!    written), and the patched PCSX2-MCP (`local/tools/pcsx2-mcp`, a debug server on TCP 21512 beside PINE on 28011) is started with
//!    `-batch [-unlimited] -statefile <slot 0 file> -- <iso>`.
//! 2. An execution breakpoint is set at `0x167370`, the instruction after the pad read of the pad task, which runs once per game frame
//!    for each of the two pads. At a stop with `s0 == 0x35b4c0` (pad 1) the 18-byte buffer at `sp` holds what the game just read. The
//!    client reads the vsync counter `0x33fe64` (`k` = it minus its value in the state), writes the scripted pad bytes over the first 6
//!    bytes, reads the values that are due, and resumes. Everything the previous game frame wrote is settled at that point.
//! 3. The state is loaded through PINE slot 0 once the breakpoint is set (`-unlimited` runs 100 vsyncs before a breakpoint could be set
//!    otherwise); see [`Attach`].
//! 4. At the last checkpoint the breakpoint is cleared and PCSX2 is killed. Rows go to `trace-orig.csv` (see `trace`).
//!
//! Only one PCSX2 may run at a time: the run takes `local/pcsx2.lock` (the main checkout's `local`; waits while another run holds a
//! lock younger than two hours) and refuses to start when the debug server or PINE port already answers. Conditional breakpoints crash
//! the patched build in `-unlimited` mode, so the breakpoint is unconditional and pad 2's stops are skipped here.

use crate::pad::PadScript;
use crate::p2s::P2s;
use crate::save::{self, Extract};
use crate::scenario::{Scenario, Values};
use crate::trace::Trace;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// The pad task's instruction after the pad read (`bltz v0`).
pub const PAD_READ_STOP: u32 = 0x0016_7370;
/// `s0` at that stop for pad 1 (pad 2 is `0x35b640`).
pub const PAD1_RECORD: u32 = 0x0035_b4c0;
pub const DEBUG_PORT: u16 = 21512;
pub const PINE_PORT: u16 = 28011;
/// The game's id as PCSX2 names its state files.
pub const GAME_ID: &str = "SLUS-21005 (DA0535FD)";
/// A lock file younger than this is respected.
const LOCK_FRESH: Duration = Duration::from_secs(2 * 3600);

// --- the debug server --------------------------------------------------------------------------------------------------------------

/// The patched PCSX2's debug server: newline-delimited JSON over TCP.
pub struct Debug {
    r: BufReader<TcpStream>,
    w: TcpStream,
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return Err("bad hex".into());
    }
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())).collect()
}

fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Debug {
    pub fn connect(port: u16) -> Result<Debug, String> {
        let s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("debug server {port}: {e}"))?;
        s.set_nodelay(true).ok();
        s.set_read_timeout(Some(Duration::from_secs(60))).ok();
        Ok(Debug { r: BufReader::new(s.try_clone().map_err(|e| e.to_string())?), w: s })
    }

    pub fn call(&mut self, req: Value) -> Result<Value, String> {
        let mut line = req.to_string();
        line.push('\n');
        self.w.write_all(line.as_bytes()).map_err(|e| format!("debug server: {e}"))?;
        let mut resp = String::new();
        if self.r.read_line(&mut resp).map_err(|e| format!("debug server: {e}"))? == 0 {
            return Err("debug server closed the connection".into());
        }
        let v: Value = serde_json::from_str(&resp).map_err(|e| format!("debug server: bad JSON ({e}): {resp:.120}"))?;
        if v["ok"] != json!(true) {
            return Err(format!("debug server: {}", v["error"].as_str().unwrap_or("request failed")));
        }
        Ok(v)
    }

    pub fn mem(&mut self, addr: u32, len: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let n = (len - out.len()).min(65536);
            let v = self.call(json!({"cmd": "read_memory", "address": addr as usize + out.len(), "length": n}))?;
            let b = hex_decode(v["hex"].as_str().ok_or("read_memory: no hex")?)?;
            if b.len() != n {
                return Err(format!("read_memory returned {} of {n} bytes", b.len()));
            }
            out.extend_from_slice(&b);
        }
        Ok(out)
    }

    pub fn u32(&mut self, addr: u32) -> Result<u32, String> {
        let b = self.mem(addr, 4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn write(&mut self, addr: u32, data: &[u8]) -> Result<(), String> {
        self.call(json!({"cmd": "write_memory", "address": addr, "data": hex_encode(data)})).map(|_| ())
    }

    pub fn paused(&mut self) -> Result<bool, String> {
        Ok(self.call(json!({"cmd": "status"}))?["data"]["paused"] == json!(true))
    }

    pub fn wait_paused(&mut self, timeout: Duration) -> Result<(), String> {
        let t = Instant::now();
        while t.elapsed() < timeout {
            if self.paused()? {
                return Ok(());
            }
        }
        Err(format!("the game did not stop at the breakpoint within {} s", timeout.as_secs()))
    }

    /// `(s0, sp)` from the general registers.
    pub fn s0_sp(&mut self) -> Result<(u32, u32), String> {
        let v = self.call(json!({"cmd": "read_registers", "category": 0}))?;
        let cat = v["data"].as_object().and_then(|o| o.values().find(|c| c.get("regs").is_some())).ok_or("read_registers: no register category")?;
        let reg = |name: &str| -> Result<u32, String> {
            let r = cat["regs"].as_array().and_then(|a| a.iter().find(|r| r["name"] == name)).ok_or_else(|| format!("read_registers: no {name}"))?;
            let s = r["value"].as_str().ok_or("read_registers: no value")?;
            u32::from_str_radix(&s[s.len().saturating_sub(8)..], 16).map_err(|e| e.to_string())
        };
        Ok((reg("s0")?, reg("sp")?))
    }

    pub fn set_breakpoint(&mut self, addr: u32) -> Result<(), String> {
        self.call(json!({"cmd": "set_breakpoint", "address": addr})).map(|_| ())
    }

    pub fn clear_breakpoints(&mut self) -> Result<(), String> {
        self.call(json!({"cmd": "clear_breakpoints"})).map(|_| ())
    }

    pub fn resume(&mut self) -> Result<(), String> {
        self.call(json!({"cmd": "resume"})).map(|_| ())
    }
}

// --- PINE --------------------------------------------------------------------------------------------------------------------------

/// PINE (PCSX2's IPC): request `u32 total length, u8 opcode, args`; reply `u32 total length, u8 result (0 = ok), data`.
pub struct Pine {
    s: TcpStream,
}

impl Pine {
    pub fn connect(port: u16) -> Result<Pine, String> {
        let s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("PINE {port}: {e}"))?;
        s.set_nodelay(true).ok();
        s.set_read_timeout(Some(Duration::from_secs(60))).ok();
        Ok(Pine { s })
    }

    fn call(&mut self, op: u8, args: &[u8]) -> Result<Vec<u8>, String> {
        let mut m = ((args.len() + 5) as u32).to_le_bytes().to_vec();
        m.push(op);
        m.extend_from_slice(args);
        self.s.write_all(&m).map_err(|e| format!("PINE: {e}"))?;
        let mut len = [0u8; 4];
        self.s.read_exact(&mut len).map_err(|e| format!("PINE: {e}"))?;
        let n = u32::from_le_bytes(len) as usize;
        let mut rest = vec![0u8; n.saturating_sub(4)];
        self.s.read_exact(&mut rest).map_err(|e| format!("PINE: {e}"))?;
        match rest.first() {
            Some(0) => Ok(rest[1..].to_vec()),
            _ => Err(format!("PINE: opcode {op} failed")),
        }
    }

    pub fn r32(&mut self, addr: u32) -> Result<u32, String> {
        let d = self.call(2, &addr.to_le_bytes())?;
        d.get(..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(|| "PINE: short read".to_string())
    }

    pub fn load_state(&mut self, slot: u8) -> Result<(), String> {
        self.call(10, &[slot]).map(|_| ())
    }
}

// --- locations ---------------------------------------------------------------------------------------------------------------------

/// Paths of the machine the rig runs on.
#[derive(Clone, Debug)]
pub struct Config {
    /// The main checkout's `local` folder (holds `tools/pcsx2-mcp`, `rig/`, `pcsx2.lock`).
    pub local: PathBuf,
    pub pcsx2: PathBuf,
    pub iso: PathBuf,
    pub sstates: PathBuf,
    /// Who holds the lock file.
    pub id: String,
}

const MCP_EXE: &str = "tools/pcsx2-mcp/PCSX2-MCP-v1.0.0-win64/pcsx2-qt.exe";

impl Config {
    /// From the environment: `KH2_RIG_LOCAL` (else the nearest `local` above the current directory that holds the PCSX2-MCP build),
    /// `KH2_PCSX2_MCP`, `KH2_ISO`, `KH2_SSTATES` (default: `Documents\PCSX2\sstates`), `KH2_RIG_ID` (default `rig`).
    pub fn from_env(id: Option<&str>) -> Result<Config, String> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let local = match env("KH2_RIG_LOCAL") {
            Some(p) => PathBuf::from(p),
            None => {
                let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
                cwd.ancestors().map(|d| d.join("local")).find(|l| l.join(MCP_EXE).is_file()).ok_or("no local/tools/pcsx2-mcp above the current directory; set KH2_RIG_LOCAL")?
            }
        };
        let pcsx2 = env("KH2_PCSX2_MCP").map_or_else(|| local.join(MCP_EXE), PathBuf::from);
        let iso = PathBuf::from(env("KH2_ISO").ok_or("KH2_ISO is not set")?);
        let sstates = env("KH2_SSTATES").map(PathBuf::from).or_else(|| env("USERPROFILE").map(|u| PathBuf::from(u).join("Documents/PCSX2/sstates"))).ok_or("cannot find the sstates folder; set KH2_SSTATES")?;
        Ok(Config { local, pcsx2, iso, sstates, id: id.map(str::to_string).or_else(|| env("KH2_RIG_ID")).unwrap_or_else(|| "rig".into()) })
    }

    /// The rig's own PINE slot 0 state file.
    pub fn slot0(&self) -> PathBuf {
        self.sstates.join(format!("{GAME_ID}.00.p2s"))
    }

    /// A scenario's `state`: a slot number (two digits) names the user's file `<id>.NN.p2s`; otherwise a path, or a file in
    /// `local/rig/states/`, or in the sstates folder.
    pub fn resolve_state(&self, name: &str) -> Result<PathBuf, String> {
        if !name.is_empty() && name.len() <= 2 && name.chars().all(|c| c.is_ascii_digit()) {
            let p = self.sstates.join(format!("{GAME_ID}.{:02}.p2s", name.parse::<u32>().unwrap_or(0)));
            return if p.is_file() { Ok(p) } else { Err(format!("state slot {name}: {} does not exist", p.display())) };
        }
        let candidates = [PathBuf::from(name), self.local.join("rig/states").join(name), self.sstates.join(name)];
        candidates.into_iter().find(|p| p.is_file()).ok_or_else(|| format!("state '{name}' not found (tried the path, local/rig/states, the sstates folder)"))
    }
}

/// Copies `state` to PINE slot 0 (`cfg.slot0()`) and writes the sidecar marker `<slot0>.kh2rig` holding the hash of what the rig wrote.
/// A slot 0 file that exists and does not match its marker (no marker, or another hash) is the user's own: it is moved to
/// `local/rig/backup/<unix seconds>.00.p2s` first. Returns the backup path when one was made.
pub fn install_slot0(cfg: &Config, state: &Path) -> Result<Option<PathBuf>, String> {
    let slot0 = cfg.slot0();
    let marker = marker_of(&slot0);
    let mut backup = None;
    if slot0.is_file() {
        let old = std::fs::read(&slot0).map_err(|e| format!("{}: {e}", slot0.display()))?;
        let ours = std::fs::read_to_string(&marker).is_ok_and(|m| m.trim() == format!("{:016x}", fnv1a(&old, FNV_BASIS)));
        if !ours {
            let dir = cfg.local.join("rig/backup");
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let mut to = dir.join(format!("{stamp}.00.p2s"));
            for n in 1.. {
                if !to.exists() {
                    break;
                }
                to = dir.join(format!("{stamp}-{n}.00.p2s"));
            }
            // A rename can fail across drives; copy then remove keeps the user's bytes safe in either case.
            if std::fs::rename(&slot0, &to).is_err() {
                std::fs::write(&to, &old).map_err(|e| format!("{}: {e}", to.display()))?;
                std::fs::remove_file(&slot0).map_err(|e| format!("{}: {e}", slot0.display()))?;
            }
            backup = Some(to);
        }
    }
    let data = std::fs::read(state).map_err(|e| format!("{}: {e}", state.display()))?;
    std::fs::write(&slot0, &data).map_err(|e| format!("{}: {e}", slot0.display()))?;
    std::fs::write(&marker, format!("{:016x}
", fnv1a(&data, FNV_BASIS))).map_err(|e| format!("{}: {e}", marker.display()))?;
    Ok(backup)
}

fn marker_of(slot0: &Path) -> PathBuf {
    let mut m = slot0.as_os_str().to_owned();
    m.push(".kh2rig");
    PathBuf::from(m)
}

const FNV_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

// --- the lock ----------------------------------------------------------------------------------------------------------------------

/// `local/pcsx2.lock` held for the life of the guard.
pub struct Lock {
    path: PathBuf,
    id: String,
}

impl Lock {
    /// Waits up to `wait` for a lock younger than two hours to go away; a stale one (older) is taken over.
    pub fn acquire(local: &Path, id: &str, wait: Duration) -> Result<Lock, String> {
        let path = local.join("pcsx2.lock");
        let t = Instant::now();
        loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    f.write_all(id.as_bytes()).map_err(|e| e.to_string())?;
                    return Ok(Lock { path, id: id.to_string() });
                }
                Err(_) => {
                    let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|m| SystemTime::now().duration_since(m).ok());
                    if age.is_none_or(|a| a >= LOCK_FRESH) {
                        // Stale (or unreadable): take it over.
                        std::fs::write(&path, id).map_err(|e| format!("{}: {e}", path.display()))?;
                        return Ok(Lock { path, id: id.to_string() });
                    }
                    if t.elapsed() >= wait {
                        let holder = std::fs::read_to_string(&path).unwrap_or_default();
                        return Err(format!("{} is held by '{}' (younger than 2 h); gave up after {} s", path.display(), holder.trim(), wait.as_secs()));
                    }
                    std::thread::sleep(Duration::from_secs(5));
                }
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // Only remove a lock that is still ours.
        if std::fs::read_to_string(&self.path).is_ok_and(|s| s.trim() == self.id) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

// --- a PCSX2 process ---------------------------------------------------------------------------------------------------------------

/// A started PCSX2; killed on drop.
pub struct Emulator {
    child: std::process::Child,
}

impl Emulator {
    pub fn start(cfg: &Config, state: &Path, unlimited: bool) -> Result<Emulator, String> {
        for (port, what) in [(DEBUG_PORT, "debug server"), (PINE_PORT, "PINE")] {
            if TcpStream::connect_timeout(&([127, 0, 0, 1], port).into(), Duration::from_millis(300)).is_ok() {
                return Err(format!("a PCSX2 is already running ({what} port {port} answers); the rig never shares an emulator"));
            }
        }
        let mut cmd = std::process::Command::new(&cfg.pcsx2);
        cmd.arg("-batch");
        if unlimited {
            cmd.arg("-unlimited");
        }
        cmd.arg("-statefile").arg(state).arg("--").arg(&cfg.iso);
        cmd.current_dir(cfg.pcsx2.parent().unwrap_or(Path::new("."))).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        let child = cmd.spawn().map_err(|e| format!("{}: {e}", cfg.pcsx2.display()))?;
        Ok(Emulator { child })
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn exited(&mut self) -> bool {
        self.child.try_wait().ok().flatten().is_some()
    }
}

impl Drop for Emulator {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn retry<T>(what: &str, timeout: Duration, emu: &mut Emulator, mut f: impl FnMut() -> Result<T, String>) -> Result<T, String> {
    let t = Instant::now();
    loop {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) if t.elapsed() >= timeout => return Err(format!("{what}: {e}")),
            Err(_) => {
                if emu.exited() {
                    return Err(format!("{what}: PCSX2 exited"));
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
}

// --- the run -----------------------------------------------------------------------------------------------------------------------

/// How the state is loaded relative to the breakpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attach {
    /// Load slot 0, wait until the vsync counter is back near the state's, then set the breakpoint (the first stop is a few vsyncs in).
    LoadFirst,
    /// Set the breakpoint, then load slot 0 (the first stop is the first pad read after the state).
    BreakFirst,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub unlimited: bool,
    pub attach: Attach,
    /// Read the whole EE RAM at the end and report a hash (6.8 s; the determinism self-test).
    pub ram_hash: bool,
    pub out: Option<PathBuf>,
    pub lock_wait: Duration,
    /// Skip the scenario's `[[shot]]` captures (`--no-shots`).
    pub no_shots: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { unlimited: false, attach: Attach::BreakFirst, ram_hash: false, out: None, lock_wait: Duration::from_secs(1800), no_shots: false }
    }
}

/// What a run produced.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub trace: Trace,
    /// The k of the first pad-1 stop at or after the state.
    pub first_k: u32,
    pub last_k: u32,
    pub stops: u32,
    pub seconds: f64,
    pub ram_hash: Option<u64>,
    pub state: PathBuf,
}

/// FNV-1a over the bytes.
pub fn fnv1a(b: &[u8], mut h: u64) -> u64 {
    for x in b {
        h ^= u64::from(*x);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Captures the client area of the emulator's window into `out` (`tools/pcsx2_shot.py --pid`): the raw picture, before `frames` crops
/// and scales it. The script is `KH2_PCSX2_SHOT`, else `tools/pcsx2_shot.py` found above the current directory or next to `local`; the
/// interpreter is `KH2_PYTHON` (default `python`).
pub fn capture_window(cfg: &Config, pid: u32, out: &Path) -> Result<(), String> {
    let script = std::env::var_os("KH2_PCSX2_SHOT")
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok().and_then(|c| c.ancestors().map(|d| d.join("tools/pcsx2_shot.py")).find(|p| p.is_file())))
        .or_else(|| cfg.local.parent().map(|r| r.join("tools/pcsx2_shot.py")).filter(|p| p.is_file()))
        .ok_or("tools/pcsx2_shot.py not found (set KH2_PCSX2_SHOT)")?;
    let python = std::env::var("KH2_PYTHON").unwrap_or_else(|_| "python".into());
    let res = std::process::Command::new(&python).arg(&script).arg(out).arg("--pid").arg(pid.to_string()).output().map_err(|e| format!("{python}: {e}"))?;
    if !res.status.success() || !out.is_file() {
        return Err(format!("window capture failed: {}{}", String::from_utf8_lossy(&res.stdout), String::from_utf8_lossy(&res.stderr)));
    }
    Ok(())
}

/// A checkpoint waiting for its stop.
struct Due {
    at: u32,
    names: Vec<String>,
}

/// The list of (checkpoint, names) a scenario asks for, in k order.
fn dues(sc: &Scenario) -> Vec<Due> {
    let mut by_k: std::collections::BTreeMap<u32, Vec<String>> = Default::default();
    for c in &sc.checks {
        for &k in &c.at {
            let e = by_k.entry(k).or_default();
            for v in &c.values {
                if !e.contains(v) {
                    e.push(v.clone());
                }
            }
        }
    }
    by_k.into_iter().map(|(at, names)| Due { at, names }).collect()
}

/// Runs `sc` on the original and returns the trace. `pad` is the parsed script (none = leave the pad alone).
pub fn run_original(cfg: &Config, sc: &Scenario, values: &Values, pad: Option<&PadScript>, opt: &Options) -> Result<Outcome, String> {
    sc.validate(values)?;
    let state_path = cfg.resolve_state(&sc.state)?;
    let state = P2s::open(&state_path)?;
    let t0 = Extract::read(&state.ee)?.vsync;
    drop(state);
    let mut due = dues(sc);
    // Screenshot checkpoints by k, taken at the first stop at or after `at` (the game is stopped there, the window shows the last frame).
    let mut shots: Vec<(u32, String)> = sc.shots.iter().filter(|_| !opt.no_shots).map(|s| (s.at, s.name.clone())).collect();
    shots.sort();
    let shot_dir = opt.out.clone().unwrap_or_else(|| cfg.local.join("rig").join(&sc.name));
    if !shots.is_empty() {
        std::fs::create_dir_all(&shot_dir).map_err(|e| format!("{}: {e}", shot_dir.display()))?;
    }

    let _lock = Lock::acquire(&cfg.local, &cfg.id, opt.lock_wait)?;
    install_slot0(cfg, &state_path)?;
    let mut emu = Emulator::start(cfg, &cfg.slot0(), opt.unlimited)?;
    let wait = Duration::from_secs(90);
    let mut dbg = retry("debug server", wait, &mut emu, || Debug::connect(DEBUG_PORT))?;
    let mut pine = retry("PINE", wait, &mut emu, || Pine::connect(PINE_PORT))?;
    // The game is running once the vsync counter answers (it is 0 before the BIOS has started the game).
    retry("game start", wait, &mut emu, || pine.r32(save::VSYNC).and_then(|v| if v == 0 { Err("vsync 0".into()) } else { Ok(()) }))?;

    let ticks = |p: &mut Pine| p.r32(save::VSYNC).map(|v| v.wrapping_sub(t0));
    match opt.attach {
        Attach::LoadFirst => {
            dbg.clear_breakpoints()?;
            pine.load_state(0)?;
            retry("state load", Duration::from_secs(60), &mut emu, || ticks(&mut pine).and_then(|k| if k < 1000 { Ok(()) } else { Err(format!("k = {k}")) }))?;
            dbg.set_breakpoint(PAD_READ_STOP)?;
        }
        Attach::BreakFirst => {
            dbg.clear_breakpoints()?;
            dbg.set_breakpoint(PAD_READ_STOP)?;
            pine.load_state(0)?;
        }
    }

    let started = Instant::now();
    let mut trace = Trace::default();
    let (mut first_k, mut stops) = (None, 0u32);
    let mut last_k: u32;
    let mut ram_hash = None;
    loop {
        dbg.wait_paused(Duration::from_secs(30))?;
        let (s0, sp) = dbg.s0_sp()?;
        if s0 != PAD1_RECORD {
            dbg.resume()?;
            continue;
        }
        let tick = dbg.u32(save::VSYNC)?;
        let k = tick.wrapping_sub(t0);
        if k >= 0x8000_0000 {
            // A stop from before the state took effect.
            dbg.resume()?;
            continue;
        }
        first_k.get_or_insert(k);
        stops += 1;
        last_k = k;
        if let Some(f) = pad.and_then(|p| p.at(k)) {
            dbg.write(sp, &f.bytes())?;
        }
        // Checkpoints whose k has been reached, then the periodic reads.
        let mut wanted: Vec<(u32, String)> = Vec::new();
        while due.first().is_some_and(|d| d.at <= k) {
            let d = due.remove(0);
            wanted.extend(d.names.into_iter().map(|n| (d.at, n)));
        }
        if sc.every > 0 && k % sc.every == 0 {
            for n in &sc.trace_values {
                if !wanted.iter().any(|(at, w)| *at == k && w == n) {
                    wanted.push((k, n.clone()));
                }
            }
        }
        for (at, name) in wanted {
            let v = values.get(&name)?.read(&mut |a, n| dbg.mem(a, n))?;
            trace.push(at, &name, v, k);
        }
        while shots.first().is_some_and(|(at, _)| *at <= k) {
            let (_, name) = shots.remove(0);
            capture_window(cfg, emu.pid(), &shot_dir.join(crate::frames::orig_shot_name(&name)))?;
        }
        if k >= sc.frames {
            if opt.ram_hash {
                let mut h = 0xcbf2_9ce4_8422_2325u64;
                for chunk in 0..(crate::p2s::EE_SIZE / 0x10000) {
                    h = fnv1a(&dbg.mem((chunk * 0x10000) as u32, 0x10000)?, h);
                }
                ram_hash = Some(h);
            }
            break;
        }
        dbg.resume()?;
    }
    let seconds = started.elapsed().as_secs_f64();
    let _ = dbg.clear_breakpoints();
    let _ = dbg.resume();
    if let Some((at, name)) = shots.first() {
        return Err(format!("screenshot '{name}' at {at} was never reached (the run ended at k = {last_k})"));
    }
    if let Some(d) = due.first() {
        return Err(format!("checkpoint {} was never reached (the run ended at k = {last_k})", d.at));
    }
    let first_k = first_k.ok_or("the game never reached the pad read")?;
    // Input scripted before the first stop never reached the game (the state's first pad read is a vsync or two in).
    if let Some(p) = pad
        && let Some(k) = (0..first_k).find(|k| p.at(*k).is_some_and(|f| f != crate::pad::Frame::default()))
    {
        return Err(format!("attached late: the first stop is at k = {first_k} but the pad script presses something at k = {k}; use --attach bp"));
    }
    Ok(Outcome { trace, first_k, last_k, stops, seconds, ram_hash, state: state_path })
}

/// `scenario.toml` -> (scenario, values, pad script).
pub fn load_scenario(path: &Path) -> Result<(Scenario, Values, Option<PadScript>), String> {
    let sc = Scenario::load(path)?;
    let values = Values::for_scenario(path)?;
    sc.validate(&values)?;
    let pad = match sc.pad_path() {
        Some(p) => {
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            Some(PadScript::parse(&text).map_err(|e| format!("{}: {e}", p.display()))?)
        }
        None => None,
    };
    Ok((sc, values, pad))
}

/// Writes `trace-orig.csv` and `meta.txt` into `dir`.
pub fn write_outcome(dir: &Path, sc: &Scenario, o: &Outcome) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(dir.join("trace-orig.csv"), o.trace.to_csv()).map_err(|e| e.to_string())?;
    let meta = format!(
        "scenario {}\nstate {}\nfirst stop k {}\nlast stop k {}\npad-1 stops {}\nseconds {:.2}\nstops per second {:.1}\n{}",
        sc.name,
        o.state.display(),
        o.first_k,
        o.last_k,
        o.stops,
        o.seconds,
        f64::from(o.stops) / o.seconds.max(1e-9),
        o.ram_hash.map_or(String::new(), |h| format!("ram hash {h:016x}\n"))
    );
    std::fs::write(dir.join("meta.txt"), meta).map_err(|e| e.to_string())
}

/// Two runs of the same scenario must give the same trace (and, with `ram_hash`, the same RAM).
pub fn determinism(cfg: &Config, sc: &Scenario, values: &Values, pad: Option<&PadScript>, opt: &Options) -> Result<Vec<String>, String> {
    let a = run_original(cfg, sc, values, pad, opt)?;
    let b = run_original(cfg, sc, values, pad, opt)?;
    let mut d = a.trace.diff(&b.trace);
    if a.first_k != b.first_k {
        d.push(format!("first stop k {} vs {}", a.first_k, b.first_k));
    }
    if a.ram_hash != b.ram_hash {
        d.push(format!("ram hash {:x?} vs {:x?}", a.ram_hash, b.ram_hash));
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_and_rejects_garbage() {
        assert_eq!(hex_decode(&hex_encode(&[0, 1, 0xab, 0xff])).unwrap(), vec![0, 1, 0xab, 0xff]);
        assert!(hex_decode("abc").is_err());
        assert!(hex_decode("zz").is_err());
    }

    #[test]
    fn fnv_is_the_standard_hash() {
        // FNV-1a 64 of "a" and of the empty string (published test vectors).
        assert_eq!(fnv1a(b"a", 0xcbf2_9ce4_8422_2325), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"", 0xcbf2_9ce4_8422_2325), 0xcbf2_9ce4_8422_2325);
    }

    #[test]
    fn checkpoints_merge_names_per_k() {
        let sc = Scenario::parse(
            "[scenario]\nname=\"a\"\nstate=\"06\"\nframes=10\n[[check]]\nat=[4,2]\nvalues=[\"x\"]\n[[check]]\nat=[2]\nvalues=[\"y\",\"x\"]\n",
            Path::new("."),
        )
        .unwrap();
        let d = dues(&sc);
        assert_eq!(d.iter().map(|d| d.at).collect::<Vec<_>>(), vec![2, 4]);
        assert_eq!(d[0].names, vec!["x", "y"]);
        assert_eq!(d[1].names, vec!["x"]);
    }

    #[test]
    fn state_names_resolve_to_slots_files_and_the_rig_folder() {
        let tmp = std::env::temp_dir().join(format!("kh2rig-cfg-{}", std::process::id()));
        let (local, ss) = (tmp.join("local"), tmp.join("sstates"));
        std::fs::create_dir_all(local.join("rig/states")).unwrap();
        std::fs::create_dir_all(&ss).unwrap();
        std::fs::write(ss.join(format!("{GAME_ID}.06.p2s")), b"x").unwrap();
        std::fs::write(local.join("rig/states/mine.p2s"), b"x").unwrap();
        let cfg = Config { local: local.clone(), pcsx2: PathBuf::new(), iso: PathBuf::new(), sstates: ss.clone(), id: "t".into() };
        assert_eq!(cfg.resolve_state("06").unwrap(), ss.join(format!("{GAME_ID}.06.p2s")));
        assert_eq!(cfg.resolve_state("6").unwrap(), ss.join(format!("{GAME_ID}.06.p2s")));
        assert_eq!(cfg.resolve_state("mine.p2s").unwrap(), local.join("rig/states/mine.p2s"));
        assert!(cfg.resolve_state("07").unwrap_err().contains("does not exist"));
        assert!(cfg.resolve_state("nope.p2s").unwrap_err().contains("not found"));
        assert_eq!(cfg.slot0(), ss.join(format!("{GAME_ID}.00.p2s")));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn slot_zero_keeps_the_users_state_and_replaces_the_rigs_own() {
        let tmp = std::env::temp_dir().join(format!("kh2rig-slot0-{}", std::process::id()));
        std::fs::remove_dir_all(&tmp).ok();
        let (local, ss) = (tmp.join("local"), tmp.join("sstates"));
        std::fs::create_dir_all(&local).unwrap();
        std::fs::create_dir_all(&ss).unwrap();
        let cfg = Config { local: local.clone(), pcsx2: PathBuf::new(), iso: PathBuf::new(), sstates: ss, id: "t".into() };
        let (a, b) = (tmp.join("a.p2s"), tmp.join("b.p2s"));
        std::fs::write(&a, b"state a").unwrap();
        std::fs::write(&b, b"state b").unwrap();
        // The user's own slot 0 (no marker) is backed up, not overwritten.
        std::fs::write(cfg.slot0(), b"users own state").unwrap();
        let bk = install_slot0(&cfg, &a).unwrap().expect("a backup is made");
        assert!(bk.starts_with(local.join("rig/backup")) && bk.to_string_lossy().ends_with(".00.p2s"));
        assert_eq!(std::fs::read(&bk).unwrap(), b"users own state");
        assert_eq!(std::fs::read(cfg.slot0()).unwrap(), b"state a");
        // The rig's own file (marker matches) is replaced without a backup.
        assert_eq!(install_slot0(&cfg, &b).unwrap(), None);
        assert_eq!(std::fs::read(cfg.slot0()).unwrap(), b"state b");
        // A user save over the rig's file (marker no longer matches) is backed up too, even within the same second.
        std::fs::write(cfg.slot0(), b"saved later").unwrap();
        let bk2 = install_slot0(&cfg, &a).unwrap().expect("second backup");
        assert_ne!(bk, bk2);
        assert_eq!(std::fs::read(&bk2).unwrap(), b"saved later");
        assert_eq!(std::fs::read_dir(local.join("rig/backup")).unwrap().count(), 2);
        // No slot 0 at all: just written.
        std::fs::remove_file(cfg.slot0()).unwrap();
        assert_eq!(install_slot0(&cfg, &b).unwrap(), None);
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn the_lock_is_exclusive_waits_and_is_released() {
        let tmp = std::env::temp_dir().join(format!("kh2rig-lock-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let a = Lock::acquire(&tmp, "A", Duration::from_secs(0)).unwrap();
        assert_eq!(std::fs::read_to_string(tmp.join("pcsx2.lock")).unwrap(), "A");
        let e = Lock::acquire(&tmp, "B", Duration::from_secs(0)).err().expect("a fresh lock is respected");
        assert!(e.contains("held by 'A'"), "{e}");
        drop(a);
        assert!(!tmp.join("pcsx2.lock").exists());
        let b = Lock::acquire(&tmp, "B", Duration::from_secs(0)).unwrap();
        drop(b);
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// A debug server that speaks the protocol on a local socket, to test the client's parsing without PCSX2.
    #[test]
    fn the_debug_client_parses_registers_memory_and_errors() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut w = s;
            let mut line = String::new();
            while r.read_line(&mut line).unwrap() > 0 {
                let req: Value = serde_json::from_str(&line).unwrap();
                let resp = match req["cmd"].as_str().unwrap() {
                    "status" => json!({"ok": true, "data": {"alive": true, "paused": true, "pc": "0x00167370"}}),
                    "read_registers" => json!({"ok": true, "data": {"GPR": {"regs": [
                        {"name": "s0", "value": "0x0000000000000000000000000035b4c0"},
                        {"name": "sp", "value": "0x00000000000000000000000001fff000"}]}, "pc": "0x00167370"}}),
                    "read_memory" => {
                        let n = req["length"].as_u64().unwrap() as usize;
                        json!({"ok": true, "hex": hex_encode(&(0..n).map(|i| i as u8).collect::<Vec<_>>())})
                    }
                    "write_memory" => json!({"ok": true, "written": req["data"].as_str().unwrap().len() / 2}),
                    _ => json!({"ok": false, "error": "Unknown command"}),
                };
                w.write_all(format!("{resp}\n").as_bytes()).unwrap();
                line.clear();
            }
        });
        let mut d = Debug::connect(port).unwrap();
        assert!(d.paused().unwrap());
        assert_eq!(d.s0_sp().unwrap(), (PAD1_RECORD, 0x01ff_f000));
        assert_eq!(d.mem(0, 5).unwrap(), vec![0, 1, 2, 3, 4]);
        // A read past 64 KiB is split and joined.
        assert_eq!(d.mem(0, 70_000).unwrap().len(), 70_000);
        assert_eq!(d.u32(0).unwrap(), 0x0302_0100);
        d.write(0x100, &[1, 2, 3]).unwrap();
        assert!(d.call(json!({"cmd": "bogus"})).unwrap_err().contains("Unknown command"));
        drop(d);
        h.join().unwrap();
    }
}
