//! `cargo xtask web-smoke --scenario <name> [--update-baselines]` -- browser
//! smoke scenarios (#144/#168): build + serve the WASM game, drive a real
//! browser against it, and verify a first-painted screen.
//!
//! Root-owned: this file is the thin CLI shim the dispatcher calls
//! (`xtask/src/commands/mod.rs` registers it exactly like `test_cmd`/
//! `check_cmd`); it owns no scenario logic itself. Everything else --
//! browser driver, ephemeral server, artifacts, baselines, and the
//! `cold-menu` scenario -- lives in `crate::web_smoke` (`xtask/src/web_smoke/`),
//! disjoint from #141's future `xtask/src/commands/assets_cmd.rs`.
//!
//! ## Why `--scenario` is the "subcommand"
//!
//! Every other command group's dispatch (see `commands::dispatch`) matches
//! `args[1]` verbatim against a fixed `SUBCOMMANDS` list and calls
//! `run(args[1])` -- it never forwards the rest of argv. That shape fits a
//! bare subcommand name (`test logic`) but not `--scenario cold-menu
//! --update-baselines`, and changing the dispatcher's forwarding behavior
//! itself would be a dispatch-convention change this module isn't allowed to
//! make (see `xtask/README.md`'s extension pattern -- only the module file
//! plus one `GROUPS` line). Registering `"--scenario"` itself as the (only)
//! recognized subcommand name satisfies the dispatcher's exact-match check
//! for the required `cargo xtask web-smoke --scenario <name>` invocation
//! without touching `commands::dispatch`; this module then reads the full
//! process argv itself (`std::env::args()`) to parse the scenario name and
//! the `--update-baselines` flag. A later scenario needs no dispatcher
//! change either: it just becomes another accepted value after
//! `--scenario`, handled by `crate::web_smoke::run_scenario`.

use crate::process::{StepError, effective_budget_ms, warn_if_over_budget};

pub const ABOUT: &str = "Browser-smoke scenarios: build+serve the WASM game and verify a first-painted screen in a real browser.";

/// Target wall-clock budget for one whole scenario run: a warm `trunk build
/// --release` plus two cold-browser checkpoints. Dominated by the release
/// wasm build (`wasm-opt -Oz` alone is minutes cold); the browser phase is
/// tens of seconds. Overridable per-invocation via `XTASK_BUDGET_MS`; see
/// `docs/feedback-budgets.md` for the budget-warning convention (#227).
const WEB_SMOKE_BUDGET_MS: u64 = 10 * 60 * 1000;

pub const SUBCOMMANDS: &[(&str, &str)] = &[(
    "--scenario",
    "Run a named scenario (currently: cold-menu). Usage: cargo xtask web-smoke --scenario cold-menu [--update-baselines]",
)];

pub fn run(sub: &str) -> Result<(), StepError> {
    match sub {
        "--scenario" => {
            let args: Vec<String> = std::env::args().collect();
            let (scenario, update_baselines) = match parse_args(&args) {
                Ok(parsed) => parsed,
                Err(message) => {
                    eprintln!("{message}");
                    return Err(crate::web_smoke::SmokeError::usage(message).into());
                }
            };
            let start = std::time::Instant::now();
            let result = crate::web_smoke::run_scenario(&scenario, update_baselines)
                .map_err(StepError::from);
            if result.is_ok() {
                warn_if_over_budget(
                    &format!("web-smoke {scenario}"),
                    start.elapsed(),
                    effective_budget_ms(WEB_SMOKE_BUDGET_MS),
                );
            }
            result
        }
        other => unreachable!("dispatch validates subcommands before calling run; got {other}"),
    }
}

/// Parses `cargo xtask web-smoke --scenario <name> [--update-baselines]` out
/// of the full process argv (not just the one token the dispatcher hands
/// `run`; see the module docs for why). Returns the scenario name and
/// whether `--update-baselines` was present. Order-independent: the flag may
/// appear before or after the scenario name.
fn parse_args(full_argv: &[String]) -> Result<(String, bool), String> {
    // full_argv[0] is the xtask binary itself, [1] is "web-smoke", [2] is
    // "--scenario" -- everything from [3] on is what we still need to parse.
    let rest = &full_argv[3.min(full_argv.len())..];
    let mut scenario: Option<String> = None;
    let mut update_baselines = false;
    for token in rest {
        match token.as_str() {
            "--update-baselines" => update_baselines = true,
            other if !other.starts_with("--") && scenario.is_none() => {
                scenario = Some(other.to_string());
            }
            other => {
                return Err(format!(
                    "cargo xtask web-smoke --scenario <name> [--update-baselines]: unexpected argument `{other}`"
                ));
            }
        }
    }
    let scenario = scenario.ok_or_else(|| {
        "cargo xtask web-smoke --scenario <name> [--update-baselines]: missing scenario name after --scenario"
            .to_string()
    })?;
    Ok((scenario, update_baselines))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_scenario_alone() {
        let (scenario, update) =
            parse_args(&argv(&["xtask", "web-smoke", "--scenario", "cold-menu"])).unwrap();
        assert_eq!(scenario, "cold-menu");
        assert!(!update);
    }

    #[test]
    fn parses_scenario_with_trailing_update_flag() {
        let (scenario, update) = parse_args(&argv(&[
            "xtask",
            "web-smoke",
            "--scenario",
            "cold-menu",
            "--update-baselines",
        ]))
        .unwrap();
        assert_eq!(scenario, "cold-menu");
        assert!(update);
    }

    #[test]
    fn parses_update_flag_before_scenario_name() {
        let (scenario, update) = parse_args(&argv(&[
            "xtask",
            "web-smoke",
            "--scenario",
            "--update-baselines",
            "cold-menu",
        ]))
        .unwrap();
        assert_eq!(scenario, "cold-menu");
        assert!(update);
    }

    #[test]
    fn missing_scenario_name_is_a_clear_error() {
        let err = parse_args(&argv(&["xtask", "web-smoke", "--scenario"])).unwrap_err();
        assert!(err.contains("missing scenario name"));
    }

    #[test]
    fn unknown_extra_flag_is_a_clear_error() {
        let err = parse_args(&argv(&[
            "xtask",
            "web-smoke",
            "--scenario",
            "cold-menu",
            "--bogus",
        ]))
        .unwrap_err();
        assert!(err.contains("unexpected argument"));
    }
}
