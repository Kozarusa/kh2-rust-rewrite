//! Oracle check behind docs/events.md section 2 (exit volumes): `Activator::contains` against the
//! game's own routines run in kh2-ee on the user's executable. FUN_001d8600 builds a volume from a
//! 0x40-byte activator record, then FUN_001d89a0 (box) or FUN_001d8a70 (cylinder) tests a point.
//! The records are the real exit activators of every Twilight Town area plus synthetic ones with
//! several turns; the points are a grid around each volume. Skips without a disc image.

use kh2_ee::Ee;

const STACK: u32 = 0x01ff_0000;
const VOLUME: u32 = 0x0180_0000;
const RECORD: u32 = 0x0181_0000;
const POINT: u32 = 0x0182_0000;

fn open() -> Option<(Ee, kh2_archive::Archive)> {
    let path = std::env::var("KH2_ISO")
        .ok()
        .or_else(|| std::fs::read_to_string("../../local/iso.txt").ok().map(|s| s.trim().to_string()))
        .filter(|p| std::path::Path::new(p).exists());
    let Some(path) = path else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return None;
    };
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    let names = std::fs::read_to_string("../../local/kh2idx.txt").ok();
    let list: Vec<&str> = names.as_deref().map(|l| l.lines().collect()).unwrap_or_default();
    a.apply_names(if list.is_empty() { vec!["000tt.idx"] } else { list }).ok()?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some((ee, a))
}

/// Runs the game's volume test for `rec` at `p`.
fn game_contains(ee: &mut Ee, rec: &[u8; 0x40], p: [f32; 3]) -> bool {
    ee.write(RECORD, rec);
    for o in (0..0x70).step_by(4) {
        ee.w32(VOLUME + o, 0);
    }
    ee.call(0x001d_8600, &[VOLUME as u64, RECORD as u64], &[], STACK).expect("FUN_001d8600");
    for (k, v) in p.iter().enumerate() {
        ee.wf32(POINT + 4 * k as u32, *v);
    }
    ee.wf32(POINT + 12, 1.0);
    let f = if u16::from_le_bytes([rec[0], rec[1]]) == 0 { 0x001d_89a0 } else { 0x001d_8a70 };
    ee.call(f, &[VOLUME as u64, POINT as u64], &[], STACK).expect("volume test") & 0xffff_ffff != 0
}

fn record(shape: u16, pos: [f32; 3], scale: [f32; 3], rot: [f32; 3]) -> [u8; 0x40] {
    let mut r = [0u8; 0x40];
    r[0..2].copy_from_slice(&shape.to_le_bytes());
    for (k, v) in pos.iter().chain(&scale).chain(&rot).enumerate() {
        r[4 + 4 * k..8 + 4 * k].copy_from_slice(&v.to_le_bytes());
    }
    r
}

#[test]
fn exit_volumes_match_the_game() {
    let Some((mut ee, mut a)) = open() else { return };
    let mut recs: Vec<[u8; 0x40]> = Vec::new();
    for n in 0..42 {
        let Ok(file) = a.read(&format!("ard/tt{n:02}.ard")) else { continue };
        let Some((ard, _)) = kh2_ard::Area::parse(&file) else { continue };
        for g in ard.spawns.values().flatten() {
            if g.exit().is_some() {
                recs.extend(g.activators.iter().filter(|v| v.shape < 2).map(|v| v.raw));
            }
        }
    }
    let real = recs.len();
    for (k, ry) in [0.0f32, 0.4, 1.2, -2.0, 3.0].iter().enumerate() {
        recs.push(record((k % 2) as u16, [50.0 * k as f32, -20.0, 30.0], [120.0, 80.0, 40.0], [0.0, *ry, 0.0]));
    }
    let (mut checked, mut inside, mut bad) = (0, 0, Vec::new());
    for rec in &recs {
        let v = kh2_ard::Activator {
            shape: u16::from_le_bytes([rec[0], rec[1]]),
            option: 0,
            position: std::array::from_fn(|k| f32::from_le_bytes(rec[4 + 4 * k..8 + 4 * k].try_into().unwrap())),
            scale: std::array::from_fn(|k| f32::from_le_bytes(rec[0x10 + 4 * k..0x14 + 4 * k].try_into().unwrap())),
            rotation: std::array::from_fn(|k| f32::from_le_bytes(rec[0x1c + 4 * k..0x20 + 4 * k].try_into().unwrap())),
            raw: *rec,
        };
        let r = v.scale[0].abs().max(v.scale[2].abs()) * 1.3;
        for i in -6..=6 {
            for j in -6..=6 {
                for y in [-1.2f32, 0.0, 0.9] {
                    let p = [v.position[0] + r * i as f32 / 6.0, v.position[1] + v.scale[1] * y, v.position[2] + r * j as f32 / 6.0];
                    let (want, got) = (game_contains(&mut ee, rec, p), v.contains(p));
                    checked += 1;
                    inside += want as usize;
                    // A point on the boundary may round either way.
                    let edge = { let l = v.local(p); (l[0].abs() - v.scale[0].abs()).abs() < 0.01 || (l[2].abs() - v.scale[2].abs()).abs() < 0.01 };
                    if want != got && !edge {
                        bad.push((v.shape, v.rotation[1], p, want));
                    }
                }
            }
        }
    }
    eprintln!("volume oracle: {} records ({real} real exit activators), {checked} points, {inside} inside, {} mismatches", recs.len(), bad.len());
    assert!(real > 0, "no exit activators found");
    assert!(bad.is_empty(), "mismatches (shape, ry, point, game): {:?}", &bad[..bad.len().min(10)]);
}
