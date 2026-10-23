//! Reference poses from the game's own motion evaluator, run in the kh2-ee interpreter on the
//! user's executable. Used to check kh2-anim's native rig solver joint by joint.
//!
//! OpenKH's Mlink.cs (Apache-2.0) calls the setup / curve / solve routines of another build at
//! 0x128260/0x128918/0x129a18; in the USA executable they sit 0x30 lower, under the entry point
//! below.

use kh2_ee::{Ee, Fault};

/// Motion evaluation entry: (ticks, motion, info, object matrix, look-at, foot IK). It runs
/// setup 0x128230, curves 0x1288e8, constraint activations 0x1309c8 and the rig solve 0x1299e8.
pub const FN_ENTRY: u32 = 0x0013_0AD8;

const MOTION: u32 = 0x0180_0000;
const BONES: u32 = 0x0190_0000;
const OBJECT: u32 = 0x0191_0000;
const VTABLE: u32 = 0x0191_0100;
const INFO: u32 = 0x0191_0200;
const BUF: u32 = 0x0192_0000;
const STACK: u32 = 0x01FF_0000;
const HOOK_BONES: u32 = 0x0000_1000;

pub struct Oracle {
    ee: Ee,
}

pub struct RigOut {
    /// World matrices of the model bones (16 floats, rows = axes then translation).
    pub bones: Vec<[f32; 16]>,
    /// World matrices of the IK helpers.
    pub helpers: Vec<[f32; 16]>,
    /// The evaluator's unscaled world matrices (all joints), parents for children.
    pub unscaled: Vec<[f32; 16]>,
    pub steps: u64,
}

/// The model's bone records (0x40 bytes each) inside an MDLX file.
pub fn bone_records(mdlx: &[u8]) -> Option<Vec<u8>> {
    let entries = kh2_bar::parse(mdlx).ok()?;
    let block = entries.iter().find(|e| e.kind == 4)?.data.get(0x90..)?;
    let n = u16::from_le_bytes(block.get(0x10..0x12)?.try_into().ok()?) as usize;
    let off = u32::from_le_bytes(block.get(0x14..0x18)?.try_into().ok()?) as usize;
    Some(block.get(off..off + 64 * n)?.to_vec())
}

/// The raw motion file (BAR type 9) of an MSET slot.
pub fn motion_file(mset: &[u8], slot: usize) -> Option<Vec<u8>> {
    let entries = kh2_bar::parse(mset).ok()?;
    let anb = kh2_bar::parse(entries.get(slot)?.data).ok()?;
    Some(anb.iter().find(|e| e.kind == 9)?.data.to_vec())
}

fn vt_bones(e: &mut Ee) {
    let obj = e.gpr[kh2_ee::reg::A0] as u32;
    let b = e.r32(obj + 4);
    e.gpr[kh2_ee::reg::V0] = b as u128;
}

fn probe_limit(e: &mut Ee) {
    use kh2_ee::reg::A0;
    let (lim, origin, value) = (e.gpr[A0 + 1] as u32, e.gpr[A0 + 2] as u32, e.gpr[A0 + 3] as u32);
    let v = |e: &mut Ee, a: u32| -> [f32; 4] { std::array::from_fn(|i| e.f32_at(a + 4 * i as u32)) };
    let (o, x) = (v(e, origin), v(e, value));
    let flags = e.r32(lim);
    eprintln!("limit @{lim:08x} flags {flags:#x} origin {o:?} value {x:?} ra {:08x}", e.gpr[31] as u32);
}

fn fault(f: Fault, what: &str) -> String {
    match f {
        Fault::Unimplemented { pc, word, what: kind } => format!("{what}: unimplemented {kind} {word:08x} at {pc:08x}"),
        Fault::Memory { pc, addr } => format!("{what}: bad address {addr:08x} at {pc:08x}"),
        Fault::Runaway { pc } => format!("{what}: runaway at {pc:08x}"),
        other => format!("{what}: {other:?}"),
    }
}

impl Oracle {
    pub fn new(elf: &[u8]) -> Result<Oracle, String> {
        let mut ee = Ee::new();
        ee.load_elf(elf).ok_or("not an ELF")?;
        ee.hooks.insert(HOOK_BONES, vt_bones);
        if std::env::var("KH2_ORACLE_LIMITS").is_ok() {
            ee.probes.insert(0x0012_7DF0, probe_limit);
        }
        // Static constructor that fills the shared identity matrix (run at boot by the game).
        ee.call(0x0011_BD58, &[1, 0xFFFF], &[], STACK).map_err(|f| fault(f, "identity ctor"))?;
        Ok(Oracle { ee })
    }

    /// Builds a rotation matrix with the game's own Euler routine (x, then y, then z).
    pub fn rotation(&mut self, angles: [f32; 3]) -> Result<[f32; 16], String> {
        let e = &mut self.ee;
        let (m, a) = (BUF, BUF + 0x100);
        let id: [f32; 16] = std::array::from_fn(|i| if i % 5 == 0 { 1.0 } else { 0.0 });
        for (i, v) in id.iter().enumerate() {
            e.wf32(m + 4 * i as u32, *v);
        }
        for (i, v) in angles.iter().enumerate() {
            e.wf32(a + 4 * i as u32, *v);
        }
        e.call(0x0011_B568, &[m as u64, a as u64], &[], STACK).map_err(|f| fault(f, "rotation"))?;
        Ok(std::array::from_fn(|i| e.f32_at(m + 4 * i as u32)))
    }

    pub fn pose(&mut self,bones: &[u8], motion: &[u8], frame: f32) -> Result<RigOut, String> {
        let e = &mut self.ee;
        let n = bones.len() / 64;
        let g = |o: usize| u32::from_le_bytes(motion[0x90 + o..0x94 + o].try_into().unwrap());
        let total = (g(0x10) >> 16) as usize;
        let total = total.max(n);
        // Fresh memory for everything we own.
        let span = (BUF - MOTION) as usize + 0x0001_0000 * 9;
        let base = MOTION as usize;
        e.ram[base..base + span].fill(0);
        e.write(MOTION, motion);
        e.write(BONES, bones);
        e.w32(OBJECT, VTABLE);
        e.w32(OBJECT + 4, BONES);
        e.w32(VTABLE + 0x20, HOOK_BONES);
        let buf = |k: u32| BUF + k * 0x0001_0000;
        // Model side: S, R, T (16 bytes per bone), local and world matrices.
        let (s, r, t, local, world) = (buf(0), buf(1), buf(2), buf(3), buf(4));
        e.w32(INFO + 0x14, OBJECT);
        e.w32(INFO + 0x1C, s);
        e.w32(INFO + 0x20, r);
        e.w32(INFO + 0x24, t);
        e.w32(INFO + 0x28, local);
        e.w32(INFO + 0x2C, world);
        // Motion side: header pointer, then per-joint work buffers.
        let (m8, mc, m10, m14) = (buf(5), buf(6), buf(7), buf(8));
        e.w32(MOTION + 4, MOTION + 0x90);
        e.w32(MOTION + 8, m8);
        e.w32(MOTION + 0xC, mc);
        e.w32(MOTION + 0x10, m10);
        e.w32(MOTION + 0x14, m14);
        if let Ok(w) = std::env::var("KH2_ORACLE_WATCH") {
            // offset into the bone-local matrix buffer, e.g. 0 for bone 0
            let o = u32::from_str_radix(w.trim_start_matches("0x"), 16).unwrap_or(0);
            e.watch = Some((local + o, local + o + 0x40));
        }
        // Identity object matrix for the evaluator's world-space extras (look-at, foot IK).
        let objm = BUF + 0x000F_0000;
        for i in 0..16u32 {
            e.wf32(objm + 4 * i, if i % 5 == 0 { 1.0 } else { 0.0 });
        }
        let fstart = f32::from_bits(g(0x80 + 0x10));
        let fps = f32::from_bits(g(0x88 + 0x10)).max(1.0);
        // The entry point takes ticks elapsed since the motion's first frame.
        let elapsed = (frame - fstart) * 60.0 / fps;
        let start = e.steps;
        e.call(FN_ENTRY, &[MOTION as u64, INFO as u64, objm as u64, 0, 0], &[elapsed], STACK).map_err(|f| fault(f, "motion"))?;
        if std::env::var("KH2_ORACLE_DEBUG").is_ok() {
            for (name, at, len) in [("S", s, 16 * n), ("R", r, 16 * n), ("T", t, 16 * n), ("local", local, 64 * n), ("world", world, 64 * n), ("m8", m8, 64 * total), ("mc", mc, 64 * total), ("m10", m10, 64 * total), ("m14", m14, 16 * total)] {
                let nz = e.ram[at as usize..at as usize + len].chunks(4).filter(|c| c.iter().any(|&b| b != 0)).count();
                eprintln!("{name:6} {at:08x}: {nz} nonzero words of {}", len / 4);
            }
            for (name, at, stride) in [("S", s, 16), ("R", r, 16), ("T", t, 16), ("local", local, 64), ("world", world, 64), ("m14", m14, 16)] {
                for k in [0u32, 1, 2] {
                    let v: Vec<String> = (0..stride / 4).map(|i| format!("{:.3}", e.f32_at(at + k * stride + 4 * i))).collect();
                    eprintln!("  {name} [{k}] {}", v.join(" "));
                }
            }
        }
        if let Ok(js) = std::env::var("KH2_ORACLE_W8") {
            for j in js.split(',').filter_map(|s| s.parse::<u32>().ok()) {
                let v: Vec<String> = (0..16).map(|i| format!("{:.3}", e.f32_at(m8 + 64 * j + 4 * i))).collect();
                eprintln!("  game W8[{j}] {}", v.join(" "));
            }
        }
        let read = |e: &mut Ee, at: u32, k: usize| -> [f32; 16] { std::array::from_fn(|i| e.f32_at(at + 64 * k as u32 + 4 * i as u32)) };
        let bones = (0..n).map(|k| read(e, world, k)).collect();
        let helpers = (0..total - n).map(|k| read(e, m10, k)).collect();
        let unscaled = (0..total).map(|k| read(e, m8, k)).collect();
        Ok(RigOut { bones, helpers, unscaled, steps: e.steps - start })
    }
}
