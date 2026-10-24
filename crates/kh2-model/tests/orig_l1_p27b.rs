//! Oracle tests of the texture animation leaves that had no test named after their address (WP-L1-P27b): kh2-ee runs the original
//! from the user's own executable and the port is `kh2_model::texanim` (`Texa::default_anim`, `TexaPlayer::sprite`,
//! `UvscPlayer::offset`). Their long oracle (`oracle.rs`, `orig_l1_p23.rs`) covers the update loops; these four are the
//! functions the update loops call and the ledger scan could not attribute.
//!
//! The TEXA blocks come from Sora's model on the disc (`obj/P_EX100.mdlx`), laid into guest memory as the original keeps them (the
//! block is the footer block of the animation object, `+0x10`). The port runs on the container bytes. Nothing is stubbed.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_model::texanim::{self, Texa, TexaPlayer, UvscPlayer};

const STACK: u32 = 0x01ff_0000;
const TAG: u32 = 0x0180_0000;
const OBJ: u32 = 0x0190_0000;

fn open(what: &str) -> Option<(Ee, kh2_archive::Archive)> {
    let path = kh2_factory::disc_path(what)?;
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    let names = std::fs::read_to_string("../../local/kh2idx.txt").unwrap_or_default();
    let subs: Vec<&str> = names.lines().filter(|l| l.ends_with(".idx")).collect();
    a.apply_names(if subs.is_empty() { vec!["000tt.idx"] } else { subs }).ok()?;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some((ee, a))
}

fn u16_at(b: &[u8], o: usize) -> u32 {
    u16::from_le_bytes([b[o], b[o + 1]]) as u32
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn clear(ee: &mut Ee, at: u32, len: u32) {
    for o in (0..len).step_by(4) {
        ee.w32(at + o, 0);
    }
}

/// The texture container of a model file (the first BAR entry of type 7).
fn container(a: &mut kh2_archive::Archive, file: &str) -> Vec<u8> {
    let f = a.read(file).expect(file);
    let e = kh2_bar::parse(&f).expect("bar");
    e.iter().find(|x| x.kind == 7).expect("textures").data.to_vec()
}

/// (offset in the container, whole block including tag and size) of every TEXA block of the footer, in order.
fn texa_blocks(tex: &[u8]) -> Vec<(usize, Vec<u8>)> {
    let mut out = Vec::new();
    let Some(mut o) = texanim::footer_offset(tex) else { return out };
    while let Some(tag) = tex.get(o..o + 4) {
        if !matches!(tag, b"TEXA" | b"UVSC" | b"_DMY") {
            break;
        }
        let size = u32_at(tex, o + 4) as usize;
        if tag == b"TEXA" {
            out.push((o, tex[o..o + 8 + size].to_vec()));
        }
        o += 8 + size;
    }
    out
}

/// orig 0x1328a8: sets the footer block and requests the default animation. The default animation index is the word at body `+0x24`;
/// the request is the first frame of that animation, or stays as it was when the index is not valid. The word is replaced by random
/// values around the animation count so that valid and invalid indices both occur.
#[test]
fn orig_1328a8_matches_the_game() {
    let Some((mut ee, mut a)) = open("0x1328a8") else { return };
    let tex = container(&mut a, "obj/P_EX100.mdlx");
    let blocks = texa_blocks(&tex);
    assert!(!blocks.is_empty());
    let (mut valid, mut invalid) = (0, 0);
    let mut seed = 0x1328a8u32;
    for (bi, (at, block)) in blocks.iter().enumerate() {
        let count = u16_at(block, 8 + 0xc);
        for k in 0..400u32 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let value = match k % 4 {
                0 => k / 4 % (count + 3),
                1 => seed % (count + 3),
                2 => seed,
                _ => count.wrapping_sub(1).wrapping_add(k / 4 % 3),
            };
            let mut patched = tex.clone();
            patched[at + 8 + 0x24..at + 8 + 0x28].copy_from_slice(&value.to_le_bytes());
            let mut raw = block.clone();
            raw[8 + 0x24..8 + 0x28].copy_from_slice(&value.to_le_bytes());
            let anims = texanim::parse(&patched);
            assert_eq!(anims.texa.len(), blocks.len(), "every block parses");
            let t: &Texa = &anims.texa[bi];
            ee.write(TAG, &raw);
            clear(&mut ee, OBJ, 0x290);
            ee.w32(OBJ + 0x274, 0xdead_0000);
            ee.call(0x1328a8, &[OBJ as u64, TAG as u64], &[], STACK).expect("FUN_001328a8");
            assert_eq!(ee.r32(OBJ + 0x10), TAG, "the block is stored");
            let want = match t.default_anim() {
                Some(i) => {
                    valid += 1;
                    let table = u32_at(&raw, 8 + 0x1c) as usize;
                    TAG + 8 + u32_at(&raw, 8 + table + 4 * i)
                }
                None => {
                    invalid += 1;
                    0xdead_0000
                }
            };
            assert_eq!(ee.r32(OBJ + 0x274), want, "block at {at:#x}, default word {value:#x}");
        }
    }
    assert!(valid > 100 && invalid > 100, "{valid} valid and {invalid} invalid defaults");
}

/// orig 0x132fc0: the address of the sprite image the current frame shows, or 0 (no block, no current frame, or a frame whose code
/// is not 0). The port gives the sprite index; the address is the data start of the block plus the index times the sprite size.
/// A frame whose sprite index is beyond the TEXA's decoded sprites shows nothing in the port (the original would hand out an address
/// past the data); the test checks that the original still answers an address there.
#[test]
fn orig_132fc0_matches_the_game() {
    let Some((mut ee, mut a)) = open("0x132fc0") else { return };
    let tex = container(&mut a, "obj/P_EX100.mdlx");
    let model = kh2_model::mdlx(&a.read("obj/P_EX100.mdlx").unwrap()).unwrap();
    let blocks = texa_blocks(&tex);
    assert_eq!(blocks.len(), model.anims.texa.len());
    let (mut shown, mut hidden, mut none, mut past) = (0, 0, 0, 0);
    for ((_, block), t) in blocks.iter().zip(&model.anims.texa) {
        ee.write(TAG, block);
        let body = TAG + 8;
        let entry = t.entry_size();
        let size = u32::from_le_bytes(block[4..8].try_into().unwrap()) as usize;
        let data = u32_at(block, 8 + 0x20);
        let sprite_bytes = (u16_at(block, 8 + 0x14) * u16_at(block, 8 + 6)) >> 3;
        let mut frames = vec![0usize];
        for anim in 0..t.anim_count() as usize {
            if let Some(start) = t.anim_start(anim) {
                frames.extend((0..6).map(|k| start + k * entry));
            }
        }
        frames.retain(|&f| f == 0 || f + 8 <= size);
        for &f in &frames {
            clear(&mut ee, OBJ, 0x290);
            ee.w32(OBJ + 0x10, TAG);
            ee.w32(OBJ + 0x27c, if f == 0 { 0 } else { body + f as u32 });
            let got = ee.call(0x132fc0, &[OBJ as u64, 0], &[], STACK).expect("FUN_00132fc0") as u32;
            let mut p = TexaPlayer::new(t);
            p.frame = (f != 0).then_some(f);
            if f == 0 {
                assert_eq!(got, 0, "no current frame");
                none += 1;
                continue;
            }
            let frame = t.frame(f).unwrap();
            match p.sprite(t) {
                Some(s) => {
                    assert_eq!(got, body + data + s as u32 * sprite_bytes * u16_at(block, 8 + 0x16), "frame at {f:#x}: sprite {s}");
                    shown += 1;
                }
                None if frame.code() != 0 => {
                    assert_eq!(got, 0, "frame at {f:#x} with code {}", frame.code());
                    hidden += 1;
                }
                None => {
                    assert_ne!(got, 0, "frame at {f:#x}: sprite {} is past the decoded sprites", frame.sprite);
                    past += 1;
                }
            }
        }
    }
    assert!(shown > 20 && none > 0, "{shown} shown, {hidden} hidden, {none} none, {past} past the decoded sprites");
    eprintln!("{shown} shown, {hidden} hidden, {none} none, {past} past the decoded sprites");
}

fn offset_test(addr: u32, acc_at: u32) {
    let Some((mut ee, _)) = open(&format!("{addr:#x}")) else { return };
    let mut n = 0;
    for k in 0..4000u32 {
        let acc = k.wrapping_mul(2_654_435_761);
        for (span, size) in [(1u32, 256u32), (256, 256), (128, 256), (64, 128), (32, 32), (1024, 128), (7, 3), (2048, 16), (3, 1024)] {
            ee.w32(OBJ + acc_at, acc);
            let got = ee.call(addr, &[OBJ as u64, span as u64, size as u64], &[], STACK).expect("the original") as u32;
            assert_eq!(got, UvscPlayer::offset(acc as i32, span, size), "acc {acc:#x} span {span} size {size}");
            n += 1;
        }
    }
    assert!(n > 30_000);
}

/// orig 0x133628: the U offset (accumulator at `+0x24`, 12 bits of `acc * span / size`).
#[test]
fn orig_133628_matches_the_game() {
    offset_test(0x133628, 0x24);
}

/// orig 0x1336d8: the V offset (accumulator at `+0x28`; the same arithmetic).
#[test]
fn orig_1336d8_matches_the_game() {
    offset_test(0x1336d8, 0x28);
}
