// stars.wgsl — Iteration 0 point-star pass.
//
// Instanced camera-facing billboards, additive blending, soft-disc falloff.
// This is the SKELETON: the software rasteriser with perspective-correct
// solid angle (PLAN §7.3) and the c/θ² glare pyramid (§7.4) land in It.2.
//
// Positions are f32 parsecs in the Sun-centred It.0 shortcut frame; the view
// matrix is camera-relative (built in f64 on the CPU), so near-camera
// precision is preserved. No f64 in WGSL — ever (NON-NEGOTIABLE).

struct FrameUniforms {
    view_proj: mat4x4<f32>,
    cam_right: vec3<f32>,
    px_scale: f32,     // view-space size of 1 pixel at distance 1
    cam_up: vec3<f32>,
    time_s: f32,
    exposure: f32,
    m0: f32,          // limiting magnitude (per-star exposure gate, It.1)
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
    _pad4: f32,
    _pad5: f32,
    _pad6: f32,
};  // size = 128 B, multiple of the uniform address-space align (16)

struct Star {
    pos_pc: vec3<f32>,
    mag: f32,
    color: vec3<f32>,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> u: FrameUniforms;
@group(0) @binding(1) var<storage, read> stars: array<Star>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) bright: f32,
};

const CORNERS: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0),
    vec2(-1.0,  1.0), vec2(1.0, -1.0), vec2( 1.0, 1.0),
);

// Screen radius in pixels from G magnitude — skeleton tuning curve.
// Real photometry→pixel mapping is PLAN §7.1 (It.2).
fn star_radius_px(mag: f32) -> f32 {
    return clamp(pow(10.0, -0.20 * (mag - 5.5)) * 2.2, 0.8, 48.0);
}

// Per-star exposure gate: stars fainter than the limiting magnitude collapse
// to a zero-size, zero-brightness quad (node LOD alone cannot express m0).
fn gated(mag: f32) -> f32 {
    return select(0.0, 1.0, mag <= u.m0);
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VsOut {
    let s = stars[ii];
    let corner = CORNERS[vi];

    // Camera-relative depth via the view part of view_proj is not separable
    // here, so recompute distance in the Sun frame shifted by the camera:
    // view_proj already includes the camera-relative translation, and the
    // w component after projection gives view-space depth for a perspective
    // matrix. We use the projected w as distance.
    let center_clip = u.view_proj * vec4(s.pos_pc, 1.0);
    let dist = max(center_clip.w, 1e-4);

    let gate = gated(s.mag);
    let r_world = gate * dist * star_radius_px(s.mag) * u.px_scale;
    let world_offset = u.cam_right * (corner.x * r_world) + u.cam_up * (corner.y * r_world);

    var out: VsOut;
    out.clip = center_clip + u.view_proj * vec4(world_offset, 0.0);
    out.uv = corner;
    out.color = s.color;
    // Bright stars saturate toward white; faint ones stay dim. Flux-ish curve.
        // Flux-proportional brightness, NO floor: a floor saturates additive
    // blending into white fog once >10^5 stars overlap (session 002 finding).
    // Faint stars contribute ~their flux; clusters of them integrate into a
    // milky glow instead of clipping. HDR + tonemap (It.2) replaces this curve.
    out.bright = gate * clamp(pow(10.0, -0.4 * (s.mag - 6.0)), 0.0, 14.0) * u.exposure;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let d = length(in.uv);
    if d > 1.0 { discard; }
    let core = 1.0 - d * d;                 // soft disc
    let intensity = pow(core, 1.6);
    // Slight core whitening for bright stars (crude; It.2 does this properly).
    let white_mix = clamp((in.bright - 1.0) * 0.25, 0.0, 0.7);
    let col = mix(in.color, vec3(1.0), white_mix * intensity);
    return vec4(col * intensity * in.bright, 1.0);
}
