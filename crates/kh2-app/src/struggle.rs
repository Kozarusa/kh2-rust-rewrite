//! The Struggle (WP-7E): the game's mission manager (`DAT_01d5dd20`) reduced to what the Struggle matches of
//! tt04 use: the two orb counters, the match clock, the mission script that ties them together, and the end.
//! See docs/systems/world-minigames/struggle.md for the rules and every address.
//!
//! What runs here, in the game's terms:
//! - the mission file `msn/us/<name>.bar` (loaded by `FUN_001b8d80`): its header records (`FUN_001d8d18` with
//!   `FUN_001b9378`) make the counters (opcode 5) and the clock record (opcode 4); its BDX entry (type 3) is the
//!   mission script, run as a system script (`FUN_001954f8`: event 2 now, event 3 as its thread) that gets every
//!   global event as event 10 with (event, argument) (`FUN_00197b70` -> `FUN_00195718` -> `FUN_00195690`);
//! - the orbs: every reaction a fighter takes knocks orbs out of its counter (`FUN_001caeb0`, the end of the
//!   reaction dispatcher `FUN_00184e80`, which the player's method +0x68 `0x179078` also reaches) into the orb
//!   pool of `PRIZE_TT` (`DAT_003600c0`), whose pickup (`FUN_001cadd8`) adds one to the picker's counter;
//! - the end: the script reacts to a full counter (global event 0x7b) or the clock's end (0x4a) and sends the
//!   result as global event 159 (the player won) or 160 (he lost) through host 1.41.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use kh2_bdx::{Call, Host, Program, Runtime, Value};

use crate::assets::Disc;
use crate::combat::{Hit, ReactionData, ReactionStatus};
use crate::data::GameData;
use crate::enemy::Enemy;
use crate::enemy_ai::AiHit;
use crate::prize::{self, Consts, Phase};
use crate::reaction_rules as rules;
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::{Sora, State};
use crate::world::World;
use crate::SCALE;

use crate::shared::struggle::exe_bytes;
pub use crate::shared::struggle::{exe_word, mission_command, parse_header, Active, Clock, Counter, EV_CLOCK_END, EV_COUNTER_EMPTY, EV_COUNTER_FULL, EV_PLAYER_KO, EV_PLAYER_LOST, EV_PLAYER_WON, Element, Finish, GameOverExits, GameOverRequest, GlobalEvent, Layout, Mission, MissionStart, ORB_POOL_TYPE, Orb, Trig};

/// The orb kinds of the pool (table 0x37c238, 16 bytes each, read by `FUN_001cad50`).
const ORB_KINDS: u32 = 0x37c238;
pub struct StrugglePlugin;

impl Plugin for StrugglePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MissionStart>().add_message::<GlobalEvent>().init_resource::<Mission>().init_resource::<crate::magic_rules::GlobalFlags>();
        let consts = app.world().get_resource::<Disc>().and_then(|d| d.read_root("SLUS_210.05")).and_then(|elf| orb_consts(&elf));
        if consts.is_none() {
            warn!("struggle: the orb constants are not readable from SLUS_210.05; no orb pool");
        }
        app.insert_resource(OrbLib(consts)).init_resource::<OrbDraws>().add_systems(Update, (build_orb_draws, show_orbs).chain()).add_systems(
            FixedUpdate,
            (
                start_mission.in_set(Sim::Enemy).after(crate::enemy::spawn_waves),
                sora_orb_loss.in_set(Sim::SoraHurt).before(crate::sora::receive_hits),
                (enemy_orb_loss, mission_tick, run_orbs).chain().in_set(Sim::Effects),
            ),
        );
    }
}

// ---------------------------------------------------------------------------------------------------
// The mission file

/// The event kind rows (`03system.bin` entry `evtp`, 8 bytes each after an 8-byte head, looked up by
/// `FUN_001d1128`): byte 0 the kind, +2 the flags `FUN_00195ae0` / `FUN_00196170` test.
pub fn event_kinds(sys: &[u8]) -> Vec<(u8, u16)> {
    let Ok(entries) = kh2_bar::parse(sys) else { return Vec::new() };
    let Some(e) = entries.iter().find(|e| e.tag == "evtp") else { return Vec::new() };
    e.data.get(8..).unwrap_or_default().as_chunks::<8>().0.iter().map(|r| (r[0], u16::from_le_bytes([r[2], r[3]]))).collect()
}

// ---------------------------------------------------------------------------------------------------
// Orbs (FUN_001caeb0, FUN_001cad50, FUN_001cadd8)

/// Orbs a reaction knocks out of the victim's counter (`FUN_001caeb0`): by the reaction's class
/// (`FUN_0018b700`): 3 for classes 0, 2 and 3, 15 for 4, 5 and 14, none otherwise; never more than the counter holds.
pub fn orbs_knocked_out(reaction: u8, counter: i32) -> i32 {
    let n = match rules::reaction_class(reaction) {
        0 | 2 | 3 => 3,
        4 | 5 | 14 => 15,
        _ => 0,
    };
    n.min(counter)
}

/// The counter a fighter's orbs belong to: the player's (+0x588 bit 0x80) is slot 0, any other's slot 1
/// (`FUN_001caeb0` and `FUN_001cadd8` both take `(+0x588 & 0x80) == 0` as the slot).
pub fn side_of(is_player: bool) -> usize {
    usize::from(!is_player)
}

/// The orb kind a victim's orbs take (`FUN_001caeb0`: 0 for the player, 2 for any other).
pub fn orb_kind(is_player: bool) -> u8 {
    if is_player { 0 } else { 2 }
}

/// The prize constants with the orb kinds of table 0x37c238 in place of the prize kinds.
pub fn orb_consts(elf: &[u8]) -> Option<Consts> {
    let mut c = Consts::from_exe(elf)?;
    let mut entries = Vec::new();
    for k in 0..4u32 {
        let e = ORB_KINDS + 16 * k;
        let w = |o: u32| {
            let b = exe_bytes(elf, e + o)?;
            Some(u32::from_le_bytes(b))
        };
        entries.push(prize::KindEntry { id: w(0)? as u8, radius: f32::from_bits(w(4)?), bounce_se: w(8)?, pickup_se: w(12)? });
    }
    for (k, e) in entries.into_iter().enumerate() {
        c.kinds[k] = e;
    }
    Some(c)
}

/// The orb constants (None: the executable was not readable).
#[derive(Resource)]
pub struct OrbLib(pub Option<Consts>);

/// A fighter that may take orbs (`FUN_00184d98`: the objects with +0x588 bit 2), in object-list order.
#[derive(Clone, Copy, Debug)]
pub struct Taker {
    pub ent: Entity,
    pub player: bool,
    /// Game space (cm, Y down, Z mirrored): the position (`FUN_0016b960`) and the centre (`FUN_0016b7d0`).
    pub feet: Vec3,
    pub centre: Vec3,
    /// The Draw ability's reach added for the player (+0x588 bit 4).
    pub extra: f32,
    /// The pickup passes it by (`pickup_skipped`).
    pub skip: bool,
}

/// `FUN_001cadd8` passes a fighter by while `FUN_00185a08` (its last requested motion, +0x154 = controller
/// (+0x140) +0x14, is 10 to 39: the reaction motions, host 2.6 `in_reaction`) or `FUN_00185a38` (140 to 150) holds,
/// or while its +0x168 bit 0x40 is set (the writer of that bit was not found; it reads clear here).
pub fn pickup_skipped(motion: Option<usize>) -> bool {
    motion.is_some_and(|m| (10..40).contains(&m) || (140..151).contains(&m))
}

/// One frame of an orb: the prize update without the life count; at the pickup check the first fighter in
/// reach takes it (`FUN_001cadd8`, which returns after the first). Returns (alive, bounce sound, taker side).
pub fn orb_tick(o: &mut Orb, c: &Consts, sweep: impl Fn(Vec3, Vec3) -> prize::Sweep, takers: &[Taker]) -> (bool, Option<u32>, Option<usize>) {
    if o.p.phase == Phase::Absorb {
        let centre = takers.iter().find(|t| Some(t.ent) == o.taker).map(|t| t.centre);
        return (centre.is_some_and(|ce| prize::absorb_step(&mut o.p, c, ce)), None, None);
    }
    let mut bounce = None;
    if o.p.phase == Phase::Fall {
        bounce = prize::fall_physics(&mut o.p, c, &sweep);
    }
    let mut side = None;
    if prize::shrink_or_check(&mut o.p, c, &sweep)
        && o.p.pickable
        && let Some(t) = takers.iter().find(|t| !t.skip && prize::in_reach(c, &o.p, t.feet, t.extra))
    {
        prize::start_absorb(&mut o.p, c, t.centre);
        o.taker = Some(t.ent);
        side = Some(side_of(t.player));
    }
    (true, bounce, side)
}

// ---------------------------------------------------------------------------------------------------
// The mission state

/// `FUN_001b9ce0`: the game over after a manager ends by itself: type 3 for the mission kinds 2, 0x1e and 0x23, else type 1.
pub fn game_over_kind(mission_kind: u8) -> u8 {
    if matches!(mission_kind, 2 | 0x1e | 0x23) { 3 } else { 1 }
}

/// The pause-menu record the end's last step opens (`FUN_001b96e0`: `FUN_001bef10(0x1a)`).
pub const RESULT_MENU_RECORD: u16 = 0x1a;

/// `FUN_001b8b10` + `FUN_001b8d80` + `FUN_001b8ee8` for a mission file: the header, its HUD elements, the
/// counters with their start values, the script.
pub fn build(id: u32, name: &str, file: Vec<u8>, orb_pool: bool, table: Option<&kh2_bdx::HostTable>) -> Result<Active, String> {
    let entries = kh2_bar::parse(&file).map_err(|e| format!("{name}: {e:?}"))?;
    let header = entries.iter().find(|e| e.kind == 2).and_then(|e| parse_header(e.data)).ok_or_else(|| format!("{name}: no header (BAR type 2)"))?;
    let rt = match entries.iter().find(|e| e.kind == 3) {
        Some(e) => {
            let prog = Program::parse(e.data).map_err(|err| format!("{name}: script: {err}"))?;
            let mut rt = Runtime::new(prog);
            if let Some(t) = table {
                rt.set_host_table(t.clone());
            }
            Some(rt)
        }
        None => None,
    };
    let mut layouts: [Option<Layout>; 4] = Default::default();
    for r in &header.records {
        if let Some((slot, l)) = Layout::from_record(r, &entries) {
            layouts[slot] = Some(l);
        }
    }
    drop(entries);
    let mut counters: [Counter; 3] = Default::default();
    let mut clock_rec = None;
    let mut elements = Vec::new();
    for r in &header.records {
        let Some(e) = Element::from_record(r) else { continue };
        match r.op {
            // FUN_001d64d0: counter `slot` (method 0 = FUN_001cf870 -> FUN_001cf508(slot), method +0x14 = FUN_001cf8a0).
            5 => {
                if let Some(c) = counters.get_mut(e.slot as usize) {
                    c.active = true;
                    c.slot = e.slot as i32;
                    c.init(e.start, e.max_word);
                }
            }
            // FUN_001d61c8: the clock record at +0xbc (FUN_001cf528: start, maximum, third word).
            4 => clock_rec = Some(e),
            _ => {}
        }
        if (4..=9).contains(&r.op) {
            elements.push(e);
        }
    }
    Ok(Active {
        id,
        name: name.to_string(),
        header,
        file: Arc::new(file),
        elements,
        counters,
        clock_rec,
        clock: Clock::default(),
        shown: false,
        stopped: false,
        latch: false,
        orb_pool,
        orbs: Vec::new(),
        gone: Vec::new(),
        seed: 1,
        rt,
        events: Vec::new(),
        story: HashSet::new(),
        records: HashSet::new(),
        tallies: HashMap::new(),
        end_cause: None,
        result: None,
        layouts,
        kinds: Vec::new(),
        triggers: Vec::new(),
        state: 0,
        collect_at_once: false,
        end_ev: 0,
        end_arg: 0,
        play: None,
        failed: false,
        ko_loses: false,
        sounds: Vec::new(),
        finish: None,
        pending: None,
    })
}

// ---------------------------------------------------------------------------------------------------
// The mission script's host

/// The integer arguments of a call (named or not).
fn ints(c: &Call) -> Option<(u16, u16, Vec<i32>)> {
    match c {
        Call::Func { group, index, args } => Some((*group, *index, args.iter().map(|a| a.int()).collect())),
        Call::Unknown { group, index, args } => Some((*group, *index, args.iter().map(|&a| a as i32).collect())),
        _ => None,
    }
}

struct MissionHost<'a> {
    a: &'a mut Active,
    /// Calls the host does not know, for the log (once each).
    unknown: &'a mut HashSet<(u16, u16)>,
}

impl Host for MissionHost<'_> {
    fn call(&mut self, c: &Call) -> Value {
        let Some((g, i, args)) = ints(c) else {
            return Value::Int(0);
        };
        let arg = |k: usize| args.get(k).copied().unwrap_or(0);
        let a = &mut *self.a;
        match (g, i) {
            // Debug prints (empty in the retail build).
            (0, 0..=2) | (0, 8) | (0, 24..=26) => Value::None,
            // 0.14 / 0.15: story flags (FUN_0017ff60 / FUN_001802e0).
            (0, 14) => {
                a.story.insert(arg(0));
                Value::None
            }
            // The read also sees the app's story flags (`progress::shared_story`, WP-E5); the
            // mission's own sets stay in `story` for the session.
            (0, 15) => Value::Bool(a.story.contains(&arg(0)) || crate::progress::shared_story().is_some_and(|s| s.lock().test(arg(0) as u16))),
            // 0.69 (0x16e088 -> FUN_001a52b0) / 0.89 (0x16e0b8 -> FUN_001a5328): a bit of the array at 0x33fdb0.
            (0, 69) => {
                a.records.insert(arg(0));
                Value::None
            }
            (0, 89) => Value::Bool(a.records.contains(&arg(0))),
            // 1.41 (0x1727b0): the global event (event, argument).
            (1, 41) => {
                a.events.push((arg(0) as u32, arg(1)));
                Value::None
            }
            // 1.317 (0x173b70): bit 0x20000 of DAT_0035d268. Its readers: the player's knock-out (FUN_001cec00,
            // the player class 0x360410 method +0x5c) sends global event 0x5a instead of the game-over path, and
            // the damage formula (FUN_001a0610 -> FUN_001a0968) scales by the word at 0x1d5d7c8 plus 1 (no writer
            // found besides the clear FUN_001b81f8: factor 1, nothing to port).
            (1, 317) => {
                a.ko_loses = true;
                Value::None
            }
            // 4.3 (FUN_001b9780 -> FUN_001b95a0): the end with event 7 (or 0x9c).
            (4, 3) => {
                info!("struggle: mission {} ends through host 4.3 (FUN_001b9780)", a.name);
                a.host_end(arg(0));
                Value::None
            }
            // 4.16 (FUN_001b97f0): the end with event 0x10.
            (4, 16) => {
                info!("struggle: mission {} ends through host 4.16 (FUN_001b97f0)", a.name);
                a.end_through(0x10, arg(0));
                Value::None
            }
            // 4.11 (FUN_001b9818): the failure path.
            (4, 11) => {
                info!("struggle: mission {} ends through host 4.11 (FUN_001b9818, the failure path)", a.name);
                a.host_fail();
                Value::None
            }
            // 4.22 (0x19e910 -> FUN_001b9908): the stop, the manager ends (FUN_001b9518), global event 0xf.
            (4, 22) => {
                info!("struggle: mission {} ends through host 4.22 (FUN_001b9908): the manager goes, global event 0xf", a.name);
                a.stop();
                a.set_finish(Finish::Gone);
                a.events.push((0xf, arg(0)));
                Value::None
            }
            // 4.29 (FUN_001d6738): a counter's value.
            (4, 29) => Value::Int(a.counter_value(arg(0).max(0) as usize)),
            // 4.5 (FUN_001d6598): set; 4.41 (FUN_001d6620): add; 4.6 / 4.20: one up / down; 4.48: one digit.
            (4, 5) => {
                a.change(arg(1).max(0) as usize, |c| c.set(arg(0)));
                Value::None
            }
            (4, 41) => {
                a.change(arg(1).max(0) as usize, |c| c.add(arg(0)));
                Value::None
            }
            (4, 6) | (4, 20) => {
                a.change(arg(0).max(0) as usize, |c| c.step(i == 6));
                Value::None
            }
            (4, 48) => {
                a.change(arg(2).max(0) as usize, |c| c.set_digit(arg(0), arg(1)));
                Value::None
            }
            // 4.26 (FUN_001d6340): the clock's frames.
            (4, 26) => Value::Int(a.clock.shown),
            // 4.42 (FUN_001d7598 -> FUN_001d7560): a tally byte + 1, held at 99.
            (4, 42) => {
                let t = a.tallies.entry((arg(0), arg(1))).or_insert(0);
                if *t < 99 {
                    *t += 1;
                }
                Value::Int(*t as i32)
            }
            // 4.54 / 4.55 (0x19e9b0 / 0x19e9d0): the latch at 0x35fec8.
            (4, 54) => {
                a.latch = true;
                Value::None
            }
            (4, 55) => Value::Bool(a.latch),
            _ => {
                if self.unknown.insert((g, i)) {
                    warn!("struggle: mission script calls host {g}.{i}, which the app does not map (answered 0)");
                }
                Value::Int(0)
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Systems

/// Script table, unknown-call memory.
#[derive(Default)]
struct ScriptEnv {
    table: Option<kh2_bdx::HostTable>,
    unknown: HashSet<(u16, u16)>,
    loaded: bool,
    kinds: Vec<(u8, u16)>,
}

fn start_mission(disc: Res<Disc>, mut starts: MessageReader<MissionStart>, mut mission: ResMut<Mission>, mut env: Local<ScriptEnv>) {
    let Some(s) = starts.read().last().cloned() else { return };
    // The same group again (the wave spawner repeats a forced group): the manager stays.
    if mission.m.as_ref().is_some_and(|a| a.id == s.id) {
        return;
    }
    if !env.loaded {
        env.loaded = true;
        env.table = disc.read_root("SLUS_210.05").and_then(|e| kh2_bdx::HostTable::from_exe(&e));
        env.kinds = disc.read("03system.bin").map(|s| event_kinds(&s)).unwrap_or_default();
        if env.kinds.is_empty() {
            warn!("struggle: no event kind rows (03system.bin evtp); layouts lock nothing");
        }
    }
    let path = format!("msn/us/{}.bar", s.name);
    let Some(file) = disc.read(&path) else {
        warn!("struggle: {path} is not on the disc; no mission");
        return;
    };
    match build(s.id, &s.name, file, s.orb_pool, env.table.as_ref()) {
        Ok(mut a) => {
            // FUN_001954f8: event 2 runs at once, event 3 becomes the script's thread.
            let mut rt = a.rt.take();
            if let Some(rt) = rt.as_mut() {
                let mut host = MissionHost { a: &mut a, unknown: &mut env.unknown };
                rt.event(&mut host, 2, &[]);
                rt.start_thread(0, 3, &[]);
            }
            a.rt = rt;
            a.kinds = env.kinds.clone();
            a.triggers = s.triggers.clone();
            info!(
                "struggle: mission {} ({:#x}) from {path}: flags {:#06x}, counters {:?}, clock {:?}, orb pool {}, script {}",
                a.name,
                a.id,
                a.header.flags,
                a.counters.iter().filter(|c| c.active).map(|c| format!("slot {} {}/{}", c.slot, c.value, c.max)).collect::<Vec<_>>(),
                a.clock_rec.map(|c| format!("{} s", c.start)),
                a.orb_pool,
                if a.rt.is_some() { "yes" } else { "no" }
            );
            info!(
                "struggle: mission kind {}, sounds {:#x} {:#x} {:#x}, layouts {:?}, area triggers {:?}",
                a.header.kind,
                a.header.se_start,
                a.header.se_end,
                a.header.se_fail,
                a.layouts.iter().map(|l| l.as_ref().map(|l| (l.op, l.entry, l.picture, l.end))).collect::<Vec<_>>(),
                a.triggers
            );
            a.begin();
            mission.m = Some(Box::new(a));
        }
        Err(e) => warn!("struggle: {e}"),
    }
}

/// Orbs a reaction knocks out (`FUN_001caeb0`) into the pool, at the victim's centre.
fn knock_out(a: &mut Active, c: &Consts, player: bool, reaction: u8, centre_game: Vec3) -> i32 {
    if !a.orb_pool {
        return 0;
    }
    let side = side_of(player);
    let n = orbs_knocked_out(reaction, a.counter_value(side));
    if n <= 0 {
        return 0;
    }
    a.change(side, |k| k.add(-n));
    // FUN_001cad50: n orbs of the victim's kind, each launched by FUN_001a3b28 (three generator steps).
    for _ in 0..n {
        let mut v = [0u32; 3];
        for x in &mut v {
            a.seed = a.seed.wrapping_mul(0x10dcd);
            *x = a.seed;
        }
        a.orbs.push(Orb::new(prize::launch(c, orb_kind(player), v, centre_game)));
    }
    n
}

/// App space (Y up) and game space (Y down, Z mirrored).
fn flip(v: Vec3) -> Vec3 {
    Vec3::new(v.x, -v.y, -v.z)
}

/// The centre (`FUN_0016b7d0`: the body's first shape of kind 1) of a body, app cm.
fn centre_of(bodies: &Query<(&Animator, &GlobalTransform)>, body: Entity, feet: Vec3) -> Vec3 {
    bodies.get(body).ok().and_then(|(a, g)| prize::shape_point(a, g, 1)).unwrap_or(feet)
}

/// Hits on Sora: the reaction they give him (as `sora::receive_hits` works it out, which runs right after)
/// knocks his orbs out. Skipped where `receive_hits` skips the reaction (invulnerable, down, guarded).
#[allow(clippy::too_many_arguments)]
fn sora_orb_loss(
    mut hits: MessageReader<Hit>,
    data: Res<GameData>,
    reaction: Option<Res<ReactionData>>,
    lib: Res<OrbLib>,
    mut mission: ResMut<Mission>,
    sora: Query<(Entity, &Transform, &Sora, Option<&ReactionStatus>)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
) {
    let (Some(a), Some(reaction), Some(c)) = (mission.m.as_deref_mut(), reaction, lib.0.as_ref()) else {
        hits.clear();
        return;
    };
    let Ok((me, tf, s, status)) = sora.single() else { return };
    for h in hits.read().filter(|h| h.target == me) {
        if s.invuln > 0.0 || matches!(s.state, State::Down | State::Viewer | State::FormChange) {
            continue;
        }
        let from_front = s.motor.forward().dot(-h.dir) > 0.0;
        // [approx] the guard's repel window (the guard motion before its blend-to-idle range) is taken as the
        // whole guard state.
        if s.state == State::Guard && from_front {
            continue;
        }
        let Some(atk) = data.atkp(h.atkp) else { continue };
        let useless = crate::combat::hit_is_ineffective(atk, &s.stats);
        let ctx = rules::VictimCtx { status: status.map_or(0, |x| x.0), grounded: s.motor.grounded, attacker_flag_4000: false, from_front, hit_part_plain: true };
        let out = rules::victim_outcome(&rules::HitRow::from_atkp(atk), useless, &ctx, &reaction.patn, &reaction.defaults, &reaction.consts);
        let feet = tf.translation / SCALE;
        let centre = centre_of(&bodies, s.body, feet);
        let n = knock_out(a, c, true, out.effective, flip(centre));
        if n > 0 {
            info!("struggle: Sora hit (reaction {}): {n} orbs knocked out, player {} orbs, opponent {}", out.effective, a.counter_value(0), a.counter_value(1));
        }
    }
}

/// Hits on the opponents (the reaction `enemy::receive_hits` reports): their orbs are knocked out.
fn enemy_orb_loss(
    mut hits: MessageReader<AiHit>,
    lib: Res<OrbLib>,
    mut mission: ResMut<Mission>,
    enemies: Query<(&Transform, &Enemy)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
) {
    let (Some(a), Some(c)) = (mission.m.as_deref_mut(), lib.0.as_ref()) else {
        hits.clear();
        return;
    };
    for h in hits.read() {
        let Ok((tf, e)) = enemies.get(h.target) else { continue };
        let feet = tf.translation / SCALE;
        let centre = centre_of(&bodies, e.body, feet);
        let n = knock_out(a, c, false, h.reaction, flip(centre));
        if n > 0 {
            info!("struggle: {} hit (reaction {}): {n} orbs knocked out, player {} orbs, opponent {}", e.name, h.reaction, a.counter_value(0), a.counter_value(1));
        }
    }
}

/// The mission's frame: the event playing and the manager's update (`FUN_001b9140`: the elements come up once the
/// start layout lets the player act, `FUN_001d5f00`; the clock record's show method `FUN_001cf748` starts the
/// clock), the clock ticks, the script runs, the global events of the frame are delivered, the player's knock-out
/// is reported (`FUN_001cec00`), and the end's last step (`FUN_001b96e0`) runs.
#[allow(clippy::too_many_arguments)]
fn mission_tick(
    mut mission: ResMut<Mission>,
    mut out: MessageWriter<GlobalEvent>,
    mut sounds: MessageWriter<crate::audio::PlaySe>,
    sora: Query<&Sora>,
    mut last_hp: Local<Option<i32>>,
    mut unknown: Local<HashSet<(u16, u16)>>,
    mut seen: Local<(bool, bool)>,
    (mut latched, mut seen_area, mut gflags): (Local<bool>, Local<Option<String>>, ResMut<crate::magic_rules::GlobalFlags>),
    area: Option<Res<crate::assets::CurrentArea>>,
) {
    collect_latch(&mut latched, &mut seen_area, area.as_deref().and_then(|a| a.loaded.as_deref()), mission.m.as_deref(), &mut gflags);
    // The player lock of a layout and the stop, for the log (the app's player controller does not read them yet).
    let now = (mission.control_locked(), mission.fight_stopped());
    if *seen != now {
        info!("struggle: player control locked by a layout: {}, fight stopped (FUN_001b9940): {}", now.0, now.1);
        *seen = now;
    }
    let Some(a) = mission.m.as_deref_mut() else {
        *last_hp = None;
        return;
    };
    a.flow_tick();
    if a.clock.tick() {
        a.events.push((EV_CLOCK_END, 0));
    }
    // The player's knock-out (FUN_00186238 -> FUN_00186530 -> method +0x5c, FUN_001cec00 for the player class
    // 0x360410): with bit 0x20000 (host 1.317) it sends global event 0x5a, which the Struggle script takes as a loss.
    if let Ok(s) = sora.single() {
        if s.hp <= 0 && last_hp.is_some_and(|h| h > 0) {
            if a.ko_loses {
                info!("struggle: Sora knocked out: global event 0x5a (FUN_001cec00)");
                a.events.push((EV_PLAYER_KO, 0));
            } else {
                info!("struggle: Sora knocked out without host 1.317's bit: no event 0x5a (menu_gameover decides: game over, or Tinker Bell's revive)");
            }
        }
        *last_hp = Some(s.hp);
    }
    let mut rt = a.rt.take();
    if let Some(rt) = rt.as_mut() {
        let mut host = MissionHost { a, unknown: &mut unknown };
        rt.update_script(&mut host);
    }
    // Events raised while delivering are delivered in the same frame (the game's calls nest).
    let mut guard = 0;
    while !a.events.is_empty() && guard < 16 {
        guard += 1;
        for (id, arg) in std::mem::take(&mut a.events) {
            note_event(a, id, arg);
            if let Some(rt) = rt.as_mut() {
                let mut host = MissionHost { a, unknown: &mut unknown };
                rt.event(&mut host, 10, &[id, arg as u32]);
            }
            out.write(GlobalEvent { id, arg });
        }
    }
    a.rt = rt;
    for se in a.sounds.drain(..) {
        info!("struggle: sound {se:#x} (FUN_002c3318)");
        sounds.write(crate::audio::PlaySe::number(se, None));
    }
    let Some(f) = a.pending.take() else { return };
    let (p, o) = (a.counter_value(0), a.counter_value(1));
    let mut menu = false;
    let mut over = None;
    let gone = match f {
        Finish::Menu => {
            // FUN_001bef10(0x1a) -> FUN_0024f330: menu kind 7 with the `slct` row 0x1a of 14mission.bar
            // (`menu_struggle`). The manager stays; a row of the menu ends it (FUN_001b9c58, `Mission::menu_exit`).
            info!("struggle: last step (FUN_001b96e0): mission kind {} opens the menu of record 0x1a (FUN_001bef10(0x1a) -> FUN_0024f330); the manager stays (player {p} orbs, opponent {o})", a.header.kind);
            menu = true;
            false
        }
        Finish::Event(ev, arg) => {
            out.write(GlobalEvent { id: ev, arg });
            match a.trigger(ev, arg) {
                Some(t) => {
                    info!("struggle: last step (FUN_001b96e0): event {ev:#x} to the area: trigger {t:?} takes it; the manager ends (FUN_001b9518)");
                    true
                }
                None if ev == 8 => {
                    info!("struggle: last step (FUN_001b96e0): event 8 with no trigger: the manager ends with the game-over path (FUN_001b9ce0)");
                    over = Some(GameOverRequest { kind: game_over_kind(a.header.kind), exits: a.game_over_exits() });
                    true
                }
                None => {
                    info!("struggle: last step (FUN_001b96e0): event {ev:#x} to the area: no trigger takes it; the manager stays");
                    false
                }
            }
        }
        Finish::GameOver => {
            info!("struggle: the manager ends with the game-over path (FUN_001b9ce0)");
            over = Some(GameOverRequest { kind: game_over_kind(a.header.kind), exits: a.game_over_exits() });
            true
        }
        Finish::Gone => true,
    };
    if menu {
        mission.menu_request = Some(RESULT_MENU_RECORD);
    }
    if gone {
        mission.end();
    }
    if over.is_some() {
        mission.game_over_request = over;
    }
}
/// Bit 3 of the global flag word 0x35d268 (`GFLAG_COLLECT_AT_ONCE`) as the struggle manager leaves it. The original sets it once
/// in `FUN_001b9140` case 3 and nothing in the manager clears it; it stays until host 1.188 clears it or the area is set up again
/// (`FUN_00190370` zeroes the word, called from `FUN_001689c0`, itself called from `FUN_001048e0`) [inferred: that FUN_001689c0
/// runs at each area load]. The magic system republishes bit 3 from its own script state every tick, so the latch (which outlives
/// the manager) is or'ed in again after it. A new manager that has not reached state 3 also releases the latch.
/// An area change (`loaded` differs from the one last seen) is the area setup: the latch is dropped and the bit cleared, as the
/// word's store of 0 does (`FUN_00190370`).
fn collect_latch(latched: &mut bool, seen: &mut Option<String>, area: Option<&str>, a: Option<&Active>, flags: &mut crate::magic_rules::GlobalFlags) {
    if seen.as_deref() != area {
        *seen = area.map(str::to_owned);
        *latched = false;
        flags.merge(crate::magic_rules::GFLAG_COLLECT_AT_ONCE, 0);
    }
    if let Some(a) = a {
        if a.collect_at_once {
            *latched = true;
        } else if a.state < 3 {
            *latched = false;
        }
    }
    if *latched {
        flags.0 |= crate::magic_rules::GFLAG_COLLECT_AT_ONCE;
    }
}

/// The log lines of the events the Struggle cares about.
fn note_event(a: &mut Active, id: u32, arg: i32) {
    let (p, o) = (a.counter_value(0), a.counter_value(1));
    let cause = match id {
        EV_COUNTER_FULL => Some(if arg == 0 { "win" } else { "lose" }),
        EV_CLOCK_END => Some("time"),
        EV_PLAYER_KO => Some("knock-out"),
        _ => None,
    };
    if let Some(cause) = cause
        && a.end_cause.is_none()
    {
        a.end_cause = Some(cause);
        info!("struggle: end {cause}: player {p} orbs, opponent {o}");
    }
    match id {
        EV_COUNTER_EMPTY => info!("struggle: counter {arg} is empty (global event 0xe)"),
        EV_PLAYER_WON | EV_PLAYER_LOST => {
            let r = if id == EV_PLAYER_WON { "win" } else { "lose" };
            a.result = Some(r);
            info!("struggle: result {r} (global event {id}): player {p} orbs, opponent {o}");
        }
        _ => {}
    }
}

/// The orbs' look: `obj/PRIZE_TT.mdlx` holds one effect file with the prize program `pppRyjMegaBirthPrize`,
/// drawn as the prizes are (`prize::kind_draws`, the pool's draw `FUN_001bf848` [inferred: the PRIZE_TT pool
/// is the same class 0x35fd38 with its own vtable 0x3600d0, whose draw slots were not read]).
#[derive(Resource, Default)]
struct OrbDraws {
    draws: Vec<prize::KindDraw>,
    tried: bool,
}

fn build_orb_draws(
    disc: Res<Disc>,
    mut d: ResMut<OrbDraws>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if d.tried {
        return;
    }
    d.tried = true;
    let Some(pax) = crate::fx::load(&disc, "obj/PRIZE_TT.mdlx", usize::MAX - 9, None) else {
        warn!("struggle: obj/PRIZE_TT.mdlx has no readable effect file; orbs run but are not drawn");
        return;
    };
    match prize::kind_draws(&pax, 4, &mut meshes, &mut materials, &mut images) {
        Some((draws, looks, shapes)) => {
            info!("struggle: orbs drawn from obj/PRIZE_TT.mdlx ({shapes} shapes): kinds {:?}", looks.iter().take(4).map(|l| (l.shape, l.rgb, l.scale)).collect::<Vec<_>>());
            d.draws = draws;
        }
        None => warn!("struggle: obj/PRIZE_TT.mdlx has no prize program; orbs are not drawn"),
    }
}

/// Places the orbs' sprites (as `prize::show_prizes`: facing the camera, scaled by the kind and the size, the
/// shape's frames stepped at 60 Hz).
fn show_orbs(
    mut commands: Commands,
    draws: Res<OrbDraws>,
    mut mission: ResMut<Mission>,
    cam: Query<&GlobalTransform, crate::menu::preview::GameCamera>,
    mut tfs: Query<(&mut Transform, &mut Visibility)>,
    mut ticks: Local<f32>,
    time: Res<Time>,
) {
    for e in mission.gone.drain(..) {
        commands.entity(e).despawn();
    }
    let Some(a) = mission.m.as_deref_mut() else { return };
    for e in a.gone.drain(..) {
        commands.entity(e).despawn();
    }
    let rot = cam.single().map(|g| g.compute_transform().rotation).unwrap_or_default();
    *ticks += time.delta_secs() * crate::data::LOGIC_HZ;
    let steps = ticks.floor() as u32;
    *ticks -= steps as f32;
    for o in &mut a.orbs {
        let Some(d) = draws.draws.get(o.p.kind as usize).filter(|d| !d.frames.is_empty()) else { continue };
        let at = flip(o.p.pos) * SCALE;
        let e = *o.ent.get_or_insert_with(|| commands.spawn((Transform::from_translation(at), Visibility::Hidden)).id());
        let n = d.frames.len();
        let mut f = o.p.frame;
        if f == 0xff {
            f = if d.loops { (o.p.pos.x.to_bits() as usize % n.max(1)) as u8 } else { 0 };
        } else {
            for _ in 0..steps {
                let next = f as usize + 1;
                f = if next < n { next as u8 } else if d.loops { 0 } else { f };
            }
        }
        o.p.frame = f;
        let size = d.scale * o.p.size as f32 / 255.0;
        if let Ok((mut t, mut vis)) = tfs.get_mut(e) {
            t.translation = at;
            t.rotation = rot;
            t.scale = Vec3::splat(size * SCALE);
            *vis = if size > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        }
        if o.shown != Some(f as usize) {
            for k in o.kids.drain(..) {
                commands.entity(k).despawn();
            }
            for sp in &d.frames[(f as usize).min(n - 1)].sprites {
                o.kids.push(commands.spawn((Mesh3d(sp.mesh.clone()), MeshMaterial3d(sp.material.clone()), Transform::default(), bevy::light::NotShadowCaster, ChildOf(e))).id());
            }
            o.shown = Some(f as usize);
        }
    }
}

/// The orb pool's frame (`FUN_001c4d50` walk; update `FUN_001c5288` / rest, pickup `FUN_001cadd8`).
#[allow(clippy::too_many_arguments)]
fn run_orbs(
    lib: Res<OrbLib>,
    mut mission: ResMut<Mission>,
    world: Option<Res<World>>,
    abilities: Option<Res<crate::abilities::Abilities>>,
    sora: Query<(Entity, &Transform, &Sora)>,
    enemies: Query<(Entity, &Transform, &Enemy)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
    mut sounds: MessageWriter<crate::audio::PlaySe>,
) {
    let (Some(a), Some(c)) = (mission.m.as_deref_mut(), lib.0.as_ref()) else { return };
    if a.orbs.is_empty() {
        return;
    }
    let mut takers = Vec::new();
    if let Ok((ent, tf, s)) = sora.single()
        && s.hp > 0
    {
        let feet = tf.translation / SCALE;
        let extra = abilities.as_ref().map_or(0.0, |x| x.block().draw_radius);
        // The player's motion id is its MSET slot / 4 (`Mset::slot`: four state variants per motion).
        let skip = pickup_skipped(bodies.get(s.body).ok().and_then(|(an, _)| an.slot).map(|sl| sl / 4));
        takers.push(Taker { ent, player: true, feet: flip(feet), centre: flip(centre_of(&bodies, s.body, feet)), extra, skip });
    }
    for (ent, tf, e) in &enemies {
        if e.alive() {
            let feet = tf.translation / SCALE;
            let skip = pickup_skipped(bodies.get(e.body).ok().and_then(|(an, _)| an.slot));
            takers.push(Taker { ent, player: false, feet: flip(feet), centre: flip(centre_of(&bodies, e.body, feet)), extra: 0.0, skip });
        }
    }
    let w = world.as_deref();
    let mut picked = Vec::new();
    let mut i = 0;
    while i < a.orbs.len() {
        let o = &mut a.orbs[i];
        let radius = c.kinds.get(o.p.kind as usize).map_or(0.0, |k| k.radius);
        let (alive, bounce, side) = orb_tick(o, c, |s, v| prize::sweep(w, radius, s, v), &takers);
        if let Some(se) = bounce {
            sounds.write(crate::audio::PlaySe::number(se, None).at(flip(o.p.pos) * SCALE));
        }
        if let Some(side) = side {
            picked.push((side, o.p.kind, o.taker));
        }
        if alive {
            i += 1;
        } else {
            let o = a.orbs.swap_remove(i);
            a.gone.extend(o.ent);
        }
    }
    for (side, kind, taker) in picked {
        // FUN_001c5808's sound at the taker's centre, then FUN_001d6620(1, side).
        if let (Some(se), Some(t)) = (c.kinds.get(kind as usize).map(|k| k.pickup_se), takers.iter().find(|t| Some(t.ent) == taker)) {
            sounds.write(crate::audio::PlaySe::number(se, None).at(flip(t.centre) * SCALE));
        }
        a.change(side, |k| k.add(1));
        if std::env::var("KH2_STRUGGLELOG").is_ok_and(|v| v != "0") {
            info!("struggle: orb taken by {}: player {} orbs, opponent {}", if side == 0 { "Sora" } else { "the opponent" }, a.counter_value(0), a.counter_value(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::struggle::{Play, LAYOUT_AFTER, LAYOUT_END, LAYOUT_FAIL, LAYOUT_START};

    fn disc() -> Option<Disc> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        if iso.is_none() {
            eprintln!("skipped: no disc (set KH2_ISO)");
        }
        iso.map(|p| Disc::open(&p))
    }

    /// A Struggle mission from the disc with the manager shown, as the first frame leaves it.
    fn struggle(disc: &Disc, name: &str) -> Active {
        let table = disc.read_root("SLUS_210.05").and_then(|e| kh2_bdx::HostTable::from_exe(&e));
        let file = disc.read(&format!("msn/us/{name}.bar")).expect("mission file");
        let mut a = build(0x128, name, file, true, table.as_ref()).expect("mission");
        let mut rt = a.rt.take().expect("script");
        let mut unknown = HashSet::new();
        rt.event(&mut MissionHost { a: &mut a, unknown: &mut unknown }, 2, &[]);
        rt.start_thread(0, 3, &[]);
        // The thread's first frame (it clears its state and waits).
        rt.update_script(&mut MissionHost { a: &mut a, unknown: &mut unknown });
        a.rt = Some(rt);
        a.kinds = disc.read("03system.bin").map(|s| event_kinds(&s)).unwrap_or_default();
        a
    }

    /// Runs the manager's frames until the last step (at most `max` frames); returns it and the frames taken.
    fn run_to_finish(a: &mut Active, max: i32) -> (Option<Finish>, i32) {
        for f in 1..=max {
            a.flow_tick();
            if let Some(x) = a.pending.take() {
                return (Some(x), f);
            }
        }
        (None, max)
    }

    /// Delivers one global event to the script and runs a few frames; returns the events it raised.
    fn deliver(a: &mut Active, id: u32, arg: i32) -> Vec<(u32, i32)> {
        let mut rt = a.rt.take().unwrap();
        let mut unknown = HashSet::new();
        rt.event(&mut MissionHost { a: &mut *a, unknown: &mut unknown }, 10, &[id, arg as u32]);
        for _ in 0..3 {
            rt.update_script(&mut MissionHost { a: &mut *a, unknown: &mut unknown });
        }
        a.rt = Some(rt);
        assert!(unknown.is_empty(), "unmapped host calls: {unknown:?}");
        std::mem::take(&mut a.events)
    }

    #[test]
    fn the_struggle_missions_hold_two_orb_counters_and_a_minute() {
        let Some(disc) = disc() else { return };
        for name in ["TT04_MS00", "TT04_MS01", "TT04_MS02"] {
            let a = struggle(&disc, name);
            let live: Vec<(i32, i32, i32)> = a.counters.iter().filter(|c| c.active).map(|c| (c.slot, c.value, c.max)).collect();
            eprintln!("{name}: flags {:#06x}, counters {live:?}, clock {:?}, elements {:?}", a.header.flags, a.clock_rec, a.elements);
            assert_eq!(live, vec![(0, 100, 200), (1, 100, 200)], "{name}");
            assert_eq!(a.clock_rec.map(|c| c.start), Some(60), "{name}");
            assert!(a.no_prizes(), "{name}: header flag 0x40");
            // Pictures: counters at BAR entries 12/13 and 14/15, the clock at 16/17.
            assert_eq!(a.elements.iter().map(|e| e.picture_entries()).collect::<Vec<_>>(), vec![Some((12, 13)), Some((14, 15)), Some((16, 17))]);
        }
    }

    #[test]
    fn the_struggle_script_decides_by_a_full_counter_or_by_time() {
        let Some(disc) = disc() else { return };
        for name in ["TT04_MS00", "TT04_MS01", "TT04_MS02"] {
            // The player's counter full (0x7b with slot 0): he wins (159), then the mission ends (4.3).
            let mut a = struggle(&disc, name);
            assert_eq!(deliver(&mut a, EV_COUNTER_FULL, 0), vec![(EV_PLAYER_WON, 0)], "{name}");
            assert!(a.stopped && a.latch, "{name}: ended through 4.54 / 4.3");
            // The opponent's full: he loses (160).
            let mut a = struggle(&disc, name);
            assert_eq!(deliver(&mut a, EV_COUNTER_FULL, 1), vec![(EV_PLAYER_LOST, 0)], "{name}");
            // Time: more orbs wins; a tie loses.
            for (p, o, ev) in [(120, 80, EV_PLAYER_WON), (80, 120, EV_PLAYER_LOST), (100, 100, EV_PLAYER_LOST)] {
                let mut a = struggle(&disc, name);
                a.counters[0].value = p;
                a.counters[1].value = o;
                assert_eq!(deliver(&mut a, EV_CLOCK_END, 0), vec![(ev, 0)], "{name} {p}:{o}");
            }
            // An empty counter decides nothing.
            let mut a = struggle(&disc, name);
            assert_eq!(deliver(&mut a, EV_COUNTER_EMPTY, 1), vec![], "{name}");
            assert!(!a.stopped);
        }
    }

    #[test]
    fn the_struggle_layouts_and_sounds_come_from_the_header() {
        let Some(disc) = disc() else { return };
        for name in ["TT04_MS00", "TT04_MS01", "TT04_MS02"] {
            let a = struggle(&disc, name);
            let h = &a.header;
            assert_eq!((h.kind, h.se_start, h.se_end, h.se_fail, h.unlock), (2, 0xbcd, 0xbce, 0xbcf, 0), "{name}");
            assert!(h.ends_in_menu());
            let l = |s: usize| a.layouts[s].as_ref().map(|l| (l.op, l.kind, l.entry, l.picture, l.object, l.end, l.picture_at));
            // The pictures' groups play 90 (st_t group 1), 300 (ed_t group 1) and 270 (ed_t group 2) ticks.
            let ticks = |s: usize, g: usize| a.layouts[s].as_ref().and_then(|l| l.group_ticks.get(g).copied().flatten());
            assert_eq!((ticks(LAYOUT_START, 1), ticks(LAYOUT_END, 1), ticks(LAYOUT_FAIL, 2)), (Some(90), Some(300), Some(270)), "{name}");
            // Record 1: event entry 2 with picture 4 (st_t) -> kind 0xd; records 2 and 3: entries 5 and 8 with the
            // two ed_t pictures; every event ends at frame 500 and shows its picture from frame 11.
            assert_eq!(l(LAYOUT_START), Some((1, 0xd, 2, 4, 0, 500, Some(11))), "{name}");
            assert_eq!(l(LAYOUT_END), Some((2, 0xe, 5, 7, 0, 500, Some(11))), "{name}");
            assert_eq!(l(LAYOUT_FAIL), Some((3, 0xf, 8, 10, 0, 500, Some(11))), "{name}");
            assert_eq!(l(LAYOUT_AFTER), None, "{name}");
            // The event kinds: all three hold the player (flag bit 3 clear); only the start's hides the field HUD.
            let flags = |k: u8| a.kinds.iter().find(|(x, _)| *x == k).map(|(_, f)| *f);
            assert_eq!((flags(0xd), flags(0xe), flags(0xf)), (Some(0x60), Some(0x44), Some(0x64)));
        }
    }

    #[test]
    fn the_start_layout_holds_the_hud_until_it_ends() {
        let Some(disc) = disc() else { return };
        let mut a = struggle(&disc, "TT04_MS00");
        a.begin();
        assert_eq!(a.sounds, vec![0xbcd]);
        let p = a.play.clone().expect("start layout");
        assert!(p.locks_control() && p.hides_hud());
        assert_eq!(a.clock.pause & 4, 4, "an event pauses the clock");
        let mut shown_at = None;
        for f in 1..=1100 {
            a.flow_tick();
            if a.play.as_ref().is_some_and(|p| p.frame() == 11) && !a.play.as_ref().unwrap().picture_shown() {
                panic!("the picture shows from event frame 11");
            }
            if a.shown {
                shown_at = Some(f);
                break;
            }
        }
        // The picture starts at event frame 11 (tick 22) and its group of 90 ticks ends the event, not the end
        // frame (500 event frames = tick 1000): control comes back at tick 22 + 90 = 112 (WP-G1).
        assert_eq!(shown_at, Some(112));
        assert!(a.play.is_none() && a.clock.pause & 4 == 0 && a.state == 1);
        assert!(a.counters[0].shown && a.clock.state == 1);
    }

    #[test]
    fn state_three_sets_the_collect_bit_and_it_outlives_the_manager() {
        use crate::magic_rules::{GFLAG_COLLECT_AT_ONCE as BIT, GlobalFlags};
        let Some(disc) = disc() else { return };
        let mut a = struggle(&disc, "TT04_MS00");
        let (mut latched, mut f, mut seen) = (false, GlobalFlags(0), None);
        a.state = 2;
        a.flow_tick();
        assert_eq!(a.state, 3);
        collect_latch(&mut latched, &mut seen, Some("tt04"), Some(&a), &mut f);
        assert_eq!(f.0, 0, "the bit comes with the state 3 step, not before");
        a.flow_tick();
        assert!(a.collect_at_once);
        collect_latch(&mut latched, &mut seen, Some("tt04"), Some(&a), &mut f);
        assert_eq!(f.0 & BIT, BIT);
        // The magic system republishes its own bit; the latch puts it back after the manager is gone.
        f.merge(BIT, 0);
        collect_latch(&mut latched, &mut seen, Some("tt04"), None, &mut f);
        assert_eq!(f.0 & BIT, BIT);
        // A fresh manager (new area) releases it.
        let fresh = struggle(&disc, "TT04_MS00");
        f.merge(BIT, 0);
        collect_latch(&mut latched, &mut seen, Some("tt04"), Some(&fresh), &mut f);
        assert_eq!(f.0, 0);
        // An area change drops the latch and clears the bit even with the manager gone.
        let mut a = struggle(&disc, "TT04_MS00");
        a.state = 3;
        a.flow_tick();
        collect_latch(&mut latched, &mut seen, Some("tt04"), Some(&a), &mut f);
        assert_eq!(f.0 & BIT, BIT);
        collect_latch(&mut latched, &mut seen, Some("tt02"), None, &mut f);
        assert_eq!((f.0, latched), (0, false), "a new area clears the word");
        collect_latch(&mut latched, &mut seen, Some("tt02"), None, &mut f);
        assert_eq!(f.0, 0);
    }

    #[test]
    fn the_struggle_end_plays_its_layout_then_opens_the_menu() {
        let Some(disc) = disc() else { return };
        for name in ["TT04_MS00", "TT04_MS01", "TT04_MS02"] {
            // Won: host 4.3 -> event 7, the end sound and record 2's layout; the mission kind 2 opens the menu.
            let mut a = struggle(&disc, name);
            a.state = 1;
            a.shown = true;
            a.counters.iter_mut().for_each(|c| c.shown = true);
            deliver(&mut a, EV_COUNTER_FULL, 0);
            assert!(a.stopped && !a.failed, "{name}");
            assert_eq!((a.state, a.end_ev, a.sounds.clone()), (4, 7, vec![0xbce]), "{name}");
            assert_eq!(a.play.as_ref().map(|p| (p.what, p.locks_control(), p.hides_hud())), Some(("end", true, false)));
            // The last step comes in the tick the picture's group (300 ticks, from tick 22) has played through.
            assert_eq!(run_to_finish(&mut a, 2000), (Some(Finish::Menu), 322), "{name}");
            // Lost: host 4.11 -> the failure sound and record 3's layout, then event 8's last step: the menu.
            let mut a = struggle(&disc, name);
            a.shown = true;
            a.counters.iter_mut().for_each(|c| c.shown = true);
            deliver(&mut a, EV_COUNTER_FULL, 1);
            assert!(a.stopped && a.failed, "{name}");
            assert_eq!((a.state, a.end_ev, a.end_arg, a.sounds.clone()), (4, 8, -1, vec![0xbcf]), "{name}");
            assert_eq!(a.play.as_ref().map(|p| (p.what, p.layout.group())), Some(("failure", 2)));
            assert_eq!(run_to_finish(&mut a, 2000), (Some(Finish::Menu), 292), "{name}");
            // A stopped mission's counters stay.
            let v = a.counter_value(0);
            a.change(0, |c| c.add(5));
            assert_eq!(a.counter_value(0), v);
        }
    }

    #[test]
    fn with_a_trigger_and_no_flag_0x40_the_end_waits_for_the_last_frames() {
        let Some(disc) = disc() else { return };
        let mut a = struggle(&disc, "TT04_MS00");
        a.header.flags &= !0x40;
        a.header.kind = 0;
        a.triggers = vec![Trig { event: 7, arg: 0xffff, story_flag: None, plays_event: false, jumps: true, jump: None }];
        a.end_through(7, 3);
        assert_eq!(a.state, 2);
        // State 2 -> 3, then the wait for 4 frames left (FUN_00196668 against 4.0 at 0x37bbc8), then state 4.
        let mut at = None;
        for f in 1..=1100 {
            a.flow_tick();
            if a.state == 4 {
                at = Some(f);
                break;
            }
        }
        // The wait for the last frames ends with the event itself now: the picture's group (300 ticks from tick
        // 22) ends it before event frame 496.
        assert_eq!(at, Some(322));
        // No record 0xd: nothing new plays (FUN_001d3538 returns), then the last step: the event goes to the area.
        assert_eq!(run_to_finish(&mut a, 100), (Some(Finish::Event(7, 3)), 1));
    }

    #[test]
    fn a_knock_out_with_host_1_317_loses_the_struggle() {
        let Some(disc) = disc() else { return };
        for name in ["TT04_MS00", "TT04_MS01", "TT04_MS02"] {
            let mut a = struggle(&disc, name);
            assert!(a.ko_loses, "{name}: event 3 calls host 1.317");
            a.shown = true;
            assert_eq!(deliver(&mut a, EV_PLAYER_KO, 0), vec![(EV_PLAYER_LOST, 0)], "{name}");
            assert!(a.failed, "{name}: the loss takes the failure path");
        }
    }

    /// `FUN_001cec00`'s view of the manager: host 1.317's bit, the menu kinds and the retry / quit jump records of the
    /// triggers for events 0x8b and 0x8d (`FUN_001b9c58`); `FUN_001b9ce0`'s type of the game over.
    #[test]
    fn the_knock_out_view_carries_the_bit_the_kind_and_the_retry_and_quit_jumps() {
        assert!(Mission::default().ko_view().is_none(), "no manager, no view");
        let Some(disc) = disc() else { return };
        let mut a = struggle(&disc, "TT04_MS00");
        let t = |event, jump| Trig { event, arg: 0xffff, story_flag: None, plays_event: false, jumps: true, jump: Some(jump) };
        a.triggers = vec![t(0x8b, [2, 2, 0, 0, 0xb6, 1]), t(0x8d, [1, 2, 4, 55, 0, 0]), t(7, [1, 1, 1, 1, 1, 1])];
        assert_eq!(a.game_over_exits(), GameOverExits { retry: Some([2, 2, 0, 0, 0xb6, 1]), quit: Some([1, 2, 4, 55, 0, 0]) });
        let kind = a.header.kind;
        let m = Mission { m: Some(Box::new(a)), ..Default::default() };
        let v = m.ko_view().expect("a manager runs");
        assert!(v.loses, "TT04_MS00's event 3 calls host 1.317");
        assert_eq!(v.menu_kind, matches!(kind, 2 | 0x19 | 0x20));
        assert_eq!(v.exits.quit, Some([1, 2, 4, 55, 0, 0]));
        assert_eq!((game_over_kind(2), game_over_kind(0x1e), game_over_kind(0x23), game_over_kind(7)), (3, 3, 3, 1));
    }

    #[test]
    fn area_triggers_match_by_event_and_argument() {
        let t = Trig { event: 7, arg: 0xffff, story_flag: Some(2083), plays_event: true, jumps: true, jump: Some([2, 2, 0, 0, 126, 1]) };
        assert!(t.matches(7, -1) && t.matches(7, 5));
        assert!(!t.matches(8, -1));
        let u = Trig { arg: 2, ..t };
        assert!(u.matches(7, 2) && !u.matches(7, 3) && !u.matches(7, -1));
        let k = kh2_ard::Trigger { event: 7, arg: 0xffff, subs: vec![(2, vec![2083]), (0, vec![8, 12593, 50]), (1, vec![0, 2, 2, 0, 0, 126, 1])] };
        assert_eq!(Trig::from_ard(&k), t);
    }

    #[test]
    fn the_pickup_passes_fighters_in_reaction_motions() {
        assert!(!pickup_skipped(None));
        assert!(!pickup_skipped(Some(9)));
        assert!(pickup_skipped(Some(10)) && pickup_skipped(Some(39)));
        assert!(!pickup_skipped(Some(40)) && !pickup_skipped(Some(139)));
        assert!(pickup_skipped(Some(140)) && pickup_skipped(Some(150)));
        assert!(!pickup_skipped(Some(151)));
    }

    #[test]
    fn reactions_knock_out_3_or_15_orbs() {
        assert_eq!(orbs_knocked_out(0, 100), 3);
        assert_eq!(orbs_knocked_out(2, 100), 3);
        assert_eq!(orbs_knocked_out(6, 100), 3);
        assert_eq!(orbs_knocked_out(4, 100), 15);
        assert_eq!(orbs_knocked_out(5, 100), 15);
        assert_eq!(orbs_knocked_out(14, 100), 15);
        assert_eq!(orbs_knocked_out(1, 100), 0);
        assert_eq!(orbs_knocked_out(12, 100), 0);
        assert_eq!(orbs_knocked_out(4, 7), 7);
        assert_eq!(orbs_knocked_out(3, 0), 0);
        assert_eq!((side_of(true), side_of(false)), (0, 1));
        assert_eq!((orb_kind(true), orb_kind(false)), (0, 2));
    }

    /// A start-like layout without a disc: the picture from event frame 11 (tick 22), a group of 90 ticks, an
    /// end frame of 500 event frames, the start's event kind (holds the player).
    fn synthetic_play(group_ticks: Option<i32>, picture_at: Option<i32>) -> Play {
        let layout = Layout { op: 1, kind: 0xd, entry: 2, picture: 4, object: 0, end: 500, picture_at, select: 0, select2: 0, group_ticks: vec![None, group_ticks] };
        Play { what: "start", layout, flags: 0x60, ticks: 0 }
    }

    #[test]
    fn the_start_event_ends_when_its_picture_group_has_played_through() {
        let mut p = synthetic_play(Some(90), Some(11));
        assert_eq!(p.picture_end(), Some(112));
        let mut unlock = None;
        for t in 1..=1100 {
            p.ticks = t;
            assert_eq!(p.picture_shown(), t >= 22,"tick {t}");
            if p.ended() {
                unlock = Some(t);
                break;
            }
            assert!(p.locks_control(), "tick {t}: the player is held until the event ends");
        }
        assert_eq!(unlock, Some(112), "control returns at 2 * 11 + 90 ticks, not at the end frame (tick 1000)");
    }

    #[test]
    fn an_event_without_a_finishing_picture_ends_at_its_end_frame() {
        // A looping group (None) or no picture command: only the end frame (500 event frames = tick 1000) ends it.
        for mut p in [synthetic_play(None, Some(11)), synthetic_play(Some(90), None)] {
            assert_eq!(p.picture_end(), None);
            p.ticks = 999;
            assert!(!p.ended());
            p.ticks = 1000;
            assert!(p.ended());
        }
    }

    #[test]
    fn mission_header_records_decode() {
        let mut d = vec![0u8; 0x1c];
        d[2..4].copy_from_slice(&0x128u16.to_le_bytes());
        d[4..6].copy_from_slice(&0x0d61u16.to_le_bytes());
        for w in [0x0004_0005u32, 100, 200, 0, 0x0101_000f, 0] {
            d.extend_from_slice(&w.to_le_bytes());
        }
        let h = parse_header(&d).unwrap();
        assert_eq!((h.id, h.flags, h.records.len()), (0x128, 0x0d61, 1));
        let e = Element::from_record(&h.records[0]).unwrap();
        assert_eq!(e, Element { op: 5, start: 100, max_word: 200, third: 0, picture: 0xf, side: 1, slot: 1 });
        assert_eq!(e.picture_entries(), Some((14, 15)));
    }

    #[test]
    fn an_orb_rests_and_goes_to_the_first_fighter_in_reach() {
        let Some(elf) = disc().and_then(|d| d.read_root("SLUS_210.05")) else { return };
        let c = orb_consts(&elf).unwrap();
        assert_eq!((c.kinds[0].id, c.kinds[2].id), (0, 2));
        let floor = |s: Vec3, v: Vec3| {
            let e = s + v;
            if e.y >= 0.0 { prize::Sweep { end: e.with_y(0.0), on_floor: true } } else { prize::Sweep { end: e, on_floor: false } }
        };
        let mut o = Orb::new(prize::launch(&c, 2, [0x8000_0000; 3], Vec3::new(0.0, -100.0, 0.0)));
        let far = Taker { ent: Entity::from_raw_u32(1).unwrap(), player: true, feet: Vec3::new(1000.0, 0.0, 0.0), centre: Vec3::new(1000.0, -90.0, 0.0), extra: 0.0, skip: false };
        let near = Taker { ent: Entity::from_raw_u32(2).unwrap(), player: false, feet: Vec3::ZERO, centre: Vec3::new(0.0, -90.0, 0.0), extra: 0.0, skip: false };
        let mut taken = None;
        for _ in 0..600 {
            let (alive, _, side) = orb_tick(&mut o, &c, floor, &[far, near]);
            assert!(alive, "an orb's life does not run down");
            if side.is_some() {
                taken = side;
                break;
            }
        }
        assert_eq!(taken, Some(1), "the opponent standing on it takes it for slot 1");
        assert_eq!(o.p.phase, Phase::Absorb);
    }

    fn counter(max: i32, v: i32) -> Counter {
        Counter { active: true, shown: true, slot: 1, max, value: v, ..Counter::default() }
    }

    #[test]
    fn a_counter_holds_its_range_and_reports_empty_and_full() {
        let mut c = counter(100, 5);
        assert_eq!(c.add(-3), None);
        assert_eq!(c.value, 2);
        assert_eq!(c.add(-7), Some((EV_COUNTER_EMPTY, 1)));
        assert_eq!(c.value, 0);
        // Already empty: no second event.
        assert_eq!(c.step(false), None);
        assert_eq!(c.add(150), Some((EV_COUNTER_FULL, 1)));
        assert_eq!(c.value, 100);
        assert_eq!(c.step(true), None);
        assert_eq!(c.value, 100);
        // Hidden counters and counters without a slot send nothing.
        let mut h = Counter { shown: false, ..counter(10, 3) };
        assert_eq!(h.set(0), None);
        let mut n = Counter { slot: -1, ..counter(10, 3) };
        assert_eq!(n.set(0), None);
        assert_eq!(n.value, 0);
    }

    #[test]
    fn a_counter_digit_is_replaced_in_place() {
        let mut c = counter(99999, 1234);
        c.set_digit(1, 7);
        assert_eq!(c.value, 1274);
        c.set_digit(0, 0);
        assert_eq!(c.value, 1270);
    }

    #[test]
    fn counter_init_reads_the_maximum_word() {
        let mut c = Counter::default();
        c.init(100, 0x4000_0000 | 200);
        assert_eq!((c.value, c.max, c.show_max), (100, 200, true));
        c.init(30, 0);
        assert_eq!((c.value, c.max), (30, 200));
    }

    #[test]
    fn the_countdown_runs_out_after_its_length() {
        let mut k = Clock::default();
        k.begin(2, 0);
        let mut ended = None;
        for f in 1..=200 {
            if k.tick() {
                ended = Some(f);
                break;
            }
        }
        // The start frame is taken when the clock starts; the tick that makes it run already counts.
        assert_eq!(ended, Some(120));
        assert_eq!(k.shown, 0);
        assert_eq!(k.state, 0);
        // A paused clock does not move.
        let mut p = Clock::default();
        p.begin(1, 0);
        p.pause = 1;
        for _ in 0..100 {
            assert!(!p.tick());
        }
        assert_eq!(p.shown, 60);
    }

    #[test]
    fn the_count_up_clock_ends_at_its_limit() {
        let mut k = Clock::default();
        k.begin(0, 1);
        let n = (1..100).find(|_| k.tick());
        assert_eq!(n, Some(60));
        assert_eq!(k.shown, 60);
        assert_eq!(Clock::split(60 * 75 + 30, 100.0 / 60.0), (0, 1, 15, 50));
    }

    /// `FUN_001997f8`: a group already at wave 200 does not count up and so never takes the wrap, even with the wrap
    /// wave (header +0x23) at 200 and a wave with members to go back to (header +0x24).
    #[test]
    fn a_group_at_wave_200_with_the_wrap_at_200_does_not_wrap() {
        use crate::enemy::{AreaGroup, GroupPart, KindTwo};
        let mut header = [0u8; 0x2c];
        header[0x22] = 9;
        header[0x23] = 200;
        header[0x24] = 5;
        let part = GroupPart { id: 0x8a, kind: 2, disabled: false, header, activators: Vec::new(), entities: vec![(2, 5, 1)], counted: Vec::new() };
        let g = AreaGroup { program: 0x50, name: "b_t".into(), parts: vec![part], members: Vec::new(), skipped: Vec::new(), player: None, player_yaw: 0.0, mission: None, orb_pool: false, triggers: Vec::new() };
        let mut two = KindTwo { part: 0, timer: 0.0, wave: 200, killed: vec![7] };
        for _ in 0..3 {
            assert!(two.run(&g, |_| false).is_empty());
            assert_eq!(two.wave, 200, "stays at 200, no wrap to wave 5");
        }
        assert_eq!(two.killed, vec![7], "the table of the defeated is not touched by a wrap that did not happen");
    }

    /// A flat floor at height 0 (centimetres) over +-30 m, with a wall across x = 10 cm.
    fn floor_and_wall() -> World {
        let tri = |v: [[f32; 3]; 3]| v;
        let quad = |a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]| [tri([a, b, c]), tri([a, c, d])];
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for t in quad([-3000.0, 0.0, -3000.0], [-3000.0, 0.0, 3000.0], [3000.0, 0.0, 3000.0], [3000.0, 0.0, -3000.0])
            .into_iter()
            .chain(quad([10.0, -100.0, -3000.0], [10.0, 500.0, -3000.0], [10.0, 500.0, 3000.0], [10.0, -100.0, 3000.0]))
        {
            for v in t {
                indices.push(positions.len() as u32);
                positions.push(v);
            }
        }
        let mesh = kh2_model::Mesh { positions, uvs: Vec::new(), colors: Vec::new(), indices, texture: 0, transparent: false, joints: Vec::new(), weights: Vec::new(), uv_scroll: 0 };
        World::from_meshes(&[mesh])
    }

    /// `ring_place_in` on a real collision `World` (end to end: positions in, placements out): on the flat floor every
    /// placement lies on the floor (y 0) between the ring's limits; the wall at x = 10 cm blocks the lines toward +x, so
    /// no placement lies past it; a ring with no floor under it gives the centre back.
    #[test]
    fn ring_place_in_on_a_floor_with_a_wall_places_on_the_floor_and_never_past_the_wall() {
        use crate::enemy::{ring_place_in, PlaceRng, Ring};
        let col = floor_and_wall();
        let ring = Ring { centre: Vec3::new(0.0, 0.5, 0.0), min: 1.5, max: 4.5, heights: None };
        let mut rng = PlaceRng::default();
        let (mut far_west, mut near_wall) = (false, false);
        for _ in 0..200 {
            let p = ring_place_in(&mut rng, &ring, &col);
            let d = Vec2::new(p.x, p.z).length();
            assert!((1.5 - 1e-3..=4.5 + 1e-3).contains(&d), "distance {d} from the centre");
            assert!(p.y.abs() < 1e-4, "on the floor: {p:?}");
            assert!(p.x < 0.1, "not past the wall at x = 0.1 m: {p:?}");
            far_west |= p.x < -3.0;
            near_wall |= p.x > -1.0;
        }
        assert!(far_west && near_wall, "the placements spread round the open side");
        // No floor anywhere under the ring: the search finds nothing and gives the centre.
        let off = Ring { centre: Vec3::new(500.0, 0.5, 500.0), ..ring };
        assert_eq!(ring_place_in(&mut rng, &off, &col), off.centre);
    }
}
