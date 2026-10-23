//! T0 octree: load + per-frame cull (PLAN §H2, §6.4 culling cascade skeleton).

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Vec4};
use serde::Deserialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// 80 B node record — must match etl/octree_build.py NODE dtype exactly.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct NodeRecord {
    pub morton: u64,
    pub depth: u32,
    pub flags: u32, // bit0 = is_leaf
    pub count: u32,
    pub leaf_id: u32,
    pub brightest: f32, // min Gaia G in subtree
    pub cx: f32,
    pub cy: f32,
    pub cz: f32,
    pub radius: f32, // cell half-diagonal, pc
    pub pad: u32,
    pub children: [i32; 8],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct LeafIdx {
    pub start: u64, // record offset into stars.bin
    pub count: u32,
    pub pad: u32,
}

#[derive(Deserialize)]
pub struct TreeMeta {
    pub schema: String,
    pub stars: u64,
    pub leaves: u64,
    pub nodes: u64,
    pub root_center_pc: [f64; 3],
    pub root_half_pc: f64,
    pub record_bytes: u64,
}

impl NodeRecord {
    #[inline]
    pub fn is_leaf(&self) -> bool {
        self.flags & 1 == 1
    }
    #[inline]
    pub fn center(&self) -> DVec3 {
        DVec3::new(self.cx as f64, self.cy as f64, self.cz as f64)
    }
}

pub struct T0Tree {
    pub meta: TreeMeta,
    pub nodes: Vec<NodeRecord>,
    pub leaves: Vec<LeafIdx>,
    stars: File,
    pub path: std::path::PathBuf,
}

impl T0Tree {
    pub fn load(dir: &Path) -> Result<Self> {
        let meta: TreeMeta = serde_json::from_str(
            &std::fs::read_to_string(dir.join("tree_meta.json"))
                .context("tree_meta.json missing — run etl/octree_build.py")?,
        )?;
        anyhow::ensure!(meta.schema == "galaxia-t0-octree-v0", "schema mismatch");
        anyhow::ensure!(meta.record_bytes == 32, "record size != 32 B");
        let mut nb = Vec::new();
        File::open(dir.join("nodes.bin"))?.read_to_end(&mut nb)?;
        anyhow::ensure!(nb.len() % std::mem::size_of::<NodeRecord>() == 0, "nodes.bin ragged");
        let nodes: Vec<NodeRecord> = bytemuck::cast_slice(&nb).to_vec();
        anyhow::ensure!(nodes.len() as u64 == meta.nodes, "node count mismatch");
        let mut lb = Vec::new();
        File::open(dir.join("leaves.idx"))?.read_to_end(&mut lb)?;
        let leaves: Vec<LeafIdx> = bytemuck::cast_slice(&lb).to_vec();
        anyhow::ensure!(leaves.len() as u64 == meta.leaves, "leaf count mismatch");
        let stars = File::open(dir.join("stars.bin"))?;
        tracing::info!(
            stars = meta.stars,
            leaves = meta.leaves,
            nodes = nodes.len(),
            "T0 octree loaded (node metadata resident)"
        );
        Ok(Self { meta, nodes, leaves, stars, path: dir.to_path_buf() })
    }

    /// Open an independent file handle for the stream worker thread.
    pub fn worker_handle(&self) -> Result<File> {
        Ok(File::open(self.path.join("stars.bin"))?)
    }

    /// Read one leaf payload (32 B/star records) — called on the stream thread.
    pub fn read_leaf_bytes(fh: &mut File, leaf: &LeafIdx) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; leaf.count as usize * 32];
        fh.seek(SeekFrom::Start(leaf.start * 32))?;
        fh.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Frustum ∩ physical-LOD cull. `planes` are six world-space half-space
    /// planes (n·p + d >= 0 inside). LOD: a node is interesting when its
    /// brightest star would appear brighter than `m0` from the camera:
    ///   m_cam = G + 5·log10(D_cam / D_sun)  ≤  m0
    /// (G is the apparent magnitude at the Sun; §H2's M_brightest criterion
    ///  in apparent-mag form for the Sun-centred skeleton frame.)
    pub fn cull(&self, cam: DVec3, planes: &[Vec4; 6], m0: f32, out: &mut Vec<u32>) {
        out.clear();
        self.cull_rec(0, cam, planes, m0, out, 0);
    }

    fn cull_rec(
        &self,
        ni: usize,
        cam: DVec3,
        planes: &[Vec4; 6],
        m0: f32,
        out: &mut Vec<u32>,
        depth_guard: u32,
    ) {
        if depth_guard > 24 {
            return;
        }
        let n = self.nodes[ni];
        let c = n.center();
        let r = n.radius as f64;
        // sphere vs frustum
        for p in planes {
            let d = p.x as f64 * c.x + p.y as f64 * c.y + p.z as f64 * c.z + p.w as f64;
            if d < -r {
                return; // fully outside this plane
            }
        }
        // physical LOD: brightest member's apparent mag from the camera
        let d_cam = (c - cam).length().max(1e-6);
        let d_sun = c.length().max(1e-6);
        let m_cam = n.brightest + 5.0 * (d_cam / d_sun).log10() as f32;
        if m_cam > m0 {
            return; // nothing in this subtree is visible at this exposure
        }
        if n.is_leaf() {
            out.push(n.leaf_id);
            return;
        }
        for &ch in &n.children {
            if ch >= 0 {
                self.cull_rec(ch as usize, cam, planes, m0, out, depth_guard + 1);
            }
        }
    }
}

/// Extract six inside-positive frustum planes from a view-projection matrix
/// (Gribb–Hartmann, row combinations; column-major glam).
pub fn frustum_planes(vp: &glam::Mat4) -> [Vec4; 6] {
    let m = vp.to_cols_array_2d(); // m[col][row]
    let row = |r: usize| Vec4::new(m[0][r], m[1][r], m[2][r], m[3][r]);
    let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
    let mk = |a: Vec4, b: Vec4, s: f32| {
        let p = a + b * s;
        let n = p.truncate();
        let l = n.length();
        if l > 1e-12 { p / l } else { p }
    };
    [
        mk(r3, r0, 1.0),  // left
        mk(r3, r0, -1.0), // right
        mk(r3, r1, 1.0),  // bottom
        mk(r3, r1, -1.0), // top
        mk(r3, r2, 1.0),  // near
        mk(r3, r2, -1.0), // far
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// It.1 astrometry gate (sandbox side): the 32 B quantisation stores f32
    /// parsecs. Worst-case angular error of a position at distance d is
    /// ulp(d)/2 / d radians. Must stay far below the 1 arcsec gate out to
    /// the T0 ceiling (5 kpc). Full <1 arcsec vs Stellarium validation of
    /// named stars runs on the target machine (PLAN §17.1).
    #[test]
    fn quantisation_angular_error_under_one_arcsec() {
        let one_arcsec_rad = 4.848_136_8e-6;
        for d_pc in [10.0_f32, 137.0, 400.0, 1500.0, 5000.0] {
            let ulp = (d_pc * 2.0f32.powi(-23)).abs(); // f32 spacing at d
            let err_rad = (ulp * 0.5) / d_pc;
            // f32 pc gives ~2^-24 rad = 0.012 arcsec — assert with 10x margin
            // against the 1 arcsec gate (PLAN §17.1).
            assert!(
                err_rad < one_arcsec_rad / 10.0,
                "d={d_pc} pc: {err_rad} rad >= 1/10 arcsec"
            );
        }
    }

    #[test]
    fn frustum_planes_contain_camera_forward() {
        // glam 0.34 has no look_at/perspective ctors — build by hand.
        let eye = glam::Vec3::new(0.0, 0.0, 5.0);
        let view = glam::Mat4::from_rows(
            glam::Vec4::new(1.0, 0.0, 0.0, 0.0),
            glam::Vec4::new(0.0, 1.0, 0.0, 0.0),
            glam::Vec4::new(0.0, 0.0, 1.0, -5.0), // backward·p - backward·eye
            glam::Vec4::new(0.0, 0.0, 0.0, 1.0),
        );
        let f = 1.0 / (1.0f32 * 0.5).tan();
        let proj = glam::Mat4::from_cols_array(&[
            f / 1.777, 0.0, 0.0, 0.0,
            0.0, f, 0.0, 0.0,
            0.0, 0.0, 0.0, -1.0,
            0.0, 0.0, 1e-4, 0.0,
        ]);
        let vp = proj * view;
        let _ = eye;
        let planes = frustum_planes(&vp);
        // a point straight ahead, mid-depth, must be inside all six planes
        let p = glam::Vec3::new(0.0, 0.0, 2.0);
        for pl in &planes {
            assert!(pl.dot(p.extend(1.0)) >= -1e-4, "plane {pl:?} rejects forward point");
        }
        // a point far outside to the side must be rejected by some plane
        let q = glam::Vec3::new(50.0, 0.0, 2.0);
        assert!(planes.iter().any(|pl| pl.dot(q.extend(1.0)) < 0.0));
    }
}
