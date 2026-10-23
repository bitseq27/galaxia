//! galaxia-core — units, math, precision, ids, error, config, time (PLAN §4.1).
//!
//! Iteration 0 scope: canonical unit newtypes (§5.1 [MUST]), equatorial→cartesian
//! transform, and a CRUDE bp_rp→RGB placeholder colour (replaced by the
//! Harre–Heller chain in Iteration 2 — see PLAN §7.1).

pub mod color;
pub mod color_hh_table;
pub mod math;
pub mod units;

/// The Iteration-0 precision shortcut, documented per BUILD_BRIEF:
/// a single f64 frame centred on the Sun, positions in AU. Valid to ~1 kpc
/// (covers the Orion prototype region). MUST be retired in Iteration 3
/// when the full §H1 frame hierarchy lands. Do not build on it.
pub const IT0_SINGLE_FRAME_SHORTCUT: &str =
    "Sun-centred f64 AU frame; f32 view matrix rebuilt per frame from f64 camera";
