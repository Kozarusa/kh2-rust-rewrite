//! The IOP layer on hand-made guest code and fake memory (no disc): the SIF hooks, each RPC server,
//! the sound ring decoder, the pad script and report, and `SetVSyncFlag`.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_hle::iop::cdvd::{self, Cdvd};
use kh2_hle::iop::loadfile::{self, LoadFile};
use kh2_hle::iop::mc::{self, Mcserv};
use kh2_hle::iop::pad::{self, Dbcman, PadFrame, PadScript, button};
use kh2_hle::iop::sound::{self, Ring};
use kh2_hle::iop::{Disc, Handle, Mem, Request, Server, SharedDisc, sif};
use kh2_hle::kernel::VBLANK_CYCLES;
use kh2_hle::Machine;

// ---- fake memory and disc ------------------------------------------------------------------------

struct VecMem(Vec<u8>);

impl VecMem {
    fn new() -> VecMem {
        VecMem(vec![0; 0x40_0000])
    }
}

impl Mem for VecMem {
    fn rd(&mut self, addr: u32, n: usize) -> Vec<u8> {
        self.0[addr as usize..addr as usize + n].to_vec()
    }

    fn wr(&mut self, addr: u32, bytes: &[u8]) {
        self.0[addr as usize..addr as usize + bytes.len()].copy_from_slice(bytes);
    }
}

/// A disc of 64 sectors; sector `n` is filled with the byte `n`. Files: `KH2.IDX` at sector 10, 5000 bytes.
struct FakeDisc;

impl Disc for FakeDisc {
    fn read_sectors(&mut self, lsn: u32, count: u32) -> Option<Vec<u8>> {
        if lsn + count > 64 {
            return None;
        }
        Some((lsn..lsn + count).flat_map(|s| vec![s as u8; 2048]).collect())
    }

    fn find(&mut self, path: &str) -> Option<(u32, u32)> {
        (path.eq_ignore_ascii_case("KH2.IDX")).then_some((10, 5000))
    }
}

fn disc() -> SharedDisc {
    Rc::new(RefCell::new(Some(Box::new(FakeDisc) as Box<dyn Disc>)))
}

fn req<'a>(sid: u32, fno: u32, send: &'a [u8], recv_len: usize) -> Request<'a> {
    Request { sid, fno, send, recv_len, send_addr: 0, recv_addr: 0, nowait: false, now: 0 }
}

fn words(b: &[u8]) -> Vec<u32> {
    b.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

// ---- a tiny assembler for guest code -------------------------------------------------------------

fn lui(rt: u32, imm: u32) -> u32 {
    0x3C00_0000 | rt << 16 | (imm & 0xFFFF)
}
fn ori(rt: u32, rs: u32, imm: u32) -> u32 {
    0x3400_0000 | rs << 21 | rt << 16 | (imm & 0xFFFF)
}
fn addiu(rt: u32, rs: u32, imm: i32) -> u32 {
    0x2400_0000 | rs << 21 | rt << 16 | (imm as u32 & 0xFFFF)
}
fn lw(rt: u32, off: i32, rs: u32) -> u32 {
    0x8C00_0000 | rs << 21 | rt << 16 | (off as u32 & 0xFFFF)
}
fn sw(rt: u32, off: i32, rs: u32) -> u32 {
    0xAC00_0000 | rs << 21 | rt << 16 | (off as u32 & 0xFFFF)
}
fn beq(rs: u32, rt: u32, off: i32) -> u32 {
    0x1000_0000 | rs << 21 | rt << 16 | (off as u32 & 0xFFFF)
}
fn jr(rs: u32) -> u32 {
    rs << 21 | 8
}
fn j(target: u32) -> u32 {
    0x0800_0000 | (target >> 2) & 0x03FF_FFFF
}
const SYSCALL: u32 = 0x0000_000C;
const NOP: u32 = 0;
const V0: u32 = 2;
const V1: u32 = 3;
const A0: u32 = 4;
const A1: u32 = 5;
const T0: u32 = 8;
const RA: u32 = 31;

fn li(rt: u32, v: u32) -> Vec<u32> {
    vec![lui(rt, v >> 16), ori(rt, rt, v & 0xFFFF)]
}

fn put(m: &mut Machine, at: u32, code: &[u32]) {
    for (i, w) in code.iter().enumerate() {
        m.ee.w32(at + 4 * i as u32, *w);
    }
}

const SP: u32 = 0x0060_0000;
const STOP: u32 = 0x0010_F000;

fn machine() -> (Machine, Handle) {
    let mut m = Machine::new();
    let iop = m.install_iop(Some(Box::new(FakeDisc)));
    put(&mut m, STOP, &[NOP, NOP]);
    m.ee.probes.insert(STOP, |ee: &mut Ee| ee.request_stop());
    (m, iop)
}

/// Runs `code` (placed at 0x200000) as a thread until it reaches the stop address.
fn run_code(m: &mut Machine, code: &[u32], budget: u64) -> kh2_hle::RunStop {
    put(m, 0x20_0000, code);
    let tid = {
        let mut k = m.kernel.borrow_mut();
        let tid = k.create_thread(0x20_0000, 0x0050_0000, 0x1000, 0, 1, 0, 0);
        assert_eq!(k.start_thread(tid, 0), 0);
        tid
    };
    m.run_thread(tid);
    m.run(budget).expect("no fault")
}

/// Calls a hooked SIF function from "guest" code the way the game does (arguments in registers, the ninth on the stack).
fn call_fn(m: &mut Machine, addr: u32, args: &[u64], ninth: u32) -> u64 {
    m.ee.w32(SP, ninth);
    m.ee.call(addr, args, &[], SP).expect("no fault")
}

const CLIENT: u32 = 0x0040_0000;
const SEND: u32 = 0x0041_0000;
const RECV: u32 = 0x0042_0000;

fn bind(m: &mut Machine, sid: u32) -> u64 {
    call_fn(m, sif::BIND, &[CLIENT as u64, sid as u64 | 0xffff_ffff_0000_0000, 0], 0)
}

// ---- SIF hooks -----------------------------------------------------------------------------------

#[test]
fn installing_the_iop_removes_the_register_defaults_and_init_rpc_publishes_the_iop_values() {
    let (mut m, iop) = machine();
    {
        let k = m.kernel.borrow();
        for r in [sif::SIF_SMFLAG, sif::SIF_IOP_CMD_BUFFER, sif::SIF_RPC_INIT] {
            assert!(!k.sif_regs.contains_key(&r), "register {r:#x} starts unset");
        }
    }
    call_fn(&mut m, sif::INIT_RPC, &[], 0);
    let k = m.kernel.borrow();
    assert_eq!(k.sif_regs[&sif::SIF_IOP_CMD_BUFFER], sif::IOP_CMD_BUFFER);
    assert_eq!(k.sif_regs[&sif::SIF_EE_CMD_BUFFER], sif::EE_CMD_BUFFER);
    assert_eq!(k.sif_regs[&sif::SIF_RPC_INIT], 1);
    assert!(!k.sif_regs.contains_key(&sif::SIF_SMFLAG), "init does not touch SMFLAG");
    assert_eq!(iop.borrow().inits, 1);
}

#[test]
fn the_iop_reboot_returns_one_sets_boot_done_and_clears_the_old_session() {
    let (mut m, iop) = machine();
    call_fn(&mut m, sif::INIT_RPC, &[], 0);
    m.ee.write(SEND, b"cdrom0:\\IOPRP300.IMG\0");
    let r = call_fn(&mut m, sif::IOP_REBOOT, &[SEND as u64], 0);
    assert_eq!(r as u32, 1);
    let k = m.kernel.borrow();
    assert_eq!(k.sif_regs[&sif::SIF_SMFLAG] & sif::SMFLAG_BOOT_DONE, sif::SMFLAG_BOOT_DONE);
    assert_eq!(k.sif_regs[&sif::SIF_RPC_INIT], 0, "the RPC layer must be initialised again");
    assert_eq!(k.sif_regs[&sif::SIF_IOP_CMD_BUFFER], 0);
    let i = iop.borrow();
    assert_eq!((i.reboots, i.reboot_images.as_slice()), (1, ["cdrom0:\\IOPRP300.IMG".to_string()].as_slice()));
}

#[test]
fn bind_gives_a_known_server_a_handle_and_an_unknown_one_none() {
    let (mut m, iop) = machine();
    m.ee.w32(CLIENT + sif::client::FLAGS, 1);
    assert_eq!(bind(&mut m, mc::SERVER_ID) as u32, 0);
    let h1 = m.ee.r32(CLIENT + sif::client::SERVER);
    assert_ne!(h1, 0);
    assert_eq!(m.ee.r32(CLIENT + sif::client::FLAGS), 0, "the client is idle after a bind");
    assert_eq!(bind(&mut m, 0x8000_0210) as u32, 0, "the call itself succeeds");
    assert_eq!(m.ee.r32(CLIENT + sif::client::SERVER), 0, "an unknown server stays unbound");
    assert_eq!(bind(&mut m, mc::SERVER_ID) as u32, 0);
    assert_eq!(m.ee.r32(CLIENT + sif::client::SERVER), h1, "binding again gives the same handle");
    assert_eq!(iop.borrow().binds, vec![(mc::SERVER_ID, true), (0x8000_0210, false), (mc::SERVER_ID, true)]);
}

#[test]
fn a_call_runs_the_server_writes_the_reply_logs_the_pair_and_runs_the_end_function() {
    let (mut m, iop) = machine();
    // End function: stores its argument at 0x300000.
    let mut f = li(T0, 0x30_0000);
    f.extend([sw(A0, 0, T0), jr(RA), NOP]);
    put(&mut m, 0x20_0000, &f);
    bind(&mut m, mc::SERVER_ID);
    let r = call_fn(&mut m, sif::CALL, &[CLIENT as u64, mc::FNO_VERSION as u64, 1, SEND as u64, 0x30, RECV as u64, 12, 0x20_0000], 0xCAFE);
    assert_eq!(r as u32, 0);
    assert_eq!(words(&m.ee.read::<12>(RECV)), vec![0, mc::MCSERV_VERSION, mc::MCMAN_VERSION]);
    assert_eq!(m.ee.r32(0x30_0000), 0xCAFE, "the end function got the ninth argument");
    assert_eq!(m.ee.r32(CLIENT + sif::client::END_FN), 0x20_0000);
    let i = iop.borrow();
    assert_eq!(i.calls.len(), 1);
    assert_eq!((i.calls[0].server, i.calls[0].fno, i.calls[0].nowait, i.calls[0].end), (mc::SERVER_ID, 0xfe, true, (0x20_0000, 0xCAFE)));
    drop(i);
    // The client is not busy afterwards.
    assert_eq!(call_fn(&mut m, sif::CHECK, &[CLIENT as u64], 0), 0);
}

#[test]
fn an_unknown_function_or_an_unbound_client_is_answered_and_recorded() {
    let (mut m, iop) = machine();
    // A client that was never bound: error.
    m.ee.w32(CLIENT + sif::client::SERVER, 0);
    let r = call_fn(&mut m, sif::CALL, &[CLIENT as u64, 1, 0, 0, 0, RECV as u64, 4, 0], 0);
    assert_eq!(r as u32 as i32, -1);
    // An unknown function id of a known server: zeros, recorded.
    bind(&mut m, mc::SERVER_ID);
    m.ee.write(RECV, &[0xff; 8]);
    let r = call_fn(&mut m, sif::CALL, &[CLIENT as u64, 0x77, 0, 0, 0, RECV as u64, 8, 0], 0);
    assert_eq!(r as u32, 0);
    assert_eq!(m.ee.read::<8>(RECV), [0; 8]);
    assert_eq!(iop.borrow().unknown, vec![(mc::SERVER_ID, 0x77)]);
}

// ---- loadfile ------------------------------------------------------------------------------------

#[test]
fn loadfile_answers_the_init_with_the_executables_word_and_loads_modules_with_rising_ids() {
    let mut mem = VecMem::new();
    mem.w32(loadfile::INIT_WORD_ADDR, 0x1234_5678);
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut lf = LoadFile::new(log.clone());
    let r = lf.call(&mut mem, &req(loadfile::SERVER_ID, loadfile::FNO_INIT, &[], 4)).unwrap();
    assert_eq!(words(&r), vec![0x1234_5678]);
    let mut send = vec![0u8; 0x200];
    let path = b"cdrom0:\\PAD2\\DS2O.IRX;1";
    send[8..8 + path.len()].copy_from_slice(path);
    let a = lf.call(&mut mem, &req(loadfile::SERVER_ID, loadfile::FNO_LOAD_MODULE, &send, 8)).unwrap();
    let b = lf.call(&mut mem, &req(loadfile::SERVER_ID, loadfile::FNO_LOAD_MODULE, &send, 8)).unwrap();
    assert_eq!(words(&a), vec![loadfile::FIRST_MODULE_ID, 0]);
    assert_eq!(words(&b), vec![loadfile::FIRST_MODULE_ID + 1, 0]);
    assert_eq!(log.borrow().len(), 2);
    assert!(log.borrow()[0].starts_with("cdrom0:\\PAD2\\DS2O.IRX"), "{:?}", log.borrow());
    assert!(lf.call(&mut mem, &req(loadfile::SERVER_ID, 9, &[], 4)).is_none());
}

// ---- cdvd ----------------------------------------------------------------------------------------

#[test]
fn cdvd_reads_sectors_into_the_ee_buffer_and_clears_the_error_word() {
    let mut mem = VecMem::new();
    mem.w32(0x9000, 0xdead);
    let mut cd = Cdvd::new(disc());
    let mut send = Vec::new();
    for w in [5u32, 3, 0x2000, 0x100, 0x7000, 0x9000] {
        send.extend_from_slice(&w.to_le_bytes());
    }
    let r = cd.call(&mut mem, &req(cdvd::NCMD, 1, &send, 0)).unwrap();
    assert!(r.is_empty());
    assert!(mem.0[0x2000..0x2800].iter().all(|b| *b == 5));
    assert!(mem.0[0x2800..0x3000].iter().all(|b| *b == 6));
    assert!(mem.0[0x3000..0x3800].iter().all(|b| *b == 7));
    assert_eq!(mem.0[0x3800], 0, "nothing past the third sector");
    assert_eq!(mem.r32(0x9000), 0);
    assert_eq!(cd.sectors_read, 3);
}

#[test]
fn cdvd_search_fills_the_record_and_normalises_the_name() {
    let mut mem = VecMem::new();
    let mut cd = Cdvd::new(disc());
    let mut send = vec![0u8; 300];
    send[0x24..0x24 + 12].copy_from_slice(b"\\KH2.IDX;1\0\0");
    send[0x124..0x128].copy_from_slice(&0x5000u32.to_le_bytes());
    let r = cd.call(&mut mem, &req(cdvd::SEARCH, 0, &send, 4)).unwrap();
    assert_eq!(words(&r), vec![1]);
    assert_eq!(mem.r32(0x5000), 10);
    assert_eq!(mem.r32(0x5004), 5000);
    assert_eq!(&mem.0[0x5008..0x5008 + 7], b"KH2.IDX");
    // A missing file: the reply is 0 and the record untouched.
    let mut send = vec![0u8; 300];
    send[0x24..0x24 + 9].copy_from_slice(b"NOPE.BIN\0");
    send[0x124..0x128].copy_from_slice(&0x6000u32.to_le_bytes());
    let r = cd.call(&mut mem, &req(cdvd::SEARCH, 0, &send, 4)).unwrap();
    assert_eq!(words(&r), vec![0]);
    assert_eq!(mem.r32(0x6000), 0);
    assert_eq!(cdvd::disc_path("\\A\\B.X;1"), "A/B.X");
}

#[test]
fn cdvd_status_functions_succeed_and_the_drive_reports_ready() {
    let mut mem = VecMem::new();
    let mut cd = Cdvd::new(disc());
    for (sid, fno, n) in [(cdvd::INIT, 0, 16), (cdvd::SCMD, 0x22, 4), (cdvd::SCMD, 4, 4), (cdvd::NCMD, 0xe, 4)] {
        let r = cd.call(&mut mem, &req(sid, fno, &[], n)).unwrap();
        assert_eq!(r, vec![0u8; n], "{sid:#x} {fno:#x}");
    }
    let r = cd.call(&mut mem, &req(cdvd::READY, 0, &[], 4)).unwrap();
    assert_eq!(words(&r), vec![cdvd::DRIVE_COMPLETE]);
    assert!(cd.call(&mut mem, &req(cdvd::SCMD, 0x99, &[], 4)).is_none());
    // Without a disc a read gives zeros and a search finds nothing.
    let mut none = Cdvd::new(Rc::new(RefCell::new(None)));
    mem.wr(0x2000, &[9; 16]);
    let send: Vec<u8> = [1u32, 1, 0x2000, 0, 0, 0].iter().flat_map(|w| w.to_le_bytes()).collect();
    none.call(&mut mem, &req(cdvd::NCMD, 1, &send, 0)).unwrap();
    assert!(mem.0[0x2000..0x2800].iter().all(|b| *b == 0));
}

// ---- mcserv, dbcman -----------------------------------------------------------------------------

#[test]
fn mcserv_reports_its_versions_and_no_card() {
    let mut mem = VecMem::new();
    let mut s = Mcserv;
    let v = s.call(&mut mem, &req(mc::SERVER_ID, mc::FNO_VERSION, &[], 12)).unwrap();
    assert_eq!(words(&v), vec![0, 0x210, 0x230]);
    for fno in [1, 0x15] {
        let r = s.call(&mut mem, &req(mc::SERVER_ID, fno, &[], 8)).unwrap();
        assert_eq!(words(&r)[0] as i32, mc::NO_CARD, "function {fno:#x}");
    }
    assert!(s.call(&mut mem, &req(mc::SERVER_ID, 0x33, &[], 4)).is_none());
}

#[test]
fn dbcman_answers_the_version_hands_out_sockets_and_echoes_port_data() {
    let mut mem = VecMem::new();
    let mut d = Dbcman::default();
    let send = vec![0u8; 0x90];
    let v = d.call(&mut mem, &req(pad::DBCMAN, pad::FNO_VERSION, &send, 0x90)).unwrap();
    assert_eq!(words(&v)[0] >> 4, 0x31, "libdbc wants a version whose high bits are 0x31");
    let a = d.call(&mut mem, &req(pad::DBCMAN, pad::FNO_SET_UP, &send, 0x90)).unwrap();
    let b = d.call(&mut mem, &req(pad::DBCMAN, pad::FNO_SET_UP, &send, 0x90)).unwrap();
    assert_eq!((words(&a)[9], words(&b)[9]), (0, 1), "the sockets are 0 and 1");
    let g = d.call(&mut mem, &req(pad::DBCMAN, pad::FNO_GET_DATA, &send, 0x90)).unwrap();
    let w = words(&g);
    assert_eq!((w[2], w[3] & 0xff, w[0x8c / 4] as i32), (1, 1, 0));
    let mut data = vec![0u8; 0x90];
    data[4] = 0x5a;
    let p = d.call(&mut mem, &req(pad::DBCMAN_PORT1, 1, &data, 0x90)).unwrap();
    assert_eq!(p, data, "port data is echoed");
    assert!(d.call(&mut mem, &req(pad::DBCMAN, 0x8000_1399, &send, 0x90)).is_none());
}

// ---- the pad -------------------------------------------------------------------------------------

#[test]
fn the_report_holds_the_buttons_active_low_then_the_sticks_then_the_pressures() {
    let f = PadFrame { buttons: button::CROSS | button::UP, lx: 10, ly: 20, rx: 30, ry: 40 };
    let r = f.report();
    let w = !(button::CROSS | button::UP);
    assert_eq!((r[0], r[1]), (w as u8, (w >> 8) as u8));
    assert_eq!(&r[2..6], &[30, 40, 10, 20], "right x, right y, left x, left y");
    assert_eq!(r[6 + 2], 0xff, "up is pressed");
    assert_eq!(r[6 + 6], 0xff, "cross is pressed");
    assert_eq!(r[6 + 5], 0, "circle is not");
    let n = PadFrame::default().report();
    assert_eq!(&n[..6], &[0xff, 0xff, 0x7f, 0x7f, 0x7f, 0x7f]);
    assert!(n[6..].iter().all(|b| *b == 0));
}

#[test]
fn a_pad_script_holds_each_line_until_the_next() {
    let s = PadScript::parse("# kh2 pad v1\n0 - 127 127 127 127\n40 cross+start 127 0 127 127\n120 - 255 127 127 127\nend 330\n").unwrap();
    assert_eq!(s.end, Some(330));
    assert_eq!(s.at(0), PadFrame::default());
    assert_eq!(s.at(39), PadFrame::default());
    assert_eq!(s.at(40), PadFrame { buttons: button::CROSS | button::START, ly: 0, ..PadFrame::default() });
    assert_eq!(s.at(119).buttons, button::CROSS | button::START);
    assert_eq!(s.at(500), PadFrame { lx: 255, ..PadFrame::default() });
    assert!(PadScript::parse("0 jump 1 2 3 4").is_err());
    assert!(PadScript::parse("0 - 1 2 3").is_err());
    assert_eq!(PadScript::default().at(7), PadFrame::default(), "no script: a neutral pad");
}

#[test]
fn the_pad_hook_writes_the_report_for_the_port_and_counts_the_read() {
    let (mut m, iop) = machine();
    iop.borrow_mut().pad = PadScript::constant(PadFrame { buttons: button::START, ..PadFrame::default() });
    let r = call_fn(&mut m, pad::PAD_READ, &[0, SEND as u64], 0);
    assert_eq!(r as usize, pad::REPORT_LEN);
    let b = m.ee.read::<18>(SEND);
    assert_eq!(b[0], !(button::START as u8));
    assert_eq!(b[1], 0xff);
    call_fn(&mut m, pad::PAD_READ, &[1, RECV as u64], 0);
    assert_eq!(m.ee.read::<2>(RECV), [0xff, 0xff], "port 1 has no script: nothing pressed");
    assert_eq!(iop.borrow().pad_reads.get(&0), Some(&1));
    assert_eq!(iop.borrow().pad_reads.get(&1), Some(&1));
    assert_eq!(call_fn(&mut m, pad::PAD_STATE, &[0], 0), 1, "a pad is connected");
}

// ---- the sound ring -------------------------------------------------------------------------------

fn slot_bytes(id: u32, args: [u32; 6]) -> Vec<u8> {
    let mut b = vec![0u8; 0x40];
    b[0..4].copy_from_slice(&id.to_le_bytes());
    for (i, a) in args.iter().enumerate() {
        b[4 + 4 * i..8 + 4 * i].copy_from_slice(&a.to_le_bytes());
    }
    b[0x3c..0x40].copy_from_slice(&1u32.to_le_bytes());
    b
}

const EE_RING: u32 = 0x8000;

#[test]
fn a_slot_is_decoded_into_id_arguments_and_flag() {
    let (id, args, flag) = sound::decode_slot(&slot_bytes(0x16, [1, 2, 3, 4, 5, 6]));
    assert_eq!((id, args, flag), (0x16, [1, 2, 3, 4, 5, 6], 1));
    assert_eq!(sound::decode_slot(&[0u8; 0x40]), (0, [0; 6], 0));
}

#[test]
fn the_ring_logs_each_command_with_its_frame_and_clears_the_slot() {
    let mut mem = VecMem::new();
    let mut ring = Ring::default();
    // Not bound yet: transfers are ignored.
    mem.wr(EE_RING, &slot_bytes(6, [7, 0, 0, 0, 0, 0]));
    ring.on_dma(&mut mem, &[[EE_RING, sound::IOP_RING, 0x40, 0]], 3, 100);
    assert!(ring.log.is_empty());
    ring.bind(&mut mem, 0x4000, EE_RING, 0x4800);
    // Slot 2 holds command 6; the notify packet (16 bytes) and a stray transfer are not commands.
    mem.wr(EE_RING + 0x80, &slot_bytes(6, [7, 8, 9, 0, 0, 0]));
    ring.on_dma(&mut mem, &[[EE_RING + 0x80, sound::IOP_RING + 0x80, 0x40, 0], [0x700, 0x1e640, 0x10, 0x44], [0, 0x12345, 0x40, 0]], 3, 100);
    assert_eq!(ring.log.len(), 1);
    let c = &ring.log[0];
    assert_eq!((c.vsync, c.slot, c.id, c.args, c.flag), (3, 2, 6, [7, 8, 9, 0, 0, 0], 1));
    assert_eq!(mem.r32(EE_RING + 0x80), 0, "an ordinary command is consumed at once");
    assert_eq!(ring.commands_in_frame(3).len(), 1);
    assert!(ring.commands_in_frame(4).is_empty());
}

#[test]
fn bank_and_wave_loads_are_consumed_after_the_delay() {
    let mut mem = VecMem::new();
    let mut ring = Ring::default();
    ring.bind(&mut mem, 0x4000, EE_RING, 0x4800);
    for (slot, id) in [(0u32, 0xbu32), (1, 0xf), (2, 0x10)] {
        mem.wr(EE_RING + slot * 0x40, &slot_bytes(id, [slot, 0, 0, 0, 0, 0]));
    }
    let descs: Vec<[u32; 4]> = (0..3).map(|s| [EE_RING + s * 0x40, sound::IOP_RING + s * 0x40, 0x40, 0]).collect();
    ring.on_dma(&mut mem, &descs, 1, 1000);
    assert_eq!(ring.log.iter().map(|c| c.id).collect::<Vec<_>>(), vec![0xb, 0xf, 0x10]);
    assert_eq!(mem.r32(EE_RING), 0xb, "the load is still pending");
    assert_eq!(mem.r32(EE_RING + 0x40), 0xf);
    assert_eq!(mem.r32(EE_RING + 0x80), 0, "id 0x10 is not a load");
    assert_eq!(ring.pending_loads(), 2);
    ring.tick(&mut mem, 1000 + sound::LOAD_DELAY - 1);
    assert_eq!(mem.r32(EE_RING), 0xb, "not yet");
    ring.tick(&mut mem, 1000 + sound::LOAD_DELAY);
    assert_eq!((mem.r32(EE_RING), mem.r32(EE_RING + 0x40), ring.pending_loads()), (0, 0, 0));
}

#[test]
fn libssl_binds_the_driver_and_the_dma_syscall_feeds_the_ring() {
    let (mut m, iop) = machine();
    // The bind call: mode, EE stream records, EE ring, EE status word.
    let mut send = vec![0u8; 0x100];
    for (i, w) in [1u32, 0x9000, EE_RING, 0x9800].iter().enumerate() {
        send[4 * i..4 * i + 4].copy_from_slice(&w.to_le_bytes());
    }
    m.ee.write(SEND, &send);
    m.ee.write(0x9000, &[0xaa; 0x68]);
    bind(&mut m, sound::SERVER_ID);
    call_fn(&mut m, sif::CALL, &[CLIENT as u64, 0, 0, SEND as u64, 0x100, RECV as u64, 8, 0], 0);
    assert_eq!(words(&m.ee.read::<8>(RECV)), vec![sound::IOP_RING, sound::IOP_STATUS]);
    assert!(iop.borrow().sound.borrow().bound);
    assert_eq!(m.ee.read::<0x68>(0x9000), [0u8; 0x68], "the stream records start empty");
    // Guest code: a command in slot 5, then sceSifSetDma with two descriptors (the slot, the notify packet).
    m.ee.write(EE_RING + 5 * 0x40, &slot_bytes(0x13, [1, 2, 3, 0, 0, 0]));
    let desc = [EE_RING + 5 * 0x40, sound::IOP_RING + 5 * 0x40, 0x40, 0, 0x7000, 0x1e640, 0x10, 0x44];
    for (i, w) in desc.iter().enumerate() {
        m.ee.w32(0xA000 + 4 * i as u32, *w);
    }
    let mut code = li(A0, 0xA000);
    code.extend([addiu(A1, 0, 2), addiu(V1, 0, 0x77), SYSCALL]);
    code.extend(li(T0, 0x30_0000));
    code.extend([sw(V0, 0, T0), j(STOP), NOP]);
    assert_eq!(run_code(&mut m, &code, 1000), kh2_hle::RunStop::Requested);
    assert_ne!(m.ee.r32(0x30_0000), 0, "the kernel still returns a transfer id");
    let i = iop.borrow();
    let ring = i.sound.borrow();
    assert_eq!(ring.log.len(), 1);
    assert_eq!((ring.log[0].slot, ring.log[0].id, ring.log[0].args), (5, 0x13, [1, 2, 3, 0, 0, 0]));
    drop(ring);
    drop(i);
    assert_eq!(m.ee.r32(EE_RING + 5 * 0x40), 0, "the slot is free again");
}

// ---- SetVSyncFlag ----------------------------------------------------------------------------------

#[test]
fn the_vsync_poll_sets_the_flag_and_the_csr_word_once_a_vblank_has_passed() {
    let (mut m, iop) = machine();
    let (flag, tick) = (0x30_0000u32, 0x30_0008u32);
    // The wait loop of the executable: `lw v0,0(a0); beq v0,zero,back; nop; j STOP`, with the load at the probed address.
    put(&mut m, 0x318920, &[lw(V0, 0, A0), beq(V0, 0, -2), NOP, j(STOP), NOP]);
    let mut code = li(A0, flag);
    code.extend(li(A1, tick));
    code.extend([addiu(V1, 0, 0x73), SYSCALL, j(0x318920), NOP]);
    assert_eq!(run_code(&mut m, &code, 20_000_000), kh2_hle::RunStop::Requested);
    assert_eq!(m.ee.r32(flag), 1);
    let csr = m.ee.r32(tick);
    assert_eq!(csr & 2, 2, "FINISH");
    assert_eq!((csr >> 13) & 1, 1, "FIELD follows the parity of vblank number 1");
    assert!(m.ee.steps >= VBLANK_CYCLES, "no flag before a vblank");
    assert!(iop.borrow().vsync_flag.is_some());
}
