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
//! **not**, by default, fail the scenario: the original (#168) explicit
//! failure conditions are console/page errors, missing required assets,
//! unexpected scrolling, and clipping (see `cold_menu`'s assertions) --
//! pixel-perfect image diffing is left to human review of the (git-diffable)
//! baseline PNG after `--update-baselines`, rather than a second, flakier
//! pass/fail gate on top of the real assertions (anti-aliasing/font-hinting
//! differences across otherwise-matching environments would make a hard
//! pixel-diff gate noisy even with forced software rendering).
//!
//! ## `--strict-visual` (#198)
//!
//! That default stays the policy for ordinary local/CI runs. Passing
//! `--strict-visual` (or setting `XTASK_WEB_SMOKE_STRICT_VISUAL=1`) turns a
//! `BaselineOutcome::Differs` into an explicit checkpoint failure -- an
//! opt-in, human-triggered gate for a reviewer who wants "no unreviewed
//! pixel drift" enforced, without making that the default noisy-CI
//! behavior. See `web_smoke_cmd`'s CLI parsing for how the flag/env reach
//! each scenario.
//!
//! ## Diff triplet artifacts (#198)
//!
//! Whenever a checkpoint's screenshot differs from its accepted baseline --
//! regardless of `--strict-visual` -- [`write_diff_triplet`] writes
//! `actual.png`/`expected.png`/`diff.png` into that checkpoint's own
//! `target/xtask-artifacts/web-smoke/<scenario>/<checkpoint>/baseline-diff/`
//! directory, so CI can upload a focused, reviewable bundle (see
//! `.github/workflows/web-smoke.yml`) instead of requiring a reviewer to
//! diff the raw screenshot against the committed baseline by hand.

use std::path::{Path, PathBuf};

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

/// The three paths [`write_diff_triplet`] writes.
pub struct DiffTriplet {
    pub actual: PathBuf,
    pub expected: PathBuf,
    pub diff: PathBuf,
}

impl DiffTriplet {
    pub fn describe(&self) -> String {
        format!(
            "actual={} expected={} diff={}",
            self.actual.display(),
            self.expected.display(),
            self.diff.display()
        )
    }
}

/// Writes `actual.png` (the freshly captured screenshot), `expected.png`
/// (the accepted baseline), and `diff.png` (a magenta-highlighted diff
/// visualization -- matching pixels rendered grayscale, differing or
/// out-of-bounds pixels rendered pure magenta) into
/// `checkpoint_dir/baseline-diff/`. Callers only reach this after
/// `handle` already reported `BaselineOutcome::Differs`, so the baseline
/// file is known to exist; if reading it races with a concurrent
/// `--update-baselines` run (not a supported concurrent usage) this simply
/// returns an `io::Error`, which callers treat as non-fatal (the
/// actual/expected comparison already happened via `handle`).
pub fn write_diff_triplet(
    scenario: &str,
    checkpoint: &str,
    screenshot_png: &[u8],
    checkpoint_dir: &Path,
) -> std::io::Result<DiffTriplet> {
    let expected_bytes = std::fs::read(baseline_path(scenario, checkpoint))?;
    write_diff_triplet_bytes(&expected_bytes, screenshot_png, checkpoint_dir)
}

/// The actual diff-writing logic, factored out of [`write_diff_triplet`] so
/// unit tests can exercise it against fabricated in-memory bytes and a
/// scratch directory, without reading/writing the real
/// `tests/visual/baselines/` tree.
fn write_diff_triplet_bytes(
    expected_bytes: &[u8],
    screenshot_png: &[u8],
    checkpoint_dir: &Path,
) -> std::io::Result<DiffTriplet> {
    let dir = checkpoint_dir.join("baseline-diff");
    std::fs::create_dir_all(&dir)?;

    let actual_path = dir.join("actual.png");
    std::fs::write(&actual_path, screenshot_png)?;

    let expected_path = dir.join("expected.png");
    std::fs::write(&expected_path, expected_bytes)?;

    let diff_path = dir.join("diff.png");
    if let (Ok(expected_img), Ok(actual_img)) = (
        image::load_from_memory(expected_bytes),
        image::load_from_memory(screenshot_png),
    ) {
        let _ = render_diff_image(&expected_img, &actual_img).save(&diff_path);
    }

    for path in [&actual_path, &expected_path, &diff_path] {
        println!("    artifact: {}", path.display());
    }

    Ok(DiffTriplet {
        actual: actual_path,
        expected: expected_path,
        diff: diff_path,
    })
}

/// Matching pixels rendered grayscale (so the highlighted diffs stand out
/// against real image content rather than a flat background); differing
/// pixels -- including any pixel only one image has, when the two differ in
/// size -- rendered pure magenta.
fn render_diff_image(
    expected: &image::DynamicImage,
    actual: &image::DynamicImage,
) -> image::RgbaImage {
    let expected = expected.to_rgba8();
    let actual = actual.to_rgba8();
    let width = expected.width().max(actual.width());
    let height = expected.height().max(actual.height());
    const MAGENTA: image::Rgba<u8> = image::Rgba([255, 0, 255, 255]);

    let mut out = image::RgbaImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let pixel = match (
                expected.get_pixel_checked(x, y),
                actual.get_pixel_checked(x, y),
            ) {
                (Some(e), Some(a)) if e == a => {
                    let gray = ((u32::from(e[0]) + u32::from(e[1]) + u32::from(e[2])) / 3) as u8;
                    image::Rgba([gray, gray, gray, 255])
                }
                _ => MAGENTA,
            };
            out.put_pixel(x, y, pixel);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
        let mut img = image::RgbaImage::new(width, height);
        for p in img.pixels_mut() {
            *p = image::Rgba(pixel);
        }
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn write_diff_triplet_writes_actual_expected_and_a_decodable_diff() {
        let tmp = std::env::temp_dir().join(format!(
            "rff-web-smoke-diff-triplet-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let expected = png_bytes(2, 2, [10, 10, 10, 255]);
        let actual = png_bytes(2, 2, [200, 0, 0, 255]);
        let triplet = write_diff_triplet_bytes(&expected, &actual, &tmp).unwrap();

        assert!(triplet.actual.exists());
        assert!(triplet.expected.exists());
        assert!(triplet.diff.exists());
        assert!(image::open(&triplet.diff).is_ok(), "diff.png must decode");
        assert_eq!(std::fs::read(&triplet.actual).unwrap(), actual);
        assert_eq!(std::fs::read(&triplet.expected).unwrap(), expected);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn render_diff_image_marks_differing_pixels_magenta_and_matching_pixels_gray() {
        let expected = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            1,
            2,
            image::Rgba([60, 60, 60, 255]),
        ));
        let mut actual_buf = image::RgbaImage::new(1, 2);
        actual_buf.put_pixel(0, 0, image::Rgba([60, 60, 60, 255])); // matches
        actual_buf.put_pixel(0, 1, image::Rgba([0, 0, 0, 255])); // differs
        let actual = image::DynamicImage::ImageRgba8(actual_buf);

        let diff = render_diff_image(&expected, &actual);
        assert_eq!(*diff.get_pixel(0, 0), image::Rgba([60, 60, 60, 255]));
        assert_eq!(*diff.get_pixel(0, 1), image::Rgba([255, 0, 255, 255]));
    }
}
