//! The Iteration-0 point-star pass: buffers, pipeline, uniforms, draw.

use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;

/// One star, GPU-side. 32 bytes — matches the quantised record size in
/// PLAN §8.4 (coincidence by design: the skeleton already respects the budget).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct StarGpu {
    /// Position in parsecs, Sun-centred It.0 shortcut frame.
    pub pos_pc: [f32; 3],
    /// Gaia G magnitude.
    pub mag: f32,
    /// Linear RGB (crude bp_rp placeholder — It.2 replaces with Harre–Heller).
    pub color: [f32; 3],
    pub _pad: f32,
}

/// CPU-side star record (source-agnostic: Gaia CSV or procedural).
#[derive(Copy, Clone, Debug)]
pub struct StarRecord {
    pub pos_pc: [f64; 3],
    pub mag: f32,
    pub color: [f32; 3],
}

impl StarRecord {
    pub fn to_gpu(&self) -> StarGpu {
        StarGpu {
            pos_pc: [
                self.pos_pc[0] as f32,
                self.pos_pc[1] as f32,
                self.pos_pc[2] as f32,
            ],
            mag: self.mag,
            color: self.color,
            _pad: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct FrameUniforms {
    pub view_proj: [[f32; 4]; 4],
    pub cam_right: [f32; 3],
    pub px_scale: f32,
    pub cam_up: [f32; 3],
    pub time_s: f32,
    pub exposure: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub _pad2: f32,
    pub _pad3: f32,
    pub _pad4: f32,
    pub _pad5: f32,
    pub _pad6: f32,
}

// static_assert: uniform struct must be exactly 128 B (WGSL uniform address
// space rounds to a multiple of the 16 B align; naga validates binding size).
const _: () = assert!(std::mem::size_of::<FrameUniforms>() == 128);

pub struct StarPass {
    pub pipeline: wgpu::RenderPipeline,
    pub layout: wgpu::PipelineLayout,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub uniform_buf: wgpu::Buffer,
    pub star_buf: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub star_count: u32,
    device: Arc<wgpu::Device>,
    shader_source: String,
}

impl StarPass {
    pub fn new(device: &Arc<wgpu::Device>, wgsl_source: &str) -> Result<Self> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stars.wgsl"),
            source: wgpu::ShaderSource::Wgsl(wgsl_source.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("star-pass-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("star-pass-layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        // Blend: additive (ONE, ONE) into the swapchain/offscreen target.
        // It.2 moves to an HDR float target; additive already anticipates it.
        let blend = Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stars"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[], // instancing only — no vertex buffer in It.0
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    // Set per-surface at creation time by the caller? No — the
                    // skeleton renders to a single known format chosen in
                    // `pipeline_for_format`. Callers must use that.
                    format: PIPELINE_FORMAT,
                    blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-uniforms"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Start with a single invisible star; real data uploaded via set_stars.
        let star_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("star-buffer"),
            size: std::mem::size_of::<StarGpu>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("star-pass-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: star_buf.as_entire_binding(),
                },
            ],
        });

        Ok(Self {
            pipeline,
            layout,
            bind_group_layout: bgl,
            uniform_buf,
            star_buf,
            bind_group,
            star_count: 0,
            device: Arc::clone(device),
            shader_source: wgsl_source.to_string(),
        })
    }

    /// Rebuild the pipeline from new WGSL source (hot-reload, PLAN It.0 key work).
    /// Returns Err with the naga/wgpu validation message if the shader is broken —
    /// the caller keeps the OLD pipeline so a typo never kills the running app.
    pub fn try_rebuild(&mut self, new_source: &str) -> Result<()> {
        let fresh = StarPass::new(&self.device, new_source)?;
        // Move the new pipeline/bindings in; keep the star buffer contents by
        // re-uploading (star buffers are small in It.0; It.1 streams properly).
        self.pipeline = fresh.pipeline;
        self.layout = fresh.layout;
        self.bind_group_layout = fresh.bind_group_layout;
        self.uniform_buf = fresh.uniform_buf;
        self.shader_source = new_source.to_string();
        // Recreate the bind group against the NEW layout but re-use the buffer.
        self.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("star-pass-bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.star_buf.as_entire_binding(),
                },
            ],
        });
        tracing::info!("WGSL hot-reload: pipeline rebuilt OK");
        Ok(())
    }

    pub fn shader_source(&self) -> &str {
        &self.shader_source
    }

    pub fn set_stars(&mut self, queue: &wgpu::Queue, stars: &[StarGpu]) {
        let count = stars.len().max(1) as u32;
        let data: Vec<StarGpu> = if stars.is_empty() {
            vec![StarGpu::zeroed()]
        } else {
            stars.to_vec()
        };
        let bytes = bytemuck::cast_slice(&data);
        let needed = bytes.len() as u64;
        if needed > self.star_buf.size() {
            self.star_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("star-buffer"),
                size: needed,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("star-pass-bg"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.uniform_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.star_buf.as_entire_binding(),
                    },
                ],
            });
        }
        queue.write_buffer(&self.star_buf, 0, bytes);
        self.star_count = count;
    }

    pub fn write_uniforms(&self, queue: &wgpu::Queue, u: &FrameUniforms) {
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(u));
    }

    pub fn draw<'a>(&'a self, rpass: &mut wgpu::RenderPass<'a>) {
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.bind_group, &[]);
        rpass.draw(0..6, 0..self.star_count);
    }
}

/// The single surface/target format the skeleton renders to.
pub const PIPELINE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
