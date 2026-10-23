//! The reaction-command messages and the Execute values the lower crates read (moved out of `reaction_cmd.rs`, `npc.rs`, `savepoint.rs` and `menu_shop.rs`, which re-export them).

use bevy::prelude::*;

/// Sent when the player accepts an offer. `enemy_ai.rs` delivers it to the target's script as event
/// 0xb (event 11) with the arguments (player, `cmd`, target record {target object, `pos`, 0, -1, 0}).
#[derive(Message, Clone, Copy, Debug)]
pub struct ReactionAccepted {
    pub cmd: u16,
    pub pos: i16,
    pub target: Entity,
}

/// What a target's script asks of the *player* while a reaction command runs: the host calls the
/// Soldier's command-34 thread makes on the player's context (docs/systems/drive-limits-summons/contract.md). Written by
/// `enemy_ai.rs`; the consumer is the player side, `player_reaction.rs` (paired motion, velocity,
/// unhittable flag and queued motion are applied to Sora; `Charge` is still not consumed).
#[allow(dead_code)]
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum PlayerReactionCmd {
    /// Host 1.29 (FUN_0017f160): play entry `slot` of `from`'s motion set on the player's body with
    /// `blend` frames (the entry holds one sub-motion per player model, named `%03d0` of the player's
    /// objentry byte +7). `serial` is the value the player's +0x158 takes; the script compares it with
    /// `obj_word_158` to see whether its motion is still the one running.
    /// `ticks` is the sub-motion's length in 60 Hz ticks (0 when the entry has none for the player's model).
    PairedMotion { from: Entity, slot: usize, blend: f32, serial: i32, ticks: f32 },
    /// Host 1.70 / 1.71 with bit 3 on the player: cannot be hit (FUN_0019a4f0 tests it).
    Unhittable(bool),
    /// Host 1.89: the player's secondary velocity vector (+0x880): unit direction in the app's space
    /// and speed in centimetres per tick. Set again every tick while the script wants it.
    Velocity { dir: Vec3, speed: f32 },
    /// Host 1.13: queue motion `slot` of the player's own set behind the running one (FUN_0017e170).
    QueueMotion { slot: usize, blend: f32 },
    /// Commands 455 and 457 accepted by a player of Mickey's class (FUN_001ce518, not the generic
    /// accept path; Sora's class sends them to the target as event 11): the player's timer at +0xb68 is set (120 ticks and motion 0xc1 for 455, 30 ticks and no motion for
    /// 457); while it runs FUN_001ce240 adds one unit to the drive gauge (FUN_001a0290) each time its
    /// second timer (+0xb6c) runs out with the motion near its end, and at zero asks for motion 5. The
    /// consumer (`drive.rs`, `sora.rs`) is a hook.
    Charge { ticks: f32, motion: Option<u16> },
}

/// The player's part of a running reaction command, as the scripts see it.
#[derive(Resource, Default, Debug)]
pub struct PlayerReaction {
    /// The player's +0x158: the serial of the motion `play_motion_with` last started on him (the
    /// counter at 0x35ccb0 counts down from 0 and never returns above -1).
    pub serial: i32,
    /// Set by the player side when the paired motion has ended (bit 1 of the player's +0x160, read by
    /// host 1.14). Set by `player_reaction.rs` from the real end of Sora's paired motion.
    pub paired_done: bool,
    /// Ticks left of Sora's current paired motion (`player_reaction.rs` writes it from the animator
    /// every tick).
    pub remaining: Option<f32>,
}

/// The Execute value FUN_00177b18 turns into a talk (case 0x1f).
pub const EXECUTE_TALK: u16 = 0x1f;

/// Execute 0x1e, "Examine" (command 30, message 0x81eb): `FUN_00177b18` case 0x1e -> `FUN_001a54a8`, the same talk table
/// with the task `FUN_001a5510` and the callback `FUN_001a5608` (WP-SH1).
pub const EXECUTE_EXAMINE: u16 = 0x1e;

/// The Execute value of command 55 (`FUN_001ba2c0` and `FUN_00177b18` case 0x37).
pub const EXECUTE_SAVE: u16 = 0x37;

/// The events `FUN_00197b70` sends to menu kind 4 (`FUN_0024f0b8(4, event)`).
pub fn opens_shop(event: u32) -> bool {
    matches!(event, 0x13 | 0x66..=0x75 | 0x83..=0x85 | 0x8a)
}

/// The Execute value of the rows whose accept fires their event (`FUN_00177b18` case 0x7f).
pub const EXECUTE_EVENT: u16 = 0x7f;

/// A shop the next frame opens (`FUN_0024f0b8(4, event)`), set by the reaction accept.
#[derive(Resource, Default)]
pub struct ShopRequest(pub Option<u16>);
