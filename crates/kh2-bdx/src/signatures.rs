//! Signatures of host functions the executable's table does not hold.
//!
//! The game's interpreter takes a host function's argument count from the table at 0x35bce8
//! (`exe.rs`). The tables of group 3 (the Gummi ship scripts), group 9 (the Gummi menu scripts) and
//! entries 0 and 1 of group 4 are empty in the executable file and nothing in the executable or on the
//! disc fills them in (see docs/bdx.md), so scripts that call them cannot be run from the table. Their
//! argument counts are recovered here from the scripts themselves. The compiler pushes a call's
//! arguments one after the other and leaves the operand stack empty at the end of every statement, so
//!
//! - the values pushed since the previous statement ended are the call's arguments (when the call is not
//!   itself an argument of another call), and
//! - the call returns a value exactly when the code after it needs one: a `pop` or a store right behind
//!   it, or an instruction further on that would take more values than the statement has left.
//!
//! The method is checked against the functions whose signature the table does give (the `sigcheck`
//! example hides a share of them and compares). It stays a derivation, and a wrong count would
//! unbalance the operand stack, which the interpreter would report as an error in the script.

use std::collections::{HashMap, HashSet};

use crate::disasm::{Analysis, analyze};
use crate::insn::Insn;
use crate::program::Program;

pub type Sig = (usize, bool);

/// Entries the executable's table does not hold, as seen on the USA disc: all of group 3, all of group 9
/// and the first two of group 4. Used when no table is at hand.
pub fn absent_from_table(group: u16, index: u16) -> bool {
    group == 3 || group == 9 || (group == 4 && index < 2)
}

/// One call of a function being inferred.
#[derive(Debug, Clone, Copy)]
struct Site {
    /// Values pushed since the statement began.
    pushed: i32,
    /// `Some(true)`: the code after the call needs a value from it. `Some(false)`: the statement after
    /// it balances without one. `None`: not decided (an unknown call comes first).
    result: Option<bool>,
    /// The call is a whole statement (its result, if any, is dropped or stored), so `pushed` counts only
    /// its own arguments.
    top_level: bool,
}

#[derive(Default, Debug)]
struct Votes {
    sites: Vec<Site>,
}

/// Argument counts and result flags of the host functions the script calls and `known` does not know.
pub fn infer(p: &Program, known: &dyn Fn(u16, u16) -> Option<Sig>) -> HashMap<(u16, u16), Sig> {
    infer_only(p, known, &|_, _| true)
}

/// As [`infer`], but only for functions `wanted` accepts (used to check the method on known functions).
pub fn infer_only(p: &Program, known: &dyn Fn(u16, u16) -> Option<Sig>, wanted: &dyn Fn(u16, u16) -> bool) -> HashMap<(u16, u16), Sig> {
    let Ok(an) = analyze(p) else { return HashMap::new() };
    let unknown: Vec<(u16, u16)> = {
        let mut u: Vec<_> = an
            .code
            .values()
            .filter_map(|(i, _)| if let Insn::Host { group, index } = i { Some((*group, *index)) } else { None })
            .filter(|&(g, i)| wanted(g, i) && known(g, i).is_none())
            .collect();
        u.sort_unstable();
        u.dedup();
        u
    };
    if unknown.is_empty() {
        return HashMap::new();
    }
    let ctx = Ctx::new(p, &an, known);
    // Functions with a call site that is a whole statement are certain; they count as known in the next
    // pass, which makes the stack depth right for calls nested in the arguments of other unknown calls.
    let mut certain: HashMap<(u16, u16), Sig> = HashMap::new();
    let mut result: HashMap<(u16, u16), Sig> = HashMap::new();
    for _ in 0..6 {
        let lookup = |g: u16, i: u16| known(g, i).or_else(|| certain.get(&(g, i)).copied());
        let open: Vec<(u16, u16)> = unknown.iter().copied().filter(|f| !certain.contains_key(f)).collect();
        let votes = ctx.collect(&lookup, &open);
        result = certain.clone();
        let mut progress = false;
        for (f, v) in &votes {
            let Some((sig, sure)) = decide(v) else { continue };
            result.insert(*f, sig);
            if sure {
                certain.insert(*f, sig);
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }
    result
}

/// Picks the signature the votes agree on, and whether some site was a whole statement (so the
/// argument count is reliable).
fn decide(v: &Votes) -> Option<(Sig, bool)> {
    let top: Vec<i32> = v.sites.iter().filter(|s| s.top_level && s.pushed >= 0).map(|s| s.pushed).collect();
    let yes = v.sites.iter().filter(|s| s.result == Some(true)).count();
    let no = v.sites.iter().filter(|s| s.result == Some(false)).count();
    let returns = yes > no;
    if !top.is_empty() {
        return Some(((mode(&top) as usize, returns), true));
    }
    // Only calls nested in other expressions: values of the enclosing expressions are mixed into the
    // count, never taken out of it, so the smallest count is the closest.
    let n = v.sites.iter().filter(|s| s.pushed >= 0).map(|s| s.pushed).min()?;
    Some(((n as usize, returns), false))
}

fn mode(values: &[i32]) -> i32 {
    let mut counts: HashMap<i32, usize> = HashMap::new();
    for &v in values {
        *counts.entry(v).or_default() += 1;
    }
    counts.into_iter().max_by_key(|&(v, c)| (c, std::cmp::Reverse(v))).map(|(v, _)| v).unwrap_or(0)
}

/// How a script routine looks from a caller: values it takes off the stack and whether it leaves one.
#[derive(Clone, Copy, Default)]
struct Shape {
    params: i32,
    returns: bool,
}

struct Ctx<'a> {
    p: &'a Program,
    items: Vec<(usize, Insn, usize)>,
    routines: HashMap<usize, Shape>,
    /// Routine and entry starts: the operand stack of the code before them does not carry over.
    starts: HashSet<usize>,
    /// Jump targets.
    labels: HashSet<usize>,
}

impl<'a> Ctx<'a> {
    fn new(p: &'a Program, an: &Analysis, known: &dyn Fn(u16, u16) -> Option<Sig>) -> Ctx<'a> {
        let items: Vec<(usize, Insn, usize)> = an.code.iter().map(|(&pc, &(insn, len))| (pc, insn, len)).collect();
        let starts: HashSet<usize> = an.routines.iter().copied().chain(p.entries.iter().map(|e| e.pc as usize)).collect();
        let mut routines = HashMap::new();
        for &r in &an.routines {
            let mut params = 0;
            let mut pc = r;
            while let Some((Insn::Store(_), len)) = an.code.get(&pc) {
                params += 1;
                pc += len;
            }
            // A routine returns a value when a `ret` carries an operand type (0xa9, 0xb9, 0x99), or, since
            // the compiler writes a plain 0x89 after a host call's result too, when the instruction before
            // it leaves a value on the stack. Code runs on into the next routine when nothing separates
            // them, so stop at its start.
            let mut returns = false;
            let mut at = r;
            let mut prev: Option<Insn> = None;
            while let Some((i, len)) = an.code.get(&at) {
                if matches!(i, Insn::Ret) && (ret_has_value(p, at) || prev.is_some_and(|v| produces_value(&v, known))) {
                    returns = true;
                }
                prev = Some(*i);
                at += len;
                if starts.contains(&at) {
                    break;
                }
            }
            routines.insert(r, Shape { params, returns });
        }
        let labels = items
            .iter()
            .filter_map(|(_, i, _)| match i {
                Insn::Jump(t) | Insn::JumpIfZero(t) | Insn::JumpIfNonZero(t) => Some(*t),
                _ => None,
            })
            .collect();
        Ctx { p, items, routines, starts, labels }
    }

    /// `(values taken, values left, ends a statement)` of an instruction, `None` when it is not known.
    fn effect(&self, at: usize, insn: &Insn, lookup: &dyn Fn(u16, u16) -> Option<Sig>) -> Option<(i32, i32, bool)> {
        Some(match insn {
            Insn::PushInt(_) | Insn::PushFloat(_) | Insn::Lea(_) | Insn::Load(_) => (0, 1, false),
            Insn::Store(_) | Insn::CopyTo { .. } | Insn::Pop => (1, 0, true),
            Insn::StoreIndirect | Insn::CopyIndirect { .. } => (2, 0, true),
            Insn::LoadField(_) | Insn::Unary { .. } | Insn::Sin | Insn::Cos | Insn::DegToRad | Insn::RadToDeg => (1, 1, false),
            Insn::Binary { .. } => (2, 1, false),
            Insn::Dup => (1, 2, false),
            Insn::JumpIfZero(_) | Insn::JumpIfNonZero(_) => (1, 0, true),
            Insn::Jump(_) | Insn::Yield | Insn::Exit => (0, 0, true),
            Insn::Ret => (i32::from(ret_has_value(self.p, at)), 0, true),
            Insn::Call { target, .. } => {
                let s = self.routines.get(target).copied().unwrap_or_default();
                (s.params, i32::from(s.returns), !s.returns)
            }
            Insn::Host { group, index } => {
                let (n, r) = lookup(*group, *index)?;
                (n as i32, i32::from(r), !r)
            }
        })
    }

    /// Do the instructions after position `k` need a value from the call at `k`? Runs them on a stack
    /// that holds nothing from the call: `Some(true)` when one of them takes more than is there,
    /// `Some(false)` when the statement ends with the stack empty, `None` when an unknown call or a
    /// join point gets in the way.
    fn needs_result(&self, k: usize, lookup: &dyn Fn(u16, u16) -> Option<Sig>) -> Option<bool> {
        let mut avail = 0i32;
        for (pc, insn, _) in self.items[k + 1..].iter().take(64) {
            if self.labels.contains(pc) || self.starts.contains(pc) {
                return if avail == 0 { Some(false) } else { None };
            }
            let (pops, pushes, ends) = self.effect(*pc, insn, lookup)?;
            if pops > avail {
                return Some(true);
            }
            avail = avail - pops + pushes;
            if ends && avail == 0 {
                return Some(false);
            }
        }
        None
    }

    /// For a call that returns a value: how many values of enclosing expressions the rest of the
    /// statement takes besides the result (the left operand of an arithmetic instruction, arguments
    /// of a call it is an argument of). Those were pushed before the call and are not its arguments.
    fn pending_after(&self, k: usize, lookup: &dyn Fn(u16, u16) -> Option<Sig>) -> Option<i32> {
        let (mut avail, mut owed) = (1i32, 0i32);
        for (pc, insn, _) in self.items[k + 1..].iter().take(64) {
            if self.labels.contains(pc) || self.starts.contains(pc) {
                return if avail == 0 { Some(owed) } else { None };
            }
            let (pops, pushes, ends) = self.effect(*pc, insn, lookup)?;
            if pops > avail {
                owed += pops - avail;
                avail = pops;
            }
            avail = avail - pops + pushes;
            if ends && avail == 0 {
                return Some(owed);
            }
        }
        None
    }

    /// Walks all code in address order tracking the operand stack depth, and records for every call of
    /// an unknown function how many values were pushed since the statement began.
    fn collect(&self, lookup: &dyn Fn(u16, u16) -> Option<Sig>, unknown: &[(u16, u16)]) -> HashMap<(u16, u16), Votes> {
        let mut votes: HashMap<(u16, u16), Votes> = HashMap::new();
        // Depth recorded at jump targets, for code reached only by a jump.
        let mut at_target: HashMap<usize, i32> = HashMap::new();
        let mut d: i32 = 0;
        let mut base: i32 = 0;
        let mut prev_end: Option<usize> = None;
        let mut falls_through = false;
        for (k, &(pc, ref insn, len)) in self.items.iter().enumerate() {
            let contiguous = prev_end == Some(pc);
            if self.starts.contains(&pc) {
                d = 0;
                base = 0;
            } else if !contiguous || !falls_through {
                // Reached by a jump (or after a gap): the depth the jump had.
                d = at_target.get(&pc).copied().unwrap_or(base);
                base = d;
            } else if self.labels.contains(&pc) {
                base = d;
            }
            falls_through = true;
            match insn {
                Insn::PushInt(_) | Insn::PushFloat(_) | Insn::Lea(_) | Insn::Load(_) | Insn::Dup => d += 1,
                Insn::Store(_) | Insn::Pop => {
                    d -= 1;
                    base = d;
                }
                // A copy into a temporary also happens in the middle of an expression (a vector result
                // whose address is then passed on), so it does not end a statement by itself.
                Insn::CopyTo { .. } => d -= 1,
                Insn::StoreIndirect | Insn::CopyIndirect { .. } => {
                    d -= 2;
                    base = d;
                }
                Insn::LoadField(_) | Insn::Unary { .. } | Insn::Sin | Insn::Cos | Insn::DegToRad | Insn::RadToDeg => {}
                Insn::Binary { .. } => d -= 1,
                Insn::Jump(t) => {
                    at_target.insert(*t, d);
                    base = d;
                    falls_through = false;
                }
                Insn::JumpIfZero(t) | Insn::JumpIfNonZero(t) => {
                    d -= 1;
                    at_target.insert(*t, d);
                    base = d;
                }
                Insn::Yield | Insn::Exit => base = d,
                Insn::Ret => falls_through = false,
                Insn::Call { target, .. } => {
                    let s = self.routines.get(target).copied().unwrap_or_default();
                    d += i32::from(s.returns) - s.params;
                    if !s.returns {
                        base = d;
                    }
                }
                Insn::Host { group, index } => {
                    let key = (*group, *index);
                    if let Some((n, r)) = lookup(*group, *index) {
                        d += i32::from(r) - n as i32;
                        if !r {
                            base = d;
                        }
                    } else if unknown.contains(&key) {
                        let immediate = self.items.get(k + 1).is_some_and(|(_, i, _)| drops_value(i));
                        let result = if immediate { Some(true) } else { self.needs_result(k, lookup) };
                        // A void call is a whole statement. One with a result is too when it is dropped or
                        // stored; otherwise the values the rest of the statement takes besides the result
                        // were pushed before the call and are not its arguments.
                        let pending = match result {
                            Some(false) => Some(0),
                            Some(true) if immediate => Some(0),
                            Some(true) => self.pending_after(k, lookup),
                            None => None,
                        };
                        let pushed = d - base;
                        let site = match pending {
                            Some(p) => Site { pushed: pushed - p, result, top_level: true },
                            None => Site { pushed, result, top_level: false },
                        };
                        votes.entry(key).or_default().sites.push(site);
                        // Carry on as if the call took everything pushed since the statement began.
                        let r = result == Some(true);
                        d = base + i32::from(r) + pending.unwrap_or(0);
                        if !r {
                            base = d;
                        }
                    }
                }
            }
            prev_end = Some(pc + len);
        }
        votes
    }
}

/// The instruction takes the value on top of the stack and nothing else uses it: the result of a
/// call that comes right before it is dropped or stored.
fn drops_value(i: &Insn) -> bool {
    matches!(i, Insn::Pop | Insn::Store(_) | Insn::StoreIndirect | Insn::CopyTo { .. } | Insn::CopyIndirect { .. } | Insn::JumpIfZero(_) | Insn::JumpIfNonZero(_))
}

/// The instruction leaves a value on the operand stack that nothing here takes again.
fn produces_value(i: &Insn, known: &dyn Fn(u16, u16) -> Option<Sig>) -> bool {
    match i {
        Insn::PushInt(_) | Insn::PushFloat(_) | Insn::Lea(_) | Insn::Load(_) | Insn::LoadField(_) | Insn::Unary { .. } | Insn::Binary { .. } | Insn::Dup => true,
        Insn::Sin | Insn::Cos | Insn::DegToRad | Insn::RadToDeg => true,
        Insn::Host { group, index } => known(*group, *index).is_some_and(|(_, r)| r),
        _ => false,
    }
}

/// A `ret` carries an operand type when it returns a value (0x89 returns nothing, 0xa9 an address, 0xb9 a word).
fn ret_has_value(p: &Program, at: usize) -> bool {
    (p.word(at).unwrap_or(0) >> 4) & 3 != 0
}
