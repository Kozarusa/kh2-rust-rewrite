//! The memory card server `mcserv` (id 0x80000400): a headless run has no card. The version
//! query (function 0xfe) answers the module versions the game's init checks; every other function
//! answers "card not detected" (`sceMcResFailDetect`, -10) [inferred: the contract allows no card as an answer].

use super::{Mem, Request, Server};

pub const SERVER_ID: u32 = 0x8000_0400;
pub const FNO_VERSION: u32 = 0xfe;
/// mcserv 2.10 and mcman 2.30, as loaded in boot [V: 0x303240 wants at least 0x20a and above 0x20d].
pub const MCSERV_VERSION: u32 = 0x210;
pub const MCMAN_VERSION: u32 = 0x230;
pub const NO_CARD: i32 = -10;

pub struct Mcserv;

impl Server for Mcserv {
    fn ids(&self) -> &[u32] {
        &[SERVER_ID]
    }

    fn call(&mut self, _: &mut dyn Mem, req: &Request) -> Option<Vec<u8>> {
        let mut r = Vec::new();
        match req.fno {
            FNO_VERSION => {
                for w in [0, MCSERV_VERSION, MCMAN_VERSION] {
                    r.extend_from_slice(&w.to_le_bytes());
                }
            }
            1 | 0x15 => r.extend_from_slice(&NO_CARD.to_le_bytes()),
            _ => return None,
        }
        Some(r)
    }
}
