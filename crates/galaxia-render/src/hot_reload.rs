//! WGSL hot-reload (PLAN It.0 key work: "You will recompile shaders a few
//! hundred times; every second of latency there compounds across months").
//!
//! mtime polling — no notify dependency, no watcher thread. `changed()` is
//! cheap enough to call once per frame. On change the caller calls
//! `StarPass::try_rebuild`, which validates via naga BEFORE swapping, so a
//! broken edit never kills the running app.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct ShaderWatcher {
    path: PathBuf,
    last_mtime: Option<SystemTime>,
    last_source: String,
}

impl ShaderWatcher {
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let source = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("cannot read shader {}: {e}", path.display()))?;
        let last_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
        Ok(Self {
            path,
            last_mtime,
            last_source: source,
        })
    }

    /// Returns the new source when the file changed on disk since last check.
    pub fn changed(&mut self) -> Option<String> {
        let meta = std::fs::metadata(&self.path).ok()?;
        let mtime = meta.modified().ok()?;
        // mtime granularity can be coarse (1s on some filesystems); also
        // compare content length as a cheap second signal.
        if Some(mtime) == self.last_mtime {
            return None;
        }
        let source = std::fs::read_to_string(&self.path).ok()?;
        if source == self.last_source {
            self.last_mtime = Some(mtime);
            return None;
        }
        self.last_mtime = Some(mtime);
        self.last_source = source.clone();
        Some(source)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
