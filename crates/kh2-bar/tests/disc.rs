//! Disc tests of the resident BAR helpers on real files: the lookups agree with the container parser, and the
//! first fix-up turns every payload offset into an address inside the file. Skips without a disc
//! (fails with `KH2_REQUIRE_DISC=1`).

use kh2_bar::resident;

#[test]
fn lookups_and_attach_agree_with_the_parser_on_real_bars() {
    let Some(path) = kh2_factory::disc_path("resident BAR disc test") else { return };
    let mut arc = kh2_archive::Archive::open(&path).expect("disc");
    let list = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../local/kh2idx.txt")).unwrap_or_default();
    arc.apply_names(if list.is_empty() { vec!["000tt.idx"] } else { list.lines().collect() }).expect("names");
    let names: Vec<String> = arc.entries().iter().filter_map(|l| l.name.clone()).filter(|n| n.ends_with(".bar") || n.ends_with(".ard")).take(60).collect();
    let mut checked = 0;
    for name in names {
        let Ok(data) = arc.read(&name) else { continue };
        let Ok(entries) = kh2_bar::parse(&data) else { continue };
        let base = 0x0100_0000u32;
        assert_eq!(resident::count_of_type(&data, base, base, 0xffff_ffff), 0, "{name}: no entry has the 32-bit wildcard as its type");
        for ty in entries.iter().map(|e| e.kind as u32) {
            let want = entries.iter().filter(|e| e.kind as u32 == ty).count() as i32;
            assert_eq!(resident::count_of_type(&data, base, base, ty), want, "{name}: type {ty}");
            let first = resident::nth_of_type(&data, base, base, ty, 0).expect("first entry of a present type");
            let k = (first - base - resident::FIRST) / resident::ENTRY;
            assert_eq!(entries[k as usize].kind as u32, ty, "{name}");
            assert!(entries[..k as usize].iter().all(|e| e.kind as u32 != ty), "{name}: not the first");
        }
        let mut img = data.clone();
        resident::attach(&mut img, base, base, &mut |_, _| {});
        for (k, e) in entries.iter().enumerate() {
            let at = (resident::FIRST as usize) + k * resident::ENTRY as usize;
            let off = u32::from_le_bytes(img[at + 8..at + 12].try_into().unwrap());
            if e.data.is_empty() {
                assert_eq!(off, 0, "{name}: entry {k} without payload");
            } else {
                assert!(off >= base + 0x10 && (off - base) as usize + e.data.len() <= data.len(), "{name}: entry {k} offset {off:#x}");
            }
        }
        checked += 1;
    }
    assert!(checked > 5, "the disc must hold BAR files ({checked})");
}
