//! The area-script interpreter (FUN_00181148 / FUN_00180a68 / FUN_00180ad8 / FUN_00180b48) as a
//! pure function of the area file, the three program ids and the entrance, plus the pieces of the
//! spawn-group runtime that area transitions need: which entities of a group are created for an
//! entrance (FUN_001981a0), exit groups (FUN_00198e10) and the activator volume tests
//! (FUN_001d8600 builds the volume, FUN_001d89a0 box, FUN_001d8a70 cylinder, FUN_00334388 shape 2).
//! docs/events.md sections 1 and 2.

use crate::{parse_trigger, Area, Command, Entity, Group, Trigger, OP_SPAWN};

/// The three BAR tags of the area scripts, in the order FUN_00181148 runs them (strings at
/// 0x37a850, 0x37a858, 0x37a860).
pub const SCRIPTS: [&str; 3] = ["map", "btl", "evt"];

/// A spawn group an area-script command asked for (FUN_00181728(tag, alt)).
#[derive(Debug, Clone, PartialEq)]
pub struct GroupLoad {
    /// Which script (`map`, `btl`, `evt`) loaded it.
    pub script: &'static str,
    pub program: u16,
    /// The spawn file's BAR tag.
    pub tag: String,
    /// Loaded by opcode 9 (FUN_00181728(tag, 1)).
    pub alt: bool,
}

/// What running an area's three scripts produced: the state FUN_00181148 leaves behind. Fields
/// keep the game's defaults when no command wrote them.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptRun {
    pub groups: Vec<GroupLoad>,
    /// Triggers defined by opcode 0xc, in order.
    pub triggers: Vec<Trigger>,
    /// Opcode 0x10: the (field, battle) music pair (`DAT_01c95638`); `None` keeps the previous pair.
    pub music: Option<[u16; 2]>,
    /// `DAT_0035cd10` after the end-of-run `(word | show) & !hide` (FUN_00181148).
    pub map_visibility: u64,
    /// Opcode 0x13 (`DAT_0035cd38`, default 0).
    pub camera_type: u32,
    /// Opcode 4 (`DAT_01c893b8`, default 10000.0 at 0x37a868).
    pub capacity: f32,
    /// Opcode 0x19 arguments (story flags to set when not yet set; E5 applies them).
    pub story_flags: Vec<u32>,
    /// Opcode 0x1d arguments (FUN_001abfe8 party reset kinds).
    pub party_resets: Vec<u32>,
    /// Opcode 0xf arguments (FUN_001a7330 on the current world record).
    pub world_records: Vec<u32>,
    /// Opcode 0x15 (FUN_001b9338): the battle manager kind and its other arguments.
    pub battle_manager: Option<(u32, Vec<u32>)>,
    /// Opcodes this port reads but does not act on (random group picks 2, 3, 0x1f; 0xa; 0x16): the
    /// app logs them.
    pub unported: Vec<u16>,
}

impl Default for ScriptRun {
    fn default() -> Self {
        ScriptRun {
            groups: Vec::new(),
            triggers: Vec::new(),
            music: None,
            map_visibility: u64::MAX,
            camera_type: 0,
            capacity: 10000.0,
            story_flags: Vec::new(),
            party_resets: Vec::new(),
            world_records: Vec::new(),
            battle_manager: None,
            unported: Vec::new(),
        }
    }
}

/// Runs the `map`, `btl` and `evt` scripts of `area` with the program ids `programs` (bytes 3..5 of
/// the area record, 0x33e833) for `entrance` (byte 2, read by opcode 0x1c), as FUN_00181148 does.
/// A script without a program of that id does nothing (FUN_00180a68 stops at id 0xffff).
pub fn run_area(area: &Area, programs: [u8; 3], entrance: u8) -> ScriptRun {
    let mut st = ScriptRun::default();
    let (mut show, mut hide) = (0u64, 0u64);
    for (k, script) in SCRIPTS.iter().enumerate() {
        // FUN_00180a68 compares the program's signed halfword id with the record byte.
        let Some(p) = area.scripts.get(*script).and_then(|ps| ps.iter().find(|p| p.id == programs[k] as u16)) else { continue };
        run_commands(&p.commands, script, p.id, entrance, &mut st, &mut show, &mut hide);
    }
    st.map_visibility = (st.map_visibility | show) & !hide;
    st
}

/// FUN_00180ad8: runs commands in order until one answers "stop" (returns false then).
fn run_commands(cmds: &[Command], script: &'static str, program: u16, entrance: u8, st: &mut ScriptRun, show: &mut u64, hide: &mut u64) -> bool {
    for c in cmds {
        if !run_command(c, script, program, entrance, st, show, hide) {
            return false;
        }
    }
    true
}

fn tag(w: u32) -> String {
    w.to_le_bytes().iter().take_while(|&&b| b != 0).map(|&b| b as char).collect()
}

/// One command (FUN_00180b48). Returns false where the game returns 0 (the program stops).
fn run_command(c: &Command, script: &'static str, program: u16, entrance: u8, st: &mut ScriptRun, show: &mut u64, hide: &mut u64) -> bool {
    let a = &c.args;
    let arg = |k: usize| a.get(k).copied().unwrap_or(0);
    match c.opcode {
        OP_SPAWN => {
            for w in a {
                st.groups.push(GroupLoad { script, program, tag: tag(*w), alt: false });
            }
        }
        1 => st.map_visibility = arg(0) as u64 | (arg(1) as u64) << 32,
        // 2 (random pick), 3 (chance, may stop), 0x1f (lottery, may stop): random group loads. Not
        // used by any Twilight Town area; this port takes the "not picked" branch (3 and 0x1f
        // then continue) and records the opcode.
        2 | 3 | 0x1f | 0xa | 0x16 => st.unported.push(c.opcode),
        4 => st.capacity = arg(0) as f32,
        9 => {
            st.groups.push(GroupLoad { script, program, tag: tag(arg(0)), alt: true });
            return true;
        }
        0xc => {
            if let Some(t) = parse_trigger(a) {
                st.triggers.push(t);
            }
        }
        0xf => st.world_records.push(arg(0)),
        0x10 => st.music = Some([arg(0) as u16, (arg(0) >> 16) as u16]),
        0x13 => st.camera_type = arg(0),
        0x15 => st.battle_manager = Some((arg(0), a.get(1..).unwrap_or_default().to_vec())),
        // [inferred] every argument (the decompiled loop's count read is ambiguous; docs/events.md).
        0x19 => st.story_flags.extend_from_slice(a),
        0x1a => a.iter().for_each(|b| *show |= 1u64.checked_shl(*b).unwrap_or(0)),
        0x1b => a.iter().for_each(|b| *hide |= 1u64.checked_shl(*b).unwrap_or(0)),
        0x1c => {
            // The rest of the arguments are commands run only for this entrance; their "stop" is
            // not passed on (case 0x1c returns 1).
            if arg(0) == entrance as u32 {
                let bytes: Vec<u8> = a.iter().skip(1).flat_map(|w| w.to_le_bytes()).collect();
                if let Some(nested) = crate::parse_commands(&bytes) {
                    run_commands(&nested, script, program, entrance, st, show, hide);
                }
            }
        }
        0x1d => st.party_resets.extend_from_slice(a),
        _ => {}
    }
    true
}

/// FUN_001981a0: whether an entity of a spawn group is created for `entrance`. Spawn kind 1 means
/// "only at this entrance" (+0x1d); every other kind is always created. (The game also drops
/// objects it cannot find in `00objentry` and, during an event with `DAT_0035e3a0` set, objects
/// whose class is 2000 or above; neither applies to the player.)
pub fn entity_for_entrance(e: &Entity, entrance: u8) -> bool {
    e.spawn_kind != 1 || e.spawn_arg == entrance
}

/// The player's `00objentry` id in spawn records (`PLAYER`).
pub const PLAYER: u32 = 0x236;

/// Where the player starts for `entrance`: the first player entity (in script and group order) of
/// the loaded groups that FUN_001981a0 creates for it, with the group tag.
pub fn player_start<'a>(area: &'a Area, run: &ScriptRun, entrance: u8) -> Option<(&'a str, &'a Entity)> {
    for g in &run.groups {
        let Some((tag, groups)) = area.spawns.get_key_value(&g.tag) else { continue };
        for grp in groups {
            if let Some(e) = grp.entities.iter().find(|e| e.object == PLAYER && entity_for_entrance(e, entrance)) {
                return Some((tag.as_str(), e));
            }
        }
    }
    None
}

/// An exit (a kind-1 group with activators; FUN_00198e10).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Exit {
    /// Header +0x1c: the destination area in the current world; 100 means world 0x12 area 1, 101
    /// world 2 area 0x28.
    pub area: u8,
    /// Header +0x1d: the destination entrance.
    pub entrance: u8,
    /// Header +0x1e: 0 the exit is always open; else it does nothing while the battle state is 2,
    /// and the walk-in volume also needs FUN_001d88a0's player tests (FUN_001d8908, FUN_001d8958).
    pub gate: u8,
}

impl Exit {
    /// The destination (world, area) for the current world (FUN_00198e10).
    pub fn destination(&self, world: u8) -> (u8, u8) {
        match self.area {
            100 => (0x12, 1),
            101 => (2, 0x28),
            a => (world, a),
        }
    }
}

impl Group {
    /// The exit this group is (FUN_00198b30 kind 1 with activators), or `None`.
    pub fn exit(&self) -> Option<Exit> {
        (self.kind & 0xff == 1 && !self.activators.is_empty()).then(|| Exit { area: self.header[0x1c], entrance: self.header[0x1d], gate: self.header[0x1e] })
    }

    /// Whether the group starts disabled (FUN_00198430: kind 5, or flag bit 0 of byte +1).
    pub fn starts_disabled(&self) -> bool {
        self.kind & 0xff == 5 || self.flags() & 1 != 0
    }

    /// The activators FUN_00198750 turns into volumes: all of them for kind 6, else those whose
    /// option (+2) is not 3 or 4.
    pub fn volumes(&self) -> impl Iterator<Item = &crate::Activator> {
        let kind6 = self.kind & 0xff == 6;
        self.activators.iter().filter(move |a| kind6 || !(3..=4).contains(&a.option))
    }
}

impl crate::Activator {
    /// The point `p` (game space) in the volume's own frame: FUN_001d8600 builds the matrix of a
    /// turn by -(rotation y) about Y placed at the position and keeps its inverse; the tests
    /// multiply the point by that inverse.
    pub fn local(&self, p: [f32; 3]) -> [f32; 3] {
        let r = -self.rotation[1];
        let (s, c) = r.sin_cos();
        let d = [p[0] - self.position[0], p[1] - self.position[1], p[2] - self.position[2]];
        // Row-vector convention (VU0: v' = x*row0 + y*row1 + z*row2 + row3) with the rotation rows
        // (c, 0, -s), (0, 1, 0), (s, 0, c); the inverse applies the transpose.
        [d[0] * c - d[2] * s, d[1], d[0] * s + d[2] * c]
    }

    /// Whether `p` (game space) is inside: shape 0 box (|x| <= sx, |y| <= sy, |z| <= sz,
    /// FUN_001d89a0), 1 cylinder (|y| <= sy and (x/sx)^2 + (z/sz)^2 <= 1, FUN_001d8a70), 2 always
    /// (FUN_00334388). Other shapes make no volume (FUN_001d8b48).
    pub fn contains(&self, p: [f32; 3]) -> bool {
        let [x, y, z] = self.local(p);
        let [sx, sy, sz] = self.scale;
        match self.shape {
            0 => x.abs() <= sx && y.abs() <= sy && z.abs() <= sz,
            1 => y.abs() <= sy && (x / sx) * (x / sx) + (z / sz) * (z / sz) <= 1.0,
            2 => true,
            _ => false,
        }
    }
}

/// The parts of a trigger (opcode 0xc) the app acts on.
impl Trigger {
    /// Sub 0: the event tag (four characters) and its argument.
    pub fn event_tag(&self) -> Option<(String, u16)> {
        self.subs.iter().find(|(op, _)| *op == 0).map(|(_, h)| {
            let w = h.get(1).copied().unwrap_or(0) as u32 | (h.get(2).copied().unwrap_or(0) as u32) << 16;
            (tag(w), h.first().copied().unwrap_or(0))
        })
    }

    /// Sub 1: the jump record (type, world, area, entrance, key, flags; FUN_0019d8f0).
    pub fn jump(&self) -> Option<Jump> {
        self.subs.iter().find(|(op, _)| *op == 1).and_then(|(_, h)| Some(Jump { kind: *h.get(1)?, world: *h.get(2)?, area: *h.get(3)?, entrance: *h.get(4)?, key: *h.get(5)?, flags: *h.get(6)? }))
    }

    /// Whether the trigger answers `(event, arg)` (FUN_00197588: the same event and the same arg, or
    /// a trigger defined with arg -1).
    pub fn matches(&self, event: u16, arg: u16) -> bool {
        self.event == event && (self.arg == arg || self.arg == 0xffff)
    }
}

/// A jump record (trigger sub 1; FUN_0019d9d8 acts on it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jump {
    /// 1 to (world, area, entrance); 2 to the area `07localset.bin` gives for `key`; 3 back to
    /// where the last exit pointed; 4 the world map; 5 menu 0xe; 6 FUN_00149f38.
    pub kind: u16,
    pub world: u16,
    pub area: u16,
    pub entrance: u16,
    pub key: u16,
    pub flags: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Activator, Program};

    fn act(shape: u16, position: [f32; 3], scale: [f32; 3], ry: f32) -> Activator {
        Activator { shape, option: 0, position, scale, rotation: [0.0, ry, 0.0], raw: [0; 0x40] }
    }

    #[test]
    fn activator_volumes_test_box_and_cylinder() {
        let b = act(0, [100.0, 0.0, 0.0], [50.0, 100.0, 10.0], 0.0);
        assert!(b.contains([140.0, 50.0, 5.0]));
        assert!(!b.contains([160.0, 0.0, 0.0]));
        assert!(!b.contains([100.0, 0.0, 11.0]));
        // A quarter turn swaps the long and short sides.
        let r = act(0, [0.0; 3], [50.0, 100.0, 10.0], std::f32::consts::FRAC_PI_2);
        assert!(r.contains([0.0, 0.0, 40.0]) && !r.contains([40.0, 0.0, 0.0]));
        let c = act(1, [0.0; 3], [100.0, 50.0, 50.0], 0.0);
        assert!(c.contains([90.0, 0.0, 0.0]) && !c.contains([90.0, 0.0, 40.0]) && !c.contains([0.0, 60.0, 0.0]));
        assert!(act(2, [0.0; 3], [0.0; 3], 0.0).contains([1e6, 0.0, 0.0]));
    }

    #[test]
    fn the_interpreter_follows_entrance_and_stop_rules() {
        // map program 0: spawn "m_00", music (5, 6), at entrance 1 spawn "m_01"; btl program 0 is
        // missing; evt program 0: opcode 9 "e_00".
        let words = |s: &[u8; 4]| u32::from_le_bytes(*s);
        let mut nested = Vec::new();
        nested.extend_from_slice(&0u16.to_le_bytes());
        nested.extend_from_slice(&1u16.to_le_bytes());
        nested.extend_from_slice(b"m_01");
        let mut ent = vec![1u32];
        ent.extend(nested.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())));
        let map = Program { id: 0, commands: vec![Command { opcode: 0, args: vec![words(b"m_00")] }, Command { opcode: 0x10, args: vec![5 | 6 << 16] }, Command { opcode: 0x1c, args: ent }] };
        let evt = Program { id: 0, commands: vec![Command { opcode: 9, args: vec![words(b"e_00")] }] };
        let mut area = Area::default();
        area.scripts.insert("map".into(), vec![map]);
        area.scripts.insert("evt".into(), vec![evt]);
        let r0 = run_area(&area, [0, 0, 0], 0);
        assert_eq!(r0.groups.iter().map(|g| g.tag.as_str()).collect::<Vec<_>>(), ["m_00", "e_00"]);
        assert_eq!(r0.music, Some([5, 6]));
        assert!(r0.groups[1].alt);
        let r1 = run_area(&area, [0, 0, 0], 1);
        assert_eq!(r1.groups.iter().map(|g| g.tag.as_str()).collect::<Vec<_>>(), ["m_00", "m_01", "e_00"]);
        // Another program id runs nothing.
        assert!(run_area(&area, [1, 1, 1], 1).groups.is_empty());
    }
}
