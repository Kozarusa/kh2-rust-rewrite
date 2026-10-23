//! The event camera track (keys, channels, cuts and shots) the lower crates read (moved out of `intro.rs`, which re-exports it).

use bevy::prelude::*;

/// One key of an event camera channel (16 bytes, `FUN_0022ab08` / `FUN_0022a9d8`): word 0 is the key type in its
/// top three bits and the time in its low 29 (signed, 1/512 s), then the value, the incoming and the outgoing
/// tangent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub kind: u32,
    pub time: i32,
    pub value: f32,
    pub tin: f32,
    pub tout: f32,
}

impl Key {
    pub fn parse(b: &[u8]) -> Option<Key> {
        let w = |i: usize| b.get(4 * i..4 * i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
        let w0 = w(0)?;
        Some(Key { kind: w0 >> 29, time: ((w0 << 3) as i32) >> 3, value: f32::from_bits(w(1)?), tin: f32::from_bits(w(2)?), tout: f32::from_bits(w(3)?) })
    }
}

/// `FUN_0022a9d8`: the value between key `i` and key `i + 1` at `s` seconds: type 0 holds key `i`, 1 is linear,
/// 2..4 a cubic Hermite with key `i`'s outgoing and key `i + 1`'s incoming tangent (per second), other types 0.
pub fn segment(keys: &[Key], i: usize, s: f32) -> f32 {
    const UNIT: f32 = 1.0 / 512.0; // 0x37f12c
    let (k, n) = (keys[i], keys[i + 1]);
    let (a, b) = (k.time as f32 * UNIT, n.time as f32 * UNIT);
    match k.kind {
        0 => k.value,
        1 => k.value + (n.value - k.value) * (s - a) / (b - a),
        2..=4 => {
            let u = s - a;
            let inv = 1.0 / (b - a);
            let u2 = u * u;
            let f6 = u2 * inv;
            let f5 = u2 * u * inv * inv;
            let f3 = u2 * 3.0 * inv * inv;
            let f4 = (f5 + f5) * inv;
            n.tin * (f5 - f6) + k.tout * ((f5 - (f6 + f6)) + u) + n.value * (f3 - f4) + k.value * ((f4 - f3) + 1.0)
        }
        _ => 0.0,
    }
}

/// `FUN_0022ab08`: one channel at `t` event frames (30 a second, 0x37f138; key times are 1/512 s, 0x37f13c).
/// Before the first key and after the last the value runs on along a straight line: the slope to the
/// neighbouring key for a linear key, else the key's own tangent, taken per key time unit (as the code does).
pub fn channel(keys: &[Key], t: f32) -> f32 {
    let s = t / 30.0;
    match keys.len() {
        0 => return 0.0,
        1 => return keys[0].value,
        _ => {}
    }
    let ti = (s * 512.0) as i32;
    let first = keys[0];
    if ti <= first.time {
        let dt = (keys[1].time - first.time) as f32;
        let slope = if first.kind == 1 && dt != 0.0 { (keys[1].value - first.value) / dt } else { first.tin };
        return first.value - (first.time - ti) as f32 * slope;
    }
    let n = keys.len();
    let last = keys[n - 1];
    if last.time <= ti {
        let dt = (last.time - keys[n - 2].time) as f32;
        let slope = if last.kind == 1 && dt != 0.0 { (last.value - keys[n - 2].value) / dt } else { last.tout };
        return last.value + (ti - last.time) as f32 * slope;
    }
    // The key with the exact time gives its value; otherwise the segment around the time.
    if let Some(k) = keys.iter().find(|k| k.time == ti) {
        return k.value;
    }
    let i = keys.iter().rposition(|k| k.time < ti).unwrap_or(0).min(n - 2);
    segment(keys, i, s)
}

/// The event camera's channels in the order the code reads them (`FUN_0022adb8`): position x, y, z, look-at x,
/// y, z, roll (degrees) and field of view (degrees). The code negates y and z into the game's Y-down space; the
/// app's space turns the game's half a turn about X, so the keys are the app's coordinates as they stand (cm).
pub type Channels = [Vec<Key>; 8];

/// What a camera record gives at a time: position and look-at (app space, cm), roll and horizontal field of view
/// (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub pos: bevy::math::Vec3,
    pub look: bevy::math::Vec3,
    pub roll: f32,
    pub fov: f32,
}

/// `FUN_0022adb8` then `FUN_0022b0f8`'s hand-off: a channel without keys keeps the start value (0, the field of
/// view 1.0 at 0x37f154); the roll goes through pi / 180 (0x37f144 / 0x37f148), the field of view through
/// 0x37f15c (pi / 180).
pub fn shot(ch: &Channels, t: f32) -> Shot {
    let v = |i: usize, default: f32| if ch[i].is_empty() { default } else { channel(&ch[i], t) };
    use bevy::math::Vec3;
    Shot {
        pos: Vec3::new(v(0, 0.0), v(1, 0.0), v(2, 0.0)),
        look: Vec3::new(v(3, 0.0), v(4, 0.0), v(5, 0.0)),
        roll: v(6, 0.0).to_radians(),
        fov: v(7, 1.0).to_radians(),
    }
}

/// One entry of the cut table (event opcode 6, `FUN_0022a970` -> the 128 entries at `DAT_0036bbb4`): camera
/// record `cam` is drawn from event frame `start` to `end` (both included, `FUN_0022b0f8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cut {
    pub cam: i16,
    pub start: i16,
    pub end: i16,
}

/// The camera part of an event (the setup reader `FUN_002245d0` and the timeline thread `FUN_00223a88`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// Opcode 0x13 (`FUN_0022b4f8`): camera records by their index (+4).
    pub cams: Vec<(i16, Channels)>,
    pub cuts: Vec<Cut>,
    /// Opcode 8 (`FUN_00226fb0`): the end frame.
    pub end: i32,
    /// Opcode 0x31 with mode 1 (`FUN_0022bc40` -> `FUN_0022b590`): from this frame the camera flies to the field
    /// camera's place behind the player, reached at the end frame.
    pub follow_at: Option<i32>,
    /// Opcode 0x46 (`FUN_0022bce0`): the camera is pulled in against the map (`DAT_0036bce4`), with the blockers
    /// when its +6 is 1 (`DAT_0036bce5`).
    pub collide: Option<bool>,
    /// Opcode 0x4c (`FUN_0022bcd0`): the springs run on the camera (`DAT_0036bcdb`).
    pub smooth: bool,
    /// Opcode 0x3c (`FUN_00226188`): (frame, effect number, mode) of the event's effect group (mode 0 starts it).
    pub effects: Vec<(i32, u16, i16)>,
}

impl Track {
    pub fn parse(cmds: &[kh2_ard::EventCommand]) -> Track {
        let mut t = Track { end: -1, ..Track::default() };
        let h = |d: &[u8], o: usize| d.get(o..o + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]));
        for c in cmds {
            let d = &c.data;
            match c.opcode {
                8 => t.end = h(d, 0) as i32,
                6 => t.cuts.push(Cut { cam: h(d, 0), start: h(d, 2), end: h(d, 4) }),
                0x13 => {
                    let ch: Channels = std::array::from_fn(|k| {
                        let (first, n) = (h(d, 2 + 4 * k) as u16 as usize, h(d, 4 + 4 * k).max(0) as usize);
                        (0..n).filter_map(|i| d.get(0x24 + 16 * (first + i)..).and_then(Key::parse)).collect()
                    });
                    t.cams.push((h(d, 0), ch));
                }
                0x31 if h(d, 2) == 1 => t.follow_at = Some(h(d, 0) as i32),
                0x46 => t.collide = Some(h(d, 2) == 1),
                0x4c => t.smooth = true,
                0x3c => t.effects.push((h(d, 0) as i32, h(d, 2) as u16, h(d, 4))),
                _ => {}
            }
        }
        t
    }

    pub fn has_camera(&self) -> bool {
        !self.cams.is_empty() && !self.cuts.is_empty()
    }

    /// `FUN_0022b0f8` at event frame `f` (fractional): the last cut whose range holds the frame, evaluated at the
    /// frame minus the cut's start.
    pub fn at(&self, f: f32) -> Option<Shot> {
        let fi = f.floor() as i32;
        let cut = self.cuts.iter().rfind(|c| c.start as i32 <= fi && fi <= c.end as i32)?;
        let (_, ch) = self.cams.iter().find(|(i, _)| *i == cut.cam)?;
        Some(shot(ch, f - cut.start as f32))
    }
}

/// `FUN_0022b590` / `FUN_0022af88`: the fly to the field camera: two keys of type 2 with zero tangents, at the
/// frame of opcode 0x31 (the view drawn then) and at the end frame (the field camera placed behind the player,
/// `FUN_00140e90`), kept in the player's space; between them a cubic ease (Hermite with zero tangents). Key times
/// are whole 1/512 s (`(frame << 9) / 30`).
pub fn follow(from: Shot, to: Shot, start: i32, end: i32, f: f32) -> Shot {
    let (a, b) = (((start << 9) / 30) as f32 / 512.0, ((end << 9) / 30) as f32 / 512.0);
    let s = f / 30.0;
    let tau = if b > a { ((s - a) / (b - a)).clamp(0.0, 1.0) } else { 1.0 };
    let w = tau * tau * (3.0 - 2.0 * tau);
    Shot { pos: from.pos.lerp(to.pos, w), look: from.look.lerp(to.look, w), roll: from.roll + (to.roll - from.roll) * w, fov: from.fov + (to.fov - from.fov) * w }
}
