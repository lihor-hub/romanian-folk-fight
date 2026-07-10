//! Asset manifest sidecars and the `cargo xtask assets check` validator
//! (#167, a child of #141), runtime-reference and image-integrity
//! validation (#185, a child of #141) in `validate/`, and the deterministic
//! static review gallery (#197, a child of #141) in `gallery/`. Owned end
//! to end by this module.
//!
//! See `schema.rs` for the sidecar schema/version, `aggregate.rs` for the
//! aggregation/coverage/duplicate-detection pass, `validate/` for the #185
//! rules, and `gallery/` for the #197 `cargo xtask assets review` gallery.
//! `xtask/README.md` documents the command surface and known limitations.

pub mod aggregate;
pub mod credits;
pub mod diagnostics;
pub mod discover;
pub mod gallery;
pub mod schema;
pub mod validate;

use std::fs;
use std::path::Path;

use aggregate::{Aggregate, CoverageSummary};
use diagnostics::Diagnostic;

/// Everything `cargo xtask assets check` needs to report: the aggregate
/// (including its own diagnostics), the credits cross-check diagnostics,
/// the #185 validation diagnostics, and the coverage summary.
pub struct CheckResult {
    pub aggregate: Aggregate,
    pub credits_diagnostics: Vec<Diagnostic>,
    pub validate_diagnostics: Vec<Diagnostic>,
    pub coverage: CoverageSummary,
}

impl CheckResult {
    pub fn all_diagnostics(&self) -> Vec<&Diagnostic> {
        self.aggregate
            .diagnostics
            .iter()
            .chain(self.credits_diagnostics.iter())
            .chain(self.validate_diagnostics.iter())
            .collect()
    }

    pub fn is_clean(&self) -> bool {
        self.all_diagnostics().is_empty()
    }
}

/// Runs the full check: builds the aggregate from every sidecar under
/// `assets_root`, cross-checks `assets/CREDITS.md` at `credits_path`
/// against it, then runs #185's runtime-reference/image-integrity rules
/// (which additionally need `workspace_root` to locate `src/` and
/// `index.html`).
pub fn run_check(assets_root: &Path, credits_path: &Path, workspace_root: &Path) -> CheckResult {
    let built = aggregate::build(assets_root);
    let coverage = aggregate::summarize(assets_root, &built);

    let credits_diagnostics = match fs::read_to_string(credits_path) {
        Ok(text) => credits::check(&text, &built.records),
        Err(err) => vec![Diagnostic::Undecodable {
            sidecar: credits_path.to_path_buf(),
            error: format!("failed to read assets/CREDITS.md: {err}"),
        }],
    };

    let validate_diagnostics = validate::run(workspace_root, assets_root, &built);

    CheckResult {
        aggregate: built,
        credits_diagnostics,
        validate_diagnostics,
        coverage,
    }
}
