//! Oracle tests of the card operations of the file layer (`FUN_0023d538`, `FUN_0023dad8`, `FUN_0023db48`, `FUN_0023dbb0`, `FUN_0023dbf0`,
//! `FUN_0023dc28`, `FUN_0023dc90`): each builds a name or path and queues one request. kh2-ee runs the original on a random driver memory,
//! a random card info block, name and path buffers; the port runs on copies; the driver memory, the path and the name buffer are compared.
//! The enqueue functions underneath run for real. Skips without a disc; fails without one when `KH2_REQUIRE_DISC=1`.

mod common;

use common::*;

/// Starts kh2-ee (`Ee`) on the user's executable with the unit's pointer globals set; `None` without a disc.
fn start(what: &str, seed: u64) -> Option<Orig> {
    let o = Orig::new(what, seed)?;
    let _: &kh2_ee::Ee = &o.ee;
    Some(o)
}

use kh2_save::dir::{NAME_LEN, NameBuf};
use kh2_save::names::NameEnv;
use kh2_save::ops::{Handles, Ops, PATH_LEN};
use kh2_save::ring::{CARD_BASE, CARD_LEN, CORE_BASE, CORE_LEN, Driver};
use kh2_save::slot::{CardInfo, INFO_LEN};

const CASES: u32 = 500;

fn random_driver(o: &mut Orig) -> Driver {
    let mut d = Driver { core: o.random(CORE_LEN).try_into().unwrap(), card: o.random(CARD_LEN) };
    d.set_word(0x36c258, o.rng.below(64) as u32);
    d.set_word(0x36c25c, o.rng.below(64) as u32);
    d
}

/// A setup: random driver, card info with port 0 or 1, a name from the shapes the screens use, random path bytes.
fn setup(o: &mut Orig, env: &NameEnv) -> Ops {
    let driver = random_driver(o);
    let mut info = CardInfo(o.random(INFO_LEN));
    info.0[0] = o.rng.below(2) as u8;
    let mut name = NameBuf::default();
    let pick = o.rng.below(5);
    let text = match pick {
        0 => "icon.sys".to_string(),
        1 => "kh2.ico".to_string(),
        2 => "*".to_string(),
        _ => kh2_save::names::save_dir_name(env, o.rng.below(99) as i64),
    };
    name.0.copy_from_slice(&o.random(NAME_LEN));
    name.set(text.as_bytes());
    let path: [u8; PATH_LEN] = o.random(PATH_LEN).try_into().unwrap();
    Ops { env: env.clone(), handles: Handles { name: NAME, path: PATH, file: FILEBUF, table: DIRTAB }, info, name, path, driver }
}

fn load(o: &mut Orig, ops: &Ops) {
    o.put(CORE_BASE, &ops.driver.core);
    o.put(CARD_BASE, &ops.driver.card);
    o.put(INFO, &ops.info.0);
    o.put(NAME, &ops.name.0);
    o.put(PATH, &ops.path);
}

fn compare(o: &Orig, ops: &Ops, what: &str) {
    same_bytes(&o.get(CORE_BASE, CORE_LEN), &ops.driver.core, &format!("{what}: core words"));
    same_bytes(&o.get(CARD_BASE, CARD_LEN), &ops.driver.card, &format!("{what}: card span"));
    same_bytes(&o.get(NAME, NAME_LEN), &ops.name.0, &format!("{what}: name buffer"));
    same_bytes(&o.get(PATH, PATH_LEN), &ops.path, &format!("{what}: path buffer"));
}

fn region(o: &mut Orig) -> NameEnv {
    let region = 1 + o.rng.below(4) as u32;
    o.set_word(0x349d44, region);
    o.name_env(region)
}

#[test]
fn orig_23d538_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23d538", 80) else { return };
    for n in 0..CASES {
        let env = region(&mut o);
        let mut ops = setup(&mut o, &env);
        load(&mut o, &ops);
        let sel: i64 = [-1, -4, -5, -3, -2, 0, 7, 98, 99, -6][o.rng.below(10) as usize];
        o.call(0x23d538, &[sel as u64]);
        ops.list_root(sel);
        compare(&o, &ops, &format!("case {n}: selector {sel}"));
    }
}

#[test]
fn orig_23dad8_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23dad8", 81) else { return };
    for n in 0..CASES {
        let env = region(&mut o);
        let mut ops = setup(&mut o, &env);
        load(&mut o, &ops);
        let mode = (n % 2) as i64;
        o.call(0x23dad8, &[mode as u64]);
        ops.change_dir(mode);
        compare(&o, &ops, &format!("case {n}: mode {mode}"));
    }
}

#[test]
fn orig_23db48_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23db48", 82) else { return };
    for n in 0..CASES {
        let env = region(&mut o);
        let mut ops = setup(&mut o, &env);
        load(&mut o, &ops);
        o.call(0x23db48, &[]);
        ops.make_dir();
        compare(&o, &ops, &format!("case {n}"));
    }
}

#[test]
fn orig_23dbb0_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23dbb0", 83) else { return };
    for n in 0..CASES {
        let env = region(&mut o);
        let mut ops = setup(&mut o, &env);
        load(&mut o, &ops);
        o.call(0x23dbb0, &[]);
        ops.delete_name();
        compare(&o, &ops, &format!("case {n}"));
    }
}

#[test]
fn orig_23dbf0_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x23dbf0", 84) else { return };
    for n in 0..CASES {
        let env = region(&mut o);
        let mut ops = setup(&mut o, &env);
        load(&mut o, &ops);
        o.call(0x23dbf0, &[]);
        ops.create_empty();
        compare(&o, &ops, &format!("case {n}"));
    }
}

/// Reads and writes of a file: `read` picks `FUN_0023dc28` or `FUN_0023dc90`.
fn transfer(addr: u32, seed: u64, port: fn(&mut Ops, i64, u32, u32)) {
    let Some(mut o) = start(&format!("oracle test of {addr:#x}"), seed) else { return };
    for n in 0..CASES {
        let env = region(&mut o);
        let mut ops = setup(&mut o, &env);
        load(&mut o, &ops);
        let kind: i64 = [-1, 0, 1, 2, 3, 5][o.rng.below(6) as usize];
        let name = if o.rng.below(2) == 0 { 0 } else { OTHER };
        let buf = if o.rng.below(2) == 0 { 0 } else { OTHER + 0x1000 };
        o.call(addr, &[kind as u64, u64::from(name), u64::from(buf)]);
        port(&mut ops, kind, name, buf);
        compare(&o, &ops, &format!("case {n}: kind {kind} name {name:#x} buffer {buf:#x}"));
    }
}

#[test]
fn orig_23dc28_matches_the_game() {
    transfer(0x23dc28, 85, |ops, k, n, b| ops.read_file(k, n, b));
}

#[test]
fn orig_23dc90_matches_the_game() {
    transfer(0x23dc90, 86, |ops, k, n, b| ops.write_file(k, n, b));
}
