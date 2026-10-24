//! Player-system data: drive forms, limits, summons, abilities and the engine tuning block.
//!
//! Typed views over tables that `sheets/tables.json` already generates (`fmlv`, `item`, `cmd`,
//! `limt`, `sumn`) plus decoders for tables the sheet does not model yet (`rcct`, `memt` and the
//! full `sstm` record). No game values live here: everything is read from the disc at run time.
//! How each rule was found is written up in `docs/systems/combat-effects/player-data.md`.

use std::collections::BTreeMap;

use kh2_archive::Archive;

use crate::msg::Messages;
use crate::rt::{DecodeError, DynTable};
use crate::tables;

/// Drive form ids as the game uses them (`cmd.Argument` of a drive command, the key nibble of
/// `fmlv`, the `ObjectForm` of the form's object). Form 0 is not a drive form: its `fmlv` rows are
/// the summon level curve.
pub mod form {
    pub const SUMMON_CURVE: u8 = 0;
    pub const VALOR: u8 = 1;
    pub const WISDOM: u8 = 2;
    pub const MASTER: u8 = 3;
    pub const FINAL: u8 = 4;
    pub const ANTI: u8 = 5;

    pub fn name(form: u8) -> &'static str {
        match form {
            0 => "Summon",
            1 => "Valor",
            2 => "Wisdom",
            3 => "Master",
            4 => "Final",
            5 => "Anti",
            _ => "?",
        }
    }
}

/// `item.Type` values seen on player-system items.
pub mod item_type {
    pub const ABILITY: u8 = 19;
    pub const SUMMON_CHARM: u8 = 20;
    pub const DRIVE_FORM: u8 = 21;
}

/// `cmd.Execute` values the player's command executor (0x1cf030) and availability check (0x1ba2c0)
/// switch on. Reaction commands and most others use their own id here.
pub mod execute {
    pub const ATTACK: u16 = 1;
    pub const MAGIC: u16 = 2;
    pub const ITEM: u16 = 3;
    pub const DRIVE: u16 = 4;
    pub const REVERT: u16 = 5;
    pub const SUMMON: u16 = 72;
    pub const PARTY: u16 = 73;
    pub const LIMIT: u16 = 81;
}

// ---------------------------------------------------------------------------------------------
// fmlv: form level curve

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormLevel {
    pub form: u8,
    /// Internal level 1..=7 (the key's low nibble).
    pub level: u8,
    /// Level shown to the player; it rises at some internal levels, which upgrades the form's
    /// growth ability. For form 5 this byte is the Anti-form roll chance instead.
    pub display_level: u8,
    /// Ability granted on reaching this level (0 = none).
    pub ability: u16,
    /// Form experience needed to leave this level (form 5: counter threshold).
    pub exp: i32,
}

#[derive(Debug, Clone, Default)]
pub struct FormLevels(pub Vec<FormLevel>);

impl FormLevels {
    pub fn from_rows(rows: &[tables::Fmlv]) -> FormLevels {
        FormLevels(
            rows.iter()
                .map(|r| FormLevel {
                    form: r.unk0 >> 4,
                    level: r.unk0 & 15,
                    display_level: r.unk1,
                    ability: r.ability,
                    exp: r.exp,
                })
                .collect(),
        )
    }

    pub fn get(&self, form: u8, level: u8) -> Option<&FormLevel> {
        self.0.iter().find(|r| r.form == form && r.level == level)
    }

    pub fn levels(&self, form: u8) -> impl Iterator<Item = &FormLevel> {
        self.0.iter().filter(move |r| r.form == form)
    }

    /// Highest internal level of a form.
    pub fn top_level(&self, form: u8) -> u8 {
        self.levels(form).map(|r| r.level).max().unwrap_or(0)
    }
}

/// Highest form level reachable, from how many of the four forms 1..=4 the player owns
/// (FUN_001d3bd0: 2 + owned, and 7 instead of 6).
pub fn form_level_cap(owned_forms: u32) -> u8 {
    match 2 + owned_forms.min(4) {
        6 => 7,
        n => n as u8,
    }
}

/// Anti-form roll chance in percent for the current drive counter (FUN_001d4278): the row of form 5
/// whose threshold the counter has reached. The game multiplies it by a factor (1 unless a special
/// battle sets it) and compares against a random 0..99.
/// orig 0x1d4278
pub fn anti_form_chance(levels: &FormLevels, counter: i32) -> u8 {
    let rows: Vec<&FormLevel> = levels.levels(form::ANTI).collect();
    let mut i = 0;
    while i + 1 < rows.len() && counter < rows[i].exp {
        i += 1;
    }
    rows.get(i).map(|r| r.display_level).unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------
// memt: party member records

/// `memt`: u32 version, u32 count, `count` records of 24 u16 words, then 7 u32 party-slot words.
/// Record 0 describes Sora. Only the form-related words are named; the rest are kept raw.
#[derive(Debug, Clone)]
pub struct Memt {
    pub version: u32,
    pub rows: Vec<[u16; 24]>,
    pub trailing: Vec<u32>,
}

impl Memt {
    pub const RECORD: usize = 48;

    pub fn parse(b: &[u8]) -> Result<Memt, DecodeError> {
        let bad = |need| DecodeError::Truncated { table: "memt", need, have: b.len() };
        let word = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
        let version = word(0).ok_or(bad(8))?;
        let n = word(4).ok_or(bad(8))? as usize;
        let end = 8 + n * Self::RECORD;
        let body = b.get(8..end).ok_or(bad(end))?;
        let rows = body
            .as_chunks::<{ Self::RECORD }>()
            .0
            .iter()
            .map(|c| std::array::from_fn(|k| u16::from_le_bytes([c[2 * k], c[2 * k + 1]])))
            .collect();
        let trailing = b[end..].as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
        Ok(Memt { version, rows, trailing })
    }

    pub fn sora(&self) -> Option<&[u16; 24]> {
        self.rows.first()
    }

    /// Sora's player object for a drive form 1..=5 (FUN_001cf2c8: record 0 byte offset 0x16 + 2 * form).
    pub fn form_object(&self, form: u8) -> Option<u16> {
        (1..=5).contains(&form).then(|| self.sora().map(|r| r[11 + form as usize]))?
    }

    /// Keyblade object for a form 0..=5 (FUN_00196ad0: record 0 byte offset 0x24 + 2 * form).
    pub fn form_weapon(&self, form: u8) -> Option<u16> {
        (form <= 5).then(|| self.sora().map(|r| r[18 + form as usize]))?
    }

    /// Sora's normal object for a world outfit index (record 0 byte offset 0x10 + 2 * outfit).
    pub fn base_object(&self, outfit: u8) -> Option<u16> {
        (outfit < 4).then(|| self.sora().map(|r| r[8 + outfit as usize]))?
    }
}

// ---------------------------------------------------------------------------------------------
// Drive forms, derived from item + cmd + memt

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveForm {
    pub form: u8,
    /// The "X Form" item (type 21) the player owns once the form is unlocked.
    pub item: u16,
    /// Drive command id shown in the menu (item.Command).
    pub command: u16,
    /// Drive points spent on entering (cmd.Cost).
    pub cost: u8,
    pub object: Option<u16>,
    pub weapon: Option<u16>,
}

pub fn drive_forms(items: &[tables::Item], cmds: &[tables::Cmd], memt: &Memt) -> Vec<DriveForm> {
    let mut out: Vec<DriveForm> = items
        .iter()
        .filter(|i| i.r#type == item_type::DRIVE_FORM)
        .filter_map(|i| {
            let c = cmds.iter().find(|c| c.id == i.command && c.execute == execute::DRIVE)?;
            let form = c.argument as u8;
            Some(DriveForm {
                form,
                item: i.id,
                command: c.id,
                cost: c.cost,
                object: memt.form_object(form),
                weapon: memt.form_weapon(form),
            })
        })
        .collect();
    out.sort_by_key(|d| d.form);
    out
}

// ---------------------------------------------------------------------------------------------
// Abilities

/// An ability is an `item` row of type 19. The ability bit used in the per-character ability set
/// is `Flag1 | Rank << 8` (FUN_001c0f40 sets bit `*(u16*)(item + 4)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ability {
    pub id: u16,
    pub bit: u16,
    /// StatEntry high byte. Matches the in-game AP cost of every ability checked (inferred).
    pub ap_cost: u8,
    /// StatEntry low byte: 0 support, 1 party-only, 2 action, 3 growth (from FUN_001c0e50, FUN_001c1340).
    pub kind: u8,
    /// Command the ability turns into (Auto forms: a drive command), 0 if none.
    pub command: u16,
    pub name: u16,
    pub description: u16,
}

pub fn abilities(items: &[tables::Item]) -> Vec<Ability> {
    items
        .iter()
        .filter(|i| i.r#type == item_type::ABILITY)
        .map(|i| Ability {
            id: i.id,
            bit: i.flag1 as u16 | (i.rank as u16) << 8,
            ap_cost: (i.stat_entry >> 8) as u8,
            kind: (i.stat_entry & 0xff) as u8,
            command: i.command,
            name: i.name,
            description: i.description,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Limits and summons

/// A limit whose `Summon` byte is 200..=204 is a summon limit: it spends the summon gauge instead
/// of MP and needs no second participant (FUN_001d1fe0).
pub fn is_summon_limit(summon_byte: u8) -> bool {
    (200..=204).contains(&summon_byte)
}

/// The limit row the game picks for a command in a world (FUN_001d2040 + FUN_001d1ce0): the first
/// row with this command whose World is 0 or the current world.
pub fn limit_for_command(rows: &[tables::Limt], command: u16, world: u16) -> Option<&tables::Limt> {
    rows.iter().find(|r| r.command == command && (r.world == 0 || r.world == world))
}

/// The summon row for a summon command (FUN_001d4ca0 matches `Command`).
pub fn summon_for_command(rows: &[tables::Sumn], command: u16) -> Option<&tables::Sumn> {
    rows.iter().find(|r| r.command == command)
}

// ---------------------------------------------------------------------------------------------
// rcct: Jiminy journal reaction-command lists

/// `rcct`: u32 version, u32 count, `count` x {u16 key, u16 command[5]}. The only reader is the
/// journal's enemy page (FUN_002aadf0): `key` is the enemy object's NeoStatus (objentry +0x4c) and
/// the commands are the reaction commands listed, with their use counts, for that enemy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rcct {
    pub key: u16,
    pub commands: [u16; 5],
}

pub fn parse_rcct(b: &[u8]) -> Result<(u32, Vec<Rcct>), DecodeError> {
    let bad = |need| DecodeError::Truncated { table: "rcct", need, have: b.len() };
    let word = |o: usize| b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()));
    let version = word(0).ok_or(bad(8))?;
    let n = word(4).ok_or(bad(8))? as usize;
    let end = 8 + n * 12;
    let body = b.get(8..end).ok_or(bad(end))?;
    let rows = body
        .as_chunks::<12>()
        .0
        .iter()
        .map(|c| {
            let w = |k: usize| u16::from_le_bytes([c[2 * k], c[2 * k + 1]]);
            Rcct { key: w(0), commands: [w(1), w(2), w(3), w(4), w(5)] }
        })
        .collect();
    Ok((version, rows))
}

// ---------------------------------------------------------------------------------------------
// sstm: engine tuning record

/// The single record of the `sstm` pref entry in 03system.bin: 92 words that the engine reads as
/// f32 or i32 by byte offset. Offsets named in [`sstm_label`] were traced to their readers.
#[derive(Debug, Clone)]
pub struct Sstm {
    pub words: Vec<u32>,
}

impl Sstm {
    pub fn f32(&self, off: usize) -> f32 {
        f32::from_bits(self.words[off / 4])
    }

    pub fn i32(&self, off: usize) -> i32 {
        self.words[off / 4] as i32
    }

    /// Gauge units in one drive point (also the unit of the form and summon timers).
    pub fn drive_point_units(&self) -> f32 {
        self.f32(0x68)
    }
}

/// Byte offsets inside the sstm record, named for what reads them.
pub fn sstm_label(off: usize) -> Option<&'static str> {
    Some(match off {
        0x60 => "drive gain scale for hits with attacker flag 0x800 (normal mode)",
        0x64 => "gauge gain scale for hits with attacker flag 0x800 (form mode)",
        0x68 => "drive point size in gauge units; form/summon timer = this * (3 + level)",
        0x6c => "form/summon timer drain scale per frame, applied while FUN_001903d8 is clear",
        0x70 => "gauge gain multiplier while in a form or summon",
        0x74 => "gauge loss multiplier when hit during a form",
        0x78 => "drive gain multiplier while MP-charging",
        0x8c => "Anti form timer basis (level = this - 2)",
        0x90 => "Anti counter decrement when Anti triggers",
        0x94 => "damage multiplier while in Anti form",
        0x98 => "Final form chance: base",
        0x9c => "Final form chance: growth per drive",
        0xa0 => "Final form chance: cap",
        0xa4 => "Anti counter decrement when Final is chosen",
        0x11c => "Berserk Charge step",
        0x120 => "MP Haste step",
        0x124 => "MP Hastera step",
        0x128 => "MP Hastega step",
        0x12c => "Draw step",
        0x130 => "Combo Boost / Air Combo Boost step",
        0x134 => "Reaction Boost step",
        0x138 => "Damage Drive step",
        0x13c => "Drive Boost step",
        0x140 => "Form Boost multiplier",
        0x148 => "Defender step",
        0x14c => "Fire/Blizzard/Thunder Boost step",
        0x150 => "MP Rage step",
        0x154 => "Hyper Healing multiplier",
        0x158 => "Combination Boost multiplier",
        0x15c => "Jackpot step",
        0x160 => "Lucky Lucky step",
        0x164 => "Item Boost step",
        0x16c => "Summon Boost multiplier",
        _ => return None,
    })
}

pub fn load_sstm(a: &mut Archive) -> Result<Sstm, DecodeError> {
    let file = a.read("03system.bin").map_err(|e| DecodeError::Archive(e.to_string()))?;
    let top = kh2_bar::parse(&file).map_err(|e| DecodeError::Archive(format!("03system.bin: {e:?}")))?;
    let pref = kh2_bar::find(&top, "pref").ok_or_else(|| DecodeError::Archive("no pref".into()))?;
    let inner = kh2_bar::parse(pref.data).map_err(|_| DecodeError::Archive("pref: not a BAR".into()))?;
    let sub = kh2_bar::find(&inner, "sstm").ok_or_else(|| DecodeError::Archive("pref: no sstm".into()))?.data;
    let bad = |need| DecodeError::Truncated { table: "sstm", need, have: sub.len() };
    let u32_at = |o: usize| sub.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()) as usize);
    let at = u32_at(4).ok_or(bad(8))?;
    let rec = sub.get(at..).ok_or(bad(at))?;
    let words = rec.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect();
    Ok(Sstm { words })
}

// ---------------------------------------------------------------------------------------------
// Loading and dumps

fn load<T: crate::rt::Record>(a: &mut Archive, def: &crate::rt::TableDef) -> Result<Vec<T>, DecodeError> {
    Ok(crate::load::<T>(a, def)?.rows)
}

pub fn load_memt(a: &mut Archive) -> Result<Memt, DecodeError> {
    let blob = crate::subfile(a, &tables::MEMT)?;
    Memt::parse(&blob)
}

pub fn load_rcct(a: &mut Archive) -> Result<(u32, Vec<Rcct>), DecodeError> {
    let blob = crate::subfile(a, &tables::RCCT)?;
    parse_rcct(&blob)
}

pub fn load_messages(a: &mut Archive) -> Option<Messages> {
    let bar = a.read("msg/us/sys.bar").ok()?;
    Messages::from_bar(&bar)
}

fn dyn_table(fields: &'static [&'static str], rows: Vec<Vec<String>>, version: u32, len: usize) -> DynTable {
    DynTable { version, fields, rows, used: len, len }
}

/// Names of the derived tables [`dump`] knows.
pub const DUMPS: &[&str] = &["rcct", "memt", "sstm_full", "abilities", "drive_forms", "form_levels", "limits_ex", "summons_ex"];

/// Builds a dump for `kh2 table NAME`; `None` if `name` is not one of [`DUMPS`].
pub fn dump(a: &mut Archive, name: &str) -> Option<Result<DynTable, DecodeError>> {
    if !DUMPS.contains(&name) {
        return None;
    }
    Some(dump_inner(a, name))
}

fn text(m: &Option<Messages>, field: u32) -> String {
    m.as_ref().and_then(|m| m.get(field)).unwrap_or_default()
}

fn dump_inner(a: &mut Archive, name: &str) -> Result<DynTable, DecodeError> {
    Ok(match name {
        "rcct" => {
            let blob = crate::subfile(a, &tables::RCCT)?;
            let (v, rows) = parse_rcct(&blob)?;
            let rows = rows
                .iter()
                .map(|r| {
                    let mut c = vec![r.key.to_string()];
                    c.extend(r.commands.iter().map(|x| x.to_string()));
                    c
                })
                .collect();
            dyn_table(&["Key", "Cmd0", "Cmd1", "Cmd2", "Cmd3", "Cmd4"], rows, v, blob.len())
        }
        "memt" => {
            let blob = crate::subfile(a, &tables::MEMT)?;
            let m = Memt::parse(&blob)?;
            static F: [&str; 24] = [
                "W00", "W01", "W02", "W03", "W04", "W05", "W06", "W07", "W08", "W09", "W10", "W11", "W12", "W13", "W14",
                "W15", "W16", "W17", "W18", "W19", "W20", "W21", "W22", "W23",
            ];
            let rows = m.rows.iter().map(|r| r.iter().map(|x| x.to_string()).collect()).collect();
            let mut t = dyn_table(&F, rows, m.version, blob.len());
            t.used = 8 + m.rows.len() * Memt::RECORD + m.trailing.len() * 4;
            t
        }
        "sstm_full" => {
            let s = load_sstm(a)?;
            let rows = (0..s.words.len())
                .map(|i| {
                    let off = i * 4;
                    vec![
                        format!("{off:#05x}"),
                        format!("{:#010x}", s.words[i]),
                        format!("{}", s.f32(off)),
                        format!("{}", s.i32(off)),
                        sstm_label(off).unwrap_or("").to_string(),
                    ]
                })
                .collect();
            dyn_table(&["Offset", "Hex", "F32", "I32", "Use"], rows, 0, s.words.len() * 4)
        }
        "abilities" => {
            let items = load::<tables::Item>(a, &tables::ITEM)?;
            let msg = load_messages(a);
            let rows = abilities(&items)
                .iter()
                .map(|x| {
                    vec![
                        x.id.to_string(),
                        text(&msg, x.name as u32),
                        x.bit.to_string(),
                        x.ap_cost.to_string(),
                        x.kind.to_string(),
                        x.command.to_string(),
                    ]
                })
                .collect();
            dyn_table(&["Id", "Name", "Bit", "ApCost", "Kind", "Command"], rows, 0, 0)
        }
        "drive_forms" => {
            let items = load::<tables::Item>(a, &tables::ITEM)?;
            let cmds = load::<tables::Cmd>(a, &tables::CMD)?;
            let memt = load_memt(a)?;
            let fl = FormLevels::from_rows(&load::<tables::Fmlv>(a, &tables::FMLV)?);
            let objs = load::<tables::Objentry>(a, &tables::OBJENTRY)?;
            let obj_name = |id: Option<u16>| {
                id.and_then(|id| objs.iter().find(|o| o.object_id == id as u32)).map(|o| o.model_name.clone()).unwrap_or_default()
            };
            let rows = drive_forms(&items, &cmds, &memt)
                .iter()
                .map(|d| {
                    vec![
                        d.form.to_string(),
                        form::name(d.form).to_string(),
                        d.item.to_string(),
                        d.command.to_string(),
                        d.cost.to_string(),
                        d.object.map(|x| x.to_string()).unwrap_or_default(),
                        obj_name(d.object),
                        d.weapon.map(|x| x.to_string()).unwrap_or_default(),
                        obj_name(d.weapon),
                        // Form 5's rows are the Anti roll table, not levels.
                        if d.form == form::ANTI { "-".to_string() } else { fl.top_level(d.form).to_string() },
                    ]
                })
                .collect();
            dyn_table(
                &["Form", "Name", "Item", "Command", "Cost", "Object", "Model", "Weapon", "WeaponModel", "TopLevel"],
                rows,
                0,
                0,
            )
        }
        "form_levels" => {
            let fl = FormLevels::from_rows(&load::<tables::Fmlv>(a, &tables::FMLV)?);
            let msg = load_messages(a);
            let items = load::<tables::Item>(a, &tables::ITEM)?;
            let rows = fl
                .0
                .iter()
                .map(|r| {
                    let ab = items.iter().find(|i| i.id == r.ability && r.ability != 0);
                    vec![
                        r.form.to_string(),
                        r.level.to_string(),
                        r.display_level.to_string(),
                        r.exp.to_string(),
                        r.ability.to_string(),
                        ab.map(|i| text(&msg, i.name as u32)).unwrap_or_default(),
                    ]
                })
                .collect();
            dyn_table(&["Form", "Level", "DisplayLevel", "Exp", "Ability", "AbilityName"], rows, 0, 0)
        }
        "limits_ex" => {
            let limt = load::<tables::Limt>(a, &tables::LIMT)?;
            let cmds = load::<tables::Cmd>(a, &tables::CMD)?;
            let items = load::<tables::Item>(a, &tables::ITEM)?;
            let msg = load_messages(a);
            let rows = limt
                .iter()
                .map(|l| {
                    let c = cmds.iter().find(|c| c.id == l.command);
                    let ab = items.iter().find(|i| i.id == l.limit);
                    vec![
                        l.id.to_string(),
                        l.character.to_string(),
                        l.summon.to_string(),
                        l.group.to_string(),
                        l.file_name.clone(),
                        l.spawn_id.to_string(),
                        l.command.to_string(),
                        c.map(|c| text(&msg, c.message_id as u32)).unwrap_or_default(),
                        c.map(|c| format!("{}/{}", c.cost, c.time)).unwrap_or_default(),
                        l.limit.to_string(),
                        ab.map(|i| text(&msg, i.name as u32)).unwrap_or_default(),
                        l.world.to_string(),
                        is_summon_limit(l.summon).to_string(),
                    ]
                })
                .collect();
            dyn_table(
                &[
                    "Id", "Char", "Summon", "Group", "File", "SpawnId", "Command", "CommandName", "Cost/Time", "Limit",
                    "LimitName", "World", "SummonLimit",
                ],
                rows,
                0,
                0,
            )
        }
        "summons_ex" => {
            let sumn = load::<tables::Sumn>(a, &tables::SUMN)?;
            let cmds = load::<tables::Cmd>(a, &tables::CMD)?;
            let msg = load_messages(a);
            let rows = sumn
                .iter()
                .map(|s| {
                    let c = cmds.iter().find(|c| c.id == s.command);
                    vec![
                        s.command.to_string(),
                        c.map(|c| text(&msg, c.message_id as u32)).unwrap_or_default(),
                        s.item.to_string(),
                        s.entity1.to_string(),
                        s.entity2.to_string(),
                        s.limit_command.to_string(),
                    ]
                })
                .collect();
            dyn_table(&["Command", "Name", "Item", "Entity1", "Entity2", "LimitCommand"], rows, 0, 0)
        }
        _ => unreachable!(),
    })
}

/// Counts rows per `kind` byte, for sanity checks and dumps.
pub fn ability_kinds(list: &[Ability]) -> BTreeMap<u8, usize> {
    let mut m = BTreeMap::new();
    for a in list {
        *m.entry(a.kind).or_insert(0) += 1;
    }
    m
}
