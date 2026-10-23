//! The resident BAR: the container as the original keeps it in memory, with its helper routines (the file unit,
//! `core.file`). The header is the file's own: u32 count at +4, and at +8 the word the file leaves as padding, which the
//! loader fills with the address the file was fixed to. Entries follow at +0x10, 0x10 bytes each: u16 type, u16 flags
//! (bit 0 set: the payload is not fixed up), u32 name, u32 offset, u32 size. A size of zero means no payload.
//!
//! The port reads files as byte slices (see the parent module), so the fix-up passes exist for the oracle and for code
//! that wants the original's in-memory image: they work on a byte image `mem` whose first byte sits at guest address
//! `base`, and call `host(callee, argument)` for each payload kind that the game hands to another subsystem (the callee
//! addresses are the original's; the port does not own those subsystems yet).

/// Byte size of one entry.
pub const ENTRY: u32 = 0x10;
/// Offset of the first entry.
pub const FIRST: u32 = 0x10;

fn rd16(mem: &[u8], base: u32, a: u32) -> u16 {
    let o = a.wrapping_sub(base) as usize;
    u16::from_le_bytes(mem[o..o + 2].try_into().unwrap())
}

fn rd32(mem: &[u8], base: u32, a: u32) -> u32 {
    let o = a.wrapping_sub(base) as usize;
    u32::from_le_bytes(mem[o..o + 4].try_into().unwrap())
}

fn wr32(mem: &mut [u8], base: u32, a: u32, v: u32) {
    let o = a.wrapping_sub(base) as usize;
    mem[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

fn entry(hdr: u32, k: u32) -> u32 {
    hdr.wrapping_add(FIRST).wrapping_add(k.wrapping_mul(ENTRY))
}

/// How many entries have this type.
///
/// orig 0x168428
pub fn count_of_type(mem: &[u8], base: u32, hdr: u32, ty: u32) -> i32 {
    let n = rd32(mem, base, hdr + 4) as i32;
    let mut c = 0;
    for k in 0..n.max(0) as u32 {
        if rd16(mem, base, entry(hdr, k)) as u32 == ty {
            c += 1;
        }
    }
    c
}

/// How many entries have this name and (unless `ty` is `0xffff_ffff`) this type.
///
/// orig 0x168460
pub fn count_of_name(mem: &[u8], base: u32, hdr: u32, ty: u32, name: i32) -> i32 {
    let mut c = 0;
    let mut k = 0u32;
    while (k as i32) < rd32(mem, base, hdr + 4) as i32 {
        let e = entry(hdr, k);
        if (ty == 0xffff_ffff || rd16(mem, base, e) as u32 == ty) && rd32(mem, base, e + 4) as i32 == name {
            c += 1;
        }
        k += 1;
    }
    c
}

/// Address of the `n`-th (from 0) entry of this type (any type for `0xffff_ffff`), or `None`.
///
/// orig 0x1684c0
pub fn nth_of_type(mem: &[u8], base: u32, hdr: u32, ty: u32, mut n: i32) -> Option<u32> {
    let count = rd32(mem, base, hdr + 4) as i32;
    for k in 0..count.max(0) as u32 {
        let e = entry(hdr, k);
        if ty == 0xffff_ffff || rd16(mem, base, e) as u32 == ty {
            n = n.wrapping_sub(1);
            if n == -1 {
                return Some(e);
            }
        }
    }
    None
}

/// Address of the `n`-th (from 0) entry with this name and type (any type for `0xffff_ffff`), or `None`.
///
/// orig 0x168510
pub fn nth_of_name(mem: &[u8], base: u32, hdr: u32, ty: u32, name: i32, mut n: i32) -> Option<u32> {
    let mut k = 0u32;
    while (k as i32) < rd32(mem, base, hdr + 4) as i32 {
        let e = entry(hdr, k);
        if (ty == 0xffff_ffff || rd16(mem, base, e) as u32 == ty) && rd32(mem, base, e + 4) as i32 == name {
            n = n.wrapping_sub(1);
            if n == -1 {
                return Some(e);
            }
        }
        k += 1;
    }
    None
}

/// Payload types that are BARs themselves; the passes recurse into them.
fn nested(ty: u16) -> bool {
    matches!(ty, 0x01 | 0x11 | 0x14 | 0x1e | 0x2e)
}

/// First fix-up: turns the offsets into addresses (once; a header whose +8 word is set is left alone) and hands the
/// payloads of kinds 4, 7, 8, 9 and 0x12 to their loaders. Entries without payload get offset zero.
///
/// orig 0x168570
pub fn attach(mem: &mut [u8], base: u32, hdr: u32, host: &mut dyn FnMut(u32, u32)) {
    if rd32(mem, base, hdr + 8) != 0 {
        return;
    }
    wr32(mem, base, hdr + 8, hdr);
    let mut k = 0u32;
    while (k as i32) < rd32(mem, base, hdr + 4) as i32 {
        let e = entry(hdr, k);
        k += 1;
        if rd32(mem, base, e + 0xc) == 0 {
            wr32(mem, base, e + 8, 0);
            continue;
        }
        let a = rd32(mem, base, hdr + 8).wrapping_add(rd32(mem, base, e + 8));
        wr32(mem, base, e + 8, a);
        if rd16(mem, base, e + 2) & 1 != 0 {
            continue;
        }
        match rd16(mem, base, e) {
            t if nested(t) => attach(mem, base, a, host),
            4 => host(0x11f0a8, a),
            7 => {
                host(0x1320a0, a);
                host(0x13aa68, a);
            }
            8 => host(0x1d97a0, a),
            9 => host(0x127af0, a),
            0x12 => host(0x1dbe30, a),
            _ => {}
        }
    }
}

/// Second fix-up: moves a fixed-up image to a new address (every payload address shifts by the distance from the old
/// address in +8) and hands payload kinds 4, 7, 8, 9, 0x12 and 0x22 to their relocators.
///
/// orig 0x168690
pub fn rebase(mem: &mut [u8], base: u32, hdr: u32, host: &mut dyn FnMut(u32, u32)) {
    let old = rd32(mem, base, hdr + 8);
    wr32(mem, base, hdr + 8, hdr);
    let mut k = 0u32;
    while (k as i32) < rd32(mem, base, hdr + 4) as i32 {
        let e = entry(hdr, k);
        k += 1;
        if rd32(mem, base, e + 0xc) == 0 {
            continue;
        }
        let a = rd32(mem, base, e + 8).wrapping_add(hdr.wrapping_sub(old));
        wr32(mem, base, e + 8, a);
        if rd16(mem, base, e + 2) & 1 != 0 {
            continue;
        }
        match rd16(mem, base, e) {
            t if nested(t) => rebase(mem, base, a, host),
            4 => host(0x11f198, a),
            7 => {
                host(0x132168, a);
                host(0x13aa68, a);
            }
            8 => host(0x1d9848, a),
            9 => host(0x127b78, a),
            0x12 => host(0x1dbe50, a),
            0x22 => host(0x2c3838, a),
            _ => {}
        }
    }
}

/// Third pass: marks the header as fixed to its own address and tells the owners of payload kinds 4, 7, 9 and 0x22
/// (the argument is the payload address in the entry).
///
/// orig 0x1687b8
pub fn release(mem: &mut [u8], base: u32, hdr: u32, host: &mut dyn FnMut(u32, u32)) {
    wr32(mem, base, hdr + 8, hdr);
    let mut k = 0u32;
    while (k as i32) < rd32(mem, base, hdr + 4) as i32 {
        let e = entry(hdr, k);
        k += 1;
        if rd32(mem, base, e + 0xc) == 0 || rd16(mem, base, e + 2) & 1 != 0 {
            continue;
        }
        let a = rd32(mem, base, e + 8);
        match rd16(mem, base, e) {
            t if nested(t) => release(mem, base, a, host),
            4 => host(0x11f170, a),
            7 => host(0x132160, a),
            9 => host(0x127b50, a),
            0x22 => host(0x2c3838, a),
            _ => {}
        }
    }
}
