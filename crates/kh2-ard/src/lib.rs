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
/// BAR entry type of an event (cutscene) in an area file (FUN_00180a28 finds them by tag).
pub const EVENT_KIND: u16 = 0x16;

/// Number of opcodes the area-script interpreter knows (FUN_00180b48: `sltiu op, 0x20`; higher
/// opcodes fall to the default case and do nothing).
pub const OPCODES: u16 = 0x20;

/// The project's name for an area-script opcode (what FUN_00180b48 does with it; docs/events.md
/// section 1). Names are descriptive, not the game's.
pub fn opcode_name(op: u16) -> &'static str {
    match op {
        0 => "spawn",
        1 => "map_visibility",
        2 => "spawn_random",
        3 => "spawn_chance_stop",
        4 => "capacity",
        5 => "heap_size",
        6 => "cache_a",
        7 => "cache_b",
        8 | 0xd => "no_op",
        9 => "spawn_alt",
        0xa => "entry_by_index",
        0xb => "word_35cd18",
        0xc => "trigger",
        0xe => "word_35cd30",
        0xf => "world_record",
        0x10 => "music",
        0x11 => "word_35cd34",
        0x12 => "heap_base",
        0x13 => "camera_type",
        0x14 => "flag_35d268_4",
        0x15 => "battle_manager",
        0x16 => "load_file",
        0x17 => "flag_35d268_10",
        0x18 => "effect_heap",
        0x19 => "set_story_flags",
        0x1a => "map_show_bits",
        0x1b => "map_hide_bits",
        0x1c => "if_entrance",
        0x1d => "party_reset",
        0x1e => "level",
        0x1f => "spawn_lottery_stop",
        _ => "ignored",
    }
}

/// A trigger definition (opcode 0xc, FUN_00180ff8): the key `(event, arg)` of the trigger record
/// (FUN_001975e0) and its sub-commands `(sub-opcode, halfwords)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Trigger {
    pub event: u16,
    pub arg: u16,
    pub subs: Vec<(u16, Vec<u16>)>,
}

/// Decodes the arguments of an opcode-0xc command into its sub-commands, with the sizes
/// FUN_00180ff8 steps by (in halfwords, sub-opcode included): 0 four (u16, u32), 1 eight (one
/// unread halfword, then the six of the jump record, FUN_0019d8f0), 2..5, 7, 8 two, 6 two plus
/// twice its count.
/// `None` when a sub-opcode is unknown or the sizes overrun.
pub fn parse_trigger(args: &[u32]) -> Option<Trigger> {
    let h: Vec<u16> = args.iter().flat_map(|w| [*w as u16, (*w >> 16) as u16]).collect();
    let (event, arg) = (*h.first()?, *h.get(1)?);
    let mut subs = Vec::new();
    let mut p = 2;
    while p < h.len() {
        let op = h[p];
        let len = match op {
            0 => 4,
            1 => 8,
            2..=5 | 7 | 8 => 2,
            6 => 2 + 2 * *h.get(p + 1)? as usize,
            _ => return None,
        };
        subs.push((op, h.get(p + 1..p + len)?.to_vec()));
        p += len;
    }
    Some(Trigger { event, arg, subs })
}

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

/// An event activator (0x40 bytes): a trigger volume of a spawn group (FUN_001d8b48 builds it,
/// FUN_001d8600 reads the fields; docs/events.md section 2).
#[derive(Debug, Clone, PartialEq)]
pub struct Activator {
    /// +0: 0 box, 1 cylinder (elliptic in x/z), 2 the third shape (FUN_001d8b48).
    pub shape: u16,
    /// +2: what the volume does in an exit group: 0 walk in, 1 and 2 a facing prompt (FUN_00198e10).
    pub option: u16,
    /// +4: centre in the game's space.
    pub position: [f32; 3],
    /// +0x10: half extents (box) or radii and half height (cylinder).
    pub scale: [f32; 3],
    /// +0x1c: rotation; the volume turns by -(+0x20) about Y (FUN_001d8600).
    pub rotation: [f32; 3],
    pub raw: [u8; 0x40],
}

/// A walk path: an 8-byte header (`u16` at +2 = point count) and 12 bytes per point.
#[derive(Debug, Clone, PartialEq)]
pub struct WalkPath {
    pub header: [u8; 8],
    /// Each point as three f32 [inferred: a position; no reader was traced].
    pub points: Vec<[f32; 3]>,
}

/// A signal (8 bytes, FUN_00198430): when the trigger `(event, arg)` fires, the group is shown
/// (`action` 0, FUN_00197838 -> FUN_00199c70) or hidden (1, FUN_00197870 -> FUN_00199c48).
#[derive(Debug, Clone, PartialEq)]
pub struct Signal {
    pub event: u16,
    /// The trigger's argument; event 0x4a ignores it (-1).
    pub arg: u16,
    pub action: u8,
    pub raw: [u8; 8],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// +0 (the game reads the low byte, FUN_00198750 / FUN_00198b30): what the group is (1 exit,
    /// 2 battle, ... docs/events.md section 2).
    pub kind: u16,
    pub id: u16,
    pub entities: Vec<Entity>,
    /// The whole 0x2c-byte header (+0x1c/+0x1d: the exit's area and entrance for kind 1).
    pub header: [u8; GROUP_HEADER],
    pub activators: Vec<Activator>,
    pub paths: Vec<WalkPath>,
    pub returns: Vec<[u8; RETURN_SIZE]>,
    pub signals: Vec<Signal>,
}

impl Group {
    /// Flag byte +1: bit 0 makes the group start disabled (FUN_00198430).
    pub fn flags(&self) -> u8 {
        self.header[1]
    }
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
        let header: [u8; GROUP_HEADER] = d.get(off..off + GROUP_HEADER)?.try_into().ok()?;
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
        let mut acts = Vec::with_capacity(activators);
        for _ in 0..activators {
            let raw: [u8; ACTIVATOR_SIZE] = d.get(off..off + ACTIVATOR_SIZE)?.try_into().ok()?;
            let f = |k: usize| f32_at(&raw, k);
            acts.push(Activator {
                shape: u16_at(&raw, 0)?,
                option: u16_at(&raw, 2)?,
                position: [f(4)?, f(8)?, f(12)?],
                scale: [f(0x10)?, f(0x14)?, f(0x18)?],
                rotation: [f(0x1c)?, f(0x20)?, f(0x24)?],
                raw,
            });
            off += ACTIVATOR_SIZE;
        }
        let mut walk = Vec::with_capacity(paths);
        for _ in 0..paths {
            let header: [u8; 8] = d.get(off..off + 8)?.try_into().ok()?;
            let points = u16_at(d, off + 2)? as usize;
            let pts = (0..points).map(|k| Some([f32_at(d, off + 8 + 12 * k)?, f32_at(d, off + 12 + 12 * k)?, f32_at(d, off + 16 + 12 * k)?])).collect::<Option<Vec<_>>>()?;
            walk.push(WalkPath { header, points: pts });
            off += 8 + 12 * points;
        }
        let mut rets = Vec::with_capacity(returns);
        for _ in 0..returns {
            rets.push(d.get(off..off + RETURN_SIZE)?.try_into().ok()?);
            off += RETURN_SIZE;
        }
        let mut sigs = Vec::with_capacity(signals);
        for _ in 0..signals {
            let raw: [u8; SIGNAL_SIZE] = d.get(off..off + SIGNAL_SIZE)?.try_into().ok()?;
            sigs.push(Signal { event: u16_at(&raw, 0)?, arg: u16_at(&raw, 2)?, action: raw[4], raw });
            off += SIGNAL_SIZE;
        }
        out.push(Group { kind, id, entities, header, activators: acts, paths: walk, returns: rets, signals: sigs });
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
    /// Every BAR entry: (tag, type, size in bytes).
    pub entries: Vec<(String, u16, usize)>,
    /// The events (type 0x16 entries) by tag, raw (see `parse_event`).
    pub events: BTreeMap<String, Vec<u8>>,
}

impl Area {
    /// Parses `ard/<area>.ard`. Entries that do not parse are left out (and listed in `bad`).
    pub fn parse(ard: &[u8]) -> Option<(Area, Vec<String>)> {
        let mut area = Area::default();
        let mut bad = Vec::new();
        for e in kh2_bar::parse(ard).ok()? {
            let tag = e.tag.to_string();
            area.entries.push((tag.clone(), e.kind, e.data.len()));
            match e.kind {
                EVENT_KIND => {
                    area.events.insert(tag, e.data.to_vec());
                }
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

/// One command of an event (BAR type 0x16 entry of an area file): `u16 size` (bytes, header
/// included), `u16 opcode`, then the operands. The event thread FUN_00223a88 runs them in order
/// (a size of 0 ends the list: the thread then idles).
#[derive(Debug, Clone, PartialEq)]
pub struct EventCommand {
    pub opcode: u16,
    /// The bytes after the 4-byte header.
    pub data: Vec<u8>,
}

/// Splits an event entry into its commands, up to the first size-0 header or the end of the
/// data. `None` when a size is below 4 or runs past the end.
pub fn parse_event(d: &[u8]) -> Option<Vec<EventCommand>> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 4 <= d.len() {
        let size = u16_at(d, p)? as usize;
        if size == 0 {
            break;
        }
        if size < 4 || p + size > d.len() {
            return None;
        }
        out.push(EventCommand { opcode: u16_at(d, p + 2)?, data: d[p + 4..p + size].to_vec() });
        p += size;
    }
    Some(out)
}

/// The story-flag side effects of `00progress.bin` (a BAR; its type-2 entries in order are the 19
/// worlds, FUN_0017fd10 stores them at 0x35ccc0). Setting flag `id` (FUN_0017ff60) sets its bit
/// and runs the list of the world `id >> 10` at the offset in the halfword `2 * (id & 0x3ff)` of
/// the world's entry: commands `u8 op, u8 n, n halfwords`, ended by op 0 (docs/events.md
/// section 5).
pub mod progress {
    /// One side-effect command of a story flag.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Effect {
        pub op: u8,
        /// The `2 * n` payload bytes.
        pub data: Vec<u8>,
    }

    impl Effect {
        /// Op 1: the area program bytes the flag writes, `(area, map, btl, evt)` per 4-byte
        /// record (FUN_00104b90 on `0x33e86f + world * 0xc0 + area * 3`; 0xff keeps a byte).
        pub fn programs(&self) -> Vec<[u8; 4]> {
            if self.op != 1 {
                return Vec::new();
            }
            (0..self.data.len() / 4).map(|k| [self.data[4 * k], self.data[4 * k + 1], self.data[4 * k + 2], self.data[4 * k + 3]]).collect()
        }

        /// Ops 7 and 0xc: the flags cleared (7) or set (0xc) in turn.
        pub fn flags(&self) -> Vec<u16> {
            (0..self.data.len() / 2).map(|k| u16::from_le_bytes([self.data[2 * k], self.data[2 * k + 1]])).collect()
        }
    }

    /// The side effects of the flag with low bits `index` (id & 0x3ff) in a world's entry.
    /// `None` when the offset or a command runs past the entry.
    pub fn effects(entry: &[u8], index: u16) -> Option<Vec<Effect>> {
        let o = 2 * index as usize;
        let mut p = u16::from_le_bytes(entry.get(o..o + 2)?.try_into().ok()?) as usize;
        let mut out = Vec::new();
        loop {
            let op = *entry.get(p)?;
            if op == 0 {
                return Some(out);
            }
            let n = *entry.get(p + 1)? as usize;
            out.push(Effect { op, data: entry.get(p + 2..p + 2 + 2 * n)?.to_vec() });
            p += 2 + 2 * n;
        }
    }

    /// The number of flags a world's entry lists [inferred: the offset table ends where the first
    /// command list begins, so its length is the smallest offset / 2].
    pub fn flag_count(entry: &[u8]) -> usize {
        let mut min = entry.len();
        let mut k = 0;
        while 2 * k + 2 <= min.min(entry.len()) {
            let off = u16::from_le_bytes([entry[2 * k], entry[2 * k + 1]]) as usize;
            if off >= 2 * (k + 1) {
                min = min.min(off);
            }
            k += 1;
        }
        min / 2
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
    fn a_trigger_decodes_its_sub_commands() {
        // Trigger (0, -1): set flag 0x879, play event "513" with argument 2, then jump (type 1)
        // to world 2 area 8 entrance 0 with flags 1; sub 7 = 0. The halfwords as on the disc (tt02).
        let h: [u16; 18] = [0, 0xffff, 2, 0x879, 0, 2, 0x3135, 0x33, 1, 0, 1, 2, 8, 0, 0, 1, 7, 0];
        let args: Vec<u32> = h.chunks(2).map(|c| c[0] as u32 | (c[1] as u32) << 16).collect();
        let t = parse_trigger(&args).unwrap();
        assert_eq!((t.event, t.arg), (0, 0xffff));
        assert_eq!(t.subs, vec![(2, vec![0x879]), (0, vec![2, 0x3135, 0x33]), (1, vec![0, 1, 2, 8, 0, 0, 1]), (7, vec![0])]);
        // An unknown sub-opcode is not a trigger.
        assert!(parse_trigger(&[0, 9]).is_none());
        assert_eq!(opcode_name(0xc), "trigger");
        assert_eq!(opcode_name(0x20), "ignored");
    }

    #[test]
    fn an_event_splits_into_sized_commands() {
        let mut d = Vec::new();
        for (size, op) in [(8u16, 0x15u16), (4, 0x0f)] {
            le16(&mut d, size);
            le16(&mut d, op);
            d.extend_from_slice(&vec![0xaa; size as usize - 4]);
        }
        le16(&mut d, 0);
        let e = parse_event(&d).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].opcode, e[0].data.len(), e[1].opcode), (0x15, 4, 0x0f));
        // A size that runs past the data is refused.
        assert!(parse_event(&[9, 0, 1, 0]).is_none());
    }

    #[test]
    fn story_flag_effects_follow_the_offset_table() {
        // Two flags: 0 has op 1 (one program record) then op 0xc (sets flag 0x805), 1 has none.
        let mut d = Vec::new();
        le16(&mut d, 4);
        le16(&mut d, 14);
        d.extend_from_slice(&[1, 2, 4, 0xff, 0x4d, 0xff, 0xc, 1, 0x05, 0x08]);
        d.push(0);
        assert_eq!(d.len(), 15);
        let e = progress::effects(&d, 0).unwrap();
        assert_eq!(e[0].programs(), vec![[4, 0xff, 0x4d, 0xff]]);
        assert_eq!((e[1].op, e[1].flags()), (0xc, vec![0x805]));
        assert!(progress::effects(&d, 1).unwrap().is_empty());
        assert_eq!(progress::flag_count(&d), 2);
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
