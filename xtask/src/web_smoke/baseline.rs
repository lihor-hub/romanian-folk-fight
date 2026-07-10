//! Baseline screenshot policy for browser-smoke checkpoints (#144/#168).
//!
//! Baselines live at `tests/visual/baselines/<scenario>/<checkpoint>.png`,
//! checked into git and reviewed like any other change. A normal run
//! **never** writes there -- it only reads the accepted baseline (if one
//! exists) to report whether the freshly captured screenshot still matches
//! it. `--update-baselines` is the only thing that overwrites it, and only
//! for the scenario actually run (`cold-menu`'s two checkpoints, not some
//! other scenario's baselines).
//!
//! A baseline mismatch or a missing baseline is reported clearly but does
//! **not** by itself fail the scenario: the issue's explicit failure
//! conditions are console/page errors, missing required assets, unexpected
//! scrolling, and clipping (see `cold_menu`'s assertions) -- pixel-perfect
//! image diffing is left to human review of the (git-diffable) baseline PNG
//! after `--update-baselines`, rather than a second, flakier pass/fail gate
//! on top of the real assertions (anti-aliasing/font-hinting differences
//! across otherwise-matching environments would make a hard pixel-diff gate
//! noisy even with forced software rendering).

use std::path::PathBuf;

pub enum BaselineOutcome {
    /// `--update-baselines` was passed; the baseline was (over)written.
    Updated,
    /// No baseline existed yet at this path.
    Missing,
    /// The captured screenshot's bytes are byte-for-byte identical to the baseline.
    Matches,
    /// The captured screenshot differs from the baseline.
    Differs { diff_pixels: u64, total_pixels: u64 },
}

pub fn baseline_path(scenario: &str, checkpoint: &str) -> PathBuf {
    workspace_root()
        .join("tests")
        .join("visual")
        .join("baselines")
        .join(scenario)
        .join(format!("{checkpoint}.png"))
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/Cargo.toml always has a parent workspace root")
        .to_path_buf()
}

/// Compares (or, with `update: true`, overwrites) the baseline for
/// `scenario`/`checkpoint` against `screenshot_png`.
pub fn handle(
    scenario: &str,
    checkpoint: &str,
    screenshot_png: &[u8],
    update: bool,
) -> std::io::Result<BaselineOutcome> {
    let path = baseline_path(scenario, checkpoint);

    if update {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, screenshot_png)?;
        return Ok(BaselineOutcome::Updated);
    }

    if !path.exists() {
        return Ok(BaselineOutcome::Missing);
    }

    let existing = std::fs::read(&path)?;
    if existing == screenshot_png {
        return Ok(BaselineOutcome::Matches);
    }

    let (diff_pixels, total_pixels) = match (
        image::load_from_memory(&existing),
        image::load_from_memory(screenshot_png),
    ) {
        (Ok(a), Ok(b)) => diff_pixel_count(&a, &b),
        _ => (0, 0), // undecodable baseline/screenshot: still "Differs", just without a pixel count
    };
    Ok(BaselineOutcome::Differs {
        diff_pixels,
        total_pixels,
    })
}

fn diff_pixel_count(a: &image::DynamicImage, b: &image::DynamicImage) -> (u64, u64) {
    let a = a.to_rgba8();
    let b = b.to_rgba8();
    let width = a.width().min(b.width());
    let height = a.height().min(b.height());
    let mut diff = 0u64;
    for y in 0..height {
        for x in 0..width {
            if a.get_pixel(x, y) != b.get_pixel(x, y) {
                diff += 1;
            }
        }
    }
    let total = u64::from(a.width().max(b.width())) * u64::from(a.height().max(b.height()));
    (diff, total)
}
