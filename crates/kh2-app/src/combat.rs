//! Hit detection driven by motion triggers.
//!
//! An attack is live while a motion's range trigger 10 (attack) or 33 (combo attack) covers the
//! current frame; its params are (atkp id, collision group[, combo id]). The hit volumes are the
//! owner model's object-collision shapes of that group with kind ATTACK, placed on their bones.
//! Targets are the BATTLE/TARGET shapes of the other team. Each trigger hits a target once.
//!
//! An attack row with the BG Hit flag is also swept against terrain that blocks attacks; touching
//! it is a `WallHit`, which the owner answers with the generic recoil (see `docs/hit-reaction.md`).
//! A swing that already hit a target does not test walls. Victim reactions are decided by the pure
//! rules in `reaction_rules`; this module holds the data they need and the messages between
//! detection, victims, effects and sounds.

use std::collections::HashSet;

use bevy::prelude::*;
use kh2_anim::triggers::range;
use kh2_model::collision::{kind, Shape};
use kh2_tables::tables::{self as t, Atkp};

use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::data::{GameData, Stats};
use crate::reaction_rules::{self as rules, ExeConsts, PatnRow, ReactionDefaults, ReflectFields};
use crate::schedule::{Frame, Sim};
use crate::skel::Animator;
use crate::SCALE;

/// The fixed sound a wall hit plays (bank 500, index 10; FUN_0018ca78 passes 0x7a12a).
pub const WALL_HIT_SOUND: u32 = 500010;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        let data = match (app.world().get_resource::<Disc>(), app.world().get_resource::<GameData>()) {
            (Some(disc), Some(game)) => ReactionData::load(disc, game),
            _ => None,
        };
        match data {
            Some(d) => {
                app.insert_resource(d);
            }
            None => warn!("reaction data (executable constants, patn) not readable: no recoil, pushes or blows"),
        }
        app.add_message::<Hit>()
            .add_message::<WallHit>()
            .add_message::<IneffectiveHit>()
            .add_systems(FixedUpdate, detect_hits.in_set(Sim::Detect))
            .add_systems(FixedUpdate, landed_hit_sounds.in_set(Sim::Sounds))
            .add_systems(Update, draw_volumes.in_set(Frame::Volumes));
    }
}

/// What the hit-reaction rules read from the user's disc: constants the executable keeps, the
/// `sstm` defaults and the `patn` status table.
#[derive(Resource)]
pub struct ReactionData {
    pub consts: ExeConsts,
    pub defaults: ReactionDefaults,
    pub patn: Vec<PatnRow>,
}

impl ReactionData {
    fn load(disc: &Disc, game: &GameData) -> Option<ReactionData> {
        let consts = ExeConsts::from_exe(&disc.read_root("SLUS_210.05")?)?;
        let patn = {
            let mut a = disc.0.lock().ok()?;
            kh2_tables::subfile(&mut a, &t::PATN).ok().and_then(|b| rules::parse_patn(&b)).unwrap_or_default()
        };
        Some(ReactionData { consts, defaults: ReactionDefaults::from(&game.sstm), patn })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Team {
    Player,
    Enemy,
}

/// Put on a character root: which bodies carry its attacks/hurtboxes.
#[derive(Component)]
pub struct Fighter {
    pub team: Team,
    /// Bodies whose motion triggers + shapes can attack (Sora: body and keyblade).
    pub attack_bodies: Vec<Entity>,
    /// Body whose BATTLE/TARGET shapes can be hit.
    pub hurt_body: Entity,
    /// Hits already landed: (target, attack body, trigger start frame).
    pub landed: HashSet<(Entity, Entity, i16)>,
    /// When false the fighter cannot be hit (dodge, sunk Shadow, death...).
    pub hittable: bool,
}

/// Status bits of a victim for the `patn` conversion table (actor words +0xabc, +0xac0, +0xac4 of
/// the game). Nothing in the app sets a status yet, so victims carry none; an armoured target
/// would add this component.
#[derive(Component, Clone, Copy, Default, Debug)]
pub struct ReactionStatus(pub u32);

/// A player attack volume touching a wall that blocks attacks, for an attack row with BG Hit.
#[derive(Message, Clone, Copy, Debug)]
pub struct WallHit {
    pub attacker: Entity,
    pub body: Entity,
    pub atkp: i16,
    pub point: Vec3,
    /// The volume belongs to an attached object (the keyblade) rather than the character itself.
    /// The game plays the wall sound only when the volume's owner has flag 0x40 in its word at
    /// +0x588, which is what the weapon class sets (inferred, see the doc).
    pub weapon_volume: bool,
}

#[derive(Message, Clone, Copy, Debug)]
pub struct Hit {
    pub attacker: Entity,
    pub target: Entity,
    pub atkp: i16,
    /// Attacker → target, horizontal.
    pub dir: Vec3,
    pub point: Vec3,
}

/// A hit whose damage multipliers for the attack's element and in general multiply to zero: the
/// game marks it ineffective (hit record flag 4), gives it no reaction and starts effect 0x12
/// instead of the normal spark (FUN_0018b618, FUN_0018d090). Written by the victim's receiver.
///
/// TODO(WP-1F2): fx.rs should start effect `rules::INEFFECTIVE_SPARK_EFFECT` (0x12) at `point` for
/// each of these, and `fx::hit_sparks` should skip the normal `EffectOnHit` spark for the same hit.
#[derive(Message, Clone, Copy, Debug)]
pub struct IneffectiveHit {
    pub attacker: Entity,
    pub target: Entity,
    pub atkp: i16,
    /// Where the effect starts; read by the effect hook, not yet by anything.
    #[allow(dead_code)]
    pub point: Vec3,
}

/// Whether `victim`'s multipliers make `atk` ineffective (the bytes at record +9 and +0x1ae times
/// +0x1ae in the game; percentages here).
pub fn hit_is_ineffective(atk: &Atkp, victim: &Stats) -> bool {
    rules::is_ineffective(victim.weakness[(atk.element as usize).min(6)], victim.weakness[6])
}

/// A world-space capsule (segment + radius).
#[derive(Clone, Copy, Debug)]
pub struct Volume {
    pub a: Vec3,
    pub b: Vec3,
    pub r: f32,
}

pub fn shape_volume(s: &Shape, anim: &Animator, body: &GlobalTransform) -> Volume {
    let m = match s.bone {
        Some(b) => anim.bone_world(body, b),
        None => body.to_matrix(),
    };
    let c = m.transform_point3(Vec3::from(s.offset));
    let r = s.radius.abs() * SCALE;
    // Columns and ellipsoids extend upward by `height`; spheres/cubes are treated as spheres.
    let h = if matches!(s.shape, 0 | 1) { s.height.max(0.0) * SCALE } else { 0.0 };
    Volume { a: c, b: c + Vec3::Y * h, r }
}

fn seg_dist(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> f32 {
    let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (s, t) = if a <= 1e-8 && e <= 1e-8 {
        (0.0, 0.0)
    } else if a <= 1e-8 {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= 1e-8 {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let den = a * e - b * b;
            let mut s = if den > 1e-8 { ((b * f - c * e) / den).clamp(0.0, 1.0) } else { 0.0 };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    ((p1 + d1 * s) - (p2 + d2 * t)).length()
}

pub fn overlap(x: &Volume, y: &Volume) -> bool {
    seg_dist(x.a, x.b, y.a, y.b) <= x.r + y.r
}

/// Live attack volumes of one body: (atkp id, trigger start, volume).
pub fn live_attacks(anim: &Animator, body: &GlobalTransform) -> Vec<(i16, i16, Volume)> {
    let Some(tr) = anim.triggers() else { return Vec::new() };
    let mut out = Vec::new();
    for r in tr.active(anim.gframe()).filter(|r| r.id == range::ATTACK || r.id == range::ATTACK_COMBO) {
        let (Some(&atkp), Some(&group)) = (r.params.first(), r.params.get(1)) else { continue };
        for s in anim.rig.model.shapes.iter().filter(|s| s.kind == kind::ATTACK && s.group as i16 == group) {
            out.push((atkp, r.start, shape_volume(s, anim, body)));
        }
    }
    out
}

pub fn hurt_volumes(anim: &Animator, body: &GlobalTransform) -> Vec<Volume> {
    anim.rig.model.shapes.iter().filter(|s| s.kind == kind::BATTLE || s.kind == kind::TARGET).map(|s| shape_volume(s, anim, body)).collect()
}

#[allow(clippy::too_many_arguments)]
pub fn detect_hits(
    mut fighters: Query<(Entity, &mut Fighter, &GlobalTransform)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
    mut hits: MessageWriter<Hit>,
    mut wall_hits: MessageWriter<WallHit>,
    world: Option<Res<crate::world::World>>,
    data: Res<GameData>,
    mut skipped: Local<Option<(Entity, i16, i16)>>,
) {
    let log = std::env::var_os("KH2_HITLOG").is_some();
    let mut skipped_now = false;
    // Snapshot defenders first.
    let defenders: Vec<(Entity, Team, Vec<Volume>, Vec3)> = fighters
        .iter()
        .filter(|(_, f, _)| f.hittable)
        .filter_map(|(e, f, g)| bodies.get(f.hurt_body).ok().map(|(a, bg)| (e, f.team, hurt_volumes(a, bg), g.translation())))
        .collect();
    for (e, mut f, g) in &mut fighters {
        let mut still_live = HashSet::new();
        let hurt_body = f.hurt_body;
        for &body in &f.attack_bodies.clone() {
            let Ok((anim, bg)) = bodies.get(body) else { continue };
            for (atkp, start, vol) in live_attacks(anim, bg) {
                // Targets first: the game sweeps a volume against hurt volumes and only then
                // against terrain, and a volume that has hit something skips the terrain test.
                for (te, team, hurts, tpos) in &defenders {
                    if *team == f.team || *te == e {
                        continue;
                    }
                    let key = (*te, body, start);
                    still_live.insert(key);
                    if f.landed.contains(&key) {
                        continue;
                    }
                    if hurts.iter().any(|h| overlap(&vol, h)) {
                        f.landed.insert(key);
                        let dir = (*tpos - g.translation()).with_y(0.0).normalize_or(Vec3::Z);
                        hits.write(Hit { attacker: e, target: *te, atkp, dir, point: vol.a });
                    }
                }
                if let (Some(world), Team::Player) = (world.as_ref(), f.team) {
                    // Each attack window bounces off a wall once.
                    let key = (Entity::PLACEHOLDER, body, start);
                    still_live.insert(key);
                    let hit_a_target = f.landed.iter().any(|k| k.0 != Entity::PLACEHOLDER && k.1 == body && k.2 == start);
                    if f.landed.contains(&key) || hit_a_target {
                        continue;
                    }
                    let Some((point, _normal)) = world.touching_m(vol.a, vol.b, vol.r, kh2_map::flags::ATTACK_COLLIDE) else { continue };
                    // The terrain test runs only for rows with BG Hit (atkp Flags bit 0).
                    if data.atkp(atkp).is_some_and(|a| a.flags & rules::ATKP_FLAG_BG_HIT != 0) {
                        f.landed.insert(key);
                        wall_hits.write(WallHit { attacker: e, body, atkp, point, weapon_volume: body != hurt_body });
                    } else if log {
                        skipped_now = true;
                        if *skipped != Some((body, start, atkp)) {
                            *skipped = Some((body, start, atkp));
                            info!("wall test not run: atkp {atkp} has no BG Hit, volume touches a wall at {point:?}");
                        }
                    }
                }
            }
        }
        // Forget windows that have closed so the next swing can hit again.
        f.landed.retain(|k| still_live.contains(k));
    }
    if !skipped_now {
        *skipped = None;
    }
}

/// The attacker's `ReflectHitSound` when its hit lands (FUN_0018c6d0 -> FUN_00197008): a sound
/// number of 500000 and up is played from the owning object's own bank, which for Sora is the
/// keyblade's. Not played for ineffective hits.
fn landed_hit_sounds(
    mut hits: MessageReader<Hit>,
    mut ineffective: MessageReader<IneffectiveHit>,
    data: Res<GameData>,
    fighters: Query<&Fighter>,
    anims: Query<&Animator>,
    mut out: MessageWriter<PlaySe>,
) {
    let cancelled: HashSet<(Entity, Entity, i16)> = ineffective.read().map(|i| (i.attacker, i.target, i.atkp)).collect();
    for h in hits.read() {
        if cancelled.contains(&(h.attacker, h.target, h.atkp)) {
            continue;
        }
        let Some(sound) = data.atkp(h.atkp).and_then(|a| ReflectFields::from_atkp(a).hit_sound()) else { continue };
        let apdx = fighters.get(h.attacker).ok().and_then(|f| f.attack_bodies.first().copied()).and_then(|b| anims.get(b).ok()).and_then(|a| a.rig.apdx.clone());
        out.write(PlaySe::number(sound, apdx));
    }
}

/// Debug: draw live attack volumes (red) and hurt volumes (green) with F4.
pub fn draw_volumes(
    keys: Res<ButtonInput<KeyCode>>,
    mut on: Local<bool>,
    mut gizmos: Gizmos,
    fighters: Query<&Fighter>,
    bodies: Query<(&Animator, &GlobalTransform)>,
) {
    if keys.just_pressed(KeyCode::F4) {
        *on = !*on;
    }
    if !*on {
        return;
    }
    for f in &fighters {
        if let Ok((a, g)) = bodies.get(f.hurt_body) {
            for v in hurt_volumes(a, g) {
                gizmos.sphere(Isometry3d::from_translation(v.a), v.r, Color::srgb(0.2, 1.0, 0.3));
            }
        }
        for b in &f.attack_bodies {
            if let Ok((a, g)) = bodies.get(*b) {
                for (_, _, v) in live_attacks(a, g) {
                    gizmos.sphere(Isometry3d::from_translation(v.a), v.r, Color::srgb(1.0, 0.2, 0.2));
                }
            }
        }
    }
}
