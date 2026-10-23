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
    #[allow(dead_code)] // kept for future direct-submit waits; poll uses caller's device
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
        let n = slots * 2;
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("pass-timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: n,
        });
        let resolve_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ts-resolve"),
            size: n as u64 * 8,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ts-read"),
            size: n as u64 * 8,
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

    /// Encode the timestamp resolve + copy into the frame's encoder.
    /// IMPORTANT: `begin_readback` must be called AFTER this encoder is
    /// submitted — wgpu associates map_async with the submission index at
    /// call time, so mapping before submit leaves the buffer mapped during
    /// the submit (validation error: "buffer is still mapped").
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) -> bool {
        let (Some(qs), Some(rb), Some(read)) =
            (&self.query_set, &self.resolve_buf, &self.read_buf)
        else {
            return false;
        };
        if self.pending {
            return false; // previous readback not consumed yet — skip a frame
        }
        encoder.resolve_query_set(qs, 0..self.slots * 2, rb, 0);
        encoder.copy_buffer_to_buffer(rb, 0, read, 0, self.slots as u64 * 16);
        true
    }

    /// Start the async readback of the just-submitted resolve. Call AFTER
    /// queue.submit for the encoder that `resolve` encoded into.
    pub fn begin_readback(&mut self, did_encode: bool) {
        if !did_encode {
            return;
        }
        let Some(read) = &self.read_buf else { return };
        let ready = Arc::clone(self.ready_flag.as_ref().unwrap());
        read.clone().slice(..).map_async(wgpu::MapMode::Read, move |_| {
            ready.store(true, std::sync::atomic::Ordering::Release);
        });
        self.pending = true;
    }

    /// Poll for completed readbacks. Returns per-pass (begin_ns, end_ns) when
    /// a frame's timestamps land; call every frame, use the latest Some(..).
    ///
    /// It.0 DEBT (recorded in BUILDLOG session 001): this BLOCKS (single
    /// read buffer — submitting while an async map is in flight is a
    /// validation error, and the alternative races). It.2 must double-buffer
    /// the query sets + read buffer so this becomes non-blocking, per the
    /// "never block the render thread" non-negotiable.
    pub fn poll_results(&mut self, device: &wgpu::Device) -> Option<Vec<(u64, u64)>> {
        if !self.pending {
            return None;
        }
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let flag = self.ready_flag.as_ref()?;
        if !flag.load(std::sync::atomic::Ordering::Acquire) {
            return None; // not landed yet — try next frame
        }
        flag.store(false, std::sync::atomic::Ordering::Release);
        let read = self.read_buf.as_ref()?;
        let out = {
            let view = match read.slice(..).get_mapped_range() {
                Ok(v) => v,
                Err(_) => return None,
            };
            let words: &[u64] = bytemuck::cast_slice(&view);
            let v = (0..self.slots as usize)
                .map(|i| (words[i * 2], words[i * 2 + 1]))
                .collect::<Vec<_>>();
            drop(view);
            v
        };
        read.unmap();
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
