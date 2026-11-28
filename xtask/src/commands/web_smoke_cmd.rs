//! `cargo xtask web-smoke --scenario <name> [--update-baselines]
//! [--strict-visual]` / `cargo xtask web-smoke --all [--update-baselines]
//! [--strict-visual]` -- browser smoke scenarios (#144/#168/#187/#198):
//! build + serve the WASM game, drive a real browser against it, and verify
//! a first-painted screen (or, for `gold-journey`, a full DPR matrix of
//! screens).
//!
//! Root-owned: this file is the thin CLI shim the dispatcher calls
//! (`xtask/src/commands/mod.rs` registers it exactly like `test_cmd`/
//! `check_cmd`); it owns no scenario logic itself. Everything else --
//! browser driver, ephemeral server, artifacts, baselines, and every
//! scenario -- lives in `crate::web_smoke` (`xtask/src/web_smoke/`),
//! disjoint from #141's future `xtask/src/commands/assets_cmd.rs`.
//!
//! ## Why `--scenario`/`--all` are the "subcommands"
//!
//! Every other command group's dispatch (see `commands::dispatch`) matches
//! `args[1]` verbatim against a fixed `SUBCOMMANDS` list and calls
//! `run(args[1])` -- it never forwards the rest of argv. That shape fits a
//! bare subcommand name (`test logic`) but not `--scenario cold-menu
//! --update-baselines` or `--all --strict-visual`, and changing the
//! dispatcher's forwarding behavior itself would be a dispatch-convention
//! change this module isn't allowed to make (see `xtask/README.md`'s
//! extension pattern -- only the module file plus one `GROUPS` line).
//! Registering `"--scenario"` and `"--all"` themselves as the recognized
//! subcommand names satisfies the dispatcher's exact-match check without
//! touching `commands::dispatch`; this module then reads the full process
//! argv itself (`std::env::args()`) to parse the scenario name and the
//! `--update-baselines`/`--strict-visual` flags. A later scenario needs no
//! dispatcher change either: it just becomes another accepted value after
//! `--scenario` (and another entry in `web_smoke::SCENARIOS` for `--all` to
//! pick up), handled by `crate::web_smoke::run_scenario`.

use crate::process::{StepError, effective_budget_ms, warn_if_over_budget};

pub const ABOUT: &str = "Browser-smoke scenarios: build+serve the WASM game and verify a first-painted screen (or DPR matrix) in a real browser.";

/// Target wall-clock budget for one `--scenario <name>` run. `cold-menu` is
/// a warm `trunk build --release` plus two cold-browser checkpoints
/// (dominated by the release wasm build; the browser phase is tens of
/// seconds). `gold-journey` (#198) can walk its full 30-checkpoint DPR
/// matrix (6 viewport journeys x 5 screens) in one invocation, which is
/// measurably heavier -- see `xtask/README.md`'s "web-smoke --all" section
/// for a measured transcript -- but #284 narrows its *default* run to the
/// single desktop/DPR-1 viewport (5 checkpoints); this budget stays sized
/// for the heavier full-matrix case (opt-in via
/// `XTASK_WEB_SMOKE_GOLD_JOURNEY_FULL_MATRIX=1`, see
/// `web_smoke::gold_journey`'s module docs) so it doesn't need re-tuning
/// when that opt-in is used. Overridable per-invocation via
/// `XTASK_BUDGET_MS`; see `docs/feedback-budgets.md` for the
/// budget-warning convention (#227).
const WEB_SMOKE_BUDGET_MS: u64 = 20 * 60 * 1000;

/// Target wall-clock budget for `--all` (#198): every registered scenario,
/// back to back, in one process.
const WEB_SMOKE_ALL_BUDGET_MS: u64 = 30 * 60 * 1000;

/// Set to enable `--strict-visual` without passing the flag on every
/// invocation -- handy for a CI job or a reviewer's shell profile. See
/// `baseline`'s module docs for what the flag does.
const STRICT_VISUAL_ENV_VAR: &str = "XTASK_WEB_SMOKE_STRICT_VISUAL";

pub const SUBCOMMANDS: &[(&str, &str)] = &[
    (
        "--build",
        "Build a validated browser bundle only. Usage: cargo xtask web-smoke --build production|review [--artifact-root <directory>]",
    ),
    (
        "--scenario",
        "Run a named scenario (cold-menu, gold-journey, hybrid-2-5d-character, romanian-paper-doll-library, accessibility-settings-reload, reduced-motion-fight, fight-palette-desktop, fight-palette-phone, high-contrast). Usage: cargo xtask web-smoke --scenario cold-menu [--update-baselines] [--strict-visual]",
    ),
    (
        "--all",
        "Run every registered scenario (including gold-journey, hybrid-2-5d-character, and romanian-paper-doll-library). Usage: cargo xtask web-smoke --all [--update-baselines] [--strict-visual]",
    ),
];

pub fn run(sub: &str) -> Result<(), StepError> {
    let full_args: Vec<String> = std::env::args().collect();
    let (args, artifact_root) =
        extract_artifact_root(&full_args).map_err(crate::web_smoke::SmokeError::usage)?;
    match sub {
        "--build" => {
            let variant = parse_build_args(&args).map_err(crate::web_smoke::SmokeError::usage)?;
            let root = artifact_root.unwrap_or_else(crate::web_smoke::build::default_root);
            crate::web_smoke::build::produce(variant, &root)?;
            Ok(())
        }
        "--scenario" => {
            let (scenario, update_baselines, strict_visual) = match parse_scenario_args(&args) {
                Ok(parsed) => parsed,
                Err(message) => {
                    eprintln!("{message}");
                    return Err(crate::web_smoke::SmokeError::usage(message).into());
                }
            };
            let start = std::time::Instant::now();
            let result = crate::web_smoke::run_scenario(
                &scenario,
                update_baselines,
                strict_visual,
                artifact_root.as_deref(),
            )
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
        "--all" => {
            let (update_baselines, strict_visual) = match parse_flag_only_args(&args) {
                Ok(parsed) => parsed,
                Err(message) => {
                    eprintln!("{message}");
                    return Err(crate::web_smoke::SmokeError::usage(message).into());
                }
            };
            let start = std::time::Instant::now();
            let result = crate::web_smoke::run_all(
                update_baselines,
                strict_visual,
                artifact_root.as_deref(),
            )
            .map_err(StepError::from);
            if result.is_ok() {
                warn_if_over_budget(
                    "web-smoke --all",
                    start.elapsed(),
                    effective_budget_ms(WEB_SMOKE_ALL_BUDGET_MS),
                );
            }
            result
        }
        other => unreachable!("dispatch validates subcommands before calling run; got {other}"),
    }
}

/// Strip only the new artifact option, preserving the existing parsers/flag semantics.
/// For scenario/all modes its presence selects validation-only consumption.
fn extract_artifact_root(
    args: &[String],
) -> Result<(Vec<String>, Option<std::path::PathBuf>), String> {
    let mut filtered = Vec::new();
    let mut root = None;
    let mut tokens = args.iter();
    while let Some(token) = tokens.next() {
        if token == "--artifact-root" {
            if root.is_some() {
                return Err("duplicate --artifact-root".into());
            }
            let value = tokens
                .next()
                .filter(|v| !v.starts_with("--"))
                .ok_or("--artifact-root requires a directory")?;
            root = Some(std::path::PathBuf::from(value));
        } else {
            filtered.push(token.clone());
        }
    }
    Ok((filtered, root))
}

fn parse_build_args(args: &[String]) -> Result<crate::web_smoke::build::Variant, String> {
    if args.len() != 4 {
        return Err(
            "usage: web-smoke --build production|review [--artifact-root <directory>]".into(),
        );
    }
    crate::web_smoke::build::Variant::parse(&args[3]).map_err(|e| e.to_string())
}

/// True if `--strict-visual` was passed on the command line, or the
/// `XTASK_WEB_SMOKE_STRICT_VISUAL` env var is set to `1`/`true`
/// (case-insensitive).
fn strict_visual_enabled(flag_present: bool) -> bool {
    flag_present
        || std::env::var(STRICT_VISUAL_ENV_VAR)
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
}

/// Parses `cargo xtask web-smoke --scenario <name> [--update-baselines]
/// [--strict-visual]` out of the full process argv (not just the one token
/// the dispatcher hands `run`; see the module docs for why). Returns the
/// scenario name and whether each flag was effectively enabled (flag or
/// env, see [`strict_visual_enabled`]). Order-independent: flags may appear
/// before or after the scenario name.
fn parse_scenario_args(full_argv: &[String]) -> Result<(String, bool, bool), String> {
    // full_argv[0] is the xtask binary itself, [1] is "web-smoke", [2] is
    // "--scenario" -- everything from [3] on is what we still need to parse.
    let rest = &full_argv[3.min(full_argv.len())..];
    let mut scenario: Option<String> = None;
    let mut update_baselines = false;
    let mut strict_visual_flag = false;
    for token in rest {
        match token.as_str() {
            "--update-baselines" => update_baselines = true,
            "--strict-visual" => strict_visual_flag = true,
            other if !other.starts_with("--") && scenario.is_none() => {
                scenario = Some(other.to_string());
            }
            other => {
                return Err(format!(
                    "cargo xtask web-smoke --scenario <name> [--update-baselines] [--strict-visual]: unexpected argument `{other}`"
                ));
            }
        }
    }
    let scenario = scenario.ok_or_else(|| {
        "cargo xtask web-smoke --scenario <name> [--update-baselines] [--strict-visual]: missing scenario name after --scenario"
            .to_string()
    })?;
    Ok((
        scenario,
        update_baselines,
        strict_visual_enabled(strict_visual_flag),
    ))
}

/// Parses `cargo xtask web-smoke --all [--update-baselines]
/// [--strict-visual]` -- like [`parse_scenario_args`] but with no scenario
/// name to accept (`--all` takes flags only).
fn parse_flag_only_args(full_argv: &[String]) -> Result<(bool, bool), String> {
    // full_argv[0] is the xtask binary, [1] is "web-smoke", [2] is "--all".
    let rest = &full_argv[3.min(full_argv.len())..];
    let mut update_baselines = false;
    let mut strict_visual_flag = false;
    for token in rest {
        match token.as_str() {
            "--update-baselines" => update_baselines = true,
            "--strict-visual" => strict_visual_flag = true,
            other => {
                return Err(format!(
                    "cargo xtask web-smoke --all [--update-baselines] [--strict-visual]: unexpected argument `{other}`"
                ));
            }
        }
    }
    Ok((update_baselines, strict_visual_enabled(strict_visual_flag)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests exercise the pure argv-parsing functions only, never the
    // `XTASK_WEB_SMOKE_STRICT_VISUAL` env var (unset in the test process),
    // so `strict_visual_enabled` reduces to the flag value alone here.

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn artifact_consumer_and_build_modes_preserve_flags_and_reject_missing_options() {
        let (filtered, root) = extract_artifact_root(&argv(&[
            "xtask",
            "web-smoke",
            "--scenario",
            "gold-journey",
            "--artifact-root",
            "target/bundles",
            "--strict-visual",
            "--update-baselines",
        ]))
        .unwrap();
        assert_eq!(root.unwrap(), std::path::PathBuf::from("target/bundles"));
        assert_eq!(
            parse_scenario_args(&filtered).unwrap(),
            ("gold-journey".into(), true, true)
        );
        let (filtered, _) = extract_artifact_root(&argv(&[
            "xtask",
            "web-smoke",
            "--all",
            "--artifact-root",
            "bundles",
            "--strict-visual",
        ]))
        .unwrap();
        assert_eq!(parse_flag_only_args(&filtered).unwrap(), (false, true));
        assert!(
            extract_artifact_root(&argv(&["xtask", "web-smoke", "--all", "--artifact-root"]))
                .is_err()
        );
        assert!(
            extract_artifact_root(&argv(&[
                "xtask",
                "web-smoke",
                "--all",
                "--artifact-root",
                "--strict-visual"
            ]))
            .is_err()
        );
        assert!(
            extract_artifact_root(&argv(&[
                "xtask",
                "web-smoke",
                "--all",
                "--artifact-root",
                "a",
                "--artifact-root",
                "b"
            ]))
            .is_err()
        );
        for variant in ["production", "review"] {
            assert_eq!(
                parse_build_args(&argv(&["xtask", "web-smoke", "--build", variant]))
                    .unwrap()
                    .name(),
                variant
            );
        }
        assert!(parse_build_args(&argv(&["xtask", "web-smoke", "--build", "dev"])).is_err());
        assert!(
            parse_build_args(&argv(&[
                "xtask",
                "web-smoke",
                "--build",
                "review",
                "--strict-visual"
            ]))
            .is_err()
        );
    }

    #[test]
    fn parses_scenario_alone() {
        let (scenario, update, strict) =
            parse_scenario_args(&argv(&["xtask", "web-smoke", "--scenario", "cold-menu"])).unwrap();
        assert_eq!(scenario, "cold-menu");
        assert!(!update);
        assert!(!strict);
    }

    #[test]
    fn parses_scenario_with_trailing_update_flag() {
        let (scenario, update, strict) = parse_scenario_args(&argv(&[
            "xtask",
            "web-smoke",
            "--scenario",
            "cold-menu",
            "--update-baselines",
        ]))
        .unwrap();
        assert_eq!(scenario, "cold-menu");
        assert!(update);
        assert!(!strict);
    }

    #[test]
    fn parses_update_flag_before_scenario_name() {
        let (scenario, update, _strict) = parse_scenario_args(&argv(&[
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
    fn parses_strict_visual_flag_alongside_scenario() {
        let (scenario, update, strict) = parse_scenario_args(&argv(&[
            "xtask",
            "web-smoke",
            "--scenario",
            "gold-journey",
            "--strict-visual",
        ]))
        .unwrap();
        assert_eq!(scenario, "gold-journey");
        assert!(!update);
        assert!(strict);
    }

    #[test]
    fn missing_scenario_name_is_a_clear_error() {
        let err = parse_scenario_args(&argv(&["xtask", "web-smoke", "--scenario"])).unwrap_err();
        assert!(err.contains("missing scenario name"));
    }

    #[test]
    fn unknown_extra_flag_is_a_clear_error() {
        let err = parse_scenario_args(&argv(&[
            "xtask",
            "web-smoke",
            "--scenario",
            "cold-menu",
            "--bogus",
        ]))
        .unwrap_err();
        assert!(err.contains("unexpected argument"));
    }

    #[test]
    fn parses_all_with_no_flags() {
        let (update, strict) =
            parse_flag_only_args(&argv(&["xtask", "web-smoke", "--all"])).unwrap();
        assert!(!update);
        assert!(!strict);
    }

    #[test]
    fn parses_all_with_both_flags_in_either_order() {
        let (update, strict) = parse_flag_only_args(&argv(&[
            "xtask",
            "web-smoke",
            "--all",
            "--strict-visual",
            "--update-baselines",
        ]))
        .unwrap();
        assert!(update);
        assert!(strict);
    }

    #[test]
    fn all_rejects_a_scenario_name_positional() {
        let err =
            parse_flag_only_args(&argv(&["xtask", "web-smoke", "--all", "cold-menu"])).unwrap_err();
        assert!(err.contains("unexpected argument"));
    }
}
