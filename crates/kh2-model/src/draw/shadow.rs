//! Projected shadows on the map (render-model.md d2, L1-P26a): the box of the casting objects, the groups of the map the box
//! touches, the projection from a light, and the record the draw later consumes. Works on a [`Mem`] image of the game's memory
//! (the shadow pass reads object, render-tree and record-table words directly), with the three routines that belong to other
//! packages behind [`ShadowHost`].
//!
//! Not wired into the app: the renderer has no shadows today (DEVIATIONS 2026-10-24). Wiring takes (1) calling
//! [`shadow_pass_objects`] / [`shadow_pass_all`] where the draw pass calls them, (2) reading the registered records from the
//! table at 0x349e70 and drawing each record's groups a second time with its two matrices and the colour at record `+0x14`
//! (the draw side of the records is the packet build and stays out), and (3) a host that provides the projection builder
//! 0x135028, the frustum tests 0x146648 / 0x145fa0 (L1-P25a) and the allocator. The visible effect would be the dark blob under
//! Sora, enemies and props on the floor.

use crate::light::{GLOBAL_LIGHT, Mat4, invert, madd, sat};
use crate::texanim::objects::Mem;

/// Number of record slots in use: the word at 0x349e68.
pub const RECORD_COUNT: u32 = 0x349e68;
/// The word at 0x349e70 holds the address of the first record slot.
pub const RECORD_TABLE: u32 = 0x349e70;
/// Bytes per record slot.
pub const RECORD_SIZE: u32 = 0x120;
/// The bump allocator's next free address (0x34e0a8; the function at 0x10a9d0 advances it by the request rounded up to 4).
pub const BUMP: u32 = 0x34e0a8;
/// The three default colour bytes the block initialiser copies (0x34e0d0..0x34e0d2).
pub const BASE_COLOUR: u32 = 0x34e0d0;
/// The block of mode words the object pass leaves for the all-groups pass (0x484380, 0x38 bytes) and the object list it was
/// made for (0x484378).
pub const SHADOW_BLOCK: u32 = 0x484380;
pub const SHADOW_OBJS: u32 = 0x484378;

/// Bytes of a mode block (seven doublewords).
pub const MODE_SIZE: usize = 0x38;

/// What the shadow pass needs from routines other packages own.
pub trait ShadowHost {
    /// 0x146648: is a box inside the view volume whose block sits at `view`: 0 outside, 1 crossing, 2 inside.
    fn box_visible(&mut self, m: &Mem, view: u32, bx: &[f32; 6]) -> i64;
    /// 0x145fa0: does a box meet one of the render tree's occluder volumes (1) or not (0).
    fn occluded(&mut self, m: &Mem, tree: u32, bx: &[f32; 6]) -> i64;
    /// 0x135028: the projection of a box (its eight corners) from a light (position and colour vectors of the light block),
    /// for a texture of the size class `kind`. Returns the two matrices it writes (the first and the second output block).
    fn project(&mut self, m: &Mem, light: &[[f32; 4]; 2], corners: &[[f32; 4]; 8], kind: i32) -> (Mat4, Mat4);
}

fn r_mat(m: &Mem, a: u32) -> Mat4 {
    std::array::from_fn(|i| std::array::from_fn(|j| m.rf32(a + 16 * i as u32 + 4 * j as u32)))
}

fn w_mat(m: &mut Mem, a: u32, v: &Mat4) {
    for (i, row) in v.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            m.wf32(a + 16 * i as u32 + 4 * j as u32, *x);
        }
    }
}

fn r_box(m: &Mem, a: u32) -> [f32; 6] {
    std::array::from_fn(|i| m.rf32(a + 4 * i as u32))
}

/// The union of two boxes (min x y z, max x y z); the second box wins ties (the rule of 0x144778, ported in `kh2_map::geom`).
fn union_box(a: &[f32; 6], b: &[f32; 6]) -> [f32; 6] {
    let lo = |x: f32, y: f32| if x < y { x } else { y };
    let hi = |x: f32, y: f32| if y < x { x } else { y };
    [lo(a[0], b[0]), lo(a[1], b[1]), lo(a[2], b[2]), hi(a[3], b[3]), hi(a[4], b[4]), hi(a[5], b[5])]
}

/// Do two boxes overlap on all three axes, closed intervals (the rule of 0x144c28, ported in `kh2_map::geom`).
fn overlap(a: &[f32; 6], b: &[f32; 6]) -> bool {
    b[0] <= a[3] && a[0] <= b[3] && b[1] <= a[4] && a[1] <= b[4] && b[2] <= a[5] && a[2] <= b[5]
}

/// The mode block as the object pass builds it (the stores of the function at 0x1084f8, which belongs to another package):
/// mode word 0, colour 0x80 0x80 0x80 0x80 at +4, the three default colour bytes at +8 with 0xff after them, two constant
/// doublewords at +0x10 and +0x18, a colour word 0x80808000 at +0x28 and the rest zero. The word at +0xc is never written
/// by it (stack garbage in the original); it is zero here.
fn mode_block_init(m: &Mem) -> [u8; MODE_SIZE] {
    let mut b = [0u8; MODE_SIZE];
    b[4..8].fill(0x80);
    for i in 0..3 {
        b[8 + i] = m.r8(BASE_COLOUR + i as u32);
    }
    b[11] = 0xff;
    b[0x10..0x18].copy_from_slice(&0x0008_00d8u64.to_le_bytes());
    b[0x18..0x20].copy_from_slice(&0x3100_0070u64.to_le_bytes());
    b[0x29..0x2c].fill(0x80);
    b
}

fn read_list(m: &Mem, a: u32) -> Vec<u16> {
    let mut v = Vec::new();
    let mut p = a;
    loop {
        let g = m.r16(p);
        if g == 0xffff {
            return v;
        }
        v.push(g);
        p += 2;
    }
}

/// orig 0x11dac0: the eight corners of the box that holds all objects of a null-terminated pointer list (the first entry is
/// read without a null check). Each object's box is six floats at `+0xd0` (minimum x y z, then maximum x y z); the corners go
/// x low to high, y alternating on the x side, z low then high, each with `w` 1.0.
pub fn box_corners(m: &Mem, objs: u32) -> [[f32; 4]; 8] {
    let mut b = r_box(m, m.r32(objs) + 0xd0);
    let mut p = objs + 4;
    loop {
        let o = m.r32(p);
        if o == 0 {
            break;
        }
        let c = r_box(m, o + 0xd0);
        for k in 0..3 {
            if c[k] < b[k] {
                b[k] = c[k];
            }
            if b[3 + k] < c[3 + k] {
                b[3 + k] = c[3 + k];
            }
        }
        p += 4;
    }
    std::array::from_fn(|i| {
        let x = if i & 2 == 0 { b[0] } else { b[3] };
        let y = if (i & 2 == 0) == (i & 1 != 0) { b[4] } else { b[1] };
        let z = if i & 4 == 0 { b[2] } else { b[5] };
        [x, y, z, 1.0]
    })
}

/// A product of two matrices as the vector unit forms it: row `i` of the result is `b[i][0] * a[0] + b[i][1] * a[1] +
/// b[i][2] * a[2] + b[i][3] * a[3]`, accumulated in that order.
fn mul_rows(b: &Mat4, a: &Mat4) -> Mat4 {
    std::array::from_fn(|i| {
        std::array::from_fn(|k| madd(madd(madd(sat(a[0][k] * b[i][0]), a[1][k], b[i][1]), a[2][k], b[i][2]), a[3][k], b[i][3]))
    })
}

/// orig 0x10a9f8: registers a shadow record in the first free slot of the table (a slot whose map word, `+4`, is zero; the
/// slot after the last when none is free, as the original has no bound check). The record: `+4` the map, `+0` a copy of the
/// object list (pointers up to the first null, then a null) and `+8` a copy of the group list (shorts, then 0xffff), both
/// copied into memory taken from the bump allocator, `+0x10` the mode block with bit 0x10000 set in its first word, `+0x50` the
/// map's matrix, `+0x90` and `+0xd0` the two projection matrices and `+0x110` the kind. When the object list is empty only
/// the map word is stored. `map_matrix` is the 0x40 bytes the caller points at, `out_b` and `out_a` the matrices in the order
/// of the original's fifth and sixth arguments.
#[allow(clippy::too_many_arguments)]
pub fn register_record(
    m: &mut Mem,
    map: u32,
    map_matrix: &Mat4,
    mode: &[u8; MODE_SIZE],
    groups: &[u16],
    out_b: &Mat4,
    out_a: &Mat4,
    objs: u32,
    kind: i32,
) {
    let count = m.r32(RECORD_COUNT);
    let table = m.r32(RECORD_TABLE);
    let mut slot = 0u32;
    if count != 0 {
        while m.r32(table + slot * RECORD_SIZE + 4) != 0 && slot + 1 < count {
            slot += 1;
        }
        // The original's loop leaves `slot == count` when every slot is taken.
        if m.r32(table + slot * RECORD_SIZE + 4) != 0 {
            slot = count;
        }
    }
    let rec = table + slot * RECORD_SIZE;
    m.w32(rec + 4, map);
    let mut n = 0u32;
    while m.r32(objs + 4 * n) != 0 {
        n += 1;
    }
    if n == 0 {
        return;
    }
    let list = alloc(m, n * 4 + 4);
    m.w32(rec, list);
    for i in 0..n {
        let v = m.r32(objs + 4 * i);
        m.w32(list + 4 * i, v);
    }
    m.w32(list + 4 * n, 0);
    let g = alloc(m, groups.len() as u32 * 2 + 2);
    m.w32(rec + 8, g);
    for (i, v) in groups.iter().enumerate() {
        m.w16(g + 2 * i as u32, *v);
    }
    m.w16(g + 2 * groups.len() as u32, 0xffff);
    for (i, b) in mode.iter().enumerate() {
        m.w8(rec + 0x10 + i as u32, *b);
    }
    let w = m.r32(rec + 0x10) | 0x10000;
    m.w32(rec + 0x10, w);
    w_mat(m, rec + 0x50, map_matrix);
    w_mat(m, rec + 0x90, out_b);
    w_mat(m, rec + 0xd0, out_a);
    m.w32(rec + 0x110, kind as u32);
}

/// The bump allocator of the original (0x10a9d0, not a ledger row of this package): the next free address moves up by the
/// request rounded up to a multiple of 4.
fn alloc(m: &mut Mem, size: u32) -> u32 {
    let at = m.r32(BUMP);
    m.w32(BUMP, at.wrapping_add(size.wrapping_add(3) & !3));
    at
}

/// orig 0x11dc40: builds and registers the shadow record of a set of objects on a map. `mode` is the 0x38-byte mode block
/// (bit 0: light at the box centre instead of the global light), `groups` the map groups to draw again, `objs` the address of
/// the null-terminated object pointer list and `kind` the shadow kind (0, 1, 2).
///
/// With the global light the projection comes from the light block at 0x354e30. With bit 0 the light is a block of its own:
/// position at the centre of the box (corner 0 plus corner 6, halved) lowered by eight times the diagonal's length, colour
/// white. The second matrix is then taken to map space: times the map model's matrix at `+0x80`, and for a map model that is
/// not of type 2 also times the inverse of its matrix at `+0x40`. The record gets the map's own `+0x40` matrix.
pub fn project_shadow<H: ShadowHost>(m: &mut Mem, h: &mut H, map: u32, mode: &[u8; MODE_SIZE], groups: &[u16], objs: u32, kind: i32) {
    let corners = box_corners(m, objs);
    let light: [[f32; 4]; 2] = if mode[0] & 1 == 0 {
        std::array::from_fn(|v| std::array::from_fn(|i| m.rf32(GLOBAL_LIGHT + 16 * v as u32 + 4 * i as u32)))
    } else {
        let sum: [f32; 4] = std::array::from_fn(|i| corners[0][i] + corners[6][i]);
        let mut centre: [f32; 4] = sum.map(|x| x * 0.5);
        let diag: [f32; 4] = std::array::from_fn(|i| corners[6][i] - corners[0][i]);
        let len = ((diag[0] * diag[0] + diag[1] * diag[1]) + diag[2] * diag[2]).abs().sqrt();
        centre[1] -= len * 8.0;
        [centre, [1.0; 4]]
    };
    let (out_a, mut out_b) = h.project(m, &light, &corners, kind);
    let m40 = r_mat(m, map + 0x40);
    let m80 = r_mat(m, map + 0x80);
    let model_kind = m.r32(m.r32(m.r32(map + 0x14) + 4));
    if model_kind == 2 {
        out_b = mul_rows(&m80, &out_b);
    } else {
        let mut inv = m40;
        invert(&mut inv);
        let p1 = mul_rows(&m80, &out_b);
        out_b = mul_rows(&inv, &p1);
    }
    register_record(m, map, &m40, mode, groups, &out_b, &out_a, objs, kind);
}

/// orig 0x148a58: the shadow of every object of the last object pass on all groups of the map. `map_ref` is the address of the
/// word that holds the map. It marks the stored mode block (0x484380) with bit 2, sets its second colour to 0x80 0x80 0x80
/// 0xff, and registers the record for the object list stored at 0x484378 with kind 0. The group list it builds counts
/// 0 to 30 and ends there (thirty-one groups, not thirty-two: the terminator takes the last slot of its 32-entry buffer).
pub fn shadow_pass_all<H: ShadowHost>(m: &mut Mem, h: &mut H, map_ref: u32) {
    let groups: Vec<u16> = (0..31).collect();
    let w = m.r32(SHADOW_BLOCK) | 2;
    m.w32(SHADOW_BLOCK, w);
    m.w8(SHADOW_BLOCK + 8, 0x80);
    m.w8(SHADOW_BLOCK + 11, 0xff);
    m.w8(SHADOW_BLOCK + 10, 0x80);
    m.w8(SHADOW_BLOCK + 9, 0x80);
    let mode: [u8; MODE_SIZE] = std::array::from_fn(|i| m.r8(SHADOW_BLOCK + i as u32));
    let map = m.r32(map_ref);
    let objs = m.r32(SHADOW_OBJS);
    project_shadow(m, h, map, &mode, &groups, objs, 0);
}

/// orig 0x148b68: collects the map groups a box touches, walking the group tree from `node`. The tree's node array is at
/// `+4` (0x30 bytes a node: eight child indices ending with -1 at `+0`, the node's box at `+0x10`, a range of group entries at
/// `+0x28` / `+0x2a`), the group entries at `+8` (0x1c bytes: a used flag, the layer at `+2`, the box at `+4`). A used entry
/// whose box overlaps `bx`, whose layer is `layer` or next to it, and that the view volume at `+0x230` sees (1) or half sees
/// (2, stored with 0x8000 set) goes into the list the word `+0x5364` points at, at index `*count`. Children whose box overlaps
/// are visited, at most eight a node.
pub fn collect_groups<H: ShadowHost>(m: &mut Mem, h: &mut H, tree: u32, node: i32, bx: &[f32; 6], count: &mut i32, layer: u32) {
    let list = m.r32(tree + 0x5364);
    let nodes = m.r32(tree + 4);
    let np = nodes.wrapping_add((node as u32).wrapping_mul(0x30));
    let start = m.r16(np + 0x28) as i16;
    let end = m.r16(np + 0x2a) as i16;
    let mut i = start as i32;
    let mut off = (start as i32).wrapping_mul(0x1c);
    while i < end as i32 {
        let e = m.r32(tree + 8).wrapping_add(off as u32);
        if m.r8(e) != 0 && overlap(&r_box(m, e + 4), bx) {
            let l = m.r8(e + 2) as u32;
            if l == layer || l == layer.wrapping_add(1) || l == layer.wrapping_sub(1) {
                let ebox = r_box(m, e + 4);
                let v = h.box_visible(m, tree + 0x230, &ebox);
                if v == 1 {
                    m.w16(list.wrapping_add((*count as u32).wrapping_mul(2)), i as u16);
                    *count += 1;
                } else if v == 2 {
                    m.w16(list.wrapping_add((*count as u32).wrapping_mul(2)), i as u16 | 0x8000);
                    *count += 1;
                }
            }
        }
        i += 1;
        off = off.wrapping_add(0x1c);
    }
    for c in 0..8u32 {
        let child = m.r16(np + 2 * c) as i16;
        if child == -1 {
            return;
        }
        let cb = r_box(m, m.r32(tree + 4).wrapping_add((child as i32 as u32).wrapping_mul(0x30)).wrapping_add(0x10));
        if overlap(&cb, bx) {
            collect_groups(m, h, tree, child as i32, bx, count, layer);
        }
    }
}

/// orig 0x148d58: collects an object and its descendants that cast a shadow (flag 8 or 0x40 at `+0xf8`) into the pointer
/// list at `list`; `count_at` is the address of the word that counts the entries so far. Children hang off `+0xf0` (first)
/// and `+0xec` of each child (next).
pub fn collect_casters(m: &mut Mem, obj: u32, list: u32, count_at: u32) {
    let f = m.r32(obj + 0xf8);
    if f & 8 != 0 || f & 0x40 != 0 {
        let n = m.r32(count_at);
        m.w32(list.wrapping_add(n.wrapping_mul(4)), obj);
        m.w32(count_at, n.wrapping_add(1));
    }
    let mut child = m.r32(obj + 0xf0);
    while child != 0 {
        collect_casters(m, child, list, count_at);
        child = m.r32(child + 0xec);
    }
}

/// orig 0x148e08: the shadow of `n` objects (`objs` is the address of their pointer list) on the map group tree of the render
/// context `tree`. Nothing happens when the first object's alpha byte (`+0x10c`) is zero. The box is the union of the objects'
/// boxes with its top raised by `(top - bottom) * 0.5` twice plus the first object's `+0x104`; it must be seen by the view at
/// `tree + 0x1f0` and not be inside an occluder. Flag 0x40 of the first object (`+0xf8`) makes the mode word's bit 0 and takes
/// the object's own colour and alpha (`+0xe8..+0xeb`); the shadow kind is 1 for flag 0x200, 2 for 0x400, else 0; the layer is
/// the byte at `+0x11e`. The groups come from [`collect_groups`] into the list at `list_buf` (the original keeps it on the
/// stack and stores its address in `tree + 0x5364`; `list_buf` must hold 1024 shorts), the mode block before its last changes
/// goes to 0x484380 and the object list address to 0x484378 for the all-groups pass, and the record is registered for the map
/// at `tree + 0x5368` with the second colour 0x80 0x80 0x80 0xff and mode bit 2.
pub fn shadow_pass_objects<H: ShadowHost>(m: &mut Mem, h: &mut H, tree: u32, objs: u32, n: i32, list_buf: u32) {
    let mut mode = mode_block_init(m);
    let o = m.r32(objs);
    mode[7] = m.r8(o + 0x10c);
    if mode[7] == 0 {
        return;
    }
    let mut bx = r_box(m, o + 0xd0);
    for k in 0..n.max(0) as u32 {
        let c = r_box(m, m.r32(objs + 4 * k) + 0xd0);
        bx = union_box(&bx, &c);
    }
    let half = (bx[4] - bx[1]) * 0.5;
    bx[4] = bx[4] + half + half + m.rf32(o + 0x104);
    if h.box_visible(m, tree + 0x1f0, &bx) == 0 || h.occluded(m, tree, &bx) == 1 {
        return;
    }
    let mut flags = m.r32(o + 0xf8);
    if flags & 0x40 != 0 {
        mode[0] |= 1;
        for i in 0..4 {
            mode[4 + i] = m.r8(o + 0xe8 + i as u32);
        }
        flags = m.r32(o + 0xf8);
    }
    let kind = if flags & 0x200 != 0 {
        1
    } else if flags & 0x400 != 0 {
        2
    } else {
        0
    };
    let layer = m.r8(o + 0x11e) as u32;
    for (i, b) in mode.iter().enumerate() {
        m.w8(SHADOW_BLOCK + i as u32, *b);
    }
    m.w32(SHADOW_OBJS, objs);
    let mut count = 0i32;
    m.w32(tree + 0x5364, list_buf);
    m.w16(list_buf, 0xffff);
    collect_groups(m, h, tree, 0, &bx, &mut count, layer);
    mode[8] = 0x80;
    mode[11] = 0xff;
    mode[0] |= 2;
    mode[10] = 0x80;
    mode[9] = 0x80;
    m.w16(list_buf + 2 * count as u32, 0xffff);
    let groups = read_list(m, list_buf);
    let map = m.r32(tree + 0x5368);
    project_shadow(m, h, map, &mode, &groups, objs, kind);
}
