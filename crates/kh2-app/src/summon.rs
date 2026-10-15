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
//! What the app does not have (docs/summons.md "Status"): the friend AI and the group 10 hosts (the
//! object stands where it was put; its script's motion requests are logged, not played, as the party's
//! are in `friend_ai.rs`), the event itself (nothing in the app starts it: the wait for it ends at
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
        /// +0x630 bit 0 (FUN_001175a0 sets it) and +0x640 the vector it copies (game units and axes).
        pub flags_630: u32,
        pub goal: [f32; 4],
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
            SummonAi { slots: [Slot::default(); SLOTS], flags_630: 0, goal: [0.0; 4], action: (0, 0), b7c: 0, flag_7f8_4: false, callbacks_98: Vec::new(), link_calls: (0, 0) }
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
        let consts = disc.read_root("SLUS_210.05").and_then(|exe| SummonConsts::from_exe(&exe).map(|c| (c, kh2_bdx::HostTable::from_exe(&exe))));
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
}

/// The status block handle host 1.84 gives for the player (his +0x48c): host 10.45 reads its +0x1b4.
const PLAYER_GAUGES: u32 = 0x7900_0000;

/// What the summon host reads of the game beside the object.
struct SummonEnv {
    /// The player's status block +0x1b4: the drive gauge's timer (`DriveState::timer.left`).
    timer: f32,
    /// A limit task runs (`LimitTask`, `DAT_01d5d6b0`).
    limit: bool,
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
            // 10.1 (0x2f6a98 -> FUN_001175a0): AI +0x630 |= 1, +0x640 = the vector. The rest (+0x65c, +0x648,
            // FUN_00117320, FUN_00193228, FUN_0017ecf0) belongs to the class update, not ported.
            (10, 1) => {
                self.ai.flags_630 |= 1;
                if let Some(Arg::Vec(v)) = args.get(1) {
                    self.ai.goal = *v;
                }
                (Value::None, St::Approx, "the goal is kept; Stitch's class update (FUN_00116b48) that walks to it is not ported")
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
            (10, 32) => (Value::None, St::Stub, "FUN_00118aa8 (a goal, motion 0xa1 and a record's action) belongs to the class update, not ported"),
            // 10.39 (0x2f72c0 -> FUN_0010ecb0(object, 0, 0)).
            (10, 39) => {
                self.ai.action = (0, 0);
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

/// The summon objects' scripts (run every tick through the enemy host, as the party's in
/// `friend_ai.rs`), their fades and their end.
#[allow(clippy::too_many_arguments)]
fn objects(
    mut commands: Commands,
    h: Res<Harness>,
    world: Option<Res<World>>,
    trace: Option<ResMut<AiTrace>>,
    sora: Query<&Sora>,
    mut q: Query<(Entity, &mut Transform, &mut SummonObject)>,
    anims: Query<&Animator>,
    children: Query<&Children>,
    handles: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    (game, drive, limit_task): (Res<GameData>, Res<DriveState>, Option<Res<crate::limit::LimitTask>>),
) {
    let (Some(mut trace), Ok(s)) = (trace, sora.single()) else { return };
    let paired = HashMap::new();
    let senv = SummonEnv { timer: drive.timer.left, limit: limit_task.is_some_and(|l| l.0.is_some()) };
    for (ent, mut tf, mut so) in &mut q {
        let so = &mut *so;
        let Ok(anim) = anims.get(so.body) else { continue };
        let rig = anim.rig.clone();
        so.obj.slot = anim.slot;
        so.obj.frame = anim.frame;
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
        let SummonObject { rt, obj, tag, started, leave, mode, errors_seen, ai, fields, .. } = so;
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
        drop(host);
        let now = rt.current_mode();
        if now != *mode {
            info!("summon: {tag} script mode {:?} -> {:?}", mode.as_deref().unwrap_or("-"), now.as_deref().unwrap_or("-"));
            *mode = now;
        }
        if let Some(req) = obj.motion_req.take()
            && h.log
        {
            info!("summon: {tag} script asked for motion {} (not applied: the friend AI's motion mapping is stage 4)", req.slot);
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
        tf.rotation = Quat::from_rotation_y(obj.motor.yaw);
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
}
