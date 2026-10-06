//! Positional volume and pan of a sound effect started at a world position: the game's
//! FUN_002c27b0 (read from the disassembly of the user's executable; the decompiler drops its VU0
//! code). The sound engine keeps the position the request gave and, every tick while the sound
//! plays, transforms it with the camera's view matrix (FUN_001068c0(0) + 0x10), derives a volume
//! from the squared distance and a pan angle from the direction in the horizontal plane, and sends
//! both to the sound driver (FUN_002be564 -> IOP command 0x15). A request without a position
//! (interface sounds, FUN_002c3318) plays at the default volume 0x3fff and pan 0x40.

use std::f32::consts::PI;

/// Full volume and the centre pan of the driver.
pub const FULL_VOLUME: i32 = 0x3fff;
pub const CENTRE_PAN: i32 = 0x40;

/// Volume at or above which the engine ignores the computed pan and sends the centre
/// (FUN_002be564 and FUN_002be49c: `volume > 0x39ff` forces pan 0x40).
const CENTRE_ABOVE: i32 = 0x39ff;

/// The divisor of the squared distance (the word at 0x36f594 in the executable).
const DISTANCE_DIVISOR: f32 = 100.0;

/// Volume and pan for a sound whose position is `rel` in camera space: x to the right of the
/// screen, y up or down (ignored), z in front of the camera, all in game units. Returns the
/// values the engine sends to the driver (pan is an angle on the driver's circle: 0 hard left,
/// 0x40 front, 0x80 hard right, 0xc0 behind).
pub fn volume_pan(rel: [f32; 3]) -> (i32, i32) {
    let [x, y, z] = rel;
    let d2 = x * x + y * y + z * z;
    let d = d2.sqrt();
    // The engine squares the square-rooted distance in single precision, then divides.
    let steps = ((d * d) / DISTANCE_DIVISOR) as i32;
    let volume = if steps < 0x4000 { (FULL_VOLUME - steps).clamp(0, FULL_VOLUME) } else { 0 };
    if volume > CENTRE_ABOVE || d <= 0.0 {
        return (volume, CENTRE_PAN);
    }
    // Direction with the vertical part dropped after normalising (it is not renormalised).
    let (nx, nz) = (x / d, z / d);
    let angle = angle_from_forward(nz);
    let a = angle * 128.0 / PI;
    let pan = if nx < 0.0 && nz > 0.0 {
        64.0 - a
    } else if nx < 0.0 && nz < 0.0 {
        320.0 - a
    } else {
        64.0 + a
    };
    (volume, pan as i32)
}

/// Sound effect entry flag: the entry always plays at full volume (`Driver::play_se_handle`).
pub const SE_FIXED_VOLUME: u8 = 0x10;
/// Sound effect entry flag: the entry always plays at the centre pan.
pub const SE_FIXED_PAN: u8 = 0x20;

/// The values a playing sound effect is given every tick (IOP command 0x15) for a source at `rel`
/// (camera space, game units), honouring the entry's fixed-volume and fixed-pan flags as the
/// driver does when it starts the sound.
pub fn se_mix(rel: [f32; 3], entry_flags: u8) -> (i32, i32) {
    let (v, p) = volume_pan(rel);
    (if entry_flags & SE_FIXED_VOLUME != 0 { FULL_VOLUME } else { v }, if entry_flags & SE_FIXED_PAN != 0 { CENTRE_PAN } else { p })
}

/// The same for a voice line (kind 5): `FUN_002c27b0` clamps the volume to the slot's minimum
/// (`actor + 0x2c7`, 0x1800 for the speakers seen so far) and the pan is centred above 0x39ff.
pub fn voice_mix(rel: [f32; 3], min_volume: i32) -> (i32, i32) {
    let (v, p) = volume_pan(rel);
    let v = v.clamp(min_volume.min(FULL_VOLUME), FULL_VOLUME);
    (v, if v > CENTRE_ABOVE { CENTRE_PAN } else { p })
}

/// FUN_0017bf10 then FUN_0017bbc0: the arccosine of the dot product with the forward axis (1 and
/// above is 0, -1 and below is pi), wrapped with fmod(angle + pi, 2 pi) - pi.
fn angle_from_forward(dot: f32) -> f32 {
    let angle = if dot < 1.0 { if dot > -1.0 { dot.acos() } else { PI } } else { 0.0 };
    if angle > 0.0 {
        (angle + PI) % (2.0 * PI) - PI
    } else {
        PI - (PI - angle) % (2.0 * PI)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_sounds_are_loud_and_centred() {
        // 100 units away (about the length of an arm): 0x3fff - 100, centre pan by the cut-off.
        assert_eq!(volume_pan([0.0, 0.0, 100.0]), (0x3fff - 100, 0x40));
        assert_eq!(volume_pan([100.0, 0.0, 0.0]).1, 0x40);
    }

    #[test]
    fn volume_falls_with_the_square_of_the_distance() {
        assert_eq!(volume_pan([0.0, 0.0, 500.0]).0, 0x3fff - 2500);
        assert_eq!(volume_pan([0.0, 0.0, 1000.0]).0, 0x3fff - 10000);
        assert_eq!(volume_pan([0.0, 0.0, 1300.0]).0, 0);
    }

    /// A fixed source passed by a moving listener (the listener moves along x at z = 600): the
    /// volume peaks at the closest point and falls on both sides, the pan sweeps from the left
    /// half to the right half of the circle through the centre.
    #[test]
    fn a_source_passing_the_listener_sweeps_volume_and_pan() {
        let mut last_pan = -1;
        let mut peak = (0, 0);
        for step in -20..=20 {
            let x = step as f32 * 60.0;
            let (v, p) = volume_pan([x, 0.0, 600.0]);
            assert!(p >= last_pan, "pan must not go back: {p} after {last_pan} at x {x}");
            last_pan = p;
            if v > peak.0 {
                peak = (v, step);
            }
        }
        assert_eq!(peak.1, 0);
        assert!(volume_pan([-1200.0, 0.0, 600.0]).1 < 0x20);
        assert!(volume_pan([1200.0, 0.0, 600.0]).1 > 0x60);
        assert!(volume_pan([-1200.0, 0.0, 600.0]).0 < volume_pan([-600.0, 0.0, 600.0]).0);
    }

    #[test]
    fn entry_flags_and_minimum_volume_apply() {
        let far = [900.0, 0.0, 900.0];
        let (v, p) = volume_pan(far);
        assert_eq!(se_mix(far, 0), (v, p));
        assert_eq!(se_mix(far, SE_FIXED_VOLUME), (FULL_VOLUME, p));
        assert_eq!(se_mix(far, SE_FIXED_PAN), (v, CENTRE_PAN));
        // A voice stays at its minimum when the source is out of range, but keeps its pan.
        assert_eq!(volume_pan([2000.0, 0.0, 2000.0]).0, 0);
        assert_eq!(voice_mix([2000.0, 0.0, 2000.0], 0x1800).0, 0x1800);
        assert!(voice_mix([2000.0, 0.0, 1.0], 0x1800).1 > 0x60);
        // Close voices are centred.
        assert_eq!(voice_mix([5.0, 0.0, 5.0], 0x1800).1, CENTRE_PAN);
    }

    #[test]
    fn pan_follows_the_side_and_the_back() {
        let far = 600.0;
        let (_, front) = volume_pan([0.0, 0.0, far]);
        let (_, left) = volume_pan([-far, 0.0, 1.0]);
        let (_, right) = volume_pan([far, 0.0, 1.0]);
        let (_, back) = volume_pan([1.0, 0.0, -far]);
        assert_eq!(front, 0x40);
        assert!(left <= 1, "left {left}");
        assert!((127..=128).contains(&right), "right {right}");
        assert!((190..=192).contains(&back), "back {back}");
        // Exactly behind the angle is pi, which the game's wrap turns into -pi: pan -64.
        assert_eq!(volume_pan([0.0, 0.0, -far]).1, -64);
        // Front left and front right straddle the centre.
        assert!(volume_pan([-far, 0.0, far]).1 < 0x40);
        assert!(volume_pan([far, 0.0, far]).1 > 0x40);
        // Behind to the left lands in the last quarter of the circle, behind right in the third.
        assert!(volume_pan([-far, 0.0, -far]).1 > 0xc0);
        assert!((0x80..0xc0).contains(&volume_pan([far, 0.0, -far]).1));
    }
}
