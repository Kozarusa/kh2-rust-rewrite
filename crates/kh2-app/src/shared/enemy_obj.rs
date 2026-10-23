//! (Moved out of `enemy_obj.rs`, which re-exports it.) The object a BDX enemy script sees: `Obj` (position and facing, the motion it asked for, flags, fade,
//! movement vector, the records and volumes it was handed) with its handle constants and the game-space
//! conversions. The host that answers the script's calls from it is in `enemy_host.rs`, the Bevy systems that
//! run the script and carry out its requests are in `enemy_ai.rs`.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use kh2_bdx::Vec4;

use crate::motor::Motor;
use crate::reaction_cmd::PlayerReactionCmd;

/// The attack rows `enemy::receive_hits` has seen, by `atkp` id: (original `EnemyReaction`, `Element`). The hit record's byte +0x27 is
/// the row's byte +10 (FUN_0018b618 stores it next to the effective reaction at +0x26; host 2.64 reads it, 0x18e0e0) and host 2.85
/// reads the row's byte +9 (0x18e218). The host answers from here because the hit record of `Obj::hits` carries the id only.
pub static HIT_ROWS: std::sync::Mutex<Vec<(i16, u8, u8)>> = std::sync::Mutex::new(Vec::new());

/// Notes the row a hit named (called for every hit on an enemy).
pub fn note_hit_row(atkp: i16, reaction: u8, element: u8) {
    let mut rows = HIT_ROWS.lock().unwrap_or_else(|e| e.into_inner());
    if !rows.iter().any(|r| r.0 == atkp) {
        rows.push((atkp, reaction, element));
    }
}

/// `(original reaction, element)` of the attack row a hit named, if `note_hit_row` has seen it.
pub fn hit_row_bytes(atkp: i16) -> Option<(u8, u8)> {
    let rows = HIT_ROWS.lock().unwrap_or_else(|e| e.into_inner());
    rows.iter().find(|r| r.0 == atkp).map(|r| (r.1, r.2))
}

/// Object handles the script sees. The scripted object's memory is `[handle, handle + OBJ_SIZE)`.
pub const SELF_BASE: u32 = 0x0100_0000;
pub const PLAYER: u32 = 0x0200_0000;
pub const TARGET: u32 = 0x0300_0000;
pub const EFFECT: u32 = 0x0400_0000;
pub const HIT: u32 = 0x0500_0000;
/// Target records handed to event 11 (0x14 bytes each, built like FUN_0018ed40 + FUN_0018f490:
/// object, position index, 0, -1, 0), one per accepted command.
pub const RECORD: u32 = 0x0600_0000;
pub const RECORD_SIZE: u32 = 0x20;
/// Attack volume objects handed to the clash hook and event 0x13 (0x1b2478), two per clash.
pub const VOLUME: u32 = 0x0700_0000;
pub const VOLUME_SIZE: u32 = 0x40;
/// The word of a volume object that holds its owner (0x1b2478 passes word +8 of the other volume
/// to FUN_001b21a0 as the other side's object). The other fields are not served.
pub const VOLUME_OWNER: u32 = 8;
pub const OBJ_SIZE: usize = 0x1000;
/// The status block of the scripted object (host 1.84: the pointer at object +0x48c).
pub const GAUGES: u32 = 0x0700_0000;
/// Bit 3 of an object's flag array: "cannot be hit" (FUN_0019a4f0).
pub const FLAG_BIT_UNHITTABLE: i32 = 3;
/// Offset of the movement vector (direction and speed in w) in a game object.
pub const MOVE_VECTOR: usize = 0x10;
/// Flag bit 3 of the object's flag array: the object cannot be hit (FUN_0019a4f0 tests it).
pub const FLAG_UNHITTABLE: u32 = 3;

// ---------------------------------------------------------------------------------------------------
// Game space

/// A position in the app (centimetres, Y up) as the script sees it.
pub fn to_game(p: Vec3) -> Vec4 {
    [p.x, -p.y, -p.z, 1.0]
}

/// A direction from the script as an app direction.
pub fn from_game(d: Vec4) -> Vec3 {
    Vec3::new(d[0], -d[1], -d[2])
}

/// App yaw of a script direction: atan2(x, z) in the game's space (atan2 of nothing is 0), then
/// mirrored into the app's.
pub fn yaw_of(d: Vec4) -> f32 {
    let g = if d[0] == 0.0 && d[2] == 0.0 { 0.0 } else { d[0].atan2(d[2]) };
    (g.sin()).atan2(-g.cos())
}

/// The handle of the `serial`-th scripted object (the app's own numbering: `SELF_BASE` plus one `OBJ_SIZE` block
/// each, wrapping after 0x1000 objects). `enemy_ai::ai_attach` hands them out in order and host 1.0 reserves one for
/// the object it creates, which `ObjTable` keeps.
pub fn handle_of_serial(serial: u32) -> u32 {
    SELF_BASE + (serial % 0x1000) * OBJ_SIZE as u32
}

// ---------------------------------------------------------------------------------------------------
// Objects made by host 1.0 and what one object asks of another

/// A ramp of the base object (+0x82c value, +0x830 step; FUN_001abf40 starts it, FUN_001abf78 steps it once per
/// frame): the multiplier of the object's draw alpha (FUN_00193270 = +0x82c * +0x834, FUN_00191c18 writes it into
/// the alpha byte). A new object starts at 1.0 with no step (FUN_00168d30); a kind-6 group's new object (FUN_00199e98)
/// ramps from 0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ramp {
    pub value: f32,
    pub step: f32,
}

impl Ramp {
    pub const STILL: Ramp = Ramp { value: 1.0, step: 0.0 };

    /// FUN_001abf40(a, b, t, out): from `a` to `b` over `t` frames; no time at all is `b` at once.
    pub fn start(a: f32, b: f32, t: f32) -> Ramp {
        if t <= 0.0 { Ramp { value: b, step: 0.0 } } else { Ramp { value: a, step: (b - a) / t } }
    }

    /// FUN_001abf78 with the frame step 1.0 (0x356f40): the value moves by its step and stops at 0 or at `limit`
    /// (0x37b76c, 1.0); false when no step was pending.
    pub fn tick(&mut self, limit: f32) -> bool {
        if self.step == 0.0 {
            return false;
        }
        self.value += self.step;
        if self.value <= 0.0 {
            *self = Ramp { value: 0.0, step: 0.0 };
        } else if self.value >= limit {
            *self = Ramp { value: limit, step: 0.0 };
        }
        true
    }
}

/// What one object can see of another (the snapshot taken at the start of the tick).
#[derive(Clone, Copy, Debug)]
pub struct Other {
    /// Centimetres, the app's space.
    pub pos: Vec3,
    /// The motion slot playing.
    pub slot: Option<usize>,
}

/// A call one object makes on another object (host 1.148, 1.28, 1.70, 1.71 with a handle that is not its own),
/// carried out by `enemy_ai::ai_remote` on the object that owns the handle.
#[derive(Clone, Debug, PartialEq)]
pub enum Remote {
    /// `set_position` (FUN_0016a618): centimetres, the app's space.
    Position(Vec3),
    /// Host 1.8 `set_mode` on the other object's context (FUN_00186db0 on its mode machine at +0x300): the mode it is to
    /// switch to, taken by its script at the start of its next tick (`Obj::modes_asked`).
    Mode(String),
    Hide,
    SetFlag(i32),
    ClearFlag(i32),
}

/// The objects host 1.0 made and the calls waiting for their owners: `enemy_ai::ai_update` fills it after each script
/// tick, `enemy::spawn_created` makes the objects, `enemy_ai::ai_remote` applies the calls.
#[derive(Resource, Default)]
pub struct ObjTable {
    /// The next object serial (`handle_of_serial`).
    pub serial: u32,
    /// Objects asked for and not yet made: handle and `00objentry` id.
    pub creates: Vec<(u32, u32)>,
    /// Calls on objects by handle, with the ticks they have waited for the owner to exist.
    pub remote: Vec<(u32, Remote, u8)>,
    /// The `00objentry` ids of the enemy kinds the app has, by id (filled with the scripts).
    pub kinds: HashMap<u32, &'static crate::shared::enemy::Kind>,
    /// The keys of `kinds`, shared with every object (`Obj::creatable`).
    pub creatable: Arc<HashSet<u32>>,
}

/// The handle of an object host 1.0 made: its brain takes it (`enemy_ai::ai_attach`) instead of the next serial's.
#[derive(Component)]
pub struct CreatedHandle(pub u32);

/// A new object of a kind-6 group (FUN_00199e98) fades in: its brain starts the appear ramp (`AppearRamp`).
#[derive(Component)]
pub struct Appearing;

/// The numbers of the kind-6 appear ramp, read from the executable: FUN_00199e98 calls FUN_001abf40(0, DAT_0037b308,
/// DAT_0037b30c, object + 0x82c) on the object it made, and FUN_001abf78 steps the ramp once per frame up to DAT_0037b76c.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct AppearRamp {
    pub target: f32,
    pub frames: f32,
    pub limit: f32,
}

impl AppearRamp {
    pub fn from_exe(elf: &[u8]) -> Option<AppearRamp> {
        let f = |va| crate::reaction_rules::exe_f32(elf, va);
        Some(AppearRamp { target: f(0x37b308)?, frames: f(0x37b30c)?, limit: f(0x37b76c)? })
    }

    /// The ramp a kind-6 object starts with (FUN_001abf40(0, target, frames)).
    pub fn start(&self) -> Ramp {
        Ramp::start(0.0, self.target, self.frames)
    }
}

/// Ticks a call on an object that does not exist (yet) is kept: the object made in this tick exists in the next.
pub const REMOTE_PATIENCE: u8 = 3;

// ---------------------------------------------------------------------------------------------------
// The scripted object

pub struct Fade {
    pub from: f32,
    pub to: f32,
    pub frames: f32,
    pub t: f32,
}

pub struct MotionReq {
    pub slot: usize,
    pub blend: f32,
    pub start: Option<f32>,
}

/// What the script sees of its object, plus what it has asked for during this tick.
pub struct Obj {
    pub handle: u32,
    /// Position (centimetres, Y up), facing, ground contact and the jump model.
    pub motor: Motor,
    pub hp: i32,
    /// A hit reaction is playing (`EState::Hurt` or `Blown`).
    pub reacting: bool,
    pub on_screen: bool,
    /// The motion slot playing, as the script left it or the reaction code changed it (the game's +0x154).
    pub slot: Option<usize>,
    /// The motion reached its end during the last tick (looping motions: each time they wrap).
    pub done: bool,
    pub flags: u64,
    pub flags768: bool,
    pub mem: Vec<u8>,
    /// Horizontal movement set by `set_move` / the object's memory, centimetres per tick; used for one tick.
    pub move_vel: Vec3,
    /// Horizontal speed given to `start_move`, used while airborne.
    pub air_vel: Vec3,
    pub alpha: f32,
    pub fade: Option<Fade>,
    pub hidden: bool,
    pub motion_req: Option<MotionReq>,
    /// Motions the jump started by `start_move` (with the commit flag) plays: falling, landing.
    pub fall_motion: i32,
    pub land_motion: i32,
    pub fall_pending: bool,
    pub effects: Vec<u16>,
    pub next_effect: u32,
    /// Effect handles the script destroyed with host 0.51 (`FUN_001da028`: the handle's instance pointer +0x1c is cleared); host 0.49 / 0.50 read them as gone.
    pub effects_gone: Vec<u32>,
    /// Hit records of event 6 by handle: hit is real (2.8) and the effective reaction, byte +0x26 (2.7).
    pub hits: HashMap<u32, (bool, u8, i16)>,
    pub next_hit: u32,
    /// Target records given to event 11, readable by the script (handle `RECORD + k * RECORD_SIZE`).
    pub records: Vec<[u8; 20]>,
    /// What the script asked of the player this tick (host calls on the player's context).
    pub player_cmds: Vec<PlayerReactionCmd>,
    /// The player's +0x158 after this tick's `play_motion_with`, if it changed.
    pub player_serial: Option<i32>,
    /// The spawn kind byte of the object's spawn record (host 1.110), -1 without one.
    pub spawn_kind: i32,
    /// Messages the script sent this tick (host 1.90 / 1.241): sender, mode name, delay in frames.
    pub outbox: Vec<(u32, String, f32)>,
    /// Volume objects of clashes (handle `VOLUME + k * VOLUME_SIZE`): owner and attack row.
    pub volumes: Vec<(u32, i16)>,
    /// Prize rows of host 1.92 calls this tick.
    pub drops: Vec<i32>,
    /// Host 6.31 ran this tick (FUN_002eece8: every prize and item collected by the player).
    pub collect_all: bool,
    /// Spawn parameters 1 and 2 (object +0x820, +0x824, copied from the record by FUN_0016b870).
    pub spawn_params: Option<[i32; 2]>,
    /// The `enmp` row of this tick's `set_obj_param` when it is not the one the stats were built from.
    pub param_row: Option<u16>,
    /// The HP record of part 0 of the status block (+0x48c, FUN_0019f770: current, maximum, lowest):
    /// the maximum and the lowest as the enemy has them, and the change the script asked for this tick.
    pub hp_max: i32,
    pub hp_min: i32,
    /// Host 1.311 calls of this tick: damage-scale index and percent (applied to `Stats::weakness` by `enemy_ai`).
    pub weak_scale: Vec<(usize, i32)>,
    /// Host 1.13 on this object: motions (slot, blend) queued behind the playing one, at most 4 (the game's ring at the
    /// motion controller +0x3c); `enemy_ai` starts the first when the playing motion ends.
    pub motion_queue: Vec<(usize, f32)>,
    pub hp_add: i32,
    /// The word at object +0xabc (host 1.211 / 1.212).
    pub flags_abc: u32,
    /// The motion controller's (+0x140) trigger bytes +0xf8 and +0xf9 (object +0x238, +0x239): bits 0..3
    /// of +0xf8 while range triggers 16..19 are active (FUN_00188030; cleared at the start of each pass,
    /// FUN_00187fb0), bits 0..3 of +0xf9 when frame triggers 9..12 fired in the last advance (the frame
    /// trigger dispatcher 0x188498, cases at 0x1886c0; cleared by FUN_0017e3b8 before it advances).
    pub trigger_bits: [u8; 2],
    /// Mover type 5, installed by `set_dash` (host 1.58): its velocity (+0x20, the app's space,
    /// centimetres per tick) and its factor (+0xf0).
    pub dash: Option<(Vec3, f32)>,
    /// The motion controller's time (+0x140 + 0x30 = object +0x170), in the motion's own frames: the
    /// `Animator` frame at the start of the tick, or the first frame of a motion requested in it.
    pub frame: f32,
    /// Blend time left (the controller's +0x34), in ticks: the blend of the last request, run down
    /// one per tick.
    pub blend_left: f32,
    /// The last tick's movement as FUN_00181e00 keeps it (the app's space, centimetres per tick):
    /// what the object asked for (+0x850) and how far it really went (+0x860, new position minus
    /// old, after collision).
    pub want_vel: Vec3,
    pub moved: Vec3,
    /// The ramp at +0x82c (`Ramp`): the draw alpha is `alpha` times its value.
    pub appear: Ramp,
    /// The ramp at +0x8c8 (value, step) that host 1.125 starts (FUN_00173858 -> FUN_001abf40) and FUN_00192bf0 steps once per frame (FUN_001abf78); a change calls FUN_00192d00, which stores the value (held 0 to a limit, times DAT_0037ae90) as a byte at object +0x6a0. The byte's reader was not traced and the app draws nothing from it; the limit is taken as 1.0 [inferred]. Starts at 0 [inferred].
    pub tween: Ramp,
    /// The object's own target record (host 2.23, object +0x9f4: object, part index, 0, -1, 0). A script may store into it (BDX `M_EX870` R0a50 stores the word 6.57 answers at +4); reads
    /// answer these bytes. Writer: `enemy_host::BevyHost::write_mem`; reader: `read_mem`.
    pub target_rec: [u8; 20],
    /// The `00objentry` ids host 1.0 makes objects of (empty: host 1.0 is not served, `Missing`).
    pub creatable: Arc<HashSet<u32>>,
    /// The next object serial a created object takes (`ObjTable::serial`, lent for the tick).
    pub next_serial: u32,
    /// Objects this tick's host 1.0 calls made: handle and `00objentry` id.
    pub creates: Vec<(u32, u32)>,
    /// Calls this tick made on other objects, in order.
    pub remote: Vec<(u32, Remote)>,
    /// Modes the object that made this one asked for (host 1.8 on this object's context), switched to by its script at the
    /// start of its next tick (`enemy_ai::ai_update`).
    pub modes_asked: Vec<String>,
    /// The other scripted objects as they stood at the start of the tick.
    pub others: Arc<HashMap<u32, Other>>,
}

impl Obj {
    pub fn new(handle: u32) -> Obj {
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
            effects_gone: Vec::new(),
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
            weak_scale: Vec::new(),
            motion_queue: Vec::new(),
            hp_add: 0,
            flags_abc: 0,
            trigger_bits: [0; 2],
            dash: None,
            frame: 0.0,
            blend_left: 0.0,
            want_vel: Vec3::ZERO,
            moved: Vec3::ZERO,
            appear: Ramp::STILL,
            tween: Ramp { value: 0.0, step: 0.0 },
            target_rec: crate::shared::enemy::target_record(),
            creatable: Arc::default(),
            next_serial: 0,
            creates: Vec::new(),
            remote: Vec::new(),
            modes_asked: Vec::new(),
            others: Arc::default(),
        }
    }

    /// The draw alpha: the script's fade times the appear ramp (FUN_00193270).
    pub fn shown_alpha(&self) -> f32 {
        self.alpha * self.appear.value
    }

    /// Whether `handle` names an object this one can call: one that existed at the start of the tick or that this tick
    /// made (FUN_0019a3f8 finds it in the object list; the game's object exists from the call on).
    /// Hosts 0.49 effect_alive (`FUN_001da0d8`) and 0.50 effect_valid (`FUN_001da0e8`, handle +0x1c != 0): an effect handle this object made
    /// is live until the script destroys it with 0.51 [Approx: an effect that ran to its end is still read as live].
    pub fn effect_live(&self, handle: u32) -> bool {
        handle > EFFECT && handle <= EFFECT + self.next_effect && !self.effects_gone.contains(&handle)
    }

    /// Host 0.51 effect_destroy (`FUN_001da028`): the handle's instance is dropped.
    pub fn destroy_effect(&mut self, handle: u32) {
        if !self.effects_gone.contains(&handle) {
            self.effects_gone.push(handle);
        }
    }

    pub fn knows(&self, handle: u32) -> bool {
        self.others.contains_key(&handle) || self.creates.iter().any(|c| c.0 == handle)
    }

    /// Where a made-this-tick object stands: the point of the last `set_position` the script gave it, else the
    /// origin where host 1.0 puts it (the place record at 0x1c883a0 is zero).
    pub fn made_at(&self, handle: u32) -> Vec3 {
        self.remote.iter().rev().find_map(|(h, r)| match r {
            Remote::Position(p) if *h == handle => Some(*p),
            _ => None,
        }).unwrap_or(Vec3::ZERO)
    }

    /// Host 1.8 on the context of another object this one can call (a made one, `knows`): queued to be carried out on it
    /// (`enemy_ai::ai_remote`). False for a handle that is this object's own or unknown.
    pub fn ask_mode(&mut self, target: u32, name: &str) -> bool {
        if target == self.handle || !self.knows(target) {
            return false;
        }
        self.remote.push((target, Remote::Mode(name.to_string())));
        true
    }

    /// Host 1.0: makes an object of `00objentry` id `object`. Its handle is the next serial's (the serial counts up
    /// before it is used, as `enemy_ai::ai_attach` does).
    pub fn create(&mut self, object: u32) -> u32 {
        self.next_serial += 1;
        let handle = handle_of_serial(self.next_serial);
        self.creates.push((handle, object));
        handle
    }

    /// A target record for event 11 (FUN_0018ed40 then FUN_0018f490: object, position index, 0, -1,
    /// and a fifth word the manager copies, 0 here). Returns its handle.
    pub fn new_record(&mut self, obj: u32, pos: i16) -> u32 {
        let mut r = [0u8; 20];
        r[0..4].copy_from_slice(&obj.to_le_bytes());
        r[4..8].copy_from_slice(&(pos as i32).to_le_bytes());
        r[12..16].copy_from_slice(&(-1i32).to_le_bytes());
        self.records.push(r);
        RECORD + (self.records.len() as u32 - 1) * RECORD_SIZE
    }

    /// The object a record of this object's list names (word 0), by the record's handle.
    pub fn record_object(&self, handle: u32) -> Option<u32> {
        let r = handle.wrapping_sub(RECORD);
        let rec = self.records.get((r / RECORD_SIZE) as usize).filter(|_| r.is_multiple_of(RECORD_SIZE))?;
        Some(u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]))
    }

    /// FUN_0018f490 on a record of this object's list: it names `obj` (position index 0). False when `handle` is no record.
    pub fn set_record_object(&mut self, handle: u32, obj: u32) -> bool {
        let r = handle.wrapping_sub(RECORD);
        let Some(rec) = self.records.get_mut((r / RECORD_SIZE) as usize).filter(|_| r.is_multiple_of(RECORD_SIZE)) else { return false };
        rec[0..4].copy_from_slice(&obj.to_le_bytes());
        rec[4..8].copy_from_slice(&0u32.to_le_bytes());
        true
    }

    /// The two volume objects of a clash: the object's own attack volume and the player's (the only
    /// other team in the app). Returns their handles (own, other).
    pub fn new_volumes(&mut self, own_atkp: i16, other_atkp: i16) -> (u32, u32) {
        let k = self.volumes.len() as u32;
        self.volumes.push((self.handle, own_atkp));
        self.volumes.push((PLAYER, other_atkp));
        (VOLUME + k * VOLUME_SIZE, VOLUME + (k + 1) * VOLUME_SIZE)
    }

    /// Word +8 (the owner) of a clash volume object.
    pub fn volume_word(&self, addr: u32, out: &mut [u8]) -> bool {
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

    pub fn advance_fade(&mut self) {
        self.tween.tick(1.0);
        if let Some(f) = &mut self.fade {
            f.t += 1.0;
            let k = if f.frames <= 0.0 { 1.0 } else { (f.t / f.frames).min(1.0) };
            self.alpha = f.from + (f.to - f.from) * k;
            if k >= 1.0 {
                self.fade = None;
            }
        }
    }

    pub fn set_move(&mut self, dir: Vec4, speed: f32) {
        let d = from_game(dir).normalize_or_zero();
        self.move_vel = Vec3::new(d.x, 0.0, d.z) * speed;
    }
}
