//! The player's heading rules, ported from the original's controller (ladder step L2, WP-L2-P1; the reading is in
//! `docs/systems/player-camera/controller.md`). Everything works on the original's layouts: a vector is four floats, the view
//! matrix is the sixteen floats at +0x10 of the view block, and the actor's fields are named by their offsets.
//!
//! The chain: the camera builds the view matrix from its smoothed position and look-at point ([`view_matrix`]); the stick is turned
//! through that matrix into the actor's move vector at actor +0x10 ([`stick_through_view`], [`stick_vector`]); the wanted yaw at
//! actor +0x66c is `atan2(x, z)` of that vector while the stick may move the actor ([`wanted_yaw`], [`stick_gate`],
//! [`update_wanted_yaw`]); the facing then turns toward it each object tick by turn speed times the frame step
//! ([`kh2_game_core::motor::turn_angle`], [`rot_to_dir`]).
//!
//! Not here: the view matrix's translation row and the roll (`FUN_0011b520`), the special stick readers `FUN_001775e8`,
//! `FUN_001775a0`, `FUN_00177710` (only the choice between them is, [`stick_reader`]), and the object tick around the turn
//! (`FUN_0016a2f0`), which is partly ported: [`Facing`] is its turn step on its own and carries no ledger line.

pub use kh2_game_core::motor::{fix_radian, turn_angle};

/// Address of the walk limit (0.8): a stick magnitude below it walks, from it up runs (`FUN_001774d8` against `0x37a4bc`).
pub const WALK_LIMIT: f32 = 0.8;

/// orig 0x106f78 (the rotation part). The view block's matrix at +0x10 as sixteen floats in memory order, built from the eye
/// (block +0x50), the look-at point (+0x60) and the up vector (+0x70): forward = normalise(look - eye), side = normalise(forward x
/// up), up' = forward x side; row r holds (side, up', forward) component r. The fourth word of each row and the translation row are
/// not computed (the translation is not read by the controller, and the words next to the rows are left over from the VU
/// shuffle); they stay 0. A zero length divides by zero on the VU, which gives the largest finite number: the product with the
/// zero vector is zero, so a zero axis stays zero here.
pub fn view_matrix(eye: [f32; 3], look: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let fwd = normalise([look[0] - eye[0], look[1] - eye[1], look[2] - eye[2]]);
    let side = normalise(cross(fwd, up));
    let up2 = cross(fwd, side);
    let mut m = [0.0; 16];
    for r in 0..3 {
        m[r * 4] = side[r];
        m[r * 4 + 1] = up2[r];
        m[r * 4 + 2] = fwd[r];
    }
    m
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalise(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len == 0.0 { [0.0; 3] } else { [v[0] / len, v[1] / len, v[2] / len] }
}

/// orig 0x177360. The stick record (x right, y down, unused, magnitude) through the view matrix: the vector (x, 0, -y) is combined
/// with the matrix rows (the transpose product), the height is cleared, the horizontal part is normalised and the magnitude goes
/// into the fourth word. A stick without magnitude gives the zero vector.
pub fn stick_through_view(view: &[f32; 16], stick: [f32; 4]) -> [f32; 4] {
    if stick[3] <= 0.0 {
        return [0.0; 4];
    }
    let v = [stick[0], 0.0, -stick[1]];
    let row = |r: usize| view[r * 4] * v[0] + view[r * 4 + 1] * v[1] + view[r * 4 + 2] * v[2];
    let h = normalise([row(0), 0.0, row(2)]);
    [h[0], 0.0, h[2], stick[3]]
}

/// orig 0x1774d8. The actor's move vector (actor +0x10) from the stick: with a stick magnitude above 0 the vector goes through the
/// view ([`stick_through_view`]) and its fourth word becomes the walk speed (actor +0x110) below [`WALK_LIMIT`] and the run speed
/// (+0x114) from it; without a stick the vector points along the wanted yaw (actor +0x66c) with no speed.
pub fn stick_vector(stick: [f32; 4], view: &[f32; 16], wanted_yaw: f32, walk: f32, run: f32) -> [f32; 4] {
    if stick[3] > 0.0 {
        let mut d = stick_through_view(view, stick);
        d[3] = if d[3] < WALK_LIMIT { walk } else { run };
        d
    } else {
        [wanted_yaw.sin(), 0.0, wanted_yaw.cos(), 0.0]
    }
}

/// Which reader `FUN_001772f8` calls for the current mover (the mover's flag word at mover +8 and id at mover +4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StickReader {
    /// `FUN_001775e8` (mover flag 8).
    Flag8,
    /// `FUN_001775a0` (mover id 2).
    Id2,
    /// `FUN_00177710` (mover ids 0x18 and 0x35).
    Id18Or35,
    /// `FUN_001774d8` ([`stick_vector`]): every other mover, the ground mover among them.
    View,
}

/// orig 0x1772f8 (the choice; the readers other than [`stick_vector`] are not ported).
pub fn stick_reader(mover_flags: u32, mover_id: i32) -> StickReader {
    if mover_flags & 8 != 0 {
        StickReader::Flag8
    } else if mover_id == 2 {
        StickReader::Id2
    } else if mover_id == 0x18 || mover_id == 0x35 {
        StickReader::Id18Or35
    } else {
        StickReader::View
    }
}

/// orig 0x174c28. Whether the stick may turn the actor: the move vector moves (fourth word above 0, x or z not zero), the actor
/// flags (actor +0x168) have bit 4 clear or bit 8 set, the mover's flag word has bit 0x100 (the ground mover's word is 0x180) and
/// the flag word at actor +0x160 has bit 31 clear. With bit 4 set and bit 8 clear the
/// original yields true without looking at the mover's word or +0x160 (the oracle test over the flag words confirms the reading).
pub fn stick_gate(dir: [f32; 4], flags_168: u32, mover_flags: u32, flags_160: u32) -> bool {
    let moving = dir[3] > 0.0 && (dir[0] != 0.0 || dir[2] != 0.0);
    if !moving {
        return false;
    }
    if flags_168 & 4 == 0 || flags_168 & 8 != 0 { mover_flags & 0x100 != 0 && (flags_160 as i32) >= 0 } else { true }
}

/// orig 0x179108. The stick may move the actor: [`stick_gate`] and the camera is not in the look-around (`FUN_00140f98`: the
/// camera object exists and its mode is 1).
pub fn stick_may_move(gate: bool, look_around: bool) -> bool {
    gate && !look_around
}

/// orig 0x16a948. The wanted yaw (actor +0x66c): `atan2(x, z)` of the move vector.
pub fn wanted_yaw(dir: [f32; 4]) -> f32 {
    dir[0].atan2(dir[2])
}

/// orig 0x174bd8 (without the call at its end, `FUN_00197118`, which is not ported). The wanted yaw after the tick's stick
/// reading: replaced by the move vector's heading when the actor's slot +0x84 ([`stick_may_move`]) allows it, else kept.
pub fn update_wanted_yaw(current: f32, dir: [f32; 4], may_move: bool) -> f32 {
    if may_move { wanted_yaw(dir) } else { current }
}

/// orig 0x17be98. An angle as the object's direction record (actor +0x550): (sin, 0, cos, wrapped angle).
pub fn rot_to_dir(angle: f32) -> [f32; 4] {
    let a = fix_radian(angle);
    [a.sin(), 0.0, a.cos(), a]
}

/// orig 0x16a790 (the call of `FUN_0019ad98` ahead of the write only reads a flag and is not ported). The write of the facing into
/// the direction record, [`rot_to_dir`].
pub fn set_rot(angle: f32) -> [f32; 4] {
    rot_to_dir(angle)
}

/// The facing turn of the object tick (the part of `FUN_0016a2f0` between the mover's tick and the path hook; not a whole-function
/// port, so no ledger line): unless free rotation holds, the facing (actor +0x55c) turns toward the wanted yaw (+0x66c) by at most
/// the turn speed (+0x11c) times the frame step and is written back by [`set_rot`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facing {
    pub angle: f32,
    pub dir: [f32; 4],
}

impl Facing {
    pub fn turn(self, wanted: f32, turn_speed: f32, step: f32) -> Facing {
        let angle = turn_angle(self.angle, wanted, turn_speed * step);
        Facing { angle, dir: set_rot(angle) }
    }
}
