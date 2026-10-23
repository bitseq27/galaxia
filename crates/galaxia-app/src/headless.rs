//! Headless render path: screenshots (`--screenshot`), benches (`--bench`),
//! CI smoke (`--headless`). Produces the numbers for measurements.csv and the
//! PNGs for docs/build/shots/ — the sandbox has no display, so THIS is how the
//! build session satisfies "every iteration ends screenshotted".

use crate::camera::Camera;
use crate::star_data;
use crate::Cli;
use anyhow::Result;
use galaxia_render::{FrameGraph, GpuContext, OffscreenTarget, ShaderWatcher, StarPass};
use std::time::Instant;

pub const FOV_Y_RAD: f32 = 60f32.to_radians();

pub fn camera_for_scene(scene: &str) -> Camera {
    let mut cam = Camera::default();
    match scene {
        // Orion cone centre (PLAN §16.2): RA 83.82°, Dec −5.39°.
        "orion_wide" => cam.look_at_equatorial(83.82, -5.39),
        "orion_belt" => cam.look_at_equatorial(84.053, -1.202),   // Alnilam
        "betelgeuse" => cam.look_at_equatorial(88.793, 7.407),
        "pleiades" => cam.look_at_equatorial(56.75, 24.12),
        "skeleton_10k" => cam.look_at_equatorial(83.82, -5.39),
        other => {
            tracing::warn!(scene = other, "unknown scene, defaulting to orion_wide");
            cam.look_at_equatorial(83.82, -5.39);
        }
    }
    cam
}

pub async fn run(cli: &Cli) -> Result<()> {
    let gpu = GpuContext::new(true).await?;
    let target = OffscreenTarget::new(&gpu.device, cli.width, cli.height)?;

    // Shader + hot-reload watcher (headless reads it once; the watcher keeps
    // the same code path as windowed mode).
    let mut watcher = ShaderWatcher::new(&cli.shader)?;
    let source = watcher.changed().unwrap_or_else(|| std::fs::read_to_string(&cli.shader).unwrap());
    let mut star_pass = StarPass::new(&gpu.device, &source)?;

    // Stars.
    let src = star_data::load_stars(cli.stars_csv.as_deref(), &cli.scene);
    tracing::info!(stars = src.stars.len(), provenance = %src.provenance, "star source");
    let gpu_stars: Vec<_> = src.stars.iter().map(|s| s.to_gpu()).collect();
    star_pass.set_stars(&gpu.queue, &gpu_stars);

    // Frame graph + GPU timers.
    let graph = FrameGraph::new();
    let mut timers = galaxia_render::timing::GpuTimers::new(
        &gpu.device,
        gpu.has_timestamps,
        graph.slot_count(),
    );
    let ts_period = gpu.queue.get_timestamp_period(); // ns per tick

    let cam = camera_for_scene(&cli.scene);
    let aspect = cli.width as f32 / cli.height as f32;
    let px_scale = 2.0 * (FOV_Y_RAD * 0.5).tan() / cli.height as f32;

    let commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    // Render frames; time the last one.
    let mut last_cpu_ms = f64::NAN;
    let mut last_gpu = galaxia_render::PassTimings::default();
    for frame in 0..cli.frames.max(1) {
        let t0 = Instant::now();
        let (right, up) = cam.basis_f32();
        let uniforms = galaxia_render::star_pass::FrameUniforms {
            view_proj: cam.view_proj_f32(aspect, FOV_Y_RAD, 1e-5).to_cols_array_2d(),
            cam_right: right,
            px_scale,
            cam_up: up,
            time_s: frame as f32 * 0.016,
            exposure: cli.exposure,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
            _pad4: 0.0,
            _pad5: 0.0,
            _pad6: 0.0,
        };
        star_pass.write_uniforms(&gpu.queue, &uniforms);

        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("headless-frame"),
        });
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.002,
                            g: 0.003,
                            b: 0.010,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            timers.begin(&mut rpass, 0);
            timers.end(&mut rpass, 0);
            timers.begin(&mut rpass, 1);
            star_pass.draw(&mut rpass);
            timers.end(&mut rpass, 1);
        }
        target.encode_copy(&mut encoder);
        let did_resolve = timers.resolve(&mut encoder);

        if frame + 1 == cli.frames {
            // Last frame: read pixels (blocks — screenshots only).
            let rgba = target.finish_read(&gpu.queue, encoder)?;
            last_cpu_ms = t0.elapsed().as_secs_f64() * 1e3;
            if let Some(path) = &cli.screenshot {
                target.save_png(path, &rgba)?;
            }
            // Count lit pixels as a sanity check that stars actually drew.
            let lit = rgba
                .chunks_exact(4)
                .filter(|p| (p[0] as u32 + p[1] as u32 + p[2] as u32) > 24)
                .count();
            tracing::info!(lit_pixels = lit, total = rgba.len() / 4, "frame content check");
            if cli.bench || cli.screenshot.is_some() {
                println!("LIT_PIXELS={lit}");
            }
        } else {
            gpu.queue.submit([encoder.finish()]);
        }
        timers.begin_readback(did_resolve);
        if let Some(raw) = timers.poll_results(&gpu.device) {
            last_gpu = graph.fold_timings(&raw, ts_period);
        }
    }
    // Give the timers one more poll chance.
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    if let Some(raw) = timers.poll_results(&gpu.device) {
        last_gpu = graph.fold_timings(&raw, ts_period);
    }

    // GPU timestamps on software rasterisers (llvmpipe) can report ~0 and
    // produce absurd fps; treat anything < 0.01 ms as invalid and fall back
    // to CPU wall time. On real GPUs the GPU number is authoritative.
    let valid = |v: Option<f32>| v.filter(|x| *x >= 0.01);
    let stars_ms = valid(last_gpu.per_pass.get(1).copied().flatten());
    let frame_ms = valid(last_gpu.frame_gpu_ms)
        .or(stars_ms)
        .unwrap_or(last_cpu_ms as f32);
    let fps = if frame_ms > 0.0 { 1000.0 / frame_ms } else { 0.0 };

    if cli.bench {
        // measurements.csv-compatible row (docs/build/measurements.csv header):
        // timestamp,session,iteration,commit,scene,stars_visible,star_raster_ms,
        // volumetrics_ms,glare_ms,post_ms,frame_ms_total,fps,peak_vram_gb,
        // resident_leaves,stream_mb_s,notes
        let now = humantime_now();
        println!(
            "{now},001,0,{commit},{scene},{stars},{star_ms},,,,{frame_ms},{fps},,,,sandbox lavapipe software raster — NOT Titan RTX numbers; It.0 skeleton point stars",
            scene = cli.scene,
            stars = src.stars.len(),
            star_ms = stars_ms.map(|v| format!("{v:.3}")).unwrap_or_default(),
            frame_ms = format!("{frame_ms:.3}"),
            fps = format!("{fps:.1}"),
        );
    }
    tracing::info!(
        scene = %cli.scene,
        stars = src.stars.len(),
        cpu_ms = last_cpu_ms,
        gpu_stars_ms = stars_ms,
        "headless run complete"
    );
    Ok(())
}

fn humantime_now() -> String {
    // Epoch seconds until chrono lands in It.1; measurements.csv consumers
    // (STATUS.yaml rewrite) treat this column as an opaque token for now.
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    format!("epoch+{}s", d.as_secs())
}
