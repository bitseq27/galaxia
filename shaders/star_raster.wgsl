// shaders/star_raster.wgsl — s009 PLAN §7.3 tile rasteriser (ADR-0002),
// u32 Q10 fixed-point accumulation (ADR-0007, amended: plain per-pixel
// stores — each pixel is owned by exactly one thread, so no atomics needed).
//
// One workgroup per 8x8 screen tile, one thread per pixel. The thread loops
// the tile's star list (CPU-binned, star_raster.rs) and deposits
//   radiance = l · k(θ) · profile(ρ/r) · coverage(pixel ∩ disc) · colour
// with:
//   profile  = (1-(ρ/r)²)^1.6            (same soft disc as stars.wgsl)
//   coverage = exact circle/unit-square overlap area (mirror of Rust
//              circle_rect_area; flux-conserving by construction)
//   k(θ)     = ((f²+ρ_s²)/f²)^{3/2}      (§7.3 [MUST] perspective-correct
//              solid angle: off-axis pixels subtend cos³θ less solid angle,
//              so radiance grows by 1/cos³ to conserve flux across the FOV)
// Scalars ride in a read-only storage array<f32> (NOT a uniform buffer:
// wgpu30-GLES compute reads UBOs as zero — probe evidence, BUILDLOG s009).
//   [0..3) background radiance, [3] inv_q, [4] width, [5] height,
//   [6] f_px, [7] tiles_x

struct RasterStar {
    cx: f32,
    cy: f32,
    r_px: f32,
    l: f32,
    r: f32,
    g: f32,
    b: f32,
    depth: f32,
};  // 32 B scalars — matches Rust RasterStar stride exactly

@group(0) @binding(0) var<storage, read> u: array<f32>;
@group(0) @binding(1) var<storage, read> stars: array<RasterStar>;
@group(0) @binding(2) var<storage, read> tile_off: array<u32>;
@group(0) @binding(3) var<storage, read> tile_ids: array<u32>;
@group(0) @binding(4) var<storage, read_write> accum: array<u32>;

const TILE: u32 = 8u;

// ── exact circle/unit-rect overlap, mirror of star_raster.rs ──────────────
fn quadrant_area(r: f32, x_in: f32, y_in: f32) -> f32 {
    let x = min(x_in, r);
    let y = min(y_in, r);
    let r2 = r * r;
    let xc = sqrt(max(r2 - y * y, 0.0));
    if (x <= xc) {
        return x * y;
    }
    let rect = xc * y;
    let hi = x;
    let integral = 0.5 * (hi * sqrt(max(r2 - hi * hi, 0.0)) + r2 * asin(hi / r))
        - 0.5 * (xc * sqrt(max(r2 - xc * xc, 0.0)) + r2 * asin(xc / r));
    return rect + integral;
}

fn corner_area(r: f32, x: f32, y: f32) -> f32 {
    if (r <= 0.0) {
        return 0.0;
    }
    let sx = select(-1.0, 1.0, x >= 0.0);
    let sy = select(-1.0, 1.0, y >= 0.0);
    return sx * sy * quadrant_area(r, abs(x), abs(y));
}

fn circle_rect_area(r: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> f32 {
    return corner_area(r, x1, y1) - corner_area(r, x0, y1)
        - corner_area(r, x1, y0) + corner_area(r, x0, y0);
}

@compute @workgroup_size(8, 8)
fn cs_tile(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) li: vec3<u32>) {
    let tile = wg.x;
    let px = (tile % u32(u[7])) * TILE + li.x;
    let py = (tile / u32(u[7])) * TILE + li.y;
    let a = tile_off[tile];
    let b = tile_off[tile + 1u];
    let fx = f32(px) + 0.5;
    let fy = f32(py) + 0.5;
    var dep_sum = vec3<f32>(0.0, 0.0, 0.0);
    for (var i = a; i < b; i = i + 1u) {
        let s = stars[tile_ids[i]];
        let dx = fx - s.cx;
        let dy = fy - s.cy;
        let rho2 = dx * dx + dy * dy;
        let rr = s.r_px + 1.5;
        if (rho2 < rr * rr) {
            // 2x2 sub-pixel quadrature of profile over pixel∩disc:
            // deposit = sum_i prof(rho_i) * area(subpixel_i ∩ disc).
            // Single-sample prof(center)*area mis-integrates the steep rim
            // and re-introduces shimmer (s009 measurement: 1.06% vs 0.65%).
            var integ = 0.0;
            for (var oy = 0; oy < 2; oy = oy + 1) {
                for (var ox = 0; ox < 2; ox = ox + 1) {
                    let sxo = dx + f32(ox) * 0.5 - 0.25;
                    let syo = dy + f32(oy) * 0.5 - 0.25;
                    let ar = circle_rect_area(s.r_px, sxo - 0.25, sxo + 0.25,
                                              syo - 0.25, syo + 0.25);
                    if (ar > 0.0) {
                        let r2s = sxo * sxo + syo * syo;
                        let qs = min(r2s / (s.r_px * s.r_px), 1.0);
                        integ = integ + pow(max(1.0 - qs, 0.0), 1.6) * ar;
                    }
                }
            }
            let cov = clamp(
                circle_rect_area(s.r_px, dx - 0.5, dx + 0.5, dy - 0.5, dy + 0.5),
                0.0, 1.0);
            if (cov > 0.0) {
                let prof = integ;
                let sx = s.cx - u[4] * 0.5;
                let sy = s.cy - u[5] * 0.5;
                let rho_s2 = sx * sx + sy * sy;
                let f2 = u[6] * u[6];
                let k = pow((f2 + rho_s2) / f2, 1.5);
                let white_mix = clamp((s.l - 1.0) * 0.25, 0.0, 0.7);
                let col = mix(vec3<f32>(s.r, s.g, s.b), vec3<f32>(1.0), white_mix * prof);
                dep_sum = dep_sum + s.l * k * prof * cov * col;
            }
        }
    }
    let base = (py * u32(u[4]) + px) * 4u;
    accum[base + 0u] = u32(round(dep_sum.x * u[8]));
    accum[base + 1u] = u32(round(dep_sum.y * u[8]));
    accum[base + 2u] = u32(round(dep_sum.z * u[8]));
}
