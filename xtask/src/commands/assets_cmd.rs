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

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::assets::CheckResult;
use crate::process::{StepError, StepReport, artifacts_dir, print_summary};

pub const ABOUT: &str = "Per-directory asset manifest sidecar inventory, runtime-reference/image-integrity validation, and the review gallery.";

pub const SUBCOMMANDS: &[(&str, &str)] = &[
    (
        "check",
        "Load every assets/**/manifest.toml sidecar, derive the aggregate, validate runtime \
         references and image integrity, and report coverage.",
    ),
    (
        "review",
        "Generate a deterministic static HTML asset review gallery from the sidecar aggregate \
         into target/xtask-artifacts/asset-gallery/, printing the index path.",
    ),
];

pub fn run(sub: &str) -> Result<(), StepError> {
    match sub {
        "check" => check(),
        "review" => review(),
        other => unreachable!("dispatch validates subcommands before calling run; got {other}"),
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
