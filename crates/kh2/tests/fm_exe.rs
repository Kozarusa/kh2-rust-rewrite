//! Final Mix executable checks behind `docs/final-mix.md`: the rules the doc states for the Final
//! Mix player controls (Square: guard, dodge roll, Quick Run) and the executable facts of
//! section 5, read from the instruction words of the user's two executables at run time.
//! Read-only; nothing from either disc is stored. Skips with a note unless both disc images are
//! configured: the USA image (`KH2_ISO` or `local/iso.txt`) and the Final Mix image (`KH2_ISO_FM`
//! or `local/iso_fm.txt`). The Final Mix disc is opened with the explicit opt-in
//! (`Archive::open_edition(.., true)`).
//!
//! Run: `$env:KH2_ISO_FM = '<path to the Final Mix image>'; cargo test -p kh2 --test fm_exe -- --nocapture`

use kh2_archive::Archive;

fn path(var: &str, file: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .or_else(|| std::fs::read_to_string(format!("../../local/{file}")).ok().map(|s| s.trim().to_string()))
        .filter(|p| std::path::Path::new(p).exists())
}

/// (USA executable, Final Mix executable), or `None` (skip) when an image is not configured.
fn executables() -> Option<(Vec<u8>, Vec<u8>)> {
    let (Some(usa), Some(fm)) = (path("KH2_ISO", "iso.txt"), path("KH2_ISO_FM", "iso_fm.txt")) else {
        eprintln!("skipped: needs both disc images (KH2_ISO and KH2_ISO_FM)");
        return None;
    };
    let mut a = Archive::open(&usa).expect("USA disc");
    let mut f = Archive::open_edition(&fm, true).expect("Final Mix disc with the opt-in");
    let boot = a.boot_executable().to_string();
    let ue = a.iso().read(&boot).expect("USA executable");
    let boot = f.boot_executable().to_string();
    let fe = f.iso().read(&boot).expect("Final Mix executable");
    Some((ue, fe))
}

/// Executable image: virtual address to bytes through the ELF program headers.
struct Elf<'a>(&'a [u8], Vec<(u32, usize, u32)>);

impl<'a> Elf<'a> {
    fn new(b: &'a [u8]) -> Elf<'a> {
        assert_eq!(&b[..4], b"\x7fELF");
        let u32_ = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let u16_ = |o: usize| u16::from_le_bytes(b[o..o + 2].try_into().unwrap()) as usize;
        let (phoff, phsize, phnum) = (u32_(0x1c) as usize, u16_(0x2a), u16_(0x2c));
        let segs = (0..phnum)
            .map(|i| phoff + i * phsize)
            .filter(|&p| u32_(p) == 1 && u32_(p + 16) > 0)
            .map(|p| (u32_(p + 8), u32_(p + 4) as usize, u32_(p + 16)))
            .collect();
        Elf(b, segs)
    }
    fn w(&self, va: u32) -> u32 {
        let &(base, off, _) = self.1.iter().find(|(b, _, s)| va >= *b && va + 4 <= *b + *s).expect("address in the image");
        let o = off + (va - base) as usize;
        u32::from_le_bytes(self.0[o..o + 4].try_into().unwrap())
    }
    fn f(&self, va: u32) -> f32 {
        f32::from_bits(self.w(va))
    }
    /// The file image of the first loaded segment (code and data) and its base address.
    fn image(&self) -> (u32, &[u8]) {
        let (base, off, size) = self.1[0];
        (base, &self.0[off..off + size as usize])
    }
    /// Targets of every `jal` in `[from, to)`, in order.
    fn calls(&self, from: u32, to: u32) -> Vec<u32> {
        (from..to)
            .step_by(4)
            .filter_map(|pc| {
                let w = self.w(pc);
                (w >> 26 == 3).then(|| ((pc + 4) & 0xf000_0000) | ((w & 0x03ff_ffff) << 2))
            })
            .collect()
    }
}

/// `addiu rt, zero, imm` (a constant load) with this immediate.
fn li(w: u32, imm: u16) -> bool {
    w >> 26 == 9 && (w >> 21) & 31 == 0 && w as u16 == imm
}

#[test]
fn fm_square_handler_tries_guard_roll_quick_run() {
    let Some((ue, fe)) = executables() else { return };
    let (u, f) = (Elf::new(&ue), Elf::new(&fe));
    // USA FUN_00177ea0: combo reset, mover-0x16 mode, Glide, keys 37/12/38, Quick Run, guard.
    assert_eq!(
        u.calls(0x177ea0, 0x177fa8),
        [0x176a70, 0x1783d8, 0x178290, 0x177fa8, 0x177fa8, 0x177fa8, 0x178108, 0x178080]
    );
    // Final Mix FUN_00178eb0 (same vtable slot, 0x34e9d8 vs USA 0x35ca28): the same chain, then
    // the guard first when the stick magnitude (+0x1c) is 0, then the dodge roll (FUN_00179530),
    // Quick Run (FUN_00179148), and the guard again.
    assert_eq!(u.w(0x35ca28), 0x177ea0);
    assert_eq!(f.w(0x34e9d8), 0x178eb0);
    assert_eq!(
        f.calls(0x178eb0, 0x178fe8),
        [0x177a28, 0x179450, 0x179308, 0x178fe8, 0x178fe8, 0x178fe8, 0x1790c0, 0x179530, 0x179148, 0x1790c0]
    );
    assert_eq!(f.w(0x178f78), 0xc601_001c, "lwc1 $f1, 0x1c(s0): the stick magnitude");
    assert_eq!(f.w(0x178f80), 0x4600_0832, "c.eq.s $f1, $f0 (against 0.0)");
    assert_eq!(f.w(0x178f84) >> 16, 0x4500, "bc1f: skip the early guard while the stick is moved");
    println!("Square (FM 0x178eb0): stick == 0 -> guard 0x1790c0; then roll 0x179530, Quick Run 0x179148, guard 0x1790c0");
}

#[test]
fn fm_dodge_roll_is_growth_slot_4_with_motion_209() {
    let Some((ue, fe)) = executables() else { return };
    let (u, f) = (Elf::new(&ue), Elf::new(&fe));
    // Growth levels (USA FUN_001c1340, FM FUN_001c35a0): 4 growth kinds in USA, 5 in FM.
    assert_eq!(u.w(0x1c138c), 0x294f_0004, "USA: slti t7, t2, 4");
    assert_eq!(f.w(0x1c35ec), 0x294f_0005, "FM: slti t7, t2, 5");
    // The roll (FM FUN_00179530): level of growth slot 4 from FUN_001c3618 (= USA FUN_001c13b8).
    let roll = f.calls(0x179530, 0x179648);
    assert_eq!(roll, [0x177860, 0x1c3618, 0x19c090, 0x1b9d40, 0x16aef0, 0x17f488, 0x17f5e0, 0x1ce300, 0x176a60]);
    assert!(li(f.w(0x179558), 4), "growth slot 4 (Dodge Roll)");
    assert!(li(f.w(0x1795e0), 209), "motion 209");
    assert_eq!(f.w(0x179610), 0xc44c_0040, "lwc1 $f12, 0x40(v0): invincibility from fmab row + 0x40");
    assert!(!roll.contains(&0x16bd60), "the roll installs no mover (no FUN_0016bd60 call)");
    // Quick Run (FM FUN_00179148 -> FUN_001791e8; USA FUN_00178108): slot 1, motion 203, mover 0x10,
    // invincibility from fmab +0x1c, as in USA; FM moved the start into a helper taking the direction.
    assert!(li(f.w(0x179214), 1) && li(f.w(0x179288), 203) && li(f.w(0x1792c0), 0x10));
    assert_eq!(f.w(0x1792d8), 0xc66c_001c, "lwc1 $f12, 0x1c(s3)");
    assert_eq!(f.calls(0x179148, 0x1791e8), [0x19c090, 0x1b9d40, 0x1791e8]);
    // USA Quick Run tests the stick itself (0.0 < +0x1c: one FPU compare); FM's Quick Run entry
    // and helper compare nothing (the Square handler's stick test comes before it).
    let compares = |e: &Elf, from: u32, to: u32| {
        (from..to).step_by(4).filter(|&pc| { let w = e.w(pc); w >> 26 == 0x11 && (w >> 21) & 31 == 0x10 && w & 0x30 == 0x30 }).count()
    };
    let (usa_cmp, fm_cmp) = (compares(&u, 0x178108, 0x178290), compares(&f, 0x179148, 0x179308));
    println!("FPU compares: USA Quick Run {usa_cmp}, FM Quick Run {fm_cmp}");
    assert_eq!((usa_cmp, fm_cmp), (1, 0));
}

#[test]
fn fm_frame_triggers_34_and_35_are_new() {
    let Some((ue, fe)) = executables() else { return };
    let (u, f) = (Elf::new(&ue), Elf::new(&fe));
    // Motion frame-trigger handler: USA 0x188498 switches over 32 types (table 0x37aa78),
    // FM 0x189740 over 36 (table 0x36caac).
    assert_eq!(u.w(0x1884b4), 0x2dcf_0020, "USA: sltiu t7, t6, 32");
    assert_eq!(f.w(0x18975c), 0x2dcf_0024, "FM: sltiu t7, t6, 36");
    let none = f.w(0x36caac + 4 * 18); // a type the handler ignores (returns at once)
    assert_eq!(none, 0x1897c0);
    assert_eq!(f.w(0x36caac + 4 * 32), none);
    assert_eq!(f.w(0x36caac + 4 * 33), none);
    let (t34, t35) = (f.w(0x36caac + 4 * 34), f.w(0x36caac + 4 * 35));
    assert_eq!((t34, t35), (0x189c78, 0x189c9c));
    // Type 34: when the command object's (+0xb44) flags (+8) have bit 4 (Square held), start
    // Quick Run along the facing vector (+0x550).
    assert_eq!(f.w(0x189c78), 0x8e2e_0b44, "lw t6, 0xb44(s1)");
    assert_eq!(f.w(0x189c80), 0x31ef_0004, "andi t7, t7, 4");
    assert_eq!(f.w(0x189c88), 0x2625_0550, "addiu a1, s1, 0x550");
    assert_eq!(f.calls(0x189c78, 0x189c9c), [0x1791e8]);
    // Type 35: when the actor is riding or (type flag 2 / +0x160 bit 0x40), play motion 4 with a
    // blend read from 0x36caa8.
    assert_eq!(f.calls(0x189c9c, 0x189d14), [0x19c090, 0x17f488]);
    assert!(li(f.w(0x189d08), 4));
    println!("FM trigger 35 blend (0x36caa8): {}", f.f(0x36caa8));
}

/// Fixed data addresses the app reads from the USA executable (`from_exe` readers), by owner.
const APP_DATA: &[(&str, u32)] = &[
    ("camera_rules rows", 0x378f04),
    ("camera_rules wall margin", 0x378e44),
    ("camera_rules overhead", 0x37939c),
    ("camera_rules event", 0x379470),
    ("camera_rules turn", 0x37a4c8),
    ("audio_bgm battle flag", 0x37ad14),
    ("player_reaction guard", 0x37a4a8),
    ("reaction_rules consts", 0x37a4a0),
    ("reaction_rules 0x37a954", 0x37a954),
    ("reaction_rules 0x37ac74", 0x37ac74),
    ("reaction_rules 0x37b408", 0x37b408),
    ("reaction_rules 0x35ce40", 0x35ce40),
    ("prize spread", 0x37b478),
    ("prize kinds", 0x37b4b0),
    ("prize pickups", 0x37b540),
    ("prize drive scale", 0x37bc04),
    ("prize physics", 0x37bf34),
    ("fx projection", 0x3780f8),
    ("hud_gauge digits", 0x358998),
    ("hud_gauge digits", 0x3589c0),
    ("hud_gauge digits", 0x3589e8),
    ("hud_menu tables", 0x3551c0),
    ("hud_menu tables", 0x355530),
    ("voice speakers (pointers)", 0x35ee10),
    ("voice language (pointers)", 0x349d44),
    ("bdx host group table (pointers)", 0x35bce8),
    ("party AI params", 0x34ea00),
];

#[test]
fn fm_executable_layout_and_app_addresses() {
    let Some((ue, fe)) = executables() else { return };
    let (u, f) = (Elf::new(&ue), Elf::new(&fe));
    println!("USA executable {} bytes, loaded image {:#x} bytes at {:#x}", ue.len(), u.1[0].2, u.1[0].0);
    println!("FM  executable {} bytes, loaded image {:#x} bytes at {:#x}", fe.len(), f.1[0].2, f.1[0].0);
    assert_eq!(u.1[0].0, f.1[0].0, "both load at the same base");
    assert_ne!(ue.len(), fe.len());
    // Same address, different contents: the USA addresses do not apply to FM.
    let (fb, fi) = f.image();
    let past_end = APP_DATA.iter().filter(|(_, a)| (*a - fb) as usize + 4 > fi.len()).count();
    let same_word = APP_DATA.iter().filter(|(_, a)| (*a - fb) as usize + 4 <= fi.len() && u.w(*a) == f.w(*a)).count();
    println!("of {} USA data addresses: {past_end} lie past the end of FM's image, {same_word} hold the same word in FM", APP_DATA.len());
    let (fbase, fimg) = f.image();
    let (mut unique, mut none) = (0, 0);
    for (what, a) in APP_DATA {
        let o = (a - u.image().0) as usize;
        let window = &u.image().1[o..o + 16];
        let hits: Vec<u32> = fimg.windows(16).enumerate().filter(|(_, w)| *w == window).map(|(i, _)| fbase + i as u32).take(3).collect();
        match hits.as_slice() {
            [h] => {
                unique += 1;
                let d = *h as i64 - *a as i64;
                println!("{what:34} USA {a:#08x} -> FM {h:#08x} (delta {}{:#x}, same 16 bytes)", if d < 0 { "-" } else { "+" }, d.unsigned_abs());
            }
            [] => {
                none += 1;
                println!("{what:34} USA {a:#08x} -> not found by bytes (pointers or changed values)");
            }
            _ => println!("{what:34} USA {a:#08x} -> ambiguous: {hits:x?}"),
        }
    }
    println!("by bytes: {unique} unique, {none} not found, {} ambiguous", APP_DATA.len() - unique - none);
    assert!(unique > APP_DATA.len() / 2, "most constant tables exist in FM at other addresses");
}
