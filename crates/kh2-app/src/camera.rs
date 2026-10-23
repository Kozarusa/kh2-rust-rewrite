//! Third-person camera after the original's field camera (model and sources in `docs/systems/player-camera/contract.md`,
//! math in `camera_rules`). The camera runs on the game's 60 Hz tick right after the simulation, so
//! it sees Sora exactly as the tick left him. Keys map to the pad's sticks: WASD is the left stick,
//! the arrow keys are the right stick (left/right turn the camera, up/down zoom), C resets the
//! camera behind Sora (R3), V is Select (the look-around, and the overhead camera's second view).
//! The mouse (right-drag, wheel) and Page Up/Down are extras.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::assets::{CurrentArea, Disc};
use crate::data::GameData;
use crate::schedule::Sim;
use crate::shared::sora::{Pad, Sora, State};
use crate::world::{CameraWorld, World};
use crate::SCALE;

#[path = "camera_rules.rs"]
pub mod rules;

use rules::{area_setup, Blocker, EventCamera, Hit, Mode, Params, Rig, Sticks, Subject, TickIn, View};

/// Camera state shared with the rest of the game. `yaw` is the angle of the camera's offset from
/// Sora (the camera sits at `(sin yaw, ., cos yaw)` from him); movement is relative to it.
/// `distance` is the orbit radius in metres. The harness may write both: a value that changed
/// since the camera last wrote it is taken as a new setting.
#[derive(Component)]
pub struct OrbitCamera {
    pub yaw: f32,
    /// Elevation of the camera above its look-at point (output only).
    pub pitch: f32,
    pub distance: f32,
    /// The look-around (mode 1) is on: the camera sits in Sora's head.
    pub first_person: bool,
    /// The player's draw fade the look-around drives (`player + 0x834`, 1 = drawn). Output;
    /// `sora::apply_player_fade` applies it to Sora's body and weapons.
    pub player_fade: f32,
}

/// Dynamic camera blockers for this tick: spheres objects register (the game rebuilds its list
/// from every object's collision shapes of kind 0x12 each time the camera asks,
/// `FUN_0015a3e8`). `camera_blockers.rs` fills it every tick before the camera runs; the camera
/// takes it.
#[derive(Resource, Default)]
pub struct CamBlockers(pub Vec<Blocker>);

/// A scripted (event) camera. While `active` is set the field camera is idle and the view is
/// the event's (`FUN_00106818(1)` stops the camera task); whoever runs the event calls
/// `EventCamera::frame` each tick. Putting the final view into `handover` and clearing `active`
/// gives the field camera back with the handover of `FUN_0015c8f8`. No script drives it yet.
#[derive(Resource, Default)]
pub struct CamEvent {
    pub active: Option<EventCamera>,
    pub handover: Option<View>,
    /// The intro-camera mode (WP-I1, `intro.rs`): an event's camera track wanted this tick, and whether it is
    /// pulled in against the map (event opcode 0x46, `DAT_0036bce4`). While it is set the event camera runs it
    /// through `FUN_0015ce00` (no springs: the track's events have no opcode 0x4c); when it stops coming the
    /// field camera takes over from the last view (`FUN_0015c8c8`, `Rig::handover`).
    pub track: Option<(View, bool)>,
    /// The field camera placed straight behind the player this tick (`FUN_00140e90`: `FUN_0013fc90`,
    /// `FUN_0013f470`), what an event track's fly-back (opcode 0x31) aims at. Written by `tick`.
    pub behind: Option<View>,
    track_owned: bool,
    /// The talk camera (`npc.rs`'s talk task): `Start` is `FUN_00159688` (mode 4 with the shot),
    /// `End` is `FUN_00159768` (the warp back to the follow camera). Taken by the next `tick`.
    pub talk: Option<TalkCam>,
}

/// A request to the talk camera (mode 4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TalkCam {
    Start(rules::TalkShot),
    End,
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.08)))
            .insert_resource(CamState { log: std::env::var_os("KH2_CAM_LOG").is_some(), ..default() })
            .init_resource::<CamBlockers>()
            .init_resource::<CamEvent>()
            .add_systems(Startup, (setup, load_params))
            .add_systems(Update, (refresh_area, gather_input.run_if(crate::schedule::sim_running)).chain().in_set(crate::schedule::Frame::Camera))
            .add_systems(FixedUpdate, tick.after(Sim::EffectTick).after(crate::camera_blockers::fill).run_if(crate::schedule::sim_running));
    }
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        // Near plane 1 cm like the game's projection (`FUN_001078f0`).
        Projection::Perspective(PerspectiveProjection { near: 0.01, ..default() }),
        Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera { yaw: 0.0, pitch: 0.0, distance: 0.0, first_person: false, player_fade: 1.0 },
        // The lens pass draws over this camera's picture (fx_lens.rs).
        crate::fx::lens::LensScene,
    ));
}

#[derive(Resource)]
pub struct CamParams(pub Params);

/// The camera table lives in the main executable, which is on the disc.
fn load_params(mut commands: Commands, disc: Res<Disc>) {
    match disc.read_root("SLUS_210.05").and_then(|exe| Params::from_exe(&exe)) {
        Some(p) => {
            commands.insert_resource(CamParams(p));
        }
        None => warn!("camera: could not read the camera table from SLUS_210.05; the camera stays where it is"),
    }
}

/// Input gathered every rendered frame, consumed by the next tick.
#[derive(Default)]
struct Pending {
    yaw: f32,
    zoom: f32,
    reset: bool,
    select: bool,
}

#[derive(Resource, Default)]
struct CamState {
    rig: Option<Rig>,
    /// Area the collision and the table row were chosen for.
    area: Option<String>,
    row: usize,
    /// Mode the area starts in (`FUN_00177128`).
    mode: Option<Mode>,
    /// Area camera type (`DAT_0035cd38`); `KH2_CAM_TYPE` sets it, no Twilight Town area has one.
    cam_type: u8,
    /// The map's camera collision (BAR type 11). Without one the game's camera passes through walls.
    collision: Option<World>,
    last_yaw: f32,
    last_dist: f32,
    prev_pos: Vec3,
    /// The area `area::AreaRecord.placed` named when the rig was last rebuilt for it.
    placed: Option<String>,
    pending: Pending,
    /// `KH2_CAM_LOG=1`: log the camera twice a second.
    log: bool,
    ticks: u32,
    /// The last tick a dynamic blocker clipped the camera (for the log line on change).
    blocked: bool,
}

/// World codes in the game's world order (OpenKH `docs/kh2/worlds.md`).
const WORLDS: [&str; 19] = ["zz", "es", "tt", "di", "hb", "bb", "he", "al", "mu", "po", "lk", "lm", "dc", "wi", "nm", "wm", "ca", "tr", "eh"];

/// The area's flags from `03system.bin`'s `arif` table (OpenKH `Arif.cs`): bit 1 = indoor.
fn area_flags(disc: &Disc, area: &str) -> Option<u32> {
    let (w, a) = area.split_at(2);
    let world = WORLDS.iter().position(|x| *x == w)?;
    let index: usize = a.parse().ok()?;
    let sys = disc.read("03system.bin")?;
    let arif = kh2_bar::parse(&sys).ok()?.into_iter().find(|e| e.tag == "arif")?.data;
    let u16_at = |o: usize| arif.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]]));
    let count = u16_at(8 + 4 * world)? as usize;
    let off = u16_at(8 + 4 * world + 2)? as usize;
    if index >= count {
        return None;
    }
    let rec = off + 0x40 * index;
    arif.get(rec..rec + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The map's camera collision: the BAR entry of type 11 (`CH_t...`), in COCT format.
pub fn camera_collision(disc: &Disc, area: &str) -> Option<World> {
    let map = disc.read(&format!("map/us/{area}.map"))?;
    let coct = kh2_bar::parse(&map).ok()?.into_iter().filter(|e| e.kind == 11).find_map(|e| kh2_map::Coct::parse(e.data))?;
    Some(World::from_coct(&coct))
}

/// The camera's ray against a collision world (`FUN_00141f08` -> `FUN_00141d90`): the nearest
/// polygon on the segment, front faces only (`FUN_001623a0` rejects a polygon whose normal points
/// along the ray). Render meshes (`KH2_CAM_RENDER`) have no reliable winding, so both faces count
/// there. Returns the hit point, the polygon normal and its flags word.
pub fn cast_world(w: &World, a: Vec3, b: Vec3) -> Option<Hit> {
    let d = b - a;
    let len = d.length();
    if len < 1e-3 {
        return None;
    }
    let dir = d / len;
    let (lo, hi) = (a.min(b), a.max(b));
    let mut best: Option<(f32, Hit)> = None;
    for t in &w.tris {
        if t.hi.x < lo.x || t.lo.x > hi.x || t.hi.y < lo.y || t.lo.y > hi.y || t.hi.z < lo.z || t.lo.z > hi.z {
            continue;
        }
        if w.coct && t.n.dot(dir) >= 0.0 {
            continue;
        }
        let Some(s) = ray_tri(a, dir, &t.v) else { continue };
        if s <= len && best.is_none_or(|(b, _)| s < b) {
            best = Some((s, Hit { point: a + dir * s, normal: t.n, attr: t.flags }));
        }
    }
    best.map(|(_, h)| h)
}

/// Möller-Trumbore ray/triangle: distance along `d` (unit) from `o`, either face.
fn ray_tri(o: Vec3, d: Vec3, v: &[Vec3; 3]) -> Option<f32> {
    let e1 = v[1] - v[0];
    let e2 = v[2] - v[0];
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - v[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let vv = d.dot(q) * inv;
    if vv < 0.0 || u + vv > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t > 0.0).then_some(t)
}

/// Picks the table row, the starting mode and the collision for a newly loaded area
/// (`FUN_00177128`: the area camera type, else row 1 for an indoor area).
fn refresh_area(disc: Res<Disc>, area: Res<CurrentArea>, mut st: ResMut<CamState>) {
    let Some(loaded) = area.loaded.clone() else { return };
    if st.area.as_deref() == Some(loaded.as_str()) {
        return;
    }
    let flags = area_flags(&disc, &loaded);
    st.cam_type = std::env::var("KH2_CAM_TYPE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let (row, mode) = area_setup(st.cam_type, flags.is_some_and(|f| f & 2 != 0));
    st.row = row;
    st.mode = Some(mode);
    st.collision = camera_collision(&disc, &loaded);
    info!(
        "{loaded}: camera type {} row {} mode {:?} (arif flags {}), camera collision {}",
        st.cam_type,
        st.row,
        mode,
        flags.map_or("?".into(), |f| format!("{f:#x}")),
        st.collision.as_ref().map_or("none".into(), |w| format!("{} triangles, bounds {} .. {} m", w.tris.len(), w.min * SCALE, w.max * SCALE))
    );
    st.area = Some(loaded);
    st.rig = None;
}

/// `KH2_KEYS` entries `t:V[:hold]` press Select at `t` seconds (the harness does not know V).
fn harness_select(t: f32, dt: f32) -> bool {
    let Ok(spec) = std::env::var("KH2_KEYS") else { return false };
    spec.split(',').any(|item| {
        let mut parts = item.split(':');
        matches!((parts.next().and_then(|a| a.parse::<f32>().ok()), parts.next()), (Some(at), Some("V")) if t >= at && t - dt < at)
    })
}

#[allow(clippy::too_many_arguments)] // Bevy system parameters
fn gather_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    cam: Query<&OrbitCamera>,
    mut pad: ResMut<Pad>,
    mut st: ResMut<CamState>,
) {
    // While the look-around runs the player applies no velocity (his method +0x84,
    // `FUN_00179108`, is false in camera mode 1, so `FUN_00174bd8` skips the move): the left
    // stick does not move Sora. The app drops the stick before his tick reads it; what the
    // original does with the stick's facing and motion requests then is not known (docs/systems/player-camera/contract.md).
    if cam.single().is_ok_and(|c| c.first_person) {
        pad.dir = Vec3::ZERO;
        pad.stick = 0.0;
    }
    let p = &mut st.pending;
    if buttons.pressed(MouseButton::Right) {
        p.yaw -= motion.delta.x * 0.005;
        p.zoom += motion.delta.y * 1.5;
    }
    p.zoom -= scroll.delta.y * 40.0;
    let axis = |a: KeyCode, b: KeyCode| (keys.pressed(a) as i32 - keys.pressed(b) as i32) as f32;
    p.zoom += axis(KeyCode::PageDown, KeyCode::PageUp) * time.delta_secs() * 600.0;
    p.reset |= keys.just_pressed(KeyCode::KeyC) || buttons.just_pressed(MouseButton::Middle);
    p.select |= keys.just_pressed(KeyCode::KeyV) || harness_select(time.elapsed_secs(), time.delta_secs());
}

/// Keyboard as a stick: unit direction (x right, y down like the pad) and magnitude.
fn stick(x: f32, y: f32, mag: f32) -> (Vec2, f32) {
    let v = Vec2::new(x, y);
    if v == Vec2::ZERO { (Vec2::ZERO, 0.0) } else { (v.normalize(), mag) }
}

#[allow(clippy::too_many_arguments)] // Bevy system parameters
fn tick(
    keys: Res<ButtonInput<KeyCode>>,
    params: Option<Res<CamParams>>,
    data: Res<GameData>,
    render_world: Option<Res<CameraWorld>>,
    mut st: ResMut<CamState>,
    mut blockers: ResMut<CamBlockers>,
    mut event: ResMut<CamEvent>,
    mut sora: Query<&mut Sora>,
    targets: Query<&GlobalTransform>,
    mut cam: Query<(&mut Transform, &mut OrbitCamera, &mut Projection)>,
    options: Option<Res<crate::menu::config::Options>>,
    area_rec: Option<Res<crate::area::AreaRecord>>,
) {
    let options = options.map(|o| *o).unwrap_or_default();
    let (Some(params), Ok(mut s), Ok((mut t, mut o, mut proj))) = (params, sora.single_mut(), cam.single_mut()) else { return };
    let (Some(mode0), false) = (st.mode, st.area.is_none()) else { return };
    let st = &mut *st;
    // Area start (`FUN_001047a8` -> `FUN_00149bb8` -> `FUN_00140c00` -> `FUN_0013f470`): the
    // camera is placed straight behind the player, before the fade-in (`FUN_0013efe8`). Sora is
    // put at the entrance by `area::run_area_script` (after a jump or with `KH2_ENTRANCE`),
    // possibly after the rig was built for the area, so the rig is rebuilt once he stands there
    // [inferred: the game spawns the player before `FUN_001047a8`].
    // A jump clears `placed` until Sora stands at the new entrance (also a jump into the same area).
    match area_rec.as_ref().and_then(|r| r.placed.as_ref()) {
        None => st.placed = None,
        Some(placed) if st.placed.as_ref() != Some(placed) => {
            st.placed = Some(placed.clone());
            st.rig = None;
            st.prev_pos = s.motor.pos;
            info!("camera: placed behind Sora at the entrance of {placed} (FUN_0013f470)");
        }
        Some(_) => {}
    }
    let params = &params.0;
    let use_render = std::env::var_os("KH2_CAM_RENDER").is_some();
    let collider: Option<&World> = if use_render { render_world.as_deref().map(|w| &w.0) } else { st.collision.as_ref() };
    let cast = |a: Vec3, b: Vec3| collider.and_then(|w| cast_world(w, a, b));

    let pos = s.motor.pos;
    let warp = pos.distance(st.prev_pos) > 500.0;
    // `can_look` stands in for the player's state flags (`FUN_0019ae30` for the look-around,
    // `FUN_001b7e00` for its turn): only the ground state here (inferred; the flags' owners are
    // not decoded).
    let mut subject = Subject {
        pos,
        facing: s.motor.forward(),
        moved: pos.xz() != st.prev_pos.xz(),
        run_speed: data.prty.run_speed,
        eye_height: data.prty.subjective_offset,
        can_look: s.state == State::Ground,
    };
    st.prev_pos = pos;
    let row = params.rows[st.row.min(3)];
    let rig = st.rig.get_or_insert_with(|| {
        let mut r = Rig::new(params, row, &subject, &cast);
        r.mode = mode0;
        r
    });
    rig.row = row;
    if let Some(v) = event.handover.take() {
        rig.handover(params, &subject, v);
    }
    // The intro-camera mode (WP-I1): the event track drives the event camera; when it ends the field camera
    // takes the last view over.
    let behind = Rig::new(params, row, &subject, &cast);
    event.behind = Some(View { look: behind.look, pos: behind.pos, fov: behind.fov, roll: behind.roll });
    match event.track.take() {
        Some((v, collide)) => {
            if !event.track_owned {
                info!("camera: intro camera on (event camera track, FUN_0022b0f8 -> FUN_0015ce00)");
            }
            event.track_owned = true;
            let ev = event.active.get_or_insert_with(|| EventCamera::start(rig));
            ev.frame(params, v.pos, v.look, v.fov, v.roll, collide.then_some((&cast as rules::Cast, None)), false);
        }
        None if event.track_owned => {
            event.track_owned = false;
            if let Some(ev) = event.active.take() {
                rig.handover(params, &subject, ev.view());
                info!("camera: intro camera off: the field camera takes over from {:.0} (FUN_0015c8c8)", ev.view().pos);
            }
        }
        None => {}
    }
    let talk_req = event.talk.take();
    match talk_req {
        Some(TalkCam::Start(shot)) => {
            rig.start_talk(params, shot);
            info!(
                "camera: talk camera on (FUN_00159688, mode 4): look {:.0}, distance {:.0}, height {:.0}, {:?}, fov {:.3}",
                shot.look, shot.dist, shot.height, shot.frame, params.talk.fov
            );
        }
        Some(TalkCam::End) => {
            rig.end_talk();
            info!("camera: talk camera off (FUN_00159768 -> FUN_00159728: warp flag, mode {:?})", rig.mode);
        }
        None => {}
    }
    let mode_before = rig.mode;
    // An event owns the view: the field camera idles (`FUN_00140a80` skips while
    // `FUN_00106848` is set).
    let view = if let Some(ev) = &event.active {
        blockers.0.clear();
        st.pending = Pending::default();
        ev.view()
    } else {
        // The harness (KH2_YAW, KH2_DIST) writes the component; take a changed value as a new setting.
        if (o.yaw - st.last_yaw).abs() > 1e-6 {
            rig.set_yaw(o.yaw);
        }
        if (o.distance - st.last_dist).abs() > 1e-6 && o.distance > 0.0 {
            rig.radius = o.distance / SCALE;
        }
        let p = std::mem::take(&mut st.pending);
        if p.yaw != 0.0 {
            let y = rig.yaw + p.yaw;
            rig.set_yaw(y);
        }
        let pressed = |k: KeyCode| keys.pressed(k) as i32 as f32;
        let (left, left_mag) = stick(
            pressed(KeyCode::KeyD) - pressed(KeyCode::KeyA),
            pressed(KeyCode::KeyS) - pressed(KeyCode::KeyW),
            if keys.pressed(KeyCode::ControlLeft) { 0.5 } else { 1.0 },
        );
        // Config: Camera Left/Right and Up/Down "Reverse" flip the right stick's axes [inferred: the game swaps the
        // two logical camera bits of each pair, `FUN_00157f98..00158090`].
        let flip = |on: bool| if on { -1.0 } else { 1.0 };
        let (right, right_mag) = stick(
            flip(options.camera_lr_reverse) * (pressed(KeyCode::ArrowRight) - pressed(KeyCode::ArrowLeft)),
            flip(options.camera_ud_reverse) * (pressed(KeyCode::ArrowDown) - pressed(KeyCode::ArrowUp)),
            1.0,
        );
        let sticks = Sticks { left, left_mag, right, right_mag };
        // The player's own update turns him with the right stick while the look-around runs
        // (`FUN_001779e8`, before the camera's tick); the look yaw then follows his facing.
        if rig.mode == Mode::Look && subject.can_look {
            let d = rules::look_turn(params, &sticks);
            if d != 0.0 {
                s.motor.yaw = rules::wrap(s.motor.yaw + d);
                subject.facing = s.motor.forward();
            }
        }
        // Lock-on target: the locked enemy's position, a metre up (the game uses the object's own
        // target point, not decoded yet).
        let lock = s.lock.and_then(|l| targets.get(l).ok()).map(|g| g.translation() / SCALE + Vec3::Y * 100.0);
        let list = std::mem::take(&mut blockers.0);
        rig.tick(
            params,
            &TickIn {
                sticks,
                reset: p.reset,
                select: p.select,
                subject,
                lock,
                zoom: p.zoom,
                auto: !options.field_camera_manual,
                warp,
                cast: &cast,
                blockers: &list,
            },
        );
        if rig.blocked != st.blocked {
            st.blocked = rig.blocked;
            info!("camera: dynamic blocker clip {} ({} blockers, FUN_0015a418)", if rig.blocked { "on" } else { "off" }, list.len());
        }
        View { look: rig.look, pos: rig.pos, fov: rig.fov, roll: rig.roll }
    };
    if matches!(talk_req, Some(TalkCam::Start(_))) && event.active.is_none() {
        info!("camera: talk view placed (FUN_001592d8, a cut): pos {:.0} look {:.0} sora {:.0}", view.pos, view.look, pos);
    }
    if matches!(talk_req, Some(TalkCam::End)) && event.active.is_none() {
        info!("camera: back to the follow camera: pos {:.0} look {:.0} yaw {:.2} radius {:.0}", view.pos, view.look, rig.yaw, rig.radius);
    }
    if (mode_before == Mode::Look) != (rig.mode == Mode::Look) {
        info!("camera: look-around {} (mode {:?} -> {:?})", if rig.mode == Mode::Look { "on" } else { "off" }, mode_before, rig.mode);
    }

    if st.log {
        st.ticks += 1;
        if st.ticks.is_multiple_of(30) {
            info!(
                "camera: mode {:?} row {} pos {:.0} look {:.0} yaw {:.2} radius {:.0} fov {:.2} look_yaw {:.2} pitch {:.2} fade {:.2} sora {:.0} lock {:?} event {}",
                rig.mode,
                st.row,
                view.pos,
                view.look,
                rig.yaw,
                rig.radius,
                view.fov,
                rig.look_yaw,
                rig.look_pitch,
                rig.fade.value,
                pos,
                s.lock,
                event.active.is_some()
            );
        }
    }
    o.yaw = rig.yaw;
    o.pitch = rig.pitch();
    o.distance = rig.radius * SCALE;
    o.first_person = rig.mode == Mode::Look;
    o.player_fade = rig.fade.value;
    st.last_yaw = o.yaw;
    st.last_dist = o.distance;

    let (pos_m, look_m) = (view.pos * SCALE, view.look * SCALE);
    if pos_m.is_finite() && look_m.is_finite() && pos_m.distance_squared(look_m) > 1e-8 {
        *t = Transform::from_translation(pos_m).looking_at(look_m, Vec3::Y);
        if view.roll != 0.0 {
            // Roll about the view axis; the sign is not checked against the original (only event
            // cameras roll, and none runs here yet).
            t.rotate_local_z(view.roll);
        }
    }
    // The game's field of view is horizontal for a 4:3 picture; Bevy wants the vertical one.
    if let Projection::Perspective(pp) = &mut *proj {
        pp.fov = 2.0 * (0.75 * (view.fov * 0.5).tan()).atan();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three Twilight Town camera collisions from the user's disc: every polygon's attribute
    /// word, to see whether any carries the camera's special value 5 in its low four bits
    /// (`FUN_00143e40`). Skipped without a disc.
    #[test]
    fn twilight_town_camera_collision_attributes() {
        let Ok(iso) = crate::assets::iso_path() else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        for area in ["tt00", "tt02", "tt04"] {
            let Some(w) = camera_collision(&disc, area) else { panic!("{area}: no camera collision") };
            let mut by_attr = std::collections::BTreeMap::new();
            for t in &w.tris {
                *by_attr.entry(t.flags).or_insert(0usize) += 1;
            }
            let fives = w.tris.iter().filter(|t| t.flags & 0xf == 5).count();
            eprintln!("{area}: {} triangles, attribute 5: {fives}, flags {by_attr:#x?}", w.tris.len());
            assert!(!w.tris.is_empty());
            // A ray across the box is stopped by it, from the inside only (front faces).
            let c = (w.min + w.max) * 0.5;
            let out = Vec3::new(w.max.x + 1000.0, c.y, c.z);
            assert!(cast_world(&w, c, out).is_some(), "{area}: no wall on the way out");
        }
    }

    #[test]
    fn cast_world_returns_the_nearest_front_face() {
        // Two walls facing +x at x = 100 and x = 200 (a COCT world: front faces only).
        let mut c = kh2_map::Coct::default();
        for x in [200.0f32, 100.0] {
            c.polygons.push(kh2_map::Polygon {
                verts: [[x, -500.0, -500.0], [x, 500.0, -500.0], [x, 500.0, 500.0], [x, -500.0, 500.0]],
                quad: true,
                normal: [1.0, 0.0, 0.0],
                d: -x,
                flags: 0x15,
                material: 0,
                mesh: 0,
            });
        }
        let w = World::from_coct(&c);
        let h = cast_world(&w, Vec3::new(400.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0)).unwrap();
        assert!((h.point.x - 200.0).abs() < 1e-3, "{h:?}");
        assert_eq!(h.attr & 0xf, 5);
        assert!((h.normal - Vec3::X).length() < 1e-4);
        assert!(cast_world(&w, Vec3::ZERO, Vec3::new(400.0, 0.0, 0.0)).is_none(), "back faces are not hit");
        assert!(cast_world(&w, Vec3::new(400.0, 0.0, 0.0), Vec3::new(300.0, 0.0, 0.0)).is_none(), "stops short");
    }
}
