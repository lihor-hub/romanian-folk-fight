//! Shared child-process plumbing used by every xtask command.
//!
//! Every verification step in this crate goes through [`run_step`] so the
//! behavior is identical everywhere: the command line is printed before it
//! runs, its wall-clock time is measured, its combined stdout/stderr is
//! retained under [`artifacts_dir`], and a failure is reported with the
//! elapsed time, the exit code, and the path to the retained log.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Directory where xtask retains one log file per executed step, so a
/// failure's full output survives after the terminal history scrolls away.
///
/// Convention: `target/xtask-artifacts/<slugified-step-label>.log`. Each
/// file is overwritten the next time that step runs; nothing is cleaned up
/// automatically, so the most recent run of every step is always on disk.
pub fn artifacts_dir() -> PathBuf {
    workspace_root().join("target").join("xtask-artifacts")
}

/// The workspace root, derived from `xtask`'s own manifest directory
/// (`<root>/xtask`) rather than the current working directory, so `cargo
/// xtask` behaves the same regardless of where it is invoked from within the
/// workspace.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/Cargo.toml always has a parent workspace root")
        .to_path_buf()
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

/// A step that completed successfully.
#[derive(Debug)]
pub struct StepReport {
    pub label: String,
    pub elapsed: Duration,
    pub artifact: PathBuf,
}

/// A step that could not be run at all, or ran and failed.
#[derive(Debug)]
pub enum StepError {
    /// The command could not even be spawned (e.g. the binary is missing).
    Spawn { label: String, source: io::Error },
    /// The command ran to completion with a non-zero/aborted exit status.
    Failed {
        label: String,
        elapsed: Duration,
        exit_code: Option<i32>,
        artifact: PathBuf,
    },
}

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepError::Spawn { label, source } => {
                write!(f, "{label}: failed to launch command: {source}")
            }
            StepError::Failed {
                label,
                elapsed,
                exit_code,
                artifact,
            } => {
                let code = exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "<terminated by signal>".to_string());
                write!(
                    f,
                    "{label} failed after {:.2}s (exit {code}); full output retained at: {}",
                    elapsed.as_secs_f64(),
                    artifact.display()
                )
            }
        }
    }
}

impl std::error::Error for StepError {}

/// Runs `command` as one reported step: prints the command line, times it,
/// retains its combined stdout/stderr under [`artifacts_dir`], and turns a
/// non-zero exit into a [`StepError::Failed`] carrying the artifact path.
pub fn run_step(label: &str, mut command: Command) -> Result<StepReport, StepError> {
    println!("\n==> {label}");
    println!("    $ {}", format_command(&command));
    let _ = io::stdout().flush();

    let artifact = artifact_path(label);
    if let Some(parent) = artifact.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let start = Instant::now();
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|source| StepError::Spawn {
            label: label.to_string(),
            source,
        })?;
    let elapsed = start.elapsed();

    let mut log = String::new();
    log.push_str(&format!("$ {}\n\n", format_command(&command)));
    log.push_str("--- stdout ---\n");
    log.push_str(&String::from_utf8_lossy(&output.stdout));
    log.push_str("\n--- stderr ---\n");
    log.push_str(&String::from_utf8_lossy(&output.stderr));
    let _ = fs::write(&artifact, log);

    if output.status.success() {
        println!(
            "    ok ({:.2}s) -- log: {}",
            elapsed.as_secs_f64(),
            artifact.display()
        );
        Ok(StepReport {
            label: label.to_string(),
            elapsed,
            artifact,
        })
    } else {
        // Surface the failing output immediately in addition to retaining it,
        // so the first failure is visible without opening the log file.
        let _ = io::stdout().write_all(&output.stdout);
        let _ = io::stderr().write_all(&output.stderr);
        let exit_code = output.status.code();
        println!(
            "    FAILED ({:.2}s), exit {} -- log retained at: {}",
            elapsed.as_secs_f64(),
            exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "<signal>".to_string()),
            artifact.display()
        );
        Err(StepError::Failed {
            label: label.to_string(),
            elapsed,
            exit_code,
            artifact,
        })
    }
}

/// Prints an itemized elapsed-time summary (and each step's retained
/// artifact path) for a multi-step command, followed by the total. Used by
/// `check build-matrix` and `pre-push` once every step has succeeded.
pub fn print_summary(reports: &[StepReport]) {
    println!("\nSummary:");
    let mut total = Duration::ZERO;
    for report in reports {
        total += report.elapsed;
        println!(
            "  {:<40} {:>7.2}s  log: {}",
            report.label,
            report.elapsed.as_secs_f64(),
            report.artifact.display()
        );
    }
    println!(
        "  {:<40} {:>7.2}s  (total)",
        "all steps",
        total.as_secs_f64()
    );
}

fn format_command(command: &Command) -> String {
    let mut parts = vec![command.get_program().to_string_lossy().into_owned()];
    parts.extend(command.get_args().map(|a| a.to_string_lossy().into_owned()));
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_lowercases_and_collapses_separators() {
        assert_eq!(slugify("check build-matrix"), "check-build-matrix");
        assert_eq!(slugify("fmt check"), "fmt-check");
        assert_eq!(slugify("cargo test"), "cargo-test");
    }

    #[test]
    fn slugify_trims_leading_and_trailing_separators() {
        assert_eq!(slugify("  spaced  "), "spaced");
    }

    #[test]
    fn run_step_reports_success_and_retains_a_log() {
        let mut cmd = Command::new("true");
        if cfg!(windows) {
            cmd = Command::new("cmd");
            cmd.args(["/C", "exit 0"]);
        }
        let report = run_step("xtask self-test true", cmd).expect("`true` always succeeds");
        assert_eq!(report.label, "xtask self-test true");
        assert!(report.artifact.exists(), "log file must be retained");
    }

    #[test]
    fn run_step_reports_failure_with_exit_code_and_log() {
        let mut cmd = Command::new("false");
        if cfg!(windows) {
            cmd = Command::new("cmd");
            cmd.args(["/C", "exit 1"]);
        }
        let err = run_step("xtask self-test false", cmd).expect_err("`false` always fails");
        match err {
            StepError::Failed {
                exit_code,
                artifact,
                ..
            } => {
                assert_eq!(exit_code, Some(1));
                assert!(artifact.exists(), "log file must be retained on failure");
            }
            StepError::Spawn { .. } => panic!("`false` must spawn successfully"),
        }
    }
}
