//! A small oracle rig for the memory card unit. The generated factory harness (`kh2_factory::fuzz`) lays argument blocks at fixed slots
//! and cannot point the unit's pointer globals (the card info block, the file buffer, the directory table, the CRC table) at a buffer,
//! so these tests set those pointers themselves, fill every buffer with random bytes, run the original in kh2-ee and compare each
//! buffer and the return value with the port run on copies of the same bytes. Skips without a disc (fails with `KH2_REQUIRE_DISC=1`).
#![allow(dead_code)]

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::{Rng, STACK};

/// Buffers the unit's pointer globals are pointed at.
pub const TABLE: u32 = 0x0190_0000;
pub const FILEBUF: u32 = 0x0191_0000;
pub const INFO: u32 = 0x0192_0000;
pub const LIST: u32 = 0x0193_0000;
pub const DIRTAB: u32 = 0x0194_0000;
pub const NAME: u32 = 0x0195_0000;
pub const PATH: u32 = 0x0196_0000;
/// A second body buffer for the functions that take a body pointer.
pub const OTHER: u32 = 0x0197_0000;
/// The save block.
pub const SAVE: u32 = 0x33e860;

pub struct Orig {
    pub ee: Ee,
    pub rng: Rng,
}

fn hook_zero(e: &mut Ee) {
    e.gpr[reg::V0] = 0;
}

impl Orig {
    /// The original's executable in an interpreter with the unit's pointer globals set; `None` without a disc.
    pub fn new(what: &str, seed: u64) -> Option<Orig> {
        let elf = kh2_factory::exe_or_skip(what)?;
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let mut o = Orig { ee, rng: Rng::new(seed) };
        o.point_globals();
        Some(o)
    }

    pub fn point_globals(&mut self) {
        for (g, v) in [(0x36cb3c, TABLE), (0x36cb14, FILEBUF), (0x36cb18, INFO), (0x36cb1c, LIST), (0x36cb20, DIRTAB), (0x36cb0c, NAME), (0x36cb10, PATH)] {
            self.ee.w32(g, v);
        }
    }

    /// Replaces a callee by `v0 = 0`.
    pub fn stub(&mut self, addr: u32) {
        self.ee.hooks.insert(addr, hook_zero);
    }

    pub fn get(&self, addr: u32, len: usize) -> Vec<u8> {
        self.ee.ram[addr as usize..addr as usize + len].to_vec()
    }

    pub fn put(&mut self, addr: u32, bytes: &[u8]) {
        self.ee.ram[addr as usize..addr as usize + bytes.len()].copy_from_slice(bytes);
    }

    pub fn random(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.rng.word() as u8).collect()
    }

    /// Writes random bytes over a range and returns them.
    pub fn fill(&mut self, addr: u32, len: usize) -> Vec<u8> {
        let b = self.random(len);
        self.put(addr, &b);
        b
    }

    pub fn word(&mut self, addr: u32) -> u32 {
        self.ee.r32(addr)
    }

    pub fn set_word(&mut self, addr: u32, v: u32) {
        self.ee.w32(addr, v);
    }

    /// Calls a function of the original, panicking on a fault.
    pub fn call(&mut self, addr: u32, args: &[u64]) -> u64 {
        self.ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
    }

    /// Reads a NUL-terminated string at an address.
    pub fn cstr(&self, addr: u32) -> Vec<u8> {
        let mut v = Vec::new();
        let mut a = addr as usize;
        while self.ee.ram[a] != 0 {
            v.push(self.ee.ram[a]);
            a += 1;
        }
        v
    }

    /// The bytes of the `n` bytes at the pointer stored at `ptr_addr`.
    pub fn deref(&self, ptr_addr: u32, n: usize) -> Vec<u8> {
        let p = u32::from_le_bytes(self.ee.ram[ptr_addr as usize..ptr_addr as usize + 4].try_into().unwrap());
        self.get(p, n)
    }
}

/// A random number in `lo..=hi`.
pub fn range(rng: &mut Rng, lo: i64, hi: i64) -> i64 {
    lo + rng.below((hi - lo + 1) as u64) as i64
}

use kh2_save::body::VariantPairs;
use kh2_save::icon::IconEnv;
use kh2_save::names::NameEnv;

impl Orig {
    /// The icon tables and strings of the executable.
    pub fn icon_env(&mut self) -> IconEnv {
        let arr = |o: &Orig, a: u32, n: usize| o.get(a, n);
        let mut marks = [[0u8; 2]; 4];
        for (i, m) in marks.iter_mut().enumerate() {
            let p = self.ee.r32(0x36caf8 + 4 * i as u32);
            m.copy_from_slice(&self.get(p, 2));
        }
        let blank_ptr = self.ee.r32(0x36cb04);
        IconEnv {
            magic: arr(self, 0x37f570, 4).try_into().unwrap(),
            background: arr(self, 0x37f590, 0x40).try_into().unwrap(),
            light_dirs: arr(self, 0x37f5d0, 0x30).try_into().unwrap(),
            light_colors: arr(self, 0x37f600, 0x30).try_into().unwrap(),
            ambient: arr(self, 0x37f630, 0x10).try_into().unwrap(),
            line1: {
                let p = self.ee.r32(0x36caf0);
                self.cstr(p)
            },
            line2: {
                let p = self.ee.r32(0x36caf4);
                self.cstr(p)
            },
            marks,
            blank: self.get(blank_ptr, 2).try_into().unwrap(),
        }
    }

    /// The icon variant flag pairs at `0x37f640`.
    pub fn variants(&mut self) -> VariantPairs {
        let mut v = [(0u16, 0u16); 8];
        for (i, p) in v.iter_mut().enumerate() {
            *p = (self.ee.r16(0x37f640 + 4 * i as u32), self.ee.r16(0x37f642 + 4 * i as u32));
        }
        v
    }

    /// The name parts the executable holds for a region (1 to 4; row 0 has a null third pointer).
    pub fn name_env(&mut self, region: u32) -> NameEnv {
        let mut parts: [String; 3] = Default::default();
        for (k, part) in parts.iter_mut().enumerate() {
            let p = self.ee.r32(0x349d5c + region * 12 + 4 * k as u32);
            *part = String::from_utf8(self.cstr(p)).unwrap();
        }
        NameEnv { region, parts }
    }

    /// Builds the CRC table in the original (`FUN_0023e4c0`).
    pub fn build_crc_table(&mut self) {
        self.call(0x23e4c0, &[]);
    }
}

/// Compares two byte buffers and names the first difference.
#[track_caller]
pub fn same_bytes(game: &[u8], port: &[u8], what: &str) {
    assert_eq!(game.len(), port.len(), "{what}: lengths");
    if let Some(i) = (0..game.len()).find(|&i| game[i] != port[i]) {
        let n = (game.len() - i).min(24);
        panic!("{what}: first difference at +{i:#x}: game {:02x?} port {:02x?}", &game[i..i + n], &port[i..i + n]);
    }
}
