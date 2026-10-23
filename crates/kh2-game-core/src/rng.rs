//! The game's random number generator, ported exactly (core runtime, WP-L1-P1; docs/systems/core-runtime/runtime.md "RNG").
//!
//! The original has one generator: a 64-bit word in the C library's reentrancy block (the block pointer sits at `0x373cf0`, the word at
//! its `+0xa8`). The boot thread seeds it with `0x875b2f` and every user steps that one word, so the sequence a player sees depends on
//! how many draws happened before. [`Lcg`] is the word and its operations; the free functions below step the one process-wide word
//! ([`state`]) the way the original's callers do, so every call site of the port draws from one stream.
//!
//! The word is part of a saved moment: a `KH2_STATE` file made by `kh2 state from-p2s` carries it in `rig.rng` and the global word
//! starts from it; without the key (or without `KH2_STATE`) it starts from the boot seed.
//!
//! This is generator A. The original has a second, unrelated generator (`DAT_003716a0`, `x * 0x10dcd`) that the placement code, the
//! script host and a few effects use; it is not in this file.
//!
//! Tests that count on a sequence use [`with_stream`], which gives the calling thread its own word for the length of the closure.

use std::cell::RefCell;
use std::sync::{LazyLock, Mutex};

/// The seed the boot thread passes (`FUN_00101d68` calls `FUN_001062f0(0x875b2f)`): the same on every boot.
pub const BOOT_SEED: u32 = 0x0087_5b2f;

/// The multiplier word the original reads at `0x385f90` (`6364136223846793005`, the C library's); the oracle test checks it against the
/// executable.
pub const MULTIPLIER: u64 = 0x5851_f42d_4c95_7f2d;

/// `DAT_003780e0` and `DAT_003780e4`: the divisor that turns a draw into a fraction (both hold 2147483520.0, one float step below 2^31).
const DIVISOR: f32 = 2_147_483_520.0;

/// The 64-bit generator word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lcg(pub u64);

/// The EE's float unit has no infinities: an overflow saturates to the largest finite number.
fn finite(x: f32) -> f32 {
    if x.is_nan() {
        f32::MAX
    } else if x.is_infinite() {
        f32::MAX.copysign(x)
    } else {
        x
    }
}

impl Lcg {
    /// The state a fresh boot has.
    pub const fn boot() -> Lcg {
        Lcg(BOOT_SEED as u64)
    }

    /// Seeds the word: the low 32 bits of `seed` become the whole word (the high half is cleared).
    ///
    /// orig 0x1062f0
    /// orig 0x311790
    pub fn seed(&mut self, seed: u64) {
        self.0 = seed & 0xffff_ffff;
    }

    /// One draw: the word becomes `word * M + 1` (modulo 2^64) and the answer is bits 32..62 of the new word, a 31-bit number.
    ///
    /// orig 0x3117a8
    pub fn rand(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(MULTIPLIER).wrapping_add(1);
        ((self.0 >> 32) as u32) & 0x7fff_ffff
    }

    /// A whole number in `0..n` (`n` an unsigned count): `n * draw / 2147483520.0` cut to an integer. A count of 0 still draws once.
    ///
    /// orig 0x106308
    pub fn below(&mut self, n: u32) -> u32 {
        let draw = self.rand() as i32 as f32;
        fixuns(finite(finite(n as f32 * draw) / DIVISOR))
    }

    /// `lo + below(hi - lo)`, the difference taken in 32 bits.
    ///
    /// orig 0x106380
    pub fn between_int(&mut self, lo: i32, hi: i32) -> i32 {
        lo.wrapping_add(self.below(hi.wrapping_sub(lo) as u32) as i32)
    }

    /// A float in `[0, x]`: `x * draw / 2147483520.0`.
    ///
    /// orig 0x1063b8
    pub fn frac(&mut self, x: f32) -> f32 {
        let draw = self.rand() as i32 as f32;
        finite(finite(x * draw) / DIVISOR)
    }

    /// A float between `lo` and `hi`: `lo + frac(hi - lo)`.
    ///
    /// orig 0x106408
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        finite(lo + self.frac(finite(hi - lo)))
    }
}

/// `__fixunssfsi` (`FUN_00326940`): a float cut to an unsigned integer; negative gives 0, too large gives all ones.
fn fixuns(f: f32) -> u32 {
    kh2_model::texanim::fixuns(f)
}

/// The word a state file carries: `rig.rng` of the `KH2_STATE` file (`kh2 state from-p2s` writes it from the original's memory), if any.
fn state_file_word() -> Option<u64> {
    let path = std::env::var("KH2_STATE").ok()?;
    word_of(&std::fs::read_to_string(path).ok()?)
}

/// `rig.rng` of a state file's text: 16 hex digits (a JSON number is read too, exact below 2^53).
fn word_of(text: &str) -> Option<u64> {
    let json = crate::progress::Json::parse(text).ok()?;
    match json.get("rig")?.get("rng")? {
        crate::progress::Json::Str(s) => u64::from_str_radix(s.trim_start_matches("0x"), 16).ok(),
        crate::progress::Json::Num(n) if *n >= 0.0 => Some(*n as u64),
        _ => None,
    }
}

/// The one word of the process. Starts from the state file's word, else the boot seed.
static GLOBAL: LazyLock<Mutex<Lcg>> = LazyLock::new(|| Mutex::new(state_file_word().map_or(Lcg::boot(), Lcg)));

thread_local! {
    /// A word of the calling thread that shadows the process-wide one (tests, [`with_stream`]).
    static LOCAL: RefCell<Option<Lcg>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Lcg) -> R) -> R {
    let mut f = Some(f);
    let local = LOCAL.with(|l| l.borrow_mut().as_mut().map(|s| (f.take().expect("once"))(s)));
    match local {
        Some(r) => r,
        None => f.take().map(|f| f(&mut GLOBAL.lock().unwrap_or_else(|e| e.into_inner()))).expect("once"),
    }
}

/// A private word for the calling thread, from [`stream`]; the process-wide word is back when it is dropped.
pub struct Stream(Option<Lcg>);

impl Drop for Stream {
    fn drop(&mut self) {
        LOCAL.with(|l| *l.borrow_mut() = self.0.take());
    }
}

/// Gives the calling thread a private word seeded with `seed` until the returned guard is dropped, so its draws are the same on every
/// run and do not move the process-wide word (tests that count on a sequence).
pub fn stream(seed: u32) -> Stream {
    Stream(LOCAL.with(|l| l.replace(Some(Lcg(seed as u64)))))
}

/// Runs `f` inside a [`stream`].
pub fn with_stream<R>(seed: u32, f: impl FnOnce() -> R) -> R {
    let _s = stream(seed);
    f()
}

/// The current word (what the rig reads from the original's memory).
pub fn state() -> u64 {
    with(|g| g.0)
}

/// Overwrites the word (a loaded state).
pub fn set_state(word: u64) {
    with(|g| g.0 = word)
}

/// Seeds the process-wide word like the boot thread (`FUN_001062f0`).
pub fn seed(seed: u32) {
    with(|g| g.seed(seed as u64))
}

/// [`Lcg::rand`] on the process-wide word.
pub fn rand() -> u32 {
    with(|g| g.rand())
}

/// [`Lcg::below`] on the process-wide word.
pub fn below(n: u32) -> u32 {
    with(|g| g.below(n))
}

/// [`Lcg::between_int`] on the process-wide word.
pub fn between_int(lo: i32, hi: i32) -> i32 {
    with(|g| g.between_int(lo, hi))
}

/// [`Lcg::frac`] on the process-wide word.
pub fn frac(x: f32) -> f32 {
    with(|g| g.frac(x))
}

/// [`Lcg::range`] on the process-wide word.
pub fn range(lo: f32, hi: f32) -> f32 {
    with(|g| g.range(lo, hi))
}

/// The party members' `getRand(n)` (`FUN_0010f420`): a whole number in `0..n`, the same as [`below`].
///
/// orig 0x10f420
pub fn friend_rand(n: u32) -> u32 {
    below(n)
}

/// The party members' 0..1 roll (`FUN_0010f438`): `below(20001)` times `DAT_00378570` (5e-5, the float nearest 1/20000).
///
/// orig 0x10f438
pub fn friend_roll() -> f32 {
    finite(below(20001) as f32 * 5e-5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boot_stream_steps_the_multiply_and_add_word() {
        let mut g = Lcg::boot();
        let first = g.rand();
        assert_eq!(g.0, 0x87_5b2fu64.wrapping_mul(MULTIPLIER).wrapping_add(1));
        assert_eq!(first, (g.0 >> 32) as u32 & 0x7fff_ffff);
        assert!(first <= 0x7fff_ffff);
    }

    #[test]
    fn seeding_keeps_the_low_half_only() {
        let mut g = Lcg(u64::MAX);
        g.seed(0xdead_beef_1234_5678);
        assert_eq!(g.0, 0x1234_5678);
    }

    #[test]
    fn ranges_stay_inside_their_bounds() {
        let mut g = Lcg::boot();
        for _ in 0..2000 {
            assert!(g.below(7) < 7);
            let v = g.between_int(-3, 4);
            assert!((-3..4).contains(&v), "{v}");
            let f = g.frac(2.0);
            assert!((0.0..=2.0).contains(&f), "{f}");
            let r = g.range(360.0, 720.0);
            assert!((360.0..=720.0).contains(&r), "{r}");
        }
        assert_eq!(g.below(0), 0);
    }

    #[test]
    fn a_stream_is_private_to_its_thread_and_repeatable() {
        let a: Vec<u32> = with_stream(5, || (0..4).map(|_| below(1000)).collect());
        let b: Vec<u32> = with_stream(5, || (0..4).map(|_| below(1000)).collect());
        assert_eq!(a, b);
    }

    #[test]
    fn a_state_file_carries_the_word_as_hex() {
        assert_eq!(word_of(r#"{"rig": {"vsync": 5, "rng": "0123456789abcdef"}}"#), Some(0x0123_4567_89ab_cdef));
        assert_eq!(word_of(r#"{"rig": {"vsync": 5}}"#), None);
        assert_eq!(word_of(r#"{"munny": 1}"#), None);
    }

    #[test]
    fn the_friend_roll_is_a_fraction() {
        with_stream(1, || {
            for _ in 0..500 {
                let r = friend_roll();
                assert!((0.0..=1.0001).contains(&r), "{r}");
            }
        });
    }
}
