//! The jump task and the area-actor marker the lower crates read (moved out of `area_jump.rs`, which re-exports them).

use bevy::prelude::*;

/// `2 x 15` ticks: `FUN_0014a7c8(fader, 0xf, ..)` and `FUN_0014a710` (`param << 1`). The fader's
/// full alpha is `DAT_00379130` = 128.0 (0x80 is opaque on the PS2); the app's alpha runs 0..1.
pub const FADE_TICKS: f32 = 30.0;

/// A destination: the six bytes `FUN_00104e18` receives (world, area, entrance, then the map, btl and evt
/// program bytes). A byte of 0xff means "keep the record's" (`FUN_00104bd0`; `FUN_00104b90` for the programs,
/// where it keeps the story table's byte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dest {
    pub world: u8,
    pub area: u8,
    pub entrance: u8,
    pub programs: [u8; 3],
}

/// The "keep" byte of a destination.
pub const KEEP: u8 = 0xff;

impl Dest {
    /// A destination without program bytes (exits and type-1 jumps).
    pub fn new(world: u8, area: u8, entrance: u8) -> Dest {
        Dest { world, area, entrance, programs: [KEEP; 3] }
    }

    /// `FUN_00104bd0`: the bytes that are 0xff become the record's current world, area and entrance
    /// (`cur`); the program bytes stay as they are (0xff keeps the table's bytes).
    pub fn apply(self, cur: (u8, u8, u8)) -> Dest {
        let k = |b: u8, c: u8| if b == KEEP { c } else { b };
        Dest { world: k(self.world, cur.0), area: k(self.area, cur.1), entrance: k(self.entrance, cur.2), programs: self.programs }
    }

    /// The program bytes as the log shows them: `0xb5`, or `-` for a byte that keeps the table's.
    pub fn programs_text(&self) -> String {
        let b: Vec<String> = self.programs.iter().map(|p| if *p == KEEP { "-".to_string() } else { format!("{p:#x}") }).collect();
        format!("({})", b.join(", "))
    }
}

/// What a record asked for besides a jump (types 5 and 6): `menu_ending` takes `ending` (menu kind 0xe), `tick` the
/// type 6 screen fade.
#[derive(Resource, Debug, Default, PartialEq, Eq)]
pub struct JumpMenus {
    /// Type 5: `FUN_0024f0b8(0xe, -1)` was called; the ending screen opens.
    pub ending: bool,
    /// Type 6: `FUN_00149f38` was called; the screen freezes and fades to black.
    pub black: bool,
    /// The party words of the record a jump took (`FUN_0019d9d8`); `party::jump_party` applies them (WP-PT1).
    pub party: Option<PartyWords>,
    /// The reset code `FUN_00104d58` runs when the fade-out is over (`FUN_001abfe8`), waiting for the load.
    pub reset_at_load: Option<i16>,
    /// Reset codes to run now (`FUN_00104e18` runs it at once when the record's wait is not 0; the jump task runs it after
    /// the fade-out); `party::jump_reset` takes them.
    pub resets: Vec<i16>,
    /// The save point menu's World Map entry (`FUN_002580b8`): ticks of the screen fader going to black while the menu is still
    /// up (the jump task cannot run, the field is frozen). `draw_fader` paints it; the menu clears it when it hands over.
    pub map_fade: Option<u32>,
}

/// What `FUN_0019d9d8` does with a record's words 4 to 6 besides the jump itself (WP-PT1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartyWords {
    /// The destination world byte (`pbVar8`); `None` when the record keeps the current world (`FUN_001a71c0(-1)`).
    pub world: Option<u8>,
    /// Word 4: the party arrangement (a preset index, negative for none).
    pub arrange: i16,
    /// Word 6: 1 or 2 opens the camp's notice page 0x15 (`FUN_0024f300(arg, ..)`); the argument is 0 or 2.
    pub menu: Option<u32>,
}

/// The jump task (`FUN_00104d58`): ticks count the fade.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub enum JumpTask {
    #[default]
    Idle,
    FadeOut { dest: Dest, kind: u32, t: f32 },
    /// The destination is set on `CurrentArea`; waits until it is loaded and Sora is placed.
    Loading { dest: Dest, kind: u32 },
    FadeIn { kind: u32, t: f32 },
    /// The jump task's wait before its fade-out (`FUN_00104e18`'s 3rd argument, `JumpExtras::delay_ticks`): `left` ticks,
    /// then the fade-out starts at tick `t0` (0, or the end of the fade for a jump taken after an event).
    Wait { dest: Dest, kind: u32, left: u32, t0: f32 },
    /// Type 6 (`FUN_00149eb0`): the screen is frozen and faded to black over `FADE_TICKS`, then stays black. Nothing in the
    /// executable fades it back in by itself (the area start's fade-in and the event end's, `FUN_0013f1a0`, `FUN_002233d8`,
    /// run when something else starts); here the black is lifted when a jump starts or an event plays [inferred].
    Black { t: f32 },
}

/// An actor that belongs to the loaded area: the area's unload removes it (the enemies of `enemy.rs`).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct AreaActor;
