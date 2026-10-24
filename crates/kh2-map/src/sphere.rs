//! The edge search of the collision object's sphere pass (WP-L1-P22, spec `docs/systems/core-runtime/collision.md`
//! section 9): `FUN_00164ce8`, the consumer of the edge helpers `nearest_edge` (0x164780) and `edge_direction_gap`
//! (0x164690) that `oct.rs` ports.

use crate::geom::*;
use crate::oct::*;

/// 0x379554: the score a polygon edge must not exceed to be taken (the search starts from it).
const START_SCORE: f32 = f32::from_bits(0x56b5_e620);

/// What the sphere pass found: the nearest qualifying polygon edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeHit {
    /// The foot of the point on the winning edge line, in world space.
    pub foot: V4,
    /// The winning edge's first end vertex, in world space.
    pub edge_a: V4,
    /// The winning edge's second end vertex, in world space.
    pub edge_b: V4,
    /// The polygon the edge belongs to.
    pub poly: u16,
}

impl Oct {
    /// orig 0x164ce8: the search of the polygons with attribute bit 0x400 (the ledge bit) whose tree boxes overlap
    /// `bounds` (a float box in world space, carried into the object's frame through the inverse placement and
    /// quantized at the object's level), for the polygon edge nearest to `point`. For each such polygon the edge
    /// foot of `nearest_edge` (in the object's frame, from the point carried through the inverse placement) scores
    /// as the distance from the point to the foot plus `edge_direction_gap(point, dir, foot)` (the gap is read from
    /// the world-space point and the frame-local foot, as the original does); an edge is taken when its score does not
    /// exceed the best so far (the search starts at the constant 0x379554, so a later polygon with an equal score
    /// wins). The winning foot and edge ends are carried back through the placement matrix. `None` leaves the
    /// caller's outputs alone, as the original does when it returns 0.
    ///
    /// The walker runs with the filter mask 0x400 / value 0x400 and the accept-all handler (kind 0xf).
    pub fn ledge_edge_search(&self, point: V4, bounds: &FBox, dir: V4) -> Option<EdgeHit> {
        let local_box = fbox_transform(bounds, self.placement_inverse());
        let q = fbox_to_qbox(&local_box, self.level());
        let mut hits = Vec::new();
        self.collect(Filter { mask: 0x400, value: 0x400, invert: false, b: false }, &q, 0xf, &mut hits);
        if hits.is_empty() {
            return None;
        }
        let local = xf(self.placement_inverse(), point);
        let mut best = START_SCORE;
        let mut found: Option<(V4, V4, V4, u16)> = None;
        for &p in &hits {
            let verts = self.poly_verts(p);
            let (edge, foot) = nearest_edge(&verts, local);
            let Some((a, b)) = edge else { continue };
            let off = sub(foot, local);
            let score = dot3(off, off).sqrt() + edge_direction_gap(point, dir, foot);
            if score <= best {
                found = Some((foot, a, b, p));
                best = score;
            }
        }
        let (foot, a, b, poly) = found?;
        let m = self.placement();
        Some(EdgeHit { foot: xf(m, foot), edge_a: xf(m, a), edge_b: xf(m, b), poly })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{BuildPoly, coct_file};

    #[test]
    fn the_nearest_ledge_edge_is_found_and_other_polygons_are_ignored() {
        // Two floors: a plain one (no ledge bit) and a ledge one 100 units away.
        let plain = BuildPoly::new(&[[-50.0, 0.0, -50.0], [50.0, 0.0, -50.0], [50.0, 0.0, 50.0], [-50.0, 0.0, 50.0]], 1);
        let ledge = BuildPoly::new(&[[150.0, 0.0, -50.0], [250.0, 0.0, -50.0], [250.0, 0.0, 50.0], [150.0, 0.0, 50.0]], 1 | 0x400);
        let oct = Oct::new(&coct_file(&[plain, ledge]), u64::MAX).unwrap();
        let bounds = fbox(-100.0, -100.0, -100.0, 300.0, 100.0, 100.0);
        let hit = oct.ledge_edge_search([140.0, 0.0, 0.0, 1.0], &bounds, [0.0, 0.0, 1.0, 1.0]).expect("the ledge polygon");
        assert_eq!(hit.poly, 1);
        assert!((hit.foot[0] - 150.0).abs() < 1e-3, "{:?}", hit.foot);
        // Without a polygon carrying the ledge bit there is nothing to find.
        let plain_only = Oct::new(&coct_file(&[BuildPoly::new(&[[-50.0, 0.0, -50.0], [50.0, 0.0, -50.0], [50.0, 0.0, 50.0], [-50.0, 0.0, 50.0]], 1)]), u64::MAX).unwrap();
        assert_eq!(plain_only.ledge_edge_search([0.0, 0.0, 0.0, 1.0], &bounds, [0.0, 0.0, 1.0, 1.0]), None);
    }
}
