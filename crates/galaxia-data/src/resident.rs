//! Bounded LRU resident set + disk stream worker (PLAN §8.6 skeleton).
//!
//! It.1 shape: leaf payloads live on disk; the resident set holds at most
//! `capacity` stars' worth of GPU-slot assignments. A worker thread reads
//! leaf ranges off `stars.bin` (never the render thread — non-negotiable),
//! hands bytes over an mpsc channel; the render thread ingests + uploads.

use crate::octree::{LeafIdx, T0Tree};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

#[derive(Clone, Copy)]
pub struct Slot {
    pub offset: u32, // record offset in the GPU instance buffer
    pub count: u32,
}

pub struct ResidentSet {
    pub capacity: u32,
    resident: HashMap<u32, Slot>,
    lru: VecDeque<u32>, // front = oldest
    pub total_stars: u32,
    pub loads: u64,
    pub evictions: u64,
    pub bytes_streamed: u64,
}

impl ResidentSet {
    pub fn new(capacity: u32) -> Self {
        Self {
            capacity,
            resident: HashMap::new(),
            lru: VecDeque::new(),
            total_stars: 0,
            loads: 0,
            evictions: 0,
            bytes_streamed: 0,
        }
    }

    pub fn slot(&self, leaf: u32) -> Option<&Slot> {
        self.resident.get(&leaf)
    }

    pub fn touch(&mut self, leaf: u32) {
        self.lru.retain(|&l| l != leaf);
        self.lru.push_back(leaf);
    }

    pub fn insert(&mut self, leaf: u32, slot: Slot) {
        self.total_stars += slot.count;
        self.resident.insert(leaf, slot);
        self.touch(leaf);
        self.loads += 1;
    }

    pub fn remove(&mut self, leaf: u32) -> Option<Slot> {
        if let Some(s) = self.resident.remove(&leaf) {
            self.total_stars -= s.count;
            self.lru.retain(|&l| l != leaf);
            self.evictions += 1;
            Some(s)
        } else {
            None
        }
    }

    /// Reconcile against this frame's desired leaf set.
    /// Policy (session 002 fix): evict ONLY residents no longer desired.
    /// Never evict a desired resident to make room for another desired leaf —
    /// that thrashes (everything evicted before its replacement arrives and
    /// the drawn set collapses to 0). Oversubscription is resolved by leaving
    /// the tail of the stable (leaf-id) order unloaded until capacity frees.
    ///
    /// Session 003 fix: returns the EVICTED LEAVES WITH THEIR SLOTS so the
    /// caller can release the GPU ranges. Previously reconcile removed the
    /// residents internally and discarded the Slots; the caller's second
    /// remove() then returned None and release() never ran — every eviction
    /// leaked its slot range until the free list was exhausted (fly-test
    /// settle: alloc-fail → re-request thrash, pending never drained).
    pub fn reconcile(
        &mut self,
        desired: &[u32],
        leaf_counts: &[u32],
    ) -> (Vec<(u32, Slot)>, Vec<u32>) {
        let want: std::collections::HashSet<u32> = desired.iter().copied().collect();
        let mut evict = Vec::new();
        for &l in self.lru.iter() {
            if !want.contains(&l) {
                evict.push(l);
            }
        }
        let mut evicted = Vec::with_capacity(evict.len());
        for &l in &evict {
            if let Some(s) = self.remove(l) {
                evicted.push((l, s));
            }
        }
        let mut free = self.capacity.saturating_sub(self.total_stars);
        let mut load = Vec::new();
        for &l in desired {
            if self.resident.contains_key(&l) {
                self.touch(l);
                continue;
            }
            let count = leaf_counts[l as usize];
            if count <= free {
                free -= count;
                load.push(l);
            }
        }
        (evicted, load)
    }

    pub fn resident_leaves(&self) -> usize {
        self.resident.len()
    }
}

/// Stream worker: owns a file handle, services leaf read requests.
pub struct StreamWorker {
    pub rx: Receiver<(u32, Vec<u8>)>,
    req_tx: Option<Sender<u32>>,
    handle: Option<JoinHandle<()>>,
}

impl StreamWorker {
    pub fn spawn(tree_leaves: Arc<Vec<LeafIdx>>, fh: std::fs::File) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<u32>();
        let (res_tx, rx) = mpsc::channel::<(u32, Vec<u8>)>();
        let handle = std::thread::Builder::new()
            .name("galaxia-stream".into())
            .spawn(move || {
                let mut fh = fh;
                for leaf in req_rx {
                    let Some(l) = tree_leaves.get(leaf as usize) else { continue };
                    match T0Tree::read_leaf_bytes(&mut fh, l) {
                        Ok(bytes) => {
                            if res_tx.send((leaf, bytes)).is_err() {
                                break;
                            }
                        }
                        Err(e) => tracing::warn!(leaf, "stream read failed: {e}"),
                    }
                }
            })
            .expect("spawn stream worker");
        Self { rx, req_tx: Some(req_tx), handle: Some(handle) }
    }

    pub fn request(&self, leaf: u32) {
        if let Some(tx) = &self.req_tx {
            let _ = tx.send(leaf);
        }
    }

    pub fn drain(&self) -> Vec<(u32, Vec<u8>)> {
        let mut out = Vec::new();
        while let Ok(item) = self.rx.try_recv() {
            out.push(item);
        }
        out
    }
}

impl Drop for StreamWorker {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            // Drop the ONLY sender so the worker's recv loop terminates,
            // then join. (Dropping a clone of the sender does NOT close the
            // channel — that deadlocked session 002's first t0 run.)
            drop(self.req_tx.take());
            let _ = h.join();
        }
    }
}

/// Mutex-guarded shared leaves vec convenience alias.
pub type SharedLeaves = Arc<Vec<LeafIdx>>;
