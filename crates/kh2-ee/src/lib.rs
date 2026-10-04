//! Minimal Emotion Engine interpreter: R5900 integer core (64/128-bit GPRs, MMI), COP1 FPU and
//! COP2 (VU0) macro mode. No OS, interrupts, DMA or VU microprograms: it exists to call leaf
//! routines of the user's own game executable (loaded from their disc at runtime) with prepared
//! memory, so native ports can be checked against the original behaviour bit for bit.
//!
//! Instruction semantics follow the public R5900 and VU user manuals; behaviour of corner cases
//! (FPU clamping, LWR sign extension) follows the conventions documented by PCSX2.

use std::collections::HashMap;

pub const RAM: usize = 32 << 20;
const SPR_BASE: u32 = 0x7000_0000;
const SPR: usize = 16 << 10;
/// Return address used by [`Ee::call`]; reaching it ends the call.
pub const SENTINEL: u32 = 0xFFFF_FFF0;

pub type Hook = fn(&mut Ee);

#[derive(Debug)]
pub enum Fault {
    Unimplemented { pc: u32, word: u32, what: &'static str },
    Memory { pc: u32, addr: u32 },
    Runaway { pc: u32 },
}

pub struct Ee {
    pub ram: Vec<u8>,
    pub spr: Vec<u8>,
    pub gpr: [u128; 32],
    pub hi: u128,
    pub lo: u128,
    pub sa: u32,
    pub pc: u32,
    npc: u32,
    pub fpr: [f32; 32],
    pub facc: f32,
    pub fcr31: u32,
    pub vf: [[f32; 4]; 32],
    pub vi: [u32; 32],
    pub vacc: [f32; 4],
    pub q: f32,
    pub hooks: HashMap<u32, Hook>,
    /// Debug: called before the instruction at an address executes (the call continues).
    pub probes: HashMap<u32, Hook>,
    pub steps: u64,
    pub limit: u64,
    fault: Option<Fault>,
    npc_next: Option<u32>,
    skip_delay: bool,
    /// Debug: log writes into [lo, hi).
    pub watch: Option<(u32, u32)>,
}

/// Register names (ABI).
pub mod reg {
    pub const V0: usize = 2;
    pub const V1: usize = 3;
    pub const A0: usize = 4;
    pub const T0: usize = 8;
    pub const SP: usize = 29;
    pub const RA: usize = 31;
}

fn clamp(f: f32) -> f32 {
    // The EE FPU and VUs have no infinities or NaNs: overflow saturates to the largest normal.
    if f.is_nan() {
        f32::MAX
    } else if f.is_infinite() {
        f32::MAX.copysign(f)
    } else {
        f
    }
}

fn sx32(v: u32) -> u64 {
    v as i32 as i64 as u64
}

impl Default for Ee {
    fn default() -> Self {
        Self::new()
    }
}

impl Ee {
    pub fn new() -> Ee {
        let mut e = Ee {
            ram: vec![0; RAM],
            spr: vec![0; SPR],
            gpr: [0; 32],
            hi: 0,
            lo: 0,
            sa: 0,
            pc: 0,
            npc: 4,
            fpr: [0.0; 32],
            facc: 0.0,
            fcr31: 0,
            vf: [[0.0; 4]; 32],
            vi: [0; 32],
            vacc: [0.0; 4],
            q: 0.0,
            hooks: HashMap::new(),
            probes: HashMap::new(),
            steps: 0,
            limit: 50_000_000,
            fault: None,
            npc_next: None,
            skip_delay: false,
            watch: None,
        };
        e.vf[0] = [0.0, 0.0, 0.0, 1.0];
        e
    }

    /// Loads the PT_LOAD segments of an ELF32 executable. Returns the entry point.
    pub fn load_elf(&mut self, elf: &[u8]) -> Option<u32> {
        let u16_ = |o: usize| -> Option<usize> { Some(u16::from_le_bytes(elf.get(o..o + 2)?.try_into().ok()?) as usize) };
        let u32_ = |o: usize| -> Option<u32> { Some(u32::from_le_bytes(elf.get(o..o + 4)?.try_into().ok()?)) };
        if elf.get(..4)? != b"\x7fELF" {
            return None;
        }
        let entry = u32_(0x18)?;
        let phoff = u32_(0x1C)? as usize;
        let phentsize: usize = u16_(0x2A)?;
        let phnum: usize = u16_(0x2C)?;
        for i in 0..phnum {
            let p = phoff + i * phentsize;
            if u32_(p)? != 1 {
                continue;
            }
            let (off, vaddr, filesz, memsz) = (u32_(p + 4)? as usize, u32_(p + 8)?, u32_(p + 16)? as usize, u32_(p + 20)? as usize);
            let dst = (vaddr & 0x01FF_FFFF) as usize;
            self.ram.get_mut(dst..dst + memsz)?.fill(0);
            self.ram[dst..dst + filesz].copy_from_slice(elf.get(off..off + filesz)?);
        }
        Some(entry)
    }

    // ---- memory -------------------------------------------------------------------------

    fn slice(&mut self, addr: u32, n: usize) -> Option<&mut [u8]> {
        let a = addr & 0x1FFF_FFFF;
        if (a as usize) + n <= RAM {
            return self.ram.get_mut(a as usize..a as usize + n);
        }
        if (SPR_BASE..SPR_BASE + SPR as u32).contains(&addr) {
            let o = (addr - SPR_BASE) as usize;
            return self.spr.get_mut(o..o + n);
        }
        None
    }

    fn mem_fault(&mut self, addr: u32) {
        if self.fault.is_none() {
            self.fault = Some(Fault::Memory { pc: self.pc, addr });
        }
    }

    pub fn read<const N: usize>(&mut self, addr: u32) -> [u8; N] {
        match self.slice(addr, N) {
            Some(s) => s.try_into().unwrap(),
            None => {
                self.mem_fault(addr);
                [0; N]
            }
        }
    }

    pub fn write(&mut self, addr: u32, bytes: &[u8]) {
        if let Some((lo, hi)) = self.watch
            && addr < hi
            && addr + bytes.len() as u32 > lo
        {
            eprintln!("watch: pc {:08x} writes {:08x} <- {:02x?}", self.pc, addr, bytes);
        }
        match self.slice(addr, bytes.len()) {
            Some(s) => s.copy_from_slice(bytes),
            None => self.mem_fault(addr),
        }
    }

    pub fn r8(&mut self, a: u32) -> u8 {
        self.read::<1>(a)[0]
    }
    pub fn r16(&mut self, a: u32) -> u16 {
        u16::from_le_bytes(self.read(a))
    }
    pub fn r32(&mut self, a: u32) -> u32 {
        u32::from_le_bytes(self.read(a))
    }
    pub fn r64(&mut self, a: u32) -> u64 {
        u64::from_le_bytes(self.read(a))
    }
    pub fn r128(&mut self, a: u32) -> u128 {
        u128::from_le_bytes(self.read(a))
    }
    pub fn f32_at(&mut self, a: u32) -> f32 {
        f32::from_bits(self.r32(a))
    }
    pub fn w32(&mut self, a: u32, v: u32) {
        self.write(a, &v.to_le_bytes())
    }
    pub fn wf32(&mut self, a: u32, v: f32) {
        self.w32(a, v.to_bits())
    }

    // ---- registers ------------------------------------------------------------------------

    fn g(&self, r: usize) -> u64 {
        self.gpr[r] as u64
    }
    fn g32(&self, r: usize) -> u32 {
        self.gpr[r] as u32
    }
    /// Writes the low 64 bits; the upper 64 bits of the register are preserved.
    fn set(&mut self, r: usize, v: u64) {
        if r != 0 {
            self.gpr[r] = (self.gpr[r] & !(u64::MAX as u128)) | v as u128;
        }
    }
    fn set128(&mut self, r: usize, v: u128) {
        if r != 0 {
            self.gpr[r] = v;
        }
    }
    pub fn arg(&mut self, i: usize, v: u64) {
        // EE ABI: integer arguments in a0-a3 then t0-t3.
        let r = if i < 4 { reg::A0 + i } else { reg::T0 + i - 4 };
        self.set(r, v);
    }

    // ---- calls --------------------------------------------------------------------------

    /// Calls `addr` with integer args (a0..), float args (f12..), and a stack at `sp`.
    /// Returns v0; f0 holds a float result.
    pub fn call(&mut self, addr: u32, args: &[u64], fargs: &[f32], sp: u32) -> Result<u64, Fault> {
        for (i, &a) in args.iter().enumerate() {
            self.arg(i, a);
        }
        for (i, &f) in fargs.iter().enumerate() {
            self.fpr[12 + i] = f;
        }
        self.set(reg::SP, sp as u64);
        self.set(reg::RA, SENTINEL as u64);
        self.pc = addr;
        self.npc = addr + 4;
        let start = self.steps;
        while self.pc != SENTINEL {
            if let Some(h) = self.hooks.get(&self.pc).copied() {
                h(self);
                let ra = self.g32(reg::RA);
                self.pc = ra;
                self.npc = ra.wrapping_add(4);
                continue;
            }
            if let Some(p) = self.probes.get(&self.pc).copied() {
                p(self);
            }
            self.step();
            if let Some(f) = self.fault.take() {
                return Err(f);
            }
            if self.steps - start > self.limit {
                return Err(Fault::Runaway { pc: self.pc });
            }
        }
        Ok(self.g(reg::V0))
    }

    fn unimpl(&mut self, word: u32, what: &'static str) {
        if self.fault.is_none() {
            self.fault = Some(Fault::Unimplemented { pc: self.pc, word, what });
        }
    }

    fn branch(&mut self, cond: bool, target: u32, likely: bool) {
        if cond {
            self.npc_next = Some(target);
        } else if likely {
            // Branch-likely not taken: the delay slot is nullified.
            self.skip_delay = true;
        }
    }

    pub fn step(&mut self) {
        self.steps += 1;
        let pc = self.pc;
        let w = self.r32(pc);
        self.npc_next = None;
        self.skip_delay = false;
        self.exec(w);
        if self.skip_delay {
            self.pc = self.npc.wrapping_add(4);
            self.npc = self.pc.wrapping_add(4);
        } else {
            let next = self.npc_next.unwrap_or(self.npc.wrapping_add(4));
            self.pc = self.npc;
            self.npc = next;
        }
    }
}

mod exec;
