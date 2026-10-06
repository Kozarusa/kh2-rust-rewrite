//! Battle voice lines: how the game picks a line of the speaker's voice bank for a voice id, and
//! the one-line-at-a-time slot every speaking actor owns. Behaviour read in the decompilation of
//! the user's USA executable (`SLUS_210.05`); addresses are given per function. Nothing copied.
//!
//! The chain, from a motion's voice trigger (frame trigger 13 or 14, voice id):
//!
//! 1. `FUN_00175d18(actor, id, prio)`: nothing without a voice bank slot (actor +0x9b8). The
//!    "other party" is the actor's target object (+0x9f4) when it exists and is not the actor
//!    itself: its `NeoStatus` (FUN_001699a8: objentry +0x4c), else 0.
//! 2. `FUN_001ae3c8`: `FUN_001ae4e8` finds the `vtbl` record (0xe bytes) with byte 0 = the
//!    speaker's NeoStatus and byte 1 = the voice id, in table order; byte 2 replaces the
//!    priority. `FUN_001ae538` picks the line ([`pick_line`]).
//! 3. `FUN_001970b8` -> `FUN_001ae798`: the line is queued on the actor's slot ([`VoiceSlot`]);
//!    `FUN_001ae7d8` (every tick, FUN_001970e8) stops the slot's playing line and starts the
//!    queued one at the actor's position (sound kind 5: the same positional volume and pan as
//!    sound effects, FUN_002c2d00 -> FUN_002c27b0, with a minimum volume of 0x1800 from the slot,
//!    constructors at 0x1746e0 and 0x174800).
//!
//! The bank is `voice/<lang>/battle/<world><n>_<speaker>.vsb` (format string at 0x37b900,
//! FUN_001ae180): `lang` is the executable's language table entry chosen at start-up
//! ([`language`]), `world` the area's world code, `n` the short at +0x30 of the area's `arif`
//! record (FUN_001933a0), `speaker` the name of the NeoStatus in the table at 0x35ee10
//! ([`SpeakerNames`]). Three banks stay loaded at a time (0x1cb8290).

/// The game's one shared random generator (`DAT_003716a0`, started at 1 by FUN_002f5fc8): a
/// multiplicative congruential step, `x = x * 0x10dcd` modulo 2^32. Other users multiply by
/// 0x1c587629 or 0xa6ffb3d5 instead; the voice code uses this one.
pub const RNG_MULTIPLIER: u32 = 0x10dcd;

/// One step of the generator; returns the new state.
pub fn rng_next(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(RNG_MULTIPLIER);
    *seed
}

/// A percent roll: `FUN_00325b38(x, 0x28f5c29)`, an unsigned 64-bit division of the 32-bit
/// state by 42,949,673 (2^32 / 100 rounded up), so 0..99.
fn percent(x: u32) -> i32 {
    (x as u64 / 0x028f_5c29) as i32
}

/// The line of a voice record (`FUN_001ae538`): the five (line, chance) byte pairs are walked in
/// order. A pair whose line resolves (through `resolve`: a negative line is -1, except -3 which is
/// the other party's line) and whose chance is positive wins when the percent roll, reduced by
/// each chance passed, goes below zero; pairs with a chance of 0 or less join a pool. When no
/// chance pair won, one pool entry is drawn: `x / ((2^32 + n - 1) / n)` with a fresh generator
/// state `x`, so the draw is the state scaled into `0..n`. -1 when the pool is empty.
///
/// `pairs` are bytes 4..0xe of the record. The generator is stepped once before the roll and
/// once more for the pool draw.
pub fn pick_line(pairs: &[i8; 10], seed: &mut u32, mut resolve: impl FnMut(i8) -> i32) -> i32 {
    let mut roll = percent(rng_next(seed));
    let mut pool = [0i32; 5];
    let mut n = 0usize;
    for k in 0..5 {
        let (line, chance) = (pairs[2 * k], pairs[2 * k + 1]);
        let r = resolve(line);
        if r < 0 {
            continue;
        }
        if chance < 1 {
            pool[n] = r;
            n += 1;
        } else {
            roll -= chance as i32;
            if roll < 0 {
                return r;
            }
        }
    }
    if n == 0 {
        return -1;
    }
    let x = rng_next(seed) as u64;
    let bucket = (1u64 << 32).div_ceil(n as u64);
    pool[(x / bucket) as usize]
}

/// `FUN_001ae6a0`: a line byte below zero is no line, except -3, which depends on the other
/// party.
pub fn resolve_line(line: i8, speaker: u16, other: u16, flags: &impl Fn(u16) -> bool) -> i32 {
    if line >= 0 {
        line as i32
    } else if line == -3 {
        other_party_line(speaker, other, flags)
    } else {
        -1
    }
}

/// NeoStatus values the voice code names (the speaker table at 0x35ee10; index = NeoStatus).
pub mod speaker {
    pub const SORA: u16 = 1;
    pub const DONALD: u16 = 2;
    pub const GOOFY: u16 = 3;
    pub const AURON: u16 = 5;
    pub const ROXAS: u16 = 14;
    pub const PIN: u16 = 15;
}

/// Story flags `FUN_001ae440` tests (through FUN_001802e0, bits of the save data at 0x33f6b0).
/// While all three are clear, lines with Auron (NeoStatus 5) as either party are refused.
pub const AURON_FLAGS: [u16; 3] = [0x184c, 0x180c, 0x182e];

/// `FUN_001ae440`: the line for a -3 entry, from the other party's NeoStatus: -1 when the two
/// parties are the same; -1 for Auron on either side while [`AURON_FLAGS`] are all clear; then
/// Sora 0, Donald 1, Goofy 2, NeoStatus 4..14 (the world partners and Roxas) 3, 15 (`pin`) 4,
/// anything else -1.
pub fn other_party_line(speaker: u16, other: u16, flags: &impl Fn(u16) -> bool) -> i32 {
    if speaker == other {
        return -1;
    }
    if !AURON_FLAGS.iter().any(|&f| flags(f)) && (speaker == speaker::AURON || other == speaker::AURON) {
        return -1;
    }
    let u = other.wrapping_sub(1);
    if u >= 0xf {
        -1
    } else if u > 2 {
        if other == speaker::PIN { 4 } else { 3 }
    } else {
        u as i32
    }
}

/// Minimum volume of a voice line (slot +0x14, set by the actor constructors at 0x1746e0 and
/// 0x174800): the positional volume never goes below it.
pub const MIN_VOLUME: i32 = 0x1800;

/// The per-actor voice slot (actor +0xb08; +8 queued line, +0xc its priority, +0x10 the playing
/// sound, +0x18 the actor): `FUN_001ae798` queues, `FUN_001ae7d8` plays.
#[derive(Clone, Debug, Default)]
pub struct VoiceSlot {
    /// Queued (line, priority); played on the next tick.
    pub pending: Option<(u16, u8)>,
    /// Priority of the line last queued (kept after it starts: a later request must reach it).
    pub priority: u8,
    /// The playing sound's handle in the driver.
    pub playing: Option<u32>,
}

impl VoiceSlot {
    /// `FUN_001ae798`: with nothing queued and nothing playing the request is taken as is;
    /// otherwise only a priority at or above the slot's replaces the queued line.
    pub fn request(&mut self, line: u16, priority: u8) {
        if self.pending.is_none() && self.playing.is_none() {
            self.pending = Some((line, priority));
            self.priority = priority;
            return;
        }
        if (priority as i32) < self.priority as i32 {
            return;
        }
        self.pending = Some((line, priority));
        self.priority = priority;
    }

    /// `FUN_001ae7d8`: the queued line to start now (the caller stops `playing` first), or
    /// nothing. A finished sound clears `playing`.
    pub fn tick(&mut self, still_playing: impl Fn(u32) -> bool) -> Option<u16> {
        if let Some((line, _)) = self.pending.take() {
            return Some(line);
        }
        if let Some(h) = self.playing
            && !still_playing(h)
        {
            self.playing = None;
        }
        None
    }
}

/// Virtual address to file offset in a PS2 ELF (the main executable's load segments).
pub(crate) fn elf_at(exe: &[u8], va: u32) -> Option<usize> {
    let u32_at = |o: usize| exe.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
    let u16_at = |o: usize| exe.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
    if exe.get(..4) != Some(b"\x7fELF") {
        return None;
    }
    let (phoff, phsz, phnum) = (u32_at(0x1c)? as usize, u16_at(0x2a)? as usize, u16_at(0x2c)? as usize);
    for i in 0..phnum {
        let p = phoff + i * phsz;
        if u32_at(p)? != 1 {
            continue;
        }
        let (off, vaddr, filesz) = (u32_at(p + 4)?, u32_at(p + 8)?, u32_at(p + 16)?);
        if va >= vaddr && va < vaddr + filesz {
            return Some((off + va - vaddr) as usize);
        }
    }
    None
}

fn c_string(exe: &[u8], va: u32) -> Option<String> {
    let at = elf_at(exe, va)?;
    let end = exe[at..].iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&exe[at..at + end]).into_owned())
}

/// The speaker name table of the executable (0x35ee10: 16 pointers to names, index = NeoStatus;
/// entry 0 is empty).
#[derive(Clone, Debug, Default)]
pub struct SpeakerNames(pub Vec<Option<String>>);

const SPEAKER_TABLE: u32 = 0x35ee10;
const SPEAKER_COUNT: u32 = 16;
const LANGUAGE_TABLE: u32 = 0x349d44;
/// The index `FUN_00101d68` passes to `FUN_00105cb0` at start-up in this build: entry 2 of the
/// language table, "us". (Entry 1 is "jp"; the Japanese path is never selected by this
/// executable.)
pub const LANGUAGE_INDEX: u32 = 2;

impl SpeakerNames {
    pub fn from_exe(exe: &[u8]) -> Option<SpeakerNames> {
        let mut names = Vec::with_capacity(SPEAKER_COUNT as usize);
        for i in 0..SPEAKER_COUNT {
            let at = elf_at(exe, SPEAKER_TABLE + 4 * i)?;
            let p = u32::from_le_bytes(exe.get(at..at + 4)?.try_into().ok()?);
            names.push(if p == 0 { None } else { c_string(exe, p) });
        }
        (names.get(1).and_then(|n| n.as_deref()) == Some("sora")).then_some(SpeakerNames(names))
    }

    pub fn name(&self, neo_status: u16) -> Option<&str> {
        self.0.get(neo_status as usize)?.as_deref()
    }
}

/// The language folder of the voice (and message) files: the executable's table at 0x349d44
/// indexed by [`LANGUAGE_INDEX`] (FUN_00105cb0 stores the pointer in DAT_00349d40).
pub fn language(exe: &[u8]) -> Option<String> {
    let at = elf_at(exe, LANGUAGE_TABLE + 4 * LANGUAGE_INDEX)?;
    let p = u32::from_le_bytes(exe.get(at..at + 4)?.try_into().ok()?);
    c_string(exe, p)
}

/// Voice bank file of a speaker in an area (`voice/%s/battle/%s%d_%s.vsb`).
pub fn bank_path(lang: &str, world: &str, bank: u16, speaker: &str) -> String {
    format!("voice/{lang}/battle/{world}{bank}_{speaker}.vsb")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_flags(_: u16) -> bool {
        false
    }

    #[test]
    fn chance_pairs_win_in_order_and_the_pool_is_drawn_otherwise() {
        // Line 7 with a 30% chance, then a pool of 1, 2, 3.
        let pairs = [7, 30, 1, 0, 2, 0, 3, 0, -1, 0];
        let (mut won, mut pooled) = (0, [0; 4]);
        let mut seed = 1u32;
        for _ in 0..10_000 {
            let line = pick_line(&pairs, &mut seed, |l| resolve_line(l, 1, 0, &no_flags));
            if line == 7 {
                won += 1;
            } else {
                pooled[line as usize] += 1;
            }
        }
        assert!((2600..3400).contains(&won), "{won}");
        assert_eq!(pooled[0], 0);
        assert!(pooled[1..].iter().all(|&c| c > 1500), "{pooled:?}");
    }

    #[test]
    fn the_draw_matches_the_division_by_the_bucket() {
        // State 0xffff_ffff: roll 99 (no chance pair of 50 wins), pool draw with the next state.
        let pairs = [9, 50, 4, 0, 5, 0, -1, 0, -1, 0];
        let mut seed = 0xffff_ffffu32;
        let s1 = seed.wrapping_mul(RNG_MULTIPLIER);
        let s2 = s1.wrapping_mul(RNG_MULTIPLIER);
        let expect = [4, 5][(s2 as u64 / ((1u64 << 32) / 2 + 0)) as usize];
        let _ = s1;
        // (2^32 + 1) / 2 = 2^31: an even split.
        let line = pick_line(&pairs, &mut seed, |l| resolve_line(l, 1, 0, &no_flags));
        assert_eq!(seed, s2);
        assert!(line == 4 || line == 5);
        assert_eq!(line, expect);
    }

    #[test]
    fn other_party_lines() {
        assert_eq!(other_party_line(1, 2, &no_flags), 1);
        assert_eq!(other_party_line(1, 3, &no_flags), 2);
        assert_eq!(other_party_line(2, 1, &no_flags), 0);
        assert_eq!(other_party_line(1, 4, &no_flags), 3);
        assert_eq!(other_party_line(1, 14, &no_flags), 3);
        assert_eq!(other_party_line(1, 15, &no_flags), 4);
        assert_eq!(other_party_line(1, 16, &no_flags), -1);
        assert_eq!(other_party_line(1, 0, &no_flags), -1);
        assert_eq!(other_party_line(1, 1, &no_flags), -1);
        // Auron is refused until one of the three story flags is set.
        assert_eq!(other_party_line(1, 5, &no_flags), -1);
        assert_eq!(other_party_line(5, 1, &no_flags), -1);
        assert_eq!(other_party_line(1, 5, &|f| f == 0x180c), 3);
    }

    #[test]
    fn slot_queues_by_priority_and_plays_once() {
        let mut s = VoiceSlot::default();
        s.request(3, 5);
        s.request(4, 2);
        assert_eq!(s.pending, Some((3, 5)));
        s.request(6, 5);
        assert_eq!(s.pending, Some((6, 5)));
        assert_eq!(s.tick(|_| true), Some(6));
        s.playing = Some(1);
        assert_eq!(s.tick(|_| true), None);
        assert_eq!(s.playing, Some(1));
        assert_eq!(s.tick(|_| false), None);
        assert_eq!(s.playing, None);
        // Nothing queued and nothing playing: a low priority is taken again.
        s.request(1, 0);
        assert_eq!(s.pending, Some((1, 0)));
    }

    #[test]
    fn bank_path_format() {
        assert_eq!(bank_path("us", "tt", 0, "sora"), "voice/us/battle/tt0_sora.vsb");
    }
}
