//! Oracle checks behind `docs/systems/party/contract.md`: the party rules as the doc states them, compared with the
//! game's own leaf routines run in kh2-ee on the user's executable. Read-only; skips with a note when
//! no disc image is configured (KH2_ISO or local/iso.txt).
//!
//! - FUN_001a6ff0 (the world's 16 member objects merged from `memt`) and FUN_0017cd20 (the object a
//!   party slot byte names: what `FRIEND_1` / `FRIEND_2` become).
//! - FUN_001a7330 (a party preset applied to the world's slot bytes, area-script command 0xf).
//! - FUN_001c77a8 (which members a drive form takes away).
//! - FUN_001a0be0 ("HP in danger"), FUN_0010f490 / FUN_0010f4d0 (HP / MP below a fraction).
//! - FUN_00114078 / FUN_001140c0 (the friend AI's follow-speed thresholds from the parameter
//!   table at 0x34ea00), printed per AI class.

use kh2_ee::Ee;

const STACK: u32 = 0x01ff_0000;
const STATUS: u32 = 0x0180_0000;
const OBJECT: u32 = 0x0181_0000;
const AI: u32 = 0x0182_0000;
const SLOTS: u32 = 0x0183_0000;
const MEMT: u32 = 0x0185_0000;
/// The world id byte (FUN_001a71c0 and FUN_001a6ff0 read it).
const WORLD: u32 = 0x0033_e830;
/// Per-world slot bytes in the save data, 4 per world (FUN_001a71c0).
const SAVE_SLOTS: u32 = 0x0034_0c9c;
/// Story flag bits (FUN_001802e0).
const FLAGS: u32 = 0x0033_f6b0;
/// memt pointers FUN_001a6f48 sets: the file and its preset list.
const MEMT_PTR: u32 = 0x01c9_5600;
const PRESET_PTR: u32 = 0x01c9_5604;
/// The merged record FUN_001a7118 returns; its 16 member objects start at +0x10.
const RECORD: u32 = 0x01c9_5608;
/// The friend-AI parameter table: u32 offsets from its start, one per AI class.
const AI_PARAMS: u32 = 0x0034_ea00;
/// The HP / MP fraction the AI's item rules test (FUN_00113888 reads 0x3785c8, FUN_00113b08 0x3785d4).
const HEAL_FRACTION: u32 = 0x0037_85c8;

fn open() -> Option<(Ee, Vec<u8>)> {
    let path = kh2_factory::disc_path("party_oracle")?;
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    let memt = kh2_tables::subfile(&mut a, &kh2_tables::tables::MEMT).ok()?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some((ee, memt))
}

fn clear(ee: &mut Ee, at: u32, len: u32) {
    for o in (0..len).step_by(4) {
        ee.w32(at + o, 0);
    }
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

/// memt records as 24 halfwords each.
fn records(memt: &[u8]) -> Vec<[u16; 24]> {
    let n = u32::from_le_bytes(memt[4..8].try_into().unwrap()) as usize;
    (0..n).map(|i| std::array::from_fn(|k| u16_at(memt, 8 + i * 48 + 2 * k))).collect()
}

/// The doc's story-flag rule (FUN_001802e0): bit `f & 31` of the word at
/// `FLAGS + (f >> 10) * 0x20 + ((f & 0x3ff) >> 5) * 4`; 0xffff is never set.
fn flag_addr(f: u16) -> (u32, u32) {
    let f = f as u32;
    (FLAGS + (f >> 10) * 0x20 + ((f & 0x3ff) >> 5) * 4, 1 << (f & 31))
}

/// The doc's rule for FUN_001a6ff0: records whose world word is 0 or the world, whose flag word is 0
/// or set and whose not-flag word is 0 or clear, applied in file order; non-zero member words replace.
fn merge(recs: &[[u16; 24]], world: u16, flag: &impl Fn(u16) -> bool) -> [u16; 16] {
    let mut out = [0u16; 16];
    for r in recs {
        if (r[0] == 0 || r[0] == world) && (r[1] == 0 || flag(r[1])) && (r[2] == 0 || !flag(r[2])) {
            for k in 0..16 {
                if r[8 + k] != 0 {
                    out[k] = r[8 + k];
                }
            }
        }
    }
    out
}

/// The doc's rule for FUN_0017cd20: the slot byte's low five bits index the merged member list;
/// 0x10 means no member (object 0).
fn slot_object(members: &[u16; 16], slot_byte: u8) -> u16 {
    let i = slot_byte & 0x1f;
    if i == 0x10 { 0 } else { members.get(i as usize).copied().unwrap_or(0) }
}

/// The doc's rule for FUN_001a7330: Sora's index from the preset; then the preset's members marked
/// 0x80/0x40; then the current members (slots 1..3, in order) that the preset also lists; then the
/// preset's remaining members; the rest empty (0x10). Bytes are stored masked with 0x9f.
fn apply_preset(cur: [u8; 4], preset: [u8; 4]) -> [u8; 4] {
    let mut tmp = preset;
    let mut out = [tmp[0] & 0x1f, 0x10, 0x10, 0x10];
    let mut k = 1;
    let mut put = |out: &mut [u8; 4], v: u8| {
        if k < 4 {
            out[k] = v & 0x9f;
        }
        k += 1;
    };
    for t in tmp.iter_mut().skip(1) {
        if *t & 0xc0 != 0 {
            put(&mut out, *t);
            *t = 0x10;
        }
    }
    for &c in &cur[1..] {
        let v = c & 0x1f;
        if v != 0x10
            && let Some(j) = (0..4).find(|&j| tmp[j] & 0x1f == v)
        {
            put(&mut out, v);
            tmp[j] = 0x10;
        }
    }
    for &t in &tmp[1..] {
        if t & 0x1f != 0x10 {
            put(&mut out, t);
        }
    }
    out
}

#[test]
fn member_table_merge_slots_and_presets_match_the_doc() {
    let Some((mut ee, memt)) = open() else { return };
    let recs = records(&memt);
    ee.write(MEMT, &memt);
    ee.w32(MEMT_PTR, MEMT);
    let presets_at = MEMT + 8 + recs.len() as u32 * 0x30;
    ee.w32(PRESET_PTR, presets_at);
    let named: Vec<u16> = recs.iter().flat_map(|r| [r[1], r[2]]).filter(|&f| f != 0).collect();

    // Two flag states: none set, and every flag a record names set.
    for all_set in [false, true] {
        clear(&mut ee, FLAGS, 0x800);
        if all_set {
            for &f in &named {
                let (at, bit) = flag_addr(f);
                let w = ee.r32(at);
                ee.w32(at, w | bit);
            }
        }
        let flag = |f: u16| all_set && named.contains(&f);
        for world in 0..=18u16 {
            ee.write(WORLD, &[world as u8]);
            clear(&mut ee, RECORD, 0x30);
            ee.call(0x1a6ff0, &[], &[], STACK).expect("FUN_001a6ff0");
            let got: [u16; 16] = std::array::from_fn(|k| ee.r16(RECORD + 0x10 + 2 * k as u32));
            let want = merge(&recs, world, &flag);
            assert_eq!(got, want, "world {world} flags set {all_set}");
            // FUN_0017cd20 for a few slot-byte patterns of this world.
            for bytes in [[0u8, 1, 2, 3], [0, 3, 2, 1], [0, 0x81, 0x10, 0x10], [0, 0x10, 0x10, 0x10], [0, 0x49, 0x0c, 0x02]] {
                ee.write(SAVE_SLOTS + 4 * world as u32, &bytes);
                for slot in 1..=3u64 {
                    let got = ee.call(0x17cd20, &[slot], &[], STACK).expect("FUN_0017cd20") as u16;
                    assert_eq!(got, slot_object(&want, bytes[slot as usize]), "world {world} bytes {bytes:02x?} slot {slot}");
                }
            }
        }
    }
    println!("FUN_001a6ff0 and FUN_0017cd20 agree with the doc for worlds 0..=18, with no story flag and with all of memt's flags set");

    // Presets: every preset of the disc against a set of current slot bytes.
    let presets: Vec<[u8; 4]> = memt[8 + recs.len() * 48..].as_chunks::<4>().0.to_vec();
    let currents = [[0u8, 1, 2, 0x10], [0, 2, 1, 0x10], [0, 3, 2, 1], [0, 0x10, 0x10, 0x10], [0, 1, 0x10, 0x10], [0, 3, 0x10, 0x10], [0, 0x81, 0x42, 3]];
    for (p, preset) in presets.iter().enumerate() {
        for cur in currents {
            ee.write(SLOTS, &cur);
            ee.call(0x1a7330, &[SLOTS as u64, p as u64], &[], STACK).expect("FUN_001a7330");
            let got: [u8; 4] = ee.read(SLOTS);
            assert_eq!(got, apply_preset(cur, *preset), "preset {p} {preset:02x?} current {cur:02x?}");
        }
    }
    println!("FUN_001a7330 agrees with the doc for the disc's {} presets", presets.len());
}

/// The doc's rule for FUN_001c77a8: a drive form takes Goofy (NeoStatus 3) in Valor (1), Donald (2) in
/// Wisdom (2), and every member in Master, Final and Anti (3..=5); nobody in form 0 or a form id of 6
/// and up. The bound is a signed compare, so a negative id (never used) counts like 3..=5.
fn form_takes(form: i32, neo: i32) -> bool {
    match form {
        0 | 6.. => false,
        1 => neo == 3,
        2 => neo == 2,
        _ => true,
    }
}

#[test]
fn form_and_danger_rules_match_the_doc() {
    let Some((mut ee, _)) = open() else { return };
    for form in -1..=8i32 {
        for neo in (0..=16).chain(200..=204) {
            let got = ee.call(0x1c77a8, &[form as i64 as u64, neo as u64], &[], STACK).expect("FUN_001c77a8") != 0;
            assert_eq!(got, form_takes(form, neo), "form {form} NeoStatus {neo}");
        }
    }

    // FUN_001a0be0: HP <= max / 4 (integer division toward zero).
    for max in 0..=400i32 {
        for hp in [0, 1, max / 4 - 1, max / 4, max / 4 + 1, max / 2, max] {
            ee.w32(STATUS, hp as u32);
            ee.w32(STATUS + 4, max as u32);
            let got = ee.call(0x1a0be0, &[STATUS as u64], &[], STACK).expect("FUN_001a0be0") != 0;
            assert_eq!(got, hp <= max / 4, "hp {hp} max {max}");
        }
    }

    // FUN_0010f490 / FUN_0010f4d0: HP (MP) below int(max * fraction); no status block -> false.
    let heal = ee.f32_at(HEAL_FRACTION);
    clear(&mut ee, OBJECT, 0x500);
    ee.w32(OBJECT + 0x48c, STATUS);
    for fraction in [heal, 0.25, 0.5, 1.0] {
        for max in [1i32, 3, 18, 25, 99, 100, 120, 999] {
            for v in 0..=max {
                ee.w32(STATUS, v as u32);
                ee.w32(STATUS + 4, max as u32);
                ee.w32(STATUS + 0x180, v as u32);
                ee.w32(STATUS + 0x184, max as u32);
                let want = v < (max as f32 * fraction) as i32;
                let hp = ee.call(0x10f490, &[OBJECT as u64], &[fraction], STACK).expect("FUN_0010f490") != 0;
                let mp = ee.call(0x10f4d0, &[OBJECT as u64], &[fraction], STACK).expect("FUN_0010f4d0") != 0;
                assert_eq!((hp, mp), (want, want), "value {v} max {max} fraction {fraction}");
            }
        }
    }
    ee.w32(OBJECT + 0x48c, 0);
    assert_eq!(ee.call(0x10f490, &[OBJECT as u64], &[heal], STACK).unwrap(), 0);
    println!("FUN_001c77a8, FUN_001a0be0, FUN_0010f490, FUN_0010f4d0 agree with the doc (item fraction {heal})");
}

#[test]
fn follow_speed_thresholds_match_the_doc() {
    let Some((mut ee, _)) = open() else { return };
    let names = [
        "Donald", "Goofy", "Auron", "Ping", "Mulan", "Aladdin", "Sparrow", "Beast", "Jack", "Simba", "Tron", "Riku", "Donald (LK)", "Goofy (LK)", "Stitch", "Genie",
        "Peter Pan", "Tinker Bell", "Chicken Little",
    ];
    for (class, name) in names.iter().enumerate() {
        let block = AI_PARAMS + ee.r32(AI_PARAMS + 4 * class as u32);
        clear(&mut ee, AI, 0x20);
        ee.w32(AI + 8, block);
        let mut rows = Vec::new();
        for (battle, follow) in [(0u64, 0u64), (1, 1), (1, 0)] {
            let base = match (battle, follow) {
                (0, _) => 0,
                (_, 1) => 0x20,
                _ => 0x40,
            };
            let mut row = Vec::new();
            for mode in 0..3u32 {
                ee.call(0x114078, &[AI as u64, mode as u64, battle, follow], &[], STACK).expect("FUN_00114078");
                let down = ee.fpr[0];
                ee.call(0x1140c0, &[AI as u64, mode as u64, battle, follow], &[], STACK).expect("FUN_001140c0");
                let up = ee.fpr[0];
                // The doc's rule: block + base + mode * 8 is the slow-down distance, +4 the speed-up one.
                assert_eq!(down, ee.f32_at(block + base + mode * 8), "{name} mode {mode}");
                assert_eq!(up, ee.f32_at(block + base + mode * 8 + 4), "{name} mode {mode}");
                row.push(format!("{down}/{up}"));
            }
            rows.push(row.join(" "));
        }
        println!("AI class {class:2} {name:15}: field {} | battle, following {} | battle, other target {}", rows[0], rows[1], rows[2]);
    }
}
