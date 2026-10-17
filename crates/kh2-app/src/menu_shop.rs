//! The shops: menu kind 4 (`itemshop.2ld`; docs/menus.md section 21) and the reaction commands that open it.
//!
//! What the original does (`SLUS_210.05`, read in WP-SH1):
//! - A shopkeeper's reaction part offers one of the "Shop" commands (cmd rows 195..213, 453 and 622; 195..212 have message 0x84c7, Execute 0x7f;
//!   Twilight Town's are 209 and 212 on `N_EX700_TT_WEAPON_RTN` and `N_EX650_TT_ITEM_RTN` in tt06). Sora's class handler
//!   gives Execute 0x7f to `FUN_00177b18`, whose case 0x7f is `FUN_00197b70(pending command +6, 0)`: a field event.
//!   [inferred: the halfword is the row's Argument (cmd +4); the writer of the pending record's +6 was not traced. Every
//!   Shop row's Argument is one of the shop events.] Command 30 is "Examine" (message 0x81eb), not a shop: its accept
//!   (`FUN_001a54a8`) runs the target's talk program in the top window class `FUN_001ab7d8` (npc.rs).
//! - `FUN_00197b70` sends events 0x13, 0x66..0x75, 0x83..0x85 and 0x8a to `FUN_0024f0b8(4, event)`: menu kind 4 with
//!   the event as its mode word (a scene change: the field stops).
//! - Kind 4's init `FUN_0025f168` and its task `FUN_0025fc18` load `itemshop.2ld` (menu file 4), build the screen
//!   (`FUN_0025fcd8`), play the open sound (kind 0x10 when the shop has the third row, else 0xb) and run the page
//!   machine `FUN_00260540` (page `DAT_0036d1d4`): 0 the top rows (`FUN_00260748`), 1 / 4 the buy / sell list
//!   (`FUN_00260ec8`), 2 / 5 the quantity (`FUN_002614e0`, `FUN_00261a28`), 3 / 6 the question (`FUN_00261d70`), 7 the
//!   first-visit text (`FUN_002621a0`), 8.. the third row's pages (`FUN_00285388` ..).
//! - The stock (`FUN_00265688(mode word, shop state)`): the shop file is `03system.bin`'s type-0x29 entry ("shop",
//!   magic `TZSH`, version 7, `FUN_00265660`), kept by `FUN_00193610` -> `FUN_00265ac0`. See `Shops` and `stock`.
//!
//! What the port does: docs/menus.md section 21 ("What the port does").

use bevy::prelude::*;
use kh2_image::layd::{self, Layd};
use kh2_image::text::{Style, Token};
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::menu::menu_sound;
use crate::menu_gfx::{self, MenuScene, Node};
use crate::menu_pages::Exe;
use crate::menu_text::{Label, MenuText};
use crate::progress::{Moogle, Progress};
use crate::schedule::Pause;

/// The message ids of the shop pages (`msg/us/sys.bar`): the question's rows (`FUN_00261bf8`: row 0 = 0x8ad0 "No",
/// row 1 = 0x8acf "Yes"; the cursor starts on row 0).
pub mod msg {
    pub const NO: u32 = 0x8ad0;
    pub const YES: u32 = 0x8acf;
}

/// Addresses in the executable.
pub mod addr {
    /// The four top rows (`FUN_0025fef0`): 0x24 bytes each, the camp entry layout (docs/menus.md 2.1): name +0x12,
    /// help +0x14, up to four record bits that must be set at +0x1c (0xff ends).
    pub const ROWS: u32 = 0x0038_1300;
    pub const ROW_SIZE: u32 = 0x24;
    /// The layout triples (enter, leave, hold) by screen (`FUN_00263f00`, `FUN_0025fcd8`): 0 the top rows, 1 the buy
    /// list, 2 the sell list [inferred: the cancel of page 1 passes 1, of page 4 passes 2].
    pub const LAYOUTS: u32 = 0x0038_1240;
    /// The first-visit texts (`FUN_00262140`): the message of a shop whose state `+0xd` is clear at 0x381128, set at
    /// 0x38112c (u32 each).
    pub const FIRST_VISIT: u32 = 0x0038_1128;
}

/// The sound kinds the pages play (`FUN_0023e738(kind, 1)`), as ids of the system bank: kind k is the u16 at
/// 0x37f654 + 2 k [inferred: the rule the docs found for kinds 0xc and 0xd]. Kind 0xb opens a shop without the third
/// row, 0x10 one with it; 8 is the question's "No", 0xf the deal.
pub mod snd {
    pub const OPEN: u32 = 0x03;
    pub const OPEN_THIRD_ROW: u32 = 0x28;
    pub const BACK: u32 = 0x02;
    pub const DEAL: u32 = 0x26;
}

/// The events `FUN_00197b70` sends to menu kind 4 (`FUN_0024f0b8(4, event)`).
pub fn opens_shop(event: u32) -> bool {
    matches!(event, 0x13 | 0x66..=0x75 | 0x83..=0x85 | 0x8a)
}

/// The Execute value of the rows whose accept fires their event (`FUN_00177b18` case 0x7f).
pub const EXECUTE_EVENT: u16 = 0x7f;

/// One record of the shop file (0x18 bytes from +0x10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShopRecord {
    /// +0: the event (menu kind 4's mode word) the record answers.
    pub event: u16,
    /// +2: a record bit (the bit array 0x33fdb0) `FUN_00265688` sets on the first open.
    pub bit: u16,
    /// +4: copied to the state's +0 (the header text, `FUN_0025fef0` -> `FUN_0023bcd0(.., *state, ..)`).
    pub message: u16,
    /// +6, +8, +0xa, +0xc: copied to the state's +2..+8 [GAP: their readers were not traced].
    pub words: [u16; 4],
    /// +0xe: bit 0x80 the record takes no own list into account for the "first group" byte and pulls the always-on
    /// groups of the other shops; 0x100 the Buy and Sell rows; 0x200 the third row.
    pub flags: u16,
    /// +0x10 count, +0x14 offset: the item groups.
    pub groups: Vec<Group>,
}

/// An item group of a record (8 bytes: i16 record bit, u16 count, u32 offset to `count` u16 item ids).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// Negative: always on; else the group is on while this record bit is set (`FUN_001a5328`).
    pub bit: i16,
    pub items: Vec<u16>,
}

/// The shop file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shops {
    pub records: Vec<ShopRecord>,
    /// +0xc: the offset of a list of up to 0x80 item ids; an item's place in it is its bit in the save's "seen in a
    /// shop" array (save +0x3714, `FUN_00265ad0` / `FUN_00265280`).
    pub seen_ids: Vec<u16>,
}

/// The magic (`FUN_00265660`): "TZSH" read as a little-endian u32, and the version (+4).
pub const MAGIC: u32 = 0x4853_5a54;
pub const VERSION: u16 = 7;

impl Shops {
    pub fn parse(d: &[u8]) -> Option<Shops> {
        let u16_at = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let u32_at = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if u32_at(0)? != MAGIC || u16_at(4)? != VERSION {
            return None;
        }
        let n = u16_at(6)? as i16;
        let mut records = Vec::new();
        for i in 0..n.max(0) as usize {
            let r = 0x10 + 0x18 * i;
            let w = |k: usize| u16_at(r + 2 * k);
            let (count, off) = (w(8)? as usize, u32_at(r + 0x14)? as usize);
            let mut groups = Vec::new();
            for g in 0..count {
                let o = off + 8 * g;
                let (bit, k, ids) = (u16_at(o)? as i16, u16_at(o + 2)? as usize, u32_at(o + 4)? as usize);
                groups.push(Group { bit, items: (0..k).map(|j| u16_at(ids + 2 * j)).collect::<Option<Vec<_>>>()? });
            }
            records.push(ShopRecord { event: w(0)?, bit: w(1)?, message: w(2)?, words: [w(3)?, w(4)?, w(5)?, w(6)?], flags: w(7)?, groups });
        }
        // FUN_00265ad0 reads at most 0x80 ids; the list ends where the file ends.
        let seen_off = u32_at(0xc)? as usize;
        let seen_ids = (0..0x80).map_while(|j| u16_at(seen_off + 2 * j)).collect();
        Some(Shops { records, seen_ids })
    }

    pub fn find(&self, event: u16) -> Option<&ShopRecord> {
        self.records.iter().find(|r| r.event == event)
    }
}

/// `FUN_00265a50`: the kind bit of an item type (+2 of its row), matched against the record's mask.
pub fn type_bit(kind: u8) -> u16 {
    match kind {
        0 | 1 => 8,
        2..=0xd => 1,
        0xe => 2,
        0xf => 4,
        0x10 => 0x10,
        _ => 0,
    }
}

/// The shop's state for one open (`FUN_00265688` writes it; the state is 0x820 bytes at `DAT_0036cfbc`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stock {
    pub message: u16,
    pub words: [u16; 4],
    /// +0xc bit 0 (record flag 0x100) and bit 1 (0x200).
    pub rows_bits: u8,
    /// +0xd: 1 when the first group has items and the record flag 0x80 is clear [GAP: its meaning; it picks the
    /// first-visit text 0x38112c over 0x381128 and the bit of save +0x3724].
    pub first_group: bool,
    /// +0xa count, +0x10 ids: the buy list.
    pub items: Vec<u16>,
    /// The record bit the open sets when it was clear (`FUN_001a52b0`).
    pub set_bit: Option<u16>,
}

/// `FUN_00265688`: the stock of the shop for `event`. `bit` tests the record bit array 0x33fdb0; `kind` answers an
/// item's type byte. The record's own groups add their items when the group bit is negative or set, with the mask
/// 0x1f (every kind bit); with the record flag 0x80 the always-on groups of every other record whose own bit is set
/// and whose flag 0x80 is clear add theirs too, with the record's flags word as the mask. An item is added once.
pub fn stock(shops: &Shops, event: u16, bit: &dyn Fn(u16) -> bool, kind: &dyn Fn(u16) -> Option<u8>) -> Option<Stock> {
    let r = shops.find(event)?;
    let mut s = Stock {
        message: r.message,
        words: r.words,
        rows_bits: u8::from(r.flags & 0x100 != 0) | (u8::from(r.flags & 0x200 != 0) << 1),
        first_group: r.flags & 0x80 == 0 && r.groups.first().is_some_and(|g| !g.items.is_empty()),
        items: Vec::new(),
        set_bit: (!bit(r.bit)).then_some(r.bit),
    };
    let add = |s: &mut Stock, ids: &[u16], mask: u16| {
        for &id in ids {
            if !s.items.contains(&id) && kind(id).is_some_and(|k| type_bit(k) & mask != 0) {
                s.items.push(id);
            }
        }
    };
    for g in &r.groups {
        if g.bit < 0 || bit(g.bit as u16) {
            add(&mut s, &g.items, 0x1f);
        }
    }
    if r.flags & 0x80 != 0 {
        for o in shops.records.iter().filter(|o| o.event != event && bit(o.bit) && o.flags & 0x80 == 0) {
            for g in o.groups.iter().filter(|g| g.bit < 0) {
                add(&mut s, &g.items, r.flags);
            }
        }
    }
    Some(s)
}

/// `FUN_00264da8` and `FUN_0025fef0`: which of the four top rows show. Rows 0 and 1 need the rows bit 0 and a
/// non-empty list, row 2 the rows bit 1, row 3 always. (`FUN_0025fef0` also needs the row table's flag ids, +0x1c,
/// all set; the disc's rows name none.)
pub fn top_rows(s: &Stock) -> Vec<usize> {
    (0..4).filter(|&k| match k {
        0 | 1 => s.rows_bits & 1 != 0 && !s.items.is_empty(),
        2 => s.rows_bits & 2 != 0,
        _ => true,
    })
    .collect()
}

/// `FUN_00265ad0`: an item's place in the shop file's list of seen-marks (at most 0x80).
pub fn item_place(shops: &Shops, id: u16) -> Option<usize> {
    shops.seen_ids.iter().take(0x80).position(|&x| x == id)
}

/// `FUN_00265280` (run by `FUN_0025fef0` at the open): the places of the buy list's items whose save bit (`+0x3714`) is
/// still clear. They are the shop's NEW items for this visit, as a bit set over the 0x80 places.
pub fn fresh_at_open(shops: &Shops, items: &[u16], seen: &dyn Fn(usize) -> bool) -> u128 {
    let mut fresh = 0u128;
    for &id in items {
        if let Some(p) = item_place(shops, id).filter(|&p| !seen(p)) {
            fresh |= 1 << p;
        }
    }
    fresh
}

/// A NEW mark of a shop row: group 0x78 (`DAT_00381238`) while the item was not seen, 0x77 (`DAT_0038123c`) once the
/// cursor has been on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    New,
    Viewed,
}

/// `FUN_002626e8` (buy list rows): a row of an item that was fresh at the open (`FUN_002653d8`) carries a tag, NEW until
/// its save bit is set (`FUN_00265420`, set by `FUN_00265348` when the cursor reaches it), then VIEWED.
pub fn item_tag(shops: &Shops, fresh: u128, id: u16, seen: &dyn Fn(usize) -> bool) -> Option<Tag> {
    let p = item_place(shops, id).filter(|&p| fresh >> p & 1 != 0)?;
    Some(if seen(p) { Tag::Viewed } else { Tag::New })
}

/// `FUN_00262bd0` (the top page): the Buy row carries a tag when any item was fresh at the open: NEW while one of them is
/// still unseen, else VIEWED.
pub fn buy_tag(fresh: u128, seen: &dyn Fn(usize) -> bool) -> Option<Tag> {
    if fresh == 0 {
        return None;
    }
    Some(if (0..0x80).any(|p| fresh >> p & 1 != 0 && !seen(p)) { Tag::New } else { Tag::Viewed })
}

/// `FUN_00265498`: the first-visit text (page 7) shows unless the shop is the third row only (`rows_bits` 2 and the state's
/// `+0xd` clear), and only while the save's `+0x3724` bit `1 << first_group` is clear. Answers that bit.
pub fn first_visit_bit(s: &Stock, visited: &dyn Fn(u8) -> bool) -> Option<u8> {
    let third_only = s.rows_bits & 2 != 0 && !s.first_group;
    let bit = u8::from(s.first_group);
    (!third_only && !visited(bit)).then_some(bit)
}

// --- the item rows the pages read -------------------------------------------------------------------

/// What the shop reads of an item row (`item`, 0x18 bytes): `Type` (+2), `Name` (+8), `ShopBuy` (+0xc), `ShopSell`
/// (+0xe). `FUN_00264b98` answers the row of a list entry; the buy price is its +0xc, the sell price its +0xe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ware {
    pub id: u16,
    pub kind: u8,
    pub name: u16,
    pub buy: u16,
    pub sell: u16,
    /// +5: the rank (a synthesis material's points, `synth::points`).
    pub rank: u8,
    /// +4: the material's category (the collection rewards' first condition byte, `FUN_00291560`).
    pub flag1: u8,
}

impl Ware {
    pub fn of(i: &t::Item) -> Ware {
        Ware { id: i.id, kind: i.r#type, name: i.name, buy: i.shop_buy, sell: i.shop_sell, rank: i.rank, flag1: i.flag1 }
    }
}

/// `FUN_00264be8`: the sell list, in item table order (`FUN_001a6730` walks the rows): an item of a type below 0x12
/// (`FUN_00255588`) that the player has (`FUN_001a1cc8`) and whose sell price is not 0.
pub fn sell_list(wares: &[Ware], count: &dyn Fn(u16) -> u8) -> Vec<(u16, u8)> {
    wares.iter().filter(|w| w.kind < 0x12 && w.sell != 0).map(|w| (w.id, count(w.id))).filter(|&(_, n)| n > 0).collect()
}

/// `FUN_00265550` on the buy page: a row is refused (the refused sound, nothing changes) when the player has 99 of it
/// already (more than 0x62) or its price is above the munny; on the sell page when its sell price is 0.
pub fn row_open(sell: bool, w: &Ware, owned: u8, munny: u32) -> bool {
    if sell { w.sell != 0 } else { owned <= 0x62 && u32::from(w.buy) <= munny }
}

/// `FUN_00261a28`: one quantity key. `d` is +1 (Up), -1 (Down), -10 (Left) or +10 (Right) (the pad bits 0x10, 0x20,
/// 0x40, 0x80). Buying, the count stops where owned + count reaches 99 and where the munny runs out (a step the munny
/// does not cover is cut to what it covers; none at all refuses); selling, at the owned count. The count never goes
/// below 1. `None`: the press is refused.
pub fn step_quantity(sell: bool, qty: i32, owned: i32, price: i32, munny: i32, d: i32) -> Option<i32> {
    let (cur, lim) = if sell { (qty, owned) } else { (qty + owned, 99) };
    if qty < 2 && d < 1 {
        return None;
    }
    if cur < lim {
        if d >= 0 {
            let mut d = d;
            if !sell && munny < price * (qty + d) {
                let rest = munny - price * qty;
                if rest < price || price == 0 {
                    return None;
                }
                d = rest / price;
            }
            let q = qty + d;
            return Some(if sell { q.min(lim) } else if q + owned < lim { q } else { lim - owned });
        }
    } else if d >= 0 {
        return None;
    }
    Some((qty + d).max(1))
}

/// The pages of the port (the original's page numbers in `number`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    /// Page 0: the top rows.
    Top,
    /// Page 7: the first-visit text, up before page 0 until it is dismissed.
    First,
    /// Pages 1 (buy) and 4 (sell).
    List { sell: bool },
    /// Pages 2 and 5.
    Quantity { sell: bool },
    /// Pages 3 and 6: the question (row 0 No, row 1 Yes).
    Question { sell: bool },
}

impl Page {
    /// The original's page number.
    pub fn number(self) -> u8 {
        match self {
            Page::Top => 0,
            Page::First => 7,
            Page::List { sell } => if sell { 4 } else { 1 },
            Page::Quantity { sell } => if sell { 5 } else { 2 },
            Page::Question { sell } => if sell { 6 } else { 3 },
        }
    }

    /// The screen whose layout triple the page shows (0 top, 1 buy, 2 sell).
    fn screen(self) -> usize {
        match self {
            Page::Top | Page::First => 0,
            Page::List { sell } | Page::Quantity { sell } | Page::Question { sell } => 1 + usize::from(sell),
        }
    }
}

/// A key on a page (the list widget's results: a row, -1 nothing, -2 cancel, -4 the Start key).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Cancel,
    /// The list result -5 (the footage's X button; `X` in the port): the reward and collection pages switch to each other.
    Switch,
    Start,
}

/// The selection (`DAT_0036cfc4`: item +2, count +4, unit price +6, total +8 = price x count).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Deal {
    pub item: u16,
    pub qty: i32,
    pub price: i32,
}

impl Deal {
    pub fn total(&self) -> i32 {
        self.price * self.qty
    }
}

/// The state of an open shop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flow {
    pub page: Page,
    /// The shown top rows (indices into the row table: 0 Buy, 1 Sell, 2 the third row, 3 Quit) and the cursor.
    pub rows: Vec<usize>,
    pub top: usize,
    /// The list cursor.
    pub list: usize,
    pub deal: Deal,
    /// The question cursor (0 No, 1 Yes; `FUN_00261bf8` starts it on row 0).
    pub ask: usize,
}

/// What a key did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    None,
    Moved,
    Refused,
    /// The page changed.
    Page,
    /// Page 3 / 6 row 1: the deal is done (`FUN_00261d70` case 1).
    Deal,
    /// The third row (page 8 and after) is not ported.
    ThirdRow,
    /// -2 on page 0, the Quit row, or -4 anywhere: the menu ends (`FUN_00263f00(1, 0, 0)`, `FUN_00295ba0`).
    Close,
}

/// The inputs a page reads.
pub struct View<'a> {
    pub buy: &'a [Ware],
    pub sell: &'a [(Ware, u8)],
    pub owned: &'a dyn Fn(u16) -> u8,
    pub munny: u32,
}

impl Flow {
    pub fn new(rows: Vec<usize>) -> Flow {
        Flow { page: Page::Top, rows, top: 0, list: 0, deal: Deal::default(), ask: 0 }
    }

    fn list_len(&self, v: &View, sell: bool) -> usize {
        if sell { v.sell.len() } else { v.buy.len() }
    }

    /// One key (the page handlers `FUN_00260748`, `FUN_00260ec8`, `FUN_002614e0`, `FUN_00261d70`).
    pub fn key(&mut self, k: Key, v: &View) -> Act {
        // Page 7 (`FUN_002621a0`): only a confirm or a cancel ends the text (the sound of Start is all it does).
        if self.page == Page::First {
            return match k {
                Key::Confirm | Key::Cancel => {
                    self.page = Page::Top;
                    Act::Page
                }
                _ => Act::None,
            };
        }
        if k == Key::Start {
            return Act::Close;
        }
        let dir = |k: Key| if matches!(k, Key::Up | Key::Left) { -1 } else { 1 };
        match self.page {
            Page::Top => match k {
                Key::Up | Key::Down => {
                    let c = crate::menu::step_cursor(self.top, self.rows.len(), dir(k), true);
                    if c == self.top {
                        Act::None
                    } else {
                        self.top = c;
                        Act::Moved
                    }
                }
                Key::Cancel => Act::Close,
                Key::Confirm => match self.rows.get(self.top) {
                    Some(&r @ (0 | 1)) => {
                        self.page = Page::List { sell: r == 1 };
                        self.list = 0;
                        Act::Page
                    }
                    Some(2) => Act::ThirdRow,
                    Some(_) => Act::Close,
                    None => Act::None,
                },
                _ => Act::None,
            },
            Page::List { sell } => match k {
                Key::Up | Key::Down => {
                    let n = self.list_len(v, sell);
                    let c = crate::menu::step_cursor(self.list, n, dir(k), true);
                    if c == self.list {
                        Act::None
                    } else {
                        self.list = c;
                        Act::Moved
                    }
                }
                Key::Cancel => {
                    self.page = Page::Top;
                    Act::Page
                }
                Key::Confirm => {
                    let w = if sell { v.sell.get(self.list).map(|x| x.0) } else { v.buy.get(self.list).copied() };
                    let Some(w) = w else { return Act::Refused };
                    if !row_open(sell, &w, (v.owned)(w.id), v.munny) {
                        return Act::Refused;
                    }
                    self.deal = Deal { item: w.id, qty: 1, price: i32::from(if sell { w.sell } else { w.buy }) };
                    self.page = Page::Quantity { sell };
                    Act::Page
                }
                _ => Act::None,
            },
            Page::Quantity { sell } => {
                let d = match k {
                    Key::Up => 1,
                    Key::Down => -1,
                    Key::Left => -10,
                    Key::Right => 10,
                    Key::Confirm => {
                        self.page = Page::Question { sell };
                        self.ask = 0;
                        return Act::Page;
                    }
                    Key::Cancel => {
                        self.page = Page::List { sell };
                        return Act::Page;
                    }
                    Key::Start => return Act::Close,
                    Key::Switch => return Act::None,
                };
                let owned = i32::from((v.owned)(self.deal.item));
                let munny = v.munny.min(i32::MAX as u32) as i32;
                match step_quantity(sell, self.deal.qty, owned, self.deal.price, munny, d) {
                    Some(q) if q != self.deal.qty => {
                        self.deal.qty = q;
                        Act::Moved
                    }
                    Some(_) => Act::None,
                    None => Act::Refused,
                }
            }
            Page::Question { sell } => match k {
                Key::Up | Key::Down | Key::Left | Key::Right => {
                    let c = crate::menu::step_cursor(self.ask, 2, dir(k), true);
                    if c == self.ask {
                        Act::None
                    } else {
                        self.ask = c;
                        Act::Moved
                    }
                }
                Key::Cancel => {
                    self.page = Page::Quantity { sell };
                    Act::Page
                }
                Key::Confirm if self.ask == 0 => {
                    self.page = Page::Quantity { sell };
                    Act::Page
                }
                Key::Confirm => {
                    self.page = Page::List { sell };
                    Act::Deal
                }
                Key::Switch => Act::None,
                Key::Start => Act::Close,
            },
            Page::First => Act::None,
        }
    }
}

/// `FUN_00261d70` case 1: buying takes the total from the munny (`FUN_0023e6b0`) and adds the count (`FUN_001a1a18`);
/// selling adds the total (`FUN_0023e620`) and removes the count (`FUN_001a1c30`). Answers the munny after it.
pub fn deal(p: &mut Progress, sell: bool, d: &Deal) -> u32 {
    let total = d.total().max(0) as u32;
    let n = d.qty.clamp(0, 99) as u8;
    if sell {
        p.use_item(d.item, n);
        p.add_to_wallet(total)
    } else {
        p.add_item(d.item, n);
        p.spend(total)
    }
}

// --- the scene ---------------------------------------------------------------------------------------

/// A shop the next frame opens (`FUN_0024f0b8(4, event)`), set by the reaction accept.
#[derive(Resource, Default)]
pub struct ShopRequest(pub Option<u16>);

#[derive(Default)]
struct Nodes {
    layers: Vec<usize>,
    title: usize,
    rows: Vec<usize>,
    list: Vec<usize>,
    prices: Vec<usize>,
    owned: Vec<usize>,
    hand: usize,
    detail: usize,
    munny: usize,
    question: [usize; 2],
    /// The NEW / VIEWED tag sprites, one per row.
    marks: Vec<usize>,
    /// The tab grid's texts: a name and a number for each of the 17 tabs (4 ranks, 13 categories), `FUN_0028aaa8`.
    tabs: Vec<usize>,
}

impl Nodes {
    fn all(&self) -> Vec<usize> {
        let mut v = self.layers.clone();
        v.extend(self.texts());
        v.extend(&self.marks);
        v
    }

    fn texts(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.rows.iter().chain(&self.list).chain(&self.prices).chain(&self.owned).chain(&self.question).chain(&self.tabs).copied().collect();
        v.extend([self.title, self.hand, self.detail, self.munny]);
        v
    }
}

/// Rows of the list the port draws at once [inferred: the original's widget scrolls; its row count was not read].
const LIST_ROWS: usize = 9;

/// Where the port puts its texts, in the menu canvas [inferred: placed by the port over the shop's frame art, not
/// read from the widgets (`FUN_00296b48`, `FUN_0029b600` were not followed to their positions)].
mod at {
    pub const TITLE: [f32; 2] = [40.0, 44.0];
    pub const ROWS: [f32; 2] = [70.0, 120.0];
    pub const PITCH: f32 = 24.0;
    pub const PRICE_RIGHT: f32 = 330.0;
    pub const OWNED_RIGHT: f32 = 400.0;
    pub const DETAIL: [f32; 2] = [70.0, 360.0];
    pub const MUNNY: [f32; 2] = [470.0, 396.0];
    pub const TAG_X: f32 = 46.0;
    pub const WINDOW: [f32; 2] = [70.0, 200.0];
    pub const HAND: [f32; 2] = [52.0, 120.0];
    pub const IDLE: [u8; 4] = [0x9f, 0x9f, 0xaf, 0x80];
    pub const SELECTED: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
    pub const GREY: [u8; 4] = [0x60, 0x60, 0x68, 0x80];
}

#[derive(Resource, Default)]
pub struct ShopMenu {
    loaded: bool,
    tried: bool,
    shops: Shops,
    wares: Vec<Ware>,
    /// The four top rows' name messages (row table +0x12).
    row_names: [u32; 4],
    /// The layout triples: 0 top, 1 buy, 2 sell, 3 the synthesis pages (`synth::addr::LAYOUTS`).
    layouts: [[usize; 3]; 4],
    layd: Layd,
    seq_base: usize,
    tex: usize,
    nodes: Nodes,
    open: Option<Open>,
    /// The third row's data (`mixdata.bar`, the executable's tables, the material list); `None` when unreadable.
    mix: Option<(synth::Mix, synth::Tables, Vec<u16>)>,
    /// The first-visit texts (message ids at 0x381128 / 0x38112c).
    first_msgs: [u32; 2],
    /// The sequence of `itemshop.2ld` that holds the tag groups.
    mark_seq: Option<usize>,
    /// A copy of that sequence for the group records and anchors the pages read (the scene owns the drawn one).
    seqd: Option<kh2_image::seqd::Seqd>,
    /// Whether any pad button was held this frame (page 10's count-up jumps to the end while one is, `FUN_00299020`).
    held: bool,
}

struct Open {
    event: u16,
    stock: Stock,
    flow: Flow,
    /// The third row while it is up (pages 8 and after).
    synth: Option<synth::Session>,
    /// The sell list as `sell_list` built it (rebuilt after a deal, `FUN_00264be8`).
    sell: Vec<(Ware, u8)>,
    closing: u32,
    /// The buy list's items that were NEW at the open (`FUN_00265280`): a bit per place in the shop file's list.
    fresh: u128,
    /// The bit of save `+0x3724` page 7 sets when it is dismissed (`None`: no first-visit text).
    first_bit: Option<u8>,
    /// The tags the pages show (refreshed before each redraw).
    tags: Tags,
}

/// The NEW / VIEWED tags of the shop rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Tags {
    /// The Buy row of the top page (`FUN_00262bd0`) and the third row (`FUN_002919e8`).
    buy: Option<Tag>,
    third: Option<Tag>,
    /// The buy list's rows by item id (`FUN_002626e8`).
    items: Vec<(u16, Option<Tag>)>,
}

impl ShopMenu {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
}

pub struct ShopPlugin;

impl Plugin for ShopPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShopMenu>()
            .init_resource::<ShopRequest>()
            .add_systems(PreUpdate, (input.before(crate::menu::input), hold.after(crate::menu::input)).after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, tick.before(crate::schedule::Frame::Popups));
    }
}

fn load(m: &mut ShopMenu, disc: &Disc, scene: &mut MenuScene, images: &mut Assets<Image>) {
    m.tried = true;
    let Some(two) = disc.read("menu/us/itemshop.2ld").and_then(|b| layd::parse_2ld(&b)) else {
        warn!("shop: menu/us/itemshop.2ld is not readable; the shops are off");
        return;
    };
    let Some(shops) = disc.read("03system.bin").and_then(|b| kh2_bar::parse(&b).ok().and_then(|e| e.iter().find(|e| e.kind == 0x29).and_then(|e| Shops::parse(e.data)))) else {
        warn!("shop: 03system.bin's shop entry (type 0x29, TZSH) is not readable; the shops are off");
        return;
    };
    let wares = {
        let Ok(mut a) = disc.0.lock() else { return };
        kh2_tables::load::<t::Item>(&mut a, &t::ITEM).ok().map(|d| d.rows.iter().map(Ware::of).collect::<Vec<_>>())
    };
    let Some(wares) = wares else { return };
    let Some(exe) = disc.read_root("SLUS_210.05").and_then(Exe::load) else { return };
    let mut names = [0u32; 4];
    for (k, n) in names.iter_mut().enumerate() {
        *n = exe.u16(addr::ROWS + addr::ROW_SIZE * k as u32 + 0x12).map_or(0, |v| u32::from(v & 0x7fff));
    }
    let mut layouts = [[0usize; 3]; 4];
    for (k, l) in layouts.iter_mut().take(3).enumerate() {
        for (i, v) in l.iter_mut().enumerate() {
            *v = exe.u16(addr::LAYOUTS + 2 * (3 * k + i) as u32).map_or(0, usize::from);
        }
    }
    m.first_msgs = [exe.u16(addr::FIRST_VISIT).map_or(0, u32::from), exe.u16(addr::FIRST_VISIT + 4).map_or(0, u32::from)];
    let tables = synth::Tables::read(&|a| exe.u16(a), &|a| exe.u8(a));
    m.mix = match (disc.read("menu/us/mixdata.bar").and_then(|b| synth::Mix::parse(&b)), tables) {
        (Some(mix), Some(t)) => {
            let mats: Vec<u16> = wares.iter().filter(|w| w.kind == synth::MATERIAL_TYPE).map(|w| w.id).take(crate::progress::MOOGLE_MATERIALS).collect();
            info!("shop: loaded mixdata.bar ({} recipes, {} levels, {} rewards), {} materials, synthesis rows {:x?}", mix.recipes.len(), mix.levels.len(), mix.conds.len(), mats.len(), t.rows);
            layouts[3] = t.layouts.map(usize::from);
            Some((mix, t, mats))
        }
        _ => {
            warn!("shop: menu/us/mixdata.bar or the synthesis tables are not readable; the third row stays off");
            None
        }
    };
    if scene.text.is_none() || two.images.is_empty() {
        return;
    }
    m.tex = scene.texs.len();
    for img in &two.images {
        scene.texs.push(menu_gfx::Tex::new(img, images));
    }
    m.seq_base = scene.seqs.len();
    let Layd { layers, layouts: lays, seqs } = two.layd;
    // The NEW / VIEWED tag groups (0x78, 0x77) are in the first sequence that has that many groups.
    let mark_seq = seqs.iter().position(|q| q.groups.len() > TAG_NEW.max(TAG_VIEWED));
    scene.seqs.extend(seqs);
    m.layd = Layd { layers, layouts: lays, seqs: Vec::new() };
    let add = |scene: &mut MenuScene, seq: Option<usize>, tex: usize| {
        let mut n = Node::new(seq);
        n.tex = tex;
        n.visible = false;
        scene.add(n)
    };
    let tex = m.tex;
    let mut n = Nodes::default();
    let most = (0..m.layd.layouts.len()).map(|i| m.layd.layout(i).len()).max().unwrap_or(0);
    for _ in 0..most {
        n.layers.push(add(scene, Some(m.seq_base), tex));
    }
    n.title = add(scene, None, tex);
    for _ in 0..4 {
        n.rows.push(add(scene, None, tex));
    }
    // Three more rows than the shop's list shows: the synthesis detail page lists its rows and the recipe's lines.
    for _ in 0..LIST_ROWS + 3 {
        n.list.push(add(scene, None, tex));
        n.prices.push(add(scene, None, tex));
        n.owned.push(add(scene, None, tex));
    }
    n.hand = add(scene, None, tex);
    n.detail = add(scene, None, tex);
    n.munny = add(scene, None, tex);
    n.question = [add(scene, None, tex), add(scene, None, tex)];
    // The tag's rectangle (0, 128)-(40, 140) is the NEW! art of the file's second image [inferred: the port's sequences
    // draw from one image each; the loader's rule for which one a sequence uses was not read].
    for _ in 0..2 * TABS {
        n.tabs.push(add(scene, None, tex));
    }
    let tag_tex = if scene.texs.len() > m.tex + 1 { m.tex + 1 } else { m.tex };
    for _ in 0..LIST_ROWS + 3 {
        n.marks.push(add(scene, mark_seq.map(|q| m.seq_base + q), tag_tex));
    }
    m.mark_seq = mark_seq;
    m.seqd = mark_seq.and_then(|q| scene.seqs.get(m.seq_base + q)).cloned();
    m.nodes = n;
    m.shops = shops;
    m.wares = wares;
    m.row_names = names;
    m.layouts = layouts;
    m.loaded = true;
    info!("shop: loaded itemshop.2ld ({} layouts) and the shop file ({} records), top rows {:x?}, layouts {:?}", m.layd.layouts.len(), m.shops.records.len(), m.row_names, m.layouts);
}

fn text_label(text: &MenuText, tokens: Vec<Token>, color: [u8; 4], align: u8, at: [f32; 2]) -> Label {
    Label { tokens: tokens.into_iter().filter(|t| *t != Token::End).collect(), style: Style { font: 1, size: Some(15.0), color, align, world: text.world, ..Style::default() }, offset: at }
}

fn message(text: &MenuText, id: u32) -> Vec<Token> {
    text.messages.as_ref().and_then(|m| m.tokens(id)).unwrap_or_default()
}

fn literal(text: &MenuText, s: &str) -> Vec<Token> {
    text.literal(s, Style::default(), [0.0; 2]).tokens
}

fn set_label(scene: &mut MenuScene, node: usize, l: Option<Label>) {
    scene.nodes[node].visible = l.is_some();
    scene.nodes[node].label = l;
}

/// The frame of a screen: the layers of the triple's enter layout, holding on the hold layout's groups.
fn set_layout(m: &ShopMenu, scene: &mut MenuScene, screen: usize) {
    let t = m.layouts[screen.min(3)];
    let enter = m.layd.layout(t[0]).to_vec();
    let hold = m.layd.layout(t[2]).to_vec();
    for (i, &id) in m.nodes.layers.iter().enumerate() {
        match enter.get(i) {
            Some(l) => {
                scene.nodes[id].visible = true;
                scene.nodes[id].seq = Some(m.seq_base + l.seq);
                scene.nodes[id].pos = [l.x as f32, l.y as f32];
                let next = hold.get(i).map_or(l.group, |h| h.group);
                scene.set_group_then(id, l.group, next);
            }
            None => scene.nodes[id].visible = false,
        }
    }
}

fn hide_all(m: &ShopMenu, scene: &mut MenuScene) {
    for id in m.nodes.all() {
        scene.nodes[id].visible = false;
        scene.nodes[id].label = None;
    }
}

fn buy_wares(m: &ShopMenu, s: &Stock) -> Vec<Ware> {
    s.items.iter().filter_map(|id| m.wares.iter().find(|w| w.id == *id).copied()).collect()
}

fn sell_wares(m: &ShopMenu, p: Option<&Progress>) -> Vec<(Ware, u8)> {
    let count = |id: u16| p.map_or(0, |p| p.item_count(id));
    sell_list(&m.wares, &count).into_iter().filter_map(|(id, n)| m.wares.iter().find(|w| w.id == id).map(|w| (*w, n))).collect()
}

/// The tag sprite groups: `DAT_00381238` (NEW) and `DAT_0038123c` (VIEWED).
const TAG_NEW: usize = 0x78;
const TAG_VIEWED: usize = 0x77;

/// Recomputes the tags from the save (`FUN_00262bd0`, `FUN_002626e8`, `FUN_002919e8`).
fn refresh_tags(m: &mut ShopMenu, p: Option<&Progress>) {
    let ShopMenu { shops, open, .. } = m;
    let (Some(o), Some(p)) = (open.as_mut(), p) else { return };
    let seen = |n: usize| p.shop_seen(n);
    let tags = Tags {
        buy: buy_tag(o.fresh, &seen),
        third: (o.stock.rows_bits & 2 != 0 && synth::has_new(&p.moogle)).then_some(Tag::New),
        items: o.stock.items.iter().map(|&id| (id, item_tag(shops, o.fresh, id, &seen))).collect(),
    };
    if tags != o.tags {
        info!("shop: tags Buy row {:?}, third row {:?}, items {:?} (FUN_00262bd0, FUN_002919e8, FUN_002626e8)", tags.buy, tags.third, tags.items);
        o.tags = tags;
    }
}

/// Shows tag `k` at `at` (nothing for `None`). The group restarts only when it changes.
fn put_tag(m: &ShopMenu, scene: &mut MenuScene, k: usize, tag: Option<Tag>, at: [f32; 2]) {
    let Some(&id) = m.nodes.marks.get(k) else { return };
    let (Some(tag), Some(seq)) = (tag, m.mark_seq) else {
        scene.nodes[id].visible = false;
        return;
    };
    let group = if tag == Tag::New { TAG_NEW } else { TAG_VIEWED };
    let n = &mut scene.nodes[id];
    n.visible = true;
    n.seq = Some(m.seq_base + seq);
    n.pos = at;
    if n.player.group != Some(group) {
        scene.set_group(id, group);
    }
}

/// Where the tags sit: left of the row's hand (the tag sprite is 40 wide and ends at its origin) [inferred: the original
/// parents the tag to the row widget at the extent of the row group (`FUN_00138ed0(tag, FUN_00237618(row + 0x18), ..)`,
/// group 0x62's extent is 0x10e); the port's rows have no plate, so the tag takes the free space at the left].
fn tag_at(row_y: f32) -> [f32; 2] {
    [at::TAG_X, row_y]
}

/// Draws the page the flow is on.
fn redraw(m: &ShopMenu, scene: &mut MenuScene, owned: &dyn Fn(u16) -> u8, munny: u32, moogle: &Moogle) {
    // The text is taken out of the scene while the labels are set, and put back.
    let Some(text) = scene.text.take() else { return };
    if m.open.as_ref().is_some_and(|o| o.synth.is_some()) {
        draw_synth(m, scene, &text, owned, munny, moogle);
    } else {
        draw_page(m, scene, &text, owned, munny);
    }
    scene.text = Some(text);
}

fn draw_page(m: &ShopMenu, scene: &mut MenuScene, text: &MenuText, owned: &dyn Fn(u16) -> u8, munny: u32) {
    let Some(o) = m.open.as_ref() else { return };
    let n = &m.nodes;
    for id in n.texts() {
        set_label(scene, id, None);
    }
    for k in 0..n.marks.len() {
        put_tag(m, scene, k, None, [0.0; 2]);
    }
    set_label(scene, n.title, Some(text_label(text, message(text, u32::from(o.stock.message)), at::SELECTED, 0, at::TITLE)));
    set_label(scene, n.munny, Some(text_label(text, literal(text, &format!("{munny}")), at::SELECTED, 1, at::MUNNY)));
    let row_y = |k: usize| at::ROWS[1] + at::PITCH * k as f32;
    let hand = text_label(text, literal(text, ">"), at::SELECTED, 0, [0.0, 0.0]);
    let put_hand = |scene: &mut MenuScene, k: usize| {
        let mut l = hand.clone();
        l.offset = [at::HAND[0], row_y(k)];
        set_label(scene, n.hand, Some(l));
    };
    match o.flow.page {
        // Page 7 (`FUN_00262140`, `FUN_00262350`): the text of the shop's class in a window on the top of the screen.
        Page::First => {
            let id = m.first_msgs[usize::from(o.first_bit.unwrap_or(0))];
            set_label(scene, n.detail, Some(text_label(text, message(text, id), at::SELECTED, 0, at::WINDOW)));
        }
        Page::Top => {
            for (k, &r) in o.flow.rows.iter().enumerate() {
                let c = if k == o.flow.top { at::SELECTED } else { at::IDLE };
                set_label(scene, n.rows[k], Some(text_label(text, message(text, m.row_names[r]), c, 0, [at::ROWS[0], row_y(k)])));
                let tag = match r {
                    0 => o.tags.buy,
                    2 => o.tags.third,
                    _ => None,
                };
                put_tag(m, scene, k, tag, tag_at(row_y(k)));
            }
            put_hand(scene, o.flow.top);
        }
        Page::List { sell } | Page::Quantity { sell } | Page::Question { sell } => {
            let list: Vec<(Ware, u8)> = if sell { o.sell.clone() } else { buy_wares(m, &o.stock).into_iter().map(|w| (w, owned(w.id))).collect() };
            let first = o.flow.list.saturating_sub(LIST_ROWS - 1);
            for (k, (w, have)) in list.iter().enumerate().skip(first).take(LIST_ROWS) {
                let row = k - first;
                let c = if !row_open(sell, w, *have, munny) { at::GREY } else if k == o.flow.list { at::SELECTED } else { at::IDLE };
                set_label(scene, n.list[row], Some(text_label(text, message(text, u32::from(w.name)), c, 0, [at::ROWS[0], row_y(row)])));
                let price = literal(text, &format!("{}", if sell { w.sell } else { w.buy }));
                set_label(scene, n.prices[row], Some(text_label(text, price, c, 1, [at::PRICE_RIGHT, row_y(row)])));
                set_label(scene, n.owned[row], Some(text_label(text, literal(text, &format!("{have}")), c, 1, [at::OWNED_RIGHT, row_y(row)])));
                if !sell {
                    let tag = o.tags.items.iter().find(|(id, _)| *id == w.id).and_then(|(_, t)| *t);
                    put_tag(m, scene, row, tag, tag_at(row_y(row)));
                }
            }
            if matches!(o.flow.page, Page::List { .. }) {
                put_hand(scene, o.flow.list - first);
            } else {
                let d = o.flow.deal;
                let mut tokens: Vec<Token> = m.wares.iter().find(|w| w.id == d.item).map(|w| message(text, u32::from(w.name))).unwrap_or_default().into_iter().filter(|t| *t != Token::End).collect();
                tokens.extend(literal(text, &format!("  x{}  {}", d.qty, d.total())));
                set_label(scene, n.detail, Some(text_label(text, tokens, at::SELECTED, 0, at::DETAIL)));
            }
            if matches!(o.flow.page, Page::Question { .. }) {
                for (k, id) in [msg::NO, msg::YES].into_iter().enumerate() {
                    let c = if k == o.flow.ask { at::SELECTED } else { at::IDLE };
                    set_label(scene, n.question[k], Some(text_label(text, message(text, id), c, 0, [at::DETAIL[0] + 300.0 + 60.0 * k as f32, at::DETAIL[1]])));
                }
            }
        }
    }
}

/// The game state as the synthesis pages see it.
struct Inv<'a>(&'a mut Progress);

impl synth::Store for Inv<'_> {
    fn moogle(&self) -> &Moogle {
        &self.0.moogle
    }
    fn moogle_mut(&mut self) -> &mut Moogle {
        &mut self.0.moogle
    }
    fn count(&self, id: u16) -> u8 {
        self.0.item_count(id)
    }
    fn add(&mut self, id: u16, n: u8) -> u8 {
        self.0.add_item(id, n)
    }
    fn remove(&mut self, id: u16, n: u8) {
        self.0.use_item(id, n)
    }
    fn story(&self, flag: u16) -> bool {
        self.0.story().test(flag)
    }
    fn set_record_bit(&mut self, n: u16) {
        self.0.set_record_bit(n)
    }
}

fn moogle_of(p: Option<&Progress>) -> Moogle {
    p.map(|p| p.moogle.clone()).unwrap_or_default()
}

/// -2 on page 0, the Quit row, or -4 anywhere: the leave layout of `screen`, then the close dim (`tick`).
fn begin_close(m: &mut ShopMenu, scene: &mut MenuScene, se: &mut MessageWriter<PlaySe>, screen: usize, page: u8) {
    menu_sound::play(se, menu_sound::CANCEL);
    info!("shop: close (FUN_00263f00(1, 0, 0), FUN_00295ba0) from page {page}");
    let leave = m.layd.layout(m.layouts[screen.min(3)][1]).to_vec();
    let layers = m.nodes.layers.clone();
    for (i, &id) in layers.iter().enumerate() {
        if let Some(l) = leave.get(i) {
            scene.set_group(id, l.group);
        }
    }
    for id in m.nodes.texts() {
        scene.nodes[id].visible = false;
    }
    if let Some(o) = m.open.as_mut() {
        o.closing = 1;
    }
}

/// A key while the third row is up (`synth::Session::key`).
fn synth_key(m: &mut ShopMenu, scene: &mut MenuScene, se: &mut MessageWriter<PlaySe>, progress: Option<&mut Progress>, key: Key) {
    let Some(p) = progress else { return };
    let ShopMenu { mix, wares, open, .. } = &mut *m;
    let (Some((mix, t, mats)), Some(o)) = (mix.as_ref(), open.as_mut()) else { return };
    let Some(mut s) = o.synth.take() else { return };
    let rank = |id: u16| wares.iter().find(|w| w.id == id).map_or(0, |w| w.rank);
    let flag1 = |id: u16| wares.iter().find(|w| w.id == id).map_or(0, |w| w.flag1);
    let book = synth::Book { mix, t, mats, rank: &rank, flag1: &flag1 };
    let before = s.page;
    let act = s.key(key, &book, &mut Inv(p));
    let entry = s.entry();
    use synth::Act as S;
    match act {
        S::None => {}
        S::Moved => menu_sound::play(se, menu_sound::CURSOR),
        S::Refused => {
            menu_sound::play(se, menu_sound::REFUSED);
            info!("synth: page {} key {key:?} refused (FUN_00291b58 / FUN_00295bf0){}", before.number(), if s.page == synth::Page::Notice { format!(": text {:#x} (page 0x12)", s.text) } else { String::new() });
        }
        S::Toggled => {
            menu_sound::play(se, menu_sound::CURSOR);
            info!("synth: bonus flags now {:#x} (the detail page's cases 1..3)", s.flags);
        }
        S::Page => {
            menu_sound::play(se, if key == Key::Cancel { menu_sound::CANCEL } else { crate::menu_status::CONFIRM });
            info!("synth: page {:#x} -> page {:#x} (entry {:?}, rows {:#x}, cursor {})", before.number(), s.page.number(), entry, s.detail, s.row);
        }
        S::Made => {
            menu_sound::play(se, snd::DEAL);
            if let Some(d) = &s.last {
                info!(
                    "synth: made item {:#x} (entry {}, FUN_00288170 / FUN_002909d0): used {:x?}, points +{} = {}, level {} -> {}, owned now {}",
                    d.item,
                    d.entry,
                    d.used,
                    d.points,
                    p.moogle.points,
                    d.level.0,
                    d.level.1,
                    p.item_count(d.item)
                );
            }
        }
        S::Back => {
            menu_sound::play(se, menu_sound::CANCEL);
            info!("synth: back to the shop's top rows (page 9 -> page 0, FUN_0025fef0)");
            o.flow.page = Page::Top;
            set_layout(m, scene, 0);
            refresh_tags(m, Some(p));
            let moogle = p.moogle.clone();
            let (owned, munny) = (|id: u16| p.item_count(id), p.wallet());
            redraw(m, scene, &owned, munny, &moogle);
            return;
        }
        S::Close => {
            o.synth = Some(s);
            begin_close(m, scene, se, 3, before.number());
            return;
        }
    }
    o.synth = Some(s);
    let moogle = p.moogle.clone();
    let (owned, munny) = (|id: u16| p.item_count(id), p.wallet());
    redraw(m, scene, &owned, munny, &moogle);
}

/// The row plate group of the reward and material lists (group 0x62 of the shop's sequence, `FUN_0028b470`): the row
/// pitch (+0x1c), the text size (+0x18, 0xf when 0) and the second text's x (the group's x, also the NEW tag's origin).
/// The port's own numbers when the file has no such group.
fn reward_metrics(m: &ShopMenu) -> (f32, f32, f32) {
    let g = m.seqd.as_ref().and_then(|q| q.groups.get(ROW_PLATE));
    g.map_or((at::PITCH, 15.0, at::PRICE_RIGHT + 20.0 - at::ROWS[0]), |g| (g.unk1c as f32, if g.size == 0 { 15.0 } else { g.size as f32 }, g.x as f32))
}

/// The row pitch of a list widget's plate group (+0x1c). Page 10 builds its rows from group 0x60 and page 0x17 from group
/// 0x6e (`FUN_0028f9a8`, over the rows `FUN_0028b470` made for page 0xb); the disc has 23 and 26.
fn group_x(m: &ShopMenu, group: usize) -> f32 {
    m.seqd.as_ref().and_then(|q| q.groups.get(group)).map_or(at::PRICE_RIGHT - at::ROWS[0], |g| g.x as f32)
}

fn group_pitch(m: &ShopMenu, group: usize) -> f32 {
    m.seqd.as_ref().and_then(|q| q.groups.get(group)).map_or(at::PITCH, |g| g.unk1c as f32)
}

/// `FUN_0028b470`: the sprite group of a reward row's plate.
const ROW_PLATE: usize = 0x62;

/// The header window of pages 10 and 0xb (`FUN_0028aaa8`: `FUN_0029b600(window, .., 0x67, 0x68, 0)`), the rank tab group and
/// the category tab group (`FUN_0029b6c0(.., 99 / 100, ..)`), and the number of tabs of each kind.
const WINDOW_GROUP: usize = 0x67;
const RANK_TAB: usize = 99;
const CATEGORY_TAB: usize = 100;
const TABS: usize = 17;

/// The anchor of a group at its last frame: the translation of its first animation with flag bit 2, plus the pivot times
/// the scale (`FUN_00236cf8` -> `FUN_002375c0`); `None` when the group has no such animation.
fn anchor_of(q: &kh2_image::seqd::Seqd, group: usize) -> Option<[f32; 2]> {
    q.anchor(group, q.last_frame(group), &kh2_image::seqd::Ext::default()).map(|a| [a[0] as f32, a[1] as f32])
}

/// Where a widget parented to the header window starts: the window's anchor (`FUN_0029b060`), then the widget's own group
/// anchor. The window's group record (x 460, y 123) is NOT added: that would put the lists at x 484 and the empty-list
/// texts (anchors 232 and 24 + 232 = 256 = half the 512 screen) show the origin is the anchor alone [inferred; the
/// decompile's `FUN_00237618` addition for the parent was not matched to a record field].
fn plate_origin(q: &kh2_image::seqd::Seqd, group: usize) -> Option<[f32; 2]> {
    let w = anchor_of(q, WINDOW_GROUP)?;
    let a = anchor_of(q, group)?;
    Some([w[0] + a[0], w[1] + a[1]])
}

/// The place of each tab's name and number (`FUN_0028aaa8`): rank tab k at the group's anchor and k times its `+0x1c`
/// below; category tab n at `(n % 4) * +0x20`, `(n / 4) * +0x1c`; the number is a child at the group record's x, y.
/// Index 0..4 are the rank tabs, 4.. the categories.
fn tab_places(q: &kh2_image::seqd::Seqd) -> Option<Vec<([f32; 2], [f32; 2])>> {
    let w = anchor_of(q, WINDOW_GROUP)?;
    let mut v = Vec::new();
    for (group, count) in [(RANK_TAB, 4usize), (CATEGORY_TAB, 13)] {
        let (a, g) = (anchor_of(q, group)?, q.groups.get(group)?);
        for n in 0..count {
            let (dx, dy) = if group == RANK_TAB { (0, g.unk1c * n as i32) } else { (g.unk20 * (n % 4) as i32, g.unk1c * (n / 4) as i32) };
            let name = [w[0] + a[0] + dx as f32, w[1] + a[1] + dy as f32];
            v.push((name, [name[0] + g.x as f32, name[1] + g.y as f32]));
        }
    }
    Some(v)
}

impl ShopMenu {
    /// The first row of a list page whose rows hang on plate group `group` of the header window; the port's own first row
    /// when the file has no such group.
    fn rows_origin(&self, group: usize) -> [f32; 2] {
        self.seqd.as_ref().and_then(|q| plate_origin(q, group)).unwrap_or(at::ROWS)
    }
}

/// `FUN_00191660`: how many characters a text shows before its end (glyphs and icons; a space is not counted).
fn shown_chars(tokens: &[Token]) -> usize {
    tokens.iter().take_while(|t| !matches!(t, Token::Newline | Token::End)).filter(|t| matches!(t, Token::Glyph(_) | Token::Icon(_))).count()
}

/// `FUN_0029bb48`: a "?" for each character of the text (the string at 0x3828b8).
fn masked(text: &MenuText, name: &[Token]) -> Vec<Token> {
    let mut v = Vec::new();
    for _ in 0..shown_chars(name) {
        v.extend(literal(text, "?"));
    }
    v
}

/// `FUN_00191a30`: the variables (command 0xd) of a text are replaced one by one by the given texts.
fn fill(tokens: Vec<Token>, args: &[Vec<Token>]) -> Vec<Token> {
    let mut next = args.iter();
    let mut v = Vec::new();
    for t in tokens {
        if t != Token::Variable {
            v.push(t);
        } else if let Some(a) = next.next() {
            v.extend(a.iter().filter(|t| **t != Token::End).cloned());
        }
    }
    v
}

/// The second column of a page 0xb row as tokens (`FUN_0028a1b8`).
fn reward_tokens(text: &MenuText, r: synth::RewardText) -> Vec<Token> {
    match r {
        synth::RewardText::Item { icon, name } => {
            let mut v = vec![Token::Icon(icon)];
            v.extend(message(text, name));
            v
        }
        synth::RewardText::Hidden { head, mask } => {
            let mut v: Vec<Token> = head.map(|h| message(text, h)).unwrap_or_default().into_iter().filter(|t| *t != Token::End).collect();
            v.extend(masked(text, &message(text, mask)));
            v
        }
        synth::RewardText::Filled { text: id } => fill(message(text, id), &[message(text, synth::msg::REWARD_HEAD)]),
    }
}

/// The synthesis pages. The texts are labels the port places, like the shop pages' [inferred positions: the list and
/// window widgets (`FUN_00296b48`, `FUN_0029b600`, `FUN_00299878`) were not followed]; the detail page also lists the
/// recipe's lines (material, need, stock) [inferred: `FUN_002894f0` and `FUN_00289aa8`, which draw them, were not read].
fn draw_synth(m: &ShopMenu, scene: &mut MenuScene, text: &MenuText, owned: &dyn Fn(u16) -> u8, munny: u32, moogle: &Moogle) {
    let (Some(o), Some((mix, t, mats))) = (m.open.as_ref(), m.mix.as_ref()) else { return };
    let Some(s) = o.synth.as_ref() else { return };
    let rank = |id: u16| m.wares.iter().find(|w| w.id == id).map_or(0, |w| w.rank);
    let flag1 = |id: u16| m.wares.iter().find(|w| w.id == id).map_or(0, |w| w.flag1);
    let book = synth::Book { mix, t, mats, rank: &rank, flag1: &flag1 };
    let n = &m.nodes;
    for id in n.texts() {
        set_label(scene, id, None);
    }
    for k in 0..n.marks.len() {
        put_tag(m, scene, k, None, [0.0; 2]);
    }
    let tag_of = |b: Option<bool>| b.map(|new| if new { Tag::New } else { Tag::Viewed });
    let name = |id: u16| -> Vec<Token> { m.wares.iter().find(|w| w.id == id).map(|w| message(text, u32::from(w.name))).unwrap_or_default().into_iter().filter(|t| *t != Token::End).collect() };
    let pitch = match s.page {
        synth::Page::Rewards => reward_metrics(m).0,
        synth::Page::Materials { totals: true } => group_pitch(m, 0x60),
        synth::Page::Materials { totals: false } => group_pitch(m, 0x6e),
        _ => at::PITCH,
    };
    // Pages 10, 0xb and 0x17 hang their rows on a plate group of the header window (`plate_origin`) and put the second text
    // at the group record's x; the other pages keep the port's own first row [inferred].
    let (base, second) = match s.page {
        synth::Page::Rewards => (m.rows_origin(ROW_PLATE), Some(reward_metrics(m).2)),
        synth::Page::Materials { totals: true } => (m.rows_origin(0x60), Some(group_x(m, 0x60))),
        synth::Page::Materials { totals: false } => (m.rows_origin(0x6e), Some(group_x(m, 0x6e))),
        _ => (at::ROWS, None),
    };
    let (price_x, price_align) = second.map_or((at::PRICE_RIGHT, 1), |x| (base[0] + x, 0));
    let hand_x = base[0] - (at::ROWS[0] - at::HAND[0]);
    let tag_x = base[0] - (at::ROWS[0] - at::TAG_X);
    let row_y = |k: usize| base[1] + pitch * k as f32;
    let row = |scene: &mut MenuScene, k: usize, tokens: Vec<Token>, c: [u8; 4], right: Option<String>| {
        if k < n.list.len() {
            set_label(scene, n.list[k], Some(text_label(text, tokens, c, 0, [base[0], row_y(k)])));
            if let Some(r) = right {
                set_label(scene, n.prices[k], Some(text_label(text, literal(text, &r), c, price_align, [price_x, row_y(k)])));
            }
        }
    };
    let hand = |scene: &mut MenuScene, k: usize| {
        set_label(scene, n.hand, Some(text_label(text, literal(text, ">"), at::SELECTED, 0, [hand_x, row_y(k)])));
    };
    let tag_at = |y: f32| [tag_x, y];
    let pick = |k: usize, cur: usize, open: bool| if !open { at::GREY } else if k == cur { at::SELECTED } else { at::IDLE };
    set_label(scene, n.munny, Some(text_label(text, literal(text, &format!("{munny}")), at::SELECTED, 1, at::MUNNY)));
    let title = |scene: &mut MenuScene, tokens: Vec<Token>| set_label(scene, n.title, Some(text_label(text, tokens, at::SELECTED, 0, at::TITLE)));
    let window = |scene: &mut MenuScene, tokens: Vec<Token>| set_label(scene, n.detail, Some(text_label(text, tokens, at::SELECTED, 0, [at::ROWS[0], 200.0])));
    // A window page (0x12, 0x13, 0x14, 0x16) shows its text alone [inferred: the window covers the page under it].
    if matches!(s.page, synth::Page::Materials { totals: true } | synth::Page::Rewards) {
        draw_tabs(m, scene, text, &book, s, moogle);
    }
    match s.page {
        synth::Page::Top => {
            title(scene, message(text, u32::from(o.stock.message)));
            for (k, &r) in s.rows.iter().enumerate() {
                row(scene, k, message(text, u32::from(t.rows[usize::from(r)])), pick(k, s.top, true), None);
                put_tag(m, scene, k, tag_of(synth::row_mark(&book, moogle, &s.marks, r)), tag_at(row_y(k)));
            }
            hand(scene, s.top);
        }
        synth::Page::List(kind) => {
            title(scene, message(text, u32::from(t.rows[usize::from(kind)])));
            if s.entries.is_empty() {
                set_label(scene, n.detail, Some(text_label(text, message(text, synth::msg::NOTHING), at::IDLE, 0, at::DETAIL)));
            }
            let first = s.list.saturating_sub(LIST_ROWS - 1);
            for (k, &e) in s.entries.iter().enumerate().skip(first).take(LIST_ROWS) {
                // An entry only story flag 0x4837 lists (`FUN_00288fc0`, flag 0x80): the item's name as "?" and dim.
                let hidden = e & synth::HIDDEN != 0;
                let Some((r, odd)) = synth::recipe(&book, e & !synth::HIDDEN) else { continue };
                let item = if odd { r.upgraded } else { r.result };
                if hidden {
                    row(scene, k - first, masked(text, &name(item)), pick(k, s.list, false), None);
                    continue;
                }
                row(scene, k - first, name(item), pick(k, s.list, synth::entry_open(&book, moogle, kind, e)), Some(format!("{}", owned(item))));
                put_tag(m, scene, k - first, tag_of(s.entry_mark(moogle, e)), tag_at(row_y(k - first)));
            }
            if !s.entries.is_empty() {
                hand(scene, s.list - first);
            }
        }
        synth::Page::Detail(kind) => {
            let Some((r, odd)) = s.entry().and_then(|e| synth::recipe(&book, e)) else { return };
            let item = if !odd && s.flags & 4 == 0 { r.result } else { r.upgraded };
            let mut head = name(item);
            head.extend(literal(text, "   "));
            head.extend(message(text, synth::msg::IN_STOCK).into_iter().filter(|t| *t != Token::End));
            head.extend(literal(text, &format!(" {}", owned(item))));
            title(scene, head);
            let shown: Vec<u8> = (0..5u8).filter(|k| s.detail >> k & 1 != 0).collect();
            for (k, &kind_k) in shown.iter().enumerate() {
                let (tokens, right) = match kind_k {
                    0 => (message(text, synth::msg::MAKE_ROW), None),
                    4 => (message(text, synth::msg::QUIT_ROW), None),
                    q => {
                        let b = t.bonus_item(r.rank, usize::from(q - 1));
                        let mut v = if s.flags >> (q - 1) & 1 != 0 { literal(text, "* ") } else { Vec::new() };
                        v.extend(name(b));
                        (v, Some(format!("{}", synth::stock(&book, moogle, b).max(0))))
                    }
                };
                row(scene, k, tokens, pick(k, s.row, true), right);
            }
            hand(scene, s.row);
            let div = synth::divisor(&book, moogle, kind, r.rank, s.flags & 2 != 0);
            for (j, &(id, c)) in r.mats.iter().enumerate() {
                let k = shown.len() + 1 + j;
                let have = synth::stock(&book, moogle, id).max(0);
                let need = synth::need(c, div);
                row(scene, k, name(id), if have >= i64::from(need) { at::IDLE } else { at::GREY }, Some(format!("{need}/{have}")));
            }
            let help = synth::row_kind(s.detail, s.row).and_then(|k| t.help.get(usize::from(k))).copied().unwrap_or(0);
            set_label(scene, n.question[0], Some(text_label(text, message(text, u32::from(help)), at::IDLE, 0, at::DETAIL)));
        }
        // Page 0xb (`FUN_0028a1b8`, rows built by `FUN_0028b470`): the reward's text and, in the second column, what it gives
        // (`synth::reward_text`: the item behind its icon, or the unlock's message; an unreached reward listed by story flag
        // 0x4837 shows its icon and a "?" for each letter). The first row's origin is the port's [inferred]; the pitch, the
        // second column and the text size are the row plate group's.
        synth::Page::Rewards => {
            title(scene, message(text, synth::msg::REWARDS));
            if s.rewards.is_empty() {
                set_label(scene, n.detail, Some(text_label(text, message(text, synth::msg::NO_REWARDS), at::IDLE, 0, at::DETAIL)));
            }
            // The rows are the row plate group's (`FUN_0028b470`): row k at k times the group's +0x1c below the first, the
            // second text at the group's x to its right with the NEW tag ending there, both texts at the group's size.
            let (pitch, size, col2) = reward_metrics(m);
            let ry = |r: usize| base[1] + pitch * r as f32;
            let sized = |l: Label| Label { style: Style { size: Some(size), ..l.style }, ..l };
            let first = s.rew.saturating_sub(LIST_ROWS - 1);
            for (k, &(ci, met)) in s.rewards.iter().enumerate().skip(first).take(LIST_ROWS) {
                let Some(c) = mix.conds.get(usize::from(ci)) else { continue };
                let r = k - first;
                let color = pick(k, s.rew, met);
                if r < n.list.len() {
                    set_label(scene, n.list[r], Some(sized(text_label(text, message(text, u32::from(c.text)), color, 0, [base[0], ry(r)]))));
                }
                let item = m.wares.iter().find(|w| w.id == c.reward).map(|w| (w.kind, w.flag1, w.name));
                if let Some(given) = synth::reward_text(t, c, met, item) {
                    set_label(scene, n.owned[r], Some(sized(text_label(text, reward_tokens(text, given), color, 0, [base[0] + col2, ry(r)]))));
                }
                let mark = Moogle::bit(s.marks.rewards, u32::from(ci)).then_some(s.marks.fresh);
                put_tag(m, scene, r, tag_of(mark), [base[0] + col2, ry(r)]);
            }
            if !s.rewards.is_empty() {
                set_label(scene, n.hand, Some(text_label(text, literal(text, ">"), at::SELECTED, 0, [hand_x, ry(s.rew - first)])));
            }
        }
        synth::Page::Materials { totals } => {
            title(scene, message(text, if totals { synth::msg::COLLECTION } else { synth::msg::MATERIALS }));
            let first = s.mat.saturating_sub(LIST_ROWS - 1);
            for (k, &i) in s.slots.iter().enumerate().skip(first).take(LIST_ROWS) {
                // Page 10 shows the counting-up totals (`s.shown`), page 0x17 the stock.
                let v = if totals { s.shown.get(i).copied().unwrap_or_else(|| moogle.total_at(i)) } else { moogle.stock_at(i) };
                // A material never handed over (listed by story flag 0x4837, `FUN_0028f9a8`) shows its name as "?".
                let label = if totals && moogle.total_at(i) == 0 { masked(text, &name(mats[i])) } else { name(mats[i]) };
                row(scene, k - first, label, pick(k, s.mat, true), Some(format!("{v}")));
                // The NEW / VIEWED tag of a material handed over for the first time at this opening (session `+8`).
                put_tag(m, scene, k - first, tag_of(synth::material_mark(&s.marks, i)), tag_at(row_y(k - first)));
            }
            if !s.slots.is_empty() {
                hand(scene, s.mat - first);
            }
        }
        _ => {}
    }
    match s.page {
        synth::Page::Notice | synth::Page::Opening => {
            // `FUN_0028e980`: the first variable of the text is the item's kind (a material's category), the second its name.
            let item = s.text_item.and_then(|id| m.wares.iter().find(|w| w.id == id));
            let args: Vec<Vec<Token>> = item
                .map(|w| vec![t.kind_text(w.kind, w.flag1).map(|k| message(text, k)).unwrap_or_default(), message(text, u32::from(w.name))])
                .unwrap_or_default();
            window(scene, fill(message(text, s.text), &args));
        }
        synth::Page::Result => {
            let mut v: Vec<Token> = message(text, synth::msg::MADE).into_iter().filter(|t| *t != Token::End).collect();
            v.extend(literal(text, "  "));
            v.extend(name(s.item));
            window(scene, v);
        }
        synth::Page::LevelUp => {
            let mut v: Vec<Token> = message(text, synth::msg::LEVEL_UP).into_iter().filter(|t| *t != Token::End).collect();
            v.extend(literal(text, "  "));
            v.extend(message(text, s.text));
            window(scene, v);
        }
        _ => {}
    }
}

/// The tab grid of pages 10 and 0xb (`FUN_0028aaa8`, numbers `FUN_0028f850`, plates `FUN_0028fea0`): the four rank tabs and
/// the thirteen category tabs, each a name and the sum of the displayed totals. A category tab shows only when the record's
/// sum is above 0 or story flag 0x4837 is up. A tab the count-up marked is drawn bright [inferred: the original swaps its
/// plate's sprite group for `FUN_0028f468(0)`].
fn draw_tabs(m: &ShopMenu, scene: &mut MenuScene, text: &MenuText, b: &synth::Book, s: &synth::Session, moogle: &Moogle) {
    let Some(places) = m.seqd.as_ref().and_then(tab_places) else { return };
    let t = b.t;
    let shown = synth::tab_sums(b, &|i| s.shown.get(i).copied().unwrap_or_else(|| moogle.total_at(i)));
    let record = synth::tab_sums(b, &|i| moogle.total_at(i));
    for (k, &(name_at, number_at)) in places.iter().enumerate() {
        let (name, number, lit, show) = if k < 4 {
            (t.rank_names[3 - k], shown.ranks[3 - k], s.marks.rank_tabs >> k & 1 != 0, true)
        } else {
            let c = k - 4;
            (t.category_names[c], shown.cats[c], s.marks.cat_tabs >> c & 1 != 0, record.cats[c] > 0 || s.all)
        };
        let (a, z) = (m.nodes.tabs[2 * k], m.nodes.tabs[2 * k + 1]);
        if !show {
            continue;
        }
        let color = if lit { at::SELECTED } else { at::IDLE };
        set_label(scene, a, Some(text_label(text, message(text, u32::from(name)), color, 0, name_at)));
        set_label(scene, z, Some(text_label(text, literal(text, &format!("{number}")), color, 0, number_at)));
    }
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut m: ResMut<ShopMenu>,
    mut req: ResMut<ShopRequest>,
    mut scene: ResMut<MenuScene>,
    mut images: ResMut<Assets<Image>>,
    disc: Res<Disc>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    mut progress: Option<ResMut<Progress>>,
    menu: Res<crate::menu::Menu>,
) {
    if let Some(event) = req.0.take() {
        if !m.tried && scene.text.is_some() {
            load(&mut m, &disc, &mut scene, &mut images);
        }
        let busy = m.open.is_some() || menu.phase != crate::menu::Phase::Closed;
        if !m.loaded || busy {
            info!("shop: event {event:#x} refused (loaded {}, a menu is up {busy})", m.loaded);
            return;
        }
        let kinds: std::collections::HashMap<u16, u8> = m.wares.iter().map(|w| (w.id, w.kind)).collect();
        let bit = |n: u16| progress.as_deref().is_some_and(|p| p.record_bit(n));
        let Some(stock) = stock(&m.shops, event, &bit, &|id| kinds.get(&id).copied()) else {
            info!("shop: event {event:#x} has no record in the shop file");
            return;
        };
        if let (Some(b), Some(p)) = (stock.set_bit, progress.as_deref_mut()) {
            p.set_record_bit(b);
        }
        let rows = top_rows(&stock);
        let sell = sell_wares(&m, progress.as_deref());
        info!(
            "shop: open menu kind 4 (FUN_0024f0b8(4, {event:#x}), FUN_0025f168): header {:#x}, rows {:?}, {} wares {:x?}, record bit set {:?}, {} sellable",
            stock.message,
            rows,
            stock.items.len(),
            stock.items,
            stock.set_bit,
            sell.len()
        );
        let third = stock.rows_bits & 2 != 0;
        // `FUN_0025fef0` -> `FUN_00265280`: the buy list's items whose save bit is clear are this visit's NEW items;
        // `FUN_0025fcd8` -> `FUN_00265498`: the first-visit text (page 7) while the class's visit bit is clear.
        let fresh = progress.as_deref().map_or(0, |p| fresh_at_open(&m.shops, &stock.items, &|n| p.shop_seen(n)));
        let first_bit = progress.as_deref().and_then(|p| first_visit_bit(&stock, &|b| p.shop_visit(b)));
        let mut flow = Flow::new(rows);
        if let Some(bit) = first_bit {
            flow.page = Page::First;
            info!("shop: first visit of this class (save +0x3724 bit {bit} clear): page 7, text {:#x} (FUN_00262140)", m.first_msgs[usize::from(bit)]);
        }
        info!("shop: {} NEW items (save +0x3714 bits clear, FUN_00265280): places {:#x}", fresh.count_ones(), fresh);
        m.open = Some(Open { event, flow, stock, sell, closing: 0, synth: None, fresh, first_bit, tags: Tags::default() });
        scene.hidden = false;
        scene.dim = crate::menu::dim_alpha(crate::menu::DIM_BRIGHTNESS);
        hide_all(&m, &mut scene);
        set_layout(&m, &mut scene, 0);
        refresh_tags(&mut m, progress.as_deref());
        let munny = progress.as_deref().map_or(0, |p| p.wallet());
        let owned = |id: u16| progress.as_deref().map_or(0, |p| p.item_count(id));
        redraw(&m, &mut scene, &owned, munny, &moogle_of(progress.as_deref()));
        menu_sound::play(&mut se, if third { snd::OPEN_THIRD_ROW } else { snd::OPEN });
        keys.reset_all();
        return;
    }
    m.held = keys.get_pressed().next().is_some();
    let Some(o) = m.open.as_ref() else { return };
    if o.closing > 0 {
        keys.reset_all();
        return;
    }
    let pressed = |k: KeyCode| keys.just_pressed(k);
    let key = if pressed(KeyCode::Enter) || pressed(KeyCode::NumpadEnter) {
        Some(Key::Start)
    } else if pressed(KeyCode::Escape) || pressed(KeyCode::Backspace) || pressed(KeyCode::KeyK) {
        Some(Key::Cancel)
    } else if pressed(KeyCode::KeyJ) {
        Some(Key::Confirm)
    } else if pressed(KeyCode::KeyX) {
        Some(Key::Switch)
    } else if pressed(KeyCode::ArrowUp) {
        Some(Key::Up)
    } else if pressed(KeyCode::ArrowDown) {
        Some(Key::Down)
    } else if pressed(KeyCode::ArrowLeft) {
        Some(Key::Left)
    } else if pressed(KeyCode::ArrowRight) {
        Some(Key::Right)
    } else {
        None
    };
    keys.reset_all();
    let Some(key) = key else { return };
    if o.synth.is_some() {
        synth_key(&mut m, &mut scene, &mut se, progress.as_deref_mut(), key);
        return;
    }
    let munny = progress.as_deref().map_or(0, |p| p.wallet());
    let buy = buy_wares(&m, &o.stock);
    let sell = o.sell.clone();
    let mut flow = o.flow.clone();
    let act = {
        let owned = |id: u16| progress.as_deref().map_or(0, |p| p.item_count(id));
        flow.key(key, &View { buy: &buy, sell: &sell, owned: &owned, munny })
    };
    let before = o.flow.page;
    let first_bit = o.first_bit;
    match act {
        Act::None => {}
        Act::Moved => menu_sound::play(&mut se, menu_sound::CURSOR),
        Act::Refused => {
            menu_sound::play(&mut se, menu_sound::REFUSED);
            info!("shop: page {} key {key:?} refused (FUN_00265550 / FUN_00261a28)", before.number());
        }
        Act::Page => {
            if before == Page::First
                && let (Some(p), Some(bit)) = (progress.as_deref_mut(), first_bit)
            {
                p.set_shop_visit(bit);
                info!("shop: page 7 dismissed: save +0x3724 bit {bit} set (FUN_002621a0), page 0");
            }
            let sound = if matches!(before, Page::Question { .. }) && key == Key::Confirm {
                snd::BACK
            } else if key == Key::Cancel {
                menu_sound::CANCEL
            } else {
                crate::menu_status::CONFIRM
            };
            menu_sound::play(&mut se, sound);
            info!("shop: page {} -> page {} (item {:#x} x{} at {})", before.number(), flow.page.number(), flow.deal.item, flow.deal.qty, flow.deal.price);
        }
        Act::Deal => {
            menu_sound::play(&mut se, snd::DEAL);
            let sell_page = matches!(before, Page::Question { sell: true });
            if let Some(p) = progress.as_deref_mut() {
                let left = deal(p, sell_page, &flow.deal);
                info!(
                    "shop: {} item {:#x} x{} for {} (FUN_00261d70 case 1): munny now {left}, owned now {}",
                    if sell_page { "sold" } else { "bought" },
                    flow.deal.item,
                    flow.deal.qty,
                    flow.deal.total(),
                    p.item_count(flow.deal.item)
                );
            }
            let new_sell = sell_wares(&m, progress.as_deref());
            let n = if sell_page { new_sell.len() } else { buy.len() };
            if let Some(o) = m.open.as_mut() {
                o.sell = new_sell;
            }
            flow.list = flow.list.min(n.saturating_sub(1));
            // FUN_00261d70 calls FUN_00283480 when the sold item is gone; an empty sell list goes back to the top
            // [inferred: the list page has no row to show].
            if sell_page && n == 0 {
                flow.page = Page::Top;
            }
        }
        Act::ThirdRow => {
            let Some(p) = progress.as_deref_mut() else { return };
            let ShopMenu { mix, wares, open, .. } = &mut *m;
            let Some((mix, t, mats)) = mix.as_ref() else {
                menu_sound::play(&mut se, menu_sound::REFUSED);
                info!("shop: the third row has no mixdata.bar; refused");
                return;
            };
            let rank = |id: u16| wares.iter().find(|w| w.id == id).map_or(0, |w| w.rank);
            let flag1 = |id: u16| wares.iter().find(|w| w.id == id).map_or(0, |w| w.flag1);
            let book = synth::Book { mix, t, mats, rank: &rank, flag1: &flag1 };
            let (s, o) = synth::Session::start(&book, &mut Inv(p));
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            info!(
                "synth: open the third row (FUN_00284c10): handed over {:x?} (FUN_002887c8), recipes taken {:x?} (FUN_002889e0), notices {:#x}, page {:#x} (rows {:?}), unlocks {:#x}, level {}, points {}",
                o.handed,
                o.recipes,
                o.notices,
                s.page.number(),
                s.rows,
                p.moogle.unlocks,
                p.moogle.level,
                p.moogle.points
            );
            if let Some(o) = open.as_mut() {
                o.synth = Some(s);
            }
            set_layout(&m, &mut scene, 3);
        }
        Act::Close => {
            begin_close(&mut m, &mut scene, &mut se, before.screen(), before.number());
            return;
        }
    }
    if act == Act::ThirdRow {
        refresh_tags(&mut m, progress.as_deref());
        redraw(&m, &mut scene, &|id| progress.as_deref().map_or(0, |p| p.item_count(id)), progress.as_deref().map_or(0, |p| p.wallet()), &moogle_of(progress.as_deref()));
        return;
    }
    let screen = (before.screen(), flow.page.screen());
    if let Some(o) = m.open.as_mut() {
        o.flow = flow;
    }
    if screen.0 != screen.1 {
        set_layout(&m, &mut scene, screen.1);
    }
    // `FUN_00260a78`, `FUN_00264668`: on the buy list the item under the cursor is marked seen (`FUN_00265348`).
    if let (Some(p), Some(o)) = (progress.as_deref_mut(), m.open.as_ref())
        && o.flow.page == (Page::List { sell: false })
    {
        let wares = buy_wares(&m, &o.stock);
        if let Some(place) = wares.get(o.flow.list).and_then(|w| item_place(&m.shops, w.id)).filter(|&n| o.fresh >> n & 1 != 0 && !p.shop_seen(n)) {
            p.set_shop_seen(place);
            info!("shop: item place {place} seen (save +0x3714 bit set, FUN_00265348)");
        }
    }
    refresh_tags(&mut m, progress.as_deref());
    let munny = progress.as_deref().map_or(0, |p| p.wallet());
    let owned = |id: u16| progress.as_deref().map_or(0, |p| p.item_count(id));
    redraw(&m, &mut scene, &owned, munny, &moogle_of(progress.as_deref()));
}

/// While the shop is up the field is frozen and the pad is the menu's (after `menu::input` wrote its own state).
fn hold(m: Res<ShopMenu>, mut pause: ResMut<Pause>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    if m.is_open() {
        pause.menu = true;
        keys.reset_all();
    }
}

/// Page 10's task (`FUN_00285b30`): while `+0x112d` is set it runs the count-up (`FUN_0028f490`) once a frame, with
/// "any button held" as its skip flag, and the page is redrawn when the totals moved.
fn count_up(m: &mut ShopMenu, scene: &mut MenuScene, p: &Progress, ticks: u32) {
    let held = m.held;
    let ShopMenu { mix, wares, open, .. } = &mut *m;
    let (Some((mix, t, mats)), Some(o)) = (mix.as_ref(), open.as_mut()) else { return };
    let Some(s) = o.synth.as_mut().filter(|s| s.page == synth::Page::Materials { totals: true }) else { return };
    let rank = |id: u16| wares.iter().find(|w| w.id == id).map_or(0, |w| w.rank);
    let flag1 = |id: u16| wares.iter().find(|w| w.id == id).map_or(0, |w| w.flag1);
    let book = synth::Book { mix, t, mats, rank: &rank, flag1: &flag1 };
    let mut moved = false;
    for _ in 0..ticks {
        if !s.count_up(&book, &p.moogle, held) {
            break;
        }
        moved = true;
    }
    if moved {
        redraw(m, scene, &|id| p.item_count(id), p.wallet(), &p.moogle);
    }
}

fn tick(mut m: ResMut<ShopMenu>, mut scene: ResMut<MenuScene>, real: Res<Time<Real>>, progress: Option<Res<Progress>>) {
    let Some(o) = m.open.as_ref() else { return };
    let ticks = scene.advance(real.delta_secs());
    if o.closing == 0 {
        if let Some(p) = progress.as_deref() {
            count_up(&mut m, &mut scene, p, ticks);
        }
        return;
    }
    let c = o.closing + ticks;
    scene.dim = crate::menu::dim_alpha(crate::menu::close_brightness(c));
    if c >= crate::menu::CLOSE_FRAMES {
        let event = o.event;
        hide_all(&m, &mut scene);
        scene.hidden = true;
        scene.dim = 0.0;
        m.open = None;
        info!("shop: menu kind 4 ({event:#x}) closed: back to the field (FUN_001038a0)");
    } else if let Some(o) = m.open.as_mut() {
        o.closing = c;
    }
}

/// The third row: the moogle's synthesis (pages 8..0x17 of `FUN_00260540`; docs/menus.md section 23).
///
/// The data is `menu/us/mixdata.bar` (`FUN_00291ae0` loads it into `DAT_0036d19c`; entry k is `FUN_0029e630(.., k)`):
/// 0 `reci` the recipes, 1 `cond` the collection rewards, 2 `leve` the moogle's levels, 3 `exp` the points per
/// material rank and the Recipes row's story flag. The record it keeps is `progress::Moogle` (save +0x3728).
pub mod synth {
    use crate::progress::{MOOGLE_MATERIALS, Moogle};

    /// Addresses in the executable (read at run time).
    pub mod addr {
        /// The seven rows of page 9 (u16 message ids; `FUN_00284ea8` names row k by entry k).
        pub const ROWS: u32 = 0x0038_25e0;
        /// The bonus materials by rank (`FUN_002910f0(rank, k)`): k 0 at 0x382690, k 1 at 0x382688, k 2 at 0x382680.
        pub const BONUS: [u32; 3] = [0x0038_2690, 0x0038_2688, 0x0038_2680];
        /// A byte per rank: the unlock bit that halves the costs on the Creations list (`FUN_00290618`).
        pub const RANK_BITS: u32 = 0x0038_2578;
        /// The help line of each detail row kind (`FUN_00289bf8`).
        pub const HELP: u32 = 0x0038_2548;
        /// The notices of the opening (`FUN_00287950`, `FUN_002879e8`): one per bit of the state's +0x130.
        pub const NOTICES: u32 = 0x0038_2628;
        /// The level names (`FUN_00288480`: entry = the new level).
        pub const LEVEL_NAMES: u32 = 0x0038_26a0;
        /// The synthesis screens' layout triple (enter, leave, hold) (`FUN_00284c10` -> `FUN_0025ee78(0x3825b8, ..)`).
        pub const LAYOUTS: u32 = 0x0038_25b8;
        /// Three category bytes (`FUN_00291560`).
        pub const SPECIAL: u32 = 0x0038_2698;
        /// A message per material category (`FUN_0028a1b8`, `FUN_002879e8`: `PTR_DAT_00382640[item +4]`, 13 entries).
        pub const CATEGORY_NAMES: u32 = 0x0038_2640;
        /// The icon of a material category (`FUN_0028a1b8`: the table at 0x382660, indexed by item +4).
        pub const CATEGORY_ICONS: u32 = 0x0038_2660;
        /// The name of each rank tab (`FUN_0028aaa8`: tab k reads entry 3 - k).
        pub const RANK_NAMES: u32 = 0x0038_2638;
        /// Pairs (item type, message) ended by a negative type: the name of an item type (`FUN_002553c0`).
        pub const TYPE_NAMES: u32 = 0x0038_0a60;
        /// Triples (item type, icon, icon) ended by a negative type (`FUN_00255368`; the first icon with a 0 flag).
        pub const TYPE_ICONS: u32 = 0x0038_09d0;
    }

    /// Message ids the pages pass as immediates.
    pub mod msg {
        /// `FUN_00288fc0`: the empty list.
        pub const NOTHING: u32 = 0xbe22;
        /// `FUN_0028ee28`: the result window.
        pub const MADE: u32 = 0xbef6;
        /// The make row's refusals (the detail page's case 0, 0x286f40): a higher-rank entry without the higher-rank
        /// bonus, too few materials, the result at 99.
        pub const NEED_RANK: u32 = 0xbef4;
        pub const SHORT: u32 = 0xbef5;
        pub const FULL: u32 = 0xbf06;
        /// `FUN_00289bf8`: the first and the last detail row, the "in stock" label.
        pub const MAKE_ROW: u32 = 0xbf1a;
        pub const QUIT_ROW: u32 = 0xbf1b;
        pub const IN_STOCK: u32 = 0xbf21;
        /// `FUN_00288480`: the level-up notice.
        pub const LEVEL_UP: u32 = 0xbed1;
        /// `FUN_0028aa40`, `FUN_00286378`: the titles of the material pages.
        pub const COLLECTION: u32 = 0xbcdf;
        pub const MATERIALS: u32 = 0xbd90;
        /// `FUN_0028aa40`: the title of page 0xb; `FUN_0028a1b8`: the empty list, and the text of a reward that opens
        /// Free Development set 0, set 1 or an unlock bit.
        pub const REWARDS: u32 = 0xbce1;
        pub const NO_REWARDS: u32 = 0xc194;
        pub const REWARD_SET0: u32 = 0xc1c3;
        pub const REWARD_SET1: u32 = 0xcc7d;
        pub const REWARD_UNLOCK: u32 = 0xc1c4;
        /// `FUN_0028a1b8`: the text that fills the variable of an unlock reward's message, and starts the hidden one.
        pub const REWARD_HEAD: u32 = 0xc1c5;
    }

    /// The type of the synthesis materials (`FUN_00284798` lists the item rows of type 0x10, at most 0x32).
    pub const MATERIAL_TYPE: u8 = 0x10;

    /// A recipe (`reci`, 0x20 bytes from +0x10).
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Recipe {
        /// +0: the recipe's item id (owning it teaches a recipe of kind 0, `FUN_002889e0`).
        pub id: u16,
        /// +2: 0 a recipe item, 1..3 a Free Development set (unlock bit 0..2).
        pub unlock: u8,
        /// +3: the rank (the bonus materials, the halving bit, the points).
        pub rank: u8,
        /// +4 the item, +6 the higher-rank item (0: none).
        pub result: u16,
        pub upgraded: u16,
        /// +8: up to six (material, count) pairs; the list ends at an id that is not positive.
        pub mats: Vec<(u16, i16)>,
    }

    /// A level (`leve`, 0xc bytes): +0 its name, +2 a text, +4 the unlock bit it grants (negative none), +8 the points.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Level {
        pub name: u16,
        pub text: u16,
        pub unlock: i16,
        pub points: u32,
    }

    /// A collection reward (`cond`, 0xc bytes from +0x10; `FUN_00291140`): +0 its text, +2 the reward (the item for
    /// kind 0; else 0 and 1 open the Free Development sets 0 and 1, any other value is an unlock bit), +4 the kind (0 an
    /// item), +5..+7 the condition `FUN_00291418` tests, +8 the count it needs, +0xa the record bit a bit reward sets.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Cond {
        pub text: u16,
        pub reward: u16,
        pub kind: u8,
        pub a: u8,
        pub b: u8,
        pub c: u8,
        pub count: u16,
        pub bit: u16,
    }

    /// `mixdata.bar`.
    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Mix {
        pub recipes: Vec<Recipe>,
        pub levels: Vec<Level>,
        /// `exp` +0x10..+0x16: the points of a material by its rank (`FUN_00290468`), +0x18 the factor of the first
        /// bonus, +0x1c a story flag and +0x1e the unlock bit it grants on the opening (`FUN_00284c10`).
        pub exp: [u16; 4],
        pub factor: f32,
        pub story_flag: u16,
        pub story_bit: u16,
        /// `cond`: the collection rewards (`FUN_00291140`).
        pub conds: Vec<Cond>,
    }

    impl Mix {
        pub fn parse(bar: &[u8]) -> Option<Mix> {
            let entries = kh2_bar::parse(bar).ok()?;
            let get = |tag: &str| entries.iter().find(|e| e.tag == tag).map(|e| e.data);
            let u16_at = |d: &[u8], o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
            let u32_at = |d: &[u8], o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            let reci = get("reci")?;
            let mut recipes = Vec::new();
            for i in 0..u32_at(reci, 8)? as usize {
                let r = 0x10 + 0x20 * i;
                let mut mats = Vec::new();
                for k in 0..6 {
                    let (id, n) = (u16_at(reci, r + 8 + 4 * k)?, u16_at(reci, r + 10 + 4 * k)? as i16);
                    if id as i16 <= 0 {
                        break;
                    }
                    mats.push((id, n));
                }
                recipes.push(Recipe { id: u16_at(reci, r)?, unlock: *reci.get(r + 2)?, rank: *reci.get(r + 3)?, result: u16_at(reci, r + 4)?, upgraded: u16_at(reci, r + 6)?, mats });
            }
            let leve = get("leve")?;
            let levels = (0..u32_at(leve, 8)? as usize)
                .map(|i| {
                    let r = 0x10 + 0xc * i;
                    Some(Level { name: u16_at(leve, r)?, text: u16_at(leve, r + 2)?, unlock: u16_at(leve, r + 4)? as i16, points: u32_at(leve, r + 8)? })
                })
                .collect::<Option<Vec<_>>>()?;
            let exp = get("exp")?;
            let cond = get("cond")?;
            let conds = (0..u32_at(cond, 8)? as usize)
                .map(|i| {
                    let r = 0x10 + 0xc * i;
                    Some(Cond { text: u16_at(cond, r)?, reward: u16_at(cond, r + 2)?, kind: *cond.get(r + 4)?, a: *cond.get(r + 5)?, b: *cond.get(r + 6)?, c: *cond.get(r + 7)?, count: u16_at(cond, r + 8)?, bit: u16_at(cond, r + 0xa)? })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Mix {
                recipes,
                levels,
                exp: [u16_at(exp, 0x10)?, u16_at(exp, 0x12)?, u16_at(exp, 0x14)?, u16_at(exp, 0x16)?],
                factor: f32::from_bits(u32_at(exp, 0x18)?),
                story_flag: u16_at(exp, 0x1c)?,
                story_bit: u16_at(exp, 0x1e)?,
                conds,
            })
        }
    }

    /// The executable's tables (`addr`).
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Tables {
        pub rows: [u16; 7],
        pub bonus: [[u16; 4]; 3],
        pub rank_bits: [u8; 4],
        pub help: [u16; 5],
        pub notices: [u16; 6],
        pub level_names: [u16; 9],
        pub layouts: [u16; 3],
        /// The categories a collection reward's condition `0x10` / `0xf` splits on (`FUN_00291560`: the bytes at 0x382698).
        pub special: [u8; 3],
        /// `FUN_0028a1b8`: the category names and icons of the materials (`addr::CATEGORY_NAMES`, `CATEGORY_ICONS`).
        pub category_names: [u16; 13],
        pub category_icons: [u16; 13],
        /// `FUN_0028aaa8`: the names of the four rank tabs (`addr::RANK_NAMES`, tab k shows entry 3 - k).
        pub rank_names: [u16; 4],
        /// `FUN_002553c0` / `FUN_00255368`: (type, message) and (type, icon) of the other items.
        pub type_names: Vec<(i16, u16)>,
        pub type_icons: Vec<(i16, u16)>,
    }

    impl Tables {
        pub fn read(u16_at: &dyn Fn(u32) -> Option<u16>, u8_at: &dyn Fn(u32) -> Option<u8>) -> Option<Tables> {
            let mut t = Tables::default();
            for (k, v) in t.rows.iter_mut().enumerate() {
                *v = u16_at(addr::ROWS + 2 * k as u32)?;
            }
            for (k, b) in t.bonus.iter_mut().enumerate() {
                for (r, v) in b.iter_mut().enumerate() {
                    *v = u16_at(addr::BONUS[k] + 2 * r as u32)?;
                }
            }
            for (r, v) in t.rank_bits.iter_mut().enumerate() {
                *v = u8_at(addr::RANK_BITS + r as u32)?;
            }
            for (k, v) in t.help.iter_mut().enumerate() {
                *v = u16_at(addr::HELP + 2 * k as u32)?;
            }
            for (k, v) in t.notices.iter_mut().enumerate() {
                *v = u16_at(addr::NOTICES + 2 * k as u32)?;
            }
            for (k, v) in t.level_names.iter_mut().enumerate() {
                *v = u16_at(addr::LEVEL_NAMES + 2 * k as u32)?;
            }
            for (k, v) in t.layouts.iter_mut().enumerate() {
                *v = u16_at(addr::LAYOUTS + 2 * k as u32)?;
            }
            for (k, v) in t.special.iter_mut().enumerate() {
                *v = u8_at(addr::SPECIAL + k as u32)?;
            }
            for k in 0..13 {
                t.category_names[k] = u16_at(addr::CATEGORY_NAMES + 2 * k as u32)?;
                t.category_icons[k] = u16_at(addr::CATEGORY_ICONS + 2 * k as u32)?;
            }
            for (k, v) in t.rank_names.iter_mut().enumerate() {
                *v = u16_at(addr::RANK_NAMES + 2 * k as u32)?;
            }
            for k in 0..64u32 {
                let ty = u16_at(addr::TYPE_NAMES + 4 * k)? as i16;
                if ty < 0 {
                    break;
                }
                t.type_names.push((ty, u16_at(addr::TYPE_NAMES + 4 * k + 2)?));
            }
            for k in 0..64u32 {
                let ty = u16_at(addr::TYPE_ICONS + 6 * k)? as i16;
                if ty < 0 {
                    break;
                }
                t.type_icons.push((ty, u16_at(addr::TYPE_ICONS + 6 * k + 2)?));
            }
            Some(t)
        }

        /// `FUN_002910f0(rank, k)`.
        pub fn bonus_item(&self, rank: u8, k: usize) -> u16 {
            self.bonus.get(k).and_then(|b| b.get(rank as usize)).copied().unwrap_or(0)
        }

        /// The message that names an item's kind (`FUN_0028a1b8`, `FUN_002879e8`): a material (type 0x10) its category's
        /// (`PTR_DAT_00382640[item +4]`), any other item its type's (`FUN_002553c0`, none for a type not in its table).
        pub fn kind_text(&self, kind: u8, category: u8) -> Option<u32> {
            if kind == MATERIAL_TYPE {
                return self.category_names.get(usize::from(category)).map(|&m| u32::from(m));
            }
            self.type_names.iter().find(|&&(t, _)| t == i16::from(kind)).map(|&(_, m)| u32::from(m))
        }

        /// The icon in front of a reward item's name (`FUN_001a1f40(item, icon)`): a material's category icon
        /// (`PTR_DAT_00382660`), else the type's (`FUN_00255368(type, 0)`, 0xb for a type not in its table).
        pub fn item_icon(&self, kind: u8, category: u8) -> u8 {
            if kind == MATERIAL_TYPE {
                return self.category_icons.get(usize::from(category)).map_or(0xb, |&i| i as u8);
            }
            self.type_icons.iter().find(|&&(t, _)| t == i16::from(kind)).map_or(0xb, |&(_, i)| i as u8)
        }
    }

    /// What the second column of a page 0xb row shows (`FUN_0028a1b8`, the buffer at state `+0x52a + 0x40 k`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum RewardText {
        /// An item reward earned: the item's name behind its icon (`FUN_001a1f40`).
        Item { icon: u8, name: u32 },
        /// A reward not earned (listed by story flag 0x4837): the message `head`, then a "?" for each character of the
        /// message `mask` (`FUN_0029bb48`). The head is the item's kind for an item reward, else message 0xc1c5.
        Hidden { head: Option<u32>, mask: u32 },
        /// An unlock reward earned: the message `text` with its variable replaced by the text of message 0xc1c5
        /// (`FUN_00191a30`).
        Filled { text: u32 },
    }

    /// `FUN_0028a1b8`: the second column of a reward's row. `item` is the reward item's (type, category, name message)
    /// for an item reward (kind 0); an item reward without an item row has no text.
    pub fn reward_text(t: &Tables, c: &Cond, met: bool, item: Option<(u8, u8, u16)>) -> Option<RewardText> {
        if c.kind == 0 {
            let (kind, category, name) = item?;
            return Some(if met {
                RewardText::Item { icon: t.item_icon(kind, category), name: u32::from(name) }
            } else {
                RewardText::Hidden { head: t.kind_text(kind, category), mask: u32::from(name) }
            });
        }
        let text = match c.reward {
            0 => msg::REWARD_SET0,
            1 => msg::REWARD_SET1,
            _ => msg::REWARD_UNLOCK,
        };
        Some(if met { RewardText::Filled { text } } else { RewardText::Hidden { head: Some(msg::REWARD_HEAD), mask: text } })
    }

    /// What the pages read: the data, the tables, the material list (`FUN_00284798`: the item ids of type 0x10 in
    /// table order) and an item's rank byte (item +5).
    pub struct Book<'a> {
        pub mix: &'a Mix,
        pub t: &'a Tables,
        pub mats: &'a [u16],
        pub rank: &'a dyn Fn(u16) -> u8,
        /// An item's +4 byte (the category the collection rewards test).
        pub flag1: &'a dyn Fn(u16) -> u8,
    }

    /// The inventory and the record the pages change.
    pub trait Store {
        fn moogle(&self) -> &Moogle;
        fn moogle_mut(&mut self) -> &mut Moogle;
        /// `FUN_001a1cc8`.
        fn count(&self, id: u16) -> u8;
        /// `FUN_001a1a18(id, n)`.
        fn add(&mut self, id: u16, n: u8) -> u8;
        /// `FUN_001a1c30(id, n)`.
        fn remove(&mut self, id: u16, n: u8);
        /// `FUN_001802e0(flag)`: a story flag.
        fn story(&self, flag: u16) -> bool;
        /// `FUN_001a52b0(n)`: sets record bit `n` (a collection reward's bit).
        fn set_record_bit(&mut self, n: u16);
    }

    /// `FUN_002910b0`: a material's place in the list.
    pub fn slot(b: &Book, id: u16) -> Option<usize> {
        b.mats.iter().take(MOOGLE_MATERIALS).position(|&m| m == id)
    }

    /// `FUN_00290400`: the stock of a material, -1 for an id that is not one.
    pub fn stock(b: &Book, m: &Moogle, id: u16) -> i64 {
        slot(b, id).map_or(-1, |i| i64::from(m.stock_at(i)))
    }

    /// `FUN_00290618(rank, halve)` for the list `kind` (0 Creations, 1 Free Development, 2 Recipes): on the Creations
    /// list a rank whose bit (0x382578) is set needs half; the Halve bonus halves again.
    pub fn divisor(b: &Book, m: &Moogle, kind: u8, rank: u8, halve: bool) -> i32 {
        let base = if kind == 0 && b.t.rank_bits.get(rank as usize).is_some_and(|&n| m.unlocked(u32::from(n))) { 2 } else { 1 };
        if halve { 2 * base } else { base }
    }

    /// The count a recipe line needs: `count / div` rounded up, at least 1 (`FUN_002906e0`, `FUN_00290ef8`).
    pub fn need(count: i16, div: i32) -> i32 {
        let (q, r) = (i32::from(count) / div, i32::from(count) % div);
        if r != 0 || q == 0 { q + 1 } else { q }
    }

    /// `FUN_002906e0(id, upgrade, halve)`: every line in stock; the higher-rank bonus (k 2) and the Halve bonus (k 1)
    /// each need one of their material; both at once need unlock bit 5.
    pub fn can_make(b: &Book, m: &Moogle, kind: u8, r: &Recipe, upgrade: bool, halve: bool) -> bool {
        let div = divisor(b, m, kind, r.rank, halve);
        if r.mats.iter().any(|&(id, n)| stock(b, m, id) < i64::from(need(n, div))) {
            return false;
        }
        if upgrade && stock(b, m, b.t.bonus_item(r.rank, 2)) == 0 {
            return false;
        }
        if halve && stock(b, m, b.t.bonus_item(r.rank, 1)) == 0 {
            return false;
        }
        !(upgrade && halve && !m.unlocked(5))
    }

    /// `FUN_00290468(id, doubled)`: the points of a make: each line's full count times the points of the material's
    /// rank (`exp`; a rank above 3 counts -1), times the factor (`exp` +0x18) with the first bonus.
    pub fn points(b: &Book, r: &Recipe, doubled: bool) -> i32 {
        let p: i32 = r.mats.iter().map(|&(id, n)| b.mix.exp.get((b.rank)(id) as usize).map_or(-1, |&e| i32::from(e)) * i32::from(n)).sum();
        if doubled { (p as f32 * b.mix.factor) as i32 } else { p }
    }

    /// `FUN_00290058`: the entries of a list (an entry is 2r for recipe r's item, 2r + 1 for its higher rank). The
    /// Creations list (0): every entry made. Free Development (1): the recipes of sets 1..3 whose set bit (0..2) is
    /// unlocked, learned and not made. Recipes (2): the learned recipes of kind 0 not made. At most 0x2d. The entries
    /// flagged 0x80 that story flag 0x4837 adds (`entries_all`) are left out here.
    pub fn entries(b: &Book, m: &Moogle, kind: u8) -> Vec<u16> {
        entries_all(b, m, kind, false)
    }

    /// The flag `FUN_00290058` puts on an entry that only the story flag 0x4837 lists: shown as a row of "?" and never
    /// opened.
    pub const HIDDEN: u16 = 0x80;

    /// `FUN_00290058` with story flag 0x4837 (`all`) read: the lists also hold, flagged `HIDDEN`, the entries the player
    /// cannot have yet. Creations: every entry not made (the higher rank's only for a recipe that has one). Free
    /// Development and Recipes: a recipe of that list's kind that is not learned (or is made, and not learned).
    pub fn entries_all(b: &Book, m: &Moogle, kind: u8, all: bool) -> Vec<u16> {
        let n = b.mix.recipes.len() as u32;
        let mut v = Vec::new();
        if kind == 0 {
            for e in 0..(2 * n).saturating_sub(1) {
                if Moogle::bit(m.made, e) {
                    v.push(e as u16);
                } else if all && (e & 1 == 0 || b.mix.recipes.get((e >> 1) as usize).is_some_and(|r| r.upgraded != 0)) {
                    v.push(e as u16 | HIDDEN);
                }
            }
        } else {
            for (i, r) in b.mix.recipes.iter().enumerate() {
                let e = 2 * i as u32;
                if !Moogle::bit(m.learned, e) || Moogle::bit(m.made, e) {
                    if all && !Moogle::bit(m.made, e) && (if kind == 1 { r.unlock != 0 } else { r.unlock == 0 }) {
                        v.push(e as u16 | HIDDEN);
                    }
                    continue;
                }
                let on = if kind == 1 { (1..=3).contains(&r.unlock) && m.unlocked(u32::from(r.unlock) - 1) } else { r.unlock == 0 };
                if on {
                    v.push(e as u16);
                }
            }
        }
        v.truncate(0x2d);
        v
    }

    /// The recipe of an entry and whether it is the higher rank.
    pub fn recipe<'a>(b: &Book<'a>, e: u16) -> Option<(&'a Recipe, bool)> {
        b.mix.recipes.get(usize::from(e >> 1)).map(|r| (r, e & 1 != 0))
    }

    /// `FUN_00291b58` on a list and the grey rows of `FUN_00288fc0`: an entry can be opened when some way of making
    /// it is in stock (a higher-rank entry with bit 4 counts its bonus; bit 3 allows the Halve bonus).
    pub fn entry_open(b: &Book, m: &Moogle, kind: u8, e: u16) -> bool {
        let Some((r, odd)) = recipe(b, e) else { return false };
        let up = odd && m.unlocked(4);
        let three = m.unlocked(3);
        if !up || !three || m.unlocked(5) {
            can_make(b, m, kind, r, up, false) || can_make(b, m, kind, r, up, three)
        } else {
            can_make(b, m, kind, r, up, false)
        }
    }

    /// `FUN_002908c0(k, rank, check)`: may bonus k (0 points x2, 1 Halve, 2 higher rank) be offered. `detail`: on the
    /// detail pages of lists 1 and 2 (pages 0xf, 0x11) the higher rank is not; k 2 needs unlock bit 4, k 0 and 1
    /// bit 3; `check` also needs the bonus material in stock.
    pub fn bonus_ok(b: &Book, m: &Moogle, kind: u8, detail: bool, k: usize, rank: u8, check: bool) -> bool {
        let mut ok = !(detail && kind != 0 && k == 2);
        if (k == 2 && !m.unlocked(4)) || (k < 2 && !m.unlocked(3)) {
            ok = false;
        }
        if check && stock(b, m, b.t.bonus_item(rank, k)) == 0 {
            ok = false;
        }
        ok
    }

    /// The detail page's rows (`FUN_00286580`: the state's +0x106b): bit 0 Synthesize and bit 4 Quit always, bits 1..3
    /// the bonuses that `bonus_ok` offers (bit 3 only for a recipe with a higher rank). And the first cursor row: 0 when
    /// the entry's item can be made as it is, else 2 (3 for a higher-rank entry) [inferred: the widget clamps a row past
    /// the last to the last].
    pub fn detail_rows(b: &Book, m: &Moogle, kind: u8, e: u16) -> (u8, usize) {
        let Some((r, odd)) = recipe(b, e) else { return (0x11, 1) };
        let mut rows = 0x11u8;
        if bonus_ok(b, m, kind, false, 0, r.rank, false) {
            rows |= 2;
        }
        if bonus_ok(b, m, kind, false, 1, r.rank, false) {
            rows |= 4;
        }
        if r.upgraded != 0 && bonus_ok(b, m, kind, false, 2, r.rank, false) {
            rows |= 8;
        }
        let cur = if odd { 3 } else if can_make(b, m, kind, r, false, false) { 0 } else { 2 };
        (rows, cur.min(rows.count_ones() as usize - 1))
    }

    /// `FUN_00290978`: the kind (0 make, 1..3 the bonuses, 4 quit) of shown row `row`.
    pub fn row_kind(rows: u8, row: usize) -> Option<u8> {
        (0..5u8).filter(|k| rows >> k & 1 != 0).nth(row)
    }

    /// The make row's answer (the detail page's case 0).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Make {
        /// Page 0x12: a refusal text.
        Notice(u32),
        /// Page 0x14: the result window for this item.
        Ok(u16),
    }

    /// The detail page's case 0 (0x286f40): with the bonus flags `flags` (bit 0 points x2, 1 Halve, 2 higher rank).
    pub fn make_check(b: &Book, s: &dyn Store, kind: u8, e: u16, flags: u8) -> Make {
        let Some((r, odd)) = recipe(b, e) else { return Make::Notice(msg::SHORT) };
        let m = s.moogle();
        let (up, halve) = (flags & 4 != 0, flags & 2 != 0);
        let can = if up && halve && !m.unlocked(5) {
            if odd { can_make(b, m, kind, r, up, false) } else { can_make(b, m, kind, r, up, false) || can_make(b, m, kind, r, false, halve) }
        } else {
            can_make(b, m, kind, r, up, halve)
        };
        let item = if !odd && !up { r.result } else { r.upgraded };
        if odd && !up {
            Make::Notice(msg::NEED_RANK)
        } else if !can {
            Make::Notice(msg::SHORT)
        } else if s.count(item) > 0x62 {
            Make::Notice(msg::FULL)
        } else {
            Make::Ok(item)
        }
    }

    /// What a make did.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Made {
        pub item: u16,
        pub points: i32,
        /// (material, count) taken from the stock.
        pub used: Vec<(u16, i32)>,
        pub entry: u16,
        pub level: (u8, u8),
        /// The Free Development entries this make's level newly learned (`FUN_00290cc0`'s session marks).
        pub learned: u64,
    }

    /// Pages 0x15 and on: `FUN_00288170` adds the item; `FUN_00290ef8` / `FUN_002909d0` take each line's need and one of
    /// every chosen bonus material from the stock and add the points (at most 99,999); `FUN_00288228` marks the entry
    /// made and seen (the higher rank's entry when the bonus made it); `FUN_00290c28` grants the bits of the levels
    /// reached, then the Free Development sets they open (`FUN_00290cc0`); `FUN_00290bb8` the level.
    pub fn make(b: &Book, s: &mut dyn Store, kind: u8, e: u16, flags: u8, item: u16) -> Option<Made> {
        let (r, odd) = recipe(b, e)?;
        // The bag limit `make_check` tests (more than 0x62 of the item is the FULL notice): a caller that skipped the check
        // gets nothing taken and nothing added.
        if s.count(item) > 0x62 {
            return None;
        }
        s.add(item, 1);
        let points = points(b, r, flags & 1 != 0);
        let div = divisor(b, s.moogle(), kind, r.rank, flags & 2 != 0);
        let mut used: Vec<(u16, i32)> = r.mats.iter().map(|&(id, n)| (id, need(n, div))).collect();
        for k in 0..3 {
            if flags >> k & 1 != 0 {
                used.push((b.t.bonus_item(r.rank, k), 1));
            }
        }
        let m = s.moogle_mut();
        for &(id, n) in &used {
            if let Some(i) = slot(b, id) {
                m.set_stock(i, m.stock_at(i).saturating_sub(n.max(0) as u32));
            }
        }
        m.points = (i64::from(m.points) + i64::from(points)).clamp(0, 99_999) as u32;
        let entry = e + u16::from(!odd && flags & 4 != 0);
        Moogle::with(&mut m.made, u32::from(entry));
        Moogle::with(&mut m.seen, u32::from(entry));
        let before = m.level;
        let learned = grant_levels(b, m);
        Some(Made { item, points, used, entry, level: (before, m.level), learned })
    }

    /// `FUN_00290c28` + `FUN_00290bb8`: the level rows 1..8 whose points are reached grant their unlock bit; the level
    /// is the count of rows reached in order (at most 8).
    pub fn grant_levels(b: &Book, m: &mut Moogle) -> u64 {
        for l in b.mix.levels.iter().skip(1).take(8) {
            if l.points > m.points {
                break;
            }
            if l.unlock >= 0 {
                Moogle::with(&mut m.unlocks, l.unlock as u32);
            }
        }
        let learned = learn_sets(b, m);
        m.level = b.mix.levels.iter().skip(1).take(8).take_while(|l| l.points <= m.points).count() as u8;
        learned
    }

    /// `FUN_00290cc0`: a Free Development recipe (set 1..3) is learned once its set's bit (0..2) is unlocked. Answers
    /// the entries newly learned (the session's NEW marks take them, `Marks::entries`).
    pub fn learn_sets(b: &Book, m: &mut Moogle) -> u64 {
        let mut fresh = 0;
        for (i, r) in b.mix.recipes.iter().enumerate() {
            let e = 2 * i as u32;
            if (1..=3).contains(&r.unlock) && m.unlocked(u32::from(r.unlock) - 1) && !Moogle::bit(m.learned, e) {
                Moogle::with(&mut m.learned, e);
                Moogle::with(&mut fresh, e);
            }
        }
        fresh
    }

    /// `FUN_00291560(item +4, a)`: the first condition byte of a reward. 0xd and 0xe: any category; below 0xd: that
    /// category; 0x10: one of the three special categories (`Tables::special`); any other value: none of them.
    pub fn category_matches(t: &Tables, flag1: u8, a: u8) -> bool {
        if matches!(a, 0xd | 0xe) {
            return true;
        }
        if a < 0xd {
            return flag1 == a;
        }
        if t.special.contains(&flag1) { a == 0x10 } else { a != 0x10 }
    }

    /// `FUN_002915f8(item +5, b)`: the second condition byte. 4 and 5: any rank; below 4: that rank.
    pub fn rank_matches(rank: u8, b: u8) -> bool {
        matches!(b, 4 | 5) || (b < 4 && rank == b)
    }

    /// `FUN_00291418(a, b, c, count)`: walks the materials (`record +0xfc`: the totals ever handed over) that match the
    /// two condition bytes. A matching material never handed over fails the test when `a` is 0xd or `b` is 4 (the
    /// "collect them all" rewards). Else the totals add up (to 99,999) and the number of matching materials with a total
    /// is counted; `c` 0 compares the sum with `count`, any other value the number.
    pub fn cond_met(b: &Book, m: &Moogle, c: &Cond) -> bool {
        let (mut sum, mut kinds) = (0u32, 0u32);
        for (i, &id) in b.mats.iter().take(MOOGLE_MATERIALS).enumerate() {
            if id == 0 || !category_matches(b.t, (b.flag1)(id), c.a) || !rank_matches((b.rank)(id), c.b) {
                continue;
            }
            let total = m.total_at(i);
            if total == 0 {
                if c.a == 0xd || c.b == 4 {
                    return false;
                }
            } else {
                sum = (sum + total).min(99_999);
                kinds += 1;
            }
        }
        u32::from(c.count) <= if c.c == 0 { sum } else { kinds }
    }

    /// What `FUN_00291140` did.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Rewards {
        /// The page 0xb list (state `+0x184`, `+0x60` entries): the reward and whether its condition is met. A reward
        /// whose condition is not met is listed only with the story flag 0x4837 (`FUN_002783f8`).
        pub list: Vec<(u16, bool)>,
        /// The bits it adds to the state's `+0x130`: 2 an item, 4 and 8 the Free Development sets 0 and 1, 0x10 an unlock bit.
        pub notices: u8,
        /// The rewards paid now (the session's `+0x18` marks).
        pub fresh: u64,
        /// (reward, item) paid in items.
        pub items: Vec<(u16, u16)>,
        /// The Free Development entries the new set bits learned.
        pub learned: u64,
        /// The state's `+0x134` marks (`FUN_00291140` via `FUN_00291628`): a bit per material slot, the first material of
        /// the category and rank an unlock reward is for. Its notice names it (`FUN_002879e8`).
        pub materials: u64,
    }

    /// The story flag `FUN_002783f8` tests: with it set the reward list shows the rewards not earned yet too.
    pub const STORY_ALL_REWARDS: u16 = 0x4837;

    /// `FUN_00291628(a, b)`: the place of the first material (table order) whose category is `a` and rank `b`.
    pub fn first_material(b: &Book, a: u8, rank: u8) -> Option<usize> {
        b.mats.iter().take(MOOGLE_MATERIALS).position(|&id| id > 0 && (b.flag1)(id) == a && (b.rank)(id) == rank)
    }

    /// `FUN_00291140`: every reward whose condition is met and whose record bit (`+8`) is clear is paid: the bit is set,
    /// an item reward (kind 0) is added once (`FUN_001a1a18`; the notice 2 comes with it even when the bag is full), a
    /// set reward 0 or 1 sets unlock bit 0 or 1 and learns the sets (`FUN_00290cc0`), any other value sets that unlock
    /// bit and the reward's record bit (`FUN_001a52b0`).
    pub fn pay_rewards(b: &Book, s: &mut dyn Store) -> Rewards {
        let mut r = Rewards::default();
        let all = s.story(STORY_ALL_REWARDS);
        for (k, c) in b.mix.conds.iter().enumerate() {
            let met = cond_met(b, s.moogle(), c);
            if !met && !all {
                continue;
            }
            r.list.push((k as u16, met));
            if !met || Moogle::bit(s.moogle().rewards, k as u32) {
                continue;
            }
            Moogle::with(&mut s.moogle_mut().rewards, k as u32);
            Moogle::with(&mut r.fresh, k as u32);
            if c.kind == 0 {
                r.notices |= 2;
                s.add(c.reward, 1);
                r.items.push((k as u16, c.reward));
                continue;
            }
            match c.reward {
                0 | 1 => {
                    r.notices |= if c.reward == 0 { 4 } else { 8 };
                    Moogle::with(&mut s.moogle_mut().unlocks, u32::from(c.reward));
                    r.learned |= learn_sets(b, s.moogle_mut());
                }
                n => {
                    r.notices |= 0x10;
                    if let Some(i) = first_material(b, c.a, c.b) {
                        Moogle::with(&mut r.materials, i as u32);
                    }
                    Moogle::with(&mut s.moogle_mut().unlocks, u32::from(n));
                    s.set_record_bit(c.bit);
                }
            }
        }
        r
    }

    /// The session's marks (`DAT_0036d1a0`, rebuilt at each opening) and the state's `+0x112d` byte.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Marks {
        /// `+0`: entries (2r, 2r + 1) learned or made and not yet looked at (`FUN_002916d8` at the opening), plus those
        /// learned during the visit (`FUN_002889e0`, `FUN_00290cc0`).
        pub entries: u64,
        /// `+8`: the materials handed over for the first time (`FUN_002887c8`).
        pub known: u64,
        /// `+0x18`: the rewards paid at the opening (`FUN_00291140`).
        pub rewards: u64,
        /// `+0x112d`: set at the opening, cleared when the first page is left: a mark is NEW while it is set, VIEWED after.
        pub fresh: bool,
        /// `+0x10`: the material slots whose displayed total differed from the record's at a count-up step
        /// (`FUN_0028f490`; `FUN_0028f9a8` lights the row plate).
        pub rows: u64,
        /// `+0x20`: the rank tabs (bit 3 - rank) of those slots (`FUN_0028fea0` lights the tab plate).
        pub rank_tabs: u32,
        /// `+0x24`: the category tabs (bit category) of those slots.
        pub cat_tabs: u32,
    }

    /// The (rank, category) of material slot `i`: the item row's `+5` and `+4` (`FUN_001a66e8` of the slot's id).
    pub fn slot_info(b: &Book, i: usize) -> (u8, u8) {
        b.mats.get(i).map_or((0, 0), |&id| ((b.rank)(id), (b.flag1)(id)))
    }

    /// The numbers of the tab grid (`FUN_0028f698`).
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Tabs {
        /// `+0x10`: the sum per rank (item `+5`), at most 99,999.
        pub ranks: [u32; 4],
        /// `+0x20`: the sum per category (item `+4`), at most 9,999.
        pub cats: [u32; 13],
        /// `+0x54`: how many materials have a non-zero value.
        pub kinds: u32,
        /// `+0x58`: the sum of all values, at most 99,999.
        pub total: u32,
    }

    /// `FUN_0028f698(0)` over the displayed totals (`values`, 50 slots), `FUN_0028f698(1)` over the record's.
    pub fn tab_sums(b: &Book, values: &dyn Fn(usize) -> u32) -> Tabs {
        let mut t = Tabs::default();
        for i in 0..MOOGLE_MATERIALS {
            let (rank, category) = slot_info(b, i);
            let v = values(i);
            let (r, c) = (usize::from(rank.min(3)), usize::from(category.min(12)));
            t.ranks[r] = (t.ranks[r] + v).min(99_999);
            t.cats[c] += v;
            if t.cats[c] > 9_999 {
                t.cats[c] = 9_999;
            }
            if v != 0 {
                t.kinds += 1;
            }
            t.total = (t.total + v).min(99_999);
        }
        t
    }

    /// `FUN_002916d8`: the 45 entries (bits 0..0x2c) that are learned or made and not yet seen (record `+0x20`).
    pub fn unseen_entries(m: &Moogle) -> u64 {
        (m.learned | m.made) & !m.seen & ((1u64 << 45) - 1)
    }

    /// `FUN_002919e8`: the shop's third row carries a NEW mark (the top page of the shop): a learned recipe (record
    /// `+0x10`) not seen yet (`+0x20`), else a made entry (`+0x18`) not seen.
    pub fn has_new(m: &Moogle) -> bool {
        unseen_entries(m) != 0
    }

    /// The mark on a row of page 9: `None`, or `Some(true)` NEW (group 0x78) / `Some(false)` VIEWED (group 0x77)
    /// (`FUN_00284ea8`). Rows 3 and 4 (the materials) carry one while the opening handed a material over for the first
    /// time, row 5 while it paid a reward, both NEW until `fresh` falls. Rows 1 and 2 carry one for the sets / recipes
    /// in the session's entries (NEW while one of them is unseen, `FUN_00291880`), row 0 (`FUN_00291960`) a VIEWED one
    /// while a made entry is among them.
    pub fn row_mark(b: &Book, m: &Moogle, k: &Marks, row: u8) -> Option<bool> {
        if (matches!(row, 3 | 4) && k.known != 0) || (row == 5 && k.rewards != 0) {
            return Some(k.fresh);
        }
        if matches!(row, 1 | 2) {
            let want_item = row == 2;
            let mask = (0..b.mix.recipes.len()).filter(|&i| (b.mix.recipes[i].unlock == 0) == want_item).fold(0u64, |a, i| a | 1 << (2 * i));
            let new = k.entries & mask;
            return (new != 0).then_some(new & !m.seen != 0);
        }
        (row == 0 && k.entries & m.made != 0).then_some(false)
    }

    /// The mark on a row of page 10 / 0x17 (`FUN_0028f9a8`): a material handed over for the first time at this opening
    /// (session `+8`) carries NEW (group 0x78) while `+0x112d` is set, VIEWED (group 0x77) after.
    pub fn material_mark(k: &Marks, slot: usize) -> Option<bool> {
        Moogle::bit(k.known, slot as u32).then_some(k.fresh)
    }

    /// The opening of the third row (`FUN_00284c10`).
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Opening {
        /// `FUN_002887c8`: (material, count) handed over.
        pub handed: Vec<(u16, u8)>,
        /// `FUN_002889e0`: the recipe items taken.
        pub recipes: Vec<u16>,
        /// The bits of the state's +0x130 (0 materials, 5 recipes, 1..4 the collection rewards).
        pub notices: u8,
        /// `FUN_00291140`: the reward list and what it paid.
        pub rewards: Rewards,
        /// The materials handed over for the first time (the session's `+8` marks).
        pub known: u64,
        /// The record's totals before the hand-over (`FUN_00284c10` copies them into the display state `+0x68` first).
        pub before: Vec<u32>,
        /// The entries learned by the recipe items and the sets the rewards opened.
        pub learned: u64,
        /// The first page: 10 after a hand-over, else 0x10 after recipes, else 9.
        pub page: Page,
    }

    /// `FUN_00284c10`: the story flag of `exp` grants its bit (10, the Recipes row); every material the player holds goes
    /// to the stock and the totals (at most 999 each; the first one sets its known bit); with bit 10, the recipe items
    /// held are handed over (`FUN_002888f8` / `FUN_002889e0`: a kind-0 recipe not learned yet is learned and its item
    /// taken; an item of the other kinds is only taken). The collection rewards (`pay_rewards`, `FUN_00291140`) come
    /// between the two, after the totals changed.
    pub fn open(b: &Book, s: &mut dyn Store) -> Opening {
        let mut o = Opening { page: Page::Top, before: (0..MOOGLE_MATERIALS).map(|i| s.moogle().total_at(i)).collect(), ..Opening::default() };
        if s.story(b.mix.story_flag) {
            Moogle::with(&mut s.moogle_mut().unlocks, u32::from(b.mix.story_bit));
        }
        for (i, &id) in b.mats.iter().take(MOOGLE_MATERIALS).enumerate() {
            let n = s.count(id);
            if n == 0 {
                continue;
            }
            let m = s.moogle_mut();
            m.set_stock(i, (m.stock_at(i) + u32::from(n)).min(999));
            m.set_total(i, (m.total_at(i) + u32::from(n)).min(999));
            if !Moogle::bit(m.known, i as u32) {
                Moogle::with(&mut o.known, i as u32);
            }
            Moogle::with(&mut m.known, i as u32);
            s.remove(id, n);
            o.handed.push((id, n));
        }
        if !o.handed.is_empty() {
            o.notices |= 1;
            o.page = Page::Materials { totals: true };
        }
        o.rewards = pay_rewards(b, s);
        o.notices |= o.rewards.notices;
        o.learned |= o.rewards.learned;
        o.learned |= learn_sets(b, s.moogle_mut());
        if s.moogle().unlocked(10) {
            let fresh = b.mix.recipes.iter().enumerate().any(|(i, r)| r.unlock == 0 && s.count(r.id) > 0 && !Moogle::bit(s.moogle().learned, 2 * i as u32));
            if fresh {
                for (i, r) in b.mix.recipes.iter().enumerate() {
                    if s.count(r.id) == 0 {
                        continue;
                    }
                    if r.unlock == 0 {
                        if Moogle::bit(s.moogle().learned, 2 * i as u32) {
                            continue;
                        }
                        Moogle::with(&mut s.moogle_mut().learned, 2 * i as u32);
                        Moogle::with(&mut o.learned, 2 * i as u32);
                    }
                    s.remove(r.id, s.count(r.id));
                    o.recipes.push(r.id);
                }
                if o.page == Page::Top {
                    o.page = Page::List(2);
                }
                o.notices |= 0x20;
            }
        }
        o
    }

    /// `FUN_00284ea8`: which of the seven rows of page 9 show: 0 Creations when something was made, 1 Free Development
    /// with a set bit (0..2), 2 Recipes with bit 10; the others always.
    pub fn top_rows(b: &Book, m: &Moogle) -> Vec<u8> {
        (0..7u8)
            .filter(|&k| match k {
                0 => !entries(b, m, 0).is_empty(),
                1 => m.unlocked(0) || m.unlocked(1) || m.unlocked(2),
                2 => m.unlocked(10),
                _ => true,
            })
            .collect()
    }

    /// The synthesis pages (the original's numbers in `number`).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum Page {
        /// Page 9: the seven rows.
        #[default]
        Top,
        /// Pages 0xc, 0xe, 0x10: Creations, Free Development, Recipes.
        List(u8),
        /// Pages 0xd, 0xf, 0x11: a recipe.
        Detail(u8),
        /// Page 0x12: a text, then back.
        Notice,
        /// Page 0x14: the result window.
        Result,
        /// Page 0x16: the level-up notices.
        LevelUp,
        /// Page 10 (`totals`: every material handed over) and page 0x17 (the stock).
        Materials { totals: bool },
        /// Page 0xb: the collection rewards.
        Rewards,
        /// Page 0x13: the opening's notices.
        Opening,
    }

    impl Page {
        pub fn number(self) -> u8 {
            match self {
                Page::Top => 9,
                Page::List(k) => 0xc + 2 * k,
                Page::Detail(k) => 0xd + 2 * k,
                Page::Notice => 0x12,
                Page::Result => 0x14,
                Page::LevelUp => 0x16,
                Page::Materials { totals } => if totals { 10 } else { 0x17 },
                Page::Rewards => 0xb,
                Page::Opening => 0x13,
            }
        }
    }

    /// The state of the third row.
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Session {
        pub page: Page,
        /// The page the notice / list returns to (the state's +0x64).
        pub ret: Page,
        pub rows: Vec<u8>,
        pub top: usize,
        pub kind: u8,
        pub entries: Vec<u16>,
        pub list: usize,
        /// +0x106b, +0x106a and the detail cursor.
        pub detail: u8,
        pub flags: u8,
        pub row: usize,
        /// The material slots the material page lists and its cursor.
        pub slots: Vec<usize>,
        pub mat: usize,
        /// The text a notice shows, the queued opening texts, the result item, the level-up names.
        pub text: u32,
        pub queue: Vec<u32>,
        /// The item each queued text names (`FUN_002879e8`: the reward item of notice 1, the material of notice 4), and
        /// the one the shown text names.
        pub queue_item: Vec<Option<u16>>,
        pub text_item: Option<u16>,
        /// Story flag 0x4837 (`FUN_002783f8`), read when the third row opened: the lists show what is not there yet.
        pub all: bool,
        pub item: u16,
        pub levels: Vec<u16>,
        pub last: Option<Made>,
        /// Page 0xb: the listed rewards (`Rewards::list`) and the cursor.
        pub rewards: Vec<(u16, bool)>,
        pub rew: usize,
        /// The NEW marks of this visit.
        pub marks: Marks,
        /// The totals page 10 displays (state `+0x68`): they start at the record's values from before the hand-over and
        /// count up to the record's (`count_up`).
        pub shown: Vec<u32>,
    }

    /// What a key did.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Act {
        None,
        Moved,
        Refused,
        Page,
        Toggled,
        /// A make happened (`Session::last`).
        Made,
        /// Back to the shop's top rows (row 6 Quit or a cancel on page 9: page 0, `FUN_0025fef0`).
        Back,
        /// -4 (Start) on any page: the menu ends (`FUN_00284b80`).
        Close,
    }

    impl Session {
        /// The third row is confirmed: the opening, then its notices and its first page.
        pub fn start(b: &Book, s: &mut dyn Store) -> (Session, Opening) {
            let o = open(b, s);
            let marks = Marks { entries: unseen_entries(s.moogle()), known: o.known, rewards: o.rewards.fresh, fresh: true, ..Marks::default() };
            let mut x = Session { rows: top_rows(b, s.moogle()), rewards: o.rewards.list.clone(), marks, all: s.story(STORY_ALL_REWARDS), shown: o.before.clone(), ..Session::default() };
            // `FUN_002879e8`: notice 1 comes once for each reward item (state `+0x13c`, by reward), notice 4 once for each
            // material marked (`+0x134`, by place); the others once.
            for k in (0..6).filter(|k| o.notices >> k & 1 != 0) {
                let text = u32::from(b.t.notices[k]);
                let items: Vec<Option<u16>> = match k {
                    1 => o.rewards.items.iter().map(|&(_, item)| Some(item)).collect(),
                    4 => (0..MOOGLE_MATERIALS as u32).filter(|&i| Moogle::bit(o.rewards.materials, i)).map(|i| b.mats.get(i as usize).copied()).collect(),
                    _ => Vec::new(),
                };
                if items.is_empty() {
                    x.queue.push(text);
                    x.queue_item.push(None);
                }
                for item in items {
                    x.queue.push(text);
                    x.queue_item.push(item);
                }
            }
            x.goto(b, s.moogle(), o.page);
            x.see(s.moogle_mut());
            if !x.queue.is_empty() {
                x.ret = x.page;
                x.text = x.queue.remove(0);
                x.text_item = x.queue_item.remove(0);
                x.page = Page::Opening;
            }
            (x, o)
        }

        /// One step of `FUN_0028f490`, run every frame by page 10's task while `+0x112d` (`marks.fresh`) is set: every
        /// displayed total below the record's goes up by one, or all of them jump to the record's at once when `skip`
        /// (any pad button held). A slot whose displayed total differs from the record's first sets its marks (row
        /// `+0x10`, rank tab `+0x20` bit 3 - rank, category tab `+0x24`). Returns whether anything was different.
        pub fn count_up(&mut self, b: &Book, m: &Moogle, skip: bool) -> bool {
            if !self.marks.fresh {
                return false;
            }
            self.shown.resize(MOOGLE_MATERIALS, 0);
            let mut changed = false;
            for i in 0..MOOGLE_MATERIALS {
                let record = m.total_at(i);
                if self.shown[i] == record {
                    continue;
                }
                changed = true;
                let (rank, category) = slot_info(b, i);
                self.marks.rows |= 1 << i;
                self.marks.rank_tabs |= 1 << (3 - u32::from(rank.min(3)));
                self.marks.cat_tabs |= 1 << u32::from(category.min(12));
                if skip {
                    self.shown[i] = record;
                } else if self.shown[i] < record {
                    self.shown[i] += 1;
                }
            }
            changed
        }

        pub fn goto(&mut self, b: &Book, m: &Moogle, p: Page) {
            match p {
                Page::Top => self.rows = top_rows(b, m),
                Page::List(k) => {
                    self.kind = k;
                    self.entries = entries_all(b, m, k, self.all);
                    self.list = self.list.min(self.entries.len().saturating_sub(1));
                }
                Page::Materials { totals } => {
                    // Page 10 also lists, with story flag 0x4837, the materials never handed over (`FUN_0028f9a8`, `FUN_0028ffc8`).
                    let all = self.all;
                    self.slots = (0..b.mats.len().min(MOOGLE_MATERIALS)).filter(|&i| if totals { all || m.total_at(i) != 0 } else { m.stock_at(i) != 0 }).collect();
                    self.mat = 0;
                }
                _ => {}
            }
            self.page = p;
        }

        /// `FUN_00288fc0`: on a list page the entry under the cursor is marked seen (record `+0x20`) every frame.
        fn see(&mut self, m: &mut Moogle) {
            if let (Page::List(_), Some(e)) = (self.page, self.entry()) {
                if e & HIDDEN != 0 {
                    return;
                }
                Moogle::with(&mut m.seen, u32::from(e));
            }
        }

        /// The mark of list entry `e` (`FUN_00288fc0`): an entry among the session's marks is NEW while its seen bit is clear.
        pub fn entry_mark(&self, m: &Moogle, e: u16) -> Option<bool> {
            Moogle::bit(self.marks.entries, u32::from(e)).then(|| !Moogle::bit(m.seen, u32::from(e)))
        }

        fn notice(&mut self, text: u32) {
            self.ret = self.page;
            self.text = text;
            self.text_item = None;
            self.page = Page::Notice;
        }

        /// The entry under the list cursor.
        pub fn entry(&self) -> Option<u16> {
            self.entries.get(self.list).copied()
        }

        pub fn key(&mut self, k: super::Key, b: &Book, s: &mut dyn Store) -> Act {
            let before = self.page;
            let act = self.key_inner(k, b, s);
            // `+0x112d` falls when page 9, page 10 or the Recipes list is left (`FUN_00285388`, `FUN_00285b30`, `FUN_00286580`).
            if act == Act::Page && self.page != before && matches!(before, Page::Top | Page::Materials { totals: true } | Page::List(2)) {
                self.marks.fresh = false;
            }
            self.see(s.moogle_mut());
            act
        }

        fn key_inner(&mut self, k: super::Key, b: &Book, s: &mut dyn Store) -> Act {
            use super::Key;
            if k == Key::Start {
                return Act::Close;
            }
            let step = |c: &mut usize, n: usize, k: Key| {
                let d = if matches!(k, Key::Up | Key::Left) { -1 } else { 1 };
                let x = crate::menu::step_cursor(*c, n, d, true);
                if x == *c {
                    Act::None
                } else {
                    *c = x;
                    Act::Moved
                }
            };
            let vertical = matches!(k, Key::Up | Key::Down);
            match self.page {
                Page::Top => match k {
                    _ if vertical => step(&mut self.top, self.rows.len(), k),
                    Key::Cancel => Act::Back,
                    Key::Confirm => match self.rows.get(self.top) {
                        Some(&r @ 0..=2) => {
                            self.list = 0;
                            self.goto(b, s.moogle(), Page::List(r));
                            Act::Page
                        }
                        Some(3) => {
                            self.goto(b, s.moogle(), Page::Materials { totals: false });
                            Act::Page
                        }
                        Some(4) => {
                            self.goto(b, s.moogle(), Page::Materials { totals: true });
                            Act::Page
                        }
                        Some(5) => {
                            self.rew = 0;
                            self.goto(b, s.moogle(), Page::Rewards);
                            Act::Page
                        }
                        Some(_) => Act::Back,
                        None => Act::None,
                    },
                    _ => Act::None,
                },
                Page::List(kind) => match k {
                    _ if vertical => step(&mut self.list, self.entries.len(), k),
                    Key::Cancel => {
                        self.goto(b, s.moogle(), Page::Top);
                        Act::Page
                    }
                    Key::Confirm => {
                        let Some(e) = self.entry() else { return Act::Refused };
                        if !entry_open(b, s.moogle(), kind, e) {
                            return Act::Refused;
                        }
                        let (rows, cur) = detail_rows(b, s.moogle(), kind, e);
                        self.detail = rows;
                        self.row = cur;
                        self.flags = 0;
                        self.page = Page::Detail(kind);
                        Act::Page
                    }
                    _ => Act::None,
                },
                Page::Detail(kind) => match k {
                    _ if vertical => step(&mut self.row, self.detail.count_ones() as usize, k),
                    Key::Cancel => {
                        self.goto(b, s.moogle(), Page::List(kind));
                        Act::Page
                    }
                    Key::Confirm => {
                        let Some(e) = self.entry() else { return Act::Refused };
                        let Some((r, _)) = recipe(b, e) else { return Act::Refused };
                        match row_kind(self.detail, self.row) {
                            Some(0) => match make_check(b, s, kind, e, self.flags) {
                                Make::Notice(t) => {
                                    self.notice(t);
                                    Act::Refused
                                }
                                Make::Ok(item) => {
                                    self.ret = self.page;
                                    self.item = item;
                                    self.page = Page::Result;
                                    Act::Page
                                }
                            },
                            Some(q @ 1..=3) => {
                                // FUN_00291b58: the flag tested is the shown row's (row - 1).
                                let m = s.moogle();
                                let on = self.flags >> (self.row.saturating_sub(1) & 7) & 1 != 0;
                                let allowed = if m.unlocked(5) { 2 } else { 1 };
                                let ok = on || ((self.flags & 7).count_ones() < allowed && bonus_ok(b, m, kind, true, usize::from(q - 1), r.rank, true));
                                if !ok {
                                    return Act::Refused;
                                }
                                self.flags ^= 1 << (q - 1);
                                Act::Toggled
                            }
                            Some(_) => {
                                self.goto(b, s.moogle(), Page::List(kind));
                                Act::Page
                            }
                            None => Act::None,
                        }
                    }
                    _ => Act::None,
                },
                Page::Notice => match k {
                    Key::Confirm | Key::Cancel => {
                        self.page = self.ret;
                        Act::Page
                    }
                    _ => Act::None,
                },
                Page::Opening => match k {
                    Key::Confirm | Key::Cancel => {
                        if self.queue.is_empty() {
                            self.page = self.ret;
                        } else {
                            self.text = self.queue.remove(0);
                            self.text_item = self.queue_item.remove(0);
                        }
                        Act::Page
                    }
                    _ => Act::None,
                },
                Page::Result => match k {
                    Key::Confirm | Key::Cancel => {
                        let (Some(e), Page::Detail(kind)) = (self.entry(), self.ret) else { return Act::None };
                        self.last = make(b, s, kind, e, self.flags, self.item);
                        self.marks.entries |= self.last.as_ref().map_or(0, |m| m.learned);
                        let (from, to) = self.last.as_ref().map_or((0, 0), |m| m.level);
                        self.levels = (from + 1..=to).map(|l| b.t.level_names.get(usize::from(l)).copied().unwrap_or(0)).collect();
                        self.goto(b, s.moogle(), Page::List(kind));
                        if !self.levels.is_empty() {
                            self.ret = self.page;
                            self.text = u32::from(self.levels.remove(0));
                            self.page = Page::LevelUp;
                        }
                        Act::Made
                    }
                    _ => Act::None,
                },
                Page::LevelUp => match k {
                    Key::Confirm | Key::Cancel => {
                        if self.levels.is_empty() {
                            self.page = self.ret;
                        } else {
                            self.text = u32::from(self.levels.remove(0));
                        }
                        Act::Page
                    }
                    _ => Act::None,
                },
                Page::Materials { .. } => match k {
                    _ if vertical => step(&mut self.mat, self.slots.len(), k),
                    Key::Cancel => {
                        self.goto(b, s.moogle(), Page::Top);
                        Act::Page
                    }
                    // -5 (`FUN_00285b30`): to the reward list, the page 9 cursor on its row.
                    Key::Switch if matches!(self.page, Page::Materials { totals: true }) => {
                        self.rew = 0;
                        self.goto(b, s.moogle(), Page::Rewards);
                        self.top = self.rows.iter().position(|&r| r == 5).unwrap_or(self.top);
                        Act::Page
                    }
                    _ => Act::None,
                },
                // Page 0xb (`FUN_00286060`): a read-only list. Confirm is refused (`FUN_00291b58` answers 0 on this page),
                // cancel goes to page 9, -5 to page 10 (the page 9 cursor on its row).
                Page::Rewards => match k {
                    _ if vertical => step(&mut self.rew, self.rewards.len(), k),
                    Key::Confirm => Act::Refused,
                    Key::Cancel => {
                        self.goto(b, s.moogle(), Page::Top);
                        Act::Page
                    }
                    Key::Switch => {
                        self.goto(b, s.moogle(), Page::Materials { totals: true });
                        self.top = self.rows.iter().position(|&r| r == 4).unwrap_or(self.top);
                        Act::Page
                    }
                    _ => Act::None,
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ware(id: u16, kind: u8, buy: u16, sell: u16) -> Ware {
        Ware { id, kind, name: 0, buy, sell, rank: 0, flag1: 0 }
    }

    fn shops() -> Shops {
        let g = |bit: i16, items: &[u16]| Group { bit, items: items.to_vec() };
        let r = |event, bit, flags, groups| ShopRecord { event, bit, message: 0x8ab0, words: [0; 4], flags, groups };
        Shops {
            records: vec![
                // A weapon shop (flags 0x181: kind bit 1, shared).
                r(0x67, 0x29, 0x181, vec![g(-1, &[0x94])]),
                // An item shop: consumables (kind bit 8), shared.
                r(0x6a, 0x2c, 0x188, vec![g(-1, &[1, 0x83])]),
                // A moogle: one always-on group and one behind record bit 0x4c.
                r(0x70, 0x32, 0x31f, vec![g(-1, &[2]), g(0x4c, &[0x13d])]),
            ],
            seen_ids: Vec::new(),
        }
    }

    fn kind(id: u16) -> Option<u8> {
        Some(match id {
            0x94 => 3,
            1 | 2 | 0x83 => 0,
            0x13d => 0x10,
            _ => return None,
        })
    }

    #[test]
    fn the_stock_follows_the_group_bits_and_the_shared_flag() {
        let s = shops();
        // First open: the record bit is clear, so the open sets it; the moogle's gated group stays out.
        let st = stock(&s, 0x70, &|_| false, &kind).unwrap();
        assert_eq!(st.items, vec![2]);
        assert_eq!(st.set_bit, Some(0x32));
        assert_eq!(st.rows_bits, 3);
        assert_eq!(top_rows(&st), vec![0, 1, 2, 3]);
        let st = stock(&s, 0x70, &|b| b == 0x4c || b == 0x32, &kind).unwrap();
        assert_eq!(st.items, vec![2, 0x13d]);
        assert_eq!(st.set_bit, None);
        // Flag 0x80: the item shop also lists the always-on wares of every record whose bit is set and whose own flag
        // 0x80 is clear (the moogle's), through its own flags as the kind mask (0x188: kind bit 8).
        let st = stock(&s, 0x6a, &|b| b == 0x32, &kind).unwrap();
        assert_eq!(st.items, vec![1, 0x83, 2]);
        let st = stock(&s, 0x6a, &|_| false, &kind).unwrap();
        assert_eq!(st.items, vec![1, 0x83], "not before the moogle's bit is set");
        // The weapon shop's mask (0x181, kind bit 1) keeps the moogle's consumables out.
        let st = stock(&s, 0x67, &|b| b == 0x32, &kind).unwrap();
        assert_eq!(st.items, vec![0x94]);
        assert!(!st.first_group, "flag 0x80 clears the first-group byte");
        assert_eq!(top_rows(&st), vec![0, 1, 3], "no third row without flag 0x200");
        assert!(stock(&s, 0x99, &|_| true, &kind).is_none());
        assert!(opens_shop(0x13) && opens_shop(0x66) && opens_shop(0x75) && opens_shop(0x8a) && !opens_shop(0x76) && !opens_shop(0x22));
    }

    #[test]
    fn the_quantity_stops_at_99_the_munny_and_one() {
        // Buying at 100 with 450 munny: +1 to 2, +10 is cut to what the munny covers (4).
        assert_eq!(step_quantity(false, 1, 0, 100, 450, 1), Some(2));
        assert_eq!(step_quantity(false, 1, 0, 100, 450, 10), Some(4));
        assert_eq!(step_quantity(false, 4, 0, 100, 450, 1), None, "the munny covers no fifth");
        // The count never goes below 1, and a minus at 1 is refused.
        assert_eq!(step_quantity(false, 1, 0, 100, 450, -1), None);
        assert_eq!(step_quantity(false, 3, 0, 100, 450, -10), Some(1));
        // With 95 owned the count stops at 4 (99 in all).
        assert_eq!(step_quantity(false, 1, 95, 1, 10_000, 10), Some(4));
        assert_eq!(step_quantity(false, 4, 95, 1, 10_000, 1), None);
        // Selling stops at the owned count.
        assert_eq!(step_quantity(true, 1, 3, 25, 0, 10), Some(3));
        assert_eq!(step_quantity(true, 3, 3, 25, 0, 1), None);
    }

    #[test]
    fn the_page_machine_buys_and_sells() {
        let buy = [ware(1, 0, 40, 10), ware(0x94, 3, 600, 300)];
        let sell = [(ware(1, 0, 40, 10), 2u8)];
        let owned = |id: u16| if id == 1 { 2 } else { 0 };
        let v = View { buy: &buy, sell: &sell, owned: &owned, munny: 100 };
        let mut f = Flow::new(vec![0, 1, 3]);
        assert_eq!(f.key(Key::Confirm, &v), Act::Page);
        assert_eq!(f.page, Page::List { sell: false });
        // The sword costs more than the munny: refused (FUN_00265550).
        assert_eq!(f.key(Key::Down, &v), Act::Moved);
        assert_eq!(f.key(Key::Confirm, &v), Act::Refused);
        assert_eq!(f.key(Key::Up, &v), Act::Moved);
        assert_eq!(f.key(Key::Confirm, &v), Act::Page);
        assert_eq!(f.page, Page::Quantity { sell: false });
        assert_eq!(f.deal, Deal { item: 1, qty: 1, price: 40 });
        assert_eq!(f.key(Key::Up, &v), Act::Moved);
        assert_eq!(f.key(Key::Up, &v), Act::Refused, "100 munny covers two at 40");
        assert_eq!(f.key(Key::Confirm, &v), Act::Page);
        assert_eq!(f.page, Page::Question { sell: false });
        // Row 0 is "No": back to the quantity.
        assert_eq!(f.key(Key::Confirm, &v), Act::Page);
        assert_eq!(f.page, Page::Quantity { sell: false });
        assert_eq!(f.key(Key::Confirm, &v), Act::Page);
        assert_eq!(f.key(Key::Down, &v), Act::Moved);
        assert_eq!(f.key(Key::Confirm, &v), Act::Deal);
        assert_eq!(f.page, Page::List { sell: false });
        assert_eq!(f.deal.total(), 80);
        // Cancel back to the top, then Sell.
        assert_eq!(f.key(Key::Cancel, &v), Act::Page);
        assert_eq!(f.key(Key::Down, &v), Act::Moved);
        assert_eq!(f.key(Key::Confirm, &v), Act::Page);
        assert_eq!(f.page, Page::List { sell: true });
        assert_eq!(f.key(Key::Confirm, &v), Act::Page);
        assert_eq!(f.deal, Deal { item: 1, qty: 1, price: 10 });
        // Quit, a cancel on the top and Start close.
        let mut f = Flow::new(vec![0, 1, 3]);
        f.top = 2;
        assert_eq!(f.key(Key::Confirm, &v), Act::Close);
        assert_eq!(Flow::new(vec![3]).key(Key::Cancel, &v), Act::Close);
        assert_eq!(Flow::new(vec![3]).key(Key::Start, &v), Act::Close);
        // The third row is refused (not ported).
        assert_eq!(Flow::new(vec![2, 3]).key(Key::Confirm, &v), Act::ThirdRow);
    }

    #[test]
    fn the_sell_list_needs_a_count_a_price_and_a_type_below_0x12() {
        let w = [ware(1, 0, 40, 10), ware(2, 0, 100, 0), ware(3, 0x12, 1, 1), ware(4, 0xf, 480, 240)];
        let have = |id: u16| if id == 4 { 0 } else { 5 };
        assert_eq!(sell_list(&w, &have), vec![(1, 5)]);
        assert!(row_open(false, &w[0], 0x62, 40) && !row_open(false, &w[0], 0x63, 40) && !row_open(false, &w[0], 0, 39));
        assert!(row_open(true, &w[0], 1, 0) && !row_open(true, &w[1], 1, 0));
    }

    // --- the third row (synthesis) -------------------------------------------------------------------

    use synth::{Book, Make, Mix, Recipe, Session, Store, Tables};

    #[derive(Default)]
    struct Fake {
        m: Moogle,
        items: std::collections::HashMap<u16, u8>,
        flags: Vec<u16>,
        records: Vec<u16>,
    }

    impl Store for Fake {
        fn moogle(&self) -> &Moogle {
            &self.m
        }
        fn moogle_mut(&mut self) -> &mut Moogle {
            &mut self.m
        }
        fn count(&self, id: u16) -> u8 {
            self.items.get(&id).copied().unwrap_or(0)
        }
        fn add(&mut self, id: u16, n: u8) -> u8 {
            let c = self.items.entry(id).or_default();
            let taken = n.min(99 - *c);
            *c += taken;
            taken
        }
        fn remove(&mut self, id: u16, n: u8) {
            let c = self.items.entry(id).or_default();
            *c = c.saturating_sub(n);
        }
        fn story(&self, flag: u16) -> bool {
            self.flags.contains(&flag)
        }
        fn set_record_bit(&mut self, n: u16) {
            self.records.push(n);
        }
    }

    /// Materials 0x200 (rank 0), 0x201 (rank 1); bonus materials 0x210 (k 0), 0x211 (k 1), 0x212 (k 2) of rank 0.
    fn mix() -> (Mix, Tables, Vec<u16>) {
        let level = |unlock, points| synth::Level { name: 0, text: 0, unlock, points };
        let mix = Mix {
            recipes: vec![
                // A recipe item (kind 0): 3 of 0x200 and 1 of 0x201 make item 0x50, the higher rank 0x51.
                Recipe { id: 0x1c1, unlock: 0, rank: 0, result: 0x50, upgraded: 0x51, mats: vec![(0x200, 3), (0x201, 1)] },
                // Free Development set 1 (bit 0): 5 of 0x200 make 0x60.
                Recipe { id: 0x1c5, unlock: 1, rank: 0, result: 0x60, upgraded: 0, mats: vec![(0x200, 5)] },
            ],
            levels: vec![level(-1, 0), level(3, 5), level(4, 1000)],
            exp: [2, 3, 4, 5],
            factor: 2.0,
            story_flag: 0x814,
            story_bit: 10,
            conds: Vec::new(),
        };
        let t = Tables { rows: [0xbc8c, 0xbc8d, 0xbc8e, 0xc48e, 0xbc8b, 0xbcdd, 0xbc8f], bonus: [[0x210; 4], [0x211; 4], [0x212; 4]], rank_bits: [6, 7, 8, 9], help: [0; 5], notices: [0xbc89, 0, 0, 0, 0, 0xbc8a], level_names: [0, 1, 2, 3, 4, 5, 6, 7, 8], layouts: [8, 10, 9], special: [9, 10, 11], ..Tables::default() };
        (mix, t, vec![0x200, 0x201, 0x210, 0x211, 0x212])
    }

    fn rank(id: u16) -> u8 {
        u8::from(id == 0x201)
    }

    fn flag1(_: u16) -> u8 {
        0
    }

    #[test]
    fn the_synthesis_opening_hands_over_materials_and_recipes() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rank, flag1: &flag1 };
        let mut s = Fake::default();
        s.items.insert(0x200, 4);
        s.items.insert(0x1c1, 1);
        // Without the story flag (bit 10) the recipe item stays; the materials go to the stock and the totals.
        let (x, o) = Session::start(&b, &mut s);
        assert_eq!(o.handed, vec![(0x200, 4)]);
        assert!(o.recipes.is_empty() && s.count(0x1c1) == 1 && s.count(0x200) == 0);
        assert_eq!((s.m.stock_at(0), s.m.total_at(0), s.m.known), (4, 4, 1));
        assert_eq!(x.page, synth::Page::Opening, "the hand-over notice first");
        assert_eq!((x.text, x.ret), (0xbc89, synth::Page::Materials { totals: true }), "then page 10");
        assert_eq!(x.rows, vec![3, 4, 5, 6], "no Creations, no Free Development, no Recipes yet");
        // The story flag grants bit 10: the recipe is learned and its item taken; page 0x10 after the notices.
        s.flags.push(0x814);
        s.items.insert(0x200, 1);
        let (mut x, o) = Session::start(&b, &mut s);
        assert_eq!((o.recipes.clone(), o.notices), (vec![0x1c1], 0x21));
        assert!(Moogle::bit(s.m.learned, 0) && s.count(0x1c1) == 0 && s.m.stock_at(0) == 5);
        assert_eq!(x.queue, vec![0xbc8a]);
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page);
        assert_eq!(x.text, 0xbc8a);
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page);
        assert_eq!(x.page, synth::Page::Materials { totals: true });
        assert_eq!(x.slots, vec![0]);
        assert_eq!(x.key(Key::Cancel, &b, &mut s), synth::Act::Page);
        assert_eq!((x.page, x.rows.clone()), (synth::Page::Top, vec![2, 3, 4, 5, 6]));
        // Stock 999 at most; a recipe item already learned is not taken again.
        s.m.set_stock(0, 998);
        s.items.insert(0x200, 5);
        s.items.insert(0x1c1, 1);
        let (_, o) = Session::start(&b, &mut s);
        assert_eq!((s.m.stock_at(0), o.recipes.len(), s.count(0x1c1)), (999, 0, 1));
        // The Free Development set is learned once its bit is unlocked; Quit and a cancel on page 9 go back.
        s.m.unlocks |= 1;
        let (mut x, _) = Session::start(&b, &mut s);
        assert!(Moogle::bit(s.m.learned, 2));
        assert_eq!(x.rows, vec![1, 2, 3, 4, 5, 6]);
        x.page = synth::Page::Top;
        assert_eq!(x.key(Key::Cancel, &b, &mut s), synth::Act::Back);
        x.top = 5;
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Back);
        x.top = 4;
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page, "row 5 (page 0xb)");
        assert_eq!(x.page, synth::Page::Rewards);
        assert_eq!(x.key(Key::Start, &b, &mut s), synth::Act::Close);
    }

    #[test]
    fn a_make_takes_the_lines_adds_the_item_the_points_and_the_level() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rank, flag1: &flag1 };
        let mut s = Fake::default();
        s.m.unlocks = 1 << 10;
        s.m.learned = 1;
        s.m.set_stock(0, 2);
        let mut x = Session { page: synth::Page::Top, rows: synth::top_rows(&b, &s.m), ..Session::default() };
        x.top = x.rows.iter().position(|&r| r == 2).unwrap();
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page);
        assert_eq!((x.page, x.entries.clone()), (synth::Page::List(2), vec![0]));
        // 2 of 3 and none of 0x201: the entry cannot be opened.
        assert!(!synth::entry_open(&b, &s.m, 2, 0));
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Refused);
        s.m.set_stock(0, 7);
        s.m.set_stock(1, 1);
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page);
        assert_eq!((x.page, x.detail, x.row), (synth::Page::Detail(2), 0x11, 0), "Synthesize and Quit, the cursor on Synthesize");
        // The item at 99: refused with the stock-full text.
        s.items.insert(0x50, 99);
        assert_eq!(synth::make_check(&b, &s, 2, 0, 0), Make::Notice(synth::msg::FULL));
        s.items.insert(0x50, 1);
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page);
        assert_eq!((x.page, x.item), (synth::Page::Result, 0x50));
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Made);
        let d = x.last.clone().unwrap();
        // 3 x rank 0 (2 points) + 1 x rank 1 (3 points) = 9 points: level row 1 (5 points) grants bit 3, level 1.
        assert_eq!((d.item, d.points, d.used.clone(), d.level), (0x50, 9, vec![(0x200, 3), (0x201, 1)], (0, 1)));
        assert_eq!((s.count(0x50), s.m.stock_at(0), s.m.stock_at(1), s.m.points), (2, 4, 0, 9));
        assert!(Moogle::bit(s.m.made, 0) && Moogle::bit(s.m.seen, 0) && s.m.unlocked(3) && s.m.level == 1);
        assert_eq!(x.page, synth::Page::LevelUp, "one level-up notice");
        assert_eq!(x.text, 1, "the name of level 1");
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page);
        assert_eq!((x.page, x.entries.clone()), (synth::Page::List(2), vec![]), "made: off the Recipes list");
        assert_eq!(synth::top_rows(&b, &s.m), vec![0, 2, 3, 4, 5, 6], "the Creations row shows");
        assert_eq!(synth::entries(&b, &s.m, 0), vec![0]);
    }

    /// Review finding 2: the paid-reward bits go through a slot file, so a reload does not pay the rewards again.
    #[test]
    fn rewards_are_not_paid_again_after_a_reload() {
        let (mut mix, t, mats) = mix();
        mix.conds = vec![cond(0, 0x30, 0xe, 5, 0, 1), cond(1, 7, 0xe, 5, 0, 1)];
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        let mut s = Fake::default();
        s.items.insert(0x200, 2);
        let (_, o) = Session::start(&b, &mut s);
        assert_eq!((o.rewards.fresh > 0, s.m.rewards, s.count(0x30)), (true, 0b11, 1), "paid once");
        // Save, then load into a new state with an empty bag.
        let mut p = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        p.moogle = s.m.clone();
        let text = p.slot_json(&crate::progress::SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        q.apply_json(&text).expect("a slot file loads");
        assert_eq!(q.moogle, s.m, "the record came back whole");
        let mut s2 = Fake { m: q.moogle.clone(), ..Fake::default() };
        let (_, o) = Session::start(&b, &mut s2);
        assert_eq!((o.rewards.fresh, s2.count(0x30), s2.records.len()), (0, 0, 0), "nothing is paid again");
    }

    /// A review finding: `make` on a full bag (a caller that skipped `make_check`) takes nothing and adds nothing.
    #[test]
    fn a_make_into_a_full_bag_changes_nothing() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rank, flag1: &flag1 };
        let mut s = Fake::default();
        s.m.unlocks = 1 << 10;
        s.m.learned = 1;
        s.m.set_stock(0, 7);
        s.m.set_stock(1, 1);
        s.items.insert(0x50, 99);
        let before = s.m.clone();
        assert_eq!(synth::make(&b, &mut s, 2, 0, 0, 0x50), None);
        assert_eq!((s.count(0x50), s.m.clone()), (99, before));
        s.items.insert(0x50, 98);
        assert!(synth::make(&b, &mut s, 2, 0, 0, 0x50).is_some());
        assert_eq!(s.count(0x50), 99);
    }

    #[test]
    fn the_bonus_rows_halve_and_raise_the_rank() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rank, flag1: &flag1 };
        let mut s = Fake::default();
        s.m.unlocks = (1 << 3) | (1 << 4);
        s.m.made = 1;
        for (i, n) in [2u32, 1, 1, 1, 1].into_iter().enumerate() {
            s.m.set_stock(i, n);
        }
        // Creations (list 0): with bit 4 the higher rank (k 2) shows besides the points (k 0) and Halve (k 1).
        let (rows, cur) = synth::detail_rows(&b, &s.m, 0, 0);
        assert_eq!((rows, cur), (0x1f, 2), "2 of 3 in stock: the cursor leaves Synthesize");
        assert_eq!(synth::make_check(&b, &s, 0, 0, 0), Make::Notice(synth::msg::SHORT));
        // Halve: 3 -> 2, 1 -> 1, one 0x211 taken.
        assert!(synth::can_make(&b, &s.m, 0, &mix.recipes[0], false, true));
        let mut x = Session { page: synth::Page::Detail(0), entries: vec![0], detail: rows, row: 2, kind: 0, ..Session::default() };
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Toggled);
        assert_eq!(x.flags, 2);
        // One bonus at a time without bit 5.
        x.row = 1;
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Refused);
        x.row = 0;
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page);
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Made);
        let d = x.last.clone().unwrap();
        assert_eq!(d.used, vec![(0x200, 2), (0x201, 1), (0x211, 1)]);
        assert_eq!((s.m.stock_at(0), s.m.stock_at(3)), (0, 0));
        // Rank bit 6 halves rank 0 on the Creations list only.
        s.m.unlocks |= 1 << 6;
        assert_eq!((synth::divisor(&b, &s.m, 0, 0, false), synth::divisor(&b, &s.m, 0, 0, true), synth::divisor(&b, &s.m, 2, 0, true)), (2, 4, 2));
        assert_eq!((synth::need(3, 2), synth::need(1, 4), synth::need(4, 2)), (2, 1, 2));
        // The higher-rank entry (1) needs its bonus; without it the make row refuses with NEED_RANK.
        s.m.made |= 2;
        assert_eq!(synth::entries(&b, &s.m, 0), vec![0, 1]);
        assert_eq!(synth::make_check(&b, &s, 0, 1, 0), Make::Notice(synth::msg::NEED_RANK));
        // Points with the first bonus: x2 (exp +0x18).
        assert_eq!((synth::points(&b, &mix.recipes[0], false), synth::points(&b, &mix.recipes[0], true)), (9, 18));
    }

    /// The disc: `mixdata.bar` and the executable's synthesis tables (skipped without `KH2_ISO`).
    #[test]
    fn the_disc_mixdata_recipes_levels_and_tables() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let mix = Mix::parse(&a.read("menu/us/mixdata.bar").unwrap()).expect("reci, cond, leve, exp");
        assert_eq!((mix.recipes.len(), mix.levels.len(), mix.conds.len()), (23, 9, 50));
        assert_eq!((mix.story_bit, mix.factor), (10, 2.0), "the Recipes row's bit; the points bonus doubles");
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).unwrap().rows;
        let mats: Vec<u16> = items.iter().filter(|i| i.r#type == synth::MATERIAL_TYPE).map(|i| i.id).take(crate::progress::MOOGLE_MATERIALS).collect();
        let msgs = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").unwrap()).unwrap();
        let name = |id: u16| items.iter().find(|i| i.id == id).and_then(|i| msgs.text(u32::from(i.name))).unwrap_or_default();
        eprintln!("materials (id, category, rank, name): {:?}", items.iter().filter(|i| i.r#type == synth::MATERIAL_TYPE).map(|i| (i.id, i.flag1, i.rank, name(i.id))).collect::<Vec<_>>());
        eprintln!("{} materials (type 0x10), levels {:?}", mats.len(), mix.levels.iter().map(|l| (l.unlock, l.points)).collect::<Vec<_>>());
        for r in &mix.recipes {
            assert!(r.mats.iter().all(|(id, n)| mats.contains(id) && *n > 0), "recipe {:#x}: every line is a material", r.id);
            assert!(items.iter().any(|i| i.id == r.result), "recipe {:#x} makes an item", r.id);
            eprintln!("recipe {:#x} ({}) kind {} rank {}: {} / {} from {:?}", r.id, name(r.id), r.unlock, r.rank, name(r.result), name(r.upgraded), r.mats.iter().map(|(id, n)| (name(*id), *n)).collect::<Vec<_>>());
        }
        assert_eq!(mix.recipes.iter().filter(|r| r.unlock == 0).count(), 10, "recipe items");
        let first = &mix.recipes[0];
        assert_eq!((first.id, first.mats.len()), (0x1c1, 3));
        assert!(mix.levels.windows(2).all(|w| w[0].points <= w[1].points), "the level thresholds rise");
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).unwrap();
        let exe = Exe::load(elf).unwrap();
        let t = Tables::read(&|v| exe.u16(v), &|v| exe.u8(v)).unwrap();
        let rows: Vec<String> = t.rows.iter().map(|&m| msgs.text(u32::from(m)).unwrap_or_default()).collect();
        eprintln!("page 9 rows {rows:?}, bonus {:x?}, rank bits {:?}, layouts {:?}", t.bonus, t.rank_bits, t.layouts);
        assert!(rows.iter().all(|r| !r.is_empty()), "every row names a message");
        assert!(t.bonus.iter().flatten().all(|id| mats.contains(id)), "the bonus materials are materials");
        let two = layd::parse_2ld(&a.read("menu/us/itemshop.2ld").unwrap()).unwrap();
        assert!(t.layouts.iter().all(|&l| (l as usize) < two.layd.layouts.len()));
    }

    /// The disc: the shop file, the "Shop" rows and the shopkeepers that offer them (skipped without `KH2_ISO`).
    #[test]
    fn the_disc_shop_file_rows_and_shopkeepers() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let idx: Vec<String> = crate::audio::WORLDS.iter().map(|w| format!("000{w}.idx")).collect();
        let _ = a.apply_names(idx.iter().map(String::as_str));
        let sys = a.read("03system.bin").unwrap();
        let bar = kh2_bar::parse(&sys).unwrap();
        let e = bar.iter().find(|e| e.kind == 0x29).expect("the type-0x29 entry");
        assert_eq!(e.tag, "shop");
        let shops = Shops::parse(e.data).expect("TZSH version 7");
        assert_eq!(shops.records.len(), 21);
        assert_eq!(shops.seen_ids.len(), 0x80);
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).unwrap().rows;
        let wares: Vec<Ware> = items.iter().map(Ware::of).collect();
        for r in &shops.records {
            assert!(opens_shop(u32::from(r.event)), "event {:#x} is one FUN_00197b70 sends to kind 4", r.event);
            for id in r.groups.iter().flat_map(|g| &g.items) {
                assert!(wares.iter().any(|w| w.id == *id), "item {id:#x} has a row");
            }
        }
        // Twilight Town's item shop (event 0x6a) on a first visit.
        let kinds = |id: u16| wares.iter().find(|w| w.id == id).map(|w| w.kind);
        let st = stock(&shops, 0x6a, &|_| false, &kinds).unwrap();
        let prices: Vec<(u16, u16, u16)> = st.items.iter().filter_map(|id| wares.iter().find(|w| w.id == *id)).map(|w| (w.id, w.buy, w.sell)).collect();
        eprintln!("shop 0x6a: header {:#x}, rows {:?}, wares (id, buy, sell) {prices:x?}", st.message, top_rows(&st));
        assert_eq!(top_rows(&st), vec![0, 1, 3], "Buy, Sell, Quit");
        assert!(prices.contains(&(1, 40, 10)), "Potion at 40, sells for 10");
        // The Shop rows: Execute 0x7f, an Argument that opens a shop, message 0x84c7.
        let cmds = kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).unwrap().rows;
        let shop_rows: Vec<(u16, i16)> = cmds.iter().filter(|c| c.execute == EXECUTE_EVENT && opens_shop(c.argument as u32)).map(|c| (c.id, c.argument)).collect();
        eprintln!("shop rows (id, event): {shop_rows:x?}");
        assert_eq!(shop_rows.len(), 21, "one row per shop record");
        assert!(cmds.iter().filter(|c| (195..=212).contains(&c.id)).all(|c| c.message_id == 0x84c7 && c.execute == EXECUTE_EVENT), "rows 195..212 are \"Shop\"");
        assert!(shop_rows.contains(&(209, 0x67)) && shop_rows.contains(&(212, 0x6a)));
        assert!(shop_rows.iter().all(|&(_, ev)| shops.find(ev as u16).is_some()), "every Shop row's event has a record");
        // Command 30 is Examine.
        let c30 = cmds.iter().find(|c| c.id == 30).unwrap();
        assert_eq!((c30.execute, c30.message_id), (0x1e, 0x81eb));
        // Twilight Town's shopkeepers: townspeople whose part command is a Shop row.
        let objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).unwrap().rows;
        let mut keepers = Vec::new();
        for n in 0..64 {
            let Ok(file) = a.read(&format!("ard/tt{n:02}.ard")) else { continue };
            let Some((ard, _)) = kh2_ard::Area::parse(&file) else { continue };
            for (tag, groups) in &ard.spawns {
                for e in groups.iter().flat_map(|g| &g.entities) {
                    if let Some(&(id, ev)) = shop_rows.iter().find(|r| r.0 == e.part_command()) {
                        // The programs whose commands name the group (the harness's KH2_PROGRAMS for a screenshot).
                        for (script, progs) in &ard.scripts {
                            for p in progs.iter().filter(|p| p.commands.iter().any(|c| tag.len() == 4 && c.args.contains(&u32::from_le_bytes(tag.as_bytes().try_into().unwrap())))) {
                                eprintln!("tt{n:02} group {tag}: named by script {script} program {:#x}", p.id);
                            }
                        }
                        let name = objs.iter().find(|o| o.object_id == e.object).map_or(String::new(), |o| o.model_name.clone());
                        keepers.push(format!("tt{n:02} {tag} {name} command {id} event {ev:#x}"));
                    }
                }
            }
        }
        eprintln!("Twilight Town shopkeepers: {keepers:?}");
        assert!(keepers.iter().any(|k| k.contains("N_EX650_TT_ITEM_RTN") && k.contains("event 0x6a")));
        assert!(keepers.iter().any(|k| k.contains("N_EX700_TT_WEAPON_RTN") && k.contains("event 0x67")));
        // itemshop.2ld and the executable's tables.
        let two = layd::parse_2ld(&a.read("menu/us/itemshop.2ld").unwrap()).expect("itemshop.2ld");
        assert_eq!(two.layd.layouts.len(), 18);
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).unwrap();
        let exe = Exe::load(elf).unwrap();
        let names: Vec<u16> = (0..4).map(|k| exe.u16(addr::ROWS + addr::ROW_SIZE * k + 0x12).unwrap()).collect();
        assert_eq!(names, vec![0x8acc, 0x8acd, 0xbbb5, 0x8ace], "Buy, Sell, Synthesize, Quit");
        let lay: Vec<u16> = (0..9).map(|i| exe.u16(addr::LAYOUTS + 2 * i).unwrap()).collect();
        eprintln!("layout triples {lay:x?}");
        assert!(lay.iter().all(|&l| (l as usize) < two.layd.layouts.len()), "{lay:?}");
    }

    /// Materials 0x200 (category 9, rank 0), 0x201 (category 1, rank 1), 0x210 (category 2, rank 2), 0x211 and 0x212
    /// (category 3, rank 3) for the reward tests.
    fn cat(id: u16) -> u8 {
        match id {
            0x200 => 9,
            0x201 => 1,
            0x210 => 2,
            _ => 3,
        }
    }

    fn rk(id: u16) -> u8 {
        match id {
            0x200 => 0,
            0x201 => 1,
            0x210 => 2,
            _ => 3,
        }
    }

    fn cond(kind: u8, reward: u16, a: u8, b: u8, c: u8, count: u16) -> synth::Cond {
        synth::Cond { text: 0xc000 + reward, reward, kind, a, b, c, count, bit: 0x40 + reward }
    }

    #[test]
    fn a_reward_condition_sums_totals_or_counts_kinds_and_collect_all_needs_every_one() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        let mut m = Moogle::default();
        // c 0: the sum of the totals of the matching materials (any category, any rank).
        let sum5 = cond(0, 1, 0xe, 5, 0, 5);
        m.set_total(0, 3);
        m.set_total(1, 1);
        assert!(!synth::cond_met(&b, &m, &sum5), "4 of 5");
        m.set_total(1, 2);
        assert!(synth::cond_met(&b, &m, &sum5), "5 of 5");
        // c 1: the number of different materials: category 2 and 3 (0x210, 0x211, 0x212): 2 needed; the category byte 1 is only 0x201.
        let kinds = cond(0, 2, 3, 5, 1, 2);
        assert!(!synth::cond_met(&b, &m, &kinds));
        m.set_total(3, 1);
        m.set_total(4, 1);
        assert!(synth::cond_met(&b, &m, &kinds), "0x211 and 0x212 are two materials of category 3");
        // The rank byte: rank 1 matches only 0x201 (category 1).
        let ranked = cond(0, 3, 0xe, 1, 0, 2);
        assert!(synth::cond_met(&b, &m, &ranked), "0x201 was handed over twice");
        assert!(!synth::cond_met(&b, &m, &cond(0, 3, 0xe, 1, 0, 3)));
        // 0x10: the special categories (9, 10, 11): 0x200 only; 0xf: the others.
        assert!(synth::cond_met(&b, &m, &cond(0, 4, 0x10, 5, 1, 1)));
        assert!(!synth::cond_met(&b, &m, &cond(0, 4, 0x10, 5, 1, 2)));
        assert!(synth::cond_met(&b, &m, &cond(0, 4, 0xf, 5, 1, 3)), "0x201, 0x211 and 0x212 have a total, 0x210 has none");
        assert!(!synth::cond_met(&b, &m, &cond(0, 4, 0xf, 5, 1, 4)));
    }

    #[test]
    fn a_collect_all_reward_fails_while_one_matching_material_was_never_handed_over() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        let mut m = Moogle::default();
        let all = cond(1, 5, 0xd, 5, 1, 1);
        for i in 0..4 {
            m.set_total(i, 1);
        }
        assert!(!synth::cond_met(&b, &m, &all), "0x212 never handed over");
        m.set_total(4, 1);
        assert!(synth::cond_met(&b, &m, &all));
        // b = 4 also means "all of them": the one material of category 9 passes with a total, fails without.
        let mut m = Moogle::default();
        m.set_total(0, 1);
        assert!(synth::cond_met(&b, &m, &cond(1, 5, 9, 4, 1, 1)));
        m.set_total(0, 0);
        assert!(!synth::cond_met(&b, &m, &cond(1, 5, 9, 4, 1, 1)));
    }

    #[test]
    fn rewards_are_paid_once_with_their_notices_unlocks_and_record_bits() {
        let (mut mix, t, mats) = mix();
        mix.conds = vec![
            // An item reward (0x30), a set reward (set 0), another set reward (set 1) and an unlock bit 7 with a record bit.
            cond(0, 0x30, 0xe, 5, 0, 1),
            cond(1, 0, 0xe, 5, 0, 1),
            cond(1, 1, 0xe, 5, 0, 99),
            cond(1, 7, 0xe, 5, 0, 1),
        ];
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        let mut s = Fake::default();
        s.items.insert(0x200, 2);
        let (x, o) = Session::start(&b, &mut s);
        // Rewards 0, 1 and 3 are met (2 handed over); 2 needs 99.
        assert_eq!(o.rewards.list, vec![(0, true), (1, true), (3, true)], "the unmet one is not listed without story flag 0x4837");
        assert_eq!(o.rewards.notices, 2 | 4 | 0x10);
        assert_eq!(o.notices, 1 | 2 | 4 | 0x10, "the hand-over's notice and the rewards'");
        assert_eq!(s.count(0x30), 1, "the item reward was added");
        assert_eq!((s.m.rewards, s.m.unlocks), (0b1011, 1 | 1 << 7), "reward bits, unlock bit 0 and 7");
        assert_eq!(s.records, vec![0x40 + 7], "the unlock-bit reward set its record bit");
        assert_eq!(o.rewards.items, vec![(0, 0x30)]);
        assert_eq!(x.rewards.len(), 3);
        assert_eq!(x.marks.rewards, 0b1011);
        // The notices come in table order, the first one on screen: the hand-over, then 1 (item), 2 (set 0), 4 (unlock).
        assert_eq!((x.text, x.queue.len()), (0xbc89, 3));
        // Opening again pays nothing new (the bits are set), the list stays.
        let (x, o) = Session::start(&b, &mut s);
        assert_eq!((o.notices, o.rewards.fresh, s.count(0x30)), (0, 0, 1));
        assert_eq!(x.rewards.len(), 3);
        // With story flag 0x4837 the unmet reward is listed too, not paid.
        s.flags.push(0x4837);
        let (_, o) = Session::start(&b, &mut s);
        assert_eq!(o.rewards.list, vec![(0, true), (1, true), (2, false), (3, true)]);
        assert_eq!(s.m.rewards & 0b100, 0);
    }

    #[test]
    fn a_full_bag_still_pays_the_reward_bit_but_the_item_is_not_added() {
        // `FUN_00291140` sets the reward bit and the notice, and adds the item only when `FUN_001a18b8` allows it (below 99 held).
        let (mut mix, t, mats) = mix();
        mix.conds = vec![cond(0, 0x30, 0xe, 5, 0, 1)];
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        let mut s = Fake::default();
        s.items.insert(0x200, 1);
        s.items.insert(0x30, 99);
        let (_, o) = Session::start(&b, &mut s);
        assert_eq!((o.rewards.notices, s.m.rewards, s.count(0x30)), (2, 1, 99));
    }

    #[test]
    fn page_0xb_lists_the_rewards_read_only_and_switches_with_page_10() {
        let (mut mix, t, mats) = mix();
        mix.conds = vec![cond(0, 0x30, 0xe, 5, 0, 1), cond(1, 0, 0xe, 5, 0, 1)];
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        let mut s = Fake::default();
        s.items.insert(0x200, 3);
        let (mut x, _) = Session::start(&b, &mut s);
        // Dismiss the notices; the hand-over leaves us on page 10.
        while x.page == synth::Page::Opening {
            x.key(Key::Confirm, &b, &mut s);
        }
        assert_eq!(x.page, synth::Page::Materials { totals: true });
        // -5 on page 10 -> page 0xb with the page 9 cursor on row 5.
        assert_eq!(x.key(Key::Switch, &b, &mut s), synth::Act::Page);
        assert_eq!((x.page, x.page.number()), (synth::Page::Rewards, 0xb));
        assert_eq!(x.rows[x.top], 5);
        // Down moves in the list, a confirm is refused, and the list does not change.
        assert_eq!(x.key(Key::Down, &b, &mut s), synth::Act::Moved);
        assert_eq!(x.rew, 1);
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Refused);
        assert_eq!(x.page, synth::Page::Rewards);
        // -5 back to page 10 (cursor on row 4), cancel to page 9.
        assert_eq!(x.key(Key::Switch, &b, &mut s), synth::Act::Page);
        assert_eq!((x.page, x.rows[x.top]), (synth::Page::Materials { totals: true }, 4));
        assert_eq!(x.key(Key::Switch, &b, &mut s), synth::Act::Page);
        assert_eq!(x.key(Key::Cancel, &b, &mut s), synth::Act::Page);
        assert_eq!(x.page, synth::Page::Top);
        // Page 0x17 (stock) has no switch.
        x.goto(&b, s.moogle(), synth::Page::Materials { totals: false });
        assert_eq!(x.key(Key::Switch, &b, &mut s), synth::Act::None);
    }

    #[test]
    fn the_marks_follow_the_session_and_fall_once_page_9_is_left() {
        let (mut mix, t, mats) = mix();
        mix.conds = vec![cond(0, 0x30, 0xe, 5, 0, 1)];
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        let mut s = Fake::default();
        s.items.insert(0x200, 1);
        s.items.insert(0x1c1, 1);
        s.flags.push(0x814);
        let (mut x, o) = Session::start(&b, &mut s);
        assert_eq!((o.known, o.rewards.fresh, o.learned), (1, 1, 1), "material 0 is new, reward 0 is paid, recipe 0 is learned");
        // Rows 3 and 4 (materials) and row 5 (rewards) carry a NEW mark; the Recipes row (2) a NEW mark for the learned item recipe.
        assert_eq!(synth::row_mark(&b, &s.m, &x.marks, 3), Some(true));
        assert_eq!(synth::row_mark(&b, &s.m, &x.marks, 4), Some(true));
        assert_eq!(synth::row_mark(&b, &s.m, &x.marks, 5), Some(true));
        assert_eq!(synth::row_mark(&b, &s.m, &x.marks, 2), Some(true));
        assert_eq!(synth::row_mark(&b, &s.m, &x.marks, 6), None);
        assert!(synth::has_new(&s.m), "the third row of the shop carries a NEW mark while a recipe is unseen");
        // Leaving the first page lowers `+0x112d`: the marks become VIEWED.
        while x.page == synth::Page::Opening {
            x.key(Key::Confirm, &b, &mut s);
        }
        assert!(x.marks.fresh);
        x.page = synth::Page::Top;
        x.rows = synth::top_rows(&b, s.moogle());
        x.top = x.rows.iter().position(|&r| r == 3).unwrap();
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page);
        assert!(!x.marks.fresh);
        assert_eq!(synth::row_mark(&b, &s.m, &x.marks, 5), Some(false));
        // The Recipes list: the entry of the learned recipe is marked NEW; the cursor marks it seen (record +0x20).
        x.page = synth::Page::Top;
        x.top = x.rows.iter().position(|&r| r == 2).unwrap();
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page);
        assert_eq!(x.page, synth::Page::List(2));
        assert!(Moogle::bit(s.m.seen, 0), "the cursor was on entry 0");
        assert_eq!(x.entry_mark(&s.m, 0), Some(false), "NEW became VIEWED");
        assert!(!synth::has_new(&s.m));
    }

    #[test]
    fn the_shops_new_items_are_the_ones_whose_save_bit_is_clear_and_the_cursor_marks_them() {
        let mut sh = shops();
        sh.seen_ids = vec![0x50, 1, 2, 0x83];
        // Place of 1 is 1, of 0x83 is 3; 7 is not in the list.
        assert_eq!((item_place(&sh, 1), item_place(&sh, 0x83), item_place(&sh, 7)), (Some(1), Some(3), None));
        let mut seen = [false; 4];
        let fresh = fresh_at_open(&sh, &[1, 0x83, 7], &|p| seen[p]);
        assert_eq!(fresh, 0b1010);
        // Place 2 (item 2) was not in the stock, so it is not fresh; an item already seen is not fresh either.
        seen[3] = true;
        assert_eq!(fresh_at_open(&sh, &[1, 0x83], &|p| seen[p]), 0b10);
        let fresh = 0b1010;
        seen = [false; 4];
        assert_eq!(buy_tag(fresh, &|p| seen[p]), Some(Tag::New));
        assert_eq!(item_tag(&sh, fresh, 1, &|p| seen[p]), Some(Tag::New));
        assert_eq!(item_tag(&sh, fresh, 2, &|p| seen[p]), None, "not fresh at the open");
        // The cursor on item 1 sets its bit: its tag turns VIEWED, the Buy row stays NEW while 0x83 is unseen.
        seen[1] = true;
        assert_eq!(item_tag(&sh, fresh, 1, &|p| seen[p]), Some(Tag::Viewed));
        assert_eq!(buy_tag(fresh, &|p| seen[p]), Some(Tag::New));
        seen[3] = true;
        assert_eq!(buy_tag(fresh, &|p| seen[p]), Some(Tag::Viewed));
        assert_eq!(buy_tag(0, &|_| false), None);
    }

    #[test]
    fn the_first_visit_text_shows_once_per_class_of_shop() {
        let shop = |event| stock(&shops(), event, &|_| false, &kind).unwrap();
        // A weapon shop (flags 0x181: flag 0x80 set, so the state's +0xd is clear): the text of bit 0.
        let weapon = shop(0x67);
        assert!(!weapon.first_group);
        assert_eq!(first_visit_bit(&weapon, &|_| false), Some(0));
        assert_eq!(first_visit_bit(&weapon, &|b| b == 0), None);
        // The moogle (flags 0x31f: flag 0x80 clear and its first group has items): the text of bit 1, even with the third row.
        let moogle = shop(0x70);
        assert!(moogle.first_group && moogle.rows_bits & 2 != 0);
        assert_eq!(first_visit_bit(&moogle, &|_| false), Some(1));
        assert_eq!(first_visit_bit(&moogle, &|b| b == 0), Some(1), "the other class's bit does not count");
        assert_eq!(first_visit_bit(&moogle, &|b| b == 1), None);
        // The third row alone (no first group): never a text (`FUN_00265498`: 0xff02 == 2).
        let mut third = moogle.clone();
        third.first_group = false;
        assert_eq!(first_visit_bit(&third, &|_| false), None);
    }

    #[test]
    fn page_7_waits_for_a_confirm_or_a_cancel_then_shows_the_top_rows() {
        let none: [(Ware, u8); 0] = [];
        let owned = |_: u16| 0u8;
        let v = View { buy: &[], sell: &none, owned: &owned, munny: 0 };
        let mut f = Flow::new(vec![0, 1, 3]);
        f.page = Page::First;
        assert_eq!(f.page.number(), 7);
        for k in [Key::Up, Key::Down, Key::Start, Key::Switch] {
            assert_eq!(f.key(k, &v), Act::None, "{k:?} does nothing on page 7");
            assert_eq!(f.page, Page::First);
        }
        assert_eq!(f.key(Key::Confirm, &v), Act::Page);
        assert_eq!(f.page, Page::Top);
        f.page = Page::First;
        assert_eq!(f.key(Key::Cancel, &v), Act::Page);
        assert_eq!(f.page, Page::Top);
    }


    /// Tables with a category name per material category (message 0x100 + category) and a name for item type 1.
    fn named() -> Tables {
        let mut t = mix().1;
        for k in 0..13 {
            t.category_names[k] = 0x100 + k as u16;
            t.category_icons[k] = 0x20 + k as u16;
        }
        t.type_names = vec![(0, 0x200), (1, 0x201)];
        t.type_icons = vec![(1, 7)];
        t
    }

    #[test]
    fn a_reward_row_shows_the_item_behind_its_icon_or_its_kind_and_question_marks() {
        let t = named();
        let item = synth::Cond { text: 1, reward: 0x30, kind: 0, a: 0xe, b: 5, c: 1, count: 1, bit: 0 };
        // A material (type 0x10, category 4): the category's icon; unreached, the category's name and a mask of the name.
        let mat = Some((synth::MATERIAL_TYPE, 4, 0x777));
        assert_eq!(synth::reward_text(&t, &item, true, mat), Some(synth::RewardText::Item { icon: 0x24, name: 0x777 }));
        assert_eq!(synth::reward_text(&t, &item, false, mat), Some(synth::RewardText::Hidden { head: Some(0x104), mask: 0x777 }));
        // Another type: its icon from the type table (0xb when the type is not in it) and its own name.
        assert_eq!(synth::reward_text(&t, &item, true, Some((1, 0, 5))), Some(synth::RewardText::Item { icon: 7, name: 5 }));
        assert_eq!(synth::reward_text(&t, &item, true, Some((9, 0, 5))), Some(synth::RewardText::Item { icon: 0xb, name: 5 }));
        assert_eq!(synth::reward_text(&t, &item, false, Some((1, 0, 5))), Some(synth::RewardText::Hidden { head: Some(0x201), mask: 5 }));
        assert_eq!(synth::reward_text(&t, &item, false, Some((9, 0, 5))), Some(synth::RewardText::Hidden { head: None, mask: 5 }), "a type with no name");
        assert_eq!(synth::reward_text(&t, &item, true, None), None, "an item reward without an item row");
        // An unlock: the message of its set or bit with the variable filled; unreached, message 0xc1c5 then a mask of it.
        let unlock = |reward| synth::Cond { reward, kind: 1, ..item.clone() };
        assert_eq!(synth::reward_text(&t, &unlock(0), true, None), Some(synth::RewardText::Filled { text: synth::msg::REWARD_SET0 }));
        assert_eq!(synth::reward_text(&t, &unlock(1), false, None), Some(synth::RewardText::Hidden { head: Some(synth::msg::REWARD_HEAD), mask: synth::msg::REWARD_SET1 }));
        assert_eq!(synth::reward_text(&t, &unlock(5), true, None), Some(synth::RewardText::Filled { text: synth::msg::REWARD_UNLOCK }));
    }

    #[test]
    fn a_text_masks_one_question_mark_per_character_and_fills_its_variables_in_order() {
        use kh2_image::text::Token as T;
        // Glyphs and icons count; spaces, colours and the end do not (`FUN_00191660`); a newline ends the count.
        let name = vec![T::Palette(2), T::Glyph(1), T::Space, T::Glyph(2), T::Icon(3), T::Newline, T::Glyph(9), T::End];
        assert_eq!(shown_chars(&name), 3);
        let filled = fill(vec![T::Glyph(1), T::Variable, T::Glyph(2), T::Variable, T::Glyph(3), T::Variable], &[vec![T::Glyph(10), T::End], vec![T::Glyph(11)]]);
        assert_eq!(filled, vec![T::Glyph(1), T::Glyph(10), T::Glyph(2), T::Glyph(11), T::Glyph(3)], "a variable with no text left shows nothing");
    }

    #[test]
    fn story_flag_4837_lists_what_the_player_cannot_have_yet_and_never_opens_it() {
        let (mut mix, t, mats) = mix();
        mix.conds = vec![cond(0, 0x30, 0xe, 5, 0, 1)];
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        // Flag clear (the default): nothing made or learned, so every list and page 10 are empty, the reward unlisted.
        let mut s = Fake::default();
        let (mut x, _) = Session::start(&b, &mut s);
        assert!(!x.all);
        for k in 0..3 {
            x.goto(&b, &s.m, synth::Page::List(k));
            assert!(x.entries.is_empty(), "list {k}");
        }
        x.goto(&b, &s.m, synth::Page::Materials { totals: true });
        assert!(x.slots.is_empty());
        assert!(x.rewards.is_empty());
        // Flag set: Creations hold every entry not made (the higher rank only of recipe 0, which has one), Free
        // Development the recipe of set 1 and Recipes the recipe item, all flagged; page 10 every material.
        s.flags.push(0x4837);
        let (mut x, _) = Session::start(&b, &mut s);
        assert!(x.all);
        let hidden = |e: u16| e | synth::HIDDEN;
        x.goto(&b, &s.m, synth::Page::List(0));
        assert_eq!(x.entries, vec![hidden(0), hidden(1), hidden(2)]);
        x.goto(&b, &s.m, synth::Page::List(1));
        assert_eq!(x.entries, vec![hidden(2)]);
        x.goto(&b, &s.m, synth::Page::List(2));
        assert_eq!(x.entries, vec![hidden(0)]);
        // A hidden entry is refused and is not marked seen.
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Refused);
        assert!(!Moogle::bit(s.m.seen, 0));
        s.m.learned = 1;
        x.goto(&b, &s.m, synth::Page::List(2));
        assert_eq!(x.entries, vec![0], "learned: the plain entry, no hidden twin");
        x.goto(&b, &s.m, synth::Page::Materials { totals: true });
        assert_eq!(x.slots, vec![0, 1, 2, 3, 4], "every material, none handed over");
        x.goto(&b, &s.m, synth::Page::Materials { totals: false });
        assert!(x.slots.is_empty(), "the stock page lists only what is held");
        assert_eq!(x.rewards, vec![(0, false)], "the unreached reward is listed");
        // A made entry is listed plain; only the entries not made are hidden.
        s.m.made = 1;
        x.goto(&b, &s.m, synth::Page::List(0));
        assert_eq!(x.entries, vec![0, hidden(1), hidden(2)]);
    }

    #[test]
    fn the_item_and_unlock_notices_name_the_reward_item_and_the_material_the_reward_marks() {
        let (mut mix, t, mats) = mix();
        // Rewards 0 and 2: items for one material. Reward 1: an unlock bit 7 for category 3, rank 3 (the first is 0x211, place 3).
        mix.conds = vec![cond(0, 0x30, 0xe, 5, 0, 1), cond(1, 7, 3, 3, 1, 1), cond(0, 0x31, 0xe, 5, 0, 1)];
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        assert_eq!(synth::first_material(&b, 3, 3), Some(3));
        assert_eq!(synth::first_material(&b, 4, 3), None);
        let mut s = Fake::default();
        s.items.insert(0x211, 1);
        let (mut x, o) = Session::start(&b, &mut s);
        assert_eq!(o.rewards.materials, 1 << 3, "state +0x134: the place of 0x211");
        assert_eq!(o.rewards.items, vec![(0, 0x30), (2, 0x31)], "state +0x13c: both reward items");
        // The hand-over (notice 0), the item notice once for each item, the unlock notice for the material.
        assert_eq!((x.text, x.text_item), (0xbc89, None));
        assert_eq!(x.queue.len(), 3);
        assert_eq!(x.queue_item, vec![Some(0x30), Some(0x31), Some(0x211)]);
        // Each confirm moves to the next text and its item.
        for want in [Some(0x30), Some(0x31), Some(0x211)] {
            assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::Page);
            assert_eq!(x.text_item, want);
        }
        assert!(x.queue.is_empty() && x.queue_item.is_empty());
    }

    #[test]
    fn a_material_handed_over_for_the_first_time_carries_a_tag_on_the_material_pages() {
        let mut k = synth::Marks { known: 0b10, fresh: true, ..synth::Marks::default() };
        assert_eq!((synth::material_mark(&k, 0), synth::material_mark(&k, 1)), (None, Some(true)), "NEW while +0x112d is set");
        k.fresh = false;
        assert_eq!(synth::material_mark(&k, 1), Some(false), "VIEWED after");
        // Only the slots whose bit is set (session +8) carry one, whatever `fresh` says; slot 63 is the last bit.
        let k = synth::Marks { known: 1 << 63 | 1 << 5, fresh: false, ..synth::Marks::default() };
        let marked: Vec<usize> = (0..64).filter(|&i| synth::material_mark(&k, i).is_some()).collect();
        assert_eq!(marked, vec![5, 63]);
        // The tag follows the opening: a material handed over once is marked, one handed over before (no `+8` bit) is not.
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rank, flag1: &flag1 };
        let mut s = Fake::default();
        s.items.insert(0x201, 1);
        let (x, _) = Session::start(&b, &mut s);
        assert_eq!((synth::material_mark(&x.marks, 0), synth::material_mark(&x.marks, 1), x.marks.fresh), (None, Some(true), true), "0x201 is slot 1");
        s.items.insert(0x201, 1);
        let (x, _) = Session::start(&b, &mut s);
        assert_eq!(synth::material_mark(&x.marks, 1), None, "handed over again: no longer first time");
    }

    /// The disc's reward rows: every reward has a second-column text, the notices of items and unlocks have two variables
    /// for the item's kind and name, and the shop's list groups give the row pitch and the second column's x.
    #[test]
    fn the_disc_reward_texts_notices_and_list_extents() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let two = layd::parse_2ld(&a.read("menu/us/itemshop.2ld").unwrap()).unwrap();
        let seq = two.layd.seqs.iter().position(|q| q.groups.len() > 0x78).expect("the sequence with the row groups");
        let q = &two.layd.seqs[seq];
        for g in [0x24usize, 0x60, 0x61, 0x62, 0x65, 0x66, 0x67, 0x68, 0x6e, 0x6f] {
            if let Some(x) = q.groups.get(g) {
                eprintln!("group {g:#x}: x {} y {} size {} +0x1c {} +0x20 {}", x.x, x.y, x.size, x.unk1c, x.unk20);
            }
        }
        for l in 0..two.layd.layouts.len() {
            eprintln!("layout {l}: {:?}", two.layd.layout(l).iter().map(|y| (y.seq, y.group, y.x, y.y)).collect::<Vec<_>>());
        }
        let plate = &q.groups[ROW_PLATE];
        assert_eq!((plate.x, plate.size, plate.unk1c), (270, 13, 23), "FUN_0028b470: the second text at x 270, size 13, rows 23 apart");
        // The pitches the port applies to the material pages (`group_pitch`): page 10's group 0x60 and page 0x17's group 0x6e.
        let menu = ShopMenu { seqd: Some(q.clone()), mark_seq: Some(seq), ..ShopMenu::default() };
        assert_eq!((group_pitch(&menu, 0x60), group_pitch(&menu, 0x6e)), (23.0, 26.0));
        assert_eq!(group_pitch(&ShopMenu::default(), 0x60), at::PITCH, "no file: the port's pitch");
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).unwrap();
        let exe = Exe::load(elf).unwrap();
        let t = synth::Tables::read(&|v| exe.u16(v), &|v| exe.u8(v)).unwrap();
        eprintln!("category names {:x?} icons {:x?} type names {:x?} type icons {:x?}", t.category_names, t.category_icons, t.type_names, t.type_icons);
        assert!(t.category_names.iter().all(|&m| m != 0) && !t.type_names.is_empty() && !t.type_icons.is_empty());
        let mix = Mix::parse(&a.read("menu/us/mixdata.bar").unwrap()).unwrap();
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).unwrap().rows;
        let msgs = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").unwrap()).unwrap();
        let txt = |id: u32| msgs.text(id).unwrap_or_default();
        for k in [1usize, 4] {
            let toks = msgs.tokens(u32::from(t.notices[k])).unwrap();
            eprintln!("notice {k} {:#x}: \"{}\" variables {}", t.notices[k], txt(u32::from(t.notices[k])), toks.iter().filter(|x| **x == Token::Variable).count());
        }
        for (k, c) in mix.conds.iter().enumerate() {
            let item = items.iter().find(|i| i.id == c.reward).map(|i| (i.r#type, i.flag1, i.name));
            for met in [true, false] {
                let r = synth::reward_text(&t, c, met, item).unwrap_or_else(|| panic!("reward {k} has no text"));
                let shown = match r {
                    synth::RewardText::Item { icon, name } => format!("[icon {icon:#x}] {}", txt(name)),
                    synth::RewardText::Hidden { head, mask } => format!("{}{}", head.map(&txt).unwrap_or_default(), "?".repeat(shown_chars(&msgs.tokens(mask).unwrap()))),
                    synth::RewardText::Filled { text } => txt(text).replace('\r', &txt(synth::msg::REWARD_HEAD)),
                };
                if k < 3 || k % 10 == 0 {
                    eprintln!("reward {k} met {met}: {shown:?}");
                }
                if c.kind == 0 && !met {
                    assert!(matches!(r, synth::RewardText::Hidden { head: Some(_), .. }), "reward {k}: an unreached item names its kind");
                }
            }
        }
        for (i, m) in mix.conds.iter().enumerate().filter(|(_, c)| c.kind != 0 && c.reward > 1) {
            let found = synth::first_material(&synth::Book { mix: &mix, t: &t, mats: &items.iter().filter(|x| x.r#type == synth::MATERIAL_TYPE).map(|x| x.id).collect::<Vec<_>>(), rank: &|id| items.iter().find(|x| x.id == id).map_or(0, |x| x.rank), flag1: &|id| items.iter().find(|x| x.id == id).map_or(0, |x| x.flag1) }, m.a, m.b);
            assert!(found.is_some(), "unlock reward {i} (category {}, rank {}) marks a material", m.a, m.b);
        }
    }

    /// The disc: `itemshop.2ld` has the tag groups 0x77 / 0x78 and the rows' plate groups, and `cond` rows read like
    /// the rules `cond_met` ports (skipped without `KH2_ISO`).
    /// Page 10's count-up (`FUN_0028f490`): the displayed totals start at the record's values from before the hand-over
    /// and rise by one a frame, or jump to the record's while a button is held; a differing slot marks its row, rank tab
    /// and category tab; nothing moves once `+0x112d` (`marks.fresh`) has fallen, and a total above the record stays.
    #[test]
    fn the_count_up_steps_one_a_frame_or_jumps_and_marks_the_tabs() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        let mut s = Fake::default();
        s.m.totals = vec![0; 0x32];
        s.m.totals[0] = 5;
        s.items.insert(0x200, 3);
        s.items.insert(0x201, 2);
        let (mut x, o) = Session::start(&b, &mut s);
        assert_eq!((s.m.total_at(0), s.m.total_at(1)), (8, 2), "the hand-over raised the record");
        assert_eq!((o.before[0], o.before[1], o.before.len()), (5, 0, 0x32), "the display starts at the totals from before");
        assert_eq!((x.shown[0], x.shown[1]), (5, 0));
        assert!(x.marks.fresh);
        let mut seen = Vec::new();
        while x.count_up(&b, &s.m, false) {
            seen.push((x.shown[0], x.shown[1]));
        }
        assert_eq!(seen, vec![(6, 1), (7, 2), (8, 2)], "one step a frame, each total stopping at the record's");
        assert_eq!(x.marks.rows, 0b11);
        // Slot 0 is rank 0 (tab 3) of category 9, slot 1 rank 1 (tab 2) of category 1.
        assert_eq!((x.marks.rank_tabs, x.marks.cat_tabs), (0b1100, 1 << 9 | 1 << 1));
        // A held button jumps at once.
        x.shown = o.before.clone();
        assert!(x.count_up(&b, &s.m, true));
        assert_eq!((x.shown[0], x.shown[1]), (8, 2));
        assert!(!x.count_up(&b, &s.m, true), "equal: nothing to do");
        // A displayed total above the record is not lowered.
        x.shown[0] = 10;
        assert!(x.count_up(&b, &s.m, false));
        assert_eq!(x.shown[0], 10);
        // Once the first page is left the display is no longer stepped.
        x.shown = o.before.clone();
        x.marks.fresh = false;
        assert!(!x.count_up(&b, &s.m, false));
        assert_eq!((x.shown[0], x.shown[1]), (5, 0));
    }

    /// `FUN_0028f698`: rank sums stop at 99,999, category sums at 9,999, the total at 99,999; the kinds count the non-zero values.
    #[test]
    fn the_tab_sums_are_capped_per_rank_category_and_total() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rk, flag1: &cat };
        let small = synth::tab_sums(&b, &|i| [3, 4][i.min(1)] * u32::from(i < 2));
        assert_eq!((small.ranks, small.kinds, small.total), ([3, 4, 0, 0], 2, 7));
        assert_eq!((small.cats[9], small.cats[1]), (3, 4));
        let big = synth::tab_sums(&b, &|i| [60_000, 50_000][i.min(1)] * u32::from(i < 2));
        assert_eq!((big.ranks, big.total), ([60_000, 50_000, 0, 0], 99_999));
        assert_eq!((big.cats[9], big.cats[1]), (9_999, 9_999));
        let huge = synth::tab_sums(&b, &|i| if i == 0 { 200_000 } else { 0 });
        assert_eq!((huge.ranks[0], huge.total, huge.kinds), (99_999, 99_999, 1));
    }

    /// The header window of pages 10 and 0xb (`FUN_0028aaa8`) on the disc: group 0x67's anchor animation (flag bit 2) rests at
    /// (24, 60); the empty-list texts' groups 0x66 and 0x6f have anchors 232 and the window's 24 put them at half the 512 screen,
    /// which is why the group record's x 460 is not added; the rows hang on the plate groups' anchors, the tabs on groups 99
    /// and 100 (skipped without `KH2_ISO`).
    #[test]
    fn the_disc_window_origin_row_origins_and_tab_places() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let two = layd::parse_2ld(&a.read("menu/us/itemshop.2ld").unwrap()).unwrap();
        let q = &two.layd.seqs[two.layd.seqs.iter().position(|q| q.groups.len() > TAG_NEW.max(TAG_VIEWED)).unwrap()];
        assert_eq!(anchor_of(q, WINDOW_GROUP), Some([24.0, 60.0]));
        assert_eq!((q.groups[WINDOW_GROUP].x, q.groups[WINDOW_GROUP].y), (460, 123), "the record that is not added");
        assert_eq!(anchor_of(q, 0x66).map(|a| 24.0 + a[0]), Some(256.0));
        assert_eq!(anchor_of(q, 0x6f).map(|a| 24.0 + a[0]), Some(256.0));
        assert_eq!(plate_origin(q, 0x60), Some([62.0, 162.0]), "page 10");
        assert_eq!(plate_origin(q, ROW_PLATE), Some([43.0, 162.0]), "page 0xb");
        assert_eq!(plate_origin(q, 0x6e), Some([64.0, 67.0]), "page 0x17");
        let tabs = tab_places(q).expect("the tab groups");
        assert_eq!(tabs.len(), TABS);
        assert_eq!(tabs[0], ([23.0, 62.0], [123.0, 62.0]), "rank tab 0: name at the anchor, number 100 to the right");
        assert_eq!(tabs[3].0, [23.0, 131.0], "rank tab 3: three pitches (23) down");
        assert_eq!(tabs[4], ([139.0, 62.0], [214.0, 62.0]), "category 0");
        assert_eq!(tabs[4 + 5].0, [229.0, 85.0], "category 5: second column (pitch 90), second row");
        assert_eq!(tabs[4 + 12].0, [139.0, 131.0], "category 12 starts the fourth row");
        assert!(tabs[4 + 3].1[0] < 512.0, "the last column's number starts inside the screen");
    }

    #[test]
    fn the_disc_shop_layout_has_the_tag_groups_and_the_rewards_read_as_conditions() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let two = layd::parse_2ld(&a.read("menu/us/itemshop.2ld").unwrap()).unwrap();
        for (i, q) in two.layd.seqs.iter().enumerate() {
            let tags: Vec<(usize, usize, i32, i32)> = [TAG_VIEWED, TAG_NEW, 0x24, 0x61, 0x62].iter().filter_map(|&g| q.groups.get(g).map(|x| (g, q.last_frame(g) as usize, x.x, x.y))).collect();
            eprintln!("itemshop.2ld seq {i}: {} groups, (group, last frame, x, y) {tags:x?}", q.groups.len());
        }
        let seq = two.layd.seqs.iter().position(|q| q.groups.len() > TAG_NEW.max(TAG_VIEWED)).expect("a sequence with the tag groups");
        eprintln!("tag sequence {seq}; {} images {:?}", two.images.len(), two.images.iter().map(|i| (i.width, i.height)).collect::<Vec<_>>());
        for g in [TAG_VIEWED, TAG_NEW] {
            let q = &two.layd.seqs[seq];
            let ext = kh2_image::seqd::Ext::default();
            for f in [0, 5, 30, 60, 100] {
                let quads = q.quads(g, f, [0.0, 0.0], &ext);
                eprintln!("group {g:#x} frame {f}: {} quads {:?}", quads.len(), quads.iter().take(3).map(|x| (x.pos[0], x.pos[3], x.uv[0], x.uv[3])).collect::<Vec<_>>());
            }
        }
        let mix = Mix::parse(&a.read("menu/us/mixdata.bar").unwrap()).unwrap();
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).unwrap().rows;
        let msgs = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").unwrap()).unwrap();
        let name = |id: u16| items.iter().find(|i| i.id == id).and_then(|i| msgs.text(u32::from(i.name))).unwrap_or_default();
        let mut kinds = [0usize; 4];
        for (k, c) in mix.conds.iter().enumerate() {
            kinds[usize::from(c.kind.min(3))] += 1;
            let reward = if c.kind == 0 { name(c.reward) } else { format!("unlock {}", c.reward) };
            eprintln!("reward {k}: \"{}\" kind {} a {:#x} b {} c {} count {} bit {:#x} -> {reward}", msgs.text(u32::from(c.text)).unwrap_or_default(), c.kind, c.a, c.b, c.c, c.count, c.bit);
        }
        eprintln!("reward kinds {kinds:?}");
        assert!(mix.conds.iter().all(|c| c.count > 0 && c.a <= 0x10 && c.b <= 5 && c.c <= 1), "the condition bytes are in the ranges the tests read");
        assert!(mix.conds.iter().filter(|c| c.kind == 0).all(|c| items.iter().any(|i| i.id == c.reward)), "an item reward is an item row");
    }

}
