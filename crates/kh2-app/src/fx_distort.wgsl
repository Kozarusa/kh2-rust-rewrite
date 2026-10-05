// Screen distortion for pppSnoMdlDttNorm. The game draws a model whose triangles show the frame
// buffer at the screen position of the vertex shifted along its normal. Here the displacement of
// each vertex (world space, metres) arrives in UV 0 (x, y) and UV 1 (x = z); the scene drawn so
// far is the view's transmission copy, which only the opaque/transmissive phase can read. So the
// blend is done here: the vertex colour tints the displaced sample and its alpha is the blend
// factor against what is already there (GS ALPHA 0x44: Cs * As + Cd * (1 - As), unclamped factor,
// result saturated).

#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{view, view_transmission_texture, view_transmission_sampler}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
#ifdef VERTEX_UVS_B
    let offset = vec3<f32>(in.uv, in.uv_b.x);
#else
    let offset = vec3<f32>(0.0);
#endif
#ifdef VERTEX_COLORS
    let tint = in.color;
#else
    let tint = vec4<f32>(1.0);
#endif
    let clip = view.clip_from_world * vec4<f32>(in.world_position.xyz + offset, 1.0);
    let ndc = clip.xy / clip.w;
    let shifted = vec2<f32>(0.5 + 0.5 * ndc.x, 0.5 - 0.5 * ndc.y);
    let here = in.position.xy / view.viewport.zw;
    let behind = textureSampleLevel(view_transmission_texture, view_transmission_sampler, here, 0.0).rgb;
    let seen = textureSampleLevel(view_transmission_texture, view_transmission_sampler, shifted, 0.0).rgb * tint.rgb;
    return vec4<f32>(clamp(behind + (seen - behind) * tint.a, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
