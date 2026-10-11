//! Developer asset viewer (WP-G10; the app's own tool, not in the original game): steps through the
//! models, motions, textures, effects and sounds the app can read from the disc and shows each one
//! with its identifiers on screen, so ids can be pasted into reports. It draws with the game's own
//! paths: models through `skel::spawn_skinned` / `assets::spawn_model`, motions through the
//! `Animator`, effects through `fx::start_placed`, sounds through the sound driver (`audio::play_se`,
//! the voice bank cache, `audio::play_bgm`); there is no second renderer.
//!
//! Open and close with F11 (`KH2_ASSETS=1` starts open). Tab / Shift+Tab change the kind; Left /
//! Right step the item (Shift: 100), PageUp / PageDown step 10, Up / Down the group or sub-item
//! (Shift: 10), Enter plays (restarts the motion or the effect, plays the sound), R turns the
//! automatic orbit on and off, right mouse drag orbits and the wheel zooms. The game's keys are
//! swallowed while it is open, as in the sound viewer (F10, which stays separate).
//!
//! The shown model and effects stand on a stage 3 km above the area's origin, out of reach of the
//! area's own drawing (the camera's far plane is 1 km), and the camera orbits it while the viewer is
//! open. Every selection prints one `asset <kind> <file> <id> ...` log line; `KH2_ASSETS_LIST=1`
//! prints the catalogue's counts at start. `KH2_ASSETS_KIND=<model|motion|texture|effect|audio>`
//! and `KH2_ASSETS_START=<file>` (a model or effect file) choose where it opens.

#[path = "asset_viewer_cat.rs"]
pub mod cat;

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use kh2_audio::positional::{CENTRE_PAN, FULL_VOLUME};
use kh2_fx::pax::Dpd;

use crate::assets::Disc;
use crate::audio::{Audio, PlaySe, SeCache, VoiceInfo};
use crate::fx::{self, Bind, EffectLib, LoadedPax, Placement, RunningEffect};
use crate::skel::{Animator, Rig};
use crate::sound_test::{self, SoundTest};
use cat::{Catalogue, Changed, Input, Kind, Nav, Sizes};

pub struct AssetViewerPlugin;

impl Plugin for AssetViewerPlugin {
    fn build(&self, app: &mut App) {
        let open = std::env::var("KH2_ASSETS").is_ok_and(|v| v != "0");
        let mut viewer = AssetViewer { open, orbit: Orbit::default(), ..default() };
        if let Some(k) = std::env::var("KH2_ASSETS_KIND").ok().and_then(|w| Kind::from_word(&w)) {
            viewer.nav.kind = k;
        }
        app.insert_resource(viewer)
            .add_systems(Startup, setup)
            .add_systems(PostStartup, list_catalogue)
            // Keys as the sound viewer takes them: after the harness presses its scripted ones, before the
            // pause menu reads Enter.
            .add_systems(PreUpdate, keys.after(bevy::input::InputSystems).after(crate::harness::Keys).before(crate::menu::input))
            .add_systems(Update, hold_pad.after(crate::sora::read_pad).in_set(crate::schedule::Frame::Pad))
            .add_systems(Update, (play_audio, apply, show).chain().in_set(crate::schedule::Frame::Hud))
            .add_systems(PostUpdate, camera.before(bevy::transform::TransformSystems::Propagate));
    }
}

/// The stage the viewer shows things on (metres, Bevy space).
const STAGE: Vec3 = Vec3::new(0.0, 3000.0, 0.0);
/// Where effects start: 1 m above the stage.
const EFFECT_AT: Vec3 = Vec3::new(0.0, 3001.0, 0.0);
/// Every change at once: the first open, and every open after a close.
const EVERYTHING: Changed = Changed { kind: true, model: true, motion: true, texture_source: true, texture: true, effect_file: true, effect: true, audio_item: 0, audio_bank: 0, play: false };
/// Id base for effect files the viewer loads itself (the effect texture cache keys on it).
const FX_ID_BASE: usize = 20_000;

/// One image the texture kind can show.
#[derive(Clone)]
struct Img {
    label: String,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    info: String,
}

/// The model on the stage.
#[derive(Default)]
struct Shown {
    file: String,
    name: String,
    mset: Option<String>,
    body: Option<Entity>,
    rig: Option<Arc<Rig>>,
    meshes: usize,
    bones: usize,
    textures: Vec<Img>,
    /// Motion slots that hold a motion.
    motions: Vec<usize>,
    centre: Vec3,
    radius: f32,
    objentry: String,
}

/// The automatic and mouse orbit of the viewer's camera.
struct Orbit {
    yaw: f32,
    pitch: f32,
    zoom: f32,
    auto: bool,
}

impl Default for Orbit {
    fn default() -> Orbit {
        Orbit { yaw: 0.6, pitch: 0.35, zoom: 1.0, auto: true }
    }
}

/// What the viewer started from the sound driver.
#[derive(Clone, Copy)]
enum Playing {
    Se(kh2_audio::ssl::Handle),
    Voice(kh2_audio::ssl::Handle),
    Bgm,
}

#[derive(Resource, Default)]
pub struct AssetViewer {
    pub open: bool,
    cat: Option<Catalogue>,
    /// `00objentry` rows: (object id, model name, motion-set name).
    objentry: Vec<(u32, String, String)>,
    nav: Nav,
    /// Changes not applied yet.
    pending: Changed,
    stage: Option<Entity>,
    shown: Option<Shown>,
    fx_file: Option<(String, Option<Arc<LoadedPax>>)>,
    fx_ids: Vec<u16>,
    fx_running: Vec<Entity>,
    fx_serial: usize,
    fx_restart: Option<f32>,
    images: Vec<Img>,
    image: Option<(Handle<Image>, u32, u32)>,
    sounds: Option<sound_test::Catalog>,
    sound: sound_test::Cursor,
    playing: Option<Playing>,
    orbit: Orbit,
    /// The current selection's line per kind (the text of its log line).
    lines: [String; 5],
}

impl AssetViewer {
    fn sizes(&self) -> Sizes {
        Sizes {
            groups: self.cat.as_ref().map_or(Vec::new(), |c| c.models.iter().map(|g| g.files.len()).collect()),
            motions: self.shown.as_ref().map_or(0, |s| s.motions.len()),
            texture_sources: 2 + self.cat.as_ref().map_or(0, |c| c.images.len()),
            textures: self.images.len(),
            effect_files: self.cat.as_ref().map_or(0, |c| c.effects.len()),
            effects: self.fx_ids.len(),
        }
    }

    fn model_file(&self) -> Option<String> {
        self.cat.as_ref()?.models.get(self.nav.group)?.files.get(self.nav.model).cloned()
    }

    fn effect_file(&self) -> Option<String> {
        self.cat.as_ref()?.effects.get(self.nav.effect_file).cloned()
    }

    fn line(&mut self, kind: Kind, text: String) {
        info!("asset {} {text}", kind.word());
        self.lines[Kind::ALL.iter().position(|&k| k == kind).unwrap_or(0)] = text;
    }
}

/// Builds the catalogue from the name list the app already reads for the world indexes
/// (`local/kh2idx.txt`), keeping the names the disc has. Without the list only the files the app
/// itself uses are listed.
fn load_catalogue(disc: &Disc) -> Catalogue {
    let list = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/kh2idx.txt");
    let text = std::fs::read_to_string(&list).unwrap_or_default();
    let mut fallback: Vec<String> = ["obj/P_EX100.mdlx", "obj/W_EX010.mdlx", "00effect.bar", "obj/P_EX100.apdx", "obj/W_EX010.apdx"].map(String::from).to_vec();
    for k in crate::enemy::KINDS {
        fallback.push(format!("obj/{}.mdlx", k.model));
        fallback.push(format!("obj/{}.apdx", k.model));
    }
    fallback.extend((0..10).map(|i| format!("map/us/tt{i:02}.map")));
    let Ok(a) = disc.0.lock() else { return Catalogue::default() };
    let keys: std::collections::HashSet<(u32, u16)> = a.entries().iter().map(|l| l.entry.key()).collect();
    let exists = |n: &str| keys.contains(&(kh2_archive::hash32(n.as_bytes()), kh2_archive::hash16(n.as_bytes())));
    if text.is_empty() {
        warn!("assets: no name list at {}; listing only the files the app uses", list.display());
        Catalogue::from_names(fallback.iter().map(String::as_str), exists)
    } else {
        Catalogue::from_names(text.lines(), exists)
    }
}

fn load_objentry(disc: &Disc) -> Vec<(u32, String, String)> {
    use kh2_tables::tables as t;
    let Ok(mut a) = disc.0.lock() else { return Vec::new() };
    kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY)
        .map(|tb| tb.rows.into_iter().map(|r| (r.object_id, r.model_name, r.animation_name)).collect())
        .unwrap_or_default()
}

#[derive(Component)]
struct ViewerText;

#[derive(Component)]
struct ViewerImage;

fn setup(mut commands: Commands) {
    commands.spawn((
        ViewerText,
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() },
        TextColor(Color::srgb(0.7, 1.0, 0.8)),
        Node { position_type: PositionType::Absolute, right: Val::Px(12.0), top: Val::Px(110.0), max_width: Val::Px(760.0), ..default() },
        Visibility::Hidden,
    ));
    commands.spawn((
        ViewerImage,
        ImageNode::default(),
        BackgroundColor(Color::srgb(0.25, 0.25, 0.3)),
        Node { position_type: PositionType::Absolute, left: Val::Px(12.0), bottom: Val::Px(12.0), ..default() },
        Visibility::Hidden,
    ));
}

/// `KH2_ASSETS_LIST=1`: the catalogue's counts per kind and per model group, at start.
fn list_catalogue(disc: Res<Disc>, info: Res<VoiceInfo>) {
    if !std::env::var("KH2_ASSETS_LIST").is_ok_and(|v| v != "0") {
        return;
    }
    let cat = load_catalogue(&disc);
    let sounds = sound_test::Catalog::load(&disc, &info.lang, &info.names.0);
    let ids: usize = sounds.banks.iter().map(|b| b.entries.iter().filter(|e| e.filled).count()).sum();
    info!(
        "assets list: models {} in {} groups, motion sets {}, effect files {}, image files {}, sound banks {} with {ids} ids",
        cat.model_count(),
        cat.models.len(),
        cat.msets,
        cat.effects.len(),
        cat.images.len(),
        sounds.banks.len()
    );
    for g in &cat.models {
        info!("assets list model group {}: {} files ({} .. {})", g.name, g.files.len(), g.files.first().map_or("", |s| s.as_str()), g.files.last().map_or("", |s| s.as_str()));
    }
    let by_ext = |list: &[String]| {
        let mut m: std::collections::BTreeMap<String, usize> = Default::default();
        for f in list {
            *m.entry(f.rsplit('.').next().unwrap_or("").to_string()).or_default() += 1;
        }
        m.into_iter().map(|(k, n)| format!(".{k} {n}")).collect::<Vec<_>>().join(", ")
    };
    info!("assets list effect files: {}", by_ext(&cat.effects));
    info!("assets list image files: {}", by_ext(&cat.images));
}

/// Keys the viewer takes while open (and swallows), with the game's movement and area keys.
const TAKEN: [KeyCode; 36] = [
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::PageUp,
    KeyCode::PageDown,
    KeyCode::Enter,
    KeyCode::NumpadEnter,
    KeyCode::Space,
    KeyCode::Backspace,
    KeyCode::Tab,
    KeyCode::KeyR,
    KeyCode::KeyW,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::KeyJ,
    KeyCode::KeyK,
    KeyCode::KeyE,
    KeyCode::KeyQ,
    KeyCode::KeyC,
    KeyCode::KeyV,
    KeyCode::BracketLeft,
    KeyCode::BracketRight,
    KeyCode::ControlLeft,
    KeyCode::F1,
    KeyCode::Digit0,
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];

fn keys(mut keys: ResMut<ButtonInput<KeyCode>>, mut v: ResMut<AssetViewer>, st: Res<SoundTest>) {
    if keys.just_pressed(KeyCode::F11) {
        v.open = !v.open;
        info!("assets {}", if v.open { "open" } else { "closed" });
    }
    // The sound viewer, when open too, has the keys.
    if !v.open || st.open {
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let p = |k: KeyCode| keys.just_pressed(k) as isize;
    let one = if shift { 100 } else { 1 };
    let input = Input {
        item: (p(KeyCode::ArrowRight) - p(KeyCode::ArrowLeft)) * one + (p(KeyCode::PageDown) - p(KeyCode::PageUp)) * 10,
        group: (p(KeyCode::ArrowDown) - p(KeyCode::ArrowUp)) * if shift { 10 } else { 1 },
        kind: p(KeyCode::Tab) * if shift { -1 } else { 1 },
        play: keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter),
    };
    if keys.just_pressed(KeyCode::KeyR) {
        v.orbit.auto = !v.orbit.auto;
    }
    for k in TAKEN {
        keys.reset(k);
    }
    let sizes = v.sizes();
    let c = v.nav.apply(input, &sizes);
    v.pending.merge(c);
}

/// Drops the pad the frame sampled while the viewer is open: no key moves Sora.
fn hold_pad(v: Res<AssetViewer>, mut pad: ResMut<crate::sora::Pad>) {
    if v.open {
        *pad = crate::sora::Pad::default();
    }
}

/// The audio kind: steps the sound viewer's catalogue and plays an id the way the sound viewer does
/// (an interface-sound mix, no position).
fn play_audio(mut v: ResMut<AssetViewer>, disc: Res<Disc>, audio: Res<Audio>, cache: Res<SeCache>, info: Res<VoiceInfo>) {
    if !v.open {
        stop_sound(&audio, &mut v.playing);
        return;
    }
    if v.nav.kind != Kind::Audio {
        return;
    }
    let v = &mut *v;
    let first = v.sounds.is_none();
    let cat = v.sounds.get_or_insert_with(|| sound_test::Catalog::load(&disc, &info.lang, &info.names.0));
    if first {
        v.sound = cat.step_bank(sound_test::Cursor::default(), 0);
    }
    let (item, bank, play, kind) = (std::mem::take(&mut v.pending.audio_item), std::mem::take(&mut v.pending.audio_bank), v.pending.play, v.pending.kind);
    if bank != 0 {
        v.sound = cat.step_bank(v.sound, bank);
    }
    if item != 0 {
        v.sound = cat.step_entry(v.sound, item);
    }
    if play {
        v.pending.play = false;
        stop_sound(&audio, &mut v.playing);
    }
    if !(first || kind || bank != 0 || item != 0 || play) {
        return;
    }
    let c = v.sound;
    let Some(b) = cat.banks.get(c.bank) else { return };
    let entry = b.entries.get(c.entry);
    let started = if play { start_sound(&disc, &audio, &cache, b, c.entry) } else { None };
    let text = format!(
        "{} {} bank {} {}/{} entry {} id {} {}{}",
        b.file,
        b.log_form(c.entry),
        b.name,
        c.bank + 1,
        cat.banks.len(),
        c.entry,
        b.id(c.entry),
        entry.filter(|e| e.filled).map_or("empty", |e| e.info.as_str()),
        if play { if started.is_some() { " playing" } else { " not started" } } else { "" }
    );
    v.playing = started.or(v.playing);
    v.line(Kind::Audio, text);
}

fn stop_sound(audio: &Audio, playing: &mut Option<Playing>) {
    let Some(p) = playing.take() else { return };
    let Ok(mut s) = audio.0.lock() else { return };
    let Some(d) = s.driver.as_mut() else { return };
    match p {
        Playing::Se(h) => d.stop_se(h),
        Playing::Voice(h) => d.stop_voice(h),
        Playing::Bgm => d.bgm_stop(0, false),
    }
}

/// Entry `i` of `bank` through the game's request paths, at the sound viewer's mix.
fn start_sound(disc: &Disc, audio: &Audio, cache: &SeCache, bank: &sound_test::Bank, i: usize) -> Option<Playing> {
    if !bank.entries.get(i).is_some_and(|e| e.filled) {
        return None;
    }
    match bank.source {
        sound_test::Source::Se | sound_test::Source::Obj => {
            let apdx = (bank.source == sound_test::Source::Obj).then(|| bank.file.clone());
            let req = PlaySe { bank: bank.number?, index: i as u16, apdx, pos: None };
            crate::audio::play_se(disc, audio, cache, &req, (FULL_VOLUME, CENTRE_PAN)).map(|(h, _)| Playing::Se(h))
        }
        sound_test::Source::Voice => {
            let vsb = crate::audio::voice_bank_file(disc, cache, &bank.file, bank.name.contains('#'))?;
            let (sample, rate) = vsb.line(i)?;
            let mut s = audio.0.lock().ok()?;
            let d = s.driver.as_mut()?;
            Some(Playing::Voice(d.play_voice_at(Arc::new(sample), rate, FULL_VOLUME, CENTRE_PAN)))
        }
        sound_test::Source::Bgm => crate::audio::play_bgm(disc, audio, i as u32).then_some(Playing::Bgm),
    }
}

/// Loads, spawns and logs whatever the last keys changed.
#[allow(clippy::too_many_arguments)]
fn apply(
    mut commands: Commands,
    disc: Res<Disc>,
    lib: Option<Res<EffectLib>>,
    time: Res<Time>,
    mut v: ResMut<AssetViewer>,
    mut assets: (ResMut<Assets<Mesh>>, ResMut<Assets<StandardMaterial>>, ResMut<Assets<Image>>, ResMut<Assets<SkinnedMeshInverseBindposes>>),
    mut anims: Query<&mut Animator>,
    mut vis: Query<&mut Visibility>,
    running: Query<(), With<RunningEffect>>,
) {
    let v = &mut *v;
    if !v.open {
        if let Some(s) = v.stage.take() {
            commands.entity(s).despawn();
        }
        clear_effects(&mut commands, v, &running);
        if v.shown.take().is_some() {
            // Opened again: everything is shown afresh.
            v.pending.merge(EVERYTHING);
        }
        return;
    }
    let (meshes, materials, images, bindposes) = (&mut *assets.0, &mut *assets.1, &mut *assets.2, &mut *assets.3);
    if v.cat.is_none() {
        let cat = load_catalogue(&disc);
        v.objentry = load_objentry(&disc);
        info!("assets: {} models in {} groups, {} effect files, {} image files", cat.model_count(), cat.models.len(), cat.effects.len(), cat.images.len());
        if let Ok(start) = std::env::var("KH2_ASSETS_START") {
            if let Some((g, i)) = cat.find_model(&start) {
                v.nav.group = g;
                v.nav.model = i;
            }
            if let Some(e) = cat.effects.iter().position(|f| *f == start) {
                v.nav.effect_file = e;
            }
        }
        v.cat = Some(cat);
        v.pending.merge(EVERYTHING);
    }
    let c = std::mem::take(&mut v.pending);
    let kind = v.nav.kind;

    if c.model {
        if let Some(s) = v.stage.take() {
            commands.entity(s).despawn();
        }
        let stage = commands.spawn((Transform::from_translation(STAGE), Visibility::default())).id();
        v.stage = Some(stage);
        let file = v.model_file().unwrap_or_default();
        let shown = spawn_shown(&mut commands, &disc, &v.objentry, stage, &file, meshes, materials, images, bindposes);
        let g = v.cat.as_ref().and_then(|c| c.models.get(v.nav.group));
        let text = format!(
            "{} {} group {} {}/{} objentry {} meshes {} bones {} textures {} motions {}{}",
            shown.file,
            shown.name,
            g.map_or("-", |g| g.name.as_str()),
            v.nav.model + 1,
            g.map_or(0, |g| g.files.len()),
            shown.objentry,
            shown.meshes,
            shown.bones,
            shown.textures.len(),
            shown.motions.len(),
            shown.mset.as_ref().map_or(String::new(), |m| format!(" ({m})"))
        );
        v.shown = Some(shown);
        v.orbit.zoom = 1.0;
        v.line(Kind::Model, text);
    }
    if let Some(s) = v.stage
        && let Ok(mut vv) = vis.get_mut(s)
    {
        *vv = if kind == Kind::Effect { Visibility::Hidden } else { Visibility::Inherited };
    }

    if c.motion || (c.play && kind == Kind::Motion) {
        let text = v.shown.as_ref().map(|s| {
            let Some(&slot) = s.motions.get(v.nav.motion) else { return format!("{} {} no motions", s.mset.as_deref().unwrap_or("-"), s.name) };
            if let Some(mut a) = s.body.and_then(|b| anims.get_mut(b).ok()) {
                a.restart(Some(slot), true, 0.0);
            }
            let ms = s.rig.as_ref().and_then(|r| r.mset.get(slot));
            let (tag, m) = (ms.map_or("-", |x| x.tag.as_str()), ms.and_then(|x| x.motion.as_ref()));
            let trig = ms.map_or((0, 0), |x| (x.triggers.frames.len(), x.triggers.ranges.len()));
            format!(
                "{} {tag} slot {slot} {}/{} frames {:.0}..{:.0} return {:.0} fps {:.0} frame triggers {} range triggers {}",
                s.mset.as_deref().unwrap_or("-"),
                v.nav.motion + 1,
                s.motions.len(),
                m.map_or(0.0, |m| m.frame_start),
                m.map_or(0.0, |m| m.frame_end),
                m.map_or(0.0, |m| m.frame_return),
                m.map_or(0.0, |m| m.fps),
                trig.0,
                trig.1
            )
        });
        if let Some(t) = text {
            if kind == Kind::Motion {
                v.line(Kind::Motion, t);
            } else {
                v.lines[1] = t;
            }
        }
    }

    if c.effect_file {
        clear_effects(&mut commands, v, &running);
        let file = v.effect_file().unwrap_or_default();
        let loaded = if file == "00effect.bar" {
            lib.as_ref().map(|l| l.system.clone())
        } else if let Some(p) = lib.as_ref().and_then(|l| l.objects.get(&file)) {
            Some(p.clone())
        } else {
            let common = disc.read("effect/texcommon.dpd").and_then(|b| Dpd::parse(&b).ok());
            v.fx_serial += 1;
            fx::load(&disc, &file, FX_ID_BASE + v.fx_serial, common.as_ref()).map(Arc::new)
        };
        let mut ids: Vec<u16> = loaded.as_ref().map_or(Vec::new(), |p| p.pax.elements.iter().map(|e| e.id).collect());
        ids.sort_unstable();
        ids.dedup();
        v.fx_ids = ids;
        v.nav.effect = v.nav.effect.min(v.fx_ids.len().saturating_sub(1));
        v.fx_file = Some((file, loaded));
    }
    if c.kind && kind != Kind::Effect {
        clear_effects(&mut commands, v, &running);
    }
    let effect_now = kind == Kind::Effect && (c.effect || c.effect_file || c.kind || c.play);
    if effect_now {
        clear_effects(&mut commands, v, &running);
        let text = start_effect(&mut commands, v);
        v.line(Kind::Effect, text);
    } else if kind == Kind::Effect && !v.fx_running.is_empty() && v.fx_running.iter().all(|&e| !running.contains(e)) {
        // The effect has ended: start it again after a short pause, so it keeps showing.
        let now = time.elapsed_secs();
        match v.fx_restart {
            None => v.fx_restart = Some(now + 0.75),
            Some(t) if now >= t => {
                v.fx_running.clear();
                start_effect(&mut commands, v);
            }
            _ => {}
        }
    }

    if c.texture || c.texture_source || (c.effect_file && v.nav.tex_source == 1) {
        v.images = match v.nav.tex_source {
            0 => v.shown.as_ref().map_or(Vec::new(), |s| s.textures.clone()),
            1 => v.fx_file.as_ref().and_then(|(_, p)| p.as_ref()).map_or(Vec::new(), |p| effect_images(p)),
            n => v.cat.as_ref().and_then(|c| c.images.get(n - 2)).map_or(Vec::new(), |f| file_images(&disc, f)),
        };
        v.nav.texture = v.nav.texture.min(v.images.len().saturating_sub(1));
        let source = match v.nav.tex_source {
            0 => format!("model {}", v.shown.as_ref().map_or("-", |s| s.file.as_str())),
            1 => format!("effect file {}", v.fx_file.as_ref().map_or("-", |f| f.0.as_str())),
            n => format!("image file {}", v.cat.as_ref().and_then(|c| c.images.get(n - 2)).map_or("-", |s| s.as_str())),
        };
        let pick = v.images.get(v.nav.texture).cloned();
        v.image = pick.as_ref().and_then(|img| make_image(img, images));
        let text = match &pick {
            Some(img) => format!("{source} {} {}/{} {}x{} {}", img.label, v.nav.texture + 1, v.images.len(), img.width, img.height, img.info),
            None => format!("{source} no images"),
        };
        if kind == Kind::Texture {
            v.line(Kind::Texture, text);
        } else {
            v.lines[2] = text;
        }
    }
    if c.kind {
        // Switching kind prints the selection of the new kind again.
        let i = Kind::ALL.iter().position(|&k| k == kind).unwrap_or(0);
        if matches!(kind, Kind::Model | Kind::Texture) || (kind == Kind::Motion && !c.motion) {
            let t = v.lines[i].clone();
            info!("asset {} {t}", kind.word());
        }
    }
}

fn clear_effects(commands: &mut Commands, v: &mut AssetViewer, running: &Query<(), With<RunningEffect>>) {
    for e in v.fx_running.drain(..) {
        if running.contains(e) {
            commands.entity(e).despawn();
        }
    }
    v.fx_restart = None;
}

/// Starts the selected element id of the current effect file at the stage; returns its log text.
fn start_effect(commands: &mut Commands, v: &mut AssetViewer) -> String {
    v.fx_restart = None;
    let Some((file, pax)) = v.fx_file.as_ref() else { return "no effect file".into() };
    let of = format!("{}/{}", v.nav.effect_file + 1, v.cat.as_ref().map_or(0, |c| c.effects.len()));
    let Some(pax) = pax else { return format!("{file} {of} has no effect data (no PAX)") };
    let Some(&id) = v.fx_ids.get(v.nav.effect) else { return format!("{file} {of} has no elements") };
    let el = pax.pax.elements.iter().position(|e| e.id == id);
    let e = el.map(|i| &pax.pax.elements[i]);
    let dpx = e.and_then(|e| fx::dpx_effect(&pax.pax, e.effect));
    let started = fx::start_placed(commands, pax, id, Bind::Fixed(Mat4::from_translation(EFFECT_AT)), Placement::default());
    let text = format!(
        "{file} {of} id {id} ({id:#x}) {}/{} element {} dpx number {} dpd {} particle data {} group {} category {:#x} flags {:#x} bone {} se {} wait {} {}",
        v.nav.effect + 1,
        v.fx_ids.len(),
        el.map_or(-1, |i| i as i64),
        e.map_or(-1, |e| e.effect as i64),
        dpx.map_or(-1, |d| d.dpd as i64),
        dpx.map_or(-1, |d| d.particle_data as i64),
        e.map_or(0, |e| e.group),
        e.map_or(0, |e| e.category),
        e.map_or(0, |e| e.flags),
        e.map_or(0, |e| e.bone),
        e.map_or(0, |e| e.se),
        e.map_or(0.0, |e| e.start_wait),
        if started.is_some() { "playing" } else { "not started" }
    );
    v.fx_running.extend(started);
    text
}

/// Loads `file` (a skeletal `.mdlx` or a `.map`) and spawns it on the stage.
#[allow(clippy::too_many_arguments)]
fn spawn_shown(
    commands: &mut Commands,
    disc: &Disc,
    objentry: &[(u32, String, String)],
    stage: Entity,
    file: &str,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) -> Shown {
    let name = file.rsplit('/').next().unwrap_or(file).rsplit_once('.').map_or(file, |(n, _)| n).to_string();
    let mut shown = Shown { file: file.to_string(), name: name.clone(), radius: 1.0, centre: STAGE, objentry: "-".into(), ..default() };
    let rows: Vec<&(u32, String, String)> = objentry.iter().filter(|r| r.1 == name).collect();
    if !rows.is_empty() {
        shown.objentry = rows.iter().map(|r| format!("{:#x}", r.0)).collect::<Vec<_>>().join(",");
    }
    let Some(data) = disc.read(file) else { return shown };
    let to_img = |label: String, t: &kh2_model::Texture| {
        let [ru, rv] = t.region;
        Img {
            label,
            width: ru.len,
            height: rv.len,
            rgba: crate::assets::crop(t),
            info: format!("(sheet {}x{}, region u {}+{} {:?}, v {}+{} {:?})", t.width, t.height, ru.start, ru.len, ru.wrap, rv.start, rv.len, rv.wrap),
        }
    };
    let mut points: Vec<Vec3> = Vec::new();
    if file.ends_with(".map") {
        let Some(map) = kh2_model::map_file(&data) else { return shown };
        let root = commands.spawn((Transform::from_scale(Vec3::splat(crate::SCALE)), Visibility::default(), ChildOf(stage))).id();
        crate::assets::spawn_model(commands, root, &map.map, meshes, materials, images);
        shown.meshes = map.map.meshes.len();
        shown.textures = map.map.textures.iter().enumerate().map(|(i, t)| to_img(format!("MAP #{i}"), t)).collect();
        points.extend(map.map.meshes.iter().flat_map(|m| m.positions.iter().map(|p| Vec3::from(*p))));
        if let Some(sky) = &map.sky {
            crate::assets::spawn_model(commands, root, sky, meshes, materials, images);
            shown.meshes += sky.meshes.len();
            shown.textures.extend(sky.textures.iter().enumerate().map(|(i, t)| to_img(format!("SK0 #{i}"), t)));
        }
    } else {
        let Some(model) = kh2_model::mdlx(&data) else { return shown };
        let mset_name = rows.first().map(|r| r.2.trim_end_matches(".mset").to_string()).filter(|s| !s.is_empty()).unwrap_or(name.clone());
        let mset_file = format!("obj/{mset_name}.mset");
        let mset = disc.0.lock().ok().and_then(|mut a| a.read(&mset_file).ok()).and_then(|b| kh2_anim::Mset::parse(&b));
        shown.mset = mset.as_ref().map(|_| mset_file);
        let mset = mset.unwrap_or(kh2_anim::Mset { slots: Vec::new() });
        shown.motions = (0..mset.slots.len()).filter(|&i| mset.has(i)).collect();
        shown.meshes = model.meshes.len();
        shown.bones = model.bones.len();
        shown.textures = model.textures.iter().enumerate().map(|(i, t)| to_img(format!("#{i}"), t)).collect();
        points.extend(model.meshes.iter().flat_map(|m| m.positions.iter().map(|p| Vec3::from(*p))));
        let rig = Arc::new(Rig { model, mset, apdx: Some(format!("obj/{name}.apdx")) });
        let body = crate::skel::spawn_skinned(commands, stage, rig.clone(), meshes, materials, images, bindposes, crate::SCALE);
        if let Some(&slot) = shown.motions.first() {
            commands.queue(move |w: &mut World| {
                if let Some(mut a) = w.get_mut::<Animator>(body) {
                    a.restart(Some(slot), true, 0.0);
                }
            });
        }
        shown.body = Some(body);
        shown.rig = Some(rig);
    }
    if let Some((c, r)) = cat::frame_points(&points) {
        shown.centre = STAGE + c * crate::SCALE;
        shown.radius = (r * crate::SCALE).max(0.2);
    }
    shown
}

/// The textures of an effect file's DPDs.
fn effect_images(p: &LoadedPax) -> Vec<Img> {
    let mut out = Vec::new();
    for (d, dpd) in p.dpds.iter().enumerate() {
        for (k, t) in dpd.textures.iter().enumerate() {
            let psm = match t.format {
                0x13 => "PSMT8",
                0x14 => "PSMT4",
                _ => "other",
            };
            out.push(Img { label: format!("dpd {d} texture {k}"), width: t.width as u32, height: t.height as u32, rgba: t.rgba.clone(), info: format!("format {:#x} {psm} tex0 {:#x}", t.format, t.tex0) });
        }
    }
    out
}

/// The images of a 2D file: an IMGD, an IMGZ, or a BAR of them (`.2dd`, `.2ld`).
fn file_images(disc: &Disc, file: &str) -> Vec<Img> {
    use kh2_image::imgd;
    let Some(data) = disc.read(file) else { return Vec::new() };
    let one = |label: String, b: &[u8]| imgd::decode(b).map(|i| Img { label, width: i.width, height: i.height, rgba: i.rgba, info: format!("IMGD{}", if i.swizzled { " (GS order)" } else { "" }) });
    let many = |prefix: &str, b: &[u8]| -> Vec<Img> {
        if imgd::is_imgd(b) {
            one(prefix.to_string(), b).into_iter().collect()
        } else if b.starts_with(b"IMGZ") {
            imgd::split_imgz(b).iter().enumerate().filter_map(|(k, x)| one(format!("{prefix} IMGZ #{k}"), x)).collect()
        } else {
            Vec::new()
        }
    };
    match kh2_bar::parse(&data) {
        Ok(entries) => entries.iter().enumerate().flat_map(|(i, e)| many(&format!("entry {i} {}", e.tag), e.data)).collect(),
        Err(_) => many("file", &data),
    }
}

/// A UI image of `img`, or `None` when its pixel data does not match its size.
fn make_image(img: &Img, images: &mut Assets<Image>) -> Option<(Handle<Image>, u32, u32)> {
    if img.width == 0 || img.height == 0 || img.rgba.len() != (img.width * img.height * 4) as usize {
        return None;
    }
    let mut i = Image::new(
        Extent3d { width: img.width, height: img.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        img.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    i.sampler = bevy::image::ImageSampler::nearest();
    Some((images.add(i), img.width, img.height))
}

/// Orbits the camera around the shown model (or the effect point) while the viewer is open; the
/// game camera's own transform is replaced after it has run.
fn camera(
    mut v: ResMut<AssetViewer>,
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    st: Res<SoundTest>,
    mut cam: Query<&mut Transform, With<Camera3d>>,
) {
    if !v.open || st.open {
        return;
    }
    let Ok(mut t) = cam.single_mut() else { return };
    let o = &mut v.orbit;
    if buttons.pressed(MouseButton::Right) {
        o.yaw -= motion.delta.x * 0.006;
        o.pitch = (o.pitch + motion.delta.y * 0.006).clamp(-1.4, 1.4);
    } else if o.auto {
        o.yaw += time.delta_secs() * 0.4;
    }
    if scroll.delta.y != 0.0 {
        o.zoom = (o.zoom * (1.0 - scroll.delta.y.signum() * 0.1)).clamp(0.05, 10.0);
    }
    let (yaw, pitch, zoom) = (o.yaw, o.pitch, o.zoom);
    let (target, radius) = if v.nav.kind == Kind::Effect { (EFFECT_AT, 2.0) } else { v.shown.as_ref().map_or((STAGE, 1.0), |s| (s.centre, s.radius)) };
    let dist = (radius * 2.6 * zoom).clamp(0.3, 900.0);
    let dir = Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos());
    *t = Transform::from_translation(target + dir * dist).looking_at(target, Vec3::Y);
}

fn show(
    v: Res<AssetViewer>,
    st: Res<SoundTest>,
    anims: Query<&Animator>,
    mut text: Query<(&mut Text, &mut Visibility), (With<ViewerText>, Without<ViewerImage>)>,
    mut pic: Query<(&mut ImageNode, &mut Node, &mut Visibility), With<ViewerImage>>,
) {
    let open = v.open && !st.open;
    if let Ok((mut img, mut node, mut vis)) = pic.single_mut() {
        match (&v.image, open && v.nav.kind == Kind::Texture) {
            (Some((h, w, hgt)), true) => {
                if img.image != *h {
                    img.image = h.clone();
                }
                // Small images are scaled up by whole steps, large ones down to fit 512 pixels.
                let big = (*w).max(*hgt) as f32;
                let scale = if big <= 128.0 { (256.0 / big).floor().max(1.0) } else { (512.0 / big).min(1.0) };
                node.width = Val::Px(*w as f32 * scale);
                node.height = Val::Px(*hgt as f32 * scale);
                *vis = Visibility::Inherited;
            }
            _ => *vis = Visibility::Hidden,
        }
    }
    let Ok((mut t, mut vis)) = text.single_mut() else { return };
    *vis = if open { Visibility::Inherited } else { Visibility::Hidden };
    if !open {
        return;
    }
    let tabs: Vec<String> = Kind::ALL.iter().map(|&k| if k == v.nav.kind { format!("[{}]", k.word().to_uppercase()) } else { k.word().to_string() }).collect();
    let i = Kind::ALL.iter().position(|&k| k == v.nav.kind).unwrap_or(0);
    let keys = match v.nav.kind {
        Kind::Model => "Left/Right model (Shift 100) | PgUp/PgDn 10 | Up/Down group",
        Kind::Motion => "Left/Right motion | PgUp/PgDn 10 | Up/Down model | Enter restart",
        Kind::Texture => "Left/Right image | PgUp/PgDn 10 | Up/Down source (model, effect file, image files)",
        Kind::Effect => "Left/Right element id | PgUp/PgDn 10 | Up/Down effect file | Enter replay",
        Kind::Audio => "Left/Right id | PgUp/PgDn 10 | Up/Down bank | Enter play (F10: sound viewer)",
    };
    let frame = match (v.nav.kind, v.shown.as_ref().and_then(|s| s.body).and_then(|b| anims.get(b).ok())) {
        (Kind::Motion, Some(a)) => format!("\nframe {:.1} ({})", a.frame, a.tag()),
        _ => String::new(),
    };
    t.0 = format!(
        "[ASSETS]  F11 close | Tab kind | {keys} | R auto-orbit, RMB drag, wheel zoom\n{}\n{}: {}{frame}\nmodel: {}",
        tabs.join("  "),
        v.nav.kind.word(),
        v.lines[i],
        v.lines[0]
    );
}
