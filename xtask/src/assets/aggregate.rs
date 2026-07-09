//! Loads every sidecar under `assets/`, derives one in-memory aggregate, and
//! reports coverage/violations. This is the single authoritative pass
//! `cargo xtask assets check` runs -- there is no separate hand-maintained
//! aggregate file (seeing #167's "must not" list).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::diagnostics::Diagnostic;
use super::discover::{self, CaseCheck, FoundSidecar};
use super::schema::{Kind, Record, SCHEMA_VERSION, Sidecar, Status, expected_extensions};

/// One record after path resolution: the sidecar it came from, and its
/// full path relative to `assets/`.
#[derive(Debug, Clone)]
pub struct ResolvedRecord {
    pub sidecar: PathBuf,
    pub full_path: PathBuf,
    pub record: Record,
}

/// One ignore entry after path resolution.
#[derive(Debug, Clone)]
pub struct ResolvedIgnore {
    pub sidecar: PathBuf,
    pub full_path: PathBuf,
    pub reason: String,
}

/// The derived, in-memory aggregate plus every diagnostic found while
/// building it. A non-empty `diagnostics` means `cargo xtask assets check`
/// must fail, but the aggregate itself is still populated as far as
/// possible so a single run surfaces every problem, not just the first.
#[derive(Debug, Default)]
pub struct Aggregate {
    pub records: Vec<ResolvedRecord>,
    pub ignores: Vec<ResolvedIgnore>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Builds the aggregate by walking `assets_root`, parsing every sidecar,
/// resolving paths, and running every cross-cutting and per-record check.
pub fn build(assets_root: &Path) -> Aggregate {
    let mut aggregate = Aggregate::default();

    let (found_sidecars, all_files) = match discover::walk_assets(assets_root) {
        Ok(pair) => pair,
        Err(err) => {
            aggregate.diagnostics.push(Diagnostic::Undecodable {
                sidecar: assets_root.to_path_buf(),
                error: format!("failed to walk assets directory: {err}"),
            });
            return aggregate;
        }
    };

    for found in &found_sidecars {
        load_sidecar(assets_root, found, &mut aggregate);
    }

    let mut duplicate_diags = Vec::new();
    check_duplicate_ids(&aggregate.records, &mut duplicate_diags);
    check_duplicate_paths(&aggregate.records, &aggregate.ignores, &mut duplicate_diags);
    aggregate.diagnostics.extend(duplicate_diags);

    let coverage_diags = coverage_diagnostics(&all_files, &aggregate);
    aggregate.diagnostics.extend(coverage_diags);

    aggregate
}

fn load_sidecar(assets_root: &Path, found: &FoundSidecar, aggregate: &mut Aggregate) {
    let sidecar: Sidecar = match toml::from_str(&found.contents) {
        Ok(sidecar) => sidecar,
        Err(err) => {
            aggregate.diagnostics.push(Diagnostic::Undecodable {
                sidecar: found.sidecar_path.clone(),
                error: err.to_string(),
            });
            return;
        }
    };

    if sidecar.version != SCHEMA_VERSION {
        aggregate.diagnostics.push(Diagnostic::UnsupportedVersion {
            sidecar: found.sidecar_path.clone(),
            found: sidecar.version,
        });
        return;
    }

    for ignore in sidecar.ignores {
        if ignore.path.contains('/') || ignore.path.contains('\\') {
            aggregate
                .diagnostics
                .push(Diagnostic::PathEscapesSidecarDirectory {
                    sidecar: found.sidecar_path.clone(),
                    id: format!("<ignore {}>", ignore.path),
                    path: ignore.path.clone(),
                });
            continue;
        }
        let full_path = found.dir.join(&ignore.path);
        match discover::case_correct(assets_root, &full_path) {
            CaseCheck::Match => {}
            CaseCheck::CaseMismatch => {
                aggregate.diagnostics.push(Diagnostic::CaseMismatch {
                    sidecar: found.sidecar_path.clone(),
                    id: format!("<ignore {}>", ignore.path),
                    path: full_path.clone(),
                });
            }
            CaseCheck::Missing => {
                aggregate.diagnostics.push(Diagnostic::StaleIgnore {
                    sidecar: found.sidecar_path.clone(),
                    path: full_path.clone(),
                });
            }
        }
        aggregate.ignores.push(ResolvedIgnore {
            sidecar: found.sidecar_path.clone(),
            full_path,
            reason: ignore.reason,
        });
    }

    for record in sidecar.records {
        if record.path.contains('/') || record.path.contains('\\') {
            aggregate
                .diagnostics
                .push(Diagnostic::PathEscapesSidecarDirectory {
                    sidecar: found.sidecar_path.clone(),
                    id: record.id.clone(),
                    path: record.path.clone(),
                });
            continue;
        }
        let full_path = found.dir.join(&record.path);
        validate_record(
            assets_root,
            &found.sidecar_path,
            &full_path,
            &record,
            aggregate,
        );
        aggregate.records.push(ResolvedRecord {
            sidecar: found.sidecar_path.clone(),
            full_path,
            record,
        });
    }
}

fn validate_record(
    assets_root: &Path,
    sidecar_path: &Path,
    full_path: &Path,
    record: &Record,
    aggregate: &mut Aggregate,
) {
    match discover::case_correct(assets_root, full_path) {
        CaseCheck::Match => {}
        CaseCheck::CaseMismatch => aggregate.diagnostics.push(Diagnostic::CaseMismatch {
            sidecar: sidecar_path.to_path_buf(),
            id: record.id.clone(),
            path: full_path.to_path_buf(),
        }),
        CaseCheck::Missing => aggregate.diagnostics.push(Diagnostic::MissingFile {
            sidecar: sidecar_path.to_path_buf(),
            id: record.id.clone(),
            path: full_path.to_path_buf(),
        }),
    }

    if record.license.trim().is_empty() {
        aggregate.diagnostics.push(Diagnostic::MissingLicense {
            sidecar: sidecar_path.to_path_buf(),
            id: record.id.clone(),
        });
    }

    // Wrongly classified: category's media family must match the declared kind.
    if record.category.expected_kind() != record.kind {
        aggregate.diagnostics.push(Diagnostic::WrongClassification {
            sidecar: sidecar_path.to_path_buf(),
            id: record.id.clone(),
            field: "category",
            detail: format!(
                "category `{}` implies kind `{}`, but record declares kind `{}`",
                record.category,
                record.category.expected_kind(),
                record.kind
            ),
        });
    }

    // Wrongly classified: the file extension must match the declared kind.
    let extension = full_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    let expected = expected_extensions(record.kind);
    if let Some(extension) = &extension
        && !expected.contains(&extension.as_str())
    {
        aggregate.diagnostics.push(Diagnostic::WrongClassification {
            sidecar: sidecar_path.to_path_buf(),
            id: record.id.clone(),
            field: "kind",
            detail: format!(
                "file extension `.{extension}` does not match declared kind `{}` (expected one of {expected:?})",
                record.kind
            ),
        });
    }

    // Required-field-by-kind/category/status checks.
    let is_svg = extension.as_deref() == Some("svg");
    if record.kind == Kind::Image && !is_svg && record.dimensions.is_none() {
        aggregate
            .diagnostics
            .push(Diagnostic::MissingRequiredField {
                sidecar: sidecar_path.to_path_buf(),
                id: record.id.clone(),
                field: "dimensions",
            });
    }
    if record.kind != Kind::Image && record.dimensions.is_some() {
        aggregate.diagnostics.push(Diagnostic::WrongClassification {
            sidecar: sidecar_path.to_path_buf(),
            id: record.id.clone(),
            field: "dimensions",
            detail: format!(
                "`dimensions` set on a non-image record (kind `{}`)",
                record.kind
            ),
        });
    }

    if record.kind == Kind::Image
        && record.status == Status::Runtime
        && !record.category.is_web()
        && record.sampler.is_none()
    {
        aggregate
            .diagnostics
            .push(Diagnostic::MissingRequiredField {
                sidecar: sidecar_path.to_path_buf(),
                id: record.id.clone(),
                field: "sampler",
            });
    }

    if record.status == Status::Runtime && record.category.is_rig_attachment() {
        if record.attachment.is_none() {
            aggregate
                .diagnostics
                .push(Diagnostic::MissingRequiredField {
                    sidecar: sidecar_path.to_path_buf(),
                    id: record.id.clone(),
                    field: "attachment",
                });
        }
        if record.pivot.is_none() {
            aggregate
                .diagnostics
                .push(Diagnostic::MissingRequiredField {
                    sidecar: sidecar_path.to_path_buf(),
                    id: record.id.clone(),
                    field: "pivot",
                });
        }
        if record.display.is_none() {
            aggregate
                .diagnostics
                .push(Diagnostic::MissingRequiredField {
                    sidecar: sidecar_path.to_path_buf(),
                    id: record.id.clone(),
                    field: "display",
                });
        }
        match &record.crop {
            None => aggregate
                .diagnostics
                .push(Diagnostic::MissingRequiredField {
                    sidecar: sidecar_path.to_path_buf(),
                    id: record.id.clone(),
                    field: "crop",
                }),
            Some(crop) if crop != "unknown" && !is_crop_rect_shaped(crop) => {
                aggregate.diagnostics.push(Diagnostic::WrongClassification {
                    sidecar: sidecar_path.to_path_buf(),
                    id: record.id.clone(),
                    field: "crop",
                    detail: format!(
                        "`crop` = {crop:?} is neither the literal \"unknown\" nor an \"x,y,w,h\" rectangle"
                    ),
                });
            }
            _ => {}
        }
    }

    // `provenance` is always required, and two provenance values imply a
    // companion field that documents *how* the derivation happened.
    if record.provenance.trim().is_empty() {
        aggregate
            .diagnostics
            .push(Diagnostic::MissingRequiredField {
                sidecar: sidecar_path.to_path_buf(),
                id: record.id.clone(),
                field: "provenance",
            });
    }
    if record.provenance == "repo-generated" && record.generator.is_none() {
        aggregate
            .diagnostics
            .push(Diagnostic::MissingRequiredField {
                sidecar: sidecar_path.to_path_buf(),
                id: record.id.clone(),
                field: "generator",
            });
    }
    if record.provenance == "cropped-from-source-sheet" && record.source_sheet.is_none() {
        aggregate
            .diagnostics
            .push(Diagnostic::MissingRequiredField {
                sidecar: sidecar_path.to_path_buf(),
                id: record.id.clone(),
                field: "source_sheet",
            });
    }
    if record.category == super::schema::Category::Font && record.license_file.is_none() {
        aggregate
            .diagnostics
            .push(Diagnostic::MissingRequiredField {
                sidecar: sidecar_path.to_path_buf(),
                id: record.id.clone(),
                field: "license_file",
            });
    }
}

/// Loosely validates an `"x,y,w,h"` crop-rectangle string: four
/// comma-separated non-negative numbers, nothing more.
fn is_crop_rect_shaped(value: &str) -> bool {
    let parts: Vec<&str> = value.split(',').collect();
    parts.len() == 4 && parts.iter().all(|p| p.trim().parse::<f32>().is_ok())
}

fn check_duplicate_ids(records: &[ResolvedRecord], diagnostics: &mut Vec<Diagnostic>) {
    let mut seen: BTreeMap<&str, &Path> = BTreeMap::new();
    for record in records {
        let id = record.record.id.as_str();
        if let Some(first) = seen.get(id) {
            diagnostics.push(Diagnostic::DuplicateId {
                id: id.to_string(),
                first: first.to_path_buf(),
                second: record.sidecar.clone(),
            });
        } else {
            seen.insert(id, &record.sidecar);
        }
    }
}

fn check_duplicate_paths(
    records: &[ResolvedRecord],
    ignores: &[ResolvedIgnore],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: BTreeMap<&Path, &Path> = BTreeMap::new();
    for record in records {
        let path = record.full_path.as_path();
        if let Some(first) = seen.get(path) {
            diagnostics.push(Diagnostic::DuplicatePath {
                path: path.to_path_buf(),
                first: first.to_path_buf(),
                second: record.sidecar.clone(),
            });
        } else {
            seen.insert(path, &record.sidecar);
        }
    }
    for ignore in ignores {
        let path = ignore.full_path.as_path();
        if let Some(first) = seen.get(path) {
            diagnostics.push(Diagnostic::DuplicatePath {
                path: path.to_path_buf(),
                first: first.to_path_buf(),
                second: ignore.sidecar.clone(),
            });
        } else {
            seen.insert(path, &ignore.sidecar);
        }
    }
}

fn coverage_diagnostics(all_files: &[PathBuf], aggregate: &Aggregate) -> Vec<Diagnostic> {
    let mut covered: BTreeMap<&Path, ()> = BTreeMap::new();
    for record in &aggregate.records {
        covered.insert(record.full_path.as_path(), ());
    }
    for ignore in &aggregate.ignores {
        covered.insert(ignore.full_path.as_path(), ());
    }

    all_files
        .iter()
        .filter(|file| !covered.contains_key(file.as_path()))
        .map(|file| Diagnostic::UncoveredFile { path: file.clone() })
        .collect()
}

/// Coverage totals for the success-path report: how many files are
/// accounted for by record vs. ignore, broken down by sidecar/category/status.
pub struct CoverageSummary {
    pub total_files: usize,
    pub total_records: usize,
    pub total_ignored: usize,
    pub by_status: BTreeMap<String, usize>,
    pub by_category: BTreeMap<String, usize>,
    pub by_sidecar: BTreeMap<PathBuf, usize>,
    pub by_provenance: BTreeMap<String, usize>,
}

pub fn summarize(assets_root: &Path, aggregate: &Aggregate) -> CoverageSummary {
    let mut by_status = BTreeMap::new();
    let mut by_category = BTreeMap::new();
    let mut by_sidecar = BTreeMap::new();
    let mut by_provenance = BTreeMap::new();

    for record in &aggregate.records {
        *by_status
            .entry(record.record.status.to_string())
            .or_insert(0) += 1;
        *by_category
            .entry(record.record.category.to_string())
            .or_insert(0) += 1;
        *by_sidecar.entry(record.sidecar.clone()).or_insert(0) += 1;
        *by_provenance
            .entry(record.record.provenance.clone())
            .or_insert(0) += 1;
    }
    for ignore in &aggregate.ignores {
        *by_sidecar.entry(ignore.sidecar.clone()).or_insert(0) += 1;
    }

    let total_files = discover::walk_assets(assets_root)
        .map(|(_, files)| files.len())
        .unwrap_or(0);

    CoverageSummary {
        total_files,
        total_records: aggregate.records.len(),
        total_ignored: aggregate.ignores.len(),
        by_status,
        by_category,
        by_sidecar,
        by_provenance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempAssets {
        root: PathBuf,
    }

    impl TempAssets {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "xtask-assets-aggregate-{name}-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
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

    impl Drop for TempAssets {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn minimal_sidecar(id: &str, file_name: &str) -> String {
        format!(
            r#"
            version = 1
            [[record]]
            id = "{id}"
            path = "{file_name}"
            kind = "image"
            category = "sprite"
            status = "runtime"
            provenance = "repo-generated"
            generator = "scripts/generate-placeholder-sprites.py"
            license = "CC0 1.0"
            dimensions = [512, 512]
            sampler = "linear"
            "#
        )
    }

    #[test]
    fn a_clean_tree_has_no_diagnostics_and_full_coverage() {
        let assets = TempAssets::new("clean");
        assets.write("sprites/player.png", "fake-png");
        assets.write(
            "sprites/manifest.toml",
            &minimal_sidecar("sprites.player", "player.png"),
        );

        let aggregate = build(&assets.root);
        assert!(
            aggregate.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            aggregate
                .diagnostics
                .iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(aggregate.records.len(), 1);

        let summary = summarize(&assets.root, &aggregate);
        assert_eq!(summary.total_files, 1);
        assert_eq!(summary.total_records, 1);
    }

    #[test]
    fn an_uncovered_file_is_flagged() {
        let assets = TempAssets::new("uncovered");
        assets.write("sprites/player.png", "fake-png");
        assets.write("sprites/orphan.png", "fake-png");
        assets.write(
            "sprites/manifest.toml",
            &minimal_sidecar("sprites.player", "player.png"),
        );

        let aggregate = build(&assets.root);
        assert!(aggregate.diagnostics.iter().any(|d| matches!(
            d,
            Diagnostic::UncoveredFile { path } if path == Path::new("sprites/orphan.png")
        )));
    }

    #[test]
    fn a_missing_file_is_flagged_with_sidecar_and_id() {
        let assets = TempAssets::new("missing");
        assets.write(
            "sprites/manifest.toml",
            &minimal_sidecar("sprites.player", "player.png"),
        );
        // Note: player.png is never written.

        let aggregate = build(&assets.root);
        let found = aggregate.diagnostics.iter().find_map(|d| match d {
            Diagnostic::MissingFile { sidecar, id, path } => Some((sidecar, id, path)),
            _ => None,
        });
        let (sidecar, id, path) = found.expect("missing-file diagnostic present");
        assert!(sidecar.ends_with("sprites/manifest.toml"));
        assert_eq!(id, "sprites.player");
        assert_eq!(path, Path::new("sprites/player.png"));
    }

    #[test]
    fn a_case_mismatched_path_is_flagged() {
        let assets = TempAssets::new("case");
        assets.write("sprites/Player.png", "fake-png");
        assets.write(
            "sprites/manifest.toml",
            &minimal_sidecar("sprites.player", "player.png"),
        );

        let aggregate = build(&assets.root);
        assert!(
            aggregate.diagnostics.iter().any(
                |d| matches!(d, Diagnostic::CaseMismatch { id, .. } if id == "sprites.player")
            )
        );
    }

    #[test]
    fn an_undecodable_sidecar_is_flagged_without_panicking() {
        let assets = TempAssets::new("undecodable");
        assets.write("sprites/manifest.toml", "this is not valid toml {{{");

        let aggregate = build(&assets.root);
        assert!(
            aggregate
                .diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::Undecodable { .. }))
        );
    }

    #[test]
    fn a_missing_license_is_flagged() {
        let assets = TempAssets::new("unlicensed");
        assets.write("sprites/player.png", "fake-png");
        assets.write(
            "sprites/manifest.toml",
            r#"
            version = 1
            [[record]]
            id = "sprites.player"
            path = "player.png"
            kind = "image"
            category = "sprite"
            status = "runtime"
            provenance = "repo-generated"
            license = ""
            dimensions = [512, 512]
            sampler = "linear"
            "#,
        );

        let aggregate = build(&assets.root);
        assert!(
            aggregate.diagnostics.iter().any(
                |d| matches!(d, Diagnostic::MissingLicense { id, .. } if id == "sprites.player")
            )
        );
    }

    #[test]
    fn a_duplicate_id_across_sidecars_is_flagged() {
        let assets = TempAssets::new("dup-id");
        assets.write("sprites/player.png", "fake-png");
        assets.write("gear/thing.png", "fake-png");
        assets.write(
            "sprites/manifest.toml",
            &minimal_sidecar("shared.id", "player.png"),
        );
        assets.write(
            "gear/manifest.toml",
            &minimal_sidecar("shared.id", "thing.png"),
        );

        let aggregate = build(&assets.root);
        assert!(
            aggregate
                .diagnostics
                .iter()
                .any(|d| matches!(d, Diagnostic::DuplicateId { id, .. } if id == "shared.id"))
        );
    }

    #[test]
    fn a_duplicate_path_across_a_record_and_an_ignore_is_flagged() {
        let assets = TempAssets::new("dup-path");
        assets.write("sprites/player.png", "fake-png");
        assets.write(
            "sprites/manifest.toml",
            &format!(
                "{}\n[[ignore]]\npath = \"player.png\"\nreason = \"also ignored, oops\"\n",
                minimal_sidecar("sprites.player", "player.png")
            ),
        );

        let aggregate = build(&assets.root);
        assert!(aggregate.diagnostics.iter().any(|d| matches!(
            d,
            Diagnostic::DuplicatePath { path, .. } if path == Path::new("sprites/player.png")
        )));
    }

    #[test]
    fn an_ignore_entry_with_a_documented_reason_covers_its_file() {
        let assets = TempAssets::new("ignore-reason");
        assets.write("sprites/README.md", "docs");
        assets.write(
            "sprites/manifest.toml",
            r#"
            version = 1
            [[ignore]]
            path = "README.md"
            reason = "directory documentation, not an asset"
            "#,
        );

        let aggregate = build(&assets.root);
        assert!(aggregate.diagnostics.is_empty());
        assert_eq!(aggregate.ignores.len(), 1);
        assert_eq!(
            aggregate.ignores[0].reason,
            "directory documentation, not an asset"
        );
    }

    #[test]
    fn wrongly_classified_kind_vs_extension_is_flagged() {
        let assets = TempAssets::new("wrong-kind");
        assets.write("audio/music_menu.ogg", "fake-audio");
        assets.write(
            "audio/manifest.toml",
            r#"
            version = 1
            [[record]]
            id = "audio.music-menu"
            path = "music_menu.ogg"
            kind = "image"
            category = "sprite"
            status = "runtime"
            provenance = "repo-generated"
            license = "CC0 1.0"
            "#,
        );

        let aggregate = build(&assets.root);
        assert!(aggregate.diagnostics.iter().any(
            |d| matches!(d, Diagnostic::WrongClassification { field, .. } if *field == "kind")
        ));
    }

    #[test]
    fn missing_required_pivot_for_a_runtime_gear_part_is_flagged() {
        let assets = TempAssets::new("missing-pivot");
        assets.write("fighters/gear/runtime/palos.png", "fake-png");
        assets.write(
            "fighters/gear/runtime/manifest.toml",
            r#"
            version = 1
            [[record]]
            id = "fighters.gear.runtime.palos"
            path = "palos.png"
            kind = "image"
            category = "gear-runtime-part"
            status = "runtime"
            provenance = "cropped-from-source-sheet"
            license = "Same as project assets unless superseded"
            dimensions = [66, 222]
            sampler = "linear"
            "#,
        );

        let aggregate = build(&assets.root);
        let missing_fields: Vec<_> = aggregate
            .diagnostics
            .iter()
            .filter_map(|d| match d {
                Diagnostic::MissingRequiredField { field, .. } => Some(*field),
                _ => None,
            })
            .collect();
        assert!(missing_fields.contains(&"attachment"));
        assert!(missing_fields.contains(&"pivot"));
        assert!(missing_fields.contains(&"display"));
    }
}
