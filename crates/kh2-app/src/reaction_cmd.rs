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
//!   FUN_00177b18 -> FUN_00177c20): bump the use counter `ReactionCount`, apply `CmdAction`
//!   (1 stop the player if moving, 2 clear velocity), then deliver the command to the offering
//!   object as script event 0xb with the arguments (player, command id, target record)
//!   (FUN_0016b420 -> FUN_0016c620). What the object's script does with it is the target side.
//!
//! The target side (the BDX event 0xb handler and the paired player motions) is a hook: this module
//! writes `ReactionAccepted` and nothing else.

use std::collections::HashMap;

use bevy::prelude::*;
use kh2_anim::Triggers;
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::enemy::Enemy;
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

/// Sent when the player accepts an offer. Hook for the offering object's script (event 0xb with the
/// arguments player, `cmd`, target record).
#[allow(dead_code)]
#[derive(Message, Clone, Copy, Debug)]
pub struct ReactionAccepted {
    pub cmd: u16,
    pub pos: i16,
    pub target: Entity,
}

/// The accept key was pressed (Triangle in the game).
#[derive(Resource, Default)]
struct AcceptPress(bool);

#[derive(Resource)]
struct Log(bool);

/// `KH2_REACTION_FAKE=<command id>`: a harness offer from every living enemy.
#[derive(Resource)]
struct FakeOffer(Option<u16>);

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
        app.insert_resource(ReactionRules { cmds })
            .insert_resource(Log(log))
            .insert_resource(FakeOffer(std::env::var("KH2_REACTION_FAKE").ok().and_then(|v| v.trim().parse().ok())))
            .init_resource::<ReactionOffer>()
            .init_resource::<ReactionUses>()
            .init_resource::<AcceptPress>()
            .add_message::<ReactionAccepted>()
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
    mut slot: ResMut<ReactionOffer>,
    sora: Query<(&Sora, &Transform)>,
    enemies: Query<(Entity, &Enemy, &Transform)>,
    anims: Query<&Animator>,
    drive: Option<Res<crate::drive::DriveState>>,
    battle: Res<crate::sora::Battle>,
) {
    let before = slot.0;
    // The player object's form field (+0xb50, read by FUN_001b7290 for DisableForm) and the
    // battle flag (FUN_001903d8, Flags bit 16).
    let form = drive.as_deref().map_or(0, |d| d.object_form());
    let battle = battle.active() || drive.as_deref().is_some_and(|d| d.force_battle);
    slot.0 = None;
    let Ok((s, st)) = sora.single() else { return };
    // FUN_00188ad0's precondition on the player (FUN_0019a3f8): in the object list and not in the
    // two states FUN_0019a440 tests (bits not decoded here; downed or dead is the reading).
    if s.hp <= 0 || matches!(s.state, State::Down | State::Viewer) {
        return;
    }
    for (e, en, tr) in &enemies {
        let Ok(a) = anims.get(en.body) else { continue };
        let mut found = a.triggers().map(|tg| offers_at(tg, a.gframe())).unwrap_or_default();
        // Harness: every living enemy offers this command while it plays any motion (no enemy on the
        // disc that the app can spawn offers one; this exercises the offer and accept path).
        if let Some(c) = fake.0.filter(|_| en.alive()) {
            found.push(Offer { cmd: c, pos: 0 });
        }
        for o in found {
            let Some(c) = rules.cmd(o.cmd) else { continue };
            let d = (tr.translation - st.translation).with_y(0.0).length() / SCALE;
            let ctx = Ctx { form, battle, distance: d, ..Default::default() };
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

/// The accept key: only an available offer runs (FUN_0019c670 pushes slot 1 when its status is 0).
fn accept(
    rules: Res<ReactionRules>,
    log: Res<Log>,
    offer: Res<ReactionOffer>,
    mut press: ResMut<AcceptPress>,
    mut uses: ResMut<ReactionUses>,
    mut out: MessageWriter<ReactionAccepted>,
    mut sora: Query<&mut Sora>,
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
    // FUN_00177c20: the use counter, then the player's own handling by CmdAction.
    if c.reaction_count != 0 {
        let n = uses.0.entry(c.reaction_count).or_insert(0);
        *n = (*n + 1).min(999);
    }
    if let Ok(mut s) = sora.single_mut() {
        match c.cmd_action {
            1 | 2 => s.motor.vel = Vec3::ZERO,
            _ => {}
        }
    }
    out.write(ReactionAccepted { cmd: a.cmd, pos: a.pos, target: a.from });
    if log.0 {
        info!(
            "reaction: accepted command {} (Execute {}, CmdAction {}, counter slot {} = {}) from {:?}; delivered as script event 0xb to the offering object (target side not ported)",
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
        CmdRow { id: 900, execute: 900, flags, disable_form: disable, dist_range: dist, message_id: 0, cmd_action: 0, reaction_count: 0 }
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
