//! Filesystem discovery: finds every sidecar (`manifest.toml`) and every
//! plain file under `assets/`, independent of the schema (so a directory
//! listing failure never depends on any file having parsed successfully).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// One `manifest.toml` found under `assets/`, with its owning directory
/// (relative to `assets/`) and raw file contents (not yet parsed).
pub struct FoundSidecar {
    /// Directory the sidecar lives in and describes, relative to `assets/`.
    /// Empty for the `assets/manifest.toml` root sidecar.
    pub dir: PathBuf,
    pub sidecar_path: PathBuf,
    pub contents: String,
}

/// Recursively walks `assets_root`, returning every `manifest.toml` sidecar
/// (parsed as raw text) and every other file's path relative to
/// `assets_root`. `manifest.toml` files are excluded from the file list --
/// they are sidecar infrastructure, not assets (see `schema.rs` docs).
pub fn walk_assets(assets_root: &Path) -> io::Result<(Vec<FoundSidecar>, Vec<PathBuf>)> {
    let mut sidecars = Vec::new();
    let mut files = Vec::new();
    walk_dir(assets_root, assets_root, &mut sidecars, &mut files)?;
    sidecars.sort_by(|a, b| a.dir.cmp(&b.dir));
    files.sort();
    Ok((sidecars, files))
}

fn walk_dir(
    assets_root: &Path,
    dir: &Path,
    sidecars: &mut Vec<FoundSidecar>,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            walk_dir(assets_root, &path, sidecars, files)?;
        } else if file_type.is_file() {
            if path.file_name().and_then(|n| n.to_str()) == Some("manifest.toml") {
                let contents = fs::read_to_string(&path)?;
                let dir_rel = path
                    .parent()
                    .expect("a file always has a parent")
                    .strip_prefix(assets_root)
                    .expect("walked path is under assets_root")
                    .to_path_buf();
                sidecars.push(FoundSidecar {
                    dir: dir_rel,
                    sidecar_path: path,
                    contents,
                });
            } else {
                let rel = path
                    .strip_prefix(assets_root)
                    .expect("walked path is under assets_root")
                    .to_path_buf();
                files.push(rel);
            }
        }
    }
    Ok(())
}

/// Checks whether `relative_path` (relative to `root`) resolves to a file
/// whose on-disk casing matches exactly, component by component. Needed
/// because case-insensitive-but-preserving file systems (macOS default,
/// Windows) would otherwise let a case-mismatched sidecar path "work"
/// locally while silently breaking on case-sensitive Linux CI.
///
/// Returns:
/// - `Ok(true)` if every path component matches on-disk casing exactly.
/// - `Ok(false)` if the path is missing entirely (no case-insensitive match
///   either).
/// - `Err(actual)` if the path exists under a different casing --
///   `actual` is the on-disk path with correct casing, for a precise
///   diagnostic.
pub fn case_correct(root: &Path, relative_path: &Path) -> CaseCheck {
    let mut current = root.to_path_buf();
    let mut actual = PathBuf::new();
    for component in relative_path.components() {
        let wanted = component.as_os_str();
        let Ok(entries) = fs::read_dir(&current) else {
            return CaseCheck::Missing;
        };
        let mut exact = false;
        let mut case_insensitive_match: Option<PathBuf> = None;
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name == wanted {
                exact = true;
                actual.push(&name);
                break;
            }
            if name.to_string_lossy().to_lowercase() == wanted.to_string_lossy().to_lowercase() {
                case_insensitive_match = Some(actual.join(&name));
            }
        }
        if exact {
            current.push(wanted);
            continue;
        }
        return match case_insensitive_match {
            Some(_) => {
                // Keep walking with the wanted (mismatched) name so the
                // final `actual` reflects only the first mismatch cleanly;
                // report failure now since first-mismatch is enough detail.
                CaseCheck::CaseMismatch
            }
            None => CaseCheck::Missing,
        };
    }
    CaseCheck::Match
}

#[derive(Debug, PartialEq, Eq)]
pub enum CaseCheck {
    Match,
    CaseMismatch,
    Missing,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "xtask-assets-discover-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn walk_assets_finds_sidecars_and_files_but_excludes_manifest_toml_itself() {
        let root = temp_dir("walk");
        fs::create_dir_all(root.join("audio")).unwrap();
        fs::write(root.join("audio/manifest.toml"), "version = 1\n").unwrap();
        fs::write(root.join("audio/music_menu.ogg"), b"fake").unwrap();
        fs::write(root.join("CREDITS.md"), b"credits").unwrap();

        let (sidecars, files) = walk_assets(&root).unwrap();
        assert_eq!(sidecars.len(), 1);
        assert_eq!(sidecars[0].dir, PathBuf::from("audio"));
        assert!(sidecars[0].contents.contains("version = 1"));

        assert_eq!(
            files,
            vec![
                PathBuf::from("CREDITS.md"),
                PathBuf::from("audio/music_menu.ogg")
            ]
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn case_correct_matches_exact_casing() {
        let root = temp_dir("case-exact");
        fs::create_dir_all(root.join("audio")).unwrap();
        fs::write(root.join("audio/music_menu.ogg"), b"fake").unwrap();

        let result = case_correct(&root, Path::new("audio/music_menu.ogg"));
        assert_eq!(result, CaseCheck::Match);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn case_correct_flags_a_mismatched_component() {
        let root = temp_dir("case-mismatch");
        fs::create_dir_all(root.join("audio")).unwrap();
        fs::write(root.join("audio/Music_Menu.ogg"), b"fake").unwrap();

        let result = case_correct(&root, Path::new("audio/music_menu.ogg"));
        assert_eq!(result, CaseCheck::CaseMismatch);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn case_correct_reports_missing_when_no_match_exists_at_all() {
        let root = temp_dir("case-missing");
        fs::create_dir_all(root.join("audio")).unwrap();

        let result = case_correct(&root, Path::new("audio/does_not_exist.ogg"));
        assert_eq!(result, CaseCheck::Missing);

        let _ = fs::remove_dir_all(&root);
    }
}
