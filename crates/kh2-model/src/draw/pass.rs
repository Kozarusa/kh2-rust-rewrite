//! The object pass of the draw unit (WP-L1-P25b, spec `docs/systems/core-runtime/render-model.md` c5): the sky call, the
//! per-object sort key, the quicksort, the distance fade, the three-part fade, the draw layers, the billboard and the
//! translucent and see-through lists.
//!
//! The pass works on the game's own byte layouts (RULES C7) through [`Words`], a sparse view of the game's memory: the
//! draw tree object (offsets in `kh2_map::draw_tree::off`, repeated as [`off`] here because that crate sits above this
//! one), the draw objects it links, the layer buckets at `0x3b4070` and the scratchpad arrays at `0x70000000`. What the
//! original reaches through other units is a call on [`PassHost`] (the model draw, the part switches, the light matrices,
//! the shadow pass, the effect creator, the projection update); [`NoPassHost`] does none of them.
//!
//! Draw object fields used here: `+0x00` model handle, `+0x08` render flag words, `+0x18` and `+0x20` the fixed GS register
//! words, `+0x34` the part alpha record, `+0x38` the light matrix output pointer, `+0x40` matrix, `+0xc0` position,
//! `+0xe8..+0xeb` colour and alpha, `+0xec` sibling, `+0xf0` first child, `+0xf8` draw flags, `+0x100` light block,
//! `+0x104` part-switch word, `+0x108` fade distance, `+0x10c` fade alpha, `+0x10d` level of detail, `+0x10e + view` the
//! per-view state byte, `+0x114` the record of fade and detail distances, `+0x11c` slot index, `+0x11f` layer, `+0x120`
//! a pointer to a projection-like matrix or 0, `+0xc..+0xf` the copy of the colour and alpha.
//!
//! Not wired into the app (the renderer draws every mesh once with Bevy's own sorting). Wiring would take a host that
//! provides the model draw `0x11c7d8` / `0x11cb38`, the part switches `0x11c580` / `0x11c5b0`, the shadow pass (`light.rs`
//! and `shadow.rs` hold the rules) and the effect creator, and a per-frame caller that keeps the tree's slot table; its
//! visible effect would be layer order for decals and water, the distance fade of props and NPCs, the three-part fade of
//! distant prop parts, Sora seen through walls, and the sky drawn after the map.

use super::frustum::{self, M4, V4, transform, vu, vu_divide};
use super::{VIEW_INDEX_WORD, VIEW_SELECT_BYTE, camera_address, fpu_div, set_field_of_view};

/// A sparse view of the game's memory at its own addresses, little endian.
pub trait Words {
    fn r8(&self, a: u32) -> u8;
    fn w8(&mut self, a: u32, v: u8);
    fn r16(&self, a: u32) -> u16;
    fn w16(&mut self, a: u32, v: u16);
    fn r32(&self, a: u32) -> u32;
    fn w32(&mut self, a: u32, v: u32);
    fn rf(&self, a: u32) -> f32 {
        f32::from_bits(self.r32(a))
    }
    fn wf(&mut self, a: u32, v: f32) {
        self.w32(a, v.to_bits());
    }
}

/// The tree offsets this pass uses (the full list is in `kh2_map::draw_tree::off`).
pub mod off {
    pub const VIEW_MATRIX: u32 = 0x130;
    pub const CLIP_SCREEN: u32 = 0x1b0;
    pub const PROJECTION: u32 = 0x20;
    pub const PLAYER_DEPTH: u32 = 0x774;
    pub const SEE_THROUGH: u32 = 0x778;
    pub const SORT_BY_GROUP: u32 = 0x779;
    pub const SEE_THROUGH_EFFECT: u32 = 0x790;
    pub const SEE_THROUGH_Z: u32 = 0x794;
    pub const SEE_THROUGH_LIST: u32 = 0x798;
    pub const SEE_THROUGH_COUNT: u32 = 0xc40;
    pub const SEE_THROUGH_VIEW_COUNT: u32 = 0x77c;
    pub const BATCH_COUNT: u32 = 0xc50;
    pub const BATCHES: u32 = 0xc60;
    pub const BATCH_SIZE: u32 = 0x370;
    pub const BATCH_Z: u32 = 0x4;
    pub const BATCH_LIST: u32 = 0x8;
    pub const BATCH_LIST_COUNT: u32 = 0x360;
    pub const SLOTS: u32 = 0x4360;
    pub const SLOT_COUNT: u32 = 0x5360;
    pub const VIEW: u32 = 0x536c;
    pub const SKY0: u32 = 0x5370;
    pub const SKY1: u32 = 0x5374;
    pub const LAYER_COUNT: u32 = 0x537c;
}

/// The layer buckets: 256 words (`0x3b4070`).
pub const LAYER_BUCKETS: u32 = 0x3b4070;
/// The scratchpad arrays of the pass: the key of every slot (`0x70000800`), the keys of the sorted objects
/// (`0x70000000`) and the slot addresses that go with them (`0x70001000`).
pub const KEYS_ALL: u32 = 0x7000_0800;
pub const SORT_KEYS: u32 = 0x7000_0000;
pub const SORT_SLOTS: u32 = 0x7000_1000;
/// The block `0x149068` initialises (the shadow mode block of the all-groups pass).
pub const SHADOW_MODE_BLOCK: u32 = 0x484380;
/// The byte `0x35594d`: the map model is not drawn while it is 1 (set by [`map_off`], cleared by [`map_on`]).
pub const FLAG_MAP_OFF: u32 = 0x35594d;
/// The global tree word (`0x355930`).
pub const TREE_WORD: u32 = 0x355930;

/// The two fixed register words `0x147e20`, `0x1484b0` store in a drawn object (`+0x20` and `+0x18`).
const REG_WORD_20: u64 = 0x3100_0070;
const REG_WORD_18: u64 = 0x0008_00d8;
/// The code addresses `0x147e20` stores in the record the model draw returns (the callbacks the packet runs).
const CALLBACK_50: u32 = 0x147d98;
const CALLBACK_54: u32 = 0x147de8;

/// What the original reaches through other units. Every method has a do-nothing default.
pub trait PassHost {
    /// `0x1b62a8`: the frame profiler marks the start of the pass.
    fn profile_begin(&mut self, _m: &mut dyn Words) {}
    /// `0x1b62d8`: the end of the object draws.
    fn profile_end(&mut self, _m: &mut dyn Words) {}
    /// `0x224b18`: is a scene event running.
    fn scene_event_running(&mut self, _m: &mut dyn Words) -> bool {
        false
    }
    /// `0x11c580(model, part)`: shows a part of the model.
    fn show_part(&mut self, _m: &mut dyn Words, _model: u32, _part: u32) {}
    /// `0x11c5b0(model, part)`: hides a part of the model.
    fn hide_part(&mut self, _m: &mut dyn Words, _model: u32, _part: u32) {}
    /// `0x11cb38(model, matrix, object matrix, state words, group list, 0)`: the model draw with a list of mesh groups
    /// (the sky). `list` is the group list the original builds on its stack.
    fn draw_model_groups(&mut self, _m: &mut dyn Words, _model: u32, _screen: u32, _matrix: u32, _state: u32, _list: &[u16]) {}
    /// `0x11c7d8(model, a1, a2, state, mode)`: the model draw. `a1` and `a2` are the 64 bytes behind the two matrix
    /// arguments (the original passes addresses, some of them on its stack). Returns the draw record (0 for none).
    fn draw_model(&mut self, _m: &mut dyn Words, _model: u32, _a1: &[u8; 64], _a2: &[u8; 64], _state: u32, _mode: u8) -> u32 {
        0
    }
    /// `0x134bd8(out, matrix, lights)` with the depth in f12: the model's light matrices.
    fn light_matrices(&mut self, _m: &mut dyn Words, _z: f32, _out: u32, _matrix: u32, _lights: u32) {}
    /// `0x148d58`: collects the object and its shadow-casting children into a list.
    fn shadow_collect(&mut self, _m: &mut dyn Words, _tree: u32, _obj: u32) -> Vec<u32> {
        Vec::new()
    }
    /// `0x148e08`: the shadow draw of the collected objects (the list the original ends with a 0 word).
    fn shadow_draw(&mut self, _m: &mut dyn Words, _tree: u32, _objs: &[u32]) {}
    /// `0x1da840`: creates the effect that draws a translucent batch or the see-through list.
    fn effect_create(&mut self, _m: &mut dyn Words, _at: u32) {}
    /// `0x106a68`: recomputes the matrices of a projection block (0x100 bytes) after its field of view changed.
    fn projection_update(&mut self, _m: &mut dyn Words, _proj: &mut [u8; 0x100]) {}
    /// `0x1084f8`: initialises the shadow mode block.
    fn init_shadow_block(&mut self, _m: &mut dyn Words, _block: u32) {}
}

/// A host that does nothing.
pub struct NoPassHost;

impl PassHost for NoPassHost {}

// ---------------------------------------------------------------------------------------------------------------------
// memory helpers
// ---------------------------------------------------------------------------------------------------------------------

fn rv4(m: &dyn Words, a: u32) -> V4 {
    std::array::from_fn(|k| m.rf(a + 4 * k as u32))
}

fn rm4(m: &dyn Words, a: u32) -> M4 {
    std::array::from_fn(|k| rv4(m, a + 16 * k as u32))
}

fn wv4(m: &mut dyn Words, a: u32, v: V4) {
    for (k, f) in v.iter().enumerate() {
        m.wf(a + 4 * k as u32, *f);
    }
}

fn wm4(m: &mut dyn Words, a: u32, v: &M4) {
    for (k, row) in v.iter().enumerate() {
        wv4(m, a + 16 * k as u32, *row);
    }
}

fn set_bits(m: &mut dyn Words, a: u32, bits: u32) {
    let v = m.r32(a);
    m.w32(a, v | bits);
}

fn clear_bits(m: &mut dyn Words, a: u32, bits: u32) {
    let v = m.r32(a);
    m.w32(a, v & !bits);
}

/// `acc + a * b` as the vector unit's accumulate chain rounds it.
fn madd(acc: f32, a: f32, b: f32) -> f32 {
    vu(acc + a * b)
}

/// The EE FPU's `cvt.w.s`: truncation toward zero, out of range values saturate.
fn float_to_int(f: f32) -> i32 {
    f as i32
}

/// The camera block of the view the pass draws through (`0x1068c0(1)`).
fn camera(m: &dyn Words) -> u32 {
    camera_address(1, m.r8(VIEW_SELECT_BYTE), m.r32(VIEW_INDEX_WORD))
}

/// The matrix the billboard and the sky-like draws use: the inverse of a view matrix whose upper 3x3 is a rotation. The
/// rows are the columns of the view's rotation (lane w 0), the translation is `0 - t * R^T` on x, y and z while the w lane of
/// the row keeps the view's own (vector unit instructions with the destination mask xyz).
fn inverse_view(view: &M4) -> M4 {
    let col = |c: usize| -> V4 { [view[0][c], view[1][c], view[2][c], 0.0] };
    let (c0, c1, c2) = (col(0), col(1), col(2));
    let d = view[3];
    let mut t = [0.0f32; 4];
    for (k, out) in t.iter_mut().enumerate().take(3) {
        let mut a = vu(c0[k] * d[0]);
        a = madd(a, c1[k], d[1]);
        a = madd(a, c2[k], d[2]);
        *out = vu(0.0 - a);
    }
    t[3] = d[3];
    [c0, c1, c2, t]
}

// ---------------------------------------------------------------------------------------------------------------------
// the small functions
// ---------------------------------------------------------------------------------------------------------------------

/// orig 0x143100: switches the sky on (its draw flag bit 2 set on both sky objects) through the global tree.
pub fn sky_on_global(m: &mut dyn Words) {
    let tree = m.r32(TREE_WORD);
    sky_on(m, tree);
}

/// orig 0x143120: switches the sky off through the global tree.
pub fn sky_off_global(m: &mut dyn Words) {
    let tree = m.r32(TREE_WORD);
    sky_off(m, tree);
}

/// orig 0x143140: the map off: the byte `0x35594d` becomes 1 and the sky is switched off (`0x143120`).
pub fn map_off(m: &mut dyn Words) {
    m.w8(FLAG_MAP_OFF, 1);
    sky_off_global(m);
}

/// orig 0x143160: the map on: the byte `0x35594d` becomes 0 and the sky is switched on (`0x143100`).
pub fn map_on(m: &mut dyn Words) {
    m.w8(FLAG_MAP_OFF, 0);
    sky_on_global(m);
}

/// orig 0x1431d0: the tree's sort-by-group switch (`+0x779`) becomes 1.
pub fn sort_by_group_on(m: &mut dyn Words) {
    let tree = m.r32(TREE_WORD);
    m.w8(tree + off::SORT_BY_GROUP, 1);
}

/// orig 0x15f1b0: a wrapper of [`sky_on_global`].
pub fn sky_on_wrapper_a(m: &mut dyn Words) {
    sky_on_global(m);
}

/// orig 0x15f1c8: a wrapper of [`sky_off_global`].
pub fn sky_off_wrapper_a(m: &mut dyn Words) {
    sky_off_global(m);
}

/// orig 0x148440: sets bit 2 of the draw flags of each sky object that exists.
pub fn sky_on(m: &mut dyn Words, tree: u32) {
    for o in [off::SKY0, off::SKY1] {
        let sky = m.r32(tree + o);
        if sky != 0 {
            set_bits(m, sky + 0xf8, 4);
        }
    }
}

/// orig 0x148478: clears bit 2 of the draw flags of each sky object that exists.
pub fn sky_off(m: &mut dyn Words, tree: u32) {
    for o in [off::SKY0, off::SKY1] {
        let sky = m.r32(tree + o);
        if sky != 0 {
            clear_bits(m, sky + 0xf8, 4);
        }
    }
}

/// orig 0x147158: is the object drawn through the see-through effect: false while a scene event runs; otherwise true when
/// the tree's see-through switch (`+0x778`) is set, bit 0 of the object's draw flags is set and the object's view depth `z`
/// is less than the player's (`+0x774`): the object lies between the camera and the player.
pub fn see_through_test(m: &mut dyn Words, host: &mut dyn PassHost, z: f32, tree: u32, obj: u32) -> bool {
    if host.scene_event_running(m) {
        return false;
    }
    m.r8(tree + off::SEE_THROUGH) != 0 && m.r32(obj + 0xf8) & 1 != 0 && z < m.rf(tree + off::PLAYER_DEPTH)
}

/// orig 0x147980: scans the group-word array from `*index` downward for an entry other than 1; sets `*index` to its
/// position and returns the word, or returns 1 (and leaves `*index`) when none is left.
pub fn next_group(index: &mut i32, words: &[u32]) -> u32 {
    let mut i = *index;
    while i > -1 {
        let w = words[i as usize];
        if w != 1 {
            *index = i;
            return w;
        }
        i -= 1;
    }
    1
}

/// orig 0x147b90: copies the colour and alpha bytes `+0xe8..+0xeb` to `+0xc,+0xd,+0xe,+0xf` (the order of the original:
/// red, alpha, green, blue; the bytes are independent).
pub fn copy_colour(m: &mut dyn Words, obj: u32) {
    let (r, a, g, b) = (m.r8(obj + 0xe8), m.r8(obj + 0xeb), m.r8(obj + 0xe9), m.r8(obj + 0xea));
    m.w8(obj + 0xc, r);
    m.w8(obj + 0xf, a);
    m.w8(obj + 0xd, g);
    m.w8(obj + 0xe, b);
}

/// orig 0x148920: the visible mark: the object's slot word gets bit 0 and the object's draw flags bit 1.
pub fn mark_visible(m: &mut dyn Words, tree: u32, obj: u32) {
    let slot = tree.wrapping_add(m.r16(obj + 0x11c) as u32 * 8);
    set_bits(m, slot + 0x4364, 1);
    set_bits(m, obj + 0xf8, 2);
}

/// orig 0x148948: empties the draw layers: the count (`+0x537c`) and the 256 buckets.
pub fn layers_reset(m: &mut dyn Words, tree: u32) {
    m.w32(tree + off::LAYER_COUNT, 0);
    for i in 0..256 {
        m.w32(LAYER_BUCKETS + 4 * i, 0);
    }
}

/// orig 0x148980: puts the object into the first free bucket at or after its layer number and counts it.
pub fn layers_add(m: &mut dyn Words, tree: u32, obj: u32) {
    let mut i = m.r8(obj + 0x11f) as u32;
    while m.r32(LAYER_BUCKETS + 4 * i) != 0 {
        i += 1;
    }
    m.w32(LAYER_BUCKETS + 4 * i, obj);
    let n = m.r32(tree + off::LAYER_COUNT);
    m.w32(tree + off::LAYER_COUNT, n.wrapping_add(1));
}

/// orig 0x1489d8: draws the layers: buckets 0 to 255 in order, each object found through the model draw
/// ([`draw_object`]), until as many were drawn as the layer count says.
pub fn layers_draw(m: &mut dyn Words, tree: u32, host: &mut dyn PassHost) {
    let mut left = m.r32(tree + off::LAYER_COUNT) as i32;
    for i in 0..256u32 {
        if left < 1 {
            return;
        }
        let obj = m.r32(LAYER_BUCKETS + 4 * i);
        if obj != 0 {
            left -= 1;
            draw_object(m, tree, obj, host);
        }
    }
}

/// orig 0x149068: when called with 1 and 0xffff initialises the shadow mode block `0x484380`.
pub fn shadow_block_hook(m: &mut dyn Words, host: &mut dyn PassHost, a: i64, b: i64) {
    if b == 0xffff && a == 1 {
        host.init_shadow_block(m, SHADOW_MODE_BLOCK);
    }
}

/// orig 0x1490a0: [`shadow_block_hook`] with (1, 0xffff).
pub fn shadow_block_init(m: &mut dyn Words, host: &mut dyn PassHost) {
    shadow_block_hook(m, host, 1, 0xffff);
}

// ---------------------------------------------------------------------------------------------------------------------
// sort
// ---------------------------------------------------------------------------------------------------------------------

/// orig 0x1471d0: the quicksort of the object pass. Sorts `keys[lo..=hi]` ascending (float compare) and moves the words of
/// `slots` the same way. Hoare's partition around the middle key `keys[(lo + hi) >> 1]`, recursion on the left part, the
/// right part by loop. `slots` and `keys` are addresses of two word arrays.
pub fn quicksort(m: &mut dyn Words, slots: u32, keys: u32, lo: i32, hi: i32) {
    let mut lo = lo;
    loop {
        let pivot = m.rf(keys.wrapping_add(((hi.wrapping_add(lo)) >> 1).wrapping_mul(4) as u32));
        let (mut i, mut j) = (lo, hi);
        loop {
            while m.rf(keys.wrapping_add(i.wrapping_mul(4) as u32)) < pivot {
                i = i.wrapping_add(1);
            }
            while pivot < m.rf(keys.wrapping_add(j.wrapping_mul(4) as u32)) {
                j = j.wrapping_sub(1);
            }
            if j < i {
                break;
            }
            let (ka, kb) = (keys.wrapping_add(i.wrapping_mul(4) as u32), keys.wrapping_add(j.wrapping_mul(4) as u32));
            let (sa, sb) = (slots.wrapping_add(i.wrapping_mul(4) as u32), slots.wrapping_add(j.wrapping_mul(4) as u32));
            i = i.wrapping_add(1);
            j = j.wrapping_sub(1);
            let (x, y) = (m.r32(ka), m.r32(kb));
            m.w32(ka, y);
            m.w32(kb, x);
            let (x, y) = (m.r32(sa), m.r32(sb));
            m.w32(sa, y);
            m.w32(sb, x);
            if i > j {
                break;
            }
        }
        if lo < j {
            quicksort(m, slots, keys, lo, j);
        }
        lo = i;
        if hi <= i {
            return;
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// fades, level of detail and the sort key
// ---------------------------------------------------------------------------------------------------------------------

/// Address 0x148550 (the level-of-detail select; its `/// orig` claim is `kh2_factory::ported::threshold_band`): picks the level of detail byte `+0x10d` from the distance `d` and the four distances of the record at
/// `+0x114` (`+0x44`, `+0x40`, `+0x3c`, `+0x38`): beyond the first the level is 2; between the first and second it keeps;
/// between the second and third 1; between the third and fourth it keeps; within the fourth 0.
pub fn lod_select(m: &mut dyn Words, obj: u32, d: f32) {
    let rec = m.r32(obj + 0x114);
    let cur = m.r8(obj + 0x10d);
    let level = if m.rf(rec + 0x44) < d {
        2
    } else if m.rf(rec + 0x40) < d {
        cur
    } else if m.rf(rec + 0x3c) < d {
        1
    } else if m.rf(rec + 0x38) < d {
        cur
    } else {
        0
    };
    m.w8(obj + 0x10d, level);
}

/// The alpha byte of one part of the three-part fade: beyond `far` the part is hidden (alpha 0, returns None for the
/// hide call), else `0x80` minus the truncated fraction of the way from `near` to `far` times 128, `0x80` before `near`.
fn part_alpha(d: f32, near: f32, far: f32) -> (u8, bool) {
    if far < d {
        return (0, true);
    }
    if d < near {
        return (0x80, false);
    }
    let ratio = fpu_div(vu(d - near), vu(far - near));
    let v = float_to_int(vu(ratio * 128.0));
    (0x80u8.wrapping_sub(v as u8), false)
}

/// orig 0x1485a8: the three-part fade of an object with a part alpha record (`+0x34`): shows parts 1, 2 and 3 of the model,
/// then sets the alpha bytes `+7`, `+0xf`, `+0x17` of the record from the distance `d` and the near and far distances of
/// the object's record (`+0x114`: part 1 `+0x60`/`+0x64`, part 2 `+0x58`/`+0x5c`, part 3 `+0x50`/`+0x54`), hiding a part
/// that is beyond its far distance.
pub fn three_part_fade(m: &mut dyn Words, host: &mut dyn PassHost, d: f32, obj: u32) {
    if m.r32(obj + 0x34) == 0 {
        return;
    }
    let rec = m.r32(obj + 0x114);
    let far1 = m.rf(rec + 0x64);
    let near3 = m.rf(rec + 0x50);
    let far3 = m.rf(rec + 0x54);
    let near2 = m.rf(rec + 0x58);
    let far2 = m.rf(rec + 0x5c);
    let near1 = m.rf(rec + 0x60);
    let model = m.r32(obj);
    for part in 1..=3 {
        host.show_part(m, model, part);
    }
    let alpha = m.r32(obj + 0x34);
    for (part, (near, far), at) in [(1u32, (near1, far1), 7u32), (2, (near2, far2), 0xf), (3, (near3, far3), 0x17)] {
        let (a, hide) = part_alpha(d, near, far);
        m.w8(alpha + at, a);
        if hide {
            host.hide_part(m, model, part);
        }
    }
}

/// The distance of the camera position `eye` to the object position `p` as the vector unit computes it: the difference,
/// the squares, `(x + y) + z`, then `VSQRT`.
fn distance(eye: V4, p: V4) -> f32 {
    let d: V4 = std::array::from_fn(|k| vu(eye[k] - p[k]));
    frustum::dot3(d, d).abs().sqrt()
}

/// orig 0x1487a8: the sort key of an object: the view depth `z` of its position (lane z of the position, w taken as 1.0,
/// times the tree's view matrix `+0x130`). With bit 0x80 of the draw flags the level of detail is chosen from the
/// distance to `eye`; with bit 0x100 the three-part fade runs on it (the distance is computed again when the level of
/// detail did not). Returns `z`.
pub fn sort_key(m: &mut dyn Words, host: &mut dyn PassHost, tree: u32, obj: u32, eye: V4) -> f32 {
    let view = rm4(m, tree + off::VIEW_MATRIX);
    let p = rv4(m, obj + 0xc0);
    let z = transform(&view, [p[0], p[1], p[2], 1.0])[2];
    let mut dist = 0.0f32;
    let mut flags = m.r32(obj + 0xf8);
    if flags & 0x80 != 0 {
        dist = distance(eye, p);
        lod_select(m, obj, dist);
        flags = m.r32(obj + 0xf8);
    }
    if flags & 0x100 != 0 {
        if dist == 0.0 {
            dist = distance(eye, p);
        }
        three_part_fade(m, host, dist, obj);
    }
    z
}

// ---------------------------------------------------------------------------------------------------------------------
// billboard and the model draw
// ---------------------------------------------------------------------------------------------------------------------

/// orig 0x147728: the billboard step of an object with bit 0x1000 and no projection pointer (`+0x120`): its matrix (`+0x40`)
/// and position (`+0xc0`) are multiplied by the inverse of the camera's view matrix ([`inverse_view`]) so that the object
/// faces the camera.
pub fn billboard(m: &mut dyn Words, obj: u32) {
    if m.r32(obj + 0xf8) & 0x1000 == 0 || m.r32(obj + 0x120) != 0 {
        return;
    }
    let cam = camera(m);
    let inv = inverse_view(&rm4(m, cam + 0x10));
    let pos = transform(&inv, rv4(m, obj + 0xc0));
    let n = rm4(m, obj + 0x40);
    let r: M4 = std::array::from_fn(|k| transform(&inv, n[k]));
    wm4(m, obj + 0x40, &r);
    wv4(m, obj + 0xc0, pos);
}

/// The screen rectangle word of a billboard draw: x low halfword, y high halfword, each `(coordinate * 16 + 0.5)`
/// truncated (`0x379024` 16.0, `0x379028` 0.5).
fn screen_word(x: f32, y: f32) -> u32 {
    let fx = float_to_int(vu(vu(x * 16.0) + 0.5));
    let fy = float_to_int(vu(vu(y * 16.0) + 0.5));
    (fx as u32 & 0xffff) | ((fy as u32) << 16)
}

fn bytes64(v: &M4) -> [u8; 64] {
    let mut out = [0u8; 64];
    for (r, row) in v.iter().enumerate() {
        for (c, f) in row.iter().enumerate() {
            out[16 * r + 4 * c..16 * r + 4 * c + 4].copy_from_slice(&f.to_le_bytes());
        }
    }
    out
}

/// The perspective divide the billboard draws use: `S * (1 / S.w)` on x and y (the vector unit divide, then `VMULq`).
fn divide_xy(s: V4) -> (f32, f32) {
    let q = vu_divide(1.0, s[3]);
    (vu(s[0] * q), vu(s[1] * q))
}

/// orig 0x147e20: draws one object through the model draw. Copies the colour (`0x147b90`), clears the object's state byte
/// for this view (`+0x10e + view`) and stores the two fixed register words. An object without bit 0x1000 is drawn with the
/// tree's screen matrix (`+0x1b0`) and its own matrix. A billboard (bit 0x1000) is drawn with a matrix whose translation is
/// recomputed, and the record the draw returns gets the callbacks `0x147d98` / `0x147de8` (`+0x50`, `+0x54`) and the screen
/// rectangle (`+0x14`):
/// * without a projection pointer (`+0x120 == 0`): the translation becomes the point `(0, 0, z)` taken through the
///   inverse of the view matrix, `z` being the object's view depth; the rectangle is the object's position through the screen
///   matrix with the viewport offset 2048 taken off;
/// * with one: the matrix is read from the pointer, its fourth column replaced (1.0, the saved column added to the
///   translation), the tree's projection block copied, its field of view set from the matrix's old `w`
///   ([`set_field_of_view`]) and updated by the host (`0x106a68`); the lane w the original adds from an unwritten stack word is
///   taken as 0.0 [inferred: harmless while the projection's fourth row has w 0].
pub fn draw_object(m: &mut dyn Words, tree: u32, obj: u32, host: &mut dyn PassHost) {
    copy_colour(m, obj);
    let view = m.r32(tree + off::VIEW);
    m.w32(obj + 0x20, REG_WORD_20 as u32);
    m.w32(obj + 0x24, (REG_WORD_20 >> 32) as u32);
    m.w8(obj.wrapping_add(view).wrapping_add(0x10e), 0);
    m.w32(obj + 0x18, REG_WORD_18 as u32);
    m.w32(obj + 0x1c, (REG_WORD_18 >> 32) as u32);
    let model = m.r32(obj);
    let mode = m.r8(obj + 0x10d);
    let screen = rm4(m, tree + off::CLIP_SCREEN);
    if m.r32(obj + 0xf8) & 0x1000 == 0 {
        let a2 = rm4(m, obj + 0x40);
        host.draw_model(m, model, &bytes64(&screen), &bytes64(&a2), obj + 8, mode);
        return;
    }
    let ptr = m.r32(obj + 0x120);
    if ptr == 0 {
        let mut n = rm4(m, obj + 0x40);
        let p = rv4(m, obj + 0xc0);
        let vw = rm4(m, tree + off::VIEW_MATRIX);
        let vz = transform(&vw, [p[0], p[1], p[2], 1.0])[2];
        let s = transform(&screen, [p[0], p[1], p[2], 1.0]);
        let (ax, ay) = divide_xy(s);
        let (ax, ay) = (vu(ax - 2048.0), vu(ay - 2048.0));
        let inv = inverse_view(&vw);
        let mut y = transform(&inv, [0.0, 0.0, vz, 1.0]);
        y[3] = 1.0;
        n[3] = y;
        let rec = host.draw_model(m, model, &bytes64(&screen), &bytes64(&n), obj + 8, mode);
        m.w32(rec + 0x50, CALLBACK_50);
        m.w32(rec + 0x54, CALLBACK_54);
        m.w32(rec + 0x14, screen_word(ax, ay));
        return;
    }
    let mut n = rm4(m, ptr);
    let saved_w = [n[0][3], n[1][3], n[2][3]];
    let focal = n[3][3];
    n[0][3] = 0.0;
    n[1][3] = 0.0;
    n[2][3] = 0.0;
    n[3][3] = 1.0;
    let t = tree + off::PROJECTION;
    let mut pr = [0u8; 0x100];
    for (i, b) in pr.iter_mut().enumerate().take(0x7c) {
        *b = m.r8(t + i as u32);
    }
    for i in 0x80..0x100u32 {
        pr[i as usize] = m.r8(t + i);
    }
    set_field_of_view(&mut pr, focal);
    host.projection_update(m, &mut pr);
    let prm: M4 = std::array::from_fn(|r| std::array::from_fn(|c| f32::from_le_bytes(pr[16 * r + 4 * c..16 * r + 4 * c + 4].try_into().unwrap())));
    let v = n[3];
    let tv: V4 = [vu(v[0] + saved_w[0]), vu(v[1] + saved_w[1]), vu(v[2] + saved_w[2]), vu(v[3] + 0.0)];
    let s = transform(&prm, tv);
    let (ax, ay) = divide_xy(s);
    let (ax, ay) = (vu(ax - 2048.0), vu(ay - 2048.0));
    n[3][0] = vu(n[3][0] - tv[0]);
    n[3][1] = vu(n[3][1] - tv[1]);
    let mut a1 = [0u8; 64];
    a1.copy_from_slice(&pr[..64]);
    let rec = host.draw_model(m, model, &a1, &bytes64(&n), obj + 8, mode);
    m.w32(rec + 0x50, CALLBACK_50);
    m.w32(rec + 0x54, CALLBACK_54);
    m.w32(rec + 0x14, screen_word(ax, ay));
}

/// orig 0x147d98: the record callback of a billboard draw: sets the GS scissor register (`0x18`) from the record's
/// rectangle halfwords (`+0x14` and `+0x16`): the low word is `0x7000 - x` and the high word `0x7300 - y`. Returns the
/// register value; the three calls into the GS state unit (`0x134638`, `0x134658`, `0x134680`) are hardware plumbing.
pub fn billboard_scissor(rect_x: i16, rect_y: i16) -> u64 {
    let lo = (0x7000i64 - rect_x as i64) as u64;
    let hi = (0x7300i64 - rect_y as i64) as u64;
    lo | (hi << 32)
}

// ---------------------------------------------------------------------------------------------------------------------
// the lists
// ---------------------------------------------------------------------------------------------------------------------

fn children(m: &dyn Words, obj: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut c = m.r32(obj + 0xf0);
    while c != 0 {
        out.push(c);
        c = m.r32(c + 0xec);
    }
    out
}

fn slot_of(tree: u32, m: &dyn Words, obj: u32) -> u32 {
    tree.wrapping_add(m.r16(obj + 0x11c) as u32 * 8)
}

/// orig 0x147bb8: puts the object (and its children, recursively) into the see-through list: when bit 2 of the draw flags
/// is set and the slot word has bit 0 (visible) the colour is copied, the depth `z` stored at `+0x794`, the object appended
/// to the list at `+0x798` (count `+0xc40`) and counted for the view (`+0x77c + 4 * view`).
pub fn see_through_add(m: &mut dyn Words, z: f32, tree: u32, obj: u32) {
    if m.r32(obj + 0xf8) & 4 != 0 && m.r32(slot_of(tree, m, obj) + 0x4364) & 1 != 0 {
        copy_colour(m, obj);
        let n = m.r32(tree + off::SEE_THROUGH_COUNT);
        m.wf(tree + off::SEE_THROUGH_Z, z);
        m.w32(tree + off::SEE_THROUGH_LIST + 4 * n, obj);
        m.w32(tree + off::SEE_THROUGH_COUNT, n.wrapping_add(1));
        let v = m.r32(tree + off::VIEW);
        let at = tree.wrapping_add(v.wrapping_mul(4)) + off::SEE_THROUGH_VIEW_COUNT;
        let c = m.r32(at);
        m.w32(at, c.wrapping_add(1));
    }
    for c in children(m, obj) {
        see_through_add(m, z, tree, c);
    }
}

/// orig 0x147c98: puts the object (and its children) into the translucent batch of this view's current count: when bit 2
/// of the draw flags is set and the slot word has bit 0, an object with alpha copies its colour, gets alpha `0x80` in the
/// copy (`+0xf`), the depth is stored in the batch (`+4`) and the object appended to the batch list (count at batch
/// `+0x360`); the object's state byte for the view becomes 2 either way.
pub fn batch_add(m: &mut dyn Words, z: f32, tree: u32, obj: u32) {
    if m.r32(obj + 0xf8) & 4 != 0 && m.r32(slot_of(tree, m, obj) + 0x4364) & 1 != 0 {
        let view = m.r32(tree + off::VIEW);
        if m.r8(obj + 0xeb) != 0 {
            let v4 = tree.wrapping_add(view.wrapping_mul(4));
            let index = m.r32(v4 + off::BATCH_COUNT);
            copy_colour(m, obj);
            m.w8(obj + 0xf, 0x80);
            let b = tree.wrapping_add(index.wrapping_mul(off::BATCH_SIZE)).wrapping_add(off::BATCHES);
            m.wf(b + off::BATCH_Z, z);
            let n = m.r32(b + off::BATCH_LIST_COUNT);
            m.w32(b + off::BATCH_LIST_COUNT, n.wrapping_add(1));
            m.w32(b + off::BATCH_LIST + 4 * n, obj);
        }
        let view = m.r32(tree + off::VIEW);
        m.w8(obj.wrapping_add(view).wrapping_add(0x10e), 2);
    }
    for c in children(m, obj) {
        batch_add(m, z, tree, c);
    }
}

/// orig 0x148388: draws the opaque part of an object and its children: an object with alpha, bit 2 and the visible slot bit
/// goes to the model draw at once when its layer byte (`+0x11f`) is 0 and into the layer buckets otherwise.
pub fn draw_or_layer(m: &mut dyn Words, tree: u32, obj: u32, host: &mut dyn PassHost) {
    if m.r8(obj + 0xeb) != 0 && m.r32(obj + 0xf8) & 4 != 0 && m.r32(slot_of(tree, m, obj) + 0x4364) & 1 != 0 {
        if m.r8(obj + 0x11f) == 0 {
            draw_object(m, tree, obj, host);
        } else {
            layers_add(m, tree, obj);
        }
    }
    for c in children(m, obj) {
        draw_or_layer(m, tree, c, host);
    }
}

fn view_counter(m: &dyn Words, tree: u32, base: u32) -> u32 {
    tree.wrapping_add(m.r32(tree + off::VIEW).wrapping_mul(4)).wrapping_add(base)
}

/// orig 0x1479c0: routes one object of the sorted draw: bit 0x4000 is cleared (and remembered); the light matrices are
/// built when the object has an output pointer (`+0x38`) and a light block (`+0x100`); an object the see-through test
/// accepts goes to the see-through list with state 1; otherwise a state-1 object is added to the see-through list once more,
/// then an object with alpha, bit 0x10 and room in the 16 batches goes to a translucent batch (not when bit 0x4000 had been
/// set: it is skipped), or, if its state is 2 and there is room, is added to a batch and also drawn now ([`draw_or_layer`]).
pub fn draw_one(m: &mut dyn Words, host: &mut dyn PassHost, z: f32, tree: u32, obj: u32) {
    let flags = m.r32(obj + 0xf8);
    let was_set = flags & 0x4000 != 0;
    if was_set {
        m.w32(obj + 0xf8, flags & !0x4000);
    }
    let out = m.r32(obj + 0x38);
    if out != 0 && m.r32(obj + 0x100) != 0 {
        let lights = m.r32(obj + 0x100);
        host.light_matrices(m, z, out, obj + 0x40, lights);
    }
    if see_through_test(m, host, z, tree, obj) {
        see_through_add(m, z, tree, obj);
        let v = m.r32(tree + off::VIEW);
        m.w8(obj.wrapping_add(v).wrapping_add(0x10e), 1);
        return;
    }
    let v = m.r32(tree + off::VIEW);
    if m.r8(obj.wrapping_add(v).wrapping_add(0x10e)) == 1 {
        see_through_add(m, z, tree, obj);
    }
    let v = m.r32(tree + off::VIEW);
    let alpha = m.r8(obj + 0xeb);
    let room = (m.r32(view_counter(m, tree, off::BATCH_COUNT)) as i32) < 16;
    if alpha != 0 && (alpha as i8) >= 0 && room && m.r32(obj + 0xf8) & 0x10 != 0 {
        if was_set {
            return;
        }
        batch_add(m, z, tree, obj);
        let c = view_counter(m, tree, off::BATCH_COUNT);
        let n = m.r32(c);
        m.w32(c, n.wrapping_add(1));
        return;
    }
    if m.r8(obj.wrapping_add(v).wrapping_add(0x10e)) == 2 && (m.r32(view_counter(m, tree, off::BATCH_COUNT)) as i32) < 16 {
        batch_add(m, z, tree, obj);
        let c = view_counter(m, tree, off::BATCH_COUNT);
        let n = m.r32(c);
        m.w32(c, n.wrapping_add(1));
    }
    draw_or_layer(m, tree, obj, host);
}

/// orig 0x1484b0: draws a sky object when it has alpha and bit 2 of its draw flags: colour copy, state byte 0 for the
/// view, the fixed register words, and the model draw with the group list `0x8000, 0xffff` and the tree's screen matrix.
pub fn sky_draw(m: &mut dyn Words, tree: u32, sky: u32, host: &mut dyn PassHost) {
    if m.r8(sky + 0xeb) != 0 && m.r32(sky + 0xf8) & 4 != 0 {
        copy_colour(m, sky);
        let view = m.r32(tree + off::VIEW);
        m.w8(sky.wrapping_add(view).wrapping_add(0x10e), 0);
        m.w32(sky + 0x20, REG_WORD_20 as u32);
        m.w32(sky + 0x24, (REG_WORD_20 >> 32) as u32);
        m.w32(sky + 0x18, REG_WORD_18 as u32);
        m.w32(sky + 0x1c, (REG_WORD_18 >> 32) as u32);
        let model = m.r32(sky);
        host.draw_model_groups(m, model, tree + off::CLIP_SCREEN, sky + 0x40, sky + 8, &[0x8000, 0xffff]);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// the pass
// ---------------------------------------------------------------------------------------------------------------------

/// orig 0x147300: the object pass, once per frame after the map's draw. In order: the view index goes to `+0x536c`; each
/// sky that exists is drawn ([`sky_draw`]); the view's translucent batch count and see-through count are zeroed; the
/// profiler is started and the layers emptied; for every registered slot the billboard step, the sort key (stored in
/// `0x70000800 + 4 i`) and, for an object with bit 2 and without bit 0x40, an entry in the sort arrays; the quicksort when
/// there are two or more; the distance fade and the shadow pass for each slot with bit 1 (a slot whose object has fade
/// distance `f` and key `z` greater than `f` is skipped this frame, one with `z` within 400 of `f` has its fade alpha byte
/// (`+0x10c`) scaled by `1 - (z - (f - 400)) / 400`); the draws in order of descending key (or, with the sort-by-group
/// switch, in groups of equal word `+0x2c`); the layers; the profiler end; the translucent batches of this view with a
/// non-empty list; and the see-through list through its effect when the switch is set and the list is not empty.
pub fn object_pass(m: &mut dyn Words, tree: u32, host: &mut dyn PassHost) {
    let view = m.r32(VIEW_INDEX_WORD);
    m.w32(tree + off::VIEW, view);
    for o in [off::SKY0, off::SKY1] {
        let sky = m.r32(tree + o);
        if sky != 0 {
            sky_draw(m, tree, sky, host);
        }
    }
    let v4 = tree.wrapping_add(m.r32(tree + off::VIEW).wrapping_mul(4));
    m.w32(v4 + off::BATCH_COUNT, 0);
    m.w32(v4 + off::SEE_THROUGH_VIEW_COUNT, 0);
    host.profile_begin(m);
    layers_reset(m, tree);
    let cam = camera(m);
    let eye = rv4(m, cam + 0x50);
    let count = m.r32(tree + off::SLOT_COUNT) as i32;
    let mut sorted = 0i32;
    for i in 0..count.max(0) as u32 {
        let slot = tree + off::SLOTS + 8 * i;
        let obj = m.r32(slot);
        billboard(m, obj);
        let z = sort_key(m, host, tree, obj, eye);
        m.wf(KEYS_ALL + 4 * i, z);
        let flags = m.r32(obj + 0xf8);
        if flags & 4 != 0 && flags & 0x40 == 0 {
            m.wf(SORT_KEYS + 4 * sorted as u32, z);
            m.w32(SORT_SLOTS + 4 * sorted as u32, slot);
            sorted += 1;
        }
    }
    if sorted >= 2 {
        quicksort(m, SORT_SLOTS, SORT_KEYS, 0, sorted - 1);
    }
    for i in 0..count.max(0) as u32 {
        let slot = tree + off::SLOTS + 8 * i;
        if m.r32(slot + 4) & 2 == 0 {
            continue;
        }
        let obj = m.r32(slot);
        let flags = m.r32(obj + 0xf8);
        if flags & 0x2000 == 0 && flags & 0x40 == 0 {
            let f = m.rf(obj + 0x108);
            if f != 0.0 {
                let z = m.rf(KEYS_ALL + 4 * i);
                if f < z {
                    continue;
                }
                let lo = vu(f - 400.0);
                if lo < z {
                    let ratio = fpu_div(vu(z - lo), 400.0);
                    let alpha = m.r8(obj + 0x10c) as f32;
                    let v = float_to_int(vu(alpha * vu(1.0 - ratio)));
                    m.w8(obj + 0x10c, v as u8);
                }
            }
        }
        let list = host.shadow_collect(m, tree, obj);
        if !list.is_empty() {
            host.shadow_draw(m, tree, &list);
        }
    }
    let last = sorted - 1;
    if m.r8(tree + off::SORT_BY_GROUP) == 0 {
        let mut i = last;
        while i >= 0 {
            let slot = m.r32(SORT_SLOTS + 4 * i as u32);
            if m.r32(slot + 4) & 2 != 0 {
                let z = m.rf(SORT_KEYS + 4 * i as u32);
                let obj = m.r32(slot);
                draw_one(m, host, z, tree, obj);
            }
            i -= 1;
        }
    } else {
        let mut words: Vec<u32> = vec![1; (sorted.max(0)) as usize];
        for (i, w) in words.iter_mut().enumerate() {
            let obj = m.r32(m.r32(SORT_SLOTS + 4 * i as u32));
            *w = m.r32(obj + 0x2c);
        }
        let mut idx = last;
        loop {
            let g = next_group(&mut idx, &words);
            if g == 1 {
                break;
            }
            let mut j = idx;
            while j >= 0 {
                let slot = m.r32(SORT_SLOTS + 4 * j as u32);
                let w = words[j as usize];
                j -= 1;
                if w != 1 && w == g {
                    words[(j + 1) as usize] = 1;
                    if m.r32(slot + 4) & 2 != 0 {
                        let obj = m.r32(slot);
                        draw_one(m, host, 0.0, tree, obj);
                    }
                }
            }
        }
    }
    layers_draw(m, tree, host);
    host.profile_end(m);
    let view_count = m.r32(view_counter(m, tree, off::BATCH_COUNT)) as i32;
    for b in 0..view_count.max(0) as u32 {
        let at = tree + off::BATCHES + off::BATCH_SIZE * b;
        if m.r32(at + off::BATCH_LIST_COUNT) != 0 {
            host.effect_create(m, at);
        }
    }
    if m.r8(tree + off::SEE_THROUGH) != 0 && m.r32(view_counter(m, tree, off::SEE_THROUGH_VIEW_COUNT)) != 0 {
        host.effect_create(m, tree + off::SEE_THROUGH_EFFECT);
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// the occluder volume builder
// ---------------------------------------------------------------------------------------------------------------------

/// The corner points `0x1418a0` collects for one polygon entry of the occluder object: the addresses of three or four
/// vertices (entries `+2`, `+4`, `+6`, `+8` of the polygon are vertex numbers, `0xffff` at `+8` for a triangle) in the
/// object's vertex array (`+0x10`, 16 bytes each) and their count.
fn polygon_vertices(m: &dyn Words, obj: u32, poly: u32) -> ([u32; 4], u32) {
    let base = m.r32(obj + 0x10);
    let at = |o: u32| base.wrapping_add(m.r16(poly + o) as u32 * 0x10);
    let (a, b, c) = (at(2), at(4), at(6));
    if m.r16(poly + 8) != 0xffff {
        ([a, b, c, at(8)], 4)
    } else {
        ([a, b, c, 0], 3)
    }
}

/// orig 0x1621a8: the eight corner points the clip test takes for a polygon of three or four vertices: the vertices again
/// and again (a triangle `p0 p1 p2 p0 p1 p2 p0 p1`, a quad `p0 p1 p2 p3 p0 p1 p2 p3`), every point with w 1.0
/// (`0x379504`).
pub fn polygon_corners(verts: &[V4; 4], count: u32) -> [V4; 8] {
    let pick: [usize; 8] = if count == 3 { [0, 1, 2, 0, 1, 2, 0, 1] } else { [0, 1, 2, 3, 0, 1, 2, 3] };
    std::array::from_fn(|k| {
        let v = verts[pick[k]];
        [v[0], v[1], v[2], 1.0]
    })
}

/// orig 0x165990: builds the occluder volumes of the occluder object `obj` seen from `eye`. The object holds groups
/// (`+0x04`, `+0x20` of them, 0x20 bytes each, polygon-table ranges as halfwords at `+0x1c` and `+0x1e`), polygon tables
/// (`+0x08`, 0x14 bytes each: range of polygons at `+0x0c`/`+0x0e`, an enable byte at `+0x10`), polygons (`+0x0c`, 0x10 bytes
/// each: vertex numbers, a plane number at `+0x0a`) and planes (`+0x18`). For every polygon of an enabled table whose plane
/// faces the eye (`eye . n + w >= 0`) and whose eight corners the cull matrix `cull` (a block of 16 floats at the address)
/// does not put outside the view, five planes ([`frustum::occluder_planes`]) are written to `planes` (0x50 bytes each, one
/// after the other) and the word at `count` is incremented.
pub fn occluder_volumes(m: &mut dyn Words, obj: u32, eye: u32, planes: u32, count: u32, cull: u32) {
    let eye_v = rv4(m, eye);
    let cull_m = rm4(m, cull);
    let mut out = planes;
    let groups = m.r32(obj + 0x20) as i32;
    for g in 0..groups.max(0) as u32 {
        let entry = m.r32(obj + 4).wrapping_add(g * 0x20);
        let (first, end) = (m.r16(entry + 0x1c) as u32, m.r16(entry + 0x1e) as u32);
        for t in first..end {
            let table = m.r32(obj + 8).wrapping_add(t * 0x14);
            if m.r8(table + 0x10) == 0 {
                continue;
            }
            let (lo, hi) = (m.r16(table + 0xc) as u32, m.r16(table + 0xe) as u32);
            if lo >= hi {
                continue;
            }
            for p in lo..hi {
                let poly = m.r32(obj + 0xc).wrapping_add(p * 0x10);
                let plane = rv4(m, m.r32(obj + 0x18).wrapping_add(m.r16(poly + 0xa) as u32 * 0x10));
                let d = vu(frustum::dot3(eye_v, plane) + plane[3]);
                if d < 0.0 {
                    continue;
                }
                let (addrs, n) = polygon_vertices(m, obj, poly);
                let verts: [V4; 4] = std::array::from_fn(|k| if k < n as usize { rv4(m, addrs[k]) } else { [0.0; 4] });
                let corners = polygon_corners(&verts, n);
                if frustum::clip_points(&cull_m, &corners) != 0 {
                    let quad: [V4; 4] = [corners[0], corners[1], corners[2], corners[3]];
                    let pl = frustum::occluder_planes(eye_v, &quad);
                    for (k, v) in pl.iter().enumerate() {
                        wv4(m, out + 16 * k as u32, *v);
                    }
                    let c = m.r32(count);
                    m.w32(count, c.wrapping_add(1));
                    out = out.wrapping_add(0x50);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Mem(HashMap<u32, u8>);

    impl Words for Mem {
        fn r8(&self, a: u32) -> u8 {
            *self.0.get(&a).unwrap_or(&0)
        }
        fn w8(&mut self, a: u32, v: u8) {
            self.0.insert(a, v);
        }
        fn r16(&self, a: u32) -> u16 {
            u16::from_le_bytes([self.r8(a), self.r8(a + 1)])
        }
        fn w16(&mut self, a: u32, v: u16) {
            for (i, b) in v.to_le_bytes().into_iter().enumerate() {
                self.w8(a + i as u32, b);
            }
        }
        fn r32(&self, a: u32) -> u32 {
            u32::from_le_bytes(std::array::from_fn(|i| self.r8(a + i as u32)))
        }
        fn w32(&mut self, a: u32, v: u32) {
            for (i, b) in v.to_le_bytes().into_iter().enumerate() {
                self.w8(a + i as u32, b);
            }
        }
    }

    #[test]
    fn quicksort_orders_keys_and_moves_the_slots_with_them() {
        let mut m = Mem::default();
        let keys = [5.0f32, -1.0, 3.5, 3.5, 0.0, 9.0, -7.0];
        for (i, k) in keys.iter().enumerate() {
            m.wf(0x1000 + 4 * i as u32, *k);
            m.w32(0x2000 + 4 * i as u32, i as u32);
        }
        quicksort(&mut m, 0x2000, 0x1000, 0, 6);
        let sorted: Vec<f32> = (0..7).map(|i| m.rf(0x1000 + 4 * i)).collect();
        assert!(sorted.windows(2).all(|w| w[0] <= w[1]), "{sorted:?}");
        for i in 0..7u32 {
            assert_eq!(keys[m.r32(0x2000 + 4 * i) as usize], sorted[i as usize]);
        }
    }

    #[test]
    fn part_alpha_fades_between_near_and_far() {
        assert_eq!(part_alpha(10.0, 20.0, 40.0), (0x80, false));
        assert_eq!(part_alpha(30.0, 20.0, 40.0), (0x40, false));
        assert_eq!(part_alpha(41.0, 20.0, 40.0), (0, true));
    }

    #[test]
    fn layers_fill_the_first_free_bucket_at_or_after_the_layer() {
        let mut m = Mem::default();
        m.w8(0x100 + 0x11f, 3);
        m.w8(0x200 + 0x11f, 3);
        layers_add(&mut m, 0x5000, 0x100);
        layers_add(&mut m, 0x5000, 0x200);
        assert_eq!((m.r32(LAYER_BUCKETS + 12), m.r32(LAYER_BUCKETS + 16)), (0x100, 0x200));
        assert_eq!(m.r32(0x5000 + off::LAYER_COUNT), 2);
    }

    #[test]
    fn a_triangle_repeats_its_corners() {
        let v: [V4; 4] = [[1.0, 0.0, 0.0, 5.0], [0.0, 1.0, 0.0, 5.0], [0.0, 0.0, 1.0, 5.0], [9.0; 4]];
        let c = polygon_corners(&v, 3);
        assert_eq!(c[3], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(c[7], [0.0, 1.0, 0.0, 1.0]);
    }
}
