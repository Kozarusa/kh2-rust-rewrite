//! The talk table (`libretto-<world>.bar`): what a townsperson says when Sora talks to it
//! (docs/events.md section 3, "Talk").
//!
//! Rules read in `SLUS_210.05`:
//!
//! * The file name is `libretto-%s.bar` with the world prefix (`FUN_001b6398`, the format at
//!   `0x35f1d4`); `FUN_001b6408` loads it when an area loads (called by `FUN_00168ba8`) and keeps
//!   the data of the file's first BAR entry of type 2 (`FUN_001684c0(file, 2, 0)`) at `0x1d5d6c4`.
//! * That entry: a u32 count at +4, then from +8 the records, 8 bytes each: `u16 talk id, u16 n,
//!   u32 offset` (from the entry start). `FUN_001b64b0` finds a record by binary search on the id
//!   (`FUN_0030ff40` with the compare `FUN_00332dc8`: `key - record.id`), so the records are sorted.
//!   `FUN_001b6510` keeps `n`; a reader with `n` = 0 does nothing (`FUN_001b6578`).
//! * The talk id of an object is the halfword +0x28 of its spawn record (`FUN_0016b870` copies it
//!   into the object's +0x500 when it is not 0).
//! * The program at the offset is a list of commands `u16 op, u16 count, count x u32`, ended by op
//!   0 (`FUN_001d8d18`). `FUN_001b65e0` runs them: 2 wait (the word as a float, `FUN_00102460`),
//!   3 fire event (`FUN_00197b70(word, 0)`), 4 stop (the program pauses after it: the next talk
//!   goes on from the command after it, `FUN_001b6578` keeps the place), 5..8 fades (`FUN_0014a7c8`
//!   / `FUN_0014a7f0`); every other op goes to the caller's own callback: for the talk command
//!   (`FUN_001a5a60`) op 1 opens a message window (class `FUN_001aba58`) with the message id of
//!   the first word and waits until it is closed.

/// BAR entry type of the talk table inside `libretto-<world>.bar`.
pub const ENTRY_KIND: u16 = 2;

/// One command of a talk program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub op: u16,
    pub args: Vec<u32>,
}

impl Command {
    /// What `FUN_001b65e0` (and the talk callback `FUN_001a5a60` for op 1) does with it.
    pub fn kind(&self) -> Op {
        let w = self.args.first().copied().unwrap_or(0);
        match self.op {
            1 => Op::Message(w),
            2 => Op::Wait(f32::from_bits(w)),
            3 => Op::Event(w),
            4 => Op::Stop,
            5..=8 => Op::Fade { op: self.op, time: f32::from_bits(w) },
            _ => Op::Other(self.op),
        }
    }
}

/// The meaning of a talk command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    /// Op 1: open a message window with this message id (in the world's message file) and wait
    /// until it is closed.
    Message(u32),
    /// Op 2: wait (`FUN_00102460`; the word is a float).
    Wait(f32),
    /// Op 3: fire `FUN_00197b70(id, 0)`.
    Event(u32),
    /// Op 4: stop here; the next talk resumes after this command.
    Stop,
    /// Ops 5..8: a fade (`FUN_0014a7c8` for 5 and 7, `FUN_0014a7f0` for 6 and 8; 7 and 8 pass 1).
    Fade { op: u16, time: f32 },
    /// Any other op: the caller's callback; the talk callback ignores it and goes on.
    Other(u16),
}

/// One command of a world map event program (WP-WM7): the executor `FUN_002c4c58` handles the ops 0x100..0x113 itself and
/// hands every other op to the talk reader's own switch (`FUN_001b65e0`: 2 wait, 5 to 8 fades, 1 and the rest the caller's).
/// The words are as the file has them (floats stay bit patterns until `Cut` turns them into numbers).
#[derive(Clone, Debug, PartialEq)]
pub enum Cut {
    /// Op 1: open the message window and wait until it has closed (`FUN_002c5db0`, `FUN_002c6010`).
    Message(u32),
    /// Op 2: wait this many frames (`FUN_00102460`).
    Wait(f32),
    /// Op 5: fade out over this many frames (`FUN_0014a7c8`).
    FadeOut(f32),
    /// Op 6: fade in over this many frames (`FUN_0014a7f0`).
    FadeIn(f32),
    /// Op 0x100: load the model of this objentry id with the event's bit (`FUN_0016e638`).
    LoadModel(u32),
    /// Op 0x101: load the event's file `libretto/<region>/wm<event>.bar` (`FUN_002c5598`).
    LoadFile,
    /// Op 0x102: make the event actor of objentry id `object` in slot `slot` (`FUN_002c5108`).
    Actor { slot: u32, object: u32 },
    /// Op 0x103: start the first of these entries of the loaded file's bar as the slot's animation, queue the rest
    /// (`FUN_002c5278`: `FUN_002f54d0` then `FUN_002f54e8` for each further entry).
    Play { slot: u32, entries: Vec<u32> },
    /// Op 0x111: queue these entries after the slot's animation (`FUN_002c5340`).
    Queue { slot: u32, entries: Vec<u32> },
    /// Op 0x104: a move task (`FUN_002c7198` -> `FUN_002c7000`) given a position, a second vector and a length in frames; the
    /// executor then waits that many frames (`FUN_00102460`).
    Fly { pos: [f32; 3], aim: [f32; 3], frames: f32 },
    /// Op 0x105: pause the event actors (`FUN_002c5a28`), wait (the float at 0x3832e0), resume them (`FUN_002c5a78`), play sound 0x3e88
    /// (`FUN_002c3318`) and wait again (the float at 0x3832e4).
    Pause,
    /// Op 0x106: finish the file load (`FUN_002c55e8`).
    Ready,
    /// Op 0x107: the message windows of this event are the second class (`+0x3c = 1`).
    Notice,
    /// Op 0x108: play the voice in this entry of the loaded file's bar (`FUN_002c5630`).
    Voice(u32),
    /// Op 0x109: the slot's actor stands at this position of the stage (`FUN_002c53c0`, object `+0x220`).
    Place { slot: u32, pos: [f32; 3] },
    /// Op 0x10a: the slot's actor turns by these angles in radians (`FUN_002c5400`, object `+0x210`).
    Turn { slot: u32, rot: [f32; 3] },
    /// Op 0x10b: the stage's position (`FUN_002c9160`, `0x1e353e0`).
    StagePos([f32; 3]),
    /// Op 0x10c: the stage's rotation in radians (`FUN_002c91d0`, `0x1e353f0`).
    StageRot([f32; 3]),
    /// Op 0x10d: remove the slot's actor (`FUN_002c5198`).
    Remove(u32),
    /// Op 0x10e: go to this world and area, entrance 0 (`FUN_00104e18`).
    Leave { world: u32, area: u32 },
    /// Op 0x10f: go to the destination of this story key in this world (`FUN_00198080`, `FUN_00104e18`).
    LeaveKey { key: u32, world: u32 },
    /// Op 0x110: `FUN_002c56f8`.
    Op110,
    /// Op 0x112: remove all four actors (`FUN_002c5230`).
    RemoveAll,
    /// Op 0x113: `FUN_002c5a78`.
    Resume,
    /// Any other op.
    Other(u16),
}

impl Command {
    fn f(&self, k: usize) -> f32 {
        f32::from_bits(self.args.get(k).copied().unwrap_or(0))
    }

    fn w(&self, k: usize) -> u32 {
        self.args.get(k).copied().unwrap_or(0)
    }

    fn v3(&self, k: usize) -> [f32; 3] {
        [self.f(k), self.f(k + 1), self.f(k + 2)]
    }

    /// The command as the world map event executor reads it (`FUN_002c4c58`).
    pub fn cut(&self) -> Cut {
        let rest = |from: usize| self.args.iter().skip(from).copied().collect::<Vec<_>>();
        match self.op {
            1 => Cut::Message(self.w(0)),
            2 => Cut::Wait(self.f(0)),
            5 => Cut::FadeOut(self.f(0)),
            6 => Cut::FadeIn(self.f(0)),
            0x100 => Cut::LoadModel(self.w(0)),
            0x101 => Cut::LoadFile,
            0x102 => Cut::Actor { slot: self.w(0), object: self.w(1) },
            0x103 => Cut::Play { slot: self.w(0), entries: rest(1) },
            0x104 => Cut::Fly { pos: self.v3(0), aim: self.v3(3), frames: self.f(6) },
            0x105 => Cut::Pause,
            0x106 => Cut::Ready,
            0x107 => Cut::Notice,
            0x108 => Cut::Voice(self.w(0)),
            0x109 => Cut::Place { slot: self.w(0), pos: self.v3(1) },
            0x10a => Cut::Turn { slot: self.w(0), rot: self.v3(1) },
            0x10b => Cut::StagePos(self.v3(0)),
            0x10c => Cut::StageRot(self.v3(0)),
            0x10d => Cut::Remove(self.w(0)),
            0x10e => Cut::Leave { world: self.w(0), area: self.w(1) },
            0x10f => Cut::LeaveKey { key: self.w(1), world: self.w(0) },
            0x110 => Cut::Op110,
            0x111 => Cut::Queue { slot: self.w(0), entries: rest(1) },
            0x112 => Cut::RemoveAll,
            0x113 => Cut::Resume,
            op => Cut::Other(op),
        }
    }
}

/// The talk table of one world.
#[derive(Clone, Debug, Default)]
pub struct Libretto {
    /// `(talk id, n, offset)` in file order.
    pub records: Vec<(u16, u16, u32)>,
    data: Vec<u8>,
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

impl Libretto {
    /// The table from the whole `libretto-<world>.bar` file (its first entry of type 2).
    pub fn from_bar(file: &[u8]) -> Option<Libretto> {
        let e = kh2_bar::parse(file).ok()?.into_iter().find(|e| e.kind == ENTRY_KIND)?;
        Libretto::parse(e.data)
    }

    /// The table from the entry's bytes.
    pub fn parse(d: &[u8]) -> Option<Libretto> {
        let n = u32_at(d, 4)? as usize;
        let mut records = Vec::with_capacity(n);
        for i in 0..n {
            let o = 8 + i * 8;
            records.push((u16_at(d, o)?, u16_at(d, o + 2)?, u32_at(d, o + 4)?));
        }
        Some(Libretto { records, data: d.to_vec() })
    }

    /// `FUN_001b64b0`: the record of a talk id (binary search like `FUN_0030ff40`).
    pub fn record(&self, id: u16) -> Option<(u16, u16, u32)> {
        let (mut lo, mut hi) = (0usize, self.records.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            let r = self.records[mid];
            match id.cmp(&r.0) {
                std::cmp::Ordering::Equal => return Some(r),
                std::cmp::Ordering::Less => hi = mid,
                std::cmp::Ordering::Greater => lo = mid + 1,
            }
        }
        None
    }

    /// The program of a talk id: its commands up to the ending op 0 (`FUN_001d8d18`). `None`
    /// when the id has no record or its `n` is 0 (`FUN_001b6578` then runs nothing).
    pub fn program(&self, id: u16) -> Option<Vec<Command>> {
        let (_, n, off) = self.record(id)?;
        if n == 0 {
            return None;
        }
        let mut o = off as usize;
        let mut out = Vec::new();
        loop {
            let op = u16_at(&self.data, o)?;
            if op == 0 {
                return Some(out);
            }
            let count = u16_at(&self.data, o + 2)? as usize;
            let args = (0..count).map(|k| u32_at(&self.data, o + 4 + 4 * k)).collect::<Option<Vec<_>>>()?;
            out.push(Command { op, args });
            o += 4 + 4 * count;
        }
    }
}

/// The place a talk reader keeps between talks (the 0x14-byte reader `FUN_001b6510` fills, kept
/// at the object's +0x504 while a program has paused at op 4).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reader {
    /// The index of the next command; `None` before the first talk or after the program ended
    /// (`FUN_001a53b8` frees the reader when `FUN_001b6578` reaches the end).
    pub next: Option<usize>,
}

impl Reader {
    /// The commands one talk runs (from the kept place, or the start, up to and including an op 4
    /// or the end), and where the next talk starts. A program that ends with op 4 keeps the place
    /// at its end: the next talk runs nothing and frees the reader (`FUN_001d8d18` answers 0 at
    /// once), the one after starts over.
    pub fn take<'a>(&mut self, program: &'a [Command]) -> &'a [Command] {
        let start = self.next.unwrap_or(0).min(program.len());
        let stop = program[start..].iter().position(|c| c.op == 4).map(|k| start + k + 1);
        self.next = stop;
        &program[start..stop.unwrap_or(program.len())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Vec<u8> {
        // Two records (ids 3 and 9); id 9 has n = 0.
        let mut d = vec![0u8; 8];
        d[4..8].copy_from_slice(&2u32.to_le_bytes());
        let prog_at = 8 + 16;
        for (id, n, off) in [(3u16, 1u16, prog_at as u32), (9, 0, prog_at as u32)] {
            d.extend_from_slice(&id.to_le_bytes());
            d.extend_from_slice(&n.to_le_bytes());
            d.extend_from_slice(&off.to_le_bytes());
        }
        for (op, args) in [(1u16, vec![0x1234u32]), (4, vec![]), (1, vec![0x1235]), (3, vec![0x4c]), (0, vec![])] {
            d.extend_from_slice(&op.to_le_bytes());
            d.extend_from_slice(&(args.len() as u16).to_le_bytes());
            for a in args {
                d.extend_from_slice(&a.to_le_bytes());
            }
        }
        d
    }

    fn command(op: u16, args: &[u32]) -> Command {
        Command { op, args: args.to_vec() }
    }

    #[test]
    fn world_map_event_ops_read_as_the_executor_reads_them() {
        let f = |v: f32| v.to_bits();
        assert_eq!(command(1, &[0x4bb6]).cut(), Cut::Message(0x4bb6));
        assert_eq!(command(5, &[f(12.0)]).cut(), Cut::FadeOut(12.0));
        assert_eq!(command(6, &[f(12.0)]).cut(), Cut::FadeIn(12.0));
        assert_eq!(command(0x102, &[2, 0x896]).cut(), Cut::Actor { slot: 2, object: 0x896 });
        assert_eq!(command(0x103, &[1, 5, 6]).cut(), Cut::Play { slot: 1, entries: vec![5, 6] });
        assert_eq!(command(0x111, &[2, 9]).cut(), Cut::Queue { slot: 2, entries: vec![9] });
        assert_eq!(command(0x109, &[1, f(-33.0), f(0.0), f(-20.0)]).cut(), Cut::Place { slot: 1, pos: [-33.0, 0.0, -20.0] });
        assert_eq!(command(0x10a, &[1, f(0.0), f(-0.5), f(0.0)]).cut(), Cut::Turn { slot: 1, rot: [0.0, -0.5, 0.0] });
        assert_eq!(command(0x10b, &[f(2.0), f(38.0), f(192.0)]).cut(), Cut::StagePos([2.0, 38.0, 192.0]));
        assert_eq!(command(0x10c, &[f(0.1), f(0.0), f(0.0)]).cut(), Cut::StageRot([0.1, 0.0, 0.0]));
        assert_eq!(command(0x10f, &[4, 0x5c]).cut(), Cut::LeaveKey { key: 0x5c, world: 4 });
        assert_eq!(command(0x112, &[]).cut(), Cut::RemoveAll);
        assert_eq!(command(0x2a, &[]).cut(), Cut::Other(0x2a));
        // A short list of words reads as zeros instead of failing.
        assert_eq!(command(0x10b, &[]).cut(), Cut::StagePos([0.0; 3]));
    }

    #[test]
    fn a_talk_program_reads_to_its_end_and_pauses_at_op_4() {
        let l = Libretto::parse(&table()).unwrap();
        assert_eq!(l.record(3), Some((3, 1, 24)));
        assert_eq!(l.record(4), None);
        assert_eq!(l.program(9), None, "n = 0 runs nothing");
        let p = l.program(3).unwrap();
        assert_eq!(p.iter().map(Command::kind).collect::<Vec<_>>(), vec![Op::Message(0x1234), Op::Stop, Op::Message(0x1235), Op::Event(0x4c)]);
        let mut r = Reader::default();
        assert_eq!(r.take(&p).len(), 2, "the first talk stops after op 4");
        assert_eq!(r.next, Some(2));
        assert_eq!(r.take(&p)[0].kind(), Op::Message(0x1235), "the second goes on after it");
        assert_eq!(r.next, None, "and the end frees the reader");
        assert_eq!(r.take(&p).len(), 2, "so the third starts over");
        // A program ending with op 4: the talk after it runs nothing, the next one starts over.
        let q = &p[..2];
        let mut r = Reader::default();
        assert_eq!(r.take(q).len(), 2);
        assert!(r.take(q).is_empty());
        assert_eq!(r.next, None);
        assert_eq!(r.take(q).len(), 2);
    }
}
