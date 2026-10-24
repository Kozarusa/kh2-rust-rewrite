//! Oracle tests of the world map's action lists and the story-flag follow-up (core.save, WP-L1-P27a): `FUN_001b6a48` (run the list of a
//! `wldf` record), `FUN_001b6a90` (one command of a list), `FUN_001b6d30` (route states, ops 8 to 0xb), `FUN_001b6da0` (every record that
//! follows a story flag sets its world flag) and `FUN_001b6860` (the table pointer). The port keeps the table as a parsed
//! `kh2_ard::worldmap::Wldf` in `progress::Story`, and the world region as a byte array from `0x341858`; the original reads a table laid out in
//! guest memory behind `DAT_01d5d790` and the whole save block, which is a randomised global here, so every save byte is compared after
//! each case (not only the region). The table has 1 to 40 records with distinct sorted keys and lists drawn from a pool of random commands
//! (ids inside the arrays they index: worlds below 19, routes below 32, episodes below 128).
//!
//! Stand-in: `FUN_001c6d10` (called by op 0xf when the hook argument is set; acts outside the save block, the port records it as unported)
//! is `jr ra`. The list-removal helper of op 0xd also writes outside the region in the original; only the save block is compared.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::sync::Arc;

use kh2_ard::worldmap::Wldf;
use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::*;
use kh2_game_core::progress::{ItemIndex, Progress, world};
use kh2_game_core::progress_flags::{WorldEnv, flag_102b, world_flag_set};
use kh2_game_core::save_block::{LEN, at};

const SAVE: u32 = 0x33e860;
const SAVE_GLOBAL: (u32, u32, bool) = (SAVE, LEN as u32, true);
const TABLE: u32 = 0x01e4_0000;
const SEED: u64 = 0x7a27;
const WORLD: usize = at::WORLD_RECORDS;

fn machine(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.hooks.insert(0x1c6d10, |e| e.gpr[reg::V0] = 0);
    Some(ee)
}

fn list(rng: &mut Rng, ops: &[u16]) -> Vec<u16> {
    let mut out = Vec::new();
    for _ in 0..rng.below(5) {
        let op = ops[rng.below(ops.len() as u64) as usize];
        let n = 1 + rng.below(3) as usize;
        out.extend([op, n as u16]);
        for _ in 0..n {
            let id = match op {
                1..=7 => rng.below(19),
                8..=0xb => rng.below(32),
                0xc | 0xd => rng.below(128),
                _ => rng.word() & 0xffff,
            };
            out.extend([id as u16, 0]);
        }
    }
    out.extend([0, 0]);
    out
}

/// The entry: header (word, count), `n` records of 16 bytes (key, kind, ?, ?, story flag, event, list offset), then the pool of lists.
fn entry(rng: &mut Rng, ops: &[u16], flags: u64) -> Vec<u8> {
    let n = 1 + rng.below(40) as usize;
    let mut keys: Vec<u8> = Vec::new();
    while keys.len() < n {
        let k = rng.word() as u8;
        if !keys.contains(&k) {
            keys.push(k);
        }
    }
    keys.sort_unstable();
    let pool: Vec<Vec<u16>> = (0..8).map(|_| list(rng, ops)).collect();
    let mut e = vec![0u8; 8 + 16 * n];
    e[4..8].copy_from_slice(&(n as u32).to_le_bytes());
    let mut offsets = Vec::new();
    for l in &pool {
        offsets.push(e.len() as u32);
        for h in l {
            e.extend(h.to_le_bytes());
        }
    }
    for (i, k) in keys.iter().enumerate() {
        let r = 8 + 16 * i;
        for b in &mut e[r + 2..r + 16] {
            *b = rng.word() as u8;
        }
        e[r] = *k;
        e[r + 1] = rng.below(3) as u8;
        e[r + 4..r + 8].copy_from_slice(&(rng.below(flags) as u32).to_le_bytes());
        let off = if rng.below(6) == 0 { 0 } else { offsets[rng.below(8) as usize] };
        e[r + 12..r + 16].copy_from_slice(&off.to_le_bytes());
    }
    e
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A `Progress` whose story has the table and the world region of the case.
fn port_world(table: &Arc<Wldf>, region: &[u8]) -> Progress {
    let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
    p.story().world_table = table.clone();
    let text = format!("{{\"world_state\": \"{}\"}}", hex(&region[..world::LEN]));
    p.apply_json(&text).expect("world_state");
    p
}

/// Runs `FUN_001b6a48(record, hook)` over random tables made from `ops`.
fn list_case(label: &str, seed: u64, ops: &[u16]) {
    let Some(mut ee) = machine(&format!("oracle test of {label}")) else { return };
    let mut rng = Rng::new(seed);
    let e = entry(&mut rng, ops, 1);
    ee.write(TABLE, &e);
    ee.w32(0x1d5d790, TABLE);
    let n = u32::from_le_bytes(e[4..8].try_into().unwrap()) as usize;
    let table = Arc::new(Wldf::parse(&e));
    for case in 0..3000 {
        let junk: Vec<u8> = (0..LEN).map(|_| rng.word() as u8).collect();
        ee.write(SAVE, &junk);
        let rec = rng.below(n as u64) as usize;
        let hook = rng.below(2) as u32;
        ee.w32(0x1d5d794, 0);
        ee.call(0x1b6a48, &[u64::from(TABLE) + 8 + 16 * rec as u64, hook as u64], &[], STACK).expect("original");
        let mut want = junk.clone();
        let p = port_world(&table, &junk[WORLD..]);
        p.story().world_run_list(e[8 + 16 * rec], hook != 0);
        let out = p.story().world_bytes();
        want[WORLD..WORLD + world::LEN].copy_from_slice(&out);
        let game: Vec<u8> = (0..LEN as u32).map(|o| ee.r8(SAVE + o)).collect();
        if let Some(i) = (0..LEN).find(|&i| game[i] != want[i]) {
            panic!("{label} case {case}: record {rec} (key {}), hook {hook}: save byte {i:#x}: game {:#x} port {:#x}", e[8 + 16 * rec], game[i], want[i]);
        }
    }
    eprintln!("{label}: 3000 cases equal");
}

/// The list runner `FUN_001b6a48(record, hook)` over every op.
#[test]
fn orig_1b6a48_matches_the_game() {
    list_case("0x1b6a48", 0x1b6a48, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 0xa, 0xb, 0xc, 0xd, 0xe, 0xf, 0x10]);
}

/// One command of a list, `FUN_001b6a90` (map states, bits, episode bits, the map byte; the unknown ops do nothing).
#[test]
fn orig_1b6a90_matches_the_game() {
    list_case("0x1b6a90", 0x1b6a90, &[1, 2, 3, 4, 5, 6, 7, 0xc, 0xd, 0xe, 0xf, 0x10]);
}

/// `FUN_001b6d30(command, state)`: ops 8 to 0xb write state `op - 8` to the route byte of each id.
#[test]
fn orig_1b6d30_matches_the_game() {
    list_case("0x1b6d30", 0x1b6d30, &[8, 9, 0xa, 0xb]);
}

/// The world flag follow-up of a story flag, `FUN_001b6da0(flag)`: every record whose story flag is `flag` sets its world flag
/// (`FUN_001b68f8`). The port's body is `Wldf::following` and `world_flag_set` (the loop of the private `Progress::world_flags_follow`).
#[test]
fn orig_1b6da0_matches_the_game() {
    struct Env<'a> {
        e: &'a [u8],
        second: bool,
    }
    impl WorldEnv for Env<'_> {
        fn record_byte(&self, index: u32, offset: usize) -> u8 {
            let n = u32::from_le_bytes(self.e[4..8].try_into().unwrap()) as usize;
            (0..n).find(|&i| u32::from(self.e[8 + 16 * i]) == index).map_or(0, |i| self.e[8 + 16 * i + offset])
        }
        fn second_part(&self) -> bool {
            self.second
        }
        fn run_list(&mut self, _: &mut [u8], _: u8, _: bool) {}
    }
    let Some(mut ee) = machine("oracle test of 0x1b6da0") else { return };
    let mut rng = Rng::new(0x1b6da0);
    let e = entry(&mut rng, &[3], 6);
    ee.write(TABLE, &e);
    ee.w32(0x1d5d790, TABLE);
    let table = Wldf::parse(&e);
    static ARGS: [Arg; 1] = [Arg::Range(0, 7)];
    let spec = Spec { addr: 0x1b6da0, args: &ARGS, floats: 0, ret: Ret::None, globals: &[SAVE_GLOBAL], stubs: &[], nulls: false };
    let st = fuzz_with(&mut ee, &spec, 3000, SEED, |c| {
        let flag = c.int(0) as u32;
        let second = flag_102b(&c.global(0)[at::FLAGS..]);
        let env = Env { e: &e, second };
        for r in table.following(flag) {
            world_flag_set(&mut c.global(0)[WORLD..], &env, u32::from(r.index));
        }
        Out::none()
    });
    eprintln!("0x1b6da0: {} cases equal", st.cases);
}

/// `FUN_001b6860(table)`: stores the pointer at `DAT_01d5d790`. The port keeps the parsed table in `Story::world_table` instead of a
/// pointer, so this test only shows what the original does (one word written).
#[test]
fn orig_1b6860_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x1b6860") else { return };
    let mut rng = Rng::new(0x1b6860);
    for case in 0..500 {
        let p = rng.word() as u32;
        ee.w32(0x1d5d790, !p);
        ee.call(0x1b6860, &[p as u64], &[], STACK).expect("original");
        assert_eq!(ee.r32(0x1d5d790), p, "case {case}");
    }
}
