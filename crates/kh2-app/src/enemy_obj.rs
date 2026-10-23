//! The object a BDX enemy script sees (`Obj`, its handle constants and the game-space conversions) lives in
//! `shared::enemy_obj` so that the lower crates can name it; this module re-exports it and keeps its tests.

pub use crate::shared::enemy_obj::*;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use crate::motor::wrap_angle;

    #[test]
    fn game_space_is_the_apps_rotated_half_a_turn_about_x() {
        let p = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(to_game(p), [1.0, -2.0, -3.0, 1.0]);
        assert_eq!(from_game([1.0, -2.0, -3.0, 0.0]), p);
    }

    #[test]
    fn yaw_of_a_script_direction_is_the_apps_facing() {
        // The app's facing for yaw a is (sin a, 0, cos a); the script sees it as (sin a, 0, -cos a).
        for k in -7..8 {
            let a = k as f32 * 0.4;
            let d = to_game(Vec3::new(a.sin(), 0.0, a.cos()) * 300.0);
            assert!(wrap_angle(yaw_of(d) - a).abs() < 1e-4, "{a}");
        }
        // atan2 of nothing is 0 in the game's space.
        assert!(wrap_angle(yaw_of([0.0; 4]) - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn a_fade_runs_its_length_and_stops() {
        let mut o = Obj::new(SELF_BASE);
        o.alpha = 0.0;
        o.fade = Some(Fade { from: 0.0, to: 1.0, frames: 30.0, t: 0.0 });
        for _ in 0..15 {
            o.advance_fade();
        }
        assert!((o.alpha - 0.5).abs() < 1e-5);
        for _ in 0..15 {
            o.advance_fade();
        }
        assert_eq!(o.alpha, 1.0);
        assert!(o.fade.is_none());
    }

    /// A clash hands two volume objects to the script; word +8 of each is its owner.
    #[test]
    fn clash_volumes_hold_their_owners() {
        let mut o = Obj::new(SELF_BASE);
        let (own, other) = o.new_volumes(5, 7);
        let w = |o: &Obj, a: u32| {
            let mut b = [0u8; 4];
            o.volume_word(a, &mut b).then(|| u32::from_le_bytes(b))
        };
        assert_eq!(w(&o, own + VOLUME_OWNER), Some(SELF_BASE));
        assert_eq!(w(&o, other + VOLUME_OWNER), Some(PLAYER));
        assert_eq!(w(&o, own), None, "other fields are not served");
    }

    /// Host 1.125 (FUN_00173858 -> FUN_001abf40) starts the ramp at +0x8c8 that FUN_00192bf0 steps once per frame.
    #[test]
    fn the_tween_started_by_host_1_125_runs_to_its_target_and_stops() {
        // FUN_001abf40(0, 1, 4): from 0 to 1 over four frames, stepped once per frame by advance_fade.
        let mut o = Obj::new(SELF_BASE);
        o.tween = Ramp::start(0.0, 1.0, 4.0);
        for _ in 0..2 {
            o.advance_fade();
        }
        assert!((o.tween.value - 0.5).abs() < 1e-6);
        for _ in 0..10 {
            o.advance_fade();
        }
        assert_eq!(o.tween, Ramp { value: 1.0, step: 0.0 });
    }

    /// FUN_001abf40 / FUN_001abf78 with the executable's appear numbers (0 to 1.0 over 10 frames, the limit 1.0): the
    /// ramp starts at 0, gains a tenth per frame, stops at 1.0 and then has no step; no time at all is the target at once.
    #[test]
    fn the_appear_ramp_runs_ten_frames_and_stops() {
        let mut r = Ramp::start(0.0, 1.0, 10.0);
        assert_eq!(r, Ramp { value: 0.0, step: 0.1 });
        for k in 1..=9 {
            assert!(r.tick(1.0));
            assert!((r.value - 0.1 * k as f32).abs() < 1e-5, "{k}: {}", r.value);
        }
        assert!(r.tick(1.0));
        assert_eq!(r, Ramp { value: 1.0, step: 0.0 }, "the tenth step reaches the limit");
        assert!(!r.tick(1.0), "no step is pending");
        assert_eq!(Ramp::start(0.0, 1.0, 0.0), Ramp { value: 1.0, step: 0.0 });
        assert_eq!(Ramp::start(0.5, 0.0, -3.0), Ramp { value: 0.0, step: 0.0 });
        // A falling ramp stops at 0 (FUN_001abf78).
        let mut f = Ramp::start(1.0, 0.0, 2.0);
        assert!(f.tick(1.0) && f.tick(1.0));
        assert_eq!(f, Ramp { value: 0.0, step: 0.0 });
        // The draw alpha is the script's fade times the ramp (FUN_00193270).
        let mut o = Obj::new(SELF_BASE);
        o.alpha = 0.5;
        o.appear = Ramp { value: 0.4, step: 0.0 };
        assert!((o.shown_alpha() - 0.2).abs() < 1e-6);
    }

    /// Host 1.0 hands out the serial after the last one used (`ai_attach` counts up before it uses a serial), so the
    /// handles of made objects and of spawned ones never meet; the made object is known to its maker, with the origin
    /// until a `set_position` (1.148) moves it.
    #[test]
    fn made_objects_get_their_own_handles_and_stand_at_the_origin() {
        let mut o = Obj::new(handle_of_serial(5));
        o.next_serial = 7;
        let a = o.create(0x6bf);
        let b = o.create(0x6bf);
        assert_eq!((a, b), (handle_of_serial(8), handle_of_serial(9)));
        assert_eq!(o.next_serial, 9);
        assert!(o.knows(a) && o.knows(b) && !o.knows(handle_of_serial(10)));
        assert_eq!(o.made_at(a), Vec3::ZERO);
        o.remote.push((a, Remote::Position(Vec3::new(1.0, 2.0, 3.0))));
        o.remote.push((b, Remote::Hide));
        o.remote.push((a, Remote::Position(Vec3::new(4.0, 5.0, 6.0))));
        assert_eq!(o.made_at(a), Vec3::new(4.0, 5.0, 6.0), "the last point");
        assert_eq!(o.made_at(b), Vec3::ZERO);
        assert_eq!(handle_of_serial(0x1001), handle_of_serial(1), "the handles wrap after 0x1000 objects");
    }

    /// Host 1.8 on the context of an object this one made (WP-EN4): queued for that object with the mode's name; on this
    /// object itself or on a handle nobody knows nothing is queued. `ai_remote` hands the queue to the owner, whose
    /// `modes_asked` the script takes at the start of its tick.
    #[test]
    fn a_mode_asked_of_a_made_object_is_queued_for_it() {
        let mut o = Obj::new(handle_of_serial(5));
        o.next_serial = 7;
        let a = o.create(0x6bf);
        assert!(o.ask_mode(a, "appear"));
        assert_eq!(o.remote, vec![(a, Remote::Mode("appear".to_string()))]);
        assert!(!o.ask_mode(o.handle, "leave"), "its own context is the runtime's plain 1.8");
        assert!(!o.ask_mode(handle_of_serial(99), "leave"), "an object nobody knows");
        assert_eq!(o.remote.len(), 1);
    }

    #[test]
    fn the_movement_vector_is_horizontal_and_in_the_apps_axes() {
        let mut o = Obj::new(SELF_BASE);
        o.set_move([3.0, 0.0, -3.0, 0.0], 8.0);
        let want = Vec3::new(1.0, 0.0, 1.0).normalize() * 8.0;
        assert!((o.move_vel - want).length() < 1e-4, "{:?}", o.move_vel);
    }

    /// Host 2.64 / 2.85 answer from the rows `note_hit_row` was given: the first note of an id stays, an unseen id has no answer.
    #[test]
    fn hit_rows_are_kept_by_attack_id() {
        note_hit_row(0x7ff0, 7, 5);
        note_hit_row(0x7ff0, 3, 1);
        assert_eq!(hit_row_bytes(0x7ff0), Some((7, 5)));
        assert_eq!(hit_row_bytes(0x7ff1), None);
    }

    /// Hosts 0.49 / 0.50 / 0.51: a handle the object made reads live until the script destroys it; a handle it never made (0, another range) never does.
    #[test]
    fn an_effect_handle_is_live_until_destroyed() {
        let mut o = Obj::new(SELF_BASE);
        o.next_effect = 2;
        let (a, b) = (EFFECT + 1, EFFECT + 2);
        assert!(o.effect_live(a) && o.effect_live(b));
        o.destroy_effect(a);
        o.destroy_effect(a);
        assert!(!o.effect_live(a), "destroyed");
        assert!(o.effect_live(b), "the other handle is untouched");
        assert_eq!(o.effects_gone, vec![a], "destroying twice keeps one record");
        assert!(!o.effect_live(0) && !o.effect_live(EFFECT) && !o.effect_live(EFFECT + 3), "handles the object never made");
    }
}
