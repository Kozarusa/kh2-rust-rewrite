//! The cdvd servers (boot contract 2.4): init 0x80000592, S-commands 0x80000593, N-commands
//! 0x80000595, file search 0x80000597 and drive ready 0x8000059c. Reads and searches are answered
//! from the user's disc through [`Disc`]; the other functions answer success.
//!
//! N-command read (function 1), request of 0x18 bytes [V: 0x2ff020]: +0 first sector, +4 sector
//! count, +8 EE buffer, +0xc read mode (3 bytes: tries, spin control, data pattern), +0x10 and
//! +0x14 EE addresses of a sector-status block and of the 4-byte error word (set to 0). The
//! data pattern is ignored: the game reads 2048-byte sectors [inferred; patterns 1 and 2 would be raw CD sectors].
//!
//! File search (function 0) [V: 0x2fe0d8]: send block of 300 bytes with the name at +0x24 and the
//! EE address of the result at +0x124; the result is 0x24 bytes (first sector, size in bytes,
//! 16-byte name, 8 date bytes, 4 bytes of padding); the reply word is 1 when found, else 0.

use super::{Mem, Request, Server, SharedDisc};

pub const INIT: u32 = 0x8000_0592;
pub const SCMD: u32 = 0x8000_0593;
pub const NCMD: u32 = 0x8000_0595;
pub const SEARCH: u32 = 0x8000_0597;
pub const READY: u32 = 0x8000_059c;

pub const SECTOR: usize = 2048;
/// The drive-ready answer for "ready" (SCECdComplete).
pub const DRIVE_COMPLETE: u32 = 2;
/// Length of the search file record.
pub const RECORD: usize = 0x24;

pub struct Cdvd {
    disc: SharedDisc,
    ids: [u32; 5],
    /// Sectors read, for tests and logs.
    pub sectors_read: u64,
}

impl Cdvd {
    pub fn new(disc: SharedDisc) -> Cdvd {
        Cdvd { disc, ids: [INIT, SCMD, NCMD, SEARCH, READY], sectors_read: 0 }
    }
}

/// A search name as a disc path: no leading separator, no version suffix, `/` separators.
pub fn disc_path(name: &str) -> String {
    let name = name.split(';').next().unwrap_or("");
    name.trim_start_matches(['\\', '/']).replace('\\', "/")
}

impl Server for Cdvd {
    fn ids(&self) -> &[u32] {
        &self.ids
    }

    fn call(&mut self, mem: &mut dyn Mem, req: &Request) -> Option<Vec<u8>> {
        let ok = Some(vec![0u8; req.recv_len]);
        match (req.sid, req.fno) {
            (INIT, 0) | (SCMD, 0x22) | (SCMD, 4) | (NCMD, 0xe) => ok,
            // The drive-ready loop at 0x1acb10 waits for the value 2 (SCECdComplete [SDK]) from function 0 [V: caller].
            (READY, 0) => Some(DRIVE_COMPLETE.to_le_bytes().iter().copied().chain(std::iter::repeat(0)).take(req.recv_len.max(4)).collect()),
            (NCMD, 1) => {
                let (lsn, count, buf, err_addr) = (req.word(0), req.word(1), req.word(2), req.word(5));
                let data = match self.disc.borrow_mut().as_mut() {
                    Some(d) => d.read_sectors(lsn, count),
                    None => None,
                };
                let data = data.unwrap_or_else(|| vec![0u8; count as usize * SECTOR]);
                mem.wr(buf, &data);
                self.sectors_read += count as u64;
                if err_addr != 0 {
                    mem.w32(err_addr, 0);
                }
                ok
            }
            (SEARCH, 0) => {
                let name_bytes = req.send.get(0x24..0x124).unwrap_or(&[]);
                let end = name_bytes.iter().position(|c| *c == 0).unwrap_or(name_bytes.len());
                let path = disc_path(&String::from_utf8_lossy(&name_bytes[..end]));
                let found = match self.disc.borrow_mut().as_mut() {
                    Some(d) => d.find(&path),
                    None => None,
                };
                if let Some((lsn, size)) = found {
                    let mut rec = vec![0u8; RECORD];
                    rec[0..4].copy_from_slice(&lsn.to_le_bytes());
                    rec[4..8].copy_from_slice(&size.to_le_bytes());
                    let leaf = path.rsplit('/').next().unwrap_or("");
                    for (i, b) in leaf.bytes().take(15).enumerate() {
                        rec[8 + i] = b;
                    }
                    let dst = req.word(0x124 / 4);
                    if dst != 0 {
                        mem.wr(dst, &rec);
                    }
                }
                let mut r = vec![0u8; req.recv_len.max(4)];
                r[0] = found.is_some() as u8;
                Some(r)
            }
            _ => None,
        }
    }
}
