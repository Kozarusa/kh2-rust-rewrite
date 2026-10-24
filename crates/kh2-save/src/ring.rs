//! The card driver's request ring (`0x230e90..0x231a60` and the getters): 64 entries of 0x20 bytes, a write and a read index, the
//! running request's words, the result words and the per-port status table.
//!
//! The state is kept as the original's memory: [`Driver::core`] is the words at `0x36c258` (write index, read index, id counter, running
//! id, running type, sub-step, default listing pointer) and [`Driver::card`] the span `0x1dc0348..0x1dc0ce0` (the ring, the running
//! request's words, the result words, the status table, the slot counts), so the oracle compares both byte for byte. Addresses below are
//! those of the original.
//!
//! The per-type handlers (`FUN_00231f90..FUN_00233040`) call the IOP card library and are not here: a request is queued, popped into the
//! running words by [`Driver::next`], and ended by [`Driver::end`]; what happens in between is the library's.

/// Start of the core words.
pub const CORE_BASE: u32 = 0x36c258;
/// Length of the core words image.
pub const CORE_LEN: usize = 0x20;
/// Start of the card span.
pub const CARD_BASE: u32 = 0x1dc0348;
/// Length of the card span.
pub const CARD_LEN: usize = 0x1dc0ce0 - 0x1dc0348;

pub const WR: u32 = 0x36c258;
pub const RD: u32 = 0x36c25c;
pub const NEXT_ID: u32 = 0x36c260;
pub const RUN_ID: u32 = 0x36c264;
pub const RUN_KIND: u32 = 0x36c268;
pub const STEP: u32 = 0x36c26c;
pub const DEFAULT_PTR: u32 = 0x36c270;
pub const TRIES: u32 = 0x1dc0b48;
pub const B4C: u32 = 0x1dc0b4c;
pub const A0: u32 = 0x1dc0b50;
pub const A1: u32 = 0x1dc0b54;
pub const A2: u32 = 0x1dc0b58;
pub const A3: u32 = 0x1dc0b5c;
pub const A4: u32 = 0x1dc0b60;
pub const DONE: u32 = 0x1dc0b64;
pub const POLL_ON: u32 = 0x1dc0b68;
pub const POLL_RELOAD: u32 = 0x1dc0b6c;
pub const POLL_COUNT: u32 = 0x1dc0b70;
pub const POLL_PORT: u32 = 0x1dc0b74;
pub const POLL_SLOT: u32 = 0x1dc0b78;
pub const CHANGED: u32 = 0x1dc0b80;
pub const RESULTS: u32 = 0x1dc0b84;
pub const PEND: u32 = 0x1dc0bb4;
pub const PORT: u32 = 0x1dc0bc0;
pub const SLOT: u32 = 0x1dc0bc4;
pub const STATUS: u32 = 0x1dc0bc8;
pub const SLOT_COUNTS: u32 = 0x1dc0cd0;

/// The request types (the first word of a ring entry).
pub mod kind {
    pub const INFO: u32 = 1;
    pub const LIST: u32 = 2;
    pub const LIST_B: u32 = 3;
    pub const FORMAT: u32 = 4;
    pub const UNFORMAT: u32 = 5;
    pub const CHDIR: u32 = 6;
    pub const MKDIR: u32 = 7;
    pub const CREATE: u32 = 8;
    pub const DELETE: u32 = 9;
    pub const READ: u32 = 10;
    pub const WRITE: u32 = 11;
    pub const SET_INFO: u32 = 12;
    pub const CALL: u32 = 13;
}

/// The driver's memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Driver {
    pub core: [u8; CORE_LEN],
    pub card: Vec<u8>,
}

impl Default for Driver {
    fn default() -> Self {
        Driver { core: [0; CORE_LEN], card: vec![0; CARD_LEN] }
    }
}

impl Driver {
    fn slice(&self, a: u32) -> (&[u8], usize) {
        if a >= CARD_BASE { (&self.card, (a - CARD_BASE) as usize) } else { (&self.core, (a - CORE_BASE) as usize) }
    }

    /// The word at an address of the original.
    pub fn word(&self, a: u32) -> u32 {
        let (b, o) = self.slice(a);
        u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
    }

    /// Sets the word at an address of the original.
    pub fn set_word(&mut self, a: u32, v: u32) {
        let (o, b) = if a >= CARD_BASE { ((a - CARD_BASE) as usize, &mut self.card[..]) } else { ((a - CORE_BASE) as usize, &mut self.core[..]) };
        b[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn entry(&self, i: u32, off: u32) -> u32 {
        self.word(CARD_BASE + (i & 0x3f) * 0x20 + off)
    }

    /// Writes a request into the ring and returns its id. `p` are the enqueue function's arguments in order (port, slot, then the
    /// type's own); `n` how many of them the type keeps (`FUN_002310c8` and the others store `p[2..n]` at entry `+0x10..`).
    fn push(&mut self, kind: u32, p: [u32; 6], n: usize) -> u32 {
        let wr = self.word(WR);
        let id = self.word(NEXT_ID);
        let e = CARD_BASE + (wr & 0x3f) * 0x20;
        self.set_word(e, kind);
        self.set_word(e + 4, id);
        self.set_word(e + 8, p[0]);
        self.set_word(e + 0xc, p[1]);
        for (i, &v) in p[2..n].iter().enumerate() {
            self.set_word(e + 0x10 + 4 * i as u32, v);
        }
        self.set_word(WR, wr.wrapping_add(1) & 0x3f);
        self.set_word(NEXT_ID, id.wrapping_add(1));
        id
    }

    /// Type 1, the card info poll: `(port, slot, completion)`.
    ///
    /// orig 0x2310c8
    pub fn enqueue_info(&mut self, port: u32, slot: u32, done: u32) -> u32 {
        self.push(kind::INFO, [port, slot, done, 0, 0, 0], 3)
    }

    /// orig 0x2315a0
    pub fn enqueue_info_slot0(&mut self, port: u32, done: u32) -> u32 {
        self.enqueue_info(port, 0, done)
    }

    /// Type 2, the directory listing: `(port, slot, a, b, table, c)`; a null `table` becomes the default listing pointer.
    ///
    /// orig 0x231120
    pub fn enqueue_list(&mut self, port: u32, slot: u32, a: u32, b: u32, table: u32, c: u32) -> u32 {
        let table = if table == 0 { self.word(DEFAULT_PTR) } else { table };
        self.push(kind::LIST, [port, slot, a, b, table, c], 6)
    }

    /// orig 0x2315c0
    pub fn enqueue_list_slot0(&mut self, port: u32, a: u32, b: u32, table: u32, c: u32) -> u32 {
        self.enqueue_list(port, 0, a, b, table, c)
    }

    /// Type 3, the other directory listing: `(port, slot, a, b, c, d)`.
    ///
    /// orig 0x2311b8
    pub fn enqueue_list_b(&mut self, p: [u32; 6]) -> u32 {
        self.push(kind::LIST_B, p, 6)
    }

    /// orig 0x2315f0
    pub fn enqueue_list_b_slot0(&mut self, port: u32, a: u32, b: u32, c: u32, d: u32) -> u32 {
        self.enqueue_list_b([port, 0, a, b, c, d])
    }

    /// Type 4, format: `(port, slot, completion)`.
    ///
    /// orig 0x231218
    pub fn enqueue_format(&mut self, port: u32, slot: u32, done: u32) -> u32 {
        self.push(kind::FORMAT, [port, slot, done, 0, 0, 0], 3)
    }

    /// orig 0x231620
    pub fn enqueue_format_slot0(&mut self, port: u32, done: u32) -> u32 {
        self.enqueue_format(port, 0, done)
    }

    /// Type 5, unformat: `(port, slot, completion)`.
    ///
    /// orig 0x231270
    pub fn enqueue_unformat(&mut self, port: u32, slot: u32, done: u32) -> u32 {
        self.push(kind::UNFORMAT, [port, slot, done, 0, 0, 0], 3)
    }

    /// orig 0x231640
    pub fn enqueue_unformat_slot0(&mut self, port: u32, done: u32) -> u32 {
        self.enqueue_unformat(port, 0, done)
    }

    /// Type 6, change directory: `(port, slot, path, completion)`.
    ///
    /// orig 0x2312c8
    pub fn enqueue_chdir(&mut self, port: u32, slot: u32, path: u32, done: u32) -> u32 {
        self.push(kind::CHDIR, [port, slot, path, done, 0, 0], 4)
    }

    /// orig 0x231660
    pub fn enqueue_chdir_slot0(&mut self, port: u32, path: u32, done: u32) -> u32 {
        self.enqueue_chdir(port, 0, path, done)
    }

    /// Type 7, make directory: `(port, slot, path, completion)`.
    ///
    /// orig 0x231320
    pub fn enqueue_mkdir(&mut self, port: u32, slot: u32, path: u32, done: u32) -> u32 {
        self.push(kind::MKDIR, [port, slot, path, done, 0, 0], 4)
    }

    /// orig 0x231680
    pub fn enqueue_mkdir_slot0(&mut self, port: u32, path: u32, done: u32) -> u32 {
        self.enqueue_mkdir(port, 0, path, done)
    }

    /// Type 8, create and write a file: `(port, slot, name, buffer, length, completion)`.
    ///
    /// orig 0x231378
    pub fn enqueue_create(&mut self, p: [u32; 6]) -> u32 {
        self.push(kind::CREATE, p, 6)
    }

    /// orig 0x2316a0
    pub fn enqueue_create_slot0(&mut self, port: u32, name: u32, buffer: u32, length: u32, done: u32) -> u32 {
        self.enqueue_create([port, 0, name, buffer, length, done])
    }

    /// Type 9, delete: `(port, slot, name, completion)`.
    ///
    /// orig 0x2313d8
    pub fn enqueue_delete(&mut self, port: u32, slot: u32, name: u32, done: u32) -> u32 {
        self.push(kind::DELETE, [port, slot, name, done, 0, 0], 4)
    }

    /// orig 0x2316d0
    pub fn enqueue_delete_slot0(&mut self, port: u32, name: u32, done: u32) -> u32 {
        self.enqueue_delete(port, 0, name, done)
    }

    /// Type 9 with a fifth word: `(port, slot, a, b, completion)`.
    ///
    /// orig 0x2314f0
    pub fn enqueue_delete_b(&mut self, port: u32, slot: u32, a: u32, b: u32, done: u32) -> u32 {
        self.push(kind::DELETE, [port, slot, a, b, done, 0], 5)
    }

    /// orig 0x231750
    pub fn enqueue_delete_b_slot0(&mut self, port: u32, a: u32, b: u32, done: u32) -> u32 {
        self.enqueue_delete_b(port, 0, a, b, done)
    }

    /// Type 10, read a file: `(port, slot, name, buffer, length, completion)`.
    ///
    /// orig 0x231430
    pub fn enqueue_read(&mut self, p: [u32; 6]) -> u32 {
        self.push(kind::READ, p, 6)
    }

    /// orig 0x2316f0
    pub fn enqueue_read_slot0(&mut self, port: u32, name: u32, buffer: u32, length: u32, done: u32) -> u32 {
        self.enqueue_read([port, 0, name, buffer, length, done])
    }

    /// Type 11, write a file: `(port, slot, name, buffer, length, completion)`.
    ///
    /// orig 0x231490
    pub fn enqueue_write(&mut self, p: [u32; 6]) -> u32 {
        self.push(kind::WRITE, p, 6)
    }

    /// orig 0x231720
    pub fn enqueue_write_slot0(&mut self, port: u32, name: u32, buffer: u32, length: u32, done: u32) -> u32 {
        self.enqueue_write([port, 0, name, buffer, length, done])
    }

    /// Type 13, call a function in the queue's order: the function pointer goes into entry `+8`.
    ///
    /// orig 0x231550
    pub fn enqueue_call(&mut self, function: u32) -> u32 {
        let wr = self.word(WR);
        let id = self.word(NEXT_ID);
        let e = CARD_BASE + (wr & 0x3f) * 0x20;
        self.set_word(e + 8, function);
        self.set_word(WR, wr.wrapping_add(1) & 0x3f);
        self.set_word(NEXT_ID, id.wrapping_add(1));
        self.set_word(e, kind::CALL);
        self.set_word(e + 4, id);
        id
    }

    /// True when no request runs.
    ///
    /// orig 0x230e90
    pub fn is_idle(&self) -> bool {
        self.word(RUN_KIND) == 0
    }

    /// Whether request `id` is still queued or running; with id 0 whether anything is (1) or the driver is idle and the ring empty (0).
    ///
    /// orig 0x230ea0
    pub fn is_busy(&self, id: u32) -> bool {
        let wr = self.word(WR);
        let mut rd = self.word(RD);
        if id == 0 {
            return !(wr == rd && self.word(RUN_KIND) == 0);
        }
        loop {
            if rd == wr {
                return self.word(RUN_KIND) != 0 && self.word(RUN_ID) == id;
            }
            let this = self.entry(rd, 4);
            rd = rd.wrapping_add(1) & 0x3f;
            if this == id {
                return true;
            }
        }
    }

    /// Ends the running request: type 0, the poll countdown reloaded. Returns the completion function (when non-zero) and the id to
    /// call it with.
    ///
    /// orig 0x231a18
    pub fn end(&mut self) -> Option<(u32, u32)> {
        self.set_word(RUN_KIND, 0);
        let reload = self.word(POLL_RELOAD);
        self.set_word(POLL_COUNT, reload);
        let done = self.word(DONE);
        (done != 0).then(|| (done, self.word(RUN_ID)))
    }

    /// The idle handler (type 0): with an empty ring, counts the idle poll down and starts a card info poll on the next port and slot
    /// when it reaches 0; otherwise pops the next request into the running words.
    ///
    /// orig 0x231a60
    pub fn next(&mut self) {
        let wr = self.word(WR);
        let rd = self.word(RD);
        if wr == rd {
            if self.card[(POLL_ON - CARD_BASE) as usize] != 0 {
                let count = self.word(POLL_COUNT);
                if count != 0 {
                    self.set_word(POLL_COUNT, count.wrapping_sub(1));
                    return;
                }
                self.set_word(RUN_KIND, 1);
                let reload = self.word(POLL_RELOAD);
                self.set_word(POLL_COUNT, reload);
                self.set_word(TRIES, 0);
                self.set_word(STEP, 0);
                let (port, slot) = (self.word(POLL_PORT), self.word(POLL_SLOT));
                self.set_word(PORT, port);
                self.set_word(B4C, 0);
                self.set_word(DONE, 0);
                self.set_word(SLOT, slot);
                let count = self.word(SLOT_COUNTS + port.wrapping_mul(4));
                if slot != count.wrapping_sub(1) {
                    self.set_word(POLL_SLOT, slot.wrapping_add(1));
                    return;
                }
                self.set_word(POLL_SLOT, 0);
                self.set_word(POLL_PORT, ((port as i32).wrapping_add(1) % 2) as u32);
            }
            return;
        }
        let e = |d: &Driver, off: u32| d.entry(rd, off);
        let k = e(self, 0);
        let advance = |d: &mut Driver| d.set_word(RD, rd.wrapping_add(1) & 0x3f);
        match k {
            1 => {
                advance(self);
                let id = e(self, 4);
                self.set_word(RUN_ID, id);
                self.set_word(RUN_KIND, 1);
                self.set_word(STEP, 0);
                self.set_word(TRIES, 3);
                self.set_word(B4C, 0);
                let (done, port, slot) = (e(self, 0x10), e(self, 8), e(self, 0xc));
                self.set_word(DONE, done);
                self.set_word(PORT, port);
                self.set_word(SLOT, slot);
            }
            2 | 3 => {
                advance(self);
                let id = e(self, 4);
                self.set_word(RUN_ID, id);
                self.set_word(RUN_KIND, k);
                self.set_word(STEP, 0);
                // Type 2 keeps (+0x18, +0x14, +0x1c, +0x10) in (A0, A4, DONE, PEND); type 3 swaps the roles of +0x10/+0x14/+0x18.
                let (a0, a4, pend) = if k == 2 { (e(self, 0x18), e(self, 0x14), e(self, 0x10)) } else { (e(self, 0x10), e(self, 0x18), e(self, 0x14)) };
                self.set_word(A0, a0);
                self.set_word(A4, a4);
                let done = e(self, 0x1c);
                self.set_word(DONE, done);
                self.set_word(PEND, pend);
                let (port, slot) = (e(self, 8), e(self, 0xc));
                self.set_word(PORT, port);
                self.set_word(SLOT, slot);
            }
            4 | 5 => {
                self.set_word(RUN_KIND, k);
                advance(self);
                let (port, slot, done, id) = (e(self, 8), e(self, 0xc), e(self, 0x10), e(self, 4));
                self.set_word(PORT, port);
                self.set_word(SLOT, slot);
                self.set_word(DONE, done);
                self.set_word(RUN_ID, id);
                self.set_word(TRIES, 0);
                self.set_word(STEP, 0);
                self.set_word(B4C, 0);
            }
            6 | 7 | 9 => {
                self.set_word(RUN_KIND, k);
                let (id, port, slot, a0, done) = (e(self, 4), e(self, 8), e(self, 0xc), e(self, 0x10), e(self, 0x14));
                self.set_word(RUN_ID, id);
                self.set_word(PORT, port);
                self.set_word(SLOT, slot);
                self.set_word(A0, a0);
                self.set_word(DONE, done);
                advance(self);
                self.set_word(STEP, 0);
            }
            8 | 10 | 11 => {
                self.set_word(RUN_KIND, k);
                advance(self);
                let id = e(self, 4);
                self.set_word(RUN_ID, id);
                self.set_word(STEP, 0);
                let (a0, a2, a3, done, port, slot) = (e(self, 0x10), e(self, 0x14), e(self, 0x18), e(self, 0x1c), e(self, 8), e(self, 0xc));
                self.set_word(A0, a0);
                self.set_word(A2, a2);
                self.set_word(A3, a3);
                self.set_word(DONE, done);
                self.set_word(PORT, port);
                self.set_word(SLOT, slot);
            }
            0xc => {
                let (id, port, slot, a0, a1, done) = (e(self, 4), e(self, 8), e(self, 0xc), e(self, 0x10), e(self, 0x14), e(self, 0x18));
                self.set_word(RUN_ID, id);
                self.set_word(PORT, port);
                self.set_word(SLOT, slot);
                self.set_word(A0, a0);
                self.set_word(A1, a1);
                self.set_word(DONE, done);
                self.set_word(RUN_KIND, 0xc);
                advance(self);
                self.set_word(STEP, 0);
            }
            0xd => {
                self.set_word(RUN_KIND, 0xd);
                let (done, id) = (e(self, 8), e(self, 4));
                self.set_word(DONE, done);
                advance(self);
                self.set_word(RUN_ID, id);
            }
            _ => {}
        }
    }

    /// The changed flag of the last info poll (`0x1dc0b80`).
    ///
    /// orig 0x231778
    pub fn changed(&self) -> u32 {
        self.word(CHANGED)
    }

    /// The result word of request type `k` (2..=12): `0x1dc0b84 + 4 * (k - 2)`. The getters `FUN_00231788..FUN_00231828` return them in
    /// order.
    ///
    /// orig 0x231788, 0x231798, 0x2317a8, 0x2317b8, 0x2317c8, 0x2317d8, 0x2317e8, 0x2317f8, 0x231808, 0x231818, 0x231828
    pub fn result(&self, k: u32) -> u32 {
        self.word(RESULTS + 4 * (k - 2))
    }

    fn status_word(&self, port: i32, slot: i32, off: u32) -> u32 {
        self.word(STATUS + (port.wrapping_mul(4).wrapping_add(slot) as u32).wrapping_mul(0x20) + off)
    }

    /// The card type of a port and slot (status `+0`).
    ///
    /// orig 0x231878
    pub fn card_type(&self, port: i32, slot: i32) -> u32 {
        self.status_word(port, slot, 0)
    }

    /// Status `+4`.
    ///
    /// orig 0x231898
    pub fn status_4(&self, port: i32, slot: i32) -> u32 {
        self.status_word(port, slot, 4)
    }

    /// Status `+8`.
    ///
    /// orig 0x2318b8
    pub fn status_8(&self, port: i32, slot: i32) -> u32 {
        self.status_word(port, slot, 8)
    }

    /// The connection status (`+0xc`).
    ///
    /// orig 0x2318d8
    pub fn connection(&self, port: i32, slot: i32) -> u32 {
        self.status_word(port, slot, 0xc)
    }

    /// Forces the connection status of a port and slot to 3.
    ///
    /// orig 0x2318f8
    pub fn force_no_card(&mut self, port: i32, slot: i32) {
        let a = STATUS + (port.wrapping_mul(4).wrapping_add(slot) as u32).wrapping_mul(0x20) + 0xc;
        self.set_word(a, 3);
    }

    /// The file count of the last listing (status `+0x1c`).
    ///
    /// orig 0x2319f8
    pub fn file_count(&self, port: i32, slot: i32) -> u32 {
        self.status_word(port, slot, 0x1c)
    }
}
