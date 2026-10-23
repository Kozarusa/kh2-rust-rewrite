//! The SIF RPC client functions of the executable, replaced by hooks (boot contract 2.4 and 5,
//! decision D1). Addresses are those of `SLUS_210.05`.
//!
//! The client structure the game passes in is the SDK's `SifRpcClientData` [SDK; fields confirmed
//! by the executable's bind and call functions]: +0x00 packet pointer, +0x04 packet id, +0x08
//! semaphore, +0x10 flags (bit 0 = a call is outstanding), +0x14 and +0x18 the server's buffers,
//! +0x1c end function, +0x20 its argument, +0x24 the server (non-zero once bound).
//!
//! * `sceSifInitRpc` 0x31b8b0: marks the RPC layer ready and publishes the IOP-driven SIF
//!   registers; the sifcmd internals it would run never run.
//! * IOP reboot 0x321a20: the IOP "restarts" at once: the registers of the old session are cleared
//!   (as 0x321850 does) and SMFLAG bit 0x40000 (boot done) is set, so the unmodified sync
//!   function 0x3219d0 returns true. That function is left running on purpose: it only reads
//!   SMFLAG and runs the game's reboot-notify list.
//! * `sceSifBindRpc` 0x31c058: a known server id gives the client a server handle (+0x24); an
//!   unknown id leaves it 0 and is logged.
//! * `sceSifCallRpc` 0x31c238: the server runs at once and writes its reply; the end function, if
//!   any, runs at once with interrupt rules (no thread switch inside it) and the client is idle.
//!   [inferred: completing a cd read in zero time; HYB-boot 5 suggests a fixed number of vsyncs.]
//! * `sceSifCheckStatRpc` 0x31c438: never busy.

use kh2_ee::{Ee, reg};

use super::{CallRecord, Handle, Mem, Request};
use crate::kernel::Shared;

pub const INIT_RPC: u32 = 0x31b8b0;
pub const IOP_REBOOT: u32 = 0x321a20;
pub const BIND: u32 = 0x31c058;
pub const CALL: u32 = 0x31c238;
pub const CHECK: u32 = 0x31c438;

/// SIF register numbers (the `sceSifSetReg` / `sceSifGetReg` index).
pub const SIF_SMFLAG: u32 = 4;
pub const SIF_IOP_CMD_BUFFER: u32 = 0x8000_0000;
pub const SIF_EE_CMD_BUFFER: u32 = 0x8000_0001;
pub const SIF_RPC_INIT: u32 = 0x8000_0002;

/// SMFLAG bit set when the IOP finished booting [V: polled by 0x3219d0].
pub const SMFLAG_BOOT_DONE: u32 = 0x40000;
/// The IOP command buffer address the IOP publishes [V: 0x1e640 in PCSX2].
pub const IOP_CMD_BUFFER: u32 = 0x1e640;
/// The EE command buffer the EE publishes [V: written by 0x31b150].
pub const EE_CMD_BUFFER: u32 = 0x01e4_0218;

/// Server handles are `HANDLE_BASE + 0x100 * position in Iop::tokens` [inferred: the real value is an IOP address].
pub const HANDLE_BASE: u32 = 0x0010_0000;

/// Offsets inside the client structure.
pub mod client {
    pub const PACKET: u32 = 0x00;
    pub const SEMA: u32 = 0x08;
    pub const FLAGS: u32 = 0x10;
    pub const BUFF: u32 = 0x14;
    pub const CBUFF: u32 = 0x18;
    pub const END_FN: u32 = 0x1c;
    pub const END_ARG: u32 = 0x20;
    pub const SERVER: u32 = 0x24;
}

fn arg(ee: &Ee, i: usize) -> u32 {
    let r = if i < 4 { reg::A0 + i } else { reg::T0 + i - 4 };
    ee.gpr[r] as u32
}

fn set_v0(ee: &mut Ee, v: i32) {
    ee.gpr[reg::V0] = (ee.gpr[reg::V0] & !(u64::MAX as u128)) | (v as i64 as u64) as u128;
}

/// Returns from the hooked function to its caller.
fn ret(ee: &mut Ee) {
    let ra = ee.gpr[reg::RA] as u32;
    ee.jump(ra);
}

pub(super) fn install(ee: &mut Ee, kernel: &Shared, iop: &Handle) {
    let (k, h) = (kernel.clone(), iop.clone());
    ee.hooks.insert(INIT_RPC, move |ee: &mut Ee| {
        ret(ee);
        let mut iop = h.borrow_mut();
        iop.inits += 1;
        let mut k = k.borrow_mut();
        k.sif_regs.insert(SIF_IOP_CMD_BUFFER, IOP_CMD_BUFFER);
        k.sif_regs.insert(SIF_EE_CMD_BUFFER, EE_CMD_BUFFER);
        k.sif_regs.insert(SIF_RPC_INIT, 1);
    });

    let (k, h) = (kernel.clone(), iop.clone());
    ee.hooks.insert(IOP_REBOOT, move |ee: &mut Ee| {
        ret(ee);
        let image = ee.cstr(arg(ee, 0), 80);
        let mut iop = h.borrow_mut();
        iop.reboots += 1;
        iop.reboot_images.push(image);
        let mut k = k.borrow_mut();
        // What 0x321850 does to the old session, then the new IOP's boot-done flag.
        k.sif_regs.insert(SIF_RPC_INIT, 0);
        k.sif_regs.insert(SIF_IOP_CMD_BUFFER, 0);
        let smflag = k.sif_regs.get(&SIF_SMFLAG).copied().unwrap_or(0);
        k.sif_regs.insert(SIF_SMFLAG, smflag | SMFLAG_BOOT_DONE);
        set_v0(ee, 1);
    });

    let h = iop.clone();
    ee.hooks.insert(BIND, move |ee: &mut Ee| {
        ret(ee);
        let (c, sid) = (arg(ee, 0), arg(ee, 1));
        let mut iop = h.borrow_mut();
        iop.tick(ee);
        let known = iop.servers.iter().any(|s| s.ids().contains(&sid));
        iop.binds.push((sid, known));
        let handle = if known {
            let at = match iop.tokens.iter().position(|t| *t == sid) {
                Some(i) => i,
                None => {
                    iop.tokens.push(sid);
                    iop.tokens.len() - 1
                }
            };
            HANDLE_BASE + 0x100 * at as u32
        } else {
            0
        };
        ee.w32(c + client::PACKET, 0);
        ee.w32(c + client::SEMA, u32::MAX);
        ee.w32(c + client::FLAGS, 0);
        ee.w32(c + client::BUFF, 0);
        ee.w32(c + client::CBUFF, 0);
        ee.w32(c + client::SERVER, handle);
        set_v0(ee, 0);
    });

    let (k, h) = (kernel.clone(), iop.clone());
    ee.hooks.insert(CALL, move |ee: &mut Ee| {
        ret(ee);
        let (c, fno, mode) = (arg(ee, 0), arg(ee, 1), arg(ee, 2));
        let (send, ssize, recv, rsize) = (arg(ee, 3), arg(ee, 4) as usize, arg(ee, 5), arg(ee, 6) as usize);
        let end_fn = arg(ee, 7);
        // The ninth argument sits at the top of the caller's stack [inferred: EE calling convention].
        let sp = ee.gpr[reg::SP] as u32;
        let end_arg = ee.r32(sp);
        let mut iop = h.borrow_mut();
        iop.tick(ee);
        let handle = ee.r32(c + client::SERVER);
        let sid = handle.checked_sub(HANDLE_BASE).and_then(|o| iop.tokens.get((o >> 8) as usize).copied());
        let Some(sid) = sid else {
            set_v0(ee, -1);
            return;
        };
        let sendbuf = if send != 0 && ssize > 0 { ee.rd(send, ssize.min(0x1_0000)) } else { Vec::new() };
        let vsync = iop.vsync(ee.steps);
        let now = k.borrow().clock.now(ee.steps);
        let req = Request { sid, fno, send: &sendbuf, recv_len: rsize, send_addr: send, recv_addr: recv, nowait: mode & 1 != 0, now };
        let mut reply = None;
        for s in iop.servers.iter_mut() {
            if s.ids().contains(&sid) {
                reply = Some(s.call(ee, &req));
                break;
            }
        }
        let mut data = match reply {
            Some(Some(d)) => d,
            _ => {
                iop.unknown.push((sid, fno));
                Vec::new()
            }
        };
        data.resize(rsize, 0);
        if recv != 0 && rsize > 0 {
            ee.write(recv, &data);
        }
        ee.w32(c + client::FLAGS, 0);
        ee.w32(c + client::END_FN, end_fn);
        ee.w32(c + client::END_ARG, end_arg);
        iop.calls.push(CallRecord { server: sid, fno, vsync, nowait: mode & 1 != 0, end: (end_fn, end_arg) });
        drop(iop);
        set_v0(ee, 0);
        if end_fn != 0 {
            // The end function runs as if from the SIF interrupt: no thread switch inside.
            let saved = ee.context();
            k.borrow_mut().in_isr += 1;
            let r = ee.call(end_fn, &[end_arg as u64], &[], sp.wrapping_sub(0x400));
            k.borrow_mut().in_isr -= 1;
            ee.set_context(&saved);
            set_v0(ee, 0);
            let mut k = k.borrow_mut();
            if let Err(f) = r {
                k.fault = Some(f);
                ee.request_stop();
            }
            k.reschedule(ee);
        }
    });

    let h = iop.clone();
    ee.hooks.insert(CHECK, move |ee: &mut Ee| {
        ret(ee);
        h.borrow_mut().tick(ee);
        set_v0(ee, 0);
    });
}
