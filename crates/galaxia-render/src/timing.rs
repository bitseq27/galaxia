//! GPU timing query sets (PLAN It.0 key work: "you cannot measure what you
//! did not instrument"). Timestamp queries around each frame-graph pass,
//! resolved + read back asynchronously. Falls back to None when the adapter
//! lacks TIMESTAMP_QUERY — callers must handle that (CPU timings still shown).
//!
//! s007: DOUBLE-BUFFERED. Two independent (query set, resolve buffer, read
//! buffer) pairs alternate per frame, so a readback in flight no longer forces
//! `resolve` to skip the next frame, and `poll_results` drives callbacks with
//! `PollType::Poll` (non-blocking) instead of the It.0-era
//! `WaitIndefinitely` block — the "never block the render thread on I/O"
//! non-negotiable (PLAN §16.4 rules). Debt retired here; see BUILDLOG s007.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// One (query set, resolve buffer, read buffer) triple + its readback state.
struct TimerSet {
    query_set: wgpu::QuerySet,
    resolve_buf: wgpu::Buffer,
    read_buf: wgpu::Buffer,
    ready: Arc<AtomicBool>,
    pending: bool,
}

pub struct GpuTimers {
    sets: Option<[TimerSet; 2]>,
    /// Set the next frame's timestamps + resolve will use.
    cur: usize,
    /// Set whose readback was begun most recently (begin_readback target).
    last: usize,
    /// Number of pass slots; each uses 2 queries (begin, end).
    slots: u32,
    #[allow(dead_code)] // kept for future direct-submit waits; poll uses caller's device
    device: Arc<wgpu::Device>,
}

impl GpuTimers {
    pub fn new(device: &Arc<wgpu::Device>, supported: bool, slots: u32) -> Self {
        let sets = if !supported {
            None
        } else {
            let n = slots * 2;
            Some([0, 1].map(|_| {
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
                TimerSet {
                    query_set,
                    resolve_buf,
                    read_buf,
                    ready: Arc::new(AtomicBool::new(false)),
                    pending: false,
                }
            }))
        };
        Self {
            sets,
            cur: 0,
            last: 0,
            slots,
            device: Arc::clone(device),
        }
    }

    pub fn enabled(&self) -> bool {
        self.sets.is_some()
    }

    /// The query set the CURRENT frame's passes must write into (the set the
    /// next `resolve` will consume).
    pub fn query_set(&self) -> Option<&wgpu::QuerySet> {
        self.sets.as_ref().map(|s| &s[self.cur].query_set)
    }

    /// Write the begin timestamp of `slot` into the render pass.
    pub fn begin<'a>(&self, rpass: &mut wgpu::RenderPass<'a>, slot: u32) {
        if let Some(qs) = self.query_set() {
            rpass.write_timestamp(qs, slot * 2);
        }
    }

    /// Write the end timestamp of `slot`.
    pub fn end<'a>(&self, rpass: &mut wgpu::RenderPass<'a>, slot: u32) {
        if let Some(qs) = self.query_set() {
            rpass.write_timestamp(qs, slot * 2 + 1);
        }
    }

    /// Encode the timestamp resolve + copy of the CURRENT set into the frame's
    /// encoder, then advance to the other set. Returns false only when both
    /// sets have readbacks in flight (GPU two full frames behind — drop the
    /// measurement, never block).
    /// IMPORTANT: `begin_readback` must be called AFTER this encoder is
    /// submitted — wgpu associates map_async with the submission index at
    /// call time, so mapping before submit leaves the buffer mapped during
    /// the submit (validation error: "buffer is still mapped").
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder) -> bool {
        let Some(sets) = self.sets.as_mut() else {
            return false;
        };
        let s = &sets[self.cur];
        if s.pending {
            return false; // both sets airborne — skip this frame's measurement
        }
        encoder.resolve_query_set(&s.query_set, 0..self.slots * 2, &s.resolve_buf, 0);
        encoder.copy_buffer_to_buffer(&s.resolve_buf, 0, &s.read_buf, 0, self.slots as u64 * 16);
        self.last = self.cur;
        self.cur ^= 1;
        true
    }

    /// Start the async readback of the just-submitted resolve. Call AFTER
    /// queue.submit for the encoder that `resolve` encoded into.
    pub fn begin_readback(&mut self, did_encode: bool) {
        if !did_encode {
            return;
        }
        let Some(sets) = self.sets.as_mut() else { return };
        let s = &mut sets[self.last];
        let ready = Arc::clone(&s.ready);
        s.read_buf.slice(..).map_async(wgpu::MapMode::Read, move |_| {
            ready.store(true, Ordering::Release);
        });
        s.pending = true;
    }

    /// Poll for completed readbacks WITHOUT BLOCKING: one non-blocking
    /// `PollType::Poll` drives map callbacks, then any set whose flag landed
    /// is consumed (oldest set index first). Returns per-pass
    /// (begin_ns, end_ns); call every frame, use the latest Some(..).
    pub fn poll_results(&mut self, device: &wgpu::Device) -> Option<Vec<(u64, u64)>> {
        let sets = self.sets.as_mut()?;
        let _ = device.poll(wgpu::PollType::Poll); // non-blocking callback drive
        for s in sets.iter_mut() {
            if !s.pending || !s.ready.load(Ordering::Acquire) {
                continue;
            }
            s.ready.store(false, Ordering::Release);
            let out = {
                let view = match s.read_buf.slice(..).get_mapped_range() {
                    Ok(v) => v,
                    Err(_) => continue, // lost the race with a reset; try next set
                };
                let words: &[u64] = bytemuck::cast_slice(&view);
                let v = (0..self.slots as usize)
                    .map(|i| (words[i * 2], words[i * 2 + 1]))
                    .collect::<Vec<_>>();
                drop(view);
                v
            };
            s.read_buf.unmap();
            s.pending = false;
            // Timestamps of 0 mean "not written" (pass skipped).
            return Some(out);
        }
        None
    }

    /// Convert a (begin, end) ns pair to milliseconds, guarding unwritten slots.
    pub fn ms(pair: (u64, u64), period_ns: f32) -> Option<f32> {
        if pair.0 == 0 || pair.1 == 0 || pair.1 < pair.0 {
            return None;
        }
        Some((pair.1 - pair.0) as f32 * period_ns / 1e6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ms() guards: unwritten (0) and inverted pairs are None, else scaled ns.
    #[test]
    fn ms_guards() {
        assert_eq!(GpuTimers::ms((0, 0), 1.0), None);
        assert_eq!(GpuTimers::ms((0, 5), 1.0), None);
        assert_eq!(GpuTimers::ms((5, 3), 1.0), None);
        // 2,000,000 ticks at 1 ns/tick = 2 ms; at 0.5 ns/tick = 1 ms.
        assert_eq!(GpuTimers::ms((1_000_000, 3_000_000), 1.0), Some(2.0));
        assert_eq!(GpuTimers::ms((1_000_000, 3_000_000), 0.5), Some(1.0));
    }

    /// The s007 contract, on a real device where one exists (skipped
    /// gracefully where no adapter/timestamps — CI without GPU):
    ///  1. a readback in flight must NOT make the next frame's resolve skip
    ///     (single-buffer design skipped every other frame);
    ///  2. poll_results with a readback airborne returns promptly (no
    ///     WaitIndefinitely — render thread never blocks on readback I/O);
    ///  3. the double-buffered cycle eventually yields one entry per slot.
    #[test]
    fn double_buffered_readback_never_skips_or_blocks() {
        let gpu = match pollster::block_on(crate::gpu::GpuContext::new(false)) {
            Ok(g) => g,
            Err(_) => {
                eprintln!("no wgpu adapter in test env — skipping GPU timing test");
                return;
            }
        };
        let slots = 7u32;
        let mut timers = GpuTimers::new(&gpu.device, gpu.has_timestamps, slots);
        if !timers.enabled() {
            eprintln!("adapter lacks TIMESTAMP_QUERY — skipping");
            return;
        }

        // Frame 1: encode resolve, submit, begin readback (readback now airborne).
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        assert!(timers.resolve(&mut enc), "first resolve must encode");
        gpu.queue.submit([enc.finish()]);
        timers.begin_readback(true);

        // Frame 2, immediately: the old single-buffer code returned false here.
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        let second = timers.resolve(&mut enc);
        gpu.queue.submit([enc.finish()]);
        timers.begin_readback(second);
        assert!(second, "double buffering must not skip the consecutive frame");

        // poll_results with readbacks airborne must not block the caller;
        // whatever it returns counts toward the two encoded frames.
        let t = std::time::Instant::now();
        let first = timers.poll_results(&gpu.device);
        let waited = t.elapsed();
        assert!(
            waited.as_millis() < 50,
            "poll_results blocked for {waited:?} — must be non-blocking"
        );

        // And the cycle yields full slot sets for both encoded frames.
        let mut got = usize::from(first.is_some());
        for _ in 0..200 {
            if got == 2 {
                break;
            }
            if timers.poll_results(&gpu.device).is_some() {
                got += 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(got, 2, "both double-buffered frames must land");

        // Slot count check on one more cycle.
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        assert!(timers.resolve(&mut enc));
        gpu.queue.submit([enc.finish()]);
        timers.begin_readback(true);
        let mut len = None;
        for _ in 0..200 {
            if let Some(v) = timers.poll_results(&gpu.device) {
                len = Some(v.len());
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(len, Some(slots as usize));
    }
}
