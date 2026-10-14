//! The Seifer intro's lens (`msn/us/TT04_MS500.bar`, effect 0, started by the start event's opcode 0x3c), read
//! from the disc (skipped without KH2_ISO or local/iso.txt).

use kh2_fx::Pax;

fn mission_pax() -> Option<Pax> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    // The mission files sit in the world sub-index `000tt.idx` (as the app's `assets::Disc` opens it).
    a.apply_names(vec!["000tt.idx"]).ok()?;
    let file = a.read("msn/us/TT04_MS500.bar").ok()?;
    let entries = kh2_bar::parse(&file).ok()?;
    Pax::parse(entries.iter().find(|e| e.kind == 18)?.data).ok()
}

/// Effect 0 is one element in camera space (flag 0x20000) whose particle data lives 12 frames (0xc000) and holds
/// ONE `pppSnoMdlDttNorm` program (group 2) with three keyed entries (frames 0, 8, 12): one lens draw per frame.
/// Every entry has byte +0x14 set, which makes the draw handler (0x20a7e8) build the lens texture from the
/// globals of the display buffer (`DAT_00360b04`, `DAT_00360b08`, `DAT_00360b1c`: 512 x 448 at GS page 0, set by
/// `FUN_001da4b8` -> `FUN_001e4350`; `FUN_0014dcd0` makes page 0 the display buffer and page 0xd8 the 3D draw
/// buffer), byte +0x15 (the taper) set, +0x16 clear (stored normals), +0x17 set (no depth test), +0x1c zero
/// (ALPHA 0x44).
#[test]
fn the_intro_lens_reads_the_display_buffer() {
    let Some(pax) = mission_pax() else {
        eprintln!("skipped: no disc (set KH2_ISO)");
        return;
    };
    let els: Vec<_> = pax.elements_for(0).collect();
    assert_eq!(els.len(), 1);
    assert_eq!(els[0].flags & 0x20000, 0x20000, "camera space");
    let fx = pax.dpx_effect(els[0].effect).expect("dpx");
    let pd = &pax.dpds[fx.dpd].particles[fx.particle_data];
    assert_eq!(pd.life, 12 * kh2_fx::pax::TICK, "12 program frames");
    let sno: Vec<_> = pd.groups.iter().flat_map(|g| &g.programs).filter(|p| kh2_fx::ppp::name(p.kind) == "pppSnoMdlDttNorm").collect();
    assert_eq!(sno.len(), 1, "one lens program");
    let p = sno[0];
    assert_eq!(p.entries(), 3);
    for e in 0..p.entries() {
        let b = &p.params[e * p.param_size..(e + 1) * p.param_size];
        eprintln!("entry {e}: frame {:#x} bytes +0x14..+0x18 {:02x?} alpha {:#x}", p.frame_at(e * p.param_size), &b[0x14..0x18], b[0x1c]);
        assert_eq!((b[0x14], b[0x15], b[0x16], b[0x17], b[0x1c]), (1, 1, 0, 1, 0), "entry {e}");
    }
}
