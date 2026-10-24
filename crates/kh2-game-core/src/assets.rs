//! Disc access and conversion of decoded KH2 models into Bevy meshes, materials and images.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use kh2_archive::Archive;

use crate::data::GameData;
use crate::schedule::Frame;
use crate::world::World;
use crate::shared::sora::Sora;
use crate::SCALE;

/// The disc, the game's tables read from it, and the area that is loaded and switched at run time.
/// Add this before any plugin that needs `Disc` or `GameData`.
pub struct AssetsPlugin {
    pub iso: PathBuf,
    /// Area loaded at startup, e.g. `tt04`.
    pub start: String,
}

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        let disc = Disc::open(&self.iso);
        crate::startup_log("disc");
        let game_data = GameData::load(&disc);
        crate::startup_log("tables");
        app.insert_resource(disc)
            .insert_resource(game_data)
            .insert_resource(CurrentArea { name: self.start.clone(), loaded: None })
            .add_systems(Update, ((switch_area, load_area).chain(), (sky_follow, sky_unculled).in_set(Frame::Sky)));
    }
}

/// ISO path: $KH2_ISO, else `local/iso.txt` written by `kh2 set-iso`.
pub fn iso_path() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("KH2_ISO") {
        return Ok(p.into());
    }
    let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
    std::fs::read_to_string(&f)
        .map(|s| PathBuf::from(s.trim()))
        .map_err(|_| "No disc configured. Run: cargo run -p kh2 -- set-iso \"<your Kingdom Hearts II (USA).iso>\"".into())
}

/// The file-name list (`kh2idx.txt`, written by `kh2 fetch-names`) that lets the world sub-indexes expand. Looked up next
/// to the executable, then in the config folder (`KH2_CONFIG_DIR`, else `%APPDATA%\kh2-rs`), then in `local/` of the
/// source tree this was built from.
pub fn names_file() -> Option<PathBuf> {
    let mut places = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        places.push(dir);
    }
    if let Some(d) = std::env::var_os("KH2_CONFIG_DIR").filter(|d| !d.is_empty()) {
        places.push(d.into());
    } else if let Some(d) = std::env::var_os("APPDATA").filter(|d| !d.is_empty()) {
        places.push(PathBuf::from(d).join("kh2-rs"));
    }
    places.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local"));
    places.into_iter().map(|d| d.join("kh2idx.txt")).find(|f| f.is_file())
}

#[derive(Resource)]
pub struct Disc(pub Mutex<Archive>);

impl Disc {
    pub fn open(iso: &Path) -> Disc {
        let mut a = Archive::open(iso).unwrap_or_else(|e| {
            eprintln!("{}: {e}", iso.display());
            std::process::exit(1);
        });
        // World sub-indexes (maps live there) are only listed once their names are known.
        let list = names_file().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default();
        let subs: Vec<&str> = list.lines().filter(|l| l.ends_with(".idx")).collect();
        if a.apply_names(if subs.is_empty() { vec!["000tt.idx"] } else { subs }).is_err() {
            eprintln!("warning: could not expand world indexes");
        }
        Disc(Mutex::new(a))
    }

    /// A file in the disc's root directory (executables, IOP modules).
    pub fn read_root(&self, name: &str) -> Option<Vec<u8>> {
        self.0.lock().ok()?.iso().read(name).map_err(|e| warn!("{name}: {e}")).ok()
    }

    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        self.0.lock().ok()?.read(name).map_err(|e| warn!("{name}: {e}")).ok()
    }
}

#[derive(Resource)]
pub struct CurrentArea {
    pub name: String,
    pub loaded: Option<String>,
}

/// Everything spawned for the current area (despawned on switch).
#[derive(Component)]
pub struct AreaPart;

#[derive(Component)]
pub struct Sky;

const AREAS: [&str; 10] = ["tt00", "tt01", "tt02", "tt03", "tt04", "tt05", "tt06", "tt07", "tt08", "tt09"];

pub fn switch_area(keys: Res<ButtonInput<KeyCode>>, mut area: ResMut<CurrentArea>) {
    let digits = [
        KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4,
        KeyCode::Digit5, KeyCode::Digit6, KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9,
    ];
    for (i, k) in digits.iter().enumerate() {
        if keys.just_pressed(*k) {
            area.name = AREAS[i].to_string();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn load_area(
    mut commands: Commands,
    disc: Res<Disc>,
    mut area: ResMut<CurrentArea>,
    old: Query<Entity, With<AreaPart>>,
    mut player: Query<(&mut Transform, &mut Sora)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    if area.loaded.as_deref() == Some(area.name.as_str()) {
        return;
    }
    area.loaded = Some(area.name.clone());
    let file = format!("map/us/{}.map", area.name);
    let Some(map) = disc.read(&file).and_then(|d| kh2_model::map_file(&d)) else {
        warn!("{file}: not found or not a map");
        return;
    };
    for e in &old {
        commands.entity(e).despawn();
    }
    let root = commands.spawn((AreaPart, Transform::from_scale(Vec3::splat(SCALE)), Visibility::default())).id();
    spawn_model(&mut commands, root, &map.map, &mut meshes, &mut materials, &mut images);
    // SK0 then SK1: the game builds one object per tag, in that order (`FUN_0015e6c0`).
    for sky in [&map.sky, &map.sky1].into_iter().flatten() {
        let s = commands.spawn((AreaPart, Sky, Transform::from_scale(Vec3::splat(SCALE)), Visibility::default())).id();
        spawn_model(&mut commands, s, sky, &mut meshes, &mut materials, &mut images);
    }
    let world = disc
        .read(&file)
        .and_then(|d| kh2_map::Coct::from_map(&d))
        .filter(|c| !c.polygons.is_empty())
        .map(|c| World::from_coct(&c))
        .unwrap_or_else(|| World::from_meshes(&map.map.meshes));
    let cam_world = World::from_meshes(&map.map.meshes);
    if let Ok((mut t, mut p)) = player.single_mut() {
        let (_, at) = spawn_point(&world, &cam_world);
        crate::shared::sora::respawn(&mut p, at);
        t.translation = at * SCALE;
    }
    info!("{file}: {} meshes, collision {} triangles ({})", map.map.meshes.len(), world.tris.len(), if world.coct { "COCT" } else { "render mesh" });
    commands.insert_resource(world);
    commands.insert_resource(crate::world::CameraWorld(cam_world));
}

/// Where Sora stands when an area loads: the main floor of the render mesh (`cam_world`, the height band
/// with the most floor area, at its point nearest the centre), put down on the collision floor below it.
/// When no collision floor is within 400 units under that point, the render mesh's main floor is scenery
/// outside the walkable collision (tt01, tt03, tt06 and tt07: Sora fell through it, WP-F1 item 15), and
/// the collision world's own main floor band is taken instead. Returns (the render floor point, the spawn
/// point). Stand-in until spawn points come from the area's event data (`world::World::spawn_point`).
pub fn spawn_point(world: &World, cam_world: &World) -> (Vec3, Vec3) {
    let near = cam_world.spawn_point_band();
    match world.ground(near, near.y + 100.0, 400.0) {
        Some(g) => (near, Vec3::new(near.x, g.y, near.z)),
        None => (near, world.spawn_point_band()),
    }
}

/// The sky stays where the game puts it. The area's sky objects are made by `FUN_0015e6c0` →
/// `FUN_0015f3a0` with position (0, 0, 0) and no rotation, and nothing in their per-frame update
/// (`FUN_0015fef8`: the model's texture players only) moves them with the camera. The sky model's
/// own ground meshes lie at world height -200 and are anchored in the world (tt05's covers
/// x -60935..65904, z -43041..48327; its green backdrop was drawn 2 m under the camera by the old
/// camera-following, which hid the floor); only the dome meshes are millions of units wide.
/// The game orders the sky before the map (`FUN_00148440` marks it, `FUN_00147300` draws it
/// first); the opaque world meshes still overdraw it here through the depth test. `[inferred]`:
/// that no other code moves the sky object (its per-frame matrix, vtable 0x35a820, was not traced
/// further).
pub fn sky_follow(_cam: Query<&Transform, (With<Camera3d>, Without<Sky>)>, mut sky: Query<&mut Transform, With<Sky>>) {
    for mut s in &mut sky {
        s.translation = Vec3::ZERO;
    }
}

/// The dome meshes lie millions of units out (tens of kilometres at `SCALE`), beyond the camera's
/// far plane (Bevy's perspective projection is infinite in depth; its `far` only feeds frustum
/// culling), so every sky mesh is exempt from culling. The PS2 has no far clip (`[inferred]`: the
/// game's projection `FUN_001078f0` sets only the near plane; no far-plane test was found).
pub fn sky_unculled(mut commands: Commands, new: Query<(Entity, &ChildOf), Added<Mesh3d>>, sky: Query<(), With<Sky>>) {
    for (e, parent) in &new {
        if sky.contains(parent.parent()) {
            commands.entity(e).insert(bevy::camera::visibility::NoFrustumCulling);
        }
    }
}

/// Spawns every mesh of a model as a child of `parent`.
pub fn spawn_model(
    commands: &mut Commands,
    parent: Entity,
    model: &kh2_model::Model,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
) {
    let textures = texture_handles(model, images);
    // One material per (texture, transparency, UV scroll) triple.
    let mut mats = std::collections::HashMap::new();
    let mut used = Vec::new();
    for m in &model.meshes {
        let mat = mats
            .entry((m.texture, m.transparent, m.uv_scroll))
            .or_insert_with(|| {
                let h = material_for(m, model, &textures, materials);
                used.push((m.texture, m.uv_scroll, h.clone()));
                h
            })
            .clone();
        let mesh = meshes.add(to_mesh(m));
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), ChildOf(parent)));
    }
    crate::texanim::log_model(model);
    if let Some(anim) = crate::texanim::TexAnim::new(model, &textures, &used) {
        commands.entity(parent).insert(anim);
    }
}

pub fn texture_handles(model: &kh2_model::Model, images: &mut Assets<Image>) -> Vec<Handle<Image>> {
    model.textures.iter().map(|t| images.add(to_image(t))).collect()
}

/// PS2-style material: unlit, vertex colour × texture, alpha-tested or blended. The UV transform
/// maps the mesh's texture coordinates onto the image cropped to the GS region (`to_image`).
pub fn material_for(
    m: &kh2_model::Mesh,
    model: &kh2_model::Model,
    textures: &[Handle<Image>],
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    let tint = crate::texanim::tinted(m.texture);
    materials.add(StandardMaterial {
        base_color: if tint { Color::srgb(1.0, 0.0, 1.0) } else { Color::WHITE },
        base_color_texture: if tint { None } else { textures.get(m.texture).cloned() },
        uv_transform: model.textures.get(m.texture).map_or(bevy::math::Affine2::IDENTITY, |t| region_affine(t, [0.0, 0.0])),
        unlit: true,
        cull_mode: None,
        double_sided: true,
        alpha_mode: if m.transparent { AlphaMode::Blend } else { AlphaMode::Mask(0.25) },
        ..default()
    })
}

/// UV transform for a texture drawn from its GS region, with a UV-scroll offset `off` (fractions
/// of the whole texture, added to the coordinates before the GS wraps them):
/// `u' = (u × W + off_u × W − start_u) / len_u`, likewise for V.
pub fn region_affine(t: &kh2_model::Texture, off: [f32; 2]) -> bevy::math::Affine2 {
    let [ru, rv] = t.region;
    let (w, h) = (t.width as f32, t.height as f32);
    let (lu, lv) = (ru.len.max(1) as f32, rv.len.max(1) as f32);
    bevy::math::Affine2::from_scale_angle_translation(
        Vec2::new(w / lu, h / lv),
        0.0,
        Vec2::new((off[0] * w - ru.start as f32) / lu, (off[1] * h - rv.start as f32) / lv),
    )
}

/// The texture's pixels inside its GS region (`Texture::region`), row-major RGBA8.
pub fn crop(t: &kh2_model::Texture) -> Vec<u8> {
    let [ru, rv] = t.region;
    let mut out = Vec::with_capacity((ru.len * rv.len * 4) as usize);
    for y in rv.start..rv.start + rv.len {
        let o = ((y * t.width + ru.start) * 4) as usize;
        out.extend_from_slice(t.rgba.get(o..o + ru.len as usize * 4).unwrap_or(&vec![0; ru.len as usize * 4]));
    }
    out
}

fn to_mesh(m: &kh2_model::Mesh) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(m.colors.clone()))
        .with_inserted_indices(Indices::U32(m.indices.clone()))
}

/// The texture as the GS samples it: cropped to its region, wrapped per the region's mode.
fn to_image(t: &kh2_model::Texture) -> Image {
    let [ru, rv] = t.region;
    let mut img = Image::new(
        Extent3d { width: ru.len, height: rv.len, depth_or_array_layers: 1 },
        TextureDimension::D2,
        crop(t),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let mode = |w: kh2_model::Wrap| match w {
        kh2_model::Wrap::Repeat => ImageAddressMode::Repeat,
        kh2_model::Wrap::Clamp => ImageAddressMode::ClampToEdge,
    };
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode(ru.wrap),
        address_mode_v: mode(rv.wrap),
        ..ImageSamplerDescriptor::linear()
    });
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WP-F1 item 15 (Sora fell through the floor after an area jump): for every Twilight Town area, the
    /// spawn point `load_area` would choose and whether the collision world has a floor under it.
    #[test]
    fn every_twilight_town_area_has_collision_ground_under_its_spawn_point() {
        let Some(iso) = iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let mut bad = Vec::new();
        for area in AREAS {
            let file = format!("map/us/{area}.map");
            let Some(data) = disc.read(&file) else {
                eprintln!("{area}: no map file");
                continue;
            };
            let Some(map) = kh2_model::map_file(&data) else {
                eprintln!("{area}: not a map");
                continue;
            };
            // Every collision entry of the file, so a wrong pick shows (`Coct::from_map` takes the first type 6).
            for (i, e) in kh2_bar::parse(&data).into_iter().flatten().enumerate() {
                if e.kind == 6 || e.kind == 11 {
                    let c = kh2_map::Coct::parse(e.data);
                    let n = c.as_ref().map_or(0, |c| c.polygons.len());
                    let (lo, hi) = c.as_ref().map_or((Vec3::ZERO, Vec3::ZERO), |c| {
                        c.polygons.iter().flat_map(|p| p.triangles()).flatten().map(Vec3::from).fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| (lo.min(v), hi.max(v)))
                    });
                    eprintln!("{area}: entry {i} kind {} tag {:?} {} bytes: {n} polygons, bounds ({:.0} {:.0} {:.0})..({:.0} {:.0} {:.0})", e.kind, e.tag, e.data.len(), lo.x, lo.y, lo.z, hi.x, hi.y, hi.z);
                }
            }
            let world = kh2_map::Coct::from_map(&data)
                .filter(|c| !c.polygons.is_empty())
                .map(|c| World::from_coct(&c))
                .unwrap_or_else(|| World::from_meshes(&map.map.meshes));
            let cam_world = World::from_meshes(&map.map.meshes);
            let (near, at) = spawn_point(&world, &cam_world);
            let floor = world.ground(at, at.y + 10.0, 20.0);
            // Before WP-F1 item 15 the spawn was the render floor point put down at most 400 units.
            let before = world.ground(near, near.y + 100.0, 400.0);
            eprintln!(
                "{area}: render floor band on {} collision floor within 400 below it (the spawn before WP-F1)",
                if before.is_some() { "a" } else { "NO" }
            );
            eprintln!(
                "{area}: {} ({} tris), render floor band ({:.0} {:.0} {:.0}), spawn ({:.0} {:.0} {:.0}), collision floor under it: {}",
                if world.coct { "COCT" } else { "render mesh" },
                world.tris.len(),
                near.x,
                near.y,
                near.z,
                at.x,
                at.y,
                at.z,
                floor.map_or("NONE".to_string(), |g| format!("y {:.0} flags {:#x}", g.y, g.flags))
            );
            if floor.is_none() {
                bad.push(area);
            }
        }
        assert!(bad.is_empty(), "areas whose spawn point stands on nothing: {bad:?}");
    }
}
