//! `cargo xtask pre-push` -- the full repository-native gate.
//!
//! Runs, in order, stopping at the first failure: `cargo fmt --check`,
//! `cargo clippy -D warnings`, `cargo test` (the whole suite), then the
//! build-feature matrix (native/release/wasm, no `dev` leakage). This is the
//! same gate the git `pre-push` hook and CI expect a clean tree to pass.

use std::process::Command;

use super::check_cmd;
use crate::process::{
    StepError, effective_budget_ms, print_summary, run_step, total_elapsed, warn_if_over_budget,
};

pub const ABOUT: &str = "Full repository gate: fmt check, clippy -D warnings, cargo test, build-matrix. Stops at the first failure.";

/// Target warm-run budget (milliseconds) for the whole gate: the
/// player-experience rework plan's feedback-loop contract names this budget
/// explicitly ("Full pre-push gate": 10 minutes). Overridable via
/// `XTASK_BUDGET_MS`; see `docs/feedback-budgets.md` for the measured
/// cold/warm timings.
const PRE_PUSH_BUDGET_MS: u64 = 10 * 60 * 1000;

pub fn run() -> Result<(), StepError> {
    let mut reports = Vec::with_capacity(6);
    reports.push(run_step("fmt check", fmt_check_cmd())?);
    reports.push(run_step("clippy -D warnings", clippy_cmd())?);
    reports.push(run_step("cargo test", cargo_test_cmd())?);
    reports.extend(check_cmd::build_matrix()?);
    println!("\ncargo xtask pre-push: all gates passed (fmt, clippy, cargo test, build-matrix).");
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

/// `--workspace`: with a `[package]` + `[workspace]` root manifest, plain
/// `cargo test` defaults to the root package only, which would silently
/// skip xtask's own unit tests. The repository-native gate should catch a
/// regression in the dispatcher itself too.
fn cargo_test_cmd() -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(["test", "--workspace"]);
    cmd
}
