//! s009 — PLAN §7.3 tile rasteriser (ADR-0002): CPU binning + compute tile
//! pass + fixed-point resolve (ADR-0007).
//!
//! Photometry contract (mirrors stars.wgsl so the two paths are comparable):
//!   m_app   = mag + 5·log10(dist/d_sun)            (s003 camera-relative fix)
//!   gate    = m_app <= m0
//!   r_px    = clamp(10^(-0.2(m_app-5.5))·2.2, 0.8, 48)   (skeleton curve)
//!   L       = gate·clamp(10^(-0.4(m_app-6)), 0, 14)      (flux-ish radiance)
//!   k(θ)    = ((f²+ρ²)/f²)^{3/2}  — perspective-correct solid angle (§7.3
//!             [MUST]): a pixel at radial screen distance ρ subtends
//!             Ω_px ∝ cos³θ; scaling L by 1/cos³θ conserves the star's flux
//!             across the FOV and makes edge stars proportionally brighter.
//!   profile = (1-(ρ_disc/r)²)^1.6                     (same soft disc)
//!   deposit(pixel) = L·k·colour·profile(ρ_c)·coverage(pixel ∩ disc)
//! with coverage = exact circle/rect overlap AREA of the unit pixel square
//! (=> flux conservation by construction; anti-aliased edges; no shimmer).
//!
//! Accumulation: per-channel u32 fixed-point Q10 atomicAdd (ADR-0007),
//! resolved into the s008 HDR target by a fullscreen pass (÷1024 + background).

use bytemuck::{Pod, Zeroable};

/// 32 B, scalars only: WGSL `array<RasterStar>` stride must match Rust
/// exactly (a vec3 member would force align-16 stride 48 in WGSL).
/// k(theta) and white_mix are derived in-shader from l + cx/cy (mirrored).
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct RasterStar {
    pub cx: f32,
    pub cy: f32,
    pub r_px: f32,
    pub l: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub depth: f32,
}

/// Fixed-point scale of the u32 accumulator (ADR-0007): Q14 (Q10 lost
/// sub-0.0005 radiance fringe deposits; s009 parity measurement).
pub const ACCUM_Q: f32 = 16384.0;
pub const TILE_PX: u32 = 8;

/// Exact area of the intersection of the circle (radius `r`, centred at the
/// origin) with the axis-aligned rect [x0,x1]×[y0,y1], via the signed
/// quadrant integral `corner_area`. Mirror of `circle_rect_area` in
/// shaders/star_raster.wgsl — keep in lockstep (unit tests pin both against
/// numeric integration).
pub fn circle_rect_area(r: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> f32 {
    corner_area(r, x1, y1) - corner_area(r, x0, y1)
        - corner_area(r, x1, y0)
        + corner_area(r, x0, y0)
}

/// ∫∫_{[0,x]×[0,y]} 1_{|p|<=r} dp with sign handling for negative x/y.
fn corner_area(r: f32, x: f32, y: f32) -> f32 {
    if r <= 0.0 {
        return 0.0;
    }
    let sx = if x >= 0.0 { 1.0 } else { -1.0 };
    let sy = if y >= 0.0 { 1.0 } else { -1.0 };
    sx * sy * quadrant_area(r, x.abs(), y.abs())
}

/// Area of circle∩[0,x]×[0,y] for x,y >= 0. Both arguments saturate at r
/// (the circle lies inside |p| <= r, so extending beyond adds nothing).
fn quadrant_area(r: f32, x: f32, y: f32) -> f32 {
    let x = x.min(r);
    let y = y.min(r);
    let r2 = r * r;
    // ∫_0^x min(y, sqrt(r²-t²)) dt; the integrand switches at t = xc.
    let xc = (r2 - y * y).sqrt();
    if x <= xc {
        x * y
    } else {
        let rect = xc * y;
        let hi = x; // already <= r
        let integral = 0.5 * (hi * (r2 - hi * hi).sqrt() + r2 * (hi / r).asin())
            - 0.5 * (xc * (r2 - xc * xc).sqrt() + r2 * (xc / r).asin());
        rect + integral
    }
}

/// Coverage fraction of the unit pixel square centred at (px,py) by the
/// disc of radius r at the origin.
pub fn pixel_coverage(r: f32, px: f32, py: f32) -> f32 {
    circle_rect_area(r, px - 0.5, px + 0.5, py - 0.5, py + 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod raster_math_tests {
    use super::{circle_rect_area, pixel_coverage, quadrant_area};

    fn numeric_circle_rect(r: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> f32 {
        // fine grid integration as the reference
        let n = 400;
        let dx = (x1 as f64 - x0 as f64) / n as f64;
        let dy = (y1 as f64 - y0 as f64) / n as f64;
        let mut acc = 0.0f64; // f64: f32 accumulation drifts over 160k adds
        for i in 0..n {
            let x = x0 as f64 + (i as f64 + 0.5) * dx;
            for j in 0..n {
                let y = y0 as f64 + (j as f64 + 0.5) * dy;
                if x * x + y * y <= r as f64 * r as f64 {
                    acc += dx * dy;
                }
            }
        }
        acc as f32
    }

    #[test]
    fn quadrant_matches_numeric() {
        for (r, x, y) in [
            (3.0, 1.0, 1.0),
            (3.0, 5.0, 5.0),
            (3.0, 2.5, 4.0),
            (0.7, 0.4, 0.9),
            (12.0, 3.3, 2.2),
        ] {
            let closed = quadrant_area(r, x, y);
            let num = numeric_circle_rect(r, 0.0, x, 0.0, y);
            assert!(
                (closed - num).abs() < 1e-3 * num.max(1e-3),
                "r={r} x={x} y={y}: closed {closed} vs numeric {num}"
            );
        }
    }

    #[test]
    fn rect_area_matches_numeric_all_quadrants() {
        let r = 2.3;
        for (x0, x1, y0, y1) in [
            (-0.2, 0.7, -0.4, 0.6),
            (1.5, 2.5, -0.5, 0.5),
            (-3.0, -1.9, 0.2, 1.1),
            (-0.5, 0.5, 2.0, 3.0),
            (-5.0, 5.0, -5.0, 5.0),
        ] {
            let closed = circle_rect_area(r, x0, x1, y0, y1);
            let num = numeric_circle_rect_sub(r, x0, x1, y0, y1);
            // numeric grid has O(boundary-cell) bias; closed form is exact
            // (the full-disc box equals pi r^2 to f32)
            assert!(
                (closed - num).abs() < 2e-3f32.max(1e-3f32 * num),
                "box ({x0},{x1},{y0},{y1}): {closed} vs {num}"
            );
        }
    }

    fn numeric_circle_rect_sub(r: f32, x0: f32, x1: f32, y0: f32, y1: f32) -> f32 {
        let n = 500;
        let dx = (x1 as f64 - x0 as f64) / n as f64;
        let dy = (y1 as f64 - y0 as f64) / n as f64;
        let mut acc = 0.0f64;
        for i in 0..n {
            let x = x0 as f64 + (i as f64 + 0.5) * dx;
            for j in 0..n {
                let y = y0 as f64 + (j as f64 + 0.5) * dy;
                if x * x + y * y <= r as f64 * r as f64 {
                    acc += dx * dy;
                }
            }
        }
        acc as f32
    }

    #[test]
    fn coverage_conserves_disc_area() {
        // sum of unit-pixel coverages over a fine lattice == disc area pi r^2
        for r in [0.8f32, 2.5, 7.3, 24.0] {
            let mut sum = 0.0f64;
            let rr = (r + 1.0).ceil() as i32;
            for ix in -rr..=rr {
                for iy in -rr..=rr {
                    sum += pixel_coverage(r, ix as f32, iy as f32) as f64;
                }
            }
            let exact = std::f64::consts::PI * (r as f64) * (r as f64);
            let err = (sum - exact).abs() / exact;
            assert!(err < 0.01, "r={r}: flux err {err:.5} (sum {sum} vs {exact})");
        }
    }

    #[test]
    fn coverage_bounds_and_symmetry() {
        assert_eq!(pixel_coverage(2.0, 0.0, 0.0), 1.0);
        assert_eq!(pixel_coverage(2.0, 100.0, 0.0), 0.0);
        let a = pixel_coverage(3.1, 2.6, -1.3);
        let b = pixel_coverage(3.1, -2.6, 1.3);
        assert!((a - b).abs() < 1e-6);
    }
}


// ── CPU binning (s009 v1; a compute binning pass replaces this on the
// target per STATUS s009) ──────────────────────────────────────────────────

#[derive(Default)]
pub struct BinData {
    pub stars: Vec<RasterStar>,
    pub tile_off: Vec<u32>,
    pub tile_ids: Vec<u32>,
    pub drawn: u32,
    pub tiles_x: u32,
}

/// Project + photometry + 8x8 tile binning. Deterministic: input order in,
/// row-major tiles, star index ascending within a tile.
pub fn bin_stars(
    stars: &[(glam::DVec3, f32, [f32; 3])],
    view_proj: &glam::Mat4,
    width: u32,
    height: u32,
    m0: f32,
) -> BinData {
    let (w, h) = (width as f32, height as f32);
    let tiles_x = width.div_ceil(TILE_PX);
    let tiles_y = height.div_ceil(TILE_PX);
    let n_tiles = (tiles_x * tiles_y) as usize;

    let mut rs_stars: Vec<RasterStar> = Vec::new();
    let mut per_tile: Vec<Vec<u32>> = vec![Vec::new(); n_tiles];

    for (pos, mag, color) in stars {
        let p = *view_proj * glam::Vec4::new(pos.x as f32, pos.y as f32, pos.z as f32, 1.0);
        if p.w <= 0.0 {
            continue;
        }
        let dist = p.w.max(1e-4);
        let ndc = p.truncate() / p.w;
        if ndc.x.abs() > 1.25 || ndc.y.abs() > 1.25 {
            continue;
        }
        let sx = (ndc.x * 0.5 + 0.5) * w;
        let sy = (1.0 - (ndc.y * 0.5 + 0.5)) * h;
        let d_sun = pos.length().max(1e-4) as f32;
        let m_app = mag + 5.0 * (dist / d_sun).log2() * (std::f64::consts::LOG10_2 as f32);
        if m_app > m0 {
            continue;
        }
        let r_px = (10f32.powf(-0.20 * (m_app - 5.5)) * 2.2).clamp(0.8, 48.0);
        let lum = (10f32.powf(-0.4 * (m_app - 6.0))).clamp(0.0, 14.0);
        let idx = rs_stars.len() as u32;
        rs_stars.push(RasterStar {
            cx: sx,
            cy: sy,
            r_px,
            l: lum,
            r: color[0],
            g: color[1],
            b: color[2],
            depth: dist,
        });
        let m = r_px + 1.0;
        let tx0 = ((sx - m).max(0.0) as u32) / TILE_PX;
        let tx1 = ((sx + m).min(w - 1.0).max(0.0) as u32) / TILE_PX;
        let ty0 = ((sy - m).max(0.0) as u32) / TILE_PX;
        let ty1 = ((sy + m).min(h - 1.0).max(0.0) as u32) / TILE_PX;
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                per_tile[(ty * tiles_x + tx) as usize].push(idx);
            }
        }
    }

    let mut tile_off = Vec::with_capacity(n_tiles + 1);
    let mut tile_ids = Vec::new();
    let mut acc = 0u32;
    tile_off.push(0);
    for t in &per_tile {
        tile_ids.extend_from_slice(t);
        acc += t.len() as u32;
        tile_off.push(acc);
    }
    BinData {
        stars: rs_stars,
        tile_off,
        tile_ids,
        drawn: acc,
        tiles_x,
    }
}

// ── GPU pass ───────────────────────────────────────────────────────────────

const CAP_STARS: usize = 3_200_000;
const CAP_IDS: usize = 12_000_000;

pub struct StarRasterPass {
    /// Owns the scalar storage buffer for the bind groups' lifetime.
    #[allow(dead_code)]
    uniform: wgpu::Buffer,
    compute: wgpu::ComputePipeline,
    resolve: wgpu::RenderPipeline,
    bg_compute: wgpu::BindGroup,
    bg_resolve: wgpu::BindGroup,
    star_buf: wgpu::Buffer,
    off_buf: wgpu::Buffer,
    ids_buf: wgpu::Buffer,
    accum: wgpu::Buffer,
    width: u32,
    height: u32,
}

impl StarRasterPass {
    pub fn new(
        device: &std::sync::Arc<wgpu::Device>,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        bg: [f32; 3],
        fov_y_rad: f32,
        hdr_format: wgpu::TextureFormat,
    ) -> anyhow::Result<Self> {
        let n_tiles = width.div_ceil(TILE_PX) * height.div_ceil(TILE_PX);
        let f_px = (height as f32 * 0.5) / (fov_y_rad * 0.5).tan();
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("raster-scalars"),
            size: 48,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mk = |cap: u64, label: &str| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: cap,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let star_buf = mk(CAP_STARS as u64 * 32, "raster-stars");
        // GL SSBO binding sizes must stay 16 B-aligned (s009 find: an
        // unaligned tile_off size silently no-opped the whole compute pass
        // on wgpu30-GLES/llvmpipe)
        let off_buf = mk(((n_tiles as u64 + 1) * 4 + 15) & !15, "raster-tile-off");
        let ids_buf = mk((CAP_IDS as u64 * 4 + 15) & !15, "raster-tile-ids");
        let accum = mk(width as u64 * height as u64 * 16, "raster-accum");
        // scalar array: bg rgb, inv_q, width, height, f_px, tiles_x
        let scalars = [
            bg[0], bg[1], bg[2], 1.0 / ACCUM_Q,
            width as f32, height as f32, f_px, width.div_ceil(TILE_PX) as f32,
            ACCUM_Q, 0.0, 0.0, 0.0,
        ];
        queue.write_buffer(&uniform, 0, bytemuck::cast_slice(&scalars));

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("star_raster"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/star_raster.wgsl").into(),
            ),
        });
        let buf_ro = |binding: u32, vis: wgpu::ShaderStages| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vis,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let bgl_c = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("raster-bgl-compute"),
            entries: &[
                buf_ro(0, wgpu::ShaderStages::COMPUTE),
                buf_ro(1, wgpu::ShaderStages::COMPUTE),
                buf_ro(2, wgpu::ShaderStages::COMPUTE),
                buf_ro(3, wgpu::ShaderStages::COMPUTE),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bg_compute = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("raster-bg-compute"),
            layout: &bgl_c,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: star_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: off_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: ids_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: accum.as_entire_binding() },
            ],
        });
        let layout_c = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("raster-layout-compute"),
            bind_group_layouts: &[Some(&bgl_c)],
            immediate_size: 0,
        });
        let compute = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("star-raster"),
            layout: Some(&layout_c),
            module: &shader,
            entry_point: Some("cs_tile"),
            compilation_options: Default::default(),
            cache: None,
        });
        let bgl_r = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("raster-bgl-resolve"),
            entries: &[
                buf_ro(0, wgpu::ShaderStages::FRAGMENT),
                buf_ro(1, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let bg_resolve = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("raster-bg-resolve"),
            layout: &bgl_r,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: accum.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: uniform.as_entire_binding() },
            ],
        });
        let layout_r = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("raster-layout-resolve"),
            bind_group_layouts: &[Some(&bgl_r)],
            immediate_size: 0,
        });
        let shader_r = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("star_raster_resolve"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/star_raster_resolve.wgsl").into(),
            ),
        });
        let resolve = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("raster-resolve"),
            layout: Some(&layout_r),
            vertex: wgpu::VertexState {
                module: &shader_r,
                entry_point: Some("vs_resolve"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader_r,
                entry_point: Some("fs_resolve"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: hdr_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            compute,
            resolve,
            bg_compute,
            bg_resolve,
            uniform,
            star_buf,
            off_buf,
            ids_buf,
            accum,
            width,
            height,
        })
    }

    /// Encode one frame: upload bins, clear accum, dispatch tiles, resolve
    /// into `hdr_view` (background radiance added in the resolve shader).
    pub fn encode(
        &self,
        device: &std::sync::Arc<wgpu::Device>,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        bin: &BinData,
        hdr_view: &wgpu::TextureView,
    ) -> anyhow::Result<()> {
        if bin.stars.len() > CAP_STARS || bin.tile_ids.len() > CAP_IDS {
            anyhow::bail!(
                "raster bin exceeds capacities ({} stars, {} ids)",
                bin.stars.len(),
                bin.tile_ids.len()
            );
        }
        queue.write_buffer(&self.star_buf, 0, bytemuck::cast_slice(&bin.stars));
        queue.write_buffer(&self.off_buf, 0, bytemuck::cast_slice(&bin.tile_off));
        queue.write_buffer(&self.ids_buf, 0, bytemuck::cast_slice(&bin.tile_ids));
        // DIAG J: compute in its OWN encoder+submit (probe worked that way)
        let mut enc2 = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("raster-compute-enc"),
        });
        if std::env::var("RASTER_NO_CLEAR").is_err() {
            enc2.clear_buffer(&self.accum, 0, None);
        }
        {
            let mut cpass = enc2.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("star-raster"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute);
            cpass.set_bind_group(0, &self.bg_compute, &[]);
            let tiles = self.width.div_ceil(TILE_PX) * self.height.div_ceil(TILE_PX);
            cpass.dispatch_workgroups(tiles, 1, 1);
        }
        queue.submit([enc2.finish()]);
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("raster-resolve"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: hdr_view,
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
            rpass.set_pipeline(&self.resolve);
            rpass.set_bind_group(0, &self.bg_resolve, &[]);
            rpass.draw(0..3, 0..1);
        }
        Ok(())
    }
}

/// Mirror of the shader deposit (star_raster.wgsl cs_tile) for tests:
/// radiance deposited at pixel centre (px,py) by one star.
#[cfg(test)]
fn deposit_at(
    star: &RasterStar,
    px: f32,
    py: f32,
    width: f32,
    height: f32,
    f_px: f32,
) -> [f32; 3] {
    let dx = px + 0.5 - star.cx;
    let dy = py + 0.5 - star.cy;
    let rho2 = dx * dx + dy * dy;
    let rr = star.r_px + 1.5;
    if rho2 >= rr * rr {
        return [0.0; 3];
    }
    let cov = pixel_coverage(star.r_px, dx, dy);
    if cov <= 0.0 {
        return [0.0; 3];
    }
    let mut prof = 0.0f32;
    for oy in 0..2 {
        for ox in 0..2 {
            let sxo = dx + ox as f32 * 0.5 - 0.25;
            let syo = dy + oy as f32 * 0.5 - 0.25;
            let ar = circle_rect_area(
                star.r_px, sxo - 0.25, sxo + 0.25, syo - 0.25, syo + 0.25);
            if ar > 0.0 {
                let qs = (sxo * sxo + syo * syo) / (star.r_px * star.r_px);
                prof += (1.0 - qs).max(0.0).powf(1.6) * ar;
            }
        }
    }
    let sx = star.cx - width * 0.5;
    let sy = star.cy - height * 0.5;
    let f2 = f_px * f_px;
    let k = ((f2 + sx * sx + sy * sy) / f2).powf(1.5);
    [star.l * k * prof * cov * star.r, star.l * k * prof * cov * star.g, star.l * k * prof * cov * star.b]
}

/// Solid-angle weight of a pixel at screen radius rho (pinhole): cos^3(theta).
#[cfg(test)]
fn omega_weight(px: f32, py: f32, width: f32, height: f32, f_px: f32) -> f32 {
    let sx = px + 0.5 - width * 0.5;
    let sy = py + 0.5 - height * 0.5;
    let f2 = f_px * f_px;
    (f2 / (f2 + sx * sx + sy * sy)).powf(1.5)
}

#[cfg(test)]
mod flux_tests {
    use super::{deposit_at, omega_weight, RasterStar};

    fn star_at(cx: f32, cy: f32, r: f32) -> RasterStar {
        RasterStar { cx, cy, r_px: r, l: 1.0, r: 1.0, g: 1.0, b: 1.0, depth: 1.0 }
    }

    fn total_flux(s: &RasterStar, w: f32, h: f32, f: f32) -> f64 {
        let mut sum = 0.0f64;
        let rr = (s.r_px + 2.0).ceil() as i32;
        let x0 = s.cx as i32 - rr;
        let x1 = s.cx as i32 + rr;
        let y0 = s.cy as i32 - rr;
        let y1 = s.cy as i32 + rr;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = deposit_at(s, x as f32, y as f32, w, h, f);
                sum += d[0] as f64 * omega_weight(x as f32, y as f32, w, h, f) as f64;
            }
        }
        sum
    }

    /// PLAN §17 / s009 gate: single-star flux equals the analytic profile
    /// integral (pi r^2 / 5.2 for (1-q)^1.6) and is POSITION-INDEPENDENT
    /// (the §7.3 perspective-correct solid-angle MUST): centre vs extreme
    /// FOV corner within 1%.
    #[test]
    fn flux_conserved_across_fov() {
        let (w, h, f) = (2560.0, 1440.0, 1440.0);
        let analytic = std::f64::consts::PI * 6.5f64 * 6.5f64 / 2.6; // 2pi r^2 int_0^1 t(1-t^2)^1.6 dt = pi r^2/2.6
        let centre = total_flux(&star_at(w * 0.5, h * 0.5, 6.5), w, h, f);
        let corner = total_flux(&star_at(w * 0.93, h * 0.90, 6.5), w, h, f);
        let edge = total_flux(&star_at(w * 0.06, h * 0.5, 6.5), w, h, f);
        for (name, v) in [("centre", centre), ("corner", corner), ("edge", edge)] {
            let err = (v - analytic).abs() / analytic;
            assert!(err < 0.01, "{name}: flux {v} vs analytic {analytic} err {err:.5}");
        }
        // position independence directly
        let spread = (centre - corner).abs() / analytic;
        assert!(spread < 0.01, "centre vs corner spread {spread:.5}");
        let _ = edge;
    }
}
