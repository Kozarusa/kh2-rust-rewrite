//! The save body's CRC (`FUN_0023e3d8` and its table, reset and update routines).
//!
//! It is not the common CRC-32: the table routine runs 255 rounds per byte value, not 8, and the update returns the complement of the
//! running state without storing it, so calls chain. The oracle test (`tests/orig_save_file.rs`) runs the original's table, reset and
//! update over random buffers and compares every table word and every returned value.

use std::sync::OnceLock;

/// The polynomial of the table routine.
const POLY: u32 = 0x04c1_1db7;

/// The table the original builds into its 0x400-byte buffer (`DAT_0036cb3c`).
///
/// orig 0x23e4c0
pub fn build_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for (i, w) in t.iter_mut().enumerate() {
        let mut c = (i as u32) << 24;
        // The loop counts from 0 while below 0xff: 255 rounds.
        for _ in 0..0xff {
            c = if c & 0x8000_0000 == 0 { c << 1 } else { (c << 1) ^ POLY };
        }
        *w = c;
    }
    t
}

fn table() -> &'static [u32; 256] {
    static T: OnceLock<[u32; 256]> = OnceLock::new();
    T.get_or_init(build_table)
}

/// The CRC unit's state (`DAT_0036cb40`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Crc {
    pub state: u32,
}

impl Default for Crc {
    fn default() -> Self {
        Crc::new()
    }
}

impl Crc {
    /// The unit after `FUN_0023e3d8` (the table built, the state reset). The allocator calls around it (`FUN_0023e3d8`'s buffer,
    /// `FUN_0023e410` that frees it, `FUN_0023e450`, `FUN_0023e488`) have no port: the table is a static.
    ///
    /// orig 0x23e3d8
    pub fn new() -> Crc {
        Crc { state: 0xffff_ffff }
    }

    /// orig 0x23e528
    pub fn reset(&mut self) {
        self.state = 0xffff_ffff;
    }

    /// Runs the bytes through the state and returns its complement (the state itself is not complemented, so a second call continues
    /// where the first stopped).
    ///
    /// orig 0x23e538
    pub fn update(&mut self, data: &[u8]) -> u32 {
        let t = table();
        for &b in data {
            self.state = (self.state << 8) ^ t[((self.state >> 24) ^ u32::from(b)) as usize];
        }
        !self.state
    }
}
