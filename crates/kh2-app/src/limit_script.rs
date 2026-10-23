//! The limit BAR's own script (type 3 entry of `limit/us/<file>`), run as a BDX thread. See docs/systems/drive-limits-summons/limits.md "Limit script".
//!
//! `FUN_001b2e08` looks the entry up (`FUN_001684c0(bar, 3, 0)`), starts a script thread on it (`FUN_0016c9b8`) and sends it event 22
//! with the limit task as the argument (`limit::start_events`); `FUN_001b28a0`'s first branch sends event 23 with a command id
//! (`limit::sub_command_event`). The game then runs the thread every frame with the other tasks (`FUN_0016c680`).
//!
//! The app runs that thread the way the other script owners do (`kh2_bdx::Runtime`): `start_thread(obj, 22, [task])` at the limit's
//! start, `restart_thread(23, [command])` for a sub-command, `update` once per 60 Hz tick while the app's countdown runs. The countdown
//! stays the end condition (the task's end, `FUN_001b3970`, waits for HUD animations, a gap). The host answers only what a call
//! needs to be answered and logs every call it does not port; a broken script (a VM error or a panic) is logged once and its thread
//! is dropped: the limit itself never depends on it.
//!
//! Logs: `limit script: start`, `limit script: first call host g.i`, `limit script: ended`, `limit script: fault`.

use std::collections::BTreeMap;

use bevy::prelude::*;
use kh2_bdx::{Call, Host, HostTable, Program, Runtime, Value};

use crate::assets::Disc;
use crate::limit::{LimitArg, LimitReceiver, LimitScriptEvent, LimitStarted, LimitTask};
use crate::schedule::Sim;
use crate::sora::Sora;

/// The stand-in for the limit task object that event 22 carries (word `+0xc` the player, `+0x10` the first participant). The same
/// value summon.rs uses for its `LIMIT_ATTACK` handle, so a script that reads the task through hosts 2.48 / 2.49 sees one object.
pub const TASK: u32 = 0x7a00_0000;

/// What one host function was asked, over the run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallStat {
    pub count: u32,
    /// The tick of the first call.
    pub first_tick: u32,
}

/// Why a script thread stopped being run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The thread ended by itself (it returned).
    Returned,
    /// A VM error ended it (`Runtime::errors`), or the interpreter panicked.
    Fault(String),
}

/// The limit task fields the script reads or changes through hosts, as the app keeps them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskState {
    /// The task's time gauge value (`+0xd4`, host 2.60): the gauge `FUN_001b2e08` fills with the command's `Time`
    /// (`FUN_001cfc28(Time, task + 0xa8)`) and `FUN_001b35e8` lowers every frame. The app's countdown is that gauge: its ticks left.
    pub remaining: f32,
    /// Count of started HUD animations (`+0x3c`, host 2.63): `FUN_001b3a70` adds one and sets bit 8 of `+0x40`; it runs for the
    /// limit whose command is 0x1e4 (Trinity) at the task's creation. The animation itself is not in the app.
    pub animations: i32,
    /// Bit 8 of `+0x40` (set with the count, cleared by host 2.89).
    pub animation_flag: bool,
    /// The participants (task `+0x10`, `+0x14`, `+0x18`; host 2.49), as character ids: `FUN_001b2c10` walks the limit row's three
    /// participant bytes (row `+1` to `+3`) and, for each byte above 1, stores the party member object `FUN_0017d400(byte)` in the
    /// next slot. The app has no party member objects, so each is a stand-in handle ([`partner`]).
    pub participants: Vec<u32>,
    /// The player's position as the scripts read it (host 1.147, game space), set each tick from Sora's motor.
    pub player: [f32; 4],
    /// Sora's yaw (radians), set with `player`.
    pub player_yaw: f32,
    /// Sora's velocity as a game-space vector (object `+0x20`, read directly by the scripts; see [`ObjectMem`]), set each tick.
    pub player_velocity: [f32; 4],
    /// The memory the scripts read and write through object pointers.
    pub mem: ObjectMem,
    /// Hosts 2.47, 2.50 and 2.59 as the app took them (the participant's motion setup, effect start and motion table lookup).
    pub requests: Vec<Request>,
    /// The participant the next host 2.47 call acts on, read from the script's memory by `call_with_mem` before the call.
    next_who: u32,
}

/// One call of hosts 2.47 / 2.50 / 2.59: the app has nothing to play for a stand-in participant, so the call is kept as the request
/// the original acted on.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Host 2.47 (`0x18e988` -> `FUN_001b3438` -> `FUN_0017f0c0`): a motion started on a participant's motion controller. `args` are the
    /// call's raw arguments (the task, the pointer to the context that holds the participant, the motion id, the blend as float bits);
    /// `who` is the participant handle the word at `context + 4` holds (`FUN_0018e988` reads `*(arg1 + 4)`), 0 when it could not be read.
    Motion { args: Vec<u32>, who: u32 },
    /// Host 2.50 (`0x18e9f8` -> `FUN_001b3468`): an effect started on the participant (the object, the effect record, the scale and
    /// the flag `!= 0`).
    Effect { args: Vec<u32> },
    /// Host 2.59 (`0x18eb00` -> `FUN_001b3c48` -> `FUN_001db868`, the function the clash spark starts a system effect with): an element of
    /// the limit file's own PAX (the type 0x12 entry, kept in the table at task `+0x18` by `FUN_001b2e08`'s `FUN_001db4d0`) started with
    /// the participant `who` (the word at `context + 4`) as its owner. `args` are (the task, the element id, the context pointer, a flag).
    PaxElement { args: Vec<u32>, who: u32 },
}

/// The stand-in for the pointer host 2.56 (`ptr_b40_4`, `0x1706f8`, "the pointer at object `+0xb40` plus 4") answers: the player's
/// target controller object has its current target handle at `+4`, and the scripts read that word.
pub const TARGET_SLOT: u32 = 0x7a30_0000;

/// The stand-in for the record host 2.23 answers (the object's own target record, object `+0x9f4`): five words, the player as the
/// object (as the enemy host's record `enemy_host::target_record`).
pub const OWN_RECORD: u32 = 0x7a40_0000;

/// Host 1.131 copies a record into the pool at `DAT_01c89bd0 + 0x14 * index` and answers its address; the stand-in pool starts here.
pub const RECORD_POOL: u32 = 0x7a50_0000;
/// Bytes of a target record (five words).
const RECORD_BYTES: u32 = 0x14;
/// Records the pool holds (the game's pool is indexed by a stack of free slots, `DAT_01c8a5d0`; the app never reuses one).
const POOL_RECORDS: u32 = 0x100;

/// The memory behind the object handles a limit script reads directly (Simba and Tron read 16 bytes at the player object's `+0x20`
/// through the context, words `0xc4b` / `0x316`, the object's velocity vector [inferred from the enemy scripts' use of that offset, not from a reader in these scripts]: host 1.58 stores the
/// dash direction there, the Quick Run start its velocity). The image is kept per handle; writes are stored and read back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectMem {
    written: BTreeMap<u32, u8>,
    /// Records filed by host 1.131 so far.
    filed: u32,
}

/// Offset of the velocity vector in an object.
pub const VELOCITY_OFFSET: u32 = 0x20;
/// Bytes of an object image the scripts may read.
const OBJECT_SPAN: u32 = 0x100;

impl ObjectMem {
    /// The handle an address belongs to and its offset: the player, a participant stand-in, or the target slot (offset 0 only).
    fn locate(addr: u32) -> Option<(u32, u32)> {
        if addr.wrapping_sub(OWN_RECORD) < RECORD_BYTES {
            return Some((OWN_RECORD, addr - OWN_RECORD));
        }
        if addr.wrapping_sub(RECORD_POOL) < RECORD_BYTES * POOL_RECORDS {
            return Some((RECORD_POOL, addr - RECORD_POOL));
        }
        let base = [crate::enemy_obj::PLAYER, TARGET_SLOT];
        if let Some(b) = base.into_iter().find(|&b| addr >= b && addr - b < OBJECT_SPAN) {
            return Some((b, addr - b));
        }
        (PARTNER_BASE..EFFECT_SLOT).contains(&addr).then(|| (addr - addr % OBJECT_SPAN, addr % OBJECT_SPAN))
    }

    /// Reads `out.len()` bytes at `addr`; false for an address that is no object of the limit.
    pub fn read(&self, addr: u32, task_velocity: [f32; 4], out: &mut [u8]) -> bool {
        let Some((_, off)) = Self::locate(addr) else { return false };
        if addr.wrapping_sub(TARGET_SLOT) < OBJECT_SPAN {
            // The target controller's word: no target (0) [inferred: the app's lock-on is not an object of this host].
            out.fill(0);
            return true;
        }
        let own = addr.wrapping_sub(OWN_RECORD) < RECORD_BYTES;
        let is_object = addr.wrapping_sub(RECORD_POOL) >= RECORD_BYTES * POOL_RECORDS && !own;
        for (i, b) in out.iter_mut().enumerate() {
            let a = addr + i as u32;
            let o = off + i as u32;
            *b = self.written.get(&a).copied().unwrap_or_else(|| {
                if own {
                    return crate::enemy_host::target_record().get((a - OWN_RECORD) as usize).copied().unwrap_or(0);
                }
                let v = o.wrapping_sub(VELOCITY_OFFSET) as usize;
                if is_object && v < 16 { task_velocity[v / 4].to_le_bytes()[v % 4] } else { 0 }
            });
        }
        true
    }

    /// Host 1.131 (`FUN_0018f3f8`): files the record at `record` in the pool and answers the copy's address (0 when the pool is full or
    /// the address holds no record).
    pub fn file_record(&mut self, record: u32) -> u32 {
        let mut bytes = [0u8; RECORD_BYTES as usize];
        if self.filed >= POOL_RECORDS || !self.read(record, [0.0; 4], &mut bytes) {
            return 0;
        }
        let at = RECORD_POOL + RECORD_BYTES * self.filed;
        self.filed += 1;
        self.write(at, &bytes);
        at
    }

    /// Stores `data` at `addr`; false for an address that is no object of the limit.
    pub fn write(&mut self, addr: u32, data: &[u8]) -> bool {
        if Self::locate(addr).is_none() {
            return false;
        }
        for (i, &b) in data.iter().enumerate() {
            self.written.insert(addr + i as u32, b);
        }
        true
    }
}

/// The stand-in handle of the party member object of character `id` (an object only this module and its host know; its memory is
/// [`ObjectMem`], one [`OBJECT_SPAN`] bytes per character id).
pub const PARTNER_BASE: u32 = 0x7a10_0000;

/// The stand-in handle of the object in task `+0x38` (host 2.55).
pub const EFFECT_SLOT: u32 = 0x7a20_0000;

/// The stand-in handle for character `id`.
pub fn partner(id: u32) -> u32 {
    PARTNER_BASE + id * OBJECT_SPAN
}

/// The participants a limit row's three bytes name, in slot order (`FUN_001b2c10`: bytes above 1 only, packed).
pub fn participants_of(members: [u8; 3]) -> Vec<u32> {
    members.iter().filter(|&&m| m > 1).map(|&m| u32::from(m)).collect()
}

impl TaskState {
    /// What the task creation leaves for the limit of `command` (`FUN_001b2e08`: `FUN_001b3a70(task, 3)` for command 0x1e4).
    pub fn new(command: u16, remaining: f32) -> TaskState {
        let trinity = command == 0x1e4;
        TaskState { remaining, animations: i32::from(trinity), animation_flag: trinity, participants: Vec::new(), player: [0.0, 0.0, 0.0, 1.0], player_yaw: 0.0, player_velocity: [0.0; 4], mem: ObjectMem::default(), requests: Vec::new(), next_who: 0 }
    }

    /// Host 2.89 (`FUN_001b3b60`): when bit 8 is set, stop the animation (the call through the player's `+0xb44` object is a gap
    /// [no HUD gauge in the app]), lower the count by one and clear the bit.
    fn release_animation(&mut self) {
        if self.animation_flag {
            self.animations -= 1;
            self.animation_flag = false;
        }
    }
}

/// The host a limit script talks to. It ports the calls whose behaviour is a game rule the app already has, answers the rest 0, and
/// records every call (`group.index`, count) so the log lists what a script uses.
struct LimitHost<'a> {
    calls: &'a mut BTreeMap<(u16, u16), CallStat>,
    task: &'a mut TaskState,
    tick: u32,
    /// Lines for the log: the first call of each host function, with its arguments.
    fresh: &'a mut Vec<String>,
}

impl Host for LimitHost<'_> {
    fn call(&mut self, c: &Call) -> Value {
        let id = crate::enemy_host::call_id(c);
        let tick = self.tick;
        let stat = self.calls.entry(id).or_insert_with(|| CallStat { count: 0, first_tick: tick });
        stat.count += 1;
        if stat.count == 1 {
            let name = kh2_bdx::names::host_name(id.0, id.1).unwrap_or("?");
            self.fresh.push(format!("limit script: first call host {}.{} {name} at tick {tick}: {c:?}", id.0, id.1));
        }
        host_answer(c, self.task)
    }

    fn call_with_mem(&mut self, c: &Call, mem: &mut dyn kh2_bdx::host::ScriptMem) -> Value {
        match crate::enemy_host::call_id(c) {
            (1, 160) => {
                self.call(c);
                host_1_160(&args_of(c), mem, &mut self.task.mem);
                Value::None
            }
            (2, 47) | (2, 59) => {
                // `FUN_0018e988` / `FUN_0018eb00`: the participant is the word at +4 of the block the argument points at (the second
                // argument of 2.47, the third of 2.59).
                let at = if crate::enemy_host::call_id(c) == (2, 59) { 2 } else { 1 };
                self.task.next_who = args_of(c).get(at).and_then(|&p| word_at(p.wrapping_add(4), mem, &self.task.mem, [0.0; 4])).unwrap_or(0);
                self.call(c)
            }
            (6, 57) => {
                self.call(c);
                host_6_57(&args_of(c), mem, &self.task.mem, self.task.player_velocity)
            }
            _ => self.call(c),
        }
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        self.task.mem.read(addr, self.task.player_velocity, out)
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        self.task.mem.write(addr, data)
    }
}

/// The word at `addr` in the script's memory, else in the limit's object memory.
fn word_at(addr: u32, script: &mut dyn kh2_bdx::host::ScriptMem, objects: &ObjectMem, velocity: [f32; 4]) -> Option<u32> {
    script.read_u32(addr).or_else(|| {
        let mut b = [0; 4];
        objects.read(addr, velocity, &mut b).then(|| u32::from_le_bytes(b))
    })
}

/// Host 1.160 (0x173770 -> `FUN_0018f490(record, word at context + 4, 0)`; arguments `record`, `context`): empties the target record
/// at `record` (`FUN_0018ed40`: object 0, words 1 and 2 zero, word 3 = -1) and files the object the context holds at `+4` (where
/// the object lookups 1.114 / 1.210 store their answer) as its object, part 0. The record is the script's own memory (a stack
/// local) or a record of the limit's object memory.
fn host_1_160(args: &[u32], script: &mut dyn kh2_bdx::host::ScriptMem, objects: &mut ObjectMem) {
    let (Some(&record), Some(&context)) = (args.first(), args.get(1)) else { return };
    let object = word_at(context.wrapping_add(4), script, objects, [0.0; 4]).unwrap_or(0);
    let bytes: Vec<u8> = [object, 0, 0, u32::MAX].iter().flat_map(|w| w.to_le_bytes()).collect();
    if !script.write(record, &bytes) {
        objects.write(record, &bytes);
    }
}

/// Host 6.57 (0x2eec10, no decomp file; read from the instructions: `lw t7,0(a0); lw t6,4(t7); sw t6,0(a0)`): answers the word at
/// `+4` of the block the argument points at, a context or record. 0 when the address is in neither memory.
fn host_6_57(args: &[u32], script: &mut dyn kh2_bdx::host::ScriptMem, objects: &ObjectMem, velocity: [f32; 4]) -> Value {
    let Some(&block) = args.first() else { return Value::Int(0) };
    Value::Int(word_at(block.wrapping_add(4), script, objects, velocity).unwrap_or(0) as i32)
}

/// The raw arguments of a call.
fn args_of(c: &Call) -> Vec<u32> {
    match c {
        Call::Func { args, .. } => args.iter().map(|a| a.int() as u32).collect(),
        Call::Unknown { args, .. } => args.clone(),
        _ => Vec::new(),
    }
}

/// The first argument of a call as an integer (a handle), 0 without one.
fn first_arg(c: &Call) -> u32 {
    match c {
        Call::Func { args, .. } => args.first().map_or(0, |a| a.int() as u32),
        Call::Unknown { args, .. } => args.first().copied().unwrap_or(0),
        _ => 0,
    }
}

/// The position a script reads for an object handle (see host 1.147).
fn position_of(who: u32, task: &TaskState) -> [f32; 4] {
    if who == crate::enemy_obj::PLAYER || (PARTNER_BASE..EFFECT_SLOT).contains(&who) {
        task.player
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// The answers of the ported calls (addresses in docs/systems/drive-limits-summons/limits.md "Limit script"); every other call is answered 0.
fn host_answer(c: &Call, task: &mut TaskState) -> Value {
    let on_task = first_arg(c) == TASK;
    // 1.114 / 1.210 (bind_object, 0x171d10 / 0x171d70) reach the host as `FindObject` and write the answer into the script's context.
    // 1.210 looks the object up by its NeoStatus value (`FUN_0016b138`), which for a party member is its character id [inferred from the
    // scripts binding 2 and 3]; the lookup finds the participants only. 1.114 (entity ids) finds the player (0x236) and nothing else.
    if let Call::FindObject { by, value } = c {
        return match by {
            kh2_bdx::host::ObjectKey::NeoStatus if u32::try_from(*value).is_ok_and(|v| task.participants.contains(&v)) => Value::Handle(partner(*value as u32)),
            kh2_bdx::host::ObjectKey::Entity if *value == 0x236 => Value::Handle(crate::enemy_obj::PLAYER),
            _ => Value::Handle(0),
        };
    }
    if let Call::FreeTarget { handle } = c {
        // 1.131 (0x1736f8): files the record in the pool and answers the copy's address.
        return Value::Handle(task.mem.file_record(*handle));
    }
    match crate::enemy_host::call_id(c) {
        // 2.49 (0x18e9d8): the word at +0x10 + 4 * i, the i-th participant (0 when the limit has fewer).
        (2, 49) if on_task => {
            let i = match c {
                Call::Func { args, .. } => args.get(1).map_or(0, |a| a.int()),
                Call::Unknown { args, .. } => args.get(1).copied().unwrap_or(0) as i32,
                _ => 0,
            };
            Value::Handle(usize::try_from(i).ok().and_then(|i| task.participants.get(i)).map_or(0, |&id| partner(id)))
        }
        // Debug prints (empty in the retail build; the mission host treats them alike).
        (0, 0..=2) | (0, 8) | (0, 24..=26) => Value::None,
        // 2.48 (0x18e9c8): the word at +0xc of the object, the task's player (`FUN_001b2c10` stores its fourth argument there).
        (2, 48) if on_task => Value::Handle(crate::enemy_obj::PLAYER),
        // 2.55 (0x18ea50): the word at +0x38, the object `FUN_001afe78(0)` made at the end of `FUN_001b2c10` (a slot of the pool at
        // `DAT_01d58308`, never null once the task exists). Its slot is an effect object the app does not draw: a stand-in handle.
        (2, 55) if on_task => Value::Handle(EFFECT_SLOT),
        // 7.3 (0x2ef370 -> FUN_002efe30): resets the pool of 30 slots at `DAT_01e356a0` (every slot free, free count 0x1e). The app
        // keeps no such pool, which is the state the reset leaves.
        (7, 3) => Value::None,
        // 7.11 (0x2ef480 -> FUN_002effd8): true when the pool's free count is 0x1e, i.e. every slot is free: always, in the app.
        (7, 11) => Value::Bool(true),
        // 7.21 (0x2ef798 -> FUN_002f3a08): copies the five words the argument points at to `DAT_00371400`; its readers are not in the
        // decompiled code (gap), so nothing in the app reads the record.
        (7, 21) => Value::None,
        // 2.56 (0x18de58, `ptr_b40_4_b`): the pointer at the object's +0xb40 plus 4, the word the target controller keeps its target in.
        // 2.23 (0x18ddb0): the object's own target record (+0x9f4). 1.131 (0x1736f8): files a record in the pool, answers the copy.
        (2, 23) => Value::Handle(OWN_RECORD),
        (2, 56) => Value::Handle(TARGET_SLOT),
        // 2.47 (0x18e988): the participant's motion controller setup (`FUN_001b3438` -> `FUN_0017f0c0`). A stand-in has no motion
        // controller, so the request is kept and the call answers 0 [inferred: the result slot is not read by the scripts that log it].
        (2, 47) => {
            task.requests.push(Request::Motion { args: args_of(c), who: task.next_who });
            Value::Int(0)
        }
        // 2.50 (0x18e9f8 -> FUN_001b3468): an effect start on the participant; nothing is drawn, the request is kept.
        (2, 50) => {
            task.requests.push(Request::Effect { args: args_of(c) });
            Value::None
        }
        // 2.59 (0x18eb00 -> FUN_001b3c48 -> FUN_001db868): the element of the limit file's PAX whose halfword id (row +2) is the
        // argument starts as an effect (`FUN_001db5f0`); the app keeps the request and `limit_visual` starts it. The answer (the effect's
        // handle in the original) is 0: no script of the 15 files is known to use it.
        // Host 2.56 (0x18de58) has no decomp file; its address rests on names.rs.
        (2, 59) => {
            task.requests.push(Request::PaxElement { args: args_of(c), who: task.next_who });
            Value::Int(0)
        }
        // 2.60 (0x18eb40): the float at +0xd4, the time gauge value.
        (2, 60) if on_task => Value::Float(task.remaining),
        // 2.63 (0x18eb50): the word at +0x3c, the count of started HUD animations.
        (2, 63) if on_task => Value::Int(task.animations),
        // 2.89 (0x18eb60 -> FUN_001b3b60).
        (2, 89) if on_task => {
            task.release_animation();
            Value::None
        }
        // 1.147 (0x170610): the object's world position, answered as the vector the script copies from. The player's is Sora's; a
        // participant has no object in the app, so its stand-in stands at the player's position [inferred: the party follows Sora; no
        // offset is known]; any other handle is at the origin, where a new object starts (1.0, `FUN_0016e8c0`).
        (1, 147) => Value::Vec(position_of(if let Call::Position { obj } = c { *obj } else { first_arg(c) }, task)),
        // 1.83 (0x172e68, `bone_to_world(O, index, v)`): the vector `v` through the matrix of the object's bone. The app has no bones
        // for a participant, so the bone is the object's own frame without a turn [inferred]: the position plus `v`.
        (1, 83) => {
            let at = position_of(first_arg(c), task);
            let v = match c {
                Call::Func { args, .. } => args.iter().find_map(|a| if let kh2_bdx::Arg::Vec(v) = a { Some(*v) } else { None }).unwrap_or([0.0; 4]),
                _ => [0.0; 4],
            };
            Value::Vec([at[0] + v[0], at[1] + v[1], at[2] + v[2], 1.0])
        }
        // 1.98 (0x173088, `ground_position(O)`): the position with y at the ground height under the object. The stand-ins stand on the
        // ground and the player is on it in the harness, so the position itself [inferred].
        (1, 98) => Value::Vec(position_of(first_arg(c), task)),
        // 1.201 (the facing vector, as the enemy host answers it): `(sin yaw, 0, -cos yaw)` of the object's yaw. Sora's yaw for the
        // player; a participant has no object, so its stand-in faces as Sora does [inferred].
        (1, 201) => Value::Vec([task.player_yaw.sin(), 0.0, -task.player_yaw.cos(), 0.0]),
        // 1.94 (0x170150): the object is in the object list; the app's only object a limit script can name is the player.
        (1, 94) => Value::Bool(first_arg(c) == crate::enemy_obj::PLAYER),
        // 1.132 (0x173728): returns a target record to the pool; the app keeps no pool (host 1.131 answers no record).
        (1, 132) => Value::None,
        // 6.47 (0x2eefe0 -> FUN_0016bf78 -> FUN_0016bed8 -> FUN_001d59a8): removes a stop id (the script passes 17) from the three-slot
        // table at +0x518 of every object. The app holds no actor stop 17 (summon.rs holds 2), so there is nothing to remove.
        (6, 47) => Value::None,
        // 7.28 / 7.30 (0x2ef7b0 / 0x2ef820 -> FUN_002f3bd8 / FUN_002f3c18) start and end a screen effect object (`DAT_00371518`).
        // Not ported: it draws.
        (7, 28) | (7, 30) => Value::None,
        _ => Value::Int(0),
    }
}

/// Host 2.50 started the type 0x16 event `name` of the limit file `file` (`FUN_001b3468`: `FUN_00168510(bar, 0x16, name, 0)`, an event
/// object of kind 0xb). `limit_camera.rs` plays the camera track of that event.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct LimitEvent {
    pub file: String,
    /// The entry name the script asked for (up to four characters, as the BAR header holds it).
    pub name: String,
}

/// The string a script pointer names in the program image (host 2.50's second argument is a pointer to a four byte name; the scripts
/// keep it as a literal in their code, so the address is `code_base + offset`). `None` for an address outside the image.
pub fn name_at(code: &[u8], code_base: u32, ptr: u32) -> Option<String> {
    let at = usize::try_from(ptr.checked_sub(code_base)?).ok()?;
    let bytes = code.get(at..at + 4)?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(4);
    Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

/// A running limit script thread.
pub struct ScriptRun {
    rt: Runtime,
    /// The program image as the interpreter maps it (`Runtime::new`: the words as little endian bytes) and its base address.
    code: Vec<u8>,
    code_base: u32,
    /// Requests of `task.requests` already looked at for event starts.
    seen: usize,
    /// Events host 2.50 started since the last [`ScriptRun::take_events`].
    events: Vec<String>,
    /// Motions host 2.47 started since the last [`ScriptRun::take_motions`].
    motions: Vec<crate::limit_visual::LimitMotion>,
    /// Effects host 2.59 started since the last [`ScriptRun::take_effects`].
    effects: Vec<crate::limit_visual::LimitEffect>,
    pub task: TaskState,
    pub calls: BTreeMap<(u16, u16), CallStat>,
    pub ticks: u32,
    /// Lines the host collected for the log, drained by the system.
    fresh: Vec<String>,
    /// The tick count when the limit's countdown ended.
    ended_at: Option<u32>,
}

/// Ticks a script thread may keep running after the limit's countdown ended (its tail: release, a yield, the cleanup calls).
const TAIL_TICKS: u32 = 30;

impl ScriptRun {
    /// `FUN_001b2e08`: the thread on the type 3 entry, event 22 with the task. The caller logs a `false` (no event 22 in the script).
    pub fn start(prog: Program, table: Option<&HostTable>, task: TaskState) -> (ScriptRun, bool) {
        let code: Vec<u8> = prog.words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let mut rt = Runtime::new(prog);
        let code_base = rt.shared.layout.code;
        if let Some(t) = table {
            rt.set_host_table(t.clone());
        }
        let started = rt.start_thread(0, 22, &[TASK]);
        (ScriptRun { rt, code, code_base, seen: 0, events: Vec::new(), motions: Vec::new(), effects: Vec::new(), task, calls: BTreeMap::new(), ticks: 0, fresh: Vec::new(), ended_at: None }, started)
    }

    /// `FUN_001b28a0` first branch: event 23 with the command id, to the running thread (`FUN_0016c620`). [inferred] The thread
    /// takes the event as `Runtime::restart_thread` does for the cast task's event 21: its code becomes the handler.
    pub fn sub_command(&mut self, command: u16) -> bool {
        self.rt.restart_thread(23, &[u32::from(command)])
    }

    /// One 60 Hz tick of the thread (`FUN_0016c680` from the task's frame). `Err` when it stopped.
    pub fn tick(&mut self, remaining: u32) -> Result<(), Stop> {
        self.ticks += 1;
        self.task.remaining = remaining as f32;
        let (rt, calls, fresh, task, tick) = (&mut self.rt, &mut self.calls, &mut self.fresh, &mut self.task, self.ticks);
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rt.update_script(&mut LimitHost { calls, task, tick, fresh })));
        if ran.is_err() {
            return Err(Stop::Fault("the interpreter panicked".into()));
        }
        self.collect_events();
        if let Some((pc, e)) = self.rt.core.errors.first() {
            return Err(Stop::Fault(format!("{e:?} at word {pc:#x}")));
        }
        if self.rt.thread_done() {
            return Err(Stop::Returned);
        }
        Ok(())
    }

    /// Looks at the 2.50 calls since the last tick: each names an event of the limit file (the name is a literal in the script).
    fn collect_events(&mut self) {
        for r in &self.task.requests[self.seen..] {
            let args = match r {
                Request::Effect { args } => args,
                // Host 2.47: (task, context pointer, motion id, blend as float bits); the participant was read from the context.
                Request::Motion { args, who } => {
                    let blend = args.get(3).map_or(0.0, |&b| f32::from_bits(b));
                    self.motions.push(crate::limit_visual::LimitMotion { who: *who, id: args.get(2).copied().unwrap_or(0) as usize, blend });
                    continue;
                }
                // Host 2.59: (task, element id, context pointer, flag); the participant was read from the context.
                Request::PaxElement { args, who } => {
                    self.effects.push(crate::limit_visual::LimitEffect { who: *who, id: args.get(1).copied().unwrap_or(0) as u16 });
                    continue;
                }
            };
            match args.get(1).and_then(|&p| name_at(&self.code, self.code_base, p)) {
                Some(n) if !n.is_empty() => self.events.push(n),
                _ => self.fresh.push(format!("limit script: host 2.50 named no event in the program image: {args:x?}")),
            }
        }
        self.seen = self.task.requests.len();
    }

    /// The events started since the last call, in order.
    pub fn take_events(&mut self) -> Vec<String> {
        std::mem::take(&mut self.events)
    }

    /// The motions started since the last call, in order.
    pub fn take_motions(&mut self) -> Vec<crate::limit_visual::LimitMotion> {
        std::mem::take(&mut self.motions)
    }

    /// The effects started since the last call, in order.
    pub fn take_effects(&mut self) -> Vec<crate::limit_visual::LimitEffect> {
        std::mem::take(&mut self.effects)
    }

    /// The host calls as `group.index xN` words, in order.
    pub fn summary(&self) -> String {
        self.calls.iter().map(|((g, i), s)| format!("{g}.{i}x{}", s.count)).collect::<Vec<_>>().join(" ")
    }
}

/// The script of the limit BAR `limit/us/<file>`: its type 3 entry.
pub fn script_of(bar: &[u8]) -> Option<Program> {
    let es = kh2_bar::parse(bar).ok()?;
    let e = es.iter().find(|e| e.kind == crate::limit::KIND_SCRIPT)?;
    Program::parse(e.data).ok()
}

#[derive(Resource, Default)]
struct ScriptState {
    run: Option<ScriptRun>,
    table: Option<HostTable>,
    table_read: bool,
    /// The file of the limit that started last (`LimitStarted`).
    file: String,
    /// The participant bytes of the limit that started last.
    members: [u8; 3],
    /// The command of the limit that started last.
    command: u16,
    /// Events 22 and 23 that came for the script, count (for the tests of the wiring).
    delivered: u32,
}

pub struct LimitScriptPlugin;

impl Plugin for LimitScriptPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LimitStarted>()
            .add_message::<LimitScriptEvent>()
            .add_message::<LimitEvent>()
            .add_message::<crate::limit_visual::LimitMotion>()
            .add_message::<crate::limit_visual::LimitEffect>()
            .init_resource::<ScriptState>()
            .add_systems(FixedUpdate, run_limit_script.after(Sim::Sora));
    }
}

fn run_limit_script(
    disc: Option<Res<Disc>>,
    task: Option<Res<LimitTask>>,
    mut started: MessageReader<LimitStarted>,
    mut events: MessageReader<LimitScriptEvent>,
    (mut started_events, mut motion_events, mut effect_events): (MessageWriter<LimitEvent>, MessageWriter<crate::limit_visual::LimitMotion>, MessageWriter<crate::limit_visual::LimitEffect>),
    sora: Query<&Sora>,
    mut st: ResMut<ScriptState>,
) {
    for s in started.read() {
        st.file = s.file.clone();
        st.command = s.command;
        st.members = s.members;
    }
    let running = task.as_deref().is_some_and(|t| t.0.is_some());
    for e in events.read().filter(|e| e.to == LimitReceiver::LimitScript) {
        match (e.event, e.arg) {
            (22, LimitArg::Task) => {
                let Some(disc) = disc.as_deref() else { continue };
                if !st.table_read {
                    st.table_read = true;
                    st.table = disc.read_root("SLUS_210.05").and_then(|x| HostTable::from_exe(&x));
                }
                let Some(prog) = disc.read(&format!("limit/us/{}", st.file)).and_then(|b| script_of(&b)) else {
                    info!("limit script: {} has no readable type 3 entry", st.file);
                    continue;
                };
                let left = task.as_deref().and_then(|t| t.0).map_or(0, |(_, l)| l);
                let (run, ok) = ScriptRun::start(prog, st.table.as_ref(), TaskState { participants: participants_of(st.members), ..TaskState::new(st.command, left as f32) });
                info!("limit script: start {} (event 22 with the task {TASK:#x}; the script has a handler for it: {ok})", st.file);
                st.delivered += 1;
                st.run = Some(run);
            }
            (23, LimitArg::Command(c)) => {
                if let Some(run) = st.run.as_mut() {
                    let ok = run.sub_command(c);
                    info!("limit script: event 23 with command {c} (handler found: {ok})");
                    st.delivered += 1;
                }
            }
            _ => {}
        }
    }
    let file = st.file.clone();
    let Some(run) = st.run.as_mut() else { return };
    if let Ok(s) = sora.single() {
        run.task.player = crate::enemy_obj::to_game(s.motor.pos);
        run.task.player_yaw = s.motor.yaw;
        let v = crate::enemy_obj::to_game(s.motor.vel);
        run.task.player_velocity = [v[0], v[1], v[2], 0.0];
    }
    // The app's countdown ends the limit (FUN_001b3970 waits for HUD animations: a gap). The gauge then reads 0, which is what ends the
    // script's own wait (host 2.60), so the thread gets the ticks its tail needs (at most `TAIL_TICKS`) and is dropped after them.
    let left = task.as_deref().and_then(|t| t.0).map_or(0, |(_, l)| l);
    let ended = !running;
    if ended && run.ended_at.is_none() {
        run.ended_at = Some(run.ticks);
    }
    let stop = match run.tick(left).err() {
        None if run.ended_at.is_some_and(|at| run.ticks - at >= TAIL_TICKS) => Some(Stop::Returned),
        other => other,
    };
    for l in run.fresh.drain(..) {
        info!("{l}");
    }
    for name in run.take_events() {
        info!("limit script: host 2.50 starts event {name:?} of {file} (FUN_001b3468)");
        started_events.write(LimitEvent { file: file.clone(), name });
    }
    for m in run.take_motions() {
        motion_events.write(m);
    }
    for e in run.take_effects() {
        effect_events.write(e);
    }
    let Some(stop) = stop else { return };
    let run = st.run.take().expect("checked above");
    match stop {
        Stop::Returned => info!("limit script: ended after {} ticks (the limit {}); host calls: {}", run.ticks, if ended { "is over" } else { "goes on" }, run.summary()),
        Stop::Fault(why) => warn!("limit script: fault after {} ticks ({why}); the thread is dropped, the limit goes on; host calls: {}", run.ticks, run.summary()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program with the given entries (event id, code words), laid out like a BDX header: the 16 byte name, three sizes, the
    /// entry table ({id, word index} pairs, a zero offset ends it), then the code.
    fn program(entries: &[(u32, &[u16])]) -> Program {
        let table_words = 6 + 4 * entries.len() + 4;
        let mut words = vec![0u16; table_words];
        let (globals, frame, stack) = (0x40u32, 0x40u32, 0x40u32);
        words[0..2].copy_from_slice(&[globals as u16, 0]);
        words[2..4].copy_from_slice(&[frame as u16, 0]);
        words[4..6].copy_from_slice(&[stack as u16, 0]);
        let mut code: Vec<u16> = Vec::new();
        for (k, (id, body)) in entries.iter().enumerate() {
            let pc = (table_words + code.len()) as u32;
            let at = 6 + 4 * k;
            words[at..at + 2].copy_from_slice(&[*id as u16, 0]);
            words[at + 2..at + 4].copy_from_slice(&[pc as u16, (pc >> 16) as u16]);
            code.extend_from_slice(body);
        }
        words.extend(code);
        let mut bytes = b"limit_test\0\0\0\0\0\0".to_vec();
        bytes.extend(words.iter().flat_map(|w| w.to_le_bytes()));
        Program::parse(&bytes).expect("the test program parses")
    }

    const RET: u16 = 0x0089;

    /// Event 22 starts the thread with the task as its argument: the handler stores it, loads it and calls host 0.14 with it, which
    /// the host sees as the call's one argument.
    #[test]
    fn the_script_starts_once_and_gets_event_22_with_the_task() {
        let prog = program(&[(22, &[0x0001, 0x0000, 0x0030, 0x0000, 0x000a, 0x000e, RET])]);
        let (mut run, ok) = ScriptRun::start(prog, None, TaskState::default());
        assert!(ok);
        let _ = run.tick(100);
        assert_eq!(run.calls.get(&(0, 14)).map(|s| s.count), Some(1), "calls {:?}", run.calls);
        let first = run.fresh.first().expect("the first call is logged");
        assert!(first.contains("host 0.14") && first.contains(&format!("{TASK}")), "{first}");
        // A second tick does not start it again.
        let _ = run.tick(100);
        assert_eq!(run.calls.get(&(0, 14)).map(|s| s.count), Some(1));
    }

    /// A script without a handler for event 22 is reported as not started and its thread is over.
    #[test]
    fn a_script_without_event_22_is_over_at_once() {
        let prog = program(&[(23, &[RET])]);
        let (mut run, ok) = ScriptRun::start(prog, None, TaskState::default());
        assert!(!ok);
        assert_eq!(run.tick(100), Err(Stop::Returned));
    }

    /// A fault (a VM error: a word that is no instruction) is reported as a fault, not a crash or a hang, and a later tick of the
    /// same run does not panic.
    #[test]
    fn a_fault_stops_the_thread_and_nothing_else() {
        // A word that is no instruction (`BadInstruction`).
        let prog = program(&[(22, &[0xffff, RET])]);
        let (mut run, _) = ScriptRun::start(prog, None, TaskState::default());
        let r = run.tick(100);
        assert!(matches!(r, Err(Stop::Fault(_))), "{r:?}");
    }

    /// A script that loops without yielding is stopped by the interpreter's step limit: the tick comes back with a fault (no hang).
    #[test]
    fn a_script_that_never_yields_does_not_hang_the_tick() {
        // `jmp` to itself.
        let prog = program(&[(22, &[0x0007, 0xfffe])]);
        let (mut run, _) = ScriptRun::start(prog, None, TaskState::default());
        match run.tick(100) {
            Err(Stop::Fault(why)) => assert!(why.contains("StepLimit"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    /// A script that yields every frame is run one slice per tick for as long as it is asked to.
    #[test]
    fn a_yielding_script_runs_one_slice_per_tick() {
        // `yield; jmp -3`.
        let prog = program(&[(22, &[0x0009, 0x0007, 0xfffd])]);
        let (mut run, _) = ScriptRun::start(prog, None, TaskState::default());
        for k in 1..=3 {
            assert_eq!(run.tick(100), Ok(()), "tick {k}");
        }
        assert_eq!(run.ticks, 3);
    }

    fn func(group: u16, index: u16, a: &[u32]) -> Call {
        Call::Func { group, index, args: a.iter().map(|&v| kh2_bdx::Arg::Raw(v)).collect() }
    }

    /// The task's fields as hosts 2.48, 2.60, 2.63 and 2.89 answer them; Trinity (command 0x1e4) starts with one HUD animation that
    /// 2.89 releases once, other limits start with none, and a handle that is not the task reads 0.
    #[test]
    fn the_task_hosts_answer_from_the_task_state() {
        let mut t = TaskState::new(0x1e4, 180.0);
        assert_eq!(host_answer(&func(2, 48, &[TASK]), &mut t), Value::Handle(crate::enemy_obj::PLAYER));
        assert_eq!(host_answer(&func(2, 60, &[TASK]), &mut t), Value::Float(180.0));
        assert_eq!(host_answer(&func(2, 63, &[TASK]), &mut t), Value::Int(1));
        host_answer(&func(2, 89, &[TASK]), &mut t);
        assert_eq!(host_answer(&func(2, 63, &[TASK]), &mut t), Value::Int(0));
        host_answer(&func(2, 89, &[TASK]), &mut t);
        assert_eq!(host_answer(&func(2, 63, &[TASK]), &mut t), Value::Int(0), "a second release finds bit 8 clear");
        assert_eq!(host_answer(&func(2, 60, &[1234]), &mut t), Value::Int(0), "another handle is not the task");
        assert_eq!(TaskState::new(282, 1.0).animations, 0);
        assert_eq!(host_answer(&func(1, 94, &[crate::enemy_obj::PLAYER]), &mut t), Value::Bool(true));
        assert_eq!(host_answer(&func(1, 94, &[0]), &mut t), Value::Bool(false));
    }

    /// `FUN_001b2c10` packs the row's participant bytes above 1 into task `+0x10`...; host 2.49 reads them by index, 1.210 binds a
    /// participant by its id, and neither finds a character that is not in the limit.
    #[test]
    fn participants_are_the_row_bytes_above_one_and_can_be_bound() {
        assert_eq!(participants_of([2, 0, 3]), vec![2, 3]);
        assert_eq!(participants_of([1, 5, 0]), vec![5]);
        assert!(participants_of([0, 0, 0]).is_empty());
        let mut t = TaskState { participants: participants_of([2, 0, 3]), ..TaskState::new(282, 10.0) };
        assert_eq!(host_answer(&func(2, 49, &[TASK, 0]), &mut t), Value::Handle(partner(2)));
        assert_eq!(host_answer(&func(2, 49, &[TASK, 1]), &mut t), Value::Handle(partner(3)));
        assert_eq!(host_answer(&func(2, 49, &[TASK, 2]), &mut t), Value::Handle(0), "a third slot is empty");
        let by = |by, value| Call::FindObject { by, value };
        use kh2_bdx::host::ObjectKey;
        assert_eq!(host_answer(&by(ObjectKey::NeoStatus, 3), &mut t), Value::Handle(partner(3)));
        assert_eq!(host_answer(&by(ObjectKey::NeoStatus, 4), &mut t), Value::Handle(0));
        assert_eq!(host_answer(&by(ObjectKey::Entity, 0x236), &mut t), Value::Handle(crate::enemy_obj::PLAYER));
        assert_eq!(host_answer(&by(ObjectKey::Entity, 0x237), &mut t), Value::Handle(0));
    }

    /// The pool hosts: 7.11 says every slot is free (the app keeps no pool), 2.55 answers a non-null object for the task only.
    #[test]
    fn the_pool_and_slot_hosts_answer() {
        let mut t = TaskState::new(1, 1.0);
        assert_eq!(host_answer(&func(7, 11, &[0]), &mut t), Value::Bool(true));
        assert_eq!(host_answer(&func(7, 3, &[]), &mut t), Value::None);
        assert_eq!(host_answer(&func(2, 55, &[TASK]), &mut t), Value::Handle(EFFECT_SLOT));
        assert_eq!(host_answer(&func(2, 55, &[5]), &mut t), Value::Int(0));
    }

    /// The gauge follows the countdown the system hands in: a script that waits on host 2.60 sees the ticks left.
    #[test]
    fn host_2_60_reads_the_countdown() {
        // Event 22: push the task handle, call host 2.60, return.
        let prog = program(&[(22, &[0x0000, 0x0000, 0x7a00, 0x008a, 0x003c, 0x0009, 0x0089])]);
        let (mut run, ok) = ScriptRun::start(prog, None, TaskState::default());
        assert!(ok);
        run.tick(77).ok();
        assert_eq!(run.task.remaining, 77.0);
        assert_eq!(run.calls.get(&(2, 60)).map(|s| s.count), Some(1), "{:?}", run.calls);
    }

    /// The system wiring: the countdown's end drops the thread after its tail; events for another receiver are not the script's.
    #[test]
    fn the_countdown_ends_the_thread() {
        let mut app = App::new();
        app.add_message::<LimitStarted>().add_message::<LimitScriptEvent>().add_message::<LimitEvent>().add_message::<crate::limit_visual::LimitMotion>().add_message::<crate::limit_visual::LimitEffect>().init_resource::<ScriptState>();
        app.insert_resource(LimitTask(Some((1, 10))));
        app.add_systems(Update, run_limit_script);
        let prog = program(&[(22, &[0x0009, 0x0007, 0xfffd])]);
        let (run, _) = ScriptRun::start(prog, None, TaskState::default());
        app.world_mut().resource_mut::<ScriptState>().run = Some(run);
        app.update();
        assert!(app.world().resource::<ScriptState>().run.is_some(), "the thread runs while the limit does");
        app.world_mut().resource_mut::<LimitTask>().0 = None;
        app.update();
        assert!(app.world().resource::<ScriptState>().run.is_some(), "the thread keeps its tail ticks after the limit's end");
        for _ in 0..TAIL_TICKS {
            app.update();
        }
        assert!(app.world().resource::<ScriptState>().run.is_none(), "the limit's end drops the thread after the tail");
    }

    /// Host 1.147 answers the player's position for the player and the stand-ins, the origin for any other handle.
    #[test]
    fn host_1_147_gives_the_player_position() {
        let mut t = TaskState::new(1, 1.0);
        t.player = [10.0, -20.0, 30.0, 1.0];
        let pos = |h| host_answer(&Call::Position { obj: h }, &mut TaskState { ..t.clone() });
        assert_eq!(pos(crate::enemy_obj::PLAYER), Value::Vec([10.0, -20.0, 30.0, 1.0]));
        assert_eq!(pos(partner(3)), Value::Vec([10.0, -20.0, 30.0, 1.0]));
        assert_eq!(pos(0x1234), Value::Vec([0.0, 0.0, 0.0, 1.0]));
    }

    /// Host 1.83 adds the vector to the object's position; host 1.98 answers the position itself; host 1.201 the facing of the yaw.
    #[test]
    fn hosts_1_83_1_98_and_1_201_follow_the_player_state() {
        let mut t = TaskState::new(1, 1.0);
        t.player = [1.0, 2.0, 3.0, 1.0];
        t.player_yaw = std::f32::consts::FRAC_PI_2;
        let call = Call::Func { group: 1, index: 83, args: vec![kh2_bdx::Arg::Raw(crate::enemy_obj::PLAYER), kh2_bdx::Arg::Raw(0), kh2_bdx::Arg::Vec([10.0, 20.0, 30.0, 1.0])] };
        assert_eq!(host_answer(&call, &mut t), Value::Vec([11.0, 22.0, 33.0, 1.0]));
        assert_eq!(host_answer(&func(1, 98, &[partner(2)]), &mut t), Value::Vec([1.0, 2.0, 3.0, 1.0]));
        let Value::Vec(f) = host_answer(&func(1, 201, &[crate::enemy_obj::PLAYER]), &mut t) else { panic!("1.201 answers a vector") };
        assert!((f[0] - 1.0).abs() < 1e-6 && f[1] == 0.0 && f[2].abs() < 1e-6, "{f:?}");
    }

    /// Simba and Tron copy 16 bytes from the player object's +0x20; the memory answers them with the velocity and keeps writes. Hosts
    /// 2.56 point at a target word that reads 0 (Riku, Trinity without a group).
    #[test]
    fn object_memory_serves_the_velocity_and_the_target_word() {
        let mut m = ObjectMem::default();
        let v = [1.5f32, -2.0, 4.0, 0.0];
        let mut b = [0u8; 16];
        assert!(m.read(crate::enemy_obj::PLAYER + VELOCITY_OFFSET, v, &mut b));
        assert_eq!(f32::from_le_bytes(b[0..4].try_into().unwrap()), 1.5);
        assert_eq!(f32::from_le_bytes(b[8..12].try_into().unwrap()), 4.0);
        assert!(m.read(partner(5) + VELOCITY_OFFSET + 4, v, &mut b[..4]));
        assert_eq!(f32::from_le_bytes(b[..4].try_into().unwrap()), -2.0);
        assert!(m.write(crate::enemy_obj::PLAYER + 0x40, &[7]));
        let mut one = [0u8; 1];
        assert!(m.read(crate::enemy_obj::PLAYER + 0x40, v, &mut one));
        assert_eq!(one, [7]);
        assert!(!m.read(0x1000, v, &mut one), "an address outside the limit's objects is no memory");
        // 2.23 answers the own record, 1.131 files a copy of it in the pool (the player as its object, -1 at word 3).
        let mut t = TaskState::new(1, 1.0);
        let Value::Handle(own) = host_answer(&func(2, 23, &[crate::enemy_obj::PLAYER]), &mut t) else { panic!("2.23 answers a record") };
        let Value::Handle(copy) = host_answer(&Call::FreeTarget { handle: own }, &mut t) else { panic!("1.131 answers a record") };
        assert_ne!(copy, 0);
        assert_ne!(copy, own);
        let mut rec = [0u8; 20];
        assert!(t.mem.read(copy, [9.0; 4], &mut rec));
        assert_eq!(rec, crate::enemy_host::target_record(), "the copy is the record, not the velocity");
        assert!(t.mem.write(copy + 4, &[5, 0, 0, 0]), "the script stores into its record");
        assert_eq!(host_answer(&Call::FreeTarget { handle: 0 }, &mut t), Value::Handle(0), "no record at 0");
        let Value::Handle(slot) = host_answer(&func(2, 56, &[crate::enemy_obj::PLAYER]), &mut t) else { panic!("2.56 answers a pointer") };
        let mut w = [9u8; 4];
        assert!(m.read(slot, v, &mut w));
        assert_eq!(w, [0; 4]);
    }

    /// A script's memory for the tests: a flat image at `BASE`.
    struct Flat(Vec<u8>);
    const BASE: u32 = 0x3000_0000;

    impl kh2_bdx::host::ScriptMem for Flat {
        fn read(&mut self, addr: u32, out: &mut [u8]) -> bool {
            let Some(o) = addr.checked_sub(BASE).map(|o| o as usize).filter(|o| o + out.len() <= self.0.len()) else { return false };
            out.copy_from_slice(&self.0[o..o + out.len()]);
            true
        }

        fn write(&mut self, addr: u32, data: &[u8]) -> bool {
            let Some(o) = addr.checked_sub(BASE).map(|o| o as usize).filter(|o| o + data.len() <= self.0.len()) else { return false };
            self.0[o..o + data.len()].copy_from_slice(data);
            true
        }
    }

    /// Host 1.160 empties the record and files the context's object (word at +4) as its object; host 6.57 answers the word at +4 of
    /// its argument, both through the script's memory.
    #[test]
    fn hosts_1_160_and_6_57_work_on_the_scripts_memory() {
        use kh2_bdx::host::ScriptMem;
        let mut mem = Flat(vec![0xee; 0x40]);
        let (record, context) = (BASE, BASE + 0x20);
        mem.write_u32(context + 4, partner(3));
        let mut objects = ObjectMem::default();
        host_1_160(&[record, context], &mut mem, &mut objects);
        let words: Vec<u32> = (0..4).map(|i| mem.read_u32(record + 4 * i).unwrap()).collect();
        assert_eq!(words, [partner(3), 0, 0, u32::MAX]);
        assert_eq!(mem.read_u32(record + 16), Some(0xeeee_eeee), "only the four record words are written");
        assert_eq!(host_6_57(&[context], &mut mem, &objects, [0.0; 4]), Value::Int(partner(3) as i32));
        assert_eq!(host_6_57(&[0x1000], &mut mem, &objects, [0.0; 4]), Value::Int(0), "an unknown address answers 0");
        // A record in the limit's object memory (the pool) takes the write instead.
        let filed = objects.file_record(OWN_RECORD);
        host_1_160(&[filed, context], &mut mem, &mut objects);
        let mut w = [0u8; 4];
        assert!(objects.read(filed, [0.0; 4], &mut w));
        assert_eq!(u32::from_le_bytes(w), partner(3));
        // Through the host: the call is logged like any other and answers as the ported call does.
        let (mut calls, mut fresh, mut task) = (BTreeMap::new(), Vec::new(), TaskState::new(1, 1.0));
        let mut host = LimitHost { calls: &mut calls, task: &mut task, tick: 0, fresh: &mut fresh };
        let call = Call::Unknown { group: 6, index: 57, args: vec![context] };
        assert_eq!(host.call_with_mem(&call, &mut mem), Value::Int(partner(3) as i32));
        assert_eq!(calls[&(6, 57)].count, 1);
    }

    /// Hosts 2.47 and 2.59 read the participant from the word at `+4` of the context block (the second argument of 2.47, the third
    /// of 2.59), and `collect_events` turns the requests into the motion and effect messages `limit_visual` plays (the id, the blend
    /// as float bits, the element id).
    #[test]
    fn hosts_2_47_and_2_59_name_their_participant_from_the_context() {
        use kh2_bdx::host::ScriptMem;
        let mut mem = Flat(vec![0; 0x40]);
        let context = BASE + 0x10;
        mem.write_u32(context + 4, partner(3));
        let (mut calls, mut fresh, mut task) = (BTreeMap::new(), Vec::new(), TaskState::new(1, 1.0));
        {
            let mut host = LimitHost { calls: &mut calls, task: &mut task, tick: 0, fresh: &mut fresh };
            host.call_with_mem(&Call::Unknown { group: 2, index: 47, args: vec![TASK, context, 253, 4.0f32.to_bits()] }, &mut mem);
            host.call_with_mem(&Call::Unknown { group: 2, index: 59, args: vec![TASK, 4, context, 1] }, &mut mem);
        }
        assert_eq!(task.requests[0], Request::Motion { args: vec![TASK, context, 253, 4.0f32.to_bits()], who: partner(3) });
        assert_eq!(task.requests[1], Request::PaxElement { args: vec![TASK, 4, context, 1], who: partner(3) });
        let (mut run, _) = ScriptRun::start(program(&[(22, &[RET])]), None, task);
        run.collect_events();
        assert_eq!(run.take_motions(), vec![crate::limit_visual::LimitMotion { who: partner(3), id: 253, blend: 4.0 }]);
        assert_eq!(run.take_effects(), vec![crate::limit_visual::LimitEffect { who: partner(3), id: 4 }]);
        assert!(run.take_motions().is_empty(), "taken once");
    }

    /// Hosts 2.47, 2.50 and 2.59 keep the request they were given and answer what the original answers without a motion or table.
    #[test]
    fn participant_requests_are_kept() {
        let mut t = TaskState::new(1, 1.0);
        assert_eq!(host_answer(&func(2, 47, &[partner(2), 5, 6, TASK]), &mut t), Value::Int(0));
        assert_eq!(host_answer(&func(2, 50, &[partner(2), 9, 0, 1]), &mut t), Value::None);
        assert_eq!(host_answer(&func(2, 59, &[TASK, 12, 3, 4]), &mut t), Value::Int(0));
        assert_eq!(t.requests.len(), 3);
        assert_eq!(t.requests[0], Request::Motion { args: vec![partner(2), 5, 6, TASK], who: 0 });
        assert!(matches!(t.requests[2], Request::PaxElement { .. }));
    }

    /// Host 2.50's second argument points at a four byte name the script keeps in its code: the name is read from the program image and
    /// becomes an event start (once); a pointer outside the image names nothing and is logged.
    #[test]
    fn host_2_50_names_an_event_from_the_program_image() {
        // "00" and "01" (a name, then NULs) sit in the code as the words 0x3030 0x0000 and 0x3130 0x0000.
        let prog = program(&[(22, &[RET, 0x3030, 0x0000, 0x3130, 0x0000])]);
        let (mut run, _) = ScriptRun::start(prog, None, TaskState::default());
        let find = |lit: &[u8]| run.code.windows(4).position(|w| w == lit).expect("the literal is in the image") as u32 + run.code_base;
        let (zero, one) = (find(b"00\0\0"), find(b"01\0\0"));
        assert_eq!(name_at(&run.code, run.code_base, zero).as_deref(), Some("00"));
        assert_eq!(name_at(&run.code, run.code_base, run.code_base.wrapping_sub(1)), None);
        for p in [zero, one, 0x7777_0000] {
            assert_eq!(host_answer(&func(2, 50, &[TASK, p, 0, 0]), &mut run.task), Value::None);
        }
        run.collect_events();
        assert_eq!(run.take_events(), vec!["00".to_string(), "01".to_string()]);
        assert!(run.fresh.iter().any(|l| l.contains("named no event")), "{:?}", run.fresh);
        run.collect_events();
        assert!(run.take_events().is_empty(), "a request is looked at once");
    }
}
