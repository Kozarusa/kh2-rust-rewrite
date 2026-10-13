//! The tutorial windows of the first Seifer fight (WP-T1): the black message box at the top centre with the
//! game's own tutorial text, and the pointer arrow over the gauge it explains. Rules and evidence in
//! docs/hud.md ("Tutorial windows").
//!
//! What the game does (`SLUS_210.05`):
//! - The mission script `msn/us/TT04_MS107.bar` (Seifer, spawn parameter 1; docs/struggle.md) opens the window in its
//!   event 3 thread, at the first instruction: `push 0x3ef7`, host call 1.296. Host 1.296 is `FUN_001744a0`: it builds
//!   a window object of class `FUN_001abba0` (0x1194 bytes; constructor chain `FUN_001ab830`, the window class 1
//!   of `kh2_image::window::CLASSES`: font 0 at 16, colour 6, top centre (256, 52), frame kind 4, 432 wide)
//!   and registers it (`FUN_001aad78`). The class constructor raises the HUD flag word `DAT_00356f1c` bit 8
//!   (`FUN_0014c748(8)`) and the destructor clears it (`FUN_0014c760(8)`; bits 1 and 4 are the pause menu and the
//!   events; `FUN_0014c7b8` stops the play-time counter while the word is not 0) and takes record 0xc of the list
//!   at `DAT_01d5dd88` (`FUN_001bef48`, `FUN_001befc0`) [what that record does is a GAP].
//! - With one page and no child window, the window waits at its prompt and closes on the pad bits 0x40c of the
//!   newly pressed word (`FUN_001ab170` case 2 -> `FUN_001aa430`, sound 0x18 and the close method `FUN_001aacd8`).
//! - The message's own 0x0f command starts the pointer (`hud_msgwin::pointer_of`).
//! - Message 0x3ef7 of `msg/us/tt.bar` is "The gauge in the bottom right is the HP gauge. ..." with the pointer
//!   (kind 1, 455, 310). Message 0x4309 holds the same HP sentence, the target mark, the attack button and, in the
//!   objective colour (cyan, 0x07 command 0, 240, 240, 128), "Approach and attack Seifer."; no script of
//!   `msn/us/*.bar`, `ard/*.ard` or `00common.bdx` pushes it (0x4306 to 0x430b are not referenced there) [GAP: its opener].
//!
//! What the port does:
//! - `MissionStart` of `TT04_MS107` opens message 0x3ef7 (the script's own first action). The game's close rule is
//!   the confirm or cancel pad bit; the port uses J or K, the keys the menus use for them [inferred].
//! - `KH2_TUTORIAL=1` (with `KH2_TUTORIAL_AT=<app second>` to start it later) plays the footage's sequence on any area (`TIMELINE`, seconds from the first frame the HUD text
//!   is ready): the window opens at 0.10 s with 0x3ef7 (the footage's first page, same sentence), switches to 0x4309 at
//!   5.50 s (the footage's "yellow mark" and "Approach and attack Seifer." pages) and closes at 8.40 s. The footage's
//!   second page (the MP gauge) has no message on the PS2 disc: it is left out [GAP].

use bevy::prelude::*;

use super::MsgWindow;
use super::Request;
use crate::hud::HudScene;
use crate::schedule::Frame;
use crate::struggle::MissionStart;

/// Window class of the tutorial windows (`kh2_image::window::CLASSES`, constructor 0x1ab8e0 / 0x1ab830).
pub const CLASS: usize = 1;
/// The mission whose script opens the window (`TT04_MS107`, mission id 0xad).
pub const MISSION: &str = "TT04_MS107";
/// The first window's message (the script's `push 0x3ef7`).
pub const HP_MESSAGE: u32 = 0x3ef7;
/// The message with the target mark and the objective line.
pub const OBJECTIVE_MESSAGE: u32 = 0x4309;
/// The footage's sequence (video 2.mp4, 60 fps frames read to 0.05 s): (second, message; `None` closes).
pub const TIMELINE: [(f32, Option<u32>); 3] = [(0.10, Some(HP_MESSAGE)), (5.50, Some(OBJECTIVE_MESSAGE)), (8.40, None)];

/// The message the harness timeline shows at second `t` (`None` before the first entry and after the close).
pub fn message_at(t: f32) -> Option<u32> {
    TIMELINE.iter().rev().find(|(s, _)| t >= *s).and_then(|(_, m)| *m)
}

#[derive(Resource, Default)]
pub struct Tutorial {
    /// Seconds since the HUD text was ready.
    t: f32,
    /// A mission-opened window is showing (closed by the confirm or cancel key).
    mission_open: bool,
    /// The harness's last request, so a change is written once.
    last: Option<Option<u32>>,
}

pub struct TutorialPlugin;

impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tutorial>().add_systems(Update, drive.in_set(Frame::Hud).before(super::update));
    }
}

fn open(win: &mut MsgWindow, scene: &HudScene, id: u32) {
    let Some(tokens) = scene.text.as_ref().and_then(|t| t.lookup(id)) else { return };
    info!("tutorial: window {id:#x} {:?}", kh2_image::text::to_text(&tokens));
    win.0 = Some(Request { tokens, class: CLASS });
}

fn drive(
    mut st: ResMut<Tutorial>,
    mut win: ResMut<MsgWindow>,
    scene: Res<HudScene>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut starts: MessageReader<MissionStart>,
    mut harness: Local<Option<bool>>,
) {
    let harness = *harness.get_or_insert_with(|| std::env::var("KH2_TUTORIAL").is_ok_and(|v| v != "0"));
    let started = starts.read().any(|s| s.name == MISSION);
    if scene.text.is_none() {
        return;
    }
    if harness {
        // `KH2_TUTORIAL_AT=<s>` starts the timeline that many seconds after the app started (screenshots need the pipelines first).
        let from = std::env::var("KH2_TUTORIAL_AT").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        if time.elapsed_secs() < from {
            return;
        }
        st.t += time.delta_secs();
        let want = message_at(st.t);
        if st.last != Some(want) {
            st.last = Some(want);
            match want {
                Some(id) => open(&mut win, &scene, id),
                None => win.0 = None,
            }
        }
        return;
    }
    if started && !st.mission_open {
        st.mission_open = true;
        open(&mut win, &scene, HP_MESSAGE);
    }
    if st.mission_open && (keys.just_pressed(KeyCode::KeyJ) || keys.just_pressed(KeyCode::KeyK)) {
        st.mission_open = false;
        win.0 = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::hud_msgwin::pointer_of;
    use kh2_image::text::{Messages, Token};

    #[test]
    fn the_timeline_follows_the_footage() {
        assert_eq!(message_at(0.0), None);
        assert_eq!(message_at(0.1), Some(HP_MESSAGE));
        assert_eq!(message_at(5.49), Some(HP_MESSAGE));
        assert_eq!(message_at(5.5), Some(OBJECTIVE_MESSAGE));
        assert_eq!(message_at(8.39), Some(OBJECTIVE_MESSAGE));
        assert_eq!(message_at(8.4), None);
    }

    #[test]
    fn the_pointer_command_carries_kind_and_position() {
        let t = vec![Token::Callback(0x0f, vec![1, 0xc7, 0x01, 0x36, 0x01]), Token::Space];
        assert_eq!(pointer_of(&t), Some((1, 455, 310)));
        assert_eq!(pointer_of(&[Token::Callback(0x0f, vec![0, 1, 0, 2, 0])]), None);
        assert_eq!(pointer_of(&[Token::Callback(0x05, vec![1, 0xc7, 0x01, 0x36, 0x01])]), None);
    }

    /// The disc's own tutorial messages: 0x3ef7 starts the pointer over the HP ring, 0x4309 holds the cyan objective.
    #[test]
    fn the_tutorial_messages_on_the_disc() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = crate::assets::Disc::open(&iso);
        let Some(bar) = disc.read("msg/us/tt.bar") else { return };
        let m = Messages::from_bar(&bar).expect("tt.bar messages");
        let hp = m.tokens(HP_MESSAGE).expect("0x3ef7");
        assert!(kh2_image::text::to_text(&hp).contains("HP gauge"), "{:?}", kh2_image::text::to_text(&hp));
        assert_eq!(pointer_of(&hp), Some((1, 455, 310)));
        let obj = m.tokens(OBJECTIVE_MESSAGE).expect("0x4309");
        assert!(kh2_image::text::to_text(&obj).contains("Approach and attack Seifer"));
        assert!(obj.contains(&Token::Rgba([0x00, 0xf0, 0xf0, 0x80])), "the objective colour");
        assert_eq!(pointer_of(&obj), None);
    }
}
