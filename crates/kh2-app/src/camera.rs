//! Third-person camera after the original's field camera (model and sources in `docs/camera.md`,
//! math in `camera_rules`). The camera runs on the game's 60 Hz tick right after the simulation, so
//! it sees Sora exactly as the tick left him. Keys map to the pad's sticks: WASD is the left stick,
//! the arrow keys are the right stick (left/right turn the camera, up/down zoom), C resets the
//! camera behind Sora (R3). The mouse (right-drag, wheel) and Page Up/Down are extras.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::assets::{CurrentArea, Disc};
use crate::data::GameData;
use crate::schedule::Sim;
use crate::sora::Sora;
use crate::world::{CameraWorld, World};
use crate::SCALE;

#[path = "camera_rules.rs"]
pub mod rules;

use rules::{Params, Rig, Sticks, Subject, TickIn};

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
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.08)))
            .insert_resource(CamState { log: std::env::var_os("KH2_CAM_LOG").is_some(), ..default() })
            .add_systems(Startup, (setup, load_params))
            .add_systems(Update, (refresh_area, gather_input).chain().in_set(crate::schedule::Frame::Camera))
            .add_systems(FixedUpdate, tick.after(Sim::EffectTick));
    }
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        // Near plane 1 cm like the game's projection (`FUN_001078f0`).
        Projection::Perspective(PerspectiveProjection { near: 0.01, ..default() }),
        Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera { yaw: 0.0, pitch: 0.0, distance: 0.0 },
    ));
}

#[derive(Resource)]
struct CamParams(Params);

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
}

#[derive(Resource, Default)]
struct CamState {
    rig: Option<Rig>,
    /// Area the collision and the table row were chosen for.
    area: Option<String>,
    row: usize,
    /// The map's camera collision (BAR type 11). Without one the game's camera passes through walls.
    collision: Option<World>,
    last_yaw: f32,
    last_dist: f32,
    prev_pos: Vec3,
    pending: Pending,
    /// `KH2_CAM_LOG=1`: log the camera twice a second.
    log: bool,
    ticks: u32,
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
fn camera_collision(disc: &Disc, area: &str) -> Option<World> {
    let map = disc.read(&format!("map/us/{area}.map"))?;
    let coct = kh2_bar::parse(&map).ok()?.into_iter().filter(|e| e.kind == 11).find_map(|e| kh2_map::Coct::parse(e.data))?;
    Some(World::from_coct(&coct))
}

/// Picks the table row and collision for a newly loaded area (`FUN_00177128`: an indoor area uses
/// the second row).
fn refresh_area(disc: Res<Disc>, area: Res<CurrentArea>, mut st: ResMut<CamState>) {
    let Some(loaded) = area.loaded.clone() else { return };
    if st.area.as_deref() == Some(loaded.as_str()) {
        return;
    }
    let flags = area_flags(&disc, &loaded);
    st.row = usize::from(flags.is_some_and(|f| f & 2 != 0));
    st.collision = camera_collision(&disc, &loaded);
    info!(
        "{loaded}: camera row {} (arif flags {}), camera collision {}",
        st.row,
        flags.map_or("?".into(), |f| format!("{f:#x}")),
        st.collision.as_ref().map_or("none".into(), |w| format!("{} triangles, bounds {} .. {} m", w.tris.len(), w.min * SCALE, w.max * SCALE))
    );
    st.area = Some(loaded);
    st.rig = None;
}

fn gather_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut st: ResMut<CamState>,
) {
    let p = &mut st.pending;
    if buttons.pressed(MouseButton::Right) {
        p.yaw -= motion.delta.x * 0.005;
        p.zoom += motion.delta.y * 1.5;
    }
    p.zoom -= scroll.delta.y * 40.0;
    let axis = |a: KeyCode, b: KeyCode| (keys.pressed(a) as i32 - keys.pressed(b) as i32) as f32;
    p.zoom += axis(KeyCode::PageDown, KeyCode::PageUp) * time.delta_secs() * 600.0;
    p.reset |= keys.just_pressed(KeyCode::KeyC) || buttons.just_pressed(MouseButton::Middle);
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
    sora: Query<&Sora>,
    targets: Query<&GlobalTransform>,
    mut cam: Query<(&mut Transform, &mut OrbitCamera, &mut Projection)>,
) {
    let (Some(params), Ok(s), Ok((mut t, mut o, mut proj))) = (params, sora.single(), cam.single_mut()) else { return };
    if st.area.is_none() {
        return;
    }
    let st = &mut *st;
    let params = &params.0;
    let use_render = std::env::var_os("KH2_CAM_RENDER").is_some();
    let collider: Option<&World> = if use_render { render_world.as_deref().map(|w| &w.0) } else { st.collision.as_ref() };
    let cast = |a: Vec3, b: Vec3| collider.and_then(|w| w.raycast_m(a * SCALE, b * SCALE)).map(|d| d / SCALE);

    let pos = s.motor.pos;
    let warp = pos.distance(st.prev_pos) > 500.0;
    let subject = Subject { pos, facing: s.motor.forward(), moved: pos.xz() != st.prev_pos.xz(), run_speed: data.prty.run_speed };
    st.prev_pos = pos;
    let row = params.rows[st.row.min(3)];
    let rig = st.rig.get_or_insert_with(|| Rig::new(row, &subject, &cast));
    rig.row = row;
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
    let (right, right_mag) = stick(pressed(KeyCode::ArrowRight) - pressed(KeyCode::ArrowLeft), pressed(KeyCode::ArrowDown) - pressed(KeyCode::ArrowUp), 1.0);
    // Lock-on target: the locked enemy's position, a metre up (the game uses the object's own
    // target point, not decoded yet).
    let lock = s.lock.and_then(|l| targets.get(l).ok()).map(|g| g.translation() / SCALE + Vec3::Y * 100.0);
    rig.tick(
        params,
        &TickIn { sticks: Sticks { left, left_mag, right, right_mag }, reset: p.reset, subject, lock, zoom: p.zoom, auto: true, warp, cast: &cast },
    );

    if st.log {
        st.ticks += 1;
        if st.ticks.is_multiple_of(30) {
            info!(
                "camera: pos {:.0} look {:.0} yaw {:.2} radius {:.0} fov {:.2} sora {:.0} lock {:?} target {:?}",
                rig.pos, rig.look, rig.yaw, rig.radius, rig.fov, pos, s.lock, lock
            );
        }
    }
    o.yaw = rig.yaw;
    o.pitch = rig.pitch();
    o.distance = rig.radius * SCALE;
    st.last_yaw = o.yaw;
    st.last_dist = o.distance;

    let (pos_m, look_m) = (rig.pos * SCALE, rig.look * SCALE);
    if pos_m.is_finite() && look_m.is_finite() && pos_m.distance_squared(look_m) > 1e-8 {
        *t = Transform::from_translation(pos_m).looking_at(look_m, Vec3::Y);
    }
    // The game's field of view is horizontal for a 4:3 picture; Bevy wants the vertical one.
    if let Projection::Perspective(pp) = &mut *proj {
        pp.fov = 2.0 * (0.75 * (rig.fov * 0.5).tan()).atan();
    }
}
