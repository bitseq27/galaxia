//! Star data sources for the skeleton:
//!  1. the real Gaia bright subset CSV (DAY0 §2: TOP 20000, G < 10, Orion cone),
//!  2. a deterministic procedural fallback (10k disk stars) when the CSV is absent.
//!
//! Records carry provenance in the returned source string; the provenance flag
//! per object (D2) becomes a real field in It.1 with the baked format.

use galaxia_core::color::{bp_rp_to_rgb_crude, UNKNOWN_RGB};
use galaxia_core::math::{equatorial_to_cartesian_au, parallax_mas_to_pc};
use galaxia_core::units::AU_IN_PC;
use galaxia_render::StarRecord;
use rand::prelude::*;
use rand_xoshiro::Xoshiro256PlusPlus;
use rand_xoshiro::rand_core::SeedableRng;
use std::path::Path;

pub const MAX_SKELETON_STARS: usize = 10_000;

pub struct StarSource {
    pub stars: Vec<StarRecord>,
    pub provenance: String,
    #[allow(dead_code)] // provenance reporting lands with the It.1 ETL
    pub skipped_no_distance: usize,
}

/// Try the real Gaia CSV first; fall back to procedural.
pub fn load_stars(csv_override: Option<&Path>, scene: &str) -> StarSource {
    let csv_path = csv_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| Path::new("etl/norm/orion_bright_g10_20k.csv").to_path_buf());
    if csv_path.exists() {
        match load_gaia_csv(&csv_path) {
            Ok(src) if !src.stars.is_empty() => return src,
            Ok(_) => tracing::warn!("CSV present but 0 usable rows; procedural fallback"),
            Err(e) => tracing::warn!("CSV load failed ({e}); procedural fallback"),
        }
    }
    if scene == "pleiades" {
        // Placeholder preset until the Pleiades fixture is fetched (DAY0 §4).
        tracing::info!("pleiades scene without fixture — procedural fallback");
    }
    procedural_10k()
}

/// Parse the DAY0 bright-subset CSV. Header-driven (column order may change
/// between fetches); never invent data: missing distance ⇒ skipped + counted,
/// missing colour ⇒ UNKNOWN_RGB (must render visibly as unknown).
pub fn load_gaia_csv(path: &Path) -> anyhow::Result<StarSource> {
    let text = std::fs::read_to_string(path)?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty CSV"))?
        .split(',')
        .map(|s| s.trim())
        .collect();
    let col = |name: &str| header.iter().position(|h| *h == name);
    let (Some(c_ra), Some(c_dec), Some(c_g), Some(c_plx)) =
        (col("ra"), col("dec"), col("phot_g_mean_mag"), col("parallax"))
    else {
        anyhow::bail!("CSV missing required columns (ra/dec/phot_g_mean_mag/parallax)");
    };
    let c_bprp = col("bp_rp");

    let mut stars = Vec::with_capacity(MAX_SKELETON_STARS);
    let mut skipped = 0usize;
    for line in lines {
        if stars.len() >= MAX_SKELETON_STARS {
            break;
        }
        let f: Vec<&str> = line.split(',').collect();
        let parse = |i: usize| f.get(i).and_then(|s| s.trim().parse::<f64>().ok());
        let (Some(ra), Some(dec), Some(g)) = (parse(c_ra), parse(c_dec), parse(c_g)) else {
            skipped += 1;
            continue;
        };
        // Rows are ORDER BY phot_g_mean_mag so the cap keeps the brightest.
        let dist_pc = match parse(c_plx).and_then(parallax_mas_to_pc) {
            Some(d) if d.is_finite() && d > 0.0 && d < 5000.0 => d,
            _ => {
                skipped += 1;
                continue; // NaN/unknown is valid data, but an unplaceable star
                           // cannot render — counted, never invented.
            }
        };
        let pos_au = equatorial_to_cartesian_au(ra, dec, dist_pc);
        let pos_pc = [pos_au.x / AU_IN_PC, pos_au.y / AU_IN_PC, pos_au.z / AU_IN_PC];
        let color = match c_bprp.and_then(|i| parse(i)).map(|v| v as f32) {
            Some(bp_rp) if bp_rp.is_finite() => bp_rp_to_rgb_crude(bp_rp),
            _ => UNKNOWN_RGB,
        };
        stars.push(StarRecord { pos_pc, mag: g as f32, color });
    }
    Ok(StarSource {
        provenance: format!(
            "Gaia DR3 bright subset (G<10, Orion cone r=20°), {} rows used, {} skipped",
            stars.len(),
            skipped
        ),
        stars,
        skipped_no_distance: skipped,
    })
}

/// Deterministic procedural 10k — seeded xoshiro (NEVER thread_rng in content
/// generation, PLAN Appendix B). Thin-disk-ish distribution around the Sun so
/// the skeleton has plausible structure before real data arrives.
pub fn procedural_10k() -> StarSource {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(0xC0FF_EE);
    let mut stars = Vec::with_capacity(MAX_SKELETON_STARS);
    for _ in 0..MAX_SKELETON_STARS {
        let r = 5.0 + 900.0 * rng.random_range(0f64..1.0).powf(1.6); // pc
        let theta = rng.random_range(0.0..std::f64::consts::TAU);
        // Exponential vertical profile, h ≈ 60 pc (thin disk, crude).
        let z = if rng.random_bool(0.5) { 1.0 } else { -1.0 }
            * 60.0 * (-rng.random_range(1e-9f64..1.0).ln());
        let x = r * theta.cos();
        let y = r * theta.sin();
        // Brightness: log-uniform-ish, biased faint.
        let mag = 1.0 + 8.0 * rng.random_range(0f64..1.0).powf(1.8);
        let bp_rp = rng.random_range(-0.3f32..2.4);
        stars.push(StarRecord {
            pos_pc: [x, y, z.min(400.0).max(-400.0)],
            mag: mag as f32,
            color: bp_rp_to_rgb_crude(bp_rp),
        });
    }
    StarSource {
        provenance: "procedural fallback (seed 0xC0FFEE, galactic-disk-ish, 10k)".to_string(),
        stars,
        skipped_no_distance: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_is_deterministic() {
        let a = procedural_10k();
        let b = procedural_10k();
        assert_eq!(a.stars.len(), 10_000);
        assert_eq!(a.stars[0].pos_pc, b.stars[0].pos_pc);
        assert_eq!(a.stars[9_999].mag, b.stars[9_999].mag);
    }

    #[test]
    fn procedural_positions_finite_and_bounded() {
        let s = procedural_10k();
        for st in &s.stars {
            assert!(st.pos_pc.iter().all(|v| v.is_finite()));
            let r = (st.pos_pc[0] * st.pos_pc[0]
                + st.pos_pc[1] * st.pos_pc[1]
                + st.pos_pc[2] * st.pos_pc[2])
                .sqrt();
            assert!((5.0..=1000.0).contains(&r), "r={r}");
        }
    }
}
