//! It.1 octree scene: cull → reconcile → stream → upload → draw.

use galaxia_data::octree::{frustum_planes, T0Tree};
use galaxia_data::resident::{ResidentSet, StreamWorker};
use galaxia_render::star_pass::StarGpu;
use galaxia_render::StarPass;
use glam::{DVec3, Vec4};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[allow(dead_code)] // evictions/desired surface in the It.6 governor overlay
pub struct FrameStats {
    pub desired_leaves: usize,
    pub resident_leaves: usize,
    pub resident_stars: u32,
    /// In-flight worker requests (issued, not yet arrived).
    pub pending: usize,
    /// Desired leaves waiting for a contiguous GPU slot range (capacity or
    /// fragmentation tail — a steady state, NOT outstanding work; the It.1
    /// settle gate measures `pending`, not `deferred`).
    pub deferred: usize,
    pub loads: u64,
    pub evictions: u64,
    pub mb_streamed: f64,
    pub drawn: u32,
}

pub struct OctreeScene {
    tree: T0Tree,
    worker: StreamWorker,
    resident: ResidentSet,
    /// Per-leaf centroid (from the leaf node records), for session 004's
    /// distance-priority admission order.
    leaf_centers: Vec<DVec3>,
    /// Leaves with an outstanding worker request. Survives eviction (the read
    /// cannot be cancelled; a stale arrival is dropped on ingest) so a leaf is
    /// never requested twice — duplicate arrivals used to overwrite the
    /// resident slot and leak the old range (session 003 fly-test find).
    inflight: HashSet<u32>,
    /// Arrivals whose alloc failed (no contiguous range): parked until the
    /// free list can actually fit them, instead of hot re-requesting.
    deferred: HashSet<u32>,
    free: Vec<(u32, u32)>, // free record ranges, sorted
    /// s009: CPU mirror of resident leaf star records for the tile-rasteriser
    /// binning pass (the GPU buffer is upload-only; binning runs on the CPU
    /// in v1). Inserted on ingest, removed on eviction — same lifetime as
    /// the resident slot.
    mirror: HashMap<u32, Arc<Vec<StarGpu>>>,
    gpu_cap: u32,
    desired: Vec<u32>,
    ranges: Vec<(u32, u32)>,
}

impl OctreeScene {
    pub fn new(tree: T0Tree, gpu_cap: u32) -> anyhow::Result<Self> {
        let fh = tree.worker_handle()?;
        let leaves = std::sync::Arc::new(tree.leaves.clone());
        let worker = StreamWorker::spawn(leaves, fh);
        let free = vec![(0, gpu_cap)];
        // Leaf centroids live on the leaf NODE records (LeafIdx is disk-layout
        // only); one pass over <=~10^3 nodes at init, never per frame.
        let mut leaf_centers = vec![DVec3::ZERO; tree.leaves.len()];
        for n in &tree.nodes {
            if n.is_leaf() {
                let id = n.leaf_id as usize;
                if id < leaf_centers.len() {
                    leaf_centers[id] = n.center();
                }
            }
        }
        Ok(Self {
            tree,
            worker,
            resident: ResidentSet::new(gpu_cap),
            mirror: HashMap::new(),
            leaf_centers,
            inflight: HashSet::new(),
            deferred: HashSet::new(),
            free,
            gpu_cap,
            desired: Vec::new(),
            ranges: Vec::new(),
        })
    }

    #[allow(dead_code)] // windowed overlay consumes these from It.1 onward
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
            if let Some(last) = out.last_mut()
                && last.0 + last.1 == r.0 {
                    last.1 += r.1;
                    continue;
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
        // Session 004: distance-priority admission. reconcile fills capacity
        // in `desired` order; leaf-id (depth-aligned Morton) order is spatially
        // clumped, so a cap below the desired set unloaded whole sky REGIONS —
        // the diagonal wedge in the s002 shot. Nearest-first makes the tail
        // the farthest shells instead: uniform thinning, no wedge.
        let centers = &self.leaf_centers;
        self.desired.sort_by(|&a, &b| {
            let da = (centers[a as usize] - cam).length_squared();
            let db = (centers[b as usize] - cam).length_squared();
            da.partial_cmp(&db)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        });
        let leaf_counts: Vec<u32> = self.tree.leaves.iter().map(|l| l.count).collect();
        let desired_set: HashSet<u32> = self.desired.iter().copied().collect();

        let (evicted, load) = self.resident.reconcile(&self.desired, &leaf_counts);
        // Session 003 fix: reconcile hands back the evicted slots — releasing
        // their ranges is what keeps the free list whole (they used to leak).
        for (l, slot) in evicted {
            self.release(slot.offset, slot.count);
            self.deferred.remove(&l);
            self.mirror.remove(&l);
        }
        // Un-park deferred leaves once the free list can actually fit them
        // (checked after this frame's releases); drop ones no longer desired.
        if !self.deferred.is_empty() {
            self.deferred.retain(|&l| {
                desired_set.contains(&l)
                    && !self
                        .free
                        .iter()
                        .any(|&(_, len)| len >= leaf_counts[l as usize])
            });
        }
        for l in load {
            // Never double-request: in-flight stays in-flight across eviction.
            if self.inflight.contains(&l) || self.deferred.contains(&l) {
                continue;
            }
            self.inflight.insert(l);
            self.worker.request(l);
        }
        // ingest arrivals
        for (leaf, bytes) in self.worker.drain() {
            self.inflight.remove(&leaf);
            let count = (bytes.len() / 32) as u32;
            // Stale duplicate (evicted then re-requested before the old read
            // landed) or no longer desired: drop the bytes, change nothing.
            if self.resident.slot(leaf).is_some() || !desired_set.contains(&leaf) {
                continue;
            }
            let stars: &[StarGpu] = bytemuck::cast_slice(&bytes);
            if let Some(off) = self.alloc(count) {
                star_pass.upload_at(queue, off, stars);
                self.mirror.insert(leaf, Arc::new(stars.to_vec()));
                self.resident.insert(
                    leaf,
                    galaxia_data::resident::Slot { offset: off, count },
                );
                self.resident.bytes_streamed += bytes.len() as u64;
                self.deferred.remove(&leaf);
            } else {
                // No contiguous range right now: park it (NOT an immediate
                // re-request — that hot loop kept `pending` > 0 forever and
                // burned worker IO; session 003 fly-test settle gate found it).
                self.deferred.insert(leaf);
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
            pending: self.inflight.len(),
            deferred: self.deferred.len(),
            loads: self.resident.loads,
            evictions: self.resident.evictions,
            mb_streamed: self.resident.bytes_streamed as f64 / 1e6,
            drawn,
        }
    }

    /// s009: collect the resident+desired star records for CPU binning.
    pub fn collect_stars(&self, out: &mut Vec<(DVec3, f32, [f32; 3])>) {
        out.clear();
        for &l in &self.desired {
            if let Some(arc) = self.mirror.get(&l) {
                for st in arc.iter() {
                    out.push((DVec3::new(st.pos_pc[0] as f64, st.pos_pc[1] as f64, st.pos_pc[2] as f64), st.mag, st.color));
                }
            }
        }
    }

    pub fn draw_ranges(&self) -> &[(u32, u32)] {
        &self.ranges
    }

    #[allow(dead_code)]
    pub fn gpu_cap(&self) -> u32 {
        self.gpu_cap
    }
}
