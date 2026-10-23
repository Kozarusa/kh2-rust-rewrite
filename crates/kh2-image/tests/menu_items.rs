//! Read-only tests against the user's executable and menu files (skipped without `KH2_ISO` or
//! `local/iso.txt`): the Items pages' tables (docs/systems/ui/menus-items.md section 13). Values are read at run time and
//! printed; the asserts check the structure the doc states. Nothing is copied into the repository.

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

fn text(m: &Messages, id: u32) -> String {
    m.text(id & 0x7fff).unwrap_or_else(|| "<none>".into())
}

/// The tables the Items pages read from the executable and the messages they show.
#[test]
fn item_page_tables_have_the_documented_shape() {
    let Some((mut a, exe)) = open() else { return };
    let m = Messages::from_bar(&a.read("msg/us/sys.bar").expect("sys.bar")).expect("messages");
    // The item types the Use row is offered for (0x381060, signed bytes, ended by a negative one): types 0 and 1.
    let mut usable = Vec::new();
    while (exe.u8(0x0038_1060 + usable.len() as u32) as i8) >= 0 {
        usable.push(exe.u8(0x0038_1060 + usable.len() as u32));
    }
    println!("usable item types (0x381060): {usable:?}");
    assert_eq!(usable, vec![0, 1]);
    // The items that can only go to one member (0x381068, signed words ended by a negative one): the two
    // drive recovery items.
    let mut single = Vec::new();
    while (exe.u16(0x0038_1068 + 2 * single.len() as u32) as i16) >= 0 {
        single.push(exe.u16(0x0038_1068 + 2 * single.len() as u32));
    }
    println!("single-target items (0x381068): {single:?}");
    assert_eq!(single, vec![0x112, 0x113]);
    // The two row labels of the Use / Throw away prompt (0x381090).
    let rows: Vec<u32> = (0..2).map(|i| u32::from(exe.u16(0x0038_1090 + 2 * i))).collect();
    println!("prompt rows (0x381090): {rows:x?} {:?}", rows.iter().map(|&r| text(&m, r)).collect::<Vec<_>>());
    // The category filters the stock page cycles through: item type -> bit pairs at 0x3823d0 (the type, the
    // bit of the filter word).
    let mut pairs = Vec::new();
    while (exe.u8(0x0038_23d0 + 2 * pairs.len() as u32) as i8) >= 0 && pairs.len() < 32 {
        let k = 2 * pairs.len() as u32;
        pairs.push((exe.u8(0x0038_23d0 + k), exe.u8(0x0038_23d0 + k + 1)));
    }
    println!("character -> filter bit pairs (0x3823d0): {pairs:?}");
    // The items the stock never lists while a story flag is set: (flag, item) pairs from 0x382450.
    let mut hidden = Vec::new();
    while exe.u16(0x0038_2450 + 4 * hidden.len() as u32) != 0 && hidden.len() < 32 {
        let k = 4 * hidden.len() as u32;
        hidden.push((exe.u16(0x0038_2450 + k), exe.u16(0x0038_2450 + k + 2)));
    }
    println!("stock exclusions (flag, item) (0x382450): {hidden:x?}");
    // The words at 0x381078: the category tab group ids and message ids.
    let w: Vec<u16> = (0..8).map(|i| exe.u16(0x0038_1078 + 2 * i)).collect();
    println!("0x381078 words: {w:x?}");
    for id in [0x455u32, 0x456, 0x457, 0x458, 0x3939, 0x393a, 0x393b, 0x393c, 0x3941, 0x3942, 0x3943, 0x3944, 0x3945, 0x394e, 0x394f, 0x3950, 0x370d, 0x3714, 0x3373, 0x390c, 0x390d, 0x390e, 0x390f, 0x45b, 0x463, 0x465, 0x467] {
        println!("message {id:#x}: {:?}", text(&m, id));
    }
    let class: Vec<(u16, String)> = w[4..8].iter().map(|&x| (x, text(&m, u32::from(x)))).collect();
    println!("filter tab messages: {class:?}");
}

/// The Customize page's tables (docs/systems/ui/menus-customize.md section 14): the row kinds, their groups and messages.
#[test]
fn customize_tables_have_the_documented_shape() {
    let Some((mut a, exe)) = open() else { return };
    let m = Messages::from_bar(&a.read("msg/us/sys.bar").expect("sys.bar")).expect("messages");
    let kinds = |va: u32| -> Vec<u8> { (0..8).map(|k| exe.u8(va + k)).collect() };
    // 0x381d40: Sora's row kinds (0 shortcuts, 1 spells, 2 items), 0x381d48: the others' (3, 4, 2).
    let sora: Vec<u8> = kinds(0x0038_1d40)[..3].to_vec();
    println!("row kinds: Sora {sora:?}, the others {:?}", &kinds(0x0038_1d48)[..3]);
    assert_eq!(sora, vec![0, 1, 2]);
    let row_group: Vec<u16> = (0..5).map(|k| exe.u16(0x0038_1ca0 + 2 * k)).collect();
    let head_group: Vec<u16> = (0..5).map(|k| exe.u16(0x0038_1cb0 + 2 * k)).collect();
    let head_msg: Vec<u16> = (0..5).map(|k| exe.u16(0x0038_1cc0 + 2 * k) & 0x7fff).collect();
    println!("row groups {row_group:x?}, heading plate groups {head_group:x?}, heading messages {head_msg:x?}");
    assert_eq!(row_group[..3], [0x108, 0x107, 0x106]);
    assert_eq!(head_group[..3], [0x11a, 0x119, 0x118]);
    for (i, id) in (0..4).map(|k| (k, exe.u16(0x0038_1cd0 + 2 * k) & 0x7fff)) {
        println!("shortcut row {i}: message {id:#x} {:?}", text(&m, u32::from(id)));
    }
    for k in 1..3usize {
        println!("heading kind {k}: {:?}", text(&m, u32::from(head_msg[k])));
    }
    for (name, va) in [("spell sub-list", 0x0038_1d20u32), ("item sub-list", 0x0038_1d30)] {
        let ids: Vec<u16> = (0..2).map(|k| exe.u16(va + 2 * k) & 0x7fff).collect();
        println!("{name}: {ids:x?} {:?}", ids.iter().map(|&i| text(&m, u32::from(i))).collect::<Vec<_>>());
    }
    // The icon of a row by item type and flag (`FUN_00255368`, 0x3809d0: type, unequipped icon, equipped icon).
    let mut icons = Vec::new();
    for k in 0..40u32 {
        let ty = exe.u16(0x0038_09d0 + 6 * k) as i16;
        if ty < 0 {
            break;
        }
        icons.push((ty, exe.u16(0x0038_09d2 + 6 * k), exe.u16(0x0038_09d4 + 6 * k)));
    }
    println!("icons (type, unequipped, equipped): {icons:?}");
    // The empty shortcut row and the remove row, the confirm question of the Auto-Reload switch.
    for id in [0x448u32, 0x459, 0x46a, 0x481, 0x482, 0x1df7, 0x3b89, 0x3b8a] {
        println!("message {id:#x}: {:?}", text(&m, id));
    }
}

/// The camp groups the Items pages draw: they exist; print their geometry for the layout.
#[test]
fn item_page_groups_exist_in_camp() {
    let Some((mut a, _)) = open() else { return };
    let bar = a.read("menu/us/camp.2ld").expect("camp.2ld");
    let two = layd::parse_2ld(&bar).expect("a .2ld");
    let seq = &two.layd.seqs[1];
    for g in [0xe3usize, 0xe4, 0xe5, 0x106, 0x107, 0x108, 0x118, 0x119, 0x11a, 0x11b, 0xc8, 0xbd, 0xb4, 0xa2, 0x99, 0x113, 0x13d, 0x14e,0x123usize, 0x124, 0x125, 0x126, 0x128, 0x129, 0x12b, 0x12c, 0x12e, 0x12f, 0x130, 0x131, 0x132, 0x84, 0x85, 0x86, 0x87, 0x9c, 0x77, 0x7b, 0x7c, 0xe6, 6, 7, 8, 9, 10, 11] {
        assert!(g < seq.groups.len(), "group {g:#x} exists");
        let gr = &seq.groups[g];
        let last = seq.last_frame(g);
        let (x, y) = seq.anchor(g, 0, &Default::default()).map_or((0, 0), |p| (p[0], p[1]));
        let q = seq.quads(g, last, [0.0, 0.0], &Default::default());
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in q.iter().flat_map(|q| q.pos) {
            x0 = x0.min(p[0]);
            x1 = x1.max(p[0]);
            y0 = y0.min(p[1]);
            y1 = y1.max(p[1]);
        }
        let alpha: Vec<u8> = q.iter().take(6).map(|q| q.color[0][3]).collect();
        println!(
            "group {g:#x}: {} quads, first alphas {alpha:?}, blend {:?}; loops {} flags {:#x} len {} last {last} xy ({}, {}) size {} unk1c {} unk20 {} anchor0 ({x}, {y}) last-frame bbox ({x0:.0}, {y0:.0})-({x1:.0}, {y1:.0})",
            q.len(),
            q.iter().take(6).map(|q| q.blend).collect::<Vec<_>>(),
            gr.loops,
            gr.flags,
            seq.group_len(g),
            gr.x,
            gr.y,
            gr.size,
            gr.unk1c,
            gr.unk20
        );
    }
}
