//! The revive's screen fade: the fader object at 0x356b30 (`FUN_0014a978`) as the camera event of Tinker Bell's revive drives it
//! (event command 0x12, mode 2 -> `FUN_0014a7c8(object, 0x32, 1)` -> `FUN_0014a710`, then one `FUN_0014a818` a tick). Docs: docs/systems/drive-limits-summons/summons-objects-1.md,
//! "WP-RV5".
//!
//! The object's blend word (+0x90) is the GS register ALPHA_1 (the packet `FUN_0014b408` sends: the register id at +0x98 is 0x42, set by
//! `FUN_0014b000`). `FUN_0014a710` writes 0x48 for the colour white (flag 1) and 0x44 for black (flag 0). The register fields are A, B, C, D in
//! bits 0..8 (two bits each: A, B, D choose 0 source colour, 1 destination colour, 2 zero; C chooses 0 source alpha, 1 destination alpha,
//! 2 the fixed value), result `(A - B) * C / 128 + D`. So 0x44 (A 0, B 1, C 0, D 1) is `Cd + (Cs - Cd) * As`, the usual alpha blend, and 0x48
//! (A 0, B 2, C 0, D 1) is `Cs * As + Cd`: white is ADDED to the picture and clamps. The fade is drawn with `ReviveFadeMaterial`, which has the
//! additive blend of `hud_gfx::HudMaterial`.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::image::Image;
use bevy::mesh::{Mesh2d, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    TextureDimension, TextureFormat,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dKey, Material2dPlugin, MeshMaterial2d};
pub use crate::shared::summon::FULL;

/// `FUN_0014a710`'s blend word for the colour (`white` is the flag stored at +0xf4).
#[cfg(test)]
pub fn blend_word(white: bool) -> u64 {
    if white {
        0x48
    } else {
        0x44
    }
}

/// How an ALPHA_1 word blends: the two words the fader writes, `None` for any other.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    /// `Cd + (Cs - Cd) * As` (A 0, B 1, C 0, D 1).
    Alpha,
    /// `Cs * As + Cd` (A 0, B 2, C 0, D 1).
    Additive,
}

/// Decodes the A, B, C, D fields of an ALPHA_1 word.
#[cfg(test)]
pub fn decode_blend(word: u64) -> Option<Blend> {
    let (a, b, c, d) = (word & 3, word >> 2 & 3, word >> 4 & 3, word >> 6 & 3);
    match (a, b, c, d) {
        (0, 1, 0, 1) => Some(Blend::Alpha),
        (0, 2, 0, 1) => Some(Blend::Additive),
        _ => None,
    }
}

/// The quad's colour for an alpha byte: white, with the alpha as the blend factor (0x80 = 1.0).
pub fn color(alpha: u32) -> [f32; 4] {
    [1.0, 1.0, 1.0, alpha.min(FULL) as f32 / FULL as f32]
}

/// A white full-screen quad drawn with the additive blend (`Blend::Additive`).
#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct ReviveFadeMaterial {
    #[texture(0)]
    #[sampler(1)]
    pub texture: Handle<Image>,
}

impl Material2d for ReviveFadeMaterial {
    fn fragment_shader() -> ShaderRef {
        // The HUD's textured, vertex-coloured shader (embedded by `hud_gfx`).
        ShaderRef::Path(crate::hud_gfx::HUD_QUAD_SHADER.into())
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(target) = descriptor.fragment.as_mut().and_then(|f| f.targets.get_mut(0)).and_then(|t| t.as_mut()) {
            target.blend = Some(BlendState {
                color: BlendComponent { src_factor: BlendFactor::SrcAlpha, dst_factor: BlendFactor::One, operation: BlendOperation::Add },
                alpha: BlendComponent::OVER,
            });
        }
        Ok(())
    }
}

/// The fade quad's entity.
#[derive(Component)]
pub struct ReviveFadeQuad;

/// Registers the material (the shader `hud_quad.wgsl` is embedded by `hud_gfx`). The quad is spawned by `spawn` and painted by `paint`.
pub struct ReviveFadePlugin;

impl Plugin for ReviveFadePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<ReviveFadeMaterial>::default());
    }
}

/// Half the quad's side in HUD units: the HUD camera shows 512 x 448 and more on a wide window; this covers any window.
const HALF: f32 = 4000.0;

/// The quad: centred on the HUD camera (256, -224), on the keyhole wipe's layer (above the HUD and the menus; the original's task
/// priority 0x19640 runs after the HUD's 8000, docs/systems/drive-limits-summons/contract.md).
pub fn spawn(commands: &mut Commands, images: &mut Assets<Image>, meshes: &mut Assets<Mesh>, materials: &mut Assets<ReviveFadeMaterial>) {
    let white = images.add(Image::new_fill(
        Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        TextureDimension::D2,
        &[255, 255, 255, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    ));
    let pos = vec![[-HALF, -HALF, 0.0], [HALF, -HALF, 0.0], [HALF, HALF, 0.0], [-HALF, -HALF, 0.0], [HALF, HALF, 0.0], [-HALF, HALF, 0.0]];
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, VertexAttributeValues::Float32x3(pos))
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, VertexAttributeValues::Float32x2(vec![[0.5, 0.5]; 6]))
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(vec![color(0); 6]));
    commands.spawn((
        ReviveFadeQuad,
        Mesh2d(meshes.add(mesh)),
        MeshMaterial2d(materials.add(ReviveFadeMaterial { texture: white })),
        Transform::from_xyz(256.0, -224.0, 60.0),
        Visibility::Hidden,
        NoFrustumCulling,
        RenderLayers::layer(crate::hud_gfx::WIPE_LAYER),
    ));
}

/// Sets the quad's colour and visibility for the fade's alpha.
pub fn paint(alpha: u32, quad: &Mesh2d, vis: &mut Visibility, meshes: &mut Assets<Mesh>) {
    *vis = if alpha == 0 { Visibility::Hidden } else { Visibility::Inherited };
    if let Some(mut mesh) = meshes.get_mut(&quad.0) {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(vec![color(alpha); 6]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::summon::alpha_after;

    /// The two words `FUN_0014a710` writes decode to the two blends: 0x44 alpha, 0x48 additive; white takes the additive one.
    #[test]
    fn the_blend_words_decode_to_alpha_and_additive() {
        assert_eq!(decode_blend(blend_word(false)), Some(Blend::Alpha));
        assert_eq!(decode_blend(blend_word(true)), Some(Blend::Additive));
        assert_eq!(decode_blend(0x4c), None);
    }

    /// The alpha byte follows `0x80 - (int)(ratio * counter)` with the counter 100 for the command's length 0x32 (ratio 1.28), is 0 before
    /// the first run and 0x80 once the counter is spent.
    #[test]
    fn the_fade_alpha_follows_the_games_counter() {
        assert_eq!(alpha_after(0, 0x32), 0);
        assert_eq!(alpha_after(1, 0x32), 2); // 128 - (int)(1.28 * 99) = 128 - 126
        assert_eq!(alpha_after(50, 0x32), 64); // 128 - (int)(1.28 * 50)
        assert_eq!(alpha_after(99, 0x32), 127); // 128 - (int)(1.28 * 1)
        assert_eq!(alpha_after(100, 0x32), FULL);
        assert_eq!(alpha_after(500, 0x32), FULL);
        // Length 0 counts as 1 (counter 2, ratio 64): the first run gives 0x80 - 64.
        assert_eq!(alpha_after(1, 0), 64);
        let mut last = 0;
        for n in 0..=100 {
            let a = alpha_after(n, 0x32);
            assert!(a >= last, "alpha never falls: run {n}");
            last = a;
        }
    }

    /// The quad's colour is white and its alpha is the blend factor.
    #[test]
    fn the_quad_is_white_with_the_alpha_as_factor() {
        assert_eq!(color(0), [1.0, 1.0, 1.0, 0.0]);
        assert_eq!(color(0x40), [1.0, 1.0, 1.0, 0.5]);
        assert_eq!(color(0x200), [1.0, 1.0, 1.0, 1.0]);
    }

    /// `paint` hides the quad at alpha 0 and writes the colour into the mesh otherwise.
    #[test]
    fn paint_hides_the_clear_quad_and_colours_the_rest() {
        let mut meshes = Assets::<Mesh>::default();
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, VertexAttributeValues::Float32x3(vec![[0.0; 3]; 3]))
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, VertexAttributeValues::Float32x4(vec![color(0); 3]));
        let quad = Mesh2d(meshes.add(mesh));
        let mut vis = Visibility::Hidden;
        paint(0x40, &quad, &mut vis, &mut meshes);
        assert_eq!(vis, Visibility::Inherited);
        let Some(VertexAttributeValues::Float32x4(c)) = meshes.get(&quad.0).unwrap().attribute(Mesh::ATTRIBUTE_COLOR) else { panic!("no colour") };
        assert_eq!(c[0], [1.0, 1.0, 1.0, 0.5]);
        paint(0, &quad, &mut vis, &mut meshes);
        assert_eq!(vis, Visibility::Hidden);
    }
}
