//! The object a BDX enemy script sees: `Obj` (position and facing, the motion it asked for, flags, fade,
//! movement vector, the records and volumes it was handed) with its handle constants and the game-space
//! conversions. The host that answers the script's calls from it is in `enemy_host.rs`, the Bevy systems that
//! run the script and carry out its requests are in `enemy_ai.rs`.

use std::collections::HashMap;

use bevy::prelude::*;
use kh2_bdx::Vec4;

use crate::motor::Motor;
use crate::reaction_cmd::PlayerReactionCmd;

/// Object handles the script sees. The scripted object's memory is `[handle, handle + OBJ_SIZE)`.
pub(crate) const SELF_BASE: u32 = 0x0100_0000;
pub(crate) const PLAYER: u32 = 0x0200_0000;
pub(crate) const TARGET: u32 = 0x0300_0000;
pub(crate) const EFFECT: u32 = 0x0400_0000;
pub(crate) const HIT: u32 = 0x0500_0000;
/// Target records handed to event 11 (0x14 bytes each, built like FUN_0018ed40 + FUN_0018f490:
/// object, position index, 0, -1, 0), one per accepted command.
pub(crate) const RECORD: u32 = 0x0600_0000;
pub(crate) const RECORD_SIZE: u32 = 0x20;
/// Attack volume objects handed to the clash hook and event 0x13 (0x1b2478), two per clash.
pub(crate) const VOLUME: u32 = 0x0700_0000;
pub(crate) const VOLUME_SIZE: u32 = 0x40;
/// The word of a volume object that holds its owner (0x1b2478 passes word +8 of the other volume
/// to FUN_001b21a0 as the other side's object). The other fields are not served.
pub(crate) const VOLUME_OWNER: u32 = 8;
pub(crate) const OBJ_SIZE: usize = 0x1000;
/// The status block of the scripted object (host 1.84: the pointer at object +0x48c).
pub(crate) const GAUGES: u32 = 0x0700_0000;
/// Bit 3 of an object's flag array: "cannot be hit" (FUN_0019a4f0).
pub(crate) const FLAG_BIT_UNHITTABLE: i32 = 3;
/// Offset of the movement vector (direction and speed in w) in a game object.
pub(crate) const MOVE_VECTOR: usize = 0x10;
/// Flag bit 3 of the object's flag array: the object cannot be hit (FUN_0019a4f0 tests it).
pub(crate) const FLAG_UNHITTABLE: u32 = 3;

// ---------------------------------------------------------------------------------------------------
// Game space

/// A position in the app (centimetres, Y up) as the script sees it.
pub(crate) fn to_game(p: Vec3) -> Vec4 {
    [p.x, -p.y, -p.z, 1.0]
}

/// A direction from the script as an app direction.
pub(crate) fn from_game(d: Vec4) -> Vec3 {
    Vec3::new(d[0], -d[1], -d[2])
}

/// App yaw of a script direction: atan2(x, z) in the game's space (atan2 of nothing is 0), then
/// mirrored into the app's.
pub(crate) fn yaw_of(d: Vec4) -> f32 {
    let g = if d[0] == 0.0 && d[2] == 0.0 { 0.0 } else { d[0].atan2(d[2]) };
    (g.sin()).atan2(-g.cos())
}

// ---------------------------------------------------------------------------------------------------
// The scripted object

pub(crate) struct Fade {
    pub(crate) from: f32,
    pub(crate) to: f32,
    pub(crate) frames: f32,
    pub(crate) t: f32,
}

pub(crate) struct MotionReq {
    pub(crate) slot: usize,
    pub(crate) blend: f32,
    pub(crate) start: Option<f32>,
}

/// What the script sees of its object, plus what it has asked for during this tick.
pub(crate) struct Obj {
    pub(crate) handle: u32,
    /// Position (centimetres, Y up), facing, ground contact and the jump model.
    pub(crate) motor: Motor,
    pub(crate) hp: i32,
    /// A hit reaction is playing (`EState::Hurt` or `Blown`).
    pub(crate) reacting: bool,
    pub(crate) on_screen: bool,
    /// The motion slot playing, as the script left it or the reaction code changed it (the game's +0x154).
    pub(crate) slot: Option<usize>,
    /// The motion reached its end during the last tick (looping motions: each time they wrap).
    pub(crate) done: bool,
    pub(crate) flags: u64,
    pub(crate) flags768: bool,
    pub(crate) mem: Vec<u8>,
    /// Horizontal movement set by `set_move` / the object's memory, centimetres per tick; used for one tick.
    pub(crate) move_vel: Vec3,
    /// Horizontal speed given to `start_move`, used while airborne.
    pub(crate) air_vel: Vec3,
    pub(crate) alpha: f32,
    pub(crate) fade: Option<Fade>,
    pub(crate) hidden: bool,
    pub(crate) motion_req: Option<MotionReq>,
    /// Motions the jump started by `start_move` (with the commit flag) plays: falling, landing.
    pub(crate) fall_motion: i32,
    pub(crate) land_motion: i32,
    pub(crate) fall_pending: bool,
    pub(crate) effects: Vec<u16>,
    pub(crate) next_effect: u32,
    /// Hit records of event 6 by handle: hit is real (2.8) and the effective reaction, byte +0x26 (2.7).
    pub(crate) hits: HashMap<u32, (bool, u8)>,
    pub(crate) next_hit: u32,
    /// Target records given to event 11, readable by the script (handle `RECORD + k * RECORD_SIZE`).
    pub(crate) records: Vec<[u8; 20]>,
    /// What the script asked of the player this tick (host calls on the player's context).
    pub(crate) player_cmds: Vec<PlayerReactionCmd>,
    /// The player's +0x158 after this tick's `play_motion_with`, if it changed.
    pub(crate) player_serial: Option<i32>,
    /// The spawn kind byte of the object's spawn record (host 1.110), -1 without one.
    pub(crate) spawn_kind: i32,
    /// Messages the script sent this tick (host 1.90 / 1.241): sender, mode name, delay in frames.
    pub(crate) outbox: Vec<(u32, String, f32)>,
    /// Volume objects of clashes (handle `VOLUME + k * VOLUME_SIZE`): owner and attack row.
    pub(crate) volumes: Vec<(u32, i16)>,
    /// Prize rows of host 1.92 calls this tick.
    pub(crate) drops: Vec<i32>,
    /// Host 6.31 ran this tick (FUN_002eece8: every prize and item collected by the player).
    pub(crate) collect_all: bool,
    /// Spawn parameters 1 and 2 (object +0x820, +0x824, copied from the record by FUN_0016b870).
    pub(crate) spawn_params: Option<[i32; 2]>,
    /// The `enmp` row of this tick's `set_obj_param` when it is not the one the stats were built from.
    pub(crate) param_row: Option<u16>,
    /// The HP record of part 0 of the status block (+0x48c, FUN_0019f770: current, maximum, lowest):
    /// the maximum and the lowest as the enemy has them, and the change the script asked for this tick.
    pub(crate) hp_max: i32,
    pub(crate) hp_min: i32,
    pub(crate) hp_add: i32,
    /// The word at object +0xabc (host 1.211 / 1.212).
    pub(crate) flags_abc: u32,
    /// The motion controller's (+0x140) trigger bytes +0xf8 and +0xf9 (object +0x238, +0x239): bits 0..3
    /// of +0xf8 while range triggers 16..19 are active (FUN_00188030; cleared at the start of each pass,
    /// FUN_00187fb0), bits 0..3 of +0xf9 when frame triggers 9..12 fired in the last advance (the frame
    /// trigger dispatcher 0x188498, cases at 0x1886c0; cleared by FUN_0017e3b8 before it advances).
    pub(crate) trigger_bits: [u8; 2],
    /// Mover type 5, installed by `set_dash` (host 1.58): its velocity (+0x20, the app's space,
    /// centimetres per tick) and its factor (+0xf0).
    pub(crate) dash: Option<(Vec3, f32)>,
    /// The motion controller's time (+0x140 + 0x30 = object +0x170), in the motion's own frames: the
    /// `Animator` frame at the start of the tick, or the first frame of a motion requested in it.
    pub(crate) frame: f32,
    /// Blend time left (the controller's +0x34), in ticks: the blend of the last request, run down
    /// one per tick.
    pub(crate) blend_left: f32,
    /// The last tick's movement as FUN_00181e00 keeps it (the app's space, centimetres per tick):
    /// what the object asked for (+0x850) and how far it really went (+0x860, new position minus
    /// old, after collision).
    pub(crate) want_vel: Vec3,
    pub(crate) moved: Vec3,
}

impl Obj {
    pub(crate) fn new(handle: u32) -> Obj {
        Obj {
            handle,
            motor: Motor { grounded: true, ..Default::default() },
            hp: 1,
            reacting: false,
            on_screen: true,
            slot: None,
            done: false,
            flags: 0,
            flags768: false,
            mem: vec![0; OBJ_SIZE],
            move_vel: Vec3::ZERO,
            air_vel: Vec3::ZERO,
            alpha: 1.0,
            fade: None,
            hidden: false,
            motion_req: None,
            fall_motion: -1,
            land_motion: -1,
            fall_pending: false,
            effects: Vec::new(),
            next_effect: 0,
            hits: HashMap::new(),
            next_hit: 0,
            records: Vec::new(),
            player_cmds: Vec::new(),
            player_serial: None,
            spawn_kind: -1,
            outbox: Vec::new(),
            volumes: Vec::new(),
            drops: Vec::new(),
            collect_all: false,
            spawn_params: None,
            param_row: None,
            hp_max: 1,
            hp_min: 0,
            hp_add: 0,
            flags_abc: 0,
            trigger_bits: [0; 2],
            dash: None,
            frame: 0.0,
            blend_left: 0.0,
            want_vel: Vec3::ZERO,
            moved: Vec3::ZERO,
        }
    }

    /// A target record for event 11 (FUN_0018ed40 then FUN_0018f490: object, position index, 0, -1,
    /// and a fifth word the manager copies, 0 here). Returns its handle.
    pub(crate) fn new_record(&mut self, obj: u32, pos: i16) -> u32 {
        let mut r = [0u8; 20];
        r[0..4].copy_from_slice(&obj.to_le_bytes());
        r[4..8].copy_from_slice(&(pos as i32).to_le_bytes());
        r[12..16].copy_from_slice(&(-1i32).to_le_bytes());
        self.records.push(r);
        RECORD + (self.records.len() as u32 - 1) * RECORD_SIZE
    }

    /// The two volume objects of a clash: the object's own attack volume and the player's (the only
    /// other team in the app). Returns their handles (own, other).
    pub(crate) fn new_volumes(&mut self, own_atkp: i16, other_atkp: i16) -> (u32, u32) {
        let k = self.volumes.len() as u32;
        self.volumes.push((self.handle, own_atkp));
        self.volumes.push((PLAYER, other_atkp));
        (VOLUME + k * VOLUME_SIZE, VOLUME + (k + 1) * VOLUME_SIZE)
    }

    /// Word +8 (the owner) of a clash volume object.
    pub(crate) fn volume_word(&self, addr: u32, out: &mut [u8]) -> bool {
        let v = addr.wrapping_sub(VOLUME);
        let Some(&(owner, _)) = self.volumes.get((v / VOLUME_SIZE) as usize) else { return false };
        let k = (v % VOLUME_SIZE).wrapping_sub(VOLUME_OWNER) as usize;
        let b = owner.to_le_bytes();
        match b.get(k..k + out.len()) {
            Some(s) if out.len() <= 4 => {
                out.copy_from_slice(s);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn advance_fade(&mut self) {
        if let Some(f) = &mut self.fade {
            f.t += 1.0;
            let k = if f.frames <= 0.0 { 1.0 } else { (f.t / f.frames).min(1.0) };
            self.alpha = f.from + (f.to - f.from) * k;
            if k >= 1.0 {
                self.fade = None;
            }
        }
    }

    pub(crate) fn set_move(&mut self, dir: Vec4, speed: f32) {
        let d = from_game(dir).normalize_or_zero();
        self.move_vel = Vec3::new(d.x, 0.0, d.z) * speed;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn the_movement_vector_is_horizontal_and_in_the_apps_axes() {
        let mut o = Obj::new(SELF_BASE);
        o.set_move([3.0, 0.0, -3.0, 0.0], 8.0);
        let want = Vec3::new(1.0, 0.0, 1.0).normalize() * 8.0;
        assert!((o.move_vel - want).length() < 1e-4, "{:?}", o.move_vel);
    }
}
