//! Prize objects: the HP, MP and drive orbs and the munny that an enemy's drop (host 1.92,
//! `FUN_001a3cb0`) throws out, their flight, rest, fade and pickup. `docs/systems/combat-effects/prizes.md` has the
//! addresses; every number here is read from the user's `SLUS_210.05` at start (`Consts::from_exe`)
//! except the code's own immediates (pool size, recheck interval, bounce sound count).
//!
//! What the game does (verified in the decompilation and the disassembly):
//! - The prize manager is an object of `PRIZE` (object 0x5c3, `FUN_001a3f98`) holding a pool of
//!   0x100 prizes of 0x60 bytes. `FUN_001a3b28` takes one per prize: the drop point, the kind's
//!   16-byte entry at 0x37b4b0 (kind, radius, bounce sound, pickup sound), the update
//!   `FUN_001c5288`, and a launch velocity from three generator steps.
//! - Falling (`FUN_001c5248`): `FUN_001c5060` moves the prize's low point (the position plus the
//!   kind's radius, Y down) by its velocity through the character mover; touching the floor while
//!   falling faster than 5 bounces it (velocity times -0.8, a sound for the first two), slower stops
//!   it (state `FUN_001c5388`, rest); a sideways move the mover cut short reverses that axis;
//!   gravity 3 per frame up to 50, sideways velocity times 0.95; once falling the prize can be
//!   picked up. Then `FUN_001c52a8`: in the last 12 frames of its 900-frame life the prize shrinks
//!   (its draw size byte = life * 255 / 12) and cannot be picked up; before that the manager's
//!   pickup check runs (`FUN_001a3848`). `FUN_001c5020` takes a frame off the life.
//! - At rest every 30 frames the mover is asked again; without a floor it falls again.
//! - Pickup (`FUN_001a3848` -> `FUN_001a3700`): within 120 (plus Draw) horizontally and 30
//!   vertically of the player's position; Anti form does not take HP orbs. `FUN_001a3a88` turns
//!   the prize toward the player (`FUN_001c5808`: 40 frames, the pickup sound) and applies the
//!   pickup table at 0x37b540 (`FUN_001a39b0`): HP, MP, drive and munny.
//! - Drawn toward the player (`FUN_001c53a8`): the offset from the player's centre turns about the
//!   vertical axis by `min(30 / remaining distance, 0.593)` per frame while its length shrinks
//!   linearly to 0 over the 40 frames; the draw size follows the same fraction.
//! - The look: `PRIZE`'s effect file has one element whose particle program
//!   `pppRyjMegaBirthPrize` (draw handler 0x203378) draws every pool entry as a sprite of the shape
//!   its kind names, with the kind's colour and scale, scaled again by the size byte.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::SCALE;
use crate::assets::Disc;
use crate::enemy::Enemy;
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::Sora;
use crate::world::World;

/// Prize kinds (the 16-byte entries at 0x37b4b0 and the pickup rows at 0x37b540).
pub const KINDS: usize = 9;
/// Pool size: `FUN_001a3f98` -> `FUN_001a34f8(.., 0x100, 0x60)`.
pub const POOL: usize = 0x100;
/// Frames between floor checks at rest (`FUN_001c4eb0` and `FUN_001c52a8` set +0x45 to 0x1e).
const RECHECK: u8 = 0x1e;
/// `FUN_001c5060` plays the bounce sound while the bounce count is below 2.
const BOUNCE_SOUNDS: u8 = 2;
/// Shape kind of the point `FUN_0016b7d0` uses as an object's centre (`FUN_0016b558(obj, 1, -1)`).
const CENTRE_SHAPE: u8 = 1;
/// Shape kind of the lock-on point (target shapes, as `sora::target_points`).
const TARGET_SHAPE: u8 = 3;
/// Index of `pppRyjMegaBirthPrize` in the particle program table.
const PROGRAM: &str = "pppRyjMegaBirthPrize";

// ---------------------------------------------------------------------------------------------------
// Constants from the executable

/// A kind's entry at 0x37b4b0 + 16 * kind.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KindEntry {
    /// +0: the kind number the pool entry keeps (+0x14) and the pickup row it uses.
    pub id: u8,
    /// +4: added to the position's Y (down) for the floor sweep and the pickup test.
    pub radius: f32,
    /// +8: sound of the first two bounces (`FUN_001c5060`).
    pub bounce_se: u32,
    /// +0xc: sound at the pickup, placed at the player's centre (`FUN_001c5808`).
    pub pickup_se: u32,
}

/// A pickup row at 0x37b540 + 8 * kind (`FUN_001a39b0`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pickup {
    /// +0: above 0, the player's glow effect for this many frames (`FUN_00175c28`; not ported).
    pub glow: f32,
    /// +4: HP added through the player's method +0x78.
    pub hp: u8,
    /// +5: MP added (`FUN_00186368` -> `FUN_001a0180`).
    pub mp: u8,
    /// +6: drive gauge units (`FUN_001788d8`, `FUN_001d4148`).
    pub drive: u8,
    /// +7: munny (`FUN_0014f2d8` the strip, `FUN_0023e620` the counter).
    pub munny: u8,
}

/// The floats the prize code reads (addresses in the field docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Consts {
    /// 0x37b480: the generator value's scale (2^-32).
    pub unit: f32,
    /// 0x37b484 / 0x37b488: X and Z launch speed = base + value * unit * span.
    pub xz_span: f32,
    pub xz_base: f32,
    /// 0x37b48c / 0x37b490: Y launch speed (Y down).
    pub y_span: f32,
    pub y_base: f32,
    /// 0x37b478 / 0x37b47c: pickup reach (horizontal) and height (`FUN_001a3700`).
    pub reach: f32,
    pub height: f32,
    pub kinds: [KindEntry; KINDS],
    pub pickups: [Pickup; KINDS],
    /// 0x37bf34: life in frames (`FUN_001c4eb0` -> +0x40).
    pub life: f32,
    /// 0x37bf44: slowest landing speed that still bounces.
    pub bounce_min: f32,
    /// 0x37bf48: how far the mover's sideways result may differ from the velocity.
    pub wall_eps: f32,
    /// 0x37bf4c / 0x37bf50: gravity per frame and the fall speed cap.
    pub gravity: f32,
    pub fall_max: f32,
    /// 0x37bf54: sideways velocity factor per frame.
    pub friction: f32,
    /// 0x37bf58: the bounce factor `FUN_001c5248` passes.
    pub bounce: f32,
    /// 0x37bf5c / 0x37bf60: frames of the closing shrink and the size scale.
    pub fade: f32,
    pub fade_scale: f32,
    /// 0x37bf64 / 0x37bf68 / 0x37bf6c: the draw-in length, turn numerator and size scale.
    pub absorb_len: f32,
    pub absorb_turn: f32,
    pub absorb_scale: f32,
    /// 0x37bf7c: frames of the draw-in (`FUN_001c5808` mode 0).
    pub absorb_time: f32,
    /// 0x37bf88: largest turn per frame (copied to 0x1d5e2c4 by `FUN_001c59b0`).
    pub turn_max: f32,
    /// The draw-in of mode 1 (`FUN_001a40b8` -> `FUN_001a3ad8` -> `FUN_001c5808(.., 1)`, update
    /// `LAB_001c5748` -> `FUN_001c5588`): 0x37bf70 length, 0x37bf74 turn numerator, 0x37bf78 size
    /// scale, 0x37bf80 frames, 0x37bf84 largest turn (copied to 0x1d5e2c0 by `FUN_001c59b0`).
    pub absorb1_len: f32,
    pub absorb1_turn: f32,
    pub absorb1_scale: f32,
    pub absorb1_time: f32,
    pub turn_max1: f32,
    /// 0x37bc04: the drive multiplier `FUN_001ba040` returns outside its special case.
    pub drive_scale: f32,
}

/// Bytes at a virtual address of the executable, through its program headers.
fn exe_bytes(elf: &[u8], va: u32, len: usize) -> Option<&[u8]> {
    let u16_at = |o: usize| elf.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |o: usize| elf.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if elf.get(0..4)? != b"\x7fELF" {
        return None;
    }
    let (phoff, phentsize, phnum) = (u32_at(0x1c)? as usize, u16_at(0x2a)?, u16_at(0x2c)?);
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        let (kind, off, vaddr, filesz) = (u32_at(p)?, u32_at(p + 4)?, u32_at(p + 8)?, u32_at(p + 16)?);
        if kind == 1 && va >= vaddr && (va - vaddr) as usize + len <= filesz as usize {
            let start = off as usize + (va - vaddr) as usize;
            return elf.get(start..start + len);
        }
    }
    None
}

impl Consts {
    pub fn from_exe(elf: &[u8]) -> Option<Consts> {
        let w = |va: u32| exe_bytes(elf, va, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let f = |va: u32| w(va).map(f32::from_bits);
        let b = |va: u32| exe_bytes(elf, va, 1).map(|x| x[0]);
        let mut kinds = [KindEntry::default(); KINDS];
        let mut pickups = [Pickup::default(); KINDS];
        for k in 0..KINDS as u32 {
            let e = 0x37b4b0 + 16 * k;
            kinds[k as usize] = KindEntry { id: w(e)? as u8, radius: f(e + 4)?, bounce_se: w(e + 8)?, pickup_se: w(e + 12)? };
            let p = 0x37b540 + 8 * k;
            pickups[k as usize] = Pickup { glow: f(p)?, hp: b(p + 4)?, mp: b(p + 5)?, drive: b(p + 6)?, munny: b(p + 7)? };
        }
        Some(Consts {
            unit: f(0x37b480)?,
            xz_span: f(0x37b484)?,
            xz_base: f(0x37b488)?,
            y_span: f(0x37b48c)?,
            y_base: f(0x37b490)?,
            reach: f(0x37b478)?,
            height: f(0x37b47c)?,
            kinds,
            pickups,
            life: f(0x37bf34)?,
            bounce_min: f(0x37bf44)?,
            wall_eps: f(0x37bf48)?,
            gravity: f(0x37bf4c)?,
            fall_max: f(0x37bf50)?,
            friction: f(0x37bf54)?,
            bounce: f(0x37bf58)?,
            fade: f(0x37bf5c)?,
            fade_scale: f(0x37bf60)?,
            absorb_len: f(0x37bf64)?,
            absorb_turn: f(0x37bf68)?,
            absorb_scale: f(0x37bf6c)?,
            absorb_time: f(0x37bf7c)?,
            turn_max: f(0x37bf88)?,
            absorb1_len: f(0x37bf70)?,
            absorb1_turn: f(0x37bf74)?,
            absorb1_scale: f(0x37bf78)?,
            absorb1_time: f(0x37bf80)?,
            turn_max1: f(0x37bf84)?,
            drive_scale: f(0x37bc04)?,
        })
    }
}

// ---------------------------------------------------------------------------------------------------
// Rules (game space: centimetres, Y down, the app's Z negated)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Update `FUN_001c5288` (physics, then the check).
    Fall,
    /// Update `FUN_001c5388` (the check only).
    Rest,
    /// Update `LAB_001c5568` -> `FUN_001c53a8` (drawn into the player).
    Absorb,
}

/// One pool entry (the fields of the 0x60-byte record the rules use).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prize {
    /// +0x14: the kind (its entry's first byte).
    pub kind: u8,
    /// +0x00: position.
    pub pos: Vec3,
    /// +0x30..+0x38: velocity; while drawn in, the unit direction from the player's centre.
    pub vel: Vec3,
    /// +0x3c: while drawn in, the distance at the pickup.
    pub dist: f32,
    /// +0x40: frames left.
    pub life: f32,
    /// +0x44: bounces so far.
    pub bounces: u8,
    /// +0x45: frames to the next floor check at rest.
    pub recheck: u8,
    /// +0x28 bit 0: may be picked up (set once it falls).
    pub pickable: bool,
    /// +0x16: draw size 0..255 (the sprite is scaled by size / 255).
    pub size: u8,
    /// +0x17: shape frame (0xff: not started).
    pub frame: u8,
    pub phase: Phase,
    /// The draw-in's mode (`FUN_001c5808` third argument): 0 the pickup, 1 collect-all.
    pub mode: u8,
    /// +0x28 bit 8: set by the pickup dispatch `FUN_001a3848` while global flag 0x800 is up (`magic_rules::prize_dispatch`),
    /// never cleared; with it the prize goes to the player once +0x44 reaches 3.
    pub flag_8: bool,
}

/// What the floor sweep found: where the point ended and whether it stands on a floor (the mover's
/// result at 0x1d5e300 and bit 32 of its flags at 0x1d5e340 clear [inferred: the bit's meaning is
/// read from how the prize code uses it]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweep {
    pub end: Vec3,
    pub on_floor: bool,
}

/// `FUN_001c4eb0` + `FUN_001a3b28`: a new prize at `at` with the launch from the three generator
/// values (each read unsigned: the disassembly's sign fix-ups at 0x1a3b58, 0x1a3bac, 0x1a3c00).
pub fn launch(c: &Consts, kind: u8, v: [u32; 3], at: Vec3) -> Prize {
    let s = |x: u32| x as f32 * c.unit;
    let id = c.kinds.get(kind as usize).map_or(kind, |k| k.id);
    Prize {
        kind: id,
        pos: at,
        vel: Vec3::new(c.xz_base + s(v[0]) * c.xz_span, c.y_base + s(v[1]) * c.y_span, c.xz_base + s(v[2]) * c.xz_span),
        dist: 0.0,
        life: c.life,
        bounces: 0,
        recheck: RECHECK,
        pickable: false,
        size: 0xff,
        frame: 0xff,
        phase: Phase::Fall,
        mode: 0,
        flag_8: false,
    }
}

impl Prize {
    fn radius(&self, c: &Consts) -> f32 {
        c.kinds.get(self.kind as usize).map_or(0.0, |k| k.radius)
    }

    /// The point the sweep and the pickup test use: the position lowered by the radius (Y down).
    pub fn low_point(&self, c: &Consts) -> Vec3 {
        self.pos + Vec3::Y * self.radius(c)
    }
}

/// `FUN_001c5060`: one frame of flight. Returns the bounce sound to play, if any.
pub fn fall_physics(p: &mut Prize, c: &Consts, sweep: impl Fn(Vec3, Vec3) -> Sweep) -> Option<u32> {
    let start = p.low_point(c);
    let r = sweep(start, p.vel);
    let mut sound = None;
    if p.vel.y >= 0.0 && r.on_floor {
        if c.bounce_min < p.vel.y {
            p.vel.y *= c.bounce;
            if p.bounces < BOUNCE_SOUNDS {
                sound = c.kinds.get(p.kind as usize).map(|k| k.bounce_se);
            }
            p.bounces = p.bounces.saturating_add(1);
        } else {
            p.vel = Vec3::ZERO;
            p.phase = Phase::Rest;
        }
    }
    let d = r.end - start;
    if (d.x - p.vel.x).abs() > c.wall_eps {
        p.vel.x = -p.vel.x;
    }
    if (d.z - p.vel.z).abs() > c.wall_eps {
        p.vel.z = -p.vel.z;
    }
    p.pos = r.end - Vec3::Y * p.radius(c);
    p.vel.y = (p.vel.y + c.gravity).min(c.fall_max);
    p.vel.x *= c.friction;
    p.vel.z *= c.friction;
    if p.vel.y >= 0.0 {
        p.pickable = true;
    }
    sound
}

/// The front of `FUN_001c52a8`: the closing shrink, or (at rest) the floor recheck. True when the
/// pickup check runs this frame.
pub fn shrink_or_check(p: &mut Prize, c: &Consts, sweep: impl Fn(Vec3, Vec3) -> Sweep) -> bool {
    if p.life < c.fade {
        p.size = ((p.life * c.fade_scale) / c.fade) as i32 as u8;
        return false;
    }
    if p.phase == Phase::Rest {
        p.recheck = p.recheck.wrapping_sub(1);
        if p.recheck == 0 {
            if !sweep(p.low_point(c), p.vel).on_floor {
                p.phase = Phase::Fall;
            }
            p.recheck = RECHECK;
        }
    }
    true
}

/// `FUN_001a3848` as the tick asks it: true when the player takes the prize now. The global flag word decides first
/// (`magic_rules::prize_dispatch`: bit 3, or flag 8 with +0x44 at least 3, gives the prize to the player at once); else the
/// party loop's test, `party_reach`, answers. [inferred: when flag 8 gives the prize and the loop would too, the original
/// pays in the same frame twice; the app pays once.]
pub fn pickup_check(flags: u32, player_exists: bool, p: &Prize, party_reach: impl FnOnce() -> bool) -> bool {
    crate::magic_rules::prize_dispatch(flags, player_exists, p.flag_8, p.bounces).give_to_player || party_reach()
}

/// `FUN_001c5020`: a frame off the life; false when it is over.
pub fn age(p: &mut Prize) -> bool {
    p.life -= 1.0;
    p.life > 0.0
}

/// `FUN_001a3700` with the default reach: horizontal distance at most `reach + extra` and vertical
/// distance at most `height` between the prize's low point and the player's position (`FUN_0016b960`).
pub fn in_reach(c: &Consts, p: &Prize, player: Vec3, extra: f32) -> bool {
    reach_test(p.low_point(c) - player, c.reach, c.height, extra)
}

/// `FUN_001a3700`'s test on the offset from the member's position: vertical at most `height`,
/// horizontal at most `reach + extra`.
pub fn reach_test(d: Vec3, reach: f32, height: f32, extra: f32) -> bool {
    d.y.abs() <= height && Vec2::new(d.x, d.z).length() <= reach + extra
}

/// `FUN_001a3848` for the player (+0x588 bit 0x80): in Anti form (+0xb50 == 5) the HP kinds (0, 1)
/// are not taken.
pub fn player_takes(kind: u8, anti: bool) -> bool {
    !anti || kind > 1
}

/// `FUN_001c5808` mode 0: the prize turns toward the player's centre.
pub fn start_absorb(p: &mut Prize, c: &Consts, centre: Vec3) {
    start_absorb_mode(p, c, centre, 0);
}

/// `FUN_001c5808(entry, member, mode)`: the direction and distance from the member's centre, then
/// the mode's frames (mode 0: 0x37bf7c, update `FUN_001c53a8`; mode 1: 0x37bf80, update `FUN_001c5588`).
/// The pickable flag (+0x28 bit 0) is left as it was.
pub fn start_absorb_mode(p: &mut Prize, c: &Consts, centre: Vec3, mode: u8) {
    let off = p.pos - centre;
    p.dist = off.length();
    p.vel = off.normalize_or_zero();
    p.life = if mode == 1 { c.absorb1_time } else { c.absorb_time };
    p.mode = mode;
    p.phase = Phase::Absorb;
}

/// `FUN_001c53a8` (mode 0) / `FUN_001c5588` (mode 1, the same steps with its own constants): one
/// frame of the draw-in. False when it is over (life out).
pub fn absorb_step(p: &mut Prize, c: &Consts, centre: Vec3) -> bool {
    p.life -= 1.0;
    if p.life <= 0.0 {
        return false;
    }
    let (len, turn_num, turn_max, scale) =
        if p.mode == 1 { (c.absorb1_len, c.absorb1_turn, c.turn_max1, c.absorb1_scale) } else { (c.absorb_len, c.absorb_turn, c.turn_max, c.absorb_scale) };
    let frac = p.life / len;
    let turn = (turn_num / (p.dist * frac)).min(turn_max);
    p.vel = rot_y(turn, p.vel);
    p.pos = centre + p.vel * (p.dist * frac);
    p.size = (frac * scale) as i32 as u8;
    true
}

/// `FUN_0011b4d8` on the identity (`FUN_0011b460`), applied to a row vector: the rotation about the
/// vertical axis that `FUN_002fb628` builds (rows 0 and 2 carry x and z, row 1 is (0, 1, 0)).
/// The sign is pinned by the oracle test `absorb_turn_matches_the_game_rotation`.
pub fn rot_y(a: f32, v: Vec3) -> Vec3 {
    let (s, co) = a.sin_cos();
    Vec3::new(v.x * co + v.z * s, v.y, v.z * co - v.x * s)
}

/// Where the tick left the prize.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tick {
    pub alive: bool,
    pub bounce: Option<u32>,
    pub came_to_rest: bool,
    pub picked: bool,
}

/// One frame of a prize: `FUN_001c5248` (falling), `FUN_001c5388` (rest) or `FUN_001c53a8`.
/// `pick` is asked when the pickup check runs and returns the player's centre when it takes it.
pub fn tick(p: &mut Prize, c: &Consts, sweep: impl Fn(Vec3, Vec3) -> Sweep, centre: Option<Vec3>, mut pick: impl FnMut(&Prize) -> bool) -> Tick {
    let mut out = Tick { alive: true, bounce: None, came_to_rest: false, picked: false };
    if p.phase == Phase::Absorb {
        out.alive = centre.is_some_and(|ce| absorb_step(p, c, ce));
        return out;
    }
    if p.phase == Phase::Fall {
        out.bounce = fall_physics(p, c, &sweep);
        out.came_to_rest = p.phase == Phase::Rest;
    }
    if shrink_or_check(p, c, &sweep)
        && p.pickable
        && let Some(ce) = centre
        && pick(p)
    {
        start_absorb(p, c, ce);
        out.picked = true;
    }
    out.alive = age(p);
    out
}

/// `FUN_001788d8` for the player in gauge mode (+0x1af == 1): the units, times `sstm[0x78]` and the
/// ability block's +0x38 while the MP charge runs, times `FUN_001ba040`, as a whole number for
/// `FUN_001a0290`.
pub fn drive_gain(units: f32, charging: bool, sstm_78: f32, block_38: f32, scale: f32) -> i32 {
    let u = if charging { units * sstm_78 * block_38 } else { units };
    (u * scale) as i32
}

impl PrizeLib {
    /// `drive_gain` with this table's `sstm[0x78]` and the drive scale of `FUN_001ba040`: the whole gauge
    /// units `FUN_001788d8` hands to `FUN_001a0290` for `units` in gauge mode. The Damage Drive ability
    /// (`abilities::damage_drive_units`) uses it.
    pub fn gauge_units(&self, units: f32, charging: bool, block_38: f32) -> i32 {
        drive_gain(units, charging, self.sstm[1], block_38, self.consts.drive_scale)
    }
}

/// The pickup glow of the player (`FUN_00175c28` starts it, `FUN_00175cb0` counts it down): object
/// +0xae4 (the running effect's handle) and +0xae8 (frames left).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Glow {
    pub running: bool,
    pub left: f32,
}

impl Glow {
    /// `FUN_00175c28(frames, player)`: without a running glow (and the player's +0x160 bit 0x4000
    /// clear, `blocked` false) system effect 6 starts and the frames are set; otherwise the frames
    /// left are raised to `frames` when that is more. True when the effect starts.
    pub fn start(&mut self, frames: f32, blocked: bool) -> bool {
        if !self.running && !blocked {
            self.running = true;
            self.left = frames;
            return true;
        }
        if self.left < frames {
            self.left = frames;
        }
        false
    }

    /// `FUN_00175cb0`: a frame off a running glow; at 0 or less (or +0x160 bit 0x4000) the effect is
    /// stopped (`FUN_001d9f48`) and the handle cleared. True when it stops this frame.
    pub fn tick(&mut self, blocked: bool) -> bool {
        if !self.running {
            return false;
        }
        self.left -= 1.0;
        if blocked || self.left <= 0.0 {
            self.running = false;
            return true;
        }
        false
    }
}

/// `FUN_001d3f88(player, amount)` on a form record: the experience grows by `amount`; while the
/// level is below `cap` (`FUN_001d3bd0`, via `FUN_001d3c38`) and the experience reaches the current
/// level's `fmlv` row (`FUN_001d3b90(form, level)` +4), the row's amount is taken off and the level
/// rises (`FUN_001d3da8`); at the cap (or without a row) the experience is set to 0. Returns the new
/// level and experience and the levels reached, in order.
pub fn add_form_exp(levels: &kh2_tables::player::FormLevels, form: u8, level: u8, exp: i32, amount: i32, cap: u8) -> (u8, i32, Vec<u8>) {
    let (mut level, mut exp) = (level, exp.wrapping_add(amount));
    let mut ups = Vec::new();
    loop {
        let row = if level < cap { levels.get(form, level) } else { None };
        let Some(r) = row else {
            exp = 0;
            break;
        };
        if exp < r.exp {
            break;
        }
        level += 1;
        ups.push(level);
        exp -= r.exp;
    }
    (level, exp, ups)
}

/// Address of the growth chains `FUN_001d3f18` reads: `0x37c556 + 10 * form + 2 * level`, five ability
/// ids per form for the display levels 0..4 (0: none). Rows 1..4 are Valor (High Jump), Wisdom (Quick
/// Run), Master (Aerial Dodge) and Final (Glide).
pub const GROWTH_CHAINS: u32 = 0x37c556;

/// The growth chains of the drive forms (`GROWTH_CHAINS`), forms 0..=4, read from the executable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GrowthChains(pub [[u16; 5]; 5]);

impl GrowthChains {
    pub fn from_exe(elf: &[u8]) -> Option<GrowthChains> {
        let b = exe_bytes(elf, GROWTH_CHAINS, 50)?;
        let mut t = [[0u16; 5]; 5];
        for i in 0..25 {
            t[i / 5][i % 5] = u16::from_le_bytes([b[2 * i], b[2 * i + 1]]);
        }
        Some(GrowthChains(t))
    }

    /// The id at `(form, level)` (0 outside the table).
    pub fn at(&self, form: u8, level: u8) -> u16 {
        self.0.get(form as usize).and_then(|r| r.get(level as usize)).copied().unwrap_or(0)
    }
}

/// `FUN_001d3f18(list, n, form, level)`: in the ability list (bit 15 = equipped) the first slot whose id
/// is the chain's id one level below `level` (an empty slot when that is 0) takes the chain's id at
/// `level`, keeping its bit 15; at most one slot changes. Returns the chain's id at `level` whether or not
/// a slot matched (the disassembly loads it into v0 before the search; the decompiler shows no return).
#[cfg(test)]
pub fn growth_upgrade(list: &mut [u16], chains: &GrowthChains, form: u8, level: u8) -> u16 {
    let new = chains.at(form, level);
    let old = if level == 0 { chains.at(form.wrapping_sub(1), 4) } else { chains.at(form, level - 1) };
    if let Some(slot) = list.iter_mut().find(|s| **s & 0x7fff == old) {
        *slot = (*slot & 0x8000) | (new & 0x7fff);
    }
    new
}

/// What `FUN_001d3da8` does at one form level-up, worked out for the log, the obtained log and the
/// popups: `growth` is (the form record's new id, Sora's new id) when the row's display level rises above
/// the previous row's (`FUN_001d3f18` on the form's 16 slots at the display level and on Sora's 64 at one
/// below); `ability` is the row's +2 (`FUN_001a18d8(ability, 1, 0)`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FormLevelUp {
    pub level: u8,
    /// The row's display level (+1).
    pub display: u8,
    pub growth: Option<(u16, u16)>,
    pub ability: u16,
}

/// `FUN_001d3da8`'s choices at `level` (reached): the display levels of the row of `level` and the one
/// below, the chain ids, the row's ability.
pub fn form_level_up(levels: &kh2_tables::player::FormLevels, chains: &GrowthChains, form: u8, level: u8) -> FormLevelUp {
    let row = levels.get(form, level);
    let prev = level.checked_sub(1).and_then(|l| levels.get(form, l));
    let display = row.map_or(0, |r| r.display_level);
    let growth = (prev.map_or(0, |p| p.display_level) < display).then(|| (chains.at(form, display), chains.at(form, display.saturating_sub(1))));
    FormLevelUp { level, display, growth, ability: row.map_or(0, |r| r.ability) }
}

/// `FUN_001d4078` (from the enemies' hit method at 0x1849d8, after the hit's +0x2a byte is added to the
/// victim's +0xaf0): a hit whose attacker is the player in Valor Form (+0xb50 == 1) gives Valor 1
/// experience (`FUN_001d3f88(player, 1)`).
pub fn valor_hit_exp(player_form: u8) -> i32 {
    (player_form == kh2_tables::player::form::VALOR) as i32
}

/// `FUN_001d40e0(id, player)` (from the enemies' defeat method `FUN_00184a30`, when the object that
/// defeated it, +0xae4, is the player): in Wisdom Form an object id (`FUN_001699a8`) in 1000..=2011, in
/// Final Form one in 3000..=4008, gives the form 1 experience.
pub fn defeat_exp(player_form: u8, object_id: u32) -> i32 {
    use kh2_tables::player::form;
    let hit = match player_form {
        form::WISDOM => object_id.wrapping_sub(1000) < 0x3f4,
        form::FINAL => object_id.wrapping_sub(3000) < 0x3f1,
        _ => false,
    };
    hit as i32
}

/// One record of the obtained log (`FUN_001d0688`, 6 bytes in the ring of 32 at 0x341518 + 4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Obtained {
    /// +0, +1: the world and area bytes (0x33e830, 0x33e831).
    pub world: u8,
    pub area: u8,
    /// +2 low 15 bits.
    pub item: u16,
    /// +2 bit 15: set for a key item (`Flag0` bit 0), else set only when the item's "obtained once" bit
    /// (0x3414f0 by its `Slot`) is clear.
    pub new: bool,
    /// +4: who got it (the `FUN_001a1768` room, 1 for the form level-up gifts).
    pub to: u8,
    /// +5: the source: 0 an item object (`FUN_001bd1d8`), 3 a level-up (`FUN_001b53d0`), 4 (`FUN_001be500`),
    /// 5 a form level-up (`FUN_001d3da8`), 6 a summon level-up (`FUN_001d4fa0`).
    pub source: u8,
}

/// Ring size of the obtained log (`FUN_001d0760`: index + 1 modulo 32).
pub const OBTAINED_LOG: usize = 32;

/// The obtained log (`FUN_001d0828` -> `FUN_001d0760(0x341518, ..)`): the index word, then 32 records;
/// each new record overwrites the one at the index, which then moves on modulo 32. It lives in the save
/// (0x341518 = save + 0x16b8); here it is kept beside `Progress` (docs/systems/combat-effects/prizes.md).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct ObtainedLog {
    pub next: usize,
    pub records: [Obtained; OBTAINED_LOG],
}

impl ObtainedLog {
    /// `FUN_001d0760`: writes `r` at the index and moves the index on.
    pub fn push(&mut self, r: Obtained) {
        let i = self.next % OBTAINED_LOG;
        self.records[i] = r;
        self.next = (i + 1) % OBTAINED_LOG;
    }
}

/// `FUN_001d0688`'s record. `key` is the item row's `Flag0` bit 0, `obtained_bit` its "obtained once" bit
/// at the time of the call.
pub fn obtained_record(place: (u8, u8), item: u16, key: bool, obtained_bit: bool, to: u8, source: u8) -> Obtained {
    Obtained { world: place.0, area: place.1, item: item & 0x7fff, new: key || !obtained_bit, to, source }
}

/// The world and area bytes (0x33e830, 0x33e831) of an area name such as `tt04` [inferred: the world is the
/// code's place in the game's world table, the area the number].
pub fn place_bytes(area: &str) -> (u8, u8) {
    let world = area.get(..2).and_then(|w| kh2_image::text::WORLDS.iter().position(|x| *x == w)).unwrap_or(0) as u8;
    let n = area.get(2..).and_then(|n| n.parse::<u8>().ok()).unwrap_or(0);
    (world, n)
}

/// The item message `FUN_001a1f40(item, -1)` builds and `FUN_00138498` queues (byte 9 = an icon, its
/// argument the item's +0x17 `Icon2`, then the name text, message `+8`). `hud_banner` shows the queue.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemObtained {
    pub item: u16,
    pub icon: u8,
    pub name: u16,
}

/// Experience for the player's current form from outside the prizes (`FUN_001d4078` Valor hits,
/// `FUN_001d40e0` Wisdom / Final defeats): applied by `FUN_001d3f88` when the player is in `form`. The
/// rules are `valor_hit_exp` and `defeat_exp`; the sender is the hit / defeat code.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormExpGain {
    pub form: u8,
    pub amount: i32,
}

// ---------------------------------------------------------------------------------------------------
// Item objects (FUN_001bd490): dropped items as objects PRIZE_BOX_YELLOW / RED / BLUE / GREEN

/// The bounce sound of an item (`FUN_001bd7a0`: `FUN_002c32b8(0xe, position)` for the first two).
pub const ITEM_BOUNCE_SE: u32 = 0xe;
/// The sound when an item is taken (state 13's enter `FUN_001bd848`: sound 0x12 at the item).
pub const ITEM_TAKE_SE: u32 = 0x12;
/// Rows of the box table at 0x37bcb0 (12 of 8 bytes; the floats of 0x37bd10 follow).
pub const BOX_ROWS: usize = 12;

/// The numbers of the item object code, read from the executable.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemConsts {
    /// 0x37bcb0 + 8 * n: the object (`00objentry` id) and its scale, by the item's byte +0x16 (`Icon1`).
    pub boxes: Vec<(u32, f32)>,
    /// 0x37bc54 / 0x37bc58 / 0x37bc5c: the yaw `FUN_001bd0d0` passes to `FUN_00168d30`: base + value * unit * span.
    pub yaw_unit: f32,
    pub yaw_span: f32,
    pub yaw_base: f32,
    /// 0x37bc6c (unit), 0x37bc70 / 0x37bc74 (X and Z span, base), 0x37bc78 / 0x37bc7c (Y span, base):
    /// the launch of state 12's enter `FUN_001bd580` (+0x20, +0x28, +0x24 in that order; Y down).
    pub unit: f32,
    pub xz_span: f32,
    pub xz_base: f32,
    pub y_span: f32,
    pub y_base: f32,
    /// 0x37bc80 / 0x37bc84 / 0x37bc88: the fall speed cap, gravity per frame and the sideways factor
    /// (`FUN_001bd700`, state 12's velocity).
    pub fall_max: f32,
    pub gravity: f32,
    pub friction: f32,
    /// 0x37bc8c / 0x37bc90: the slowest landing that still bounces and the bounce factor (`FUN_001bd7a0`).
    pub bounce_min: f32,
    pub bounce: f32,
    /// 0x37bc60 / 0x37bc64 / 0x37bc68: the draw-in's frames = distance * 0.05 held in 30..60 (`FUN_001bd1d8`).
    pub time_scale: f32,
    pub time_min: f32,
    pub time_max: f32,
    /// 0x37bc94 / 0x37bc98 / 0x37bc9c / 0x37bca0 / 0x37bca4 / 0x37bca8 / 0x37bcac: `FUN_001bd878`'s
    /// one, pi, half, arc height, turn per frame, shrink start and shrink scale.
    pub one: f32,
    pub pi: f32,
    pub half: f32,
    pub lift: f32,
    pub spin: f32,
    pub shrink_at: f32,
    pub shrink_scale: f32,
}

impl ItemConsts {
    pub fn from_exe(elf: &[u8]) -> Option<ItemConsts> {
        let w = |va: u32| exe_bytes(elf, va, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let f = |va: u32| w(va).map(f32::from_bits);
        let boxes = (0..BOX_ROWS as u32).map(|n| Some((w(0x37bcb0 + 8 * n)?, f(0x37bcb4 + 8 * n)?))).collect::<Option<Vec<_>>>()?;
        Some(ItemConsts {
            boxes,
            yaw_unit: f(0x37bc54)?,
            yaw_span: f(0x37bc58)?,
            yaw_base: f(0x37bc5c)?,
            unit: f(0x37bc6c)?,
            xz_span: f(0x37bc70)?,
            xz_base: f(0x37bc74)?,
            y_span: f(0x37bc78)?,
            y_base: f(0x37bc7c)?,
            fall_max: f(0x37bc80)?,
            gravity: f(0x37bc84)?,
            friction: f(0x37bc88)?,
            bounce_min: f(0x37bc8c)?,
            bounce: f(0x37bc90)?,
            time_scale: f(0x37bc60)?,
            time_min: f(0x37bc64)?,
            time_max: f(0x37bc68)?,
            one: f(0x37bc94)?,
            pi: f(0x37bc98)?,
            half: f(0x37bc9c)?,
            lift: f(0x37bca0)?,
            spin: f(0x37bca4)?,
            shrink_at: f(0x37bca8)?,
            shrink_scale: f(0x37bcac)?,
        })
    }
}

/// The draw-in of a taken item (state 13, `FUN_001bd878`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemAbsorb {
    /// +0x9d0: the item's position minus the collector's centre at the pickup.
    pub offset: Vec3,
    /// +0x9bc / +0x9c0: frames left and frames in all.
    pub left: f32,
    pub total: f32,
}

/// An item object (the fields of the 0x9f0-byte object the rules use; game space, Y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    /// +0x9b4: the item id.
    pub id: u16,
    /// +0x540: position (the object's foot).
    pub pos: Vec3,
    /// +0x20..+0x28: velocity.
    pub vel: Vec3,
    /// The yaw `FUN_00168d30` is given; turned while drawn in (`FUN_0016a7f0`, [inferred: the facing]).
    pub yaw: f32,
    /// +0x3c: scale; +0x9b8 the scale it started with (the box table's).
    pub scale: f32,
    pub base_scale: f32,
    /// +0x108 bit 1 clear: `FUN_0019a4b0` lets it be taken (`FUN_001bd0d0` sets the bit, `FUN_001bd700`
    /// clears it once the item falls, state 13's enter sets it again).
    pub pickable: bool,
    /// +0x9e0 bit 0: came to rest (`FUN_001bd7a0`).
    pub rested: bool,
    /// +0x9e0 bit 2: taken (`FUN_001bd1d8`).
    pub taken: bool,
    /// +0x9e4: bounces so far.
    pub bounces: u32,
    pub absorb: Option<ItemAbsorb>,
}

/// `FUN_001bd0d0` + state 12's enter `FUN_001bd580`: a new item object at `at` with the four generator
/// values (yaw, then X, Z, Y of the launch; read unsigned as the decompilation's `(float)uint`).
pub fn item_launch(c: &ItemConsts, id: u16, scale: f32, v: [u32; 4], at: Vec3) -> Item {
    let s = |x: u32, unit: f32| x as f32 * unit;
    Item {
        id,
        pos: at,
        vel: Vec3::new(
            c.xz_base + s(v[1], c.unit) * c.xz_span,
            c.y_base + s(v[3], c.unit) * c.y_span,
            c.xz_base + s(v[2], c.unit) * c.xz_span,
        ),
        yaw: c.yaw_base + s(v[0], c.yaw_unit) * c.yaw_span,
        scale,
        base_scale: scale,
        pickable: false,
        rested: false,
        taken: false,
        bounces: 0,
        absorb: None,
    }
}

/// One frame of a falling item: `FUN_001bd700` (velocity: a falling item becomes pickable; gravity
/// to the cap; the sideways factor) moves it through the sweep, and a floor contact while falling
/// runs `FUN_001bd7a0` (faster than 2.5: bounce, the sound for the first two; else rest, velocity 0).
/// [inferred: the generic object mover that calls these two (state 12's base `FUN_0017b440`) is not
/// ported; the app's sweep stands in for it as for the prizes.] Returns the bounce sound, if any.
pub fn item_fall(o: &mut Item, c: &ItemConsts, sweep: impl Fn(Vec3, Vec3) -> Sweep) -> Option<u32> {
    if o.vel.y > 0.0 {
        o.pickable = true;
    }
    o.vel.y = (o.vel.y + c.gravity).min(c.fall_max);
    o.vel.x *= c.friction;
    o.vel.z *= c.friction;
    let r = sweep(o.pos, o.vel);
    o.pos = r.end;
    let mut sound = None;
    if r.on_floor && o.vel.y >= 0.0 {
        if c.bounce_min < o.vel.y {
            o.vel.y *= c.bounce;
            if o.bounces < 2 {
                sound = Some(ITEM_BOUNCE_SE);
            }
            o.bounces += 1;
        } else {
            o.rested = true;
            o.vel = Vec3::ZERO;
        }
    }
    sound
}

/// The tail of `FUN_001bd1d8` / `FUN_001bd358` once the item is taken: the offset from the
/// collector's centre, the draw-in's frames (`|offset| * 0.05` held in 30..60) and state 13 (not
/// pickable any more).
pub fn item_take(o: &mut Item, c: &ItemConsts, centre: Vec3) {
    o.taken = true;
    o.pickable = false;
    let offset = o.pos - centre;
    let t = (offset.length() * c.time_scale).clamp(c.time_min, c.time_max);
    o.absorb = Some(ItemAbsorb { offset, left: t, total: t });
}

/// `FUN_001bd878`: one frame of the draw-in. `g = sin((1 - left / total) * pi / 2)`, `f = 1 - g`: the
/// item sits at the centre plus the offset times `f`, raised by `sin(f * pi) * 50`; its yaw grows by
/// `g * 0.5236`; below `f` 0.1 its scale is the start scale times `f * 10`. False when the frames run
/// out (the item is then destroyed after its message, `FUN_001a1f40` + `FUN_00138498`).
pub fn item_absorb_step(o: &mut Item, c: &ItemConsts, centre: Vec3) -> bool {
    let Some(a) = o.absorb.as_mut() else { return false };
    a.left -= 1.0;
    if a.left <= 0.0 {
        return false;
    }
    let g = ((c.one - a.left / a.total) * c.pi * c.half).sin();
    let f = c.one - g;
    let mut off = a.offset * f;
    off.y -= (f * c.pi).sin() * c.lift;
    o.yaw += (c.one - f) * c.spin;
    o.pos = centre + off;
    if f < c.shrink_at {
        o.scale = o.base_scale * f * c.shrink_scale;
    }
    true
}

// ---------------------------------------------------------------------------------------------------
// App side

/// A drop's prize objects, written by `enemy_ai` when host 1.92 runs.
#[derive(Message, Clone, Debug)]
pub struct PrizeDrop {
    /// The enemy that dropped them (its lock-on point is the drop point).
    pub from: Entity,
    pub row: u16,
    pub launches: Vec<(u8, [u32; 3])>,
}

/// Munny picked up (the strip of `hud_banner` shows it).
#[derive(Message, Clone, Copy, Debug)]
pub struct MunnyGained {
    pub amount: u32,
}

/// A drop's item objects (`FUN_001bd490` per item that won its roll), written by `enemy_ai`.
#[derive(Message, Clone, Debug)]
pub struct ItemDrop {
    /// The enemy that dropped them (its lock-on point is the drop point, as for the prizes).
    pub from: Entity,
    /// Item id and the four generator values of its object (`enemy_host::Drop::item_launches`).
    pub launches: Vec<(u16, [u32; 4])>,
}

/// Host 6.31 (`FUN_002eece8`): every pickable prize is drawn into the player in mode 1 and paid
/// (`FUN_001a40b8`), every item object is handed to the player (`FUN_001bda98`).
#[derive(Message, Clone, Copy, Debug)]
pub struct CollectAll;

/// The player's pickup glow (`Glow`) and the handle of its effect (the player's +0xae4).
#[derive(Resource, Default)]
pub struct PickupGlow(pub Glow, pub Option<crate::fx::FxHandle>);

/// The system effect the pickup glow starts (`FUN_00175c28`: `FUN_001db868(.., 6, ..)` on the system file 0x1c954d8).
pub const GLOW_EFFECT: u16 = 6;

/// The kind's look from `pppRyjMegaBirthPrize`'s parameters (+4 + 12 * kind).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KindLook {
    /// +0: the shape (0xffff: none).
    pub shape: u32,
    /// +4..+6: colour.
    pub rgb: [u8; 3],
    /// +8: scale.
    pub scale: f32,
}

/// The program's per-kind parameters.
pub fn kind_looks(params: &[u8]) -> Vec<KindLook> {
    (0..10)
        .map_while(|k| {
            let o = 4 + 12 * k;
            let r = params.get(o..o + 12)?;
            Some(KindLook {
                shape: u32::from_le_bytes([r[0], r[1], r[2], r[3]]),
                rgb: [r[4], r[5], r[6]],
                scale: f32::from_le_bytes([r[8], r[9], r[10], r[11]]),
            })
        })
        .collect()
}

/// One sprite of a frame ready to draw: its quad (local units, Y up) and the material of its own
/// texture (TEX0 with its palette offset) and blend: the sprites of a frame differ in both (the HP
/// orb mixes an additive 0x48 sprite and a blend 0x44 one).
pub(crate) struct SpriteDraw {
    pub(crate) mesh: Handle<Mesh>,
    pub(crate) material: Handle<StandardMaterial>,
}

/// A sprite frame: its sprites in draw order.
pub(crate) struct FrameDraw {
    pub(crate) sprites: Vec<SpriteDraw>,
}

#[derive(Default)]
pub(crate) struct KindDraw {
    pub(crate) scale: f32,
    /// Frames of the kind's shape; `loops` when the last frame's flag has 0x80.
    pub(crate) frames: Vec<FrameDraw>,
    pub(crate) loops: bool,
}

#[derive(Resource)]
pub struct PrizeLib {
    pub consts: Consts,
    /// `sstm` +0x70 (form timer per unit), +0x78 (drive gain while charging), +0x7c (charge per MP).
    sstm: [f32; 3],
    draws: Vec<KindDraw>,
    /// The growth chains of `FUN_001d3f18` (0x37c556).
    pub chains: GrowthChains,
}

#[derive(Component)]
pub struct PrizeObj {
    pub p: Prize,
    shown: Option<usize>,
    /// The sprite entities of the frame shown (children of the prize entity).
    kids: Vec<Entity>,
}

/// The live prizes (the pool's used slots).
#[derive(Resource, Default)]
pub struct PrizePool {
    pub live: usize,
}

impl PrizePool {
    pub fn free(&self) -> usize {
        POOL.saturating_sub(self.live)
    }
}

pub struct PrizePlugin;

impl Plugin for PrizePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PrizeDrop>()
            .add_message::<MunnyGained>()
            .add_message::<ItemDrop>()
            .add_message::<CollectAll>()
            .add_message::<ItemObtained>()
            .add_message::<FormExpGain>()
            .init_resource::<ObtainedLog>()
            .init_resource::<PrizePool>()
            .init_resource::<PickupGlow>()
            .add_systems(FixedUpdate, unload_prizes.in_set(Sim::EffectTick));
        let Some(disc) = app.world().get_resource::<Disc>() else { return };
        let Some(elf) = disc.read_root("SLUS_210.05") else {
            warn!("prize: SLUS_210.05 is not readable; drops make no prize or item objects");
            return;
        };
        let items = ItemLib::load(disc, &elf);
        let consts = Consts::from_exe(&elf);
        let sstm = {
            let mut a = disc.0.lock().unwrap();
            kh2_tables::player::load_sstm(&mut a).ok().map_or([0.0; 3], |s| [s.f32(0x70), s.f32(0x78), s.f32(0x7c)])
        };
        let pax = consts.as_ref().and_then(|_| crate::fx::load(disc, "obj/PRIZE.mdlx", usize::MAX - 7, None));
        match items {
            Some(lib) => {
                app.insert_resource(lib).add_systems(FixedUpdate, (item_test, spawn_items, run_items).chain().in_set(Sim::EffectTick)).add_systems(Update, show_items);
            }
            None => warn!("prize: the item object constants or the item table are not readable; dropped items make no objects"),
        }
        let Some(consts) = consts else {
            warn!("prize: the prize constants are not readable from SLUS_210.05; drops make no prize objects");
            return;
        };
        if pax.is_none() {
            warn!("prize: obj/PRIZE.mdlx has no readable effect file; prizes run but are not drawn");
        }
        app.insert_resource(PrizeLib { consts, sstm, draws: Vec::new(), chains: GrowthChains::from_exe(&elf).unwrap_or_default() })
            .insert_resource(PrizePax(pax.map(Arc::new)))
            .add_systems(Startup, build_draws)
            .add_systems(FixedUpdate, (prize_test, form_exp_test, spawn_drops, run_prizes).chain().in_set(Sim::EffectTick))
            .add_systems(Update, show_prizes);
    }
}

/// Every prize and item object.
type PrizeOrItem = Or<(With<PrizeObj>, With<ItemObj>)>;

/// The area's unload takes the prizes and item objects with it. The jump task's unload (`FUN_00104ce0` ->
/// `FUN_00168ca0` -> `FUN_001b8800`) frees every object of the list at 0x1d5d7e0 [inferred: the `PRIZE` manager
/// `DAT_0035e7a0` (object 0x5c3, whose pool holds the orbs and munny) and the item objects (type 0x12) are in that
/// list like the area's other objects; their own teardown was not read]. Here: the change of the task to `Loading`
/// despawns every prize and item object once per jump and empties the pool count; what a prize had not paid out yet
/// is lost, as it would be. An item already taken and being drawn in is in the inventory (`FUN_001bd1d8` gave it at the
/// pickup); only its message (`FUN_001a1f40(item, -1)`, sent when the draw-in ends) is still owed, so the unload sends
/// it before the object goes [inferred: the message queue outlives the area; the unload's own order was not read].
fn unload_prizes(
    task: Option<Res<crate::area_jump::JumpTask>>,
    mut done: Local<bool>,
    objects: Query<(Entity, Option<&ItemObj>), PrizeOrItem>,
    lib: Option<Res<ItemLib>>,
    mut obtained: MessageWriter<ItemObtained>,
    mut pool: ResMut<PrizePool>,
    mut commands: Commands,
) {
    let loading = task.is_some_and(|t| matches!(*t, crate::area_jump::JumpTask::Loading { .. }));
    if !loading {
        *done = false;
        return;
    }
    if *done {
        return;
    }
    *done = true;
    let n = objects.iter().count();
    for (e, item) in &objects {
        if let Some(it) = item.filter(|it| it.o.absorb.is_some()) {
            let (name, icon) = lib.as_ref().and_then(|l| l.names.get(&it.o.id).copied()).unwrap_or((0, 0));
            obtained.write(ItemObtained { item: it.o.id, icon, name });
            info!("prize: item {:#x} was being drawn in at the unload: its message is sent, not lost", it.o.id);
        }
        commands.entity(e).despawn();
    }
    pool.live = 0;
    if n > 0 {
        info!("prize: area unload (FUN_00104ce0): {n} prize and item objects removed");
    }
}

#[derive(Resource)]
struct PrizePax(Option<Arc<crate::fx::LoadedPax>>);

/// GS ALPHA register to a Bevy blend (as `fx`'s effect materials).
fn alpha_mode(alpha: u8) -> AlphaMode {
    match (alpha & 3, (alpha >> 2) & 3, (alpha >> 6) & 3) {
        (0, 2, 1) => AlphaMode::Add,
        (2, 0, 1) => AlphaMode::Multiply,
        _ => AlphaMode::Blend,
    }
}

/// The sprite meshes of each kind's shape: one mesh and material per sprite
/// of each frame (quads in local units, X right and Y up, the vertex colour the sprite's colour times the kind's, 0x80 = 1.0).
fn build_draws(
    mut lib: ResMut<PrizeLib>,
    pax: Res<PrizePax>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(pax) = &pax.0 else { return };
    let Some((draws, looks, shapes)) = kind_draws(pax, KINDS, &mut meshes, &mut materials, &mut images) else {
        warn!("prize: obj/PRIZE.mdlx has no {PROGRAM} program; prizes are not drawn");
        return;
    };
    info!(
        "prize: {} drawn from obj/PRIZE.mdlx ({} shapes): kinds {:?}",
        PROGRAM,
        shapes,
        looks.iter().take(KINDS).map(|l| (l.shape, l.rgb, l.scale)).collect::<Vec<_>>()
    );
    lib.draws = draws;
}

/// The sprite meshes of the first `kinds` kinds of a prize effect file's `pppRyjMegaBirthPrize` program
/// (also the Struggle orbs' `obj/PRIZE_TT.mdlx`, `struggle.rs`): the draws, the kinds' looks, the shape count.
pub(crate) fn kind_draws(
    pax: &crate::fx::LoadedPax,
    kinds: usize,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) -> Option<(Vec<KindDraw>, Vec<KindLook>, usize)> {
    let program = kh2_fx::ppp::NAMES.iter().position(|n| *n == PROGRAM).unwrap_or(usize::MAX) as u32;
    let (dpd, params) = pax.dpds.iter().enumerate().find_map(|(i, d)| {
        d.particles.iter().flat_map(|p| p.groups.iter()).flat_map(|g| g.programs.iter()).find(|p| p.kind == program).map(|p| (i, p.params.clone()))
    })?;
    let looks = kind_looks(&params);
    let d = &pax.dpds[dpd];
    let mut textures: HashMap<u64, Handle<Image>> = HashMap::new();
    let mut draws = Vec::new();
    for k in 0..kinds {
        let Some(look) = looks.get(k).copied() else {
            draws.push(KindDraw::default());
            continue;
        };
        let Some(shape) = d.shapes.get(look.shape as usize) else {
            draws.push(KindDraw::default());
            continue;
        };
        let mut frames = Vec::new();
        for f in &shape.frames {
            let mut sprites = Vec::new();
            for (order, s) in f.sprites.iter().enumerate() {
                // The sprite's own TEX0 (the frame's plus its palette offset) and blend.
                let tex0 = s.tex0(f.tex0);
                let tex = textures
                    .entry(tex0)
                    .or_insert_with(|| {
                        let (w, h, rgba) = pax.vram[dpd].texture8(tex0);
                        let mut img = Image::new(
                            Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
                            TextureDimension::D2,
                            rgba,
                            TextureFormat::Rgba8UnormSrgb,
                            RenderAssetUsages::default(),
                        );
                        img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                            address_mode_u: ImageAddressMode::Repeat,
                            address_mode_v: ImageAddressMode::Repeat,
                            ..ImageSamplerDescriptor::linear()
                        });
                        images.add(img)
                    })
                    .clone();
                // The texture rectangle as fractions of the texture (raw / 4096), the normalised ST the
                // game's draw hands to the GS (`kh2_fx::shape::Sprite::st`).
                let [u, v, w, h] = s.st;
                let uvs = [[u, v], [u + w, v], [u, v + h], [u + w, v + h]];
                let (mut pos, mut uv, mut col) = (Vec::new(), Vec::new(), Vec::new());
                for (c, t) in s.corners.iter().zip(uvs) {
                    pos.push([c[0], -c[1], 0.0]);
                    uv.push([t[0], t[1]]);
                    let ch = |i: usize| s.rgba[i] as f32 / 128.0 * look.rgb.get(i).map_or(1.0, |x| *x as f32 / 128.0);
                    col.push([ch(0), ch(1), ch(2), s.rgba[3] as f32 / 128.0]);
                }
                let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(col))
                    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 1, 3, 2]));
                // The sprites of a frame are drawn in order on the same plane: the bias keeps that order.
                let material = materials.add(StandardMaterial {
                    base_color_texture: Some(tex),
                    unlit: true,
                    cull_mode: None,
                    double_sided: true,
                    alpha_mode: alpha_mode(s.alpha),
                    depth_bias: order as f32,
                    ..default()
                });
                sprites.push(SpriteDraw { mesh: meshes.add(mesh), material });
            }
            frames.push(FrameDraw { sprites });
        }
        let loops = shape.frames.last().is_some_and(|f| f.flag & 0x80 != 0);
        draws.push(KindDraw { scale: look.scale, frames, loops });
    }
    Some((draws, looks, d.shapes.len()))
}

/// App (cm, Y up) and game space (Y and Z negated) are each other's mirror.
fn flip(v: Vec3) -> Vec3 {
    Vec3::new(v.x, -v.y, -v.z)
}

/// The floor sweep of a point in game space over the app's collision: sideways the point slides as
/// a ball of the kind's radius against walls, downward it stops on the highest floor it crosses
/// [inferred: the game uses its character mover `FUN_00143840`, not ported; this keeps its results:
/// the end point and whether it stands on a floor].
pub(crate) fn sweep(world: Option<&World>, radius: f32, start: Vec3, vel: Vec3) -> Sweep {
    let Some(w) = world else { return Sweep { end: start + vel, on_floor: false } };
    let (s, v) = (flip(start), flip(vel));
    let mut p = s;
    let side = Vec3::new(v.x, 0.0, v.z);
    if side.length_squared() > 1e-6 {
        w.slide(&mut p, side, radius.max(1.0), 0.0, 2.0 * radius.max(1.0));
    }
    let next = p.y + v.y;
    let mut on_floor = false;
    if v.y <= 0.0 {
        match w.ground(p, p.y + 1.0, (p.y - next) + 1.0) {
            Some(g) if g.y >= next => {
                p.y = g.y;
                on_floor = true;
            }
            _ => p.y = next,
        }
    } else {
        p.y = next;
    }
    Sweep { end: flip(p), on_floor }
}

/// The first shape of `kind` of a body, on its bone (`FUN_0016b558` + `FUN_001894d0`), in app cm.
pub(crate) fn shape_point(anim: &Animator, body: &GlobalTransform, kind: u8) -> Option<Vec3> {
    let s = anim.rig.model.shapes.iter().find(|s| s.kind == kind && (kind != TARGET_SHAPE || s.argument & 1 == 0))?;
    let m = match s.bone {
        Some(b) => anim.bone_world(body, b),
        None => body.to_matrix(),
    };
    let mut p = m.transform_point3(Vec3::from(s.offset)) / SCALE;
    p.y += s.lift;
    Some(p)
}

fn log_on() -> bool {
    std::env::var("KH2_PRIZELOG").is_ok_and(|v| v != "0")
}

/// Makes the prize objects of this tick's drops at the dropping enemy's lock-on point.
fn spawn_drops(
    mut commands: Commands,
    lib: Res<PrizeLib>,
    mut pool: ResMut<PrizePool>,
    mut drops: MessageReader<PrizeDrop>,
    enemies: Query<(&Transform, &Enemy)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
) {
    for d in drops.read() {
        let Ok((t, e)) = enemies.get(d.from) else { continue };
        // FUN_0016b738: the lock-on point (FUN_0016b6e0), unless the line to it from the raised
        // position hits something [the line test is not ported].
        let at = bodies.get(e.body).ok().and_then(|(a, g)| shape_point(a, g, TARGET_SHAPE)).unwrap_or(t.translation / SCALE);
        let mut kinds = Vec::new();
        for &(kind, v) in &d.launches {
            let p = launch(&lib.consts, kind, v, flip(at));
            kinds.push(p.kind);
            pool.live += 1;
            commands.spawn((PrizeObj { p, shown: None, kids: Vec::new() }, Transform::from_translation(at * SCALE), Visibility::Hidden));
        }
        info!("prize: drop of row {} at {:.0?}: {} objects, kinds {:?}", d.row, at, kinds.len(), kinds);
    }
}

/// Harness aid, not game behaviour: `KH2_PRIZETEST=<kind>,<kind>,..[@<seconds>]` (default 4 s)
/// launches those kinds once from `KH2_PRIZETEST_DIST` cm (default 300) in front of Sora, 100 cm up,
/// with launch values from a fixed generator start, so each kind can be looked at without a fight.
fn prize_test(
    mut commands: Commands,
    lib: Res<PrizeLib>,
    mut pool: ResMut<PrizePool>,
    sora: Query<&Transform, With<Sora>>,
    time: Res<Time>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let Ok(v) = std::env::var("KH2_PRIZETEST") else {
        *done = true;
        return;
    };
    let (list, at) = v.split_once('@').map_or((v.as_str(), 4.0), |(l, t)| (l, t.parse().unwrap_or(4.0)));
    if time.elapsed_secs() < at {
        return;
    }
    *done = true;
    let Ok(t) = sora.single() else { return };
    let dist: f32 = std::env::var("KH2_PRIZETEST_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(300.0);
    let fwd = (t.rotation * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
    let at = t.translation / SCALE + fwd * dist + Vec3::Y * 100.0;
    let mut seed = 1u32;
    for kind in list.split(',').filter_map(|k| k.trim().parse::<u8>().ok()).filter(|&k| (k as usize) < KINDS) {
        let mut v = [0u32; 3];
        for x in &mut v {
            seed = seed.wrapping_mul(0x10dcd);
            *x = seed;
        }
        let mut p = launch(&lib.consts, kind, v, flip(at));
        // `KH2_PRIZETEST_STILL=1`: no sideways launch, so the prize lands where it starts.
        if std::env::var("KH2_PRIZETEST_STILL").is_ok_and(|v| v != "0") {
            p.vel.x = 0.0;
            p.vel.z = 0.0;
        }
        pool.live += 1;
        commands.spawn((PrizeObj { p, shown: None, kids: Vec::new() }, Transform::from_translation(at * SCALE), Visibility::Hidden));
        info!("prize: test launch of kind {kind} at {at:.0?}");
    }
}

/// Harness aid, not game behaviour: `KH2_FORMHITS=<n>[@<s>]` counts as `n` hits Sora lands at that time
/// (default 6 s), `KH2_FORMDEFEATS=<id>,..[@<s>]` as defeats of objects with those ids; each sends the
/// `FormExpGain` that `valor_hit_exp` / `defeat_exp` give for the form Sora is in (the hit and defeat code is
/// not wired to them yet).
fn form_exp_test(time: Res<Time>, drive: Option<Res<crate::drive::DriveState>>, mut out: MessageWriter<FormExpGain>, mut done: Local<[bool; 2]>) {
    let form = drive.as_ref().map_or(0, |d| d.object_form());
    for (i, key) in ["KH2_FORMHITS", "KH2_FORMDEFEATS"].into_iter().enumerate() {
        if done[i] {
            continue;
        }
        let Ok(v) = std::env::var(key) else {
            done[i] = true;
            continue;
        };
        let (list, at) = v.split_once('@').map_or((v.as_str(), 6.0), |(l, t)| (l, t.parse().unwrap_or(6.0)));
        if time.elapsed_secs() < at {
            continue;
        }
        done[i] = true;
        let gains: Vec<i32> = if i == 0 {
            (0..list.trim().parse::<u32>().unwrap_or(0)).map(|_| valor_hit_exp(form)).collect()
        } else {
            list.split(',').filter_map(|x| x.trim().parse::<u32>().ok()).map(|id| defeat_exp(form, id)).collect()
        };
        info!("prize: {key} in form {form}: experience {gains:?}");
        for amount in gains.into_iter().filter(|&a| a > 0) {
            out.write(FormExpGain { form, amount });
        }
    }
}

/// The player-side state a pickup changes or reads.
type PlayerState<'w> = (
    Option<ResMut<'w, crate::magic::MagicState>>,
    Option<ResMut<'w, crate::drive::DriveState>>,
    Option<ResMut<'w, crate::abilities::Abilities>>,
    Option<Res<'w, crate::drive::DriveRules>>,
);

/// The prizes' frame: flight, rest, pickup and draw-in; the pickup applies `FUN_001a39b0`. A
/// `CollectAll` (host 6.31, `FUN_001a40b8`) first draws every pickable prize in (mode 1) and pays it.
#[allow(clippy::too_many_arguments)]
fn run_prizes(
    mut commands: Commands,
    lib: Res<PrizeLib>,
    mut pool: ResMut<PrizePool>,
    world: Option<Res<World>>,
    mut prizes: Query<(Entity, &mut PrizeObj, &mut Transform), Without<Sora>>,
    mut sora: Query<(&Transform, &mut Sora)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
    (mut magic, mut drive, mut abilities, rules): PlayerState<'_>,
    mut progress: Option<ResMut<crate::progress::Progress>>,
    gflags: Option<Res<crate::magic_rules::GlobalFlags>>,
    (mut sounds, mut munny): (MessageWriter<crate::audio::PlaySe>, MessageWriter<MunnyGained>),
    (mut collect, mut glow): (MessageReader<CollectAll>, ResMut<PickupGlow>),
    (mut fx_start, mut fx_stop): (MessageWriter<crate::fx::StartBoundSystemEffect>, MessageWriter<crate::fx::StopEffect>),
    (mut log, area, mut gains): (ResMut<ObtainedLog>, Option<Res<crate::assets::CurrentArea>>, MessageReader<FormExpGain>),
) {
    let c = &lib.consts;
    let place = area.as_ref().map_or((0, 0), |a| place_bytes(&a.name));
    let Ok((st, mut s)) = sora.single_mut() else { return };
    // FUN_001d4078 / FUN_001d40e0: experience for the form the player is in (FUN_001d3f88 reads +0xb50).
    for g in gains.read() {
        if g.amount > 0 && drive.as_ref().is_some_and(|d| d.object_form() == g.form) {
            let note = apply_form_exp(
                (g.form, g.amount),
                (progress.as_deref_mut(), drive.as_deref_mut(), rules.as_deref(), abilities.as_deref_mut()),
                (&lib.chains, place, &mut log),
                (&mut fx_start, s.body),
            );
            info!("prize: form experience +{}{note}", g.amount);
        }
    }
    let feet = st.translation / SCALE;
    let centre = bodies.get(s.body).ok().and_then(|(a, g)| shape_point(a, g, CENTRE_SHAPE)).unwrap_or(feet);
    let (feet_g, centre_g) = (flip(feet), flip(centre));
    let anti = drive.as_ref().is_some_and(|d| d.object_form() == kh2_tables::player::form::ANTI);
    let draw = abilities.as_ref().map_or(0.0, |a| a.block().draw_radius);
    let alive_player = s.hp > 0;
    // FUN_00175cb0 (the player's update): the glow's frames run down; at 0 the effect is told to end
    // (FUN_001d9f48) and the handle cleared.
    if glow.0.tick(false) {
        if let Some(h) = glow.1.take() {
            fx_stop.write(crate::fx::StopEffect { handle: h, kill: false });
        }
        info!("prize: pickup glow (system effect {GLOW_EFFECT}) ends");
    }
    // FUN_001a40b8: every entry whose flag bit 0 is set, whatever its update (one already drawn in
    // starts again in mode 1 and pays again: FUN_001a3ad8 does not look). Not while the player is gone
    // [inferred: FUN_00175b68/FUN_0019a3f8 are not asked here, the member DAT_0035c998 is used as is].
    let collect_all = collect.read().count() > 0;
    let mut picked: Vec<(u8, u8)> = Vec::new();
    for (e, mut obj, mut t) in &mut prizes {
        let radius = c.kinds.get(obj.p.kind as usize).map_or(0.0, |k| k.radius);
        if collect_all && obj.p.pickable {
            start_absorb_mode(&mut obj.p, c, centre_g, 1);
            picked.push((obj.p.kind, 1));
            info!("prize: kind {} drawn toward Sora by collect-all from {:.0} away ({} frames)", obj.p.kind, obj.p.dist, c.absorb1_time);
            t.translation = flip(obj.p.pos) * SCALE;
            continue;
        }
        let was = obj.p.phase;
        let w = world.as_deref();
        // +0x588 bit 4 is taken as set for Sora, so Draw's ability-block +0x30 widens the reach
        // [inferred]; the status float +0x1c4 the reach also adds is taken as 0 (not traced).
        // FUN_001a3848 (the pickup check): the global flag word first (bit 0x800 sets the record's flag 8; bit 3, or
        // flag 8 with +0x44 at least 3, hands the prize to the player whatever the reach test says), else the party loop.
        let (flags, checked) = (gflags.as_deref().map_or(0, |g| g.0), std::cell::Cell::new(false));
        let out = tick(&mut obj.p, c, |a, v| sweep(w, radius, a, v), alive_player.then_some(centre_g), |p| {
            checked.set(true);
            pickup_check(flags, alive_player, p, || player_takes(p.kind, anti) && in_reach(c, p, feet_g, draw))
        });
        if checked.get() && crate::magic_rules::global_flag_800_active(flags, alive_player) {
            obj.p.flag_8 = true;
        }
        if let Some(se) = out.bounce {
            sounds.write(crate::audio::PlaySe::number(se, None).at(flip(obj.p.pos) * SCALE));
        }
        if out.came_to_rest && log_on() {
            info!("prize: kind {} came to rest at {:.0?} after {} bounces", obj.p.kind, flip(obj.p.pos), obj.p.bounces);
        }
        if was == Phase::Rest && obj.p.phase == Phase::Fall && log_on() {
            info!("prize: kind {} lost its floor and falls again", obj.p.kind);
        }
        if out.picked {
            picked.push((obj.p.kind, 0));
            info!("prize: kind {} drawn toward Sora from {:.0} away ({} frames)", obj.p.kind, obj.p.dist, c.absorb_time);
        }
        if !out.alive {
            if was != Phase::Absorb && obj.p.phase != Phase::Absorb && log_on() {
                info!("prize: kind {} ran out of time at {:.0?}", obj.p.kind, flip(obj.p.pos));
            }
            pool.live = pool.live.saturating_sub(1);
            commands.entity(e).despawn();
            continue;
        }
        t.translation = flip(obj.p.pos) * SCALE;
    }
    for (kind, mode) in picked {
        let k = kind as usize;
        let pick = c.pickups.get(k).copied().unwrap_or_default();
        // FUN_001c5808's sound, at the member's centre.
        if let Some(se) = c.kinds.get(k).map(|x| x.pickup_se) {
            sounds.write(crate::audio::PlaySe::number(se, None).at(centre * SCALE));
        }
        // FUN_001a39b0, in its order: glow, HP, MP, drive (and Master Form experience), munny.
        let mut glow_note = String::new();
        if pick.glow > 0.0 {
            // FUN_00175c28: system effect 6 bound to the player (his body model), its handle kept.
            let started = glow.0.start(pick.glow, false);
            if started {
                let handle = crate::fx::FxHandle::new();
                glow.1 = Some(handle);
                fx_start.write(crate::fx::StartBoundSystemEffect { id: GLOW_EFFECT, owner: s.body, handle });
            }
            glow_note = format!(" (glow {} for {:.0} frames)", if started { "starts" } else { "extended" }, glow.0.left);
        }
        let hp0 = s.hp;
        if pick.hp > 0 {
            // Method +0x78 = FUN_001792f0 -> FUN_00176c98 -> FUN_00186238: HP + amount, held at the maximum.
            s.hp = (s.hp + pick.hp as i32).min(s.stats.max_hp);
        }
        let (mp0, mut mp_note) = (s.mp, String::new());
        if pick.mp > 0 {
            // FUN_00186368 -> FUN_001a0180: during the MP charge the charge time drops by
            // amount * sstm[0x7c] (FUN_001a0540; at 0 the charge ends, which `magic` does on its
            // next tick [inferred one tick late]); otherwise MP + amount, held at the maximum.
            match magic.as_deref_mut().filter(|m| m.charge.active()) {
                Some(m) => {
                    m.charge.left = (m.charge.left - pick.mp as f32 * lib.sstm[2]).clamp(0.0, m.charge.total);
                    mp_note = format!(" (charge left {:.0})", m.charge.left);
                }
                None => s.mp = (s.mp + pick.mp as i32).min(s.stats.max_mp),
            }
        }
        let mut drive_note = String::new();
        let mut form_gains: Vec<(u8, i32)> = Vec::new();
        if pick.drive > 0
            && let Some(d) = drive.as_deref_mut()
        {
            let units = pick.drive as f32;
            if d.in_form {
                // FUN_001788d8, form mode: the timer gains units * sstm[0x70] (FUN_001a0358).
                let form = d.form;
                d.timer.add(form, units * lib.sstm[0]);
                drive_note = format!(" (form timer {:.0})", d.timer.left);
            } else {
                let charging = magic.as_deref().is_some_and(|m| m.charge.active());
                let block_38 = abilities.as_ref().map_or(1.0, |a| a.block().drive_gain_charging);
                d.points.gain(drive_gain(units, charging, lib.sstm[1], block_38, c.drive_scale));
                drive_note = format!(" (gauge {}.{:02})", d.points.whole, d.points.fraction);
            }
            // FUN_001d4148: in Master Form (+0xb50 == 3) the units are form experience (FUN_001d3f88).
            if d.object_form() == kh2_tables::player::form::MASTER {
                form_gains.push((kh2_tables::player::form::MASTER, pick.drive as i32));
            }
        }
        for (form, amount) in form_gains.drain(..) {
            drive_note += &apply_form_exp(
                (form, amount),
                (progress.as_deref_mut(), drive.as_deref_mut(), rules.as_deref(), abilities.as_deref_mut()),
                (&lib.chains, place, &mut log),
                (&mut fx_start, s.body),
            );
        }
        let mut munny_note = String::new();
        if pick.munny > 0 {
            munny.write(MunnyGained { amount: pick.munny as u32 });
            if let Some(pr) = progress.as_deref_mut() {
                munny_note = format!(" (munny {})", pr.add_to_wallet(pick.munny as u32));
            }
        }
        info!(
            "prize: picked up kind {k}{}: HP +{} MP +{} drive +{} munny +{} -> HP {hp0}->{} MP {mp0}->{}{glow_note}{mp_note}{drive_note}{munny_note}",
            if mode == 1 { " (collect-all)" } else { "" },
            pick.hp,
            pick.mp,
            pick.drive,
            pick.munny,
            s.hp,
            s.mp
        );
    }
}

/// `FUN_001d3f88` and what follows a level-up: `form_exp` on the game state; after any level-up the
/// effect 0x10 bound to the player (`FUN_001b5528`, once after the loop) and, while the player is in the
/// form, its ability list rebuilt at the new level (`FUN_0019fb98`'s stat rebuild, `drive::refresh_form_list`).
/// `FUN_001d5718` (progress flags 0x6a, 0x67, 0x68 by the form and summon levels) is not ported (`Progress`
/// has no flag setter). Returns the log note.
fn apply_form_exp(
    (form, amount): (u8, i32),
    (pr, d, rules, ab): (Option<&mut crate::progress::Progress>, Option<&mut crate::drive::DriveState>, Option<&crate::drive::DriveRules>, Option<&mut crate::abilities::Abilities>),
    (chains, place, log): (&GrowthChains, (u8, u8), &mut ObtainedLog),
    (fx, body): (&mut MessageWriter<crate::fx::StartBoundSystemEffect>, Entity),
) -> String {
    let (Some(pr), Some(d), Some(r)) = (pr, d, rules) else { return String::new() };
    let (mut note, ups) = form_exp(pr, d, &r.levels, chains, form, amount, place, log);
    if !ups.is_empty() {
        fx.write(crate::fx::StartBoundSystemEffect { id: LEVEL_UP_EFFECT, owner: body, handle: crate::fx::FxHandle::new() });
        note += &format!(" (level-up effect {LEVEL_UP_EFFECT:#x}");
        match ab.and_then(|ab| crate::drive::refresh_form_list(d, &r.levels, ab, form)) {
            Some(list) => note += &format!(", form list rebuilt {list:?})"),
            None => note += ")",
        }
    }
    note
}

/// The system effect `FUN_001b5528` starts on the player after a form level-up (`FUN_001db868(.., 0x10, 1,
/// 1, 0x2715, player)`).
pub const LEVEL_UP_EFFECT: u16 = 0x10;
/// The first line of the form's level-up popup (`FUN_001b5588(popup, 0xc3f2)` in `FUN_001d3da8`).
pub const FORM_POPUP_LINE: u16 = 0xc3f2;

/// `FUN_001d3f88(player, amount)` on the game state for the player's form `form`: the record's experience
/// and level (`add_form_exp`; the cap `FUN_001d3bd0` = 2 + the owned forms 1..4, 7 instead of 6), and for
/// each level reached `FUN_001d3da8` (`form_level_up`): the growth upgrade's ids and the row's ability
/// given to Sora (`FUN_001a18d8(ability, 1, 0)`), each written to the obtained log with source 5. The
/// minigame flag of `FUN_001b9bd8` (the manager `DAT_01d5dd20`) is taken as clear: the app has no such
/// manager. The record lists themselves (`FUN_001d3f18` on the form's 16 slots and Sora's 64) are not
/// written here (`Progress` has no setter for them; the form list the player uses is rebuilt from the level
/// by `drive::refresh_form_list`). Returns the log note and the level-ups.
#[allow(clippy::too_many_arguments)]
fn form_exp(
    pr: &mut crate::progress::Progress,
    d: &mut crate::drive::DriveState,
    levels: &kh2_tables::player::FormLevels,
    chains: &GrowthChains,
    form: u8,
    amount: i32,
    place: (u8, u8),
    log: &mut ObtainedLog,
) -> (String, Vec<FormLevelUp>) {
    let owned = (1..=4u8).filter(|&f| pr.owns_form(f)).count() as u32;
    let cap = kh2_tables::player::form_level_cap(owned);
    let (level0, exp0) = (pr.form_level(form), pr.form_exp(form) as i32);
    let (level, exp, ups) = add_form_exp(levels, form, level0, exp0, amount, cap);
    pr.set_form_level(form, level);
    pr.set_form_exp(form, exp as u32);
    let name = ["", "Valor", "Wisdom", "Master", "Final", "Anti"].get(form as usize).copied().unwrap_or("?");
    let mut note = format!(" ({name} Form experience {exp0}->{exp}, level {level0}");
    let mut done = Vec::new();
    for &l in &ups {
        let up = form_level_up(levels, chains, form, l);
        let key = |pr: &crate::progress::Progress, id: u16| pr.items().get(id).is_some_and(|i| i.is_key());
        note += &format!(" -> {l}");
        if let Some((form_id, sora_id)) = up.growth {
            // FUN_001d3f18 twice; the obtained log gets Sora's id (FUN_001d0828(id, 1, 5)), whose
            // "obtained once" bit is not set by the upgrade [the bit is read as clear]. The upgrade is
            // written to Sora's record, where a first level arrives unequipped as in the game.
            let lv = up.display.saturating_sub(1);
            let old = if lv == 0 { chains.at(form.wrapping_sub(1), 4) } else { chains.at(form, lv - 1) };
            let changed = pr
                .upgrade_ability(crate::progress::SORA, old, chains.at(form, lv))
                .map_or("no slot matches".to_string(), |s| format!("slot {s} -> {:#x}", chains.at(form, lv)));
            note += &format!(" (growth: form {form_id:#x}, Sora {sora_id:#x}: {changed})");
            log.push(obtained_record(place, sora_id, key(pr, sora_id), false, 1, 5));
        }
        if up.ability != 0 {
            let given = pr.give(up.ability, crate::progress::SORA, false);
            note += &format!(" (ability {:#x}{})", up.ability, if given { "" } else { " refused" });
            // FUN_001a18d8 set the obtained bit just before when it gave the item [inferred: `Progress`
            // does not show the bit; a refused gift leaves it as it was, read as clear].
            log.push(obtained_record(place, up.ability, key(pr, up.ability), given, 1, 5));
        }
        // FUN_001b5788 twice: the form's popup (title: the form's item name; lines: message 0xc3f2, the
        // form's growth id) and Sora's (title: message 0x37ba90[1]; lines: Sora's growth id, the ability),
        // each queued in the four-entry popup queue of DAT_003587e8 + 0x24f8 (FUN_00152ed0) when it has a
        // line. Not drawn (the popup object's draw was not followed): logged.
        let sora_lines: Vec<String> = up.growth.map(|g| g.1).into_iter().chain((up.ability != 0).then_some(up.ability)).filter(|&i| i != 0).map(|i| format!("{i:#x}")).collect();
        info!(
            "prize: form level-up popups (not drawn): {name} Form LV{l}: [msg {FORM_POPUP_LINE:#x}{}]; Sora: {}",
            up.growth.map_or(String::new(), |g| format!(", {:#x}", g.0)),
            if sora_lines.is_empty() { "no popup".to_string() } else { format!("[{}]", sora_lines.join(", ")) }
        );
        done.push(up);
    }
    if let Some(slot) = d.level.get_mut(form as usize) {
        *slot = level;
    }
    note += &format!(", cap {cap})");
    (note, done)
}

/// The draw (`FUN_00203378` per pool entry): the kind's shape frame as a sprite facing the camera
/// [inferred: the handler multiplies by the matrix at 0x1db1000, taken as the camera's rotation],
/// scaled by the kind's scale times size / 255. The frame steps once per tick; a looping shape
/// starts at a random frame (0xff), the app draws that from the launch.
fn show_prizes(
    mut commands: Commands,
    lib: Res<PrizeLib>,
    cam: Query<&GlobalTransform, crate::menu::preview::GameCamera>,
    mut prizes: Query<(Entity, &mut PrizeObj, &mut Transform, &mut Visibility)>,
    mut ticks: Local<f32>,
    time: Res<Time>,
) {
    let rot = cam.single().map(|g| g.compute_transform().rotation).unwrap_or_default();
    // Frames step at the game's 60 Hz.
    *ticks += time.delta_secs() * crate::data::LOGIC_HZ;
    let steps = ticks.floor() as u32;
    *ticks -= steps as f32;
    for (e, mut obj, mut t, mut vis) in &mut prizes {
        let Some(d) = lib.draws.get(obj.p.kind as usize).filter(|d| !d.frames.is_empty()) else { continue };
        let n = d.frames.len();
        let mut f = obj.p.frame;
        if f == 0xff {
            f = if d.loops { (obj.p.pos.x.to_bits() as usize % n.max(1)) as u8 } else { 0 };
        } else {
            for _ in 0..steps {
                let next = f as usize + 1;
                f = if next < n { next as u8 } else if d.loops { 0 } else { f };
            }
        }
        obj.p.frame = f;
        let size = d.scale * obj.p.size as f32 / 255.0;
        t.rotation = rot;
        t.scale = Vec3::splat(size * SCALE);
        *vis = if size > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if obj.shown != Some(f as usize) {
            let fr = &d.frames[(f as usize).min(n - 1)];
            for k in obj.kids.drain(..) {
                commands.entity(k).despawn();
            }
            for sp in &fr.sprites {
                let kid = commands.spawn((Mesh3d(sp.mesh.clone()), MeshMaterial3d(sp.material.clone()), Transform::default(), bevy::light::NotShadowCaster, ChildOf(e))).id();
                obj.kids.push(kid);
            }
            obj.shown = Some(f as usize);
        }
    }
}

// ---------------------------------------------------------------------------------------------------
// Item objects, app side

/// What the item objects need from the disc: the constants, each item's box row (item +0x16,
/// `Icon1`) and the four box models (`obj/PRIZE_BOX_*.mdlx`, objects 0x3b8..0x3bb of `00objentry`).
#[derive(Resource)]
pub struct ItemLib {
    pub consts: ItemConsts,
    /// 0x37b478 / 0x37b47c: the reach `FUN_001a3700` tests (the same as the prizes').
    reach: f32,
    height: f32,
    icon: HashMap<u16, u8>,
    /// The item's name message (+8) and its `Icon2` (+0x17), for its message (`FUN_001a1ee0`).
    names: HashMap<u16, (u16, u8)>,
    models: HashMap<u32, Arc<kh2_model::Model>>,
}

impl ItemLib {
    fn load(disc: &Disc, elf: &[u8]) -> Option<ItemLib> {
        let consts = ItemConsts::from_exe(elf)?;
        let f = |va: u32| exe_bytes(elf, va, 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let (reach, height) = (f(0x37b478)?, f(0x37b47c)?);
        let (items, objs) = {
            let mut a = disc.0.lock().ok()?;
            (
                kh2_tables::load::<kh2_tables::tables::Item>(&mut a, &kh2_tables::tables::ITEM).ok()?.rows,
                kh2_tables::load::<kh2_tables::tables::Objentry>(&mut a, &kh2_tables::tables::OBJENTRY).ok()?.rows,
            )
        };
        let icon = items.iter().map(|i| (i.id, i.icon1)).collect();
        let names = items.iter().map(|i| (i.id, (i.name, i.icon2))).collect();
        let mut models = HashMap::new();
        for &(obj, _) in &consts.boxes {
            if models.contains_key(&obj) {
                continue;
            }
            let Some(name) = objs.iter().find(|o| o.object_id == obj).map(|o| o.model_name.clone()) else {
                warn!("prize: item box object {obj:#x} has no 00objentry row");
                continue;
            };
            match disc.read(&format!("obj/{name}.mdlx")).and_then(|d| kh2_model::mdlx(&d)) {
                Some(m) => {
                    models.insert(obj, Arc::new(m));
                }
                None => warn!("prize: obj/{name}.mdlx is not a readable model; its items are not drawn"),
            }
        }
        info!("prize: item boxes {:?}, {} models", consts.boxes.iter().map(|(o, s)| format!("{o:#x} x{s}")).collect::<Vec<_>>(), models.len());
        Some(ItemLib { consts, reach, height, icon, names, models })
    }

    /// `FUN_001bd490`: the box row of an item (its byte +0x16), when the table has it.
    pub fn box_of(&self, id: u16) -> Option<(u32, f32)> {
        self.icon.get(&id).and_then(|&n| self.consts.boxes.get(n as usize)).copied()
    }
}

/// `FUN_001bd1d8`'s gift: `FUN_001a1768(id, 1, 1)` finds the place for one copy (Sora's party id;
/// a consumable into a free battle item slot, else the inventory up to 99), `FUN_001a18d8` gives it.
/// `None` when there is no room (the item is lost).
pub fn give_item(p: &mut crate::progress::Progress, id: u16) -> Option<u16> {
    let to = p.room(id, crate::progress::SORA, 1);
    (to != 0 && p.give(id, to, false)).then_some(to)
}

#[derive(Component)]
pub struct ItemObj {
    pub o: Item,
}

/// Makes this tick's dropped items (`FUN_001bd490`) at the dropping enemy's lock-on point.
#[allow(clippy::too_many_arguments)]
fn spawn_items(
    mut commands: Commands,
    lib: Res<ItemLib>,
    mut drops: MessageReader<ItemDrop>,
    enemies: Query<(&Transform, &Enemy)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    for d in drops.read() {
        let Ok((t, e)) = enemies.get(d.from) else { continue };
        // FUN_001a3cb0 passes the same point as to the prizes (FUN_0016b738's lock-on point).
        let at = bodies.get(e.body).ok().and_then(|(a, g)| shape_point(a, g, TARGET_SHAPE)).unwrap_or(t.translation / SCALE);
        for &(id, v) in &d.launches {
            spawn_item(&mut commands, &lib, id, v, at, (&mut meshes, &mut materials, &mut images));
        }
    }
}

/// One item object with its box model (app cm `at`).
fn spawn_item(
    commands: &mut Commands,
    lib: &ItemLib,
    id: u16,
    v: [u32; 4],
    at: Vec3,
    (meshes, materials, images): (&mut Assets<Mesh>, &mut Assets<StandardMaterial>, &mut Assets<Image>),
) {
    let Some((object, scale)) = lib.box_of(id) else {
        warn!("prize: item {id:#x} has no box row (item +0x16 outside the table); no object");
        return;
    };
    let o = item_launch(&lib.consts, id, scale, v, flip(at));
    let ent = commands.spawn((ItemObj { o }, Transform::from_translation(at * SCALE), Visibility::default())).id();
    if let Some(m) = lib.models.get(&object) {
        let model = commands.spawn((Transform::default(), Visibility::default(), ChildOf(ent))).id();
        crate::assets::spawn_model(commands, model, m, meshes, materials, images);
    }
    info!("prize: item {id:#x} dropped at {at:.0?} as object {object:#x} (scale {scale}), launch {:.1?}, yaw {:.2}", flip(o.vel), o.yaw);
}

/// Harness aids, not game behaviour: `KH2_ITEMTEST=<id>,<id>,..[@<seconds>]` (hex ids with `0x`,
/// default 4 s) drops those items once from `KH2_PRIZETEST_DIST` cm (default 300) in front of Sora,
/// 100 cm up, with launch values from a fixed generator start; `KH2_COLLECTALL=<seconds>` sends one
/// `CollectAll` (what host 6.31 does) at that time.
#[allow(clippy::too_many_arguments)]
fn item_test(
    mut commands: Commands,
    lib: Res<ItemLib>,
    sora: Query<&Transform, With<Sora>>,
    time: Res<Time>,
    mut done: Local<(bool, bool)>,
    mut collect: MessageWriter<CollectAll>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if !done.1
        && let Some(at) = std::env::var("KH2_COLLECTALL").ok().and_then(|v| v.parse::<f32>().ok())
        && time.elapsed_secs() >= at
    {
        done.1 = true;
        info!("prize: harness collect-all (host 6.31) at {at} s");
        collect.write(CollectAll);
    }
    if done.0 {
        return;
    }
    let Ok(v) = std::env::var("KH2_ITEMTEST") else {
        done.0 = true;
        return;
    };
    let (list, at) = v.split_once('@').map_or((v.as_str(), 4.0), |(l, t)| (l, t.parse().unwrap_or(4.0)));
    if time.elapsed_secs() < at {
        return;
    }
    done.0 = true;
    let Ok(t) = sora.single() else { return };
    let dist: f32 = std::env::var("KH2_PRIZETEST_DIST").ok().and_then(|d| d.parse().ok()).unwrap_or(300.0);
    let fwd = (t.rotation * Vec3::Z).with_y(0.0).normalize_or(Vec3::Z);
    let at = t.translation / SCALE + fwd * dist + Vec3::Y * 100.0;
    let mut seed = 7u32;
    let parse = |s: &str| s.trim().strip_prefix("0x").map_or_else(|| s.trim().parse::<u16>().ok(), |h| u16::from_str_radix(h, 16).ok());
    for id in list.split(',').filter_map(parse) {
        let mut v = [0u32; 4];
        for x in &mut v {
            seed = seed.wrapping_mul(0x10dcd);
            *x = seed;
        }
        spawn_item(&mut commands, &lib, id, v, at, (&mut meshes, &mut materials, &mut images));
    }
}

/// The item objects' frame: state 12 (fall, `FUN_001bd700` / `FUN_001bd7a0`), the pickup check
/// (`FUN_001bd358`, the object's vtable slot 5 [inferred: run once per frame]) and state 13 (the
/// draw-in, `FUN_001bd878`). A `CollectAll` hands every item to the player first (`FUN_001bda98` ->
/// `FUN_001bd1d8`, which looks at nothing: an item already drawn in is given again).
#[allow(clippy::too_many_arguments)]
fn run_items(
    mut commands: Commands,
    lib: Res<ItemLib>,
    world: Option<Res<World>>,
    mut items: Query<(Entity, &mut ItemObj)>,
    sora: Query<(&Transform, &Sora)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
    abilities: Option<Res<crate::abilities::Abilities>>,
    mut progress: Option<ResMut<crate::progress::Progress>>,
    gflags: Option<Res<crate::magic_rules::GlobalFlags>>,
    mut sounds: MessageWriter<crate::audio::PlaySe>,
    mut collect: MessageReader<CollectAll>,
    (mut log, area, mut obtained): (ResMut<ObtainedLog>, Option<Res<crate::assets::CurrentArea>>, MessageWriter<ItemObtained>),
) {
    let c = &lib.consts;
    let place = area.as_ref().map_or((0, 0), |a| place_bytes(&a.name));
    let Ok((st, s)) = sora.single() else { return };
    let feet = st.translation / SCALE;
    let centre = bodies.get(s.body).ok().and_then(|(a, g)| shape_point(a, g, CENTRE_SHAPE)).unwrap_or(feet);
    let (feet_g, centre_g) = (flip(feet), flip(centre));
    let draw = abilities.as_ref().map_or(0.0, |a| a.block().draw_radius);
    // FUN_0019a3f8(player) and +0x588 bit 0x2000000 clear [the bit is taken as clear].
    let alive_player = s.hp > 0;
    let collect_all = collect.read().count() > 0;
    for (e, mut it) in &mut items {
        let o = &mut it.o;
        if o.absorb.is_some() && !collect_all {
            // FUN_001bd878: the collector gone (FUN_0019a3f8) or the frames out: the message and the end.
            if !alive_player || !item_absorb_step(o, c, centre_g) {
                // FUN_001a1f40(item, -1): byte 9 with the item's Icon2, then its name; FUN_00138498 queues it.
                let (name, icon) = lib.names.get(&o.id).copied().unwrap_or((0, 0));
                obtained.write(ItemObtained { item: o.id, icon, name });
                info!("prize: item {:#x} reached Sora: its message (icon {icon}, name {name:#x}) goes to the queue", o.id);
                commands.entity(e).despawn();
            }
            continue;
        }
        if !collect_all {
            let w = world.as_deref();
            let was_rested = o.rested;
            if let Some(se) = item_fall(o, c, |a, v| sweep(w, 1.0, a, v)) {
                sounds.write(crate::audio::PlaySe::number(se, None).at(flip(o.pos) * SCALE));
            }
            if o.rested && !was_rested && log_on() {
                info!("prize: item {:#x} came to rest at {:.0?} after {} bounces", o.id, flip(o.pos), o.bounces);
            }
            // FUN_001bd358: the player alive, the item not taken and pickable (FUN_0019a4b0), within
            // reach of the player's position (FUN_001a3700; Draw as for the prizes [inferred]), or the
            // instant-collection flags (FUN_001905e8 / FUN_00190610, `item_taken_by_player`); the item is
            // no object 0x883 (FUN_001bdb10), so only the player is asked.
            let flags = gflags.as_deref().map_or(0, |g| g.0);
            let reach = reach_test(o.pos - feet_g, lib.reach, lib.height, draw);
            if !(alive_player && !o.taken && o.pickable && crate::magic_rules::item_taken_by_player(flags, alive_player, reach, false)) {
                continue;
            }
        }
        // FUN_001bd1d8 / the tail of FUN_001bd358: the room for one (FUN_001a1768 to the player's
        // party id, NeoStatus 1 = Sora) and the gift (FUN_001a18d8); with no room nothing is given
        // and the item is drawn in all the same. A gift goes into the obtained log (FUN_001d0828(item, room,
        // 0)); FUN_001a18d8 has just set a non-key item's "obtained once" bit, so its record is not new.
        let note = match progress.as_deref_mut() {
            Some(p) => match give_item(p, o.id) {
                Some(to) => {
                    let key = p.items().get(o.id).is_some_and(|i| i.is_key());
                    log.push(obtained_record(place, o.id, key, !key, to as u8, 0));
                    let room = if to == crate::progress::INVENTORY { "the inventory" } else { "Sora's battle item slots" };
                    format!("given to {room} (count {}), obtained log record {}", p.item_count(o.id), (log.next + OBTAINED_LOG - 1) % OBTAINED_LOG)
                }
                None => format!("refused (no room, count {}): drawn in all the same", p.item_count(o.id)),
            },
            None => "no game state".to_string(),
        };
        item_take(o, c, centre_g);
        // State 13's enter (FUN_001bd848): sound 0x12 at the item.
        sounds.write(crate::audio::PlaySe::number(ITEM_TAKE_SE, None).at(flip(o.pos) * SCALE));
        let frames = o.absorb.map_or(0.0, |a| a.total);
        info!("prize: item {:#x} taken by Sora{}: {note}; drawn in over {frames:.0} frames", o.id, if collect_all { " (collect-all)" } else { "" });
    }
}

/// The item objects' place, facing and size; the model faces its yaw [inferred: the game's yaw sign
/// against the app's mirrored Z].
fn show_items(mut items: Query<(&ItemObj, &mut Transform)>) {
    for (it, mut t) in &mut items {
        t.translation = flip(it.o.pos) * SCALE;
        t.rotation = Quat::from_rotation_y(-it.o.yaw);
        t.scale = Vec3::splat(it.o.scale * SCALE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consts() -> Consts {
        // Shapes of the real values (the tests below that need the real ones read the disc).
        let mut c = Consts {
            unit: 2f32.powi(-32),
            xz_span: 40.0,
            xz_base: -20.0,
            y_span: 20.0,
            y_base: -40.0,
            reach: 120.0,
            height: 30.0,
            life: 900.0,
            bounce_min: 5.0,
            wall_eps: 0.1,
            gravity: 3.0,
            fall_max: 50.0,
            friction: 0.95,
            bounce: -0.8,
            fade: 12.0,
            fade_scale: 255.0,
            absorb_len: 40.0,
            absorb_turn: 30.0,
            absorb_scale: 255.0,
            absorb_time: 40.0,
            turn_max: 0.6,
            drive_scale: 1.0,
            ..Default::default()
        };
        for k in 0..KINDS {
            c.kinds[k] = KindEntry { id: k as u8, radius: 10.0, bounce_se: 12, pickup_se: 16 };
        }
        c
    }

    /// A flat floor at game Y = 0 (Y down: the floor is below negative Y), no walls.
    fn flat(start: Vec3, vel: Vec3) -> Sweep {
        let end = start + vel;
        if end.y >= 0.0 && vel.y >= 0.0 { Sweep { end: Vec3::new(end.x, 0.0, end.z), on_floor: true } } else { Sweep { end, on_floor: false } }
    }

    #[test]
    fn a_launch_is_base_plus_value_times_span() {
        let c = consts();
        let p = launch(&c, 5, [0, u32::MAX, 0x8000_0000], Vec3::new(1.0, -50.0, 2.0));
        assert_eq!(p.vel.x, -20.0);
        assert!((p.vel.y - -20.0).abs() < 1e-3, "{}", p.vel.y);
        assert!((p.vel.z - 0.0).abs() < 1e-3);
        assert_eq!((p.kind, p.life, p.size, p.frame, p.phase), (5, 900.0, 0xff, 0xff, Phase::Fall));
    }

    /// FUN_001c5060 over many frames: rises, falls, bounces with -0.8 while faster than 5 (a sound for
    /// the first two), then rests on the floor; it becomes pickable once it falls.
    #[test]
    fn a_prize_bounces_then_rests() {
        let c = consts();
        let mut p = launch(&c, 0, [0x8000_0000, 0, 0x8000_0000], Vec3::new(0.0, -100.0, 0.0));
        assert!(p.vel.y < 0.0, "launched upward (Y down)");
        let mut sounds = 0;
        let mut frames = 0;
        fall_physics(&mut p, &c, flat);
        assert!(p.vel.y < 0.0 && !p.pickable, "still rising after one frame: not pickable yet");
        while p.phase == Phase::Fall && frames < 300 {
            if fall_physics(&mut p, &c, flat).is_some() {
                sounds += 1;
            }
            frames += 1;
        }
        assert_eq!(p.phase, Phase::Rest, "rests within 300 frames");
        assert!(p.bounces >= 1);
        assert_eq!(sounds, p.bounces.min(2) as usize);
        assert!(p.pickable);
        // Resting: position radius above the floor point (Y down: pos.y = -radius), velocity gravity only.
        assert!((p.pos.y - -10.0).abs() < 1e-3, "{}", p.pos.y);
        assert_eq!(p.vel, Vec3::new(0.0, c.gravity, 0.0));
    }

    #[test]
    fn a_wall_reverses_the_cut_axis() {
        let c = consts();
        let mut p = launch(&c, 0, [u32::MAX, 0x8000_0000, 0x8000_0000], Vec3::ZERO);
        let vx = p.vel.x;
        // A wall at x = 0: the sweep gives no sideways movement.
        fall_physics(&mut p, &c, |s, v| Sweep { end: Vec3::new(s.x, s.y + v.y, s.z + v.z), on_floor: false });
        assert!((p.vel.x - -vx * c.friction).abs() < 1e-4, "{} vs {}", p.vel.x, vx);
    }

    /// FUN_001c52a8 + FUN_001c5020: the size shrinks over the last 12 frames, no pickup then; the
    /// prize is gone when its life runs out.
    #[test]
    fn the_last_frames_shrink_and_end_the_prize() {
        let c = consts();
        let mut p = launch(&c, 0, [0x8000_0000; 3], Vec3::ZERO);
        p.phase = Phase::Rest;
        p.pickable = true;
        p.life = 12.0;
        let mut asked = 0;
        let r = tick(&mut p, &c, flat, Some(Vec3::ZERO), |_| {
            asked += 1;
            false
        });
        assert!(r.alive && asked == 1, "12 frames left: the check still runs");
        let mut last = p.size;
        let mut n = 0;
        loop {
            let r = tick(&mut p, &c, flat, Some(Vec3::ZERO), |_| panic!("no pickup while shrinking"));
            n += 1;
            if !r.alive {
                break;
            }
            assert!(p.size <= last);
            last = p.size;
        }
        assert_eq!(n, 11, "lives 11..1 shrink, the 11th frame ends it");
        assert_eq!(p.size, (1.0f32 * 255.0 / 12.0) as u8);
    }

    #[test]
    fn rest_rechecks_the_floor_every_30_frames() {
        let c = consts();
        let mut p = launch(&c, 0, [0x8000_0000; 3], Vec3::ZERO);
        p.phase = Phase::Rest;
        p.vel = Vec3::new(0.0, c.gravity, 0.0);
        let air = |s: Vec3, v: Vec3| Sweep { end: s + v, on_floor: false };
        for _ in 0..29 {
            shrink_or_check(&mut p, &c, air);
            assert_eq!(p.phase, Phase::Rest);
        }
        shrink_or_check(&mut p, &c, air);
        assert_eq!(p.phase, Phase::Fall);
        assert_eq!(p.recheck, RECHECK);
    }

    #[test]
    fn reach_is_horizontal_plus_draw_and_vertical_30() {
        let c = consts();
        let mut p = launch(&c, 0, [0x8000_0000; 3], Vec3::ZERO);
        p.pos = Vec3::new(119.0, -10.0, 0.0);
        assert!(in_reach(&c, &p, Vec3::ZERO, 0.0));
        p.pos.x = 121.0;
        assert!(!in_reach(&c, &p, Vec3::ZERO, 0.0));
        assert!(in_reach(&c, &p, Vec3::ZERO, 5.0), "Draw widens the reach");
        p.pos = Vec3::new(0.0, -41.0, 0.0);
        assert!(!in_reach(&c, &p, Vec3::ZERO, 0.0), "low point 31 above the feet");
        assert!(!player_takes(0, true) && !player_takes(1, true) && player_takes(2, true) && player_takes(0, false));
    }

    /// FUN_001c5808 + FUN_001c53a8: the prize spirals into the centre over 40 frames.
    #[test]
    fn the_draw_in_shrinks_the_distance_linearly() {
        let c = consts();
        let mut p = launch(&c, 0, [0x8000_0000; 3], Vec3::new(100.0, 0.0, 0.0));
        start_absorb(&mut p, &c, Vec3::ZERO);
        assert_eq!((p.dist, p.life), (100.0, 40.0));
        let mut frames = 0;
        while absorb_step(&mut p, &c, Vec3::ZERO) {
            frames += 1;
            let want = 100.0 * p.life / 40.0;
            assert!((p.pos.length() - want).abs() < 1e-3);
            assert_eq!(p.size, (p.life / 40.0 * 255.0) as u8);
        }
        assert_eq!(frames, 39);
    }

    #[test]
    fn global_flags_hand_the_prize_to_the_player() {
        let mut p = launch(&consts(), 0, [0; 3], Vec3::ZERO);
        let never = || false;
        assert!(!pickup_check(0, true, &p, never), "clear: out of reach stays unpicked");
        assert!(pickup_check(0, true, &p, || true), "clear: the reach test decides");
        assert!(pickup_check(crate::magic_rules::GFLAG_COLLECT_AT_ONCE, true, &p, never), "bit 3: at once");
        assert!(!pickup_check(crate::magic_rules::GFLAG_COLLECT_AT_ONCE, false, &p, never), "bit 3 needs the player object");
        assert!(!pickup_check(crate::magic_rules::GFLAG_TARGET_PLAYER, true, &p, never), "bit 0x800: only from +0x44 = 3");
        p.bounces = 3;
        assert!(pickup_check(crate::magic_rules::GFLAG_TARGET_PLAYER, true, &p, never));
        p.flag_8 = true;
        assert!(pickup_check(0, true, &p, never), "flag 8 stays after the bit is gone");
    }

    #[test]
    fn drive_gain_scales_only_while_charging() {
        assert_eq!(drive_gain(3.0, false, 2.0, 1.5, 1.0), 3);
        assert_eq!(drive_gain(3.0, true, 2.0, 1.5, 1.0), 9);
        assert_eq!(drive_gain(1.0, false, 2.0, 1.5, 1.0), 1);
    }

    #[test]
    fn wallet_caps_below_a_million() {
        let ix = Arc::new(crate::progress::ItemIndex::new(&[], &[], &[]));
        let mut p = crate::progress::Progress::from_block(&[], ix);
        assert_eq!(p.add_to_wallet(10), 10);
        assert_eq!(p.add_to_wallet(999_989), 999_999);
        assert_eq!(p.add_to_wallet(1), 999_999, "a sum of 1,000,000 becomes 999,999 (FUN_0023e5e0)");
        assert_eq!(p.experience(), 0, "the experience word is not touched");
    }

    fn exe() -> Option<Vec<u8>> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists())?;
        Disc::open(&iso).read_root("SLUS_210.05")
    }

    /// The tables of the user's executable: nine kinds numbered 0..8, the pickup rows give HP to
    /// kinds 0-1, munny to 2-4, MP to 5-6, drive to 7-8 (skips without an ISO).
    #[test]
    fn real_prize_tables() {
        let Some(elf) = exe() else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let c = Consts::from_exe(&elf).expect("constants");
        assert!(c.kinds.iter().enumerate().all(|(k, e)| e.id as usize == k && e.radius > 0.0));
        let has = |k: usize| {
            let p = c.pickups[k];
            [p.hp > 0, p.mp > 0, p.drive > 0, p.munny > 0]
        };
        for k in 0..2 {
            assert_eq!(has(k), [true, false, false, false], "kind {k}");
        }
        for k in 2..5 {
            assert_eq!(has(k), [false, false, false, true], "kind {k}");
        }
        for k in 5..7 {
            assert_eq!(has(k), [false, true, false, false], "kind {k}");
        }
        for k in 7..9 {
            assert_eq!(has(k), [false, false, true, false], "kind {k}");
        }
        assert_eq!(c.unit, 2f32.powi(-32));
        assert!(c.y_base < 0.0 && c.y_base + c.y_span <= 0.0, "launched upward (Y down)");
        assert!(c.bounce < 0.0 && c.gravity > 0.0 && c.life > c.fade);
    }

    /// The oracle: the game's own FUN_001a3b28 on the user's executable (kh2-ee) gives the same launch
    /// velocity and generator state as `launch` + `compute_drop`'s three steps. FUN_001c4d78 (the pool's
    /// allocator) is replaced by a stub that hands out a fixed record: the routine only writes the
    /// record's velocity and the generator.
    #[test]
    fn launch_matches_fun_001a3b28() {
        let Some(elf) = exe() else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let c = Consts::from_exe(&elf).expect("constants");
        const STACK: u32 = 0x01ff_0000;
        const REC: u32 = 0x0180_0000;
        const SEED: u32 = 0x0037_16a0;
        for (i, start) in [1u32, 0x1234_5678, 0x8000_0001, 0xdead_beef, 7].into_iter().enumerate() {
            let mut ee = kh2_ee::Ee::new();
            ee.load_elf(&elf).expect("elf");
            // FUN_001c4d78 -> return REC: lui v0, hi; jr ra; ori v0, v0, lo.
            ee.w32(0x001c_4d78, 0x3c02_0000 | (REC >> 16));
            ee.w32(0x001c_4d7c, 0x03e0_0008);
            ee.w32(0x001c_4d80, 0x3442_0000 | (REC & 0xffff));
            ee.w32(SEED, start);
            let kind = (i % KINDS) as u32;
            ee.call(0x001a_3b28, &[0x0190_0000, (0x37b4b0 + 16 * kind) as u64, 0x0191_0000], &[], STACK).expect("FUN_001a3b28");
            let game = Vec3::new(ee.f32_at(REC + 0x30), ee.f32_at(REC + 0x34), ee.f32_at(REC + 0x38));
            let mut seed = start;
            let mut v = [0u32; 3];
            for x in &mut v {
                seed = seed.wrapping_mul(0x10dcd);
                *x = seed;
            }
            let port = launch(&c, kind as u8, v, Vec3::ZERO).vel;
            assert_eq!(ee.r32(SEED), seed, "generator after three steps");
            assert!((game - port).length() < 1e-4, "start {start:#x}: game {game:?} port {port:?}");
            assert_eq!(ee.f32_at(REC + 0x3c), 0.0);
        }
    }

    /// The oracle for the draw-in turn: the game's FUN_0011b460 + FUN_0011b4d8 build the matrix whose
    /// rows rotate a row vector the way `rot_y` does (kh2-ee runs the VU0 macro code).
    #[test]
    fn absorb_turn_matches_the_game_rotation() {
        let Some(elf) = exe() else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        const STACK: u32 = 0x01ff_0000;
        const M: u32 = 0x0180_0000;
        for a in [0.3f32, -0.5, 0.59341] {
            let mut ee = kh2_ee::Ee::new();
            ee.load_elf(&elf).expect("elf");
            // FUN_0011b460 copies the identity from 0x354a60, which the executable fills at run time
            // (zero in the file): the identity is written here instead.
            for i in 0..4 {
                for j in 0..4 {
                    ee.wf32(M + 16 * i + 4 * j, if i == j { 1.0 } else { 0.0 });
                }
            }
            ee.call(0x0011_b4d8, &[M as u64], &[a], STACK).expect("FUN_0011b4d8");
            let mut row = |r: u32| Vec3::new(ee.f32_at(M + 16 * r), ee.f32_at(M + 16 * r + 4), ee.f32_at(M + 16 * r + 8));
            let (r0, r1, r2) = (row(0), row(1), row(2));
            // A row vector v times the matrix: v.x * row0 + v.y * row1 + v.z * row2.
            for v in [Vec3::X, Vec3::Z, Vec3::new(0.6, 0.0, -0.8)] {
                let game = r0 * v.x + r1 * v.y + r2 * v.z;
                let port = rot_y(a, v);
                assert!((game - port).length() < 1e-4, "angle {a} v {v:?}: game {game:?} port {port:?}");
            }
        }
    }

    /// The effect file of PRIZE: one `pppRyjMegaBirthPrize` program whose kind parameters name shapes
    /// of its DPD (skips without an ISO).
    #[test]
    fn prize_looks_name_real_shapes() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let disc = Disc::open(&iso);
        let pax = crate::fx::load(&disc, "obj/PRIZE.mdlx", 0, None).expect("PRIZE pax");
        let program = kh2_fx::ppp::NAMES.iter().position(|n| *n == PROGRAM).expect("program") as u32;
        let d = &pax.dpds[0];
        let params = d.particles.iter().flat_map(|p| p.groups.iter()).flat_map(|g| g.programs.iter()).find(|p| p.kind == program).expect("program in PRIZE").params.clone();
        let looks = kind_looks(&params);
        eprintln!("looks {looks:?}; {} shapes", d.shapes.len());
        // Every sprite's texture rectangle (fractions of the texture, raw / 4096) lies inside the texture.
        for s in &d.shapes {
            for f in &s.frames {
                for sp in &f.sprites {
                    let [u, v, w, h] = sp.st;
                    assert!(u >= 0.0 && v >= 0.0 && u + w <= 1.0 && v + h <= 1.0, "{:?}", sp.st);
                }
            }
        }
        assert!(looks.len() >= KINDS);
        for (k, l) in looks.iter().take(KINDS).enumerate() {
            assert!((l.shape as usize) < d.shapes.len(), "kind {k}: shape {} of {}", l.shape, d.shapes.len());
            assert!(l.scale > 0.0, "kind {k}");
        }
    }

    /// Shapes of the item object's real numbers (the disc test reads the real ones).
    fn item_consts() -> ItemConsts {
        ItemConsts {
            boxes: vec![(0x3b9, 1.6); BOX_ROWS],
            yaw_unit: 2f32.powi(-32),
            yaw_span: std::f32::consts::TAU,
            yaw_base: -std::f32::consts::PI,
            unit: 2f32.powi(-32),
            xz_span: 10.0,
            xz_base: -5.0,
            y_span: 5.0,
            y_base: -15.0,
            fall_max: 25.0,
            gravity: 1.5,
            friction: 0.97,
            bounce_min: 2.5,
            bounce: -0.8,
            time_scale: 0.05,
            time_min: 30.0,
            time_max: 60.0,
            one: 1.0,
            pi: std::f32::consts::PI,
            half: 0.5,
            lift: 50.0,
            spin: 0.5236,
            shrink_at: 0.1,
            shrink_scale: 10.0,
        }
    }

    /// FUN_001bd0d0 + FUN_001bd580: value 0 is the yaw, values 1, 2, 3 go to X, Z and Y (+0x20, +0x28,
    /// +0x24 in the order the enter writes them); the item starts not pickable.
    #[test]
    fn an_item_launch_takes_yaw_then_x_z_y() {
        let c = item_consts();
        let o = item_launch(&c, 1, 1.6, [0, u32::MAX, 0, 0x8000_0000], Vec3::new(1.0, -50.0, 2.0));
        assert_eq!(o.yaw, -std::f32::consts::PI);
        assert!((o.vel.x - 5.0).abs() < 1e-3, "{:?}", o.vel);
        assert_eq!(o.vel.z, -5.0);
        assert!((o.vel.y - -12.5).abs() < 1e-3, "{:?}", o.vel);
        assert!(!o.pickable && !o.taken && o.absorb.is_none());
        assert_eq!((o.scale, o.base_scale), (1.6, 1.6));
    }

    /// FUN_001bd700 + FUN_001bd7a0: upward first (not pickable), pickable once it falls, bounces while
    /// faster than 2.5 (factor -0.8, the sound 0xe on the first two), then rests with no velocity.
    #[test]
    fn an_item_bounces_then_rests() {
        let c = item_consts();
        let mut o = item_launch(&c, 1, 1.6, [0, 0x8000_0000, 0x8000_0000, 0], Vec3::new(0.0, -100.0, 0.0));
        item_fall(&mut o, &c, flat);
        assert!(!o.pickable, "still rising");
        let (mut sounds, mut frames) = (Vec::new(), 0);
        while !o.rested && frames < 1000 {
            if let Some(s) = item_fall(&mut o, &c, flat) {
                sounds.push(s);
            }
            frames += 1;
        }
        assert!(o.rested, "rests within {frames} frames");
        assert!(o.pickable);
        assert_eq!(o.vel, Vec3::ZERO);
        assert!(o.bounces >= 2, "{} bounces", o.bounces);
        assert_eq!(sounds, vec![ITEM_BOUNCE_SE; 2]);
        assert_eq!(o.pos.y, 0.0);
        // At rest it stays on the floor.
        item_fall(&mut o, &c, flat);
        assert_eq!((o.pos.y, o.vel), (0.0, Vec3::ZERO));
    }

    /// FUN_001bd1d8 + FUN_001bd878: frames = distance * 0.05 held in 30..60; the item arcs up
    /// (sin(f * pi) * 50) on its way in, spins faster as it nears, shrinks below f 0.1 and ends at the
    /// centre when the frames run out.
    #[test]
    fn a_taken_item_arcs_into_the_collector() {
        let c = item_consts();
        let centre = Vec3::new(0.0, -100.0, 0.0);
        let mut o = item_launch(&c, 1, 1.6, [0; 4], Vec3::new(200.0, -100.0, 0.0));
        item_take(&mut o, &c, centre);
        assert!(o.taken && !o.pickable);
        let a = o.absorb.unwrap();
        assert_eq!((a.left, a.total), (30.0, 30.0), "|offset| 200 * 0.05 = 10 is raised to 30");
        let mut far = item_launch(&c, 1, 1.6, [0; 4], Vec3::new(2000.0, -100.0, 0.0));
        item_take(&mut far, &c, centre);
        assert_eq!(far.absorb.unwrap().total, 60.0, "2000 * 0.05 = 100 is held at 60");
        let mut frames = 0;
        let mut highest = 0.0f32;
        let yaw0 = o.yaw;
        while item_absorb_step(&mut o, &c, centre) {
            frames += 1;
            highest = highest.max(-(o.pos.y - centre.y));
        }
        assert_eq!(frames, 29, "the 30th frame ends it");
        assert!(highest > 30.0, "the arc rises above the line: {highest}");
        assert!(o.yaw > yaw0);
        assert!(o.scale < 1.6, "shrunk at the end: {}", o.scale);
        assert!((o.pos - centre).length() < 15.0, "{:?}", o.pos);
    }

    /// FUN_00175c28 / FUN_00175cb0: one start; a second pickup only raises the frames left.
    #[test]
    fn the_pickup_glow_starts_once_and_extends() {
        let mut g = Glow::default();
        assert!(g.start(20.0, false));
        assert!(!g.start(10.0, false));
        assert_eq!(g.left, 20.0);
        assert!(!g.tick(false));
        assert!(!g.start(30.0, false));
        assert_eq!(g.left, 30.0);
        for _ in 0..29 {
            assert!(!g.tick(false));
        }
        assert!(g.tick(false), "ends at 0");
        assert!(!g.running);
        let mut b = Glow::default();
        assert!(!b.start(20.0, true), "+0x160 bit 0x4000 blocks the start");
    }

    /// FUN_001d3f88: the experience carries over each level's amount, stops below the next one, and
    /// at the cap is set to 0.
    #[test]
    fn master_form_experience_levels_up_to_the_cap() {
        use kh2_tables::player::{FormLevel, FormLevels};
        let row = |level: u8, exp: i32, ability: u16| FormLevel { form: 3, level, display_level: level, ability, exp };
        let levels = FormLevels(vec![row(1, 10, 0), row(2, 20, 0x100), row(3, 30, 0x101), row(4, 40, 0), row(5, 50, 0), row(6, 60, 0), row(7, 0, 0x102)]);
        assert_eq!(add_form_exp(&levels, 3, 1, 0, 9, 7), (1, 9, vec![]));
        assert_eq!(add_form_exp(&levels, 3, 1, 9, 1, 7), (2, 0, vec![2]));
        assert_eq!(add_form_exp(&levels, 3, 1, 5, 40, 7), (3, 15, vec![2, 3]));
        // Cap 3 (one form owned): level 3 is the top; the experience is cleared there.
        assert_eq!(add_form_exp(&levels, 3, 1, 0, 100, 3), (3, 0, vec![2, 3]));
        assert_eq!(add_form_exp(&levels, 3, 3, 0, 5, 3), (3, 0, vec![]));
    }

    /// `FUN_001d3f18` on the executable's chains (0x37c556): rows 1..4 are four-level chains; an empty slot
    /// takes level 1, the level-1 id becomes level 2 with bit 15 kept, nothing else changes; the id at the
    /// level comes back either way. `form_level_up` on the disc's `fmlv` rows of Master Form: every
    /// level-up whose display level rises names the chain's ids (form at the display level, Sora one below).
    /// Skips without an ISO.
    #[test]
    fn form_level_ups_upgrade_the_growth_chain() {
        let Some(elf) = exe() else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let chains = GrowthChains::from_exe(&elf).expect("chains");
        eprintln!("growth chains {:x?}", chains.0);
        for f in 1..=4u8 {
            assert_eq!(chains.at(f, 0), 0, "form {f}: level 0 is none");
            assert!((1..=4).all(|l| chains.at(f, l) != 0), "form {f}: four levels");
        }
        let (l1, l2) = (chains.at(3, 1), chains.at(3, 2));
        let mut list = [0x8100u16, 0, 0, 0];
        assert_eq!(growth_upgrade(&mut list, &chains, 3, 1), l1);
        assert_eq!(list, [0x8100, l1, 0, 0], "the first empty slot takes level 1");
        list[1] |= 0x8000;
        assert_eq!(growth_upgrade(&mut list, &chains, 3, 2), l2);
        assert_eq!(list, [0x8100, 0x8000 | l2, 0, 0], "level 1 becomes level 2, still equipped");
        let mut none = [0x8100u16, 0x100];
        assert_eq!(growth_upgrade(&mut none, &chains, 3, 3), chains.at(3, 3));
        assert_eq!(none, [0x8100, 0x100], "no level-2 slot: nothing changes");
        let iso = crate::assets::iso_path().expect("iso");
        let rules = crate::drive::DriveRules::load(&Disc::open(&iso)).expect("drive rules");
        let mut rises = 0;
        for level in 2..=7u8 {
            let up = form_level_up(&rules.levels, &chains, 3, level);
            let (row, prev) = (rules.levels.get(3, level).expect("row"), rules.levels.get(3, level - 1).expect("prev"));
            eprintln!("Master LV{level}: display {} (before {}), {up:x?}", row.display_level, prev.display_level);
            assert_eq!(up.ability, row.ability);
            match up.growth {
                Some((f, s)) => {
                    rises += 1;
                    assert_eq!((f, s), (chains.at(3, row.display_level), chains.at(3, row.display_level - 1)));
                }
                None => assert!(row.display_level <= prev.display_level),
            }
        }
        assert!(rises > 0, "the display level rises somewhere between 2 and 7");
    }

    /// `FUN_001d4078` / `FUN_001d40e0`: Valor gains on hits, Wisdom on ids 1000..=2011, Final on 3000..=4008.
    #[test]
    fn form_experience_from_hits_and_defeats() {
        use kh2_tables::player::form;
        assert_eq!((valor_hit_exp(form::VALOR), valor_hit_exp(form::MASTER), valor_hit_exp(0)), (1, 0, 0));
        assert_eq!((defeat_exp(form::WISDOM, 999), defeat_exp(form::WISDOM, 1000), defeat_exp(form::WISDOM, 2011), defeat_exp(form::WISDOM, 2012)), (0, 1, 1, 0));
        assert_eq!((defeat_exp(form::FINAL, 2999), defeat_exp(form::FINAL, 3000), defeat_exp(form::FINAL, 4008), defeat_exp(form::FINAL, 4009)), (0, 1, 1, 0));
        assert_eq!(defeat_exp(form::VALOR, 1500), 0);
        assert_eq!(defeat_exp(form::FINAL, 1500), 0);
    }

    /// `FUN_001d0760` / `FUN_001d0688`: the ring of 32 overwrites the oldest; the "new" bit is set for a
    /// key item or an item whose "obtained once" bit is clear; the place bytes come from the area name.
    #[test]
    fn the_obtained_log_is_a_ring_of_32() {
        let mut log = ObtainedLog::default();
        for i in 0..33u16 {
            log.push(obtained_record((2, 4), i, false, true, 1, 0));
        }
        assert_eq!(log.next, 1);
        assert_eq!(log.records[0].item, 32, "the 33rd record took the first slot");
        assert_eq!(log.records[1].item, 1);
        assert!(!log.records[1].new);
        assert!(obtained_record((0, 0), 5, true, true, 1, 5).new, "a key item is always new");
        assert!(obtained_record((0, 0), 5, false, false, 1, 5).new, "never obtained before: new");
        assert_eq!(obtained_record((0, 0), 0x8005, false, true, 1, 5).item, 5, "15 bits");
        assert_eq!(place_bytes("tt04"), (2, 4));
        assert_eq!(place_bytes("hb10"), (4, 10));
    }

    /// FUN_001c5808 mode 1 (collect-all) runs 80 frames with its own constants; mode 0 runs 40.
    #[test]
    fn collect_all_draws_in_over_its_own_frames() {
        let mut c = consts();
        c.absorb1_time = 80.0;
        c.absorb1_len = 80.0;
        c.absorb1_turn = 150.0;
        c.absorb1_scale = 255.0;
        c.turn_max1 = 0.5236;
        let centre = Vec3::new(0.0, -100.0, 0.0);
        for (mode, want) in [(0u8, 39), (1, 79)] {
            let mut p = launch(&c, 2, [0; 3], Vec3::new(300.0, 0.0, 0.0));
            start_absorb_mode(&mut p, &c, centre, mode);
            let mut frames = 0;
            while absorb_step(&mut p, &c, centre) {
                frames += 1;
            }
            assert_eq!(frames, want, "mode {mode}");
        }
    }

    /// The executable's item object numbers and the four box objects; the disc's items each name a box
    /// row, and a dropped consumable goes through `Progress::give` up to 99, then is refused (skips
    /// without an ISO).
    #[test]
    fn real_item_objects() {
        let Some(elf) = exe() else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let c = ItemConsts::from_exe(&elf).expect("item constants");
        assert_eq!(c.boxes.len(), BOX_ROWS);
        assert!(c.boxes.iter().all(|&(o, s)| (0x3b8..=0x3bb).contains(&o) && s > 0.0), "{:?}", c.boxes);
        assert!(c.gravity > 0.0 && c.fall_max > c.gravity && c.bounce < 0.0 && c.time_min < c.time_max);
        let o = item_launch(&c, 1, 1.0, [u32::MAX; 4], Vec3::ZERO);
        assert!(o.vel.y < 0.0, "launched upward (Y down): {:?}", o.vel);
        let iso = crate::assets::iso_path().unwrap();
        let disc = Disc::open(&iso);
        let lib = ItemLib::load(&disc, &elf).expect("item lib");
        assert_eq!(lib.models.len(), 4, "PRIZE_BOX_YELLOW, RED, BLUE, GREEN");
        let mut p = crate::progress::Progress::start(&disc).expect("state");
        let potion = p.items().rows.iter().find(|r| r.kind == crate::progress::item_type::CONSUMABLE).map(|r| r.id).expect("a consumable");
        assert!(lib.box_of(potion).is_some(), "item {potion:#x} names a box row");
        let n0 = p.item_count(potion);
        let mut given = 0;
        while give_item(&mut p, potion).is_some() {
            given += 1;
            assert!(given < 200);
        }
        assert_eq!(p.item_count(potion), crate::progress::MAX_COUNT, "from {n0}, {given} given");
        assert_eq!(give_item(&mut p, potion), None, "a full count refuses");
    }

    /// EN2: the jump's unload removes the prizes and item objects once, when the task reaches `Loading`, and empties
    /// the pool count; other entities stay; a task that never loads removes nothing.
    #[test]
    fn a_jump_clears_prizes_and_items() {
        use crate::area_jump::{Dest, JumpTask};
        use bevy::ecs::system::RunSystemOnce;
        let mut w = bevy::ecs::world::World::new();
        w.insert_resource(PrizePool { live: 3 });
        let prizes: Vec<Entity> = (0..3).map(|_| w.spawn(PrizeObj { p: launch(&consts(), 0, [0; 3], Vec3::ZERO), shown: None, kids: Vec::new() }).id()).collect();
        let other = w.spawn_empty().id();
        w.init_resource::<Messages<ItemObtained>>();
        let item = |id: u16, absorb: Option<ItemAbsorb>| {
            ItemObj { o: Item { id, pos: Vec3::ZERO, vel: Vec3::ZERO, yaw: 0.0, scale: 1.0, base_scale: 1.0, pickable: true, rested: true, taken: absorb.is_some(), bounces: 0, absorb } }
        };
        let falling = w.spawn(item(0x11, None)).id();
        let drawn = w.spawn(item(0x22, Some(ItemAbsorb { offset: Vec3::ZERO, left: 5.0, total: 20.0 }))).id();
        let dest = Dest::new(crate::area::TWILIGHT_TOWN, 4, 0);
        w.insert_resource(JumpTask::FadeOut { dest, kind: 0, t: 0.0 });
        w.run_system_once(unload_prizes).unwrap();
        assert!(prizes.iter().all(|&e| w.get_entity(e).is_ok()), "the fade-out keeps them");
        assert_eq!(w.resource::<PrizePool>().live, 3);
        w.insert_resource(JumpTask::Loading { dest, kind: 0 });
        w.run_system_once(unload_prizes).unwrap();
        assert!(prizes.iter().all(|&e| w.get_entity(e).is_err()), "the unload removes the prizes");
        assert!(w.get_entity(falling).is_err() && w.get_entity(drawn).is_err(), "and the items");
        // Only the item that was being drawn in (already given) owes its message; the one still on the floor was never taken.
        let sent: Vec<ItemObtained> = w.resource_mut::<Messages<ItemObtained>>().drain().collect();
        assert_eq!(sent, [ItemObtained { item: 0x22, icon: 0, name: 0 }]);
        assert!(w.get_entity(other).is_ok(), "other entities stay");
        assert_eq!(w.resource::<PrizePool>().live, 0);
    }
}
