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
        "t0_orion_m13" => cam.look_at_equatorial(83.82, -5.39),
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
    if cli.probe_compute {
        return probe_compute(&gpu).await;
    }
    let target = OffscreenTarget::new(&gpu.device, cli.width, cli.height, gpu.hdr_capable)?;
    // s008 HDR chain: stars -> hdr target; tonemap -> LDR readback target.
    let mut tonemap = galaxia_render::TonemapPass::new(
        &gpu.device,
        target.hdr_format,
        galaxia_render::star_pass::LDR_FORMAT,
    )?;
    tonemap.bind(&gpu.device, &target.hdr_view);
    tonemap.set_exposure(&gpu.queue, cli.exposure);
    // s009: §7.3 tile rasteriser (ADR-0002), flag-gated until its gates pass.
    let use_raster = cli.star_renderer == "raster";
    let raster = if use_raster {
        Some(galaxia_render::star_raster::StarRasterPass::new(
            &gpu.device,
            &gpu.queue,
            cli.width,
            cli.height,
            [0.002, 0.003, 0.010],
            FOV_Y_RAD,
            target.hdr_format,
        )?)
    } else {
        None
    };
    let mut bin_input: Vec<(glam::DVec3, f32, [f32; 3])> = Vec::new();

    // Shader + hot-reload watcher (headless reads it once; the watcher keeps
    // the same code path as windowed mode).
    let mut watcher = ShaderWatcher::new(&cli.shader)?;
    let source = watcher.changed().unwrap_or_else(|| std::fs::read_to_string(&cli.shader).unwrap());
    let mut star_pass = StarPass::new(&gpu.device, &source, target.hdr_format)?;
    let use_t0 = cli.scene.starts_with("t0")
        && std::path::Path::new("etl/build/t0/tree_meta.json").exists();
    let mut octree: Option<crate::octree_scene::OctreeScene> = None;
    let mut octree_stats: Option<crate::octree_scene::FrameStats> = None;
    if use_t0 {
        let tree = galaxia_data::T0Tree::load(std::path::Path::new("etl/build/t0"))?;
        star_pass.ensure_capacity(cli.resident_cap);
        octree = Some(crate::octree_scene::OctreeScene::new(tree, cli.resident_cap)?);
    }

    // Octree (It.1) scenes: t0_* stream from the baked T0 artifact set.
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
    let mut osc_gpu_ranges: Vec<(u32, u32)> = Vec::new();
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
            m0: if use_t0 { cli.m0 } else { 99.0 },
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
            _pad4: 0.0,
            _pad5: 0.0,
            _pad6: 0.0,
        };
        // s009 shimmer harness: sub-pixel NDC translation of the projection
        let (shx, shy) = {
            let mut it = cli.screen_shift_px.split(',');
            (
                it.next().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0),
                it.next().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0),
            )
        };
        let uniforms = if shx != 0.0 || shy != 0.0 {
            let mut vp = glam::Mat4::from_cols_array_2d(&uniforms.view_proj);
            let shift = glam::Mat4::from_translation(glam::Vec3::new(
                2.0 * shx / cli.width as f32,
                -2.0 * shy / cli.height as f32,
                0.0,
            ));
            vp = shift * vp;
            galaxia_render::star_pass::FrameUniforms {
                view_proj: vp.to_cols_array_2d(),
                ..uniforms
            }
        } else {
            uniforms
        };
        star_pass.write_uniforms(&gpu.queue, &uniforms);

        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("headless-frame"),
        });
        let m0_bin = if use_t0 { cli.m0 } else { 99.0 };
        if let Some(raster) = raster.as_ref() {
            let vp = glam::Mat4::from_cols_array_2d(&uniforms.view_proj);
            if let Some(osc) = octree.as_mut() {
                let stats = osc.frame(cam.pos_pc, &vp, cli.m0, &mut star_pass, &gpu.queue);
                osc_gpu_ranges.clear();
                osc_gpu_ranges.extend_from_slice(osc.draw_ranges());
                octree_stats = Some(stats);
                osc.collect_stars(&mut bin_input);
            } else {
                bin_input.clear();
                bin_input.extend(
                    src.stars.iter().map(|r| (glam::DVec3::from(r.pos_pc), r.mag, r.color)),
                );
            }
            let bin = galaxia_render::star_raster::bin_stars(
                &bin_input,
                &vp,
                cli.width,
                cli.height,
                m0_bin,
            );
            if frame == 0 {
                tracing::info!(stars = bin.stars.len(), ids = bin.tile_ids.len(),
                               tiles = bin.tiles_x, "raster bin stats");
            }
            raster.encode(&gpu.device, &gpu.queue, &mut encoder, &bin, &target.hdr_view)?;
        } else {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.hdr_view,
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
            if let Some(osc) = octree.as_mut() {
                let vp = glam::Mat4::from_cols_array_2d(&uniforms.view_proj);
                let stats =
                    osc.frame(cam.pos_pc, &vp, cli.m0, &mut star_pass, &gpu.queue);
                osc_gpu_ranges.clear();
                osc_gpu_ranges.extend_from_slice(osc.draw_ranges());
                octree_stats = Some(stats);
                star_pass.draw_ranges(&mut rpass, &osc_gpu_ranges);
            } else {
                star_pass.draw(&mut rpass);
            }
            timers.end(&mut rpass, 1);
        }
        {
            // s008: HDR -> LDR display (exposure + clamp + sRGB OETF), slot 5.
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tonemap"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            timers.begin(&mut rpass, 5);
            tonemap.draw(&mut rpass);
            timers.end(&mut rpass, 5);
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
                .as_chunks::<4>().0.iter()
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
    let visible = octree_stats.as_ref().map(|s| s.drawn as usize).unwrap_or(src.stars.len());
    if let Some(s) = &octree_stats {
        tracing::info!(
            drawn = s.drawn,
            resident = s.resident_stars,
            leaves = s.resident_leaves,
            pending = s.pending,
            loads = s.loads,
            mb = s.mb_streamed,
            "octree scene final frame"
        );
    }
    let (frame_ms, frame_ms_source) = galaxia_render::select_frame_ms(
        last_gpu.frame_gpu_ms,
        last_gpu.per_pass.get(1).copied().flatten(),
        last_cpu_ms as f32,
    );
    // star_raster column: same plausibility range applied to the stars pass
    let (_, star_src) = galaxia_render::select_frame_ms(
        None,
        last_gpu.per_pass.get(1).copied().flatten(),
        last_cpu_ms as f32,
    );
    let stars_ms = (star_src == "gpu_stars")
        .then(|| last_gpu.per_pass.get(1).copied().flatten().unwrap_or_default());
    let fps = if frame_ms > 0.0 { 1000.0 / frame_ms } else { 0.0 };

    if cli.bench {
        // measurements.csv-compatible row (docs/build/measurements.csv header):
        // timestamp,session,iteration,commit,scene,stars_visible,star_raster_ms,
        // volumetrics_ms,glare_ms,post_ms,frame_ms_total,fps,peak_vram_gb,
        // resident_leaves,stream_mb_s,notes
        let now = humantime_now();
        let star_ms = stars_ms.map(|v| format!("{v:.3}")).unwrap_or_default();
        let frame_ms = format!("{frame_ms:.3}");
        let fps = format!("{fps:.1}");
        println!(
            "{now},{sess},0,{commit},{scene},{stars},{star_ms},,,,{frame_ms},{fps},,,,sandbox lavapipe software raster — NOT Titan RTX numbers; It.0 skeleton point stars",
            scene = cli.scene,
            stars = visible,
            sess = cli.session,
        );
    }
    tracing::info!(
        scene = %cli.scene,
        stars = src.stars.len(),
        cpu_ms = last_cpu_ms,
        gpu_frame_ms = last_gpu.frame_gpu_ms,
        frame_ms = frame_ms,
        frame_ms_source = frame_ms_source,
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


/// s009 diagnostic: can this backend execute a compute pass at all?
async fn probe_compute(gpu: &GpuContext) -> Result<()> {
    let probe_big = std::env::var("PROBE_BIG").is_ok();
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe"),
        size: if probe_big { 2560 * 1440 * 16 } else { 16 },
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe-read"),
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let with_uniform = std::env::var("PROBE_UNIFORM").is_ok();
    let shader = gpu.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("probe"),
        source: wgpu::ShaderSource::Wgsl(
            if with_uniform {
                r#"
                struct U { bg: vec3<f32>, inv_q: f32, width: f32, height: f32, f_px: f32, tiles_x: f32 };
                @group(0) @binding(0) var<uniform> u: U;
                @group(0) @binding(1) var<storage, read_write> acc: array<u32>;
                @compute @workgroup_size(1)
                fn main(@builtin(workgroup_id) wg: vec3<u32>) {
                    if (wg.x == 0u) { acc[0] = 512000u + u32(u.width); }
                }
                "#.into()
            } else {
                r#"
                @group(0) @binding(0) var<storage, read_write> acc: array<u32>;
                @compute @workgroup_size(1)
                fn main(@builtin(workgroup_id) wg: vec3<u32>) {
                    if (wg.x == 0u) { acc[0] = 512000u; }
                }
                "#.into()
            },
        ),
    });
    let ubuf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("probe-uniform"),
        size: 48,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut ub = [0u8; 48];
    ub[20..24].copy_from_slice(&2560.0f32.to_le_bytes());
    gpu.queue.write_buffer(&ubuf, 0, &ub);
    let ro_storage = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let bgl = gpu.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("probe-bgl"),
        entries: &(if with_uniform {
            vec![
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                ro_storage(1),
            ]
        } else {
            vec![ro_storage(0)]
        }),
    });
    let bg = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("probe-bg"),
        layout: &bgl,
        entries: &(if with_uniform {
            vec![
                wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: buf.as_entire_binding() },
            ]
        } else {
            vec![wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }]
        }),
    });
    let layout = gpu.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("probe-layout"),
        bind_group_layouts: &[Some(&bgl)],
        immediate_size: 0,
    });
    let pipe = gpu.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("probe"),
        layout: Some(&layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("probe"),
            timestamp_writes: None,
        });
        cp.set_pipeline(&pipe);
        cp.set_bind_group(0, &bg, &[]);
        cp.dispatch_workgroups(1, 1, 1);
    }
    enc.copy_buffer_to_buffer(&buf, 0, &read, 0, 16);
    gpu.queue.submit([enc.finish()]);
    let slice = read.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
    let view = slice.get_mapped_range()?;
    let v: &[u32] = bytemuck::cast_slice(&view);
    println!("PROBE_COMPUTE value={} (expect 512000)", v[0]);
    drop(view);
    read.unmap();
    Ok(())
}
