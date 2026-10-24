//! The map's collision slots and their wrappers (WP-L1-P22, spec `docs/systems/core-runtime/collision.md`).
//!
//! The game keeps the collision objects of the loaded map in globals: `DAT_00355934` the walking map (built from the
//! map file's first type-6 entry), `DAT_00355938` a second map collision, `DAT_0035593c` the floor colour map,
//! `DAT_00355940` a fourth object nothing in the clusters read uses, `DAT_00355944` the floor kind object, then
//! `DAT_00355948` the map model instance, and the flag bytes `DAT_0035594c..4f`. `DAT_00355930` is the drawn map's
//! octree, which belongs to the render clusters and is not modelled here (the group edits and the set-up below leave
//! its half out and say so). [`MapCollision`] holds the same slots; the wrappers are the original's one-line
//! functions over them.
//!
//! A slot keeps the COCT file image next to its [`Oct`]: three of the wrappers read the file's mesh and polygon
//! tables directly (the polygon-on test, the root group box, the walk over the polygons with attribute bits
//! `0x60000`).

use crate::geom::*;
use crate::mapobj::MapModel;
use crate::oct::*;

/// 0x378fbc: the lift of the collider the footstep query builds (the sphere centre's height above the foot).
const FOOT_LIFT: f32 = 100.0;

/// One collision slot: the object and the file image it was built over.
#[derive(Clone, Debug)]
pub struct CollSlot {
    file: Vec<u8>,
    /// The built object (orig 0x141500).
    pub oct: Oct,
}

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn rd16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}

impl CollSlot {
    /// Builds the object over `file` with the map's group word.
    pub fn new(file: &[u8], groups: u64) -> Option<CollSlot> {
        Some(CollSlot { file: file.to_vec(), oct: Oct::new(file, groups)? })
    }

    /// orig 0x141690: is polygon `poly` in a mesh that is switched on? The meshes are searched in order and the first
    /// one whose polygon range holds `poly` decides; the polygon index 0xffff and a polygon in no mesh answer false.
    pub fn polygon_on(&self, poly: u16) -> bool {
        if poly == 0xffff {
            return false;
        }
        let meshes = rd32(&self.file, 0x20) as usize;
        for m in 0..self.oct.mesh_count() {
            let at = meshes + 0x14 * m;
            let (first, last) = (rd16(&self.file, at + 0xc), rd16(&self.file, at + 0xe));
            if first <= poly && poly < last {
                return self.oct.mesh_enabled(m);
            }
        }
        false
    }

    /// orig 0x142818: the box of the root node of the tree (the box of the whole file) as floats: the node's
    /// quantized box (`node +0x10`) at the file's level through `qbox_to_fbox` (0x145000).
    pub fn root_box(&self) -> FBox {
        let nodes = rd32(&self.file, 0x18) as usize;
        let q: QBox = std::array::from_fn(|k| rd16(&self.file, nodes + 0x10 + 2 * k) as i16);
        qbox_to_fbox(&q, self.oct.level())
    }

    /// orig 0x142b10: the polygons whose attribute word has bits `0x60000`, in index order, as the pairs `(mode,
    /// polygon)` the original hands to `FUN_0014ea50` (the mode is bits 17..18 of the attribute word).
    pub fn special_polygons(&self) -> Vec<(u32, u32)> {
        let count = (rd32(&self.file, 0x2c) >> 4) as usize;
        let polys = rd32(&self.file, 0x28) as usize;
        let attrs = rd32(&self.file, 0x48) as usize;
        let mut out = Vec::new();
        for p in 0..count {
            let idx = rd16(&self.file, polys + 16 * p + 0xe) as usize;
            let attr = rd32(&self.file, attrs + 4 * idx);
            if attr & 0x60000 != 0 {
                out.push((attr >> 17 & 3, p as u32));
            }
        }
        out
    }
}

/// orig 0x141898: stores the address of the hit polygon table in the object (`oct +0x3c`) the walker fills; `oct` is
/// the object's bytes (the original's record, 0xe0 bytes).
pub fn set_hit_table_address(oct: &mut [u8], addr: u32) {
    oct[0x3c..0x40].copy_from_slice(&addr.to_le_bytes());
}

/// The file images the map set-up builds its collision objects over, one per slot (the map object's words
/// `+0x30` to `+0x40` name them; a zero word is `None`).
#[derive(Clone, Copy, Debug, Default)]
pub struct MapFiles<'a> {
    pub main: Option<&'a [u8]>,
    pub second: Option<&'a [u8]>,
    pub colour: Option<&'a [u8]>,
    pub aux: Option<&'a [u8]>,
    pub kinds: Option<&'a [u8]>,
}

/// What the collision half of the map set-up reports for the caller: the polygons of the walking map with attribute
/// bits `0x60000`, which the original registers with the camera (`FUN_0014ea50`, mode and polygon).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SetUp {
    pub special: Vec<(u32, u32)>,
}

/// The map's collision slots.
#[derive(Clone, Debug, Default)]
pub struct MapCollision {
    /// `DAT_00355934`: the walking map.
    pub main: Option<CollSlot>,
    /// `DAT_00355938`: the second map collision.
    pub second: Option<CollSlot>,
    /// `DAT_0035593c`: the floor colour map.
    pub colour: Option<CollSlot>,
    /// `DAT_00355940`.
    pub aux: Option<CollSlot>,
    /// `DAT_00355944`: the floor kind object.
    pub kinds: Option<CollSlot>,
    /// `DAT_00355948`: the map model instance (created by the drawn map's set-up, not by `set_up` here).
    pub model: Option<MapModel>,
    /// `DAT_0035594c`: the byte the set-up was given.
    pub flag_4c: u8,
    /// `DAT_0035594d`.
    pub flag_4d: u8,
    /// `DAT_0035594e`.
    pub flag_4e: u8,
    /// `DAT_0035594f`.
    pub flag_4f: u8,
}

impl MapCollision {
    /// orig 0x142ba8, the collision half: sets the flag bytes (`0x35594d` to 0, `0x35594c` to `flag`, `0x35594f` to 1,
    /// `0x35594e` to 0) and builds the slots from the map's files with the group word `groups`; the walking map's
    /// special polygons are reported (0x142b10). Not ported here: the allocation of the drawn map's octree
    /// (`FUN_00145748`), the model instance block and its setup calls (`FUN_0011e018`, `FUN_0011e8c0`, `FUN_0011e638`,
    /// `FUN_001084f8`, the identity matrix), and the draw tables (`FUN_0014e9e0`, `FUN_0015c018`, `FUN_0015c1e8`,
    /// `FUN_001531f8`); those belong to the render clusters.
    pub fn set_up(&mut self, files: &MapFiles, groups: u64, flag: u8) -> SetUp {
        self.flag_4d = 0;
        self.flag_4c = flag;
        self.set_flag_4f();
        self.clear_flag_4e();
        let build = |f: Option<&[u8]>| f.and_then(|f| CollSlot::new(f, groups));
        self.main = build(files.main);
        let special = self.main.as_ref().map(|s| s.special_polygons()).unwrap_or_default();
        self.second = build(files.second);
        self.colour = build(files.colour);
        self.aux = build(files.aux);
        self.kinds = build(files.kinds);
        SetUp { special }
    }

    /// orig 0x142e68, the collision half: drops the five slots (the original frees the floor kind object, the fourth
    /// object, the colour map, the second map, the model instance and the walking map, in that order) and returns the
    /// model instance for the caller to destroy. Not ported: the audio and draw releases the original starts with
    /// (`FUN_0014ead0`, `FUN_00153378`) and the drawn octree's release (`FUN_001459d0`).
    pub fn release(&mut self) -> Option<MapModel> {
        self.kinds = None;
        self.aux = None;
        self.colour = None;
        self.second = None;
        let model = self.model.take();
        self.main = None;
        model
    }

    /// orig 0x143028: switches mesh group `group` on in every slot (the original also shows the drawn group first,
    /// `FUN_00145b80` on the drawn octree, which is not ported here).
    pub fn group_on(&mut self, group: u8) {
        for s in self.slots_mut() {
            s.oct.set_group(group, true);
        }
    }

    /// orig 0x142f78: switches mesh group `group` off in every slot (the original also hides the drawn group first,
    /// `FUN_00145b40`, not ported here).
    pub fn group_off(&mut self, group: u8) {
        for s in self.slots_mut() {
            s.oct.set_group(group, false);
        }
    }

    fn slots_mut(&mut self) -> impl Iterator<Item = &mut CollSlot> {
        [&mut self.kinds, &mut self.main, &mut self.aux, &mut self.colour, &mut self.second].into_iter().flatten()
    }

    /// orig 0x143180: sets the flag byte `0x35594e`.
    pub fn set_flag_4e(&mut self) {
        self.flag_4e = 1;
    }

    /// orig 0x143190: clears the flag byte `0x35594e`.
    pub fn clear_flag_4e(&mut self) {
        self.flag_4e = 0;
    }

    /// orig 0x1431b0: sets the flag byte `0x35594f`.
    pub fn set_flag_4f(&mut self) {
        self.flag_4f = 1;
    }

    /// orig 0x1431c0: clears the flag byte `0x35594f`.
    pub fn clear_flag_4f(&mut self) {
        self.flag_4f = 0;
    }

    /// Clears the switch byte of the model instance (`MapModel::clear_switch`, 0x1430d8); nothing without an instance (the
    /// original would write through the null slot).
    pub fn clear_model_switch(&mut self) {
        if let Some(m) = self.model.as_mut() {
            m.clear_switch();
        }
    }

    /// Stores the colour bytes of the model instance (`MapModel::set_colour`, 0x143208).
    pub fn set_model_colour(&mut self, a: u8, b: u8, c: u8) {
        if let Some(m) = self.model.as_mut() {
            m.set_colour(a, b, c);
        }
    }

    /// The first word of the model instance (`MapModel::handle`, 0x143230); 0 without one.
    pub fn model_handle(&self) -> u32 {
        self.model.as_ref().map_or(0, |m| m.handle())
    }

    /// orig 0x143360: is polygon `poly` of the walking map in a switched-on mesh (`CollSlot::polygon_on`)? False without a
    /// walking map.
    pub fn polygon_on(&self, poly: u16) -> bool {
        self.main.as_ref().is_some_and(|s| s.polygon_on(poly))
    }

    /// orig 0x143388: the sphere pass on the walking map (`Oct::ledge_edge_search`).
    pub fn sphere_pass(&self, point: V4, bounds: &FBox, dir: V4) -> Option<EdgeHit> {
        self.main.as_ref()?.oct.ledge_edge_search(point, bounds, dir)
    }

    /// orig 0x143dc0: the vertices of polygon `poly` of the walking map as `polygon_vertices` (0x142120) gives them.
    pub fn polygon_verts(&self, poly: u16) -> Option<Verts> {
        self.main.as_ref().map(|s| polygon_vertices(&s.oct, poly))
    }

    /// orig 0x143de0: the plane of polygon `poly` of the walking map (0x142190).
    pub fn polygon_plane(&self, poly: u16) -> Option<V4> {
        self.main.as_ref().map(|s| s.oct.plane(poly))
    }

    /// orig 0x143e68: the vertices of polygon `poly` of the walking map in world space (0x1422d8).
    pub fn polygon_verts_world(&self, poly: u16) -> Option<Vec<V4>> {
        self.main.as_ref().map(|s| s.oct.verts_world(poly))
    }

    /// The object the polygon wrappers use: the second map collision when `secondary` is asked for and the slot is
    /// filled, else the walking map.
    fn pick(&self, secondary: bool) -> Option<&Oct> {
        match (secondary, self.second.as_ref()) {
            (true, Some(s)) => Some(&s.oct),
            _ => self.main.as_ref().map(|s| &s.oct),
        }
    }

    /// orig 0x143d68: the unit normal of polygon `poly` (`Oct::normal`, 0x1421b8), of the second map collision when
    /// `secondary` and that slot is filled, else of the walking map.
    pub fn normal(&self, poly: u16, secondary: bool) -> Option<V4> {
        self.pick(secondary).map(|o| o.normal(poly))
    }

    /// orig 0x143e00: the attribute word of polygon `poly` (0x1422b0), from the object `normal` picks.
    pub fn attr(&self, poly: u16, secondary: bool) -> Option<u32> {
        self.pick(secondary).map(|o| o.attr(poly))
    }

    /// orig 0x143e40: the kind nibble (low four bits) of the attribute word 0x143e00 gives. Added by WP-L1-P27b as the wrapper.
    pub fn attr_kind(&self, poly: u16, secondary: bool) -> Option<u32> {
        self.attr(poly, secondary).map(|a| a & 0xf)
    }

    /// orig 0x1439f8: the segment cast against the walking map with the empty attribute filter and the
    /// accept-everything handler of the caller's `kind` (0x141f08). Returns the hit and polygon (0xffff when none).
    pub fn ray(&self, seg: &[V4; 2], kind: u32) -> Option<(Option<V4>, u16)> {
        self.main.as_ref().map(|s| s.oct.ray(Filter::ALL, seg[0], seg[1], kind))
    }

    /// orig 0x1439d8: `ray` with a scratch polygon output (the caller wants the hit only).
    pub fn ray_hit(&self, seg: &[V4; 2], kind: u32) -> Option<Option<V4>> {
        self.ray(seg, kind).map(|r| r.0)
    }

    /// orig 0x143778, the collision side: the class byte the floor colour map's polygon under a segment names, with
    /// the empty filter and kind 0xf (`None`: the slot is empty or nothing is hit; the original then leaves the colour
    /// table class at 0). The original starts by setting the colour to table class 0 (`FUN_0015c690`) and ends by
    /// lighting it for the segment's first point (`FUN_00153540`); those two are drawing and are not ported.
    pub fn floor_colour_class(&self, seg: &[V4; 2]) -> Option<u8> {
        let s = self.colour.as_ref()?;
        let (hit, poly) = s.oct.ray(Filter::ALL, seg[0], seg[1], 0xf);
        hit?;
        (poly != 0xffff).then(|| s.oct.poly_bytes(poly).0)
    }

    /// orig 0x143438: the floor facts under a point for the footsteps: `Oct::floor_info` of the walking map for a
    /// default collider whose end is `point`, whose lift is `FOOT_LIFT` and whose floor polygon is "none", with the
    /// colour map and the floor kind object of the slots. `None` without a walking map. The colour table words
    /// (drawing) are not part of the result, as for `floor_info`.
    pub fn footstep_info(&self, point: V4) -> Option<FloorInfo> {
        let main = self.main.as_ref()?;
        let mut c = Collider::default();
        c.set_v4(coll::END, point);
        c.set_u16(coll::FLOOR_POLY, 0xffff);
        c.set_u16(coll::POLY_A, 0xffff);
        c.set_u16(coll::POLY_B, 0xffff);
        c.set_f32(coll::LIFT, FOOT_LIFT);
        c.set_u32(coll::MOVING, 0);
        Some(main.oct.floor_info(&c, self.colour.as_ref().map(|s| &s.oct), self.kinds.as_ref().map(|s| &s.oct)))
    }
}

/// orig 0x142120: the vertices of polygon `poly` as the original's static record holds them (it fills a static
/// vertex-pointer record at 0x484360 and hands back its address; the port returns the vertices themselves).
pub fn polygon_vertices(oct: &Oct, poly: u16) -> Verts {
    oct.poly_verts(poly)
}

/// orig 0x144328: the empty static initialiser of the collision unit (a constructor list word); it does nothing.
pub fn coct_static_init() {}

pub use crate::sphere::EdgeHit;
