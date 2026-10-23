//! The status pool: 64 status blocks of 0x258 bytes (core runtime, WP-L1-P14; FAC-8 L1-c).
//!
//! The original keeps the blocks in one array (0x1c8bdb0, stride 0x258) and the free block numbers on a stack (0x1c953b0, 64 words,
//! count word at 0x1c954b0), plus one saved word at 0x1c954b4 that the drive-state save and load pass between each other. A block is
//! taken with [`StatusPool::alloc`] (`FUN_0019f708`, which clears it with `FUN_0019f5d8`), filled from a party record
//! ([`StatusBlock::load_party`], `FUN_0019f920`), written back ([`StatusBlock::store_party`], `FUN_001a0030`) and given up with
//! [`StatusPool::release`] (`FUN_0019ffb8`, which writes back and frees the block once its use count falls below 1).
//!
//! Handles: block `n` is `STATUS_BASE + n * 0x1000` (the FAC-8 form, 0 is null). The pointer fields of a block (+0x238 party record,
//! +0x23c, +0x24c owner object, +0x250 drive store) hold handles; the things they point at belong to other units, so the functions
//! reach them through a [`StatusHost`]. The same trait stands for the callees of the unit that are not ported here: the level
//! stat row (`FUN_001b50c0`), the ability-set code (`FUN_001c0d88`, `FUN_001c0e50`) and the per-item stat application
//! (`FUN_001a0c08`). Nothing in the app uses this module yet.

/// Bytes of one status block.
pub const STATUS_SIZE: usize = 0x258;
/// Blocks in the pool.
pub const POOL_SIZE: usize = 64;
/// First status handle; block `n` is `STATUS_BASE + n * HANDLE_STRIDE`. [inferred] an unused base chosen by the port.
pub const STATUS_BASE: u32 = 0x0b00_0000;
/// Distance between two status handles.
pub const HANDLE_STRIDE: u32 = 0x1000;
/// The EE constant the party record's two timer bytes are scaled by (and divided by when stored back): 60.0 at 0x37b418/0x37b41c.
pub const TIMER_SCALE: f32 = 60.0;
/// The ability-set area inside a status block (+0x1cc, 0x6c bytes up to the party record word at +0x238).
pub const OFF_ABILITY: usize = 0x1cc;

fn get(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn put(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

fn get16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}

fn put16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}

/// A status block (0x258 bytes with the original's offsets).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StatusBlock(pub [u8; STATUS_SIZE]);

impl Default for StatusBlock {
    fn default() -> Self {
        StatusBlock([0; STATUS_SIZE])
    }
}

macro_rules! words {
    ($($(#[$m:meta])* $get:ident / $set:ident @ $off:expr),* $(,)?) => {$(
        $(#[$m])*
        pub fn $get(&self) -> u32 {
            get(&self.0, $off)
        }
        pub fn $set(&mut self, v: u32) {
            put(&mut self.0, $off, v)
        }
    )*};
}

impl StatusBlock {
    words! {
        hp / set_hp @ 0x0,
        hp_max / set_hp_max @ 0x4,
        /// A word `FUN_0019f920` zeroes and the drive load/store pair carries through the pool's saved word.
        word8 / set_word8 @ 0x8,
        mp / set_mp @ 0x180,
        mp_max / set_mp_max @ 0x184,
        /// Drive timer bits (f32 at +0x1b4).
        drive_timer_bits / set_drive_timer_bits @ 0x1b4,
        word_1b8 / set_word_1b8 @ 0x1b8,
        /// The first party record timer (f32 at +0x1bc).
        timer_1bc_bits / set_timer_1bc_bits @ 0x1bc,
        charge_timer_bits / set_charge_timer_bits @ 0x1c0,
        /// The party record handle (+0x238).
        record / set_record @ 0x238,
        /// A handle to a record whose first u16 is an item id applied by the load (+0x23c).
        item_ref / set_item_ref @ 0x23c,
        member / set_member @ 0x244,
        use_count / set_use_count @ 0x248,
        /// The owner object handle (+0x24c).
        owner / set_owner @ 0x24c,
        /// The drive store handle (+0x250).
        drive_store / set_drive_store @ 0x250,
    }

    pub fn strength(&self) -> u16 {
        get16(&self.0, 0x188)
    }
    pub fn magic(&self) -> u16 {
        get16(&self.0, 0x18a)
    }
    pub fn defense(&self) -> u16 {
        get16(&self.0, 0x18c)
    }
    pub fn ap(&self) -> u16 {
        get16(&self.0, 0x18e)
    }
    pub fn resist(&self, element: usize) -> u8 {
        self.0[0x1a8 + element]
    }
    pub fn drive_fraction(&self) -> u8 {
        self.0[0x1b0]
    }
    pub fn drive_gauge(&self) -> u8 {
        self.0[0x1b1]
    }
    pub fn drive_gauge_max(&self) -> u8 {
        self.0[0x1b2]
    }
    pub fn drive_level(&self) -> u8 {
        self.0[0x1b3]
    }
    pub fn drive_timer(&self) -> f32 {
        f32::from_bits(self.drive_timer_bits())
    }

    /// Resets the block to the empty state: the first 0x180 bytes, the stat and drive words and the ability words cleared, the two
    /// 99999/1 pairs, six resist bytes and the general one at 100, no member (-1), use count 1, no handles. The drive level byte
    /// (+0x1b3) and the ability area beyond its first 8 words are left alone; the ability-set code then initializes its own area.
    ///
    /// orig 0x19f5d8
    pub fn clear(&mut self, host: &mut impl StatusHost) {
        let b = &mut self.0;
        b[..0x180].fill(0);
        for o in [0x180, 0x184] {
            put(b, o, 0);
        }
        for o in [0x188, 0x18a, 0x18c, 0x18e, 0x190, 0x192, 0x194, 0x196] {
            put16(b, o, 0);
        }
        put(b, 0x1a0, 99999);
        put(b, 0x1a4, 1);
        put(b, 0x198, 99999);
        put(b, 0x19c, 1);
        for o in [0x1af, 0x1b0, 0x1b1, 0x1b2] {
            b[o] = 0;
        }
        for o in [0x1b4, 0x1b8, 0x1bc, 0x1c0, 0x1c4, 0x1c8] {
            put(b, o, 0);
        }
        b[OFF_ABILITY..OFF_ABILITY + 0x20].fill(0);
        put(b, 0x238, 0);
        put(b, 0x244, u32::MAX);
        put(b, 0x23c, 0);
        put(b, 0x240, 0);
        put(b, 0x248, 1);
        put(b, 0x24c, 0);
        put(b, 0x250, 0);
        put(b, 0x254, 0);
        b[0x1a8..0x1af].fill(100);
        host.ability_init(self);
    }

    /// Fills the block from a party record: HP and MP (current capped by the maximum), the four stats as the level row plus the
    /// record value (twice: +0x188.. and +0x190..), the two timers (record bytes +0xc, +0xd times 60), the drive state from the
    /// drive store when the block has one, the ability set rebuilt from the equipment (two weapons, the +0x23c item, the armor and
    /// accessory slots) and the resists back to 100. `record` is the record to use; `None` takes the block's own (+0x238) and a block
    /// without one does nothing. `saved` is the pool's saved word (0x1c954b4).
    ///
    /// orig 0x19f920
    pub fn load_party(&mut self, record: Option<&PartyMember>, saved: u32, host: &mut impl StatusHost) {
        let own;
        let rec = match record {
            Some(r) => r,
            None => match host.party(self.record()).cloned() {
                Some(r) => {
                    own = r;
                    &own
                }
                None => return,
            },
        };
        let row = host.level_row(self.member() as i32, rec.level());
        let drive = self.drive_store();
        self.set_hp(rec.hp_max() as u32);
        self.set_hp_max(rec.hp_max() as u32);
        self.set_word8(0);
        self.set_hp(rec.hp().min(rec.hp_max()) as u32);
        self.set_mp(rec.mp().min(rec.mp_max()) as u32);
        self.set_mp_max(rec.mp_max() as u32);
        let b = &mut self.0;
        for (k, (o, v)) in [(0x188, rec.strength()), (0x18a, rec.magic()), (0x18c, rec.defense()), (0x18e, rec.ap())].into_iter().enumerate() {
            let s = row[k] as u16 + v as u16;
            put16(b, o, s);
            put16(b, o + 8, s);
        }
        put(b, 0x1bc, (rec.timer_c() as f32 * TIMER_SCALE).to_bits());
        put(b, 0x1c0, (rec.timer_d() as f32 * TIMER_SCALE).to_bits());
        if drive != 0 {
            if let Some(d) = host.drive(drive) {
                b[0x1b0] = d.fraction;
                b[0x1b1] = d.gauge;
                b[0x1b2] = d.gauge_max;
                b[0x1b3] = d.level;
                put(b, 0x1b4, d.timer.to_bits());
                put(b, 0x1b8, d.word);
            }
            put(b, 0x8, saved);
        }
        host.ability_init(self);
        self.0[0x1a8..0x1af].fill(100);
        host.apply_item(self, rec.weapon(0));
        host.apply_item(self, rec.weapon(1));
        let item = self.item_ref();
        if item != 0 {
            let id = host.item_ref_id(item);
            host.apply_item(self, id);
        }
        for i in 0..rec.armor_slots() as usize {
            host.apply_item(self, rec.armor(i));
        }
        for i in 0..rec.accessory_slots() as usize {
            host.apply_item(self, rec.accessory(i));
        }
        host.ability_finish(self);
    }

    /// Writes the block back: the record gets HP, HP max, MP, MP max (low bytes) and the two timers divided by 60 and truncated; the
    /// drive store, when there is one, gets the drive state, and the block's +0x8 word goes to the pool's saved word (returned).
    ///
    /// orig 0x1a0030
    pub fn store_party(&self, saved: u32, host: &mut impl StatusHost) -> u32 {
        let mut saved = saved;
        let record = self.record();
        if let Some(r) = host.party(record).filter(|_| record != 0) {
            r.set_hp(self.hp() as u8);
            r.set_hp_max(self.hp_max() as u8);
            r.set_mp(self.mp() as u8);
            r.set_mp_max(self.mp_max() as u8);
            r.set_timer_c((f32::from_bits(self.timer_1bc_bits()) / TIMER_SCALE) as i32 as u8);
            r.set_timer_d((f32::from_bits(self.charge_timer_bits()) / TIMER_SCALE) as i32 as u8);
        }
        if self.drive_store() != 0 {
            if let Some(d) = host.drive(self.drive_store()) {
                d.fraction = self.drive_fraction();
                d.gauge = self.drive_gauge();
                d.gauge_max = self.drive_gauge_max();
                d.level = self.drive_level();
                d.timer = self.drive_timer();
                d.word = self.word_1b8();
            }
            saved = self.word8();
        }
        saved
    }
}

/// A party member record (0xf4 bytes). Only the fields the status code reads and writes have accessors.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PartyMember(pub [u8; 0xf4]);

impl Default for PartyMember {
    fn default() -> Self {
        PartyMember([0; 0xf4])
    }
}

impl PartyMember {
    /// The two weapon item ids (u16 at +0 and +2).
    pub fn weapon(&self, i: usize) -> u16 {
        get16(&self.0, i * 2)
    }
    pub fn set_weapon(&mut self, i: usize, v: u16) {
        put16(&mut self.0, i * 2, v)
    }
    pub fn hp(&self) -> u8 {
        self.0[4]
    }
    pub fn set_hp(&mut self, v: u8) {
        self.0[4] = v
    }
    pub fn hp_max(&self) -> u8 {
        self.0[5]
    }
    pub fn set_hp_max(&mut self, v: u8) {
        self.0[5] = v
    }
    pub fn mp(&self) -> u8 {
        self.0[6]
    }
    pub fn set_mp(&mut self, v: u8) {
        self.0[6] = v
    }
    pub fn mp_max(&self) -> u8 {
        self.0[7]
    }
    pub fn set_mp_max(&mut self, v: u8) {
        self.0[7] = v
    }
    pub fn ap(&self) -> u8 {
        self.0[8]
    }
    pub fn strength(&self) -> u8 {
        self.0[9]
    }
    pub fn magic(&self) -> u8 {
        self.0[0xa]
    }
    pub fn defense(&self) -> u8 {
        self.0[0xb]
    }
    /// The byte at +0xc, kept in seconds by the status block as a 60-based float timer.
    pub fn timer_c(&self) -> u8 {
        self.0[0xc]
    }
    pub fn set_timer_c(&mut self, v: u8) {
        self.0[0xc] = v
    }
    pub fn timer_d(&self) -> u8 {
        self.0[0xd]
    }
    pub fn set_timer_d(&mut self, v: u8) {
        self.0[0xd] = v
    }
    pub fn level(&self) -> u8 {
        self.0[0xf]
    }
    pub fn armor_slots(&self) -> u8 {
        self.0[0x10]
    }
    pub fn accessory_slots(&self) -> u8 {
        self.0[0x11]
    }
    /// Armor id `i` (u16 at +0x14 + 2i; reads on into the accessory array like the original when the count exceeds 8).
    pub fn armor(&self, i: usize) -> u16 {
        get16(&self.0, 0x14 + 2 * i)
    }
    pub fn accessory(&self, i: usize) -> u16 {
        get16(&self.0, 0x24 + 2 * i)
    }
}

/// The drive state the status block copies to and from its drive store (store bytes +0xdd0..+0xddc).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct DriveSave {
    pub fraction: u8,
    pub gauge: u8,
    pub gauge_max: u8,
    pub level: u8,
    pub timer: f32,
    pub word: u32,
}

/// The things the status code reaches that other units own, plus the callees that are not ported here.
pub trait StatusHost {
    /// The party record a handle names.
    fn party(&mut self, h: u32) -> Option<&mut PartyMember>;
    /// The drive store a handle names.
    fn drive(&mut self, h: u32) -> Option<&mut DriveSave>;
    /// The first u16 of the record the +0x23c handle names (an item id).
    fn item_ref_id(&mut self, h: u32) -> u16;
    /// The level-up stat row for a member and level: its bytes +4..+7 (strength, magic, defense, ap), orig 0x1b50c0.
    fn level_row(&mut self, member: i32, level: u8) -> [u8; 4];
    /// The ability-set initializer on the block's ability area, orig 0x1c0d88 (stood in for).
    fn ability_init(&mut self, st: &mut StatusBlock);
    /// Applies one item's stat changes and ability to the block (nothing for id 0), orig 0x1a0c08 (stood in for).
    fn apply_item(&mut self, st: &mut StatusBlock, id: u16);
    /// The ability-set finishing pass over the equipped abilities, orig 0x1c0e50 (stood in for).
    fn ability_finish(&mut self, st: &mut StatusBlock);
}

/// The pool: 64 blocks, the free stack, and the saved word.
pub struct StatusPool {
    blocks: Vec<StatusBlock>,
    stack: [i32; POOL_SIZE],
    count: i32,
    /// The word at 0x1c954b4.
    pub saved: u32,
}

impl Default for StatusPool {
    fn default() -> Self {
        StatusPool::new()
    }
}

impl StatusPool {
    /// A pool in the state `FUN_0019f450` leaves: stack 0..63, count 64, saved word 0; blocks zeroed.
    ///
    /// orig 0x19f450
    pub fn new() -> StatusPool {
        StatusPool { blocks: vec![StatusBlock::default(); POOL_SIZE], stack: std::array::from_fn(|i| i as i32), count: POOL_SIZE as i32, saved: 0 }
    }

    pub fn handle_of(index: usize) -> u32 {
        STATUS_BASE + index as u32 * HANDLE_STRIDE
    }

    /// The pool index of a handle, `None` for 0 or a value outside the pool.
    pub fn index_of(h: u32) -> Option<usize> {
        let d = h.checked_sub(STATUS_BASE)?;
        let i = (d / HANDLE_STRIDE) as usize;
        (d % HANDLE_STRIDE == 0 && i < POOL_SIZE).then_some(i)
    }

    pub fn block(&self, h: u32) -> Option<&StatusBlock> {
        self.blocks.get(Self::index_of(h)?)
    }

    pub fn block_mut(&mut self, h: u32) -> Option<&mut StatusBlock> {
        self.blocks.get_mut(Self::index_of(h)?)
    }

    /// Blocks on the free stack (the count word).
    pub fn free_count(&self) -> i32 {
        self.count
    }

    /// The free stack as it stands: the first `free_count` entries, bottom first.
    pub fn free_stack(&self) -> Vec<i32> {
        self.stack[..self.count.clamp(0, POOL_SIZE as i32) as usize].to_vec()
    }

    /// Takes the block on top of the free stack and clears it; `None` when the stack is empty (the original reads below the stack).
    ///
    /// orig 0x19f708
    pub fn alloc(&mut self, host: &mut impl StatusHost) -> Option<u32> {
        if self.count <= 0 {
            return None;
        }
        self.count -= 1;
        let i = self.stack[self.count as usize] as usize;
        self.blocks[i].clear(host);
        Some(Self::handle_of(i))
    }

    /// Writes the block back to its record and drive store, takes one use away, and puts the block on the free stack when the use
    /// count has fallen below 1 (compared as a signed number).
    ///
    /// orig 0x19ffb8
    pub fn release(&mut self, h: u32, host: &mut impl StatusHost) {
        let Some(i) = Self::index_of(h) else { return };
        self.saved = self.blocks[i].store_party(self.saved, host);
        let n = self.blocks[i].use_count().wrapping_sub(1);
        self.blocks[i].set_use_count(n);
        if (n as i32) < 1 && (self.count as usize) < POOL_SIZE {
            self.stack[self.count as usize] = i as i32;
            self.count += 1;
        }
    }

    /// Writes a block back to its record and drive store (see [`StatusBlock::store_party`]), updating the saved word.
    pub fn store_party(&mut self, h: u32, host: &mut impl StatusHost) {
        let saved = self.saved;
        if let Some(b) = self.block(h) {
            self.saved = b.store_party(saved, host);
        }
    }

    /// Fills a block from a party record (see [`StatusBlock::load_party`]).
    pub fn load_party(&mut self, h: u32, record: Option<&PartyMember>, host: &mut impl StatusHost) {
        let saved = self.saved;
        if let Some(b) = self.block_mut(h) {
            b.load_party(record, saved, host);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Host {
        rec: PartyMember,
        drive: DriveSave,
        log: Vec<u16>,
    }

    impl StatusHost for Host {
        fn party(&mut self, _h: u32) -> Option<&mut PartyMember> {
            Some(&mut self.rec)
        }
        fn drive(&mut self, _h: u32) -> Option<&mut DriveSave> {
            Some(&mut self.drive)
        }
        fn item_ref_id(&mut self, _h: u32) -> u16 {
            0x77
        }
        fn level_row(&mut self, _m: i32, _l: u8) -> [u8; 4] {
            [1, 2, 3, 4]
        }
        fn ability_init(&mut self, _st: &mut StatusBlock) {}
        fn apply_item(&mut self, _st: &mut StatusBlock, id: u16) {
            self.log.push(id);
        }
        fn ability_finish(&mut self, _st: &mut StatusBlock) {}
    }

    #[test]
    fn alloc_pops_the_stack_top_and_release_pushes_it_back() {
        let mut p = StatusPool::new();
        let mut h = Host::default();
        let a = p.alloc(&mut h).unwrap();
        let b = p.alloc(&mut h).unwrap();
        assert_eq!((a, b), (StatusPool::handle_of(63), StatusPool::handle_of(62)));
        assert_eq!(p.block(a).unwrap().use_count(), 1);
        assert_eq!(p.block(a).unwrap().member() as i32, -1);
        p.release(a, &mut h);
        assert_eq!(p.free_count(), 63);
        assert_eq!(p.alloc(&mut h), Some(a));
    }

    #[test]
    fn a_shared_block_is_freed_by_the_last_release() {
        let mut p = StatusPool::new();
        let mut h = Host::default();
        let a = p.alloc(&mut h).unwrap();
        p.block_mut(a).unwrap().set_use_count(2);
        p.release(a, &mut h);
        assert_eq!(p.free_count(), 63);
        p.release(a, &mut h);
        assert_eq!(p.free_count(), 64);
    }

    #[test]
    fn load_applies_the_equipment_in_order_and_store_scales_the_timers() {
        let mut p = StatusPool::new();
        let mut h = Host::default();
        h.rec.0[5] = 50;
        h.rec.0[4] = 60;
        h.rec.0[0xc] = 3;
        h.rec.0[0x10] = 1;
        h.rec.0[0x11] = 2;
        h.rec.set_weapon(0, 10);
        h.rec.set_weapon(1, 11);
        h.rec.0[0x14] = 20;
        h.rec.0[0x24] = 30;
        h.rec.0[0x26] = 31;
        let a = p.alloc(&mut h).unwrap();
        p.block_mut(a).unwrap().set_item_ref(0x1234);
        p.block_mut(a).unwrap().set_record(1);
        let rec = h.rec.clone();
        p.load_party(a, Some(&rec), &mut h);
        let st = p.block(a).unwrap();
        assert_eq!((st.hp(), st.hp_max()), (50, 50));
        assert_eq!(st.resist(3), 100);
        assert_eq!(h.log, vec![10, 11, 0x77, 20, 30, 31]);
        assert_eq!(st.timer_1bc_bits(), 180f32.to_bits());
        h.rec.0[0xc] = 0;
        p.release(a, &mut h);
        assert_eq!(h.rec.0[0xc], 3);
    }
}
