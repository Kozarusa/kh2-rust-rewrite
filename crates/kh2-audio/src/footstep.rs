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
    /// The landing path of FUN_0017af10 (inferred from where it is called).
    pub const LAND: u8 = 8;
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

#[cfg(test)]
mod tests {
    use super::*;

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
