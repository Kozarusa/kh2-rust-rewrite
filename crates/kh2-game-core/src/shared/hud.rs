//! The HUD data the lower crates write: the gauges, the lock-on marker, the party panel, the command menu, the
//! script window and the text label (moved out of `hud.rs` and its children, which re-export them; the HUD's
//! `PartyMember` is `HudPartyMember` here).

use bevy::prelude::*;
use kh2_image::text::{Style, Token};

/// Developer overlay switch.
#[derive(Resource)]
pub struct DebugHud(pub bool);

#[derive(Message, Clone, Copy)]
pub struct DamagePopup {
    pub at: Vec3,
    pub amount: i32,
}

/// Sora's MP recharge state (player +0x1bc progress and +0x1c0 total, floats). The MP bar switches to its
/// recharge look while `total > 0`: a full-size back bar and a fill that grows with `progress / total`
/// (0x157628, 0x1556a0). The player module fills it in; `KH2_HUDMPRECHARGE=<progress>:<total>` shows it.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct MpRecharge {
    pub progress: f32,
    pub total: f32,
}

/// Which art the drive gauge uses (the player's mode byte at +0x1af in the game: 0 normal, 2 and 3 the
/// two form variants, 4 locked behind chains).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DriveMode {
    #[default]
    Normal,
    FormA,
    FormB,
    Locked,
}

/// Game-state input of the drive gauge. The player module fills it in; `KH2_HUDDRIVE=<points>:<max>:<fraction>[:mode]`
/// (mode 0 normal, 2, 3 or 4) shows it by hand. `enabled` is false until the player has a drive gauge.
#[derive(Resource, Clone, Debug)]
pub struct DriveGauge {
    pub enabled: bool,
    pub mode: DriveMode,
    /// Whole drive points (player +0x1b1) and the most the gauge holds (+0x1b2).
    pub points: u8,
    pub max: u8,
    /// Progress to the next point, 0 to 99 (+0x1b0).
    pub fraction: u8,
    /// Modes 2 and 3: the fill inside the current segment, 0.0 to 1.0 (`FUN_00155bf0`: the timer's
    /// remainder over the segment size, full on the last segment of a full timer). `None` uses
    /// `fraction`. The summon module fills it in mode 3.
    pub time_fill: Option<f32>,
}

impl Default for DriveGauge {
    fn default() -> DriveGauge {
        DriveGauge { enabled: false, mode: DriveMode::Normal, points: 0, max: 5, fraction: 0, time_fill: None }
    }
}

/// Magic lock-on marker groups (0x19bbc0 hands them to 0x13e528): mode 1 uses 5 when the target record's
/// shape byte is 0 and 1 otherwise, mode 2 uses 0x1b and 0x17.
pub fn marker_group(mode: u8, shape: u8) -> Option<usize> {
    match (mode, shape != 0) {
        (1, false) => Some(5),
        (1, true) => Some(1),
        (2, false) => Some(0x1b),
        (2, true) => Some(0x17),
        _ => None,
    }
}

/// Game-state input of the magic lock-on marker: the lock-on controller (player +0xb40) is in `mode` 1 or 2 on a
/// target record whose shape byte is `shape`, at world point `at` (the record's target point, FUN_0018efa0). The magic module inserts it
/// while the controller holds a record and removes it when the controller is released (FUN_0019b580);
/// `KH2_HUDMARKER=<mode>:<shape>` draws one at the free-aim point.
#[derive(Resource, Clone, Copy)]
pub struct MagicMarker {
    pub mode: u8,
    pub shape: u8,
    pub at: Vec3,
    /// The bubble flag of 0x19bbc0 (same rule as `TargetBubble`).
    pub alert: bool,
}

/// One companion as the panel reads it: the two values of +0x48c (HP, maximum), +0x180 and +0x184
/// (MP, maximum), and the scene file of the member's `face` pair when it was loaded.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HudPartyMember {
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
    pub face: Option<usize>,
}

/// The party, in slot order (filled by `party::party_hud`); empty draws nothing.
/// `KH2_HUDPARTY=<hp>:<max>:<mp>:<mpmax>[;...]` fills it instead, for a look at the geometry.
#[derive(Resource, Default, Clone, Debug)]
pub struct PartyMembers(pub Vec<HudPartyMember>);

/// What the summon asks of the party panel (WP-SUM8, docs/systems/drive-limits-summons/summons-stages.md "The summon face panel"). While a summon
/// object holds party slot 0 and the gauge is in mode 3, the party panels give way to one panel built like a party
/// panel: sequence group 154 (once), then 155 (looping), with the summon's own `face` picture in it; when the
/// gauge leaves mode 3 group 156 plays out (FUN_00156fe8 / FUN_00157020 -> FUN_00156e30). `summon.rs` fills it.
#[derive(Resource, Default, Clone, Debug, PartialEq, Eq)]
pub struct SummonFace {
    /// The model of the summon object in party slot 0 (`obj/<model>.apdx` holds its face); `None` for none.
    pub model: Option<String>,
}

/// A command shown in a list.
#[derive(Clone, Debug, Default)]
pub struct MenuEntry {
    /// The command's message id (`cmd.MessageId`; bit 15 is ignored).
    pub message: u32,
    /// `cmd.CmdIcon`: picks the icon on the selected bar (0 none, 1 to 7 as the game's switch at 0x13bd48).
    pub icon: u8,
    /// Text colour style, 0 to 8: the entry's status byte (entry +5, list +0x11 for the first entry):
    /// 0 usable, 1 a sub-menu with nothing usable, 2 a spell short of MP (orange), 5 unavailable,
    /// 6 an item only reserved (docs/systems/ui/items.md).
    pub style: u8,
}

/// One open list.
#[derive(Clone, Debug, Default)]
pub struct MenuPage {
    /// Which menu: the command list's index (0 and 6 the decks, 1 magic, 2 items, 3 forms, 4 the targets,
    /// 5 shortcuts, 7 summons, 8 friends, 9 limits; the art of each is listed in docs/systems/ui/contract.md).
    pub kind: u8,
    pub entries: Vec<MenuEntry>,
    pub cursor: usize,
}

/// The reaction command offered above the list.
#[derive(Clone, Debug, Default)]
pub struct Reaction {
    pub message: u32,
    /// The command's `Flags` bit 3 (picks the capsule's alternate art).
    pub alt: bool,
    /// The list entry type 7 (a second alternate).
    pub kind7: bool,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandMenu {
    pub visible: bool,
    pub pages: Vec<MenuPage>,
    pub reaction: Option<Reaction>,
}

/// A window game code opens by message id (the talk's `FUN_001a5a60`): the window looks the id up
/// (0x190c40), shows its pages and closes on the button; `closed` is set when it reached state 3
/// (what the opener waits for).
#[derive(Resource, Default, Debug)]
pub struct ScriptWindow {
    /// Open this message in this class (index into `CLASSES`).
    pub open: Option<(u32, usize)>,
    /// The window opened through `open` was closed by the player.
    pub closed: bool,
    /// Close the window opened through `open` (the owner's vtable +0x20, `FUN_001aacb0`: the closing
    /// states start at once, without the button). The event player's opcode 0x2d action 1 sets it
    /// (`FUN_002275a8`); the window takes it in its next update. Ignored when no such window is open.
    pub close: bool,
    /// The window's state is 3 (state +0x70 of `FUN_001ab170`): the button closed it and the closing
    /// groups run. The event player's opcode 0x2d action 2 waits for it (`FUN_00222588`). Cleared when
    /// a new window opens.
    pub waiting: bool,
}

/// The pages of message bytes: a page ends at text command 0x10 (0x190fa8 walks the commands by
/// their sizes, 0x190f68), the message at 0. A 0x10 byte is a page end only where the walk reaches
/// it: the tokens of the bytes up to it equal the page's own tokens.
pub fn pages(b: &[u8]) -> Vec<Vec<Token>> {
    let mut out = Vec::new();
    let mut rest = b;
    while !rest.is_empty() {
        let page = kh2_image::text::tokens(rest);
        let end = (0..rest.len()).find(|&k| matches!(rest[k], 0 | 0x10) && kh2_image::text::tokens(&rest[..=k]) == page);
        out.push(page);
        match end {
            Some(k) if rest[k] == 0x10 => rest = &rest[k + 1..],
            _ => break,
        }
    }
    out
}

/// A text drawn by a node: the message, how to lay it out, and where it sits from the node's origin.
#[derive(Clone)]
pub struct Label {
    pub tokens: Vec<Token>,
    pub style: Style,
    pub offset: [f32; 2],
}

/// The set that holds only `hud_menu::mirror_reaction` (the HUD capsule's copy of the player's reaction offer), so
/// the lower crates order against it (`npc` shows its offer before it and hides it after it).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MirrorReaction;
