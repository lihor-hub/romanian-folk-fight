//! `cargo xtask check build-matrix` -- the native build-feature matrix.
//!
//! Root-owned: covers only the native/release/wasm `cargo check` matrix and
//! its `dev`-feature-leakage guarantee. Asset (#141) and browser-smoke
//! (#144) checks are separate, independently owned modules.

use std::process::Command;

use crate::process::{
    StepError, StepReport, effective_budget_ms, print_summary, run_step, total_elapsed,
    warn_if_over_budget,
};

pub const ABOUT: &str = "Native build-feature matrix (native, release, wasm; no `dev` leakage).";

pub const SUBCOMMANDS: &[(&str, &str)] = &[(
    "build-matrix",
    "cargo check across plain native, --release, and --target wasm32-unknown-unknown, none with the `dev` feature.",
)];

/// Target warm-run budget (milliseconds) for the whole three-step matrix
/// (native + release + wasm `cargo check`), compared against the *summed*
/// elapsed time of all three steps rather than each individually. The
/// player-experience rework plan's feedback-loop contract does not name a
/// budget for this loop directly (it only lists focused-test/asset/browser
/// budgets); this is this issue's own initial target, set from the measured
/// warm timing in `docs/feedback-budgets.md` with headroom, and revisable.
/// Overridable via `XTASK_BUDGET_MS`.
const BUILD_MATRIX_BUDGET_MS: u64 = 240_000;

pub fn run(sub: &str) -> Result<(), StepError> {
    match sub {
        "build-matrix" => {
            let reports = build_matrix()?;
            print_summary(&reports);
            warn_if_over_budget(
                "check build-matrix",
                total_elapsed(&reports),
                effective_budget_ms(BUILD_MATRIX_BUDGET_MS),
            );
            Ok(())
        }
        other => unreachable!("dispatch validates subcommands before calling run; got {other}"),
    }
}

/// Runs all three checks in order, stopping at the first failure, and
/// returns each step's report. `pub(crate)` so `pre_push` can reuse this
/// exact implementation (and fold its reports into the full gate's summary)
/// instead of duplicating the matrix.
///
/// None of the three ever passes `--features dev`: `dev` (Bevy dynamic
/// linking) exists only to speed up local `cargo run --features dev`
/// iteration and must never leak into a plain native, release, or wasm
/// build (a dynamically linked release/wasm artifact would not run
/// standalone). Omitting the flag is itself the guarantee -- there is no
/// default feature list in the root `Cargo.toml` that could pull `dev` in
/// implicitly.
pub(crate) fn build_matrix() -> Result<Vec<StepReport>, StepError> {
    Ok(vec![
        run_step("check native (no dev feature)", cargo_check(&[]))?,
        run_step(
            "check release (no dev feature)",
            cargo_check(&["--release"]),
        )?,
        run_step(
            "check wasm32-unknown-unknown (no dev feature)",
            cargo_check(&["--target", "wasm32-unknown-unknown"]),
        )?,
    ])
}

fn cargo_check(extra_args: &[&str]) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.arg("check");
    cmd.args(extra_args);
    cmd
}
