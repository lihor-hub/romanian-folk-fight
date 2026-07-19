use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::symlink;

struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "xtask-runtime-workspace-root-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("nested/directory"))
            .expect("temporary nested workspace directory is created");
        fs::write(root.join("Cargo.toml"), "[workspace]\nresolver = \"3\"\n")
            .expect("temporary workspace manifest is written");
        Self { root }
    }

    fn nested_dir(&self) -> PathBuf {
        self.root.join("nested/directory")
    }

    #[cfg(unix)]
    fn link_workspace_inputs(&self, source_workspace: &Path) {
        for relative in ["assets", "src", "index.html"] {
            symlink(source_workspace.join(relative), self.root.join(relative))
                .unwrap_or_else(|error| panic!("failed to link {relative}: {error}"));
        }
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn startup_reports_workspace_root_resolved_from_invocation_directory() {
    let workspace = TempWorkspace::new();
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("--help")
        .current_dir(workspace.nested_dir())
        .output()
        .expect("xtask binary launches");

    assert!(
        output.status.success(),
        "xtask failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("xtask stdout is UTF-8");
    let expected_root = workspace
        .root
        .canonicalize()
        .expect("temporary workspace root canonicalizes");
    assert!(
        stdout.contains(&format!(
            "cargo xtask: workspace root: {}",
            display_path(&expected_root)
        )),
        "startup output did not report the invocation workspace root:\n{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn assets_check_uses_invocation_workspace_for_inputs_and_artifacts() {
    let source_workspace = caller_workspace_root();
    let workspace = TempWorkspace::new();
    workspace.link_workspace_inputs(&source_workspace);

    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["assets", "check"])
        .current_dir(&workspace.root)
        .output()
        .expect("xtask binary launches");

    assert!(
        output.status.success(),
        "xtask failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let runtime_root = workspace
        .root
        .canonicalize()
        .expect("temporary workspace root canonicalizes");
    let artifact = runtime_root.join("target/xtask-artifacts/assets-check.log");
    let stdout = String::from_utf8(output.stdout).expect("xtask stdout is UTF-8");
    assert!(
        stdout.contains(&format!(
            "cargo xtask: workspace root: {}",
            runtime_root.display()
        )),
        "startup did not report the invocation root:\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("log: {}", artifact.display())),
        "assets check did not report an invocation-root artifact:\n{stdout}"
    );
    assert!(artifact.is_file(), "assets-check artifact was not written");
}

#[test]
fn xtask_sources_do_not_embed_a_compile_time_workspace_root() {
    let workspace_root = caller_workspace_root();
    let mut rust_sources = Vec::new();
    collect_rust_sources(&workspace_root.join("xtask/src"), &mut rust_sources);

    let offenders = rust_sources
        .into_iter()
        .filter(|path| {
            fs::read_to_string(path)
                .expect("xtask Rust source is readable")
                .contains("env!(\"CARGO_MANIFEST_DIR\")")
        })
        .collect::<Vec<_>>();
    assert!(
        offenders.is_empty(),
        "xtask sources embed compile-time workspace roots: {offenders:?}"
    );
}

fn caller_workspace_root() -> PathBuf {
    let current_dir = std::env::current_dir().expect("test invocation directory is readable");
    current_dir
        .ancestors()
        .find(|candidate| {
            fs::read_to_string(candidate.join("Cargo.toml"))
                .is_ok_and(|manifest| manifest.lines().any(|line| line.trim() == "[workspace]"))
        })
        .expect("test is invoked from inside a Cargo workspace")
        .to_path_buf()
}

fn collect_rust_sources(directory: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("xtask source directory is readable") {
        let path = entry.expect("xtask source entry is readable").path();
        if path.is_dir() {
            collect_rust_sources(&path, out);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}
