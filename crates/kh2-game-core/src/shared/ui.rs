//! The menu data the lower crates read: the option word, the game-camera filter and the menu phase (moved out of `menu_config.rs`, `menu_preview.rs` and `menu.rs`, which re-export them).

use bevy::prelude::*;

/// Option word bits (the readers' own shifts, checked by `menu_tables`).
pub mod bit {
    pub const VIBRATION: u32 = 0;
    pub const SOUND: u32 = 1; // two bits
    pub const NAV_MAP: u32 = 3;
    pub const FIELD_CAMERA: u32 = 4;
    pub const RIGHT_STICK: u32 = 5;
    pub const COMMAND_MENU: u32 = 6;
    pub const CAMERA_LR: u32 = 7;
    pub const CAMERA_UD: u32 = 8;
    pub const SUMMON: u32 = 9; // two bits
}

/// Summon Effects as the arrival code reads it (`FUN_001c65f0`: raw 1 plays every time, 2 only the first
/// time, anything else never).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummonEffects {
    None,
    Full,
    Auto,
    /// Raw 3: the page shows Full, the arrival code plays nothing.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Mono,
    Stereo,
    ProLogic2,
    /// Raw 3 (the page shows Stereo).
    Other,
}

/// The option word as the rest of the game reads it (`FUN_0023eb10..0023eed8`). A resource: the systems
/// that apply a setting read it, the page replaces it when it is left.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Field Camera: Manual (bit 4 set); Auto otherwise (`FUN_0023ec68`).
    pub field_camera_manual: bool,
    /// Right Analog Stick: Command Menu (bit 5 set); Camera otherwise (`FUN_0023ece0`).
    pub right_stick_command_menu: bool,
    /// Camera Up/Down reversed (`FUN_0023ee68`) and Left/Right reversed (`FUN_0023edf8`).
    pub camera_ud_reverse: bool,
    pub camera_lr_reverse: bool,
    pub summon: SummonEffects,
    /// Navigational Map ON (`FUN_0023ebf8`), Vibration ON (`FUN_0023eb10`).
    pub nav_map: bool,
    pub vibration: bool,
    pub sound: Sound,
    /// Command Menu: the per-world Kingdom Hearts theme (bit 6 clear, `FUN_0023ed80`), else the original.
    pub command_menu_themed: bool,
}

impl Options {
    pub fn from_word(w: u32) -> Options {
        let b = |n: u32| (w >> n) & 1 != 0;
        Options {
            field_camera_manual: b(bit::FIELD_CAMERA),
            right_stick_command_menu: b(bit::RIGHT_STICK),
            camera_ud_reverse: b(bit::CAMERA_UD),
            camera_lr_reverse: b(bit::CAMERA_LR),
            summon: match (w >> bit::SUMMON) & 3 {
                0 => SummonEffects::None,
                1 => SummonEffects::Full,
                2 => SummonEffects::Auto,
                _ => SummonEffects::Other,
            },
            nav_map: b(bit::NAV_MAP),
            vibration: b(bit::VIBRATION),
            sound: match (w >> bit::SOUND) & 3 {
                0 => Sound::Mono,
                1 => Sound::Stereo,
                2 => Sound::ProLogic2,
                _ => Sound::Other,
            },
            command_menu_themed: !b(bit::COMMAND_MENU),
        }
    }

    /// The raw 2-bit Summon Effects field `FUN_001c65f0` tests.
    pub fn summon_raw(&self) -> u32 {
        match self.summon {
            SummonEffects::None => 0,
            SummonEffects::Full => 1,
            SummonEffects::Auto => 2,
            SummonEffects::Other => 3,
        }
    }
}

impl Default for Options {
    fn default() -> Options {
        Options::from_word(crate::progress::OPTION_DEFAULT)
    }
}

/// Marks the preview's own `Camera3d`. Every system that looks for THE game camera filters `Without<PreviewCamera>`:
/// with two 3D cameras alive `Query::single()` would fail (the effects' view fell back to identity, the HUD lost its projection).
#[derive(Component)]
pub struct PreviewCamera;

/// The filter of every query for THE game camera (the preview's own `Camera3d` is left out).
pub type GameCamera = (With<Camera3d>, Without<PreviewCamera>);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Closed,
    /// The enter animations play (the first 15-frame groups of the layout).
    Opening,
    Open,
    /// The leave animations play and the dim returns over `CLOSE_FRAMES`.
    Closing,
}

/// The menu's phase (`Menu::phase`) as the fixed-step gates of the lower crates read it (the limit and reaction-command
/// gates). The menu crate copies it in `RunFixedMainLoopSystems::BeforeFixedMainLoop`: the menu changes its phase
/// only in `PreUpdate` and `Update`, so every fixed step reads the same value it read from `Menu` itself.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuPhase(pub Phase);

impl Default for MenuPhase {
    fn default() -> MenuPhase {
        MenuPhase(Phase::Closed)
    }
}

/// The systems of the lower crates that run only while the menu is closed; the menu crate gives the set the
/// condition `menu::is_closed`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WhileMenuClosed;

/// A request of the camp's party-switch notice page (page 0x15): the notice and the camp's argument
/// (`FUN_0024f300` 's first parameter, 0 or 2). `party_switch` re-exports it as `Request`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchRequest {
    pub notice: crate::party_rules::Notice,
    pub arg: u32,
}

/// The party-switch request the last jump record made (`party::jump_party` sets it); the page takes it when no
/// menu is up. Moved out of `PartySwitch` so the party crate can write it.
#[derive(Resource, Default, Debug)]
pub struct PartySwitchRequest(pub Option<SwitchRequest>);

/// A save point's request of the save menu (menu kind 1, `FUN_0024f0b8(1, mode)` from `FUN_001784b8`), for the event
/// crate's `savepoint::point_use`. `open` mirrors whether `SaveMenu::request_point` would take a request (the menu is
/// loaded, closed and holds no request): the menu crate writes it in `RunFixedMainLoopSystems::BeforeFixedMainLoop`
/// (the save menu changes only in `Startup`, `PreUpdate` and `Update`) and moves `mode` into the save menu in
/// `AfterFixedMainLoop`, before any `Update` system reads it.
#[derive(Resource, Default, Debug)]
pub struct SavePointRequest {
    pub mode: Option<i32>,
    pub open: bool,
}

impl SavePointRequest {
    /// `SaveMenu::request_point` from the fixed step: false when the menu cannot open (no `save.2ld`, or a menu is up).
    pub fn request(&mut self, mode: i32) -> bool {
        if !self.open || self.mode.is_some() {
            return false;
        }
        self.mode = Some(mode);
        true
    }
}
