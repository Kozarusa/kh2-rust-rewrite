//! Reaction commands (the Triangle prompt): offered by motion range trigger 20 of the object that
//! gives the reaction, accepted with a key, handed back to the offering object. See
//! docs/limits-reactions.md for the rules and the gaps.
//!
//! Game rules ported (addresses in `SLUS_210.05`):
//! - FUN_00188030 case 0x14: a range trigger of id 20 that covers the current frame calls
//!   FUN_00188ad0(object, params[0] = command id, params[1] = position index, 0 when absent).
//! - FUN_00188ad0: needs the player to be in the object list and not downed (FUN_0019a3f8); builds a
//!   target record for the offering object and writes the command into the command manager's slot 1
//!   with priority 100; a slot holding a priority above 100 is kept (FUN_001bb3e0). The manager clears
//!   its slots every tick (FUN_001bb368), so an offer lasts while the trigger covers the frame.
//! - The slot's status (FUN_001ba670 -> FUN_001ba2c0): reaction commands fall through the Execute
//!   switch, so only the command flags (FUN_001b70b8) and the `DisableForm` mask (FUN_001b7290)
//!   decide, then the distance test of FUN_0019c3c8 (horizontal distance from the target to the
//!   player above `DistRange` marks it unavailable).
//! - Accepting (FUN_0019c670 pushes slot 1 as input action 0xcb, the player runs it through
//!   FUN_00177b18 -> FUN_00177c20): bump the use counter `ReactionCount`; refuse unless the target
//!   object is still in the object list (FUN_001b80e8); set the player's bytes +0xb04 and +0xb05 to 1
//!   (FUN_00176a70, the combo step counters); apply `CmdAction` (1 stop the player if moving, 2 clear
//!   the velocity at +0x20 and call the mover with a zero vector; both clear the vector at +0xad0);
//!   then deliver the command to the target object as script event 0xb with the arguments (player
//!   object, command id, target record) (FUN_0016b420 -> FUN_0016c620).
//!
//! The target side is the script: `enemy_ai.rs` delivers `ReactionAccepted` as event 11 to the
//! target's BDX runtime, and what the script then asks of the *player* comes back here as
//! [`PlayerReactionCmd`] (the hook for `sora.rs`, see docs/limits-reactions.md).

use std::collections::HashMap;

use bevy::prelude::*;
use kh2_anim::Triggers;
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::enemy::Enemy;
use crate::reaction_parts::{self as parts, Table};
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::{Sora, State};
use crate::SCALE;

/// Range trigger id of the offer.
pub const TRIGGER: u8 = 20;

/// One offer read from a motion's triggers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offer {
    pub cmd: u16,
    /// `params[1]` when the trigger has two or more parameters, else 0 (the target offset).
    pub pos: i16,
}

/// The offers a motion makes at `gframe` (a global 60 Hz frame): every range trigger 20 covering it.
pub fn offers_at(t: &Triggers, gframe: f32) -> Vec<Offer> {
    t.active(gframe)
        .filter(|r| r.id == TRIGGER)
        .filter_map(|r| Some(Offer { cmd: *r.params.first()? as u16, pos: r.params.get(1).copied().unwrap_or(0) }))
        .collect()
}

/// Every offer a motion set can make: slot, command, position index, frame range (the disc scan).
#[cfg(test)]
pub fn scan_mset(m: &kh2_anim::Mset) -> Vec<(usize, String, Offer, i16, i16)> {
    let mut out = Vec::new();
    for slot in 0..m.slots.len() {
        let Some(s) = m.get(slot) else { continue };
        for r in s.triggers.ranges.iter().filter(|r| r.id == TRIGGER) {
            if let Some(&c) = r.params.first() {
                out.push((slot, s.tag.clone(), Offer { cmd: c as u16, pos: r.params.get(1).copied().unwrap_or(0) }, r.start, r.end));
            }
        }
    }
    out
}

/// Execute values the player's availability switch (FUN_001ba2c0) handles itself. A reaction command
/// with one of these is not decided by flags alone and is reported as not ported.
const EXECUTE_SWITCH: &[u16] = &[1, 2, 4, 5, 0x37, 0x48, 0x51, 0x67, 0x68, 0x7f, 0x13c, 0x14a, 0x1c4];

/// What the availability test reads.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ctx {
    /// The player's form id (0 none).
    pub form: u8,
    /// Global bit 2 of `DAT_0035d268`: commands locked (Flags bit 5 ignores it).
    pub commands_locked: bool,
    /// The battle flag (FUN_001903d8); Flags bit 16 needs it.
    pub battle: bool,
    /// FUN_0019aa28 for the player; Flags bit 1 makes the command unavailable while it holds. Its
    /// meaning was not found, so the app passes false.
    pub player_blocked: bool,
    /// Horizontal distance from the offering object to the player, game units.
    pub distance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Available,
    /// Flags, lock or form mask (FUN_001b70b8, FUN_001b7290).
    Blocked,
    /// Farther than `DistRange` (FUN_0019c3c8).
    TooFar,
    /// The Execute value belongs to a command the availability switch handles with state the app has
    /// no source for.
    NotPorted,
}

/// The fields of a `cmd` row the offer reads.
#[derive(Clone, Debug)]
pub struct CmdRow {
    pub id: u16,
    pub execute: u16,
    pub flags: u32,
    pub disable_form: u16,
    pub dist_range: u16,
    /// Offer cone: the player must face the target within this dot product (FUN_0019cb70; -1 is off).
    pub range: f32,
    pub priority: u8,
    pub message_id: i32,
    pub cmd_action: u8,
    pub reaction_count: u16,
}

impl From<&t::Cmd> for CmdRow {
    fn from(c: &t::Cmd) -> CmdRow {
        CmdRow {
            id: c.id,
            execute: c.execute,
            flags: c.flags,
            disable_form: c.disable_form,
            dist_range: c.dist_range,
            range: c.range,
            priority: c.priority,
            message_id: c.message_id,
            cmd_action: c.cmd_action,
            reaction_count: c.reaction_count,
        }
    }
}

/// The command row a `DisableForm` check reads: the row itself, or the row named by `Execute` when it
/// differs from the id (FUN_001b7290).
fn disable_mask(cmds: &[CmdRow], c: &CmdRow) -> u16 {
    let mut c = c;
    for _ in 0..4 {
        if c.id == c.execute {
            break;
        }
        match cmds.iter().find(|r| r.id == c.execute) {
            Some(n) => c = n,
            None => break,
        }
    }
    c.disable_form
}

pub fn status(cmds: &[CmdRow], c: &CmdRow, ctx: &Ctx) -> Status {
    if EXECUTE_SWITCH.contains(&c.execute) {
        return Status::NotPorted;
    }
    // FUN_001b70b8
    let blocked = (c.flags >> 1 & 1 != 0 && ctx.player_blocked)
        || (c.flags >> 16 & 1 != 0 && !ctx.battle)
        || (c.flags >> 5 & 1 == 0 && ctx.commands_locked);
    if blocked || disable_mask(cmds, c) >> (ctx.form & 0x1f) & 1 != 0 {
        return Status::Blocked;
    }
    if c.dist_range != 0 && ctx.distance > c.dist_range as f32 {
        return Status::TooFar;
    }
    Status::Available
}

/// The commands and message ids the offer needs, read once.
#[derive(Resource, Default)]
pub struct ReactionRules {
    pub cmds: Vec<CmdRow>,
}

impl ReactionRules {
    pub fn cmd(&self, id: u16) -> Option<&CmdRow> {
        self.cmds.iter().find(|c| c.id == id)
    }
}

/// The command currently offered in slot 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Active {
    pub cmd: u16,
    pub pos: i16,
    pub from: Entity,
    pub status: Status,
    /// The command's message id, for the HUD's reaction capsule (`CommandMenu.reaction`).
    pub message: u32,
}

#[derive(Resource, Default)]
pub struct ReactionOffer(pub Option<Active>);

/// Saved per-command use counters, indexed by `ReactionCount` (capped at 999).
#[derive(Resource, Default)]
pub struct ReactionUses(pub HashMap<u16, u16>);

/// Sent when the player accepts an offer. `enemy_ai.rs` delivers it to the target's script as event
/// 0xb (event 11) with the arguments (player, `cmd`, target record {target object, `pos`, 0, -1, 0}).
#[derive(Message, Clone, Copy, Debug)]
pub struct ReactionAccepted {
    pub cmd: u16,
    pub pos: i16,
    pub target: Entity,
}

/// What a target's script asks of the *player* while a reaction command runs: the host calls the
/// Soldier's command-34 thread makes on the player's context (docs/limits-reactions.md). Written by
/// `enemy_ai.rs`; the consumer is the player side (`sora.rs`), which is a hook: nothing in the app
/// reads these yet, so the player neither plays the paired motion nor moves.
#[allow(dead_code)]
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum PlayerReactionCmd {
    /// Host 1.29 (FUN_0017f160): play entry `slot` of `from`'s motion set on the player's body with
    /// `blend` frames (the entry holds one sub-motion per player model, named `%03d0` of the player's
    /// objentry byte +7). `serial` is the value the player's +0x158 takes; the script compares it with
    /// `obj_word_158` to see whether its motion is still the one running.
    /// `ticks` is the sub-motion's length in 60 Hz ticks (0 when the entry has none for the player's model).
    PairedMotion { from: Entity, slot: usize, blend: f32, serial: i32, ticks: f32 },
    /// Host 1.70 / 1.71 with bit 3 on the player: cannot be hit (FUN_0019a4f0 tests it).
    Unhittable(bool),
    /// Host 1.89: the player's secondary velocity vector (+0x880): unit direction in the app's space
    /// and speed in centimetres per tick. Set again every tick while the script wants it.
    Velocity { dir: Vec3, speed: f32 },
    /// Host 1.13: queue motion `slot` of the player's own set behind the running one (FUN_0017e170).
    QueueMotion { slot: usize, blend: f32 },
    /// Commands 455 and 457 are the player's own (FUN_001ce518, not the generic accept path): the
    /// player's timer at +0xb68 is set (120 ticks and motion 0xc1 for 455, 30 ticks and no motion for
    /// 457); while it runs FUN_001ce240 adds one unit to the drive gauge (FUN_001a0290) each time its
    /// second timer (+0xb6c) runs out with the motion near its end, and at zero asks for motion 5. The
    /// consumer (`drive.rs`, `sora.rs`) is a hook.
    Charge { ticks: f32, motion: Option<u16> },
}

/// The player's part of a running reaction command, as the scripts see it.
#[derive(Resource, Default, Debug)]
pub struct PlayerReaction {
    /// The player's +0x158: the serial of the motion `play_motion_with` last started on him (the
    /// counter at 0x35ccb0 counts down from 0 and never returns above -1).
    pub serial: i32,
    /// Set by the player side when the paired motion has ended (bit 1 of the player's +0x160, read by
    /// host 1.14). Nothing sets it yet.
    pub paired_done: bool,
    /// Ticks the paired motion still has to run before `paired_done` is set (`enemy_ai.rs` counts it).
    pub remaining: Option<f32>,
}

/// The accept key was pressed (Triangle in the game).
#[derive(Resource, Default)]
struct AcceptPress(bool);

#[derive(Resource)]
struct Log(bool);

/// `KH2_REACTION_FAKE_ON=<enemy name>` limits the harness offer to that kind (Soldier is the one whose
/// script answers command 34).
fn fake_on() -> Option<String> {
    std::env::var("KH2_REACTION_FAKE_ON").ok().filter(|v| !v.is_empty())
}

/// `KH2_REACTION_FAKE=<command id>`: a harness offer from every living enemy.
#[derive(Resource)]
struct FakeOffer(Option<u16>);

/// The collision tables of the enemies and the player's body volume (docs/limits-reactions.md, the
/// native source of reaction commands).
#[derive(Resource, Default)]
struct Parts {
    by_kind: HashMap<&'static str, Table>,
    player: Option<parts::Rec>,
}

pub struct ReactionPlugin;

impl Plugin for ReactionPlugin {
    fn build(&self, app: &mut App) {
        let cmds = app.world().get_resource::<Disc>().and_then(|d| {
            let mut a = d.0.lock().ok()?;
            kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok().map(|t| t.rows.iter().map(CmdRow::from).collect::<Vec<_>>())
        });
        let Some(cmds) = cmds else {
            warn!("reaction: command table not readable; reaction commands are off");
            return;
        };
        let log = std::env::var("KH2_REACTIONLOG").is_ok_and(|v| v != "0");
        let mut tables = Parts::default();
        if let Some(d) = app.world().get_resource::<Disc>() {
            for (kind, model) in [("Shadow", "M_EX020"), ("Soldier", "M_EX010")] {
                if let Some(t) = d.read(&format!("obj/{model}.mdlx")).and_then(|f| Table::from_mdlx(&f)) {
                    tables.by_kind.insert(kind, t);
                }
            }
            tables.player = d.read("obj/P_EX100.mdlx").and_then(|f| Table::from_mdlx(&f)).and_then(|t| t.first(parts::TYPE_BODY).copied());
        }
        app.insert_resource(ReactionRules { cmds })
            .insert_resource(tables)
            .insert_resource(Log(log))
            .insert_resource(FakeOffer(std::env::var("KH2_REACTION_FAKE").ok().and_then(|v| v.trim().parse().ok())))
            .init_resource::<ReactionOffer>()
            .init_resource::<ReactionUses>()
            .init_resource::<AcceptPress>()
            .init_resource::<PlayerReaction>()
            .add_message::<ReactionAccepted>()
            .add_message::<PlayerReactionCmd>()
            .add_systems(Update, read_key)
            .add_systems(FixedUpdate, (offer, accept).chain().in_set(Sim::Effects));
    }
}

/// R stands in for Triangle.
/// `KH2_REACTION_PRESS=<s>[,<s>...]` presses it at those times (the key harness has no R).
fn read_key(keys: Res<ButtonInput<KeyCode>>, time: Res<Time>, mut press: ResMut<AcceptPress>, mut scripted: Local<Option<Vec<f32>>>) {
    let at = scripted.get_or_insert_with(|| {
        std::env::var("KH2_REACTION_PRESS").map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect()).unwrap_or_default()
    });
    let (now, dt) = (time.elapsed_secs(), time.delta_secs());
    if keys.just_pressed(KeyCode::KeyR) || at.iter().any(|&t| now >= t && now - dt < t) {
        press.0 = true;
    }
}

/// Slot 1: cleared and refilled every tick from the triggers of the objects that are playing a
/// motion with range trigger 20 (a later offer of equal priority replaces an earlier one).
#[allow(clippy::too_many_arguments)]
fn offer(
    rules: Res<ReactionRules>,
    log: Res<Log>,
    fake: Res<FakeOffer>,
    tables: Res<Parts>,
    mut slot: ResMut<ReactionOffer>,
    sora: Query<(&Sora, &Transform)>,
    enemies: Query<(Entity, &Enemy, &Transform)>,
    anims: Query<&Animator>,
) {
    let before = slot.0;
    slot.0 = None;
    let Ok((s, st)) = sora.single() else { return };
    // FUN_00188ad0's precondition on the player (FUN_0019a3f8): in the object list and not in the
    // two states FUN_0019a440 tests (bits not decoded here; downed or dead is the reading).
    if s.hp <= 0 || matches!(s.state, State::Down | State::Viewer) {
        return;
    }
    // The native candidates compete by the row's priority and then by distance (FUN_0019c520): a
    // candidate replaces the best one when its priority is higher, or equal and it is nearer.
    let mut best: Option<(u8, f32)> = None;
    let s_yaw = st.rotation.to_euler(EulerRot::YXZ).0;
    for (e, en, tr) in &enemies {
        let Ok(a) = anims.get(en.body) else { continue };
        if let (Some(table), Some(body)) = (tables.by_kind.get(en.kind.name), tables.player.as_ref())
            && en.alive()
        {
            native_offers(&rules, table, body, a, (e, tr), (st.translation / SCALE, s_yaw), &mut best, &mut slot.0);
        }
        let mut found = a.triggers().map(|tg| offers_at(tg, a.gframe())).unwrap_or_default();
        // Harness: every living enemy offers this command while it plays any motion (no enemy on the
        // disc that the app can spawn offers one; this exercises the offer and accept path).
        if let Some(c) = fake.0.filter(|_| en.alive() && fake_on().is_none_or(|k| k == en.name)) {
            found.push(Offer { cmd: c, pos: 0 });
        }
        for o in found {
            let Some(c) = rules.cmd(o.cmd) else { continue };
            let d = (tr.translation - st.translation).with_y(0.0).length() / SCALE;
            let ctx = Ctx { form: 0, distance: d, ..Default::default() };
            let status = status(&rules.cmds, c, &ctx);
            slot.0 = Some(Active { cmd: o.cmd, pos: o.pos, from: e, status, message: c.message_id as u32 });
        }
    }
    if log.0 && slot.0.map(|a| (a.cmd, a.from, a.status)) != before.map(|a| (a.cmd, a.from, a.status)) {
        match slot.0 {
            Some(a) => info!("reaction: offered command {} (message {}) by {:?}, status {:?}", a.cmd, a.message, a.from, a.status),
            None => info!("reaction: offer withdrawn"),
        }
    }
}

/// Commands the player's own handler (FUN_001ce518) runs itself: id, timer ticks (the floats at 0x37c340 and
/// 0x37c344 are 120.0 and 30.0) and motion. 455 (0x1c7) plays motion 0xc1, 457 (0x1c9) none. Not ported:
/// 456 (0x1c8, motion 0xc0 and FUN_001ce680) and 452 (0x1c4), which this disc offers to nobody here.
fn own_command(cmd: u16) -> Option<(f32, Option<u16>)> {
    match cmd {
        455 => Some((120.0, Some(0xc1))),
        457 => Some((30.0, None)),
        _ => None,
    }
}

/// The native offer of one enemy (FUN_0019cb70): every enabled reaction part (`reaction_parts`) whose
/// volume overlaps the player's body volume offers its command. The row's Flags decide first
/// (FUN_0019cf58: bit 4 lets it show in battle, taken as on in the app; bit 6 needs the trigger window
/// of the player's own motion, id 25 or 26, which the app does not read: those rows are never offered
/// [GAP]); the Range column is the cone test of FUN_0019c868 (the player must face the enemy within that
/// dot product; -1 is off). The candidate replaces the best so far when its priority is higher, or equal
/// and it is nearer (FUN_0019c520); the lock-on distance bonus of FUN_0019c868 is not applied [GAP].
#[allow(clippy::too_many_arguments)]
fn native_offers(
    rules: &ReactionRules,
    table: &Table,
    body: &parts::Rec,
    anim: &Animator,
    (e, tr): (Entity, &Transform),
    (player_pos, player_yaw): (Vec3, f32),
    best: &mut Option<(u8, f32)>,
    slot: &mut Option<Active>,
) {
    let (mut on, mut off) = (Vec::new(), Vec::new());
    if let Some(tg) = anim.triggers() {
        // Range triggers 4 and 5 enable and disable collision parts while they cover the frame.
        for r in tg.active(anim.gframe()) {
            match (r.id, r.params.first()) {
                (4, Some(&p)) => on.push(p),
                (5, Some(&p)) => off.push(p),
                _ => {}
            }
        }
    }
    let Some(player) = parts::shape_of(body, player_pos, player_yaw) else { return };
    let (epos, eyaw) = (tr.translation / SCALE, tr.rotation.to_euler(EulerRot::YXZ).0);
    for r in table.reaction_parts() {
        if !parts::part_usable(table, r.part, &on, &off) || r.id == 0x83 {
            continue;
        }
        let Some(c) = rules.cmd(r.id) else { continue };
        if c.flags >> 6 & 1 != 0 {
            continue;
        }
        let Some(shape) = parts::shape_of(r, epos, eyaw) else { continue };
        if parts::overlaps(&shape, &player) != Some(true) {
            continue;
        }
        let dist = (shape.center - player_pos).length();
        if best.is_some_and(|(p, d)| c.priority < p || (c.priority == p && d <= dist)) {
            continue;
        }
        let mut st = status(&rules.cmds, c, &Ctx { distance: (epos - player_pos).with_y(0.0).length(), ..Default::default() });
        if st == Status::Available && c.range > -1.0 {
            let face = Vec3::new(player_yaw.sin(), 0.0, player_yaw.cos());
            if (epos - player_pos).normalize_or_zero().dot(face) < c.range {
                st = Status::TooFar;
            }
        }
        *best = Some((c.priority, dist));
        *slot = Some(Active { cmd: r.id, pos: r.part as i16, from: e, status: st, message: c.message_id as u32 });
    }
}

/// The accept key: only an available offer runs (FUN_0019c670 pushes slot 1 when its status is 0).
fn accept(
    rules: Res<ReactionRules>,
    log: Res<Log>,
    offer: Res<ReactionOffer>,
    mut press: ResMut<AcceptPress>,
    mut uses: ResMut<ReactionUses>,
    mut out: MessageWriter<ReactionAccepted>,
    mut sora: Query<&mut Sora>,
    targets: Query<&Enemy>,
    mut player_cmds: MessageWriter<PlayerReactionCmd>,
) {
    if !std::mem::take(&mut press.0) {
        return;
    }
    let Some(a) = offer.0 else {
        if log.0 {
            info!("reaction: accept pressed, nothing offered");
        }
        return;
    };
    if a.status != Status::Available {
        if log.0 {
            info!("reaction: accept pressed for command {}, refused ({:?})", a.cmd, a.status);
        }
        return;
    }
    let Some(c) = rules.cmd(a.cmd) else { return };
    // FUN_00177c20: the use counter first (it is counted even when nothing follows).
    if c.reaction_count != 0 && own_command(a.cmd).is_none() {
        let n = uses.0.entry(c.reaction_count).or_insert(0);
        *n = (*n + 1).min(999);
    }
    // FUN_001b80e8: the target must still be in the object list.
    if own_command(a.cmd).is_none() && targets.get(a.from).is_err() {
        if log.0 {
            info!("reaction: accept of command {} dropped, the target {:?} is gone", a.cmd, a.from);
        }
        return;
    }
    // FUN_00176a70 sets the player's bytes +0xb04 and +0xb05 to 1 (the combo step counters) [not
    // ported: they live in sora.rs]. CmdAction 1 stops the mover when it is moving, 2 clears the
    // velocity at +0x20 and gives the mover a zero vector; both clear the vector at +0xad0.
    if let Ok(mut s) = sora.single_mut() {
        match c.cmd_action {
            1 | 2 => s.motor.vel = Vec3::ZERO,
            _ => {}
        }
    }
    // The player's own handler FUN_001ce518 takes 455 (0x1c7) and 457 (0x1c9) before the generic path,
    // so they send no event and bump no use counter (that is inside FUN_00177c20).
    if let Some((ticks, motion)) = own_command(a.cmd) {
        player_cmds.write(PlayerReactionCmd::Charge { ticks, motion });
        if log.0 {
            info!("reaction: accepted command {} (player's own handler): timer {ticks} ticks, motion {motion:?}; the gauge charge and the motion are the player side's [hook]", a.cmd);
        }
        return;
    }
    out.write(ReactionAccepted { cmd: a.cmd, pos: a.pos, target: a.from });
    if log.0 {
        info!(
            "reaction: accepted command {} (Execute {}, CmdAction {}, counter slot {} = {}) from {:?}; delivering script event 0xb (player, command id, target record) to the target's script",
            a.cmd,
            c.execute,
            c.cmd_action,
            c.reaction_count,
            uses.0.get(&c.reaction_count).copied().unwrap_or(0),
            a.from
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kh2_anim::triggers::{range, RangeTrigger};

    fn trig(ranges: Vec<RangeTrigger>) -> Triggers {
        Triggers { ranges, frames: vec![] }
    }

    #[test]
    fn offer_reads_command_and_position() {
        let t = trig(vec![
            RangeTrigger { start: 10, end: 30, id: 20, params: vec![0x1c8, 3] },
            RangeTrigger { start: 10, end: 30, id: 20, params: vec![5] },
            RangeTrigger { start: 0, end: 99, id: range::ATTACK, params: vec![1] },
        ]);
        let o = offers_at(&t, 12.0);
        assert_eq!(o, vec![Offer { cmd: 0x1c8, pos: 3 }, Offer { cmd: 5, pos: 0 }]);
        assert!(offers_at(&t, 31.0).is_empty());
    }

    fn mk(flags: u32, disable: u16, dist: u16) -> CmdRow {
        CmdRow { id: 900, execute: 900, flags, disable_form: disable, dist_range: dist, range: -1.0, priority: 100, message_id: 0, cmd_action: 0, reaction_count: 0 }
    }

    #[test]
    fn status_follows_flags_form_and_distance() {
        let c = mk(0, 0, 100);
        assert_eq!(status(&[], &c, &Ctx { distance: 50.0, ..Default::default() }), Status::Available);
        assert_eq!(status(&[], &c, &Ctx { distance: 150.0, ..Default::default() }), Status::TooFar);
        assert_eq!(status(&[], &c, &Ctx { commands_locked: true, ..Default::default() }), Status::Blocked);
        // Flags bit 5 ignores the lock; bit 16 needs the battle flag.
        assert_eq!(status(&[], &mk(1 << 5, 0, 0), &Ctx { commands_locked: true, ..Default::default() }), Status::Available);
        assert_eq!(status(&[], &mk(1 << 16, 0, 0), &Ctx::default()), Status::Blocked);
        // DisableForm bit n hides the command in form n.
        assert_eq!(status(&[], &mk(0, 1 << 3, 0), &Ctx { form: 3, ..Default::default() }), Status::Blocked);
        assert_eq!(status(&[], &mk(0, 1 << 3, 0), &Ctx { form: 2, ..Default::default() }), Status::Available);
        // An Execute the player's switch handles is reported, not guessed.
        let mut sw = mk(0, 0, 0);
        sw.execute = 0x13c;
        assert_eq!(status(&[], &sw, &Ctx::default()), Status::NotPorted);
    }

    #[test]
    fn disable_mask_chases_execute() {
        let base = CmdRow { id: 4, execute: 4, disable_form: 0b111110, ..mk(0, 0, 0) };
        let drive = CmdRow { id: 9, execute: 4, disable_form: 0, ..mk(0, 0, 0) };
        assert_eq!(disable_mask(&[base, drive.clone()], &drive), 0b111110);
    }

    fn disc() -> Option<Disc> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        if iso.is_none() {
            eprintln!("skipped: no disc (set KH2_ISO)");
        }
        iso.map(|p| Disc::open(&p))
    }

    /// Every motion set on the disc: print the reaction offers (skips without an ISO). Run with
    /// `--nocapture` to get the table that docs/limits-reactions.md summarises.
    #[test]
    fn real_disc_scan() {
        let Some(d) = disc() else { return };
        // The file name list (local/kh2idx.txt, from `kh2 fetch-names`) is needed to list the motion sets.
        let list = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/kh2idx.txt")).unwrap_or_default();
        if list.is_empty() {
            eprintln!("skipped: no name list (kh2 fetch-names)");
            return;
        }
        let names: Vec<String> = {
            let mut a = d.0.lock().unwrap();
            a.apply_names(list.lines()).expect("names");
            a.entries().iter().filter_map(|l| l.name.clone()).filter(|n| n.ends_with(".mset")).collect()
        };
        let mut total = 0;
        for n in &names {
            let Some(raw) = d.read(n) else { continue };
            let Some(m) = kh2_anim::Mset::parse(&raw) else { continue };
            for (slot, tag, o, a, b) in scan_mset(&m) {
                total += 1;
                eprintln!("OFFER {n} slot {slot} {tag} cmd {} pos {} frames {a}..{b}", o.cmd, o.pos);
            }
        }
        eprintln!("motion sets {} offers {total}", names.len());
        assert!(!names.is_empty());
    }

    /// The command table's reaction rows: the 124 rows with CmdAction 1 or 2 whose Execute is their own
    /// id, plus the two rows of the motion offer (455 and 457, CmdAction 0), 126 in all. Their facts
    /// (docs/limits-reactions.md): all can show in battle (Flags bit 4), 12 wait for the player's trigger
    /// window (bit 6), two have an offer cone, three a distance limit, and with the neutral context
    /// every one is available or blocked by its flags, none is "not ported".
    #[test]
    fn real_reaction_rows() {
        let Some(d) = disc() else { return };
        let rows: Vec<t::Cmd> = {
            let mut a = d.0.lock().unwrap();
            kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).expect("cmd").rows
        };
        let cmds: Vec<CmdRow> = rows.iter().map(CmdRow::from).collect();
        let react: Vec<&t::Cmd> = rows.iter().filter(|c| matches!(c.cmd_action, 1 | 2) && c.execute == c.id).collect();
        assert_eq!(react.len(), 124);
        let extra: Vec<&t::Cmd> = rows.iter().filter(|c| matches!(c.id, 455 | 457)).collect();
        assert_eq!(extra.len(), 2);
        assert!(react.iter().all(|c| c.flags >> 4 & 1 != 0), "every reaction row shows in battle");
        assert!(react.iter().all(|c| c.dir == 0.0 && c.dir_range == -1.0));
        assert_eq!(react.iter().filter(|c| c.flags >> 6 & 1 != 0).count(), 12, "rows that wait for the trigger window");
        assert_eq!(react.iter().filter(|c| c.range > -1.0).map(|c| c.range).collect::<Vec<_>>(), vec![0.5, 0.5]);
        assert_eq!(react.iter().filter(|c| c.dist_range != 0).map(|c| (c.id, c.dist_range)).collect::<Vec<_>>(), vec![(466, 1000), (516, 500), (517, 1000)]);
        assert_eq!(react.iter().filter(|c| c.reaction_count != 0).count(), 43);
        // Command 34 (the Soldier's) and 457 (Mickey's motion offer).
        let c34 = react.iter().find(|c| c.id == 34).expect("34");
        assert_eq!((c34.mark, c34.cmd_action, c34.reaction_count, c34.priority, c34.message_id), (2, 1, 1, 100, 33949));
        let c457 = extra.iter().find(|c| c.id == 457).expect("457");
        assert_eq!((c457.cmd_action, c457.mark, c457.priority, c457.require, c457.message_id), (0, 6, 255, 455, 48047));
        for c in react.iter().copied().chain(extra.iter().copied()) {
            let row = CmdRow::from(c);
            let st = status(&cmds, &row, &Ctx::default());
            assert!(matches!(st, Status::Available | Status::Blocked | Status::TooFar), "command {} is {st:?}", c.id);
        }
    }

    /// The Shadow and Soldier sets (the app's two enemies) make no reaction offer.
    #[test]
    fn app_enemies_offer_nothing() {
        let Some(d) = disc() else { return };
        for n in ["obj/M_EX020.mset", "obj/M_EX010.mset"] {
            let m = kh2_anim::Mset::parse(&d.read(n).expect("mset")).expect("parse");
            assert!(scan_mset(&m).is_empty(), "{n}");
        }
    }
}
