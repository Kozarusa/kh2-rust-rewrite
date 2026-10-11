//! Skinned characters: KH2 bones → Bevy joint entities, GPU skinning, and motion playback.
//!
//! Each model bone becomes a joint entity under the character's `body`; every 60 Hz tick its
//! Transform is set to the bone's model-space matrix from the current motion (crossfaded on
//! transitions), and Bevy skins the meshes on the GPU against the rest pose.
//!
//! Motions advance in game ticks: a 30 fps motion moves half a frame per tick. Motions with
//! root motion report the root's per-tick movement (`root_delta`) for the character to apply,
//! and their poses are stored relative to the root.

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use kh2_anim::{Mset, Triggers};
use kh2_model::{Bone, Model};

use crate::assets::{material_for, texture_handles};

/// Shared, immutable character data.
pub struct Rig {
    pub model: Model,
    pub mset: Mset,
    /// The object's effect file (`obj/<name>.apdx`), for motion-triggered effects.
    pub apdx: Option<String>,
}

#[derive(Component)]
pub struct Animator {
    pub rig: Arc<Rig>,
    pub joints: Vec<Entity>,
    pub slot: Option<usize>,
    /// Current frame in the motion's local frame units.
    pub frame: f32,
    /// Frame at the previous update (for trigger edges).
    pub prev_frame: f32,
    /// True from a (re)start until its first tick, so frame triggers at the very first frame fire.
    fresh: bool,
    pub speed: f32,
    pub looping: bool,
    pub finished: bool,
    /// Model-space bone matrices of the current (blended) pose.
    pub pose: Vec<Mat4>,
    /// The cross-fade from the pose drawn when the motion was requested (`kh2_anim::blend`).
    blend: kh2_anim::blend::Blend,
    /// True once a motion pose has been drawn (the controller's +0x104 bit 0): before it every
    /// request is a cut.
    posed: bool,
    /// Current root pose of the motion (zero when it has no root motion).
    pub root: kh2_anim::motion::RootPose,
    /// Root movement over the last tick: model-space translation and yaw.
    pub root_delta: (Vec3, f32),
    /// The motion rig's work buffers (persist like the game's per-object buffers).
    rig_state: kh2_anim::rig::RigState,
    /// A motion from outside the rig's own set (`play_external`), played instead of `slot`.
    extra: Option<Arc<kh2_anim::Slot>>,
}

impl Animator {
    /// Starts a motion slot. `blend` is the crossfade length in ticks (60 Hz); 2 or less is a cut
    /// (`kh2_anim::blend`).
    pub fn play(&mut self, slot: Option<usize>, looping: bool, blend: f32) {
        if slot == self.slot && !self.finished && looping == self.looping && self.extra.is_none() {
            return;
        }
        self.restart(slot, looping, blend);
    }

    /// Like `play` but always restarts (e.g. the same attack twice).
    pub fn restart(&mut self, slot: Option<usize>, looping: bool, blend: f32) {
        self.extra = None;
        self.start(slot, looping, blend);
    }

    /// Plays a motion that is not in the rig's own set (a paired motion of a reaction command, held
    /// in the other object's motion set but made for this skeleton), once, crossfaded over `blend`
    /// ticks. `slot` reads `None` while it plays; any `play` or `restart` replaces it.
    pub fn play_external(&mut self, motion: Arc<kh2_anim::Slot>, blend: f32) {
        self.extra = Some(motion);
        self.start(None, false, blend);
    }

    fn start(&mut self, slot: Option<usize>, looping: bool, blend: f32) {
        // FUN_0017dd38: the cross-fade starts from the pose drawn last (an interrupted blend's
        // mixed pose included), or the request is a cut.
        self.blend.start(self.posed.then_some(self.pose.as_slice()), blend);
        self.slot = slot;
        self.looping = looping;
        self.finished = false;
        let start = self.motion().map_or(0.0, |m| m.frame_start);
        self.frame = start;
        self.prev_frame = start;
        self.fresh = true;
        self.root = self.root_at(start);
        self.root_delta = (Vec3::ZERO, 0.0);
    }

    fn root_at(&self, frame: f32) -> kh2_anim::motion::RootPose {
        match self.motion().map(|m| (m, &m.data)) {
            Some((m, kh2_anim::motion::Data::Interpolated(ip))) => ip.root_at(m.tick(frame)).unwrap_or_default(),
            _ => Default::default(),
        }
    }

    /// Advances one 60 Hz tick: frame, looping, and root movement.
    pub fn tick(&mut self) {
        // While a blend runs the new motion is held at its start frame (FUN_0017e3b8 hands the
        // start +0x38 to the trigger and root code while +0x34 > 0); the step that ends the blend
        // runs it on by the time left over.
        let mut step = self.speed;
        if self.blend.active() {
            match self.blend.advance(self.speed) {
                None => {
                    self.prev_frame = self.frame;
                    self.root_delta = (Vec3::ZERO, 0.0);
                    return;
                }
                Some(over) => step = over,
            }
        }
        // The first tick of a motion covers its first frame too (triggers at frame 0).
        self.prev_frame = if std::mem::take(&mut self.fresh) { self.frame - 1.0 } else { self.frame };
        let Some(m) = self.motion().cloned_meta() else {
            self.root_delta = (Vec3::ZERO, 0.0);
            return;
        };
        let before = self.root;
        let mut moved = (Vec3::ZERO, 0.0);
        self.frame += m.fps / crate::data::LOGIC_HZ * step;
        if self.frame >= m.end {
            if self.looping {
                // Root motion up to the end, then from the loop point.
                let end = self.root_at(m.end);
                moved.0 += end.translation - before.translation;
                moved.1 += crate::motor::wrap_angle(end.yaw - before.yaw);
                let span = (m.end - m.ret).max(1.0 / 64.0);
                self.frame = m.ret + (self.frame - m.end) % span;
                self.prev_frame = m.ret.min(self.prev_frame);
                let back = self.root_at(m.ret);
                self.root = back;
            } else {
                self.frame = m.end;
                self.finished = true;
            }
        }
        let now = self.root_at(self.frame);
        moved.0 += now.translation - self.root.translation;
        moved.1 += crate::motor::wrap_angle(now.yaw - self.root.yaw);
        self.root = now;
        self.root_delta = moved;
    }

    pub fn motion(&self) -> Option<&kh2_anim::Motion> {
        if let Some(x) = &self.extra {
            return x.motion.as_ref();
        }
        self.rig.mset.get(self.slot?)?.motion.as_ref()
    }

    pub fn triggers(&self) -> Option<&Triggers> {
        if let Some(x) = &self.extra {
            return Some(&x.triggers);
        }
        Some(&self.rig.mset.get(self.slot?)?.triggers)
    }

    /// Current time in global frames (60 Hz). Motion triggers are authored in global frames,
    /// while curves run at the motion's own rate (e.g. 30 fps): a 28-frame motion can carry a
    /// combo window starting at global frame 30.
    pub fn gframe(&self) -> f32 {
        self.motion().map_or(self.frame, |m| self.frame * crate::data::LOGIC_HZ / m.fps.max(1.0))
    }

    /// True while range trigger `id` covers the current frame.
    pub fn trigger(&self, id: u8) -> bool {
        self.triggers().is_some_and(|t| t.is_active(self.gframe(), id))
    }

    pub fn tag(&self) -> &str {
        self.extra.as_deref().or_else(|| self.slot.and_then(|s| self.rig.mset.get(s))).map_or("-", |s| s.tag.as_str())
    }

    /// Weight of the new motion in the drawn pose (1 when no blend runs).
    pub fn blend_weight(&self) -> f32 {
        self.blend.weight()
    }

    /// True while a cross-fade runs (the controller's +0x34 > 0, as `FUN_0017f400` tests it).
    pub fn blending(&self) -> bool {
        self.blend.active()
    }

    /// World-space matrix of a bone given the body's global transform.
    pub fn bone_world(&self, body: &GlobalTransform, bone: usize) -> Mat4 {
        body.to_matrix() * self.pose.get(bone).copied().unwrap_or(Mat4::IDENTITY)
    }
}

#[derive(Component)]
pub struct Body;

/// Spawns `rig` as a skinned character under `parent`. Returns the body entity.
#[allow(clippy::too_many_arguments)]
pub fn spawn_skinned(
    commands: &mut Commands,
    parent: Entity,
    rig: Arc<Rig>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
    scale: f32,
) -> Entity {
    let rest = kh2_model::rest_pose(&rig.model.bones);
    let body = commands.spawn((Body, Transform::from_scale(Vec3::splat(scale)), Visibility::default(), ChildOf(parent))).id();
    let joints: Vec<Entity> =
        rest.iter().map(|m| commands.spawn((Transform::from_matrix(*m), ChildOf(body))).id()).collect();
    let inverse: Vec<Mat4> = rest.iter().map(|m| m.inverse()).collect();
    let inv = bindposes.add(SkinnedMeshInverseBindposes::from(inverse));
    let textures = texture_handles(&rig.model, images);
    let mut used = Vec::new();
    for m in &rig.model.meshes {
        let mat = material_for(m, &rig.model, &textures, materials);
        used.push((m.texture, m.uv_scroll, mat.clone()));
        let mesh = meshes.add(skinned_mesh(m));
        commands.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(mat),
            Transform::default(),
            SkinnedMesh { inverse_bindposes: inv.clone(), joints: joints.clone() },
            ChildOf(body),
        ));
    }
    if let Some(anim) = crate::texanim::TexAnim::new(&rig.model, &textures, &used) {
        commands.entity(body).insert(anim);
    }
    commands.entity(body).insert(Animator {
        pose: rest,
        joints,
        slot: None,
        frame: 0.0,
        prev_frame: 0.0,
        fresh: false,
        speed: 1.0,
        looping: true,
        finished: false,
        blend: Default::default(),
        posed: false,
        root: Default::default(),
        root_delta: (Vec3::ZERO, 0.0),
        rig_state: Default::default(),
        extra: None,
        rig,
    });
    body
}

fn skinned_mesh(m: &kh2_model::Mesh) -> Mesh {
    // Weights must sum to 1 for skinning.
    let weights: Vec<[f32; 4]> = m
        .weights
        .iter()
        .map(|w| {
            let s: f32 = w.iter().sum();
            if s > 1e-6 { w.map(|x| x / s) } else { [1.0, 0.0, 0.0, 0.0] }
        })
        .collect();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(m.colors.clone()))
        .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(m.joints.clone()))
        .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights)
        .with_inserted_indices(Indices::U32(m.indices.clone()))
}

pub struct SkelPlugin;

impl Plugin for SkelPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, animate.in_set(crate::schedule::Sim::Pose));
    }
}

/// Advances every animator one tick and writes joint transforms (60 Hz fixed step).
pub fn animate(mut q: Query<&mut Animator>, mut joints: Query<&mut Transform>) {
    for mut a in &mut q {
        let a = &mut *a;
        if !step(a) {
            continue;
        }
        for (j, m) in a.joints.iter().zip(&a.pose) {
            if let Ok(mut t) = joints.get_mut(*j) {
                *t = Transform::from_matrix(*m);
            }
        }
    }
}

/// One tick of one animator: advances the motion and builds the drawn pose into `a.pose`
/// (false when no motion plays and the pose is left as it was).
fn step(a: &mut Animator) -> bool {
    a.tick();
    if a.motion().is_none() {
        return false;
    }
    let rig = a.rig.clone();
    let extra = a.extra.clone();
    let bones: &[Bone] = &rig.model.bones;
    let motion = match &extra {
        Some(x) => x.motion.as_ref(),
        None => a.slot.and_then(|s| rig.mset.get(s)).and_then(|s| s.motion.as_ref()),
    };
    let mut pose = motion.map(|mo| mo.pose_with(bones, a.frame, &mut a.rig_state)).unwrap_or_else(|| a.pose.clone());
    // Poses are kept relative to the motion's root: root motion moves the character
    // instead, and crossfades between motions stay continuous.
    if a.root != Default::default() {
        let undo = (Mat4::from_translation(a.root.translation) * Mat4::from_rotation_y(a.root.yaw)).inverse();
        for m in &mut pose {
            *m = undo * *m;
        }
    }
    // FUN_0017e928: while blending, the drawn pose mixes the pose seen at the request with
    // the new motion (held at its start) by time / blend.
    a.blend.apply(&mut pose);
    pose_log(a, &pose);
    a.pose = pose;
    a.posed = true;
    true
}

/// `KH2_POSELOG=<name>` (a substring of the rig's object name, e.g. `P_EX100`): one line per tick
/// for each matching animator with its motion, frame, blend weight and the largest distance a
/// bone origin moved since the last drawn pose (a pop shows as a spike).
fn pose_log(a: &Animator, pose: &[Mat4]) {
    static FILTER: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let Some(f) = FILTER.get_or_init(|| std::env::var("KH2_POSELOG").ok().filter(|s| !s.is_empty())) else { return };
    let name = a.rig.apdx.as_deref().unwrap_or("-");
    if !name.contains(f.as_str()) {
        return;
    }
    let d = kh2_anim::blend::max_bone_delta(&a.pose, pose);
    info!("pose: {name} {} frame {:.2} blend {:.3} max bone delta {d:.2}", a.tag(), a.frame, a.blend_weight());
}

struct Meta {
    fps: f32,
    end: f32,
    ret: f32,
}

trait MotionMeta {
    fn cloned_meta(self) -> Option<Meta>;
}

impl MotionMeta for Option<&kh2_anim::Motion> {
    fn cloned_meta(self) -> Option<Meta> {
        self.map(|m| Meta { fps: m.fps.max(1.0), end: m.frame_end, ret: m.frame_return.clamp(m.frame_start, m.frame_end) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pose as `animate` builds it: the motion at `frame`, relative to its root.
    fn root_relative_pose(m: &Model, mo: &kh2_anim::Motion, frame: f32) -> Vec<Mat4> {
        let mut st = kh2_anim::rig::RigState::default();
        let mut pose = mo.pose_with(&m.bones, frame, &mut st);
        let root = match &mo.data {
            kh2_anim::motion::Data::Interpolated(ip) => ip.root_at(mo.tick(frame)).unwrap_or_default(),
            _ => Default::default(),
        };
        let undo = (Mat4::from_translation(root.translation) * Mat4::from_rotation_y(root.yaw)).inverse();
        for p in &mut pose {
            *p = undo * *p;
        }
        pose
    }

    fn animator(rig: Arc<Rig>) -> Animator {
        Animator {
            pose: kh2_model::rest_pose(&rig.model.bones),
            joints: Vec::new(),
            slot: None,
            frame: 0.0,
            prev_frame: 0.0,
            fresh: false,
            speed: 1.0,
            looping: true,
            finished: false,
            blend: Default::default(),
            posed: false,
            root: Default::default(),
            root_delta: (Vec3::ZERO, 0.0),
            rig_state: Default::default(),
            extra: None,
            rig,
        }
    }

    /// G6: the motion controller's cross-fade through the one `Animator` path every actor uses, on
    /// Sora's idle (motion 0), run (2) and first ground attack (A300). The first request of a
    /// never-drawn object is a cut; idle to run with blend 8 (FUN_0017aeb8, `DAT_0037a4e4`) draws
    /// the weights 2/8 .. 7/8 with run held at its start frame, and shows run's own start pose on
    /// the sixth tick after the request; no tick moves a bone more than a quarter of the cut's
    /// jump. A request during the blend starts from the pose drawn last (no pop).
    #[test]
    fn idle_to_run_cross_fades_from_the_drawn_pose() {
        let Some(iso) = std::env::var("KH2_ISO").ok() else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let mut read = |p: &str| a.read(p).unwrap_or_else(|e| panic!("{p}: {e}"));
        let model = kh2_model::mdlx(&read("obj/P_EX100.mdlx")).unwrap();
        let mset = Mset::parse(&read("obj/P_EX100.mset")).unwrap();
        let state = kh2_anim::PlayerState::BattleWeapon;
        let (idle, run) = (mset.slot(kh2_anim::id::IDLE, state).unwrap(), mset.slot(kh2_anim::id::RUN, state).unwrap());
        let attack = mset.by_tag("A300").next().unwrap();
        let rig = Arc::new(Rig { model, mset, apdx: None });
        let start_pose = |slot: usize| {
            let mo = rig.mset.get(slot).and_then(|s| s.motion.as_ref()).unwrap();
            root_relative_pose(&rig.model, mo, mo.frame_start)
        };
        let mut an = animator(rig.clone());
        an.restart(Some(idle), true, 8.0);
        assert!(step(&mut an));
        assert_eq!(an.blend_weight(), 1.0, "the first request of a never-drawn object is a cut");
        for _ in 0..20 {
            step(&mut an);
        }
        let seen = an.pose.clone();
        let target = start_pose(run);
        let cut = kh2_anim::blend::max_bone_delta(&seen, &target);
        an.restart(Some(run), true, 8.0);
        let run_start = an.frame;
        let mut prev = seen.clone();
        let mut weights = Vec::new();
        let mut worst: f32 = 0.0;
        for k in 0..6 {
            step(&mut an);
            weights.push(an.blend_weight());
            assert_eq!(an.frame, run_start, "run held at its start while blending (tick {k})");
            worst = worst.max(kh2_anim::blend::max_bone_delta(&prev, &an.pose));
            prev = an.pose.clone();
        }
        assert_eq!(weights, vec![0.25, 0.375, 0.5, 0.625, 0.75, 0.875]);
        step(&mut an);
        assert!(!an.blending());
        worst = worst.max(kh2_anim::blend::max_bone_delta(&prev, &an.pose));
        let end = kh2_anim::blend::max_bone_delta(&an.pose, &target);
        eprintln!("idle->run: cut jump {cut:.2}, largest blended step {worst:.2}, off run's start pose at the end {end:.4}");
        assert!(end < 1e-3, "the blend ends on run's start pose");
        assert!(cut > 1.0 && worst <= cut * 0.25 + 1e-3, "cut {cut} worst {worst}");

        // Interrupted: two ticks into a new idle->run blend, the attack is requested.
        an.restart(Some(idle), true, 0.0);
        step(&mut an);
        an.restart(Some(run), true, 8.0);
        step(&mut an);
        step(&mut an);
        let seen = an.pose.clone();
        an.restart(Some(attack), false, 8.0);
        step(&mut an);
        let jump = kh2_anim::blend::max_bone_delta(&seen, &an.pose);
        let full = kh2_anim::blend::max_bone_delta(&seen, &start_pose(attack));
        eprintln!("interrupted: first step {jump:.2} of {full:.2} to the attack's start pose");
        assert!(jump <= full * 0.25 + 1e-3, "jump {jump} full {full}");
    }

    /// A343 (slot 776), "the keyblade shaft missing at motion frames 18 to 20" (WP-5A): skinned on the
    /// CPU exactly as the GPU does it (joint pose times inverse rest pose, weights normalised), on
    /// Sora's hand bone 178 (both poses equal the game's evaluator there, `kh2 rig`), the blade
    /// (mesh 0) is whole at every frame: about 100 units long, above the floor, and from frame 18 to
    /// 20 it points forward (+z, Sora's facing) and down from his hip. Seen from the default camera
    /// behind Sora it is behind his body; seen from the side or the front it is drawn (screenshots).
    #[test]
    fn a343_keyblade_is_whole_and_in_front_of_sora() {
        let Some(iso) = std::env::var("KH2_ISO").ok() else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let mut read = |p: &str| a.read(p).unwrap_or_else(|e| panic!("{p}: {e}"));
        let body = kh2_model::mdlx(&read("obj/P_EX100.mdlx")).unwrap();
        let bset = Mset::parse(&read("obj/P_EX100.mset")).unwrap();
        let blade = kh2_model::mdlx(&read("obj/W_EX010.mdlx")).unwrap();
        let wset = Mset::parse(&read("obj/W_EX010.mset")).unwrap();
        let bmo = bset.get(776).and_then(|s| s.motion.as_ref()).unwrap();
        let wmo = wset.get(776).and_then(|s| s.motion.as_ref()).unwrap();
        let inv: Vec<Mat4> = kh2_model::rest_pose(&blade.bones).iter().map(|m| m.inverse()).collect();
        let mesh = &blade.meshes[0];
        for f in [16.0f32, 17.0, 18.0, 19.0, 20.0, 21.0, 22.0] {
            let hand = root_relative_pose(&body, bmo, f)[178];
            let wp = root_relative_pose(&blade, wmo, f);
            let mut bb = (Vec3::MAX, Vec3::MIN);
            for (i, p) in mesh.positions.iter().enumerate() {
                let w = mesh.weights[i];
                let s: f32 = w.iter().sum();
                let mut v = Vec3::ZERO;
                for k in 0..4 {
                    let wk = if s > 1e-6 { w[k] / s } else if k == 0 { 1.0 } else { 0.0 };
                    let j = mesh.joints[i][k] as usize;
                    v += wk * (hand * wp[j] * inv[j]).transform_point3(Vec3::from(*p));
                }
                assert!(v.is_finite());
                bb = (bb.0.min(v), bb.1.max(v));
            }
            let size = (bb.1 - bb.0).length();
            eprintln!("A343 frame {f}: hand ({:.1} {:.1} {:.1}), blade {:.0?}..{:.0?}, extent {size:.0}", hand.w_axis.x, hand.w_axis.y, hand.w_axis.z, bb.0, bb.1);
            assert!(size > 90.0, "blade collapsed at frame {f}");
            assert!(bb.0.y > 0.0, "blade under the floor at frame {f}");
            if (18.0..=20.0).contains(&f) {
                assert!(bb.1.z > 100.0, "blade not in front at frame {f}");
            }
        }
        // Frame 19 by hand: the hand at the hip, in front of the body.
        let hand = root_relative_pose(&body, bmo, 19.0)[178].w_axis;
        assert!((hand.y - 79.2).abs() < 0.5 && (hand.z - 55.4).abs() < 0.5, "{hand:?}");
    }

    /// WP-F2 item B (user report: the keyblade's faces look inverted). The weapon socket takes the
    /// hand bone's full pose matrix (`sora::follow_hand`); a negative determinant there would mirror
    /// the blade. Checked on the disc: no bone of P_EX100, W_EX010, P_EX020 (Donald) or P_EX030
    /// (Goofy) has a negative bind scale, and the hand bones 178 and 86 keep a positive determinant
    /// through the rest pose and the motions A000, A300, A330, A343 and the first drive-form slot.
    #[test]
    fn hand_bone_poses_never_mirror_the_weapon() {
        let Some(iso) = std::env::var("KH2_ISO").ok() else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let mut read = |p: &str| a.read(p).unwrap_or_else(|e| panic!("{p}: {e}"));
        for name in ["P_EX100", "W_EX010", "P_EX020", "P_EX030"] {
            let m = kh2_model::mdlx(&read(&format!("obj/{name}.mdlx"))).unwrap();
            let negative: Vec<(usize, Vec3)> = m.bones.iter().enumerate().filter(|(_, b)| b.scale.min_element() < 0.0).map(|(i, b)| (i, b.scale)).collect();
            eprintln!("{name}: {} bones, negative bind scales: {negative:?}", m.bones.len());
            assert!(negative.is_empty(), "{name}: bones with a negative bind scale: {negative:?}");
            let rest = kh2_model::rest_pose(&m.bones);
            for (i, p) in rest.iter().enumerate() {
                assert!(p.determinant() > 0.0, "{name}: rest pose of bone {i} has determinant {}", p.determinant());
            }
        }
        let body = kh2_model::mdlx(&read("obj/P_EX100.mdlx")).unwrap();
        let bset = Mset::parse(&read("obj/P_EX100.mset")).unwrap();
        for tag in ["A000", "A300", "A330", "A343"] {
            let Some(slot) = bset.by_tag(tag).next() else { panic!("{tag} missing") };
            let mo = bset.get(slot).and_then(|s| s.motion.as_ref()).unwrap();
            let mut st = kh2_anim::rig::RigState::default();
            let mut worst = f32::MAX;
            let mut f = mo.frame_start;
            while f <= mo.frame_end {
                let pose = mo.pose_with(&body.bones, f, &mut st);
                for bone in super::super::sora::HAND_BONES {
                    let d = pose[bone].determinant();
                    worst = worst.min(d);
                    assert!(d > 0.0, "{tag} slot {slot} frame {f}: hand bone {bone} determinant {d}");
                }
                f += 1.0;
            }
            eprintln!("{tag} (slot {slot}): hand bones 178 and 86 keep a positive determinant, smallest {worst:.3}");
        }
    }
}
