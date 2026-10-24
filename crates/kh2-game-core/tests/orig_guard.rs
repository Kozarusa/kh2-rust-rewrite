//! WP-FIX-GUARD: where the original gives Sora Guard (ability item 82) and Aerial Recovery (158) at the start of the story, and the
//! oracle of the give path.
//!
//! The finding (read from the disc and the executable, docs/systems/combat-effects/abilities.md "Guard and Aerial Recovery"):
//! - Neither is in a level-up row, a treasure row, a new-game `plrp` row or a BDX host call (`trap_item_get` 0.63 and
//!   `trap_bonuslevel_up` 2.73 have no call site in any script on the disc). They are `bons` bonus rewards: reward 54 gives
//!   character 14 (Roxas, whose record is Sora's: `FUN_001b58d8` maps 14 to record 1) item 158, reward 33 gives him item 82 and 5 HP.
//! - A bonus reward reaches the game through a mission's end (`FUN_001b95a0`: the mission header byte `+0xd`, when not 0, goes to
//!   `FUN_001be6b8`), an `enmp` BonusLevel (0 on this disc) or a script host call (none on this disc). The headers of
//!   `msn/us/TT32_MS302.bar` (mission 179) and `msn/us/TT34_MS304.bar` (mission 181) carry 54 and 33.
//! - `FUN_001be6b8` -> `FUN_001be500` -> `FUN_001a18d8(item, 14, 0)` -> `FUN_00187868`: the ability goes unequipped into the first
//!   empty slot of Sora's ability list (the list the Abilities menu shows).
//!
//! Tests: the disc facts and the port's path (`Progress::give_bonus`, wired to the mission end), and an oracle of `FUN_001a18d8` for
//! ability items over random ability lists (a hand harness: kh2-ee runs the original on the same party block bytes as the port; the
//! item table is the disc's, behind its pointer word `0x1c95588`; every byte of the party block is compared after each give).
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::sync::Arc;

use kh2_ee::Ee;
use kh2_game_core::progress::{ItemIndex, Progress, addr, off};
use kh2_game_core::shared::struggle::{mission_command, parse_header};
use kh2_tables::tables::{self as t, Bons, Cmd, Item, Magc, Plrp};

const STACK: u32 = 0x01ff_0000;
const ITEM_PTR: u32 = 0x01c9_5588;
const GUARD: u16 = 82;
const AERIAL_RECOVERY: u16 = 158;
/// Roxas's party id (`FUN_001b58d8`: his record is Sora's).
const ROXAS: u16 = 14;

struct Disc {
    archive: kh2_archive::Archive,
    items: Vec<Item>,
    cmds: Vec<Cmd>,
    magc: Vec<Magc>,
    plrp: Vec<Plrp>,
    bons: Vec<Bons>,
}

fn disc(what: &str) -> Option<Disc> {
    let iso = std::env::var("KH2_ISO").ok().or_else(|| {
        let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
        std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
    });
    let Some(iso) = iso.filter(|p| std::path::Path::new(p).exists()) else {
        assert!(std::env::var("KH2_REQUIRE_DISC").as_deref() != Ok("1"), "{what}: no disc and KH2_REQUIRE_DISC=1");
        eprintln!("skipped {what}: no disc (set KH2_ISO)");
        return None;
    };
    let mut a = kh2_archive::Archive::open(&iso).expect("disc");
    // The mission files sit in a sub-index that only a name list opens (local/kh2idx.txt, `kh2 fetch-names`).
    let names = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/kh2idx.txt")).unwrap_or_default();
    a.apply_names(names.lines()).expect("name list");
    Some(Disc {
        items: kh2_tables::load::<Item>(&mut a, &t::ITEM).expect("item").rows,
        cmds: kh2_tables::load::<Cmd>(&mut a, &t::CMD).expect("cmd").rows,
        magc: kh2_tables::load::<Magc>(&mut a, &t::MAGC).expect("magc").rows,
        plrp: kh2_tables::load::<Plrp>(&mut a, &t::PLRP).expect("plrp").rows,
        bons: kh2_tables::load::<Bons>(&mut a, &t::BONS).expect("bons").rows,
        archive: a,
    })
}

fn read(a: &mut kh2_archive::Archive, name: &str) -> Vec<u8> {
    a.read(name).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The mission header's bonus byte (`+0xd`) of a mission file.
fn mission_bonus(a: &mut kh2_archive::Archive, name: &str) -> (u16, u8) {
    let file = read(a, &format!("msn/us/{name}.bar"));
    let entries = kh2_bar::parse(&file).expect("mission BAR");
    let h = entries.iter().find(|e| e.kind == 2).and_then(|e| parse_header(e.data)).expect("mission header");
    (h.id, h.unlock)
}

/// The rows of a `bons` reward: (character, item 1, item 2, HP).
fn reward(rows: &[Bons], id: u8) -> Vec<(u8, u16, u16, u8)> {
    rows.iter().filter(|b| b.reward_id == id).map(|b| (b.character_id, b.bonus_item1, b.bonus_item2, b.hp_increase)).collect()
}

#[test]
fn guard_and_aerial_recovery_are_the_tt32_and_tt34_mission_rewards() {
    let Some(mut d) = disc("Guard grant") else { return };
    // The mission headers carry the reward ids.
    assert_eq!(mission_bonus(&mut d.archive, "TT32_MS302"), (179, 54));
    assert_eq!(mission_bonus(&mut d.archive, "TT34_MS304"), (181, 33));
    // The rewards give the two abilities to Roxas (and Guard's 5 HP).
    assert_eq!(reward(&d.bons, 54), vec![(ROXAS as u8, AERIAL_RECOVERY, 0, 0)]);
    assert_eq!(reward(&d.bons, 33), vec![(ROXAS as u8, GUARD, 0, 5)]);
    // No other reward, no level-up row and no treasure row gives either.
    assert!(d.bons.iter().filter(|b| [b.bonus_item1, b.bonus_item2].iter().any(|&i| i == GUARD || i == AERIAL_RECOVERY)).count() == 2);
    // The other Twilight Town missions with a header reward (the port pays them too, as the original does).
    for (name, id) in [("TT14_MS001", 56u8), ("TT20_MS603", 34), ("TT40_MS003", 63)] {
        assert_eq!(mission_bonus(&mut d.archive, name).1, id, "{name}");
        eprintln!("{name}: reward {id}: {:?}", d.bons.iter().filter(|b| b.reward_id == id).collect::<Vec<_>>());
        assert!(!reward(&d.bons, id).is_empty());
    }
    // The areas' battle programs start those missions (area-script opcode 0x15).
    for (area, name) in [("tt32", "TT32_MS302"), ("tt34", "TT34_MS304")] {
        let file = read(&mut d.archive, &format!("ard/{area}.ard"));
        let (ard, _) = kh2_ard::Area::parse(&file).expect("area file");
        let progs: Vec<_> = ard.scripts.get("btl").into_iter().flatten().filter(|p| mission_command(p).is_some_and(|(_, n)| n == name)).collect();
        assert!(!progs.is_empty(), "{area}: no btl program starts {name}");
        for p in progs {
            eprintln!("{area}: btl program {} starts {name}: {:?}", p.id, p.commands.iter().map(|c| (c.opcode, c.args.clone())).collect::<Vec<_>>());
        }
    }
}

/// The port's path: the two rewards, given as the mission ends give them, put Aerial Recovery then Guard into Sora's ability list,
/// unequipped, and Guard's 5 HP into his maximum; a second end gives nothing (the reward's flag).
#[test]
fn the_mission_rewards_put_guard_and_aerial_recovery_into_soras_list() {
    let Some(d) = disc("Guard grant through the port") else { return };
    let ix = Arc::new(ItemIndex::new(&d.items, &d.cmds, &d.magc));
    let mut p = Progress::new_game(ix, &d.plrp);
    let before: Vec<u16> = p.sora().abilities.iter().map(|&(id, _)| id).collect();
    assert!(!before.contains(&GUARD) && !before.contains(&AERIAL_RECOVERY), "the new game has neither: {before:?}");
    let hp = p.sora().hp_max;
    let a = p.give_bonus(&d.bons, 54).expect("reward 54");
    assert_eq!(a.parts.len(), 1);
    assert_eq!((a.parts[0].member, a.parts[0].gifts.clone()), (ROXAS, vec![(AERIAL_RECOVERY, true)]));
    let g = p.give_bonus(&d.bons, 33).expect("reward 33");
    assert_eq!(g.parts[0].gifts, vec![(GUARD, true)]);
    let list = p.sora().abilities;
    let added: Vec<_> = list.iter().filter(|&&(id, _)| id == GUARD || id == AERIAL_RECOVERY).copied().collect();
    assert_eq!(added, vec![(AERIAL_RECOVERY, false), (GUARD, false)], "unequipped, in grant order: {list:?}");
    assert_eq!(p.sora().hp_max, hp + 5);
    assert!(p.give_bonus(&d.bons, 33).is_none(), "a reward is given once");
    assert_eq!(p.bonus_level(), 2);
}

/// A small generator (the cases must be the same on every run).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
}

/// The oracle of `FUN_001a18d8` for ability items: 1,500 cases, each a new-game party block whose Sora, Donald and Goofy ability
/// lists (record `+0x54`, 64 halfwords) are filled with a random number of random ability ids (some equipped, some lists full),
/// then a give of a random ability item (Guard and Aerial Recovery in half the cases) to Roxas (14), Sora (1), Donald (2) or the
/// inventory (100), equipped or not. The game's answer and every byte of the party block must equal the port's.
#[test]
fn orig_1a18d8_matches_the_game() {
    let Some(mut d) = disc("oracle test of 0x1a18d8") else { return };
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| d.archive.iso().read(p).ok()).expect("executable");
    let blob = kh2_tables::subfile(&mut d.archive, &t::ITEM).expect("item table");
    let ix = Arc::new(ItemIndex::new(&d.items, &d.cmds, &d.magc));
    let abilities: Vec<u16> = d.items.iter().filter(|r| r.r#type == 0x13 && r.id != 0).map(|r| r.id).collect();
    assert!(abilities.contains(&GUARD) && abilities.contains(&AERIAL_RECOVERY));
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.write(0x0180_0000, &blob);
    ee.w32(ITEM_PTR, 0x0180_0000);
    let base = Progress::new_game(ix.clone(), &d.plrp);
    let mut rng = Lcg(0x6a_7d11);
    let (mut granted, mut refused, mut unchanged) = (0, 0, 0);
    for case in 0..1500 {
        let mut block = base.block().to_vec();
        // Whether Donald's (index 0) and Sora's (index 1) lists are full.
        let mut full = [false; 2];
        for rec in 1..=3usize {
            let at = kh2_game_core::save_block::member_offset(rec as i32).expect("record") + 0x54;
            let n = match rng.next() % 4 {
                0 => 0,
                1 => 64,
                _ => (rng.next() % 64) as usize,
            };
            if rec <= 2 {
                full[2 - rec] = n == 64;
            }
            for k in 0..64 {
                let v = if k < n { abilities[rng.next() as usize % abilities.len()] | if rng.next() % 2 == 0 { 0x8000 } else { 0 } } else { 0 };
                block[at + 2 * k..at + 2 * k + 2].copy_from_slice(&v.to_le_bytes());
            }
        }
        let id = match rng.next() % 4 {
            0 => GUARD,
            1 => AERIAL_RECOVERY,
            _ => abilities[rng.next() as usize % abilities.len()],
        };
        let to = [ROXAS, 1, 2, 100][rng.next() as usize % 4];
        let eq = rng.next() % 2 == 1;
        ee.write(addr::PARTY, &block);
        let mut port = Progress::from_block(&block, ix.clone());
        let ok = ee.call(0x001a_18d8, &[id as u64, to as u64, eq as u64], &[], STACK).expect("FUN_001a18d8") as u32 != 0;
        assert_eq!(port.give(id, to, eq), ok, "case {case}: give {id} to {to} (equipped {eq})");
        let game: Vec<u8> = (0..off::LEN as u32).map(|o| ee.r8(addr::PARTY + o)).collect();
        let diff: Vec<String> = game.iter().zip(port.block()).enumerate().filter(|(_, (x, y))| x != y).map(|(o, (x, y))| format!("+{o:#x}: game {x:#x} port {y:#x}")).take(8).collect();
        assert!(diff.is_empty(), "case {case}: give {id} to {to} (equipped {eq}): {diff:?}");
        if ok { granted += 1 } else { refused += 1 }
        if full[usize::from(to != 2)] && to != 100 {
            unchanged += 1;
        }
    }
    // An ability item is always taken (`FUN_001a1768` answers the member for type 0x13), and a full list leaves the list as it was.
    assert!(granted > 1000 && unchanged > 50, "the cases reach a full list: {granted} given, {unchanged} into a full list");
    eprintln!("0x1a18d8: 1500 cases equal ({granted} given, {refused} refused, {unchanged} into a full list)");
}
