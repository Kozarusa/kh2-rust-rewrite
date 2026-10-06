//! A deterministic stand-in world for tests and for reading what a script does: one scripted object,
//! the player standing some distance away, motions that last a fixed number of frames, and a log of
//! every call the script made. Nothing here comes from the game.

use crate::host::{Call, Host, Value, Vec4};

pub const SELF_OBJ: u32 = 0x100;
pub const PLAYER: u32 = 0x200;
pub const TARGET: u32 = 0x300;
/// How long the toy world keeps an object in the air after a jump.
pub const JUMP_FRAMES: u32 = 30;

pub struct MockHost {
    pub frame: u32,
    pub position: Vec4,
    pub player_position: Vec4,
    pub motion: i32,
    pub motion_frames_left: f32,
    /// Frames a motion lasts.
    pub motion_length: f32,
    pub move_dir: Vec4,
    pub move_speed: f32,
    pub facing: Vec4,
    pub flags: Vec<i32>,
    /// Hit reaction frames left.
    pub stun: u32,
    /// Frames left in the air; a move that points upwards (a jump) sets it to [`JUMP_FRAMES`].
    pub airborne: u32,
    /// The scripted object's own memory (`SELF_OBJ..`), for scripts that write fields such as the
    /// movement vector at +0x10 directly.
    pub object: Vec<u8>,
    /// What host 1.39 (spawn parameter, word i of the object's array at +0x81c) answers; 99 by default.
    pub spawn_param: i32,
    /// Every call made by the script, with the frame it happened on.
    pub log: Vec<(u32, Call)>,
}

/// Size of the scripted object's memory in the toy world.
const OBJECT_SIZE: usize = 0x1000;
/// Offset of the movement vector (direction and speed in w) in a game object.
const MOVE_VECTOR: usize = 0x10;

impl MockHost {
    pub fn new() -> MockHost {
        MockHost {
            frame: 0,
            position: [0.0, 0.0, 0.0, 1.0],
            player_position: [400.0, 0.0, 0.0, 1.0],
            motion: 0,
            motion_frames_left: 0.0,
            motion_length: 30.0,
            move_dir: [0.0; 4],
            move_speed: 0.0,
            facing: [0.0, 0.0, 1.0, 0.0],
            flags: Vec::new(),
            stun: 0,
            airborne: 0,
            object: vec![0; OBJECT_SIZE],
            spawn_param: 99,
            log: Vec::new(),
        }
    }

    /// Advances the toy world one frame: motions run down, the object moves along its move vector.
    pub fn tick(&mut self) {
        self.frame += 1;
        if self.motion_frames_left > 0.0 {
            self.motion_frames_left -= 1.0;
        }
        self.stun = self.stun.saturating_sub(1);
        self.airborne = self.airborne.saturating_sub(1);
        for i in 0..3 {
            self.position[i] += self.move_dir[i] * self.move_speed;
        }
    }

    pub fn calls(&self) -> impl Iterator<Item = &Call> {
        self.log.iter().map(|(_, c)| c)
    }

    fn sub(a: Vec4, b: Vec4) -> Vec4 {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2], 0.0]
    }
}

impl Default for MockHost {
    fn default() -> Self {
        MockHost::new()
    }
}

impl Host for MockHost {
    fn call(&mut self, c: &Call) -> Value {
        self.log.push((self.frame, c.clone()));
        match c {
            Call::Player | Call::LastAttacker { .. } => Value::Handle(PLAYER),
            Call::Target { .. } => Value::Handle(TARGET),
            Call::FindObject { by: crate::host::ObjectKey::Entity, value: 0x236 } => Value::Handle(PLAYER),
            Call::FindObject { .. } => Value::Handle(TARGET),
            Call::Position { obj } => Value::Vec(if *obj == PLAYER { self.player_position } else { self.position }),
            Call::TargetPosition { .. } => Value::Vec(self.player_position),
            Call::FacingVector { .. } => Value::Vec(self.facing),
            Call::ObjVec860 { .. } | Call::ObjVec850 { .. } => Value::Vec([0.0; 4]),
            Call::PlayMotion { motion, .. } | Call::PlayMotionChecked { motion, .. } | Call::PlayMotionAt { motion, .. } => {
                self.motion = *motion;
                self.motion_frames_left = self.motion_length;
                Value::None
            }
            Call::StartMove { dir, motion, .. } | Call::StartMoveAlt { dir, motion, .. } => {
                if dir[1] > 0.0 {
                    self.airborne = JUMP_FRAMES;
                }
                self.move_dir = *dir;
                self.motion = *motion;
                self.motion_frames_left = self.motion_length;
                Value::None
            }
            Call::SetMove { dir, speed, .. } | Call::SetVelocity { dir, speed, .. } => {
                self.move_dir = *dir;
                self.move_speed = *speed;
                Value::None
            }
            Call::FaceDirection { dir, .. } | Call::FaceToward { dir, .. } => {
                self.facing = *dir;
                Value::None
            }
            Call::TurnToward { dir, .. } => {
                self.facing = *dir;
                Value::Bool(false)
            }
            Call::MotionDone { .. } => Value::Bool(self.motion_frames_left <= 0.0),
            // 1.63 motion_near_end: the toy motions have no return frame, so their end.
            Call::Func { group: 1, index: 63, .. } => Value::Bool(self.motion_frames_left <= 0.0),
            Call::MotionState { .. } => Value::Int(0),
            Call::MotionLength { .. } | Call::MotionEnd { .. } => Value::Float(self.motion_length),
            Call::OnScreen { .. } => Value::Bool(true),
            Call::Airborne { .. } => Value::Bool(self.airborne > 0),
            Call::Busy { .. } | Call::Flag124 { .. } | Call::Flag309 { .. } => Value::Bool(false),
            Call::HitStun { .. } => Value::Bool(self.stun > 0),
            Call::SpawnParam { .. } => Value::Int(self.spawn_param),
            Call::SpawnKind { .. } => Value::Int(0),
            Call::GlobalByte33e830 | Call::GlobalByte301 => Value::Int(0),
            Call::GlobalFlag4 | Call::GlobalFlag80 | Call::EventActive => Value::Bool(false),
            Call::HitIsReal { .. } => Value::Bool(true),
            Call::ProbeGround { .. } => Value::Int(0),
            Call::SpawnAttached { .. } => Value::Handle(0x400),
            Call::SetFlag { bit, .. } => {
                self.flags.push(*bit);
                Value::None
            }
            Call::ClearFlag { bit, .. } => {
                self.flags.retain(|b| b != bit);
                Value::None
            }
            Call::ModeFrozen { .. } | Call::ModeInterrupted { .. } => Value::Bool(false),
            Call::ModeMayStart { .. } => Value::Bool(true),
            Call::Unknown { .. } => Value::Int(0),
            _ => Value::None,
        }
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        let o = addr.wrapping_sub(SELF_OBJ) as usize;
        match self.object.get(o..o + out.len()) {
            Some(b) => {
                out.copy_from_slice(b);
                true
            }
            None => false,
        }
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        let o = addr.wrapping_sub(SELF_OBJ) as usize;
        let Some(b) = self.object.get_mut(o..o + data.len()) else { return false };
        b.copy_from_slice(data);
        // A write that covers the movement vector steers the toy object like `set_move` does.
        if o <= MOVE_VECTOR && o + data.len() >= MOVE_VECTOR + 16 {
            let v: [f32; 4] = std::array::from_fn(|i| f32::from_le_bytes(self.object[MOVE_VECTOR + 4 * i..MOVE_VECTOR + 4 * i + 4].try_into().unwrap()));
            self.move_dir = [v[0], v[1], v[2], 0.0];
            self.move_speed = v[3];
        }
        true
    }
}

#[allow(dead_code)]
fn _unused(a: Vec4, b: Vec4) -> Vec4 {
    MockHost::sub(a, b)
}
