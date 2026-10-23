//! The pad: the `dbcman`/`ds2o` servers the pad library (libdbc) binds, and the replacement of
//! the libpad2 read (boot contract 2.4 and 5).
//!
//! The servers only have to make the library's setup succeed; the data does not travel through
//! them. The game reads a port through 0x2fd2b0(port, buffer) [V: 0x167310]: the buffer gets 18
//! bytes of the DS2 report, buttons first, active low (bytes 0 and 1), then the two sticks
//! (bytes 2 to 5: right x, right y, left x, left y, centre 0x7f), then 12 pressure bytes [SDK
//! libpad layout; the game reads bytes 0 to 5 only]. That function is hooked and fills the buffer
//! from a [`PadScript`]. The library's own port-state (0x2fd458; 1 = a pad is connected) and
//! description (0x2fd388) code reads buffers the IOP would write by DMA, so both are hooked to
//! report a connected DualShock 2 with analog sticks [inferred from their callers]; the calls they would make to `dbcman`
//! (functions 0x80001303 and 0x8000131a) therefore never happen.

use kh2_ee::{Ee, reg};

use super::{Handle, Mem, Request, Server};

pub const DBCMAN: u32 = 0x8000_1300;
pub const DBCMAN_1C: u32 = 0x8000_131c;
pub const DBCMAN_PORT0: u32 = 0x8000_131e;
pub const DBCMAN_PORT1: u32 = 0x8000_131f;

/// libdbc functions (the function id is the server id plus a number).
pub const FNO_SET_UP: u32 = 0x8000_1301;
pub const FNO_PORT_STATE: u32 = 0x8000_1303;
pub const FNO_INIT: u32 = 0x8000_1304;
pub const FNO_GET_DATA: u32 = 0x8000_131a;
pub const FNO_VERSION: u32 = 0x8000_1363;
/// The module version libdbc requires (high bits `0x31`, [V: 0x2fbeb0 compares `version >> 4` with 0x31]).
pub const DBC_VERSION: u32 = 0x310;

/// The libpad2 read the game calls per port [V: 0x167310], the port state and the pad description read.
pub const PAD_READ: u32 = 0x2fd2b0;
pub const PAD_STATE: u32 = 0x2fd458;
pub const PAD_INFO: u32 = 0x2fd388;
/// The button profile of a DualShock 2, as the description read copies it: every button, both
/// sticks and every pressure byte present [inferred: libpad2's button profile, one bit per
/// report field]. The game tests byte 2's low nibble (the four stick axes) for 0xf before it reads
/// the sticks at all [V: 0x167310 checks pad +0x52, the copy at +0x50 made by 0x2fd388]; with
/// nothing written the pad counts as digital and the sticks are ignored.
pub const DS2_PROFILE: [u8; 4] = [0xff, 0xff, 0xff, 0xff];
/// Size of the report written by the read hook.
pub const REPORT_LEN: usize = 18;

/// The pad library's server. Each set-up call (function 0x80001301, one per pad) creates a socket
/// and answers its number, 0 for the first pad and 1 for the second [inferred: the game stores the
/// answer as the port it reads, 0x167148].
#[derive(Default)]
pub struct Dbcman {
    sockets: u32,
}

impl Server for Dbcman {
    fn ids(&self) -> &[u32] {
        &[DBCMAN, DBCMAN_1C, DBCMAN_PORT0, DBCMAN_PORT1]
    }

    fn call(&mut self, _: &mut dyn Mem, req: &Request) -> Option<Vec<u8>> {
        let socket = self.sockets;
        // The reply goes to the block the call sent from (0x90 bytes), so the reply starts as the request.
        let mut r = req.send.to_vec();
        r.resize(req.recv_len.max(0x90), 0);
        let set = |r: &mut Vec<u8>, w: usize, v: u32| r[4 * w..4 * w + 4].copy_from_slice(&v.to_le_bytes());
        match (req.sid, req.fno) {
            (DBCMAN, FNO_VERSION) => set(&mut r, 0, DBC_VERSION),
            (DBCMAN, FNO_INIT) => set(&mut r, 0, 1),
            (DBCMAN, FNO_SET_UP) => {
                set(&mut r, 9, socket);
                self.sockets += 1;
            }
            (DBCMAN, FNO_PORT_STATE) => set(&mut r, 1, 0),
            // Get-data: one byte of state (1 = a pad is connected) at +0xc, its length at +8, result 0 at +0x8c [inferred from 0x2fcf58 and 0x2fd458].
            (DBCMAN, FNO_GET_DATA) => {
                set(&mut r, 2, 1);
                set(&mut r, 3, 1);
                set(&mut r, 0x8c / 4, 0);
            }
            // The per-port servers: send data, function 0 for the first port, 1 for the second [V: 0x2fcc50].
            (DBCMAN_PORT0, 0) | (DBCMAN_PORT1, 1) | (DBCMAN_1C, _) => {}
            _ => return None,
        }
        Some(r)
    }
}

/// Buttons of the DS2, as bits of the pressed mask (the report holds the complement) [SDK libpad].
pub mod button {
    pub const SELECT: u16 = 1 << 0;
    pub const L3: u16 = 1 << 1;
    pub const R3: u16 = 1 << 2;
    pub const START: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const RIGHT: u16 = 1 << 5;
    pub const DOWN: u16 = 1 << 6;
    pub const LEFT: u16 = 1 << 7;
    pub const L2: u16 = 1 << 8;
    pub const R2: u16 = 1 << 9;
    pub const L1: u16 = 1 << 10;
    pub const R1: u16 = 1 << 11;
    pub const TRIANGLE: u16 = 1 << 12;
    pub const CIRCLE: u16 = 1 << 13;
    pub const CROSS: u16 = 1 << 14;
    pub const SQUARE: u16 = 1 << 15;
    pub const NAMES: [(&str, u16); 16] = [
        ("select", SELECT),
        ("l3", L3),
        ("r3", R3),
        ("start", START),
        ("up", UP),
        ("right", RIGHT),
        ("down", DOWN),
        ("left", LEFT),
        ("l2", L2),
        ("r2", R2),
        ("l1", L1),
        ("r1", R1),
        ("triangle", TRIANGLE),
        ("circle", CIRCLE),
        ("cross", CROSS),
        ("square", SQUARE),
    ];
}

/// One pad state: the pressed buttons and the four stick bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadFrame {
    pub buttons: u16,
    pub lx: u8,
    pub ly: u8,
    pub rx: u8,
    pub ry: u8,
}

impl Default for PadFrame {
    fn default() -> Self {
        PadFrame { buttons: 0, lx: 0x7f, ly: 0x7f, rx: 0x7f, ry: 0x7f }
    }
}

impl PadFrame {
    /// The 18-byte report: buttons active low, sticks, pressure bytes (0xff for a pressed button).
    pub fn report(&self) -> [u8; REPORT_LEN] {
        let mut r = [0u8; REPORT_LEN];
        let word = !self.buttons;
        r[0] = word as u8;
        r[1] = (word >> 8) as u8;
        r[2] = self.rx;
        r[3] = self.ry;
        r[4] = self.lx;
        r[5] = self.ly;
        // Pressure order of libpad: right, left, up, down, triangle, circle, cross, square, l1, r1, l2, r2.
        let order = [
            button::RIGHT,
            button::LEFT,
            button::UP,
            button::DOWN,
            button::TRIANGLE,
            button::CIRCLE,
            button::CROSS,
            button::SQUARE,
            button::L1,
            button::R1,
            button::L2,
            button::R2,
        ];
        for (i, b) in order.iter().enumerate() {
            r[6 + i] = if self.buttons & b != 0 { 0xff } else { 0 };
        }
        r
    }
}

/// The pad of port 0 over time: (vsync, state) lines, each held until the next [RIG.md 2.2 format].
#[derive(Clone, Debug, Default)]
pub struct PadScript {
    lines: Vec<(u64, PadFrame)>,
    /// The vsync the script ends at (`end N`), if given.
    pub end: Option<u64>,
}

impl PadScript {
    /// A script that holds one state forever.
    pub fn constant(f: PadFrame) -> PadScript {
        PadScript { lines: vec![(0, f)], end: None }
    }

    /// Parses `k buttons lx ly rx ry` lines (`#` comments, `-` for no buttons, buttons joined by
    /// `+`, `end N`).
    pub fn parse(text: &str) -> Result<PadScript, String> {
        let mut s = PadScript::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let t: Vec<&str> = line.split_whitespace().collect();
            if t[0] == "end" {
                s.end = Some(t.get(1).and_then(|v| v.parse().ok()).ok_or(format!("line {}: end needs a number", n + 1))?);
                continue;
            }
            if t.len() != 6 {
                return Err(format!("line {}: expected 6 fields", n + 1));
            }
            let k: u64 = t[0].parse().map_err(|_| format!("line {}: bad time", n + 1))?;
            let mut buttons = 0u16;
            if t[1] != "-" {
                for name in t[1].split('+') {
                    let b = button::NAMES.iter().find(|(nm, _)| *nm == name).ok_or(format!("line {}: unknown button {name}", n + 1))?;
                    buttons |= b.1;
                }
            }
            let stick = |i: usize| t[i].parse::<u8>().map_err(|_| format!("line {}: bad stick value", n + 1));
            s.lines.push((k, PadFrame { buttons, lx: stick(2)?, ly: stick(3)?, rx: stick(4)?, ry: stick(5)? }));
        }
        s.lines.sort_by_key(|l| l.0);
        Ok(s)
    }

    /// The state at vsync `k`: the last line at or before it, or the neutral pad.
    pub fn at(&self, k: u64) -> PadFrame {
        self.lines.iter().rev().find(|l| l.0 <= k).map_or_else(PadFrame::default, |l| l.1)
    }
}

pub(super) fn install(ee: &mut Ee, iop: &Handle) {
    let h = iop.clone();
    ee.hooks.insert(PAD_READ, move |ee: &mut Ee| {
        let ra = ee.gpr[reg::RA] as u32;
        ee.jump(ra);
        let (port, buf) = (ee.gpr[reg::A0] as u32, ee.gpr[reg::A0 + 1] as u32);
        let mut iop = h.borrow_mut();
        iop.tick(ee);
        let vsync = iop.vsync(ee.steps);
        let frame = if port == 0 { iop.pad.at(vsync) } else { PadFrame::default() };
        *iop.pad_reads.entry(port).or_default() += 1;
        ee.write(buf, &frame.report());
        ee.gpr[reg::V0] = REPORT_LEN as u128;
    });
    ee.hooks.insert(PAD_STATE, |ee: &mut Ee| {
        let ra = ee.gpr[reg::RA] as u32;
        ee.jump(ra);
        ee.gpr[reg::V0] = 1;
    });
    ee.hooks.insert(PAD_INFO, |ee: &mut Ee| {
        let ra = ee.gpr[reg::RA] as u32;
        ee.jump(ra);
        let buf = ee.gpr[reg::A0 + 1] as u32;
        ee.write(buf, &DS2_PROFILE);
        ee.gpr[reg::V0] = DS2_PROFILE.len() as u128;
    });
}
