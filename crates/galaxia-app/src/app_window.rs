//! Windowed walking skeleton: winit event loop, wgpu surface, egui debug
//! overlay (ADR-0003: egui is the intentional prototype shortcut — diegetic
//! UI is wave W2), WGSL hot-reload, GPU pass timings.

use crate::camera::Camera;
use crate::input::{self, Action, MoveState, LOOK_SENS, SPEED_STEP};
use crate::octree_scene::{FrameStats, OctreeScene};
use crate::star_data;
use crate::Cli;
use anyhow::Result;
use galaxia_render::star_pass::FrameUniforms;
use galaxia_render::timing::GpuTimers;
use galaxia_render::{TonemapPass, FrameGraph, GpuContext, PassTimings, ShaderWatcher, StarPass};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

pub const FOV_Y_RAD: f32 = 60f32.to_radians();
/// Derived near plane placeholder (pc). It.3 derives it from the frame tree (§5.4).
const NEAR_PC: f32 = 1e-5;

pub async fn run(cli: &Cli) -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(cli.clone()).await?;
    event_loop
        .run_app(&mut app)
        .map_err(|e| anyhow::anyhow!("event loop: {e}"))
}

struct App {
    cli: Cli,
    gpu: GpuContext,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    star_pass: StarPass,
    shader_watcher: ShaderWatcher,
    graph: FrameGraph,
    timers: GpuTimers,
    ts_period_ns: f32,
    timings: PassTimings,
    camera: Camera,
    move_state: MoveState,
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    look_accum: (f64, f64),
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    egui_renderer: Option<egui_wgpu::Renderer>,
    // s008 HDR chain (windowed): hdr radiance target sized to the surface,
    // recreated on resize; tonemap pass built once the surface format is known.
    hdr: Option<(wgpu::Texture, wgpu::TextureView, wgpu::TextureFormat, u32, u32)>,
    tonemap: Option<TonemapPass>,
    raster: Option<galaxia_render::star_raster::StarRasterPass>,
    bin_input: Vec<(glam::DVec3, f32, [f32; 3])>,
    bin_src: Vec<(glam::DVec3, f32, [f32; 3])>,
    show_overlay: bool,
    star_count: usize,
    provenance: String,
    fps_ema: f64,
    frame_ms_ema: f64,
    last_frame: Instant,
    reloads: u32,
    reload_errors: Vec<String>,
    frames: u64,
    m0_uniform: f32,
    /// It.1: t0_* scenes stream from the baked octree (None → CSV star path).
    octree: Option<OctreeScene>,
    osc_stats: Option<FrameStats>,
    osc_ranges: Vec<(u32, u32)>,
}

impl App {
    async fn new(cli: Cli) -> Result<Self> {
        let gpu = GpuContext::new(true).await?;

        let mut watcher = ShaderWatcher::new(&cli.shader)?;
        let source = watcher
            .changed()
            .unwrap_or_else(|| std::fs::read_to_string(&cli.shader).unwrap());
        let hdr_format = if gpu.hdr_capable {
            galaxia_render::star_pass::HDR_FORMAT
        } else {
            galaxia_render::star_pass::HDR_FALLBACK_FORMAT
        };
        let mut star_pass = StarPass::new(&gpu.device, &source, hdr_format)?;

        let src = star_data::load_stars(cli.stars_csv.as_deref(), &cli.scene);
        tracing::info!(stars = src.stars.len(), provenance = %src.provenance, "star source");
        let gpu_stars: Vec<_> = src.stars.iter().map(|s| s.to_gpu()).collect();
        star_pass.set_stars(&gpu.queue, &gpu_stars);
        let star_count = src.stars.len();
        let provenance = src.provenance.clone();

        let graph = FrameGraph::new();
        let timers = GpuTimers::new(&gpu.device, gpu.has_timestamps, graph.slot_count());
        let ts_period_ns = gpu.queue.get_timestamp_period();

        let m0_uniform = if cli.scene.starts_with("t0") { cli.m0 } else { 99.0 };
        // It.1 octree wiring (mirrors headless.rs): t0_* scenes stream leaves
        // from etl/build/t0 through the resident set; anything else keeps the
        // It.0 CSV path. Missing artifact = loud warning, not a hard failure —
        // the window still opens on CSV stars (never brick the app for data).
        let mut octree = None;
        if cli.scene.starts_with("t0") {
            if std::path::Path::new("etl/build/t0/tree_meta.json").exists() {
                match galaxia_data::T0Tree::load(std::path::Path::new("etl/build/t0"))
                    .and_then(|tree| {
                        star_pass.ensure_capacity(cli.resident_cap);
                        OctreeScene::new(tree, cli.resident_cap)
                    }) {
                    Ok(scene) => {
                        tracing::info!(cap = cli.resident_cap, "T0 octree streaming active");
                        octree = Some(scene);
                    }
                    Err(e) => tracing::error!("T0 octree init failed ({e}); CSV stars only"),
                }
            } else {
                tracing::warn!("t0 scene requested but etl/build/t0 is missing — CSV stars only");
            }
        }
        Ok(Self {
            camera: crate::headless::camera_for_scene(&cli.scene),
            cli,
            gpu,
            window: None,
            surface: None,
            surface_config: None,
            star_pass,
            shader_watcher: watcher,
            graph,
            timers,
            ts_period_ns,
            timings: PassTimings::default(),
            move_state: MoveState::new(),
            dragging: false,
            last_cursor: None,
            look_accum: (0.0, 0.0),
            egui_ctx: egui::Context::default(),
            egui_state: None,
            egui_renderer: None,
            hdr: None,
            tonemap: None,
            raster: None,
            bin_input: Vec::new(),
            bin_src: src.stars.iter().map(|r| (glam::DVec3::from(r.pos_pc), r.mag, r.color)).collect(),
            show_overlay: true,
            star_count,
            provenance,
            fps_ema: 0.0,
            frame_ms_ema: 0.0,
            last_frame: Instant::now(),
            reloads: 0,
            reload_errors: Vec::new(),
            frames: 0,
            m0_uniform,
            octree,
            osc_stats: None,
            osc_ranges: Vec::new(),
        })
    }

    fn init_window(&mut self, el: &ActiveEventLoop) -> Result<()> {
        #[allow(deprecated)]
        let attrs = Window::default_attributes()
            .with_title("galaxia — PROJECT MILKY WAY — Iteration 0 skeleton")
            .with_inner_size(winit::dpi::PhysicalSize::new(1600, 900));
        let window = Arc::new(el.create_window(attrs)?);

        let size = window.inner_size();
        let surface = self.gpu.instance.create_surface(Arc::clone(&window))?;
        let mut config = surface
            .get_default_config(&self.gpu.adapter, size.width.max(2), size.height.max(2))
            .ok_or_else(|| anyhow::anyhow!("surface unsupported on this adapter"))?;
        // s008: prefer a NON-srgb surface format — the tonemap pass applies the
        // sRGB OETF in-shader (and egui writes sRGB-encoded colours directly).
        {
            let caps = surface.get_capabilities(&self.gpu.adapter);
            let preferred = [
                galaxia_render::star_pass::LDR_FORMAT,
                wgpu::TextureFormat::Rgba8Unorm,
            ];
            if let Some(f) = preferred.iter().find(|f| caps.formats.contains(f)) {
                config.format = *f;
            } else {
                tracing::warn!(
                    surface_format = ?config.format,
                    "no non-srgb surface format available; tonemap will double-encode"
                );
            }
        }
        // The skeleton pipeline was built for Bgra8UnormSrgb; if the surface
        // prefers something else, keep its preference and rebuild the pipeline
        // format via view_formats — for It.0 we simply require the format.
        if config.format != galaxia_render::star_pass::PIPELINE_FORMAT {
            tracing::warn!(
                surface_format = ?config.format,
                "surface format differs from skeleton PIPELINE_FORMAT; keeping surface format"
            );
        }
        config.present_mode = wgpu::PresentMode::AutoNoVsync;
        surface.configure(&self.gpu.device, &config);

        let ppp = window.scale_factor() as f32;
        self.egui_ctx.set_pixels_per_point(ppp);
        let mut state = egui_winit::State::new(
            self.egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(ppp),
            None,
            None,
        );
        state.set_max_texture_side(8192);
        self.egui_state = Some(state);
        self.egui_renderer = Some(egui_wgpu::Renderer::new(
            &self.gpu.device,
            config.format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                ..Default::default()
            },
        ));

        self.window = Some(window);
        self.surface = Some(surface);
        self.surface_config = Some(config);
        Ok(())
    }

    fn handle_action(&mut self, a: Action, el: &ActiveEventLoop) {
        self.move_state.apply(a);
        match a {
            Action::SpeedUp => self.camera.speed_exp = (self.camera.speed_exp + SPEED_STEP).min(14.0),
            Action::SpeedDown => self.camera.speed_exp = (self.camera.speed_exp - SPEED_STEP).max(-4.0),
            Action::ToggleOverlay => self.show_overlay = !self.show_overlay,
            Action::Quit => el.exit(),
            _ => {}
        }
    }

    fn render(&mut self) {
        let (Some(window), Some(surface), Some(config)) =
            (self.window.clone(), self.surface.as_ref(), self.surface_config.clone())
        else {
            return;
        };
        let frame_t0 = Instant::now();
        use wgpu::CurrentSurfaceTexture as Cur;
        let frame = match surface.get_current_texture() {
            Cur::Success(t) | Cur::Suboptimal(t) => t,
            Cur::Timeout | Cur::Occluded => return,
            Cur::Outdated | Cur::Lost | Cur::Validation => {
                let size = window.inner_size();
                let mut c = config.clone();
                c.width = size.width.max(2);
                c.height = size.height.max(2);
                surface.configure(&self.gpu.device, &c);
                self.surface_config = Some(c);
                return;
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let size = window.inner_size();
        let (w, h) = (size.width.max(2), size.height.max(2));

        // s008 HDR chain: radiance target sized to the surface (recreated on
        // resize); tonemap pass rebuilt with it (binds the hdr view).
        if self.hdr.as_ref().map(|t| (t.3, t.4)) != Some((w, h)) {
            use galaxia_render::star_pass::{HDR_FALLBACK_FORMAT, HDR_FORMAT};
            let format = if self.gpu.hdr_capable { HDR_FORMAT } else { HDR_FALLBACK_FORMAT };
            let tex = self.gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("window-hdr"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let hview = tex.create_view(&wgpu::TextureViewDescriptor::default());
            let surface_format = self
                .surface_config
                .as_ref()
                .map(|c| c.format)
                .unwrap_or(galaxia_render::star_pass::LDR_FORMAT);
            let mut tm = match TonemapPass::new(&self.gpu.device, format, surface_format) {
                Ok(tm) => tm,
                Err(e) => {
                    tracing::error!(error = %e, "tonemap pass creation failed; skipping frame");
                    return;
                }
            };
            tm.bind(&self.gpu.device, &hview);
            self.tonemap = Some(tm);
            self.hdr = Some((tex, hview, format, w, h));
        }
        self.tonemap
            .as_mut()
            .unwrap()
            .set_exposure(&self.gpu.queue, self.cli.exposure);

        // ── camera integrate ────────────────────────────────────────────────
        let dt = self.last_frame.elapsed().as_secs_f64().clamp(1e-4, 0.25);
        self.last_frame = Instant::now();
        let look = std::mem::take(&mut self.look_accum);
        let axis = self.move_state.axis();
        self.camera.update(dt, axis, look);

        // frame-rate EMAs for the overlay
        let frame_ms = dt * 1e3;
        let k = 0.08;
        self.frame_ms_ema += (frame_ms - self.frame_ms_ema) * k;
        let fps = 1000.0 / self.frame_ms_ema.max(1e-3);
        self.fps_ema += (fps - self.fps_ema) * k;

        // ── WGSL hot-reload (validate-before-swap; a typo never kills the app) ──
        if let Some(new_src) = self.shader_watcher.changed() {
            match self.star_pass.try_rebuild(&new_src) {
                Ok(()) => {
                    self.reloads += 1;
                    self.reload_errors.clear();
                }
                Err(e) => {
                    let msg = format!("{}", e).chars().take(300).collect::<String>();
                    tracing::warn!("WGSL reload rejected: {msg}");
                    self.reload_errors.push(msg);
                    if self.reload_errors.len() > 3 {
                        self.reload_errors.remove(0);
                    }
                }
            }
        }

        // ── uniforms ────────────────────────────────────────────────────────
        let aspect = w as f32 / h as f32;
        let px_scale = 2.0 * (FOV_Y_RAD * 0.5).tan() / h as f32;
        let (right, up) = self.camera.basis_f32();
        let uniforms = FrameUniforms {
            view_proj: self.camera.view_proj_f32(aspect, FOV_Y_RAD, NEAR_PC).to_cols_array_2d(),
            cam_right: right,
            px_scale,
            cam_up: up,
            time_s: frame_t0.elapsed().as_secs_f32(),
            exposure: self.cli.exposure,
            m0: self.m0_uniform,
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
            _pad4: 0.0,
            _pad5: 0.0,
            _pad6: 0.0,
        };
        self.star_pass.write_uniforms(&self.gpu.queue, &uniforms);

        // ── egui input → output → tessellation ──────────────────────────────
        let (clipped, platform_output) = if self.show_overlay {
            let state = self.egui_state.as_mut().unwrap();
            let raw_input = state.take_egui_input(&window);
            let ctx = self.egui_ctx.clone();
            let output = ctx.run_ui(raw_input, |ui| self.overlay_ui(ui));
            let ppp = output.pixels_per_point;
            let clipped = ctx.tessellate(output.shapes, ppp);
            (clipped, output.platform_output)
        } else {
            (Vec::new(), Default::default())
        };
        if let Some(state) = self.egui_state.as_mut() {
            state.handle_platform_output(&window, platform_output);
        }
        let screen_desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [w, h],
            pixels_per_point: self.egui_ctx.pixels_per_point(),
        };

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });

        let mut egui_paint_cmds = Vec::new();
        if self.show_overlay {
            let renderer = self.egui_renderer.as_mut().unwrap();
            if let Some(state) = self.egui_state.as_ref() {
                let _ = state; // textures handled below via output deltas
            }
            egui_paint_cmds = renderer.update_buffers(
                &self.gpu.device,
                &self.gpu.queue,
                &mut encoder,
                &clipped,
                &screen_desc,
            );
        }

        {
            let load = wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: 0.002,
                    g: 0.003,
                    b: 0.010,
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            };
            let hdr_view: &wgpu::TextureView = &self.hdr.as_ref().unwrap().1;
            if self.cli.star_renderer == "raster" {
                // s009 §7.3 rasteriser path (windowed; compile-verified here)
                if self.raster.is_none() {
                    let format = self.hdr.as_ref().unwrap().2;
                    match galaxia_render::star_raster::StarRasterPass::new(
                        &self.gpu.device,
                        &self.gpu.queue,
                        w,
                        h,
                        [0.002, 0.003, 0.010],
                        FOV_Y_RAD,
                        format,
                    ) {
                        Ok(r) => self.raster = Some(r),
                        Err(e) => {
                            tracing::error!(error = %e, "raster pass init failed");
                            return;
                        }
                    }
                }
                let vp = glam::Mat4::from_cols_array_2d(&uniforms.view_proj);
                let m0_bin = if self.octree.is_some() { self.cli.m0 } else { 99.0 };
                if let Some(osc) = self.octree.as_mut() {
                    osc.collect_stars(&mut self.bin_input);
                } else {
                    self.bin_input.clear();
                    self.bin_input.extend(self.bin_src.iter().copied());
                }
                let bin = galaxia_render::star_raster::bin_stars(
                    &self.bin_input, &vp, w, h, m0_bin,
                );
                if let Err(e) = self.raster.as_ref().unwrap().encode(
                    &self.gpu.device, &self.gpu.queue, &mut encoder, &bin, hdr_view,
                ) {
                    tracing::error!(error = %e, "raster encode failed");
                    return;
                }
            } else {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: hdr_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: load,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.timers.begin(&mut rpass, 0);
            self.timers.end(&mut rpass, 0);
            self.timers.begin(&mut rpass, 1);
            if let Some(osc) = self.octree.as_mut() {
                // It.1 streaming path: cull → reconcile → ingest → draw ranges
                // (identical to headless.rs; the overlay shows FrameStats).
                let vp = glam::Mat4::from_cols_array_2d(&uniforms.view_proj);
                let stats = osc.frame(
                    self.camera.pos_pc,
                    &vp,
                    self.cli.m0,
                    &mut self.star_pass,
                    &self.gpu.queue,
                );
                self.osc_ranges.clear();
                self.osc_ranges.extend_from_slice(osc.draw_ranges());
                self.osc_stats = Some(stats);
                self.star_pass.draw_ranges(&mut rpass, &self.osc_ranges);
            } else {
                self.star_pass.draw(&mut rpass);
            }
            self.timers.end(&mut rpass, 1);
        }
            }
        {
            // s008: HDR -> surface (exposure + clamp + sRGB OETF), slot 5.
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tonemap"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
            self.timers.begin(&mut rpass, 5);
            self.tonemap.as_ref().unwrap().draw(&mut rpass);
            self.timers.end(&mut rpass, 5);
        }
        if self.show_overlay {
            // egui writes sRGB-encoded colours directly: correct on the
            // non-srgb surface format chosen at configure time (s008).
            let rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // egui-wgpu 0.37 wants a 'static pass borrow (same trick as pre-s008)
            let mut rpass = rpass.forget_lifetime();
            self.timers.begin(&mut rpass, 6);
            self.egui_renderer
                .as_ref()
                .unwrap()
                .render(&mut rpass, &clipped, &screen_desc);
            self.timers.end(&mut rpass, 6);
        }
        let did_resolve = self.timers.resolve(&mut encoder);

        self.gpu.queue.submit(egui_paint_cmds.into_iter().chain([encoder.finish()]));
        self.gpu.queue.present(frame);
        self.frames += 1;
        self.timers.begin_readback(did_resolve);

        if let Some(raw) = self.timers.poll_results(&self.gpu.device) {
            self.timings = self.graph.fold_timings(&raw, self.ts_period_ns);
        }
    }

    fn overlay_ui(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        egui::Area::new(egui::Id::NULL)
            .fixed_pos([10.0, 10.0])
            .show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(330.0);
                ui.heading(format!("{:.1} fps", self.fps_ema));
                ui.label(format!(
                    "frame {:.2} ms (CPU-side EMA)  ·  GPU frame {:.2}",
                    self.frame_ms_ema,
                    self.timings
                        .frame_gpu_ms
                        .map(|v| format!("{v:.2} ms"))
                        .unwrap_or_else(|| "n/a".into())
                ));
                ui.separator();
                ui.label(egui::RichText::new("per-pass GPU (ms)").strong());
                for (i, p) in self.graph.passes.iter().enumerate() {
                    let t = self
                        .timings
                        .per_pass
                        .get(i)
                        .copied()
                        .flatten()
                        .map(|v| format!("{v:8.3}"))
                        .unwrap_or_else(|| "    —   ".into());
                    let mark = if p.enabled { "▶" } else { "○" };
                    ui.monospace(format!(
                        "{mark} {:<16} {t}  ({})",
                        p.name, p.planned_in
                    ));
                }
                if !self.gpu.has_timestamps {
                    ui.colored_label(egui::Color32::YELLOW, "GPU timestamps unavailable — CPU only");
                }
                ui.separator();
                ui.label(format!("stars: {}", self.star_count));
                ui.label(format!("source: {}", self.provenance));
                ui.label(format!("scene: {}", self.cli.scene));
                if let Some(s) = &self.osc_stats {
                    ui.separator();
                    ui.label(egui::RichText::new("T0 streaming (It.1)").strong());
                    ui.monospace(format!(
                        "leaves desired/resident {:>4} / {:<4}",
                        s.desired_leaves, s.resident_leaves
                    ));
                    ui.monospace(format!(
                        "stars resident/drawn {:>9} / {:<9}",
                        s.resident_stars, s.drawn
                    ));
                    ui.monospace(format!(
                        "pending {} · deferred {} · loads {} · evictions {}",
                        s.pending, s.deferred, s.loads, s.evictions
                    ));
                    ui.monospace(format!("streamed {:.1} MB", s.mb_streamed));
                }
                ui.label(format!(
                    "cam pos (pc): [{:.3}, {:.3}, {:.3}]",
                    self.camera.pos_pc.x, self.camera.pos_pc.y, self.camera.pos_pc.z
                ));
                ui.label(format!(
                    "speed: 1e{:.1} AU/s = {:.3} pc/s   [scroll to change]",
                    self.camera.speed_exp,
                    self.camera.speed_pc_s()
                ));
                ui.label(format!(
                    "yaw {:.1}°  pitch {:.1}°",
                    (-self.camera.yaw).to_degrees(),
                    self.camera.pitch.to_degrees()
                ));
                ui.separator();
                ui.label(format!(
                    "adapter: {}", self.gpu.adapter_info
                ));
                ui.label(format!("shader reloads: {} ({} — hot)", self.reloads, self.cli.shader.display()));
                for e in &self.reload_errors {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("reload error: {e}"));
                }
                ui.separator();
                ui.small(
                    "WASD fly · Space/Shift up/down · drag mouse look · scroll speed · ` overlay · Q quit",
                );
                    ui.small(galaxia_core::IT0_SINGLE_FRAME_SHORTCUT);
                });
            });
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_none()
            && let Err(e) = self.init_window(el) {
                tracing::error!("window init failed: {e}");
                el.exit();
            }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, ev: WindowEvent) {
        if let Some(state) = self.egui_state.as_mut() {
            let window = self.window.as_ref().unwrap();
            let resp = state.on_window_event(window, &ev);
            if resp.consumed {
                return;
            }
        }
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0
                    && let (Some(surface), Some(mut config)) =
                        (self.surface.as_ref(), self.surface_config.clone())
                    {
                        config.width = size.width;
                        config.height = size.height;
                        surface.configure(&self.gpu.device, &config);
                        self.surface_config = Some(config);
                    }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let Some(a) = input::map_key_event(&event) {
                    self.handle_action(a, el);
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.dragging = state == ElementState::Pressed;
                if !self.dragging {
                    self.last_cursor = None;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if self.dragging {
                    let (x, y) = (position.x, position.y);
                    if let Some((px, py)) = self.last_cursor {
                        self.look_accum.0 += (x - px) * LOOK_SENS;
                        self.look_accum.1 += -((py - y) * LOOK_SENS);
                    }
                    self.last_cursor = Some((x, y));
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(a) = input::map_scroll(&delta) {
                    self.handle_action(a, el);
                }
            }
            WindowEvent::RedrawRequested => self.render(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
        el.set_control_flow(ControlFlow::Poll);
    }
}
