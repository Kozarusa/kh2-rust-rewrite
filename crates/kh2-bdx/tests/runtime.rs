//! The runtime's thread tree, message queue and remote mode queue on a small hand-assembled script (no
//! disc needed). Event 20 starts a child thread, 21 sends a message, 22 queues a mode for another object,
//! 23 / 24 start and stop the action thread, 25 starts a child that ends at once.

use kh2_bdx::host::{Arg, Call, ObjectKey};
use kh2_bdx::mock::{MockHost, PLAYER, SELF_OBJ};
use kh2_bdx::{Program, Runtime, analyze};

#[derive(Default)]
struct Asm {
    w: Vec<u16>,
}

impl Asm {
    fn here(&self) -> usize {
        self.w.len()
    }
    fn raw(&mut self, w: &[u16]) -> &mut Self {
        self.w.extend_from_slice(w);
        self
    }
    fn push_i(&mut self, v: i32) -> &mut Self {
        self.raw(&[0x0000, v as u16, (v >> 16) as u16])
    }
    fn push_f(&mut self, v: f32) -> &mut Self {
        let b = v.to_bits();
        self.raw(&[0x0010, b as u16, (b >> 16) as u16])
    }
    fn load_local(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0030, off])
    }
    fn store_local(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0001, off])
    }
    fn load_global(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0070, off])
    }
    fn store_global(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0041, off])
    }
    fn lea_global(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0060, off])
    }
    fn lea_code(&mut self, word_off: u16) -> &mut Self {
        self.raw(&[0x00e0, word_off])
    }
    fn ldf(&mut self, off: u16) -> &mut Self {
        self.raw(&[0x0003, off])
    }
    fn add(&mut self) -> &mut Self {
        self.raw(&[0x0006])
    }
    fn host(&mut self, g: u16, i: u16) -> &mut Self {
        self.raw(&[(g << 6) | 0xa, i])
    }
    fn yield_(&mut self) -> &mut Self {
        self.raw(&[0x0009])
    }
    fn ret(&mut self) -> &mut Self {
        self.raw(&[0x0089])
    }
    fn pop(&mut self) -> &mut Self {
        self.raw(&[0x00c9])
    }
    fn jmp(&mut self, target: usize) -> &mut Self {
        let rel = target as i32 - (self.here() as i32 + 2);
        self.raw(&[0x0007, rel as i16 as u16])
    }
    fn string(&mut self, s: &str) -> u16 {
        let at = self.here() as u16;
        let mut b = s.as_bytes().to_vec();
        b.push(0);
        if b.len() % 2 == 1 {
            b.push(0);
        }
        for c in b.chunks(2) {
            self.w.push(u16::from_le_bytes([c[0], c[1]]));
        }
        at
    }
    /// `global[off] += 1`
    fn bump(&mut self, off: u16) -> &mut Self {
        self.load_global(off).push_i(1).add().store_global(off)
    }
}

fn base(n: usize) -> usize {
    6 + 4 * n + 4
}

fn image(entries: &[(u32, u32)], code: &[u16], globals: u32) -> Vec<u8> {
    let mut b = Vec::new();
    let mut name = b"runtime_test".to_vec();
    name.resize(16, 0);
    b.extend(name);
    b.extend_from_slice(&globals.to_le_bytes());
    b.extend_from_slice(&0x200u32.to_le_bytes());
    b.extend_from_slice(&0x200u32.to_le_bytes());
    for (id, pc) in entries {
        b.extend_from_slice(&id.to_le_bytes());
        b.extend_from_slice(&pc.to_le_bytes());
    }
    b.extend_from_slice(&[0; 8]);
    while (b.len() - 0x10) / 2 < base(entries.len()) {
        b.push(0);
    }
    for w in code {
        b.extend_from_slice(&w.to_le_bytes());
    }
    b
}

// Globals: counters and the context structure the mode machine hands to routines.
const CHILD: u16 = 0x00; // frames the looping child has run
const CLEANUP: u16 = 0x04; // cleanup routine runs
const SENDER: u16 = 0x08; // object of the last message's sender
const MESSAGES: u16 = 0x0c; // message hook runs
const CTX: u16 = 0x10; // context structure: +4 holds the object
const SHORT: u16 = 0x18; // runs of the child that ends at once
const ACTION: u16 = 0x1c; // frames the action thread has run
const TABLE: u16 = 0x100; // mode records

const ENTRIES: usize = 12;

struct Script {
    prog: Program,
}

fn script() -> Script {
    let mut a = Asm { w: vec![0; base(ENTRIES)] };
    // Looping child: counts frames.
    let child = a.here();
    a.bump(CHILD).yield_().jmp(child);
    let cleanup = a.here();
    a.bump(CLEANUP).ret();
    let short = a.here();
    a.bump(SHORT).ret();
    // Action thread: receives the object, counts frames.
    let action = a.here();
    a.store_local(0);
    let action_loop = a.here();
    a.bump(ACTION).yield_().jmp(action_loop);
    // Mode routines: the main routine yields forever; the message hook counts and remembers the sender.
    let idle = a.here();
    a.yield_().jmp(idle);
    let message = a.here();
    a.store_local(4).store_local(0).bump(MESSAGES).load_local(4).ldf(4).store_global(SENDER).ret();
    let ping = a.string("ping");
    let stay = a.string("stay");
    let low = a.string("low");

    let mut entry_pc = Vec::new();
    // event 0: remember the object in the context, attach it with the mode table, register three modes
    entry_pc.push((0u32, a.here() as u32));
    a.store_local(0).load_local(0).store_global(CTX + 4);
    a.lea_global(CTX).lea_global(TABLE).host(1, 7);
    for (name, message_pc, priority) in [(ping, message, 100), (stay, 0, 100), (low, 0, 50)] {
        a.lea_global(TABLE).lea_code(name).push_i(priority).push_i(idle as i32);
        for slot in 1..9 {
            // exit, begin, end, damage, slot 5, message (slot 6), enter, slot 8
            a.push_i(if slot == 6 { message_pc as i32 } else { 0 });
        }
        a.host(1, 6);
    }
    a.ret();
    // event 1: yield forever
    entry_pc.push((1, a.here() as u32));
    let main = a.here();
    a.yield_().jmp(main);
    // event 20: start a child thread, tag 1, with the cleanup routine
    entry_pc.push((20, a.here() as u32));
    a.push_i(1).push_i(child as i32).push_i(cleanup as i32).push_i(0).host(0, 9).pop().ret();
    // event 21: message "ping" to everybody after 2.5 frames, sent by the context's object
    entry_pc.push((21, a.here() as u32));
    a.lea_global(CTX).lea_code(ping).push_f(2.5).host(1, 90).ret();
    // event 22: queue the mode "stay" for another object
    entry_pc.push((22, a.here() as u32));
    a.lea_global(CTX).lea_code(stay).host(6, 58).ret();
    // event 23 / 24: start and stop the action thread
    entry_pc.push((23, a.here() as u32));
    a.lea_global(CTX).push_i(action as i32).push_i(0).host(1, 112).ret();
    entry_pc.push((24, a.here() as u32));
    a.lea_global(CTX).host(1, 214).ret();
    // event 25: a child with tag 2 that ends at once
    entry_pc.push((25, a.here() as u32));
    a.push_i(2).push_i(short as i32).push_i(0).push_i(0).host(0, 9).pop().ret();
    // event 26: a named function with decoded arguments (1.72 test_flag) and an object lookup (1.114)
    entry_pc.push((26, a.here() as u32));
    a.lea_global(CTX).push_i(3).host(1, 72).pop().lea_global(CTX).push_i(0x236).host(1, 114).ret();
    // event 27 / 28 / 29: interrupt to "stay", switch to "ping" / "low" only when the priority allows
    entry_pc.push((27, a.here() as u32));
    a.lea_global(CTX).lea_code(stay).host(1, 81).ret();
    entry_pc.push((28, a.here() as u32));
    a.lea_global(CTX).lea_code(ping).host(1, 167).ret();
    entry_pc.push((29, a.here() as u32));
    a.lea_global(CTX).lea_code(low).host(1, 167).ret();
    assert_eq!(entry_pc.len(), ENTRIES);
    let img = image(&entry_pc, &a.w[base(ENTRIES)..], 0x1200);
    Script { prog: Program::parse(&img).expect("parse") }
}

fn word(rt: &Runtime, off: u16) -> u32 {
    u32::from_le_bytes(rt.shared.globals[off as usize..off as usize + 4].try_into().unwrap())
}

fn start() -> (Runtime, MockHost) {
    let s = script();
    analyze(&s.prog).expect("the script analyses");
    let mut rt = Runtime::new(s.prog);
    let mut host = MockHost::new();
    assert!(rt.spawn(&mut host, SELF_OBJ));
    assert_eq!(word(&rt, CTX + 4), SELF_OBJ);
    (rt, host)
}

/// A script that calls functions of group 9, whose table the executable does not fill in: the
/// signatures come from the script's own code, so the arguments reach the host intact and the stack stays balanced.
#[test]
fn functions_missing_from_the_executable_table_get_their_signature_from_the_script() {
    let mut a = Asm { w: vec![0; base(1)] };
    let main = a.here();
    // 9.1(5, 6) returns a value that is stored; 9.2(value) returns nothing; 9.3(1, 2, 3) returns nothing
    // and is followed by a statement that starts with a push; 9.4(9.1(7, 8)) nests a call with a result.
    a.push_i(5).push_i(6).host(9, 1).store_local(0);
    a.load_local(0).host(9, 2);
    a.push_i(1).push_i(2).push_i(3).host(9, 3);
    a.push_i(7).push_i(8).host(9, 1).host(9, 4);
    a.push_i(99).host(9, 2);
    let spin = a.here();
    a.yield_().jmp(spin);
    let img = image(&[(1, main as u32)], &a.w[base(1)..], 0x40);
    let prog = Program::parse(&img).expect("parse");
    let mut rt = Runtime::new(prog);
    let mut host = MockHost::new();
    assert!(rt.spawn(&mut host, SELF_OBJ));
    rt.update(&mut host);
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
    let calls: Vec<_> = host.calls().filter_map(|c| if let Call::Unknown { group, index, args } = c { Some((*group, *index, args.clone())) } else { None }).collect();
    assert_eq!(
        calls,
        vec![(9, 1, vec![5, 6]), (9, 2, vec![0]), (9, 3, vec![1, 2, 3]), (9, 1, vec![7, 8]), (9, 4, vec![0]), (9, 2, vec![99])]
    );
}

/// WP-F1 item 1: 1.141 `probe_ground_down(point, out, distance)` is answered by the runtime's own arm, because
/// `out` is the script's memory: the host's point goes through it with a return of 1, else the point itself
/// with 0 (FUN_00172538 -> FUN_001a5c30). The mock's floor is at the object's height, y 0.
#[test]
fn probe_ground_down_writes_the_found_point_through_the_script_pointer() {
    const POINT: u16 = 0x20;
    const OUT: u16 = 0x30;
    const OUT2: u16 = 0x40;
    const R1: u16 = 0x50;
    const R2: u16 = 0x54;
    let mut a = Asm { w: vec![0; base(1)] };
    let main = a.here();
    // Game space, Y down: a point 300 above the floor.
    for (k, v) in [10.0f32, -300.0, 20.0, 1.0].into_iter().enumerate() {
        a.push_f(v).store_global(POINT + 4 * k as u16);
    }
    // 500 down reaches the floor: 1, and the point on the floor is written to `out`.
    a.lea_global(POINT).lea_global(OUT).push_f(500.0).host(1, 141).store_global(R1);
    // 100 down does not: 0, and the point itself is copied.
    a.lea_global(POINT).lea_global(OUT2).push_f(100.0).host(1, 141).store_global(R2);
    let spin = a.here();
    a.yield_().jmp(spin);
    let img = image(&[(1, main as u32)], &a.w[base(1)..], 0x80);
    let prog = Program::parse(&img).expect("parse");
    let mut rt = Runtime::new(prog);
    let mut host = MockHost::new();
    assert!(rt.spawn(&mut host, SELF_OBJ));
    rt.update(&mut host);
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
    let f = |off: u16| f32::from_bits(word(&rt, off));
    let vec = |off: u16| [f(off), f(off + 4), f(off + 8), f(off + 12)];
    assert_eq!(word(&rt, R1), 1);
    assert_eq!(vec(OUT), [10.0, 0.0, 20.0, 1.0]);
    assert_eq!(word(&rt, R2), 0);
    assert_eq!(vec(OUT2), [10.0, -300.0, 20.0, 1.0]);
    let probes: Vec<_> = host.calls().filter_map(|c| if let Call::Func { group: 1, index: 141, args } = c { Some(args.clone()) } else { None }).collect();
    assert_eq!(probes.len(), 2);
    assert_eq!(probes[0][0], Arg::Vec([10.0, -300.0, 20.0, 1.0]));
    assert_eq!(probes[0][2], Arg::Float(500.0));
}

/// Host 10.51 (FUN_0011a6d8) is answered by the runtime's own arm: the host names the object found and the script's
/// record (the second argument) is written as object, 0, 0, -1; with nothing found it holds the emptied record
/// (FUN_0018ed40: object 0, -1 in word 3). The record starts out filled with 0x55 words to show it is written.
#[test]
fn screen_target_writes_the_found_object_into_the_scripts_record() {
    use kh2_bdx::host::Value;
    struct Finder {
        inner: MockHost,
        found: Option<u32>,
        asked: Vec<Vec<Arg>>,
    }
    impl kh2_bdx::Host for Finder {
        fn call(&mut self, c: &Call) -> Value {
            match c {
                Call::Func { group: 10, index: 51, args } => {
                    self.asked.push(args.clone());
                    self.found.map_or(Value::None, Value::Handle)
                }
                _ => self.inner.call(c),
            }
        }
    }
    const REC: u16 = 0x20;
    let mut a = Asm { w: vec![0; base(1)] };
    let main = a.here();
    for k in 0..5u16 {
        a.push_i(0x5555).store_global(REC + 4 * k);
    }
    // The object argument is a context structure whose +4 holds the object (as the mode machine passes).
    a.push_i(SELF_OBJ as i32).store_global(0x64);
    a.lea_global(0x60).lea_global(REC).push_f(120.0).push_f(60.0).host(10, 51);
    let spin = a.here();
    a.yield_().jmp(spin);
    let img = image(&[(1, main as u32)], &a.w[base(1)..], 0x80);
    let record = |found: Option<u32>| {
        let mut rt = Runtime::new(Program::parse(&img).expect("parse"));
        let mut host = Finder { inner: MockHost::new(), found, asked: Vec::new() };
        assert!(rt.spawn(&mut host, SELF_OBJ));
        rt.update(&mut host);
        assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
        let asked = &host.asked;
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0][2], Arg::Float(120.0));
        assert_eq!(asked[0][3], Arg::Float(60.0));
        [word(&rt, REC), word(&rt, REC + 4), word(&rt, REC + 8), word(&rt, REC + 12), word(&rt, REC + 16)]
    };
    assert_eq!(record(Some(0x1234)), [0x1234, 0, 0, u32::MAX, 0x5555]);
    assert_eq!(record(None), [0, 0, 0, u32::MAX, 0x5555]);
}

#[test]
fn named_functions_arrive_with_decoded_arguments_and_lookups_fill_the_context() {
    let (mut rt, mut host) = start();
    rt.event(&mut host, 26, &[]);
    let funcs: Vec<_> = host.calls().filter_map(|c| if let Call::Func { group, index, args } = c { Some((*group, *index, args.clone())) } else { None }).collect();
    assert_eq!(funcs, vec![(1, 72, vec![Arg::Obj(SELF_OBJ), Arg::Int(3)])]);
    assert!(host.calls().any(|c| matches!(c, Call::FindObject { by: ObjectKey::Entity, value: 0x236 })));
    // The answer went into the context the script passed.
    assert_eq!(word(&rt, CTX + 4), PLAYER);
}

#[test]
fn mode_machine_functions_interrupt_and_guard_by_priority() {
    let (mut rt, mut host) = start();
    rt.event(&mut host, 27, &[]);
    assert_eq!(rt.current_mode(), None, "the interrupted mode waits at the front of the queue");
    rt.update(&mut host);
    assert_eq!(rt.current_mode().as_deref(), Some("stay"));
    // A mode with a lower priority than the running one does not replace it...
    rt.event(&mut host, 29, &[]);
    assert_eq!(rt.current_mode().as_deref(), Some("stay"));
    // ...one with at least the same priority does, at once.
    rt.event(&mut host, 28, &[]);
    assert_eq!(rt.current_mode().as_deref(), Some("ping"));
}

#[test]
fn a_child_thread_runs_every_frame_after_its_parent() {
    let (mut rt, mut host) = start();
    rt.event(&mut host, 20, &[]);
    // The request waits for the next frame of the persistent thread, which then runs the new child too.
    assert_eq!(word(&rt, CHILD), 0);
    for frame in 1..=3 {
        rt.update(&mut host);
        assert_eq!(word(&rt, CHILD), frame);
    }
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
    assert_eq!(rt.live_threads(), 2, "the persistent thread and the child");
}

#[test]
fn starting_a_thread_with_the_same_tag_kills_the_old_one_and_runs_its_cleanup() {
    let (mut rt, mut host) = start();
    rt.event(&mut host, 20, &[]);
    rt.update(&mut host);
    assert_eq!((word(&rt, CHILD), word(&rt, CLEANUP)), (1, 0));
    rt.event(&mut host, 20, &[]);
    rt.update(&mut host);
    // The first child was destroyed (cleanup once); the second one ran its first frame.
    assert_eq!((word(&rt, CHILD), word(&rt, CLEANUP)), (2, 1));
    rt.update(&mut host);
    assert_eq!((word(&rt, CHILD), word(&rt, CLEANUP)), (3, 1));
    // A thread with a tag that shares no bit with the first one does not touch it.
    rt.event(&mut host, 25, &[]);
    rt.update(&mut host);
    assert_eq!(word(&rt, CHILD), 4);
    assert_eq!(word(&rt, SHORT), 1);
    // The short thread ended and was removed: only the persistent thread and the looping child are left.
    assert_eq!(rt.live_threads(), 2);
}

#[test]
fn messages_reach_the_mode_of_that_name_after_their_delay() {
    let (mut rt, mut host) = start();
    // The script's own send reaches the host as a request.
    rt.event(&mut host, 21, &[]);
    let sent: Vec<_> = host.calls().filter_map(|c| if let Call::SendMessage { sender, name, delay } = c { Some((*sender, name.clone(), *delay)) } else { None }).collect();
    assert_eq!(sent, vec![(SELF_OBJ, "ping".to_string(), 2.5)]);
    // Immediate delivery (delay 0) runs the hook with the sender's context.
    rt.receive_message(&mut host, "ping", 0x777, 0.0);
    assert_eq!((word(&rt, MESSAGES), word(&rt, SENDER)), (1, 0x777));
    // A message for a mode the object does not have is dropped.
    rt.receive_message(&mut host, "nothing", 0x1, 0.0);
    assert_eq!(word(&rt, MESSAGES), 1);
    // A delayed message waits in the single pending slot; a newer one replaces it.
    rt.receive_message(&mut host, "ping", 0x111, 3.0);
    rt.receive_message(&mut host, "ping", 0x222, 2.5);
    rt.update(&mut host);
    rt.update(&mut host);
    assert_eq!(word(&rt, MESSAGES), 1, "still counting down: 2.5, 1.5, 0.5");
    rt.update(&mut host);
    assert_eq!((word(&rt, MESSAGES), word(&rt, SENDER)), (2, 0x222));
    rt.update(&mut host);
    assert_eq!(word(&rt, MESSAGES), 2, "delivered once");
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
}

#[test]
fn message_delays_come_from_the_games_generator() {
    let mut seed = 1u32;
    assert_eq!(kh2_bdx::message_delay(&mut seed, 0.0), 0.0);
    assert_eq!(seed, 1, "no delay, no random number");
    let d = kh2_bdx::message_delay(&mut seed, 60.0);
    assert_eq!(seed, 0x10dcd);
    assert!((d - 0x10dcd as f32 * 2.328_306_4e-10 * 60.0).abs() < 1e-9);
    assert!((0.0..60.0).contains(&d));
}

#[test]
fn a_remote_queue_entry_goes_to_the_host_instead_of_starting_a_mode() {
    let (mut rt, mut host) = start();
    rt.event(&mut host, 22, &[]);
    rt.update(&mut host);
    assert_eq!(rt.current_mode(), None, "the mode was not started here");
    assert!(host.calls().any(|c| matches!(c, Call::RemoteSetMode { obj, name } if *obj == SELF_OBJ && name == "stay")));
    // The host passes it on; the other object's runtime switches to the mode.
    let (mut other, mut host2) = start();
    other.set_mode_named(&mut host2, "stay");
    assert_eq!(other.current_mode().as_deref(), Some("stay"));
}

/// The script part of the enemy clash handler (0x1b2478): the running mode's slot 8 hook with
/// (context, own volume, other volume), then event 0x13 with (own volume, other volume).
#[test]
fn a_clash_runs_the_mode_hook_then_event_19() {
    const SEQ: u16 = 0x00;
    const HOOK_AT: u16 = 0x04;
    const HOOK_OWN: u16 = 0x08;
    const HOOK_OTHER: u16 = 0x0c;
    const EV_AT: u16 = 0x14;
    const EV_OWN: u16 = 0x18;
    const EV_OTHER: u16 = 0x1c;
    const CTX: u16 = 0x20;
    const TABLE: u16 = 0x100;
    let mut a = Asm { w: vec![0; base(3)] };
    let idle = a.here();
    a.yield_().jmp(idle);
    let hook = a.here();
    a.store_local(8).store_local(4).store_local(0).bump(SEQ).load_global(SEQ).store_global(HOOK_AT);
    a.load_local(4).store_global(HOOK_OWN).load_local(8).store_global(HOOK_OTHER).ret();
    let fight = a.string("fight");
    let mut entries = Vec::new();
    entries.push((0u32, a.here() as u32));
    a.store_local(0).load_local(0).store_global(CTX + 4);
    a.lea_global(CTX).lea_global(TABLE).host(1, 7);
    a.lea_global(TABLE).lea_code(fight).push_i(100).push_i(idle as i32);
    for slot in 1..9 {
        a.push_i(if slot == 8 { hook as i32 } else { 0 });
    }
    a.host(1, 6).ret();
    entries.push((1, a.here() as u32));
    let main = a.here();
    a.yield_().jmp(main);
    entries.push((0x13, a.here() as u32));
    a.store_local(4).store_local(0).bump(SEQ).load_global(SEQ).store_global(EV_AT);
    a.load_local(0).store_global(EV_OWN).load_local(4).store_global(EV_OTHER).ret();
    let img = image(&entries, &a.w[base(3)..], 0x1200);
    let mut rt = Runtime::new(Program::parse(&img).expect("parse"));
    let mut host = MockHost::new();
    // Before the script thread exists nothing runs (0x1b2478 tests +0x484).
    assert!(!rt.clash(&mut host, 0x70, 0x71));
    assert!(rt.spawn(&mut host, SELF_OBJ));
    // Without a running mode only the event runs.
    assert!(rt.clash(&mut host, 0x70, 0x71));
    assert_eq!((word(&rt, SEQ), word(&rt, HOOK_AT), word(&rt, EV_AT)), (1, 0, 1));
    assert_eq!((word(&rt, EV_OWN), word(&rt, EV_OTHER)), (0x70, 0x71));
    rt.set_mode_named(&mut host, "fight");
    // A started mode whose record asks the host first (flag 4 of the machine) takes no hooks until
    // the machine has run it (FUN_00187040 tests +0x9c bit 4).
    rt.update(&mut host);
    assert!(rt.clash(&mut host, 0x80, 0x81));
    assert_eq!((word(&rt, HOOK_AT), word(&rt, EV_AT)), (2, 3), "the hook first, then the event");
    assert_eq!((word(&rt, HOOK_OWN), word(&rt, HOOK_OTHER)), (0x80, 0x81));
    assert_eq!((word(&rt, EV_OWN), word(&rt, EV_OTHER)), (0x80, 0x81));
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
}

#[test]
fn the_action_thread_runs_until_it_is_stopped() {
    let (mut rt, mut host) = start();
    rt.event(&mut host, 23, &[]);
    rt.update(&mut host);
    rt.update(&mut host);
    assert_eq!(word(&rt, ACTION), 2);
    rt.event(&mut host, 24, &[]);
    rt.update(&mut host);
    rt.update(&mut host);
    assert_eq!(word(&rt, ACTION), 2, "stopped");
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
}

/// The persistent spell thread of FUN_001a4798: event 20 runs as the thread's code until it is pointed
/// at event 21 (FUN_0016c500), which runs on the same thread and ends it.
#[test]
fn a_thread_can_be_restarted_at_another_event() {
    const LOOPS: u16 = 0x00;
    const AFTER: u16 = 0x04;
    let mut a = Asm { w: vec![0; base(2)] };
    let ev20 = a.here();
    a.store_local(0);
    let lp = a.here();
    a.bump(LOOPS).yield_().jmp(lp);
    let ev21 = a.here();
    a.bump(AFTER).ret();
    let img = image(&[(20, ev20 as u32), (21, ev21 as u32)], &a.w[base(2)..], 0x40);
    let mut rt = Runtime::new(Program::parse(&img).expect("parse"));
    let mut host = MockHost::new();
    assert!(rt.start_thread(SELF_OBJ, 20, &[7]));
    for _ in 0..3 {
        rt.update(&mut host);
    }
    assert_eq!((word(&rt, LOOPS), word(&rt, AFTER)), (3, 0));
    assert!(!rt.thread_done());
    assert!(rt.restart_thread(21, &[]));
    rt.update(&mut host);
    assert_eq!((word(&rt, LOOPS), word(&rt, AFTER)), (3, 1), "event 21 ran once, the loop of event 20 no more");
    assert!(rt.thread_done());
    assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
    // An event the program lacks ends the thread.
    assert!(!rt.restart_thread(22, &[]));
    assert!(rt.thread_done());
    let mut none = Runtime::new(Program::parse(&img).expect("parse"));
    assert!(!none.start_thread(SELF_OBJ, 5, &[]));
    assert!(none.thread_done());
}

mod restart_oracle {
    //! The restart against the game's own code: the four spell scripts' event 20 runs some slices in the
    //! game's interpreter (FUN_001d7cb0 in kh2-ee) and in this crate's, then the game's FUN_001d7bb0
    //! points its thread at event 21 (what FUN_0016c500 -> FUN_0016c4d8 does) and `Thread::restart`
    //! points this crate's; both run on. Program counter, operand stack depth, frame pointer and top of
    //! stack are compared after every instruction, the host calls with their arguments. Host functions
    //! answer by the same script on both sides (math functions run natively on the game's side).

    use std::cell::RefCell;
    use std::collections::HashMap;

    use kh2_bdx::exe::HostTable;
    use kh2_bdx::insn::Insn;
    use kh2_bdx::math::{self, VecAccess};
    use kh2_bdx::program::Program;
    use kh2_bdx::vm::{self, Env, Mem, Shared, Stop, Thread, TraceState, VmError};
    use kh2_ee::Ee;

    const PROG: u32 = 0x0140_0000;
    const THREAD: u32 = 0x0150_0000;
    const GLOB: u32 = 0x0151_0000;
    const FRAME: u32 = 0x0152_0000;
    const VALS: u32 = 0x0153_0000;
    const EXT: u32 = 0x0154_0000;
    const EXT_LEN: usize = 0x2_0000;
    const STACK: u32 = 0x01ff_0000;
    const CB_ADDR: u32 = 0x0000_3000;
    const GROUP_TABLE: u32 = 0x35bce8;
    const SEED_ADDR: u32 = 0x35b9a8;
    const IDENTITY: u32 = 0x354a60;
    const SEED: u32 = 0x1234_5679;
    /// FUN_001d7bb0: set a thread's program counter, push four zero frame words and the arguments.
    const START: u32 = 0x1d7bb0;
    /// FUN_001d7cb0: run a thread one slice.
    const RUN: u32 = 0x1d7cb0;

    #[derive(Debug, Clone, PartialEq)]
    enum Canon {
        Raw(u32),
        Code(u32),
        Global(u32),
        Frame(u32),
        Scratch,
    }

    type Step = (usize, usize, usize, Option<Canon>);

    #[derive(Default)]
    struct EeState {
        n: u32,
        by_addr: HashMap<u32, (u16, u16, usize, bool)>,
        steps: Vec<Step>,
        calls: Vec<(u16, u16, Vec<Canon>)>,
        code_len: u32,
    }

    thread_local! {
        static ST: RefCell<EeState> = RefCell::new(EeState::default());
    }

    fn canon(v: u32, code: u32, code_len: u32) -> Canon {
        if (code..code + code_len).contains(&v) {
            Canon::Code(v - code)
        } else if (GLOB..GLOB + 0x4000).contains(&v) {
            Canon::Global(v - GLOB)
        } else if (FRAME..FRAME + 0x1000).contains(&v) {
            Canon::Frame(v - FRAME)
        } else if (0x522100..0x522300).contains(&v) || (0x0522_2000..0x0522_2100).contains(&v) {
            Canon::Scratch
        } else {
            Canon::Raw(v)
        }
    }

    /// The scripted reply of a host function: vector results point at a buffer both sides can read, the
    /// heap gives blocks inside it, the rest cycles through a few numbers. 1.15 (`motion_state`) keeps
    /// its answer, so event 20's wait loop waits.
    fn policy(g: u16, i: u16, n: u32) -> u32 {
        if matches!((g, i), (1, 62) | (1, 64) | (1, 120) | (1, 147) | (1, 201) | (0, 53) | (2, 23) | (1, 83)) {
            return EXT + 16 * (n % 8);
        }
        if (g, i) == (0, 47) {
            return EXT + 0x1_0000 + 0x100 * (n % 64);
        }
        if (g, i) == (1, 15) {
            return 7;
        }
        [0, 0x3f80_0000, 3, 99, 0, 2][((n + i as u32) % 6) as usize]
    }

    fn ext_initial() -> Vec<u8> {
        let mut b = vec![0u8; EXT_LEN];
        for k in 0..8u32 {
            for (j, x) in [k as f32 * 10.0 + 1.0, 2.0, 3.0 + k as f32, 0.0].iter().enumerate() {
                let o = (16 * k + 4 * j as u32) as usize;
                b[o..o + 4].copy_from_slice(&x.to_le_bytes());
            }
        }
        b
    }

    fn host_hook(ee: &mut Ee) {
        let pc = ee.pc;
        let a0 = ee.gpr[kh2_ee::reg::A0] as u32;
        let (g, i, nargs, ret) = ST.with(|s| s.borrow().by_addr[&pc]);
        let args: Vec<u32> = (0..nargs as u32).map(|k| ee.r32(a0 + 4 * k)).collect();
        ST.with(|s| {
            let mut s = s.borrow_mut();
            let n = s.n;
            s.n += 1;
            let cl = s.code_len;
            let c: Vec<Canon> = args.iter().map(|&v| canon(v, PROG + 0x10, cl)).collect();
            s.calls.push((g, i, c));
            if ret {
                drop(s);
                ee.w32(a0, policy(g, i, n));
            }
        });
    }

    fn cb_hook(ee: &mut Ee) {
        let th = ee.gpr[kh2_ee::reg::A0] as u32;
        let pc = ee.r32(th + 4) as usize;
        let fp = ee.r32(th + 8);
        let vsp = ee.r32(th + 12);
        let depth = (vsp.wrapping_sub(VALS) / 4) as usize;
        let top = if depth > 0 && depth < 0x1000 { Some(ee.r32(vsp - 4)) } else { None };
        ST.with(|s| {
            let mut s = s.borrow_mut();
            let cl = s.code_len;
            let t = top.map(|v| canon(v, PROG + 0x10, cl));
            s.steps.push((pc, depth, fp.wrapping_sub(FRAME) as usize, t));
        });
    }

    struct Io<'a>(&'a mut Vec<u8>);

    impl VecAccess for Io<'_> {
        fn read_vec(&mut self, mem: &mut Mem<'_>, addr: u32) -> Result<[f32; 4], VmError> {
            match mem.read_vec4(addr) {
                Ok(v) => Ok(v),
                Err(vm::Outside::Ext(a)) => {
                    let o = a.wrapping_sub(EXT) as usize;
                    let b = self.0.get(o..o + 16).ok_or(VmError::BadAddress(a))?;
                    Ok(std::array::from_fn(|k| f32::from_le_bytes(b[4 * k..4 * k + 4].try_into().unwrap())))
                }
                Err(_) => Err(VmError::BadAddress(addr)),
            }
        }

        fn write_vec(&mut self, mem: &mut Mem<'_>, addr: u32, v: [f32; 4]) -> Result<(), VmError> {
            match mem.write_vec4(addr, v) {
                Ok(()) => Ok(()),
                Err(vm::Outside::Ext(a)) => {
                    let o = a.wrapping_sub(EXT) as usize;
                    let b = self.0.get_mut(o..o + 16).ok_or(VmError::BadAddress(a))?;
                    for (k, x) in v.iter().enumerate() {
                        b[4 * k..4 * k + 4].copy_from_slice(&x.to_le_bytes());
                    }
                    Ok(())
                }
                Err(_) => Err(VmError::BadAddress(addr)),
            }
        }
    }

    struct TestEnv {
        ext: Vec<u8>,
        table: HostTable,
        n: u32,
        code_len: u32,
        steps: Vec<Step>,
        calls: Vec<(u16, u16, Vec<Canon>)>,
    }

    impl Env for TestEnv {
        fn signature(&self, g: u16, i: u16) -> Option<(usize, bool)> {
            self.table.get(g, i).map(|s| (s.nargs as usize, s.returns))
        }

        fn host_call(&mut self, _p: &Program, mem: &mut Mem<'_>, g: u16, i: u16, a: &[u32]) -> Result<Option<u32>, VmError> {
            if g == 0 && math::is_math(i) {
                return math::call(mem, i, a, &mut Io(&mut self.ext));
            }
            let ret = self.table.get(g, i).unwrap().returns;
            let n = self.n;
            self.n += 1;
            let cl = self.code_len;
            self.calls.push((g, i, a.iter().map(|&v| canon(v, layout().code, cl)).collect()));
            Ok(ret.then(|| policy(g, i, n)))
        }

        fn ext_read(&mut self, addr: u32, out: &mut [u8]) -> bool {
            let o = addr.wrapping_sub(EXT) as usize;
            match self.ext.get(o..o + out.len()) {
                Some(b) => {
                    out.copy_from_slice(b);
                    true
                }
                None => false,
            }
        }

        fn ext_write(&mut self, addr: u32, data: &[u8]) -> bool {
            let o = addr.wrapping_sub(EXT) as usize;
            match self.ext.get_mut(o..o + data.len()) {
                Some(b) => {
                    b.copy_from_slice(data);
                    true
                }
                None => false,
            }
        }

        fn trace(&mut self, s: &TraceState, _i: &Insn) {
            let t = s.top.map(|v| canon(v, layout().code, self.code_len));
            self.steps.push((s.pc, s.depth, s.fp, t));
        }
    }

    fn layout() -> vm::Layout {
        vm::Layout { code: PROG + 0x10, globals: GLOB, frames: FRAME, frame_stride: 0x1_0000, scratch: 0x0522_2000 }
    }

    fn make_ee(elf: &[u8], table: &HostTable) -> Ee {
        let mut ee = Ee::new();
        ee.load_elf(elf).expect("elf");
        ee.limit = 300_000_000;
        for k in 0..16u32 {
            ee.wf32(IDENTITY + 4 * k, if k % 5 == 0 { 1.0 } else { 0.0 });
        }
        for g in 0..=11u16 {
            for i in 0..700u16 {
                if g == 0 && math::is_math(i) {
                    continue;
                }
                if let Some(sig) = table.get(g, i) {
                    ST.with(|s| s.borrow_mut().by_addr.insert(sig.address, (g, i, sig.nargs as usize, sig.returns)));
                    ee.hooks.insert(sig.address, host_hook);
                }
            }
        }
        ee.hooks.insert(CB_ADDR, cb_hook);
        ee
    }

    /// Runs the game's slice and this crate's, compares them; returns whether the thread yielded.
    fn slice(ee: &mut Ee, prog: &Program, sh: &mut Shared, th: &mut Thread, env: &mut TestEnv, label: &str) -> Result<bool, String> {
        ST.with(|s| {
            let mut s = s.borrow_mut();
            s.steps.clear();
            s.calls.clear();
        });
        env.steps.clear();
        env.calls.clear();
        let game = ee.call(RUN, &[THREAD as u64, GROUP_TABLE as u64, CB_ADDR as u64, 0], &[], STACK).map_err(|e| format!("{label}: game: {e:?}"))?;
        let mine = vm::run(prog, sh, th, env).map_err(|e| format!("{label}: this crate: {e:?} after the calls {:?}", env.calls))?;
        let (gsteps, gcalls) = ST.with(|s| {
            let s = s.borrow();
            (s.steps.clone(), s.calls.clone())
        });
        // The game's callback reports the state before each instruction from the second on.
        let n = env.steps.len();
        if gsteps.len() != n.saturating_sub(1) && gsteps.len() != n {
            return Err(format!("{label}: game ran {} steps, this crate {n}", gsteps.len()));
        }
        for k in 1..n {
            if gsteps.get(k - 1) != Some(&env.steps[k]) {
                return Err(format!("{label}: step {k} differs\n  game {:?}\n  this {:?}", gsteps.get(k - 1), env.steps[k]));
            }
        }
        let key = |c: &(u16, u16, Vec<Canon>)| (env.table.get(c.0, c.1).map(|s| s.address), c.2.clone());
        if gcalls.len() != env.calls.len() || gcalls.iter().zip(&env.calls).any(|(a, b)| key(a) != key(b)) {
            return Err(format!("{label}: host calls differ\n  game {gcalls:?}\n  this {:?}", env.calls));
        }
        let code = match mine {
            Stop::Yield => 1,
            Stop::Exit => 2,
            Stop::Done => 3,
        };
        if game as u32 != code {
            return Err(format!("{label}: game result {game}, this crate {code}"));
        }
        Ok(code == 1)
    }

    fn open_disc() -> Option<(kh2_archive::Archive, Vec<u8>)> {
        let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
        if !std::path::Path::new(&path).exists() {
            return None;
        }
        let mut a = kh2_archive::Archive::open(&path).ok()?;
        if let Ok(names) = std::fs::read_to_string("../../local/kh2idx.txt") {
            a.apply_names(names.lines()).ok()?;
        }
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
        Some((a, elf))
    }

    /// For each level-1 spell script and each number of event-20 slices before the cast trigger (1, 2,
    /// 5), the game's and this crate's threads agree instruction by instruction, the restart at event
    /// 21 included, and event 21 runs on to its end (at most 80 slices).
    #[test]
    fn spell_thread_restart_matches_the_games() {
        let Some((mut a, elf)) = open_disc() else {
            eprintln!("skipped: no disc image (set KH2_ISO)");
            return;
        };
        let table = HostTable::from_exe(&elf).expect("host table");
        let mut ee = make_ee(&elf, &table);
        let mut total = (0usize, 0usize);
        for name in ["magic/FIRE_1.mag", "magic/BLIZZARD_1.mag", "magic/THUNDER_1.mag", "magic/CURE_1.mag"] {
            let file = a.read(name).unwrap_or_else(|e| panic!("{name}: {e}"));
            let data = kh2_bar::parse(&file).expect("bar").into_iter().find(|e| e.kind == 3).expect("script").data.to_vec();
            let prog = Program::parse(&data).expect("parse");
            let code_len = (prog.words.len() * 2) as u32;
            let (pc20, pc21) = (prog.entry(20).expect("event 20"), prog.entry(21).expect("event 21"));
            for before in [1usize, 2, 5] {
                // The game's thread: event 20 with the four cast arguments (caster, effect file, kind,
                // magi row; pointers into the shared buffer, kind 0).
                let args = [EXT + 0x100, EXT + 0x200, 0, EXT + 0x300];
                ee.write(PROG, &data);
                ee.write(GLOB, &vec![0u8; 0x4000]);
                ee.write(FRAME, &vec![0u8; 0x1000]);
                ee.write(VALS, &vec![0u8; 0x1000]);
                ee.write(EXT, &ext_initial());
                ee.w32(SEED_ADDR, SEED);
                ee.w32(THREAD, PROG);
                ee.w32(THREAD + 8, FRAME);
                ee.w32(THREAD + 12, VALS);
                ee.w32(THREAD + 16, GLOB);
                for (k, v) in args.iter().enumerate() {
                    ee.w32(EXT + 0x400 + 4 * k as u32, *v);
                }
                ee.call(START, &[THREAD as u64, pc20 as u64, (EXT + 0x400) as u64, 4], &[], STACK).expect("start");
                ST.with(|s| {
                    let mut s = s.borrow_mut();
                    s.n = 0;
                    s.code_len = code_len;
                });
                let mut sh = Shared::new(&prog);
                sh.layout = layout();
                sh.rng = SEED;
                let mut th = Thread::new(&prog, &mut sh);
                th.restart(pc20 as usize, &args);
                let mut env = TestEnv { ext: ext_initial(), table: table.clone(), n: 0, code_len, steps: Vec::new(), calls: Vec::new() };
                let mut alive = true;
                for k in 0..before {
                    alive = slice(&mut ee, &prog, &mut sh, &mut th, &mut env, &format!("{name} event 20 slice {k}")).unwrap_or_else(|e| panic!("{e}"));
                    total.0 += env.steps.len();
                    total.1 += env.calls.len();
                    if !alive {
                        break;
                    }
                }
                assert!(alive, "{name}: event 20 waits for the motion");
                // The cast trigger: FUN_001a4a60 -> FUN_0016c500(thread, 21, 0, 0).
                ee.call(START, &[THREAD as u64, pc21 as u64, 0, 0], &[], STACK).expect("restart");
                th.restart(pc21 as usize, &[]);
                assert_eq!(ee.r32(THREAD + 4), pc21, "{name}: the game's thread points at event 21");
                let mut ended = false;
                for k in 0..80 {
                    let more = slice(&mut ee, &prog, &mut sh, &mut th, &mut env, &format!("{name} after {before} slices: event 21 slice {k}")).unwrap_or_else(|e| panic!("{e}"));
                    total.0 += env.steps.len();
                    total.1 += env.calls.len();
                    if !more {
                        ended = true;
                        break;
                    }
                }
                eprintln!("{name}: {before} slices of event 20, restart at 21: ended {ended}");
            }
        }
        eprintln!("restart oracle: {} interpreter steps and {} host calls compared, 0 differences", total.0, total.1);
        assert!(total.0 > 0);
    }
}
