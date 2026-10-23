//! Frame-graph SKELETON (PLAN §4.3, §6.1).
//!
//! It.0: a declarative, ordered list of named passes with enable flags and
//! per-pass GPU timing slots. This is NOT yet the real dependency-graph frame
//! graph of §4.3 (resource lifetime analysis, barrier insertion, async
//! compute) — it is the hook every later pass registers into, so the pass
//! list, ordering, and timings are in one place from day one.

/// A pass slot in the skeleton frame graph. `slot` indexes the GPU timing
/// query pairs (see timing.rs).
#[derive(Debug, Clone)]
pub struct PassSpec {
    pub name: &'static str,
    pub slot: u32,
    pub enabled: bool,
    /// Placeholder for §6.1 ordering; the real graph lands with It.2+.
    pub planned_in: &'static str,
}

/// The prototype pass list, in §6.1 order. Disabled passes are stubs that
/// exist so the overlay shows the full pipeline shape from week 1.
pub fn default_passes() -> Vec<PassSpec> {
    vec![
        PassSpec { name: "sky_clear",     slot: 0, enabled: true,  planned_in: "It.0" },
        PassSpec { name: "stars",         slot: 1, enabled: true,  planned_in: "It.0" },
        PassSpec { name: "star_raster_sw",slot: 2, enabled: false, planned_in: "It.2" },
        PassSpec { name: "glare_pyramid", slot: 3, enabled: false, planned_in: "It.2" },
        PassSpec { name: "volumetrics",   slot: 4, enabled: false, planned_in: "It.4" },
        PassSpec { name: "tonemap_fsr2",  slot: 5, enabled: false, planned_in: "It.2" },
        PassSpec { name: "overlay",       slot: 6, enabled: true,  planned_in: "It.0" },
    ]
}

pub struct FrameGraph {
    pub passes: Vec<PassSpec>,
}

/// Latest per-pass timings (ms). None = not measured (disabled/no GPU ts).
#[derive(Debug, Clone, Default)]
pub struct PassTimings {
    pub per_pass: Vec<Option<f32>>,
    pub frame_gpu_ms: Option<f32>,
}

impl FrameGraph {
    pub fn new() -> Self {
        Self { passes: default_passes() }
    }

    pub fn slot_count(&self) -> u32 {
        self.passes.len() as u32
    }

    pub fn get(&self, name: &str) -> Option<&PassSpec> {
        self.passes.iter().find(|p| p.name == name)
    }

    /// Fold raw timestamp pairs into PassTimings. `period_ns` is the adapter's
    /// timestamp period (wgpu exposes it via queue.get_timestamp_period()).
    pub fn fold_timings(&self, raw: &[(u64, u64)], period_ns: f32) -> PassTimings {
        let per_pass = self
            .passes
            .iter()
            .map(|p| {
                raw.get(p.slot as usize)
                    .and_then(|pair| crate::timing::GpuTimers::ms(*pair, period_ns))
                    .filter(|_| p.enabled)
            })
            .collect();
        let frame_gpu_ms = raw
            .first()
            .zip(raw.last())
            .and_then(|(a, b)| crate::timing::GpuTimers::ms((a.0, b.1), period_ns));
        PassTimings { per_pass, frame_gpu_ms }
    }
}

impl Default for FrameGraph {
    fn default() -> Self {
        Self::new()
    }
}
