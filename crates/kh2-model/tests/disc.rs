//! Texture footers and GS regions on the user's disc (skipped without KH2_ISO or local/iso.txt).
//! `cargo test -p kh2-model --test disc -- --nocapture` prints every TEXA / UVSC entry of the
//! tt04 map, its sky and Sora (`P_EX100`), and the tt04 textures whose meshes sample outside the
//! GS region (docs/systems/foundations/render.md "tt04 roofs").

use kh2_model::texanim::{self, TexaPlayer, UvscPlayer};
use kh2_model::{Model, Wrap};

fn open() -> Option<kh2_archive::Archive> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    let names = std::fs::read_to_string("../../local/kh2idx.txt").ok();
    let list: Vec<&str> = names.as_deref().map(|l| l.lines().filter(|l| l.ends_with(".idx")).collect()).unwrap_or_default();
    a.apply_names(if list.is_empty() { vec!["000tt.idx"] } else { list }).ok()?;
    Some(a)
}

fn report(name: &str, m: &Model) {
    let a = &m.anims;
    eprintln!("{name}: {} textures, {} meshes, {} TEXA, {} UVSC", m.textures.len(), m.meshes.len(), a.texa.len(), a.uvsc.len());
    for (i, t) in a.texa.iter().enumerate() {
        let tex = &m.textures[t.texture];
        let def = t.default_anim();
        let frames = def.map(|d| t.frames(d)).unwrap_or_default();
        eprintln!(
            "  texa {i}: texture {} ({}x{}) in place {} rect ({},{}) {}x{} bpp {} sprites {} anims {} default {:?} frames (code, min, max, sprite) {:?}",
            t.texture, tex.width, tex.height, t.in_place, t.x, t.y, t.width, t.height, t.bpp(), t.sprites.len(), t.anim_count(), def,
            frames.iter().map(|f| (f.code(), f.min, f.max, f.sprite)).collect::<Vec<_>>()
        );
    }
    for u in &a.uvsc {
        let users: Vec<_> = m.meshes.iter().filter(|x| x.uv_scroll as u32 == u.index + 1).map(|x| x.texture).collect();
        let spans: Vec<_> = users.iter().map(|&t| (m.textures[t].width, m.textures[t].height, m.textures[t].uv_span)).collect();
        eprintln!("  uvsc {}: speed ({}, {}); meshes on textures {users:?} (size, span) {spans:?}", u.index, u.speed_u, u.speed_v);
    }
}

/// Texel range of a texture's mesh UVs along `axis`.
fn uv_range(m: &Model, tex: usize, axis: usize) -> (f32, f32) {
    let size = if axis == 0 { m.textures[tex].width } else { m.textures[tex].height } as f32;
    m.meshes.iter().filter(|x| x.texture == tex).flat_map(|x| x.uvs.iter()).fold((f32::MAX, f32::MIN), |(a, b), uv| (a.min(uv[axis] * size), b.max(uv[axis] * size)))
}

#[test]
fn tt04_map_has_no_texture_animation_and_samples_region_wrapped_atlases() {
    let Some(mut a) = open() else { return };
    let file = a.read("map/us/tt04.map").expect("tt04 map");
    for e in kh2_bar::parse(&file).expect("bar").iter().filter(|e| e.kind == 7) {
        let foot = texanim::footer_offset(e.data).map(|o| String::from_utf8_lossy(&e.data[o..(o + 4).min(e.data.len())]).into_owned());
        eprintln!("tt04.map {:?} textures: footer starts with {foot:?}", e.tag);
    }
    let map = kh2_model::map_file(&file).expect("map");
    report("tt04 MAP", &map.map);
    let m = &map.map;
    // The map's own textures carry no animation: the footer is only the end marker.
    assert!(m.anims.texa.is_empty() && m.anims.uvsc.is_empty());
    // Many textures are sub-images of a shared GS page: REGION_REPEAT / REGION_CLAMP regions
    // smaller than the texture, with meshes whose UVs run past the region.
    let mut outside = 0;
    for (i, t) in m.textures.iter().enumerate() {
        for (axis, r) in t.region.iter().enumerate() {
            let size = if axis == 0 { t.width } else { t.height };
            assert!(r.len >= 1 && r.start + r.len <= size, "texture {i} axis {axis}: region {r:?} inside {size}");
            let (lo, hi) = uv_range(m, i, axis);
            if r.len < size && (lo < r.start as f32 - 1.0 || hi > (r.start + r.len) as f32 + 1.0) {
                outside += 1;
                eprintln!("texture {i} axis {axis}: {size} texels, region {r:?}, mesh UVs {lo:.0}..{hi:.0}");
            }
        }
    }
    assert!(outside > 10, "region-wrapped textures sampled past their region: {outside}");
    assert!(m.textures.iter().any(|t| t.region[0].wrap == Wrap::Repeat && t.region[0].len < t.width));
    // The sky scrolls: UVSC entries that sky meshes name, on textures that allow a scroll.
    let sky = map.sky.as_ref().expect("sky");
    report("tt04 SK0", sky);
    assert!(!sky.anims.uvsc.is_empty());
    let scrolled: Vec<_> = sky.meshes.iter().filter(|x| x.uv_scroll != 0).collect();
    assert!(!scrolled.is_empty());
    for x in scrolled {
        let u = sky.anims.uvsc.iter().find(|u| u.index + 1 == x.uv_scroll as u32).expect("named UVSC entry");
        let t = &sky.textures[x.texture];
        let span = t.uv_span.expect("scrolling texture wraps by REPEAT");
        // A minute of fields moves the texture.
        let mut p = UvscPlayer::default();
        for _ in 0..3600 {
            p.update(u, 1.0);
        }
        eprintln!("sky mesh on texture {}: after 3600 fields uv offset {:?}", x.texture, p.uv(span, t.width, t.height));
        assert_ne!(p.uv(span, t.width, t.height), [0.0, 0.0]);
    }
}

#[test]
fn sora_eyes_blink_by_texture_animation() {
    let Some(mut a) = open() else { return };
    let file = a.read("obj/P_EX100.mdlx").expect("P_EX100");
    let m = kh2_model::mdlx(&file).expect("mdlx");
    report("P_EX100", &m);
    assert_eq!(m.anims.texa.len(), 2);
    for t in &m.anims.texa {
        let tex = &m.textures[t.texture];
        assert!(t.in_place && t.x + t.width <= tex.width && t.y + t.height <= tex.height);
        assert!(t.sprites.iter().all(|s| s.len() == (t.width * t.height * 4) as usize));
        for k in 0..t.anim_count() as usize {
            let f = t.frames(k);
            assert!(!f.is_empty() && f.last().unwrap().code() >= 1, "animation {k} ends in a jump, stop or blank frame");
            assert!(f.iter().all(|f| f.code() != 0 || (f.sprite as usize) < t.sprites.len()));
        }
    }
    // One TEXA's default animation loops through sprites and a blank of random length (a blink);
    // the other's default shows nothing and stops (the mouth, driven by events).
    let mut r = 1u64;
    let mut rand = move || {
        r = r.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((r >> 32) & 0x7fff_ffff) as u32
    };
    let mut changes = [0usize; 2];
    for (i, t) in m.anims.texa.iter().enumerate() {
        let mut p = TexaPlayer::new(t);
        let mut last = None;
        for _ in 0..3600 {
            p.update(t, 1.0, &mut rand);
            let s = p.sprite(t);
            changes[i] += (s != last) as usize;
            last = s;
        }
    }
    eprintln!("P_EX100 sprite changes in 3600 fields: {changes:?}");
    assert!(changes.iter().any(|&c| c >= 20), "the blink runs: {changes:?}");
    assert!(changes.contains(&0), "the other region stays on its base image");
}

/// WP-F1 item 11 (the keyblade showed its back faces): every mesh of the Kingdom Key and of Sora, with the
/// chain header's blend flag and its triangle winding against the rest pose.
#[test]
fn keyblade_meshes_report_their_blend_flags_and_winding() {
    let Some(mut a) = open() else { return };
    for name in ["obj/W_EX010.mdlx", "obj/P_EX100.mdlx"] {
        let file = a.read(name).expect(name);
        let m = kh2_model::mdlx(&file).expect("mdlx");
        for (i, x) in m.meshes.iter().enumerate() {
            // Winding: the sum over triangles of (normal . (centroid - model centre)), positive when the
            // triangles face outward in the right-handed sense.
            let centre = {
                let n = x.positions.len().max(1) as f32;
                let s = x.positions.iter().fold([0.0f32; 3], |s, p| [s[0] + p[0], s[1] + p[1], s[2] + p[2]]);
                [s[0] / n, s[1] / n, s[2] / n]
            };
            let mut outward = 0.0f32;
            for t in x.indices.chunks_exact(3) {
                let p = |k: usize| x.positions[t[k] as usize];
                let (a, b, c) = (p(0), p(1), p(2));
                let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let n = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let g = [(a[0] + b[0] + c[0]) / 3.0 - centre[0], (a[1] + b[1] + c[1]) / 3.0 - centre[1], (a[2] + b[2] + c[2]) / 3.0 - centre[2]];
                outward += n[0] * g[0] + n[1] * g[1] + n[2] * g[2];
            }
            eprintln!(
                "{name} mesh {i}: texture {} transparent {} uv_scroll {} tris {} outward {:.0}",
                x.texture,
                x.transparent,
                x.uv_scroll,
                x.indices.len() / 3,
                outward
            );
        }
    }
}

/// WP-SK1: which maps carry which sky. The game counts the entries tagged `SK0` / `SK1` (map object
/// +0x6c / +0x70, FUN_00149348) and builds a sky object for a tag only when its count is exactly 1
/// (FUN_0015e6c0). tt00 has only SK1; tt03 and tt24..28 have both (docs/systems/foundations/render.md "Sky placement").
#[test]
fn skies_per_area_sk0_and_sk1() {
    let Some(mut a) = open() else { return };
    let mut report = Vec::new();
    for n in (0..=28).map(|i| format!("tt{i:02}")) {
        let Ok(file) = a.read(&format!("map/us/{n}.map")) else { continue };
        let Ok(e) = kh2_bar::parse(&file) else { continue };
        let count = |tag: &str| e.iter().filter(|x| x.tag == tag && x.kind == 4).count();
        let m = kh2_model::map_file(&file);
        let (s0, s1) = (count("SK0"), count("SK1"));
        let loaded = m.as_ref().map(|m| (m.sky.is_some(), m.sky1.is_some()));
        eprintln!("{n}: SK0 entries {s0}, SK1 entries {s1}; map_file sky {loaded:?}");
        assert_eq!(loaded, Some((s0 > 0, s1 > 0)), "{n}");
        report.push((n, s0, s1));
    }
    let find = |n: &str| report.iter().find(|r| r.0 == n).cloned();
    if let Some((_, s0, s1)) = find("tt00") {
        assert!(s0 == 0 && s1 == 1, "tt00 has only SK1");
    }
    if let Some((_, s0, s1)) = find("tt04") {
        assert!(s0 == 1 && s1 == 0);
    }
}
/// Every `.mdlx` on the disc: classify the texture entry's header, check wrapped entries decode to
/// real textures, and check plain models are unchanged by the wrapper handling (docs/systems/foundations/render.md "Wrapped texture entries").
#[test]
fn every_mdlx_texture_entry_decodes_and_wrapped_ones_split() {
    let Some(mut a) = open() else { return };
    let list = std::fs::read_to_string("../../local/kh2idx.txt").unwrap_or_default();
    let names: Vec<String> = list.lines().map(str::trim).filter(|n| n.ends_with(".mdlx")).map(String::from).collect();
    if names.is_empty() {
        return;
    }
    let (mut plain, mut wrapped, mut other, mut no_tex) = (0, Vec::new(), Vec::new(), 0);
    for n in &names {
        let Ok(file) = a.read(n) else { continue };
        let Ok(entries) = kh2_bar::parse(&file) else { continue };
        let Some(t) = entries.iter().find(|e| e.kind == 7) else {
            no_tex += 1;
            continue;
        };
        let head: Vec<u8> = t.data.iter().copied().take(4).collect();
        if head.is_empty() {
            // F_TT840 and G_OA300: a zero-length texture entry (nothing to decode).
            no_tex += 1;
        } else if head == [0, 0, 0, 0] {
            plain += 1;
            let sets = kh2_model::texture::decode_sets(t.data);
            assert_eq!(sets.len(), 1, "{n}");
            assert_eq!(sets[0].len(), kh2_model::texture::decode(t.data).len(), "{n}");
        } else if head == [0xff; 4] {
            let sets = kh2_model::texture::decode_sets(t.data);
            let sizes: Vec<Vec<(u32, u32)>> = sets.iter().map(|s| s.iter().map(|x| (x.width, x.height)).collect()).collect();
            eprintln!("wrapped {n}: {} sets, sizes {sizes:?}, sets equal {}", sets.len(), sets.windows(2).all(|w| w[0].iter().zip(&w[1]).all(|(x, y)| x.rgba == y.rgba)));
            assert!(!sets.is_empty() && sets.iter().all(|s| !s.is_empty() && s.iter().all(|x| x.width > 1 && x.height > 1)), "{n}");
            wrapped.push(n.clone());
        } else {
            eprintln!("other header {n}: {head:02x?}");
            other.push(n.clone());
        }
    }
    eprintln!("mdlx files {}: plain {plain}, wrapped {}, other {}, without texture entry {no_tex}", names.len(), wrapped.len(), other.len());
    assert!(other.is_empty(), "unknown texture headers: {other:?}");
    assert!(wrapped.iter().any(|n| n.ends_with("M_EX500.mdlx")));
}

#[test]
fn m_ex500_draws_with_its_first_set_and_other_models_keep_one_set() {
    let Some(mut a) = open() else { return };
    let file = a.read("obj/M_EX500.mdlx").expect("M_EX500");
    let m = kh2_model::mdlx(&file).expect("mdlx");
    assert_eq!(m.texture_sets.len(), 2);
    assert_eq!(m.textures.len(), 3, "set 1 has 3 textures");
    assert!(m.textures.iter().all(|t| t.width > 1 && t.height > 1));
    eprintln!("M_EX500 set sizes {:?}", m.texture_sets.iter().map(|s| s.iter().map(|t| (t.width, t.height)).collect::<Vec<_>>()).collect::<Vec<_>>());
    assert_eq!(m.texture_sets[1].len(), 3);
    for n in ["M_EX020", "M_EX570", "M_EX740", "M_EX600", "P_EX100"] {
        let m = kh2_model::mdlx(&a.read(&format!("obj/{n}.mdlx")).expect(n)).expect(n);
        assert_eq!(m.texture_sets.len(), 1, "{n}");
        assert_eq!(m.texture_sets[0].len(), m.textures.len(), "{n}");
        assert!(m.textures.iter().all(|t| t.width > 1 && t.height > 1), "{n}");
    }
}
