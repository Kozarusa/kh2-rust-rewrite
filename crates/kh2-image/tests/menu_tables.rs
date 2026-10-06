//! Read-only tests against the user's executable on the disc (skipped without `KH2_ISO` or
//! `local/iso.txt`): the menu tables docs/menus.md quotes. Values are read at run time and printed; the
//! asserts check the structure the doc states (menu kinds, the top-level entries and their pages, the
//! Config rows and the option-word bits their readers use, the menu sound table). Nothing is copied
//! into the repository.

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

fn messages(a: &mut kh2_archive::Archive) -> Messages {
    Messages::from_bar(&a.read("msg/us/sys.bar").expect("sys.bar")).expect("messages")
}

fn text(m: &Messages, id: u16) -> String {
    m.text(u32::from(id & 0x7fff)).unwrap_or_else(|| format!("<no message {:#x}>", id & 0x7fff))
}

#[test]
fn menu_kind_table_has_sixteen_kinds() {
    let Some((_a, exe)) = open() else { return };
    // 0x380560: 16-byte records {u16 kind, u16 scene, u32 init, u32 term, u32 mode pointer}, ended by kind -1
    // (FUN_0024f250 finds a record by kind; FUN_0024f0b8 writes the record's scene as the next scene).
    let mut kinds = Vec::new();
    let mut at = 0x0038_0560;
    while exe.u16(at) != 0xffff {
        let (kind, scene, init, term, mode) = (exe.u16(at), exe.u16(at + 2), exe.u32(at + 4), exe.u32(at + 8), exe.u32(at + 12));
        println!("menu kind {kind:#x}: scene {scene:#x} init {init:#x} term {term:#x} mode word {mode:#x}");
        kinds.push((kind, scene, init));
        at += 16;
    }
    assert_eq!(kinds.len(), 16);
    // kind 0 = the camp (pause) menu, scene 0, init FUN_0024faf0; kind 5 = the same menu opened at a given page.
    assert_eq!(kinds[0], (0, 0, 0x0024_faf0));
    assert_eq!(kinds[5], (5, 1, 0x0025_0450));
    // kind 3 = the journal (scene 2, init FUN_002756f8); kind 7 = the save menu (scene 0x32, init FUN_002540c8).
    assert_eq!(kinds[3], (3, 2, 0x0027_56f8));
    assert_eq!(kinds[7], (7, 0x32, 0x0025_40c8));
    // The layout file names and the file-name format.
    let name = |va: u32| (0..).map(|i| exe.u8(va + i)).take_while(|&c| c != 0).map(char::from).collect::<String>();
    assert_eq!(name(0x0038_0670), "pause.2ld");
    assert_eq!(name(0x0038_0680), "save.2ld");
    assert_eq!(name(0x0038_0690), "camp.2ld");
    assert_eq!(name(0x0038_06a0), "jmemo.2ld");
    assert_eq!(name(0x0038_06b0), "itemshop.2ld");
    assert_eq!(name(0x0038_06e8), "menu/%s/%s");
}

#[test]
fn top_level_entries_names_help_flags_and_pages() {
    let Some((mut a, exe)) = open() else { return };
    let m = messages(&mut a);
    // 0x3808a8: eight 0x24-byte entries. +0 and +8: four u16 animation group ids each (idle row, selected
    // row sets); +0x12 name message, +0x14 help message (bit 15 ignored); +0x1c: up to four story-flag bits
    // that must all be set (list ends at -1), +0x20: up to four that must all be clear. The flag bits are
    // bit n of the word array at 0x33fdb0 (FUN_001a5328).
    let want = [
        ("Items", "Equip weapons and armor,\nor use items in stock.", vec![0x70], vec![]),
        ("Abilities", "Equip Abilities.", vec![], vec![]),
        ("Customize", "Change Character settings.", vec![0x72], vec![]),
        ("Party", "Switch Party Members.", vec![0x71], vec![]),
        ("Status", "Show Status.", vec![], vec![]),
        ("Journal", "Read Jiminy's Journal.", vec![0x73], vec![]),
        ("Journal", "Read Roxas's Journal.", vec![], vec![0x73]),
        ("Config", "Change Game settings.", vec![], vec![]),
    ];
    for (i, (name, help, need, deny)) in want.iter().enumerate() {
        let at = 0x0038_08a8 + 0x24 * i as u32;
        let flags = |o: u32| (0..4).map(|k| exe.u8(at + o + k) as i8).take_while(|&b| b >= 0).map(|b| b as u32).collect::<Vec<_>>();
        let groups: Vec<u16> = (0..8).map(|k| exe.u16(at + 2 * k)).collect();
        println!(
            "entry {i}: {:?} / {:?}; groups {:x?}; needs flags {:x?}; none of {:x?}",
            text(&m, exe.u16(at + 0x12)),
            text(&m, exe.u16(at + 0x14)),
            groups,
            flags(0x1c),
            flags(0x20)
        );
        assert_eq!(&text(&m, exe.u16(at + 0x12)), name, "entry {i} name");
        assert_eq!(&text(&m, exe.u16(at + 0x14)), help, "entry {i} help");
        assert_eq!(&flags(0x1c), need, "entry {i} needs");
        assert_eq!(&flags(0x20), deny, "entry {i} denies");
        assert_eq!(groups, vec![0x85, 0x87, 0x86, 0xffff, 0x85, 0x87, 0x84, 0xffff], "entry {i} plate groups");
    }
    // The jump table at 0x380794 picks the page by entry kind; each target is `addiu s0,zero,<page>`, directly
    // or in the delay slot of a branch (FUN_00251848). Items 1, Abilities 15, Customize 25, Party 20, Status 23,
    // Config 36; the two Journal entries leave the menu for the journal scene instead.
    let page = |target: u32| {
        let w0 = exe.u32(target);
        let w = if w0 >> 16 == 0x1000 { exe.u32(target + 4) } else { w0 };
        (w >> 26 == 9 && (w >> 16) & 31 == 16).then_some(w & 0xffff)
    };
    let targets: Vec<u32> = (0..8).map(|i| exe.u32(0x0038_0794 + 4 * i)).collect();
    let pages: Vec<Option<u32>> = targets.iter().map(|&t| page(t)).collect();
    println!("jump table {:x?} -> pages {pages:?}", targets);
    assert_eq!(pages[0], Some(1));
    assert_eq!(pages[1], Some(15));
    assert_eq!(pages[2], Some(25));
    assert_eq!(pages[3], Some(20));
    assert_eq!(pages[4], Some(23));
    assert_eq!(page(targets[7]), Some(36));
}

/// Row readers of the Config page (FUN_0027e1f8 calls these for rows 0..8; each reads the option word at
/// save + 0x31b0): returns (shift, mask, inverted) found in the routine's instructions.
fn option_bits(exe: &Exe, start: u32) -> (u32, u32, bool) {
    let (mut shift, mut mask, mut inv, mut seen_load) = (0, u32::MAX, false, false);
    for k in 0..12 {
        let w = exe.u32(start + 4 * k);
        match (w >> 26, w & 63) {
            (0x23, _) if w & 0xffff == 0x31b0 => seen_load = true,
            (0, 2) | (0, 3) if w >> 21 & 31 == 0 => shift = w >> 6 & 31,
            (12, _) => mask = w & 0xffff,
            (14, _) => inv = w & 0xffff == 1,
            _ => {}
        }
        if w == 0x03e0_0008 {
            break;
        }
    }
    assert!(seen_load, "reader {start:#x} loads +0x31b0");
    (shift, mask, inv)
}

#[test]
fn config_rows_labels_values_and_option_word_bits() {
    let Some((mut a, exe)) = open() else { return };
    let m = messages(&mut a);
    // 0x382158: ten 16-byte rows of eight u16: value count, label, up to three value labels, then the help
    // line of each value (two-valued rows have a zero in the third slot). Bit 15 of a message id is ignored.
    let expect = [
        ("Field Camera", 2, "Auto", "Manual"),
        ("Right Analog Stick", 2, "Camera", "Command Menu"),
        ("Camera Up/Down", 2, "Original", "Reverse"),
        ("Camera Left/Right", 2, "Original", "Reverse"),
        ("Summon Effects", 3, "Auto", "Full"),
        ("Navigational Map", 2, "ON", "OFF"),
        ("Vibration", 2, "ON", "OFF"),
        ("Sound", 3, "Stereo", "Pro Logic II"),
        ("Command Menu", 2, "Kingdom Hearts <g85>", "Kingdom Hearts"),
        ("Game Mode", 1, "Beginner Mode", "Standard Mode"),
    ];
    for (i, (label, count, v0, v1)) in expect.iter().enumerate() {
        let at = 0x0038_2158 + 0x10 * i as u32;
        let (n, l, a0, a1) = (exe.u16(at), text(&m, exe.u16(at + 2)), text(&m, exe.u16(at + 4)), text(&m, exe.u16(at + 6)));
        println!("row {i}: {l:?} x{n}: {a0:?} / {a1:?} / {:?}", text(&m, exe.u16(at + 8)));
        assert_eq!(n, *count, "row {i} value count");
        assert_eq!(&l, label, "row {i} label");
        assert_eq!(&a0, v0, "row {i} first value");
        if i != 8 && i != 9 {
            assert_eq!(&a1, v1, "row {i} second value");
        }
    }
    // The readers: (row, reader address, shift, mask, inverted-by-xori-1).
    let readers: [(usize, u32); 9] = [
        (0, 0x23ec68),
        (1, 0x23ece0),
        (2, 0x23ee68),
        (3, 0x23edf8),
        (4, 0x23eed8),
        (5, 0x23ebf8),
        (6, 0x23eb10),
        (7, 0x23eb78),
        (8, 0x23ed80),
    ];
    let mut bits = Vec::new();
    for (row, f) in readers {
        let (shift, mask, inv) = option_bits(&exe, f);
        println!("row {row}: option word >> {shift} & {mask:#x}{}", if inv { " (xor 1)" } else { "" });
        bits.push((shift, mask, inv));
    }
    assert_eq!(
        bits,
        vec![(4, 1, true), (5, 1, true), (8, 1, false), (7, 1, false), (9, 3, false), (3, 1, false), (0, 1, false), (1, 3, false), (6, 1, true)]
    );
}

#[test]
fn menu_sound_kinds_map_to_system_sound_ids() {
    let Some((_a, exe)) = open() else { return };
    // FUN_0023e738(kind, play) -> FUN_0023e828(kind): kinds below 6 go through the s16 table at 0x36cb48, then
    // the u16 table at 0x37f654 gives the sound id that FUN_002c14a8 starts (FUN_002be49c(2, id, 0, 0x3fff, 0x40)).
    let redirect: Vec<i16> = (0..6).map(|i| exe.u16(0x0036_cb48 + 2 * i) as i16).collect();
    let id = |k: usize| exe.u16(0x0037_f654 + 2 * redirect.get(k).map_or(k as u32, |&r| r as u32));
    println!("kind -> sound id: {:?}", (0..12).map(|k| (k, id(k))).collect::<Vec<_>>());
    assert_eq!(redirect, vec![6, 7, 8, 9, 10, 8]);
    // kind 1 cursor, 2 confirm, 3 cancel, 4 refused, 5 close (FUN_00295bf0 picks them from the list result).
    assert_eq!([id(1), id(2), id(3), id(4), id(5)], [1, 2, 4, 5, 2]);
    assert_eq!(id(0xb), 3);
}
