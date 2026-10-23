// Textured, vertex-coloured triangles for the HUD. The game's GS modulates texture by vertex
// colour (0x80 = 1.0) and clamps; the CPU side already scaled the colours and converted them to
// linear light, so this is a plain multiply.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

#ifdef SRGB_OUTPUT
#import bevy_render::color_operations::linear_to_srgb
#endif

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var hud_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var hud_sampler: sampler;

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    var c = textureSample(hud_texture, hud_sampler, mesh.uv) * mesh.color;
    c = clamp(c, vec4<f32>(0.0), vec4<f32>(1.0));
#ifdef SRGB_OUTPUT
    c = vec4<f32>(linear_to_srgb(c.rgb), c.a);
#endif
    return c;
}
