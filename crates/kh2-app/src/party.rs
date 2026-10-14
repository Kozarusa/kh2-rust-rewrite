//! Party members, stages 1 and 2 of docs/party.md section 9: Donald and Goofy (whoever the world's
//! party names) stand where the area's `map` script puts `FRIEND_1` / `FRIEND_2`, with their models,
//! motion sets and weapons, and follow the player under the friend AI's follow rules: the follow
//! points, the speed modes, the steer, the player's trail and the warp (catch-up). In battle (the
//! game's battle flag, FUN_001903d8) they choose targets and actions and attack (stage 2).
//!
//! Who: the world's party slot bytes start from preset 1 (`party_rules::apply_preset`, the writer of
//! the new-game bytes is a gap, docs/party.md section 10); `FRIEND_1` is slot byte 1 and `FRIEND_2`
//! slot byte 2 (FUN_0016e208 -> FUN_0017cd20), resolved through the world's merged `memt` record.
//! The member takes the first free party slot (FUN_0017cd08) and the follow position slot + 1.
//!
//! Each 60 Hz tick (`party_tick`) runs the party manager (FUN_00111d18: the trail, the follow points)
//! and then, per member that is free (not in an attack motion), the decision loop FUN_00111070: the
//! far counter, out of battle the follow (stage 1), in battle the target (FUN_0010df08, style 0
//! FUN_0010d0c0) and the action choice (FUN_0010fcd0, `party_battle`); the steer; the movement; then
//! the queued attack command (FUN_001112e8 -> FUN_00174cf8 -> FUN_00110790: the entry's motion through
//! the player-type remap FUN_0017da30) and the warp check (FUN_00110eb0). The locomotion motion is the
//! motion id of the speed (0 stand, 1 walk, 2 run) [inferred]; the member turns toward its movement at
//! its `prty` turn speed [inferred].
//!
//! Log: `KH2_PARTYLOG=1` prints speed-mode changes, warps, the targets and the chosen actions, the
//! actions per second and a status line per second; the spawn lines always print. Harness:
//! `KH2_NO_PARTY=1` spawns nobody; `KH2_PARTY_WARP=<s>` at that time asks every member for a warp the
//! way host function 0x2eec20 does (FUN_00110e08: far counter 1000, no cooldown, nearest trail
//! point); `KH2_PARTY_WARP_ONSCREEN=1` lets that forced warp happen in view (a harness aid to see it:
//! the game warps only off screen); `KH2_PARTY_WEAPON_BONE=<n>` overrides the weapons' bone (the bone
//! search aid).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::color::Alpha;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_anim::id;
use kh2_tables::tables as t;

use crate::assets::{AreaPart, CurrentArea, Disc};
use crate::combat::{Fighter, Hit, Team};
use crate::data::GameData;
use crate::motor::Motor;
use crate::party_battle::{self as battle, Choice, ClassAi, Handler, MemberNow};
use crate::party_rules::{self as rules, Consts, Follow, FollowIn, Thresholds, Trail};
use crate::schedule::Sim;
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::sora::Sora;
use crate::world::World;
use crate::SCALE;

/// Blend of the locomotion motions, as Sora's (`sora::BLEND_LOCOMOTION`).
const BLEND: f32 = 8.0;
/// The frame unit the timers count down by (`DAT_00356f40`, one 60 Hz frame).
const FRAME: f32 = 1.0;

pub struct PartyPlugin;

impl Plugin for PartyPlugin {
    fn build(&self, app: &mut App) {
        let data = PartyData::load(app.world().resource::<Disc>());
        app.insert_resource(data)
            .init_resource::<PartySlots>()
            .insert_resource(Manager { log: std::env::var("KH2_PARTYLOG").is_ok_and(|v| !v.is_empty() && v != "0"), rng: 0x2545_f491, ..Default::default() })
            .add_systems(FixedUpdate, (spawn_party, party_tick, party_push_apart, party_fade, party_hits).chain().in_set(PartyTick).in_set(Sim::Enemy))
            .add_systems(FixedUpdate, member_hands.in_set(Sim::Enemy).before(PartyTick))
            .add_systems(FixedUpdate, (party_receive_hits, party_item_uses, party_script_hits).chain().in_set(Sim::SoraHurt))
            .add_systems(FixedUpdate, party_hud.after(Sim::SoraHurt).run_if(crate::schedule::sim_running))
            .add_systems(FixedUpdate, party_forced_hits.in_set(Sim::Detect));
        crate::friend_ai::register(app);
    }
}

/// The party's own systems (spawn, tick, fade); the members' scripts run after them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PartyTick;

/// The tables the weapon lookup reads (FUN_0016ef28 -> FUN_001845b0 `went`, FUN_00184640 `wmst`).
#[derive(Default)]
struct WeaponTables {
    items: Vec<t::Item>,
    /// `item_stat` rows (a weapon's attack and magic, `Item::stat_entry`).
    stats: Vec<t::ItemStat>,
    went: Vec<u8>,
    wmst: Vec<[String; 2]>,
}

/// The disc and executable data the party needs.
#[derive(Resource)]
pub struct PartyData {
    consts: Option<Consts>,
    /// Per AI class, its parameter block and action records (`party_battle::ClassAi`).
    ai: HashMap<u8, ClassAi>,
    /// FUN_0017da30's slot orders (0x37a5c8).
    orders: [[u8; 4]; 4],
    /// 0x3784ac: style 2 halves the battle distance (FUN_0010df08).
    half: f32,
    /// 0x378564 / 0x378568: FUN_0010e5e8's stop when the target is right above.
    stop: (f32, f32),
    objs: Vec<t::Objentry>,
    memt: Option<kh2_tables::player::Memt>,
    prty: Vec<t::PrefPrty>,
    /// The `prty` entry's row offsets (FUN_001cc140 falls back to row 1 for an offset of 0).
    prty_offsets: Vec<u32>,
    /// The `sstm` record (03system.bin): +0x54 the revive heal scale, +0x58 the revive heal, +0x5c the
    /// out-of-battle revive count-down scale, +0x7c the MP-charge feed of an MP gain.
    sstm: Option<kh2_tables::player::Sstm>,
    weapons: WeaponTables,
    /// Stage 4's executable values: FUN_00110f90's MP-gate weights (0x378598, 0x37859c), FUN_0010c800's
    /// degrees to radians (0x3784a0), the item handlers' fractions (0x3785c8 HP, 0x3785d4 MP) and choice
    /// orders (0x352e28 / 0x352e3c HP, 0x352e50 / 0x352e60 MP).
    mp_gate_w: (f32, f32),
    deg_to_rad: f32,
    item_frac: (f32, f32),
    hp_items: (Vec<u16>, Vec<u16>),
    mp_items: (Vec<u16>, Vec<u16>),
    /// Loaded rigs by model name (`None`: not loadable).
    rigs: HashMap<String, Option<Arc<Rig>>>,
}

impl PartyData {
    fn load(disc: &Disc) -> PartyData {
        let exe = disc.read_root("SLUS_210.05");
        let consts = exe.as_deref().and_then(Consts::from_exe);
        if consts.is_none() {
            warn!("party: the friend AI's constants are not readable from SLUS_210.05; nobody follows");
        }
        let mut ai = HashMap::new();
        let (mut orders, mut half, mut stop) = ([[2, 3, 1, 0]; 4], 0.5, (0.0, 0.0));
        let (mut mp_gate_w, mut deg_to_rad, mut item_frac) = ((0.0, 0.0), 0.0, (0.0, 0.0));
        let (mut hp_items, mut mp_items) = ((Vec::new(), Vec::new()), (Vec::new(), Vec::new()));
        if let Some(elf) = exe.as_deref() {
            let g = |va: u32| rules::exe_bytes(elf, va, 4).map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            mp_gate_w = (g(battle::MP_GATE_SLOPE), g(battle::MP_GATE_BASE));
            deg_to_rad = g(battle::DEG_TO_RAD);
            item_frac = (g(battle::HP_ITEM_FRACTION), g(battle::MP_ITEM_FRACTION));
            hp_items = (battle::item_order(elf, battle::HP_ITEMS_ONE, battle::HP_ITEM_CHOICES), battle::item_order(elf, battle::HP_ITEMS_MANY, battle::HP_ITEM_CHOICES));
            mp_items = (battle::item_order(elf, battle::MP_ITEMS_ONE, battle::MP_ITEM_CHOICES), battle::item_order(elf, battle::MP_ITEMS_MANY, battle::MP_ITEM_CHOICES));
            for class in 0..rules::CLASSES as u8 {
                if let Some(c) = ClassAi::from_exe(elf, class) {
                    ai.insert(class, c);
                }
            }
            orders = battle::motion_orders(elf).unwrap_or(orders);
            let f = |va: u32| rules::exe_bytes(elf, va, 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            half = f(0x3784ac).unwrap_or(half);
            stop = (f(0x378564).unwrap_or(0.0), f(0x378568).unwrap_or(0.0));
        }
        let (objs, memt, prty, weapons, prty_offsets, sstm) = match disc.0.lock() {
            Ok(mut a) => (
                kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default(),
                kh2_tables::player::load_memt(&mut a).ok(),
                kh2_tables::load::<t::PrefPrty>(&mut a, &t::PREF_PRTY).map(|d| d.rows).unwrap_or_default(),
                WeaponTables {
                    items: kh2_tables::load::<t::Item>(&mut a, &t::ITEM).map(|d| d.rows).unwrap_or_default(),
                    stats: kh2_tables::load::<t::ItemStat>(&mut a, &t::ITEM_STAT).map(|d| d.rows).unwrap_or_default(),
                    went: kh2_tables::subfile(&mut a, &t::WENT).unwrap_or_default(),
                    wmst: kh2_tables::subfile(&mut a, &t::WMST).map(|b| crate::drive::rules::parse_wmst(&b)).unwrap_or_default(),
                },
                prty_offsets(&mut a),
                kh2_tables::player::load_sstm(&mut a).ok(),
            ),
            Err(_) => (Vec::new(), None, Vec::new(), WeaponTables::default(), Vec::new(), None),
        };
        PartyData { consts, ai, orders, half, stop, objs, memt, prty, prty_offsets, sstm, weapons, mp_gate_w, deg_to_rad, item_frac, hp_items, mp_items, rigs: HashMap::new() }
    }

    /// The record at +0x12c (and the movement block at +0x110): the `prty` row of the object's
    /// `objentry` NeoMoveset (FUN_00196b58 tail-calls FUN_00194438(+0x110, NeoMoveset) -> FUN_001cc1c8 ->
    /// FUN_001cc140 with fallback row 1), which also gives +0x110 walk, +0x114 run, +0x118 jump, +0x11c
    /// turn.
    fn prty_row(&self, moveset: u16) -> Option<&t::PrefPrty> {
        battle::pref_row(&self.prty_offsets, moveset as usize, 1).and_then(|k| self.prty.get(k))
    }

    /// A word of the `sstm` record as f32 (0.0 when the record is missing).
    fn sstm_f32(&self, off: usize) -> f32 {
        self.sstm.as_ref().filter(|s| s.words.len() > off / 4).map_or(0.0, |s| s.f32(off))
    }

    /// FUN_0017da30's slot order of a motion-state row.
    pub(crate) fn order(&self, row: usize) -> [u8; 4] {
        self.orders[row.min(3)]
    }

    /// The model (`obj/<model>.mdlx`) with the motion set its `objentry` row names.
    fn rig(&mut self, disc: &Disc, row: &t::Objentry) -> Option<Arc<Rig>> {
        let model = row.model_name.clone();
        let anim = row.animation_name.trim_end_matches(".mset").to_string();
        self.rigs
            .entry(model.clone())
            .or_insert_with(|| {
                let m = disc.read(&format!("obj/{model}.mdlx")).and_then(|d| kh2_model::mdlx(&d))?;
                let s = disc.read(&format!("obj/{anim}.mset")).and_then(|d| kh2_anim::Mset::parse(&d))?;
                let apdx = Some(format!("obj/{model}.apdx")).filter(|a| disc.read(a).is_some());
                Some(Arc::new(Rig { model: m, mset: s, apdx }))
            })
            .clone()
    }

    /// FUN_0017c718's hand object `hand` of a member: FUN_0016ef28 takes the member's saved weapon
    /// word (`item`), FUN_001845b0 the weapon object of the member's moveset for the item's
    /// `Flag1 | Rank << 8` slot (`went`), FUN_00184640 the hand's motion set name (`wmst`). `None` when
    /// any step names nothing.
    fn weapon_files(&self, row: &t::Objentry, item: u16, hand: usize) -> Option<crate::drive::WeaponFiles> {
        let it = self.weapons.items.iter().find(|i| i.id == item)?;
        let slot = it.flag1 as u16 | (it.rank as u16) << 8;
        let ms = row.neo_moveset as usize;
        let id = crate::drive::rules::weapon_object(&self.weapons.went, ms, slot)?;
        let w = self.objs.iter().find(|w| w.object_id == id)?;
        let mset = self.weapons.wmst.get(ms).and_then(|n| n.get(hand)).filter(|s| !s.is_empty()).cloned();
        Some(crate::drive::WeaponFiles { model: w.model_name.clone(), mset })
    }
}

/// The row offsets of the `prty` entry of 03system.bin's `pref` (u32 count, then the offsets).
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

/// The current world's party: the four slot bytes (`0x340c9c + world x 4`: player, the two
/// companions, the partner; low five bits the member index, 0x10 empty).
#[derive(Resource, Default, Debug)]
pub struct PartySlots {
    pub world: u16,
    pub bytes: [u8; 4],
}

/// The party manager's state (FUN_00112168 / FUN_00111d18).
#[derive(Resource, Default)]
struct Manager {
    area: Option<String>,
    trail: Trail,
    /// `DAT_0034e980`: 2 warps any time, 1 only out of battle, 0 never (FUN_00112270).
    warp_mode: u8,
    tick: u64,
    rng: u32,
    log: bool,
    forced_done: bool,
    in_battle: bool,
}

impl Manager {
    /// A random number below `n` ([inferred] stand-in for FUN_0010f420; the generator's state is not
    /// the game's).
    fn random(&mut self, n: u32) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng % n.max(1)
    }

    /// FUN_0010f438: 0..1 ([inferred] stand-in as above).
    fn roll(&mut self) -> f32 {
        self.random(20001) as f32 / 20000.0
    }

    /// FUN_00106408: a random value in `[a, b]`.
    fn between(&mut self, (a, b): (f32, f32)) -> f32 {
        a + self.roll() * (b - a)
    }
}

/// The member's target (+0x9f4): an enemy fighter and one of its hurt shapes (by index among its
/// BATTLE / TARGET shapes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Target {
    entity: Entity,
    shape: usize,
}

/// An attack, follow-up jump or item motion that runs (the member is not free: +0x160 bits clear).
#[derive(Clone, Copy, Debug)]
struct Attacking {
    /// The action record (`None` for a combo follow-up).
    record: Option<usize>,
    slot: usize,
    /// A follow-up jump ends when the member lands (its jump motion may loop).
    until_landed: bool,
}

/// An item use that runs (FUN_00175e20: +0xb2c the item, +0xb30 the target) until frame trigger 6 of
/// motion 9 (FUN_00188498 case 6 -> FUN_001b8710).
#[derive(Clone, Copy, Debug)]
struct ItemUse {
    item: u16,
    target: battle::Who,
    slot: usize,
}

/// An item taken out of the member's slots, waiting `left` frames before its effect (FUN_001b82b8 with
/// `DAT_0037bba4` = 30, as Sora's `items::USE_WAIT`).
#[derive(Clone, Copy, Debug)]
struct QueuedUse {
    left: f32,
    item: u16,
    target: battle::Who,
}

/// An item handler's two choice orders (one target, several) and the value it compares (HP or MP).
type ItemLists = (Vec<u16>, Vec<u16>);
type VitalPart = fn(&battle::Vital) -> (i32, i32);

/// What a member's target record (+0x9f4) names for a special: a party object or an enemy.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ScriptTarget {
    Party(battle::Who),
    Enemy(Target),
}

/// What a chosen record starts this tick.
enum Start {
    /// FUN_001137d8: the attack command with the entry's motion.
    Attack(usize, battle::Entry),
    /// FUN_0010c800: a combo follow-up jump.
    FollowUp(battle::FollowUp),
    /// FUN_00113888 / FUN_00113b08 phase 0 -> FUN_0010c9d0: an item on a target.
    Item(usize, u16, battle::Who),
    /// FUN_00113d48 phase 0: the script's mode and the pending MP byte (+0xaf4).
    Special(usize, String, u8, Option<ScriptTarget>),
}

/// One party member in the field.
#[derive(Component)]
pub struct PartyMember {
    /// Party slot 0 or 1 (+0xb48); the follow position is slot + 1 (+0xb70).
    pub slot: u8,
    pub neo: u16,
    pub class: u8,
    pub model: String,
    pub body: Entity,
    motor: Motor,
    follow: Follow,
    thresholds: Thresholds,
    /// +0x110 / +0x114 (the `prty` row of the NeoStatus, [inferred]) and the turn rate; +0x118 the jump
    /// height ([inferred] the row's JumpHeight).
    walk: f32,
    run: f32,
    turn: f32,
    jump: f32,
    /// The body shape (+0x5b0 / +0x5b8 radius, +0x5b4 height).
    radius: f32,
    height: f32,
    /// +0xcd8 far counter, +0xcdc warp cooldown, +0xce0 warps since the last trail point.
    pub far: i32,
    pub cooldown: f32,
    pub warps: i32,
    /// +0xb7c bit 1 (a warp's fade-in runs) and bit 0x8000 (the forced warp of FUN_00110e08).
    warping: bool,
    forced: bool,
    /// Frames into the warp's fade-in.
    fade: Option<f32>,
    alpha: f32,
    applied_alpha: f32,
    original_modes: HashMap<AssetId<StandardMaterial>, AlphaMode>,
    /// The last movement step was blocked ([inferred] reading of +0x660 bit 4).
    blocked: bool,
    speed: f32,
    /// Stage 2: the action choice's state, the wait timer (+0xcd4), the target, bit 8 (the target
    /// changed this frame), the held velocity of Goofy's wait, the running attack, the wanted facing
    /// (+0x66c) and the actions chosen since the last per-second line.
    choosing: battle::Choosing,
    timer: f32,
    target: Option<Target>,
    target_changed: bool,
    held: Option<(Vec3, f32)>,
    attack: Option<Attacking>,
    want_yaw: Option<f32>,
    chosen: Vec<String>,
    /// The motion slot the member's script asked for last (FUN_0017e220 remapped), played while the
    /// member stands; `friend_ai` writes it.
    pub(crate) script_slot: Option<usize>,
    /// The script asked for a motion this tick (FUN_0017e220 starts it again even when it plays), and the
    /// motion it queued after it (host 1.13, FUN_0017e170).
    pub(crate) script_restart: bool,
    pub(crate) script_queue: Option<usize>,
    /// FUN_0017da30's state row now (1 battle, 2 field).
    pub(crate) motion_row: usize,
    pub(crate) free: bool,
    /// Stage 3: the status block (+0x48c): HP +0, maximum +4, minimum +8, MP +0x180, maximum +0x184,
    /// the MP charge +0x1bc / +0x1c0.
    pub hp: i32,
    pub hp_max: i32,
    pub mp: i32,
    pub mp_max: i32,
    pub(crate) charge: crate::magic_rules::Charge,
    /// +0x7f8 bit 4 (KO, FUN_00186580), the revive timer +0xb40 and the handle of system effect 0xe
    /// (+0xb44) while the member lies in motion 0x36.
    pub ko: bool,
    pub revive: f32,
    down_fx: Option<crate::fx::FxHandle>,
    /// The `prty` row of the NeoMoveset (+0x12c): StunTime (+0x18, the revive time) and MpDrive (+0x1c,
    /// the MP charge length).
    stun: f32,
    mp_drive: f32,
    /// A hit-reaction motion runs (slot): the member is not free until it ends.
    hurt: Option<usize>,
    /// Stage 4: the MP gate bit of this frame (+0xb7c 0x20, FUN_00110f90), the member in danger (+0xce4,
    /// FUN_0010f830), the item use that runs and the uses waiting for their effect.
    mp_gate: bool,
    danger: Option<battle::Who>,
    using: Option<ItemUse>,
    queued: Vec<QueuedUse>,
    /// FUN_00113d48: the mode the script is asked to switch to (FUN_00186e70 on +0x300; `friend_ai`
    /// carries it out) and the MP cost the script pays through host 1.276 (+0xaf4).
    pub(crate) mode_request: Option<String>,
    pub(crate) pending_mp: u8,
    /// The script's mode now (`friend_ai` writes it): the script is busy (+0x390) while it runs a
    /// special's mode [inferred].
    pub(crate) script_mode: Option<String>,
    /// FUN_00113d48 put the special's (or Cure's) target into +0x9f4; kept while its mode runs.
    special_target: bool,
    /// Who that target record names, and the hits the script's attack objects delivered on it (hosts
    /// 2.10 / 2.17, riend_ai fills them; party_script_hits carries them out).
    special_who: Option<ScriptTarget>,
    pub(crate) script_hits: Vec<i16>,
    /// The point of the target record (+0x9f4) host 1.120 answers: the centre of the hurt shape the
    /// member targets (game units, the app's axes), kept while the member is not free.
    pub(crate) target_point: Option<Vec3>,
}

/// Whom a special's target record names, for the effects bound to its object (host 1.149).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SpecialOn {
    Player,
    Me,
    Other,
    Enemy(Entity),
}

/// Motion ids of FUN_0017ca70 / FUN_0017cee8: lying KO'd (0x36) and getting up (0x37).
const MOTION_DOWN: i32 = 0x36;
const MOTION_GET_UP: i32 = 0x37;
/// `DAT_0037a55c`: the blend of FUN_0017cee8's get-up request (8.0 in the executable; read there).
const GET_UP_BLEND: f32 = 8.0;
/// System effect of a KO'd member (FUN_0017ca70: FUN_001db868(.., 0xe, ..)).
const EFFECT_DOWN: u16 = 0xe;
/// Voice-table ids the member's voice method (+0x54, FUN_00175d18) is asked for: 2 on KO (FUN_00175f10),
/// 9 when its HP enters danger (FUN_0017cde0).
const VOICE_KO: u16 = 2;
const VOICE_DANGER: u16 = 9;

impl PartyMember {
    /// Position (game units, the app's axes) and facing (app yaw).
    pub(crate) fn position(&self) -> Vec3 {
        self.motor.pos
    }

    pub(crate) fn yaw(&self) -> f32 {
        self.motor.yaw
    }

    /// The member's target record (+0x9f4) is valid (host 1.140).
    pub(crate) fn target_valid(&self) -> bool {
        self.special_target || self.target.is_some()
    }

    /// The object of the target record while a special runs (FUN_00113d48 copied phase 1's record).
    pub(crate) fn special_on(&self) -> Option<SpecialOn> {
        Some(match self.special_who? {
            ScriptTarget::Party(battle::Who::Player) => SpecialOn::Player,
            ScriptTarget::Party(battle::Who::Me) => SpecialOn::Me,
            ScriptTarget::Party(battle::Who::Other) => SpecialOn::Other,
            ScriptTarget::Enemy(t) => SpecialOn::Enemy(t.entity),
        })
    }

    /// The status block's HP and MP as the danger test and the item handlers read them.
    fn vital(&self) -> battle::Vital {
        battle::Vital { hp: self.hp, hp_max: self.hp_max, mp: self.mp, mp_max: self.mp_max }
    }

    /// Puts the member at `at` (game units, the app's axes) facing `yaw`, on the ground, with no
    /// movement left: the re-creation of `FUN_0017d290` (`FUN_0010be80(yaw, object, entity, position)`)
    /// after a summon (WP-F1 item 4). `follow_party` writes the transform from the motor.
    pub(crate) fn place(&mut self, at: Vec3, yaw: f32) {
        self.motor.pos = at;
        self.motor.yaw = yaw;
        self.motor.vel = Vec3::ZERO;
        self.motor.grounded = true;
        self.motor.jump = Default::default();
        self.far = 0;
        self.held = None;
        self.attack = None;
        self.want_yaw = None;
    }

    /// The HP method (FUN_00176c98 -> FUN_00186238) on the member's status block; a KO by it sets the
    /// revive timer (FUN_0017ce60). The revive scale is `sstm` +0x54: the ability block's +0x10 bit 8 (1.0
    /// instead) is not built for the members [inferred: clear]. The MP Rage gain of FUN_00176c98 for a
    /// damage is not ported (the members' ability block +0x54 is not built).
    fn change_hp(&mut self, delta: i32, data: &PartyData) -> battle::HpChange {
        let c = battle::hp_change(&mut self.hp, self.hp_max, 0, &mut self.ko, delta, data.sstm_f32(0x54));
        if c.knocked_out {
            // FUN_0017ce60: StunTime of the +0x12c row x the ability block's Hyper Healing (+0x58; the
            // members' block is the default 1.0 [inferred: no Hyper Healing equipped]).
            self.revive = battle::revive_time(self.stun, 1.0);
            self.attack = None;
            self.using = None;
            self.held = None;
        }
        c
    }

    /// FUN_001769e0 -> FUN_00186368 with minus `cost` (host 1.276): nothing while KO'd; MP below 0 starts
    /// the MP charge (FUN_00186440: the `prty` MpDrive of the +0x12c row x `DAT_0037a974`, MP 0),
    /// otherwise MP drops (FUN_001a0180), as Sora's spells pay (`magic_rules::pay`).
    pub(crate) fn pay_mp(&mut self, cost: u8) {
        if self.ko || self.hp_max == 0 {
            return;
        }
        let before = self.mp;
        match crate::magic_rules::pay(self.mp, self.mp_max, cost, self.mp_drive) {
            crate::magic_rules::Paid::Mp(v) => self.mp = v,
            crate::magic_rules::Paid::Charge { total } => {
                self.mp = 0;
                self.charge = crate::magic_rules::Charge { left: total, total };
            }
        }
        info!("party: {} paid {cost} MP (host 1.276): MP {before} -> {}{}", self.model, self.mp, if self.charge.active() { format!(", MP charge {:.0}", self.charge.left) } else { String::new() });
    }

    /// FUN_0017cee8's motion: 0x37 (blend `DAT_0037a55c`) when the member lies in 0x36; it is busy until it
    /// ends.
    fn get_up(&mut self, anims: &mut Query<(&mut Animator, &GlobalTransform)>, order: [u8; 4]) {
        let Ok((mut a, _)) = anims.get_mut(self.body) else { return };
        let rig = a.rig.clone();
        let down = battle::remap(MOTION_DOWN, order, |s| rig.mset.has(s));
        if down.is_some() && a.slot == down {
            let slot = battle::remap(MOTION_GET_UP, order, |s| rig.mset.has(s));
            a.restart(slot, false, GET_UP_BLEND);
            self.hurt = slot;
        }
    }
}

/// The player's objentry id in spawn records (`PLAYER`, FUN_0016e208).
const PLAYER_ENTITY: u32 = 0x236;

/// The bone the members' hand objects hang on, by model: found by looking at the hand on screen with
/// `KH2_PARTY_WEAPON_BONE` (as Sora's 178 was); the game's attach path (FUN_001751b0 -> FUN_00184190)
/// ends in resource handles, not a bone number, and objentry `WeaponJoint` (2 Donald, 3 Goofy) is not a
/// bone index of these models (docs/party.md section 11).
fn weapon_bone(model: &str) -> Option<usize> {
    if let Some(b) = std::env::var("KH2_PARTY_WEAPON_BONE").ok().and_then(|v| v.parse().ok()) {
        return Some(b);
    }
    match model {
        "P_EX020" => Some(DONALD_HAND),
        "P_EX030" => Some(GOOFY_HAND),
        _ => None,
    }
}

/// Donald's and Goofy's weapon bones (`weapon_bone`): the wrist bones with four finger children on the
/// bind pose's -X side, the side of Sora's 178 (bone dump of the models); on screen the staff and the
/// shield sit in the hand and face outward with these, and Goofy's shield turns edge-on into his leg on
/// the other wrist (27).
const DONALD_HAND: usize = 131;
const GOOFY_HAND: usize = 101;

/// Marks a member's weapon socket: the entity follows the bone of the member's body.
#[derive(Component)]
struct MemberHand {
    body: Entity,
    bone: usize,
}

/// The rows of the area's `map` script that place `FRIEND_1` / `FRIEND_2` (and the player's row of
/// the first friend's group): (objentry id, group, position in game units in the app's axes, app
/// yaw, entrance byte +0x1d), in script order, the first row of each object. With `at` (the groups the
/// area scripts loaded and the entrance of `area::AreaRecord`, after a jump or `KH2_ENTRANCE`): only
/// those groups and only the rows `FUN_001981a0` creates for that entrance (WP-E1c).
fn friend_records(disc: &Disc, area: &str, at: Option<(Vec<String>, u8)>) -> Vec<(u32, String, Vec3, f32, u8)> {
    let Some(file) = disc.read(&format!("ard/{area}.ard")) else { return Vec::new() };
    let Some((ard, _)) = kh2_ard::Area::parse(&file) else { return Vec::new() };
    let mut out: Vec<(u32, String, Vec3, f32, u8)> = Vec::new();
    let names: Vec<String> = at.as_ref().map_or_else(|| ard.scripts.get("map").into_iter().flatten().flat_map(|p| p.spawns()).collect(), |a| a.0.clone());
    let made = |e: &&kh2_ard::Entity| at.as_ref().is_none_or(|a| kh2_ard::run::entity_for_entrance(e, a.1));
    for name in names {
        let friends_here = ard.spawns.get(&name).into_iter().flatten().flat_map(|g| &g.entities).filter(made).any(|e| e.object == rules::FRIEND_1 || e.object == rules::FRIEND_2);
        for e in ard.spawns.get(&name).into_iter().flatten().flat_map(|g| &g.entities).filter(made) {
            let wanted = e.object == rules::FRIEND_1 || e.object == rules::FRIEND_2 || (e.object == PLAYER_ENTITY && friends_here);
            if wanted && !out.iter().any(|r| r.0 == e.object) {
                let [x, y, z] = e.position;
                out.push((e.object, name.clone(), Vec3::new(x, -y, -z), std::f32::consts::PI - e.rotation[1], e.raw[0x1d]));
            }
        }
    }
    out
}

fn body_shape(rig: &Rig) -> (f32, f32) {
    rig.model.shapes.iter().find(|s| s.kind == 0 || s.kind == 4).map_or((35.0, 150.0), |s| (s.radius.abs(), s.height.abs()))
}

/// Spawns the members of a newly loaded area (they are area parts: the next area load removes them).
#[allow(clippy::too_many_arguments)]
fn spawn_party(
    mut commands: Commands,
    disc: Res<Disc>,
    area: Res<CurrentArea>,
    world: Option<Res<World>>,
    drive_world: Option<Res<crate::drive::DriveWorld>>,
    progress: Option<Res<crate::progress::Progress>>,
    (mut data, mut slots, mut mgr): (ResMut<PartyData>, ResMut<PartySlots>, ResMut<Manager>),
    mut sora: Query<(&mut Transform, &mut Sora), Without<PartyMember>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut lib: Option<ResMut<crate::fx::EffectLib>>,
    rec: Option<Res<crate::area::AreaRecord>>,
) {
    let Some(w) = world else { return };
    if area.loaded == mgr.area {
        return;
    }
    // WP-E1c: with a real entrance (a jump, `KH2_ENTRANCE`) wait for this area's script run.
    let at = match rec.as_deref().filter(|r| r.place_on_load) {
        Some(r) => match r.run.as_deref().filter(|run| area.loaded.as_ref() == Some(&run.name)) {
            Some(run) => Some((run.script.groups.iter().map(|g| g.tag.clone()).collect::<Vec<_>>(), r.entrance)),
            None => return,
        },
        None => None,
    };
    mgr.area = area.loaded.clone();
    mgr.trail = Trail::default();
    let Some(name) = area.loaded.clone() else { return };
    if std::env::var("KH2_NO_PARTY").is_ok_and(|v| v != "0") {
        return;
    }
    let world_id = drive_world.map_or(2, |d| d.id);
    // FUN_00112270: warp mode 1 in world 8 area 8 and world 13 area 3, else 2 (the area number is
    // the digits of the area's name).
    let area_no: u16 = name.trim_start_matches(|c: char| c.is_ascii_alphabetic()).parse().unwrap_or(0);
    mgr.warp_mode = if matches!((world_id, area_no), (8, 8) | (13, 3)) { 1 } else { 2 };
    let Some(memt) = data.memt.clone() else {
        warn!("party: memt not readable; nobody spawns");
        return;
    };
    let preset = memt.trailing.get(rules::DEFAULT_PRESET).map_or([0, rules::EMPTY, rules::EMPTY, rules::EMPTY], |w| w.to_le_bytes());
    slots.world = world_id;
    slots.bytes = rules::apply_preset([0, rules::EMPTY, rules::EMPTY, rules::EMPTY], preset);
    let merged = crate::drive::rules::memt_merge(&memt, world_id, |_| false);
    info!("party: {name} world {world_id} slot bytes {:02x?} (preset {}), warp mode {}", slots.bytes, rules::DEFAULT_PRESET, mgr.warp_mode);
    let Some(consts) = data.consts.clone() else { return };
    let (player_rec, records): (Vec<_>, Vec<_>) = friend_records(&disc, &name, at).into_iter().partition(|r| r.0 == PLAYER_ENTITY);
    if records.is_empty() {
        info!("party: {name}: the map script places no FRIEND_1 / FRIEND_2");
    }
    // Harness: `KH2_PARTY_AT_RECORD=1` puts Sora at the player's row of the friends' group (the app
    // starts him at its own spawn point; the game places the player at his row, docs/events.md). A
    // test aid, not the game's rule.
    if std::env::var("KH2_PARTY_AT_RECORD").is_ok_and(|v| v == "1")
        && let (Some(r), Ok((mut tf, mut s))) = (player_rec.first(), sora.single_mut())
    {
        let at = r.2.with_y(w.ground(r.2, r.2.y + 100.0, 400.0).map_or(r.2.y, |g| g.y));
        crate::sora::respawn(&mut s, at);
        s.motor.yaw = r.3;
        tf.translation = at * SCALE;
        info!("party: Sora placed at the player's row of {} at {at:?}", r.1);
    }
    let mut next_slot = 0u8;
    for (entity, group, at, yaw, entrance) in records {
        let byte = slots.bytes[if entity == rules::FRIEND_1 { 1 } else { 2 }];
        let Some(obj) = rules::slot_object(&merged, byte) else {
            info!("party: {group}: object {entity:#x}: slot byte {byte:#04x} names nobody");
            continue;
        };
        let Some(row) = data.objs.iter().find(|r| r.object_id == obj as u32).cloned() else { continue };
        let Some(class) = rules::ai_class(row.neo_status, row.neo_moveset) else { continue };
        let Some(rig) = data.rig(&disc, &row) else {
            warn!("party: {}: model or motion set {} not loadable", row.model_name, row.animation_name);
            continue;
        };
        let (radius, height) = body_shape(&rig);
        // +0x110..+0x11c and the record at +0x12c: the `prty` row of the NeoMoveset (FUN_00194438).
        let prty = data.prty_row(row.neo_moveset);
        let (walk, run, turn, jump) = prty.map_or((0.0, 0.0, 0.0, 0.0), |p| (p.walk_speed, p.run_speed, p.turn_speed, p.jump_height));
        let (stun, mp_drive) = prty.map_or((0.0, 0.0), |p| (p.stun_time, p.mp_drive));
        // The status block (FUN_0019fd30): rebuilt from the saved party record; a new block gets its HP
        // through FUN_001a0140 [inferred: the maximum], and MP at its maximum [inferred].
        let rec = progress.as_ref().and_then(|p| p.member(row.neo_status));
        let (hp_max, mp_max) = rec.as_ref().map_or((0, 0), |r| (r.hp_max as i32, r.mp_max as i32));
        let ground = w.ground(at, at.y + 100.0, 400.0).map_or(at.y, |g| g.y);
        let mut motor = Motor { pos: at.with_y(ground), yaw, grounded: true, ..Motor::default() };
        motor.capsule.radius = radius;
        let slot = next_slot;
        next_slot += 1;
        let root = commands.spawn((AreaPart, Transform::from_translation(motor.pos * SCALE).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default())).id();
        let body = spawn_skinned(&mut commands, root, rig.clone(), &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
        // The member's own effect file (the object's +0x80): its scripts' effects (hosts 1.22, 1.87,
        // 1.149) and its attack objects' hit sparks (2.22) come from it.
        if let Some(lib) = lib.as_deref_mut() {
            crate::drive::register_effects(&disc, lib, &rig);
        }
        // FUN_0017c718: the hand objects. Hand 0 is the member's saved weapon word (FUN_0016ee98).
        // The app's new game leaves this word 0 (its writer in the original was not found): no hand
        // object then, as FUN_0016ee98 answers 0. `KH2_PARTY_WEAPONS=<Donald item>,<Goofy item>` is a
        // harness aid that hands in an item for a zero word.
        let saved = progress.as_ref().and_then(|p| p.member(row.neo_status)).map_or(0, |m| m.weapons[0]);
        let aid = std::env::var("KH2_PARTY_WEAPONS").ok().and_then(|v| v.split(',').nth(row.neo_status.saturating_sub(2) as usize).and_then(|s| s.trim().parse::<u16>().ok()));
        let item = if saved != 0 { saved } else { aid.unwrap_or(0) };
        let mut weapons = Vec::new();
        match (data.weapon_files(&row, item, 0), weapon_bone(&row.model_name)) {
            (Some(files), Some(bone)) => match crate::drive::weapon_rig(&disc, &files) {
                Some(wrig) => {
                    // The weapon's own effect file (hit sparks come from the attacker's weapon, FUN_0018d090).
                    if let Some(lib) = lib.as_deref_mut() {
                        crate::drive::register_effects(&disc, lib, &wrig);
                    }
                    let socket = commands.spawn((Transform::default(), Visibility::default(), ChildOf(body), MemberHand { body, bone })).id();
                    weapons.push(spawn_skinned(&mut commands, socket, wrig, &mut meshes, &mut materials, &mut images, &mut bindposes, 1.0));
                    info!("party: {} weapon {} (item {item}, motion set {:?}) on bone {bone}", row.model_name, files.model, files.mset);
                }
                None => warn!("party: {}: weapon {} not loadable", row.model_name, files.model),
            },
            (Some(files), None) => info!("party: {}: weapon {} not attached (no hand bone known for this model)", row.model_name, files.model),
            (None, _) => info!("party: {}: no weapon (item {item}: no went entry for moveset {})", row.model_name, row.neo_moveset),
        }
        info!(
            "party: spawn {} slot {slot} ({} at {group} entrance {entrance}, NeoStatus {}, AI class {class}, walk {walk} run {run}, radius {radius}) at {:?}",
            row.model_name,
            if entity == rules::FRIEND_1 { "FRIEND_1" } else { "FRIEND_2" },
            row.neo_status,
            motor.pos
        );
        let mut attack_bodies = vec![body];
        attack_bodies.extend(weapons.iter().copied());
        commands.entity(root).insert((
            PartyMember {
                slot,
                neo: row.neo_status,
                class,
                model: row.model_name.clone(),
                body,
                motor,
                follow: Follow::default(),
                thresholds: consts.thresholds(class),
                walk,
                run,
                turn,
                jump,
                radius,
                height,
                far: 0,
                cooldown: 0.0,
                warps: 0,
                warping: false,
                forced: false,
                fade: None,
                alpha: 1.0,
                applied_alpha: 1.0,
                original_modes: HashMap::new(),
                blocked: false,
                speed: 0.0,
                choosing: Default::default(),
                timer: 0.0,
                target: None,
                target_changed: false,
                held: None,
                attack: None,
                want_yaw: None,
                chosen: Vec::new(),
                script_slot: None,
                script_restart: false,
                script_queue: None,
                motion_row: battle::motion_state(false, false),
                free: true,
                // Harness: `KH2_PARTY_HP=<n>` starts the members with n HP (to see a KO soon).
                hp: std::env::var("KH2_PARTY_HP").ok().and_then(|v| v.parse::<i32>().ok()).map_or(hp_max, |v| v.clamp(1, hp_max.max(1))),
                hp_max,
                mp: mp_max,
                mp_max,
                charge: Default::default(),
                ko: false,
                revive: 0.0,
                down_fx: None,
                stun,
                mp_drive,
                hurt: None,
                mp_gate: false,
                danger: None,
                using: None,
                queued: Vec::new(),
                mode_request: None,
                pending_mp: 0,
                script_mode: None,
                special_target: false,
                special_who: None,
                script_hits: Vec::new(),
                target_point: None,
            },
            // The members' attacks hit enemies (stage 2) and enemies' attacks hit them (stage 3,
            // `party_receive_hits`).
            Fighter { team: Team::Player, attack_bodies, hurt_body: body, landed: Default::default(), hittable: true },
        ));
        if next_slot >= 2 {
            break;
        }
    }
}

/// Keeps every member's weapon socket on its hand bone (the joint entities are siblings under the
/// body, as Sora's `sora::follow_hand`), and plays the body's slot on the weapon (the weapons'
/// motion sets mirror the body's slots, as Sora's `sora::sync_weapon`).
///
/// The hand objects show only while the battle flag is set [inferred from the user's reference video
/// `10_menu_open_close.mp4` at 0.0 s: out of battle Donald and Goofy hold nothing; the game's switch
/// (FUN_001752a0 and +0x588 bit 4 select the weapon rows of FUN_0017da30) was not traced further].
fn member_hands(flag: Option<Res<crate::audio_bgm::BattleFlag>>, mut sockets: Query<(&MemberHand, &mut Transform, &mut Visibility, &Children)>, mut anims: Query<&mut Animator>) {
    let shown = if flag.is_some_and(|f| f.active()) { Visibility::Inherited } else { Visibility::Hidden };
    for (h, mut t, mut vis, kids) in &mut sockets {
        if *vis != shown {
            *vis = shown;
        }
        let Ok(a) = anims.get(h.body) else { continue };
        *t = Transform::from_matrix(a.pose.get(h.bone).copied().unwrap_or(Mat4::IDENTITY));
        let (slot, frame, prev, looping, speed) = (a.slot, a.frame, a.prev_frame, a.looping, a.speed);
        for k in kids.iter() {
            let Ok(mut w) = anims.get_mut(k) else { continue };
            let slot = slot.filter(|&x| w.rig.mset.has(x));
            if w.slot != slot {
                w.restart(slot, looping, 0.0);
            }
            w.frame = frame;
            w.prev_frame = prev;
            w.looping = looping;
            w.speed = speed;
        }
    }
}

/// Whether a box (game units, app axes: centre of the base, radius, height) shows on screen
/// ([inferred] stand-in for FUN_00146648 on the view 0x3b38d0 with the model's box +0x740..+0x754).
fn on_screen(cam: Option<(&Camera, &GlobalTransform)>, base: Vec3, r: f32, h: f32) -> bool {
    let Some((c, g)) = cam else { return false };
    [-r, r]
        .into_iter()
        .flat_map(|x| [-r, r].into_iter().flat_map(move |z| [0.0, h].map(|y| Vec3::new(x, y, z))))
        .filter_map(|o| c.world_to_ndc(g, (base + o) * SCALE))
        .any(|n| n.x.abs() <= 1.0 && n.y.abs() <= 1.0 && (0.0..=1.0).contains(&n.z))
}

/// A target's hurt shape now: centre (game units, app axes) and radius; `None` when the fighter is
/// gone, not hittable or has no such shape ([inferred] mapping of FUN_00112ac0's validity).
fn shape_of(t: Target, fighters: &[(Entity, Vec<(Vec3, f32)>)]) -> Option<(Vec3, f32)> {
    fighters.iter().find(|f| f.0 == t.entity).and_then(|f| f.1.get(t.shape).copied())
}

/// The relation FUN_0010b428 keeps (+0xb80 player, +0xbb0 target): the GAME's height difference
/// (target Y minus own Y with Y down), the ground direction and distance (the 3D parts are not used).
#[derive(Clone, Copy, Debug, Default)]
struct Rel {
    dy: f32,
    gdir: Vec3,
    gd: f32,
}

fn relation(from: Vec3, to: Vec3) -> Rel {
    let d = to - from;
    Rel { dy: -(to.y - from.y), gdir: d.with_y(0.0).normalize_or_zero(), gd: d.with_y(0.0).length() }
}

/// One tick of the party manager and of every member.
#[allow(clippy::too_many_arguments)]
fn party_tick(
    data: Res<PartyData>,
    game: Res<GameData>,
    mut mgr: ResMut<Manager>,
    world: Option<Res<World>>,
    flag: Option<Res<crate::audio_bgm::BattleFlag>>,
    progress: Option<Res<crate::progress::Progress>>,
    sora: Query<&Sora>,
    cam: Query<(&Camera, &GlobalTransform), crate::menu::preview::GameCamera>,
    fighters: Query<(Entity, &Fighter), Without<PartyMember>>,
    mut q: Query<(&mut Transform, &mut PartyMember)>,
    mut anims: Query<(&mut Animator, &GlobalTransform)>,
    mut fx: (MessageWriter<crate::fx::StartBoundSystemEffect>, MessageWriter<crate::fx::StopEffect>),
) {
    let (Some(c), Some(w), Ok(s)) = (data.consts.as_ref(), world, sora.single()) else { return };
    if q.is_empty() {
        return;
    }
    mgr.tick += 1;
    let log = mgr.log;
    let status = log && mgr.tick.is_multiple_of(60);
    let cam = cam.single().ok();
    let (player, player_yaw) = (s.motor.pos, s.motor.yaw);
    // FUN_001903d8: the battle flag.
    let in_battle = flag.as_ref().is_some_and(|f| f.active());
    if in_battle != mgr.in_battle {
        mgr.in_battle = in_battle;
        if log {
            info!("party: battle flag {}", if in_battle { "set: the members fight" } else { "clear: the members follow" });
        }
    }
    // The enemies' hurt shapes (centre in game units, radius): the members' possible targets.
    let enemies: Vec<(Entity, Vec<(Vec3, f32)>)> = fighters
        .iter()
        .filter(|(_, f)| f.team == Team::Enemy && f.hittable)
        .filter_map(|(e, f)| {
            let (a, g) = anims.get(f.hurt_body).ok()?;
            let shapes = a.rig.model.shapes.iter().filter(|s| s.kind == kh2_model::collision::kind::BATTLE || s.kind == kh2_model::collision::kind::TARGET);
            Some((e, shapes.map(|s| crate::combat::hurt_spheroid(s, a, g)).map(|h| (h.center / SCALE, h.radius / SCALE)).collect()))
        })
        .collect();

    // FUN_00110e08 by harness: at the given second every member asks for a forced warp.
    if !mgr.forced_done && let Some(at) = std::env::var("KH2_PARTY_WARP").ok().and_then(|v| v.parse::<f32>().ok()) && mgr.tick as f32 >= at * 60.0 {
        mgr.forced_done = true;
        for (_, mut m) in &mut q {
            m.far = 1000;
            m.cooldown = 0.0;
            m.forced = true;
            info!("party: {} forced warp requested (far counter 1000)", m.model);
        }
    }
    let warp_on_screen = std::env::var("KH2_PARTY_WARP_ONSCREEN").is_ok_and(|v| v == "1");

    // FUN_00111d18: the trail (a new point clears every member's warp count), then the follow points.
    if mgr.trail.push(player, c.trail_step2) {
        for (_, mut m) in &mut q {
            m.warps = 0;
        }
    }
    let mut inputs = [None; 2];
    for (_, m) in q.iter() {
        if let Some(i) = inputs.get_mut(m.slot as usize) {
            *i = Some((m.motor.pos, rules::follow_distance(&m.thresholds, m.radius, c)));
        }
    }
    let points = rules::follow_points(player, player_yaw, inputs, c);
    // The status blocks the danger test and the item handlers read: the player's and each member's.
    let player_vital = battle::Vital { hp: s.hp, hp_max: s.stats.max_hp, mp: s.mp, mp_max: game.sora_max_mp };
    let vitals: Vec<(u8, battle::Vital)> = q.iter().map(|(_, m)| (m.slot, m.vital())).collect();

    for (mut tf, mut m) in &mut q {
        let m = &mut *m;
        let pos = m.motor.pos;
        let to_player = player - pos;
        let ground_dir = to_player.with_y(0.0).normalize_or_zero();
        let before_mode = m.follow.mode;
        let facing = Vec3::new(m.motor.yaw.sin(), 0.0, m.motor.yaw.cos());
        m.motion_row = battle::motion_state(in_battle, false);
        let order = data.orders[m.motion_row];
        // FUN_001112e8: the timers count down by the frame unit.
        if m.cooldown > 0.0 {
            m.cooldown -= FRAME;
        }
        if m.timer > 0.0 {
            m.timer -= FRAME;
        }
        // FUN_00175f48 -> FUN_001a0540: the MP charge runs down (the ability block's MP regeneration
        // +0x34 is the default 1.0 for the members [inferred]); at its end MP refills (FUN_001a05a8).
        if m.charge.tick(1.0) {
            m.mp = m.mp_max;
            info!("party: {} MP charge over: MP {}", m.model, m.mp);
        }
        let mut delta = Vec3::ZERO;
        let mut start: Option<Start> = None;
        // A reaction or get-up motion that ended frees the member.
        if let Some(slot) = m.hurt
            && anims.get(m.body).is_ok_and(|(a, _)| a.finished || a.slot != Some(slot))
        {
            m.hurt = None;
        }
        // FUN_0017ca70 (the friend base's tick, vtable +0xc): a KO'd member's revive count-down; at 0
        // FUN_0017cc20 heals `maxHP x sstm +0x58` through the HP method, which revives (FUN_0017cee8).
        if m.ko && battle::revive_step(&mut m.revive, FRAME, in_battle, data.sstm_f32(0x5c)) {
            let heal = battle::revive_heal(m.hp_max, data.sstm_f32(0x58));
            let c = m.change_hp(heal, &data);
            info!("party: {} revive: timer out, heal {heal} (maxHP {} x sstm +0x58), HP now {} of {}", m.model, m.hp_max, m.hp, m.hp_max);
            if c.revived {
                m.get_up(&mut anims, order);
            }
        }
        if m.ko {
            // While KO'd and free on the ground (FUN_001b7e00, not FUN_0019aa28): effect 0xe once and
            // motion 0x36 (FUN_0017e2d8 asks only when it is not already playing).
            if m.hurt.is_none()
                && m.attack.is_none()
                && m.motor.grounded
                && let Ok((mut a, _)) = anims.get_mut(m.body)
            {
                if m.down_fx.is_none() {
                    let handle = crate::fx::FxHandle::new();
                    fx.0.write(crate::fx::StartBoundSystemEffect { id: EFFECT_DOWN, owner: m.body, handle });
                    m.down_fx = Some(handle);
                }
                let rig = a.rig.clone();
                let slot = battle::remap(MOTION_DOWN, order, |s| rig.mset.has(s));
                if a.slot != slot {
                    a.play(slot, true, BLEND);
                    if log {
                        info!("party: {} lies down: motion {MOTION_DOWN:#x} -> slot {slot:?}, system effect {EFFECT_DOWN:#x}", m.model);
                    }
                }
            }
        } else if let Some(h) = m.down_fx.take() {
            // Out of motion 0x36: the effect is told to end (FUN_001d9f20 -> FUN_001e0298).
            fx.1.write(crate::fx::StopEffect { handle: h, kill: false });
            // A revive by another heal (an item, Cure) gets up as well (FUN_0017cee8).
            m.get_up(&mut anims, order);
        }
        m.free = m.attack.is_none() && m.hurt.is_none() && !m.ko;
        if m.free {
            // FUN_00111070: the far counter (a blocked step [inferred] and a height difference over far_dy).
            if m.blocked {
                m.far += 1;
            }
            if to_player.y.abs() > c.far_dy {
                m.far += 1;
            }
            let ai = data.ai.get(&m.class);
            // FUN_0010df08: bit 0x40 when the member is farther from the player than the battle
            // distance (style 0: the class's +0x80); then, in battle, the target (style 0, FUN_0010d0c0)
            // when the re-target test asks for one (FUN_0010cf90 -> FUN_0010cbe0).
            let rel_player = relation(pos, player);
            let too_far = ai.is_some_and(|a| battle::battle_distance(a.params.battle, 0, data.half) < rel_player.gd);
            m.target_changed = false;
            if in_battle {
                let valid = m.target.and_then(|t| shape_of(t, &enemies));
                let retarget = match (valid, ai) {
                    (Some((ct, _)), Some(a)) => battle::retarget_style0(false, (ct - pos).length(), a.params.retarget),
                    _ => true,
                };
                if retarget && let Some(a) = ai {
                    let all = enemies.iter().enumerate().flat_map(|(fi, (_, shapes))| shapes.iter().enumerate().map(move |(si, &(ct, r))| (fi * 256 + si, ct, r)));
                    let new = battle::nearest_target(pos, a.params.range, all).map(|k| Target { entity: enemies[k / 256].0, shape: k % 256 });
                    if new != m.target {
                        m.target_changed = true;
                        if log {
                            match new {
                                Some(t) => info!("party: {} target {:?} shape {} ({:.0} away)", m.model, t.entity, t.shape, shape_of(t, &enemies).map_or(0.0, |(ct, _)| (ct - pos).length())),
                                None => info!("party: {} no target within {:.0}", m.model, a.params.range),
                            }
                        }
                    }
                    m.target = new;
                }
            } else {
                m.target = None;
            }
            let tshape = m.target.and_then(|t| shape_of(t, &enemies));
            m.target_point = tshape.map(|(ct, _)| ct);
            let rel_t = tshape.map_or(Rel::default(), |(ct, _)| relation(pos, ct));
            // FUN_0010ca88: a valid target and bit 0x40 clear.
            let target_ok = tshape.is_some() && !too_far;
            let (dir, speed);
            // The script is busy with a special: the member holds still so its cast motion plays.
            let mut hold = false;
            m.want_yaw = None;
            // FUN_001102c8 (the player relation) or FUN_001103e0 (the target), as FUN_001103f8 picks.
            let follow_player = |m: &mut PartyMember, row: usize| {
                let point = points.get(m.slot as usize).copied().flatten().unwrap_or(player);
                let input = FollowIn { to_point: point - pos, to_player: to_player.normalize_or_zero(), dist: to_player.length(), ground_dir, facing, walk: m.walk, run: m.run };
                rules::follow_step_row(&mut m.follow, &input, &m.thresholds, row, c)
            };
            let toward = |m: &mut PartyMember| {
                if target_ok {
                    battle::approach_step(&mut m.follow, rel_t.gdir, rel_t.gd, rel_t.dy, &m.thresholds, m.walk, m.run, data.stop)
                } else {
                    follow_player(m, rules::ROW_BATTLE_PLAYER)
                }
            };
            if !in_battle || ai.is_none_or(|a| a.records.is_empty()) {
                (dir, speed) = follow_player(m, if in_battle { rules::ROW_BATTLE_PLAYER } else { rules::ROW_FIELD });
            } else {
                let ai = ai.unwrap();
                // FUN_00110f90: the MP gate of this frame; FUN_0010f830: the member in danger.
                let roll = mgr.roll();
                m.mp_gate = battle::mp_gate(roll, ai.params.mp_chance, m.mp, m.mp_max, data.mp_gate_w.0, data.mp_gate_w.1);
                let me_v = m.vital();
                let other_v = vitals.iter().find(|(sl, _)| *sl != m.slot).map(|(_, v)| *v);
                let danger = battle::danger_member(player_vital, me_v, other_v, false);
                if log && danger != m.danger {
                    info!("party: {} member in danger (FUN_0010f830): {danger:?}", m.model);
                }
                m.danger = danger;
                // +0x390: the script runs a special's mode (or is asked to) [inferred].
                let script_busy = m.mode_request.is_some() || ai.records.iter().any(|r| !r.mode.is_empty() && m.script_mode.as_deref() == Some(r.mode.as_str()));
                if !script_busy {
                    m.special_target = false;
                }
                // FUN_0010f8d8 sets the MP gate before its tests.
                let emergency_now = danger.is_some() && !script_busy;
                if emergency_now {
                    m.mp_gate = true;
                }
                let now = MemberNow {
                    standing: m.motor.grounded,
                    free: true,
                    status: true,
                    mp: m.mp,
                    mp_gate: m.mp_gate,
                    mp_charge: m.charge.active(),
                    target_ok,
                    dy: rel_t.dy,
                    gd: rel_t.gd,
                    facing_dot: facing.dot(rel_t.gdir),
                    radius: tshape.map_or(0.0, |s| s.1),
                };
                let member = progress.as_ref().and_then(|p| p.member(m.neo));
                // The setting bytes (party record +0xd4) are zero at a new game (`Progress::init_party`).
                let setting = |_: usize| Some(0u8);
                // Harness: `KH2_PARTY_ABILITIES=<item>,...` counts those ability items as equipped on every
                // member (the new game's records hold only Donald Thunder and Cure; Fire 165, Blizzard 166,
                // Goofy's specials need a later record). A test aid, not the game's rule.
                let aid = |item: u32| std::env::var("KH2_PARTY_ABILITIES").is_ok_and(|v| v.split(',').any(|s| s.trim().parse::<u32>().ok() == Some(item)));
                let equipped = |r: &battle::Record| aid(r.item) || member.as_ref().is_some_and(|mm| mm.abilities.iter().any(|&(id, on)| on && id as u32 == r.item));
                let skip = battle::repeated(m.choosing.last, m.class);
                let any_attack = |skip: Option<usize>| ai.records.iter().enumerate().any(|(i, r)| r.flags & battle::flag::ATTACK != 0 && Some(i) != skip && !matches!(r.handler, Handler::Other(0)) && r.handler == Handler::Attack && battle::feasible(r, &now, setting(i), equipped(r)));
                let (current, timer, changed) = (m.choosing.current, m.timer, m.target_changed);
                // FUN_0010f510 -> FUN_001a1d50: the member stands, is free and holds the item in its own
                // battle item slots.
                let stock = |item: u16| now.standing && member.as_ref().is_some_and(|mm| mm.items.contains(&item));
                // The item handlers' phase 1 (FUN_00113888 HP, FUN_00113b08 MP): target and item.
                let item_choice = |h: Handler| -> Option<(battle::Who, u16)> {
                    let (frac, lists, value): (f32, &ItemLists, VitalPart) = match h {
                        Handler::HpItem => (data.item_frac.0, &data.hp_items, |v| (v.hp, v.hp_max)),
                        _ => (data.item_frac.1, &data.mp_items, |v| (v.mp, v.mp_max)),
                    };
                    battle::item_pick(danger, me_v, player_vital, other_v, frac, value, &lists.0, &lists.1, |_| 0, stock)
                };
                // FUN_00113d48 phase 1: a mode to switch to, the script free to take it, and FUN_0010f220.
                let special_ok = |i: usize, r: &battle::Record| !r.mode.is_empty() && !script_busy && battle::feasible(r, &now, setting(i), equipped(r));
                let can = |i: usize, cont: bool| {
                    let r = &ai.records[i];
                    let waited = !(changed || timer <= 0.0);
                    match r.handler {
                        Handler::Attack => battle::feasible(r, &now, setting(i), equipped(r)),
                        Handler::Approach => (if cont { waited } else { current != Some(i) }) && !any_attack(skip),
                        Handler::Wait => (!cont || waited) && !any_attack(skip),
                        Handler::GoofyWait => (!cont || waited) && target_ok,
                        Handler::HpItem | Handler::MpItem => battle::feasible(r, &now, setting(i), equipped(r)) && item_choice(r.handler).is_some(),
                        Handler::Special => special_ok(i, r),
                        // FUN_00113e68: as the special, when someone's HP is below the entry's +0x3c.
                        Handler::Cure => special_ok(i, r) && r.entry.is_some_and(|e| battle::cure_target(danger, me_v, player_vital, other_v, e.cure).is_some()),
                        Handler::Other(_) => false,
                    }
                };
                let chain = m.motor.grounded && target_ok;
                let mut choosing = std::mem::take(&mut m.choosing);
                let choice = {
                    let mgr = &mut *mgr;
                    let rng = std::cell::RefCell::new(mgr);
                    // FUN_0010f8d8 first when someone is in danger; FUN_0010fcd0 when it found nothing.
                    // FUN_0010fcd0 and FUN_0010f8d8 (FUN_0010f698) do nothing while the script is busy
                    // (+0x390): the member stands and the script's motion plays.
                    let urgent = if script_busy {
                        None
                    } else if emergency_now { battle::emergency(&ai.records, &mut choosing, true, now.mp_charge, |i| can(i, false), |n| rng.borrow_mut().random(n)) } else { None };
                    match urgent {
                        Some(k) => {
                            if log {
                                info!("party: {} emergency pass (FUN_0010f8d8) chose record {k} {:?}", m.model, ai.records[k].handler);
                            }
                            Choice::Do(k)
                        }
                        None if script_busy => Choice::Busy,
                        None => battle::choose(&ai.records, &mut choosing, &ai.params, m.class, &now, false, chain, setting, can, || rng.borrow_mut().roll(), |n| rng.borrow_mut().random(n)),
                    }
                };
                m.choosing = choosing;
                let record = match choice {
                    Choice::Keep(i) | Choice::Do(i) => Some(i),
                    _ => None,
                };
                if let Choice::Do(i) = choice {
                    let r = &ai.records[i];
                    m.chosen.push(format!("{:?}#{i}", r.handler));
                    if log {
                        info!(
                            "party: {} chose record {i} {:?} id {:#x}{} (target {})",
                            m.model,
                            r.handler,
                            r.id,
                            r.entry.map_or(String::new(), |e| format!(" motion {:#x}{}", e.motion, if e.air != 0.0 { " (air)" } else { "" })),
                            if target_ok { format!("{:.0} away, dy {:.0}", rel_t.gd, rel_t.dy) } else { "none".into() }
                        );
                    }
                }
                // A combo follow-up (FUN_0010fcd0 -> FUN_0010c800 with the class's entry k): the jump.
                if let Choice::FollowUp(k) = choice
                    && let Some(f) = ai.followups.get(k as usize)
                {
                    m.chosen.push(format!("FollowUp#{k}"));
                    if log {
                        info!("party: {} combo follow-up {k} (FUN_0010c800): angle {} height {} speed {} frames {}", m.model, f.angle, f.height, f.speed, f.frames);
                    }
                    start = Some(Start::FollowUp(*f));
                }
                let continuing = matches!(choice, Choice::Keep(_));
                match (choice, record.map(|i| &ai.records[i])) {
                    (_, Some(r)) => match r.handler {
                        Handler::Attack => {
                            // FUN_001137d8 phase 0: FUN_001103f8, then the command (FUN_0010c8f0).
                            (dir, speed) = toward(m);
                            if let Some(e) = r.entry {
                                start = Some(Start::Attack(record.unwrap(), e));
                            }
                        }
                        Handler::HpItem | Handler::MpItem => {
                            // Phase 0: FUN_0010c9d0 with the target and item phase 1 found.
                            (dir, speed) = (facing, 0.0);
                            if let Some((who, item)) = item_choice(r.handler) {
                                start = Some(Start::Item(record.unwrap(), item, who));
                            }
                        }
                        Handler::Special | Handler::Cure => {
                            // FUN_00113d48 phase 0: the target kept, +0xaf4 = the entry's MP byte, the
                            // script's mode switched to the record's name.
                            (dir, speed) = (facing, 0.0);
                            // Phase 1's record: Cure's is FUN_00113e68's (the member in danger or below +0x3c), the others' the member's target.
                            let who = if r.handler == Handler::Cure { r.entry.and_then(|e| battle::cure_target(danger, me_v, player_vital, other_v, e.cure)).map(ScriptTarget::Party) } else { m.target.map(ScriptTarget::Enemy) };
                            start = Some(Start::Special(record.unwrap(), r.mode.clone(), r.entry.map_or(0, |e| e.mp as u8), who));
                        }
                        Handler::Approach => {
                            // FUN_001135e0 phase 0: the wait, the facing toward the target, standing.
                            if !continuing {
                                let w = mgr.between(ai.params.wait_a);
                                m.timer = w;
                            }
                            if target_ok {
                                m.want_yaw = Some(rel_t.gdir.x.atan2(rel_t.gdir.z));
                            }
                            (dir, speed) = (facing, 0.0);
                        }
                        Handler::Wait => {
                            if !continuing {
                                let w = mgr.between(ai.params.wait_b);
                                m.timer = w;
                            }
                            (dir, speed) = toward(m);
                        }
                        Handler::GoofyWait => {
                            // FUN_00114738 phase 0: continuing, the held velocity; else the wait, the move,
                            // and the move's direction held at the run speed.
                            if continuing && let Some(h) = m.held {
                                (dir, speed) = h;
                            } else {
                                let w = mgr.between(ai.params.wait_a);
                                m.timer = w;
                                (dir, speed) = toward(m);
                                m.held = Some((dir, m.run));
                            }
                        }
                        _ => (dir, speed) = toward(m),
                    },
                    (Choice::Busy, _) => {
                        (dir, speed) = (facing, 0.0);
                        hold = true;
                    }
                    (Choice::Stand, _) => {
                        (dir, _) = toward(m);
                        speed = 0.0;
                        if dir != Vec3::ZERO {
                            m.want_yaw = Some(dir.x.atan2(dir.z));
                        }
                    }
                    _ => (dir, speed) = toward(m),
                }
            }
            // FUN_00110468: the steer.
            let (d, sp) = rules::steer(&mut m.follow, dir, speed, m.walk, m.run, c);
            // [inferred] The steer's six-frame average would keep a member that just stopped walking for a
            // few frames, which replaces the script's motion; the game's busy member does not move.
            let sp = if hold { 0.0 } else { sp };
            if log && m.follow.mode != before_mode {
                info!("party: {} speed mode {before_mode} -> {} (distance to the player {:.0}, speed {sp:.1})", m.model, m.follow.mode, to_player.length());
            }
            m.speed = sp;
            delta = d.with_y(0.0).normalize_or_zero() * sp;
        } else if m.attack.is_none() {
            // KO'd or in a reaction / get-up motion: no movement of its own.
            m.speed = 0.0;
        }
        // What the choice started: the queued attack command (FUN_001112e8 -> FUN_00174cf8 ->
        // FUN_00110790: the entry's motion through the player-type remap in the battle row; an air entry
        // jumps toward the target, height `air` minus the height difference held between 0 and the jump
        // height, at the run speed); a combo follow-up's jump (FUN_0010c800); an item use (FUN_0010c9d0 ->
        // FUN_00175e20: motion 9, blend 8.0); a special's mode switch (FUN_00113d48).
        let started = start.is_some();
        match start {
            Some(Start::Attack(rec, e)) => {
                if let Ok((mut a, _)) = anims.get_mut(m.body) {
                    let rig = a.rig.clone();
                    match battle::remap(e.motion, order, |s| rig.mset.has(s)) {
                        Some(slot) => {
                            a.restart(Some(slot), false, BLEND);
                            m.attack = Some(Attacking { record: Some(rec), slot, until_landed: false });
                            m.free = false;
                            m.script_slot = None;
                            m.speed = 0.0;
                            delta = Vec3::ZERO;
                            if e.air != 0.0 {
                                let to = m.target.and_then(|t| shape_of(t, &enemies)).map_or(Rel::default(), |(ct, _)| relation(m.motor.pos, ct));
                                let h = (e.air - to.dy).clamp(0.0, m.jump);
                                m.motor.start_jump(h);
                                m.held = Some((to.gdir, m.run));
                            } else {
                                m.held = None;
                            }
                            if log {
                                info!("party: {} attack motion {:#x} -> slot {slot}{}", m.model, e.motion, if e.air != 0.0 { " (jump)" } else { "" });
                            }
                        }
                        None => {
                            if log {
                                info!("party: {} attack motion {:#x} has no slot in the battle row; not started", m.model, e.motion);
                            }
                        }
                    }
                }
            }
            Some(Start::FollowUp(f)) => {
                // FUN_0010c800: the direction is the yaw (+0x55c) plus the entry's angle; FUN_001b0758
                // jumps `height` high reaching the top after `frames` frames, moving `speed` per frame
                // along it; motion 3 (FUN_0017e220 blend 0). [inferred] The app's motor rises with its own
                // gravity to that height (FUN_001b0758's `2h / t^2` is not modelled).
                if let Ok((mut a, _)) = anims.get_mut(m.body) {
                    let rig = a.rig.clone();
                    let slot = battle::remap(kh2_anim::id::JUMP as i32, order, |s| rig.mset.has(s));
                    let dir = battle::followup_dir(m.motor.yaw, &f, data.deg_to_rad);
                    if let Some(slot) = slot {
                        a.restart(Some(slot), false, 0.0);
                        m.attack = Some(Attacking { record: None, slot, until_landed: true });
                        m.free = false;
                        m.script_slot = None;
                        m.speed = 0.0;
                        delta = Vec3::ZERO;
                        m.motor.start_leap(f.height);
                        m.held = Some((dir, f.speed));
                        if log {
                            info!("party: {} follow-up jump: direction {dir:.2?} height {} speed {} -> slot {slot}", m.model, f.height, f.speed);
                        }
                    }
                }
            }
            Some(Start::Item(rec, item, who)) => {
                if let Ok((mut a, _)) = anims.get_mut(m.body) {
                    let rig = a.rig.clone();
                    if let Some(slot) = battle::remap(crate::items::USE_MOTION as i32, order, |s| rig.mset.has(s)) {
                        a.restart(Some(slot), false, crate::items::USE_BLEND);
                        m.attack = Some(Attacking { record: Some(rec), slot, until_landed: false });
                        m.using = Some(ItemUse { item, target: who, slot });
                        m.free = false;
                        m.script_slot = None;
                        m.speed = 0.0;
                        delta = Vec3::ZERO;
                        info!("party: {} uses item {item} on {who:?} (record {rec}, FUN_0010c9d0): motion 9 -> slot {slot}; HP {} MP {}", m.model, m.hp, m.mp);
                    }
                }
            }
            Some(Start::Special(rec, mode, mp, who)) => {
                m.special_who = who;
                info!("party: {} special record {rec}: script mode {mode:?}, pending MP {mp} (+0xaf4, FUN_00113d48)", m.model);
                m.pending_mp = mp;
                m.mode_request = Some(mode);
                m.special_target = true;
            }
            None => {}
        }
        // A running attack, follow-up or item motion: root motion as Sora's (translation turned by the
        // facing the motion started from), the air entry's or the jump's run, and free again once the
        // motion ended on the ground (a follow-up: once it landed).
        if let Some(at) = m.attack
            && !m.free
            && !started
            && let Ok((a, _)) = anims.get(m.body)
        {
            let (root_t, root_yaw) = a.root_delta;
            m.motor.yaw = crate::motor::wrap_angle(m.motor.yaw + root_yaw);
            let start_yaw = m.motor.yaw - a.root.yaw;
            delta = (Quat::from_rotation_y(start_yaw) * root_t).with_y(0.0);
            if !m.motor.grounded
                && let Some((d, sp)) = m.held
            {
                delta += d * sp;
            }
            let over = if at.until_landed { m.motor.grounded } else { a.finished || a.slot != Some(at.slot) };
            if over && m.motor.grounded {
                if mgr.log {
                    info!("party: {} action of record {:?} ended", m.model, at.record);
                }
                m.attack = None;
                m.held = None;
                m.free = true;
            }
        }
        // Movement: the velocity's direction on the ground at the snapped speed (per 60 Hz frame).
        let airborne = (!m.motor.grounded).then(|| m.motor.jump_dy(game.sstm.fall_max));
        let before = m.motor.pos;
        m.motor.move_by(&w, delta, airborne);
        let moved = (m.motor.pos - before).with_y(0.0).length();
        m.blocked = m.speed > 0.0 && moved < 0.5 * m.speed;
        // Facing: a turning member turns toward the player, which ends the turn (dot above
        // `turn_off`); a wanted yaw (+0x66c) is turned to; otherwise toward its movement. All
        // [inferred]: the writer of the facing (+0x550) in the friend's motion states was not read.
        if m.free {
            if m.follow.turning {
                m.motor.turn_to(ground_dir, m.turn);
            } else if let Some(y) = m.want_yaw {
                m.motor.turn_to(Vec3::new(y.sin(), 0.0, y.cos()), m.turn);
            } else if delta != Vec3::ZERO {
                m.motor.turn_to(delta, m.turn);
            }
        }
        if m.free
            && let Ok((mut a, _)) = anims.get_mut(m.body)
        {
            let rig = a.rig.clone();
            let has = |s: usize| rig.mset.has(s);
            // A motion loops when its data has a loop range (`frame_return < frame_end`), as `enemy_ai`
            // plays the scripts' requests.
            let looping = |s: usize| rig.mset.get(s).and_then(|x| x.motion.as_ref()).is_some_and(|mo| mo.frame_return < mo.frame_end);
            if m.speed <= 0.0
                && let Some(slot) = m.script_slot.filter(|&s| has(s))
            {
                // The script's request (its `idle` mode asks for motion 0, a special's cast motion 192)
                // while the member stands: a new request starts the motion again (FUN_0017e220), and
                // once it has ended the motion the script queued (host 1.13, FUN_0017e170) follows.
                if std::mem::take(&mut m.script_restart) {
                    a.restart(Some(slot), looping(slot), BLEND);
                } else if a.finished && a.slot == Some(slot) && let Some(q) = m.script_queue.take().filter(|&q| has(q)) {
                    m.script_slot = Some(q);
                    a.restart(Some(q), looping(q), BLEND);
                } else {
                    a.play(Some(slot), looping(slot), BLEND);
                }
            } else {
                let motion = if m.speed <= 0.0 { id::IDLE } else if m.speed < m.run { id::WALK } else { id::RUN };
                let slot = battle::remap(motion as i32, order, has);
                a.play(slot, true, BLEND);
            }
        }
        // FUN_00110eb0: a running warp waits for its fade; else the warp check.
        if m.warping {
            if m.fade.is_none() {
                m.warping = false;
                m.forced = false;
            }
        } else if m.free
            && (mgr.warp_mode == 2 || (mgr.warp_mode == 1 && !in_battle))
            && mgr.trail.head >= 0
            && rules::warp_due(m.cooldown, m.far)
            && ((m.forced && warp_on_screen) || !on_screen(cam, m.motor.pos, m.radius, m.height))
        {
            let r = mgr.random(rules::WARP_RANDOM);
            let me = m.motor.pos;
            let ignore_screen = m.forced && warp_on_screen;
            let ok = |p: Vec3| {
                (ignore_screen || !on_screen(cam, p, m.radius, m.height))
                    && rules::warp_point_ok(p, player, me, ground_dir, c)
                    // [inferred] stand-in for the collision test of FUN_001108c8 (FUN_00143840 at the
                    // point, bit 34 clear): a floor under the point.
                    && w.ground(p, p.y + m.motor.capsule.step, 2.0 * m.motor.capsule.step).is_some()
            };
            let pick = if m.forced {
                // Flag 0x8000: every trail point; the accepted one nearest the player.
                (0..rules::TRAIL_LEN).filter(|&k| ok(mgr.trail.points[k])).min_by(|&a, &b| {
                    let d = |k: usize| (mgr.trail.points[k] - player).length_squared();
                    d(a).total_cmp(&d(b))
                })
            } else {
                rules::warp_order(m.warps, r).find(|&k| ok(mgr.trail.points[k]))
            };
            if let Some(k) = pick {
                let p = mgr.trail.points[k];
                // FUN_00110e28: put there, fade in, far counter 0, one more warp, the cooldown.
                m.motor.pos = p;
                m.motor.grounded = true;
                m.motor.jump = Default::default();
                m.warping = true;
                m.fade = Some(0.0);
                m.alpha = 0.0;
                m.far = 0;
                m.warps += 1;
                m.cooldown = c.warp_cooldown;
                info!(
                    "party: {} warp to trail point {k} at {p:?} (from {me:?}, {:.0} from the player{}{})",
                    m.model,
                    (p - player).length(),
                    if m.forced { ", forced" } else { "" },
                    if ignore_screen { ", on screen by KH2_PARTY_WARP_ONSCREEN" } else { "" }
                );
            }
        }
        if let Some(f) = m.fade.as_mut() {
            *f += 1.0;
            m.alpha = (*f / c.warp_fade).min(1.0);
            if *f >= c.warp_fade {
                m.fade = None;
            }
        }
        if status {
            info!(
                "party: status {} (NeoStatus {}, class {}) slot {} at {:?} distance {:.0} mode {} speed {:.1} far {} cooldown {:.0}{}",
                m.model,
                m.neo,
                m.class,
                m.slot,
                m.motor.pos,
                to_player.length(),
                m.follow.mode,
                m.speed,
                m.far,
                m.cooldown,
                if in_battle { format!(" target {:?} attacking {}", m.target.map(|t| t.entity), m.attack.is_some()) } else { String::new() }
            );
            if in_battle {
                let chosen = std::mem::take(&mut m.chosen);
                info!("party: {} actions in the last second: {} [{}]", m.model, chosen.len(), chosen.join(", "));
            }
        }
        tf.translation = m.motor.pos * SCALE;
        tf.rotation = Quat::from_rotation_y(m.motor.yaw);
    }
}

/// Fighters other than the members and Sora.
type NotMemberOrSora = (Without<PartyMember>, Without<Sora>);

/// Keeps the members from overlapping each other, Sora and the enemies: a member inside another
/// body's radius moves out along the line between them on the ground (two members share the move).
/// [inferred] The game's object-to-object push (the movement's collision against other objects'
/// cylinders) was not read; the radii are the bodies' shapes.
fn party_push_apart(
    sora: Query<(&Sora, &Transform), Without<PartyMember>>,
    fighters: Query<(&Fighter, &Transform), NotMemberOrSora>,
    rigs: Query<&Animator>,
    mut q: Query<(&mut Transform, &mut PartyMember)>,
) {
    let mut others: Vec<(Vec3, f32)> = Vec::new();
    if let Ok((s, _)) = sora.single() {
        others.push((s.motor.pos, s.motor.capsule.radius.max(1.0)));
    }
    for (f, t) in &fighters {
        if f.team == Team::Enemy
            && f.hittable
            && let Ok(a) = rigs.get(f.hurt_body)
        {
            others.push((t.translation / SCALE, body_shape(&a.rig).0));
        }
    }
    let members: Vec<(Vec3, f32)> = q.iter().map(|(_, m)| (m.motor.pos, m.radius)).collect();
    for (i, (mut tf, mut m)) in q.iter_mut().enumerate() {
        let mut push = Vec3::ZERO;
        let mut add = |p: Vec3, r: f32, share: f32| {
            let d = (m.motor.pos - p).with_y(0.0);
            let l = d.length();
            let need = m.radius + r;
            if l < need {
                push += if l > 1e-3 { d / l } else { Vec3::X } * (need - l) * share;
            }
        };
        for &(p, r) in &others {
            add(p, r, 1.0);
        }
        for (j, &(p, r)) in members.iter().enumerate() {
            if i != j {
                add(p, r, 0.5);
            }
        }
        if push != Vec3::ZERO {
            m.motor.pos += push;
            tf.translation = m.motor.pos * SCALE;
        }
    }
}

/// `KH2_PARTYLOG`: the members' landed hits (the enemy's own line prints the damage).
fn party_hits(mut hits: MessageReader<Hit>, mgr: Res<Manager>, q: Query<&PartyMember>) {
    for h in hits.read() {
        if let Ok(m) = q.get(h.attacker)
            && mgr.log
        {
            info!("party: {} hit {:?} with atkp {} at {:.2?}", m.model, h.target, h.atkp, h.point);
        }
    }
}

/// FUN_00175d18 (the party class's voice method +0x54) for a member: the voice table's line of `voice` for
/// the member's NeoStatus, from the member's bank of the current world, at the member. The other party
/// (the NeoStatus of the member's target +0x9f4) is passed as 0 [inferred: the app's member target is
/// not mapped to an object id here].
fn member_voice(v: &VoiceRes, rng: &mut u32, neo: u16, voice: u16, at: Vec3, owner: Entity, body: Entity) -> Option<crate::audio::PlayVoice> {
    let (table, info) = (v.0.as_ref()?, v.1.as_ref()?);
    let area_name = v.2.loaded.as_deref()?;
    let world = area_name.get(..2)?;
    let (line, priority) = table.pick(neo, voice, 0, rng, &|_| false)?;
    let bank = crate::audio::area_voice_bank(&v.3, area_name).unwrap_or(0);
    let name = info.names.name(neo)?;
    Some(crate::audio::PlayVoice {
        file: kh2_audio::voice::bank_path(&info.lang, world, bank, name),
        in_bar: false,
        line,
        priority,
        pos: Some(at),
        speaker: owner.to_bits(),
        follow: Some(body),
    })
}

/// What a member's voice reads: the voice table, the speaker names and language, the area, the disc.
type VoiceRes<'w> = (Option<Res<'w, crate::audio::VoiceTable>>, Option<Res<'w, crate::audio::VoiceInfo>>, Res<'w, CurrentArea>, Res<'w, Disc>);

/// Enemy hits on the members (stage 3): the friend's receive method FUN_001106b8 -> FUN_0017cde0 ->
/// FUN_00175340. A KO'd member ignores the hit (FUN_00175340 returns for attack kinds other than 5 and
/// 6, which no enemy of the app uses [inferred]). The damage is the `atkp` row's on the member's stats
/// from its saved record (`enemy::member_stats`) by the attacker's stats, at least 1 unless ineffective,
/// as Sora's (`sora::receive_hits`), through the HP method (FUN_00185a68 -> FUN_00176c98 ->
/// FUN_00186238): HP 0 knocks out (FUN_0017ce60: voice 2, the revive timer). Voice 9 when the hit takes
/// the HP into danger (FUN_0017cde0, FUN_001a0be0). The reaction is the party class's (+0x68, the rule
/// `reaction_rules::victim_outcome` that Sora's hits use) as a motion of the member's set through the
/// player-type remap; pushes and the blow's launch are not carried out [inferred: the members' movers
/// are not modelled].
#[allow(clippy::too_many_arguments)]
fn party_receive_hits(
    mut hits: MessageReader<Hit>,
    game: Res<GameData>,
    data: Res<PartyData>,
    reaction: Option<Res<crate::combat::ReactionData>>,
    progress: Option<Res<crate::progress::Progress>>,
    enemies: Query<&crate::enemy::Enemy>,
    mut q: Query<(Entity, &mut PartyMember, &mut Fighter, &GlobalTransform)>,
    mut anims: Query<&mut Animator>,
    (voice, mut rng, mut voices): (VoiceRes, Option<ResMut<crate::audio::GameRng>>, MessageWriter<crate::audio::PlayVoice>),
) {
    use crate::reaction_rules::{self as rr, VictimAction, VictimCtx};
    let Some(reaction) = reaction else { return };
    for h in hits.read() {
        let Ok((me, mut m, mut fighter, place)) = q.get_mut(h.target) else { continue };
        if m.ko {
            continue;
        }
        let (Some(atk), Ok(from), Some(to)) = (game.hit_row(h), enemies.get(h.attacker).map(|e| e.stats), crate::enemy::member_stats(progress.as_deref(), m.neo)) else { continue };
        let useless = crate::combat::hit_is_ineffective(atk, &to);
        let dmg = if useless { game.damage(atk, &from, &to) } else { game.damage(atk, &from, &to).max(1) };
        let (before, danger_before) = (m.hp, battle::in_danger(m.hp, m.hp_max));
        let c = m.change_hp(-dmg, &data);
        let mut asked = Vec::new();
        if c.knocked_out {
            asked.push(VOICE_KO);
        }
        if !danger_before && battle::in_danger(m.hp, m.hp_max) {
            asked.push(VOICE_DANGER);
        }
        let facing = Vec3::new(m.motor.yaw.sin(), 0.0, m.motor.yaw.cos());
        let from_front = facing.dot(-h.dir) > 0.0;
        let ctx = VictimCtx { status: 0, grounded: m.motor.grounded, attacker_flag_4000: false, from_front, hit_part_plain: true };
        let out = rr::victim_outcome(&rr::HitRow::from_atkp(atk), useless, &ctx, &reaction.patn, &reaction.defaults, &reaction.consts);
        let motion = match out.action {
            VictimAction::Flinch { motion, fallback } | VictimAction::FlinchPush { motion, fallback, .. } => Some((motion as i32, Some(fallback as i32))),
            VictimAction::Class14 { motion, .. } | VictimAction::Blow { motion, .. } => Some((motion as i32, None)),
            _ => None,
        };
        let mut played = None;
        if let Some((motion, fallback)) = motion
            && let Ok(mut a) = anims.get_mut(m.body)
        {
            let rig = a.rig.clone();
            let order = data.order(m.motion_row);
            let slot = battle::remap(motion, order, |s| rig.mset.has(s)).or_else(|| fallback.and_then(|f| battle::remap(f, order, |s| rig.mset.has(s))));
            if slot.is_some() {
                a.restart(slot, false, 2.0);
                m.hurt = slot;
                m.attack = None;
                m.using = None;
                m.held = None;
                fighter.landed.clear();
                played = slot;
            }
        }
        info!(
            "party: {} hit: atkp {} damage {dmg} HP {before} -> {} of {}{} reaction {:?} -> slot {played:?}{}",
            m.model,
            h.atkp,
            m.hp,
            m.hp_max,
            if useless { " (ineffective)" } else { "" },
            out.action,
            if c.knocked_out { format!("; KO (FUN_0017ce60): revive timer {:.0} (prty StunTime of row NeoMoveset)", m.revive) } else { String::new() }
        );
        if let Some(rng) = rng.as_mut() {
            for id in asked {
                let line = member_voice(&voice, &mut rng.0, m.neo, id, place.translation(), me, m.body);
                info!("party: {} voice {id} ({}) -> {:?}", m.model, if id == VOICE_KO { "KO" } else { "HP in danger" }, line.as_ref().map(|l| (&l.file, l.line)));
                if let Some(l) = line {
                    voices.write(l);
                }
            }
        }
    }
    // A KO'd member is not hit [inferred, see above]; the others are.
    for (_, m, mut f, _) in &mut q {
        let want = !m.ko;
        if f.hittable != want {
            f.hittable = want;
        }
    }
}

/// The members' item uses (stage 4): at frame trigger 6 of motion 9 (FUN_00188498 case 6 -> FUN_001b8710)
/// the item leaves the member's own slots (FUN_00175ec0, `Progress::take_item` with the member's NeoStatus)
/// and waits 30 frames (FUN_001b82b8); then its effect (FUN_001a12a0, `items::item_effect`, multiplier
/// 100: the members' Item Boost is not built [inferred 0]) lands on the target, every party object for a
/// group item (FUN_001a1fe0). A member's HP part goes through its HP method (a KO'd member is revived,
/// FUN_0017cee8) [inferred: FUN_001a12a0 on a block with an object calls the object's method, as the
/// `items` oracle test's note says]. The use effects (system effect 0 and the item's own) are not started.
#[allow(clippy::too_many_arguments)]
fn party_item_uses(
    data: Res<PartyData>,
    game: Res<GameData>,
    mut progress: Option<ResMut<crate::progress::Progress>>,
    mut q: Query<(Entity, &mut PartyMember)>,
    anims: Query<(&Animator, &GlobalTransform)>,
    mut sora: Query<&mut Sora>,
    mut magic: Option<ResMut<crate::magic::MagicState>>,
) {
    let feed = data.sstm_f32(0x7c);
    let mut landed: Vec<(Entity, u16, battle::Who)> = Vec::new();
    for (e, mut m) in &mut q {
        if let Some(u) = m.using {
            let Ok((a, _)) = anims.get(m.body) else { continue };
            let in_motion = a.slot == Some(u.slot);
            let fired = a
                .motion()
                .zip(a.triggers())
                .map(|(mo, tr)| {
                    // [inferred] The trigger frames count 60 Hz ticks from the motion's first frame: the
                    // members' motion 9 starts at frame 15 / 12 (30 fps) with trigger 6 at tick 14 / 19, and
                    // every trigger of the motions printed by `party_ko_data_from_the_disc` lies inside the
                    // motion's length counted that way (the app's other trigger readers count from frame 0,
                    // which never reaches these).
                    let g = |f: f32| (f - mo.frame_start) * crate::data::LOGIC_HZ / mo.fps.max(1.0);
                    let (from, to) = (g(a.prev_frame), g(a.frame));
                    from != to && tr.crossed(from, to).any(|f| f.id == crate::items::TRIGGER_USE)
                })
                .unwrap_or(false);
            if !in_motion {
                info!("party: {} item motion ended before trigger {}: nothing used", m.model, crate::items::TRIGGER_USE);
                m.using = None;
            } else if fired {
                m.using = None;
                let taken = progress.as_mut().is_some_and(|p| p.take_item(u.item, m.neo));
                info!("party: {} trigger {}: item {} {} the member's slots", m.model, crate::items::TRIGGER_USE, u.item, if taken { "taken from" } else { "not in" });
                if taken {
                    m.queued.push(QueuedUse { left: USE_WAIT, item: u.item, target: u.target });
                }
            }
        }
        let mut keep = Vec::new();
        for mut qu in std::mem::take(&mut m.queued) {
            qu.left -= FRAME;
            if qu.left > 0.0 {
                keep.push(qu);
            } else {
                landed.push((e, qu.item, qu.target));
            }
        }
        m.queued = keep;
    }
    for (user, item, who) in landed {
        let Some(row) = data.weapons.items.iter().find(|i| i.id == item) else { continue };
        let amount = row.flag1 as u16 | (row.rank as u16) << 8;
        let user_slot = q.get(user).map_or(0, |(_, m)| m.slot);
        let group = crate::items::item_id::group(item);
        // The targets: Sora, and the members by entity.
        let to_sora = group || who == battle::Who::Player;
        let members: Vec<Entity> = q.iter().filter(|(e, m)| group || (who == battle::Who::Me && *e == user) || (who == battle::Who::Other && *e != user && m.slot != user_slot)).map(|(e, _)| e).collect();
        if to_sora && let Ok(mut s) = sora.single_mut() {
            let charge = magic.as_ref().map_or(Default::default(), |m| m.charge);
            let mut v = crate::items::Vitals { hp: s.hp, hp_max: s.stats.max_hp, hp_min: 0, mp: s.mp, mp_max: game.sora_max_mp, charge_left: charge.left, charge_total: charge.total, ..Default::default() };
            let before = (s.hp, s.mp);
            crate::items::item_effect(item, amount, 100, &mut v, feed);
            s.hp = v.hp;
            s.mp = v.mp;
            if let Some(mg) = magic.as_mut() {
                mg.charge.left = v.charge_left;
                mg.charge.total = v.charge_total;
            }
            info!("party: item {item} (amount {amount}) on Sora: HP {} -> {}, MP {} -> {}", before.0, s.hp, before.1, s.mp);
        }
        for t in members {
            let Ok((_, mut m)) = q.get_mut(t) else { continue };
            let mut v = crate::items::Vitals { hp: m.hp, hp_max: m.hp_max, hp_min: 0, mp: m.mp, mp_max: m.mp_max, charge_left: m.charge.left, charge_total: m.charge.total, ..Default::default() };
            let before = (m.hp, m.mp);
            crate::items::item_effect(item, amount, 100, &mut v, feed);
            let gain = v.hp - m.hp;
            let c = m.change_hp(gain, &data);
            m.mp = v.mp;
            m.charge.left = v.charge_left;
            m.charge.total = v.charge_total;
            info!("party: item {item} (amount {amount}) on {}: HP {} -> {}{}, MP {} -> {}", m.model, before.0, m.hp, if c.revived { " (revived)" } else { "" }, before.1, m.mp);
        }
    }
}

/// The hits the members' scripts delivered through their attack objects (hosts 2.10 / 2.17: the target's
/// receive method with the hit record, FUN_0018c6d0). On an enemy target: a `combat::Hit` with the `atkp`
/// row (the enemy's receive path does the rest). On a party object: FUN_00175340 -> FUN_00185a68 takes
/// attack types 5 and 6 as a heal (the amount added, not subtracted), even on a KO'd member, which the HP
/// method then revives (FUN_00186238 -> FUN_0017cee8); Cure's attack 1525 is type 6, so the amount is
/// `(magic x Power + 99) / 100` of the caster (`GameData::damage`). Other types on a party object are not
/// carried out [inferred: no member script aims one there].
#[allow(clippy::too_many_arguments)]
fn party_script_hits(
    game: Res<GameData>,
    data: Res<PartyData>,
    progress: Option<Res<crate::progress::Progress>>,
    mut q: Query<(Entity, &mut PartyMember)>,
    mut sora: Query<&mut Sora>,
    places: Query<&GlobalTransform>,
    mut hits: MessageWriter<Hit>,
) {
    let mut heals: Vec<(Entity, battle::Who, i32, i16)> = Vec::new();
    for (me, mut m) in &mut q {
        if m.script_hits.is_empty() {
            continue;
        }
        let ids = std::mem::take(&mut m.script_hits);
        let Some(mut caster) = crate::enemy::member_stats(progress.as_deref(), m.neo) else { continue };
        // [inferred] The status rebuild's equipment pass (FUN_0019f920) adds the hand-0 weapon's attack and
        // magic, as `GameData::sora_stats` adds the keyblade's; `enemy::member_stats` leaves it out.
        let weapon = progress.as_ref().and_then(|p| p.member(m.neo)).map_or(0, |r| r.weapons[0]);
        if let Some(st) = data.weapons.items.iter().find(|i| i.id == weapon).and_then(|i| data.weapons.stats.iter().find(|s| s.id == i.stat_entry)) {
            caster.strength += st.attack as i32;
            caster.magic += st.magic as i32;
        }
        info!("party: {} script hits {ids:?}: caster strength {} magic {} (weapon item {weapon})", m.model, caster.strength, caster.magic);
        for id in ids {
            let Some(atk) = game.atkp(id) else { continue };
            match m.special_who {
                Some(ScriptTarget::Enemy(t)) => {
                    let at = places.get(t.entity).map_or(Vec3::ZERO, |g| g.translation());
                    let from = m.position() * SCALE;
                    hits.write(Hit { attacker: me, target: t.entity, atkp: id, dir: (at - from).with_y(0.0).normalize_or(Vec3::Z), point: at, row: None });
                    info!("party: {} script attack atkp {id} hits its target {:?}", m.model, t.entity);
                }
                Some(ScriptTarget::Party(who)) if matches!(atk.r#type, 5 | 6) => {
                    let to = if who == battle::Who::Player { sora.single().map(|s| s.stats).ok() } else { None }.unwrap_or(caster);
                    heals.push((me, who, game.damage(atk, &caster, &to), id));
                }
                other => info!("party: {} script attack atkp {id} (type {}) on {other:?}: not carried out", m.model, atk.r#type),
            }
        }
    }
    for (caster, who, amount, id) in heals {
        let (caster_slot, caster_model) = q.get(caster).map_or((0, String::new()), |(_, m)| (m.slot, m.model.clone()));
        match who {
            battle::Who::Player => {
                if let Ok(mut s) = sora.single_mut() {
                    // Sora's HP method (FUN_00186238 on the player, not KO'd): added, held at the maximum.
                    let before = s.hp;
                    s.hp = (s.hp + amount).clamp(0, s.stats.max_hp);
                    info!("party: {caster_model} heals Sora with atkp {id} (type 6, FUN_00185a68): +{amount}, HP {before} -> {}", s.hp);
                }
            }
            battle::Who::Me | battle::Who::Other => {
                for (e, mut m) in &mut q {
                    let hit = if who == battle::Who::Me { e == caster } else { e != caster && m.slot != caster_slot };
                    if !hit {
                        continue;
                    }
                    let (before, was_ko) = (m.hp, m.ko);
                    let c = m.change_hp(amount, &data);
                    info!("party: {caster_model} heals {} with atkp {id} (type 6, FUN_00185a68): +{amount}, HP {before} -> {}{}", m.model, m.hp, if was_ko && c.revived { " (was KO'd: revived)" } else { "" });
                }
            }
        }
    }
}

/// FUN_001b82b8's wait before a used item's effect (`DAT_0037bba4`, 30 frames; `items` reads the same).
const USE_WAIT: f32 = 30.0;

/// Harness: `KH2_PARTY_HIT=<s>:<atkp>[:<model>][,...]` makes the nearest living enemy land a hit with
/// that `atkp` row on every member at those seconds (the enemies of the app aim at Sora, so a member is
/// hit only by chance). A test aid, not the game's rule: the hit then takes the members' real receive path.
fn party_forced_hits(
    mut done: Local<Vec<f32>>,
    mut tick: Local<u32>,
    enemies: Query<(Entity, &crate::enemy::Enemy, &GlobalTransform)>,
    q: Query<(Entity, &PartyMember)>,
    mut hits: MessageWriter<Hit>,
) {
    let Ok(spec) = std::env::var("KH2_PARTY_HIT") else { return };
    *tick += 1;
    let now = *tick as f32 / 60.0;
    for item in spec.split(',') {
        let mut parts = item.split(':');
        let (Some(t), Some(atkp)) = (parts.next(), parts.next()) else { continue };
        let only = parts.next().map(str::trim);
        let (Ok(t), Ok(atkp)) = (t.trim().parse::<f32>(), atkp.trim().parse::<i16>()) else { continue };
        if now < t || done.contains(&t) {
            continue;
        }
        done.push(t);
        for (me, m) in q.iter().filter(|(_, m)| only.is_none_or(|o| o == m.model)) {
            let at = m.position() * SCALE;
            let Some((e, en, g)) = enemies.iter().filter(|(_, en, _)| en.alive()).min_by(|a, b| a.2.translation().distance_squared(at).total_cmp(&b.2.translation().distance_squared(at))) else { continue };
            let dir = (at - g.translation()).with_y(0.0).normalize_or(Vec3::Z);
            info!("party: KH2_PARTY_HIT at {t} s: {} lands atkp {atkp} on {}", en.name, m.model);
            hits.write(Hit { attacker: e, target: me, atkp, dir, point: at + Vec3::Y * (m.height * 0.5 * SCALE), row: None });
        }
    }
}

/// The party panels' inputs (docs/party.md section 7): per party slot the member's status block as the
/// panel reads it (HP +0 over +4, MP +0x180 over +0x184; FUN_0013dcd8 registers slot + 1 at creation,
/// summons get none). The harness fill `KH2_HUDPARTY` keeps precedence.
fn party_hud(q: Query<&PartyMember>, panels: Option<ResMut<crate::hud::hud_party::PartyMembers>>, mut filled: Local<bool>) {
    let Some(mut panels) = panels else { return };
    if std::env::var_os("KH2_HUDPARTY").is_some() {
        return;
    }
    let mut members: Vec<&PartyMember> = q.iter().collect();
    if members.is_empty() {
        if *filled {
            panels.0.clear();
            *filled = false;
        }
        return;
    }
    members.sort_by_key(|m| m.slot);
    let face = |s: usize| panels.0.get(s).and_then(|p| p.face);
    let want: Vec<crate::hud::hud_party::PartyMember> =
        members.iter().enumerate().map(|(s, m)| crate::hud::hud_party::PartyMember { hp: m.hp, max_hp: m.hp_max, mp: m.mp, max_mp: m.mp_max, face: face(s) }).collect();
    if panels.0 != want {
        panels.0 = want;
    }
    *filled = true;
}

/// The warp's fade-in on the member's materials (blended while it fades, as authored otherwise).
fn party_fade(
    mut q: Query<&mut PartyMember>,
    children: Query<&Children>,
    handles: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for mut m in &mut q {
        let alpha = m.alpha.clamp(0.0, 1.0);
        if (alpha - m.applied_alpha).abs() < 1e-4 {
            continue;
        }
        m.applied_alpha = alpha;
        let Ok(kids) = children.get(m.body) else { continue };
        for k in kids.iter() {
            let Ok(h) = handles.get(k) else { continue };
            let Some(mut mat) = materials.get_mut(&h.0) else { continue };
            let original = *m.original_modes.entry(h.0.id()).or_insert_with(|| mat.alpha_mode);
            mat.base_color = mat.base_color.with_alpha(alpha);
            mat.alpha_mode = if alpha < 1.0 { AlphaMode::Blend } else { original };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kh2_anim::PlayerState;

    /// tt02's map script places FRIEND_1 and FRIEND_2; with preset 1 in Twilight Town (world 2) they
    /// are Donald (`P_EX020`) and Goofy (`P_EX030`), friend-AI classes 0 and 1, with their models and
    /// motion sets on the disc. Needs `KH2_ISO`.
    #[test]
    fn party_tt02_friends_are_donald_and_goofy() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let mut data = PartyData::load(&disc);
        let c = data.consts.clone().expect("constants");
        let memt = data.memt.clone().expect("memt");
        let preset = memt.trailing[rules::DEFAULT_PRESET].to_le_bytes();
        let bytes = rules::apply_preset([0, rules::EMPTY, rules::EMPTY, rules::EMPTY], preset);
        let merged = crate::drive::rules::memt_merge(&memt, 2, |_| false);
        let recs = friend_records(&disc, "tt02", None);
        for r in &recs {
            eprintln!("tt02 record {:#x} group {} at {:?} yaw {:.2} entrance {}", r.0, r.1, r.2, r.3, r.4);
        }
        assert_eq!(recs.iter().filter(|r| r.0 != PLAYER_ENTITY).count(), 2);
        let mut models = Vec::new();
        for (k, byte) in [bytes[1], bytes[2]].into_iter().enumerate() {
            let obj = rules::slot_object(&merged, byte).expect("member");
            let row = data.objs.iter().find(|r| r.object_id == obj as u32).cloned().expect("objentry row");
            let class = rules::ai_class(row.neo_status, row.neo_moveset).expect("class");
            let rig = data.rig(&disc, &row).expect("rig");
            let t = c.thresholds(class);
            eprintln!(
                "FRIEND_{}: {} anim {} NeoStatus {} moveset {} class {class} slots {} idle/walk/run {:?} thresholds {:?} follow distance {}",
                k + 1,
                row.model_name,
                row.animation_name,
                row.neo_status,
                row.neo_moveset,
                rig.mset.slots.len(),
                [id::IDLE, id::WALK, id::RUN].map(|i| rig.mset.slot(i, PlayerState::FieldNoWeapon)),
                t[rules::ROW_FIELD],
                rules::follow_distance(&t, body_shape(&rig).0, &c)
            );
            eprintln!("  objentry WeaponJoint {} bones {}", row.weapon_joint, rig.model.bones.len());
            assert!(rig.mset.slot(id::IDLE, PlayerState::FieldNoWeapon).is_some());
            assert!(rig.mset.slot(id::RUN, PlayerState::FieldNoWeapon).is_some());
            models.push(row.model_name.clone());
        }
        assert_eq!(models, ["P_EX020", "P_EX030"]);
    }

    /// The members' hand objects: the new game's saved weapon word of Donald and Goofy, the `went`
    /// object of their movesets for it and the `wmst` motion set (printed); both resolve to a loadable
    /// weapon. Needs `KH2_ISO`.
    #[test]
    fn party_member_weapons_from_went() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let data = PartyData::load(&disc);
        let progress = crate::progress::Progress::start(&disc);
        for (neo, model) in [(2u16, "P_EX020"), (3, "P_EX030")] {
            let row = data.objs.iter().find(|r| r.model_name == model).cloned().expect("row");
            let saved = progress.as_ref().and_then(|p| p.member(neo)).map_or(0, |m| m.weapons[0]);
            // The app's new game leaves the members' weapon words 0 (no hand object, FUN_0016ee98);
            // the base weapon items (Mage's Staff 75, Knight's Shield 49 [inferred from the item ids])
            // are what `KH2_PARTY_WEAPONS` hands in.
            let item = if saved != 0 { saved } else if neo == 2 { 75 } else { 49 };
            let files = data.weapon_files(&row, item, 0);
            let rig = files.as_ref().and_then(|f| crate::drive::weapon_rig(&disc, f));
            eprintln!(
                "{model}: NeoStatus {neo} moveset {} WeaponJoint {} saved weapon word {saved}, item {item} -> {files:?} (bones {:?}, motion slots {:?})",
                row.neo_moveset,
                row.weapon_joint,
                rig.as_ref().map(|r| r.model.bones.len()),
                rig.as_ref().map(|r| r.mset.slots.len())
            );
            let w = |i: usize| data.weapons.went.get(i * 4..i * 4 + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0);
            let off = w(row.neo_moveset as usize) as usize;
            for k in 0..64 {
                let id = if off == 0 { 0 } else { w(off + k) };
                if id != 0 {
                    let o = data.objs.iter().find(|o| o.object_id == id).map(|o| o.model_name.clone());
                    let items: Vec<u16> = data.weapons.items.iter().filter(|i| (i.flag1 as usize | (i.rank as usize) << 8) == k).map(|i| i.id).collect();
                    eprintln!("  went slot {k}: object {id} {o:?} items {items:?}");
                }
            }
            assert!(rig.is_some(), "{model}: weapon not loadable");
        }
    }

    /// Stage 3's data from the disc: the `prty` row of Donald's and Goofy's NeoMoveset (the record at
    /// +0x12c: StunTime = the revive time, MpDrive = the MP charge), the `sstm` words the KO path reads
    /// (+0x54, +0x58, +0x5c), the get-up blend `DAT_0037a55c`, and that the members' motion sets have the
    /// KO motions 0x36 / 0x37 and the item motion 9 in the battle row (printed). Needs `KH2_ISO`.
    #[test]
    fn party_ko_data_from_the_disc() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let mut data = PartyData::load(&disc);
        eprintln!("prty: {} rows, offsets {:?}", data.prty.len(), data.prty_offsets);
        eprintln!("sstm +0x54 {} +0x58 {} +0x5c {} +0x7c {}", data.sstm_f32(0x54), data.sstm_f32(0x58), data.sstm_f32(0x5c), data.sstm_f32(0x7c));
        let elf = disc.read_root("SLUS_210.05").expect("exe");
        let blend = rules::exe_bytes(&elf, 0x37a55c, 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        assert_eq!(blend, Some(GET_UP_BLEND), "DAT_0037a55c");
        // Sora's item motion for comparison (the frame unit of the triggers).
        if let Some(row) = data.objs.iter().find(|r| r.model_name == "P_EX100").cloned()
            && let Some(rig) = data.rig(&disc, &row)
        {
            let slot = rig.mset.slot(crate::items::USE_MOTION, kh2_anim::PlayerState::BattleWeapon);
            let s = slot.and_then(|s| rig.mset.get(s));
            eprintln!(
                "P_EX100 motion 9 slot {slot:?}: frames {:?} triggers {:?}",
                s.and_then(|s| s.motion.as_ref()).map(|mo| (mo.frame_start, mo.frame_end, mo.fps)),
                s.map(|s| s.triggers.frames.iter().map(|f| (f.frame, f.id)).collect::<Vec<_>>())
            );
        }
        for model in ["P_EX020", "P_EX030"] {
            let row = data.objs.iter().find(|r| r.model_name == model).cloned().expect("row");
            let k = battle::pref_row(&data.prty_offsets, row.neo_moveset as usize, 1);
            let p = data.prty_row(row.neo_moveset).cloned().expect("prty row");
            eprintln!(
                "{model}: NeoStatus {} NeoMoveset {} -> prty row {k:?}: walk {} run {} jump {} turn {} StunTime {} MpDrive {} (row NeoStatus: {:?})",
                row.neo_status,
                row.neo_moveset,
                p.walk_speed,
                p.run_speed,
                p.jump_height,
                p.turn_speed,
                p.stun_time,
                p.mp_drive,
                data.prty.get(row.neo_status as usize).map(|q| (q.walk_speed, q.run_speed, q.stun_time, q.mp_drive))
            );
            let rig = data.rig(&disc, &row).expect("rig");
            let order = data.order(1);
            // 192 (0xc0): the spells' cast motion of Donald's `fire` .. `cure` modes (R1559); slot 768 is its
            // number as the script asks for it.
            for motion in [MOTION_DOWN, MOTION_GET_UP, crate::items::USE_MOTION as i32, 3, 0xc0] {
                let slot = battle::remap(motion, order, |s| rig.mset.has(s)).or_else(|| (motion == 0xc0 && rig.mset.has(768)).then_some(768));
                let tr = slot.and_then(|s| rig.mset.get(s)).map(|s| s.triggers.frames.iter().map(|f| (f.frame, f.id)).collect::<Vec<_>>());
                let span = slot.and_then(|s| rig.mset.get(s)).and_then(|s| s.motion.as_ref()).map(|mo| (mo.frame_start, mo.frame_end, mo.fps));
                eprintln!("  motion {motion:#x} -> battle slot {slot:?}, frames (start, end, fps) {span:?}, frame triggers (frame, id) {tr:?}");
                assert!(slot.is_some() || motion == 0xc0, "{model}: motion {motion:#x}");
            }
        }
    }

    /// The executable's follow constants are the ones docs/party.md names (printed, and checked for
    /// the shape the rules rely on: two mirrored side offsets, a positive forward part).
    #[test]
    fn party_consts_from_the_executable() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let c = PartyData::load(&disc).consts.expect("constants");
        eprintln!("{c:?}");
        assert_eq!(c.classes.len(), rules::CLASSES);
        assert_eq!(c.side[0], -c.side[1]);
        assert!(c.ahead > 0.0 && c.trail_step2 > 0.0 && c.warp_cooldown > 0.0);
        // Every class's field row is a hysteresis: each mode slows down below where it speeds up.
        for t in &c.classes {
            assert!(t[rules::ROW_FIELD].iter().all(|(down, up)| down <= up), "{t:?}");
        }
    }
}
