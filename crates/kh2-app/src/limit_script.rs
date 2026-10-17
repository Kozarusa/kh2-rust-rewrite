//! The limit BAR's own script (type 3 entry of `limit/us/<file>`), run as a BDX thread. See docs/limits-reactions.md "Limit script".
//!
//! `FUN_001b2e08` looks the entry up (`FUN_001684c0(bar, 3, 0)`), starts a script thread on it (`FUN_0016c9b8`) and sends it event 22
//! with the limit task as the argument (`limit::start_events`); `FUN_001b28a0`'s first branch sends event 23 with a command id
//! (`limit::sub_command_event`). The game then runs the thread every frame with the other tasks (`FUN_0016c680`).
//!
//! The app runs that thread the way the other script owners do (`kh2_bdx::Runtime`): `start_thread(obj, 22, [task])` at the limit's
//! start, `restart_thread(23, [command])` for a sub-command, `update` once per 60 Hz tick while the app's countdown runs. The countdown
//! stays the end condition (the task's end, `FUN_001b3970`, waits for HUD animations, a gap). The host answers only what a call
//! needs to be answered and logs every call it does not port; a broken script (a VM error or a panic) is logged once and its thread
//! is dropped: the limit itself never depends on it.
//!
//! Logs: `limit script: start`, `limit script: first call host g.i`, `limit script: ended`, `limit script: fault`.

use std::collections::BTreeMap;

use bevy::prelude::*;
use kh2_bdx::{Call, Host, HostTable, Program, Runtime, Value};

use crate::assets::Disc;
use crate::limit::{LimitArg, LimitReceiver, LimitScriptEvent, LimitStarted, LimitTask};
use crate::schedule::Sim;

/// The stand-in for the limit task object that event 22 carries (word `+0xc` the player, `+0x10` the first participant). The same
/// value summon.rs uses for its `LIMIT_ATTACK` handle, so a script that reads the task through hosts 2.48 / 2.49 sees one object.
pub const TASK: u32 = 0x7a00_0000;

/// What one host function was asked, over the run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CallStat {
    pub count: u32,
    /// The tick of the first call.
    pub first_tick: u32,
}

/// Why a script thread stopped being run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    /// The thread ended by itself (it returned).
    Returned,
    /// A VM error ended it (`Runtime::errors`), or the interpreter panicked.
    Fault(String),
}

/// The limit task fields the script reads or changes through hosts, as the app keeps them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TaskState {
    /// The task's time gauge value (`+0xd4`, host 2.60): the gauge `FUN_001b2e08` fills with the command's `Time`
    /// (`FUN_001cfc28(Time, task + 0xa8)`) and `FUN_001b35e8` lowers every frame. The app's countdown is that gauge: its ticks left.
    pub remaining: f32,
    /// Count of started HUD animations (`+0x3c`, host 2.63): `FUN_001b3a70` adds one and sets bit 8 of `+0x40`; it runs for the
    /// limit whose command is 0x1e4 (Trinity) at the task's creation. The animation itself is not in the app.
    pub animations: i32,
    /// Bit 8 of `+0x40` (set with the count, cleared by host 2.89).
    pub animation_flag: bool,
}

impl TaskState {
    /// What the task creation leaves for the limit of `command` (`FUN_001b2e08`: `FUN_001b3a70(task, 3)` for command 0x1e4).
    pub fn new(command: u16, remaining: f32) -> TaskState {
        let trinity = command == 0x1e4;
        TaskState { remaining, animations: i32::from(trinity), animation_flag: trinity }
    }

    /// Host 2.89 (`FUN_001b3b60`): when bit 8 is set, stop the animation (the call through the player's `+0xb44` object is a gap
    /// [no HUD gauge in the app]), lower the count by one and clear the bit.
    fn release_animation(&mut self) {
        if self.animation_flag {
            self.animations -= 1;
            self.animation_flag = false;
        }
    }
}

/// The host a limit script talks to. It ports the calls whose behaviour is a game rule the app already has, answers the rest 0, and
/// records every call (`group.index`, count) so the log lists what a script uses.
struct LimitHost<'a> {
    calls: &'a mut BTreeMap<(u16, u16), CallStat>,
    task: &'a mut TaskState,
    tick: u32,
    /// Lines for the log: the first call of each host function, with its arguments.
    fresh: &'a mut Vec<String>,
}

impl Host for LimitHost<'_> {
    fn call(&mut self, c: &Call) -> Value {
        let id = crate::enemy_host::call_id(c);
        let tick = self.tick;
        let stat = self.calls.entry(id).or_insert_with(|| CallStat { count: 0, first_tick: tick });
        stat.count += 1;
        if stat.count == 1 {
            let name = kh2_bdx::names::host_name(id.0, id.1).unwrap_or("?");
            self.fresh.push(format!("limit script: first call host {}.{} {name} at tick {tick}: {c:?}", id.0, id.1));
        }
        host_answer(c, self.task)
    }
}

/// The first argument of a call as an integer (a handle), 0 without one.
fn first_arg(c: &Call) -> u32 {
    match c {
        Call::Func { args, .. } => args.first().map_or(0, |a| a.int() as u32),
        Call::Unknown { args, .. } => args.first().copied().unwrap_or(0),
        _ => 0,
    }
}

/// The answers of the ported calls (addresses in docs/limits-reactions.md "Limit script"); every other call is answered 0.
fn host_answer(c: &Call, task: &mut TaskState) -> Value {
    let on_task = first_arg(c) == TASK;
    match crate::enemy_host::call_id(c) {
        // Debug prints (empty in the retail build; the mission host treats them alike).
        (0, 0..=2) | (0, 8) | (0, 24..=26) => Value::None,
        // 2.48 (0x18e9c8): the word at +0xc of the object, the task's player (`FUN_001b2c10` stores its fourth argument there).
        (2, 48) if on_task => Value::Handle(crate::enemy_obj::PLAYER),
        // 2.60 (0x18eb40): the float at +0xd4, the time gauge value.
        (2, 60) if on_task => Value::Float(task.remaining),
        // 2.63 (0x18eb50): the word at +0x3c, the count of started HUD animations.
        (2, 63) if on_task => Value::Int(task.animations),
        // 2.89 (0x18eb60 -> FUN_001b3b60).
        (2, 89) if on_task => {
            task.release_animation();
            Value::None
        }
        // 1.94 (0x170150): the object is in the object list; the app's only object a limit script can name is the player.
        (1, 94) => Value::Bool(first_arg(c) == crate::enemy_obj::PLAYER),
        // 1.132 (0x173728): returns a target record to the pool; the app keeps no pool (host 1.131 answers no record).
        (1, 132) => Value::None,
        // 6.47 (0x2eefe0 -> FUN_0016bf78 -> FUN_0016bed8 -> FUN_001d59a8): removes a stop id (the script passes 17) from the three-slot
        // table at +0x518 of every object. The app holds no actor stop 17 (summon.rs holds 2), so there is nothing to remove.
        (6, 47) => Value::None,
        // 7.28 / 7.30 (0x2ef7b0 / 0x2ef820 -> FUN_002f3bd8 / FUN_002f3c18) start and end a screen effect object (`DAT_00371518`).
        // Not ported: it draws.
        (7, 28) | (7, 30) => Value::None,
        _ => Value::Int(0),
    }
}

/// A running limit script thread.
pub struct ScriptRun {
    rt: Runtime,
    pub task: TaskState,
    pub calls: BTreeMap<(u16, u16), CallStat>,
    pub ticks: u32,
    /// Lines the host collected for the log, drained by the system.
    fresh: Vec<String>,
    /// The tick count when the limit's countdown ended.
    ended_at: Option<u32>,
}

/// Ticks a script thread may keep running after the limit's countdown ended (its tail: release, a yield, the cleanup calls).
const TAIL_TICKS: u32 = 30;

impl ScriptRun {
    /// `FUN_001b2e08`: the thread on the type 3 entry, event 22 with the task. The caller logs a `false` (no event 22 in the script).
    pub fn start(prog: Program, table: Option<&HostTable>, task: TaskState) -> (ScriptRun, bool) {
        let mut rt = Runtime::new(prog);
        if let Some(t) = table {
            rt.set_host_table(t.clone());
        }
        let started = rt.start_thread(0, 22, &[TASK]);
        (ScriptRun { rt, task, calls: BTreeMap::new(), ticks: 0, fresh: Vec::new(), ended_at: None }, started)
    }

    /// `FUN_001b28a0` first branch: event 23 with the command id, to the running thread (`FUN_0016c620`). [inferred] The thread
    /// takes the event as `Runtime::restart_thread` does for the cast task's event 21: its code becomes the handler.
    pub fn sub_command(&mut self, command: u16) -> bool {
        self.rt.restart_thread(23, &[u32::from(command)])
    }

    /// One 60 Hz tick of the thread (`FUN_0016c680` from the task's frame). `Err` when it stopped.
    pub fn tick(&mut self, remaining: u32) -> Result<(), Stop> {
        self.ticks += 1;
        self.task.remaining = remaining as f32;
        let (rt, calls, fresh, task, tick) = (&mut self.rt, &mut self.calls, &mut self.fresh, &mut self.task, self.ticks);
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rt.update_script(&mut LimitHost { calls, task, tick, fresh })));
        if ran.is_err() {
            return Err(Stop::Fault("the interpreter panicked".into()));
        }
        if let Some((pc, e)) = self.rt.core.errors.first() {
            return Err(Stop::Fault(format!("{e:?} at word {pc:#x}")));
        }
        if self.rt.thread_done() {
            return Err(Stop::Returned);
        }
        Ok(())
    }

    /// The host calls as `group.index xN` words, in order.
    pub fn summary(&self) -> String {
        self.calls.iter().map(|((g, i), s)| format!("{g}.{i}x{}", s.count)).collect::<Vec<_>>().join(" ")
    }
}

/// The script of the limit BAR `limit/us/<file>`: its type 3 entry.
pub fn script_of(bar: &[u8]) -> Option<Program> {
    let es = kh2_bar::parse(bar).ok()?;
    let e = es.iter().find(|e| e.kind == crate::limit::KIND_SCRIPT)?;
    Program::parse(e.data).ok()
}

#[derive(Resource, Default)]
struct ScriptState {
    run: Option<ScriptRun>,
    table: Option<HostTable>,
    table_read: bool,
    /// The file of the limit that started last (`LimitStarted`).
    file: String,
    /// The command of the limit that started last.
    command: u16,
    /// Events 22 and 23 that came for the script, count (for the tests of the wiring).
    delivered: u32,
}

pub struct LimitScriptPlugin;

impl Plugin for LimitScriptPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<LimitStarted>()
            .add_message::<LimitScriptEvent>()
            .init_resource::<ScriptState>()
            .add_systems(FixedUpdate, run_limit_script.after(Sim::Sora));
    }
}

fn run_limit_script(
    disc: Option<Res<Disc>>,
    task: Option<Res<LimitTask>>,
    mut started: MessageReader<LimitStarted>,
    mut events: MessageReader<LimitScriptEvent>,
    mut st: ResMut<ScriptState>,
) {
    for s in started.read() {
        st.file = s.file.clone();
        st.command = s.command;
    }
    let running = task.as_deref().is_some_and(|t| t.0.is_some());
    for e in events.read().filter(|e| e.to == LimitReceiver::LimitScript) {
        match (e.event, e.arg) {
            (22, LimitArg::Task) => {
                let Some(disc) = disc.as_deref() else { continue };
                if !st.table_read {
                    st.table_read = true;
                    st.table = disc.read_root("SLUS_210.05").and_then(|x| HostTable::from_exe(&x));
                }
                let Some(prog) = disc.read(&format!("limit/us/{}", st.file)).and_then(|b| script_of(&b)) else {
                    info!("limit script: {} has no readable type 3 entry", st.file);
                    continue;
                };
                let left = task.as_deref().and_then(|t| t.0).map_or(0, |(_, l)| l);
                let (run, ok) = ScriptRun::start(prog, st.table.as_ref(), TaskState::new(st.command, left as f32));
                info!("limit script: start {} (event 22 with the task {TASK:#x}; the script has a handler for it: {ok})", st.file);
                st.delivered += 1;
                st.run = Some(run);
            }
            (23, LimitArg::Command(c)) => {
                if let Some(run) = st.run.as_mut() {
                    let ok = run.sub_command(c);
                    info!("limit script: event 23 with command {c} (handler found: {ok})");
                    st.delivered += 1;
                }
            }
            _ => {}
        }
    }
    let Some(run) = st.run.as_mut() else { return };
    // The app's countdown ends the limit (FUN_001b3970 waits for HUD animations: a gap). The gauge then reads 0, which is what ends the
    // script's own wait (host 2.60), so the thread gets the ticks its tail needs (at most `TAIL_TICKS`) and is dropped after them.
    let left = task.as_deref().and_then(|t| t.0).map_or(0, |(_, l)| l);
    let ended = !running;
    if ended && run.ended_at.is_none() {
        run.ended_at = Some(run.ticks);
    }
    let stop = match run.tick(left).err() {
        None if run.ended_at.is_some_and(|at| run.ticks - at >= TAIL_TICKS) => Some(Stop::Returned),
        other => other,
    };
    for l in run.fresh.drain(..) {
        info!("{l}");
    }
    let Some(stop) = stop else { return };
    let run = st.run.take().expect("checked above");
    match stop {
        Stop::Returned => info!("limit script: ended after {} ticks (the limit {}); host calls: {}", run.ticks, if ended { "is over" } else { "goes on" }, run.summary()),
        Stop::Fault(why) => warn!("limit script: fault after {} ticks ({why}); the thread is dropped, the limit goes on; host calls: {}", run.ticks, run.summary()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A program with the given entries (event id, code words), laid out like a BDX header: the 16 byte name, three sizes, the
    /// entry table ({id, word index} pairs, a zero offset ends it), then the code.
    fn program(entries: &[(u32, &[u16])]) -> Program {
        let table_words = 6 + 4 * entries.len() + 4;
        let mut words = vec![0u16; table_words];
        let (globals, frame, stack) = (0x40u32, 0x40u32, 0x40u32);
        words[0..2].copy_from_slice(&[globals as u16, 0]);
        words[2..4].copy_from_slice(&[frame as u16, 0]);
        words[4..6].copy_from_slice(&[stack as u16, 0]);
        let mut code: Vec<u16> = Vec::new();
        for (k, (id, body)) in entries.iter().enumerate() {
            let pc = (table_words + code.len()) as u32;
            let at = 6 + 4 * k;
            words[at..at + 2].copy_from_slice(&[*id as u16, 0]);
            words[at + 2..at + 4].copy_from_slice(&[pc as u16, (pc >> 16) as u16]);
            code.extend_from_slice(body);
        }
        words.extend(code);
        let mut bytes = b"limit_test\0\0\0\0\0\0".to_vec();
        bytes.extend(words.iter().flat_map(|w| w.to_le_bytes()));
        Program::parse(&bytes).expect("the test program parses")
    }

    const RET: u16 = 0x0089;

    /// Event 22 starts the thread with the task as its argument: the handler stores it, loads it and calls host 0.14 with it, which
    /// the host sees as the call's one argument.
    #[test]
    fn the_script_starts_once_and_gets_event_22_with_the_task() {
        let prog = program(&[(22, &[0x0001, 0x0000, 0x0030, 0x0000, 0x000a, 0x000e, RET])]);
        let (mut run, ok) = ScriptRun::start(prog, None, TaskState::default());
        assert!(ok);
        let _ = run.tick(100);
        assert_eq!(run.calls.get(&(0, 14)).map(|s| s.count), Some(1), "calls {:?}", run.calls);
        let first = run.fresh.first().expect("the first call is logged");
        assert!(first.contains("host 0.14") && first.contains(&format!("{TASK}")), "{first}");
        // A second tick does not start it again.
        let _ = run.tick(100);
        assert_eq!(run.calls.get(&(0, 14)).map(|s| s.count), Some(1));
    }

    /// A script without a handler for event 22 is reported as not started and its thread is over.
    #[test]
    fn a_script_without_event_22_is_over_at_once() {
        let prog = program(&[(23, &[RET])]);
        let (mut run, ok) = ScriptRun::start(prog, None, TaskState::default());
        assert!(!ok);
        assert_eq!(run.tick(100), Err(Stop::Returned));
    }

    /// A fault (a VM error: a word that is no instruction) is reported as a fault, not a crash or a hang, and a later tick of the
    /// same run does not panic.
    #[test]
    fn a_fault_stops_the_thread_and_nothing_else() {
        // A word that is no instruction (`BadInstruction`).
        let prog = program(&[(22, &[0xffff, RET])]);
        let (mut run, _) = ScriptRun::start(prog, None, TaskState::default());
        let r = run.tick(100);
        assert!(matches!(r, Err(Stop::Fault(_))), "{r:?}");
    }

    /// A script that loops without yielding is stopped by the interpreter's step limit: the tick comes back with a fault (no hang).
    #[test]
    fn a_script_that_never_yields_does_not_hang_the_tick() {
        // `jmp` to itself.
        let prog = program(&[(22, &[0x0007, 0xfffe])]);
        let (mut run, _) = ScriptRun::start(prog, None, TaskState::default());
        match run.tick(100) {
            Err(Stop::Fault(why)) => assert!(why.contains("StepLimit"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    /// A script that yields every frame is run one slice per tick for as long as it is asked to.
    #[test]
    fn a_yielding_script_runs_one_slice_per_tick() {
        // `yield; jmp -3`.
        let prog = program(&[(22, &[0x0009, 0x0007, 0xfffd])]);
        let (mut run, _) = ScriptRun::start(prog, None, TaskState::default());
        for k in 1..=3 {
            assert_eq!(run.tick(100), Ok(()), "tick {k}");
        }
        assert_eq!(run.ticks, 3);
    }

    fn func(group: u16, index: u16, a: &[u32]) -> Call {
        Call::Func { group, index, args: a.iter().map(|&v| kh2_bdx::Arg::Raw(v)).collect() }
    }

    /// The task's fields as hosts 2.48, 2.60, 2.63 and 2.89 answer them; Trinity (command 0x1e4) starts with one HUD animation that
    /// 2.89 releases once, other limits start with none, and a handle that is not the task reads 0.
    #[test]
    fn the_task_hosts_answer_from_the_task_state() {
        let mut t = TaskState::new(0x1e4, 180.0);
        assert_eq!(host_answer(&func(2, 48, &[TASK]), &mut t), Value::Handle(crate::enemy_obj::PLAYER));
        assert_eq!(host_answer(&func(2, 60, &[TASK]), &mut t), Value::Float(180.0));
        assert_eq!(host_answer(&func(2, 63, &[TASK]), &mut t), Value::Int(1));
        host_answer(&func(2, 89, &[TASK]), &mut t);
        assert_eq!(host_answer(&func(2, 63, &[TASK]), &mut t), Value::Int(0));
        host_answer(&func(2, 89, &[TASK]), &mut t);
        assert_eq!(host_answer(&func(2, 63, &[TASK]), &mut t), Value::Int(0), "a second release finds bit 8 clear");
        assert_eq!(host_answer(&func(2, 60, &[1234]), &mut t), Value::Int(0), "another handle is not the task");
        assert_eq!(TaskState::new(282, 1.0).animations, 0);
        assert_eq!(host_answer(&func(1, 94, &[crate::enemy_obj::PLAYER]), &mut t), Value::Bool(true));
        assert_eq!(host_answer(&func(1, 94, &[0]), &mut t), Value::Bool(false));
    }

    /// The gauge follows the countdown the system hands in: a script that waits on host 2.60 sees the ticks left.
    #[test]
    fn host_2_60_reads_the_countdown() {
        // Event 22: push the task handle, call host 2.60, return.
        let prog = program(&[(22, &[0x0000, 0x0000, 0x7a00, 0x008a, 0x003c, 0x0009, 0x0089])]);
        let (mut run, ok) = ScriptRun::start(prog, None, TaskState::default());
        assert!(ok);
        run.tick(77).ok();
        assert_eq!(run.task.remaining, 77.0);
        assert_eq!(run.calls.get(&(2, 60)).map(|s| s.count), Some(1), "{:?}", run.calls);
    }

    /// The system wiring: the countdown's end drops the thread after its tail; events for another receiver are not the script's.
    #[test]
    fn the_countdown_ends_the_thread() {
        let mut app = App::new();
        app.add_message::<LimitStarted>().add_message::<LimitScriptEvent>().init_resource::<ScriptState>();
        app.insert_resource(LimitTask(Some((1, 10))));
        app.add_systems(Update, run_limit_script);
        let prog = program(&[(22, &[0x0009, 0x0007, 0xfffd])]);
        let (run, _) = ScriptRun::start(prog, None, TaskState::default());
        app.world_mut().resource_mut::<ScriptState>().run = Some(run);
        app.update();
        assert!(app.world().resource::<ScriptState>().run.is_some(), "the thread runs while the limit does");
        app.world_mut().resource_mut::<LimitTask>().0 = None;
        app.update();
        assert!(app.world().resource::<ScriptState>().run.is_some(), "the thread keeps its tail ticks after the limit's end");
        for _ in 0..TAIL_TICKS {
            app.update();
        }
        assert!(app.world().resource::<ScriptState>().run.is_none(), "the limit's end drops the thread after the tail");
    }
}
