//! Prize objects: the HP, MP and drive orbs and the munny that an enemy's drop (host 1.92,
//! `FUN_001a3cb0`) throws out, their flight, rest, fade and pickup. `docs/prizes.md` has the
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
/// The parsed shape UV that stands for a whole texture side (see `build_draws`).
const UV_UNIT: f32 = 512.0;

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

/// `FUN_001c5020`: a frame off the life; false when it is over.
pub fn age(p: &mut Prize) -> bool {
    p.life -= 1.0;
    p.life > 0.0
}

/// `FUN_001a3700` with the default reach: horizontal distance at most `reach + extra` and vertical
/// distance at most `height` between the prize's low point and the player's position (`FUN_0016b960`).
pub fn in_reach(c: &Consts, p: &Prize, player: Vec3, extra: f32) -> bool {
    let d = p.low_point(c) - player;
    d.y.abs() <= c.height && Vec2::new(d.x, d.z).length() <= c.reach + extra
}

/// `FUN_001a3848` for the player (+0x588 bit 0x80): in Anti form (+0xb50 == 5) the HP kinds (0, 1)
/// are not taken.
pub fn player_takes(kind: u8, anti: bool) -> bool {
    !anti || kind > 1
}

/// `FUN_001c5808` mode 0: the prize turns toward the player's centre.
pub fn start_absorb(p: &mut Prize, c: &Consts, centre: Vec3) {
    let off = p.pos - centre;
    p.dist = off.length();
    p.vel = off.normalize_or_zero();
    p.life = c.absorb_time;
    p.phase = Phase::Absorb;
}

/// `FUN_001c53a8`: one frame of the draw-in. False when it is over (life out).
pub fn absorb_step(p: &mut Prize, c: &Consts, centre: Vec3) -> bool {
    p.life -= 1.0;
    if p.life <= 0.0 {
        return false;
    }
    let frac = p.life / c.absorb_len;
    let turn = (c.absorb_turn / (p.dist * frac)).min(c.turn_max);
    p.vel = rot_y(turn, p.vel);
    p.pos = centre + p.vel * (p.dist * frac);
    p.size = (frac * c.absorb_scale) as i32 as u8;
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

/// A sprite frame ready to draw: mesh (local units, Y up) and material.
struct FrameDraw {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
}

#[derive(Default)]
struct KindDraw {
    scale: f32,
    /// Frames of the kind's shape; `loops` when the last frame's flag has 0x80.
    frames: Vec<FrameDraw>,
    loops: bool,
}

#[derive(Resource)]
pub struct PrizeLib {
    pub consts: Consts,
    /// `sstm` +0x70 (form timer per unit), +0x78 (drive gain while charging), +0x7c (charge per MP).
    sstm: [f32; 3],
    draws: Vec<KindDraw>,
}

#[derive(Component)]
pub struct PrizeObj {
    pub p: Prize,
    shown: Option<usize>,
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
        app.add_message::<PrizeDrop>().add_message::<MunnyGained>().init_resource::<PrizePool>();
        let Some(disc) = app.world().get_resource::<Disc>() else { return };
        let Some(consts) = disc.read_root("SLUS_210.05").and_then(|e| Consts::from_exe(&e)) else {
            warn!("prize: the prize constants are not readable from SLUS_210.05; drops make no prize objects");
            return;
        };
        let sstm = {
            let mut a = disc.0.lock().unwrap();
            kh2_tables::player::load_sstm(&mut a).ok().map_or([0.0; 3], |s| [s.f32(0x70), s.f32(0x78), s.f32(0x7c)])
        };
        let pax = crate::fx::load(disc, "obj/PRIZE.mdlx", usize::MAX - 7, None);
        if pax.is_none() {
            warn!("prize: obj/PRIZE.mdlx has no readable effect file; prizes run but are not drawn");
        }
        app.insert_resource(PrizeLib { consts, sstm, draws: Vec::new() })
            .insert_resource(PrizePax(pax.map(Arc::new)))
            .add_systems(Startup, build_draws)
            .add_systems(FixedUpdate, (prize_test, spawn_drops, run_prizes).chain().in_set(Sim::EffectTick))
            .add_systems(Update, show_prizes);
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

/// The sprite meshes of each kind's shape: one mesh per frame (its sprites as quads in local units,
/// X right and Y up, the vertex colour the sprite's colour times the kind's, 0x80 = 1.0).
fn build_draws(
    mut lib: ResMut<PrizeLib>,
    pax: Res<PrizePax>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(pax) = &pax.0 else { return };
    let program = kh2_fx::ppp::NAMES.iter().position(|n| *n == PROGRAM).unwrap_or(usize::MAX) as u32;
    let Some((dpd, params)) = pax.dpds.iter().enumerate().find_map(|(i, d)| {
        d.particles.iter().flat_map(|p| p.groups.iter()).flat_map(|g| g.programs.iter()).find(|p| p.kind == program).map(|p| (i, p.params.clone()))
    }) else {
        warn!("prize: obj/PRIZE.mdlx has no {PROGRAM} program; prizes are not drawn");
        return;
    };
    let looks = kind_looks(&params);
    let d = &pax.dpds[dpd];
    let mut textures: HashMap<u64, (Handle<Image>, [f32; 2])> = HashMap::new();
    let mut draws = Vec::new();
    for k in 0..KINDS {
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
            let (tex, _texels) = textures
                .entry(f.tex0)
                .or_insert_with(|| {
                    let (w, h, rgba) = pax.vram[dpd].texture8(f.tex0);
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
                    (images.add(img), [w as f32, h as f32])
                })
                .clone();
            // The sprite UVs of this file, as parsed, fill their texture only when read as fractions
            // of 512 on both axes: then every sprite covers one square cell of its 128x64 page,
            // matching its square quad [inferred from the data; the VU1 code was not read].
            let size = [UV_UNIT, UV_UNIT];
            let (mut pos, mut uv, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for s in &f.sprites {
                let base = pos.len() as u32;
                let [u, v, w, h] = s.uv;
                let uvs = [[u, v], [u + w, v], [u, v + h], [u + w, v + h]];
                for (c, t) in s.corners.iter().zip(uvs) {
                    pos.push([c[0], -c[1], 0.0]);
                    uv.push([t[0] / size[0], t[1] / size[1]]);
                    let ch = |i: usize| s.rgba[i] as f32 / 128.0 * look.rgb.get(i).map_or(1.0, |x| *x as f32 / 128.0);
                    col.push([ch(0), ch(1), ch(2), s.rgba[3] as f32 / 128.0]);
                }
                idx.extend([base, base + 1, base + 2, base + 1, base + 3, base + 2]);
            }
            let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(col))
                .with_inserted_indices(Indices::U32(idx));
            let material = materials.add(StandardMaterial {
                base_color_texture: Some(tex),
                unlit: true,
                cull_mode: None,
                double_sided: true,
                alpha_mode: alpha_mode(f.sprites.first().map_or(0x44, |s| s.alpha)),
                ..default()
            });
            frames.push(FrameDraw { mesh: meshes.add(mesh), material });
        }
        let loops = shape.frames.last().is_some_and(|f| f.flag & 0x80 != 0);
        draws.push(KindDraw { scale: look.scale, frames, loops });
    }
    info!(
        "prize: {} drawn from obj/PRIZE.mdlx ({} shapes): kinds {:?}",
        PROGRAM,
        d.shapes.len(),
        looks.iter().take(KINDS).map(|l| (l.shape, l.rgb, l.scale)).collect::<Vec<_>>()
    );
    lib.draws = draws;
}

/// App (cm, Y up) and game space (Y and Z negated) are each other's mirror.
fn flip(v: Vec3) -> Vec3 {
    Vec3::new(v.x, -v.y, -v.z)
}

/// The floor sweep of a point in game space over the app's collision: sideways the point slides as
/// a ball of the kind's radius against walls, downward it stops on the highest floor it crosses
/// [inferred: the game uses its character mover `FUN_00143840`, not ported; this keeps its results:
/// the end point and whether it stands on a floor].
fn sweep(world: Option<&World>, radius: f32, start: Vec3, vel: Vec3) -> Sweep {
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
fn shape_point(anim: &Animator, body: &GlobalTransform, kind: u8) -> Option<Vec3> {
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
            commands.spawn((PrizeObj { p, shown: None }, Transform::from_translation(at * SCALE), Visibility::Hidden));
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
        commands.spawn((PrizeObj { p, shown: None }, Transform::from_translation(at * SCALE), Visibility::Hidden));
        info!("prize: test launch of kind {kind} at {at:.0?}");
    }
}

/// The prizes' frame: flight, rest, pickup and draw-in; the pickup applies `FUN_001a39b0`.
#[allow(clippy::too_many_arguments)]
fn run_prizes(
    mut commands: Commands,
    lib: Res<PrizeLib>,
    mut pool: ResMut<PrizePool>,
    world: Option<Res<World>>,
    data: Res<crate::data::GameData>,
    mut prizes: Query<(Entity, &mut PrizeObj, &mut Transform), Without<Sora>>,
    mut sora: Query<(&Transform, &mut Sora)>,
    bodies: Query<(&Animator, &GlobalTransform)>,
    mut magic: Option<ResMut<crate::magic::MagicState>>,
    mut drive: Option<ResMut<crate::drive::DriveState>>,
    abilities: Option<Res<crate::abilities::Abilities>>,
    mut progress: Option<ResMut<crate::progress::Progress>>,
    mut sounds: MessageWriter<crate::audio::PlaySe>,
    mut munny: MessageWriter<MunnyGained>,
) {
    let c = &lib.consts;
    let Ok((st, mut s)) = sora.single_mut() else { return };
    let feet = st.translation / SCALE;
    let centre = bodies.get(s.body).ok().and_then(|(a, g)| shape_point(a, g, CENTRE_SHAPE)).unwrap_or(feet);
    let (feet_g, centre_g) = (flip(feet), flip(centre));
    let anti = drive.as_ref().is_some_and(|d| d.object_form() == kh2_tables::player::form::ANTI);
    let draw = abilities.as_ref().map_or(0.0, |a| a.block().draw_radius);
    let alive_player = s.hp > 0;
    for (e, mut obj, mut t) in &mut prizes {
        let radius = c.kinds.get(obj.p.kind as usize).map_or(0.0, |k| k.radius);
        let was = obj.p.phase;
        let w = world.as_deref();
        // +0x588 bit 4 is taken as set for Sora, so Draw's ability-block +0x30 widens the reach
        // [inferred]; the status float +0x1c4 the reach also adds is taken as 0 (not traced).
        let out = tick(&mut obj.p, c, |a, v| sweep(w, radius, a, v), alive_player.then_some(centre_g), |p| {
            player_takes(p.kind, anti) && in_reach(c, p, feet_g, draw)
        });
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
            let k = obj.p.kind as usize;
            let pick = c.pickups.get(k).copied().unwrap_or_default();
            if let Some(se) = c.kinds.get(k).map(|x| x.pickup_se) {
                sounds.write(crate::audio::PlaySe::number(se, None).at(centre * SCALE));
            }
            info!("prize: kind {} drawn toward Sora from {:.0} away ({} frames)", k, obj.p.dist, c.absorb_time);
            // FUN_001a39b0, in its order: glow (not ported), HP, MP, drive, munny.
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
                    None => s.mp = (s.mp + pick.mp as i32).min(data.sora_max_mp),
                }
            }
            let mut drive_note = String::new();
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
            }
            let mut munny_note = String::new();
            if pick.munny > 0 {
                munny.write(MunnyGained { amount: pick.munny as u32 });
                if let Some(pr) = progress.as_deref_mut() {
                    munny_note = format!(" (munny {})", pr.add_to_wallet(pick.munny as u32));
                }
            }
            info!(
                "prize: picked up kind {k}: HP +{} MP +{} drive +{} munny +{} -> HP {hp0}->{} MP {mp0}->{}{mp_note}{drive_note}{munny_note}",
                pick.hp, pick.mp, pick.drive, pick.munny, s.hp, s.mp
            );
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
}

/// The draw (`FUN_00203378` per pool entry): the kind's shape frame as a sprite facing the camera
/// [inferred: the handler multiplies by the matrix at 0x1db1000, taken as the camera's rotation],
/// scaled by the kind's scale times size / 255. The frame steps once per tick; a looping shape
/// starts at a random frame (0xff), the app draws that from the launch.
fn show_prizes(
    mut commands: Commands,
    lib: Res<PrizeLib>,
    cam: Query<&GlobalTransform, With<Camera3d>>,
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
            commands.entity(e).insert((Mesh3d(fr.mesh.clone()), MeshMaterial3d(fr.material.clone()), bevy::light::NotShadowCaster));
            obj.shown = Some(f as usize);
        }
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
        assert_eq!(p.munny(), 0, "the experience word is not touched");
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
        // Every sprite's UV rectangle, read as fractions of UV_UNIT, lies inside its texture page.
        for s in &d.shapes {
            for f in &s.frames {
                for sp in &f.sprites {
                    let [u, v, w, h] = sp.uv;
                    assert!(u >= 0.0 && v >= 0.0 && u + w <= UV_UNIT && v + h <= UV_UNIT, "{:?}", sp.uv);
                }
            }
        }
        assert!(looks.len() >= KINDS);
        for (k, l) in looks.iter().take(KINDS).enumerate() {
            assert!((l.shape as usize) < d.shapes.len(), "kind {k}: shape {} of {}", l.shape, d.shapes.len());
            assert!(l.scale > 0.0, "kind {k}");
        }
    }
}
