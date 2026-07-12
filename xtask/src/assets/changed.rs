//! `cargo xtask assets review --changed` (#211, a child of #141, blocked by
//! #197): safe git-base selection, changed-file -> sidecar-record mapping,
//! and the transitive dependency closure that decides which gallery pages a
//! focused review must include.
//!
//! This module is pure logic plus a thin `std::process::Command` wrapper
//! around `git` (never `xtask/src/web_smoke/`, never a root process
//! helper -- see `xtask/README.md`'s ownership boundaries). It never
//! hand-edits or reads a central aggregate manifest: every mapping/closure
//! decision below reads only the same in-memory aggregate `assets check`/
//! `assets review` already build (`super::aggregate::build`).
//!
//! # Base resolution (see [`resolve_base`])
//!
//! Precedence, first match wins:
//! 1. An explicit `--base <ref>` flag.
//! 2. `GITHUB_BASE_REF` (set by GitHub Actions on `pull_request` events),
//!    tried both as given and as `origin/<ref>` (a plain checkout of a PR
//!    branch typically only has the remote-tracking form).
//! 3. `git merge-base HEAD origin/main`, when it resolves to exactly one
//!    commit.
//!
//! A missing/invalid base is always an actionable [`BaseError`] -- this
//! module never silently falls back to reviewing every asset (#211's
//! explicit "must not" rule).
//!
//! # Changed-file mapping (see [`map_changed_files`])
//!
//! `git diff --name-status -M <base>...HEAD -- assets` is parsed into
//! [`ChangedFile`]s (added/modified/deleted/renamed), then mapped to sidecar
//! record ids: a changed content file maps to the one record whose resolved
//! path matches it; a changed `manifest.toml` sidecar conservatively maps to
//! *every* record it declares (the issue's documented conservative rule --
//! any field on any of its records could have changed). A deleted/
//! renamed-away path that no longer resolves to any current record is
//! surfaced as a [`RemovedAsset`] rather than silently dropped; its id is
//! recovered, best-effort, by reading the *comparison base*'s sidecar (see
//! [`resolve_removed_record_id`]).
//!
//! # Dependency closure (see [`page_closure`])
//!
//! Starting from the directly-changed record ids, a worklist expands to
//! every page a change must dirty, per the #197 handoff's rules:
//! - a fighter body part dirties its own page, its identity's composition
//!   (`composition.<identity>`), and -- only for the `human` identity, since
//!   every gear composition renders the full human rig -- all 13 gear
//!   compositions;
//! - a composable gear part/overlay dirties its own page and its one
//!   `composition.gear.<slug>` page;
//! - a background layer dirties its own page and its scene's composition
//!   (`composition.background.<scene>`); the one background used as "the
//!   representative background" ([`super::gallery::pages::REPRESENTATIVE_BACKGROUND_ID`])
//!   additionally dirties every fighter/gear part and composition page,
//!   since every one of those embeds it as a rendered dependency even
//!   though its own HTML bytes never mention that background's id;
//! - a source-sheet record dirties every record that names it as their own
//!   `source_sheet` (which then cascades through the rules above);
//! - everything else (UI, fonts, audio, generic/legacy assets) dirties only
//!   its own page;
//! - `index.html` (the sentinel id `"index"`) is always included.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::aggregate::ResolvedRecord;
use super::gallery::{model, pages};
use super::schema::{Category, Sidecar};

// ---------------------------------------------------------------------
// Base resolution
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseSource {
    ExplicitFlag,
    GithubBaseRef,
    MergeBaseWithOriginMain,
}

impl std::fmt::Display for BaseSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BaseSource::ExplicitFlag => "--base flag",
            BaseSource::GithubBaseRef => "GITHUB_BASE_REF",
            BaseSource::MergeBaseWithOriginMain => "merge-base with origin/main",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBase {
    /// The resolved base as a full commit sha (never a symbolic ref), so
    /// every downstream `git diff`/`git show` call is unambiguous.
    pub sha: String,
    pub source: BaseSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseError {
    /// `--base <ref>` was passed but does not resolve to a commit.
    InvalidExplicitBase { requested: String, detail: String },
    /// No `--base` flag and no usable `GITHUB_BASE_REF`/merge-base was
    /// found. `attempted` lists every candidate tried, for an actionable
    /// diagnostic.
    NoUsableBase { attempted: Vec<String> },
}

impl std::fmt::Display for BaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BaseError::InvalidExplicitBase { requested, detail } => write!(
                f,
                "`--base {requested}` does not resolve to a commit ({detail}). \
                 Pass a valid ref/sha, e.g. `--base origin/main` or `--base <sha>`."
            ),
            BaseError::NoUsableBase { attempted } => write!(
                f,
                "could not determine a comparison base for `cargo xtask assets review --changed`. \
                 Tried, in precedence order: {}. Pass an explicit base: \
                 `cargo xtask assets review --changed --base <ref>` (e.g. `origin/main` or a commit sha). \
                 This command never falls back to reviewing every asset when the base is unknown.",
                attempted.join("; ")
            ),
        }
    }
}

/// Resolves the comparison base with the precedence documented in this
/// module's docs: explicit flag > `GITHUB_BASE_REF` > merge-base with
/// `origin/main`. `explicit` is the parsed `--base <ref>` value (if any);
/// `github_base_ref` is normally `std::env::var("GITHUB_BASE_REF").ok()`,
/// threaded in explicitly so this function stays pure/testable.
pub fn resolve_base(
    explicit: Option<&str>,
    github_base_ref: Option<&str>,
    workspace_root: &Path,
) -> Result<ResolvedBase, BaseError> {
    if let Some(requested) = explicit {
        return match verify_ref(workspace_root, requested) {
            Ok(sha) => Ok(ResolvedBase {
                sha,
                source: BaseSource::ExplicitFlag,
            }),
            Err(detail) => Err(BaseError::InvalidExplicitBase {
                requested: requested.to_string(),
                detail,
            }),
        };
    }

    let mut attempted = Vec::new();

    if let Some(base_ref) = github_base_ref.map(str::trim).filter(|s| !s.is_empty()) {
        for candidate in [base_ref.to_string(), format!("origin/{base_ref}")] {
            match verify_ref(workspace_root, &candidate) {
                Ok(sha) => {
                    return Ok(ResolvedBase {
                        sha,
                        source: BaseSource::GithubBaseRef,
                    });
                }
                Err(detail) => attempted.push(format!(
                    "GITHUB_BASE_REF candidate `{candidate}` ({detail})"
                )),
            }
        }
    }

    match merge_base(workspace_root, "HEAD", "origin/main") {
        Ok(sha) => {
            return Ok(ResolvedBase {
                sha,
                source: BaseSource::MergeBaseWithOriginMain,
            });
        }
        Err(detail) => attempted.push(format!("merge-base(HEAD, origin/main) ({detail})")),
    }

    Err(BaseError::NoUsableBase { attempted })
}

fn run_git(workspace_root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace_root)
        .args(args)
        .output()
        .map_err(|e| format!("failed to spawn `git {}`: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "`git {}` exited with {}: {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn verify_ref(workspace_root: &Path, r: &str) -> Result<String, String> {
    run_git(
        workspace_root,
        &["rev-parse", "--verify", &format!("{r}^{{commit}}")],
    )
}

fn merge_base(workspace_root: &Path, a: &str, b: &str) -> Result<String, String> {
    run_git(workspace_root, &["merge-base", a, b])
}

// ---------------------------------------------------------------------
// Changed-file diff parsing
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    /// `from` is the old path; the [`ChangedFile::path`] field holds the new
    /// path.
    Renamed {
        from: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    /// Repo-root-relative path (forward slashes), e.g.
    /// `assets/fighters/human/runtime/head.png`. The *new* path for renames.
    pub path: String,
    pub status: ChangeStatus,
}

/// Runs `git diff --name-status -M <base>...HEAD -- assets` and parses the
/// result. Restricted server-side to the `assets` pathspec since only
/// `assets/**` changes are ever relevant to a focused review.
pub fn diff_changed_assets(workspace_root: &Path, base: &str) -> Result<Vec<ChangedFile>, String> {
    let stdout = run_git(
        workspace_root,
        &[
            "diff",
            "--name-status",
            "-M",
            &format!("{base}...HEAD"),
            "--",
            "assets",
        ],
    )?;
    Ok(parse_name_status(&stdout))
}

/// Parses raw `git diff --name-status [-M]` output. Exposed separately from
/// [`diff_changed_assets`] so the parsing logic is testable without a real
/// git repo.
pub fn parse_name_status(text: &str) -> Vec<ChangedFile> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        let Some(code) = fields.next() else { continue };
        match code.chars().next() {
            Some('A') => {
                if let Some(p) = fields.next() {
                    out.push(ChangedFile {
                        path: p.to_string(),
                        status: ChangeStatus::Added,
                    });
                }
            }
            Some('M') => {
                if let Some(p) = fields.next() {
                    out.push(ChangedFile {
                        path: p.to_string(),
                        status: ChangeStatus::Modified,
                    });
                }
            }
            Some('D') => {
                if let Some(p) = fields.next() {
                    out.push(ChangedFile {
                        path: p.to_string(),
                        status: ChangeStatus::Deleted,
                    });
                }
            }
            Some('R') => {
                let (Some(from), Some(to)) = (fields.next(), fields.next()) else {
                    continue;
                };
                out.push(ChangedFile {
                    path: to.to_string(),
                    status: ChangeStatus::Renamed {
                        from: from.to_string(),
                    },
                });
            }
            Some('C') => {
                // Copied: treat the new path like an addition; the source
                // path is untouched by this change, unlike a rename.
                let (Some(_from), Some(to)) = (fields.next(), fields.next()) else {
                    continue;
                };
                out.push(ChangedFile {
                    path: to.to_string(),
                    status: ChangeStatus::Added,
                });
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------
// Changed-file -> sidecar-record mapping
// ---------------------------------------------------------------------

/// A deleted or renamed-away asset with no current record to link a page to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedAsset {
    pub path: String,
    /// Best-effort: resolved from the comparison base's sidecar when
    /// possible (see [`resolve_removed_record_id`]); `None` when it can't be
    /// determined (e.g. the sidecar itself is gone, or was never valid).
    pub id: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ChangeMapping {
    pub direct_ids: BTreeSet<String>,
    pub removed: Vec<RemovedAsset>,
}

/// Maps `changed` files (already filtered to `assets/**`) to sidecar record
/// ids in the *current* (HEAD) aggregate. A changed `manifest.toml` maps
/// conservatively to every record declared by that exact sidecar file. A
/// deleted/renamed-away path that resolves to no current record is
/// collected into `removed` with `id: None`; call
/// [`resolve_removed_record_id`] per entry to fill in the id, best-effort,
/// from the comparison base (kept separate since that needs `git`, while
/// this function stays pure and easy to test).
pub fn map_changed_files(
    assets_root: &Path,
    records: &[&ResolvedRecord],
    changed: &[ChangedFile],
) -> ChangeMapping {
    let by_full_path: BTreeMap<PathBuf, &str> = records
        .iter()
        .map(|r| (r.full_path.clone(), r.record.id.as_str()))
        .collect();
    let mut ids_by_sidecar: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    for r in records {
        ids_by_sidecar
            .entry(r.sidecar.clone())
            .or_default()
            .push(r.record.id.clone());
    }

    let mut mapping = ChangeMapping::default();

    let handle = |path_str: &str, is_delete: bool, mapping: &mut ChangeMapping| {
        let Some(rel) = path_str.strip_prefix("assets/") else {
            return;
        };
        let rel_path = PathBuf::from(rel);
        if rel_path.file_name().and_then(|n| n.to_str()) == Some("manifest.toml") {
            let sidecar_abs = assets_root.join(&rel_path);
            if let Some(ids) = ids_by_sidecar.get(&sidecar_abs) {
                mapping.direct_ids.extend(ids.iter().cloned());
            }
            return;
        }
        if let Some(id) = by_full_path.get(&rel_path) {
            mapping.direct_ids.insert(id.to_string());
        } else if is_delete {
            mapping.removed.push(RemovedAsset {
                path: path_str.to_string(),
                id: None,
            });
        }
    };

    for cf in changed {
        match &cf.status {
            ChangeStatus::Added | ChangeStatus::Modified => handle(&cf.path, false, &mut mapping),
            ChangeStatus::Deleted => handle(&cf.path, true, &mut mapping),
            ChangeStatus::Renamed { from } => {
                handle(&cf.path, false, &mut mapping);
                handle(from, true, &mut mapping);
            }
        }
    }

    mapping
}

/// Best-effort recovery of the sidecar record id a now-deleted/renamed-away
/// `assets/...` path used to have, by reading its owning `manifest.toml` at
/// `base` (before the deletion). Returns `None` if the base sidecar can't be
/// read/parsed or never declared a record with that bare file name.
pub fn resolve_removed_record_id(
    workspace_root: &Path,
    base: &str,
    deleted_path: &str,
) -> Option<String> {
    let rel = deleted_path.strip_prefix("assets/")?;
    let rel_path = Path::new(rel);
    let file_name = rel_path.file_name()?.to_str()?;
    let dir = rel_path.parent().unwrap_or_else(|| Path::new(""));
    let manifest_git_path = if dir.as_os_str().is_empty() {
        "assets/manifest.toml".to_string()
    } else {
        format!("assets/{}/manifest.toml", dir.to_string_lossy())
    };

    let output = Command::new("git")
        .arg("-C")
        .arg(workspace_root)
        .arg("show")
        .arg(format!("{base}:{manifest_git_path}"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let sidecar: Sidecar = toml::from_str(&text).ok()?;
    sidecar
        .records
        .into_iter()
        .find(|r| r.path == file_name)
        .map(|r| r.id)
}

// ---------------------------------------------------------------------
// Transitive dependency closure
// ---------------------------------------------------------------------

/// Sentinel page id standing in for `index.html`, which every closure always
/// includes -- it is a dependency of any change, but is not itself a
/// per-record `.html` page (see `gallery::mod::write_page`/`generate_filtered`,
/// which writes `index.html` unconditionally regardless of the filter).
pub const INDEX_PAGE_ID: &str = "index";

/// Computes the full set of gallery page ids that must be included given
/// `direct_ids` (records mapped directly from the diff). See this module's
/// doc comment for the exact propagation rules. Pure: takes only the
/// already-built aggregate, no I/O.
pub fn page_closure(
    records: &[&ResolvedRecord],
    direct_ids: &BTreeSet<String>,
) -> BTreeSet<String> {
    let by_id: BTreeMap<&str, &ResolvedRecord> =
        records.iter().map(|r| (r.record.id.as_str(), *r)).collect();
    let composable_gear_ids: Vec<String> = model::composable_gear(records)
        .iter()
        .map(|g| g.record.id.clone())
        .collect();
    let identities = model::fighter_identities(records);

    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut worklist: Vec<String> = direct_ids.iter().cloned().collect();
    let mut page_ids: BTreeSet<String> = BTreeSet::new();
    page_ids.insert(INDEX_PAGE_ID.to_string());

    while let Some(id) = worklist.pop() {
        if !visited.insert(id.clone()) {
            continue;
        }
        page_ids.insert(id.clone());

        let Some(record) = by_id.get(id.as_str()) else {
            continue;
        };

        match record.record.category {
            Category::FighterRuntimePart => {
                if let Some(identity) = model::identity_of(&id) {
                    page_ids.insert(format!("composition.{identity}"));
                    if identity == "human" {
                        for gear_id in &composable_gear_ids {
                            page_ids.insert(format!(
                                "composition.gear.{}",
                                model::last_segment(gear_id)
                            ));
                        }
                    }
                }
            }
            Category::GearRuntimePart | Category::GearOverlay => {
                if composable_gear_ids.contains(&id) {
                    page_ids.insert(format!("composition.gear.{}", model::last_segment(&id)));
                }
            }
            Category::Background => {
                if let Some(scene) = model::background_scene_of(&id) {
                    page_ids.insert(format!("composition.background.{scene}"));
                }
                if id == pages::REPRESENTATIVE_BACKGROUND_ID {
                    // Every fighter/gear part and composition page embeds
                    // this background as a rendered dependency (see
                    // gallery::pages::render_part_page /
                    // facing_canvas), even though its own record id never
                    // appears in their HTML bytes.
                    for identity in &identities {
                        page_ids.insert(format!("composition.{identity}"));
                    }
                    for gear_id in &composable_gear_ids {
                        page_ids
                            .insert(format!("composition.gear.{}", model::last_segment(gear_id)));
                    }
                    for r in records {
                        let is_fighter_part = r.record.category == Category::FighterRuntimePart;
                        let is_composable_gear = composable_gear_ids.contains(&r.record.id);
                        if is_fighter_part || is_composable_gear {
                            worklist.push(r.record.id.clone());
                        }
                    }
                }
            }
            Category::FighterSourceSheet | Category::GearSourceSheet | Category::UiSourceSheet => {
                for r in records {
                    if r.record.source_sheet.as_deref() == Some(id.as_str()) {
                        worklist.push(r.record.id.clone());
                    }
                }
            }
            _ => {}
        }
    }

    page_ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::schema::{Kind, Record, Status};
    use std::fs;

    // ---------------- git fixture harness ----------------

    struct GitRepo {
        root: PathBuf,
    }

    impl GitRepo {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "xtask-changed-{name}-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            let repo = Self { root };
            repo.git(&["init", "-q", "-b", "main"]);
            repo.git(&["config", "user.email", "test@example.com"]);
            repo.git(&["config", "user.name", "Test"]);
            repo
        }

        fn git(&self, args: &[&str]) -> std::process::Output {
            Command::new("git")
                .arg("-C")
                .arg(&self.root)
                .args(args)
                .output()
                .expect("git command spawns")
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }

        fn remove(&self, relative: &str) {
            let _ = fs::remove_file(self.root.join(relative));
        }

        fn commit(&self, message: &str) {
            self.git(&["add", "-A"]);
            let out = self.git(&["commit", "-q", "-m", message]);
            assert!(
                out.status.success(),
                "commit failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        fn checkout_new_branch(&self, name: &str) {
            let out = self.git(&["checkout", "-q", "-b", name]);
            assert!(out.status.success());
        }

        fn rev_parse(&self, r: &str) -> String {
            let out = self.git(&["rev-parse", r]);
            assert!(out.status.success());
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
    }

    impl Drop for GitRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn sprite_sidecar(id: &str, file_name: &str) -> String {
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

    // ---------------- base resolution tests ----------------

    #[test]
    fn explicit_base_flag_wins_and_resolves_to_a_sha() {
        let repo = GitRepo::new("base-explicit");
        repo.write("assets/sprites/player.png", "a");
        repo.write(
            "assets/sprites/manifest.toml",
            &sprite_sidecar("sprites.player", "player.png"),
        );
        repo.commit("initial");
        let head_sha = repo.rev_parse("HEAD");

        let resolved = resolve_base(Some("HEAD"), None, &repo.root).expect("resolves");
        assert_eq!(resolved.sha, head_sha);
        assert_eq!(resolved.source, BaseSource::ExplicitFlag);
    }

    #[test]
    fn invalid_explicit_base_is_an_actionable_failure_not_a_fallback() {
        let repo = GitRepo::new("base-invalid");
        repo.write("assets/sprites/player.png", "a");
        repo.commit("initial");

        let err = resolve_base(Some("not-a-real-ref"), None, &repo.root).unwrap_err();
        match &err {
            BaseError::InvalidExplicitBase { requested, .. } => {
                assert_eq!(requested, "not-a-real-ref");
            }
            other => panic!("expected InvalidExplicitBase, got {other:?}"),
        }
        let message = err.to_string();
        assert!(message.contains("not-a-real-ref"));
        assert!(message.contains("--base"));
    }

    #[test]
    fn github_base_ref_is_tried_as_origin_prefixed_when_bare_ref_is_absent() {
        // Branch is deliberately not named "main" -- a real PR checkout
        // typically has no local branch matching GITHUB_BASE_REF, only a
        // remote-tracking `origin/<base>` ref, which is exactly the case
        // this test simulates.
        let repo = GitRepo::new("base-github-ref");
        repo.git(&["checkout", "-q", "-b", "trunk"]);
        repo.write("assets/sprites/player.png", "a");
        repo.commit("initial");
        let main_sha = repo.rev_parse("HEAD");
        repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        repo.write("assets/sprites/extra.png", "b");
        repo.commit("second commit, moves HEAD past origin/main");

        let resolved =
            resolve_base(None, Some("main"), &repo.root).expect("resolves via origin/main");
        assert_eq!(resolved.sha, main_sha);
        assert_eq!(resolved.source, BaseSource::GithubBaseRef);
    }

    #[test]
    fn merge_base_with_origin_main_is_the_last_resort() {
        let repo = GitRepo::new("base-merge-base");
        repo.write("assets/sprites/player.png", "a");
        repo.commit("initial on main");
        let branch_point = repo.rev_parse("HEAD");
        repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        repo.checkout_new_branch("feature");
        repo.write("assets/sprites/player.png", "a-changed");
        repo.commit("feature change");

        let resolved = resolve_base(None, None, &repo.root).expect("resolves via merge-base");
        assert_eq!(resolved.sha, branch_point);
        assert_eq!(resolved.source, BaseSource::MergeBaseWithOriginMain);
    }

    #[test]
    fn missing_base_is_actionable_and_never_falls_back_to_reviewing_everything() {
        let repo = GitRepo::new("base-missing");
        repo.write("assets/sprites/player.png", "a");
        repo.commit("initial");
        // No origin/main ref exists at all, no --base, no GITHUB_BASE_REF.

        let err = resolve_base(None, None, &repo.root).unwrap_err();
        match &err {
            BaseError::NoUsableBase { attempted } => {
                assert!(!attempted.is_empty());
            }
            other => panic!("expected NoUsableBase, got {other:?}"),
        }
        let message = err.to_string();
        assert!(message.contains("--base"));
        assert!(message.contains("never falls back"));
    }

    // ---------------- diff parsing tests ----------------

    #[test]
    fn parses_added_modified_deleted_and_renamed_lines() {
        let text = "A\tassets/sprites/new.png\n\
                     M\tassets/sprites/manifest.toml\n\
                     D\tassets/sprites/gone.png\n\
                     R100\tassets/sprites/old.png\tassets/sprites/renamed.png\n";
        let parsed = parse_name_status(text);
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].path, "assets/sprites/new.png");
        assert_eq!(parsed[0].status, ChangeStatus::Added);
        assert_eq!(parsed[1].path, "assets/sprites/manifest.toml");
        assert_eq!(parsed[1].status, ChangeStatus::Modified);
        assert_eq!(parsed[2].path, "assets/sprites/gone.png");
        assert_eq!(parsed[2].status, ChangeStatus::Deleted);
        assert_eq!(parsed[3].path, "assets/sprites/renamed.png");
        assert_eq!(
            parsed[3].status,
            ChangeStatus::Renamed {
                from: "assets/sprites/old.png".to_string()
            }
        );
    }

    #[test]
    fn diff_changed_assets_reflects_a_real_two_commit_repo() {
        let repo = GitRepo::new("diff-real");
        repo.write("assets/sprites/player.png", "a");
        repo.write(
            "assets/sprites/manifest.toml",
            &sprite_sidecar("sprites.player", "player.png"),
        );
        repo.commit("initial");
        let base = repo.rev_parse("HEAD");
        repo.write("assets/sprites/player.png", "a-changed");
        repo.commit("change player sprite");

        let changed = diff_changed_assets(&repo.root, &base).expect("diff succeeds");
        assert!(
            changed.iter().any(
                |c| c.path == "assets/sprites/player.png" && c.status == ChangeStatus::Modified
            )
        );
    }

    // ---------------- record helpers for closure tests ----------------

    #[allow(clippy::too_many_arguments)]
    fn record(
        id: &str,
        path: &str,
        category: Category,
        status: Status,
        sidecar: &str,
        attachment: Option<&str>,
        source_sheet: Option<&str>,
    ) -> ResolvedRecord {
        ResolvedRecord {
            sidecar: PathBuf::from(sidecar),
            full_path: PathBuf::from(path),
            record: Record {
                id: id.to_string(),
                path: Path::new(path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                kind: Kind::Image,
                category,
                status,
                provenance: if source_sheet.is_some() {
                    "cropped-from-source-sheet".to_string()
                } else {
                    "repo-generated".to_string()
                },
                license: "CC0 1.0".to_string(),
                generator: if source_sheet.is_some() {
                    None
                } else {
                    Some("gen.py".to_string())
                },
                source_sheet: source_sheet.map(str::to_string),
                license_file: None,
                dimensions: Some([64, 64]),
                sampler: None,
                attachment: attachment.map(str::to_string),
                pivot: Some([0.0, 0.0]),
                display: Some([10.0, 10.0]),
                crop: Some("unknown".to_string()),
            },
        }
    }

    fn human_head() -> ResolvedRecord {
        record(
            "fighters.human.runtime.head",
            "fighters/human/runtime/head.png",
            Category::FighterRuntimePart,
            Status::Runtime,
            "assets/fighters/human/runtime/manifest.toml",
            Some("head"),
            Some("fighters.human.source.sheet"),
        )
    }

    fn human_torso() -> ResolvedRecord {
        record(
            "fighters.human.runtime.torso",
            "fighters/human/runtime/torso.png",
            Category::FighterRuntimePart,
            Status::Runtime,
            "assets/fighters/human/runtime/manifest.toml",
            Some("torso"),
            None,
        )
    }

    fn strigoi_head() -> ResolvedRecord {
        record(
            "fighters.strigoi.runtime.head",
            "fighters/strigoi/runtime/head.png",
            Category::FighterRuntimePart,
            Status::Runtime,
            "assets/fighters/strigoi/runtime/manifest.toml",
            Some("head"),
            None,
        )
    }

    fn gear_palos() -> ResolvedRecord {
        record(
            "fighters.gear.runtime.palos",
            "fighters/gear/runtime/palos.png",
            Category::GearRuntimePart,
            Status::Runtime,
            "assets/fighters/gear/runtime/manifest.toml",
            Some("hand_front"),
            None,
        )
    }

    fn gear_scut() -> ResolvedRecord {
        record(
            "fighters.gear.runtime.scut",
            "fighters/gear/runtime/scut.png",
            Category::GearRuntimePart,
            Status::Runtime,
            "assets/fighters/gear/runtime/manifest.toml",
            Some("hand_back"),
            None,
        )
    }

    fn background_village_near() -> ResolvedRecord {
        record(
            "backgrounds.village-near",
            "backgrounds/village_near.png",
            Category::Background,
            Status::Runtime,
            "assets/backgrounds/manifest.toml",
            None,
            None,
        )
    }

    fn background_village_far() -> ResolvedRecord {
        record(
            "backgrounds.village-far",
            "backgrounds/village_far.png",
            Category::Background,
            Status::Runtime,
            "assets/backgrounds/manifest.toml",
            None,
            None,
        )
    }

    fn source_sheet() -> ResolvedRecord {
        record(
            "fighters.human.source.sheet",
            "fighters/human/source/sheet.png",
            Category::FighterSourceSheet,
            Status::Source,
            "assets/fighters/human/source/manifest.toml",
            None,
            None,
        )
    }

    fn ui_icon() -> ResolvedRecord {
        record(
            "ui.icon-coin",
            "ui/icon_coin.png",
            Category::UiIcon,
            Status::Runtime,
            "assets/ui/manifest.toml",
            None,
            None,
        )
    }

    fn all_records() -> Vec<ResolvedRecord> {
        vec![
            human_head(),
            human_torso(),
            strigoi_head(),
            gear_palos(),
            gear_scut(),
            background_village_near(),
            background_village_far(),
            source_sheet(),
            ui_icon(),
        ]
    }

    fn refs(records: &[ResolvedRecord]) -> Vec<&ResolvedRecord> {
        records.iter().collect()
    }

    // ---------------- changed-file mapping tests ----------------

    #[test]
    fn a_changed_content_file_maps_to_its_own_record_only() {
        let records = all_records();
        let refs = refs(&records);
        let changed = vec![ChangedFile {
            path: "assets/fighters/human/runtime/head.png".to_string(),
            status: ChangeStatus::Modified,
        }];
        let mapping = map_changed_files(Path::new("assets"), &refs, &changed);
        assert_eq!(
            mapping.direct_ids,
            BTreeSet::from(["fighters.human.runtime.head".to_string()])
        );
        assert!(mapping.removed.is_empty());
    }

    #[test]
    fn a_changed_sidecar_maps_conservatively_to_every_record_it_declares() {
        let records = all_records();
        let refs = refs(&records);
        let changed = vec![ChangedFile {
            path: "assets/fighters/human/runtime/manifest.toml".to_string(),
            status: ChangeStatus::Modified,
        }];
        let mapping = map_changed_files(Path::new("assets"), &refs, &changed);
        assert_eq!(
            mapping.direct_ids,
            BTreeSet::from([
                "fighters.human.runtime.head".to_string(),
                "fighters.human.runtime.torso".to_string(),
            ])
        );
    }

    #[test]
    fn a_deleted_file_with_no_current_record_is_surfaced_as_removed() {
        let records = all_records();
        let refs = refs(&records);
        let changed = vec![ChangedFile {
            path: "assets/fighters/human/runtime/foot_back.png".to_string(),
            status: ChangeStatus::Deleted,
        }];
        let mapping = map_changed_files(Path::new("assets"), &refs, &changed);
        assert!(mapping.direct_ids.is_empty());
        assert_eq!(mapping.removed.len(), 1);
        assert_eq!(
            mapping.removed[0].path,
            "assets/fighters/human/runtime/foot_back.png"
        );
        assert_eq!(mapping.removed[0].id, None);
    }

    #[test]
    fn a_rename_surfaces_the_old_path_as_removed_and_the_new_path_as_changed() {
        // The renamed-to path ("torso.png") does resolve to a current
        // record (human_torso), so it becomes a direct id; the renamed-from
        // path ("old_torso.png") resolves to nothing current, so it's
        // surfaced as removed.
        let records = all_records();
        let refs = refs(&records);
        let changed = vec![ChangedFile {
            path: "assets/fighters/human/runtime/torso.png".to_string(),
            status: ChangeStatus::Renamed {
                from: "assets/fighters/human/runtime/old_torso.png".to_string(),
            },
        }];
        let mapping = map_changed_files(Path::new("assets"), &refs, &changed);
        assert!(mapping.direct_ids.contains("fighters.human.runtime.torso"));
        assert_eq!(mapping.removed.len(), 1);
        assert_eq!(
            mapping.removed[0].path,
            "assets/fighters/human/runtime/old_torso.png"
        );
    }

    #[test]
    fn resolve_removed_record_id_reads_the_base_sidecar() {
        let repo = GitRepo::new("removed-id");
        repo.write("assets/sprites/player.png", "a");
        repo.write(
            "assets/sprites/manifest.toml",
            &sprite_sidecar("sprites.player", "player.png"),
        );
        repo.commit("has the sprite");
        let base = repo.rev_parse("HEAD");
        repo.remove("assets/sprites/player.png");
        repo.write("assets/sprites/manifest.toml", "version = 1\n");
        repo.commit("remove the sprite and its record");

        let id = resolve_removed_record_id(&repo.root, &base, "assets/sprites/player.png");
        assert_eq!(id, Some("sprites.player".to_string()));
    }

    #[test]
    fn resolve_removed_record_id_is_none_when_the_base_sidecar_never_had_it() {
        let repo = GitRepo::new("removed-id-none");
        repo.write("assets/sprites/manifest.toml", "version = 1\n");
        repo.commit("empty sidecar");
        let base = repo.rev_parse("HEAD");

        let id = resolve_removed_record_id(&repo.root, &base, "assets/sprites/never_existed.png");
        assert_eq!(id, None);
    }

    // ---------------- dependency closure tests ----------------

    #[test]
    fn direct_fighter_part_change_dirties_its_own_page_and_its_identity_composition() {
        let records = all_records();
        let refs = refs(&records);
        let direct = BTreeSet::from(["fighters.strigoi.runtime.head".to_string()]);
        let closure = page_closure(&refs, &direct);
        assert!(closure.contains("fighters.strigoi.runtime.head"));
        assert!(closure.contains("composition.strigoi"));
        assert!(closure.contains(INDEX_PAGE_ID));
        // Not a human part, so it must not dirty any gear composition.
        assert!(!closure.contains("composition.gear.palos"));
        assert!(!closure.contains("composition.human"));
    }

    #[test]
    fn a_human_part_change_dirties_composition_human_and_every_gear_composition() {
        let records = all_records();
        let refs = refs(&records);
        let direct = BTreeSet::from(["fighters.human.runtime.torso".to_string()]);
        let closure = page_closure(&refs, &direct);
        assert!(closure.contains("composition.human"));
        assert!(closure.contains("composition.gear.palos"));
        assert!(closure.contains("composition.gear.scut"));
        // Unrelated identity must stay excluded.
        assert!(!closure.contains("composition.strigoi"));
        assert!(!closure.contains("fighters.strigoi.runtime.head"));
    }

    #[test]
    fn a_gear_part_change_dirties_only_its_own_gear_composition() {
        let records = all_records();
        let refs = refs(&records);
        let direct = BTreeSet::from(["fighters.gear.runtime.palos".to_string()]);
        let closure = page_closure(&refs, &direct);
        assert!(closure.contains("fighters.gear.runtime.palos"));
        assert!(closure.contains("composition.gear.palos"));
        assert!(!closure.contains("composition.gear.scut"));
        assert!(!closure.contains("composition.human"));
    }

    #[test]
    fn a_source_sheet_change_transitively_dirties_every_dependent_and_its_own_compositions() {
        let records = all_records();
        let refs = refs(&records);
        let direct = BTreeSet::from(["fighters.human.source.sheet".to_string()]);
        let closure = page_closure(&refs, &direct);
        // The source sheet's own page.
        assert!(closure.contains("fighters.human.source.sheet"));
        // human_head names it as source_sheet -> dirtied transitively...
        assert!(closure.contains("fighters.human.runtime.head"));
        // ...which then cascades into human composition + all gear comps.
        assert!(closure.contains("composition.human"));
        assert!(closure.contains("composition.gear.palos"));
        assert!(closure.contains("composition.gear.scut"));
        // human_torso does not reference this sheet, so it stays excluded.
        assert!(!closure.contains("fighters.human.runtime.torso"));
    }

    #[test]
    fn the_representative_background_change_dirties_every_fighter_and_gear_page() {
        assert_eq!(
            pages::REPRESENTATIVE_BACKGROUND_ID,
            "backgrounds.village-near"
        );
        let records = all_records();
        let refs = refs(&records);
        let direct = BTreeSet::from(["backgrounds.village-near".to_string()]);
        let closure = page_closure(&refs, &direct);
        assert!(closure.contains("backgrounds.village-near"));
        assert!(closure.contains("composition.background.village"));
        // Embedded as a rendered dependency in every fighter/gear page.
        assert!(closure.contains("fighters.human.runtime.head"));
        assert!(closure.contains("fighters.human.runtime.torso"));
        assert!(closure.contains("fighters.strigoi.runtime.head"));
        assert!(closure.contains("fighters.gear.runtime.palos"));
        assert!(closure.contains("fighters.gear.runtime.scut"));
        assert!(closure.contains("composition.human"));
        assert!(closure.contains("composition.strigoi"));
        assert!(closure.contains("composition.gear.palos"));
        assert!(closure.contains("composition.gear.scut"));
        // UI is never composited over this background -> stays excluded.
        assert!(!closure.contains("ui.icon-coin"));
    }

    #[test]
    fn a_non_representative_background_layer_only_dirties_its_own_scene() {
        let records = all_records();
        let refs = refs(&records);
        let direct = BTreeSet::from(["backgrounds.village-far".to_string()]);
        let closure = page_closure(&refs, &direct);
        assert!(closure.contains("backgrounds.village-far"));
        assert!(closure.contains("composition.background.village"));
        assert!(!closure.contains("fighters.human.runtime.head"));
        assert!(!closure.contains("composition.human"));
    }

    #[test]
    fn unrelated_assets_are_excluded_from_the_closure() {
        let records = all_records();
        let refs = refs(&records);
        let direct = BTreeSet::from(["ui.icon-coin".to_string()]);
        let closure = page_closure(&refs, &direct);
        assert!(closure.contains("ui.icon-coin"));
        assert!(closure.contains(INDEX_PAGE_ID));
        assert_eq!(
            closure.len(),
            2,
            "a UI icon change must dirty only its own page plus the index, got {closure:?}"
        );
    }

    #[test]
    fn index_is_always_included_even_with_an_empty_direct_set() {
        let records = all_records();
        let refs = refs(&records);
        let closure = page_closure(&refs, &BTreeSet::new());
        assert_eq!(closure, BTreeSet::from([INDEX_PAGE_ID.to_string()]));
    }
}
