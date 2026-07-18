//! Character-catalog references checked against the sidecar aggregate.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::super::aggregate::Aggregate;
use super::super::diagnostics::Diagnostic;
use super::super::schema::Status;

const CATALOG_PATH: &str = "fighters/catalog/human-foundation.json";
const CATALOG_VERSION: u32 = 2;

#[derive(Deserialize)]
struct CatalogDocument {
    version: u32,
    parts: Vec<CatalogPart>,
}

#[derive(Deserialize)]
struct CatalogPart {
    id: String,
    asset_path: String,
    attachment: CatalogAttachment,
}

#[derive(Deserialize)]
struct CatalogAttachment {
    point: String,
    pivot: [f32; 2],
}

pub fn check(assets_root: &Path, aggregate: &Aggregate) -> Vec<Diagnostic> {
    let relative = Path::new(CATALOG_PATH);
    let display_path = Path::new("assets").join(relative);
    match fs::read_to_string(assets_root.join(relative)) {
        Ok(json) => check_json(&display_path, &json, aggregate),
        Err(error) => vec![Diagnostic::CatalogContent {
            catalog: display_path,
            part_id: "<catalog>".to_owned(),
            detail: format!("could not read runtime catalog: {error}"),
        }],
    }
}

fn check_json(catalog_path: &Path, json: &str, aggregate: &Aggregate) -> Vec<Diagnostic> {
    let document: CatalogDocument = match serde_json::from_str(json) {
        Ok(document) => document,
        Err(error) => {
            return vec![Diagnostic::CatalogContent {
                catalog: catalog_path.to_path_buf(),
                part_id: "<catalog>".to_owned(),
                detail: format!("invalid JSON: {error}"),
            }];
        }
    };
    let mut diagnostics = Vec::new();
    if document.version != CATALOG_VERSION {
        diagnostics.push(Diagnostic::CatalogContent {
            catalog: catalog_path.to_path_buf(),
            part_id: "<catalog>".to_owned(),
            detail: format!(
                "unsupported schema version {} (expected {CATALOG_VERSION})",
                document.version
            ),
        });
    }

    for part in document.parts {
        validate_part(catalog_path, part, aggregate, &mut diagnostics);
    }
    diagnostics
}

fn validate_part(
    catalog_path: &Path,
    part: CatalogPart,
    aggregate: &Aggregate,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let asset_path = PathBuf::from(&part.asset_path);
    let Some(record) = aggregate
        .records
        .iter()
        .find(|record| record.full_path == asset_path)
    else {
        diagnostics.push(Diagnostic::CatalogContent {
            catalog: catalog_path.to_path_buf(),
            part_id: part.id,
            detail: format!(
                "asset path {:?} is not registered for runtime",
                part.asset_path
            ),
        });
        return;
    };
    if record.record.status != Status::Runtime {
        diagnostics.push(Diagnostic::CatalogContent {
            catalog: catalog_path.to_path_buf(),
            part_id: part.id.clone(),
            detail: format!(
                "asset path {:?} has status `{}`, expected `runtime`",
                part.asset_path, record.record.status
            ),
        });
    }
    if record.record.attachment.as_deref() != Some(part.attachment.point.as_str()) {
        diagnostics.push(Diagnostic::CatalogContent {
            catalog: catalog_path.to_path_buf(),
            part_id: part.id.clone(),
            detail: format!(
                "attachment point {:?} disagrees with sidecar {:?}",
                part.attachment.point, record.record.attachment
            ),
        });
    }
    if record.record.pivot != Some(part.attachment.pivot) {
        diagnostics.push(Diagnostic::CatalogContent {
            catalog: catalog_path.to_path_buf(),
            part_id: part.id,
            detail: format!(
                "pivot {:?} disagrees with sidecar {:?}",
                part.attachment.pivot, record.record.pivot
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::assets::aggregate;

    use super::{check, check_json};

    #[test]
    fn bundled_catalog_references_registered_runtime_assets_with_matching_rig_metadata() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives below the workspace root");
        let assets = workspace.join("assets");
        let aggregate = aggregate::build(&assets);

        assert!(check(&assets, &aggregate).is_empty());
    }

    #[test]
    fn catalog_check_reports_an_unregistered_asset_reference() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives below the workspace root");
        let assets = workspace.join("assets");
        let aggregate = aggregate::build(&assets);
        let source = std::fs::read_to_string(assets.join("fighters/catalog/human-foundation.json"))
            .expect("bundled catalog is readable");
        let invalid = source.replace(
            "fighters/human/runtime/hair.png",
            "fighters/human/runtime/not-registered.png",
        );

        let diagnostics = check_json(
            Path::new("assets/fighters/catalog/human-foundation.json"),
            &invalid,
            &aggregate,
        );

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .to_string()
                .contains("not registered for runtime")
        }));
    }
}
