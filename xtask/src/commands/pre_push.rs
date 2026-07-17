//! `cargo xtask pre-push` -- the full repository-native gate.
//!
//! Runs, in order, stopping at the first failure: `cargo fmt --check`,
//! `cargo clippy -D warnings`, `cargo test` (the whole suite), the same
//! clippy/test pair again with `--features review` (#294), then the
//! build-feature matrix (native/release/wasm, no `dev`/`review` leakage).
//! This is the same gate the git `pre-push` hook and CI expect a clean tree
//! to pass.
//!
//! ## Why a review-feature clippy/test pair (#294)
//!
//! `src/review/mod.rs` (the deterministic browser-harness seam, #187) is
//! `#![cfg(feature = "review")]`-gated with no matching `cfg(test)` escape
//! hatch, so its 23 tests -- and the module itself -- are invisible to a
//! plain `cargo clippy`/`cargo test`: the code is not compiled at all
//! without `--features review`. Before this leg existed, a break in that
//! seam compiled green through this gate every time and only surfaced when
//! someone separately ran the dedicated review-feature web-smoke build
//! (`cargo xtask web-smoke --scenario gold-journey`, see
//! `xtask/src/web_smoke/gold_journey.rs`) or ran the feature's clippy/test
//! commands by hand.
//!
//! The `review` feature is additive-only: nothing in the crate is gated
//! `cfg(not(feature = "review"))`, so `--features review` is a strict
//! superset -- everything the default-feature steps already check, plus the
//! review module. It would be tempting to fold `--features review` into the
//! existing `clippy_cmd`/`cargo_test_cmd` and drop the default-feature
//! invocation entirely, but that would stop ever running clippy/`cargo test`
//! against the exact feature set every other build in this repo uses (a
//! plain `cargo build`, `cargo build --release`, and the ordinary `trunk
//! build --release` all never enable `review`, see `Cargo.toml`) -- a
//! regression only visible without the feature (e.g. an item that becomes
//! dead code once its sole caller lives behind `#[cfg(feature =
//! "review")]`) would go uncaught. Running review as its own extra pair
//! keeps both configurations covered. Measured cost on a warm worktree
//! (bevy's own dependency graph already compiled by the default-feature
//! steps just before): the review clippy/test pair adds low single-digit to
//! ~25 seconds, since only this crate's own compilation unit needs
//! rebuilding under the different feature flag -- see the PR description for
//! the full before/after transcript.

use std::process::Command;

use super::check_cmd;
use crate::process::{
    StepError, effective_budget_ms, print_summary, run_step, total_elapsed, warn_if_over_budget,
};

pub const ABOUT: &str = "Full repository gate: fmt check, clippy -D warnings (default + review feature), cargo test (default + review feature), build-matrix. Stops at the first failure.";

/// Target warm-run budget (milliseconds) for the whole gate: the
/// player-experience rework plan's feedback-loop contract names this budget
/// explicitly ("Full pre-push gate": 10 minutes). Overridable via
/// `XTASK_BUDGET_MS`; see `docs/feedback-budgets.md` for the measured
/// cold/warm timings.
const PRE_PUSH_BUDGET_MS: u64 = 10 * 60 * 1000;

pub fn run() -> Result<(), StepError> {
    let mut reports = Vec::with_capacity(8);
    reports.push(run_step("fmt check", fmt_check_cmd())?);
    reports.push(run_step("clippy -D warnings", clippy_cmd())?);
    reports.push(run_step(
        "clippy --features review -D warnings",
        clippy_review_cmd(),
    )?);
    reports.push(run_step("cargo test", cargo_test_cmd())?);
    reports.push(run_step(
        "cargo test --features review",
        cargo_test_review_cmd(),
    )?);
    reports.extend(check_cmd::build_matrix()?);
    println!(
        "\ncargo xtask pre-push: all gates passed (fmt, clippy, cargo test, review-feature clippy+test, build-matrix)."
    );
    print_summary(&reports);
    warn_if_over_budget(
        "pre-push",
        total_elapsed(&reports),
        effective_budget_ms(PRE_PUSH_BUDGET_MS),
    );
    Ok(())
}

fn fmt_check_cmd() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(["fmt", "--all", "--", "--check"]);
    cmd
}

/// `--workspace`: without it, `cargo clippy` on this `[package]` +
/// `[workspace]` root manifest would only lint the root package and never
/// see xtask's own code.
fn clippy_cmd() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]);
    cmd
}

/// #294: same command as [`clippy_cmd`] plus `--features review`, so
/// `src/review/mod.rs` (otherwise entirely absent from compilation, see this
/// module's doc comment) is linted too. `--workspace` for the same reason as
/// `clippy_cmd` -- xtask has no `review` feature of its own, but omitting
/// `--workspace` here would silently drop xtask from this leg as well; Cargo
/// scopes `--features review` to only the workspace members that declare
/// it, so passing it alongside `--workspace` does not error on xtask's
/// behalf.
fn clippy_review_cmd() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--features",
        "review",
        "--",
        "-D",
        "warnings",
    ]);
    cmd
}

/// `--workspace`: with a `[package]` + `[workspace]` root manifest, plain
/// `cargo test` defaults to the root package only, which would silently
/// skip xtask's own unit tests. The repository-native gate should catch a
/// regression in the dispatcher itself too.
fn cargo_test_cmd() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(["test", "--workspace"]);
    cmd
}

/// #294: same command as [`cargo_test_cmd`] plus `--features review`, so
/// `src/review/mod.rs`'s 23 tests actually run. See [`clippy_review_cmd`]
/// for why `--workspace` and `--features review` combine safely here too.
fn cargo_test_review_cmd() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(["test", "--workspace", "--features", "review"]);
    cmd
}
