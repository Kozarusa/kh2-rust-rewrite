//! The `loadfile` server (id 0x80000006): module loading by path. Every load succeeds and gets
//! the next module id; the path is recorded (boot contract 2.4: nine loads in boot, paths of the
//! form `cdrom0:\NAME.IRX;1`).

use std::cell::RefCell;
use std::rc::Rc;

use super::{Mem, Request, Server};

pub const SERVER_ID: u32 = 0x8000_0006;
/// Function: load a module (send: argument length, then the path at +8 (0xfc bytes), the arguments at +0x10c).
pub const FNO_LOAD_MODULE: u32 = 0;
/// Function the game calls once after binding; its reply is compared with words of the executable at 0x37416c.
pub const FNO_INIT: u32 = 0xff;
/// Where the executable keeps the word the init reply must equal [V: 0x320bf0 compares it with the reply].
pub const INIT_WORD_ADDR: u32 = 0x37416c;
/// The first module id handed out [inferred: any positive id will do; the IOPRP300 set is already loaded].
pub const FIRST_MODULE_ID: u32 = 0x100;

pub struct LoadFile {
    modules: Rc<RefCell<Vec<String>>>,
    next: u32,
}

impl LoadFile {
    pub fn new(modules: Rc<RefCell<Vec<String>>>) -> LoadFile {
        LoadFile { modules, next: FIRST_MODULE_ID }
    }
}

impl Server for LoadFile {
    fn ids(&self) -> &[u32] {
        &[SERVER_ID]
    }

    fn call(&mut self, mem: &mut dyn Mem, req: &Request) -> Option<Vec<u8>> {
        match req.fno {
            FNO_INIT => {
                // [inferred] 0x320bf0 fails the version check unless the reply equals this word of the executable.
                Some(mem.r32(INIT_WORD_ADDR).to_le_bytes().to_vec())
            }
            FNO_LOAD_MODULE => {
                let path = req.send.get(8..8 + 0xfc).map_or(String::new(), |b| {
                    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
                    String::from_utf8_lossy(&b[..end]).into_owned()
                });
                self.modules.borrow_mut().push(path);
                let id = self.next;
                self.next += 1;
                // [V: 0x3212c8] word 0 is the function result (a module id, positive), word 1 the module's own result.
                let mut r = id.to_le_bytes().to_vec();
                r.extend_from_slice(&0u32.to_le_bytes());
                Some(r)
            }
            _ => None,
        }
    }
}
