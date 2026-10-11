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
    /// The magic damage entry point: the exact `SubId` of the `atkp` row a spell's attack object
    /// used for this hit (`magic.rs`; FUN_0019e7e0 picks it by the cast's kind and the hit count).
    /// `None` for motion-trigger attacks, whose row is the default lookup. Readers that need the
    /// row call `GameData::hit_row`.
    pub row: Option<u16>,
}

/// A hit whose damage multipliers for the attack's element and in general multiply to zero: the
/// game marks it ineffective (hit record flag 4), gives it no reaction and starts effect 0x12
/// instead of the normal spark (FUN_0018b618, FUN_0018d090). Written by the victim's receiver.
///
/// `fx::hit_sparks` applies the same rule on its own and starts effect 0x12 instead of the normal
/// spark, so this message only cancels the landed-hit sound.
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
    live_attack_shapes(anim, body).into_iter().map(|(atkp, start, vol, _)| (atkp, start, vol)).collect()
}

/// `live_attacks` with each volume's centre as the hit code keeps it: the attack entry's volume
/// record has the hurt records' layout (FUN_00189208 dispatches both on the kind word at +0), and
/// FUN_0018ce18 reads its centre at +0x10, which FUN_001894d0 fills as the bone matrix times the
/// shape offset, raised by the shape's lift. That the attack records are filled the same way is
/// [inferred] (their writer was not traced).
fn live_attack_shapes(anim: &Animator, body: &GlobalTransform) -> Vec<(i16, i16, Volume, Vec3)> {
    let Some(tr) = anim.triggers() else { return Vec::new() };
    let mut out = Vec::new();
    for r in tr.active(anim.gframe()).filter(|r| r.id == range::ATTACK || r.id == range::ATTACK_COMBO) {
        let (Some(&atkp), Some(&group)) = (r.params.first(), r.params.get(1)) else { continue };
        for s in anim.rig.model.shapes.iter().filter(|s| s.kind == kind::ATTACK && s.group as i16 == group) {
            out.push((atkp, r.start, shape_volume(s, anim, body), hurt_spheroid(s, anim, body).center));
        }
    }
    out
}

pub fn hurt_volumes(anim: &Animator, body: &GlobalTransform) -> Vec<Volume> {
    anim.rig.model.shapes.iter().filter(|s| s.kind == kind::BATTLE || s.kind == kind::TARGET).map(|s| shape_volume(s, anim, body)).collect()
}

/// A hurt shape as the game's hit code keeps it (the entry FUN_001894d0 fills for every shape of
/// the hurt list): a spheroid at the bone's world matrix times the shape offset, moved up by the
/// shape's lift, with the shape's radius and its height as the half height. Metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HurtSpheroid {
    pub center: Vec3,
    pub radius: f32,
    pub half_height: f32,
}

pub fn hurt_spheroid(s: &Shape, anim: &Animator, body: &GlobalTransform) -> HurtSpheroid {
    let m = match s.bone {
        Some(b) => anim.bone_world(body, b),
        None => body.to_matrix(),
    };
    let center = m.transform_point3(Vec3::from(s.offset)) + Vec3::Y * (s.lift * SCALE);
    HurtSpheroid { center, radius: s.radius.abs() * SCALE, half_height: s.height * SCALE }
}

/// A snapshot of a hittable fighter: entity, team, hurt shapes with their spheroids, position.
type Defender = (Entity, Team, Vec<(Volume, HurtSpheroid)>, Vec3);

/// The hurt shapes of a body paired with the spheroids the hit point is taken on.
fn hurt_shapes(anim: &Animator, body: &GlobalTransform) -> Vec<(Volume, HurtSpheroid)> {
    anim.rig
        .model
        .shapes
        .iter()
        .filter(|s| s.kind == kind::BATTLE || s.kind == kind::TARGET)
        .map(|s| (shape_volume(s, anim, body), hurt_spheroid(s, anim, body)))
        .collect()
}

/// Where a hit is placed (FUN_0018ce18, called by the hit-effect code FUN_0018d090 for every
/// landed hit): the point of the victim's spheroid in the direction of the attack sphere's
/// centre. The height difference is clamped to the half height; the horizontal radius at that
/// height is `radius * sqrt(1 - (dy / half_height)^2)` (the constant 1.0 at 0x37ac84). A shape
/// without height gets the equator (the game would divide by zero there).
pub fn victim_point(h: &HurtSpheroid, toward: Vec3) -> Vec3 {
    let d = toward - h.center;
    let dy = d.y.clamp(-h.half_height, h.half_height);
    let f = if h.half_height > 0.0 { (dy / h.half_height).abs() } else { 0.0 };
    let flat = d.with_y(0.0).normalize_or_zero() * (h.radius * (1.0 - f * f).max(0.0).sqrt());
    h.center + flat + Vec3::Y * dy
}

/// The wall cast of one attack window for one tick (FUN_001a27e8 -> FUN_0018c8e0): the segment
/// from the first volume's centre of the last tick (`prev`) to this tick's (`cur`), metres, against
/// terrain that blocks attacks. When it misses and the row's `FloorCheck` is not 0, a second segment
/// from `cur` down by that many game units (the game adds the byte to the segment's y in its own
/// world space, whose y is the app's -y) [inferred: the direction]. Returns (hit point, normal).
pub fn wall_cast(world: &crate::world::World, floor_check: u8, prev: Vec3, cur: Vec3) -> Option<(Vec3, Vec3)> {
    world.cast_m(prev, cur, kh2_map::flags::ATTACK_COLLIDE).or_else(|| {
        (floor_check != 0).then(|| world.cast_m(cur, cur - Vec3::Y * (floor_check as f32 * SCALE), kh2_map::flags::ATTACK_COLLIDE)).flatten()
    })
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
    // The centre of each window's first volume on the last tick (the game's attack entry keeps the
    // previous centres at +0xd0 and sets its flag 4 once they exist): (attack body, trigger start).
    mut wall_prev: Local<std::collections::HashMap<(Entity, i16), Vec3>>,
) {
    let log = std::env::var_os("KH2_HITLOG").is_some();
    let mut skipped_now = false;
    let mut first_seen: HashSet<(Entity, i16)> = HashSet::new();
    let mut live_windows: HashSet<(Entity, i16)> = HashSet::new();
    // Snapshot defenders first.
    let defenders: Vec<Defender> = fighters
        .iter()
        .filter(|(_, f, _)| f.hittable)
        .filter_map(|(e, f, g)| bodies.get(f.hurt_body).ok().map(|(a, bg)| (e, f.team, hurt_shapes(a, bg), g.translation())))
        .collect();
    for (e, mut f, g) in &mut fighters {
        let mut still_live = HashSet::new();
        let hurt_body = f.hurt_body;
        for &body in &f.attack_bodies.clone() {
            let Ok((anim, bg)) = bodies.get(body) else { continue };
            for (atkp, start, vol, centre) in live_attack_shapes(anim, bg) {
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
                    // The first hurt shape the volume touches (FUN_001b5a58 returns the first
                    // entry of the victim's list) places the hit on its surface.
                    if let Some((_, sph)) = hurts.iter().find(|(h, _)| overlap(&vol, h)) {
                        f.landed.insert(key);
                        let dir = (*tpos - g.translation()).with_y(0.0).normalize_or(Vec3::Z);
                        hits.write(Hit { attacker: e, target: *te, atkp, dir, point: victim_point(sph, centre), row: None });
                    }
                }
                if let (Some(world), Team::Player) = (world.as_ref(), f.team) {
                    // The game casts only the FIRST volume of the group (FUN_001a27e8: the cast runs
                    // for entry 0 of the centre list), a segment from its centre of the last tick to
                    // this tick's (FUN_0018c8e0). The first tick of a window has no previous centre
                    // and casts nothing (entry flag 4 is set after the centres are first stored).
                    if !first_seen.insert((body, start)) {
                        continue;
                    }
                    live_windows.insert((body, start));
                    let prev = wall_prev.insert((body, start), centre);
                    // Each attack window bounces off a wall once.
                    let key = (Entity::PLACEHOLDER, body, start);
                    still_live.insert(key);
                    let hit_a_target = f.landed.iter().any(|k| k.0 != Entity::PLACEHOLDER && k.1 == body && k.2 == start);
                    if f.landed.contains(&key) || hit_a_target {
                        continue;
                    }
                    let Some(prev) = prev else { continue };
                    let Some(row) = data.atkp(atkp) else { continue };
                    let Some((point, normal)) = wall_cast(world, row.floor_check, prev, centre) else { continue };
                    // The terrain test runs only for rows with BG Hit (atkp Flags bit 0).
                    if row.flags & rules::ATKP_FLAG_BG_HIT != 0 {
                        if log {
                            info!("wall cast: atkp {atkp} window {start} from {prev:?} to {centre:?} hits terrain at {point:?} (normal {normal:?}); volume radius {:.2}", vol.r);
                        }
                        f.landed.insert(key);
                        wall_hits.write(WallHit { attacker: e, body, atkp, point, weapon_volume: body != hurt_body });
                    } else if log {
                        skipped_now = true;
                        if *skipped != Some((body, start, atkp)) {
                            *skipped = Some((body, start, atkp));
                            info!("wall test not run: atkp {atkp} has no BG Hit, volume path crosses a wall at {point:?}");
                        }
                    }
                }
            }
        }
        // Forget windows that have closed so the next swing can hit again.
        f.landed.retain(|k| still_live.contains(k));
    }
    wall_prev.retain(|k, _| live_windows.contains(k));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn victim_point_lies_on_the_spheroid_toward_the_attack() {
        let h = HurtSpheroid { center: Vec3::new(1.0, 2.0, 3.0), radius: 0.5, half_height: 1.0 };
        // Level with the centre: the equator point toward the attack, one radius out.
        let p = victim_point(&h, Vec3::new(5.0, 2.0, 3.0));
        assert!((p - Vec3::new(1.5, 2.0, 3.0)).length() < 1e-5, "{p:?}");
        // Half way up: the horizontal radius shrinks to r * sqrt(1 - 0.25).
        let p = victim_point(&h, Vec3::new(1.0, 2.5, 7.0));
        assert!((p - Vec3::new(1.0, 2.5, 3.0 + 0.5 * 0.75f32.sqrt())).length() < 1e-5, "{p:?}");
        // Above the top: the height clamps to the half height and the point is the pole.
        let p = victim_point(&h, Vec3::new(1.0, 9.0, 3.1));
        assert!((p - Vec3::new(1.0, 3.0, 3.0)).length() < 1e-5, "{p:?}");
        // Below the bottom, the other pole.
        let p = victim_point(&h, Vec3::new(2.0, -9.0, 3.0));
        assert!((p - Vec3::new(1.0, 1.0, 3.0)).length() < 1e-5, "{p:?}");
        // A flat shape (no height) keeps the equator instead of the game's division by zero.
        let flat = HurtSpheroid { half_height: 0.0, ..h };
        let p = victim_point(&flat, Vec3::new(1.0, 5.0, 0.0));
        assert!((p - Vec3::new(1.0, 2.0, 2.5)).length() < 1e-5, "{p:?}");
    }

    #[test]
    fn victim_point_matches_fun_0018ce18_worked_by_hand() {
        // C = (0, 1, 0), r = 0.3, h = 0.6; attacker centre A = (0.8, 1.3, -0.6).
        // d = A - C = (0.8, 0.3, -0.6); dy = clamp(0.3, -0.6, 0.6) = 0.3; t = |0.3 / 0.6| = 0.5;
        // r * sqrt(1 - t^2) = 0.3 * 0.8660254 = 0.2598076; normalise(0.8, 0, -0.6) = (0.8, 0, -0.6);
        // point = C + (0.8 * 0.2598076, 0.3, -0.6 * 0.2598076) = (0.2078461, 1.3, -0.1558846).
        let h = HurtSpheroid { center: Vec3::new(0.0, 1.0, 0.0), radius: 0.3, half_height: 0.6 };
        let p = victim_point(&h, Vec3::new(0.8, 1.3, -0.6));
        assert!((p - Vec3::new(0.207_846_1, 1.3, -0.155_884_6)).length() < 1e-6, "{p:?}");
        // A far below: d = (-2, -6, 0); dy clamps to -0.6, t = 1, no horizontal part: (0, 0.4, 0).
        let p = victim_point(&h, Vec3::new(-2.0, -5.0, 0.0));
        assert!((p - Vec3::new(0.0, 0.4, 0.0)).length() < 1e-6, "{p:?}");
        // A level, straight along -x at any distance: the equator, one radius out: (-0.3, 1, 0).
        let p = victim_point(&h, Vec3::new(-7.0, 1.0, 0.0));
        assert!((p - Vec3::new(-0.3, 1.0, 0.0)).length() < 1e-6, "{p:?}");
    }

    /// A collision world of one wall (x = 100 units, facing -x) and one floor (y = 0), both blocking
    /// attacks, plus a second wall at x = 300 without the flag.
    fn wall_world() -> crate::world::World {
        let quad = |v: [[f32; 3]; 4], normal: [f32; 3], flags: u32| kh2_map::Polygon { verts: v, quad: true, normal, d: 0.0, flags, material: 0, mesh: 0 };
        let a = kh2_map::flags::ATTACK_COLLIDE;
        let coct = kh2_map::Coct {
            polygons: vec![
                quad([[100.0, 0.0, -300.0], [100.0, 300.0, -300.0], [100.0, 300.0, 300.0], [100.0, 0.0, 300.0]], [-1.0, 0.0, 0.0], a | kh2_map::flags::PARTY_COLLIDE),
                quad([[-300.0, 0.0, -300.0], [-300.0, 0.0, 300.0], [300.0, 0.0, 300.0], [300.0, 0.0, -300.0]], [0.0, 1.0, 0.0], a | kh2_map::flags::PARTY_STAND),
                quad([[300.0, 0.0, -300.0], [300.0, 300.0, -300.0], [300.0, 300.0, 300.0], [300.0, 0.0, 300.0]], [-1.0, 0.0, 0.0], kh2_map::flags::PARTY_COLLIDE),
            ],
        };
        crate::world::World::from_coct(&coct)
    }

    /// WP-F2 item A: the wall test is the game's segment cast between two ticks' centres
    /// (FUN_0018c8e0), so a swing that never crosses attack-blocking terrain recoils from nothing.
    #[test]
    fn wall_cast_hits_only_terrain_the_centre_path_crosses() {
        let w = wall_world();
        // A swing in open ground: the centre moves 40 cm toward a wall 50 cm away. Nothing.
        assert_eq!(wall_cast(&w, 0, Vec3::new(0.5, 1.0, 0.0), Vec3::new(0.9, 1.0, 0.0)), None);
        // The same swing a tick later crosses the wall: the hit is on the wall plane, x = 1.0 m.
        let (p, n) = wall_cast(&w, 0, Vec3::new(0.9, 1.0, 0.0), Vec3::new(1.3, 1.0, 0.0)).expect("the wall");
        assert!((p - Vec3::new(1.0, 1.0, 0.0)).length() < 1e-4, "{p:?}");
        assert!((n - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-4, "{n:?}");
        // A wall without the attack flag is not terrain for the cast.
        assert_eq!(wall_cast(&w, 0, Vec3::new(2.9, 1.0, 0.0), Vec3::new(3.3, 1.0, 0.0)), None);
        // A cast with no movement (the same centre twice) is nothing, not a hit on whatever is near.
        assert_eq!(wall_cast(&w, 0, Vec3::new(0.99, 1.0, 0.0), Vec3::new(0.99, 1.0, 0.0)), None);
        // Crossing the wall from behind is nothing: the game's segment test (FUN_001623a0) cuts a
        // polygon only when the segment runs against its normal.
        assert_eq!(wall_cast(&w, 0, Vec3::new(1.2, 1.0, 0.0), Vec3::new(0.8, 1.0, 0.0)), None);
        // A swing that ends exactly at the wall plane counts (the parameter may be 1).
        assert!(wall_cast(&w, 0, Vec3::new(0.6, 1.0, 0.0), Vec3::new(1.0, 1.0, 0.0)).is_some());
    }

    /// The `FloorCheck` byte adds a second, downward segment of that many game units when the
    /// first cast misses; every Sora combo row has 0, so his swings never probe the floor.
    #[test]
    fn wall_cast_floor_check_probes_down_only_when_set() {
        let w = wall_world();
        let (prev, cur) = (Vec3::new(0.2, 0.5, 0.0), Vec3::new(0.3, 0.5, 0.0));
        assert_eq!(wall_cast(&w, 0, prev, cur), None);
        assert_eq!(wall_cast(&w, 40, prev, cur), None, "40 units down from 50 cm up does not reach the floor");
        let (p, _) = wall_cast(&w, 60, prev, cur).expect("the floor");
        assert!((p - Vec3::new(0.3, 0.0, 0.0)).length() < 1e-4, "{p:?}");
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
