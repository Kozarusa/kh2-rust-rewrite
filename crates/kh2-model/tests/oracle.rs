//! Texture animation ports against the game's own routines run in kh2-ee on the user's executable
//! (skipped without KH2_ISO or local/iso.txt).
//!
//! - FUN_00132a80 (TEXA update, with FUN_00132950 start) and FUN_00132fc0 (current sprite) on every
//!   animation of Sora's two TEXA blocks, field by field.
//! - FUN_00133518 (UVSC update) on the tt04 sky's UVSC blocks.
//! - FUN_00133628 (U offset) over a sweep of accumulators, spans and sizes.
//!
//! `rand()` (FUN_003117a8) is replaced by a stub returning a constant: it reads and advances the
//! game's global generator, which the port cannot share; the port is fed the same constant.

use kh2_ee::Ee;
use kh2_model::texanim::{self, Texa, TexaPlayer, UvscPlayer};

const STACK: u32 = 0x01ff_0000;
const TAG: u32 = 0x0180_0000;
const OBJ: u32 = 0x0190_0000;
const RAND: u32 = 0x0031_17a8;

fn open() -> Option<(Ee, kh2_archive::Archive)> {
    let path = std::env::var("KH2_ISO")
        .ok()
        .or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))
        .filter(|p| std::path::Path::new(p).exists());
    let Some(path) = path else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return None;
    };
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    let names = std::fs::read_to_string("../../local/kh2idx.txt").unwrap_or_default();
    let subs: Vec<&str> = names.lines().filter(|l| l.ends_with(".idx")).collect();
    a.apply_names(if subs.is_empty() { vec!["000tt.idx"] } else { subs }).ok()?;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some((ee, a))
}

/// `rand()` stub: `lui v0, hi; jr ra; ori v0, v0, lo`.
fn stub_rand(ee: &mut Ee, v: u32) {
    ee.w32(RAND, 0x3c02_0000 | v >> 16);
    ee.w32(RAND + 4, 0x03e0_0008);
    ee.w32(RAND + 8, 0x3442_0000 | (v & 0xffff));
}

/// The tagged blocks of a texture container's footer: (tag, whole block incl. tag and size).
fn blocks(tex: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut out = Vec::new();
    let Some(mut o) = texanim::footer_offset(tex) else { return out };
    while let Some(tag) = tex.get(o..o + 4) {
        let tag: [u8; 4] = tag.try_into().unwrap();
        if !matches!(&tag, b"TEXA" | b"UVSC" | b"_DMY") {
            break;
        }
        let size = u32::from_le_bytes(tex[o + 4..o + 8].try_into().unwrap()) as usize;
        out.push((tag, tex[o..o + 8 + size].to_vec()));
        o += 8 + size;
    }
    out
}

fn container(a: &mut kh2_archive::Archive, file: &str, tag: Option<&str>) -> Vec<u8> {
    let f = a.read(file).expect(file);
    let e = kh2_bar::parse(&f).expect("bar");
    e.iter().find(|x| x.kind == 7 && tag.is_none_or(|t| x.tag == t)).expect("textures").data.to_vec()
}

fn clear(ee: &mut Ee, at: u32, len: u32) {
    for o in (0..len).step_by(4) {
        ee.w32(at + o, 0);
    }
}

#[test]
fn texa_update_matches_the_game() {
    let Some((mut ee, mut a)) = open() else { return };
    let tex = container(&mut a, "obj/P_EX100.mdlx", None);
    let model = kh2_model::mdlx(&a.read("obj/P_EX100.mdlx").unwrap()).unwrap();
    let raw: Vec<Vec<u8>> = blocks(&tex).into_iter().filter(|(t, _)| t == b"TEXA").map(|(_, b)| b).collect();
    assert_eq!(raw.len(), model.anims.texa.len());
    let mut checked = 0;
    for r in [0u32, 0x2aaa_aaaa, 0x7fff_0000] {
        stub_rand(&mut ee, r);
        for (t, block) in model.anims.texa.iter().zip(&raw) {
            ee.write(TAG, block);
            let body = TAG + 8;
            for anim in 0..t.anim_count() as usize {
                let Some(start) = t.anim_start(anim) else { continue };
                clear(&mut ee, OBJ, 0x290);
                ee.w32(OBJ + 0x10, TAG);
                ee.wf32(OBJ + 0x18, 1.0);
                ee.w32(OBJ + 0x274, body + start as u32);
                let mut p = TexaPlayer::new(t);
                p.request = Some(start);
                let mut rand = || r;
                for field in 0..900 {
                    let dt = if field % 7 == 3 { 2.0 } else { 1.0 };
                    ee.call(0x132a80, &[OBJ as u64], &[dt], STACK).expect("FUN_00132a80");
                    p.update(t, dt, &mut rand);
                    let frame = ee.r32(OBJ + 0x27c);
                    let want = p.frame.map_or(0, |f| body + f as u32);
                    assert_eq!(frame, want, "texa on texture {} anim {anim} field {field}: frame pointer", t.texture);
                    assert_eq!(ee.f32_at(OBJ + 0x280), p.length, "anim {anim} field {field}: frame length");
                    assert_eq!(ee.f32_at(OBJ + 0x284), p.time, "anim {anim} field {field}: time in frame");
                    let sprite = ee.call(0x132fc0, &[OBJ as u64, 0], &[], STACK).expect("FUN_00132fc0") as u32;
                    assert_eq!(sprite != 0, sprite_shown(t, &p), "anim {anim} field {field}: sprite shown");
                    checked += 1;
                }
            }
        }
    }
    eprintln!("{checked} TEXA fields compared");
}

fn sprite_shown(t: &Texa, p: &TexaPlayer) -> bool {
    p.sprite(t).is_some()
}

#[test]
fn uvsc_update_and_offset_match_the_game() {
    let Some((mut ee, mut a)) = open() else { return };
    let tex = container(&mut a, "map/us/tt04.map", Some("SK0"));
    let raw: Vec<Vec<u8>> = blocks(&tex).into_iter().filter(|(t, _)| t == b"UVSC").map(|(_, b)| b).collect();
    let sky = kh2_model::map_file(&a.read("map/us/tt04.map").unwrap()).unwrap().sky.unwrap();
    assert!(!raw.is_empty() && raw.len() == sky.anims.uvsc.len());
    for (u, block) in sky.anims.uvsc.iter().zip(&raw) {
        ee.write(TAG, block);
        clear(&mut ee, OBJ, 0x2c);
        ee.w32(OBJ + 0x10, TAG);
        ee.wf32(OBJ + 0x18, 1.0);
        let mut p = UvscPlayer::default();
        for field in 0..5000 {
            let dt = [1.0, 2.0, 0.5][field % 3];
            ee.call(0x133518, &[OBJ as u64], &[dt], STACK).expect("FUN_00133518");
            p.update(u, dt);
            let got = (ee.r32(OBJ + 0x24) as i32, ee.r32(OBJ + 0x28) as i32, ee.f32_at(OBJ + 0xc));
            assert_eq!(got, (p.acc_u, p.acc_v, p.time), "uvsc {} field {field}", u.index);
        }
    }
    // The offset over accumulators of both signs, spans and texture sizes.
    let mut n = 0;
    for k in 0..4000u32 {
        let acc = (k.wrapping_mul(2_654_435_761)) as i32;
        for (span, size) in [(1u32, 256u32), (256, 256), (128, 256), (64, 128), (32, 32), (1024, 128)] {
            ee.w32(OBJ + 0x24, acc as u32);
            let got = ee.call(0x133628, &[OBJ as u64, span as u64, size as u64], &[], STACK).expect("FUN_00133628") as u32;
            assert_eq!(got, UvscPlayer::offset(acc, span, size), "acc {acc:#x} span {span} size {size}");
            n += 1;
        }
    }
    eprintln!("{n} UV offsets compared");
}
