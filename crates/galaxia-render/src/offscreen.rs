//! Offscreen render target → PNG. This is how the headless build sandbox
//! produces `docs/build/shots/` screenshots, and how the golden-image harness
//! (PLAN §17.4) will work later. On the Windows target the same code path
//! powers `--screenshot`.

use anyhow::Result;
use std::sync::Arc;

use crate::star_pass::PIPELINE_FORMAT;

pub struct OffscreenTarget {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    read_buf: wgpu::Buffer,
    device: Arc<wgpu::Device>,
}

impl OffscreenTarget {
    pub fn new(device: &Arc<wgpu::Device>, width: u32, height: u32) -> Result<Self> {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: PIPELINE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bytes_per_row = width * 4;
        let read_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("offscreen-read"),
            size: (bytes_per_row * height) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Ok(Self {
            texture,
            view,
            width,
            height,
            read_buf,
            device: Arc::clone(device),
        })
    }

    /// Encode the texture→buffer copy. Call before `finish_read`.
    pub fn encode_copy(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::ImageCopyBuffer {
                buffer: &self.read_buf,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(self.width * 4),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
        );
    }

    /// Submit `encoder`, then map + read. Returns RGBA8 pixels, top-down.
    /// Blocking is fine here: this path is ONLY for screenshots/benches,
    /// never the live loop.
    pub fn finish_read(&self, queue: &wgpu::Queue, encoder: wgpu::CommandEncoder) -> Result<Vec<u8>> {
        queue.submit([encoder.finish()]);
        let slice = self.read_buf.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::Wait { submission_index: None });
        let mut rgba = slice.get_mapped_range().to_vec();
        slice.unmap();
        // BGRA → RGBA
        for px in rgba.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        Ok(rgba)
    }

    pub fn save_png(&self, path: &std::path::Path, rgba: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        image::save_buffer(
            path,
            rgba,
            self.width,
            self.height,
            image::ColorType::Rgba8,
        )?;
        tracing::info!(path = %path.display(), w = self.width, h = self.height, "screenshot saved");
        Ok(())
    }
}
