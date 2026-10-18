//! The 3D model of a character's own page (0xf, 0x12, 0x15) and of the model page behind it (0x10, 0x13, 0x16)
//! (docs/menus.md 20.12; WP-JR2).
//!
//! The original (`SLUS_210.05`): `FUN_002abf60(p)` builds the model of the shown record (p = 0 the small one on the character page,
//! p = 1 the large one on the model page). It stands in a preview object of the class `FUN_0023a018` made for the scene (p = 0 uses the
//! object `DAT_0036d1f8` that the camp's preview uses, p = 1 an object of its own, `DAT_0036d494`) through the vtable's +0x18:
//! `FUN_0023a520(slot 0, object id | 0x30000000, motion = record +0xe, variant 2, x, y)`, so the same slot camera as the camp's
//! preview (eye (0, -50, -800), look-at (0, 0, 55), the projection centred x, y from the screen's middle; docs/menus.md 16.8) and
//! the motion pick of `FUN_001a7a48(set, motion, 2)`. The record (`jiminy.bar` entry 3, 0x24 bytes) gives the rest:
//!
//! | | small model (p = 0) | large model (p = 1) |
//! |---|---|---|
//! | x, y | +0x12, +0x14 | +0x18, +0x1a |
//! | turn about the vertical axis (degrees, `FUN_0023b3c8`) | the short at +0x16 | 0 |
//! | scale (`FUN_0023b408`, all three axes) | the float at +0x1c | the float at +0x20 |
//!
//! The depth is `DAT_00382df8` = -100 (`FUN_0023b348`). The object ids of the list at 0x382e68 (one: 996) instead stand at that
//! float plus -100 with the scale `DAT_00382dfc` = 1. The record's flags `+0x10` pick mesh parts through `FUN_0023b640` (below).
//! The journal never calls `FUN_0023b448` (the camp's and the Party page's 0x60 tint): the models draw in the default colour 0x80.
//!
//! The model page rotates the large model: the list flags `0x30000` make Left / Right the results -12 / -13, which
//! `FUN_002ac228` turns into the rotation +2 / -2 degrees a frame (`DAT_00382e08`); the object ids of the list at 0x382e60 (one:
//! 2300) stop at +16 / -16 (`DAT_00382e0c`, `DAT_00382e10`), every other model wraps at +-360 (`DAT_00382e14`).
//!
//! Port: one camera of its own renders the model into a picture of the menu canvas (render layer 10), shown by one node of the menu
//! scene. The object id is looked up in `00objentry` (`ModelName`, `AnimationName`), as the field's objects are.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use kh2_tables::tables as t;

use super::chars::CharRec;
use super::Journal;
use crate::assets::Disc;
use crate::menu::{Menu, Phase};
use crate::menu_gfx::{MenuScene, Pic, Tex};
use crate::menu::preview::PreviewCamera;
use crate::skel::{Animator, Rig};

/// Render layer of the journal's model camera and its model (the camp's preview uses 9, the dim 9, the menu 8).
const LAYER: usize = 10;
/// Where the model's private stage stands (Bevy units), far below any area.
const STAGE: Vec3 = Vec3::new(0.0, -12_000.0, 0.0);
/// The picture's size: the menu canvas (512 x 448 units) at twice the resolution.
const TARGET: [u32; 2] = [1024, 896];

/// The slot camera of `FUN_0023a520` (KH2 space, y down): eye 0x37f468 / 0x37f46c, look-at 0x37f470.
const EYE: Vec3 = Vec3::new(0.0, -50.0, -800.0);
const LOOK_AT: Vec3 = Vec3::new(0.0, 0.0, 55.0);
/// The model's depth (`FUN_0023b348(DAT_00382df8)`) and the scale of the models of the second list (`DAT_00382dfc`).
const DEPTH: f32 = -100.0;
const NEAR_SCALE: f32 = 1.0;
/// The object ids whose rotation stops at +-16 degrees (`FUN_002ac660`, the u16 list at 0x382e60) and those that stand nearer
/// by the record's float (`FUN_002ac6b8`, the list at 0x382e68). Both lists are halfword arrays ending at 0 (`sll 1` in the loops).
pub const TURN_LIMITED: [u16; 1] = [2300];
pub const NEAR: [u16; 1] = [996];
/// The turn a frame (`DAT_00382e08`), its limits (`DAT_00382e0c`, `DAT_00382e10`) and the wrap (`DAT_00382e14`), degrees.
pub const TURN_STEP: f32 = 2.0;
pub const TURN_LIMIT: f32 = 16.0;
pub const TURN_WRAP: f32 = 360.0;
/// The motion variant `FUN_0023a520` asks for.
const VARIANT: usize = 2;

/// The model page that follows a character page (`FUN_002a9b80` case -6: 0x10 from 0xf, 0x13 from 0x12, else 0x16).
pub fn view_page(shown: u16) -> u16 {
    match shown {
        0xf => 0x10,
        0x12 => 0x13,
        _ => 0x16,
    }
}

/// The character page a model page returns to. `FUN_002aa0b0` case -2 passes 0xf from 0x10 and 0x15 from both 0x13 and 0x16 (0x2aa2ec:
/// one compare with 0x10), so the original's Heartless view lands on the Nobodies' page number (`CharPage::nobody`: its cancel then goes to the
/// Nobodies' list).
pub fn char_page(view: u16) -> u16 {
    match view {
        0x10 => 0xf,
        _ => 0x15,
    }
}

/// Whether the page is one of the three model pages.
pub fn is_view(page: u16) -> bool {
    matches!(page, 0x10 | 0x13 | 0x16)
}

/// How one model stands (`FUN_002abf60`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// The projection centre's offset from the screen's middle, menu units.
    pub at: [f32; 2],
    /// The turn about the vertical axis, degrees.
    pub turn: f32,
    /// The depth of the model's origin in the slot's world (`FUN_0023b348`).
    pub z: f32,
    /// The scale on all three axes (`FUN_0023b408`).
    pub scale: f32,
}

/// The pose of a record's model: the small one (`large` false) or the large one.
pub fn pose(rec: &CharRec, large: bool) -> Pose {
    let p = rec.place[usize::from(large)];
    let near = NEAR.contains(&rec.model);
    Pose {
        at: [f32::from(p.x), f32::from(p.y)],
        turn: if large { 0.0 } else { f32::from(p.turn) },
        z: if near { p.scale + DEPTH } else { DEPTH },
        scale: if near { NEAR_SCALE } else { p.scale },
    }
}

/// The mesh-part mask of a record (`FUN_002abf60` before `FUN_0023b640`): the flags' low ten bits, moved up by ten bits when flag
/// 0x1000 and 0x2000 are set and by twenty when only 0x1000 is.
pub fn part_mask(flags: u16) -> u32 {
    let low = u32::from(flags & 0x3ff);
    match (flags & 0x1000 != 0, flags & 0x2000 != 0) {
        (false, _) => low,
        (true, true) => low << 10,
        (true, false) => low << 20,
    }
}

/// The meshes the part mask hides (`FUN_0023b640` calls `FUN_0011c5b0(model, bit)` for every set bit, which clears the draw flags of the meshes
/// whose group, bits 6..10 of the mesh header's flags word, equals the bit number: `FUN_00121278`).
pub fn hidden(groups: &[u8], mask: u32) -> Vec<bool> {
    groups.iter().map(|&g| g < 32 && mask >> g & 1 != 0).collect()
}

/// One frame of the rotation keys (`FUN_002ac228`): `dir` +1 for Left (-12) and -1 for Right (-13). The result of a limited model
/// stops at +-16; the others wrap at +-360.
pub fn turned(turn: f32, dir: i32, model: u16) -> f32 {
    let limited = TURN_LIMITED.contains(&model);
    let mut v = if dir > 0 { turn + TURN_STEP } else { turn - TURN_STEP };
    if limited {
        v = v.clamp(-TURN_LIMIT, TURN_LIMIT);
    }
    if TURN_WRAP <= v {
        v -= TURN_WRAP;
    } else if v <= -TURN_WRAP {
        v += TURN_WRAP;
    }
    v
}

/// The motion entry `FUN_001a7a48(set, motion, 2)` picks: entry `motion * 4 + 2` of a player set (then +3, +2, +1, +0), or entry
/// `motion` of any other set.
pub fn motion_slot(rig: &Rig, motion: usize) -> Option<usize> {
    if rig.mset.slots.len() > motion * 4 {
        [VARIANT, 3, 2, 1, 0].into_iter().map(|v| motion * 4 + v).find(|&s| rig.mset.has(s))
    } else {
        Some(motion).filter(|&s| rig.mset.has(s))
    }
}

/// The turn about the vertical axis as the app's rotation: KH2's positive turn (`FUN_0023b3c8`, degrees) carries the model's face to the
/// screen's left, toward the page's text [inferred from the sign of the records' turns: every small model's +0x16 is positive, and
/// with this sign the MEMO pose, which stands turned to the right, comes out facing the viewer].
fn spin(turn: f32) -> Quat {
    Quat::from_rotation_y(-turn.to_radians())
}

/// The slot camera's numbers for a model at depth `z`: how far below the projection centre its origin lands (menu units), its
/// depth from the eye and the camera's pitch.
pub fn projection(z: f32) -> (f32, f32, f32) {
    let d = LOOK_AT - EYE;
    let pitch = d.y.atan2(d.z);
    let rel = Vec3::new(0.0, 0.0, z) - EYE;
    let (s, c) = pitch.sin_cos();
    let depth = rel.y * s + rel.z * c;
    let y = rel.y * c - rel.z * s;
    let (_, fy) = crate::menu::preview::focal();
    (fy * y / depth, depth, pitch)
}

/// What the journal shows now: the record, whether it is the large model, and the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Want {
    pub ch: usize,
    pub large: bool,
}

/// The model a page shows (`FUN_002a9b80` builds the small one every frame it has none, `FUN_002aa0b0` the large one): pages 0xf / 0x12 /
/// 0x15 the small model of the shown record `list[cur]`, 0x10 / 0x13 / 0x16 the large one; every other page none, and a record without
/// a model (`+0xc` = 0) none.
pub fn choose(page: u16, list: &[usize], cur: usize, chars: &[CharRec]) -> Option<Want> {
    let large = is_view(page);
    if !large && !matches!(page, 0xf | 0x12 | 0x15) {
        return None;
    }
    let ch = *list.get(cur)?;
    (chars.get(ch)?.model != 0).then_some(Want { ch, large })
}

fn want(j: &Journal) -> Option<Want> {
    let o = j.open.as_ref()?;
    choose(o.page, &o.cp.list, o.cp.cur, &j.chars)
}

/// Whether the page shows a model now, and whether it is the large one (the alpha object that leaves with a page turn).
pub(super) fn shown_size(j: &Journal) -> Option<bool> {
    want(j).map(|w| w.large)
}

/// The colour alpha (1 = 0x80) of a group's first animation at `frame`: the start colour when the animation steps its colour (flag 0x80), else
/// the linear blend of start and end (`FUN_00236cf8`).
pub fn group_alpha(s: &kh2_image::seqd::Seqd, group: usize, frame: i32) -> f32 {
    use kh2_image::seqd::flag;
    let Some(g) = s.groups.get(group) else { return 1.0 };
    let Some(a) = s.anims.get(g.first) else { return 1.0 };
    let (c0, c1) = (f32::from(a.color_start[3]), f32::from(a.color_end[3]));
    let span = (a.frame_end - a.frame_start).max(1) as f32;
    let t = ((frame - a.frame_start) as f32 / span).clamp(0.0, 1.0);
    let v = if a.flags & flag::COLOR_STEP != 0 { c0 } else { c0 + (c1 - c0) * t };
    (v / 128.0).clamp(0.0, 1.0)
}

/// The model takes its alpha from the alpha object of its size (`FUN_0023b4e0`: the object's colour alpha, `FUN_00237518(object + 0x18) >> 24`):
/// group 7 raises it from 0 to 0x80 in 24 frames, group 8 holds 0x80, group 9 lowers it in 12 frames (docs/menus.md 20.14). The port draws the model
/// into a picture, so the picture carries the alpha.
fn fade(j: Res<Journal>, v: Res<ModelView>, mut scene: ResMut<MenuScene>) {
    let (Some(node), Some((w, _))) = (v.node, v.key) else { return };
    let n = &scene.nodes[j.nodes.cpage.alpha[usize::from(w.large)]];
    let alpha = match (n.player.group, n.seq.and_then(|s| scene.seqs.get(s))) {
        (Some(g), Some(s)) => group_alpha(s, g, n.player.frame()),
        _ => 1.0,
    };
    scene.nodes[node].label_fade = alpha;
}

#[derive(Component)]
struct ViewModel {
    body: Entity,
    state: kh2_anim::rig::RigState,
    done: bool,
    /// Which mesh (in file order) the record's part mask hides (`FUN_0023b640` -> `FUN_00121278`: the meshes of the group whose number is a set bit).
    hide: Vec<bool>,
}

/// A rig with the mesh group of each of its meshes.
type Loaded = (Arc<Rig>, Vec<u8>);

/// The model the picture shows now, its camera and the rotation of the large model.
#[derive(Resource, Default)]
pub(super) struct ModelView {
    target: Handle<Image>,
    node: Option<usize>,
    camera: Option<Entity>,
    root: Option<Entity>,
    key: Option<(Want, u16)>,
    /// The large model's turn, degrees (`FUN_0023b3c8` on the object `DAT_0036d494`).
    turn: f32,
    objects: Option<Vec<t::Objentry>>,
    rigs: HashMap<u32, Option<Loaded>>,
}

pub struct ModelPlugin;

impl Plugin for ModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ModelView>().add_systems(Update, (show, fade, animate, layer).chain().after(super::tick).before(crate::schedule::Frame::Popups));
    }
}

/// The motion file of a journal model (`FUN_0016eda8` with the id's bit 0x10000000: the object's own set with its extension replaced by
/// `_MEMO.mset`, or `<model>_MEMO.mset` when the row names no set).
pub fn memo_path(model: &str, animation: &str) -> String {
    if animation.is_empty() {
        format!("obj/{model}_MEMO.mset")
    } else {
        let stem = animation.split('.').next().unwrap_or(animation);
        format!("obj/{stem}_MEMO.mset")
    }
}

/// The rig of an object id: `00objentry`'s model and motion set.
fn rig_of(v: &mut ModelView, disc: &Disc, id: u32) -> Option<Loaded> {
    if v.objects.is_none() {
        let rows = disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default();
        v.objects = Some(rows);
    }
    if let Some(r) = v.rigs.get(&id) {
        return r.clone();
    }
    let rig = v.objects.as_ref().and_then(|o| o.iter().find(|r| r.object_id == id)).and_then(|row| {
        let model = row.model_name.clone();
        let file = disc.read(&format!("obj/{model}.mdlx"))?;
        let groups = crate::intro::event_player::mesh_groups(&file).unwrap_or_default();
        let m = kh2_model::mdlx(&file)?;
        let s = disc.read(&memo_path(&model, &row.animation_name)).and_then(|d| kh2_anim::Mset::parse(&d))?;
        let apdx = Some(format!("obj/{model}.apdx")).filter(|a| disc.read(a).is_some());
        Some((Arc::new(Rig { model: m, mset: s, apdx }), groups))
    });
    v.rigs.insert(id, rig.clone());
    rig
}

#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    j: Res<Journal>,
    menu: Res<Menu>,
    mut scene: ResMut<MenuScene>,
    disc: Res<Disc>,
    mut v: ResMut<ModelView>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let wanted = if menu.phase == Phase::Closed || scene.texs.is_empty() { None } else { want(&j) };
    let page = j.open.as_ref().map_or(0, |o| o.page);
    // The page's number in the original (the character pages of the lists run as page 0xf).
    let label = j.open.as_ref().map_or(0, |o| if is_view(page) { view_page(super::shown_page(o.cp.back)) } else { super::shown_page(o.cp.back) });
    // The picture, once; the node was made with the page's objects (after the layer, so the page's texts stand in front of the model).
    if v.node.is_none() && wanted.is_some() && j.loaded {
        let size = Extent3d { width: TARGET[0], height: TARGET[1], depth_or_array_layers: 1 };
        let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        let h = images.add(image);
        v.target = h.clone();
        scene.texs.push(Tex { width: TARGET[0], height: TARGET[1], linear: h.clone(), nearest: h });
        let tex = scene.texs.len() - 1;
        let y = crate::menu_gfx::Y_BIAS;
        let node = j.nodes.cpage.model;
        scene.nodes[node].pic = Some(Pic { tex, rect: [0.0, -y, 512.0, 448.0 - y], v_end: 1.0 });
        v.node = Some(node);
    }
    let Some(node) = v.node else { return };
    let key = wanted.map(|w| (w, page));
    if key == v.key && (key.is_none() || v.camera.is_some()) {
        scene.nodes[node].visible = key.is_some();
        return;
    }
    // Something changed: the model stands again.
    if let Some(c) = v.camera.take() {
        commands.entity(c).despawn();
    }
    if let Some(r) = v.root.take() {
        commands.entity(r).despawn();
    }
    scene.nodes[node].visible = false;
    if v.key.map(|k| k.0.large) != wanted.map(|w| w.large) || v.key.map(|k| k.0.ch) != wanted.map(|w| w.ch) {
        // The large model starts unturned (`FUN_002abf60(1)` passes 0.0).
        v.turn = 0.0;
    }
    v.key = key;
    let Some(w) = wanted else { return };
    // A new model is a new alpha object (`FUN_002abc40` makes it with the model): it fades in.
    let alpha = j.nodes.cpage.alpha[usize::from(w.large)];
    scene.set_group_then(alpha, super::group::MODEL_ALPHA[0], super::group::MODEL_ALPHA[1]);
    let rec = j.chars[w.ch];
    let Some((rig, groups)) = rig_of(&mut v, &disc, u32::from(rec.model)) else {
        info!("journal: page {label:#x} model: object {} has no model files; nothing is drawn (FUN_002abf60)", rec.model);
        return;
    };
    let p = pose(&rec, w.large);
    let turn = if w.large { v.turn } else { p.turn };
    let (drop, depth, pitch) = projection(p.z);
    let (fx, fy) = crate::menu::preview::focal();
    let fov = 2.0 * (224.0 / fy).atan();
    let camera = commands
        .spawn((
            Camera3d::default(),
            PreviewCamera,
            Camera { order: -2, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(v.target.clone().into()),
            Projection::Perspective(PerspectiveProjection { fov, near: 0.01, far: 100.0, ..default() }),
            Transform::from_translation(STAGE),
            RenderLayers::layer(LAYER),
        ))
        .id();
    v.camera = Some(camera);
    let at = Vec2::new(256.0 + p.at[0], 208.0 + p.at[1] + drop);
    let d = depth * crate::SCALE;
    let local = Vec3::new((at.x - 256.0) * d / fy, -(at.y - 208.0) * d / fy, -d);
    let rot = Quat::from_rotation_x(pitch) * spin(turn);
    let scale = Vec3::new(crate::SCALE * fx / fy, crate::SCALE, crate::SCALE) * p.scale;
    let root = commands.spawn((Transform { translation: STAGE + local, rotation: rot, scale }, Visibility::default(), RenderLayers::layer(LAYER))).id();
    let body = crate::skel::spawn_skinned(&mut commands, root, rig.clone(), &mut meshes, &mut materials, &mut images, &mut bindposes, 1.0);
    let m = motion_slot(&rig, usize::from(rec.motion));
    commands.entity(root).insert(ViewModel { body, state: Default::default(), done: false, hide: hidden(&groups, part_mask(rec.flags)) });
    info!(
        "journal: page {label:#x} model (FUN_002abf60({})): character {}, object {} ({}), motion {} -> slot {m:?}, at menu ({:.0}, {:.0}), turn {turn}, depth {:.0}, scale {}",
        u8::from(w.large),
        w.ch,
        rec.model,
        rig.apdx.as_deref().unwrap_or("-"),
        rec.motion,
        at.x,
        at.y,
        depth,
        p.scale
    );
    commands.queue(move |world: &mut World| {
        if let Some(mut a) = world.get_mut::<Animator>(body) {
            a.restart(m, true, 0.0);
        }
    });
    v.root = Some(root);
    scene.nodes[node].visible = true;
}

/// One 60 Hz step of the model's motion (the simulation and `skel::animate` are stopped while the menu is open), and the turn of the
/// large model: Left / Right turn it 2 degrees a frame (`FUN_002ac228`).
#[allow(clippy::too_many_arguments)]
fn animate(
    mut q: Query<(Entity, &mut ViewModel)>,
    mut anims: Query<&mut Animator>,
    mut joints: Query<&mut Transform, Without<ViewModel>>,
    mut roots: Query<&mut Transform, With<ViewModel>>,
    j: Res<Journal>,
    mut v: ResMut<ModelView>,
    keys: Res<ButtonInput<KeyCode>>,
    real: Res<Time<Real>>,
    mut acc: Local<f32>,
) {
    *acc = (*acc + real.delta_secs()).min(0.25);
    let mut ticks = 0;
    while *acc >= 1.0 / 60.0 {
        *acc -= 1.0 / 60.0;
        ticks += 1;
    }
    let large = want(&j).is_some_and(|w| w.large);
    if large && let Some(w) = want(&j) {
        let model = j.chars[w.ch].model;
        let dir = i32::from(keys.pressed(KeyCode::ArrowLeft) || hold(&real, 1)) - i32::from(keys.pressed(KeyCode::ArrowRight) || hold(&real, -1));
        for _ in 0..ticks {
            if dir != 0 {
                v.turn = turned(v.turn, dir, model);
            }
        }
        if dir != 0 && let Some(root) = v.root && let Ok(mut tr) = roots.get_mut(root) {
            let pitch = projection(DEPTH).2;
            tr.rotation = Quat::from_rotation_x(pitch) * spin(v.turn);
        }
    }
    for (_, mut p) in &mut q {
        let p = &mut *p;
        let Ok(mut a) = anims.get_mut(p.body) else { continue };
        let a = &mut *a;
        for _ in 0..ticks {
            a.tick();
        }
        let rig = a.rig.clone();
        let Some(motion) = a.slot.and_then(|s| rig.mset.get(s)).and_then(|s| s.motion.as_ref()) else { continue };
        let mut pose = motion.pose_with(&rig.model.bones, a.frame, &mut p.state);
        if a.root != Default::default() {
            let undo = (Mat4::from_translation(a.root.translation) * Mat4::from_rotation_y(a.root.yaw)).inverse();
            for m in &mut pose {
                *m = undo * *m;
            }
        }
        for (jn, m) in a.joints.iter().zip(&pose) {
            if let Ok(mut t) = joints.get_mut(*jn) {
                *t = Transform::from_matrix(*m);
            }
        }
        a.pose = pose;
    }
}

/// `KH2_JOURNAL_HOLD=<at>:<Left|Right>:<seconds>` (the harness aid of the face pages) as a held rotation key: +1 for Left, -1 for Right.
fn hold(time: &Time<Real>, dir: i32) -> bool {
    let Ok(spec) = std::env::var("KH2_JOURNAL_HOLD") else { return false };
    let now = time.elapsed_secs();
    spec.split(',').any(|item| {
        let mut f = item.split(':');
        let (Some(at), Some(k), Some(len)) = (f.next().and_then(|v| v.parse::<f32>().ok()), f.next(), f.next().and_then(|v| v.parse::<f32>().ok())) else { return false };
        (at..at + len).contains(&now) && matches!((k, dir), ("Left", 1) | ("Right", -1))
    })
}

/// Once the model's meshes exist: put them on the layer of the model camera.
fn layer(mut commands: Commands, mut q: Query<&mut ViewModel>, children: Query<&Children>, meshes: Query<(), With<Mesh3d>>) {
    for mut p in &mut q {
        if p.done {
            continue;
        }
        let Ok(kids) = children.get(p.body) else { continue };
        let mut any = false;
        let mut mesh = 0;
        for k in kids.iter() {
            commands.entity(k).insert(RenderLayers::layer(LAYER));
            if meshes.contains(k) {
                if p.hide.get(mesh).copied().unwrap_or(false) {
                    commands.entity(k).insert(Visibility::Hidden);
                }
                mesh += 1;
            }
            any = true;
        }
        commands.entity(p.body).insert(RenderLayers::layer(LAYER));
        p.done = any;
    }
}

#[cfg(test)]
mod tests {
    use super::super::chars::ModelPlace;
    use super::*;

    fn rec(model: u16, small: ModelPlace, large: ModelPlace) -> CharRec {
        CharRec { model, place: [small, large], ..CharRec::default() }
    }

    /// The two placements read different record fields (`FUN_002abf60`): the small model the turn and +0x12 / +0x14 / +0x1c, the
    /// large one +0x18 / +0x1a / +0x20 and no turn; both stand at the depth -100.
    #[test]
    fn the_small_and_the_large_model_read_their_own_fields() {
        let r = rec(5, ModelPlace { x: 10, y: -20, turn: 30, scale: 1.5 }, ModelPlace { x: -7, y: 9, turn: 99, scale: 0.5 });
        assert_eq!(pose(&r, false), Pose { at: [10.0, -20.0], turn: 30.0, z: -100.0, scale: 1.5 });
        assert_eq!(pose(&r, true), Pose { at: [-7.0, 9.0], turn: 0.0, z: -100.0, scale: 0.5 });
    }

    /// Object 996 stands nearer by the record's float and takes the scale 1 (the list at 0x382e68).
    #[test]
    fn the_near_model_uses_the_float_as_depth() {
        let r = rec(996, ModelPlace { x: 0, y: 0, turn: 0, scale: 40.0 }, ModelPlace { x: 0, y: 0, turn: 0, scale: -30.0 });
        assert_eq!((pose(&r, false).z, pose(&r, false).scale), (-60.0, 1.0));
        assert_eq!((pose(&r, true).z, pose(&r, true).scale), (-130.0, 1.0));
    }

    /// The part mask: flags 0x1000 move the ten low bits up by ten (with 0x2000) or twenty.
    #[test]
    fn the_part_mask_moves_with_the_flags() {
        assert_eq!(part_mask(0x0005), 5);
        assert_eq!(part_mask(0x0400), 0);
        assert_eq!(part_mask(0x3005), 5 << 10);
        assert_eq!(part_mask(0x1005), 5 << 20);
    }

    /// The mask hides the meshes whose group is a set bit.
    #[test]
    fn the_mask_hides_the_meshes_of_its_groups() {
        assert_eq!(hidden(&[0, 1, 2, 1, 31, 40], 0b10), vec![false, true, false, true, false, false]);
        assert_eq!(hidden(&[20, 29, 30], part_mask(0x13ff)), vec![true, true, false]);
        assert_eq!(hidden(&[0, 1], 0), vec![false, false]);
    }

    /// Left adds 2, Right subtracts 2, object 2300 stops at 16, the others wrap at 360.
    #[test]
    fn the_turn_steps_stops_and_wraps() {
        assert_eq!(turned(0.0, 1, 5), 2.0);
        assert_eq!(turned(0.0, -1, 5), -2.0);
        assert_eq!(turned(14.0, 1, 2300), 16.0);
        assert_eq!(turned(16.0, 1, 2300), 16.0);
        assert_eq!(turned(-16.0, -1, 2300), -16.0);
        assert_eq!(turned(359.0, 1, 5), 1.0);
        assert_eq!(turned(-359.0, -1, 5), -1.0);
        assert_eq!(turned(18.0, 1, 5), 20.0);
    }

    /// The model pages follow the character pages and return to them.
    #[test]
    fn the_view_pages_pair_with_the_character_pages() {
        assert_eq!([view_page(0xf), view_page(0x12), view_page(0x15)], [0x10, 0x13, 0x16]);
        assert_eq!([char_page(0x10), char_page(0x13), char_page(0x16)], [0xf, 0x15, 0x15]);
        assert!(is_view(0x13) && !is_view(0x12));
    }

    /// The model sits a little below the projection centre and 702 units from the eye (as the camp's preview does).
    fn recs(n: usize) -> Vec<CharRec> {
        (0..n).map(|i| CharRec { model: if i == 2 { 0 } else { 100 + i as u16 }, ..CharRec::default() }).collect()
    }

    /// The page decides the model: the character pages show the small model and the model pages the large one of the record at the list's
    /// cursor, every other page and a record without a model none.
    #[test]
    fn the_page_picks_the_model_of_the_shown_record() {
        let chars = recs(6);
        let list = [4, 1, 2, 5];
        for page in [0xf, 0x12, 0x15] {
            assert_eq!(choose(page, &list, 0, &chars), Some(Want { ch: 4, large: false }), "page {page:#x}");
            assert_eq!(choose(page, &list, 3, &chars), Some(Want { ch: 5, large: false }));
        }
        for page in [0x10, 0x13, 0x16] {
            assert_eq!(choose(page, &list, 1, &chars), Some(Want { ch: 1, large: true }), "page {page:#x}");
        }
        assert_eq!(choose(0xf, &list, 2, &chars), None, "record 2 has no model");
        assert_eq!(choose(0xf, &list, 9, &chars), None, "past the list");
        assert_eq!(choose(0xf, &[99], 0, &chars), None, "past the records");
        for page in [0, 2, 6, 0xb, 0xe, 0x11, 0x14, 0x17] {
            assert_eq!(choose(page, &list, 0, &chars), None, "page {page:#x}");
        }
    }

    /// The motion file name: the object's own set with the extension replaced, else the model's name (`FUN_0016eda8`).
    #[test]
    fn the_memo_set_is_named_after_the_object() {
        assert_eq!(memo_path("N_EX500_RTN", "N_EX500_RTN.mset"), "obj/N_EX500_RTN_MEMO.mset");
        assert_eq!(memo_path("P_EX100_WI", "P_EX100.mset"), "obj/P_EX100_MEMO.mset");
        assert_eq!(memo_path("N_TR580_GM", ""), "obj/N_TR580_GM_MEMO.mset");
    }

    /// Every record of the disc's `jiminy.bar` entry 3 names an object that has a row, a model, a `_MEMO` motion set with its motion and,
    /// where the record has a part mask, meshes of those groups to hide; the two lists of special objects name records of the disc.
    #[test]
    fn the_disc_journal_models() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let rows = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).unwrap().rows;
        let bar = a.read("menu/us/jiminy.bar").unwrap();
        let e = kh2_bar::parse(&bar).unwrap();
        let chars = super::super::chars::parse_chars(e[3].data);
        assert!(chars.len() > 300, "{} records", chars.len());
        let mut seen: HashMap<u16, (bool, bool)> = HashMap::new();
        let mut hid = 0;
        for (i, c) in chars.iter().enumerate() {
            assert!(c.model != 0 && c.motion == 133, "record {i}: model {} motion {}", c.model, c.motion);
            let row = rows.iter().find(|r| r.object_id == u32::from(c.model)).unwrap_or_else(|| panic!("record {i}: no objentry row {}", c.model));
            let (shows, mask_ok) = *seen.entry(c.model).or_insert_with(|| {
                let file = a.read(&format!("obj/{}.mdlx", row.model_name)).unwrap_or_else(|_| panic!("record {i}: no model {}", row.model_name));
                let model = kh2_model::mdlx(&file).unwrap_or_else(|| panic!("model {} does not parse", row.model_name));
                let set = a.read(&memo_path(&row.model_name, &row.animation_name)).unwrap_or_else(|_| panic!("record {i}: no memo set for {}", row.model_name));
                let rig = Rig { model, mset: kh2_anim::Mset::parse(&set).unwrap(), apdx: None };
                let groups = crate::intro::event_player::mesh_groups(&file).unwrap_or_default();
                let h = hidden(&groups, part_mask(c.flags));
                (motion_slot(&rig, usize::from(c.motion)).is_some(), c.flags & 0x3ff == 0 || (h.iter().any(|&x| x) && h.iter().any(|&x| !x)))
            });
            assert!(shows, "record {i} ({}): no motion {} in the memo set", row.model_name, c.motion);
            assert!(mask_ok, "record {i} ({}): the part mask {:#x} hides nothing or everything", row.model_name, part_mask(c.flags));
            hid += usize::from(c.flags & 0x3ff != 0);
        }
        assert!(hid > 20, "{hid} records with a part mask");
        // The two special lists name records of the disc.
        assert!(chars.iter().any(|c| NEAR.contains(&c.model)) && chars.iter().any(|c| TURN_LIMITED.contains(&c.model)));
    }

    #[test]
    fn the_projection_matches_the_camp_preview() {
        let (drop, depth, pitch) = projection(DEPTH);
        assert!((drop - crate::menu::preview::model_drop()).abs() < 0.01, "{drop}");
        assert!((depth - 702.0).abs() < 2.0 && pitch > 0.05 && pitch < 0.06, "{depth} {pitch}");
    }

    /// The model alpha objects of jmemo.2ld sequence 5 (skipped without `KH2_ISO`): group 7 fades 0 to 0x80 in 24 frames, group 8 holds
    /// 0x80), group 9 fades 0x80 to 0 in 12 frames; the header leave (129) lasts 12 frames, the viewer plate's groups 4 / 5 / 6 exist.
    #[test]
    fn the_disc_model_alpha_and_turn_groups() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let two = kh2_image::layd::parse_2ld(&a.read("menu/us/jmemo.2ld").unwrap()).unwrap();
        let s = &two.layd.seqs[5];
        assert_eq!((group_alpha(s, 7, 0), group_alpha(s, 7, 24), group_alpha(s, 8, 5)), (0.0, 1.0, 1.0));
        assert!((group_alpha(s, 7, 12) - 0.5).abs() < 0.01);
        assert_eq!((group_alpha(s, 9, 0), group_alpha(s, 9, 12)), (1.0, 0.0));
        assert!((group_alpha(s, 9, 6) - 0.5).abs() < 0.01);
        assert_eq!(s.anims[s.groups[129].first].frame_end, 12);
        for g in [4, 5, 6] {
            assert!(s.groups[g].count == 4, "viewer plate group {g}");
        }
    }
}
