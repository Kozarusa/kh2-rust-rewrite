//! The pad unit, ported on the original's byte layout (core runtime, WP-L1-P20; docs/systems/core-runtime/runtime.md "Pad unit").
//!
//! The original keeps two physical pad records (0x180 bytes each, from `0x35b4c0`) and two logical pad blocks (0x60 bytes each, from
//! `0x35b7c0`; word +0x44 of each points at its physical record). A logical block holds four 64-bit button words (held, newly
//! pressed, released, repeating), the repeat timer (+0x20 area) and a flag word at +0x48 of the record. The pad task (`0x167038`,
//! priority 110000 on the root manager) reads both physical pads (`0x167550`, hardware) and rebuilds both logical blocks
//! (`0x167840`) once per game frame. The stick scale (`0x522168..0x522170`) is set once at start from three floats in the
//! executable; the active button set (`0x35b8ac`) is picked by number from a table of six pointers (`0x35b888`).
//!
//! This file uses nothing outside `std`: `crates/kh2-ard/tests/button_tables.rs` includes it by path to compare the executable's
//! button tables with [`logical`] (the crate cannot depend on this one).
//!
//! What is NOT here: the pad library start (`0x167588`, hardware), the physical read (`0x167310`) and the logical build
//! (`0x1676a0`, `0x167738`) behind the two calls of the task, which other packages own. [`PadUnit::task`] makes those two calls
//! through the closure it is given. Vibration is [`Vibration`]. Nothing in the app uses this module yet.

/// Address of the first physical pad record and the distance to the next.
pub const RECORD_BASE: u32 = 0x35b4c0;
pub const RECORD_SIZE: u32 = 0x180;
/// Address of the first logical pad block and the distance to the next.
pub const LOGICAL_BASE: u32 = 0x35b7c0;
pub const LOGICAL_SIZE: usize = 0x60;
/// End of the run of logical blocks (the loop of the set selection stops there).
pub const LOGICAL_END: u32 = 0x35b880;
/// The task's priority on the root manager and its function.
pub const TASK_PRIORITY: i32 = 110_000;
pub const TASK_FUNCTION: u32 = 0x167038;

/// The first repeat delay, the repeat interval and the stick constant that `0x166fd0` stores come from three floats in the executable
/// (`0x379680`, `0x379684`, `0x379688`); the stick centre and length cap that the stick conversion reads are `0x37968c` and `0x379690`.
pub const STICK_CONSTANTS: [u32; 3] = [0x379680, 0x379684, 0x379688];

/// The logical button words the port names: the bits the shared table (`0x379698`) gives the pad, which every menu and the field
/// read, plus the field's face and shoulder buttons from the field set (`0x379768`) [documented in docs/systems/ui/menus-pause.md
/// 1.7]. `button_tables.rs` checks each against the executable's tables.
pub mod logical {
    /// The soft-reset combination (Select, Start, L2, R2, L1, R1 held together, physical mask 0xf09).
    pub const RESET: u64 = 0x1;
    /// Start.
    pub const START: u64 = 0x2;
    /// The synthetic physical bits 0x01000000 and 0x02000000 (confirm and cancel after the Cross/Circle choice) give these.
    pub const CONFIRM: u64 = 0x4;
    pub const CANCEL: u64 = 0x8;
    /// The d-pad (and the stick pushed in that direction, physical bits 0x00800000 / 0x00200000 / 0x00400000 / 0x00100000).
    pub const UP: u64 = 0x10;
    pub const DOWN: u64 = 0x20;
    pub const LEFT: u64 = 0x40;
    pub const RIGHT: u64 = 0x80;

    /// A physical button of the pad buffer (bit order of the pad read, pressed = 1) and the logical word the shared table maps it to.
    /// The d-pad bits map to the same logical bit as the stick-direction bits; `stick` is that second physical mask.
    pub struct Shared {
        pub physical: u64,
        pub stick: u64,
        pub logical: u64,
    }

    /// The d-pad and Start rows of the shared table.
    pub const SHARED: [Shared; 5] = [
        Shared { physical: 0x8, stick: 0, logical: START },
        Shared { physical: 0x10, stick: 0x0080_0000, logical: UP },
        Shared { physical: 0x40, stick: 0x0020_0000, logical: DOWN },
        Shared { physical: 0x80, stick: 0x0040_0000, logical: LEFT },
        Shared { physical: 0x20, stick: 0x0010_0000, logical: RIGHT },
    ];

    /// The physical bits of the pad buffer.
    pub const PHYS_SELECT: u64 = 0x1;
    pub const PHYS_START: u64 = 0x8;
    pub const PHYS_UP: u64 = 0x10;
    pub const PHYS_RIGHT: u64 = 0x20;
    pub const PHYS_DOWN: u64 = 0x40;
    pub const PHYS_LEFT: u64 = 0x80;

    /// The soft-reset combination as a physical mask (Select, Start, L2, R2, L1, R1).
    pub const PHYS_RESET: u64 = 0xf09;
    pub const SYNTH_CONFIRM: u64 = 0x0100_0000;
    pub const SYNTH_CANCEL: u64 = 0x0200_0000;

    pub const PHYS_L2: u64 = 0x100;
    pub const PHYS_R2: u64 = 0x200;
    pub const PHYS_L1: u64 = 0x400;
    pub const PHYS_R1: u64 = 0x800;
    pub const PHYS_TRIANGLE: u64 = 0x1000;
    pub const PHYS_CIRCLE: u64 = 0x2000;
    pub const PHYS_CROSS: u64 = 0x4000;
    pub const PHYS_SQUARE: u64 = 0x8000;

    /// A row of a button set: the physical mask and the logical bit the set gives it.
    pub struct Row {
        pub physical: u64,
        pub logical: u64,
        pub name: &'static str,
    }

    /// The single-button rows of the field set (set 1, table `0x379768`, chosen by the field start `0x102a60`) that name one physical
    /// button and one logical bit each; the set has more rows (combinations with the right stick, shoulder pairs, camera flags). What
    /// each logical bit does in the field is not established here: the readers of the logical words are other units [gap].
    pub const FIELD: [Row; 11] = [
        Row { physical: PHYS_CROSS, logical: 0x800_0000, name: "Cross" },
        Row { physical: PHYS_CIRCLE, logical: 0x100_0000, name: "Circle" },
        Row { physical: PHYS_TRIANGLE, logical: 0x200_0000, name: "Triangle" },
        Row { physical: PHYS_SQUARE, logical: 0x400_0000, name: "Square" },
        Row { physical: PHYS_L1, logical: 0x80_0000, name: "L1" },
        Row { physical: PHYS_R1, logical: 0x800, name: "R1" },
        Row { physical: PHYS_SELECT, logical: 0x40_0000, name: "Select" },
        Row { physical: PHYS_UP, logical: 0x4000, name: "Up" },
        Row { physical: PHYS_DOWN, logical: 0x8000, name: "Down" },
        Row { physical: PHYS_LEFT, logical: 0x1_0000, name: "Left" },
        Row { physical: PHYS_RIGHT, logical: 0x1000_0000, name: "Right" },
    ];

    /// The three rows of set 3 (table `0x379ab8`).
    pub const SET3: [Row; 3] = [
        Row { physical: PHYS_R1, logical: 0x100, name: "R1" },
        Row { physical: PHYS_L1, logical: 0x200, name: "L1" },
        Row { physical: PHYS_TRIANGLE, logical: 0x400, name: "Triangle" },
    ];

    /// The d-pad rows that sets 4 and 5 (tables `0x379af8`, `0x379b88`) start with.
    pub const SET5_DPAD: [Row; 4] = [
        Row { physical: PHYS_UP, logical: 0x100, name: "Up" },
        Row { physical: PHYS_DOWN, logical: 0x200, name: "Down" },
        Row { physical: PHYS_LEFT, logical: 0x400, name: "Left" },
        Row { physical: PHYS_RIGHT, logical: 0x800, name: "Right" },
    ];
}

/// The logical word a table gives a physical word: every row `(mask, bit)` whose mask is fully pressed adds its bit. This is what
/// `FUN_00167660` does for the shared table and for the active set; `FUN_001676a0` ORs the two results.
pub fn apply_table(rows: &[(u64, u64)], physical: u64) -> u64 {
    rows.iter().filter(|&&(mask, _)| physical & mask == mask).fold(0, |acc, &(_, bit)| acc | bit)
}

fn put32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

fn get64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

fn put64(b: &mut [u8], o: usize, v: u64) {
    b[o..o + 8].copy_from_slice(&v.to_le_bytes());
}

/// The unit's globals as the original lays them out.
#[derive(Clone, PartialEq, Debug)]
pub struct PadUnit {
    /// `0x522168..0x522170`: the three floats `FUN_00167058` stores (the stick constant, the first repeat delay, the repeat interval
    /// [inferred from the repeat code `FUN_001670d0`, which loads the second as the delay and the third as the interval]).
    pub scale: [f32; 3],
    /// `0x35b7c0` and `0x35b820`: the two logical pad blocks.
    pub logical: [[u8; LOGICAL_SIZE]; 2],
    /// `0x35b888`: the pointer table of the six button sets (data of the executable; the port is given it).
    pub sets: [u32; 6],
    /// `0x35b8ac`: the active set's table address.
    pub active: u32,
    /// `0x522150`: the number of the active set, -1 before the first selection.
    pub active_number: u32,
    /// `0x35b880`.
    pub word_880: u32,
}

impl PadUnit {
    pub fn new(sets: [u32; 6]) -> PadUnit {
        PadUnit { scale: [0.0; 3], logical: [[0; LOGICAL_SIZE]; 2], sets, active: 0, active_number: 0, word_880: 0 }
    }

    /// Stores the three floats of the stick and repeat scale.
    ///
    /// orig 0x167058
    pub fn set_scale(&mut self, a: f32, b: f32, c: f32) {
        self.scale = [a, b, c];
    }

    /// The address of physical pad record `n`.
    ///
    /// orig 0x1675e0
    pub fn record_address(n: i32) -> u32 {
        RECORD_BASE.wrapping_add((n as u32).wrapping_mul(RECORD_SIZE))
    }

    /// Picks button set `n`: the active table becomes the set's pointer, both logical blocks are masked with 0xff (the words keep only
    /// their low byte, the repeat vectors and the flag word are cleared), and the set number is stored.
    ///
    /// orig 0x167908
    pub fn select_set(&mut self, n: u32) {
        self.active = self.sets.get(n as usize).copied().unwrap_or(0);
        for b in self.logical.iter_mut() {
            mask_block(b, 0xff);
        }
        self.active_number = n;
    }

    /// Links each logical block to its physical record, resets the set number to -1 and a word to 0, and picks set 0.
    ///
    /// orig 0x167600
    pub fn init_blocks(&mut self) {
        for (i, b) in self.logical.iter_mut().enumerate() {
            put32(b, 0x44, Self::record_address(i as i32));
        }
        self.active_number = u32::MAX;
        self.word_880 = 0;
        self.select_set(0);
    }

    /// Starts the unit: the scale from the given floats, the library start (hardware, not ported), the blocks, and the pad task
    /// created through `create_task(bits, priority, function)` on the root manager.
    ///
    /// orig 0x166fd0
    pub fn start(&mut self, constants: [f32; 3], mut create_task: impl FnMut(u32, i32, u32)) {
        self.set_scale(constants[0], constants[1], constants[2]);
        self.init_blocks();
        create_task(0, TASK_PRIORITY, TASK_FUNCTION);
    }

    /// The pad task: reads both pads (`0x167550`), then rebuilds both logical blocks (`0x167840`). `call` makes a call of the original
    /// function at the given address.
    ///
    /// orig 0x167038
    pub fn task(&mut self, mut call: impl FnMut(u32)) {
        call(0x167550);
        call(0x167840);
    }

    /// The logical block `i` as the four button words: held, newly pressed, released, repeating.
    pub fn words(&self, i: usize) -> [u64; 4] {
        let b = &self.logical[i];
        [get64(b, 0), get64(b, 8), get64(b, 16), get64(b, 24)]
    }
}

/// Masks the four 64-bit words of a logical block and clears its two repeat vectors (+0x20, +0x30) and the word at +0x40. This is
/// `FUN_00167078`, ported with its oracle in `leaves_b::mask_block_167078`; the copy keeps this file free of crate paths.
fn mask_block(b: &mut [u8], mask: u64) {
    for i in 0..4 {
        let v = get64(b, i * 8) & mask;
        put64(b, i * 8, v);
    }
    put32(b, 0x40, 0);
    b[0x20..0x40].fill(0);
}

/// The vibration curve unit (`0x36ca08..0x36ca28`): two curves, one per motor, played over `length` frames. Each curve is a list of
/// keys of two ints (time, strength) at an address of the original; the strength between two keys is interpolated and written as a
/// byte into the physical pad record's request bytes (+0x44, +0x45, which the pad read `0x167310` turns into the motor command).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Vibration {
    /// `0x36ca08` and `0x36ca0c`: the number of keys of each curve.
    pub counts: [i32; 2],
    /// `0x36ca10` and `0x36ca14`: the address of each curve's first key.
    pub curves: [u32; 2],
    /// `0x36ca18`: 0 idle, 1 asked to start, 2 playing.
    pub state: i32,
    /// `0x36ca1c`: the time on the curves, in the unit of one call of the step.
    pub time: f32,
    /// `0x36ca20`: the length of the play.
    pub length: i32,
    /// `0x36ca24`: non-zero when the curves are used.
    pub use_curves: u32,
}

/// The EE's float unit as kh2-ee (the oracle) has it: single-precision arithmetic that rounds to nearest, and an overflow or a NaN
/// saturates to the largest finite number (the EE has no infinity or NaN). The real EE rounds toward zero in some operations and
/// treats exponent 255 as a number [inferred from the EE documentation; the oracle does not model it, so the port follows the oracle].
pub mod ee {
    /// Saturates an overflow or NaN.
    pub fn sat(f: f32) -> f32 {
        if f.is_nan() {
            f32::MAX
        } else if f.is_infinite() {
            f32::MAX.copysign(f)
        } else {
            f
        }
    }
    pub fn add(a: f32, b: f32) -> f32 {
        sat(a + b)
    }
    pub fn sub(a: f32, b: f32) -> f32 {
        sat(a - b)
    }
    pub fn mul(a: f32, b: f32) -> f32 {
        sat(a * b)
    }
    /// The divisor must not be zero.
    pub fn div(a: f32, b: f32) -> f32 {
        sat(a / b)
    }
    /// An int converted to a float (`cvt.s.w`).
    pub fn from_i32(x: i32) -> f32 {
        x as f32
    }
}

impl Vibration {
    /// One step of the play. `one` is the float at `0x37f454` (1.0): the time grows by it per call, and it is the fraction between
    /// two keys of equal time. `read32` reads a word of the original's memory (the curve keys). `record0` is the first physical pad
    /// record; the motor bytes land at +0x44 and +0x45.
    ///
    /// orig 0x239b20
    pub fn step(&mut self, one: f32, mut read32: impl FnMut(u32) -> u32, record0: &mut [u8]) {
        if self.state == 1 {
            self.state = 2;
            self.time = 0.0;
        } else if self.state < 2 || self.state != 2 {
            return;
        } else {
            self.time = ee::add(self.time, one);
        }
        if ee::from_i32(self.length) <= self.time {
            self.state = 0;
            self.time = ee::from_i32(self.length.wrapping_sub(1));
        }
        if self.use_curves == 0 {
            return;
        }
        for i in 0..2 {
            let first = self.curves[i];
            if ee::from_i32(read32(first) as i32) > self.time {
                continue;
            }
            let mut n = self.counts[i];
            let mut p = first;
            let found = loop {
                n = n.wrapping_sub(1);
                if n < 1 {
                    break None;
                }
                let next_time = ee::from_i32(read32(p.wrapping_add(8)) as i32);
                if next_time >= self.time {
                    break Some(next_time);
                }
                p = p.wrapping_add(8);
            };
            let Some(next_time) = found else { continue };
            let t0 = ee::from_i32(read32(p) as i32);
            let span = ee::sub(next_time, t0);
            let frac = if span != 0.0 { ee::div(ee::sub(self.time, t0), span) } else { one };
            let v0 = read32(p.wrapping_add(4)) as i32;
            let v1 = read32(p.wrapping_add(12)) as i32;
            let delta = ee::mul(ee::from_i32(v1.wrapping_sub(v0)), frac) as i32;
            record0[0x44 + i] = (v0 as u8).wrapping_add(delta as u8);
        }
    }

    /// The vibration task: one step, and the task goes on next frame unless the game mode word (`0x1dc0dd0`) is 2 or the play is idle
    /// (state 0). Returns true when the task ends (the caller clears the task's function, `FUN_00102498`).
    ///
    /// orig 0x239ac8
    pub fn task(&mut self, mode: u32, one: f32, read32: impl FnMut(u32) -> u32, record0: &mut [u8]) -> bool {
        self.step(one, read32, record0);
        !(mode != 2 && self.state != 0)
    }
}
