//! Read-only tests against the user's executable and menu files (skipped without `KH2_ISO` or
//! `local/iso.txt`): the Status screen's tables (docs/menus.md 2.6 and section 10). Values are read at
//! run time and printed; the asserts check the structure the doc states. Nothing is copied into the
//! repository.

use kh2_image::layd;
use kh2_image::text::Messages;

struct Exe {
    data: Vec<u8>,
    /// (virtual address, file offset, file size, memory size) of each PT_LOAD segment.
    segs: Vec<(u32, usize, usize, usize)>,
}

impl Exe {
    fn load(data: Vec<u8>) -> Exe {
        let rd = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
        let rd16 = |o: usize| u16::from_le_bytes(data[o..o + 2].try_into().unwrap()) as usize;
        let (phoff, phentsize, phnum) = (rd(0x1c) as usize, rd16(0x2a), rd16(0x2c));
        let mut segs = Vec::new();
        for i in 0..phnum {
            let o = phoff + i * phentsize;
            if rd(o) == 1 {
                segs.push((rd(o + 8), rd(o + 4) as usize, rd(o + 16) as usize, rd(o + 20) as usize));
            }
        }
        Exe { data, segs }
    }
    fn u8(&self, va: u32) -> u8 {
        for &(v, off, fsz, msz) in &self.segs {
            if va >= v && ((va - v) as usize) < msz {
                let k = (va - v) as usize;
                return if k < fsz { self.data[off + k] } else { 0 };
            }
        }
        panic!("address {va:#x} is not in the executable");
    }
    fn u16(&self, va: u32) -> u16 {
        u16::from_le_bytes([self.u8(va), self.u8(va + 1)])
    }
    fn u32(&self, va: u32) -> u32 {
        u32::from(self.u16(va)) | u32::from(self.u16(va + 2)) << 16
    }
    fn cstr(&self, va: u32) -> String {
        (0..16).map(|i| self.u8(va + i)).take_while(|&b| b != 0).map(|b| b as char).collect()
    }
}

fn open() -> Option<(kh2_archive::Archive, Exe)> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return None;
    }
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    Some((a, Exe::load(elf)))
}

fn text(m: &Messages, id: u16) -> String {
    m.text(u32::from(id & 0x7fff)).unwrap_or_else(|| "<none>".into())
}

/// The tables of the Status screen (addresses in docs/menus.md 2.6): the three number formats the page
/// prints with, the tab name messages, the row tables of the three kinds of tab and the group each row
/// type draws.
#[test]
fn status_tables_have_the_documented_shape() {
    let Some((mut a, exe)) = open() else { return };
    let m = Messages::from_bar(&a.read("msg/us/sys.bar").expect("sys.bar")).expect("messages");
    assert_eq!(exe.cstr(0x0038_16b8), "%d");
    assert_eq!(exe.cstr(0x0038_16c0), "%d/%d");
    assert_eq!(exe.cstr(0x0038_16c8), "%d%%");
    // Tab names (0x3817d0): the Sora tab, four forms, the summons, then the party members' own names.
    let names: Vec<String> = (0..9).map(|i| text(&m, exe.u16(0x0038_17d0 + 2 * i))).collect();
    println!("status tab name messages: {names:?}");
    // Row tables: 0x3817e8 (Sora and party members), 0x381860 (forms), 0x3818d8 (summons); two columns
    // (a toggle byte) of ten rows of 6 bytes each (name message, help message, row type; type 0xffff ends).
    for (name, base) in [("sora/party", 0x0038_17e8u32), ("form", 0x0038_1860), ("summon", 0x0038_18d8)] {
        for col in 0..2u32 {
            let b = base + col * 0x3c;
            println!("status rows {name} column {col}:");
            for r in 0..10u32 {
                let (l, h, t) = (exe.u16(b + 6 * r), exe.u16(b + 6 * r + 2), exe.u16(b + 6 * r + 4));
                if t == 0xffff {
                    break;
                }
                if l == 0 {
                    continue;
                }
                println!("  row {r}: type {t} name {l:#x} {:?} help {h:#x} {:?}", text(&m, l), text(&m, h));
            }
        }
    }
    // The first Sora row is the level, then experience, then what is left to the next level.
    let first: Vec<u16> = (0..3).map(|r| exe.u16(0x0038_17e8 + 6 * r) & 0x7fff).collect();
    assert_eq!(first, vec![0x4559, 0x455a, 0x455b]);
    // Row type -> (value plate group, row group): 0x3817b8, four bytes per type.
    let types: Vec<(u16, u16)> = (0..5).map(|t| (exe.u16(0x0038_17b8 + 4 * t), exe.u16(0x0038_17ba + 4 * t))).collect();
    println!("status row types (value group, row group): {types:?}");
    assert_eq!(types[0], (0x89, 0x143));
    // The group sets of the tab rows (`FUN_00253048`: 0x380860 + 8 * n, `FUN_00253060`: 0x380870 + 8 * n,
    // four u16 each) and the value plate group (`FUN_00253088(2)`: u16 at 0x3807f8 + 2 * n).
    for base in [0x0038_0860u32, 0x0038_0870] {
        for n in 0..2u32 {
            let set: Vec<u16> = (0..4).map(|k| exe.u16(base + 8 * n + 2 * k)).collect();
            println!("tab row group set {base:#x}+{n}: {set:x?}");
        }
    }
    let plates: Vec<u16> = (0..6).map(|k| exe.u16(0x0038_07f8 + 2 * k)).collect();
    println!("0x3807f8 plate groups: {plates:x?}");
    // The two words before the table: the NEW tag groups of the Munny plate and of Status.
    println!("0x3817b0/0x3817b4: {:#x} {:#x}", exe.u32(0x0038_17b0), exe.u32(0x0038_17b4));
}

/// The camp sequence's groups the Status rows draw (from the type table above): they exist, and the row
/// groups are the ones that carry a pitch (`unk20`) the page lays its rows out with.
#[test]
fn status_groups_exist_in_camp() {
    let Some((mut a, _)) = open() else { return };
    let bar = a.read("menu/us/camp.2ld").expect("camp.2ld");
    let two = layd::parse_2ld(&bar).expect("a .2ld");
    let seq = &two.layd.seqs[1];
    for g in [0x89usize, 0x8b, 0x8f, 0x90, 0x96, 0x143, 0x144, 0x145, 0x146, 0x147] {
        let gr = &seq.groups[g];
        let (x, y) = seq.anchor(g, 0, &Default::default()).map_or((0, 0), |p| (p[0], p[1]));
        println!(
            "group {g:#x}: loops {} flags {:#x} len {} frames {} xy ({}, {}) size {} {} pitch {} anchor ({x}, {y})",
            gr.loops,
            gr.flags,
            seq.group_len(g),
            seq.last_frame(g),
            gr.x,
            gr.y,
            gr.size,
            gr.unk1c,
            gr.unk20
        );
        for f in 0..=seq.last_frame(g) {
            let q = seq.quads(g, f, [0.0, 0.0], &Default::default());
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in q.iter().flat_map(|q| q.pos) {
                x0 = x0.min(p[0]);
                x1 = x1.max(p[0]);
                y0 = y0.min(p[1]);
                y1 = y1.max(p[1]);
            }
            let a = seq.anchor(g, f, &Default::default());
            println!("   frame {f}: {} quads, bbox ({x0:.0}, {y0:.0})-({x1:.0}, {y1:.0}) anchor {a:?}", q.len());
        }
    }
}
