//! Tests on hand-built data, so no game data is needed.

use kh2_ps2::gs;

use crate::imgd::{decode, split_imgz};
use crate::seqd::{parse, Ext, Player};

fn le(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn lef(v: &mut Vec<u8>, x: f32) {
    le(v, x.to_bits());
}

/// One sprite (2x2 texels at 4,4), one part (-8..8 square), one group, two animations, one
/// animation group.
fn sample() -> Vec<u8> {
    let mut b = b"SEQD".to_vec();
    le(&mut b, 0x100);
    // sprites, parts, sprite groups, animations, animation groups: (count, offset)
    let off = [0x30u32, 0x30 + 0x2c, 0x30 + 0x2c + 0x14, 0x30 + 0x2c + 0x14 + 4, 0x30 + 0x2c + 0x14 + 4 + 2 * 0x90];
    let counts = [1u32, 1, 1, 2, 1];
    for (c, o) in counts.iter().zip(off) {
        le(&mut b, *c);
        le(&mut b, o);
    }
    // sprite: palette, u0 v0 u1 v1, scrolls, four corner colours
    for v in [0, 4, 4, 6, 6] {
        le(&mut b, v);
    }
    lef(&mut b, 0.0);
    lef(&mut b, 0.0);
    for _ in 0..4 {
        b.extend_from_slice(&[0x80, 0x80, 0x80, 0x80]);
    }
    // part: left, top, right, bottom, sprite
    for v in [-8i32, -8, 8, 8, 0] {
        le(&mut b, v as u32);
    }
    // sprite group: start 0, count 1
    b.extend_from_slice(&[0, 0, 1, 0]);
    // animation 0: frames 0..10, linear and flagged as the anchor, x 0 -> 100
    // animation 1: frames 11..20, bezier with control values 0 and 1 (ease in and out), x 0 -> 100
    for (flags, fs, fe) in [(3u32, 0u32, 10u32), (0, 11, 20)] {
        le(&mut b, flags);
        le(&mut b, 0);
        le(&mut b, fs);
        le(&mut b, fe);
        // translate x, y; pivot x, y (start, end each)
        for v in [0i32, 100, 0, 0, 0, 0, 0, 0] {
            le(&mut b, v as u32);
        }
        for _ in 0..6 {
            lef(&mut b, 0.0); // rotations
        }
        for _ in 0..6 {
            lef(&mut b, 1.0); // scale, scale x, scale y
        }
        // curve x start, curve y start, curve x end, curve y end
        for v in [0.0, 0.0, 0.0, 1.0] {
            lef(&mut b, v);
        }
        for _ in 0..4 {
            lef(&mut b, 0.0); // bounce
        }
        b.extend_from_slice(&[0; 4]); // bounce counts
        le(&mut b, 0); // blend
        b.extend_from_slice(&[0x80; 8]);
    }
    // animation group: animations 0..2, plays forever, loop point 0, x and y 5 and 7
    b.extend_from_slice(&[0, 0, 2, 0, 0, 0, 0, 0]);
    for v in [0u32, 0, 5, 7, 0, 0, 0] {
        le(&mut b, v);
    }
    b
}

#[test]
fn parses_the_documented_layout() {
    let s = parse(&sample()).expect("parse");
    assert_eq!((s.sprites.len(), s.parts.len(), s.anims.len(), s.groups.len()), (1, 1, 2, 1));
    assert_eq!((s.sprites[0].u0, s.sprites[0].v1), (4, 6));
    assert_eq!((s.groups[0].x, s.groups[0].y), (5, 7));
    assert_eq!(s.anims[1].curve_end, 1.0);
    assert!(parse(b"SEQD").is_none());
}

#[test]
fn linear_translation_and_group_length() {
    let s = parse(&sample()).unwrap();
    assert_eq!(s.group_len(0), 21);
    let q = s.quads(0, 5, [10.0, 20.0], &Ext::default());
    assert_eq!(q.len(), 1);
    // Halfway through 0..100 plus the origin, part corner (-8, -8).
    assert_eq!(q[0].pos[0], [10.0 + 50.0 - 8.0, 20.0 - 8.0]);
    assert_eq!(q[0].pos[3], [10.0 + 50.0 + 8.0, 20.0 + 8.0]);
    assert_eq!(q[0].uv[0], [4.0, 4.0]);
}

#[test]
fn bezier_time_curve_eases_and_hits_the_ends() {
    let s = parse(&sample()).unwrap();
    let x = |f: i32| s.quads(0, f, [0.0, 0.0], &Ext::default())[0].pos[0][0] + 8.0;
    assert_eq!(x(11), 0.0);
    assert!((x(20) - 100.0).abs() < 1e-3);
    // Control values 0 and 1 make an ease-in/ease-out curve: slow at the start.
    assert!(x(13) < 100.0 * 2.0 / 9.0);
    let mid = (x(15) + x(16)) / 2.0;
    assert!((mid - 50.0).abs() < 8.0, "{mid}");
}

#[test]
fn anchor_follows_the_flagged_animation() {
    let s = parse(&sample()).unwrap();
    assert_eq!(s.anchor(0, 10, &Ext::default()), Some([100, 0]));
    // The second animation is not flagged, so after frame 10 there is no anchor.
    assert_eq!(s.anchor(0, 15, &Ext::default()), None);
}

#[test]
fn player_loops_and_finishes() {
    let s = parse(&sample()).unwrap();
    let mut p = Player::default();
    p.set_group(&s, 0);
    p.tick(1.0);
    assert_eq!(p.frame(), 0);
    for _ in 0..20 {
        p.tick(1.0);
    }
    assert_eq!(p.frame(), 20);
    p.tick(1.0);
    // Past the end it wraps to the loop point (0 plays means forever).
    assert_eq!(p.frame(), 0);
    let mut once = s.clone();
    once.groups[0].loops = 1;
    p.set_group(&once, 0);
    for _ in 0..40 {
        p.tick(1.0);
    }
    assert!(p.finished());
    assert_eq!(p.frame(), 20);
}

#[test]
fn zero_width_sprite_samples_the_texel_centre() {
    let mut s = parse(&sample()).unwrap();
    s.sprites[0].u1 = s.sprites[0].u0;
    let q = s.quads(0, 0, [0.0, 0.0], &Ext::default());
    assert_eq!((q[0].uv[0][0], q[0].uv[1][0]), (4.5, 4.5));
}

#[test]
fn extra_rotation_turns_the_quad() {
    let s = parse(&sample()).unwrap();
    let ext = Ext { rot: Some([0.0, 0.0, std::f32::consts::FRAC_PI_2]), ..Ext::default() };
    let q = s.quads(0, 0, [0.0, 0.0], &ext);
    // x' = x cos + y sin, y' = -x sin + y cos: the corner (-8, -8) goes to (-8, 8).
    assert!((q[0].pos[0][0] + 8.0).abs() < 1e-4 && (q[0].pos[0][1] - 8.0).abs() < 1e-4, "{:?}", q[0].pos[0]);
}

/// An 8-bit swizzled IMGD with the given upload canvas and palette.
fn imgd8(w: u16, h: u16, canvas: &[u8], pal: &[u8]) -> Vec<u8> {
    let mut b = b"IMGD".to_vec();
    for v in [0x100u32, 0x40, canvas.len() as u32, 0x40 + canvas.len() as u32, pal.len() as u32, 0xffff_ffff] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&w.to_le_bytes());
    b.extend_from_slice(&h.to_le_bytes());
    b.resize(0x34, 0);
    b.extend_from_slice(&5u16.to_le_bytes());
    b.resize(0x3c, 0);
    b.extend_from_slice(&7u32.to_le_bytes());
    b.extend_from_slice(canvas);
    b.extend_from_slice(pal);
    b
}

#[test]
fn swizzled_eight_bit_round_trips_through_gs_memory() {
    let (w, h) = (128usize, 64usize);
    let lin: Vec<u8> = (0..w * h).map(|p| ((p % w) * 7 + (p / w) * 13) as u8).collect();
    // Put the linear indices into GS memory as PSMT8, then find the PSMCT32 canvas whose upload
    // gives that memory (the inverse of the page encoder, found with a probe of unique values).
    let mut vram = gs::Vram::default();
    vram.write8(0, 2, 0, 0, w, h, &lin);
    let mem = vram.mem[..w * h].to_vec();
    let probe: Vec<u8> = (0..(w * h / 4) as u32).flat_map(|i| i.to_le_bytes()).collect();
    let probe_mem = gs::encode32(&probe, 1, 1);
    let mut canvas = vec![0u8; w * h];
    for d in 0..w * h / 4 {
        let i = u32::from_le_bytes(probe_mem[d * 4..d * 4 + 4].try_into().unwrap()) as usize;
        canvas[i * 4..i * 4 + 4].copy_from_slice(&mem[d * 4..d * 4 + 4]);
    }
    let pal: Vec<u8> = (0..256).flat_map(|k| [k as u8, 0, 0, 0x80]).collect();
    let img = decode(&imgd8(w as u16, h as u16, &canvas, &pal)).expect("decode");
    assert!(img.swizzled);
    for p in [0usize, 1, 127, 128, 4000, w * h - 1] {
        let ix = lin[p] as usize;
        // Swizzled 8-bit palettes are stored with bits 3 and 4 swapped.
        let k = (ix & 0xe7) | (ix & 8) << 1 | (ix & 16) >> 1;
        assert_eq!(img.rgba[p * 4], k as u8, "pixel {p}");
        assert_eq!(img.rgba[p * 4 + 3], 255);
    }
}

#[test]
fn rejects_other_data() {
    assert!(decode(b"IMGD").is_none());
    assert!(decode(&[0; 100]).is_none());
    assert!(split_imgz(b"IMGZ").is_empty());
}
