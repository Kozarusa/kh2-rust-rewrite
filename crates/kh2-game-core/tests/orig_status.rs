//! Oracle tests for the status pool (0x19f450 pool init, 0x19f708 alloc, 0x19f5d8 clear, 0x19f920 load from a party record,
//! 0x1a0030 store back, 0x19ffb8 release): kh2-ee runs the original on random alloc, load, store and release sequences and the port
//! (`kh2_game_core::status_pool`) runs on the same ones; after every operation the touched block, the free stack, the count and the
//! saved word, the party records, the drive stores and the stand-in call log are compared, and at the end of a case the whole pool.
//!
//! The harness is written for this unit instead of `factory gen` because the inputs are a pool with a free stack and linked
//! records that the generator cannot lay out. It follows the same rules: the original runs from the user's executable, cases come
//! from a seeded generator (a failing case number reproduces), and a guard block audits that the original writes nothing outside
//! its arenas.
//!
//! Stand-ins (stubs), all documented here and nowhere else. Each logs its kind and arguments and the port's host logs the same:
//! - `FUN_001b50c0` (level stat row): returns a pointer into a table of random rows; the port's host returns the same row bytes.
//! - `FUN_001c0d88` (ability-set initializer), `FUN_001c0e50` (ability-set finishing pass), `FUN_001a0c08` (apply one item): do
//!   nothing but log. They belong to the ability and item unit, which is another package.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_game_core::status_pool::*;

const POOL: u32 = 0x01c8_bdb0;
const STACK_WORDS: u32 = 0x01c9_53b0;
const COUNT: u32 = 0x01c9_54b0;
const SAVED: u32 = 0x01c9_54b4;
const PARTY: u32 = 0x0180_0000;
const DRIVE: u32 = 0x0190_0000;
const ITEM: u32 = 0x01a0_0000;
const ROWS: u32 = 0x01b0_0000;
const GUARD: u32 = 0x01b8_0000;
const PARTY_H: u32 = 0x0c00_0000;
const DRIVE_H: u32 = 0x0d00_0000;
const ITEM_H: u32 = 0x0e00_0000;
const N_PARTY: u32 = 6;
const CASES: u64 = 800;

/// Log kinds: level row (member, level), ability init (block offset of a0, block), apply (block, id), finish (block).
const ROW: u32 = 1;
const INIT: u32 = 2;
const APPLY: u32 = 3;
const FINISH: u32 = 4;
type Ev = (u32, u32, u32, u32);

fn pool_addr(i: usize) -> u32 {
    POOL + i as u32 * STATUS_SIZE as u32
}

/// The block number of a pool address, or `u32::MAX`.
fn block_no(a: u32) -> u32 {
    if (POOL..POOL + POOL_SIZE as u32 * STATUS_SIZE as u32).contains(&a) { (a - POOL) / STATUS_SIZE as u32 } else { u32::MAX }
}

/// Handle to address for a pointer word; a word that is not one of the handles laid out here (old random contents) stays as it is.
fn tr(h: u32, base: u32, addr: u32, stride: u32) -> u32 {
    match h.checked_sub(base) {
        Some(d) if d % 0x1000 == 0 && d / 0x1000 < N_PARTY => addr + d / 0x1000 * stride,
        _ => h,
    }
}
fn tr_party(h: u32) -> u32 {
    tr(h, PARTY_H, PARTY, 0x100)
}
fn tr_drive(h: u32) -> u32 {
    tr(h, DRIVE_H, DRIVE, 0x1000)
}
fn tr_item(h: u32) -> u32 {
    tr(h, ITEM_H, ITEM, 0x10)
}

struct Port {
    party: Vec<PartyMember>,
    drive: Vec<DriveSave>,
    items: Vec<u16>,
    rows: Vec<[u8; 4]>,
    log: Vec<Ev>,
}

impl StatusHost for Port {
    fn party(&mut self, h: u32) -> Option<&mut PartyMember> {
        self.party.get_mut(((h.checked_sub(PARTY_H)?) / 0x1000) as usize)
    }
    fn drive(&mut self, h: u32) -> Option<&mut DriveSave> {
        self.drive.get_mut(((h.checked_sub(DRIVE_H)?) / 0x1000) as usize)
    }
    fn item_ref_id(&mut self, h: u32) -> u16 {
        self.items[((h - ITEM_H) / 0x1000) as usize]
    }
    fn level_row(&mut self, member: i32, level: u8) -> [u8; 4] {
        self.log.push((ROW, member as u32, level as u32, 0));
        self.rows[row_index(member as u32, level as u32)]
    }
    fn ability_init(&mut self, st: &mut StatusBlock) {
        self.log.push((INIT, OFF_ABILITY as u32, st_no(st), 0));
    }
    fn apply_item(&mut self, st: &mut StatusBlock, id: u16) {
        self.log.push((APPLY, st_no(st), id as u32, 0));
    }
    fn ability_finish(&mut self, st: &mut StatusBlock) {
        self.log.push((FINISH, st_no(st), 0, 0));
    }
}

thread_local! {
    /// The block number the port is working on (the host callbacks get the block by reference, not its handle).
    static CURRENT: RefCell<u32> = const { RefCell::new(0) };
}

fn st_no(_st: &StatusBlock) -> u32 {
    CURRENT.with(|c| *c.borrow())
}

fn row_index(member: u32, level: u32) -> usize {
    (member.wrapping_mul(31).wrapping_add(level) & 0xff) as usize
}

fn install_hooks(ee: &mut Ee, log: &Rc<RefCell<Vec<Ev>>>) {
    let l = log.clone();
    ee.hooks.insert(0x1b50c0, move |ee: &mut Ee| {
        let (m, lv) = (ee.gpr[4] as u32, ee.gpr[5] as u32);
        l.borrow_mut().push((ROW, m, lv, 0));
        // The port's host reads the row bytes +4..+7; the table entry holds them at those offsets.
        ee.gpr[2] = (ROWS + row_index(m, lv) as u32 * 16) as u128;
    });
    let l = log.clone();
    ee.hooks.insert(0x1c0d88, move |ee: &mut Ee| {
        let (a0, a1) = (ee.gpr[4] as u32, ee.gpr[5] as u32);
        let n = block_no(a1);
        let off = if n == u32::MAX { u32::MAX } else { a0.wrapping_sub(pool_addr(n as usize)) };
        l.borrow_mut().push((INIT, off, n, 0));
    });
    let l = log.clone();
    ee.hooks.insert(0x1a0c08, move |ee: &mut Ee| {
        let (a0, a1) = (ee.gpr[4] as u32, ee.gpr[5]);
        l.borrow_mut().push((APPLY, block_no(a0), a1 as u32, 0));
    });
    let l = log.clone();
    ee.hooks.insert(0x1c0e50, move |ee: &mut Ee| {
        let a0 = ee.gpr[4] as u32;
        // Called with the ability area: the block is 0x1cc below.
        l.borrow_mut().push((FINISH, block_no(a0.wrapping_sub(OFF_ABILITY as u32)), 0, 0));
    });
}

fn rand_float(r: &mut Rng) -> f32 {
    match r.below(6) {
        0 => 0.0,
        1 => (r.below(256) as f32) * 60.0,
        2 => (r.below(256) as f32) * 60.0 + r.below(60) as f32,
        3 => -(r.below(500) as f32),
        4 => 1.0e12,
        _ => f32::from_bits(r.word() as u32 & 0x7f7f_ffff),
    }
}

struct World {
    ee: Ee,
    log: Rc<RefCell<Vec<Ev>>>,
    port: Port,
    pool: StatusPool,
    live: Vec<u32>,
    compared: u64,
    loads: u64,
    stores: u64,
    frees: u64,
}

impl World {
    fn call(&mut self, addr: u32, args: &[u64]) -> u64 {
        self.ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
    }

    fn reset(&mut self, r: &mut Rng) {
        self.log.borrow_mut().clear();
        self.ee.write(GUARD, &[0xa5; 0x1000]);
        self.call(0x19f450, &[]);
        self.pool = StatusPool::new();
        let mut port = Port {
            party: vec![PartyMember::default(); N_PARTY as usize],
            drive: vec![DriveSave::default(); N_PARTY as usize],
            items: Vec::new(),
            rows: Vec::new(),
            log: Vec::new(),
        };
        // Random old contents in every block, so that what a clear leaves alone is compared too.
        for i in 0..POOL_SIZE {
            let bytes: Vec<u8> = (0..STATUS_SIZE).map(|_| r.word() as u8).collect();
            self.ee.write(pool_addr(i), &bytes);
            self.pool.block_mut(StatusPool::handle_of(i)).unwrap().0.copy_from_slice(&bytes);
        }
        for i in 0..256 {
            let row: [u8; 4] = std::array::from_fn(|_| r.word() as u8);
            let mut mem = [0u8; 16];
            mem[4..8].copy_from_slice(&row);
            self.ee.write(ROWS + i * 16, &mem);
            port.rows.push(row);
        }
        for i in 0..N_PARTY as usize {
            let mut rec = PartyMember::default();
            randomize_party(r, &mut rec);
            self.ee.write(PARTY + i as u32 * 0x100, &rec.0);
            port.party[i] = rec;
            let d = DriveSave { fraction: r.word() as u8, gauge: r.word() as u8, gauge_max: r.word() as u8, level: r.word() as u8, timer: rand_float(r), word: r.word() as u32 };
            let a = DRIVE + i as u32 * 0x1000;
            self.ee.write(a + 0xdd0, &[d.fraction, d.gauge, d.gauge_max, d.level]);
            self.ee.wf32(a + 0xdd4, d.timer);
            self.ee.w32(a + 0xdd8, d.word);
            port.drive[i] = d;
            let id = r.word() as u16;
            self.ee.write(ITEM + i as u32 * 0x10, &id.to_le_bytes());
            port.items.push(id);
        }
        self.port = port;
        self.live.clear();
    }

    /// Sets the pointer-like words of a block (record, item ref, drive store, member, use count, owner) on both sides.
    fn wire(&mut self, r: &mut Rng, idx: usize) {
        let h = StatusPool::handle_of(idx);
        let a = pool_addr(idx);
        let pick = |r: &mut Rng, base: u32| if r.below(4) == 0 { 0 } else { base + r.below(N_PARTY as u64) as u32 * 0x1000 };
        let (rec, item, drive) = (pick(r, PARTY_H), pick(r, ITEM_H), pick(r, DRIVE_H));
        let member = r.below(8) as i32 - 1;
        let uses = r.below(4) as u32;
        let owner = r.word() as u32 & 0xf000;
        let st = self.pool.block_mut(h).unwrap();
        st.set_record(rec);
        st.set_item_ref(item);
        st.set_drive_store(drive);
        st.set_member(member as u32);
        st.set_use_count(uses);
        st.set_owner(owner);
        for (o, v) in [(0x238, tr_party(rec)), (0x23c, tr_item(item)), (0x250, tr_drive(drive)), (0x244, member as u32), (0x248, uses), (0x24c, owner)] {
            self.ee.w32(a + o, v);
        }
        // Timers of the block: floats that survive the divide by 60, the drive timer.
        for o in [0x1bc, 0x1c0, 0x1b4] {
            let f = rand_float(r);
            self.ee.wf32(a + o, f);
            self.pool.block_mut(h).unwrap().0[o as usize..o as usize + 4].copy_from_slice(&f.to_bits().to_le_bytes());
        }
    }

    fn check_block(&mut self, idx: usize, case: u64, what: &str) {
        let a = pool_addr(idx);
        let mut mine = self.pool.block(StatusPool::handle_of(idx)).unwrap().0;
        for (o, f) in [(0x238usize, tr_party as fn(u32) -> u32), (0x23c, tr_item), (0x250, tr_drive)] {
            let v = f(u32::from_le_bytes(mine[o..o + 4].try_into().unwrap()));
            mine[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        let game: Vec<u8> = (0..STATUS_SIZE as u32).map(|i| self.ee.r8(a + i)).collect();
        if let Some(o) = (0..STATUS_SIZE).find(|&o| game[o] != mine[o]) {
            panic!("case {case} {what}: block {idx} byte +{o:#x}: game {:02x} port {:02x}", game[o], mine[o]);
        }
        self.compared += 1;
    }

    fn check_state(&mut self, case: u64, what: &str) {
        let n = self.ee.r32(COUNT) as i32;
        assert_eq!(n, self.pool.free_count(), "case {case} {what}: free count");
        let game: Vec<i32> = (0..n.clamp(0, 64) as u32).map(|i| self.ee.r32(STACK_WORDS + i * 4) as i32).collect();
        assert_eq!(game, self.pool.free_stack(), "case {case} {what}: free stack");
        assert_eq!(self.ee.r32(SAVED), self.pool.saved, "case {case} {what}: saved word");
        for i in 0..N_PARTY as usize {
            let g: Vec<u8> = (0..0xf4).map(|k| self.ee.r8(PARTY + i as u32 * 0x100 + k)).collect();
            assert_eq!(g, self.port.party[i].0.to_vec(), "case {case} {what}: party record {i}");
            let a = DRIVE + i as u32 * 0x1000;
            let d = &self.port.drive[i];
            let g = (self.ee.r8(a + 0xdd0), self.ee.r8(a + 0xdd1), self.ee.r8(a + 0xdd2), self.ee.r8(a + 0xdd3), self.ee.r32(a + 0xdd4), self.ee.r32(a + 0xdd8));
            assert_eq!(g, (d.fraction, d.gauge, d.gauge_max, d.level, d.timer.to_bits(), d.word), "case {case} {what}: drive store {i}");
        }
        let game = std::mem::take(&mut *self.log.borrow_mut());
        let mine = std::mem::take(&mut self.port.log);
        assert_eq!(game, mine, "case {case} {what}: stand-in calls (kind, a, b, c) game vs port");
    }
}

fn randomize_party(r: &mut Rng, rec: &mut PartyMember) {
    for b in rec.0.iter_mut() {
        *b = r.word() as u8;
    }
    // Slot counts stay inside the record (armor and accessory arrays hold 8 each, and the armor count may run on into the accessories).
    rec.0[0x10] = r.below(10) as u8;
    rec.0[0x11] = r.below(9) as u8;
    if r.below(3) == 0 {
        rec.0[4] = rec.0[5];
    }
    if r.below(3) == 0 {
        rec.0[6] = rec.0[7].wrapping_sub(r.below(3) as u8);
    }
    for k in [0x14usize, 0x16, 0x24, 0x26, 0, 2] {
        if r.below(4) == 0 {
            rec.0[k] = 0;
            rec.0[k + 1] = 0;
        }
    }
}

fn run() {
    let Some(elf) = kh2_factory::exe_or_skip("status pool oracle") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let log = Rc::new(RefCell::new(Vec::new()));
    install_hooks(&mut ee, &log);
    let port = Port { party: vec![], drive: vec![], items: vec![], rows: vec![], log: vec![] };
    let mut w = World { ee, log, port, pool: StatusPool::new(), live: Vec::new(), compared: 0, loads: 0, stores: 0, frees: 0 };
    let mut r = Rng::new(0x57a705);
    for case in 0..CASES {
        w.reset(&mut r);
        w.check_state(case, "after pool init");
        for _ in 0..4 + r.below(50) {
            match r.below(10) {
                0..=2 => {
                    if w.pool.free_count() == 0 {
                        continue;
                    }
                    let e = w.call(0x19f708, &[]) as u32;
                    // The host callbacks name the block by this number: the one that comes off the stack.
                    CURRENT.with(|c| *c.borrow_mut() = *w.pool.free_stack().last().unwrap() as u32);
                    let h = w.pool.alloc(&mut w.port).unwrap();
                    let idx = StatusPool::index_of(h).unwrap();
                    assert_eq!(e, pool_addr(idx), "case {case}: alloc returns the block");
                    w.live.push(h);
                    w.check_block(idx, case, "after alloc");
                    w.check_state(case, "after alloc");
                    w.wire(&mut r, idx);
                }
                3..=5 => {
                    let Some(&h) = w.live.get(r.below(w.live.len().max(1) as u64) as usize) else { continue };
                    let idx = StatusPool::index_of(h).unwrap();
                    CURRENT.with(|c| *c.borrow_mut() = idx as u32);
                    // Sometimes a record of its own at a spare address, sometimes the block's own (argument 0).
                    let (rec, rec_a) = if r.below(3) == 0 {
                        (None, 0)
                    } else {
                        let mut rec = PartyMember::default();
                        randomize_party(&mut r, &mut rec);
                        let a = PARTY + (N_PARTY + 1) * 0x100;
                        w.ee.write(a, &rec.0);
                        (Some(rec), a)
                    };
                    w.call(0x19f920, &[pool_addr(idx) as u64, rec_a as u64]);
                    w.pool.load_party(h, rec.as_ref(), &mut w.port);
                    w.loads += 1;
                    w.check_block(idx, case, "after load");
                    w.check_state(case, "after load");
                }
                6 | 7 => {
                    let Some(&h) = w.live.get(r.below(w.live.len().max(1) as u64) as usize) else { continue };
                    let idx = StatusPool::index_of(h).unwrap();
                    w.call(0x1a0030, &[pool_addr(idx) as u64]);
                    w.pool.store_party(h, &mut w.port);
                    w.stores += 1;
                    w.check_block(idx, case, "after store");
                    w.check_state(case, "after store");
                }
                _ => {
                    if w.live.is_empty() {
                        continue;
                    }
                    let k = r.below(w.live.len() as u64) as usize;
                    let h = w.live[k];
                    let idx = StatusPool::index_of(h).unwrap();
                    w.call(0x19ffb8, &[pool_addr(idx) as u64]);
                    w.pool.release(h, &mut w.port);
                    w.frees += 1;
                    if w.pool.free_stack().contains(&(idx as i32)) {
                        w.live.remove(k);
                    }
                    w.check_block(idx, case, "after release");
                    w.check_state(case, "after release");
                }
            }
        }
        for i in 0..POOL_SIZE {
            w.check_block(i, case, "end of case");
        }
        let guard: Vec<u8> = (0..0x1000u32).map(|i| w.ee.r8(GUARD + i)).collect();
        assert!(guard.iter().all(|&b| b == 0xa5), "case {case}: the original wrote outside its arenas");
    }
    eprintln!("status pool: {CASES} cases, {} loads, {} stores, {} releases, {} blocks compared", w.loads, w.stores, w.frees, w.compared);
    assert!(w.loads > 1000 && w.stores > 500 && w.frees > 500, "the generator exercises every operation");
}

/// One generated run compares every function of the unit; each address has its own test name so the ledger marks it verified.
#[test]
fn orig_19f450_matches_the_game() {
    run();
}

#[test]
fn orig_19f708_matches_the_game() {
    run();
}

#[test]
fn orig_19f5d8_matches_the_game() {
    run();
}

#[test]
fn orig_19f920_matches_the_game() {
    run();
}

#[test]
fn orig_1a0030_matches_the_game() {
    run();
}

#[test]
fn orig_19ffb8_matches_the_game() {
    run();
}
