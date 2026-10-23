//! Oracle test of the area-script interpreter `kh2_ard::script::command` (orig 0x180b48, with its loop 0x180ad8 reached
//! through opcode 0x1c) against the original run in kh2-ee on the user's executable.
//!
//! Each case lays one random command (opcode drawn over the whole table and a little beyond, random counts and
//! arguments, valid nested commands for opcode 0x1c) in guest memory, randomises the script variables the commands
//! write (`0x35cd10..0x35cd44`, `0x35d268`, `0x1c893b8..0x1c893d4`, the random word `0x3716a0`, the area record byte
//! at `0x33e832`), runs the original and the port, and compares the answer, every variable byte and the list of calls
//! to the rest of the game. The callees the commands reach are replaced by recording hooks (the original does not run
//! them): `FUN_00181728`, `FUN_001684c0`, `FUN_001817f0`, `FUN_00180ff8`, `FUN_001a71c0`, `FUN_001a7330`,
//! `FUN_001a7700`, `FUN_001b9338`, `FUN_00311c08`, `FUN_0016fac8`, `FUN_001802e0`, `FUN_0017ff60`, `FUN_001abfe8`,
//! `FUN_001d7970`. `FUN_00325b38` (the 64-bit divide of opcodes 2 and 3) and `FUN_00180ad8` (opcode 0x1c) run for real.
//! The lottery behind opcode 0x1f has its own test below. Skips without a disc (fails with `KH2_REQUIRE_DISC=1`).

use kh2_ard::script::{command, Lottery, ScriptHost, ScriptVars};
use kh2_ee::Ee;
use kh2_factory::fuzz::Rng;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const STACK: u32 = 0x01ff_0000;
const CMD: u32 = 0x0181_0000;
const CMD_WORDS: usize = 4096;
const SCRATCH: u32 = 0x0183_0000;
const LANG: u32 = 0x349d40; // the language code the sprintf takes (not compared: the port leaves the formatting to the host)

type Log = Rc<RefCell<Vec<(u32, Vec<u32>)>>>;

/// The variable bytes the commands touch, in the order [`pack`] writes them.
const VARS: [(u32, u32); 5] = [(0x35cd10, 0x34), (0x35d268, 4), (0x1c893b8, 0x1c), (0x3716a0, 4), (0x33e830, 4)];

fn pack(v: &ScriptVars, area: [u8; 4]) -> Vec<Vec<u8>> {
    let mut a = vec![0u8; 0x34];
    a[0x00..0x08].copy_from_slice(&v.map_visibility.to_le_bytes());
    a[0x08..0x10].copy_from_slice(&v.word_35cd18.to_le_bytes());
    a[0x10..0x14].copy_from_slice(&v.heap_size.to_le_bytes());
    a[0x14..0x18].copy_from_slice(&v.heap_base.to_le_bytes());
    a[0x18..0x1c].copy_from_slice(&v.cache_a.to_le_bytes());
    a[0x1c..0x20].copy_from_slice(&v.cache_b.to_le_bytes());
    a[0x20..0x24].copy_from_slice(&v.word_35cd30.to_le_bytes());
    a[0x24..0x28].copy_from_slice(&v.word_35cd34.to_le_bytes());
    a[0x28..0x2c].copy_from_slice(&v.camera_type.to_le_bytes());
    a[0x2c..0x30].copy_from_slice(&v.effect_heap[0].to_le_bytes());
    a[0x30..0x34].copy_from_slice(&v.effect_heap[1].to_le_bytes());
    let mut c = vec![0u8; 0x1c];
    c[0x00..0x04].copy_from_slice(&v.capacity.to_le_bytes());
    c[0x08..0x10].copy_from_slice(&v.show.to_le_bytes());
    c[0x10..0x18].copy_from_slice(&v.hide.to_le_bytes());
    c[0x18..0x1c].copy_from_slice(&v.level.to_le_bytes());
    vec![a, v.flags_35d268.to_le_bytes().to_vec(), c, v.lcg.to_le_bytes().to_vec(), area.to_vec()]
}

fn unpack(raw: &[Vec<u8>]) -> (ScriptVars, [u8; 4]) {
    let w = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let q = |b: &[u8], o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let (a, c) = (&raw[0], &raw[2]);
    let v = ScriptVars {
        map_visibility: q(a, 0),
        word_35cd18: q(a, 8),
        heap_size: w(a, 0x10),
        heap_base: w(a, 0x14),
        cache_a: w(a, 0x18),
        cache_b: w(a, 0x1c),
        word_35cd30: w(a, 0x20),
        word_35cd34: w(a, 0x24),
        camera_type: w(a, 0x28),
        effect_heap: [w(a, 0x2c), w(a, 0x30)],
        flags_35d268: w(&raw[1], 0),
        capacity: f32::from_bits(w(c, 0)),
        show: q(c, 8),
        hide: q(c, 0x10),
        level: w(c, 0x18),
        lcg: w(&raw[3], 0),
    };
    (v, raw[4].clone().try_into().unwrap())
}

/// The port's host: logs every call the original's hooks log, in the same shape.
struct LogHost {
    log: Log,
    flags_set: bool,
    lottery: bool,
}

impl LogHost {
    fn push(&self, id: u32, v: Vec<u32>) {
        self.log.borrow_mut().push((id, v));
    }
}

impl ScriptHost for LogHost {
    fn load_group(&mut self, tag: u32, alt: bool) {
        self.push(1, vec![tag, alt as u32]);
    }
    fn entry_by_index(&mut self, index: u32) {
        // The original asks FUN_001684c0 (the hook answers an entry for odd indices whose bit 1 is set) and calls FUN_001817f0 then.
        if index & 3 == 3 {
            self.push(2, vec![index]);
        }
    }
    fn define_trigger(&mut self, arguments: &[u32]) {
        self.push(3, arguments.to_vec());
    }
    fn world_record(&mut self, argument: u32) {
        self.push(4, vec![0x1234, argument]);
    }
    fn music(&mut self, word: u32) {
        self.push(5, vec![word]);
    }
    fn battle_manager(&mut self, kind: u32, rest: &[u32]) {
        self.push(6, vec![kind, rest.first().copied().unwrap_or(0)]);
    }
    fn load_file(&mut self, format: &[u32]) {
        let mut v = vec![0x1c893d8];
        v.extend_from_slice(format);
        self.push(7, v);
        self.push(8, vec![0x1c893d8, 0x38e, 0]);
    }
    fn flag_is_set(&mut self, flag: u32) -> bool {
        self.push(9, vec![flag]);
        flag & 1 == 1
    }
    fn set_flag(&mut self, flag: u32) {
        self.flags_set = true;
        self.push(10, vec![flag]);
    }
    fn party_reset(&mut self, kind: u32) {
        self.push(11, vec![kind]);
    }
    fn lottery(&mut self, _vars: &mut ScriptVars) -> bool {
        self.push(12, vec![]);
        self.lottery
    }
}

fn hooks(ee: &mut Ee, log: &Log, lottery: &Rc<Cell<bool>>) {
    let l = log.clone();
    let push = move |id: u32, v: Vec<u32>| l.borrow_mut().push((id, v));
    let p = push.clone();
    ee.hooks.insert(0x181728, move |e: &mut Ee| p(1, vec![e.gpr[4] as u32, e.gpr[5] as u32 & 0xff]));
    let last_index = Rc::new(Cell::new(0u32));
    let li = last_index.clone();
    ee.hooks.insert(0x1684c0, move |e: &mut Ee| {
        let index = e.gpr[6] as u32;
        li.set(index);
        if index & 1 == 1 {
            e.w32(SCRATCH + 8, if index & 2 != 0 { 7 } else { 0 });
            e.gpr[2] = SCRATCH as u128;
        } else {
            e.gpr[2] = 0;
        }
    });
    let p = push.clone();
    ee.hooks.insert(0x1817f0, move |_e: &mut Ee| p(2, vec![last_index.get()]));
    let p = push.clone();
    ee.hooks.insert(0x180ff8, move |e: &mut Ee| {
        let at = e.gpr[5] as u32;
        let n = e.r16(at + 2) as i16 as i32;
        let v: Vec<u32> = (0..n.max(0) as u32).map(|k| e.r32(at + 4 + 4 * k)).collect();
        p(3, v);
    });
    ee.hooks.insert(0x1a71c0, |e: &mut Ee| e.gpr[2] = 0x1234);
    let p = push.clone();
    ee.hooks.insert(0x1a7330, move |e: &mut Ee| p(4, vec![e.gpr[4] as u32, e.gpr[5] as u32]));
    let p = push.clone();
    ee.hooks.insert(0x1a7700, move |e: &mut Ee| {
        let w = e.r32(e.gpr[4] as u32);
        p(5, vec![w]);
    });
    let p = push.clone();
    ee.hooks.insert(0x1b9338, move |e: &mut Ee| {
        let rest = e.gpr[5] as u32;
        let n = e.r16(rest - 8 + 2) as i16 as i32;
        let first = if n >= 2 { e.r32(rest) } else { 0 };
        p(6, vec![e.gpr[4] as u32, first]);
    });
    let p = push.clone();
    ee.hooks.insert(0x311c08, move |e: &mut Ee| {
        let at = e.gpr[5] as u32;
        let n = e.r16(at - 4 + 2) as i16 as i32;
        let mut v = vec![e.gpr[4] as u32];
        v.extend((0..n.max(0) as u32).map(|k| e.r32(at + 4 * k)));
        p(7, v);
    });
    let p = push.clone();
    ee.hooks.insert(0x16fac8, move |e: &mut Ee| p(8, vec![e.gpr[4] as u32, e.gpr[5] as u32, e.gpr[6] as u32]));
    let p = push.clone();
    ee.hooks.insert(0x1802e0, move |e: &mut Ee| {
        let f = e.gpr[4] as u32;
        p(9, vec![f]);
        e.gpr[2] = (f & 1) as u128;
    });
    let p = push.clone();
    ee.hooks.insert(0x17ff60, move |e: &mut Ee| p(10, vec![e.gpr[4] as u32]));
    let p = push.clone();
    ee.hooks.insert(0x1abfe8, move |e: &mut Ee| p(11, vec![e.gpr[4] as u32]));
    let lot = lottery.clone();
    ee.hooks.insert(0x1d7970, move |e: &mut Ee| {
        push(12, vec![]);
        e.gpr[2] = lot.get() as u128;
    });
}

fn arg(rng: &mut Rng) -> u32 {
    match rng.below(8) {
        0 | 1 => rng.below(40) as u32,
        2 => rng.below(0x100) as u32,
        3 => [0, 1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff, 63, 64, 65][rng.below(8) as usize],
        _ => rng.word() as u32,
    }
}

/// A command (opcode, count, arguments) with `n` arguments of random words.
fn plain(rng: &mut Rng, op: u16, n: i32) -> Vec<u32> {
    let mut w = vec![op as u32 | (n as u16 as u32) << 16];
    w.extend((0..n.max(0)).map(|_| arg(rng)));
    w
}

fn gen_command(rng: &mut Rng, entrance: u32) -> Vec<u32> {
    let op: u16 = match rng.below(20) {
        0 => 0x20 + rng.below(0x40) as u16,
        1 => rng.below(0x10000) as u16,
        _ => rng.below(0x20) as u16,
    };
    let n = match rng.below(12) {
        0 => 0,
        1 => 8,
        2 => -(1 + rng.below(3) as i32),
        _ => 1 + rng.below(5) as i32,
    };
    if op == 2 || op == 0x15 || op == 0xc || op == 0x16 {
        // These read through the count (opcode 2 divides by it; a trigger and a format need real arguments).
        let n = if op == 2 { n.max(1) } else { n.max(0) };
        return plain(rng, op, n);
    }
    if op == 0x1c {
        let mut body = Vec::new();
        for _ in 0..rng.below(4) {
            let inner = rng.below(0x20) as u16;
            if inner == 0x1c || inner == 2 {
                continue;
            }
            let k = match rng.below(4) {
                0 => 0,
                _ => 1 + rng.below(3) as i32,
            };
            body.extend(plain(rng, inner, k));
        }
        let e = if rng.below(2) == 0 { entrance } else { arg(rng) };
        let mut w = vec![0x1c | ((1 + body.len()) as u32) << 16, e];
        w.extend(body);
        return w;
    }
    plain(rng, op, n)
}

#[test]
fn orig_180b48_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x180b48") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let log: Log = Rc::new(RefCell::new(Vec::new()));
    let lottery = Rc::new(Cell::new(false));
    hooks(&mut ee, &log, &lottery);
    let mut rng = Rng::new(0x180b48);
    let mut per_op = [0u32; 0x21];
    let mut stops = 0;
    let (mut max_words, mut cases) = (0usize, 0u32);
    for n in 0..4000u32 {
        // Inputs.
        let entrance = rng.below(256) as u32;
        let mut cmd = gen_command(&mut rng, entrance);
        let used = cmd.len();
        while cmd.len() < CMD_WORDS {
            cmd.push(rng.word() as u32);
        }
        let mut vars_raw: Vec<Vec<u8>> = VARS.iter().map(|&(_, len)| (0..len).map(|_| rng.word() as u8).collect()).collect();
        // The random word must not be 0 (every roll would be 0 and the test would not see the picks) and the shifts
        // and floats should vary: leave them random. The area bytes: byte 2 is the entrance record.
        vars_raw[4][2] = entrance as u8;
        lottery.set(rng.below(2) == 1);
        let (vars0, area0) = unpack(&vars_raw);
        // The original.
        for (i, w) in cmd.iter().enumerate() {
            ee.w32(CMD + 4 * i as u32, *w);
        }
        for (i, &(addr, _)) in VARS.iter().enumerate() {
            ee.write(addr, &vars_raw[i]);
        }
        ee.w32(LANG, rng.word() as u32);
        log.borrow_mut().clear();
        let r = match ee.call(0x180b48, &[0, CMD as u64], &[], STACK) {
            Ok(r) => r,
            Err(f) => panic!("case {n}: the original faulted: {f:?}; command {:x?}", &cmd[..used]),
        };
        let game_log = log.borrow().clone();
        let game_vars: Vec<Vec<u8>> = VARS.iter().map(|&(a, len)| (0..len).map(|i| ee.r8(a + i)).collect()).collect();
        // The port.
        log.borrow_mut().clear();
        let mut host = LogHost { log: log.clone(), flags_set: false, lottery: lottery.get() };
        let mut v = vars0.clone();
        let ans = command(&mut host, &mut v, &cmd, area0[2]);
        let mut mine = pack(&v, area0);
        // Bytes the port does not model (the unused halves of the variable blocks): take the original's.
        for (b, (m, g)) in mine.iter_mut().zip(&game_vars).enumerate() {
            for (k, x) in m.iter_mut().enumerate() {
                if !modelled(b, k) {
                    *x = g[k];
                }
            }
        }
        let show = || format!("case {n}: command {:x?}", &cmd[..used]);
        assert_eq!((r & 1) as u32, ans as u32, "{}: answer", show());
        assert_eq!(game_log, *log.borrow(), "{}: calls out", show());
        for (b, (g, m)) in game_vars.iter().zip(&mine).enumerate() {
            if let Some(k) = (0..g.len()).find(|&k| g[k] != m[k]) {
                panic!("{}: variable block {:#x} byte +{k:#x}: game {:02x} port {:02x}", show(), VARS[b].0, g[k], m[k]);
            }
        }
        per_op[(cmd[0] as u16).min(0x20) as usize] += 1;
        stops += !ans as u32;
        max_words = max_words.max(used);
        cases += 1;
    }
    // Every opcode of the table was exercised, the stopping answer happened, and the nested form ran.
    assert!(per_op[..0x20].iter().all(|&c| c >= 20), "opcode coverage {per_op:?}");
    assert!(stops > 10, "stops {stops}");
    eprintln!("{cases} cases equal; per opcode {per_op:x?}; {stops} stops; longest command {max_words} words");
}

/// Whether byte `k` of variable block `b` belongs to a variable the port models.
fn modelled(b: usize, k: usize) -> bool {
    match b {
        0 => k < 0x34,
        2 => k < 4 || (0x08..0x1c).contains(&k),
        4 => k == 2,
        _ => true,
    }
}

/// `FUN_001d7970` (opcode 0x1f's lottery, with `FUN_001d7928`, `FUN_001d7a98`, `FUN_001d7908` -> `FUN_001d7848`) against `Lottery::roll`:
/// slots drawn from {0, 1, 2}, the five area pairs of the executable's table read at run time, the area bytes sometimes equal
/// to a pair, a random word.
#[test]
fn orig_1d7970_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x1d7970") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let table: [[u8; 2]; 5] = std::array::from_fn(|i| [ee.r8(0x37c5f8 + 2 * i as u32), ee.r8(0x37c5f8 + 2 * i as u32 + 1)]);
    let mut rng = Rng::new(0x1d7970);
    let mut wins = 0;
    for n in 0..3000 {
        let mut lot = Lottery { slots: std::array::from_fn(|_| rng.below(3) as u8) };
        if rng.below(4) == 0 {
            // Four used and one open or winner, so the restart path runs.
            lot.slots = [1; 5];
            lot.slots[rng.below(5) as usize] = rng.below(3) as u8;
        }
        let area = if rng.below(4) == 0 { [rng.word() as u8, rng.word() as u8] } else { table[rng.below(5) as usize] };
        let seed = rng.word() as u32;
        ee.write(0x1d6a9c0, &lot.slots);
        ee.write(0x33e830, &area);
        ee.w32(0x3716a0, seed);
        let r = ee.call(0x1d7970, &[0x33e830], &[], STACK).unwrap_or_else(|f| panic!("case {n}: {f:?}"));
        let game_slots: [u8; 5] = std::array::from_fn(|i| ee.r8(0x1d6a9c0 + i as u32));
        let game_lcg = ee.r32(0x3716a0);
        let mut lcg = seed;
        let before = lot;
        let won = lot.roll(&table, area, &mut lcg);
        assert_eq!(r & 0xff, won as u64, "case {n}: slots {:?} area {area:?} seed {seed:#x}: answer", before.slots);
        assert_eq!(game_slots, lot.slots, "case {n}: slots {:?} area {area:?} seed {seed:#x}", before.slots);
        assert_eq!(game_lcg, lcg, "case {n}: random word, slots {:?} area {area:?} seed {seed:#x}", before.slots);
        wins += won as u32;    }
    assert!(wins > 100, "wins {wins}");
    eprintln!("3000 cases equal; {wins} wins");
}

