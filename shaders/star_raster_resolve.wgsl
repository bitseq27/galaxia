// shaders/star_raster_resolve.wgsl — s009: resolve pass of the tile
// rasteriser: Q10 fixed-point accumulator -> HDR radiance (background
// added here). Separate module from star_raster.wgsl (wgpu30-GLES bind
// hygiene; see BUILDLOG s009).
// Scalars: [0..3) bg radiance, [3] inv_q, [4] width, ...

@group(0) @binding(0) var<storage, read> accum_ro: array<u32>;
@group(0) @binding(1) var<storage, read> ru: array<f32>;

@vertex
fn vs_resolve(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    return vec4<f32>(p[vi], 0.0, 1.0);
}

@fragment
fn fs_resolve(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let x = u32(pos.x);
    let y = u32(pos.y);
    let base = (y * u32(ru[4]) + x) * 4u;
    let acc = vec3<f32>(
        f32(accum_ro[base + 0u]),
        f32(accum_ro[base + 1u]),
        f32(accum_ro[base + 2u])
    );
    return vec4<f32>(vec3<f32>(ru[0], ru[1], ru[2]) + acc * ru[3], 1.0);
}
