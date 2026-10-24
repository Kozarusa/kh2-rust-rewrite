//! Oracle tests of the item and record rules of `Progress` (core.save, WP-L1-P27a). kh2-ee runs the original from the user's executable
//! on a random party block with the disc's `item`, `cmd`, `magc` and `plrp` tables laid out behind their pointers, and the port runs on
//! a copy of the same bytes; the return value and every byte of the party block are compared (the same set-up as `orig_save.rs` and
//! `orig_guard.rs`, whose helpers are private to those files).
//!
//! Several originals have no public port function of their own: `FUN_001a1a18` (item add), `FUN_001a2010` (a form item takes a
//! keyblade), `FUN_001a20b0` (take the first item of a type), `FUN_001a2120` (note a consumable) are private methods of `Progress` that
//! `Progress::give` runs. Their tests drive the original `FUN_001a18d8` (the give) with items of the type that reaches the function and
//! compare the whole block after it, so every byte the function writes is checked against the original running the function itself.
//!
//! Stand-ins: `FUN_001803b0` (the Struggle Wand flag side effect) and `FUN_00180450` (the spell tutorial flags) act on the story flags
//! outside the party block and are replaced by `jr ra`; the port leaves them out for the same reason (its doc comment says so).
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::sync::Arc;

use kh2_ee::Ee;
use kh2_factory::fuzz::*;
use kh2_game_core::progress::{INVENTORY, ItemIndex, Progress, item_type, lvup_row};
use kh2_game_core::save_block::party;
use kh2_tables::tables::{self as t, Cmd, Item, Lvup, Magc, Plrp};

const PARTY: u32 = 0x33fec0;
const ITEM_PTR: u32 = 0x01c9_5588;
const PLRP_PTR: u32 = 0x01d5_df5c;
const CMD_PTR: u32 = 0x01d5_d798;
const MAGC_PTR: u32 = 0x01c9_5520;
const LVUP_PTR: u32 = 0x01d5_d6b4;

struct Tables {
    items: Vec<Item>,
    cmds: Vec<Cmd>,
    magc: Vec<Magc>,
    plrp: Vec<Plrp>,
    lvup: Vec<Vec<Lvup>>,
    ee: Ee,
}

impl Tables {
    fn index(&self) -> Arc<ItemIndex> {
        Arc::new(ItemIndex::new(&self.items, &self.cmds, &self.magc))
    }
}

/// The disc's tables decoded for the port and laid out in a kh2-ee machine for the original, or `None` after a skip.
fn tables(what: &str) -> Option<Tables> {
    let mut a = kh2_factory::disc(what)?;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    let items = kh2_tables::load::<Item>(&mut a, &t::ITEM).ok()?.rows;
    let cmds = kh2_tables::load::<Cmd>(&mut a, &t::CMD).ok()?.rows;
    let magc = kh2_tables::load::<Magc>(&mut a, &t::MAGC).ok()?.rows;
    let plrp = kh2_tables::load::<Plrp>(&mut a, &t::PLRP).ok()?.rows;
    let blob = |a: &mut kh2_archive::Archive, d: &kh2_tables::TableDef| kh2_tables::subfile(a, d).expect("table");
    let lvup_blob = blob(&mut a, &t::LVUP);
    let lvup = kh2_tables::rt::decode_lvup::<Lvup>(&lvup_blob)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    for (ptr, at, data) in [
        (ITEM_PTR, 0x0180_0000, blob(&mut a, &t::ITEM)),
        (PLRP_PTR, 0x0181_0000, blob(&mut a, &t::PLRP)),
        (CMD_PTR, 0x0182_0000, blob(&mut a, &t::CMD)),
        (MAGC_PTR, 0x0183_0000, blob(&mut a, &t::MAGC)),
        (LVUP_PTR, 0x0184_0000, lvup_blob),
    ] {
        ee.write(at, &data);
        ee.w32(ptr, at);
    }
    // The give's two calls that act outside the party block.
    ee.hooks.insert(0x1803b0, |e| e.gpr[kh2_ee::reg::V0] = 0);
    ee.hooks.insert(0x180450, |e| e.gpr[kh2_ee::reg::V0] = 0);
    Some(Tables { items, cmds, magc, plrp, lvup, ee })
}

fn read(ee: &mut Ee, at: u32, n: usize) -> Vec<u8> {
    (0..n as u32).map(|o| ee.r8(at + o)).collect()
}

fn first_diff(a: &[u8], b: &[u8]) -> String {
    a.iter().zip(b).enumerate().filter(|(_, (x, y))| x != y).map(|(o, (x, y))| format!("+{o:#x}: game {x:#x} port {y:#x}")).take(8).collect::<Vec<_>>().join("; ")
}

fn call(ee: &mut Ee, addr: u32, args: &[u64]) -> u64 {
    ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
}

/// A random party block with plausible content: record fields random, item ids drawn from the table (or 0) in the battle item slots,
/// the stock counts small numbers so the refusal at 99 and the empty stock both happen, the key and obtained bits random, and the
/// recent items, the shortcuts and the three forms' weapon words empty about half the time (the branches that fill them).
fn plausible_party(rng: &mut Rng, ids: &[u16]) -> Vec<u8> {
    let mut b: Vec<u8> = (0..party::LEN).map(|_| rng.word() as u8).collect();
    for m in 0..party::MEMBERS {
        let r = m * party::MEMBER_SIZE;
        b[r + 0x12] = rng.below(9) as u8;
        for i in 0..8 {
            for base in [0x34usize, 0x44] {
                let v = if rng.below(3) == 0 { 0 } else { ids[rng.below(ids.len() as u64) as usize] };
                b[r + base + 2 * i..r + base + 2 * i + 2].copy_from_slice(&v.to_le_bytes());
            }
        }
    }
    for o in 0..party::COUNT_SLOTS {
        b[party::COUNTS + o] = [0, 0, 1, 2, 98, 99, 100, 255][rng.below(8) as usize];
    }
    for k in 0..party::FORMS + 1 {
        if rng.below(2) == 0 {
            let at = party::FORM_BASE + k * party::FORM_SIZE;
            if at + 2 <= party::LEN {
                b[at..at + 2].fill(0);
            }
        }
    }
    if rng.below(2) == 0 {
        b[party::RECENT..party::RECENT + 16].fill(0);
    }
    if rng.below(2) == 0 {
        b[party::SHORTCUTS..party::SHORTCUTS + 8].fill(0);
    }
    if rng.below(3) == 0 {
        // Sparse stock so a keyblade lookup finds none.
        b[party::COUNTS..party::COUNTS + party::COUNT_SLOTS].fill(0);
        for _ in 0..rng.below(6) {
            b[party::COUNTS + rng.below(party::COUNT_SLOTS as u64) as usize] = 1 + rng.below(3) as u8;
        }
    }
    b
}

struct Setup {
    tb: Tables,
    ix: Arc<ItemIndex>,
    ids: Vec<u16>,
}

fn setup(what: &str) -> Option<Setup> {
    let tb = tables(what)?;
    let ix = tb.index();
    let ids: Vec<u16> = ix.rows.iter().map(|r| r.id).filter(|&i| i != 0).collect();
    Some(Setup { tb, ix, ids })
}

/// Drives the original give `FUN_001a18d8(id, to, equipped)` and `Progress::give` on random party blocks with `pick` choosing the item and
/// the target; the answer and the block must agree. Returns (given, refused).
fn give_cases(what: &str, addr: u32, cases: u32, pick: impl Fn(&mut Rng, &Setup) -> (u16, u16, bool)) -> (u32, u32) {
    let Some(mut s) = setup(what) else { return (0, 0) };
    let mut rng = Rng::new(u64::from(addr) ^ 0x27a);
    let (mut given, mut refused) = (0, 0);
    for case in 0..cases {
        let bytes = plausible_party(&mut rng, &s.ids);
        s.tb.ee.write(PARTY, &bytes);
        let mut port = Progress::from_block(&bytes, s.ix.clone());
        let (id, to, eq) = pick(&mut rng, &s);
        let ok = call(&mut s.tb.ee, 0x1a18d8, &[id as u64, to as u64, eq as u64]) as u32 != 0;
        assert_eq!(port.give(id, to, eq), ok, "case {case}: give {id} to {to} (equipped {eq})");
        let game = read(&mut s.tb.ee, PARTY, party::LEN);
        assert!(game == port.block(), "case {case}: give {id} to {to} (equipped {eq}): {}", first_diff(&game, port.block()));
        if ok { given += 1 } else { refused += 1 }
    }
    eprintln!("{addr:#x}: {cases} cases equal ({given} given, {refused} refused)");
    (given, refused)
}

fn of_kind(s: &Setup, kind: u8) -> Vec<u16> {
    s.ix.rows.iter().filter(|r| r.kind == kind && r.id != 0).map(|r| r.id).collect()
}

/// Items of any type into the inventory: `FUN_001a1a18` counts the item (or sets its key bit) and runs the type's side effect.
#[test]
fn orig_1a1a18_matches_the_game() {
    let (given, refused) = give_cases("oracle test of 0x1a1a18", 0x1a1a18, 3000, |rng, s| (s.ids[rng.below(s.ids.len() as u64) as usize], INVENTORY, false));
    assert!(given > 500 && refused > 20, "{given} given, {refused} refused");
}

/// Drive form items (type 0x15): `FUN_001a2010` resets the Anti counter and gives Valor, Master and Final a keyblade for the left hand.
#[test]
fn orig_1a2010_matches_the_game() {
    let Some(s) = setup("oracle test of 0x1a2010") else { return };
    let forms = of_kind(&s, item_type::DRIVE_FORM);
    assert!(!forms.is_empty(), "the disc has form items");
    drop(s);
    let (given, _) = give_cases("oracle test of 0x1a2010", 0x1a2010, 2500, |rng, _| (forms[rng.below(forms.len() as u64) as usize], INVENTORY, false));
    assert!(given > 500, "{given} given");
}

/// The first item of a type with a count (`FUN_001a20b0(2)` takes a keyblade, `FUN_001a1c30` removes it): reached through a form item for
/// Valor, Master or Final with no second keyblade, over stocks that hold a keyblade, several, or none.
#[test]
fn orig_1a20b0_matches_the_game() {
    let Some(s) = setup("oracle test of 0x1a20b0") else { return };
    let forms: Vec<u16> = of_kind(&s, item_type::DRIVE_FORM);
    let keyblades = of_kind(&s, item_type::KEYBLADE);
    assert!(!keyblades.is_empty());
    let slot_of = |id: u16| s.ix.get(id).map(|i| i.slot as usize).unwrap_or(0);
    let key_slots: Vec<usize> = keyblades.iter().map(|&k| slot_of(k)).collect();
    drop(s);
    let Some(mut s) = setup("oracle test of 0x1a20b0") else { return };
    let mut rng = Rng::new(0x1a20b0);
    let mut took = 0;
    for case in 0..2500 {
        let mut bytes = plausible_party(&mut rng, &s.ids);
        // Stock: each keyblade owned about a third of the time.
        for &k in &key_slots {
            bytes[party::COUNTS + k] = if rng.below(3) == 0 { 1 + rng.below(3) as u8 } else { 0 };
        }
        // The three forms with a second keyblade have an empty weapon word.
        for f in [1usize, 3, 4] {
            let at = party::FORM_BASE + f * party::FORM_SIZE;
            bytes[at..at + 2].fill(0);
        }
        s.tb.ee.write(PARTY, &bytes);
        let mut port = Progress::from_block(&bytes, s.ix.clone());
        let id = forms[rng.below(forms.len() as u64) as usize];
        let ok = call(&mut s.tb.ee, 0x1a18d8, &[id as u64, INVENTORY as u64, 0]) as u32 != 0;
        assert_eq!(port.give(id, INVENTORY, false), ok, "case {case}: form item {id}");
        let game = read(&mut s.tb.ee, PARTY, party::LEN);
        assert!(game == port.block(), "case {case}: form item {id}: {}", first_diff(&game, port.block()));
        let before = &bytes[party::COUNTS..party::COUNTS + party::COUNT_SLOTS];
        took += (&game[party::COUNTS..party::COUNTS + party::COUNT_SLOTS] != before) as u32;
    }
    assert!(took > 100, "a keyblade was taken in {took} cases");
}

/// Consumables into the inventory and to Sora (1) or Roxas (14): `FUN_001a2120` notes the item as recent, and `FUN_001a1f78` gives the
/// first one its command as shortcut 0.
#[test]
fn orig_1a2120_matches_the_game() {
    let Some(s) = setup("oracle test of 0x1a2120") else { return };
    let cons = of_kind(&s, item_type::CONSUMABLE);
    drop(s);
    let (given, _) = give_cases("oracle test of 0x1a2120", 0x1a2120, 3000, |rng, _| (cons[rng.below(cons.len() as u64) as usize], [INVENTORY, 1, 14, 2][rng.below(4) as usize], false));
    assert!(given > 500, "{given} given");
}

/// Consumables only: the command of a consumable (`FUN_001a1f78`, type 0 answers the item's own command word) is what lands in shortcut 0.
/// The spell branch of that function (type 0x12) is not reached by the give and belongs to the menu's port (see the report).
#[test]
fn orig_1a1f78_matches_the_game_for_consumables() {
    let Some(mut s) = setup("oracle test of 0x1a1f78") else { return };
    let cons = of_kind(&s, item_type::CONSUMABLE);
    for &id in &cons {
        let game = call(&mut s.tb.ee, 0x1a1f78, &[id as u64]) as u16;
        assert_eq!(game, s.ix.get(id).expect("row").command, "item {id}");
    }
}

/// The inventory count of an item: a key item answers its bit, an ability 0, everything else its count byte.
#[test]
fn orig_1a1cc8_matches_the_game() {
    let Some(mut s) = setup("oracle test of 0x1a1cc8") else { return };
    let mut rng = Rng::new(0x1a1cc8);
    let (mut keys, mut abilities, mut counts) = (0, 0, 0);
    for case in 0..4000 {
        let bytes = plausible_party(&mut rng, &s.ids);
        s.tb.ee.write(PARTY, &bytes);
        let port = Progress::from_block(&bytes, s.ix.clone());
        let id = s.ids[rng.below(s.ids.len() as u64) as usize];
        let game = call(&mut s.tb.ee, 0x1a1cc8, &[id as u64]) as u8;
        assert_eq!(game, port.item_count(id), "case {case}: item {id}");
        match s.ix.get(id).map(|i| (i.is_key(), i.kind)) {
            Some((true, _)) => keys += 1,
            Some((_, item_type::ABILITY)) => abilities += 1,
            _ => counts += 1,
        }
    }
    assert!(keys > 20 && abilities > 20 && counts > 500, "{keys} keys, {abilities} abilities, {counts} counts");
}

/// Removing `n` copies: the count floors at 0, a key item loses its bit.
#[test]
fn orig_1a1c30_matches_the_game() {
    let Some(mut s) = setup("oracle test of 0x1a1c30") else { return };
    let mut rng = Rng::new(0x1a1c30);
    for case in 0..4000 {
        let bytes = plausible_party(&mut rng, &s.ids);
        s.tb.ee.write(PARTY, &bytes);
        let mut port = Progress::from_block(&bytes, s.ix.clone());
        let id = s.ids[rng.below(s.ids.len() as u64) as usize];
        let n = [0u8, 1, 1, 2, 5, 50, 98, 99, 100, 255][rng.below(10) as usize];
        call(&mut s.tb.ee, 0x1a1c30, &[id as u64, n as u64]);
        port.use_item(id, n);
        let game = read(&mut s.tb.ee, PARTY, party::LEN);
        assert!(game == port.block(), "case {case}: remove {n} of {id}: {}", first_diff(&game, port.block()));
    }
}

/// Whether `n` copies would be taken (the answer of `FUN_001a1768` tested against 0) for the inventory and for party members.
#[test]
fn orig_1a18b8_matches_the_game() {
    let Some(mut s) = setup("oracle test of 0x1a18b8") else { return };
    let mut rng = Rng::new(0x1a18b8);
    let (mut yes, mut no) = (0, 0);
    for case in 0..5000 {
        let bytes = plausible_party(&mut rng, &s.ids);
        s.tb.ee.write(PARTY, &bytes);
        let port = Progress::from_block(&bytes, s.ix.clone());
        let id = s.ids[rng.below(s.ids.len() as u64) as usize];
        let to = [INVENTORY, 1, 2, 3, 14, 4, 5][rng.below(7) as usize];
        let n = [1u8, 1, 2, 5, 99][rng.below(5) as usize];
        let game = call(&mut s.tb.ee, 0x1a18b8, &[id as u64, to as u64, n as u64]) & 0xff != 0;
        assert_eq!(game, port.can_get(id, to, n), "case {case}: {n} of {id} to {to}");
        if game { yes += 1 } else { no += 1 }
    }
    assert!(yes > 500 && no > 500, "{yes} yes, {no} no");
}

/// The pause menu's battle item slot assignment `FUN_0026c698(slot, item)`: the old item goes back to the stock, the new one comes off it
/// when it is carried (or 0). The page's objects are stand-ins: the sheet of the current character (`FUN_002829c0`) holds a pointer to the
/// member's record at `+0x238`, the item list entry (`FUN_00283750`) is a 6-byte row whose byte `+2` is the carried flag, and the list
/// row number (`FUN_00298e08`) is 1.
#[test]
fn orig_26c698_matches_the_game() {
    let Some(mut s) = setup("oracle test of 0x26c698") else { return };
    const SHEET: u32 = 0x01f0_0000;
    const LIST: u32 = 0x01f0_1000;
    let ee = &mut s.tb.ee;
    ee.hooks.insert(0x252fb8, |e| e.gpr[kh2_ee::reg::V0] = 0);
    ee.hooks.insert(0x2829c0, |e| e.gpr[kh2_ee::reg::V0] = u128::from(SHEET));
    ee.hooks.insert(0x283750, |e| e.gpr[kh2_ee::reg::V0] = u128::from(LIST));
    ee.hooks.insert(0x298e08, |e| e.gpr[kh2_ee::reg::V0] = 1);
    let mut rng = Rng::new(0x26c698);
    let (mut carried_n, mut left_n) = (0, 0);
    for case in 0..4000 {
        let bytes = plausible_party(&mut rng, &s.ids);
        s.tb.ee.write(PARTY, &bytes);
        let mut port = Progress::from_block(&bytes, s.ix.clone());
        let member = 1 + rng.below(13) as u16;
        let slot = rng.below(8) as usize;
        let item = if rng.below(4) == 0 { 0 } else { s.ids[rng.below(s.ids.len() as u64) as usize] };
        let carried = rng.below(2) == 0;
        let rec = PARTY + (member as u32 - 1) * party::MEMBER_SIZE as u32;
        s.tb.ee.w32(SHEET + 0x238, rec);
        let mut row = [0u8; 6];
        row[2] = carried as u8;
        s.tb.ee.write(LIST, &row);
        let r = call(&mut s.tb.ee, 0x26c698, &[slot as u64, item as u64]) as u32;
        assert_eq!(r, 1);
        assert!(port.equip_battle_item(member, slot, item, carried));
        let game = read(&mut s.tb.ee, PARTY, party::LEN);
        assert!(game == port.block(), "case {case}: member {member} slot {slot} item {item} carried {carried}: {}", first_diff(&game, port.block()));
        if carried || item == 0 { carried_n += 1 } else { left_n += 1 }
    }
    assert!(carried_n > 500 && left_n > 500, "{carried_n} taken, {left_n} reserved only");
}

/// The level-up row of a member's level (`FUN_001b50c0`): the member's table (ids 14 and 15 use 1 and 6), held at its length.
#[test]
fn orig_1b50c0_matches_the_game() {
    let Some(mut s) = setup("oracle test of 0x1b50c0") else { return };
    let mut rng = Rng::new(0x1b50c0);
    for case in 0..4000 {
        let member = 1 + rng.below(15) as u16;
        let level = (1 + rng.below(110)) as u8;
        let at = call(&mut s.tb.ee, 0x1b50c0, &[member as u64, level as u64]) as u32;
        let game = read(&mut s.tb.ee, at, 16);
        let row = lvup_row(&s.tb.lvup, member, level).expect("a row");
        let mut want = Vec::new();
        want.extend_from_slice(&row.exp.to_le_bytes());
        want.extend_from_slice(&[row.strength, row.magic, row.defense, row.ap]);
        assert_eq!(&game[..8], &want[..], "case {case}: member {member} level {level}");
        assert_eq!(u16::from_le_bytes([game[8], game[9]]), row.sword_ability, "case {case}: sword ability");
        assert_eq!(u16::from_le_bytes([game[10], game[11]]), row.shield_ability, "case {case}: shield ability");
        assert_eq!(u16::from_le_bytes([game[12], game[13]]), row.staff_ability, "case {case}: staff ability");
    }
}

/// The limit best counts (`FUN_001d2590` reads, `FUN_001d25b0` raises, the u16 array at `0x3414b0` indexed by the limit row's id byte
/// that the argument points at) and the reaction use counts (`FUN_001b7168`, the u16 array at `0x3410b0` indexed by the command row's
/// halfword at `+0x26`). The port keeps these as maps of the slots that were raised; both sides start from the same array, which the
/// port is seeded with through its public writers.
#[test]
fn orig_1d2590_matches_the_game() {
    limit_cases(0x1d2590);
}

#[test]
fn orig_1d25b0_matches_the_game() {
    limit_cases(0x1d25b0);
}

const LIMITS: u32 = 0x3414b0;
const REACTIONS: u32 = 0x3410b0;
const ROW: u32 = 0x01f0_2000;

fn limit_cases(addr: u32) {
    let Some(mut s) = setup(&format!("oracle test of {addr:#x}")) else { return };
    let mut rng = Rng::new(u64::from(addr));
    for case in 0..3000 {
        // A few slots hold a count (0 for the rest).
        let mut port = Progress::from_block(&vec![0u8; party::LEN], s.ix.clone());
        let mut counts = [0u16; 64];
        for _ in 0..rng.below(8) {
            let slot = rng.below(64) as u8;
            let n = 1 + rng.below(300) as u16;
            counts[slot as usize] = counts[slot as usize].max(n);
            port.raise_limit_best(slot, n);
        }
        for (i, c) in counts.iter().enumerate() {
            s.tb.ee.write(LIMITS + 2 * i as u32, &c.to_le_bytes());
        }
        let slot = rng.below(64) as u8;
        s.tb.ee.write(ROW, &[slot]);
        if addr == 0x1d2590 {
            let game = call(&mut s.tb.ee, addr, &[ROW as u64]) as u16;
            assert_eq!(game, port.limit_best(slot), "case {case}: slot {slot}");
        } else {
            let n = [0u16, 1, 5, 100, 300, 301, 0xffff][rng.below(7) as usize];
            call(&mut s.tb.ee, addr, &[ROW as u64, n as u64]);
            port.raise_limit_best(slot, n);
            for (i, _) in counts.iter().enumerate() {
                let game = s.tb.ee.r16(LIMITS + 2 * i as u32);
                assert_eq!(game, port.limit_best(i as u8), "case {case}: slot {i} after raising {slot} to {n}");
            }
        }
    }
}

#[test]
fn orig_1b7168_matches_the_game() {
    let Some(mut s) = setup("oracle test of 0x1b7168") else { return };
    let mut rng = Rng::new(0x1b7168);
    for case in 0..3000 {
        let mut port = Progress::from_block(&vec![0u8; party::LEN], s.ix.clone());
        let mut counts = [0u16; 128];
        for _ in 0..rng.below(12) {
            let slot = rng.below(128) as u16;
            counts[slot as usize] = 1 + rng.below(998) as u16;
            port.set_reaction_use(slot, counts[slot as usize]);
        }
        for (i, c) in counts.iter().enumerate() {
            s.tb.ee.write(REACTIONS + 2 * i as u32, &c.to_le_bytes());
        }
        let slot = rng.below(128) as u16;
        let mut row = [0u8; 0x30];
        row[0x26..0x28].copy_from_slice(&slot.to_le_bytes());
        s.tb.ee.write(ROW, &row);
        let game = call(&mut s.tb.ee, 0x1b7168, &[ROW as u64]) as u16;
        assert_eq!(game, port.reaction_use(slot), "case {case}: slot {slot}");
    }
}

/// The minigame record address `FUN_001d36e8(n)` = `0x341620 + 8 * n`; the port keeps (state byte, score word) per record, seeded through
/// its writer, and the original's record holds the same two values at the address it answers.
#[test]
fn orig_1d36e8_matches_the_game() {
    let Some(mut s) = setup("oracle test of 0x1d36e8") else { return };
    let mut rng = Rng::new(0x1d36e8);
    for case in 0..3000 {
        let n = rng.below(40) as u16;
        let (state, score) = ([0u8, 1, 2, 4][rng.below(4) as usize], rng.word() as u32);
        let mut port = Progress::from_block(&vec![0u8; party::LEN], s.ix.clone());
        port.set_minigame(n, state, score);
        let at = call(&mut s.tb.ee, 0x1d36e8, &[n as u64]) as u32;
        assert_eq!(at, 0x341620 + 8 * n as u32, "case {case}");
        s.tb.ee.write(at, &[state]);
        s.tb.ee.w32(at + 4, score);
        let got = (s.tb.ee.r8(at), s.tb.ee.r32(at + 4));
        assert_eq!(got, port.minigame(n), "case {case}: record {n}");
    }
}

/// The `magc` row a spell slot uses (`FUN_001a4c78(slot)`): the first row of the id is the base, a row with an item needs that item
/// (`FUN_001a1cc8`, the spell's level), then the later rows of the id whose level is not above the count and whose world is 0 or the
/// current one replace the pick. The original runs on a random `magc` table (ids in runs, levels rising, worlds 0 to 3, items from the
/// disc's item table or 0) and a random party block; the world byte is `0x33e830`.
#[test]
fn orig_1a4c78_matches_the_game() {
    use kh2_game_core::magic_rules::pick_row;
    let Some(mut s) = setup("oracle test of 0x1a4c78") else { return };
    let magc_def = &t::MAGC;
    let mut rng = Rng::new(0x1a4c78);
    let (mut some, mut none, mut later) = (0, 0, 0);
    for case in 0..3000 {
        // The table: runs of rows per id 0..=5, built as raw 0x38-byte rows so both sides read the same bytes.
        let mut raw: Vec<[u8; 0x38]> = Vec::new();
        for id in 0..6u8 {
            if rng.below(6) == 0 {
                continue;
            }
            let item = if rng.below(4) == 0 { 0 } else { s.ids[rng.below(s.ids.len() as u64) as usize] };
            for k in 0..1 + rng.below(4) as u8 {
                let mut r = [0u8; 0x38];
                for b in &mut r[4..0x24] {
                    *b = 0;
                }
                r[0] = id;
                r[1] = k + 1 + (rng.below(2) as u8) * (rng.below(2) as u8);
                r[2] = [0, 0, 1, 2, 3][rng.below(5) as usize];
                r[0x24..0x26].copy_from_slice(&item.to_le_bytes());
                r[0x26..0x28].copy_from_slice(&((rng.word() as u16) | 1).to_le_bytes());
                raw.push(r);
            }
        }
        let mut blob = vec![0u8; 8];
        blob[..4].copy_from_slice(&1u32.to_le_bytes());
        blob[4..8].copy_from_slice(&(raw.len() as u32).to_le_bytes());
        for r in &raw {
            blob.extend_from_slice(r);
        }
        let rows = kh2_tables::rt::decode::<Magc>(magc_def, &blob).expect("decodes").rows;
        assert_eq!(rows.len(), raw.len());
        s.tb.ee.write(0x0183_0000, &blob);
        let bytes = plausible_party(&mut rng, &s.ids);
        s.tb.ee.write(PARTY, &bytes);
        let progress = Progress::from_block(&bytes, s.ix.clone());
        let world = rng.below(4) as u8;
        s.tb.ee.write(0x33e830, &[world]);
        let id = rng.below(7) as u8;
        let at = call(&mut s.tb.ee, 0x1a4c78, &[id as u64]) as u32;
        let first = rows.iter().position(|r| r.id == id);
        let owned = first.map_or(0, |f| if rows[f].item != 0 { progress.item_count(rows[f].item) } else { 0 });
        let want = pick_row(&rows, id, owned, world);
        match want {
            Some(r) => {
                let idx = (r as *const Magc as usize - rows.as_ptr() as usize) / std::mem::size_of::<Magc>();
                assert_eq!(at, 0x0183_0008 + 0x38 * idx as u32, "case {case}: id {id}, world {world}, owned {owned}, rows {raw:x?}");
                some += 1;
                later += (Some(idx) != first) as u32;
            }
            None => {
                assert_eq!(at, 0, "case {case}: id {id}, world {world}, owned {owned}");
                none += 1;
            }
        }
    }
    assert!(some > 500 && none > 300 && later > 100, "{some} rows, {none} none, {later} later rows");
}

/// Whether the chest with the record at the argument has been opened (`FUN_001a6a90(record)`): the halfword at record `+10` is the chest
/// index `n`; 0 means none; otherwise bit `n` of the record bit words from `0x33fdcc` (the record array of `0x33fdb0` from bit 224).
#[test]
fn orig_1a6a90_matches_the_game() {
    let Some(mut s) = setup("oracle test of 0x1a6a90") else { return };
    let mut rng = Rng::new(0x1a6a90);
    let (mut yes, mut no) = (0, 0);
    for case in 0..4000 {
        let mut port = Progress::from_block(&vec![0u8; party::LEN], s.ix.clone());
        let words: Vec<u32> = (0..8).map(|_| if rng.below(3) == 0 { 0 } else { rng.word() as u32 }).collect();
        for (i, w) in words.iter().enumerate() {
            s.tb.ee.w32(0x33fdcc + 4 * i as u32, *w);
            for b in 0..32u16 {
                if w >> b & 1 != 0 && i * 32 + b as usize != 0 {
                    port.set_chest_opened((i * 32) as u16 + b);
                }
            }
        }
        let n = if rng.below(10) == 0 { 0 } else { rng.below(256) as u16 };
        let mut rec = [0u8; 16];
        rec[10..12].copy_from_slice(&n.to_le_bytes());
        s.tb.ee.write(ROW, &rec);
        let game = call(&mut s.tb.ee, 0x1a6a90, &[ROW as u64]) & 0xff != 0;
        assert_eq!(game, port.chest_opened(n), "case {case}: chest {n}");
        if game { yes += 1 } else { no += 1 }
    }
    assert!(yes > 500 && no > 500, "{yes} opened, {no} not");
}

/// `FUN_00180948` (`AREADATA::Destroy`) is an empty function: `jr ra` and a delay slot with no effect. The port's callers do nothing for it.
/// The test runs it over random registers and a random memory word, and checks that the return value register and the word are
/// untouched and that the code is exactly those two instructions.
#[test]
fn orig_180948_matches_the_game() {
    let Some(mut s) = setup("oracle test of 0x180948") else { return };
    assert_eq!((s.tb.ee.r32(0x180948), s.tb.ee.r32(0x18094c)), (0x03e0_0008, 0), "jr ra, nop");
    let mut rng = Rng::new(0x180948);
    for _ in 0..200 {
        let v = rng.word() as u32;
        s.tb.ee.w32(ROW, v);
        let r = call(&mut s.tb.ee, 0x180948, &[rng.word(), rng.word()]);
        let _ = r;
        assert_eq!(s.tb.ee.r32(ROW), v);
    }
}
