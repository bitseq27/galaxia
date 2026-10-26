//! s009 — PLAN §7.3 tile rasteriser (ADR-0002): CPU binning + compute tile
//! pass + fixed-point resolve (ADR-0007).
//!
//! Photometry contract (mirrors stars.wgsl so the two paths are comparable):
//!   m_app   = mag + 5·log10(dist/d_sun)            (s003 camera-relative fix)
//!   gate    = m_app <= m0
//!   r_px    = clamp(10^(-0.2(m_app-5.5))·2.2, 0.8, 48)   (skeleton curve)
//!   L       = gate·clamp(10^(-0.4(m_app-6)), 0, 14)      (flux-ish radiance)
//!   k(θ)    = ((f²+ρ²)/f²)^{3/2}  — perspective-correct solid angle (§7.3
//!             [MUST]): a pixel at radial screen distance ρ subtends
//!             Ω_px ∝ cos³θ; scaling L by 1/cos³θ conserves the star's flux
//!             across the FOV and makes edge stars proportionally brighter.
//!   profile = (1-(ρ_disc/r)²)^1.6                     (same soft disc)
//!   deposit(pixel) = L·k·colour·profile(ρ_c)·coverage(pixel ∩ disc)
//! with coverage = exact circle/rect overlap AREA of the unit pixel square
//! (=> flux conservation by construction; anti-aliased edges; no shimmer).
//!
//! Accumulation: per-channel u32 fixed-point Q10 atomicAdd (ADR-0007),
//! resolved into the s008 HDR target by a fullscreen pass (÷1024 + background).

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct RasterStar {
    pub cx: f32,
    pub cy: f32,
    pub r_px: f32,
    pub l_k: f32, // L * k(theta)
    pub color: [f32; 3],
    pub white_mix: f32,
    pub depth: f32,
    pub _pad: f32,
}

/// Fixed-point scale of the u32 accumulator (ADR-0007): Q10.
pub const ACCUM_Q: f32 = 1024.0;
pub const TILE_PX: u32 = 8;

/// Exact area of the intersection of the circle (radius `r`, centred at the
/// origin) with the axis-aligned rect [x0,x1]×[y0,y1], via the signed
/// quadrant integral `corner_area`. Mirror of `circle_rect_area` in
/// shaders/star_raster.wgsl — keep in lockstep (unit tests pin both against
/// numeric integration).
pub fn circle_rect_area(r: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> f32 {
    corner_area(r, x1, y1) - corner_area(r, x0, y1)
        - corner_area(r, x1, y0)
        + corner_area(r, x0, y0)
}

/// ∫∫_{[0,x]×[0,y]} 1_{|p|<=r} dp with sign handling for negative x/y.
fn corner_area(r: f32, x: f32, y: f32) -> f32 {
    if r <= 0.0 {
        return 0.0;
    }
    let sx = if x >= 0.0 { 1.0 } else { -1.0 };
    let sy = if y >= 0.0 { 1.0 } else { -1.0 };
    sx * sy * quadrant_area(r, x.abs(), y.abs())
}

/// Area of circle∩[0,x]×[0,y] for x,y >= 0. Both arguments saturate at r
/// (the circle lies inside |p| <= r, so extending beyond adds nothing).
fn quadrant_area(r: f32, x: f32, y: f32) -> f32 {
    let x = x.min(r);
    let y = y.min(r);
    let r2 = r * r;
    // ∫_0^x min(y, sqrt(r²-t²)) dt; the integrand switches at t = xc.
    let xc = (r2 - y * y).sqrt();
    if x <= xc {
        x * y
    } else {
        let rect = xc * y;
        let hi = x; // already <= r
        let integral = 0.5 * (hi * (r2 - hi * hi).sqrt() + r2 * (hi / r).asin())
            - 0.5 * (xc * (r2 - xc * xc).sqrt() + r2 * (xc / r).asin());
        rect + integral
    }
}

/// Coverage fraction of the unit pixel square centred at (px,py) by the
/// disc of radius r at the origin.
pub fn pixel_coverage(r: f32, px: f32, py: f32) -> f32 {
    circle_rect_area(r, px - 0.5, px + 0.5, py - 0.5, py + 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod raster_math_tests {
    use super::{circle_rect_area, pixel_coverage, quadrant_area};

    fn numeric_circle_rect(r: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> f32 {
        // fine grid integration as the reference
        let n = 400;
        let dx = (x1 as f64 - x0 as f64) / n as f64;
        let dy = (y1 as f64 - y0 as f64) / n as f64;
        let mut acc = 0.0f64; // f64: f32 accumulation drifts over 160k adds
        for i in 0..n {
            let x = x0 as f64 + (i as f64 + 0.5) * dx;
            for j in 0..n {
                let y = y0 as f64 + (j as f64 + 0.5) * dy;
                if x * x + y * y <= r as f64 * r as f64 {
                    acc += dx * dy;
                }
            }
        }
        acc as f32
    }

    #[test]
    fn quadrant_matches_numeric() {
        for (r, x, y) in [
            (3.0, 1.0, 1.0),
            (3.0, 5.0, 5.0),
            (3.0, 2.5, 4.0),
            (0.7, 0.4, 0.9),
            (12.0, 3.3, 2.2),
        ] {
            let closed = quadrant_area(r, x, y);
            let num = numeric_circle_rect(r, 0.0, x, 0.0, y);
            assert!(
                (closed - num).abs() < 1e-3 * num.max(1e-3),
                "r={r} x={x} y={y}: closed {closed} vs numeric {num}"
            );
        }
    }

    #[test]
    fn rect_area_matches_numeric_all_quadrants() {
        let r = 2.3;
        for (x0, x1, y0, y1) in [
            (-0.2, 0.7, -0.4, 0.6),
            (1.5, 2.5, -0.5, 0.5),
            (-3.0, -1.9, 0.2, 1.1),
            (-0.5, 0.5, 2.0, 3.0),
            (-5.0, 5.0, -5.0, 5.0),
        ] {
            let closed = circle_rect_area(r, x0, x1, y0, y1);
            let num = numeric_circle_rect_sub(r, x0, x1, y0, y1);
            // numeric grid has O(boundary-cell) bias; closed form is exact
            // (the full-disc box equals pi r^2 to f32)
            assert!(
                (closed - num).abs() < 2e-3f32.max(1e-3f32 * num),
                "box ({x0},{x1},{y0},{y1}): {closed} vs {num}"
            );
        }
    }

    fn numeric_circle_rect_sub(r: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> f32 {
        let n = 500;
        let dx = (x1 as f64 - x0 as f64) / n as f64;
        let dy = (y1 as f64 - y0 as f64) / n as f64;
        let mut acc = 0.0f64;
        for i in 0..n {
            let x = x0 as f64 + (i as f64 + 0.5) * dx;
            for j in 0..n {
                let y = y0 as f64 + (j as f64 + 0.5) * dy;
                if x * x + y * y <= r as f64 * r as f64 {
                    acc += dx * dy;
                }
            }
        }
        acc as f32
    }

    #[test]
    fn coverage_conserves_disc_area() {
        // sum of unit-pixel coverages over a fine lattice == disc area pi r^2
        for r in [0.8f32, 2.5, 7.3, 24.0] {
            let mut sum = 0.0f64;
            let rr = (r + 1.0).ceil() as i32;
            for ix in -rr..=rr {
                for iy in -rr..=rr {
                    sum += pixel_coverage(r, ix as f32, iy as f32) as f64;
                }
            }
            let exact = std::f64::consts::PI * (r as f64) * (r as f64);
            let err = (sum - exact).abs() / exact;
            assert!(err < 0.01, "r={r}: flux err {err:.5} (sum {sum} vs {exact})");
        }
    }

    #[test]
    fn coverage_bounds_and_symmetry() {
        assert_eq!(pixel_coverage(2.0, 0.0, 0.0), 1.0);
        assert_eq!(pixel_coverage(2.0, 100.0, 0.0), 0.0);
        let a = pixel_coverage(3.1, 2.6, -1.3);
        let b = pixel_coverage(3.1, -2.6, 1.3);
        assert!((a - b).abs() < 1e-6);
    }
}
