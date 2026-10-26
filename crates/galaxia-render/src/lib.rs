//! galaxia-render — wgpu device, frame graph, passes, WGSL (PLAN §4.1).
//!
//! Iteration 0 scope: device context, a frame-graph *skeleton* (named pass
//! list + per-pass GPU timing), the point-star pass, WGSL hot-reload, and an
//! offscreen render target for headless screenshots.

pub mod frame_graph;
pub mod gpu;
pub mod hot_reload;
pub mod offscreen;
pub mod star_pass;
pub mod timing;
pub mod star_raster;
pub mod tonemap;

pub use frame_graph::{select_frame_ms, FrameGraph, PassTimings};
pub use tonemap::TonemapPass;
pub use gpu::GpuContext;
pub use hot_reload::ShaderWatcher;
pub use offscreen::OffscreenTarget;
pub use star_pass::{StarPass, StarRecord};
