//! The boot on the Rust kernel and the Rust IOP reaches `ovl_title.x` (boot contract 1.2 and 2.4).
//! Skipped without the disc.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_hle::iop::{Disc, Mem, Request, Server, cdvd, sif, sound};
use kh2_hle::{Machine, RunStop};

struct ArchiveDisc(kh2_archive::Archive);

impl Disc for ArchiveDisc {
    fn read_sectors(&mut self, lsn: u32, count: u32) -> Option<Vec<u8>> {
        self.0.iso().read_at(lsn as u64 * 2048, count as usize * 2048).ok()
    }

    fn find(&mut self, path: &str) -> Option<(u32, u32)> {
        self.0.iso().find(path).map(|e| (e.lba, e.size))
    }
}

/// The (server, function) pairs of the boot in the order they are first called, up to the request
/// for `ovl_title.x` (HYB-boot 2.4 and 1.2). Differences from the 20 pairs of the contract's
/// table, which counts the whole traced boot:
/// * `mcserv` function 1 (the card status poll) first comes after the request, on the title
///   screen; the test below checks it in the continued run.
/// * `dbcman` functions 0x80001303 and 0x8000131a are made by libdbc's port-state code, which the
///   pad hooks replace (the data would arrive by DMA from the IOP); they never happen here.
const BOOT_CALLS: [(u32, u32); 17] = [
    (0x8000_0592, 0),           // cd init
    (0x8000_0593, 0x22),        // cd S-command: media mode
    (0x8000_0006, 0xff),        // loadfile: init
    (0x8000_0006, 0),           // loadfile: load module (nine times)
    (0x8000_059c, 0),           // cd: drive ready
    (0x8000_0597, 0),           // cd: search file
    (0x8000_0595, 0xe),         // cd N-command: status
    (0x8000_0595, 1),           // cd N-command: read
    (0x8000_0593, 4),           // cd S-command: result word
    (0x8000_1300, 0x8000_1363), // dbcman: version
    (0x8000_1300, 0x8000_1304), // dbcman: init
    (0x8000_1300, 0x8000_1301), // dbcman: set up a pad
    (0x11, 0),                  // libssl: sound driver bind
    (0x8000_0400, 0xfe),        // mcserv: version
    (0x8000_0400, 0x15),        // mcserv
    (0x8000_131f, 1),           // pad port data, second port first
    (0x8000_131e, 0),           // pad port data, first port
];

/// The ids of the first sound commands of the traced boot (HYB-boot 2.4, "first 20 in order").
const SOUND_IDS: [u32; 20] = [0x32, 9, 8, 0x10, 0x10, 0x16, 0x14, 6, 0xd, 0x14, 6, 0xd, 6, 0xb, 6, 0xb, 0x21, 0x32, 0x16, 6];

#[test]
fn the_boot_reaches_ovl_title_with_the_iop_answering_in_rust() {
    let Some(elf) = kh2_factory::exe_or_skip("iop boot") else { return };
    let Some(arc) = kh2_factory::disc("iop boot") else { return };
    let mut m = Machine::new();
    m.load_elf(&elf).expect("elf");
    m.install_usa_hooks();
    let iop = m.install_iop(Some(Box::new(ArchiveDisc(arc))));
    let title = Rc::new(RefCell::new(None::<u64>));
    let t = title.clone();
    m.ee.probes.insert(0x167ee0, move |ee: &mut Ee| {
        if ee.cstr(ee.gpr[4] as u32, 64) == "ovl_title.x" {
            *t.borrow_mut() = Some(ee.steps);
            ee.request_stop();
        }
    });
    let r = m.run(200_000_000).expect("no fault");
    assert_eq!(r, RunStop::Requested, "stopped at the overlay request");
    let steps = title.borrow().expect("0x167ee0 was called with ovl_title.x");
    // Measured: 48.3 million instructions (9 vsyncs of virtual time; the real disc takes 69).
    assert!((30_000_000..80_000_000).contains(&steps), "steps to the title request: {steps}");
    {
        let k = m.kernel.borrow();
        assert!(k.unimplemented.is_empty(), "{:?}", k.unimplemented);
        assert!(k.exception.is_none());
        assert_eq!(k.sif_regs[&sif::SIF_RPC_INIT], 1);
        assert_eq!(k.sif_regs[&sif::SIF_IOP_CMD_BUFFER], sif::IOP_CMD_BUFFER);
        assert_eq!(k.sif_regs[&sif::SIF_SMFLAG] & sif::SMFLAG_BOOT_DONE, sif::SMFLAG_BOOT_DONE);
    }
    {
        let i = iop.borrow();
        assert_eq!(i.first_calls(), BOOT_CALLS.to_vec(), "the (server, function) pairs in order");
        assert!(i.unknown.is_empty(), "{:x?}", i.unknown);
        assert!(i.binds.iter().all(|b| b.1), "every bound server is known: {:x?}", i.binds);
        assert_eq!((i.reboots, i.reboot_images.len()), (1, 1));
        assert!(i.reboot_images[0].ends_with("IOPRP300.IMG"), "{:?}", i.reboot_images);
        assert!(i.inits >= 2, "InitRpc runs before and after the reboot");
        let names: Vec<String> = i.modules.borrow().iter().map(|p| p.rsplit('\\').next().unwrap().trim_end_matches(";1").to_string()).collect();
        assert_eq!(names, ["SIO2MAN.IRX", "SIO2D.IRX", "DBCMAN.IRX", "DS2O.IRX", "DS2O.IRX", "MCMAN.IRX", "MCSERV.IRX", "LIBSD.IRX", "LIBSSL.IRX"]);
        // The sound ring decoded the same commands the traced boot sent, in the same order (17 of
        // the first 20 are out by the title request; the rest follow in the continued run).
        let ring = i.sound.borrow();
        let ids: Vec<u32> = ring.log.iter().map(|c| c.id).collect();
        assert_eq!(ids, SOUND_IDS[..17]);
        assert!(ring.log.iter().all(|c| c.flag == 1 && c.slot < sound::SLOTS));
        assert_eq!(i.pad_reads.keys().copied().collect::<Vec<_>>(), vec![0, 1], "both pad ports are read before the title");
    }
    // The run goes on through the title overlay load without a fault; the card poll comes after the request.
    m.ee.probes.remove(&0x167ee0);
    let r = m.run(100_000_000).expect("no fault");
    assert_eq!(r, RunStop::Budget);
    let k = m.kernel.borrow();
    assert!(k.unimplemented.is_empty() && k.exception.is_none());
    let i = iop.borrow();
    assert!(i.unknown.is_empty(), "{:x?}", i.unknown);
    let pairs = i.first_calls();
    assert_eq!(pairs.len(), BOOT_CALLS.len() + 1);
    assert_eq!(pairs[BOOT_CALLS.len()], (0x8000_0400, 1), "mcserv: the card status poll");
    assert!(i.pad_reads.values().all(|n| *n > 100), "the pad is read every frame: {:?}", i.pad_reads);
    let ids: Vec<u32> = i.sound.borrow().log.iter().take(SOUND_IDS.len()).map(|c| c.id).collect();
    assert_eq!(ids, SOUND_IDS);
    assert_eq!(i.sound.borrow().pending_loads(), 0, "every bank and wave load was consumed");
}

struct FlatMem(Vec<u8>);

impl Mem for FlatMem {
    fn rd(&mut self, addr: u32, n: usize) -> Vec<u8> {
        self.0[addr as usize..addr as usize + n].to_vec()
    }

    fn wr(&mut self, addr: u32, bytes: &[u8]) {
        self.0[addr as usize..addr as usize + bytes.len()].copy_from_slice(bytes);
    }
}

#[test]
fn the_cd_search_and_read_servers_return_the_discs_own_bytes() {
    let Some(mut arc) = kh2_factory::disc("cd server on the disc") else { return };
    let (lba, size) = {
        let e = arc.iso().find("KH2.IDX").expect("KH2.IDX on the disc");
        (e.lba, e.size)
    };
    let expected = arc.iso().read_at(lba as u64 * 2048, 2048).unwrap();
    let shared = Rc::new(RefCell::new(Some(Box::new(ArchiveDisc(arc)) as Box<dyn Disc>)));
    let mut cd = cdvd::Cdvd::new(shared);
    let mut mem = FlatMem(vec![0; 0x2_0000]);
    let mut send = vec![0u8; 300];
    send[0x24..0x24 + 12].copy_from_slice(b"\\KH2.IDX;1\0\0");
    send[0x124..0x128].copy_from_slice(&0x1000u32.to_le_bytes());
    let req = Request { sid: cdvd::SEARCH, fno: 0, send: &send, recv_len: 4, send_addr: 0, recv_addr: 0, nowait: false, now: 0 };
    assert_eq!(cd.call(&mut mem, &req).unwrap(), vec![1, 0, 0, 0]);
    assert_eq!((mem.r32(0x1000), mem.r32(0x1004)), (lba, size));
    let rd: Vec<u8> = [lba, 1, 0x4000, 0x100, 0, 0x8000].iter().flat_map(|w: &u32| w.to_le_bytes()).collect();
    let req = Request { sid: cdvd::NCMD, fno: 1, send: &rd, recv_len: 0, send_addr: 0, recv_addr: 0, nowait: true, now: 0 };
    cd.call(&mut mem, &req).unwrap();
    assert_eq!(&mem.0[0x4000..0x4800], &expected[..]);
}
