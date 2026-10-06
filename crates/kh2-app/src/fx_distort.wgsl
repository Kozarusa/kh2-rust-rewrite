// Screen distortion for pppSnoMdlDttNorm. The game draws a model whose triangles show the frame
// buffer at the screen position of the vertex shifted along its normal. Here the displacement of
// each vertex (world space, metres) arrives in UV 0 (x, y) and UV 1 (x = z); the scene drawn so
// far is the view's transmission copy, which only the opaque/transmissive phase can read. So the
// blend is done here: the vertex colour tints the displaced sample and its alpha is the blend
// factor against what is already there (GS ALPHA 0x44: Cs * As + Cd * (1 - As), unclamped factor,
// result saturated).
//
// Depth bias: the game lowers its projection's z translation for the draw, which depth-tests every
// vertex as if it sat at a fixed factor of its distance from the camera (UV 1, y). The vertex stage
// takes the clip position from the view-space position scaled by that factor: the perspective
// divide leaves x and y where they are and only the depth moves.

#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput},
    mesh_view_bindings::{view, view_transmission_texture, view_transmission_sampler},
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    out.world_position = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    out.world_normal = vec3<f32>(0.0, 0.0, 1.0);
#ifdef VERTEX_UVS_B
    let depth_scale = vertex.uv_b.y;
#else
    let depth_scale = 1.0;
#endif
    let view_pos = view.view_from_world * out.world_position;
    out.position = view.clip_from_view * vec4<f32>(view_pos.xyz * depth_scale, 1.0);
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
    return out;
}

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
