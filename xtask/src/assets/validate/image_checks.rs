//! Pixel-level image-integrity validation: recorded-vs-actual dimensions,
//! empty alpha, and chroma-key fringe (#185, a child of #141). The only
//! module in this crate that decodes real pixels (via the `image` crate,
//! `png`-only -- see `xtask/Cargo.toml`).

use std::collections::BTreeSet;
use std::path::Path;

use image::GenericImageView;

use super::super::aggregate::ResolvedRecord;
use super::super::diagnostics::Diagnostic;
use super::super::schema::{Kind, Status};

/// Chroma-key colors this check treats as "obvious fringe": the two
/// classic background-removal keys (pure magenta and pure green-screen
/// green). Documented explicitly rather than inferred from image content.
const CHROMA_KEY_COLORS: [[u8; 3]; 2] = [
    [255, 0, 255], // magenta
    [0, 255, 0],   // green screen
];

/// Maximum per-channel (Chebyshev/max-component) distance from a
/// `CHROMA_KEY_COLORS` entry, on the 0-255 scale, for a pixel to count as
/// fringe. ~16% of the channel range: generous enough to catch
/// anti-aliased/lightly-compressed near-matches, tight enough that no
/// ordinary saturated art color (this project's palette tops out at
/// `#7a1f1f` deep red and `#c9a227` gold, both far from magenta/green --
/// see `docs/art-direction.md`) is caught by accident.
const CHROMA_KEY_TOLERANCE: u8 = 40;

/// Minimum alpha (0-255) for a chroma-key-colored pixel to count as a
/// *visible* fringe violation. Empirically, every chroma-key-colored pixel
/// found across the current runtime PNG inventory sits at alpha <= 2/255
/// (~0.8% opacity) -- residual background-removal dust, invisible at any
/// real render scale even under linear-filtered upscaling (measured while
/// building this check; see the PR description for the full scan). This
/// floor sits comfortably above that (~3%), so it fails anything a player
/// could plausibly perceive without chasing sub-perceptible noise that
/// isn't actionable.
const CHROMA_KEY_ALPHA_FLOOR: u8 = 8;

/// Runs dimension/alpha/fringe checks over every image record in the
/// aggregate. `assets_root` resolves each record's `full_path` (which,
/// like every path in `aggregate::ResolvedRecord`, is relative to
/// `assets_root`, not absolute or cwd-relative) to an openable path. Each
/// distinct file is decoded at most once.
pub fn check(assets_root: &Path, records: &[ResolvedRecord]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut decoded_paths = BTreeSet::new();

    for record in records {
        if record.record.kind != Kind::Image {
            continue;
        }
        if is_svg(&record.full_path) {
            continue; // resolution-independent; no pixels to check.
        }
        if !decoded_paths.insert(record.full_path.clone()) {
            // Same file covered by more than one record -- already
            // reported as `Diagnostic::DuplicatePath` by `aggregate.rs`.
            continue;
        }

        let absolute_path = assets_root.join(&record.full_path);
        let decoded = match image::open(&absolute_path) {
            Ok(img) => img,
            Err(err) => {
                diagnostics.push(Diagnostic::ImageDecodeError {
                    sidecar: record.sidecar.clone(),
                    id: record.record.id.clone(),
                    path: record.full_path.clone(),
                    error: err.to_string(),
                });
                continue;
            }
        };

        let (actual_w, actual_h) = decoded.dimensions();
        if let Some(recorded) = record.record.dimensions
            && recorded != [actual_w, actual_h]
        {
            diagnostics.push(Diagnostic::DimensionMismatch {
                sidecar: record.sidecar.clone(),
                id: record.record.id.clone(),
                recorded,
                actual: [actual_w, actual_h],
            });
        }

        if record.record.status != Status::Runtime {
            // Empty-alpha and chroma-key fringe are runtime-only checks
            // (#185's acceptance criteria: "a runtime PNG...", "runtime
            // images exceeding it..."). `source`/`legacy` art predates the
            // production pipeline and isn't rendered.
            continue;
        }

        let rgba = decoded.to_rgba8();
        let mut any_visible = false;
        let mut fringe_count = 0usize;
        let mut fringe_max_alpha = 0u8;
        for pixel in rgba.pixels() {
            let [r, g, b, a] = pixel.0;
            if a > 0 {
                any_visible = true;
            }
            if a >= CHROMA_KEY_ALPHA_FLOOR && is_chroma_key_color([r, g, b]) {
                fringe_count += 1;
                fringe_max_alpha = fringe_max_alpha.max(a);
            }
        }

        if !any_visible {
            diagnostics.push(Diagnostic::EmptyAlpha {
                sidecar: record.sidecar.clone(),
                id: record.record.id.clone(),
                path: record.full_path.clone(),
            });
        }
        if fringe_count > 0 {
            diagnostics.push(Diagnostic::ChromaKeyFringe {
                sidecar: record.sidecar.clone(),
                id: record.record.id.clone(),
                path: record.full_path.clone(),
                count: fringe_count,
                max_alpha: fringe_max_alpha,
            });
        }
    }

    diagnostics
}

fn is_svg(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("svg"))
        .unwrap_or(false)
}

fn is_chroma_key_color(rgb: [u8; 3]) -> bool {
    CHROMA_KEY_COLORS
        .iter()
        .any(|key| chroma_distance(rgb, *key) <= CHROMA_KEY_TOLERANCE)
}

fn chroma_distance(a: [u8; 3], b: [u8; 3]) -> u8 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::schema::{Category, Record};
    use image::{ImageBuffer, Rgba};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempPng {
        path: PathBuf,
    }

    impl TempPng {
        fn write(name: &str, pixels: &[(u8, u8, u8, u8)], width: u32, height: u32) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "xtask-assets-validate-image-checks-{}-{}-{:?}",
                name,
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&dir).unwrap();
            let path = dir.join("fixture.png");
            let buf: ImageBuffer<Rgba<u8>, Vec<u8>> =
                ImageBuffer::from_fn(width, height, |x, y| {
                    let (r, g, b, a) = pixels[(y * width + x) as usize];
                    Rgba([r, g, b, a])
                });
            buf.save(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TempPng {
        fn drop(&mut self) {
            if let Some(dir) = self.path.parent() {
                let _ = fs::remove_dir_all(dir);
            }
        }
    }

    fn record(path: PathBuf, dimensions: Option<[u32; 2]>, status: Status) -> ResolvedRecord {
        ResolvedRecord {
            sidecar: PathBuf::from("assets/fixtures/manifest.toml"),
            full_path: path,
            record: Record {
                id: "fixtures.fixture".to_string(),
                path: "fixture.png".to_string(),
                kind: Kind::Image,
                category: Category::Sprite,
                status,
                provenance: "repo-generated".to_string(),
                license: "CC0 1.0".to_string(),
                generator: Some("test".to_string()),
                source_sheet: None,
                license_file: None,
                dimensions,
                sampler: Some(crate::assets::schema::Sampler::Linear),
                attachment: None,
                pivot: None,
                display: None,
                crop: None,
            },
        }
    }

    #[test]
    fn a_normal_opaque_image_is_clean() {
        let png = TempPng::write("clean", &[(10, 10, 10, 255); 4], 2, 2);
        let records = vec![record(png.path.clone(), Some([2, 2]), Status::Runtime)];
        let diagnostics = check(Path::new(""), &records);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_recorded_dimension_mismatch_is_flagged() {
        let png = TempPng::write("mismatch", &[(10, 10, 10, 255); 4], 2, 2);
        let records = vec![record(png.path.clone(), Some([99, 99]), Status::Runtime)];
        let diagnostics = check(Path::new(""), &records);
        assert!(diagnostics.iter().any(|d| matches!(
            d,
            Diagnostic::DimensionMismatch { recorded, actual, .. }
                if *recorded == [99, 99] && *actual == [2, 2]
        )));
    }

    #[test]
    fn a_fully_transparent_runtime_image_is_flagged() {
        let png = TempPng::write("empty-alpha", &[(0, 0, 0, 0); 4], 2, 2);
        let records = vec![record(png.path.clone(), Some([2, 2]), Status::Runtime)];
        let diagnostics = check(Path::new(""), &records);
        assert!(
            diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::EmptyAlpha { .. }))
        );
    }

    #[test]
    fn a_fully_transparent_source_image_is_not_flagged() {
        // Empty-alpha is a runtime-only rule (#185).
        let png = TempPng::write("empty-alpha-source", &[(0, 0, 0, 0); 4], 2, 2);
        let records = vec![record(png.path.clone(), Some([2, 2]), Status::Source)];
        assert!(check(Path::new(""), &records).is_empty());
    }

    #[test]
    fn a_visible_magenta_fringe_pixel_is_flagged() {
        let mut pixels = vec![(10, 10, 10, 255); 3];
        pixels.push((255, 0, 255, 200)); // clearly visible chroma-key remnant
        let png = TempPng::write("fringe", &pixels, 2, 2);
        let records = vec![record(png.path.clone(), Some([2, 2]), Status::Runtime)];
        let diagnostics = check(Path::new(""), &records);
        assert!(diagnostics.iter().any(
            |d| matches!(d, Diagnostic::ChromaKeyFringe { count, max_alpha, .. } if *count == 1 && *max_alpha == 200)
        ));
    }

    #[test]
    fn a_sub_floor_alpha_magenta_remnant_is_not_flagged() {
        // Matches the real, measured current-inventory state: chroma-key
        // remnants at alpha <= 2/255 are below CHROMA_KEY_ALPHA_FLOOR.
        let mut pixels = vec![(10, 10, 10, 255); 3];
        pixels.push((255, 0, 255, 1));
        let png = TempPng::write("sub-floor-fringe", &pixels, 2, 2);
        let records = vec![record(png.path.clone(), Some([2, 2]), Status::Runtime)];
        assert!(check(Path::new(""), &records).is_empty());
    }

    #[test]
    fn an_undecodable_image_is_flagged_without_panicking() {
        let dir = std::env::temp_dir().join(format!(
            "xtask-assets-validate-image-checks-corrupt-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("corrupt.png");
        fs::write(&path, b"not a real png").unwrap();
        let records = vec![record(path, Some([2, 2]), Status::Runtime)];
        let diagnostics = check(Path::new(""), &records);
        assert!(
            diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::ImageDecodeError { .. }))
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
