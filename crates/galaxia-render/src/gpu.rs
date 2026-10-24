//! GPU device context. wgpu is PINNED (workspace Cargo.toml, Appendix B).

use anyhow::{Context, Result};
use std::sync::Arc;

pub struct GpuContext {
    pub instance: wgpu::Instance,
    pub adapter: Arc<wgpu::Adapter>,
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    /// True when the adapter supports timestamp queries (GPU timing).
    pub has_timestamps: bool,
    /// True when Rgba16Float is renderable (s008 HDR chain). False => the HDR
    /// slot degrades to Rgba8Unorm (chain identical, no >1 headroom).
    pub hdr_capable: bool,
    pub adapter_info: String,
}

impl GpuContext {
    /// Create a device. Backend choice is compile-time (ADR-0010): the Windows
    /// target builds with `vulkan` (decision D-stack); the build sandbox builds
    /// with `gles` because rustc OOMs on `ash` in 1 GB RAM. Only compiled-in
    /// backends can ever be selected, so runtime behaviour stays deterministic
    /// per build flavour.
    pub async fn new(force_vulkan: bool) -> Result<Self> {
        let backends = if cfg!(feature = "vulkan") && force_vulkan {
            wgpu::Backends::VULKAN
        } else if cfg!(feature = "gles") && !cfg!(feature = "vulkan") {
            wgpu::Backends::GL
        } else {
            wgpu::Backends::PRIMARY
        };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::default(),
            memory_budget_thresholds: Default::default(),
            backend_options: Default::default(),
            display: None,
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .context("no suitable wgpu adapter — is a Vulkan driver installed?")?;

        let info = adapter.get_info();
        let adapter_info = format!("{} ({:?})", info.name, info.backend);

        // Timestamps are optional: we degrade gracefully to CPU timings when
        // absent (never block the skeleton). wgpu 30 splits the feature:
        // writing timestamps INSIDE passes needs TIMESTAMP_QUERY_INSIDE_PASSES
        // in addition to TIMESTAMP_QUERY (resolve/readback).
        let want = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES;
        let features = adapter.features() & want;
        let has_timestamps = features.contains(wgpu::Features::TIMESTAMP_QUERY)
            && features.contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_PASSES);
        // s008 HDR chain: float render target for radiance accumulation.
        let hdr_capable = adapter
            .get_texture_format_features(wgpu::TextureFormat::Rgba16Float)
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("galaxia-device"),
                required_features: features,
                required_limits: wgpu::Limits::default(),
                experimental_features: Default::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .context("request_device failed")?;

        tracing::info!(adapter = %adapter_info, timestamps = has_timestamps, hdr = hdr_capable, "GPU ready");
        Ok(Self {
            instance,
            adapter: Arc::new(adapter),
            device: Arc::new(device),
            queue: Arc::new(queue),
            has_timestamps,
            hdr_capable,
            adapter_info,
        })
    }
}
