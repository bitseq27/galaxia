//! Star data sources for the skeleton:
//!  1. the real Gaia bright subset CSV (DAY0 §2: TOP 20000, G < 10, Orion cone),
//!  2. a deterministic procedural fallback (10k disk stars) when the CSV is absent.
//!
//! Records carry provenance in the returned source string; the provenance flag
//! per object (D2) becomes a real field in It.1 with the baked format.

use galaxia_core::color::{bp_rp_to_linear_srgb, UNKNOWN_RGB};
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
    /// Rows whose distance came from `d_lit_pc` (literature compilation, s005;
    /// flags bit2 class — bit0 provenance stays "real": positions/mags are Gaia).
    /// The count is surfaced in `provenance`; the field lands in the It.2
    /// overlay provenance panel.
    #[allow(dead_code)]
    pub literature_distances: usize,
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
    // s005: optional literature-distance column (etl/enrich_bright_csv.py).
    // A usable Gaia parallax ALWAYS wins; d_lit_pc only rescues rows whose
    // parallax is null/negative/too-small — counted separately, never invented.
    let c_dlit = col("d_lit_pc");

    let mut stars = Vec::with_capacity(MAX_SKELETON_STARS);
    let mut skipped = 0usize;
    let mut lit_used = 0usize;
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
        let usable = |d: Option<f64>| d.filter(|d| d.is_finite() && *d > 0.0 && *d < 5000.0);
        let (dist_pc, from_lit) = match usable(parse(c_plx).and_then(parallax_mas_to_pc)) {
            Some(d) => (d, false),
            None => match usable(c_dlit.and_then(parse)) {
                Some(d) => (d, true),
                None => {
                    skipped += 1;
                    continue; // NaN/unknown is valid data, but an unplaceable
                              // star cannot render — counted, never invented.
                }
            },
        };
        lit_used += from_lit as usize;
        let pos_au = equatorial_to_cartesian_au(ra, dec, dist_pc);
        let pos_pc = [pos_au.x / AU_IN_PC, pos_au.y / AU_IN_PC, pos_au.z / AU_IN_PC];
        let color = match c_bprp.and_then(parse).map(|v| v as f32) {
            Some(bp_rp) if bp_rp.is_finite() => bp_rp_to_linear_srgb(bp_rp),
            _ => UNKNOWN_RGB,
        };
        stars.push(StarRecord { pos_pc, mag: g as f32, color });
    }
    Ok(StarSource {
        provenance: format!(
            "Gaia DR3 bright subset (G<10, Orion cone r=20°), {} rows used, {} skipped, \
             {} literature distances (s005, SIMBAD-compiled, bit2 class)",
            stars.len(),
            skipped,
            lit_used
        ),
        stars,
        skipped_no_distance: skipped,
        literature_distances: lit_used,
    })
}

/// Deterministic procedural 10k — seeded xoshiro (NEVER thread_rng in content
/// generation, PLAN Appendix B). Thin-disk-ish distribution around the Sun so
/// the skeleton has plausible structure before real data arrives.
pub fn procedural_10k() -> StarSource {
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(0x00C0_FFEE);
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
            pos_pc: [x, y, z.clamp(-400.0, 400.0)],
            mag: mag as f32,
            color: bp_rp_to_linear_srgb(bp_rp),
        });
    }
    StarSource {
        provenance: "procedural fallback (seed 0xC0FFEE, galactic-disk-ish, 10k)".to_string(),
        stars,
        skipped_no_distance: 0,
        literature_distances: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Repo ETL norm dir as seen from this crate (tests run in the repo tree).
    fn norm_path(file: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../etl/norm")
            .join(file)
    }

    /// s005 DATA GATE: the committed literature table must carry Saiph
    /// (kappa Ori, Gaia DR3 source_id 3011968416163350272) — the brightest
    /// null-parallax star of the Orion cone (G=2.0778). NOTE: earlier session
    /// notes called this star "Rigel G=2.08"; s005 verified against the
    /// archive that DR3 Rigel (3206991473643150464) HAS parallax 3.235 mas
    /// (G=6.60, degraded photometry, VARIABLE) and needs no literature help —
    /// the G=2.08 null-parallax star at (86.939, -9.670) is Saiph.
    /// Literature constant: SIMBAD adopted PLX_VALUE = 5.04 mas => 198.41 pc
    /// (full provenance in the table's origin column).
    #[test]
    fn literature_table_has_saiph_within_10pct() {
        let text = std::fs::read_to_string(norm_path("literature_distances.csv"))
            .expect("etl/norm/literature_distances.csv must be committed");
        let mut lines = text.lines();
        let header: Vec<&str> = lines.next().expect("header").split(',').collect();
        let c_sid = header.iter().position(|h| *h == "source_id").unwrap();
        let c_d = header.iter().position(|h| *h == "d_pc").unwrap();
        let mut found = None;
        for line in lines {
            let f: Vec<&str> = line.split(',').collect();
            if f.get(c_sid) == Some(&"3011968416163350272") {
                found = f.get(c_d).and_then(|s| s.parse::<f64>().ok());
                break;
            }
        }
        let d = found.expect("Saiph must be resolved in the literature table");
        let lit = 198.413; // 1000 / 5.04 mas (SIMBAD adopted, see origin col)
        assert!(
            (d - lit).abs() <= 0.10 * lit,
            "Saiph d={d} pc deviates more than 10% from literature {lit} pc"
        );
    }

    /// s005 DATA GATE: the enriched bright-subset CSV must rescue the
    /// literature stars through the loader (pre-s005 they were skipped), and
    /// Saiph must land at its literature distance in the right direction.
    #[test]
    fn loader_rescues_literature_stars() {
        let src = load_gaia_csv(&norm_path("orion_bright_g10_20k.csv"))
            .expect("bright subset CSV must load");
        assert!(
            src.literature_distances >= 50,
            "expected most of the 120 literature rows inside the 10k window, got {}",
            src.literature_distances
        );
        // Saiph: ra 86.93912852502406 deg, dec -9.669607137549677 deg, 198.413 pc.
        let (ra, dec, d) = (86.93912852502406f64, -9.669607137549677f64, 198.413f64);
        let cd = dec.to_radians().cos();
        let want = [
            d * cd * ra.to_radians().cos(),
            d * cd * ra.to_radians().sin(),
            d * dec.to_radians().sin(),
        ];
        let wlen = (want[0] * want[0] + want[1] * want[1] + want[2] * want[2]).sqrt();
        let mut best_ang = f64::MAX;
        let mut best_r = 0f64;
        for s in &src.stars {
            let p = s.pos_pc;
            let plen = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            if plen <= 0.0 {
                continue;
            }
            let dot = (p[0] * want[0] + p[1] * want[1] + p[2] * want[2]) / (plen * wlen);
            let ang = dot.clamp(-1.0, 1.0).acos().to_degrees();
            if ang < best_ang {
                best_ang = ang;
                best_r = plen;
            }
        }
        assert!(best_ang < 0.05, "no loaded star within 0.05 deg of Saiph ({best_ang} deg)");
        assert!(
            (best_r - d).abs() <= 0.10 * d,
            "Saiph rendered at r={best_r} pc, literature {d} pc (+-10% gate)"
        );
    }

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
