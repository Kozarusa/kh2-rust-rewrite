//! Static scan of one function of the original executable: what it calls, where its stores and loads go (classified by
//! where the base pointer comes from: the stack, an argument, a pointer read out of an argument's block, a global, or
//! unknown), which argument registers it reads, the fields it touches through its pointer arguments (the layout the
//! oracle test lays out), and its use of VU0, hardware registers and instructions kh2-ee cannot run.
//!
//! The analysis is a forward dataflow over the function's instructions with a branch's delay slot folded into the
//! branch, joined at merge points, with no path sensitivity. It is an estimate for classification and for generating
//! test skeletons: the oracle test itself is the proof.

use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const OFF_UNK: i32 = i32::MIN;

/// Where a register's value comes from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum V {
    Unk,
    Const(u32),
    /// The stack pointer plus an offset.
    Sp(i32),
    /// Pointer argument `a` (0..7 = a0..a3, t0..t3) plus an offset.
    Arg { a: u8, off: i32 },
    /// The word at field `f` of argument `a`'s block (a pointer to a sub-block) plus an offset.
    Deref { a: u8, f: i32, off: i32 },
    /// Derived from argument memory deeper than one level, or from several arguments.
    ArgMem,
    /// A scalar (shifted, multiplied, compared): not a pointer.
    Data,
    /// A pointer into global memory built from a constant base (0 when several bases merge).
    GlobPtr(u32),
    /// A value loaded from global memory (a pointer into a global object, or a scalar).
    GlobVal,
}

use V::*;

pub fn addr_like(c: u32) -> bool {
    (0x0010_0000..0x0200_0000).contains(&c)
        || (0x2010_0000..0x2200_0000).contains(&c)
        || (0x3010_0000..0x3200_0000).contains(&c)
        || (0x7000_0000..0x7000_4000).contains(&c)
}

pub fn hw_like(c: u32) -> bool {
    (0x1000_0000..0x1400_0000).contains(&c) || (0xb000_0000..0xb400_0000).contains(&c)
}

fn offadd(o: i32, c: i32) -> i32 {
    if o == OFF_UNK { o } else { o.wrapping_add(c) }
}

fn join(x: V, y: V) -> V {
    if x == y {
        return x;
    }
    match (x, y) {
        (Const(0), o) | (o, Const(0)) => o,
        (Const(a), Const(b)) => {
            if addr_like(a) && addr_like(b) {
                GlobPtr(0)
            } else {
                Data
            }
        }
        (Const(a), GlobPtr(_)) | (GlobPtr(_), Const(a)) if addr_like(a) => GlobPtr(0),
        (GlobPtr(_), GlobPtr(_)) => GlobPtr(0),
        (Sp(_), Sp(_)) => Sp(OFF_UNK),
        (Arg { a, .. }, Arg { a: b, .. }) if a == b => Arg { a, off: OFF_UNK },
        (Deref { a, f, .. }, Deref { a: b, f: g, .. }) if a == b && f == g => Deref { a, f, off: OFF_UNK },
        (Arg { .. } | Deref { .. } | ArgMem, Arg { .. } | Deref { .. } | ArgMem) => ArgMem,
        (Data, Const(_)) | (Const(_), Data) => Data,
        (GlobVal, GlobPtr(_)) | (GlobPtr(_), GlobVal) => GlobVal,
        _ => Unk,
    }
}

fn add(x: V, y: V) -> V {
    match (x, y) {
        (o, Const(0)) | (Const(0), o) => o,
        (Const(a), Const(b)) => Const(a.wrapping_add(b)),
        (Sp(o), Const(c)) | (Const(c), Sp(o)) => Sp(offadd(o, c as i32)),
        (Sp(_), _) | (_, Sp(_)) => Sp(OFF_UNK),
        (Const(c), _) | (_, Const(c)) if addr_like(c) => GlobPtr(c),
        (Arg { a, off }, Const(c)) | (Const(c), Arg { a, off }) => Arg { a, off: offadd(off, c as i32) },
        (Deref { a, f, off }, Const(c)) | (Const(c), Deref { a, f, off }) => Deref { a, f, off: offadd(off, c as i32) },
        (GlobPtr(b), _) | (_, GlobPtr(b)) => GlobPtr(b),
        (Arg { a, .. }, Data | GlobVal) | (Data | GlobVal, Arg { a, .. }) => Arg { a, off: OFF_UNK },
        (Deref { a, f, .. }, Data | GlobVal) | (Data | GlobVal, Deref { a, f, .. }) => Deref { a, f, off: OFF_UNK },
        (Arg { .. } | Deref { .. } | ArgMem, Arg { .. } | Deref { .. } | ArgMem) => ArgMem,
        (ArgMem, Data | GlobVal) | (Data | GlobVal, ArgMem) => ArgMem,
        (GlobVal, Data | Const(_)) | (Data | Const(_), GlobVal) => GlobVal,
        (Data, Data) | (Data, Const(_)) | (Const(_), Data) => Data,
        _ => Unk,
    }
}

fn scalar(x: V) -> V {
    match x {
        Sp(_) => Sp(OFF_UNK),
        Unk => Unk,
        _ => Data,
    }
}

#[derive(Clone, PartialEq, Debug)]
struct State {
    r: [V; 32],
    hl: V,
    slots: Vec<(i32, V)>,
    /// GPRs written so far on every path (argument registers start unwritten).
    gw: u32,
    /// FPRs written so far on every path (f12..f19 start unwritten).
    fw: u32,
    /// v0, v1, f0 written (bits 0, 1, 2) on every path.
    ret: u8,
}

impl State {
    fn entry() -> State {
        let mut r = [Unk; 32];
        r[0] = Const(0);
        r[28] = GlobPtr(0);
        r[29] = Sp(0);
        for i in 0..8 {
            r[4 + i] = Arg { a: i as u8, off: 0 };
        }
        State { r, hl: Unk, slots: Vec::new(), gw: !0x0000_0ff0, fw: !0x000f_f000, ret: 0 }
    }

    fn join(&self, o: &State) -> State {
        let r = std::array::from_fn(|i| join(self.r[i], o.r[i]));
        let slots = self.slots.iter().filter_map(|&(k, v)| o.slots.iter().find(|s| s.0 == k).map(|s| (k, join(v, s.1)))).collect();
        State { r, hl: join(self.hl, o.hl), slots, gw: self.gw & o.gw, fw: self.fw & o.fw, ret: self.ret & o.ret }
    }

    fn set(&mut self, r: usize, v: V) {
        if r != 0 {
            self.r[r] = v;
            self.gw |= 1 << r;
            match r {
                2 => self.ret |= 1,
                3 => self.ret |= 2,
                _ => {}
            }
        }
    }

    fn fset(&mut self, f: usize) {
        self.fw |= 1 << f;
        if f == 0 {
            self.ret |= 4;
        }
    }

    fn slot(&self, o: i32) -> V {
        self.slots.iter().find(|s| s.0 == o).map(|s| s.1).unwrap_or(Unk)
    }

    fn put_slot(&mut self, o: i32, v: V) {
        self.slots.retain(|s| s.0 != o);
        if self.slots.len() < 64 {
            self.slots.push((o, v));
        }
    }

    fn clobber(&mut self) {
        for r in (1..=15).chain([24, 25, 31]) {
            self.r[r] = Unk;
            self.gw |= 1 << r;
        }
        self.hl = Unk;
        self.fw |= 0x000f_ffff;
        self.ret = 7;
    }
}

/// One field of an argument's block (or of a sub-block a pointer field of it points at).
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Field {
    pub width: u8,
    pub load: bool,
    pub store: bool,
    /// Accessed by FPU or VU0 loads/stores (a float or a vector).
    pub float: bool,
    /// Loaded as a word and then used as a base pointer (a pointer field).
    pub pointer: bool,
}

/// Counts of memory accesses by where the base pointer comes from.
#[derive(Clone, Copy, Default, Debug)]
pub struct Access {
    pub stack: u32,
    pub arg: u32,
    pub glob_static: u32,
    pub glob_indexed: u32,
    pub glob_object: u32,
    pub hw: u32,
    pub unk: u32,
}

#[derive(Clone, Default, Debug)]
pub struct Scan {
    pub addr: u32,
    pub size: u32,
    pub calls: BTreeSet<u32>,
    pub tail: BTreeSet<u32>,
    pub jalr: u32,
    pub jr_ind: u32,
    pub syscall: u32,
    pub cop0: u32,
    pub cache: u32,
    pub vu0: u32,
    /// `cfc2` of the VU0 status, MAC or clip flags (vi16..18): kh2-ee reads zero there.
    pub vu_flags: u32,
    pub unimpl: u32,
    pub st: Access,
    pub ld: Access,
    /// Static global addresses stored to / loaded from (indexed tables by their base).
    pub glob_st: BTreeSet<u32>,
    pub glob_ld: BTreeSet<u32>,
    /// Argument registers a0..a3, t0..t3 read before written (bit i).
    pub int_args: u8,
    /// f12..f19 read before written (bit i).
    pub float_args: u8,
    /// v0 / v1 / f0 written on some return path (bits 0, 1, 2).
    pub ret: u8,
    /// (argument, parent field or -1, offset) -> field.
    pub fields: BTreeMap<(u8, i32, i32), Field>,
    /// Stores through an argument pointer at an unknown offset (arrays indexed at run time), per argument.
    pub arg_indexed: u8,
    /// Immediates compared against (slti/sltiu, and constants loaded into a register that is then compared).
    pub consts: BTreeSet<i32>,
}

/// End of the main executable's initialised data: a static load below it reads the executable's own tables.
pub const DATA_END: u32 = 0x0038_8ebc;

impl Scan {
    /// The function's own class (FAC 1.3, without the call closure): `X` not runnable in kh2-ee (an instruction it
    /// lacks, a syscall, COP0, a hardware register), `S` memory reached through a pointer loaded from a global or of
    /// unknown origin (needs a state layout), `G` static or table-indexed globals (the generator lays them out), `A`
    /// only its arguments' memory, the stack and constant tables. Loads of static globals below [`DATA_END`] count as
    /// constant tables here; the table of FAC 1.3 also asks whether any function stores there.
    pub fn class(&self) -> char {
        if self.unimpl > 0 || self.syscall > 0 || self.cop0 > 0 || self.st.hw + self.ld.hw > 0 {
            'X'
        } else if self.st.glob_object + self.st.unk + self.ld.glob_object + self.ld.unk > 0 {
            'S'
        } else if self.st.glob_static + self.st.glob_indexed > 0 || self.glob_ld.iter().any(|&a| a >= DATA_END) {
            'G'
        } else {
            'A'
        }
    }

    /// No call, tail call or indirect call.
    pub fn leaf(&self) -> bool {
        self.calls.is_empty() && self.tail.is_empty() && self.jalr == 0
    }
}

enum Ctl {
    Normal,
    Branch { target: u32, likely: bool, uncond: bool },
    Jump { target: u32, link: bool },
    JumpReg { rs: usize, link: bool },
}

fn ctl(pc: u32, w: u32) -> Ctl {
    let op = w >> 26;
    let rs = ((w >> 21) & 31) as usize;
    let rt = ((w >> 16) & 31) as usize;
    let bt = pc.wrapping_add(4).wrapping_add(((w as u16 as i16 as i32) << 2) as u32);
    match op {
        0x00 => match w & 63 {
            0x08 => Ctl::JumpReg { rs, link: false },
            0x09 => Ctl::JumpReg { rs, link: true },
            _ => Ctl::Normal,
        },
        0x01 => match rt {
            0x00..=0x03 | 0x10..=0x13 => Ctl::Branch { target: bt, likely: rt & 2 != 0, uncond: rs == 0 && (rt & 1) == 1 },
            _ => Ctl::Normal,
        },
        0x02 | 0x03 => Ctl::Jump { target: (pc.wrapping_add(4) & 0xf000_0000) | ((w & 0x03ff_ffff) << 2), link: op == 3 },
        0x04 | 0x14 => Ctl::Branch { target: bt, likely: op == 0x14, uncond: rs == rt },
        0x05 | 0x06 | 0x07 | 0x15 | 0x16 | 0x17 => Ctl::Branch { target: bt, likely: op >= 0x14, uncond: false },
        0x11 | 0x12 if rs == 8 => Ctl::Branch { target: bt, likely: rt & 2 != 0, uncond: false },
        _ => Ctl::Normal,
    }
}

struct Ctx<'a> {
    rec: Option<&'a mut Scan>,
    in_func: (u32, u32),
}

impl Ctx<'_> {
    fn read(&mut self, s: &State, r: usize) {
        if (4..=11).contains(&r) && s.gw & (1 << r) == 0
            && let Some(sc) = self.rec.as_deref_mut()
        {
            sc.int_args |= 1 << (r - 4);
        }
    }

    fn fread(&mut self, s: &State, f: usize) {
        if (12..=19).contains(&f) && s.fw & (1 << f) == 0
            && let Some(sc) = self.rec.as_deref_mut()
        {
            sc.float_args |= 1 << (f - 12);
        }
    }

    fn access(&mut self, base: V, imm: i32, width: u8, store: bool, float: bool) {
        let Some(sc) = self.rec.as_deref_mut() else { return };
        let acc = if store { &mut sc.st } else { &mut sc.ld };
        match base {
            Sp(_) => acc.stack += 1,
            Arg { a, off } => {
                acc.arg += 1;
                if off == OFF_UNK {
                    if store {
                        sc.arg_indexed |= 1 << a;
                    }
                } else {
                    let f = sc.fields.entry((a, -1, off.wrapping_add(imm))).or_default();
                    f.width = f.width.max(width);
                    f.load |= !store;
                    f.store |= store;
                    f.float |= float;
                }
            }
            Deref { a, f: pf, off } => {
                acc.arg += 1;
                sc.fields.entry((a, -1, pf)).or_default().pointer = true;
                if off != OFF_UNK {
                    let f = sc.fields.entry((a, pf, off.wrapping_add(imm))).or_default();
                    f.width = f.width.max(width);
                    f.load |= !store;
                    f.store |= store;
                    f.float |= float;
                }
            }
            ArgMem | Data => acc.arg += 1,
            Const(c) => {
                let a = c.wrapping_add(imm as u32);
                if hw_like(a) {
                    acc.hw += 1;
                } else {
                    acc.glob_static += 1;
                    if store { &mut sc.glob_st } else { &mut sc.glob_ld }.insert(a);
                }
            }
            GlobPtr(b) => {
                if hw_like(b) {
                    acc.hw += 1;
                } else {
                    acc.glob_indexed += 1;
                    if b != 0 {
                        if store { &mut sc.glob_st } else { &mut sc.glob_ld }.insert(b);
                    }
                }
            }
            GlobVal => acc.glob_object += 1,
            Unk => acc.unk += 1,
        }
    }

    fn load_val(&self, s: &State, base: V, imm: i32, width: u8) -> V {
        if width < 4 {
            return Data;
        }
        match base {
            Sp(o) if o != OFF_UNK => s.slot(o.wrapping_add(imm)),
            Sp(_) => Unk,
            Arg { a, off } if off != OFF_UNK => Deref { a, f: off.wrapping_add(imm), off: 0 },
            Arg { .. } | Deref { .. } | ArgMem | Data => ArgMem,
            Const(_) | GlobPtr(_) | GlobVal => GlobVal,
            Unk => Unk,
        }
    }

    /// One instruction's effect on the state (and, when recording, on the scan).
    fn transfer(&mut self, pc: u32, w: u32, mut s: State) -> State {
        let op = w >> 26;
        let rs = ((w >> 21) & 31) as usize;
        let rt = ((w >> 16) & 31) as usize;
        let rd = ((w >> 11) & 31) as usize;
        let sa = (w >> 6) & 31;
        let funct = w & 63;
        let imm = w as u16 as i16 as i32;
        let (a, b) = (s.r[rs], s.r[rt]);
        macro_rules! rec {
            ($f:ident) => {
                if let Some(sc) = self.rec.as_deref_mut() {
                    sc.$f += 1;
                }
            };
        }
        match op {
            0x00 => {
                let shift_imm = matches!(funct, 0x00 | 0x02 | 0x03 | 0x38 | 0x3a | 0x3b | 0x3c | 0x3e | 0x3f);
                if !shift_imm && !matches!(funct, 0x10 | 0x12 | 0x0c | 0x0d | 0x0f | 0x28) {
                    self.read(&s, rs);
                }
                if !matches!(funct, 0x08 | 0x09 | 0x11 | 0x13 | 0x10 | 0x12 | 0x0c | 0x0d | 0x0f | 0x28 | 0x29) {
                    self.read(&s, rt);
                }
                match funct {
                    0x00 | 0x02 | 0x03 => {
                        let v = match b {
                            Const(c) => Const(match funct {
                                0 => c << sa,
                                2 => c >> sa,
                                _ => ((c as i32) >> sa) as u32,
                            }),
                            o => scalar(o),
                        };
                        s.set(rd, v)
                    }
                    0x04 | 0x06 | 0x07 | 0x14 | 0x16 | 0x17 | 0x38 | 0x3a | 0x3b | 0x3c | 0x3e | 0x3f => s.set(rd, scalar(b)),
                    0x09 => s.set(rd, Unk),
                    0x0a | 0x0b => {
                        let v = join(s.r[rd], a);
                        s.set(rd, v)
                    }
                    0x0c => rec!(syscall),
                    0x10 | 0x12 => s.set(rd, s.hl),
                    0x11 | 0x13 => s.hl = a,
                    0x18..=0x1b => {
                        s.hl = if a == Unk || b == Unk { Unk } else { Data };
                        if funct < 0x1a {
                            s.set(rd, s.hl)
                        }
                    }
                    0x21 | 0x20 | 0x2c | 0x2d => s.set(rd, add(a, b)),
                    0x25 => s.set(
                        rd,
                        match (a, b) {
                            (Const(x), Const(y)) => Const(x | y),
                            _ => add(a, b),
                        },
                    ),
                    0x22 | 0x23 | 0x2e | 0x2f => s.set(
                        rd,
                        match (a, b) {
                            (o, Const(0)) => o,
                            (Const(x), Const(y)) => Const(x.wrapping_sub(y)),
                            (Sp(_), _) => Sp(OFF_UNK),
                            _ => {
                                if a == Unk || b == Unk {
                                    Unk
                                } else {
                                    Data
                                }
                            }
                        },
                    ),
                    0x24 => s.set(
                        rd,
                        match (a, b) {
                            (Const(x), Const(y)) => Const(x & y),
                            (p @ (Sp(_) | Arg { .. } | GlobPtr(_)), Const(_)) | (Const(_), p @ (Sp(_) | Arg { .. } | GlobPtr(_))) => add(p, Data),
                            _ => scalar(if a == Unk { a } else { b }),
                        },
                    ),
                    0x26 | 0x27 | 0x2a | 0x2b => s.set(rd, if a == Unk || b == Unk { Unk } else { Data }),
                    0x28 => s.set(rd, Data),
                    _ => {}
                }
            }
            0x01 => self.read(&s, rs),
            0x03 => s.set(31, Unk),
            0x04 | 0x05 | 0x14 | 0x15 => {
                self.read(&s, rs);
                self.read(&s, rt);
                for v in [a, b] {
                    if let (Const(c), Some(sc)) = (v, self.rec.as_deref_mut()) {
                        sc.consts.insert(c as i32);
                    }
                }
            }
            0x06 | 0x07 | 0x16 | 0x17 => self.read(&s, rs),
            0x08 | 0x09 | 0x18 | 0x19 => {
                self.read(&s, rs);
                s.set(rt, add(a, Const(imm as u32)))
            }
            0x0a | 0x0b => {
                self.read(&s, rs);
                if let Some(sc) = self.rec.as_deref_mut() {
                    sc.consts.insert(imm);
                }
                s.set(rt, scalar(a))
            }
            0x0c => {
                self.read(&s, rs);
                s.set(
                    rt,
                    match a {
                        Const(c) => Const(c & (imm as u16 as u32)),
                        o => scalar(o),
                    },
                )
            }
            0x0d => {
                self.read(&s, rs);
                s.set(
                    rt,
                    match a {
                        Const(c) => Const(c | (imm as u16 as u32)),
                        o => add(o, Const(0)),
                    },
                )
            }
            0x0e => {
                self.read(&s, rs);
                s.set(rt, scalar(a))
            }
            0x0f => s.set(rt, Const((imm as u16 as u32) << 16)),
            0x10 => {
                rec!(cop0);
                if rs == 0 {
                    s.set(rt, Unk)
                }
            }
            0x11 => {
                let (fs, ft, fd) = (rd, rt, sa as usize);
                match rs {
                    0x00 => {
                        self.fread(&s, fs);
                        s.set(rt, Data)
                    }
                    0x02 => s.set(rt, Data),
                    0x04 | 0x06 => {
                        self.read(&s, rt);
                        if rs == 4 {
                            s.fset(fs)
                        }
                    }
                    0x10 => {
                        let reads: &[usize] = match funct {
                            0x04 => &[ft],
                            0x05 | 0x06 | 0x07 | 0x24 => &[fs],
                            _ => &[fs, ft],
                        };
                        for &r in reads {
                            self.fread(&s, r);
                        }
                        if !matches!(funct, 0x18..=0x1b | 0x1e | 0x1f | 0x30..=0x3f) {
                            s.fset(fd)
                        }
                    }
                    0x14 => {
                        self.fread(&s, fs);
                        s.fset(fd)
                    }
                    _ => {}
                }
            }
            0x12 => {
                rec!(vu0);
                if w & (1 << 25) == 0 {
                    match rs {
                        0x01 => s.set(rt, Data),
                        0x02 => {
                            if (16..=18).contains(&rd) {
                                rec!(vu_flags);
                            }
                            s.set(rt, Data)
                        }
                        0x05 | 0x06 => self.read(&s, rt),
                        _ => {}
                    }
                }
            }
            0x1c => {
                self.read(&s, rs);
                self.read(&s, rt);
                match funct {
                    0x10 | 0x12 => s.set(rd, s.hl),
                    0x11 | 0x13 => s.hl = a,
                    0x00 | 0x01 | 0x18..=0x1b | 0x20 | 0x21 => {
                        s.hl = if a == Unk || b == Unk { Unk } else { Data };
                        if !matches!(funct, 0x1a | 0x1b) {
                            s.set(rd, s.hl)
                        }
                    }
                    0x09 if matches!(sa, 0x08 | 0x09) => s.set(rd, s.hl),
                    _ => s.set(rd, if a == Unk || b == Unk { Unk } else { Data }),
                }
            }
            0x1a | 0x1b | 0x1e | 0x20..=0x27 | 0x37 => {
                self.read(&s, rs);
                let width = match op {
                    0x20 | 0x24 => 1,
                    0x21 | 0x25 => 2,
                    0x22 | 0x23 | 0x26 | 0x27 => 4,
                    0x1e => 16,
                    _ => 8,
                };
                if matches!(op, 0x1a | 0x1b | 0x22 | 0x26) {
                    self.read(&s, rt);
                }
                self.access(a, imm, width, false, false);
                let v = self.load_val(&s, a, imm, width);
                if let (Deref { a: x, f, .. }, Some(sc)) = (v, self.rec.as_deref_mut()) {
                    let _ = (x, f, sc);
                }
                s.set(rt, v)
            }
            0x28 | 0x29 | 0x2a | 0x2b | 0x2c | 0x2d | 0x2e | 0x3f | 0x1f => {
                self.read(&s, rs);
                self.read(&s, rt);
                let width = match op {
                    0x28 => 1,
                    0x29 => 2,
                    0x2a | 0x2b | 0x2e => 4,
                    0x1f => 16,
                    _ => 8,
                };
                self.access(a, imm, width, true, false);
                match a {
                    Sp(o) if o != OFF_UNK && width >= 4 => s.put_slot(o.wrapping_add(imm), b),
                    Sp(_) => {}
                    _ => {}
                }
            }
            0x31 | 0x36 => {
                self.read(&s, rs);
                self.access(a, imm, if op == 0x31 { 4 } else { 16 }, false, true);
                if op == 0x31 {
                    s.fset(rt)
                } else {
                    rec!(vu0)
                }
            }
            0x39 | 0x3e => {
                self.read(&s, rs);
                if op == 0x39 {
                    self.fread(&s, rt)
                } else {
                    rec!(vu0)
                }
                self.access(a, imm, if op == 0x39 { 4 } else { 16 }, true, true);
            }
            0x2f => rec!(cache),
            _ => {}
        }
        let _ = (pc, self.in_func);
        s
    }
}

/// Marks pointer fields: a field whose loaded value is used as a base (`Deref` accesses record their parent).
fn finish(sc: &mut Scan) {
    let parents: Vec<(u8, i32)> = sc.fields.keys().filter(|k| k.1 >= 0).map(|k| (k.0, k.1)).collect();
    for (a, f) in parents {
        let e = sc.fields.entry((a, -1, f)).or_default();
        e.pointer = true;
        e.width = e.width.max(4);
        e.load = true;
    }
}

/// Instruction support of kh2-ee, probed by running each distinct word once in a scratch interpreter.
pub struct Probe {
    ee: kh2_ee::Ee,
    cache: HashMap<u32, bool>,
}

const PROBE_AT: u32 = 0x0010_0000;

impl Default for Probe {
    fn default() -> Self {
        Self::new()
    }
}

impl Probe {
    pub fn new() -> Probe {
        let mut ee = kh2_ee::Ee::new();
        ee.limit = 8;
        Probe { ee, cache: HashMap::new() }
    }

    /// Whether kh2-ee implements this instruction word.
    pub fn supported(&mut self, w: u32) -> bool {
        if let Some(&v) = self.cache.get(&w) {
            return v;
        }
        let e = &mut self.ee;
        e.gpr = [0; 32];
        e.w32(PROBE_AT, w);
        e.w32(PROBE_AT + 4, 0x03e0_0008);
        e.w32(PROBE_AT + 8, 0);
        let r = e.call(PROBE_AT, &[], &[], 0x01ff_0000);
        let ok = !matches!(r, Err(kh2_ee::Fault::Unimplemented { pc, .. }) if pc == PROBE_AT);
        self.cache.insert(w, ok);
        ok
    }
}

/// Scans the function at `addr` of `size` bytes; `code(pc)` reads an instruction word.
pub fn scan(addr: u32, size: u32, code: &dyn Fn(u32) -> Option<u32>, probe: &mut Probe) -> Scan {
    let n = (size / 4) as usize;
    let words: Vec<u32> = (0..n).map(|i| code(addr + 4 * i as u32).unwrap_or(0)).collect();
    let mut entry: Vec<Option<State>> = vec![None; n];
    let mut dentry: Vec<Option<State>> = vec![None; n];
    let mut sc = Scan { addr, size, ..Default::default() };
    let end = addr + size;
    let mut jr_state: Option<State> = None;
    let mut ret_any = 0u8;
    let mut work: Vec<usize> = Vec::new();
    let idx = |t: u32| -> Option<usize> { (t >= addr && t < end && t.is_multiple_of(4)).then(|| ((t - addr) / 4) as usize) };
    let mut ctx = Ctx { rec: None, in_func: (addr, end) };
    let push = |entry: &mut Vec<Option<State>>, work: &mut Vec<usize>, i: usize, s: State| {
        if i >= entry.len() {
            return;
        }
        let new = match &entry[i] {
            None => s,
            Some(old) => {
                let j = old.join(&s);
                if &j == old {
                    return;
                }
                j
            }
        };
        entry[i] = Some(new);
        work.push(i);
    };
    let mut seeded = false;
    loop {
        if !seeded {
            push(&mut entry, &mut work, 0, State::entry());
            seeded = true;
        }
        while let Some(i) = work.pop() {
            let s = entry[i].clone().unwrap();
            let pc = addr + 4 * i as u32;
            let w = words[i];
            let s1 = ctx.transfer(pc, w, s);
            match ctl(pc, w) {
                Ctl::Normal => push(&mut entry, &mut work, i + 1, s1),
                c => {
                    if i + 1 < n {
                        dentry[i + 1] = Some(match &dentry[i + 1] {
                            None => s1.clone(),
                            Some(o) => o.join(&s1),
                        });
                    }
                    let ds = if i + 1 < n { ctx.transfer(pc + 4, words[i + 1], s1.clone()) } else { s1.clone() };
                    match c {
                        Ctl::Branch { target, likely, uncond } => {
                            if let Some(t) = idx(target) {
                                push(&mut entry, &mut work, t, ds.clone());
                            }
                            if !uncond {
                                push(&mut entry, &mut work, i + 2, if likely { s1 } else { ds });
                            }
                        }
                        Ctl::Jump { target, link } => {
                            if link {
                                let mut a = ds;
                                a.clobber();
                                push(&mut entry, &mut work, i + 2, a);
                            } else if let Some(t) = idx(target) {
                                push(&mut entry, &mut work, t, ds);
                            }
                        }
                        Ctl::JumpReg { rs, link } => {
                            if link {
                                let mut a = ds;
                                a.clobber();
                                push(&mut entry, &mut work, i + 2, a);
                            } else if rs == 31 {
                                ret_any |= ds.ret;
                            } else {
                                jr_state = Some(match jr_state {
                                    None => ds,
                                    Some(o) => o.join(&ds),
                                });
                            }
                        }
                        Ctl::Normal => unreachable!(),
                    }
                }
            }
        }
        // Code reached only through a jump table: seed the first unreached instruction with the state at the jump.
        let Some(u) = (0..n).find(|&i| entry[i].is_none() && dentry[i].is_none() && words[i] != 0) else { break };
        let s = jr_state.clone().unwrap_or_else(State::entry);
        push(&mut entry, &mut work, u, s);
    }
    // Recording pass over every reached instruction with its final entry state.
    let mut ctx = Ctx { rec: Some(&mut sc), in_func: (addr, end) };
    for i in 0..n {
        let pc = addr + 4 * i as u32;
        let w = words[i];
        for s in [&entry[i], &dentry[i]].into_iter().flatten() {
            ctx.transfer(pc, w, s.clone());
        }
    }
    for (i, &w) in words.iter().enumerate() {
        let pc = addr + 4 * i as u32;
        if !probe.supported(w) {
            sc.unimpl += 1;
        }
        match ctl(pc, w) {
            Ctl::Jump { target, link } => {
                if link {
                    sc.calls.insert(target);
                } else if idx(target).is_none() {
                    sc.tail.insert(target);
                }
            }
            Ctl::Branch { target, .. } if idx(target).is_none() => {
                sc.tail.insert(target);
            }
            Ctl::JumpReg { rs, link } => {
                if link {
                    sc.jalr += 1;
                } else if rs != 31 {
                    sc.jr_ind += 1;
                }
            }
            _ => {}
        }
    }
    sc.ret = ret_any;
    finish(&mut sc);
    sc
}
