//! The "NEW!" tags of the pause menu (docs/menus.md 16.1).
//!
//! What the original does (`SLUS_210.05`): the camp keeps two bit arrays over the item ids (0x220 bits). One is
//! the **seen** array in the save block (`FUN_002937e8` = save base + 0x31b4); the other is built when the camp
//! starts (`FUN_00293688`, `FUN_00293808`): `FUN_00293838` marks the equipment the party wears as seen when the
//! camp ends (it fills a third array that `FUN_00293758` merges into the seen array on leaving),
//! `FUN_00293de8` sets the **fresh** bit of every owned item whose type is the spell type (0x12, `FUN_00293ea0`)
//! or one of the types listed at 0x382808 (`FUN_00293b60`) and whose seen bit is clear. A row is NEW when its fresh
//! bit is set (`FUN_00293ba0`, spells `FUN_00293eb0`); the tag shows the **viewed** form once its seen bit is set as
//! well (`FUN_00293c78`, spells `FUN_00293fa8`). `FUN_00293d98(id)` sets a seen bit; the Stock grid calls it when the
//! cursor reaches a NEW cell (`FUN_0025dcf8`), which flips that cell's tag from the group at 0x381070 (0x32) to
//! the one at 0x381074 (0x31). On page 0 `FUN_00251100` tests the whole set: any fresh item in the list types shows
//! the tag on the Items row, any fresh spell on Customize, any fresh ability on Abilities; the tag is group 0x32
//! (`DAT_003807e8`) while one of them is still unseen and 0x31 (`DAT_003807ec`) when every fresh one has been seen.
//!
//! Abilities (`FUN_00294470`): the save block keeps a list of every ability the leader ever had in his slots
//! (u16 x 0x40 at +0x98 of the same region) and one seen bit per list index (+0x44). The camp appends the
//! abilities of the slots that are not in the list yet, a list entry is fresh when its seen bit is clear
//! (`FUN_00294b58`), and an entry whose ability has left the slots is marked seen. Highlighting an ability row
//! sets its seen bit (`FUN_0027be78` -> `FUN_00295178`).
//!
//! The app keeps these arrays in a resource for the session; the original keeps them in the save block (the app has
//! no save file for them yet). Status (summon items, `FUN_00294260`) and the Journal (`DAT_0036cedc`) have no source
//! in the app, so their rows never carry a tag.

use bevy::prelude::*;

use crate::assets::Disc;
use crate::menu::item_pages::ItemRow;
use crate::menu_pages::Exe;

/// Bits of the arrays (`FUN_00293688` and the loops of 0x220).
pub const BITS: usize = 0x220;
const WORDS: usize = BITS / 32 + 1;
/// The item type of the spells (`FUN_00293ea0`).
pub const SPELL_TYPE: u8 = 0x12;
/// Where the list types are (`FUN_00293b60` walks it until a negative byte).
const TYPE_LIST: u32 = 0x0038_2808;
/// Tag groups of `camp.2ld` sequence 1: `DAT_003807e8` (something unseen) and `DAT_003807ec` (all seen), and the
/// cell forms `DAT_00381070` (0x32) and `DAT_00381074` (0x31). The equipment lists use 0x3819e0 / 0x3819e4.
pub const NEW: usize = 0x32;
pub const VIEWED: usize = 0x31;
/// The abilities' list capacity (`FUN_00294470` stops at 0x40).
const KNOWN_MAX: usize = 0x40;

/// Which rows of page 0 test which set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Set {
    /// Entry 0, Items: owned items of the listed types.
    Items,
    /// Entry 2, Customize: owned spells.
    Spells,
    /// Entry 1, Abilities: the leader's ability list.
    Abilities,
}

/// The tag state, in the order the game builds it.
#[derive(Resource, Default, Debug, Clone)]
pub struct NewTags {
    /// The item types the Items set lists (the bytes at 0x382808).
    types: Vec<u8>,
    /// The type of every item id the camp saw at its start (0xff: none).
    kind: Vec<u8>,
    seen: [u32; WORDS],
    /// The seen words came from the loaded progress once (`load_once`).
    synced: bool,
    fresh: [u32; WORDS],
    /// Abilities the leader ever had, in the order he got them, and the seen and fresh flags per index.
    known: Vec<u16>,
    known_seen: Vec<bool>,
    known_fresh: Vec<bool>,
}

fn bit(words: &[u32; WORDS], id: usize) -> bool {
    id < BITS && words[id >> 5] >> (id & 31) & 1 != 0
}

fn set_bit(words: &mut [u32; WORDS], id: usize) {
    if id < BITS {
        words[id >> 5] |= 1 << (id & 31);
    }
}

/// The type list from the executable.
pub fn read_types(exe: &Exe) -> Vec<u8> {
    (0..32u32).map_while(|k| exe.u8(TYPE_LIST + k).filter(|&b| b < 0x80)).collect()
}

impl NewTags {
    /// A state with the given list types (tests).
    #[cfg(test)]
    pub fn with_types(types: &[u8]) -> NewTags {
        NewTags { types: types.to_vec(), ..Default::default() }
    }

    /// The camp starts (`FUN_00293808`: `FUN_00293838`, `FUN_00293de8`, `FUN_002940e0`, `FUN_00294470`).
    /// `rows` is the item table, `count` the stock, `abilities` the leader's ability ids in slot order.
    pub fn begin(&mut self, rows: &[ItemRow], count: &dyn Fn(u16) -> u8, abilities: &[u16]) {
        self.fresh = [0; WORDS];
        self.kind = vec![0xff; BITS];
        for r in rows {
            let id = r.id as usize;
            if id >= BITS {
                continue;
            }
            self.kind[id] = r.kind;
            let listed = r.kind == SPELL_TYPE || self.types.contains(&r.kind);
            if listed && count(r.id) > 0 && !bit(&self.seen, id) {
                set_bit(&mut self.fresh, id);
            }
        }
        // `FUN_00294470`: append what the slots hold and the list lacks, then fresh = not seen.
        for &a in abilities {
            if a != 0 && !self.known.contains(&a) && self.known.len() < KNOWN_MAX {
                self.known.push(a);
                self.known_seen.push(false);
            }
        }
        self.known_fresh = self.known_seen.iter().map(|&s| !s).collect();
        // An ability that left the slots is marked seen so it never shows as NEW later.
        for (i, a) in self.known.clone().into_iter().enumerate() {
            if !abilities.contains(&a) {
                self.known_seen[i] = true;
            }
        }
    }

    /// The seen array as words (what a slot file stores).
    pub fn seen_words(&self) -> Vec<u32> {
        self.seen.to_vec()
    }

    /// Replace the seen array with stored words (up to `WORDS`).
    pub fn set_seen_words(&mut self, w: &[u32]) {
        self.seen = [0; WORDS];
        let n = w.len().min(WORDS);
        self.seen[..n].copy_from_slice(&w[..n]);
    }

    /// Take the progress's stored words the first time only (a slot loaded at start); later the tags own them.
    pub fn load_once(&mut self, w: &[u32]) {
        if !self.synced {
            self.synced = true;
            if !w.is_empty() {
                self.set_seen_words(w);
            }
        }
    }

    /// The camp ends (`FUN_00293758`): what the party wears counts as seen.
    pub fn end(&mut self, worn: &[u16]) {
        for &id in worn {
            set_bit(&mut self.seen, id as usize);
        }
    }

    /// `FUN_00293ba0(id)`: the id's fresh bit.
    pub fn is_new(&self, id: u16) -> bool {
        bit(&self.fresh, id as usize)
    }

    /// `FUN_00293c78(id)`: fresh and seen.
    pub fn is_viewed(&self, id: u16) -> bool {
        self.is_new(id) && bit(&self.seen, id as usize)
    }

    /// `FUN_00293d98(id)`: set the seen bit (the original sets it for any id; only a fresh one changes a tag).
    pub fn view(&mut self, id: u16) -> bool {
        let before = bit(&self.seen, id as usize);
        set_bit(&mut self.seen, id as usize);
        !before && self.is_new(id)
    }

    /// The group of the tag one cell carries (0x32 unseen, 0x31 seen), `None` without a tag.
    pub fn cell_group(&self, id: u16) -> Option<usize> {
        self.is_new(id).then(|| if self.is_viewed(id) { VIEWED } else { NEW })
    }

    fn fresh_of(&self, set: Set) -> Vec<usize> {
        (0..BITS)
            .filter(|&id| bit(&self.fresh, id))
            .filter(|&id| {
                let k = self.kind.get(id).copied().unwrap_or(0xff);
                match set {
                    Set::Spells => k == SPELL_TYPE,
                    _ => k != SPELL_TYPE && self.types.contains(&k),
                }
            })
            .collect()
    }

    /// The tag of a page 0 row: `FUN_00251100`'s test of the whole set. `None`: no fresh member.
    pub fn row_group(&self, set: Set) -> Option<usize> {
        match set {
            Set::Abilities => {
                let any = self.known_fresh.iter().any(|&f| f);
                let all_seen = self.known_fresh.iter().zip(&self.known_seen).all(|(&f, &s)| !f || s);
                any.then_some(if all_seen { VIEWED } else { NEW })
            }
            _ => {
                let fresh = self.fresh_of(set);
                (!fresh.is_empty()).then(|| if fresh.iter().all(|&id| bit(&self.seen, id)) { VIEWED } else { NEW })
            }
        }
    }

    /// The set a page 0 entry tests (`FUN_00251100`: entries 0, 1, 2; 4 and 5 have no source here).
    pub fn set_of_entry(entry: usize) -> Option<Set> {
        match entry {
            0 => Some(Set::Items),
            1 => Some(Set::Abilities),
            2 => Some(Set::Spells),
            _ => None,
        }
    }

    /// An ability row's tag (`FUN_0027b9a8`: the row's index in the list, `FUN_00294b58`, then its seen bit).
    pub fn ability_group(&self, ability: u16) -> Option<usize> {
        let i = self.known.iter().position(|&a| a == ability)?;
        self.known_fresh.get(i).copied().filter(|&f| f).map(|_| if self.known_seen[i] { VIEWED } else { NEW })
    }

    /// `FUN_00295178`: highlighting an ability row sets its seen bit.
    pub fn view_ability(&mut self, ability: u16) -> bool {
        let Some(i) = self.known.iter().position(|&a| a == ability) else { return false };
        let changed = self.known_fresh[i] && !self.known_seen[i];
        self.known_seen[i] = true;
        changed
    }

    /// One line for the log: the fresh and seen counts.
    pub fn summary(&self) -> String {
        format!(
            "new items {}, new spells {}, abilities known {} (fresh {})",
            self.fresh_of(Set::Items).len(),
            self.fresh_of(Set::Spells).len(),
            self.known.len(),
            self.known_fresh.iter().filter(|&&f| f).count()
        )
    }
}

pub struct NewTagsPlugin;

impl Plugin for NewTagsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NewTags>().add_systems(Startup, load);
    }
}

fn load(disc: Res<Disc>, mut tags: ResMut<NewTags>) {
    match disc.read_root("SLUS_210.05").and_then(Exe::load) {
        Some(exe) => tags.types = read_types(&exe),
        None => warn!("menu: the executable is not readable; the NEW tags are off"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u16, kind: u8) -> ItemRow {
        ItemRow { id, kind, amount: 0, name: 0, description: 0, sell: 0, icon: 0 }
    }

    fn table() -> Vec<ItemRow> {
        vec![row(1, 0), row(2, 0), row(10, 0x12), row(11, 0x12), row(40, 2), row(50, 0x13)]
    }

    fn own(id: u16) -> u8 {
        match id {
            1 | 10 | 40 | 50 => 1,
            _ => 0,
        }
    }

    #[test]
    fn seen_bits_round_trip_through_a_slot_file() {
        let mut t = NewTags::with_types(&[0]);
        t.view(77);
        let mut p = crate::progress::Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        p.set_new_seen(&t.seen_words());
        let info = crate::progress::SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt00".into() };
        let mut q = crate::progress::Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        q.apply_json(&p.slot_json(&info)).expect("parses");
        let mut u = NewTags::with_types(&[0]);
        u.load_once(q.new_seen());
        assert!(bit(&u.seen, 77) && !bit(&u.seen, 78));
    }

    #[test]
    fn owned_unseen_items_of_the_listed_types_are_fresh() {
        let mut t = NewTags::with_types(&[0, 2]);
        t.begin(&table(), &own, &[]);
        assert!(t.is_new(1) && t.is_new(40), "owned items of listed types");
        assert!(!t.is_new(2), "not owned");
        assert!(!t.is_new(50), "type 0x13 is not listed");
        assert!(t.is_new(10), "owned spell");
        assert_eq!(t.row_group(Set::Items), Some(NEW));
        assert_eq!(t.row_group(Set::Spells), Some(NEW));
    }

    #[test]
    fn seeing_every_fresh_member_turns_the_row_tag_to_the_viewed_form() {
        let mut t = NewTags::with_types(&[0, 2]);
        t.begin(&table(), &own, &[]);
        assert_eq!(t.cell_group(1), Some(NEW));
        assert!(t.view(1));
        assert_eq!(t.cell_group(1), Some(VIEWED));
        assert_eq!(t.row_group(Set::Items), Some(NEW), "item 40 is still unseen");
        t.view(40);
        assert_eq!(t.row_group(Set::Items), Some(VIEWED));
        assert!(!t.view(1), "a second look changes nothing");
        assert_eq!(t.row_group(Set::Spells), Some(NEW), "spells are their own set");
    }

    #[test]
    fn a_seen_item_is_not_fresh_at_the_next_start_and_worn_items_become_seen() {
        let mut t = NewTags::with_types(&[0, 2]);
        t.begin(&table(), &own, &[]);
        t.view(1);
        t.end(&[40]);
        t.begin(&table(), &own, &[]);
        assert!(!t.is_new(1) && !t.is_new(40), "seen last time");
        assert_eq!(t.row_group(Set::Items), None);
        assert_eq!(t.cell_group(1), None);
    }

    #[test]
    fn abilities_are_new_until_their_row_is_highlighted() {
        let mut t = NewTags::with_types(&[0]);
        t.begin(&table(), &own, &[0x91, 0x92]);
        assert_eq!(t.row_group(Set::Abilities), Some(NEW));
        assert_eq!(t.ability_group(0x91), Some(NEW));
        assert!(t.view_ability(0x91));
        assert_eq!(t.ability_group(0x91), Some(VIEWED));
        assert_eq!(t.row_group(Set::Abilities), Some(NEW), "0x92 is unseen");
        t.view_ability(0x92);
        assert_eq!(t.row_group(Set::Abilities), Some(VIEWED));
        // A new ability appended later is fresh; an ability that left the slots never is.
        t.begin(&table(), &own, &[0x91, 0x93]);
        assert_eq!(t.ability_group(0x93), Some(NEW));
        assert_eq!(t.ability_group(0x91), None, "already seen");
        assert_eq!(t.ability_group(0x92), None);
    }

    #[test]
    fn entries_map_to_the_sets_the_page_tests() {
        assert_eq!(NewTags::set_of_entry(0), Some(Set::Items));
        assert_eq!(NewTags::set_of_entry(1), Some(Set::Abilities));
        assert_eq!(NewTags::set_of_entry(2), Some(Set::Spells));
        assert_eq!(NewTags::set_of_entry(4), None);
    }

    /// The type list in the executable (skipped without `KH2_ISO`): the bytes the doc quotes.
    #[test]
    fn the_executables_type_list() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let Some(elf) = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()) else { return };
        let exe = Exe::load(elf).expect("an ELF");
        assert_eq!(read_types(&exe), vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 6, 14, 15, 16, 17, 20]);
    }
}
