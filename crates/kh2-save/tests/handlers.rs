//! Hand tests of the card driver's per-type handlers with a scripted card library. The oracle cannot run these (they call the IOP card
//! library, and the factory's stubs return 0), so each handler is driven through its steps with scripted library results: one case per
//! result code of the table in `docs/systems/core-runtime/memcard.md` section 1, plus the step sequences (the call made at step 0, the
//! poll at step 1, the retry of format, the descriptor kept by the file machine). Not `verified` by the ledger: `tested`.

use std::collections::VecDeque;

use kh2_save::handlers::{CardLib, Completion, InfoOut};
use kh2_save::ring::{Driver, RESULTS, STATUS};

const STEP: u32 = 0x36c26c;
const KIND: u32 = 0x36c268;
const RES: u32 = 0x1dc0bb8;

/// A scripted library: records every call, answers polls from a queue.
#[derive(Default)]
struct Lib {
    polls: VecDeque<Option<i32>>,
    calls: Vec<String>,
    info: InfoOut,
    slots: [u32; 2],
    call_result: i64,
}

impl CardLib for Lib {
    fn slot_count(&mut self, port: u32) -> u32 {
        self.calls.push(format!("slots {port}"));
        self.slots[port as usize]
    }
    fn get_info(&mut self, port: u32, slot: u32) -> InfoOut {
        self.calls.push(format!("info {port} {slot}"));
        self.info
    }
    fn list(&mut self, port: u32, slot: u32, pattern: u32, max: u32, table: u32) {
        self.calls.push(format!("list {port} {slot} {pattern:#x} {max} {table:#x}"));
    }
    fn format(&mut self, port: u32, slot: u32) {
        self.calls.push(format!("format {port} {slot}"));
    }
    fn unformat(&mut self, port: u32, slot: u32) {
        self.calls.push(format!("unformat {port} {slot}"));
    }
    fn chdir(&mut self, port: u32, slot: u32, path: u32) {
        self.calls.push(format!("chdir {port} {slot} {path:#x}"));
    }
    fn mkdir(&mut self, port: u32, slot: u32, path: u32) {
        self.calls.push(format!("mkdir {port} {slot} {path:#x}"));
    }
    fn open(&mut self, port: u32, slot: u32, name: u32, mode: u32) {
        self.calls.push(format!("open {port} {slot} {name:#x} {mode:#x}"));
    }
    fn read(&mut self, fd: u32, buffer: u32, len: u32) {
        self.calls.push(format!("read {fd} {buffer:#x} {len:#x}"));
    }
    fn write(&mut self, fd: u32, buffer: u32, len: u32) {
        self.calls.push(format!("write {fd} {buffer:#x} {len:#x}"));
    }
    fn close(&mut self, fd: u32) {
        self.calls.push(format!("close {fd}"));
    }
    fn delete(&mut self, port: u32, slot: u32, name: u32) {
        self.calls.push(format!("delete {port} {slot} {name:#x}"));
    }
    fn set_info(&mut self, port: u32, slot: u32, a: u32, b: u32) {
        self.calls.push(format!("set_info {port} {slot} {a:#x} {b:#x}"));
    }
    fn poll(&mut self) -> Option<i32> {
        self.polls.pop_front().unwrap_or(None)
    }
    fn call(&mut self, function: u32, id: u32) -> i64 {
        self.calls.push(format!("call {function:#x} {id}"));
        self.call_result
    }
}

/// A driver with request type `kind` running on port 1, slot 2, with the arguments words set.
fn running(kind: u32) -> Driver {
    let mut d = Driver::default();
    d.set_word(KIND, kind);
    d.set_word(0x36c264, 77);
    d.set_word(0x1dc0bc0, 1);
    d.set_word(0x1dc0bc4, 2);
    d.set_word(0x1dc0b50, 0x1000);
    d.set_word(0x1dc0b54, 0x2000);
    d.set_word(0x1dc0b58, 0x3000);
    d.set_word(0x1dc0b5c, 0x400);
    d.set_word(0x1dc0b60, 99);
    d.set_word(0x1dc0bb4, 0x5000);
    d.set_word(0x1dc0b64, 0x4444);
    d
}

fn result(d: &Driver, k: u32) -> u32 {
    d.result(k)
}

/// Drives a handler from step 0 to its end with the library answering `res` to the first poll; returns the calls and the completion.
fn run_simple(kind: u32, handler: fn(&mut Driver, &mut dyn CardLib) -> Completion, res: i32) -> (Driver, Lib, Completion) {
    let mut d = running(kind);
    let mut lib = Lib::default();
    lib.polls.extend([None, Some(res)]);
    assert_eq!(handler(&mut d, &mut lib), None, "step 0 starts the call");
    assert_eq!(d.word(STEP), 1);
    assert_eq!(handler(&mut d, &mut lib), None, "the first poll is pending");
    assert_eq!(d.word(STEP), 1);
    assert_eq!(handler(&mut d, &mut lib), None, "the second poll completes");
    assert_eq!(d.word(STEP), 2);
    assert_eq!(d.word(RES) as i32, res);
    let c = handler(&mut d, &mut lib);
    (d, lib, c)
}

fn ended(d: &Driver, c: Completion) {
    assert_eq!(d.word(KIND), 0, "the request ended");
    assert_eq!(c, Some((0x4444, 77)), "the completion function is returned with the request id");
}

/// orig 0x232228
#[test]
fn list_maps_every_library_result_to_the_result_word() {
    for (res, code) in [(5, 0), (0, 0), (-2, 3), (-3, 2), (-4, 1), (-5, 2), (-6, 2), (-7, 2), (-8, 2), (-1, 2), (-100, 2)] {
        let (d, lib, c) = run_simple(2, Driver::handle_list, res);
        assert_eq!(lib.calls, vec!["list 1 2 0x1000 99 0x5000"], "res {res}");
        assert_eq!(result(&d, 2), code, "res {res}");
        // The status table keeps the count, or -1 on an error.
        let count = d.word(STATUS + (4 + 2) * 0x20 + 0x1c);
        assert_eq!(count, if res < 0 { 0xffff_ffff } else { res as u32 }, "res {res}");
        ended(&d, c);
    }
}

/// orig 0x2323c8
#[test]
fn list_b_maps_every_library_result_to_the_result_word() {
    for (res, code) in [(3, 0), (0, 0), (-2, 4), (-3, 3), (-4, 2), (-5, 3), (-6, 3), (-7, 3), (-8, 3), (-1, 3)] {
        let (d, lib, c) = run_simple(3, Driver::handle_list_b, res);
        assert_eq!(lib.calls, vec!["list 1 2 0x1000 99 0x5000"], "res {res}");
        assert_eq!(d.word(RESULTS + 4), code, "res {res}");
        ended(&d, c);
    }
}

/// orig 0x232558, 0x232668
#[test]
fn format_and_unformat_retry_while_tries_last() {
    for (unformat, handler, call, word) in [(false, Driver::handle_format as fn(&mut Driver, &mut dyn CardLib) -> Completion, "format 1 2", 8u32), (true, Driver::handle_unformat, "unformat 1 2", 12)] {
        // Success.
        let (d, lib, c) = run_simple(if unformat { 5 } else { 4 }, handler, 0);
        assert_eq!(lib.calls, vec![call]);
        assert_eq!(d.word(RESULTS + word), 0);
        ended(&d, c);
        // Failure with no tries left (what a queued request starts with): result 1 at once.
        let (d, _, c) = run_simple(if unformat { 5 } else { 4 }, handler, -2);
        assert_eq!(d.word(RESULTS + word), 1);
        ended(&d, c);
        // Failure with one try: wait 60 frames, call again, and the second failure ends with 1.
        let mut d = running(if unformat { 5 } else { 4 });
        d.set_word(0x1dc0b48, 1);
        let mut lib = Lib::default();
        lib.polls.extend([Some(-5), Some(-5)]);
        assert_eq!(handler(&mut d, &mut lib), None);
        assert_eq!(handler(&mut d, &mut lib), None);
        assert_eq!(handler(&mut d, &mut lib), None, "the failed result schedules a retry");
        assert_eq!((d.word(STEP), d.word(0x1dc0b4c), d.word(0x1dc0b48)), (3, 0x3c, 0));
        for left in (0..0x3c).rev() {
            assert_eq!(handler(&mut d, &mut lib), None);
            assert_eq!(d.word(0x1dc0b4c), left);
            assert_eq!(d.word(STEP), 3);
        }
        assert_eq!(handler(&mut d, &mut lib), None);
        assert_eq!(d.word(STEP), 0, "after the wait the call is made again");
        assert_eq!(handler(&mut d, &mut lib), None);
        assert_eq!(lib.calls, vec![call, call]);
        assert_eq!(handler(&mut d, &mut lib), None);
        let c = handler(&mut d, &mut lib);
        assert_eq!(d.word(RESULTS + word), 1);
        ended(&d, c);
    }
}

/// orig 0x232778
#[test]
fn chdir_maps_every_library_result_to_the_result_word() {
    for (res, code) in [(0, 0), (-2, 4), (-3, 2), (-4, 1), (-5, 2), (-6, 3), (-7, 2), (-8, 2), (-1, 2), (4, 2)] {
        let (d, lib, c) = run_simple(6, Driver::handle_chdir, res);
        assert_eq!(lib.calls, vec!["chdir 1 2 0x1000"], "res {res}");
        assert_eq!(d.word(RESULTS + 16), code, "res {res}");
        ended(&d, c);
    }
}

/// orig 0x232880
#[test]
fn mkdir_maps_every_library_result_to_the_result_word() {
    for (res, code) in [(0, 0), (-2, 4), (-3, 1), (-4, 2), (-5, 3), (-6, 3), (-7, 3), (-8, 3), (-1, 3), (2, 3)] {
        let (d, lib, c) = run_simple(7, Driver::handle_mkdir, res);
        assert_eq!(lib.calls, vec!["mkdir 1 2 0x1000"], "res {res}");
        assert_eq!(d.word(RESULTS + 20), code, "res {res}");
        ended(&d, c);
    }
}

/// orig 0x232b70
#[test]
fn delete_maps_every_library_result_to_the_result_word() {
    for (res, code) in [(0, 0), (-2, 4), (-3, 3), (-4, 1), (-5, 5), (-6, 2), (-7, 3), (-8, 3), (-1, 3)] {
        let (d, lib, c) = run_simple(9, Driver::handle_delete, res);
        assert_eq!(lib.calls, vec!["delete 1 2 0x1000"], "res {res}");
        assert_eq!(d.word(RESULTS + 28), code, "res {res}");
        ended(&d, c);
    }
}

/// orig 0x233040
#[test]
fn set_info_maps_every_library_result_to_the_result_word() {
    for (res, code) in [(0, 0), (-2, 3), (-3, 2), (-4, 1), (-5, 2), (-1, 2), (3, 2)] {
        let (d, lib, c) = run_simple(12, Driver::handle_set_info, res);
        assert_eq!(lib.calls, vec!["set_info 1 2 0x1000 0x2000"], "res {res}");
        assert_eq!(d.word(RESULTS + 40), code, "res {res}");
        ended(&d, c);
    }
}

/// Runs the file machine: open answers `open`; when it succeeds the transfer answers `io`; the close answers 0. Returns the driver, the calls
/// and the completion.
fn run_file(kind: u32, handler: fn(&mut Driver, &mut dyn CardLib) -> Completion, open: i32, io: i32) -> (Driver, Lib, Completion) {
    let mut d = running(kind);
    let mut lib = Lib::default();
    lib.polls.extend([Some(open), Some(io), Some(0)]);
    let mut done = None;
    for _ in 0..12 {
        done = handler(&mut d, &mut lib);
        if done.is_some() {
            break;
        }
    }
    (d, lib, done)
}

/// The three file machines with their codes: (kind, handler, result offset, mode, open codes, io codes, short code).
type FileCase = (u32, fn(&mut Driver, &mut dyn CardLib) -> Completion, u32, u32, [(i32, u32); 7], [(i32, u32); 7], u32);

fn file_cases() -> Vec<FileCase> {
    vec![
        // orig 0x232988
        (8, Driver::handle_create, 24, 0x203, [(-2, 6), (-3, 1), (-4, 2), (-5, 3), (-6, 5), (-7, 7), (-8, 5)], [(-2, 6), (-3, 1), (-4, 7), (-5, 3), (-6, 5), (-7, 5), (-8, 4)], 4),
        // orig 0x232c80
        (10, Driver::handle_read, 32, 1, [(-2, 5), (-3, 6), (-4, 1), (-5, 2), (-6, 4), (-7, 7), (-8, 4)], [(-2, 5), (-3, 3), (-4, 7), (-5, 2), (-6, 4), (-7, 4), (-8, 4)], 3),
        // orig 0x232e58
        (11, Driver::handle_write, 36, 2, [(-2, 7), (-3, 1), (-4, 2), (-5, 3), (-6, 6), (-7, 4), (-8, 6)], [(-2, 7), (-3, 1), (-4, 4), (-5, 3), (-6, 6), (-7, 6), (-8, 5)], 5),
    ]
}

#[test]
fn file_machines_map_open_and_transfer_results() {
    for (kind, handler, word, mode, open_codes, io_codes, short) in file_cases() {
        let io_call = if kind == 10 { "read 5 0x3000 0x400" } else { "write 5 0x3000 0x400" };
        // A full successful run: open, descriptor kept, transfer of the whole length, close, end.
        let (d, lib, c) = run_file(kind, handler, 5, 0x400);
        assert_eq!(lib.calls, vec![format!("open 1 2 0x1000 {mode:#x}"), io_call.to_string(), "close 5".to_string()], "kind {kind}");
        assert_eq!(d.word(RESULTS + word), 0, "kind {kind}");
        assert_eq!(d.word(0x1dc0bbc), 5, "the descriptor is kept");
        ended(&d, c);
        // A short count.
        let (d, _, c) = run_file(kind, handler, 5, 0x3ff);
        assert_eq!(d.word(RESULTS + word), short, "kind {kind}: short count");
        ended(&d, c);
        // Open errors end the request without a transfer.
        for (res, code) in open_codes {
            let (d, lib, c) = run_file(kind, handler, res, 0);
            assert_eq!(lib.calls, vec![format!("open 1 2 0x1000 {mode:#x}")], "kind {kind} open {res}");
            assert_eq!(d.word(RESULTS + word), code, "kind {kind} open {res}");
            ended(&d, c);
        }
        // Transfer errors still close the file, then end.
        for (res, code) in io_codes {
            let (d, lib, c) = run_file(kind, handler, 5, res);
            assert_eq!(lib.calls.len(), 3, "kind {kind} transfer {res}: open, transfer, close");
            assert_eq!(d.word(RESULTS + word), code, "kind {kind} transfer {res}");
            ended(&d, c);
        }
    }
}

/// orig 0x231f90
#[test]
fn info_poll_sets_the_connection_status() {
    let status = |d: &Driver, off: u32| d.word(STATUS + (4 + 2) * 0x20 + off);
    // Step 0 asks the library for both ports' slot counts and the info of the running port and slot.
    let mut d = running(1);
    d.set_word(0x1dc0b48, 3);
    let mut lib = Lib { slots: [1, 4], info: InfoOut { card_type: 2, free: 7000, format: 1 }, ..Lib::default() };
    assert_eq!(d.handle_info(&mut lib), None);
    assert_eq!(lib.calls, vec!["slots 0", "slots 1", "info 1 2"]);
    assert_eq!((d.word(0x1dc0cc8), d.word(0x1dc0ccc), d.word(0x1dc0cd0), d.word(0x1dc0cd4)), (1, 1, 1, 4));
    assert_eq!((status(&d, 0), status(&d, 8), status(&d, 4)), (2, 7000, 1));
    assert_eq!(d.word(STEP), 1);
    // (library result, card type, expected connection status, changed flag before, after, latches 0x10 / 0x14 / 0x18)
    type Case = (i32, u32, u32, u32, u32, [u32; 3]);
    let cases: [Case; 8] = [
        (0, 2, 0, 1, 0, [0, 0, 0]),
        (-1, 2, 1, 1, 0, [1, 1, 0]),
        (-2, 2, 2, 1, 0, [1, 1, 0]),
        (5, 2, 3, 0, 1, [1, 0, 1]),
        (5, 2, 3, 1, 1, [0, 0, 0]),
        (-2, 3, 3, 1, 1, [0, 0, 0]),
        (0, 3, 3, 0, 0, [0, 0, 0]),
        (-3, 2, 3, 0, 1, [1, 0, 1]),
    ];
    for (res, card_type, conn, changed0, changed1, latches) in cases {
        let mut d = running(1);
        d.set_word(0x1dc0b48, 3);
        d.set_word(0x1dc0b80, changed0);
        let mut lib = Lib { info: InfoOut { card_type, ..InfoOut::default() }, ..Lib::default() };
        lib.polls.extend([Some(res)]);
        d.handle_info(&mut lib);
        d.handle_info(&mut lib);
        assert_eq!(d.word(STEP), 2);
        // A -3 or worse result retries first while tries remain; the cases here with res > -3 end at once.
        let c = d.handle_info(&mut lib);
        let name = format!("result {res}, type {card_type}, changed {changed0}");
        if res <= -3 {
            assert_eq!(c, None, "{name}: a retry, not an end");
            assert_eq!((d.word(STEP), d.word(0x1dc0b4c), d.word(0x1dc0b48)), (3, 1, 2), "{name}");
            // The wait of one frame, then the poll starts over.
            assert_eq!(d.handle_info(&mut lib), None);
            assert_eq!((d.word(STEP), d.word(0x1dc0b4c)), (3, 0), "{name}");
            assert_eq!(d.handle_info(&mut lib), None);
            assert_eq!(d.word(STEP), 0, "{name}");
            d.set_word(0x1dc0b48, 0);
            d.set_word(0x1dc0bb8, res as u32);
            d.set_word(STEP, 2);
            let c = d.handle_info(&mut lib);
            assert_eq!(c, Some((0x4444, 77)), "{name}: with no tries left it ends");
        } else {
            assert_eq!(c, Some((0x4444, 77)), "{name}");
        }
        assert_eq!(status(&d, 0xc), conn, "{name}: connection");
        assert_eq!(d.word(0x1dc0b80), changed1, "{name}: changed flag");
        assert_eq!([status(&d, 0x10), status(&d, 0x14), status(&d, 0x18)], latches, "{name}: latches");
        assert_eq!(d.word(KIND), 0);
    }
}

/// orig 0x233140
#[test]
fn call_request_stores_whether_the_function_returned_zero() {
    for (ret, stored) in [(0i64, 1u32), (1, 0), (-1, 0), (1 << 40, 0)] {
        let mut d = running(13);
        let mut lib = Lib { call_result: ret, ..Lib::default() };
        let c = d.handle_call(&mut lib);
        assert_eq!(lib.calls, vec!["call 0x4444 77"]);
        assert_eq!(d.word(0x1dc0bb0), stored);
        // The end calls the same function again as the completion.
        ended(&d, c);
    }
}

/// orig 0x233178, 0x230f48
#[test]
fn queued_requests_run_through_the_dispatcher_and_the_wait() {
    let mut d = Driver::default();
    d.set_word(0x1dc0cd0, 1);
    d.set_word(0x1dc0cd4, 1);
    let id1 = d.enqueue_delete_slot0(1, 0x1234, 0xaaaa);
    let id2 = d.enqueue_mkdir_slot0(0, 0x5678, 0xbbbb);
    assert_eq!((id1, id2), (0, 1));
    assert!(d.is_busy(id1) && d.is_busy(id2) && d.is_busy(0));
    let mut lib = Lib::default();
    // Every poll completes with success after one pending answer.
    lib.polls.extend([None, Some(0), None, Some(0)]);
    let mut frames = 0;
    let mut done = Vec::new();
    d.wait(0, &mut lib, || frames += 1, |f, id| done.push((f, id)));
    assert_eq!(lib.calls, vec!["delete 1 0 0x1234", "mkdir 0 0 0x5678"]);
    assert_eq!(done, vec![(0xaaaa, 0), (0xbbbb, 1)]);
    assert!(frames >= 6, "{frames} frames");
    assert!(!d.is_busy(0) && d.is_idle());
    assert_eq!((d.result(9), d.result(7)), (0, 0));
    // Waiting for one id stops once that request has ended and leaves the later ones queued.
    let mut d = Driver::default();
    // Ids start at 10: id 0 means "everything" to the busy query and the wait.
    d.set_word(0x36c260, 10);
    let a = d.enqueue_delete_slot0(0, 1, 0);
    let b = d.enqueue_delete_slot0(0, 2, 0);
    let mut lib = Lib::default();
    lib.polls.extend([Some(0), Some(0)]);
    d.wait(a, &mut lib, || {}, |_, _| {});
    assert!(!d.is_busy(a), "the first request has ended");
    assert!(d.is_busy(b), "the second is still queued");
    assert_eq!(lib.calls, vec!["delete 0 0 0x1"]);
    // An unknown type does nothing.
    let mut d = running(99);
    assert_eq!(d.step(&mut Lib::default()), None);
    assert_eq!(d.word(KIND), 99);
}
