//! Exact production/review smoke bundles. Explicit consumers never build.
//!
//! Identity covers the supported repository recipe, including dirty/untracked
//! source and assets, but not caches, logs, baselines, or documentation. Unknown
//! compiler/Trunk overrides and build scripts/path dependencies fail closed.
//! Cargo aliases, target/cache directories, jobs, and network settings do not
//! affect this recipe. Config/credential contents and arbitrary environment
//! values are never copied or hashed into the public manifest.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::SmokeError;

type Result<T> = std::result::Result<T, SmokeError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Variant {
    Production,
    Review,
}

impl Variant {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "production" => Ok(Self::Production),
            "review" => Ok(Self::Review),
            _ => Err(error("variant must be production or review")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Review => "review",
        }
    }
}

/// Constructed only after complete manifest/input/output validation.
pub struct Bundle {
    dist: PathBuf,
}

impl Bundle {
    pub fn dist_dir(&self) -> PathBuf {
        self.dist.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    variant: Variant,
    profile: String,
    target: String,
    optimizer: String,
    /// None follows Cargo/profile defaults; Some(false) is the existing CI disable.
    cargo_incremental: Option<bool>,
    public_url: String,
    inputs: BTreeMap<String, String>,
    tools: BTreeMap<String, String>,
    provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Provenance {
    Local,
    Github {
        repository: String,
        run: String,
        attempt: String,
        sha: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    identity: Identity,
    outputs: BTreeMap<String, String>,
}

fn error(message: impl Into<String>) -> SmokeError {
    SmokeError::usage(format!("smoke bundle: {}", message.into()))
}

fn io<T>(value: std::io::Result<T>) -> Result<T> {
    value.map_err(|_| error("filesystem operation failed (no bundle accepted)"))
}

fn reject_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(error("symlinked input/output path"));
        }
    }
    Ok(())
}

fn read_toml(path: &Path) -> Result<toml::Value> {
    reject_links(path)?;
    // Do not expose TOML error excerpts: external Cargo configs may hold secrets.
    if !io(fs::symlink_metadata(path))?.is_file() {
        return Err(error("non-regular configuration input"));
    }
    toml::from_str(&io(fs::read_to_string(path))?).map_err(|_| error("invalid configuration"))
}

fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn walk(root: &Path, directory: &Path, files: &mut BTreeMap<String, String>) -> Result<()> {
    reject_links(directory)?;
    let metadata = io(fs::symlink_metadata(directory))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(error("non-regular input/output directory"));
    }
    for entry in io(fs::read_dir(directory))? {
        let path = io(entry)?.path();
        let metadata = io(fs::symlink_metadata(&path))?;
        if metadata.file_type().is_symlink() {
            return Err(error("symlinks are not supported in smoke inputs/outputs"));
        }
        if metadata.is_dir() {
            walk(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| error("path escapes bundle/input root"))?;
            let components: Result<Vec<_>> = relative
                .components()
                .map(|component| {
                    let std::path::Component::Normal(name) = component else {
                        return Err(error("non-relative inventory path"));
                    };
                    name.to_str()
                        .filter(|s| !s.contains('\\'))
                        .ok_or_else(|| error("non-portable inventory path"))
                })
                .collect();
            let name = components?.join("/");
            if files.insert(name, digest(&io(fs::read(path))?)).is_some() {
                return Err(error("duplicate inventory path"));
            }
        } else {
            return Err(error("non-regular smoke input/output"));
        }
    }
    Ok(())
}

fn add_file(root: &Path, relative: &str, files: &mut BTreeMap<String, String>) -> Result<()> {
    let path = root.join(relative);
    reject_links(&path)?;
    if !io(fs::symlink_metadata(&path))?.is_file() {
        return Err(error("non-regular build input"));
    }
    files.insert(relative.to_owned(), digest(&io(fs::read(path))?));
    Ok(())
}

fn has_external_dependencies(value: &toml::Value) -> bool {
    let Some(table) = value.as_table() else {
        return false;
    };
    table.iter().any(|(key, value)| {
        matches!(key.as_str(), "patch" | "replace")
            || (matches!(
                key.as_str(),
                "dependencies" | "dev-dependencies" | "build-dependencies"
            ) && value
                .as_table()
                .is_some_and(|deps| deps.values().any(|dep| dep.get("path").is_some())))
            || has_external_dependencies(value)
    })
}

fn check_manifest(path: &Path) -> Result<()> {
    let manifest = read_toml(path)?;
    if manifest.get("lib").is_some()
        || manifest.get("bin").is_some()
        || manifest
            .get("workspace")
            .and_then(|w| w.get("members"))
            .and_then(toml::Value::as_array)
            .is_some_and(|members| members.iter().any(|m| m.as_str() != Some("xtask")))
        || manifest
            .get("features")
            .and_then(|f| f.get("default"))
            .and_then(toml::Value::as_array)
            .is_some_and(|f| !f.is_empty())
        || manifest
            .get("features")
            .and_then(|f| f.get("review"))
            .and_then(toml::Value::as_array)
            .is_some_and(|f| !f.is_empty())
    {
        return Err(error(
            "unsupported Cargo target/workspace/feature configuration",
        ));
    }
    if has_external_dependencies(&manifest)
        || manifest
            .get("package")
            .and_then(|p| p.get("build"))
            .is_some()
        || path.parent().is_some_and(|p| p.join("build.rs").exists())
    {
        return Err(error(
            "custom build scripts/path dependencies need explicit smoke identity support",
        ));
    }
    Ok(())
}

fn cargo_config(path: &Path) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    if !io(fs::symlink_metadata(path))?.is_file() {
        return Err(error("symlinked Cargo config unsupported"));
    }
    let value = read_toml(path)?;
    let mut wrapper = false;
    for (section, fields) in value
        .as_table()
        .ok_or_else(|| error("invalid Cargo config"))?
    {
        let fields = fields
            .as_table()
            .ok_or_else(|| error("unsupported Cargo config"))?;
        for (key, value) in fields {
            match (section.as_str(), key.as_str()) {
                ("alias", _)
                | ("build", "target-dir" | "jobs")
                | ("net", "offline" | "retry" | "git-fetch-with-cli") => {}
                ("build", "rustc-wrapper") if value.as_str() == Some("sccache") => wrapper = true,
                _ => {
                    return Err(error(
                        "unsupported Cargo config; only cache/alias/jobs/network and sccache settings are supported",
                    ));
                }
            }
        }
    }
    Ok(wrapper)
}

fn check_environment(env: &BTreeMap<String, String>) -> Result<(bool, Option<bool>)> {
    let mut wrapper = false;
    let mut cargo_incremental = None;
    for (key, value) in env {
        if value.is_empty() {
            continue;
        }
        if key == "CARGO_INCREMENTAL" && value == "0" {
            cargo_incremental = Some(false);
            continue;
        }
        if key == "RUSTC_WRAPPER" && value == "sccache" {
            wrapper = true;
            continue;
        }
        let forbidden = matches!(
            key.as_str(),
            "RUSTFLAGS"
                | "RUSTDOCFLAGS"
                | "RUSTC"
                | "RUSTC_WRAPPER"
                | "RUSTC_WORKSPACE_WRAPPER"
                | "CARGO_ENCODED_RUSTFLAGS"
                | "CARGO_ENCODED_RUSTDOCFLAGS"
                | "CARGO_INCREMENTAL"
                | "CC"
                | "CFLAGS"
                | "CXX"
                | "CXXFLAGS"
        ) || key.starts_with("RUSTC_")
            || key.starts_with("CARGO_UNSTABLE_")
            || key.starts_with("CARGO_PROFILE_")
            || (key.starts_with("CARGO_BUILD_")
                && !matches!(
                    key.as_str(),
                    "CARGO_BUILD_JOBS" | "CARGO_BUILD_BUILD_DIR" | "CARGO_BUILD_TARGET_DIR"
                ))
            || (key.starts_with("CARGO_TARGET_") && key != "CARGO_TARGET_DIR")
            || (key.starts_with("TRUNK_")
                && !matches!(
                    key.as_str(),
                    "TRUNK_COLOR" | "TRUNK_OFFLINE" | "TRUNK_SKIP_VERSION_CHECK"
                ));
        if forbidden {
            // Name only, never the value or a hash of it.
            return Err(error(format!(
                "unsupported build override {key}; no artifact reused"
            )));
        }
    }
    Ok((wrapper, cargo_incremental))
}

fn version(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    let output = io(Command::new(program).current_dir(root).args(args).output())?;
    if !output.status.success() {
        return Err(error(format!("cannot identify {program}")));
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|_| error("invalid tool version"))
}

fn trunk_tool_versions(report: &str) -> Result<BTreeMap<String, String>> {
    let mut section = "";
    let mut versions = BTreeMap::new();
    for line in report.lines() {
        if !line.is_empty() && !line.starts_with(' ') {
            section = line.trim();
        }
        if !matches!(section, "wasm-bindgen" | "wasm-opt") {
            continue;
        }
        for field in ["Installed Version", "Default Version"] {
            if let Some(value) = line.trim().strip_prefix(&format!("{field}: ")) {
                versions.insert(format!("{section}-{field}"), value.to_owned());
            }
        }
    }
    if versions.len() != 4 || versions.values().any(|v| v.is_empty()) {
        return Err(error("cannot identify Trunk's wasm tool selection"));
    }
    Ok(versions)
}

fn cargo_home(override_home: Option<&str>, fallback_home: Option<PathBuf>) -> Result<PathBuf> {
    let home = override_home
        .map(PathBuf::from)
        .or_else(|| fallback_home.map(|h| h.join(".cargo")))
        .ok_or_else(|| error("cannot locate Cargo configuration"))?;
    if !home.is_absolute() {
        return Err(error("relative CARGO_HOME is unsupported"));
    }
    Ok(home)
}

fn capture(root: &Path, variant: Variant) -> Result<Identity> {
    let env: BTreeMap<String, String> = std::env::vars_os()
        .map(|(key, value)| {
            Ok((
                key.into_string()
                    .map_err(|_| error("non-UTF-8 environment name"))?,
                value
                    .into_string()
                    .map_err(|_| error("non-UTF-8 environment value"))?,
            ))
        })
        .collect::<Result<_>>()?;
    let (mut wrapper, cargo_incremental) = check_environment(&env)?;
    for ancestor in root.ancestors() {
        for file in ["config", "config.toml"] {
            wrapper |= cargo_config(&ancestor.join(".cargo").join(file))?;
        }
    }
    let home = cargo_home(env.get("CARGO_HOME").map(String::as_str), dirs::home_dir())?;
    for file in ["config", "config.toml"] {
        wrapper |= cargo_config(&home.join(file))?;
    }
    let mut tools = BTreeMap::from([
        ("rustc".to_owned(), version(root, "rustc", &["-Vv"])?),
        ("cargo".to_owned(), version(root, "cargo", &["-V"])?),
        ("trunk".to_owned(), version(root, "trunk", &["--version"])?),
    ]);
    // Record reported installed/default versions, not cache locations or URLs.
    tools.extend(trunk_tool_versions(&version(
        root,
        "trunk",
        &["tools", "show"],
    )?)?);
    if wrapper {
        tools.insert("sccache".into(), version(root, "sccache", &["--version"])?);
    }
    let provenance = if env.get("GITHUB_ACTIONS").is_some_and(|s| s == "true") {
        let get = |key: &str| {
            env.get(key)
                .filter(|s| !s.is_empty())
                .cloned()
                .ok_or_else(|| error("incomplete GitHub provenance"))
        };
        Provenance::Github {
            repository: get("GITHUB_REPOSITORY")?,
            run: get("GITHUB_RUN_ID")?,
            attempt: get("GITHUB_RUN_ATTEMPT")?,
            sha: get("GITHUB_SHA")?,
        }
    } else {
        Provenance::Local
    };
    let mut captured = identity(root, variant, tools, provenance)?;
    captured.cargo_incremental = cargo_incremental;
    Ok(captured)
}

fn identity(
    root: &Path,
    variant: Variant,
    mut tools: BTreeMap<String, String>,
    provenance: Provenance,
) -> Result<Identity> {
    check_manifest(&root.join("Cargo.toml"))?;
    check_manifest(&root.join("xtask/Cargo.toml"))?;
    let config = read_toml(&root.join("Trunk.toml"))?;
    let build = config
        .get("build")
        .and_then(toml::Value::as_table)
        .ok_or_else(|| error("missing Trunk build config"))?;
    for key in build.keys() {
        if !matches!(key.as_str(), "target" | "dist" | "public_url") {
            return Err(error("unsupported Trunk build configuration"));
        }
    }
    if build.get("target").and_then(toml::Value::as_str) != Some("index.html")
        || config.get("hooks").is_some()
        || config.get("tools").is_some()
        || config.as_table().is_none_or(|t| {
            t.keys()
                .any(|k| !matches!(k.as_str(), "build" | "serve" | "watch"))
        })
    {
        return Err(error("unsupported Trunk recipe/hooks/tool overrides"));
    }
    reject_links(&root.join("index.html"))?;
    let html = io(fs::read_to_string(root.join("index.html")))?;
    let pipelines: Vec<_> = html
        .split("<link")
        .skip(1)
        .filter_map(|s| s.split('>').next())
        .filter(|s| s.contains("data-trunk"))
        .collect();
    let normalized: Vec<_> = pipelines
        .iter()
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    if html.to_ascii_lowercase().matches("data-trunk").count() != 3
        || normalized.len() != 3
        || !normalized
            .iter()
            .any(|s| s == "data-trunk rel=\"rust\" data-wasm-opt=\"z\" /")
        || !normalized
            .iter()
            .any(|s| s == "data-trunk rel=\"copy-dir\" href=\"assets\" /")
        || !normalized
            .iter()
            .any(|s| s == "data-trunk rel=\"copy-file\" href=\"manifest.webmanifest\" /")
    {
        return Err(error(
            "unsupported HTML build pipelines; expected Rust -Oz, assets and root manifest copy",
        ));
    }
    let mut inputs = BTreeMap::new();
    for directory in ["src", "assets", "xtask/src"] {
        walk(root, &root.join(directory), &mut inputs)?;
    }
    for file in [
        "Cargo.toml",
        "Cargo.lock",
        "xtask/Cargo.toml",
        "Trunk.toml",
        "index.html",
        "manifest.webmanifest",
    ] {
        add_file(root, file, &mut inputs)?;
    }
    for file in ["rust-toolchain", "rust-toolchain.toml"] {
        if root.join(file).exists() {
            add_file(root, file, &mut inputs)?;
        }
    }
    // Record the configured public URL; Trunk retains its existing URL handling.
    let public_url = build
        .get("public_url")
        .and_then(toml::Value::as_str)
        .unwrap_or("/")
        .to_owned();
    let lock = read_toml(&root.join("Cargo.lock"))?;
    let bindgen = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .and_then(|p| {
            p.iter()
                .find(|p| p.get("name").and_then(toml::Value::as_str) == Some("wasm-bindgen"))
        })
        .and_then(|p| p.get("version"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| error("missing locked wasm-bindgen version"))?;
    tools.insert("wasm-bindgen-selected-from-lock".into(), bindgen.into());
    Ok(Identity {
        variant,
        profile: "release".into(),
        target: "wasm32-unknown-unknown".into(),
        optimizer: "-Oz".into(),
        cargo_incremental: None,
        public_url,
        inputs,
        tools,
        provenance,
    })
}

fn validate(path: &Path, expected: &Identity) -> Result<Bundle> {
    if io(fs::symlink_metadata(path))?.file_type().is_symlink() {
        return Err(error("symlinked bundle root"));
    }
    let manifest_path = path.join("manifest.json");
    if !io(fs::symlink_metadata(&manifest_path))?.is_file() {
        return Err(error("non-regular bundle manifest"));
    }
    let bytes = io(fs::read(manifest_path))?;
    let seal = path.join("manifest.blake3");
    if !io(fs::symlink_metadata(&seal))?.is_file() {
        return Err(error("non-regular manifest checksum"));
    }
    if digest(&bytes) != io(fs::read_to_string(seal))? {
        return Err(error("tampered manifest"));
    }
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| error("missing/corrupt bundle manifest"))?;
    if manifest.format != 1 || &manifest.identity != expected {
        return Err(error("bundle input/config/tool/provenance mismatch"));
    }
    let dist = path.join("dist");
    if io(fs::symlink_metadata(&dist))?.file_type().is_symlink() {
        return Err(error("symlinked dist"));
    }
    let mut outputs = BTreeMap::new();
    walk(&dist, &dist, &mut outputs)?;
    if !outputs.contains_key("index.html")
        || !outputs.keys().any(|p| p.ends_with(".wasm"))
        || !outputs.keys().any(|p| p.ends_with(".js"))
        || outputs != manifest.outputs
        || expected
            .inputs
            .iter()
            .filter(|(path, _)| {
                path.starts_with("assets/") || path.as_str() == "manifest.webmanifest"
            })
            .any(|(path, checksum)| outputs.get(path) != Some(checksum))
    {
        return Err(error("missing/partial/tampered bundle outputs"));
    }
    Ok(Bundle { dist })
}

fn trunk_command(root: &Path, variant: Variant, dist: &Path) -> Command {
    let mut cmd = Command::new("trunk");
    cmd.current_dir(root).args(["build", "--release"]);
    if variant == Variant::Review {
        cmd.args(["--features", "review"]);
    }
    cmd.arg("--dist").arg(dist);
    cmd
}

fn publish(
    destination: &Path,
    expected: Identity,
    build: impl FnOnce(&Path) -> Result<()>,
    recapture: impl FnOnce() -> Result<Identity>,
) -> Result<Bundle> {
    reject_links(destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| error("invalid artifact root"))?;
    io(fs::create_dir_all(parent))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| error("clock before epoch"))?
        .as_nanos();
    let staging = parent.join(format!(".staging-{}-{nonce}", std::process::id()));
    io(fs::create_dir(&staging))?;
    let result = (|| {
        let dist = staging.join("dist");
        build(&dist)?;
        if expected != recapture()? {
            return Err(error("build inputs changed during build"));
        }
        let mut outputs = BTreeMap::new();
        walk(&dist, &dist, &mut outputs)?;
        let manifest = Manifest {
            format: 1,
            identity: expected.clone(),
            outputs,
        };
        let bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|_| error("manifest serialization failed"))?;
        io(fs::write(staging.join("manifest.blake3"), digest(&bytes)))?;
        io(fs::write(staging.join("manifest.json"), bytes))?;
        validate(&staging, &expected)?;
        if destination.exists() {
            if io(fs::symlink_metadata(destination))?
                .file_type()
                .is_symlink()
            {
                return Err(error("symlinked destination"));
            }
            io(fs::remove_dir_all(destination))?;
        }
        io(fs::rename(&staging, destination))?;
        validate(destination, &expected)
    })();
    if staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn default_root() -> PathBuf {
    super::workspace_root().join("target/web-smoke-bundles")
}

fn root_is_safe(workspace: &Path, artifacts: &Path) -> Result<()> {
    // Reject symlink ancestors and lexical traversal, including non-existing paths.
    if artifacts
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(error("artifact path traversal"));
    }
    for ancestor in artifacts.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(error("symlinked artifact ancestor"));
        }
    }
    for input in ["src", "assets", "xtask", ".cargo"] {
        if artifacts.starts_with(workspace.join(input)) {
            return Err(error("artifact directory overlaps build inputs"));
        }
    }
    if workspace.starts_with(artifacts) {
        return Err(error("artifact root contains workspace"));
    }
    Ok(())
}

/// Build-only producer; local exact matches can be reused, CI identity binds run/attempt.
pub fn produce(variant: Variant, artifacts: &Path) -> Result<Bundle> {
    let workspace = super::workspace_root();
    let artifacts = if artifacts.is_absolute() {
        artifacts.to_owned()
    } else {
        workspace.join(artifacts)
    };
    root_is_safe(&workspace, &artifacts)?;
    let expected = capture(&workspace, variant)?;
    let destination = artifacts.join(variant.name());
    obtain(
        &destination,
        expected,
        |dist| {
            crate::process::run_step(
                &format!("web-smoke: build {} bundle", variant.name()),
                trunk_command(&workspace, variant, dist),
            )?;
            Ok(())
        },
        || capture(&workspace, variant),
    )
}

fn obtain(
    destination: &Path,
    expected: Identity,
    build: impl FnOnce(&Path) -> Result<()>,
    recapture: impl FnOnce() -> Result<Identity>,
) -> Result<Bundle> {
    if let Ok(bundle) = validate(destination, &expected) {
        println!(
            "smoke bundle: validated reuse ({})",
            expected.variant.name()
        );
        return Ok(bundle);
    }
    println!("smoke bundle: building {}", expected.variant.name());
    publish(destination, expected, build, recapture)
}

/// Explicit consumption is validation-only. No fallback or rebuild on any error.
pub fn consume(variant: Variant, artifacts: &Path) -> Result<Bundle> {
    let workspace = super::workspace_root();
    let artifacts = if artifacts.is_absolute() {
        artifacts.to_owned()
    } else {
        workspace.join(artifacts)
    };
    root_is_safe(&workspace, &artifacts)?;
    let expected = capture(&workspace, variant)?;
    let bundle = validate(&artifacts.join(variant.name()), &expected)?;
    println!(
        "smoke bundle: consumed verified {} artifact",
        variant.name()
    );
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "rff-bundle-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(root.join("src")).unwrap();
            fs::create_dir_all(root.join("assets")).unwrap();
            fs::create_dir_all(root.join("xtask/src")).unwrap();
            for (name, value) in [
                ("Cargo.toml", "[package]\nname='fixture'\n"),
                ("xtask/Cargo.toml", "[package]\nname='xtask'\n"),
                (
                    "Cargo.lock",
                    "[[package]]\nname='wasm-bindgen'\nversion='0.2.126'\n",
                ),
                ("Trunk.toml", "[build]\ntarget='index.html'\ndist='dist'\n"),
                (
                    "index.html",
                    "<link data-trunk rel=\"rust\" data-wasm-opt=\"z\" /><link data-trunk rel=\"copy-dir\" href=\"assets\" /><link data-trunk rel=\"copy-file\" href=\"manifest.webmanifest\" />",
                ),
                ("manifest.webmanifest", "{}"),
                ("src/main.rs", "fn main() {}"),
                ("assets/example.txt", "asset"),
                ("xtask/src/main.rs", "fn main() {}"),
            ] {
                fs::write(root.join(name), value).unwrap();
            }
            Self(root)
        }
        fn identity(&self, variant: Variant) -> Identity {
            identity(
                &self.0,
                variant,
                BTreeMap::from([("rustc".into(), "fixture-compiler".into())]),
                Provenance::Local,
            )
            .unwrap()
        }
        fn destination(&self) -> PathBuf {
            self.0.join("target/bundles/production")
        }
        fn ready(&self) -> Identity {
            let expected = self.identity(Variant::Production);
            publish(&self.destination(), expected.clone(), fake_build, || {
                Ok(expected.clone())
            })
            .unwrap();
            expected
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fake_build(dist: &Path) -> Result<()> {
        io(fs::create_dir_all(dist))?;
        for file in ["index.html", "game.js", "game.wasm"] {
            io(fs::write(dist.join(file), file))?;
        }
        io(fs::write(dist.join("manifest.webmanifest"), "{}"))?;
        io(fs::create_dir(dist.join("assets")))?;
        io(fs::write(dist.join("assets/example.txt"), "asset"))?;
        Ok(())
    }

    #[test]
    fn web_manifest_metadata_uses_relative_links_and_existing_png_sizes() {
        let root = crate::web_smoke::workspace_root();
        let html = fs::read_to_string(root.join("index.html")).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("manifest.webmanifest")).unwrap()).unwrap();
        assert_eq!(manifest["name"], "Romanian Folk Fight");
        assert_eq!(manifest["short_name"], "Folk Fight");
        assert_eq!(manifest["start_url"], ".");
        assert_eq!(manifest["display"], "standalone");
        assert_eq!(manifest["background_color"], "#1a1214");
        assert_eq!(manifest["theme_color"], "#7a1f1f");
        assert!(html.contains("background: #1a1214;"));
        assert!(html.contains("<meta name=\"theme-color\" content=\"#7a1f1f\" />"));
        assert!(html.contains("<link rel=\"manifest\" href=\"manifest.webmanifest\" />"));
        assert!(
            html.contains("<link data-trunk rel=\"copy-file\" href=\"manifest.webmanifest\" />")
        );
        for (attribute, field) in [
            ("property", "og:site_name"),
            ("property", "og:image:alt"),
            ("name", "twitter:image:alt"),
        ] {
            let tag = html
                .split("<meta")
                .filter_map(|part| part.split('>').next())
                .find(|tag| tag.contains(&format!("{attribute}=\"{field}\"")))
                .unwrap();
            let content = tag
                .split_once("content=\"")
                .unwrap()
                .1
                .split('"')
                .next()
                .unwrap();
            assert!(!content.trim().is_empty(), "{field}");
            if field == "og:site_name" {
                assert_eq!(content, manifest["name"].as_str().unwrap());
            }
        }
        let icons = manifest["icons"].as_array().unwrap();
        let sources = [
            "assets/web/favicon-32.png",
            "assets/web/apple-touch-icon.png",
            "assets/web/og-image.png",
        ];
        assert_eq!(icons.len(), sources.len());
        for (icon, source) in icons.iter().zip(sources) {
            assert_eq!(icon["src"], source);
            assert!(
                Path::new(source)
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)))
            );
            assert_eq!(icon["type"], "image/png");
            let (width, height) = image::image_dimensions(root.join(source)).unwrap();
            assert_eq!(icon["sizes"], format!("{width}x{height}"));
        }
    }

    #[test]
    fn web_manifest_bytes_invalidate_and_missing_or_changed_copies_reject() {
        let f = Fixture::new();
        let expected = f.ready();
        let manifest = f.0.join("manifest.webmanifest");
        fs::write(&manifest, "{ }").unwrap();
        let changed = f.identity(Variant::Production);
        assert_ne!(changed, expected);
        assert!(validate(&f.destination(), &changed).is_err());
        fs::write(manifest, "{}").unwrap();
        for wrong in [None, Some("{ }")] {
            assert!(
                publish(
                    &f.destination(),
                    expected.clone(),
                    |dist| {
                        fake_build(dist)?;
                        let output = dist.join("manifest.webmanifest");
                        match wrong {
                            None => io(fs::remove_file(output))?,
                            Some(bytes) => io(fs::write(output, bytes))?,
                        }
                        Ok(())
                    },
                    || Ok(expected.clone()),
                )
                .is_err()
            );
            assert!(validate(&f.destination(), &expected).is_ok());
        }
    }

    #[test]
    fn web_manifest_copy_recipe_rejects_unknown_sources_and_attributes() {
        let f = Fixture::new();
        let path = f.0.join("index.html");
        let html = fs::read_to_string(&path).unwrap();
        assert!(
            identity(
                &f.0,
                Variant::Production,
                BTreeMap::new(),
                Provenance::Local
            )
            .is_ok()
        );
        for replacement in [
            "data-trunk rel=\"copy-file\" href=\"other.webmanifest\" /",
            "data-trunk rel=\"copy-file\" href=\"../manifest.webmanifest\" /",
            "data-trunk rel=\"copy-file\" href=\"/manifest.webmanifest\" /",
            "data-trunk rel=\"copy-file\" href=\"assets/manifest.webmanifest\" /",
            "data-trunk rel=\"copy-file\" href=\"manifest\\webmanifest\" /",
            "data-trunk rel=\"copy-file\" href=\"manifest.webmanifest\" data-target-path=\"elsewhere\" /",
            "data-trunk rel=\"copy-dir\" href=\"manifest.webmanifest\" /",
        ] {
            fs::write(
                &path,
                html.replace(
                    "data-trunk rel=\"copy-file\" href=\"manifest.webmanifest\" /",
                    replacement,
                ),
            )
            .unwrap();
            assert!(
                identity(
                    &f.0,
                    Variant::Production,
                    BTreeMap::new(),
                    Provenance::Local
                )
                .is_err()
            );
        }
        fs::write(
            &path,
            format!("{html}<link data-trunk rel=\"copy-file\" href=\"manifest.webmanifest\" />"),
        )
        .unwrap();
        assert!(
            identity(
                &f.0,
                Variant::Production,
                BTreeMap::new(),
                Provenance::Local
            )
            .is_err()
        );
    }

    #[test]
    fn web_manifest_source_must_be_regular_and_not_a_symlink() {
        let f = Fixture::new();
        assert!(
            identity(
                &f.0,
                Variant::Production,
                BTreeMap::new(),
                Provenance::Local
            )
            .is_ok()
        );
        let manifest = f.0.join("manifest.webmanifest");
        fs::remove_file(&manifest).unwrap();
        assert!(
            identity(
                &f.0,
                Variant::Production,
                BTreeMap::new(),
                Provenance::Local
            )
            .is_err()
        );
        fs::create_dir(&manifest).unwrap();
        assert!(
            identity(
                &f.0,
                Variant::Production,
                BTreeMap::new(),
                Provenance::Local
            )
            .is_err()
        );
        fs::remove_dir(&manifest).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(f.0.join("Cargo.toml"), &manifest).unwrap();
            assert!(
                identity(
                    &f.0,
                    Variant::Production,
                    BTreeMap::new(),
                    Provenance::Local
                )
                .is_err()
            );
        }
    }

    #[test]
    fn actual_dirty_and_untracked_inputs_invalidate_but_cache_logs_and_baselines_do_not() {
        let f = Fixture::new();
        let expected = f.identity(Variant::Production);
        for file in [
            "src/main.rs",
            "assets/example.txt",
            "Cargo.lock",
            "Cargo.toml",
            "index.html",
            "xtask/src/main.rs",
        ] {
            let path = f.0.join(file);
            let original = fs::read(&path).unwrap();
            let mut changed = original.clone();
            changed.extend_from_slice(if file.ends_with(".toml") || file.ends_with(".lock") {
                b"\n# changed\n"
            } else {
                b"\nchanged\n"
            });
            fs::write(&path, changed).unwrap();
            assert_ne!(f.identity(Variant::Production), expected, "{file}");
            fs::write(path, original).unwrap();
        }
        fs::write(f.0.join("assets/untracked.txt"), "new").unwrap();
        assert_ne!(f.identity(Variant::Production), expected);
        fs::remove_file(f.0.join("assets/untracked.txt")).unwrap();
        for dir in ["target", "tests/visual/baselines", ".local"] {
            fs::create_dir_all(f.0.join(dir)).unwrap();
            fs::write(f.0.join(dir).join("incidental.log"), "ignored").unwrap();
        }
        assert_eq!(f.identity(Variant::Production), expected);
        fs::write(
            f.0.join("Trunk.toml"),
            "[build]\ntarget='index.html'\npublic_url='/another/'\n",
        )
        .unwrap();
        assert_ne!(f.identity(Variant::Production), expected);
    }

    #[test]
    fn existing_ci_incremental_disable_is_supported_but_other_overrides_still_reject() {
        assert_eq!(
            check_environment(&BTreeMap::from([("CARGO_INCREMENTAL".into(), "0".into())])).unwrap(),
            (false, Some(false))
        );
        assert_eq!(check_environment(&BTreeMap::new()).unwrap(), (false, None));
        for value in ["1", "unsupported"] {
            assert!(
                check_environment(&BTreeMap::from([(
                    "CARGO_INCREMENTAL".into(),
                    value.into()
                )]))
                .is_err()
            );
        }
    }

    #[test]
    fn cargo_home_supports_system_home_fallback_without_environment_mutation() {
        let f = Fixture::new();
        assert_eq!(
            cargo_home(None, Some(f.0.clone())).unwrap(),
            f.0.join(".cargo")
        );
        assert_eq!(cargo_home(Some(f.0.to_str().unwrap()), None).unwrap(), f.0);
        assert!(cargo_home(None, None).is_err());
        assert!(cargo_home(Some("relative-home"), Some(f.0.clone())).is_err());
    }

    #[test]
    fn tool_report_records_only_named_wasm_versions_and_requires_complete_identification() {
        let report = "sass\n    Default Version: unused\nwasm-bindgen\n    Installed Version: n/a\n    Default Version: 0.2.89\n    Location: PRIVATE-PATH\nwasm-opt\n    Installed Version: n/a\n    Default Version: version_116\n    Download URL: PRIVATE-URL\n";
        let versions = trunk_tool_versions(report).unwrap();
        assert_eq!(versions.len(), 4);
        assert_eq!(versions["wasm-opt-Default Version"], "version_116");
        assert!(!format!("{versions:?}").contains("PRIVATE"));
        assert!(trunk_tool_versions("wasm-opt\n    Default Version: version_116").is_err());
    }

    #[test]
    fn manifest_metadata_and_checksum_tampering_fail_closed() {
        let f = Fixture::new();
        let expected = f.ready();
        let path = f.destination().join("manifest.json");
        let original = fs::read_to_string(&path).unwrap();
        fs::write(&path, original.replace("production", "review")).unwrap();
        assert!(validate(&f.destination(), &f.identity(Variant::Review)).is_err());
        fs::write(&path, &original).unwrap();
        fs::write(f.destination().join("manifest.blake3"), "tampered").unwrap();
        assert!(validate(&f.destination(), &expected).is_err());
    }

    #[test]
    fn successful_but_partial_build_cannot_publish_a_complete_looking_manifest() {
        let f = Fixture::new();
        let expected = f.identity(Variant::Production);
        assert!(
            publish(
                &f.destination(),
                expected.clone(),
                |dist| {
                    fake_build(dist)?;
                    io(fs::remove_dir_all(dist.join("assets")))?;
                    Ok(())
                },
                || Ok(expected.clone())
            )
            .is_err()
        );
        assert!(!f.destination().exists());
    }

    #[test]
    fn missing_partial_extra_and_tampered_outputs_fail_closed() {
        let f = Fixture::new();
        let expected = f.identity(Variant::Production);
        assert!(validate(&f.destination(), &expected).is_err());
        f.ready();
        let wasm = f.destination().join("dist/game.wasm");
        fs::write(&wasm, "tampered").unwrap();
        assert!(validate(&f.destination(), &expected).is_err());
        fs::remove_file(&wasm).unwrap();
        assert!(validate(&f.destination(), &expected).is_err());
        fs::write(wasm, "game.wasm").unwrap();
        fs::write(f.destination().join("dist/extra.js"), "extra").unwrap();
        assert!(validate(&f.destination(), &expected).is_err());
    }

    #[test]
    fn variant_tool_input_and_ci_provenance_are_not_interchangeable() {
        let f = Fixture::new();
        let expected = f.ready();
        let mut cases = vec![f.identity(Variant::Review)];
        let mut ci_incremental = expected.clone();
        ci_incremental.cargo_incremental = Some(false);
        cases.push(ci_incremental);
        let mut tools = expected.clone();
        tools.tools.insert("rustc".into(), "another".into());
        cases.push(tools);
        let ci = |run: &str, attempt: &str| Provenance::Github {
            repository: "owner/repo".into(),
            run: run.into(),
            attempt: attempt.into(),
            sha: "sha".into(),
        };
        for provenance in [ci("1", "1"), ci("2", "1"), ci("1", "2")] {
            let mut other = expected.clone();
            other.provenance = provenance;
            cases.push(other);
        }
        for other in cases {
            assert!(validate(&f.destination(), &other).is_err());
        }
        fs::write(f.0.join("assets/example.txt"), "changed").unwrap();
        assert!(validate(&f.destination(), &f.identity(Variant::Production)).is_err());
        // A CI receipt is also rejected for different runs/attempts, not merely for Local.
        let mut first = expected.clone();
        first.provenance = ci("1", "1");
        publish(&f.destination(), first.clone(), fake_build, || {
            Ok(first.clone())
        })
        .unwrap();
        for provenance in [ci("2", "1"), ci("1", "2"), Provenance::Local] {
            let mut other = first.clone();
            other.provenance = provenance;
            assert!(validate(&f.destination(), &other).is_err());
        }
    }

    #[test]
    fn build_failures_and_mid_build_input_changes_never_publish_a_new_receipt() {
        let f = Fixture::new();
        let old = f.ready();
        let receipt = fs::read(f.destination().join("manifest.json")).unwrap();
        fs::write(f.0.join("assets/example.txt"), "new").unwrap();
        let next = f.identity(Variant::Production);
        assert!(
            publish(
                &f.destination(),
                next.clone(),
                |dist| {
                    fake_build(dist)?;
                    Err(error("fixture failure"))
                },
                || Ok(next.clone())
            )
            .is_err()
        );
        assert!(
            publish(&f.destination(), next.clone(), fake_build, || Ok(
                old.clone()
            ))
            .is_err()
        );
        assert_eq!(
            fs::read(f.destination().join("manifest.json")).unwrap(),
            receipt
        );
        assert!(validate(&f.destination(), &old).is_ok());
        assert!(validate(&f.destination(), &next).is_err());
        assert!(
            !fs::read_dir(f.destination().parent().unwrap())
                .unwrap()
                .any(|e| e
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".staging"))
        );
    }

    #[test]
    fn batches_build_only_once_per_exact_variant_and_keep_release_optimizer_and_features() {
        let f = Fixture::new();
        let count = AtomicUsize::new(0);
        for _ in 0..3 {
            for variant in [Variant::Production, Variant::Review] {
                let expected = f.identity(variant);
                obtain(
                    &f.0.join("target/bundles").join(variant.name()),
                    expected.clone(),
                    |dist| {
                        count.fetch_add(1, Ordering::Relaxed);
                        fake_build(dist)
                    },
                    || Ok(expected.clone()),
                )
                .unwrap();
                let command = trunk_command(&f.0, variant, Path::new("output"));
                let args: Vec<_> = command.get_args().map(|s| s.to_str().unwrap()).collect();
                let wanted = if variant == Variant::Review {
                    vec![
                        "build",
                        "--release",
                        "--features",
                        "review",
                        "--dist",
                        "output",
                    ]
                } else {
                    vec!["build", "--release", "--dist", "output"]
                };
                assert_eq!(args, wanted);
                assert_eq!(expected.optimizer, "-Oz");
            }
        }
        assert_eq!(count.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn unknown_flags_configs_and_build_pipelines_reject_without_exposing_values() {
        let f = Fixture::new();
        for key in [
            "RUSTFLAGS",
            "TRUNK_BUILD_FEATURES",
            "CARGO_PROFILE_RELEASE_OPT_LEVEL",
            "RUSTC_WRAPPER",
        ] {
            let error = check_environment(&BTreeMap::from([(key.into(), "DO-NOT-EXPOSE".into())]))
                .unwrap_err()
                .to_string();
            assert!(!error.contains("DO-NOT-EXPOSE"));
        }
        let config = f.0.join("config.toml");
        fs::write(
            &config,
            "[alias]\nxtask='run -p xtask --'\n[build]\ntarget-dir='cache'\njobs=2\n",
        )
        .unwrap();
        assert!(!cargo_config(&config).unwrap());
        fs::write(&config, "[registry]\ntoken='DO-NOT-EXPOSE'\n").unwrap();
        assert!(
            !cargo_config(&config)
                .unwrap_err()
                .to_string()
                .contains("DO-NOT-EXPOSE")
        );
        fs::write(
            f.0.join("Trunk.toml"),
            "[build]\ntarget='index.html'\nfeatures=['dev']\n",
        )
        .unwrap();
        assert!(
            identity(
                &f.0,
                Variant::Production,
                BTreeMap::new(),
                Provenance::Local
            )
            .is_err()
        );
        fs::write(f.0.join("Trunk.toml"), "[build]\ntarget='index.html'\n").unwrap();
        fs::write(f.0.join("index.html"), "<link data-trunk rel=\"rust\" data-wasm-opt=\"z\" data-cargo-features=\"dev\" /><link data-trunk rel=\"copy-dir\" href=\"assets\" />").unwrap();
        assert!(
            identity(
                &f.0,
                Variant::Production,
                BTreeMap::new(),
                Provenance::Local
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_inputs_outputs_manifests_and_traversal_are_rejected() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let expected = f.ready();
        fs::write(f.0.join("assets/back\\slash"), "collision risk").unwrap();
        assert!(
            identity(
                &f.0,
                Variant::Production,
                BTreeMap::new(),
                Provenance::Local
            )
            .is_err()
        );
        fs::remove_file(f.0.join("assets/back\\slash")).unwrap();
        symlink(f.0.join("Cargo.toml"), f.0.join("assets/escape")).unwrap();
        assert!(
            identity(
                &f.0,
                Variant::Production,
                BTreeMap::new(),
                Provenance::Local
            )
            .is_err()
        );
        fs::remove_file(f.0.join("assets/escape")).unwrap();
        symlink(f.0.join("Cargo.toml"), f.destination().join("dist/escape")).unwrap();
        assert!(validate(&f.destination(), &expected).is_err());
        fs::remove_file(f.destination().join("dist/escape")).unwrap();
        fs::rename(f.destination().join("manifest.json"), f.0.join("receipt")).unwrap();
        symlink(f.0.join("receipt"), f.destination().join("manifest.json")).unwrap();
        assert!(validate(&f.destination(), &expected).is_err());
        assert!(root_is_safe(&f.0, &f.0.join("target/../assets/output")).is_err());
        assert!(root_is_safe(&f.0, &f.0.join("assets/output")).is_err());
        symlink(f.0.join("target"), f.0.join("escape")).unwrap();
        assert!(root_is_safe(&f.0, &f.0.join("escape/output")).is_err());
    }
}
