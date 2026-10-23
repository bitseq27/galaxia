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
    pub adapter_info: String,
}

impl GpuContext {
    /// Create a device. `force_vulkan` keeps backend selection deterministic
    /// across machines (the Windows target runs Vulkan; the Linux build sandbox
    /// runs lavapipe/llvmpipe via Vulkan).
    pub async fn new(force_vulkan: bool) -> Result<Self> {
        let backends = if force_vulkan {
            wgpu::Backends::VULKAN
        } else {
            wgpu::Backends::PRIMARY
        };
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
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

        // TIMESTAMP_QUERY is optional: lavapipe supports it, but we degrade
        // gracefully to CPU timings when absent (never block the skeleton).
        let want = wgpu::Features::TIMESTAMP_QUERY;
        let features = adapter.features() & want;
        let has_timestamps = features.contains(wgpu::Features::TIMESTAMP_QUERY);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("galaxia-device"),
                required_features: features,
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .context("request_device failed")?;

        tracing::info!(adapter = %adapter_info, timestamps = has_timestamps, "GPU ready");
        Ok(Self {
            instance,
            adapter,
            device: Arc::new(device),
            queue: Arc::new(queue),
            has_timestamps,
            adapter_info,
        })
    }
}
