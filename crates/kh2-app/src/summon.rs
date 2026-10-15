//! Summons, stages 2 and 3 of docs/summons.md section 6: the gauge, the command, and the summon object
//! with the arrival that skips the event. The rules are pure functions in `drive::rules` (the summon
//! uses the drive gauge under mode byte 3) and in `rules` below; this module keeps the summon's own
//! state and runs the sequence:
//!
//! - Summon (`Execute` 72, inlined into `FUN_001cf030`; the same steps as `FUN_001c6168`): the player
//!   plays motion request 0x8a (blend `DAT_0037c358`), system effect 0x13 starts bound to him (handle
//!   at +0xb68), the cost in drive points (`FUN_001a0320`), the active summon index (`DAT_00340c88`
//!   byte 5), the sequence bit (`0x35feb8` bit 0), the summon state (`DAT_01d5e368`) 2, byte 7 (Genie:
//!   a random owned drive form, `FUN_001c6a88`; Peter Pan: 1), the player's +0x108 |= 0x104, the
//!   arrival thread (0x1c6260), the companions' fade-out over `DAT_0037bff0` frames (`FUN_001c7740`),
//!   `FUN_001c6578` (`0x35fec0` bit 3, actor stop id 2 on every actor but classes 0, 1, 5, 19) and the
//!   use counter (`FUN_001d76f8`).
//! - The arrival thread (0x1c6260): the event choice (`FUN_001c65f0`: the Summon Effects option and the
//!   per-summon seen bits), the wait (`sstm[0x84]` with the event, `sstm[0x88]` without) until the wait
//!   has run out and both party slots are empty (`FUN_0017d470`); the player's +0x108 |= 1; gauge mode 3
//!   (`FUN_001a0480(status, 3, summon level)`); the summon object (`sumn` `Entity1`, constructor
//!   `FUN_001c5e20`) at the player's position. Without the event: effect 0x13 ends (`FUN_001d9f48`), the
//!   player plays motion 0x82 (blend 0), the object moves to a free spot 200..300 units away
//!   (`FUN_001a5ea8`) and its script switches to `appear_skip`. With the event: the effect is killed
//!   (`FUN_001da028`), the script switches to `appear`, the state is 1 while an event runs. Then the
//!   state is 2, +0x108 &= ~0x105, and the thread's cleanup (`FUN_001c65b8`) lifts the stop.
//! - Mode 3 each tick: the drain (`FUN_00177788` with the ability block's Summon Boost factor), the
//!   whole segments lost feed the summon experience (`FUN_001a0358` -> `FUN_001d4ec8`), written back to
//!   `Progress` (byte 6 and `0x340e4c`), and hits Sora lands add `DriveDrain x sstm[0x70]` to the timer
//!   while it is not empty (`FUN_00178748` -> `FUN_001788d8`); a summon limit's cost arrives as a
//!   `SummonRequest::Gauge` from limit.rs (`FUN_001cf030` case 0x51 -> `FUN_001788d8(-Cost)`).
//! - End: the timer at 0 (`FUN_001ceda8` -> `FUN_001cf298`) or Dismiss (`FUN_001cf030` case 0x13c): the
//!   leave (`FUN_001c67b0`: state 3, byte 5 = 0; the thread 0x1c6810 switches the first summon object to
//!   `leave` and waits until both summon slots are empty, then brings the companions back,
//!   `FUN_001c79c8`; its cleanup clears `0x35feb8` bit 0) and gauge mode 1. The state is 0 again when
//!   the last summon object is gone (its destructor, `FUN_001c5f80` / `FUN_001c6038`).
//!
//! The object's script runs through `SummonHost` (WP-SUM4): the party host plus the group 10 entries of
//! Stitch's script (the AI object's fields in `ai::SummonAi`).
//!
//! What the app does not have (docs/summons.md "Status"): the friend AI (the object stands where it was
//! put; its script's motion requests are logged, not played), the event itself (nothing in the app starts it: the wait for it ends at
//! once), the HUD layers the event hides, the screen fade list (`FUN_0016ff50`), the free-spot
//! collision tests (`FUN_001a5c30` / `FUN_001a5bf8`: a floor under the point stands in), the
//! companions' return position (party.rs keeps their movement; they fade in where they are), the
//! actor stop on enemy scripts (enemy_ai.rs runs them; this module stops their motion and makes them
//! untouchable), the summon face panel.
//!
//! Harness: `KH2_SUMMON=<sumn index 1..4 or command id>` asks for that summon at `KH2_SUMMON_AT`
//! seconds (default 4); `KH2_DISMISS_AT=<s>` asks for Dismiss. The command menu (`items.rs`) issues
//! the same requests: the Summon entry is on the second deck, which `H` (`KH2_KEYS` `t:H`) swaps in
//! (the original's d-pad Left at the deck). `KH2_SUMMON_PARTY=1` takes both party slots as filled
//! when no companion is spawned (harness only: the original refuses Summon then, `FUN_001c78f0`).
//! Logs: `summon start`, `summon: arrival`, `summon: companions`, `summon: object`, `summon: gauge
//! mode 3`, `summon: timer` once a second, `summon: level up`, `summon end (timer)` / `summon end
//! (dismiss)`, `summon: leave`; `KH2_SUMMONLOG=1` adds the object's script mode changes and requests.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::color::Alpha;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_bdx::{Program, Runtime};
use kh2_tables::tables::{self as t, Cmd};

use crate::abilities::Abilities;
use crate::assets::Disc;
use crate::combat::{Fighter, Hit};
use crate::data::GameData;
use crate::drive::rules::{self as drive_rules, GaugeMode};
use crate::drive::{DriveRules, DriveState};
use crate::enemy::Enemy;
use crate::enemy_ai::AiTrace;
use crate::enemy_host::{BevyHost, Env};
use crate::enemy_obj::{to_game, Obj, OBJ_SIZE, SELF_BASE};
use crate::party::PartyMember;
use crate::progress::Progress;
use crate::schedule::Sim;
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::sora::{Battle, Sora, State};
use crate::world::World;
use crate::SCALE;

/// Command ids (immediates of the code).
pub mod cmd {
    /// Dismiss (`FUN_001ceda8` looks it up with `FUN_001b6f48(0x13c)`).
    pub const DISMISS: u16 = 0x13c;
    /// Peter Pan's command: byte 7 = 1 (Tinker Bell ready, `FUN_001c6168`).
    pub const PETER_PAN: u16 = 0x132;
    /// Genie's command: byte 7 = a random owned drive form (`FUN_001c6a88`).
    pub const GENIE: u16 = 0x59;
}

/// Motion requests the summon sequence plays on the player (`FUN_0017e220`).
pub mod motion {
    /// The Summon command (`FUN_001cf030`, blend `DAT_0037c358`).
    pub const SUMMON: usize = 0x8a;
    /// The summon appears without the event (0x1c6260, blend 0).
    pub const APPEAR: usize = 0x82;
}

/// The system effect started on the player by the Summon command (`FUN_001db868(.., 0x13, ..)`).
pub const EFFECT: u16 = 0x13;
/// Actor stop id of the arrival (`FUN_001c6578` -> `FUN_0016bef0(2, mask)`).
pub const STOP_ID: u16 = 2;
/// The object classes the stop leaves out: bits clear in 0xfff7ffdc (classes 0, 1, 5 and 19).
pub const STOP_MASK: u32 = 0xfff7_ffdc;
/// Script handles of summon objects: past the enemies' and the party's ranges.
const HANDLE_BASE: u32 = 0x1800;

/// The summon's friend AI object as far as the group 10 hosts of the summon scripts read and write it
/// (stage 4 of docs/summons.md: Stitch first). FUN_0010b780 picks the AI object by NeoStatus and stores it
/// at object +0xb60 (FUN_0010c5f0 reads it back for every group 10 wrapper that names the AI); Stitch's is
/// FUN_001179c0 (0x790 bytes, vtable 0x354870, 7 action records at 0x354708). Offsets are the AI
/// object's unless they say "object".
pub mod ai {
    /// Slots of the AI object's list at +0x20 (0x18 of 0x40 bytes; FUN_001176a8 clears one).
    pub const SLOTS: usize = 0x18;

    /// One 0x40-byte slot of the list (written by FUN_00117708; read by the class update FUN_00116b48,
    /// which is not ported).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Slot {
        /// +0x20 the flags (1: the object named is the AI's own object, 0x20 too when the kind is 0; 2: the
        /// object named is the player; 8: argument 5 set; 0x40000000: host 10.16).
        pub flags: u32,
        /// +0x24 the kind (-1: free, FUN_001176a8).
        pub kind: i32,
        /// +0x28 an effect instance (FUN_00117528 frees the slot once FUN_001da128 says it ended; 0: none).
        pub effect: u32,
        /// +0x2c, +0x30, +0x34, +0x38: arguments 6, 7, 4 and 3 of FUN_00117708 as it stores them.
        pub a6: u32,
        pub a7: u32,
        pub a4: u32,
        pub a3: u32,
    }

    impl Default for Slot {
        fn default() -> Self {
            Slot { flags: 0, kind: -1, effect: 0, a6: 0, a7: 0, a4: 0, a3: 0 }
        }
    }

    /// The fields of the AI object the hosts reach, plus the object fields they reach (named "object").
    #[derive(Clone, Debug)]
    pub struct SummonAi {
        pub slots: [Slot; SLOTS],
        /// +0x630 bit 0 (FUN_001175a0 sets it).
        pub flags_630: u32,
        /// Stitch's class fields (+0x620.., the position +0x640 FUN_001175a0 writes): `super::stitch`.
        pub stitch: super::stitch::StitchAi,
        /// Object +0xc60 / +0xc64 (FUN_0010ecb0): the action the class runs and its argument.
        pub action: (u32, u32),
        /// Object +0xb7c (FUN_001129f0 ors 0x40000000).
        pub b7c: u32,
        /// Object +0x7f8 bit 4 (FUN_00186580 sets it; host 1.272 reads it).
        pub flag_7f8_4: bool,
        /// Attack objects' +0x98 callbacks (host 2.74): (attack handle, routine, argument). FUN_0018d430, the
        /// clash of two attacks, is their only caller; the summon's attacks meet no other attack in the app.
        pub callbacks_98: Vec<(u32, u32, u32)>,
        /// The vtable +0x28 / +0x2c calls (hosts 10.3 / 10.4) seen, for the log.
        pub link_calls: (u32, u32),
    }

    impl Default for SummonAi {
        fn default() -> Self {
            SummonAi { slots: [Slot::default(); SLOTS], flags_630: 0, stitch: Default::default(), action: (0, 0), b7c: 0, flag_7f8_4: false, callbacks_98: Vec::new(), link_calls: (0, 0) }
        }
    }

    /// What FUN_00117708 compares the object argument with: the AI's own object and the player
    /// (FUN_00111c00(0), `DAT_0034e908`).
    #[derive(Clone, Copy, Debug)]
    pub struct Who {
        pub own: u32,
        pub player: u32,
    }

    impl SummonAi {
        /// FUN_00117668: the first slot with a negative kind and no effect; its flags are cleared. With none
        /// free the game takes index 0x18 (past the list) [the app answers 0x18 and writes nothing].
        pub fn take_slot(&mut self) -> usize {
            let i = self.slots.iter().position(|s| s.kind < 0 && s.effect == 0).unwrap_or(SLOTS);
            if let Some(s) = self.slots.get_mut(i) {
                s.flags = 0;
            }
            i
        }

        /// FUN_00117708(ai, kind, obj, a3, a4, flag, a6, a7) (host 10.12, wrapper 0x2f6d00): a slot from
        /// FUN_00117668 with +0x30 = a7, +0x34 = a4, +0x2c = a6, +0x38 = a3, +0x20 = 0, +0x24 = kind (the
        /// vectors +0x40 / +0x50 are set to 0 and (0, 0, 0, 1), not kept); then for a non-zero object: the AI's
        /// own object ors 1 into the flags (0x21 when the kind is 0, from the flags read before the or), the
        /// player 2; `flag` ors 8. Returns the slot index.
        #[allow(clippy::too_many_arguments)]
        pub fn start(&mut self, who: Who, kind: i32, obj: u32, a3: u32, a4: u32, flag: bool, a6: u32, a7: u32) -> usize {
            let i = self.take_slot();
            let Some(s) = self.slots.get_mut(i) else { return i };
            s.a7 = a7;
            s.a4 = a4;
            s.a6 = a6;
            s.a3 = a3;
            s.flags = 0;
            s.kind = kind;
            if obj != 0 {
                if obj == who.own {
                    let before = s.flags;
                    s.flags = before | 1;
                    if kind == 0 {
                        s.flags = before | 0x21;
                    }
                } else if obj == who.player {
                    s.flags |= 2;
                }
            }
            if flag {
                s.flags |= 8;
            }
            i
        }

        /// FUN_00117840 (host 10.16): slot `i` flags |= 0x40000000 (the game does not test the index).
        pub fn mark(&mut self, i: i32) {
            if let Some(s) = usize::try_from(i).ok().and_then(|i| self.slots.get_mut(i)) {
                s.flags |= 0x4000_0000;
            }
        }
    }
}

/// Stitch's friend AI (WP-SUM4b): the class FUN_001179c0 (vtable 0x354870, class 0xe at object +0xb6c), its
/// update FUN_001155e0 (object +0xb64, set by FUN_0010b780 for NeoStatus 200 in place of the members'
/// FUN_00111070) and its seven action records at 0x354708, chosen by the friend engine's FUN_0010fcd0
/// (`party_battle::choose`).
///
/// Stitch has no ground follow: FUN_001155e0 never calls the members' follow (FUN_00110468 / FUN_00110f90).
/// He moves in a plane of his own: the position +0x640 is (x, y, depth, yaw) with the depth held at
/// `DAT_00378840` / `DAT_003787d0` (1024 in the executable), x wrapped across `DAT_003787b8..c0` and y across
/// `DAT_003787c4..cc` (FUN_00116530), goals from FUN_00116780's fixed spots and two tables of six. FUN_001175a0
/// builds a matrix from it (FUN_00117320: the rotation by the yaw, the translation `+0x640 x DAT_0037882c`) at
/// AI +0x6c0 and hands it to the object as its parent matrix (FUN_00193228: object +0x790, +0x10c |= 0x40,
/// +0x768 |= 0x1000); with +0x10c bit 0x40 the object's movement adds no gravity or collision (FUN_00181e00).
/// [inferred] The plane is the camera's: the depth is constant, the wrap ranges are screen-sized, the class
/// update's slot kind 3 puts effects through the camera's inverse view (FUN_00116b48 with FUN_001068c0(1)),
/// and the blast (FUN_00118110) aims from the camera's position (FUN_001071c8 of FUN_001068c0(1)). Who reads
/// object +0x790 was not found (GAP: `mips_scan --load-off 0x790` finds no load; the decompilation has only
/// the two writers FUN_00193228 / FUN_00193248).
pub mod stitch {
    use crate::party_battle::{self as battle, Choice, Choosing, Handler, MemberNow, Params, Record};

    /// Stitch's NeoStatus and AI class (FUN_0010b780: +0xb6c = 0xe).
    pub const NEO: u16 = 200;
    pub const CLASS: u8 = 0xe;
    /// FUN_001179c0: +0xc = 7 records, +0x10 = 0x354708.
    pub const RECORDS: u32 = 0x354708;
    pub const RECORD_COUNT: u32 = 7;
    /// FUN_001179c0: +0x14 = the class block's entry with id 0x80 (FUN_00114228, the list at block +0xb0/+0xb4).
    pub const ENTRY_ID: u32 = 0x80;
    /// The handlers (record +4).
    pub const STAND: u32 = 0x117cf8;
    pub const MOVE: u32 = 0x117ea0;
    pub const BLAST: u32 = 0x118348;
    pub const REFILL: u32 = 0x118730;
    pub const LINK: u32 = 0x118db8;
    /// Motion requests (immediates): stand (FUN_00117cf8, FUN_0017e2d8 with motion 0), move (0xa1), the blast
    /// (0xcb), the MP refill (0xcc).
    pub const M_STAND: u16 = 0;
    pub const M_MOVE: u16 = 0xa1;
    pub const M_BLAST: u16 = 0xcb;
    pub const M_REFILL: u16 = 0xcc;
    /// The blast's attack object (FUN_00118110 -> FUN_0018bef0 with `atkp` 0x4ba).
    pub const BLAST_ATKP: i16 = 0x4ba;
    /// FUN_00118110's slots (FUN_00117708 kinds 0xc and 0xd with argument 0x13 on Stitch) and FUN_00118730's
    /// (kind 0xb on Stitch) and its effect on the player (FUN_0016a570(object, 10, 1, 0, player)).
    pub const BLAST_SLOTS: [(i32, bool, u32); 2] = [(0xc, false, u32::MAX), (0xd, true, 0x13)];
    pub const REFILL_SLOT: i32 = 0xb;
    pub const REFILL_EFFECT: i32 = 10;

    /// The executable's numbers the class reads.
    #[derive(Clone, Copy, Debug, PartialEq, Default)]
    pub struct Consts {
        /// `DAT_003787d0` (goal depth, FUN_00116780) and `DAT_00378840` (place depth, FUN_001175a0).
        pub depth: f32,
        pub place_depth: f32,
        /// FUN_00116780's fixed spots: `DAT_003787d4/d8` (case 0, overwritten by case 1's), `DAT_003787dc`
        /// and `DAT_003787e4` (x), `DAT_003787e0` and `DAT_003787e8` (y), `DAT_003787ec/f0` (case 6),
        /// `DAT_003787f4/f8` (case 7).
        pub x_a: f32,
        pub x_b: f32,
        pub y_a: f32,
        pub y_b: f32,
        pub case0: [f32; 2],
        pub case6: [f32; 2],
        pub case7: [f32; 2],
        /// The random spots: FUN_001166d0 (0x354808) and FUN_00116728 (0x354838), six (x, y) each.
        pub left: [[f32; 2]; 6],
        pub right: [[f32; 2]; 6],
        /// `DAT_003787fc` (the roll split), `DAT_00378800` (the shift scale), `DAT_00378804` (x shift) and
        /// `DAT_00378808` (y shift) of FUN_00116780's random branch.
        pub split: f32,
        pub shift: f32,
        pub shift_x: f32,
        pub shift_y: f32,
        /// FUN_00116530: x below `DAT_003787b8` / above `DAT_003787c0` moves by `DAT_003787bc`, y below
        /// `DAT_003787c4` / above `DAT_003787cc` by `DAT_003787c8`.
        pub wrap_x: [f32; 3],
        pub wrap_y: [f32; 3],
        /// FUN_001179c0: +0x628 = `DAT_0037885c` (the speed cap and arrival distance), +0x62c = `DAT_00378860`
        /// (the turn per frame and the arrival angle).
        pub max_speed: f32,
        pub turn: f32,
        /// FUN_00117cf8: in battle the wait is `2 w + DAT_00378864`; above `DAT_00378868` of yaw it is times
        /// `DAT_0037886c`.
        pub stand_add: f32,
        pub stand_angle: f32,
        pub stand_short: f32,
        /// FUN_00118348 / FUN_00118730: the first leg ends nearer than `DAT_00378878` / `DAT_0037887c`.
        pub near_blast: f32,
        pub near_refill: f32,
        /// FUN_00117320: the translation scale `DAT_0037882c`.
        pub scale: f32,
        /// The class block (0x34ea00 + offset[0xe]) +0x70 / +0x74: the stand wait.
        pub wait: (f32, f32),
        /// The entry 0x80: +4 the 0x674 period, +8 the HP fraction of the blast, +0xc the MP fraction of the
        /// refill, +0x10 the choice count and +0x14 the chance of FUN_001155e0's forced pick.
        pub period: f32,
        pub hp_frac: f32,
        pub mp_frac: f32,
        pub count: f32,
        pub chance: f32,
        /// `DAT_00356f40` (the frame unit) and `DAT_00356f48` (the speed unit).
        pub frame: f32,
        pub unit: f32,
    }

    fn w(elf: &[u8], va: u32) -> Option<u32> {
        crate::party_rules::exe_bytes(elf, va, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f(elf: &[u8], va: u32) -> Option<f32> {
        w(elf, va).map(f32::from_bits)
    }

    impl Consts {
        pub fn from_exe(elf: &[u8]) -> Option<Consts> {
            let block = crate::party_rules::AI_PARAMS + w(elf, crate::party_rules::AI_PARAMS + 4 * CLASS as u32)?;
            let (n, at) = (w(elf, block + 0xb0)?, block + w(elf, block + 0xb4)?);
            let entry = (0..n.min(64)).map(|k| at + k * 0x80).find(|&e| w(elf, e) == Some(ENTRY_ID))?;
            let table = |base: u32| -> Option<[[f32; 2]; 6]> {
                let mut t = [[0.0; 2]; 6];
                for (k, v) in t.iter_mut().enumerate() {
                    *v = [f(elf, base + 8 * k as u32)?, f(elf, base + 8 * k as u32 + 4)?];
                }
                Some(t)
            };
            Some(Consts {
                depth: f(elf, 0x3787d0)?,
                place_depth: f(elf, 0x378840)?,
                case0: [f(elf, 0x3787d4)?, f(elf, 0x3787d8)?],
                x_a: f(elf, 0x3787dc)?,
                y_a: f(elf, 0x3787e0)?,
                x_b: f(elf, 0x3787e4)?,
                y_b: f(elf, 0x3787e8)?,
                case6: [f(elf, 0x3787ec)?, f(elf, 0x3787f0)?],
                case7: [f(elf, 0x3787f4)?, f(elf, 0x3787f8)?],
                left: table(0x354808)?,
                right: table(0x354838)?,
                split: f(elf, 0x3787fc)?,
                shift: f(elf, 0x378800)?,
                shift_x: f(elf, 0x378804)?,
                shift_y: f(elf, 0x378808)?,
                wrap_x: [f(elf, 0x3787b8)?, f(elf, 0x3787bc)?, f(elf, 0x3787c0)?],
                wrap_y: [f(elf, 0x3787c4)?, f(elf, 0x3787c8)?, f(elf, 0x3787cc)?],
                max_speed: f(elf, 0x37885c)?,
                turn: f(elf, 0x378860)?,
                stand_add: f(elf, 0x378864)?,
                stand_angle: f(elf, 0x378868)?,
                stand_short: f(elf, 0x37886c)?,
                near_blast: f(elf, 0x378878)?,
                near_refill: f(elf, 0x37887c)?,
                scale: f(elf, 0x37882c)?,
                wait: (f(elf, block + 0x70)?, f(elf, block + 0x74)?),
                period: f(elf, entry + 4)?,
                hp_frac: f(elf, entry + 8)?,
                mp_frac: f(elf, entry + 0xc)?,
                count: f(elf, entry + 0x10)?,
                chance: f(elf, entry + 0x14)?,
                frame: f(elf, 0x356f40)?,
                unit: f(elf, 0x356f48)?,
            })
        }
    }

    /// The seven records (flags, handler; no parameter entries: their +0xc ids are 0).
    pub fn records(elf: &[u8]) -> Option<Vec<Record>> {
        (0..RECORD_COUNT)
            .map(|i| {
                let r = RECORDS + i * 0x24;
                Some(Record {
                    flags: w(elf, r)?,
                    handler: Handler::from_addr(w(elf, r + 4)?),
                    id: w(elf, r + 0xc)?,
                    entry: None,
                    ability: w(elf, r + 0x14)?,
                    item: w(elf, r + 0x18)?,
                    setting: w(elf, r + 0x1c)? as i32,
                    mode: String::new(),
                })
            })
            .collect()
    }

    /// The handler address of a record (`Handler::Other` for Stitch's own).
    pub fn addr(r: &Record) -> u32 {
        match r.handler {
            Handler::Other(a) => a,
            _ => 0,
        }
    }

    /// Stitch's AI fields (offsets of the AI object unless they say "object").
    #[derive(Clone, Debug, Default)]
    pub struct StitchAi {
        /// +0x640: (x, y, depth, yaw); +0x650 the goal (x, y, depth, yaw); +0x660 the direction and +0x66c
        /// the speed; +0x65c is the goal's w, the wanted yaw.
        pub pos: [f32; 4],
        pub goal: [f32; 4],
        pub dir: [f32; 3],
        pub speed: f32,
        /// +0x634: the phase of the blast and the refill.
        pub phase: i32,
        /// +0x788: choices since the last forced pick (FUN_001155e0).
        pub count: i32,
        /// Object +0xcd4: the stand wait (FUN_0010f648 / FUN_0010f670).
        pub timer: f32,
        /// FUN_0010fcd0's state (object +0xc60, +0xb7c bit 4, the last three).
        pub choosing: Choosing,
        /// Object +0x10c bit 0x40 / +0x790: the parent matrix is set (FUN_00193228).
        pub attached: bool,
        /// The motion requested last (object +0x154) and whether a new request is pending.
        pub motion: Option<u16>,
        pub motion_new: bool,
        /// +0x620 / +0x624: the blast's attack object and the motion it lives in (FUN_00118110).
        pub attack: Option<u16>,
        pub rng: Rng,
    }

    /// [inferred] A stand-in for the game's generator FUN_00106308: an xorshift, as party.rs's `Manager`
    /// (the game's generator is not ported).
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Rng(pub u32);

    impl Rng {
        /// FUN_0010f420(n): 0..n.
        pub fn rand(&mut self, n: u32) -> u32 {
            if self.0 == 0 {
                self.0 = 0x2545_f491;
            }
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0 % n.max(1)
        }

        /// FUN_0010f438: 0..1, as party.rs's stand-in.
        pub fn roll(&mut self) -> f32 {
            self.rand(20001) as f32 / 20000.0
        }
    }

    /// `update` with the AI's own generator.
    pub fn tick(c: &Consts, recs: &[Record], ai: &mut StitchAi, now: &Now) -> Out {
        let rng = std::cell::RefCell::new(std::mem::take(&mut ai.rng));
        let out = update(c, recs, ai, now, &mut || rng.borrow_mut().roll(), &mut |n| rng.borrow_mut().rand(n));
        ai.rng = rng.into_inner();
        out
    }

    /// What the update reads of the game.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Now {
        /// FUN_001903d8 (a battle) and FUN_001b3c00 (a limit task).
        pub battle: bool,
        pub limit: bool,
        /// The player's status block: HP / max HP (+0, +4) and MP / max MP (+0x180, +0x184).
        pub hp: (i32, i32),
        pub mp: (i32, i32),
        /// FUN_00118068: enemies (FUN_0010cae8) not out of view (FUN_0019ada8).
        pub in_view: usize,
        /// The body's motion ended (object +0x160 bit 2 [inferred]).
        pub motion_done: bool,
    }

    /// What one update asks of the world.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Out {
        /// A record started (index, forced by FUN_001155e0's pick).
        pub started: Option<(usize, bool)>,
        /// FUN_00118110 runs this frame (the attack object exists and hits).
        pub blast: bool,
        /// FUN_001a0240 on the player's status (the MP refill).
        pub refill: bool,
        /// AI slots started (kind, on Stitch, argument 3) and effects on the player (FUN_0016a570).
        pub slots: Vec<(i32, bool, u32)>,
        pub player_effect: Option<i32>,
    }

    /// FUN_00106408 as the party uses it: a value in `[a, b]` from a 0..1 roll [inferred].
    fn between(roll: f32, a: f32, b: f32) -> f32 {
        a + (b - a) * roll
    }

    /// FUN_001166d0 (`left`) / FUN_00116728 (`right`): one of six spots by FUN_0010f420(6), into the goal's x and
    /// y. [inferred] The calls the decompilation shows with two arguments write y too (the third argument
    /// register still holds +0x654 there).
    fn pick(t: &[[f32; 2]; 6], goal: &mut [f32; 4], rand: &mut dyn FnMut(u32) -> u32) {
        let k = (rand(6) as usize).min(5);
        goal[0] = t[k][0];
        goal[1] = t[k][1];
    }

    /// FUN_00116780(ai, case): the goal's depth, then the case's spot; case 5 is the origin; any other case the
    /// random branch: below the split a spot on Stitch's side, else a spot of a side shifted by FUN_0010f420(4)'s
    /// rule (0, 1: across in x by the roll's shift or the wrap width; 2, 3: in y).
    pub fn goal(c: &Consts, case: i32, pos: [f32; 4], goal: &mut [f32; 4], roll: &mut dyn FnMut() -> f32, rand: &mut dyn FnMut(u32) -> u32) {
        goal[2] = c.depth;
        match case {
            0 | 1 => (goal[0], goal[1]) = (c.x_a, c.y_a),
            2 => (goal[0], goal[1]) = (c.x_b, c.y_a),
            3 => (goal[0], goal[1]) = (c.x_b, c.y_b),
            4 => (goal[0], goal[1]) = (c.x_a, c.y_b),
            5 => (goal[0], goal[1]) = (0.0, 0.0),
            6 => (goal[0], goal[1]) = (c.case6[0], c.case6[1]),
            7 => (goal[0], goal[1]) = (c.case7[0], c.case7[1]),
            _ => {
                let r = roll();
                if r < c.split {
                    if 0.0 <= pos[0] { pick(&c.right, goal, rand) } else { pick(&c.left, goal, rand) }
                    return;
                }
                let s = (roll() * c.split + c.split) * c.shift;
                let left_side = pos[0] < 0.0;
                match rand(4) {
                    // Across in x: a spot of the other side, moved by the wrap width (1) or the shift (0).
                    k @ (0 | 1) => {
                        let d = if k == 1 { c.shift_x } else { s };
                        if left_side {
                            pick(&c.right, goal, rand);
                            goal[0] -= d;
                        } else {
                            pick(&c.left, goal, rand);
                            goal[0] += d;
                        }
                    }
                    // In y: a spot of Stitch's side, y down by the height (3) or up by the height less the shift (2).
                    k @ (2 | 3) => {
                        if left_side { pick(&c.left, goal, rand) } else { pick(&c.right, goal, rand) }
                        if k == 3 {
                            goal[1] -= c.shift_y;
                        } else {
                            goal[1] += c.shift_y - s;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// FUN_00116530 (with AI +0x630 bit 0): position and goal both beyond an edge move across together.
    pub fn wrap(c: &Consts, pos: &mut [f32; 4], goal: &mut [f32; 4]) {
        for (k, [lo, step, hi]) in [(0usize, c.wrap_x), (1, c.wrap_y)] {
            if pos[k] < lo && goal[k] < lo {
                pos[k] += step;
                goal[k] += step;
            }
            if hi < pos[k] && hi < goal[k] {
                pos[k] -= step;
                goal[k] -= step;
            }
        }
    }

    /// The handlers' shared step: the direction to the goal (x, y, depth) and the speed, the distance times
    /// `DAT_00356f48` capped at +0x628. Returns the distance.
    pub fn toward(c: &Consts, ai: &mut StitchAi) -> f32 {
        let d = [ai.goal[0] - ai.pos[0], ai.goal[1] - ai.pos[1], ai.goal[2] - ai.pos[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        ai.dir = if len > 0.0 { [d[0] / len, d[1] / len, d[2] / len] } else { [0.0; 3] };
        ai.speed = (len * c.unit).min(c.max_speed);
        len
    }

    /// FUN_00116aa0: nearer than +0x628 (times the frame unit) and turned to within +0x62c of the goal's yaw.
    pub fn arrived(c: &Consts, pos: [f32; 4], goal: [f32; 4]) -> bool {
        let d = [pos[0] - goal[0], pos[1] - goal[1], pos[2] - goal[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        len < c.max_speed * c.frame && crate::camera::rules::wrap(pos[3] - goal[3]).abs() < c.turn * c.frame
    }

    /// FUN_00116658: with a direction in x or y, the wanted yaw +0x65c = atan2(x, -y) (FUN_003003e8).
    pub fn face(ai: &mut StitchAi) {
        if ai.dir[0] != 0.0 || ai.dir[1] != 0.0 {
            ai.goal[3] = ai.dir[0].atan2(-ai.dir[1]);
        }
    }

    /// FUN_001175a0 (host 10.1): +0x630 |= 1 (the caller's), the position and the wanted yaw from the vector,
    /// the depth `DAT_00378840`, and once the parent matrix (FUN_00117320, FUN_00193228). The yaw loses the
    /// motion's root rotation (FUN_0017ecf0's w) [inferred 0: the summon's root motion is not read].
    pub fn place(c: &Consts, ai: &mut StitchAi, v: [f32; 4]) {
        ai.pos = v;
        ai.goal[3] = v[3];
        ai.pos[2] = c.place_depth;
        ai.attached = true;
    }

    fn request(ai: &mut StitchAi, m: u16) {
        ai.motion = Some(m);
        ai.motion_new = true;
    }

    /// FUN_0010f490 / FUN_0010f4d0: the value below `frac` of its maximum (`(int)(max x frac) > value`).
    pub fn below(v: (i32, i32), frac: f32) -> bool {
        ((v.1 as f32 * frac) as i32) > v.0
    }

    /// A record's phase 1 (the handler called with the test flag): `cont` is +0xb7c bit 4.
    pub fn can(c: &Consts, recs: &[Record], ai: &StitchAi, now: &Now, i: usize, cont: bool) -> bool {
        let gates = !now.limit;
        // FUN_001180c0: the player's HP below the entry's fraction and an enemy in view.
        let blast_fresh = gates && below(now.hp, c.hp_frac) && now.in_view > 0;
        // FUN_0010ed20: another record with flag 2 feasible (fresh) [the record FUN_0010fc68 names is none for
        // a class above 13]; of Stitch's flag-2 records only the blast can start fresh (FUN_00118db8 never does;
        // FUN_00118ec0 and FUN_00119068 were not read and count as never).
        let other = |me: usize| recs.iter().enumerate().any(|(k, r)| k != me && r.flags & battle::flag::ATTACK != 0 && addr(r) == BLAST && blast_fresh);
        match recs.get(i).map(addr) {
            Some(STAND) => gates && !other(i) && (!cont || ai.timer > 0.0),
            Some(MOVE) => gates && !other(i) && if cont { !arrived(c, ai.pos, ai.goal) } else { ai.choosing.current != Some(i) },
            Some(BLAST) => gates && if cont { ai.phase != 4 } else { blast_fresh },
            Some(REFILL) => gates && if cont { ai.phase != 3 } else { below(now.mp, c.mp_frac) },
            Some(LINK) => cont && ai.motion == Some(0xc9),
            _ => false,
        }
    }

    /// A record's phase 0 (the handler called without the test flag).
    #[allow(clippy::too_many_arguments)]
    pub fn run(c: &Consts, recs: &[Record], ai: &mut StitchAi, now: &Now, i: usize, cont: bool, out: &mut Out, roll: &mut dyn FnMut() -> f32, rand: &mut dyn FnMut(u32) -> u32) {
        match recs.get(i).map(addr) {
            // FUN_00117cf8: a new wait from the class block's +0x70..+0x74 (in battle 2 w + 60, a third and less
            // when turned away), motion 0, standing.
            Some(STAND) => {
                if !cont {
                    let (mut a, mut b) = c.wait;
                    if now.battle {
                        a = a + a + c.stand_add;
                        b = b + b + c.stand_add;
                    }
                    if crate::camera::rules::wrap(ai.pos[3]).abs() > c.stand_angle {
                        a *= c.stand_short;
                        b *= c.stand_short;
                    }
                    ai.timer = between(roll(), a, b);
                }
                request(ai, M_STAND);
                ai.dir = [0.0; 3];
            }
            // FUN_00117ea0: a random goal and motion 0xa1, then the step toward it and the facing.
            Some(MOVE) => {
                if !cont {
                    let pos = ai.pos;
                    goal(c, -1, pos, &mut ai.goal, roll, rand);
                    request(ai, M_MOVE);
                }
                toward(c, ai);
                face(ai);
            }
            Some(h @ (BLAST | REFILL)) => {
                let (case, near, last) = if h == BLAST { (6, c.near_blast, 3) } else { (7, c.near_refill, 2) };
                if !cont {
                    ai.phase = 0;
                    let pos = ai.pos;
                    goal(c, case, pos, &mut ai.goal, roll, rand);
                }
                match ai.phase {
                    0 => {
                        let d = toward(c, ai);
                        face(ai);
                        if d < near {
                            ai.goal[3] = 0.0;
                            ai.phase = 1;
                        }
                    }
                    1 => {
                        toward(c, ai);
                        if arrived(c, ai.pos, ai.goal) {
                            ai.phase = 2;
                        }
                    }
                    p if (2..=last).contains(&p) => {
                        ai.dir = [0.0; 3];
                        ai.pos = ai.goal;
                        if h == REFILL {
                            // FUN_00118730 phase 2: motion 0xcc, FUN_001a0240 on the player, effect 10 on him, a slot.
                            request(ai, M_REFILL);
                            out.refill = true;
                            out.player_effect = Some(REFILL_EFFECT);
                            out.slots.push((REFILL_SLOT, true, u32::MAX));
                            ai.phase += 1;
                        } else {
                            // FUN_00118348 phases 2 and 3: motion 0xcb and the two slots once, then the blast each
                            // frame while 0xcb plays; the phase moves on when it ended.
                            if p == 2 {
                                request(ai, M_BLAST);
                                out.slots.extend(BLAST_SLOTS);
                                ai.phase += 1;
                            } else if ai.motion == Some(M_BLAST) {
                                if now.motion_done {
                                    ai.phase += 1;
                                }
                            } else {
                                ai.phase += 1;
                            }
                            if ai.attack.is_none() {
                                ai.attack = Some(M_BLAST);
                            }
                            out.blast = true;
                        }
                    }
                    _ => {}
                }
            }
            _ => ai.dir = [0.0; 3],
        }
    }

    /// FUN_001155e0 while the object is free: the wrap, the stopped direction, the forced pick (in battle, no
    /// limit, more than the entry's count of choices, the entry's chance: the refill when MP is the lower
    /// fraction, else the blast) or FUN_0010fcd0, the turn toward the wanted yaw and the step; then the class
    /// update's release of the attack object (FUN_00116b48: the motion is not 0xcb any more or it ended).
    pub fn update(c: &Consts, recs: &[Record], ai: &mut StitchAi, now: &Now, roll: &mut dyn FnMut() -> f32, rand: &mut dyn FnMut(u32) -> u32) -> Out {
        let mut out = Out::default();
        if !ai.attached {
            // FUN_001155e0: +0x630 bit 0 clear -> FUN_001175a0 with the zero vector.
            place(c, ai, [0.0; 4]);
        }
        let (mut pos, mut g) = (ai.pos, ai.goal);
        wrap(c, &mut pos, &mut g);
        (ai.pos, ai.goal) = (pos, g);
        ai.dir = [0.0; 3];
        if ai.timer > 0.0 {
            // [inferred] The wait counts down one frame unit per frame (as party.rs's +0xcd4).
            ai.timer -= c.frame;
        }
        let forced = now.battle && !now.limit && c.count < ai.count as f32 && {
            ai.count = 0;
            roll() < c.chance
        };
        if forced {
            let k = if (now.mp.0 as i64) * (now.hp.1 as i64) < (now.hp.0 as i64) * (now.mp.1 as i64) { REFILL } else { BLAST };
            if let Some(i) = recs.iter().position(|r| addr(r) == k) {
                ai.choosing.current = Some(i);
                ai.choosing.continuing = false;
                run(c, recs, ai, now, i, false, &mut out, roll, rand);
                out.started = Some((i, true));
            }
        } else {
            let before = ai.choosing.current;
            let mut st = std::mem::take(&mut ai.choosing);
            st.current = before;
            let snapshot = ai.clone();
            let choice = {
                let mut can_f = |i: usize, cont: bool| {
                    let mut s = snapshot.clone();
                    s.choosing.current = before;
                    can(c, recs, &s, now, i, cont)
                };
                battle::choose(recs, &mut st, &Params::default(), CLASS, &MemberNow::default(), false, false, |_| None, &mut can_f, || roll(), |n| rand(n))
            };
            ai.choosing = st;
            match choice {
                Choice::Keep(i) => run(c, recs, ai, now, i, true, &mut out, roll, rand),
                Choice::Do(i) => {
                    run(c, recs, ai, now, i, false, &mut out, roll, rand);
                    out.started = Some((i, false));
                }
                // [inferred] FUN_001103f8 (the members' move when nothing is chosen) was not read for Stitch:
                // he stands.
                _ => ai.dir = [0.0; 3],
            }
            if before != ai.choosing.current || !ai.choosing.continuing {
                ai.count += 1;
            }
        }
        // The turn toward +0x65c by +0x62c and the step: +0x640 += direction x speed x frame.
        ai.pos[3] = crate::camera::rules::approach_angle(ai.pos[3], ai.goal[3], c.turn * c.frame);
        for k in 0..3 {
            ai.pos[k] += ai.dir[k] * ai.speed * c.frame;
        }
        if ai.attack.is_some() && (ai.motion != Some(M_BLAST) || (now.motion_done && !ai.motion_new)) {
            ai.attack = None;
        }
        out
    }

    /// FUN_00117320's translation: the position in the parent's (the camera's [inferred]) space, game units
    /// and axes (x right, y down, z forward).
    pub fn camera_point(c: &Consts, pos: [f32; 4]) -> [f32; 3] {
        [pos[0] * c.scale, pos[1] * c.scale, pos[2] * c.scale]
    }
}

/// The pure rules of stage 3, each with the address it ports.
pub mod rules {
    use bevy::prelude::Vec3;

    /// The LCG the game steps for its random picks (`DAT_003716a0 *= 0x10dcd`).
    pub fn lcg(state: &mut u32) -> u32 {
        *state = state.wrapping_mul(0x10dcd);
        *state
    }

    /// `FUN_001c6a88`: Genie's form byte. Steps the LCG once and picks among the owned forms (the list
    /// `FUN_001c6a20` builds: forms 1..4 in order whose form item is owned) by dividing the new word by
    /// `(2^32 + n - 1) / n` (`FUN_00325b38`, an unsigned 64-bit division). With no form owned the game
    /// divides by zero; the port answers 0 then [inferred].
    pub fn genie_form(state: &mut u32, owned: &[u8]) -> u8 {
        let r = lcg(state);
        let n = owned.len() as u64;
        if n == 0 {
            return 0;
        }
        let step = (1u64 << 32).div_ceil(n);
        owned.get((r as u64 / step) as usize).copied().unwrap_or(0)
    }

    /// `FUN_001c65f0`: the arrival plays as an event. `option` is the Summon Effects field (bits 9..10 of
    /// the option word); `seen` the word `DAT_01d5e370` (bit = the 1-based summon index), set when the
    /// event plays; `player_810` the player's +0x810. Checked against the game by the oracle in
    /// `crates/kh2/tests/summon_oracle.rs` and `summon_arrival_choice_matches_the_game`.
    pub fn arrival_event(option: u32, seen: &mut u32, index: u8, player_810: u32) -> bool {
        if player_810 != 0 {
            return false;
        }
        let bit = 1u32 << (index as u32 & 31);
        let event = match option & 3 {
            1 => true,
            2 => *seen & bit == 0,
            _ => false,
        };
        if event {
            *seen |= bit;
        }
        event
    }

    /// The executable's constants of the free-spot search (`FUN_001a6068` / `FUN_001a6198`).
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct SpotConsts {
        /// `DAT_0037b5a8`: the angle of one of the eight start sectors.
        pub sector: f32,
        /// `DAT_0037b5ac`: the scale that turns the LCG word into a fraction.
        pub unit: f32,
        /// `DAT_0037b5b0`: the radius step after eight tries (also the lift of the second point).
        pub radius_step: f32,
        /// `DAT_0037b5b4`: the angle step between tries.
        pub angle_step: f32,
        /// `DAT_0037b5b8`: how much farther than the radius the second point lies.
        pub beyond: f32,
        /// `DAT_0037b5bc`, `DAT_0037b5c0`: the floor probe's two arguments.
        pub probe: [f32; 2],
    }

    /// The search record `FUN_001a6068` fills (game space: y points down).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Spot {
        pub origin: [f32; 4],
        pub min: f32,
        pub radius: f32,
        pub angle: f32,
        pub tries: i32,
        pub flags: u32,
    }

    /// The LCG word as a float, read unsigned (0 .. 2^32; checked by the oracle
    /// `summon_spot_start_and_genie_form_match_the_game`).
    fn word_f(w: u32) -> f32 {
        w as f32
    }

    /// `FUN_001a6068(min, max, out, origin, flags)`: a radius between `min` and `max` and a start angle
    /// (one of eight sectors in the order 0, 2, 4, 6, 1, 3, 5, 7 of the counter `DAT_0035e9b0`, plus a
    /// random part of a sector).
    pub fn spot_start(c: &SpotConsts, rng: &mut u32, sector: &mut i32, min: f32, max: f32, origin: [f32; 4], flags: u32) -> Spot {
        let r1 = super::rules::lcg(rng);
        let radius = min + word_f(r1) * c.unit * (max - min);
        let r2 = super::rules::lcg(rng);
        let s = *sector;
        let order = ((s & 4) >> 2) | (s << 1);
        let angle = order as f32 * c.sector + word_f(r2) * c.unit * c.sector;
        *sector = (s + 1).rem_euclid(8);
        Spot { origin, min, radius, angle, tries: 0, flags }
    }

    /// `FUN_001a6198`: tries eight directions per radius, from the start angle on, shrinking the radius
    /// by `radius_step` until it is below the minimum. `floor(p)` stands for `FUN_001a5c30` (a floor
    /// under the point: its position, game space) and `clear(origin, p)` for `FUN_001a5bf8` (the way to
    /// the second point is free). The direction is (sin, 0, cos) of the angle (`FUN_00300278`,
    /// `FUN_003000d8`; checked by `summon_spot_trig_is_sin_and_cos`). Without a hit the floor under the
    /// origin is taken.
    pub fn spot_find(c: &SpotConsts, s: &mut Spot, floor: &dyn Fn([f32; 4]) -> Option<[f32; 4]>, clear: &dyn Fn([f32; 4], [f32; 4]) -> bool) -> Option<[f32; 4]> {
        while s.min <= s.radius {
            s.tries += 1;
            let a = s.angle + s.tries as f32 * c.angle_step;
            let (x, z) = (a.sin(), a.cos());
            let at = |r: f32| [s.origin[0] + x * r, s.origin[1], s.origin[2] + z * r, s.origin[3]];
            if let Some(hit) = floor(at(s.radius)) {
                let mut far = at(s.radius + c.beyond);
                far[1] = hit[1] - c.radius_step;
                if clear(s.origin, far) {
                    return Some(hit);
                }
            }
            if s.tries >= 8 {
                s.tries = 0;
                s.radius -= c.radius_step;
            }
        }
        floor(s.origin)
    }

    /// A game-space point as an app position.
    pub fn app(p: [f32; 4]) -> Vec3 {
        Vec3::new(p[0], -p[1], -p[2])
    }
}

/// The executable's constants this module reads (addresses in the doc comments of `rules`).
#[derive(Clone, Copy, Debug, Default)]
pub struct SummonConsts {
    /// `DAT_0037bff0`: the companions' fade-out in frames.
    pub fade_out: f32,
    /// `DAT_0037c358`: the blend of motion 0x8a.
    pub blend: f32,
    /// `DAT_0037c00c`, `DAT_0037c010`: the summon's spot, 200..300 units from the player.
    pub spot: [f32; 2],
    /// `DAT_0037c058` (lift), `DAT_0037c05c`, `DAT_0037c060` (range), `DAT_0037c064` (fade frames),
    /// `DAT_0037c068` (fade target): the companions' return (`FUN_001c7ab0`).
    pub ret_lift: f32,
    pub ret_spot: [f32; 2],
    pub ret_fade: f32,
    pub ret_alpha: f32,
    pub search: rules::SpotConsts,
    /// The initial words of `DAT_003716a0` (the LCG) and `DAT_0035e9b0` (the sector counter).
    pub rng: u32,
    pub sector: i32,
}

impl SummonConsts {
    pub fn from_exe(elf: &[u8]) -> Option<SummonConsts> {
        let w = |va: u32| exe_word(elf, va);
        let f = |va: u32| w(va).map(f32::from_bits);
        Some(SummonConsts {
            fade_out: f(0x37bff0)?,
            blend: f(0x37c358)?,
            spot: [f(0x37c00c)?, f(0x37c010)?],
            ret_lift: f(0x37c058)?,
            ret_spot: [f(0x37c05c)?, f(0x37c060)?],
            ret_fade: f(0x37c064)?,
            ret_alpha: f(0x37c068)?,
            search: rules::SpotConsts {
                sector: f(0x37b5a8)?,
                unit: f(0x37b5ac)?,
                radius_step: f(0x37b5b0)?,
                angle_step: f(0x37b5b4)?,
                beyond: f(0x37b5b8)?,
                probe: [f(0x37b5bc)?, f(0x37b5c0)?],
            },
            rng: w(0x3716a0)?,
            // In the zero-filled data when the file has no bytes for it.
            sector: w(0x35e9b0).unwrap_or(0) as i32,
        })
    }
}

/// A word of the executable at a virtual address (through the program headers).
fn exe_word(elf: &[u8], va: u32) -> Option<u32> {
    let u16_at = |o: usize| elf.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |o: usize| elf.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if elf.get(0..4)? != b"\x7fELF" {
        return None;
    }
    let (phoff, phentsize, phnum) = (u32_at(0x1c)? as usize, u16_at(0x2a)?, u16_at(0x2c)?);
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        let (kind, off, vaddr, filesz) = (u32_at(p)?, u32_at(p + 4)?, u32_at(p + 8)?, u32_at(p + 16)?);
        if kind == 1 && va >= vaddr && va - vaddr + 4 <= filesz {
            return u32_at(off as usize + (va - vaddr) as usize);
        }
    }
    None
}

/// The disc data the summons read.
#[derive(Resource)]
pub struct SummonData {
    /// The `sumn` rows (1-based index = row + 1).
    pub rows: Vec<t::Sumn>,
    pub cmds: Vec<Cmd>,
    pub tuning: drive_rules::SummonTuning,
    pub consts: Option<SummonConsts>,
    /// Stitch's class (WP-SUM4b): its constants and records, and FUN_0017da30's motion slot orders.
    pub stitch: Option<(stitch::Consts, Vec<crate::party_battle::Record>)>,
    motion_orders: Option<[[u8; 4]; 4]>,
    objs: Vec<t::Objentry>,
    host_table: Option<kh2_bdx::HostTable>,
    /// Command names for the log (`msg/us/sys.bar`).
    names: Vec<(u16, String)>,
    /// Loaded models and scripts by model name (`None`: not loadable).
    rigs: HashMap<String, Option<Arc<Rig>>>,
    scripts: HashMap<String, Option<Program>>,
}

impl SummonData {
    fn load(disc: &Disc) -> Option<SummonData> {
        let exe = disc.read_root("SLUS_210.05");
        let stitch = exe.as_deref().and_then(|e| Some((stitch::Consts::from_exe(e)?, stitch::records(e)?)));
        if stitch.is_none() {
            warn!("summon: Stitch's class constants (FUN_001179c0) not readable: Stitch stands where he appears");
        }
        let motion_orders = exe.as_deref().and_then(crate::party_battle::motion_orders);
        let consts = exe.and_then(|exe| SummonConsts::from_exe(&exe).map(|c| (c, kh2_bdx::HostTable::from_exe(&exe))));
        let mut a = disc.0.lock().ok()?;
        let rows = kh2_tables::load::<t::Sumn>(&mut a, &t::SUMN).ok()?.rows;
        let cmds = kh2_tables::load::<Cmd>(&mut a, &t::CMD).ok()?.rows;
        let objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default();
        let sstm = kh2_tables::player::load_sstm(&mut a).ok()?;
        let msg = kh2_tables::player::load_messages(&mut a);
        let names = rows
            .iter()
            .map(|r| r.command)
            .chain([cmd::DISMISS])
            .map(|c| {
                let n = cmds.iter().find(|x| x.id == c).and_then(|x| msg.as_ref()?.get(x.message_id as u32)).unwrap_or_else(|| format!("command {c}"));
                (c, n)
            })
            .collect();
        let (consts, host_table) = consts.map_or((None, None), |(c, h)| (Some(c), h));
        Some(SummonData {
            rows,
            cmds,
            tuning: drive_rules::SummonTuning::from_sstm(&sstm),
            consts,
            stitch,
            motion_orders,
            objs,
            host_table,
            names,
            rigs: HashMap::new(),
            scripts: HashMap::new(),
        })
    }

    pub fn name(&self, c: u16) -> String {
        self.names.iter().find(|n| n.0 == c).map_or_else(|| format!("command {c}"), |n| n.1.clone())
    }

    /// `FUN_001d4ca0`: the 1-based row index of a summon command.
    pub fn index_of(&self, c: u16) -> Option<u8> {
        self.rows.iter().position(|r| r.command == c).map(|i| i as u8 + 1)
    }

    fn cmd(&self, c: u16) -> Option<&Cmd> {
        self.cmds.iter().find(|x| x.id == c)
    }

    fn row_name(&self, index: u8) -> String {
        self.rows.get((index as usize).wrapping_sub(1)).map_or_else(String::new, |r| self.name(r.command))
    }

    /// The model with its motion set (`obj/<model>.mdlx`, `obj/<mset>.mset`) and its script (BAR entry
    /// of type 3 of the model file).
    fn object(&mut self, disc: &Disc, row: &t::Objentry) -> (Option<Arc<Rig>>, Option<Program>) {
        let model = row.model_name.clone();
        let mset = row.animation_name.trim_end_matches(".mset").to_string();
        let rig = self.rigs.entry(model.clone()).or_insert_with(|| crate::drive::read_rig(disc, &model, &mset)).clone();
        let prog = self
            .scripts
            .entry(model.clone())
            .or_insert_with(|| {
                let file = disc.read(&format!("obj/{model}.mdlx"))?;
                let entry = kh2_bar::parse(&file).ok()?.into_iter().find(|e| e.kind == 3)?.data.to_vec();
                Program::parse(&entry).map_err(|e| warn!("summon: {model}: script not readable: {e}")).ok()
            })
            .clone();
        (rig, prog)
    }
}

/// The summon's state (the game keeps it in `DAT_00340c88` bytes 5..7, `DAT_00340e4c`,
/// `DAT_01d5e368`, the words `0x35feb8` / `0x35fec0` and the two object slots at 0x1d5e360).
#[derive(Resource, Clone, Debug, Default)]
pub struct SummonState {
    /// The active summon, a 1-based `sumn` index (byte 5), 0 for none.
    pub index: u8,
    /// Summon level (byte 6) and experience (`DAT_00340e4c`).
    pub exp: drive_rules::SummonExp,
    /// Genie's form / Tinker Bell byte (byte 7).
    pub extra: u8,
    /// `DAT_01d5e368`: 0 none, 1 the arrival event plays, 2 active, 3 leaving.
    pub state: u8,
    /// `0x35feb8` bit 0: a summon sequence runs (set at the start, cleared by the leave thread).
    pub sequence: bool,
    /// The arrival wait left in frames while the arrival thread (0x1c6260) runs.
    pub arrival: Option<f32>,
    /// The arrival plays as an event (`FUN_001c65f0`).
    pub event: bool,
    /// `DAT_01d5e370`: the summons whose arrival event has played (bit = index).
    pub seen: u32,
    /// `0x35fec0` bit 3 with actor stop id 2 on the actors of `STOP_MASK` (`FUN_001c6578` ..
    /// `FUN_001c65b8`).
    pub stop: bool,
    /// The player's +0x108 bits the sequence sets: 0x100 (no hit reaches him, `FUN_00186638`), 0x4
    /// (busy, `FUN_0019a530`), 0x1 (from mode 3 to the end of the arrival) [meanings inferred].
    pub player_flags: u32,
    /// The summon objects (0x1d5e360 / 0x1d5e364).
    pub slots: [Option<Entity>; 2],
    /// The NeoStatus of each summon object (what `FUN_00175ba0` finds a summon limit's member by).
    pub present: Vec<u16>,
    /// Each summon command's use counter (+0x34184a + 2 x index, capped at 999).
    pub uses: [u16; 4],
    /// The SUMMONS list (`FUN_001d4d48`): every `sumn` command whose charm is owned, in row order.
    pub commands: Vec<u16>,
    /// The summon level cap (`FUN_001d4e68`).
    pub cap: u8,
    /// `FUN_001c78f0` this tick: both party slots hold members in the party.
    pub party_ok: bool,
    /// The limit task exists (`FUN_001b3c00`: `DAT_01d5d6b0` != 0), from limit.rs.
    pub limit_task: bool,
    /// `DAT_003716a0` and `DAT_0035e9b0` (the game's LCG and the free-spot sector counter).
    pub rng: u32,
    pub sector: i32,
    /// The leave thread (0x1c6810) runs.
    leaving: bool,
    /// The mode-3 tick counter (for the once-a-second log line).
    ticks: u32,
}

impl SummonState {
    /// The summon timer is non-zero (what limit.rs calls the summon gauge, status +0x1b4 in mode 3).
    pub fn gauge(&self, drive: &DriveState) -> bool {
        drive.gauge_mode == GaugeMode::Summon && drive.timer.left > 0.0
    }

    /// Dismiss is available (`FUN_001ba2c0` case 0x13c: the state is 2 and `FUN_001b3c00` finds no
    /// limit task).
    pub fn dismiss_ok(&self) -> bool {
        drive_rules::dismiss_available(self.state, self.limit_task)
    }

    /// A summon command is available (`FUN_001ba568` with `FUN_001c7970`).
    pub fn command_ok(&self, drive: &DriveState, c: &Cmd) -> bool {
        drive_rules::summon_available(&drive_rules::SummonCtx {
            busy: self.sequence || self.arrival.is_some(),
            form: drive.object_form(),
            party_ok: self.party_ok,
            locked: false,
            points: drive.points.whole,
            cost: c.cost,
        })
    }
}

/// Requests to the summon system, from the command menu, limit.rs or the harness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SummonRequest {
    /// A summon command (its id).
    Summon(u16),
    Dismiss,
    /// A change of the mode-3 timer through `FUN_001788d8` (already multiplied by `sstm[0x70]`).
    Gauge(f32),
}

#[derive(Resource, Default)]
pub struct SummonQueue(pub Vec<SummonRequest>);

/// The scripted harness.
#[derive(Resource, Default)]
struct Harness {
    summon: Option<(f32, u16)>,
    dismiss_at: Option<f32>,
    force_party: bool,
    log: bool,
}

/// What the HUD's drive gauge showed before mode 3 (put back when the mode leaves 3).
#[derive(Resource, Default)]
struct HudSaved(Option<crate::hud::hud_gauge::DriveGauge>);

/// What `tick` asks the systems with world access to do this tick, in order.
#[derive(Clone, Debug, PartialEq)]
enum Act {
    /// A motion request on the player, with its blend (`FUN_0017e220`).
    PlayerMotion(usize, f32),
    /// System effect 0x13 bound to the player.
    EffectStart,
    /// The effect ends its emitters (`FUN_001d9f48`) or goes at once (`FUN_001da028`).
    EffectEnd { kill: bool },
    /// The companions fade out and are listed (`FUN_001c7740`).
    CompanionsOut(f32),
    /// The listed companions come back (`FUN_001c79c8`).
    CompanionsBack { frames: f32, alpha: f32 },
    /// The summon object of `sumn` row `index` at the player (no event: then at a free spot) with its
    /// script switched to `mode`.
    Spawn { index: u8, mode: &'static str, skip: bool },
    /// The first summon object's script switches to `leave` (0x1c6810).
    Leave,
}

#[derive(Resource, Default)]
struct Acts(Vec<Act>);

/// A companion faded out by the summon (+0x7f8 bit 0x10, listed at 0x1d5e380), or fading back in.
#[derive(Component)]
pub struct Away {
    fade: Fade,
    /// The fade-out has ended: the member counts as gone (`FUN_0017d470`).
    pub gone: bool,
    returning: bool,
}

#[derive(Clone, Copy, Debug)]
struct Fade {
    from: f32,
    to: f32,
    frames: f32,
    t: f32,
}

impl Fade {
    fn step(&mut self) -> (f32, bool) {
        self.t += 1.0;
        let k = if self.frames <= 0.0 { 1.0 } else { (self.t / self.frames).min(1.0) };
        (self.from + (self.to - self.from) * k, k >= 1.0)
    }
}

/// A summon object (the friend class with flag 0x400000 at +0x588, vtable 0x35fe18) and its script.
#[derive(Component)]
pub struct SummonObject {
    pub neo: u16,
    pub model: String,
    pub body: Entity,
    /// The slot it took (0x1d5e360 + 4 x slot).
    pub slot: u8,
    rt: Option<Runtime>,
    obj: Obj,
    tag: String,
    flags124: bool,
    started: Option<&'static str>,
    errors_seen: usize,
    mode: Option<String>,
    leave: bool,
    alpha: f32,
    originals: HashMap<AssetId<StandardMaterial>, AlphaMode>,
    /// The friend AI object's fields the group 10 hosts keep, and the party host's fields (attack objects,
    /// effects) of the script.
    ai: ai::SummonAi,
    fields: crate::friend_host::FriendFields,
    /// The targets the blast's attack object has hit (FUN_0018c5f8: once per attack object).
    blast_hit: std::collections::HashSet<Entity>,
    /// Ticks the class has run (for the once-a-second log line).
    ticks: u32,
}

/// The status block handle host 1.84 gives for the player (his +0x48c): host 10.45 reads its +0x1b4.
const PLAYER_GAUGES: u32 = 0x7900_0000;

/// What the summon host reads of the game beside the object.
struct SummonEnv {
    /// The player's status block +0x1b4: the drive gauge's timer (`DriveState::timer.left`).
    timer: f32,
    /// A limit task runs (`LimitTask`, `DAT_01d5d6b0`).
    limit: bool,
    /// Stitch's class constants and the index of his move record (FUN_00117ea0).
    stitch: Option<stitch::Consts>,
    move_record: Option<usize>,
}

/// The summon object's script host: the party members' host (`FriendHost`: attack objects, effects,
/// target record) with the summon's own group 10 entries and the calls the summon's script makes of the
/// player; motion requests go straight to the enemy host (the summon's motions are not remapped yet: the
/// object's motions are logged, not played, docs/summons.md).
struct SummonHost<'a, 'b> {
    f: crate::friend_host::FriendHost<'a, 'b>,
    ai: &'b mut ai::SummonAi,
    env: &'b SummonEnv,
}

impl SummonHost<'_, '_> {
    fn record(&mut self, c: &kh2_bdx::Call, v: &kh2_bdx::Value, st: crate::enemy_host::St, why: &'static str) {
        let tag = self.f.inner.tag;
        self.f.inner.trace.record(tag, c, v, st, why);
    }

    /// The calls answered here; `None` for the rest.
    fn own(&mut self, c: &kh2_bdx::Call) -> Option<(kh2_bdx::Value, crate::enemy_host::St, &'static str)> {
        use crate::enemy_host::St;
        use kh2_bdx::host::Arg;
        use kh2_bdx::{Call, Value};
        let me = self.f.inner.obj.handle;
        let word = |a: Option<&Arg>| match a {
            Some(Arg::Float(f)) => f.to_bits(),
            Some(a) => a.int() as u32,
            None => 0,
        };
        let Call::Func { group, index, args } = c else { return None };
        let a = |k: usize| word(args.get(k));
        Some(match (*group, *index) {
            // 1.84 gauges (0x172ee8) of the player: his status block (+0x48c), read by 10.45.
            (1, 84) if a(0) == crate::enemy_obj::PLAYER => (Value::Handle(PLAYER_GAUGES), St::Impl, ""),
            // 1.272 (0x171cb0): (object +0x7f8 & 4) != 0. FUN_00186580 sets the bit (host wrapper 0x171cd0
            // through FUN_001865a8, FUN_00175f10, FUN_001b25d0); nothing sets it for the summon in the app.
            (1, 272) if a(0) == me => (Value::Bool(self.ai.flag_7f8_4), St::Impl, ""),
            // 1.165 offer_command (0x1734a8): Stitch's main thread offers Ohana (519) in slot 2 while the
            // timer is above 0 (docs/summons.md section 4). Summon limits are stage 5.
            (1, 165) => (Value::None, St::Stub, "summon limit offers are stage 5: the offer is logged, the command menu does not show it"),
            // 2.74 attack_callback_98 (0x18e870): attack +0x98 = routine, +0xa0 = argument. Only FUN_0018d430
            // (two attacks clash) runs it; the summon's attack objects meet no other attack.
            (2, 74) => {
                self.ai.callbacks_98.push((a(0), a(1), a(2)));
                (Value::None, St::Impl, "")
            }
            // 10.1 (0x2f6a98 -> FUN_001175a0): AI +0x630 |= 1, the position +0x640 and the wanted yaw from the
            // vector, the depth, the parent matrix (stitch::place).
            (10, 1) => {
                self.ai.flags_630 |= 1;
                match (args.get(1), self.env.stitch.as_ref()) {
                    (Some(Arg::Vec(v)), Some(k)) => {
                        stitch::place(k, &mut self.ai.stitch, *v);
                        (Value::None, St::Approx, "the motion's root rotation (FUN_0017ecf0's w) is taken as 0")
                    }
                    _ => (Value::None, St::Stub, "Stitch's constants are not readable"),
                }
            }
            // 10.3 / 10.4: Stitch's vtable +0x28 (FUN_001191a8) / +0x2c (FUN_00119360), the player's link
            // for a summon limit (stage 5).
            (10, 3) => {
                self.ai.link_calls.0 += 1;
                (Value::None, St::Stub, "FUN_001191a8 (the limit's link of the player to Stitch) is stage 5")
            }
            (10, 4) => {
                self.ai.link_calls.1 += 1;
                (Value::None, St::Stub, "FUN_00119360 (the limit's link undone) is stage 5")
            }
            // 10.12 (0x2f6d00 -> FUN_00117708): the slot index.
            (10, 12) => {
                let who = ai::Who { own: me, player: crate::enemy_obj::PLAYER };
                let i = self.ai.start(who, a(1) as i32, a(2), a(3), a(4), a(5) != 0, a(6), a(7));
                (Value::Int(i as i32), St::Impl, "")
            }
            // 10.13 (0x2f6d68 -> FUN_001195e8): Stitch's event 17 with code 1000 (the limit's shots).
            (10, 13) => (Value::None, St::Stub, "FUN_001195e8 (event 17 code 1000, the limit's attacks) is stage 5"),
            // 10.16 (0x2f6df8 -> FUN_00117840).
            (10, 16) => {
                self.ai.mark(a(1) as i32);
                (Value::None, St::Impl, "")
            }
            // 10.19 (0x2f6e60): object +0x10c bit 0x10000, set by the limit task for its participants
            // (FUN_001b2c10). While a summon is out only summon limits run (FUN_001d1ce0 refuses the others).
            (10, 19) => (Value::Bool(a(0) == me && self.env.limit), St::Approx, "in a limit = a limit task runs (only summon limits can run while a summon is out)"),
            // 10.32 (0x2f7138 -> FUN_00118aa8): a goal (FUN_00116780), motion 0xa1, the record whose handler is
            // `_PTR_FUN_00378880` (FUN_00117ea0, the move) made the current action (FUN_0010ecb0) and +0xb7c |= 4.
            (10, 32) => match self.env.stitch.as_ref() {
                Some(k) => {
                    let st = &mut self.ai.stitch;
                    let pos = st.pos;
                    let mut rng = std::mem::take(&mut st.rng);
                    let rng_c = std::cell::RefCell::new(&mut rng);
                    stitch::goal(k, -1, pos, &mut st.goal, &mut || rng_c.borrow_mut().roll(), &mut |n| rng_c.borrow_mut().rand(n));
                    st.rng = rng;
                    st.motion = Some(stitch::M_MOVE);
                    st.motion_new = true;
                    st.choosing.current = self.env.move_record;
                    st.choosing.continuing = true;
                    self.ai.action = (stitch::MOVE, 0);
                    (Value::None, St::Approx, "FUN_00116780's case is a register the wrapper leaves (taken as the random branch)")
                }
                None => (Value::None, St::Stub, "Stitch's constants are not readable"),
            },
            // 10.39 (0x2f72c0 -> FUN_0010ecb0(object, 0, 0)).
            (10, 39) => {
                self.ai.action = (0, 0);
                self.ai.stitch.choosing.current = None;
                (Value::None, St::Impl, "")
            }
            // 10.41 (0x2f7328 -> FUN_001129f0).
            (10, 41) => {
                self.ai.b7c |= 0x4000_0000;
                (Value::None, St::Impl, "")
            }
            // 10.45 (0x2f73b0): the float at +0x1b4 of the status block.
            (10, 45) if a(0) == PLAYER_GAUGES => (Value::Float(self.env.timer), St::Impl, ""),
            _ => return None,
        })
    }
}

impl kh2_bdx::Host for SummonHost<'_, '_> {
    fn call(&mut self, c: &kh2_bdx::Call) -> kh2_bdx::Value {
        use kh2_bdx::Call;
        // Motion requests and their state as the enemy host has them (no player-type remap for the summon yet).
        if matches!(c, Call::PlayMotion { .. } | Call::PlayMotionChecked { .. } | Call::PlayMotionAt { .. } | Call::MotionState { .. }) {
            return self.f.inner.call(c);
        }
        if let Some((v, st, why)) = self.own(c) {
            self.record(c, &v, st, why);
            return v;
        }
        self.f.call(c)
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        self.f.read_mem(addr, out)
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        self.f.write_mem(addr, data)
    }

    fn target_parts(&mut self, kind: i32, skip_flagged: bool) -> Option<Vec<kh2_bdx::search::TargetPart>> {
        self.f.target_parts(kind, skip_flagged)
    }

    fn target_found(&mut self, record: u32, found: Option<(u32, u8)>, walked: u8) {
        self.f.target_found(record, found, walked)
    }
}

/// Original alpha modes of the companions' materials while the summon fades them.
#[derive(Resource, Default)]
struct Originals(HashMap<AssetId<StandardMaterial>, AlphaMode>);

/// The player's sequence state the systems with world access keep.
#[derive(Resource, Default)]
struct PlayerSide {
    /// The effect 0x13 instance (+0xb68).
    effect: Option<Entity>,
    /// A summon motion plays on Sora (the body slot), which holds the controller.
    motion: Option<Option<usize>>,
}

pub struct SummonPlugin;

impl Plugin for SummonPlugin {
    fn build(&self, app: &mut App) {
        let Some(data) = app.world().get_resource::<Disc>().and_then(SummonData::load) else {
            warn!("summon: tables not readable; summons are off");
            return;
        };
        crate::progress::ensure(app);
        let progress = app.world().get_resource::<Progress>();
        let exp = progress.map_or(drive_rules::SummonExp { level: 1, exp: 0 }, |p| drive_rules::SummonExp { level: p.summon_level(), exp: p.summon_exp() as i32 });
        let env_f = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
        let summon = std::env::var("KH2_SUMMON").ok().and_then(|v| v.trim().parse::<u16>().ok()).map(|v| {
            // 1..=4 names a `sumn` row, anything else a command id.
            let c = if (1..=4).contains(&v) { data.rows.get(v as usize - 1).map_or(v, |r| r.command) } else { v };
            (env_f("KH2_SUMMON_AT").unwrap_or(4.0), c)
        });
        let harness = Harness {
            summon,
            dismiss_at: env_f("KH2_DISMISS_AT"),
            force_party: std::env::var("KH2_SUMMON_PARTY").is_ok_and(|v| v == "1"),
            log: std::env::var("KH2_SUMMONLOG").is_ok_and(|v| !v.is_empty() && v != "0"),
        };
        info!(
            "summon: {} rows ({}), level {} exp {}, wait {} frames (with event {}), gauge change factor {}; executable constants {:?}",
            data.rows.len(),
            data.rows.iter().map(|r| format!("{} charm {} object {}", data.name(r.command), r.item, r.entity1)).collect::<Vec<_>>().join(", "),
            exp.level,
            exp.exp,
            data.tuning.wait,
            data.tuning.wait_event,
            data.tuning.gain,
            data.consts
        );
        let (rng, sector) = data.consts.map_or((1, 0), |c| (c.rng, c.sector));
        app.insert_resource(data)
            .insert_resource(SummonState { exp, rng, sector, ..Default::default() })
            .insert_resource(harness)
            .init_resource::<SummonQueue>()
            .init_resource::<HudSaved>()
            .init_resource::<Acts>()
            .init_resource::<Originals>()
            .init_resource::<PlayerSide>()
            .add_systems(Update, (keys.before(crate::magic::harness_keys), harness_requests, hud_feed.before(crate::hud::hud_gauge::update)))
            .add_systems(FixedUpdate, (tick, act, objects, companions).chain().in_set(Sim::Sounds))
            .add_systems(FixedUpdate, stops.after(Sim::Enemy).before(Sim::Detect).run_if(crate::schedule::sim_running));
    }
}

/// `KH2_KEYS` entries `t:H`: the key the command menu reads as d-pad Left (the deck swap).
fn keys(time: Res<Time>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    let Ok(spec) = std::env::var("KH2_KEYS") else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    for item in spec.split(',') {
        let mut parts = item.split(':');
        let (Some(at), Some("H")) = (parts.next(), parts.next()) else { continue };
        let hold: f32 = parts.next().and_then(|h| h.parse().ok()).unwrap_or(0.05);
        let Ok(at) = at.parse::<f32>() else { continue };
        if t >= at && t - dt < at {
            keys.press(KeyCode::KeyH);
        } else if t >= at + hold && t - dt < at + hold {
            keys.release(KeyCode::KeyH);
        }
    }
}

fn harness_requests(time: Res<Time>, h: Res<Harness>, mut queue: ResMut<SummonQueue>) {
    let (now, dt) = (time.elapsed_secs(), time.delta_secs());
    let crossed = |at: f32| now >= at && now - dt < at;
    if let Some((at, c)) = h.summon
        && crossed(at)
    {
        queue.0.push(SummonRequest::Summon(c));
    }
    if let Some(at) = h.dismiss_at
        && crossed(at)
    {
        queue.0.push(SummonRequest::Dismiss);
    }
}

/// The leave (`FUN_001c67b0`, only while the state is set) and gauge mode 1 (`FUN_001cf298` /
/// `FUN_001cf030` case 0x13c).
fn leave(sm: &mut SummonState, drive: &mut DriveState, acts: &mut Acts, why: &str, name: &str) {
    if sm.state == 0 {
        return;
    }
    sm.state = 3;
    let index = std::mem::take(&mut sm.index);
    drive.gauge_mode = GaugeMode::Normal;
    sm.leaving = true;
    acts.0.push(Act::Leave);
    info!(
        "summon end ({why}): {name} (index {index}), timer {:.0}/{:.0}, gauge mode 1, drive points {} + {}%, summon level {} exp {}",
        drive.timer.left, drive.timer.total, drive.points.whole, drive.points.fraction, sm.exp.level, sm.exp.exp
    );
}

/// The experience (`FUN_001d4ec8`; the minigame flag of `FUN_001b9bd8` is never set in the app),
/// written back to `Progress`.
fn add_exp(sm: &mut SummonState, levels: &kh2_tables::player::FormLevels, progress: Option<&mut Progress>, segments: i32) {
    if segments <= 0 {
        return;
    }
    let before = sm.exp;
    let reached = sm.exp.add(levels, sm.cap, segments, false);
    info!("summon: {segments} segment(s) used: experience {} -> {} (level {})", before.exp, sm.exp.exp, sm.exp.level);
    for l in reached {
        info!("summon: level up to {l} (notice 0xc5bd not shown: FUN_001d4fa0 not ported)");
    }
    if let Some(p) = progress {
        p.set_summon_level(sm.exp.level);
        p.set_summon_exp(sm.exp.exp.max(0) as u32);
    }
}

#[allow(clippy::too_many_arguments)]
fn tick(
    data: Res<SummonData>,
    rules_d: Res<DriveRules>,
    game: Res<GameData>,
    mut progress: Option<ResMut<Progress>>,
    abilities: Option<Res<Abilities>>,
    battle: Res<Battle>,
    h: Res<Harness>,
    limit_task: Option<Res<crate::limit::LimitTask>>,
    (mut sm, mut queue, mut drive, mut acts): (ResMut<SummonState>, ResMut<SummonQueue>, ResMut<DriveState>, ResMut<Acts>),
    mut hits: MessageReader<Hit>,
    sora: Query<(Entity, &Sora)>,
    party: Query<(&PartyMember, Option<&Away>)>,
    objects: Query<&SummonObject>,
) {
    let Ok((root, s)) = sora.single() else {
        hits.clear();
        return;
    };
    let sm = &mut *sm;
    let st = &mut *drive;
    let acts = &mut *acts;
    let tuning = rules_d.tuning;
    sm.limit_task = limit_task.is_some_and(|l| l.0.is_some());
    // The SUMMONS list and the level cap from the charms owned (FUN_001d4d48, FUN_001d4e68).
    let owned_rows: Vec<bool> = data.rows.iter().map(|r| progress.as_ref().is_some_and(|p| p.item_count(r.item) > 0)).collect();
    sm.commands = data.rows.iter().zip(&owned_rows).filter(|(_, o)| **o).map(|(r, _)| r.command).collect();
    sm.cap = drive_rules::summon_level_cap(owned_rows.iter().filter(|o| **o).count() as u32);
    // FUN_001c78f0: party slots 0 and 1 hold members (the app's members are always in the party).
    let slot = |n: u8| party.iter().any(|(m, away)| m.slot == n && away.is_none());
    sm.party_ok = (slot(0) && slot(1)) || h.force_party;
    // The summon slots as the objects hold them (their destructor empties the slot; with both empty the
    // state is 0, FUN_001c5f80 / FUN_001c6038).
    let mut slots = [None; 2];
    sm.present.clear();
    for o in &objects {
        if let Some(sl) = slots.get_mut(o.slot as usize) {
            *sl = Some(o.body);
        }
        sm.present.push(o.neo);
    }
    let had_object = sm.slots.iter().any(Option::is_some);
    sm.slots = slots;
    if had_object && sm.slots.iter().all(Option::is_none) && sm.state != 0 {
        sm.state = 0;
        info!("summon: the last summon object is gone: summon state 0 (FUN_001c5f80)");
    }
    // The leave thread (0x1c6810): once both summon slots are empty, one more frame, no event running
    // (FUN_001c7c88; the player's +0x160 bit 0x4000 is not modelled), the companions come back
    // (FUN_001c79c8); the cleanup clears `0x35feb8` bit 0. The state goes to 0 in the destructor; a
    // leave that never had an object (the summon ended during the arrival) ends it here [inferred].
    if sm.leaving && sm.slots.iter().all(Option::is_none) {
        sm.leaving = false;
        sm.sequence = false;
        if sm.state == 3 {
            sm.state = 0;
        }
        if let Some(c) = data.consts {
            acts.0.push(Act::CompanionsBack { frames: c.ret_fade, alpha: c.ret_alpha });
        }
        info!("summon: leave done (summon state {}, the sequence bit cleared); the companions come back", sm.state);
    }

    for req in std::mem::take(&mut queue.0) {
        match req {
            SummonRequest::Summon(c) => {
                let (Some(row), Some(index)) = (data.cmd(c), data.index_of(c)) else {
                    info!("summon: command {c} is not a summon command");
                    continue;
                };
                // The menu offers only usable entries; the harness gets the same tests, with the
                // command's `Flags` bit 1 (no use while airborne, FUN_001b70b8).
                let airborne = row.flags >> 1 & 1 != 0 && !s.motor.grounded;
                let owned = owned_rows[index as usize - 1];
                if !owned || airborne || !sm.command_ok(st, row) {
                    info!(
                        "summon: {} refused (charm owned {owned}, airborne {airborne}, sequence {}, arrival {}, party {}, form {}, points {} cost {})",
                        data.name(c),
                        sm.sequence,
                        sm.arrival.is_some(),
                        sm.party_ok,
                        st.object_form(),
                        st.points.whole,
                        row.cost
                    );
                    continue;
                }
                let blend = data.consts.map_or(0.0, |k| k.blend);
                acts.0.push(Act::PlayerMotion(motion::SUMMON, blend));
                acts.0.push(Act::EffectStart);
                let before = st.points;
                st.points.spend(row.cost);
                sm.index = index;
                sm.sequence = true;
                sm.state = 2;
                if c == cmd::GENIE {
                    // FUN_001c6a20: the forms 1..4 whose form item is owned, in order.
                    let forms: Vec<u8> = (1..=4).filter(|&f| progress.as_ref().is_some_and(|p| p.owns_form(f))).collect();
                    sm.extra = rules::genie_form(&mut sm.rng, &forms);
                    info!("summon: Genie's form byte {} (FUN_001c6a88, owned forms {forms:?})", sm.extra);
                } else if c == cmd::PETER_PAN {
                    sm.extra = 1;
                }
                sm.player_flags |= 0x104;
                // The arrival thread's choice (FUN_001c65f0; the player's +0x810 is 0 in the app).
                let option = progress.as_ref().map_or(crate::progress::OPTION_DEFAULT, |p| p.option_word()) >> 9 & 3;
                sm.event = rules::arrival_event(option, &mut sm.seen, index, 0);
                sm.arrival = Some(if sm.event { data.tuning.wait_event } else { data.tuning.wait });
                if let Some(k) = data.consts {
                    acts.0.push(Act::CompanionsOut(k.fade_out));
                }
                sm.stop = true;
                let u = &mut sm.uses[index as usize - 1];
                *u = (*u + 1).min(999);
                info!(
                    "summon start: {} (command {c}, index {index}, cost {}, drive points {} -> {}, summon level {} exp {} cap {}); player motion {:#x} blend {blend}, effect {EFFECT:#x}, +0x108 |= 0x104, actor stop id {STOP_ID}; arrival {} (Summon Effects field {option}), wait {} frames",
                    data.name(c),
                    row.cost,
                    before.whole,
                    st.points.whole,
                    sm.exp.level,
                    sm.exp.exp,
                    sm.cap,
                    motion::SUMMON,
                    if sm.event { "as an event" } else { "without the event" },
                    sm.arrival.unwrap_or(0.0)
                );
            }
            SummonRequest::Dismiss => {
                if sm.dismiss_ok() {
                    let name = data.row_name(sm.index);
                    leave(sm, st, acts, "dismiss", &name);
                } else {
                    info!("summon: Dismiss refused (summon state {}, limit task {})", sm.state, sm.limit_task);
                }
            }
            SummonRequest::Gauge(d) => {
                if st.gauge_mode == GaugeMode::Summon {
                    let lost = st.timer.change(&tuning, GaugeMode::Summon, st.form, d);
                    info!("summon: gauge change {d:.1} (FUN_001788d8) -> timer {:.0}", st.timer.left);
                    add_exp(sm, &rules_d.levels, progress.as_deref_mut(), lost);
                }
            }
        }
    }

    // The arrival thread (0x1c6260): the wait runs down by the frame unit; it ends when the wait has
    // run out and both party slots are empty (FUN_0017d470; the screen fades of FUN_0016ff50 are not
    // modelled).
    if let Some(w) = sm.arrival.as_mut() {
        *w -= drive_rules::FRAME_UNIT;
        let companions_gone = party.iter().all(|(_, away)| away.is_some_and(|a| a.gone));
        if *w <= 0.0 && companions_gone {
            sm.arrival = None;
            sm.player_flags |= 1;
            let (timer, segments) = drive_rules::summon_timer(&tuning, sm.exp.level);
            st.timer = timer;
            st.segments = segments;
            st.gauge_mode = GaugeMode::Summon;
            sm.ticks = 0;
            info!("summon: gauge mode 3: timer {:.0}/{:.0}, {segments} segments (summon level {})", timer.left, timer.total, sm.exp.level);
            if sm.event {
                // The effect is killed, the script plays `appear`, state 1 while an event runs. Nothing
                // in the app starts the event (docs/summons.md section 7), so the wait for it ends at
                // once and the HUD layers it hides (FUN_0014eec0) are not touched [inferred].
                acts.0.push(Act::EffectEnd { kill: true });
                acts.0.push(Act::Spawn { index: sm.index, mode: "appear", skip: false });
                sm.state = 1;
                info!("summon: arrival event: summon state 1; no event runs in the app, the wait for it ends at once");
            } else {
                acts.0.push(Act::EffectEnd { kill: false });
                acts.0.push(Act::PlayerMotion(motion::APPEAR, 0.0));
                acts.0.push(Act::Spawn { index: sm.index, mode: "appear_skip", skip: true });
            }
            // The end of the thread: state 2, +0x108 &= ~0x105, the objects' +0x108 bit 0 cleared; the
            // cleanup (FUN_001c65b8) lifts the stop.
            sm.state = 2;
            sm.player_flags &= !0x105;
            sm.stop = false;
            info!("summon: arrival done: summon state 2, player +0x108 &= ~0x105, actor stop id {STOP_ID} lifted");
        }
    }
    st.summon_busy = sm.sequence || sm.arrival.is_some();

    if st.gauge_mode != GaugeMode::Summon {
        hits.clear();
        return;
    }
    let mut segments_lost = 0;
    // Hits Sora lands while the timer is not empty (FUN_00178748 -> FUN_001788d8 mode 3; the victim
    // flags 0x10 / 0x800 are not modelled, as in drive.rs).
    for hit in hits.read().filter(|x| x.attacker == root) {
        let Some(d) = game.atkp(hit.atkp).map(|a| a.drive_drain).filter(|&d| d != 0) else { continue };
        if st.timer.left == 0.0 {
            continue;
        }
        let gain = drive_rules::summon_gauge_change(&data.tuning, d as f32);
        segments_lost += st.timer.change(&tuning, GaugeMode::Summon, st.form, gain);
        info!("summon: landed hit (atkp {}, DriveDrain {d}): timer +{gain:.1} -> {:.0}", hit.atkp, st.timer.left);
    }
    // The drain (FUN_00177788; no event runs in the app).
    let factor = drive_rules::mode_drain_factor(GaugeMode::Summon, 1.0, abilities.as_ref().map_or(1.0, |a| a.block().summon_drain)).unwrap_or(1.0);
    let battle_on = st.force_battle || battle.active();
    let drain = drive_rules::drain_per_tick(&tuning, battle_on, factor);
    segments_lost += st.timer.change(&tuning, GaugeMode::Summon, st.form, -drain);
    add_exp(sm, &rules_d.levels, progress.as_deref_mut(), segments_lost);
    sm.ticks += 1;
    if sm.ticks.is_multiple_of(crate::data::LOGIC_HZ as u32) {
        let (digit, fill, seg) = drive_rules::segment_display(st.timer.left, st.timer.total, st.segments);
        info!(
            "summon: timer {:.0}/{:.0} ({} whole segments, HUD digit {digit} fill {fill}/{seg}), drain {drain:.2}/tick (battle {battle_on}, Summon Boost factor {factor}), level {} exp {}, objects {:?}",
            st.timer.left,
            st.timer.total,
            st.timer.whole_points(&tuning),
            sm.exp.level,
            sm.exp.exp,
            sm.present
        );
    }
    // The timer at 0 (FUN_001ceda8: summon state 2 and Dismiss accepted -> FUN_001cf298). The game
    // tests +0x1b4 in any mode; only mode 3 is tested here: during the arrival the player is busy
    // (+0x108 bit 4, FUN_0019abb0) [inferred].
    if st.timer.left <= 0.0
        && sm.state == 2
        && sm.dismiss_ok()
        && let Some(dismiss) = data.cmd(cmd::DISMISS)
        && !(dismiss.flags >> 1 & 1 != 0 && !s.motor.grounded)
    {
        let name = data.row_name(sm.index);
        leave(sm, st, acts, "timer", &name);
    }
}

/// The resources `act` reads besides its own.
type ActRes<'w> = (ResMut<'w, SummonState>, Res<'w, Battle>, Option<Res<'w, World>>, Option<Res<'w, crate::fx::EffectLib>>);
/// The asset stores a spawned model needs.
type Gfx<'w> = (ResMut<'w, Assets<Mesh>>, ResMut<'w, Assets<StandardMaterial>>, ResMut<'w, Assets<Image>>, ResMut<'w, Assets<SkinnedMeshInverseBindposes>>);

/// Carries out what `tick` asked for: the player's motions and effect, the companions' fades, the
/// object's spawn and its `leave`.
#[allow(clippy::too_many_arguments)]
fn act(
    mut commands: Commands,
    disc: Res<Disc>,
    mut data: ResMut<SummonData>,
    mut acts: ResMut<Acts>,
    mut side: ResMut<PlayerSide>,
    (mut sm, battle, world, lib): ActRes,
    mut sora: Query<&mut Sora>,
    mut anims: Query<&mut Animator>,
    mut running: Query<&mut crate::fx::RunningEffect>,
    mut party: Query<(Entity, &mut PartyMember, Option<&Away>)>,
    mut objects: Query<&mut SummonObject>,
    gfx: Gfx,
) {
    let (mut meshes, mut materials, mut images, mut bindposes) = gfx;
    let Ok(mut s) = sora.single_mut() else {
        acts.0.clear();
        return;
    };
    // The summon motion holds the controller until it ends (the game's motion request with the
    // player's +0x108 bit 4 [inferred]).
    if let Some(slot) = side.motion {
        let done = anims.get(s.body).map_or(true, |b| b.slot != slot || b.finished);
        if done || s.state != State::Reaction {
            side.motion = None;
            if s.state == State::Reaction {
                let next = if s.motor.grounded { State::Ground } else { State::Air };
                s.enter(next, None);
            }
        }
    }
    let pstate = if battle.active() { kh2_anim::PlayerState::BattleWeapon } else { kh2_anim::PlayerState::FieldNoWeapon };
    for a in std::mem::take(&mut acts.0) {
        match a {
            Act::PlayerMotion(id, blend) => {
                let mut cue = crate::sora::WeaponCue::default();
                let slot = anims.get_mut(s.body).ok().and_then(|mut body| {
                    let sl = body.rig.mset.slot(id, pstate)?;
                    body.restart(Some(sl), false, blend);
                    Some(sl)
                });
                cue.restart(slot, false, blend);
                cue.apply(&s.weapons, &mut anims);
                match slot {
                    Some(sl) => {
                        info!("summon: player motion {id:#x} (slot {sl}, blend {blend})");
                        s.enter(State::Reaction, None);
                        side.motion = Some(Some(sl));
                    }
                    None => info!("summon: player motion {id:#x}: not in Sora's motion set"),
                }
            }
            Act::EffectStart => {
                side.effect = lib.as_ref().and_then(|l| crate::fx::start_placed(&mut commands, &l.system, EFFECT, crate::fx::Bind::Owner(s.body), Default::default()));
                let why = match lib.as_ref() {
                    _ if side.effect.is_some() => "started",
                    None => "not started: no effect library",
                    Some(l) if l.system.pax.elements_for(EFFECT).next().is_none() => "not started: no such element in 00effect.bar",
                    Some(_) => "not started: its particle program did not start (kh2_fx::Effect::new)",
                };
                info!("summon: system effect {EFFECT:#x} (00effect.bar) bound to the player: {why}");
            }
            Act::EffectEnd { kill } => {
                if let Some(e) = side.effect.take() {
                    if kill {
                        if let Ok(mut c) = commands.get_entity(e) {
                            c.despawn();
                        }
                    } else if let Ok(mut r) = running.get_mut(e) {
                        r.effect.end();
                    }
                    info!("summon: system effect {EFFECT:#x} {}", if kill { "killed (FUN_001da028)" } else { "told to end (FUN_001d9f48)" });
                }
            }
            Act::CompanionsOut(frames) => {
                let mut n = 0;
                for (e, m, away) in &party {
                    if away.is_none() {
                        commands.entity(e).insert(Away { fade: Fade { from: 1.0, to: 0.0, frames, t: 0.0 }, gone: false, returning: false });
                        n += 1;
                        info!("summon: companions: {} (slot {}) fades out over {frames} frames and is listed (FUN_001c7600)", m.model, m.slot);
                    }
                }
                if n == 0 {
                    info!("summon: companions: none in the party slots");
                }
            }
            Act::CompanionsBack { frames, alpha } => {
                for (e, mut m, away) in &mut party {
                    if away.is_some() {
                        // FUN_001c7ab0 -> FUN_0017d290(lift, min, max, member, no hint): the member is re-created
                        // at a free spot (FUN_001a5ea8: the same search as the summon's spot, `spot_start` /
                        // `spot_find`) between `ret_spot` units from the player, searched from `ret_lift` above
                        // him, facing him (FUN_003003e8 of the difference -> FUN_0010be80) [WP-F1 item 4; the
                        // collision stand-ins are those of the summon's spot below].
                        let mut at = s.motor.pos;
                        if let Some(c) = data.consts {
                            let origin = s.motor.pos + Vec3::Y * c.ret_lift;
                            let mut rng = sm.rng;
                            let mut sector = sm.sector;
                            let mut search = rules::spot_start(&c.search, &mut rng, &mut sector, c.ret_spot[0], c.ret_spot[1], to_game(origin), 0x10000);
                            let w = world.as_deref();
                            let (up, down) = (c.search.probe[0].abs().min(c.search.probe[1].abs()), c.search.probe[0].abs().max(c.search.probe[1].abs()));
                            let floor = |p: [f32; 4]| {
                                let a = rules::app(p);
                                w.and_then(|w| w.ground(a, a.y + up, up + down)).map(|g| to_game(a.with_y(g.y)))
                            };
                            let lift = Vec3::Y * c.search.radius_step;
                            let clear = |from: [f32; 4], to: [f32; 4]| w.is_none_or(|w| w.raycast(rules::app(from) + lift, rules::app(to)).is_none());
                            if let Some(p) = rules::spot_find(&c.search, &mut search, &floor, &clear) {
                                at = rules::app(p);
                            }
                            sm.rng = rng;
                            sm.sector = sector;
                        }
                        let to_player = (s.motor.pos - at).with_y(0.0);
                        let yaw = if to_player.length_squared() > 1e-6 { to_player.x.atan2(to_player.z) } else { m.yaw() };
                        m.place(at, yaw);
                        commands.entity(e).insert(Away { fade: Fade { from: 0.0, to: alpha, frames, t: 0.0 }, gone: false, returning: true });
                        commands.entity(e).insert(Visibility::Inherited);
                        info!(
                            "summon: companions: {} comes back at {:?} ({:.0} units from the player, FUN_0017d290: a free spot {:?} units away searched from {} above him), facing him, fading in over {frames} frames (FUN_001c7ab0)",
                            m.model,
                            at,
                            (at - s.motor.pos).with_y(0.0).length(),
                            data.consts.map(|c| c.ret_spot),
                            data.consts.map_or(0.0, |c| c.ret_lift)
                        );
                    }
                }
            }
            Act::Spawn { index, mode, skip } => {
                let Some(row) = data.rows.get(index as usize - 1).cloned() else { continue };
                let Some(o) = data.objs.iter().find(|r| r.object_id == row.entity1).cloned() else {
                    warn!("summon: object {} of summon {index} has no objentry row", row.entity1);
                    continue;
                };
                let (Some(rig), prog) = data.object(&disc, &o) else {
                    warn!("summon: object: {} model or motion set {} not loadable", o.model_name, o.animation_name);
                    continue;
                };
                let Some(slot) = (0..2u8).find(|&k| sm.slots[k as usize].is_none()) else { continue };
                // FUN_0016b960: the player's position; without the event the free spot (FUN_001a5ea8).
                let mut at = s.motor.pos;
                if skip && let Some(c) = data.consts {
                    let mut rng = sm.rng;
                    let mut sector = sm.sector;
                    let mut search = rules::spot_start(&c.search, &mut rng, &mut sector, c.spot[0], c.spot[1], to_game(at), 0x10000);
                    let w = world.as_deref();
                    // [inferred] stand-ins for the collision tests that were not read: FUN_001a5c30 as a
                    // floor within the probe's two distances of the point (the smaller up, the larger
                    // down), FUN_001a5bf8 as nothing between the origin and the second point, both lifted
                    // by the radius step.
                    let (up, down) = (c.search.probe[0].abs().min(c.search.probe[1].abs()), c.search.probe[0].abs().max(c.search.probe[1].abs()));
                    let floor = |p: [f32; 4]| {
                        let a = rules::app(p);
                        w.and_then(|w| w.ground(a, a.y + up, up + down)).map(|g| to_game(a.with_y(g.y)))
                    };
                    let lift = Vec3::Y * c.search.radius_step;
                    let clear = |from: [f32; 4], to: [f32; 4]| w.is_none_or(|w| w.raycast(rules::app(from) + lift, rules::app(to)).is_none());
                    let found = rules::spot_find(&c.search, &mut search, &floor, &clear);
                    sm.rng = rng;
                    sm.sector = sector;
                    if let Some(p) = found {
                        at = rules::app(p);
                    }
                    info!("summon: free spot {:.0} units from the player (radius {:.0}, start angle {:.2}) at {at:?}", (at - s.motor.pos).with_y(0.0).length(), search.radius, search.angle);
                }
                let flags124 = o.flags & 8 != 0;
                let yaw = s.motor.yaw;
                let root = commands.spawn((Transform::from_translation(at * SCALE).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default())).id();
                let body = spawn_skinned(&mut commands, root, rig.clone(), &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
                let idle = rig.mset.slot(0, kh2_anim::PlayerState::FieldNoWeapon).or_else(|| rig.mset.has(0).then_some(0));
                let handle = SELF_BASE + (HANDLE_BASE + slot as u32) * OBJ_SIZE as u32;
                let mut obj = Obj::new(handle);
                obj.motor.pos = at;
                obj.motor.yaw = yaw;
                let rt = prog.map(|p| {
                    let mut rt = Runtime::new(p);
                    if let Some(t) = &data.host_table {
                        rt.set_host_table(t.clone());
                    }
                    rt.shared.rng = 0x2f6b_1c3d ^ (slot as u32).wrapping_mul(0x85eb_ca6b);
                    rt.core.mode_log = Some(Vec::new());
                    rt
                });
                let script = rt.as_ref().map(|r| r.prog.name.clone());
                sm.slots[slot as usize] = Some(body);
                sm.present.push(o.neo_status);
                info!(
                    "summon: object {} ({}, objentry {}, type {}, NeoStatus {}) in summon slot {slot} at {at:?}, script {}, mode {mode}; body motion slot {idle:?}",
                    o.model_name,
                    data.name(row.command),
                    o.object_id,
                    o.object_type,
                    o.neo_status,
                    script.as_deref().unwrap_or("none")
                );
                if let Ok(mut b) = anims.get_mut(body) {
                    b.play(idle, true, 0.0);
                }
                commands.entity(root).insert(SummonObject {
                    neo: o.neo_status,
                    model: o.model_name.clone(),
                    body,
                    slot,
                    rt,
                    obj,
                    tag: format!("{}#summon{slot}", o.model_name),
                    flags124,
                    started: script.map(|_| mode),
                    errors_seen: 0,
                    mode: None,
                    leave: false,
                    alpha: 1.0,
                    originals: HashMap::new(),
                    ai: ai::SummonAi::default(),
                    fields: crate::friend_host::FriendFields::default(),
                    blast_hit: Default::default(),
                    ticks: 0,
                });
            }
            Act::Leave => {
                // 0x1c6810: the first summon object (slot 0) switches to `leave`.
                match objects.iter_mut().find(|o| o.slot == 0) {
                    Some(mut o) => {
                        o.leave = true;
                        info!("summon: leave: {} switches to mode leave", o.model);
                    }
                    None => info!("summon: leave: no summon object"),
                }
            }
        }
    }
}

/// What Stitch's class reaches beside his object: the game camera (his plane's parent [inferred]), the
/// enemies and their bodies, the hits his blast sends, the MP charge his refill ends, the battle flag.
type StitchWorld<'w, 's> = (
    Query<'w, 's, (&'static Camera, &'static GlobalTransform), crate::menu::preview::GameCamera>,
    Query<'w, 's, (Entity, &'static Enemy, &'static Fighter)>,
    Query<'w, 's, &'static GlobalTransform>,
    MessageWriter<'w, Hit>,
    Option<ResMut<'w, crate::magic::MagicState>>,
    Res<'w, Battle>,
);

/// The summon objects' scripts (run every tick through the enemy host, as the party's in
/// `friend_ai.rs`), their fades and their end.
#[allow(clippy::too_many_arguments)]
fn objects(
    mut commands: Commands,
    h: Res<Harness>,
    world: Option<Res<World>>,
    trace: Option<ResMut<AiTrace>>,
    mut sora: Query<&mut Sora>,
    mut q: Query<(Entity, &mut Transform, &mut SummonObject)>,
    mut anims: Query<&mut Animator>,
    children: Query<&Children>,
    handles: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    (game, drive, limit_task, data): (Res<GameData>, Res<DriveState>, Option<Res<crate::limit::LimitTask>>, Res<SummonData>),
    (cam, enemies, gts, mut hits, mut magic, battle): StitchWorld,
) {
    let (Some(mut trace), Ok(mut s)) = (trace, sora.single_mut()) else { return };
    let paired = HashMap::new();
    let move_record = data.stitch.as_ref().and_then(|(_, r)| r.iter().position(|r| stitch::addr(r) == stitch::MOVE));
    let senv = SummonEnv { timer: drive.timer.left, limit: limit_task.is_some_and(|l| l.0.is_some()), stitch: data.stitch.as_ref().map(|d| d.0), move_record };
    let camera = cam.iter().next();
    // FUN_00118068 / FUN_00118110's targets: valid enemies (FUN_00112ac0: alive, hittable) not out of view
    // (FUN_0019ada8 -> FUN_001a5db0, the camera's frustum [inferred: the point's projection inside the screen]),
    // each with the centre of its nearest hurt volume (FUN_00112d20) in app units.
    let targets: Vec<(Entity, Vec3)> = enemies
        .iter()
        .filter(|(_, e, f)| e.alive() && f.hittable)
        .filter_map(|(te, _, f)| {
            let body = gts.get(f.hurt_body).ok()?;
            let a = anims.get(f.hurt_body).ok()?;
            let (cc, cg) = camera?;
            let vols = crate::combat::hurt_volumes(&a, body);
            let p = vols.iter().map(|v| (v.a + v.b) * 0.5).min_by(|x, y| x.distance_squared(cg.translation()).total_cmp(&y.distance_squared(cg.translation()))).unwrap_or(body.translation());
            let ndc = cc.world_to_ndc(cg, p)?;
            (ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z)).then_some((te, p))
        })
        .collect();
    for (ent, mut tf, mut so) in &mut q {
        let so = &mut *so;
        let Ok(anim) = anims.get(so.body) else { continue };
        let rig = anim.rig.clone();
        so.obj.slot = anim.slot;
        so.obj.frame = anim.frame;
        let motion_done = anim.finished;
        let env = Env {
            player: s.motor.pos,
            player_yaw: s.motor.yaw,
            world: world.as_deref(),
            rig: &rig,
            flags124: so.flags124,
            enmp: 0,
            player_serial: 0,
            player_motion_done: true,
            paired: &paired,
            player_model: 0,
            mission_counters: None,
            event_active: false,
        };
        let (neo, body) = (so.neo, so.body);
        let SummonObject { rt, obj, tag, started, leave, mode, errors_seen, ai, fields, blast_hit, ticks, .. } = so;
        let Some(rt) = rt.as_mut() else {
            // No script: nothing ends the object but the leave [inferred].
            if std::mem::take(leave) {
                obj.hidden = true;
            }
            if obj.hidden {
                info!("summon: {tag} has no script: removed at the leave");
                commands.entity(ent).despawn();
            }
            continue;
        };
        let handle = obj.handle;
        let inner = BevyHost { obj, env: &env, trace: &mut trace, tag: tag.as_str() };
        let f = crate::friend_host::FriendHost { inner, fields, game: &game, free: None, targets: &[] };
        let mut host = SummonHost { f, ai, env: &senv };
        if let Some(m) = started.take() {
            rt.spawn(&mut host, handle);
            rt.set_mode_named(&mut host, m);
        }
        if std::mem::take(leave) {
            rt.set_mode_named(&mut host, "leave");
        }
        rt.update(&mut host);
        let now = rt.current_mode();
        if now != *mode {
            info!("summon: {tag} script mode {:?} -> {:?}", mode.as_deref().unwrap_or("-"), now.as_deref().unwrap_or("-"));
            *mode = now;
        }
        if let Some(req) = obj.motion_req.take()
            && h.log
        {
            info!("summon: {tag} script asked for motion {} (not applied: the class's own requests drive the body)", req.slot);
        }
        // Stitch's class (FUN_001155e0 and its records, WP-SUM4b) while he is not leaving.
        let mut placed = None;
        if neo == stitch::NEO
            && let Some((k, recs)) = data.stitch.as_ref()
            && rt.current_mode().as_deref() != Some("leave")
        {
            *ticks += 1;
            let st = &mut ai.stitch;
            let now = stitch::Now { battle: battle.active(), limit: senv.limit, hp: (s.hp, s.stats.max_hp), mp: (s.mp, game.sora_max_mp), in_view: targets.len(), motion_done };
            let out = stitch::tick(k, recs, st, &now);
            if let Some((i, forced)) = out.started {
                info!(
                    "summon: {tag} record {i} (handler {:#x}{}) starts: position ({:.0}, {:.0}) goal ({:.0}, {:.0}), player HP {}/{} MP {}/{}, enemies in view {}",
                    stitch::addr(&recs[i]),
                    if forced { ", FUN_001155e0's forced pick" } else { "" },
                    st.pos[0],
                    st.pos[1],
                    st.goal[0],
                    st.goal[1],
                    now.hp.0,
                    now.hp.1,
                    now.mp.0,
                    now.mp.1,
                    now.in_view
                );
            }
            for (kind, own, a3) in &out.slots {
                // FUN_00116b48 starts each slot's effect (FUN_0016a530 with the kind as the effect number of
                // Stitch's own file); the slot effects are not drawn [GAP: the slot list's effect start and its
                // placement types 1..3 are not ported].
                info!("summon: {tag} AI slot kind {kind:#x} on {} (argument {a3:#x}): effect not drawn", if *own { "Stitch" } else { "nothing" });
            }
            if out.refill {
                let ended = crate::magic::refill_mp(magic.as_deref_mut().map(|m| &mut m.charge), &mut s.mp, game.sora_max_mp);
                info!("summon: {tag} MP refill (FUN_00118730 -> FUN_001a0240): player MP {} (charge ended {ended}); effect {} on the player not drawn", s.mp, out.player_effect.unwrap_or(-1));
            }
            if st.attack.is_none() && !blast_hit.is_empty() {
                info!("summon: {tag} blast attack object released ({} hit)", blast_hit.len());
                blast_hit.clear();
            }
            if out.blast
                && let Some((_, cg)) = camera
            {
                // FUN_00118110: the attack object (atkp 0x4ba) hits each target once (FUN_0018c5f8), aimed from the
                // camera's position (FUN_001071c8).
                for &(te, p) in &targets {
                    if !blast_hit.insert(te) {
                        continue;
                    }
                    let dir = (p - cg.translation()).with_y(0.0).normalize_or_zero();
                    hits.write(Hit { attacker: ent, target: te, atkp: stitch::BLAST_ATKP, dir, point: p, row: None });
                    info!("summon: {tag} blast (FUN_00118110, atkp {:#x}) hits {te:?} at {:.0?}", stitch::BLAST_ATKP, p / SCALE);
                }
            }
            // The motion the class asked for (FUN_0017e220 / FUN_0017e2d8), through FUN_0017da30's slot order.
            if std::mem::take(&mut st.motion_new)
                && let (Some(m), Some(order)) = (st.motion, data.motion_orders)
                && let Ok(mut a) = anims.get_mut(body)
            {
                let rig = a.rig.clone();
                let state = crate::party_battle::motion_state(battle.active(), false);
                let slot = crate::party_battle::remap(m as i32, order[state], |x| rig.mset.has(x));
                let looping = slot.and_then(|x| rig.mset.get(x)).and_then(|x| x.motion.as_ref()).is_some_and(|mo| mo.frame_return < mo.frame_end);
                if h.log {
                    info!("summon: {tag} motion request {m:#x} -> slot {slot:?} (looping {looping})");
                }
                if slot.is_some() {
                    a.restart(slot, looping, 0.0);
                }
            }
            if st.attached
                && let Some((_, cg)) = camera
            {
                // FUN_00117320: the matrix of the position in the parent's space; the camera's [inferred], game
                // camera axes (x right, y down, z forward) to the app's camera axes (x right, y up, -z forward).
                let p = stitch::camera_point(k, st.pos);
                let world_at = cg.transform_point(Vec3::new(p[0], -p[1], -p[2]) * SCALE);
                obj.motor.pos = world_at / SCALE;
                placed = Some(cg.rotation() * Quat::from_rotation_y(-st.pos[3]));
            }
            if h.log && *ticks % 60 == 1 {
                info!(
                    "summon: {tag} plane ({:.0}, {:.0}, {:.0}) yaw {:.2} goal ({:.0}, {:.0}) speed {:.1} record {:?} phase {} -> world {:.0?} ({:.0} units from the player)",
                    st.pos[0],
                    st.pos[1],
                    st.pos[2],
                    st.pos[3],
                    st.goal[0],
                    st.goal[1],
                    st.speed,
                    st.choosing.current,
                    st.phase,
                    obj.motor.pos,
                    (obj.motor.pos - s.motor.pos).length()
                );
            }
        }
        // Nothing else the script asks for is carried out before stage 4 (docs/summons.md).
        obj.move_vel = Vec3::ZERO;
        obj.effects.clear();
        obj.outbox.clear();
        obj.player_cmds.clear();
        obj.drops.clear();
        for entry in rt.core.mode_log.as_mut().map(std::mem::take).unwrap_or_default() {
            if h.log {
                info!("summon: {tag} script mode {entry}");
            }
        }
        for (pc, err) in &rt.core.errors[*errors_seen..] {
            warn!("summon: {tag}: script error {err:?} at {pc:#06x}");
        }
        *errors_seen = rt.core.errors.len();
        obj.advance_fade();
        tf.translation = obj.motor.pos * SCALE;
        tf.rotation = placed.unwrap_or_else(|| Quat::from_rotation_y(obj.motor.yaw));
        let alpha = if obj.hidden { 0.0 } else { obj.alpha.clamp(0.0, 1.0) };
        if (alpha - so.alpha).abs() > 1e-4 {
            so.alpha = alpha;
            set_alpha(so.body, alpha, &children, &handles, &mut materials, &mut so.originals);
        }
        // The script ends the object (hide): the destructor empties its summon slot (FUN_001c5f80).
        if so.obj.hidden {
            info!("summon: {} hidden by its script: removed (summon slot {} empty)", so.tag, so.slot);
            commands.entity(ent).despawn();
        }
    }
}

/// The companions' fades (`FUN_001abf40` on +0x82c) and, once faded out, their hiding.
fn companions(
    mut commands: Commands,
    mut q: Query<(Entity, &PartyMember, &mut Away)>,
    children: Query<&Children>,
    handles: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut originals: ResMut<Originals>,
) {
    for (e, m, mut away) in &mut q {
        if away.gone {
            continue;
        }
        let (alpha, done) = away.fade.step();
        set_alpha(m.body, alpha, &children, &handles, &mut materials, &mut originals.0);
        if !done {
            continue;
        }
        if away.returning {
            commands.entity(e).remove::<Away>();
            info!("summon: companions: {} back", m.model);
        } else {
            away.gone = true;
            commands.entity(e).insert(Visibility::Hidden);
            info!("summon: companions: {} gone (party slot empty for FUN_0017d470)", m.model);
        }
    }
}

/// Sets a body's material alpha (blended below 1, the authored mode at 1).
fn set_alpha(
    body: Entity,
    alpha: f32,
    children: &Query<&Children>,
    handles: &Query<&MeshMaterial3d<StandardMaterial>>,
    materials: &mut Assets<StandardMaterial>,
    originals: &mut HashMap<AssetId<StandardMaterial>, AlphaMode>,
) {
    let Ok(kids) = children.get(body) else { return };
    for k in kids.iter() {
        let Ok(h) = handles.get(k) else { continue };
        let Some(mut mat) = materials.get_mut(&h.0) else { continue };
        let original = *originals.entry(h.0.id()).or_insert_with(|| mat.alpha_mode);
        mat.base_color = mat.base_color.with_alpha(alpha);
        mat.alpha_mode = if alpha < 1.0 { AlphaMode::Blend } else { original };
    }
}

/// Actor stop id 2 during the arrival (`FUN_0016bef0(2, STOP_MASK)`; the `stop` row of id 2 has flags
/// 7: frozen, no reaction, untouchable) on the enemies, whose object type is in the mask, and the
/// player's +0x108 bit 0x100 (no hit reaches him). The enemies' motion stops (rate 0) and no hit
/// reaches them; their scripts still run (enemy_ai.rs) [app gap]. Runs after the AI and the player's
/// controller, which set `hittable` from their own state.
#[allow(clippy::type_complexity)]
fn stops(
    data: Option<Res<SummonData>>,
    sm: Option<Res<SummonState>>,
    mut held: Local<Vec<Entity>>,
    mut enemies: Query<(&Enemy, &mut Fighter), Without<Sora>>,
    mut player: Query<&mut Fighter, With<Sora>>,
    mut anims: Query<&mut Animator>,
) {
    let (Some(data), Some(sm)) = (data, sm) else { return };
    if sm.player_flags & 0x100 != 0
        && let Ok(mut f) = player.single_mut()
    {
        f.hittable = false;
    }
    if !sm.stop {
        for b in held.drain(..) {
            if let Ok(mut a) = anims.get_mut(b) {
                a.speed = 1.0;
            }
        }
        return;
    }
    let started = held.is_empty();
    for (e, mut f) in &mut enemies {
        let class = data.objs.iter().find(|r| r.model_name == e.kind.model).map_or(2, |r| r.object_type as u32);
        if STOP_MASK >> (class & 31) & 1 == 0 {
            continue;
        }
        f.hittable = false;
        if let Ok(mut a) = anims.get_mut(e.body)
            && !held.contains(&e.body)
        {
            a.speed = 0.0;
            held.push(e.body);
        }
    }
    if started && !held.is_empty() {
        info!("summon: actor stop id {STOP_ID} on {} enemies (motion frozen, untouchable)", held.len());
    }
}

/// Mode 3 on the HUD's drive gauge (`FUN_00155bf0`: the digit of the segment and the fill inside it;
/// the art set of mode 3, `DriveMode::FormB`). Outside mode 3 the gauge is left as it was.
fn hud_feed(drive: Option<Res<DriveState>>, mut gauge: ResMut<crate::hud::hud_gauge::DriveGauge>, mut saved: ResMut<HudSaved>) {
    use crate::hud::hud_gauge::{DriveGauge, DriveMode};
    let Some(st) = drive else { return };
    if st.gauge_mode == GaugeMode::Summon {
        if saved.0.is_none() {
            saved.0 = Some(gauge.clone());
        }
        let (digit, fill, seg) = drive_rules::segment_display(st.timer.left, st.timer.total, st.segments);
        *gauge = DriveGauge {
            enabled: true,
            mode: DriveMode::FormB,
            points: digit.min(9) as u8,
            max: st.points.max,
            fraction: 0,
            time_fill: Some(if seg > 0 { fill as f32 / seg as f32 } else { 0.0 }),
        };
    } else if let Some(g) = saved.0.take() {
        *gauge = g;
    }
}

#[cfg(test)]
mod tests {
    use super::rules::*;
    use super::*;

    fn archive() -> Option<kh2_archive::Archive> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        kh2_archive::Archive::open(&iso).ok()
    }

    fn elf(a: &mut kh2_archive::Archive) -> Vec<u8> {
        ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable")
    }

    /// The experience a summon earns is written back to `Progress` (byte 6 and `0x340e4c`) with the
    /// level-up of `FUN_001d4ec8` (the disc's `fmlv` form-0 rows).
    #[test]
    fn summon_exp_is_written_to_progress() {
        let Some(mut a) = archive() else { return };
        let fmlv = kh2_tables::load::<t::Fmlv>(&mut a, &t::FMLV).expect("fmlv").rows;
        let levels = kh2_tables::player::FormLevels::from_rows(&fmlv);
        let mut p = Progress::from_block(&[], Arc::new(crate::progress::ItemIndex::default()));
        let mut sm = SummonState { exp: drive_rules::SummonExp { level: 1, exp: 0 }, cap: 7, ..Default::default() };
        for _ in 0..12 {
            add_exp(&mut sm, &levels, Some(&mut p), 1);
            assert_eq!((p.summon_level(), p.summon_exp()), (sm.exp.level, sm.exp.exp as u32));
        }
        assert!(sm.exp.level > 1, "twelve segments level a summon up from 1: {:?}", sm.exp);
    }

    #[test]
    fn summon_arrival_choice_follows_the_option_and_the_seen_bits() {        let mut seen = 0;
        // Auto (2): the first time only, per summon.
        assert!(arrival_event(2, &mut seen, 3, 0));
        assert_eq!(seen, 1 << 3);
        assert!(!arrival_event(2, &mut seen, 3, 0));
        assert!(arrival_event(2, &mut seen, 1, 0));
        // Full (1) every time; 0 and 3 never; +0x810 set never.
        assert!(arrival_event(1, &mut seen, 3, 0));
        assert!(!arrival_event(0, &mut 0, 2, 0) && !arrival_event(3, &mut 0, 2, 0));
        assert!(!arrival_event(1, &mut 0, 2, 1));
    }

    #[test]
    fn summon_genie_form_picks_an_owned_form() {
        let mut rng = 0x1234_5678;
        for owned in [vec![1u8], vec![1, 2], vec![2, 3, 4], vec![1, 2, 3, 4]] {
            for _ in 0..200 {
                let f = genie_form(&mut rng, &owned);
                assert!(owned.contains(&f), "{f} from {owned:?}");
            }
        }
        assert_eq!(genie_form(&mut rng, &[]), 0);
    }

    /// The executable's constants read, and the free-spot search on a flat floor lands between the
    /// two radii (it takes the first direction: every point has a floor).
    #[test]
    fn summon_consts_and_spot_on_a_flat_floor() {
        let Some(mut a) = archive() else { return };
        let c = SummonConsts::from_exe(&elf(&mut a)).expect("constants");
        eprintln!("{c:?}");
        assert!(c.fade_out > 0.0 && c.spot[0] > 0.0 && c.spot[0] < c.spot[1] && c.ret_spot[0] < c.ret_spot[1]);
        let (mut rng, mut sector) = (c.rng, c.sector);
        for _ in 0..50 {
            let mut s = spot_start(&c.search, &mut rng, &mut sector, c.spot[0], c.spot[1], [10.0, -5.0, 20.0, 1.0], 0x10000);
            let p = spot_find(&c.search, &mut s, &|p| Some(p), &|_, _| true).expect("spot");
            let d = ((p[0] - 10.0).powi(2) + (p[2] - 20.0).powi(2)).sqrt();
            assert!(d <= c.spot[1] + 1e-3, "{d}");
        }
    }

    /// The oracle (PLAYBOOK R2): the game's FUN_001a6068 (a leaf) against `spot_start` for many LCG
    /// states and every sector, and FUN_001c6a88 (with FUN_001c6a20 reading the form items from a fake
    /// inventory, as in `drive::tests::summon_gauge_matches_the_game`) against `genie_form`.
    #[test]
    fn summon_spot_start_and_genie_form_match_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const OUT: u32 = 0x0180_0000;
        const ORIGIN: u32 = 0x0181_0000;
        const ITEM_AT: u32 = 0x0189_0000;
        let Some(mut a) = archive() else { return };
        let exe = elf(&mut a);
        let c = SummonConsts::from_exe(&exe).expect("constants");
        let item_blob = kh2_tables::subfile(&mut a, &t::ITEM).expect("item");
        let mut ee = Ee::new();
        ee.load_elf(&exe).expect("elf");
        let origin = [12.5f32, -40.0, 300.25, 1.0];
        for (i, v) in origin.iter().enumerate() {
            ee.wf32(ORIGIN + 4 * i as u32, *v);
        }
        let mut checked = 0;
        let mut seed = 0x9e37_79b9u32;
        for k in 0..64u32 {
            seed = seed.wrapping_mul(0x0019_660d).wrapping_add(0x3c6e_f35f);
            let sector = (k % 8) as i32;
            ee.w32(0x0037_16a0, seed);
            ee.w32(0x0035_e9b0, sector as u32);
            ee.call(0x001a_6068, &[OUT as u64, ORIGIN as u64, 0x10000], &[c.spot[0], c.spot[1]], STACK).expect("FUN_001a6068");
            let (mut rng, mut sec) = (seed, sector);
            let s = spot_start(&c.search, &mut rng, &mut sec, c.spot[0], c.spot[1], origin, 0x10000);
            let game = (ee.f32_at(OUT + 0x10), ee.f32_at(OUT + 0x14), ee.f32_at(OUT + 0x18), ee.r32(OUT + 0x1c), ee.r32(OUT + 0x20), ee.r32(0x0037_16a0), ee.r32(0x0035_e9b0) as i32);
            assert_eq!(game, (s.min, s.radius, s.angle, 0, 0x10000, rng, sec), "seed {seed:#x} sector {sector}");
            checked += 1;
        }
        // FUN_001c6a88 with the form items in a fake inventory (FUN_001a1cc8 reads 0x340ce8 or the bits
        // at 0x340e28 through the item record of FUN_001a66e8).
        ee.write(ITEM_AT, &item_blob);
        ee.w32(0x01c9_5588, ITEM_AT);
        let set_item = |ee: &mut Ee, item: u16, on: bool| {
            let rec = ee.call(0x001a_66e8, &[item as u64], &[], STACK).expect("FUN_001a66e8") as u32;
            assert_ne!(rec, 0, "item {item} has a record");
            let slot = ee.r16(rec + 0x12) as u32;
            if ee.r8(rec + 3) & 1 == 0 {
                ee.write(0x0034_0ce8 + slot, &[on as u8]);
            } else {
                let w = 0x0034_0e28 + (slot >> 5) * 4;
                let v = (ee.r32(w) & !(1 << (slot & 31))) | (on as u32) << (slot & 31);
                ee.w32(w, v);
            }
        };
        let form_items: Vec<u16> = (1..=4u64).map(|f| ee.call(0x001a_1e38, &[f], &[], STACK).expect("FUN_001a1e38") as u16).collect();
        for mask in 1..16u32 {
            let owned: Vec<u8> = (1..=4u8).filter(|f| mask >> (f - 1) & 1 != 0).collect();
            for (k, &it) in form_items.iter().enumerate() {
                set_item(&mut ee, it, mask >> k & 1 != 0);
            }
            for _ in 0..24 {
                seed = seed.wrapping_mul(0x0019_660d).wrapping_add(0x3c6e_f35f);
                ee.w32(0x0037_16a0, seed);
                let got = ee.call(0x001c_6a88, &[], &[], STACK).expect("FUN_001c6a88") as u8;
                let mut rng = seed;
                let want = genie_form(&mut rng, &owned);
                assert_eq!((got, ee.r32(0x0037_16a0)), (want, rng), "forms {owned:?} seed {seed:#x}");
                checked += 1;
            }
        }
        eprintln!("oracle: FUN_001a6068 and FUN_001c6a88 equal in {checked} cases (form items {form_items:?})");
    }

    /// FUN_00300278 is sine and FUN_003000d8 cosine (the free-spot direction is (sin, 0, cos)).
    #[test]
    fn summon_spot_trig_is_sin_and_cos() {
        use kh2_ee::Ee;
        let Some(mut a) = archive() else { return };
        let mut ee = Ee::new();
        ee.load_elf(&elf(&mut a)).expect("elf");
        for x in [-2.5f32, -0.7, 0.0, 0.3, 1.2, 3.0] {
            ee.call(0x0030_0278, &[], &[x], 0x01ff_0000).expect("FUN_00300278");
            let s = ee.fpr[0];
            ee.call(0x0030_00d8, &[], &[x], 0x01ff_0000).expect("FUN_003000d8");
            let c = ee.fpr[0];
            assert!((s - x.sin()).abs() < 1e-3 && (c - x.cos()).abs() < 1e-3, "{x}: {s} {c}");
        }
    }

    /// The game's FUN_001c65f0 against `arrival_event` (option, seen bit, +0x810, every index).
    #[test]
    fn summon_arrival_choice_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const OBJECT: u32 = 0x0182_0000;
        let Some(mut a) = archive() else { return };
        let mut ee = Ee::new();
        ee.load_elf(&elf(&mut a)).expect("elf");
        // The config block FUN_001057a8 returns; FUN_0023eed8 reads bits 9..10 of its word +0x31b0.
        let config = 0x0033_e860u32;
        let base = ee.r32(config + 0x31b0) & !(3 << 9);
        let mut n = 0;
        for option in 0..4u32 {
            for seen in [false, true] {
                for p810 in [0u32, 1] {
                    for index in 1..=4u8 {
                        ee.w32(config + 0x31b0, base | option << 9);
                        let w = if seen { 1 << index } else { 0 };
                        ee.w32(0x01d5_e370, w);
                        ee.write(OBJECT, &[0u8; 0x900]);
                        ee.w32(OBJECT + 0x810, p810);
                        let got = ee.call(0x001c_65f0, &[index as u64, OBJECT as u64], &[], STACK).expect("FUN_001c65f0") != 0;
                        let mut port = w;
                        assert_eq!((got, ee.r32(0x01d5_e370)), (arrival_event(option, &mut port, index, p810), port), "option {option} seen {seen} +0x810 {p810} index {index}");
                        n += 1;
                    }
                }
            }
        }
        eprintln!("oracle: FUN_001c65f0 equal in {n} cases");
    }

    /// Test numbers for Stitch's class (not the executable's: those are read in the disc tests).
    fn stitch_test_consts() -> stitch::Consts {
        stitch::Consts {
            depth: 100.0,
            place_depth: 100.0,
            x_a: -50.0,
            x_b: 50.0,
            y_a: -30.0,
            y_b: 30.0,
            case0: [1.0, 2.0],
            case6: [-40.0, 20.0],
            case7: [40.0, 20.0],
            left: [[-10.0, 1.0], [-20.0, 2.0], [-30.0, 3.0], [-40.0, 4.0], [-50.0, 5.0], [-60.0, 6.0]],
            right: [[10.0, 1.0], [20.0, 2.0], [30.0, 3.0], [40.0, 4.0], [50.0, 5.0], [60.0, 6.0]],
            split: 0.5,
            shift: 10.0,
            shift_x: 160.0,
            shift_y: 120.0,
            wrap_x: [-80.0, 160.0, 80.0],
            wrap_y: [-60.0, 120.0, 60.0],
            max_speed: 3.0,
            turn: 0.2,
            stand_add: 6.0,
            stand_angle: 0.8,
            stand_short: 0.5,
            near_blast: 10.0,
            near_refill: 10.0,
            scale: 0.5,
            wait: (9.0, 18.0),
            period: 60.0,
            hp_frac: 0.25,
            mp_frac: 0.75,
            count: 5.0,
            chance: 0.5,
            frame: 1.0,
            unit: 1.0,
        }
    }

    fn stitch_test_records() -> Vec<crate::party_battle::Record> {
        use crate::party_battle::{Handler, Record};
        [(0x201, stitch::STAND), (0x201, stitch::MOVE), (0x212, stitch::BLAST), (0x220, stitch::REFILL), (0x212, stitch::LINK)]
            .iter()
            .map(|&(flags, a)| Record { flags, handler: Handler::Other(a), id: 0, entry: None, ability: 0, item: 0, setting: -1, mode: String::new() })
            .collect()
    }

    /// The follow decision of Stitch's class (FUN_001155e0 with FUN_0010fcd0 and the records' phases): out of
    /// battle he only stands and moves between goals in his plane, each move ends at its goal, a goal beyond an
    /// edge comes back across it; with the player's HP low and an enemy in view the blast is the only choice
    /// (FUN_0010ed20 rules stand and move out), it walks to its spot and fires until its motion ends; with MP
    /// the lower fraction in battle the forced pick is the refill.
    #[test]
    fn stitch_follow_decision_and_blast() {
        let c = stitch_test_consts();
        let recs = stitch_test_records();
        let mut ai = stitch::StitchAi { rng: stitch::Rng(7), ..Default::default() };
        let calm = stitch::Now { battle: false, limit: false, hp: (100, 100), mp: (100, 100), in_view: 0, motion_done: false };
        let (mut moves, mut stands, mut moved) = (0, 0, 0.0f32);
        for _ in 0..2000 {
            let before = ai.pos;
            let out = stitch::tick(&c, &recs, &mut ai, &calm);
            match out.started.map(|(i, _)| stitch::addr(&recs[i])) {
                Some(stitch::MOVE) => moves += 1,
                Some(stitch::STAND) => stands += 1,
                Some(a) => panic!("out of battle with full HP only stand and move start, not {a:#x}"),
                None => {}
            }
            assert!(!out.blast && !out.refill);
            moved += ((ai.pos[0] - before[0]).powi(2) + (ai.pos[1] - before[1]).powi(2)).sqrt();
            assert!(ai.speed <= c.max_speed + 1e-4);
            assert!(ai.pos[0].abs() <= 80.0 + 160.0 && ai.pos[1].abs() <= 60.0 + 120.0, "wrapped back: {:?}", ai.pos);
            assert_eq!(ai.pos[2], 100.0, "the depth stays");
        }
        assert!(moves > 3 && stands > 3 && moved > 100.0, "moves {moves} stands {stands} distance {moved}");
        // A move ends at its goal: arrived ends the record (its phase 1 fails once arrived).
        let mut a2 = stitch::StitchAi { pos: [0.0, 0.0, 100.0, 0.0], goal: [9.0, 0.0, 100.0, 0.0], ..Default::default() };
        a2.choosing.current = Some(1);
        assert!(stitch::can(&c, &recs, &a2, &calm, 1, true));
        a2.pos = [8.0, 0.0, 100.0, 0.0];
        assert!(!stitch::can(&c, &recs, &a2, &calm, 1, true), "nearer than the cap and facing the goal's yaw: arrived");
        // The blast: HP below a quarter and an enemy in view.
        let hurt = stitch::Now { hp: (20, 100), in_view: 2, battle: true, ..calm };
        assert!(!stitch::can(&c, &recs, &ai, &hurt, 0, false) && !stitch::can(&c, &recs, &ai, &hurt, 1, false), "stand and move give way to a feasible blast");
        assert!(stitch::can(&c, &recs, &ai, &hurt, 2, false));
        assert!(!stitch::can(&c, &recs, &ai, &stitch::Now { in_view: 0, ..hurt }, 2, false), "no enemy in view: no blast");
        let mut ai = stitch::StitchAi { rng: stitch::Rng(3), ..Default::default() };
        let (mut fired, mut phase3, mut done) = (0, false, false);
        for t in 0..400 {
            let now = stitch::Now { motion_done: phase3 && t % 50 == 0, ..hurt };
            let out = stitch::tick(&c, &recs, &mut ai, &now);
            if out.blast {
                fired += 1;
                assert_eq!(ai.motion, Some(stitch::M_BLAST));
                assert_eq!((ai.pos[0], ai.pos[1]), (c.case6[0], c.case6[1]), "it fires from its spot");
            }
            phase3 |= ai.phase == 3;
            if ai.phase == 4 {
                done = true;
                break;
            }
        }
        assert!(fired > 0 && done, "fired {fired} done {done}");
        // The forced pick: in battle after more than `count` choices, the refill when MP is the lower fraction.
        let low_mp = stitch::Now { battle: true, hp: (90, 100), mp: (10, 100), ..calm };
        let mut ai = stitch::StitchAi { rng: stitch::Rng(11), count: 6, ..Default::default() };
        let mut refilled = false;
        for _ in 0..200 {
            let out = stitch::tick(&c, &recs, &mut ai, &low_mp);
            if let Some((i, true)) = out.started {
                assert_eq!(stitch::addr(&recs[i]), stitch::REFILL);
            }
            refilled |= out.refill;
        }
        assert!(refilled, "the refill ran");
    }

    /// The executable's numbers and records of Stitch's class.
    #[test]
    fn stitch_class_from_the_executable() {
        let Some(mut a) = archive() else { return };
        let exe = elf(&mut a);
        let c = stitch::Consts::from_exe(&exe).expect("constants");
        let recs = stitch::records(&exe).expect("records");
        eprintln!("{c:?}");
        let handlers: Vec<u32> = recs.iter().map(stitch::addr).collect();
        assert_eq!(&handlers[..5], &[stitch::STAND, stitch::MOVE, stitch::BLAST, stitch::REFILL, stitch::LINK]);
        assert_eq!(c.depth, c.place_depth);
        assert!(c.max_speed > 0.0 && c.turn > 0.0 && c.wrap_x[1] > 0.0 && c.wrap_y[1] > 0.0 && c.hp_frac > 0.0 && c.mp_frac > c.hp_frac);
    }

    /// Oracle (PLAYBOOK R2): the game's FUN_00116530 (wrap), FUN_00116aa0 (arrived), FUN_00116658 (face) and
    /// FUN_00116780 with its fixed cases 0..7 against `stitch::wrap`, `arrived`, `face` and `goal`, over random
    /// positions and goals on an AI object laid out at a free address (+0x628 / +0x62c from the constants as
    /// FUN_001179c0 writes them).
    #[test]
    fn stitch_plane_rules_match_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const AI: u32 = 0x0180_0000;
        let Some(mut a) = archive() else { return };
        let exe = elf(&mut a);
        let c = stitch::Consts::from_exe(&exe).expect("constants");
        let mut ee = Ee::new();
        ee.load_elf(&exe).expect("elf");
        let mut seed = 0x51f1_7c4du32;
        let mut next = |span: f32| {
            seed = seed.wrapping_mul(0x0019_660d).wrapping_add(0x3c6e_f35f);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * span
        };
        let put = |ee: &mut Ee, off: u32, v: [f32; 4]| {
            for (k, x) in v.iter().enumerate() {
                ee.wf32(AI + off + 4 * k as u32, *x);
            }
        };
        let get = |ee: &mut Ee, off: u32| -> [f32; 4] { std::array::from_fn(|k| ee.f32_at(AI + off + 4 * k as u32)) };
        let mut n = 0;
        for round in 0..300 {
            ee.write(AI, &[0u8; 0x790]);
            ee.wf32(AI + 0x628, c.max_speed);
            ee.wf32(AI + 0x62c, c.turn);
            ee.w32(AI + 0x630, 1);
            // Near the edges half the time, small distances a third of the time (the arrival test).
            let span = if round % 2 == 0 { 600.0 } else { 300.0 };
            let pos = [next(span), next(span), c.depth + next(2.0), next(3.0)];
            let goal = if round % 3 == 0 { [pos[0] + next(20.0), pos[1] + next(20.0), pos[2] + next(5.0), pos[3] + next(0.3)] } else { [next(span), next(span), c.depth, next(3.0)] };
            put(&mut ee, 0x640, pos);
            put(&mut ee, 0x650, goal);
            ee.call(0x0011_6530, &[AI as u64], &[], STACK).expect("FUN_00116530");
            let (mut p, mut g) = (pos, goal);
            stitch::wrap(&c, &mut p, &mut g);
            assert_eq!((get(&mut ee, 0x640), get(&mut ee, 0x650)), (p, g), "wrap round {round}");
            let arrived = ee.call(0x0011_6aa0, &[AI as u64], &[], STACK).expect("FUN_00116aa0") != 0;
            assert_eq!(arrived, stitch::arrived(&c, p, g), "arrived round {round}: {p:?} {g:?}");
            // FUN_00116658 on a direction (zero now and then).
            let dir = if round % 7 == 0 { [0.0, 0.0, next(1.0)] } else { [next(1.0), next(1.0), next(1.0)] };
            put(&mut ee, 0x660, [dir[0], dir[1], dir[2], 0.0]);
            ee.wf32(AI + 0x65c, g[3]);
            ee.call(0x0011_6658, &[AI as u64], &[], STACK).expect("FUN_00116658");
            let mut s = stitch::StitchAi { goal: g, dir, ..Default::default() };
            stitch::face(&mut s);
            assert!((ee.f32_at(AI + 0x65c) - s.goal[3]).abs() < 1e-5, "face round {round}: {} vs {}", ee.f32_at(AI + 0x65c), s.goal[3]);
            // FUN_00116780's fixed cases.
            let case = round % 8;
            ee.call(0x0011_6780, &[AI as u64, case as u64], &[], STACK).expect("FUN_00116780");
            let mut want = g;
            stitch::goal(&c, case as i32, p, &mut want, &mut || 0.0, &mut |_| 0);
            let got = get(&mut ee, 0x650);
            assert_eq!(&got[..3], &want[..3], "goal case {case}");
            n += 1;
        }
        eprintln!("oracle: FUN_00116530, FUN_00116aa0, FUN_00116658 and FUN_00116780 (cases 0..7) equal in {n} rounds");
    }

    /// The AI slot list of hosts 10.12 / 10.16 without the disc: the first free slot is taken, the flags
    /// follow the object named, a full list answers 0x18 and changes nothing.
    #[test]
    fn summon_ai_slots_take_the_first_free_one() {
        let who = ai::Who { own: 0x10, player: 0x20 };
        let mut s = ai::SummonAi::default();
        assert_eq!(s.start(who, 0, 0x10, 19, u32::MAX, false, 1, 0), 0);
        assert_eq!((s.slots[0].kind, s.slots[0].flags, s.slots[0].a3, s.slots[0].a4, s.slots[0].a6), (0, 0x21, 19, u32::MAX, 1));
        assert_eq!(s.start(who, 8, 0x20, 0x4000, 0, true, 1, 0), 1);
        assert_eq!(s.slots[1].flags, 2 | 8);
        assert_eq!(s.start(who, 9, 0x10, 0x4000, 0, false, 1, 0), 2);
        assert_eq!(s.slots[2].flags, 1);
        s.mark(2);
        assert_eq!(s.slots[2].flags, 0x4000_0001);
        // A slot whose effect still runs is not free even with a negative kind (FUN_00117668).
        s.slots[3].effect = 7;
        assert_eq!(s.start(who, 1, 0, 0, 0, false, 0, 0), 4);
        for k in 5..ai::SLOTS {
            s.start(who, 1, 0, 0, 0, false, 0, 0);
            assert_eq!(s.slots[k].kind, 1);
        }
        let before = s.slots;
        assert_eq!(s.start(who, 1, 0x10, 0, 0, true, 0, 0), ai::SLOTS);
        assert_eq!(s.slots, before);
    }

    /// Oracle: FUN_00117708 (host 10.12) with FUN_00117668 run by kh2-ee on the user's executable against
    /// `SummonAi::start` over random slot lists and arguments; the player is `DAT_0034e908` (FUN_00111c00(0)).
    #[test]
    fn summon_ai_slot_start_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const AI: u32 = 0x0180_0000;
        const OWN: u32 = 0x0181_0000;
        const PLAYER: u32 = 0x0182_0000;
        const OTHER: u32 = 0x0183_0000;
        // Contexts whose word +4 is the object, as the host wrapper 0x2f6d00 reads them.
        let Some(mut a) = archive() else { return };
        let exe = elf(&mut a);
        let mut ee = Ee::new();
        ee.load_elf(&exe).expect("elf");
        let who = ai::Who { own: OWN, player: PLAYER };
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed = seed.wrapping_mul(0x0019_660d).wrapping_add(0x3c6e_f35f);
            seed
        };
        let mut n = 0;
        for round in 0..200 {
            // A random list: some slots taken (kind >= 0), some with an effect left.
            let mut port = ai::SummonAi::default();
            for (k, s) in port.slots.iter_mut().enumerate() {
                let r = next();
                s.kind = if r & 3 == 0 || round % 50 == 49 { (r >> 8) as i32 & 0xff } else { -1 };
                s.effect = if r & 0x30 == 0x30 { 0x0190_0000 + k as u32 } else { 0 };
                s.flags = next();
                let base = AI + 0x20 + 0x40 * k as u32;
                ee.w32(base, s.flags);
                ee.w32(base + 4, s.kind as u32);
                ee.w32(base + 8, s.effect);
                for o in [0xc, 0x10, 0x14, 0x18] {
                    ee.w32(base + o, 0);
                }
            }
            ee.w32(AI + 4, OWN);
            ee.w32(0x0034_e908, PLAYER);
            let obj = [0, OWN, PLAYER, OTHER][(next() % 4) as usize];
            let kind = [0i32, 1, 8, 9, 19][(next() % 5) as usize];
            let (a3, a4, a6, a7) = (next() & 0xffff, next(), next() & 1, next() & 0xff);
            let flag = next() & 1 == 1;
            let got = ee.call(0x0011_7708, &[AI as u64, kind as i64 as u64, obj as u64, a3 as u64, a4 as u64, flag as u64, a6 as u64, a7 as u64], &[], STACK).expect("FUN_00117708") as u32 as usize;
            let want = port.start(who, kind, obj, a3, a4, flag, a6, a7);
            assert_eq!(got, want, "round {round}: slot index");
            for (k, s) in port.slots.iter().enumerate() {
                let base = AI + 0x20 + 0x40 * k as u32;
                let game = (ee.r32(base), ee.r32(base + 4) as i32, ee.r32(base + 8), ee.r32(base + 0xc), ee.r32(base + 0x10), ee.r32(base + 0x14), ee.r32(base + 0x18));
                assert_eq!(game, (s.flags, s.kind, s.effect, s.a6, s.a7, s.a4, s.a3), "round {round} slot {k}");
            }
            n += 1;
        }
        eprintln!("oracle: FUN_00117708 equal in {n} calls");
    }
}
