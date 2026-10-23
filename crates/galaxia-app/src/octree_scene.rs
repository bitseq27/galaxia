//! It.1 octree scene: cull → reconcile → stream → upload → draw.

use galaxia_data::octree::{frustum_planes, T0Tree};
use galaxia_data::resident::{ResidentSet, StreamWorker};
use galaxia_render::star_pass::StarGpu;
use galaxia_render::StarPass;
use glam::{DVec3, Vec4};
use std::collections::HashSet;

pub struct FrameStats {
    pub desired_leaves: usize,
    pub resident_leaves: usize,
    pub resident_stars: u32,
    pub pending: usize,
    pub loads: u64,
    pub evictions: u64,
    pub mb_streamed: f64,
    pub drawn: u32,
}

pub struct OctreeScene {
    tree: T0Tree,
    worker: StreamWorker,
    resident: ResidentSet,
    pending: HashSet<u32>,
    free: Vec<(u32, u32)>, // free record ranges, sorted
    gpu_cap: u32,
    desired: Vec<u32>,
    ranges: Vec<(u32, u32)>,
}

impl OctreeScene {
    pub fn new(tree: T0Tree, gpu_cap: u32) -> anyhow::Result<Self> {
        let fh = tree.worker_handle()?;
        let leaves = std::sync::Arc::new(tree.leaves.clone());
        let worker = StreamWorker::spawn(leaves, fh);
        let mut free = Vec::new();
        free.push((0, gpu_cap));
        Ok(Self {
            tree,
            worker,
            resident: ResidentSet::new(gpu_cap),
            pending: HashSet::new(),
            free,
            gpu_cap,
            desired: Vec::new(),
            ranges: Vec::new(),
        })
    }

    pub fn tree(&self) -> &T0Tree {
        &self.tree
    }

    fn alloc(&mut self, count: u32) -> Option<u32> {
        let i = self.free.iter().position(|&(_, len)| len >= count)?;
        let (off, len) = self.free[i];
        if len == count {
            self.free.remove(i);
        } else {
            self.free[i] = (off + count, len - count);
        }
        Some(off)
    }

    fn release(&mut self, off: u32, count: u32) {
        self.free.push((off, count));
        self.free.sort_unstable();
        // coalesce
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(self.free.len());
        for r in self.free.drain(..) {
            if let Some(last) = out.last_mut() {
                if last.0 + last.1 == r.0 {
                    last.1 += r.1;
                    continue;
                }
            }
            out.push(r);
        }
        self.free = out;
    }

    /// One frame: cull, reconcile, ingest arrivals, produce draw ranges.
    pub fn frame(
        &mut self,
        cam: DVec3,
        vp: &glam::Mat4,
        m0: f32,
        star_pass: &mut StarPass,
        queue: &wgpu::Queue,
    ) -> FrameStats {
        let planes: [Vec4; 6] = frustum_planes(vp);
        self.tree.cull(cam, &planes, m0, &mut self.desired);
        let leaf_counts: Vec<u32> = self.tree.leaves.iter().map(|l| l.count).collect();
        let (evict, load) = self.resident.reconcile(&self.desired, &leaf_counts);
        for l in evict {
            if let Some(slot) = self.resident.remove(l) {
                self.release(slot.offset, slot.count);
            }
            self.pending.remove(&l);
        }
        for l in load {
            if self.pending.insert(l) {
                self.worker.request(l);
            }
        }
        // ingest arrivals
        for (leaf, bytes) in self.worker.drain() {
            self.pending.remove(&leaf);
            let count = (bytes.len() / 32) as u32;
            let stars: &[StarGpu] = bytemuck::cast_slice(&bytes);
            if let Some(off) = self.alloc(count) {
                star_pass.upload_at(queue, off, stars);
                self.resident.insert(
                    leaf,
                    galaxia_data::resident::Slot { offset: off, count },
                );
                self.resident.bytes_streamed += bytes.len() as u64;
            } else {
                // GPU slot space fragmented away; drop and retry later
                self.resident.remove(leaf);
            }
        }
        // draw ranges for resident+desired leaves (Morton order = leaf id order
        // is NOT draw order relevant; any order is fine for additive points)
        self.ranges.clear();
        for &l in &self.desired {
            if let Some(slot) = self.resident.slot(l) {
                self.ranges.push((slot.offset, slot.count));
            }
        }
        let drawn: u32 = self.ranges.iter().map(|r| r.1).sum();
        FrameStats {
            desired_leaves: self.desired.len(),
            resident_leaves: self.resident.resident_leaves(),
            resident_stars: self.resident.total_stars,
            pending: self.pending.len(),
            loads: self.resident.loads,
            evictions: self.resident.evictions,
            mb_streamed: self.resident.bytes_streamed as f64 / 1e6,
            drawn,
        }
    }

    pub fn draw_ranges(&self) -> &[(u32, u32)] {
        &self.ranges
    }

    pub fn gpu_cap(&self) -> u32 {
        self.gpu_cap
    }
}
