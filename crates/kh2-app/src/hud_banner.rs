//! The `fiel` text banners and the number popup: the "PUSH!", "CHANCE!", "COMBO!" and "FINISH!" button
//! prompts, the "NEXT LEVEL" and "OBTAINED MUNNY!" strips, and the munny gain popup (0x15b198).
//! The game state that raises them is not read here: `HudBanner` and `MunnyPopups` are plain
//! resources for the owning module to fill. Which group belongs to which word was read from the
//! sprite rectangles each group draws (docs/hud.md); the position of a banner is not known.

use bevy::prelude::*;

use super::hud_gfx::{HudScene, Node};

/// Scene file index of `fiel` (the first pair of the field file).
const FIEL: usize = 0;
/// Munny digits: group 6 + digit (table 0x358c50 holds 6 to 15), the prefix symbol is group 16.
const DIGIT_BASE: usize = 6;
const PREFIX: usize = 16;
/// The game caps the munny counter at 9,999,999, so a popup has at most 7 digits.
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

/// A munny gain popup: the amount at a screen position (the game projects the defeated enemy's world
/// position). `KH2_HUDMUNNY=<amount>[:x:y]` shows one.
#[derive(Clone, Copy, Debug)]
pub struct MunnyPopup {
    pub amount: u32,
    pub pos: [f32; 2],
}

#[derive(Resource, Default, Clone, Debug)]
pub struct MunnyPopups(pub Vec<MunnyPopup>);

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

pub fn update(mut scene: ResMut<HudScene>, mut banner: ResMut<HudBanner>, mut popups: ResMut<MunnyPopups>, mut nodes: ResMut<BannerNodes>, mut demo: Local<bool>) {
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
        if let Ok(v) = std::env::var("KH2_HUDMUNNY") {
            let p: Vec<&str> = v.split(':').collect();
            if let Some(amount) = p.first().and_then(|a| a.parse().ok()) {
                let n = |i: usize, d: f32| p.get(i).and_then(|x| x.parse().ok()).unwrap_or(d);
                popups.0.push(MunnyPopup { amount, pos: [n(1, 256.0), n(2, 200.0)] });
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
    // Munny popups: the prefix symbol with the digits laid out from the groups' advance widths and
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
    /// Group 17 with digit groups 28 to 37 (table 0x3586e8), raised by the level-up routine
    /// 0x1a39b0 through 0x14f2d8 (kind 0x11 in the popup registry 0x14f8b0).
    LevelUp,
    /// Group 48 with digit groups 38 to 47 (table 0x358730), raised when the number popup is
    /// created (0x15b198 calls 0x14f2b8 with its third argument; kind 0xe).
    PopupTotal,
}

impl StripKind {
    fn groups(self) -> (usize, usize) {
        match self {
            StripKind::LevelUp => (17, 28),
            StripKind::PopupTotal => (48, 38),
        }
    }

    pub fn parse(s: &str) -> Option<StripKind> {
        Some(match s {
            "levelup" => StripKind::LevelUp,
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
}

/// Strips on screen. `KH2_HUDSTRIP=<levelup|popup>:<value>[:x:y]` shows one.
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
pub fn update_strips(mut scene: ResMut<HudScene>, mut strips: ResMut<NumberStrips>, mut nodes: ResMut<StripNodes>, mut demo: Local<bool>) {
    if scene.files.is_empty() {
        return;
    }
    if !*demo {
        *demo = true;
        if let Ok(v) = std::env::var("KH2_HUDSTRIP") {
            let p: Vec<&str> = v.split(':').collect();
            if let (Some(kind), Some(value)) = (p.first().and_then(|k| StripKind::parse(k)), p.get(1).and_then(|a| a.parse().ok())) {
                let n = |i: usize, d: f32| p.get(i).and_then(|x| x.parse().ok()).unwrap_or(d);
                strips.0.push(NumberStrip { kind, value, pos: [n(2, 256.0), n(3, 300.0)] });
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_kinds_use_their_own_digit_tables() {
        assert_eq!(StripKind::LevelUp.groups(), (17, 28));
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
