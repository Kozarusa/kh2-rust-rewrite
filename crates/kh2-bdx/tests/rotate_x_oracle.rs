//! Host 0.68 (`vec_rotate_x`, FUN_0016cf78 -> FUN_0017c398): this crate's `math::rotate_x` against the game's own routine run in kh2-ee on
//! the user's executable, over many vectors and angles. The routine is called as the host does: a block of the vector's address and the
//! angle, the answer an address of a static vector. Skips when no disc image is configured (KH2_ISO).

use kh2_bdx::math::rotate_x;
use kh2_ee::Ee;

const BLOCK: u32 = 0x0180_0000;
const VEC: u32 = 0x0181_0000;
const STACK: u32 = 0x01ff_0000;
/// The identity matrix FUN_0011b460 copies (the game sets it up at start-up).
const IDENTITY: u32 = 0x354a60;

#[test]
fn vec_rotate_x_matches_the_game() {
    let Some(path) = std::env::var("KH2_ISO").ok().filter(|p| std::path::Path::new(p).exists()) else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return;
    };
    let mut a = kh2_archive::Archive::open(&path).expect("iso");
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable");
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    for k in 0..16 {
        ee.wf32(IDENTITY + 4 * k, if k % 5 == 0 { 1.0 } else { 0.0 });
    }
    let vectors = [[0.0f32, 0.0, 1.0, 1.0], [1.0, 2.0, 3.0, 1.0], [-4.5, 7.25, -0.5, 1.0], [0.0, 300.0, -600.0, 0.0], [10.0, -3.0, 8.0, 1.0]];
    let angles = [1.047198f32, -1.047198, 0.3, 2.5, -2.9, 3.5, 0.0, 6.0, 0.7853982];
    let mut checked = 0;
    for v in vectors {
        for ang in angles {
            for (i, x) in v.iter().enumerate() {
                ee.w32(VEC + 4 * i as u32, x.to_bits());
            }
            ee.w32(BLOCK, VEC);
            ee.w32(BLOCK + 4, ang.to_bits());
            ee.call(0x16cf78, &[BLOCK as u64], &[], STACK).expect("FUN_0016cf78");
            let at = ee.r32(BLOCK);
            let game: Vec<f32> = (0..4).map(|i| ee.f32_at(at + 4 * i)).collect();
            let mine = rotate_x(v, ang);
            for k in 0..4 {
                let tol = 1e-3 * (1.0 + game[k].abs());
                assert!((game[k] - mine[k]).abs() <= tol, "vector {v:?} angle {ang}: lane {k} game {game:?} port {mine:?}");
            }
            checked += 1;
        }
    }
    assert_eq!(checked, vectors.len() * angles.len());
}
