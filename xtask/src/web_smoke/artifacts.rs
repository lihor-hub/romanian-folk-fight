//! Per-run artifact directory conventions for browser-smoke scenarios.
//!
//! Layered under xtask's existing `target/xtask-artifacts/` convention (see
//! `crate::process::artifacts_dir`) rather than inventing a new location:
//!
//! ```text
//! target/xtask-artifacts/web-smoke/<scenario>/<checkpoint>/
//!   screenshot.png   -- the checkpoint's captured screenshot (PNG, DPR 1)
//!   console.log      -- every browser console message observed
//!   network.log      -- every request/response observed: status + URL
//!   viewport.log      -- requested vs. measured viewport/document scroll extents
//!   server.log        -- the ephemeral static server's request log for the run
//! ```
//!
//! Nothing here is cleaned up automatically (matching `process::artifacts_dir`'s
//! own "most recent run of every step is always on disk" convention), so a
//! failure's full diagnostics survive after the run ends. Paths under this
//! directory are printed by every scenario, on both success and failure.

use std::path::{Path, PathBuf};

/// `target/xtask-artifacts/web-smoke/<scenario>/`.
pub fn scenario_dir(scenario: &str) -> PathBuf {
    crate::process::artifacts_dir()
        .join("web-smoke")
        .join(scenario)
}

/// `target/xtask-artifacts/web-smoke/<scenario>/<checkpoint>/`, created if
/// it doesn't exist yet.
pub fn checkpoint_dir(scenario: &str, checkpoint: &str) -> std::io::Result<PathBuf> {
    let dir = scenario_dir(scenario).join(checkpoint);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Writes `contents` to `dir/name`, printing the path so a failing (or
/// successful) run always shows where its diagnostics live.
pub fn write_artifact(
    dir: &Path,
    name: &str,
    contents: impl AsRef<[u8]>,
) -> std::io::Result<PathBuf> {
    let path = dir.join(name);
    std::fs::write(&path, contents)?;
    println!("    artifact: {}", path.display());
    Ok(path)
}
