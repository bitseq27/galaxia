//! CRUDE placeholder colour for the Iteration-0 skeleton: Gaia (BP−RP) → linear
//! RGB via piecewise-linear interpolation of a handful of reference points.
//!
//! This is deliberately NOT the science chain. Iteration 2 replaces it with the
//! full Harre–Heller (2021) colour table + PBR photometry (PLAN §7.1, gate:
//! 20 reference stars within ΔE2000 < 3). Recorded in BUILDLOG session 001.

/// (bp_rp, linear-ish sRGB) anchor points, blue → red.
const ANCHORS: [(f32, [f32; 3]); 7] = [
    (-0.5, [0.62, 0.72, 1.00]), // hot O/B
    (0.0, [0.76, 0.83, 1.00]), // A0 (Vega-like)
    (0.5, [0.92, 0.93, 1.00]), // F
    (0.82, [1.00, 1.00, 0.96]), // G2 (Sun)
    (1.15, [1.00, 0.91, 0.78]), // K2
    (1.8, [1.00, 0.74, 0.55]), // K7/M0
    (3.5, [1.00, 0.55, 0.42]), // M5+
];

/// Piecewise-linear BP−RP → RGB. Clamps outside the anchor range.
pub fn bp_rp_to_rgb_crude(bp_rp: f32) -> [f32; 3] {
    let x = bp_rp.clamp(ANCHORS[0].0, ANCHORS[ANCHORS.len() - 1].0);
    for w in ANCHORS.windows(2) {
        let (x0, c0) = w[0];
        let (x1, c1) = w[1];
        if x <= x1 {
            let t = ((x - x0) / (x1 - x0)).clamp(0.0, 1.0);
            return [
                c0[0] + (c1[0] - c0[0]) * t,
                c0[1] + (c1[1] - c0[1]) * t,
                c0[2] + (c1[2] - c0[2]) * t,
            ];
        }
    }
    ANCHORS[ANCHORS.len() - 1].1
}

/// Missing/NaN colour must render as a distinct "unknown" marker, never as
/// black or white (NON-NEGOTIABLE: never invent data). Magenta-ish.
pub const UNKNOWN_RGB: [f32; 3] = [1.0, 0.25, 0.9];

/// G magnitude → relative flux (Vega-normalised). Photometry proper is §7.1.
#[inline]
pub fn mag_to_flux(mag: f32) -> f32 {
    10f32.powf(-0.4 * mag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_is_near_white() {
        let c = bp_rp_to_rgb_crude(0.82);
        assert!((c[0] - 1.0).abs() < 0.01 && (c[2] - 0.96).abs() < 0.01);
    }

    #[test]
    fn clamps_at_extremes() {
        assert_eq!(bp_rp_to_rgb_crude(-9.0), ANCHORS[0].1);
        assert_eq!(bp_rp_to_rgb_crude(9.0), ANCHORS[6].1);
    }

    #[test]
    fn nan_input_does_not_panic() {
        let c = bp_rp_to_rgb_crude(f32::NAN);
        assert!(c.iter().all(|v| v.is_finite()), "{c:?}");
    }
}
