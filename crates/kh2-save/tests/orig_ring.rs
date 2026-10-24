//! Oracle tests of the card driver's request ring: every enqueue function and its slot-0 wrapper, the idle handler that pops a request into
//! the running words (or starts the idle poll), the end of a request, the busy queries and the result getters. kh2-ee runs the original on
//! random driver memory (the words at `0x36c258` and the span `0x1dc0348..0x1dc0ce0`, with the indices in range) and the port runs on a copy;
//! both memories are compared byte for byte, and the returned ids too. The per-type handlers that call the IOP card library are not here
//! (spec section 4: hand tests with scripted results).
//!
//! Skips without a disc; fails without one when `KH2_REQUIRE_DISC=1`.

mod common;

use common::*;

/// Starts kh2-ee (`Ee`) on the user's executable with the unit's pointer globals set; `None` without a disc.
fn start(what: &str, seed: u64) -> Option<Orig> {
    let o = Orig::new(what, seed)?;
    let _: &kh2_ee::Ee = &o.ee;
    Some(o)
}

use kh2_save::ring::{CARD_BASE, CARD_LEN, CORE_BASE, CORE_LEN, Driver};

const CASES: u32 = 500;

fn random_driver(o: &mut Orig) -> Driver {
    let mut d = Driver { core: o.random(CORE_LEN).try_into().unwrap(), card: o.random(CARD_LEN) };
    d.set_word(0x36c258, o.rng.below(64) as u32);
    d.set_word(0x36c25c, o.rng.below(64) as u32);
    // Mostly small request kinds so every handler case is met.
    for i in 0..64u32 {
        let k = o.rng.below(17) as u32;
        d.set_word(CARD_BASE + i * 0x20, k);
    }
    // The idle poll: a port of 0 or 1, small countdowns and slot counts, the poll flag on half the time.
    d.set_word(0x1dc0b74, o.rng.below(2) as u32);
    d.set_word(0x1dc0b70, o.rng.below(3) as u32);
    d.set_word(0x1dc0b78, o.rng.below(4) as u32);
    d.set_word(0x1dc0cd0, 1 + o.rng.below(3) as u32);
    d.set_word(0x1dc0cd4, 1 + o.rng.below(3) as u32);
    d.card[(0x1dc0b68 - CARD_BASE) as usize] = o.rng.below(2) as u8;
    d
}

fn put(o: &mut Orig, d: &Driver) {
    o.put(CORE_BASE, &d.core);
    o.put(CARD_BASE, &d.card);
}

fn same(o: &Orig, d: &Driver, what: &str) {
    assert_eq!(o.get(CORE_BASE, CORE_LEN), d.core, "{what}: core words");
    let game = o.get(CARD_BASE, CARD_LEN);
    if let Some(i) = (0..CARD_LEN).find(|&i| game[i] != d.card[i]) {
        panic!("{what}: card span differs first at {:#x}: game {:#x} port {:#x}", CARD_BASE as usize + i, game[i], d.card[i]);
    }
}

type Enqueue = fn(&mut Driver, &[u32]) -> u32;

/// Runs one enqueue function with random arguments (zero a quarter of the time: the listing's null pointer).
fn enqueue_case(addr: u32, nargs: usize, port: Enqueue) {
    let Some(mut o) = start(&format!("oracle test of {addr:#x}"), u64::from(addr)) else { return };
    for n in 0..CASES {
        let mut d = random_driver(&mut o);
        put(&mut o, &d);
        let args: Vec<u32> = (0..nargs).map(|_| if o.rng.below(4) == 0 { 0 } else { o.rng.word() as u32 }).collect();
        let a: Vec<u64> = args.iter().map(|&v| v as i32 as i64 as u64).collect();
        let r = o.call(addr, &a) as u32;
        let id = port(&mut d, &args);
        assert_eq!(r, id, "case {n}: id");
        same(&o, &d, &format!("case {n} {args:x?}"));
    }
}

#[test]
fn orig_2310c8_matches_the_game() {
    enqueue_case(0x2310c8, 3, |d, a| d.enqueue_info(a[0], a[1], a[2]));
}

#[test]
fn orig_2315a0_matches_the_game() {
    enqueue_case(0x2315a0, 2, |d, a| d.enqueue_info_slot0(a[0], a[1]));
}

#[test]
fn orig_231120_matches_the_game() {
    enqueue_case(0x231120, 6, |d, a| d.enqueue_list(a[0], a[1], a[2], a[3], a[4], a[5]));
}

#[test]
fn orig_2315c0_matches_the_game() {
    enqueue_case(0x2315c0, 5, |d, a| d.enqueue_list_slot0(a[0], a[1], a[2], a[3], a[4]));
}

#[test]
fn orig_2311b8_matches_the_game() {
    enqueue_case(0x2311b8, 6, |d, a| d.enqueue_list_b([a[0], a[1], a[2], a[3], a[4], a[5]]));
}

#[test]
fn orig_2315f0_matches_the_game() {
    enqueue_case(0x2315f0, 5, |d, a| d.enqueue_list_b_slot0(a[0], a[1], a[2], a[3], a[4]));
}

#[test]
fn orig_231218_matches_the_game() {
    enqueue_case(0x231218, 3, |d, a| d.enqueue_format(a[0], a[1], a[2]));
}

#[test]
fn orig_231620_matches_the_game() {
    enqueue_case(0x231620, 2, |d, a| d.enqueue_format_slot0(a[0], a[1]));
}

#[test]
fn orig_231270_matches_the_game() {
    enqueue_case(0x231270, 3, |d, a| d.enqueue_unformat(a[0], a[1], a[2]));
}

#[test]
fn orig_231640_matches_the_game() {
    enqueue_case(0x231640, 2, |d, a| d.enqueue_unformat_slot0(a[0], a[1]));
}

#[test]
fn orig_2312c8_matches_the_game() {
    enqueue_case(0x2312c8, 4, |d, a| d.enqueue_chdir(a[0], a[1], a[2], a[3]));
}

#[test]
fn orig_231660_matches_the_game() {
    enqueue_case(0x231660, 3, |d, a| d.enqueue_chdir_slot0(a[0], a[1], a[2]));
}

#[test]
fn orig_231320_matches_the_game() {
    enqueue_case(0x231320, 4, |d, a| d.enqueue_mkdir(a[0], a[1], a[2], a[3]));
}

#[test]
fn orig_231680_matches_the_game() {
    enqueue_case(0x231680, 3, |d, a| d.enqueue_mkdir_slot0(a[0], a[1], a[2]));
}

#[test]
fn orig_231378_matches_the_game() {
    enqueue_case(0x231378, 6, |d, a| d.enqueue_create([a[0], a[1], a[2], a[3], a[4], a[5]]));
}

#[test]
fn orig_2316a0_matches_the_game() {
    enqueue_case(0x2316a0, 5, |d, a| d.enqueue_create_slot0(a[0], a[1], a[2], a[3], a[4]));
}

#[test]
fn orig_2313d8_matches_the_game() {
    enqueue_case(0x2313d8, 4, |d, a| d.enqueue_delete(a[0], a[1], a[2], a[3]));
}

#[test]
fn orig_2316d0_matches_the_game() {
    enqueue_case(0x2316d0, 3, |d, a| d.enqueue_delete_slot0(a[0], a[1], a[2]));
}

#[test]
fn orig_2314f0_matches_the_game() {
    enqueue_case(0x2314f0, 5, |d, a| d.enqueue_delete_b(a[0], a[1], a[2], a[3], a[4]));
}

#[test]
fn orig_231750_matches_the_game() {
    enqueue_case(0x231750, 4, |d, a| d.enqueue_delete_b_slot0(a[0], a[1], a[2], a[3]));
}

#[test]
fn orig_231430_matches_the_game() {
    enqueue_case(0x231430, 6, |d, a| d.enqueue_read([a[0], a[1], a[2], a[3], a[4], a[5]]));
}

#[test]
fn orig_2316f0_matches_the_game() {
    enqueue_case(0x2316f0, 5, |d, a| d.enqueue_read_slot0(a[0], a[1], a[2], a[3], a[4]));
}

#[test]
fn orig_231490_matches_the_game() {
    enqueue_case(0x231490, 6, |d, a| d.enqueue_write([a[0], a[1], a[2], a[3], a[4], a[5]]));
}

#[test]
fn orig_231720_matches_the_game() {
    enqueue_case(0x231720, 5, |d, a| d.enqueue_write_slot0(a[0], a[1], a[2], a[3], a[4]));
}

#[test]
fn orig_231550_matches_the_game() {
    enqueue_case(0x231550, 1, |d, a| d.enqueue_call(a[0]));
}

#[test]
fn orig_231a60_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x231a60", 0x231a60) else { return };
    let mut kinds = [0u32; 17];
    let mut polled = 0;
    for n in 0..2000 {
        let mut d = random_driver(&mut o);
        if n % 3 == 0 {
            // An empty ring, so the idle poll runs.
            let wr = d.word(0x36c258);
            d.set_word(0x36c25c, wr);
            d.set_word(0x1dc0b70, 0);
        }
        put(&mut o, &d);
        let rd = d.word(0x36c25c);
        if d.word(0x36c258) != rd {
            kinds[d.word(CARD_BASE + (rd & 0x3f) * 0x20).min(16) as usize] += 1;
        } else if d.card[(0x1dc0b68 - CARD_BASE) as usize] != 0 && d.word(0x1dc0b70) == 0 {
            polled += 1;
        }
        o.call(0x231a60, &[]);
        d.next();
        same(&o, &d, &format!("case {n}"));
    }
    assert!(kinds[1..14].iter().all(|&c| c > 5), "every request kind is popped: {kinds:?}");
    assert!(polled > 20, "{polled} idle polls");
}

#[test]
fn orig_231a18_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x231a18", 0x231a18) else { return };
    for n in 0..CASES {
        let mut d = random_driver(&mut o);
        // A completion function that is harmless to call (`FUN_0023e528` resets the CRC state), or none.
        let done = if n % 2 == 0 { 0 } else { 0x23e528 };
        d.set_word(0x1dc0b64, done);
        put(&mut o, &d);
        o.call(0x231a18, &[]);
        let cb = d.end();
        same(&o, &d, &format!("case {n}"));
        assert_eq!(cb.is_some(), done != 0);
        if let Some((f, id)) = cb {
            assert_eq!(f, done);
            assert_eq!(id, d.word(0x36c264));
        }
    }
}

/// A driver in the states the busy queries distinguish: an empty ring and an idle driver, an idle driver only, or random.
fn busy_driver(o: &mut Orig) -> Driver {
    let mut d = random_driver(o);
    if o.rng.below(3) == 0 {
        let wr = d.word(0x36c258);
        d.set_word(0x36c25c, wr);
        d.set_word(0x36c268, 0);
    }
    if o.rng.below(3) == 0 {
        d.set_word(0x36c268, 0);
    }
    d
}

#[test]
fn orig_230e90_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x230e90", 0x230e90) else { return };
    let (mut idle, mut busy) = (0, 0);
    for n in 0..2000 {
        let d = busy_driver(&mut o);
        put(&mut o, &d);
        let r = o.call(0x230e90, &[]) as u32 & 0xff != 0;
        assert_eq!(r, d.is_idle(), "case {n}");
        if r {
            idle += 1;
        } else {
            busy += 1;
        }
    }
    assert!(idle > 200 && busy > 200, "{idle} idle, {busy} busy");
}

#[test]
fn orig_230ea0_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x230ea0", 0x230ea0) else { return };
    let (mut busy, mut free) = (0, 0);
    for n in 0..2000 {
        let d = busy_driver(&mut o);
        put(&mut o, &d);
        let id = match o.rng.below(4) {
            0 => 0,
            1 => d.word(CARD_BASE + (o.rng.below(64) as u32) * 0x20 + 4),
            2 => d.word(0x36c264),
            _ => o.rng.word() as u32,
        };
        let r = o.call(0x230ea0, &[id as i32 as i64 as u64]) as u32 != 0;
        assert_eq!(r, d.is_busy(id), "case {n}: id {id:#x}");
        if r {
            busy += 1;
        } else {
            free += 1;
        }
    }
    assert!(busy > 200 && free > 200, "{busy} busy, {free} not");
}

/// A getter without arguments over random driver memory.
fn getter_case(addr: u32, port: fn(&Driver) -> u32) {
    let Some(mut o) = start(&format!("oracle test of {addr:#x}"), u64::from(addr)) else { return };
    for n in 0..CASES {
        let d = random_driver(&mut o);
        put(&mut o, &d);
        assert_eq!(o.call(addr, &[]) as u32, port(&d), "case {n}");
    }
}

/// A status table getter (port, slot) over random driver memory.
fn status_case(addr: u32, port: fn(&Driver, i32, i32) -> u32) {
    let Some(mut o) = start(&format!("oracle test of {addr:#x}"), u64::from(addr)) else { return };
    for n in 0..CASES {
        let d = random_driver(&mut o);
        put(&mut o, &d);
        let (p, s) = (o.rng.below(2) as i32, o.rng.below(4) as i32);
        assert_eq!(o.call(addr, &[p as u64, s as u64]) as u32, port(&d, p, s), "case {n}: port {p} slot {s}");
    }
}

#[test]
fn orig_231778_matches_the_game() {
    getter_case(0x231778, |d| d.changed());
}

#[test]
fn orig_231788_matches_the_game() {
    getter_case(0x231788, |d| d.result(2));
}

#[test]
fn orig_231798_matches_the_game() {
    getter_case(0x231798, |d| d.result(3));
}

#[test]
fn orig_2317a8_matches_the_game() {
    getter_case(0x2317a8, |d| d.result(4));
}

#[test]
fn orig_2317b8_matches_the_game() {
    getter_case(0x2317b8, |d| d.result(5));
}

#[test]
fn orig_2317c8_matches_the_game() {
    getter_case(0x2317c8, |d| d.result(6));
}

#[test]
fn orig_2317d8_matches_the_game() {
    getter_case(0x2317d8, |d| d.result(7));
}

#[test]
fn orig_2317e8_matches_the_game() {
    getter_case(0x2317e8, |d| d.result(8));
}

#[test]
fn orig_2317f8_matches_the_game() {
    getter_case(0x2317f8, |d| d.result(9));
}

#[test]
fn orig_231808_matches_the_game() {
    getter_case(0x231808, |d| d.result(10));
}

#[test]
fn orig_231818_matches_the_game() {
    getter_case(0x231818, |d| d.result(11));
}

#[test]
fn orig_231828_matches_the_game() {
    getter_case(0x231828, |d| d.result(12));
}

#[test]
fn orig_231878_matches_the_game() {
    status_case(0x231878, |d, p, s| d.card_type(p, s));
}

#[test]
fn orig_231898_matches_the_game() {
    status_case(0x231898, |d, p, s| d.status_4(p, s));
}

#[test]
fn orig_2318b8_matches_the_game() {
    status_case(0x2318b8, |d, p, s| d.status_8(p, s));
}

#[test]
fn orig_2318d8_matches_the_game() {
    status_case(0x2318d8, |d, p, s| d.connection(p, s));
}

#[test]
fn orig_2319f8_matches_the_game() {
    status_case(0x2319f8, |d, p, s| d.file_count(p, s));
}

#[test]
fn orig_2318f8_matches_the_game() {
    let Some(mut o) = start("oracle test of 0x2318f8", 0x2318f8) else { return };
    for n in 0..CASES {
        let mut d = random_driver(&mut o);
        put(&mut o, &d);
        let (p, s) = (o.rng.below(2) as i32, o.rng.below(4) as i32);
        o.call(0x2318f8, &[p as u64, s as u64]);
        d.force_no_card(p, s);
        same(&o, &d, &format!("case {n}"));
    }
}
