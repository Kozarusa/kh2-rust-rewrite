//! The moving collision objects (lifts, platforms and other bodies' collision; WP-L1-P15, spec
//! `docs/systems/core-runtime/collision.md` section 9.5).
//!
//! The original keeps a pool of 64 entries of 16 bytes at `DAT_0035a8a0` (`FUN_001652b8`): `{owner, collision object,
//! flags, next}`, a stack of free indices at `+0x400` with its count at `+0x500`, and a singly linked list (head
//! `DAT_005210c0`, tail `DAT_005210c4`) in allocation order. An entry is named by its address (`base + 16 * index`),
//! which is also what `coll +0x78` holds while a body stands on a moving object; the port keeps the same numbers so a
//! collider record compares byte for byte with the game's. The collision object of an entry is an `Oct` built over the
//! entry's COCT file with every group on, then placed with a matrix (`Oct::place`).
//!
//! Flag bits of an entry: bit 0 disabled (the sweeps skip it), bit 1 hidden (the list walk `FUN_00165910` skips it
//! altogether). [inferred] bit 1 has no writer in the clusters read.

use crate::geom::*;
use crate::oct::*;

/// The pool size.
pub const POOL_ENTRIES: usize = 0x40;

/// 0x37955c: the start of the highest-floor search, the "no moving floor" height.
pub(crate) const FLOOR_START: f32 = crate::oct::FLOOR_OUT_NONE;

/// One pool entry.
#[derive(Clone, Debug, Default)]
struct Entry {
    owner: u32,
    oct: Option<Oct>,
    flags: u32,
    next: u32,
}

impl Entry {
    /// orig 0x1650e0: owner, collision object and flags to zero (the link stays).
    fn clear(&mut self) {
        self.flags = 0;
        self.owner = 0;
        self.oct = None;
    }
}

/// The pool of moving collision objects.
#[derive(Clone, Debug, Default)]
pub struct MovingPool {
    /// `DAT_0035a8a0`: the address entry handles count from.
    base: u32,
    /// `DAT_0035a8a4`: the allocator tag handed to every allocation [inferred: the game's heap id].
    tag: u32,
    entries: Vec<Entry>,
    /// The stack of free indices (`+0x400`) and its count (`+0x500`).
    free: Vec<u32>,
    free_count: i32,
    /// `DAT_005210c0` and `DAT_005210c4`.
    head: u32,
    tail: u32,
}

/// What `MovingPool::ray_all` found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolRay {
    /// Some object was hit.
    pub found: bool,
    /// The polygon of the last (nearest) hit, 0xffff for none.
    pub poly: u16,
    /// The entry handle of the last hit, 0 for none.
    pub entry: u32,
}

impl MovingPool {
    /// A pool with no entries at all: every query finds nothing (the plain field of the original, where the list
    /// head `DAT_005210c0` is 0).
    pub fn empty() -> MovingPool {
        MovingPool::default()
    }

    /// orig 0x1652b8: the pool of 64 empty entries, all indices free (the stack holds 0..63, count 64), the list empty.
    /// `base` is the address entry handles count from (the game's heap block), `tag` the allocator id.
    pub fn new(base: u32, tag: u32) -> MovingPool {
        MovingPool {
            base,
            tag,
            entries: vec![Entry::default(); POOL_ENTRIES],
            free: (0..POOL_ENTRIES as u32).collect(),
            free_count: POOL_ENTRIES as i32,
            head: 0,
            tail: 0,
        }
    }

    /// orig 0x165388: frees the pool block (`DAT_0035a8a0` becomes 0). The list head and tail globals are left as
    /// they were, as in the original; the entries are gone.
    pub fn release(&mut self) {
        self.base = 0;
        self.entries.clear();
        self.free.clear();
        self.free_count = 0;
    }

    /// `DAT_0035a8a0`: the address entry handles count from, 0 once the pool is released.
    pub fn base(&self) -> u32 {
        self.base
    }

    /// The allocator tag `DAT_0035a8a4`.
    pub fn tag(&self) -> u32 {
        self.tag
    }

    /// The list head and tail handles (`DAT_005210c0`, `DAT_005210c4`).
    pub fn ends(&self) -> (u32, u32) {
        (self.head, self.tail)
    }

    /// The number of free entries (`+0x500`).
    pub fn free_count(&self) -> i32 {
        self.free_count
    }

    fn index(&self, entry: u32) -> Option<usize> {
        let i = (entry.wrapping_sub(self.base) >> 4) as usize;
        (entry != 0 && entry >= self.base && i < self.entries.len()).then_some(i)
    }

    fn entry(&self, handle: u32) -> Option<&Entry> {
        self.index(handle).map(|i| &self.entries[i])
    }

    /// The handle of entry `i` (`base + 16 * i`).
    pub fn handle(&self, i: usize) -> u32 {
        self.base + 16 * i as u32
    }

    /// The collision object of an entry.
    pub fn oct_of(&self, handle: u32) -> Option<&Oct> {
        self.entry(handle)?.oct.as_ref()
    }

    /// The owner of an entry.
    pub fn owner_of(&self, handle: u32) -> Option<u32> {
        self.entry(handle).map(|e| e.owner)
    }

    /// An entry's flag word.
    pub fn flags_of(&self, handle: u32) -> Option<u32> {
        self.entry(handle).map(|e| e.flags)
    }

    /// The `next` link of an entry.
    pub fn next_of(&self, handle: u32) -> Option<u32> {
        self.entry(handle).map(|e| e.next)
    }

    /// orig 0x1653d8: takes the top free index and builds an entry there for `owner` over the COCT `file`.
    ///
    /// orig 0x1650f0: clears the entry, builds the collision object with every group on and links the entry at the
    /// tail of the list. The original has no check for an empty pool or a short file; `None` here (nothing is
    /// changed) [inferred: invalid use].
    pub fn alloc(&mut self, file: &[u8], owner: u32) -> Option<u32> {
        if self.free_count <= 0 {
            return None;
        }
        let oct = Oct::new(file, u64::MAX)?;
        self.free_count -= 1;
        let idx = self.free[self.free_count as usize] as usize;
        let handle = self.handle(idx);
        {
            let e = &mut self.entries[idx];
            e.clear();
            e.owner = owner;
            e.oct = Some(oct);
            e.next = 0;
        }
        if self.tail == 0 {
            self.head = handle;
        } else if let Some(i) = self.index(self.tail) {
            self.entries[i].next = handle;
        }
        self.tail = handle;
        Some(handle)
    }

    /// orig 0x165188: unlinks an entry from the list, frees its collision object and pushes its index back.
    pub fn free(&mut self, handle: u32) {
        let Some(idx) = self.index(handle) else { return };
        let mut prev = 0;
        if handle == self.head {
            self.head = self.entries[idx].next;
        } else {
            let mut cur = self.head;
            let mut guard = 0;
            while cur != 0 && cur != handle && guard <= POOL_ENTRIES {
                prev = cur;
                cur = self.next_of(cur).unwrap_or(0);
                guard += 1;
            }
            if let Some(p) = self.index(prev) {
                self.entries[p].next = self.entries[idx].next;
            }
        }
        if handle == self.tail {
            self.tail = prev;
        }
        let e = &mut self.entries[idx];
        e.next = 0;
        e.oct = None;
        if let Some(slot) = self.free.get_mut(self.free_count as usize) {
            *slot = idx as u32;
        } else {
            self.free.push(idx as u32);
        }
        self.free_count += 1;
    }

    /// orig 0x165290: disables an entry (flag bit 0): the sweeps skip it.
    ///
    /// orig 0x1652a0: enables it again.
    pub fn set_disabled(&mut self, handle: u32, disabled: bool) {
        if let Some(i) = self.index(handle) {
            let f = &mut self.entries[i].flags;
            *f = if disabled { *f | 1 } else { *f & !1 };
        }
    }

    /// Sets or clears flag bit 1 of an entry, which hides it from the list walk. [inferred] The original has no writer
    /// of this bit in the clusters read; `FUN_00165910` is its only reader.
    pub fn set_hidden(&mut self, handle: u32, hidden: bool) {
        if let Some(i) = self.index(handle) {
            let f = &mut self.entries[i].flags;
            *f = if hidden { *f | 2 } else { *f & !2 };
        }
    }

    /// The free-index stack, bottom to top (`+0x400`, `free_count` live words).
    pub fn free_stack(&self) -> &[u32] {
        &self.free[..self.free_count.max(0) as usize]
    }

    /// orig 0x165250: places an entry's collision object with the matrix `m` (`Oct::place`).
    pub fn place(&mut self, handle: u32, m: &M4) {
        if let Some(i) = self.index(handle)
            && let Some(o) = self.entries[i].oct.as_mut()
        {
            o.place(m);
        }
    }

    /// orig 0x165910: the entry after `cur` in the list (the head for `cur` = 0) that does not have flag bit 1 set;
    /// 0 at the end of the list.
    pub fn next_visible(&self, mut cur: u32) -> u32 {
        loop {
            let next = if cur != 0 { self.next_of(cur).unwrap_or(0) } else { self.head };
            if next == 0 {
                return 0;
            }
            cur = next;
            if self.flags_of(next).unwrap_or(0) & 2 == 0 {
                return next;
            }
        }
    }

    /// The live entries `FUN_00165910` visits, in order.
    fn visible(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cur = self.next_visible(0);
        while cur != 0 && out.len() <= POOL_ENTRIES {
            out.push(cur);
            cur = self.next_visible(cur);
        }
        out
    }

    /// orig 0x165430: the collider's sphere sweep against every moving object that is not the collider's own (owner
    /// `coll +0x7c`), not disabled and not collapsed. A sweep that touches rewrites the delta (`coll +0x10`) to
    /// `+0x30 - +0x00` so the next sweep (and the map's) continues from the pushed end. Returns 1 when any touched.
    pub fn sweep_all(&self, c: &mut Collider) -> u64 {
        let mut touched = 0;
        for h in self.visible() {
            let Some(e) = self.entry(h) else { continue };
            let Some(oct) = e.oct.as_ref() else { continue };
            if e.owner == c.owner() || e.flags & 1 != 0 || oct.collapsed() {
                continue;
            }
            if oct.sweep(c) != 0 {
                touched = 1;
                c.set_v4(coll::DELTA, sub(c.v4(coll::END), c.v4(coll::START)));
            }
        }
        touched
    }

    /// orig 0x1654f8: the floor ray against every moving object that is not the collider's own and not collapsed; the
    /// highest floor (smallest y) wins. `floor` starts at the "no floor" height and takes the lowest `coll +0x68`
    /// among objects whose ray reported bit 32 (a floor too far below the foot). `coll +0x78` is set to each entry
    /// whose ray hit. Of the hits with bit 32 clear the one with the smallest end height is copied back whole
    /// (`0x84` bytes) when the walk ends. The end height is restored after every ray, so each ray starts from the same
    /// point. Returns 1 when such a hit exists.
    pub fn floor_ray_all(&self, c: &mut Collider, floor: &mut f32) -> u32 {
        let mut best = FLOOR_START;
        *floor = FLOOR_START;
        let mut snap: Option<[u8; 0x84]> = None;
        let saved_y = c.u32_at(coll::END + 4);
        for h in self.visible() {
            let Some(e) = self.entry(h) else { continue };
            let Some(oct) = e.oct.as_ref() else { continue };
            if e.owner == c.u32_at(coll::OWNER) || oct.collapsed() || !oct.floor_ray(c) {
                continue;
            }
            c.set_u32(coll::MOVING, h);
            if c.flags() & FLAG_NO_FLOOR_AHEAD != 0 {
                if c.f32_at(coll::FLOOR_Y) < *floor {
                    *floor = c.f32_at(coll::FLOOR_Y);
                }
            } else if c.f32_at(coll::END + 4) < best {
                let mut s = [0u8; 0x84];
                s.copy_from_slice(&c.0[..0x84]);
                snap = Some(s);
                best = c.f32_at(coll::END + 4);
            }
            c.set_u32(coll::END + 4, saved_y);
        }
        match snap {
            Some(s) => {
                c.0[..0x84].copy_from_slice(&s);
                1
            }
            None => 0,
        }
    }

    /// orig 0x1657d8: the segment `seg` cast against every moving object that is not `owner` and not collapsed under
    /// the attribute filter `filt` and the handler `kind`. Each hit is written to `hit` and shortens the segment's end
    /// to the hit, so later objects count only nearer hits. (The disabled flag is not tested here.)
    pub fn ray_all(&self, seg: &mut [V4; 2], hit: &mut V4, filt: Filter, kind: u32, owner: Option<u32>) -> PoolRay {
        let mut out = PoolRay { found: false, poly: 0xffff, entry: 0 };
        for h in self.visible() {
            let Some(e) = self.entry(h) else { continue };
            let Some(oct) = e.oct.as_ref() else { continue };
            if Some(e.owner) == owner || oct.collapsed() {
                continue;
            }
            let (found, poly) = oct.ray(filt, seg[0], seg[1], kind);
            if let Some(p) = found {
                out.found = true;
                *hit = p;
                seg[1] = p;
                out.entry = h;
                out.poly = poly;
            }
        }
        out
    }
}

/// orig 0x143b80: does the segment cross anything with attribute bit 0x200, on the moving objects first and then on
/// the walking map? A moving hit shortens the segment, so the map is cast against the shortened one; the hit point of
/// the last find is left in `hit`. (The original's moving-object owner argument is the caller's stack garbage; no owner is
/// excluded here [inferred].)
pub fn ray_attr_200(map: &Oct, pool: &MovingPool, seg: &mut [V4; 2], hit: &mut V4) -> bool {
    let filt = Filter { mask: 0x200, value: 0x200, invert: false, b: false };
    let moving = pool.ray_all(seg, hit, filt, 0xf, None);
    let (found, _) = map.ray(filt, seg[0], seg[1], 0xf);
    if let Some(p) = found {
        *hit = p;
    }
    moving.found || found.is_some()
}

/// orig 0x143c28: the segment cast against the second map collision (slot 0x355938, no attribute filter, handler kind
/// 0xf). `None` when the slot is empty or nothing is hit; else the hit point and polygon. (The original also stores 0
/// in a fourth output.)
pub fn ray_second(slot: Option<&Oct>, seg: &[V4; 2]) -> Option<(V4, u16)> {
    let (found, poly) = slot?.ray(Filter::ALL, seg[0], seg[1], 0xf);
    found.map(|p| (p, poly))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{BuildPoly, coct_file};

    fn slab(y: f32) -> Vec<u8> {
        coct_file(&[BuildPoly::new(&[[-100.0, y, -100.0], [100.0, y, -100.0], [100.0, y, 100.0], [-100.0, y, 100.0]], 1 | crate::flags::PARTY_COLLIDE)])
    }

    #[test]
    fn entries_are_handed_out_from_the_top_of_the_free_stack_and_linked_in_order() {
        let mut p = MovingPool::new(0x1000, 7);
        assert_eq!((p.free_count(), p.ends(), p.tag()), (64, (0, 0), 7));
        let a = p.alloc(&slab(-10.0), 1).unwrap();
        let b = p.alloc(&slab(-20.0), 2).unwrap();
        let c = p.alloc(&slab(-30.0), 3).unwrap();
        // The stack holds 0..63 with 63 on top, so the first entry is index 63.
        assert_eq!((a, b, c), (0x1000 + 16 * 63, 0x1000 + 16 * 62, 0x1000 + 16 * 61));
        assert_eq!(p.ends(), (a, c));
        assert_eq!((p.next_visible(0), p.next_visible(a), p.next_visible(b), p.next_visible(c)), (a, b, c, 0));
        // Freeing the middle entry relinks the list and pushes its index back for the next allocation.
        p.free(b);
        assert_eq!((p.next_of(a), p.free_count()), (Some(c), 62));
        assert_eq!(p.alloc(&slab(-40.0), 4), Some(b));
        assert_eq!(p.ends(), (a, b));
        // Freeing the head moves it; freeing the only entry empties the list.
        p.free(a);
        assert_eq!(p.ends(), (c, b));
        p.free(c);
        p.free(b);
        assert_eq!((p.ends(), p.free_count()), ((0, 0), 64));
    }

    #[test]
    fn the_flags_hide_and_disable_entries() {
        let mut p = MovingPool::new(0x1000, 0);
        let a = p.alloc(&slab(-10.0), 1).unwrap();
        let b = p.alloc(&slab(-20.0), 2).unwrap();
        p.set_hidden(a, true);
        assert_eq!(p.next_visible(0), b, "a hidden entry is skipped by the list walk");
        p.set_hidden(a, false);
        p.set_disabled(a, true);
        assert_eq!((p.next_visible(0), p.flags_of(a)), (a, Some(1)), "a disabled entry is still walked; the sweeps skip it");
        // A ray against the moving objects finds the nearer slab first and shortens the segment to it.
        let mut seg = [[0.0, -50.0, 0.0, 1.0], [0.0, 50.0, 0.0, 1.0]];
        let mut hit = [0.0; 4];
        let r = p.ray_all(&mut seg, &mut hit, Filter::ALL, 0xf, None);
        assert_eq!((r.found, r.entry), (true, b));
        assert!((hit[1] - -20.0).abs() < 1e-4 && seg[1] == hit);
        // The owner is skipped.
        let mut seg = [[0.0, -50.0, 0.0, 1.0], [0.0, 50.0, 0.0, 1.0]];
        let r = p.ray_all(&mut seg, &mut hit, Filter::ALL, 0xf, Some(2));
        assert_eq!(r.entry, a);
    }

    #[test]
    fn a_collapsed_object_is_reported_and_an_empty_pool_finds_nothing() {
        let mut p = MovingPool::new(0x1000, 0);
        let a = p.alloc(&slab(-10.0), 1).unwrap();
        let mut m = M4_IDENTITY;
        m[1] = [0.0, 0.05, 0.0, 0.0];
        p.place(a, &m);
        assert!(p.oct_of(a).unwrap().collapsed());
        assert!(!p.oct_of(a).unwrap().is_identity());
        let mut seg = [[0.0, -50.0, 0.0, 1.0], [0.0, 50.0, 0.0, 1.0]];
        assert!(!p.ray_all(&mut seg, &mut [0.0; 4], Filter::ALL, 0xf, None).found, "collapsed objects are skipped");
        assert_eq!(ray_second(None, &seg), None);
        assert_eq!(MovingPool::empty().next_visible(0), 0);
        p.release();
        assert_eq!(p.base(), 0);
    }
}
