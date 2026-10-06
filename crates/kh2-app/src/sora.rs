//! Sora's player controller, run at the game's 60 Hz logic rate.
//!
//! Movement comes from `motor` (the game's ground, jump, air and fall model), the growth
//! abilities (High Jump, Quick Run, Aerial Dodge, Glide) use the `fmab` rows of their level, and
//! motions carry their own root motion (attack lunges, finisher spins). Motion IDs and blend
//! times are the game's (blends in ticks). Combo flow follows the motions' trigger windows:
//! range trigger 11 (allow combo) and 50 (allow finisher) open the next input; 3 suspends
//! gravity; 2 allows blending out early.
//!
//! Which motion an attack plays comes from the game's player attack table (`ptya`, see
//! `kh2_anim::moveset`): airborne-only and ground-only actions are told apart by the table's own
//! flags, finishers need a long enough combo or an open finisher window, and ability-gated actions
//! need their ability bit. Motion tags (A330, ...) are animation names; the table names motion IDs.

use std::sync::Arc;

use bevy::input::mouse::MouseButton;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_anim::moveset::{self, key, Action, BlowState, Chain, ExplicitMiss, Moveset, Probe, Recovery, Scene, Verdict, Windows};
use kh2_anim::{id, triggers::range, PlayerState};
use kh2_model::collision::kind as shape_kind;

use crate::assets::Disc;
use crate::camera::OrbitCamera;
use crate::combat::{Fighter, Hit, IneffectiveHit, ReactionData, ReactionStatus, Team, WallHit};
use crate::data::{GameData, Stats};
use crate::enemy::Enemy;
use crate::motor::{wrap_angle, Jump, Motor};
use crate::reaction_rules::{self as rules, Blow, Push, ReflectFields, VictimAction, VictimCtx};
use crate::schedule::{Frame, Sim};
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::world::World;
use crate::SCALE;

/// The bones the weapons hang from, by hand: 0 the right hand (bone 178, checked on screen), 1 the
/// left hand (bone 86, the mirror of 178, inferred: the game's attach path `FUN_001751b0` ->
/// `FUN_00184190` ends in resource handles, not a bone number; `objentry.WeaponJoint` is the
/// data candidate, see docs/drive-forms.md). The game attaches hand 1 only while a drive form is
/// active (`FUN_00176f88`: form field 1..5 or 9).
pub const HAND_BONES: [usize; 2] = [178, 86];

/// Ability bits (the attack table's gate) that start set: Guard.
const ABILITY_GUARD: u16 = 1;
/// Base combo lengths of the status block (FUN_001c0d88): attacks before the finisher on the
/// ground and in the air, and finishers per chain.
const BASE_HITS_GROUND: u8 = 3;
const BASE_HITS_AIR: u8 = 3;
const BASE_FINISHERS: u8 = 1;
/// Ticks the keyblade stays out after the last sign of a fight.
const BATTLE_TICKS: f32 = 240.0;

/// The battle flag every system reads (the game's `FUN_001903d8`, a tri-state set from the player's
/// and the enemies' awareness). The app's stand-in: ticks left of the battle mode that Sora's
/// controller raises while an enemy is within lock-on range or after an attack, and that drives
/// the keyblade out. The drive code reads it too.
#[derive(Resource, Default, Debug)]
pub struct Battle {
    pub ticks: f32,
}

impl Battle {
    pub fn active(&self) -> bool {
        self.ticks > 0.0
    }
}

/// The ability block as the controller reads it (`FUN_001c0d88`, `FUN_001c0f40`): the ability
/// bits that gate the attack table and the combo lengths. The ability system (`abilities.rs`)
/// writes it from its block whenever that changes (equipment, the `KH2_ABILITIES` overrides, a
/// drive form's list on entry and revert). The default is the app's baseline when the ability
/// system is off: Guard and the block's base lengths.
#[derive(Resource, Clone, Debug)]
pub struct Abilities {
    pub bits: u128,
    pub max_ground: u8,
    pub max_air: u8,
    pub max_finishers: u8,
}

impl Default for Abilities {
    fn default() -> Abilities {
        Abilities { bits: 1u128 << ABILITY_GUARD, max_ground: BASE_HITS_GROUND, max_air: BASE_HITS_AIR, max_finishers: BASE_FINISHERS }
    }
}

impl Abilities {
    /// The controller's view of the ability block.
    pub fn from_block(a: &crate::abilities::Abilities) -> Abilities {
        Abilities { bits: a.bits(), max_ground: a.ground_combo(), max_air: a.air_combo(), max_finishers: a.finishers() }
    }

    /// Every ability bit set (the low 128 of the block's 256).
    pub fn bits(&self) -> u128 {
        self.bits
    }

    /// Attacks before the finisher on the ground and in the air, and finishers per chain
    /// (block `+0x28`, `+0x29`, `+0x2a`; the game counts equipped copies).
    pub fn combo_block(&self) -> (u8, u8, u8) {
        (self.max_ground, self.max_air, self.max_finishers)
    }

    /// The combo state the attack table's availability tests read.
    fn chain(&self, hits: u8, finishers: u8, airborne: bool, finisher_window: bool) -> Chain {
        let (max_ground, max_air, max_finishers) = self.combo_block();
        Chain { airborne, abilities: self.bits(), combo: hits, finishers, max_ground, max_air, max_finishers, finisher_window }
    }
}

/// `KH2_MOVELOG=1`: one log block per attack press (each record considered and why it was
/// flagged or not, and the chosen motion).
fn move_log() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("KH2_MOVELOG").is_some())
}

/// Motion IDs of the player motion set (state-independent part of the slot).
mod mid {
    pub const HIGH_JUMP: usize = 201;
    pub const AERIAL_DODGE: usize = 202;
    pub const QUICK_RUN: usize = 203;
    pub const QUICK_RUN_END: usize = 204;
    pub const GLIDE: usize = 205;
    pub const GLIDE_LEFT: usize = 206;
    pub const GLIDE_RIGHT: usize = 207;
    pub const GLIDE_HOVER: usize = 208;
}

/// Blend lengths in ticks.
const BLEND_LOCOMOTION: f32 = 8.0;
const BLEND_FALL: f32 = 8.0;
const BLEND_GLIDE: f32 = 16.0;
const BLEND_ATTACK: f32 = 4.0;
/// Stick magnitude below which Sora walks.
const WALK_STICK: f32 = 0.8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Ground,
    Air,
    QuickRun,
    Glide,
    Attack { air: bool, finisher: bool },
    Guard,
    Counter,
    Hurt,
    Blown,
    Down,
    Viewer,
    /// A drive form change is playing (motions A120 and A121): the drive code holds the controller,
    /// there is no input, and the hit tests are off. It sets the state back when the change ends.
    FormChange,
    /// A reaction command's paired motion is playing on Sora (`player_reaction.rs` starts and ends
    /// it): no input, his velocity is the script's secondary velocity, gravity goes on.
    Reaction,
    /// A spell's cast motion is playing (`magic.rs` starts it): no input until the motion lets go.
    Magic,
}

/// Growth-ability levels (0 = not learned, 5 = MAX). Written from the ability block's growth
/// levels (`FUN_001c1340`) whenever the abilities change (`abilities::sync_controller`); in a form
/// the form's own growth ability is the only one that applies.
#[derive(Resource, Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Growth {
    pub high_jump: u8,
    pub quick_run: u8,
    pub aerial_dodge: u8,
    pub glide: u8,
}

/// `KH2_GROWTH=a,b,c,d`: the harness's growth levels (up to 5, the forced MAX of `FUN_001c13b8`),
/// used instead of the ability block's while no form is active.
#[derive(Resource, Clone, Copy, Default, Debug)]
pub struct GrowthOverride(pub Option<Growth>);

/// Controller input, sampled every rendered frame and consumed by the 60 Hz tick.
#[derive(Resource, Default, Debug)]
pub struct Pad {
    /// Camera-relative move direction (unit, or zero).
    pub dir: Vec3,
    /// Stick magnitude 0..1 (keyboard: 1, or 0.5 while walking).
    pub stick: f32,
    pub jump_held: bool,
    pub square_held: bool,
    attack: u8,
    jump: u8,
    square: u8,
    lock: u8,
    viewer: u8,
    next: u8,
    prev: u8,
}

impl Pad {
    fn take(n: &mut u8) -> bool {
        let p = *n > 0;
        *n = n.saturating_sub(1);
        p
    }

    /// Takes one pending attack press for another use (the command menu executes the selected
    /// command with the same button, `magic.rs`). True when there was one.
    pub fn steal_attack(&mut self) -> bool {
        Pad::take(&mut self.attack)
    }
}

#[derive(Component)]
pub struct Sora {
    pub state: State,
    pub motor: Motor,
    /// The game's combo counters (actor +0xb04, +0xb05): attacks accepted in the current chain, and
    /// finishers accepted (1 at the start of a chain).
    pub hits: u8,
    pub finishers: u8,
    pub queued_attack: bool,
    /// A hit of the current swing landed (actor +0x108 bit 0x2000000): a chain only goes on after one.
    pub connected: bool,
    /// The refused press was already logged.
    deny_logged: bool,
    /// Sora's attack table (the ability bits that unlock gated actions are the `Abilities` resource).
    pub moves: Arc<Moveset>,
    /// The blow that knocked Sora down came from behind (picks the matching landing motion).
    blown_back: bool,
    /// Decaying push (wall recoil, push reactions), added to the movement every tick.
    push: Option<Push>,
    /// Flight of the blow that put Sora in `State::Blown`.
    blow: Option<Blow>,
    pub stats: Stats,
    pub hp: i32,
    pub mp: i32,
    /// Invincible ticks left.
    pub invuln: f32,
    pub lock: Option<Entity>,
    pub target: Option<Entity>,
    pub body: Entity,
    /// The weapons in hand order (0 right, 1 left), as the game's per-object weapon list
    /// (`FUN_001751b0` attaches hand 0 for every player object and hand 1 in a form). Empty
    /// for a form without a weapon (Anti). The drive code swaps it with the body.
    pub weapons: Vec<Entity>,
    /// Ticks in the current state.
    timer: f32,
    /// Quick Run: ticks left while the button is held, and the per-tick brake.
    dash_time: f32,
    dash_brake: f32,
    /// One Aerial Dodge per airtime below MAX.
    aerial_dodge_ready: bool,
    lunge_left: f32,
    lunge_speed: f32,
    viewer_slot: usize,
}

/// One motion request the controller makes of the weapons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponMotion {
    pub slot: Option<usize>,
    pub looping: bool,
    pub blend: f32,
    /// `Animator::restart` rather than `play`.
    pub restart: bool,
}

/// The weapons as the controller addresses them in one tick. It has the `play`/`restart` shape
/// of `Animator`, so the attack code is written as for one weapon; `apply` replays the requests on
/// every weapon in order, each weapon playing a slot only when its own motion set has it (what
/// the one-weapon code did with `wrig.mset.has`). The requests are applied before the pose step
/// of the same tick (`Sim::Pose` runs after `Sim::Sora` and `Sim::SoraHurt`), so the result is
/// the same as writing the animator directly.
#[derive(Default, Debug)]
pub struct WeaponCue {
    pub ops: Vec<WeaponMotion>,
}

impl WeaponCue {
    pub fn play(&mut self, slot: Option<usize>, looping: bool, blend: f32) {
        self.ops.push(WeaponMotion { slot, looping, blend, restart: false });
    }

    pub fn restart(&mut self, slot: Option<usize>, looping: bool, blend: f32) {
        self.ops.push(WeaponMotion { slot, looping, blend, restart: true });
    }

    pub fn apply(&self, weapons: &[Entity], anims: &mut Query<&mut Animator>) {
        for w in weapons {
            let Ok(mut a) = anims.get_mut(*w) else { continue };
            for op in &self.ops {
                let slot = op.slot.filter(|&x| a.rig.mset.has(x));
                if op.restart {
                    a.restart(slot, op.looping, op.blend);
                } else {
                    a.play(slot, op.looping, op.blend);
                }
            }
        }
    }
}

/// The weapons' motion sets together: `mset.has(slot)` when any weapon has the slot (the
/// controller only asks whether the weapons follow a body slot; `WeaponCue::apply` decides per weapon).
pub struct WeaponRigs {
    pub mset: WeaponMsets,
}

pub struct WeaponMsets(Vec<Arc<Rig>>);

impl WeaponMsets {
    pub fn has(&self, slot: usize) -> bool {
        self.0.iter().any(|r| r.mset.has(slot))
    }
}

impl WeaponRigs {
    pub fn of(weapons: &[Entity], anims: &Query<&mut Animator>) -> WeaponRigs {
        WeaponRigs { mset: WeaponMsets(weapons.iter().filter_map(|&w| anims.get(w).ok().map(|a| a.rig.clone())).collect()) }
    }
}

impl Sora {
    pub fn grounded(&self) -> bool {
        self.motor.grounded
    }
    pub fn yaw(&self) -> f32 {
        self.motor.yaw
    }

    /// Enters `state` from outside the controller (reaction commands and clashes,
    /// `player_reaction.rs`): the motion is already requested; the state's clock restarts, the
    /// velocity and a pending attack are dropped and `push` replaces the running push.
    pub fn enter(&mut self, state: State, push: Option<Push>) {
        self.state = state;
        self.timer = 0.0;
        self.motor.vel = Vec3::ZERO;
        self.queued_attack = false;
        self.push = push;
    }
}

/// A target point of an enemy in game units (Y up here), from its target-kind collision shape.
#[derive(Clone, Copy, Debug)]
struct TargetPoint {
    pos: Vec3,
    radius: f32,
    half_height: f32,
}

/// The target points of a body: its target shapes (kind 3) whose argument has bit 0 clear, on their
/// bones, moved up by the shape's lift (FUN_001894d0).
fn target_points(anim: &Animator, body: &GlobalTransform) -> Vec<TargetPoint> {
    anim.rig
        .model
        .shapes
        .iter()
        .filter(|s| s.kind == shape_kind::TARGET && s.argument & 1 == 0)
        .map(|s| {
            let m = match s.bone {
                Some(b) => anim.bone_world(body, b),
                None => body.to_matrix(),
            };
            let mut pos = m.transform_point3(Vec3::from(s.offset)) / SCALE;
            pos.y += s.lift;
            TargetPoint { pos, radius: s.radius, half_height: s.height }
        })
        .collect()
}

/// How `p` lies from `from` (FUN_001cba68): horizontal distance less the radius, the Y-down height
/// difference, and the dot product of the horizontal direction with `reference` (a unit vector).
fn probe(p: &TargetPoint, from: Vec3, reference: Vec3) -> Probe {
    let d = p.pos - from;
    let flat = d.with_y(0.0);
    let len = flat.length();
    Probe {
        dist: (len - p.radius).max(0.0),
        dy: -d.y,
        dot: (flat.normalize_or_zero().dot(reference)).min(1.0),
        half_height: p.half_height,
    }
}

/// The reference direction of the selection (`ctx + 0x30` of FUN_001cb928): the horizontal unit
/// direction to the target, or Sora's facing without one.
fn reference_direction(m: &Motor, target: Option<TargetPoint>) -> Vec3 {
    target
        .map(|t| (t.pos - m.pos).with_y(0.0).normalize_or_zero())
        .filter(|d| *d != Vec3::ZERO)
        .unwrap_or_else(|| m.forward().with_y(0.0).normalize_or_zero())
}

/// Everything the table selection needs about the surroundings (FUN_001cb928): the target and its
/// direction (the reference direction of the angle test; Sora's facing without a target), the
/// target points of every enemy, the jump height the flag-bit-1 records add to their reach, and
/// whether the ledge probe in front is set (no floor ahead).
fn selection_scene(m: &Motor, target: Option<TargetPoint>, others: &[TargetPoint], jump_height: f32, ledge_probe: bool) -> Scene {
    let reference = reference_direction(m, target);
    Scene {
        target: target.map(|t| probe(&t, m.pos, reference)),
        others: others.iter().map(|t| probe(t, m.pos, reference)).collect(),
        ledge_probe,
        jump_height,
    }
}

/// FUN_0016be20 (FUN_0016bd98): the jump height the flag-bit-1 records add to their reach, and
/// the height the jump action uses. A party member (+0x588 flag 4) whose mover does not carry
/// flag 0x800 and whose High Jump level (ability block +0x20) is above 0 gets the High Jump
/// height of the `fmab` row of that level (and motion 201); otherwise the actor's own jump height
/// (+0x118; the app reads `prty` JumpHeight, the game's writer of +0x118 was not found).
fn jump_height(data: &GameData, growth: &Growth) -> f32 {
    data.fmab(growth.high_jump).map_or(data.prty.jump_height, |f| f.high_jump_height)
}

/// Sora's body collision shape (the first shape of kind 0 or 4, FUN_00188e70 with mask 0x11 in
/// FUN_00169af0), in game units: its radius is the actor's collision radius (+0x5b0), its height
/// field +0x5b4 and its lift the step height (+0x5fc).
#[derive(Clone, Copy, Debug, PartialEq)]
struct BodyShape {
    radius: f32,
    height: f32,
    lift: f32,
}

fn body_shape(rig: &Rig) -> Option<BodyShape> {
    rig.model.shapes.iter().find(|s| s.kind == 0 || s.kind == 4).map(|s| BodyShape { radius: s.radius.abs(), height: s.height, lift: s.lift })
}

/// The result of the ledge probe: where the sweep ended, the floor found under it, the lowest
/// floor height that still counts, and whether the probe's bit is set (no floor, or too low).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ledge {
    end: Vec3,
    floor: Option<f32>,
    limit: f32,
    flagged: bool,
}

/// The ledge probe of the table selection (FUN_001cbcf8 -> FUN_00182e38 -> FUN_0019a8e8). Only on
/// the ground (FUN_0019aa28 false) for an actor whose mover is not a rider (flag 2) and that has no
/// +0x160 flag 0x40 and no +0x810 (neither modelled): the vector is the reference direction
/// projected on the ground plane (the actor's +0x630 normal) times `radius + plyr AttackJFront`.
/// FUN_0019a8e8 returns bit 32 of the word at +0x600 (`+0x604 & 1`): the floor test after the
/// sweep, so a set result means NO floor ahead. Key 1 is flagged then. `airborne` is the
/// selection's airborne state (FUN_0019aa28, the chain's), not the motor's floor contact.
fn ledge_probe(world: &World, m: &Motor, airborne: bool, shape: Option<BodyShape>, reference: Vec3, j_front: f32) -> Option<Ledge> {
    if airborne {
        return None;
    }
    let b = shape?;
    let normal = m.ground.map_or(Vec3::Y, |g| g.normal);
    // FUN_00182e38: two cross products with the ground normal keep the direction in its plane.
    let dir = normal.cross(reference).cross(normal).normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    Some(ledge_at(world, m.pos, dir * (b.radius + j_front), b))
}

/// FUN_00143840 for the probe of FUN_0019a8e8 from `start` (the actor position, +0x590) along `v`
/// (+0x5a0), Y up here (the game's Y points down):
/// * the sweep (FUN_00164138): the body sphere, centred `lift` above the start, moves along `v`
///   against the party-collide walls. [inferred] It ends where a wall ahead stops the sphere, else
///   at `start + v` (the sweep's end point, +0x5c0, is what the floor test reads).
/// * the floor test (FUN_001423d8): a ray down at the end point from `lift + clamp(|v.y|, 0, 100)`
///   above it to `3 * lift` below it (DAT_00378f60 = 100, DAT_00378f64 = 3; the clamp applies for
///   mask index 0, Sora's). No floor sets bit 33 and, through FUN_00141910, bit 32.
/// * FUN_00141910: a floor found clears bit 32 when it is no lower than
///   `start.y - max(0, -v.y) - (lift - height) + 1` (the game's `hit.y <= start.y + max(0, v.y) +
///   (lift - +0x5b4) + +0x610 - 1.0` with Y down, DAT_00378f50 = 1.0; +0x610 is 0 because the probe
///   only runs with +0x810 zero, FUN_00183e90); a lower floor sets it.
fn ledge_at(world: &World, start: Vec3, v: Vec3, b: BodyShape) -> Ledge {
    let len = v.length();
    let centre = start + Vec3::Y * b.lift;
    let travel = if len > 1e-4 {
        let dir = v / len;
        crate::motor::sweep_wall(world, centre, centre + v, b.radius, kh2_map::flags::PARTY_COLLIDE)
            .map(|(q, _)| (q - centre).dot(dir))
            // A wall the sphere already touches level with or behind its centre blocks nothing
            // ahead (the earlier port's approximation; the game's start-overlap case was not read).
            .filter(|ahead| *ahead > 0.0)
            .map_or(len, |ahead| (ahead - b.radius).clamp(0.0, len))
    } else {
        0.0
    };
    let end = if len > 1e-4 { start + v * (travel / len) } else { start };
    let top = end.y + b.lift + v.y.abs().min(100.0);
    let bottom = end.y - 3.0 * b.lift;
    let floor = world.ground(end, top, top - bottom).map(|g| g.y);
    let limit = start.y - (-v.y).max(0.0) - (b.lift - b.height) + 1.0;
    let flagged = floor.is_none_or(|y| y < limit);
    Ledge { end, floor, limit, flagged }
}

fn ledge_text(l: &Ledge) -> String {
    format!(
        "{}: sweep end ({:.0} {:.0} {:.0}), floor {}, lowest floor that counts {:.1}",
        if l.flagged { "SET (no floor ahead), key 1 flagged" } else { "clear" },
        l.end.x,
        l.end.y,
        l.end.z,
        l.floor.map_or("none".to_string(), |y| format!("{y:.1}")),
        l.limit
    )
}

/// A button press while Sora is blown (FUN_001758b0; the party controller runs it on every
/// command tick before the Square actions and the attack). `jump` is command flag 2 (the jump
/// button, Space: FUN_001babf0 sets it from logical pad bit 0x100, which FUN_00167310 maps from
/// the cancel button) and `square` command flag 8 (Square, K/E: logical 0x200). Aerial Recovery
/// plays motion 40 and ends the flight; Retaliating Slash restarts the counters and starts the
/// key 40 record through the attack start. Returns true when the state changed.
#[allow(clippy::too_many_arguments)]
fn blown_press(
    s: &mut Sora,
    body: &mut Animator,
    weapon: &mut WeaponCue,
    abilities: &Abilities,
    data: &GameData,
    jump: bool,
    square: bool,
    rig: &Rig,
    pstate: PlayerState,
    start_attack: &impl Fn(&mut Sora, &mut Animator, &mut WeaponCue, &Action, bool) -> bool,
) -> bool {
    let air = !s.motor.grounded;
    let mut chain = abilities.chain(s.hits, s.finishers, air, false);
    // The blow movers (modes 4 and 8 for a party member) carry flag 0x40; the flight timer is the
    // ticks in the state; +0x7f8 flag 4 stands in as "no HP left" (inferred: set when the actor
    // leaves play).
    let blow = BlowState { recoverable: s.blow.is_some(), ticks: s.timer, recovery_time: data.plyr.blow_recovery_time, locked: s.hp <= 0 };
    let moves = s.moves.clone();
    match moves.recovery(&blow, jump, square, &mut chain) {
        Ok(Recovery::AerialRecovery) => {
            // FUN_001759b8: motion 40 with blend 4, then the mover's launch (its method +8 with a
            // vector built from the stick direction, plyr BlowRecoveryH and BlowRecoveryV; the
            // launch itself was not decoded). The app ends the flight and lets Sora fall.
            body.restart(rig.mset.slot(id::DAMAGE_BLOW_RECOV, pstate), false, 4.0);
            weapon.restart(None, false, 4.0);
            s.blow = None;
            s.push = None;
            s.motor.vel = Vec3::ZERO;
            s.motor.start_fall();
            s.state = State::Air;
            s.timer = 0.0;
            s.hits = 0;
            s.finishers = 1;
            info!("move: blown press: Aerial Recovery (ability bit {}) after {:.0} ticks (BlowRecoveryTime {:.0}): motion 40 {}", moveset::ABILITY_AERIAL_RECOVERY, blow.ticks, blow.recovery_time, body.tag());
            true
        }
        Ok(Recovery::Retaliate(a)) => {
            let a = a.clone();
            info!("move: blown press: Retaliating Slash key {} after {:.0} ticks (BlowRecoveryTime {:.0}): counters restart (combo {}, finishers {})", a.key, blow.ticks, blow.recovery_time, chain.combo, chain.finishers);
            s.hits = chain.combo;
            s.finishers = chain.finishers;
            s.blow = None;
            s.push = None;
            start_attack(s, body, weapon, &a, air)
        }
        Err(m) => {
            if move_log() {
                info!("move: blown press refused: {m:?} (ticks {:.0}, BlowRecoveryTime {:.0}, jump {jump}, square {square})", blow.ticks, blow.recovery_time);
            }
            false
        }
    }
}

#[derive(Resource)]
pub struct Rigs {
    pub sora: Arc<Rig>,
    pub keyblade: Arc<Rig>,
    pub shadow: Arc<Rig>,
    pub soldier: Arc<Rig>,
}

fn rig(disc: &Disc, model: &str, mset: &str) -> Arc<Rig> {
    let m = disc.read(model).and_then(|d| kh2_model::mdlx(&d)).unwrap_or_else(|| panic!("{model}: cannot load"));
    let s = disc.read(mset).and_then(|d| kh2_anim::Mset::parse(&d)).unwrap_or_else(|| panic!("{mset}: cannot load"));
    let apdx = model.strip_suffix(".mdlx").map(|b| format!("{b}.apdx")).filter(|a| disc.read(a).is_some());
    Arc::new(Rig { model: m, mset: s, apdx })
}

/// Sora, the pad and the character rigs (Sora, keyblade, Shadow, Soldier). Needs `Disc`, so it
/// is added after `AssetsPlugin`.
pub struct SoraPlugin;

impl Plugin for SoraPlugin {
    fn build(&self, app: &mut App) {
        let rigs = {
            let disc = app.world().resource::<Disc>();
            Rigs {
                sora: rig(disc, "obj/P_EX100.mdlx", "obj/P_EX100.mset"),
                keyblade: rig(disc, "obj/W_EX010.mdlx", "obj/W_EX010.mset"),
                shadow: rig(disc, "obj/M_EX020.mdlx", "obj/M_EX020.mset"),
                soldier: rig(disc, "obj/M_EX010.mdlx", "obj/M_EX010.mset"),
            }
        };
        crate::startup_log("rigs");
        app.insert_resource(rigs)
            .init_resource::<Pad>()
            .init_resource::<Battle>()
            .init_resource::<Abilities>()
            .add_systems(Startup, setup)
            .add_systems(Update, read_pad.in_set(Frame::Pad))
            .add_systems(
                FixedUpdate,
                (
                    sync_weapon.in_set(Sim::Weapon),
                    (follow_hand, tick).chain().in_set(Sim::Sora),
                    (track_connects, receive_hits, receive_wall_hits).chain().in_set(Sim::SoraHurt),
                    apply_player_fade.after(Sim::Pose),
                ),
            );
    }
}

/// The material state a faded mesh goes back to.
#[derive(Component)]
pub struct FadeBase(AlphaMode);

/// The player's draw fade (`actor+0x834`, driven by the look-around, `OrbitCamera::player_fade`)
/// applied to Sora's body and weapons. The game multiplies the fade into the object's alpha byte
/// (`FUN_00191c18` stores `base alpha * FUN_00193270`, and a child object such as a weapon also
/// multiplies its parent's factor), so the model goes see-through and at 0 is not drawn. Here a
/// faded mesh switches to alpha blending with the fade as its alpha; at 1 the original alpha
/// mode comes back.
pub fn apply_player_fade(
    cam: Query<&OrbitCamera>,
    sora: Query<&Sora>,
    children: Query<&Children>,
    mut meshes: Query<(Entity, &MeshMaterial3d<StandardMaterial>, Option<&FadeBase>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    mut faded: Local<bool>,
) {
    let (Ok(cam), Ok(s)) = (cam.single(), sora.single()) else { return };
    let fade = cam.player_fade.clamp(0.0, 1.0);
    if fade >= 1.0 && !*faded {
        return;
    }
    *faded = fade < 1.0;
    let roots = std::iter::once(s.body).chain(s.weapons.iter().copied());
    for root in roots {
        for e in children.iter_descendants(root) {
            let Ok((entity, handle, base)) = meshes.get_mut(e) else { continue };
            let Some(mut m) = materials.get_mut(&handle.0) else { continue };
            if fade < 1.0 {
                let original = base.map_or(m.alpha_mode, |b| b.0);
                if base.is_none() {
                    commands.entity(entity).insert(FadeBase(original));
                }
                m.alpha_mode = AlphaMode::Blend;
                m.base_color.set_alpha(fade);
            } else if let Some(b) = base {
                m.alpha_mode = b.0;
                m.base_color.set_alpha(1.0);
                commands.entity(entity).remove::<FadeBase>();
            }
        }
    }
}

/// Spawns Sora: the base body of the current world and its keyblade. The game builds the player
/// object from the merged `memt` record (`FUN_001cf2c8` with form 0: word 8 + the world's outfit
/// index) and the weapon from its moveset's `went` row with the equipped keyblade (`FUN_0016ef28`,
/// `FUN_001845b0`) and motion set from `wmst` (`FUN_00184640`); `DriveRules::base_files` is that
/// lookup. The rigs loaded by the plugin (`P_EX100`, `W_EX010`) are reused when the merge names
/// them and replaced otherwise, so the world outfit variants (`_TR`, `_WI`, `_NM`) appear, and
/// every reader of `Rigs` (the revert, the effect library) sees the chosen base.
#[allow(clippy::too_many_arguments)]
pub fn setup(
    mut commands: Commands,
    mut rigs: ResMut<Rigs>,
    data: Res<GameData>,
    disc: Res<Disc>,
    drive: Option<Res<crate::drive::DriveRules>>,
    world: Option<Res<crate::drive::DriveWorld>>,
    mut lib: Option<ResMut<crate::fx::EffectLib>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let base = drive.as_deref().zip(world.as_deref()).and_then(|(d, w)| d.base_files(w));
    let mut moves = None;
    match &base {
        Some(f) => {
            info!(
                "sora: base body {} (mset {}, attack table {}), hand 0 {} (memt merge, world {}, outfit {})",
                f.model,
                f.mset,
                f.moveset,
                f.hands[0].as_ref().map_or("none".to_string(), |w| format!("{} ({})", w.model, w.mset.as_deref().unwrap_or("no motion set"))),
                world.as_deref().map_or(0, |w| w.id),
                f.outfit
            );
            if f.model != "P_EX100" || f.mset != "P_EX100" {
                match crate::drive::read_rig(&disc, &f.model, &f.mset) {
                    Some(r) => rigs.sora = r,
                    None => warn!("sora: {} not loadable; keeping P_EX100", f.model),
                }
            }
            match &f.hands[0] {
                Some(w) if w.model != "W_EX010" || w.mset.as_deref() != Some("W_EX010") => match crate::drive::weapon_rig(&disc, w) {
                    Some(r) => rigs.keyblade = r,
                    None => warn!("sora: weapon {} not loadable; keeping W_EX010", w.model),
                },
                Some(_) => {}
                None => warn!("sora: the base moveset names no weapon for the equipped keyblade; keeping W_EX010"),
            }
            if let Some(l) = lib.as_deref_mut() {
                // The effect library was built before this ran: register the chosen rigs' effect files.
                crate::drive::register_effects(&disc, l, &rigs.sora);
                crate::drive::register_effects(&disc, l, &rigs.keyblade);
            }
            moves = load_moveset_index(&disc, f.moveset as usize);
        }
        None => info!("sora: base body P_EX100 (drive tables off; no memt merge)"),
    }
    let root = commands.spawn((Transform::default(), Visibility::default())).id();
    let body = spawn_skinned(&mut commands, root, rigs.sora.clone(), &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
    // Hand 0 only: the game attaches the second hand in a drive form (FUN_00176f88).
    let weapons = spawn_weapons(&mut commands, body, &[(0, rigs.keyblade.clone())], &mut meshes, &mut materials, &mut images, &mut bindposes);
    let stats = data.sora_stats(1);
    let attack_bodies = weapons.iter().copied().chain([body]).collect();
    commands.entity(root).insert((
        Sora {
            state: State::Ground,
            motor: Motor::default(),
            hits: 0,
            finishers: 0,
            queued_attack: false,
            connected: false,
            deny_logged: false,
            moves: Arc::new(moves.unwrap_or_else(|| load_moveset(&disc, "P_EX100"))),
            blown_back: false,
            push: None,
            blow: None,
            hp: stats.max_hp,
            mp: data.sora_max_mp,
            stats,
            invuln: 0.0,
            lock: None,
            target: None,
            body,
            weapons,
            timer: 0.0,
            dash_time: 0.0,
            dash_brake: 0.0,
            aerial_dodge_ready: false,
            lunge_left: 0.0,
            lunge_speed: 0.0,
            viewer_slot: 0,
        },
        Fighter { team: Team::Player, attack_bodies, hurt_body: body, landed: Default::default(), hittable: true },
    ));
    let growth = std::env::var("KH2_GROWTH").ok().map(|v| {
        let l: Vec<u8> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        Growth { high_jump: l.first().copied().unwrap_or(0), quick_run: l.get(1).copied().unwrap_or(0), aerial_dodge: l.get(2).copied().unwrap_or(0), glide: l.get(3).copied().unwrap_or(0) }
    });
    commands.insert_resource(GrowthOverride(growth));
    commands.insert_resource(growth.unwrap_or_default());
}

/// Spawns one skinned weapon per `(hand, rig)` under `body`, each on the socket of its hand
/// (`HAND_BONES`), and returns the weapon entities in the given order. The weapons are in model units.
#[allow(clippy::too_many_arguments)]
pub fn spawn_weapons(
    commands: &mut Commands,
    body: Entity,
    rigs: &[(usize, Arc<Rig>)],
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    bindposes: &mut Assets<SkinnedMeshInverseBindposes>,
) -> Vec<Entity> {
    rigs.iter()
        .map(|(hand, rig)| {
            let bone = HAND_BONES.get(*hand).copied().unwrap_or(HAND_BONES[0]);
            let socket = commands.spawn((Transform::default(), Visibility::default(), ChildOf(body), HandSocket(bone))).id();
            spawn_skinned(commands, socket, rig.clone(), meshes, materials, images, bindposes, 1.0)
        })
        .collect()
}

/// The attack table of the object `model` (`NeoMoveset` of its `00objentry` row, an index into the
/// `ptya` subfile of `00battle.bin`).
fn load_moveset(disc: &Disc, model: &str) -> Moveset {
    use kh2_tables::tables as t;
    let index = {
        let mut a = disc.0.lock().unwrap();
        kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY)
            .ok()
            .and_then(|d| d.rows.into_iter().find(|r| r.model_name == model))
            .map(|r| r.neo_moveset as usize)
    };
    index.and_then(|i| load_moveset_index(disc, i)).unwrap_or_else(|| {
        warn!("{model}: no attack table; Sora cannot attack");
        Moveset::default()
    })
}

/// The attack table at `index` of `ptya`.
fn load_moveset_index(disc: &Disc, index: usize) -> Option<Moveset> {
    use kh2_tables::tables as t;
    let mut a = disc.0.lock().ok()?;
    kh2_tables::subfile(&mut a, &t::PTYA).ok().and_then(|blob| Moveset::parse(&blob, index))
}

/// Samples keyboard and mouse into the pad (every rendered frame). WASD move, Ctrl walk,
/// Space jump (Circle), J / left mouse attack (X), K / E guard and growth abilities (Square),
/// Q / Tab lock-on, F1 motion viewer, F6 cycles growth-ability levels.
pub fn read_pad(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cam: Query<&OrbitCamera>,
    mut pad: ResMut<Pad>,
    mut growth: ResMut<Growth>,
) {
    let mut v = Vec2::ZERO;
    for (k, d) in [(KeyCode::KeyW, Vec2::Y), (KeyCode::KeyS, -Vec2::Y), (KeyCode::KeyA, -Vec2::X), (KeyCode::KeyD, Vec2::X)] {
        if keys.pressed(k) {
            v += d;
        }
    }
    let yaw = cam.single().map_or(0.0, |c| c.yaw);
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    pad.dir = (fwd * v.y + right * v.x).normalize_or_zero();
    pad.stick = if pad.dir == Vec3::ZERO { 0.0 } else if keys.pressed(KeyCode::ControlLeft) { 0.5 } else { 1.0 };
    pad.jump_held = keys.pressed(KeyCode::Space);
    pad.square_held = keys.pressed(KeyCode::KeyK) || keys.pressed(KeyCode::KeyE);
    let bump = |n: &mut u8, p: bool| {
        if p {
            *n = (*n + 1).min(2);
        }
    };
    bump(&mut pad.attack, keys.just_pressed(KeyCode::KeyJ) || mouse.just_pressed(MouseButton::Left));
    bump(&mut pad.jump, keys.just_pressed(KeyCode::Space));
    bump(&mut pad.square, keys.just_pressed(KeyCode::KeyK) || keys.just_pressed(KeyCode::KeyE));
    bump(&mut pad.lock, keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::Tab));
    bump(&mut pad.viewer, keys.just_pressed(KeyCode::F1));
    bump(&mut pad.next, keys.just_pressed(KeyCode::BracketRight));
    bump(&mut pad.prev, keys.just_pressed(KeyCode::BracketLeft));
    if keys.just_pressed(KeyCode::F6) {
        let l = (growth.high_jump + 1) % 6;
        *growth = Growth { high_jump: l, quick_run: l, aerial_dodge: l, glide: l };
        info!("growth abilities: level {l}");
    }
}

/// The weapons' motion sets mirror Sora's slot for slot: lock every weapon to Sora's slot and
/// frame (a weapon without the slot plays nothing).
pub fn sync_weapon(sora: Query<&Sora>, mut anims: Query<&mut Animator>) {
    let Ok(s) = sora.single() else { return };
    let Ok(body) = anims.get(s.body) else { return };
    let (slot, frame, prev_frame, looping, speed) = (body.slot, body.frame, body.prev_frame, body.looping, body.speed);
    for w in &s.weapons {
        let Ok(mut weapon) = anims.get_mut(*w) else { continue };
        let slot = slot.filter(|&x| weapon.rig.mset.has(x));
        if weapon.slot != slot {
            weapon.restart(slot, looping, 0.0);
        }
        weapon.frame = frame;
        weapon.prev_frame = prev_frame;
        weapon.looping = looping;
        weapon.speed = speed;
    }
}

/// Marks a weapon socket under Sora's body: the entity follows the bone it names (`HAND_BONES`).
#[derive(Component)]
pub struct HandSocket(pub usize);

/// Keeps every weapon socket on its hand bone (the joint entities are siblings under the body).
pub fn follow_hand(sora: Query<&Sora>, anims: Query<&Animator>, mut sockets: Query<(&HandSocket, &mut Transform)>) {
    let Ok(s) = sora.single() else { return };
    let Ok(a) = anims.get(s.body) else { return };
    for (h, mut t) in &mut sockets {
        *t = Transform::from_matrix(a.pose.get(h.0).copied().unwrap_or(Mat4::IDENTITY));
    }
}

/// Height of the ground below Sora (0 when standing on it or nothing is below).
fn height_above_ground(world: &World, m: &Motor) -> f32 {
    world.ground(m.pos, m.pos.y + 1.0, 10_000.0).map_or(0.0, |g| (m.pos.y - g.y).max(0.0))
}

/// One 60 Hz tick of Sora's controller.
#[allow(clippy::too_many_arguments)]
pub fn tick(
    mut pad: ResMut<Pad>,
    data: Res<GameData>,
    growth: Res<Growth>,
    abilities: Res<Abilities>,
    mut battle: ResMut<Battle>,
    world: Option<Res<World>>,
    reaction: Option<Res<ReactionData>>,
    enemies: Query<(Entity, &GlobalTransform, &Enemy)>,
    mut sora_q: Query<(&mut Transform, &mut Sora, &mut Fighter)>,
    mut anims: Query<&mut Animator>,
    globals: Query<&GlobalTransform>,
    mut bodies: Query<&mut Transform, Without<Sora>>,
    mut vis: Query<&mut Visibility>,
) {
    let (Ok((mut tf, mut s, mut fighter)), Some(world)) = (sora_q.single_mut(), world) else { return };
    let s = &mut *s;
    // The target points of the enemies, for the attack table's range tests.
    let enemy_points: Vec<(Entity, Vec<TargetPoint>)> = enemies
        .iter()
        .filter(|(_, _, e)| e.alive())
        .filter_map(|(en, _, e)| Some((en, target_points(anims.get(e.body).ok()?, globals.get(e.body).ok()?))))
        .collect();
    // The weapons are addressed through a cue that is replayed on each of them at the end of
    // the tick (`WeaponCue`); `wrig` answers whether any weapon follows a body slot.
    let wrig = WeaponRigs::of(&s.weapons, &anims);
    let Ok(mut body) = anims.get_mut(s.body) else { return };
    let rig = body.rig.clone();
    let mut weapon = WeaponCue::default();
    let pos_m = s.motor.pos * SCALE;
    let dir = pad.dir;
    let attack = Pad::take(&mut pad.attack);
    let jump = Pad::take(&mut pad.jump);
    let square = Pad::take(&mut pad.square);
    let lock_pressed = Pad::take(&mut pad.lock);
    s.invuln = (s.invuln - 1.0).max(0.0);
    s.timer += 1.0;
    fighter.hittable = s.invuln <= 0.0 && !matches!(s.state, State::Down | State::Viewer | State::FormChange);
    // Set by the attack and guard starts; applied to the battle flag at the end of the tick.
    let keep_out = std::cell::Cell::new(false);
    let prty = &data.prty;
    let move_speed = if pad.stick <= 0.0 { 0.0 } else if pad.stick < WALK_STICK { prty.walk_speed } else { prty.run_speed };

    // Targets: lock-on (sstm LockOnRange), auto-target (plyr AutoLockOnRange).
    let alive: Vec<(Entity, Vec3)> = enemies.iter().filter(|(_, _, e)| e.alive()).map(|(e, g, _)| (e, g.translation())).collect();
    let lock_range = data.sstm.lock_on_range * SCALE;
    let nearest = |range: f32| {
        alive.iter().filter(|(_, p)| p.distance(pos_m) <= range).min_by(|a, b| a.1.distance(pos_m).total_cmp(&b.1.distance(pos_m))).map(|x| x.0)
    };
    if lock_pressed {
        s.lock = if s.lock.is_some() { None } else { nearest(lock_range) };
    }
    if let Some(l) = s.lock
        && !alive.iter().any(|(e, p)| *e == l && p.distance(pos_m) <= data.sstm.lock_on_release_range * SCALE)
    {
        s.lock = None;
    }
    s.target = s.lock.or_else(|| auto_target(&data.plyr, &s.motor, &alive));
    let target_pos = s.target.and_then(|t| alive.iter().find(|x| x.0 == t).map(|x| x.1 / SCALE));

    // Battle mode (keyblade out) while enemies are near or after attacking.
    if nearest(lock_range).is_some() {
        battle.ticks = BATTLE_TICKS;
    }
    battle.ticks = (battle.ticks - 1.0).max(0.0);
    let battle_on = battle.active();
    let pstate = if battle_on { PlayerState::BattleWeapon } else { PlayerState::FieldNoWeapon };
    for w in &s.weapons {
        if let Ok(mut v) = vis.get_mut(*w) {
            // The change motions are the battle-state ones (the drive code asks for that variant), so the keyblade is out.
            *v = if battle_on || matches!(s.state, State::Viewer | State::FormChange) { Visibility::Inherited } else { Visibility::Hidden };
        }
    }
    let motion = |id: usize| rig.mset.slot(id, pstate);

    // Motion viewer: F1 toggles, [ and ] step through every motion slot.
    if Pad::take(&mut pad.viewer) {
        s.state = if s.state == State::Viewer { State::Ground } else { State::Viewer };
    }
    // `KH2_VIEW_SLOT=<slot>` opens the viewer on a slot; `KH2_VIEW_AFTER=<seconds>` delays that, so a
    // screenshot run can catch the motion's effects after the renderer has warmed up.
    let view_after: f32 = std::env::var("KH2_VIEW_AFTER").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    if s.state != State::Viewer
        && s.timer >= view_after * crate::data::LOGIC_HZ
        && let Some(want) = std::env::var("KH2_VIEW_SLOT").ok().and_then(|v| v.parse::<usize>().ok())
    {
        s.state = State::Viewer;
        s.viewer_slot = (0..rig.mset.slots.len()).filter(|&i| rig.mset.has(i)).position(|i| i == want).unwrap_or(0);
    }
    if s.state == State::Viewer {
        let slots: Vec<usize> = (0..rig.mset.slots.len()).filter(|&i| rig.mset.has(i)).collect();
        if Pad::take(&mut pad.next) {
            s.viewer_slot = (s.viewer_slot + 1) % slots.len();
        }
        if Pad::take(&mut pad.prev) {
            s.viewer_slot = (s.viewer_slot + slots.len() - 1) % slots.len();
        }
        let slot = slots.get(s.viewer_slot).copied();
        body.play(slot, true, BLEND_LOCOMOTION);
        weapon.play(slot.filter(|&x| wrig.mset.has(x)), true, BLEND_LOCOMOTION);
        place(&mut tf, s, &body, &mut bodies);
        weapon.apply(&s.weapons, &mut anims);
        return;
    }

    // Starts the table action `a` (FUN_00176050).
    let start_attack = |s: &mut Sora, body: &mut Animator, weapon: &mut WeaponCue, a: &Action, air: bool| -> bool {
        let finisher = a.finisher();
        let v0 = if finisher {
            data.plyr.attack_finish_v0
        } else if s.hits <= 1 {
            data.plyr.attack_first_v0
        } else {
            data.plyr.attack_combo_v0
        };
        let Some(slot) = rig.mset.slot(a.motion as usize, pstate) else { return false };
        body.restart(Some(slot), false, BLEND_ATTACK);
        weapon.restart(Some(slot).filter(|&x| wrig.mset.has(x)), false, BLEND_ATTACK);
        info!("sora attack {} (key {}, motion id {}, air {air}, hit {}, finisher {finisher})", body.tag(), a.key, a.motion, s.hits);
        s.state = State::Attack { air, finisher };
        s.timer = 0.0;
        s.queued_attack = false;
        s.deny_logged = false;
        // Starting an action clears the hit-landed latch.
        s.connected = false;
        keep_out.set(true);
        s.motor.vel = Vec3::ZERO;
        // Face the target; close distance beyond the swing's own root motion at plyr V0.
        s.lunge_left = 0.0;
        if let Some(tp) = target_pos {
            let to = (tp - s.motor.pos).with_y(0.0);
            s.motor.yaw = to.x.atan2(to.z);
            s.lunge_left = (to.length() - data.plyr.attack_radius.max(100.0)).max(0.0);
            s.lunge_speed = v0;
        } else if dir != Vec3::ZERO {
            s.motor.yaw = dir.x.atan2(dir.z);
        }
        true
    };

    // The Square press's explicit actions (FUN_00174cf8 calls FUN_00177ea0 through vtable +0x80
    // when the command carries flag 8, the Square button). FUN_00177ea0, in this order: nothing
    // while the mover has flag 0x800; the counters restart in an idle range (FUN_00176a70);
    // FUN_001783d8 (mover 0x16, only when actor +0xb50 is 6: not identified, not ported);
    // FUN_00178290 (Glide, growth slot 3: the caller's); then the explicit actions through
    // FUN_00177fa8, keys 37, 12, 38; then FUN_00178108 (Quick Run, growth slot 1) and FUN_00178080
    // (guard, key 11), both the caller's. Each explicit action needs its record available and an
    // allow-combo or finisher window, then passes the press counters (FUN_00176b00) and starts.
    // True when one started. The counter restart is kept only when an action starts (the app's
    // counters restart on every later press from an idle range anyway).
    let square_explicit = |s: &mut Sora, body: &mut Animator, weapon: &mut WeaponCue, air: bool, w: Windows, fin_window: bool| -> bool {
        let log = move_log();
        let mut chain = abilities.chain(s.hits, s.finishers, air, fin_window);
        if w.free {
            chain.restart();
        }
        let mut lines: Vec<String> = Vec::new();
        let mut chosen = None;
        for k in key::EXPLICIT {
            match s.moves.explicit(k, &chain, w) {
                Ok(a) => {
                    let mut c = chain;
                    if let Some(p) = c.press(w, s.connected) {
                        chain = c;
                        chosen = Some((a.clone(), p));
                        break;
                    }
                    lines.push(format!("explicit key {k}: available, but the press is refused"));
                }
                Err(m) => lines.push(format!("explicit key {k}: {}", explicit_text(m))),
            }
        }
        if log {
            let playing = body.slot.map_or(usize::MAX, |sl| sl / 4);
            info!(
                "move: square press: {} at motion {playing}, windows free {} combo {} finisher {fin_window}, hit landed {}",
                if air { "airborne" } else { "grounded" },
                w.free,
                w.combo,
                s.connected
            );
            for l in &lines {
                info!("move:   {l}");
            }
        }
        let Some((a, p)) = chosen else { return false };
        if log {
            let tag = rig.mset.slot(a.motion as usize, pstate).and_then(|sl| rig.mset.get(sl)).map_or("-", |x| x.tag.as_str());
            info!("move:   {p:?} (combo {}, finishers {}): chosen key {} motion {} ({tag}) by explicit key (Square)", chain.combo, chain.finishers, a.key, a.motion);
        }
        s.hits = chain.combo;
        s.finishers = chain.finishers;
        start_attack(s, body, weapon, &a, air)
    };

    // The attack button (FUN_00174cf8, command type 1): the combo counters of FUN_00176b00, then
    // the table selection of FUN_00177d58. False when the press is refused or the table offers
    // nothing. `w` holds the playing motion's input windows and `fin_window` its finisher window.
    let target_point = s.target.and_then(|t| enemy_points.iter().find(|(e, _)| *e == t)).and_then(|(_, p)| p.first().copied());
    let all_points: Vec<TargetPoint> = enemy_points.iter().flat_map(|(_, p)| p.iter().copied()).collect();
    let request = |s: &mut Sora, body: &mut Animator, weapon: &mut WeaponCue, air: bool, w: Windows, fin_window: bool| -> bool {
        let log = move_log();
        let playing = body.slot.map_or(usize::MAX, |sl| sl / 4);
        let tag_of = |id: u16| rig.mset.slot(id as usize, pstate).and_then(|sl| rig.mset.get(sl)).map_or("-", |x| x.tag.as_str());
        let mut chain = abilities.chain(s.hits, s.finishers, air, fin_window);
        // KEPT FROM THE EARLIER PORT (not the game's attack path): the idle-range restart belongs to
        // FUN_00177ea0, which runs on the Square flag only; FUN_00174cf8's attack path calls
        // FUN_00176b00 alone. It only matters when an idle range and the allow-combo range are open
        // together with a landed hit; kept so the ground combo and air chain stay as they were.
        if w.free {
            chain.restart();
        }
        let press = chain.press(w, s.connected);
        if press.is_none() {
            if log && !s.deny_logged {
                info!(
                    "move: attack press refused: motion {playing} ({}), windows free {} combo {} finisher {fin_window}, hit landed {}: no chain without a landed hit in the combo window, and no restart while the motion plays",
                    tag_of(playing as u16),
                    w.free,
                    w.combo,
                    s.connected
                );
                s.deny_logged = true;
            }
            return false;
        }
        // The ledge probe runs inside the selection (FUN_001cbcf8), once per press.
        let ledge = ledge_probe(&world, &s.motor, air, body_shape(&rig), reference_direction(&s.motor, target_point), data.plyr.attack_j_front);
        let scene = selection_scene(&s.motor, target_point, &all_points, jump_height(&data, &growth), ledge.as_ref().is_some_and(|l| l.flagged));
        let (evals, idx) = s.moves.select(&chain, &scene, playing);
        let chosen = idx.map(|i| s.moves.actions[i].clone());
        if log {
            info!(
                "move: attack press: {} (combo {}, finishers {}), {} at motion {playing} ({}), windows free {} combo {} finisher {fin_window}, hit landed {}",
                press.map_or("-".to_string(), |p| format!("{p:?}")),
                chain.combo,
                chain.finishers,
                if air { "airborne" } else { "grounded" },
                tag_of(playing as u16),
                w.free,
                w.combo,
                s.connected
            );
            info!(
                "move:   target {}; {} enemy point(s) {}",
                scene.target.map_or("none".to_string(), |p| probe_text(&p)),
                scene.others.len(),
                scene.others.iter().map(probe_text).collect::<Vec<_>>().join(" | ")
            );
            info!(
                "move:   at ({:.0} {:.0} {:.0}) yaw {:.2}; jump height {:.1} (flag-bit-1 reach); ledge probe {}",
                s.motor.pos.x,
                s.motor.pos.y,
                s.motor.pos.z,
                s.motor.yaw,
                scene.jump_height,
                ledge.as_ref().map_or("not run (airborne or no body shape)".to_string(), ledge_text),
            );
            for e in &evals {
                let a = &s.moves.actions[e.index];
                info!("move:   #{} key {} motion {} ({}) group {} -> {}", e.index, a.key, a.motion, tag_of(a.motion), a.group, verdict_text(&e.verdict));
            }
        }
        let Some(a) = chosen else {
            if log {
                info!("move:   the table offers no attack");
            }
            return false;
        };
        if log {
            info!("move:   chosen key {} motion {} ({}) by table", a.key, a.motion, tag_of(a.motion));
        }
        s.hits = chain.combo;
        s.finishers = chain.finishers;
        start_attack(s, body, weapon, &a, air)
    };

    let start_jump = |s: &mut Sora, body: &mut Animator| {
        let high = data.fmab(growth.high_jump);
        let h = high.map_or(prty.jump_height, |f| f.high_jump_height);
        s.motor.vel = dir * move_speed;
        s.motor.start_jump(h);
        s.aerial_dodge_ready = true;
        s.state = State::Air;
        s.timer = 0.0;
        body.restart(if high.is_some() { motion(mid::HIGH_JUMP).or(motion(id::JUMP)) } else { motion(id::JUMP) }, false, 0.0);
    };

    // Vertical motion this tick: None = follow the floor.
    let mut airborne_dy: Option<f32> = None;
    let fall_max = data.sstm.fall_max;
    match s.state {
        State::Ground => {
            s.hits = 0;
            s.finishers = 1;
            s.motor.ground_velocity(dir, move_speed);
            if move_speed > 0.0 {
                s.motor.turn_to(dir, prty.turn_speed);
            }
            let landing = body.slot == motion(id::LAND) && !body.finished;
            if move_speed > 0.0 || !landing {
                let id = if move_speed <= 0.0 { id::IDLE } else if move_speed < prty.run_speed { id::WALK } else { id::RUN };
                body.play(motion(id), true, BLEND_LOCOMOTION);
            }
            weapon.play(None, true, BLEND_LOCOMOTION);
            if jump {
                start_jump(s, &mut body);
            } else if attack {
                request(s, &mut body, &mut weapon, false, Windows { free: true, combo: false }, false);
            } else if square {
                // FUN_00177ea0's order: the explicit actions first (they need a combo or finisher
                // window, so from idle they never start), then Quick Run, then the guard.
                if square_explicit(s, &mut body, &mut weapon, false, Windows { free: true, combo: false }, false) {
                    // An explicit action started.
                } else if let Some(f) = data.fmab(growth.quick_run).filter(|_| move_speed > 0.0) {
                    // Quick Run: dash along the input at AirSlideSpeed while the button is held.
                    s.state = State::QuickRun;
                    s.timer = 0.0;
                    s.motor.yaw = dir.x.atan2(dir.z);
                    s.motor.vel = dir * f.air_slide_speed;
                    s.dash_time = f.air_slide_time;
                    s.dash_brake = f.air_slide_brake;
                    s.invuln = f.air_slide_star_time;
                    body.restart(motion(mid::QUICK_RUN), false, 0.0);
                } else if let Some(g) = s.moves.by_key(key::GUARD).filter(|g| abilities.chain(s.hits, s.finishers, false, false).check(g).is_ok()).cloned()
                    && let Some(slot) = rig.mset.slot(g.motion as usize, pstate)
                {
                    // The guard action is a ground action of the attack table.
                    s.state = State::Guard;
                    s.timer = 0.0;
                    body.restart(Some(slot), false, BLEND_ATTACK);
                    weapon.restart(Some(slot).filter(|&x| wrig.mset.has(x)), false, BLEND_ATTACK);
                    info!("sora guard {} (key {}, motion id {})", body.tag(), g.key, g.motion);
                    s.motor.vel = Vec3::ZERO;
                    keep_out.set(true);
                }
            }
        }
        State::Air => {
            // Free fall counts as an idle motion: a new air chain may start.
            s.hits = 0;
            s.finishers = 1;
            s.motor.air_velocity(dir, move_speed);
            if move_speed > 0.0 {
                s.motor.turn_to(dir, prty.turn_speed);
            }
            if s.motor.jump_control(pad.jump_held) {
                body.play(motion(id::FALL), true, BLEND_FALL);
            }
            let can_air_act = s.motor.jump.t > s.motor.jump.rise.max(0.0) + 6.0 || (s.motor.jump.rise == 0.0 && s.motor.jump.t > 15.0);
            if jump && let Some(f) = data.fmab(growth.aerial_dodge).filter(|_| s.aerial_dodge_ready || growth.aerial_dodge >= 5) {
                // Aerial Dodge: a second jump of AirDodgeHeight, moving at AirDodgeSpeed.
                s.aerial_dodge_ready = false;
                s.motor.vel = if move_speed > 0.0 { dir * f.air_dodge_speed } else { s.motor.vel };
                s.motor.start_jump(f.air_dodge_height);
                body.restart(motion(mid::AERIAL_DODGE), false, 0.0);
            } else if square
                && let Some(f) = data.fmab(growth.glide)
                && can_air_act
                && height_above_ground(&world, &s.motor) > f.glide_start_height
            {
                s.state = State::Glide;
                s.timer = 0.0;
                body.play(motion(if move_speed > 0.0 { mid::GLIDE } else { mid::GLIDE_HOVER }), true, BLEND_GLIDE);
            } else if square && square_explicit(s, &mut body, &mut weapon, true, Windows { free: true, combo: false }, false) {
                // After Glide, the explicit actions (FUN_00177ea0); none starts without a window.
            } else if attack {
                request(s, &mut body, &mut weapon, true, Windows { free: true, combo: false }, false);
            }
            // No air guard: the guard action is ground-only in the attack table (A350 is the air
            // recoil motion).
            if s.state == State::Air {
                airborne_dy = Some(s.motor.jump_dy(fall_max));
            }
        }
        State::QuickRun => {
            if pad.square_held {
                s.dash_time -= 1.0;
            } else {
                s.dash_time = 0.0;
            }
            if s.dash_time <= 0.0 && body.slot != motion(mid::QUICK_RUN_END) {
                let f = data.fmab(growth.quick_run.max(1)).unwrap();
                s.dash_brake = f.air_slide_stop_brake;
                body.restart(motion(mid::QUICK_RUN_END), false, 0.0);
            }
            s.motor.vel *= s.dash_brake;
            if body.slot == motion(mid::QUICK_RUN_END) && (body.finished || s.motor.vel.length() < prty.walk_speed) {
                s.state = State::Ground;
            }
        }
        State::Glide => {
            let f = data.fmab(growth.glide.max(1)).unwrap();
            let h = height_above_ground(&world, &s.motor);
            if !pad.square_held || h < f.glide_end_height {
                s.state = State::Air;
                s.motor.start_fall();
                body.play(motion(id::FALL), true, BLEND_GLIDE);
            } else {
                if move_speed > 0.0 {
                    // Glide: speed eases toward GlideSpeed, along facing, turning at GlideTurnSpeed.
                    s.motor.turn_to(dir, f.glide_turn_speed);
                    let speed = s.motor.vel.length() * f.glide_acceleration + f.glide_speed * (1.0 - f.glide_acceleration);
                    s.motor.vel = s.motor.forward() * speed;
                    let facing = s.motor.forward();
                    let id = if facing.dot(dir) < 0.98 { if facing.cross(dir).y > 0.0 { mid::GLIDE_LEFT } else { mid::GLIDE_RIGHT } } else { mid::GLIDE };
                    body.play(motion(id), true, BLEND_GLIDE);
                } else {
                    s.motor.vel *= 0.9;
                    body.play(motion(mid::GLIDE_HOVER), true, BLEND_GLIDE);
                }
                let sink = ((h - f.glide_fall_height).max(0.0) * f.glide_fall_ratio).min(f.glide_fall_max);
                airborne_dy = Some(-sink);
            }
        }
        State::Attack { air, .. } => {
            if attack {
                s.queued_attack = true;
            }
            let combo_window = body.trigger(range::ALLOW_COMBO);
            let finisher_window = body.trigger(range::ALLOW_FINISHER);
            let window = combo_window || finisher_window;
            // Close the gap to the target at plyr V0, then let the motion's root motion carry.
            let step = s.lunge_speed.min(s.lunge_left);
            s.lunge_left -= step;
            s.motor.vel = s.motor.forward() * step;
            if let Some(tp) = target_pos
                && (body.trigger(range::TURN_TO_LOCK) || body.trigger(range::TURN_TO_TARGET))
            {
                s.motor.turn_to(tp - s.motor.pos, prty.turn_speed);
            }
            let hover = air || body.trigger(range::NO_GRAVITY) || body.trigger(range::NO_GRAVITY_KEEP_KINETICS);
            if !s.motor.grounded {
                airborne_dy = Some(if hover { 0.0 } else { s.motor.jump_dy(fall_max) });
                if hover {
                    s.motor.jump = Jump::fall(0.0, s.motor.pos.y);
                }
            }
            // A pressed attack is tried every tick while a window of the motion is open (the game
            // keeps the pending command and retries it); the table decides air or ground by Sora's
            // state at this moment.
            let air_now = !s.motor.grounded || body.trigger(range::NO_GRAVITY);
            let windows = Windows { free: body.trigger(range::GROUNDED) || body.trigger(range::AIR) || body.trigger(range::BLEND_TO_IDLE), combo: combo_window };
            // A Square press is tried on its own tick (the command flag comes from the pad's
            // newly-pressed word, FUN_001670d0 / FUN_001babf0) and before the pending attack, as
            // FUN_00174cf8 runs FUN_00177ea0 first. Quick Run, Glide and the guard from an idle
            // range of an attack motion are not ported.
            if square && square_explicit(s, &mut body, &mut weapon, air_now, windows, finisher_window) {
                // An explicit action started.
            } else if s.queued_attack && (windows.free || windows.combo || finisher_window) && request(s, &mut body, &mut weapon, air_now, windows, finisher_window) {
                // Chained into the next attack.
            } else if jump && window && s.motor.grounded {
                start_jump(s, &mut body);
            } else if body.finished || (body.trigger(range::BLEND_TO_IDLE) && dir != Vec3::ZERO) {
                s.state = if s.motor.grounded { State::Ground } else { State::Air };
                if s.state == State::Air {
                    s.motor.start_fall();
                    body.play(motion(id::FALL), true, BLEND_FALL);
                }
            }
        }
        State::Guard | State::Counter => {
            s.motor.vel = Vec3::ZERO;
            if !s.motor.grounded {
                airborne_dy = Some(s.motor.jump_dy(fall_max));
            }
            // The guard motion lets Sora out when its blend-to-idle range opens; an attack pressed
            // after a guard hit answers with the counter action of the attack table (it needs its
            // ability), as the game does while the guard-hit motion plays.
            let counter = (s.state == State::Counter && attack)
                .then(|| s.moves.by_key(key::COUNTER).filter(|c| abilities.chain(s.hits, s.finishers, false, false).check(c).is_ok()).cloned())
                .flatten()
                .and_then(|c| rig.mset.slot(c.motion as usize, pstate).map(|slot| (c, slot)));
            if let Some((c, slot)) = counter {
                body.restart(Some(slot), false, BLEND_ATTACK);
                weapon.restart(Some(slot).filter(|&x| wrig.mset.has(x)), false, BLEND_ATTACK);
                info!("sora counter {} (key {}, motion id {})", body.tag(), c.key, c.motion);
                s.state = State::Attack { air: false, finisher: false };
                s.timer = 0.0;
                s.hits = 1;
                s.finishers = 1;
                s.queued_attack = false;
                s.connected = false;
            } else if body.finished || (body.trigger(range::BLEND_TO_IDLE) && (dir != Vec3::ZERO || attack)) {
                s.state = if s.motor.grounded { State::Ground } else { State::Air };
                if attack && s.motor.grounded {
                    request(s, &mut body, &mut weapon, false, Windows { free: true, combo: false }, false);
                }
            }
        }
        State::Hurt => {
            s.motor.vel *= 0.85;
            if !s.motor.grounded {
                airborne_dy = Some(s.motor.jump_dy(fall_max));
            }
            if body.finished {
                s.state = if s.motor.grounded { State::Ground } else { State::Air };
            }
        }
        State::Blown => {
            // A press while blown (FUN_001758b0, run before everything else on every command
            // tick): Aerial Recovery on the jump flag, Retaliating Slash (key 40) on the Square
            // flag, once plyr BlowRecoveryTime ticks have passed. The attack button does nothing.
            if (jump || square) && s.blow.is_some() {
                blown_press(s, &mut body, &mut weapon, &abilities, &data, jump, square, &rig, pstate, &start_attack);
            }
            // The flight of a blow (FUN_0019e4d8): horizontal speed brakes down to a floor, the
            // rise brakes, gravity pulls. Without the reaction data the jump model stands in.
            if let (Some(b), Some(r)) = (s.blow.as_mut(), reaction.as_ref()) {
                let (h, vy) = b.step(&r.defaults, &r.consts);
                s.motor.vel = b.dir * h;
                airborne_dy = Some(vy);
            } else {
                airborne_dy = (!s.motor.grounded).then(|| s.motor.jump_dy(fall_max));
            }
            if s.motor.grounded && s.timer > 2.0 {
                s.blow = None;
                s.motor.vel = Vec3::ZERO;
                if s.hp <= 0 {
                    s.state = State::Down;
                    s.timer = 0.0;
                } else {
                    s.state = State::Hurt;
                    body.restart(motion(if s.blown_back { id::DAMAGE_BLOW_BACK_LAND } else { id::DAMAGE_BLOW_FRONT_LAND }), false, 3.0);
                }
            }
        }
        State::Down => {
            s.motor.vel = Vec3::ZERO;
            if s.timer > 180.0 {
                // KH2 shows the game-over screen here; kh2-rs revives in place.
                s.hp = s.stats.max_hp;
                s.invuln = 120.0;
                s.state = State::Hurt;
                body.restart(motion(id::DAMAGE_BLOW_RECOV), false, 6.0);
            }
        }
        State::Viewer => {}
        State::FormChange => {
            // No input: the drive code plays the change motions and ends the state. Gravity goes on.
            s.motor.vel = Vec3::ZERO;
            if !s.motor.grounded {
                airborne_dy = Some(s.motor.jump_dy(fall_max));
            }
        }
        State::Reaction => {
            // No input: `player_reaction.rs` set the velocity for this tick and ends the state.
            if !s.motor.grounded {
                airborne_dy = Some(s.motor.jump_dy(fall_max));
            }
        }
        State::Magic => {
            // The cast motion (`magic.rs`): gravity unless the motion suspends it; control comes back
            // when it ends or its blend-to-idle range opens with a press or a direction.
            let hover = body.trigger(range::NO_GRAVITY) || body.trigger(range::NO_GRAVITY_KEEP_KINETICS);
            if !s.motor.grounded {
                airborne_dy = Some(if hover { 0.0 } else { s.motor.jump_dy(fall_max) });
                if hover {
                    s.motor.jump = Jump::fall(0.0, s.motor.pos.y);
                }
            }
            if body.finished || (body.trigger(range::BLEND_TO_IDLE) && (dir != Vec3::ZERO || attack || jump)) {
                s.state = if s.motor.grounded { State::Ground } else { State::Air };
                if s.state == State::Air {
                    s.motor.start_fall();
                    body.play(motion(id::FALL), true, BLEND_FALL);
                } else if attack {
                    request(s, &mut body, &mut weapon, false, Windows { free: true, combo: false }, false);
                }
            }
        }
    }
    if keep_out.get() {
        battle.ticks = BATTLE_TICKS;
    }

    // Root motion: yaw turns the facing; translation is rotated by the facing the motion
    // started from.
    let (root_t, root_yaw) = body.root_delta;
    s.motor.yaw = wrap_angle(s.motor.yaw + root_yaw);
    let start_yaw = s.motor.yaw - body.root.yaw;
    let root_move = Quat::from_rotation_y(start_yaw) * root_t;
    let mut delta = s.motor.vel + root_move.with_y(0.0);
    if let (Some(p), Some(r)) = (s.push.as_mut(), reaction.as_ref()) {
        delta += p.step(&r.consts);
        if p.finished() {
            s.push = None;
        }
    }
    let dy = airborne_dy.map(|d| d + root_move.y);
    let was_air = airborne_dy.is_some();
    let landed = s.motor.move_by(&world, delta, dy);
    if landed && was_air {
        land(s, &mut body, motion(id::LAND), dir != Vec3::ZERO);
    } else if !s.motor.grounded && s.state == State::Ground {
        // Walked off a ledge.
        s.state = State::Air;
        s.motor.start_fall();
        body.play(motion(id::FALL), true, BLEND_FALL);
    }
    if s.motor.pos.y < world.min.y - 5000.0 || !s.motor.pos.is_finite() {
        s.motor = Motor { pos: world.spawn_point(), ..Motor::default() };
    }
    place(&mut tf, s, &body, &mut bodies);
    if std::env::var("KH2_TRACE").is_ok() {
        // Pelvis (bone 0) in world units vs the character position: root motion check.
        let pelvis = body.pose.first().map_or(Vec3::ZERO, |m| m.w_axis.truncate());
        let world = s.motor.pos + Quat::from_rotation_y(s.motor.yaw) * pelvis;
        info!(
            "trace {:?} {} f{:.1} pos ({:.0} {:.0} {:.0}) yaw {:.2} pelvis ({:.0} {:.0} {:.0}) root ({:.0} {:.2})",
            s.state, body.tag(), body.frame, s.motor.pos.x, s.motor.pos.y, s.motor.pos.z, s.motor.yaw, world.x, world.y, world.z, body.root.translation.z, body.root.yaw
        );
    }
    weapon.apply(&s.weapons, &mut anims);
}

/// The game's automatic target choice: among targets within AutoLockOnRange and the height
/// window (plyr heights are Y-down: MinH above, MaxH below), the lowest
/// (distance + |height| × HAdjust) × ((1 − facing·direction) / 2 × (1 − Inner) + Inner).
fn auto_target(p: &kh2_tables::tables::PrefPlyr, m: &Motor, alive: &[(Entity, Vec3)]) -> Option<Entity> {
    let facing = m.forward();
    alive
        .iter()
        .filter_map(|&(e, at_m)| {
            let at = at_m / SCALE;
            let rel = at - m.pos;
            let up = rel.y;
            if up > -p.auto_lock_on_min_h || up < -p.auto_lock_on_max_h {
                return None;
            }
            let flat = rel.with_y(0.0);
            let dist = flat.length();
            if dist > p.auto_lock_on_range {
                return None;
            }
            let dir = flat.normalize_or_zero();
            let inner = p.auto_lock_on_inner_adjust;
            let score = (dist + up.abs() * p.auto_lock_on_h_adjust) * ((1.0 - facing.dot(dir)) * 0.5 * (1.0 - inner) + inner);
            Some((e, score))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|x| x.0)
}

fn land(s: &mut Sora, body: &mut Animator, land_slot: Option<usize>, moving: bool) {
    s.hits = 0;
    s.finishers = 1;
    match s.state {
        State::Air | State::Glide | State::Blown => {
            if s.state != State::Blown {
                s.state = State::Ground;
                if !moving {
                    body.restart(land_slot, false, 0.0);
                }
            }
        }
        _ => {}
    }
}

/// Writes Sora's transform (poses are root-relative, so the body needs no offset).
fn place(tf: &mut Transform, s: &Sora, _body: &Animator, _bodies: &mut Query<&mut Transform, Without<Sora>>) {
    tf.translation = s.motor.pos * SCALE;
    tf.rotation = Quat::from_rotation_y(s.motor.yaw);
}

/// Puts Sora on an area's spawn point.
pub fn respawn(s: &mut Sora, at: Vec3) {
    s.motor = Motor { pos: at, ..Motor::default() };
    s.state = State::Ground;
}

fn probe_text(p: &Probe) -> String {
    format!("(dist {:.1}, dy {:.1}, dot {:.3}, half height {:.1})", p.dist, p.dy, p.dot, p.half_height)
}

fn reject_text(r: moveset::Reject) -> String {
    match r {
        moveset::Reject::NoMotion => "no motion".into(),
        moveset::Reject::Ability(b) => format!("ability bit {b} not set"),
        moveset::Reject::State => "wrong airborne state".into(),
        moveset::Reject::FinisherNotReady => "finisher, but the combo is not long enough and no finisher window is open".into(),
        moveset::Reject::FinisherDone => "normal attack, but the finishers of the chain are used up".into(),
    }
}

fn verdict_text(v: &Verdict) -> String {
    match v {
        Verdict::ByKeyOnly => "group 0: only chosen by key".into(),
        Verdict::Rejected(r) => format!("not available: {}", reject_text(*r)),
        Verdict::NoTarget => "no point to test".into(),
        Verdict::Outside(m) => format!("outside the {} window", match m {
            moveset::Miss::Distance => "distance",
            moveset::Miss::Height => "height",
            moveset::Miss::Angle => "angle",
        }),
        Verdict::Flagged => "FLAGGED".into(),
    }
}

fn explicit_text(m: ExplicitMiss) -> String {
    match m {
        ExplicitMiss::NoRecord => "no such record".into(),
        ExplicitMiss::Rejected(r) => format!("not available: {}", reject_text(r)),
        ExplicitMiss::FinisherPossible => "a normal attack while a finisher is possible".into(),
        ExplicitMiss::NoWindow => "no allow-combo or finisher window open".into(),
    }
}

/// A hit of Sora's swing landed on something: the game sets a latch (FUN_0018c6d0) that the next
/// attack press needs to continue the chain.
pub fn track_connects(mut hits: MessageReader<Hit>, mut walls: MessageReader<WallHit>, mut sora_q: Query<(Entity, &mut Sora)>) {
    let Ok((me, mut s)) = sora_q.single_mut() else { return };
    if hits.read().any(|h| h.attacker == me) | walls.read().any(|w| w.attacker == me) {
        s.connected = true;
    }
}

/// Sora's attack volume touching a wall: the generic recoil of the actor class (FUN_00175668, then
/// FUN_00185d30). It runs for rows with BG Hit (the wall message only exists for those), whatever
/// the row's `RefactSelf` says: motion 41 (A320) or, in the air, 43 (A350), the combo counter goes
/// down by one, and Sora is pushed backwards along his own facing. The spark and the sound are
/// started by `fx::hit_sparks` and `audio::wall_sounds` from the same message.
pub fn receive_wall_hits(
    mut walls: MessageReader<WallHit>,
    data: Res<GameData>,
    reaction: Option<Res<ReactionData>>,
    mut sora_q: Query<(Entity, &mut Sora, &mut Fighter)>,
    mut anims: Query<&mut Animator>,
) {
    let Ok((me, mut s, mut fighter)) = sora_q.single_mut() else { return };
    let Some(reaction) = reaction else { return };
    for w in walls.read().filter(|w| w.attacker == me) {
        let Some(row) = data.atkp(w.atkp) else { continue };
        if !matches!(s.state, State::Attack { .. }) {
            info!("wall recoil: atkp {} touched a wall while Sora is {:?}: no recoil", w.atkp, s.state);
            continue;
        }
        let airborne = !s.motor.grounded;
        let Some(rc) = rules::wall_recoil(&ReflectFields::from_atkp(row), false, airborne, &reaction.consts) else {
            info!("wall recoil: atkp {} has no BG Hit: no recoil", w.atkp);
            continue;
        };
        let Ok(mut body) = anims.get_mut(s.body) else { return };
        let slot = body.rig.mset.slot(rc.motion as usize, PlayerState::BattleWeapon);
        // The blend time of the game's motion request was not read; 2 ticks as for the other reactions.
        body.restart(slot, false, 2.0);
        let mut weapon = WeaponCue::default();
        weapon.restart(slot, false, 2.0);
        // Ground: FUN_001857b0. In the air the game calls FUN_00185978 instead (a jump-back push
        // scaled by HitBackToJump, not decoded); the ground push stands in for it.
        s.push = Push::new(-s.motor.forward(), rc.push_power, 0.0, &reaction.consts);
        let hits_before = s.hits;
        if rc.refund_combo_step {
            s.hits = s.hits.saturating_sub(1);
        }
        info!(
            "wall recoil: atkp {} (BG Hit, {}) -> motion id {} slot {} {}, push power {} speed {:.1}, combo {} -> {}",
            w.atkp,
            if w.weapon_volume { "keyblade volume" } else { "body volume" },
            rc.motion,
            slot.map_or(-1, |x| x as i32),
            body.tag(),
            rc.push_power,
            s.push.map_or(0.0, |p| p.speed),
            hits_before,
            s.hits
        );
        weapon.apply(&s.weapons, &mut anims);
        s.state = State::Hurt;
        s.motor.vel = Vec3::ZERO;
        s.finishers = 1;
        s.queued_attack = false;
        s.timer = 0.0;
        fighter.landed.clear();
    }
}

/// Applies enemy hits to Sora: guard repels from the front, otherwise the victim reaction of the
/// attack row (`reaction_rules::victim_outcome`) after damage.
pub fn receive_hits(
    mut hits: MessageReader<Hit>,
    data: Res<GameData>,
    reaction: Option<Res<ReactionData>>,
    enemies: Query<&Enemy>,
    mut sora_q: Query<(Entity, &mut Sora, &mut Fighter, Option<&ReactionStatus>)>,
    mut anims: Query<&mut Animator>,
    mut ineffective: MessageWriter<IneffectiveHit>,
) {
    let Ok((me, mut s, mut fighter, status)) = sora_q.single_mut() else { return };
    let Some(reaction) = reaction else { return };
    for h in hits.read().filter(|h| h.target == me) {
        if s.invuln > 0.0 || matches!(s.state, State::Down | State::Viewer | State::FormChange) {
            continue;
        }
        let Ok(mut body) = anims.get_mut(s.body) else { return };
        let mut weapon = WeaponCue::default();
        let rig = body.rig.clone();
        let facing = s.motor.forward();
        let from_front = facing.dot(-h.dir) > 0.0;
        // Guard: repelled while the guard motion has not reached its blend-to-idle range. The
        // guard-hit motion is the auto-guard one (A321); no game routine for the block itself was found.
        if s.state == State::Guard && !body.trigger(range::BLEND_TO_IDLE) && from_front {
            let slot = rig.mset.slot(id::AUTOGUARD, PlayerState::BattleWeapon);
            body.restart(slot, false, 2.0);
            weapon.restart(slot, false, 2.0);
            weapon.apply(&s.weapons, &mut anims);
            s.state = State::Counter;
            s.invuln = 18.0;
            continue;
        }
        let Some(atk) = data.atkp(h.atkp) else { continue };
        let from = enemies.get(h.attacker).map(|e| e.stats).unwrap_or(s.stats);
        let useless = crate::combat::hit_is_ineffective(atk, &s.stats);
        let dmg = if useless { data.damage(atk, &from, &s.stats) } else { data.damage(atk, &from, &s.stats).max(1) };
        s.hp = (s.hp - dmg).max(0);
        if useless {
            ineffective.write(IneffectiveHit { attacker: h.attacker, target: me, atkp: h.atkp, point: h.point });
        }
        let ctx = VictimCtx {
            status: status.map_or(0, |x| x.0),
            grounded: s.motor.grounded,
            attacker_flag_4000: false,
            from_front,
            hit_part_plain: true,
        };
        let out = rules::victim_outcome(&rules::HitRow::from_atkp(atk), useless, &ctx, &reaction.patn, &reaction.defaults, &reaction.consts);
        // Dying is not part of the reaction rules: the last hit always launches.
        let d = &reaction.defaults;
        let action = match out.action {
            a @ VictimAction::Blow { .. } => a,
            _ if s.hp == 0 => VictimAction::Blow { motion: rules::blow_motion(from_front), up: d.blow_up, horiz: d.blow_speed, face_attacker: from_front },
            a => a,
        };
        info!(
            "sora hit: atkp {} reaction {} effective {} ineffective {useless} {from_front_str} -> {action:?}",
            h.atkp,
            atk.enemy_reaction,
            out.effective,
            from_front_str = if from_front { "front" } else { "back" },
        );
        let pstate = PlayerState::BattleWeapon;
        let first_slot = |ids: &[usize]| ids.iter().find_map(|&i| rig.mset.slot(i, pstate));
        let away = h.dir.with_y(0.0).normalize_or_zero();
        match action {
            VictimAction::None | VictimAction::NotDecoded { .. } => {}
            VictimAction::Push { strength } => {
                s.push = Push::new(away, strength, 0.0, &reaction.consts);
            }
            VictimAction::Flinch { motion, fallback } | VictimAction::FlinchPush { motion, fallback, .. } => {
                if let VictimAction::FlinchPush { strength, .. } = action {
                    s.push = Push::new(away, strength, 0.0, &reaction.consts);
                }
                body.restart(first_slot(&[motion as usize, fallback as usize]), false, 2.0);
                weapon.restart(None, false, 2.0);
                s.state = State::Hurt;
                s.motor.vel = Vec3::ZERO;
            }
            VictimAction::Class14 { motion, strength, face_attacker } => {
                let to = if face_attacker { -away } else { away };
                s.motor.yaw = to.x.atan2(to.z);
                s.push = Push::new(away, strength, 0.0, &reaction.consts);
                body.restart(first_slot(&[motion as usize, id::DAMAGE_S_FRONT_HIGH]), false, 2.0);
                weapon.restart(None, false, 2.0);
                s.state = State::Hurt;
                s.motor.vel = Vec3::ZERO;
            }
            VictimAction::Blow { motion, up, horiz, face_attacker } => {
                // The victim faces the attacker when hit from the front and is launched along the
                // hit direction (FUN_00185460).
                let to = if face_attacker { -away } else { away };
                s.motor.yaw = to.x.atan2(to.z);
                s.blow = Some(Blow::new(away, up, horiz, d));
                s.push = None;
                s.motor.vel = Vec3::ZERO;
                s.motor.start_fall();
                s.state = State::Blown;
                s.timer = 0.0;
                s.blown_back = !from_front;
                body.restart(rig.mset.slot(motion as usize, pstate), false, 2.0);
                weapon.restart(None, false, 2.0);
            }
        }
        if !matches!(action, VictimAction::None | VictimAction::NotDecoded { .. } | VictimAction::Push { .. }) {
            s.hits = 0;
            s.finishers = 1;
            fighter.landed.clear();
        }
        // The game's invulnerability after a hit was not found; the app keeps its own timer.
        s.invuln = 36.0;
        weapon.apply(&s.weapons, &mut anims);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kh2_anim::moveset::Press;
    use kh2_tables::tables as t;

    /// Sora's real attack table from the user's disc; None (test skipped) without one.
    fn real_moves() -> Option<Moveset> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        let mut a = kh2_archive::Archive::open(&iso).map_err(|e| eprintln!("skipped: {iso}: {e}")).ok()?;
        let index = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()?.rows.into_iter().find(|r| r.model_name == "P_EX100")?.neo_moveset as usize;
        Moveset::parse(&kh2_tables::subfile(&mut a, &t::PTYA).ok()?, index)
    }

    fn chain(air: bool, combo: u8, abilities: &[u16]) -> Chain {
        Chain {
            airborne: air,
            abilities: abilities.iter().fold(1u128 << ABILITY_GUARD, |m, b| m | 1 << b),
            combo,
            finishers: 1,
            max_ground: BASE_HITS_GROUND,
            max_air: BASE_HITS_AIR,
            max_finishers: BASE_FINISHERS,
            finisher_window: false,
        }
    }

    /// A target point at horizontal distance `dist`, `up` above Sora (Y up), dead ahead.
    fn point(dist: f32, up: f32) -> Probe {
        Probe { dist, dy: -up, dot: 1.0, half_height: 45.0 }
    }

    /// Another enemy's point: off the target's direction, so a group 2 record can take it (group 2's
    /// angle window ends just below 1: the target's own direction does not count).
    fn other(dist: f32, up: f32) -> Probe {
        Probe { dot: 0.5, ..point(dist, up) }
    }

    fn pick(m: &Moveset, c: &Chain, scene: &Scene, playing: usize) -> u8 {
        let (_, i) = m.select(c, scene, playing);
        m.actions[i.expect("something is chosen")].key
    }

    fn motion_of(m: &Moveset, key: u8) -> u16 {
        m.by_key(key).unwrap().motion
    }

    #[test]
    fn ground_combo_hits_follow_range_and_finisher() {
        let Some(m) = real_moves() else { return };
        // The table's own geometry: A301 key 4 (group 2, within 180, any enemy not behind), A302 key 6
        // (group 1, target 180 to 300 away), finishers key 2 (A303, group 2) and key 3 (A304, group 3).
        assert_eq!((motion_of(&m, 5), motion_of(&m, 4), motion_of(&m, 6), motion_of(&m, 2), motion_of(&m, 3)), (151, 152, 153, 154, 155));
        let none = Scene::default();
        // First hit from idle: the default attack.
        assert_eq!(pick(&m, &chain(false, 1, &[]), &none, 0), 5);
        // Second hit with no enemy about: A300's alternative, A302.
        assert_eq!(pick(&m, &chain(false, 2, &[]), &none, 151), 6);
        // An enemy within 180 in front: A301.
        let near = Scene { others: vec![other(100.0, 50.0)], ..Scene::default() };
        assert_eq!(pick(&m, &chain(false, 2, &[]), &near, 151), 4);
        // The locked target 180 to 300 away: A302 directly (group 1 comes before key 4 in the table).
        let far = Scene { target: Some(point(220.0, 50.0)), others: vec![point(220.0, 50.0)], ..Scene::default() };
        assert_eq!(pick(&m, &chain(false, 2, &[]), &far, 151), 6);
        // The same target while A301 plays and nothing is close: key 5 (A300) is next in the table order.
        assert_eq!(pick(&m, &chain(false, 2, &[]), &none, 152), 5);
        // Third hit: a finisher. A303 with an enemy in front within 180, else A304.
        assert_eq!(pick(&m, &chain(false, 3, &[]), &near, 152), 2);
        assert_eq!(pick(&m, &chain(false, 3, &[]), &none, 152), 3);
        // An enemy behind Sora (dot below -0.707) does not count for the group 2 windows.
        let behind = Scene { others: vec![Probe { dot: -0.9, ..other(100.0, 50.0) }], ..Scene::default() };
        assert_eq!(pick(&m, &chain(false, 3, &[]), &behind, 152), 3);
        // A longer combo with Combo Plus: the third press is still a normal hit.
        let mut plus = chain(false, 3, &[21]);
        plus.max_ground = 4;
        assert_eq!(pick(&m, &plus, &near, 151), 4);
        // A motion is never picked again while it plays: with A301 playing key 5 (A300) is next.
        assert_eq!(pick(&m, &plus, &near, 152), 5);
    }

    #[test]
    fn height_window_of_a_group_1_record() {
        let Some(m) = real_moves() else { return };
        let a6 = m.by_key(6).unwrap().clone();
        // Heights are Y down: a target point 50 above Sora is dy -50; the window is (-110 - half, -20 + half].
        assert_eq!(a6.window(&point(220.0, 50.0), 0.0), Ok(()));
        assert_eq!(a6.window(&point(220.0, 190.0), 0.0), Err(moveset::Miss::Height));
        assert_eq!(a6.window(&point(220.0, -80.0), 0.0), Err(moveset::Miss::Height));
        assert_eq!(a6.window(&point(150.0, 50.0), 0.0), Err(moveset::Miss::Distance));
        assert_eq!(a6.window(&point(300.0, 50.0), 0.0), Err(moveset::Miss::Distance));
        // The angle window of group 1 only passes a point in the reference direction.
        assert_eq!(a6.window(&Probe { dot: 0.9, ..point(220.0, 50.0) }, 0.0), Err(moveset::Miss::Angle));
    }

    #[test]
    fn air_chain_a330_a331_then_a332_or_a333() {
        let Some(m) = real_moves() else { return };
        assert_eq!((motion_of(&m, 9), motion_of(&m, 10), motion_of(&m, 8), motion_of(&m, 7), motion_of(&m, 29)), (181, 182, 183, 184, 194));
        let none = Scene::default();
        // Press 1 in the air: A330. Press 2 while it plays: its alternative key 10, A331.
        assert_eq!(pick(&m, &chain(true, 1, &[]), &none, 0), 9);
        assert_eq!(pick(&m, &chain(true, 2, &[]), &none, 181), 10);
        // Press 3 is the finisher: A333 (key 7) with an enemy within 260 and not behind, else A332 (key 8).
        let near = Scene { others: vec![other(150.0, 0.0)], ..Scene::default() };
        assert_eq!(pick(&m, &chain(true, 3, &[]), &near, 182), 7);
        assert_eq!(pick(&m, &chain(true, 3, &[]), &none, 182), 8);
        let far = Scene { others: vec![other(400.0, 0.0)], ..Scene::default() };
        assert_eq!(pick(&m, &chain(true, 3, &[]), &far, 182), 8);
        // A340 to A344 are gated on ability bits 97 to 101; Aerial Finish (100, key 29) comes before
        // key 8 in the table but after key 7.
        assert_eq!(pick(&m, &chain(true, 3, &[100]), &none, 182), 29);
        assert_eq!(pick(&m, &chain(true, 3, &[100]), &near, 182), 7);
    }

    #[test]
    fn gated_records_need_their_ability_and_window() {
        let Some(m) = real_moves() else { return };
        // A341 (key 36, bit 98): group 1 reaches a target up to 650 away at any height; group 2 needs an
        // enemy 300 to 650 away within the angle.
        let target = Scene { target: Some(point(500.0, 400.0)), ..Scene::default() };
        let (ev, _) = m.select(&chain(true, 1, &[]), &target, 0);
        let i = m.actions.iter().position(|a| a.key == 36).unwrap();
        assert_eq!(ev[i].verdict, Verdict::Rejected(moveset::Reject::Ability(98)));
        let (ev, chosen) = m.select(&chain(true, 1, &[98]), &target, 0);
        assert_eq!(ev[i].verdict, Verdict::Flagged);
        assert_eq!(m.actions[chosen.unwrap()].key, 36);
        // A340 (key 33, bit 97) is a grounded action for a target 110 to 250 above within 260.
        let high = Scene { target: Some(point(100.0, 200.0)), ..Scene::default() };
        let (ev, _) = m.select(&chain(false, 1, &[97]), &high, 0);
        let i = m.actions.iter().position(|a| a.key == 33).unwrap();
        assert_eq!(ev[i].verdict, Verdict::Flagged);
        let (ev, _) = m.select(&chain(false, 1, &[97]), &Scene { target: Some(point(100.0, 20.0)), ..Scene::default() }, 0);
        assert_eq!(ev[i].verdict, Verdict::Outside(moveset::Miss::Height));
        let (ev, _) = m.select(&chain(true, 1, &[97]), &high, 0);
        assert_eq!(ev[i].verdict, Verdict::Rejected(moveset::Reject::State));
    }

    #[test]
    fn ledge_probe_flags_key_1_and_jump_height_widens_flag_bit_1() {
        let Some(m) = real_moves() else { return };
        // Key 1 is flagged by the ledge probe on the ground whatever its group says.
        let i = m.actions.iter().position(|a| a.key == key::LEDGE_PROBE).unwrap();
        let none = Scene::default();
        let (ev, _) = m.select(&chain(false, 1, &[]), &none, 0);
        assert_ne!(ev[i].verdict, Verdict::Flagged);
        let ledge = Scene { ledge_probe: true, ..Scene::default() };
        let (ev, _) = m.select(&chain(false, 1, &[]), &ledge, 0);
        assert_eq!(ev[i].verdict, Verdict::Flagged);
        // Key 33 (A340, bit 97) has flag bit 1: a target higher than its plain window is reached
        // once the jump height lowers the window's bound.
        let a = m.by_key(33).unwrap();
        assert!(a.flags & 2 != 0);
        let high = point(100.0, 400.0);
        assert_eq!(a.window(&high, 0.0), Err(moveset::Miss::Height));
        assert_eq!(a.window(&high, 235.0), Ok(()));
        // Key 40 (A344) is an record (motion id 172) gated on bit 101 that only the blown press starts.
        let k40 = m.by_key(key::RETALIATING).unwrap();
        assert_eq!((k40.motion, k40.ability, k40.group), (172, 101, 0));
    }

    /// A world of horizontal floor quads (x0, x1, y), each 2000 wide in z, plus party-collide walls.
    fn ledge_world(floors: &[(f32, f32, f32)], walls: &[f32]) -> World {
        let mut positions = Vec::new();
        let mut indices = Vec::new();
        for &(x0, x1, y) in floors {
            let b = positions.len() as u32;
            positions.extend([[x0, y, -1000.0], [x1, y, -1000.0], [x1, y, 1000.0], [x0, y, 1000.0]]);
            indices.extend([b, b + 2, b + 1, b, b + 3, b + 2]);
        }
        let n = positions.len();
        let mesh = kh2_model::Mesh { positions, uvs: vec![[0.0; 2]; n], colors: vec![[1.0; 4]; n], indices, texture: 0, transparent: false, joints: vec![], weights: vec![] };
        let mut w = World::from_meshes(&[mesh]);
        // Walls across x (facing -x), only for the sweep (it walks `tris` directly).
        for &x in walls {
            let v = [Vec3::new(x, -500.0, -1000.0), Vec3::new(x, -500.0, 1000.0), Vec3::new(x, 1000.0, 0.0)];
            w.tris.push(crate::world::Tri { v, n: Vec3::NEG_X, flags: kh2_map::flags::PARTY_COLLIDE, material: 0, lo: v[0].min(v[1]).min(v[2]), hi: v[0].max(v[1]).max(v[2]) });
        }
        w
    }

    #[test]
    fn ledge_probe_is_set_only_without_a_floor_ahead() {
        // Sora's numbers in the shape of FUN_0019a8e8: radius 90, lift 115, a height of 90 here, so a
        // floor counts down to start - (115 - 90) + 1 = 24 below; the vector is 90 - 5 = 85 ahead.
        let b = BodyShape { radius: 90.0, height: 90.0, lift: 115.0 };
        let v = Vec3::X * 85.0;
        // Open floor: the floor at the end point is level, the bit stays clear.
        let l = ledge_at(&ledge_world(&[(-1000.0, 1000.0, 0.0)], &[]), Vec3::ZERO, v, b);
        assert_eq!((l.end, l.floor, l.limit, l.flagged), (Vec3::new(85.0, 0.0, 0.0), Some(0.0), -24.0, false));
        // The floor ends 50 ahead: no floor under the end point, the bit is set.
        let l = ledge_at(&ledge_world(&[(-1000.0, 50.0, 0.0)], &[]), Vec3::ZERO, v, b);
        assert_eq!((l.floor, l.flagged), (None, true));
        // A step down beyond 50: 20 lower still counts, 100 lower does not.
        let l = ledge_at(&ledge_world(&[(-1000.0, 50.0, 0.0), (50.0, 1000.0, -20.0)], &[]), Vec3::ZERO, v, b);
        assert_eq!((l.floor, l.flagged), (Some(-20.0), false));
        let l = ledge_at(&ledge_world(&[(-1000.0, 50.0, 0.0), (50.0, 1000.0, -100.0)], &[]), Vec3::ZERO, v, b);
        assert_eq!((l.floor, l.flagged), (Some(-100.0), true));
        // A floor more than 3 * lift below the end point is out of the ray: no floor.
        let l = ledge_at(&ledge_world(&[(-1000.0, 50.0, 0.0), (50.0, 1000.0, -400.0)], &[]), Vec3::ZERO, v, b);
        assert_eq!((l.floor, l.flagged), (None, true));
        // A step up within lift above the end point is found and counts.
        let l = ledge_at(&ledge_world(&[(-1000.0, 50.0, 0.0), (50.0, 1000.0, 60.0)], &[]), Vec3::ZERO, v, b);
        assert_eq!((l.floor, l.flagged), (Some(60.0), false));
        // A wall 100 ahead stops the sphere after 10: the end point stays on the floor before an edge
        // at 60 that the unblocked sweep would have passed.
        let l = ledge_at(&ledge_world(&[(-1000.0, 60.0, 0.0)], &[100.0]), Vec3::ZERO, v, b);
        assert!((l.end.x - 10.0).abs() < 1e-3, "{l:?}");
        assert_eq!((l.floor, l.flagged), (Some(0.0), false));
        // Sloped travel: v.y below zero (moving down, Y up here) lowers the limit by that much.
        let l = ledge_at(&ledge_world(&[(-1000.0, 1000.0, -30.0)], &[]), Vec3::ZERO, Vec3::new(80.0, -30.0, 0.0), b);
        assert_eq!((l.limit, l.flagged), (-54.0, false));
    }

    #[test]
    fn ledge_probe_runs_on_the_ground_only() {
        let w = ledge_world(&[(-1000.0, 50.0, 0.0)], &[]);
        let b = Some(BodyShape { radius: 90.0, height: 90.0, lift: 115.0 });
        let m = Motor { grounded: true, ..Motor::default() };
        assert!(ledge_probe(&w, &m, false, b, Vec3::X, -5.0).is_some_and(|l| l.flagged));
        // Facing away from the edge: floor under the end point.
        assert!(ledge_probe(&w, &m, false, b, Vec3::NEG_X, -5.0).is_some_and(|l| !l.flagged));
        // Airborne for the selection (FUN_0019aa28): no probe.
        assert!(ledge_probe(&w, &m, true, b, Vec3::X, -5.0).is_none());
    }

    #[test]
    fn explicit_actions_need_a_window_and_their_ability() {
        let Some(m) = real_moves() else { return };
        let w = Windows { free: false, combo: true };
        // Finishing Leap (key 37, bit 95, finisher with argument 1): one attack early.
        assert!(m.explicit(37, &chain(false, 1, &[95]), w).is_err());
        assert_eq!(m.explicit(37, &chain(false, 2, &[95]), w).map(|a| a.motion), Ok(167));
        assert_eq!(m.explicit(37, &chain(false, 2, &[]), w), Err(ExplicitMiss::Rejected(moveset::Reject::Ability(95))));
        assert_eq!(m.explicit(37, &chain(false, 2, &[95]), Windows::default()), Err(ExplicitMiss::NoWindow));
        // Upper Slash (key 12, bit 18): a normal attack, so only while no finisher is possible.
        assert_eq!(m.explicit(12, &chain(false, 1, &[18]), w).map(|a| a.motion), Ok(161));
        assert_eq!(m.explicit(12, &chain(false, 3, &[18]), w), Err(ExplicitMiss::FinisherPossible));
        // Horizontal Slash (key 38, bit 99) is an airborne action.
        assert_eq!(m.explicit(38, &chain(true, 1, &[99]), w).map(|a| a.motion), Ok(193));
        assert_eq!(m.explicit(38, &chain(false, 1, &[99]), w), Err(ExplicitMiss::Rejected(moveset::Reject::State)));
    }

    #[test]
    fn controller_view_follows_the_ability_block() {
        use crate::abilities::{self, id};
        assert_eq!(Abilities::default().combo_block(), (3, 3, 1));
        assert!(Abilities::default().bits() >> ABILITY_GUARD & 1 == 1);
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let Some(rules) = abilities::AbilityRules::load(&mut a) else { return };
        let mut ab = abilities::Abilities::new(Arc::new(rules));
        ab.equip_ability(id::GUARD);
        ab.equip_ability(id::COMBO_PLUS);
        let v = Abilities::from_block(&ab);
        assert_eq!(v.combo_block(), (4, 3, 1));
        assert!(v.bits() >> ABILITY_GUARD & 1 == 1 && v.bits() >> 21 & 1 == 1);
        // A form's list replaces the player's action abilities; its own grants count per copy.
        ab.set_form(Some(vec![id::COMBO_PLUS, id::AIR_COMBO_PLUS, id::AIR_COMBO_PLUS]));
        let v = Abilities::from_block(&ab);
        assert_eq!(v.combo_block(), (5, 5, 1));
        assert!(v.bits() >> ABILITY_GUARD & 1 == 0);
        ab.set_form(None);
        assert_eq!(Abilities::from_block(&ab).combo_block(), (4, 3, 1));
    }

    #[test]
    fn press_counters() {
        // The counters follow FUN_00176b00 whatever the table says.
        let mut c = chain(false, 1, &[]);
        let idle = Windows { free: true, combo: false };
        assert_eq!(c.press(idle, false), Some(Press::Restart));
        assert_eq!((c.combo, c.finishers), (1, 1));
        let window = Windows { free: false, combo: true };
        // No hit landed: the press is refused in the combo window of a playing swing.
        assert_eq!(c.press(window, false), None);
        assert_eq!(c.press(window, true), Some(Press::Continue));
        assert_eq!(c.combo, 2);
        // An extra finisher needs Finishing Plus (max 2) and an open finisher window with a landed hit;
        // with the base single finisher an open finisher window alone does nothing.
        c.finisher_window = true;
        assert_eq!(c.press(Windows::default(), true), None);
        let mut plus = chain(false, 3, &[]);
        plus.max_finishers = 2;
        plus.finisher_window = true;
        assert_eq!(plus.press(Windows::default(), true), Some(Press::ExtraFinisher));
        assert_eq!(plus.finishers, 2);
        // After it a normal attack is refused (finisher ready, finishers used up).
        let normal = Action { key: 6, group: 1, alt: -1, arg: 0, flags: 0, motion: 153, next_motion: 0, distance: [0.0; 2], height: [0.0; 2], angle: [0.0; 2], ability: 0 };
        assert_eq!(plus.check(&normal), Err(moveset::Reject::FinisherDone));
    }
}
