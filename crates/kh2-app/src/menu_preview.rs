//! The party standing in the camp menu's top page: the 3D preview object (docs/menus.md 16.6, 16.8; WP-MN1).
//!
//! The original (`SLUS_210.05`): `FUN_00295720` creates the object `DAT_0036d1f8` (`FUN_0023a018`, vtable 0x36ca50)
//! when the camp scene starts (`FUN_0024ef30` passes 4 as its mode). Page 0's build `FUN_00250a00` fills one slot per
//! party member (up to three, `FUN_00282940`) through the vtable's +0x18, `FUN_0023a520(slot, object | 0x20000000,
//! motion 0x8b, 2, x, y)`, with x = 0x2f, 0x91, 0xf3 for slots 0..2 minus `(n * 98 - 98) / n` and y = 0x60 (0x3d in the
//! world whose save byte `+0xc` is 10, `FUN_0027c218`). Each slot has a camera of its own (`FUN_0023a848`: the projection
//! of `FUN_00106ba0(512, 0.8, 0.866667, x + 2048, y + 2048, ...)`, horizontal field of view 0x37f49c = 40 degrees through
//! `FUN_00106c48`; the field camera's centre is (2048, 2048), `FUN_001078f0`), so the slot's picture is centred `x`, `y`
//! from the screen's middle. The view (`FUN_0023a520`): eye (0, -50, -800) (0x37f468, 0x37f46c), look-at (0, 0, 55)
//! (0x37f470), up (0, -1, 0) (0x37f474). The model stands at (0, 0, -100) (`FUN_0023b348(0x3806f8 = -100)` writes the
//! slot's translation z, `FUN_0023a920` builds the matrix from it), unrotated (`FUN_0023b3a8(0)`, `FUN_0023b3c8(0)`),
//! scale 1 (0x37f478), and plays motion 0x8b of its set (`FUN_001a7a48(set, 0x8b, 2)`: entry 0x8b * 4 + 2, else +3, +2,
//! +1, +0; a non-player set: entry 0x8b) looping, one frame per tick (`FUN_0023a920` adds `FUN_0014cbe8`, the frame step).
//! The colour is (0x60, 0x60, 0x60) of 0x80 (`FUN_0023b448`), the alpha follows the panel outline object's
//! (`FUN_0023b4e0` reads `DAT_0036ceac`'s colour, groups 0x2e/0x2f/0x30).
//!
//! The port renders the members with a camera of its own into a picture of the menu canvas (render layer 9, only while
//! the menu shows page 0), drawn by the menu as one node right after the panel outline: the field behind keeps its dim
//! and scan lines, the models stand in front of them and behind the rows and cards. One perspective (the original's
//! vertical focal length, centred on the canvas middle) stands in for the camera per slot: each model is shifted
//! sideways to its slot centre instead of moving the projection centre [inferred: exact at the model's own depth].

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

use super::{Menu, Phase};
use crate::menu_gfx::{MenuScene, Pic, Tex};
use crate::skel::{Animator, Rig};

pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Preview>()
            .add_systems(Startup, setup.after(super::load))
            .add_systems(Update, (show, animate, layer_and_tint).chain().after(super::tick).before(crate::schedule::Frame::Popups));
    }
}

/// The slot camera of `FUN_0023a520` (KH2 space, y down).
const EYE: Vec3 = Vec3::new(0.0, -50.0, -800.0);
const LOOK_AT: Vec3 = Vec3::new(0.0, 0.0, 55.0);
/// The model's place (`FUN_0023b348`, 0x3806f8).
const MODEL_AT: Vec3 = Vec3::new(0.0, 0.0, -100.0);
/// The slot centre's vertical offset from the screen's middle (`FUN_00250a00`: 0x60, 0x3d in world 10).
const SLOT_Y: [f32; 2] = [96.0, 61.0];
/// Motion of the preview (`FUN_00250a00` passes 0x8b) and the variant `FUN_001a7a48` tries first.
pub const MOTION: usize = 0x8b;
const VARIANT: usize = 2;
/// The model colour 0x60 of 0x80 (`FUN_0023b448`).
const TINT: f32 = 0x60 as f32 / 128.0;
/// Render layer of the preview camera and its models (the main camera draws layer 0, the HUD 7, the menu 8).
const PREVIEW_LAYER: usize = 9;
/// Where the preview's private stage stands (Bevy units), far below any area, so the main camera never sees a model
/// in the frame before its meshes get the layer.
const STAGE: Vec3 = Vec3::new(0.0, -10_000.0, 0.0);
/// The picture's size: the menu canvas (512 x 448 units) at twice the resolution.
const TARGET: [u32; 2] = [1024, 896];

/// The original's projection in menu units: (focal length x, focal length y). `FUN_00106c48(40 degrees)` with the
/// 0.8 / 0.866667 scales: the horizontal focal length is 256 / tan(20 degrees) buffer pixels and the vertical one
/// 208 * f / 240 with f = 320 / tan(20 degrees) (the 512 x 416 buffer shown as 4:3).
pub fn focal() -> (f32, f32) {
    let f640 = (512.0 / 0.8 * 0.5) / (0.698_132_f32 / 2.0).tan();
    (256.0 / (0.698_132_f32 / 2.0).tan(), 208.0 * f640 / (416.0 / 0.866_667 * 0.5))
}

/// The slot centre's x offset from the screen's middle (`FUN_00250a00`).
pub fn slot_x(slot: usize, count: usize) -> f32 {
    let n = count.clamp(1, 3) as i32;
    let base = super::at::CARD_BASE.get(slot).copied().unwrap_or(0) as f32;
    base - ((n * 98 - 98) / n) as f32
}

/// Where the model's origin lands, in menu units, its depth in the slot camera, and the camera's pitch (radians, the
/// view looks down by it).
pub fn origin_on_screen(slot: usize, count: usize, world10: bool) -> (Vec2, f32, f32) {
    let d = LOOK_AT - EYE;
    let pitch = d.y.atan2(d.z);
    let rel = MODEL_AT - EYE;
    // Camera axes in the y-z plane (y down): forward (0, sin, cos), down (0, cos, -sin).
    let (s, c) = pitch.sin_cos();
    let z = rel.y * s + rel.z * c;
    let y = rel.y * c - rel.z * s;
    let (_, fy) = focal();
    let cy = SLOT_Y[usize::from(world10)];
    (Vec2::new(256.0 + slot_x(slot, count), 208.0 + cy + fy * y / z), z, pitch)
}

/// The motion entry `FUN_001a7a48(set, 0x8b, 2)` picks.
pub fn motion_slot(rig: &Rig) -> Option<usize> {
    let player = rig.mset.slots.len() > MOTION * 4;
    if player {
        [VARIANT, 3, 2, 1, 0].into_iter().map(|v| MOTION * 4 + v).find(|&s| rig.mset.has(s))
    } else {
        Some(MOTION).filter(|&s| rig.mset.has(s))
    }
}

/// Marks the preview's own `Camera3d`. Every system that looks for THE game camera filters `Without<PreviewCamera>`:
/// with two 3D cameras alive `Query::single()` would fail (the effects' view fell back to identity, the HUD lost its projection).
#[derive(Component)]
pub struct PreviewCamera;

/// The filter of every query for THE game camera (the preview's own `Camera3d` is left out).
pub type GameCamera = (With<Camera3d>, Without<PreviewCamera>);

#[derive(Component)]
struct PreviewModel {
    body: Entity,
    state: kh2_anim::rig::RigState,
    done: bool,
}

#[derive(Resource, Default)]
struct Preview {
    target: Handle<Image>,
    camera: Option<Entity>,
    roots: Vec<Entity>,
}

/// The picture the preview camera draws into, as a texture of the menu, and the page-0 node that shows it.
fn setup(mut preview: ResMut<Preview>, mut images: ResMut<Assets<Image>>, mut scene: ResMut<MenuScene>, menu: Res<Menu>) {
    if !menu.loaded {
        return;
    }
    let size = Extent3d { width: TARGET[0], height: TARGET[1], depth_or_array_layers: 1 };
    let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Rgba8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let h = images.add(image);
    preview.target = h.clone();
    scene.texs.push(Tex { width: TARGET[0], height: TARGET[1], linear: h.clone(), nearest: h });
    let tex = scene.texs.len() - 1;
    let y = crate::menu_gfx::Y_BIAS;
    let node = &mut scene.nodes[menu.nodes.preview];
    node.pic = Some(Pic { tex, rect: [0.0, -y, 512.0, 448.0 - y], v_end: 1.0 });
    node.visible = false;
}

/// The rigs of the party in slot order: Sora, then the members in the field by their party slot.
fn party_rigs(sora: &Query<&crate::sora::Sora>, members: &Query<&crate::party::PartyMember>, anims: &Query<&Animator>) -> Vec<Arc<Rig>> {
    let mut out = Vec::new();
    if let Some(a) = sora.single().ok().and_then(|s| anims.get(s.body).ok()) {
        out.push(a.rig.clone());
    }
    let mut m: Vec<&crate::party::PartyMember> = members.iter().collect();
    m.sort_by_key(|m| m.slot);
    for p in m {
        if let Ok(a) = anims.get(p.body) {
            out.push(a.rig.clone());
        }
    }
    out.truncate(3);
    out
}

#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    menu: Res<Menu>,
    mut preview: ResMut<Preview>,
    mut scene: ResMut<MenuScene>,
    sora: Query<&crate::sora::Sora>,
    members: Query<&crate::party::PartyMember>,
    anims: Query<&Animator>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    if !menu.loaded || scene.nodes.get(menu.nodes.preview).is_none_or(|n| n.pic.is_none()) {
        return;
    }
    let want = matches!(menu.phase, Phase::Opening | Phase::Open) && menu.page == 0 && std::env::var_os("KH2_NO_MENU_PREVIEW").is_none();
    if !want {
        if let Some(c) = preview.camera.take() {
            // The camera exists only while the preview shows (other systems expect a single 3D camera).
            commands.entity(c).despawn();
            for e in preview.roots.drain(..) {
                commands.entity(e).despawn();
            }
            scene.nodes[menu.nodes.preview].visible = false;
            info!("menu: party preview removed");
        }
        return;
    }
    if preview.camera.is_some() {
        return;
    }
    let (fx, fy) = focal();
    let fov = 2.0 * (224.0 / fy).atan();
    let camera = commands
        .spawn((
            Camera3d::default(),
            PreviewCamera,
            Camera { order: -1, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(preview.target.clone().into()),
            Projection::Perspective(PerspectiveProjection { fov, near: 0.01, far: 100.0, ..default() }),
            Transform::from_translation(STAGE),
            RenderLayers::layer(PREVIEW_LAYER),
        ))
        .id();
    preview.camera = Some(camera);
    scene.nodes[menu.nodes.preview].visible = true;
    let rigs = party_rigs(&sora, &members, &anims);
    let n = rigs.len();
    let yaw = std::env::var("KH2_PREVIEW_YAW").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0).to_radians();
    for (slot, rig) in rigs.into_iter().enumerate() {
        let (at, z, pitch) = origin_on_screen(slot, n, false);
        // KH2 units to Bevy (0.01); the picture is the canvas, its middle the canvas middle (256, 208).
        let depth = z * crate::SCALE;
        let local = Vec3::new((at.x - 256.0) * depth / fy, -(at.y - 208.0) * depth / fy, -depth);
        let rot = Quat::from_rotation_x(pitch) * Quat::from_rotation_y(yaw);
        let scale = Vec3::new(crate::SCALE * fx / fy, crate::SCALE, crate::SCALE);
        let root = commands.spawn((Transform { translation: STAGE + local, rotation: rot, scale }, Visibility::default(), RenderLayers::layer(PREVIEW_LAYER))).id();
        let body = crate::skel::spawn_skinned(&mut commands, root, rig.clone(), &mut meshes, &mut materials, &mut images, &mut bindposes, 1.0);
        let m = motion_slot(&rig);
        commands.entity(root).insert(PreviewModel { body, state: Default::default(), done: false });
        info!("menu: party preview slot {slot} of {n}: {} motion {:?} at menu ({:.0}, {:.0}), depth {z:.0}", rig.apdx.as_deref().unwrap_or("-"), m, at.x, at.y);
        commands.queue(move |world: &mut World| {
            if let Some(mut a) = world.get_mut::<Animator>(body) {
                a.restart(m, true, 0.0);
            }
        });
        preview.roots.push(root);
    }
}

/// One 60 Hz step of each preview model's motion while the simulation (and with it `skel::animate`) is stopped.
fn animate(mut q: Query<&mut PreviewModel>, mut anims: Query<&mut Animator>, mut joints: Query<&mut Transform>, real: Res<Time<Real>>, mut acc: Local<f32>) {
    *acc = (*acc + real.delta_secs()).min(0.25);
    let mut ticks = 0;
    while *acc >= 1.0 / 60.0 {
        *acc -= 1.0 / 60.0;
        ticks += 1;
    }
    for mut p in &mut q {
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
        for (j, m) in a.joints.iter().zip(&pose) {
            if let Ok(mut t) = joints.get_mut(*j) {
                *t = Transform::from_matrix(*m);
            }
        }
        a.pose = pose;
    }
}

/// Once a model's meshes exist: put them on the preview's layer and give them the model colour (0x60 of 0x80) on
/// their own materials.
fn layer_and_tint(
    mut commands: Commands,
    mut q: Query<&mut PreviewModel>,
    children: Query<&Children>,
    handles: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for mut p in &mut q {
        if p.done {
            continue;
        }
        let Ok(kids) = children.get(p.body) else { continue };
        let mut any = false;
        for k in kids.iter() {
            commands.entity(k).insert(RenderLayers::layer(PREVIEW_LAYER));
            let Ok(h) = handles.get(k) else { continue };
            if let Some(mut m) = materials.get_mut(&h.0) {
                let c = m.base_color.to_srgba();
                m.base_color = Color::srgba(c.red * TINT, c.green * TINT, c.blue * TINT, c.alpha);
                any = true;
            }
        }
        commands.entity(p.body).insert(RenderLayers::layer(PREVIEW_LAYER));
        p.done = any;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The slot centres and the focal lengths follow the constants of `FUN_00250a00`, `FUN_0023a520` and `FUN_00106c48`.
    #[test]
    fn the_slots_sit_where_the_original_puts_them() {
        assert_eq!([slot_x(0, 3), slot_x(1, 3), slot_x(2, 3)], [-18.0, 80.0, 178.0]);
        assert_eq!(slot_x(0, 1), 47.0);
        let (fx, fy) = focal();
        assert!((fx - 703.4).abs() < 0.5 && (fy - 762.0).abs() < 0.5, "{fx} {fy}");
        let (at, z, pitch) = origin_on_screen(0, 3, false);
        // The model stands 100 units in front of the look-at plane: its feet land a little under the slot centre.
        assert!((z - 702.0).abs() < 2.0 && pitch > 0.05 && pitch < 0.06, "{z} {pitch}");
        assert!(at.y > 304.0 && at.y < 316.0, "{at:?}");
        assert_eq!(at.x, 238.0);
    }

    /// With the preview camera alive the game camera is still the only match of the queries the other systems use.
    #[test]
    fn main_camera_queries_resolve_with_the_preview_camera_present() {
        let mut world = World::new();
        let main = world.spawn((Camera3d::default(), Transform::default())).id();
        world.spawn((Camera3d::default(), PreviewCamera));
        let mut q = world.query_filtered::<Entity, (With<Camera3d>, Without<PreviewCamera>)>();
        assert_eq!(q.single(&world).unwrap(), main);
        let mut all = world.query_filtered::<Entity, With<Camera3d>>();
        assert!(all.single(&world).is_err(), "the unfiltered query is ambiguous, which is the bug");
    }
}
