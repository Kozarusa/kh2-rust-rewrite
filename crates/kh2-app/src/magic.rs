//! Sora's magic, phase 1: the Magic sub-menu of the command menu, the cast motion, the MP cost and
//! the MP charge, and the spell objects of Fire and Cure.
//!
//! The game keeps one magic resource per slot (FUN_001a45c0): the `magc` row the slot uses
//! (`magic_rules::pick_row`), the row's BAR (`magic/<SPELL>_<level>.mag`) with two BDX scripts and a
//! PAX effect file. A cast (FUN_00179400 -> FUN_001a4798) plays the row's motion, creates a script
//! thread on the spell's program and starts its event 20 with (caster, effect file, kind, `magi` row)
//! as the thread's code (FUN_0016c500). Frame trigger 17 of the cast motion (FUN_00188498 case 0x11 ->
//! FUN_001a4a60) pays the MP (FUN_001769e0) and restarts the thread at event 21, which makes the
//! spell. A cast whose motion is left before the trigger ends with event 20 (it waits for the
//! motion to change) and costs nothing.
//!
//! The scripts are not run in the BDX interpreter here (the interpreter has no way yet to start an
//! event as a persistent thread, see docs/magic.md); what Fire's and Cure's scripts do is ported by
//! hand below, step by step, from their disassembly (`kh2 bdx magic/FIRE_1.mag`). Blizzard and
//! Thunder make their projectiles in request threads (host 2.20) whose routines are not ported:
//! they cast, pay and start their first effects, and make no attack (logged).
//!
//! Keys: the command menu answers T (cursor up), G (cursor down), B (back); J / left mouse
//! executes the selected command (the game's X button): Attack attacks, Magic opens the list, a
//! spell is cast. `KH2_KEYS` drives T, G and B too (the harness maps J). `KH2_MAGIC=f,b,t,c[,m,r]`
//! sets the spell levels owned (default 1,1,1,1,0,0: the app has no inventory). `KH2_MAGIC_LOG=1`
//! logs the menu, the casts, MP, hits and damage.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use kh2_anim::moveset::{Chain, Windows};
use kh2_anim::triggers::{frame, range};
use kh2_anim::PlayerState;
use kh2_model::collision::kind as shape_kind;
use kh2_tables::tables::{self as t, Cmd, Magc};

use crate::assets::Disc;
use crate::combat::{self, Fighter, Hit, HurtSpheroid, Team, Volume};
use crate::data::GameData;
use crate::fx::{self, Bind, LoadedPax, Placement, RunningEffect};
use crate::hud::hud_menu::{CommandMenu, MenuEntry, MenuPage};
use crate::magic_rules::{self as rules, Charge, HitMemory, MagiRow, Paid, Status};
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::{Battle, Pad, Sora, State};
use crate::SCALE;

/// `atkp` id of Fire's attack objects (FIRE_*.mag event 21: `attack_create(ctx, 502, -1, kind)`).
pub const ATKP_FIRE: u16 = 502;
/// `atkp` ids of Cure's heal on its target and on another member (CURE_*.mag event 21).
pub const ATKP_CURE_SELF: u16 = 708;
pub const ATKP_CURE_OTHER: u16 = 709;
/// `magi` offsets the scripts read: attack radius and height, attack time (Fire), heal radius (Cure).
const MAGI_RADIUS: usize = 0x0;
const MAGI_HEIGHT: usize = 0x4;
const MAGI_TIME: usize = 0x8;
const MAGI_CURE_RANGE: usize = 0x38;
/// The ids of the base command list (Attack, Magic, Items, Drive: the rows whose `Execute` is the
/// same as their id), see `hud_menu::base_entries`.
const BASE_COMMANDS: [u16; 4] = [1, 2, 3, 4];
/// Ticks a cast request waits for Sora to accept it (app rule: the game keeps a pending command
/// and retries it while the playing motion has no window, the retry limit was not traced).
const REQUEST_TICKS: u32 = 20;
/// Sound of the menu? none: the game's menu sounds are not ported.
/// Size step of the swept attack test (FUN_001a2cc0: `min(radius, height) * DAT_0037b460`, 0.5).
const SWEEP_STEP: f32 = 0.5;
/// FUN_001a2fc8: a move longer than twice the frame step (`DAT_00356f40` = 1.0) sets the sweep flag.
const SWEEP_MIN: f32 = 2.0;

pub struct MagicPlugin;

impl Plugin for MagicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MagicState>()
            .add_systems(Startup, setup.after(crate::sora::setup))
            .add_systems(Update, (harness_keys, menu).chain().after(crate::sora::read_pad))
            .add_systems(FixedUpdate, run.in_set(Sim::Sora).after(crate::sora::tick));
    }
}

fn log_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_MAGIC_LOG").is_ok_and(|v| v != "0"))
}

macro_rules! mlog {
    ($($t:tt)*) => {
        if log_on() {
            info!($($t)*);
        }
    };
}

/// What the game loads per slot (FUN_001a45c0) and the tables the rules read.
#[derive(Resource)]
pub struct MagicData {
    pub magc: Vec<Magc>,
    pub cmds: Vec<Cmd>,
    pub magi: Vec<MagiRow>,
    /// Spell levels owned per slot (the count of the slot's `magc` item).
    pub levels: [u8; rules::SLOTS as usize],
    pub world: u8,
    /// The row each slot uses, with its effect file.
    pub slots: Vec<Option<(Magc, Option<Arc<LoadedPax>>)>>,
    /// `prty` MpDrive of Sora's row (FUN_00186440 reads the player's `prty` row at +0x12c, +0x1c).
    pub mp_drive: f32,
}

impl MagicData {
    fn cmd(&self, id: u16) -> Option<&Cmd> {
        self.cmds.iter().find(|c| c.id == id)
    }

    fn slot_of_cmd(&self, cmd: u16) -> Option<u8> {
        self.slots.iter().position(|s| s.as_ref().is_some_and(|(r, _)| r.command == cmd)).map(|i| i as u8)
    }

    /// FUN_001ba2c0 for a magic command, with the `DisableForm` mask of command 2 (FUN_001b7290).
    fn status(&self, cmd: u16, mp: i32, charging: bool, form: u8) -> Status {
        let Some(c) = self.cmd(cmd) else { return Status::Unavailable };
        let mask = self.cmd(c.execute).map_or(c.disable_form, |e| e.disable_form);
        rules::availability(c.cost, mp, charging, false, rules::form_disables(mask, form))
    }
}

/// The command menu as this module drives it: which page is open and the cursors.
#[derive(Default, Debug, Clone)]
pub struct MenuState {
    pub sub_open: bool,
    pub cursor: usize,
    pub sub_cursor: usize,
}

#[derive(Resource, Default)]
pub struct MagicState {
    pub charge: Charge,
    pub menu: MenuState,
    /// A spell command waiting for Sora (command id, ticks left).
    request: Option<(u16, u32)>,
    casts: Vec<Cast>,
    attacks: Vec<SpellAttack>,
    memory: HashMap<Entity, HitMemory>,
    next_group: u32,
}

/// One cast: the script instance of FUN_001a4798.
struct Cast {
    slot: u8,
    cmd: u16,
    cost: u8,
    kind: u32,
    magi: MagiRow,
    pax: Option<Arc<LoadedPax>>,
    /// The cast motion's slot: event 20 waits while it plays (host 1.15 `motion_state`).
    motion: Option<usize>,
    /// Element started by event 20 ([g]+0x1c), for event 21 to stop.
    charge_fx: Option<u16>,
    step: Step,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    /// Event 20: waiting for frame trigger 17 or the end of the motion.
    Waiting,
    /// Event 21 ran its first slice; `ball` and `ball2` are the elements whose position the
    /// attacks follow, `t` the attack time left.
    Fire { ball: u16, ball2: Option<u16>, attacks: Option<(usize, Option<usize>)>, t: f32 },
    /// The script has ended.
    Done,
}

/// An attack object of a spell (FUN_001a2ba0): its group, the base `atkp` row, its position this and
/// last frame, its size (radius +0x100, half height +0x104, centimetres) and whether it is live.
struct SpellAttack {
    group: u32,
    base_row: usize,
    pos: Option<Vec3>,
    prev: Option<Vec3>,
    radius: f32,
    height: f32,
    live: bool,
}

fn setup(mut commands: Commands, disc: Res<Disc>, data: Res<GameData>, world: Option<Res<crate::drive::DriveWorld>>) {
    let (magc, cmds) = {
        let Ok(mut a) = disc.0.lock() else { return };
        let magc = kh2_tables::load::<Magc>(&mut a, &t::MAGC).map(|d| d.rows).unwrap_or_default();
        let cmds = kh2_tables::load::<Cmd>(&mut a, &t::CMD).map(|d| d.rows).unwrap_or_default();
        (magc, cmds)
    };
    let magi = disc
        .read("03system.bin")
        .and_then(|f| {
            let top = kh2_bar::parse(&f).ok()?;
            let pref = kh2_bar::find(&top, "pref")?;
            let inner = kh2_bar::parse(pref.data).ok()?;
            Some(rules::parse_magi(kh2_bar::find(&inner, "magi")?.data))
        })
        .unwrap_or_default();
    let mut levels = [1u8, 1, 1, 1, 0, 0];
    if let Ok(v) = std::env::var("KH2_MAGIC") {
        for (i, x) in v.split(',').enumerate().take(levels.len()) {
            levels[i] = x.trim().parse().unwrap_or(0);
        }
    }
    let world_id = world.map_or(0, |w| w.id as u8);
    let common = disc.read("effect/texcommon.dpd").and_then(|b| kh2_fx::Dpd::parse(&b).ok());
    let slots: Vec<_> = (0..rules::SLOTS)
        .map(|id| {
            let row = rules::pick_row(&magc, id, levels[id as usize], world_id)?.clone();
            let pax = fx::load(&disc, &row.file_name, 100 + id as usize, common.as_ref()).map(Arc::new);
            if pax.is_none() {
                warn!("magic: {} has no effect file", row.file_name);
            }
            Some((row, pax))
        })
        .collect();
    info!(
        "magic: slots {} (levels {levels:?}, world {world_id}); magi rows {}; MpDrive {}",
        slots.iter().flatten().map(|(r, _)| format!("{} cmd {}", r.file_name, r.command)).collect::<Vec<_>>().join(", "),
        magi.len(),
        data.prty.mp_drive
    );
    commands.insert_resource(MagicData { magc, cmds, magi, levels, world: world_id, slots, mp_drive: data.prty.mp_drive });
}

/// `KH2_KEYS` entries for the menu keys the harness does not map (T, G, B), pressed the same way.
fn harness_keys(time: Res<Time>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    let Ok(spec) = std::env::var("KH2_KEYS") else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    for item in spec.split(',') {
        let mut parts = item.split(':');
        let (Some(at), Some(k)) = (parts.next(), parts.next()) else { continue };
        let hold: f32 = parts.next().and_then(|h| h.parse().ok()).unwrap_or(0.05);
        let Ok(at) = at.parse::<f32>() else { continue };
        let key = match k {
            "T" => KeyCode::KeyT,
            "G" => KeyCode::KeyG,
            "B" => KeyCode::KeyB,
            _ => continue,
        };
        if t >= at && t - dt < at {
            keys.press(key);
        } else if t >= at + hold && t - dt < at + hold {
            keys.release(key);
        }
    }
}

/// The command menu: the base list and the Magic sub-menu (FUN_001bbee8's list), the cursor keys,
/// and the attack button executing the selected command. The list entries carry the commands'
/// own names and icons; a spell that is not usable (FUN_001ba2c0 status 5) is drawn in style 1.
#[allow(clippy::too_many_arguments)]
fn menu(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    data: Option<Res<MagicData>>,
    mut st: ResMut<MagicState>,
    mut pad: ResMut<Pad>,
    mut cm: ResMut<CommandMenu>,
    sora: Query<&Sora>,
    drive: Option<Res<crate::drive::DriveState>>,
) {
    let Some(data) = data else { return };
    let Ok(s) = sora.single() else { return };
    let form = drive.map_or(0, |d| d.form);
    let charging = st.charge.active();
    let list = rules::magic_list(&data.magc, &data.levels, data.world);
    let base: Vec<&Cmd> = BASE_COMMANDS.iter().filter_map(|&e| data.cmds.iter().find(|c| c.id == e && c.execute == e)).collect();
    let up = keys.just_pressed(KeyCode::KeyT);
    let down = keys.just_pressed(KeyCode::KeyG);
    let back = keys.just_pressed(KeyCode::KeyB);
    let exec = keys.just_pressed(KeyCode::KeyJ) || mouse.just_pressed(MouseButton::Left);
    let step = |c: &mut usize, n: usize| {
        // [inferred] the cursor wraps; the game's list cursor code was not traced.
        if n > 0 {
            if up {
                *c = (*c + n - 1) % n;
            }
            if down {
                *c = (*c + 1) % n;
            }
        }
    };
    let st = &mut *st;
    if st.menu.sub_open {
        step(&mut st.menu.sub_cursor, list.len());
        if back {
            st.menu.sub_open = false;
            mlog!("magic: menu: back to the command list");
        }
    } else {
        step(&mut st.menu.cursor, base.len());
    }
    if (up || down) && log_on() {
        info!("magic: menu: cursor {} sub {} (sub-menu open {})", st.menu.cursor, st.menu.sub_cursor, st.menu.sub_open);
    }
    if exec {
        let sel_base = base.get(st.menu.cursor).map(|c| c.id);
        if st.menu.sub_open {
            // The attack button casts the selected spell.
            pad.steal_attack();
            if let Some(&cmd) = list.get(st.menu.sub_cursor) {
                let status = data.status(cmd, s.mp, charging, form);
                if status.usable() {
                    st.request = Some((cmd, REQUEST_TICKS));
                    mlog!("magic: menu: cast command {cmd} selected (status {status:?}, MP {})", s.mp);
                } else {
                    mlog!("magic: menu: command {cmd} not usable (status {status:?}, MP {}, charging {charging})", s.mp);
                }
            }
        } else if sel_base == Some(rules::EXECUTE_MAGIC) {
            pad.steal_attack();
            let status = data.status(rules::EXECUTE_MAGIC, s.mp, charging, form);
            if status.usable() && !list.is_empty() {
                st.menu.sub_open = true;
                st.menu.sub_cursor = st.menu.sub_cursor.min(list.len() - 1);
                mlog!("magic: menu: Magic opened: {} spells {:?} (MP {}, charging {charging})", list.len(), list, s.mp);
            } else {
                mlog!("magic: menu: Magic not usable (status {status:?}, {} spells)", list.len());
            }
        } else if sel_base != Some(1) {
            // Items and Drive are not run from this menu yet: the press does nothing (not an attack).
            pad.steal_attack();
            mlog!("magic: menu: command {sel_base:?} is not ported from the menu");
        }
    }
    // The menu the HUD draws.
    let entry = |c: &Cmd, style: u8| MenuEntry { message: c.message_id as u32, icon: c.cmd_icon, style };
    let mut pages = vec![MenuPage {
        kind: 0,
        entries: base
            .iter()
            .map(|c| entry(c, if c.id == rules::EXECUTE_MAGIC && !data.status(c.id, s.mp, charging, form).usable() { 1 } else { 0 }))
            .collect(),
        cursor: st.menu.cursor,
    }];
    if st.menu.sub_open {
        let kind = data.cmd(rules::EXECUTE_MAGIC).map_or(1, |c| c.sub_menu.max(0) as u8);
        let entries = list
            .iter()
            .filter_map(|&id| data.cmd(id).map(|c| entry(c, if data.status(id, s.mp, charging, form).usable() { 0 } else { 1 })))
            .collect();
        pages.push(MenuPage { kind, entries, cursor: st.menu.sub_cursor });
    }
    cm.visible = true;
    cm.pages = pages;
}

/// The effect position of an element bound to `owner` (host 0.53, FUN_001d9d00: the bind record's
/// position): the owner's matrix times the element's own translation (game axes, Y and Z flipped).
fn element_position(pax: &LoadedPax, id: u16, owner: Mat4) -> Vec3 {
    let t = pax.pax.elements_for(id).next().map_or([0.0; 3], |e| e.translation);
    owner.transform_point3(Vec3::new(t[0], -t[1], -t[2]))
}

fn stop_effect(running: &mut Query<&mut RunningEffect>, pax: &LoadedPax, id: u16, owner: Entity) {
    for mut r in running.iter_mut() {
        if r.lib.id == pax.id && r.game_id == id && matches!(r.bind, Bind::Owner(o) if o == owner) {
            r.effect.end();
        }
    }
}

type Victim = (Entity, Vec<(Volume, HurtSpheroid)>, Vec3);

/// The 60 Hz part: the MP charge, accepting a cast, the cast trigger, the spell scripts and their
/// attacks' hits.
#[allow(clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    data: Option<Res<MagicData>>,
    game: Res<GameData>,
    mut st: ResMut<MagicState>,
    mut battle: ResMut<Battle>,
    abilities: Res<crate::sora::Abilities>,
    (block, drive): (Option<Res<crate::abilities::Abilities>>, Option<Res<crate::drive::DriveState>>),
    mut sora_q: Query<(Entity, &mut Sora)>,
    mut anims: Query<&mut Animator>,
    locals: Query<(&Transform, Option<&ChildOf>)>,
    fighters: Query<(Entity, &Fighter, &GlobalTransform)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
    mut running: Query<&mut RunningEffect>,
    mut hits: MessageWriter<Hit>,
    enemies: Query<&crate::enemy::Enemy>,
    mut recharge: ResMut<crate::hud::hud_gauge::MpRecharge>,
) {
    let Some(data) = data else { return };
    let Ok((me, mut s)) = sora_q.single_mut() else { return };
    let st = &mut *st;
    let form = drive.as_ref().map_or(0, |d| d.form);

    // MP charge (FUN_00175f48 each frame; the regeneration multiplier is the block's +0x34). An MP
    // refill from outside (the drive form start, FUN_001a04b0 -> FUN_001a0240) ends the charge.
    if st.charge.active() {
        if s.mp > 0 {
            mlog!("magic: MP refilled from outside ({}): charge ended", s.mp);
            st.charge = Charge::default();
        } else {
            let regen = block.as_ref().map_or(1.0, |b| b.block().mp_regen);
            if st.charge.tick(regen) {
                s.mp = game.sora_max_mp;
                mlog!("magic: MP charge over: MP {}", s.mp);
            }
        }
    }
    recharge.progress = st.charge.left;
    recharge.total = st.charge.total;

    let body_e = s.body;
    let Ok(mut body) = anims.get_mut(body_e) else { return };

    // Accept a pending cast (FUN_00174cf8: FUN_00176b00 with the hit latch forced, then FUN_00179400).
    if let Some((cmd, left)) = st.request.take() {
        let airborne = !s.motor.grounded || body.trigger(range::NO_GRAVITY);
        let (free, combo, fin) = match s.state {
            State::Ground | State::Air => (true, false, false),
            State::Attack { .. } | State::Magic => (
                body.trigger(range::GROUNDED) || body.trigger(range::AIR) || body.trigger(range::BLEND_TO_IDLE),
                body.trigger(range::ALLOW_COMBO),
                body.trigger(range::ALLOW_FINISHER),
            ),
            _ => (false, false, false),
        };
        let (mg, ma, mf) = abilities.combo_block();
        let mut chain = Chain { airborne, abilities: abilities.bits(), combo: s.hits, finishers: s.finishers, max_ground: mg, max_air: ma, max_finishers: mf, finisher_window: fin };
        let slot = data.slot_of_cmd(cmd);
        let row = slot.and_then(|i| data.slots[i as usize].as_ref());
        let status = data.status(cmd, s.mp, st.charge.active(), form);
        let pressed = (free || combo || fin).then(|| chain.press(Windows { free, combo }, true)).flatten();
        match (row, pressed) {
            (Some((row, pax)), Some(press)) if status.usable() && !st.casts.iter().any(|c| c.step == Step::Waiting) => {
                s.hits = chain.combo;
                s.finishers = chain.finishers;
                let finisher = form != 6 && rules::is_finisher(chain.combo, airborne, mg, ma);
                let (motion_id, blend, fin_motion) = rules::cast_motion(row, airborne, finisher);
                let kind = rules::cast_kind(row.level, fin_motion, form);
                battle.ticks = battle.ticks.max(240.0);
                let pstate = PlayerState::BattleWeapon;
                let mslot = body.rig.mset.slot(motion_id as usize, pstate);
                body.restart(mslot, false, blend);
                s.enter(State::Magic, None);
                if airborne {
                    // The mover's method +0xc with sstm MagicJump (FUN_001a4798) [inferred: a jump start].
                    s.motor.start_jump(game.sstm.magic_jump);
                }
                let voice = rules::cast_voice(row, fin_motion, false);
                let magi = rules::magi_row(&data.magi, kind).cloned().unwrap_or_default();
                let cost = data.cmd(cmd).map_or(0, |c| c.cost);
                mlog!(
                    "magic: cast {} (command {cmd}, slot {}, level {}): {press:?}, combo {} finisher {finisher}, kind {kind}, motion {motion_id} ({}) blend {blend}, airborne {airborne}, cost {cost}, MP {}, voice {voice} (not played: no voice hook), magi {:?}",
                    row.file_name,
                    row.id,
                    row.level,
                    chain.combo,
                    body.tag(),
                    s.mp,
                    &magi.0[..magi.0.len().min(16)]
                );
                // Event 20 (FIRE/BLIZZARD/THUNDER/CURE): the charge effect of the cast. Wisdom and Final
                // (kind / 10 = 1, 3) start none in Fire; every other case starts element 0.
                let charge_fx = (!(row.id == 0 && matches!(kind / 10, 1 | 3)) && row.id <= 3).then_some(0u16);
                if let (Some(id), Some(p)) = (charge_fx, pax) {
                    fx::start(&mut commands, p, id, Bind::Owner(body_e));
                }
                st.casts.push(Cast { slot: row.id, cmd, cost, kind, magi, pax: pax.clone(), motion: mslot, charge_fx, step: Step::Waiting });
            }
            _ => {
                if left > 1 && status.usable() {
                    st.request = Some((cmd, left - 1));
                } else {
                    mlog!("magic: cast of command {cmd} dropped (state {:?}, status {status:?}, windows free {free} combo {combo} finisher {fin})", s.state);
                }
            }
        }
    }

    // Frame trigger 17 of the motion this tick (FUN_00188498 case 0x11): pay, then event 21.
    let fired = body.motion().zip(body.triggers()).is_some_and(|(m, t)| {
        let g = |f: f32| f * crate::data::LOGIC_HZ / m.fps.max(1.0);
        let (from, to) = (g(body.prev_frame), g(body.frame));
        from != to && t.crossed(from, to).any(|f| f.id == frame::MAGIC_CAST)
    });
    let owner_m = fx::world_now(body_e, &locals);
    let airborne_now = !s.motor.grounded;
    let (magic_state, playing) = (s.state == State::Magic, body.slot);
    let in_motion = |c: &Cast| magic_state && playing == c.motion;
    for c in st.casts.iter_mut() {
        if c.step != Step::Waiting {
            continue;
        }
        if fired && in_motion(c) {
            // FUN_001a4a60 -> FUN_001769e0: the cost byte comes off MP (FUN_00186368).
            let before = s.mp;
            match rules::pay(s.mp, game.sora_max_mp, c.cost, data.mp_drive) {
                Paid::Mp(v) => s.mp = v,
                Paid::Charge { total } => {
                    s.mp = 0;
                    st.charge = Charge { left: total, total };
                }
            }
            mlog!("magic: trigger 17 at frame {:.1}: MP spent {} -> {} (cost {}){}", body.gframe(), before, s.mp, c.cost, if st.charge.active() { format!(", MP charge {:.0} frames", st.charge.total) } else { String::new() });
            c.step = event21(&mut commands, c, &data, &game, &mut s, me, body_e, owner_m, airborne_now, &mut running, &mut st.attacks, &mut st.next_group);
        } else if !in_motion(c) {
            // Event 20 ends when the caster's motion changes (host 1.15): no spell, no cost.
            mlog!("magic: cast of command {} ended before its trigger (Sora {:?})", c.cmd, s.state);
            c.step = Step::Done;
        }
    }

    // The scripts after their first slice (Fire's attack loop).
    for c in st.casts.iter_mut() {
        if let Step::Fire { ball, ball2, attacks, t } = c.step {
            let Some(pax) = c.pax.clone() else {
                c.step = Step::Done;
                continue;
            };
            let (a1, a2) = match attacks {
                Some(a) => a,
                None => {
                    // After the yield: the attack objects (FIRE event 21).
                    let a1 = new_attack(&mut st.attacks, &mut st.next_group, &game, ATKP_FIRE, c.kind, c.magi.f(MAGI_RADIUS), c.magi.f(MAGI_HEIGHT));
                    let mut a2 = None;
                    if c.kind / 10 == 3 {
                        // Host 2.31 copies the attack with id 502, sized 30 smaller; a finisher widens the first to 450.
                        a2 = new_attack(&mut st.attacks, &mut st.next_group, &game, ATKP_FIRE, c.kind, c.magi.f(MAGI_RADIUS) - 30.0, c.magi.f(MAGI_HEIGHT) - 30.0);
                        if c.kind & 1 == 1
                            && let Some(i) = a1
                        {
                            st.attacks[i].radius = 450.0;
                        }
                    }
                    let Some(a1) = a1 else {
                        warn!("magic: no atkp row {ATKP_FIRE} for kind {}", c.kind);
                        c.step = Step::Done;
                        continue;
                    };
                    mlog!("magic: Fire attack {} made: atkp {ATKP_FIRE} kind {} radius {} height {} for {} frames", st.attacks[a1].group, c.kind, st.attacks[a1].radius, st.attacks[a1].height, c.magi.f(MAGI_TIME));
                    (a1, a2)
                }
            };
            if t > 0.0 {
                let p = element_position(&pax, ball, owner_m);
                move_attack(&mut st.attacks[a1], p);
                if let (Some(b2), Some(i2)) = (ball2, a2) {
                    move_attack(&mut st.attacks[i2], element_position(&pax, b2, owner_m));
                }
                c.step = Step::Fire { ball, ball2, attacks: Some((a1, a2)), t: t - 1.0 };
            } else {
                // Host 2.13: the attack objects are finished.
                st.attacks[a1].live = false;
                if let Some(i2) = a2 {
                    st.attacks[i2].live = false;
                }
                mlog!("magic: Fire attack {} over", st.attacks[a1].group);
                c.step = Step::Done;
            }
        }
    }
    st.casts.retain(|c| c.step != Step::Done);

    // The attacks against the enemies' hurt shapes, then the victims' hit memory counts down.
    let victims: Vec<Victim> = fighters
        .iter()
        .filter(|(_, f, _)| f.team == Team::Enemy && f.hittable)
        .filter_map(|(e, f, g)| {
            let (a, bg) = bodies.get(f.hurt_body).ok()?;
            let shapes = a
                .rig
                .model
                .shapes
                .iter()
                .filter(|sh| sh.kind == shape_kind::BATTLE || sh.kind == shape_kind::TARGET)
                .map(|sh| (combat::shape_volume(sh, a, bg), combat::hurt_spheroid(sh, a, bg)))
                .collect();
            Some((e, shapes, g.translation()))
        })
        .collect();
    let sora_pos = s.motor.pos * SCALE;
    for a in st.attacks.iter().filter(|a| a.live) {
        let Some(cur) = a.pos else { continue };
        for (te, shapes, tpos) in &victims {
            let mem = st.memory.entry(*te).or_default();
            let row = &game.atkp[a.base_row];
            if !mem.may_hit(row.r#type, a.group) {
                continue;
            }
            let Some((point_from, sph)) = sweep_points(a, cur).into_iter().find_map(|c| {
                let v = attack_volume(c, a.radius, a.height);
                shapes.iter().find(|(h, _)| combat::overlap(&v, h)).map(|(_, sph)| (c, *sph))
            }) else {
                continue;
            };
            let n = mem.count_hit(a.group);
            let vi = rules::variant(&game.atkp, a.base_row, n);
            let vrow = &game.atkp[vi];
            mem.mark(a.group, vrow.interval);
            let dir = (*tpos - sora_pos).with_y(0.0).normalize_or(Vec3::Z);
            let point = combat::victim_point(&sph, point_from);
            let hit = Hit { attacker: me, target: *te, atkp: vrow.id as i16, dir, point, row: Some(vrow.sub_id) };
            if log_on() {
                // The game's damage for this row (FUN_001a0610 before the boosts the receiver adds).
                let expected = game.hit_row(&hit).zip(enemies.get(*te).ok()).map(|(r, e)| game.damage(r, &s.stats, &e.stats));
                info!(
                    "magic: hit by attack {} on {te:?}: atkp {} row SubId {} (hit {n}, Power {}, Element {}, Interval {}), magic {}, damage {expected:?}",
                    a.group, vrow.id, vrow.sub_id, vrow.power, vrow.element, vrow.interval, s.stats.magic
                );
            }
            hits.write(hit);
        }
    }
    for m in st.memory.values_mut() {
        m.tick();
    }
    st.attacks.retain(|a| a.live);
}

/// The first slice of event 21 of the spell's script (after the MP is paid).
#[allow(clippy::too_many_arguments)]
fn event21(
    commands: &mut Commands,
    c: &Cast,
    data: &MagicData,
    game: &GameData,
    s: &mut Sora,
    me: Entity,
    body: Entity,
    owner_m: Mat4,
    airborne: bool,
    running: &mut Query<&mut RunningEffect>,
    _attacks: &mut [SpellAttack],
    _next: &mut u32,
) -> Step {
    let Some(pax) = c.pax.as_ref() else { return Step::Done };
    let start = |commands: &mut Commands, id: u16| fx::start(commands, pax, id, Bind::Owner(body));
    let k10 = c.kind / 10;
    let fin = c.kind & 1 == 1;
    match c.slot {
        0 => {
            // FIRE: Final stops the charge effect and starts element 1.
            if k10 == 3 {
                if let Some(id) = c.charge_fx {
                    stop_effect(running, pax, id, body);
                }
                start(commands, 1);
            }
            let (ball, extra) = if airborne { (5, 7) } else { (2, 6) };
            start(commands, ball);
            if (k10 == 1 && fin) || k10 == 2 || (k10 == 3 && fin) {
                start(commands, extra);
            }
            let mut ball2 = None;
            if k10 == 3 {
                start(commands, 10);
                ball2 = Some(10);
                for id in if fin { [11, 12] } else { [8, 9] } {
                    start(commands, id);
                }
            }
            mlog!("magic: Fire event 21: element {ball} (airborne {airborne}), attack follows it for {} frames", c.magi.f(MAGI_TIME));
            Step::Fire { ball, ball2, attacks: None, t: c.magi.f(MAGI_TIME) }
        }
        3 => {
            // CURE: stop the charge effect, element 1 on the caster, element 2 at the target's centre,
            // then every party member in range of the centre is healed (atkp 708 on the target).
            if let Some(id) = c.charge_fx {
                stop_effect(running, pax, id, body);
            }
            start(commands, 1);
            // The target is the caster (the app has no command target record); its bone 0x4001 is
            // taken as its root [inferred].
            let centre = owner_m.transform_point3(Vec3::ZERO);
            fx::start_placed(commands, pax, 2, Bind::Fixed(Mat4::from_translation(centre)), Placement { replace_translation: true, ..default() });
            let me_pos = s.motor.pos * SCALE;
            let dist = (me_pos - centre).with_y(0.0).length() / SCALE;
            let range = c.magi.f(MAGI_CURE_RANGE);
            if s.hp > 0 && dist - range < 0.0 {
                start(commands, 3);
                let row = rules::attack_row(&game.atkp, ATKP_CURE_SELF, c.kind).map(|i| &game.atkp[i]);
                match row {
                    Some(r) => {
                        let heal = rules::heal_amount(r, s.stats.strength, s.stats.magic);
                        let before = s.hp;
                        s.hp = (s.hp + heal).min(s.stats.max_hp);
                        mlog!("magic: Cure heals Sora: atkp {} SubId {} Power {} x magic {} -> {heal}; HP {before} -> {} (range {range}, distance {dist:.0})", r.id, r.sub_id, r.power, s.stats.magic, s.hp);
                    }
                    None => warn!("magic: no atkp row {ATKP_CURE_SELF} for kind {}", c.kind),
                }
            }
            let _ = (me, data);
            mlog!("magic: Cure: no other party member to heal (atkp {ATKP_CURE_OTHER} for them)");
            Step::Done
        }
        1 | 2 => {
            // BLIZZARD / THUNDER: their projectiles are request threads (host 2.20, routine 0x319 of
            // BLIZZARD_1) that are not ported; nothing is made.
            warn!("magic: {} cast: its projectile threads are not ported (no attack, no effect)", if c.slot == 1 { "Blizzard" } else { "Thunder" });
            Step::Done
        }
        _ => Step::Done,
    }
}

fn new_attack(attacks: &mut Vec<SpellAttack>, next: &mut u32, game: &GameData, id: u16, kind: u32, radius: f32, height: f32) -> Option<usize> {
    let base_row = rules::attack_row(&game.atkp, id, kind)?;
    *next += 1;
    // FUN_001a30e0: the height is the radius unless it is positive.
    let height = if height > 0.0 { height } else { radius };
    attacks.push(SpellAttack { group: *next, base_row, pos: None, prev: None, radius, height, live: true });
    Some(attacks.len() - 1)
}

/// Host 2.12 (FUN_001a2fc8): the previous position is kept for the swept test.
fn move_attack(a: &mut SpellAttack, p: Vec3) {
    a.prev = a.pos.or(Some(p));
    a.pos = Some(p);
}

/// The points FUN_001a2cc0 tests: the current position, then, after a move of more than two units,
/// steps of `min(radius, height) / 2`... toward the previous position.
fn sweep_points(a: &SpellAttack, cur: Vec3) -> Vec<Vec3> {
    let mut out = vec![cur];
    let Some(prev) = a.prev else { return out };
    let d = (prev - cur) / SCALE;
    let len = d.length();
    let step = a.radius.min(a.height);
    if len <= SWEEP_MIN || step <= 0.0 {
        return out;
    }
    let dir = d / len;
    let mut left = len;
    let mut p = cur / SCALE;
    while left > step * SWEEP_STEP {
        p += dir * step;
        left -= step;
        out.push(p * SCALE);
    }
    out
}

/// An attack volume (FUN_00189610): radius and half height around the centre; as the app's
/// capsules, a vertical segment of `height - radius` either side [inferred shape].
fn attack_volume(c: Vec3, radius: f32, height: f32) -> Volume {
    let half = (height - radius).max(0.0) * SCALE;
    Volume { a: c - Vec3::Y * half, b: c + Vec3::Y * half, r: radius * SCALE }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_steps_back_toward_the_previous_position() {
        let a = SpellAttack { group: 1, base_row: 0, pos: None, prev: Some(Vec3::new(0.0, 0.0, 1.0)), radius: 20.0, height: 40.0, live: true };
        let pts = sweep_points(&a, Vec3::ZERO);
        // 100 units at steps of 20 while more than 10 is left: 5 extra points.
        assert_eq!(pts.len(), 6);
        assert!((pts[1].z - 0.2).abs() < 1e-5);
        let still = SpellAttack { prev: Some(Vec3::new(0.0, 0.0, 0.01)), ..a };
        assert_eq!(sweep_points(&still, Vec3::ZERO).len(), 1, "a move of 1 unit tests one place");
    }

    /// The rules against the disc: Fire, Blizzard, Thunder and Cure at level 1 (`KH2_ISO` needed).
    #[test]
    fn magic_rows_from_the_disc() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let (magc, cmds, atkp) = {
            let mut a = disc.0.lock().unwrap();
            (
                kh2_tables::load::<Magc>(&mut a, &t::MAGC).unwrap().rows,
                kh2_tables::load::<Cmd>(&mut a, &t::CMD).unwrap().rows,
                kh2_tables::load::<t::Atkp>(&mut a, &t::ATKP).unwrap().rows,
            )
        };
        let list = rules::magic_list(&magc, &[1, 1, 1, 1, 0, 0], 2);
        eprintln!("magic list {list:?}");
        assert_eq!(list.len(), 4);
        for &c in &list {
            let cmd = cmds.iter().find(|x| x.id == c).unwrap();
            assert_eq!(cmd.execute, rules::EXECUTE_MAGIC);
            eprintln!("cmd {c}: cost {} argument {}", cmd.cost, cmd.argument);
        }
        // Cure costs all MP.
        let cure = cmds.iter().find(|x| x.id == list[3]).unwrap();
        assert_eq!(cure.cost, rules::COST_ALL);
        // Fire's level-1 attack rows: kind 0 and its variant for the third hit.
        let base = rules::attack_row(&atkp, ATKP_FIRE, 0).expect("502 kind 0");
        let v0 = rules::variant(&atkp, base, 0);
        let v2 = rules::variant(&atkp, base, 2);
        eprintln!("502: base SubId {} power {} interval {}; hit 2 SubId {} power {}", atkp[base].sub_id, atkp[base].power, atkp[base].interval, atkp[v2].sub_id, atkp[v2].power);
        assert_eq!(v0, base);
        assert_ne!(v2, base, "a later hit uses a later row");
        // Cure on self heals with type 6: (magic * Power + 99) / 100.
        let c = rules::attack_row(&atkp, ATKP_CURE_SELF, 0).expect("708 kind 0");
        assert_eq!(atkp[c].r#type, 6);
        let heal = rules::heal_amount(&atkp[c], 2, 6);
        assert_eq!(heal, (6 * atkp[c].power as i32 + 99) / 100);
        // magi rows and MpDrive.
        let f = disc.read("03system.bin").unwrap();
        let top = kh2_bar::parse(&f).unwrap();
        let inner = kh2_bar::parse(kh2_bar::find(&top, "pref").unwrap().data).unwrap();
        let magi = rules::parse_magi(kh2_bar::find(&inner, "magi").unwrap().data);
        eprintln!("magi rows {}; row 0 {:?}", magi.len(), &magi[0].0[..magi[0].0.len().min(16)]);
        assert!(magi[0].f(MAGI_TIME) > 0.0, "Fire's attack time");
    }
}
