//! galaxia-data — schemas, octree, streaming, resident set (PLAN §4.1, §H2, §8.6).
//!
//! It.1 scope: read the T0 octree artifact set produced by `etl/octree_build.py`,
//! cull it per frame (frustum ∩ physical LOD), and keep a bounded LRU resident
//! set of leaf payloads streamed from disk on a worker thread.

pub mod octree;
pub mod resident;

pub use octree::T0Tree;
pub use resident::ResidentSet;
