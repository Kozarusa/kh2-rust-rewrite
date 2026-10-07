//! Read-only tests against the user's executable and menu files (skipped without `KH2_ISO` or `local/iso.txt`): the
//! tables and groups of the M7 follow-ups (docs/menus.md section 16): the NEW tags, the switch hint, the Help overlay's
//! pages and the item pictures. Values are read at run time and printed; nothing is copied into the repository.

use kh2_image::layd;
use kh2_image::text::Messages;

struct Exe {
    data: Vec<u8>,
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
    fn words(&self, va: u32, n: u32) -> Vec<u16> {
        (0..n).map(|k| self.u16(va + 2 * k)).collect()
    }
    fn cstr(&self, va: u32) -> String {
        (0..40u32).map(|k| self.u8(va + k)).take_while(|&b| b != 0).map(|b| b as char).collect()
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

/// Writes an image as a PNG (for looking at; the file stays out of the repository).
fn dump(path: &str, img: &kh2_image::imgd::Image) {
    let Ok(f) = std::fs::File::create(path) else { return };
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    if let Ok(mut w) = enc.write_header() {
        w.write_image_data(&img.rgba).ok();
    }
}

fn text(m: &Messages, id: u32) -> String {
    m.text(id & 0x7fff).unwrap_or_else(|| "<none>".into())
}

/// The NEW tag groups and the type list (`FUN_00293b60`, `FUN_00251100`, `FUN_0025dae8`, `FUN_0026bd08`, `FUN_0027b9a8`).
#[test]
fn new_tag_tables_have_the_documented_shape() {
    let Some((_, exe)) = open() else { return };
    // Page 0: DAT_003807e8 (something unseen) and DAT_003807ec (everything seen), both the first word of their group set.
    assert_eq!(exe.u16(0x0038_07e8), 0x32);
    assert_eq!(exe.u16(0x0038_07ec), 0x31);
    // The Stock cells (0x381070, 0x381074), the equipment lists (0x3819e0, 0x3819e4), the Abilities rows (0x382044, 0x382048).
    for (unseen, seen) in [(0x0038_1070u32, 0x0038_1074u32), (0x0038_19e0, 0x0038_19e4), (0x0038_2044, 0x0038_2048)] {
        assert_eq!((exe.u16(unseen), exe.u16(seen)), (0x32, 0x31), "{unseen:#x}");
    }
    let types: Vec<u8> = (0..32u32).map(|k| exe.u8(0x0038_2808 + k)).take_while(|&b| b < 0x80).collect();
    println!("item types of the NEW set (0x382808): {types:?}");
    assert_eq!(types, vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 6, 14, 15, 16, 17, 20]);
    println!("summon-related list (0x382820): {:x?}, (0x382830): {:x?}", exe.words(0x0038_2820, 6), exe.words(0x0038_2830, 6));
}

/// The camp groups of the follow-ups: print their geometry.
#[test]
fn m7_groups_exist_in_camp() {
    let Some((mut a, _)) = open() else { return };
    let bar = a.read("menu/us/camp.2ld").expect("camp.2ld");
    let two = layd::parse_2ld(&bar).expect("a .2ld");
    let seq = &two.layd.seqs[1];
    for g in [0x31usize, 0x32, 0xef, 0xf0, 0xf, 0x10, 0x12, 0x13, 0x11f, 0x120, 0x131, 0x132, 0x15c, 0x15d, 0x15e, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x7b, 0x7c, 1, 4, 0x8b, 0x7d] {
        if g >= seq.groups.len() {
            println!("group {g:#x}: not in the sequence");
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
    // The page 2 and pick page windows (tables 0x381a68 and 0x381a78) are printed by `menu_equipment`.
}

/// The Help overlay: its table, titles and the page images.
#[test]
fn help_tables_and_pages_have_the_documented_shape() {
    let Some((mut a, exe)) = open() else { return };
    let m = Messages::from_bar(&a.read("msg/us/sys.bar").expect("sys.bar")).expect("messages");
    println!("file format {:?}, variants {:?}", exe.cstr(0x0038_26d8), (0..3u32).map(|k| exe.u8(0x0038_26d0 + k) as char).collect::<Vec<_>>());
    let mut stems = Vec::new();
    for k in 0..9u32 {
        let at = 0x0038_2730 + 8 * k;
        let stem = exe.cstr(exe.u32(at));
        println!("entry {k}: stem {stem}, flag {:#x}, topic {}, flags {:#x}", exe.u16(at + 4), exe.u8(at + 6), exe.u8(at + 7));
        stems.push(stem);
    }
    assert_eq!(exe.u16(0x0038_2730 + 8 * 9 + 4), 0, "the table ends after nine entries");
    for t in 0..4u32 {
        let id = exe.u16(0x0038_2780 + 2 * t);
        println!("topic {t} title {:#x}: {:?}", id & 0x7fff, text(&m, u32::from(id)));
    }
    println!("counter message 0x4bd8: {:?}, format {:?}", text(&m, 0x4bd8), exe.cstr(0x0038_26b8));
    for stem in &stems {
        let bar = a.read(&format!("menu/us/{stem}_s.bar")).expect("the story variant exists");
        let entries = kh2_bar::parse(&bar).expect("a bar");
        for e in entries {
            let img = kh2_image::imgd::decode(e.data);
            println!("{stem}: entry {:?} {} bytes -> {:?}", e.tag, e.data.len(), img.as_ref().map(|i| (i.width, i.height)));
            if let (Some(i), Ok(dir)) = (img, std::env::var("KH2_DUMP_DIR")) {
                dump(&format!("{dir}/{stem}.png"), &i);
            }
        }
    }
}

/// The item pictures: a few of them decode, 128 by 128.
#[test]
fn item_pictures_decode() {
    let Some((mut a, exe)) = open() else { return };
    println!("picture file format {:?}", exe.cstr(0x0038_29c0));
    let mut n = 0;
    for id in 0..400u32 {
        let Ok(b) = a.read(&format!("itempic/item-{id:03}.imd")) else { continue };
        let img = kh2_image::imgd::decode(&b).expect("an IMGD");
        if n < 5 {
            println!("item-{id:03}.imd: {}x{}", img.width, img.height);
        }
        if let Ok(dir) = std::env::var("KH2_DUMP_DIR")
            && id < 6
        {
            dump(&format!("{dir}/item-{id:03}.png"), &img);
        }
        n += 1;
    }
    println!("{n} pictures found");
    assert!(n > 100);
}
