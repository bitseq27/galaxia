//! s008 tonemap pass (ADR-0002 prerequisite, PLAN §6.2 pass 16): HDR float
//! radiance -> LDR display bytes. Fullscreen triangle, one texel-to-pixel
//! load, exposure multiply, clamp, sRGB OETF in-shader (the LDR target is a
//! NON-srgb format, so there is no hardware double-encode).
//!
//! Curve choice (recorded in BUILDLOG s008): clamp + OETF, i.e. photometric
//! parity with the pre-s008 hardware path (blend-linear -> clamp -> hw
//! encode). Filmic knees (extended Reinhard/ACES) belong with the glare/
//! exposure work later in It.2; adopting one now would break the golden-shot
//! parity gate that makes this refactor verifiable. The float target still
//! carries >1 headroom for the rasteriser and glare pyramid to use.

use anyhow::Result;
use std::sync::Arc;

/// sRGB OETF (IEC 61966-2-1) — mirror of `srgb_oetf` in shaders/tonemap.wgsl.
/// Keep the two in lockstep; unit tests pin the shared constants.
pub fn srgb_oetf(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c > 0.0031308 {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    } else {
        12.92 * c
    }
}

/// Exposure + clamp + OETF on one channel — the scalar form of the shader body.
pub fn tonemap_channel(radiance: f32, exposure: f32) -> f32 {
    srgb_oetf((radiance * exposure).clamp(0.0, 1.0))
}


pub struct TonemapPass {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    bind_group: Option<wgpu::BindGroup>,
    uniform: wgpu::Buffer,
    exposure: f32,
}

impl TonemapPass {
    pub fn new(
        device: &Arc<wgpu::Device>,
        _hdr_format: wgpu::TextureFormat,
        ldr_format: wgpu::TextureFormat,
    ) -> Result<Self> {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tonemap-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tonemap-uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tonemap"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../../../shaders/tonemap.wgsl").into(),
            ),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tonemap-layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tonemap"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: ldr_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            pipeline,
            bgl,
            bind_group: None,
            uniform,
            exposure: 1.0,
        })
    }

    /// (Re)bind the HDR source view (after target creation/resize).
    pub fn bind(&mut self, device: &Arc<wgpu::Device>, hdr_view: &wgpu::TextureView) {
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(hdr_view) },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.uniform.as_entire_binding(),
                },
            ],
        }));
    }

    pub fn set_exposure(&mut self, queue: &wgpu::Queue, exposure: f32) {
        self.exposure = exposure;
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&[exposure, 0.0f32, 0.0, 0.0]));
    }

    pub fn exposure(&self) -> f32 {
        self.exposure
    }

    pub fn draw<'a>(&'a self, rpass: &mut wgpu::RenderPass<'a>) {
        let bg = self
            .bind_group
            .as_ref()
            .expect("tonemap: bind() before draw");
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, bg, &[]);
        rpass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tonemap_tests {
    use super::{srgb_oetf, tonemap_channel};

    #[test]
    fn oetf_anchors() {
        assert!((srgb_oetf(0.0) - 0.0).abs() < 1e-6);
        assert!((srgb_oetf(1.0) - 1.0).abs() < 1e-6);
        assert!((srgb_oetf(0.5) - 0.73535).abs() < 1e-4);
        // the linear segment anchor
        assert!((srgb_oetf(0.0031308) - 12.92 * 0.0031308).abs() < 1e-6);
    }

    #[test]
    fn oetf_monotonic() {
        let mut prev = -1.0f32;
        let mut i = 0;
        while i <= 1024 {
            let v = srgb_oetf(i as f32 / 1024.0);
            assert!(v >= prev, "non-monotonic at {i}");
            prev = v;
            i += 1;
        }
    }

    #[test]
    fn exposure_and_clamp() {
        // parity with the old hardware path: clamp(exposure * sum) then encode
        assert!((tonemap_channel(4.0, 0.25) - srgb_oetf(1.0)).abs() < 1e-6);
        assert!((tonemap_channel(10.0, 1.0) - 1.0).abs() < 1e-6); // clamps
        assert!((tonemap_channel(0.25, 2.0) - srgb_oetf(0.5)).abs() < 1e-6);
    }
}
