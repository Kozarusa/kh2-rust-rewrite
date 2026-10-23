//! Reads the user's own PCSX2 save states (`KH2_SSTATES`, default `Documents\PCSX2\sstates`) and compares with the values
//! local/specs/RIG.md 1.1 recorded. Skips (passes with a note) when the folder or a state is absent.

use kh2_rig::p2s::P2s;
use kh2_rig::save::{self, Extract};
use std::path::PathBuf;

fn state(name: &str) -> Option<PathBuf> {
    let dir = std::env::var_os("KH2_SSTATES").map(PathBuf::from).or_else(|| std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("Documents/PCSX2/sstates")))?;
    let p = dir.join(name);
    p.is_file().then_some(p)
}

#[test]
fn state_06_is_the_mansion_computer_room_with_sora_at_650_250() {
    let Some(p) = state("SLUS-21005 (DA0535FD).06.p2s") else {
        eprintln!("skipped: the user's state .06 is not here");
        return;
    };
    let s = P2s::open(&p).unwrap();
    assert_eq!(s.version.as_deref(), Some("v2.8.2"));
    let x = Extract::read(&s.ee).unwrap();
    assert_eq!((x.world, x.area, x.entrance, x.vsync, x.frame_step), (2, 0x15, 99, 495_135, 2.0));
    let sora = x.sora.expect("Sora exists in state .06");
    assert_eq!(sora.pos, [650.0, 0.0, 250.0]);
    // The state file the app loads: the party block and the story arrays are the memory's own bytes.
    let json: serde_json::Value = serde_json::from_str(&save::state_json(&s.ee, "06").unwrap()).unwrap();
    let block = json["block"].as_str().unwrap();
    assert_eq!(block.len(), 2 * save::PARTY_BLOCK_LEN);
    let at = save::PARTY_BLOCK as usize;
    assert_eq!(&block[..8], s.ee[at..at + 4].iter().map(|b| format!("{b:02x}")).collect::<String>());
    assert_eq!(json["munny"], 95);
}

#[test]
fn every_user_state_reads_and_matches_the_recorded_table() {
    // (file, world, area, entrance, vsync counter, has player object): the RIG.md 1.1 table.
    let table: [(&str, u8, u8, u8, u32, bool); 8] = [
        ("SLUS-21005 (DA0535FD).01.p2s", 2, 7, 0, 39_742, false),
        ("SLUS-21005 (DA0535FD).02.p2s", 2, 5, 0, 354_496, true),
        ("SLUS-21005 (DA0535FD).03.p2s", 2, 0x14, 0, 505_045, true),
        ("SLUS-21005 (DA0535FD).04.p2s", 2, 8, 0, 531_888, true),
        ("SLUS-21005 (DA0535FD).07.p2s", 4, 9, 0, 637_580, false),
        ("SLUS-21005 (DA0535FD).08.p2s", 4, 8, 0, 651_804, true),
        ("SLUS-21005 (DA0535FD).09.p2s", 4, 8, 0, 651_272, false),
        ("SLUS-21005 (DA0535FD).10.p2s", 255, 255, 255, 2_132, false),
    ];
    let mut seen = 0;
    for (name, world, area, entrance, vsync, player) in table {
        let Some(p) = state(name) else { continue };
        let x = Extract::read(&P2s::open(&p).unwrap().ee).unwrap();
        assert_eq!((x.world, x.area, x.entrance, x.vsync, x.sora.is_some()), (world, area, entrance, vsync, player), "{name}");
        seen += 1;
    }
    eprintln!("{seen} of {} states present", table.len());
}
