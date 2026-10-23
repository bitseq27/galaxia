//! GPU timing query sets (PLAN It.0 key work: "you cannot measure what you
//! did not instrument"). Timestamp queries around each frame-graph pass,
//! resolved + read back asynchronously. Falls back to None when the adapter
//! lacks TIMESTAMP_QUERY — callers must handle that (CPU timings still shown).

use std::sync::Arc;

pub struct GpuTimers {
    query_set: Option<wgpu::QuerySet>,
    resolve_buf: Option<wgpu::Buffer>,
    read_buf: Option<wgpu::Buffer>,
    ready_flag: Option<Arc<std::sync::atomic::AtomicBool>>,
    /// Number of pass slots; each uses 2 queries (begin, end).
    slots: u32,
    device: Arc<wgpu::Device>,
    pending: bool,
}

impl GpuTimers {
    pub fn new(device: &Arc<wgpu::Device>, supported: bool, slots: u32) -> Self {
        if !supported {
            return Self {
                query_set: None,
                resolve_buf: None,
                read_buf: None,
                ready_flag: None,
                slots,
                device: Arc::clone(device),
                pending: false,
            };
        }
        let n = (slots * 2) as u64;
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("pass-timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: n,
        });
        let resolve_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ts-resolve"),
            size: n * 8,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ts-read"),
            size: n * 8,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Self {
            query_set: Some(query_set),
            resolve_buf: Some(resolve_buf),
            read_buf: Some(read_buf),
            ready_flag: Some(Arc::new(std::sync::atomic::AtomicBool::new(false))),
            slots,
            device: Arc::clone(device),
            pending: false,
        }
    }

    pub fn enabled(&self) -> bool {
        self.query_set.is_some()
    }

    pub fn query_set(&self) -> Option<&wgpu::QuerySet> {
        self.query_set.as_ref()
    }

    /// Write the begin timestamp of `slot` into the render pass.
    pub fn begin<'a>(&self, rpass: &mut wgpu::RenderPass<'a>, slot: u32) {
        if let Some(qs) = &self.query_set {
            rpass.write_timestamp(qs, slot * 2);
        }
    }

    /// Write the end timestamp of `slot`.
    pub fn end<'a>(&self, rpass: &mut wgpu::RenderPass<'a>, slot: u32) {
        if let Some(qs) = &self.query_set {
            rpass.write_timestamp(qs, slot * 2 + 1);
        }
    }

    /// Resolve timestamps for this frame and start async readback.
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) {
        let (Some(qs), Some(rb), Some(read)) =
            (&self.query_set, &self.resolve_buf, &self.read_buf)
        else {
            return;
        };
        if self.pending {
            return; // previous readback not consumed yet — skip a frame
        }
        encoder.resolve_query_set(qs, 0..self.slots * 2, rb, 0);
        encoder.copy_buffer_to_buffer(rb, 0, read, 0, self.slots as u64 * 16);
        let ready = Arc::clone(&self.ready_flag.as_ref().unwrap());
        read.clone().slice(..).map_async(wgpu::MapMode::Read, move |_| {
            ready.store(true, std::sync::atomic::Ordering::Release);
        });
        self.pending = true;
    }

    /// Poll for completed readbacks. Returns per-pass (begin_ns, end_ns) when
    /// a frame's timestamps land; call every frame, use the latest Some(..).
    /// NON-BLOCKING: uses PollType::Poll plus a ready flag from the map_async
    /// callback, so the render thread never waits on the GPU (§ non-negotiable).
    pub fn poll_results(&mut self, device: &wgpu::Device) -> Option<Vec<(u64, u64)>> {
        if !self.pending {
            return None;
        }
        let _ = device.poll(wgpu::PollType::Poll);
        let flag = self.ready_flag.as_ref()?;
        if !flag.load(std::sync::atomic::Ordering::Acquire) {
            return None; // not landed yet — try next frame
        }
        flag.store(false, std::sync::atomic::Ordering::Release);
        let read = self.read_buf.as_ref()?;
        let out = {
            let view = read.slice(..).get_mapped_range();
            let words: &[u64] = bytemuck::cast_slice(&view);
            (0..self.slots as usize)
                .map(|i| (words[i * 2], words[i * 2 + 1]))
                .collect::<Vec<_>>()
        };
        read.slice(..).unmap();
        self.pending = false;
        // Timestamps of 0 mean "not written" (pass skipped).
        Some(out)
    }

    /// Convert a (begin, end) ns pair to milliseconds, guarding unwritten slots.
    pub fn ms(pair: (u64, u64), period_ns: f32) -> Option<f32> {
        if pair.0 == 0 || pair.1 == 0 || pair.1 < pair.0 {
            return None;
        }
        Some((pair.1 - pair.0) as f32 * period_ns / 1e6)
    }
}
