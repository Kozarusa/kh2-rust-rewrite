//! The player's side of reaction commands and clashes: what a target's script asks of Sora while a
//! reaction command runs (`reaction_cmd::PlayerReactionCmd`), and what an attack-against-attack
//! clash does to him (`clash::Clash`). See docs/limits-reactions.md and docs/hit-reaction.md.
//!
//! Game rules ported (addresses in `SLUS_210.05`):
//! - host 1.29 `play_motion_with` (FUN_0017f160): the player's motion queue is cleared (FUN_0017e118)
//!   and sub-motion `%03d0` of his objentry byte +7 inside entry `slot` of the other object's motion
//!   set is played on his own controller (FUN_0017ef98, FUN_0017ee40). The app plays it through
//!   `Animator::play_external`; Sora takes no input while it runs (`State::Reaction`).
//! - host 1.14 `motion_done`: bit 1 of the player's +0x160, the current motion has ended. Set from
//!   the real end of the animator's motion (`PlayerReaction::paired_done`).
//! - host 1.70 / 1.71 on bit 3 of the player's flags: FUN_0019a4f0 refuses hits while it is set.
//! - host 1.89 `set_velocity` (FUN_00183418): the secondary velocity at +0x880 (unit direction, speed
//!   in +0x88c). The mover FUN_00181e00 adds direction * speed * the frame step to the tick's
//!   displacement and clears the whole vector at its end, so it moves the player for the one tick
//!   it was set.
//! - host 1.13 `queue_motion` (FUN_0017e170): a motion id appended to the controller's 4-entry
//!   queue; it plays when the running motion ends.
//! - the party clash handler FUN_001754a0 (vtable +0x6c through FUN_00178708): see [`party_clash`].
//!
//! Timing: the scripts run in `enemy_ai::ai_update` after Sora's tick, so what a script asks in
//! one tick reaches Sora at the start of the next (one tick later than a script that ran before the
//! player in the game would; the game's order between the two was not read).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use kh2_anim::{PlayerState, Slot, Triggers};
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::clash::Clash;
use crate::combat::{live_attacks, Fighter, ReactionData};
use crate::data::GameData;
use crate::enemy::Enemy;
use crate::fx::StartSystemEffect;
use crate::reaction_cmd::{PlayerReaction, PlayerReactionCmd};
use crate::reaction_rules::{self as rules, ClashActor, ClashReaction, ExeConsts, Push, ReflectFields, RECOIL_BLEND};
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::{Battle, Sora, State, WeaponCue};
use crate::SCALE;

/// The enemies whose motion sets the app reads for paired motions (the kinds the app spawns).
const KINDS: [(&str, &str); 2] = [("Shadow", "M_EX020"), ("Soldier", "M_EX010")];

/// Ticks Sora holds the last frame of a paired motion that ended with nothing queued, waiting for
/// the script's next request (it arrives one tick later, see the module notes), before control
/// returns. The app's: the game's motion controller with an empty queue was not read.
const RELEASE_AFTER: u32 = 2;

/// One paired sub-motion: the sub-entry's name and the motion with its triggers.
pub type Paired = (String, Arc<Slot>);

/// The paired player motions of a motion set: the entries of BAR type 20, each a BAR of sub-motions
/// named `%03d0` of a player model byte (FUN_0017ee40), each sub-motion an animation BAR like a slot
/// of the set (motion type 9, triggers type 16). Empty sub-entries are left out.
pub fn paired_entries(mset: &[u8]) -> HashMap<usize, Vec<Paired>> {
    let mut out = HashMap::new();
    let Ok(entries) = kh2_bar::parse(mset) else { return out };
    for (slot, e) in entries.iter().enumerate().filter(|(_, e)| e.kind == 20) {
        let Ok(subs) = kh2_bar::parse(e.data) else { continue };
        let list: Vec<Paired> = subs
            .iter()
            .filter(|s| !s.data.is_empty())
            .filter_map(|s| {
                let anb = kh2_bar::parse(s.data).ok()?;
                let motion = anb.iter().find(|x| x.kind == 9).and_then(|x| kh2_anim::Motion::parse(x.data))?;
                let triggers = anb.iter().find(|x| x.kind == 16).map(|x| Triggers::parse(x.data)).unwrap_or_default();
                Some((s.tag.to_string(), Arc::new(Slot { tag: s.tag.to_string(), motion: Some(motion), triggers })))
            })
            .collect();
        out.insert(slot, list);
    }
    out
}

/// The sub-motion name for a player model byte (the format string at 0x37a598 used by FUN_0017ef98).
pub fn sub_name(model: u8) -> String {
    format!("{model:03}0")
}

/// Paired motions by enemy kind, and the player's model byte.
#[derive(Resource, Default)]
struct PairedLib {
    by_kind: HashMap<&'static str, HashMap<usize, Vec<Paired>>>,
    model: u8,
}

impl PairedLib {
    fn find(&self, kind: &str, slot: usize) -> Option<&Paired> {
        let name = sub_name(self.model);
        self.by_kind.get(kind)?.get(&slot)?.iter().find(|(n, _)| *n == name)
    }
}

/// The reaction command as the player side runs it.
#[derive(Resource, Default, Debug)]
struct Run {
    /// A paired motion was started on Sora and control has not returned yet.
    active: bool,
    /// Ticks the current paired motion has run.
    ticks: u32,
    /// The motion id queued behind the paired motion (host 1.13) and its blend.
    queued: Option<(usize, f32)>,
    /// Bit 3 of the player's flags (host 1.70 / 1.71).
    unhittable: bool,
    /// Ticks since the paired motion ended with nothing queued.
    ended: u32,
    /// Something other than the reaction took Sora over (logged once).
    interrupted: bool,
}

/// The two guard-bounce numbers the executable keeps beside the ones in `ExeConsts`.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct GuardConsts {
    /// Scale of the direction FUN_001756c8 turns the actor to (0x37a4a8; -1.0: face the other attack).
    pub face_scale: f32,
    /// Invincibility ticks after a guard bounce (0x37a4b0, written to +0xb00 by FUN_00175aa8).
    pub star_ticks: f32,
}

#[derive(Resource)]
struct Log(bool);

pub struct PlayerReactionPlugin;

impl Plugin for PlayerReactionPlugin {
    fn build(&self, app: &mut App) {
        let mut lib = PairedLib { model: 1, ..Default::default() };
        let mut guard = None;
        if let Some(d) = app.world().get_resource::<Disc>() {
            let rows = d.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|x| x.rows).unwrap_or_default();
            // The player's objentry byte +7 (`WeaponJoint`) names the sub-motion; Sora's row is P_EX100.
            lib.model = rows.iter().find(|r| r.model_name == "P_EX100").map_or(1, |r| r.weapon_joint);
            for (kind, model) in KINDS {
                let paired = d.read(&format!("obj/{model}.mset")).map(|m| paired_entries(&m)).unwrap_or_default();
                if !paired.is_empty() {
                    lib.by_kind.insert(kind, paired);
                }
            }
            guard = d.read_root("SLUS_210.05").and_then(|exe| Some(GuardConsts { face_scale: exe_f32(&exe, 0x37a4a8)?, star_ticks: exe_f32(&exe, 0x37a4b0)? }));
        }
        if guard.is_none() {
            warn!("player: the guard bounce constants are not readable from SLUS_210.05; a guard clash only recoils");
        }
        if let Some(g) = guard {
            app.insert_resource(g);
        }
        let log = std::env::var("KH2_REACTIONLOG").is_ok_and(|v| v != "0");
        app.insert_resource(lib)
            .insert_resource(Log(log))
            .init_resource::<Run>()
            .init_resource::<PlayerReaction>()
            .add_message::<PlayerReactionCmd>()
            .add_message::<Clash>()
            .add_systems(
                FixedUpdate,
                (
                    consume.in_set(Sim::Sora).before(crate::sora::tick),
                    unhittable.in_set(Sim::Sora).after(crate::sora::tick),
                    receive_clashes.in_set(Sim::SoraHurt).after(crate::sora::receive_wall_hits),
                ),
            );
    }
}

/// A float of the executable at a virtual address, through the ELF program headers.
fn exe_f32(elf: &[u8], va: u32) -> Option<f32> {
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
            let o = off as usize + (va - vaddr) as usize;
            return elf.get(o..o + 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        }
    }
    None
}

/// The script's requests of the last tick, then the end of the paired motion. Runs after the pose
/// step and before Sora's controller.
#[allow(clippy::too_many_arguments)]
fn consume(
    mut cmds: MessageReader<PlayerReactionCmd>,
    lib: Res<PairedLib>,
    log: Res<Log>,
    battle: Res<Battle>,
    enemies: Query<&Enemy>,
    mut sora_q: Query<&mut Sora>,
    mut anims: Query<&mut Animator>,
    mut pr: ResMut<PlayerReaction>,
    mut run: ResMut<Run>,
) {
    let Ok(mut s) = sora_q.single_mut() else {
        cmds.clear();
        return;
    };
    let Ok(mut body) = anims.get_mut(s.body) else { return };
    let mut velocity: Option<Vec3> = None;
    for c in cmds.read() {
        match *c {
            PlayerReactionCmd::PairedMotion { from, slot, blend, serial, .. } => {
                let kind = enemies.get(from).map(|e| e.kind.name).unwrap_or("?");
                match lib.find(kind, slot) {
                    Some((name, m)) => {
                        // FUN_0017f160: the queue is cleared, then the sub-motion starts.
                        body.play_external(m.clone(), blend);
                        s.enter(State::Reaction, None);
                        *run = Run { active: true, unhittable: run.unhittable, ..Default::default() };
                        info!(
                            "player: paired motion started: entry {slot} sub-motion {name} of {kind} on Sora ({:.0} ticks, blend {blend}), serial {serial}",
                            ticks_of(m)
                        );
                    }
                    None => {
                        // Nothing to play: the script reads the motion as ended at once (the game's
                        // lookup failure path in FUN_0017ee40 was not read) [inferred].
                        pr.paired_done = true;
                        warn!("player: paired motion entry {slot} of {kind} has no sub-motion {}; reported as ended", sub_name(lib.model));
                    }
                }
            }
            PlayerReactionCmd::Unhittable(on) => {
                if run.unhittable != on {
                    info!("player: unhittable {}", if on { "set" } else { "cleared" });
                }
                run.unhittable = on;
            }
            // set_velocity overwrites the vector: the last request of the tick holds.
            PlayerReactionCmd::Velocity { dir, speed } => velocity = Some(dir * speed),
            PlayerReactionCmd::QueueMotion { slot, blend } => {
                run.queued = Some((slot, blend));
                info!("player: motion {slot} queued behind the paired motion (blend {blend})");
            }
            // The drive-gauge charge of commands 455 and 457 is not this module's.
            PlayerReactionCmd::Charge { .. } => {}
        }
    }

    // The secondary velocity moves Sora for this tick only (FUN_00181e00 clears +0x880).
    if s.state == State::Reaction {
        s.motor.vel = velocity.unwrap_or(Vec3::ZERO);
    }
    if let Some(v) = velocity
        && log.0
    {
        if s.state == State::Reaction {
            info!("player: velocity ({:.2} {:.2} {:.2}) {:.1} cm this tick", v.x, v.y, v.z, v.length());
        } else {
            info!("player: velocity request dropped: Sora is {:?}, not in the paired motion [app gap]", s.state);
        }
    }

    if !run.active {
        return;
    }
    run.ticks += 1;
    // Bit 1 of +0x160: the current motion of the player has ended.
    pr.paired_done = body.finished;
    pr.remaining = body.motion().map(|m| ((m.frame_end - body.frame).max(0.0) * crate::data::LOGIC_HZ / m.fps.max(1.0)).round());
    if s.state != State::Reaction {
        // A hit or another request replaced the paired motion (a motion request clears the queue,
        // FUN_0017e220); the script still waits for the current motion's end, which is what
        // `paired_done` follows. The run is over when that motion ends.
        if !run.interrupted {
            run.interrupted = true;
            run.queued = None;
            info!("player: paired motion interrupted: Sora is {:?} ({})", s.state, body.tag());
        }
        if body.finished {
            run.active = false;
        }
        return;
    }
    if !body.finished {
        return;
    }
    if run.ended == 0 {
        info!("player: paired motion ended after {} ticks: motion_done set", run.ticks);
    }
    let pstate = if battle.active() { PlayerState::BattleWeapon } else { PlayerState::FieldNoWeapon };
    if let Some((id, blend)) = run.queued.take() {
        let slot = body.rig.mset.slot(id, pstate);
        body.restart(slot, true, blend);
        let state = if s.motor.grounded { State::Ground } else { State::Air };
        s.enter(state, None);
        run.active = false;
        info!("player: queued motion {id} ({}) plays, control returns ({state:?})", body.tag());
        return;
    }
    run.ended += 1;
    if run.ended > RELEASE_AFTER {
        let state = if s.motor.grounded { State::Ground } else { State::Air };
        s.enter(state, None);
        if state == State::Air {
            s.motor.start_fall();
        }
        run.active = false;
        info!("player: paired motion ended with nothing queued: control returns ({state:?})");
    }
}

/// Length of a paired sub-motion in 60 Hz ticks.
fn ticks_of(m: &Slot) -> f32 {
    m.motion.as_ref().map_or(0.0, |m| (m.frame_end - m.frame_start).max(0.0) * crate::data::LOGIC_HZ / m.fps.max(1.0))
}

/// Bit 3 of the player's flags: no hit reaches him while it is set (FUN_0019a4f0). Runs after the
/// controller, which sets `hittable` from its own state.
fn unhittable(run: Res<Run>, mut q: Query<&mut Fighter, With<Sora>>) {
    if run.unhittable
        && let Ok(mut f) = q.single_mut()
    {
        f.hittable = false;
    }
}

/// What FUN_001754a0 does to a party actor whose attack `own` met the attack `other`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PartyOutcome {
    /// Only the spark (`RefactSelf` 2 on the own row or `RefactOther` 2 on the other).
    Spark,
    /// FUN_00185d30 with the power at 0x37a4a0 and no motion override: motion 0x29, or 0x2b when
    /// airborne, the push backwards along the facing, and the combo counter +0xb04 down by one
    /// (FUN_00176aa0, not below 0).
    Recoil { motion: u8, power: f32 },
    /// FUN_001756c8: face the other attack, pushed away from it with `power` (0x37a4ac), motion
    /// 0x2a with the queue cleared and motion 0 queued behind it, `star` ticks of invincibility
    /// (+0xb00, FUN_00175aa8: the timer FUN_00175ab0 counts down and FUN_00175ad8 tests).
    GuardBounce { motion: u8, power: f32, star: f32 },
}

/// The party clash handler FUN_001754a0, all of it but the spark itself: the actor's hit-landed
/// latch +0x108 bit 0x2000000 is set first (`connected`), then the outcome by `party_clash`.
/// Returns the outcome and the system effect id of the spark FUN_0018d560 starts (0xd for the spark
/// alone and the recoil; for the guard bounce 0xb when the actor's +0x588 has bit 0x80, which the
/// party class sets (FUN_001746e0, FUN_00176e00), else 0xc).
pub fn party_clash(own: &ReflectFields, other: &ReflectFields, actor: ClashActor, c: &ExeConsts, g: Option<&GuardConsts>) -> (PartyOutcome, u16) {
    match rules::party_clash(own.refact_self, other.refact_other, actor.plain_ground()) {
        ClashReaction::None => (PartyOutcome::Spark, 0xd),
        ClashReaction::Guard if g.is_some() => {
            (PartyOutcome::GuardBounce { motion: rules::motion::AUTOGUARD, power: c.guard_power, star: g.map_or(0.0, |g| g.star_ticks) }, 0xb)
        }
        // Without the guard constants the bounce cannot be built; the recoil stands in (warned at start).
        ClashReaction::Reflect | ClashReaction::Guard => (
            PartyOutcome::Recoil { motion: actor.recoil_motion(), power: rules::recoil_power(c.clash_power, c) },
            0xd,
        ),
    }
}

/// Sora's side of a clash (`clash::Clash`): FUN_001754a0 with his own state.
#[allow(clippy::too_many_arguments)]
fn receive_clashes(
    mut clashes: MessageReader<Clash>,
    data: Res<GameData>,
    reaction: Option<Res<ReactionData>>,
    guard: Option<Res<GuardConsts>>,
    enemies: Query<&Fighter, Without<Sora>>,
    globals: Query<&GlobalTransform>,
    mut sora_q: Query<(Entity, &mut Sora, &mut Fighter)>,
    mut anims: Query<&mut Animator>,
    mut sparks: MessageWriter<StartSystemEffect>,
) {
    let Ok((me, mut s, mut fighter)) = sora_q.single_mut() else { return };
    let Some(reaction) = reaction else { return };
    let c = &reaction.consts;
    for k in clashes.read().filter(|k| k.player == me) {
        let (Some(own), Some(other)) = (data.atkp(k.player_atkp), data.atkp(k.enemy_atkp)) else { continue };
        let (own, other) = (ReflectFields::from_atkp(own), ReflectFields::from_atkp(other));
        // FUN_0019aa28: airborne (the mover's bit 1) or range trigger 3 / 30 of Sora's motion; +0x160
        // bit 0x40: range trigger 24 (docs/hit-reaction.md). The mover's bit 2 reads as clear.
        let ranges: Vec<u8> = anims.get(s.body).ok().and_then(|a| a.triggers().map(|t| t.active(a.gframe()).map(|r| r.id).collect())).unwrap_or_default();
        let actor = ClashActor::new(!s.motor.grounded, ranges);
        let airborne = actor.fun_0019aa28;
        let (outcome, spark) = party_clash(&own, &other, actor, c, guard.as_deref());
        s.connected = true;
        // FUN_0018d560: the spark is element `spark` of the system effect file, at the own volume's
        // centre moved toward the other volume's centre by the own radius. The volume centre is the
        // midpoint of the capsule here (the hit code's own centre is private to combat.rs) [inferred].
        // The game starts it only when neither volume has flag 0x80: `detect_clashes` sends a Clash
        // once per fresh pair, and no effect file of the app's objects has a category 0x10 element,
        // so FUN_0018d6b0 (which would set the flag first) never sparks (test
        // `fx::tests::clash_spark_elements_by_file`); the flag is therefore clear here.
        let volume_of = |bodies: &[Entity], atkp: i16| {
            bodies.iter().find_map(|b| {
                let (a, g) = (anims.get(*b).ok()?, globals.get(*b).ok()?);
                live_attacks(a, g).into_iter().find(|(id, _, _)| *id == atkp).map(|(_, _, v)| v)
            })
        };
        let own_vol = volume_of(&fighter.attack_bodies, k.player_atkp);
        let other_vol = enemies.get(k.enemy).ok().and_then(|f| volume_of(&f.attack_bodies, k.enemy_atkp));
        let spark_note = match (own_vol, other_vol) {
            (Some(o), Some(t)) => {
                let (oc, tc) = ((o.a + o.b) * 0.5, (t.a + t.b) * 0.5);
                let at = oc + (tc - oc).normalize_or_zero() * o.r;
                sparks.write(StartSystemEffect { id: spark, at: Mat4::from_translation(at) });
                format!("started at {at:.3?}")
            }
            _ => "not started: a volume of the pair is no longer live".to_string(),
        };
        let what = match outcome {
            PartyOutcome::Spark => "spark only".to_string(),
            PartyOutcome::Recoil { motion, power } => {
                let Ok(mut body) = anims.get_mut(s.body) else { return };
                let slot = body.rig.mset.slot(motion as usize, PlayerState::BattleWeapon);
                // FUN_00185d30 requests the motion with blend 0 (RECOIL_BLEND).
                body.restart(slot, false, RECOIL_BLEND);
                let mut weapon = WeaponCue::default();
                weapon.restart(slot, false, RECOIL_BLEND);
                weapon.apply(&s.weapons, &mut anims);
                let push = Push::new(-s.motor.forward(), power, 0.0, c);
                let before = s.hits;
                s.hits = s.hits.saturating_sub(1);
                s.enter(State::Hurt, push);
                fighter.landed.clear();
                format!("recoil motion {motion} slot {}, push {power} backwards, combo {before} -> {}", slot.map_or(-1, |x| x as i32), s.hits)
            }
            PartyOutcome::GuardBounce { motion, power, star } => {
                // FUN_0018cc48: from the other attack's volume to the actor (+0x730), flat, unit.
                let other_at = enemies
                    .get(k.enemy)
                    .ok()
                    .and_then(|f| {
                        f.attack_bodies.iter().find_map(|b| {
                            let (a, g) = (anims.get(*b).ok()?, globals.get(*b).ok()?);
                            live_attacks(a, g).into_iter().find(|(atkp, _, _)| *atkp == k.enemy_atkp).map(|(_, _, v)| (v.a + v.b) * 0.5 / SCALE)
                        })
                    })
                    .or_else(|| globals.get(k.enemy).ok().map(|g| g.translation() / SCALE));
                let away = other_at.map_or(-s.motor.forward(), |p| (s.motor.pos - p).with_y(0.0).normalize_or_zero());
                // FUN_0016a828 turns the actor along the direction times 0x37a4a8 (-1: toward the
                // other attack); its FUN_0019ad98 exception was not decoded.
                let face = away * guard.as_ref().map_or(-1.0, |g| g.face_scale);
                if face != Vec3::ZERO {
                    s.motor.yaw = face.x.atan2(face.z);
                }
                let Ok(mut body) = anims.get_mut(s.body) else { return };
                let slot = body.rig.mset.slot(motion as usize, PlayerState::BattleWeapon);
                body.restart(slot, false, RECOIL_BLEND);
                let mut weapon = WeaponCue::default();
                weapon.restart(slot, false, RECOIL_BLEND);
                weapon.apply(&s.weapons, &mut anims);
                s.enter(State::Hurt, Push::new(away, power, 0.0, c));
                s.invuln = star;
                fighter.landed.clear();
                format!("guard bounce motion {motion} slot {}, push {power} away from the other attack, {star} ticks invincible", slot.map_or(-1, |x| x as i32))
            }
        };
        info!(
            "player: clash: own atkp {} (RefactSelf {:?}) against atkp {} (RefactOther {:?}), airborne {airborne}: {what}; system effect {spark:#x} ({spark_note})",
            k.player_atkp, own.refact_self, k.enemy_atkp, other.refact_other
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction_rules::Refact;

    fn fields(own: Refact, other: Refact) -> ReflectFields {
        ReflectFields {
            refact_self: own,
            refact_other: other,
            reflected_motion: 0,
            reflect_hit_back: 0,
            reflect_action: 0,
            reflect_hit_sound: -1,
            reflect_rc: 0,
            reflect_range: 0,
            reflect_angle: 0,
            bg_hit: false,
        }
    }

    /// The executable's values (read from the user's disc in the app; written here for the test).
    fn consts() -> (ExeConsts, GuardConsts) {
        let c = ExeConsts {
            wall_power: -1.0,
            clash_power: -1.0,
            default_power: 50.0,
            guard_power: 200.0,
            push_scale: 0.1,
            push_cutoff: 1.0,
            push_decay: 0.9,
            reduced_scale: 0.5,
            flinch_motions: [10, 11, 12, 13],
            gravity: 20.0 / 49.0,
            clash_pairs: [0; 49],
        };
        (c, GuardConsts { face_scale: -1.0, star_ticks: 80.0 })
    }

    #[test]
    fn party_clash_follows_fun_001754a0() {
        use Refact::*;
        let (c, g) = consts();
        let k = |own, other, air| party_clash(&fields(own, Reflect), &fields(Reflect, other), ClashActor::new(air, []), &c, Some(&g));
        // RefactSelf 2 of the own row, or RefactOther 2 of the other: the spark alone (effect 0xd).
        assert_eq!(k(Nothing, Guard, false), (PartyOutcome::Spark, 0xd));
        assert_eq!(k(Guard, Nothing, true), (PartyOutcome::Spark, 0xd));
        // Both 0 on the ground: the recoil of FUN_00185d30 with the default power 50.
        assert_eq!(k(Reflect, Reflect, false), (PartyOutcome::Recoil { motion: 0x29, power: 50.0 }, 0xd));
        // Airborne always recoils, with motion 0x2b.
        assert_eq!(k(Guard, Guard, true), (PartyOutcome::Recoil { motion: 0x2b, power: 50.0 }, 0xd));
        // A 1 on either side on the ground: the guard bounce, spark 0xb for the party class.
        assert_eq!(k(Reflect, Guard, false), (PartyOutcome::GuardBounce { motion: 0x2a, power: 200.0, star: 80.0 }, 0xb));
        assert_eq!(k(Guard, Reflect, false), (PartyOutcome::GuardBounce { motion: 0x2a, power: 200.0, star: 80.0 }, 0xb));
        // Without the guard constants the recoil stands in.
        assert_eq!(party_clash(&fields(Guard, Reflect), &fields(Reflect, Reflect), ClashActor::default(), &c, None).0, PartyOutcome::Recoil { motion: 0x29, power: 50.0 });
    }

    #[test]
    fn sub_motion_name_is_three_digits_and_zero() {
        assert_eq!(sub_name(1), "0010");
        assert_eq!(sub_name(12), "0120");
    }

    fn disc() -> Option<Disc> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists());
        if iso.is_none() {
            eprintln!("skipped: no disc (set KH2_ISO)");
        }
        iso.map(|p| Disc::open(&p))
    }

    /// The Soldier's entries 252 and 253 (command 34's dodge and its follow-up) hold a sub-motion
    /// for Sora's model byte, a motion of Sora's skeleton (skips without an ISO).
    #[test]
    fn real_soldier_paired_motions() {
        let Some(d) = disc() else { return };
        let paired = paired_entries(&d.read("obj/M_EX010.mset").expect("mset"));
        let sora = kh2_model::mdlx(&d.read("obj/P_EX100.mdlx").expect("mdlx")).expect("model");
        for slot in [252, 253] {
            let list = paired.get(&slot).unwrap_or_else(|| panic!("entry {slot}"));
            let (name, m) = list.iter().find(|(n, _)| n == "0010").unwrap_or_else(|| panic!("entry {slot} has no 0010"));
            let motion = m.motion.as_ref().expect("motion");
            let mut st = Default::default();
            let pose = motion.pose_with(&sora.bones, motion.frame_start, &mut st);
            assert_eq!(pose.len(), sora.bones.len(), "{name}");
            eprintln!("entry {slot} {name}: {:.0} ticks, {} range triggers", ticks_of(m), m.triggers.ranges.len());
        }
        assert_eq!(ticks_of(&paired[&252].iter().find(|(n, _)| n == "0010").unwrap().1), 12.0);
        assert_eq!(ticks_of(&paired[&253].iter().find(|(n, _)| n == "0010").unwrap().1), 70.0);
    }

    /// The executable's guard-bounce numbers and the clash spark ids in the system effect file
    /// `00effect.bar` (skips without an ISO).
    #[test]
    fn real_guard_consts_and_spark_elements() {
        let Some(d) = disc() else { return };
        let exe = d.read_root("SLUS_210.05").expect("exe");
        assert_eq!((exe_f32(&exe, 0x37a4a8), exe_f32(&exe, 0x37a4b0)), (Some(-1.0), Some(80.0)));
        let file = d.read("00effect.bar").expect("00effect.bar");
        let pax = kh2_bar::parse(&file).ok().and_then(|e| e.iter().find(|e| e.kind == 18).and_then(|e| kh2_fx::Pax::parse(e.data).ok())).expect("pax");
        for id in [0xb_u16, 0xc, 0xd] {
            let el = pax.elements_for(id).next().unwrap_or_else(|| panic!("element {id:#x}"));
            eprintln!("00effect.bar element id {id:#x}: effect {}", el.effect);
        }
    }
}
