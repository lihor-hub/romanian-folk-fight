//! Asset manifest sidecars and the `cargo xtask assets check` validator
//! (#167, a child of #141). Owned end to end by this module -- later,
//! independently owned #141 children (image-integrity rules, the review
//! gallery) add their own modules alongside this one rather than editing it.
//!
//! See `schema.rs` for the sidecar schema/version and `aggregate.rs` for the
//! aggregation/coverage/duplicate-detection pass. `xtask/README.md` documents
//! the command surface and known limitations.

pub mod aggregate;
pub mod credits;
pub mod diagnostics;
pub mod discover;
pub mod schema;

use std::fs;
use std::path::Path;

use aggregate::{Aggregate, CoverageSummary};
use diagnostics::Diagnostic;

/// Everything `cargo xtask assets check` needs to report: the aggregate
/// (including its own diagnostics), the credits cross-check diagnostics,
/// and the coverage summary.
pub struct CheckResult {
    pub aggregate: Aggregate,
    pub credits_diagnostics: Vec<Diagnostic>,
    pub coverage: CoverageSummary,
}

impl CheckResult {
    pub fn all_diagnostics(&self) -> Vec<&Diagnostic> {
        self.aggregate
            .diagnostics
            .iter()
            .chain(self.credits_diagnostics.iter())
            .collect()
    }

    pub fn is_clean(&self) -> bool {
        self.all_diagnostics().is_empty()
    }
}

/// Runs the full check: builds the aggregate from every sidecar under
/// `assets_root`, then cross-checks `assets/CREDITS.md` at `credits_path`
/// against it.
pub fn run_check(assets_root: &Path, credits_path: &Path) -> CheckResult {
    let built = aggregate::build(assets_root);
    let coverage = aggregate::summarize(assets_root, &built);

    let credits_diagnostics = match fs::read_to_string(credits_path) {
        Ok(text) => credits::check(&text, &built.records),
        Err(err) => vec![Diagnostic::Undecodable {
            sidecar: credits_path.to_path_buf(),
            error: format!("failed to read assets/CREDITS.md: {err}"),
        }],
    };

    CheckResult {
        aggregate: built,
        credits_diagnostics,
        coverage,
    }
}
