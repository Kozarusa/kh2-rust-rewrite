//! Card to `Progress` and back: reading the saves of a PCSX2 card image and writing ones the original loads.
//!
//! Nothing in the running game calls this yet. [`from_env`] is the entry point behind the environment variable `KH2_CARD` (the path of a
//! `.ps2` card image; `KH2_CARD_SLOT` picks a slot, else the save with the latest modify time is taken, as the original's default cursor
//! does); a caller that wants a loaded game calls it with its `Progress`.

use crate::body::{self, BODY_LEN, ICON_LEN, VariantPairs};
use crate::icon::{IconEnv, write_icon_sys};
use crate::image::{Card, DirEntry, Error};
use crate::names::{NameEnv, save_dir_name, slot_of_name};
use crate::slot::{CardInfo, SLOTS};
use kh2_game_core::progress::Progress;

/// One save found on a card.
#[derive(Clone, Debug)]
pub struct CardSave {
    pub slot: usize,
    pub dir: DirEntry,
    /// The body when the three files are there with the right sizes.
    pub body: Option<Vec<u8>>,
    /// Whether the body's header and CRC pass the original's check (`FUN_0023d1c8`).
    pub valid: bool,
}

/// The saves of a card: every root directory named like a save (`NameEnv` gives the pattern).
pub fn saves(card: &Card, env: &NameEnv) -> Result<Vec<CardSave>, Error> {
    let prefix = save_dir_name(env, -1);
    let prefix = &prefix[..prefix.len() - 2];
    let mut out = Vec::new();
    for dir in card.root()? {
        if !dir.is_dir() || !dir.name.starts_with(prefix) {
            continue;
        }
        let Some(slot) = slot_of_name(dir.name.as_bytes()) else { continue };
        let body = card.read_file(&dir.name, &dir.name)?.filter(|b| b.len() == BODY_LEN);
        let valid = body.as_deref().is_some_and(|b| body::is_valid(b, env.region));
        out.push(CardSave { slot, dir, body, valid });
    }
    out.sort_by_key(|s| s.slot);
    Ok(out)
}

/// The card info block for a card: what the original's listing, the directory scans and the body checks leave in it (`place` supplies the
/// place id, `FUN_0019e178`).
pub fn slot_table(card: &Card, env: &NameEnv, place: crate::slot::PlaceFn) -> Result<CardInfo, Error> {
    let mut info = CardInfo::default();
    info.clear_records();
    for s in saves(card, env)? {
        let o = crate::slot::RECORDS + s.slot * crate::slot::RECORD_LEN;
        info.0[o..o + 4].copy_from_slice(&1u32.to_le_bytes());
        info.0[o + 0x18..o + 0x20].copy_from_slice(&s.dir.modified);
        match (&s.body, s.valid) {
            (Some(b), true) => info.describe(s.slot, b, 0x10, place),
            _ => info.describe(s.slot, &[0u8; BODY_LEN], 0x40, place),
        }
    }
    Ok(info)
}

fn hex(b: &[u8]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for &x in b {
        s.push(D[(x >> 4) as usize] as char);
        s.push(D[(x & 15) as usize] as char);
    }
    s
}

/// Loads a save body into `Progress` (as the `save_block` key of a state file does: the whole block, split over what the app keeps apart).
pub fn apply(progress: &mut Progress, body: &[u8]) -> Result<(), String> {
    if body.len() != BODY_LEN {
        return Err(format!("a save body is {BODY_LEN} bytes, not {}", body.len()));
    }
    progress.apply_json(&format!("{{\"save_block\": \"{}\"}}", hex(body)))
}

/// Picks a save: the slot asked for, else the one with the latest modify time.
pub fn pick(saves: &[CardSave], slot: Option<usize>) -> Option<&CardSave> {
    match slot {
        Some(s) => saves.iter().find(|x| x.slot == s),
        None => {
            let mut info = CardInfo::default();
            info.clear_records();
            for s in saves.iter().filter(|s| s.valid) {
                let o = crate::slot::RECORDS + s.slot * crate::slot::RECORD_LEN;
                info.0[o..o + 4].copy_from_slice(&0x10u32.to_le_bytes());
                info.0[o + 0x18..o + 0x20].copy_from_slice(&s.dir.modified);
            }
            let latest = info.latest();
            saves.iter().find(|s| s.slot == latest && s.valid).or_else(|| saves.iter().find(|s| s.valid))
        }
    }
}

/// Loads the card named by `KH2_CARD` into `progress`. Returns `None` when the variable is unset, else the slot loaded or the reason it
/// failed. `KH2_CARD_SLOT` picks the slot.
pub fn from_env(progress: &mut Progress) -> Option<Result<usize, String>> {
    let path = std::env::var("KH2_CARD").ok()?;
    let slot = std::env::var("KH2_CARD_SLOT").ok().and_then(|s| s.trim().parse().ok());
    Some(load_file(progress, std::path::Path::new(&path), &NameEnv::usa(), slot))
}

/// Loads one save of the card file at `path` into `progress`; returns its slot.
pub fn load_file(progress: &mut Progress, path: &std::path::Path, env: &NameEnv, slot: Option<usize>) -> Result<usize, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let card = Card::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let all = saves(&card, env).map_err(|e| e.to_string())?;
    let s = pick(&all, slot).ok_or_else(|| "no save on the card".to_string())?;
    let body = s.body.as_deref().filter(|_| s.valid).ok_or_else(|| format!("slot {} does not hold a valid save", s.slot))?;
    apply(progress, body)?;
    Ok(s.slot)
}

/// What writing a save needs from the executable and the disc.
pub struct WriteEnv {
    pub names: NameEnv,
    pub icon: IconEnv,
    /// The icon variant flag pairs (`0x37f640`).
    pub variants: VariantPairs,
    /// The `kh2.ico` file (0x8a58 bytes).
    pub model: Vec<u8>,
}

/// Writes the live save block (`Progress::save_image`) into `slot` of the card as the original saves it: the directory with `kh2.ico`,
/// `icon.sys` and the body, replacing a save in that slot. `stamp` is the modify time.
pub fn write_save(card: &mut Card, env: &WriteEnv, slot: usize, live: &[u8], stamp: [u8; 8]) -> Result<(), String> {
    if slot >= SLOTS {
        return Err(format!("slot {slot} is not below {SLOTS}"));
    }
    if env.model.len() != ICON_LEN {
        return Err(format!("kh2.ico is {ICON_LEN} bytes, not {}", env.model.len()));
    }
    let body = body::export(live, &env.variants).ok_or("no icon variant applies to this save block")?;
    let icon_sys = write_icon_sys(&env.icon, slot as i32, live);
    let name = save_dir_name(&env.names, slot as i64);
    card.write_dir(&name, &[("kh2.ico", &env.model), ("icon.sys", &icon_sys), (&name, &body)], stamp).map_err(|e| e.to_string())
}
