//! Visual audit of the enemy and party models on the user's disc (WP-VIS1; skipped without KH2_ISO).
//! `cargo test -p kh2-model --test enemy_models -- --nocapture` prints, per model, the meshes, the texture sets and the
//! textures each mesh draws; the asserts catch the defects that hid for a long time because checks were log based
//! (WP-MD1: a mis-parsed texture entry drew M_EX500 as a flat silhouette): a mesh naming a texture that does not exist,
//! indices past the vertex list, a drawn texture with (almost) no colour variation, NaN positions.

use kh2_model::Model;

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

/// The model files of `enemy::KINDS` (their `Kind::model`), the Sora forms, Donald, Goofy and the party extras.
const MODELS: [&str; 58] = [
    "M_EX020", "M_EX010", "M_EX990", "N_EX570_BTL", "M_EX930", "N_EX500_BTL", "N_EX600_BTL", "M_EX940", "M_EX850", "M_EX760", "M_EX420", "M_EX880",
    "M_EX700", "M_EX950", "M_EX900", "M_EX650", "M_EX110", "M_EX660", "M_EX520", "M_EX770", "M_EX210", "M_EX910", "M_EX690", "M_EX050", "M_EX730",
    "M_EX750", "M_EX510", "M_EX560", "M_EX060", "M_EX600", "M_EX620", "M_EX920", "M_EX120", "M_EX630", "M_EX870", "M_EX960", "M_AL020_FIRE",
    "M_AL020_ICEE", "M_EX130", "M_EX890", "M_EX020_RAW", "M_EX680", "M_EX580", "M_EX780", "M_EX570", "M_EX740", "M_EX500", "M_EX710", "P_EX100",
    "P_EX100_BTLF", "P_EX100_TR", "P_EX100_WI", "P_EX100_NM", "P_EX020", "P_EX030", "P_EX200", "P_EX350", "W_EX010",
];

/// Share of the texture's pixels that are not the most common colour, and the number of distinct 5-bit colours.
fn variation(t: &kh2_model::Texture) -> (f32, usize) {
    let mut seen = std::collections::HashMap::new();
    for p in t.rgba.chunks_exact(4).filter(|p| p[3] > 0) {
        *seen.entry([p[0] >> 3, p[1] >> 3, p[2] >> 3]).or_insert(0usize) += 1;
    }
    let total: usize = seen.values().sum();
    let top = seen.values().copied().max().unwrap_or(0);
    (if total == 0 { 0.0 } else { 1.0 - top as f32 / total as f32 }, seen.len())
}

fn audit(name: &str, m: &Model) -> Vec<String> {
    let mut bad = Vec::new();
    let sets: Vec<usize> = m.texture_sets.iter().map(|s| s.len()).collect();
    let used: std::collections::BTreeSet<usize> = m.meshes.iter().map(|x| x.texture).collect();
    eprintln!("{name}: {} meshes, {} triangles, {} bones, texture sets {sets:?}, textures drawn {used:?}", m.meshes.len(), m.meshes.iter().map(|x| x.indices.len() / 3).sum::<usize>(), m.bones.len());
    for (i, x) in m.meshes.iter().enumerate() {
        if !m.textures.is_empty() && x.texture >= m.textures.len() {
            bad.push(format!("{name}: mesh {i} draws texture {} of {}", x.texture, m.textures.len()));
        }
        if x.indices.iter().any(|&k| k as usize >= x.positions.len()) {
            bad.push(format!("{name}: mesh {i} has an index past its {} vertices", x.positions.len()));
        }
        if x.positions.iter().flatten().any(|v| !v.is_finite()) {
            bad.push(format!("{name}: mesh {i} has a non-finite position"));
        }
        if x.uvs.len() != x.positions.len() || x.colors.len() != x.positions.len() {
            bad.push(format!("{name}: mesh {i} attribute lengths differ"));
        }
    }
    for &t in &used {
        let Some(tex) = m.textures.get(t) else { continue };
        let (v, n) = variation(tex);
        eprintln!("  texture {t}: {}x{}, {n} colours, {:.0}% not the main colour", tex.width, tex.height, v * 100.0);
        if v < 0.05 {
            bad.push(format!("{name}: drawn texture {t} is nearly flat ({n} colours)"));
        }
    }
    bad
}

#[test]
fn enemy_and_party_models_decode_with_sane_meshes_and_textures() {
    let Some(mut a) = open() else {
        eprintln!("skipped: no disc (set KH2_ISO)");
        return;
    };
    let mut bad = Vec::new();
    let mut seen = 0;
    for name in MODELS {
        let file = format!("obj/{name}.mdlx");
        let Ok(data) = a.read(&file) else {
            bad.push(format!("{name}: file missing"));
            continue;
        };
        let Some(m) = kh2_model::mdlx(&data) else {
            bad.push(format!("{name}: does not parse"));
            continue;
        };
        seen += 1;
        bad.extend(audit(name, &m));
    }
    assert_eq!(seen, MODELS.len());
    assert!(bad.is_empty(), "{bad:#?}");
}

/// M_EX870 is the invisible half of the eh17 fighter pair: four triangles, no texture (docs/enemies.md, WP-EN11b).
#[test]
fn m_ex870_is_the_known_untextured_placeholder() {
    let Some(mut a) = open() else { return };
    let m = kh2_model::mdlx(&a.read("obj/M_EX870.mdlx").expect("file")).expect("parses");
    assert!(m.textures.is_empty());
    assert_eq!(m.meshes.iter().map(|x| x.indices.len() / 3).sum::<usize>(), 4);
}
