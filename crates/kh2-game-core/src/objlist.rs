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

use crate::area::ctl::AreaHost;
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

// ---- the base object constructor (0x168d30) and its finishing pass (0x1698d0) --------------------------------------------------

/// The arguments of the base constructor besides the object itself (`a0`): the model id (`a1`), the object entry pointer (`a2`, stored at
/// +8), the list priority (`a3`, its low byte), the pointer to the 16-byte start position (`t0`) and the start rotation (float argument
/// `f12`).
#[derive(Clone, Copy, Debug)]
pub struct CtorArgs {
    pub id: u64,
    pub entry: u32,
    pub priority: u32,
    pub pos: u32,
    pub rot: f32,
}

impl ObjList {
    /// The base object constructor: sets every field of the object block `this` (which must have been taken with [`ObjList::alloc`]) to its
    /// start value, builds its parts through the other units (every call goes through the host and is logged: the part constructors, the
    /// action and animation set-up, the model data), links the object into the list and copies the start position and rotation. The vector
    /// at +0x620 is the constant (0, 0, 0, 1) the original stores from the vector unit's zero register. `cost` is the 8 bytes of the area
    /// cost budget (0x1c893b8), to which the entry's byte +0x54 is added. The object's parts are at these offsets: +0x80 and +0x9c (two
    /// animation sets), +0x110, +0x140, +0x308 (16 pairs), +0x3b4 and +0x414 (the flag tables), +0x478, +0x480, +0x590, +0x670, +0x7a0 (the
    /// data cache record), +0x8e0 and +0x4a0. `scratch` is the address of a 16-byte stack copy of the start position that the original
    /// hands to `0x16a700`. The float argument of `0x16a700`'s sibling `0x16a7f0` (the rotation, in f12) is passed to the host as its bit pattern after the register arguments.
    ///
    /// orig 0x168d30
    pub fn construct(&mut self, host: &mut impl AreaHost, cost: &mut [u8], this: u32, a: CtorArgs, scratch: u32) {
        let c0 = host.read32(0x37a180);
        let c1 = host.read32(0x37a184);
        {
            let b = &mut self.get_mut(this).expect("an allocated object block").0[..];
            put(b, 0, 0x35b950);
            put(b, 8, a.entry);
            put(b, 4, this);
            for o in [0x1c, 0x14, 0x18, 0x2c, 0x24, 0x28, 0x10, 0x20] {
                put(b, o, 0);
            }
            for o in [0x3c, 0x30, 0x34, 0x38] {
                put(b, o, c0);
            }
        }
        host.call(0x1db410, &[(this + 0x80) as u64]);
        host.call(0x1db410, &[(this + 0x9c) as u64]);
        {
            let b = &mut self.get_mut(this).unwrap().0[..];
            put(b, 0x108, 0);
            put(b, 0x100, 0xffff_ffff);
            put(b, 0x10c, 0);
            put(b, 0x104, 5);
        }
        host.call(0x1943c8, &[(this + 0x110) as u64]);
        host.call(0x17d768, &[(this + 0x140) as u64, this as u64]);
        for k in 0..16u32 {
            let b = &mut self.get_mut(this).unwrap().0[..];
            put(b, 0x308 + 8 * k as usize, 0);
            put(b, 0x30c + 8 * k as usize, 0);
            host.call(0x32f6a0, &[(this + 0x30c + 8 * k) as u64]);
        }
        {
            let b = &mut self.get_mut(this).unwrap().0[..];
            for o in [0x3a0, 0x300, 0x304, 0x388, 0x38c, 0x390, 0x394, 0x398, 0x39c] {
                put(b, o, 0);
            }
            for o in [0x3af, 0x3ad, 0x3ae, 0x3ac] {
                b[o] = 0x80;
            }
            put(b, 0x3b0, 0);
            crate::progress_flags::recom_clear(&mut b[0x3b4..0x414]);
            crate::progress_flags::recom_clear(&mut b[0x414..0x474]);
            put(b, 0x474, 0);
        }
        host.call(0x330d38, &[(this + 0x478) as u64]);
        put(&mut self.get_mut(this).unwrap().0[..], 0x47c, 0);
        host.call(0x330d38, &[(this + 0x480) as u64]);
        {
            let b = &mut self.get_mut(this).unwrap().0[..];
            for o in [0x484, 0x488, 0x48c, 0x4a4, 0x4a8, 0x4a0, 0x4b4, 0x4b0, 0x500, 0x504, 0x508, 0x50c, 0x510] {
                put(b, o, 0);
            }
            put(b, 0x4ac, 0xffff_ffff);
            b[0x514] = 0;
            b[0x518..0x51a].fill(0);
            b[0x51c..0x53c].fill(0);
            for o in [0x56c, 0x564, 0x568, 0x560, 0x570, 0x57c, 0x588, 0x58c] {
                put(b, o, 0);
            }
            put(b, 0x578, 3);
            b[0x576..0x578].fill(0);
        }
        host.call(0x32e458, &[(this + 0x590) as u64]);
        {
            let b = &mut self.get_mut(this).unwrap().0[..];
            put(b, 0x644, 0xffff);
            for o in [0x648, 0x64c, 0x650] {
                put(b, o, 0);
            }
            b[0x654] = 0;
            b[0x620..0x630].copy_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x80, 0x3f]);
            b[0x630..0x640].fill(0);
            let f = get(b, 0x660);
            put(b, 0x660, f & 0xffff_fffa | 2);
        }
        host.call(0x330bd0, &[(this + 0x670) as u64]);
        {
            let b = &mut self.get_mut(this).unwrap().0[..];
            crate::progress_flags::reset_record(&mut b[0x7a0..0x7b8]);
        }
        {
            let mut rec = [0u8; 0x18];
            rec.copy_from_slice(&self.get(this).unwrap().0[0x7a0..0x7b8]);
            crate::system_tables::data_cache_set(host, this + 0x7a0, &mut rec, a.id, a.entry as i32 as i64 as u64);
            self.get_mut(this).unwrap().0[0x7a0..0x7b8].copy_from_slice(&rec);
        }
        {
            let b = &mut self.get_mut(this).unwrap().0[..];
            for o in [0x7f8, 0x800, 0x80c, 0x804, 0x808, 0x810, 0x818, 0x81c, 0x828, 0x830, 0x838] {
                put(b, o, 0);
            }
            b[0x7fc] = 0;
            put(b, 0x82c, c0);
            put(b, 0x834, c0);
            for o in [0x860, 0x86c, 0x864, 0x868, 0x87c, 0x874, 0x878, 0x88c, 0x884, 0x888, 0x8ac, 0x8a4, 0x8a8, 0x870, 0x880, 0x8a0] {
                put(b, o, 0);
            }
            b[0x8bc..0x8c0].fill(0x80);
            for o in [0x8d4, 0x8c4, 0x8c8, 0x8cc] {
                put(b, o, 0);
            }
            put(b, 0x8d0, c0);
            b[0x8dc] = 1;
            put(b, 0x8c0, c0);
            put(b, 0x8d8, 0);
            b[0x8dd] = 0;
        }
        host.call(0x330ce0, &[(this + 0x8e0) as u64]);
        let no_shadow = host.call(0x1934c0, &[]);
        let shadow_byte = if no_shadow == 0 && host.read32(0x35d268) & 0x10 == 0 { host.read8(a.entry.wrapping_add(0x56)) } else { 0 };
        {
            let b = &mut self.get_mut(this).unwrap().0[..];
            put(b, 0x9a8, c1);
            b[0x9ac] = shadow_byte;
            put(b, 0x9b0, 0);
            b[0x9ad] = a.priority as u8;
        }
        self.insert(this);
        host.call(0x16b648, &[this as u64, 0]);
        let knm = host.call(0x1a79d8, &[(this + 0x7a0) as u64]);
        put(&mut self.get_mut(this).unwrap().0[..], 0x8b0, knm as u32);
        if knm == 0 {
            let b = &mut self.get_mut(this).unwrap().0[..];
            put(b, 0x670, 0);
            let f = get(b, 0x768);
            put(b, 0x768, f & 0xffff_fff3);
        } else {
            let alloc = host.call(0x104f98, &[]);
            let obj = get(&self.get(this).unwrap().0[..], 0x8b0);
            let v = host.vcall(obj, 9, &[alloc]);
            put(&mut self.get_mut(this).unwrap().0[..], 0x670, v as u32);
            let knt = host.call(0x1a7a10, &[(this + 0x7a0) as u64]);
            if knt != 0 {
                let timer = get(&self.get(this).unwrap().0[..], 0x670);
                host.call(0x11e018, &[timer as i32 as i64 as u64, knt]);
                let b = &mut self.get_mut(this).unwrap().0[..];
                put(b, 0x69c, knt as u32);
                let f = get(b, 0x7f8);
                put(b, 0x7f8, f | 2);
            }
            let timer = get(&self.get(this).unwrap().0[..], 0x670);
            host.call(0x11c268, &[timer as i32 as i64 as u64]);
            let (w88, w8c) = (host.read32(0x37a188), host.read32(0x37a18c));
            {
                let b = &mut self.get_mut(this).unwrap().0[..];
                put(b, 0x774, w88);
                put(b, 0x778, w8c);
            }
            host.call(0x193048, &[this as u64]);
            let has_bit = host.read16(a.entry.wrapping_add(0x48)) & 0x10 != 0;
            let b = &mut self.get_mut(this).unwrap().0[..];
            if has_bit {
                let f = get(b, 0x768);
                put(b, 0x768, f | 0x20);
            }
            let e = get(b, 8);
            b[0x78f] = host.read8(e.wrapping_add(6));
        }
        let file = {
            let b = &mut self.get_mut(this).unwrap().0[..];
            put(b, 0x668, 0);
            put(b, 0x664, 0);
            get(b, 0x7ac)
        };
        let e = crate::system_tables::bar_nth_of_type(host, file, 0x17, 0);
        if e != 0 {
            let payload = host.read32(e.wrapping_add(8));
            if payload != 0 {
                host.call(0x169af0, &[this as u64, payload as i32 as i64 as u64]);
            }
        }
        {
            let b = &mut self.get_mut(this).unwrap().0[..];
            put(b, 0x638, 0);
            put(b, 0x63c, c0);
            put(b, 0x630, 0);
            put(b, 0x634, c0);
        }
        let used = host.read8(a.entry.wrapping_add(0x54)) as f32;
        crate::system_tables::cost_add(cost, used);
        host.call(0x1ae998, &[(this + 0x4a0) as u64, this as u64]);
        {
            let mut v = [0u32; 4];
            if 0x890 + this != a.pos {
                for (k, w) in v.iter_mut().enumerate() {
                    *w = host.read32(a.pos.wrapping_add(4 * k as u32));
                }
            }
            let b = &mut self.get_mut(this).unwrap().0[..];
            put(b, 0x820, 0);
            put(b, 0x824, 0);
            if 0x890 + this != a.pos {
                for (k, w) in v.iter().enumerate() {
                    put(b, 0x890 + 4 * k, *w);
                }
            }
            put(b, 0x89c, a.rot.to_bits());
        }
        host.call(0x16a7f0, &[this as u64, a.rot.to_bits() as u64]);
        for k in 0..4u32 {
            let w = host.read32(a.pos.wrapping_add(4 * k));
            host.write32(scratch.wrapping_add(4 * k), w);
        }
        host.call(0x16a700, &[this as u64, scratch as u64]);
        host.call(0x1921e8, &[this as u64]);
        host.call(0x169be8, &[this as u64]);
        host.call(0x1a7b50, &[(this + 0x7a0) as u64, this as u64, (this + 0x9c) as u64, (this + 0x7b8) as u64]);
    }

    /// The pass after an object is built: when the object's flag word +0x588 has bit 0x20 clear, the type-3 entry of the object's BAR (the
    /// file at +0x7ac) is looked up, its address kept in the word 0x52217c and, when the entry has a payload, `0x169a90` is told the object
    /// and the payload; the 16-byte position at +0x540 is copied to +0x890, bit 0x80 is set in the word +0x7f8 and the word +0x89c takes
    /// the float at +0x55c.
    ///
    /// orig 0x1698d0
    pub fn finish_construct(&mut self, host: &mut impl AreaHost, this: u32) {
        let (flags, file) = {
            let b = &self.get(this).unwrap().0[..];
            (get(b, 0x588), get(b, 0x7ac))
        };
        if flags & 0x20 == 0 {
            let e = crate::system_tables::bar_nth_of_type(host, file, 3, 0);
            host.write32(0x52217c, e);
            if e != 0 {
                let payload = host.read32(e.wrapping_add(8));
                if payload != 0 {
                    host.call(0x169a90, &[this as u64, payload as i32 as i64 as u64]);
                }
            }
        }
        let b = &mut self.get_mut(this).unwrap().0[..];
        let pos: [u8; 16] = b[0x540..0x550].try_into().unwrap();
        b[0x890..0x8a0].copy_from_slice(&pos);
        let f = get(b, 0x7f8);
        put(b, 0x7f8, f | 0x80);
        let w = get(b, 0x55c);
        put(b, 0x89c, w);
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
