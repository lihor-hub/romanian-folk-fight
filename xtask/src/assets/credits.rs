//! Checks `assets/CREDITS.md` against sidecar provenance/licenses.
//!
//! # Synchronization direction
//!
//! Sidecars are authoritative; `CREDITS.md` is the human-readable rollup.
//! This module never regenerates `CREDITS.md` -- it only checks that every
//! non-ignored, non-exempt sidecar record's path is mentioned somewhere in
//! `CREDITS.md`, and that the exact `license` string from the sidecar
//! appears on a line that also mentions the path. Sidecar license strings
//! are authored to match `CREDITS.md`'s existing wording verbatim (e.g.
//! `"CC0 1.0"`, not the SPDX id `"CC0-1.0"`), which keeps this a simple,
//! auditable substring check rather than a fuzzy license-name normalizer.
//!
//! One record is exempt: the bundled font license text
//! (`fonts.ofl-alegreya-license`) is linked from `CREDITS.md` as a citation
//! target for the font's own row rather than getting its own inventory row,
//! so it is excluded from the "path is mentioned" check.

use super::aggregate::ResolvedRecord;
use super::diagnostics::Diagnostic;

/// Records whose path is deliberately not expected to appear as its own
/// `CREDITS.md` row (see module docs).
const CREDITS_ROW_EXEMPT_IDS: &[&str] = &["fonts.ofl-alegreya-license"];

pub fn check(credits_text: &str, records: &[ResolvedRecord]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for resolved in records {
        let id = resolved.record.id.as_str();
        if CREDITS_ROW_EXEMPT_IDS.contains(&id) {
            continue;
        }

        let path_str = resolved.full_path.to_string_lossy().replace('\\', "/");
        let file_name = resolved
            .full_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        let matching_lines: Vec<&str> = credits_text
            .lines()
            .filter(|line| line.contains(path_str.as_str()) || line.contains(file_name))
            .collect();

        if matching_lines.is_empty() {
            diagnostics.push(Diagnostic::CreditsDrift {
                id: id.to_string(),
                path: resolved.full_path.clone(),
                detail: "path is not mentioned anywhere in assets/CREDITS.md".to_string(),
            });
            continue;
        }

        let license_present = matching_lines
            .iter()
            .any(|line| line.contains(resolved.record.license.as_str()));
        if !license_present {
            diagnostics.push(Diagnostic::CreditsDrift {
                id: id.to_string(),
                path: resolved.full_path.clone(),
                detail: format!(
                    "assets/CREDITS.md mentions the path but not the sidecar-declared license {:?}",
                    resolved.record.license
                ),
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::schema::{Category, Kind, Record, Status};
    use std::path::PathBuf;

    fn record(id: &str, path: &str, license: &str) -> ResolvedRecord {
        ResolvedRecord {
            sidecar: PathBuf::from("assets/sprites/manifest.toml"),
            full_path: PathBuf::from(path),
            record: Record {
                id: id.to_string(),
                path: path.rsplit('/').next().unwrap().to_string(),
                kind: Kind::Image,
                category: Category::Sprite,
                status: Status::Runtime,
                provenance: "repo-generated".to_string(),
                license: license.to_string(),
                generator: None,
                source_sheet: None,
                license_file: None,
                dimensions: Some([512, 512]),
                sampler: None,
                attachment: None,
                pivot: None,
                display: None,
                crop: None,
            },
        }
    }

    #[test]
    fn a_path_and_license_both_present_is_clean() {
        let credits = "| `sprites/player.png` | Player | self-generated | CC0 1.0 |\n";
        let records = vec![record("sprites.player", "sprites/player.png", "CC0 1.0")];
        assert!(check(credits, &records).is_empty());
    }

    #[test]
    fn a_path_missing_from_credits_is_flagged() {
        let credits = "no mention of anything here\n";
        let records = vec![record("sprites.player", "sprites/player.png", "CC0 1.0")];
        let diagnostics = check(credits, &records);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            matches!(&diagnostics[0], Diagnostic::CreditsDrift { detail, .. } if detail.contains("not mentioned"))
        );
    }

    #[test]
    fn a_path_present_with_a_different_license_is_flagged() {
        let credits = "| `sprites/player.png` | Player | self-generated | MIT |\n";
        let records = vec![record("sprites.player", "sprites/player.png", "CC0 1.0")];
        let diagnostics = check(credits, &records);
        assert_eq!(diagnostics.len(), 1);
        assert!(
            matches!(&diagnostics[0], Diagnostic::CreditsDrift { detail, .. } if detail.contains("license"))
        );
    }

    #[test]
    fn the_font_license_text_record_is_exempt_from_the_row_check() {
        let credits = "no mention of the license file text at all\n";
        let records = vec![record(
            "fonts.ofl-alegreya-license",
            "fonts/OFL-Alegreya.txt",
            "SIL OFL 1.1",
        )];
        assert!(check(credits, &records).is_empty());
    }
}
