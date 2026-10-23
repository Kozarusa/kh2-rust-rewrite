//! Per-address Rust handlers: hooks (replace a function), probes (run before an instruction) and
//! syscalls (the host kernel's table). All three are tables of closures; the run loop does not
//! look at them for ordinary instructions, only at addresses recorded in a bitmap.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::Ee;

/// A handler: a plain function or any closure. It may keep state in its captures (`Rc<RefCell<..>>`,
/// `Cell`s) and may call [`Ee::call`] for other addresses; reaching its own address again while it
/// runs is a [`crate::Fault::Reentrant`].
pub type Handler = Rc<RefCell<dyn FnMut(&mut Ee)>>;

/// Plain function form of a handler, kept for callers that pass `fn(&mut Ee)`.
pub type Hook = fn(&mut Ee);

pub(crate) fn handler<F: FnMut(&mut Ee) + 'static>(f: F) -> Handler {
    Rc::new(RefCell::new(f))
}

/// Closures keyed by guest address. `insert` and `remove` keep the same shape as the `HashMap`
/// this type replaced, so existing call sites compile unchanged.
#[derive(Default)]
pub struct HookTable {
    map: HashMap<u32, Handler>,
    /// Bumped on every change; the run loop rebuilds its bitmap when it differs from the value
    /// it last saw.
    pub(crate) generation: u64,
}

impl HookTable {
    pub fn insert<F: FnMut(&mut Ee) + 'static>(&mut self, addr: u32, f: F) -> Option<Handler> {
        self.generation += 1;
        self.map.insert(addr, handler(f))
    }

    pub fn remove(&mut self, addr: &u32) -> Option<Handler> {
        self.generation += 1;
        self.map.remove(addr)
    }

    pub fn contains_key(&self, addr: &u32) -> bool {
        self.map.contains_key(addr)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn clear(&mut self) {
        self.generation += 1;
        self.map.clear();
    }

    pub(crate) fn get(&self, addr: u32) -> Option<Handler> {
        self.map.get(&addr).cloned()
    }

    pub(crate) fn addresses(&self) -> impl Iterator<Item = u32> + '_ {
        self.map.keys().copied()
    }
}

/// Host syscalls keyed by the kernel's syscall number (the value in `v1` when `syscall` runs,
/// signed: the SDK uses negative numbers for the interrupt-context variants).
#[derive(Default)]
pub struct SyscallTable {
    map: HashMap<i32, Handler>,
}

impl SyscallTable {
    /// Installs the handler for syscall `number`. It runs with `pc` already past the `syscall`
    /// instruction (so it may simply return); to resume somewhere else it calls [`Ee::jump`] or
    /// [`Ee::set_context`]. Arguments are in a0..a3, the result goes to v0.
    pub fn insert<F: FnMut(&mut Ee) + 'static>(&mut self, number: i32, f: F) -> Option<Handler> {
        self.map.insert(number, handler(f))
    }

    pub fn remove(&mut self, number: &i32) -> Option<Handler> {
        self.map.remove(number)
    }

    pub fn contains_key(&self, number: &i32) -> bool {
        self.map.contains_key(number)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub(crate) fn get(&self, number: i32) -> Option<Handler> {
        self.map.get(&number).cloned()
    }
}

/// One bit per instruction word of the 32 MB address space: set where a hook or a probe sits.
pub(crate) struct Special {
    bits: Box<[u64; Self::WORDS]>,
    /// One byte per 4 KB page: nonzero if any word in the page has a handler. Small enough to stay
    /// in the first-level cache, so most instructions never touch `bits`.
    pages: Box<[u8; Self::PAGES]>,
}

impl Special {
    const WORDS: usize = (crate::RAM / 4) / 64;
    const PAGES: usize = crate::RAM / 4096;

    pub(crate) fn new() -> Special {
        let mut s = Special {
            bits: vec![0u64; Self::WORDS].into_boxed_slice().try_into().expect("size"),
            pages: vec![0u8; Self::PAGES].into_boxed_slice().try_into().expect("size"),
        };
        // The call sentinel always takes the slow path so that a return ends the run loop.
        s.set(crate::SENTINEL);
        s
    }

    #[inline(always)]
    pub(crate) fn has(&self, pc: u32) -> bool {
        if self.pages[((pc >> 12) as usize) & (Self::PAGES - 1)] == 0 {
            return false;
        }
        let i = ((pc & 0x01FF_FFFF) >> 2) as usize;
        self.bits[(i >> 6) & (Self::WORDS - 1)] >> (i & 63) & 1 != 0
    }

    fn set(&mut self, pc: u32) {
        self.pages[((pc >> 12) as usize) & (Self::PAGES - 1)] = 1;
        let i = ((pc & 0x01FF_FFFF) >> 2) as usize;
        self.bits[(i >> 6) & (Self::WORDS - 1)] |= 1 << (i & 63);
    }

    pub(crate) fn rebuild<'a>(&mut self, tables: impl IntoIterator<Item = &'a HookTable>) {
        self.bits.fill(0);
        self.pages.fill(0);
        self.set(crate::SENTINEL);
        for t in tables {
            for a in t.addresses() {
                self.set(a);
            }
        }
    }
}
