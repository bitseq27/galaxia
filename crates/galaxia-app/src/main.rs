//! galaxia — the binary: event loop, state machine, wiring (PLAN §4.1).
//!
//! Iteration 0: walking skeleton. Window opens, 10k stars render as points,
//! camera flies (exponential speed, damped rotation), debug overlay shows FPS
//! and per-pass GPU timings. `--screenshot` renders headless to PNG — this is
//! how the build sandbox produces docs/build/shots/ images.

mod app_window;
mod camera;
mod fly_test;
mod headless;
mod input;
mod octree_scene;
mod star_data;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "galaxia", version, about = "PROJECT MILKY WAY — Iteration 0 skeleton")]
pub struct Cli {
    /// Named scene (PLAN §17.3 harness starts here).
    #[arg(long, default_value = "orion_wide")]
    pub scene: String,

    /// Render headless and save a PNG screenshot, then exit.
    #[arg(long, value_name = "PATH")]
    pub screenshot: Option<PathBuf>,

    /// Run headless (no window) without saving — for benches/CI.
    #[arg(long)]
    pub headless: bool,

    /// Render/target width in pixels.
    #[arg(long, default_value_t = 2560)]
    pub width: u32,

    /// Render/target height in pixels.
    #[arg(long, default_value_t = 1440)]
    pub height: u32,

    /// Frames to render in headless mode (timing = last frame).
    #[arg(long, default_value_t = 3)]
    pub frames: u32,

    /// Print a measurements line (CSV-compatible) and exit. Implies --headless.
    #[arg(long)]
    pub bench: bool,

    /// Path to stars.wgsl (hot-reloaded in windowed mode).
    #[arg(long, default_value = "shaders/stars.wgsl")]
    pub shader: PathBuf,

    /// Override star data CSV (default: etl/norm/orion_bright_g10_20k.csv if present).
    #[arg(long, value_name = "PATH")]
    pub stars_csv: Option<PathBuf>,

    /// Exposure multiplier for the overlay/screenshot.
    #[arg(long, default_value_t = 1.0)]
    pub exposure: f32,

    /// Limiting magnitude for the octree LOD cull (It.1 scenes).
    #[arg(long, default_value_t = 13.0)]
    pub m0: f32,

    /// GPU resident-set capacity in stars (sandbox default below target's).
    #[arg(long, default_value_t = 1_500_000)]
    pub resident_cap: u32,

    /// Build-log session id stamped into measurements rows.
    #[arg(long, default_value = "003")]
    pub session: String,

    /// It.1 streaming gate: fly along +view for SECONDS at PC_PER_S, log
    /// per-frame streaming stats to docs/build/flytest.csv (implies headless).
    #[arg(long, value_name = "SECONDS,PC_PER_S")]
    pub fly_test: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "galaxia=info,galaxia_render=info,wgpu=warn".into()),
        )
        .init();

    if cli.fly_test.is_some() {
        pollster::block_on(fly_test::run(&cli))
    } else if cli.bench || cli.headless || cli.screenshot.is_some() {
        pollster::block_on(headless::run(&cli))
    } else {
        pollster::block_on(app_window::run(&cli))
    }
}
