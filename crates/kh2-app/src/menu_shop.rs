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
use crate::progress::Progress;
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
}

impl Ware {
    pub fn of(i: &t::Item) -> Ware {
        Ware { id: i.id, kind: i.r#type, name: i.name, buy: i.shop_buy, sell: i.shop_sell }
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
            Page::List { sell } => if sell { 4 } else { 1 },
            Page::Quantity { sell } => if sell { 5 } else { 2 },
            Page::Question { sell } => if sell { 6 } else { 3 },
        }
    }

    /// The screen whose layout triple the page shows (0 top, 1 buy, 2 sell).
    fn screen(self) -> usize {
        match self {
            Page::Top => 0,
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
                Key::Start => Act::Close,
            },
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
}

impl Nodes {
    fn all(&self) -> Vec<usize> {
        let mut v = self.layers.clone();
        v.extend(self.texts());
        v
    }

    fn texts(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.rows.iter().chain(&self.list).chain(&self.prices).chain(&self.owned).chain(&self.question).copied().collect();
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
    layouts: [[usize; 3]; 3],
    layd: Layd,
    seq_base: usize,
    tex: usize,
    nodes: Nodes,
    open: Option<Open>,
}

struct Open {
    event: u16,
    stock: Stock,
    flow: Flow,
    /// The sell list as `sell_list` built it (rebuilt after a deal, `FUN_00264be8`).
    sell: Vec<(Ware, u8)>,
    closing: u32,
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
    let mut layouts = [[0usize; 3]; 3];
    for (k, l) in layouts.iter_mut().enumerate() {
        for (i, v) in l.iter_mut().enumerate() {
            *v = exe.u16(addr::LAYOUTS + 2 * (3 * k + i) as u32).map_or(0, usize::from);
        }
    }
    if scene.text.is_none() || two.images.is_empty() {
        return;
    }
    m.tex = scene.texs.len();
    for img in &two.images {
        scene.texs.push(menu_gfx::Tex::new(img, images));
    }
    m.seq_base = scene.seqs.len();
    let Layd { layers, layouts: lays, seqs } = two.layd;
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
    for _ in 0..LIST_ROWS {
        n.list.push(add(scene, None, tex));
        n.prices.push(add(scene, None, tex));
        n.owned.push(add(scene, None, tex));
    }
    n.hand = add(scene, None, tex);
    n.detail = add(scene, None, tex);
    n.munny = add(scene, None, tex);
    n.question = [add(scene, None, tex), add(scene, None, tex)];
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
    let t = m.layouts[screen.min(2)];
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

/// Draws the page the flow is on.
fn redraw(m: &ShopMenu, scene: &mut MenuScene, owned: &dyn Fn(u16) -> u8, munny: u32) {
    // The text is taken out of the scene while the labels are set, and put back.
    let Some(text) = scene.text.take() else { return };
    draw_page(m, scene, &text, owned, munny);
    scene.text = Some(text);
}

fn draw_page(m: &ShopMenu, scene: &mut MenuScene, text: &MenuText, owned: &dyn Fn(u16) -> u8, munny: u32) {
    let Some(o) = m.open.as_ref() else { return };
    let n = &m.nodes;
    for id in n.texts() {
        set_label(scene, id, None);
    }
    set_label(scene, n.title, Some(text_label(&text, message(&text, u32::from(o.stock.message)), at::SELECTED, 0, at::TITLE)));
    set_label(scene, n.munny, Some(text_label(&text, literal(&text, &format!("{munny}")), at::SELECTED, 1, at::MUNNY)));
    let row_y = |k: usize| at::ROWS[1] + at::PITCH * k as f32;
    let hand = text_label(&text, literal(&text, ">"), at::SELECTED, 0, [0.0, 0.0]);
    let put_hand = |scene: &mut MenuScene, k: usize| {
        let mut l = hand.clone();
        l.offset = [at::HAND[0], row_y(k)];
        set_label(scene, n.hand, Some(l));
    };
    match o.flow.page {
        Page::Top => {
            for (k, &r) in o.flow.rows.iter().enumerate() {
                let c = if k == o.flow.top { at::SELECTED } else { at::IDLE };
                set_label(scene, n.rows[k], Some(text_label(&text, message(&text, m.row_names[r]), c, 0, [at::ROWS[0], row_y(k)])));
            }
            put_hand(scene, o.flow.top);
        }
        Page::List { sell } | Page::Quantity { sell } | Page::Question { sell } => {
            let list: Vec<(Ware, u8)> = if sell { o.sell.clone() } else { buy_wares(m, &o.stock).into_iter().map(|w| (w, owned(w.id))).collect() };
            let first = o.flow.list.saturating_sub(LIST_ROWS - 1);
            for (k, (w, have)) in list.iter().enumerate().skip(first).take(LIST_ROWS) {
                let row = k - first;
                let c = if !row_open(sell, w, *have, munny) { at::GREY } else if k == o.flow.list { at::SELECTED } else { at::IDLE };
                set_label(scene, n.list[row], Some(text_label(&text, message(&text, u32::from(w.name)), c, 0, [at::ROWS[0], row_y(row)])));
                let price = literal(&text, &format!("{}", if sell { w.sell } else { w.buy }));
                set_label(scene, n.prices[row], Some(text_label(&text, price, c, 1, [at::PRICE_RIGHT, row_y(row)])));
                set_label(scene, n.owned[row], Some(text_label(&text, literal(&text, &format!("{have}")), c, 1, [at::OWNED_RIGHT, row_y(row)])));
            }
            if matches!(o.flow.page, Page::List { .. }) {
                put_hand(scene, o.flow.list - first);
            } else {
                let d = o.flow.deal;
                let mut tokens: Vec<Token> = m.wares.iter().find(|w| w.id == d.item).map(|w| message(&text, u32::from(w.name))).unwrap_or_default().into_iter().filter(|t| *t != Token::End).collect();
                tokens.extend(literal(&text, &format!("  x{}  {}", d.qty, d.total())));
                set_label(scene, n.detail, Some(text_label(&text, tokens, at::SELECTED, 0, at::DETAIL)));
            }
            if matches!(o.flow.page, Page::Question { .. }) {
                for (k, id) in [msg::NO, msg::YES].into_iter().enumerate() {
                    let c = if k == o.flow.ask { at::SELECTED } else { at::IDLE };
                    set_label(scene, n.question[k], Some(text_label(&text, message(&text, id), c, 0, [at::DETAIL[0] + 300.0 + 60.0 * k as f32, at::DETAIL[1]])));
                }
            }
        }
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
        m.open = Some(Open { event, flow: Flow::new(rows), stock, sell, closing: 0 });
        scene.hidden = false;
        scene.dim = crate::menu::dim_alpha(crate::menu::DIM_BRIGHTNESS);
        hide_all(&m, &mut scene);
        set_layout(&m, &mut scene, 0);
        let munny = progress.as_deref().map_or(0, |p| p.wallet());
        let owned = |id: u16| progress.as_deref().map_or(0, |p| p.item_count(id));
        redraw(&m, &mut scene, &owned, munny);
        menu_sound::play(&mut se, if third { snd::OPEN_THIRD_ROW } else { snd::OPEN });
        keys.reset_all();
        return;
    }
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
    let munny = progress.as_deref().map_or(0, |p| p.wallet());
    let buy = buy_wares(&m, &o.stock);
    let sell = o.sell.clone();
    let mut flow = o.flow.clone();
    let act = {
        let owned = |id: u16| progress.as_deref().map_or(0, |p| p.item_count(id));
        flow.key(key, &View { buy: &buy, sell: &sell, owned: &owned, munny })
    };
    let before = o.flow.page;
    match act {
        Act::None => {}
        Act::Moved => menu_sound::play(&mut se, menu_sound::CURSOR),
        Act::Refused => {
            menu_sound::play(&mut se, menu_sound::REFUSED);
            info!("shop: page {} key {key:?} refused (FUN_00265550 / FUN_00261a28)", before.number());
        }
        Act::Page => {
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
            menu_sound::play(&mut se, menu_sound::REFUSED);
            info!("shop: the third row (page 8, FUN_00285388 and after) is not ported; refused");
        }
        Act::Close => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            info!("shop: close (FUN_00263f00(1, 0, 0), FUN_00295ba0) from page {}", before.number());
            let leave = m.layd.layout(m.layouts[before.screen()][1]).to_vec();
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
            return;
        }
    }
    let screen = (before.screen(), flow.page.screen());
    if let Some(o) = m.open.as_mut() {
        o.flow = flow;
    }
    if screen.0 != screen.1 {
        set_layout(&m, &mut scene, screen.1);
    }
    let munny = progress.as_deref().map_or(0, |p| p.wallet());
    let owned = |id: u16| progress.as_deref().map_or(0, |p| p.item_count(id));
    redraw(&m, &mut scene, &owned, munny);
}

/// While the shop is up the field is frozen and the pad is the menu's (after `menu::input` wrote its own state).
fn hold(m: Res<ShopMenu>, mut pause: ResMut<Pause>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    if m.is_open() {
        pause.menu = true;
        keys.reset_all();
    }
}

fn tick(mut m: ResMut<ShopMenu>, mut scene: ResMut<MenuScene>, real: Res<Time<Real>>) {
    let Some(o) = m.open.as_ref() else { return };
    let ticks = scene.advance(real.delta_secs());
    if o.closing == 0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ware(id: u16, kind: u8, buy: u16, sell: u16) -> Ware {
        Ware { id, kind, name: 0, buy, sell }
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
}
