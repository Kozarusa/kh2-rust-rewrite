//! The interface between scripts and the game world. A script talks to the world only through host
//! functions; the interpreter decodes each one into a [`Call`] and hands it to the [`Host`].
//!
//! `obj` is always the game object the script drives (the AI "self" unless a variant says otherwise);
//! the interpreter resolves the script's context pointer to it. Objects, targets and hit records are
//! opaque 32-bit handles owned by the host. Vectors are `[x, y, z, w]`.
//!
//! Which fields of the original object each call touches is listed in docs/bdx.md. Calls marked
//! "role inferred" have their effect on the object read from the game code but their game-level
//! meaning guessed from how scripts use them.

pub type Vec4 = [f32; 4];

/// What a call returns. The interpreter converts it to the word the script expects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    None,
    Int(i32),
    Float(f32),
    Bool(bool),
    /// Handle of an object, target or effect.
    Handle(u32),
    Vec(Vec4),
}

impl Value {
    pub fn int(self) -> i32 {
        match self {
            Value::Int(i) => i,
            Value::Bool(b) => b as i32,
            Value::Handle(h) => h as i32,
            Value::Float(f) => f as i32,
            _ => 0,
        }
    }
}

/// What an object lookup (`Call::FindObject`) compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKey {
    /// The entity id the game uses for objects (0x236 is the player), FUN_0016b078.
    Entity,
    /// The byte at +0x1e of the spawn record (FUN_0016b928).
    SpawnGroup,
    /// The object's NeoStatus value, the halfword at +0x4c of its object-entry row (FUN_001699a8).
    NeoStatus,
}

/// One argument of a [`Call::Func`], decoded by the kind letter the function table gives it.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Int(i32),
    Float(f32),
    /// The object behind a context pointer (`O`) or an object handle (`o`).
    Obj(u32),
    Vec(Vec4),
    Str(String),
    /// A pointer or handle the script or the host owns.
    Raw(u32),
}

impl Arg {
    pub fn int(&self) -> i32 {
        match self {
            Arg::Int(i) => *i,
            Arg::Float(f) => *f as i32,
            Arg::Obj(o) | Arg::Raw(o) => *o as i32,
            _ => 0,
        }
    }

    pub fn float(&self) -> f32 {
        match self {
            Arg::Float(f) => *f,
            Arg::Int(i) => *i as f32,
            _ => 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    // ---- world queries -------------------------------------------------------------------
    /// The player's object (Sora). Game global.
    Player,
    /// World position of the object (vector).
    Position { obj: u32 },
    /// Facing vector of the object.
    FacingVector { obj: u32 },
    /// Vector stored in the object at +0x860 (role inferred: velocity or last move).
    ObjVec860 { obj: u32 },
    /// Vector stored in the object at +0x850 (role inferred).
    ObjVec850 { obj: u32 },
    /// The object's current target record (the thing it is hunting; handle).
    Target { obj: u32 },
    /// World position of a target record.
    TargetPosition { target: u32 },
    /// Last object that hit this one, or the player if there is none.
    LastAttacker { obj: u32 },
    /// Set the lock mode of a target record (0..6 select the game's lock handlers, higher values store directly).
    SetTargetMode { target: u32, obj: u32, mode: i32 },
    /// The current motion has finished (bit 1 of the object's status word).
    MotionDone { obj: u32 },
    /// Object state number at +0x154 (role inferred: motion/behaviour state).
    MotionState { obj: u32 },
    /// Length in frames of a motion (`motion < 0` or the current one: the running motion).
    MotionLength { obj: u32, motion: i32 },
    /// End frame / second duration of a motion (role inferred).
    MotionEnd { obj: u32, motion: i32 },
    /// The object is inside the camera frustum.
    OnScreen { obj: u32 },
    /// The object is in the air (bit 0 of the flag word of its mover; role inferred).
    Airborne { obj: u32 },
    /// The object is busy: hit reaction, knocked or locked (role inferred from FUN_001729f8's flags).
    Busy { obj: u32 },
    /// The object is in a hit reaction (bit 9 of its status word).
    HitStun { obj: u32 },
    /// Spawn parameter `index` (the array at +0x81c; the area script's spawn record). 99 means "none" in Shadow.
    SpawnParam { obj: u32, index: i32 },
    /// Spawn kind byte of the object's spawn record, -1 when it has none.
    SpawnKind { obj: u32 },
    /// Difficulty-like byte read by scripts (global at 0x33e830; role inferred).
    GlobalByte33e830,
    /// Global flag word bit 2 (0x35d268 & 4).
    GlobalFlag4,
    /// Global flag word bit 7 (0x35d268 & 0x80).
    GlobalFlag80,
    /// An event or pause is in progress (global 0x35e3a4 != 0; scripts wait on it before acting).
    EventActive,
    /// A global byte at 0x35ea48+0x1e (FUN_001abee8; role unknown).
    GlobalByte301,
    /// Attack record check (FUN_0018b700 on the byte at +0x26 of a hit record): true when the hit is
    /// a real attack (value >= 2).
    HitIsReal { hit: u32 },
    /// Test of flag bit 0x8 in the object's collision word (FUN_0019aaf0; role unknown).
    Flag124 { obj: u32 },
    /// Test of a status bit of the object (+0x588 & 0x80 after validity check; role unknown).
    Flag309 { obj: u32 },
    /// Probe the ground for a position along a direction. Returns 0 (clear), 1, 2 or 3 (blocked kinds).
    ProbeGround { obj: u32, dir: Vec4, a: f32, b: f32 },

    // ---- motion --------------------------------------------------------------------------
    /// Start motion `motion` with `blend` frames of blending.
    PlayMotion { obj: u32, motion: i32, blend: f32 },
    /// Start a motion unless the game rejects the change (FUN_0017e2d8; role inferred: loop/continue).
    PlayMotionChecked { obj: u32, motion: i32, blend: f32 },
    /// Start a motion at a frame: `start` is the first frame, `blend` the blend frames.
    PlayMotionAt { obj: u32, motion: i32, start: f32, blend: f32 },
    /// Face along `dir` immediately (yaw = atan2(x, z)).
    FaceDirection { obj: u32, dir: Vec4 },
    /// Turn toward `dir` (abrupt turn unless the object is locked, then it stores the target direction).
    FaceToward { obj: u32, dir: Vec4 },
    /// Turn toward `dir` at most `speed` per frame (0 = the object's own turn rate). True while it is still turning.
    TurnToward { obj: u32, dir: Vec4, speed: f32 },

    // ---- movement ------------------------------------------------------------------------
    /// Set the walking direction/speed vector: `dir` is normalised, `speed` goes in w.
    SetMove { obj: u32, dir: Vec4, speed: f32 },
    /// Same for the secondary velocity vector at +0x880.
    SetVelocity { obj: u32, dir: Vec4, speed: f32 },
    /// Set direction, start motion `motion` and (when `commit`) record motion+1/+2 as the following motions.
    StartMove { obj: u32, dir: Vec4, motion: i32, blend: f32, commit: bool },
    /// As StartMove through the alternate setter (FUN_001b4ee0).
    StartMoveAlt { obj: u32, dir: Vec4, motion: i32, blend: f32, commit: bool },
    /// Hide the object at once.
    Hide { obj: u32 },
    /// Fade out over `frames`.
    FadeOut { obj: u32, frames: f32 },
    /// Fade in over `frames`.
    FadeIn { obj: u32, frames: f32 },
    /// Host 1.92 (0x171078 -> FUN_0016b738): the object's drop, row `id` of the prize table `przt`
    /// (FUN_001a3cb0: orbs and munny, item rolls), at the object's lock-on point or the collision in
    /// between. The name is historical.
    GroundMarker { obj: u32, id: i32 },

    // ---- flags and effects ---------------------------------------------------------------
    /// Set bit `bit` of the object's flag array at +0x108.
    SetFlag { obj: u32, bit: i32 },
    ClearFlag { obj: u32, bit: i32 },
    /// Set bit 26 of the object flags (+0x108); scripts use it at event start.
    SetFlag26 { obj: u32 },
    /// Set bit 6 of the word at +0x768.
    SetFlag768 { obj: u32 },
    ClearFlag768 { obj: u32 },
    /// Spawn an effect/child attached to the object; returns its handle.
    SpawnAttached { obj: u32, id: i32, flag: i32, extra: i32 },
    /// Hand an effect handle back to the game (queued for deletion later).
    ReleaseEffect { handle: u32 },
    /// Virtual method 0x44 of the object (role unknown; Shadow brackets its appear timer with 0x44/0x48).
    ObjMethod44 { obj: u32 },
    ObjMethod48 { obj: u32 },
    /// Virtual call on the object's mover (+0xc) with one float (role unknown).
    MoverMethod { obj: u32, value: f32 },
    /// Set an attribute pair on the object (FUN_001b2148; role inferred: attack/parameter slot).
    SetObjParam { obj: u32, a: i32, b: i32 },
    /// Free a target record (FUN_0018f3f8; role inferred).
    FreeTarget { handle: u32 },

    // ---- mode machine hooks (host decides; the machine itself runs in the interpreter) ---
    /// The mode machine must not run now (object flag 0x40000 at +0x160).
    ModeFrozen { obj: u32 },
    /// A mode with priority below 300 is interrupted (FUN_00186b70).
    ModeInterrupted { obj: u32 },
    /// A mode that wants a check may start (FUN_001873d0).
    ModeMayStart { obj: u32 },

    /// Host 1.90 / 1.241 (FUN_00187170, FUN_00187190): a message addressed to the modes named `name` of
    /// every object. `sender` is the object that sent it, 0 for none. The game keeps the message in a
    /// 64-entry queue and delivers it to all objects in a later pass; the host does the same with
    /// [`crate::Runtime::receive_message`], choosing each receiver's delay with [`crate::message_delay`].
    SendMessage { sender: u32, name: String, delay: f32 },
    /// A mode queued with host 6.58 (FUN_00186f50) came up in `obj`'s queue: instead of starting it, the
    /// game switches another object to the mode of that name (FUN_00186db0 on FUN_002f4398(1), role of
    /// that object not determined). The host does it with [`crate::Runtime::set_mode_named`].
    /// Host 1.8 (set_mode) whose context names another object than the caller (the script made it with host 1.0): `obj`
    /// is that object and the host switches it to the mode of that name. (For the queue entry above `obj` is the owner of
    /// the queue.)
    RemoteSetMode { obj: u32, name: String },

    /// Host 1.114, 1.143 and 1.210: find an object. The interpreter stores the answer in the context
    /// (+4), where scripts read "their" object from, so the host only returns the handle (0 when none).
    FindObject { by: ObjectKey, value: i32 },

    /// Any other analysed function (see `names.rs`: kinds, return kind, notes), with its arguments
    /// decoded according to the kinds the function table gives. The function is found by
    /// `kh2_bdx::names::lookup(group, index)`.
    Func { group: u16, index: u16, args: Vec<Arg> },

    /// A registered host function whose behaviour has not been analysed. Returns 0 unless the host knows better.
    Unknown { group: u16, index: u16, args: Vec<u32> },
}

/// The game world as seen by scripts.
pub trait Host {
    /// Performs a call and returns its result (`Value::None` for calls without one).
    fn call(&mut self, c: &Call) -> Value;

    /// Reads memory the script holds a pointer into (game objects, hit records). Return false if unknown.
    fn read_mem(&mut self, _addr: u32, _out: &mut [u8]) -> bool {
        false
    }

    fn write_mem(&mut self, _addr: u32, _data: &[u8]) -> bool {
        false
    }

    /// Host 6.5 (FUN_002f04a8) walks the game's object list (FUN_001b7c88) and keeps the objects that pass
    /// FUN_002f0218 for `kind` (0: +0x588 bit 0x10; 1, 2: bit 4; else any; an object linked to a parent
    /// with +0x588 bit 0x80 never) and FUN_002f03f8 (kind 2 or bit 4, else FUN_0019a5d0: a targetable
    /// object with a target part), without those with +0x108 bit 0x8000 when `skip_flagged`; the host
    /// answers their target parts (FUN_0016b4e0 type 3) in list order with their points (FUN_001894d0).
    /// The runtime does the rest (`crate::search`). `None`: the host keeps no object list; the call then
    /// reaches [`Host::call`] as before.
    fn target_parts(&mut self, _kind: i32, _skip_flagged: bool) -> Option<Vec<crate::search::TargetPart>> {
        None
    }

    /// The answer of a host 6.5 search the runtime made: the record at `record` (the script's or the
    /// host's memory) now names `found` (object, part id), or is cleared (`None`). `walked` is the
    /// block's byte +5.
    fn target_found(&mut self, _record: u32, _found: Option<(u32, u8)>, _walked: u8) {}
}
