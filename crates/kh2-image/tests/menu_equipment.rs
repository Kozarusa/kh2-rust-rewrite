//! Read-only tests against the user's executable and menu files (skipped without `KH2_ISO` or
//! `local/iso.txt`): the Equipment pages' tables (docs/systems/ui/menus-equipment.md section 15). Values are read at run time and
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
    fn words(&self, va: u32, n: u32) -> Vec<u16> {
        (0..n).map(|k| self.u16(va + 2 * k)).collect()
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

/// The tables of the pages 2 and 5 to 9 (`FUN_00269580`, `FUN_0026a528`, `FUN_00269028`, `FUN_00283be0`).
#[test]
fn equipment_page_tables_have_the_documented_shape() {
    let Some((mut a, exe)) = open() else { return };
    let m = Messages::from_bar(&a.read("msg/us/sys.bar").expect("sys.bar")).expect("messages");
    // 0x381970: the page of each category (the Weapons, forms' weapons, Armor, Accessories and Items lists).
    let pages = exe.words(0x0038_1970, 6);
    println!("category pages (0x381970): {pages:x?}");
    assert_eq!(pages[..5], [5, 6, 7, 8, 9]);
    // 0x3819a8: the filter argument `FUN_00283a70` gets per category (1 = the character's keyblade type, 2 = armor,
    // 3 = accessories, 4 = consumables).
    let filters = exe.words(0x0038_19a8, 5);
    println!("category filters (0x3819a8): {filters:x?}");
    assert_eq!(filters, [1, 1, 2, 3, 4]);
    // 0x3819cc: the writer each category's confirm jumps to (`FUN_0026c280`).
    let jump: Vec<u32> = (0..5).map(|k| u32::from(exe.u16(0x0038_19cc + 4 * k)) | u32::from(exe.u16(0x0038_19ce + 4 * k)) << 16).collect();
    println!("writer jump table (0x3819cc): {jump:x?}");
    assert_eq!(jump, [0x26c338, 0x26c388, 0x26c3a4, 0x26c3c4, 0x26c3ec]);
    // Row plates, tag plates and labels of page 2: per category, and per form bit for the forms' rows.
    println!("page 2 row plate groups per category (0x3819e8): {:x?}", exe.words(0x0038_19e8, 5));
    println!("page 2 row plate groups per form bit (0x3819f8): {:x?}", exe.words(0x0038_19f8, 6));
    println!("page 2 tag groups per category (0x381a08): {:x?}", exe.words(0x0038_1a08, 5));
    println!("page 2 tag groups per form bit (0x381a28): {:x?}", exe.words(0x0038_1a28, 6));
    let labels = exe.words(0x0038_1a18, 5);
    println!("category labels (0x381a18): {:?}", labels.iter().map(|&l| format!("{:#x} {}", l & 0x7fff, text(&m, u32::from(l)))).collect::<Vec<_>>());
    let forms = exe.words(0x0038_1a38, 5);
    println!("form labels per form (0x381a38): {:?}", forms.iter().map(|&l| format!("{:#x} {}", l & 0x7fff, text(&m, u32::from(l)))).collect::<Vec<_>>());
    // The pick pages: the list window (enter, leave, hold) per category, the item window, the row plates.
    for c in 0..5u32 {
        println!(
            "category {c}: window {:x?}, plates (idle, selected) ({:#x}, {:#x}), strip group {:#x}, picture window {:x?}, empty row type {:#x}",
            exe.words(0x0038_1a48 + 6 * c, 3),
            exe.u16(0x0038_1a98 + 2 * c),
            exe.u16(0x0038_1a98 + 2 * (c + 5)),
            exe.u16(0x0038_1a68 + 2 * c),
            exe.words(0x0038_1a78 + 6 * c, 3),
            exe.u16(0x0038_2460 + 2 * c)
        );
    }
    println!("group sets for the page 2 rows (0x380848): {:x?}", exe.words(0x0038_0848, 8));
    println!("auto-reload glyph groups (0x3807f8): {:x?}", exe.words(0x0038_07f8, 8));
    // The weapon type per character (0x382410: character, item type).
    println!("member -> weapon type (0x382410): {:x?}", exe.words(0x0038_2410, 12));
    for id in [0x45bu32, 0x45c, 0x45d, 0x45e, 0x45f, 0x460, 0x461, 0x462, 0x463, 0x464, 0x465, 0x466, 0x467, 0x468, 0x3353, 0x846a, 0xc22e, 0x1df7, 0x448, 0x45a, 0x1fd0, 0x1fd1] {
        println!("message {id:#x}: {:?}", text(&m, id));
    }
}

/// The camp groups the Equipment pages draw: they exist; print their geometry for the layout.
#[test]
fn equipment_page_groups_exist_in_camp() {
    let Some((mut a, exe)) = open() else { return };
    let bar = a.read("menu/us/camp.2ld").expect("camp.2ld");
    let two = layd::parse_2ld(&bar).expect("a .2ld");
    let seq = &two.layd.seqs[1];
    let mut ids: Vec<usize> = vec![0xe3, 0xe4, 0xe5, 0x109, 0x10a, 0x10b, 0x10c, 0xfc, 0xf8, 0x106, 0x11b, 0x11c, 0x11d, 0x11e, 0x116, 0x114, 0x118, 0x7a, 0x7b, 0x11f, 0x120, 0xf9, 0xfa, 6, 7, 8];
    for c in 0..5u32 {
        ids.extend(exe.words(0x0038_1a48 + 6 * c, 3).iter().map(|&x| x as usize));
        ids.extend(exe.words(0x0038_1a78 + 6 * c, 3).iter().map(|&x| x as usize));
        ids.push(exe.u16(0x0038_1a98 + 2 * c) as usize);
        ids.push(exe.u16(0x0038_1a98 + 2 * (c + 5)) as usize);
        ids.push(exe.u16(0x0038_1a68 + 2 * c) as usize);
    }
    ids.sort_unstable();
    ids.dedup();
    for g in ids {
        if g == 0xffff || g >= seq.groups.len() {
            continue;
        }
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
        println!(
            "group {g:#x}: {} quads, len {} last {last} xy ({}, {}) size {} unk1c {} unk20 {} anchor0 ({x}, {y}) last-frame bbox ({x0:.0}, {y0:.0})-({x1:.0}, {y1:.0})",
            q.len(),
            seq.group_len(g),
            gr.x,
            gr.y,
            gr.size,
            gr.unk1c,
            gr.unk20
        );
    }
}
