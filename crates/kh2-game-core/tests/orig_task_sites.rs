//! The task priority table (`kh2_game_core::task::SITES`) against the user's executable: every listed caller is scanned from its
//! first instruction for the calls `jal 0x102778` (and the tail calls `j 0x102778`), the argument registers a1 (bits), a2 (priority) and a3 (function) are followed
//! through the instructions that set them (lui, ori, addiu, daddiu, register moves, the delay slot included), and the calls of each
//! caller must give the table's rows in order. A register set by anything else (a load, arithmetic on an unknown value) is unknown,
//! and an unknown argument fails the test.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_game_core::task::{FRAME_PRIORITY, FRAME_TASK, On, SITES};

const CREATE: u32 = 0x102778;
const JAL_CREATE: u32 = (3 << 26) | (CREATE >> 2);
/// The tail-call form `j 0x102778`.
const J_CREATE: u32 = (2 << 26) | (CREATE >> 2);
/// The longest scan (every listed caller is shorter).
const SCAN: u32 = 0x4000;

/// The (bits, priority, function) of each `jal 0x102778` of `caller` (up to its end, at most `SCAN` bytes), `None` for an argument that is not a
/// constant.
fn calls(ee: &mut Ee, caller: u32, end: u32) -> Vec<[Option<u32>; 3]> {
    let mut reg: [Option<u32>; 32] = [None; 32];
    reg[0] = Some(0);
    let mut out = Vec::new();
    let mut pc = caller;
    // The call whose delay slot is next: Some(true) for `jal 0x102778`, Some(false) for any other call.
    let mut pending: Option<bool> = None;
    while pc < end.min(caller + SCAN) {
        let w = ee.r32(pc);
        step(&mut reg, w);
        match pending.take() {
            Some(true) => {
                out.push([reg[5], reg[6], reg[7]]);
                clobber(&mut reg);
                // A tail call ends the function.
                if ee.r32(pc - 4) == J_CREATE {
                    break;
                }
            }
            Some(false) => clobber(&mut reg),
            None => {}
        }
        let jalr = w >> 26 == 0 && w & 0x3f == 0x09;
        if w == JAL_CREATE || w == J_CREATE {
            pending = Some(true);
        } else if w >> 26 == 3 || jalr {
            pending = Some(false);
        }
        // A `jr ra` or a jump (a tail call) ends the function when the next function's prologue (`addiu sp, sp, -n`, after any
        // padding) follows its delay slot; any other is an early return or a jump inside the function.
        if w == 0x03e0_0008 || w >> 26 == 2 {
            let mut a = pc + 8;
            while ee.r32(a) == 0 && a < pc + 0x20 {
                a += 4;
            }
            let next = ee.r32(a);
            if next >> 16 == 0x27bd && next & 0x8000 != 0 {
                let slot = ee.r32(pc + 4);
                step(&mut reg, slot);
                if pending.take() == Some(true) {
                    out.push([reg[5], reg[6], reg[7]]);
                }
                break;
            }
        }
        pc += 4;
    }
    out
}

/// A call leaves the argument, return and temporary registers unknown.
fn clobber(reg: &mut [Option<u32>; 32]) {
    for r in (1..=15).chain([24, 25, 31]) {
        reg[r] = None;
    }
}
/// Follows one instruction's write to a general register.
fn step(reg: &mut [Option<u32>; 32], w: u32) {
    let op = w >> 26;
    let rs = ((w >> 21) & 31) as usize;
    let rt = ((w >> 16) & 31) as usize;
    let rd = ((w >> 11) & 31) as usize;
    let imm = w & 0xffff;
    let simm = imm as u16 as i16 as i32 as u32;
    let set = |reg: &mut [Option<u32>; 32], r: usize, v: Option<u32>| {
        if r != 0 {
            reg[r] = v;
        }
    };
    match op {
        0 => {
            let funct = w & 0x3f;
            // addu, daddu, or with zero: a move.
            let v = match funct {
                0x21 | 0x2d | 0x25 if rt == 0 => reg[rs],
                0x21 | 0x2d | 0x25 if rs == 0 => reg[rt],
                // jr, jalr, syscall, break, sync, mult/div (no rd write): nothing to follow
                0x08 | 0x09 | 0x0c | 0x0d | 0x0f | 0x18 | 0x19 | 0x1a | 0x1b => return,
                _ => None,
            };
            set(reg, rd, v);
        }
        0x0f => set(reg, rt, Some(imm << 16)),
        0x0d => set(reg, rt, reg[rs].map(|v| v | imm)),
        0x09 | 0x19 => set(reg, rt, reg[rs].map(|v| v.wrapping_add(simm))),
        // Branches and jumps write no general register (jal writes ra, which the scan does not follow).
        0x01..=0x07 | 0x14..=0x17 => {
            if op == 3 {
                set(reg, 31, None);
            }
        }
        // Stores write memory only.
        0x28..=0x2f | 0x39 | 0x3d | 0x3e | 0x3f | 0x1f => {}
        // Everything else that has an rt destination (loads, other immediates): unknown.
        _ => set(reg, rt, None),
    }
}

#[test]
fn task_creation_sites_match_the_executable() {
    let Some(elf) = kh2_factory::exe_or_skip("task_creation_sites_match_the_executable") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut callers: Vec<u32> = SITES.iter().map(|s| s.caller).collect();
    callers.dedup();
    let mut checked = 0;
    for caller in callers {
        let want: Vec<[Option<u32>; 3]> =
            SITES.iter().filter(|s| s.caller == caller).map(|s| [Some(s.bits), Some(s.priority as u32), Some(s.function)]).collect();
        // Another listed caller is the next function start at the latest.
        let end = SITES.iter().map(|s| s.caller).filter(|&c| c > caller).min().unwrap_or(u32::MAX);
        let got = calls(&mut ee, caller, end);
        assert_eq!(got, want, "the task creations of {caller:#x}: (bits, priority, function)");
        checked += got.len();
    }
    assert_eq!(checked, SITES.len());
}

/// The order the frame step relies on: the frame task has the highest priority of the root manager (it runs last in every pass),
/// and the count-down timer the lowest.
#[test]
fn the_frame_task_is_the_last_task_of_the_root_manager() {
    let root: Vec<_> = SITES.iter().filter(|s| s.on == On::Root).collect();
    let max = root.iter().map(|s| s.priority).max().unwrap();
    assert_eq!(max, FRAME_PRIORITY);
    assert!(root.iter().any(|s| s.function == FRAME_TASK && s.priority == FRAME_PRIORITY));
    assert_eq!(root.iter().min_by_key(|s| s.priority).unwrap().function, 0x14c560);
}
