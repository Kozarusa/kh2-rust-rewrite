//! The map's draw tree: build, view matrices, cull walk, occluders, object registration and move, and the show/hide
//! of drawn mesh groups (WP-L1-P25a, spec `docs/systems/core-runtime/render-model.md` c3 and c4).
//!
//! The tree is the 0x5380-byte object the game keeps behind the word `0x355930`. Its state is laid out byte for byte
//! as the original has it (RULES C7): the port does not hold a Rust copy of the node and group tables but reads and
//! writes a [`Ram`], a sparse model of the game's address space, at the original's offsets. That is what lets the
//! oracle tests compare the whole block after a call, and it is how the objects the tree links (actors, props, the
//! sky) can stay where the rest of the port keeps them: an object is an address into the same [`Ram`].
//!
//! Tree offsets: `+0x00` the map file (a node and group table follow its header), `+0x04` nodes (0x30 bytes each),
//! `+0x08` groups (0x1c bytes each), `+0x0c` a third table, `+0x10 +0x14 +0x18` the three counts, `+0x1c` the output
//! list the walk writes, `+0x20` a copy of the projection block (0x100 bytes), `+0x120` a copy of the camera block,
//! `+0x130` the camera's view matrix, `+0x1b0` view times screen projection, `+0x1f0` view times cull matrix,
//! `+0x230` view times the clip-to-GS matrix, `+0x270` up to 16 occluder volumes of five planes (0x50 bytes each),
//! `+0x770` their count, `+0x774` the player's view depth, `+0x778` the see-through switch, `+0x779` the
//! sort-by-group switch, `+0x790` the see-through effect, `+0xc44..+0xc4c` the player's screen rectangle, `+0xc50`
//! the per-view translucent batch counters, `+0xc60` sixteen translucent batches of 0x370 bytes, `+0x4360` 512
//! object slots of eight bytes (object address, flag word), `+0x5360` the slot count, `+0x5364` the shadow group
//! list, `+0x5368` the map model, `+0x536c` the view index, `+0x5370` and `+0x5374` the two skies.
//!
//! Node (0x30 bytes): `+0x00` up to eight child indices (s16, -1 ends the list), `+0x10` box (six floats), `+0x28`
//! first group, `+0x2a` end group (s16, half open), `+0x2c` head of the node's object list (the link is object
//! `+0x110`). Group (0x1c bytes): `+0x00` shown byte, `+0x01` group number, `+0x04` box. Draw object (the block of an
//! actor, prop or sky): `+0x08` flag word, `+0x40` matrix, `+0x80` its copy from the last frame, `+0xc0` position,
//! `+0xd0` box, `+0xeb` alpha byte, `+0xec` sibling, `+0xf0` first child, `+0xf8` draw flags, `+0xfc` pointer to eight
//! corner points or 0, `+0x110` node list link, `+0x118` node, `+0x11a` depth, `+0x11c` slot index.
//!
//! What the original does through other units is a [`Host`] call: the constructors and destructors of the
//! translucent batches and the see-through effect, the occluder volume builder, the player's position, the map model's
//! own draw, and the object pass. [`NoHost`] does none of them.

use crate::geom::{FBox, V4};
use crate::mapcoll::MapCollision;
use kh2_model::draw::frustum::{self, M4, Plane};
use kh2_model::draw;
use std::collections::HashMap;

/// The address of the word that holds the tree (`DAT_00355930`).
pub const TREE_WORD: u32 = 0x355930;
/// The occluder object word (`DAT_00355940`).
pub const OCCLUDER_WORD: u32 = 0x355940;
/// The map model instance word (`DAT_00355948`).
pub const MODEL_WORD: u32 = 0x355948;
/// The byte `0x35594c`: the player box is wanted.
pub const FLAG_PLAYER_BOX: u32 = 0x35594c;
/// The byte `0x35594d`: the map model is not drawn while it is 1.
pub const FLAG_MAP_OFF: u32 = 0x35594d;
/// The byte `0x35594e`: the whole map pass is skipped while it is 1 (the area fade holds the last frame).
pub const FLAG_FREEZE: u32 = 0x35594e;
/// The byte `0x35594f`: occluder volumes are built.
pub const FLAG_OCCLUDERS: u32 = 0x35594f;
/// The word `0x356f40`: the field's frame step the texture players advance by.
pub const FRAME_STEP_WORD: u32 = 0x356f40;
/// Bytes of the tree object.
pub const TREE_SIZE: u32 = 0x5380;

/// Offsets inside the tree object.
pub mod off {
    pub const FILE: u32 = 0x0;
    pub const NODES: u32 = 0x4;
    pub const GROUPS: u32 = 0x8;
    pub const THIRD: u32 = 0xc;
    pub const NODE_COUNT: u32 = 0x10;
    pub const GROUP_COUNT: u32 = 0x14;
    pub const THIRD_COUNT: u32 = 0x18;
    pub const LIST: u32 = 0x1c;
    pub const PROJECTION: u32 = 0x20;
    pub const CAMERA: u32 = 0x120;
    pub const VIEW_MATRIX: u32 = 0x130;
    pub const CLIP_SCREEN: u32 = 0x1b0;
    pub const CLIP_CULL: u32 = 0x1f0;
    pub const CLIP_GS: u32 = 0x230;
    pub const VOLUMES: u32 = 0x270;
    pub const VOLUME_COUNT: u32 = 0x770;
    pub const PLAYER_DEPTH: u32 = 0x774;
    pub const SEE_THROUGH: u32 = 0x778;
    pub const SORT_BY_GROUP: u32 = 0x779;
    pub const SEE_THROUGH_EFFECT: u32 = 0x790;
    pub const PLAYER_RECT: u32 = 0xc44;
    pub const BATCHES: u32 = 0xc60;
    pub const BATCH_SIZE: u32 = 0x370;
    pub const SLOTS: u32 = 0x4360;
    pub const SLOT_COUNT: u32 = 0x5360;
    pub const SHADOW_LIST: u32 = 0x5364;
    pub const MODEL: u32 = 0x5368;
    pub const VIEW: u32 = 0x536c;
    pub const SKY0: u32 = 0x5370;
    pub const SKY1: u32 = 0x5374;
}

/// Size of a node, of a group.
const NODE: u32 = 0x30;
const GROUP: u32 = 0x1c;

/// The vtable words `0x1459d0` leaves in the see-through effect while it tears the tree down.
const SEE_THROUGH_VTABLE_A: u32 = 0x35a7e0;
const SEE_THROUGH_VTABLE_B: u32 = 0x355e88;
const RECT_VTABLE: u32 = 0x356ef0;

const PAGE: u32 = 0x1000;

/// A sparse model of the game's memory (4 KB pages created on first touch, zero filled).
#[derive(Clone, Default)]
pub struct Ram {
    pages: HashMap<u32, Box<[u8; PAGE as usize]>>,
}

impl Ram {
    pub fn new() -> Ram {
        Ram::default()
    }

    fn page(&mut self, a: u32) -> &mut [u8; PAGE as usize] {
        self.pages.entry(a / PAGE).or_insert_with(|| Box::new([0; PAGE as usize]))
    }

    pub fn r8(&self, a: u32) -> u8 {
        self.pages.get(&(a / PAGE)).map_or(0, |p| p[(a % PAGE) as usize])
    }

    pub fn w8(&mut self, a: u32, v: u8) {
        self.page(a)[(a % PAGE) as usize] = v;
    }

    pub fn r16(&self, a: u32) -> u16 {
        u16::from_le_bytes([self.r8(a), self.r8(a.wrapping_add(1))])
    }

    pub fn w16(&mut self, a: u32, v: u16) {
        for (i, b) in v.to_le_bytes().into_iter().enumerate() {
            self.w8(a.wrapping_add(i as u32), b);
        }
    }

    pub fn r32(&self, a: u32) -> u32 {
        u32::from_le_bytes(std::array::from_fn(|i| self.r8(a.wrapping_add(i as u32))))
    }

    pub fn w32(&mut self, a: u32, v: u32) {
        for (i, b) in v.to_le_bytes().into_iter().enumerate() {
            self.w8(a.wrapping_add(i as u32), b);
        }
    }

    pub fn rf(&self, a: u32) -> f32 {
        f32::from_bits(self.r32(a))
    }

    pub fn wf(&mut self, a: u32, v: f32) {
        self.w32(a, v.to_bits());
    }

    /// `len` bytes from `a`.
    pub fn read(&self, a: u32, len: u32) -> Vec<u8> {
        (0..len).map(|i| self.r8(a.wrapping_add(i))).collect()
    }

    /// Stores `bytes` at `a`.
    pub fn write(&mut self, a: u32, bytes: &[u8]) {
        for (i, b) in bytes.iter().enumerate() {
            self.w8(a.wrapping_add(i as u32), *b);
        }
    }

    /// Four floats at `a`.
    pub fn v4(&self, a: u32) -> V4 {
        std::array::from_fn(|k| self.rf(a + 4 * k as u32))
    }

    /// A 4x4 matrix at `a` (four rows).
    pub fn m4(&self, a: u32) -> M4 {
        std::array::from_fn(|k| self.v4(a + 16 * k as u32))
    }

    pub fn put_v4(&mut self, a: u32, v: V4) {
        for (k, f) in v.iter().enumerate() {
            self.wf(a + 4 * k as u32, *f);
        }
    }

    pub fn put_m4(&mut self, a: u32, m: &M4) {
        for (k, row) in m.iter().enumerate() {
            self.put_v4(a + 16 * k as u32, *row);
        }
    }

    /// Six floats at `a` as a box.
    pub fn fbox(&self, a: u32) -> FBox {
        std::array::from_fn(|k| self.rf(a + 4 * k as u32))
    }

    /// Copies `words` 32-bit words from `src` to `dst`, word by word in order (the original's copies read all words
    /// first; source and destination never overlap here).
    fn copy_words(&mut self, dst: u32, src: u32, words: u32) {
        for i in 0..words {
            let v = self.r32(src + 4 * i);
            self.w32(dst + 4 * i, v);
        }
    }
}

/// The object pass of `kh2_model` reads and writes the game's memory through this view.
impl kh2_model::draw::pass::Words for Ram {
    fn r8(&self, a: u32) -> u8 {
        Ram::r8(self, a)
    }
    fn w8(&mut self, a: u32, v: u8) {
        Ram::w8(self, a, v)
    }
    fn r16(&self, a: u32) -> u16 {
        Ram::r16(self, a)
    }
    fn w16(&mut self, a: u32, v: u16) {
        Ram::w16(self, a, v)
    }
    fn r32(&self, a: u32) -> u32 {
        Ram::r32(self, a)
    }
    fn w32(&mut self, a: u32, v: u32) {
        Ram::w32(self, a, v)
    }
}

/// What the original reaches through units outside this package. Every method has a no-op default.
pub trait Host {
    /// `0x15dff8`: the constructor of the see-through effect at tree `+0x790`.
    fn construct_see_through(&mut self, _ram: &mut Ram, _at: u32) {}
    /// `0x15e4f8`: the constructor of one translucent batch (16 of them from tree `+0xc60`).
    fn construct_batch(&mut self, _ram: &mut Ram, _at: u32) {}
    /// The virtual destructor call `0x1459d0` makes on each batch (the batch's own vtable entry 0).
    fn destroy_batch(&mut self, _ram: &mut Ram, _at: u32) {}
    /// `0x32e530` (on tree `+0x930`) and `0x32e520` (on tree `+0x820`): destructors of two members of the effect.
    fn destroy_member(&mut self, _ram: &mut Ram, _at: u32, _which: u32) {}
    /// `0x165990`: fills the occluder volumes of the occluder object for the eye position: `planes` is tree `+0x270`,
    /// `count` tree `+0x770`, `cull` the matrix at tree `+0x1f0`.
    fn occluder_volumes(&mut self, _ram: &mut Ram, _occluder: u32, _eye: V4, _planes: u32, _count: u32, _cull: u32) {}
    /// The player's position vector (`0x16aa08` then `0x16b960`), `None` when there is no player.
    fn player_position(&mut self, _ram: &mut Ram) -> Option<V4> {
        None
    }
    /// `0x11c268` on the map model handle (the model's base pose).
    fn model_pose_reset(&mut self, _ram: &mut Ram, _model: u32) {}
    /// `0x11e7f8(step, model)`: the model's texture animation objects advance by the field step.
    fn model_texture_update(&mut self, _ram: &mut Ram, _step: u32, _model: u32) {}
    /// `0x11cb38(model, matrix, instance + 0x10, instance + 0x50, list, 0)`: the map model's draw with the group list.
    fn draw_map_model(&mut self, _ram: &mut Ram, _model: u32, _matrix: &M4, _arg_a: u32, _arg_b: u32, _list: u32) {}
    /// `0x147300`: the object pass (sky, sort, layers, translucent batches).
    fn object_pass(&mut self, _ram: &mut Ram, _tree: u32) {}
}

/// A host that does nothing.
pub struct NoHost;

impl Host for NoHost {}

/// The tree, by its address in a [`Ram`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawTree(pub u32);

fn s16(r: &Ram, a: u32) -> i32 {
    r.r16(a) as i16 as i32
}

fn clear_flags(r: &mut Ram, a: u32, bits: u32) {
    let v = r.r32(a);
    r.w32(a, v & !bits);
}

fn set_flags(r: &mut Ram, a: u32, bits: u32) {
    let v = r.r32(a);
    r.w32(a, v | bits);
}

impl DrawTree {
    fn nodes(self, r: &Ram) -> u32 {
        r.r32(self.0 + off::NODES)
    }

    fn groups(self, r: &Ram) -> u32 {
        r.r32(self.0 + off::GROUPS)
    }

    /// The address of node `i` (the index is a short, sign extended).
    fn node(self, r: &Ram, i: i32) -> u32 {
        self.nodes(r).wrapping_add((i.wrapping_mul(NODE as i32)) as u32)
    }

    fn group(self, r: &Ram, i: i32) -> u32 {
        self.groups(r).wrapping_add((i.wrapping_mul(GROUP as i32)) as u32)
    }

    /// orig 0x145748: builds the tree at this address over the map file's image at `file` (an address in `ram`) with
    /// the see-through switch `flag` and the 64-bit group mask. The file header words at `+0x14`, `+0x1c`, `+0x24` are
    /// the offsets of the three tables, `+0x18`, `+0x20`, `+0x28` their byte sizes (the counts are those divided by
    /// 0x30, 0x1c, 0x34). The root node's box becomes (-1e8, -1e8, -1e8) to (1e8, 1e8, 1e8) (the executable's
    /// words `0x37900c` and `0x379010`). The projection and camera constructors (0x106918, 0x106f28) run in place; the
    /// see-through effect and the 16 translucent batches are constructed by the host, in this order.
    pub fn build(self, ram: &mut Ram, file: u32, flag: u8, mask: u64, host: &mut impl Host) {
        let t = self.0;
        for o in [off::NODES, off::GROUPS, off::THIRD, off::NODE_COUNT, off::GROUP_COUNT, off::THIRD_COUNT, off::LIST] {
            ram.w32(t + o, 0);
        }
        draw::projection_construct();
        let mut cam = ram.read(t + off::CAMERA, 4);
        draw::camera_construct(&mut cam);
        ram.write(t + off::CAMERA, &cam);
        ram.w8(t + off::SEE_THROUGH, flag);
        ram.w8(t + off::SORT_BY_GROUP, 0);
        host.construct_see_through(ram, t + off::SEE_THROUGH_EFFECT);
        for i in 0..16 {
            host.construct_batch(ram, t + off::BATCHES + off::BATCH_SIZE * i);
        }
        for i in 0..512 {
            ram.w32(t + off::SLOTS + 8 * i, 0);
            ram.w32(t + off::SLOTS + 8 * i + 4, 0);
        }
        ram.w32(t + off::FILE, file);
        for o in [off::SHADOW_LIST, off::VIEW, off::SKY0, off::SKY1, 0x5378, 0x537c] {
            ram.w32(t + o, 0);
        }
        let nodes = file.wrapping_add(ram.r32(file + 0x14));
        ram.w32(t + off::NODES, nodes);
        let groups = file.wrapping_add(ram.r32(file + 0x1c));
        ram.w32(t + off::GROUPS, groups);
        let third = file.wrapping_add(ram.r32(file + 0x24));
        ram.w32(t + off::THIRD, third);
        ram.w32(t + off::NODE_COUNT, (ram.r32(file + 0x18) as i32 / 0x30) as u32);
        ram.w32(t + off::GROUP_COUNT, (ram.r32(file + 0x20) as i32 / 0x1c) as u32);
        ram.w8(t + off::SEE_THROUGH, flag);
        ram.w32(t + off::THIRD_COUNT, (ram.r32(file + 0x28) as i32 / 0x34) as u32);
        let root_box: FBox = [-1.0e8, -1.0e8, -1.0e8, 1.0e8, 1.0e8, 1.0e8];
        for (k, f) in root_box.iter().enumerate() {
            ram.wf(nodes + 0x10 + 4 * k as u32, *f);
        }
        self.show_mask(ram, mask);
        self.reset_object_lists(ram);
    }

    /// orig 0x1459d0: tears the tree down: the 16 batch destructors (the last first), the vtable words of the effect,
    /// the two member destructors, and the camera and projection destructors.
    pub fn release(self, ram: &mut Ram, host: &mut impl Host) {
        let t = self.0;
        for i in (0..16).rev() {
            host.destroy_batch(ram, t + off::BATCHES + off::BATCH_SIZE * i);
        }
        ram.w32(t + off::SEE_THROUGH_EFFECT, SEE_THROUGH_VTABLE_A);
        host.destroy_member(ram, t + 0x930, 0x32e530);
        ram.w32(t + 0x820, RECT_VTABLE);
        host.destroy_member(ram, t + 0x820, 0x32e520);
        ram.w32(t + off::SEE_THROUGH_EFFECT, SEE_THROUGH_VTABLE_B);
        let mut cam = ram.read(t + off::CAMERA, 4);
        draw::camera_destruct(&mut cam);
        ram.write(t + off::CAMERA, &cam);
        draw::projection_destruct();
    }

    /// orig 0x145aa8: clears the shown byte of every group, then shows the groups numbered by the set bits of `mask`.
    pub fn show_mask(self, ram: &mut Ram, mask: u64) {
        let n = ram.r32(self.0 + off::GROUP_COUNT) as i32;
        for i in 0..n.max(0) {
            let g = self.group(ram, i);
            ram.w8(g, 0);
        }
        for b in 0..64u32 {
            if mask & (1u64 << b) != 0 {
                self.group_show(ram, b as u8);
            }
        }
    }

    /// orig 0x145b40: hides every group numbered `number`.
    pub fn group_hide(self, ram: &mut Ram, number: u8) {
        let n = ram.r32(self.0 + off::GROUP_COUNT) as i32;
        for i in 0..n.max(0) {
            let g = self.group(ram, i);
            if ram.r8(g + 1) == number {
                ram.w8(g, 0);
            }
        }
    }

    /// orig 0x145b80: shows every group numbered `number`.
    pub fn group_show(self, ram: &mut Ram, number: u8) {
        let n = ram.r32(self.0 + off::GROUP_COUNT) as i32;
        for i in 0..n.max(0) {
            let g = self.group(ram, i);
            if ram.r8(g + 1) == number {
                ram.w8(g, 1);
            }
        }
    }

    /// orig 0x145bc8: the per-frame matrices. Copies the current projection block (`0x106878(1)`) and camera block
    /// (`0x1068c0(1)`) into the tree and builds the three clip matrices, each the matrix applied to the rows of the
    /// camera's view matrix: `+0x1b0` with the projection's own matrix, `+0x1f0` with its cull matrix, `+0x230` with
    /// its clip-to-GS matrix. The copies skip the words the original skips: the projection's `+0x7c` and the camera's
    /// block outside `+0x10..+0x83`.
    pub fn frame_matrices(self, ram: &mut Ram) {
        let t = self.0;
        let index = ram.r32(draw::VIEW_INDEX_WORD);
        let select = ram.r8(draw::VIEW_SELECT_BYTE);
        let src = draw::projection_address(1, select, index);
        let dst = t + off::PROJECTION;
        if dst != src {
            ram.copy_words(dst, src, 16);
        }
        ram.copy_words(dst + 0x40, src + 0x40, 8);
        ram.copy_words(dst + 0x60, src + 0x60, 7);
        if dst + 0x80 != src + 0x80 {
            ram.copy_words(dst + 0x80, src + 0x80, 16);
        }
        if dst + 0xc0 != src + 0xc0 {
            ram.copy_words(dst + 0xc0, src + 0xc0, 16);
        }
        let cull = draw::projection_cull_matrix(dst);
        let gs = draw::projection_view_clip_gs_matrix(dst);
        let cam = draw::camera_address(1, select, index);
        let view = t + off::VIEW_MATRIX;
        ram.copy_words(view, cam + 0x10, 16);
        ram.copy_words(t + 0x170, cam + 0x50, 4);
        ram.copy_words(t + 0x180, cam + 0x60, 4);
        ram.copy_words(t + 0x190, cam + 0x70, 4);
        let roll = ram.r32(cam + 0x80);
        ram.w32(t + 0x1a0, roll);
        let v = ram.m4(view);
        let screen = frustum::matrix_product(&ram.m4(dst), &v);
        ram.put_m4(t + off::CLIP_SCREEN, &screen);
        let cull_m = frustum::matrix_product(&ram.m4(cull), &v);
        ram.put_m4(t + off::CLIP_CULL, &frustum::matrix_copy(&cull_m));
        let gs_m = frustum::matrix_product(&ram.m4(gs), &v);
        ram.put_m4(t + off::CLIP_GS, &frustum::matrix_copy(&gs_m));
    }

    /// orig 0x145f88: sets the output list the walk appends to (tree `+0x1c`).
    pub fn set_list(self, ram: &mut Ram, list: u32) {
        ram.w32(self.0 + off::LIST, list);
    }

    /// orig 0x145f90: sets the model word (tree `+0x5368`).
    pub fn set_model(self, ram: &mut Ram, model: u32) {
        ram.w32(self.0 + off::MODEL, model);
    }

    /// orig 0x145f98: sets the shadow group list word (tree `+0x5364`).
    pub fn set_shadow_list(self, ram: &mut Ram, list: u32) {
        ram.w32(self.0 + off::SHADOW_LIST, list);
    }

    /// orig 0x145fa0: is the box at `box_addr` inside any occluder volume? True at the first volume for which
    /// [`frustum::box_inside_planes`] holds.
    pub fn in_volume(self, ram: &Ram, box_addr: u32) -> bool {
        let n = ram.r32(self.0 + off::VOLUME_COUNT) as i32;
        let b = ram.fbox(box_addr);
        for i in 0..n.max(0) as u32 {
            let base = self.0 + off::VOLUMES + 0x50 * i;
            let planes: [Plane; 5] = std::array::from_fn(|k| ram.v4(base + 16 * k as u32));
            if frustum::box_inside_planes(&planes, &b) {
                return true;
            }
        }
        false
    }

    /// orig 0x146018: appends `value` to the output list and advances the count.
    pub fn list_push(self, ram: &mut Ram, count: &mut i32, value: u16) {
        let list = ram.r32(self.0 + off::LIST);
        ram.w16(list.wrapping_add((*count).wrapping_mul(2) as u32), value);
        *count = count.wrapping_add(1);
    }

    /// orig 0x146040: the second clip test of a group the first one found crossing: against the clip-to-GS matrix. A
    /// group fully inside is appended as is, a crossing one with bit 15 set (it needs clipping).
    pub fn list_group_clipped(self, ram: &mut Ram, count: &mut i32, group: u16) {
        let m = ram.m4(self.0 + off::CLIP_GS);
        let b = ram.fbox(self.group(ram, group as i16 as i32) + 4);
        match frustum::clip_box(&m, &b) {
            frustum::CLIP_INSIDE => self.list_push(ram, count, group as i16 as u16),
            frustum::CLIP_CROSSES => self.list_push(ram, count, group | 0x8000),
            _ => {}
        }
    }

    /// The visible mark, 0x148920, ported with the object pass in `kh2_model::draw::pass::mark_visible`.
    fn mark_visible(self, ram: &mut Ram, obj: u32) {
        kh2_model::draw::pass::mark_visible(ram, self.0, obj);
    }

    /// orig 0x1470f0: marks the object visible and clears bit 2 of its flag word `+0x08`.
    pub fn visible(self, ram: &mut Ram, obj: u32) {
        self.mark_visible(ram, obj);
        clear_flags(ram, obj + 8, 4);
    }

    /// orig 0x147128: marks the object visible with the clip mark (bit 2 of `+0x08` set).
    pub fn visible_clipped(self, ram: &mut Ram, obj: u32) {
        self.mark_visible(ram, obj);
        set_flags(ram, obj + 8, 4);
    }

    /// orig 0x146108: marks every object of the node visible.
    pub fn node_objects_visible(self, ram: &mut Ram, node: u32) {
        let mut o = ram.r32(node + 0x2c);
        while o != 0 {
            self.visible(ram, o);
            o = ram.r32(o + 0x110);
        }
    }

    /// orig 0x146168: the objects of a node against the clip matrices (rules in the module's c3 text: bit 0x1000 skips
    /// the test, bit 0x800 asks for it, bit 0x20 asks for the second test and the clip mark).
    pub fn node_objects(self, ram: &mut Ram, node: u32) {
        let mut o = ram.r32(node + 0x2c);
        while o != 0 {
            let flags = ram.r32(o + 0xf8);
            if flags & 0x1000 != 0 {
                if flags & 0x20 != 0 {
                    self.visible_clipped(ram, o);
                } else {
                    self.visible(ram, o);
                }
            } else if flags & 0x800 == 0 {
                self.visible(ram, o);
            } else {
                let given = ram.r32(o + 0xfc);
                let points: [V4; 8] = if given == 0 {
                    box_corners(&ram.fbox(o + 0xd0))
                } else {
                    std::array::from_fn(|k| ram.v4(given + 16 * k as u32))
                };
                match frustum::clip_points(&ram.m4(self.0 + off::CLIP_CULL), &points) {
                    frustum::CLIP_INSIDE => self.visible(ram, o),
                    frustum::CLIP_CROSSES => {
                        if ram.r32(o + 0xf8) & 0x20 != 0 {
                            match frustum::clip_points(&ram.m4(self.0 + off::CLIP_GS), &points) {
                                frustum::CLIP_INSIDE => self.visible(ram, o),
                                frustum::CLIP_CROSSES => self.visible_clipped(ram, o),
                                _ => {}
                            }
                        } else {
                            self.visible(ram, o);
                        }
                    }
                    _ => {}
                }
            }
            o = ram.r32(o + 0x110);
        }
    }

    /// orig 0x1462a8: the walk of a node known to be fully inside the view: nothing in an occluder volume, else every
    /// shown group, every object marked visible, then the children (at most eight).
    pub fn walk_inside(self, ram: &mut Ram, node_index: i32, count: &mut i32) {
        let node = self.node(ram, node_index);
        if self.in_volume(ram, node + 0x10) {
            return;
        }
        let first = s16(ram, node + 0x28);
        let end = s16(ram, node + 0x2a);
        for g in first..end {
            if ram.r8(self.group(ram, g)) != 0 {
                self.list_push(ram, count, g as u16);
            }
        }
        self.node_objects_visible(ram, node);
        for k in 0..8u32 {
            let child = s16(ram, node + 2 * k);
            if child == -1 {
                return;
            }
            self.walk_inside(ram, child, count);
        }
    }

    /// orig 0x1463b0: the cull walk from `node_index` (the caller starts with 0). A node outside the cull matrix's view
    /// ends the branch, one inside is walked by [`DrawTree::walk_inside`], one crossing is tested group by group and its
    /// objects one by one, then its children (at most eight) are walked.
    pub fn walk(self, ram: &mut Ram, node_index: i32, count: &mut i32) {
        let node = self.node(ram, node_index);
        let cull = ram.m4(self.0 + off::CLIP_CULL);
        match frustum::clip_box(&cull, &ram.fbox(node + 0x10)) {
            frustum::CLIP_INSIDE => {
                self.walk_inside(ram, node_index, count);
                return;
            }
            frustum::CLIP_CROSSES => {
                if self.in_volume(ram, node + 0x10) {
                    return;
                }
                let first = s16(ram, node + 0x28);
                let end = s16(ram, node + 0x2a);
                for g in first..end {
                    let ga = self.group(ram, g);
                    if ram.r8(ga) == 0 {
                        continue;
                    }
                    match frustum::clip_box(&ram.m4(self.0 + off::CLIP_CULL), &ram.fbox(ga + 4)) {
                        frustum::CLIP_INSIDE => self.list_push(ram, count, g as u16),
                        frustum::CLIP_CROSSES => self.list_group_clipped(ram, count, g as u16),
                        _ => {}
                    }
                }
                self.node_objects(ram, node);
            }
            _ => return,
        }
        for k in 0..8u32 {
            let child = s16(ram, node + 2 * k);
            if child == -1 {
                return;
            }
            self.walk(ram, child, count);
        }
    }

    /// orig 0x146c40: empties the object slots and every node's object list (the build and the per-frame clear call
    /// this; the slot count becomes 0).
    pub fn reset_object_lists(self, ram: &mut Ram) {
        ram.w32(self.0 + off::SLOT_COUNT, 0);
        let n = ram.r32(self.0 + off::NODE_COUNT) as i32;
        for i in 0..n.max(0) {
            let node = self.node(ram, i);
            ram.w32(node + 0x2c, 0);
        }
    }

    /// orig 0x146c80: the per-frame clear before the actors move: the slot count becomes 0, and every object of every
    /// node list gets its matrix `+0x40` copied to `+0x80` (the last frame's matrix) before the lists are emptied.
    pub fn clear_object_lists(self, ram: &mut Ram) {
        ram.w32(self.0 + off::SLOT_COUNT, 0);
        let n = ram.r32(self.0 + off::NODE_COUNT) as i32;
        let nodes = self.nodes(ram);
        for i in 0..n.max(0) {
            let head = nodes.wrapping_add((i * NODE as i32) as u32) + 0x2c;
            let mut o = ram.r32(head);
            while o != 0 {
                ram.copy_words(o + 0x80, o + 0x40, 16);
                o = ram.r32(o + 0x110);
            }
            ram.w32(head, 0);
        }
    }

    /// orig 0x146cf8: clears the visible marks of every registered object (bit 1 of its draw flags, bit 0 of its slot
    /// word).
    pub fn clear_visible(self, ram: &mut Ram) {
        let n = ram.r32(self.0 + off::SLOT_COUNT) as i32;
        for i in 0..n.max(0) as u32 {
            let slot = self.0 + 8 * i;
            let o = ram.r32(slot + off::SLOTS);
            clear_flags(ram, o + 0xf8, 2);
            clear_flags(ram, slot + off::SLOTS + 4, 1);
        }
    }

    /// orig 0x146d40: registers the object: slot index into `+0x11c`, the object into the next slot, and the object at
    /// the head of its node's list (the node is the object's `+0x118`).
    pub fn register(self, ram: &mut Ram, obj: u32) {
        let t = self.0;
        let n = ram.r32(t + off::SLOT_COUNT);
        ram.w16(obj + 0x11c, n as u16);
        ram.w32(t + off::SLOTS + 8 * n, obj);
        ram.w32(t + off::SLOT_COUNT, n.wrapping_add(1));
        let node = self.nodes(ram).wrapping_add(ram.r16(obj + 0x118) as u32 * NODE);
        let head = ram.r32(node + 0x2c);
        ram.w32(obj + 0x110, head);
        ram.w32(node + 0x2c, obj);
    }

    /// orig 0x146d88: stores sky object `which` (0 the first sky, anything else the second).
    pub fn set_sky(self, ram: &mut Ram, obj: u32, which: i64) {
        let at = if which == 0 { off::SKY0 } else { off::SKY1 };
        ram.w32(self.0 + at, obj);
    }

    /// orig 0x146da0: adds an object whose draw flags have bit 4 or bit 8: its (next) slot word gets bit 1 and the
    /// object is registered in the node it already names. Other objects are ignored.
    pub fn add_object(self, ram: &mut Ram, obj: u32) {
        let f = ram.r32(obj + 0xf8);
        if f & 4 == 0 && f & 8 == 0 {
            return;
        }
        let slot = self.0 + 8 * ram.r32(self.0 + off::SLOT_COUNT);
        set_flags(ram, slot + 0x4364, 2);
        self.register(ram, obj);
    }

    /// orig 0x146e40: moves an object to the node that now holds its box and registers it again, then moves its
    /// children (first child `+0xf0`, siblings `+0xec`) with `top` 0. An object without bit 4 or 8 is not registered
    /// but its children are still moved. With `top` 0 the slot word loses bit 1 and the node becomes 0.
    pub fn move_object(self, ram: &mut Ram, obj: u32, top: u8) {
        let flags = ram.r32(obj + 0xf8);
        if flags & 4 != 0 || flags & 8 != 0 {
            let slot = self.0 + 8 * ram.r32(self.0 + off::SLOT_COUNT) + 0x4364;
            let search = flags & 0x1000 == 0 && cull_state(ram, obj) == 0;
            if top == 0 {
                clear_flags(ram, slot, 2);
                ram.w16(obj + 0x11a, 0);
                ram.w16(obj + 0x118, 0);
            } else {
                set_flags(ram, slot, 2);
                if search {
                    self.search_node(ram, obj);
                } else {
                    ram.w16(obj + 0x11a, 0);
                    ram.w16(obj + 0x118, 0);
                }
            }
            self.register(ram, obj);
        }
        let mut c = ram.r32(obj + 0xf0);
        while c != 0 {
            self.move_object(ram, c, 0);
            c = ram.r32(c + 0xec);
        }
    }

    /// orig 0x146f68: the deepest node whose box holds the object's box (`obj + 0xd0`), searched from `node` at `depth`.
    /// `best` is the depth of the best hit so far, replaced when a node at the same or a greater depth holds the box;
    /// a child hit (a nonzero index) replaces the result, so the last deepest hit wins. Returns 0 when nothing holds
    /// the box.
    pub fn find_node(self, ram: &Ram, node: i32, obj: u32, depth: i32, best: &mut i32) -> i32 {
        let n = self.node(ram, node);
        if !contains(&ram.fbox(n + 0x10), &ram.fbox(obj + 0xd0)) {
            return 0;
        }
        let mut result = 0;
        if *best <= depth {
            *best = depth;
            result = node;
        }
        for k in 0..8u32 {
            let child = s16(ram, n + 2 * k);
            if child == -1 {
                return result;
            }
            let r = self.find_node(ram, child, obj, depth + 1, best);
            if r != 0 {
                result = r;
            }
        }
        result
    }

    /// orig 0x147058: searches the object's node again: from its current node and depth when that node's box still
    /// holds the object's box, otherwise from the root; stores the node (`+0x118`) and depth (`+0x11a`).
    pub fn search_node(self, ram: &mut Ram, obj: u32) {
        let current = ram.r16(obj + 0x118);
        let node = self.nodes(ram).wrapping_add(current as u32 * NODE);
        let (start, depth) = if contains(&ram.fbox(node + 0x10), &ram.fbox(obj + 0xd0)) {
            (current, ram.r16(obj + 0x11a))
        } else {
            (0, 0)
        };
        let mut best = depth as i32;
        let found = self.find_node(ram, start as i32, obj, depth as i32, &mut best);
        ram.w16(obj + 0x118, found as u16);
        ram.w16(obj + 0x11a, best as u16);
    }

    /// orig 0x143f78 (the tree is the global's): empties the occluder volumes, and when the occluder object exists
    /// and its switch byte is set has the host build them from the camera position.
    pub fn build_occluders(self, ram: &mut Ram, host: &mut impl Host) {
        ram.w32(self.0 + off::VOLUME_COUNT, 0);
        let occluder = ram.r32(OCCLUDER_WORD);
        if occluder != 0 && ram.r8(FLAG_OCCLUDERS) != 0 {
            let cam = draw::camera_address(1, ram.r8(draw::VIEW_SELECT_BYTE), ram.r32(draw::VIEW_INDEX_WORD));
            let eye = ram.v4(cam + 0x50);
            host.occluder_volumes(ram, occluder, eye, self.0 + off::VOLUMES, self.0 + off::VOLUME_COUNT, self.0 + off::CLIP_CULL);
        }
    }

    /// orig 0x143ff8: the player's screen rectangle for the see-through effect, when the byte `0x35594c` is set. The
    /// player's view depth (the z lane of its position times the view matrix) goes to `+0x774` (0 without a player).
    /// The position and the position lowered by 200 go through the screen matrix (`+0x1b0`) to 12.4 fixed point;
    /// the height is `(y0 - y1) >> 4` times 1.6, and the rectangle origin is `x >> 4` of each point minus half the
    /// height (first) and a quarter (second, a division by two applied twice), minus 0x700 and 0x730.
    pub fn player_box(self, ram: &mut Ram, host: &mut impl Host) {
        let t = self.0;
        if ram.r8(FLAG_PLAYER_BOX) == 0 {
            return;
        }
        let Some(p) = host.player_position(ram) else {
            ram.w32(t + off::PLAYER_DEPTH, 0);
            return;
        };
        let view = frustum::transform(&ram.m4(t + off::VIEW_MATRIX), p);
        ram.wf(t + off::PLAYER_DEPTH, view[2]);
        let screen = ram.m4(t + off::CLIP_SCREEN);
        let first = project_fixed(&screen, p);
        let lowered = [p[0], frustum::vu(p[1] - 200.0), p[2], p[3]];
        let second = project_fixed(&screen, lowered);
        let h = frustum::vu((first[1].wrapping_sub(second[1]) >> 4) as f32 * 1.6) as i32;
        ram.w32(t + off::PLAYER_RECT + 8, h as u32);
        let half = h.wrapping_add(((h as u32) >> 31) as i32) >> 1;
        ram.w32(t + off::PLAYER_RECT, ((first[0] >> 4).wrapping_sub(half).wrapping_sub(0x700)) as u32);
        let sign = (h.wrapping_add(((h as u32) >> 31) as i32) as u32) >> 31;
        let quarter = half.wrapping_add(sign as i32) >> 1;
        ram.w32(t + off::PLAYER_RECT + 4, ((second[0] >> 4).wrapping_sub(quarter).wrapping_sub(0x730)) as u32);
    }

    /// orig 0x144168: the map's draw task, with the global bytes and words read from `ram`. Nothing while the freeze
    /// byte `0x35594e` is 1. Otherwise, in order: the frame matrices, the visible-mark clear, the occluder volumes, the
    /// player box, the cull walk from node 0 into the list at `list` (the caller's 2048-entry array; the first entry is
    /// 0xffff before the walk and the entry after the last one is 0xffff), then, unless the map is switched off
    /// (`0x35594d`), the map model's draw with the list and the matrix `projection * camera`, and the object pass.
    pub fn map_pass(self, ram: &mut Ram, list: u32, host: &mut impl Host) {
        if ram.r8(FLAG_FREEZE) == 1 {
            return;
        }
        self.frame_matrices(ram);
        self.clear_visible(ram);
        self.build_occluders(ram, host);
        self.player_box(ram, host);
        ram.w16(list, 0xffff);
        let mut count = 0i32;
        self.set_list(ram, list);
        let instance = ram.r32(MODEL_WORD);
        let model = ram.r32(instance);
        self.set_model(ram, model);
        self.walk(ram, 0, &mut count);
        ram.w16(list.wrapping_add((count as u32).wrapping_mul(2)), 0xffff);
        if ram.r8(FLAG_MAP_OFF) == 0 {
            let index = ram.r32(draw::VIEW_INDEX_WORD);
            let select = ram.r8(draw::VIEW_SELECT_BYTE);
            let cam = draw::camera_address(1, select, index);
            let proj = draw::projection_address(1, select, index);
            let p = ram.m4(proj);
            let view: M4 = std::array::from_fn(|k| ram.v4(cam + 0x10 + 16 * k as u32));
            let m = frustum::matrix_product(&p, &view);
            host.draw_map_model(ram, model, &m, instance + 0x10, instance + 0x50, list);
        }
        host.object_pass(ram, self.0);
    }
}

/// The twelve-four fixed point screen position of a point (`0x2fbc10` with a zero flag): the point through the
/// matrix, the first three lanes times `1 / w` (the VU divide), then x and y times 16 truncated to integers. Only x and
/// y are returned.
fn project_fixed(m: &M4, v: V4) -> [i32; 2] {
    let q = frustum::transform(m, v);
    let inv = frustum::vu_divide(1.0, q[3]);
    [(frustum::vu(q[0] * inv) * 16.0) as i32, (frustum::vu(q[1] * inv) * 16.0) as i32]
}

/// The eight corners of a box (0x144a50, ported in `geom`).
fn box_corners(b: &FBox) -> [V4; 8] {
    crate::geom::fbox_corners8(b)
}

/// Does box `a` contain box `b` (0x144ca8, ported in `geom`)?
fn contains(a: &FBox, b: &FBox) -> bool {
    crate::geom::fbox_contains(a, b)
}

/// orig 0x146df0: decides whether an object still has to be culled by its box. An object without an alpha byte, or
/// with a negative one (as a signed byte), or without bit 0x10 has bit 0x800 set and answers 0; otherwise bit 0x800
/// is cleared and the answer is 1.
pub fn cull_state(ram: &mut Ram, obj: u32) -> u32 {
    let alpha = ram.r8(obj + 0xeb);
    let flags = ram.r32(obj + 0xf8);
    let (flags, r) = if alpha != 0 && (alpha as i8) >= 0 && flags & 0x10 != 0 { (flags & !0x800, 1) } else { (flags | 0x800, 0) };
    ram.w32(obj + 0xf8, flags);
    r
}

/// The global tree word.
fn global_tree(ram: &Ram) -> DrawTree {
    DrawTree(ram.r32(TREE_WORD))
}

/// orig 0x143240: adds an object to the global tree ([`DrawTree::add_object`]).
pub fn tree_add_object(ram: &mut Ram, obj: u32) {
    global_tree(ram).add_object(ram, obj);
}

/// orig 0x143260: searches the node of an object from the root and stores node and depth in it. [`DrawTree::find_node`]
/// from node 0 at depth 0 with a best depth of 0.
pub fn tree_find_node(ram: &mut Ram, obj: u32) {
    let t = global_tree(ram);
    let mut best = 0;
    let node = t.find_node(ram, 0, obj, 0, &mut best);
    ram.w16(obj + 0x118, node as u16);
    ram.w16(obj + 0x11a, best as u16);
}

/// orig 0x1432b0: moves an object in the global tree ([`DrawTree::move_object`] with `top` 1).
pub fn tree_move_object(ram: &mut Ram, obj: u32) {
    global_tree(ram).move_object(ram, obj, 1);
}

/// orig 0x1432d8: stores a sky in the global tree ([`DrawTree::set_sky`]).
pub fn tree_set_sky(ram: &mut Ram, obj: u32, which: i64) {
    global_tree(ram).set_sky(ram, obj, which);
}

/// orig 0x143300: the per-frame list clear task: the tree's [`DrawTree::clear_object_lists`], then the map model's base
/// pose, and unless the model instance's switch byte (`+0x8c`) is set the model's texture animation by the field
/// step (`0x356f40`).
pub fn clear_object_lists_task(ram: &mut Ram, host: &mut impl Host) {
    global_tree(ram).clear_object_lists(ram);
    let instance = ram.r32(MODEL_WORD);
    let model = ram.r32(instance);
    host.model_pose_reset(ram, model);
    if ram.r8(instance + 0x8c) != 0 {
        return;
    }
    let step = ram.r32(FRAME_STEP_WORD);
    host.model_texture_update(ram, step, model);
}

/// The script opcode of 0x142f78 and 0x143028 (both halves): hides (`on` false) or shows group `number` in the
/// drawn tree of the global word and in the collision objects (the combined effect of one script opcode). The
/// collision half is [`MapCollision::group_off`] and [`MapCollision::group_on`].
pub fn switch_group(ram: &mut Ram, coll: &mut MapCollision, number: u8, on: bool) {
    let t = global_tree(ram);
    if on {
        t.group_show(ram, number);
        coll.group_on(number);
    } else {
        t.group_hide(ram, number);
        coll.group_off(number);
    }
}
