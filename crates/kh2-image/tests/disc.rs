//! Tests against the user's disc (skipped without KH2_ISO or local/iso.txt): the font files, the
//! system message table and the font-style colours decode and agree with the game's code.

use kh2_image::font::{Font, FontSet};
use kh2_image::text::{self, ColorTable, Messages, Style, Token};

fn open() -> Option<kh2_archive::Archive> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return None;
    }
    kh2_archive::Archive::open(&path).ok()
}

fn fonts(a: &mut kh2_archive::Archive) -> FontSet {
    let info = a.read("msg/us/fontinfo.bar").expect("fontinfo.bar");
    let img = a.read("msg/us/fontimage.bar").expect("fontimage.bar");
    FontSet::parse(&info, &img).expect("fonts")
}

#[test]
fn width_tables_cover_every_glyph_cell() {
    let Some(mut a) = open() else { return };
    let set = fonts(&mut a);
    // One width per cell: 28 x 10 for the 18x24 font, 21 x 16 for the 24x32 font, 10 x 6 icons.
    assert_eq!(set.widths[0].len(), Font::get(0).per_page());
    assert_eq!(set.widths[1].len(), Font::get(3).per_page());
    assert_eq!(set.widths[2].len(), 60);
    // The first sys glyph is the 18 pixel wide block; digits are 15 wide.
    assert_eq!(set.width(&Font::get(0), 0), 18);
    assert_eq!(set.width(&Font::get(0), 1), 15);
}

#[test]
fn palette_rows_are_the_alpha_ramps() {
    let Some(mut a) = open() else { return };
    let set = fonts(&mut a);
    // Row 8 (the plain font): the low two bits of the nibble pick 0, 42, 98, 128 (0x80 = opaque).
    let alpha = |row: usize, n: usize| set.clut[row * 16 + n][3];
    assert_eq!([alpha(8, 0), alpha(8, 1), alpha(8, 2), alpha(8, 3)], [0, 42, 98, 128]);
    // Row 9 (page 1) picks by the high two bits.
    assert_eq!([alpha(9, 0), alpha(9, 4), alpha(9, 8), alpha(9, 12)], [0, 42, 98, 128]);
    // The atlas only uses the low two bits.
    let (w, h, rgba) = set.atlas_rgba(Font::get(0).atlas, 8);
    assert_eq!((w, h), (512, 256));
    assert!(rgba.chunks(4).any(|p| p[3] == 255));
}

#[test]
fn sys_messages_decode_to_known_text() {
    let Some(mut a) = open() else { return };
    let bar = a.read("msg/us/sys.bar").expect("sys.bar");
    let m = Messages::from_bar(&bar).expect("messages");
    assert!(m.len() > 2000);
    // Messages seen in the file: ability names and help text, with the apostrophe as a glyph.
    assert_eq!(m.text(0x3981).as_deref(), Some("Master Strike"));
    assert_eq!(m.text(0x3983).as_deref(), Some("Disaster"));
    assert_eq!(m.text(0x3944).as_deref(), Some("All"));
    assert_eq!(m.text(0x3941).as_deref(), Some("Use an item."));
    assert_eq!(m.text(0x3943).as_deref(), Some("Use on who?"));
    let t = m.text(0x3992).unwrap();
    assert!(t.contains("target\u{2019}s") || t.contains("target's"), "{t}");
    // Bit 15 of a table field is ignored.
    assert_eq!(m.text(0x3981 | 0x8000), m.text(0x3981));
    // The multi-line help text has two newline commands.
    assert_eq!(m.tokens(0x3994).unwrap().iter().filter(|t| **t == Token::Newline).count(), 2);
}

#[test]
fn every_message_tokenises_and_only_known_commands_appear() {
    let Some(mut a) = open() else { return };
    let bar = a.read("msg/us/sys.bar").expect("sys.bar");
    let m = Messages::from_bar(&bar).expect("messages");
    let mut ids: Vec<u32> = m.ids().collect();
    ids.sort_unstable();
    for id in ids {
        let t = m.tokens(id).unwrap();
        assert!(!t.is_empty());
        assert_eq!(t.last(), Some(&Token::End), "{id:#x}");
        for k in &t {
            // Every command the file uses has a token of its own now (0x08 and 0x13 are callbacks,
            // 0x11 a position, 0x14 a wait); nothing is left as a bare command.
            if let Token::Command(op, args) = k {
                panic!("{id:#x}: unexpected command {op:#x} {args:?}");
            }
            if let Token::Callback(op, _) = k {
                assert!(matches!(op, 0x08 | 0x13), "{id:#x}: unexpected callback {op:#x}");
            }
        }
    }
}

/// The phase-three commands as they occur in the real system text: positions, waits and callbacks
/// parse with the game's lengths and lay out without panicking; a message that sets the pen puts
/// its later glyphs at the requested x.
#[test]
fn phase_three_commands_in_real_messages() {
    let Some(mut a) = open() else { return };
    let set = fonts(&mut a);
    let bar = a.read("msg/us/sys.bar").expect("sys.bar");
    let m = Messages::from_bar(&bar).expect("messages");
    let mut ids: Vec<u32> = m.ids().collect();
    ids.sort_unstable();
    let (mut positions, mut waits, mut callbacks) = (0, 0, 0);
    for id in &ids {
        let t = m.tokens(*id).unwrap();
        for k in &t {
            match k {
                Token::Position(..) => positions += 1,
                Token::Wait(_) => waits += 1,
                Token::Callback(..) => callbacks += 1,
                _ => {}
            }
        }
        let l = text::layout(&set, None, &t, &Style { font: 1, size: Some(14.0), ..Style::default() }, [0.0, 0.0]);
        assert!(l.complete, "{id:#x}: no typewriter in the system text, so nothing is cut");
        if let Some(Token::Position(dx, _)) = t.iter().find(|k| matches!(k, Token::Position(..))) {
            // The first glyph after the position starts at dx from the origin.
            let after = t.iter().position(|k| matches!(k, Token::Position(..))).unwrap();
            let glyphs_before = t[..after].iter().filter(|k| matches!(k, Token::Glyph(_) | Token::Icon(_))).count();
            if let Some(q) = l.quads.get(glyphs_before) {
                assert_eq!(q.pos[0], *dx as f32, "{id:#x}");
            }
        }
    }
    eprintln!("sys.bar: {positions} position commands, {waits} waits, {callbacks} callbacks");
    assert!(positions > 0 && waits > 0 && callbacks > 0);
}

#[test]
fn layout_advances_by_the_width_table() {
    let Some(mut a) = open() else { return };
    let set = fonts(&mut a);
    let bar = a.read("msg/us/sys.bar").expect("sys.bar");
    let m = Messages::from_bar(&bar).expect("messages");
    let t = m.tokens(0x3981).unwrap();
    // At the font's own size every glyph advances by its table width (scale 1).
    let lay = text::layout(&set, None, &t, &Style { font: 0, ..Style::default() }, [0.0, 0.0]);
    let font = Font::get(0);
    let mut expect = 0.0;
    for k in &t {
        match k {
            Token::Glyph(g) => expect += set.width(&font, *g as usize) as f32,
            Token::Space => expect += 6.0,
            _ => {}
        }
    }
    assert_eq!(lay.size[0], expect);
    // A glyph's cell is 18x24 and sits on the baseline, 18 below the line top.
    let q = lay.quads[0];
    assert_eq!((q.pos[2] - q.pos[0], q.pos[3] - q.pos[1]), (18.0, 24.0));
    assert_eq!(q.pos[3], 18.0);
    // Size 14 scales cells and advances by 14/18.
    let small = text::layout(&set, None, &t, &Style { font: 0, size: Some(9.0), ..Style::default() }, [0.0, 0.0]);
    assert!(small.size[0] < lay.size[0] * 0.55 && small.size[0] > lay.size[0] * 0.4);
}

#[test]
fn font_style_colours_read_from_the_system_table() {
    let Some(mut a) = open() else { return };
    let bin = a.read("03system.bin").expect("03system.bin");
    let entries = kh2_bar::parse(&bin).unwrap();
    let ftst = entries.iter().find(|e| e.tag == "ftst").expect("ftst").data;
    let t = ColorTable::parse(ftst).expect("colours");
    // 9 records by 19 world columns.
    assert_eq!(ftst.len(), 8 + 9 * 0x50);
    // Id 1 is red in every world column; ids are not contiguous (9 is absent).
    assert_eq!(t.color(1, 2), Some([0xff, 0x20, 0x20, 0x80]));
    assert!(t.color(9, 2).is_none());
    assert!(t.color(1, 19).is_none());
}

/// The message-window frames: the `md_m` sequence of fontinfo.bar holds the ten frame kinds as
/// group triples from 67 (open, idle, close; the string at 0x35ea48), each drawing a nine-part
/// sprite group with its own margins in the group record; the world message files carry the
/// 256x128 frame image (`md_m`, loaded at 0x190b70 into the window texture 0x1c95710), the system
/// file an empty one.
#[test]
fn window_frames_in_the_font_and_world_files() {
    use kh2_image::window::{frame_group, Phase, KINDS};
    let Some(mut a) = open() else { return };
    let info = a.read("msg/us/fontinfo.bar").expect("fontinfo.bar");
    let entries = kh2_bar::parse(&info).unwrap();
    let seq = entries.iter().find(|e| e.tag == "md_m" && e.kind == 25).expect("md_m sequence").data;
    let seq = kh2_image::seqd::parse(seq).expect("seqd");
    assert_eq!(seq.groups.len(), 97);
    let mut margins = Vec::new();
    for kind in 0..KINDS {
        let (open, idle, close) = (frame_group(kind, Phase::Open), frame_group(kind, Phase::Idle), frame_group(kind, Phase::Close));
        let g = seq.groups[idle];
        margins.push((g.x, g.y));
        // Opening and closing play once and hold; the idle group loops.
        assert_eq!((seq.groups[open].loops, g.loops, seq.groups[close].loops), (1, 0, 1), "kind {kind}");
        for grp in [open, idle, close] {
            let first = &seq.anims[seq.groups[grp].first];
            assert_eq!(seq.sprite_groups[first.group].count, 9, "kind {kind} group {grp}");
        }
    }
    assert_eq!(&margins[..5], &[(16, 8), (10, 8), (16, 8), (8, 8), (0, 0)]);
    // Kind 0: 16x16 corners cut from the top-left 32x48 of the image.
    let mut s = seq.clone();
    s.resize_frame(frame_group(0, Phase::Idle), 200, 40);
    let sg = seq.sprite_groups[seq.anims[seq.groups[frame_group(0, Phase::Idle)].first].group];
    let p = &s.parts[sg.start..sg.start + 9];
    assert_eq!([p[4].left, p[4].top, p[4].right, p[4].bottom], [-108, -24, 108, 24]);
    assert_eq!([p[0].left, p[0].top, p[0].right, p[0].bottom], [-124, -40, -108, -24]);
    let corner = seq.sprites[seq.parts[sg.start].sprite];
    assert_eq!([corner.u0, corner.v0, corner.u1, corner.v1], [0, 0, 16, 16]);
    // The system message file's frame image is empty.
    let sys = a.read("msg/us/sys.bar").expect("sys.bar");
    let e = kh2_bar::parse(&sys).unwrap();
    assert!(e.iter().any(|e| e.tag == "md_m" && e.kind == 24 && e.data.is_empty()));
    // The world files live in the world sub-indexes; with the name list they can be read.
    let Ok(names) = std::fs::read_to_string("../../local/kh2idx.txt") else {
        eprintln!("skipped the world file part: no local/kh2idx.txt");
        return;
    };
    a.apply_names(names.lines()).unwrap();
    let tt = a.read("msg/us/tt.bar").expect("tt.bar");
    let e = kh2_bar::parse(&tt).unwrap();
    let img = e.iter().find(|e| e.tag == "md_m" && e.kind == 24).expect("md_m image").data;
    let img = kh2_image::imgd::decode(img).expect("imgd");
    assert_eq!((img.width, img.height), (256, 128));
}