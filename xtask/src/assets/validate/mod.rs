//! Runtime-reference and image-integrity validation (#185, a child of
//! #141). Builds on the #167 sidecar/derived-aggregate contract
//! (`super::aggregate`) without changing it: this module only reads the
//! aggregate `cargo xtask assets check` already builds and adds its own
//! diagnostics alongside #167's.
//!
//! - [`refs`]: scans production Rust (`src/**/*.rs`) and `index.html` for
//!   asset references and cross-checks each one against the aggregate.
//! - [`bounds`]: validates `crop`/`pivot`/`display` metadata bounds and
//!   source/display aspect distortion, purely from sidecar metadata.
//! - [`image_checks`]: decodes real pixels (via the `image` crate) to
//!   check recorded-vs-actual dimensions, empty alpha, and chroma-key
//!   fringe.
//! - [`rust_scan`]: the shared low-level Rust-source tokenizer `refs`
//!   uses to exclude `#[cfg(test)]`-only string literals.

pub mod bounds;
pub mod catalog;
pub mod image_checks;
pub mod refs;
mod rust_scan;

use std::path::Path;

use super::aggregate::Aggregate;
use super::diagnostics::Diagnostic;

/// Runs every #185 validation rule and returns every diagnostic found.
/// Always runs to completion (each sub-check collects its own
/// diagnostics independently), matching #167's "one run surfaces every
/// problem" convention. `assets_root` and `workspace_root` are both
/// absolute: `assets_root` resolves `aggregate`'s (assets-root-relative)
/// `full_path`s for decoding, `workspace_root` locates `src/` and
/// `index.html` for reference discovery.
pub fn run(workspace_root: &Path, assets_root: &Path, aggregate: &Aggregate) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(refs::check(workspace_root, aggregate));
    diagnostics.extend(catalog::check(assets_root, aggregate));
    diagnostics.extend(bounds::check(&aggregate.records));
    diagnostics.extend(image_checks::check(assets_root, &aggregate.records));
    diagnostics
}
