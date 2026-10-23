//! Attack against attack: an enemy attack volume meets a player attack volume.
//!
//! The game pairs live attack volumes of opposing teams in FUN_0018d808: the pair must be allowed by
//! the 7 by 7 byte table at 0x37ac98 (indexed by the two attack rows' `Type`, read from the user's
//! executable into `ExeConsts::clash_pairs`), neither side may have hit the other's owner already, the
//! shapes must overlap, and both rows must pass the cone test FUN_0018d308 (`ReflectRange`). Then
//! FUN_0018d6b0 spawns a spark and disables both volumes (volume flag 0x80) except one whose own row says
//! `RefactSelf` 2, and FUN_0018d430 calls the owner's clash method (+0x6c) for each side: the enemy-style
//! handler (0x1b2478) plays `ReflectedMotion` with a recoil, the party handler (FUN_001754a0) is the
//! player's side and lives in `sora.rs`. See docs/systems/drive-limits-summons/contract.md and docs/systems/combat-effects/hit-reaction.md.
//!
//! What this module does: detects the pairs the way the app detects hits (capsule overlap of
//! `combat::live_attacks`), disables the volumes by marking their windows as landed so
//! `combat::detect_hits` skips them, writes [`EnemyClash`] for the enemy side and [`Clash`] for the
//! player's side (consumed by `player_reaction.rs`, which also starts the spark through
//! `fx::StartSystemEffect`). The cone test of FUN_0018d308 runs both ways for rows with `ReflectRange`
//! (`reaction_rules::reflect_cone`). Not done: `ReflectAction` (FUN_0019e870 reads it as an offset from
//! the row to an action record that 0x1b2478 hands to the actor's action system at +0x300,
//! FUN_00186db0), the hit registry tests of FUN_001be108. The script part of the enemy handler (the
//! running mode's clash hook and event 0x13) is delivered by `enemy_ai.rs` from [`EnemyClash`].

use std::collections::HashSet;

use bevy::prelude::*;

use crate::combat::{live_attacks, overlap, Fighter, ReactionData, Team};
use crate::data::GameData;
use crate::enemy::EnemyClash;
use crate::reaction_rules::{self as rules, ReflectFields};
use crate::schedule::Sim;
use crate::skel::Animator;

/// The player's side of a clash, for `sora.rs` (party handler FUN_001754a0, [`rules::party_clash`]):
/// the rows of both volumes, so the consumer can decide with its own state (airborne, owner flags).
#[allow(dead_code)]
#[derive(Message, Clone, Copy, Debug)]
pub struct Clash {
    pub player: Entity,
    pub enemy: Entity,
    pub player_atkp: i16,
    pub enemy_atkp: i16,
    /// `party_clash` for a plain ground actor.
    pub on_ground: rules::ClashReaction,
}

pub struct ClashPlugin;

impl Plugin for ClashPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Clash>()
            .add_systems(FixedUpdate, (detect_clashes, force_enemy_clash).after(crate::enemy::think).in_set(Sim::Enemy));
    }
}

/// Harness `KH2_CLASH_FORCE_ENEMY=<s>`: at that time the first living enemy gets one clash with no
/// reaction of its own (rows 0 and 0), so the script part of 0x1b2478 (clash hook, event 0x13) runs
/// once. Not the original's behaviour: the real pairing needs attack volumes that meet.
fn force_enemy_clash(time: Res<Time>, enemies: Query<(Entity, &crate::enemy::Enemy)>, mut out: MessageWriter<EnemyClash>, mut at: Local<Option<Option<f32>>>) {
    let t = *at.get_or_insert_with(|| std::env::var("KH2_CLASH_FORCE_ENEMY").ok().and_then(|v| v.trim().parse().ok()));
    let Some(t) = t else { return };
    if time.elapsed_secs() < t {
        return;
    }
    let Some((e, en)) = enemies.iter().find(|(_, en)| en.alive()) else { return };
    info!("clash: forced enemy clash on {} {e:?} (harness)", en.name);
    out.write(EnemyClash { enemy: e, motion: None, power: 0.0, own_atkp: 0, other_atkp: 0 });
    *at = Some(None);
}

/// One live attack window.
struct Live {
    fighter: Entity,
    team: Team,
    body: Entity,
    atkp: i16,
    start: i16,
    vol: crate::combat::Volume,
}

#[allow(clippy::too_many_arguments)]
fn detect_clashes(
    mut fighters: Query<(Entity, &mut Fighter)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
    data: Res<GameData>,
    reaction: Option<Res<ReactionData>>,
    places: Query<&GlobalTransform, With<Fighter>>,
    mut done: Local<HashSet<(Entity, i16, Entity, i16)>>,
    mut enemy_out: MessageWriter<EnemyClash>,
    mut player_out: MessageWriter<Clash>,
) {
    let Some(reaction) = reaction else { return };
    let mut live: Vec<Live> = Vec::new();
    for (e, f) in &fighters {
        for &body in &f.attack_bodies {
            let Ok((anim, g)) = bodies.get(body) else { continue };
            for (atkp, start, vol) in live_attacks(anim, g) {
                live.push(Live { fighter: e, team: f.team, body, atkp, start, vol });
            }
        }
    }
    // FUN_0018d308(A, B) with the owners' places: A's facing, and the flat unit vector from B's owner
    // (its +0x730, taken as its position [inferred]) to A's owner (FUN_0018c4f8).
    let log_cone = std::env::var("KH2_HITLOG").is_ok_and(|v| v != "0");
    let cone = |me: &Live, other: &Live, row: &kh2_tables::tables::Atkp| {
        let (Ok(pa), Ok(pb)) = (places.get(me.fighter), places.get(other.fighter)) else { return true };
        let facing = pa.rotation() * Vec3::Z;
        let from_other = (pa.translation() - pb.translation()).with_y(0.0).normalize_or(-facing);
        rules::reflect_cone(row.reflect_range, row.reflect_angle, facing, from_other)
    };
    done.retain(|(a, sa, b, sb)| live.iter().any(|l| l.fighter == *a && l.start == *sa) && live.iter().any(|l| l.fighter == *b && l.start == *sb));
    let mut disable: Vec<(Entity, Entity, i16)> = Vec::new();
    for (i, a) in live.iter().enumerate() {
        for b in &live[i + 1..] {
            if a.team == b.team || done.contains(&(a.fighter, a.start, b.fighter, b.start)) || done.contains(&(b.fighter, b.start, a.fighter, a.start)) {
                continue;
            }
            let (Some(ra), Some(rb)) = (data.atkp(a.atkp), data.atkp(b.atkp)) else { continue };
            if !reaction.consts.types_clash(ra.r#type, rb.r#type) || !overlap(&a.vol, &b.vol) {
                continue;
            }
            // FUN_0018d308 both ways: a row with a ReflectRange cone clashes only with an attack whose
            // owner stands inside the cone in front of its own owner.
            if (ra.reflect_range != 0 || rb.reflect_range != 0) && !(cone(a, b, ra) && cone(b, a, rb)) {
                if log_cone {
                    info!("clash: attack {} against {}: outside the ReflectRange cone ({} / {}), no clash", a.atkp, b.atkp, ra.reflect_range, rb.reflect_range);
                }
                continue;
            }
            done.insert((a.fighter, a.start, b.fighter, b.start));
            let (fa, fb) = (ReflectFields::from_atkp(ra), ReflectFields::from_atkp(rb));
            info!("clash: {:?} attack {} meets {:?} attack {}", a.team, a.atkp, b.team, b.atkp);
            for (me, other, mine, theirs) in [(a, b, &fa, &fb), (b, a, &fb, &fa)] {
                // FUN_0018d6b0: both volumes are disabled unless the side's own row says Nothing.
                if mine.refact_self != rules::Refact::Nothing {
                    disable.push((me.fighter, me.body, me.start));
                }
                match me.team {
                    Team::Enemy => {
                        // Every clash: 0x1b2478 hands it to the script (event 0x13) before its own tests.
                        let c = rules::enemy_clash(mine, false);
                        enemy_out.write(EnemyClash {
                            enemy: me.fighter,
                            motion: c.and_then(|c| c.motion),
                            power: c.map_or(0.0, |c| c.push_power),
                            own_atkp: me.atkp,
                            other_atkp: other.atkp,
                        });
                        if let Some(action) = c.and_then(|c| c.action) {
                            debug!("clash: enemy row {} would start script action {action} (not decoded)", me.atkp);
                        }
                    }
                    Team::Player => {
                        player_out.write(Clash {
                            player: me.fighter,
                            enemy: other.fighter,
                            player_atkp: me.atkp,
                            enemy_atkp: other.atkp,
                            on_ground: rules::party_clash(mine.refact_self, theirs.refact_other, true),
                        });
                    }
                }
            }
        }
    }
    // A disabled volume cannot hit: mark its window as landed on every fighter of the other team so
    // `combat::detect_hits` skips it until the window closes.
    if disable.is_empty() {
        return;
    }
    let teams: Vec<(Entity, Team)> = fighters.iter().map(|(e, f)| (e, f.team)).collect();
    for (owner, body, start) in disable {
        let Ok((_, mut f)) = fighters.get_mut(owner) else { continue };
        let mine = f.team;
        for (target, team) in &teams {
            if *team != mine {
                f.landed.insert((*target, body, start));
            }
        }
    }
}
