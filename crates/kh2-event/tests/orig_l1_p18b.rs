//! Oracle tests of the area-jump code that ports original functions, run in kh2-ee on the user's executable.
//!
//! `orig_199058_matches_the_game`: `faces_volume` against the facing test of `FUN_00199058` (the player's position and
//! forward vector come from hooks on `FUN_0016b960` and `FUN_0016a890`; the prompt functions it then calls are recording
//! stubs, only checked to run exactly when the test passes).
//!
//! `orig_180ff8_matches_the_game`: `kh2_ard::parse_trigger` and `JumpExtras::from_subs` against `FUN_00180ff8` over random
//! well-formed trigger commands: the original's calls to the record functions (`FUN_001975e0`, `FUN_00197818`, `FUN_0019d8f0`,
//! `FUN_00197808`, `FUN_00197810`, `FUN_00197828`) are recording hooks and the four record words the function writes itself
//! (+0x44, +0x48, +0x4c, +0x50) are compared. Skips without a disc (fails with `KH2_REQUIRE_DISC=1`).

use bevy::prelude::Vec3;
use kh2_ard::{parse_trigger, Activator};
use kh2_ee::Ee;
use kh2_event::area_jump::{faces_volume, JumpExtras, Walker};
use kh2_factory::fuzz::Rng;
use kh2_game_core::area::game_to_app;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const STACK: u32 = 0x01ff_0000;
const CMD: u32 = 0x0181_0000;
const REC: u32 = 0x0182_0000;
const POS: u32 = 0x0183_0000;
const PAIR: u32 = 0x0184_0000;

fn coord(rng: &mut Rng) -> f32 {
    (rng.below(2_000_001) as f32 - 1_000_000.0) / 100.0
}

fn unit(rng: &mut Rng) -> f32 {
    (rng.below(20_001) as f32 - 10_000.0) / 10_000.0
}

#[test]
fn orig_199058_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x199058") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let forward = Rc::new(Cell::new([0f32; 4]));
    let prompts = Rc::new(Cell::new(0u32));
    ee.hooks.insert(0x16b960, |e: &mut Ee| e.gpr[2] = POS as u128);
    let f = forward.clone();
    ee.hooks.insert(0x16a890, move |e: &mut Ee| {
        let out = e.gpr[4] as u32;
        for (k, v) in f.get().iter().enumerate() {
            e.wf32(out + 4 * k as u32, *v);
        }
    });
    ee.hooks.insert(0x1953e0, |e: &mut Ee| e.gpr[2] = 7);
    for addr in [0x15ad80, 0x15acc8] {
        let p = prompts.clone();
        ee.hooks.insert(addr, move |_e: &mut Ee| p.set(p.get() + 1));
    }
    let mut rng = Rng::new(0x199058);
    let (mut yes, mut no) = (0, 0);
    for case in 0..6000 {
        let player = [coord(&mut rng), coord(&mut rng), coord(&mut rng)];
        let mut centre = [coord(&mut rng), coord(&mut rng), coord(&mut rng)];
        match rng.below(8) {
            0 => centre = [player[0], centre[1], player[2]],
            1 => centre = [player[0] + 1.0, centre[1], player[2]],
            2 => centre[1] = player[1],
            _ => {}
        }
        let mut fwd = [unit(&mut rng), unit(&mut rng), unit(&mut rng), 0.0];
        match rng.below(8) {
            0 => fwd[1] = 0.0,
            1 => fwd = [0.0, unit(&mut rng), 0.0, 0.0],
            2 => fwd[1] *= 4.0,
            _ => {}
        }
        forward.set(fwd);
        for (k, v) in player.iter().enumerate() {
            ee.wf32(POS + 4 * k as u32, *v);
        }
        ee.wf32(POS + 12, 1.0);
        let mut record = [0u8; 0x40];
        for (k, v) in centre.iter().enumerate() {
            record[4 + 4 * k..8 + 4 * k].copy_from_slice(&v.to_le_bytes());
        }
        ee.write(REC, &record);
        ee.write(PAIR, &[3, 4]);
        let wide = rng.below(2);
        prompts.set(0);
        let r = ee.call(0x199058, &[0, PAIR as u64, REC as u64, wide, 5], &[], STACK).unwrap_or_else(|f| panic!("case {case}: {f:?}"));
        let game = r & 0xffff_ffff == 1;
        assert_eq!(prompts.get(), game as u32, "case {case}: the original raises the prompt exactly when the test passes");
        let activator = Activator { shape: 0, option: 0, position: centre, scale: [1.0; 3], rotation: [0.0; 3], raw: record };
        let walker = Walker { pos: game_to_app(player), facing: Vec3::new(fwd[0], -fwd[1], -fwd[2]), vel: Vec3::ZERO };
        assert_eq!(faces_volume(&activator, &walker), game, "case {case}: player {player:?} centre {centre:?} forward {fwd:?}");
        if game {
            yes += 1;
        } else {
            no += 1;
        }
    }
    assert!(yes > 1000 && no > 1000, "yes {yes} no {no}");
}

type Log = Rc<RefCell<Vec<(u32, Vec<u32>)>>>;

fn hooks(ee: &mut Ee, log: &Log) {
    let l = log.clone();
    let push = move |id: u32, v: Vec<u32>| l.borrow_mut().push((id, v));
    let p = push.clone();
    ee.hooks.insert(0x1975e0, move |e: &mut Ee| {
        p(100, vec![e.gpr[4] as u32 & 0xffff, e.gpr[5] as u32 & 0xffff]);
        e.gpr[2] = REC as u128;
    });
    let p = push.clone();
    ee.hooks.insert(0x197818, move |e: &mut Ee| {
        assert_eq!(e.gpr[4] as u32, REC);
        p(0, vec![e.gpr[5] as u32, e.gpr[6] as u32 & 0xffff]);
    });
    let p = push.clone();
    ee.hooks.insert(0x19d8f0, move |e: &mut Ee| {
        assert_eq!(e.gpr[4] as u32, REC + 0x34);
        let at = e.gpr[5] as u32;
        p(1, (0..6).map(|k| e.r16(at + 2 * k) as u32).collect());
    });
    let p = push.clone();
    ee.hooks.insert(0x197808, move |e: &mut Ee| p(2, vec![e.gpr[5] as u32 & 0xffff]));
    let p = push.clone();
    ee.hooks.insert(0x197810, move |e: &mut Ee| p(3, vec![e.gpr[5] as u32 & 0xffff]));
    ee.hooks.insert(0x197828, move |e: &mut Ee| {
        let (n, at) = (e.gpr[5] as u32 & 0xffff, e.gpr[6] as u32);
        let mut v = vec![n];
        v.extend((0..2 * n).map(|k| e.r16(at + 2 * k) as u32));
        push(6, v);
    });
}

/// A random well-formed trigger command in halfwords after the header: event, argument, then sub-commands.
fn command(rng: &mut Rng) -> Vec<u16> {
    let mut h = vec![rng.word() as u16, rng.word() as u16];
    for _ in 0..rng.below(7) {
        let op = rng.below(9) as u16;
        let len = match op {
            0 => 4,
            1 => 8,
            6 => 2 + 2 * rng.below(4) as usize,
            _ => 2,
        };
        let start = h.len();
        h.extend((0..len).map(|_| rng.word() as u16));
        h[start] = op;
        if op == 6 {
            h[start + 1] = ((len - 2) / 2) as u16;
        }
    }
    h
}

#[test]
fn orig_180ff8_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x180ff8") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    hooks(&mut ee, &log);
    let mut rng = Rng::new(0x180ff8);
    let mut seen = [0u32; 9];
    for case in 0..4000 {
        let h = command(&mut rng);
        let words = h.len() / 2;
        // The command: opcode 0xc, the word count, then the halfwords, padded with random bytes.
        let mut bytes = vec![0xc, 0, words as u8, (words >> 8) as u8];
        bytes.extend(h.iter().flat_map(|v| v.to_le_bytes()));
        bytes.extend((0..64).map(|_| rng.word() as u8));
        ee.write(CMD, &bytes);
        let record: Vec<u8> = (0..0x60).map(|_| rng.word() as u8).collect();
        ee.write(REC, &record);
        log.borrow_mut().clear();
        ee.call(0x180ff8, &[0, CMD as u64], &[], STACK).unwrap_or_else(|f| panic!("case {case}: {f:?}"));
        let game_log = log.borrow().clone();
        let game_words: Vec<u32> = [0x44, 0x48, 0x4c, 0x50].iter().map(|o| ee.r32(REC + o)).collect();
        // The port.
        let args: Vec<u32> = h.chunks(2).map(|c| c[0] as u32 | (c[1] as u32) << 16).collect();
        let t = parse_trigger(&args).unwrap_or_else(|| panic!("case {case}: the port rejects a well-formed command {h:x?}"));
        let mut mine = vec![(100, vec![t.event as u32, t.arg as u32])];
        for (op, s) in &t.subs {
            seen[*op as usize] += 1;
            match op {
                0 => mine.push((0, vec![s[1] as u32 | (s[2] as u32) << 16, s[0] as u32])),
                1 => mine.push((1, s[1..7].iter().map(|v| *v as u32).collect())),
                2 | 3 => mine.push((*op as u32, vec![s[0] as u32])),
                6 => mine.push((6, s.iter().map(|v| *v as u32).collect())),
                _ => {}
            }
        }
        assert_eq!(game_log, mine, "case {case}: calls out for {h:x?}");
        let extras = JumpExtras::from_subs(&t.subs);
        let initial: Vec<u32> = [0x44usize, 0x48, 0x4c, 0x50].iter().map(|o| u32::from_le_bytes(record[*o..*o + 4].try_into().unwrap())).collect();
        let has = |op: u16| t.subs.iter().any(|(o, _)| *o == op);
        let expect = [
            if has(4) { extras.party as i32 as u32 } else { initial[0] },
            if has(5) { extras.reset as i32 as u32 } else { initial[1] },
            if has(7) { extras.menu_before as i32 as u32 } else { initial[2] },
            if extras.flag { initial[3] | 1 } else { initial[3] },
        ];
        assert_eq!(game_words, expect, "case {case}: record words for {h:x?}");
    }
    assert!(seen.iter().all(|&c| c > 200), "sub-command coverage {seen:?}");
}

/// The decoder refuses what the original would read past the end of or never leave: a sub-command that overruns, and an
/// unknown sub-command (the original loops on it forever, so there is nothing to compare).
#[test]
fn parse_trigger_rejects_overruns_and_unknown_subcommands() {
    assert!(parse_trigger(&[0x0002_0001, 0x0000_0001]).is_none(), "sub-command 1 needs eight halfwords");
    assert!(parse_trigger(&[0x0002_0001, 0x0000_0009]).is_none(), "unknown sub-command");
    assert!(parse_trigger(&[0x0002_0001]).is_some_and(|t| t.subs.is_empty()));
}
