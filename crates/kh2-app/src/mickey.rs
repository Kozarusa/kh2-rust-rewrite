//! Mickey's rescue, the part of it the original decides in the game over (docs/menus.md 26.5).
//!
//! What the original does (`SLUS_210.05`). Mickey is not a summon and not a script: he is a **player-class object**
//! (class table 0x360360, base constructor `FUN_00176e00(.., 10, ..)`, the member index 9 of docs/party.md) made by
//! `FUN_001cdf48`, whose only caller is step 7 of the game-over task (`FUN_001ad9a0`). Step 7 is reached from the callback
//! `0x1adbb8` of the **alternate list** (menu record 10, "I won't give up!" code 9 / "It's all over..." code 8). The
//! alternate list replaces the normal Continue / Load Game list when the game over is built (`FUN_001ad378`):
//! * the type is 0 (the field game over) and the area's mission manager has header word `+4` bit 9 (`FUN_001b9aa0`), and
//! * `FUN_001ad520`: neither of the two party slots (`FUN_0017cd08(0..1)`) holds an ally that exists and answers
//!   `FUN_0017d030 == 0`, and
//! * a roll of the shared generator (`DAT_003716a0 = DAT_003716a0 * 0x10dcd`, as an unsigned number times 2^-32) is below
//!   the chance `max(sstm[0xdc], sstm[0xd8] ^ saves)`, `saves` being the word `0x34161c`.
//!
//! `0x34161c` and `0x34161e` (report row 4) are written only by the object's constructors `FUN_001cda78` / `FUN_001cdb98`
//! (+1, up to 999 each; an instruction scan of the save-body offsets `0x2dbc` and `0x2dbe` finds no other writer), so they
//! always hold the same number; the port keeps one ([`Progress::mickey_save_count`]).
//!
//! Code 8 ("It's all over...") loads the scene `obj/GAMEOVER.mset` and opens the normal list (record 1), as the game over
//! does without the alternate. Code 9 ("I won't give up!") plays the music back and makes Mickey: his constructor counts
//! the rescue, sets his drive gauge maximum to 3 (status `+0x1b2`), and `FUN_001cdf48` places him at Sora, starts Sora's
//! motion 0xfc from the shared motion table, starts system effect 0x11 on Sora (`FUN_001db868` on `DAT_01c954d8`), and
//! hands every object's target `+0x9f4` that was Sora to Mickey (`FUN_0018f188`).
//!
//! The port takes the decision and the counter. The object (a second player class with its own command handler
//! `FUN_001ce518`, tick `FUN_001ce240` and command menu), the amount Sora is healed to and what the effect does are not
//! ported (see docs). Eleven missions of the disc set header bit 9 ([`BIT9_MISSIONS`], pinned by a test), so the original
//! offers the rescue there; the app keeps the offer off while [`RESCUE_PORTED`] is false (the rescue would stand Sora up
//! with a heal the original's code was not read for), and only the harness aid `KH2_MICKEY_OFFER=1` shows it.

use kh2_tables::player::Sstm;

/// Whether Mickey's rescue is ported in full (the Mickey object, Sora's heal, motion 0xfc, effect 0x11). While false the
/// app never offers the alternate list in normal play: a rescue with an invented heal would break the original's fidelity
/// (RULES A4). Only `KH2_MICKEY_OFFER` (the harness) bypasses it.
pub const RESCUE_PORTED: bool = false;

/// The missions of the disc (`msn/us/<name>.bar`, header word `+4`) whose bit 9 is set: the original can offer the rescue
/// in these. Pinned by `the_disc_has_eleven_missions_with_header_bit_9`.
#[cfg(test)]
pub const BIT9_MISSIONS: [&str; 11] = [
    "AL03_MS103", "BB05_MS104A", "BB05_MS104B", "BB11_MS102", "BB15_MS203", "CA18_MS202", "EH24_MS110", "HE07_MS103", "HE18_MS108",
    "MU08_MS204", "MU09_MS108",
];

/// `FUN_001b9aa0`: the mission manager's header word `+4` bit 9.
pub const HEADER_BIT: u16 = 0x200;
/// The menu record `FUN_001ad9a0` step 5 opens when the alternate byte is set (`FUN_001bef10(10)`).
pub const RECORD: u16 = 10;
/// "I won't give up!": the callback `0x1adbb8` sets step 7, which makes Mickey.
pub const CODE_RESCUE: u16 = 9;
/// "It's all over...": the callback opens the normal list.
pub const CODE_GIVE_UP: u16 = 8;
/// The multiplier of the shared generator (`FUN_001ad520`).
pub const LCG_MUL: u32 = 0x10dcd;
/// The scale the roll is multiplied by (the float at `0x37b84c`: 2^-32).
pub const ROLL_SCALE: f32 = 1.0 / 4_294_967_296.0;
/// The generator's value after the reset to 1 (`FUN_002f5fc8`).
pub const SEED_START: u32 = 1;

/// The `sstm` fields `FUN_001ad520` reads through `DAT_00360130`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chance {
    /// `sstm[0xd8]`: the base raised to the number of earlier rescues.
    pub base: f32,
    /// `sstm[0xdc]`: the least chance.
    pub floor: f32,
}

impl Chance {
    pub fn from_sstm(s: &Sstm) -> Chance {
        Chance { base: s.f32(0xd8), floor: s.f32(0xdc) }
    }

    /// The chance after `saves` rescues: the least of the two, replaced by the power when that is larger.
    pub fn after(self, saves: u16) -> f32 {
        let p = self.base.powf(f32::from(saves));
        if self.floor < p { p } else { self.floor }
    }
}

/// One step of the shared generator.
pub fn next_seed(seed: u32) -> u32 {
    seed.wrapping_mul(LCG_MUL)
}

/// The seed as the roll the original compares: an unsigned number as a float, times 2^-32.
pub fn roll(seed: u32) -> f32 {
    seed as f32 * ROLL_SCALE
}

/// What `FUN_001ad378` knows when it builds the game over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offer {
    /// The game over type (only 0 asks).
    pub kind: u8,
    /// The mission manager's header word `+4` has bit 9 (`FUN_001b9aa0`).
    pub manager_bit9: bool,
    /// Per party slot: an ally that exists and answers `FUN_0017d030 == 0` [inferred: the app's member that is not knocked out].
    pub ally_up: [bool; 2],
    pub chance: Chance,
    /// `0x34161c`.
    pub saves: u16,
}

/// `FUN_001ad378` + `FUN_001ad520`: does the game over become the alternate one? Returns the answer and the generator's
/// next seed (it is stepped only when the roll is made: not for another type, not without the bit, not with an ally up).
pub fn offered(o: Offer, seed: u32) -> (bool, u32) {
    if o.kind != 0 || !o.manager_bit9 || o.ally_up.iter().any(|&up| up) {
        return (false, seed);
    }
    let next = next_seed(seed);
    (roll(next) < o.chance.after(o.saves), next)
}

/// `KH2_MICKEY_OFFER=1` (a harness aid): the game over type 0 takes the alternate list whatever the manager and the allies
/// are, and the roll is skipped. The original has no such switch.
pub fn forced() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_MICKEY_OFFER").is_ok_and(|v| v != "0"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANCE: Chance = Chance { base: 0.5, floor: 0.25 };

    fn offer(f: impl FnOnce(&mut Offer)) -> Offer {
        let mut o = Offer { kind: 0, manager_bit9: true, ally_up: [false, false], chance: CHANCE, saves: 0 };
        f(&mut o);
        o
    }

    #[test]
    fn the_chance_falls_with_the_saves_to_the_floor() {
        assert_eq!(CHANCE.after(0), 1.0);
        assert_eq!(CHANCE.after(1), 0.5);
        assert_eq!(CHANCE.after(2), 0.25);
        // Below the floor the floor stays (the original replaces the floor only when the power is larger).
        assert_eq!(CHANCE.after(3), 0.25);
        assert_eq!(CHANCE.after(999), 0.25);
    }

    #[test]
    fn the_generator_is_the_games_and_the_roll_is_unsigned() {
        assert_eq!(next_seed(SEED_START), 0x10dcd);
        assert_eq!(next_seed(0x1_0000), 0x10dcd_u32.wrapping_mul(0x1_0000), "the product wraps at 32 bits");
        assert_eq!(roll(0), 0.0);
        assert_eq!(roll(0x8000_0000), 0.5, "a seed with the top bit set is a large positive number, not negative");
        assert!(roll(u32::MAX) <= 1.0);
    }

    #[test]
    fn only_the_field_game_over_with_the_bit_and_no_ally_up_rolls() {
        let seed = 7;
        assert_eq!(offered(offer(|o| o.kind = 3), seed), (false, seed), "type 3 never asks");
        assert_eq!(offered(offer(|o| o.kind = 1), seed), (false, seed));
        assert_eq!(offered(offer(|o| o.manager_bit9 = false), seed), (false, seed), "no header bit 9: no roll");
        assert_eq!(offered(offer(|o| o.ally_up = [false, true]), seed), (false, seed), "Goofy up: no roll");
        assert_eq!(offered(offer(|o| o.ally_up = [true, false]), seed), (false, seed), "Donald up: no roll");
        // With the roll made the seed steps once.
        let (_, next) = offered(offer(|_| {}), seed);
        assert_eq!(next, next_seed(seed));
    }

    #[test]
    fn the_roll_against_the_chance_decides() {
        // First rescue: the chance is 1, every roll below 1 passes.
        let (yes, _) = offered(offer(|_| {}), SEED_START);
        assert!(yes);
        // After many rescues the chance is the floor 0.25: the roll of 0x10dcd * 2^-32 is tiny and passes; a seed whose next
        // value is above 0.25 * 2^32 does not.
        let low = offer(|o| o.saves = 50);
        assert!(offered(low, SEED_START).0, "roll {} is below 0.25", roll(next_seed(SEED_START)));
        // A seed s with next_seed(s) = 0xc000_0000 (roll 0.75): 0x10dcd is odd, so it has an inverse modulo 2^32.
        let inv = (0..32).fold(1u32, |x, _| x.wrapping_mul(2u32.wrapping_sub(LCG_MUL.wrapping_mul(x))));
        assert_eq!(LCG_MUL.wrapping_mul(inv), 1);
        let s = 0xc000_0000u32.wrapping_mul(inv);
        assert_eq!(next_seed(s), 0xc000_0000);
        assert!(!offered(low, s).0, "roll 0.75 is above the chance 0.25");
        assert!(offered(offer(|o| o.saves = 0), s).0, "the same roll passes while the chance is 1");
    }

    /// Reads every `msn/us/*.bar` header (BAR entry type 2, flags word at +4) and pins the set with bit 9.
    #[test]
    fn the_disc_has_eleven_missions_with_header_bit_9() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let list = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/kh2idx.txt")).unwrap_or_default();
        let names: Vec<&str> = list.lines().filter(|l| l.starts_with("msn/us/") && l.ends_with(".bar")).collect();
        if names.is_empty() {
            eprintln!("skipped: local/kh2idx.txt is missing");
            return;
        }
        let disc = crate::assets::Disc::open(std::path::Path::new(&iso));
        let mut found = Vec::new();
        let mut read = 0;
        for n in &names {
            let Some(file) = disc.read(n) else { continue };
            let Ok(entries) = kh2_bar::parse(&file) else { continue };
            let Some(h) = entries.iter().find(|e| e.kind == 2) else { continue };
            let Some(f) = h.data.get(4..6).map(|b| u16::from_le_bytes([b[0], b[1]])) else { continue };
            read += 1;
            if f & HEADER_BIT != 0 {
                found.push(n.trim_start_matches("msn/us/").trim_end_matches(".bar").to_string());
            }
        }
        found.sort();
        let mut want: Vec<String> = BIT9_MISSIONS.iter().map(|s| s.to_string()).collect();
        want.sort();
        eprintln!("{read} mission headers read, bit 9 in {found:?}");
        assert!(read > 250, "most mission files are read: {read}");
        assert_eq!(found, want);
    }

    #[test]
    fn the_chance_reads_the_disc_when_there_is_one() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(std::path::Path::new(&iso)) else { return };
        let s = kh2_tables::player::load_sstm(&mut a).expect("sstm");
        let c = Chance::from_sstm(&s);
        eprintln!("sstm[0xd8] = {} sstm[0xdc] = {}", c.base, c.floor);
        assert!(c.base > 0.0 && c.base <= 1.0, "a base that makes the chance fall: {}", c.base);
        assert!((0.0..=1.0).contains(&c.floor));
        assert_eq!(c.after(0), 1.0);
    }
}
