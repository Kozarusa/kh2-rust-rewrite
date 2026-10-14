//! The INFORMATION bar (WP-I1): the strip at the top left that shows a mission's objective ("Defeat Seifer!")
//! and then scrolls it away. See docs/hud.md "The INFORMATION bar".
//!
//! What the game does (`SLUS_210.05`):
//! - The mission start (`FUN_001b8ee8`) ends with `FUN_001b9a08(0)`: the header's +6 message id, when not 0, is
//!   looked up (`FUN_00190c40`) and handed to `FUN_0014f1e8`, which shows it on the bar object `DAT_0035852c`
//!   (made by `FUN_0014ef70`, class vtable 0x358668), or replaces the text when the bar is already up
//!   (`FUN_001505b0`).
//! - `FUN_00150428` builds it from the field file's `gaug` pair (`FUN_0013a998(1)`): the text object at +0x30
//!   (`FUN_00138fc8`: font 0, colour 0xe5e5e5 alpha 0x80, size the group record's +0x18 or 15 when that is 0)
//!   plays group 0 with group 1 as its follow-up (+0x1ac); a second object at +0x220 plays group 0 with group 2
//!   as its follow-up (+0x39c). Both are draw tag 7, priority 8000, so the field-HUD hide of an event (tags 3 to
//!   6) leaves them up. `FUN_00150510` plays system sound 0x22 (not played here: the system sound list is not
//!   wired, as for the item messages) and puts the text at the group record's x (+0x190 = `FUN_00237618`).
//! - The update `FUN_00150688`: when the follow-up has started (the object's +0x188, set by `FUN_00138aa8`),
//!   the text's scissor opens to the group's width (+0x20) and a wait of 180 (0x3792b0) runs down by the tick
//!   scale `DAT_00356f44`; then the text moves left by one unit per update until its x is below the start x
//!   minus the text width (`FUN_001a7ed8` measures it), and `FUN_001507d8` closes the bar: group 3 (the fade
//!   out), no follow-up, the +0x220 object hidden. A non-looping group with no follow-up ends its object
//!   (`FUN_00138aa8`, vtable +0x14).
//! - The text's place (`FUN_00139048`, the object flags 4, 8 and 0x10 set by `FUN_00138fc8`): the group's
//!   anchor plus the group record's x, y plus the stacking offset (+0x164, +0x168: `FUN_0014f8b0` row 1 of
//!   column 0, 0 while nothing stands in row 0) plus the scrolled x.
//!
//! [measured] from the reference clip 3.mp4: the text moves about 25 units a second (0.5 per 60 Hz tick: the
//! one unit per update reads as an update every other frame [inferred]); the 180 wait reads as 3 s at 60 ticks.
//! [inferred] the scissor's left edge (screen x 0) is the window's left edge: the bar is anchored to the left
//! edge (the user's widescreen rule), and glyphs left of it are dropped whole (the app has no text scissor).

use bevy::prelude::*;
use kh2_image::text::{Style, Token};

use super::super::hud_gfx::{Edge, HudScene, Node};
use super::super::hud_text::Label;

/// `gaug` is the second pair of the field file (`FUN_0013a998(1)`).
const GAUG: usize = 1;
/// The text object's groups (`FUN_00150428`, `FUN_001507d8`): open, idle (follow-up), close.
pub const OPEN: usize = 0;
pub const IDLE: usize = 1;
pub const CLOSE: usize = 3;
/// The second object's follow-up group (+0x39c = 2).
pub const SWEEP: usize = 2;
/// The wait before the text scrolls (0x3792b0, 180.0).
pub const WAIT: f32 = 180.0;
/// Text units the text moves per 60 Hz tick [measured, 3.mp4].
pub const SCROLL_PER_TICK: f32 = 0.5;
/// The text object's colour (`FUN_00138fc8`: 0x80e5e5e5) and the default size when the group has none.
const COLOR: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
const DEFAULT_SIZE: f32 = 15.0;
/// The system sound `FUN_00150510` plays (`FUN_002c14a8(0x22, 0)`).
pub const SE_SHOW: u32 = 0x22;

/// Where the bar is in its life.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Group 0 plays; the text is cut to nothing (the 1 x 1 scissor).
    Opening,
    /// The follow-up runs: the wait counts down (`left`), then the text scrolls (`x`).
    Shown { left: f32, x: f32 },
    /// Group 3 plays; no text.
    Closing,
    Gone,
}

/// One step of `FUN_00150688` for `ticks` 60 Hz ticks: `opened` once the follow-up group has started,
/// `start_x` the group record's x, `width` the text's width. Returns the new phase.
pub fn step(phase: Phase, opened: bool, start_x: f32, width: f32, ticks: f32) -> Phase {
    match phase {
        Phase::Opening if opened => step(Phase::Shown { left: WAIT, x: start_x }, true, start_x, width, ticks),
        Phase::Shown { left, x } => {
            let left_after = left - ticks;
            if left_after > 0.0 {
                return Phase::Shown { left: left_after, x };
            }
            let x = x - SCROLL_PER_TICK * (-left_after).min(ticks);
            if x < start_x - width { Phase::Closing } else { Phase::Shown { left: 0.0, x } }
        }
        p => p,
    }
}

#[derive(Default)]
pub struct InfoBar {
    /// The mission whose message was shown (one show per manager, `FUN_001b9a08(0)` at its start).
    mission: Option<u32>,
    /// The text object (+0x30) and the second object (+0x220).
    nodes: Option<[usize; 2]>,
    phase: Option<Phase>,
    tokens: Vec<Token>,
    message: u32,
    acc: f32,
}

impl InfoBar {
    pub fn nodes(&self) -> Vec<usize> {
        self.nodes.map(|n| n.to_vec()).unwrap_or_default()
    }
}

/// The header's +6 message id (`FUN_001b9a08`), 0 for none.
pub fn header_message(file: &[u8]) -> u32 {
    kh2_bar::parse(file).ok().and_then(|e| e.iter().find(|e| e.kind == 2).and_then(|e| e.data.get(6..8)).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32)).unwrap_or(0)
}

/// `KH2_INFO=<hex message id>`: shows the bar with that message once (a harness aid).
fn forced() -> Option<u32> {
    let v = std::env::var("KH2_INFO").ok()?;
    u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()
}

/// The INFORMATION bar's clock is the field's: it stands still while the camp menu or the PAUSE band is up (the original
/// stops the field's tasks by the scene change, docs/menus.md 1.1; the app's `schedule::Pause`).
pub fn step_dt(frozen: bool, dt: f32) -> f32 {
    if frozen {
        0.0
    } else {
        dt
    }
}

pub fn update(scene: &mut HudScene, mission: &crate::struggle::Mission, bar: &mut InfoBar, dt: f32) {
    if scene.files.len() <= GAUG {
        return;
    }
    // FUN_001b9a08(0) at the mission start.
    let want = match mission.m.as_deref() {
        Some(a) if bar.mission != Some(a.id) => {
            bar.mission = Some(a.id);
            Some(header_message(&a.file))
        }
        _ => None,
    };
    let want = want.or_else(|| (bar.mission.is_none() && bar.phase.is_none()).then(forced).flatten());
    if let Some(id) = want.filter(|id| *id != 0) {
        show(scene, bar, id);
    }
    let (Some([a, b]), Some(phase)) = (bar.nodes, bar.phase) else { return };
    bar.acc = (bar.acc + dt * 60.0).min(15.0);
    let ticks = bar.acc.floor();
    bar.acc -= ticks;
    let rec = |scene: &HudScene, g: usize| scene.files[GAUG].seq.groups.get(g).map_or((0.0, 0.0, 0.0), |r| (r.x as f32, r.y as f32, r.size as f32));
    let (start_x, _, size) = rec(scene, OPEN);
    let size = if size > 0.0 { size } else { DEFAULT_SIZE };
    let style = Style { font: 0, size: Some(size), color: COLOR, ..Style::default() };
    let width = scene.text.as_ref().map_or(0.0, |t| t.measure(&Label { tokens: bar.tokens.clone(), style: style.clone(), offset: [0.0; 2] })[0]);
    let opened = scene.nodes[a].player.group == Some(IDLE);
    let next = step(phase, opened, start_x, width, ticks);
    if next != phase {
        match next {
            Phase::Shown { .. } if !matches!(phase, Phase::Shown { .. }) => info!("hud: INFORMATION bar open: the text waits {WAIT} ticks, then scrolls"),
            Phase::Closing => {
                info!("hud: INFORMATION bar: message {:#x} scrolled out, closing (group {CLOSE}, FUN_001507d8)", bar.message);
                scene.set_group(a, CLOSE);
                scene.nodes[a].next = None;
                scene.nodes[b].visible = false;
            }
            _ => {}
        }
    }
    bar.phase = Some(next);
    if next == Phase::Closing && scene.nodes[a].player.finished() {
        // A non-looping group with no follow-up ends its object (FUN_00138aa8 -> vtable +0x14).
        scene.nodes[a].visible = false;
        scene.nodes[a].label = None;
        bar.phase = Some(Phase::Gone);
        info!("hud: INFORMATION bar gone");
        return;
    }
    // The text: shown only while the bar is open (the scissor), at anchor + group record (x, y) + scrolled x.
    let label = match next {
        Phase::Shown { x, .. } => {
            let n = &scene.nodes[a];
            let cur = n.player.group.unwrap_or(IDLE);
            let anchor = scene.files[GAUG].seq.anchor(cur, n.player.frame(), &n.ext).unwrap_or([0, 0]);
            let (gx, gy, _) = rec(scene, cur);
            let at = [anchor[0] as f32 + gx + x, anchor[1] as f32 + gy];
            // Glyphs left of the screen edge are dropped whole (the scissor's left edge is screen x 0).
            let edge = -scene.margin - at[0];
            let (tokens, shift) = trim_left(scene, &bar.tokens, &style, edge);
            Some(Label { tokens, style, offset: [at[0] + shift, at[1]] })
        }
        _ => None,
    };
    scene.nodes[a].label = label;
}

/// Drops the leading glyphs that start left of `edge` (text units from the text's origin); returns what is left
/// and how far its origin moved.
fn trim_left(scene: &HudScene, tokens: &[Token], style: &Style, edge: f32) -> (Vec<Token>, f32) {
    if edge <= 0.0 {
        return (tokens.to_vec(), 0.0);
    }
    let Some(t) = scene.text.as_ref() else { return (tokens.to_vec(), 0.0) };
    let w = |k: usize| t.measure(&Label { tokens: tokens[..k].to_vec(), style: style.clone(), offset: [0.0; 2] })[0];
    let k = (0..=tokens.len()).find(|&k| w(k) >= edge).unwrap_or(tokens.len());
    (tokens[k..].to_vec(), w(k))
}

/// `FUN_0014f1e8` -> `FUN_00150428` (or `FUN_001505b0` when the bar is up: the text is replaced and the groups
/// restart).
fn show(scene: &mut HudScene, bar: &mut InfoBar, id: u32) {
    let Some(tokens) = scene.text.as_ref().and_then(|t| t.lookup(id)) else {
        warn!("hud: INFORMATION bar: message {id:#x} not found");
        return;
    };
    let [a, b] = *bar.nodes.get_or_insert_with(|| {
        let mut add = || {
            let mut n = Node::new(GAUG, None);
            n.edge = Edge::Left;
            scene.add(n)
        };
        [add(), add()]
    });
    scene.set_group_then(a, OPEN, IDLE);
    scene.set_group_then(b, OPEN, SWEEP);
    for n in [a, b] {
        scene.nodes[n].visible = true;
    }
    scene.nodes[a].label = None;
    info!(
        "hud: INFORMATION bar: message {id:#x} {:?} (mission header +6, FUN_001b9a08 -> FUN_0014f1e8), gaug groups {OPEN} -> {IDLE} and {OPEN} -> {SWEEP} (system sound {SE_SHOW:#x}: not played, the system sound list is not wired)",
        kh2_image::text::to_text(&tokens)
    );
    bar.tokens = tokens;
    bar.message = id;
    bar.phase = Some(Phase::Opening);
    bar.acc = 0.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wait and the scroll step from `step_dt`: zero while the menu or the PAUSE band freezes the field.
    #[test]
    fn the_bar_stands_still_while_frozen() {
        assert_eq!(step_dt(true, 0.016), 0.0);
        assert_eq!(step_dt(false, 0.016), 0.016);
        // A frozen frame gives step() zero ticks: the wait does not advance.
        let p = Phase::Shown { left: WAIT, x: 10.0 };
        assert_eq!(step(p, true, 10.0, 100.0, 0.0), p);
    }

    /// `FUN_00150688`: nothing moves before the follow-up group starts; then 180 ticks of wait, then the text
    /// runs left until it is one text width left of its start, and the bar closes.
    #[test]
    fn the_information_text_waits_then_scrolls_out_and_the_bar_closes() {
        let mut p = Phase::Opening;
        for _ in 0..30 {
            p = step(p, false, 6.0, 80.0, 1.0);
        }
        assert_eq!(p, Phase::Opening);
        p = step(p, true, 6.0, 80.0, 1.0);
        assert_eq!(p, Phase::Shown { left: WAIT - 1.0, x: 6.0 });
        let mut ticks = 1;
        while matches!(p, Phase::Shown { left, .. } if left > 0.0) {
            p = step(p, true, 6.0, 80.0, 1.0);
            ticks += 1;
        }
        assert_eq!(ticks, WAIT as i32, "the wait is 180 ticks");
        let mut scroll = 0;
        while matches!(p, Phase::Shown { .. }) {
            p = step(p, true, 6.0, 80.0, 1.0);
            scroll += 1;
        }
        assert_eq!(p, Phase::Closing);
        // 80 units at half a unit per tick, plus the tick that passes the end.
        assert_eq!(scroll, 161);
    }

    #[test]
    fn the_seifer_missions_name_their_objective_in_the_header() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = crate::assets::Disc::open(&iso);
        // The lookup of 0x190c40: the system table, then the world's.
        let tables: Vec<_> = ["msg/us/sys.bar", "msg/us/tt.bar"].iter().filter_map(|f| disc.read(f).and_then(|b| kh2_image::text::Messages::from_bar(&b))).collect();
        let text = |id: u32| tables.iter().find_map(|m| m.text(id));
        for (name, id) in [("TT04_MS500", 0x48c1), ("TT04_MS108", 0x375a), ("TT03_MS400", 0x48bf), ("TT04_MS00", 0)] {
            let file = disc.read(&format!("msn/us/{name}.bar")).expect("mission file");
            assert_eq!(header_message(&file), id, "{name}");
            if id != 0 {
                assert_eq!(text(id).as_deref(), Some("Defeat Seifer!"), "{name}");
            }
        }
        // The bar's groups in the field file's gaug pair: 0 and 3 play once, 1 and 2 loop.
        let file = disc.read("field2d/us/tt0field.2dd").expect("tt0field.2dd");
        let entries = kh2_bar::parse(&file).unwrap();
        let s = kh2_image::seqd::parse(entries[3].data).expect("gaug sequence");
        let loops: Vec<u16> = [OPEN, IDLE, SWEEP, CLOSE].iter().map(|&g| s.groups[g].loops).collect();
        assert_eq!(loops, vec![1, 0, 0, 1]);
    }
}
