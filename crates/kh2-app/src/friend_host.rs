//! The party members' script host: the enemy host (`BevyHost`) plus what only a friend object answers
//! (docs/party.md section 5, host group 10 at table 0x371810) and the player-type motion remap of the
//! motion requests (FUN_0017e000 -> FUN_0017da30).
//!
//! Group 10 entries the members' scripts reach (`kh2 bdx P_EX020` / `P_EX030`: Donald 10.31; Goofy 10.14,
//! 10.31, 10.40, 10.55) are answered here; the rest of group 10 goes to the enemy host, which reports it
//! as missing in the `KH2_BDXLOG` table. Everything else is the enemy host's.

use bevy::prelude::*;
use kh2_bdx::host::{Arg, Call, Host, Value};

use crate::enemy_ai::AiTrace;
use crate::enemy_host::{BevyHost, St};

/// What the group-10 entries and the remap read or write of the member (kept on `FriendBrain`).
#[derive(Clone, Copy, Debug, Default)]
pub struct FriendFields {
    /// +0xb7c bit 0x10000: the script set the member's target (host 10.14 with a mode other than 0).
    pub script_target: bool,
    /// +0x160 bits 4/8/0x10: the member is free (host 10.36).
    pub free: bool,
    /// FUN_0017da30's slot order for the member's motion state now.
    pub order: [u8; 4],
    /// +0xaf4: the MP the special's mode costs (FUN_00113d48 writes it); host 1.276 pays it.
    pub pending_mp: u8,
    /// What host 1.276 paid this tick (`friend_ai` hands it to the member's MP, FUN_00186368).
    pub paid: Option<u8>,
}

/// The member's script host.
pub(crate) struct FriendHost<'a, 'b> {
    pub(crate) inner: BevyHost<'a>,
    pub(crate) fields: &'b mut FriendFields,
}

fn words(c: &Call) -> Option<(u16, u16, Vec<u32>)> {
    match c {
        Call::Unknown { group, index, args } => Some((*group, *index, args.clone())),
        Call::Func { group, index, args } => Some((
            *group,
            *index,
            args.iter()
                .map(|a| match a {
                    Arg::Float(f) => f.to_bits(),
                    a => a.int() as u32,
                })
                .collect(),
        )),
        _ => None,
    }
}

impl FriendHost<'_, '_> {
    fn trace(&mut self) -> &mut AiTrace {
        &mut *self.inner.trace
    }

    /// FUN_0017da30 on a motion number of the member's own player-type motion set: the slot the
    /// enemy host then plays. A number with no slot in the state's order stays as it is (the enemy
    /// host then reports it as not in the set).
    fn remap(&self, motion: i32) -> i32 {
        let rig = self.inner.env.rig;
        crate::party_battle::remap(motion, self.fields.order, |s| rig.mset.has(s)).map_or(motion, |s| s as i32)
    }

    fn group10(&mut self, index: u16, a: &[u32]) -> Option<(Value, St, &'static str)> {
        let arg = |k: usize| a.get(k).copied().unwrap_or(0);
        Some(match index {
            // 10.14 (0x2f6da0): mode 0 clears +0xb7c bit 0x10000, any other mode sets it after copying a
            // target record into +0x9f4 (1 the argument's, 2 the player's lock-on 0x34e9b0, 3 FUN_0010d720).
            // The app's member keeps its own target (party.rs); only the bit is carried.
            14 => {
                self.fields.script_target = arg(1) != 0;
                if arg(1) == 0 {
                    (Value::None, St::Impl, "")
                } else {
                    (Value::None, St::Approx, "the target bit is set; the record copy into the member's target is not carried out")
                }
            }
            // 10.19 (0x2f6e60): +0x10c bit 0x10000 (in a limit): no limits with partners yet (stage 5).
            19 => (Value::Bool(false), St::Approx, "no limit with partners in the app (stage 5): read clear"),
            // 10.31 (0x2f7118): +0x588 bit 0x400000, set only for summons (FUN_0010c5f8 path): a party
            // member is never one.
            31 => (Value::Bool(false), St::Impl, ""),
            // 10.36 (0x2f7230): free (+0x160 bits 4/8/0x10).
            36 => (Value::Bool(self.fields.free), St::Approx, "free = not in an attack motion (the controller's +0x160 bits are not modelled)"),
            // 10.40 (0x2f72e8 -> FUN_0017eb48): whether the second object's motion controller is linked
            // under the first's (+0x94 / +0x9c chain). The app links no controllers.
            40 => (Value::Bool(false), St::Approx, "the app links no motion controllers: read false"),
            // 10.55 (0x2f7540 -> FUN_002f0758): bit 16 of the word at the first argument + 4 = (second != 0).
            55 => {
                let at = arg(0).wrapping_add(4);
                let mut b = [0u8; 4];
                if !self.inner.read_mem(at, &mut b) {
                    return Some((Value::None, St::Missing, "10.55 on a word outside the member's memory"));
                }
                let w = u32::from_le_bytes(b) & !0x1_0000 | u32::from(arg(1) != 0) << 16;
                self.inner.write_mem(at, &w.to_le_bytes());
                (Value::None, St::Impl, "")
            }
            _ => return None,
        })
    }
}

impl Host for FriendHost<'_, '_> {
    fn call(&mut self, c: &Call) -> Value {
        let me = self.inner.obj.handle;
        match c {
            Call::PlayMotion { obj, motion, blend } if *obj == me => {
                return self.inner.call(&Call::PlayMotion { obj: *obj, motion: self.remap(*motion), blend: *blend });
            }
            Call::PlayMotionChecked { obj, motion, blend } if *obj == me => {
                return self.inner.call(&Call::PlayMotionChecked { obj: *obj, motion: self.remap(*motion), blend: *blend });
            }
            Call::PlayMotionAt { obj, motion, start, blend } if *obj == me => {
                return self.inner.call(&Call::PlayMotionAt { obj: *obj, motion: self.remap(*motion), start: *start, blend: *blend });
            }
            _ => {}
        }
        // 1.276 (0x170a00): for a party object (+0x588 bit 4) FUN_001769e0 pays the pending MP byte
        // +0xaf4 (FUN_00186368 with minus it) and clears it. [inferred] The object named is the member
        // itself (its script runs only here).
        if let Some((1, 276, _)) = words(c) {
            let cost = std::mem::take(&mut self.fields.pending_mp);
            self.fields.paid = Some(self.fields.paid.unwrap_or(0).saturating_add(cost));
            let tag = self.inner.tag;
            self.trace().record(tag, c, &Value::None, St::Impl, "");
            return Value::None;
        }
        if let Some((10, index, a)) = words(c)
            && let Some((v, st, why)) = self.group10(index, &a)
        {
            let tag = self.inner.tag;
            self.trace().record(tag, c, &v, st, why);
            return v;
        }
        self.inner.call(c)
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        self.inner.read_mem(addr, out)
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        self.inner.write_mem(addr, data)
    }
}
