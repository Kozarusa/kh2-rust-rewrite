//! Flow-following disassembler. Code is found by walking from every entry point through jumps and
//! calls; whatever is left over is data (constant pools, strings, argument blocks).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use crate::insn::{Base, Insn, Operand, decode};
use crate::names;
use crate::program::{Error, Program};

pub struct Analysis {
    /// Decoded instructions by word index, with their length in words.
    pub code: BTreeMap<usize, (Insn, usize)>,
    /// Targets of calls (routine starts).
    pub routines: BTreeSet<usize>,
    /// Word indices addressed through `Base::Code` operands (start of a data object).
    pub data_refs: BTreeSet<usize>,
}

/// Walks all code reachable from the entry points. Any word on the way that does not decode, or any
/// jump that leaves the code area, is an error.
pub fn analyze(p: &Program) -> Result<Analysis, Error> {
    let mut a = Analysis { code: BTreeMap::new(), routines: BTreeSet::new(), data_refs: BTreeSet::new() };
    let mut work: Vec<usize> = p.entries.iter().map(|e| e.pc as usize).collect();
    let end = p.words.len();
    let mut registered: BTreeSet<usize> = BTreeSet::new();
    loop {
        // Mode registrations (host 1.6) pass code addresses as plain integers: treat them as entry points.
        let regs: Vec<usize> = a
            .code
            .iter()
            .filter(|(pc, (i, _))| matches!(i, Insn::Host { group: 1, index: 6 }) && !registered.contains(pc))
            .map(|(pc, _)| *pc)
            .collect();
        for r in regs {
            registered.insert(r);
            let prev: Vec<&Insn> = a.code.range(..r).rev().take(12).map(|(_, (i, _))| i).collect();
            if prev.len() == 12 {
                // prev[0] is the last pushed argument (args[11]); args[3..=11] are routine addresses.
                for ins in prev.iter().take(9) {
                    if let &&Insn::PushInt(v) = ins
                        && v != 0 && (v as usize) < end {
                            work.push(v as usize);
                            a.routines.insert(v as usize);
                        }
                }
            }
        }
        if work.is_empty() {
            break;
        }
        walk(p, &mut a, &mut work)?;
    }
    Ok(a)
}

fn walk(p: &Program, a: &mut Analysis, work: &mut Vec<usize>) -> Result<(), Error> {
    let end = p.words.len();
    while let Some(mut pc) = work.pop() {
        loop {
            if pc >= end {
                return Err(Error::FlowOutOfRange { pc });
            }
            if a.code.contains_key(&pc) {
                break;
            }
            let (insn, len) = decode(p, pc).ok_or(Error::BadCode { pc, word: p.words[pc] })?;
            a.code.insert(pc, (insn, len));
            let next = pc + len;
            match insn {
                Insn::Jump(t) => {
                    work.push(t);
                    break;
                }
                Insn::JumpIfZero(t) | Insn::JumpIfNonZero(t) => {
                    work.push(t);
                }
                Insn::Call { target, .. } => {
                    a.routines.insert(target);
                    work.push(target);
                }
                Insn::Ret => break,
                Insn::Lea(Operand { base: Base::Code, offset })
                | Insn::Load(Operand { base: Base::Code, offset })
                | Insn::Store(Operand { base: Base::Code, offset })
                | Insn::CopyTo { dst: Operand { base: Base::Code, offset }, .. } => {
                    a.data_refs.insert(offset as usize);
                }
                _ => {}
            }
            pc = next;
        }
    }
    Ok(())
}

fn operand_text(o: Operand) -> String {
    match o.base {
        Base::Local => format!("local+{:#x}", o.offset),
        Base::Global => format!("global+{:#x}", o.offset),
        Base::LocalPtr => format!("[local0]+{:#x}", o.offset),
        Base::Code => format!("code[{:#x}]", o.offset),
        Base::Absolute => format!("abs {:#x}", o.offset),
    }
}

/// Mnemonic and operands of one instruction.
pub fn text(i: &Insn) -> String {
    use Insn::*;
    match *i {
        PushInt(v) => format!("push.i {}", v as i32),
        PushFloat(v) => format!("push.f {v}"),
        Lea(o) => format!("lea {}", operand_text(o)),
        Load(o) => format!("load {}", operand_text(o)),
        Store(o) => format!("store {}", operand_text(o)),
        CopyTo { dst, size } => format!("copyto {}, {size}", operand_text(dst)),
        LoadField(v) => format!("ldf +{v:#x}"),
        StoreIndirect => "store.ind".into(),
        CopyIndirect { size } => format!("copy.ind {size}"),
        Unary { float, op } => format!("{}.{:?}", if float { "f" } else { "i" }, op).to_lowercase(),
        Binary { float, op } => format!("{}.{:?}", if float { "f" } else { "i" }, op).to_lowercase(),
        Jump(t) => format!("jmp L{t:04x}"),
        JumpIfZero(t) => format!("jz L{t:04x}"),
        JumpIfNonZero(t) => format!("jnz L{t:04x}"),
        Call { frame_words, target } => format!("call R{target:04x} frame={frame_words}"),
        Yield => "yield".into(),
        Exit => "exit".into(),
        Ret => "ret".into(),
        Pop => "pop".into(),
        Dup => "dup".into(),
        Sin => "sin".into(),
        Cos => "cos".into(),
        DegToRad => "deg2rad".into(),
        RadToDeg => "rad2deg".into(),
        Host { group, index } => match names::host_name(group, index) {
            Some(n) => format!("host {group}.{index} {n}"),
            None => format!("host {group}.{index}"),
        },
    }
}

fn printable_string(p: &Program, at: usize) -> Option<String> {
    let mut bytes = Vec::new();
    let mut k = at;
    loop {
        let w = p.word(k)?;
        for b in [w as u8, (w >> 8) as u8] {
            if b == 0 {
                return if bytes.len() >= 2 { String::from_utf8(bytes).ok() } else { None };
            }
            if !(0x20..0x7f).contains(&b) {
                return None;
            }
            bytes.push(b);
        }
        k += 1;
        if bytes.len() > 64 {
            return None;
        }
    }
}

/// Full listing: header, entry table, code with labels, and leftover words as data.
pub fn listing(p: &Program, a: &Analysis) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "; {}  globals {:#x}  frame stack {:#x}  operand stack {:#x}  {} words",
        p.name,
        p.globals,
        p.frame_stack,
        p.value_stack,
        p.words.len()
    );
    for e in &p.entries {
        let _ = writeln!(s, "; entry event {} ({}) -> {:04x}", e.id, names::event_name(e.id), e.pc);
    }
    let mut labels: BTreeSet<usize> = BTreeSet::new();
    for (i, _) in a.code.values() {
        match *i {
            Insn::Jump(t) | Insn::JumpIfZero(t) | Insn::JumpIfNonZero(t) => {
                labels.insert(t);
            }
            _ => {}
        }
    }
    let mut pc = p.table_end();
    let end = p.words.len();
    while pc < end {
        if let Some(e) = p.entries.iter().find(|e| e.pc as usize == pc) {
            let _ = writeln!(s, "\n{:04x}: ; ---- entry: event {} ({})", pc, e.id, names::event_name(e.id));
        } else if a.routines.contains(&pc) {
            let _ = writeln!(s, "\nR{pc:04x}:");
        }
        if labels.contains(&pc) {
            let _ = writeln!(s, "L{pc:04x}:");
        }
        if let Some((i, len)) = a.code.get(&pc) {
            let raw: Vec<String> = (0..*len).map(|k| format!("{:04x}", p.words[pc + k])).collect();
            let mut line = format!("  {pc:04x}:  {:<16} {}", raw.join(" "), text(i));
            if let Insn::Lea(Operand { base: Base::Code, offset }) = i
                && let Some(t) = printable_string(p, *offset as usize) {
                    let _ = write!(line, "   ; \"{t}\"");
                }
            let _ = writeln!(s, "{line}");
            pc += len;
        } else {
            // Data run: until the next decoded instruction.
            let next = a.code.range(pc..).next().map(|(k, _)| *k).unwrap_or(end);
            let mut k = pc;
            while k < next {
                if let Some(t) = printable_string(p, k).filter(|t| a.data_refs.contains(&k) || t.len() >= 4) {
                    let _ = writeln!(s, "  {k:04x}:  .string \"{t}\"");
                    k += t.len() / 2 + 1;
                    continue;
                }
                // A run of raw words, cut where the next referenced object starts.
                let cut = a.data_refs.range(k + 1..).next().copied().unwrap_or(next);
                let stop = (k + 8).min(next).min(cut);
                let chunk: Vec<String> = (k..stop).map(|j| format!("{:04x}", p.words[j])).collect();
                let _ = writeln!(s, "  {k:04x}:  .data {}", chunk.join(" "));
                k = stop;
            }
            pc = next.max(pc + 1);
        }
    }
    s
}
