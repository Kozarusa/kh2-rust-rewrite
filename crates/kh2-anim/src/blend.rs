//! Cross-fade from the visible pose to a newly started motion (the motion controller's blend).
//!
//! The game's motion request (`FUN_0017e220` -> `FUN_0017e000` -> `FUN_0017df10` -> `FUN_0017dd38`)
//! takes a blend length `B` in controller time (one unit per 60 Hz tick: the controller step
//! `FUN_0017e3b8` adds the frame unit `DAT_00356f40` = 1.0 times the motion speed, controller +0xf4).
//!
//! * `B <= CUT_MAX` (`DAT_0037a58c`), or an object that has never been posed (controller +0x104 bit 0,
//!   set by the pose builder `FUN_0017e928`), is a cut: the new motion shows at once.
//! * Otherwise `FUN_0017dc80` stores `B` (+0x34) and the start frame (+0x38) and sets the
//!   controller time (+0x30) to `START` (`DAT_0037a588`). At the end of the request's tick
//!   `FUN_0017e6c0` snapshots two poses: the pose last drawn (+0xd0, a copy of the model's bone
//!   matrices, made relative to the previous motion's root) and the new motion at its start
//!   frame (+0xd4).
//! * While blending, the pose drawn (`FUN_0017e928` -> `FUN_0011c460`) mixes the two snapshots
//!   with the weight `time / B` (linear); the new motion itself is held at its start frame
//!   (`FUN_0017e3b8` hands +0x38, not +0x30, to the trigger and root code while +0x34 > 0).
//! * Each later tick adds the step to the time; when it reaches `B` the blend ends (+0x34 = 0)
//!   and the motion runs on from `start + (time - B)`.
//!
//! So a blend of 8 draws the weights 2/8, 3/8, .., 7/8 on the request tick and the five ticks
//! after it, and the motion's own start pose on the sixth tick.
//!
//! The game mixes the bone matrices row by row (`FUN_00125470`: the translation row and the two
//! first axis rows lerped, the axes re-normalised and re-orthogonalised by cross products).
//! [inferred equivalent] This port mixes decomposed bones: translation and scale lerped, rotation
//! slerped. Both are linear in the weight and agree at the ends; mid-blend they differ only by the
//! nlerp-versus-slerp speed profile of the rotation.

use glam::{Mat4, Quat, Vec3};

/// `DAT_0037a58c`: a requested blend at or below this is a cut (`FUN_0017dd38`).
pub const CUT_MAX: f32 = 2.0;
/// `DAT_0037a588`: the controller time a blend starts at (`FUN_0017dc80`).
pub const START: f32 = 2.0;

/// One bone of a pose, decomposed.
pub type Srt = (Vec3, Quat, Vec3);

/// The blend state of one animator (the controller's +0x30 / +0x34 and the source snapshot).
#[derive(Clone, Debug, Default)]
pub struct Blend {
    /// The source pose (the visible pose when the motion was requested), scale-rotation-translation.
    from: Vec<Srt>,
    /// Blend length `B` (+0x34); 0 when no blend runs.
    len: f32,
    /// Controller time while blending (+0x30).
    time: f32,
    /// True on the request tick: the time does not advance until the next tick (the controller
    /// step returns early while +0x20 bit 0 is set, cleared by `FUN_0017e6c0` at the end of the tick).
    fresh: bool,
}

impl Blend {
    /// Starts a blend of `len` ticks from `visible` (the pose drawn last; `None` when the object
    /// has never been posed). Returns false and stays idle for a cut.
    pub fn start(&mut self, visible: Option<&[Mat4]>, len: f32) -> bool {
        match visible {
            Some(pose) if len > CUT_MAX && !pose.is_empty() => {
                self.from = pose.iter().map(|m| m.to_scale_rotation_translation()).collect();
                self.len = len;
                self.time = START;
                self.fresh = true;
                true
            }
            _ => {
                self.stop();
                false
            }
        }
    }

    /// Ends any blend at once.
    pub fn stop(&mut self) {
        self.len = 0.0;
        self.time = 0.0;
        self.fresh = false;
        self.from.clear();
    }

    /// True while a blend runs (the controller's +0x34 > 0): the new motion is held at its start.
    pub fn active(&self) -> bool {
        self.len > 0.0
    }

    /// Mix weight of the new motion: `time / B` (0 = source pose, 1 = new motion); 1 when idle.
    pub fn weight(&self) -> f32 {
        if self.active() { (self.time / self.len).clamp(0.0, 1.0) } else { 1.0 }
    }

    /// One controller step of `dt` (the frame unit times the motion speed). Returns `Some(over)`
    /// on the step that ends the blend, `over` being the time past `B` the motion runs on by.
    pub fn advance(&mut self, dt: f32) -> Option<f32> {
        if !self.active() {
            return None;
        }
        if std::mem::take(&mut self.fresh) {
            return None;
        }
        self.time += dt;
        if self.time < self.len {
            return None;
        }
        let over = self.time - self.len;
        self.stop();
        Some(over)
    }

    /// Mixes `target` (the new motion's pose) with the source pose at the current weight, in place.
    pub fn apply(&self, target: &mut [Mat4]) {
        if !self.active() {
            return;
        }
        let w = self.weight();
        for (p, from) in target.iter_mut().zip(&self.from) {
            *p = mix(from, *p, w);
        }
    }
}

/// One bone: `from` towards `to` by `w` (translation and scale lerped, rotation slerped).
pub fn mix(from: &Srt, to: Mat4, w: f32) -> Mat4 {
    let (fs, fr, ft) = *from;
    let (ts, tr, tt) = to.to_scale_rotation_translation();
    Mat4::from_scale_rotation_translation(fs.lerp(ts, w), fr.slerp(tr, w), ft.lerp(tt, w))
}

/// Largest distance any bone origin moved between two poses (a debug measure of pose pops).
pub fn max_bone_delta(a: &[Mat4], b: &[Mat4]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x.w_axis - y.w_axis).truncate().length()).fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose(x: f32, angle: f32) -> Vec<Mat4> {
        vec![Mat4::from_rotation_translation(Quat::from_rotation_y(angle), Vec3::new(x, 0.0, 0.0)); 3]
    }

    /// Weights drawn on the request tick and the ticks after it (each tick steps, then draws).
    fn weights(b: &mut Blend, ticks: usize) -> Vec<f32> {
        (0..ticks).map(|_| {
            b.advance(1.0);
            b.weight()
        }).collect()
    }

    #[test]
    fn blend_of_8_is_linear_and_reaches_the_new_pose_at_time_8() {
        let mut b = Blend::default();
        assert!(b.start(Some(&pose(0.0, 0.0)), 8.0));
        let w = weights(&mut b, 8);
        // Request tick 2/8, then +1/8 per tick; time 8 (sixth tick after the request) ends it.
        assert_eq!(w, vec![0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0, 1.0]);
        assert!(!b.active());
    }

    #[test]
    fn the_ending_step_reports_the_overflow() {
        let mut b = Blend::default();
        b.start(Some(&pose(0.0, 0.0)), 8.0);
        assert_eq!(b.advance(1.0), None, "request tick holds");
        let mut ends = Vec::new();
        for _ in 0..4 {
            ends.push(b.advance(1.5));
        }
        // 2 -> 3.5 -> 5 -> 6.5 -> 8: ends with nothing left over.
        assert_eq!(ends, vec![None, None, None, Some(0.0)]);
        let mut b = Blend::default();
        b.start(Some(&pose(0.0, 0.0)), 7.5);
        b.advance(1.0);
        let over: Vec<_> = (0..6).filter_map(|_| b.advance(1.0)).collect();
        assert_eq!(over, vec![0.5]);
    }

    #[test]
    fn blends_of_2_or_less_and_unposed_objects_cut() {
        let mut b = Blend::default();
        for len in [0.0, 1.0, CUT_MAX] {
            assert!(!b.start(Some(&pose(0.0, 0.0)), len), "{len}");
            assert!(!b.active());
            assert_eq!(b.weight(), 1.0);
        }
        assert!(!b.start(None, 8.0), "never posed");
        assert!(b.start(Some(&pose(0.0, 0.0)), 2.5));
    }

    #[test]
    fn mix_ends_are_exact_and_midpoint_is_halfway() {
        let a = pose(0.0, 0.0);
        let mut t = pose(10.0, std::f32::consts::FRAC_PI_2);
        let mut b = Blend::default();
        b.start(Some(&a), 4.0); // weight 2/4 on the request tick
        b.apply(&mut t);
        let (_, r, tr) = t[0].to_scale_rotation_translation();
        assert!((tr.x - 5.0).abs() < 1e-4);
        assert!(r.angle_between(Quat::from_rotation_y(std::f32::consts::FRAC_PI_4)) < 1e-4);
        let from = a[0].to_scale_rotation_translation();
        let to = pose(10.0, 1.0)[0];
        assert!(mix(&from, to, 0.0).abs_diff_eq(a[0], 1e-5));
        assert!(mix(&from, to, 1.0).abs_diff_eq(to, 1e-5));
    }

    /// An interrupted blend starts from the pose last drawn: the first pose of the new blend is
    /// within one step of the previous drawn pose (no pop back to the old motion or the target).
    #[test]
    fn interrupted_blend_starts_from_the_visible_pose() {
        let idle = pose(0.0, 0.0);
        let run = pose(10.0, 1.0);
        let attack = pose(-20.0, -1.0);
        let mut b = Blend::default();
        b.start(Some(&idle), 8.0);
        let mut drawn = Vec::new();
        for _ in 0..3 {
            b.advance(1.0);
            let mut p = run.clone();
            b.apply(&mut p);
            drawn = p;
        }
        // Weight 4/8 was drawn last: halfway between idle and run.
        assert!((drawn[0].w_axis.x - 5.0).abs() < 1e-4);
        b.start(Some(&drawn), 8.0);
        b.advance(1.0);
        let mut p = attack.clone();
        b.apply(&mut p);
        // Request tick of the new blend: 2/8 of the way from the visible pose to the attack.
        let expect = 5.0 + (-20.0 - 5.0) * 0.25;
        assert!((p[0].w_axis.x - expect).abs() < 1e-3, "{}", p[0].w_axis.x);
        assert!(max_bone_delta(&p, &drawn) <= 25.0 * 0.25 + 1e-3);
    }
}
