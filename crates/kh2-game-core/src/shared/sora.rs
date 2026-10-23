//! Sora's controller state that the lower crates read (moved out of `sora.rs`, which re-exports it).

use std::sync::Arc;

use bevy::prelude::*;
use kh2_anim::moveset::{Chain, Moveset};
use kh2_anim::PlayerState;

use crate::data::Stats;
use crate::motor::Motor;
use crate::reaction_rules::{Blow, Push};
use crate::skel::{Animator, Rig};

/// The battle flag every system reads (the game's `FUN_001903d8`, a tri-state set from the player's
/// and the enemies' awareness). The app's stand-in: ticks left of the battle mode that Sora's
/// controller raises while an enemy is within lock-on range or after an attack, and that drives
/// the keyblade out. The drive code reads it too.
#[derive(Resource, Default, Debug)]
pub struct Battle {
    pub ticks: f32,
    /// The motion-set variant of the last tick, for the `KH2_MOVELOG` line on a change.
    pub last_state: Option<PlayerState>,
}

impl Battle {
    pub fn active(&self) -> bool {
        self.ticks > 0.0
    }
}

/// The ability block as the controller reads it (`FUN_001c0d88`, `FUN_001c0f40`): the ability
/// bits that gate the attack table and the combo lengths. The ability system (`abilities.rs`)
/// writes it from its block whenever that changes (equipment, the `KH2_ABILITIES` overrides, a
/// drive form's list on entry and revert). The default is the app's baseline when the ability
/// system is off: Guard and the block's base lengths.
#[derive(Resource, Clone, Debug)]
pub struct Abilities {
    pub bits: u128,
    /// The block's bits 128..255 (Once More 128, Auto Limit 129, ...), which the attack table does
    /// not gate.
    pub high: u128,
    pub max_ground: u8,
    pub max_air: u8,
    pub max_finishers: u8,
}

impl Default for Abilities {
    fn default() -> Abilities {
        Abilities { bits: 1u128 << ABILITY_GUARD, high: 0, max_ground: BASE_HITS_GROUND, max_air: BASE_HITS_AIR, max_finishers: BASE_FINISHERS }
    }
}

impl Abilities {
    /// The controller's view of the ability block.
    pub fn from_block(a: &crate::shared::abilities::Abilities) -> Abilities {
        let w = &a.block().bits;
        let high = (4..8).fold(0u128, |h, i| h | u128::from(w[i]) << (32 * (i - 4)));
        Abilities { bits: a.bits(), high, max_ground: a.ground_combo(), max_air: a.air_combo(), max_finishers: a.finishers() }
    }

    /// One bit of the block (0..256).
    pub fn has_bit(&self, bit: u16) -> bool {
        match bit {
            0..128 => self.bits >> bit & 1 != 0,
            128..256 => self.high >> (bit - 128) & 1 != 0,
            _ => false,
        }
    }

    /// Every ability bit set (the low 128 of the block's 256).
    pub fn bits(&self) -> u128 {
        self.bits
    }

    /// Attacks before the finisher on the ground and in the air, and finishers per chain
    /// (block `+0x28`, `+0x29`, `+0x2a`; the game counts equipped copies).
    pub fn combo_block(&self) -> (u8, u8, u8) {
        (self.max_ground, self.max_air, self.max_finishers)
    }

    /// The combo state the attack table's availability tests read.
    pub fn chain(&self, hits: u8, finishers: u8, airborne: bool, finisher_window: bool) -> Chain {
        let (max_ground, max_air, max_finishers) = self.combo_block();
        Chain { airborne, abilities: self.bits(), combo: hits, finishers, max_ground, max_air, max_finishers, finisher_window }
    }
}

/// Ability bits (the attack table's gate) that start set: Guard.
pub const ABILITY_GUARD: u16 = 1;

/// Base combo lengths of the status block (FUN_001c0d88): attacks before the finisher on the
/// ground and in the air, and finishers per chain.
pub const BASE_HITS_GROUND: u8 = 3;

pub const BASE_HITS_AIR: u8 = 3;

pub const BASE_FINISHERS: u8 = 1;

/// Constants of the air swing's movement rules, read from the executable at start-up.
///
/// The air wall recoil: FUN_00175668 (the actor's wall method +0x70) calls FUN_00185d30, whose
/// airborne branch (FUN_0019aa28) requests motion 43 with motion 4 (the fall) queued and, for a power
/// above 0, calls FUN_00185978 with the facing (FUN_0016a890, actor +0x550) times `recoil_facing`.
/// FUN_00185978 (when FUN_0019b1d0 allows a push) builds the launch vector (x, `recoil_height`, z,
/// power x sstm HitBackToJump) and launches the actor's mover with it (method +8; the jump mover's
/// FUN_001796a8 -> FUN_0017b278).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingConsts {
    /// DAT_0037a960: the recoil launch vector's y, the jump height FUN_0017b1e0 starts.
    pub recoil_height: f32,
    /// DAT_0037a968: the factor on the facing (FUN_00185d30); -1 sends Sora backwards.
    pub recoil_facing: f32,
    /// DAT_0037a970: FUN_00186018 turns nothing while the target is this close (horizontal units).
    pub turn_min_gap: f32,
    /// DAT_0037a988: the trigger parameter's unit for the turn rate (degrees to radians).
    pub turn_unit: f32,
}

impl SwingConsts {
    pub fn from_exe(exe: &[u8]) -> Option<SwingConsts> {
        let f = |va: u32| crate::reaction_rules::exe_f32(exe, va);
        Some(SwingConsts { recoil_height: f(0x37a960)?, recoil_facing: f(0x37a968)?, turn_min_gap: f(0x37a970)?, turn_unit: f(0x37a988)? })
    }
}

/// The phase of a Square dash on the ground (`State::QuickRun`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dash {
    /// Pressed while moving, still held, fewer than `TAP_TICKS` ticks ago: not yet decided.
    #[default]
    Pending,
    /// Released within `TAP_TICKS`: the dodge roll, which runs `TAP_TICKS` ticks and stops.
    Roll,
    /// Held past `TAP_TICKS` with Quick Run learned: the Quick Run (FUN_00178108 / FUN_001c1660),
    /// which runs while the button is held, at most `AirSlideTime` ticks.
    Run,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Ground,
    Air,
    QuickRun,
    Glide,
    Attack { air: bool, finisher: bool },
    Guard,
    Counter,
    Hurt,
    Blown,
    Down,
    Viewer,
    /// A drive form change is playing (motions A120 and A121): the drive code holds the controller,
    /// there is no input, and the hit tests are off. It sets the state back when the change ends.
    FormChange,
    /// A reaction command's paired motion is playing on Sora (`player_reaction.rs` starts and ends
    /// it): no input, his velocity is the script's secondary velocity, gravity goes on.
    Reaction,
    /// A spell's cast motion is playing (`magic.rs` starts it), or the item-use motion (motion
    /// request 9, `items.rs`): no input until the motion lets go.
    Magic,
}

/// Controller input, sampled every rendered frame and consumed by the 60 Hz tick.
#[derive(Resource, Default, Debug)]
pub struct Pad {
    /// Camera-relative move direction (unit, or zero).
    pub dir: Vec3,
    /// Stick magnitude 0..1 (keyboard: 1, or 0.5 while walking).
    pub stick: f32,
    pub jump_held: bool,
    pub square_held: bool,
    pub attack: u8,
    pub jump: u8,
    pub square: u8,
    pub lock: u8,
    /// The lock button was newly pressed in the controller's last tick (the field button set's logical bit 0x800 at
    /// the input record's +8, which FUN_0019b7a8 reads); see `lock_newly_pressed`.
    pub lock_new: bool,
    pub viewer: u8,
    pub next: u8,
    pub prev: u8,
}

impl Pad {
    pub fn take(n: &mut u8) -> bool {
        let p = *n > 0;
        *n = n.saturating_sub(1);
        p
    }

    /// Takes one pending attack press for another use (the command menu executes the selected
    /// command with the same button, `magic.rs`). True when there was one.
    pub fn steal_attack(&mut self) -> bool {
        Pad::take(&mut self.attack)
    }

    /// The pad reads as untouched and its pending presses are dropped (WP-F1 item 2): while a mission
    /// layout plays, the players lose control (`FUN_00196280` -> `FUN_001b7ce8`, read by `FUN_001b7d50`
    /// through `FUN_001794f0`), and after the fight's stop (`FUN_001b9940`) every command is refused
    /// (`FUN_001ba2c0` -> `FUN_001b70b8` answers 5) and the player's method +0x60 reports busy.
    pub fn lock(&mut self) {
        self.dir = Vec3::ZERO;
        self.stick = 0.0;
        self.jump_held = false;
        self.square_held = false;
        self.attack = 0;
        self.jump = 0;
        self.square = 0;
        self.lock = 0;
    }

    /// The lock button was newly pressed in the last controller tick (`tick` takes the press and keeps it here until the
    /// next tick). The lock-on controller of the magic casts reads it exactly (FUN_0019b7a8: the input record's +8 bit 0x800).
    pub fn lock_newly_pressed(&self) -> bool {
        self.lock_new
    }

    /// A press is pending or a button is held.
    pub fn any_input(&self) -> bool {
        self.dir != Vec3::ZERO || self.stick > 0.0 || self.jump_held || self.square_held || self.attack > 0 || self.jump > 0 || self.square > 0 || self.lock > 0
    }
}

#[derive(Component)]
pub struct Sora {
    pub state: State,
    pub motor: Motor,
    /// The game's combo counters (actor +0xb04, +0xb05): attacks accepted in the current chain, and
    /// finishers accepted (1 at the start of a chain).
    pub hits: u8,
    pub finishers: u8,
    pub queued_attack: bool,
    /// A hit of the current swing landed (actor +0x108 bit 0x2000000): a chain only goes on after one.
    pub connected: bool,
    /// The refused press was already logged.
    pub deny_logged: bool,
    /// Sora's attack table (the ability bits that unlock gated actions are the `Abilities` resource).
    pub moves: Arc<Moveset>,
    /// The blow that knocked Sora down came from behind (picks the matching landing motion).
    pub blown_back: bool,
    /// Decaying push (wall recoil, push reactions), added to the movement every tick.
    pub push: Option<Push>,
    /// Flight of the blow that put Sora in `State::Blown`.
    pub blow: Option<Blow>,
    pub stats: Stats,
    pub hp: i32,
    pub mp: i32,
    /// A game over holds him down (`menu_gameover`, `FUN_001cec00` -> `FUN_001ad810`): the in-place stand-up below does not
    /// run, the game over's choice decides what happens to him.
    pub game_over: bool,
    /// Invincible ticks left.
    pub invuln: f32,
    /// Leaf Bracer's untouchable ticks (actor +0xb00, written by Cure's script through host 1.91,
    /// lowered each tick by `FUN_00175ab0`): every hit is dropped while it is above 0
    /// (`FUN_00175ad8`, `FUN_00175340`). Start it with `Sora::start_leaf_bracer`.
    pub leaf_bracer: f32,
    /// The damage motion id playing (10..=39 for the hit reactions), for Once More (`FUN_00185a08`
    /// reads the actor's motion id +0x154); None outside a hit reaction.
    pub damage_motion: Option<usize>,
    pub lock: Option<Entity>,
    pub target: Option<Entity>,
    pub body: Entity,
    /// The weapons in hand order (0 right, 1 left), as the game's per-object weapon list
    /// (`FUN_001751b0` attaches hand 0 for every player object and hand 1 in a form). Empty
    /// for a form without a weapon (Anti). The drive code swaps it with the body.
    pub weapons: Vec<Entity>,
    /// Ticks in the current state.
    pub timer: f32,
    /// Quick Run: ticks left while the button is held, and the per-tick brake.
    pub dash_time: f32,
    pub dash_brake: f32,
    /// The Square dash's phase (dodge roll or Quick Run, decided by how long the button is held).
    pub dash: Dash,
    /// A gravity-suspending trigger (3, 24 or 30) covered the playing motion last tick while airborne:
    /// actor +0x108 bits 0x800000 / 0x1000000 (FUN_00169ee8). When it ends FUN_0016a2f0 restarts
    /// the fall carrying the last real movement.
    pub hover: bool,
    /// The last tick's real displacement after collisions (actor +0x860, FUN_00181e00).
    pub last_move: Vec3,
    /// The air wall recoil (motion 43) is playing on the jump mover it launched (`air_recoil_launch`):
    /// the mover's brake and gravity run, and a landing plays the landing motion (FUN_0017b9e0).
    pub air_recoil: bool,
    /// The executable's constants of the air recoil and the attack turns (None: not readable).
    pub swing: Option<SwingConsts>,
    /// One Aerial Dodge per airtime below MAX.
    pub aerial_dodge_ready: bool,
    pub lunge_left: f32,
    pub lunge_speed: f32,
    pub viewer_slot: usize,
}

impl Sora {
    /// A controller with the defaults and no scene, for the tests of systems that read his HP and flags.
    pub fn for_test(hp: i32, max_hp: i32) -> Sora {
        Sora {
            state: State::Ground,
            motor: Motor::default(),
            hits: 0,
            finishers: 0,
            queued_attack: false,
            connected: false,
            deny_logged: false,
            moves: Arc::new(Moveset::default()),
            blown_back: false,
            push: None,
            blow: None,
            hp,
            mp: 0,
            stats: Stats { level: 1, max_hp, strength: 0, magic: 0, defense: 0, ap: 0, max_mp: 0, max_taken: 0, min_taken: 0, max_dealt: 0, min_dealt: 0, weakness: [0; 7] },
            invuln: 0.0,
            game_over: false,
            leaf_bracer: 0.0,
            damage_motion: None,
            lock: None,
            target: None,
            body: Entity::PLACEHOLDER,
            weapons: Vec::new(),
            timer: 0.0,
            dash_time: 0.0,
            dash_brake: 0.0,
            dash: Dash::Pending,
            hover: false,
            last_move: Vec3::ZERO,
            air_recoil: false,
            swing: None,
            aerial_dodge_ready: false,
            lunge_left: 0.0,
            lunge_speed: 0.0,
            viewer_slot: 0,
        }
    }

    /// Leaf Bracer at a Cure cast's event 20 (`abilities::leaf_bracer_frames`): with the ability and
    /// the cast on Sora himself, he is untouchable for 60 ticks. Returns whether it started.
    /// The caller is Cure's cast in `magic.rs` (since commit 7ab807f) and the tests.
    pub fn start_leaf_bracer(&mut self, abilities: &Abilities, target_is_caster: bool) -> bool {
        let Some(t) = crate::shared::abilities::leaf_bracer_frames(abilities.has_bit(crate::shared::abilities::BIT_LEAF_BRACER), target_is_caster) else { return false };
        self.leaf_bracer = t;
        if std::env::var_os("KH2_ABILITYLOG").is_some() {
            info!("abilities: Leaf Bracer: Cure on Sora, untouchable for {t} ticks (host 1.91 -> +0xb00)");
        }
        true
    }

    // Accessors for the motor; no non-test caller yet.
    #[allow(dead_code)]
    pub fn grounded(&self) -> bool {
        self.motor.grounded
    }
    #[allow(dead_code)]
    pub fn yaw(&self) -> f32 {
        self.motor.yaw
    }

    /// Enters `state` from outside the controller (reaction commands and clashes,
    /// `player_reaction.rs`): the motion is already requested; the state's clock restarts, the
    /// velocity and a pending attack are dropped and `push` replaces the running push.
    pub fn enter(&mut self, state: State, push: Option<Push>) {
        self.state = state;
        self.timer = 0.0;
        self.motor.vel = Vec3::ZERO;
        self.queued_attack = false;
        self.push = push;
    }
}

/// One motion request the controller makes of the weapons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponMotion {
    pub slot: Option<usize>,
    pub looping: bool,
    pub blend: f32,
    /// `Animator::restart` rather than `play`.
    pub restart: bool,
}

/// The weapons as the controller addresses them in one tick. It has the `play`/`restart` shape
/// of `Animator`, so the attack code is written as for one weapon; `apply` replays the requests on
/// every weapon in order, each weapon playing a slot only when its own motion set has it (what
/// the one-weapon code did with `wrig.mset.has`). The requests are applied before the pose step
/// of the same tick (`Sim::Pose` runs after `Sim::Sora` and `Sim::SoraHurt`), so the result is
/// the same as writing the animator directly.
#[derive(Default, Debug)]
pub struct WeaponCue {
    pub ops: Vec<WeaponMotion>,
}

impl WeaponCue {
    pub fn play(&mut self, slot: Option<usize>, looping: bool, blend: f32) {
        self.ops.push(WeaponMotion { slot, looping, blend, restart: false });
    }

    pub fn restart(&mut self, slot: Option<usize>, looping: bool, blend: f32) {
        self.ops.push(WeaponMotion { slot, looping, blend, restart: true });
    }

    pub fn apply(&self, weapons: &[Entity], anims: &mut Query<&mut Animator>) {
        for w in weapons {
            let Ok(mut a) = anims.get_mut(*w) else { continue };
            for op in &self.ops {
                let slot = op.slot.filter(|&x| a.rig.mset.has(x));
                if op.restart {
                    a.restart(slot, op.looping, op.blend);
                } else {
                    a.play(slot, op.looping, op.blend);
                }
            }
        }
    }
}

#[derive(Resource)]
pub struct Rigs {
    pub sora: Arc<Rig>,
    pub keyblade: Arc<Rig>,
    pub shadow: Arc<Rig>,
    pub soldier: Arc<Rig>,
}

/// Puts Sora on an area's spawn point.
pub fn respawn(s: &mut Sora, at: Vec3) {
    s.motor = Motor { pos: at, ..Motor::default() };
    s.state = State::Ground;
}

/// The bones the weapons hang from, by hand: 0 the right hand (bone 178, checked on screen), 1 the
/// left hand (bone 86, the mirror of 178, inferred: the game's attach path `FUN_001751b0` ->
/// `FUN_00184190` ends in resource handles, not a bone number; `objentry.WeaponJoint` is the
/// data candidate, see docs/systems/drive-limits-summons/drive-forms.md). The game attaches hand 1 only while a drive form is
/// active (`FUN_00176f88`: form field 1..5 or 9).
pub const HAND_BONES: [usize; 2] = [178, 86];
