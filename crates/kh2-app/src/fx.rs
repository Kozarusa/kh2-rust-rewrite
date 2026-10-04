//! Effects: PAX elements run by the `kh2-fx` particle program runtime, one tick per 60 Hz frame,
//! drawn as dynamic meshes.
//!
//! Effects run in "fx space": Bevy world axes in game units (centimetres), so the programs'
//! integer rounding and lengths match the game's.

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use kh2_fx::pax::element_flag;
use kh2_fx::{Dpd, Draw, Element, Env, Pax};

use crate::SCALE;
use crate::assets::Disc;
use crate::skel::Animator;

/// A PAX with its DPDs shared and its textures uploaded.
pub struct LoadedPax {
    pub pax: Pax,
    pub dpds: Vec<Arc<Dpd>>,
    /// Per DPD, per texture.
    pub textures: Vec<Vec<Handle<Image>>>,
}

#[derive(Resource)]
pub struct EffectLib {
    pub keyblade: Arc<LoadedPax>,
}

/// Range trigger: weapon swing, show the trail (keyblade motion set).
pub const TRIGGER_TRAIL: u8 = 12;
/// Game effect id of the weapon trail.
pub const EFFECT_TRAIL: u16 = 10;

#[derive(Component)]
pub struct RunningEffect {
    pub lib: Arc<LoadedPax>,
    pub element: Element,
    pub effect: kh2_fx::Effect,
    /// Entity with the `Animator` the effect is bound to.
    pub owner: Entity,
    pub mesh: Handle<Mesh>,
    pub game_id: u16,
}

pub fn load(disc: &Disc, images: &mut Assets<Image>, name: &str) -> Option<LoadedPax> {
    let file = disc.read(name)?;
    let entries = kh2_bar::parse(&file).ok()?;
    let pax = Pax::parse(entries.iter().find(|e| e.kind == 18)?.data).ok()?;
    let textures = pax.dpds.iter().map(|d| d.textures.iter().map(|t| images.add(to_image(t))).collect()).collect();
    let dpds = pax.dpds.iter().cloned().map(Arc::new).collect();
    Some(LoadedPax { pax, dpds, textures })
}

fn to_image(t: &kh2_fx::pax::Texture) -> Image {
    let mut img = Image::new(
        Extent3d { width: t.width as u32, height: t.height as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        t.rgba.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    img
}

pub fn setup(mut commands: Commands, disc: Res<Disc>, mut images: ResMut<Assets<Image>>) {
    let keyblade = load(&disc, &mut images, "obj/W_EX010.apdx").expect("obj/W_EX010.apdx: no effects");
    commands.insert_resource(EffectLib { keyblade: Arc::new(keyblade) });
}

/// Starts every element of `lib` registered for game effect `id`.
#[allow(clippy::too_many_arguments)]
pub fn start(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    lib: &Arc<LoadedPax>,
    id: u16,
    owner: Entity,
) {
    for el in lib.pax.elements_for(id) {
        let Some(fx) = lib.pax.effects.get(el.effect as usize) else { continue };
        let Some(effect) = kh2_fx::Effect::new(lib.dpds[fx.dpd].clone(), fx.dpd, fx.particle_data) else { continue };
        let mesh = meshes.add(empty_mesh());
        let texture = lib.textures[fx.dpd].first().cloned();
        let material = materials.add(StandardMaterial {
            base_color_texture: texture,
            unlit: true,
            cull_mode: None,
            double_sided: true,
            alpha_mode: AlphaMode::Add,
            ..default()
        });
        commands.spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::default(),
            Visibility::default(),
            bevy::light::NotShadowCaster,
            RunningEffect { lib: lib.clone(), element: el.clone(), effect, owner, mesh, game_id: id },
        ));
    }
}

/// Weapon trails: run effect 10 while the keyblade's motion has its trail range trigger.
pub fn weapon_trails(
    mut commands: Commands,
    lib: Res<EffectLib>,
    sora: Query<&crate::sora::Sora>,
    anims: Query<&Animator>,
    mut running: Query<&mut RunningEffect>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Ok(s) = sora.single() else { return };
    let Ok(weapon) = anims.get(s.weapon) else { return };
    let on = weapon.trigger(TRIGGER_TRAIL);
    let mut have = false;
    for mut r in running.iter_mut().filter(|r| r.owner == s.weapon && r.game_id == EFFECT_TRAIL) {
        if !on {
            r.effect.end();
        } else if !r.effect.is_ending() {
            have = true;
        }
    }
    if on && !have {
        start(&mut commands, &mut meshes, &mut materials, &lib.keyblade, EFFECT_TRAIL, s.weapon);
    }
}

/// One effect tick for every running effect, after the poses of this tick are known.
pub fn tick(
    mut commands: Commands,
    mut effects: Query<(Entity, &mut RunningEffect)>,
    owners: Query<&Animator>,
    locals: Query<(&Transform, Option<&ChildOf>)>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let view_inv = camera.single().map(|c| fx_space(c.to_matrix())).unwrap_or(Mat4::IDENTITY);
    for (e, mut r) in &mut effects {
        let Ok(anim) = owners.get(r.owner) else {
            commands.entity(e).despawn();
            continue;
        };
        let el = r.element.clone();
        let body = GlobalTransform::from(world_now(r.owner, &locals));
        let body = &body;
        let bone = anim.bone_world(body, el.bone.max(0) as usize);
        let bind = fx_space(bone);
        let mut env = Env { bind, view: view_inv.inverse(), view_inv, ..Default::default() };
        env.bind_scale = Vec3::new(bind.x_axis.truncate().length(), bind.y_axis.truncate().length(), bind.z_axis.truncate().length());
        if el.flags & element_flag::TWO_POINT != 0 {
            let p = |b: u16, ratio: f32| {
                let len = anim.rig.model.bones.get(b as usize).map_or(0.0, |x| x.length);
                fx_space(anim.bone_world(body, b as usize)).transform_point3(Vec3::new(ratio * len, 0.0, 0.0))
            };
            env.two_point = Some([p(el.bone_pos.a, el.bone_pos.ratio_a), p(el.bone_pos.b, el.bone_pos.ratio_b)]);
        }
        let draws = r.effect.tick(&env);
        if std::env::var_os("KH2_FXLOG").is_some() {
            let base = env.two_point.map_or(Vec3::ZERO, |p| p[0]);
            let far = draws.iter().flat_map(|Draw::Strip(s)| s.quads.iter().flat_map(|q| q.iter().filter(|c| c.1.w > 0.0).map(|c| (c.0 - base).length()))).fold(0.0f32, f32::max);
            let vis = draws.iter().map(|Draw::Strip(s)| s.quads.iter().filter(|q| q[0].1.w > 0.0).count()).sum::<usize>();
            info!("fx {} objs {} visible quads {vis} max dist {far:.1} tp {:?}", r.game_id, r.effect.alive_objects(), env.two_point.map(|p| (p[1] - p[0]).length()));
        }
        if r.effect.done {
            commands.entity(e).despawn();
            continue;
        }
        if let Some(mut m) = meshes.get_mut(&r.mesh) {
            *m = build_mesh(&draws);
        }
    }
}

/// World matrix from this tick's local transforms (GlobalTransform is only propagated after
/// the fixed-rate systems, one frame late for anything that moved this tick).
pub fn world_now(e: Entity, locals: &Query<(&Transform, Option<&ChildOf>)>) -> Mat4 {
    let mut m = Mat4::IDENTITY;
    let mut cur = Some(e);
    while let Some(x) = cur {
        let Ok((t, parent)) = locals.get(x) else { break };
        m = t.to_matrix() * m;
        cur = parent.map(|p| p.parent());
    }
    m
}

/// Bevy world (metres) → fx space (centimetres).
fn fx_space(m: Mat4) -> Mat4 {
    Mat4::from_scale(Vec3::splat(1.0 / SCALE)) * m
}

fn empty_mesh() -> Mesh {
    build_mesh(&[])
}

fn build_mesh(draws: &[Draw]) -> Mesh {
    let (mut pos, mut col, mut uv, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for d in draws {
        match d {
            Draw::Strip(s) => {
                for q in &s.quads {
                    let b = pos.len() as u32;
                    for (p, c, t) in q {
                        pos.push((*p * SCALE).to_array());
                        col.push(c.to_array());
                        uv.push(*t);
                    }
                    // GS triangle strip over the quad's corners: (0,1,2), (1,3,2).
                    idx.extend_from_slice(&[b, b + 1, b + 2, b + 1, b + 3, b + 2]);
                }
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(col))
        .with_inserted_indices(Indices::U32(idx))
}
