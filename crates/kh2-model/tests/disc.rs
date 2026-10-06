//! Texture footers and GS regions on the user's disc (skipped without KH2_ISO or local/iso.txt).
//! `cargo test -p kh2-model --test disc -- --nocapture` prints every TEXA / UVSC entry of the
//! tt04 map, its sky and Sora (`P_EX100`), and the tt04 textures whose meshes sample outside the
//! GS region (docs/render.md "tt04 roofs").

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
