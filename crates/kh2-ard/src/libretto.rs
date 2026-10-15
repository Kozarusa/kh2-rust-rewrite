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
