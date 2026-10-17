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
//! Stitch's class (module `stitch`, WP-SUM4b) moves him in his camera-space plane, chooses his records and
//! fires his blast; its motion requests play on his body.
//!
//! Genie's class (module `genie`, WP-SUM5) runs the members' follow and his two records, and answers his hosts
//! 10.9 / 10.10.
//!
//! What the app does not have (docs/summons.md "Status"): the friend AI of Chicken Little and Peter Pan (they stand where
//! they were put; their script's motion requests are logged, not played), the event itself (nothing in the app starts it: the wait for it ends at
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
/// Script handles of the objects a summon's script makes with host 1.0 (Tinker Bell, Chicken Little's surprise): one
/// `OBJ_SIZE` block each after the two summon slots' handles, counted up from 1 by `SummonState::kid_serial`.
const KID_BASE: u32 = 0x1810;
/// Ticks a call on a made object waits for the object to be built.
const KID_PATIENCE: u8 = 3;

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

    /// The executable's numbers of the airborne mover's launch (FUN_0017b1e0) and of the fall time (FUN_0017bee0).
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct AirConsts {
        /// `DAT_0037a538`: the gravity FUN_0017bee0 divides twice the height by.
        pub gravity: f32,
        /// `DAT_0037a4f0` and `DAT_0037a4f4`: the factor and divisor of a downward launch's speed.
        pub down_factor: f32,
        pub down_divisor: f32,
    }

    /// FUN_0017bee0 (host 1.100, wrapper 0x173178): `sqrt(2 x / gravity)`, the time a body takes to fall `x`. The EE's
    /// `sqrt.s` takes the root of the absolute value of a negative operand (checked against the game's routine in the
    /// oracle test `air_launch_and_fall_time_match_the_game`).
    pub fn fall_time(x: f32, gravity: f32) -> f32 {
        ((x + x) / gravity).abs().sqrt()
    }

    /// What FUN_0017b1e0 (the airborne mover's launch, vtable +0xc of the mover FUN_0016b698 gives objects) leaves in the
    /// object: the words +0xd0, +0xd4, +0xd8 and +0xdc, and the mover kind 1 (FUN_0017b1b0: +0x514 = 1, +0x100 = -1,
    /// +0x10c bit 0 cleared, +0x104 = 5, +0xcc = 0). The mover's own update (FUN_0017b328 / FUN_0017b440) is not ported.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Launch {
        pub d0: f32,
        pub d4: f32,
        pub d8: f32,
        pub dc: f32,
    }

    /// FUN_0017b1e0(speed): an upward speed (>= 0) leaves +0xd8 = 0, +0xdc = -speed, +0xd0 = 0 and +0xd4 the fall time of
    /// that height; a negative one leaves +0xd8 = speed, +0xd4 = +0xdc = 0 and +0xd0 = `sqrt(speed x factor / divisor)`.
    pub fn launch(speed: f32, c: &AirConsts) -> Launch {
        if 0.0 <= speed {
            Launch { d0: 0.0, d4: fall_time(speed, c.gravity), d8: 0.0, dc: -speed }
        } else {
            Launch { d0: ((speed * c.down_factor) / c.down_divisor).sqrt(), d4: 0.0, d8: speed, dc: 0.0 }
        }
    }

    /// The fields of the AI object the hosts reach, plus the object fields they reach (named "object").
    #[derive(Clone, Debug)]
    pub struct SummonAi {
        /// The object's NeoStatus (200 Stitch .. 204 Chicken Little; the summon's own objects made by a script: 0): set by
        /// `objects` for the tick. The AI class's vtable +0x28 / +0x2c (hosts 10.3 / 10.4) differ by class.
        pub neo: u16,
        /// Host 1.128 on this object: the airborne launch it left (the mover's update is not ported).
        pub air: Option<Launch>,
        /// Host 7.26 this tick (FUN_00112518 -> the callback at object +0xcfc): the limit sub-command asked of the object.
        pub limit_cmd: Option<i32>,
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
        /// The player's HP and maximum as this tick starts (set by `objects`), and the HP added to it by host 1.155 this tick
        /// (applied after the script ran).
        pub player_hp: (i32, i32),
        pub player_heal: i32,
        /// Genie's class fields (AI +0x14 and the members' follow state): `super::genie`.
        pub genie: super::genie::GenieAi,
        /// The fields of Chicken Little's, Peter Pan's and Tinker Bell's classes: `super::scripted`.
        pub scripted: super::scripted::ScriptedAi,
        /// Host 1.0 this tick: the handle given and the `00objentry` id (`SummonState::kid_makes` after the tick).
        pub makes: Vec<(u32, u32)>,
        /// What the script asked of the objects it made, in order (`SummonState::kid_ops` after the tick).
        pub remote: Vec<(u32, KidOp)>,
        /// The serial of the last object made (lent from `SummonState::kid_serial` for the tick).
        pub kid_serial: u32,
        /// The objects made so far and still there (the handles host 1.94 calls valid), and those made this tick.
        pub kids: Vec<u32>,
        /// Host 1.18 on this object (link_start): the parent it follows.
        pub link: Option<Link>,
        /// The effect instances host 10.53 registered (`register_effect`): the list of the object the call names (the summon's
        /// object for Chicken Little's child), kept with the script that made the call.
        pub effect_list: Vec<u32>,
        /// Host 1.277 was called on the player this tick (applied after the script ran).
        pub revive: bool,
        /// The links host 1.102 (or a second 1.18) broke this tick, in order, that had a routine: each routine is run after
        /// the script's tick. The game runs it inside the break (FUN_001becb8 -> FUN_0016c550, synchronously); the app defers
        /// it to the end of the script's update because the host cannot re-enter the runtime that is calling it [inferred
        /// deviation: a routine that reads state the rest of the tick changes sees it a few calls later].
        pub link_broken: Vec<Link>,
    }

    /// A call a summon's script makes on an object it made with host 1.0 (the object exists a tick later: the calls wait
    /// for it, `super::objects`).
    #[derive(Clone, Debug, PartialEq)]
    pub enum KidOp {
        /// Host 1.148 `set_position` (FUN_0016a618), centimetres, the app's space.
        Position(bevy::prelude::Vec3),
        /// Host 1.8 `set_mode` (FUN_00186db0 on the other object's mode machine).
        Mode(String),
        /// Host 1.28 `hide`.
        Hide,
        /// Hosts 1.70 / 1.71.
        SetFlag(i32),
        ClearFlag(i32),
    }

    impl SummonAi {
        /// Host 1.18 (FUN_001beb90): a link already there is broken first (FUN_001becb8, its routine runs), then the new one
        /// is made with the halfwords and the routine and argument of the call.
        pub fn link_start(&mut self, link: Link) {
            self.link_end();
            self.link = Some(link);
        }

        /// Host 1.102 (FUN_00170e38 -> FUN_001becb8) and the first step of 1.18: with a link made (FUN_001beb00 tests the
        /// parent at +0x570) the object leaves its parent's list and the parent is cleared; the object stays where the link
        /// put it. The link's routine is queued in `link_broken` for the tick's end (see there). Without a link nothing happens.
        pub fn link_end(&mut self) -> bool {
            let Some(link) = self.link.take() else { return false };
            if link.routine != 0 {
                self.link_broken.push(link);
            }
            true
        }

        /// Host 1.277 on the player (FUN_00178d80, FUN_001d4a50, then his HP method with his status block's word +4, the
        /// maximum HP, as the amount): the HP method adds the amount and holds the result at the maximum, and the dead bit it
        /// refuses on (+0x7f8 value 4) is cleared first, so a player with HP left is filled up the same way. Returns the HP
        /// afterwards and whether he was down (the dead bit set: leaving it is the app's stand-up).
        pub fn revive_player(hp: i32, max: i32, down: bool) -> (i32, bool) {
            ((hp.max(0) + max).min(max), down)
        }

        /// FUN_0010ecb0(object, 0, 0): the action the class runs and its argument are cleared (the current record too).
        pub fn clear_action(&mut self) {
            self.action = (0, 0);
            self.stitch.choosing.current = None;
        }

        /// FUN_00114170, the vtable +0x28 (host 10.3) of the classes of Genie, Peter Pan and Tinker Bell (the vtables at
        /// 0x354910, 0x354978 and 0x3549b8 all hold it): the action is cleared and a link the object has is broken
        /// (FUN_001beb00, FUN_001becb8). Their +0x2c (host 10.4, FUN_001141c0) is a bare return.
        pub fn base_enter(&mut self) {
            self.clear_action();
            self.link_end();
        }

        /// Chicken Little's vtable +0x28 (FUN_0011ae60, host 10.3): the base part, then AI +0x20 bit 0 (FPS mode), AI +0x24 =
        /// `camera` (`DAT_00378904`), AI +0x28 = 0, and the object linked to the player with the halfwords 0x4000 and 4
        /// (FUN_001beb90, offset zero, so it stands at his origin with his facing). The player's +0x108 / +0x10c bits, the
        /// camera object (FUN_0013e528, mode 9 through FUN_0014e870), the player's mover 0x3e and the HUD interpolators
        /// (FUN_001abf40) are not carried out.
        pub fn fps_enter(&mut self, camera: f32) {
            self.base_enter();
            self.scripted.fps = true;
            self.scripted.fps_arg = [camera, 0.0];
            self.link_start(Link { parent: crate::enemy_obj::PLAYER, kind: 0x4000, flags: 4, routine: 0, arg: 0 });
        }

        /// Chicken Little's vtable +0x2c (FUN_0011b008, host 10.4): the link broken, AI +0x20 bit 0 cleared and the target
        /// record emptied (the player's and the camera's parts are not carried out, see `fps_enter`).
        pub fn fps_leave(&mut self) {
            self.link_end();
            self.scripted.fps = false;
            self.scripted.fps_found = None;
        }

        /// Stitch's vtable +0x28 (FUN_001191a8, host 10.3): the base part, Stitch placed at the first limit spot
        /// (FUN_001175a0 with the vector at 0x3b4050) with the stand motion, and the link to the player marked. The player's
        /// motion 0xfe (FUN_0017f160), his +0x10c bit 0x200, the player and hand objects' parent matrix (FUN_00193228),
        /// the player's mover 0x3c and the callback at object +0xcfc (FUN_00112510 with 0x119488) are not carried out
        /// except the callback, which `limit_cmd` stands for.
        pub fn stitch_limit_enter(&mut self, c: &super::stitch::Consts) {
            self.base_enter();
            super::stitch::place(c, &mut self.stitch, [c.limit_a[0], c.limit_a[1], 0.0, 0.0]);
            self.stitch.motion = Some(super::stitch::M_STAND);
            self.stitch.motion_new = true;
            self.stitch.limit_link = true;
        }

        /// Stitch's vtable +0x2c (FUN_00119360, host 10.4): the player and the hands detached again, the callback removed.
        pub fn stitch_limit_leave(&mut self) {
            self.stitch.limit_link = false;
        }

        /// Host 1.155 on the player (FUN_00186238 -> FUN_001a0100): `amount` is added to the HP held for this tick and the
        /// result clamped to the maximum; a KO'd player (HP 0) stays as he is (`magic_rules::healed`). Returns the HP the
        /// method gives back. The HP gained is kept in `player_heal`.
        pub fn heal_player(&mut self, amount: i32) -> i32 {
            let (hp, max) = self.player_hp;
            let new = crate::magic_rules::healed(hp, max, amount);
            self.player_heal += new - hp;
            self.player_hp.0 = new;
            new
        }

        /// What `objects` does after the script ran: when host 1.155 healed this tick, the player's HP becomes the held
        /// value. Returns (HP before, HP after) for the log.
        pub fn apply_heal(&mut self, hp: &mut i32) -> Option<(i32, i32)> {
            if std::mem::take(&mut self.player_heal) <= 0 {
                return None;
            }
            let before = *hp;
            *hp = self.player_hp.0;
            Some((before, *hp))
        }
    }

    impl Default for SummonAi {
        fn default() -> Self {
            SummonAi {
                neo: 0,
                air: None,
                limit_cmd: None,
                slots: [Slot::default(); SLOTS],
                flags_630: 0,
                stitch: Default::default(),
                action: (0, 0),
                b7c: 0,
                flag_7f8_4: false,
                callbacks_98: Vec::new(),
                link_calls: (0, 0),
                player_hp: (0, 0),
                player_heal: 0,
                genie: Default::default(),
                makes: Vec::new(),
                remote: Vec::new(),
                kid_serial: 0,
                kids: Vec::new(),
                scripted: Default::default(),
                link: None,
                effect_list: Vec::new(),
                revive: false,
                link_broken: Vec::new(),
            }
        }
    }

    /// The link host 1.18 made (FUN_001beb90): this object's parent (+0x570) and the two halfwords it stores (+0x574, +0x576).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Link {
        pub parent: u32,
        pub kind: u16,
        pub flags: u16,
        /// The routine and its argument host 1.18 registers on the link record (+0x10, +0x14; 0 for none). The link's
        /// break (FUN_001becb8) runs the routine with the argument (FUN_0016c550).
        pub routine: u32,
        pub arg: u32,
    }

    impl Link {
        /// Where the linked object stands this tick when its parent is the player. FUN_001beb90 zeroes the offset vector
        /// +0x540 (0, 0, 0, 1) and, with bit 1 of +0x576 clear, the facing +0x550; FUN_0016b698(.., 3) gives the object the
        /// link mover, and FUN_0016b960 reads its pose from the block at +0x70 while linked. The block's later writers were
        /// not read: [inferred] the pose is the parent's origin with the parent's facing (the cleared offsets), and an
        /// object whose bit 1 is set keeps its own facing. `None` for a parent that is not the player (the app has no
        /// other pose to follow).
        pub fn pose(&self, player_pos: bevy::prelude::Vec3, player_yaw: f32, own_yaw: f32) -> Option<(bevy::prelude::Vec3, f32)> {
            (self.parent == crate::enemy_obj::PLAYER).then_some((player_pos, if self.flags & 2 == 0 { player_yaw } else { own_yaw }))
        }
    }

    /// Host 10.53 (FUN_002f74c0 -> FUN_0010c5b8, FUN_00136388): the list at an object's +0xb5c holds up to 0x20
    /// (instance, generation) pairs. FUN_00136388 first drops an entry of the same instance (FUN_00136430), then, when the
    /// list is full, the oldest (FUN_00136430 on the first entry's instance after FUN_00136250 compacted it), and appends.
    /// A zero handle is ignored. The generation (FUN_001d9cf8) is not kept: a handle names one instance for its life here.
    pub fn register_effect(list: &mut Vec<u32>, effect: u32) {
        if effect == 0 {
            return;
        }
        list.retain(|&e| e != effect);
        if list.len() >= 0x20 {
            list.remove(0);
        }
        list.push(effect);
    }

    /// Host 6.49 (FUN_002ee7d8 -> FUN_0019b088): an object is occupied when its +0x518 bit 0 is set (the actor stop,
    /// FUN_0016bea0), or FUN_0019af28 (its +0x588 bits 4 and 0x40 clear and the summon state is 1: the arrival), or its
    /// +0x108 bit 0x400, or the manager at DAT_0036f6a4 holds an entry (FUN_002c5680). `stopped`, `flag_400` and
    /// `manager` are not modelled in the app and read clear; `bits_clear` is [inferred] true for the summon objects.
    pub fn object_occupied(summon_state: u8, bits_clear: bool, stopped: bool, flag_400: bool, manager: bool) -> bool {
        stopped || (bits_clear && summon_state == 1) || flag_400 || manager
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
    /// Records 5 and 6 (flags 0x212): the limit's second and third attacks. Like `LINK` (record 4) they are never chosen
    /// by the engine: the object's callback FUN_00119488 (+0xcfc, set by FUN_001191a8, run by host 7.26) starts them for
    /// the limit sub-commands 525, 526 and 527.
    pub const LIMIT_B: u32 = 0x118ec0;
    pub const LIMIT_C: u32 = 0x119068;
    /// Motion requests (immediates): stand (FUN_00117cf8, FUN_0017e2d8 with motion 0), move (0xa1), the blast
    /// (0xcb), the MP refill (0xcc), and the limit records' (0xc9, 0xcd, 0xf2).
    pub const M_STAND: u16 = 0;
    pub const M_MOVE: u16 = 0xa1;
    pub const M_BLAST: u16 = 0xcb;
    pub const M_REFILL: u16 = 0xcc;
    pub const M_LINK: u16 = 0xc9;
    pub const M_LIMIT_B: u16 = 0xcd;
    pub const M_LIMIT_C: u16 = 0xf2;

    /// FUN_00119488's switch (the callback host 7.26 runs): the limit sub-command to the record's handler, the pointers at
    /// 0x378898, 0x3788a0 and 0x3788a8. Any other command is not read past its prologue (FUN_0010ecb0(object, 0, 0)).
    pub fn limit_record(cmd: i32) -> Option<u32> {
        match cmd {
            525 => Some(LINK),
            526 => Some(LIMIT_B),
            527 => Some(LIMIT_C),
            _ => None,
        }
    }
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
        /// The two vectors FUN_001198c8 builds at 0x3b4050 and 0x3b4060 (x, y; z is `DAT_003788c0`, the depth
        /// FUN_001175a0 overwrites): the spots the limit's records (FUN_00118db8, FUN_00118ec0, FUN_00119068,
        /// FUN_001191a8) put Stitch at.
        pub limit_a: [f32; 2],
        pub limit_b: [f32; 2],
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
                limit_a: [f(elf, 0x3788b8)?, f(elf, 0x3788bc)?],
                limit_b: [f(elf, 0x3788c4)?, f(elf, 0x3788c8)?],
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
        /// +0x674: the frames left to the next heal of the player (FUN_001155e0's tail). The constructor
        /// (FUN_001179c0) starts it at the entry's +4 (`Consts::period`); `None` until the first update.
        pub heal_timer: Option<f32>,
        /// Object +0x10c bit 0x200 of the player and the hands, and the link of the player to Stitch for the limit
        /// (FUN_001191a8 sets it, FUN_00119360 ends it; hosts 10.3 / 10.4).
        pub limit_link: bool,
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

    /// `limit_command` with the AI's own generator.
    pub fn limit_tick(c: &Consts, recs: &[Record], ai: &mut StitchAi, now: &Now, cmd: i32) -> Out {
        let rng = std::cell::RefCell::new(std::mem::take(&mut ai.rng));
        let out = limit_command(c, recs, ai, now, cmd, &mut || rng.borrow_mut().roll(), &mut |n| rng.borrow_mut().rand(n));
        ai.rng = rng.into_inner();
        out
    }

    impl Out {
        /// The asks of two runs in the same frame, in order.
        pub fn merge(&mut self, o: Out) {
            self.started = o.started.or(self.started);
            self.blast |= o.blast;
            self.refill |= o.refill;
            self.slots.extend(o.slots);
            self.player_effect = o.player_effect.or(self.player_effect);
            self.player_motion = o.player_motion.or(self.player_motion);
            self.heal |= o.heal;
        }
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
        /// The motion the limit's record linked the player to (FUN_0017f160's last argument).
        pub player_motion: Option<u16>,
        /// The player's HP method +0x78 is called with (1, 0, 0): the heal of FUN_001155e0's tail.
        pub heal: bool,
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
            // The limit's records (phase 1 of FUN_00118db8 / FUN_00118ec0 / FUN_00119068): never fresh, and running while
            // the +0xb7c bit 2 is set and the motion they asked for plays. FUN_00118ec0 also ends the actors' stop
            // 0x11 (FUN_00184b40) when the motion has changed [not carried out].
            Some(LINK) => cont && ai.motion == Some(M_LINK),
            Some(LIMIT_B) => cont && ai.motion == Some(M_LIMIT_B),
            Some(LIMIT_C) => cont && ai.motion == Some(M_LIMIT_C),
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
            // FUN_00118db8 (525), FUN_00118ec0 (526), FUN_00119068 (527) phase 0: with +0xb7c bit 2 clear, the target pick
            // (FUN_00118b38: the blast hits every enemy in view, so the pick is not needed), Stitch at the limit's spot
            // (FUN_001175a0), his motion, the player's linked motion (FUN_0017f160 with 0x101, 0xff, 0x100) and, for
            // the two attacks, the AI slots (FUN_00117708); the attacks then run FUN_00118110 with their motion every frame.
            Some(h @ (LINK | LIMIT_B | LIMIT_C)) => {
                ai.dir = [0.0; 3];
                let (motion, spot, player_motion) = match h {
                    LINK => (M_LINK, c.limit_a, 0x101),
                    LIMIT_B => (M_LIMIT_B, c.limit_a, 0xff),
                    _ => (M_LIMIT_C, c.limit_b, 0x100),
                };
                if !cont {
                    place(c, ai, [spot[0], spot[1], 0.0, 0.0]);
                    request(ai, motion);
                    out.player_motion = Some(player_motion);
                    match h {
                        LIMIT_B => out.slots.extend([(3, false, u32::MAX), (4, true, 0x13), (4, false, 0x41)]),
                        LIMIT_C => out.slots.extend([(0xc, false, u32::MAX), (0xd, false, 0x41)]),
                        _ => {}
                    }
                }
                if h != LINK {
                    if ai.attack.is_none() {
                        ai.attack = Some(motion);
                    }
                    out.blast = true;
                }
            }
            _ => ai.dir = [0.0; 3],
        }
    }

    /// FUN_00119488 (the callback host 7.26 runs for the limit sub-commands, object +0xcfc): for 525, 526 and 527 the
    /// record's handler runs its phase 0 with the +0xb7c bit 2 as it stands, becomes the current action
    /// (FUN_0010ecb0) and the bits 0 and 2 are cleared, so the next frame's choice keeps it while its test holds. Any other
    /// command only clears the action.
    #[allow(clippy::too_many_arguments)]
    pub fn limit_command(c: &Consts, recs: &[Record], ai: &mut StitchAi, now: &Now, cmd: i32, roll: &mut dyn FnMut() -> f32, rand: &mut dyn FnMut(u32) -> u32) -> Out {
        let mut out = Out::default();
        ai.choosing.current = None;
        let Some(want) = limit_record(cmd) else { return out };
        if let Some(i) = recs.iter().position(|r| addr(r) == want) {
            let cont = ai.choosing.continuing;
            run(c, recs, ai, now, i, cont, &mut out, roll, rand);
            ai.choosing.current = Some(i);
            ai.choosing.continuing = false;
            out.started = Some((i, true));
        }
        out
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
        // Records 4 and 5 queued motion 0 behind theirs (FUN_0017e378 with 0): when their motion ended the controller plays
        // it, the object's motion is 0 and the records' test (the motion is theirs) fails. Record 6 queues nothing: its
        // motion stays the current one until the script clears the action (host 10.39).
        if now.motion_done && !ai.motion_new && matches!(ai.motion, Some(M_LINK | M_LIMIT_B)) {
            request(ai, M_STAND);
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
                battle::choose(recs, &mut st, &Params::default(), CLASS, &MemberNow::default(), false, false, |_| None, &mut can_f, &mut *roll, &mut *rand)
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
        // The attack object lives in the motion FUN_00118110 was given (0xcb for the blast, 0xcd and 0xf2 for the limit's).
        if ai.attack.is_some() && (ai.motion != ai.attack || (now.motion_done && !ai.motion_new)) {
            ai.attack = None;
        }
        // FUN_001155e0's tail (also reached when the object is not free): while the player's HP (status word 0) is
        // below his maximum (word 1) the timer +0x674 loses a frame unit; at 0 or less the player's vtable +0x78
        // is called with (1, 0, 0) (one HP) and the timer starts again at the entry's +4.
        if now.hp.0 < now.hp.1 {
            let left = ai.heal_timer.unwrap_or(c.period) - c.frame;
            if left <= 0.0 {
                out.heal = true;
                ai.heal_timer = Some(c.period);
            } else {
                ai.heal_timer = Some(left);
            }
        }
        out
    }

    /// FUN_00117320's translation: the position in the parent's (the camera's [inferred]) space, game units
    /// and axes (x right, y down, z forward).
    pub fn camera_point(c: &Consts, pos: [f32; 4]) -> [f32; 3] {
        [pos[0] * c.scale, pos[1] * c.scale, pos[2] * c.scale]
    }
}

/// Genie's friend AI (WP-SUM5): the class FUN_001199f8 (0x18 bytes, vtable 0x354910, class 0xf at object +0xb6c,
/// object +0xb7c |= 0x800), two action records at 0x3548a8. Unlike Stitch, FUN_0010b780 leaves Genie the members'
/// update (object +0xb64 = `PTR_FUN_00378488` = FUN_00111070): out of battle the player relation (FUN_001102c8),
/// in battle the danger test (FUN_0010f830, summon class: the player only), the emergency pass (FUN_0010f8d8) and
/// the choice (FUN_0010fcd0); then the steer (FUN_00110468). The follow rules are party_rules' (the same routines
/// as Donald's and Goofy's), with the class's own thresholds (0x34ea00 + offset[0xf]).
///
/// - Record 0 (flags 0x201, FUN_00119af8): always feasible; phase 0 is FUN_001102c8, the follow of the player,
///   whose class 0xf branch raises a speed above the walk speed to the player's last movement (+0x860) plus
///   `DAT_00378574` when that is faster.
/// - Record 1 (flags 0x10, FUN_00119b28): feasible while the player's HP is below `DAT_003788d0` of its maximum
///   (FUN_0010f490), which also makes the player the record's target; phase 0 is FUN_0010c9d0(object, 1, target,
///   0): speed mode 0, the command of item 1 (FUN_001a1fc0(1): FUN_001a66e8 finds item 1 in 03system.bin `item`,
///   its +0x10 is the command, FUN_001b6f48 the `cmd` row) kept at object +0xb2c with the target at +0xb30, and
///   motion request 9 (FUN_00175e20, blend `DAT_0037a4b8`).
/// - The first update (FUN_001112e8, object +0xb7c bit 0x20000000 clear) calls the object's vtable +0x94
///   (FUN_0010c630 in 0x352e1c and 0x35feac), which calls the AI's vtable +0x18: Genie's FUN_00119a40 reads byte 7
///   back (FUN_001c6b28 -> FUN_001c6b10: the table at 0x37c036) and runs FUN_00119c18 with that command.
pub mod genie {
    use crate::party_battle::{self as battle, Choice, Choosing, Handler, MemberNow, Params, Record};

    /// Genie's NeoStatus and AI class (FUN_0010b780: +0xb6c = 0xf).
    pub const NEO: u16 = 201;
    pub const CLASS: u8 = 0xf;
    /// FUN_001199f8: +0x10 = 0x3548a8, +0xc = 2 records.
    pub const RECORDS: u32 = 0x3548a8;
    pub const RECORD_COUNT: u32 = 2;
    /// The handlers (record +4).
    pub const FOLLOW: u32 = 0x119af8;
    pub const POTION: u32 = 0x119b28;
    /// FUN_00119c18 / FUN_00119ca8: five (form command, limit command) pairs of i16.
    pub const FORMS: u32 = 0x3548f0;
    pub const FORM_COUNT: usize = 5;
    /// FUN_001c6b10: byte 7 to the form command (u16 per byte value).
    pub const FORM_COMMANDS: u32 = 0x37c036;
    /// FUN_00119b28: the player's HP fraction (`DAT_003788d0`); FUN_001102c8: the speed added to the player's
    /// movement (`DAT_00378574`).
    pub const HEAL_FRACTION: u32 = 0x3788d0;
    pub const SPEED_ADD: u32 = 0x378574;
    /// FUN_002f6320's table for NeoStatus 0xc9 (201).
    pub const LOCO: u32 = 0x3716d8;
    /// FUN_00119b28 -> FUN_0010c9d0(object, 1, ..): the item whose command Genie uses on the player.
    pub const POTION_ITEM: u16 = 1;
    /// FUN_00175e20's motion request.
    pub const M_USE: u16 = 9;

    /// The executable's numbers the class reads.
    #[derive(Clone, Copy, Debug, PartialEq, Default)]
    pub struct Consts {
        pub forms: [(i16, i16); FORM_COUNT],
        /// `0x37c036` + 2 x byte 7, for byte 7 = 0..5.
        pub form_commands: [u16; 6],
        pub heal_fraction: f32,
        pub speed_add: f32,
        /// FUN_002f6320's locomotion motions for NeoStatus 201 (0x3716d8): the first, second and third entries, then -1, -1.
        pub loco: [i32; 5],
    }

    fn w(elf: &[u8], va: u32) -> Option<u32> {
        crate::party_rules::exe_bytes(elf, va, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn h(elf: &[u8], va: u32) -> Option<i16> {
        crate::party_rules::exe_bytes(elf, va, 2).map(|b| i16::from_le_bytes([b[0], b[1]]))
    }

    impl Consts {
        pub fn from_exe(elf: &[u8]) -> Option<Consts> {
            let mut forms = [(0, 0); FORM_COUNT];
            for (k, p) in forms.iter_mut().enumerate() {
                *p = (h(elf, FORMS + 4 * k as u32)?, h(elf, FORMS + 4 * k as u32 + 2)?);
            }
            let mut form_commands = [0; 6];
            for (k, c) in form_commands.iter_mut().enumerate() {
                *c = h(elf, FORM_COMMANDS + 2 * k as u32)? as u16;
            }
            let mut loco = [-1; 5];
            for (k, m) in loco.iter_mut().enumerate() {
                *m = w(elf, LOCO + 4 * k as u32)? as i32;
            }
            Some(Consts { forms, form_commands, heal_fraction: f32::from_bits(w(elf, HEAL_FRACTION)?), speed_add: f32::from_bits(w(elf, SPEED_ADD)?), loco })
        }
    }

    /// The two records (flags, handler; no parameter entries).
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

    /// FUN_00119ca8 (host 10.10): the limit command paired with AI +0x14, 0 when none.
    pub fn limit(c: &Consts, cmd: i32) -> i32 {
        c.forms.iter().find(|p| p.0 as i32 == cmd).map_or(0, |p| p.1 as i32)
    }

    /// What FUN_00119c18 did: the parts it hid (FUN_00192ce8: object +0x670's vtable +0x40 with the part), the part
    /// it showed (FUN_00192cd0: vtable +0x3c) and the new byte 7 (FUN_001c6b48).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Form {
        pub hidden: [i32; FORM_COUNT],
        pub shown: Option<i32>,
        pub byte7: u8,
    }

    /// FUN_00119c18 (host 10.9, and FUN_00119a40): AI +0x14 = `cmd` (the caller's); parts 2..6 hidden; the row k
    /// whose form command is `cmd` shows part k + 2 and sets byte 7 to k + 1; without a row byte 7 becomes 6.
    pub fn form(c: &Consts, cmd: i32) -> Form {
        let hidden = [2, 3, 4, 5, 6];
        match c.forms.iter().position(|p| p.0 as i32 == cmd) {
            Some(k) => Form { hidden, shown: Some(k as i32 + 2), byte7: k as u8 + 1 },
            None => Form { hidden, shown: None, byte7: FORM_COUNT as u8 + 1 },
        }
    }

    /// FUN_001c6b10: the form command of byte 7 (0 past the table, [inferred]: the game reads on).
    pub fn start_command(c: &Consts, byte7: u8) -> i32 {
        c.form_commands.get(byte7 as usize).map_or(0, |&v| v as i32)
    }

    /// Genie's fields (AI +0x14 and the object's fields the members' engine keeps).
    #[derive(Clone, Debug, Default)]
    pub struct GenieAi {
        /// AI +0x14: the form command (FUN_00119c18).
        pub cmd: i32,
        /// Object +0xb7c bit 0x20000000: the first update ran (FUN_001112e8).
        pub init: bool,
        /// The members' follow state (+0xb78, +0xb7c bit 0x10, the steer ring).
        pub follow: crate::party_rules::Follow,
        /// FUN_0010fcd0's state.
        pub choosing: Choosing,
        /// FUN_00119c18's last outcome, for the world to apply (the parts and byte 7).
        pub form: Option<Form>,
        /// Record 1 ran: motion 9 plays and the object is not free until it ends [inferred, as the members'
        /// item use].
        pub using: bool,
        pub rng: super::stitch::Rng,
    }

    /// What the update reads.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Now {
        /// FUN_001903d8.
        pub battle: bool,
        /// The player's HP and maximum (his status block +0, +4).
        pub hp: (i32, i32),
    }

    /// What one decision asks of the world.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub struct Out {
        /// A record started (index, by the emergency pass).
        pub started: Option<(usize, bool)>,
        /// Record 0 (or the field) runs FUN_001102c8: follow the player.
        pub follow: bool,
        /// Record 1's phase 0: the potion command on the player with motion 9.
        pub potion: bool,
    }

    /// A record's phase 1.
    pub fn can(c: &Consts, recs: &[Record], now: &Now, i: usize) -> bool {
        match recs.get(i).map(|r| r.handler) {
            Some(Handler::Other(FOLLOW)) => true,
            Some(Handler::Other(POTION)) => battle::below_fraction(now.hp.0, now.hp.1, c.heal_fraction),
            _ => false,
        }
    }

    /// FUN_00111070's decision while the object is free: out of battle the follow; in battle the emergency pass when
    /// the player is in danger (FUN_0010f830 for a summon class), else FUN_0010fcd0, and the chosen record's phase 0.
    pub fn decide(c: &Consts, recs: &[Record], ai: &mut GenieAi, now: &Now) -> Out {
        let mut out = Out::default();
        if !now.battle {
            out.follow = true;
            return out;
        }
        let rng = std::cell::RefCell::new(std::mem::take(&mut ai.rng));
        let player = battle::Vital { hp: now.hp.0, hp_max: now.hp.1, ..Default::default() };
        let danger = battle::danger_member(player, battle::Vital::default(), None, true);
        let mut st = std::mem::take(&mut ai.choosing);
        let urgent = if danger.is_some() { battle::emergency(recs, &mut st, false, false, |i| can(c, recs, now, i), |n| rng.borrow_mut().rand(n)) } else { None };
        let choice = match urgent {
            Some(k) => Choice::Do(k),
            None => battle::choose(recs, &mut st, &Params::default(), CLASS, &MemberNow::default(), false, false, |_| None, |i, _| can(c, recs, now, i), || rng.borrow_mut().roll(), |n| rng.borrow_mut().rand(n)),
        };
        ai.choosing = st;
        ai.rng = rng.into_inner();
        let rec = match choice {
            Choice::Keep(i) => Some(i),
            Choice::Do(i) => {
                out.started = Some((i, urgent.is_some()));
                Some(i)
            }
            _ => None,
        };
        match rec.and_then(|i| recs.get(i)).map(|r| r.handler) {
            Some(Handler::Other(POTION)) => out.potion = true,
            // Record 0, and FUN_001103f8 when nothing was chosen [inferred: its player relation].
            _ => out.follow = true,
        }
        out
    }

    /// FUN_001102c8's class 0xf branch: a speed above the walk speed becomes the player's movement plus
    /// `DAT_00378574` when that is faster.
    pub fn follow_speed(c: &Consts, speed: f32, walk: f32, player_move: f32) -> f32 {
        if walk < speed && speed < player_move { player_move + c.speed_add } else { speed }
    }
}

/// Chicken Little, Peter Pan and Tinker Bell (WP-SUM8): the friend AI classes 0x12, 0x10 and 0x11 that
/// FUN_0010b780 picks for NeoStatus 204, 202 and 203.
///
/// Peter Pan and Tinker Bell keep the members' update FUN_00111070; Chicken Little's own update
/// FUN_0011a2e8 (object +0xb64) runs it first and then keeps the aim point (AI +0x40, below). The classes have
/// at most one action record (FUN_0011a450 / FUN_0011a048: +0xc = 1, +0x10 = 0x3549f0 / 0x354948; Tinker
/// Bell's FUN_0011a230 sets neither, so she has none). The record is flagged 3 (not counted, an attack), always
/// feasible, and its handler (FUN_0011a590 / FUN_0011a118) starts the script mode named by the string at record
/// +0x20 (`"battle"`) with FUN_00186e70 (the mode switch that checks the running mode's priority): it is the
/// whole of what the class decides. The script's `battle` mode then picks the sub-mode (`follow_player`, `ball`,
/// `whistle`, `surprise`, `use_potion`) by its own tests and queues `battle` again.
///
/// FUN_00111070 (the members' update) is what runs the record and the follow:
///
/// - out of battle (FUN_001903d8 false): FUN_001102c8, the follow step toward the follow point;
/// - in battle: the choice FUN_0010fcd0, which returns at once while the script is busy (FUN_0010f698: object
///   +0x390 != 0, the running mode record, so `current_mode()` is `Some`); with no mode running the record is
///   chosen (nothing else is feasible) and its handler starts `battle`;
/// - then the steer FUN_00110468 with the direction and speed the follow step left (the script's
///   `follow_player` mode calls host 10.20 = FUN_001102c8 every frame).
///
/// FUN_001102c8 raises a walking speed to the player's movement plus `DAT_00378574` only for the classes 0xc,
/// 0xf and 0x10: Peter Pan (0x10) has the rule, Chicken Little (0x12) and Tinker Bell (0x11) do not.
pub mod scripted {
    use crate::party_battle::{self as battle, Choice, Choosing, Handler, MemberNow, Params, Record};

    /// What FUN_0010b780 gives a NeoStatus.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Class {
        pub name: &'static str,
        pub neo: u16,
        /// Object +0xb6c.
        pub class: u8,
        /// AI +0x10 (the records) and +0xc (their count).
        pub records: u32,
        pub count: u32,
        /// The bits FUN_0010b780 ors into object +0xb7c.
        pub b7c: u32,
    }

    pub const CHICKEN_NEO: u16 = 204;
    pub const CHICKEN: Class = Class { name: "Chicken Little", neo: CHICKEN_NEO, class: 0x12, records: 0x3549f0, count: 1, b7c: 0x1000 };
    pub const PAN: Class = Class { name: "Peter Pan", neo: 202, class: 0x10, records: 0x354948, count: 1, b7c: 0x800 };
    pub const TINKER: Class = Class { name: "Tinker Bell", neo: 203, class: 0x11, records: 0, count: 0, b7c: 0x802 };
    pub const CLASSES: [Class; 3] = [CHICKEN, PAN, TINKER];

    /// The handlers of the one record (FUN_0011a590 for Chicken Little, FUN_0011a118 for Peter Pan): the same
    /// code, phase 0 switches the script to the record's mode, phase 1 answers 1 and hands the AI's target
    /// record (+0x9f4) out.
    pub const START_MODE: [u32; 2] = [0x11a590, 0x11a118];
    /// `DAT_00378574`, the speed FUN_001102c8 adds for Peter Pan.
    pub const SPEED_ADD: u32 = 0x378574;
    /// FUN_0011a2e8 (Chicken Little): `DAT_003788e8`, how far in front of the object the aim point lies without a
    /// target.
    pub const AIM_AHEAD: u32 = 0x3788e8;
    /// FUN_00119ce8: `DAT_003788d8`, the window of frames in which a party member may join the player's attack.
    pub const JOIN_WINDOW: u32 = 0x3788d8;

    pub fn class_of(neo: u16) -> Option<&'static Class> {
        CLASSES.iter().find(|c| c.neo == neo)
    }

    /// The numbers the classes read.
    #[derive(Clone, Copy, Debug, PartialEq, Default)]
    pub struct Consts {
        pub speed_add: f32,
        pub aim_ahead: f32,
        pub join_window: f32,
    }

    fn w(elf: &[u8], va: u32) -> Option<u32> {
        crate::party_rules::exe_bytes(elf, va, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn cstr(elf: &[u8], va: u32) -> Option<String> {
        let mut out = Vec::new();
        for k in 0..32 {
            let b = crate::party_rules::exe_bytes(elf, va + k, 1)?[0];
            if b == 0 {
                return String::from_utf8(out).ok();
            }
            out.push(b);
        }
        None
    }

    impl Consts {
        pub fn from_exe(elf: &[u8]) -> Option<Consts> {
            let f = |va: u32| w(elf, va).map(f32::from_bits);
            Some(Consts { speed_add: f(SPEED_ADD)?, aim_ahead: f(AIM_AHEAD)?, join_window: f(JOIN_WINDOW)? })
        }
    }

    /// A class with its records and its locomotion motions.
    #[derive(Clone, Debug)]
    pub struct Data {
        pub class: Class,
        pub records: Vec<Record>,
        /// FUN_002f6320's motion table of the NeoStatus (0x3716a8, 0x3716c0 for 202 .. 0xca, 0x3716d8 for 201 ..
        /// 0xc9): walk, run, the stop motion and the two turn motions (-1: none).
        pub loco: [i32; 5],
    }

    impl Data {
        pub fn from_exe(elf: &[u8], c: &Class) -> Option<Data> {
            let table = if c.neo == PAN.neo { 0x3716c0 } else { 0x3716a8 };
            let mut loco = [-1; 5];
            for (k, m) in loco.iter_mut().enumerate() {
                *m = w(elf, table + 4 * k as u32)? as i32;
            }
            Some(Data { class: *c, records: records(elf, c)?, loco })
        }
    }

    /// The class's records (flags, handler, mode string at +0x20; no parameter entries).
    pub fn records(elf: &[u8], c: &Class) -> Option<Vec<Record>> {
        (0..c.count)
            .map(|i| {
                let r = c.records + i * 0x24;
                Some(Record {
                    flags: w(elf, r)?,
                    handler: Handler::from_addr(w(elf, r + 4)?),
                    id: w(elf, r + 0xc)?,
                    entry: None,
                    ability: w(elf, r + 0x14)?,
                    item: w(elf, r + 0x18)?,
                    setting: w(elf, r + 0x1c)? as i32,
                    mode: cstr(elf, w(elf, r + 0x20)?)?,
                })
            })
            .collect()
    }

    /// The class's fields: what the group 10 hosts keep of the AI object and of the object.
    #[derive(Clone, Debug, Default)]
    pub struct ScriptedAi {
        /// The members' follow state (+0xb78, +0xb7c bit 0x10, the steer ring).
        pub follow: crate::party_rules::Follow,
        /// FUN_0010fcd0's state.
        pub choosing: Choosing,
        pub rng: super::stitch::Rng,
        /// Host 10.20 ran this tick (FUN_001102c8: the follow step's direction and speed are fresh).
        pub follow_asked: bool,
        /// AI +0x20 bit 0: the FPS mode is on (FUN_0011ae60 sets it, FUN_0011b008 clears it; hosts 10.3 / 10.4).
        pub fps: bool,
        /// AI +0x24 and +0x28 (FUN_0011ae60 sets them: the camera distance argument `DAT_00378904` and 0).
        pub fps_arg: [f32; 2],
        /// The enemy host 10.51 found in the aim box last (FUN_0011a6d8's record), and how many searches ran.
        pub fps_found: Option<bevy::prelude::Entity>,
        pub fps_searches: u32,
        /// AI +0x40: the aim point (FUN_0011a2e8 keeps it, host 10.59 sets it, 10.5 reads it).
        pub aim: [f32; 4],
        /// Host 10.29 this tick: the item whose command is used on the target and the target's handle.
        pub item: Option<(i32, u32)>,
        /// The item use in progress (motion 9, then the trigger, then the wait): see `ItemUse`.
        pub using: Option<ItemUse>,
        /// The first update ran (FUN_001112e8, +0xb7c bit 0x20000000).
        pub init: bool,
        /// The object's target record (+0x9f4) as the script made it (1.160, a target search), and its position
        /// (object +0xbe0, host 10.8).
        pub target: Option<bevy::prelude::Entity>,
        pub target_pos: [f32; 4],
        /// The player's attack the member may join (FUN_00112850 at the attack's start: the body slot of his
        /// attack motion), and whether the mark 0x200 is set (host 10.26).
        pub armed: Option<usize>,
        pub joined: bool,
        /// Host 10.33 / 10.34 took the player's lock-on out of the list.
        pub recent_taken: bool,
        /// The motion host 1.13 queued after the running one (FUN_0017e170).
        pub queued: Option<usize>,
        /// The hits the script's attack objects landed on named enemies this tick (host 2.17): attack row, victim handle.
        pub hits: Vec<(i16, u32)>,
        /// The object's bones' world matrices and the root's (Bevy space), from the last drawn pose (host 1.83).
        pub bones: Vec<bevy::prelude::Mat4>,
        pub root: bevy::prelude::Mat4,
    }

    /// A summon's item use (host 10.29, and Genie's record 1: FUN_0010c9d0 -> FUN_00175e20 sets the item at object +0xb2c,
    /// the target at +0xb30 and requests motion 9). At frame trigger 6 of that motion (FUN_00188498 case 6 -> FUN_001b8710)
    /// the item is not taken from any slot (FUN_00175ec0 answers true for the summon objects: +0x588 bit 0x400000) and
    /// FUN_001b82b8 queues the use with the wait `DAT_0037bba4`; when FUN_001b84c8's countdown ends the item's effect
    /// lands on the target (FUN_001b8658 -> FUN_001a11a0).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct ItemUse {
        /// The item (`item` sheet `Id`), the motion slot the use plays in, and the frames left once the trigger fired.
        pub item: u16,
        pub slot: usize,
        pub left: Option<f32>,
    }

    /// What one tick of an item use did.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum UseStep {
        /// Nothing yet: the motion plays on, or the countdown runs.
        Wait,
        /// The motion ended before its trigger: nothing is used (FUN_001b8710 never ran).
        Ended,
        /// The trigger fired: the use is queued.
        Queued,
        /// The wait is over: the effect lands.
        Land,
    }

    impl ItemUse {
        /// One tick: before the trigger the use needs its motion playing (`in_motion`); on the trigger it is queued with `wait`
        /// frames, of which the tick itself already takes `frame` (as `party::party_item_uses`, which counts the queue in the
        /// same pass); afterwards `frame` is taken off each tick and the effect lands at 0 or less.
        pub fn step(&mut self, in_motion: bool, trigger: bool, wait: f32, frame: f32) -> UseStep {
            match self.left {
                None if !in_motion => UseStep::Ended,
                None if trigger => {
                    self.left = Some(wait - frame);
                    UseStep::Queued
                }
                None => UseStep::Wait,
                Some(left) => {
                    self.left = Some(left - frame);
                    if left - frame <= 0.0 { UseStep::Land } else { UseStep::Wait }
                }
            }
        }
    }

    /// What the decision reads.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct Now {
        /// FUN_001903d8.
        pub battle: bool,
        /// The script runs a mode (object +0x390 != 0, FUN_0010f698).
        pub mode_running: bool,
    }

    /// What one decision asks of the world.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Out {
        /// The record's phase 0: start this mode (FUN_00186e70) and note the record.
        pub start: Option<(usize, String)>,
        /// FUN_001102c8 runs in the update itself (out of battle).
        pub follow: bool,
    }

    /// FUN_00111070's decision for the classes: out of battle the follow; in battle FUN_0010fcd0 over the
    /// records (nothing with the script busy), and the record's phase 0.
    pub fn decide(recs: &[Record], class: u8, ai: &mut ScriptedAi, now: &Now) -> Out {
        let mut out = Out::default();
        if !now.battle {
            out.follow = true;
            return out;
        }
        // FUN_0010f8d8 (the emergency pass over the records flagged 0x10 / 0x20) finds none: the record is
        // flagged 3. FUN_0010fcd0 returns at once while the script is busy.
        if now.mode_running {
            return out;
        }
        let rng = std::cell::RefCell::new(std::mem::take(&mut ai.rng));
        let mut st = std::mem::take(&mut ai.choosing);
        if st.current.is_some_and(|c| c >= recs.len()) {
            st.current = None;
        }
        // The stand-ins for what FUN_0010fcd0 reads of the member, each tied to the record so a record that does not fit
        // them is never chosen instead of passing through an always-true test:
        // - The record's phase 1 (`can`): the handlers FUN_0011a590 / FUN_0011a118 answer 1 (see `START_MODE`); any other
        //   handler is not ported here and is not offered.
        // - The setting byte (`setting`): read only for a record whose +0x1c is not -1; the classes' record has -1, any
        //   other is not offered.
        // - The MP tests (`MemberNow`, `Params`): read only for a record whose entry costs MP; the classes' record has no
        //   parameter entry, any other is not offered. [inferred from the one record per class, `Data::from_exe`]
        // With those tests the stand-ins (`MemberNow::default()`, `Params::default()`) are never read.
        let fits = |r: &Record| matches!(r.handler, Handler::Other(h) if START_MODE.contains(&h)) && r.setting == -1 && r.entry.is_none_or(|e| e.mp <= 0);
        let choice = battle::choose(
            recs,
            &mut st,
            &Params::default(),
            class,
            &MemberNow::default(),
            class == PAN.class,
            false,
            |_| None,
            |i, _| fits(&recs[i]),
            || rng.borrow_mut().roll(),
            |n| rng.borrow_mut().rand(n),
        );
        ai.choosing = st;
        ai.rng = rng.into_inner();
        if let Choice::Do(i) | Choice::Keep(i) = choice {
            out.start = Some((i, recs[i].mode.clone()));
        }
        out
    }

    /// FUN_001102c8's speed rule for the classes 0xc, 0xf and 0x10 (the others return before it).
    pub fn follow_speed(class: u8, c: &Consts, speed: f32, walk: f32, player_move: f32) -> f32 {
        if matches!(class, 0xc | 0xf | 0x10) && walk < speed && speed < player_move { player_move + c.speed_add } else { speed }
    }

    /// FUN_0011a2e8's aim point: the position of the object's target record when it names a valid target, else the
    /// object's position plus its facing vector (FUN_0016a890, +0x550) times `DAT_003788e8`.
    pub fn aim_point(c: &Consts, target: Option<[f32; 4]>, pos: [f32; 4], facing: [f32; 4]) -> [f32; 4] {
        match target {
            Some(t) => t,
            None => [pos[0] + facing[0] * c.aim_ahead, pos[1] + facing[1] * c.aim_ahead, pos[2] + facing[2] * c.aim_ahead, pos[3] + facing[3] * c.aim_ahead],
        }
    }

    /// FUN_00119ce8 (host 10.25): a member may join the player's attack: the object's +0x200 mark is clear, the
    /// player's recorded motion `DAT_0034e9d0` is not negative and `DAT_0034e9c8 + window` has not fallen below
    /// `DAT_0034e9cc`.
    pub fn join_ready(c: &Consts, b7c: u32, motion: i32, since: f32, mark: f32) -> bool {
        motion >= 0 && b7c & 0x200 == 0 && !matches!((since + c.join_window).partial_cmp(&mark), Some(std::cmp::Ordering::Less))
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

/// Chicken Little's FPS limit, the part of it that is a rule of its own: the screen-box target search of host 10.51
/// (FUN_0011a6d8).
pub mod fps {
    use bevy::prelude::*;

    /// The game camera as FUN_0011a6d8 projects with it: the world to view transform and the focal length it multiplies
    /// by (`FUN_00106f00` of the camera's projection block, +0x40). [inferred] That word is the focal length
    /// `(512 / 0.8) x 0.5 / tan(hfov / 2)` of the camera builder (docs/camera.md "Field of view"); the app's vertical
    /// field of view is `2 atan(0.75 tan(hfov / 2))`, so the focal length is `240 x` the projection's `y_axis.y`.
    #[derive(Clone, Copy, Debug)]
    pub struct ScreenView {
        pub world_to_view: bevy::math::Affine3A,
        pub focal: f32,
    }

    impl ScreenView {
        /// From the app's camera transform and its projection matrix.
        pub fn new(camera: &GlobalTransform, clip_from_view: Mat4) -> ScreenView {
            ScreenView { world_to_view: camera.affine().inverse(), focal: 240.0 * clip_from_view.y_axis.y }
        }

        /// The camera's world position.
        pub fn position(&self) -> Vec3 {
            self.world_to_view.inverse().translation.into()
        }

        /// FUN_0011a6d8's projection of a point (the app's world units): the point in the game camera's axes (x right, y
        /// down, z forward) scaled by `focal / z`. `None` behind the camera (the game divides by the depth whatever its sign;
        /// a point behind it is never in view).
        pub fn project(&self, p: Vec3) -> Option<[f32; 2]> {
            let v = self.world_to_view.transform_point3(p);
            let (x, y, z) = (v.x, -v.y, -v.z);
            (z > 0.0).then(|| [x * self.focal / z, y * self.focal / z])
        }
    }

    /// FUN_0011a6d8(half width, half height, AI, record): the enemy (valid, alive, not out of view: `cands` holds the ones that
    /// pass) whose projected position lies within `half` of the projected aim point (AI +0x40) on both axes, the nearest to
    /// it by squared distance. The record starts empty (FUN_0018ed40), so the first candidate in the box is taken whatever its
    /// distance, and a later one only when strictly nearer. Points are the app's world units.
    pub fn screen_target(view: &ScreenView, aim: Vec3, half: [f32; 2], cands: &[(Entity, Vec3)]) -> Option<Entity> {
        let centre = view.project(aim)?;
        let mut best = 0.0;
        let mut found = None;
        for &(e, p) in cands {
            let Some(s) = view.project(p) else { continue };
            let (dx, dy) = (s[0] - centre[0], s[1] - centre[1]);
            if dx.abs() <= half[0] && dy.abs() <= half[1] {
                let d = dx * dx + dy * dy;
                if found.is_none() || d < best {
                    found = Some(e);
                    best = d;
                }
            }
        }
        found
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
    /// The airborne launch and fall time (hosts 1.100 / 1.128).
    pub air: ai::AirConsts,
    /// `DAT_00378904`: AI +0x24 of Chicken Little's FPS mode (FUN_0011ae60).
    pub fps_camera: f32,
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
            air: ai::AirConsts { gravity: f(0x37a538)?, down_factor: f(0x37a4f0)?, down_divisor: f(0x37a4f4)? },
            fps_camera: f(0x378904)?,
        })
    }
}

/// FUN_002f6320's locomotion motion for an object (WP-SUM8, shared with Genie since WP-SM2): `table` is the class's five
/// motions (Chicken Little and Tinker Bell 0x3716a8, Peter Pan 0x3716c0, Genie 0x3716d8). Only while motion 0 or one of the
/// table's plays is the motion chosen (a motion the script asked for is left alone: `None`); moving (speed above 1) it is
/// the table's first below the mid speed of the walk and run, else its second; standing it is 0, or the third (the stop
/// motion) while the first or second is playing, kept until it finished.
fn locomotion_want(table: &[i32; 5], cur: i32, finished: bool, sp: f32, walk: f32, run: f32) -> Option<i32> {
    if !(cur == 0 || table.contains(&cur)) {
        return None;
    }
    let moving = sp > 1.0;
    let mut want = if moving { if sp >= (walk + run) * 0.5 { table[1] } else { table[0] } } else { 0 };
    if !moving && table[2] >= 0 && (cur == table[0] || cur == table[1]) {
        want = table[2];
    }
    if cur == table[2] && table[2] >= 0 && !finished && !moving {
        want = cur;
    }
    Some(want)
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
    /// Genie's class (WP-SUM5) and the members' follow rules it runs (party_rules, the class 0xf thresholds).
    pub genie: Option<(genie::Consts, Vec<crate::party_battle::Record>)>,
    follow: Option<crate::party_rules::Consts>,
    /// Chicken Little's, Peter Pan's and Tinker Bell's classes (WP-SUM8): the constants and each class's records.
    pub scripted: Option<(scripted::Consts, Vec<scripted::Data>)>,
    /// 03system.bin `pref` `prty` and its row offsets: walk, run and turn of a NeoMoveset (FUN_00194438, as
    /// party.rs reads them).
    prty: Vec<t::PrefPrty>,
    prty_offsets: Vec<u32>,
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
        let genie = exe.as_deref().and_then(|e| Some((genie::Consts::from_exe(e)?, genie::records(e)?)));
        let follow = exe.as_deref().and_then(crate::party_rules::Consts::from_exe);
        let scripted = exe.as_deref().and_then(|e| {
            let all = scripted::CLASSES.iter().map(|c| scripted::Data::from_exe(e, c)).collect::<Option<Vec<_>>>()?;
            Some((scripted::Consts::from_exe(e)?, all))
        });
        if scripted.is_none() {
            warn!("summon: the classes of Chicken Little, Peter Pan and Tinker Bell (FUN_0011a450 / FUN_0011a048 / FUN_0011a230) not readable: they stand where they appear");
        }
        if genie.is_none() || follow.is_none() {
            warn!("summon: Genie's class constants (FUN_001199f8) or the follow rules not readable: Genie stands where he appears");
        }
        let consts = exe.and_then(|exe| SummonConsts::from_exe(&exe).map(|c| (c, kh2_bdx::HostTable::from_exe(&exe))));
        let mut a = disc.0.lock().ok()?;
        let rows = kh2_tables::load::<t::Sumn>(&mut a, &t::SUMN).ok()?.rows;
        let cmds = kh2_tables::load::<Cmd>(&mut a, &t::CMD).ok()?.rows;
        let objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default();
        let sstm = kh2_tables::player::load_sstm(&mut a).ok()?;
        let msg = kh2_tables::player::load_messages(&mut a);
        let prty = kh2_tables::load::<t::PrefPrty>(&mut a, &t::PREF_PRTY).map(|d| d.rows).unwrap_or_default();
        let prty_offsets = prty_offsets(&mut a);
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
            genie,
            follow,
            scripted,
            prty,
            prty_offsets,
            objs,
            host_table,
            names,
            rigs: HashMap::new(),
            scripts: HashMap::new(),
        })
    }

    /// The `prty` row of a NeoMoveset (FUN_001cc140 with fallback row 1, as party.rs's `prty_row`).
    fn prty_row(&self, moveset: u16) -> Option<&t::PrefPrty> {
        crate::party_battle::pref_row(&self.prty_offsets, moveset as usize, 1).and_then(|k| self.prty.get(k))
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
        let rig = self
            .rigs
            .entry(model.clone())
            .or_insert_with(|| {
                if !mset.is_empty() {
                    return crate::drive::read_rig(disc, &model, &mset);
                }
                // An object with no motion set (Chicken Little's P_EX360, objentry type 9): its file holds a script and
                // effects but no model (no entry of kind 4), so the object has no body to draw; one with a model keeps
                // it with no motions, as a weapon.
                let file = disc.read(&format!("obj/{model}.mdlx"))?;
                let m = kh2_model::mdlx(&file).unwrap_or_default();
                let apdx = format!("obj/{model}.apdx");
                let apdx = disc.read(&apdx).is_some().then_some(apdx);
                Some(Arc::new(Rig { model: m, mset: kh2_anim::Mset { slots: Vec::new() }, apdx }))
            })
            .clone();
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

/// The row offsets of 03system.bin `pref` `prty` (the same read as party.rs's: a count, then one offset per row).
fn prty_offsets(a: &mut kh2_archive::Archive) -> Vec<u32> {
    let Ok(file) = a.read("03system.bin") else { return Vec::new() };
    let Some(sub) = kh2_bar::parse(&file).ok().and_then(|top| {
        let pref = kh2_bar::find(&top, "pref")?;
        let inner = kh2_bar::parse(pref.data).ok()?;
        Some(kh2_bar::find(&inner, "prty")?.data.to_vec())
    }) else {
        return Vec::new();
    };
    let w = |o: usize| sub.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let n = w(0).unwrap_or(0) as usize;
    (0..n).map_while(|k| w(4 + 4 * k)).collect()
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
    /// The form command of "Drive!" waiting to reach Genie's script as event 11.
    pub pending_form: Option<u16>,
    /// The enemy the player's latest hit landed on (a stand-in for `0x34e9c4`, FUN_00112850 at his attack's start).
    pub player_hit: Option<Entity>,
    /// Objects the summon scripts made with host 1.0 (`ai::SummonAi::makes`), waiting for `act` to build them: handle and
    /// `00objentry` id.
    kid_makes: Vec<(u32, u32)>,
    /// What the scripts asked of those objects, with the ticks each call has waited for its object.
    kid_ops: Vec<(u32, ai::KidOp, u8)>,
    /// The serial of the last object made (their handles are `KID_BASE + serial`).
    kid_serial: u32,
    /// The leave thread (0x1c6810) runs.
    leaving: bool,
    /// Tinker Bell's revive sequence (`FUN_001ad378` type 2 and its thread `FUN_001ad9a0`).
    revive: Revive,
    /// What the sequence did this tick, for `revive_effects` (music, sound, log).
    revive_acts: Vec<ReviveAct>,
    /// The mode-3 tick counter (for the once-a-second log line).
    ticks: u32,
}

impl SummonState {
    /// The summon timer is non-zero (what limit.rs calls the summon gauge, status +0x1b4 in mode 3).
    pub fn gauge(&self, drive: &DriveState) -> bool {
        drive.gauge_mode == GaugeMode::Summon && drive.timer.left > 0.0
    }

    /// Genie is out: the state is 2 and `FUN_001c6940` (the player-type id of the object at `DAT_01d5e360`) answers his
    /// NeoStatus 0xc9 (`FUN_001bc6e0`'s test for the "Drive!" entry). The summon objects' NeoStatus is looked up among
    /// all of them, not only the first slot [inferred: Genie is a single-summon object].
    pub fn genie_out(&self) -> bool {
        self.state == 2 && self.present.contains(&genie::NEO)
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
    /// Genie's "Drive!" list: the form command chosen (535..538, `Execute` handled by the generic executor FUN_00177c20 ->
    /// FUN_0016b420, which raises event 11 on the first summon object's script with (the player, the command, a target
    /// record)). The command menu issues it (`items.rs`: the second deck's "Drive!" list while Genie is out, `SummonState::genie_out`);
    /// the harness also pushes it (`KH2_GENIE_FORM`).
    Form(u16),
}

#[derive(Resource, Default)]
pub struct SummonQueue(pub Vec<SummonRequest>);

/// The scripted harness.
#[derive(Resource, Default)]
struct Harness {
    summon: Option<(f32, u16)>,
    dismiss_at: Option<f32>,
    /// `KH2_GENIE_FORM=<command>` at `KH2_GENIE_FORM_AT` seconds: the "Drive!" choice.
    form: Option<(f32, u16)>,
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
    /// The object a summon script made with host 1.0: handle and `00objentry` id (at the origin, no summon slot).
    Kid { handle: u32, object: u32 },
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
    /// The movement block the members' follow reads (Genie, WP-SUM5).
    moves: Moves,
    /// Made by another summon object's host 1.0 (Tinker Bell, Chicken Little's surprise): in no summon slot, gone with
    /// the summon.
    kid: bool,
    /// Modes the object that made this one asked for (host 1.8 on this object), switched to once its script runs.
    modes_asked: Vec<String>,
}

/// Object +0x110 walk, +0x114 run, +0x11c turn speed and the body radius +0x5b8.
#[derive(Clone, Copy, Debug, Default)]
struct Moves {
    walk: f32,
    run: f32,
    turn: f32,
    radius: f32,
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
    /// Genie's class constants (the form table of hosts 10.9 / 10.10).
    genie: Option<genie::Consts>,
    /// FUN_001c69b0 (host 2.75): a summon is out, it is Peter Pan's row and byte 7 is set.
    tinker_ready: bool,
    /// The objects host 2.4 walks (FUN_00184ac8: the enemies, +0x588 bit 0x10).
    enemies: Vec<EnemyView>,
    /// The enemies' target parts (host 6.23).
    parts: Vec<crate::friend_host::SearchPart>,
    /// The newest object of the player's lock-on list (host 10.33).
    recent: Option<u32>,
    /// FUN_001903d8 (host 1.244): a battle runs.
    battle: bool,
    /// The enemy of the player's latest landed hit (`SummonState::player_hit`; `0x34e9c4` is the object of his attack).
    player_hit: Option<Entity>,
    /// The constants of Chicken Little's, Peter Pan's and Tinker Bell's classes.
    scripted: Option<scripted::Consts>,
    /// The `00objentry` ids host 1.0 can make for the summon scripts: the second object a `sumn` row preloads (Tinker Bell
    /// for Peter Pan) and the `SpawnObject1..4` of the summon's own objentry row (Chicken Little's 2098).
    kid_ids: Vec<u32>,
    /// The summon state (`DAT_01d5e368`, FUN_001c6918): 1 while the summon arrives.
    summon_state: u8,
    /// The summon's own object: the `sumn` row's Entity1 (an `00objentry` id) and the script handle of the object (host 1.114
    /// finds it by that id: Chicken Little's child binds to him with 1736).
    main: Option<(u32, u32)>,
    /// The airborne launch and fall time numbers (hosts 1.100 / 1.128) and Chicken Little's FPS argument (AI +0x24).
    air: ai::AirConsts,
    fps_camera: f32,
    /// The game camera for host 10.51's projection, and the enemies not out of view (FUN_0019ada8) as `objects` finds them.
    view: Option<fps::ScreenView>,
    in_view: Vec<Entity>,
}

impl SummonEnv {
    fn enemy(&self, h: u32) -> Option<&EnemyView> {
        self.enemies.iter().find(|e| e.handle == h)
    }
}

/// An enemy as the summon scripts' object walk sees it: the handle they hold, its position (game units, the
/// app's axes) and whether a hit reaction plays.
#[derive(Clone, Copy, Debug)]
struct EnemyView {
    entity: Entity,
    handle: u32,
    pos: Vec3,
    reacting: bool,
    /// The enemy is in a blow (`EState::Blown`: the mover with flag 0x40, hosts 10.22 / 10.30).
    blown: bool,
}

/// Host 10.51's search over the app's enemies: the candidates are those not out of view (FUN_0019ada8, `in_view`), each at
/// its target part's point (or its own position); `aim` is the AI's aim point (+0x40) in game units. Answers the enemy found
/// and its script handle (0 for none), the handle host 10.51 returns.
fn screen_search(view: &fps::ScreenView, enemies: &[EnemyView], in_view: &[Entity], parts: &[crate::friend_host::SearchPart], aim: Vec3, half: [f32; 2]) -> (Option<Entity>, u32) {
    let cands: Vec<(Entity, Vec3)> = enemies
        .iter()
        .filter(|e| in_view.contains(&e.entity))
        .map(|e| (e.entity, parts.iter().find(|p| p.entity == e.entity).map_or(e.pos, |p| p.point) * SCALE))
        .collect();
    let found = fps::screen_target(view, aim * SCALE, half, &cands);
    let handle = found.and_then(|f| enemies.iter().find(|e| e.entity == f)).map_or(0, |e| e.handle);
    (found, handle)
}

/// The keyed buffers of host 2.21 (the list at 0x1c95528, FUN_001a5040), kept for the run.
#[derive(Resource, Default)]
pub struct SharedBlocks(pub kh2_bdx::shared::SharedBlocks);

/// The summon object's script host: the party members' host (`FriendHost`: attack objects, effects,
/// target record) with the summon's own group 10 entries and the calls the summon's script makes of the
/// player; motion requests go straight to the enemy host (the summon's motions are not remapped yet: the
/// object's motions are logged, not played, docs/summons.md).
struct SummonHost<'a, 'b> {
    f: crate::friend_host::FriendHost<'a, 'b>,
    ai: &'b mut ai::SummonAi,
    env: &'b SummonEnv,
    shared: &'b mut kh2_bdx::shared::SharedBlocks,
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
            // 1.0 create_object (0x170190 -> FUN_0016e8c0(DAT_0037a484, id, place 0x1c883a0)): the object of objentry `id`
            // at the zeroed place, so at the origin until the script moves it (host 1.148). The objects the summon rows name
            // are made by `act` on the next tick and run their own scripts from then.
            (1, 0) => {
                let id = a(0);
                if self.env.kid_ids.contains(&id) {
                    self.ai.kid_serial += 1;
                    let h = SELF_BASE + (KID_BASE + self.ai.kid_serial) * OBJ_SIZE as u32;
                    self.ai.makes.push((h, id));
                    self.ai.kids.push(h);
                    (Value::Handle(h), St::Approx, "the object appears a tick later (`act`); only the objentry ids the summon's rows name are made")
                } else {
                    (Value::Handle(0), St::Missing, "create_object of an `00objentry` id the summon does not preload")
                }
            }
            // 1.148 set_position (0x170658 -> FUN_0016a618) of an object the script made.
            (1, 148) if a(0) != me && self.ai.kids.contains(&a(0)) => match args.get(1) {
                Some(Arg::Vec(v)) => {
                    self.ai.remote.push((a(0), ai::KidOp::Position(crate::enemy_obj::from_game(*v))));
                    (Value::None, St::Approx, "carried out on the made object once it exists (its position is the point)")
                }
                _ => (Value::None, St::Stub, "set_position without a vector"),
            },
            // 1.277 (0x172030 -> FUN_00178df0): the player's revive: FUN_00178d80 clears the dead bit (+0x7f8 value 4) and gives
            // back his normal mover, FUN_001d4a50 sets the bits FUN_001d49c8 cleared at his death (+0x9a4 bit 0, +0xa70 bit 0, and
            // clears +0x20 bit 0 of the object at +0xb3c), and his HP method (+0x78) adds his full HP (the word at his status block
            // +4). The app sets his HP after the tick (`objects`); the dead bit is `State::Down` and the bits of the +0xb3c object
            // are what `sora.rs` derives from the state (`hittable`), so leaving `State::Down` carries them.
            (1, 277) if a(0) == crate::enemy_obj::PLAYER => {
                self.ai.revive = true;
                (Value::None, St::Approx, "full HP and the stand-up of `sora.rs`; the dead bit, the mover and the +0xb3c object's bits (FUN_00178d80, FUN_001d4a50) are the app's `State::Down` and what it derives from it")
            }
            // 1.18 link_start (0x170dd8 -> FUN_001beb90(child, parent, +0x574, +0x576), then the record's +0xc / +0x10 / +0x14 when
            // a routine is given) with this object as the child and the player as the parent (Tinker Bell's `appear`: halfwords
            // 0x4002 and 0, no routine). The object's parent becomes the player (+0x570), its offset and facing are cleared and it
            // gets the link mover (FUN_0016b698(.., 3)); `objects` places it at the player every tick from then on
            // (`ai::Link::pose`). A link already made is broken first (FUN_001becb8). The routine and its argument run when the
            // link breaks (FUN_001becb8 -> FUN_0016c550).
            (1, 18) if a(0) == me && a(1) == crate::enemy_obj::PLAYER => {
                self.ai.link_start(ai::Link { parent: a(1), kind: a(2) as u16, flags: a(3) as u16, routine: a(4), arg: a(5) });
                (Value::None, St::Approx, "the object follows the player's origin with his facing [inferred from the cleared offsets, the link mover's own update was not read]; the routine runs when the link breaks")
            }
            // 1.102 link_end (0x170e38 -> FUN_001becb8): the link of this object is undone; it stays where it stood.
            (1, 102) if a(0) == me => {
                let was = self.ai.link_end();
                (Value::None, St::Approx, if was { "the link is broken and its routine runs after the tick; the facing set from the object's +0x60 / +0x68 (FUN_003003e8, FUN_0016a7f0) and the cleared +0x7f8 value 8 are not modelled" } else { "no link is made: FUN_001beb00 finds no parent and the call does nothing" })
            }
            // 10.53 (0x2f74c0 -> FUN_0010c5b8(object), FUN_00136388(list, effect)): the effect instance is entered in the effect
            // list at the object's +0xb5c (made on first use, FUN_00135ff8).
            (10, 53) => {
                ai::register_effect(&mut self.ai.effect_list, a(1));
                (Value::None, St::Approx, "the instance is kept in the list of the script that called; the list's readers (the object's end) were not read")
            }
            // 10.54 (0x2f74f8 -> FUN_001364d0): the instance is in the list 10.53 fills.
            (10, 54) => (Value::Bool(self.ai.effect_list.contains(&a(1))), St::Approx, "the list is the calling script's own (see 10.53)"),
            // 6.49 (0x2ee7d8 -> FUN_0019b088): the object named is occupied (see `ai::object_occupied`). Chicken Little's child
            // asks it of each enemy its object walk (2.4) hands out; the answer does not depend on which object it is here (the
            // bits and flags of the other conditions are not modelled), only on the summon state.
            (6, 49) => {
                let v = ai::object_occupied(self.env.summon_state, true, false, false, false);
                (Value::Bool(v), St::Approx, "true while the summon state is 1 (FUN_0019af28); the actor stop bit (+0x518), the object's +0x108 bit 0x400 and the manager at 0x36f6a4 read clear")
            }
            // 1.94 obj_valid of an object the script made.
            (1, 94) if self.ai.kids.contains(&a(0)) => (Value::Bool(true), St::Approx, "an object made by host 1.0 is in the object list"),
            // 1.165 offer_command (0x1734a8): Stitch's main thread offers Ohana (519) in slot 2 while the
            // timer is above 0 (docs/summons.md section 4). Summon limits are stage 5.
            (1, 165) => (Value::None, St::Stub, "summon limit offers are stage 5: the offer is logged, the command menu does not show it"),
            // 1.155 obj_method_78 (0x171c48): the player's virtual method +0x78 = FUN_00186238 (docs/magic.md; the same
            // slot as the enemies') with (amount, part, flag). Peter Pan's script passes (5, 0, 0) next to the effect it
            // starts on the player. A positive amount is added to the HP of part 0 and the result held between its lowest
            // (0) and its maximum (FUN_001a0100); the method returns that HP. The app adds it after the tick.
            (1, 155) if a(0) == crate::enemy_obj::PLAYER && a(2) == 0 => {
                let hp = self.ai.player_hp.0;
                let amount = a(1) as i32;
                if amount <= 0 {
                    (Value::Int(hp), St::Stub, "a negative amount (a hit through the method) is not taken: the summon's scripts only heal [inferred]")
                } else {
                    let new = self.ai.heal_player(amount);
                    (Value::Int(new), St::Approx, "the amount is added to the player's HP and clamped; the revive branch (+0x7f8 bit 4, FUN_0017cee8), the panel's heal event (FUN_0013de10) and the death method are not reproduced")
                }
            }
            // 0.40 (0x16db10): the vector copied raw into the instance's second vector (FUN_001d9d50). Its only
            // other writer, 0.41, stores the angles of a direction there, which the friend host turns into the
            // element's rotation; the angles of this call are taken the same way (pitch, yaw; the third is dropped).
            (0, 40) => match (args.first(), args.get(1)) {
                (Some(h), Some(Arg::Vec(v))) => {
                    let (sp, cp, sy, cy) = (v[0].sin(), v[0].cos(), v[1].sin(), v[1].cos());
                    let dir = Arg::Vec([sy * cp, -sp, cy * cp, 1.0]);
                    let turn = Call::Func { group: 0, index: 41, args: vec![h.clone(), dir] };
                    kh2_bdx::Host::call(&mut self.f, &turn);
                    (Value::None, St::Approx, "the vector is taken as the angles 0.41 would write (pitch, yaw) and becomes the element's rotation; the instance's own use of the vector was not read, the roll is dropped [inferred]")
                }
                _ => (Value::None, St::Stub, "no vector argument"),
            },
            // 2.21 shared_block (0x18e928 -> FUN_001a5040): the buffer registered under the key.
            (2, 21) => (Value::Handle(self.shared.get(a(0) as i32, a(1) as i32)), St::Impl, ""),
            // 2.75 (0x18eba8 -> FUN_001c69b0): Tinker Bell's revive is ready.
            (2, 75) => (Value::Bool(self.env.tinker_ready), St::Impl, ""),
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
            // 10.3 / 10.4 (0x2f6b28 / 0x2f6b58): the AI class's vtable +0x28 / +0x2c, which differs by class: Stitch's
            // FUN_001191a8 / FUN_00119360 (the player and the hands taken into the limit), Chicken Little's FUN_0011ae60 /
            // FUN_0011b008 (the FPS mode), and for Genie, Peter Pan and Tinker Bell FUN_00114170 (the action cleared, the link
            // broken) and a bare return (vtables 0x354910, 0x354978, 0x3549b8).
            (10, 3) => {
                self.ai.link_calls.0 += 1;
                match (self.ai.neo, self.env.stitch.as_ref()) {
                    (stitch::NEO, Some(k)) => {
                        self.ai.stitch_limit_enter(k);
                        (Value::None, St::Approx, "FUN_001191a8: Stitch at the limit's first spot with the stand motion and the callback of the limit sub-commands (host 7.26); the player's linked motion 0xfe, his +0x10c bit 0x200, the parent matrices of the player and the hands, the player's mover 0x3c and the camera are not carried out")
                    }
                    (stitch::NEO, None) => (Value::None, St::Stub, "Stitch's constants are not readable"),
                    (scripted::CHICKEN_NEO, _) => {
                        self.ai.fps_enter(self.env.fps_camera);
                        (Value::None, St::Approx, "FUN_0011ae60: FPS mode on, AI +0x24 / +0x28 set, the object linked to the player (0x4000, 4: at his origin, his facing); the player's +0x108 / +0x10c bits and mover 0x3e, the camera object and mode 9 and the HUD interpolators are not carried out")
                    }
                    _ => {
                        self.ai.base_enter();
                        (Value::None, St::Impl, "")
                    }
                }
            }
            (10, 4) => {
                self.ai.link_calls.1 += 1;
                match self.ai.neo {
                    stitch::NEO => {
                        self.ai.stitch_limit_leave();
                        (Value::None, St::Approx, "FUN_00119360: the link of the limit ended; the player's and the hands' parent matrices, mover and the camera are not carried out")
                    }
                    scripted::CHICKEN_NEO => {
                        self.ai.fps_leave();
                        (Value::None, St::Approx, "FUN_0011b008: FPS mode off, the link broken, the target record emptied; the player's bits and mover, the camera object and the HUD interpolators are not carried out")
                    }
                    _ => (Value::None, St::Impl, "FUN_001141c0 returns at once"),
                }
            }
            // 10.59 (0x2f75c0 -> FUN_0011a6b8): AI +0x40 = the vector (the aim point 10.5 reads).
            (10, 59) => match args.get(1) {
                Some(Arg::Vec(v)) => {
                    self.ai.scripted.aim = *v;
                    (Value::None, St::Impl, "")
                }
                _ => (Value::None, St::Stub, "no vector argument"),
            },
            // 10.51 (0x2f7450 -> FUN_0011a6d8): the enemy in the screen box about the aim point (see `fps::screen_target`). The
            // answer is the enemy's handle (0 for none); kh2-bdx's runtime writes it into the script's record (the object, part 0,
            // 0, -1; as for 6.5), and the search is also kept in `scripted.fps_found` for the app.
            (10, 51) => {
                let halves: Vec<f32> = args.iter().filter_map(|x| if let Arg::Float(f) = x { Some(*f) } else { None }).collect();
                match (self.env.view.as_ref(), halves.as_slice()) {
                    (Some(view), [w, h]) => {
                        let aim = crate::enemy_obj::from_game(self.ai.scripted.aim);
                        let (found, handle) = screen_search(view, &self.env.enemies, &self.env.in_view, &self.env.parts, aim, [*w, *h]);
                        self.ai.scripted.fps_found = found;
                        self.ai.scripted.fps_searches += 1;
                        (Value::Handle(handle), St::Approx, "the enemy found is answered and the runtime writes the record; the camera's focal length is [inferred] from the camera builder, and `in view` is the app's projection test")
                    }
                    _ => (Value::None, St::Stub, "no camera or box in this tick")
                }
            }
            // 2.48 hit_owner (0x18e9c8) and 2.49 attack_word_10 (0x18e9d8) of the harness's limit attack object (`LIMIT_ATTACK`,
            // event 22): the owner is the player and the first participant word is the summon (see the constant).
            (2, 48) if a(0) == LIMIT_ATTACK => (Value::Handle(crate::enemy_obj::PLAYER), St::Approx, "the harness's limit attack object: owner = the player [inferred, `LIMIT_ATTACK`]"),
            (2, 49) if a(0) == LIMIT_ATTACK => (Value::Int(if a(1) == 0 { me as i32 } else { 0 }), St::Approx, "the harness's limit attack object: participant 0 = the summon, the others empty [inferred, `LIMIT_ATTACK`]"),
            // 2.60 attack_f_d4 (0x18eb40): the float at +0xd4 of an attack object. Stitch's event 23 handler asks it of the limit's
            // attack object before it calls 7.26 for a follow-up (525 .. 527); it is the limit task's own field, so the harness's
            // object answers 1 (open) [inferred from the test `> 0` that gates the call].
            (2, 60) if a(0) == LIMIT_ATTACK => (Value::Float(1.0), St::Approx, "the harness's limit attack object: the follow-up window reads open [inferred, `LIMIT_ATTACK`]"),
            // 10.9 (0x2f6c68 -> FUN_00119c18): Genie's form command at AI +0x14, the parts and byte 7. The parts
            // (FUN_00192ce8 / FUN_00192cd0 through object +0x670's vtable +0x40 / +0x3c) are logged, not drawn.
            (10, 9) => match self.env.genie.as_ref() {
                Some(k) => {
                    self.ai.genie.cmd = a(1) as i32;
                    self.ai.genie.form = Some(genie::form(k, a(1) as i32));
                    (Value::None, St::Approx, "AI +0x14 and byte 7 as FUN_00119c18; the model parts it hides and shows (object +0x670) are not drawn")
                }
                None => (Value::None, St::Stub, "Genie's constants are not readable"),
            },
            // 10.10 (0x2f6ca0 -> FUN_00119ca8): the limit command of the form command at AI +0x14.
            (10, 10) => match self.env.genie.as_ref() {
                Some(k) => (Value::Int(genie::limit(k, self.ai.genie.cmd)), St::Impl, ""),
                None => (Value::Int(0), St::Stub, "Genie's constants are not readable"),
            },
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
            // 1.132 target_release (0x173728 -> FUN_0018f458): the record goes back to the pool.
            (1, 132) => (Value::None, St::Approx, "the records are kept for the run"),
            // 1.140 target_valid (0x173740 -> FUN_0018ed58): a record answers when it names an object (and the app's
            // enemy is still there).
            (1, 140) => {
                let src = a(0);
                let valid = if src == crate::enemy_obj::TARGET {
                    self.ai.scripted.target.is_some_and(|t| self.env.enemies.iter().any(|e| e.entity == t))
                } else {
                    self.f.inner.obj.record_object(src).is_some_and(|o| o != 0 && self.env.enemy(o).is_some())
                };
                (Value::Bool(valid), St::Approx, "the record's object is an enemy of the walk")
            }
            // 2.17 attack_hit_object (0x18e6e0): the attack object hits the object named (FUN_0018c5f8, then the object's
            // method +100): the hit is handed to the app with the victim.
            (2, 17) => {
                let (attack, victim) = (a(0), a(1));
                match (self.f.fields.attacks.iter().find(|x| x.handle == attack).map(|x| x.atkp as i16), self.env.enemy(victim)) {
                    (Some(id), Some(_)) => {
                        self.ai.scripted.hits.push((id, victim));
                        (Value::None, St::Approx, "the hit lands on the enemy named: the damage rows are the app's (combat::Hit)")
                    }
                    _ => (Value::None, St::Stub, "the attack object or the victim is not one of the summon's"),
                }
            }
            // 6.23 (0x2ee858 -> FUN_002ee858): the id of the part of the object whose point is nearest to the vector.
            (6, 23) => {
                let v = match args.get(1) {
                    Some(Arg::Vec(v)) => crate::enemy_obj::from_game(*v),
                    _ => Vec3::ZERO,
                };
                let part = self.env.enemy(a(0)).and_then(|e| self.env.parts.iter().filter(|p| p.entity == e.entity).min_by(|x, y| x.point.distance_squared(v).total_cmp(&y.point.distance_squared(v)))).map_or(-1, |p| p.part as i32);
                (Value::Int(part), St::Approx, "the TARGET shapes' ids (group) and centres stand for the collision records of type 2")
            }
            // 10.44 (0x2f7390): the object's mover has flag 0x10 in +8 [role unknown: no enemy of the app is read to have it].
            (10, 44) => (Value::Bool(false), St::Stub, "mover flag 0x10 is not modelled"),
            // 1.88 lockon_point (0x172f40 -> FUN_0016b6e0): the object's lock-on point. An enemy's is the centre of its
            // first target shape (its root without one); the player's and the summon's own is the position.
            (1, 88) => {
                let at = match self.env.enemy(a(0)) {
                    Some(e) => self.env.parts.iter().find(|p| p.entity == e.entity).map_or(e.pos, |p| p.point),
                    None => self.f.inner.obj.motor.pos,
                };
                (Value::Vec(to_game(at)), St::Approx, "the centre of the enemy's first target shape [inferred: the record's point]; the player and the object itself read as their position")
            }
            // 1.192 flag_160_10 (0x171650): object +0x160 bit 0x10 (the controller's flag), which is also bit 4 of the
            // class's free test (host 10.36): the motion allows the next action [inferred]; it reads set.
            (1, 192) if a(0) == me => (Value::Bool(true), St::Approx, "the controller's +0x160 bit 0x10 is not modelled: read set"),
            // 1.83 bone_to_world (0x172e68 -> FUN_0016aa18): the vector through the matrix of the object's bone `i`.
            (1, 83) if a(0) == me => match (args.get(1).map(|x| x.int()), args.get(2)) {
                (Some(i), Some(Arg::Vec(v))) => {
                    let m = usize::try_from(i).ok().and_then(|i| self.ai.scripted.bones.get(i)).copied().unwrap_or(self.ai.scripted.root);
                    let p = m.transform_point3(crate::enemy_obj::from_game(*v)) / SCALE;
                    (Value::Vec(to_game(p)), St::Approx, "the bone's world matrix of the last drawn pose (one tick old)")
                }
                _ => (Value::Vec([0.0; 4]), St::Stub, "no vector argument"),
            },
            // 1.230 (0x174140): float(word 0) / float(word 1) of record `i` of a status block. Record 0 of the player's block
            // (1.84) is his HP part: current over maximum (Chicken Little's `use_potion` test, below 0.25).
            (1, 230) if a(0) == PLAYER_GAUGES && a(1) == 0 => {
                let (hp, max) = self.ai.player_hp;
                (Value::Float(hp as f32 / max as f32), St::Impl, "")
            }
            // 1.244 (0x173ab8 -> FUN_001903d8): a battle runs.
            (1, 244) => (Value::Bool(self.env.battle), St::Impl, ""),
            // 2.4 (0x18df50 -> FUN_00184ac8): the next object of the global list that has +0x588 bit 0x10 (the
            // enemies), after the one given; 0 starts the walk and ends it.
            (2, 4) => {
                let list = &self.env.enemies;
                let next = match a(0) {
                    0 => list.first(),
                    h => list.iter().position(|e| e.handle == h).and_then(|i| list.get(i + 1)),
                };
                (Value::Handle(next.map_or(0, |e| e.handle)), St::Approx, "the walk is over the enemies that are alive and hittable, in the app's order; the game's list also holds objects with +0x588 bit 0x10 the app does not have")
            }
            // The enemies the walk handed out: their position and reaction (hosts 1.147, 2.6, 1.60).
            (1, 147) if self.env.enemy(a(0)).is_some() => {
                let e = self.env.enemy(a(0)).copied();
                (Value::Vec(e.map_or([0.0; 4], |e| to_game(e.pos))), St::Approx, "the enemy's root position")
            }
            (2, 6) | (1, 60) if self.env.enemy(a(0)).is_some() => (Value::Bool(self.env.enemy(a(0)).is_some_and(|e| e.reacting)), St::Approx, "a hurt or blown enemy reads as in a reaction"),
            // 10.5 (0x2f6b88 -> FUN_0011a6a0): AI +0x40, the aim point.
            (10, 5) => (Value::Vec(self.ai.scripted.aim), St::Impl, ""),
            // 10.8 (0x2f6c20 -> FUN_00112538): the 16 bytes at object +0xbe0, the position of the object's target
            // record (FUN_00111d18 copies it every frame with FUN_0018efa0).
            (10, 8) => (Value::Vec(self.ai.scripted.target_pos), St::Approx, "the object's target record is the script's own (1.160); its position is the enemy's root"),
            // 10.17 / 10.18 (0x2f6e30 / 0x2f6e48): object +0x588 bits 0x800 / 0x400. Bit 0x400 is set by the boss
            // constructors (FUN_001acf20); no object of the app has either.
            (10, 17) | (10, 18) => (Value::Bool(false), St::Approx, "bit 0x800 / 0x400 of +0x588 (the boss classes) is set on no object of the app"),
            // 10.20 (0x2f6e80 -> FUN_001102c8): the follow step; the object's +0x10 / +0x20 take the results.
            (10, 20) if a(0) == me => {
                self.ai.scripted.follow_asked = true;
                (Value::None, St::Impl, "the follow step runs after the script's slice, with the steer (FUN_00110468)")
            }
            // 10.22 / 10.30 (0x2f6f60 / 0x2f70e0): the object's mover (+0xc) has flag 0x40 in +8, a blow mover (the object
            // is knocked away: docs/hit-reaction.md); 10.30 also tests +0x10c bit 0x20000 [role unknown: read set].
            (10, 22) | (10, 30) => (Value::Bool(self.env.enemy(a(0)).is_some_and(|e| e.blown)), St::Approx, "an enemy in `EState::Blown` has the blow mover; +0x10c bit 0x20000 (10.30) is read as set"),
            // 10.23 (0x2f6f88): object +0xae4 (the last attacker) is the second object: the player's latest landed hit.
            (10, 23) => {
                let hit = self.env.player_hit;
                (Value::Bool(a(1) == crate::enemy_obj::PLAYER && self.env.enemy(a(0)).is_some_and(|e| Some(e.entity) == hit)), St::Approx, "the last attacker of an enemy is the player when his latest landed hit was on it")
            }
            // 7.24 / 7.25 (0x2ef848 / 0x2ef868 -> FUN_0010f6b8 / FUN_0010f6d0): object +0xb7c bit 0x400 cleared / set. Nothing
            // read in the class reads the bit [GAP: readers of +0xb7c bit 0x400 not searched].
            (7, 24) => {
                self.ai.b7c &= !0x400;
                (Value::None, St::Impl, "")
            }
            (7, 25) => {
                self.ai.b7c |= 0x400;
                (Value::None, St::Impl, "")
            }
            // 7.19 (0x2ef760 -> FUN_0010df08): the target refresh. Peter Pan's chooser (FUN_00119d60) takes, among the
            // objects the player has attacked that are blown (10.30), the nearest; without one and with the join window
            // open (FUN_00119ce8) the object of the player's attack (FUN_0010d848); else the target stays. The answer
            // is whether the target changed (+0xb7c bit 3).
            (7, 19) => {
                let before = self.ai.scripted.target;
                let hit = self.env.player_hit.filter(|h| self.env.enemies.iter().any(|e| e.entity == *h));
                let blown = hit.filter(|h| self.env.enemies.iter().any(|e| e.entity == *h && e.blown));
                let join = self.env.scripted.is_some_and(|k| scripted::join_ready(&k, self.ai.b7c | if self.ai.scripted.joined { 0x200 } else { 0 }, if self.ai.scripted.armed.is_some() { 0 } else { -1 }, 0.0, 0.0));
                let still = before.filter(|b| self.env.enemies.iter().any(|e| e.entity == *b));
                let pick = blown.or(if join { hit } else { None }).or(still);
                self.ai.scripted.target = pick;
                (Value::Bool(pick != before), St::Approx, "the candidates are the player's latest hit (the game's list: objects with +0x10c bit 0x20000, mover flag 0x40, last attacker the player); the surface distance order is not needed with one")
            }
            // 10.25 / 10.26 (0x2f6fe8 / 0x2f7020): FUN_00119ce8 on +0xb7c, and the mark 0x200 (FUN_0011a1d0).
            (10, 25) => match self.env.scripted {
                Some(k) => (Value::Bool(scripted::join_ready(&k, self.ai.b7c | if self.ai.scripted.joined { 0x200 } else { 0 }, if self.ai.scripted.armed.is_some() { 0 } else { -1 }, 0.0, 0.0)), St::Approx, "the player's attack window (DAT_0034e9c8..d0, FUN_00112850 at the attack's start) is the app's: his attack state, the motion that started it"),
                None => (Value::Bool(false), St::Stub, "the class constants are not readable"),
            },
            (10, 26) => {
                if self.ai.scripted.armed.is_some() && !self.ai.scripted.joined {
                    self.ai.scripted.joined = true;
                    self.ai.b7c |= 0x200;
                }
                (Value::None, St::Approx, "the mark is kept; FUN_0011a1d0 tests 10.25 first")
            }
            // 10.27 / 10.28 (0x2f7070 / 0x2f7090 -> FUN_0010f6e0 / FUN_0010f728): +0xb7c bit 0x4000 cleared / set (the
            // class is neither 0x20 nor 0x1f, whose branch is a motion request).
            (10, 27) => {
                self.ai.b7c &= !0x4000;
                (Value::None, St::Impl, "")
            }
            (10, 28) => {
                self.ai.b7c |= 0x4000;
                (Value::None, St::Impl, "")
            }
            // 10.29 (0x2f70b0 -> FUN_0010c9d0): the item's command on the target, motion request 9.
            (10, 29) if a(0) == me => {
                self.ai.scripted.item = Some((a(1) as i32, a(2)));
                (Value::None, St::Approx, "motion 9 plays and the item's effect lands on the player after trigger 6 and the wait (`ItemUse`); the item's effect on another target, the use effects and the user's Item Boost are not carried out")
            }
            // 10.31 (0x2f7118): +0x588 bit 0x400000: the summon objects.
            (10, 31) => (Value::Bool(a(0) == me), St::Impl, ""),
            // 10.33 / 10.34 (0x2f7170 / 0x2f71d0): the player's list of recent lock-on targets (0x34e988).
            (10, 33) => {
                let h = self.env.recent;
                if a(0) != 0 && h.is_some() {
                    self.ai.scripted.recent_taken = true;
                }
                (Value::Handle(h.filter(|_| !self.ai.scripted.recent_taken).unwrap_or(0)), St::Approx, "the list is the player's current lock-on or target; FUN_00112850's other conditions (the object type, the player's state) are not read")
            }
            (10, 34) => {
                self.ai.scripted.recent_taken = true;
                (Value::None, St::Approx, "the list is emptied up to and including the object")
            }
            // 10.36 / 10.37 (0x2f7230 / 0x2f7270): object +0x160 bits 4, 8, 0x10 (free) / 0x40000000.
            (10, 37) => (Value::Bool(false), St::Approx, "+0x160 bit 0x40000000 is set on no object of the app"),
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
        // The calls on an object this script made with host 1.0: carried out on it when it exists (`objects`).
        let made = |o: u32, ai: &ai::SummonAi| ai.kids.contains(&o);
        let remote = match c {
            Call::RemoteSetMode { obj, name } if made(*obj, self.ai) => Some((*obj, ai::KidOp::Mode(name.clone()), "a mode asked of the object the script made (host 1.8, FUN_00186db0)")),
            Call::Hide { obj } if made(*obj, self.ai) => Some((*obj, ai::KidOp::Hide, "host 1.28 on the object the script made")),
            Call::SetFlag { obj, bit } if made(*obj, self.ai) => Some((*obj, ai::KidOp::SetFlag(*bit), "host 1.70 on the object the script made")),
            Call::ClearFlag { obj, bit } if made(*obj, self.ai) => Some((*obj, ai::KidOp::ClearFlag(*bit), "host 1.71 on the object the script made")),
            _ => None,
        };
        if let Some((o, op, why)) = remote {
            self.ai.remote.push((o, op));
            self.record(c, &kh2_bdx::Value::None, crate::enemy_host::St::Approx, why);
            return kh2_bdx::Value::None;
        }
        // The queries on the enemies the object walk (2.4) handed out: their position and reaction.
        match c {
            // 1.114 bind_object (0x171d10 -> FUN_0016b078(id)): the object whose id is `id`. Chicken Little's child (P_EX360) binds
            // to the object of the `sumn` row's Entity1 (1736, his own), the one object of the summon the app has.
            Call::FindObject { by: kh2_bdx::host::ObjectKey::Entity, value } if self.env.main.is_some_and(|(id, _)| id as i32 == *value) => {
                let v = kh2_bdx::Value::Handle(self.env.main.map_or(0, |(_, h)| h));
                self.record(c, &v, crate::enemy_host::St::Impl, "");
                return v;
            }
            Call::Position { obj } if self.env.enemy(*obj).is_some() => {
                let v = kh2_bdx::Value::Vec(self.env.enemy(*obj).map_or([0.0; 4], |e| to_game(e.pos)));
                self.record(c, &v, crate::enemy_host::St::Approx, "the enemy's root position");
                return v;
            }
            Call::Busy { obj } | Call::HitStun { obj } if self.env.enemy(*obj).is_some() => {
                let v = kh2_bdx::Value::Bool(self.env.enemy(*obj).is_some_and(|e| e.reacting));
                self.record(c, &v, crate::enemy_host::St::Approx, "a hurt or blown enemy reads as in a reaction");
                return v;
            }
            Call::OnScreen { obj } if self.env.enemy(*obj).is_some() => {
                let v = kh2_bdx::Value::Bool(true);
                self.record(c, &v, crate::enemy_host::St::Stub, "the frustum test of enemies is not made: they read as on screen");
                return v;
            }
            _ => {}
        }
        // 1.131 free_target (0x1736f8 -> FUN_0018f3f8): a new target record from the pool (0x1c89bd0, 0x14 bytes each),
        // a copy of the record given. The copy names the same object: the object's own target (`TARGET`), or the
        // object another copy names.
        if let Call::FreeTarget { handle } = c {
            let obj = if *handle == crate::enemy_obj::TARGET {
                self.ai.scripted.target.and_then(|t| self.env.enemies.iter().find(|e| e.entity == t)).map_or(0, |e| e.handle)
            } else {
                self.f.inner.obj.record_object(*handle).unwrap_or(0)
            };
            let v = kh2_bdx::Value::Handle(self.f.inner.obj.new_record(obj, 0));
            self.record(c, &v, crate::enemy_host::St::Approx, "the pool is the object's own list of records (kept for the run); a record in the script's own memory is copied as an empty one");
            return v;
        }
        // 1.120 target_position (0x173688) of a record: the point of its object (the nearest part's point of the enemy).
        if let Call::TargetPosition { target } = c {
            let obj = if *target == crate::enemy_obj::TARGET {
                self.ai.scripted.target.and_then(|t| self.env.enemies.iter().find(|e| e.entity == t)).map(|e| e.handle)
            } else {
                self.f.inner.obj.record_object(*target)
            };
            if let Some(e) = obj.and_then(|o| self.env.enemy(o)).copied() {
                let at = self.env.parts.iter().find(|p| p.entity == e.entity).map_or(e.pos, |p| p.point);
                let v = kh2_bdx::Value::Vec(to_game(at));
                self.record(c, &v, crate::enemy_host::St::Approx, "the centre of the enemy's first target shape (its root without one) [inferred: the record's point]");
                return v;
            }
        }
        // The raw-word calls on target records: 1.160 (FUN_0018f490, the third word is the context's object, see the
        // runtime's dispatch) makes a record name an object, 6.16 (FUN_0018ed40) clears one. The record is the
        // object's own target (`TARGET`, the app's `ScriptedAi::target`) or one of its pool (1.131).
        if let Call::Unknown { group, index, args } = c
            && let Some(&rec) = args.first()
            && (rec == crate::enemy_obj::TARGET || self.f.inner.obj.record_object(rec).is_some())
        {
            let own = rec == crate::enemy_obj::TARGET;
            match (*group, *index) {
                (1, 160) => {
                    let named = args.get(2).copied().unwrap_or(0);
                    let (st, why) = if own {
                        match self.env.enemy(named).map(|e| e.entity) {
                            Some(e) => {
                                self.ai.scripted.target = Some(e);
                                (crate::enemy_host::St::Approx, "the object's target record names the enemy (shape 0); the position index and the part are not kept")
                            }
                            None => (crate::enemy_host::St::Stub, "the object named is not an enemy of the walk: the record is left as it is")
                        }
                    } else {
                        self.f.inner.obj.set_record_object(rec, named);
                        (crate::enemy_host::St::Impl, "")
                    };
                    self.record(c, &kh2_bdx::Value::None, st, why);
                    return kh2_bdx::Value::None;
                }
                (6, 16) => {
                    if own {
                        self.ai.scripted.target = None;
                    } else {
                        self.f.inner.obj.set_record_object(rec, 0);
                    }
                    self.record(c, &kh2_bdx::Value::None, crate::enemy_host::St::Impl, "");
                    return kh2_bdx::Value::None;
                }
                _ => {}
            }
        }
        // 1.100 (0x173178 -> FUN_0017bee0; the table gives it no name or signature, so it arrives as raw words): sqrt(2 x /
        // DAT_0037a538) of the float word, the time a body takes to fall x. 1.128 (0x171298, `MoverMethod`): the object's mover,
        // vtable +0xc, with one float: for the mover FUN_00179868 gives new objects (kind 0, vtable 0x35cbb0) that is
        // FUN_001796e8 -> FUN_0017b1e0 (the airborne launch) and the mover kind 1 (FUN_0017b1b0).
        let gravity = self.env.air.gravity;
        match c {
            Call::Unknown { group: 1, index: 100, args } if gravity != 0.0 => {
                let v = kh2_bdx::Value::Float(ai::fall_time(f32::from_bits(args.first().copied().unwrap_or(0)), gravity));
                self.record(c, &v, crate::enemy_host::St::Impl, "");
                return v;
            }
            // 7.26 (0x2ef888 -> FUN_00112518(object, command): the callback at the object's +0xcfc; the table gives it no
            // signature, so the arguments arrive as raw words: the object's record, then the command). Stitch's FUN_001191a8
            // sets the callback (FUN_00119488); it runs after the script's tick (`objects`).
            Call::Unknown { group: 7, index: 26, args } if args.len() >= 2 => {
                self.ai.limit_cmd = Some(args[1] as i32);
                let v = kh2_bdx::Value::Int(0);
                self.record(c, &v, crate::enemy_host::St::Approx, "the dispatcher runs after the script's tick; its result is not read by the scripts (they pop it)");
                return v;
            }
            // 1.305 (0x173e80): the game camera's position (FUN_001071c8 of FUN_001068c0(1)) at a fixed address, returned as a
            // pointer: Stitch's Ohana start faces the camera and places himself from it. 7.0 (0x2ef2f0 -> FUN_00184b40(id)):
            // the actor stop `id` released on every enemy (the same walk as host 2.4); the app sets no stop id but 2 (the
            // arrival), so releasing another (Ohana's start asks 17) changes nothing.
            Call::Unknown { group: 1, index: 305, .. } if self.env.view.is_some() => {
                let at = self.env.view.map_or(Vec3::ZERO, |v| v.position() / SCALE);
                let v = kh2_bdx::Value::Vec(to_game(at));
                self.record(c, &v, crate::enemy_host::St::Approx, "the camera's world position (the app's camera transform)");
                return v;
            }
            Call::Unknown { group: 7, index: 0, .. } => {
                self.record(c, &kh2_bdx::Value::None, crate::enemy_host::St::Impl, "the app sets no actor stop id but 2");
                return kh2_bdx::Value::None;
            }
            Call::MoverMethod { obj, value } if *obj == self.f.inner.obj.handle && gravity != 0.0 => {
                self.ai.air = Some(ai::launch(*value, &self.env.air));
                self.record(c, &kh2_bdx::Value::None, crate::enemy_host::St::Approx, "FUN_0017b1e0's fields (+0xd0..+0xdc) and the mover kind 1 are recorded; the mover's own update (FUN_0017b328 / FUN_0017b440: the flight) is not ported, so the object does not fly [GAP]");
                return kh2_bdx::Value::None;
            }
            _ => {}
        }
        if let Some((v, st, why)) = self.own(c) {
            self.record(c, &v, st, why);
            return v;
        }
        self.f.call(c)
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        // The object's own target record (host 2.23: object +0x9f4), laid out like the records of event 11 (object,
        // position index, 0, -1, 0): its object is the enemy the script named (1.160), or none.
        let t = addr.wrapping_sub(crate::enemy_obj::TARGET) as usize;
        if t + out.len() <= 20 {
            let obj = self.ai.scripted.target.and_then(|t| self.env.enemies.iter().find(|e| e.entity == t)).map_or(0, |e| e.handle);
            let mut r = crate::enemy_host::target_record();
            r[0..4].copy_from_slice(&obj.to_le_bytes());
            out.copy_from_slice(&r[t..t + out.len()]);
            return true;
        }
        self.shared.read(addr, out) || self.f.read_mem(addr, out)
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        // The own target record is the app's (`ScriptedAi::target`): writes into it are taken and dropped.
        if addr.wrapping_sub(crate::enemy_obj::TARGET) as usize + data.len() <= 20 {
            return true;
        }
        self.shared.write(addr, data) || self.f.write_mem(addr, data)
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
            form: std::env::var("KH2_GENIE_FORM").ok().and_then(|v| v.trim().parse::<u16>().ok()).map(|c| (env_f("KH2_GENIE_FORM_AT").unwrap_or(12.0), c)),
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
            .init_resource::<SharedBlocks>()
            .init_resource::<PlayerSide>()
            .init_resource::<crate::hud::hud_party::SummonFace>()
            .add_systems(Update, (keys.before(crate::magic::harness_keys), harness_requests, hud_feed.before(crate::hud::hud_gauge::update), face_feed.before(crate::hud::hud_party::update)))
            .add_systems(FixedUpdate, (track_player_hits, tick, act, objects, revive_effects, companions).chain().in_set(Sim::Sounds))
            .add_systems(FixedUpdate, revive_pad.before(Sim::Sora))
            .add_systems(FixedUpdate, stops.after(Sim::Enemy).before(Sim::Detect).run_if(crate::schedule::sim_running));
    }
}

/// The summon face panel's request (`hud_party::SummonFace`): while the gauge is in mode 3 and a summon object holds party
/// slot 0 the panel shows its face (FUN_00156ca8 waits for the object, FUN_00156fe8 starts group 0x9a / 0x9b); when the gauge
/// leaves mode 3 the panel plays group 0x9c (FUN_00156c78 -> FUN_00157020).
fn face_feed(drive: Res<DriveState>, objects: Query<&SummonObject>, mut face: ResMut<crate::hud::hud_party::SummonFace>) {
    let want = (drive.gauge_mode == GaugeMode::Summon).then(|| objects.iter().find(|o| o.slot == 0).map(|o| o.model.clone())).flatten();
    if face.model != want {
        face.model = want;
    }
}

/// The enemy the player hit last (a stand-in for `DAT_0034e9c4`, written by FUN_00112850 at the start of his attack).
fn track_player_hits(mut hits: MessageReader<Hit>, sora: Query<(), With<Sora>>, enemies: Query<(), With<Enemy>>, objects: Query<&SummonObject>, h: Res<Harness>, mut sm: ResMut<SummonState>) {
    for hit in hits.read() {
        if sora.contains(hit.attacker) && enemies.contains(hit.target) {
            sm.player_hit = Some(hit.target);
        }
        if let Ok(o) = objects.get(hit.attacker)
            && h.log
        {
            info!("summon: {} hits {:?} with atkp {} (the body's attack volume)", o.tag, hit.target, hit.atkp);
        }
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
    if let Some((at, c)) = h.form
        && crossed(at)
    {
        queue.0.push(SummonRequest::Form(c));
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

/// The summon start (`FUN_001c6168`) ends with `FUN_001d76f8(FUN_001d4ca0(command))`: the BATTLE REPORT's use counter of the
/// summon's 1-based row index (1 to 4; `Progress::count_summon_use`).
fn count_summon_call(progress: Option<&mut Progress>, index: u8) {
    if let Some(p) = progress {
        p.count_summon_use(index as usize);
    }
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
        if !o.kid {
            sm.present.push(o.neo);
        }
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
                count_summon_call(progress.as_deref_mut(), index);
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
            SummonRequest::Form(c) => {
                // Only while Genie is out (the menu lists "Drive!" in that state: FUN_001bc6e0 with state 2 and NeoStatus 201).
                if sm.genie_out() {
                    sm.pending_form = Some(c);
                    info!("summon: Drive! chose command {c} (event 11 for Genie)");
                } else {
                    info!("summon: Drive! command {c} refused: Genie is not out (state {})", sm.state);
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
type ActRes<'w> = (ResMut<'w, SummonState>, Res<'w, Battle>, Option<Res<'w, World>>, Option<ResMut<'w, crate::fx::EffectLib>>);
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
    (mut sm, battle, world, mut lib): ActRes,
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
        sm.kid_makes.clear();
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
    for (handle, object) in std::mem::take(&mut sm.kid_makes) {
        acts.0.push(Act::Kid { handle, object });
    }
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
            Act::Spawn { .. } | Act::Kid { .. } => {
                let (index, mode, skip, kid) = match a {
                    Act::Spawn { index, mode, skip } => (index, mode, skip, None),
                    Act::Kid { handle, object } => (0, "", false, Some((handle, object))),
                    _ => unreachable!(),
                };
                let (object_id, label) = match kid {
                    Some((_, id)) => (id, "made by host 1.0".to_string()),
                    None => {
                        let Some(row) = data.rows.get(index as usize - 1).cloned() else { continue };
                        (row.entity1, data.name(row.command))
                    }
                };
                let Some(o) = data.objs.iter().find(|r| r.object_id == object_id).cloned() else {
                    warn!("summon: object {object_id} of summon {index} has no objentry row");
                    continue;
                };
                let (Some(rig), prog) = data.object(&disc, &o) else {
                    warn!("summon: object: {} model or motion set {} not loadable", o.model_name, o.animation_name);
                    continue;
                };
                // A made object takes no summon slot (the slots' handles and the leave thread are the summon's own).
                let slot = if kid.is_some() {
                    2
                } else {
                    let Some(slot) = (0..2u8).find(|&k| sm.slots[k as usize].is_none()) else { continue };
                    slot
                };
                // The object's own effect file (`obj/<model>.apdx`) is registered with the effect library so its script's
                // effects (1.87, 1.149, 1.157) can start.
                if let Some(l) = lib.as_deref_mut() {
                    crate::drive::register_effects(&disc, l, &rig);
                }
                // FUN_0016b960: the player's position; without the event the free spot (FUN_001a5ea8).
                let mut at = if kid.is_some() { Vec3::ZERO } else { s.motor.pos };
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
                let yaw = if kid.is_some() { 0.0 } else { s.motor.yaw };
                let root = commands.spawn((Transform::from_translation(at * SCALE).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default())).id();
                let body = spawn_skinned(&mut commands, root, rig.clone(), &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
                let idle = rig.mset.slot(0, kh2_anim::PlayerState::FieldNoWeapon).or_else(|| rig.mset.has(0).then_some(0));
                let handle = kid.map_or(SELF_BASE + (HANDLE_BASE + slot as u32) * OBJ_SIZE as u32, |(h, _)| h);
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
                if kid.is_none() {
                    sm.slots[slot as usize] = Some(body);
                    sm.present.push(o.neo_status);
                }
                let slots: Vec<usize> = (0..rig.mset.slots.len()).filter(|&x| rig.mset.has(x)).collect();
                info!("summon: object {} motion set slots {slots:?}", o.model_name);
                info!(
                    "summon: object {} ({label}, objentry {}, type {}, NeoStatus {}) in summon slot {slot} at {at:?}, script {}, mode {mode:?}; body motion slot {idle:?}",
                    o.model_name,
                    o.object_id,
                    o.object_type,
                    o.neo_status,
                    script.as_deref().unwrap_or("none")
                );
                if let Ok(mut b) = anims.get_mut(body) {
                    b.play(idle, true, 0.0);
                }
                // The motions' attack triggers are generic (the game evaluates them for any object): the body can hit the
                // enemies, and nothing hits it (the summon takes no damage: FUN_001c60f8 drains the gauge instead).
                commands.entity(root).insert(Fighter { team: crate::combat::Team::Player, attack_bodies: vec![body], hurt_body: body, landed: Default::default(), hittable: false });
                commands.entity(root).insert(SummonObject {
                    neo: o.neo_status,
                    model: o.model_name.clone(),
                    body,
                    slot,
                    rt,
                    obj,
                    tag: if kid.is_some() { format!("{}#kid", o.model_name) } else { format!("{}#summon{slot}", o.model_name) },
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
                    kid: kid.is_some(),
                    modes_asked: Vec::new(),
                    moves: {
                        // +0x110 walk, +0x114 run, +0x11c turn: the `prty` row of the NeoMoveset (FUN_00194438); the
                        // body radius +0x5b8 from the model's first shape of kind 0 or 4 (35 without), as party.rs.
                        let p = data.prty_row(o.neo_moveset);
                        let radius = rig.model.shapes.iter().find(|s| s.kind == 0 || s.kind == 4).map_or(35.0, |s| s.radius.abs());
                        Moves { walk: p.map_or(0.0, |p| p.walk_speed), run: p.map_or(0.0, |p| p.run_speed), turn: p.map_or(0.0, |p| p.turn_speed), radius }
                    },
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

/// What the classes of Chicken Little, Peter Pan and Tinker Bell reach beside Stitch's: the effect library, the victims'
/// hit memory of their attack objects, the effect hits and the running effects.
type ScriptedRes<'w, 's> = (
    Option<Res<'w, crate::fx::EffectLib>>,
    Local<'s, crate::friend_ai::VictimMemory>,
    MessageWriter<'w, crate::fx::SpellHit>,
    Query<'w, 's, &'static mut crate::fx::RunningEffect>,
    Option<Res<'w, crate::items::ItemsData>>,
);

/// The game state the summon objects read: the game data, the drive gauge, the limit task, the summon data and state.
type SummonRes<'w> = (Res<'w, GameData>, Res<'w, DriveState>, Option<Res<'w, crate::limit::LimitTask>>, Res<'w, SummonData>, ResMut<'w, SummonState>, ResMut<'w, SharedBlocks>);

/// FUN_001c69b0: the summon state is not 0, byte 5 is the `sumn` row of Peter Pan's command (0x132,
/// FUN_001d4ca0: 0 when absent) and byte 7 is set (1 from the start, FUN_001c6168; cleared by the revive
/// FUN_001adeb8 through FUN_001c6a10).
fn tinker_ready(sm: &SummonState, data: &SummonData) -> bool {
    sm.state != 0 && sm.index == data.index_of(cmd::PETER_PAN).unwrap_or(0) && sm.extra != 0
}

/// The `00objentry` ids host 1.0 can make for the summon of `sumn` index `index` (1-based, 0 for none): the second object its
/// row preloads (`Entity2`: Tinker Bell for Peter Pan) and the `SpawnObject1..4` of the objentry row of its own object
/// (Chicken Little's 2098, `P_EX360`).
fn kid_object_ids(rows: &[t::Sumn], objs: &[t::Objentry], index: u8) -> Vec<u32> {
    let Some(row) = (index != 0).then(|| rows.get(index as usize - 1)).flatten() else { return Vec::new() };
    let own = objs.iter().find(|r| r.object_id == row.entity1);
    let named = own.map_or([0; 4], |o| [o.spawn_object1, o.spawn_object2, o.spawn_object3, o.spawn_object4]);
    std::iter::once(row.entity2).chain(named.into_iter().map(u32::from)).filter(|&i| i != 0).collect()
}

/// What Tinker Bell's revive sequence does, in the order the game does it (`FUN_001ad378` type 2, then the thread
/// `FUN_001ad9a0`, then `FUN_001adeb8`). The acts are carried out by `revive_effects` and `revive_pad`; the state machine
/// itself needs neither audio nor a world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviveAct {
    /// `DAT_0035d268` bit 1 set (`FUN_001ad378`); cleared again by `FlagsClear`.
    FlagsSet,
    /// `FUN_00137520`: the music state is kept and cleared (0x354ebc -> 0x354ebe), the channel ramps `FUN_002c1a08` and the
    /// BGM stream stop `FUN_00136a68` (0x1e0).
    MusicStop,
    /// `FUN_002c14a8(0x13, 0)`: system sound 0x13 (type 2).
    Sound(u32),
    /// `FUN_00195898` / `FUN_00195a78` / `FUN_00195ae0` would open the message window; its text is not found (the argument is
    /// the player object by the decompile), so none is opened.
    WindowSkipped,
    /// `FUN_00138708(2)`: `DAT_00354f70` bit 2 set (cleared by `FUN_001ad790`, `FieldReleased`).
    FieldHeld,
    /// Step 1 of the thread: `FUN_002c38b8` sets `DAT_0036f598`, which stops `FUN_002c3228` handing out pad events.
    PadLock,
    /// `FUN_002c38c8` clears it again.
    PadRelease,
    /// `FUN_00183fe0`: the player is put on the floor point under him.
    PlayerToFloor,
    /// `FUN_001adeb8`: byte 7 cleared (`FUN_001c6a10`) and Tinker Bell made.
    TinkerMade,
    /// `FUN_001adeb8`: `DAT_0035d268` bit 1 cleared.
    FlagsClear,
    /// `FUN_001adeb8` -> `FUN_00137558`: the kept music state back, the area's music started again.
    MusicRestart,
    /// The thread ends (`FUN_001ad960` -> `FUN_001ad790`): `DAT_00354f70` bit 2 cleared.
    FieldReleased,
}

/// The revive thread (`FUN_001ad9a0`, state word +8) with the object's fields it needs.
#[derive(Debug, Clone, Default)]
pub struct Revive {
    /// `Some(step)` while the thread runs: 0 the first tick, 1 the pad lock, 2 the waits.
    step: Option<u8>,
    /// `00objentry` id of Tinker Bell, made when the thread is done.
    tinker: u32,
}

impl Revive {
    /// The thread runs.
    pub fn running(&self) -> bool {
        self.step.is_some()
    }

    /// The pad is locked (`DAT_0036f598` set in step 1, cleared when the waits are over).
    pub fn pad_locked(&self) -> bool {
        self.step.is_some_and(|s| s >= 2)
    }

    /// `FUN_001ad378` with type 2, started by `FUN_001cec00` -> `FUN_001ad8a8` when Tinker Bell is ready and the player is
    /// down: everything it does before the thread's first tick.
    pub fn begin(&mut self, tinker: u32) -> Vec<ReviveAct> {
        self.step = Some(0);
        self.tinker = tinker;
        vec![ReviveAct::FlagsSet, ReviveAct::MusicStop, ReviveAct::Sound(0x13), ReviveAct::WindowSkipped, ReviveAct::FieldHeld]
    }

    /// One tick of the thread (`FUN_001ad9a0`): case 0 moves on, case 1 locks the pad and falls into case 2, which waits
    /// while the message window exists (`FUN_00196418`) or the fade object 0x356b30 is not idle (+0xfc != 4), then lets go
    /// of the pad, puts the player on the floor and finishes with `FUN_001adeb8` (type 2).
    pub fn tick(&mut self, window_open: bool, fade_idle: bool) -> Vec<ReviveAct> {
        let mut acts = Vec::new();
        match self.step {
            None => {}
            Some(0) => self.step = Some(1),
            Some(1) => {
                acts.push(ReviveAct::PadLock);
                self.step = Some(2);
                self.wait(window_open, fade_idle, &mut acts);
            }
            Some(_) => self.wait(window_open, fade_idle, &mut acts),
        }
        acts
    }

    fn wait(&mut self, window_open: bool, fade_idle: bool, acts: &mut Vec<ReviveAct>) {
        if window_open || !fade_idle {
            return;
        }
        acts.extend([
            ReviveAct::PadRelease,
            ReviveAct::PlayerToFloor,
            ReviveAct::TinkerMade,
            ReviveAct::FlagsClear,
            ReviveAct::MusicRestart,
            ReviveAct::FieldReleased,
        ]);
        self.step = None;
    }
}

/// Whether the knock-out starts Tinker Bell's revive: Peter Pan is out with byte 7 set (`ready`, FUN_001c69b0), the player lies
/// down (`State::Down`) with no HP.
fn revive_due(ready: bool, down: bool, hp: i32) -> bool {
    ready && down && hp <= 0
}

/// The summon's own object as host 1.114 finds it: the Entity1 of its `sumn` row (`index` is 1-based, 0 for none) with the
/// script handle of the object in the summon slot (`None` while there is none).
fn main_object(rows: &[t::Sumn], index: u8, handle: Option<u32>) -> Option<(u32, u32)> {
    let row = (index != 0).then(|| rows.get(index as usize - 1)).flatten()?;
    Some((row.entity1, handle?))
}

/// `KH2_SUMMON_KO=<seconds>`: true once, from that many seconds after the first call on, while a summon is out.
fn ko_due(summon_out: bool) -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};
    static SPEC: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    static DONE: AtomicBool = AtomicBool::new(false);
    let start = *START.get_or_init(std::time::Instant::now);
    let Some(at) = *SPEC.get_or_init(|| std::env::var("KH2_SUMMON_KO").ok()?.trim().parse().ok()) else { return false };
    summon_out && start.elapsed().as_secs_f32() >= at && !DONE.swap(true, Ordering::Relaxed)
}

/// The attack object of a summon limit as `KH2_SUMMON_EVENT` hands it to event 22 (`22:limit`): the limit task's own object
/// is not ported, so hosts 2.48 / 2.49 answer for this one handle only what the scripts read of it: its owner (word +0xc,
/// the player) and its first participant (word +0x10, the summon). [inferred] from event 22's handlers, which store those
/// two as the limit's player and summon records (`ctx + 4`, `ctx + 0x14`) and link the second to the first.
const LIMIT_ATTACK: u32 = 0x7a00_0000;

/// `KH2_SUMMON_EVENT=<event>:<argument>[@<seconds>][,<event>:<argument>[@<seconds>]...]` (a harness aid; a missing time is
/// the one before, 12 s for the first): each event is delivered once, while a summon is out, in order, to the summon's
/// script, as the limit task's script would deliver events 22 (a limit starts, with its attack object: `22:limit`, whose
/// handler starts the limit's own command) and 23 (a sub-command, FUN_001b28a0's first branch, docs/summons.md): `22:limit`
/// starts Chicken Little's FPS mode, Peter Pan's Never Land or Stitch's Ohana; `23:525` .. `23:527` are Stitch's follow-ups. The
/// limit task body is not ported, so nothing else delivers them. [inferred] The receiver is the summon's script.
fn event_due(summon_out: bool) -> Vec<(u32, u32)> {
    static SPEC: std::sync::OnceLock<Vec<(u32, u32, f32)>> = std::sync::OnceLock::new();
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    static DONE: std::sync::Mutex<Vec<bool>> = std::sync::Mutex::new(Vec::new());
    let start = *START.get_or_init(std::time::Instant::now);
    let spec = SPEC.get_or_init(|| {
        let Ok(v) = std::env::var("KH2_SUMMON_EVENT") else { return Vec::new() };
        let mut at = 12.0;
        v.split(',')
            .filter_map(|p| {
                let (what, t) = p.split_once('@').map_or((p, None), |(w, t)| (w, t.trim().parse::<f32>().ok()));
                at = t.unwrap_or(at);
                let (e, a) = what.split_once(':')?;
                Some((e.trim().parse().ok()?, if a.trim() == "limit" { LIMIT_ATTACK } else { a.trim().parse().ok()? }, at))
            })
            .collect()
    });
    let Ok(mut done) = DONE.lock() else { return Vec::new() };
    done.resize(spec.len(), false);
    let now = start.elapsed().as_secs_f32();
    let mut due = Vec::new();
    for (k, &(e, a, at)) in spec.iter().enumerate() {
        if summon_out && !done[k] && now >= at {
            done[k] = true;
            due.push((e, a));
        }
    }
    due
}

/// Carries out the revive sequence's acts that touch the music, the sounds and the log (`FUN_00137520`, `FUN_00137558`,
/// `FUN_002c14a8`). The music stop and restart are logged with `KH2_NO_MUSIC` / `KH2_NO_SOUND` too.
fn revive_effects(
    mut sm: ResMut<SummonState>,
    mut music: Option<ResMut<crate::audio_bgm::AreaMusic>>,
    audio: Option<Res<crate::audio::Audio>>,
    mut se: MessageWriter<crate::audio::PlaySe>,
) {
    for act in std::mem::take(&mut sm.revive_acts) {
        match act {
            ReviveAct::FlagsSet => info!("summon: revive: flag 0x35d268 bit 1 set (FUN_001ad378)"),
            ReviveAct::MusicStop => {
                info!("summon: revive: music stops (FUN_00137520: state kept, FUN_00136a68 stream stop 0x1e0)");
                if let Some(m) = music.as_deref_mut() {
                    crate::audio_bgm::revive_stop(m, audio.as_deref());
                }
            }
            ReviveAct::Sound(n) => {
                info!("summon: revive: system sound {n:#x} (FUN_002c14a8)");
                se.write(crate::audio::PlaySe::system(n));
            }
            ReviveAct::WindowSkipped => info!("summon: revive: the message window (FUN_00195898 / FUN_00195a78 / FUN_00195ae0) is not opened: its text is not found"),
            ReviveAct::FieldHeld => info!("summon: revive: 0x354f70 bit 2 set (FUN_00138708)"),
            ReviveAct::PadLock => info!("summon: revive: pad locked (FUN_002c38b8)"),
            ReviveAct::PadRelease => info!("summon: revive: pad released (FUN_002c38c8)"),
            ReviveAct::PlayerToFloor => info!("summon: revive: the player is put on the floor (FUN_00183fe0; sora.rs's ground contact)"),
            ReviveAct::TinkerMade => {}
            ReviveAct::FlagsClear => info!("summon: revive: flag 0x35d268 bit 1 cleared (FUN_001adeb8)"),
            ReviveAct::MusicRestart => {
                info!("summon: revive: music restarts (FUN_00137558: state back, area music started again)");
                if let Some(m) = music.as_deref_mut() {
                    crate::audio_bgm::revive_restart(m, audio.as_deref());
                }
            }
            ReviveAct::FieldReleased => info!("summon: revive: 0x354f70 bit 2 cleared (FUN_001ad790)"),
        }
    }
}

/// While the revive thread holds the pad (`DAT_0036f598`, no pad events handed out) the pad reads as untouched.
fn revive_pad(sm: Res<SummonState>, pad: Option<ResMut<crate::sora::Pad>>) {
    if sm.revive.pad_locked()
        && let Some(mut pad) = pad
    {
        pad.lock();
    }
}

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
    (game, drive, limit_task, data, mut sm, mut shared): SummonRes,
    (cam, enemies, gts, mut hits, mut magic, battle): StitchWorld,
    (lib, mut memory, mut spell_hits, mut running, item_data): ScriptedRes,
) {
    let (Some(mut trace), Ok(mut s)) = (trace, sora.single_mut()) else { return };
    let paired = HashMap::new();
    // Host 1.0: the objects the summon's rows name (Peter Pan's second object, `SpawnObject1..4` of the summon's own row).
    let kid_ids = kid_object_ids(&data.rows, &data.objs, sm.index);
    // `KH2_SUMMON_EVENT` (a harness aid): one event for the summon's own script.
    let mut aid_event = event_due(sm.state != 0);
    // `KH2_SUMMON_KO=<seconds>` (a harness aid): once, that many seconds after the start, while a summon is out, the player is
    // knocked out (HP 0, `State::Down`) so Tinker Bell's revive can be seen without waiting for an enemy to do it.
    if ko_due(sm.state != 0) {
        info!("summon: KH2_SUMMON_KO: the player is knocked out (HP {} -> 0)", s.hp);
        s.hp = 0;
        s.enter(State::Down, None);
    }
    // Tinker Bell's revive (FUN_001cec00 -> FUN_001ad8a8 -> FUN_001ad378 type 2): Peter Pan is out with byte 7 set and the
    // player is knocked out. `Revive` runs the sequence: the music stops and the pad locks, and when its waits are over
    // FUN_001adeb8 clears byte 7 (FUN_001c6a10), makes Tinker Bell (objentry 0x4f4, the row's Entity2) at the player and
    // sets her mode `reraise` (the string at 0x37b880); her script raises the player (host 1.277). The message window's
    // text is not found, so no window is opened, and the app has no 0x356b30 fade to wait for (docs/summons.md).
    let mut acts = Vec::new();
    if sm.revive.running() {
        acts = sm.revive.tick(false, true);
    } else if revive_due(tinker_ready(&sm, &data), matches!(s.state, State::Down), s.hp)
        && let Some(id) = kid_ids.first().copied()
    {
        acts = sm.revive.begin(id);
        info!("summon: the player is knocked out with Tinker Bell ready (FUN_001c69b0): the revive sequence starts (FUN_001ad378 type 2)");
    }
    for &act in &acts {
        if act == ReviveAct::TinkerMade {
            let id = sm.revive.tinker;
            sm.extra = 0;
            sm.kid_serial += 1;
            let handle = SELF_BASE + (KID_BASE + sm.kid_serial) * OBJ_SIZE as u32;
            sm.kid_makes.push((handle, id));
            sm.kid_ops.push((handle, ai::KidOp::Position(s.motor.pos), 0));
            sm.kid_ops.push((handle, ai::KidOp::Mode("reraise".to_string()), 0));
            info!("summon: byte 7 cleared (FUN_001c6a10), Tinker Bell (objentry {id}) made at the player in mode \"reraise\" (FUN_001adeb8)");
        }
    }
    sm.revive_acts.extend(acts);
    // The calls the scripts made on the objects they made, last tick: carried out now that those objects exist (a call
    // for an object not built yet waits up to `KID_PATIENCE` ticks).
    let summon_out = sm.slots.iter().all(Option::is_none);
    let kid_handles: Vec<u32> = q.iter().filter(|(_, _, so)| so.kid).map(|(_, _, so)| so.obj.handle).collect();
    for (h, op, age) in std::mem::take(&mut sm.kid_ops) {
        let Some((_, mut tf, mut so)) = q.iter_mut().find(|(_, _, so)| so.obj.handle == h) else {
            if age + 1 < KID_PATIENCE {
                sm.kid_ops.push((h, op, age + 1));
            }
            continue;
        };
        let o = &mut so.obj;
        match op {
            ai::KidOp::Position(p) => {
                o.motor.pos = p;
                tf.translation = p * SCALE;
            }
            ai::KidOp::Mode(m) => {
                info!("summon: {} asked for mode {m:?} by the script that made it", so.tag);
                so.modes_asked.push(m);
            }
            ai::KidOp::Hide => {
                o.hidden = true;
                o.alpha = 0.0;
                o.fade = None;
            }
            ai::KidOp::SetFlag(bit) if (0..64).contains(&bit) => o.flags |= 1 << bit,
            ai::KidOp::ClearFlag(bit) if (0..64).contains(&bit) => o.flags &= !(1 << bit),
            ai::KidOp::SetFlag(_) | ai::KidOp::ClearFlag(_) => {}
        }
    }
    let move_record = data.stitch.as_ref().and_then(|(_, r)| r.iter().position(|r| stitch::addr(r) == stitch::MOVE));
    // The object walk of host 2.4 (FUN_00184ac8: the objects with +0x588 bit 0x10, the enemies) and the enemies'
    // hurt shapes the summons' attack objects are tested against.
    let views: Vec<EnemyView> = enemies
        .iter()
        .filter(|(_, e, f)| e.alive() && f.hittable)
        .filter_map(|(te, e, _)| {
            Some(EnemyView {
                entity: te,
                handle: crate::friend_host::enemy_handle(te),
                pos: gts.get(te).ok()?.translation() / SCALE,
                reacting: matches!(e.state, crate::enemy::EState::Hurt | crate::enemy::EState::Blown),
                blown: matches!(e.state, crate::enemy::EState::Blown),
            })
        })
        .collect();
    let victims: Vec<_> = enemies
        .iter()
        .filter_map(|(e, _, f)| {
            let f = (f.team == crate::combat::Team::Enemy && f.hittable).then_some(f)?;
            let (a, g) = (anims.get(f.hurt_body).ok()?, gts.get(f.hurt_body).ok()?);
            let shapes = a.rig.model.shapes.iter().filter(|sh| sh.kind == kh2_model::collision::kind::BATTLE || sh.kind == kh2_model::collision::kind::TARGET);
            Some((e, shapes.map(|sh| (crate::combat::shape_volume(sh, a, g), crate::combat::hurt_spheroid(sh, a, g))).collect::<Vec<_>>()))
        })
        .collect();
    let parts: Vec<crate::friend_host::SearchPart> = enemies
        .iter()
        .filter_map(|(e, _, f)| {
            let f = (f.team == crate::combat::Team::Enemy && f.hittable).then_some(f)?;
            let (a, g) = (anims.get(f.hurt_body).ok()?, gts.get(f.hurt_body).ok()?);
            let shapes = a.rig.model.shapes.iter().filter(|sh| sh.kind == kh2_model::collision::kind::BATTLE || sh.kind == kh2_model::collision::kind::TARGET).enumerate();
            Some(
                shapes
                    .filter(|(_, sh)| sh.kind == kh2_model::collision::kind::TARGET)
                    .map(|(i, sh)| {
                        let h = crate::combat::hurt_spheroid(sh, a, g);
                        crate::friend_host::SearchPart { entity: e, shape: i, part: sh.group, point: h.center / SCALE, radius: h.radius / SCALE }
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .flatten()
        .collect();
    for m in memory.0.values_mut() {
        m.tick();
    }
    let pax_of = |rig: &Arc<Rig>| lib.as_deref().and_then(|l| l.for_rig(rig)).cloned();
    let mut senv = SummonEnv {
        timer: drive.timer.left,
        limit: limit_task.is_some_and(|l| l.0.is_some()),
        stitch: data.stitch.as_ref().map(|d| d.0),
        move_record,
        genie: data.genie.as_ref().map(|d| d.0),
        tinker_ready: tinker_ready(&sm, &data),
        enemies: views.clone(),
        parts: parts.clone(),
        battle: battle.active(),
        player_hit: sm.player_hit,
        recent: s.lock.or(s.target).and_then(|e| views.iter().find(|v| v.entity == e)).map(|v| v.handle),
        scripted: data.scripted.as_ref().map(|d| d.0),
        kid_ids,
        summon_state: sm.state,
        main: main_object(&data.rows, sm.index, q.iter().find(|(_, _, so)| !so.kid).map(|(_, _, so)| so.obj.handle)),
        air: data.consts.map(|c| c.air).unwrap_or_default(),
        fps_camera: data.consts.map_or(0.0, |c| c.fps_camera),
        view: None,
        in_view: Vec::new(),
    };
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
            let vols = crate::combat::hurt_volumes(a, body);
            let p = vols.iter().map(|v| (v.a + v.b) * 0.5).min_by(|x, y| x.distance_squared(cg.translation()).total_cmp(&y.distance_squared(cg.translation()))).unwrap_or(body.translation());
            let ndc = cc.world_to_ndc(cg, p)?;
            (ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z)).then_some((te, p))
        })
        .collect();
    // Host 10.51's projection (FUN_0011a6d8) and the enemies it counts as in view (FUN_0019ada8, as above).
    senv.view = camera.map(|(cc, cg)| fps::ScreenView::new(cg, cc.clip_from_view()));
    senv.in_view = targets.iter().map(|t| t.0).collect();
    let senv = senv;
    for (ent, mut tf, mut so) in &mut q {
        let so = &mut *so;
        let Ok(anim) = anims.get(so.body) else { continue };
        let rig = anim.rig.clone();
        so.obj.slot = anim.slot;
        so.obj.frame = anim.frame;
        let motion_done = anim.finished;
        // Host 1.14 (bit 1 of +0x160): the motion ended, or a looping one wrapped (as `enemy_ai`, `friend_ai`).
        so.obj.done = anim.slot.is_some() && (anim.finished || anim.looping && anim.frame < anim.prev_frame);
        so.obj.trigger_bits = crate::friend_ai::trigger_bits(anim);
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
        let (neo, body, is_kid) = (so.neo, so.body, so.kid);
        let so_model = so.model.clone();
        // The objects a summon's script made go with the summon (their maker's object is in a summon slot).
        if so.kid && summon_out {
            info!("summon: {} removed with its summon", so.tag);
            commands.entity(ent).despawn();
            continue;
        }
        let SummonObject { rt, obj, tag, started, leave, mode, errors_seen, ai, fields, blast_hit, ticks, moves, modes_asked, .. } = so;
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
        // The made objects still there (host 1.94) and the serial the next one takes.
        ai.kids.retain(|h| kid_handles.contains(h));
        ai.kid_serial = sm.kid_serial;
        let pax = pax_of(&rig);
        if let (Ok(a), Ok(g)) = (anims.get(body), gts.get(body)) {
            ai.scripted.bones = (0..a.pose.len()).map(|i| a.bone_world(g, i)).collect();
            ai.scripted.root = g.to_matrix();
        }
        // The class's values the script reads through the party host (as `friend_ai` fills them for a member).
        let target_view = ai.scripted.target.and_then(|t| views.iter().find(|v| v.entity == t));
        ai.scripted.target_pos = target_view.map_or([0.0; 4], |v| to_game(v.pos));
        fields.free = true;
        fields.target_valid = target_view.is_some();
        fields.target_point = target_view.map(|v| v.pos);
        fields.target_radius = ai.scripted.target.and_then(|t| parts.iter().find(|p| p.entity == t)).map_or(0.0, |p| p.radius);
        // The player's attack the member may join (FUN_00112850 at the attack's start): his attack state and the motion
        // that started it.
        let player_slot = anims.get(s.body).ok().and_then(|a| a.slot);
        if matches!(s.state, State::Attack { .. }) {
            if ai.scripted.armed != player_slot {
                ai.scripted.armed = player_slot;
                ai.scripted.joined = false;
                ai.b7c &= !0x200;
            }
        } else {
            ai.scripted.armed = None;
        }
        fx_before(fields, pax.as_ref(), body, s.body, &anims.as_readonly(), &gts, &mut running);
        let inner = BevyHost { obj, env: &env, trace: &mut trace, tag: tag.as_str() };
        let f = crate::friend_host::FriendHost { inner, fields, game: &game, free: None, targets: &parts };
        ai.player_hp = (s.hp, s.stats.max_hp);
        ai.player_heal = 0;
        ai.neo = neo;
        let mut host = SummonHost { f, ai, env: &senv, shared: &mut shared.0 };
        if let Some(m) = started.take() {
            rt.spawn(&mut host, handle);
            if !m.is_empty() {
                rt.set_mode_named(&mut host, m);
            }
        }
        // Host 1.8 on this object by the script that made it (Tinker Bell's `appear`).
        for m in std::mem::take(modes_asked) {
            rt.set_mode_named(&mut host, &m);
        }
        if std::mem::take(leave) {
            rt.set_mode_named(&mut host, "leave");
        }
        // Chicken Little's, Peter Pan's and Tinker Bell's classes (WP-SUM8): the update's decision before the script's
        // slice: out of battle the follow, in battle the record that starts the script's `battle` mode once no mode runs.
        let mut pre = None;
        if let Some(cls) = scripted::class_of(neo)
            && let Some((_, classes)) = data.scripted.as_ref()
            && let Some(sd) = classes.iter().find(|c| c.class.neo == neo)
            && rt.current_mode().as_deref() != Some("leave")
        {
            host.ai.scripted.follow_asked = false;
            host.ai.scripted.item = None;
            host.ai.scripted.init = true;
            let now = scripted::Now { battle: battle.active(), mode_running: rt.current_mode().is_some() };
            let out = scripted::decide(&sd.records, cls.class, &mut host.ai.scripted, &now);
            if let Some((i, name)) = &out.start {
                if h.log {
                    info!("summon: {tag} record {i} (handler {:#x}) starts script mode {name:?} (FUN_00186e70)", scripted::START_MODE[0]);
                }
                rt.set_mode_priority(&mut host, name);
            }
            pre = Some((*cls, out));
        }
        // Genie's "Drive!" (FUN_00177c20 -> FUN_0016b420): event 11 with (the player, the form command, a target record).
        if neo == genie::NEO
            && let Some(cmd) = sm.pending_form.take()
        {
            let rec = host.f.inner.obj.new_record(crate::enemy_obj::PLAYER, 0);
            let handled = rt.event(&mut host, 11, &[crate::enemy_obj::PLAYER, cmd as u32, rec]);
            info!("summon: {tag} event 11 (Drive!: form command {cmd}): handler {}", if handled { "ran" } else { "absent" });
        }
        if !is_kid && !aid_event.is_empty() {
            for (event, arg) in std::mem::take(&mut aid_event) {
                let handled = rt.event(&mut host, event, &[arg]);
                info!("summon: {tag} KH2_SUMMON_EVENT: event {event} with {arg:#x}: handler {}", if handled { "ran" } else { "absent" });
            }
        }
        rt.update(&mut host);
        // The routine of a link that broke this tick (host 1.102, or 1.18 over a link): FUN_001becb8 runs it with the
        // argument registered by 1.18 (FUN_0016c550).
        for link in std::mem::take(&mut host.ai.link_broken) {
            info!("summon: {tag} link to the player broken (FUN_001becb8): routine R{:04x} runs with argument {}", link.routine, link.arg);
            rt.call_routine(&mut host, link.routine, &[link.arg]);
        }
        // What the script made and asked of the objects it made (host 1.0, 1.148, 1.8, 1.28, 1.70): `act` builds them and the
        // next tick carries the calls out.
        sm.kid_serial = sm.kid_serial.max(ai.kid_serial);
        ai.kid_serial = sm.kid_serial;
        sm.kid_makes.append(&mut ai.makes);
        sm.kid_ops.extend(ai.remote.drain(..).map(|(h, op)| (h, op, 0)));
        if let Some((before, after)) = ai.apply_heal(&mut s.hp) {
            info!("summon: {tag} healed the player (host 1.155): HP {before} -> {after}");
        }
        // Host 1.277 (Tinker Bell's `reraise`): the player's HP method adds his full HP (FUN_00178df0), after the dead bit is
        // cleared; a player who is down also stands up (the app's own stand-up after a knock-out, `sora.rs`, `State::Down`).
        if std::mem::take(&mut ai.revive) {
            let down = matches!(s.state, State::Down);
            let (hp, down) = ai::SummonAi::revive_player(s.hp, s.stats.max_hp, down);
            info!("summon: {tag} raised the player (host 1.277, FUN_00178df0): HP {} -> {hp}, {}", s.hp, if down { "he was down and stands up" } else { "he was not down" });
            s.hp = hp;
            if down {
                s.invuln = 120.0;
                s.state = State::Hurt;
                let pstate = if battle.active() { kh2_anim::PlayerState::BattleWeapon } else { kh2_anim::PlayerState::FieldNoWeapon };
                if let Ok(mut b) = anims.get_mut(s.body) {
                    let slot = b.rig.mset.slot(kh2_anim::id::DAMAGE_BLOW_RECOV, pstate);
                    b.restart(slot, false, 6.0);
                }
            }
        }
        let now = rt.current_mode();
        if now != *mode {
            info!("summon: {tag} script mode {:?} -> {:?}", mode.as_deref().unwrap_or("-"), now.as_deref().unwrap_or("-"));
            *mode = now;
        }
        let requested = obj.motion_req.is_some();
        if let Some(req) = obj.motion_req.take() {
            if neo == stitch::NEO {
                if h.log {
                    info!("summon: {tag} script asked for motion {} (not applied: the class's own requests drive the body)", req.slot);
                }
            } else if let Ok(mut a) = anims.get_mut(body) {
                // The script's own motion request (hosts 1.11 / 1.12 / 1.13): the slot is the motion number (FUN_0017da30
                // returns the number for these motion sets, as for the enemies), played at once with its blend.
                let looping = rig.mset.get(req.slot).and_then(|x| x.motion.as_ref()).is_some_and(|mo| mo.frame_return < mo.frame_end);
                if h.log {
                    info!("summon: {tag} script motion {} (blend {}, looping {looping})", req.slot, req.blend);
                }
                a.restart(Some(req.slot), looping, req.blend);
                if let Some(f) = req.start {
                    a.frame = f;
                    a.prev_frame = f;
                }
            }
        }
        // Host 1.13 queued a motion after the running one (FUN_0017e170): it follows when that one ends.
        if let Some(q) = fields.queued.take() {
            ai.scripted.queued = Some(q);
        } else if obj.done
            && !requested
            && let Some(q) = ai.scripted.queued.take()
            && let Ok(mut a) = anims.get_mut(body)
        {
            let looping = rig.mset.get(q).and_then(|x| x.motion.as_ref()).is_some_and(|mo| mo.frame_return < mo.frame_end);
            a.restart(Some(q), looping, 0.0);
        }
        // Stitch's class (FUN_001155e0 and its records, WP-SUM4b) while he is not leaving.
        let mut placed = None;
        if neo == stitch::NEO
            && let Some((k, recs)) = data.stitch.as_ref()
            && rt.current_mode().as_deref() != Some("leave")
        {
            *ticks += 1;
            // Host 7.26 (the limit sub-command dispatcher FUN_00119488) ran in the script this tick.
            let limit_cmd = ai.limit_cmd.take();
            let st = &mut ai.stitch;
            let now = stitch::Now { battle: battle.active(), limit: senv.limit, hp: (s.hp, s.stats.max_hp), mp: (s.mp, game.sora_max_mp), in_view: targets.len(), motion_done };
            let mut out = stitch::tick(k, recs, st, &now);
            if let Some(cmd) = limit_cmd {
                let lo = stitch::limit_tick(k, recs, st, &now, cmd);
                info!("summon: {tag} limit sub-command {cmd} (host 7.26, FUN_00119488): {}", lo.started.map_or("no record for it".to_string(), |(i, _)| format!("record {i} (handler {:#x}) starts", stitch::addr(&recs[i]))));
                out.merge(lo);
            }
            if let Some(m) = out.player_motion {
                info!("summon: {tag} the player is linked to motion {m:#x} of Stitch's limit (FUN_0017f160): not carried out (the player's motions are sora.rs's)");
            }
            if out.heal {
                let before = s.hp;
                s.hp = crate::magic_rules::healed(s.hp, s.stats.max_hp, 1);
                info!("summon: {tag} heals the player (FUN_001155e0's tail, the player's method +0x78 with 1): HP {before} -> {}", s.hp);
            }
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
                // Stitch's motion set holds his motions at their own numbers (slots 0, 0xa1, 0xc9..0xce, 0xe7, 0xf1,
                // 0xf2 in N_HB040_BTL's set): FUN_0017da30 uses the four-slot order only for a motion set whose header
                // +0xc has bit 0 (FUN_0017d960 -> FUN_001a7d70), else the number is the slot. [inferred] Stitch's set
                // has the bit clear (the header bit is not read here): a slot at the number wins, the order is the
                // fallback.
                let state = crate::party_battle::motion_state(battle.active(), false);
                let slot = if rig.mset.has(m as usize) { Some(m as usize) } else { crate::party_battle::remap(m as i32, order[state], |x| rig.mset.has(x)) };
                let looping = slot.and_then(|x| rig.mset.get(x)).and_then(|x| x.motion.as_ref()).is_some_and(|mo| mo.frame_return < mo.frame_end);
                if h.log && (a.slot != slot || slot.is_none()) {
                    let near: Vec<usize> = (0..rig.mset.slots.len()).filter(|&x| rig.mset.has(x)).collect();
                    info!("summon: {tag} motion request {m:#x} -> slot {slot:?} (looping {looping}){}", if slot.is_none() { format!("; the motion set has slots {near:?}") } else { String::new() });
                }
                // A request of the motion that plays changes nothing; a new one starts (FUN_0017e220).
                if slot.is_some() {
                    a.play(slot, looping, 0.0);
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
        // Genie's class (WP-SUM5): the members' update FUN_00111070 with Genie's two records, while he is not leaving.
        if neo == genie::NEO
            && let (Some((k, recs)), Some(fc)) = (data.genie.as_ref(), data.follow.as_ref())
            && rt.current_mode().as_deref() != Some("leave")
        {
            use crate::party_rules as pr;
            let g = &mut ai.genie;
            // FUN_001112e8's first update: the object's vtable +0x94 -> FUN_0010c630 -> the AI's +0x18 (FUN_00119a40):
            // byte 7's form command (FUN_001c6b28) through FUN_00119c18.
            if !g.init {
                g.init = true;
                g.cmd = genie::start_command(k, sm.extra);
                g.form = Some(genie::form(k, g.cmd));
                info!("summon: {tag} first update (FUN_00119a40): byte 7 = {} -> form command {} (FUN_001c6b10)", sm.extra, g.cmd);
            }
            if let Some(f) = g.form.take() {
                sm.extra = f.byte7;
                info!("summon: {tag} form command {} (FUN_00119c18): parts {:?} hidden, part {:?} shown (not drawn), byte 7 = {}, limit command {}", g.cmd, f.hidden, f.shown, f.byte7, genie::limit(k, g.cmd));
            }
            *ticks += 1;
            if g.using && motion_done {
                g.using = false;
            }
            if !g.using {
                let now = genie::Now { battle: battle.active(), hp: (s.hp, s.stats.max_hp) };
                let out = genie::decide(k, recs, g, &now);
                if let Some((i, urgent)) = out.started {
                    info!("summon: {tag} record {i} (handler {:#x}{}) starts: player HP {}/{}", stitch::addr(&recs[i]), if urgent { ", the emergency pass FUN_0010f8d8" } else { "" }, now.hp.0, now.hp.1);
                }
                let (pos, player) = (obj.motor.pos, s.motor.pos);
                let facing = obj.motor.forward();
                let t = fc.thresholds(genie::CLASS);
                let mv = *moves;
                let to_player = player - pos;
                let ground_dir = to_player.with_y(0.0).normalize_or_zero();
                let (mut dir, mut speed) = (facing, 0.0);
                if out.follow {
                    // FUN_001102c8: the follow point (FUN_00111670, party slot 0: the summon is the only member) and the
                    // follow step (FUN_0010e5e8) with the row of the battle state, then the class 0xf speed rule.
                    let d = pr::follow_distance(&t, mv.radius, fc);
                    let point = pr::follow_points(player, s.motor.yaw, [Some((pos, d)), None], fc)[0].unwrap_or(player);
                    let input = pr::FollowIn { to_point: point - pos, to_player: to_player.normalize_or_zero(), dist: to_player.length(), ground_dir, facing, walk: mv.walk, run: mv.run };
                    let row = if now.battle { pr::ROW_BATTLE_PLAYER } else { pr::ROW_FIELD };
                    (dir, speed) = pr::follow_step_row(&mut g.follow, &input, &t, row, fc);
                    speed = genie::follow_speed(k, speed, mv.walk, s.motor.vel.length());
                }
                if out.potion {
                    // FUN_0010c9d0 -> FUN_00175e20: speed mode 0, the potion command on the player, motion 9.
                    g.follow.mode = 0;
                    info!(
                        "summon: {tag} record 1 (FUN_00119b28): the command of item {} on the player (FUN_0010c9d0, motion {}); the effect lands on the player after trigger 6 and the wait",
                        genie::POTION_ITEM,
                        genie::M_USE
                    );
                    if let Ok(mut a) = anims.get_mut(body) {
                        let slot = if rig.mset.has(genie::M_USE as usize) { Some(genie::M_USE as usize) } else { data.motion_orders.and_then(|o| crate::party_battle::remap(genie::M_USE as i32, o[crate::party_battle::motion_state(true, false)], |x| rig.mset.has(x))) };
                        if h.log {
                            info!("summon: {tag} motion request {:#x} -> slot {slot:?}", genie::M_USE);
                        }
                        if slot.is_some() {
                            a.play(slot, false, 0.0);
                        }
                        // [inferred] Not free until motion 9 ends, as the members' item use.
                        g.using = slot.is_some();
                        if let Some(sl) = slot {
                            ai.scripted.using = Some(scripted::ItemUse { item: genie::POTION_ITEM, slot: sl, left: None });
                        }
                    }
                }
                // FUN_00110468: the steer.
                let (d, sp) = pr::steer(&mut g.follow, dir, speed, mv.walk, mv.run, fc);
                let sp = if out.potion { 0.0 } else { sp };
                let delta = d.with_y(0.0).normalize_or_zero() * sp;
                // [inferred] Genie flies (+0xb7c 0x800): no ground probe; he keeps the player's height.
                obj.motor.pos = (pos + delta).with_y(player.y);
                if delta != Vec3::ZERO {
                    obj.motor.turn_to(delta, mv.turn);
                } else if g.follow.turning {
                    obj.motor.turn_to(ground_dir, mv.turn);
                }
                // FUN_002f6320 with his table (0x3716d8): the follow's motion, while motion 0 or one of the table's plays (the
                // potion's motion 9 and a motion the script asked for are left alone).
                if !requested
                    && !out.potion
                    && let Ok(mut a) = anims.get_mut(body)
                    && let Some(want) = locomotion_want(&k.loco, a.slot.map_or(0, |c| c as i32), a.finished, sp, mv.walk, mv.run)
                {
                    let state = crate::party_battle::motion_state(battle.active(), false);
                    let slot = if want >= 0 && rig.mset.has(want as usize) { Some(want as usize) } else { data.motion_orders.and_then(|o| crate::party_battle::remap(want, o[state], |x| rig.mset.has(x))) };
                    if let Some(sl) = slot
                        && a.slot != Some(sl)
                    {
                        let looping = rig.mset.get(sl).and_then(|x| x.motion.as_ref()).is_some_and(|mo| mo.frame_return < mo.frame_end);
                        if h.log {
                            info!("summon: {tag} locomotion motion {want} -> slot {sl} (speed {sp:.1}, walk {:.1} run {:.1})", mv.walk, mv.run);
                        }
                        a.play(Some(sl), looping, 8.0);
                    }
                }
                if h.log && *ticks % 60 == 1 {
                    info!(
                        "summon: {tag} follow: {:.0} units from the player, speed mode {} speed {sp:.1} (walk {:.1} run {:.1}), record {:?}, battle {}",
                        (obj.motor.pos - player).length(),
                        g.follow.mode,
                        mv.walk,
                        mv.run,
                        g.choosing.current,
                        now.battle
                    );
                }
            }
        }
        // Chicken Little's, Peter Pan's and Tinker Bell's classes: the object's target, the update's steer and the
        // locomotion motion (WP-SUM8).
        if let Some((cls, pre_out)) = pre
            && let (Some((k, classes)), Some(fc)) = (data.scripted.as_ref(), data.follow.as_ref())
            && let Some(sd) = classes.iter().find(|c| c.class.neo == neo)
        {
            use crate::party_rules as pr;
            *ticks += 1;
            let sa = &mut ai.scripted;
            // The target record the script made (1.160, or a target search 6.5).
            if let Some(e) = fields.retarget.take() {
                if sa.target != Some(e) && h.log {
                    info!("summon: {tag} target record now names {e:?} (1.160, FUN_0018f490)");
                }
                sa.target = Some(e);
            }
            if let Some((found, walked)) = fields.searched.take() {
                if h.log {
                    info!("summon: {tag} target search (6.5): {walked} parts walked, {}", found.map_or("none found".to_string(), |(e, i)| format!("{e:?} shape {i}")));
                }
                sa.target = found.map(|(e, _)| e);
            }
            if sa.target.is_some_and(|t| !views.iter().any(|v| v.entity == t)) {
                // FUN_00111d18: a record that no longer answers (FUN_00112b28) is cleared.
                sa.target = None;
            }
            let (pos, player) = (obj.motor.pos, s.motor.pos);
            let facing = obj.motor.forward();
            // FUN_0011a2e8 (Chicken Little): the aim point, while the FPS flag (AI +0x20 bit 0) is clear.
            if neo == scripted::CHICKEN.neo && !sa.fps {
                let tgt = ai_target_pos(sa.target, &views);
                sa.aim = scripted::aim_point(k, tgt, to_game(pos), [facing.x, -facing.y, -facing.z, 0.0]);
            }
            let t = fc.thresholds(cls.class);
            let mv = *moves;
            let to_player = player - pos;
            let ground_dir = to_player.with_y(0.0).normalize_or_zero();
            let (mut dir, mut speed) = (facing, 0.0);
            let follow = pre_out.follow || sa.follow_asked;
            if follow {
                // FUN_001102c8: the follow point (FUN_00111670, party slot 0: the summon is the only member) and the
                // follow step (FUN_0010e5e8) with the row of the battle state.
                let d = pr::follow_distance(&t, mv.radius, fc);
                let point = pr::follow_points(player, s.motor.yaw, [Some((pos, d)), None], fc)[0].unwrap_or(player);
                let input = pr::FollowIn { to_point: point - pos, to_player: to_player.normalize_or_zero(), dist: to_player.length(), ground_dir, facing, walk: mv.walk, run: mv.run };
                let row = if battle.active() { pr::ROW_BATTLE_PLAYER } else { pr::ROW_FIELD };
                (dir, speed) = pr::follow_step_row(&mut sa.follow, &input, &t, row, fc);
                speed = scripted::follow_speed(cls.class, k, speed, mv.walk, s.motor.vel.length());
            }
            // FUN_00110468: the steer.
            let (d, sp) = pr::steer(&mut sa.follow, dir, speed, mv.walk, mv.run, fc);
            let delta = d.with_y(0.0).normalize_or_zero() * sp;
            // The script's own movement (1.111 set_move, one tick).
            let mut new = pos + delta + obj.move_vel;
            if cls.b7c & 0x800 != 0 {
                // [inferred] Peter Pan and Tinker Bell fly (+0xb7c bit 0x800): they keep the player's height.
                new.y = player.y;
            } else if let Some(w) = world.as_deref()
                && let Some(g) = w.ground(new, new.y + 60.0, 360.0)
            {
                new.y = g.y;
            }
            obj.motor.pos = new;
            let step = (new - pos).with_y(0.0);
            if delta != Vec3::ZERO {
                obj.motor.turn_to(delta, mv.turn);
            } else if sa.follow.turning {
                obj.motor.turn_to(ground_dir, mv.turn);
            }
            // FUN_002f6320: the locomotion motion, only while motion 0 or one of the table's plays (a motion the script
            // asked for is left alone).
            if !requested && let Ok(mut a) = anims.get_mut(body) {
                let cur = a.slot.map_or(0, |c| c as i32);
                let table = sd.loco;
                if let Some(want) = locomotion_want(&table, cur, a.finished, sp, mv.walk, mv.run) {
                    let state = crate::party_battle::motion_state(battle.active(), false);
                    let slot = if want >= 0 && rig.mset.has(want as usize) { Some(want as usize) } else { data.motion_orders.and_then(|o| crate::party_battle::remap(want, o[state], |x| rig.mset.has(x))) };
                    if let Some(sl) = slot
                        && a.slot != Some(sl)
                    {
                        let looping = rig.mset.get(sl).and_then(|x| x.motion.as_ref()).is_some_and(|mo| mo.frame_return < mo.frame_end);
                        if h.log {
                            info!("summon: {tag} locomotion motion {want} -> slot {sl} (speed {sp:.1}, walk {:.1} run {:.1})", mv.walk, mv.run);
                        }
                        a.play(Some(sl), looping, 8.0);
                    }
                }
            }
            // Host 10.29 (FUN_0010c9d0): the item's command on the target with the item motion 9 (FUN_00175e20).
            // The item is the one the script names (Chicken Little's use_potion: item 1 on the player); its effect lands on the player
            // after trigger 6 of the motion and the wait (the common section below).
            if let Some((item, target)) = sa.item.take() {
                let on_player = target == crate::enemy_obj::PLAYER && item > 0;
                info!(
                    "summon: {tag} item {item} on {target:#x} (host 10.29, FUN_0010c9d0): motion 9 plays; {}",
                    if on_player { "the effect lands on the player after trigger 6 and the wait" } else { "the target is not the player: the effect is not carried out" }
                );
                if let Ok(mut a) = anims.get_mut(body) {
                    let state = crate::party_battle::motion_state(true, false);
                    let slot = if rig.mset.has(9) { Some(9) } else { data.motion_orders.and_then(|o| crate::party_battle::remap(9, o[state], |x| rig.mset.has(x))) };
                    if slot.is_some() {
                        a.play(slot, false, 0.0);
                    }
                    if let (true, Some(sl)) = (on_player, slot) {
                        sa.using = Some(scripted::ItemUse { item: item as u16, slot: sl, left: None });
                    }
                }
            }
            if h.log && *ticks % 60 == 1 {
                info!(
                    "summon: {tag} class {:#x}: {:.0} units from the player, step {:.1}, speed mode {} speed {sp:.1} (walk {:.1} run {:.1}), script mode {:?}, battle {}, follow {follow}",
                    cls.class,
                    (new - player).with_y(0.0).length(),
                    step.length(),
                    sa.follow.mode,
                    mv.walk,
                    mv.run,
                    mode.as_deref().unwrap_or("-"),
                    battle.active()
                );
            }
        }
        // The item use of host 10.29 and of Genie's record 1 (FUN_00175e20): trigger 6 of motion 9, the wait, then the item's
        // effect on the player (FUN_001b8710, FUN_001b82b8, FUN_001b84c8, FUN_001b8658 -> FUN_001a11a0). The summon's item is not
        // taken from any slot (FUN_00175ec0 answers true for the objects with +0x588 bit 0x400000: host 10.31) and the user's
        // Item Boost is not known for a summon [inferred 0: the multiplier is 100].
        if let Some(mut u) = ai.scripted.using {
            let (in_motion, trigger) = anims.get(body).map_or((false, false), |a| {
                let fired = a
                    .motion()
                    .zip(a.triggers())
                    .map(|(mo, tr)| {
                        let g = |f: f32| (f - mo.frame_start) * crate::data::LOGIC_HZ / mo.fps.max(1.0);
                        let (from, to) = (g(a.prev_frame), g(a.frame));
                        from != to && tr.crossed(from, to).any(|f| f.id == crate::items::TRIGGER_USE)
                    })
                    .unwrap_or(false);
                (a.slot == Some(u.slot), fired)
            });
            let (wait, feed) = item_data.as_deref().map_or((30.0, 1.0), |d| (d.use_wait, d.charge_feed));
            match u.step(in_motion, trigger, wait, 1.0) {
                scripted::UseStep::Ended => {
                    info!("summon: {tag} item {} motion ended before trigger {}: nothing used", u.item, crate::items::TRIGGER_USE);
                    ai.scripted.using = None;
                }
                scripted::UseStep::Queued => {
                    info!("summon: {tag} trigger {}: item {} is used (not taken from any slot), the effect lands in {wait} frames", crate::items::TRIGGER_USE, u.item);
                    ai.scripted.using = Some(u);
                }
                scripted::UseStep::Wait => ai.scripted.using = Some(u),
                scripted::UseStep::Land => {
                    ai.scripted.using = None;
                    let row = item_data.as_deref().and_then(|d| d.items.iter().find(|i| i.id == u.item));
                    if let Some(row) = row {
                        let amount = row.flag1 as u16 | (row.rank as u16) << 8;
                        let charge = magic.as_deref().map_or(Default::default(), |m| m.charge);
                        let mut v = crate::items::Vitals { hp: s.hp, hp_max: s.stats.max_hp, hp_min: 0, mp: s.mp, mp_max: game.sora_max_mp, charge_left: charge.left, charge_total: charge.total, ..Default::default() };
                        let (hp0, mp0) = (v.hp, v.mp);
                        crate::items::item_effect(u.item, amount, 100, &mut v, feed);
                        // The player's HP method refuses while his dead bit is set (docs/summons.md WP-SM1): a player who is down
                        // (HP 0) keeps his HP; the other parts of the effect still apply.
                        if hp0 <= 0 {
                            v.hp = hp0;
                        }
                        (s.hp, s.mp) = (v.hp, v.mp);
                        if let Some(m) = magic.as_deref_mut() {
                            m.charge.left = v.charge_left;
                            m.charge.total = v.charge_total;
                        }
                        info!("summon: {tag} item {} (amount {amount}) lands on the player: HP {hp0} -> {}, MP {mp0} -> {}", u.item, s.hp, s.mp);
                    } else {
                        info!("summon: {tag} item {} has no row in the item sheet: nothing happens", u.item);
                    }
                }
            }
        }
        // Host 1.18 made this object a child of the player (Tinker Bell's `appear`): it stands where the link puts it, after
        // the class's own steering.
        if let Some(link) = ai.link
            && let Some((pos, yaw)) = link.pose(s.motor.pos, s.motor.yaw, obj.motor.yaw)
        {
            if h.log && (obj.motor.pos - pos).length() > 50.0 {
                info!("summon: {tag} linked to the player (host 1.18, kind {:#x}, flags {}): placed at {:.0?}", link.kind, link.flags, pos);
            }
            obj.motor.pos = pos;
            obj.motor.yaw = yaw;
        }
        // The script's attack objects and effects (the members' FUN_0018c2d8 pass and effect starts, as friend_ai).
        {
            let model = so_model.as_str();
            let mut out_hits = Vec::new();
            crate::friend_ai::step_attacks(ent, model, obj.motor.pos, fields, &game, &victims, &mut memory, pax.as_ref(), world.as_deref(), h.log, &mut out_hits);
            // Host 2.17: the script's attack objects hit the enemies it named.
            fields.target_hits.clear();
            for (id, victim) in std::mem::take(&mut ai.scripted.hits) {
                let Some(t) = views.iter().find(|v| v.handle == victim) else { continue };
                let at = t.pos * SCALE;
                let dir = (at - obj.motor.pos * SCALE).with_y(0.0).normalize_or(Vec3::Z);
                hits.write(Hit { attacker: ent, target: t.entity, atkp: id, dir, point: at, row: None });
                info!("summon: {tag} script attack atkp {id} hits {:?} (host 2.17)", t.entity);
            }
            for (hit, sh, _) in out_hits {
                hits.write(hit);
                if let Some(sh) = sh {
                    spell_hits.write(sh);
                }
            }
            fx_after(&mut commands, fields, pax.as_ref(), body, s.body, &mut running, h.log, tag);
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

/// The position of an enemy view (game units, the app's axes).
fn ai_target_pos(target: Option<Entity>, views: &[EnemyView]) -> Option<[f32; 4]> {
    target.and_then(|t| views.iter().find(|v| v.entity == t)).map(|v| to_game(v.pos))
}

/// The owner entity an effect of a summon's script is bound to (1.87 the object, 1.149 the object named: the player).
fn fx_owner(on: Option<u32>, body: Entity, player: Entity) -> Option<Entity> {
    match on {
        None => Some(body),
        Some(crate::enemy_obj::PLAYER) => Some(player),
        Some(_) => None,
    }
}

/// Before the script's slice: which effect instances have ended (host 0.49) and where the bound ones are (0.53), as
/// `friend_ai` does for a member.
fn fx_before(
    fields: &mut crate::friend_host::FriendFields,
    pax: Option<&Arc<crate::fx::LoadedPax>>,
    body: Entity,
    player: Entity,
    anims: &Query<&Animator>,
    gts: &Query<&GlobalTransform>,
    running: &mut Query<&mut crate::fx::RunningEffect>,
) {
    for f in fields.fx.iter_mut() {
        if let Some(e) = f.entity
            && running.get(e).is_err()
        {
            f.entity = None;
            f.gone = true;
        }
        if f.gone {
            f.gone_ticks += 1;
            continue;
        }
        let (crate::friend_host::FxReq::Bound { id, on }, Some(p)) = (f.req, pax) else { continue };
        if let Some(owner) = fx_owner(on, body, player)
            && let Some(at) = crate::friend_ai::element_position(p, id, owner, anims, gts, &[])
        {
            f.pos = at / SCALE;
        }
    }
}

/// After the script's slice: the effects it asked for are started, moved, stopped and destroyed.
#[allow(clippy::too_many_arguments)]
fn fx_after(
    commands: &mut Commands,
    fields: &mut crate::friend_host::FriendFields,
    pax: Option<&Arc<crate::fx::LoadedPax>>,
    body: Entity,
    player: Entity,
    running: &mut Query<&mut crate::fx::RunningEffect>,
    log: bool,
    tag: &str,
) {
    use crate::fx::{Bind, Placement};
    use crate::friend_host::FxReq;
    for f in fields.fx.iter_mut() {
        let owner = match f.req {
            FxReq::Bound { on, .. } => fx_owner(on, body, player),
            FxReq::At { .. } => None,
        };
        if !f.started {
            f.started = true;
            let (id, bind, place) = match f.req {
                FxReq::Bound { id, .. } => (id, owner.map(Bind::Owner), Placement::default()),
                FxReq::At { id, pos } => (id, Some(Bind::Fixed(Mat4::from_translation(pos * SCALE))), Placement { replace_translation: true, ..default() }),
            };
            f.entity = match (pax, bind) {
                (Some(p), Some(b)) => crate::fx::start_placed(commands, p, id, b, place),
                _ => None,
            };
            if f.entity.is_none() {
                f.gone = true;
            }
            if log {
                info!("summon: {tag} effect element {id} ({:?}): {}", f.req, if f.entity.is_some() { "started" } else { "not started (no such element, file or owner)" });
            }
        }
        if let Some(e) = f.entity {
            if f.destroy {
                if running.get(e).is_ok() {
                    commands.entity(e).despawn();
                }
                f.entity = None;
                continue;
            }
            if let Ok(mut r) = running.get_mut(e) {
                if std::mem::take(&mut f.stop) {
                    r.effect.end();
                }
                if std::mem::take(&mut f.moved) {
                    r.bind = Bind::Fixed(Mat4::from_translation(f.pos * SCALE));
                }
                if let Some(turn) = f.turn.take() {
                    r.element.rotation = turn;
                }
            }
        }
    }
    fields.fx.retain(|f| !f.destroy && f.gone_ticks < crate::friend_ai::FX_FORGET);
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

    /// WP-SM4: the revive sequence's order (`FUN_001ad378` type 2, thread `FUN_001ad9a0`, `FUN_001adeb8`): the music stops
    /// first, the pad locks in step 1 and is released before Tinker Bell is made, the music restarts after she is made, and
    /// the thread's end releases the field bit last.
    #[test]
    fn the_revive_sequence_runs_in_the_games_order() {
        use ReviveAct::*;
        let mut r = Revive::default();
        assert!(!r.running());
        let start = r.begin(0x4f4);
        assert_eq!(start, [FlagsSet, MusicStop, Sound(0x13), WindowSkipped, FieldHeld]);
        assert!(r.running() && !r.pad_locked());
        // Case 0 only moves on.
        assert!(r.tick(false, true).is_empty());
        assert!(!r.pad_locked());
        // Case 1 locks the pad and falls into the waits, which are over at once.
        let end = r.tick(false, true);
        assert_eq!(end, [PadLock, PadRelease, PlayerToFloor, TinkerMade, FlagsClear, MusicRestart, FieldReleased]);
        assert!(!r.running() && !r.pad_locked());
        assert!(r.tick(false, true).is_empty());
    }

    /// WP-SM4: an open message window or a fade that is not idle (`FUN_00196418`, 0x356b30 +0xfc != 4) keeps the thread
    /// waiting with the pad locked; nothing is made and the music stays stopped until both are over.
    #[test]
    fn the_revive_waits_for_the_window_and_the_fade_with_the_pad_locked() {
        use ReviveAct::*;
        let mut r = Revive::default();
        r.begin(1);
        r.tick(true, true);
        assert_eq!(r.tick(true, true), [PadLock]);
        assert!(r.pad_locked());
        assert!(r.tick(true, true).is_empty());
        assert!(r.tick(false, false).is_empty());
        assert!(r.running() && r.pad_locked());
        let end = r.tick(false, true);
        assert_eq!(end.first(), Some(&PadRelease));
        assert_eq!(end.iter().filter(|a| **a == TinkerMade).count(), 1);
        assert!(end.iter().position(|a| *a == TinkerMade) < end.iter().position(|a| *a == MusicRestart));
        assert!(!r.running());
    }

    /// WP-SM3: a summon call counts one use in the BATTLE REPORT word of its row index (`FUN_001c6168` -> `FUN_001d76f8`);
    /// without a `Progress` nothing happens.
    #[test]
    fn a_summon_call_counts_one_use_of_its_row() {
        let mut p = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        count_summon_call(Some(&mut p), 2);
        count_summon_call(Some(&mut p), 2);
        count_summon_call(Some(&mut p), 4);
        count_summon_call(None, 1);
        assert_eq!(p.summon_uses(), [0, 2, 0, 1]);
        assert_eq!(p.antiform_uses(), 0);
    }

    fn archive() -> Option<kh2_archive::Archive> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        kh2_archive::Archive::open(&iso).ok()
    }

    /// Host 1.0 makes the objects the summon's rows name: Peter Pan (index 4) Tinker Bell, objentry 1268 (`Entity2` of his
    /// `sumn` row), Chicken Little (index 1) 2098 (`SpawnObject1` of his own objentry row); Stitch and Genie none; no summon
    /// (index 0) none.
    #[test]
    fn host_1_0_makes_the_objects_the_summon_rows_name() {
        let Some(mut a) = archive() else { return };
        let rows = kh2_tables::load::<t::Sumn>(&mut a, &t::SUMN).expect("sumn").rows;
        let objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).expect("objentry").rows;
        assert_eq!(kid_object_ids(&rows, &objs, 4), vec![1268]);
        assert_eq!(kid_object_ids(&rows, &objs, 1), vec![2098]);
        assert!(kid_object_ids(&rows, &objs, 2).is_empty() && kid_object_ids(&rows, &objs, 3).is_empty() && kid_object_ids(&rows, &objs, 0).is_empty());
    }

    /// Host 1.114 on Chicken Little's child: the `sumn` row of index 1 names his own object (Entity1 1736), found by that id
    /// while he is in his slot; no summon (index 0), a row past the table, or no object in a slot find nothing.
    #[test]
    fn host_1_114_finds_the_summons_own_object_by_the_rows_entity() {
        let Some(mut a) = archive() else { return };
        let rows = kh2_tables::load::<t::Sumn>(&mut a, &t::SUMN).expect("sumn").rows;
        assert_eq!(main_object(&rows, 1, Some(0x4242)), Some((1736, 0x4242)));
        assert_eq!(main_object(&rows, 1, None), None);
        assert_eq!(main_object(&rows, 0, Some(0x4242)), None);
        assert_eq!(main_object(&rows, 9, Some(0x4242)), None);
    }

    /// Host 1.18 with the player as the parent (Tinker Bell's `appear`, halfwords 0x4002 and 0): the object stands at the
    /// player with his facing; with bit 1 of the second halfword (FUN_001beb90 keeps the facing) it keeps its own; a parent
    /// that is not the player gives no pose.
    #[test]
    fn a_link_to_the_player_places_the_object_at_him() {
        let link = ai::Link { parent: crate::enemy_obj::PLAYER, kind: 0x4002, flags: 0, routine: 0, arg: 0 };
        let (p, yaw) = link.pose(Vec3::new(10.0, 20.0, 30.0), 1.5, 0.25).expect("pose");
        assert_eq!((p, yaw), (Vec3::new(10.0, 20.0, 30.0), 1.5));
        let keep = ai::Link { flags: 2, ..link };
        assert_eq!(keep.pose(Vec3::ONE, 1.5, 0.25), Some((Vec3::ONE, 0.25)));
        let other = ai::Link { parent: 0x1234, ..link };
        assert_eq!(other.pose(Vec3::ONE, 1.5, 0.25), None);
    }

    /// WP-SM3: Peter Pan's two calls of host 1.18 on the player (`P_EX330`): his `appear` makes (0x4002, 0, no routine) and his limit
    /// (command 468) makes (0x4002, 1, routine 0x244, an argument). FUN_001beb90 breaks a link the object already has first
    /// (FUN_001becb8), and only a link with a routine queues anything (FUN_00170dd8 stores it on the record); 1.102 breaks the link
    /// made last and queues its routine with the argument.
    #[test]
    fn peter_pans_links_of_the_player_break_with_their_routine() {
        let player = crate::enemy_obj::PLAYER;
        let appear = ai::Link { parent: player, kind: 0x4002, flags: 0, routine: 0, arg: 0 };
        let limit = ai::Link { parent: player, kind: 0x4002, flags: 1, routine: 0x244, arg: 0x2000_0000 };
        let mut a = ai::SummonAi::default();
        a.link_start(appear);
        assert_eq!(a.link, Some(appear));
        a.link_start(limit);
        assert_eq!(a.link, Some(limit));
        assert!(a.link_broken.is_empty(), "the appear link has no routine: nothing to run");
        assert!(a.link_end(), "the limit link is broken");
        assert!(a.link.is_none());
        assert_eq!(a.link_broken, vec![limit], "its routine 0x244 runs with the argument");
        assert!(!a.link_end(), "no link: nothing happens");
        a.link_start(limit);
        a.link_start(limit);
        assert_eq!(a.link_broken, vec![limit, limit], "a second start breaks the first link and runs its routine");
    }

    /// Host 10.53: entering an instance twice leaves one entry (FUN_00136430 drops the old one), a zero handle is ignored,
    /// and a full list (0x20) loses its oldest entry.
    #[test]
    fn effect_list_registration_follows_fun_00136388() {
        let mut l = Vec::new();
        ai::register_effect(&mut l, 0);
        assert!(l.is_empty());
        ai::register_effect(&mut l, 7);
        ai::register_effect(&mut l, 8);
        ai::register_effect(&mut l, 7);
        assert_eq!(l, vec![8, 7]);
        let mut full: Vec<u32> = (1..=0x20).collect();
        ai::register_effect(&mut full, 99);
        assert_eq!((full.len(), full[0], full[0x1f]), (0x20, 2, 99));
    }

    /// Host 6.49: occupied during the arrival (summon state 1) and not after, whichever of the other three conditions
    /// (actor stop bit, +0x108 bit 0x400, the manager at 0x36f6a4) holds.
    #[test]
    fn host_6_49_is_occupied_in_the_arrival_only() {
        assert!(ai::object_occupied(1, true, false, false, false));
        assert!(!ai::object_occupied(2, true, false, false, false));
        assert!(!ai::object_occupied(0, true, false, false, false));
        assert!(!ai::object_occupied(1, false, false, false, false), "FUN_0019af28 needs +0x588 bits 4 and 0x40 clear");
        assert!(ai::object_occupied(2, true, true, false, false) && ai::object_occupied(2, true, false, true, false) && ai::object_occupied(2, true, false, false, true));
    }

    /// Tinker Bell's revive starts when she is ready (FUN_001c69b0: Peter Pan out, byte 7 set) and the player lies down with no
    /// HP; not while he still stands, still has HP, or when she is not ready (byte 7 cleared by FUN_001c6a10 after one use).
    #[test]
    fn tinker_bells_revive_needs_ready_and_a_knocked_out_player() {
        assert!(revive_due(true, true, 0));
        assert!(!revive_due(true, false, 0), "he is not down yet");
        assert!(!revive_due(true, true, 1), "he has HP");
        assert!(!revive_due(false, true, 0), "byte 7 is clear or another summon is out");
    }

    /// Host 1.18 stores the routine and argument it is given; host 1.102 (and a second 1.18) breaks the link and leaves the
    /// routine to run with that argument (FUN_001becb8 -> FUN_0016c550); a link without a routine breaks without a call; 1.102
    /// without a link does nothing (FUN_001beb00).
    #[test]
    fn a_link_break_hands_back_its_routine_and_argument() {
        let mut ai = ai::SummonAi::default();
        assert!(!ai.link_end(), "no link, nothing to break");
        let with = ai::Link { parent: crate::enemy_obj::PLAYER, kind: 0x4002, flags: 0, routine: 0x0a96, arg: 7 };
        ai.link_start(with);
        assert_eq!(ai.link, Some(with));
        assert!(ai.link_broken.is_empty());
        assert!(ai.link_end());
        assert_eq!((ai.link, ai.link_broken.clone()), (None, vec![with]), "the link is gone and its routine is left to run");
        ai.link_broken.clear();
        ai.link_start(ai::Link { routine: 0, arg: 0, ..with });
        assert!(ai.link_end());
        assert_eq!((ai.link, ai.link_broken.len()), (None, 0), "no routine registered");
        ai.link_start(with);
        let second = ai::Link { kind: 1, ..with };
        ai.link_start(second);
        assert_eq!((ai.link, ai.link_broken.clone()), (Some(second), vec![with]), "a second link_start breaks the first, running its routine");
    }

    /// A routine registered with host 1.18 runs after the link breaks, with the argument registered, and its effect shows: a
    /// two-instruction routine stores its argument in a global. Two breaks in one tick keep both routines in order.
    #[test]
    fn a_broken_links_routine_runs_with_its_argument() {
        use kh2_bdx::{Program, Runtime};
        // The routine at word 10 (after the 10-word entry table of an image with no entries): store its argument, return.
        let code: [u16; 6] = [0x0001, 0x0000, 0x0030, 0x0000, 0x0041, 0x0000];
        let mut b = b"summon_test     ".to_vec();
        for v in [0x20u32, 0x200, 0x200] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&[0; 8]);
        while (b.len() - 0x10) / 2 < 10 {
            b.push(0);
        }
        for w in code.iter().chain(&[0x0089u16]) {
            b.extend_from_slice(&w.to_le_bytes());
        }
        let mut rt = Runtime::new(Program::parse(&b).expect("parse"));
        let mut host = kh2_bdx::mock::MockHost::new();
        rt.spawn(&mut host, kh2_bdx::mock::SELF_OBJ);
        let mut ai = ai::SummonAi::default();
        let link = ai::Link { parent: crate::enemy_obj::PLAYER, kind: 0x4002, flags: 0, routine: 10, arg: 77 };
        ai.link_start(link);
        ai.link_start(ai::Link { arg: 78, ..link });
        assert!(ai.link_end());
        let broken = std::mem::take(&mut ai.link_broken);
        assert_eq!(broken.iter().map(|l| l.arg).collect::<Vec<_>>(), vec![77, 78], "both breaks keep their routine, in order");
        let word = |rt: &Runtime| u32::from_le_bytes(rt.shared.globals[0..4].try_into().unwrap());
        assert_eq!(word(&rt), 0);
        for l in broken {
            rt.call_routine(&mut host, l.routine, &[l.arg]);
        }
        assert_eq!(word(&rt), 78, "the routine stored its argument; the later break's ran last");
        assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
    }

    /// Host 1.277: the HP method adds the maximum and holds the result at it, so a player who is down (HP 0) and one with HP
    /// left both end at the maximum; only the one who is down stands up.
    #[test]
    fn host_1_277_fills_the_players_hp_and_stands_up_only_a_downed_player() {
        assert_eq!(ai::SummonAi::revive_player(0, 120, true), (120, true));
        assert_eq!(ai::SummonAi::revive_player(35, 120, false), (120, false));
        assert_eq!(ai::SummonAi::revive_player(-4, 120, true), (120, true), "a negative HP counts as 0");
        assert_eq!(ai::SummonAi::revive_player(120, 120, false), (120, false));
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
            limit_a: [-70.0, 40.0],
            limit_b: [70.0, 40.0],
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
        assert_eq!(&handlers[..7], &[stitch::STAND, stitch::MOVE, stitch::BLAST, stitch::REFILL, stitch::LINK, stitch::LIMIT_B, stitch::LIMIT_C]);
        assert_eq!(c.depth, c.place_depth);
        assert!(c.max_speed > 0.0 && c.turn > 0.0 && c.wrap_x[1] > 0.0 && c.wrap_y[1] > 0.0 && c.hp_frac > 0.0 && c.mp_frac > c.hp_frac);
        // The limit's two spots (FUN_001198c8 builds the vectors at 0x3b4050 / 0x3b4060 from these words) are read.
        assert!(c.limit_a != [0.0; 2] && c.limit_b != [0.0; 2] && c.limit_a != c.limit_b, "{:?} {:?}", c.limit_a, c.limit_b);
        // The three limit sub-commands the dispatcher FUN_00119488 knows, and the pointers it reads for them.
        for (cmd, ptmf) in [(525, 0x378898u32), (526, 0x3788a0), (527, 0x3788a8)] {
            assert_eq!(stitch::limit_record(cmd), exe_word(&exe, ptmf), "command {cmd} -> the handler at {ptmf:#x}");
        }
        assert_eq!(stitch::limit_record(528), None);
    }

    /// Oracle: the game's FUN_0017bee0 (host 1.100's fall time) against `ai::fall_time`, and FUN_0017b1e0 (the launch host
    /// 1.128 reaches through the mover) with FUN_0017b1b0 against `ai::launch`, over speeds of both signs. The EE's `sqrt.s`
    /// of a negative operand (the fall time of a negative height) is the root of its absolute value in the emulator.
    #[test]
    fn air_launch_and_fall_time_match_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const OBJECT: u32 = 0x0182_0000;
        let Some(mut a) = archive() else { return };
        let exe = elf(&mut a);
        let c = SummonConsts::from_exe(&exe).expect("constants").air;
        assert!((c.gravity - 20.0 / 49.0).abs() < 1e-6 && c.down_factor == -2.0, "{c:?}");
        let mut ee = Ee::new();
        ee.load_elf(&exe).expect("elf");
        let mut seed = 0x2f6b_1c05u32;
        let mut next = |span: f32| {
            seed = seed.wrapping_mul(0x0019_660d).wrapping_add(0x3c6e_f35f);
            ((seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 2.0 * span
        };
        let mut n = 0;
        for k in 0..400 {
            let x = if k % 2 == 0 { next(40.0) } else { next(900.0) };
            ee.call(0x0017_bee0, &[], &[x], STACK).expect("FUN_0017bee0");
            let game = ee.fpr[0];
            let port = ai::fall_time(x, c.gravity);
            assert!((game - port).abs() <= 1e-3 * port.abs().max(1.0), "fall time of {x}: game {game} port {port}");
            // The launch: the object block is zero, then the launch fills +0xd0..+0xdc and the mover fields.
            ee.write(OBJECT, &[0u8; 0x200]);
            ee.w32(OBJECT + 0x10c, 0xffff_ffff);
            ee.call(0x0017_b1e0, &[OBJECT as u64], &[x], STACK).expect("FUN_0017b1e0");
            let got = [ee.f32_at(OBJECT + 0xd0), ee.f32_at(OBJECT + 0xd4), ee.f32_at(OBJECT + 0xd8), ee.f32_at(OBJECT + 0xdc)];
            let l = ai::launch(x, &c);
            for (g, p) in got.iter().zip([l.d0, l.d4, l.d8, l.dc]) {
                assert!((g - p).abs() <= 1e-3 * p.abs().max(1.0), "launch of {x}: game {got:?} port {l:?}");
            }
            // FUN_0017b1b0: the mover kind 1 fields.
            assert_eq!((ee.r8(OBJECT + 0x514), ee.r32(OBJECT + 0x100), ee.r32(OBJECT + 0x104), ee.r32(OBJECT + 0xcc), ee.r32(OBJECT + 0x10c) & 1), (1, u32::MAX, 5, 0, 0));
            n += 1;
        }
        eprintln!("oracle: FUN_0017bee0 and FUN_0017b1e0 equal in {n} cases");
    }

    /// The formulas without the disc: the fall time of a height and the two launch branches.
    #[test]
    fn fall_time_and_launch_follow_the_formulas() {
        let c = ai::AirConsts { gravity: 20.0 / 49.0, down_factor: -2.0, down_divisor: 20.0 / 49.0 };
        assert!((ai::fall_time(0.4, c.gravity) - 1.4).abs() < 1e-5, "sqrt(0.8 / 0.408) = 1.4");
        assert_eq!(ai::fall_time(0.0, c.gravity), 0.0);
        let up = ai::launch(3.0, &c);
        assert_eq!((up.d0, up.d8, up.dc), (0.0, 0.0, -3.0));
        assert!((up.d4 - (6.0f32 / c.gravity).sqrt()).abs() < 1e-5);
        let down = ai::launch(-1.0, &c);
        assert_eq!((down.d4, down.dc, down.d8), (0.0, 0.0, -1.0));
        assert!((down.d0 - (2.0f32 / c.down_divisor).sqrt()).abs() < 1e-5);
    }

    fn stitch_limit_records() -> Vec<crate::party_battle::Record> {
        use crate::party_battle::{Handler, Record};
        let mut recs = stitch_test_records();
        recs.extend([(0x212, stitch::LIMIT_B), (0x212, stitch::LIMIT_C)].iter().map(|&(flags, a)| Record { flags, handler: Handler::Other(a), id: 0, entry: None, ability: 0, item: 0, setting: -1, mode: String::new() }));
        recs
    }

    /// FUN_00119488 (host 7.26): 525 starts record 4 (motion 0xc9, the player's linked motion 0x101, no attack), 526 record 5
    /// (the first spot, motion 0xcd, the attack object of FUN_00118110 every frame and three AI slots) and 527 record 6 (the
    /// second spot, motion 0xf2, two slots); the records then stay while their motion plays and end when it changes; any other
    /// command only clears the action.
    #[test]
    fn stitch_limit_sub_commands_start_records_4_5_and_6() {
        let c = stitch_test_consts();
        let recs = stitch_limit_records();
        let now = stitch::Now { battle: true, hp: (100, 100), mp: (100, 100), in_view: 1, ..Default::default() };
        let mut ai = stitch::StitchAi { rng: stitch::Rng(5), ..Default::default() };
        let out = stitch::limit_tick(&c, &recs, &mut ai, &now, 525);
        assert_eq!(out.started, Some((4, true)));
        assert_eq!((ai.motion, out.player_motion, out.blast, ai.choosing.current), (Some(stitch::M_LINK), Some(0x101), false, Some(4)));
        assert_eq!((ai.pos[0], ai.pos[1], ai.pos[2]), (c.limit_a[0], c.limit_a[1], c.place_depth), "placed at the first spot (FUN_001175a0)");
        for (cmd, rec, motion, spot, slots, player) in [(526, 5, stitch::M_LIMIT_B, c.limit_a, 3, 0xff), (527, 6, stitch::M_LIMIT_C, c.limit_b, 2, 0x100)] {
            let mut ai = stitch::StitchAi { rng: stitch::Rng(5), ..Default::default() };
            let out = stitch::limit_tick(&c, &recs, &mut ai, &now, cmd);
            assert_eq!(out.started, Some((rec, true)), "command {cmd}");
            assert!(out.blast && out.slots.len() == slots && out.player_motion == Some(player), "{out:?}");
            assert_eq!((ai.motion, ai.attack, ai.choosing.current), (Some(motion), Some(motion), Some(rec)));
            assert_eq!((ai.pos[0], ai.pos[1]), (spot[0], spot[1]));
            // The next frames keep the record and fire the attack object each frame while the motion plays.
            for _ in 0..10 {
                ai.motion_new = false;
                let out = stitch::tick(&c, &recs, &mut ai, &now);
                assert!(out.blast && out.started.is_none() && ai.choosing.current == Some(rec), "command {cmd}: {out:?} {:?}", ai.choosing.current);
                assert_eq!((ai.pos[0], ai.pos[1]), (spot[0], spot[1]), "it stays at its spot");
            }
            // The motion changes (the next record asked for another one): the attack object is released and the record ends.
            ai.motion = Some(stitch::M_STAND);
            let out = stitch::tick(&c, &recs, &mut ai, &now);
            assert!(ai.attack.is_none() && !out.blast, "released with the motion: {out:?}");
            assert_ne!(ai.choosing.current, Some(rec), "the record ended");
        }
        let mut ai = stitch::StitchAi { rng: stitch::Rng(5), ..Default::default() };
        ai.choosing.current = Some(2);
        let out = stitch::limit_tick(&c, &recs, &mut ai, &now, 528);
        assert!(out.started.is_none() && ai.choosing.current.is_none() && ai.motion.is_none());
        // Records 4 and 5 queued motion 0 behind theirs: when it ended the motion is 0 and the record is over; record 6 queued
        // nothing, so its motion stays and the record goes on (and fires again) until the script clears the action.
        for (cmd, rec, motion, over) in [(525, 4, stitch::M_LINK, true), (526, 5, stitch::M_LIMIT_B, true), (527, 6, stitch::M_LIMIT_C, false)] {
            let mut ai = stitch::StitchAi { rng: stitch::Rng(5), ..Default::default() };
            stitch::limit_tick(&c, &recs, &mut ai, &now, cmd);
            ai.motion_new = false;
            let done = stitch::Now { motion_done: true, ..now };
            stitch::tick(&c, &recs, &mut ai, &done);
            stitch::tick(&c, &recs, &mut ai, &done);
            assert_eq!(ai.motion != Some(motion), over, "command {cmd}: the motion is {:?}", ai.motion);
            assert_eq!(ai.choosing.current != Some(rec), over, "command {cmd}: record {rec} {} its motion ended", if over { "is over once" } else { "goes on when" });
        }
    }

    /// An item use (host 10.29, Genie's record 1): the motion has to play until trigger 6; the trigger queues the use for
    /// `wait` frames (the tick of the trigger takes one) and the effect lands when they are gone; a motion that ended earlier
    /// uses nothing.
    #[test]
    fn an_item_use_waits_for_its_trigger_and_then_for_the_queue() {
        use scripted::{ItemUse, UseStep};
        let mut u = ItemUse { item: 1, slot: 9, left: None };
        assert_eq!(u.step(true, false, 30.0, 1.0), UseStep::Wait);
        assert_eq!(u.step(true, true, 30.0, 1.0), UseStep::Queued);
        assert_eq!(u.left, Some(29.0));
        let mut ticks = 0;
        while u.step(false, false, 30.0, 1.0) != UseStep::Land {
            ticks += 1;
            assert!(ticks < 40);
        }
        assert_eq!(ticks, 28, "29 frames are left after the trigger tick: the 29th later tick lands it");
        let mut early = ItemUse { item: 1, slot: 9, left: None };
        assert_eq!(early.step(false, true, 30.0, 1.0), UseStep::Ended, "the motion is not playing: nothing is used even on the trigger");
    }

    /// FUN_001155e0's tail: while the player's HP is below his maximum one HP is given every `period` frames (the timer
    /// starts at the entry's +4 and only runs while he is hurt); at full HP the timer holds.
    #[test]
    fn stitch_heals_the_player_one_hp_each_period_while_hurt() {
        let c = stitch_test_consts();
        let recs = stitch_test_records();
        let hurt = stitch::Now { battle: false, hp: (50, 100), mp: (100, 100), ..Default::default() };
        let mut ai = stitch::StitchAi { rng: stitch::Rng(9), ..Default::default() };
        let heals: Vec<usize> = (0..200).filter(|_| stitch::tick(&c, &recs, &mut ai, &hurt).heal).collect();
        assert_eq!(heals, vec![59, 119, 179], "the period is {} frames", c.period);
        let full = stitch::Now { hp: (100, 100), ..hurt };
        let mut ai = stitch::StitchAi { rng: stitch::Rng(9), ..Default::default() };
        assert!((0..500).all(|_| !stitch::tick(&c, &recs, &mut ai, &full).heal));
        assert_eq!(ai.heal_timer, None, "the timer does not run at full HP");
    }

    /// Hosts 10.3 / 10.4 by class: Genie, Peter Pan and Tinker Bell clear the action and break the link they have (the routine
    /// of the broken link is queued); Chicken Little's FPS mode also links him to the player (0x4000, 4: his origin and facing)
    /// and the way out breaks it; Stitch's limit places him at the first spot with the stand motion.
    #[test]
    fn hosts_10_3_and_10_4_act_by_class() {
        use bevy::prelude::Vec3;
        let link = ai::Link { parent: crate::enemy_obj::PLAYER, kind: 0x4002, flags: 0, routine: 0x244, arg: 9 };
        let mut a = ai::SummonAi { action: (3, 4), link: Some(link), ..Default::default() };
        a.base_enter();
        assert_eq!(a.action, (0, 0));
        assert!(a.link.is_none() && a.link_broken == vec![link], "the link broke and its routine is queued");
        let mut a = ai::SummonAi { action: (3, 4), link: Some(link), ..Default::default() };
        a.fps_enter(100.0);
        assert!(a.scripted.fps && a.scripted.fps_arg == [100.0, 0.0] && a.action == (0, 0));
        assert_eq!(a.link_broken, vec![link], "the earlier link broke first");
        let l = a.link.expect("linked to the player");
        assert_eq!((l.parent, l.kind, l.flags, l.routine), (crate::enemy_obj::PLAYER, 0x4000, 4, 0));
        assert_eq!(l.pose(Vec3::new(5.0, 0.0, 2.0), 1.0, 0.3), Some((Vec3::new(5.0, 0.0, 2.0), 1.0)), "at his origin with his facing");
        a.fps_leave();
        assert!(!a.scripted.fps && a.link.is_none());
        let c = stitch_test_consts();
        let mut a = ai::SummonAi::default();
        a.stitch_limit_enter(&c);
        assert!(a.stitch.limit_link && a.stitch.attached && a.stitch.motion == Some(stitch::M_STAND));
        assert_eq!((a.stitch.pos[0], a.stitch.pos[1], a.stitch.pos[2]), (c.limit_a[0], c.limit_a[1], c.place_depth));
        a.stitch_limit_leave();
        assert!(!a.stitch.limit_link);
    }

    /// Host 10.51's answer (`screen_search`): the script handle of the enemy `fps::screen_target` finds, and 0 when the box
    /// holds none, the enemy is out of view, or the camera is missing the point; the part's point wins over the position.
    #[test]
    fn host_10_51_answers_the_handle_of_the_enemy_in_the_box() {
        let view = fps::ScreenView::new(&GlobalTransform::default(), Mat4::perspective_rh(1.0, 4.0 / 3.0, 0.01, 1000.0));
        let mut w = bevy::ecs::world::World::new();
        let [a, b, c] = std::array::from_fn(|_| w.spawn_empty().id());
        let ev = |entity, handle, x: f32| EnemyView { entity, handle, pos: Vec3::new(x, 0.0, -10.0) / SCALE, reacting: false, blown: false };
        let enemies = [ev(a, 0x7000_0001, 1.0), ev(b, 0x7000_0002, 0.4), ev(c, 0x7000_0003, 0.0)];
        let aim = Vec3::new(0.0, 0.0, -10.0) / SCALE;
        let all = [a, b, c];
        assert_eq!(screen_search(&view, &enemies, &all, &[], aim, [120.0, 60.0]), (Some(c), 0x7000_0003), "c is the nearest to the aim");
        // c is out of view: b is nearer than a.
        assert_eq!(screen_search(&view, &enemies, &[a, b], &[], aim, [120.0, 60.0]), (Some(b), 0x7000_0002));
        // b's target part sits far to the side, outside the box: a is taken.
        let part = crate::friend_host::SearchPart { entity: b, shape: 0, part: 0, point: Vec3::new(5.0, 0.0, -10.0) / SCALE, radius: 1.0 };
        assert_eq!(screen_search(&view, &enemies, &[a, b], &[part], aim, [120.0, 60.0]), (Some(a), 0x7000_0001));
        assert_eq!(screen_search(&view, &enemies, &[], &[], aim, [120.0, 60.0]), (None, 0), "no enemy in view: the empty answer");
    }

    /// Host 10.51's search (FUN_0011a6d8): the first enemy in the box is taken, a nearer one replaces it, one outside the
    /// box on either axis or behind the camera never counts, and an aim behind the camera finds nothing.
    #[test]
    fn the_screen_box_search_takes_the_nearest_enemy_inside_the_box() {
        use bevy::prelude::*;
        let view = fps::ScreenView::new(&GlobalTransform::default(), Mat4::perspective_rh(1.0, 4.0 / 3.0, 0.01, 1000.0));
        let mut w = World::new();
        let [a, b, c, d, e] = std::array::from_fn(|_| w.spawn_empty().id());
        let aim = Vec3::new(0.0, 0.0, -10.0);
        let at = |x: f32, y: f32| Vec3::new(x, y, -10.0);
        // 439 pixels of focal length: one unit at depth 10 is 44 pixels.
        let near = [(a, at(1.0, 0.0)), (b, at(3.0, 0.0)), (c, at(0.5, 0.0)), (d, at(0.0, 2.0)), (e, Vec3::new(0.0, 0.0, 5.0))];
        assert_eq!(fps::screen_target(&view, aim, [120.0, 60.0], &near), Some(c), "b is too wide, d too high, e behind; c is nearer than a");
        assert_eq!(fps::screen_target(&view, aim, [120.0, 60.0], &near[..2]), Some(a), "only a is inside");
        assert_eq!(fps::screen_target(&view, aim, [120.0, 60.0], &near[1..2]), None);
        assert_eq!(fps::screen_target(&view, Vec3::new(0.0, 0.0, 5.0), [120.0, 60.0], &near), None, "the aim point is behind the camera");
        // A wider box takes b as well; a candidate equally near does not replace the first.
        assert_eq!(fps::screen_target(&view, aim, [200.0, 60.0], &[(b, at(3.0, 0.0))]), Some(b));
        assert_eq!(fps::screen_target(&view, aim, [200.0, 60.0], &[(a, at(1.0, 0.0)), (b, at(-1.0, 0.0))]), Some(a));
    }

    /// FUN_002f6320's choice among a class's five motions, with Genie's table from the executable: standing it is motion 0,
    /// walking the first entry, running the second, and the third (the stop motion) after either until it has finished; a
    /// motion the script asked for is left alone.
    #[test]
    fn locomotion_want_follows_fun_002f6320() {
        let genie = [1, 0x98, 0x99, -1, -1];
        assert_eq!(locomotion_want(&genie, 0, false, 0.0, 2.0, 8.0), Some(0));
        assert_eq!(locomotion_want(&genie, 0, false, 3.0, 2.0, 8.0), Some(1), "below the mid speed 5");
        assert_eq!(locomotion_want(&genie, 1, false, 6.0, 2.0, 8.0), Some(0x98), "from the mid speed");
        assert_eq!(locomotion_want(&genie, 0x98, false, 0.0, 2.0, 8.0), Some(0x99), "stopping after the run");
        assert_eq!(locomotion_want(&genie, 0x99, false, 0.0, 2.0, 8.0), Some(0x99), "the stop motion plays out");
        assert_eq!(locomotion_want(&genie, 0x99, true, 0.0, 2.0, 8.0), Some(0));
        assert_eq!(locomotion_want(&genie, 55, false, 6.0, 2.0, 8.0), None, "a script's motion stays");
        // Chicken Little's table has no stop motion: standing is 0 whatever played.
        assert_eq!(locomotion_want(&[1, 2, -1, -1, -1], 2, false, 0.0, 2.0, 8.0), Some(0));
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
            stitch::goal(&c, case, p, &mut want, &mut || 0.0, &mut |_| 0);
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

    /// Genie's class from the executable: the two records (FUN_001199f8: 0x3548a8, count 2), the form table and
    /// the decision: out of battle the follow; in battle the follow until the player's HP is in danger, then the
    /// emergency pass takes the potion record, and the class 0xf speed rule.
    #[test]
    fn genie_class_from_the_executable_and_its_decision() {
        let Some(mut a) = archive() else { return };
        let exe = elf(&mut a);
        let k = genie::Consts::from_exe(&exe).expect("constants");
        let recs = genie::records(&exe).expect("records");
        let handlers: Vec<u32> = recs.iter().map(stitch::addr).collect();
        assert_eq!(handlers, [genie::FOLLOW, genie::POTION]);
        assert_eq!(recs.iter().map(|r| r.flags).collect::<Vec<_>>(), [0x201, 0x10]);
        eprintln!("genie: forms {:?}, byte 7 commands {:?}, heal fraction {}, speed add {}", k.forms, k.form_commands, k.heal_fraction, k.speed_add);
        // FUN_002f6320's table for NeoStatus 201: motion 1, the flying motions 0x98 and 0x99, two unused entries.
        assert_eq!(k.loco, [1, 0x98, 0x99, -1, -1]);
        // Every form command maps to its limit and back to its byte 7 through FUN_001c6b10.
        for (row, &(cmd, lim)) in k.forms.iter().enumerate() {
            assert_eq!(genie::limit(&k, cmd as i32), lim as i32);
            let f = genie::form(&k, cmd as i32);
            assert_eq!((f.shown, f.byte7), (Some(row as i32 + 2), row as u8 + 1));
            if row < 4 {
                assert_eq!(genie::start_command(&k, f.byte7), cmd as i32, "byte 7 {} reads back its command", f.byte7);
            }
        }
        assert_eq!(genie::limit(&k, 0), 0);
        assert_eq!(genie::form(&k, 0).byte7, 6);
        let mut ai = genie::GenieAi { rng: stitch::Rng(5), ..Default::default() };
        let calm = genie::Now { battle: false, hp: (100, 100) };
        assert_eq!(genie::decide(&k, &recs, &mut ai, &calm), genie::Out { follow: true, ..Default::default() });
        let fight = genie::Now { battle: true, hp: (100, 100) };
        for _ in 0..50 {
            let out = genie::decide(&k, &recs, &mut ai, &fight);
            assert!(out.follow && !out.potion, "healthy: only the follow record is feasible");
        }
        let low = genie::Now { battle: true, hp: (20, 100) };
        let out = genie::decide(&k, &recs, &mut ai, &low);
        assert_eq!((out.potion, out.started), (true, Some((1, true))), "in danger: the emergency pass takes record 1");
        assert_eq!(genie::follow_speed(&k, 8.0, 4.0, 10.0), 10.0 + k.speed_add);
        assert_eq!(genie::follow_speed(&k, 3.0, 4.0, 10.0), 3.0, "at or below the walk speed the speed stays");
        assert_eq!(genie::follow_speed(&k, 12.0, 4.0, 10.0), 12.0);
    }

    /// Oracle: FUN_00119ca8 (host 10.10), FUN_00119c18 (host 10.9, with FUN_00192ce8 / FUN_00192cd0 replaced by
    /// stubs that log their part argument, FUN_001c6b48 the game's own) and FUN_00119a40's FUN_001c6b10 run by
    /// kh2-ee on the user's executable against `genie::limit`, `genie::form` and `genie::start_command` for every
    /// table command and other values; FUN_0010f490 against `party_battle::below_fraction` over random HP.
    #[test]
    fn genie_form_and_limit_match_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const AI: u32 = 0x0180_0000;
        const OBJ: u32 = 0x0181_0000;
        const STATUS: u32 = 0x0182_0000;
        const LOG: u32 = 0x0190_0000;
        const STUBS: u32 = 0x0188_0000;
        let Some(mut a) = archive() else { return };
        let exe = elf(&mut a);
        let k = genie::Consts::from_exe(&exe).expect("constants");
        let mut ee = Ee::new();
        ee.load_elf(&exe).expect("elf");
        // Two stubs: log[count] = part | tag; count += 1 (lui t0, LOG; lw t1; sll t2, t1, 2; addu t2, t2, t0;
        // addiu t3, a1, tag; sw t3, 4(t2); addiu t1, t1, 1; sw t1; jr ra; nop), and jumps to them.
        for (at, tag) in [(STUBS, 0x100u32), (STUBS + 0x40, 0x200)] {
            let code = [0x3c08_0000 | (LOG >> 16), 0x8d09_0000, 0x0009_5080, 0x0148_5021, 0x24ab_0000 | tag, 0xad4b_0004, 0x2529_0001, 0xad09_0000, 0x03e0_0008, 0];
            for (i, w) in code.iter().enumerate() {
                ee.w32(at + 4 * i as u32, *w);
            }
        }
        ee.w32(0x0019_2ce8, 0x0800_0000 | (STUBS >> 2));
        ee.w32(0x0019_2cec, 0);
        ee.w32(0x0019_2cd0, 0x0800_0000 | ((STUBS + 0x40) >> 2));
        ee.w32(0x0019_2cd4, 0);
        let mut cmds: Vec<i32> = k.forms.iter().map(|p| p.0 as i32).collect();
        cmds.extend([0, 1, 534, 539, 606, 608, -1]);
        for &cmd in &cmds {
            ee.w32(AI + 4, OBJ);
            ee.w32(AI + 0x14, cmd as u32);
            let got = ee.call(0x0011_9ca8, &[AI as u64], &[], STACK).expect("FUN_00119ca8") as u32 as i32;
            assert_eq!(got, genie::limit(&k, cmd), "FUN_00119ca8 for {cmd}");
            ee.w32(LOG, 0);
            ee.w32(AI + 0x14, 0);
            ee.call(0x0011_9c18, &[AI as u64, cmd as i64 as u64], &[], STACK).expect("FUN_00119c18");
            let want = genie::form(&k, cmd);
            assert_eq!(ee.r32(AI + 0x14) as i32, cmd, "AI +0x14");
            let n = ee.r32(LOG) as usize;
            let log: Vec<u32> = (0..n).map(|i| ee.r32(LOG + 4 + 4 * i as u32)).collect();
            let mut expect: Vec<u32> = want.hidden.iter().map(|&p| 0x100 + p as u32).collect();
            expect.extend(want.shown.map(|p| 0x200 + p as u32));
            assert_eq!(log, expect, "FUN_00119c18 parts for {cmd}");
            assert_eq!(ee.r8(0x0034_0c8f), want.byte7, "byte 7 for {cmd}");
        }
        for b in 0..5u8 {
            let got = ee.call(0x001c_6b10, &[b as u64], &[], STACK).expect("FUN_001c6b10") as u16;
            assert_eq!(got as i32, genie::start_command(&k, b), "FUN_001c6b10({b})");
        }
        let mut seed = 0x9e37_79b9u32;
        for _ in 0..300 {
            seed = seed.wrapping_mul(0x0019_660d).wrapping_add(0x3c6e_f35f);
            let max = (seed >> 8) % 400 + 1;
            let hp = (seed >> 20) % (max + 1);
            ee.w32(OBJ + 0x48c, STATUS);
            ee.w32(STATUS, hp);
            ee.w32(STATUS + 4, max);
            let got = ee.call(0x0010_f490, &[OBJ as u64], &[k.heal_fraction], STACK).expect("FUN_0010f490") != 0;
            assert_eq!(got, crate::party_battle::below_fraction(hp as i32, max as i32, k.heal_fraction), "FUN_0010f490 hp {hp}/{max}");
        }
        eprintln!("oracle: FUN_00119ca8, FUN_00119c18, FUN_001c6b10 equal for {} commands; FUN_0010f490 over 300 rounds", cmds.len());
    }

    /// The three classes of Chicken Little, Peter Pan and Tinker Bell from the executable (FUN_0011a450 / FUN_0011a048 /
    /// FUN_0011a230): their records (one for each of the first two, none for Tinker Bell), the record handlers and mode
    /// string, the locomotion tables of FUN_002f6320, and the decision of FUN_00111070 for them: out of battle the follow,
    /// in battle nothing while the script runs a mode (FUN_0010fcd0 returns at once, FUN_0010f698) and the record that
    /// starts `battle` otherwise.
    #[test]
    fn scripted_classes_from_the_executable_and_their_decision() {
        let Some(mut a) = archive() else { return };
        let exe = elf(&mut a);
        let k = scripted::Consts::from_exe(&exe).expect("constants");
        let all: Vec<_> = scripted::CLASSES.iter().map(|c| scripted::Data::from_exe(&exe, c).expect("class")).collect();
        let (chicken, pan, tinker) = (&all[0], &all[1], &all[2]);
        eprintln!("scripted: speed add {} aim ahead {} join window {}; loco {:?} {:?} {:?}", k.speed_add, k.aim_ahead, k.join_window, chicken.loco, pan.loco, tinker.loco);
        assert_eq!((chicken.records.len(), pan.records.len(), tinker.records.len()), (1, 1, 0));
        for (d, handler) in [(chicken, scripted::START_MODE[0]), (pan, scripted::START_MODE[1])] {
            let r = &d.records[0];
            assert_eq!(r.flags, 3, "not counted, an attack");
            assert_eq!(r.handler, crate::party_battle::Handler::Other(handler));
            assert_eq!(r.mode, "battle");
        }
        // FUN_002f6320's tables: the default {1, 2, -1, -1, -1} (Chicken Little, Tinker Bell), Peter Pan's own.
        assert_eq!(chicken.loco, [1, 2, -1, -1, -1]);
        assert_eq!(tinker.loco, chicken.loco);
        // Peter Pan's own table at 0x3716c0, read from the disc and printed above: motions 165, 161..164.
        assert_eq!(pan.loco, [165, 161, 162, 163, 164]);
        assert_eq!((k.speed_add, k.aim_ahead, k.join_window), (2.0, 200.0, 8.0));
        let mut ai = scripted::ScriptedAi { rng: stitch::Rng(7), ..Default::default() };
        let calm = scripted::Now { battle: false, mode_running: false };
        assert_eq!(scripted::decide(&chicken.records, 0x12, &mut ai, &calm), scripted::Out { follow: true, ..Default::default() });
        let busy = scripted::Now { battle: true, mode_running: true };
        for _ in 0..30 {
            assert_eq!(scripted::decide(&chicken.records, 0x12, &mut ai, &busy), scripted::Out::default(), "the script is busy: nothing is chosen");
        }
        let idle = scripted::Now { battle: true, mode_running: false };
        for d in [chicken, pan] {
            let out = scripted::decide(&d.records, d.class.class, &mut ai, &idle);
            assert_eq!(out.start, Some((0, "battle".to_string())));
            assert!(!out.follow);
        }
        // Tinker Bell has no record: nothing starts.
        assert_eq!(scripted::decide(&tinker.records, 0x11, &mut ai, &idle), scripted::Out::default());
        // FUN_001102c8's speed rule is for the classes 0xc, 0xf and 0x10 only.
        assert_eq!(scripted::follow_speed(0x10, &k, 8.0, 4.0, 10.0), 10.0 + k.speed_add);
        assert_eq!(scripted::follow_speed(0x12, &k, 8.0, 4.0, 10.0), 8.0);
        assert_eq!(scripted::follow_speed(0x11, &k, 8.0, 4.0, 10.0), 8.0);
        assert_eq!(scripted::follow_speed(0x10, &k, 3.0, 4.0, 10.0), 3.0);
        // FUN_0011a2e8: the aim point is the target's position, else the facing vector times DAT_003788e8 from the object.
        assert_eq!(scripted::aim_point(&k, Some([1.0, 2.0, 3.0, 1.0]), [9.0; 4], [0.0; 4]), [1.0, 2.0, 3.0, 1.0]);
        let ahead = scripted::aim_point(&k, None, [10.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(ahead, [10.0, 0.0, k.aim_ahead, 1.0]);
    }

    /// Host 1.155 on the player (the arm in `SummonHost::own` calls `SummonAi::heal_player`) and what `objects` does after the
    /// tick (`apply_heal`): Peter Pan's amount 5 on 10/100 gives 15 and the player's HP becomes 15; the maximum holds; a KO'd
    /// player (HP 0) is not revived by it; a tick without a heal leaves the HP alone.
    #[test]
    fn host_1_155_heal_reaches_the_players_hp() {
        let mut ai = ai::SummonAi { player_hp: (10, 100), ..Default::default() };
        assert_eq!(ai.heal_player(5), 15, "the method gives the new HP back");
        assert_eq!((ai.player_hp.0, ai.player_heal), (15, 5));
        let mut hp = 10;
        assert_eq!(ai.apply_heal(&mut hp), Some((10, 15)));
        assert_eq!((hp, ai.player_heal), (15, 0), "taken once");
        assert_eq!(ai.apply_heal(&mut hp), None, "no heal this tick");
        // Two heals in a tick add; the maximum clamps.
        let mut ai = ai::SummonAi { player_hp: (97, 100), ..Default::default() };
        assert_eq!(ai.heal_player(5), 100);
        assert_eq!(ai.heal_player(5), 100);
        assert_eq!(ai.player_heal, 3);
        let mut hp = 97;
        assert_eq!(ai.apply_heal(&mut hp), Some((97, 100)));
        // KO: nothing changes and nothing is applied.
        let mut ai = ai::SummonAi { player_hp: (0, 100), ..Default::default() };
        assert_eq!(ai.heal_player(5), 0);
        assert_eq!(ai.player_heal, 0);
        let mut hp = 0;
        assert_eq!(ai.apply_heal(&mut hp), None);
        assert_eq!(hp, 0);
    }

    /// `scripted::decide` offers only a record its stand-ins fit: the handler is one of the class handlers whose phase 1
    /// answers 1, the setting index is -1 and no MP is spent. A flagged-3 record with another handler, a setting or an MP
    /// cost is not chosen, instead of passing an always-true test.
    #[test]
    fn scripted_decide_offers_only_records_its_stand_ins_fit() {
        use crate::party_battle::{Handler, Record};
        let rec = |handler| Record { flags: 3, handler, id: 0, entry: None, ability: 0, item: 0, setting: -1, mode: "battle".to_string() };
        let idle = scripted::Now { battle: true, mode_running: false };
        let decide = |r: Record| scripted::decide(&[r], 0x12, &mut scripted::ScriptedAi { rng: stitch::Rng(7), ..Default::default() }, &idle);
        assert_eq!(decide(rec(Handler::Other(scripted::START_MODE[0]))).start, Some((0, "battle".to_string())));
        assert_eq!(decide(rec(Handler::Other(scripted::START_MODE[1]))).start, Some((0, "battle".to_string())));
        assert_eq!(decide(rec(Handler::Other(0x11_1111))).start, None, "another handler: phase 1 not ported");
        assert_eq!(decide(rec(Handler::Attack)).start, None);
        let mut with_setting = rec(Handler::Other(scripted::START_MODE[0]));
        with_setting.setting = 3;
        assert_eq!(decide(with_setting).start, None, "a setting byte would be read");
    }

    /// Oracle: FUN_00119ce8 (host 10.25's test: may a member join the player's attack) run by kh2-ee on the user's
    /// executable against `scripted::join_ready` over the three globals it reads (`DAT_0034e9d0` the recorded motion,
    /// `DAT_0034e9c8` / `DAT_0034e9cc` the attack clock and its mark) and the object's +0xb7c.
    #[test]
    fn scripted_join_window_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        let Some(mut a) = archive() else { return };
        let exe = elf(&mut a);
        let k = scripted::Consts::from_exe(&exe).expect("constants");
        let mut ee = Ee::new();
        ee.load_elf(&exe).expect("elf");
        let mut seed = 0x1234_5678u32;
        let mut rnd = move || {
            seed = seed.wrapping_mul(0x0019_660d).wrapping_add(0x3c6e_f35f);
            seed
        };
        let mut yes = 0;
        for _ in 0..400 {
            let motion = (rnd() % 9) as i32 - 3;
            let since = (rnd() % 40) as f32 - 5.0;
            let mark = (rnd() % 40) as f32 - 5.0;
            let b7c = rnd() & 0x7ff;
            ee.w32(0x0034_e9d0, motion as u32);
            ee.w32(0x0034_e9c8, since.to_bits());
            ee.w32(0x0034_e9cc, mark.to_bits());
            let got = ee.call(0x0011_9ce8, &[b7c as u64], &[], STACK).expect("FUN_00119ce8") & 0xff != 0;
            let want = scripted::join_ready(&k, b7c, motion, since, mark);
            assert_eq!(got, want, "motion {motion} clock {since} mark {mark} +0xb7c {b7c:#x}");
            yes += got as u32;
        }
        assert!(yes > 20 && yes < 380, "both answers occur ({yes} of 400)");
        eprintln!("oracle: FUN_00119ce8 equal to scripted::join_ready over 400 rounds ({yes} yes)");
    }
}
