//! Screen state of the original's render setup (render-model.md d3 to d6, L1-P26b): the fog words and their setters, the
//! pass word, the screen clear colour and switch of the frame unit, the freeze copies of the area fade and events, and the
//! monochrome test. Every function works on the bytes of the block or global it touches, in the original's layout (RULES C7):
//! the doc line of each says which address the slice starts at. A call the original makes into another unit (the GS image
//! copies, the clear packet, the background loader) is returned as a [`Call`] for the caller (or the oracle test) to perform.
//!
//! Not wired into the app: the renderer has no fog, the clear colour is the fixed one in `camera.rs`, the area fade darkens the
//! live scene, and nothing is drawn monochrome. Wiring takes (1) a fog term in the map and model shaders from the near/far words
//! ([`fog_value_terms`] gives the original's two factors, so the GS fog value is `terms.0 + terms.1 * z`, clamped by the limit
//! words), (2) `ClearColor` taken from the clear block's colour bytes when its switch byte is 1 and black otherwise, (3) holding
//! the last frame while the freeze byte (0x35594e) is 1 and an area fade runs, and (4) a grey conversion of the colour data when
//! [`is_monochrome`] holds. The visible effects: distant parts of foggy areas fade to the fog colour, gaps in a sky show the
//! area's clear colour, the fade-out stops animating the world, and the monochrome world loses its colour.

fn r16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}
fn rs16(b: &[u8], o: usize) -> u32 {
    i16::from_le_bytes(b[o..o + 2].try_into().unwrap()) as i32 as u32
}
fn r32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn w32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
fn wf(b: &mut [u8], o: usize, v: f32) {
    w32(b, o, v.to_bits());
}

/// The EE's float unit has no infinities or NaN: an overflowing result is the largest finite value of its sign.
fn sat(f: f32) -> f32 {
    if f.is_nan() {
        f32::MAX
    } else if f.is_infinite() {
        f32::MAX.copysign(f)
    } else {
        f
    }
}

/// `div.s` of the EE: a zero divisor gives the largest finite value, signed by both operands.
fn ee_div(n: f32, d: f32) -> f32 {
    sat(if d == 0.0 { f32::MAX.copysign(n) * d.signum() } else { n / d })
}

/// A call the original makes into code another package owns: the target address and the eight argument registers
/// (a0 to a3, t0 to t3), the low 32 bits of each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Call {
    pub target: u32,
    pub args: [u32; 8],
}

impl Call {
    fn new(target: u32, a: &[u32]) -> Call {
        let mut args = [0u32; 8];
        args[..a.len()].copy_from_slice(a);
        Call { target, args }
    }
}

// ---- fog ---------------------------------------------------------------------------------------------------------------

/// The frame unit the clear and freeze routines are called with.
pub const FRAME_UNIT: u32 = 0x358000;
/// The fog colour bytes (red, green, blue) at 0x34e0d0..0x34e0d2.
pub const FOG_COLOUR: u32 = 0x34e0d0;
/// The four fog words 0x34e0b4 (near), 0x34e0b8 (far), 0x34e0bc and 0x34e0c0 (the two factors).
pub const FOG_RANGE: u32 = 0x34e0b4;
/// The two limit words 0x34e0c8 (min) and 0x34e0cc (max).
pub const FOG_LIMITS: u32 = 0x34e0c8;
/// The pass word of the frame's draw lists.
pub const FOG_PASS: u32 = 0x34e0d4;
/// The packet block the pass words are followed by.
pub const FOG_PACKET: u32 = 0x34e0d8;

/// Stores the fog colour bytes (`g` starts at 0x34e0d0).
///
/// orig 0x10a5e0
pub fn set_fog_colour(g: &mut [u8], r: u8, green: u8, b: u8) {
    g[0] = r;
    g[2] = b;
    g[1] = green;
}

/// Stores the near and far distances and derives the two factors of the GS fog value `F(z) = terms.0 + terms.1 * z`
/// (255 at the near end, 0 at the far end): word 2 is `-255 / (far - near)`, word 3 is `255 * far / (far - near)` (`g` starts at
/// 0x34e0b4: near, far, factor, offset).
///
/// orig 0x10a630
pub fn set_fog_range(g: &mut [u8], near: f32, far: f32) {
    wf(g, 4, far);
    wf(g, 0, near);
    wf(g, 12, ee_div(sat(far * 255.0), sat(far - near)));
    wf(g, 8, ee_div(-255.0, sat(far - near)));
}

/// The two factors stored by [`set_fog_range`] as `(offset, factor)`, so that the fog value is `offset + factor * z` (`g` as
/// for [`set_fog_range`]).
pub fn fog_value_terms(g: &[u8]) -> (f32, f32) {
    (f32::from_bits(r32(g, 12)), f32::from_bits(r32(g, 8)))
}

/// Stores the two fog limit words, passed as floats (`g` starts at 0x34e0c8).
///
/// orig 0x10a6b0
pub fn set_fog_limits(g: &mut [u8], min: f32, max: f32) {
    wf(g, 0, min);
    wf(g, 4, max);
}

/// The pass word (`g` starts at 0x34e0d4).
///
/// orig 0x10a9b0
pub fn fog_pass(g: &[u8]) -> u32 {
    r32(g, 0)
}

/// The address of the fog packet block, 0x34e0d8.
///
/// orig 0x10ac98
pub const fn fog_packet_block() -> u32 {
    FOG_PACKET
}

// ---- monochrome ----------------------------------------------------------------------------------------------------------

/// Whether the area is monochrome: the word at 0x355158 is 1 (`g` starts there).
///
/// orig 0x13ab98
pub fn is_monochrome(g: &[u8]) -> bool {
    r32(g, 0) == 1
}

// ---- the clear block ------------------------------------------------------------------------------------------------------

/// Offset of the clear block inside the frame unit (the address the original's helper at 0x14e370 computes).
pub const CLEAR_BLOCK: usize = 0x2f0;

/// Stores the clear colour bytes of a clear block at `+0x1d6` (red), `+0x1d7` (green) and `+0x1d8` (blue).
///
/// orig 0x14da88
pub fn clear_set_colour(block: &mut [u8], r: u8, g: u8, b: u8) {
    block[0x1d6] = r;
    block[0x1d8] = b;
    block[0x1d7] = g;
}

/// Reads the clear colour bytes of a clear block (red, green, blue).
///
/// orig 0x14daa8
pub fn clear_colour(block: &[u8]) -> (u8, u8, u8) {
    (block[0x1d6], block[0x1d7], block[0x1d8])
}

/// Stores the clear switch byte of a clear block (`+0x1d9`; 1 sends the clear sprite).
///
/// orig 0x14dac8
pub fn clear_set_switch(block: &mut [u8], on: u8) {
    block[0x1d9] = on;
}

/// Stores the clear colour through the frame unit (`unit` starts at 0x358000; the clear block is 0x2f0 bytes into it).
///
/// orig 0x14de68
pub fn clear_set_colour_of_unit(unit: &mut [u8], r: u8, g: u8, b: u8) {
    clear_set_colour(&mut unit[CLEAR_BLOCK..], r, g, b);
}

/// The clear task of the frame: it sends the clear packet of the frame unit (callee 0x14e348 with the unit address) and, when
/// the byte at 0x3584e0 is 1, loads the background image 0xd8 (callee 0x14ddc0). Returns the calls in order.
///
/// orig 0x14dc70
pub fn clear_task(byte_3584e0: u8) -> Vec<Call> {
    let mut v = vec![Call::new(0x14e348, &[FRAME_UNIT])];
    if byte_3584e0 == 1 {
        v.push(Call::new(0x14ddc0, &[0xd8]));
    }
    v
}

// ---- the freeze copies ------------------------------------------------------------------------------------------------------

/// The event freeze: sends a stored frame back in two halves. `unit` is the stored frame's record (width as a halfword at `+4`,
/// height at `+6`), `dst` the destination address and `mode` the packed word whose low eleven bits, shifted up five, give the base.
/// The first half copies rows 0 to height/2, the second rows height/2 onward into `dst + 0x4e000`; both go through the copy
/// routine 0x166038. Returns the two calls.
///
/// orig 0x14db58
pub fn freeze_event(unit: &[u8], dst: u32, mode: i32) -> [Call; 2] {
    let base = (mode << 21) >> 16;
    let (w, h) = (rs16(unit, 4), (r16(unit, 6) >> 1) as u32);
    [
        Call::new(0x166038, &[dst, 1, base as u32, w, h, 0, 0]),
        Call::new(0x166038, &[dst.wrapping_add(0x4e000), 1, base as u32, w, h, h, 1]),
    ]
}

/// The area fade's frame store: copies the displayed frame (record of halfwords: base `+0`, width `+4`, height `+6`) from GS
/// memory to `dst` through the GS routines: the transfer set-up 0x2f9460 for the block 0x4843d0, a cache flush (0x318620), 0x2f8f60
/// and the transfer start 0x2f9720. Returns the calls in order.
///
/// orig 0x14dbe8
pub fn store_frame(rec: &[u8], dst: u32) -> [Call; 4] {
    let base = (((r16(rec, 0) as u32) << 21) as i32) >> 16;
    let (w, h) = (rs16(rec, 4), rs16(rec, 6));
    [
        Call::new(0x2f9460, &[0x4843d0, base as u32, (r16(rec, 4) >> 6) as u32, 1, 0, 0, w, h]),
        Call::new(0x318620, &[0]),
        Call::new(0x2f8f60, &[0, 0]),
        Call::new(0x2f9720, &[0x4843d0, dst]),
    ]
}
