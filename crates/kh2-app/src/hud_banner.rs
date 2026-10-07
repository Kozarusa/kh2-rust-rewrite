//! The `fiel` text banners and the number popup: the "PUSH!", "CHANCE!", "COMBO!" and "FINISH!" button
//! prompts, the "NEXT LEVEL" and "OBTAINED MUNNY!" strips, the experience gain popup (0x15b198), and
//! the item messages of picked-up items (`gaug` group 0x4c, the queue of `FUN_00138498`).
//! The game state that raises them is not read here: `HudBanner` and `ExperiencePopups` are plain
//! resources for the owning module to fill. Which group belongs to which word was read from the
//! sprite rectangles each group draws (docs/hud.md); the position of a banner is not known.

use bevy::prelude::*;

use kh2_image::text::{Style, Token};

use super::hud_gfx::{HudScene, Node};
use super::hud_text::Label;

/// Scene file index of `fiel` (the first pair of the field file).
const FIEL: usize = 0;
/// Popup digits: group 6 + digit (table 0x358c50 holds 6 to 15), the prefix symbol is group 16.
const DIGIT_BASE: usize = 6;
const PREFIX: usize = 16;
/// The game caps the experience counter at 9,999,999, so a popup has at most 7 digits.
const DIGITS: usize = 7;
const POPUPS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerKind {
    Push,
    Chance,
    Combo,
    Finish,
    NextLevel,
    ObtainedMunny,
}

impl BannerKind {
    /// The groups a banner plays: one that opens it, then the idle one (inferred from the sprite
    /// rectangles: the idle groups draw the word's strip). `NextLevel` and `ObtainedMunny` are single strips.
    fn groups(self) -> (usize, Option<usize>) {
        match self {
            BannerKind::Chance => (57, Some(58)),
            BannerKind::Combo => (61, Some(62)),
            BannerKind::Push => (63, Some(64)),
            BannerKind::Finish => (65, Some(66)),
            BannerKind::NextLevel => (48, None),
            BannerKind::ObtainedMunny => (17, None),
        }
    }

    pub fn parse(s: &str) -> Option<BannerKind> {
        Some(match s {
            "push" => BannerKind::Push,
            "chance" => BannerKind::Chance,
            "combo" => BannerKind::Combo,
            "finish" => BannerKind::Finish,
            "nextlevel" => BannerKind::NextLevel,
            "munny" => BannerKind::ObtainedMunny,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Banner {
    pub kind: BannerKind,
    /// Screen position in HUD units (512x448).
    pub pos: [f32; 2],
}

/// The banner on screen, if any. `KH2_HUDBANNER=<push|chance|combo|finish|nextlevel|munny>[:x:y]` shows one.
#[derive(Resource, Default, Clone, Debug)]
pub struct HudBanner(pub Option<Banner>);

/// An experience gain popup (0x15b198 shows the value `FUN_001b51e0` adds to the experience total, called
/// from `FUN_001b25d0` and `FUN_001c46a8`): the amount at a screen position (the game projects the
/// defeated enemy's world position). `KH2_HUDEXP=<amount>[:x:y]` (older name `KH2_HUDMUNNY`) shows one.
#[derive(Clone, Copy, Debug)]
pub struct ExperiencePopup {
    pub amount: u32,
    pub pos: [f32; 2],
}

#[derive(Resource, Default, Clone, Debug)]
pub struct ExperiencePopups(pub Vec<ExperiencePopup>);

#[derive(Resource, Default)]
pub struct BannerNodes {
    built: bool,
    banner: usize,
    shown: Option<BannerKind>,
    /// Per popup: root (the prefix) and its digit nodes.
    popups: [(usize, [usize; DIGITS]); POPUPS],
    drawn: [Option<u32>; POPUPS],
}

fn build(scene: &mut HudScene, n: &mut BannerNodes) {
    let add = |scene: &mut HudScene, parent: Option<usize>| {
        let mut node = Node::new(FIEL, parent);
        node.visible = false;
        scene.add(node)
    };
    n.banner = add(scene, None);
    for p in 0..POPUPS {
        let root = add(scene, None);
        let digits = std::array::from_fn(|_| add(scene, Some(root)));
        n.popups[p] = (root, digits);
    }
    n.built = true;
}

/// Width the game advances for a group: its record's `unk20` field (0x237698).
fn advance(scene: &HudScene, group: usize) -> f32 {
    scene.files.get(FIEL).and_then(|f| f.seq.groups.get(group)).map(|g| g.unk20 as f32).unwrap_or(0.0)
}

pub fn update(mut scene: ResMut<HudScene>, mut banner: ResMut<HudBanner>, mut popups: ResMut<ExperiencePopups>, mut nodes: ResMut<BannerNodes>, mut demo: Local<bool>) {
    if scene.files.is_empty() {
        return;
    }
    if !*demo {
        *demo = true;
        if let Ok(v) = std::env::var("KH2_HUDBANNER") {
            let p: Vec<&str> = v.split(':').collect();
            if let Some(kind) = p.first().and_then(|k| BannerKind::parse(k)) {
                let n = |i: usize, d: f32| p.get(i).and_then(|x| x.parse().ok()).unwrap_or(d);
                banner.0 = Some(Banner { kind, pos: [n(1, 256.0), n(2, 120.0)] });
            }
        }
        if let Ok(v) = std::env::var("KH2_HUDEXP").or_else(|_| std::env::var("KH2_HUDMUNNY")) {
            let p: Vec<&str> = v.split(':').collect();
            if let Some(amount) = p.first().and_then(|a| a.parse().ok()) {
                let n = |i: usize, d: f32| p.get(i).and_then(|x| x.parse().ok()).unwrap_or(d);
                popups.0.push(ExperiencePopup { amount, pos: [n(1, 256.0), n(2, 200.0)] });
            }
        }
    }
    if !nodes.built {
        build(&mut scene, &mut nodes);
        // `KH2_HUDFREEZE=<frame>` holds the banner and popup sequences at that frame (for screenshots).
        if let Some(f) = std::env::var("KH2_HUDFREEZE").ok().and_then(|v| v.parse::<f32>().ok()) {
            let ids: Vec<usize> = std::iter::once(nodes.banner).chain(nodes.popups.iter().flat_map(|(r, d)| std::iter::once(*r).chain(d.iter().copied()))).collect();
            for id in ids {
                scene.nodes[id].freeze = Some(f);
            }
        }
    }
    // Banner: the opening group, then the idle one.
    let b = nodes.banner;
    match banner.0 {
        None => {
            scene.nodes[b].visible = false;
            nodes.shown = None;
        }
        Some(Banner { kind, pos }) => {
            let (open, idle) = kind.groups();
            if nodes.shown != Some(kind) {
                nodes.shown = Some(kind);
                scene.set_group(b, open);
            } else if let Some(idle) = idle.filter(|_| scene.nodes[b].player.group == Some(open) && scene.nodes[b].player.finished()) {
                scene.set_group(b, idle);
            }
            scene.nodes[b].pos = pos;
            scene.nodes[b].visible = true;
        }
    }
    // Experience popups: the prefix symbol with the digits laid out from the groups' advance widths and
    // centred on the position (0x15ae80).
    for i in 0..POPUPS {
        let (root, digits) = nodes.popups[i];
        let Some(p) = popups.0.get(i).copied() else {
            scene.nodes[root].visible = false;
            for d in digits {
                scene.nodes[d].visible = false;
            }
            nodes.drawn[i] = None;
            continue;
        };
        let text = p.amount.min(9_999_999).to_string();
        if nodes.drawn[i] != Some(p.amount) {
            nodes.drawn[i] = Some(p.amount);
            scene.set_group(root, PREFIX);
            for (k, d) in digits.iter().enumerate() {
                if let Some(c) = text.as_bytes().get(k) {
                    scene.set_group(*d, DIGIT_BASE + (c - b'0') as usize);
                }
            }
        }
        let widths: Vec<f32> = text.bytes().map(|c| advance(&scene, DIGIT_BASE + (c - b'0') as usize)).collect();
        let prefix_w = advance(&scene, PREFIX);
        let half = ((widths.iter().sum::<f32>() + prefix_w) / 2.0).floor();
        scene.nodes[root].pos = p.pos;
        scene.nodes[root].off = [half - prefix_w, 0.0];
        scene.nodes[root].visible = true;
        let mut x = 0.0;
        for (k, d) in digits.iter().enumerate() {
            scene.nodes[*d].visible = k < widths.len();
            if k < widths.len() {
                scene.nodes[*d].off = [x - half, 0.0];
                x += widths[k];
            }
        }
    }
}

/// The two strip-with-number objects of `fiel` (WP-4B3). Each is a strip group with six digit
/// players (0x151970 and 0x151d48: `FUN_00236650` x 6, then 0x1518c8 / 0x151ca0 give digit `k` the
/// group `table[digit]` and the running sum of the groups' `unk20` widths). The game formats the
/// value with `%d` (0x15b010, format at 0x379430) and keeps at most six digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripKind {
    /// Group 17 with digit groups 28 to 37 (table 0x3586e8), raised through 0x14f2d8 (kind 0x11 in
    /// the popup registry 0x14f8b0) by 0x1a39b0, which is the prize pickup (WP-7B, docs/prizes.md):
    /// this is the "OBTAINED MUNNY!" strip with the munny picked up.
    Munny,
    /// Group 48 with digit groups 38 to 47 (table 0x358730), raised when the number popup is
    /// created (0x15b198 calls 0x14f2b8 with its third argument; kind 0xe).
    PopupTotal,
}

impl StripKind {
    fn groups(self) -> (usize, usize) {
        match self {
            StripKind::Munny => (17, 28),
            StripKind::PopupTotal => (48, 38),
        }
    }

    pub fn parse(s: &str) -> Option<StripKind> {
        Some(match s {
            "munny" | "levelup" => StripKind::Munny,
            "popup" => StripKind::PopupTotal,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NumberStrip {
    pub kind: StripKind,
    pub value: u32,
    /// Screen position in HUD units; the game's placement (0x14f8b0 registers the object with the
    /// group's own x from 0x237678) was not followed, so the owner gives it.
    pub pos: [f32; 2],
    /// Raised by a munny pickup (`crate::prize::MunnyGained`): later pickups add to it while it
    /// shows, and it goes when its group's animation has ended.
    pub pickup: bool,
}

/// Where a pickup's munny strip is put: the sequence's own placement [inferred: the game's position
/// was not followed, see `NumberStrip::pos`].
const PICKUP_STRIP_POS: [f32; 2] = [0.0, 0.0];

/// Strips on screen. `KH2_HUDSTRIP=<munny|popup>:<value>[:x:y]` shows one.
#[derive(Resource, Default, Clone, Debug)]
pub struct NumberStrips(pub Vec<NumberStrip>);

const STRIP_DIGITS: usize = 6;
const STRIPS: usize = 2;

#[derive(Resource, Default)]
pub struct StripNodes {
    built: bool,
    strips: [(usize, [usize; STRIP_DIGITS]); STRIPS],
    drawn: [Option<(StripKind, u32)>; STRIPS],
}

/// The strip and its digits: the digits run to the right of the strip's origin, each advanced by
/// its group's `unk20`. Where the digits sit relative to the strip is inferred: the sums 0x1518c8
/// stores at +0xad0 are offsets, and the draw that applies them was not read.
pub fn update_strips(
    mut scene: ResMut<HudScene>,
    mut strips: ResMut<NumberStrips>,
    mut nodes: ResMut<StripNodes>,
    mut demo: Local<bool>,
    mut munny: MessageReader<crate::prize::MunnyGained>,
    (mut items, mut obtained): (Local<ItemMessages>, MessageReader<crate::prize::ItemObtained>),
) {
    if scene.files.is_empty() {
        return;
    }
    item_messages(&mut scene, &mut items, &mut obtained);
    if !*demo {
        *demo = true;
        if let Ok(v) = std::env::var("KH2_HUDSTRIP") {
            let p: Vec<&str> = v.split(':').collect();
            if let (Some(kind), Some(value)) = (p.first().and_then(|k| StripKind::parse(k)), p.get(1).and_then(|a| a.parse().ok())) {
                let n = |i: usize, d: f32| p.get(i).and_then(|x| x.parse().ok()).unwrap_or(d);
                strips.0.push(NumberStrip { kind, value, pos: [n(2, 256.0), n(3, 300.0)], pickup: false });
            }
        }
    }
    // A munny pickup (0x1a39b0 -> 0x14f2d8 -> 0x151970): while the strip is up (0x151c80) the amount
    // is added to the one shown and group 0x11 starts again (0x151c40); otherwise it opens with
    // the amount.
    for m in munny.read() {
        match strips.0.iter().position(|s| s.pickup) {
            Some(i) => {
                strips.0[i].value = strips.0[i].value.saturating_add(m.amount);
                if i < STRIPS {
                    nodes.drawn[i] = None;
                }
            }
            None => strips.0.push(NumberStrip { kind: StripKind::Munny, value: m.amount, pos: PICKUP_STRIP_POS, pickup: true }),
        }
    }
    // A pickup strip goes once its group has played out [inferred: when the game's popup registry
    // drops it was not followed].
    if nodes.built {
        let done: Vec<usize> = (0..STRIPS.min(strips.0.len()))
            .filter(|&i| strips.0[i].pickup && nodes.drawn[i].is_some() && scene.nodes[nodes.strips[i].0].player.finished())
            .collect();
        if !done.is_empty() {
            for i in done.into_iter().rev() {
                strips.0.remove(i);
            }
            // The strips after a removed one move up a slot: draw them all again.
            nodes.drawn = [None; STRIPS];
        }
    }
    if !nodes.built {
        let add = |scene: &mut HudScene, parent: Option<usize>| {
            let mut node = Node::new(FIEL, parent);
            node.visible = false;
            scene.add(node)
        };
        for s in 0..STRIPS {
            let root = add(&mut scene, None);
            let digits = std::array::from_fn(|_| add(&mut scene, Some(root)));
            nodes.strips[s] = (root, digits);
        }
        nodes.built = true;
    }
    for i in 0..STRIPS {
        let (root, digits) = nodes.strips[i];
        let Some(s) = strips.0.get(i).copied() else {
            scene.nodes[root].visible = false;
            for d in digits {
                scene.nodes[d].visible = false;
            }
            nodes.drawn[i] = None;
            continue;
        };
        let (strip, base) = s.kind.groups();
        let text: String = s.value.to_string().chars().take(STRIP_DIGITS).collect();
        if nodes.drawn[i] != Some((s.kind, s.value)) {
            nodes.drawn[i] = Some((s.kind, s.value));
            scene.set_group(root, strip);
            for (k, d) in digits.iter().enumerate() {
                if let Some(c) = text.as_bytes().get(k) {
                    scene.set_group(*d, base + (c - b'0') as usize);
                }
            }
        }
        scene.nodes[root].pos = s.pos;
        scene.nodes[root].visible = true;
        let mut x = 0.0;
        for (k, d) in digits.iter().enumerate() {
            let shown = k < text.len();
            scene.nodes[*d].visible = shown;
            if shown {
                scene.nodes[*d].off = [x, 0.0];
                x += advance(&scene, base + (text.as_bytes()[k] - b'0') as usize);
            }
        }
    }
}

/// Scene file of `gaug` (the second pair of the field file: `FUN_0013a998(1)`).
const GAUG: usize = 1;
/// The item message's group of `gaug` (`FUN_001379c0` -> `FUN_00137648(0, entry, 0x4c, text, -1)`).
pub const ITEM_MSG_GROUP: usize = 0x4c;
/// The queue of `FUN_00138498`: 16 entries of 0x1d0 bytes at `DAT_00354ecc` + 0x28cc; a message is taken
/// while fewer than 15 are in it (the one shown counts until it ends).
pub const ITEM_QUEUE: usize = 16;
/// The system sound the entry's update plays when it starts (`FUN_001376e0`: `FUN_002c14a8(10, 0)`).
pub const ITEM_MSG_SE: u32 = 10;
/// The entry's text: font 0 (+0x1a0), size 0x10 (+0x19c, `FUN_00137648`), colour 0xe5e5e5 (+0x198,
/// `FUN_00138fc8`) with the group's alpha (flags 4 and 8, `FUN_00139048`) [the group alpha is taken as
/// 0x80], left aligned (+0x1a4 = 0).
pub const ITEM_MSG_STYLE: (usize, f32, [u8; 4]) = (0, 16.0, [0xe5, 0xe5, 0xe5, 0x80]);

/// The item messages (`FUN_00138498` queue, shown by the manager `DAT_00354ecc`'s update
/// `FUN_00137de0`): one at a time, in order; the shown one plays `gaug` group 0x4c once with the text
/// (an icon, then the name) at the group's text point (the group record's x, y: `FUN_00237618`,
/// `FUN_00237638`), and leaves when the group has played (`FUN_00138aa8`: a group that loops forever
/// never ends by itself).
#[derive(Default, Debug)]
pub struct ItemMessages {
    pub queue: std::collections::VecDeque<crate::prize::ItemObtained>,
    node: Option<usize>,
    shown: bool,
    /// The group's anchor point last computed (the player's +0xe0, +0xe4).
    anchor: [i32; 2],
}

impl ItemMessages {
    /// `FUN_00138498`: true when the message was taken.
    pub fn offer(&mut self, m: crate::prize::ItemObtained) -> bool {
        if self.queue.len() < ITEM_QUEUE - 1 {
            self.queue.push_back(m);
            true
        } else {
            false
        }
    }
}

/// The tokens of an item message (`FUN_001a1ee0`: byte 9 = an icon with the item's `Icon2`, then the
/// item's name message).
pub fn item_message_tokens(m: &crate::prize::ItemObtained, name: Option<Vec<Token>>) -> Vec<Token> {
    std::iter::once(Token::Icon(m.icon)).chain(name.unwrap_or_default()).collect()
}

fn item_messages(scene: &mut HudScene, q: &mut ItemMessages, incoming: &mut MessageReader<crate::prize::ItemObtained>) {
    for m in incoming.read() {
        if !q.offer(*m) {
            info!("hud: item message {:#x} refused: the queue is full", m.item);
        }
    }
    let node = *q.node.get_or_insert_with(|| {
        let mut n = Node::new(GAUG, None);
        n.visible = false;
        scene.add(n)
    });
    if q.shown && scene.nodes[node].player.finished() {
        q.queue.pop_front();
        q.shown = false;
        scene.nodes[node].visible = false;
        scene.nodes[node].label = None;
    }
    if q.shown {
        place_item_text(scene, q, node);
        return;
    }
    let Some(m) = q.queue.front().copied() else { return };
    let name = scene.text.as_ref().and_then(|t| t.lookup(m.name as u32));
    let tokens = item_message_tokens(&m, name);
    let group = scene.files.get(GAUG).and_then(|f| f.seq.groups.get(ITEM_MSG_GROUP)).map(|g| (g.x as f32, g.y as f32, g.loops));
    info!(
        "hud: item message {:#x} shown: {:?}, gaug group {ITEM_MSG_GROUP:#x} {group:?} (system sound {ITEM_MSG_SE}: not played, the system sound list is not wired), {} waiting",
        m.item,
        kh2_image::text::to_text(&tokens),
        q.queue.len() - 1
    );
    let (font, size, color) = ITEM_MSG_STYLE;
    scene.set_group(node, ITEM_MSG_GROUP);
    let n = &mut scene.nodes[node];
    n.label = Some(Label { tokens, style: Style { font, size: Some(size), color, ..Style::default() }, offset: [group.map_or(0.0, |g| g.0), group.map_or(0.0, |g| g.1)] });
    n.label_time = 0.0;
    n.visible = true;
    q.shown = true;
    q.anchor = [0, 0];
    place_item_text(scene, q, node);
}

/// The text point of the shown entry (`FUN_00139048`): the player's place (`FUN_002375c0` -> `FUN_00236cf8`,
/// the group's anchor animation at the current frame, `Seqd::anchor`; kept from the last frame one covered)
/// plus the group record's x, y.
fn place_item_text(scene: &mut HudScene, q: &mut ItemMessages, node: usize) {
    let Some(seq) = scene.files.get(GAUG).map(|f| &f.seq) else { return };
    let n = &scene.nodes[node];
    if let Some(a) = seq.anchor(ITEM_MSG_GROUP, n.player.frame(), &n.ext) {
        q.anchor = a;
    }
    let (x, y) = seq.groups.get(ITEM_MSG_GROUP).map_or((0, 0), |g| (g.x, g.y));
    let off = [(q.anchor[0] + x) as f32, (q.anchor[1] + y) as f32];
    if let Some(l) = scene.nodes[node].label.as_mut() {
        l.offset = off;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `FUN_00138498` takes 15 messages (the shown one included) and refuses the 16th; the message is
    /// the item's icon, then its name.
    #[test]
    fn the_item_message_queue_takes_fifteen() {
        let mut q = ItemMessages::default();
        let m = |item: u16| crate::prize::ItemObtained { item, icon: 3, name: 0x1234 };
        for i in 0..15 {
            assert!(q.offer(m(i)), "message {i}");
        }
        assert!(!q.offer(m(99)), "the 16th waits for nothing: refused");
        assert_eq!(q.queue.front().map(|x| x.item), Some(0));
        let t = item_message_tokens(&m(1), Some(vec![Token::Icon(7)]));
        assert_eq!(t, vec![Token::Icon(3), Token::Icon(7)]);
    }

    #[test]
    fn strip_kinds_use_their_own_digit_tables() {
        assert_eq!(StripKind::Munny.groups(), (17, 28));
        assert_eq!(StripKind::PopupTotal.groups(), (48, 38));
        assert_eq!(StripKind::parse("popup"), Some(StripKind::PopupTotal));
        assert_eq!(StripKind::parse("x"), None);
    }

    #[test]
    fn banner_names_parse() {
        assert_eq!(BannerKind::parse("combo"), Some(BannerKind::Combo));
        assert_eq!(BannerKind::parse("munny"), Some(BannerKind::ObtainedMunny));
        assert_eq!(BannerKind::parse("x"), None);
    }

    #[test]
    fn each_text_banner_opens_then_idles() {
        for k in [BannerKind::Push, BannerKind::Chance, BannerKind::Combo, BannerKind::Finish] {
            let (open, idle) = k.groups();
            assert_eq!(idle, Some(open + 1));
        }
        assert_eq!(BannerKind::NextLevel.groups().1, None);
    }
}
