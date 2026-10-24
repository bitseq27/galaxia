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

/// Choose the frame-time number for measurement rows (s008, closes the s007
/// "0.594 ms phantom").
///
/// GPU timestamps win only when PLAUSIBLE against the CPU wall time of the
/// same frame (the headless bench wall includes a blocking readback, so it
/// upper-bounds the GPU work):
///   - `> cpu*1.05 + 0.5`  : impossible (GPU cannot exceed the wall containing
///     it) -> reject;
///   - `< max(cpu*0.02, 0.01)` : >50x below the wall that contained it = the
///     broken-timestamp class (llvmpipe returns small nonzero garbage; the
///     s007 phantom was 0.594 ms against a ~50 ms wall) -> reject;
///   - everything else: trust the GPU number (real GPUs legitimately sit
///     well below the wall on CPU-bound scenes: e.g. 0.3 ms GPU in a 2 ms wall
///     = ratio 0.15, far above the 0.02 floor).
///
/// Returns (ms, source) for forensics.
pub fn select_frame_ms(
    gpu_frame_ms: Option<f32>,
    gpu_stars_ms: Option<f32>,
    cpu_wall_ms: f32,
) -> (f32, &'static str) {
    let floor = (cpu_wall_ms * 0.02).max(0.01);
    let ceil = cpu_wall_ms * 1.05 + 0.5;
    for (v, src) in [(gpu_frame_ms, "gpu"), (gpu_stars_ms, "gpu_stars")] {
        if let Some(v) = v
            && v.is_finite()
            && v >= floor
            && v <= ceil
        {
            return (v, src);
        }
    }
    (cpu_wall_ms, "cpu_wall")
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

#[cfg(test)]
mod frame_ms_tests {
    use super::select_frame_ms;

    #[test]
    fn s007_phantom_class_is_rejected() {
        // the actual s007 outlier: 0.594 ms GPU claim inside a ~50 ms wall
        let (ms, src) = select_frame_ms(Some(0.594), None, 50.0);
        assert_eq!(src, "cpu_wall");
        assert!((ms - 50.0).abs() < 1e-6);
    }

    #[test]
    fn none_and_sub_threshold_fall_back_to_cpu() {
        assert_eq!(select_frame_ms(None, None, 42.0).1, "cpu_wall");
        assert_eq!(select_frame_ms(Some(0.005), None, 42.0).1, "cpu_wall");
    }

    #[test]
    fn impossible_over_wall_is_rejected() {
        assert_eq!(select_frame_ms(Some(500.0), None, 50.0).1, "cpu_wall");
    }

    #[test]
    fn plausible_gpu_wins_including_cpu_bound_scenes() {
        assert_eq!(select_frame_ms(Some(30.0), None, 50.0).1, "gpu");
        // real-GPU CPU-bound ratio (0.3 ms GPU in 2 ms wall) stays trusted
        assert_eq!(select_frame_ms(Some(0.3), None, 2.0).1, "gpu");
    }

    #[test]
    fn stars_pass_is_the_second_witness() {
        assert_eq!(select_frame_ms(None, Some(30.0), 50.0).1, "gpu_stars");
        assert_eq!(select_frame_ms(None, Some(0.594), 50.0).1, "cpu_wall");
    }
}
