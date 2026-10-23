//! The whole headless runtime (kernel, hardware pages, IOP) on the user's disc: the boot order of
//! contract 1.2, the title loop (HYB-5b) and, as an ignored long test, NEW GAME to the first field
//! frame with the player moving under the pad script (HYB-boot 1.3). Skipped without the disc.

use kh2_hle::RunStop;
use kh2_hle::bringup::{BootEvent, Hle};
use kh2_hle::iop::Disc;
use kh2_hle::iop::pad::PadScript;

struct ArchiveDisc(kh2_archive::Archive);

impl Disc for ArchiveDisc {
    fn read_sectors(&mut self, lsn: u32, count: u32) -> Option<Vec<u8>> {
        self.0.iso().read_at(lsn as u64 * 2048, count as usize * 2048).ok()
    }

    fn find(&mut self, path: &str) -> Option<(u32, u32)> {
        self.0.iso().find(path).map(|e| (e.lba, e.size))
    }
}

fn hle(what: &str) -> Option<Hle> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let arc = kh2_factory::disc(what)?;
    Some(Hle::new(&elf, Some(Box::new(ArchiveDisc(arc)))).expect("the executable loads"))
}

fn title_requested(h: &Hle) -> bool {
    h.events.borrow().iter().any(|e| e.1 == BootEvent::Overlay("ovl_title.x".into()))
}

use BootEvent::{Bind, Init, Module};

/// The order of HYB-boot 1.2: `main`'s init calls with the binds and module loads that happen
/// inside them (rows 7, 11, 12 and 16: the IOP boot binds the cd servers and loadfile and loads the
/// nine modules; game init binds the cd command servers and the pad library; the sound init binds
/// libssl; the menu systems bind mcserv).
fn traced_order() -> Vec<BootEvent> {
    let mut v = vec![Init(0x1058d0), Init(0x1024f8), Init(0x16f1d8), Init(0x2fa288), Init(0x2fbdf8), Init(0x1062f0), Init(0x166ee8)];
    v.extend([Bind(0x8000_0592), Bind(0x8000_0593), Bind(0x8000_0592), Bind(0x8000_0593), Bind(0x8000_0006)]);
    for m in ["SIO2MAN.IRX", "SIO2D.IRX", "DBCMAN.IRX", "DS2O.IRX", "DS2O.IRX", "MCMAN.IRX", "MCSERV.IRX", "LIBSD.IRX", "LIBSSL.IRX"] {
        v.push(Module(m.into()));
    }
    v.extend([Init(0x138860), Init(0x105cb0), Init(0x102df0), Init(0x1688a0)]);
    v.extend([Bind(0x8000_059c), Bind(0x8000_0597), Bind(0x8000_0595), Bind(0x8000_1300), Bind(0x8000_131c), Bind(0x8000_131e), Bind(0x8000_131f)]);
    v.extend([Init(0x101d50), Bind(0x11), Init(0x138888), Init(0x1da458), Init(0x223180), Init(0x2305b0), Bind(0x8000_0400)]);
    v.extend([0x11b198, 0x102dc8, 0x102b98, 0x103328, 0x239db8, 0x103f38, 0x1043f8, 0x2d2058, 0x105638, 0x102070].map(Init));
    v.push(BootEvent::Overlay("ovl_title.x".into()));
    v
}

#[test]
fn the_boot_makes_the_init_calls_binds_and_module_loads_in_the_traced_order() {
    let Some(mut h) = hle("hle boot order") else { return };
    let r = h.run_frames(100, 200_000_000).expect("no fault");
    assert_eq!(r, RunStop::Requested);
    assert!(title_requested(&h));
    assert_eq!(h.boot_order(), traced_order());
}

/// The per-vsync rates of the syscalls the VBlank handler and the frame end make, from HYB-boot
/// 2.1's per-field-frame column halved (two vsyncs per field frame): iSignalSema, ReferSemaStatus,
/// iReferSemaStatus, iFlushCache.
const PER_VSYNC: [(u32, f64); 4] = [(0x43, 1.0), (0x47, 1.0), (0x48, 1.0), (0x68, 2.0)];

fn rates(before: &std::collections::BTreeMap<u32, u64>, after: &std::collections::BTreeMap<u32, u64>, n: u64) -> Vec<(u32, f64)> {
    PER_VSYNC.iter().map(|(k, _)| (*k, (after.get(k).copied().unwrap_or(0) - before.get(k).copied().unwrap_or(0)) as f64 / n as f64)).collect()
}

fn assert_rates(got: &[(u32, f64)]) {
    for ((k, want), (_, g)) in PER_VSYNC.iter().zip(got) {
        assert!((g - want).abs() <= 0.1 * want, "syscall {k:#x}: {g:.3} per vsync, contract {want}");
    }
}

#[test]
fn six_hundred_title_frames_with_the_hardware_pages_stay_clean() {
    let Some(mut h) = hle("hle title loop") else { return };
    // Past the overlay load and the title's own file loads (frame 300: the memory card prompt).
    h.run_frames(300, 400_000_000).expect("no fault");
    assert!(title_requested(&h));
    let before = h.hw.machine.kernel.borrow().counts.clone();
    let (v0, s0, f0) = (h.hw.machine.kernel.borrow().clock.vsyncs, h.hw.machine.ee.steps, h.frame_count());
    let r = h.run_frames(600, 1_000_000_000).expect("no fault");
    assert_eq!(r, RunStop::Requested);
    assert_eq!(h.frame_count() - f0, 600);
    let k = h.hw.machine.kernel.borrow();
    let vsyncs = k.clock.vsyncs - v0;
    // The title runs at the frame limit 1: one vsync per frame [measured: 600 vsyncs].
    assert!((600..=610).contains(&vsyncs), "{vsyncs} vsyncs for 600 title frames");
    // Measured: about 196,000 instructions per title frame (memory card prompt of the title overlay).
    let per_frame = (h.hw.machine.ee.steps - s0) / 600;
    assert!((100_000..400_000).contains(&per_frame), "{per_frame} instructions per title frame");
    assert_rates(&rates(&before, &k.counts, vsyncs));
    assert!(k.unimplemented.is_empty(), "{:x?}", k.unimplemented);
    assert!(k.exception.is_none());
    drop(k);
    h.hw.hw.assert_clean();
    let iop = h.iop.borrow();
    assert!(iop.unknown.is_empty(), "{:x?}", iop.unknown);
    assert!(iop.pad_reads.values().all(|n| *n >= 600), "both ports read every frame: {:?}", iop.pad_reads);
}

/// NEW GAME with no memory card: Left then Cross answers the overlay's no-card prompt (0x1f194b8:
/// choice 1 continues), Cross on the title logo and on NEW GAME, then Cross every 40 vsyncs through
/// the opening events. From vsync 18400 the left stick is held up.
fn new_game_script() -> PadScript {
    let mut t = String::from("300 left 127 127 127 127\n308 - 127 127 127 127\n330 cross 127 127 127 127\n336 - 127 127 127 127\n");
    for v in (600..18000).step_by(40) {
        t.push_str(&format!("{v} cross 127 127 127 127\n{} - 127 127 127 127\n", v + 4));
    }
    t.push_str("18400 - 127 0 127 127\n18700 - 127 127 127 127\n");
    PadScript::parse(&t).expect("script")
}

/// About 11 billion instructions (two to three minutes in a release build): run with
/// `cargo test --release -p kh2-hle --test bringup -- --ignored`.
#[test]
#[ignore = "long: NEW GAME to the field is about 11 billion guest instructions"]
fn new_game_reaches_the_first_field_frame_and_the_player_moves_under_the_stick() {
    let Some(mut h) = hle("hle new game") else { return };
    h.iop.borrow_mut().pad = new_game_script();
    // Run to the first frame with a player object in Twilight Town 02-07 [measured: frame 9,4xx,
    // vsync 17,2xx].
    let mut first = None;
    while h.hw.machine.ee.steps < 12_000_000_000 {
        h.run_frames(10, 1_000_000_000).expect("no fault");
        let p = h.place();
        if (p.world, p.area) == (2, 7) && p.player.is_some() {
            first = Some((h.frame_count(), h.hw.machine.kernel.borrow().clock.vsyncs));
            break;
        }
    }
    let (frame, vsync) = first.expect("the field with a player object is reached");
    assert!(vsync < 18_000, "reached at vsync {vsync}, frame {frame}");
    assert_eq!(h.hw.movies.borrow().iter().filter(|c| c.0 == 0).count(), 1, "the opening movie was skipped by the hook");
    // Idle field frames: instructions and syscalls per frame.
    let mut idle = 0;
    while h.hw.machine.kernel.borrow().clock.vsyncs < 17_800 {
        h.run_frames(1, 100_000_000).expect("no fault");
        idle += 1;
    }
    assert!(idle > 50);
    let before = h.hw.machine.kernel.borrow().counts.clone();
    let (v0, s0) = (h.hw.machine.kernel.borrow().clock.vsyncs, h.hw.machine.ee.steps);
    h.run_frames(200, 2_000_000_000).expect("no fault");
    let vsyncs = h.hw.machine.kernel.borrow().clock.vsyncs - v0;
    assert_eq!(vsyncs, 400, "two vsyncs per field frame");
    // Measured: 1.88 million instructions per field frame (02-07, standing).
    let per_frame = (h.hw.machine.ee.steps - s0) / 200;
    assert!((1_000_000..3_000_000).contains(&per_frame), "{per_frame} instructions per field frame");
    assert_rates(&rates(&before, &h.hw.machine.kernel.borrow().counts, vsyncs));
    let start = h.place().player.expect("player");
    // The stick goes up at vsync 18400; by 18600 the player has walked [measured: about 1,300 units].
    while h.hw.machine.kernel.borrow().clock.vsyncs < 18_600 {
        h.run_frames(10, 1_000_000_000).expect("no fault");
    }
    let end = h.place().player.expect("player");
    let moved = ((end[0] - start[0]).powi(2) + (end[2] - start[2]).powi(2)).sqrt();
    assert!(moved > 300.0, "moved {moved} from {start:?} to {end:?}");
    let k = h.hw.machine.kernel.borrow();
    assert!(k.unimplemented.is_empty(), "{:x?}", k.unimplemented);
    drop(k);
    h.hw.hw.assert_clean();
    assert!(h.iop.borrow().unknown.is_empty());
}

/// HYB-7: the checkpoints on which a cold PCSX2 boot with the same pad script agreed with the HLE
/// (script `scenarios/hle/new-game-nocard.pad`; PCSX2 ran it with a memory card and the opening movie
/// played in real time, its script clock paused during the movie). Agreed exactly: the place order
/// title, 02-01-00, 02-02-00, 02-07-00; the first player position (bit-identical floats); the frame step
/// 1.0 at the title and 2.0 in the field; the position where the stick-up walk ends; the C library RNG
/// seed 0x875b2f. Not agreed (timing, not asserted): the vsync of each place change and the RNG state
/// after the seed. About 11 billion instructions: `cargo test --release -p kh2-hle --test bringup -- --ignored`.
#[test]
#[ignore = "long: NEW GAME to the field is about 11 billion guest instructions"]
fn new_game_checkpoints_agree_with_the_pcsx2_trace() {
    let Some(mut h) = hle("hle pcsx2 checkpoints") else { return };
    h.iop.borrow_mut().pad = PadScript::parse(include_str!("../../../scenarios/hle/new-game-nocard.pad")).expect("script");
    let step = |h: &mut Hle| f32::from_bits(h.hw.machine.ee.r32(0x356f40));
    let mut places: Vec<(u8, u8, u8)> = Vec::new();
    let (mut title_step, mut field_step) = (None, None);
    let mut first = None;
    let mut seed_checked = false;
    while h.hw.machine.ee.steps < 12_000_000_000 {
        h.run_frames(5, 1_000_000_000).expect("no fault");
        let p = h.place();
        if !seed_checked && h.frame_count() > 10 {
            let ptr = h.hw.machine.ee.r32(0x373cf0);
            assert_eq!(h.hw.machine.ee.r32(ptr + 0xa8), 0x0087_5b2f, "srand seed");
            seed_checked = true;
        }
        let t = (p.world, p.area, p.entrance);
        if places.last() != Some(&t) {
            places.push(t);
        }
        if t == (255, 255, 255) && title_step.is_none() && h.frame_count() > 300 {
            title_step = Some(step(&mut h));
        }
        if t == (2, 7, 0) && p.player.is_some() {
            field_step = Some(step(&mut h));
            first = p.player;
            break;
        }
    }
    assert_eq!(places, [(255, 255, 255), (2, 1, 0), (2, 2, 0), (2, 7, 0)]);
    assert_eq!(title_step, Some(1.0));
    assert_eq!(field_step, Some(2.0));
    let p = first.expect("first field frame");
    // PCSX2: bits 0xc50f1975, 0x00000000, 0xc4ca091f.
    assert_eq!([p[0].to_bits(), p[1].to_bits(), p[2].to_bits()], [0xc50f_1975, 0, 0xc4ca_091f]);
    // The stick goes up at vsync 18400 and the player walks to the same stopping point as on PCSX2.
    while h.hw.machine.kernel.borrow().clock.vsyncs < 18_700 {
        h.run_frames(10, 1_000_000_000).expect("no fault");
    }
    let e = h.place().player.expect("player");
    assert!((e[0] + 1400.29).abs() < 0.01 && (e[1] + 5.0).abs() < 0.01 && (e[2] + 2602.56).abs() < 0.01, "{e:?}");
}
