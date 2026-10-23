// The lens pass (fx_lens.rs). `copy_*` draws one screen-covering triangle that copies the bound texture;
// `lens_*` draws the lens triangles: the bound texture (the previous frame's picture) at the vertex's texture
// position, tinted by the vertex colour, its alpha the blend weight (the pipeline blends SrcAlpha /
// OneMinusSrcAlpha over the scene, GS ALPHA 0x44).

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct CopyOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn copy_vertex(@builtin(vertex_index) i: u32) -> CopyOut {
    let uv = vec2<f32>(f32(i >> 1u), f32(i & 1u)) * 2.0;
    return CopyOut(vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0), uv);
}

@fragment
fn copy_fragment(in: CopyOut) -> @location(0) vec4<f32> {
    return vec4<f32>(textureSampleLevel(source, source_sampler, in.uv, 0.0).rgb, 1.0);
}

struct LensIn {
    @location(0) clip: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) tint: vec4<f32>,
};

// The GS interpolates the texture position and the colour linearly on the screen (UV, Gouraud).
struct LensOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(linear) uv: vec2<f32>,
    @location(1) @interpolate(linear) tint: vec4<f32>,
};

@vertex
fn lens_vertex(in: LensIn) -> LensOut {
    return LensOut(in.clip, in.uv, in.tint);
}

@fragment
fn lens_fragment(in: LensOut) -> @location(0) vec4<f32> {
    let seen = textureSampleLevel(source, source_sampler, in.uv, 0.0).rgb * in.tint.rgb;
    return vec4<f32>(clamp(seen, vec3<f32>(0.0), vec3<f32>(1.0)), clamp(in.tint.a, 0.0, 1.0));
}
