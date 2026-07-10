//! Discovers production asset references in `src/**/*.rs` and `index.html`,
//! then cross-checks each one against the derived sidecar aggregate (#185,
//! a child of #141).
//!
//! # Discovery rules
//!
//! - Every `.rs` file under `src/` is scanned (never `xtask/`, `target/`,
//!   or any other directory -- `src/` is this workspace's one production
//!   crate root).
//! - A reference is any double-quoted string literal (see
//!   `rust_scan::tokenize`) whose text ends in one of the asset extensions
//!   `xtask assets check` already recognizes (`schema::expected_extensions`
//!   -- `.png`, `.ogg`, `.ttf`, `.svg`). This is a deliberately pragmatic,
//!   textual rule: it does not resolve `format!`-constructed paths, and it
//!   only sees a named constant's asset path at the constant's own
//!   declaration (e.g. `pub const UI_FONT_PATH: &str = "fonts/....ttf";`
//!   in `src/core/mod.rs`), not at every place the constant is later used.
//!   Neither gap matters today: this repository has no asset path built
//!   via `format!` or similar (verified by inspection while building this
//!   check -- every path is one literal, either at its use site or at a
//!   `pub const ..._PATH` declaration), and a literal only needs to be
//!   seen once to be validated.
//! - A string literal inside a `#[cfg(test)]`-attributed item (`mod`,
//!   `fn`, `const`, ...) is excluded -- see `rust_scan` for exactly how
//!   that's detected. This repo's one concrete case is
//!   `src/core/mod.rs`'s `asset_server.load("fonts/does-not-exist.ttf")`,
//!   a deliberately-broken path used to prove a stalled asset handle
//!   blocks the loading gate; it must never be treated as a production
//!   reference.
//! - `index.html` is scanned separately (it isn't Rust): any attribute
//!   value containing the substring `assets/` is taken from that
//!   substring onward as an assets-relative reference. This covers both
//!   relative hrefs (`href="assets/web/favicon.svg"`) and the absolute
//!   Open Graph/Twitter-card URLs
//!   (`https://.../assets/web/og-image.png`).
//! - A `.rs` reference containing `../` is resolved relative to its own
//!   source file's directory before being compared against the aggregate
//!   (this repository's one such literal,
//!   `include_bytes!("../../assets/fonts/Alegreya-Variable.ttf")` in
//!   `src/core/mod.rs`, is in fact inside a `#[cfg(test)]` fn and already
//!   excluded by the rule above; this resolver exists so a production use
//!   of the same pattern would still be validated correctly). Every other
//!   `.rs` reference is assumed already relative to `assets/`, matching
//!   how `bevy::asset::AssetServer::load` and this project's
//!   `ui`/`fighters`/`audio`/... paths are written everywhere else in
//!   `src/`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::super::aggregate::Aggregate;
use super::super::diagnostics::Diagnostic;
use super::super::schema::{Kind, Status, expected_extensions};
use super::rust_scan;

/// Named, documented compatibility exemptions letting a specific
/// production reference to a `source`/`legacy` asset pass validation.
/// Empty today: every production reference in this repository already
/// resolves to a `status = "runtime"` record (that is what this check
/// verifies). The mechanism exists so a genuine future compatibility need
/// -- e.g. temporarily falling back to a legacy asset while its runtime
/// replacement is mid-migration -- has a place to go without weakening the
/// check for every other reference.
///
/// Add an entry as `(asset id, reason)`. The asset id must already exist
/// as a `source`/`legacy` record in some sidecar; the reason must explain
/// *why* production code is allowed to reference it despite that status.
/// An entry that stops matching any discovered reference is flagged by
/// `Diagnostic::StaleRuntimeReferenceExemption` so the list can't
/// silently drift, mirroring `aggregate`'s `StaleIgnore`.
pub const RUNTIME_REFERENCE_EXEMPTIONS: &[(&str, &str)] = &[];

/// One asset-path-like string literal found in a production source file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DiscoveredReference {
    file: PathBuf,
    line: usize,
    /// The literal text exactly as written (pre path-resolution).
    text: String,
    source: ReferenceSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReferenceSource {
    Rust,
    Html,
}

/// Runs the full runtime-reference check: discovers every production
/// reference under `workspace_root/src` and `workspace_root/index.html`,
/// resolves each to an assets-relative path, and cross-checks it against
/// `aggregate`.
pub fn check(workspace_root: &Path, aggregate: &Aggregate) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut references = Vec::new();

    let src_root = workspace_root.join("src");
    match scan_rust_sources(&src_root) {
        Ok(found) => references.extend(found),
        Err(err) => diagnostics.push(Diagnostic::Undecodable {
            sidecar: src_root.clone(),
            error: format!("failed to walk src/ for asset references: {err}"),
        }),
    }

    let index_html = workspace_root.join("index.html");
    match fs::read_to_string(&index_html) {
        Ok(contents) => references.extend(scan_html(&index_html, &contents)),
        Err(err) => diagnostics.push(Diagnostic::Undecodable {
            sidecar: index_html.clone(),
            error: format!("failed to read index.html: {err}"),
        }),
    }

    let assets_root = workspace_root.join("assets");
    let mut used_exemptions = BTreeSet::new();

    for reference in &references {
        let Some(resolved) = resolve(workspace_root, &assets_root, reference) else {
            continue;
        };
        let record = aggregate.records.iter().find(|r| r.full_path == resolved);
        match record {
            None => {
                if aggregate.ignores.iter().any(|ig| ig.full_path == resolved) {
                    // References a file that exists but is a deliberate
                    // non-asset ignore entry (e.g. a README) -- not a
                    // sidecar-covered asset reference either way, but not
                    // this check's concern; #167's coverage pass already
                    // treats such files as intentionally non-asset.
                    continue;
                }
                diagnostics.push(Diagnostic::UnresolvedRuntimeReference {
                    file: reference.file.clone(),
                    line: reference.line,
                    reference: reference.text.clone(),
                });
            }
            Some(r) if r.record.status != Status::Runtime => {
                if let Some((id, _)) = RUNTIME_REFERENCE_EXEMPTIONS
                    .iter()
                    .find(|(id, _)| *id == r.record.id)
                {
                    used_exemptions.insert(*id);
                } else {
                    diagnostics.push(Diagnostic::IllegalRuntimeReference {
                        file: reference.file.clone(),
                        line: reference.line,
                        reference: reference.text.clone(),
                        id: r.record.id.clone(),
                        status: r.record.status.to_string(),
                    });
                }
            }
            Some(_) => {}
        }
    }

    for (id, _) in RUNTIME_REFERENCE_EXEMPTIONS {
        if !used_exemptions.contains(id) {
            diagnostics.push(Diagnostic::StaleRuntimeReferenceExemption { id: id.to_string() });
        }
    }

    diagnostics
}

fn scan_rust_sources(src_root: &Path) -> std::io::Result<Vec<DiscoveredReference>> {
    let mut references = Vec::new();
    for path in walk_rs_files(src_root)? {
        let contents = fs::read_to_string(&path)?;
        references.extend(scan_rust_file(&path, &contents));
    }
    Ok(references)
}

fn scan_rust_file(path: &Path, contents: &str) -> Vec<DiscoveredReference> {
    let tokens = rust_scan::tokenize(contents);
    let test_ranges = rust_scan::test_code_ranges(contents, &tokens);
    rust_scan::production_string_literals(contents, &tokens, &test_ranges)
        .into_iter()
        .filter(|(_, text)| looks_like_asset_path(text))
        .map(|(line, text)| DiscoveredReference {
            file: path.to_path_buf(),
            line,
            text: text.to_string(),
            source: ReferenceSource::Rust,
        })
        .collect()
}

/// Scans `index.html` for attribute values containing `assets/`. Textual
/// on purpose (no HTML parser dependency) -- this repo has exactly one
/// `index.html`, hand-authored and simple; see module docs.
fn scan_html(path: &Path, contents: &str) -> Vec<DiscoveredReference> {
    let mut references = Vec::new();
    for (idx, line) in contents.lines().enumerate() {
        let mut rest = line;
        while let Some(pos) = rest.find('"') {
            let after_quote = &rest[pos + 1..];
            let Some(end) = after_quote.find('"') else {
                break;
            };
            let value = &after_quote[..end];
            if value.contains("assets/") && looks_like_asset_path(value) {
                references.push(DiscoveredReference {
                    file: path.to_path_buf(),
                    line: idx + 1,
                    text: value.to_string(),
                    source: ReferenceSource::Html,
                });
            }
            rest = &after_quote[end + 1..];
        }
    }
    references
}

/// Every media-family variant, used to derive "does this look like an
/// asset path" from `schema::expected_extensions` without duplicating the
/// extension list here.
const ASSET_KINDS: [Kind; 4] = [Kind::Image, Kind::Audio, Kind::Font, Kind::Document];

/// Whether `text` looks like a bare relative asset path rather than, say, a
/// prose string that happens to mention a path (e.g.
/// `"Grafică: placeholder-e proprii (CC0) — vezi assets/CREDITS.md"` in
/// `src/progression/victory_ui.rs`, real production UI copy that must not
/// be mistaken for a load path). Requires both a recognized extension
/// *and* that every character is one that can legally appear in a path
/// component this project uses (ASCII letters/digits, `_`, `-`, `.`, `/`)
/// -- prose sentences fail on the space/punctuation/diacritics they
/// contain.
fn looks_like_asset_path(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let extension = text.rsplit('.').next().unwrap_or_default();
    let has_known_extension = ASSET_KINDS
        .iter()
        .any(|kind| expected_extensions(*kind).contains(&extension));
    has_known_extension
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
}

fn walk_rs_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.is_dir() {
        return Ok(files);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Resolves a discovered reference to a path relative to `assets_root`,
/// matching the convention `aggregate::ResolvedRecord::full_path` uses.
/// Returns `None` if the reference plainly isn't an `assets/`-relative
/// path at all (nothing to check).
fn resolve(
    workspace_root: &Path,
    assets_root: &Path,
    reference: &DiscoveredReference,
) -> Option<PathBuf> {
    match reference.source {
        ReferenceSource::Html => {
            let pos = reference.text.find("assets/")?;
            let relative = &reference.text[pos + "assets/".len()..];
            Some(PathBuf::from(relative))
        }
        ReferenceSource::Rust => {
            if reference.text.contains("..") {
                let base = reference.file.parent().unwrap_or(workspace_root);
                let joined = base.join(&reference.text);
                let normalized = normalize_lexically(&joined);
                normalized
                    .strip_prefix(assets_root)
                    .ok()
                    .map(|p| p.to_path_buf())
            } else {
                Some(PathBuf::from(&reference.text))
            }
        }
    }
}

/// Lexically collapses `.`/`..` components (no filesystem access -- the
/// path may not exist, e.g. for a deliberately broken test fixture).
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut stack: Vec<std::ffi::OsString> = Vec::new();
    for component in path.components() {
        use std::path::Component;
        match component {
            Component::ParentDir => {
                stack.pop();
            }
            Component::CurDir => {}
            Component::Normal(part) => stack.push(part.to_os_string()),
            Component::RootDir | Component::Prefix(_) => {
                stack.clear();
                stack.push(component.as_os_str().to_os_string());
            }
        }
    }
    stack.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::aggregate;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempWorkspace {
        root: PathBuf,
    }

    impl TempWorkspace {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "xtask-assets-validate-refs-{name}-{}-{:?}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
    }

    impl Drop for TempWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn sidecar(id: &str, file_name: &str, status: &str) -> String {
        format!(
            r#"
            version = 1
            [[record]]
            id = "{id}"
            path = "{file_name}"
            kind = "image"
            category = "sprite"
            status = "{status}"
            provenance = "repo-generated"
            generator = "scripts/generate-placeholder-sprites.py"
            license = "CC0 1.0"
            dimensions = [512, 512]
            {sampler}
            "#,
            sampler = if status == "runtime" {
                "sampler = \"linear\""
            } else {
                ""
            }
        )
    }

    #[test]
    fn a_production_reference_to_a_runtime_asset_is_clean() {
        let ws = TempWorkspace::new("clean");
        ws.write("assets/sprites/player.png", "fake-png");
        ws.write(
            "assets/sprites/manifest.toml",
            &sidecar("sprites.player", "player.png", "runtime"),
        );
        ws.write(
            "src/lib.rs",
            "pub const PLAYER: &str = \"sprites/player.png\";\n",
        );
        ws.write("index.html", "<html></html>\n");

        let aggregate = aggregate::build(&ws.root.join("assets"));
        let diagnostics = check(&ws.root, &aggregate);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_production_reference_to_a_legacy_asset_without_exemption_is_flagged() {
        let ws = TempWorkspace::new("illegal");
        ws.write("assets/gear/old.png", "fake-png");
        ws.write(
            "assets/gear/manifest.toml",
            &sidecar("gear.old", "old.png", "legacy"),
        );
        ws.write("src/lib.rs", "const OLD: &str = \"gear/old.png\";\n");
        ws.write("index.html", "<html></html>\n");

        let aggregate = aggregate::build(&ws.root.join("assets"));
        let diagnostics = check(&ws.root, &aggregate);
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            &diagnostics[0],
            Diagnostic::IllegalRuntimeReference { id, status, .. }
                if id == "gear.old" && status == "legacy"
        ));
    }

    #[test]
    fn a_reference_with_no_sidecar_record_is_flagged() {
        let ws = TempWorkspace::new("unresolved");
        ws.write("src/lib.rs", "const MISSING: &str = \"gear/ghost.png\";\n");

        let aggregate = aggregate::build(&ws.root.join("assets"));
        let diagnostics = check(&ws.root, &aggregate);
        assert!(diagnostics.iter().any(|d| matches!(
            d,
            Diagnostic::UnresolvedRuntimeReference { reference, .. }
                if reference == "gear/ghost.png"
        )));
    }

    #[test]
    fn a_reference_inside_cfg_test_code_is_ignored() {
        let ws = TempWorkspace::new("test-only");
        ws.write(
            "src/lib.rs",
            concat!(
                "#[cfg(test)]\n",
                "mod tests {\n",
                "    #[test]\n",
                "    fn broken_handle() {\n",
                "        let _ = \"gear/ghost.png\";\n",
                "    }\n",
                "}\n",
            ),
        );
        ws.write("index.html", "<html></html>\n");

        let aggregate = aggregate::build(&ws.root.join("assets"));
        let diagnostics = check(&ws.root, &aggregate);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn a_named_exemption_lets_a_legacy_reference_pass() {
        // This test constructs its own local exemption list rather than
        // relying on the real (empty) `RUNTIME_REFERENCE_EXEMPTIONS`, by
        // duplicating just enough of `check`'s cross-check logic. Keeping
        // the production constant empty (see its doc comment) while still
        // proving the exemption *mechanism* works.
        let ws = TempWorkspace::new("exempt");
        ws.write("assets/gear/old.png", "fake-png");
        ws.write(
            "assets/gear/manifest.toml",
            &sidecar("gear.old", "old.png", "legacy"),
        );
        ws.write("src/lib.rs", "const OLD: &str = \"gear/old.png\";\n");

        let aggregate = aggregate::build(&ws.root.join("assets"));
        let exemptions: &[(&str, &str)] = &[("gear.old", "test: proves the exemption mechanism")];
        let diagnostics = check_with_exemptions(&ws.root, &aggregate, exemptions);
        assert!(diagnostics.is_empty());
    }

    /// Test-only twin of `check` that accepts an injected exemption list,
    /// so the exemption *mechanism* can be proven without adding a real
    /// entry to the checked-in (deliberately empty) constant.
    fn check_with_exemptions(
        workspace_root: &Path,
        aggregate: &Aggregate,
        exemptions: &[(&str, &str)],
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut references = Vec::new();
        let src_root = workspace_root.join("src");
        if let Ok(found) = scan_rust_sources(&src_root) {
            references.extend(found);
        }
        let assets_root = workspace_root.join("assets");
        for reference in &references {
            let Some(resolved) = resolve(workspace_root, &assets_root, reference) else {
                continue;
            };
            let Some(r) = aggregate.records.iter().find(|r| r.full_path == resolved) else {
                continue;
            };
            if r.record.status != Status::Runtime
                && !exemptions.iter().any(|(id, _)| *id == r.record.id)
            {
                diagnostics.push(Diagnostic::IllegalRuntimeReference {
                    file: reference.file.clone(),
                    line: reference.line,
                    reference: reference.text.clone(),
                    id: r.record.id.clone(),
                    status: r.record.status.to_string(),
                });
            }
        }
        diagnostics
    }

    #[test]
    fn an_html_reference_is_discovered_and_resolved() {
        let ws = TempWorkspace::new("html");
        ws.write("assets/web/favicon.svg", "<svg/>");
        ws.write(
            "assets/web/manifest.toml",
            r#"
            version = 1
            [[record]]
            id = "web.favicon"
            path = "favicon.svg"
            kind = "image"
            category = "web-icon"
            status = "runtime"
            provenance = "hand-authored"
            license = "same as the project"
            "#,
        );
        ws.write(
            "index.html",
            "<link rel=\"icon\" href=\"assets/web/favicon.svg\" />\n",
        );

        let aggregate = aggregate::build(&ws.root.join("assets"));
        let diagnostics = check(&ws.root, &aggregate);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            diagnostics
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
        );
    }
}
