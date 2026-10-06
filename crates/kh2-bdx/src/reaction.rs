//! Reaction commands in scripts. The game delivers an accepted reaction command to the target's script
//! as event 11 (0xb) with the arguments (controller, command id, target record) (FUN_0016b420), and a
//! script offers commands itself with host 1.165 (slot, command, object, flag), 1.237 (four commands)
//! and 1.325 (send an event 11 to another object). This module reads both from a program without
//! running it. See docs/limits-reactions.md.

use std::collections::BTreeSet;

use crate::disasm::{Analysis, call_constants};
use crate::insn::{BinaryOp, Insn, decode};
use crate::program::Program;

/// The script event a reaction command arrives as.
pub const EVENT: u32 = 11;

/// The command ids the event 11 handler compares its second argument with: a constant pushed in front of
/// an integer subtraction, within the first 400 instructions reachable from the entry (both ways of
/// every jump). This is the pattern of every handler on the disc that has been read (the Soldier's
/// compares with 34 and calls its routine on a match). An id handled through a table or computed is not
/// found. `None` when the script has no event 11.
pub fn handled(p: &Program) -> Option<BTreeSet<i32>> {
    let pc = p.entry(EVENT)? as usize;
    let mut seen = BTreeSet::new();
    let mut work = vec![pc];
    let mut found = BTreeSet::new();
    let mut budget = 400;
    while let Some(mut at) = work.pop() {
        while budget > 0 && seen.insert(at) {
            budget -= 1;
            let Some((i, len)) = decode(p, at) else { break };
            match i {
                Insn::Jump(t) => {
                    work.push(t);
                    break;
                }
                Insn::JumpIfZero(t) | Insn::JumpIfNonZero(t) => work.push(t),
                Insn::Ret | Insn::Exit => break,
                Insn::Binary { float: false, op: BinaryOp::Sub } => {
                    // the instruction in front of the subtraction is the constant
                    if let Some((Insn::PushInt(v), _)) = at.checked_sub(3).and_then(|b| decode(p, b)) {
                        found.insert(v as i32);
                    }
                }
                _ => {}
            }
            at += len;
        }
    }
    Some(found)
}

/// The command ids a script offers through host 1.165, 1.237 and 1.325 with a constant id.
pub fn offered(an: &Analysis) -> BTreeSet<i32> {
    let mut out = BTreeSet::new();
    for (pc, (i, _)) in &an.code {
        if let Insn::Host { group: 1, index } = i {
            let (n, which): (usize, &[usize]) = match index {
                165 => (4, &[1]),
                237 => (5, &[0, 1, 2, 3]),
                325 => (4, &[2]),
                _ => continue,
            };
            let c = call_constants(an, *pc, n);
            out.extend(which.iter().filter_map(|&k| c[k]).map(|v| v as i32));
        }
    }
    out
}
