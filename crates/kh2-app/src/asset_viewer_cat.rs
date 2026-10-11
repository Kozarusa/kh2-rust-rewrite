//! The asset viewer's catalogue and cursor (WP-G10): which files of the disc it lists under each
//! kind, and how the keys move through them. Pure data, no Bevy: `asset_viewer.rs` loads and shows.

use bevy::math::Vec3;

/// What the viewer is browsing; Tab steps through them in this order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// Skeletal models (`obj/*.mdlx`) and map models (`*.map`: the `MAP` and `SK0` models).
    #[default]
    Model,
    /// The motions of the shown model's motion set (`.mset` slots).
    Motion,
    /// Images: the shown model's textures, the current effect file's textures, 2D image files.
    Texture,
    /// Effect elements of an effect file (`00effect.bar`, object `.apdx`, `.mag`, map PAX, event PAX).
    Effect,
    /// Sound banks and ids (the sound viewer's catalogue, `sound_test::Catalog`).
    Audio,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Model, Kind::Motion, Kind::Texture, Kind::Effect, Kind::Audio];

    /// The word of the log lines (`asset <kind> ...`) and of `KH2_ASSETS_KIND`.
    pub fn word(self) -> &'static str {
        match self {
            Kind::Model => "model",
            Kind::Motion => "motion",
            Kind::Texture => "texture",
            Kind::Effect => "effect",
            Kind::Audio => "audio",
        }
    }

    pub fn from_word(w: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.word() == w.trim().to_ascii_lowercase())
    }

    pub fn step(self, delta: isize) -> Kind {
        let i = Kind::ALL.iter().position(|&k| k == self).unwrap_or(0);
        Kind::ALL[wrap(Kind::ALL.len(), i, delta)]
    }
}

/// `from + delta` wrapped into `0..len` (0 for an empty list).
pub fn wrap(len: usize, from: usize, delta: isize) -> usize {
    crate::sound_test::step_bank(len, from.min(len.saturating_sub(1)), delta)
}

/// A run of model files listed together (Up / Down in the model kind).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// `obj/P` (every `obj/P*.mdlx`), `map/us`, ...
    pub name: String,
    pub files: Vec<String>,
}

/// Every file the viewer lists, by kind. Motions and textures belong to the shown model and audio
/// to the sound viewer's catalogue, so they are not listed here.
#[derive(Clone, Debug, Default)]
pub struct Catalogue {
    pub models: Vec<Group>,
    pub effects: Vec<String>,
    pub images: Vec<String>,
    /// Motion-set files on the disc (counted for `KH2_ASSETS_LIST`).
    pub msets: usize,
}

/// The model group of a file name, or `None` when it is not a model.
pub fn model_group(name: &str) -> Option<String> {
    let (dir, file) = name.rsplit_once('/').unwrap_or(("", name));
    if name.ends_with(".mdlx") {
        // Objects by their first letter (P player and party, W weapons, N, M, B, F, G, H, ...).
        let first = file.chars().next()?;
        return Some(if dir.is_empty() { format!("{first}") } else { format!("{dir}/{first}") });
    }
    name.ends_with(".map").then(|| dir.to_string())
}

/// An effect file: a BAR with a PAX entry (type 18) or a bare PAX.
pub fn is_effect_file(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name);
    name.ends_with(".apdx") || name.ends_with(".mag") || name.ends_with(".pax") || (file.ends_with(".bar") && file.contains("effect")) || (name.starts_with("map/") && name.ends_with(".map"))
}

/// A 2D image file: a BAR of IMGD entries (`.2dd`, `.2ld`), an IMGD (`.imd`) or an IMGZ (`.imz`).
pub fn is_image_file(name: &str) -> bool {
    [".2dd", ".2ld", ".imd", ".imz"].iter().any(|x| name.ends_with(x))
}

/// Display rank of a model group: objects in the order P, W, N, M, B, then other objects, then maps.
fn group_rank(g: &str) -> (u8, String) {
    let rank = match g.strip_prefix("obj/") {
        Some(l) => "PWNMB".find(l).map_or(5, |i| i as u8),
        None => 10,
    };
    (rank, g.to_string())
}

impl Catalogue {
    /// Builds the catalogue from a name list, keeping the names `exists` says are on the disc.
    /// The effect list starts with the system file `00effect.bar`.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>, exists: impl Fn(&str) -> bool) -> Catalogue {
        let mut names: Vec<&str> = names.into_iter().map(str::trim).filter(|n| !n.is_empty() && !n.starts_with('#')).collect();
        names.sort_unstable();
        names.dedup();
        let mut groups: std::collections::BTreeMap<(u8, String), Vec<String>> = Default::default();
        let mut cat = Catalogue::default();
        for n in names {
            let model = model_group(n);
            let effect = is_effect_file(n);
            let image = is_image_file(n);
            let mset = n.ends_with(".mset");
            if model.is_none() && !effect && !image && !mset {
                continue;
            }
            if !exists(n) {
                continue;
            }
            if let Some(g) = model {
                groups.entry(group_rank(&g)).or_default().push(n.to_string());
            }
            if effect {
                cat.effects.push(n.to_string());
            }
            if image {
                cat.images.push(n.to_string());
            }
            cat.msets += mset as usize;
        }
        if let Some(i) = cat.effects.iter().position(|e| e == "00effect.bar") {
            let sys = cat.effects.remove(i);
            cat.effects.insert(0, sys);
        }
        cat.models = groups.into_iter().map(|((_, name), files)| Group { name, files }).collect();
        cat
    }

    pub fn model_count(&self) -> usize {
        self.models.iter().map(|g| g.files.len()).sum()
    }

    /// Group and index of a model file.
    pub fn find_model(&self, file: &str) -> Option<(usize, usize)> {
        self.models.iter().enumerate().find_map(|(g, grp)| grp.files.iter().position(|f| f == file).map(|i| (g, i)))
    }
}

/// One frame's key input, already turned into steps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    /// Left / Right (1) and PageUp / PageDown (10).
    pub item: isize,
    /// Up / Down.
    pub group: isize,
    /// Tab (Shift+Tab backwards).
    pub kind: isize,
    /// Enter.
    pub play: bool,
}

/// How many entries each list has right now.
#[derive(Clone, Debug, Default)]
pub struct Sizes {
    /// Files per model group.
    pub groups: Vec<usize>,
    /// Motions of the shown model.
    pub motions: usize,
    /// Texture sources: the shown model, the current effect file, then every image file.
    pub texture_sources: usize,
    /// Images of the current texture source.
    pub textures: usize,
    pub effect_files: usize,
    /// Element ids of the current effect file.
    pub effects: usize,
}

/// Where the viewer stands in every kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Nav {
    pub kind: Kind,
    pub group: usize,
    pub model: usize,
    pub motion: usize,
    pub tex_source: usize,
    pub texture: usize,
    pub effect_file: usize,
    pub effect: usize,
}

/// What an input changed (the caller reloads and logs those).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changed {
    pub kind: bool,
    pub model: bool,
    pub motion: bool,
    pub texture_source: bool,
    pub texture: bool,
    pub effect_file: bool,
    pub effect: bool,
    /// Steps for the sound catalogue (the audio kind's ids and banks).
    pub audio_item: isize,
    pub audio_bank: isize,
    pub play: bool,
}

impl Changed {
    /// Adds `o` to what is still to be done (several inputs before the next update).
    pub fn merge(&mut self, o: Changed) {
        self.kind |= o.kind;
        self.model |= o.model;
        self.motion |= o.motion;
        self.texture_source |= o.texture_source;
        self.texture |= o.texture;
        self.effect_file |= o.effect_file;
        self.effect |= o.effect;
        self.audio_item += o.audio_item;
        self.audio_bank += o.audio_bank;
        self.play |= o.play;
    }
}

impl Nav {
    /// Applies one frame's input. Model: Up / Down group, Left / Right model. Motion: Left / Right
    /// motion, Up / Down the model. Texture: Left / Right image, Up / Down source. Effect: Left /
    /// Right element id, Up / Down file. Audio: Left / Right id, Up / Down bank. Every list wraps.
    pub fn apply(&mut self, i: Input, s: &Sizes) -> Changed {
        let mut c = Changed { play: i.play, ..Default::default() };
        if i.kind != 0 {
            self.kind = self.kind.step(i.kind);
            c.kind = true;
        }
        let in_group = |g: usize| s.groups.get(g).copied().unwrap_or(0);
        let step_model = |n: &mut Nav, d: isize, c: &mut Changed| {
            if d != 0 && in_group(n.group) > 0 {
                n.model = wrap(in_group(n.group), n.model, d);
                n.model_changed(c);
            }
        };
        match self.kind {
            Kind::Model => {
                if i.group != 0 && !s.groups.is_empty() {
                    self.group = wrap(s.groups.len(), self.group, i.group);
                    self.model = 0;
                    self.model_changed(&mut c);
                }
                step_model(self, i.item, &mut c);
            }
            Kind::Motion => {
                step_model(self, i.group, &mut c);
                if i.item != 0 && s.motions > 0 && !c.model {
                    self.motion = wrap(s.motions, self.motion, i.item);
                    c.motion = true;
                }
            }
            Kind::Texture => {
                if i.group != 0 && s.texture_sources > 0 {
                    self.tex_source = wrap(s.texture_sources, self.tex_source, i.group);
                    self.texture = 0;
                    c.texture_source = true;
                    c.texture = true;
                }
                if i.item != 0 && s.textures > 0 && !c.texture_source {
                    self.texture = wrap(s.textures, self.texture, i.item);
                    c.texture = true;
                }
            }
            Kind::Effect => {
                if i.group != 0 && s.effect_files > 0 {
                    self.effect_file = wrap(s.effect_files, self.effect_file, i.group);
                    self.effect = 0;
                    c.effect_file = true;
                    c.effect = true;
                }
                if i.item != 0 && s.effects > 0 && !c.effect_file {
                    self.effect = wrap(s.effects, self.effect, i.item);
                    c.effect = true;
                }
            }
            Kind::Audio => {
                c.audio_item = i.item;
                c.audio_bank = i.group;
            }
        }
        c
    }

    /// A new model: its motion list and (when the model is the texture source) its images start over.
    fn model_changed(&mut self, c: &mut Changed) {
        c.model = true;
        self.motion = 0;
        c.motion = true;
        if self.tex_source == 0 {
            self.texture = 0;
            c.texture = true;
        }
    }
}

/// Centre and radius for framing a set of points: the per-axis median as the centre and the 90th
/// percentile of the distances from it as the radius, so a few far-off vertices (a map's distant
/// backdrop pieces) do not push the camera out of reach. `None` for no points.
pub fn frame_points(points: &[Vec3]) -> Option<(Vec3, f32)> {
    if points.is_empty() {
        return None;
    }
    let median = |f: fn(&Vec3) -> f32| {
        let mut v: Vec<f32> = points.iter().map(f).collect();
        v.sort_unstable_by(f32::total_cmp);
        v[v.len() / 2]
    };
    let c = Vec3::new(median(|p| p.x), median(|p| p.y), median(|p| p.z));
    let mut d: Vec<f32> = points.iter().map(|p| p.distance(c)).collect();
    d.sort_unstable_by(f32::total_cmp);
    let r = d[((d.len() - 1) * 9) / 10];
    Some((c, r))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "obj/P_EX100.mdlx",
        "obj/P_EX100.mset",
        "obj/P_EX100.apdx",
        "obj/W_EX010.mdlx",
        "obj/M_EX010.mdlx",
        "obj/F_TT010.mdlx",
        "obj/B_EX100.mdlx",
        "obj/N_TT010_BTL.mdlx",
        "map/us/tt04.map",
        "map/jp/tt04.map",
        "00effect.bar",
        "magic/FIRE_1.mag",
        "event/effect/tt_event_101.pax",
        "menu/us/camp.2ld",
        "itempic/item-011.imd",
        "voice/us/battle/tt0_sora.vsb",
        "",
        "# comment",
    ];

    /// Every model groups under its folder and first letter, in the order P, W, N, M, B, other
    /// objects, maps; names missing from the disc are left out; 00effect.bar leads the effects.
    #[test]
    fn assets_catalogue_groups_and_filters_names() {
        let cat = Catalogue::from_names(NAMES.iter().copied(), |n| !n.starts_with("map/jp"));
        let groups: Vec<&str> = cat.models.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(groups, ["obj/P", "obj/W", "obj/N", "obj/M", "obj/B", "obj/F", "map/us"]);
        assert_eq!(cat.model_count(), 7, "map/jp/tt04.map is not on the disc");
        assert_eq!(cat.effects, ["00effect.bar", "event/effect/tt_event_101.pax", "magic/FIRE_1.mag", "map/us/tt04.map", "obj/P_EX100.apdx"]);
        assert_eq!(cat.images, ["itempic/item-011.imd", "menu/us/camp.2ld"]);
        assert_eq!(cat.msets, 1);
        assert_eq!(cat.find_model("map/us/tt04.map"), Some((6, 0)));
        assert_eq!(cat.find_model("obj/W_EX010.mdlx"), Some((1, 0)));
        assert_eq!(cat.find_model("obj/X.mdlx"), None);
        assert_eq!(model_group("obj/P_EX100.mdlx").as_deref(), Some("obj/P"));
        assert_eq!(model_group("gumibattle/gm00.map").as_deref(), Some("gumibattle"));
        assert_eq!(model_group("obj/P_EX100.mset"), None);
    }

    fn sizes() -> Sizes {
        Sizes { groups: vec![3, 1, 12], motions: 5, texture_sources: 4, textures: 2, effect_files: 3, effects: 25 }
    }

    /// Model kind: Left / Right step the model within its group (wrapping), Up / Down the group
    /// (back to its first model); a new model restarts the motion list and the model's images.
    #[test]
    fn assets_nav_steps_models_and_groups() {
        let s = sizes();
        let mut n = Nav::default();
        let c = n.apply(Input { item: 1, ..default() }, &s);
        assert!(c.model && c.motion && c.texture);
        assert_eq!((n.group, n.model), (0, 1));
        n.apply(Input { item: 10, ..default() }, &s);
        assert_eq!(n.model, 2, "11 mod 3");
        n.apply(Input { item: 1, ..default() }, &s);
        assert_eq!(n.model, 0, "wraps to the first");
        n.apply(Input { group: -1, ..default() }, &s);
        assert_eq!((n.group, n.model), (2, 0), "wraps to the last group");
        n.apply(Input { item: -10, ..default() }, &s);
        assert_eq!(n.model, 2, "-10 mod 12");
        let none = n.apply(Input::default(), &s);
        assert_eq!(none, Changed::default());
    }

    /// Tab walks the kinds and wraps; in the motion kind Left / Right step the motion and Up / Down
    /// the model; in the texture kind Up / Down change the source; in the effect kind Up / Down the
    /// file; the audio kind hands its steps to the sound catalogue.
    #[test]
    fn assets_nav_kinds_and_sub_items() {
        let s = sizes();
        let mut n = Nav::default();
        assert!(n.apply(Input { kind: 1, ..default() }, &s).kind);
        assert_eq!(n.kind, Kind::Motion);
        let c = n.apply(Input { item: -1, ..default() }, &s);
        assert!(c.motion && !c.model);
        assert_eq!(n.motion, 4);
        let c = n.apply(Input { group: 1, ..default() }, &s);
        assert!(c.model);
        assert_eq!((n.model, n.motion), (1, 0), "next model, its first motion");
        n.apply(Input { kind: 1, ..default() }, &s);
        assert_eq!(n.kind, Kind::Texture);
        n.apply(Input { item: 1, ..default() }, &s);
        assert_eq!(n.texture, 1);
        let c = n.apply(Input { group: 1, ..default() }, &s);
        assert!(c.texture_source && c.texture);
        assert_eq!((n.tex_source, n.texture), (1, 0));
        n.apply(Input { kind: 1, ..default() }, &s);
        n.apply(Input { item: 10, ..default() }, &s);
        assert_eq!((n.kind, n.effect), (Kind::Effect, 10));
        let c = n.apply(Input { group: -1, ..default() }, &s);
        assert!(c.effect_file && c.effect);
        assert_eq!((n.effect_file, n.effect), (2, 0));
        n.apply(Input { kind: 1, ..default() }, &s);
        let c = n.apply(Input { item: 10, group: -1, play: true, kind: 0 }, &s);
        assert_eq!((c.audio_item, c.audio_bank, c.play), (10, -1, true));
        n.apply(Input { kind: 1, ..default() }, &s);
        assert_eq!(n.kind, Kind::Model, "Tab wraps");
        n.apply(Input { kind: -1, ..default() }, &s);
        assert_eq!(n.kind, Kind::Audio, "Shift+Tab wraps backwards");
        assert_eq!(Kind::from_word("Effect"), Some(Kind::Effect));
        assert_eq!(Kind::from_word("x"), None);
        // An empty list never moves.
        let mut e = Nav { kind: Kind::Effect, ..default() };
        assert_eq!(e.apply(Input { item: 1, group: 1, ..default() }, &Sizes::default()), Changed::default());
    }

    /// Framing: the median centre and the 90th-percentile radius ignore a lone far vertex.
    #[test]
    fn assets_frame_points_ignores_outliers() {
        assert_eq!(frame_points(&[]), None);
        let mut pts: Vec<Vec3> = (0..=20).map(|i| Vec3::new(i as f32 - 10.0, 0.0, 0.0)).collect();
        pts.push(Vec3::new(100_000.0, 0.0, 0.0));
        let (c, r) = frame_points(&pts).unwrap();
        assert!(c.x.abs() <= 1.0 && c.y == 0.0, "{c:?}");
        assert!((8.0..=11.0).contains(&r), "{r}");
    }

    fn default<T: Default>() -> T {
        T::default()
    }
}
