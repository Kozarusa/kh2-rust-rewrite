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
    /// +5: the rank (a synthesis material's points, `synth::points`).
    pub rank: u8,
}

impl Ware {
    pub fn of(i: &t::Item) -> Ware {
        Ware { id: i.id, kind: i.r#type, name: i.name, buy: i.shop_buy, sell: i.shop_sell, rank: i.rank }
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
    /// The layout triples: 0 top, 1 buy, 2 sell, 3 the synthesis pages (`synth::addr::LAYOUTS`).
    layouts: [[usize; 3]; 4],
    layd: Layd,
    seq_base: usize,
    tex: usize,
    nodes: Nodes,
    open: Option<Open>,
    /// The third row's data (`mixdata.bar`, the executable's tables, the material list); `None` when unreadable.
    mix: Option<(synth::Mix, synth::Tables, Vec<u16>)>,
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
    let tables = synth::Tables::read(&|a| exe.u16(a), &|a| exe.u8(a));
    m.mix = match (disc.read("menu/us/mixdata.bar").and_then(|b| synth::Mix::parse(&b)), tables) {
        (Some(mix), Some(t)) => {
            let mats: Vec<u16> = wares.iter().filter(|w| w.kind == synth::MATERIAL_TYPE).map(|w| w.id).take(crate::progress::MOOGLE_MATERIALS).collect();
            info!("shop: loaded mixdata.bar ({} recipes, {} levels, {} rewards), {} materials, synthesis rows {:x?}", mix.recipes.len(), mix.levels.len(), mix.conds, mats.len(), t.rows);
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
        Page::Top => {
            for (k, &r) in o.flow.rows.iter().enumerate() {
                let c = if k == o.flow.top { at::SELECTED } else { at::IDLE };
                set_label(scene, n.rows[k], Some(text_label(text, message(text, m.row_names[r]), c, 0, [at::ROWS[0], row_y(k)])));
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
    let book = synth::Book { mix, t, mats, rank: &rank };
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
        S::NotPorted => {
            menu_sound::play(se, menu_sound::REFUSED);
            info!("synth: row 5 (page 0xb, FUN_00286060) is not ported; refused");
        }
        S::Back => {
            menu_sound::play(se, menu_sound::CANCEL);
            info!("synth: back to the shop's top rows (page 9 -> page 0, FUN_0025fef0)");
            o.flow.page = Page::Top;
            set_layout(m, scene, 0);
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

/// The synthesis pages. The texts are labels the port places, like the shop pages' [inferred positions: the list and
/// window widgets (`FUN_00296b48`, `FUN_0029b600`, `FUN_00299878`) were not followed]; the detail page also lists the
/// recipe's lines (material, need, stock) [inferred: `FUN_002894f0` and `FUN_00289aa8`, which draw them, were not read].
fn draw_synth(m: &ShopMenu, scene: &mut MenuScene, text: &MenuText, owned: &dyn Fn(u16) -> u8, munny: u32, moogle: &Moogle) {
    let (Some(o), Some((mix, t, mats))) = (m.open.as_ref(), m.mix.as_ref()) else { return };
    let Some(s) = o.synth.as_ref() else { return };
    let rank = |id: u16| m.wares.iter().find(|w| w.id == id).map_or(0, |w| w.rank);
    let book = synth::Book { mix, t, mats, rank: &rank };
    let n = &m.nodes;
    for id in n.texts() {
        set_label(scene, id, None);
    }
    let name = |id: u16| -> Vec<Token> { m.wares.iter().find(|w| w.id == id).map(|w| message(text, u32::from(w.name))).unwrap_or_default().into_iter().filter(|t| *t != Token::End).collect() };
    let row_y = |k: usize| at::ROWS[1] + at::PITCH * k as f32;
    let row = |scene: &mut MenuScene, k: usize, tokens: Vec<Token>, c: [u8; 4], right: Option<String>| {
        if k < n.list.len() {
            set_label(scene, n.list[k], Some(text_label(text, tokens, c, 0, [at::ROWS[0], row_y(k)])));
            if let Some(r) = right {
                set_label(scene, n.prices[k], Some(text_label(text, literal(text, &r), c, 1, [at::PRICE_RIGHT, row_y(k)])));
            }
        }
    };
    let hand = |scene: &mut MenuScene, k: usize| {
        set_label(scene, n.hand, Some(text_label(text, literal(text, ">"), at::SELECTED, 0, [at::HAND[0], row_y(k)])));
    };
    let pick = |k: usize, cur: usize, open: bool| if !open { at::GREY } else if k == cur { at::SELECTED } else { at::IDLE };
    set_label(scene, n.munny, Some(text_label(text, literal(text, &format!("{munny}")), at::SELECTED, 1, at::MUNNY)));
    let title = |scene: &mut MenuScene, tokens: Vec<Token>| set_label(scene, n.title, Some(text_label(text, tokens, at::SELECTED, 0, at::TITLE)));
    let window = |scene: &mut MenuScene, tokens: Vec<Token>| set_label(scene, n.detail, Some(text_label(text, tokens, at::SELECTED, 0, [at::ROWS[0], 200.0])));
    // A window page (0x12, 0x13, 0x14, 0x16) shows its text alone [inferred: the window covers the page under it].
    match s.page {
        synth::Page::Top => {
            title(scene, message(text, u32::from(o.stock.message)));
            for (k, &r) in s.rows.iter().enumerate() {
                row(scene, k, message(text, u32::from(t.rows[usize::from(r)])), pick(k, s.top, true), None);
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
                let Some((r, odd)) = synth::recipe(&book, e) else { continue };
                let item = if odd { r.upgraded } else { r.result };
                row(scene, k - first, name(item), pick(k, s.list, synth::entry_open(&book, moogle, kind, e)), Some(format!("{}", owned(item))));
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
        synth::Page::Materials { totals } => {
            title(scene, message(text, if totals { synth::msg::COLLECTION } else { synth::msg::MATERIALS }));
            let first = s.mat.saturating_sub(LIST_ROWS - 1);
            for (k, &i) in s.slots.iter().enumerate().skip(first).take(LIST_ROWS) {
                let v = if totals { moogle.total_at(i) } else { moogle.stock_at(i) };
                row(scene, k - first, name(mats[i]), pick(k, s.mat, true), Some(format!("{v}")));
            }
            if !s.slots.is_empty() {
                hand(scene, s.mat - first);
            }
        }
        _ => {}
    }
    match s.page {
        synth::Page::Notice | synth::Page::Opening => window(scene, message(text, s.text)),
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
        m.open = Some(Open { event, flow: Flow::new(rows), stock, sell, closing: 0, synth: None });
        scene.hidden = false;
        scene.dim = crate::menu::dim_alpha(crate::menu::DIM_BRIGHTNESS);
        hide_all(&m, &mut scene);
        set_layout(&m, &mut scene, 0);
        let munny = progress.as_deref().map_or(0, |p| p.wallet());
        let owned = |id: u16| progress.as_deref().map_or(0, |p| p.item_count(id));
        redraw(&m, &mut scene, &owned, munny, &moogle_of(progress.as_deref()));
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
            let Some(p) = progress.as_deref_mut() else { return };
            let ShopMenu { mix, wares, open, .. } = &mut *m;
            let Some((mix, t, mats)) = mix.as_ref() else {
                menu_sound::play(&mut se, menu_sound::REFUSED);
                info!("shop: the third row has no mixdata.bar; refused");
                return;
            };
            let rank = |id: u16| wares.iter().find(|w| w.id == id).map_or(0, |w| w.rank);
            let book = synth::Book { mix, t, mats, rank: &rank };
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
        /// `cond`: the collection rewards (`FUN_00291140`; not ported).
        pub conds: usize,
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
            Some(Mix {
                recipes,
                levels,
                exp: [u16_at(exp, 0x10)?, u16_at(exp, 0x12)?, u16_at(exp, 0x14)?, u16_at(exp, 0x16)?],
                factor: f32::from_bits(u32_at(exp, 0x18)?),
                story_flag: u16_at(exp, 0x1c)?,
                story_bit: u16_at(exp, 0x1e)?,
                conds: u32_at(cond, 8)? as usize,
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
            Some(t)
        }

        /// `FUN_002910f0(rank, k)`.
        pub fn bonus_item(&self, rank: u8, k: usize) -> u16 {
            self.bonus.get(k).and_then(|b| b.get(rank as usize)).copied().unwrap_or(0)
        }
    }

    /// What the pages read: the data, the tables, the material list (`FUN_00284798`: the item ids of type 0x10 in
    /// table order) and an item's rank byte (item +5).
    pub struct Book<'a> {
        pub mix: &'a Mix,
        pub t: &'a Tables,
        pub mats: &'a [u16],
        pub rank: &'a dyn Fn(u16) -> u8,
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
    /// unlocked, learned and not made. Recipes (2): the learned recipes of kind 0 not made. At most 0x2d. [The
    /// entries flagged 0x80 that `FUN_002783f8` (story flag 0x4837) adds are left out.]
    pub fn entries(b: &Book, m: &Moogle, kind: u8) -> Vec<u16> {
        let n = b.mix.recipes.len() as u32;
        let mut v = Vec::new();
        if kind == 0 {
            v.extend((0..(2 * n).saturating_sub(1)).filter(|&e| Moogle::bit(m.made, e)).map(|e| e as u16));
        } else {
            for (i, r) in b.mix.recipes.iter().enumerate() {
                let e = 2 * i as u32;
                if !Moogle::bit(m.learned, e) || Moogle::bit(m.made, e) {
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
    }

    /// Pages 0x15 and on: `FUN_00288170` adds the item; `FUN_00290ef8` / `FUN_002909d0` take each line's need and one of
    /// every chosen bonus material from the stock and add the points (at most 99,999); `FUN_00288228` marks the entry
    /// made and seen (the higher rank's entry when the bonus made it); `FUN_00290c28` grants the bits of the levels
    /// reached, then the Free Development sets they open (`FUN_00290cc0`); `FUN_00290bb8` the level.
    pub fn make(b: &Book, s: &mut dyn Store, kind: u8, e: u16, flags: u8, item: u16) -> Option<Made> {
        let (r, odd) = recipe(b, e)?;
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
        grant_levels(b, m);
        Some(Made { item, points, used, entry, level: (before, m.level) })
    }

    /// `FUN_00290c28` + `FUN_00290bb8`: the level rows 1..8 whose points are reached grant their unlock bit; the level
    /// is the count of rows reached in order (at most 8).
    pub fn grant_levels(b: &Book, m: &mut Moogle) {
        for l in b.mix.levels.iter().skip(1).take(8) {
            if l.points > m.points {
                break;
            }
            if l.unlock >= 0 {
                Moogle::with(&mut m.unlocks, l.unlock as u32);
            }
        }
        learn_sets(b, m);
        m.level = b.mix.levels.iter().skip(1).take(8).take_while(|l| l.points <= m.points).count() as u8;
    }

    /// `FUN_00290cc0`: a Free Development recipe (set 1..3) is learned once its set's bit (0..2) is unlocked.
    pub fn learn_sets(b: &Book, m: &mut Moogle) {
        for (i, r) in b.mix.recipes.iter().enumerate() {
            if (1..=3).contains(&r.unlock) && m.unlocked(u32::from(r.unlock) - 1) {
                Moogle::with(&mut m.learned, 2 * i as u32);
            }
        }
    }

    /// The opening of the third row (`FUN_00284c10`).
    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct Opening {
        /// `FUN_002887c8`: (material, count) handed over.
        pub handed: Vec<(u16, u8)>,
        /// `FUN_002889e0`: the recipe items taken.
        pub recipes: Vec<u16>,
        /// The bits of the state's +0x130 (0 materials, 5 recipes; 1..4 the collection rewards, not ported).
        pub notices: u8,
        /// The first page: 10 after a hand-over, else 0x10 after recipes, else 9.
        pub page: Page,
    }

    /// `FUN_00284c10`: the story flag of `exp` grants its bit (10, the Recipes row); every material the player holds goes
    /// to the stock and the totals (at most 999 each; the first one sets its known bit); with bit 10, the recipe items
    /// held are handed over (`FUN_002888f8` / `FUN_002889e0`: a kind-0 recipe not learned yet is learned and its item
    /// taken; an item of the other kinds is only taken). Not ported: the collection rewards (`FUN_00291140`, `cond`).
    pub fn open(b: &Book, s: &mut dyn Store) -> Opening {
        let mut o = Opening { page: Page::Top, ..Opening::default() };
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
            Moogle::with(&mut m.known, i as u32);
            s.remove(id, n);
            o.handed.push((id, n));
        }
        if !o.handed.is_empty() {
            o.notices |= 1;
            o.page = Page::Materials { totals: true };
        }
        learn_sets(b, s.moogle_mut());
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
        pub item: u16,
        pub levels: Vec<u16>,
        pub last: Option<Made>,
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
        /// A row that is not ported (row 5 "Lists", page 0xb).
        NotPorted,
    }

    impl Session {
        /// The third row is confirmed: the opening, then its notices and its first page.
        pub fn start(b: &Book, s: &mut dyn Store) -> (Session, Opening) {
            let o = open(b, s);
            let mut x = Session { rows: top_rows(b, s.moogle()), ..Session::default() };
            x.queue = (0..6).filter(|k| o.notices >> k & 1 != 0).map(|k| u32::from(b.t.notices[k])).collect();
            x.goto(b, s.moogle(), o.page);
            if !x.queue.is_empty() {
                x.ret = x.page;
                x.text = x.queue.remove(0);
                x.page = Page::Opening;
            }
            (x, o)
        }

        fn goto(&mut self, b: &Book, m: &Moogle, p: Page) {
            match p {
                Page::Top => self.rows = top_rows(b, m),
                Page::List(k) => {
                    self.kind = k;
                    self.entries = entries(b, m, k);
                    self.list = self.list.min(self.entries.len().saturating_sub(1));
                }
                Page::Materials { totals } => {
                    self.slots = (0..b.mats.len().min(MOOGLE_MATERIALS)).filter(|&i| if totals { m.total_at(i) != 0 } else { m.stock_at(i) != 0 }).collect();
                    self.mat = 0;
                }
                _ => {}
            }
            self.page = p;
        }

        fn notice(&mut self, text: u32) {
            self.ret = self.page;
            self.text = text;
            self.page = Page::Notice;
        }

        /// The entry under the list cursor.
        pub fn entry(&self) -> Option<u16> {
            self.entries.get(self.list).copied()
        }

        pub fn key(&mut self, k: super::Key, b: &Book, s: &mut dyn Store) -> Act {
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
                        Some(5) => Act::NotPorted,
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
                        }
                        Act::Page
                    }
                    _ => Act::None,
                },
                Page::Result => match k {
                    Key::Confirm | Key::Cancel => {
                        let (Some(e), Page::Detail(kind)) = (self.entry(), self.ret) else { return Act::None };
                        self.last = make(b, s, kind, e, self.flags, self.item);
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
        Ware { id, kind, name: 0, buy, sell, rank: 0 }
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
            *self.items.entry(id).or_default() += n;
            n
        }
        fn remove(&mut self, id: u16, n: u8) {
            let c = self.items.entry(id).or_default();
            *c = c.saturating_sub(n);
        }
        fn story(&self, flag: u16) -> bool {
            self.flags.contains(&flag)
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
            conds: 0,
        };
        let t = Tables { rows: [0xbc8c, 0xbc8d, 0xbc8e, 0xc48e, 0xbc8b, 0xbcdd, 0xbc8f], bonus: [[0x210; 4], [0x211; 4], [0x212; 4]], rank_bits: [6, 7, 8, 9], help: [0; 5], notices: [0xbc89, 0, 0, 0, 0, 0xbc8a], level_names: [0, 1, 2, 3, 4, 5, 6, 7, 8], layouts: [8, 10, 9] };
        (mix, t, vec![0x200, 0x201, 0x210, 0x211, 0x212])
    }

    fn rank(id: u16) -> u8 {
        u8::from(id == 0x201)
    }

    #[test]
    fn the_synthesis_opening_hands_over_materials_and_recipes() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rank };
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
        assert_eq!(x.key(Key::Confirm, &b, &mut s), synth::Act::NotPorted, "row 5 (page 0xb)");
        assert_eq!(x.key(Key::Start, &b, &mut s), synth::Act::Close);
    }

    #[test]
    fn a_make_takes_the_lines_adds_the_item_the_points_and_the_level() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rank };
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

    #[test]
    fn the_bonus_rows_halve_and_raise_the_rank() {
        let (mix, t, mats) = mix();
        let b = Book { mix: &mix, t: &t, mats: &mats, rank: &rank };
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
        assert_eq!((mix.recipes.len(), mix.levels.len(), mix.conds), (23, 9, 50));
        assert_eq!((mix.story_bit, mix.factor), (10, 2.0), "the Recipes row's bit; the points bonus doubles");
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).unwrap().rows;
        let mats: Vec<u16> = items.iter().filter(|i| i.r#type == synth::MATERIAL_TYPE).map(|i| i.id).take(crate::progress::MOOGLE_MATERIALS).collect();
        let msgs = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").unwrap()).unwrap();
        let name = |id: u16| items.iter().find(|i| i.id == id).and_then(|i| msgs.text(u32::from(i.name))).unwrap_or_default();
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
}
