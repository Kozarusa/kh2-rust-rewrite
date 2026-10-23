//! The area-script command interpreter on the original's layout: `FUN_00180b48` (one command),
//! `FUN_00180ad8` (a run of commands) and the area-start defaults of `FUN_00181148`, plus the
//! five-slot lottery behind opcode 0x1f (`FUN_001d7970`). docs/systems/core-runtime/areas.md section 1.
//!
//! A command is a run of `u32` words: word 0 holds the opcode (low halfword) and the argument count
//! (high halfword, signed); the arguments follow. The interpreter reads the words through a slice that
//! starts at the command and may run past its end (opcode 2 indexes with a computed number, the game
//! reads whatever lies there), so the slice should be the rest of the program.
//!
//! Everything the commands do besides storing into the script variables goes through [`ScriptHost`];
//! [`crate::run::ScriptRun`] implements it by recording, the oracle test (`tests/orig_180b48.rs`)
//! by logging the calls the original makes.

/// The game's script variables that commands write, with the addresses they live at in the USA
/// executable. `Default` is the state `FUN_00181148` sets before the scripts run.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptVars {
    /// `DAT_0035cd10`: opcode 1; -1 at the start.
    pub map_visibility: u64,
    /// `DAT_0035cd18`: opcode 0xb; 0 at the start.
    pub word_35cd18: u64,
    /// `DAT_0035cd20`: opcode 5; 0 at the start (0x280000 after the scripts when still 0).
    pub heap_size: u32,
    /// `DAT_0035cd24`: opcode 0x12; 0 at the start (0x100000 after the scripts when still 0).
    pub heap_base: u32,
    /// `DAT_0035cd28`: opcode 6; 3 at the start.
    pub cache_a: u32,
    /// `DAT_0035cd2c`: opcode 7; 4 at the start.
    pub cache_b: u32,
    /// `DAT_0035cd30`: opcode 0xe; 0 at the start.
    pub word_35cd30: u32,
    /// `DAT_0035cd34`: opcode 0x11; -1 at the start.
    pub word_35cd34: u32,
    /// `DAT_0035cd38`: opcode 0x13; 0 at the start.
    pub camera_type: u32,
    /// `DAT_0035cd3c`, `DAT_0035cd40`: opcode 0x18; 0 at the start (0x80000 and 0x20000 after the scripts when still 0).
    pub effect_heap: [u32; 2],
    /// `DAT_0035d268`: opcodes 0x14 (bit 2) and 0x17 (bit 4) set bits; not reset by the area start.
    pub flags_35d268: u32,
    /// `DAT_01c893b8`: opcode 4 (a `u32` converted to `f32`); 10000.0 (`0x37a868`) at the start.
    pub capacity: f32,
    /// `DAT_01c893c0`, `DAT_01c893c8`: opcodes 0x1a and 0x1b OR in `1 << argument`; 0 at the start.
    pub show: u64,
    pub hide: u64,
    /// `DAT_01c893d0`: opcode 0x1e; 0 at the start.
    pub level: u32,
    /// The game's linear congruential word `DAT_003716a0` (opcodes 2, 3 and the lottery multiply it by 0x10dcd).
    pub lcg: u32,
}

impl Default for ScriptVars {
    fn default() -> Self {
        ScriptVars {
            map_visibility: u64::MAX,
            word_35cd18: 0,
            heap_size: 0,
            heap_base: 0,
            cache_a: 3,
            cache_b: 4,
            word_35cd30: 0,
            word_35cd34: u32::MAX,
            camera_type: 0,
            effect_heap: [0, 0],
            flags_35d268: 0,
            capacity: 10000.0,
            show: 0,
            hide: 0,
            level: 0,
            lcg: 0,
        }
    }
}

impl ScriptVars {
    /// The end of `FUN_00181148` after the three scripts: the sizes still 0 take their defaults, the
    /// two cache counts become 0 when the map program byte is above 0x32, and the map word is
    /// `(word | show) & !hide`.
    pub fn finish(&mut self, map_program: u8) {
        if self.heap_size == 0 {
            self.heap_size = 0x28_0000;
        }
        if self.heap_base == 0 {
            self.heap_base = 0x10_0000;
        }
        if self.effect_heap[0] == 0 {
            self.effect_heap[0] = 0x8_0000;
        }
        if self.effect_heap[1] == 0 {
            self.effect_heap[1] = 0x2_0000;
        }
        if map_program > 0x32 {
            self.cache_a = 0;
            self.cache_b = 0;
        }
        self.map_visibility = (self.map_visibility | self.show) & !self.hide;
    }
}

/// What commands ask of the rest of the game. Each method is one call the original makes.
pub trait ScriptHost {
    /// `FUN_00181728(tag, alt)`: loads the spawn groups of the area file's type-12 entry named `tag`
    /// (four characters in one word); `alt` is opcode 9's second flag.
    fn load_group(&mut self, tag: u32, alt: bool);
    /// Opcode 0xa: the area file's entry with this index (`FUN_001684c0(file, -1, index)`); when it exists
    /// and its size word is not 0 the game hands it to `FUN_001817f0`.
    fn entry_by_index(&mut self, index: u32);
    /// Opcode 0xc (`FUN_00180ff8`): `arguments` are the command's argument words (key, then the sub-commands).
    fn define_trigger(&mut self, arguments: &[u32]);
    /// Opcode 0xf: `FUN_001a7330(current world record, argument)`.
    fn world_record(&mut self, argument: u32);
    /// Opcode 0x10: `FUN_001a7700` stores the word in `DAT_01c95638` (field music low, battle music high).
    fn music(&mut self, word: u32);
    /// Opcode 0x15: `FUN_001b9338(kind, rest)`.
    fn battle_manager(&mut self, kind: u32, rest: &[u32]);
    /// Opcode 0x16: `FUN_00311c08(0x1c893d8, arguments, language)` formats the arguments (a printf format
    /// string held in the words) with the language code, then `FUN_0016fac8` requests that file (buffer id 0x38e).
    fn load_file(&mut self, format: &[u32]);
    /// Opcode 0x19: `FUN_001802e0(flag)`.
    fn flag_is_set(&mut self, flag: u32) -> bool;
    /// Opcode 0x19: `FUN_0017ff60(flag)`.
    fn set_flag(&mut self, flag: u32);
    /// Opcode 0x1d: `FUN_001abfe8(kind)`.
    fn party_reset(&mut self, kind: u32);
    /// Opcode 0x1f: `FUN_001d7970(0x33e830)`, the area lottery. It may use [`ScriptVars::lcg`].
    fn lottery(&mut self, vars: &mut ScriptVars) -> bool;
}

/// `FUN_00325b38`: the unsigned 64-bit quotient (the compiler's `__udivdi3`). The original breaks
/// (trap 7) on a zero divisor; no script reaches it, this returns 0.
pub fn udiv64(a: u64, b: u64) -> u64 {
    a.checked_div(b).unwrap_or(0)
}

/// The word `index` of `w`, 0 past its end.
fn word(w: &[u32], index: usize) -> u32 {
    w.get(index).copied().unwrap_or(0)
}

/// The argument count of the command at the start of `w` (the signed high halfword of word 0).
fn count(w: &[u32]) -> i32 {
    (word(w, 0) >> 16) as u16 as i16 as i32
}

/// The argument words of the command at the start of `w`, clipped to the slice.
fn args(w: &[u32]) -> &[u32] {
    let n = count(w).max(0) as usize;
    w.get(1..(1 + n).min(w.len())).unwrap_or_default()
}

/// orig 0x180b48
///
/// Runs one command (the words starting at `w[0]`); `false` is the game's answer 0 that stops the
/// program. `entrance` is the byte `0x33e830 + 2` that opcode 0x1c compares with.
///
/// Opcodes 8 and 0xd and everything above 0x1f do nothing and answer 1 (the jump table's default).
/// Opcode 2 with a count of 0 or less would divide by zero in the game (break 7): here it does nothing.
pub fn command(host: &mut dyn ScriptHost, v: &mut ScriptVars, w: &[u32], entrance: u8) -> bool {
    let op = word(w, 0) as u16 as i16;
    let n = count(w);
    let a = |k: usize| word(w, 1 + k);
    match op {
        0 => {
            for k in 0..n.max(0) as usize {
                host.load_group(a(k), false);
            }
        }
        1 => v.map_visibility = a(0) as u64 | (a(1) as u64) << 32,
        2 => {
            v.lcg = v.lcg.wrapping_mul(0x10dcd);
            if n <= 0 {
                return true;
            }
            // FUN_00325b38((n - 1) + 2^32, n) is the step ceil(2^32 / n); the roll divided by it picks the argument.
            let step = udiv64((n as i64 - 1) as u64 + (1 << 32), n as i64 as u64);
            let k = udiv64(v.lcg as u64, step) as i32;
            host.load_group(word(w, (k as i64 + 1) as usize), false);
            return true;
        }
        3 => {
            v.lcg = v.lcg.wrapping_mul(0x10dcd);
            let roll = udiv64(v.lcg as u64, 0x28f_5c29) as u32;
            if a(0) <= roll {
                return true;
            }
            host.load_group(a(1), false);
            return false;
        }
        4 => v.capacity = a(0) as f32,
        5 => v.heap_size = a(0),
        6 => v.cache_a = a(0),
        7 => v.cache_b = a(0),
        9 => {
            host.load_group(a(0), true);
            return true;
        }
        0xa => {
            host.entry_by_index(a(0));
            return true;
        }
        0xb => v.word_35cd18 = a(0) as u64 | (a(1) as u64) << 32,
        0xc => {
            host.define_trigger(args(w));
            return true;
        }
        0xe => v.word_35cd30 = a(0),
        0xf => {
            host.world_record(a(0));
            return true;
        }
        0x10 => {
            host.music(a(0));
            return true;
        }
        0x11 => v.word_35cd34 = a(0),
        0x12 => v.heap_base = a(0),
        0x13 => v.camera_type = a(0),
        0x14 => v.flags_35d268 |= 4,
        0x15 => {
            host.battle_manager(a(0), w.get(2..(1 + n.max(0) as usize).max(2).min(w.len())).unwrap_or_default());
            return true;
        }
        0x16 => {
            host.load_file(args(w));
            return true;
        }
        0x17 => v.flags_35d268 |= 0x10,
        0x18 => {
            v.effect_heap = [a(0), a(1)];
        }
        0x19 => {
            for k in bit_loop(w) {
                if !host.flag_is_set(a(k)) {
                    host.set_flag(a(k));
                }
            }
        }
        0x1a => {
            for k in bit_loop(w) {
                v.show |= 1u64.wrapping_shl(a(k));
            }
        }
        0x1b => {
            for k in bit_loop(w) {
                v.hide |= 1u64.wrapping_shl(a(k));
            }
        }
        0x1c => {
            if a(0) == entrance as u32 {
                // The nested commands are the words after the entrance argument, up to the end of this command.
                let end = (1 + n as i64).max(2) as usize;
                if let Some(nested) = w.get(2..) {
                    run_program(host, v, nested, end - 2, entrance);
                }
            }
            return true;
        }
        0x1d => {
            for k in 0..n.max(0) as usize {
                host.party_reset(a(k));
            }
        }
        0x1e => v.level = a(0),
        0x1f => {
            if !host.lottery(v) {
                return true;
            }
            host.load_group(a(0), false);
            return false;
        }
        _ => {}
    }
    true
}

/// orig 0x180ad8
///
/// Runs the commands in `w` in order until one answers "stop" or the first `end` words are used up (a command may read past `end`); each next command
/// starts `count * 4 + 4` bytes after the previous one.
pub fn run_program(host: &mut dyn ScriptHost, v: &mut ScriptVars, w: &[u32], end: usize, entrance: u8) {
    let mut at = 0usize;
    while at < end.min(w.len()) {
        if !command(host, v, &w[at..], entrance) {
            return;
        }
        let step = 1 + count(&w[at..]) as i64;
        let next = at as i64 + step;
        if next <= at as i64 {
            // A count of -1 or less would walk backwards for ever in the game; no script has one.
            return;
        }
        at = next as usize;
    }
}

/// The words of a list of parsed commands, as the game holds them in memory.
pub fn words_of(commands: &[crate::Command]) -> Vec<u32> {
    let mut out = Vec::new();
    for c in commands {
        out.push(c.opcode as u32 | (c.args.len() as u32) << 16);
        out.extend_from_slice(&c.args);
    }
    out
}

/// The five-slot lottery behind opcode 0x1f (`FUN_001d7970`): one byte per slot at `0x1d6a9c0`
/// (0 open, 1 used, 2 the winner). `table` is the five `(world, area)` byte pairs at `0x37c5f8`
/// (read from the executable at run time), `area` the first two bytes of `0x33e830`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Lottery {
    pub slots: [u8; 5],
}

impl Lottery {
    /// orig 0x1d7928: the index of the table pair equal to `area`, 0 when none matches.
    pub fn index_of(table: &[[u8; 2]; 5], area: [u8; 2]) -> usize {
        table.iter().position(|p| *p == area).unwrap_or(0)
    }

    /// orig 0x1d7a98: whether all five slots are 1.
    pub fn all_used(slots: &[u8; 5]) -> bool {
        slots.iter().all(|&s| s == 1)
    }

    /// orig 0x1d7848 (`FUN_001d7908` passes the index `i`): clears the slots and makes one the winner. A negative `i`
    /// picks `lcg / 0x33333334` (the multiply first); otherwise the high word of `lcg << 2`, plus one when it is not
    /// above `i`, so the winner is never `i` itself.
    pub fn reset(&mut self, i: i64, lcg: &mut u32) {
        self.slots = [0; 5];
        *lcg = lcg.wrapping_mul(0x10dcd);
        let pick = if i < 0 {
            udiv64(*lcg as u64, 0x3333_3334) as i32 as i64
        } else {
            let hi = (((*lcg as u64) << 2) as i64) >> 32;
            if i <= hi { hi + 1 } else { hi }
        };
        if let Some(s) = self.slots.get_mut(pick as usize) {
            *s = 2;
        }
    }

    /// orig 0x1d7970: marks slot `index_of(table, area)` used and answers whether it was the winner; one of
    /// the still-open slots becomes the new winner (the multiply picks it); when all five are used the lottery restarts.
    pub fn roll(&mut self, table: &[[u8; 2]; 5], area: [u8; 2], lcg: &mut u32) -> bool {
        let i = Self::index_of(table, area);
        let old = self.slots[i];
        self.slots[i] = 1;
        let open: Vec<usize> = (0..5).filter(|&k| self.slots[k] == 0).collect();
        if !open.is_empty() {
            *lcg = lcg.wrapping_mul(0x10dcd);
            let n = open.len() as u64;
            let step = udiv64((n - 1) + (1 << 32), n);
            let k = udiv64(*lcg as u64, step) as i32;
            if let Some(&slot) = open.get(k as usize) {
                self.slots[slot] = 2;
            }
        }
        if Self::all_used(&self.slots) {
            self.reset(i as i64, lcg);
        }
        old == 2
    }
}

/// The argument numbers opcodes 0x19, 0x1a and 0x1b visit. The original's loop re-reads its bound after each argument from the
/// halfword at command offset `4 * (k + 1) + 2` (the high half of argument `k`, not the header's count): it takes argument 0 when the
/// count is positive, then goes on to argument `k + 1` while `k + 1` is below the signed high half of argument `k`. With story flags
/// and bit numbers (all below 0x10000) only argument 0 is ever taken. [V by the oracle test against the executable]
fn bit_loop(w: &[u32]) -> Vec<usize> {
    let mut out = Vec::new();
    if count(w) <= 0 {
        return out;
    }
    let mut k = 0usize;
    loop {
        out.push(k);
        let bound = (word(w, 1 + k) >> 16) as u16 as i16 as i64;
        k += 1;
        if (k as i64) >= bound {
            return out;
        }
    }
}

/// The file name opcode 0x16 requests: `FUN_00311c08` is the C `sprintf` (the format held in the command's argument words, a
/// NUL-terminated string, the language code as the one conversion argument), so `%s` becomes `language` and `%%` a percent sign.
/// The area files on the USA disc use only `%s` (`file/%s/<name>.2ld`, `menu/%s/jiminy.bar`) or no conversion (`minigame/<name>.bar`).
/// The buffer is 0x1c893d8; `FUN_0016fac8(buffer, 0x38e, 0)` then requests the file (the sprintf is SDK code and not ported as such).
pub fn file_name(format: &[u32], language: &str) -> String {
    let bytes: Vec<u8> = format.iter().flat_map(|w| w.to_le_bytes()).take_while(|&b| b != 0).collect();
    let text = String::from_utf8_lossy(&bytes);
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push_str(language),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<u32> {
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        while b.len() % 4 != 0 {
            b.push(0);
        }
        b.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
    }

    #[test]
    fn file_names_take_the_language_code() {
        assert_eq!(file_name(&words("file/%s/tt_pt.2ld"), "us"), "file/us/tt_pt.2ld");
        assert_eq!(file_name(&words("minigame/al_14.bar"), "us"), "minigame/al_14.bar");
        assert_eq!(file_name(&words("100%%"), "fr"), "100%");
    }

    #[derive(Default)]
    struct Rec(Vec<String>);
    impl ScriptHost for Rec {
        fn load_group(&mut self, tag: u32, alt: bool) {
            self.0.push(format!("group {tag:x} {alt}"));
        }
        fn entry_by_index(&mut self, i: u32) {
            self.0.push(format!("entry {i}"));
        }
        fn define_trigger(&mut self, _: &[u32]) {}
        fn world_record(&mut self, _: u32) {}
        fn music(&mut self, _: u32) {}
        fn battle_manager(&mut self, _: u32, _: &[u32]) {}
        fn load_file(&mut self, f: &[u32]) {
            self.0.push(file_name(f, "us"));
        }
        fn flag_is_set(&mut self, _: u32) -> bool {
            false
        }
        fn set_flag(&mut self, f: u32) {
            self.0.push(format!("flag {f}"));
        }
        fn party_reset(&mut self, _: u32) {}
        fn lottery(&mut self, _: &mut ScriptVars) -> bool {
            true
        }
    }

    fn run(w: &[u32]) -> (Rec, ScriptVars, bool) {
        let (mut h, mut v) = (Rec::default(), ScriptVars::default());
        let more = command(&mut h, &mut v, w, 1);
        (h, v, more)
    }

    #[test]
    fn load_file_requests_the_formatted_name() {
        let mut w = vec![0x16 | (words("file/%s/he_ft.2ld").len() as u32) << 16];
        w.extend(words("file/%s/he_ft.2ld"));
        let (h, _, more) = run(&w);
        assert!(more);
        assert_eq!(h.0, ["file/us/he_ft.2ld"]);
    }

    #[test]
    fn only_the_first_story_flag_is_set_when_the_arguments_are_small() {
        // The loop bound is re-read from the high half of the argument just taken (see `bit_loop`).
        let (h, _, _) = run(&[0x19 | 3 << 16, 0x805, 0x806, 0x807]);
        assert_eq!(h.0, ["flag 2053"]);
        // A high half above the position keeps it going: argument 0 = 0x0003_0805 takes argument 1 and 2 as well.
        let (h, _, _) = run(&[0x19 | 3 << 16, 0x0003_0805, 0x0003_0806, 0x0001_0807]);
        assert_eq!(h.0.len(), 3);
    }

    #[test]
    fn the_lottery_stop_loads_the_group_and_stops() {
        let (h, _, more) = run(&[0x1f | 1 << 16, u32::from_le_bytes(*b"m_00")]);
        assert!(!more);
        assert_eq!(h.0.len(), 1);
    }

    #[test]
    fn opcode_two_draws_with_the_zero_extended_random_word() {
        let mut h = Rec::default();
        let mut v = ScriptVars { lcg: 0x1234_5678, ..ScriptVars::default() };
        command(&mut h, &mut v, &[2 | 3 << 16, 0xa, 0xb, 0xc], 0);
        assert_eq!(v.lcg, 0x1234_5678u32.wrapping_mul(0x10dcd));
        assert_eq!(h.0.len(), 1);
    }
}
