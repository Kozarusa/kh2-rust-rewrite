//! What the app needs out of a save state: the save block, the place, Sora, and the rig clock.
//!
//! All addresses are EE addresses (the executable's own; a `.p2s` holds EE RAM from address 0). Sources: docs/progress.md (the blocks and
//! offsets `Progress` already splits), local/specs/RIG.md section 1.1 (place, player pointer, vsync counter, frame step; read in the user's
//! states), docs/dynamic-tracing.md.

use crate::p2s::P2s;

/// The whole save block: header `KH2U-`, then everything the memory card keeps.
pub const SAVE_BLOCK: u32 = 0x0033_e860;
pub const SAVE_BLOCK_LEN: usize = 0xb4e0;
/// Current world, area and entrance: three bytes (a fourth, whose meaning is unknown, follows).
pub const PLACE: u32 = 0x0033_e830;
/// The player object pointer (zero when no Sora exists, for example at the title or in some menus).
pub const PLAYER_POINTER: u32 = 0x0035_c998;
/// Position vec4 in the player object (w = 1.0), game units (cm), the game's axes.
pub const PLAYER_POS: u32 = 0x70;
/// A unit vector equal to minus the +0x60 rotation row [inferred: the facing].
pub const PLAYER_FACING: u32 = 0x10;
/// The vsync counter saved in the state (60 per second, also in menus): the rig clock.
pub const VSYNC: u32 = 0x0033_fe64;
/// The field time step (2.0 in the field at 30 frames per second, 1.0 in menus).
pub const FRAME_STEP: u32 = 0x0035_6f40;
/// The party block (`progress::off`): member records, forms, item counts, experience.
pub const PARTY_BLOCK: u32 = 0x0033_fec0;
pub const PARTY_BLOCK_LEN: usize = 0x1998;
/// The wallet (save `+0x1600`), the game mode (save `+0x1658`).
pub const WALLET: u32 = 0x0033_fe60;
pub const GAME_MODE: u32 = 0x0033_feb8;
/// The record bit words (save `+0x1550`): 23 words up to the per-world array at 0x33fe0c.
pub const RECORD_BITS: u32 = 0x0033_fdb0;
pub const RECORD_WORDS: usize = 23;
/// The story flags (`progress::story`): 19 worlds of 8 words, the area program table (`(map, btl, evt)` x 0xc0 bytes per world),
/// the world-point bits (two words) and the exit locks (state, message halfword pairs) up to the record bits.
pub const STORY_FLAGS: u32 = 0x0033_f6b0;
pub const STORY_WORLDS: usize = 19;
pub const STORY_PAGE: usize = 0x20;
pub const STORY_PROGRAMS: u32 = 0x0033_e86f;
pub const STORY_PROGRAMS_PER_WORLD: usize = 0xc0;
pub const WORLD_POINTS: u32 = 0x0033_f910;
pub const EXIT_LOCKS: u32 = 0x0033_f918;
/// The world map's save bytes `0x341858 ..= 0x341a05` (`progress::world`).
pub const WORLD_STATE: u32 = 0x0034_1858;
pub const WORLD_STATE_LEN: usize = 0x0034_1a05 - 0x0034_1858 + 1;

/// Read access to EE memory with bounds checks (a state's image or any slice from address 0).
pub struct Ee<'a>(pub &'a [u8]);

impl Ee<'_> {
    pub fn bytes(&self, addr: u32, len: usize) -> Option<&[u8]> {
        let a = addr as usize;
        self.0.get(a..a.checked_add(len)?)
    }
    pub fn u8(&self, addr: u32) -> Option<u8> {
        self.bytes(addr, 1).map(|b| b[0])
    }
    pub fn u32(&self, addr: u32) -> Option<u32> {
        self.bytes(addr, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn f32(&self, addr: u32) -> Option<f32> {
        self.u32(addr).map(f32::from_bits)
    }
    pub fn vec3(&self, addr: u32) -> Option<[f32; 3]> {
        Some([self.f32(addr)?, self.f32(addr + 4)?, self.f32(addr + 8)?])
    }
}

/// Sora as the player object holds him.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sora {
    pub object: u32,
    pub pos: [f32; 3],
    pub facing: [f32; 3],
}

/// The values read out of a state.
#[derive(Clone, Debug, PartialEq)]
pub struct Extract {
    pub world: u8,
    pub area: u8,
    pub entrance: u8,
    pub vsync: u32,
    pub frame_step: f32,
    /// `None` when the pointer is zero or points outside RAM.
    pub sora: Option<Sora>,
}

impl Extract {
    pub fn read(ee: &[u8]) -> Result<Extract, String> {
        let m = Ee(ee);
        let miss = |what: &str| format!("state memory too short for {what}");
        let header = m.bytes(SAVE_BLOCK, 5).ok_or_else(|| miss("the save block"))?;
        if header != b"KH2U-" {
            return Err(format!("save block header at {SAVE_BLOCK:#x} reads {:?}, not \"KH2U-\" (not a Kingdom Hearts II USA state?)", String::from_utf8_lossy(header)));
        }
        let place = m.bytes(PLACE, 3).ok_or_else(|| miss("the place"))?;
        let object = m.u32(PLAYER_POINTER).ok_or_else(|| miss("the player pointer"))?;
        let sora = if object == 0 {
            None
        } else {
            m.vec3(object.wrapping_add(PLAYER_POS)).zip(m.vec3(object.wrapping_add(PLAYER_FACING))).map(|(pos, facing)| Sora { object, pos, facing })
        };
        Ok(Extract { world: place[0], area: place[1], entrance: place[2], vsync: m.u32(VSYNC).ok_or_else(|| miss("the vsync counter"))?, frame_step: m.f32(FRAME_STEP).ok_or_else(|| miss("the frame step"))?, sora })
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn nums<T: ToString>(v: impl IntoIterator<Item = T>) -> String {
    format!("[{}]", v.into_iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
}

/// Finite floats print as JSON numbers, others as null.
fn num(v: f32) -> String {
    if v.is_finite() { format!("{v}") } else { "null".into() }
}

/// The `KH2_STATE` keys the app reads today, in the order `Progress::apply_json` needs them (`block` first: it resets the keys that
/// follow). Everything comes from the save block by the addresses `progress.rs` already uses; the pieces that need the game's
/// own decoding (moogle record, shops, journal, minigames, kill counters) are not split here: the whole block is in `save_block` of
/// [`rig_json`] for the app side to split.
pub fn state_json(ee: &[u8], source: &str) -> Result<String, String> {
    let x = Extract::read(ee)?;
    let m = Ee(ee);
    let miss = |what: &str| format!("state memory too short for {what}");
    let party = m.bytes(PARTY_BLOCK, PARTY_BLOCK_LEN).ok_or_else(|| miss("the party block"))?;
    let wallet = m.u32(WALLET).ok_or_else(|| miss("the wallet"))?;
    let mode = m.u32(GAME_MODE).ok_or_else(|| miss("the game mode"))?;
    let mut record: Vec<u32> = (0..RECORD_WORDS).map(|i| m.u32(RECORD_BITS + 4 * i as u32).ok_or_else(|| miss("the record bits"))).collect::<Result<_, _>>()?;
    while record.last() == Some(&0) {
        record.pop();
    }
    let words = m.bytes(STORY_FLAGS, STORY_WORLDS * STORY_PAGE).ok_or_else(|| miss("the story flags"))?;
    let programs = m.bytes(STORY_PROGRAMS, STORY_WORLDS * STORY_PROGRAMS_PER_WORLD).ok_or_else(|| miss("the area programs"))?;
    let lock_bytes = m.bytes(EXIT_LOCKS, (RECORD_BITS - EXIT_LOCKS) as usize).ok_or_else(|| miss("the exit locks"))?;
    let mut locks: Vec<[u16; 2]> = lock_bytes.as_chunks::<4>().0.iter().map(|c| [u16::from_le_bytes([c[0], c[1]]), u16::from_le_bytes([c[2], c[3]])]).collect();
    while locks.last() == Some(&[0, 0]) {
        locks.pop();
    }
    let points = [m.u32(WORLD_POINTS).ok_or_else(|| miss("the world points"))?, m.u32(WORLD_POINTS + 4).ok_or_else(|| miss("the world points"))?];
    let points: Vec<usize> = (0..64).filter(|&i| points[i >> 5] >> (i & 31) & 1 != 0).collect();
    let world = m.bytes(WORLD_STATE, WORLD_STATE_LEN).ok_or_else(|| miss("the world map bytes"))?;
    let comment = source.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').collect::<String>().replace(['"', '\\'], "");
    let fields = [
        ("comment", format!("\"kh2 state from-p2s {comment}; vsync {}\"", x.vsync)),
        ("block", format!("\"{}\"", hex(party))),
        ("munny", wallet.to_string()),
        ("game_mode", mode.min(2).to_string()),
        ("record_bits", nums(record)),
        ("story_raw", format!("{{\"words\": \"{}\", \"programs\": \"{}\", \"locks\": {}}}", hex(words), hex(programs), nums(locks.iter().flat_map(|l| [l[0], l[1]])))),
        ("world_points", nums(points)),
        ("world_state", format!("\"{}\"", hex(world))),
    ];
    Ok(join(&fields))
}

/// The keys the app cannot read yet (RIG-3): the place, Sora, the rig clock and the whole save block. Written beside the state file
/// (`<name>.rig.json`), or merged into it with `--with-rig-keys` once `Progress::apply_json` accepts them.
pub fn rig_json(ee: &[u8], source: &str, sha1: Option<&str>) -> Result<String, String> {
    let x = Extract::read(ee)?;
    let mem = Ee(ee);
    let block = mem.bytes(SAVE_BLOCK, SAVE_BLOCK_LEN).ok_or("state memory too short for the save block")?;
    let sora = match x.sora {
        Some(s) => format!("{{\"pos\": [{}, {}, {}], \"facing\": [{}, {}, {}], \"object\": {}}}", num(s.pos[0]), num(s.pos[1]), num(s.pos[2]), num(s.facing[0]), num(s.facing[1]), num(s.facing[2]), s.object),
        None => "null".into(),
    };
    let name = source.rsplit(['/', '\\']).next().unwrap_or(source).replace(['"', '\\'], "");
    let sha = sha1.map_or(String::new(), |s| format!(", \"sha1\": \"{s}\""));
    let fields = [
        ("save_block", format!("\"{}\"", hex(block))),
        ("place", format!("{{\"world\": {}, \"area\": {}, \"entrance\": {}}}", x.world, x.area, x.entrance)),
        ("sora", sora),
        ("rig", format!("{{\"vsync\": {}, \"frame_step\": {}, \"source\": \"{name}\"{sha}}}", x.vsync, num(x.frame_step))),
    ];
    Ok(join(&fields))
}

/// `state_json` with the rig keys merged in (one file, once the app reads them).
pub fn merged_json(ee: &[u8], source: &str, sha1: Option<&str>) -> Result<String, String> {
    let a = state_json(ee, source)?;
    let b = rig_json(ee, source, sha1)?;
    let inner = |s: &str| s.trim().trim_start_matches('{').trim_end_matches('}').trim().to_string();
    Ok(format!("{{\n  {},\n  {}\n}}\n", inner(&a), inner(&b)))
}

fn join(fields: &[(&str, String)]) -> String {
    let lines: Vec<String> = fields.iter().map(|(k, v)| format!("  \"{k}\": {v}")).collect();
    format!("{{\n{}\n}}\n", lines.join(",\n"))
}

/// Convenience: read a state file and return the memory.
pub fn load(path: &std::path::Path) -> Result<P2s, String> {
    P2s::open(path)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A synthetic EE image with the save block header, a place, Sora and some party/story bytes. No game data.
    pub fn synthetic() -> Vec<u8> {
        let mut ee = vec![0u8; crate::p2s::EE_SIZE];
        let put32 = |ee: &mut Vec<u8>, a: u32, v: u32| ee[a as usize..a as usize + 4].copy_from_slice(&v.to_le_bytes());
        ee[SAVE_BLOCK as usize..SAVE_BLOCK as usize + 5].copy_from_slice(b"KH2U-");
        ee[PLACE as usize..PLACE as usize + 3].copy_from_slice(&[2, 0x15, 99]);
        put32(&mut ee, VSYNC, 495_135);
        put32(&mut ee, FRAME_STEP, 2.0f32.to_bits());
        put32(&mut ee, WALLET, 95);
        put32(&mut ee, GAME_MODE, 1);
        let obj = 0x0180_0000u32;
        put32(&mut ee, PLAYER_POINTER, obj);
        for (i, v) in [650.0f32, 0.0, 250.0, 1.0].into_iter().enumerate() {
            put32(&mut ee, obj + PLAYER_POS + 4 * i as u32, v.to_bits());
        }
        for (i, v) in [0.0f32, 0.0, -1.0].into_iter().enumerate() {
            put32(&mut ee, obj + PLAYER_FACING + 4 * i as u32, v.to_bits());
        }
        ee[PARTY_BLOCK as usize] = 0xab;
        ee[PARTY_BLOCK as usize + PARTY_BLOCK_LEN - 1] = 0xcd;
        put32(&mut ee, RECORD_BITS + 8, 5);
        put32(&mut ee, STORY_FLAGS, 0x12);
        ee[STORY_PROGRAMS as usize] = 7;
        put32(&mut ee, EXIT_LOCKS + 8, 0x0003_0002); // lock entry 2: state 2, message 3
        put32(&mut ee, WORLD_POINTS, 0x8000_0001);
        ee[WORLD_STATE as usize + 1] = 4;
        ee
    }

    #[test]
    fn extract_reads_the_place_sora_and_the_clock() {
        let x = Extract::read(&synthetic()).unwrap();
        assert_eq!((x.world, x.area, x.entrance, x.vsync, x.frame_step), (2, 0x15, 99, 495_135, 2.0));
        let s = x.sora.unwrap();
        assert_eq!((s.object, s.pos, s.facing), (0x0180_0000, [650.0, 0.0, 250.0], [0.0, 0.0, -1.0]));
    }

    #[test]
    fn no_player_object_means_no_sora() {
        let mut ee = synthetic();
        ee[PLAYER_POINTER as usize..PLAYER_POINTER as usize + 4].fill(0);
        assert_eq!(Extract::read(&ee).unwrap().sora, None);
        // A pointer outside RAM is ignored too.
        ee[PLAYER_POINTER as usize..PLAYER_POINTER as usize + 4].copy_from_slice(&0xdead_0000u32.to_le_bytes());
        assert_eq!(Extract::read(&ee).unwrap().sora, None);
    }

    #[test]
    fn a_state_of_another_game_is_refused() {
        let mut ee = synthetic();
        ee[SAVE_BLOCK as usize] = b'X';
        assert!(Extract::read(&ee).unwrap_err().contains("KH2U-"));
        assert!(Extract::read(&[0u8; 16]).is_err());
    }

    fn parse(text: &str) -> serde_json::Value {
        serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}: {text:.200}"))
    }

    #[test]
    fn the_state_file_carries_the_keys_the_app_reads() {
        let j = parse(&state_json(&synthetic(), "state.06.p2s").unwrap());
        let obj = j.as_object().unwrap();
        // The keys must be a subset of `Progress::apply_json`'s (docs/progress.md; unknown keys make the app drop the whole file).
        for k in obj.keys() {
            assert!(["comment", "block", "munny", "game_mode", "record_bits", "story_raw", "world_points", "world_state"].contains(&k.as_str()), "key {k} is not read by the app");
        }
        assert_eq!(obj.keys().next().map(String::as_str), Some("comment"));
        assert_eq!(obj.keys().nth(1).map(String::as_str), Some("block"), "block comes before the keys it resets");
        let block = obj["block"].as_str().unwrap();
        assert_eq!(block.len(), 2 * PARTY_BLOCK_LEN);
        assert!(block.starts_with("ab") && block.ends_with("cd"));
        assert_eq!(obj["munny"], 95);
        assert_eq!(obj["record_bits"], serde_json::json!([0, 0, 5]));
        assert_eq!(obj["world_points"], serde_json::json!([0, 31]));
        let raw = &obj["story_raw"];
        assert_eq!(raw["words"].as_str().unwrap().len(), 2 * STORY_WORLDS * STORY_PAGE);
        assert!(raw["words"].as_str().unwrap().starts_with("12000000"));
        assert!(raw["programs"].as_str().unwrap().starts_with("07"));
        assert_eq!(raw["locks"], serde_json::json!([0, 0, 0, 0, 2, 3]));
        assert_eq!(obj["world_state"].as_str().unwrap().len(), 2 * WORLD_STATE_LEN);
    }

    #[test]
    fn the_rig_file_carries_place_sora_and_the_whole_block() {
        let j = parse(&rig_json(&synthetic(), "C:\\x\\state.06.p2s", Some("abc")).unwrap());
        assert_eq!(j["place"], serde_json::json!({"world": 2, "area": 21, "entrance": 99}));
        let pos: Vec<f64> = j["sora"]["pos"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert_eq!(pos, vec![650.0, 0.0, 250.0]);
        assert_eq!(j["rig"]["vsync"], 495_135);
        assert_eq!(j["rig"]["source"], "state.06.p2s");
        assert_eq!(j["rig"]["sha1"], "abc");
        assert_eq!(j["save_block"].as_str().unwrap().len(), 2 * SAVE_BLOCK_LEN);
        assert!(j["save_block"].as_str().unwrap().starts_with(&hex(b"KH2U-")));
        let m = parse(&merged_json(&synthetic(), "s.p2s", None).unwrap());
        assert_eq!(m["place"]["area"], 21);
        assert_eq!(m["munny"], 95);
    }
}
