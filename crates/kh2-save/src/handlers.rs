//! The per-type request handlers of the card driver (`FUN_00231f90..FUN_00233140`) and the dispatcher: one step of the running request's
//! state machine per call. They talk to the IOP card library (`FUN_00303*`, `FUN_00304*`), which is outside this unit: [`CardLib`] stands
//! for it. The oracle cannot script the library (the factory stubs return 0), so these are covered by hand tests with scripted results
//! (`tests/handlers.rs`), one per library result code of the table in `docs/systems/core-runtime/memcard.md` section 1; they are not
//! `verified` by the ledger.
//!
//! Every handler works on the running request's words of [`Driver`] (step `0x36c26c`, the arguments `0x1dc0b50..`, the result word of its
//! type) and ends the request through [`Driver::end`]; the completion function that returns is the caller's to call.

use crate::ring::{
    A0, A1, A2, A3, A4, B4C, CHANGED, Driver, PEND, PORT, RESULTS, RUN_ID, RUN_KIND, SLOT, SLOT_COUNTS, STATUS, STEP, TRIES, DONE,
};

const RES: u32 = 0x1dc0bb8;
const FD: u32 = 0x1dc0bbc;
const CALL_RESULT: u32 = 0x1dc0bb0;
const PRESENT: u32 = 0x1dc0cc8;

/// What the info call delivers: the card type (status `+0`), the free space (status `+8`) and the format word (status `+4`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InfoOut {
    pub card_type: u32,
    pub free: u32,
    pub format: u32,
}

/// The IOP card library as the handlers use it. Each `start` call begins a library request; [`CardLib::poll`] is the non-blocking
/// completion check (`FUN_00303c90(1, 0, &result)`): `Some(result)` once the request has finished.
pub trait CardLib {
    /// `FUN_00303590`: the number of slots of a port.
    fn slot_count(&mut self, port: u32) -> u32;
    /// `FUN_00303db0`: the card info request. The three outputs are written to the status table when the request is started [inferred: the
    /// library writes them at completion, before the handler's next step reads them].
    fn get_info(&mut self, port: u32, slot: u32) -> InfoOut;
    /// `FUN_00303fa0`: list a directory by pattern, up to `max` entries into `table`.
    fn list(&mut self, port: u32, slot: u32, pattern: u32, max: u32, table: u32);
    /// `FUN_003042c0`.
    fn format(&mut self, port: u32, slot: u32);
    /// `FUN_00304898`.
    fn unformat(&mut self, port: u32, slot: u32);
    /// `FUN_00304180`.
    fn chdir(&mut self, port: u32, slot: u32, path: u32);
    /// `FUN_00303778`.
    fn mkdir(&mut self, port: u32, slot: u32, path: u32);
    /// `FUN_00303650`: open a file (mode 0x203 create and read-write, 1 read, 2 write).
    fn open(&mut self, port: u32, slot: u32, name: u32, mode: u32);
    /// `FUN_00303a00`.
    fn read(&mut self, fd: u32, buffer: u32, len: u32);
    /// `FUN_00303b18`.
    fn write(&mut self, fd: u32, buffer: u32, len: u32);
    /// `FUN_003037b0`.
    fn close(&mut self, fd: u32);
    /// `FUN_00304390`.
    fn delete(&mut self, port: u32, slot: u32, name: u32);
    /// `FUN_00304740`.
    fn set_info(&mut self, port: u32, slot: u32, a: u32, b: u32);
    /// `FUN_00303c90(1, 0, &result)`.
    fn poll(&mut self) -> Option<i32>;
    /// A type 13 request's function, called with the request id; returns its result.
    fn call(&mut self, function: u32, id: u32) -> i64;
}

/// A completion function to call with an id (what [`Driver::end`] returns).
pub type Completion = Option<(u32, u32)>;

impl Driver {
    fn step_no(&self) -> i32 {
        self.word(STEP) as i32
    }

    fn set_step(&mut self, v: i32) {
        self.set_word(STEP, v as u32);
    }

    fn res(&self) -> i32 {
        self.word(RES) as i32
    }

    fn port_slot(&self) -> (u32, u32) {
        (self.word(PORT), self.word(SLOT))
    }

    fn status_at(&self, off: u32) -> u32 {
        let (p, s) = self.port_slot();
        STATUS + p.wrapping_mul(4).wrapping_add(s).wrapping_mul(0x20) + off
    }

    /// Polls the library; on completion stores the result word and reports it.
    fn poll_done(&mut self, lib: &mut dyn CardLib) -> bool {
        match lib.poll() {
            Some(r) => {
                self.set_word(RES, r as u32);
                true
            }
            None => false,
        }
    }

    /// The card info poll (type 1).
    ///
    /// orig 0x231f90
    pub fn handle_info(&mut self, lib: &mut dyn CardLib) -> Completion {
        let step = self.step_no();
        let mut b4c = self.word(B4C);
        if step == 1 {
            if self.poll_done(lib) {
                self.set_step(2);
            }
        } else if step < 2 {
            if step == 0 {
                for port in 0..2u32 {
                    self.set_word(PRESENT + 4 * port, 1);
                    let n = lib.slot_count(port);
                    self.set_word(SLOT_COUNTS + 4 * port, n);
                }
                let (p, s) = self.port_slot();
                let out = lib.get_info(p, s);
                self.set_word(self.status_at(0), out.card_type);
                self.set_word(self.status_at(8), out.free);
                self.set_word(self.status_at(4), out.format);
                self.set_step(1);
            }
        } else if step == 2 {
            let r = self.res();
            if -3 < r || self.word(TRIES) == 0 {
                let st = |d: &Driver, off: u32| d.status_at(off);
                if self.word(st(self, 0)) == 3 {
                    self.set_word(st(self, 0xc), 3);
                } else if r == -1 {
                    self.set_word(st(self, 0x14), 1);
                    self.set_word(CHANGED, 0);
                    self.set_word(st(self, 0xc), 1);
                    self.set_word(st(self, 0x10), 1);
                } else if r == -2 {
                    self.set_word(st(self, 0x14), 1);
                    self.set_word(st(self, 0xc), 2);
                    self.set_word(CHANGED, 0);
                    self.set_word(st(self, 0x10), 1);
                } else if r == 0 {
                    self.set_word(st(self, 0xc), 0);
                    self.set_word(CHANGED, 0);
                } else {
                    self.set_word(st(self, 0xc), 3);
                    if self.word(CHANGED) == 0 {
                        self.set_word(st(self, 0x18), 1);
                        self.set_word(st(self, 0x10), 1);
                    }
                    self.set_word(CHANGED, 1);
                }
                return self.end();
            }
            self.set_step(3);
            b4c = 1;
            let t = self.word(TRIES);
            self.set_word(TRIES, t.wrapping_sub(1));
        } else if step == 3 {
            if b4c == 0 {
                self.set_step(0);
            } else {
                b4c -= 1;
            }
        }
        self.set_word(B4C, b4c);
        None
    }

    /// The step shared by the simple one-call handlers: step 0 starts the library call, step 1 polls, step 2 is the caller's closure
    /// (which maps the result and ends the request); other steps do nothing.
    fn simple(&mut self, lib: &mut dyn CardLib, start: impl FnOnce(&mut dyn CardLib, u32, u32, &Driver), finish: impl FnOnce(&mut Driver) -> Completion) -> Completion {
        match self.step_no() {
            1 => {
                if !self.poll_done(lib) {
                    return None;
                }
            }
            2 => return finish(self),
            0 => {
                let (p, s) = self.port_slot();
                start(lib, p, s, self);
            }
            _ => return None,
        }
        let st = self.step_no();
        self.set_step(st + 1);
        None
    }

    /// The directory listing (type 2). Result word `0x1dc0b84`: 0 and the count, 3 for -2, 1 for -4, 2 for any other error.
    ///
    /// orig 0x232228
    pub fn handle_list(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.simple(
            lib,
            |l, p, s, d| l.list(p, s, d.word(A0), d.word(A4), d.word(PEND)),
            |d| {
                let r = d.res();
                let at = d.status_at(0x1c);
                if r < 0 {
                    let code = if r == -2 { 3 } else if r == -4 { 1 } else { d.word(STEP) };
                    d.set_word(RESULTS, code);
                    d.set_word(at, 0xffff_ffff);
                } else {
                    d.set_word(RESULTS, 0);
                    d.set_word(at, r as u32);
                }
                d.end()
            },
        )
    }

    /// The other directory listing (type 3). Result word `0x1dc0b88`: 0 and the count, 4 for -2, 2 for -4, 3 for any other error.
    ///
    /// orig 0x2323c8
    pub fn handle_list_b(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.simple(
            lib,
            |l, p, s, d| l.list(p, s, d.word(A0), d.word(A4), d.word(PEND)),
            |d| {
                let r = d.res();
                let at = d.status_at(0x1c);
                if r < 0 {
                    let code = if r == -2 { 4 } else if r == -4 { d.word(STEP) } else { 3 };
                    d.set_word(RESULTS + 4, code);
                    d.set_word(at, 0xffff_ffff);
                } else {
                    d.set_word(RESULTS + 4, 0);
                    d.set_word(at, r as u32);
                }
                d.end()
            },
        )
    }

    /// Format and unformat share their steps: a failed result is retried after 60 frames while the try counter lasts (a queued request
    /// starts with 0 tries, so it fails at once), else the result word is 1.
    fn format_like(&mut self, lib: &mut dyn CardLib, unformat: bool, result: u32) -> Completion {
        match self.step_no() {
            1 => {
                if !self.poll_done(lib) {
                    return None;
                }
            }
            2 => {
                if self.res() == 0 {
                    self.set_word(result, 0);
                } else {
                    let t = self.word(TRIES);
                    if t != 0 {
                        self.set_step(3);
                        self.set_word(TRIES, t.wrapping_sub(1));
                        self.set_word(B4C, 0x3c);
                        return None;
                    }
                    self.set_word(result, 1);
                }
                return self.end();
            }
            3 => {
                let c = self.word(B4C);
                if c != 0 {
                    self.set_word(B4C, c - 1);
                    return None;
                }
                self.set_step(0);
                return None;
            }
            0 => {
                let (p, s) = self.port_slot();
                if unformat {
                    lib.unformat(p, s);
                } else {
                    lib.format(p, s);
                }
            }
            _ => return None,
        }
        let st = self.step_no();
        self.set_step(st + 1);
        None
    }

    /// Format (type 4). Result word `0x1dc0b8c`.
    ///
    /// orig 0x232558
    pub fn handle_format(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.format_like(lib, false, RESULTS + 8)
    }

    /// Unformat (type 5). Result word `0x1dc0b90`.
    ///
    /// orig 0x232668
    pub fn handle_unformat(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.format_like(lib, true, RESULTS + 12)
    }

    /// Change directory (type 6). Result word `0x1dc0b94`: 0, 1 for -4, 3 for -6, 4 for -2, 2 for anything else.
    ///
    /// orig 0x232778
    pub fn handle_chdir(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.simple(
            lib,
            |l, p, s, d| l.chdir(p, s, d.word(A0)),
            |d| {
                let r = d.res();
                let code = if r == -4 {
                    1
                } else if r < -3 {
                    if r == -6 { 3 } else { 2 }
                } else if r == -2 {
                    4
                } else if r == 0 {
                    0
                } else {
                    2
                };
                d.set_word(RESULTS + 16, code);
                d.end()
            },
        )
    }

    /// Make directory (type 7). Result word `0x1dc0b98`: 0, 1 for -3, 2 for -4, 4 for -2, 3 for anything else.
    ///
    /// orig 0x232880
    pub fn handle_mkdir(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.simple(
            lib,
            |l, p, s, d| l.mkdir(p, s, d.word(A0)),
            |d| {
                let r = d.res();
                let code = if r == -3 {
                    1
                } else if r < -2 {
                    if r == -4 { d.word(STEP) } else { 3 }
                } else if r == -2 {
                    4
                } else if r == 0 {
                    0
                } else {
                    3
                };
                d.set_word(RESULTS + 20, code);
                d.end()
            },
        )
    }

    /// Delete (type 9). Result word `0x1dc0ba0`: 0, 4 for -2, 1 for -4, 5 for -5, 2 for -6, 3 for anything else.
    ///
    /// orig 0x232b70
    pub fn handle_delete(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.simple(
            lib,
            |l, p, s, d| l.delete(p, s, d.word(A0)),
            |d| {
                let code = match d.res() {
                    0 => 0,
                    -6 => 2,
                    -5 => 5,
                    -4 => 1,
                    -2 => 4,
                    _ => 3,
                };
                d.set_word(RESULTS + 28, code);
                d.end()
            },
        )
    }

    /// Set file info (type 12). Result word `0x1dc0bac`: 0, 3 for -2, 1 for -4, 2 for anything else.
    ///
    /// orig 0x233040
    pub fn handle_set_info(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.simple(
            lib,
            |l, p, s, d| l.set_info(p, s, d.word(A0), d.word(A1)),
            |d| {
                let r = d.res();
                let code = if r == -2 {
                    3
                } else if r < -1 {
                    if r == -4 { 1 } else { 2 }
                } else if r == 0 {
                    0
                } else {
                    2
                };
                d.set_word(RESULTS + 40, code);
                d.end()
            },
        )
    }

    /// The file transfer machine: open (steps 0..2, the descriptor kept in `0x1dc0bbc`), the transfer (3..5, a count other than the length
    /// asked is an error), close (6, 7). `open_code` and `io_code` map a negative library result to the type's result code; `short` is the
    /// code for a short count.
    #[allow(clippy::too_many_arguments)]
    fn transfer(&mut self, lib: &mut dyn CardLib, mode: u32, write: bool, result: u32, open_code: fn(i32) -> u32, io_code: fn(i32) -> u32, short: u32) -> Completion {
        match self.step_no() {
            0 => {
                let (p, s) = self.port_slot();
                lib.open(p, s, self.word(A0), mode);
            }
            1 | 4 => {
                if !self.poll_done(lib) {
                    return None;
                }
            }
            2 => {
                let r = self.res();
                if r >= 0 {
                    let st = self.step_no();
                    self.set_step(st + 1);
                    self.set_word(FD, r as u32);
                    return None;
                }
                self.set_word(result, open_code(r));
                return self.end();
            }
            3 => {
                let (fd, buf, len) = (self.word(FD), self.word(A2), self.word(A3));
                if write {
                    lib.write(fd, buf, len);
                } else {
                    lib.read(fd, buf, len);
                }
            }
            5 => {
                let r = self.res();
                let code = if r < 0 {
                    io_code(r)
                } else if r as u32 == self.word(A3) {
                    0
                } else {
                    short
                };
                self.set_word(result, code);
            }
            6 => {
                let fd = self.word(FD);
                lib.close(fd);
            }
            7 => {
                if self.poll_done(lib) {
                    return self.end();
                }
                return None;
            }
            _ => return None,
        }
        let st = self.step_no();
        self.set_step(st + 1);
        None
    }

    /// Create and write a file (type 8). Result word `0x1dc0b9c`; open codes -2 6, -3 1, -4 2, -5 3, -7 7, other 5; write codes -2 6, -3 1,
    /// -4 7, -5 3, -8 4, other 5; a short count 4.
    ///
    /// orig 0x232988
    pub fn handle_create(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.transfer(
            lib,
            0x203,
            true,
            RESULTS + 24,
            |r| match r {
                -2 => 6,
                -3 => 1,
                -4 => 2,
                -5 => 3,
                -7 => 7,
                _ => 5,
            },
            |r| match r {
                -2 => 6,
                -3 => 1,
                -4 => 7,
                -5 => 3,
                -8 => 4,
                _ => 5,
            },
            4,
        )
    }

    /// Read a file (type 10). Result word `0x1dc0ba4`; open codes -2 5, -3 6, -4 1, -5 2, -7 7, other 4; read codes -2 5, -3 3, -4 7, -5 2,
    /// other 4; a short count 3.
    ///
    /// orig 0x232c80
    pub fn handle_read(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.transfer(
            lib,
            1,
            false,
            RESULTS + 32,
            |r| match r {
                -2 => 5,
                -3 => 6,
                -4 => 1,
                -5 => 2,
                -7 => 7,
                _ => 4,
            },
            |r| match r {
                -2 => 5,
                -3 => 3,
                -4 => 7,
                -5 => 2,
                _ => 4,
            },
            3,
        )
    }

    /// Write a file (type 11). Result word `0x1dc0ba8`; open codes -2 7, -3 1, -4 2, -5 3, -7 4, other 6; write codes -2 7, -3 1, -4 4, -5 3,
    /// -8 5, other 6; a short count 5.
    ///
    /// orig 0x232e58
    pub fn handle_write(&mut self, lib: &mut dyn CardLib) -> Completion {
        self.transfer(
            lib,
            2,
            true,
            RESULTS + 36,
            |r| match r {
                -2 => 7,
                -3 => 1,
                -4 => 2,
                -5 => 3,
                -7 => 4,
                _ => 6,
            },
            |r| match r {
                -2 => 7,
                -3 => 1,
                -4 => 4,
                -5 => 3,
                -8 => 5,
                _ => 6,
            },
            5,
        )
    }

    /// The call request (type 13): calls the request's function with its id, stores whether it returned 0 in `0x1dc0bb0` and ends the
    /// request (which calls the same function again as the completion, as the original does).
    ///
    /// orig 0x233140
    pub fn handle_call(&mut self, lib: &mut dyn CardLib) -> Completion {
        let (f, id) = (self.word(DONE), self.word(RUN_ID));
        let r = lib.call(f, id);
        self.set_word(CALL_RESULT, u32::from(r == 0));
        self.end()
    }

    /// Runs one step of the running request's handler (the table at `0x36c278` by type; type 0 is [`Driver::next`]). Types above 13
    /// have no entry; nothing happens.
    ///
    /// orig 0x233178
    pub fn step(&mut self, lib: &mut dyn CardLib) -> Completion {
        match self.word(RUN_KIND) {
            0 => {
                self.next();
                None
            }
            1 => self.handle_info(lib),
            2 => self.handle_list(lib),
            3 => self.handle_list_b(lib),
            4 => self.handle_format(lib),
            5 => self.handle_unformat(lib),
            6 => self.handle_chdir(lib),
            7 => self.handle_mkdir(lib),
            8 => self.handle_create(lib),
            9 => self.handle_delete(lib),
            10 => self.handle_read(lib),
            11 => self.handle_write(lib),
            12 => self.handle_set_info(lib),
            13 => self.handle_call(lib),
            _ => None,
        }
    }

    /// Steps the driver until request `id` has finished (every request when `id` is 0), calling `frame` between steps (the original waits
    /// a vertical sync there, `FUN_002f8ec8(0)`) and `complete` for each completion function that returns.
    ///
    /// orig 0x230f48
    pub fn wait(&mut self, id: u32, lib: &mut dyn CardLib, mut frame: impl FnMut(), mut complete: impl FnMut(u32, u32)) {
        use crate::ring::{RD, WR};
        let mut run = |d: &mut Driver, lib: &mut dyn CardLib| {
            if let Some((f, i)) = d.step(lib) {
                complete(f, i);
            }
            frame();
        };
        if id == 0 {
            // Until the ring is empty, then until the running request ends.
            loop {
                if self.word(WR) == self.word(RD) && self.word(RUN_KIND) == 0 {
                    return;
                }
                run(self, lib);
            }
        }
        // While the request is still queued, step; once it is running, until the driver is idle again.
        loop {
            let (wr, mut rd) = (self.word(WR), self.word(RD));
            let mut queued = false;
            while rd != wr {
                if self.word(crate::ring::CARD_BASE + rd * 0x20 + 4) == id {
                    queued = true;
                    break;
                }
                rd = (rd + 1) & 0x3f;
            }
            if !queued {
                break;
            }
            run(self, lib);
        }
        if self.word(RUN_KIND) != 0 && self.word(RUN_ID) == id {
            while self.word(RUN_KIND) != 0 {
                run(self, lib);
            }
        }
    }
}
