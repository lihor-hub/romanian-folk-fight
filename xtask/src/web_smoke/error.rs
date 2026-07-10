//! Error type for browser-smoke scenarios, convertible into the shared
//! [`StepError`] contract every other xtask command reports through (see
//! `crate::process`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::process::StepError;

/// Something that made a browser-smoke scenario fail (or a usage mistake in
/// its arguments), carrying enough context to report through the same
/// `StepError` contract as every `run_step`-driven command, even though a
/// scenario runs several steps (build, serve, launch, navigate, assert) that
/// don't each map onto a single child process.
#[derive(Debug)]
pub struct SmokeError {
    label: String,
    message: String,
    artifacts_dir: Option<PathBuf>,
}

impl SmokeError {
    /// A malformed `--scenario`/`--update-baselines` invocation -- no
    /// artifacts to point at, just the usage message.
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            label: "web-smoke: usage".to_string(),
            message: message.into(),
            artifacts_dir: None,
        }
    }

    /// A scenario step failed after doing real work; `artifacts_dir` is
    /// where its diagnostics (screenshot/console/network/server logs) were
    /// retained, so the failure message can point at them.
    pub fn scenario(
        label: impl Into<String>,
        message: impl Into<String>,
        artifacts_dir: PathBuf,
    ) -> Self {
        Self {
            label: label.into(),
            message: message.into(),
            artifacts_dir: Some(artifacts_dir),
        }
    }
}

impl std::fmt::Display for SmokeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(dir) = &self.artifacts_dir {
            write!(f, "\nartifacts: {}", dir.display())?;
        }
        Ok(())
    }
}

impl std::error::Error for SmokeError {}

/// Lets scenario code `?`-propagate a failed `run_step` call (e.g. `trunk
/// build --release`) directly as a `SmokeError`; `run_step` already wrote
/// its own artifact log under `target/xtask-artifacts/`, so that path is
/// reused instead of writing a second, redundant one.
impl From<StepError> for SmokeError {
    fn from(err: StepError) -> Self {
        match err {
            StepError::Spawn { label, source } => SmokeError {
                label,
                message: format!("failed to launch command: {source}"),
                artifacts_dir: None,
            },
            StepError::Failed {
                label,
                exit_code,
                artifact,
                ..
            } => SmokeError {
                label,
                message: format!(
                    "step failed (exit {}); full output retained at: {}",
                    exit_code
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "<signal>".to_string()),
                    artifact.display()
                ),
                artifacts_dir: artifact.parent().map(Path::to_path_buf),
            },
        }
    }
}

impl From<SmokeError> for StepError {
    fn from(err: SmokeError) -> Self {
        // A scenario isn't a single child process, so there's no exit code
        // or `run_step`-written log -- write one synthetic log file so
        // `StepError`'s existing "full output retained at: <path>" Display
        // still points somewhere real, in the same `target/xtask-artifacts/`
        // convention every other command uses.
        let dir = err
            .artifacts_dir
            .clone()
            .unwrap_or_else(crate::process::artifacts_dir);
        let _ = std::fs::create_dir_all(&dir);
        let artifact = dir.join("web-smoke-failure.log");
        let _ = std::fs::write(&artifact, format!("{err}\n"));
        StepError::Failed {
            label: err.label,
            elapsed: Duration::ZERO,
            // Not a child process's real exit status -- but `main` maps any
            // `StepError` to `ExitCode::FAILURE` (1), so report that instead
            // of `None` (whose Display reads "<terminated by signal>", which
            // would be misleading for an assertion failure).
            exit_code: Some(1),
            artifact,
        }
    }
}
