//! Save points and world points (WP-E4, docs/systems/events-movies/npc-savepoints.md section 4).
//!
//! What the game does (`SLUS_210.05`):
//! - `FUN_0016e8c0` makes an object of `00objentry` type 7 (`WORLD_POINT` 569, `SAVE_POINT` 570) with
//!   `FUN_00194ab8` (0xa00 bytes, vtable 0x35e300): object flag +0x588 |= 0x1000, the "first frame"
//!   byte +0x9fd = 1, and its own effect element 1 started at once (`FUN_0016a570(obj, 1, 1, 0, 0)`,
//!   handle +0x9f4; `FUN_00194b68` ends it). For the world point (objentry 0x239) the current
//!   (world, area) is looked up in `00worldpoint.bin` (`FUN_00194f68`: 4-byte records world, area,
//!   entrance, -, at most 0x36) and that point's bit is set at 0x33f910 (`FUN_00194fa8`).
//! - The two models carry no mesh and no motion set: the object file holds only the collision
//!   records (one type-5 part, id 55, a cylinder of radius 50 and half height 50 lifted 50) and the
//!   effect file (`obj/<name>.apdx`, elements 0, 1, 2).
//! - The offer: the part offers command 55 (`FUN_0019cb70`, as for every object). Its status
//!   (`FUN_001ba2c0` case 0x37) is 5 while the state word 0x35fec0 has a bit set (`FUN_001c6c08`:
//!   bit 1 by the drive form start `FUN_001b44c8`, bit 2 by a summon `FUN_001b4170`, bit 3 by
//!   `FUN_001c6578`), then the command flags and form mask decide as for every command.
//! - The heal: the player's reaction manager `FUN_0019c0f0`, while slot 1 holds command 55 with a
//!   status below 4, calls `FUN_00194c08(the point)` every frame: for each of the four party slots a
//!   member without an object in the world gets its MP refilled and its maximum HP added
//!   (`FUN_001a0240`, `FUN_001a0100`); a member in the world that is not full (HP below its maximum,
//!   or MP below its maximum, or an MP charge running, `FUN_001a05d0`) gains 1 HP (`FUN_001a0140` ->
//!   method +0x78 = `FUN_001792f0` -> `FUN_00176c98` -> `FUN_00186238`) and 1 MP (`FUN_001a0210` ->
//!   `FUN_00186368` -> `FUN_001a0180`; while a charge runs the charge time is shortened instead,
//!   `FUN_001a0540`), and the point's +0x9fc is set; the first call also sets it (+0x9fd).
//! - The point's update `FUN_00194b98`: with +0x9fc set, effect element 2 is started once (handle
//!   +0x9f8); with it clear, a running element 2 is ended (`FUN_001d9f20`); +0x9fc is cleared.
//! - The use: command 55 (`Execute` 0x37) goes through Sora's class `FUN_001cf030` to
//!   `FUN_00177b18` case 0x37 = `FUN_001784b8(player, point)`: party reset kind 7 (`FUN_001abfe8`),
//!   the members' method +0x88 (`FUN_00176758`), byte 0x33e86e = the point's spawn word +0x20
//!   (`FUN_0016b940(point, 1)`), bit 0 of 0x33febc = the player is of objentry kind 0xe, then
//!   `FUN_0024f0b8(1, 0)` for the world point and `FUN_0024f0b8(1, 2)` for the save point: menu kind 1
//!   (scene 3, `FUN_00255598`, task 0x2556f0, docs/systems/ui/menus-save.md 18.5): the points' own copy of the
//!   file screens (same memory card driver, same `save.2ld` groups), not kind 7. `menu_save.rs`
//!   serves it (`SaveMenu::request_point`): one "Save" row in mode 2, a second row in mode 0.
//!
//! Log lines: `save point: ...` (spawn, offer, use) always; `save point heal: ...` when a heal run
//! starts and ends.

use std::sync::Arc;

use bevy::prelude::*;
use kh2_tables::tables as t;

use crate::area::{app_yaw, game_to_app, AreaRecord};
use crate::assets::{AreaPart, Disc};
use crate::fx::{self, Bind, FxHandle, LoadedPax, Placement, StopEffect};
use crate::npc::{part_offer, Talk};
use crate::reaction_cmd::{Active, CmdRow, Ctx, ReactionOffer, ReactionRules, Status};
use crate::reaction_parts::{self as parts};
use crate::schedule::{sim_running, Sim};
use crate::sora::Sora;
use crate::SCALE;
pub use crate::shared::reaction::EXECUTE_SAVE;

/// `00objentry` ObjectType of save and world points (FUN_0016e8c0 case 7 -> FUN_00194ab8).
pub const OBJECT_POINT: u8 = 7;
/// The world point's objentry id (`FUN_00194ab8` compares `FUN_00169958(obj)` with 0x239).
pub const OBJECT_WORLD_POINT: u32 = 0x239;
/// The command the points' part offers.
pub const COMMAND_SAVE: u16 = 55;
/// The effect element started with the point (`FUN_00194ab8`).
pub const EFFECT_IDLE: u16 = 1;
/// The effect element that plays while the point heals (`FUN_00194b98`).
pub const EFFECT_HEAL: u16 = 2;
/// Records of `00worldpoint.bin` searched by `FUN_00194f68` (indices 0..0x35).
pub const WORLD_POINTS: usize = 0x36;

pub struct SavePointPlugin;

impl Plugin for SavePointPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SavePoints>()
            .add_systems(FixedUpdate, point_spawn.in_set(Sim::Enemy).after(crate::npc::npc_update))
            .add_systems(
                FixedUpdate,
                (point_offer, point_heal, point_use).chain().after(crate::npc::talk_offer).before(crate::npc::talk_step).run_if(sim_running),
            );
    }
}

/// A save point or world point in the world.
#[derive(Component)]
pub struct SavePoint {
    pub object: u32,
    /// Position (app space, cm) and facing.
    pub pos: Vec3,
    pub yaw: f32,
    /// The collision records of the object file (its type-5 part).
    pub parts: Option<Arc<parts::Table>>,
    /// The spawn record (word +0x20 goes to 0x33e86e on use).
    pub record: kh2_ard::Entity,
    pub pax: Option<Arc<LoadedPax>>,
    /// +0x9f8: the running heal effect (element 2).
    heal_fx: Option<FxHandle>,
    /// +0x9fc (healed this frame) and +0x9fd (the first frame).
    healed: bool,
    first: bool,
}

/// The points of the loaded area, the world-point bits and the shared files.
#[derive(Resource, Default)]
pub struct SavePoints {
    area: Option<String>,
    table: Option<Vec<[u8; 4]>>,
    common: Option<Option<kh2_fx::Dpd>>,
    /// The offer this point made last tick (for the log).
    last_offer: Option<(Entity, Status)>,
    /// A heal run is going (for the log).
    healing: bool,
}

/// `FUN_00194f68(world, area)`: the index of the first `00worldpoint.bin` record with that world and
/// area, among the first 0x36.
pub fn world_point_index(table: &[[u8; 4]], world: u8, area: u8) -> Option<usize> {
    table.iter().take(WORLD_POINTS).position(|r| r[0] == world && r[1] == area)
}

/// `FUN_001ba2c0` case 0x37 then the flags and the form mask (`FUN_001b70b8`, `FUN_001b7290`):
/// blocked while a drive form, a summon or the third state of 0x35fec0 runs (`busy`).
pub fn save_status(cmds: &[CmdRow], c: &CmdRow, ctx: &Ctx, busy: bool) -> Status {
    // The generic test without the Execute switch (the row's own masks; it has no distance limit).
    let row = CmdRow { execute: u16::MAX, ..c.clone() };
    match crate::reaction_cmd::status(cmds, &row, ctx) {
        Status::Available if busy => Status::Blocked,
        s => s,
    }
}

/// What `FUN_00194c08` does for the player (a party member in the world): `None` when he is full
/// (HP at its maximum, no MP charge running, MP at its maximum), else HP and MP after one frame (1
/// each, clamped; while a charge runs MP is left alone, the charge being shortened instead).
pub fn heal_step(hp: i32, max_hp: i32, mp: i32, max_mp: i32, charging: bool) -> Option<(i32, i32)> {
    if hp >= max_hp && !charging && mp >= max_mp {
        return None;
    }
    let hp = (hp + 1).min(max_hp);
    let mp = if charging { mp } else { (mp + 1).min(max_mp) };
    Some((hp, mp))
}

/// `FUN_00194ab8` for every type-7 entity of the groups the area scripts loaded.
#[allow(clippy::too_many_arguments)]
fn point_spawn(
    mut commands: Commands,
    disc: Res<Disc>,
    rec: Res<AreaRecord>,
    mut points: ResMut<SavePoints>,
    mut objs: Local<Option<Vec<t::Objentry>>>,
    mut sora: Query<(&mut Transform, &mut Sora)>,
    progress: Option<Res<crate::progress::Progress>>,
) {
    let Some(run) = rec.run.as_ref() else { return };
    if points.area.as_deref() == Some(run.name.as_str()) {
        return;
    }
    points.area = Some(run.name.clone());
    points.last_offer = None;
    points.healing = false;
    let objs = objs.get_or_insert_with(|| disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default());
    let table = points
        .table
        .get_or_insert_with(|| disc.read("00worldpoint.bin").map(|b| b.as_chunks::<4>().0.to_vec()).unwrap_or_default())
        .clone();
    let common = points.common.get_or_insert_with(|| disc.read("effect/texcommon.dpd").and_then(|b| kh2_fx::Dpd::parse(&b).ok())).clone();
    let mut serial = 0;
    for g in run.script.groups.iter() {
        let Some(groups) = run.ard.spawns.get(&g.tag) else { continue };
        for grp in groups {
            for e in &grp.entities {
                if !kh2_ard::run::entity_for_entrance(e, rec.entrance) {
                    continue;
                }
                let Some(row) = objs.iter().find(|r| r.object_id == e.object) else { continue };
                if row.object_type != OBJECT_POINT {
                    continue;
                }
                serial += 1;
                let model = row.model_name.clone();
                let file = disc.read(&format!("obj/{model}.mdlx"));
                let table_parts = file.as_deref().and_then(parts::Table::from_mdlx).map(Arc::new);
                // The effect file: a pax id past the ones `fx` and the area effects hand out.
                let pax = fx::load(&disc, &format!("obj/{model}.apdx"), 0x7000 + serial, common.as_ref()).map(Arc::new);
                let pos = game_to_app(e.position);
                let yaw = app_yaw(e.rotation[1]);
                let at = Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), pos * SCALE);
                let root = commands.spawn((Transform::from_matrix(at), Visibility::default(), AreaPart)).id();
                // FUN_00194ab8: element 1 at once.
                let idle = pax.as_ref().and_then(|p| fx::start_placed(&mut commands, p, EFFECT_IDLE, Bind::Fixed(at), Placement::default()));
                if let Some(fx_e) = idle {
                    commands.entity(fx_e).insert(AreaPart);
                }
                commands.entity(root).insert(SavePoint { object: e.object, pos, yaw, parts: table_parts, record: e.clone(), pax, heal_fx: None, healed: false, first: true });
                info!(
                    "save point: {} group {} spawned {model} (object {:#x}) at ({:.0}, {:.0}, {:.0}), effect element {EFFECT_IDLE} {}",
                    run.name,
                    g.tag,
                    e.object,
                    pos.x,
                    pos.y,
                    pos.z,
                    if idle.is_some() { "started" } else { "not found" }
                );
                // Harness (a test aid, not the game's rule): `KH2_SAVEPOINT_WATCH=<hp>` puts Sora on the
                // first point with that much HP.
                if serial == 1
                    && let Some(hp) = std::env::var("KH2_SAVEPOINT_WATCH").ok().and_then(|v| v.parse::<i32>().ok())
                    && let Ok((mut tf, mut s)) = sora.single_mut()
                {
                    crate::sora::respawn(&mut s, pos);
                    s.hp = hp;
                    tf.translation = pos * SCALE;
                    info!("save point: KH2_SAVEPOINT_WATCH: Sora put on {model} with HP {hp}");
                }
                if e.object == OBJECT_WORLD_POINT {
                    match world_point_index(&table, rec.world, rec.area) {
                        Some(i) => {
                            // FUN_00194fa8: the bit lives in the save block (`progress.rs`, 0x33f910).
                            let set = progress.as_deref().map(|p| {
                                p.story().set_world_point(i);
                                p.story().world_point(i)
                            });
                            info!("save point: world point {i} of 00worldpoint.bin (world {}, area {}) seen: bit set (FUN_00194fa8, 0x33f910), read back {set:?}", rec.world, rec.area);
                        }
                        None => info!("save point: no 00worldpoint.bin record for world {} area {}", rec.world, rec.area),
                    }
                }
            }
        }
    }
}

/// `FUN_0019cb70` for the points: their part offers command 55; it takes the player's slot 1 from a
/// townsperson or an enemy offer of a lower priority (`FUN_0019c520`, the nearer one on a tie).
#[allow(clippy::too_many_arguments)]
fn point_offer(
    mut talk: ResMut<Talk>,
    mut state: ResMut<SavePoints>,
    rules: Option<Res<ReactionRules>>,
    slot: Option<Res<ReactionOffer>>,
    points: Query<(Entity, &SavePoint)>,
    sora: Query<&Sora>,
    disc: Res<Disc>,
    mut body: Local<Option<Option<parts::Rec>>>,
    drive: Option<Res<crate::drive::DriveState>>,
    battle: Res<crate::sora::Battle>,
) {
    let (Some(rules), Ok(s)) = (rules.as_deref(), sora.single()) else { return };
    let body = *body.get_or_insert_with(|| disc.read("obj/P_EX100.mdlx").and_then(|f| parts::Table::from_mdlx(&f)).and_then(|t| t.first(parts::TYPE_BODY).copied()));
    let mut best: Option<(crate::npc::PartOffer, Entity)> = None;
    if let Some(player) = body.as_ref().and_then(|b| parts::shape_of(b, s.motor.pos, s.motor.yaw))
        && !talk.running()
        && s.hp > 0
    {
        let form = drive.as_deref().map_or(0, |d| d.object_form());
        let ctx = Ctx { form, battle: battle.active() || drive.as_deref().is_some_and(|d| d.force_battle), mp: s.mp, ..Default::default() };
        for (e, p) in &points {
            let Some(table) = p.parts.as_deref() else { continue };
            let Some(mut o) = part_offer(rules, table, p.record.part_command(), (&[], &[]), (p.pos, p.yaw), &player, (s.motor.pos, s.motor.yaw), (&ctx, None)) else { continue };
            if let Some(c) = rules.cmd(o.cmd).filter(|c| c.execute == EXECUTE_SAVE) {
                // The state word 0x35fec0: bit 1 is the drive form [bits 2 and 3, summon and the
                // third state, are not tracked by the app].
                o.status = save_status(&rules.cmds, c, &ctx, form != 0);
            }
            if best.is_some_and(|(b, _)| o.priority < b.priority || (o.priority == b.priority && b.dist <= o.dist)) {
                continue;
            }
            best = Some((o, e));
        }
    }
    let enemy = slot.as_deref().and_then(|o| o.0).and_then(|a| rules.cmd(a.cmd)).map(|c| c.priority);
    let other = talk.offer.and_then(|a| rules.cmd(a.cmd)).map(|c| c.priority);
    let won = best.filter(|(o, _)| enemy.is_none_or(|p| p < o.priority) && other.is_none_or(|p| p <= o.priority));
    if let Some((o, e)) = won {
        let message = rules.cmd(o.cmd).map_or(0, |c| c.message_id as u32);
        talk.offer = Some(Active { cmd: o.cmd, pos: o.part as i16, from: e, status: o.status, message });
    }
    let now = won.map(|(o, e)| (e, o.status));
    if now != state.last_offer {
        match won {
            Some((o, e)) => info!("save point: offered command {} by {e:?} (part {}, FUN_0019cb70), status {:?}", o.cmd, o.part, o.status),
            None => info!("save point: offer withdrawn"),
        }
        state.last_offer = now;
    }
}

/// `FUN_0019c0f0` -> `FUN_00194c08` while slot 1 holds command 55 with a status below 4, then the
/// point's update `FUN_00194b98` (effect element 2).
#[allow(clippy::too_many_arguments)]
fn point_heal(
    mut commands: Commands,
    talk: Res<Talk>,
    mut state: ResMut<SavePoints>,
    mut points: Query<(Entity, &mut SavePoint)>,
    mut sora: Query<&mut Sora>,
    magic: Option<Res<crate::magic::MagicState>>,
    mut stop: MessageWriter<StopEffect>,
) {
    let offer = talk.offer.filter(|a| a.cmd == COMMAND_SAVE && matches!(a.status, Status::Available | Status::NoMp));
    let mut any = false;
    for (e, mut p) in &mut points {
        if offer.is_some_and(|a| a.from == e)
            && let Ok(mut s) = sora.single_mut()
        {
            // FUN_00194c08 for the player's slot (the friends' slots: GAP, party.rs).
            let charging = magic.as_deref().is_some_and(|m| m.charge.active());
            let (max_hp, max_mp) = (s.stats.max_hp, s.stats.max_mp);
            if let Some((hp, mp)) = heal_step(s.hp, max_hp, s.mp, max_mp, charging) {
                if !state.healing {
                    info!("save point heal: HP {}/{max_hp} MP {}/{max_mp} (FUN_00194c08: 1 HP and 1 MP a frame while command 55 is offered)", s.hp, s.mp);
                    state.healing = true;
                }
                s.hp = hp;
                s.mp = mp;
                p.healed = true;
                any = true;
            }
            if std::mem::take(&mut p.first) {
                p.healed = true;
            }
        }
        // FUN_00194b98.
        if std::mem::take(&mut p.healed) {
            if p.heal_fx.is_none() {
                let h = FxHandle::new();
                let at = Mat4::from_rotation_translation(Quat::from_rotation_y(p.yaw), p.pos * SCALE);
                if let Some(fx_e) = p.pax.as_ref().and_then(|x| fx::start_placed(&mut commands, x, EFFECT_HEAL, Bind::Fixed(at), Placement::default())) {
                    commands.entity(fx_e).insert((h, AreaPart));
                }
                p.heal_fx = Some(h);
                info!("save point: effect element {EFFECT_HEAL} started (FUN_00194b98)");
            }
        } else if let Some(h) = p.heal_fx.take() {
            stop.write(StopEffect { handle: h, kill: false });
            info!("save point: effect element {EFFECT_HEAL} ended (FUN_001d9f20)");
        }
    }
    if state.healing && !any {
        state.healing = false;
        if let Ok(s) = sora.single() {
            info!("save point heal: done, HP {}/{} MP {}/{}", s.hp, s.stats.max_hp, s.mp, s.stats.max_mp);
        }
    }
}

/// The accept of command 55 (`FUN_00177b18` case 0x37 -> `FUN_001784b8`), dispatched by
/// `reaction_cmd::accept` (an available offer, Execute 0x37, left in `Talk::accepted`): menu kind 1
/// with mode 0 (world point) or 2 (save point), served by `menu_save` (`SaveMenu::request_point`).
/// The menu's open freezes the field (`schedule::Pause`, as the original's scene change stops the
/// field scene) and its close returns to it (`FUN_001038a0`); the area is not reloaded.
fn point_use(mut talk: ResMut<Talk>, points: Query<&SavePoint>, mut save: ResMut<crate::shared::ui::SavePointRequest>) {
    let Some(a) = talk.accepted.filter(|a| points.contains(a.from)) else { return };
    talk.accepted = None;
    let Ok(p) = points.get(a.from) else { return };
    let mode = if p.object == OBJECT_WORLD_POINT { 0 } else { 2 };
    let word = u32::from_le_bytes(p.record.raw[0x20..0x24].try_into().unwrap_or_default());
    let opened = save.request(mode);
    info!(
        "save point: accepted command {} (Execute {EXECUTE_SAVE:#x}): FUN_001784b8 sets 0x33e86e = {:#x} and opens menu kind 1 (scene 3) with mode {mode} (FUN_0024f0b8(1, {mode})): {}",
        a.cmd,
        word & 0xff,
        if opened { "requested from menu_save" } else { "the save menu cannot open (not loaded or already open)" }
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction_parts::shapes_overlap as overlaps;
    use crate::reaction_parts::Shape;

    #[test]
    fn a_cylinder_part_meets_a_body_by_fun_00188fd0() {
        let cyl = Shape { kind: 1, center: Vec3::new(0.0, 50.0, 0.0), r: [50.0, 50.0] };
        let body = |x: f32, y: f32| Shape { kind: 0, center: Vec3::new(x, y, 0.0), r: [35.0, 55.0] };
        assert_eq!(overlaps(&body(0.0, 85.0), &cyl), Some(true), "standing on the point");
        assert_eq!(overlaps(&cyl, &body(84.0, 85.0)), Some(true), "the radii add up to 85");
        assert_eq!(overlaps(&body(86.0, 85.0), &cyl), Some(false));
        // Above the cylinder's half height the ellipsoid is narrower: at 50 + 55 * 0.8 above the
        // centre k = 0.8 and the reach is 0.6 * 35 + 50 = 71.
        let y = 50.0 + 50.0 + 0.8 * 55.0;
        assert_eq!(overlaps(&body(70.0, y), &cyl), Some(true));
        assert_eq!(overlaps(&body(72.0, y), &cyl), Some(false));
        assert_eq!(overlaps(&body(0.0, 50.0 + 106.0), &cyl), Some(false), "too high");
        let c2 = Shape { kind: 1, center: Vec3::new(99.0, 50.0, 0.0), r: [50.0, 50.0] };
        assert_eq!(overlaps(&cyl, &c2), Some(true));
    }

    #[test]
    fn the_heal_adds_one_a_frame_until_full() {
        assert_eq!(heal_step(10, 20, 50, 100, false), Some((11, 51)));
        assert_eq!(heal_step(20, 20, 99, 100, false), Some((20, 100)));
        assert_eq!(heal_step(20, 20, 100, 100, false), None, "full: no heal, no flag");
        assert_eq!(heal_step(20, 20, 0, 100, true), Some((20, 0)), "a charge keeps MP and still heals");
    }

    #[test]
    fn world_points_are_found_by_world_and_area() {
        let table = vec![[1, 4, 0, 0], [2, 2, 1, 0], [2, 2, 3, 0]];
        assert_eq!(world_point_index(&table, 2, 2), Some(1));
        assert_eq!(world_point_index(&table, 2, 5), None);
    }

    /// The two type-7 objects on the disc: no mesh, one type-5 part (id 55, a cylinder) and an effect
    /// file with elements 1 and 2; tt02's `m_20` / `m_21` stand at the same point; `00worldpoint.bin`
    /// names tt02; command 55 is Execute 0x37 with the field-save message.
    #[test]
    fn the_points_on_the_disc() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let disc = Disc::open(&iso);
        let objs = disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default();
        let points: Vec<_> = objs.iter().filter(|o| o.object_type == OBJECT_POINT).map(|o| (o.object_id, o.model_name.clone())).collect();
        assert_eq!(points, vec![(0x239, "WORLD_POINT".to_string()), (0x23a, "SAVE_POINT".to_string())]);
        for (_, model) in &points {
            let f = disc.read(&format!("obj/{model}.mdlx")).expect("mdlx");
            assert!(kh2_model::mdlx(&f).is_none(), "{model}: no mesh");
            let t = parts::Table::from_mdlx(&f).expect("parts");
            let r: Vec<_> = t.reaction_parts().map(|r| (r.id, r.shape)).collect();
            assert_eq!(r, vec![(COMMAND_SAVE, 1)], "{model}");
            let a = disc.read(&format!("obj/{model}.apdx")).expect("apdx");
            let pax = kh2_bar::parse(&a).unwrap().iter().find(|e| e.kind == 18).and_then(|e| kh2_fx::Pax::parse(e.data).ok()).expect("pax");
            for id in [EFFECT_IDLE, EFFECT_HEAL] {
                assert!(pax.elements.iter().any(|e| e.id == id), "{model} element {id}");
            }
        }
        let (area, _) = kh2_ard::Area::parse(&disc.read("ard/tt02.ard").unwrap()).unwrap();
        let at = |tag: &str| area.spawns.get(tag).and_then(|g| g.first()).and_then(|g| g.entities.first()).map(|e| (e.object, e.position));
        assert_eq!(at("m_20").map(|p| p.0), Some(0x239));
        assert_eq!(at("m_21").map(|p| p.0), Some(0x23a));
        assert_eq!(at("m_20").map(|p| p.1), at("m_21").map(|p| p.1));
        let wp = disc.read("00worldpoint.bin").expect("00worldpoint.bin");
        assert_eq!(wp.len(), WORLD_POINTS * 4);
        let table: Vec<[u8; 4]> = wp.as_chunks::<4>().0.to_vec();
        let i = world_point_index(&table, 2, 2).expect("tt02 has a world point");
        eprintln!("tt02 world point: record {i} {:?}", table[i]);
        let cmds: Vec<t::Cmd> = disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok()).map(|d| d.rows).unwrap_or_default();
        let c = cmds.iter().find(|c| c.id == COMMAND_SAVE).expect("cmd 55");
        assert_eq!(c.execute, EXECUTE_SAVE);
        let row = CmdRow::from(c);
        assert_eq!(save_status(&[], &row, &Ctx::default(), false), Status::Available);
        assert_eq!(save_status(&[], &row, &Ctx::default(), true), Status::Blocked);
    }

}
