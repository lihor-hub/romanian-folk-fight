//! `cargo xtask assets check` -- validates every per-directory manifest
//! sidecar under `assets/`, derives one in-memory aggregate, reports
//! coverage (#167, a child of #141), and additionally validates production
//! runtime references and image integrity against that same aggregate
//! (#185, a child of #141; see `crate::assets::validate`).
//!
//! Unlike `test_cmd`/`check_cmd`, this command's work is pure in-process
//! Rust (parsing TOML, walking the filesystem) rather than a spawned
//! `cargo`/external subprocess, so it does not go through
//! [`crate::process::run_step`] (which spawns a [`std::process::Command`]).
//! It still follows the same *conventions* that module documents: it
//! measures elapsed time, retains a full diagnostics log under the same
//! [`crate::process::artifacts_dir`] path convention, and reports success via
//! [`crate::process::StepReport`] / failure via
//! [`crate::process::StepError::Failed`] so `cargo xtask --help` and the
//! dispatcher treat this group exactly like any other.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::assets::CheckResult;
use crate::assets::changed;
use crate::assets::gallery::RemovedAssetNote;
use crate::process::{
    StepError, StepReport, artifacts_dir, effective_budget_ms, print_summary, warn_if_over_budget,
};

pub const ABOUT: &str = "Per-directory asset manifest sidecar inventory, runtime-reference/image-integrity validation, and the review gallery.";

/// Target warm-run budget (milliseconds) for `assets review --changed`: the
/// "changed-asset gallery" loop from the player-experience rework plan's
/// feedback-loop contract (30 seconds warm; see `docs/feedback-budgets.md`).
/// Overridable per-invocation via `XTASK_BUDGET_MS`.
const CHANGED_REVIEW_BUDGET_MS: u64 = 30_000;

pub const SUBCOMMANDS: &[(&str, &str)] = &[
    (
        "check",
        "Load every assets/**/manifest.toml sidecar, derive the aggregate, validate runtime \
         references and image integrity, and report coverage.",
    ),
    (
        "review",
        "Generate a deterministic static HTML asset review gallery from the sidecar aggregate \
         into target/xtask-artifacts/asset-gallery/, printing the index path. With --changed \
         [--base <ref>] (#211): generate only the pages dirtied by assets changed since the \
         comparison base (precedence: --base > GITHUB_BASE_REF > merge-base with origin/main) \
         into target/xtask-artifacts/asset-gallery-changed/.",
    ),
];

pub fn run(sub: &str) -> Result<(), StepError> {
    match sub {
        "check" => check(),
        "review" => {
            // The dispatcher only validates/forwards the one subcommand
            // token; flags after it are parsed from the full process argv,
            // exactly like `web_smoke_cmd` (see that module's docs for why
            // the dispatch convention itself must not change).
            let args: Vec<String> = std::env::args().collect();
            match parse_review_args(&args) {
                Ok(ReviewArgs {
                    changed: false,
                    base: None,
                }) => review(),
                Ok(ReviewArgs {
                    changed: true,
                    base,
                }) => review_changed(base.as_deref()),
                Ok(ReviewArgs {
                    changed: false,
                    base: Some(_),
                }) => {
                    let message = "cargo xtask assets review: `--base <ref>` requires `--changed`";
                    eprintln!("{message}");
                    Err(usage_failure("assets review", message))
                }
                Err(message) => {
                    eprintln!("{message}");
                    Err(usage_failure("assets review", &message))
                }
            }
        }
        other => unreachable!("dispatch validates subcommands before calling run; got {other}"),
    }
}

#[derive(Debug)]
struct ReviewArgs {
    changed: bool,
    base: Option<String>,
}

/// Parses `cargo xtask assets review [--changed] [--base <ref>]` out of the
/// full process argv (`full_argv[0]` is the xtask binary, `[1]` is
/// `assets`, `[2]` is `review`).
fn parse_review_args(full_argv: &[String]) -> Result<ReviewArgs, String> {
    let rest = &full_argv[3.min(full_argv.len())..];
    let mut parsed = ReviewArgs {
        changed: false,
        base: None,
    };
    let mut tokens = rest.iter();
    while let Some(token) = tokens.next() {
        match token.as_str() {
            "--changed" => parsed.changed = true,
            "--base" => {
                let Some(value) = tokens.next() else {
                    return Err(
                        "cargo xtask assets review --changed --base <ref>: missing ref after --base"
                            .to_string(),
                    );
                };
                parsed.base = Some(value.clone());
            }
            other => {
                return Err(format!(
                    "cargo xtask assets review [--changed] [--base <ref>]: unexpected argument `{other}`"
                ));
            }
        }
    }
    Ok(parsed)
}

fn usage_failure(label: &str, _message: &str) -> StepError {
    StepError::Failed {
        label: label.to_string(),
        elapsed: std::time::Duration::ZERO,
        exit_code: Some(2),
        artifact: artifact_path(label),
    }
}

fn check() -> Result<(), StepError> {
    let label = "assets check";
    println!("\n==> {label}");

    let root = workspace_root();
    let assets_root = root.join("assets");
    let credits_path = assets_root.join("CREDITS.md");

    let start = Instant::now();
    let result = crate::assets::run_check(&assets_root, &credits_path, &root);
    let elapsed = start.elapsed();

    let report_text = render_report(&result);
    let artifact = artifact_path(label);
    if let Some(parent) = artifact.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&artifact, &report_text);

    print!("{report_text}");

    if result.is_clean() {
        println!(
            "    ok ({:.2}s) -- log: {}",
            elapsed.as_secs_f64(),
            artifact.display()
        );
        print_summary(&[StepReport {
            label: label.to_string(),
            elapsed,
            artifact,
        }]);
        Ok(())
    } else {
        println!(
            "    FAILED ({:.2}s) -- log retained at: {}",
            elapsed.as_secs_f64(),
            artifact.display()
        );
        Err(StepError::Failed {
            label: label.to_string(),
            elapsed,
            exit_code: Some(1),
            artifact,
        })
    }
}

/// `cargo xtask assets review` (#197, a child of #141): generates the
/// static HTML gallery from `xtask/src/assets/gallery/` into
/// `target/xtask-artifacts/asset-gallery/` and prints the index path. Like
/// `check` above, this is pure in-process Rust (filesystem walking, string
/// building), not a spawned subprocess, so it does not go through
/// [`crate::process::run_step`], but still reports success/failure via the
/// same [`StepReport`]/[`StepError::Failed`] shapes.
fn review() -> Result<(), StepError> {
    let label = "assets review";
    println!("\n==> {label}");

    let root = workspace_root();
    let assets_root = root.join("assets");
    let out_dir = root
        .join("target")
        .join("xtask-artifacts")
        .join("asset-gallery");

    let start = Instant::now();
    let result = crate::assets::gallery::generate(&assets_root, &out_dir);
    let elapsed = start.elapsed();

    match result {
        Ok(report) => {
            println!(
                "    ok ({:.2}s) -- {} page(s) generated",
                elapsed.as_secs_f64(),
                report.page_count
            );
            println!("Gallery index: {}", report.index_path.display());
            print_summary(&[StepReport {
                label: label.to_string(),
                elapsed,
                artifact: report.index_path,
            }]);
            Ok(())
        }
        Err(err) => {
            println!("    FAILED ({:.2}s): {err}", elapsed.as_secs_f64());
            Err(StepError::Failed {
                label: label.to_string(),
                elapsed,
                exit_code: Some(1),
                artifact: out_dir,
            })
        }
    }
}

/// `cargo xtask assets review --changed [--base <ref>]` (#211, a child of
/// #141): resolves the comparison base (precedence: `--base` >
/// `GITHUB_BASE_REF` > merge-base with `origin/main`; a missing/invalid
/// base is an actionable failure, never a silent fall-back to reviewing
/// everything -- see `crate::assets::changed`), maps `git diff` output to
/// sidecar records, expands the transitive dependency closure, and
/// generates only those pages into
/// `target/xtask-artifacts/asset-gallery-changed/` (a directory separate
/// from the full gallery's, so a focused CI artifact never mixes with, or
/// wipes, a full local gallery).
fn review_changed(explicit_base: Option<&str>) -> Result<(), StepError> {
    let label = "assets review --changed";
    println!("\n==> {label}");

    let root = workspace_root();
    let assets_root = root.join("assets");
    let out_dir = root
        .join("target")
        .join("xtask-artifacts")
        .join("asset-gallery-changed");

    let start = Instant::now();

    let github_base_ref = std::env::var("GITHUB_BASE_REF").ok();
    let base = match changed::resolve_base(explicit_base, github_base_ref.as_deref(), &root) {
        Ok(base) => base,
        Err(err) => {
            println!("    FAILED: {err}");
            return Err(StepError::Failed {
                label: label.to_string(),
                elapsed: start.elapsed(),
                exit_code: Some(1),
                artifact: out_dir,
            });
        }
    };
    println!("Comparison base: {} (via {})", base.sha, base.source);

    let changed_files = match changed::diff_changed_assets(&root, &base.sha) {
        Ok(files) => files,
        Err(err) => {
            println!("    FAILED: {err}");
            return Err(StepError::Failed {
                label: label.to_string(),
                elapsed: start.elapsed(),
                exit_code: Some(1),
                artifact: out_dir,
            });
        }
    };
    println!(
        "Changed files under assets/ since base: {}",
        changed_files.len()
    );

    let built = crate::assets::aggregate::build(&assets_root);
    let records: Vec<&crate::assets::aggregate::ResolvedRecord> = built.records.iter().collect();

    let mut mapping = changed::map_changed_files(&assets_root, &records, &changed_files);
    for removed in &mut mapping.removed {
        if removed.id.is_none() {
            removed.id = changed::resolve_removed_record_id(&root, &base.sha, &removed.path);
        }
    }

    if !mapping.direct_ids.is_empty() {
        println!("Directly changed record(s):");
        for id in &mapping.direct_ids {
            println!("  {id}");
        }
    }
    if !mapping.removed.is_empty() {
        println!("Removed asset(s) (surfaced on the index):");
        for removed in &mapping.removed {
            match &removed.id {
                Some(id) => println!("  {} (was {id})", removed.path),
                None => println!("  {} (previous record id unknown)", removed.path),
            }
        }
    }

    let closure: BTreeSet<String> = changed::page_closure(&records, &mapping.direct_ids);
    let removed_notes: Vec<RemovedAssetNote> = mapping
        .removed
        .iter()
        .map(|r| RemovedAssetNote {
            path: r.path.clone(),
            id: r.id.clone(),
        })
        .collect();

    let result = crate::assets::gallery::generate_filtered(
        &assets_root,
        &out_dir,
        Some(&closure),
        &removed_notes,
    );
    let elapsed = start.elapsed();

    match result {
        Ok(report) => {
            println!(
                "    ok ({:.2}s) -- {} focused page(s) generated (of the dependency closure's {} page id(s))",
                elapsed.as_secs_f64(),
                report.page_count,
                closure.len().saturating_sub(1), // minus the index sentinel
            );
            println!("Included page id(s):");
            for id in &report.included_page_ids {
                println!("  {id}");
            }
            println!("Gallery index: {}", report.index_path.display());
            print_summary(&[StepReport {
                label: label.to_string(),
                elapsed,
                artifact: report.index_path,
            }]);
            warn_if_over_budget(
                label,
                elapsed,
                effective_budget_ms(CHANGED_REVIEW_BUDGET_MS),
            );
            Ok(())
        }
        Err(err) => {
            println!("    FAILED ({:.2}s): {err}", elapsed.as_secs_f64());
            Err(StepError::Failed {
                label: label.to_string(),
                elapsed,
                exit_code: Some(1),
                artifact: out_dir,
            })
        }
    }
}

fn render_report(result: &CheckResult) -> String {
    let mut out = String::new();

    let diagnostics = result.all_diagnostics();
    if diagnostics.is_empty() {
        out.push_str("All sidecar records validated cleanly.\n\n");
    } else {
        out.push_str(&format!("{} diagnostic(s):\n", diagnostics.len()));
        for diagnostic in &diagnostics {
            out.push_str(&format!("  - {diagnostic}\n"));
        }
        out.push('\n');
    }

    let coverage = &result.coverage;
    out.push_str("Coverage:\n");
    out.push_str(&format!(
        "  files under assets/:      {}\n",
        coverage.total_files
    ));
    out.push_str(&format!(
        "  covered by records:       {}\n",
        coverage.total_records
    ));
    out.push_str(&format!(
        "  covered by ignore entries: {}\n",
        coverage.total_ignored
    ));
    out.push_str(&format!(
        "  total covered:             {}\n",
        coverage.total_records + coverage.total_ignored
    ));

    out.push_str("\nBy status:\n");
    for (status, count) in &coverage.by_status {
        out.push_str(&format!("  {status:<10} {count}\n"));
    }

    out.push_str("\nBy category:\n");
    for (category, count) in &coverage.by_category {
        out.push_str(&format!("  {category:<24} {count}\n"));
    }

    out.push_str("\nBy provenance:\n");
    for (provenance, count) in &coverage.by_provenance {
        out.push_str(&format!("  {provenance:<28} {count}\n"));
    }

    out.push_str("\nBy sidecar:\n");
    for (sidecar, count) in &coverage.by_sidecar {
        out.push_str(&format!("  {:<55} {count}\n", sidecar.display()));
    }

    if !result.aggregate.ignores.is_empty() {
        out.push_str("\nIgnored (documented reason):\n");
        for ignore in &result.aggregate.ignores {
            out.push_str(&format!(
                "  {:<40} {}\n",
                ignore.full_path.display(),
                ignore.reason
            ));
        }
    }

    use crate::assets::validate::bounds::ASPECT_DISTORTION_KNOWN_FAILURES;
    use crate::assets::validate::refs::RUNTIME_REFERENCE_EXEMPTIONS;

    if !RUNTIME_REFERENCE_EXEMPTIONS.is_empty() {
        out.push_str("\nRuntime-reference compatibility exemptions (#185):\n");
        for (id, reason) in RUNTIME_REFERENCE_EXEMPTIONS {
            out.push_str(&format!("  {id:<40} {reason}\n"));
        }
    }

    if !ASPECT_DISTORTION_KNOWN_FAILURES.is_empty() {
        out.push_str(
            "\nAspect-distortion known failures (#185, see xtask/src/assets/validate/bounds.rs):\n",
        );
        for (id, reason) in ASPECT_DISTORTION_KNOWN_FAILURES {
            out.push_str(&format!("  {id:<40} {reason}\n"));
        }
    }

    out
}

fn artifact_path(label: &str) -> PathBuf {
    artifacts_dir().join(format!("{}.log", slugify(label)))
}

fn slugify(label: &str) -> String {
    let mut slug = String::with_capacity(label.len());
    let mut last_was_dash = false;
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/Cargo.toml always has a parent workspace root")
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plain_review_parses_with_no_flags() {
        let parsed = parse_review_args(&argv(&["xtask", "assets", "review"])).unwrap();
        assert!(!parsed.changed);
        assert!(parsed.base.is_none());
    }

    #[test]
    fn changed_flag_parses_alone() {
        let parsed = parse_review_args(&argv(&["xtask", "assets", "review", "--changed"])).unwrap();
        assert!(parsed.changed);
        assert!(parsed.base.is_none());
    }

    #[test]
    fn changed_with_base_parses_in_either_order() {
        let parsed = parse_review_args(&argv(&[
            "xtask",
            "assets",
            "review",
            "--changed",
            "--base",
            "origin/main",
        ]))
        .unwrap();
        assert!(parsed.changed);
        assert_eq!(parsed.base.as_deref(), Some("origin/main"));

        let parsed = parse_review_args(&argv(&[
            "xtask",
            "assets",
            "review",
            "--base",
            "abc123",
            "--changed",
        ]))
        .unwrap();
        assert!(parsed.changed);
        assert_eq!(parsed.base.as_deref(), Some("abc123"));
    }

    #[test]
    fn base_without_a_value_is_a_clear_error() {
        let err = parse_review_args(&argv(&["xtask", "assets", "review", "--changed", "--base"]))
            .unwrap_err();
        assert!(err.contains("missing ref after --base"));
    }

    #[test]
    fn an_unknown_flag_is_a_clear_error() {
        let err = parse_review_args(&argv(&["xtask", "assets", "review", "--bogus"])).unwrap_err();
        assert!(err.contains("unexpected argument"));
    }
}
