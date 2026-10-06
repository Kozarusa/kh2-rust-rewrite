//! Player attack tables: which motion each attack action plays and when it may be chosen.
//!
//! The table is the `ptya` subfile of `00battle.bin`. A character's table is picked by the
//! `NeoMoveset` field of its `00objentry` row. Layout (read from the game's lookup and selection
//! routines; OpenKH leaves the subfile undocumented): a u32 version, a u32 table count, then one
//! u32 offset per table (from the start of the subfile). A table is a u32 record count followed by
//! 0x44-byte records.
//!
//! Record fields that the selection code uses:
//!
//! | offset | meaning |
//! |---|---|
//! | 0 | action key; other code asks for an action by key (guard, the default first attack, ...) |
//! | 1 | candidate group: 3 = always a candidate, 1 = candidate for the locked target by range, 2 = candidate for other threatening enemies by range, 0 = only chosen explicitly by key |
//! | 2 | alternative key (signed, -1 none): tried when this action's motion is already playing |
//! | 3 | argument of the finisher test |
//! | 4 | flags: bit 0 airborne only, bit 1 widens the height window's lower bound (and the action's leap) by the actor's jump height, bits 3 and 4 pick the sign of the target's half-height added to the upper and lower bounds, bit 2 finisher |
//! | 8 | motion ID played |
//! | 10 | motion ID queued after it (4, the fall, for air attacks) |
//! | 0x20, 0x24 | horizontal distance window (min inclusive, max exclusive) |
//! | 0x28, 0x2c | height window, Y down (upper bound inclusive, lower bound exclusive) |
//! | 0x30, 0x34 | window on the dot product of the facing/lock direction with the direction to the candidate |
//! | 0x40 | ability bit that must be set (0 none) |

use std::sync::Arc;

pub const RECORD_SIZE: usize = 0x44;

/// One action of a player's attack table.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub key: u8,
    pub group: u8,
    pub alt: i8,
    pub arg: u8,
    pub flags: u32,
    pub motion: u16,
    pub next_motion: u16,
    pub distance: [f32; 2],
    pub height: [f32; 2],
    pub angle: [f32; 2],
    pub ability: u16,
}

impl Action {
    /// The action may only be chosen while airborne (flag bit 0).
    pub fn air_only(&self) -> bool {
        self.flags & 1 != 0
    }
    /// A finisher: chosen once the combo is long enough or a finisher window is open (flag bit 2).
    pub fn finisher(&self) -> bool {
        self.flags & 4 != 0
    }
}

/// One character's attack table.
#[derive(Debug, Clone, Default)]
pub struct Moveset {
    pub actions: Vec<Action>,
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn f32_at(b: &[u8], o: usize) -> Option<f32> {
    u32_at(b, o).map(f32::from_bits)
}

impl Moveset {
    /// Table `index` of a `ptya` subfile; `None` when the index is out of range or unused.
    pub fn parse(ptya: &[u8], index: usize) -> Option<Moveset> {
        let count = u32_at(ptya, 4)? as usize;
        if index >= count {
            return None;
        }
        let at = u32_at(ptya, 8 + 4 * index)? as usize;
        if at == 0 {
            return None;
        }
        let n = u32_at(ptya, at)? as usize;
        let mut actions = Vec::with_capacity(n);
        for i in 0..n {
            let r = ptya.get(at + 4 + RECORD_SIZE * i..at + 4 + RECORD_SIZE * (i + 1))?;
            actions.push(Action {
                key: r[0],
                group: r[1],
                alt: r[2] as i8,
                arg: r[3],
                flags: u32_at(r, 4)?,
                motion: u16_at(r, 8)?,
                next_motion: u16_at(r, 10)?,
                distance: [f32_at(r, 0x20)?, f32_at(r, 0x24)?],
                height: [f32_at(r, 0x28)?, f32_at(r, 0x2c)?],
                angle: [f32_at(r, 0x30)?, f32_at(r, 0x34)?],
                ability: u16_at(r, 0x40)?,
            });
        }
        Some(Moveset { actions })
    }

    /// The first action with this key.
    pub fn by_key(&self, key: u8) -> Option<&Action> {
        self.actions.iter().find(|a| a.key == key)
    }

    /// The game's choice among the candidate actions (`candidate` marks the flagged ones): the first
    /// flagged action whose motion differs from the one playing, or whose alternative key is its own;
    /// when every flagged action is already playing, the first one's alternative.
    pub fn pick(&self, candidate: impl Fn(&Action) -> bool, playing: usize) -> Option<&Action> {
        let mut fallback = None;
        for a in self.actions.iter().filter(|a| candidate(a)) {
            if a.alt as i32 == a.key as i32 || a.motion as usize != playing {
                return Some(a);
            }
            if fallback.is_none() && a.alt >= 0 {
                fallback = self.by_key(a.alt as u8);
            }
        }
        fallback
    }
}

// ---------------------------------------------------------------------------------------------
// Selection rules. The game's attack request (FUN_00174cf8, command type 1) runs the explicit-key
// checks (FUN_00177ea0), the combo counter update (FUN_00176b00) and then the table selection
// (FUN_00177d58 -> FUN_001cbcf8 builds a bit per record, FUN_001cbf48 picks). See
// docs/moveset-selection.md for the evaluation order and the gaps.
// ---------------------------------------------------------------------------------------------

/// Action keys that the game's code asks for by name.
pub mod key {
    /// The attack chosen when nothing else qualifies, on the ground and in the air.
    pub const GROUND_ATTACK: u8 = 5;
    pub const AIR_ATTACK: u8 = 9;
    /// Flagged by the ledge probe (FUN_001cbcf8 -> FUN_0019a8e8: no floor ahead), whatever its
    /// group says.
    pub const LEDGE_PROBE: u8 = 1;
    pub const GUARD: u8 = 11;
    /// Explicit actions tried on an attack press before the table selection, in this order
    /// (FUN_00177ea0): Finishing Leap, Upper Slash, Horizontal Slash.
    pub const EXPLICIT: [u8; 3] = [37, 12, 38];
    /// The attack that answers a guard hit.
    pub const COUNTER: u8 = 39;
    /// Retaliating Slash (A344): the Square press of a blown actor (FUN_001758b0).
    pub const RETALIATING: u8 = 40;
}

/// Ability bit of Aerial Recovery (ability item 158): the jump press of a blown actor plays the
/// recovery motion instead of Retaliating Slash (FUN_001758b0 tests `block & 0x100000`).
pub const ABILITY_AERIAL_RECOVERY: u16 = 20;

/// The combo state the availability tests read (fields of the actor and its status block).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chain {
    /// Airborne in the sense of FUN_0019aa28 (also true while a no-gravity range is active).
    pub airborne: bool,
    /// Ability bits (the records' gate); bit 0 of a record means no gate.
    pub abilities: u128,
    /// Actor +0xb04: attacks accepted in the chain, counting the one being chosen.
    pub combo: u8,
    /// Actor +0xb05: finishers accepted in the chain, starts at 1.
    pub finishers: u8,
    /// Status block +0x28 / +0x29: attacks before the finisher on the ground / in the air.
    pub max_ground: u8,
    pub max_air: u8,
    /// Status block +0x2a: finishers per chain.
    pub max_finishers: u8,
    /// A finisher window (range trigger 50) is open: actor +0x168 bit 0x20.
    pub finisher_window: bool,
}

/// Why a record is not available (FUN_0019f1b0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    NoMotion,
    /// The ability bit of the record is not set.
    Ability(u16),
    /// The record is for the other airborne state.
    State,
    /// A finisher while the combo is not long enough and no finisher window is open.
    FinisherNotReady,
    /// A normal attack after the combo is long enough and the finisher count is used up.
    FinisherDone,
}

impl Chain {
    pub fn has_ability(&self, bit: u16) -> bool {
        bit == 0 || (bit < 128 && self.abilities >> bit & 1 != 0)
    }

    /// FUN_00176908: a finisher is possible now: a finisher window is open, or the chain is long
    /// enough (`arg` counts attacks the finisher may be early by). The status flag 0x800 case of
    /// FUN_001768b8 (which makes it false) is not modelled.
    pub fn finisher_ready(&self, arg: u8) -> bool {
        self.finisher_window || (if self.airborne { self.max_air } else { self.max_ground }) as i32 <= self.combo as i32 + arg as i32
    }

    /// FUN_0019f1b0.
    pub fn check(&self, a: &Action) -> Result<(), Reject> {
        if a.motion == 0 {
            return Err(Reject::NoMotion);
        }
        if !self.has_ability(a.ability) {
            return Err(Reject::Ability(a.ability));
        }
        if self.airborne != a.air_only() {
            return Err(Reject::State);
        }
        if a.finisher() {
            if !self.finisher_ready(a.arg) {
                return Err(Reject::FinisherNotReady);
            }
        } else if self.finisher_ready(0) && self.finishers > 1 {
            return Err(Reject::FinisherDone);
        }
        Ok(())
    }

    /// FUN_00176a70: a chain starts.
    pub fn restart(&mut self) {
        self.combo = 1;
        self.finishers = 1;
    }
}

/// Which input windows of the playing motion are open (the actor's range-trigger word, +0x160).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Windows {
    /// A grounded, airborne or blend-to-idle range (bits 4, 8, 0x10): the actor is free to start.
    pub free: bool,
    /// The allow-combo range (bit 0x400).
    pub combo: bool,
}

/// How an attack press was accepted (FUN_00176b00).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    /// A new chain: the counters were reset.
    Restart,
    /// The chain goes on: the combo counter went up.
    Continue,
    /// An extra finisher in an open finisher window: the finisher counter went up.
    ExtraFinisher,
}

impl Chain {
    /// FUN_00176b00: whether an attack press is accepted now, updating the counters. `connected` is
    /// the actor's "a hit landed" latch (+0x108 bit 0x2000000, set by FUN_0018c6d0 and the wall
    /// recoil, cleared when an action starts).
    pub fn press(&mut self, w: Windows, connected: bool) -> Option<Press> {
        if self.finisher_window && connected && self.finishers < self.max_finishers {
            self.finishers += 1;
            return Some(Press::ExtraFinisher);
        }
        if !w.combo || !connected {
            if w.free {
                self.restart();
                return Some(Press::Restart);
            }
            return None;
        }
        self.combo = (self.combo + 1).min(100);
        Some(Press::Continue)
    }
}

/// How a target point lies relative to the actor (the arguments of FUN_0019f090). Heights are the
/// game's Y-down: a point above the actor has a negative `dy`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    /// Horizontal distance to the point less the point's radius, at least 0.
    pub dist: f32,
    /// The point's height less the actor's.
    pub dy: f32,
    /// Dot product of the horizontal direction to the point with the reference direction (the
    /// direction to the actor's target, or its facing without one), at most 1.
    pub dot: f32,
    /// The point's half height (the target shape's height field).
    pub half_height: f32,
}

/// Which part of a record's window a probe misses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Miss {
    Distance,
    Height,
    Angle,
}

impl Action {
    /// FUN_0019f090: the record's distance, height and angle windows against a probe. `jump_height`
    /// is what FUN_0016be20 returns for flag bit 1: the actor's jump height (the High Jump height
    /// of the `fmab` row of its level when the actor has one, else its plain jump height).
    pub fn window(&self, p: &Probe, jump_height: f32) -> Result<(), Miss> {
        let mut hi = self.height[0];
        let mut lo = self.height[1];
        if self.flags & 2 != 0 {
            lo -= jump_height;
        }
        hi += if self.flags & 8 == 0 { p.half_height } else { -p.half_height };
        lo += if self.flags & 0x10 == 0 { -p.half_height } else { p.half_height };
        if !(self.distance[0] <= p.dist && p.dist < self.distance[1]) {
            return Err(Miss::Distance);
        }
        if !(p.dy <= hi && lo < p.dy) {
            return Err(Miss::Height);
        }
        if !(p.dot <= self.angle[1] && self.angle[0] < p.dot) {
            return Err(Miss::Angle);
        }
        Ok(())
    }
}

/// What the selection sees besides the chain.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    /// The actor's target (lock-on or automatic target), when it has one: group 1 records are tested
    /// against it.
    pub target: Option<Probe>,
    /// Every target point of the other enemies (including the target's): group 2 records pass when
    /// any of them is inside the window.
    pub others: Vec<Probe>,
    /// The ledge probe of FUN_001cbcf8 (FUN_0019a8e8, bit 32 of actor +0x600) is set: after the
    /// actor's body sphere is swept forward by its radius plus `plyr` AttackJFront, the floor test
    /// at the sweep's end found no floor, or one too far below. It flags key 1 whatever that record
    /// says.
    pub ledge_probe: bool,
    /// The actor's jump height (FUN_0016be20), added to the reach of records with flag bit 1.
    pub jump_height: f32,
}

/// What became of one record in the selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Verdict {
    /// Group 0: only chosen by key.
    ByKeyOnly,
    Rejected(Reject),
    /// Group 1 without a target, or group 2 without other enemies.
    NoTarget,
    /// Outside the window of the best-fitting probe (the first one for group 2).
    Outside(Miss),
    Flagged,
}

#[derive(Clone, Copy, Debug)]
pub struct Evaluation {
    pub index: usize,
    pub verdict: Verdict,
}

impl Moveset {
    /// FUN_001cbcf8: one verdict per record. Group 3 records are flagged when available, group 1 when
    /// available and the target is inside the window, group 2 when available and any other enemy's
    /// target point is. Key 1 is also flagged by the ledge probe.
    pub fn evaluate(&self, chain: &Chain, scene: &Scene) -> Vec<Evaluation> {
        self.actions
            .iter()
            .enumerate()
            .map(|(index, a)| {
                let mut verdict = match a.group {
                    1..=3 => match chain.check(a) {
                        Err(r) => Verdict::Rejected(r),
                        Ok(()) if a.group == 3 => Verdict::Flagged,
                        Ok(()) => {
                            let probes: &[Probe] = if a.group == 1 { scene.target.as_slice() } else { &scene.others };
                            let mut first_miss = None;
                            let mut hit = false;
                            for p in probes {
                                match a.window(p, scene.jump_height) {
                                    Ok(()) => {
                                        hit = true;
                                        break;
                                    }
                                    Err(m) => first_miss = first_miss.or(Some(m)),
                                }
                            }
                            if hit {
                                Verdict::Flagged
                            } else {
                                first_miss.map_or(Verdict::NoTarget, Verdict::Outside)
                            }
                        }
                    },
                    _ => Verdict::ByKeyOnly,
                };
                if a.key == key::LEDGE_PROBE && scene.ledge_probe {
                    verdict = Verdict::Flagged;
                }
                Evaluation { index, verdict }
            })
            .collect()
    }

    /// FUN_001cbf48 over the flagged records: the first whose motion is not the one playing, or whose
    /// alternative key is its own; failing that, the first flagged record's alternative.
    pub fn pick_flagged(&self, ev: &[Evaluation], playing: usize) -> Option<usize> {
        let mut fallback = None;
        for e in ev.iter().filter(|e| e.verdict == Verdict::Flagged) {
            let a = &self.actions[e.index];
            if a.alt as i32 == a.key as i32 || a.motion as usize != playing {
                return Some(e.index);
            }
            if fallback.is_none() && a.alt >= 0 {
                fallback = self.actions.iter().position(|b| b.key == a.alt as u8);
            }
        }
        fallback
    }

    /// The whole selection: evaluate, pick, and when nothing was picked flag the default attack (key 5
    /// on the ground, 9 in the air; flagged whatever its availability) and pick again. Returns the
    /// verdicts (with the default flagged if it was needed) and the chosen record's index.
    pub fn select(&self, chain: &Chain, scene: &Scene, playing: usize) -> (Vec<Evaluation>, Option<usize>) {
        let mut ev = self.evaluate(chain, scene);
        let mut chosen = self.pick_flagged(&ev, playing);
        if chosen.is_none() {
            let default = if chain.airborne { key::AIR_ATTACK } else { key::GROUND_ATTACK };
            if let Some(i) = self.actions.iter().position(|a| a.key == default) {
                ev[i].verdict = Verdict::Flagged;
                chosen = self.pick_flagged(&ev, playing);
            }
        }
        (ev, chosen)
    }

    /// FUN_00177fa8: an explicit-key action is possible: the record is available, a normal attack
    /// only while no finisher is possible, and the chain is in an allow-combo or finisher window.
    /// (The press itself, FUN_00176b00, is the caller's.)
    pub fn explicit(&self, key: u8, chain: &Chain, w: Windows) -> Result<&Action, ExplicitMiss> {
        let a = self.by_key(key).ok_or(ExplicitMiss::NoRecord)?;
        chain.check(a).map_err(ExplicitMiss::Rejected)?;
        if !a.finisher() && chain.finisher_ready(0) {
            return Err(ExplicitMiss::FinisherPossible);
        }
        if !w.combo && !chain.finisher_window {
            return Err(ExplicitMiss::NoWindow);
        }
        Ok(a)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplicitMiss {
    NoRecord,
    Rejected(Reject),
    FinisherPossible,
    NoWindow,
}

/// What a blown actor's state looks like to FUN_001758b0.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlowState {
    /// The actor's mover has flag 0x40 and not 0x800: the blow movers (modes 4, 8, 0x15, 0x21;
    /// mode 0x2d carries 0x800 and allows nothing).
    pub recoverable: bool,
    /// Ticks since the blow was launched (actor +0xf8: zeroed by FUN_0019e468, +1 per tick in
    /// FUN_0019e4d8).
    pub ticks: f32,
    /// `plyr` BlowRecoveryTime (the record at actor +0x130, offset 0x58).
    pub recovery_time: f32,
    /// Actor +0x7f8 flag 4 (set by FUN_00186580 together with a motion-request reset; cleared by
    /// FUN_00178d80 which hands the actor back to its controller). Inferred: the actor is out of play.
    pub locked: bool,
}

/// The outcome of a blown actor's button press (FUN_001758b0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Recovery<'a> {
    /// The jump flag (command flag 2) with Aerial Recovery: motion 40 and the mover's launch
    /// (FUN_001759b8).
    AerialRecovery,
    /// The Square flag: the counters restart and the key 40 record starts.
    Retaliate(&'a Action),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryMiss {
    /// Not in a blow mover, or in one that allows nothing.
    NotRecoverable,
    /// Fewer ticks than BlowRecoveryTime have passed.
    TooEarly,
    Locked,
    /// Neither the jump flag with Aerial Recovery nor the Square flag.
    NoInput,
    NoRecord,
    Rejected(Reject),
}

impl Moveset {
    /// FUN_001758b0, run for every command tick before the Square actions and the attack: in a
    /// blow mover, once BlowRecoveryTime ticks have passed and the actor is not locked, command
    /// flag 2 (`jump`: set by FUN_001babf0 from logical pad bit 0x100, the jump button; the same flag
    /// makes FUN_00174cf8 call the mover's jump method) with the Aerial Recovery ability plays the
    /// recovery; otherwise command flag 8 (`square`: logical pad bit 0x200, physical Square 0x8000)
    /// restarts the counters and starts the key 40 record if it is available. The restart happens
    /// even when the record is missing or rejected, as in the game.
    pub fn recovery(&self, blow: &BlowState, jump: bool, square: bool, chain: &mut Chain) -> Result<Recovery<'_>, RecoveryMiss> {
        if !blow.recoverable {
            return Err(RecoveryMiss::NotRecoverable);
        }
        if blow.ticks < blow.recovery_time {
            return Err(RecoveryMiss::TooEarly);
        }
        if blow.locked {
            return Err(RecoveryMiss::Locked);
        }
        if jump && chain.has_ability(ABILITY_AERIAL_RECOVERY) {
            return Ok(Recovery::AerialRecovery);
        }
        if !square {
            return Err(RecoveryMiss::NoInput);
        }
        chain.restart();
        let a = self.by_key(key::RETALIATING).ok_or(RecoveryMiss::NoRecord)?;
        chain.check(a).map_err(RecoveryMiss::Rejected)?;
        Ok(Recovery::Retaliate(a))
    }
}

/// A shared handle for components.
pub type SharedMoveset = Arc<Moveset>;

#[cfg(test)]
mod tests {
    use super::*;

    fn record(key: u8, group: u8, alt: i8, flags: u32, motion: u16) -> Vec<u8> {
        let mut r = vec![0u8; RECORD_SIZE];
        r[0] = key;
        r[1] = group;
        r[2] = alt as u8;
        r[4..8].copy_from_slice(&flags.to_le_bytes());
        r[8..10].copy_from_slice(&motion.to_le_bytes());
        r
    }

    fn table(records: &[Vec<u8>]) -> Vec<u8> {
        // Version, one table at offset 12.
        let mut b = vec![];
        b.extend(2u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(12u32.to_le_bytes());
        b.extend((records.len() as u32).to_le_bytes());
        for r in records {
            b.extend(r);
        }
        b
    }

    #[test]
    fn parses_records() {
        let t = table(&[record(5, 3, 6, 0, 151), record(8, 3, 7, 5, 183)]);
        let m = Moveset::parse(&t, 0).unwrap();
        assert_eq!(m.actions.len(), 2);
        assert_eq!((m.actions[1].key, m.actions[1].motion, m.actions[1].alt), (8, 183, 7));
        assert!(m.actions[1].air_only() && m.actions[1].finisher() && !m.actions[0].finisher());
        assert!(Moveset::parse(&t, 1).is_none());
    }

    #[test]
    fn recovery_press_follows_fun_001758b0() {
        // Key 40 as an air-only record (flag bit 0) gated on ability bit 101.
        let mut r40 = record(40, 0, -1, 1, 196);
        r40[0x40..0x42].copy_from_slice(&101u16.to_le_bytes());
        let t = table(&[record(5, 3, 6, 0, 151), r40]);
        let m = Moveset::parse(&t, 0).unwrap();
        let base = Chain { airborne: true, abilities: 1 << 101, combo: 3, finishers: 2, max_ground: 3, max_air: 3, max_finishers: 1, finisher_window: false };
        let blow = BlowState { recoverable: true, ticks: 10.0, recovery_time: 10.0, locked: false };
        let mut c = base;
        // Square in a blow mover after BlowRecoveryTime: the counters restart and key 40 starts.
        assert_eq!(m.recovery(&blow, false, true, &mut c).map(|r| matches!(r, Recovery::Retaliate(a) if a.key == 40)), Ok(true));
        assert_eq!((c.combo, c.finishers), (1, 1));
        // The gates, in the game's order.
        let mut c = base;
        assert_eq!(m.recovery(&BlowState { recoverable: false, ..blow }, false, true, &mut c), Err(RecoveryMiss::NotRecoverable));
        assert_eq!(m.recovery(&BlowState { ticks: 9.0, ..blow }, false, true, &mut c), Err(RecoveryMiss::TooEarly));
        assert_eq!(m.recovery(&BlowState { locked: true, ..blow }, false, true, &mut c), Err(RecoveryMiss::Locked));
        assert_eq!(m.recovery(&blow, true, false, &mut c), Err(RecoveryMiss::NoInput));
        assert_eq!((c.combo, c.finishers), (3, 2));
        // Jump (command flag 2) with Aerial Recovery wins over Square; without the ability it is
        // ignored. Square alone never recovers, jump alone never retaliates.
        let mut c = Chain { abilities: 1 << 101 | 1 << ABILITY_AERIAL_RECOVERY, ..base };
        assert_eq!(m.recovery(&blow, true, true, &mut c), Ok(Recovery::AerialRecovery));
        assert_eq!(m.recovery(&blow, true, false, &mut c), Ok(Recovery::AerialRecovery));
        assert_eq!((c.combo, c.finishers), (3, 2));
        assert!(matches!(m.recovery(&blow, false, true, &mut c), Ok(Recovery::Retaliate(a)) if a.key == key::RETALIATING));
        // Square without the record's ability: the counters still restart, the record is rejected.
        let mut c = Chain { abilities: 0, ..base };
        assert_eq!(m.recovery(&blow, false, true, &mut c), Err(RecoveryMiss::Rejected(Reject::Ability(101))));
        assert_eq!((c.combo, c.finishers), (1, 1));
        // On the ground the air-only record is for the other state.
        let mut c = Chain { airborne: false, ..base };
        assert_eq!(m.recovery(&blow, false, true, &mut c), Err(RecoveryMiss::Rejected(Reject::State)));
    }

    #[test]
    fn pick_prefers_other_motion_then_alternative() {
        let t = table(&[record(5, 3, 6, 0, 151), record(6, 1, -1, 0, 153)]);
        let m = Moveset::parse(&t, 0).unwrap();
        // Only key 5 is a candidate. From idle it is chosen; while it plays its alternative (key 6) is.
        assert_eq!(m.pick(|a| a.key == 5, 0).unwrap().key, 5);
        assert_eq!(m.pick(|a| a.key == 5, 151).unwrap().key, 6);
        assert!(m.pick(|a| a.key == 6, 153).is_none());
    }
}
