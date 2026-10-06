//! ARD area data (`ard/<area>.ard`, a BAR): the area scripts that say which spawn groups an area
//! loads, and the spawn groups themselves (objects, where they stand, their spawn records).
//!
//! Layouts, checked against every `tt` area of the USA disc (each spawn file is consumed to its
//! last byte by the sizes below):
//!
//! * **Area script** (BAR type 13, entries `map`, `btl`, `evt`): a list of programs, each
//!   `u16 id, u16 size` (size in bytes, header included) followed by commands
//!   `u16 opcode, u16 count, count × u32`. Id 0xffff ends the list. Opcode 0 names a spawn group
//!   (four characters, the BAR tag of a type-12 entry) to load.
//! * **Spawn file** (BAR type 12): `u32 version (2), u32 groups`, then per group a 0x2c-byte
//!   header (`u16 kind, u16 id, u16 entities, u16 event activators, u16 walk paths,
//!   u16 return parameters, u16 signals`, the rest zero on this disc), the entities (0x40 bytes
//!   each), the event activators (0x40), the walk paths (an 8-byte header with the point count at
//!   +2, then 12 bytes per point), the return parameters (0x10) and the signals (8 bytes; `ard/tt16.ard` `b_00` has one).
//! * **Entity** (0x40 bytes): `u32 object` (the `00objentry` id), position and rotation (three
//!   f32 each, the game's space: centimetres, Y down), then the spawn record bytes the game reads
//!   through the object's pointer at +0x81c: the spawn kind byte at +0x1c (FUN_0016b858, host
//!   1.110) and the halfword at +0x1e (FUN_0016b928, the key of host 1.143).
//!
//! Format names follow OpenKH (`OpenKh.Kh2/Ard/SpawnPoint.cs`, `AreaDataScript.cs`, Apache-2.0);
//! the sizes were checked on the disc.

use std::collections::BTreeMap;

/// BAR entry type of an area script.
pub const SCRIPT_KIND: u16 = 13;
/// BAR entry type of a spawn file.
pub const SPAWN_KIND: u16 = 12;
/// Area-script opcode that loads a spawn group.
pub const OP_SPAWN: u16 = 0;

#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub opcode: u16,
    pub args: Vec<u32>,
}

impl Command {
    /// The spawn group an `OP_SPAWN` command loads.
    pub fn spawn_name(&self) -> Option<String> {
        if self.opcode != OP_SPAWN {
            return None;
        }
        let w = *self.args.first()?;
        let s: String = w.to_le_bytes().iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
        Some(s)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub id: u16,
    pub commands: Vec<Command>,
}

impl Program {
    /// Names of the spawn groups the program loads, in order.
    pub fn spawns(&self) -> impl Iterator<Item = String> + '_ {
        self.commands.iter().filter_map(Command::spawn_name)
    }
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

fn f32_at(d: &[u8], o: usize) -> Option<f32> {
    u32_at(d, o).map(f32::from_bits)
}

/// Parses an area script (BAR type 13).
pub fn parse_script(d: &[u8]) -> Option<Vec<Program>> {
    let mut out = Vec::new();
    let mut off = 0;
    while off + 4 <= d.len() {
        let (id, size) = (u16_at(d, off)?, u16_at(d, off + 2)? as usize);
        if id == 0xffff {
            break;
        }
        if size < 4 {
            return None;
        }
        let end = off + size;
        let mut p = off + 4;
        let mut commands = Vec::new();
        while p < end {
            let (opcode, n) = (u16_at(d, p)?, u16_at(d, p + 2)? as usize);
            let args = (0..n).map(|k| u32_at(d, p + 4 + 4 * k)).collect::<Option<Vec<u32>>>()?;
            commands.push(Command { opcode, args });
            p += 4 + 4 * n;
        }
        if p != end {
            return None;
        }
        out.push(Program { id, commands });
        off = end;
    }
    Some(out)
}

/// One object of a spawn group.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    /// `00objentry` id of the object.
    pub object: u32,
    /// Position in the game's space (centimetres, Y down).
    pub position: [f32; 3],
    /// Rotation in radians (the game's space).
    pub rotation: [f32; 3],
    /// The spawn kind byte (+0x1c), what host 1.110 returns for the object.
    pub spawn_kind: u8,
    /// The byte after it (+0x1d).
    pub spawn_arg: u8,
    /// The halfword at +0x1e.
    pub serial: u16,
    /// The whole 0x40-byte record.
    pub raw: [u8; 0x40],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub kind: u16,
    pub id: u16,
    pub entities: Vec<Entity>,
}

const GROUP_HEADER: usize = 0x2c;
const ENTITY_SIZE: usize = 0x40;
const ACTIVATOR_SIZE: usize = 0x40;
const RETURN_SIZE: usize = 0x10;
const SIGNAL_SIZE: usize = 8;

/// The only spawn-file version this crate reads (every `tt` area; three areas elsewhere carry
/// version-1 files, which `Area::parse` lists in `Area::other_versions`).
pub const SPAWN_VERSION: u32 = 2;

/// Parses a spawn file (BAR type 12). `None` when it is not version 2 or the sizes do not add up
/// to the file's length.
pub fn parse_spawns(d: &[u8]) -> Option<Vec<Group>> {
    if d.is_empty() {
        return Some(Vec::new());
    }
    if u32_at(d, 0)? != SPAWN_VERSION {
        return None;
    }
    let count = u32_at(d, 4)? as usize;
    let mut off = 8;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let h = |k: usize| u16_at(d, off + 2 * k);
        let (kind, id, n, activators, paths, returns, signals) = (h(0)?, h(1)?, h(2)? as usize, h(3)? as usize, h(4)? as usize, h(5)? as usize, h(6)? as usize);
        off += GROUP_HEADER;
        let mut entities = Vec::with_capacity(n);
        for _ in 0..n {
            let raw: [u8; ENTITY_SIZE] = d.get(off..off + ENTITY_SIZE)?.try_into().ok()?;
            let f = |k: usize| f32_at(&raw, k);
            entities.push(Entity {
                object: u32_at(&raw, 0)?,
                position: [f(4)?, f(8)?, f(12)?],
                rotation: [f(16)?, f(20)?, f(24)?],
                spawn_kind: raw[0x1c],
                spawn_arg: raw[0x1d],
                serial: u16_at(&raw, 0x1e)?,
                raw,
            });
            off += ENTITY_SIZE;
        }
        off += activators * ACTIVATOR_SIZE;
        for _ in 0..paths {
            let points = u16_at(d, off + 2)? as usize;
            off += 8 + 12 * points;
        }
        off += returns * RETURN_SIZE + signals * SIGNAL_SIZE;
        out.push(Group { kind, id, entities });
    }
    (off == d.len()).then_some(out)
}

/// An area file: its scripts by BAR tag (`map`, `btl`, `evt`) and its spawn files by tag.
#[derive(Debug, Clone, Default)]
pub struct Area {
    pub scripts: BTreeMap<String, Vec<Program>>,
    pub spawns: BTreeMap<String, Vec<Group>>,
    /// Spawn files of another version than `SPAWN_VERSION` (not read).
    pub other_versions: Vec<String>,
}

impl Area {
    /// Parses `ard/<area>.ard`. Entries that do not parse are left out (and listed in `bad`).
    pub fn parse(ard: &[u8]) -> Option<(Area, Vec<String>)> {
        let mut area = Area::default();
        let mut bad = Vec::new();
        for e in kh2_bar::parse(ard).ok()? {
            let tag = e.tag.to_string();
            match e.kind {
                SCRIPT_KIND => match parse_script(e.data) {
                    Some(p) => {
                        area.scripts.insert(tag, p);
                    }
                    None => bad.push(tag),
                },
                SPAWN_KIND if e.data.len() >= 4 && u32_at(e.data, 0) != Some(SPAWN_VERSION) => area.other_versions.push(tag),
                SPAWN_KIND => match parse_spawns(e.data) {
                    Some(g) => {
                        area.spawns.insert(tag, g);
                    }
                    None => bad.push(tag),
                },
                _ => {}
            }
        }
        Some((area, bad))
    }

    /// Every (script tag, program id, spawn group tag, entity) the area's scripts can load.
    pub fn spawned(&self) -> Vec<(&str, u16, String, &Entity)> {
        let mut out = Vec::new();
        for (tag, progs) in &self.scripts {
            for p in progs {
                for name in p.spawns() {
                    for g in self.spawns.get(&name).into_iter().flatten() {
                        for e in &g.entities {
                            out.push((tag.as_str(), p.id, name.clone(), e));
                        }
                    }
                }
            }
        }
        out
    }

    /// The spawn groups (in script order, each once) that a script's programs load and that hold at
    /// least one entity of `object`, with the program that loads each.
    pub fn groups_with(&self, script: &str, object: u32) -> Vec<(u16, String)> {
        let mut out: Vec<(u16, String)> = Vec::new();
        for p in self.scripts.get(script).into_iter().flatten() {
            for name in p.spawns() {
                let has = self.spawns.get(&name).into_iter().flatten().any(|g| g.entities.iter().any(|e| e.object == object));
                if has && !out.iter().any(|(_, n)| *n == name) {
                    out.push((p.id, name));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le16(v: &mut Vec<u8>, x: u16) {
        v.extend_from_slice(&x.to_le_bytes());
    }

    #[test]
    fn a_script_with_two_programs() {
        let mut d = Vec::new();
        // program 1: spawn "b_00"; program 7: an opcode with two arguments, then spawn "b_21".
        le16(&mut d, 1);
        le16(&mut d, 12);
        le16(&mut d, 0);
        le16(&mut d, 1);
        d.extend_from_slice(b"b_00");
        le16(&mut d, 7);
        le16(&mut d, 4 + 12 + 8);
        le16(&mut d, 4);
        le16(&mut d, 2);
        d.extend_from_slice(&[1, 0, 0, 0, 2, 0, 0, 0]);
        le16(&mut d, 0);
        le16(&mut d, 1);
        d.extend_from_slice(b"b_21");
        le16(&mut d, 0xffff);
        le16(&mut d, 0);
        let p = parse_script(&d).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].spawns().collect::<Vec<_>>(), ["b_00"]);
        assert_eq!(p[1].commands[0], Command { opcode: 4, args: vec![1, 2] });
        assert_eq!(p[1].spawns().collect::<Vec<_>>(), ["b_21"]);
    }

    #[test]
    fn a_spawn_group_is_consumed_to_its_last_byte() {
        let mut d = vec![2, 0, 0, 0, 1, 0, 0, 0];
        let mut h = vec![0u8; GROUP_HEADER];
        h[0] = 2; // kind
        h[4] = 1; // one entity
        h[6] = 1; // one event activator
        d.extend_from_slice(&h);
        let mut e = [0u8; ENTITY_SIZE];
        e[0..4].copy_from_slice(&0x13eu32.to_le_bytes());
        e[4..8].copy_from_slice(&(-54.5f32).to_le_bytes());
        e[0x1c] = 2;
        e[0x1e] = 0x27;
        d.extend_from_slice(&e);
        d.extend_from_slice(&[0u8; ACTIVATOR_SIZE]);
        let g = parse_spawns(&d).unwrap();
        assert_eq!(g[0].entities[0].object, 0x13e);
        assert_eq!(g[0].entities[0].position[0], -54.5);
        assert_eq!((g[0].entities[0].spawn_kind, g[0].entities[0].serial), (2, 0x27));
        // One byte too many or too few is not a spawn file.
        d.push(0);
        assert!(parse_spawns(&d).is_none());
        d.truncate(d.len() - 2);
        assert!(parse_spawns(&d).is_none());
    }
}
