//! The party members' own BDX scripts (stage 1 of docs/party.md section 9: "the member's BDX attached
//! and put in `idle`"). Each member's model file carries a script (BAR type 3 of `obj/<model>.mdlx`,
//! `kh2 bdx P_EX020`): its init event registers the modes (`appear`, `idle`, `idle_time`, `footwork`,
//! `mode_battle`, the spells ...) and its main event only yields; the native friend AI starts modes by
//! name (docs/party.md section 4, action records +0x20). Stage 1 starts the script when the member
//! appears and switches it to `idle`, then runs it every tick through the enemy host (`BevyHost`).
//!
//! The script runs through `friend_host::FriendHost`: its motion requests go through the player-type
//! remap FUN_0017da30 (the member's motion state row, `PartyMember::motion_row`) before the enemy host
//! plays them, and the group-10 entries the members' scripts reach are answered there. The requested
//! slot is handed to `party.rs` (`PartyMember::script_slot`), which plays it while the member stands:
//! `idle` (BDX `P_EX020` R0489 -> R0499) faces the target and asks for motion 0. Movement, effects
//! and the other requests of the script are not carried out (the locomotion owns the body).
//!
//! Log: `KH2_PARTYLOG=1` prints the mode changes; `KH2_BDXLOG=1` the host calls as for enemies.

use std::collections::HashMap;

use bevy::prelude::*;
use kh2_bdx::{Program, Runtime};
use kh2_tables::tables as t;

use kh2_model::collision::kind as shape_kind;

use crate::assets::Disc;
use crate::combat::{Fighter, Hit, Team};
use crate::data::GameData;
use crate::enemy_ai::AiTrace;
use crate::enemy_host::{BevyHost, Env};
use crate::friend_host::{FriendFields, FriendHost, FxReq, TARGET_ENEMY, TARGET_OTHER};
use crate::enemy_obj::{Obj, OBJ_SIZE, PLAYER, SELF_BASE};
use crate::fx::{Bind, Placement};
use crate::party::{PartyMember, SpecialOn};
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::Sora;
use crate::world::World;

/// Script handles of the members: past the enemies' range (`enemy_ai` uses serials below 0x1000), and
/// one further on so that slot 0's handle is not `SELF_BASE + 0x1000 x OBJ_SIZE` = `PLAYER` (0x0200_0000):
/// with 0x1000 Donald's script read as the player in every host arm that tests for the player (WP-S3).
const HANDLE_BASE: u32 = 0x1001;

/// The mode the stage puts a new member's script in.
const START_MODE: &str = "idle";

/// The scripts by model name (`None`: the model has no readable script) and the executable's host table.
#[derive(Resource, Default)]
struct FriendScripts {
    by_model: HashMap<String, Option<(Program, bool)>>,
    table: Option<kh2_bdx::HostTable>,
    objs: Vec<t::Objentry>,
    loaded: bool,
}

impl FriendScripts {
    fn get(&mut self, disc: &Disc, model: &str) -> Option<(Program, bool)> {
        if !self.loaded {
            self.loaded = true;
            self.table = disc.read_root("SLUS_210.05").and_then(|exe| kh2_bdx::HostTable::from_exe(&exe));
            if let Ok(mut a) = disc.0.lock() {
                self.objs = kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default();
            }
        }
        let flags124 = self.objs.iter().find(|r| r.model_name == model).is_some_and(|r| r.flags & 8 != 0);
        self.by_model
            .entry(model.to_string())
            .or_insert_with(|| {
                let file = disc.read(&format!("obj/{model}.mdlx"))?;
                let entry = kh2_bar::parse(&file).ok()?.into_iter().find(|e| e.kind == 3)?.data.to_vec();
                match Program::parse(&entry) {
                    Ok(p) => Some((p, flags124)),
                    Err(e) => {
                        warn!("party: {model}: script not readable: {e}");
                        None
                    }
                }
            })
            .clone()
    }
}

/// A member's script runtime.
#[derive(Component)]
pub struct FriendBrain {
    rt: Runtime,
    obj: Obj,
    tag: String,
    flags124: bool,
    started: bool,
    errors_seen: usize,
    mode: Option<String>,
    fields: FriendFields,
}

/// Registers the systems (called by `party::PartyPlugin`, which runs them after its own tick).
pub(crate) fn register(app: &mut App) {
    app.init_resource::<FriendScripts>().add_systems(FixedUpdate, (friend_attach, friend_update).chain().after(crate::party::PartyTick).in_set(Sim::Enemy));
}

fn friend_attach(mut commands: Commands, disc: Res<Disc>, mut scripts: ResMut<FriendScripts>, q: Query<(Entity, &PartyMember), Without<FriendBrain>>) {
    for (ent, m) in &q {
        let Some((prog, flags124)) = scripts.get(&disc, &m.model) else { continue };
        info!("party: {} script {} ({} words) attached", m.model, prog.name, prog.words.len());
        let mut rt = Runtime::new(prog);
        if let Some(t) = &scripts.table {
            rt.set_host_table(t.clone());
        }
        rt.shared.rng = 0x51ed_270b ^ (m.slot as u32).wrapping_mul(0x85eb_ca6b);
        rt.core.mode_log = Some(Vec::new());
        let handle = SELF_BASE + (HANDLE_BASE + m.slot as u32) * OBJ_SIZE as u32;
        commands.entity(ent).insert(FriendBrain {
            rt,
            obj: Obj::new(handle),
            tag: format!("{}#{}", m.model, m.slot),
            flags124,
            started: false,
            errors_seen: 0,
            mode: None,
            fields: FriendFields::default(),
        });
    }
}

/// The motion controller's trigger bytes (+0x238 / +0x239, hosts 1.150 / 1.151) of the member after the
/// last advance, as `enemy_ai` builds them for enemies: range triggers 16..19 active, frame triggers 9..12
/// crossed (the dispatcher 0x188498). The members' scripts wait on bit 0 of +0x239 for the cast (Donald's
/// R089a after motion 192, whose trigger 9 lies at tick 26). [inferred] Frame triggers are counted from the
/// motion's first frame, as `party` does for the item motion (docs/party.md section 10).
fn trigger_bits(anim: &Animator) -> [u8; 2] {
    let (Some(t), Some(m)) = (anim.triggers(), anim.motion()) else { return [0; 2] };
    let g = |f: f32| (f - m.frame_start) * crate::data::LOGIC_HZ / m.fps.max(1.0);
    let mut bits = [0u8; 2];
    for k in 0..4u8 {
        if t.is_active(anim.gframe(), 16 + k) {
            bits[0] |= 1 << k;
        }
    }
    if anim.frame != anim.prev_frame {
        for ft in t.crossed(g(anim.prev_frame), g(anim.frame)).filter(|f| (9..=12).contains(&f.id)) {
            bits[1] |= 1 << (ft.id - 9);
        }
    }
    bits
}

/// The victims' hit memory for the members' attack objects (the victim's block +0xa50: FUN_001be108
/// tests, FUN_001be190 marks, FUN_001be300 counts down), by (member, victim). [inferred] Kept apart
/// from the spells' memory in `magic.rs`: the groups of the two never meet.
#[derive(Default)]
struct VictimMemory(HashMap<(Entity, Entity), crate::magic_rules::HitMemory>);

/// FUN_001a2cc0's sweep: after a move of more than two frame units (+4 bit 4, FUN_001a2fc8) the
/// volume is tested at the position and then in steps of `min(radius, height)` toward the previous
/// one while more than half a step is left (`DAT_0037b460` 0.5). Game units, the app's axes.
fn sweep_points(pos: Vec3, prev: Option<Vec3>, step: f32) -> Vec<Vec3> {
    let mut out = vec![pos];
    let Some(prev) = prev else { return out };
    let d = prev - pos;
    let mut left = d.length();
    if left <= 2.0 || step <= 0.0 {
        return out;
    }
    let dir = d / left;
    let mut p = pos;
    while left > step * 0.5 {
        p += dir * step;
        left -= step;
        out.push(p);
    }
    out
}

/// The size FUN_001a2cc0 tests with: with a growth (+0xcc > 0) the start size eased toward the final
/// one by `s = sin(t / frames x pi x 0.5)` (FUN_00300278 = sinf; `DAT_0037b45c` pi, `DAT_0037b460`
/// 0.5): `from x (1 - s) + final x s`; else the final size.
fn attack_size(a: &crate::friend_host::AttackObj) -> (f32, f32) {
    if a.grow > 0.0 {
        let s = (a.t / a.grow * std::f32::consts::PI * 0.5).sin();
        (a.from.0 * (1.0 - s) + a.radius * s, a.from.1 * (1.0 - s) + a.height * s)
    } else {
        (a.radius, a.height)
    }
}

/// An attack volume (FUN_00189610 with the radius and half height; kind 3, a sphere, when they are
/// equal): as `magic.rs`, a vertical segment of `height - radius` either side of the centre with the
/// radius [inferred shape]. Bevy space in, metres.
fn attack_volume(c: Vec3, radius: f32, height: f32) -> crate::combat::Volume {
    let half = (height - radius).max(0.0) * crate::SCALE;
    crate::combat::Volume { a: c - Vec3::Y * half, b: c + Vec3::Y * half, r: radius * crate::SCALE }
}

/// One pass of FUN_0018c2d8 over the member's attack objects: a released object (+4 bit 0x800) is
/// destroyed; any other runs FUN_001a2cc0: the growth's clock (+0xc8) counts one frame (`DAT_00356f40`)
/// and past +0xcc the object is released without a test; otherwise its volume is tested against the
/// enemies' hurt shapes. A hit takes the victim's memory (`HitMemory`: once per group until the row's
/// Interval) and the row variant of the hit count (FUN_0019e7e0, `magic_rules::variant`).
#[allow(clippy::too_many_arguments)]
fn step_attacks(
    me: Entity,
    m: &PartyMember,
    fields: &mut FriendFields,
    game: &GameData,
    victims: &[(Entity, Vec<(crate::combat::Volume, crate::combat::HurtSpheroid)>)],
    memory: &mut VictimMemory,
    pax: Option<&std::sync::Arc<crate::fx::LoadedPax>>,
    log: bool,
    out: &mut Vec<(Hit, Option<crate::fx::SpellHit>)>,
) {
    fields.attacks.retain(|a| {
        if a.released && log {
            info!("party: {} attack object {:#x} (atkp {} group {:#x}) destroyed (FUN_0018c2d8)", m.model, a.handle, a.atkp, a.group);
        }
        !a.released
    });
    for a in fields.attacks.iter_mut() {
        a.steps += 1;
        if a.grow > 0.0 {
            a.t += 1.0;
            if a.grow < a.t {
                a.released = true;
                if log {
                    info!("party: {} attack object {:#x} (atkp {}) released after its {} frames (FUN_001a2cc0 -> FUN_0018dc98)", m.model, a.handle, a.atkp, a.grow);
                }
                continue;
            }
        }
        let Some(pos) = a.pos else { continue };
        let (r, h) = attack_size(a);
        let step = r.min(h);
        if step <= 0.0 {
            continue;
        }
        let Some(base) = crate::magic_rules::attack_row(&game.atkp, a.atkp, a.kind) else {
            if a.steps == 1 {
                warn!("party: {} attack object atkp {} kind {}: no atkp row", m.model, a.atkp, a.kind);
            }
            continue;
        };
        if log && a.steps == 1 {
            info!("party: {} attack object {:#x}: atkp {} group {:#x} at {:.0?} radius {r} height {h} for {} frames", m.model, a.handle, a.atkp, a.group, pos, a.grow);
        }
        let points = sweep_points(pos, a.prev, step);
        for (ve, shapes) in victims {
            let mem = memory.0.entry((me, *ve)).or_default();
            if !mem.may_hit(game.atkp[base].r#type, a.group) {
                continue;
            }
            let Some((from, sph)) = points.iter().find_map(|&c| {
                let v = attack_volume(c * crate::SCALE, r, h);
                shapes.iter().find(|(hv, _)| crate::combat::overlap(&v, hv)).map(|(_, s)| (c * crate::SCALE, *s))
            }) else {
                continue;
            };
            let n = mem.count_hit(a.group);
            let vi = crate::magic_rules::variant(&game.atkp, base, n);
            let row = &game.atkp[vi];
            mem.mark(a.group, row.interval);
            let point = crate::combat::victim_point(&sph, from);
            let dir = (sph.center - m.position() * crate::SCALE).with_y(0.0).normalize_or(Vec3::Z);
            let hit = Hit { attacker: me, target: *ve, atkp: row.id as i16, dir, point, row: Some(row.sub_id) };
            info!("party: {} attack object {:#x} hits {ve:?}: atkp {} SubId {} (hit {n}, Power {}, Element {}, Interval {})", m.model, a.handle, row.id, row.sub_id, row.power, row.element, row.interval);
            out.push((hit, pax.map(|p| crate::fx::SpellHit { target: *ve, atkp: hit.atkp, pax: p.clone() })));
        }
    }
}

/// Ticks an ended effect's record is kept for the script's polls (0.49) before it is dropped.
const FX_FORGET: u32 = 600;

/// The frames of a member's hand objects (its `Fighter` attack bodies after the body), for the bone
/// numbers 0x4003.. (`fx::object_bone`).
fn hands_of(f: Option<&Fighter>, anims: &Query<&Animator>, gts: &Query<&GlobalTransform>) -> Vec<Option<crate::fx::ObjectFrame>> {
    f.map_or(Vec::new(), |f| f.attack_bodies.iter().skip(1).map(|w| Some(crate::fx::ObjectFrame::of(anims.get(*w).ok()?, gts.get(*w).ok()?.to_matrix()))).collect())
}

/// The position of an element bound to `owner` (host 0.53, FUN_001d9d00: the bind record's position):
/// the frame its bone names (FUN_0016aa18; `fx::object_bone` for the numbers with 0x4000, the bone's
/// world matrix otherwise) times the element's translation (game axes, Y and Z flipped). Bevy space,
/// from the owner's last drawn pose (one tick old).
fn element_position(pax: &crate::fx::LoadedPax, id: u16, owner: Entity, anims: &Query<&Animator>, gts: &Query<&GlobalTransform>, hands: &[Option<crate::fx::ObjectFrame>]) -> Option<Vec3> {
    let (a, g) = (anims.get(owner).ok()?, gts.get(owner).ok()?);
    let e = pax.pax.elements_for(id).next()?;
    let t = e.translation;
    let bone = e.bone as u16;
    let m = if bone & 0x4000 != 0 || e.bone < 0 {
        crate::fx::object_bone(bone, &crate::fx::ObjectFrame::of(a, g.to_matrix()), hands, true)
    } else {
        a.bone_world(g, bone as usize)
    };
    Some(m.transform_point3(Vec3::new(t[0], -t[1], -t[2])))
}

/// The model entity an effect bound to an object follows (1.87 the member, 1.149 the object named: the
/// player, or the object the special's target record names, `FriendHost::read_mem`).
fn fx_owner(on: Option<u32>, m: &PartyMember, s: &Sora, bodies: &[(u8, Entity)], fighters: &Query<&Fighter, Without<PartyMember>>) -> Option<Entity> {
    match on {
        None => Some(m.body),
        Some(PLAYER) => Some(s.body),
        Some(TARGET_OTHER) => bodies.iter().find(|(slot, _)| *slot != m.slot).map(|(_, b)| *b),
        Some(TARGET_ENEMY) => match m.special_on()? {
            SpecialOn::Enemy(e) => fighters.get(e).ok().map(|f| f.hurt_body),
            _ => None,
        },
        Some(_) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn friend_update(
    mut commands: Commands,
    world: Option<Res<World>>,
    trace: Option<ResMut<AiTrace>>,
    sora: Query<&Sora>,
    (data, game, lib): (Res<crate::party::PartyData>, Res<GameData>, Option<Res<crate::fx::EffectLib>>),
    mut q: Query<(Entity, &mut PartyMember, &mut FriendBrain)>,
    anims: Query<&Animator>,
    fighters: Query<&Fighter, Without<PartyMember>>,
    fighter_roots: Query<Entity, (With<Fighter>, Without<PartyMember>)>,
    gts: Query<&GlobalTransform>,
    mut memory: Local<VictimMemory>,
    (mut hits, mut spell_hits): (MessageWriter<Hit>, MessageWriter<crate::fx::SpellHit>),
    mut running: Query<&mut crate::fx::RunningEffect>,
    member_fighters: Query<&Fighter, With<PartyMember>>,
) {
    let (Some(mut trace), Ok(s)) = (trace, sora.single()) else { return };
    let log = std::env::var("KH2_PARTYLOG").is_ok_and(|v| !v.is_empty() && v != "0");
    let paired = HashMap::new();
    // The enemies' hurt shapes (the victims of the members' attack objects), and the members' bodies.
    let victims: Vec<_> = fighter_roots
        .iter()
        .filter_map(|e| {
            let f = fighters.get(e).ok().filter(|f| f.team == Team::Enemy && f.hittable)?;
            let (a, g) = (anims.get(f.hurt_body).ok()?, gts.get(f.hurt_body).ok()?);
            let shapes = a.rig.model.shapes.iter().filter(|sh| sh.kind == shape_kind::BATTLE || sh.kind == shape_kind::TARGET);
            Some((e, shapes.map(|sh| (crate::combat::shape_volume(sh, a, g), crate::combat::hurt_spheroid(sh, a, g))).collect::<Vec<_>>()))
        })
        .collect();
    let bodies: Vec<(u8, Entity)> = q.iter().map(|(_, m, _)| (m.slot, m.body)).collect();
    for m in memory.0.values_mut() {
        m.tick();
    }
    for (me, mut m, mut brain) in &mut q {
        let Ok(anim) = anims.get(m.body) else { continue };
        let rig = anim.rig.clone();
        let FriendBrain { rt, obj, tag, flags124, started, errors_seen, mode, fields } = &mut *brain;
        fields.order = data.order(m.motion_row);
        fields.free = m.free;
        fields.target_valid = m.target_valid();
        fields.target_point = m.target_point;
        fields.special = m.special_on();
        // Before the slice: which instances have ended (host 0.49) and where the bound ones are (0.53).
        let pax = lib.as_deref().and_then(|l| l.for_rig(&rig)).cloned();
        for f in fields.fx.iter_mut() {
            if let Some(e) = f.entity
                && running.get(e).is_err()
            {
                f.entity = None;
                f.gone = true;
            }
            if f.gone {
                f.gone_ticks += 1;
                continue;
            }
            let (FxReq::Bound { id, on }, Some(p)) = (f.req, &pax) else { continue };
            let owner = fx_owner(on, &m, s, &bodies, &fighters);
            let hands: Vec<Option<crate::fx::ObjectFrame>> = if on.is_some() { Vec::new() } else { hands_of(member_fighters.get(me).ok(), &anims, &gts) };
            if let Some(at) = owner.and_then(|o| element_position(p, id, o, &anims, &gts, &hands)) {
                f.pos = at / crate::SCALE;
            }
        }
        obj.motor.pos = m.position();
        obj.motor.yaw = m.yaw();
        obj.motor.grounded = true;
        obj.slot = anim.slot;
        obj.frame = anim.frame;
        obj.trigger_bits = trigger_bits(anim);
        let env = Env {
            player: s.motor.pos,
            player_yaw: s.motor.yaw,
            world: world.as_deref(),
            rig: &rig,
            flags124: *flags124,
            enmp: 0,
            player_serial: 0,
            player_motion_done: true,
            paired: &paired,
            player_model: 0,
            mission_counters: None,
            event_active: false,
        };
        let handle = obj.handle;
        let mut host = FriendHost { inner: BevyHost { obj, env: &env, trace: &mut trace, tag: tag.as_str() }, fields, game: &game };
        if !*started {
            *started = true;
            rt.spawn(&mut host, handle);
            rt.set_mode_named(&mut host, START_MODE);
        }
        // FUN_00113d48 phase 0: the special's mode (FUN_00186e70 on the script controller +0x300) and
        // the pending MP byte +0xaf4.
        if let Some(want) = m.mode_request.take() {
            host.fields.pending_mp = m.pending_mp;
            rt.set_mode_named(&mut host, &want);
            if log {
                info!("party: {tag} special: script mode {want:?} asked (pending MP {})", m.pending_mp);
            }
        }
        rt.update(&mut host);
        // Host 1.276 paid the pending MP: FUN_00186368 on the member (an MP charge when it is short).
        if let Some(cost) = host.fields.paid.take() {
            m.pay_mp(cost);
        }
        // Hosts 2.10 / 2.17: the script's attack hits on its target record, for `party` to deliver.
        m.script_hits.append(&mut host.fields.target_hits);
        m.pending_mp = host.fields.pending_mp;
        // The effects the script holds (1.22, 1.87, 1.149): elements of the member's own effect file,
        // started, moved, stopped and destroyed as it asked.
        for f in fields.fx.iter_mut() {
            let owner = match f.req {
                FxReq::Bound { on, .. } => fx_owner(on, &m, s, &bodies, &fighters),
                FxReq::At { .. } => None,
            };
            if !f.started {
                f.started = true;
                let (id, bind, place) = match f.req {
                    FxReq::Bound { id, .. } => (id, owner.map(Bind::Owner), Placement::default()),
                    FxReq::At { id, pos } => (id, Some(Bind::Fixed(Mat4::from_translation(pos * crate::SCALE))), Placement { replace_translation: true, ..default() }),
                };
                f.entity = match (&pax, bind) {
                    (Some(p), Some(b)) => crate::fx::start_placed(&mut commands, p, id, b, place),
                    _ => None,
                };
                if f.entity.is_none() {
                    f.gone = true;
                }
                if log {
                    info!("party: {tag} effect element {id} of {} ({:?}): {}", rig.apdx.as_deref().unwrap_or("no effect file"), f.req, if f.entity.is_some() { "started" } else { "not started (no such element, file or owner)" });
                }
            }
            if let Some(e) = f.entity {
                if f.destroy {
                    if running.get(e).is_ok() {
                        commands.entity(e).despawn();
                    }
                    if log {
                        info!("party: {tag} effect {:#x} destroyed (0.51)", f.handle);
                    }
                    f.entity = None;
                    continue;
                }
                if let Ok(mut r) = running.get_mut(e) {
                    if std::mem::take(&mut f.stop) {
                        r.effect.end();
                    }
                    if std::mem::take(&mut f.moved) {
                        r.bind = Bind::Fixed(Mat4::from_translation(f.pos * crate::SCALE));
                    }
                }
            }
        }
        fields.fx.retain(|f| !f.destroy && f.gone_ticks < FX_FORGET);
        // FUN_0018c2d8's pass over the member's attack objects.
        let mut out = Vec::new();
        step_attacks(me, &m, fields, &game, &victims, &mut memory, pax.as_ref(), log, &mut out);
        for (h, sh) in out {
            hits.write(h);
            if let Some(sh) = sh {
                spell_hits.write(sh);
            }
        }
        let now = rt.current_mode();
        m.script_mode = now.clone();
        if now != *mode {
            if log {
                info!("party: {tag} script mode {:?} -> {:?}", mode.as_deref().unwrap_or("-"), now.as_deref().unwrap_or("-"));
            }
            *mode = now;
        }
        if let Some(req) = obj.motion_req.take() {
            if log && m.script_slot != Some(req.slot) {
                info!("party: {tag} script asked for slot {} (FUN_0017da30 row {}): played while the member stands", req.slot, m.motion_row);
            }
            m.script_slot = Some(req.slot);
            m.script_restart = true;
            m.script_queue = None;
        }
        if let Some(q) = fields.queued.take() {
            m.script_queue = Some(q);
        }
        // Nothing else the script asked for is carried out.
        obj.move_vel = Vec3::ZERO;
        obj.effects.clear();
        obj.outbox.clear();
        obj.player_cmds.clear();
        obj.drops.clear();
        for entry in rt.core.mode_log.as_mut().map(std::mem::take).unwrap_or_default() {
            if log {
                info!("party: {tag} script mode {entry}");
            }
        }
        for (pc, err) in &rt.core.errors[*errors_seen..] {
            warn!("party: {tag}: script error {err:?} at {pc:#06x}");
        }
        *errors_seen = rt.core.errors.len();
    }
}
