//! Prototype ports made through the factory loop (brief, generated oracle test, port, green). In the landed factory
//! these live in the crate of their system; they sit here only to show the loop end to end. Blocks are byte slices in
//! the original's layout (RULES C7), read and written through the typed accessors of [`crate::layout`].

use crate::layout::Bytes;

/// orig 0x127b98: an angle brought into [-pi, pi] by whole turns (a positive angle above pi loses 2 pi until it is at
/// most pi; any other angle below -pi gains 2 pi until it is at least -pi). The constants are the executable's
/// 0x378a20 (pi), 0x378a24 (2 pi) and 0x378a28 (-pi).
pub fn wrap_angle(mut x: f32) -> f32 {
    const PI: f32 = std::f32::consts::PI;
    const TURN: f32 = 2.0 * std::f32::consts::PI;
    if 0.0 < x {
        while PI < x {
            x -= TURN;
        }
    } else {
        while x < -PI {
            x += TURN;
        }
    }
    x
}

crate::layout!(pub BandObj, 0x120, {
    band / set_band: u8 @ 0x10d,
    rec / set_rec: u32 @ 0x114,
});

crate::layout!(pub BandRec, 0x50, {
    t3 / set_t3: f32 @ 0x38,
    t2 / set_t2: f32 @ 0x3c,
    t1 / set_t1: f32 @ 0x40,
    t0 / set_t0: f32 @ 0x44,
});

/// orig 0x148550: a band (0, 1, 2) for a value against four descending thresholds of the record a1+0x114 points at
/// (+0x44, +0x40, +0x3c, +0x38), with hysteresis: between +0x44 and +0x40, and between +0x3c and +0x38, the band in
/// a1+0x10d is kept. a0 is not read. `obj` is a1's block (only its first 0x120 bytes are known), `rec` the block its
/// +0x114 points at (a handle in the port; the original reads through it unconditionally).
pub fn threshold_band(obj: &mut BandObj, rec: &BandRec, v: f32) -> u8 {
    let band = if v > rec.t0() {
        2
    } else if v > rec.t1() {
        obj.band()
    } else if v > rec.t2() {
        1
    } else if v > rec.t3() {
        obj.band()
    } else {
        0
    };
    obj.set_band(band);
    band
}

/// orig 0x239468: true when the record's state (+0x38) is 1 or 2 and either its flag byte (+0x50) is set or its
/// value +0x40 is below +0x44.
pub fn state_active(rec: &[u8]) -> bool {
    let s = rec.u32_at(0x38);
    s.wrapping_sub(1) < 2 && (rec.u8_at(0x50) != 0 || rec.f32_at(0x40) < rec.f32_at(0x44))
}

/// orig 0x103b18: stores its argument in the word at 0x33e78c (an example of a global the port keeps in a block that
/// mirrors the original's global memory; `word` is those 4 bytes).
pub fn set_word_33e78c(word: &mut [u8], v: i32) {
    word[..4].copy_from_slice(&v.to_le_bytes());
}
