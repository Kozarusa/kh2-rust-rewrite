//! The IOP side answered in Rust (boot contract sections 2.4 and 5, decision D1): the SIF RPC client
//! functions of the executable are replaced by hooks that run a server written here, the sound
//! command ring is decoded from the raw `sceSifSetDma` syscall, and the pad read is replaced by a
//! pad script.
//!
//! * [`sif`]: the hooks (IOP reboot, `InitRpc`, bind, call, check status) and the SIF registers the
//!   IOP drives (SMFLAG, the command buffer address, the RPC-initialised word).
//! * [`Server`]: one RPC server, called with the function id, the send block and the size of the
//!   receive block. The servers are `loadfile`, the five cdvd ids (reads and file search through a
//!   [`Disc`]), `dbcman`/`ds2o` (pad library setup), `mcserv` ("no card") and `libssl` (the sound
//!   driver's bind).
//! * [`sound`]: the 512-slot command ring the game copies to the IOP by DMA: each slot is decoded,
//!   logged with the frame number, and cleared in EE memory the way the IOP consumes it.
//! * [`pad`]: a per-frame pad script and the hook of the libpad2 read.
//!
//! Anything the contract does not describe is answered with success (a zero reply) and recorded in
//! [`Iop::unknown`], as HYB-boot section 10 says.

pub mod cdvd;
pub mod loadfile;
pub mod mc;
pub mod pad;
pub mod sif;
pub mod sound;
pub mod vsync;

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::Ee;

use crate::kernel::{Machine, Shared, VBLANK_CYCLES};

/// Access to the EE's memory for the servers (the interpreter, or a byte array in unit tests).
pub trait Mem {
    fn rd(&mut self, addr: u32, n: usize) -> Vec<u8>;
    fn wr(&mut self, addr: u32, bytes: &[u8]);

    fn r32(&mut self, addr: u32) -> u32 {
        let b = self.rd(addr, 4);
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }

    fn w32(&mut self, addr: u32, v: u32) {
        self.wr(addr, &v.to_le_bytes());
    }

    /// A NUL-terminated string of at most `max` bytes.
    fn cstr(&mut self, addr: u32, max: usize) -> String {
        let b = self.rd(addr, max);
        let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
        String::from_utf8_lossy(&b[..end]).into_owned()
    }
}

impl Mem for Ee {
    fn rd(&mut self, addr: u32, n: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(n);
        let mut a = addr;
        while out.len() + 16 <= n {
            out.extend_from_slice(&self.read::<16>(a));
            a = a.wrapping_add(16);
        }
        while out.len() < n {
            out.push(self.r8(a));
            a = a.wrapping_add(1);
        }
        out
    }

    fn wr(&mut self, addr: u32, bytes: &[u8]) {
        self.write(addr, bytes);
    }
}

/// The user's disc as the cdvd servers see it.
pub trait Disc {
    /// `count` sectors of 2048 bytes from logical sector `lsn`, or `None` past the end of the image.
    fn read_sectors(&mut self, lsn: u32, count: u32) -> Option<Vec<u8>>;
    /// The (first sector, size in bytes) of a file; the name is a disc path with `/` separators and no version suffix.
    fn find(&mut self, path: &str) -> Option<(u32, u32)>;
}

/// The disc, shared by the cdvd servers.
pub type SharedDisc = Rc<RefCell<Option<Box<dyn Disc>>>>;

/// One RPC call as the server sees it.
pub struct Request<'a> {
    /// The server id the caller bound (a server may answer several).
    pub sid: u32,
    pub fno: u32,
    /// The send block (`ssize` bytes read from the EE).
    pub send: &'a [u8],
    /// The size of the receive block the caller gave.
    pub recv_len: usize,
    /// The EE addresses of the send and receive blocks (some functions carry pointers inside the block).
    pub send_addr: u32,
    pub recv_addr: u32,
    pub nowait: bool,
    /// Virtual time in EE cycles.
    pub now: u64,
}

impl Request<'_> {
    pub fn word(&self, i: usize) -> u32 {
        self.send.get(4 * i..4 * i + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// An RPC server (a module of the IOP).
pub trait Server {
    /// The server id the EE binds to.
    fn ids(&self) -> &[u32];
    /// Answers a call: the bytes for the receive block (shorter is padded with zeros, longer cut).
    /// `None` means the function id is unknown; the dispatcher then answers with zeros and records it.
    fn call(&mut self, mem: &mut dyn Mem, req: &Request) -> Option<Vec<u8>>;
}

/// One RPC call, as logged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallRecord {
    pub server: u32,
    pub fno: u32,
    /// The vsync (frame of the virtual clock) the call was made in.
    pub vsync: u64,
    pub nowait: bool,
    /// Address of the end function and its argument (0 when none).
    pub end: (u32, u32),
}

/// Where the IOP answers: the state of every server and the logs.
pub struct Iop {
    pub servers: Vec<Box<dyn Server>>,
    pub kernel: Shared,
    pub disc: SharedDisc,
    /// Every RPC call, in order.
    pub calls: Vec<CallRecord>,
    /// The server ids that were bound, in order; the position is the server handle a client holds.
    pub tokens: Vec<u32>,
    /// Every bind, in order (server id, known).
    pub binds: Vec<(u32, bool)>,
    /// (server, function) pairs no server knew; they were answered with zeros.
    pub unknown: Vec<(u32, u32)>,
    /// Paths of the modules the loadfile server was asked to load, in order.
    pub modules: Rc<RefCell<Vec<String>>>,
    /// The image names the IOP was rebooted with.
    pub reboot_images: Vec<String>,
    pub sound: Rc<RefCell<sound::Ring>>,
    pub pad: pad::PadScript,
    /// Times the pad read hook ran, by port number.
    pub pad_reads: std::collections::BTreeMap<u32, u64>,
    /// Times the IOP was rebooted.
    pub reboots: u32,
    /// The pointers of the last `SetVSyncFlag`.
    pub vsync_flag: Option<vsync::VsyncFlag>,
    /// Times `InitRpc` ran.
    pub inits: u32,
    /// Binds and module loads with the step count they happened at (for the boot order).
    pub timeline: Vec<(u64, crate::bringup::BootEvent)>,
}

/// The IOP state shared by the hooks.
pub type Handle = Rc<RefCell<Iop>>;

impl Iop {
    /// The vsync number for a step count.
    pub fn vsync(&self, steps: u64) -> u64 {
        self.kernel.borrow().clock.now(steps) / VBLANK_CYCLES
    }

    /// Delayed work that has come due: the slots of loads the sound driver finished.
    pub fn tick(&mut self, ee: &mut Ee) {
        let now = self.kernel.borrow().clock.now(ee.steps);
        self.sound.borrow_mut().tick(ee, now);
    }

    /// Distinct (server, function) pairs in the order they were first called.
    pub fn first_calls(&self) -> Vec<(u32, u32)> {
        let mut out: Vec<(u32, u32)> = Vec::new();
        for c in &self.calls {
            if !out.contains(&(c.server, c.fno)) {
                out.push((c.server, c.fno));
            }
        }
        out
    }
}

impl Machine {
    /// Installs the IOP: the SIF hooks, the sound ring on the DMA syscall and the pad hooks.
    /// Call after [`Machine::install_usa_hooks`]. `disc` is the user's disc (without one the
    /// cdvd servers find no file and read zeros).
    pub fn install_iop(&mut self, disc: Option<Box<dyn Disc>>) -> Handle {
        let shared: SharedDisc = Rc::new(RefCell::new(disc));
        let modules: Rc<RefCell<Vec<String>>> = Rc::default();
        let ring: Rc<RefCell<sound::Ring>> = Rc::default();
        let servers: Vec<Box<dyn Server>> = vec![
            Box::new(loadfile::LoadFile::new(modules.clone())),
            Box::new(cdvd::Cdvd::new(shared.clone())),
            Box::new(pad::Dbcman::default()),
            Box::new(mc::Mcserv),
            Box::new(sound::Libssl::new(ring.clone())),
        ];
        let iop = Rc::new(RefCell::new(Iop {
            servers,
            kernel: self.kernel.clone(),
            disc: shared,
            calls: Vec::new(),
            tokens: Vec::new(),
            binds: Vec::new(),
            unknown: Vec::new(),
            modules: modules.clone(),
            reboot_images: Vec::new(),
            sound: ring,
            pad: pad::PadScript::default(),
            pad_reads: Default::default(),
            reboots: 0,
            vsync_flag: None,
            inits: 0,
            timeline: Vec::new(),
        }));
        {
            // The IOP drives these registers; HYB-3's defaults for a finished boot are gone.
            let mut k = self.kernel.borrow_mut();
            k.sif_regs.remove(&4);
            k.sif_regs.remove(&0x8000_0000);
            k.sif_regs.remove(&0x8000_0002);
        }
        sif::install(&mut self.ee, &self.kernel, &iop);
        sound::install(&mut self.ee, &iop);
        pad::install(&mut self.ee, &iop);
        vsync::install(&mut self.ee, &iop);
        iop
    }
}
