//! The file request queue of the original's file unit (`core.file`): 0x4c-byte request records on a singly linked list
//! (the list head and tail words sit at `0x35b940` and `0x35b944`), filled by the request builders and consumed by the
//! loader thread and the completion pass. The port reads files straight from the disc image, so only the records and
//! the list bookkeeping exist here, byte for byte as the game lays them out, for code that needs the original's
//! scheduling to line up; the thread, the CD transfer and the callbacks are not ported (`not-needed:hw`, see the
//! ledger notes).
//!
//! Record layout: +0 next link, +4 state (0 queued, 1 reading, 2 done), +8 name (copied by the name helper at
//! `0x312078`), +0x30 kind (0 plain read, 1 read then fix up as BAR, 2 special read into the fixed 0x1f18000 buffer),
//! +0x34 destination, +0x38 and +0x3c request arguments (the completion callback and its argument, read at
//! `0x168010`), +0x40 result of the read, +0x44 offset argument, +0x4a u16 offset-read flag.

/// Size of one request record.
pub const RECORD: usize = 0x4c;
/// Address of the fixed buffer the special read (kind 2) fills, and its size (cleared first).
pub const SPECIAL_BUFFER: u32 = 0x01f1_8000;
pub const SPECIAL_BUFFER_SIZE: u32 = 0xa8000;

/// The list words: newest request first; the tail word is set only when a request joins an empty list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Queue {
    pub head: u32,
    pub tail: u32,
}

/// One request record as bytes in the game's layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request(pub [u8; RECORD]);

impl Default for Request {
    fn default() -> Self {
        Request([0; RECORD])
    }
}

impl Request {
    fn put32(&mut self, o: usize, v: u32) {
        self.0[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub fn get32(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.0[o..o + 4].try_into().unwrap())
    }
}

/// Fills a fresh record and links it at the head of the list. The name helpers `0x31218c` and `0x312078` are the host's
/// (`name_host(callee)` is called for each, in the original's order).
///
/// orig 0x168268
/// orig 0x1682e0
pub fn enqueue(q: &mut Queue, at: u32, r: &mut Request, kind: i32, name_host: &mut dyn FnMut(u32)) {
    r.put32(0x30, kind as u32);
    r.put32(4, 0);
    r.put32(0x44, 0);
    r.0[0x4a..0x4c].copy_from_slice(&0u16.to_le_bytes());
    name_host(0x31218c);
    name_host(0x312078);
    if q.head == 0 {
        q.head = at;
        q.tail = at;
        r.put32(0, 0);
    } else {
        r.put32(0, q.head);
        q.head = at;
    }
}

/// Plain read request.
///
/// orig 0x167d20
pub fn plain(q: &mut Queue, at: u32, r: &mut Request, dest: u32, arg: u32, arg2: u32, name_host: &mut dyn FnMut(u32)) {
    enqueue(q, at, r, 0, name_host);
    r.put32(0x3c, arg2);
    r.put32(0x34, dest);
    r.put32(0x38, arg);
}

/// Read request at an offset (sets the offset-read flag word).
///
/// orig 0x167da8
#[allow(clippy::too_many_arguments)]
pub fn at_offset(q: &mut Queue, at: u32, r: &mut Request, dest: u32, offset: u32, flag: u16, arg: u32, arg2: u32, name_host: &mut dyn FnMut(u32)) {
    enqueue(q, at, r, 0, name_host);
    r.0[0x4a..0x4c].copy_from_slice(&flag.to_le_bytes());
    r.put32(0x34, dest);
    r.put32(0x38, arg);
    r.put32(0x3c, arg2);
    r.put32(0x44, offset);
}

/// Read request that fixes the file up as a BAR when it arrives.
///
/// orig 0x167e58
pub fn bar(q: &mut Queue, at: u32, r: &mut Request, dest: u32, arg: u32, arg2: u32, name_host: &mut dyn FnMut(u32)) {
    enqueue(q, at, r, 1, name_host);
    r.put32(0x3c, arg2);
    r.put32(0x34, dest);
    r.put32(0x38, arg);
}

/// Special read into the fixed buffer; the caller clears the buffer first (`clear_host` gets the address and size
/// of the clear, the original's `0x310ba4`).
///
/// orig 0x167ee0
pub fn special(q: &mut Queue, at: u32, r: &mut Request, arg: u32, arg2: u32, clear_host: &mut dyn FnMut(u32, u32), name_host: &mut dyn FnMut(u32)) {
    clear_host(SPECIAL_BUFFER, SPECIAL_BUFFER_SIZE);
    enqueue(q, at, r, 2, name_host);
    r.put32(0x3c, arg2);
    r.put32(0x34, SPECIAL_BUFFER);
    r.put32(0x38, arg);
}

/// Whether any request is on the list.
///
/// orig 0x168240
pub fn is_reading(q: &Queue) -> bool {
    q.head != 0
}

/// Drops the whole list when called with `(1, 0xffff)`; any other pair does nothing.
///
/// orig 0x1683e0
pub fn reset_if(q: &mut Queue, a: i32, b: i64) {
    if b == 0xffff && a == 1 {
        *q = Queue::default();
    }
}

/// The list reset the game runs at its reset points.
///
/// orig 0x168408
pub fn reset(q: &mut Queue) {
    reset_if(q, 1, 0xffff);
}
