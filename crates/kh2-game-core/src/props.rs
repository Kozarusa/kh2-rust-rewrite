//! The map's props (WP-L1-P25c, spec `docs/systems/core-runtime/render-model.md` c6): create and set up a prop, the task
//! that runs it, the per-frame add or move in the draw tree, the colour scale, the motion holder and the texture
//! players of the sky objects. Nothing here is wired into the app yet: the area loader that reads the map file's prop
//! records and the frame order that runs the task are other packages.
//!
//! A prop is the 0x2c0-byte object of the original, kept byte for byte in a [`Ram`] (the sparse model of the game's
//! address space of `kh2_map::draw_tree`, so that the draw tree functions it calls see the same memory) at the offsets of
//! [`off`]. A task manager block is not in that memory but in [`Tasks`]; a task's argument word (`+4`) holds the prop.
//! What the original reaches through other units is a [`PropHost`] call: the prop's two vtables and its owner's
//! (the model data object), the draw object constructor, the matrix rotate, the footstep floor query, the model
//! instance calls and the motion objects' own methods. [`NoHost`] does none of them.
//!
//! The oracle tests are in `tests/orig_l1_p25c.rs`; the pure arithmetic (colour, clock, matrices, boxes) is in
//! `kh2_model::draw::props`.

use crate::task::Tasks;
use kh2_map::draw_tree::{Ram, tree_add_object, tree_find_node, tree_move_object, tree_set_sky};
use kh2_map::geom::{M4, V4, fbox_centre, fbox_corners, fbox_corners8, fbox_corners8_negated};
use kh2_model::draw::props as pure;

/// Prop block offsets. The prop contains the motion holder at `+0x110` and the draw object at `+0x1c0`.
pub mod off {
    /// The vtable word.
    pub const VTABLE: u32 = 0x00;
    /// The owner: the model data object the prop was made from.
    pub const OWNER: u32 = 0x04;
    /// The prop's matrix (4 rows of 4 floats).
    pub const MATRIX: u32 = 0x10;
    /// Position vector.
    pub const POSITION: u32 = 0x50;
    /// Rotation vector (x, y, z angles).
    pub const ROTATION: u32 = 0x60;
    /// Scale vector.
    pub const SCALE: u32 = 0x70;
    /// Link to the next prop in the list headed by word `0x520a38`.
    pub const LINK: u32 = 0x80;
    /// Link to the next prop in the task list headed by word `0x520a40`.
    pub const TASK_LINK: u32 = 0x84;
    /// Eight box corners as vectors.
    pub const CORNERS: u32 = 0x90;
    /// The motion holder (matrix at `+0`, see `kh2_model::draw::props`).
    pub const HOLDER: u32 = 0x110;
    /// The holder's animation box (`holder +0x54`).
    pub const HOLDER_BOX: u32 = 0x164;
    /// Flag word: bit 0 dead, bit 1 moving.
    pub const FLAGS: u32 = 0x180;
    /// Base colour bytes r, g, b, a.
    pub const BASE_COLOUR: u32 = 0x1a4;
    /// The draw object (a model instance handle at `+0`; the draw tree links this address).
    pub const DRAW: u32 = 0x1c0;
    /// The model instance's matrix (draw object `+0x40`).
    pub const POSE_MATRIX: u32 = 0x200;
    /// The scaled colour bytes (draw object `+0xe8`).
    pub const COLOUR: u32 = 0x2a8;
    /// The centre vector (draw object `+0xc0`).
    pub const CENTRE: u32 = 0x280;
    /// The world box (draw object `+0xd0`).
    pub const BOX: u32 = 0x290;
    /// The draw flags (draw object `+0xf8`).
    pub const DRAW_FLAGS: u32 = 0x2b8;
}

/// The vtable of a prop made by `0x15f490` (and `0x15f8e0`).
pub const VTABLE_PROP: u32 = 0x35a850;
/// The vtable of a sky prop made by `0x15f3a0`.
pub const VTABLE_SKY: u32 = 0x35a820;
/// The word handed to the owner's instance calls (`0x35a838`).
pub const OWNER_ARG_WORD: u32 = 0x35a838;
/// The word holding the model table base (`0x35a83c`); entry `i` is 12 bytes, with owner `+0x78`, bind value `+0x7c`
/// and motion table `+0x80`.
pub const MODEL_TABLE_WORD: u32 = 0x35a83c;
/// The byte that stops the prop task from running the props (`0x35a840`).
pub const HOLD_BYTE: u32 = 0x35a840;
/// The game flag word whose bit 5 stops the prop task from running the props (`0x35d268`).
pub const FLAG_WORD: u32 = 0x35d268;
/// The field step (a float), the same word `kh2_map::draw_tree::FRAME_STEP_WORD` names.
pub const STEP_WORD: u32 = 0x356f40;
/// The constant that scales rows 1 and 2 of a moving prop's matrix (`0x3794e0`).
pub const ROW_SCALE_WORD: u32 = 0x3794e0;
/// The 16 words of the unit matrix `0x11b460` copies (`0x354a60`).
pub const UNIT_TABLE: u32 = 0x354a60;
/// The head and tail words of the list of props (`0x520a38`, `0x520a3c`), linked by prop `+0x80`.
pub const LIST_HEAD: u32 = 0x520a38;
pub const LIST_TAIL: u32 = 0x520a3c;
/// The head and tail words of the list of props the task has run (`0x520a40`, `0x520a44`), linked by prop `+0x84`.
pub const TASK_LIST_HEAD: u32 = 0x520a40;
pub const TASK_LIST_TAIL: u32 = 0x520a44;
/// The function of a prop's task (`0x1604a8`) and its finalizer (`0x160558`).
pub const TASK_FUNCTION: u32 = 0x1604a8;
pub const TASK_FINALIZER: u32 = 0x160558;
/// The size of a prop block.
pub const PROP_SIZE: u32 = 0x2c0;

/// What the original reaches through other units. Every method has a default that does nothing (and returns 0 or
/// the neutral value).
pub trait PropHost {
    /// The prop's own vtable call: `slot_offset` 0 and 4 are the two methods the task runs (`*(prop)` then `+0` or `+4`).
    fn prop_call(&mut self, _ram: &mut Ram, _prop: u32, _slot_offset: u32) {}
    /// `0x330bd0`: the constructor of the draw object at `at` (prop `+0x1c0`).
    fn construct_draw_object(&mut self, _ram: &mut Ram, _at: u32) {}
    /// Owner vtable `+0x24` with the word `0x35a838`: makes the model instance, returns its handle.
    fn owner_instance(&mut self, _ram: &mut Ram, _owner: u32, _arg: u32) -> u32 {
        0
    }
    /// Owner vtable `+0x28` with the word `0x35a838` and the instance handle: releases the instance.
    fn owner_release(&mut self, _ram: &mut Ram, _owner: u32, _arg: u32, _instance: u32) {}
    /// Owner vtable `+0xc`: the bounding radius (a float result).
    fn owner_radius(&mut self, _ram: &mut Ram, _owner: u32) -> f32 {
        0.0
    }
    /// Owner vtable `+0x10`: the model's bounds, two vectors (minimum and maximum).
    fn owner_bounds(&mut self, _ram: &mut Ram, _owner: u32) -> (V4, V4) {
        ([0.0; 4], [0.0; 4])
    }
    /// Owner vtable `+0x38`: a 64-bit answer; the set-up looks at every bit but bit 0.
    fn owner_flags(&mut self, _ram: &mut Ram, _owner: u32) -> u64 {
        0
    }
    /// `0x11e018(instance, value)`: binds the model data to the instance.
    fn bind_instance(&mut self, _ram: &mut Ram, _instance: u32, _value: u32) {}
    /// `0x11e8c0(instance, a1)`.
    fn call_11e8c0(&mut self, _ram: &mut Ram, _instance: u32, _a1: u32) {}
    /// `0x11e638(instance, a1, a2, a3)`.
    fn call_11e638(&mut self, _ram: &mut Ram, _instance: u32, _a1: u32, _a2: u32, _a3: u32) {}
    /// `0x11e760(instance, a1, 0, 0)`.
    fn call_11e760(&mut self, _ram: &mut Ram, _instance: u32, _a1: u64) {}
    /// `0x11e7f8(step, instance)`: the instance's texture animation by `step`.
    fn texture_update(&mut self, _ram: &mut Ram, _step: f32, _instance: u32) {}
    /// `0x11b568(matrix, rotation)`: rotates the matrix at `matrix` in place by the three angles at `rotation`; returns
    /// the address of the result (the matrix itself in the original).
    fn rotate_xyz(&mut self, _ram: &mut Ram, matrix: u32, _rotation: u32) -> u32 {
        matrix
    }
    /// `0x143438(point, record)`: the footstep floor facts under the point at `point` (prop `+0x50`); `record` is the
    /// 0x40-byte record the caller prepared. The prop reads the colour bytes `+0x20..+0x22` of it afterwards.
    fn floor_facts(&mut self, _ram: &mut Ram, _point: u32, _record: &mut [u8; 0x40]) {}
    /// `0x1684c0(table, kind, index)`: the motion of a kind and index in a motion table, 0 for none.
    fn motion_lookup(&mut self, _ram: &mut Ram, _table: u32, _kind: u32, _index: i64) -> u32 {
        0
    }
    /// Motion vtable `+0x8`: the motion's length in frames.
    fn motion_length(&mut self, _ram: &mut Ram, _motion: u32) -> i32 {
        0
    }
    /// Motion vtable `+0x14`: the motion's box, two vectors.
    fn motion_bounds(&mut self, _ram: &mut Ram, _motion: u32) -> (V4, V4) {
        ([0.0; 4], [0.0; 4])
    }
    /// Motion vtable `+0x18(motion, out)` with the time in `f12`: `Some(matrix)` when the call returns 0 (the matrix is
    /// written), `None` for any other result.
    fn motion_matrix(&mut self, _ram: &mut Ram, _motion: u32, _time: f32) -> Option<M4> {
        None
    }
    /// `0x11c2a0(instance, matrix, motion, 0, 0, 0)` with the time in `f12`: poses the instance by the motion.
    fn pose_motion(&mut self, _ram: &mut Ram, _instance: u32, _matrix: u32, _motion: u32, _time: f32) {}
    /// `0x11c268(instance, matrix, 0)`: poses the instance in its base pose.
    fn pose_base(&mut self, _ram: &mut Ram, _instance: u32, _matrix: u32) {}
    /// The player's position vector (`0x16aa08` then `0x16b960`), `None` when there is no player.
    fn player_position(&mut self, _ram: &mut Ram) -> Option<V4> {
        None
    }
    /// `0x102498(task)`: what the task does for a dead prop (a log print).
    fn dead_prop(&mut self, _ram: &mut Ram, _task: u32) {}
    /// The model allocator's vtable `+4` with the prop: frees the prop block (`0x15fe50`).
    fn free_prop(&mut self, _ram: &mut Ram, _allocator: u32, _prop: u32) {}
}

/// A host that does nothing.
pub struct NoHost;

impl PropHost for NoHost {}

fn copy_bytes(ram: &mut Ram, dst: u32, src: u32, len: u32) {
    let b = ram.read(src, len);
    ram.write(dst, &b);
}

fn set_flags(ram: &mut Ram, a: u32, bits: u32) {
    let v = ram.r32(a);
    ram.w32(a, v | bits);
}

/// The unit matrix routine `0x11b460` (outside this package): copies the 16 words at [`UNIT_TABLE`] to `dst`.
fn unit_matrix(ram: &mut Ram, dst: u32) {
    for k in 0..16 {
        let v = ram.r32(UNIT_TABLE + 4 * k);
        ram.w32(dst + 4 * k, v);
    }
}

/// The box store of `0x1446f8`: the two corner vectors' x, y and z, ordered per axis, written as six floats.
fn store_box(ram: &mut Ram, at: u32, a: V4, b: V4) {
    let bx = fbox_corners([a[0], a[1], a[2]], [b[0], b[1], b[2]]);
    for (k, f) in bx.iter().enumerate() {
        ram.wf(at + 4 * k as u32, *f);
    }
}

/// orig 0x15f488: an empty function (a virtual slot of the prop's vtable that does nothing).
pub fn empty_15f488() {}

/// orig 0x15ffb8: an empty function (another virtual slot).
pub fn empty_15ffb8() {}

/// orig 0x160ff8: an empty function (the motion holder's destructor).
pub fn empty_160ff8() {}

/// orig 0x1611a0: the motion holder's matrix set to the unit matrix.
pub fn holder_unit(ram: &mut Ram, holder: u32) {
    unit_matrix(ram, holder);
}

/// orig 0x160fb0: binds a motion holder to its owner: owner `+0x44`, the motion table, motion, time and length cleared,
/// the animation box emptied (its six floats), the matrix set to unit.
pub fn bind_holder(ram: &mut Ram, holder: u32, owner: u32) {
    ram.w32(holder + 0x44, owner);
    ram.w32(holder + 0x40, 0);
    ram.w32(holder + 0x48, 0);
    ram.w32(holder + 0x4c, 0);
    ram.w32(holder + 0x50, 0);
    for k in 0..6 {
        ram.w32(holder + 0x54 + 4 * k, 0);
    }
    holder_unit(ram, holder);
}

/// orig 0x161000: the motion time of the holder set to 0.
pub fn reset_time(ram: &mut Ram, holder: u32) {
    ram.w32(holder + 0x4c, 0);
}

/// orig 0x15ffa0: a prop's motion time set to 0 (its holder is at `+0x110`).
pub fn reset_prop_time(ram: &mut Ram, prop: u32) {
    reset_time(ram, prop + off::HOLDER);
}

/// orig 0x161008: the holder's motion table word set.
pub fn set_motion_table(ram: &mut Ram, holder: u32, table: u32) {
    ram.w32(holder + 0x40, table);
}

/// orig 0x161010: the motion `index` of the holder's table: 0 for a negative index or no table, else what the table
/// lookup (`0x1684c0` with kind 9) names, read at its `+8`, 0 when the lookup finds nothing.
pub fn motion_lookup(ram: &mut Ram, host: &mut impl PropHost, holder: u32, index: i64) -> u32 {
    if index < 0 {
        return 0;
    }
    let table = ram.r32(holder + 0x40);
    if table == 0 {
        return 0;
    }
    let found = host.motion_lookup(ram, table, 9, index);
    if found == 0 { 0 } else { ram.r32(found + 8) }
}

/// orig 0x1611b8: the holder's matrix posed at `time`: with a motion, the motion's matrix call fills the holder's
/// matrix when it answers 0, else the unit matrix is stored. Without a motion nothing happens.
pub fn motion_pose(ram: &mut Ram, host: &mut impl PropHost, holder: u32, time: f32) {
    let motion = ram.r32(holder + 0x48);
    if motion == 0 {
        return;
    }
    match host.motion_matrix(ram, motion, time) {
        Some(m) => ram.put_m4(holder, &m),
        None => holder_unit(ram, holder),
    }
}

/// orig 0x161238: the holder's animation box (`+0x54`) from the motion's own box call.
pub fn motion_box(ram: &mut Ram, host: &mut impl PropHost, holder: u32) {
    let motion = ram.r32(holder + 0x48);
    let (a, b) = host.motion_bounds(ram, motion);
    store_box(ram, holder + 0x54, a, b);
}

/// orig 0x1610b0: starts the motion `index` on the holder at `time`: the motion word is stored (0 when there is none,
/// and nothing else happens), then the time, the length (the motion's own count as a float), the box and the unit matrix.
pub fn motion_start(ram: &mut Ram, host: &mut impl PropHost, time: f32, holder: u32, index: i64) {
    let motion = motion_lookup(ram, host, holder, index);
    ram.w32(holder + 0x48, motion);
    if motion == 0 {
        return;
    }
    ram.wf(holder + 0x4c, time);
    let len = host.motion_length(ram, motion);
    ram.wf(holder + 0x50, len as f32);
    motion_box(ram, host, holder);
    holder_unit(ram, holder);
}

/// 0x161130 on a holder in memory: the clock step of `kh2_model::draw::props::motion_step` (orig 0x161130 is there), with
/// the pose call `0x1611b8` ([`motion_pose`]) for each sampled time.
pub fn motion_step(ram: &mut Ram, host: &mut impl PropHost, holder: u32, dt: f32) {
    let mut c = pure::Clock {
        motion: ram.r32(holder + 0x48),
        time: ram.rf(holder + 0x4c),
        length: ram.rf(holder + 0x50),
    };
    if c.motion == 0 {
        return;
    }
    // The original stores the new time before it samples; the sampling call does not read it, so the order against
    // the host calls is not observable, but the stored words are written at the same points here.
    c.time += dt;
    ram.wf(holder + 0x4c, c.time);
    motion_pose(ram, host, holder, c.time);
    if c.length < c.time {
        c.time -= c.length;
        ram.wf(holder + 0x4c, c.time);
        motion_pose(ram, host, holder, c.length);
    }
}

/// orig 0x161060: the holder's instance posed for its current motion: with a motion, the motion pose at the stored time,
/// else the base pose; the matrix is the owner's pose matrix (owner `+0x200`).
pub fn motion_stop(ram: &mut Ram, host: &mut impl PropHost, holder: u32) {
    let owner = ram.r32(holder + 0x44);
    let instance = ram.r32(owner + off::DRAW);
    let matrix = owner + off::POSE_MATRIX;
    let motion = ram.r32(holder + 0x48);
    if motion != 0 {
        let time = ram.rf(holder + 0x4c);
        host.pose_motion(ram, instance, matrix, motion, time);
    } else {
        host.pose_base(ram, instance, matrix);
    }
}

/// The scaled colour of the prop in memory: [`pure::colour_scale`] (orig 0x15ff50 is there) on the six bytes involved.
pub fn colour_scale(ram: &mut Ram, prop: u32, r: u32, g: u32, b: u32) {
    let base = |o: u32| ram.r8(prop + off::BASE_COLOUR + o);
    let (br, bg, bb) = (base(0), base(1), base(2));
    ram.w8(prop + off::COLOUR + 2, pure::colour_channel(bb, b));
    ram.w8(prop + off::COLOUR + 1, pure::colour_channel(bg, g));
    ram.w8(prop + off::COLOUR, pure::colour_channel(br, r));
}

/// orig 0x15ffc0: the prop's matrix (`+0x10`): the unit matrix, rotated by the angles at `+0x60` (the host call), the
/// position at `+0x50` added to the translation row and rows 0 to 2 scaled by the scale vector at `+0x70`
/// ([`pure::place_matrix`]).
pub fn prop_matrix(ram: &mut Ram, host: &mut impl PropHost, prop: u32) {
    let m = prop + off::MATRIX;
    unit_matrix(ram, m);
    let result = host.rotate_xyz(ram, m, prop + off::ROTATION);
    if result != m {
        copy_bytes(ram, m, result, 64);
    }
    let rotated = ram.m4(m);
    let out = pure::place_matrix(&rotated, ram.v4(prop + off::POSITION), ram.v4(prop + off::SCALE));
    ram.put_m4(m, &out);
}

/// orig 0x160068: the prop's eight box corners (`+0x90`) carried through `matrix`, written back, and the box of the
/// results stored as the world box (`+0x290`, six floats).
pub fn corners_box(ram: &mut Ram, prop: u32, matrix: &M4) {
    let mut corners = [[0.0f32; 4]; 8];
    for (i, c) in corners.iter_mut().enumerate() {
        *c = ram.v4(prop + off::CORNERS + 16 * i as u32);
    }
    let (out, b) = pure::transform_corners(&corners, matrix);
    for (i, c) in out.iter().enumerate() {
        ram.put_v4(prop + off::CORNERS + 16 * i as u32, *c);
    }
    for (k, f) in b.iter().enumerate() {
        ram.wf(prop + off::BOX + 4 * k as u32, *f);
    }
}

/// orig 0x1601a8: the world box of a moving prop. The prop's matrix with rows 1 and 2 scaled by the constant at
/// `0x3794e0`, then each row of the holder's matrix carried through it; row 3 is the new centre (`+0x280`). The holder's
/// animation box, widened by the owner's radius, gives eight corners (`+0x90`), which [`corners_box`] carries through the
/// matrix.
pub fn world_box(ram: &mut Ram, host: &mut impl PropHost, prop: u32) {
    let object = ram.m4(prop + off::MATRIX);
    let holder = ram.m4(prop + off::HOLDER);
    let m = pure::world_matrix(&object, &holder, ram.rf(ROW_SCALE_WORD));
    ram.put_v4(prop + off::CENTRE, m[3]);
    let owner = ram.r32(prop + off::OWNER);
    let radius = host.owner_radius(ram, owner);
    let b = pure::inflated_box(&ram.fbox(prop + off::HOLDER_BOX), radius);
    for (i, c) in fbox_corners8(&b).iter().enumerate() {
        ram.put_v4(prop + off::CORNERS + 16 * i as u32, *c);
    }
    corners_box(ram, prop, &m);
}

/// orig 0x160388: the prop's centre (`+0x280`) from its world box, and when there is a player whose height lies inside
/// the box's height range, the centre's height is the player's.
pub fn prop_centre(ram: &mut Ram, host: &mut impl PropHost, prop: u32) {
    let c = fbox_centre(&ram.fbox(prop + off::BOX));
    ram.put_v4(prop + off::CENTRE, c);
    if let Some(p) = host.player_position(ram) {
        let y = p[1];
        if ram.rf(prop + off::BOX + 4) <= y && y <= ram.rf(prop + off::BOX + 16) {
            ram.wf(prop + off::CENTRE + 4, y);
        }
    }
}

/// orig 0x160428: the per-frame update. A static prop (flag bit 1 clear) takes its centre when its draw flag bit 0 is
/// set, then is added to the draw tree; a moving prop gets its world box, its centre when flagged, and is moved in the
/// tree.
pub fn prop_frame(ram: &mut Ram, host: &mut impl PropHost, prop: u32) {
    let moving = ram.r32(prop + off::FLAGS) & 2 != 0;
    if moving {
        world_box(ram, host, prop);
    }
    if ram.r32(prop + off::DRAW_FLAGS) & 1 != 0 {
        prop_centre(ram, host, prop);
    }
    if moving {
        tree_move_object(ram, prop + off::DRAW);
    } else {
        tree_add_object(ram, prop + off::DRAW);
    }
}

/// orig 0x160408: the prop's instance call `0x11e760` with the caller's argument and two zeros.
pub fn prop_instance_call(ram: &mut Ram, host: &mut impl PropHost, prop: u32, a1: u64) {
    let instance = ram.r32(prop + off::DRAW);
    host.call_11e760(ram, instance, a1);
}

/// orig 0x15fef8: the texture player of a camera-following (sky) prop: the holder posed for its motion
/// ([`motion_stop`]), stepped by the field step, then the instance's texture animation by the same step.
pub fn sky_player(ram: &mut Ram, host: &mut impl PropHost, prop: u32) {
    let holder = prop + off::HOLDER;
    motion_stop(ram, host, holder);
    let step = ram.rf(STEP_WORD);
    motion_step(ram, host, holder, step);
    let step = ram.rf(STEP_WORD);
    let instance = ram.r32(prop + off::DRAW);
    host.texture_update(ram, step, instance);
}

/// orig 0x160588: makes the prop's task on the manager: bits 0, the given priority, function `0x1604a8`; the task's
/// argument word is the prop and its finalizer `0x160558`. Returns the task handle.
pub fn create_prop_task(tasks: &mut Tasks, manager: u32, prop: u32, priority: i32) -> u32 {
    let t = tasks.create_task(manager, 0, priority, TASK_FUNCTION);
    let task = tasks.task_mut(t).expect("the task just made");
    task.set_arg(prop);
    task.set_finalizer(TASK_FINALIZER);
    t
}

/// orig 0x1604a8: the prop task. The prop is the task's argument. A dead prop (flag bit 0) only reports. Otherwise, unless
/// the game flag word bit 5 is set or the hold byte is not 0, the prop's two virtual methods run (offset 0, then 4); then
/// the prop is linked at the tail of the task list.
pub fn prop_task(ram: &mut Ram, host: &mut impl PropHost, tasks: &Tasks, task: u32) {
    let prop = tasks.task(task).map_or(0, |t| t.arg());
    if ram.r32(prop + off::FLAGS) & 1 != 0 {
        host.dead_prop(ram, task);
        return;
    }
    if ram.r32(FLAG_WORD) & 0x20 == 0 && ram.r8(HOLD_BYTE) == 0 {
        host.prop_call(ram, prop, 0);
        host.prop_call(ram, prop, 4);
    }
    let tail = ram.r32(TASK_LIST_TAIL);
    if tail != 0 {
        ram.w32(tail + off::TASK_LINK, prop);
        ram.w32(prop + off::TASK_LINK, 0);
    } else {
        ram.w32(TASK_LIST_HEAD, prop);
        ram.w32(prop + off::TASK_LINK, 0);
    }
    ram.w32(TASK_LIST_TAIL, prop);
}

/// orig 0x15fdc0: the prop's destructor body: the vtable word, the owner releases the instance, the holder's (empty)
/// destructor.
pub fn prop_drop(ram: &mut Ram, host: &mut impl PropHost, prop: u32) {
    ram.w32(prop + off::VTABLE, VTABLE_PROP);
    let owner = ram.r32(prop + off::OWNER);
    let instance = ram.r32(prop + off::DRAW);
    let arg = ram.r32(OWNER_ARG_WORD);
    host.owner_release(ram, owner, arg, instance);
    empty_160ff8();
}

/// orig 0x15fe50: the deleting destructor: [`prop_drop`], then the model allocator frees the prop.
pub fn prop_delete(ram: &mut Ram, host: &mut impl PropHost, prop: u32) {
    prop_drop(ram, host, prop);
    let allocator = ram.r32(OWNER_ARG_WORD);
    host.free_prop(ram, allocator, prop);
}

/// The arguments of the prop constructor `0x15f490` beyond the prop and the model index.
#[derive(Clone, Copy, Debug, Default)]
pub struct InitArgs {
    /// The second argument (`a1`): stored in the draw object (`+0x2d4`) when the owner asks for it.
    pub tint: u32,
    /// Position, rotation and scale vectors (copied to `+0x50`, `+0x60`, `+0x70`).
    pub position: V4,
    pub rotation: V4,
    pub scale: V4,
    /// The priority of the prop's task.
    pub priority: i32,
    /// The motion index to start, -1 for none.
    pub motion: i32,
    /// The start time of that motion, an integer count: the start time stored is twice its float value.
    pub start: i32,
    /// A word stored at `+0x1a8`.
    pub extra: u32,
    /// Flags (`+0x1ac`): bit 0 take the centre now, bit 1 take the colour from the floor.
    pub flags: u32,
}

/// orig 0x15f490
/// orig 0x15f8e0: the prop constructor (the two entries have the same body). `model` indexes the model table at word
/// `0x35a83c`. Sets up the vtable, vectors, the motion holder, the draw object and its model instance, the matrix
/// ([`prop_matrix`]), the box and the world box ([`corners_box`]), places the draw object in the tree, and sets the draw
/// flags. With flag bit 1 the colour bytes come from the floor under the prop; with a motion the motion is started.
/// The prop is linked at the tail of the prop list and its task is made ([`create_prop_task`], on `manager`).
pub fn prop_init(ram: &mut Ram, host: &mut impl PropHost, tasks: &mut Tasks, manager: u32, prop: u32, model: u32, a: &InitArgs) {
    ram.w32(prop + off::VTABLE, VTABLE_PROP);
    ram.put_v4(prop + off::POSITION, a.position);
    ram.put_v4(prop + off::ROTATION, a.rotation);
    ram.put_v4(prop + off::SCALE, a.scale);
    let holder = prop + off::HOLDER;
    bind_holder(ram, holder, prop);
    ram.w32(prop + off::FLAGS, 0);
    for k in 0..4 {
        ram.w8(prop + off::BASE_COLOUR + k, 0x80);
    }
    ram.w32(prop + 0x1a8, a.extra);
    ram.w32(prop + 0x1b0, 0);
    ram.w32(prop + 0x1ac, a.flags);
    host.construct_draw_object(ram, prop + off::DRAW);
    let table = ram.r32(MODEL_TABLE_WORD);
    let entry = table.wrapping_add(model.wrapping_mul(12));
    let owner = ram.r32(entry.wrapping_add(0x78));
    ram.w32(prop + off::OWNER, owner);
    let arg = ram.r32(OWNER_ARG_WORD);
    let instance = host.owner_instance(ram, owner, arg);
    ram.w32(prop + off::DRAW, instance);
    let bind = ram.r32(entry.wrapping_add(0x7c));
    let instance = ram.r32(prop + off::DRAW);
    host.bind_instance(ram, instance, bind);
    ram.w32(prop + 0x1ec, bind);
    prop_matrix(ram, host, prop);
    copy_bytes(ram, prop + off::POSE_MATRIX, prop + off::MATRIX, 64);
    copy_bytes(ram, prop + off::CENTRE, prop + off::POSITION, 16);
    let owner = ram.r32(prop + off::OWNER);
    let (lo, hi) = host.owner_bounds(ram, owner);
    let bx = fbox_corners([lo[0], lo[1], lo[2]], [hi[0], hi[1], hi[2]]);
    ram.w32(prop + 0x2bc, prop + off::CORNERS);
    for (i, c) in fbox_corners8_negated(&bx).iter().enumerate() {
        ram.put_v4(prop + off::CORNERS + 16 * i as u32, *c);
    }
    let matrix = ram.m4(prop + off::MATRIX);
    corners_box(ram, prop, &matrix);
    tree_find_node(ram, prop + off::DRAW);
    let flags = ram.r32(prop + off::DRAW_FLAGS);
    ram.w32(prop + off::DRAW_FLAGS, (flags & !8) | 0x20);
    let instance = ram.r32(prop + off::DRAW);
    host.call_11e8c0(ram, instance, 0);
    let instance = ram.r32(prop + off::DRAW);
    host.call_11e638(ram, instance, 2, 0, 0);
    if a.flags & 1 != 0 {
        set_flags(ram, prop + off::DRAW_FLAGS, 1);
        prop_centre(ram, host, prop);
    }
    if a.flags & 2 != 0 {
        // The record the floor query fills: the normal (0, 0, 0, 1) and its negation at +0x10, the polygon 0xffff at
        // +0x24, zeros after. The w lane of the second vector is a left-over vector register in the original and the
        // bytes of the matrix copy that lay on the stack stay in the rest; the host must read none of them.
        let mut record = [0u8; 0x40];
        record[12..16].copy_from_slice(&1.0f32.to_bits().to_le_bytes());
        record[0x24..0x28].copy_from_slice(&0xffffu32.to_le_bytes());
        host.floor_facts(ram, prop + off::POSITION, &mut record);
        ram.w8(prop + off::BASE_COLOUR + 3, 0x80);
        ram.w8(prop + off::BASE_COLOUR, record[0x20]);
        ram.w8(prop + off::BASE_COLOUR + 1, record[0x21]);
        ram.w8(prop + off::BASE_COLOUR + 2, record[0x22]);
        colour_scale(ram, prop, 0x80, 0x80, 0x80);
    }
    let s = ram.v4(prop + off::SCALE);
    if (s[0] * s[1]) * s[2] < 0.0 {
        set_flags(ram, prop + off::DRAW + 8, 8);
    }
    if a.motion != -1 {
        let motions = ram.r32(entry.wrapping_add(0x80));
        if motions != 0 {
            set_flags(ram, prop + off::FLAGS, 2);
            set_motion_table(ram, holder, motions);
            let t = a.start as f32;
            motion_start(ram, host, t + t, holder, a.motion as i64);
        }
    }
    let owner = ram.r32(prop + off::OWNER);
    let owner_data = ram.r32(owner + 4);
    if ram.r32(owner_data) == 0xffff_ffff {
        ram.w32(prop + 0x2d4, a.tint);
        set_flags(ram, prop + off::DRAW_FLAGS, 0x80);
    }
    ram.w32(prop + 0x1f4, 0);
    let bits = host.owner_flags(ram, owner);
    if bits & !1 != 0 {
        ram.w32(prop + 0x2d4, a.tint);
        ram.w32(prop + 0x1f4, prop + 0x184);
        set_flags(ram, prop + off::DRAW_FLAGS, 0x100);
        for i in 0..4u32 {
            let at = prop + 8 * i;
            for k in 0..4 {
                ram.w8(at + 0x188 + k, 0x80);
            }
            ram.w32(at + 0x184, 1u32 << (i + 1));
        }
        ram.w32(prop + 0x19c, 0);
    }
    let tail = ram.r32(LIST_TAIL);
    if tail != 0 {
        ram.w32(tail + off::LINK, prop);
        ram.w32(prop + off::LINK, 0);
    } else {
        ram.w32(LIST_HEAD, prop);
        ram.w32(prop + off::LINK, 0);
    }
    ram.w32(LIST_TAIL, prop);
    create_prop_task(tasks, manager, prop, a.priority);
}

/// orig 0x15f3a0: creates a sky prop: [`prop_init`] with tint 0, no motion and no extra words, then the sky vtable; when
/// `sky` is 0 the draw object gets the bytes 0x80, 0x80, 0x80 and 0xff at its `+0x10` and flag bit 1 (`+8`); the sky
/// is stored in the draw tree (`sky` 0 for the first sky, else the second); the prop's word `+0x1f4` is cleared, `+0x1b0`
/// set to 1 and the draw flag bits 7 and 8 cleared.
#[allow(clippy::too_many_arguments)]
pub fn sky_prop_init(ram: &mut Ram, host: &mut impl PropHost, tasks: &mut Tasks, manager: u32, prop: u32, model: u32, position: V4, rotation: V4, scale: V4, priority: i32, sky: i64) {
    let a = InitArgs { tint: 0, position, rotation, scale, priority, motion: -1, start: 0, extra: 0, flags: 0 };
    prop_init(ram, host, tasks, manager, prop, model, &a);
    ram.w32(prop + off::VTABLE, VTABLE_SKY);
    if sky == 0 {
        let d = prop + off::DRAW;
        ram.w8(d + 0x10, 0x80);
        ram.w8(d + 0x13, 0xff);
        ram.w8(d + 0x12, 0x80);
        ram.w8(d + 0x11, 0x80);
        set_flags(ram, d + 8, 2);
    }
    tree_set_sky(ram, prop + off::DRAW, sky);
    ram.w32(prop + 0x1f4, 0);
    ram.w32(prop + 0x1b0, 1);
    let f = ram.r32(prop + off::DRAW_FLAGS);
    ram.w32(prop + off::DRAW_FLAGS, f & !0x80 & !0x100);
}
