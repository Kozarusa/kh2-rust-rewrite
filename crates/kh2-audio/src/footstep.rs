//! Footstep sound numbers: the game's rule for which sound effect a footstep, run step or landing
//! plays (frame trigger 2 of a motion, FUN_0016baa0 in the USA executable).
//!
//! The sound number is `bank * 1000 + material * 16 + variant`. The bank comes from the area's
//! record of `12soundinfo.bar` (one BAR entry per world, named by the world code; each entry is a
//! u32 count and `count` 24-byte records, one per area number) and is picked by the actor's
//! `NeoStatus`. The material is the ground polygon's material byte (see `kh2_map::Polygon`). The
//! variant is the trigger's `kind`, bumped by a per-actor step counter for kinds 0 and 4 (run and
//! walk steps cycle through four sounds) and kind 0xd (alternates two).

/// The five bank numbers of one area record (the shorts at +0xe, +0x10, +0x12, +0x14, +0x16 of the
/// 24-byte record). A negative value means "no bank".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundInfoRecord {
    pub banks: [i16; 5],
}

impl SoundInfoRecord {
    /// The bank the game uses for an actor with `neo_status` (`objentry` +0x4c): 1 and 0xe use the
    /// first short, 2 the second, 3 the third, 4 the fifth, any other value below 0x10 the fourth,
    /// 0x10 and above the fifth. `None` when the record holds no bank there.
    pub fn bank(&self, neo_status: u16) -> Option<u16> {
        let slot = match neo_status {
            1 | 0xe => 0,
            2 => 1,
            3 => 2,
            4 => 4,
            n if n < 0x10 => 3,
            _ => 4,
        };
        // Inferred: the game forms bank * 1000 + ..., so a negative bank gives a negative number
        // that no sound bank answers. A bank of 0 is treated as absent the same way.
        u16::try_from(self.banks[slot]).ok().filter(|&b| b > 0)
    }
}

/// One world's entry of `12soundinfo.bar`: the area records.
#[derive(Debug, Clone, Default)]
pub struct WorldSoundInfo {
    pub records: Vec<SoundInfoRecord>,
}

impl WorldSoundInfo {
    pub fn parse(entry: &[u8]) -> Option<WorldSoundInfo> {
        let count = u32::from_le_bytes(entry.get(0..4)?.try_into().ok()?) as usize;
        let mut records = Vec::with_capacity(count);
        for i in 0..count {
            let r = entry.get(4 + 24 * i..4 + 24 * (i + 1))?;
            let s = |o: usize| i16::from_le_bytes([r[o], r[o + 1]]);
            records.push(SoundInfoRecord { banks: [s(0xe), s(0x10), s(0x12), s(0x14), s(0x16)] });
        }
        Some(WorldSoundInfo { records })
    }

    /// The record of area number `area` (the digits of the area name: `tt02` is area 2), or `None`
    /// when the world has fewer areas.
    pub fn area(&self, area: usize) -> Option<&SoundInfoRecord> {
        self.records.get(area)
    }
}

/// Which `kind` values the frame trigger carries are listed on the motions; these are the ones
/// the executable's own callers use besides the trigger parameter.
pub mod kind {
    /// The stop step of FUN_0017af10 ([`super::stop_step`]): walking or running with no movement
    /// command left. (Earlier docs called it the landing path; it is not.)
    pub const STOP: u16 = 8;
}

/// The motion ids the ground mover's stop step looks for: walk and run.
pub const MOTION_WALK: u32 = 1;
pub const MOTION_RUN: u32 = 2;

/// FUN_0017af10, the per-tick update of the ground mover (descriptor class 0x35cbb0, mover 0, and
/// 0x371298, mover 0x2b; slot 6, called by FUN_0016a2f0 every tick): with no movement command
/// (`commanded_speed`, actor +0x1c, the walk or run speed FUN_001774d8 writes from the stick, 0
/// without input), for a party actor (`party`, +0x588 bit 0x80) whose last requested motion id
/// (+0x154, controller +0x14, written by FUN_0017e000) is walk (1) or run (2) and whose motion has no
/// range trigger 42 open (`range_42`, +0x164 bit 0, set by FUN_00188030 case 0x2a) it plays footstep
/// kind 8 (FUN_0016baa0) and the dust effects 8 and 9 (FUN_001b1c90, not for NeoStatus 0xe). The same
/// call ends with FUN_0017aeb8, which asks the controller for motion 0 (idle) while range trigger 0 is
/// open (controller +0x20 bit 4; walk and run have it over every frame), so +0x154 becomes 0 and the
/// step is one-shot: it marks the tick the stick is let go. True when the step plays.
pub fn stop_step(party: bool, commanded_speed: f32, range_42: bool, motion: u32) -> bool {
    commanded_speed <= 0.0 && party && !range_42 && matches!(motion, MOTION_WALK | MOTION_RUN)
}

/// Material forced by bit 0x20 of the class descriptor's flag word (`*(actor + 0xc) + 8`).
pub const FORCED_MATERIAL: u8 = 0x12;
/// The descriptor flag bit of [`FORCED_MATERIAL`].
pub const DESCRIPTOR_FORCED_MATERIAL: u32 = 0x20;

/// FUN_00183730: the material of an actor, `ground` being the byte at actor +0x648. Which
/// descriptors carry bit 0x20 is not known (no writer of that bit was found); every actor the app
/// has so far passes 0.
pub fn actor_material(descriptor_flags: u32, ground: u8) -> u8 {
    if descriptor_flags & DESCRIPTOR_FORCED_MATERIAL != 0 { FORCED_MATERIAL } else { ground }
}

/// The sound number FUN_0016baa0 plays, or `None` when it plays nothing (material 0, or no bank).
///
/// `counter` is the actor's step counter (the byte at actor +0x7fc, starts at 0): kinds 0 and 4
/// use `kind + (counter & 3)` and kind 0xd uses `0xd + (counter & 1)`, all three advance it;
/// other kinds are used as given. `material` is FUN_00183730's value (the ground material, or
/// 0x12 for actors whose class descriptor has bit 0x20 set).
pub fn footstep_number(record: Option<&SoundInfoRecord>, neo_status: u16, material: u8, kind: u16, counter: &mut u8) -> Option<u32> {
    if material == 0 {
        return None;
    }
    let bank = record?.bank(neo_status);
    // The counter advances after the record check and before the bank is used.
    let variant = match kind {
        0 | 4 => {
            let c = *counter;
            *counter = c.wrapping_add(1);
            kind + (c & 3) as u16
        }
        0xd => {
            let c = *counter;
            *counter = c.wrapping_add(1);
            0xd + (c & 1) as u16
        }
        k => k,
    };
    Some(bank? as u32 * 1000 + material as u32 * 16 + variant as u32)
}

/// The dust kinds `FUN_0017af10` asks `FUN_001b1c90` for (the motions' trigger 19 carries the other
/// kinds in its first parameter; an even kind is the left foot, an odd one the right).
pub mod dust_kind {
    /// Stop step (the stick is let go while walking or running): left, right.
    pub const STOP: [u16; 2] = [8, 9];
    /// First step out of idle (a movement command while the last motion request was idle): left, right.
    pub const START: [u16; 2] = [10, 11];
    /// Last kind that places the dust at a foot (FUN_001b1c90's switch covers 0..=0x13); a larger
    /// kind starts at the actor's own position and facing.
    pub const LAST_FOOT: u16 = 0x13;
    /// The NeoStatus the stop and start dust skip (`FUN_001699a8(actor) == 0xe`).
    pub const NO_DUST_NEO_STATUS: u16 = 0xe;
}

/// The start-to-move dust of `FUN_0017af10` (its commanded branch): a party actor whose last
/// requested motion id (+0x154) is idle (0) while a movement command is held (+0x1c above 0) starts
/// kinds 10 and 11, unless its NeoStatus is 0xe. There is no sound and no range-trigger test; the
/// same call ends by requesting walk or run, so it fires on the tick the stick goes out.
pub fn start_dust(party: bool, commanded_speed: f32, motion: u32, neo_status: u16) -> bool {
    commanded_speed > 0.0 && party && motion == 0 && neo_status != dust_kind::NO_DUST_NEO_STATUS
}

/// What `FUN_001b1c90` reads from the executable: the byte table at 0x37b9b8 (ground material to dust
/// class, 0 = no dust on that floor), the two probe distances (0x37b598 above the foot, 0x37b59c
/// below it, in centimetres) and the foot-bone reach factor (0x37b9d8, which multiplies the bone
/// length of the foot point).
#[derive(Clone, Debug, PartialEq)]
pub struct DustRules {
    classes: Vec<u8>,
    /// Centimetres above the foot point where the ground probe starts.
    pub probe_above: f32,
    /// Centimetres below the foot point where it ends.
    pub probe_below: f32,
    /// Factor on the foot bone's length for the foot point (`FUN_0016adb0`'s first argument).
    pub foot_reach: f32,
}

const DUST_CLASSES: u32 = 0x37b9b8;
const DUST_CLASS_COUNT: usize = 32;
const DUST_ABOVE: u32 = 0x37b598;
const DUST_BELOW: u32 = 0x37b59c;
const DUST_REACH: u32 = 0x37b9d8;

impl DustRules {
    /// Reads the table and the constants from the USA executable (`None` for another file).
    pub fn from_exe(exe: &[u8]) -> Option<DustRules> {
        let at = crate::voice::elf_at(exe, DUST_CLASSES)?;
        let classes = exe.get(at..at + DUST_CLASS_COUNT)?.to_vec();
        let f32_at = |va: u32| {
            let o = crate::voice::elf_at(exe, va)?;
            Some(f32::from_le_bytes(exe.get(o..o + 4)?.try_into().ok()?))
        };
        Some(DustRules { classes, probe_above: f32_at(DUST_ABOVE)?, probe_below: f32_at(DUST_BELOW)?, foot_reach: f32_at(DUST_REACH)? })
    }

    /// The dust class of a ground material (`FUN_00183730`'s value); 0 means no dust. The game
    /// indexes the table without a bound; a material past its 32 entries is read as 0 here.
    pub fn class(&self, material: u8) -> u8 {
        self.classes.get(material as usize).copied().unwrap_or(0)
    }

    /// The category bits the effect must carry (`FUN_001db9e0`'s mask): bit `class` and bit
    /// `kind + 0x20`. `None` when the class is 0 (the game returns before the lookup).
    pub fn mask(&self, material: u8, kind: u16) -> Option<u64> {
        let class = self.class(material);
        (class != 0).then(|| 1u64.wrapping_shl(class as u32) | 1u64.wrapping_shl(kind as u32 + 0x20))
    }

    /// Whether `kind` places the dust at a foot (and probes the ground there).
    pub fn at_foot(kind: u16) -> bool {
        kind <= dust_kind::LAST_FOOT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_step_gate_of_fun_0017af10() {
        // Walk or run with the stick let go: the step.
        assert!(stop_step(true, 0.0, false, MOTION_WALK));
        assert!(stop_step(true, 0.0, false, MOTION_RUN));
        // Still commanded, idle, landing (5) or any other motion: nothing.
        assert!(!stop_step(true, 8.0, false, MOTION_RUN));
        for m in [0, 3, 4, 5, 6, 194] {
            assert!(!stop_step(true, 0.0, false, m));
        }
        // Range trigger 42 open, or not a party actor: nothing.
        assert!(!stop_step(true, 0.0, true, MOTION_WALK));
        assert!(!stop_step(false, 0.0, false, MOTION_WALK));
    }

    #[test]
    fn start_dust_gate_of_fun_0017af10() {
        // A held command, a party actor, last request idle: kinds 10 and 11 start.
        assert!(start_dust(true, 8.0, 0, 1));
        // Not commanded, already walking or running, not a party actor, or NeoStatus 0xe: nothing.
        assert!(!start_dust(true, 0.0, 0, 1));
        assert!(!start_dust(true, 8.0, MOTION_WALK, 1));
        assert!(!start_dust(true, 8.0, MOTION_RUN, 1));
        assert!(!start_dust(false, 8.0, 0, 1));
        assert!(!start_dust(true, 8.0, 0, 0xe));
        assert_eq!(dust_kind::STOP.map(|k| k % 2), [0, 1]);
        assert_eq!(dust_kind::START.map(|k| k % 2), [0, 1]);
    }

    #[test]
    fn dust_masks_select_class_and_kind_bits() {
        let rules = DustRules { classes: vec![0, 1, 0, 2, 3], probe_above: 50.0, probe_below: 100.0, foot_reach: 1.0 };
        assert_eq!(rules.class(1), 1);
        assert_eq!(rules.class(3), 2);
        // Material 0 and class-0 materials start nothing; a material past the table too.
        assert_eq!(rules.mask(0, 8), None);
        assert_eq!(rules.mask(2, 8), None);
        assert_eq!(rules.mask(200, 8), None);
        // Bit `class` and bit `kind + 0x20`.
        assert_eq!(rules.mask(1, 0), Some(1 << 1 | 1 << 0x20));
        assert_eq!(rules.mask(3, 9), Some(1 << 2 | 1 << 0x29));
        assert_eq!(rules.mask(4, 0x13), Some(1 << 3 | 1 << 0x33));
        assert!(DustRules::at_foot(0x13) && !DustRules::at_foot(0x14));
    }

    fn entry() -> Vec<u8> {
        // Two areas; the second has banks 697, 698, 699, -1, 729.
        let mut b = 2u32.to_le_bytes().to_vec();
        for banks in [[1i16, 2, 3, 4, 5], [697, 698, 699, -1, 729]] {
            let mut r = [0u8; 24];
            for (i, v) in banks.iter().enumerate() {
                r[0xe + 2 * i..0x10 + 2 * i].copy_from_slice(&v.to_le_bytes());
            }
            b.extend(r);
        }
        b
    }

    #[test]
    fn bank_by_neo_status() {
        let w = WorldSoundInfo::parse(&entry()).unwrap();
        let r = w.area(1).unwrap();
        assert_eq!(r.bank(1), Some(697));
        assert_eq!(r.bank(0xe), Some(697));
        assert_eq!(r.bank(2), Some(698));
        assert_eq!(r.bank(3), Some(699));
        assert_eq!(r.bank(4), Some(729));
        assert_eq!(r.bank(5), None);
        assert_eq!(r.bank(0xf), None);
        assert_eq!(r.bank(0x10), Some(729));
        assert!(w.area(2).is_none());
    }

    #[test]
    fn variants_cycle_with_the_counter() {
        let w = WorldSoundInfo::parse(&entry()).unwrap();
        let r = w.area(1);
        let mut c = 0;
        // Walk (kind 4): 4, 5, 6, 7, 4 ... on material 1; run (kind 0) shares the counter.
        let walk: Vec<_> = (0..5).map(|_| footstep_number(r, 1, 1, 4, &mut c).unwrap()).collect();
        assert_eq!(walk, [697_020, 697_021, 697_022, 697_023, 697_020]);
        assert_eq!(footstep_number(r, 1, 1, 0, &mut c), Some(697_016 + 1));
        // Landing and the other kinds are used as given and leave the counter alone.
        let before = c;
        assert_eq!(footstep_number(r, 1, 7, 9, &mut c), Some(697_000 + 7 * 16 + 9));
        assert_eq!(c, before);
        // Kind 0xd alternates two sounds.
        let mut c = 0;
        assert_eq!(footstep_number(r, 1, 1, 0xd, &mut c), Some(697_016 + 0xd));
        assert_eq!(footstep_number(r, 1, 1, 0xd, &mut c), Some(697_016 + 0xe));
    }

    #[test]
    fn descriptor_bit_forces_material_0x12() {
        assert_eq!(actor_material(0x20, 3), 0x12);
        assert_eq!(actor_material(0x20, 0), 0x12);
        assert_eq!(actor_material(0xdf, 3), 3);
        let w = WorldSoundInfo::parse(&entry()).unwrap();
        let mut c = 0;
        // A forced actor sounds material 0x12 even in the air (ground material 0).
        assert_eq!(footstep_number(w.area(1), 1, actor_material(0x20, 0), 9, &mut c), Some(697_000 + 0x12 * 16 + 9));
    }

    #[test]
    fn silent_cases() {
        let w = WorldSoundInfo::parse(&entry()).unwrap();
        let mut c = 0;
        assert_eq!(footstep_number(w.area(1), 1, 0, 4, &mut c), None, "material 0 is silent");
        assert_eq!(footstep_number(None, 1, 1, 4, &mut c), None, "no record");
        assert_eq!(footstep_number(w.area(1), 5, 1, 4, &mut c), None, "no bank for this NeoStatus");
    }
}
