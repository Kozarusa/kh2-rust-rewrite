//! The card operations of the file layer (`FUN_0023d538`, `FUN_0023dad8`, `FUN_0023db48`, `FUN_0023dbb0`, `FUN_0023dbf0`,
//! `FUN_0023dc28`, `FUN_0023dc90`): each builds the name or path in the unit's buffers and queues one request on the driver's ring.
//!
//! The requests carry the addresses of the unit's buffers (name, path, file, listing table); [`Handles`] holds the values the port gives
//! them. The card port is byte 0 of the card info block.

use crate::body::file_size;
use crate::dir::NameBuf;
use crate::names::{NameEnv, name_for};
use crate::ring::Driver;
use crate::slot::CardInfo;

/// Length of the path buffer (`DAT_0036cb10`).
pub const PATH_LEN: usize = 0x12;

/// The values the buffers' addresses have in the requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handles {
    pub name: u32,
    pub path: u32,
    pub file: u32,
    pub table: u32,
}

/// The unit's state the operations work on.
#[derive(Clone, Debug)]
pub struct Ops {
    pub env: NameEnv,
    pub handles: Handles,
    pub info: CardInfo,
    pub name: NameBuf,
    pub path: [u8; PATH_LEN],
    pub driver: Driver,
}

impl Ops {
    fn port(&self) -> u32 {
        u32::from(self.info.port())
    }

    fn set_name(&mut self, selector: i64) {
        if selector != -5 {
            let cur = String::from_utf8_lossy(self.name.bytes()).into_owned();
            self.name.set(name_for(&self.env, selector, &cur).as_bytes());
        }
    }

    /// Writes `/` and, when asked, the name behind it into the path buffer (the string append copies the name with its NUL).
    fn build_path(&mut self, with_name: bool) {
        self.path[0] = b'/';
        self.path[1] = 0;
        if with_name {
            let n = self.name.bytes().to_vec();
            let room = PATH_LEN - 1;
            let k = n.len().min(room - 1);
            self.path[1..1 + k].copy_from_slice(&n[..k]);
            self.path[1 + k] = 0;
        }
    }

    /// Lists the root: sets the name for `selector` (the caller's argument: -1 the pattern with `??`, -4 `*`, or the cursor) and queues a
    /// listing of up to 99 entries into the table.
    ///
    /// orig 0x23d538
    pub fn list_root(&mut self, selector: i64) {
        self.set_name(selector);
        let (port, table, name) = (self.port(), self.handles.table, self.handles.name);
        self.driver.enqueue_list_slot0(port, table, 99, name, 0);
    }

    /// Changes into the save directory (`mode` 0: `/<name>`) or the root (`/`).
    ///
    /// orig 0x23dad8
    pub fn change_dir(&mut self, mode: i64) {
        self.build_path(mode == 0);
        let (port, path) = (self.port(), self.handles.path);
        self.driver.enqueue_chdir_slot0(port, path, 0);
    }

    /// Makes the save directory `/<name>`.
    ///
    /// orig 0x23db48
    pub fn make_dir(&mut self) {
        self.build_path(true);
        let (port, path) = (self.port(), self.handles.path);
        self.driver.enqueue_mkdir_slot0(port, path, 0);
    }

    /// Deletes the file or directory named in the name buffer.
    ///
    /// orig 0x23dbb0
    pub fn delete_name(&mut self) {
        let (port, name) = (self.port(), self.handles.name);
        self.driver.enqueue_delete_slot0(port, name, 0);
    }

    /// Creates the named file empty (a create request with no buffer and length 0).
    ///
    /// orig 0x23dbf0
    pub fn create_empty(&mut self) {
        let (port, name) = (self.port(), self.handles.name);
        self.driver.enqueue_create_slot0(port, name, 0, 0, 0);
    }

    /// Reads a file of `kind` (`file_size(kind)` bytes): `name` and `buffer` are addresses, 0 meaning the name buffer and the file buffer.
    /// [inferred: the size routine is called with the caller's first argument still in its register.]
    ///
    /// orig 0x23dc28
    pub fn read_file(&mut self, kind: i64, name: u32, buffer: u32) {
        let len = file_size(kind);
        let name = if name == 0 { self.handles.name } else { name };
        let buffer = if buffer == 0 { self.handles.file } else { buffer };
        let port = self.port();
        self.driver.enqueue_read_slot0(port, name, buffer, len, 0);
    }

    /// Writes a file of `kind`; arguments as for [`Ops::read_file`].
    ///
    /// orig 0x23dc90
    pub fn write_file(&mut self, kind: i64, name: u32, buffer: u32) {
        let len = file_size(kind);
        let name = if name == 0 { self.handles.name } else { name };
        let buffer = if buffer == 0 { self.handles.file } else { buffer };
        let port = self.port();
        self.driver.enqueue_write_slot0(port, name, buffer, len, 0);
    }
}
