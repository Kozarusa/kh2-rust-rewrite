//! Oracle tests of the card driver's per-type handlers and the dispatcher (WP-L1-P27a): `FUN_00231f90`, `FUN_00232228`,
//! `FUN_002323c8`, `FUN_00232558`, `FUN_00232668`, `FUN_00232778`, `FUN_00232880`, `FUN_00232988`, `FUN_00232b70`, `FUN_00232c80`,
//! `FUN_00232e58`, `FUN_00233040`, `FUN_00233140`, `FUN_00233178` and the wait `FUN_00230f48`.
//!
//! Until now these were covered only by hand tests with a scripted library (`tests/handlers.rs`) because the factory's stubs return 0.
//! Here kh2-ee runs the original with the IOP card library replaced by hooks that answer from a script: the poll `FUN_00303c90`
//! pops the next scripted answer (writes it to the result word and returns 1, or returns 0 for "pending"), the slot count and the
//! card info are functions of the port, slot and a per-case salt, and every other library call only logs its arguments. The port runs
//! the same script through a [`CardLib`] that logs the same events. After every call the driver memory (the core words and the card
//! span, byte for byte), the event log and the completion are compared.
//!
//! The completion function of a request is a hook at [`HDONE`] (logs its argument, answers a scripted value as the `call` request
//! does); the vertical-sync wait `FUN_002f8ec8` of the wait loop logs `frame`. Everything else, including the request end
//! `FUN_00231a18` and the idle handler `FUN_00231a60`, runs for real. Request types above 13 index past the handler table in the
//! original and are not drawn.
//!
//! Skips without a disc; fails without one when `KH2_REQUIRE_DISC=1`.

mod common;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use common::*;
use kh2_ee::Ee;
use kh2_save::handlers::{CardLib, Completion, InfoOut};
use kh2_save::ring::{CARD_BASE, CARD_LEN, CORE_BASE, CORE_LEN, Driver};

/// The completion function (a hook; nothing is executed there).
const HDONE: u32 = 0x0171_0000;
const POLL: u32 = 0x303c90;
const FRAME: u32 = 0x2f8ec8;
/// Result words the handlers' maps tell apart, plus out-of-range values.
const CODES: [i32; 16] = [0, 1, -1, -2, -3, -4, -5, -6, -7, -8, 3, 2, 100, -100, i32::MIN, i32::MAX];

type Events = Rc<RefCell<Vec<String>>>;
type Script = Rc<RefCell<VecDeque<Option<i32>>>>;

/// What the case scripts: poll answers, the salt that makes the slot counts and card infos, the answer of the completion function.
#[derive(Clone)]
struct Plan {
    polls: VecDeque<Option<i32>>,
    salt: u32,
    call_result: i64,
}

fn slots_of(salt: u32, port: u32) -> u32 {
    (salt.wrapping_mul(7).wrapping_add(port.wrapping_mul(3))) % 5
}

fn info_of(salt: u32, port: u32, slot: u32) -> InfoOut {
    InfoOut { card_type: (salt + port + 2 * slot) % 5, free: salt.wrapping_mul(0x9e37_79b9) ^ (port << 8 | slot), format: (salt >> 1) % 3 }
}

fn fmt(name: &str, a: &[u32]) -> String {
    let mut s = name.to_string();
    for v in a {
        s.push_str(&format!(" {v:x}"));
    }
    s
}

/// The port's library: logs the same events as the hooks.
struct Lib {
    ev: Events,
    plan: Plan,
}

impl Lib {
    fn log(&self, name: &str, a: &[u32]) {
        self.ev.borrow_mut().push(fmt(name, a));
    }
}

impl CardLib for Lib {
    fn slot_count(&mut self, port: u32) -> u32 {
        self.log("slots", &[port]);
        slots_of(self.plan.salt, port)
    }
    fn get_info(&mut self, port: u32, slot: u32) -> InfoOut {
        self.log("info", &[port, slot]);
        info_of(self.plan.salt, port, slot)
    }
    fn list(&mut self, port: u32, slot: u32, pattern: u32, max: u32, table: u32) {
        self.log("list", &[port, slot, pattern, 0, max, table]);
    }
    fn format(&mut self, port: u32, slot: u32) {
        self.log("format", &[port, slot]);
    }
    fn unformat(&mut self, port: u32, slot: u32) {
        self.log("unformat", &[port, slot]);
    }
    fn chdir(&mut self, port: u32, slot: u32, path: u32) {
        self.log("chdir", &[port, slot, path, 0]);
    }
    fn mkdir(&mut self, port: u32, slot: u32, path: u32) {
        self.log("mkdir", &[port, slot, path]);
    }
    fn open(&mut self, port: u32, slot: u32, name: u32, mode: u32) {
        self.log("open", &[port, slot, name, mode]);
    }
    fn read(&mut self, fd: u32, buffer: u32, len: u32) {
        self.log("read", &[fd, buffer, len]);
    }
    fn write(&mut self, fd: u32, buffer: u32, len: u32) {
        self.log("write", &[fd, buffer, len]);
    }
    fn close(&mut self, fd: u32) {
        self.log("close", &[fd]);
    }
    fn delete(&mut self, port: u32, slot: u32, name: u32) {
        self.log("delete", &[port, slot, name]);
    }
    fn set_info(&mut self, port: u32, slot: u32, a: u32, b: u32) {
        self.log("set_info", &[port, slot, a, b]);
    }
    fn poll(&mut self) -> Option<i32> {
        // The script is exhausted into "done with 0" so a loop cannot wait forever.
        let r = self.plan.polls.pop_front().unwrap_or(Some(0));
        self.log("poll", &[r.map_or(0xdead_0000, |v| v as u32)]);
        r
    }
    fn call(&mut self, function: u32, id: u32) -> i64 {
        assert_eq!(function, HDONE);
        self.log("done", &[id]);
        self.plan.call_result
    }
}

/// The original with the library hooked to the same plan.
fn hook_library(ee: &mut Ee, ev: &Events, plan: &Plan) {
    let polls: Script = Rc::new(RefCell::new(plan.polls.clone()));
    let salt = plan.salt;
    let result = plan.call_result;
    let (e, p) = (ev.clone(), polls);
    ee.hooks.insert(POLL, move |ee: &mut Ee| {
        let r = p.borrow_mut().pop_front().unwrap_or(Some(0));
        e.borrow_mut().push(fmt("poll", &[r.map_or(0xdead_0000, |v| v as u32)]));
        // The call is `poll(1, 0, &result)`: the result word is written only when the request is done.
        match r {
            Some(v) => {
                let at = ee.gpr[6] as u32;
                ee.w32(at, v as u32);
                ee.gpr[2] = 1;
            }
            None => ee.gpr[2] = 0,
        }
    });
    let log_args = |ee: &mut Ee, e: &Events, name: &str, n: usize| {
        let a: Vec<u32> = (0..n).map(|i| ee.gpr[4 + i] as u32).collect();
        e.borrow_mut().push(fmt(name, &a));
        ee.gpr[2] = 0;
    };
    for (addr, name, n) in [
        (0x303fa0u32, "list", 6usize),
        (0x3042c0, "format", 2),
        (0x304898, "unformat", 2),
        (0x304180, "chdir", 4),
        (0x303778, "mkdir", 3),
        (0x303650, "open", 4),
        (0x303a00, "read", 3),
        (0x303b18, "write", 3),
        (0x3037b0, "close", 1),
        (0x304390, "delete", 3),
        (0x304740, "set_info", 4),
    ] {
        let e = ev.clone();
        ee.hooks.insert(addr, move |ee: &mut Ee| log_args(ee, &e, name, n));
    }
    let e = ev.clone();
    ee.hooks.insert(0x303590, move |ee: &mut Ee| {
        let port = ee.gpr[4] as u32;
        e.borrow_mut().push(fmt("slots", &[port]));
        ee.gpr[2] = u128::from(slots_of(salt, port));
    });
    let e = ev.clone();
    ee.hooks.insert(0x303db0, move |ee: &mut Ee| {
        let (port, slot) = (ee.gpr[4] as u32, ee.gpr[5] as u32);
        e.borrow_mut().push(fmt("info", &[port, slot]));
        let out = info_of(salt, port, slot);
        // The three outputs go to the status entry of the port and slot: type, free space, format word.
        let (t, f, w) = (ee.gpr[6] as u32, ee.gpr[7] as u32, ee.gpr[8] as u32);
        ee.w32(t, out.card_type);
        ee.w32(f, out.free);
        ee.w32(w, out.format);
        ee.gpr[2] = 0;
    });
    let e = ev.clone();
    ee.hooks.insert(HDONE, move |ee: &mut Ee| {
        e.borrow_mut().push(fmt("done", &[ee.gpr[4] as u32]));
        ee.gpr[2] = result as u64 as u128;
    });
    let e = ev.clone();
    ee.hooks.insert(FRAME, move |_: &mut Ee| e.borrow_mut().push("frame".into()));
}

fn put(o: &mut Orig, d: &Driver) {
    o.put(CORE_BASE, &d.core);
    o.put(CARD_BASE, &d.card);
}

fn same(o: &Orig, d: &Driver, what: &str) {
    assert_eq!(o.get(CORE_BASE, CORE_LEN), d.core, "{what}: core words");
    let game = o.get(CARD_BASE, CARD_LEN);
    if let Some(i) = (0..CARD_LEN).find(|&i| game[i] != d.card[i]) {
        panic!("{what}: card span differs first at {:#x}: game {:#x} port {:#x}", CARD_BASE as usize + i, game[i], d.card[i]);
    }
}

fn plan(o: &mut Orig, length: usize) -> Plan {
    let polls = (0..length)
        .map(|_| match o.rng.below(5) {
            0 => None,
            _ => Some(CODES[o.rng.below(CODES.len() as u64) as usize]),
        })
        .collect();
    Plan { polls, salt: o.rng.word() as u32, call_result: if o.rng.below(2) == 0 { 0 } else { o.rng.word() as i64 } }
}

/// A driver in the middle of a request of `kind`: random memory with the running words set to what the handler reads.
fn running(o: &mut Orig, kind: u32) -> Driver {
    let mut d = Driver { core: o.random(CORE_LEN).try_into().unwrap(), card: o.random(CARD_LEN) };
    d.set_word(0x36c268, kind);
    // The step: all the machine's steps (0..=8) and a few past the end.
    let step = o.rng.below(10) as u32;
    d.set_word(0x36c26c, step);
    // Port 0 or 1 and slot 0 to 3 so the status entry lies inside the card span.
    d.set_word(0x1dc0bc0, o.rng.below(2) as u32);
    d.set_word(0x1dc0bc4, o.rng.below(4) as u32);
    d.set_word(0x1dc0b48, o.rng.below(4) as u32);
    d.set_word(0x1dc0b4c, o.rng.below(3) as u32);
    d.set_word(0x1dc0b80, o.rng.below(2) as u32);
    // The result word: mostly a library code, sometimes random bytes.
    let res = if o.rng.below(6) == 0 { o.rng.word() as u32 } else { CODES[o.rng.below(CODES.len() as u64) as usize] as u32 };
    d.set_word(0x1dc0bb8, res);
    // A format wait counter of 0 or small, so the countdown ends in a few calls.
    if o.rng.below(2) == 0 {
        d.set_word(0x1dc0b4c, 0x3c * o.rng.below(2) as u32);
    }
    // The completion function: the hook (or none, except for the call request, which calls it).
    let done = if kind == 13 || o.rng.below(2) == 0 { HDONE } else { 0 };
    d.set_word(0x1dc0b64, done);
    d
}

type Handler = fn(&mut Driver, &mut dyn CardLib) -> Completion;

/// Runs `addr` (the original handler) against `port` over random running requests of `kind`, a few consecutive calls per case.
fn handler_case(addr: u32, kind: u32, port: Handler) {
    let Some(mut o) = Orig::new(&format!("oracle test of {addr:#x}"), u64::from(addr)) else { return };
    let (mut done, mut pending) = (0u32, 0u32);
    for n in 0..1500 {
        let mut d = running(&mut o, kind);
        put(&mut o, &d);
        let pl = plan(&mut o, 8);
        let (ev_o, ev_p): (Events, Events) = (Rc::default(), Rc::default());
        hook_library(&mut o.ee, &ev_o, &pl);
        let mut lib = Lib { ev: ev_p.clone(), plan: pl };
        for call in 0..5 {
            o.call(addr, &[]);
            let completion = port(&mut d, &mut lib);
            // The original calls the completion inside the end; the port returns it: turn it into the same event.
            if let Some((f, id)) = completion {
                assert_eq!(f, HDONE, "case {n} call {call}");
                ev_p.borrow_mut().push(fmt("done", &[id]));
                done += 1;
            } else {
                pending += 1;
            }
            let what = format!("case {n} call {call}");
            same(&o, &d, &what);
            assert_eq!(*ev_o.borrow(), *ev_p.borrow(), "{what}: library events");
        }
    }
    assert!(done > 20 && (pending > 20 || kind == 13), "{done} completions, {pending} calls that stay running");
}

#[test]
fn orig_231f90_matches_the_game() {
    handler_case(0x231f90, 1, Driver::handle_info);
}

#[test]
fn orig_232228_matches_the_game() {
    handler_case(0x232228, 2, Driver::handle_list);
}

#[test]
fn orig_2323c8_matches_the_game() {
    handler_case(0x2323c8, 3, Driver::handle_list_b);
}

#[test]
fn orig_232558_matches_the_game() {
    handler_case(0x232558, 4, Driver::handle_format);
}

#[test]
fn orig_232668_matches_the_game() {
    handler_case(0x232668, 5, Driver::handle_unformat);
}

#[test]
fn orig_232778_matches_the_game() {
    handler_case(0x232778, 6, Driver::handle_chdir);
}

#[test]
fn orig_232880_matches_the_game() {
    handler_case(0x232880, 7, Driver::handle_mkdir);
}

#[test]
fn orig_232988_matches_the_game() {
    handler_case(0x232988, 8, Driver::handle_create);
}

#[test]
fn orig_232b70_matches_the_game() {
    handler_case(0x232b70, 9, Driver::handle_delete);
}

#[test]
fn orig_232c80_matches_the_game() {
    handler_case(0x232c80, 10, Driver::handle_read);
}

#[test]
fn orig_232e58_matches_the_game() {
    handler_case(0x232e58, 11, Driver::handle_write);
}

#[test]
fn orig_233040_matches_the_game() {
    handler_case(0x233040, 12, Driver::handle_set_info);
}

#[test]
fn orig_233140_matches_the_game() {
    handler_case(0x233140, 13, Driver::handle_call);
}

/// The dispatcher: the running request's type picks the handler (types 0 to 13), whichever the state.
#[test]
fn orig_233178_matches_the_game() {
    let Some(mut o) = Orig::new("oracle test of 0x233178", 0x233178) else { return };
    let mut kinds = [0u32; 14];
    for n in 0..4000 {
        let kind = o.rng.below(14) as u32;
        let mut d = running(&mut o, kind);
        if kind == 0 {
            // The idle handler: an empty or a non-empty ring with the idle poll off, so nothing waits on the card library.
            d.card[(0x1dc0b68 - CARD_BASE) as usize] = 0;
            d.set_word(0x36c258, o.rng.below(64) as u32);
            d.set_word(0x36c25c, o.rng.below(64) as u32);
            for i in 0..64u32 {
                d.set_word(CARD_BASE + i * 0x20, o.rng.below(14) as u32);
            }
        }
        kinds[kind as usize] += 1;
        put(&mut o, &d);
        let pl = plan(&mut o, 4);
        let (ev_o, ev_p): (Events, Events) = (Rc::default(), Rc::default());
        hook_library(&mut o.ee, &ev_o, &pl);
        let mut lib = Lib { ev: ev_p.clone(), plan: pl };
        o.call(0x233178, &[]);
        if let Some((_, id)) = d.step(&mut lib) {
            ev_p.borrow_mut().push(fmt("done", &[id]));
        }
        same(&o, &d, &format!("case {n} kind {kind}"));
        assert_eq!(*ev_o.borrow(), *ev_p.borrow(), "case {n} kind {kind}: library events");
    }
    assert!(kinds.iter().all(|&c| c > 100), "every type drawn: {kinds:?}");
}

/// The wait: a driver with one to three queued requests of any type, waiting for everything (id 0) or for one id.
#[test]
fn orig_230f48_matches_the_game() {
    let Some(mut o) = Orig::new("oracle test of 0x230f48", 0x230f48) else { return };
    let (mut all, mut one, mut frames) = (0u32, 0u32, 0u32);
    for n in 0..800 {
        let mut d = Driver::default();
        d.set_word(0x1dc0cd0, 1);
        d.set_word(0x1dc0cd4, 1);
        // Ids start at 10: id 0 means "everything" to the busy query and the wait.
        d.set_word(0x36c260, 10);
        let count = 1 + o.rng.below(3);
        let mut ids = Vec::new();
        for _ in 0..count {
            let (port, name, buf, len) = (o.rng.below(2) as u32, 0x1000 + o.rng.below(16) as u32, 0x2000, 0x40);
            let done = if o.rng.below(2) == 0 { HDONE } else { 0 };
            ids.push(match o.rng.below(8) {
                0 => d.enqueue_info_slot0(port, done),
                1 => d.enqueue_delete_slot0(port, name, done),
                2 => d.enqueue_mkdir_slot0(port, name, done),
                3 => d.enqueue_chdir_slot0(port, name, done),
                4 => d.enqueue_format_slot0(port, done),
                5 => d.enqueue_read_slot0(port, name, buf, len, done),
                6 => d.enqueue_write_slot0(port, name, buf, len, done),
                _ => d.enqueue_create_slot0(port, name, buf, len, done),
            });
        }
        let id = if o.rng.below(3) == 0 { 0 } else { ids[o.rng.below(ids.len() as u64) as usize] };
        put(&mut o, &d);
        let pl = plan(&mut o, 30);
        let (ev_o, ev_p): (Events, Events) = (Rc::default(), Rc::default());
        hook_library(&mut o.ee, &ev_o, &pl);
        let mut lib = Lib { ev: ev_p.clone(), plan: pl };
        o.call(0x230f48, &[id as u64]);
        let (e1, e2) = (ev_p.clone(), ev_p.clone());
        d.wait(id, &mut lib, move || e1.borrow_mut().push("frame".into()), move |_, i| e2.borrow_mut().push(fmt("done", &[i])));
        same(&o, &d, &format!("case {n} id {id}"));
        assert_eq!(*ev_o.borrow(), *ev_p.borrow(), "case {n} id {id}: events");
        frames += ev_p.borrow().iter().filter(|e| *e == "frame").count() as u32;
        if id == 0 {
            all += 1;
        } else {
            one += 1;
        }
    }
    assert!(all > 100 && one > 100 && frames > 1000, "{all} waits for all, {one} for one id, {frames} frames");
}
