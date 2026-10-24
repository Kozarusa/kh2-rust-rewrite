//! File and directory names of a save (`FUN_0023cb90`, `FUN_0023cc38`, `FUN_0023cc98`).
//!
//! A save is the directory `<prefix><part0>-<part1><part2>-<NN>` holding `icon.sys`, `kh2.ico` and a file of the same name as the
//! directory. For the USA disc that is `BASLUS-21005-00` .. `BASLUS-21005-98` (confirmed on a PCSX2 card).

/// What the name routines read from the executable: the region word (`0x349d44`, 2 in a running USA game) and the product code parts
/// (three strings at `0x349d5c + region * 12`, `FUN_00105d10`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameEnv {
    pub region: u32,
    pub parts: [String; 3],
}

impl NameEnv {
    /// The USA disc's names. The parts are the product code `SLUS-21005` as the region table holds it (`SLUS`, `210`, `05`), read from
    /// the executable by the oracle test and equal to what the real card holds.
    pub fn usa() -> NameEnv {
        NameEnv { region: 2, parts: ["SLUS".into(), "210".into(), "05".into()] }
    }
}

/// The two-letter prefix by region: 1 `BI`, 2 `BA`, anything else `BE`.
///
/// orig 0x23cc38
pub fn region_prefix(region: u32) -> &'static str {
    match region {
        1 => "BI",
        2 => "BA",
        _ => "BE",
    }
}

/// The directory name of slot `n` (`%s%s-%s%s-%02d`), or the listing pattern with `??` for a negative `n`.
///
/// orig 0x23cb90
pub fn save_dir_name(env: &NameEnv, n: i64) -> String {
    let head = format!("{}{}-{}{}", region_prefix(env.region), env.parts[0], env.parts[1], env.parts[2]);
    if n < 0 { format!("{head}-??") } else { format!("{head}-{n:02}") }
}

/// The name for a selector: -5 keeps `current`, -4 is `*`, -3 `icon.sys`, -2 `kh2.ico`, anything else a save directory name.
///
/// orig 0x23cc98
pub fn name_for(env: &NameEnv, n: i64, current: &str) -> String {
    match n {
        -5 => current.to_string(),
        -4 => "*".to_string(),
        -3 => "icon.sys".to_string(),
        -2 => "kh2.ico".to_string(),
        _ => save_dir_name(env, n),
    }
}

/// The slot number a directory name stands for: 15 characters with a digit at index 13 give `name[13] * 10 + name[14] - 0x210`, as in
/// the scan at `0x23d578` (index 14 is not checked to be a digit there).
pub fn slot_of_name(name: &[u8]) -> Option<usize> {
    if name.len() != 15 || name[13].wrapping_sub(0x30) >= 10 {
        return None;
    }
    let s = i32::from(name[13]) * 10 + i32::from(name[14]) - 0x210;
    (0..99).contains(&s).then_some(s as usize)
}
