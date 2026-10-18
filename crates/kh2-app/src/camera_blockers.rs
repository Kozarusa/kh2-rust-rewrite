//! The camera's dynamic blockers, filled from the objects' collision shapes of kind 0x12 (the
//! walk of `FUN_0016bce0` through `FUN_0015a3e8`; model and sources in `docs/camera.md`).
//!
//! The game rebuilds the list every time the camera asks: for each live object in its object
//! list, for each of its collision records of kind 0x12 (`FUN_0016b4e0(obj, 0x12)`), one entry
//! of 0x20 bytes (`FUN_001894d0`), at most 0x40 entries in all. The entry's radius is the short at
//! +0x10 of the record. Its centre depends on the record's bone number:
//! - a bone number with bit 0x4000 (the object's own frames, `FUN_0016aa18`): the record's offset
//!   (+8, +0xa, +0xc) in that frame, then moved by the short at +0xe (`FUN_001894d0`);
//! - any other bone number: only when the object's flag word +0x7f8 has bit 0x400 does the same
//!   happen with the bone's matrix. Otherwise the entry is written as radius 0 and centre
//!   (0, 0, 0). Nothing in the app sets that flag (its writer was not found, see the gaps in
//!   `docs/camera.md`), so such records give the empty entry. They still take a list slot, as in
//!   the game.

use bevy::prelude::*;

use crate::camera::rules::{Blocker, MAX_BLOCKERS};
use crate::camera::CamBlockers;
use crate::fx::{object_bone, shape_point, ObjectFrame};
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::SCALE;

/// The collision record kind the camera collects (`FUN_0016b4e0(obj, 0x12)`).
pub const KIND_CAMERA: u8 = 0x12;

/// The first bone number the object's own frames use (bit 0x4000 of the record's +6).
const OWN_FRAME: usize = 0x4000;

/// The blocker one record gives, in the camera's units (game units, y up). `owner` holds the
/// object's frames in Bevy space.
pub fn blocker_of(sh: &kh2_model::collision::Shape, owner: &ObjectFrame) -> Blocker {
    // `Shape::bone` is `None` for exactly 0x4000 and the number itself otherwise.
    let own = sh.bone.is_none_or(|b| b >= OWN_FRAME);
    if !own {
        return Blocker { center: Vec3::ZERO, radius: 0.0 };
    }
    let joint = object_bone(sh.bone.unwrap_or(OWN_FRAME) as u16, owner, &[], false);
    Blocker { center: shape_point(sh, joint) / SCALE, radius: sh.radius }
}

/// The list the game would build from `objects` (animated bodies with their world matrices), in
/// that order, cut at the list size.
pub fn collect<'a>(objects: impl IntoIterator<Item = (&'a Animator, Mat4)>) -> Vec<Blocker> {
    let mut list = Vec::new();
    'all: for (anim, body) in objects {
        let mut shapes = anim.rig.model.shapes.iter().filter(|s| s.kind == KIND_CAMERA).peekable();
        if shapes.peek().is_none() {
            continue;
        }
        let owner = ObjectFrame::of(anim, body);
        for sh in shapes {
            list.push(blocker_of(sh, &owner));
            if list.len() >= MAX_BLOCKERS {
                break 'all;
            }
        }
    }
    list
}

pub struct CameraBlockersPlugin;

impl Plugin for CameraBlockersPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, fill.after(Sim::EffectTick).run_if(crate::schedule::sim_running));
    }
}

/// Rebuilds `CamBlockers` for this tick (`FUN_0015a3e8`).
pub(crate) fn fill(mut blockers: ResMut<CamBlockers>, objects: Query<(&Animator, &GlobalTransform)>, mut last: Local<usize>) {
    blockers.0 = collect(objects.iter().map(|(a, g)| (a, g.to_matrix())));
    if blockers.0.len() != *last {
        *last = blockers.0.len();
        info!("camera: {} dynamic blockers (kind 0x12 collision records, FUN_0016bce0)", *last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kh2_model::collision::Shape;

    fn shape(bone: Option<usize>, radius: f32, lift: f32) -> Shape {
        Shape { group: 0, parts: 0, argument: 0, kind: KIND_CAMERA, shape: 3, bone, offset: [0.0; 3], lift, radius, height: 0.0 }
    }

    fn frame(at: Vec3) -> ObjectFrame {
        ObjectFrame { matrix: Mat4::from_translation(at), body_centre: None, point_9: None, blade: None }
    }

    /// A record on the object's own matrix (bone 0x4000) gives a sphere at the object, lifted by the
    /// record's +0xe, in game units (the camera's space), with the record's radius.
    #[test]
    fn own_frame_record_gives_a_sphere_at_the_object() {
        let b = blocker_of(&shape(None, 30.0, 12.0), &frame(Vec3::new(1.0, 0.0, 2.0)));
        assert!((b.center - Vec3::new(100.0, 12.0, 200.0)).length() < 1e-3, "{:?}", b.center);
        assert_eq!(b.radius, 30.0);
    }

    /// The numbered own frames follow `object_bone`: 0x4001 is the first type-1 shape's centre.
    #[test]
    fn body_frame_record_follows_the_body_centre() {
        let mut f = frame(Vec3::ZERO);
        f.body_centre = Some(Vec3::new(0.0, 0.5, 0.0));
        let b = blocker_of(&shape(Some(0x4001), 20.0, 0.0), &f);
        assert!((b.center - Vec3::new(0.0, 50.0, 0.0)).length() < 1e-3, "{:?}", b.center);
    }

    /// An ordinary bone without the object's flag 0x400 gives the empty entry (radius 0 at the origin).
    #[test]
    fn bone_record_gives_the_empty_entry() {
        let b = blocker_of(&shape(Some(7), 30.0, 0.0), &frame(Vec3::new(5.0, 0.0, 0.0)));
        assert_eq!(b, Blocker { center: Vec3::ZERO, radius: 0.0 });
    }

    /// An enemy between the wanted position and the look-at point pulls the end in to the sphere.
    #[test]
    fn a_blocker_between_look_and_camera_clips_the_segment() {
        let b = blocker_of(&shape(None, 30.0, 0.0), &frame(Vec3::new(0.5, 0.0, 0.0)));
        let end = crate::camera::rules::blockers_clip(&[b], Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0)).expect("clipped");
        assert!(end.x < 100.0 && end.x > 0.0, "{end:?}");
    }

    /// Real records: the boss `B_EX260` has one kind 0x12 sphere on its own matrix (radius 700, 600 in front);
    /// the NPC `N_DC010_RTN` one of radius 40 at its origin; `B_HE110` one on bone 2 (an ordinary bone).
    #[test]
    fn disc_records_give_the_spheres_the_models_hold() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = crate::assets::Disc::open(&iso);
        let at = Vec3::new(2.0, 0.0, 3.0);
        // The objects' matrices carry the world scale (the shapes' offsets are in game units).
        let mut owner = frame(Vec3::ZERO);
        owner.matrix = Mat4::from_scale_rotation_translation(Vec3::splat(SCALE), Quat::IDENTITY, at);
        let records = |name: &str| -> Vec<(Blocker, f32)> {
            let m = kh2_model::mdlx(&disc.read(&format!("obj/{name}.mdlx")).expect(name)).expect("model");
            m.shapes.iter().filter(|s| s.kind == KIND_CAMERA).map(|s| (blocker_of(s, &owner), s.lift)).collect()
        };
        let boss = records("B_EX260");
        assert_eq!(boss.len(), 1);
        assert_eq!(boss[0].0.radius, 700.0);
        assert!((boss[0].0.center - Vec3::new(200.0, boss[0].1, 300.0 + 600.0)).length() < 1.0, "{:?}", boss[0]);
        let npc = records("N_DC010_RTN");
        assert_eq!(npc.len(), 1);
        assert_eq!(npc[0].0.radius, 40.0);
        assert!((npc[0].0.center - Vec3::new(200.0, npc[0].1, 300.0)).length() < 1.0, "{:?}", npc[0]);
        assert_eq!(records("B_HE110")[0].0, Blocker { center: Vec3::ZERO, radius: 0.0 });
    }
}
