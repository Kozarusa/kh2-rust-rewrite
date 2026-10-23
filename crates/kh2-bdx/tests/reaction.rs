//! Reaction commands in the scripts on the user's disc: which scripts answer which command id with their
//! event 11 handler, and which offer commands themselves. Skips without a disc (KH2_ISO); the
//! disc-wide part also needs the file-name list (local/kh2idx.txt, `kh2 fetch-names`).

use std::collections::{BTreeMap, BTreeSet};

use kh2_bdx::{Call, Host, Program, Runtime, Value, analyze, reaction};

fn open_disc() -> Option<kh2_archive::Archive> {
    let path = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))?;
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    kh2_archive::Archive::open(&path).ok()
}

fn object_script(a: &mut kh2_archive::Archive, name: &str) -> Program {
    let data = a.read(&format!("obj/{name}.mdlx")).expect("object file");
    let bdx = kh2_bar::parse(&data).unwrap().into_iter().find(|e| e.kind == 3).expect("BDX entry").data.to_vec();
    Program::parse(&bdx).expect("script")
}

#[test]
fn soldier_answers_command_34_and_shadow_none() {
    let Some(mut a) = open_disc() else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return;
    };
    let soldier = object_script(&mut a, "M_EX010");
    assert_eq!(reaction::handled(&soldier), Some(BTreeSet::from([34])));
    assert!(reaction::offered(&analyze(&soldier).unwrap()).is_empty(), "no script offers command 34 to the Soldier");
    assert_eq!(reaction::handled(&object_script(&mut a, "M_EX020")), None);
    // The Soldier's variants answer the same command.
    for v in ["M_EX010_NM", "M_EX010_TR", "M_EX010_WI"] {
        if a.read(&format!("obj/{v}.mdlx")).is_ok() {
            assert_eq!(reaction::handled(&object_script(&mut a, v)), Some(BTreeSet::from([34])), "{v}");
        }
    }
}

/// Every script on the disc: the command ids handled and offered. The numbers are printed for
/// docs/systems/drive-limits-summons/contract.md; the asserts pin the ones the document quotes.
#[test]
fn disc_wide_reaction_map() {
    let Some(mut a) = open_disc() else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return;
    };
    let Ok(names) = std::fs::read_to_string("../../local/kh2idx.txt") else {
        eprintln!("skipped: no name list (kh2 fetch-names)");
        return;
    };
    a.apply_names(names.lines()).expect("names");
    let mut handlers: BTreeMap<i32, BTreeSet<String>> = BTreeMap::new();
    let mut offers: BTreeMap<i32, BTreeSet<String>> = BTreeMap::new();
    let (mut scripts, mut with_handler) = (0, 0);
    let list: Vec<_> = a.entries().to_vec();
    for l in &list {
        let Some(name) = &l.name else { continue };
        let Ok(data) = a.read_entry(&l.entry) else { continue };
        if !kh2_bar::is_bar(&data) {
            continue;
        }
        for e in kh2_bar::parse(&data).unwrap_or_default().iter().filter(|e| e.kind == 3 && !e.data.is_empty()) {
            let Ok(p) = Program::parse(e.data) else { continue };
            let Ok(an) = analyze(&p) else { continue };
            scripts += 1;
            let label = name.rsplit('/').next().unwrap_or(name).to_string();
            if let Some(h) = reaction::handled(&p) {
                with_handler += 1;
                for c in h {
                    handlers.entry(c).or_default().insert(label.clone());
                }
            }
            for c in reaction::offered(&an) {
                offers.entry(c).or_default().insert(label.clone());
            }
        }
    }
    eprintln!("scripts {scripts}, with an event 11 handler {with_handler}, distinct ids handled {}, offered by a script {}", handlers.len(), offers.len());
    for (c, who) in &handlers {
        eprintln!("HANDLED {c}: {}", who.iter().cloned().collect::<Vec<_>>().join(" "));
    }
    for (c, who) in &offers {
        eprintln!("OFFERED {c}: {}", who.iter().cloned().collect::<Vec<_>>().join(" "));
    }
    assert!(handlers[&34].contains("M_EX010.mdlx"));
    assert!(with_handler > 100);
}

// ---- running the Soldier's command 34 on a recording host ----

const SELF: u32 = 0x0100_0000;
const PLAYER: u32 = 0x0200_0000;
const RECORD: u32 = 0x0600_0000;

/// A host that records the calls the Soldier's reaction thread makes on the player.
#[derive(Default)]
struct Rec {
    calls: Vec<String>,
    /// Calls to `motion_done` on the player so far; the paired motion "ends" at the fourth.
    done_asked: u32,
}

impl Host for Rec {
    fn call(&mut self, c: &Call) -> Value {
        match c {
            Call::Player => Value::Handle(PLAYER),
            Call::Target { .. } => Value::Handle(0x0300_0000),
            Call::Position { obj } => Value::Vec(if *obj == SELF { [100.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] }),
            Call::FacingVector { .. } | Call::TargetPosition { .. } | Call::ObjVec860 { .. } | Call::ObjVec850 { .. } => Value::Vec([0.0, 0.0, 1.0, 0.0]),
            Call::MotionDone { obj } if *obj == PLAYER => {
                self.done_asked += 1;
                Value::Bool(self.done_asked > 3)
            }
            Call::SetFlag { obj, bit } => {
                self.calls.push(format!("set_flag {obj:#x} {bit}"));
                Value::None
            }
            Call::ClearFlag { obj, bit } => {
                self.calls.push(format!("clear_flag {obj:#x} {bit}"));
                Value::None
            }
            Call::SetVelocity { obj, dir, speed } => {
                self.calls.push(format!("set_velocity {obj:#x} {:.2} {:.2} {:.2} {speed}", dir[0], dir[1], dir[2]));
                Value::None
            }
            Call::Func { group: 1, index: 29, args } => {
                let (to, motion) = (args[1].int() as u32, args[2].int());
                self.calls.push(format!("play_motion_with {to:#x} {motion}"));
                Value::Int(-1)
            }
            Call::Func { group: 1, index: 238, .. } => Value::Int(-1),
            Call::Func { group: 1, index: 13, args } => {
                self.calls.push(format!("queue_motion {:#x} {}", args[0].int() as u32, args[1].int()));
                Value::None
            }
            Call::Func { .. } | Call::Unknown { .. } => Value::Int(0),
            Call::HitIsReal { .. } | Call::Flag124 { .. } | Call::Flag309 { .. } | Call::OnScreen { .. } | Call::Busy { .. } | Call::HitStun { .. } => Value::Bool(false),
            _ => Value::Int(0),
        }
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        // The target record {Soldier, part index 6, 0, -1} and the controller whose word at +4 is the player.
        let (base, words): (u32, [u32; 4]) = if (RECORD..RECORD + 0x20).contains(&addr) { (RECORD, [SELF, 6, 0, u32::MAX]) } else if (PLAYER..PLAYER + 16).contains(&addr) { (PLAYER, [0, PLAYER, 0, 0]) } else { return false };
        let b: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let o = (addr - base) as usize;
        match b.get(o..o + out.len()) {
            Some(s) => {
                out.copy_from_slice(s);
                true
            }
            None => false,
        }
    }
}

fn run_soldier(a: &mut kh2_archive::Archive, cmd: u32) -> Vec<String> {
    let mut rt = Runtime::new(object_script(a, "M_EX010"));
    let mut h = Rec::default();
    rt.spawn(&mut h, SELF);
    h.calls.clear();
    assert!(rt.event(&mut h, reaction::EVENT, &[PLAYER, cmd, RECORD]), "the Soldier has an event 11 handler");
    for _ in 0..12 {
        rt.update(&mut h);
    }
    assert!(rt.core.errors.is_empty(), "script errors: {:?}", rt.core.errors);
    assert_eq!(rt.request_threads(), 0, "the detached thread has ended");
    h.calls
}

/// Command 34: the thread started through host 2.20 makes the player untouchable, plays the paired motion
/// (entry 252), pushes him tangentially at 30 per tick while it runs, then clears the flag, plays entry 253
/// and queues idle. Any other command does nothing.
#[test]
fn soldier_command_34_runs_its_player_thread() {
    let Some(mut a) = open_disc() else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return;
    };
    let calls = run_soldier(&mut a, 34);
    eprintln!("{}", calls.join(" | "));
    assert_eq!(calls.first().map(String::as_str), Some("set_flag 0x2000000 3"));
    assert_eq!(calls[1], "play_motion_with 0x2000000 252");
    // The soldier is at +x, the player at the origin: a quarter turn of the line between them is along z.
    let vel: Vec<&String> = calls.iter().filter(|c| c.starts_with("set_velocity")).collect();
    assert!(vel.len() >= 2, "{vel:?}");
    for v in &vel {
        let f: Vec<f32> = v.split(' ').skip(2).map(|x| x.parse().unwrap()).collect();
        assert!(f[0].abs() < 1e-3 && (f[2].abs() - 1.0).abs() < 1e-3 && f[3] == 30.0, "{v}");
    }
    let n = calls.len();
    assert_eq!(&calls[n - 3..], ["clear_flag 0x2000000 3", "play_motion_with 0x2000000 253", "queue_motion 0x2000000 0"]);
    // Other commands fall through.
    assert!(run_soldier(&mut a, 457).is_empty());
    assert!(run_soldier(&mut a, 33).is_empty());
}
