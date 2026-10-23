//! The game's global object list and the handle scheme of the object blocks (core runtime, WP-L1-P14; FAC-8 L1-b).
//!
//! Every object the game creates is linked into one list: head word at 0x1d5d7a0, tail word at 0x1d5d7a4, the link at object +0x8b4,
//! kept sorted by the priority byte at +0x9ad (lowest first; a new object goes in front of every object of equal or higher priority).
//! The object constructor inserts itself (0x1b7440). Walkers skip objects that are leaving (flags +0x108 bit 0x80000 or 0x10000000).
//!
//! Handles (the FAC-8 form): an object is `OBJ_BASE + slot * 0x1000`, 0 is null. Every object class fits the 0x1000 stride (the
//! largest is 0xd20). The slot policy (lowest free slot first) is the port's own; the original's allocator is another unit, and the
//! oracle test translates handles to the addresses it laid its blocks at (copy-in and copy-out with address translation). An object
//! block here is a 0x1000-byte array with typed accessors at the original's offsets, so a test can compare it byte by byte.
//!
//! What is NOT here: the object constructors and the removal from the list (the destructor path), the spawn record behind +0x81c
//! (its serial lookup takes a resolver, see [`ObjList::find_by_serial`]). Nothing in the app uses this module yet.

use crate::shared::enemy_obj::SELF_BASE;

/// First object handle; slot `n` is `OBJ_BASE + n * HANDLE_STRIDE`. Same base as the enemy objects of the AI host.
pub const OBJ_BASE: u32 = SELF_BASE;
/// Distance between two object handles.
pub const HANDLE_STRIDE: u32 = 0x1000;
/// Bytes of one object block (the stride; the base object is 0x9c0, the classes up to 0xd20).
pub const OBJ_BLOCK: usize = 0x1000;

/// Object +0x108: flags. 0x80000 and 0x10000000 mark an object that is leaving (`FUN_0019a440`).
pub const OFF_FLAGS: usize = 0x108;
/// Object +0x81c: the spawn record handle (`unit_arg[0]`); the record's u16 at +0x1e is the object's serial (`FUN_0016b928`).
pub const OFF_SPAWN: usize = 0x81c;
/// Object +0x8b4: the link to the next object of the list.
pub const OFF_LINK: usize = 0x8b4;
/// Object +0x9ad: the list priority (byte), the fifth constructor argument.
pub const OFF_PRIORITY: usize = 0x9ad;
/// The flag bits that make an object "leaving".
pub const LEAVE_BITS: u32 = 0x1008_0000;

fn get(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn put(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// An object block (0x1000 bytes with the original's offsets). Only the list fields have accessors here.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ObjBlock(pub Box<[u8; OBJ_BLOCK]>);

impl Default for ObjBlock {
    fn default() -> Self {
        ObjBlock(Box::new([0; OBJ_BLOCK]))
    }
}

impl ObjBlock {
    pub fn flags(&self) -> u32 {
        get(&self.0[..], OFF_FLAGS)
    }
    pub fn set_flags(&mut self, v: u32) {
        put(&mut self.0[..], OFF_FLAGS, v)
    }
    /// The spawn record handle, 0 for none.
    pub fn spawn(&self) -> u32 {
        get(&self.0[..], OFF_SPAWN)
    }
    pub fn set_spawn(&mut self, v: u32) {
        put(&mut self.0[..], OFF_SPAWN, v)
    }
    /// The next object of the list, 0 at the end.
    pub fn link(&self) -> u32 {
        get(&self.0[..], OFF_LINK)
    }
    pub fn set_link(&mut self, v: u32) {
        put(&mut self.0[..], OFF_LINK, v)
    }
    pub fn priority(&self) -> u8 {
        self.0[OFF_PRIORITY]
    }
    pub fn set_priority(&mut self, v: u8) {
        self.0[OFF_PRIORITY] = v
    }

    /// Whether the object is leaving: flag 0x80000 or 0x10000000 set.
    ///
    /// orig 0x19a440
    pub fn is_leave(&self) -> bool {
        let f = self.flags();
        f & 0x8_0000 != 0 || f & 0x1000_0000 != 0
    }
}

/// The object blocks of the port and the two list words.
#[derive(Default)]
pub struct ObjList {
    slots: Vec<Option<ObjBlock>>,
    /// The word at 0x1d5d7a0: first object, 0 for an empty list.
    pub head: u32,
    /// The word at 0x1d5d7a4: last object.
    pub tail: u32,
}

impl ObjList {
    pub fn new() -> ObjList {
        ObjList::default()
    }

    /// The slot of a handle, or `None` for 0 or a value that is not an object handle.
    pub fn slot_of(h: u32) -> Option<usize> {
        let d = h.checked_sub(OBJ_BASE)?;
        (d % HANDLE_STRIDE == 0).then_some((d / HANDLE_STRIDE) as usize)
    }

    pub fn handle_of(slot: usize) -> u32 {
        OBJ_BASE + slot as u32 * HANDLE_STRIDE
    }

    /// Takes the lowest free slot for a zeroed block and returns its handle. The block is not linked.
    pub fn alloc(&mut self) -> u32 {
        let slot = self.slots.iter().position(|s| s.is_none()).unwrap_or_else(|| {
            self.slots.push(None);
            self.slots.len() - 1
        });
        self.slots[slot] = Some(ObjBlock::default());
        Self::handle_of(slot)
    }

    /// Gives the slot back (the caller has unlinked the object, which this module does not do).
    pub fn free(&mut self, h: u32) {
        if let Some(s) = Self::slot_of(h).and_then(|s| self.slots.get_mut(s)) {
            *s = None;
        }
    }

    pub fn get(&self, h: u32) -> Option<&ObjBlock> {
        self.slots.get(Self::slot_of(h)?)?.as_ref()
    }

    pub fn get_mut(&mut self, h: u32) -> Option<&mut ObjBlock> {
        self.slots.get_mut(Self::slot_of(h)?)?.as_mut()
    }

    /// The live handles in slot order.
    pub fn live(&self) -> Vec<u32> {
        (0..self.slots.len()).filter(|&s| self.slots[s].is_some()).map(Self::handle_of).collect()
    }

    fn link_of(&self, h: u32) -> u32 {
        self.get(h).map_or(0, |b| b.link())
    }

    fn priority_of(&self, h: u32) -> u8 {
        self.get(h).map_or(0, |b| b.priority())
    }

    /// The handles of the list from the head along the links (stops at a dangling link or after 100000 steps).
    pub fn walk(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut p = self.head;
        while p != 0 && self.get(p).is_some() && out.len() < 100_000 {
            out.push(p);
            p = self.link_of(p);
        }
        out
    }

    /// Links the object into the list behind the last object whose priority is below its own (strictly), so it goes in front of
    /// equal priorities; head and tail are kept. The object must not be in the list already.
    ///
    /// orig 0x1b7440
    pub fn insert(&mut self, obj: u32) {
        let new_prio = self.priority_of(obj);
        let mut prev = 0;
        loop {
            let cur = if prev != 0 { self.link_of(prev) } else { self.head };
            if cur == 0 || self.priority_of(cur) >= new_prio {
                break;
            }
            prev = cur;
        }
        if prev != 0 {
            if prev == self.tail {
                self.get_mut(prev).unwrap().set_link(obj);
                self.get_mut(obj).unwrap().set_link(0);
                self.tail = obj;
            } else {
                let after = self.link_of(prev);
                self.get_mut(obj).unwrap().set_link(after);
                self.get_mut(prev).unwrap().set_link(obj);
            }
        } else if self.head != 0 {
            let head = self.head;
            self.get_mut(obj).unwrap().set_link(head);
            self.head = obj;
        } else {
            self.head = obj;
            self.tail = obj;
            self.get_mut(obj).unwrap().set_link(0);
        }
    }

    /// Whether the handle is a member of the list (a walk comparing every link).
    ///
    /// orig 0x1b80e8
    pub fn contains(&self, obj: u32) -> bool {
        let mut prev = 0;
        loop {
            let cur = if prev != 0 { self.link_of(prev) } else { self.head };
            if cur == 0 {
                return false;
            }
            if cur == obj {
                return true;
            }
            prev = cur;
        }
    }

    /// The object in the list and not leaving (0x1b80e8 and 0x19a440 together).
    ///
    /// orig 0x19a3f8
    pub fn is_live(&self, obj: u32) -> bool {
        self.contains(obj) && self.get(obj).is_some_and(|b| !b.is_leave())
    }

    /// The next object after `prev` (the head for 0) that is not leaving, 0 at the end; the list walker of the game.
    ///
    /// orig 0x1b7c88
    pub fn next_live(&self, prev: u32) -> u32 {
        let mut p = prev;
        loop {
            p = if p != 0 { self.link_of(p) } else { self.head };
            if p == 0 || !self.get(p).is_some_and(|b| b.is_leave()) {
                return p;
            }
        }
    }

    /// The serial of the object's spawn record: the u16 at record +0x1e, 0 when the object has no record. `record_serial` answers
    /// for a record handle (the records are another unit's blocks).
    ///
    /// orig 0x16b928
    pub fn serial_of(&self, obj: u32, record_serial: &impl Fn(u32) -> u16) -> u16 {
        match self.get(obj).map_or(0, |b| b.spawn()) {
            0 => 0,
            rec => record_serial(rec),
        }
    }

    /// The first non-leaving object whose serial equals `serial` (compared as a 16-bit number), 0 when there is none.
    ///
    /// orig 0x16b0d8
    pub fn find_by_serial(&self, serial: u16, record_serial: &impl Fn(u32) -> u16) -> u32 {
        let mut p = 0;
        loop {
            p = self.next_live(p);
            if p == 0 || self.serial_of(p, record_serial) == serial {
                return p;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_priority(l: &mut ObjList, p: u8) -> u32 {
        let h = l.alloc();
        l.get_mut(h).unwrap().set_priority(p);
        l.insert(h);
        h
    }

    #[test]
    fn insert_sorts_by_priority_and_puts_equals_in_front() {
        let mut l = ObjList::new();
        let a = with_priority(&mut l, 5);
        let b = with_priority(&mut l, 2);
        let c = with_priority(&mut l, 5);
        let d = with_priority(&mut l, 9);
        assert_eq!(l.walk(), vec![b, c, a, d]);
        assert_eq!((l.head, l.tail), (b, d));
        assert_eq!(l.get(d).unwrap().link(), 0);
    }

    #[test]
    fn leaving_objects_are_skipped_by_the_walker() {
        let mut l = ObjList::new();
        let a = with_priority(&mut l, 1);
        let b = with_priority(&mut l, 2);
        let c = with_priority(&mut l, 3);
        l.get_mut(b).unwrap().set_flags(0x10_0000 | 0x8_0000);
        assert_eq!(l.next_live(0), a);
        assert_eq!(l.next_live(a), c);
        assert_eq!(l.next_live(c), 0);
        assert!(l.contains(b) && !l.is_live(b) && l.is_live(c));
        assert!(!l.contains(0xdead));
    }

    #[test]
    fn serial_lookup_uses_the_spawn_record() {
        let mut l = ObjList::new();
        let a = with_priority(&mut l, 1);
        let b = with_priority(&mut l, 2);
        l.get_mut(a).unwrap().set_spawn(0x0f00_0000);
        l.get_mut(b).unwrap().set_spawn(0x0f00_1000);
        let rec = |h: u32| if h == 0x0f00_0000 { 7 } else { 9 };
        assert_eq!(l.find_by_serial(9, &rec), b);
        assert_eq!(l.find_by_serial(7, &rec), a);
        assert_eq!(l.find_by_serial(1, &rec), 0);
        assert_eq!(l.find_by_serial(0, &rec), 0);
    }
}
