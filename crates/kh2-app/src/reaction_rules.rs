//! Pure rules for how an attack's reflect/refact fields and reaction class change what happens
//! when a hit lands, hits a wall or clashes with another attack. No Bevy systems here: the
//! combat, Sora and enemy modules will call these functions. The rules and where they come from
//! are written up in `docs/hit-reaction.md`; the addresses quoted below are functions in the
//! game's main executable, studied in the local decompilation.
//!
//! Field meanings for `atkp` follow OpenKH `docs/kh2/file/type/00battle.md` (Apache-2.0, see
//! CREDITS.md); everything else was read from the game's code. Numbers the game keeps in its
//! executable (push scale, default recoil power, the flinch motion list) are read from the
//! user's own `SLUS_210.05` at run time by [`ExeConsts::from_exe`], never stored here.

// The rules are not wired into the game yet; the combat work packages will use them.
#![allow(dead_code)]

use kh2_tables::tables::{Atkp, PrefSstm};

/// `atkp.Flags` bit 0 ("BG Hit"): the attack volume is tested against ATTACK_COLLIDE geometry
/// (FUN_0018c8e0 returns early without it).
pub const ATKP_FLAG_BG_HIT: u8 = 1;

/// Motion IDs that the generic code requests by number (immediates in the code).
pub mod motion {
    /// Wall or clash recoil on the ground (FUN_00185d30 default). Slot = state + id * 4.
    pub const REFLECT_GROUND: u8 = 0x29;
    /// Guard bounce after a clash (FUN_001756c8).
    pub const AUTOGUARD: u8 = 0x2a;
    /// Wall or clash recoil when the actor counts as airborne (FUN_00185d30).
    pub const REFLECT_AIR: u8 = 0x2b;
    /// Blow reaction when the victim was hit from the front / from behind (FUN_00185460 callers).
    pub const BLOW_FRONT: u8 = 16;
    pub const BLOW_BACK: u8 = 18;
    /// Reaction class 14 motions, front / back (FUN_001856a8).
    pub const CLASS14_FRONT: u8 = 0x17;
    pub const CLASS14_BACK: u8 = 0x18;
}

/// `RefactSelf` / `RefactOther` values (OpenKH: 0 Reflect, 1 Guard, 2 Nothing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refact {
    Reflect,
    Guard,
    Nothing,
    /// Not used by any row in the USA disc; the game treats it like Guard (non-zero, not 2).
    Other(u8),
}

impl Refact {
    pub fn from_raw(v: u8) -> Refact {
        match v {
            0 => Refact::Reflect,
            1 => Refact::Guard,
            2 => Refact::Nothing,
            n => Refact::Other(n),
        }
    }
}

/// The reflect/refact part of an `atkp` row, decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReflectFields {
    /// 0x13: how the attacker reacts when this attack clashes with another attack.
    pub refact_self: Refact,
    /// 0x14: what this attack does to the owner of the attack it clashes with.
    pub refact_other: Refact,
    /// 0x15: motion ID the attacker plays after a clash (0 = none). Enemy rows use 140..143.
    pub reflected_motion: u8,
    /// 0x16: recoil push power for that motion; negative means "use the game's default".
    pub reflect_hit_back: i16,
    /// 0x18: behaviour-script action started after a clash (0 = none).
    pub reflect_action: i32,
    /// 0x1c: sound the attacker plays when this attack hits a victim; -1 = none (FUN_0018c6d0).
    pub reflect_hit_sound: i32,
    /// 0x20: stored on the attacker after a clash (FUN_00178708). Zero in every USA row.
    pub reflect_rc: u16,
    /// 0x22 and 0x23: cone (degrees) in which another attack counts as clashing (FUN_0018d308).
    pub reflect_range: u8,
    pub reflect_angle: i8,
    /// `Flags` bit 0.
    pub bg_hit: bool,
}

impl ReflectFields {
    pub fn from_atkp(a: &Atkp) -> ReflectFields {
        ReflectFields {
            refact_self: Refact::from_raw(a.refact_self),
            refact_other: Refact::from_raw(a.refact_other),
            reflected_motion: a.reflected_motion,
            reflect_hit_back: a.reflect_hit_back,
            reflect_action: a.reflect_action,
            reflect_hit_sound: a.reflect_hit_sound,
            reflect_rc: a.reflect_rc,
            reflect_range: a.reflect_range,
            reflect_angle: a.reflect_angle,
            bg_hit: a.flags & ATKP_FLAG_BG_HIT != 0,
        }
    }

    /// Sound number for a landed hit, if the row has one.
    pub fn hit_sound(&self) -> Option<u32> {
        u32::try_from(self.reflect_hit_sound).ok()
    }
}

// ---- Values the game keeps in its executable ----

/// Scalars and the flinch motion list the generic hit code reads from the executable's data.
/// Loaded from the user's `SLUS_210.05`; the addresses are the data words the code loads.
#[derive(Debug, Clone, PartialEq)]
pub struct ExeConsts {
    /// Power argument of the wall recoil call (0x37a4a4). Negative selects `default_power`.
    pub wall_power: f32,
    /// Power argument of the clash recoil call (0x37a4a0). Negative selects `default_power`.
    pub clash_power: f32,
    /// Recoil power used when the argument is negative (0x37a964).
    pub default_power: f32,
    /// Power of the guard bounce push (0x37a4ac).
    pub guard_power: f32,
    /// Push power to initial speed factor (0x37a954).
    pub push_scale: f32,
    /// Speed is dropped to zero once it falls below this (0x37a958).
    pub push_cutoff: f32,
    /// Per-tick speed decay of a push (0x37a95c).
    pub push_decay: f32,
    /// Push is multiplied by this when the effective reaction is 12 or 13 (0x37ac74).
    pub reduced_scale: f32,
    /// Flinch motion IDs: [front a, front b, back a, back b] (u16 x4 at 0x35ce40).
    pub flinch_motions: [u16; 4],
}

impl ExeConsts {
    /// Reads the values from the main executable (an ELF32). `None` if an address is not mapped.
    pub fn from_exe(elf: &[u8]) -> Option<ExeConsts> {
        let f = |va| exe_f32(elf, va);
        let mut flinch_motions = [0u16; 4];
        for (i, m) in flinch_motions.iter_mut().enumerate() {
            *m = exe_u16(elf, 0x35ce40 + 2 * i as u32)?;
        }
        Some(ExeConsts {
            wall_power: f(0x37a4a4)?,
            clash_power: f(0x37a4a0)?,
            default_power: f(0x37a964)?,
            guard_power: f(0x37a4ac)?,
            push_scale: f(0x37a954)?,
            push_cutoff: f(0x37a958)?,
            push_decay: f(0x37a95c)?,
            reduced_scale: f(0x37ac74)?,
            flinch_motions,
        })
    }
}

/// Bytes at a virtual address, via the ELF program headers.
fn exe_bytes(elf: &[u8], va: u32, len: usize) -> Option<&[u8]> {
    let u16_at = |o: usize| elf.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |o: usize| elf.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if elf.get(0..4)? != b"\x7fELF" {
        return None;
    }
    let phoff = u32_at(0x1c)? as usize;
    let phentsize = u16_at(0x2a)?;
    let phnum = u16_at(0x2c)?;
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        let (kind, off, vaddr, filesz) = (u32_at(p)?, u32_at(p + 4)?, u32_at(p + 8)?, u32_at(p + 16)?);
        if kind == 1 && va >= vaddr && (va - vaddr) as usize + len <= filesz as usize {
            let start = off as usize + (va - vaddr) as usize;
            return elf.get(start..start + len);
        }
    }
    None
}

fn exe_f32(elf: &[u8], va: u32) -> Option<f32> {
    exe_bytes(elf, va, 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn exe_u16(elf: &[u8], va: u32) -> Option<u16> {
    exe_bytes(elf, va, 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

// ---- Wall recoil (attack volume against ATTACK_COLLIDE geometry) ----

/// What an actor does when its attack volume touches a wall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallRecoil {
    /// Motion ID to request (slot = state + id * 4): 0x29 normally, 0x2b when airborne.
    pub motion: u8,
    /// Push power. Ground: pushed backwards along its own facing (FUN_001857b0). Air: the jump-back
    /// push (FUN_00185978). Zero or less means no push.
    pub push_power: f32,
    pub airborne: bool,
    /// The combo step counter at actor+0xb04 goes down by one (FUN_00176aa0), so the swing that
    /// bounced does not use up a combo step.
    pub refund_combo_step: bool,
}

/// Decides the recoil for a wall hit, as the generic actor handler does (vtable +0x70 =
/// FUN_00175668, which calls FUN_00185d30 with the "no motion override" value).
///
/// * `already_reacted`: the volume has flag 0x40 (it already caused a clash or wall reaction).
/// * `airborne`: FUN_0019aa28 for the actor.
///
/// This does not depend on `RefactSelf`, `ReflectedMotion` or `ReflectHitBack`: only on the row's
/// BG Hit flag, which is what makes the wall test run at all.
pub fn wall_recoil(f: &ReflectFields, already_reacted: bool, airborne: bool, c: &ExeConsts) -> Option<WallRecoil> {
    if !f.bg_hit || already_reacted {
        return None;
    }
    Some(WallRecoil {
        motion: if airborne { motion::REFLECT_AIR } else { motion::REFLECT_GROUND },
        push_power: recoil_power(c.wall_power, c),
        airborne,
        refund_combo_step: true,
    })
}

/// FUN_00185d30: a negative power argument is replaced by the default.
pub fn recoil_power(arg: f32, c: &ExeConsts) -> f32 {
    if arg >= 0.0 {
        arg
    } else {
        c.default_power
    }
}

/// Backward push speed per tick (game units) for a push of `power` (+ the actor's own bonus term,
/// actor stat block +0x1c8, meaning unknown): FUN_001857b0 stores power * scale as the speed, and
/// the per-tick update (FUN_001858b0) moves by that speed, then multiplies it by the decay, until
/// it falls below the cutoff. Speeds are for a logic step of 1.0.
pub fn push_speeds(power: f32, c: &ExeConsts) -> Vec<f32> {
    let mut out = Vec::new();
    let mut speed = power * c.push_scale;
    // A zero or negative scale would never stop; the game's values are 0.1 and 0.9.
    while speed > 0.0 && speed >= c.push_cutoff && c.push_decay < 1.0 && out.len() < 1000 {
        out.push(speed);
        speed *= c.push_decay;
    }
    out
}

// ---- Attack against attack ("clash") ----

/// Outcome for a party member (Sora and friends, FUN_001754a0) whose attack volume `A` overlaps
/// another attack `B`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClashReaction {
    /// Only a spark; nothing happens to the actor (either side says Nothing).
    None,
    /// Recoil motion 0x29 / 0x2b with the default push; the combo step is refunded.
    Reflect,
    /// Guard bounce: motion 0x2a and a strong push (FUN_001756c8).
    Guard,
}

/// `own` is `A`'s `RefactSelf`, `other` is `B`'s `RefactOther`. `plain_ground` is true when the
/// actor is on the ground and neither the owner-type flag 2 nor actor flag 0x40 (+0x160) is set;
/// anything else always takes the Reflect branch.
pub fn party_clash(own: Refact, other: Refact, plain_ground: bool) -> ClashReaction {
    if own == Refact::Nothing || other == Refact::Nothing {
        return ClashReaction::None;
    }
    if !plain_ground {
        return ClashReaction::Reflect;
    }
    if own == Refact::Reflect && other == Refact::Reflect {
        ClashReaction::Reflect
    } else {
        ClashReaction::Guard
    }
}

/// What the other actor classes (enemies; the handler at 0x1b2478) do on a clash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnemyClash {
    /// Motion to request, from `ReflectedMotion`; `None` when the row has 0.
    pub motion: Option<u8>,
    /// `ReflectHitBack` as the recoil power argument (negative = default), applied only with a motion.
    pub push_power: f32,
    /// `ReflectAction` script action to start, if non-zero.
    pub action: Option<i32>,
}

/// `already_reacted` is volume flag 0x40. Returns `None` when the handler does nothing.
pub fn enemy_clash(f: &ReflectFields, already_reacted: bool) -> Option<EnemyClash> {
    if already_reacted || f.refact_self == Refact::Nothing {
        return None;
    }
    Some(EnemyClash {
        motion: (f.reflected_motion != 0).then_some(f.reflected_motion),
        push_power: f.reflect_hit_back as f32,
        action: (f.reflect_action != 0).then_some(f.reflect_action),
    })
}

// ---- Victim reaction ----

/// Reaction class after the game's collapse of `EnemyReaction` values (FUN_0018b700).
pub fn reaction_class(raw: u8) -> u8 {
    match raw {
        1 | 12 => 1,
        4 | 7 | 9 | 11 | 15 | 17 => 4,
        3 | 6 | 8 | 10 | 13 | 16 | 18 => 3,
        other => other,
    }
}

/// What the generic victim handler (FUN_00184e80) does for a class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactionClass {
    /// 0: nothing.
    None,
    /// 1: pushed away, no motion (FUN_00185098).
    Push,
    /// 2: flinch motion only (FUN_00185198).
    Flinch,
    /// 3: flinch plus push; in the air the air flinch (FUN_001850c8).
    FlinchPush,
    /// 4: blown away with an upward and a horizontal speed (FUN_00185460).
    Blow,
    /// 5: FUN_00185668 (pull or throw towards the attacker); not decoded further.
    Special5,
    /// 14: motion 0x17 / 0x18 plus push (FUN_001856a8).
    Special14,
    /// No handler: classes 6..=13 and 15 upwards.
    Unhandled(u8),
}

impl ReactionClass {
    pub fn from_raw(raw: u8) -> ReactionClass {
        match reaction_class(raw) {
            0 => ReactionClass::None,
            1 => ReactionClass::Push,
            2 => ReactionClass::Flinch,
            3 => ReactionClass::FlinchPush,
            4 => ReactionClass::Blow,
            5 => ReactionClass::Special5,
            14 => ReactionClass::Special14,
            n => ReactionClass::Unhandled(n),
        }
    }
}

/// The `sstm` fields the reaction code uses as defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReactionDefaults {
    pub hit_back: f32,
    pub hit_back_small: f32,
    pub blow_up: f32,
    pub blow_speed: f32,
    pub blow_to_hit_back: f32,
}

impl From<&PrefSstm> for ReactionDefaults {
    fn from(s: &PrefSstm) -> Self {
        ReactionDefaults {
            hit_back: s.hit_back,
            hit_back_small: s.hit_back_small,
            blow_up: s.blow_up,
            blow_speed: s.blow_speed,
            blow_to_hit_back: s.blow_to_hit_back,
        }
    }
}

/// Upward and horizontal launch speeds of a class 4 blow (FUN_0018b9e0 and FUN_001855a8). Note the
/// order: `KnockbackStrength1` is the upward speed, `KnockbackStrength2` the horizontal one. The
/// `sstm` defaults are used when both are zero.
pub fn blow_launch(kb1: i16, kb2: i16, d: &ReactionDefaults) -> (f32, f32) {
    if kb1 != 0 || kb2 != 0 {
        (kb1 as f32, kb2 as f32)
    } else {
        (d.blow_up, d.blow_speed)
    }
}

/// Push strength for the classes that use FUN_0018b7e8: 1, 3 and 14. `original` is the row's
/// `EnemyReaction`, `effective` the value after the state conversion table, `kb1` the row's
/// `KnockbackStrength1` (`None` when there is no row). A `kb1` of 0x7fff selects the `sstm` default
/// (HitBack for class 1 and 14, HitBackSmall for class 3); any other value is used as is, so zero
/// means no push. `None` for classes this does not cover (4 uses `blow_launch`; 5 is random).
pub fn push_strength(original: u8, effective: u8, kb1: Option<i16>, d: &ReactionDefaults, c: &ExeConsts) -> Option<f32> {
    let class = reaction_class(original);
    if class == 4 || class == 5 {
        return None;
    }
    let default = if class == 3 { d.hit_back_small } else { d.hit_back };
    let mut v = match kb1 {
        Some(k) if k != 0x7fff => k as f32,
        _ => default,
    };
    if effective == 12 || effective == 13 {
        v *= c.reduced_scale;
    }
    Some(v)
}

/// Flinch motion for class 2 and the ground part of class 3 (FUN_00185198). `from_front`: the
/// victim's facing opposes the hit direction. `hit_part_plain`: the hit record's byte at +0x24 is
/// one of 0, 2, 3, 4, 5 (FUN_00184e30; what that byte describes is not known).
pub fn flinch_motion(from_front: bool, hit_part_plain: bool, c: &ExeConsts) -> u16 {
    let mut i = if from_front { 0 } else { 2 };
    if !hit_part_plain {
        i += 1;
    }
    c.flinch_motions[i]
}

/// Motion of a class 4 blow (FUN_00185460).
pub fn blow_motion(from_front: bool) -> u8 {
    if from_front {
        motion::BLOW_FRONT
    } else {
        motion::BLOW_BACK
    }
}

// ---- State-dependent reaction conversion ("patn" table) ----

/// One record of the `patn` table in 00battle.bin: a status bit and a replacement for each
/// reaction value (0xff = leave as is). 15 records of 32 bytes after an 8 byte header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatnRow {
    pub status_bit: u8,
    pub map: [u8; 19],
}

/// Decodes the `patn` subfile.
pub fn parse_patn(b: &[u8]) -> Option<Vec<PatnRow>> {
    let count = u32::from_le_bytes(b.get(4..8)?.try_into().ok()?) as usize;
    let mut rows = Vec::with_capacity(count);
    for i in 0..count {
        let r = b.get(8 + 32 * i..8 + 32 * (i + 1))?;
        let mut map = [0u8; 19];
        map.copy_from_slice(&r[1..20]);
        rows.push(PatnRow { status_bit: r[0], map });
    }
    Some(rows)
}

/// The victim facts the conversion looks at (FUN_001bff00).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatnContext {
    /// (status word A | status word B) & !status word C of the victim (actor +0xabc, +0xac0, +0xac4).
    pub status: u32,
    /// Victim is on the ground and not flagged as airborne-like (inverse of the air check).
    pub grounded: bool,
    /// The attacker has flag 0x4000 in its flag word at +0x588.
    pub attacker_flag_4000: bool,
}

/// The effective reaction for a hit: the row's `EnemyReaction` run through every record whose
/// status bit is set, later records winning. Rows for bit 7 apply only on the ground, rows for
/// bit 8 only in the air, rows for bit 2 only when the attacker has flag 0x4000. Not applied to
/// hits already marked ineffective.
pub fn effective_reaction(original: u8, rows: &[PatnRow], ctx: &PatnContext) -> u8 {
    let mut eff = original;
    for r in rows {
        if ctx.status & (1u32 << (r.status_bit & 31)) == 0 {
            continue;
        }
        let applies = match r.status_bit {
            7 => ctx.grounded,
            8 => !ctx.grounded,
            2 => ctx.attacker_flag_4000,
            _ => true,
        };
        if !applies {
            continue;
        }
        // The lookup uses the original reaction, not the running value.
        if let Some(&m) = r.map.get(original as usize).filter(|&&m| m != 0xff) {
            eff = m;
        }
    }
    eff
}

#[cfg(test)]
mod tests {
    use super::*;
    use kh2_tables::tables as t;

    fn open_disc() -> Option<kh2_archive::Archive> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        kh2_archive::Archive::open(&iso).map_err(|e| eprintln!("skipped: {iso}: {e}")).ok()
    }

    fn atkp_rows(a: &mut kh2_archive::Archive) -> Vec<Atkp> {
        kh2_tables::load::<Atkp>(a, &t::ATKP).expect("atkp").rows
    }

    fn row(rows: &[Atkp], id: u16) -> &Atkp {
        rows.iter().find(|r| r.id == id && r.sub_id == 0).unwrap_or_else(|| panic!("atkp {id}"))
    }

    #[test]
    fn class_collapse_matches_the_code() {
        let expect = [0, 1, 2, 3, 4, 5, 3, 4, 3, 4, 3, 4, 1, 3, 14, 4, 3, 4, 3, 19, 20];
        for (raw, want) in expect.iter().enumerate() {
            assert_eq!(reaction_class(raw as u8), *want, "raw {raw}");
        }
        assert_eq!(ReactionClass::from_raw(12), ReactionClass::Push);
        assert_eq!(ReactionClass::from_raw(9), ReactionClass::Blow);
        assert_eq!(ReactionClass::from_raw(14), ReactionClass::Special14);
        assert_eq!(ReactionClass::from_raw(19), ReactionClass::Unhandled(19));
    }

    #[test]
    fn clash_rules() {
        use Refact::*;
        assert_eq!(party_clash(Nothing, Reflect, true), ClashReaction::None);
        assert_eq!(party_clash(Reflect, Nothing, false), ClashReaction::None);
        assert_eq!(party_clash(Reflect, Reflect, true), ClashReaction::Reflect);
        assert_eq!(party_clash(Reflect, Guard, true), ClashReaction::Guard);
        assert_eq!(party_clash(Guard, Reflect, true), ClashReaction::Guard);
        assert_eq!(party_clash(Guard, Guard, false), ClashReaction::Reflect);
    }

    #[test]
    fn push_profile_decays() {
        let c = ExeConsts {
            wall_power: -1.0,
            clash_power: -1.0,
            default_power: 50.0,
            guard_power: 200.0,
            push_scale: 0.1,
            push_cutoff: 1.0,
            push_decay: 0.9,
            reduced_scale: 0.5,
            flinch_motions: [10, 11, 12, 13],
        };
        let s = push_speeds(50.0, &c);
        assert!((s[0] - 5.0).abs() < 1e-5);
        assert!(s.windows(2).all(|w| w[1] < w[0]));
        assert!(*s.last().unwrap() >= 1.0);
        assert_eq!(s.len(), 16);
        assert!(push_speeds(0.0, &c).is_empty());
    }

    #[test]
    fn patn_conversion() {
        let mut map = [0xffu8; 19];
        map[4] = 0;
        let rows = vec![PatnRow { status_bit: 7, map }, PatnRow { status_bit: 8, map: [1; 19] }];
        let mut ctx = PatnContext { status: 1 << 7, grounded: true, attacker_flag_4000: false };
        assert_eq!(effective_reaction(4, &rows, &ctx), 0);
        ctx.grounded = false;
        assert_eq!(effective_reaction(4, &rows, &ctx), 4);
        ctx.status = 1 << 8;
        assert_eq!(effective_reaction(4, &rows, &ctx), 1);
        ctx.status = 0;
        assert_eq!(effective_reaction(4, &rows, &ctx), 4);
    }

    #[test]
    fn sora_combo_rows() {
        let Some(mut a) = open_disc() else { return };
        let rows = atkp_rows(&mut a);
        // Rows with bytes read straight from 00battle.bin: (id, bg_hit, refact_self, refact_other, hit sound).
        let sora = [
            (87, false, 2, 1, 500002),
            (88, false, 2, 1, -1),
            (124, false, 2, 0, -1),
            (125, false, 2, 1, -1),
            (126, true, 0, 0, 500000),
            (127, false, 0, 0, 500001),
            (128, true, 0, 0, 500002),
            (129, false, 0, 0, 500004),
            (130, false, 0, 0, 500003),
            (131, true, 0, 0, 500005),
            (147, false, 0, 0, 500001),
            (148, false, 2, 1, 500011),
        ];
        for (id, bg, rs, ro, snd) in sora {
            let f = ReflectFields::from_atkp(row(&rows, id));
            assert_eq!(f.bg_hit, bg, "atkp {id} bg_hit");
            assert_eq!(f.refact_self, Refact::from_raw(rs), "atkp {id} refact_self");
            assert_eq!(f.refact_other, Refact::from_raw(ro), "atkp {id} refact_other");
            assert_eq!(f.reflect_hit_sound, snd, "atkp {id} sound");
            // Sora's rows have no reflect data at all.
            assert_eq!((f.reflected_motion, f.reflect_hit_back, f.reflect_action, f.reflect_rc, f.reflect_range, f.reflect_angle), (0, 0, 0, 0, 0, 0), "atkp {id}");
        }
        assert_eq!(ReflectFields::from_atkp(row(&rows, 126)).hit_sound(), Some(500000));
        assert_eq!(ReflectFields::from_atkp(row(&rows, 88)).hit_sound(), None);
    }

    #[test]
    fn enemy_rows_with_reflect_fields() {
        let Some(mut a) = open_disc() else { return };
        let rows = atkp_rows(&mut a);
        // (id, EnemyReaction, refact_self, refact_other, motion, hit back, action, range, angle)
        let enemy = [
            (5, 3, 0, 0, 140, -1, 111449, 0, 0),
            (14, 7, 0, 0, 141, -1, 110889, 0, 0),
            (123, 0, 2, 0, 140, -1, 104457, 180, 0),
            (161, 0, 1, 0, 0, 0, 0, 100, 0),
            (173, 0, 0, 0, 141, -1, 0, 90, 0),
            (233, 0, 2, 2, 0, 0, 0, 60, 120),
        ];
        for (id, react, rs, ro, mot, back, action, range, angle) in enemy {
            let r = row(&rows, id);
            let f = ReflectFields::from_atkp(r);
            assert_eq!(r.enemy_reaction, react, "atkp {id}");
            assert_eq!(f.refact_self, Refact::from_raw(rs), "atkp {id}");
            assert_eq!(f.refact_other, Refact::from_raw(ro), "atkp {id}");
            assert_eq!(f.reflected_motion, mot, "atkp {id}");
            assert_eq!(f.reflect_hit_back, back, "atkp {id}");
            assert_eq!(f.reflect_action, action, "atkp {id}");
            assert_eq!(f.reflect_range, range, "atkp {id}");
            assert_eq!(f.reflect_angle, angle, "atkp {id}");
        }
        // Enemy clash: row 5 plays motion 140 with the default recoil; row 123 and 161 do not react.
        let c5 = enemy_clash(&ReflectFields::from_atkp(row(&rows, 5)), false).unwrap();
        assert_eq!((c5.motion, c5.push_power, c5.action), (Some(140), -1.0, Some(111449)));
        assert!(enemy_clash(&ReflectFields::from_atkp(row(&rows, 123)), false).is_none());
        let c161 = enemy_clash(&ReflectFields::from_atkp(row(&rows, 161)), false).unwrap();
        assert_eq!((c161.motion, c161.action), (None, None));
        assert!(enemy_clash(&ReflectFields::from_atkp(row(&rows, 5)), true).is_none());
    }

    #[test]
    fn table_wide_reflect_facts() {
        let Some(mut a) = open_disc() else { return };
        let rows = atkp_rows(&mut a);
        assert_eq!(rows.len(), 2326);
        assert!(rows.iter().all(|r| r.reflect_rc == 0), "ReflectRC is never set");
        assert!(rows.iter().all(|r| r.reflect_hit_back == 0 || r.reflect_hit_back == -1), "hit back is 0 or -1 only");
        assert!(rows.iter().all(|r| matches!(r.refact_self, 0..=2) && matches!(r.refact_other, 0..=2)));
        assert!(rows.iter().filter(|r| r.reflected_motion != 0).all(|r| matches!(r.reflected_motion, 10 | 140..=143)));
    }

    #[test]
    fn wall_recoil_from_real_rows_and_exe() {
        let Some(mut a) = open_disc() else { return };
        let rows = atkp_rows(&mut a);
        let Ok(exe) = a.iso().read("SLUS_210.05") else {
            eprintln!("skipped: no executable on the disc");
            return;
        };
        let c = ExeConsts::from_exe(&exe).expect("exe constants");
        // Values read from the executable's data (negative power arguments select the default).
        assert_eq!((c.wall_power, c.clash_power, c.default_power, c.guard_power), (-1.0, -1.0, 50.0, 200.0));
        assert_eq!((c.push_scale, c.push_cutoff, c.reduced_scale), (0.1, 1.0, 0.5));
        assert!((c.push_decay - 0.9).abs() < 1e-6);
        assert_eq!(c.flinch_motions, [10, 11, 12, 13]);

        // Sora's combo hit 126 has BG Hit set: it bounces off a wall with the generic recoil,
        // although its RefactSelf is 0 and it has no reflect data.
        let w = wall_recoil(&ReflectFields::from_atkp(row(&rows, 126)), false, false, &c).unwrap();
        assert_eq!((w.motion, w.push_power, w.airborne, w.refund_combo_step), (motion::REFLECT_GROUND, 50.0, false, true));
        let w = wall_recoil(&ReflectFields::from_atkp(row(&rows, 126)), false, true, &c).unwrap();
        assert_eq!(w.motion, motion::REFLECT_AIR);
        // Without the flag the volume is never tested against walls, and a volume reacts once.
        assert!(wall_recoil(&ReflectFields::from_atkp(row(&rows, 127)), false, false, &c).is_none());
        assert!(wall_recoil(&ReflectFields::from_atkp(row(&rows, 126)), true, false, &c).is_none());
        assert_eq!(flinch_motion(true, true, &c), 10);
        assert_eq!(flinch_motion(false, false, &c), 13);
    }

    #[test]
    fn push_and_blow_from_real_rows() {
        let Some(mut a) = open_disc() else { return };
        let rows = atkp_rows(&mut a);
        let Ok(exe) = a.iso().read("SLUS_210.05") else { return };
        let c = ExeConsts::from_exe(&exe).unwrap();
        let sstm = kh2_tables::load::<t::PrefSstm>(&mut a, &t::PREF_SSTM).expect("sstm").rows.remove(0);
        let d = ReactionDefaults::from(&sstm);
        // Sora's combo hit 126 (reaction 3, KnockbackStrength1 125): flinch plus a push of 125.
        let r = row(&rows, 126);
        assert_eq!(ReactionClass::from_raw(r.enemy_reaction), ReactionClass::FlinchPush);
        assert_eq!(push_strength(r.enemy_reaction, r.enemy_reaction, Some(r.knockback_strength1), &d, &c), Some(125.0));
        // Row 124 has 0x7fff: the sstm default. Row 87 has 0: no push.
        let r = row(&rows, 124);
        assert_eq!(push_strength(r.enemy_reaction, r.enemy_reaction, Some(r.knockback_strength1), &d, &c), Some(d.hit_back));
        let r = row(&rows, 87);
        assert_eq!(push_strength(r.enemy_reaction, r.enemy_reaction, Some(r.knockback_strength1), &d, &c), Some(0.0));
        // Row 88 is a blow (reaction 4) with KnockbackStrength1 20 (up) and 100 (forwards).
        let r = row(&rows, 88);
        assert_eq!(ReactionClass::from_raw(r.enemy_reaction), ReactionClass::Blow);
        assert_eq!(blow_launch(r.knockback_strength1, r.knockback_strength2, &d), (20.0, 100.0));
        assert_eq!(push_strength(r.enemy_reaction, r.enemy_reaction, Some(0), &d, &c), None);
        assert_eq!(blow_launch(0, 0, &d), (d.blow_up, d.blow_speed));
        // Reaction 12 and 13 halve the push.
        assert_eq!(push_strength(12, 12, Some(100), &d, &c), Some(50.0));
    }

    #[test]
    fn patn_table_from_disc() {
        let Some(mut a) = open_disc() else { return };
        let blob = kh2_tables::subfile(&mut a, &t::PATN).expect("patn");
        let rows = parse_patn(&blob).expect("patn rows");
        assert_eq!(rows.len(), 15);
        let ids: Vec<u8> = rows.iter().map(|r| r.status_bit).collect();
        assert_eq!(ids, [15, 13, 11, 10, 9, 14, 12, 5, 6, 8, 7, 4, 3, 2, 1]);
        let ctx = |bit: u32| PatnContext { status: 1 << bit, grounded: true, attacker_flag_4000: false };
        // Status bit 14 turns reaction 5 into 3; bit 12 turns blows (4, 7, 9) into reaction 14.
        assert_eq!(effective_reaction(5, &rows, &ctx(14)), 3);
        assert_eq!(effective_reaction(4, &rows, &ctx(12)), 14);
        assert_eq!(effective_reaction(9, &rows, &ctx(12)), 14);
        // Status bit 1 cancels reactions 3..=11 and 15..=18.
        assert_eq!(effective_reaction(3, &rows, &ctx(1)), 0);
        assert_eq!(effective_reaction(11, &rows, &ctx(1)), 0);
        assert_eq!(effective_reaction(12, &rows, &ctx(1)), 12);
        assert_eq!(effective_reaction(3, &rows, &PatnContext { status: 0, grounded: true, attacker_flag_4000: false }), 3);
    }
}
