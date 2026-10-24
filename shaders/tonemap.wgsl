// shaders/tonemap.wgsl — s008 HDR -> LDR display pass (ADR-0002 prerequisite).
// Fullscreen triangle; texel-to-pixel load of the HDR radiance target;
// exposure multiply; clamp; sRGB OETF. Mirror of galaxia-render tonemap.rs
// (srgb_oetf / tonemap_channel unit tests pin the constants).

struct Uniforms {
    exposure: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var hdr: texture_2d<f32>;
@group(0) @binding(1) var<uniform> u: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // one oversized triangle, no vertex buffer
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    return vec4<f32>(p[vi], 0.0, 1.0);
}

fn srgb_oetf(c: vec3<f32>) -> vec3<f32> {
    return select(12.92 * c, 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055,
                  c > vec3<f32>(0.0031308));
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let radiance = textureLoad(hdr, vec2<i32>(i32(pos.x), i32(pos.y)), 0).rgb;
    let display = clamp(radiance * u.exposure, vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(srgb_oetf(display), 1.0);
}
