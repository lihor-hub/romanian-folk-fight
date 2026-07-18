//! Character-catalog references checked against the sidecar aggregate.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::super::aggregate::Aggregate;
use super::super::diagnostics::Diagnostic;
use super::super::schema::Status;

const CATALOG_PATH: &str = "fighters/catalog/human-foundation.json";
const CATALOG_VERSION: u32 = 3;

#[derive(Deserialize)]
struct CatalogDocument {
    version: u32,
    parts: Vec<CatalogPart>,
}

#[derive(Deserialize)]
struct CatalogPart {
    id: String,
    layers: Vec<CatalogLayer>,
}

#[derive(Deserialize)]
struct CatalogLayer {
    asset_path: String,
    attachment: CatalogAttachment,
    #[serde(default)]
    material: CatalogMaterial,
}

#[derive(Deserialize)]
struct CatalogAttachment {
    point: String,
    pivot: [f32; 2],
}

#[derive(Default, Deserialize)]
struct CatalogMaterial {
    #[serde(default)]
    mask_path: Option<String>,
    #[serde(default)]
    normal_path: Option<String>,
    #[serde(default)]
    shadow_path: Option<String>,
}

pub fn check(assets_root: &Path, aggregate: &Aggregate) -> Vec<Diagnostic> {
    let relative = Path::new(CATALOG_PATH);
    let display_path = Path::new("assets").join(relative);
    match fs::read_to_string(assets_root.join(relative)) {
        Ok(json) => check_json(assets_root, &display_path, &json, aggregate),
        Err(error) => vec![Diagnostic::CatalogContent {
            catalog: display_path,
            part_id: "<catalog>".to_owned(),
            detail: format!("could not read runtime catalog: {error}"),
        }],
    }
}

fn check_json(
    assets_root: &Path,
    catalog_path: &Path,
    json: &str,
    aggregate: &Aggregate,
) -> Vec<Diagnostic> {
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
        for layer in part.layers {
            validate_layer(
                assets_root,
                catalog_path,
                &part.id,
                layer,
                aggregate,
                &mut diagnostics,
            );
        }
    }
    diagnostics
}

fn validate_layer(
    assets_root: &Path,
    catalog_path: &Path,
    part_id: &str,
    layer: CatalogLayer,
    aggregate: &Aggregate,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let asset_path = PathBuf::from(&layer.asset_path);
    let Some(record) = aggregate
        .records
        .iter()
        .find(|record| record.full_path == asset_path)
    else {
        diagnostics.push(Diagnostic::CatalogContent {
            catalog: catalog_path.to_path_buf(),
            part_id: part_id.to_owned(),
            detail: format!(
                "asset path {:?} is not registered for runtime",
                layer.asset_path
            ),
        });
        return;
    };
    if record.record.status != Status::Runtime {
        diagnostics.push(Diagnostic::CatalogContent {
            catalog: catalog_path.to_path_buf(),
            part_id: part_id.to_owned(),
            detail: format!(
                "asset path {:?} has status `{}`, expected `runtime`",
                layer.asset_path, record.record.status
            ),
        });
    }
    if record.record.attachment.as_deref() != Some(layer.attachment.point.as_str()) {
        diagnostics.push(Diagnostic::CatalogContent {
            catalog: catalog_path.to_path_buf(),
            part_id: part_id.to_owned(),
            detail: format!(
                "attachment point {:?} disagrees with sidecar {:?}",
                layer.attachment.point, record.record.attachment
            ),
        });
    }
    if record.record.pivot != Some(layer.attachment.pivot) {
        diagnostics.push(Diagnostic::CatalogContent {
            catalog: catalog_path.to_path_buf(),
            part_id: part_id.to_owned(),
            detail: format!(
                "pivot {:?} disagrees with sidecar {:?}",
                layer.attachment.pivot, record.record.pivot
            ),
        });
    }
    let Some(albedo_dimensions) = record.record.dimensions else {
        diagnostics.push(Diagnostic::CatalogContent {
            catalog: catalog_path.to_path_buf(),
            part_id: part_id.to_owned(),
            detail: format!(
                "asset path {:?} has no recorded dimensions",
                layer.asset_path
            ),
        });
        return;
    };

    let companion_context = MaterialCompanionContext {
        assets_root,
        catalog_path,
        part_id,
        layer: &layer,
        albedo_dimensions,
        aggregate,
    };
    for (channel, companion_path) in [
        ("mask_path", layer.material.mask_path.as_deref()),
        ("normal_path", layer.material.normal_path.as_deref()),
        ("shadow_path", layer.material.shadow_path.as_deref()),
    ] {
        let Some(companion_path) = companion_path else {
            continue;
        };
        companion_context.validate(channel, companion_path, diagnostics);
    }
}

struct MaterialCompanionContext<'a> {
    assets_root: &'a Path,
    catalog_path: &'a Path,
    part_id: &'a str,
    layer: &'a CatalogLayer,
    albedo_dimensions: [u32; 2],
    aggregate: &'a Aggregate,
}

impl MaterialCompanionContext<'_> {
    fn validate(&self, channel: &str, companion_path: &str, diagnostics: &mut Vec<Diagnostic>) {
        let Some(companion) = self
            .aggregate
            .records
            .iter()
            .find(|record| record.full_path == Path::new(companion_path))
        else {
            diagnostics.push(Diagnostic::CatalogContent {
                catalog: self.catalog_path.to_path_buf(),
                part_id: self.part_id.to_owned(),
                detail: format!(
                    "material `{channel}` asset {companion_path:?} is not registered for runtime"
                ),
            });
            return;
        };
        if companion.record.status != Status::Runtime {
            diagnostics.push(Diagnostic::CatalogContent {
                catalog: self.catalog_path.to_path_buf(),
                part_id: self.part_id.to_owned(),
                detail: format!("material `{channel}` asset {companion_path:?} is not runtime"),
            });
        }
        if companion.record.attachment.as_deref() != Some(self.layer.attachment.point.as_str()) {
            diagnostics.push(Diagnostic::CatalogContent {
                catalog: self.catalog_path.to_path_buf(),
                part_id: self.part_id.to_owned(),
                detail: format!(
                    "material `{channel}` attachment {:?} disagrees with layer attachment {:?}",
                    companion.record.attachment, self.layer.attachment.point
                ),
            });
        }
        if companion.record.pivot != Some(self.layer.attachment.pivot) {
            diagnostics.push(Diagnostic::CatalogContent {
                catalog: self.catalog_path.to_path_buf(),
                part_id: self.part_id.to_owned(),
                detail: format!(
                    "material `{channel}` pivot {:?} disagrees with layer pivot {:?}",
                    companion.record.pivot, self.layer.attachment.pivot
                ),
            });
        }
        if companion.record.dimensions != Some(self.albedo_dimensions) {
            diagnostics.push(Diagnostic::CatalogContent {
                catalog: self.catalog_path.to_path_buf(),
                part_id: self.part_id.to_owned(),
                detail: format!(
                    "material `{channel}` dimensions {:?} disagree with albedo dimensions {:?}",
                    companion.record.dimensions, self.albedo_dimensions
                ),
            });
        }
        match matching_alpha(
            &self.assets_root.join(&self.layer.asset_path),
            &self.assets_root.join(companion_path),
        ) {
            Ok(true) => {}
            Ok(false) => diagnostics.push(Diagnostic::CatalogContent {
                catalog: self.catalog_path.to_path_buf(),
                part_id: self.part_id.to_owned(),
                detail: format!("material `{channel}` alpha disagrees with albedo alpha"),
            }),
            Err(error) => diagnostics.push(Diagnostic::CatalogContent {
                catalog: self.catalog_path.to_path_buf(),
                part_id: self.part_id.to_owned(),
                detail: format!("could not compare material `{channel}` alpha: {error}"),
            }),
        }
    }
}

fn matching_alpha(albedo_path: &Path, companion_path: &Path) -> Result<bool, image::ImageError> {
    let albedo = image::open(albedo_path)?.to_rgba8();
    let companion = image::open(companion_path)?.to_rgba8();
    Ok(albedo.dimensions() == companion.dimensions()
        && albedo
            .pixels()
            .zip(companion.pixels())
            .all(|(albedo, companion)| albedo[3] == companion[3]))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::assets::aggregate;

    use super::{check, check_json};

    fn catalog_with_layer(asset_path: &str, pivot: [f32; 2]) -> String {
        serde_json::json!({
            "version": 3,
            "parts": [{
                "id": "human.hair.test.v1",
                "layers": [{
                    "asset_path": asset_path,
                    "attachment": {
                        "point": "hair",
                        "pivot": pivot,
                        "draw_layer": 0
                    },
                    "material": {}
                }]
            }]
        })
        .to_string()
    }

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
            &assets,
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

    #[test]
    fn catalog_check_reports_an_unregistered_layer_asset() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives below the workspace root");
        let assets = workspace.join("assets");
        let aggregate = aggregate::build(&assets);
        let invalid = catalog_with_layer("fighters/human/runtime/not-registered.png", [1.0, 71.0]);

        let diagnostics = check_json(
            &assets,
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

    #[test]
    fn catalog_check_reports_a_layer_pivot_mismatch() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask lives below the workspace root");
        let assets = workspace.join("assets");
        let aggregate = aggregate::build(&assets);
        let invalid = catalog_with_layer("fighters/human/runtime/hair.png", [99.0, 99.0]);

        let diagnostics = check_json(
            &assets,
            Path::new("assets/fighters/catalog/human-foundation.json"),
            &invalid,
            &aggregate,
        );

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.to_string().contains("pivot [99.0, 99.0]"))
        );
    }
}
