//! Hit detection driven by motion triggers.
//!
//! An attack is live while a motion's range trigger 10 (attack) or 33 (combo attack) covers the
//! current frame; its params are (atkp id, collision group[, combo id]). The hit volumes are the
//! owner model's object-collision shapes of that group with kind ATTACK, placed on their bones.
//! Targets are the BATTLE/TARGET shapes of the other team. Each trigger hits a target once.

use std::collections::HashSet;

use bevy::prelude::*;
use kh2_anim::triggers::range;
use kh2_model::collision::{kind, Shape};

use crate::schedule::{Frame, Sim};
use crate::skel::Animator;
use crate::SCALE;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Hit>()
            .add_message::<WallHit>()
            .add_systems(FixedUpdate, detect_hits.in_set(Sim::Detect))
            .add_systems(Update, draw_volumes.in_set(Frame::Volumes));
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

/// A player attack volume touching a wall that blocks attacks.
#[derive(Message, Clone, Copy, Debug)]
pub struct WallHit {
    pub attacker: Entity,
    pub body: Entity,
    pub atkp: i16,
    pub point: Vec3,
    pub normal: Vec3,
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

pub fn detect_hits(
    mut fighters: Query<(Entity, &mut Fighter, &GlobalTransform)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
    mut hits: MessageWriter<Hit>,
    mut wall_hits: MessageWriter<WallHit>,
    world: Option<Res<crate::world::World>>,
) {
    // Snapshot defenders first.
    let defenders: Vec<(Entity, Team, Vec<Volume>, Vec3)> = fighters
        .iter()
        .filter(|(_, f, _)| f.hittable)
        .filter_map(|(e, f, g)| bodies.get(f.hurt_body).ok().map(|(a, bg)| (e, f.team, hurt_volumes(a, bg), g.translation())))
        .collect();
    for (e, mut f, g) in &mut fighters {
        let mut still_live = HashSet::new();
        for &body in &f.attack_bodies.clone() {
            let Ok((anim, bg)) = bodies.get(body) else { continue };
            for (atkp, start, vol) in live_attacks(anim, bg) {
                if let (Some(world), Team::Player) = (world.as_ref(), f.team) {
                    // Each attack window bounces off a wall once.
                    let key = (Entity::PLACEHOLDER, body, start);
                    still_live.insert(key);
                    if !f.landed.contains(&key) {
                        if let Some((point, normal)) = world.touching_m(vol.a, vol.b, vol.r, kh2_map::flags::ATTACK_COLLIDE) {
                            f.landed.insert(key);
                            wall_hits.write(WallHit { attacker: e, body, atkp, point, normal });
                        }
                    }
                }
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
            }
        }
        // Forget windows that have closed so the next swing can hit again.
        f.landed.retain(|k| still_live.contains(k));
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
