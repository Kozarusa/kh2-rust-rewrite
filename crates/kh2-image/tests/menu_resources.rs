//! Read-only tests against the user's disc (skipped without `KH2_ISO` or `local/iso.txt`): the layout
//! archives the pause menu, the save menu, the journal and the shop load (docs/menus.md section 1).
//!
//! A `menu/us/*.2ld` is a BAR of two entries: type 29 = IMGZ (the images) and type 28 = LAYD (the layout:
//! a list of layers, a list of layouts that group layers, and the SEQD sequence files, embedded). The
//! tests parse the LAYD with the small reader below (its layout is written in docs/menus.md section 1.2),
//! decode every embedded SEQD and image with `kh2_image`, and print what each file holds. Nothing is
//! written unless `KH2_MENU_DUMP=<dir>` is set (then the full system message table is written there as
//! text, outside the repository).

use kh2_image::layd;
use kh2_image::text::Messages;

fn open() -> Option<kh2_archive::Archive> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return None;
    }
    kh2_archive::Archive::open(&path).ok()
}

/// The layout archives the menu code names (strings at 0x380670..0x3806d0; the loader FUN_0024f558 /
/// FUN_0024f908 takes them by index 0..5: pause, save, camp, jmemo, itemshop, gm_ps). `gm_ps.2ld` is
/// not on the US disc under `menu/us/`; title.2ld is loaded by the title screen, not by these menus.
const MENU_FILES: [&str; 5] = ["pause.2ld", "save.2ld", "camp.2ld", "jmemo.2ld", "itemshop.2ld"];

#[test]
fn menu_layout_archives_parse_and_are_listed() {
    let Some(mut a) = open() else { return };
    for name in MENU_FILES {
        let bar = a.read(&format!("menu/us/{name}")).unwrap_or_else(|e| panic!("menu/us/{name}: {e:?}"));
        let entries = kh2_bar::parse(&bar).expect("a BAR");
        println!("menu/us/{name}: {} bytes, {} BAR entries", bar.len(), entries.len());
        for e in &entries {
            println!("  tag {:?} type {} {} bytes, magic {:?}", e.tag, e.kind, e.data.len(), String::from_utf8_lossy(&e.data[..4.min(e.data.len())]));
        }
        let imgz = entries.iter().find(|e| e.data.starts_with(b"IMGZ")).unwrap_or_else(|| panic!("{name}: IMGZ"));
        let layd = entries.iter().find(|e| e.data.starts_with(b"LAYD")).unwrap_or_else(|| panic!("{name}: LAYD"));
        assert_eq!((imgz.kind, layd.kind), (29, 28), "{name}: IMGZ is BAR type 29, LAYD type 28");
        let two = layd::parse_2ld(&bar).unwrap_or_else(|| panic!("{name}: a .2ld"));
        assert!(!two.images.is_empty(), "{name}: images");
        for (i, d) in two.images.iter().enumerate() {
            println!("  image {i}: {}x{}", d.width, d.height);
        }
        let l = two.layd;
        println!("  LAYD: {} layers, {} layouts, {} sequences", l.layers.len(), l.layouts.len(), l.seqs.len());
        for (i, q) in l.seqs.iter().enumerate() {
            println!(
                "  SEQD {i}: sprites {} parts {} sprite groups {} anims {} anim groups {}",
                q.sprites.len(),
                q.parts.len(),
                q.sprite_groups.len(),
                q.anims.len(),
                q.groups.len()
            );
        }
        for (i, ly) in l.layers.iter().enumerate() {
            println!("  layer {i}: kind {} seq {} group {} at ({}, {})", ly.kind, ly.seq, ly.group, ly.x, ly.y);
        }
        println!("  layouts {:?}", l.layouts);
        // Every layout covers a valid run of layers, every layer names an existing sequence and group.
        let mut covered = 0;
        for &(first, count) in &l.layouts {
            assert!(first + count <= l.layers.len(), "{name}: layout run");
            covered += count;
        }
        assert_eq!(covered, l.layers.len(), "{name}: the layouts cover every layer once");
        for ly in &l.layers {
            assert!(ly.group < l.seqs[ly.seq].groups.len(), "{name}: group {} of sequence {}", ly.group, ly.seq);
        }
    }
}

#[test]
fn camp_layout_matches_the_numbers_the_doc_quotes() {
    let Some(mut a) = open() else { return };
    // docs/menus.md section 1.2: camp.2ld has one 256x256 image, two sequences (a 7-group one for the
    // top and bottom bars and a 351-group one for everything else) and ten layers in three layouts.
    let bar = a.read("menu/us/camp.2ld").expect("camp.2ld");
    let two = layd::parse_2ld(&bar).unwrap();
    let layd = two.layd;
    assert_eq!(two.images.len(), 1);
    let d = &two.images[0];
    assert_eq!((d.width, d.height), (256, 256));
    assert_eq!(layd.seqs.len(), 2);
    assert_eq!(layd.seqs[0].groups.len(), 7);
    assert_eq!(layd.seqs[1].groups.len(), 351);
    assert_eq!(layd.layers.len(), 10);
    assert_eq!(layd.layouts, vec![(0, 3), (3, 4), (7, 3)]);
    // The plate groups the top list uses (executable table 0x3808a8): 0x84 selected, 0x85 and 0x86 idle.
    let s = &layd.seqs[1];
    for g in [0x84usize, 0x85, 0x86, 0x87] {
        assert!(g < s.groups.len());
    }
}

#[test]
fn menu_text_ids_found_in_the_message_table() {
    let Some(mut a) = open() else { return };
    let bar = a.read("msg/us/sys.bar").expect("sys.bar");
    let m = Messages::from_bar(&bar).expect("messages");
    if let Ok(dir) = std::env::var("KH2_MENU_DUMP") {
        let mut ids: Vec<u32> = m.ids().collect();
        ids.sort_unstable();
        let mut out = String::new();
        for id in ids {
            out.push_str(&format!("{id:#06x}\t{}\n", m.text(id).unwrap_or_default().replace('\n', "\\n")));
        }
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("sys_messages.txt"), out).unwrap();
    }
    println!("sys.bar messages: {}", m.len());
    assert!(m.len() > 2000);
    // The top-level entries and their help lines (menu entry table at 0x3808a8; bit 15 of a field is ignored).
    for (id, text) in [(0x44b, "Items"), (0x44d, "Abilities"), (0x451, "Customize"), (0x44e, "Party"), (0x44f, "Status"), (0x450, "Journal"), (0x3617, "Config")] {
        assert_eq!(m.text(id).as_deref(), Some(text), "message {id:#x}");
    }
    assert_eq!(m.text(0x370f).as_deref(), Some("Equip Abilities."));
    assert_eq!(m.text(0x4539).as_deref(), Some("Journal"));
    // The summon option (docs/menus.md Config row 4): label and the three value lines.
    assert_eq!(m.text(0x42f7).as_deref(), Some("Summon Effects"));
    assert_eq!(m.text(0x4302).as_deref(), Some("Auto"));
    assert_eq!(m.text(0x4300).as_deref(), Some("Full"));
    assert_eq!(m.text(0x4301).as_deref(), Some("None"));
}
