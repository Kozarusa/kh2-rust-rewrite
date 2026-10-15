//! The target search of host 6.5 (`kh2_bdx::search`) against the game's own FUN_002f04a8, run in the
//! kh2-ee emulator on the user's executable. Every routine it calls is the game's own: the object list
//! walk (FUN_001b7c88 from the head 0x1d5d7a0, +0x8b4 the next object), the object tests (FUN_002f0218,
//! FUN_002f03f8 -> FUN_0019a5d0), the part walk (FUN_0016b4e0 over the collision list at +0x8d8, type 3,
//! FUN_0019a6a0's enable mask), the part point (FUN_001894d0 with bone 0x4002, the object's matrix +0x6b0),
//! the angle (FUN_003003e8, FUN_0017bbc0), the record (FUN_0018ed40 / FUN_0018f490) and the history
//! (FUN_002f0438 -> FUN_002f02f8). The scenes are random objects laid out in emulator memory; the port gets
//! the same objects through a model of the object tests (the host's part in the app) and must give the
//! same record and the same block bytes (history, counts, slot).
//!
//! Skips without KH2_ISO.

use kh2_bdx::HostTable;
use kh2_bdx::search::{Block, SearchConsts, TargetPart, block_size, search};
use kh2_ee::Ee;

const SEARCH: u32 = 0x2f04a8;
const LIST_HEAD: u32 = 0x1d5d7a0;
const OBJS: u32 = 0x0180_0000;
const OBJ_SIZE: u32 = 0x1000;
const CLASS: u32 = 0x0188_0000;
const PARTS: u32 = 0x0189_0000;
const BLOCK: u32 = 0x0190_0000;
const RECORD: u32 = 0x0190_1000;
const POINT: u32 = 0x0190_2000;
const DIR: u32 = 0x0190_2010;
const STACK: u32 = 0x01f0_0000;

fn elf() -> Option<Vec<u8>> {
    let path = std::env::var("KH2_ISO").ok()?;
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())
}

struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12345);
        self.0 >> 8
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
    fn chance(&mut self, pct: u32) -> bool {
        self.below(100) < pct
    }
    fn coord(&mut self) -> f32 {
        self.below(1200) as f32 - 600.0
    }
}

struct Part {
    id: u8,
    kind: u8,
    off: [i16; 3],
    lift: i16,
}

struct Obj {
    flags108: u32,
    flags588: u32,
    class_kind: u8,
    flags7f8: u32,
    enable: u32,
    at: [f32; 3],
    parts: Vec<Part>,
}

fn scene(r: &mut Rng) -> Vec<Obj> {
    (0..1 + r.below(5))
        .map(|_| {
            let parts = (0..1 + r.below(3))
                .map(|i| Part { id: i as u8 + r.below(3) as u8, kind: if r.chance(80) { 3 } else { 2 }, off: [r.below(60) as i16 - 30, r.below(60) as i16 - 30, r.below(60) as i16 - 30], lift: r.below(40) as i16 })
                .collect();
            Obj {
                flags108: if r.chance(15) { 0x8000 } else { 0 } | if r.chance(10) { 0x20 } else { 0 } | if r.chance(8) { 0x80000 } else { 0 },
                flags588: if r.chance(70) { 0x10 } else { 0 } | if r.chance(25) { 4 } else { 0 },
                class_kind: [2, 3, 2, 1, 4][r.below(5) as usize],
                flags7f8: if r.chance(90) { 0x400 } else { 0 },
                enable: if r.chance(85) { u32::MAX } else { r.next() },
                at: [r.coord(), r.coord() / 4.0, r.coord()],
                parts,
            }
        })
        .collect()
}

/// The host's side in the app, as a model of FUN_001b7c88 / FUN_002f0218 / FUN_002f03f8 /
/// FUN_0019a5d0 / FUN_0016b4e0 / FUN_001894d0 on the scene.
fn host_parts(objs: &[Obj], kind: i32, skip_flagged: bool) -> Vec<TargetPart> {
    let mut out = Vec::new();
    for (k, o) in objs.iter().enumerate() {
        if o.flags108 & 0x1008_0000 != 0 {
            continue;
        }
        let kind_ok = match kind {
            0 => o.flags588 & 0x10 != 0,
            1 | 2 => o.flags588 & 4 != 0,
            _ => true,
        };
        if !kind_ok || skip_flagged && o.flags108 & 0x8000 != 0 {
            continue;
        }
        let parts: Vec<&Part> = o.parts.iter().filter(|p| p.kind == 3 && o.enable & 1 << (p.id & 31) != 0).collect();
        let targetable = kind == 2 || o.flags588 & 4 != 0 || !matches!(o.class_kind, 0 | 1 | 5) && o.flags7f8 & 0x400 != 0 && o.flags108 & 0x20 == 0 && !parts.is_empty();
        if !targetable {
            continue;
        }
        for p in parts {
            let point = [o.at[0] + p.off[0] as f32, o.at[1] + p.off[1] as f32 - p.lift as f32, o.at[2] + p.off[2] as f32, 1.0];
            out.push(TargetPart { obj: OBJS + k as u32 * OBJ_SIZE, part: p.id, point });
        }
    }
    out
}

fn lay_out(ee: &mut Ee, objs: &[Obj]) {
    ee.w32(LIST_HEAD, if objs.is_empty() { 0 } else { OBJS });
    for (k, o) in objs.iter().enumerate() {
        let base = OBJS + k as u32 * OBJ_SIZE;
        ee.write(base, &[0u8; OBJ_SIZE as usize]);
        let next = if k + 1 < objs.len() { base + OBJ_SIZE } else { 0 };
        ee.w32(base + 0x8b4, next);
        ee.w32(base + 0x108, o.flags108);
        ee.w32(base + 0x588, o.flags588);
        ee.w32(base + 0x7f8, o.flags7f8);
        let class = CLASS + k as u32 * 0x10;
        ee.write(class, &[0u8; 0x10]);
        ee.write(class + 4, &[o.class_kind]);
        ee.w32(base + 8, class);
        // The object's matrix (+0x6b0): a translation.
        let m = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, o.at[0], o.at[1], o.at[2], 1.0];
        for (i, v) in m.iter().enumerate() {
            ee.wf32(base + 0x6b0 + 4 * i as u32, *v);
        }
        // The collision list: count, enable mask, records of 20 bytes from +0x40.
        let list = PARTS + k as u32 * 0x200;
        ee.write(list, &[0u8; 0x200]);
        ee.w32(list, o.parts.len() as u32);
        ee.w32(list + 4, o.enable);
        for (i, p) in o.parts.iter().enumerate() {
            let r = list + 0x40 + 20 * i as u32;
            ee.write(r, &[p.id, 0, 0, 0, p.kind, 3]);
            ee.write(r + 6, &0x4002u16.to_le_bytes());
            for (j, v) in p.off.iter().chain([&p.lift, &20, &20]).enumerate() {
                ee.write(r + 8 + 2 * j as u32, &v.to_le_bytes());
            }
        }
        ee.w32(base + 0x8d8, list);
    }
}

fn random_block(r: &mut Rng, k: &SearchConsts, objs: &[Obj]) -> Block {
    let mut b = Block::new(1 + r.below(5) as i32, k);
    b.kind = [0, 0, 0, 1, 2, 3][r.below(6) as usize];
    b.flags = r.chance(50) as u16;
    b.min = if r.chance(50) { 0.0 } else { r.below(200) as f32 };
    b.max = if r.chance(30) { k.max } else { 200.0 + r.below(800) as f32 };
    b.angle = if r.chance(50) { std::f32::consts::TAU } else { 0.2 + r.below(250) as f32 / 100.0 };
    b.count = r.below(b.cap as u32 + 1) as i32;
    b.next = r.below(b.cap as u32) as u8;
    b.walked = r.below(9) as u8;
    for e in b.history.iter_mut() {
        let k = r.below(objs.len().max(1) as u32);
        *e = (OBJS + k * OBJ_SIZE, r.below(4) as i32);
    }
    b
}

#[test]
fn search_matches_fun_002f04a8() {
    let Some(elf) = elf() else {
        eprintln!("KH2_ISO not set: skipped");
        return;
    };
    let k = HostTable::from_exe(&elf).and_then(|t| t.search).expect("the search numbers");
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut r = Rng(0x5eed_1234);
    let (mut found, mut cases) = (0, 0);
    for case in 0..400 {
        let objs = scene(&mut r);
        lay_out(&mut ee, &objs);
        let mut block = random_block(&mut r, &k, &objs);
        let point = [r.coord(), r.coord() / 4.0, r.coord(), 1.0];
        let a = r.below(628) as f32 / 100.0;
        let dir = [a.sin(), 0.0, a.cos(), 0.0];
        let (nearest, flat, history) = (r.chance(70), r.chance(60), r.chance(60));
        ee.write(BLOCK, &block.bytes());
        ee.write(RECORD, &[0xcd; 20]);
        for (i, v) in point.iter().chain(dir.iter()).enumerate() {
            ee.wf32(POINT + 4 * i as u32, *v);
        }
        ee.call(SEARCH, &[BLOCK as u64, RECORD as u64, POINT as u64, DIR as u64, nearest as u64, flat as u64, history as u64], &[], STACK).unwrap_or_else(|e| panic!("case {case}: {e:?}"));
        let parts = host_parts(&objs, block.kind, block.skips_flagged());
        let want = search(&mut block, &parts, point, dir, nearest, flat, history, &k);
        let rec: Vec<u32> = (0..4).map(|i| ee.r32(RECORD + 4 * i)).collect();
        let (o, p) = want.map_or((0, 0), |(o, p)| (o, p as u32));
        assert_eq!(rec, vec![o, p, 0, u32::MAX], "case {case}: the record (parts {parts:?})");
        let game: Vec<u8> = (0..block_size(block.cap) as u32).map(|i| ee.r8(BLOCK + i)).collect();
        assert_eq!(Block::read(&game), block, "case {case}: the block");
        found += want.is_some() as u32;
        cases += 1;
    }
    eprintln!("{cases} scenes, {found} with a target");
    assert!(found > 50 && found < cases, "the scenes cover both outcomes");
}
