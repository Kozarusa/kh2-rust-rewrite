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
