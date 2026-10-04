//! Disc access and conversion of decoded KH2 models into Bevy meshes, materials and images.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use kh2_archive::Archive;

use crate::collision::Collider;
use crate::sora::Sora;
use crate::SCALE;

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

#[derive(Resource)]
pub struct Disc(pub Mutex<Archive>);

impl Disc {
    pub fn open(iso: &Path) -> Disc {
        let mut a = Archive::open(iso).unwrap_or_else(|e| {
            eprintln!("{}: {e}", iso.display());
            std::process::exit(1);
        });
        // World sub-indexes (maps live there) are only listed once their names are known.
        let names = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/kh2idx.txt");
        let list = std::fs::read_to_string(&names).unwrap_or_default();
        let subs: Vec<&str> = list.lines().filter(|l| l.ends_with(".idx")).collect();
        if a.apply_names(if subs.is_empty() { vec!["000tt.idx"] } else { subs }).is_err() {
            eprintln!("warning: could not expand world indexes");
        }
        Disc(Mutex::new(a))
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
    if let Some(sky) = &map.sky {
        let s = commands.spawn((AreaPart, Sky, Transform::from_scale(Vec3::splat(SCALE)), Visibility::default())).id();
        spawn_model(&mut commands, s, sky, &mut meshes, &mut materials, &mut images);
    }
    let collider = Collider::from_meshes(&map.map.meshes, SCALE);
    if let Ok((mut t, mut p)) = player.single_mut() {
        t.translation = collider.spawn_point();
        p.vel = Vec3::ZERO;
    }
    commands.insert_resource(collider);
    info!("{file}: {} meshes", map.map.meshes.len());
}

pub fn sky_follow(cam: Query<&Transform, (With<Camera3d>, Without<Sky>)>, mut sky: Query<&mut Transform, With<Sky>>) {
    let Ok(c) = cam.single() else { return };
    for mut s in &mut sky {
        s.translation = c.translation;
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
    // One material per (texture, transparency) pair.
    let mut mats = std::collections::HashMap::new();
    for m in &model.meshes {
        let mat = mats.entry((m.texture, m.transparent)).or_insert_with(|| material_for(m, &textures, materials)).clone();
        let mesh = meshes.add(to_mesh(m));
        commands.spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::default(), ChildOf(parent)));
    }
}

pub fn texture_handles(model: &kh2_model::Model, images: &mut Assets<Image>) -> Vec<Handle<Image>> {
    model.textures.iter().map(|t| images.add(to_image(t))).collect()
}

/// PS2-style material: unlit, vertex colour Ã— texture, alpha-tested or blended.
pub fn material_for(m: &kh2_model::Mesh, textures: &[Handle<Image>], materials: &mut Assets<StandardMaterial>) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color_texture: textures.get(m.texture).cloned(),
        unlit: true,
        cull_mode: None,
        double_sided: true,
        alpha_mode: if m.transparent { AlphaMode::Blend } else { AlphaMode::Mask(0.25) },
        ..default()
    })
}

fn to_mesh(m: &kh2_model::Mesh) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(m.colors.clone()))
        .with_inserted_indices(Indices::U32(m.indices.clone()))
}

fn to_image(t: &kh2_model::Texture) -> Image {
    let mut img = Image::new(
        Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 },
        TextureDimension::D2,
        t.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let mode = |w: kh2_model::Wrap| match w {
        kh2_model::Wrap::Repeat => ImageAddressMode::Repeat,
        kh2_model::Wrap::Clamp => ImageAddressMode::ClampToEdge,
    };
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode(t.wrap_u),
        address_mode_v: mode(t.wrap_v),
        ..ImageSamplerDescriptor::linear()
    });
    img
}
