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
use kh2_anim::moveset::{self, key, Action, BlowState, ExplicitMiss, Moveset, Probe, Recovery, Scene, Verdict, Windows};
use kh2_anim::{id, triggers::range, PlayerState};
use kh2_model::collision::kind as shape_kind;

use crate::assets::Disc;
use crate::camera::OrbitCamera;
use crate::combat::{Fighter, Hit, IneffectiveHit, ReactionData, ReactionStatus, Team, WallHit};
use crate::controller_rules::{stick_through_view, view_matrix};
use crate::data::GameData;
use crate::enemy::Enemy;
use crate::motor::{wrap_angle, Jump, Motor};
use crate::reaction_rules::{self as rules, Blow, Push, ReflectFields, VictimAction, VictimCtx};
use crate::schedule::{Frame, Sim};
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::world::World;
use crate::SCALE;

pub use crate::shared::sora::{HAND_BONES, respawn, Abilities, Battle, Dash, Pad, Rigs, Sora, State, SwingConsts, WeaponCue};

/// Ticks the keyblade stays out after the last sign of a fight.
const BATTLE_TICKS: f32 = 240.0;

/// The motion-set variant (the relative slot of `id * 4 + variant`) Sora's controller asks for, the
/// rule of `FUN_0017da30` (tables at 0x37a5c8: row 0 = 0,1,3,2; row 1 = 1,0,2,3; row 2 = 2,3,1,0; row 3 = 3,2,0,1):
/// the row is 1 while the actor's battle bit (+0x108 bit 0x100000, raised by `FUN_00174b68` when the game's
/// battle flag `FUN_001903d8` is set, dropped by `FUN_0019a820` when it is clear) is set, else 2; for a
/// party member (+0x588 bit 4) whose weapon is out (`FUN_001752a0`) the low bit flips, so the row becomes 0
/// (battle) or 3 (field). The first slot of the row that exists wins (`Mset::slot`). Weapon out with the
/// game's battle flag clear is the field variant 3: its walk and run (`A001`, `A002`, slots 7 and 11) are the
/// armed ones. [inferred] A hidden weapon with the flag set (row 1) is left as the field variant without a
/// weapon: the game would play a weapon variant and show the weapon at once, the app shows it by its own
/// stand-in timer (`Battle`).
pub fn motion_state(weapon_out: bool, battle_flag: bool) -> PlayerState {
    match (weapon_out, battle_flag) {
        (true, true) => PlayerState::BattleWeapon,
        (true, false) => PlayerState::FieldWeapon,
        (false, _) => PlayerState::FieldNoWeapon,
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

/// The dodge roll's motion: `A180`, motion id 209 of Sora's motion set (user report 2026-10-24; Final Mix FM 0x179530 asks for
/// motion 209, docs/systems/foundations/final-mix.md). The id is tried first with the state fallback of `Mset::slot`, then any slot
/// tagged `A180`. `None` when the motion set has no such motion (the USA disc's has none [FM-only]): the caller then shows the Quick
/// Run start motion.
pub const DODGE_ROLL_ID: usize = 209;
pub const DODGE_ROLL_TAG: &str = "A180";

pub fn dodge_roll_slot(mset: &kh2_anim::Mset, state: PlayerState) -> Option<usize> {
    mset.slot(DODGE_ROLL_ID, state)
        .filter(|&s| mset.get(s).is_some_and(|x| x.tag == DODGE_ROLL_TAG))
        .or_else(|| mset.by_tag(DODGE_ROLL_TAG).next())
}

/// A Square press while moving (Dodge Roll learned) that is released before this many ticks is a
/// tap: the dodge roll; still held at this tick it is the Quick Run. The number is the Final Mix
/// executable's: the roll's motion A180 carries frame trigger type 34 at tick 10, whose handler
/// (FM 0x189c78) starts Quick Run when Square is still held (docs/systems/foundations/final-mix.md "Tap and hold").
/// The USA executable has no roll, so the rule is a Final Mix research result [FM rule constant].
const TAP_TICKS: f32 = 10.0;

/// Range trigger 30: suspends gravity like trigger 3 (FUN_00188030 sets actor +0x160 bit
/// 0x1000000 for it, which FUN_0017b6e0 and FUN_00169ee8 read together with bit 0x20 of trigger 3).
const TRIGGER_NO_GRAVITY_30: u8 = 30;

/// Range trigger 0x31 (49): FUN_00188030 case 0x31 sets bit 0x10 of the motion controller's third
/// trigger word (actor +0x168); FUN_0017b9e0 and FUN_00183150 (the landing callbacks) play no landing
/// motion while it is set, so the playing motion continues on the floor.
const TRIGGER_NO_LANDING_MOTION_49: u8 = 49;

/// The leap of an attack record (`ptya` record fields the selection does not use): FUN_00176050
/// launches the actor's mover with it when `height` (+0xc) is above 0. Air attacks have one; ground
/// attacks have none (all zero), except the ledge record key 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Leap {
    /// +0xc: the jump height without a target (and the "has a leap" test).
    pub height: f32,
    /// +0x10: the highest jump toward a target (plus the jump height for flag bit 1).
    pub reach: f32,
    /// +0x14: the lowest jump toward a target.
    pub min_height: f32,
    /// +0x18, +0x1c: the horizontal speed range toward a target; +0x1c is also the speed without one.
    pub min_speed: f32,
    pub max_speed: f32,
    /// +0x3c: added to the horizontal gap to the target.
    pub gap: f32,
}

impl Leap {
    fn from_record(r: &[u8]) -> Option<Leap> {
        let f = |o: usize| r.get(o..o + 4).map(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]]));
        Some(Leap { height: f(0xc)?, reach: f(0x10)?, min_height: f(0x14)?, min_speed: f(0x18)?, max_speed: f(0x1c)?, gap: f(0x3c)? })
    }
}

/// The leap fields of every record of every `ptya` table, beside the record as the selection reads
/// it (`moveset::Action` does not carry them). The controller finds the started record here; the
/// drive code swaps `Sora::moves` for a form's table, so the lookup is by record, not by index.
#[derive(Resource, Default)]
pub struct AttackLeaps(Vec<(Action, Leap)>);

impl AttackLeaps {
    pub fn parse(ptya: &[u8]) -> AttackLeaps {
        let word = |o: usize| ptya.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]) as usize);
        let mut out = Vec::new();
        for index in 0..word(4).unwrap_or(0) {
            let (Some(m), Some(at)) = (Moveset::parse(ptya, index), word(8 + 4 * index)) else { continue };
            for (i, a) in m.actions.into_iter().enumerate() {
                let start = at + 4 + moveset::RECORD_SIZE * i;
                if let Some(l) = ptya.get(start..start + moveset::RECORD_SIZE).and_then(Leap::from_record) {
                    out.push((a, l));
                }
            }
        }
        AttackLeaps(out)
    }

    /// The leap of a started record (all zero when the record is not found).
    pub fn of(&self, a: &Action) -> Leap {
        self.0.iter().find(|(x, _)| x == a).map_or(Leap::default(), |(_, l)| *l)
    }
}

/// The launch of an attack's leap (FUN_00176050, record +0xc above 0): the horizontal unit
/// direction, the horizontal speed per tick and the jump height. `to_target` is the target's point
/// less Sora's own (both the centres of their target shapes, FUN_00176400 -> FUN_0018eec0 /
/// FUN_001894d0, Y up here) and `radii` the sum of their radii; `jump_height` is what record flag
/// bit 1 adds to the reach (FUN_0016be20).
/// * With a target: the height is the target's height above Sora, at least +0x14 and at most the
///   reach; the speed covers the horizontal gap (distance less the radii, plus +0x3c) in the rise
///   time sqrt(2h/g) of that height (FUN_0017bee0), clamped to +0x18..+0x1c.
///   [inferred] The distance is read from the VU0 Q register after the square root (`cfc2`).
/// * Without one: height +0xc, along the facing (FUN_0016a890), at speed +0x1c.
pub fn leap_launch(l: &Leap, flags: u32, jump_height: f32, facing: Vec3, target: Option<(Vec3, f32)>) -> (Vec3, f32, f32) {
    let Some((to, radii)) = target else { return (facing.with_y(0.0).normalize_or_zero(), l.max_speed, l.height) };
    let reach = l.reach + if flags & 2 != 0 { jump_height } else { 0.0 };
    let up = to.y;
    let h = if l.min_height <= up { if reach < up { reach } else { up } } else { l.min_height };
    let flat = to.with_y(0.0);
    let gap = flat.length() - radii + l.gap;
    let rise = (2.0 * h / crate::motor::G).sqrt();
    let s = gap / rise;
    let speed = if l.min_speed <= s { if s <= l.max_speed { s } else { l.max_speed } } else { l.min_speed };
    (flat.normalize_or_zero(), speed, h)
}

/// The air wall recoil's launch (FUN_00185978 -> FUN_0017b278): the horizontal velocity per tick
/// and the jump height of the new jump (`Motor::start_leap`). FUN_0017b278 zeroes the vector's y,
/// normalises it and scales it by its w, power x HitBackToJump; FUN_0017b328 copies the result into
/// the velocity. None when the power is not above 0 (FUN_00185d30 calls no push then).
pub fn air_recoil_launch(facing: Vec3, power: f32, hit_back_to_jump: f32, c: &SwingConsts) -> Option<(Vec3, f32)> {
    if power <= 0.0 {
        return None;
    }
    let dir = (facing * c.recoil_facing).with_y(0.0).normalize_or_zero();
    Some((dir * (power * hit_back_to_jump), c.recoil_height))
}

/// The turn of range triggers 22 and 28 (FUN_00188030 cases 0x16 / 0x1c -> FUN_00186118 /
/// FUN_001861b8 -> FUN_00186018 -> FUN_00183848), per tick while the trigger covers the frame: the
/// largest turn toward the target in radians, the trigger's first parameter x `turn_unit` (a
/// parameter of 0 takes the actor's turn speed +0x11c, here `turn_speed`). None while the target is
/// within `turn_min_gap` horizontally (FUN_00186018 skips the turn) or without the constants.
/// Trigger 28 aims at the player's target (+0xb40), trigger 22 at the attack's own target (+0x9f4,
/// stored by FUN_00176050); the app has one target for both.
pub fn attack_turn_rate(param: i16, turn_speed: f32, to_target: Vec3, c: Option<&SwingConsts>) -> Option<f32> {
    let c = c?;
    if to_target.with_y(0.0).length() <= c.turn_min_gap {
        return None;
    }
    let rate = param as f32 * c.turn_unit;
    Some(if rate == 0.0 { turn_speed } else { rate })
}
/// What a Square press on the ground (no explicit action started) does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SquareGround {
    /// The guard (FUN_00178080; Final Mix FM 0x1790c0).
    Guard,
    /// The Dodge Roll start (FM 0x179530); the hold past `TAP_TICKS` turns it into the Quick Run.
    Roll,
    /// Quick Run at the press (FUN_00178108 = FM 0x179148).
    QuickRun,
}

/// The Square decision after the explicit keys, in the Final Mix order (FM 0x178eb0, docs/systems/foundations/final-mix.md):
/// stick at rest: the guard first; then the Dodge Roll (growth slot 4); then Quick Run (slot 1;
/// needs the stick pushed); then the guard again. With neither learned, a moving press therefore
/// still guards, as the USA code does (it has no stick test in the guard). `moving` is the stick
/// magnitude above 0 (actor +0x1c).
pub fn square_ground(moving: bool, dodge_roll: u8, quick_run: u8) -> SquareGround {
    if !moving {
        SquareGround::Guard
    } else if dodge_roll >= 1 {
        SquareGround::Roll
    } else if quick_run >= 1 {
        SquareGround::QuickRun
    } else {
        SquareGround::Guard
    }
}

/// One tick of a Square dash: `held` is the button, `ticks` the ticks since the press, `quick_run`
/// the Quick Run level and `run_left` its clock (`AirSlideTime`, counted down while held). Returns
/// the new phase and whether the dash stops now (the stop brake starts).
pub fn dash_step(phase: Dash, held: bool, ticks: f32, quick_run: u8, run_left: f32) -> (Dash, bool) {
    match phase {
        Dash::Pending if !held => (Dash::Roll, ticks >= TAP_TICKS),
        Dash::Pending if ticks >= TAP_TICKS => {
            if quick_run >= 1 {
                (Dash::Run, run_left <= 0.0)
            } else {
                // Quick Run not learned: the press was a roll after all.
                (Dash::Roll, true)
            }
        }
        Dash::Pending => (Dash::Pending, false),
        Dash::Roll => (Dash::Roll, ticks >= TAP_TICKS),
        // FUN_001c1660: the clock runs only while the button is held; a release ends it at once.
        Dash::Run => (Dash::Run, !held || run_left <= 0.0),
    }
}

/// Blend lengths in ticks.
const BLEND_LOCOMOTION: f32 = 8.0;
const BLEND_FALL: f32 = 8.0;
const BLEND_GLIDE: f32 = 16.0;
/// The attack start (FUN_00176050) requests its motion with blend 0 (`mtc1 r0,$f20` at 0x176090):
/// attack to attack is a cut.
const BLEND_ATTACK: f32 = 0.0;
/// Stick magnitude below which Sora walks.
const WALK_STICK: f32 = 0.8;
/// The frame step of one tick (`DAT_00356f40` per vblank run, see `schedule::FrameClock::vblank`).
const TICK_STEP: f32 = 1.0;

/// Growth-ability levels (0 = not learned, 5 = MAX). Written from the ability block's growth
/// levels (`FUN_001c1340`) whenever the abilities change (`abilities::sync_controller`); in a form
/// the form's own growth ability is the only one that applies.
#[derive(Resource, Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct Growth {
    pub high_jump: u8,
    pub quick_run: u8,
    pub aerial_dodge: u8,
    pub glide: u8,
    /// The fifth growth kind (growth slot 4 in Final Mix, docs/systems/foundations/final-mix.md): from `Progress`.
    pub dodge_roll: u8,
}

/// `KH2_GROWTH=a,b,c,d[,e]`: the harness's growth levels (High Jump, Quick Run, Aerial Dodge,
/// Glide, Dodge Roll; up to 5, the forced MAX of `FUN_001c13b8`; an omitted value is 0, so
/// `KH2_GROWTH=0,1,0,0` is the USA rules: no Dodge Roll), used instead of the ability block's while
/// no form is active.
#[derive(Resource, Clone, Copy, Default, Debug)]
pub struct GrowthOverride(pub Option<Growth>);

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
            // FUN_001759b8: motion 40 with blend 0 (`mtc1 r0, $f20` at 0x1759c8, $f12 = $f13 = 0 for both requests), then the mover's launch (its method +8 with a
            // vector built from the stick direction, plyr BlowRecoveryH and BlowRecoveryV; the
            // launch itself was not decoded). The app ends the flight and lets Sora fall.
            body.restart(rig.mset.slot(id::DAMAGE_BLOW_RECOV, pstate), false, BLEND_ATTACK);
            weapon.restart(None, false, BLEND_ATTACK);
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
        let leaps = {
            let disc = app.world().resource::<Disc>();
            let mut a = disc.0.lock().unwrap();
            kh2_tables::subfile(&mut a, &kh2_tables::tables::PTYA).map(|b| AttackLeaps::parse(&b)).unwrap_or_default()
        };
        app.insert_resource(rigs)
            .insert_resource(leaps)
            .init_resource::<Pad>()
            .init_resource::<Battle>()
            .init_resource::<Abilities>()
            .add_systems(Startup, setup)
            .add_systems(Update, read_pad.in_set(Frame::Pad))
            .add_systems(
                FixedUpdate,
                (
                    (refresh_status, sync_weapon).in_set(Sim::Weapon),
                    (follow_hand, tick).chain().in_set(Sim::Sora),
                    (track_connects, receive_hits, receive_wall_hits).chain().in_set(Sim::SoraHurt),
                    apply_player_fade.after(Sim::Pose),
                ),
            );
    }
}

/// `FUN_0019fb98` for the player: the status block is rebuilt from the record, so a level-up (or an equipment
/// change) reaches the strength, magic, defense and maximum HP Sora fights with. Runs every tick and writes only
/// when the rebuilt block differs, which covers the area start (Sora spawns with the level 1 block) and the tick
/// after `pay_defeats` raises the level. HP keeps its value, held under the new maximum; a full bar stays full
/// when the maximum grows (the spawn). Without a `Progress` resource the block is left alone.
fn refresh_status(progress: Option<Res<crate::progress::Progress>>, data: Res<GameData>, mut sora: Query<&mut Sora>) {
    let Some(progress) = progress else { return };
    let Ok(mut s) = sora.single_mut() else { return };
    let new = data.sora_status(&progress);
    let old = &s.stats;
    if (old.level, old.max_hp, old.max_mp, old.strength, old.magic, old.defense, old.ap, old.weakness)
        == (new.level, new.max_hp, new.max_mp, new.strength, new.magic, new.defense, new.ap, new.weakness)
    {
        return;
    }
    let was_full = s.hp >= old.max_hp;
    info!(
        "sora: status rebuilt from the record: level {} HP max {} MP max {} strength {} magic {} defense {} AP {} resistances {:?}",
        new.level, new.max_hp, new.max_mp, new.strength, new.magic, new.defense, new.ap, new.weakness
    );
    s.hp = if was_full { new.max_hp } else { s.hp.min(new.max_hp) };
    s.mp = s.mp.min(new.max_mp);
    s.stats = new;
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
    let swing = disc.read_root("SLUS_210.05").and_then(|e| SwingConsts::from_exe(&e));
    if swing.is_none() {
        warn!("sora: the air swing constants are not readable from SLUS_210.05; the air wall recoil uses the ground push and the attack turns use the turn speed");
    }
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
            game_over: false,
            leaf_bracer: 0.0,
            damage_motion: None,
            lock: None,
            target: None,
            body,
            weapons,
            timer: 0.0,
            dash_time: 0.0,
            dash_brake: 0.0,
            dash: Dash::Pending,
            hover: false,
            last_move: Vec3::ZERO,
            air_recoil: false,
            swing,
            aerial_dodge_ready: false,
            lunge_left: 0.0,
            lunge_speed: 0.0,
            viewer_slot: 0,
        },
        Fighter { team: Team::Player, attack_bodies, hurt_body: body, landed: Default::default(), hittable: true },
    ));
    let growth = std::env::var("KH2_GROWTH").ok().map(|v| {
        let l: Vec<u8> = v.split(',').filter_map(|x| x.parse().ok()).collect();
        Growth { high_jump: l.first().copied().unwrap_or(0), quick_run: l.get(1).copied().unwrap_or(0), aerial_dodge: l.get(2).copied().unwrap_or(0), glide: l.get(3).copied().unwrap_or(0), dodge_roll: l.get(4).copied().unwrap_or(0) }
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
    mut pad_state: ResMut<crate::rig::PadState>,
    scale: Res<crate::rig::StickScale>,
) {
    // The left stick as the original's record: unit direction (x right, y up here) and magnitude. The keyboard is one pad view.
    let (stick_dir, stick_mag) = if pad_state.scripted {
        // A rig pad script is running: the left stick drives Sora through the original's dead zone and scale (`FUN_00167198`). The
        // buttons already arrived as the keys the script mirrors (`rig::BUTTON_KEYS`).
        crate::rig::stick_vector(pad_state.lx, pad_state.ly, &scale)
    } else {
        let mut v = Vec2::ZERO;
        for (k, d) in [(KeyCode::KeyW, Vec2::Y), (KeyCode::KeyS, -Vec2::Y), (KeyCode::KeyA, -Vec2::X), (KeyCode::KeyD, Vec2::X)] {
            if keys.pressed(k) {
                v += d;
            }
        }
        *pad_state = crate::rig::PadState::from_keyboard(&keys);
        let v = v.normalize_or_zero();
        (v, if v == Vec2::ZERO { 0.0 } else if keys.pressed(KeyCode::ControlLeft) { 0.5 } else { 1.0 })
    };
    // FUN_001774d8 -> FUN_00177360: the stick goes through the DRAWN view (the camera's smoothed position and look-at point,
    // FUN_00106f78), not through the camera's orbit yaw: where the camera collision holds the camera the two differ. The orbit yaw
    // (a camera at (sin yaw, ., cos yaw) from the player looking at him) is the fallback before the camera has run.
    let view = match cam.single() {
        Ok(c) => c.drawn.map_or_else(|| orbit_view(c.yaw), |(eye, look)| view_matrix(eye.to_array(), look.to_array(), Vec3::Y.to_array())),
        Err(_) => orbit_view(0.0),
    };
    let heading = stick_through_view(&view, [stick_dir.x, -stick_dir.y, 0.0, stick_mag]);
    pad.dir = Vec3::new(heading[0], 0.0, heading[2]);
    pad.stick = if pad.dir == Vec3::ZERO { 0.0 } else { stick_mag };
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
        *growth = Growth { high_jump: l, quick_run: l, aerial_dodge: l, glide: l, dodge_roll: l };
        info!("growth abilities: level {l}");
    }
}

/// The view matrix of a camera orbiting the player at `yaw` (the camera at (sin yaw, ., cos yaw) from him, looking at him).
fn orbit_view(yaw: f32) -> [f32; 16] {
    view_matrix([yaw.sin(), 0.0, yaw.cos()], [0.0; 3], Vec3::Y.to_array())
}

/// FUN_00184498 (the weapon object's update) calls FUN_0019afe8 on the owner and skips the update when it returns 1. Of that
/// function's terms only the global flag word is modelled (bit 0x1000, Peter Pan's summon limit); the owner's +0x10c bits
/// 0x80 / 0x200, DAT_0035ee9c, FUN_0019af28, FUN_0019b100 and FUN_002c56c0 have no model here [GAP].
pub fn weapon_update_held(flags: u32) -> bool {
    crate::magic_rules::update_gate_global(flags)
}

/// The weapons' motion sets mirror Sora's slot for slot: lock every weapon to Sora's slot and
/// frame (a weapon without the slot plays nothing).
pub fn sync_weapon(sora: Query<&Sora>, mut anims: Query<&mut Animator>, gflags: Option<Res<crate::magic_rules::GlobalFlags>>, mut held: Local<bool>) {
    let Ok(s) = sora.single() else { return };
    let gate = weapon_update_held(gflags.as_deref().map_or(0, |g| g.0));
    if gate != *held {
        *held = gate;
        info!("weapon update gate (global flag 0x1000): {}", if gate { "held, the weapons' motion stops" } else { "released" });
    }
    if gate {
        // FUN_00184498 skips the weapon object's update while FUN_0019afe8 holds: the motion stops where it is.
        for w in &s.weapons {
            if let Ok(mut weapon) = anims.get_mut(*w) {
                weapon.speed = 0.0;
            }
        }
        return;
    }
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

/// The optional resources of `tick`, in one parameter (Bevy takes 16 system parameters at most).
type TickOptionals<'a> = (Option<Res<'a, crate::shared::struggle::Mission>>, Option<Res<'a, crate::audio_bgm::BattleFlag>>, Option<Res<'a, crate::shared::summon::SummonState>>);

/// One 60 Hz tick of Sora's controller.
#[allow(clippy::too_many_arguments)]
pub fn tick(
    mut pad: ResMut<Pad>,
    data: Res<GameData>,
    growth: Res<Growth>,
    abilities: Res<Abilities>,
    leaps: Res<AttackLeaps>,
    mut battle: ResMut<Battle>,
    world: Option<Res<World>>,
    reaction: Option<Res<ReactionData>>,
    enemies: Query<(Entity, &GlobalTransform, &Enemy)>,
    mut sora_q: Query<(&mut Transform, &mut Sora, &mut Fighter)>,
    mut anims: Query<&mut Animator>,
    globals: Query<&GlobalTransform>,
    mut bodies: Query<&mut Transform, Without<Sora>>,
    mut vis: Query<&mut Visibility>,
    // Bevy takes 16 system parameters at most: the optional resources come as one tuple.
    (mission, game_flag, summons): TickOptionals,
) {
    pad.lock_new = false;
    let (Ok((mut tf, mut s, mut fighter)), Some(world)) = (sora_q.single_mut(), world) else { return };
    let s = &mut *s;
    // A playing mission layout or the fight's stop: no input reaches the controller (WP-F1 item 2).
    // Tinker Bell's revive event takes the pad from the players for its 42 ticks (`FUN_00196280`, docs/systems/drive-limits-summons/contract.md).
    let locked = mission.as_deref().is_some_and(|m| m.control_locked() || m.fight_stopped()) || summons.as_deref().is_some_and(|m| m.revive_pad_locked());
    if locked {
        if pad.any_input() && move_log() {
            info!("move: input ignored: the mission's layout locks control or the fight stopped (FUN_001b7d50 / FUN_001b9940)");
        }
        pad.lock();
    }
    // The target points of the enemies, for the attack table's range tests.
    let enemy_points: Vec<(Entity, Vec<TargetPoint>)> = enemies
        .iter()
        .filter(|(_, _, e)| e.alive())
        .filter_map(|(en, _, e)| Some((en, target_points(anims.get(e.body).ok()?, globals.get(e.body).ok()?))))
        .collect();
    // The weapons are addressed through a cue that is replayed on each of them at the end of
    // the tick (`WeaponCue`); `wrig` answers whether any weapon follows a body slot.
    let wrig = WeaponRigs::of(&s.weapons, &anims);
    // Sora's own target point (FUN_0016b558(actor, 3, -1): his first target-kind shape), for the leap.
    let own_point = anims.get(s.body).ok().zip(globals.get(s.body).ok()).and_then(|(a, g)| target_points(a, g).first().copied());
    let Ok(mut body) = anims.get_mut(s.body) else { return };
    let rig = body.rig.clone();
    let mut weapon = WeaponCue::default();
    let pos_m = s.motor.pos * SCALE;
    let dir = pad.dir;
    let attack = Pad::take(&mut pad.attack);
    let jump = Pad::take(&mut pad.jump);
    let square = Pad::take(&mut pad.square);
    let lock_pressed = Pad::take(&mut pad.lock);
    pad.lock_new = lock_pressed;
    s.invuln = (s.invuln - 1.0).max(0.0);
    // FUN_00175ab0: lowered by the frame step while above 0 (one tick here [inferred: the app's tick is
    // the game's frame step]).
    if s.leaf_bracer > 0.0 {
        s.leaf_bracer -= 1.0;
    }
    if !matches!(s.state, State::Hurt | State::Blown) {
        s.damage_motion = None;
    }
    s.timer += 1.0;
    // Another system (a hit, a reaction, a spell) took Sora out of the air recoil.
    if s.state != State::Hurt {
        s.air_recoil = false;
    }
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
        // The lock sets the target marker (FUN_0019b3f0), which binds the target's HP gauge only with
        // Scan (FUN_0019b4a8). App enemies are in the actor list, have a status block and keep no
        // +0x108 flags 0x4000 / 0x80000 / 0x10000000 [inferred: no app writer], so the gate is the bit.
        if s.lock.is_some() && std::env::var_os("KH2_ABILITYLOG").is_some() {
            let scan = abilities.has_bit(crate::abilities::BIT_SCAN);
            let bound = crate::abilities::scan_binds_gauge(true, scan, true, 0, true);
            info!("abilities: lock-on target marker: Scan (bit {}) {}: target gauge {}", crate::abilities::BIT_SCAN, if scan { "set" } else { "not set" }, if bound { "bound (FUN_0019b4a8)" } else { "not bound" });
        }
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
    // Without the game's flag resource (tests) the stand-in decides alone, as before.
    let pstate = motion_state(battle_on, game_flag.as_ref().is_none_or(|f| f.active()));
    if battle.last_state != Some(pstate) {
        if move_log() {
            info!("move: motion variant {:?} -> {pstate:?} (keyblade out {battle_on}, game battle flag {:?}; FUN_0017da30)", battle.last_state, game_flag.as_ref().map(|f| f.active()));
        }
        battle.last_state = Some(pstate);
    }
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

    // The target's first target point (the selection's target and the leap's aim).
    let target_point = s.target.and_then(|t| enemy_points.iter().find(|(e, _)| *e == t)).and_then(|(_, p)| p.first().copied());

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
        // FUN_00176050 writes no velocity for a record without a leap. On the ground the mover's
        // velocity follows the stick, which FUN_001778e8 zeroes outside an idle range, so Sora
        // stops; in the air the jump mover keeps its velocity (FUN_001832b8 brakes it by 0.95 a
        // tick while the stick is zeroed), so the swing does not stop him.
        let airborne = !s.motor.grounded;
        if !airborne {
            s.motor.vel = Vec3::ZERO;
        }
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
        let leap = leaps.of(a);
        if leap.height > 0.0 {
            // FUN_00176050 with record +0xc above 0: on the ground the counters restart
            // (FUN_00176a70), then the mover is launched (its method +8, FUN_001796a8): a new jump
            // of the leap's height carrying the leap's horizontal velocity. It replaces the plyr V0
            // approach of the app.
            if !airborne {
                s.hits = 1;
                s.finishers = 1;
            }
            let own = own_point.unwrap_or(TargetPoint { pos: s.motor.pos, radius: 0.0, half_height: 0.0 });
            let to_target = target_point.map(|t| (t.pos - own.pos, t.radius + own.radius));
            let (d, speed, h) = leap_launch(&leap, a.flags, jump_height(&data, &growth), s.motor.forward(), to_target);
            s.motor.vel = d * speed;
            s.motor.start_leap(h);
            s.lunge_left = 0.0;
            info!(
                "sora leap: key {} height {:.1} speed {:.1} ({}), velocity ({:.1} {:.1})",
                a.key,
                h,
                speed,
                if to_target.is_some() { "toward the target" } else { "along the facing" },
                s.motor.vel.x,
                s.motor.vel.z
            );
        } else if airborne && move_log() {
            info!("move: air attack without a leap: velocity kept ({:.1} {:.1})", s.motor.vel.x, s.motor.vel.z);
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
    // An attack's plyr V0 approach while airborne, added to the mover's own velocity.
    let mut lunge_move = Vec3::ZERO;
    // Gravity suspended by the playing motion (triggers 3, 24, 30; FUN_00169ee8 latches it in actor
    // +0x108 bits 0x800000 / 0x1000000). When it ends while airborne, FUN_0016a2f0 restarts the jump
    // mover as a fall that carries the last real movement (+0x860): the horizontal velocity becomes
    // last tick's displacement and the fall starts at its downward speed. Ported for the attack and
    // the free fall that follows it.
    let hover_now = !s.motor.grounded && (body.trigger(range::NO_GRAVITY) || body.trigger(range::NO_GRAVITY_KEEP_KINETICS) || body.trigger(TRIGGER_NO_GRAVITY_30));
    if s.hover && !hover_now && !s.motor.grounded && matches!(s.state, State::Attack { .. } | State::Air) {
        s.motor.vel = s.last_move.with_y(0.0);
        s.motor.jump = Jump::falling_at(-s.last_move.y, s.motor.pos.y);
        if move_log() {
            info!("move: gravity back: fall carrying ({:.1} {:.1} {:.1})", s.last_move.x, s.last_move.y, s.last_move.z);
        }
    }
    s.hover = hover_now;
    match s.state {
        State::Ground => {
            s.hits = 0;
            s.finishers = 1;
            s.motor.ground_velocity(dir, move_speed);
            if move_speed > 0.0 {
                // FUN_0016a2f0: the facing turns by the turn speed times the frame step. The app runs one tick per vblank of the
                // frame, so a frame of step 2 is two ticks of step 1.0: the same turn per frame.
                s.motor.turn_to_step(dir, prty.turn_speed, TICK_STEP);
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
                // window, so from idle they never start), then the dash, then the guard.
                // The Final Mix order (`square_ground`, docs/systems/foundations/final-mix.md): still = guard; moving with
                // Dodge Roll learned = the roll (a hold of `TAP_TICKS` ticks becomes the Quick Run when
                // that is learned); moving with only Quick Run learned = Quick Run at the press (USA
                // FUN_00178108); moving with neither = the guard (USA FUN_00178080, no stick test).
                let act = square_ground(move_speed > 0.0, growth.dodge_roll, growth.quick_run);
                if square_explicit(s, &mut body, &mut weapon, false, Windows { free: true, combo: false }, false) {
                    // An explicit action started.
                } else if act != SquareGround::Guard
                    && let Some(f) = data.fmab(growth.quick_run.max(1))
                {
                    // The dash starts at the press with FUN_00178108's numbers: AirSlideSpeed along the
                    // stick, AirSlideBrake per tick, AirSlideStarTime of invincibility (+0xb00), motion
                    // 203 (0xcb, A150). [inferred] The roll (FM motion 209 = A180, FM only) is shown by
                    // the Quick Run start motion, and with Quick Run not learned it uses the level 1 row:
                    // the USA disc has no roll data (no motion, no 17th `fmab` column).
                    s.state = State::QuickRun;
                    s.timer = 0.0;
                    s.dash = if act == SquareGround::Roll { Dash::Pending } else { Dash::Run };
                    s.motor.yaw = dir.x.atan2(dir.z);
                    s.motor.vel = dir * f.air_slide_speed;
                    s.dash_time = f.air_slide_time;
                    s.dash_brake = f.air_slide_brake;
                    s.invuln = f.air_slide_star_time;
                    // The roll shows A180 (user report); a motion set without it keeps the Quick Run start motion.
                    let start = if act == SquareGround::Roll { dodge_roll_slot(&rig.mset, pstate) } else { None };
                    body.restart(start.or_else(|| motion(mid::QUICK_RUN)), false, 0.0);
                    info!(
                        "sora square: moving, Dodge Roll level {}, Quick Run level {}: {} (speed {:.1})",
                        growth.dodge_roll,
                        growth.quick_run,
                        if act == SquareGround::Roll { "dodge roll pending (tap = roll, hold = quick run)" } else { "quick run at the press (USA rule)" },
                        f.air_slide_speed
                    );
                } else if act == SquareGround::Guard
                    && let Some(g) = s.moves.by_key(key::GUARD).filter(|g| abilities.chain(s.hits, s.finishers, false, false).check(g).is_ok()).cloned()
                    && let Some(slot) = rig.mset.slot(g.motion as usize, pstate)
                {
                    // The guard action is a ground action of the attack table (FUN_00178080: key 11,
                    // available, an idle range; it zeroes the stick). While standing still, or moving
                    // with neither Dodge Roll nor Quick Run learned (the USA rule).
                    s.state = State::Guard;
                    s.timer = 0.0;
                    body.restart(Some(slot), false, BLEND_ATTACK);
                    weapon.restart(Some(slot).filter(|&x| wrig.mset.has(x)), false, BLEND_ATTACK);
                    info!(
                        "sora guard {} (key {}, motion id {}; {}, Dodge Roll level {}, Quick Run level {})",
                        body.tag(),
                        g.key,
                        g.motion,
                        if move_speed > 0.0 { "stick pushed, neither learned" } else { "standing still" },
                        growth.dodge_roll,
                        growth.quick_run
                    );
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
            // FUN_001c1660: the Quick Run clock runs while the button is held (+0x10c bit 4).
            if pad.square_held {
                s.dash_time -= 1.0;
            }
            let stopping = body.slot == motion(mid::QUICK_RUN_END);
            // The roll plays A180 (dodge_roll_slot): a tap lets it play out; a hold past the tap window turns it into the Quick Run
            // (the original's frame trigger 34 at tick 10 starts the Quick Run from the roll).
            let roll_slot = dodge_roll_slot(&rig.mset, pstate);
            let rolling = roll_slot.is_some() && body.slot == roll_slot;
            if !stopping {
                let (phase, stop) = dash_step(s.dash, pad.square_held, s.timer, growth.quick_run, s.dash_time);
                if phase != s.dash {
                    match phase {
                        Dash::Roll if pad.square_held => info!("sora dodge roll: Square held {:.0} ticks but Quick Run is not learned (level 0)", s.timer),
                        Dash::Roll => info!("sora dodge roll: Square released after {:.0} ticks (tap, under {TAP_TICKS:.0}), Quick Run level {}", s.timer, growth.quick_run),
                        Dash::Run => info!("sora quick run: Square held {:.0} ticks, Quick Run level {}, {:.0} ticks left", s.timer, growth.quick_run, s.dash_time),
                        Dash::Pending => {}
                    }
                    s.dash = phase;
                }
                if rolling && phase == Dash::Run {
                    body.restart(motion(mid::QUICK_RUN), false, 0.0);
                }
                if stop && rolling && s.dash == Dash::Roll {
                    // A tap: A180 plays on while the speed brakes; the state ends with the motion (below).
                    if let Some(f) = data.fmab(growth.quick_run.max(1)) {
                        s.dash_brake = f.air_slide_stop_brake;
                    }
                } else if stop && let Some(f) = data.fmab(growth.quick_run.max(1)) {
                    // The stop: AirSlideStopBrake per tick and motion 204 (0xcc), back to idle after.
                    s.dash_brake = f.air_slide_stop_brake;
                    body.restart(motion(mid::QUICK_RUN_END), false, 0.0);
                    info!("sora dash stop: {:?} after {:.0} ticks", s.dash, s.timer);
                }
            }
            s.motor.vel *= s.dash_brake;
            if (body.slot == motion(mid::QUICK_RUN_END) && (body.finished || s.motor.vel.length() < prty.walk_speed)) || (rolling && s.dash == Dash::Roll && body.finished) {
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
        State::Attack { .. } => {
            if attack {
                s.queued_attack = true;
            }
            let combo_window = body.trigger(range::ALLOW_COMBO);
            let finisher_window = body.trigger(range::ALLOW_FINISHER);
            let window = combo_window || finisher_window;
            // Close the gap to the target at plyr V0 (the app's approach for a record without a
            // leap), then let the motion's root motion carry.
            let step = s.lunge_speed.min(s.lunge_left);
            s.lunge_left -= step;
            if s.motor.grounded {
                s.motor.vel = s.motor.forward() * step;
            } else {
                // The jump mover goes on during an air swing (FUN_0017b620 -> FUN_001832b8). The
                // stick is zeroed outside an idle range (FUN_001778e8: +0x160 bits 4, 8, 0x10 =
                // triggers 0, 1, 2), so the velocity brakes by 0.95 a tick (DAT_0037a50c); in an
                // idle range it steers as in a jump (0.9 blend, DAT_0037a508).
                let idle = body.trigger(range::GROUNDED) || body.trigger(range::AIR) || body.trigger(range::BLEND_TO_IDLE);
                if idle {
                    s.motor.air_velocity(dir, move_speed);
                } else {
                    s.motor.air_velocity(Vec3::ZERO, 0.0);
                }
                lunge_move = s.motor.forward() * step;
            }
            // Range triggers 22 and 28 turn Sora toward the target at the trigger's own rate
            // (`attack_turn_rate`); the target point is the target's first target shape, as the
            // game's FUN_0018efa0 [inferred: the game takes the point the target record names].
            if let Some(tp) = target_point.map(|p| p.pos).or(target_pos) {
                let gframe = body.gframe();
                let rates: Vec<f32> = body
                    .triggers()
                    .map(|t| {
                        t.active(gframe)
                            .filter(|r| r.id == range::TURN_TO_LOCK || r.id == range::TURN_TO_TARGET)
                            .filter_map(|r| attack_turn_rate(r.params.first().copied().unwrap_or(0), prty.turn_speed, tp - s.motor.pos, s.swing.as_ref()))
                            .collect()
                    })
                    .unwrap_or_default();
                for rate in rates {
                    s.motor.turn_to(tp - s.motor.pos, rate);
                }
            }
            // Gravity is held only while trigger 3, 24 or 30 covers the frame (FUN_0017b6e0); the
            // fall restarts when they end (`hover`, before the match).
            if !s.motor.grounded {
                airborne_dy = Some(if hover_now { 0.0 } else { s.motor.jump_dy(fall_max) });
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
                    // The jump mover goes on: no new fall (the record's queued motion is the fall, 4).
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
        State::Hurt if s.air_recoil => {
            // The air wall recoil runs on the jump mover the recoil launched (FUN_00185978): its
            // horizontal step FUN_001832b8 brakes by 0.95 a tick while the stick is zeroed outside an
            // idle range (FUN_001778e8) and steers with the 0.9 blend inside one; gravity is the jump
            // curve's. When motion 43 ends, the queued motion 4 (the fall) plays on the same mover.
            let idle = body.trigger(range::GROUNDED) || body.trigger(range::AIR) || body.trigger(range::BLEND_TO_IDLE);
            if idle {
                s.motor.air_velocity(dir, move_speed);
            } else {
                s.motor.air_velocity(Vec3::ZERO, 0.0);
            }
            if !s.motor.grounded {
                airborne_dy = Some(s.motor.jump_dy(fall_max));
            }
            if body.finished {
                s.air_recoil = false;
                s.state = if s.motor.grounded { State::Ground } else { State::Air };
                if s.state == State::Air {
                    body.play(motion(id::FALL), true, BLEND_FALL);
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
            // No revival while the mission holds the player: the original keeps Sora down through the
            // result (docs/systems/world-minigames/struggle.md). The game over (`menu_gameover`, docs/systems/ui/menus-gameover.md section 26) owns the knock-out
            // where the original has one; this stand-up is what is left where it does not (a mission with host
            // 1.317's bit, a drive form's end, no disc rows for the list).
            if s.timer > 180.0 && !locked && !s.game_over {
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
    // A jump, a leap off the ground or an attack started this tick from another state while
    // airborne: the jump mover moves Sora on this tick too. Without this the floor-follow below put
    // him back on the floor (`grounded` stayed set through a jump's whole rise) and ended a leap.
    if airborne_dy.is_none() && !s.motor.grounded && matches!(s.state, State::Attack { .. } | State::Air) {
        airborne_dy = Some(s.motor.jump_dy(fall_max));
    }

    // Root motion: yaw turns the facing; translation is rotated by the facing the motion
    // started from.
    let (root_t, root_yaw) = body.root_delta;
    s.motor.yaw = wrap_angle(s.motor.yaw + root_yaw);
    let start_yaw = s.motor.yaw - body.root.yaw;
    let root_move = Quat::from_rotation_y(start_yaw) * root_t;
    let mut delta = s.motor.vel + root_move.with_y(0.0) + lunge_move;
    if let (Some(p), Some(r)) = (s.push.as_mut(), reaction.as_ref()) {
        delta += p.step(&r.consts);
        if p.finished() {
            s.push = None;
        }
    }
    let dy = airborne_dy.map(|d| d + root_move.y);
    let was_air = airborne_dy.is_some();
    let before = s.motor.pos;
    let landed = s.motor.move_by(&world, delta, dy);
    s.last_move = s.motor.pos - before;
    if move_log() && (matches!(s.state, State::Attack { .. } | State::Air) || s.air_recoil) && !s.motor.grounded {
        info!(
            "move: {} {} f{:.0}: velocity ({:.1} {:.1}) horizontal {:.1}, moved ({:.1} {:.1} {:.1}){}",
            if s.state == State::Air {
                "airborne"
            } else if s.air_recoil {
                "air recoil"
            } else {
                "air swing"
            },
            body.tag(),
            body.frame,
            s.motor.vel.x,
            s.motor.vel.z,
            s.motor.vel.with_y(0.0).length(),
            s.last_move.x,
            s.last_move.y,
            s.last_move.z,
            if hover_now { ", gravity held" } else { "" }
        );
    }
    if landed && was_air && matches!(s.state, State::Attack { .. }) {
        // Landing in the middle of an air swing (WP-F1 item 10, user report: a landing lag after air
        // attacks). The jump mover's landing callback (vtable 0x35cbe0 +0x14 = FUN_0017bab8 ->
        // FUN_0017b9e0) plays the landing motion +0x104 (5, set by FUN_0017b1b0) with motion 0 queued
        // behind it, and sets +0x108 bit 0x8000000, unless +0x168 bit 0x10 (range trigger 49) is set or
        // +0x104 is negative. The landing motion replaces the swing, so the swing does not play out on
        // the floor; the ground state's input rules apply from this tick. [inferred] that the attack
        // command ends with its motion (the game's command object was not traced past the motion swap).
        if body.trigger(TRIGGER_NO_LANDING_MOTION_49) {
            if move_log() {
                info!("move: landed during {} f{:.0}: range trigger 49 keeps the swing (FUN_0017b9e0 plays no landing motion)", body.tag(), body.frame);
            }
        } else {
            if move_log() {
                info!("move: landed during {} f{:.0}: the landing motion replaces the swing (FUN_0017b9e0: slot 5, then idle)", body.tag(), body.frame);
            }
            s.state = State::Ground;
            s.timer = 0.0;
            s.hits = 0;
            s.finishers = 1;
            s.queued_attack = false;
            s.lunge_left = 0.0;
            s.motor.vel = Vec3::ZERO;
            body.restart(motion(id::LAND), false, 0.0);
            weapon.play(None, true, BLEND_LOCOMOTION);
        }
    } else if landed && was_air && s.state == State::Hurt && s.air_recoil {
        // The recoil's launch went through FUN_0017b1e0 -> FUN_0017b1b0, which sets the landing motion
        // +0x104 to 5: the jump mover's landing callback (FUN_0017bab8 -> FUN_0017b9e0) plays A005 with
        // motion 0 behind it in place of the rest of motion 43, unless range trigger 49 is active.
        s.air_recoil = false;
        if !body.trigger(TRIGGER_NO_LANDING_MOTION_49) {
            if move_log() {
                info!("move: landed during the air recoil {} f{:.0}: the landing motion replaces it (FUN_0017b9e0: slot 5, then idle)", body.tag(), body.frame);
            }
            s.state = State::Ground;
            s.timer = 0.0;
            s.motor.vel = Vec3::ZERO;
            body.restart(motion(id::LAND), false, 0.0);
            weapon.play(None, true, BLEND_LOCOMOTION);
        }
    } else if landed && was_air {
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
        State::Air | State::Glide => {
            s.state = State::Ground;
            if !moving {
                body.restart(land_slot, false, 0.0);
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
/// down by one, and Sora is pushed backwards along his own facing (on the ground) or launched up and
/// back on a new jump (in the air, `air_recoil_launch`). The spark and the sound are
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
        // Ground: FUN_001857b0, a decaying push backwards along the facing. In the air FUN_00185d30
        // calls FUN_00185978 instead: the jump mover is launched again with a jump of
        // `SwingConsts::recoil_height` and a backward velocity of power x HitBackToJump (up and back,
        // then the fall). Without the executable's constants the ground push stands in.
        let air = rc.airborne.then(|| s.swing.as_ref().and_then(|c| air_recoil_launch(s.motor.forward(), rc.push_power, data.sstm.hit_back_to_jump, c))).flatten();
        s.motor.vel = Vec3::ZERO;
        s.push = None;
        s.air_recoil = false;
        if let Some((vel, h)) = air {
            s.motor.vel = vel;
            s.motor.start_leap(h);
            s.air_recoil = true;
        } else {
            s.push = Push::new(-s.motor.forward(), rc.push_power, 0.0, &reaction.consts);
        }
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
        if let Some((vel, h)) = air {
            info!(
                "wall recoil in the air: jump mover launched (FUN_00185978): jump {h:.1} from y {:.1}, velocity ({:.2} {:.2}) = {:.1} a tick backwards (power {} x HitBackToJump {})",
                s.motor.pos.y,
                vel.x,
                vel.z,
                vel.length(),
                rc.push_power,
                data.sstm.hit_back_to_jump
            );
        }
        weapon.apply(&s.weapons, &mut anims);
        s.state = State::Hurt;
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
    (data, abilities, block, mut damaged): (Res<GameData>, Res<Abilities>, Res<crate::abilities::Abilities>, MessageWriter<crate::abilities::SoraDamaged>),
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
        // Leaf Bracer (FUN_00175340 drops the hit while vtable +0x60 = FUN_00175ad8 answers
        // untouchable; enemy hits are never heals).
        if s.leaf_bracer > 0.0 {
            if std::env::var_os("KH2_ABILITYLOG").is_some() {
                info!("abilities: Leaf Bracer drops atkp {} ({:.0} ticks left)", h.atkp, s.leaf_bracer);
            }
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
        // A hit that lands ends the air recoil's own movement (its reaction sets the motion and mover).
        s.air_recoil = false;
        let from = enemies.get(h.attacker).map(|e| e.stats).unwrap_or(s.stats);
        let useless = crate::combat::hit_is_ineffective(atk, &s.stats);
        // Defender (FUN_001a0610): at low HP the block's defense bonus lowers the damage too.
        let mut to = s.stats;
        to.defense += crate::abilities::defender_bonus(block.block(), s.hp, s.stats.max_hp);
        let dmg = if useless { data.damage(atk, &from, &to) } else { data.damage(atk, &from, &to).max(1) };
        // FUN_00185a68: a hit that would bring HP to the minimum (0 for Sora) asks vtable +0x58
        // (FUN_00176d78: Second Chance, Once More) whether he survives it with 1 HP.
        let second_chance = abilities.has_bit(crate::abilities::BIT_SECOND_CHANCE);
        let once_more = abilities.has_bit(crate::abilities::BIT_ONCE_MORE);
        let lethal = s.hp - dmg <= 0;
        let survives = lethal && crate::abilities::survives_lethal_hit(second_chance, once_more, s.hp, s.damage_motion);
        let dmg = crate::abilities::lethal_hit_damage(s.hp, 0, dmg, survives);
        if survives && std::env::var_os("KH2_ABILITYLOG").is_some() {
            info!(
                "abilities: {} keeps Sora at 1 HP: HP {} damage {dmg} (atkp {}, damage motion {:?})",
                if second_chance && s.hp >= 2 { "Second Chance" } else { "Once More" },
                s.hp,
                h.atkp,
                s.damage_motion
            );
        }
        s.hp = (s.hp - dmg).max(0);
        // The HP method's ability effects (Damage Drive, MP Rage): abilities::apply_damage_rewards.
        damaged.write(crate::abilities::SoraDamaged { amount: dmg });
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
                s.damage_motion = Some(if rig.mset.slot(motion as usize, pstate).is_some() { motion as usize } else { fallback as usize });
                s.motor.vel = Vec3::ZERO;
            }
            VictimAction::Class14 { motion, strength, face_attacker } => {
                let to = if face_attacker { -away } else { away };
                s.motor.yaw = to.x.atan2(to.z);
                s.push = Push::new(away, strength, 0.0, &reaction.consts);
                body.restart(first_slot(&[motion as usize, id::DAMAGE_S_FRONT_HIGH]), false, 2.0);
                weapon.restart(None, false, 2.0);
                s.state = State::Hurt;
                s.damage_motion = Some(if rig.mset.slot(motion as usize, pstate).is_some() { motion as usize } else { id::DAMAGE_S_FRONT_HIGH });
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
                s.damage_motion = Some(motion as usize);
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
    use crate::shared::sora::{ABILITY_GUARD, BASE_FINISHERS, BASE_HITS_AIR, BASE_HITS_GROUND};
    use kh2_anim::moveset::Chain;

    #[test]
    fn weapon_update_is_held_only_by_the_global_gate() {
        assert!(weapon_update_held(0x1000));
        assert!(weapon_update_held(0x1004));
        assert!(!weapon_update_held(0));
        assert!(!weapon_update_held(0xefff));
    }
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

    /// Sora's status block follows his record (`FUN_0019f920`): at level 1 it is the `lvup` row plus the record's bonus
    /// bytes plus the equipped items' `item_stat` rows, and after experience raises the level it is the higher row.
    #[test]
    fn sora_status_follows_the_record_level() {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt")).ok().map(|s| s.trim().to_string()));
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(std::path::Path::new(&iso));        let data = GameData::load(&disc);
        let Some(mut p) = crate::progress::Progress::start(&disc) else { return };
        let lvup = {
            let mut a = disc.0.lock().unwrap();
            kh2_tables::subfile(&mut a, &t::LVUP).ok().and_then(|b| kh2_tables::rt::decode_lvup::<t::Lvup>(&b)).expect("lvup")
        };
        let gear = |p: &crate::progress::Progress| p.equipment().iter().filter_map(|i| data.gear.get(i)).fold([0; 3], |a, g| [a[0] + g[0], a[1] + g[1], a[2] + g[2]]);
        let first = data.sora_status(&p);
        let p0 = p.clone();
        let (m, g) = (p.sora(), gear(&p));
        let row = &data.sora_levels[usize::from(m.level) - 1];
        assert_eq!(first.level, i32::from(m.level));
        assert_eq!(first.strength, i32::from(row.strength) + i32::from(m.strength) + g[0]);
        assert_eq!(first.magic, i32::from(row.magic) + i32::from(m.magic) + g[1]);
        assert_eq!(first.defense, i32::from(row.defense) + i32::from(m.defense) + g[2]);
        assert_eq!(first.max_hp, i32::from(m.hp_max));
        assert!(g[0] > 0, "the equipped keyblade adds attack");
        p.pay_experience(lvup[0][49].exp, &lvup, 1);
        let later = data.sora_status(&p);
        let (m2, g2) = (p.sora(), gear(&p));
        assert!(m2.level > m.level, "the payout raised the level");
        let row2 = &data.sora_levels[usize::from(m2.level) - 1];
        assert_eq!(later.level, i32::from(m2.level));
        assert_eq!(later.strength, i32::from(row2.strength) + i32::from(m2.strength) + g2[0]);
        assert!(later.strength + later.magic + later.defense > first.strength + first.magic + first.defense, "a higher level has higher stats");
        // AP: the lvup AP of the level, the record's `+8` and the items' AP; MP maximum: the record's `+7`.
        let ap = |p: &crate::progress::Progress| p.equipment().iter().filter_map(|i| data.gear_extra.get(i)).map(|e| e[0]).sum::<i32>();
        assert_eq!(first.ap, i32::from(row.ap) + i32::from(m.ap) + ap(&p0));
        assert_eq!(later.ap, i32::from(row2.ap) + i32::from(m2.ap) + ap(&p));
        assert!(later.ap >= first.ap, "AP does not fall with the level");
        assert!(row2.ap >= row.ap);
        assert_eq!(first.max_mp, i32::from(m.mp_max));
        assert_eq!(later.max_mp, i32::from(m2.mp_max));
        assert_eq!(first.max_mp, data.sora_max_mp, "the new game's record holds the plrp value");
        assert_eq!(first.weakness, data.resistances(&p0.equipment()));
    }

    /// The resistance bytes follow `FUN_001a0c08`: 100 each, every row scales them with a ceiling, in order. The
    /// disc check records a fact of the USA data: no item an actor can equip (`item` StatEntry 0..4 and 80..147)
    /// has a resistance byte other than 100, so Sora's resistances stay 100 there.
    #[test]
    fn resistances_scale_per_item_with_a_ceiling() {
        use crate::data::fold_resistances;
        assert_eq!(fold_resistances([].iter()), [100; 7], "no item: 100 everywhere");
        let fire80 = [0, 100, 80, 100, 100, 100, 100, 100];
        let all75 = [0, 75, 75, 75, 75, 75, 75, 75];
        assert_eq!(fold_resistances([fire80].iter()), [100, 80, 100, 100, 100, 100, 100]);
        assert_eq!(fold_resistances([fire80, fire80].iter())[1], 64, "ceil(80 * 80 / 100)");
        assert_eq!(fold_resistances([all75, all75].iter())[0], 57, "ceil(75 * 75 / 100) = 57");
        assert_eq!(fold_resistances([all75, fire80].iter())[1], 60, "75 then 80: 100 -> 75 -> 60");
        let iso = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt")).ok().map(|s| s.trim().to_string()));
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let data = GameData::load(&Disc::open(std::path::Path::new(&iso)));
        assert!(!data.gear_extra.is_empty());
        let all: Vec<u16> = data.gear_extra.keys().copied().collect();
        assert_eq!(data.resistances(&all), [100; 7], "no USA item changes a resistance");
    }

    /// The variant rule of `FUN_0017da30` (pure part): weapon out with the game's battle flag clear is the
    /// field-with-weapon variant, with it set the battle one, no weapon the field variant without.
    #[test]
    fn motion_state_picks_the_armed_field_variant_out_of_battle() {
        assert_eq!(motion_state(true, true), PlayerState::BattleWeapon);
        assert_eq!(motion_state(true, false), PlayerState::FieldWeapon);
        assert_eq!(motion_state(false, false), PlayerState::FieldNoWeapon);
        assert_eq!(motion_state(false, true), PlayerState::FieldNoWeapon);
    }

    /// The four variant rows of `FUN_0017da30` (bytes at 0x37a5c8 of `SLUS_210.05`) are `PlayerState`'s fallback
    /// orders, and on P_EX100 the keyblade-out field walk and run are the A001 / A002 variant-3 slots (7 and 11),
    /// the battle ones the variant-0 slots (4 and 8). Skips without a disc.
    #[test]
    fn armed_field_run_is_the_variant_three_slot_of_the_disc() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let exe = disc.read_root("SLUS_210.05").expect("SLUS_210.05");
        let at = 0x37a5c8 - 0xff000;
        let rows: Vec<[u8; 4]> = (0..4).map(|r| exe[at + r * 4..at + r * 4 + 4].try_into().unwrap()).collect();
        assert_eq!(rows, [[0, 1, 3, 2], [1, 0, 2, 3], [2, 3, 1, 0], [3, 2, 0, 1]]);
        // The state the app asks for -> the row the game takes (0 = battle + weapon, 3 = field + weapon, 2 = field).
        for (st, row) in [(PlayerState::BattleWeapon, 0), (PlayerState::FieldWeapon, 3), (PlayerState::FieldNoWeapon, 2)] {
            let mset = kh2_anim::Mset::parse(&disc.read("obj/P_EX100.mset").expect("mset")).expect("parse");
            let want: Vec<usize> = rows[row].iter().map(|&b| b as usize).collect();
            let got = |i: usize| mset.slot(i, st);
            let first = |i: usize| want.iter().map(|&r| r + i * 4).find(|&s| mset.has(s));
            for i in [id::IDLE, id::WALK, id::RUN] {
                assert_eq!(got(i), first(i), "{st:?} motion {i}");
            }
        }
        let mset = kh2_anim::Mset::parse(&disc.read("obj/P_EX100.mset").unwrap()).unwrap();
        assert_eq!(mset.slot(id::WALK, PlayerState::FieldWeapon), Some(7));
        assert_eq!(mset.slot(id::RUN, PlayerState::FieldWeapon), Some(11));
        assert_eq!(mset.slot(id::WALK, PlayerState::BattleWeapon), Some(4));
        assert_eq!(mset.slot(id::RUN, PlayerState::BattleWeapon), Some(8));
        assert_eq!(mset.slot(id::RUN, PlayerState::FieldNoWeapon), Some(10));
        // The armed run is a slot of its own, with the run's tag.
        assert_eq!(mset.get(11).map(|s| s.tag.as_str()), Some("A002"));
    }

    /// The air records carry a leap and the ground combo none (`ptya` from the user's disc).
    #[test]
    fn air_attacks_leap_and_ground_attacks_do_not() {
        let iso = std::env::var("KH2_ISO").ok();
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let Ok(p) = kh2_tables::subfile(&mut a, &t::PTYA) else { return };
        let leaps = AttackLeaps::parse(&p);
        let Some(m) = real_moves() else { return };
        // A300..A304 (keys 5, 4, 6, 2, 3): no leap, the swing stops Sora on the ground.
        for k in [5, 4, 6, 2, 3] {
            assert_eq!(leaps.of(m.by_key(k).unwrap()).height, 0.0, "key {k}");
        }
        // A330..A333 (keys 9, 10, 8, 7): a leap with a positive height and a forward speed.
        for k in [9, 10, 8, 7] {
            let l = leaps.of(m.by_key(k).unwrap());
            assert!(l.height > 0.0 && l.max_speed > 0.0 && l.min_height > 0.0 && l.reach >= l.min_height, "key {k}: {l:?}");
        }
    }

    #[test]
    fn leap_launch_follows_fun_00176050() {
        let l = Leap { height: 21.0, reach: 71.0, min_height: 21.0, min_speed: -8.0, max_speed: 8.0, gap: 85.0 };
        // No target: the record's height, along the facing, at the top speed.
        let (d, s, h) = leap_launch(&l, 1, 185.0, Vec3::new(0.0, 0.0, 1.0), None);
        assert_eq!((d, s, h), (Vec3::Z, 8.0, 21.0));
        // A target level with Sora: the lowest height; a far target is reached at the top speed.
        let (d, s, h) = leap_launch(&l, 1, 185.0, Vec3::Z, Some((Vec3::new(1000.0, 0.0, 0.0), 60.0)));
        assert_eq!((d, s, h), (Vec3::X, 8.0, 21.0));
        // A target 50 above: height 50; the gap over the rise time sqrt(2h/g).
        let (_, s, h) = leap_launch(&l, 1, 185.0, Vec3::Z, Some((Vec3::new(0.0, 50.0, 30.0), 60.0)));
        let want = (30.0 - 60.0 + 85.0) / (2.0 * 50.0 / crate::motor::G).sqrt();
        assert_eq!(h, 50.0);
        assert!((s - want).abs() < 1e-4, "{s} {want}");
        // Higher than the reach: the reach; flag bit 1 adds the jump height to it.
        assert_eq!(leap_launch(&l, 1, 185.0, Vec3::Z, Some((Vec3::new(0.0, 500.0, 30.0), 60.0))).2, 71.0);
        assert_eq!(leap_launch(&l, 3, 185.0, Vec3::Z, Some((Vec3::new(0.0, 500.0, 30.0), 60.0))).2, 256.0);
        // A target behind the reach of a short gap: the speed is clamped to the lowest (backwards).
        assert_eq!(leap_launch(&l, 1, 185.0, Vec3::Z, Some((Vec3::new(0.0, 0.0, 1.0), 500.0))).1, -8.0);
    }

    /// A motion set with A180 at motion id 209 (the state variants differ by the low two bits of the slot).
    fn mset_with(tag_slots: &[(usize, &str)]) -> kh2_anim::Mset {
        use kh2_anim::{motion::{Data, Motion}, Slot, Triggers};
        let n = tag_slots.iter().map(|x| x.0).max().unwrap_or(0) + 1;
        let mut slots: Vec<Option<Slot>> = (0..n).map(|_| None).collect();
        for &(i, tag) in tag_slots {
            let motion = Motion { frame_start: 0.0, frame_end: 26.0, fps: 30.0, frame_return: 0.0, data: Data::Raw { bones: 0, frames: Vec::new() } };
            slots[i] = Some(Slot { tag: tag.to_string(), motion: Some(motion), triggers: Triggers::default() });
        }
        kh2_anim::Mset { slots }
    }

    #[test]
    fn dodge_roll_plays_a180_motion_209() {
        // User report 2026-10-24: the roll is A180, motion id 209 (slot 209 * 4 + variant).
        let m = mset_with(&[(203 * 4 + 2, "A150"), (209 * 4 + 2, "A180"), (209 * 4 + 3, "A180")]);
        let s = dodge_roll_slot(&m, PlayerState::FieldNoWeapon).expect("A180 is in the set");
        assert_eq!(s / 4, DODGE_ROLL_ID);
        assert_eq!(m.get(s).unwrap().tag, "A180");
        assert_eq!(dodge_roll_slot(&m, PlayerState::FieldWeapon), Some(209 * 4 + 3), "the state variant is followed");
        // The tag wins over a stray id: a set whose motion 209 is another motion falls back to the A180 tag elsewhere.
        let m = mset_with(&[(209 * 4 + 2, "A999"), (300 * 4, "A180")]);
        assert_eq!(dodge_roll_slot(&m, PlayerState::FieldNoWeapon), Some(1200));
        // No A180 (the USA disc): none, the caller keeps the Quick Run start motion.
        assert_eq!(dodge_roll_slot(&mset_with(&[(203 * 4 + 2, "A150")]), PlayerState::FieldNoWeapon), None);
    }

    #[test]
    fn square_while_moving_tap_rolls_and_hold_runs() {
        // The threshold is the Final Mix trigger's tick (type 34 at tick 10 of the roll motion).
        assert_eq!(TAP_TICKS, 10.0);
        // Released within the tap window: a roll that runs TAP_TICKS ticks, then stops.
        assert_eq!(dash_step(Dash::Pending, false, 3.0, 1, 11.0), (Dash::Roll, false));
        assert_eq!(dash_step(Dash::Pending, false, TAP_TICKS - 1.0, 1, 11.0), (Dash::Roll, false));
        assert_eq!(dash_step(Dash::Roll, false, TAP_TICKS - 1.0, 1, 11.0), (Dash::Roll, false));
        assert_eq!(dash_step(Dash::Roll, true, TAP_TICKS, 1, 11.0), (Dash::Roll, true));
        // Still held: undecided until the window ends, then the Quick Run.
        assert_eq!(dash_step(Dash::Pending, true, TAP_TICKS - 1.0, 1, 3.0), (Dash::Pending, false));
        assert_eq!(dash_step(Dash::Pending, true, TAP_TICKS, 1, 2.0), (Dash::Run, false));
        // The Quick Run runs while held and while its clock lasts (FUN_001c1660).
        assert_eq!(dash_step(Dash::Run, true, 13.0, 1, 1.0), (Dash::Run, false));
        assert_eq!(dash_step(Dash::Run, true, 14.0, 1, 0.0), (Dash::Run, true));
        assert_eq!(dash_step(Dash::Run, false, 13.0, 4, 20.0), (Dash::Run, true));
        // Held without Quick Run learned: it ends as a roll.
        assert_eq!(dash_step(Dash::Pending, true, TAP_TICKS, 0, 2.0), (Dash::Roll, true));
    }

    /// WP-F1 item 2: while a mission layout locks the players (FUN_001b7d50) or the fight is stopped
    /// (FUN_001b9940), the controller sees no stick and no press, pending presses included.
    #[test]
    fn a_locked_pad_reads_as_untouched() {
        let mut pad = Pad { dir: Vec3::Z, stick: 1.0, jump_held: true, square_held: true, attack: 2, jump: 1, square: 1, lock: 1, ..Default::default() };
        assert!(pad.any_input());
        pad.lock();
        assert!(!pad.any_input());
        assert!(!Pad::take(&mut pad.attack) && !Pad::take(&mut pad.jump) && !Pad::take(&mut pad.square) && !Pad::take(&mut pad.lock));
        assert_eq!((pad.dir, pad.stick, pad.jump_held, pad.square_held), (Vec3::ZERO, 0.0, false, false));
        // The viewer keys are the harness's, not the game's: they stay.
        pad.viewer = 1;
        pad.lock();
        assert!(Pad::take(&mut pad.viewer));
    }

    /// The decision table of a Square press on the ground: still / moving x Dodge Roll x Quick Run
    /// x tap / hold (FM 0x178eb0 order, docs/systems/foundations/final-mix.md; the tap/hold part is `dash_step`).
    #[test]
    fn square_decision_table() {
        use SquareGround::{Guard, QuickRun, Roll};
        for roll in [0u8, 1] {
            for run in [0u8, 1] {
                // Standing still: always the guard, whatever is learned.
                assert_eq!(square_ground(false, roll, run), Guard, "still, roll {roll}, run {run}");
            }
        }
        // Moving, neither learned: the guard (USA rule).
        assert_eq!(square_ground(true, 0, 0), Guard);
        // Moving, Quick Run only: Quick Run at the press (USA rule), no tap window.
        assert_eq!(square_ground(true, 0, 1), QuickRun);
        // Moving with Dodge Roll: always the roll start, with or without Quick Run.
        assert_eq!(square_ground(true, 1, 0), Roll);
        assert_eq!(square_ground(true, 1, 1), Roll);
        // The roll start resolves by tap/hold: a tap rolls; a hold at tick 10 runs only with Quick Run.
        assert_eq!(dash_step(Dash::Pending, false, 4.0, 1, 11.0).0, Dash::Roll, "roll + run, tap");
        assert_eq!(dash_step(Dash::Pending, true, TAP_TICKS, 1, 11.0).0, Dash::Run, "roll + run, hold");
        assert_eq!(dash_step(Dash::Pending, false, 4.0, 0, 11.0).0, Dash::Roll, "roll only, tap");
        assert_eq!(dash_step(Dash::Pending, true, TAP_TICKS, 0, 11.0), (Dash::Roll, true), "roll only, hold: still a roll");
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
        let mesh = kh2_model::Mesh { positions, uvs: vec![[0.0; 2]; n], colors: vec![[1.0; 4]; n], indices, texture: 0, transparent: false, joints: vec![], weights: vec![], uv_scroll: 0 };
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
        // WP-A1: the view carries the block's bits above 127 too (Once More 128).
        for x in [402, 415, 416] {
            ab.equip_ability(x);
        }
        let v = Abilities::from_block(&ab);
        for b in [abilities::BIT_LEAF_BRACER, abilities::BIT_SECOND_CHANCE, abilities::BIT_ONCE_MORE] {
            assert!(v.has_bit(b), "bit {b}");
        }
        assert!(!v.has_bit(abilities::BIT_AUTO_LIMIT) && !v.has_bit(300));
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

    /// The user's disc: the archive (None, test skipped, without one).
    fn open_disc() -> Option<kh2_archive::Archive> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        kh2_archive::Archive::open(&iso).map_err(|e| eprintln!("skipped: {iso}: {e}")).ok()
    }

    /// One tick of the jump mover while the stick is zeroed (FUN_0017b620: FUN_001832b8 brakes, then
    /// FUN_0017b6e0 gives the curve's step), applied to `m` without a world. Returns the step.
    fn mover_tick(m: &mut Motor, fall_max: f32) -> Vec3 {
        m.air_velocity(Vec3::ZERO, 0.0);
        let dy = m.jump_dy(fall_max);
        let step = m.vel.with_y(dy);
        m.pos += step;
        step
    }

    /// The air swing's movement with the disc's A330 record (key 9): FUN_00176050 launches the jump
    /// mover (height +0xc 21, speed +0x1c 8 along the facing without a target); the mover rises in
    /// sqrt(2h/g) ticks (FUN_0017bee0, g = DAT_0037a518 = DAT_0037a538 = 20/49), brakes 0.95 a tick
    /// (DAT_0037a50c) and then falls on the same curve; nothing holds gravity (A330 has no trigger
    /// 3, 24 or 30). After the 60 ticks of A330 Sora is far below his start: the "stall" of one
    /// swing is the leap's 21 units and ~10 ticks of rise.
    #[test]
    fn air_swing_path_follows_the_jump_mover_with_the_disc_leap() {
        let Some(mut a) = open_disc() else { return };
        let ptya = kh2_tables::subfile(&mut a, &t::PTYA).expect("ptya");
        let moves = real_moves().expect("moveset");
        let a330 = moves.by_key(9).expect("key 9").clone();
        assert_eq!(a330.motion, 181);
        let leap = AttackLeaps::parse(&ptya).of(&a330);
        let (d, speed, h) = leap_launch(&leap, a330.flags, 185.0, Vec3::Z, None);
        assert_eq!((d, speed, h), (Vec3::Z, 8.0, 21.0), "{leap:?}");
        let mut m = Motor::default();
        m.vel = d * speed;
        m.start_leap(h);
        let rise = m.jump.rise;
        assert!((rise - (2.0 * 21.0 / crate::motor::G).sqrt()).abs() < 1e-4 && (rise - 10.14).abs() < 0.01, "{rise}");
        let mut top: f32 = 0.0;
        let mut top_tick = 0;
        let mut z = 0.0;
        for tick in 1..=60 {
            let s = mover_tick(&mut m, 1.0e6);
            z += s.z;
            if m.pos.y > top {
                top = m.pos.y;
                top_tick = tick;
            }
            // The horizontal step is the braked velocity: 8 x 0.95^tick.
            assert!((s.z - 8.0 * 0.95f32.powi(tick)).abs() < 1e-3, "tick {tick}: {}", s.z);
        }
        assert!((top - 21.0).abs() < 0.05 && top_tick == 10, "apex {top} at tick {top_tick}");
        let want_z = 8.0 * 0.95 * (1.0 - 0.95f32.powi(60)) / 0.05;
        assert!((z - want_z).abs() < 0.01, "{z} {want_z}");
        let want_y = 21.0 - 0.5 * crate::motor::G * (60.0 - rise) * (60.0 - rise);
        assert!((m.pos.y - want_y).abs() < 0.01, "{} {want_y}", m.pos.y);
        eprintln!("A330 without a target: apex {top:.2} at tick {top_tick}, after 60 ticks y {:.1}, z {z:.1}", m.pos.y);
    }

    /// The air wall recoil with the executable's constants and the disc's sstm row: FUN_00175668 ->
    /// FUN_00185d30 (power: DAT_0037a4a4 = -1 selects DAT_0037a964) -> FUN_00185978: a jump of
    /// DAT_0037a960 from the current height, backwards (DAT_0037a968 x facing) at power x
    /// HitBackToJump a tick, braked 0.95 a tick by the jump mover, then the fall.
    #[test]
    fn air_wall_recoil_jumps_up_and_back_with_the_disc_constants() {
        let Some(mut a) = open_disc() else { return };
        let Ok(exe) = a.iso().read("SLUS_210.05") else {
            eprintln!("skipped: no executable on the disc");
            return;
        };
        let c = SwingConsts::from_exe(&exe).expect("swing constants");
        assert_eq!((c.recoil_height, c.recoil_facing, c.turn_min_gap), (50.0, -1.0, 50.0));
        assert!((c.turn_unit - std::f32::consts::PI / 180.0).abs() < 1e-7, "{}", c.turn_unit);
        let consts = crate::reaction_rules::ExeConsts::from_exe(&exe).expect("exe constants");
        let power = rules::recoil_power(consts.wall_power, &consts);
        assert_eq!(power, 50.0);
        let sstm = kh2_tables::load::<t::PrefSstm>(&mut a, &t::PREF_SSTM).expect("sstm").rows.into_iter().next().expect("sstm row");
        let hbtj = sstm.hit_back_to_jump;
        assert!(hbtj > 0.0, "{hbtj}");
        // Facing +z: launched along -z at 50 x HitBackToJump, with a jump of 50.
        let (vel, h) = air_recoil_launch(Vec3::Z, power, hbtj, &c).expect("launch");
        assert!((vel - Vec3::new(0.0, 0.0, -power * hbtj)).length() < 1e-4, "{vel}");
        assert_eq!(h, 50.0);
        // No power, no push (FUN_00185d30 tests the power above 0).
        assert!(air_recoil_launch(Vec3::Z, 0.0, hbtj, &c).is_none());
        // The path: up 50 in sqrt(100/g) = 15.65 ticks while moving back, then down.
        let mut m = Motor { pos: Vec3::new(0.0, 300.0, 0.0), ..Motor::default() };
        m.vel = vel;
        m.start_leap(h);
        assert!(!m.jump.release_cuts, "the recoil's rise is not cut by the jump button");
        let mut top: f32 = 0.0;
        let mut top_tick = 0;
        let mut back = 0.0;
        for tick in 1..=40 {
            let s = mover_tick(&mut m, 1.0e6);
            back -= s.z;
            if m.pos.y > top {
                top = m.pos.y;
                top_tick = tick;
            }
        }
        // The eased rise is flat at its top: tick 15 is 0.0003 below the apex, tick 16 already past it.
        assert!((top - 350.0).abs() < 0.05 && top_tick == 15, "apex {top} at tick {top_tick}");
        let want_back = power * hbtj * 0.95 * (1.0 - 0.95f32.powi(40)) / 0.05;
        assert!((back - want_back).abs() < 0.01 && back > 0.0, "{back} {want_back}");
        assert!(m.pos.y < 300.0, "falling after the rise: {}", m.pos.y);
        eprintln!("air recoil: HitBackToJump {hbtj}, speed {:.2} a tick, apex +{:.1} at tick {top_tick}, {back:.1} back after 40 ticks", vel.length(), top - 300.0);
    }

    /// Triggers 22 and 28 turn at the trigger's parameter in degrees a tick (A330: 28 over global
    /// frames 0..23 with 15), the turn speed for a parameter of 0, and not at all within 50 units.
    #[test]
    fn attack_turn_rate_is_the_trigger_parameter() {
        let c = SwingConsts { recoil_height: 50.0, recoil_facing: -1.0, turn_min_gap: 50.0, turn_unit: std::f32::consts::PI / 180.0 };
        let far = Vec3::new(0.0, 80.0, 300.0);
        assert!((attack_turn_rate(15, 0.2, far, Some(&c)).unwrap() - 15f32.to_radians()).abs() < 1e-6);
        assert_eq!(attack_turn_rate(0, 0.2, far, Some(&c)), Some(0.2));
        // Within the gap (horizontal; the height does not count): no turn.
        assert_eq!(attack_turn_rate(15, 0.2, Vec3::new(30.0, 400.0, 30.0), Some(&c)), None);
        assert_eq!(attack_turn_rate(15, 0.2, far, None), None);
        // The disc's A330 carries trigger 28 over 0..23 with parameter 15.
        let Some(mut a) = open_disc() else { return };
        let mset = kh2_anim::Mset::parse(&a.read("obj/P_EX100.mset").expect("mset")).expect("parse");
        let a330 = mset.get(181 * 4).expect("A330 slot");
        let r = a330.triggers.ranges.iter().find(|r| r.id == range::TURN_TO_LOCK).expect("trigger 28");
        assert_eq!((r.start, r.end, r.params.first().copied()), (0, 23, Some(15)));
    }}
