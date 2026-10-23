//! The native source of reaction commands: collision records of type 5 in an object's file.
//!
//! An object file (`obj/*.mdlx`) has a BAR entry of type 23: a table of 0x14-byte collision records
//! behind a 0x40-byte header (the game's pointer is the object's +0x8d8; FUN_00188e70 walks it). A
//! record of type 5 is a *reaction part*: while it is enabled and the player's own type 1 volume
//! overlaps it, the command whose id is in the record is offered (FUN_0019cb70, called for every
//! object by FUN_0019cad0 from the player's controller FUN_0019c670). The Soldier has five such parts
//! with the command id 34 (a cross of five ellipsoids around it), which is how command 34 reaches him;
//! no script offers it. See docs/systems/drive-limits-summons/contract.md.
//!
//! Record layout [V from FUN_001894d0, FUN_00188e70, FUN_0019cb70]:
//!
//! | offset | meaning |
//! | --- | --- |
//! | +0 | part index (bit number in the enable masks) |
//! | +2 | u16 command id (type 5) |
//! | +4 | type: 1 the object's own body volume, 5 reaction part, 6 hurt box, ... |
//! | +5 | shape kind: 0 ellipsoid, 1 cylinder, 3 sphere (FUN_00189208 dispatches on the pair) |
//! | +6 | u16 joint: bit 0x4000 is a special frame; 0x4000 itself is the object's own matrix |
//! | +8, +0xa, +0xc | s16 x, y, z offset in the joint's frame (game space, Y down) |
//! | +0xe | s16 subtracted from the centre's y afterwards (lifts it) |
//! | +0x10, +0x12 | s16 radii: +4 of the shape (horizontal), +8 (vertical) |

use bevy::math::Vec3;

pub const HEADER: usize = 0x40;
pub const RECORD: usize = 0x14;
/// BAR entry type of the collision table in an object file.
pub const ENTRY_TYPE: u16 = 23;
/// Record type of a reaction part.
pub const TYPE_REACTION: u8 = 5;
/// Record type of the object's own body volume (the player's one is tested against the parts).
pub const TYPE_BODY: u8 = 1;
/// The joint value that means "the object's own matrix at its position" (FUN_0016aa18 case 0x4000).
pub const JOINT_OBJECT: u16 = 0x4000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rec {
    pub part: u8,
    pub id: u16,
    pub kind: u8,
    pub shape: u8,
    pub joint: u16,
    pub off: [i16; 3],
    pub lift: i16,
    pub r: [i16; 2],
}

#[derive(Clone, Debug, Default)]
pub struct Table {
    /// The word at +4 of the header: parts enabled by default, one bit per part index (FUN_0019a6a0).
    pub default_mask: u32,
    pub recs: Vec<Rec>,
}

impl Table {
    /// Reads the type 23 entry of an object file.
    pub fn from_mdlx(file: &[u8]) -> Option<Table> {
        let bar = kh2_bar::parse(file).ok()?;
        let e = bar.iter().find(|e| e.kind == ENTRY_TYPE)?;
        Table::parse(e.data)
    }

    pub fn parse(b: &[u8]) -> Option<Table> {
        let u32_at = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]));
        let count = u32_at(0)? as usize;
        let default_mask = u32_at(4)?;
        let mut recs = Vec::with_capacity(count);
        for i in 0..count {
            let r = b.get(HEADER + RECORD * i..HEADER + RECORD * (i + 1))?;
            let s = |o: usize| i16::from_le_bytes([r[o], r[o + 1]]);
            recs.push(Rec {
                part: r[0],
                id: u16::from_le_bytes([r[2], r[3]]),
                kind: r[4],
                shape: r[5],
                joint: u16::from_le_bytes([r[6], r[7]]),
                off: [s(8), s(10), s(12)],
                lift: s(14),
                r: [s(16), s(18)],
            });
        }
        Some(Table { default_mask, recs })
    }

    /// First record of a type (FUN_0016b558 with -1).
    pub fn first(&self, kind: u8) -> Option<&Rec> {
        self.recs.iter().find(|r| r.kind == kind)
    }

    pub fn reaction_parts(&self) -> impl Iterator<Item = &Rec> {
        self.recs.iter().filter(|r| r.kind == TYPE_REACTION)
    }
}

/// FUN_0019a6a0 for the masks the app can fill: a part is usable when a motion trigger 4 enabled it,
/// not when a trigger 5 disabled it, else by the default mask. (The script's own masks, host 1.52 and
/// 1.53, come first in the game; no script of the app's enemies sets them.) `enabled` and `disabled`
/// are the parameters of the range triggers 4 and 5 that cover the frame.
pub fn part_usable(table: &Table, part: u8, enabled: &[i16], disabled: &[i16]) -> bool {
    if enabled.contains(&(part as i16)) {
        return true;
    }
    if disabled.contains(&(part as i16)) {
        return false;
    }
    part < 32 && table.default_mask >> part & 1 != 0
}

/// A volume in the app's space, centimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub kind: u8,
    pub center: Vec3,
    /// Horizontal and vertical radius (+4 and +8 of the game's shape record).
    pub r: [f32; 2],
}

/// FUN_001894d0 for joint 0x4000: the record's offset rotated by the object's facing and added to its
/// position, the centre's y then lifted by `lift`. `pos` is the object's position and `yaw` its facing
/// in the app's space (the app is the game's rotated half a turn about X; game y is down). The object's
/// matrix is read as a rotation about Y with local +z forward [inferred; the game's matrix at +0x40 was
/// not decoded, the Soldier's cross of parts is symmetric under a half turn but not a quarter turn].
/// `None` for a joint other than 0x4000 [GAP: the bone matrix of FUN_0016aa18's other cases].
pub fn shape_of(rec: &Rec, pos: Vec3, yaw: f32) -> Option<Shape> {
    if rec.joint != JOINT_OBJECT {
        return None;
    }
    // Game-space facing (sin a, 0, -cos a) has the yaw theta = atan2(sin a, -cos a).
    let theta = yaw.sin().atan2(-yaw.cos());
    let (s, c) = theta.sin_cos();
    let (x, y, z) = (rec.off[0] as f32, rec.off[1] as f32, rec.off[2] as f32);
    let (gx, gz) = (x * c + z * s, -x * s + z * c);
    // Game position of the object, then back to the app's axes.
    let g = Vec3::new(pos.x + gx, -pos.y + y - rec.lift as f32, -pos.z + gz);
    Some(Shape { kind: rec.shape, center: Vec3::new(g.x, -g.y, -g.z), r: [rec.r[0] as f32, rec.r[1] as f32] })
}

/// FUN_00189208 for the pairs the discs' records use: two ellipsoids (kind 0, FUN_00188f08) overlap when
/// the offset of their centres, scaled by the sums of the radii (horizontal for x and z, vertical for
/// y), is shorter than 1. Two spheres (kind 3, FUN_00189190) when the distance is below the sum of the
/// first radii. Other pairs are `None` [GAP: the cylinder tests FUN_00188fd0 and FUN_001890d8].
pub fn overlaps(a: &Shape, b: &Shape) -> Option<bool> {
    let d = b.center - a.center;
    match (a.kind, b.kind) {
        (0, 0) | (0, 3) | (3, 0) => {
            let k = Vec3::new(1.0 / (a.r[0] + b.r[0]), 1.0 / (a.r[1] + b.r[1]), 1.0 / (a.r[0] + b.r[0]));
            Some((d * k).length_squared() < 1.0)
        }
        (3, 3) => Some(d.length_squared() < (a.r[0] + b.r[0]) * (a.r[0] + b.r[0])),
        _ => None,
    }
}

/// `FUN_00189208` for every pair of shape kinds the parts use: `parts::overlaps` for ellipsoids and
/// spheres, and the cylinder (kind 1) tests: `FUN_00188fd0(a, cylinder)` for an ellipsoid or sphere
/// `a`: the height difference must be below the two vertical radii; above the cylinder's half height
/// the ellipsoid's horizontal radius at that height (`sqrt(1 - k^2) * a.r`, k the height past the
/// cylinder over a's vertical radius, `DAT_0037ab24` = 1) is added to the cylinder's radius, else the
/// full radii; the horizontal distance must be below that sum. `FUN_001890d8` for two cylinders: the
/// same with the full radii.
pub fn shapes_overlap(a: &Shape, b: &Shape) -> Option<bool> {
    let cyl = |e: &Shape, c: &Shape| {
        let d = c.center - e.center;
        let dy = d.y.abs();
        if dy >= e.r[1] + c.r[1] {
            return false;
        }
        let r = if e.kind != 1 && dy > c.r[1] {
            let k = (dy - c.r[1]) / e.r[1];
            (1.0 - k * k).max(0.0).sqrt() * e.r[0] + c.r[0]
        } else {
            e.r[0] + c.r[0]
        };
        d.x * d.x + d.z * d.z < r * r
    };
    match (a.kind, b.kind) {
        (1, 1) => Some(cyl(a, b)),
        (0 | 3, 1) => Some(cyl(a, b)),
        (1, 0 | 3) => Some(cyl(b, a)),
        _ => overlaps(a, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(off: [i16; 3], lift: i16, r: [i16; 2]) -> Rec {
        Rec { part: 6, id: 34, kind: 5, shape: 0, joint: JOINT_OBJECT, off, lift, r }
    }

    #[test]
    fn parts_follow_the_enable_masks() {
        let t = Table { default_mask: 1, recs: vec![] };
        assert!(part_usable(&t, 0, &[], &[]));
        assert!(!part_usable(&t, 6, &[], &[]));
        assert!(part_usable(&t, 6, &[6], &[]));
        assert!(!part_usable(&t, 0, &[], &[0]));
        // A trigger 4 wins over a trigger 5 of the same part (FUN_0019a6a0 tests the enable word first).
        assert!(part_usable(&t, 6, &[6], &[6]));
    }

    #[test]
    fn a_part_sits_at_its_offset_in_the_objects_frame() {
        // Game y is down: an offset of -150 is 150 cm above the object in the app.
        let r = rec([0, -150, 0], 0, [700, 300]);
        let s = shape_of(&r, Vec3::new(10.0, 20.0, 30.0), 0.7).unwrap();
        assert!((s.center - Vec3::new(10.0, 170.0, 30.0)).length() < 1e-3);
        // A lift raises the centre in the app too.
        let s = shape_of(&rec([0, 0, 0], 85, [35, 55]), Vec3::ZERO, 0.0).unwrap();
        assert!((s.center.y - 85.0).abs() < 1e-3);
        // A horizontal offset keeps its length whatever the facing.
        for k in 0..8 {
            let s = shape_of(&rec([300, 0, 0], 0, [1, 1]), Vec3::ZERO, k as f32 * 0.8).unwrap();
            assert!((s.center.length() - 300.0).abs() < 1e-2);
        }
        assert!(shape_of(&Rec { joint: 3, ..r }, Vec3::ZERO, 0.0).is_none());
    }

    #[test]
    fn ellipsoids_overlap_by_scaled_distance() {
        let part = Shape { kind: 0, center: Vec3::new(0.0, 150.0, 0.0), r: [700.0, 300.0] };
        let player = |x: f32, y: f32| Shape { kind: 0, center: Vec3::new(x, y, 0.0), r: [35.0, 55.0] };
        assert_eq!(overlaps(&part, &player(0.0, 85.0)), Some(true));
        assert_eq!(overlaps(&part, &player(730.0, 150.0)), Some(true));
        assert_eq!(overlaps(&part, &player(740.0, 150.0)), Some(false));
        // Vertically the radii sum is 355.
        assert_eq!(overlaps(&part, &player(0.0, 150.0 + 350.0)), Some(true));
        assert_eq!(overlaps(&part, &player(0.0, 150.0 + 360.0)), Some(false));
        assert_eq!(overlaps(&Shape { kind: 1, ..part }, &player(0.0, 0.0)), None);
    }

    /// The disc's Soldier: five reaction parts with command 34, enabled by the kick motion's trigger 4
    /// only, and Sora's body volume as the shape tested against them (skipped without KH2_ISO).
    #[test]
    fn soldier_parts_from_the_disc() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let d = crate::assets::Disc::open(&iso);
        let soldier = Table::from_mdlx(&d.read("obj/M_EX010.mdlx").expect("M_EX010")).expect("collision table");
        let parts: Vec<&Rec> = soldier.reaction_parts().collect();
        assert_eq!(parts.len(), 5);
        assert!(parts.iter().all(|p| p.id == 34 && p.part == 6 && p.shape == 0 && p.joint == JOINT_OBJECT && p.r == [700, 300]));
        assert_eq!(soldier.default_mask, 1, "only part 0 is on by default");
        assert!(!part_usable(&soldier, 6, &[], &[]));
        // Which motions of the Soldier enable part 6: exactly the kick (slot 203).
        let m = kh2_anim::Mset::parse(&d.read("obj/M_EX010.mset").expect("mset")).expect("parse");
        let enabling: Vec<usize> = (0..m.slots.len())
            .filter(|&i| m.get(i).is_some_and(|s| s.triggers.ranges.iter().any(|r| r.id == 4 && r.params.first() == Some(&6))))
            .collect();
        assert_eq!(enabling, vec![203]);
        let sora = Table::from_mdlx(&d.read("obj/P_EX100.mdlx").expect("P_EX100")).expect("collision table");
        let body = sora.first(TYPE_BODY).expect("body volume");
        assert_eq!((body.shape, body.joint, body.r), (0, JOINT_OBJECT, [35, 55]));
        // Standing at the Soldier's feet overlaps a part; 2000 cm away does not.
        let near = shape_of(body, Vec3::new(100.0, 0.0, 0.0), 0.0).unwrap();
        let far = shape_of(body, Vec3::new(2000.0, 0.0, 0.0), 0.0).unwrap();
        let hit = |p: &Shape| parts.iter().any(|r| overlaps(&shape_of(r, Vec3::ZERO, 0.0).unwrap(), p) == Some(true));
        assert!(hit(&near));
        assert!(!hit(&far));
    }
}
