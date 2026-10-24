//! The draw unit's view and projection (WP-L1-P25a, spec `docs/systems/core-runtime/render-model.md` c1) and the
//! frustum tests it feeds ([`frustum`]).
//!
//! The game keeps its views in two static arrays: one 0x100-byte projection block per view at `0x3b21f0` (and a second
//! array at `0x3b23f0` that a selector of 0 reads while the byte `0x349e44` is 0) and one camera block of 0x90 bytes at
//! `0x3b25f0` (`0x3b2710` for the same selector), the view being the word `0x349e40`. The blocks are plain byte
//! layouts here, with the original's offsets: the functions below take a block as a byte slice.
//!
//! Projection block: `+0x40` focal length, `+0x44` horizontal field of view, `+0x48` vertical field of view, `+0x4c` and
//! `+0x50` the two scale words, the matrix `+0x80` is the cull matrix. Camera block: the vtable word `+0x00`, the
//! view matrix `+0x10`, position `+0x50`, target `+0x60`, up vector `+0x70`, roll `+0x80`.
//!
//! Field of view of the main view (the one the map draws through): the area set-up (`0x1078f0`, a function outside this
//! package) gives the projection the focal length 512.0 and the scale words 0.8 (`+0x4c`, constant `0x378138`) and
//! 0.8666667 (`+0x50`, constant `0x37813c`). Then `tan(fov_v / 2) / tan(fov_h / 2)` is `(416 / 0.8666667) / (512 / 0.8)`,
//! which is exactly 0.75: the picture is a 4:3 frustum drawn into the 512 by 416 buffer with non-square pixels.

pub mod frustum;

/// The address of the view index word (`0x349e40`).
pub const VIEW_INDEX_WORD: u32 = 0x349e40;
/// The address of the selector override byte (`0x349e44`).
pub const VIEW_SELECT_BYTE: u32 = 0x349e44;
/// The projection array a selector of 0 reads while the override byte is 0.
pub const PROJECTION_ARRAY_CULL: u32 = 0x3b23f0;
/// The projection array every other selector reads.
pub const PROJECTION_ARRAY: u32 = 0x3b21f0;
/// Bytes of one projection block.
pub const PROJECTION_SIZE: u32 = 0x100;
/// The camera array a selector of 0 reads while the override byte is 0.
pub const CAMERA_ARRAY_CULL: u32 = 0x3b2710;
/// The camera array every other selector reads.
pub const CAMERA_ARRAY: u32 = 0x3b25f0;
/// Bytes of one camera block (0x24 words).
pub const CAMERA_SIZE: u32 = 0x90;
/// The camera object's vtable word (what the constructor stores at `+0x00`).
pub const CAMERA_VTABLE: u32 = 0x349e50;

/// orig 0x106868: the current view index, the word at `0x349e40`.
pub fn view_index(index_word: u32) -> u32 {
    index_word
}

/// orig 0x106878: the address of the projection block of view `index_word`. The selector is a byte: 0 with the
/// override byte `select_byte` also 0 reads the cull array, anything else the other array; the view index times 0x100
/// is added.
pub fn projection_address(selector: u32, select_byte: u8, index_word: u32) -> u32 {
    let base = if selector as u8 == 0 && select_byte == 0 { PROJECTION_ARRAY_CULL } else { PROJECTION_ARRAY };
    base.wrapping_add(index_word.wrapping_mul(PROJECTION_SIZE))
}

/// orig 0x1068c0: the address of the camera block of view `index_word`, chosen by the selector as
/// [`projection_address`] chooses its array.
pub fn camera_address(selector: u32, select_byte: u8, index_word: u32) -> u32 {
    let base = if selector as u8 == 0 && select_byte == 0 { CAMERA_ARRAY_CULL } else { CAMERA_ARRAY };
    base.wrapping_add(index_word.wrapping_mul(CAMERA_SIZE))
}

fn rd_f32(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

fn wr_f32(b: &mut [u8], o: usize, v: f32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

fn wr_bytes(b: &mut [u8], o: usize, v: &[u8]) {
    b[o..o + v.len()].copy_from_slice(v);
}

/// The EE FPU divide: a zero divisor gives the largest finite float, signed like the dividend times the divisor's sign;
/// an overflow reads as the largest finite float.
fn fpu_div(s: f32, t: f32) -> f32 {
    if t == 0.0 { f32::MAX.copysign(s) * t.signum() } else { frustum::vu(s / t) }
}

/// orig 0x106e48: sets the focal length and the two fields of view of a projection block. `+0x40` = `focal`;
/// `+0x44` = `2 atan((512 / scale_x) * 0.5 / focal)` and `+0x48` = `2 atan((416 / scale_y) * 0.5 / focal)` with the
/// scale words at `+0x4c` and `+0x50`. The original's `atan` is the library routine `0x2ffd38`; the port uses the
/// standard single precision arctangent (the oracle test compares within 1e-5 relative).
pub fn set_field_of_view(proj: &mut [u8], focal: f32) {
    let half_w = frustum::vu(fpu_div(512.0, rd_f32(proj, 0x4c)) * 0.5);
    wr_f32(proj, 0x40, focal);
    let a = fpu_div(half_w, focal).atan();
    let half_h = fpu_div(416.0, rd_f32(proj, 0x50));
    wr_f32(proj, 0x44, a + a);
    let b = fpu_div(frustum::vu(half_h * 0.5), focal).atan();
    wr_f32(proj, 0x48, b + b);
}

/// orig 0x106918: the projection constructor; it does nothing.
pub fn projection_construct() {}

/// orig 0x106928: the projection destructor; it does nothing.
pub fn projection_destruct() {}

/// orig 0x106930: the address of the cull matrix of a projection block at `base` (`base + 0x80`).
pub fn projection_cull_matrix(base: u32) -> u32 {
    base.wrapping_add(0x80)
}

/// orig 0x106940: the address of the view-clip-GS matrix of a projection block at `base` (`base + 0xc0`).
pub fn projection_view_clip_gs_matrix(base: u32) -> u32 {
    base.wrapping_add(0xc0)
}

/// orig 0x106f28: the camera constructor stores its vtable word at `+0x00`.
pub fn camera_construct(cam: &mut [u8]) {
    wr_bytes(cam, 0, &CAMERA_VTABLE.to_le_bytes());
}

/// orig 0x106f48: the camera destructor stores the same vtable word.
pub fn camera_destruct(cam: &mut [u8]) {
    wr_bytes(cam, 0, &CAMERA_VTABLE.to_le_bytes());
}

/// orig 0x107190: sets the position x, y, z (`+0x50`, `+0x54`, `+0x58`); the fourth lane stays.
pub fn camera_set_position(cam: &mut [u8], x: f32, y: f32, z: f32) {
    wr_f32(cam, 0x50, x);
    wr_f32(cam, 0x54, y);
    wr_f32(cam, 0x58, z);
}

/// orig 0x1071a0: sets the reference point x, y, z (`+0x60`, `+0x64`, `+0x68`).
pub fn camera_set_reference(cam: &mut [u8], x: f32, y: f32, z: f32) {
    wr_f32(cam, 0x60, x);
    wr_f32(cam, 0x64, y);
    wr_f32(cam, 0x68, z);
}

/// orig 0x1071b0: sets the up vector x, y, z (`+0x70`, `+0x74`, `+0x78`).
pub fn camera_set_up_vector(cam: &mut [u8], x: f32, y: f32, z: f32) {
    wr_f32(cam, 0x70, x);
    wr_f32(cam, 0x74, y);
    wr_f32(cam, 0x78, z);
}

/// orig 0x1071c0: sets the roll (`+0x80`).
pub fn camera_set_roll(cam: &mut [u8], roll: f32) {
    wr_f32(cam, 0x80, roll);
}

/// orig 0x1070e8: sets position, reference and up vector from three four-float vectors (16 bytes each into `+0x50`,
/// `+0x60`, `+0x70`; the original skips a copy whose source is the destination itself).
pub fn camera_set_three(cam: &mut [u8], position: &[u8; 16], reference: &[u8; 16], up: &[u8; 16]) {
    wr_bytes(cam, 0x50, position);
    wr_bytes(cam, 0x60, reference);
    wr_bytes(cam, 0x70, up);
}

/// orig 0x107130: sets position and reference from two four-float vectors (`+0x50`, `+0x60`).
pub fn camera_set_two(cam: &mut [u8], position: &[u8; 16], reference: &[u8; 16]) {
    wr_bytes(cam, 0x50, position);
    wr_bytes(cam, 0x60, reference);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_view_has_a_four_by_three_frustum() {
        let mut p = vec![0u8; 0x100];
        wr_f32(&mut p, 0x4c, f32::from_bits(0x3f4c_cccc));
        wr_f32(&mut p, 0x50, f32::from_bits(0x3f5d_dddd));
        set_field_of_view(&mut p, 512.0);
        let (h, v) = (rd_f32(&p, 0x44), rd_f32(&p, 0x48));
        let ratio = (v * 0.5).tan() / (h * 0.5).tan();
        assert!((ratio - 0.75).abs() < 1e-4, "ratio {ratio}");
    }

    #[test]
    fn view_arrays_follow_the_selector() {
        assert_eq!(projection_address(0, 0, 2), 0x3b23f0 + 0x200);
        assert_eq!(projection_address(0, 1, 2), 0x3b21f0 + 0x200);
        assert_eq!(projection_address(1, 0, 0), 0x3b21f0);
        assert_eq!(camera_address(1, 0, 3), 0x3b25f0 + 3 * 0x90);
    }
}
