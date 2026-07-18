//! Validated, data-driven part records for the modular character pipeline.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::OnceLock,
};

use bevy::prelude::Resource;
use serde::Deserialize;

use super::{CharacterDefinition, PartId, PartSelections, SkeletonFamily};

/// Only catalog schema understood by this foundation build.
pub const CHARACTER_CATALOG_VERSION: u32 = 2;

/// The semantic character region occupied by a catalog part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyRegion {
    Body,
    Face,
    Hair,
    FacialHair,
    Torso,
    Legs,
    Feet,
    Waist,
    Accessory,
}

/// Attachment data consumed by the cutout renderer once catalog resolution is
/// connected to the existing rig.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentMetadata {
    pub point: String,
    pub pivot: [f32; 2],
    pub draw_layer: i32,
}

/// Semantic material inputs attached to a character part's required albedo.
///
/// The catalog deliberately names only asset channels and authored semantic
/// controls. Renderer implementation details remain Rust-owned so catalog
/// data can evolve without exposing shader configuration as authoring API.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialMetadata {
    #[serde(default)]
    pub mask_path: Option<String>,
    #[serde(default)]
    pub normal_path: Option<String>,
    #[serde(default)]
    pub shadow_path: Option<String>,
    #[serde(default)]
    pub palette: Vec<PaletteRegion>,
    #[serde(default)]
    pub depth_offset: Option<f32>,
    #[serde(default)]
    pub highlight: Option<f32>,
}

/// The closed semantic regions a recolorable material mask may expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaletteRegion {
    Skin,
    Hair,
    Cloth,
    Embroidery,
    Leather,
    Metal,
}

/// One independently selectable character part from an authored catalog.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartRecord {
    pub id: PartId,
    pub region: BodyRegion,
    pub asset_path: String,
    pub skeletons: Vec<SkeletonFamily>,
    pub cultural_tags: Vec<String>,
    pub attachment: AttachmentMetadata,
    #[serde(default)]
    pub material: MaterialMetadata,
    #[serde(default)]
    pub exclusions: Vec<PartId>,
    #[serde(default)]
    pub companions: Vec<PartId>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogDocument {
    version: u32,
    parts: Vec<PartRecord>,
    known_good_human: PartSelections,
    known_good_human_cultural_tags: Vec<String>,
}

/// A validated lookup table for character parts and a safe human fallback.
#[derive(Resource, Debug, Clone)]
pub struct CharacterCatalog {
    version: u32,
    parts: HashMap<PartId, PartRecord>,
    duplicate_ids: Vec<PartId>,
    known_good_human: PartSelections,
    known_good_human_cultural_tags: Vec<String>,
}

/// Parses and version-checks a human catalog without requiring every authored
/// record to be valid. Generation and normal resolution still call
/// [`CharacterCatalog::validate`]; keeping parsing separate lets the runtime
/// resolve the independently validated known-good slice after an unrelated
/// record breaks whole-catalog validation.
pub fn load_human_catalog(json: &str) -> Result<CharacterCatalog, CatalogError> {
    CharacterCatalog::from_json(json)
}

/// Single parsed and version-checked catalog instance shared by generation,
/// rendering, and the ECS resource registered by [`super::CharacterPlugin`].
pub fn bundled_human_catalog() -> Result<&'static CharacterCatalog, CatalogError> {
    static CATALOG: OnceLock<Result<CharacterCatalog, CatalogError>> = OnceLock::new();
    match CATALOG.get_or_init(|| {
        load_human_catalog(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
    }) {
        Ok(catalog) => Ok(catalog),
        Err(error) => Err(error.clone()),
    }
}

impl CharacterCatalog {
    /// Parses one catalog document without discarding duplicate authored IDs.
    ///
    /// Call [`Self::validate`] before using a catalog from untrusted or newly
    /// authored data. [`Self::resolve`] always performs that validation.
    pub fn from_json(json: &str) -> Result<Self, CatalogError> {
        let document: CatalogDocument = serde_json::from_str(json)
            .map_err(|error| CatalogError::InvalidJson(error.to_string()))?;
        if document.version != CHARACTER_CATALOG_VERSION {
            return Err(CatalogError::UnsupportedCatalogVersion {
                found: document.version,
                supported: CHARACTER_CATALOG_VERSION,
            });
        }
        let mut parts = HashMap::with_capacity(document.parts.len());
        let mut duplicate_ids = Vec::new();

        for part in document.parts {
            if parts.contains_key(&part.id) {
                duplicate_ids.push(part.id);
            } else {
                parts.insert(part.id.clone(), part);
            }
        }

        Ok(Self {
            version: document.version,
            parts,
            duplicate_ids,
            known_good_human: document.known_good_human,
            known_good_human_cultural_tags: document.known_good_human_cultural_tags,
        })
    }

    /// Returns the catalog schema version supplied by its authored document.
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Returns the first complete human selections used as the explicit
    /// versioned fallback when later runtime adapters need one.
    pub fn known_good_human(&self) -> &PartSelections {
        &self.known_good_human
    }

    /// Returns the cultural profile tags validated with the human fallback.
    pub fn known_good_human_cultural_tags(&self) -> &[String] {
        &self.known_good_human_cultural_tags
    }

    /// Returns the resolved record for a stable part ID, when present.
    pub fn part(&self, id: &PartId) -> Option<&PartRecord> {
        self.parts.get(id)
    }

    /// Returns every part compatible with the requested semantic slot,
    /// skeleton, and cultural profile.
    ///
    /// The returned order intentionally follows no contract: callers that
    /// make deterministic choices must order records by their stable IDs
    /// before selecting one.
    pub fn compatible_parts(
        &self,
        region: BodyRegion,
        skeleton: SkeletonFamily,
        culture_tags: &[String],
    ) -> Vec<&PartRecord> {
        self.parts
            .values()
            .filter(|part| {
                part.region == region
                    && part.skeletons.contains(&skeleton)
                    && culture_tags
                        .iter()
                        .any(|tag| part.cultural_tags.contains(tag))
            })
            .collect()
    }

    /// Validates catalog-wide relationships before a definition is resolved.
    pub fn validate(&self) -> Result<(), CatalogError> {
        if let Some(id) = self.duplicate_ids.first() {
            return Err(CatalogError::DuplicatePartId(id.clone()));
        }

        for region in Self::REQUIRED_HUMAN_REGIONS {
            if !self.parts.values().any(|part| part.region == region) {
                return Err(CatalogError::MissingRequiredRegion(region));
            }
        }

        for part in self.parts.values() {
            validate_part_content(part)?;
            for companion in &part.companions {
                if !self.parts.contains_key(companion) {
                    return Err(CatalogError::MissingCompanionPart {
                        part_id: part.id.clone(),
                        companion: companion.clone(),
                    });
                }
            }
            for exclusion in &part.exclusions {
                if !self.parts.contains_key(exclusion) {
                    return Err(CatalogError::MissingExclusionPart {
                        part_id: part.id.clone(),
                        exclusion: exclusion.clone(),
                    });
                }
            }
        }

        self.resolve_parts(
            &self.known_good_human,
            SkeletonFamily::Human,
            &self.known_good_human_cultural_tags,
        )
        .map(|_| ())
    }

    /// Resolves a stable definition into the records needed by the renderer.
    pub fn resolve(
        &self,
        definition: &CharacterDefinition,
    ) -> Result<ResolvedCharacter, CatalogError> {
        if definition.version != super::CHARACTER_DEFINITION_VERSION {
            return Err(CatalogError::UnsupportedDefinitionVersion {
                found: definition.version,
                supported: super::CHARACTER_DEFINITION_VERSION,
            });
        }
        self.validate()?;

        let parts = self.resolve_parts(
            &definition.parts,
            definition.skeleton,
            &definition.culture.tags,
        )?;

        Ok(ResolvedCharacter {
            definition: definition.clone(),
            parts,
        })
    }

    /// Resolves only the catalog-owned known-good selection. This deliberately
    /// skips unrelated records after the primary catalog validation fails,
    /// while still validating every selected fallback record's content and
    /// compatibility before it reaches the renderer.
    pub fn resolve_known_good_human(&self) -> Result<ResolvedCharacter, CatalogError> {
        let definition = CharacterDefinition {
            version: super::CHARACTER_DEFINITION_VERSION,
            seed: None,
            skeleton: SkeletonFamily::Human,
            culture: super::CulturalProfile {
                tags: self.known_good_human_cultural_tags.clone(),
            },
            parts: self.known_good_human.clone(),
            appearance: super::PlayerAppearance::default(),
        };
        let parts = self.resolve_parts(
            &definition.parts,
            definition.skeleton,
            &definition.culture.tags,
        )?;
        Ok(ResolvedCharacter { definition, parts })
    }

    fn resolve_parts(
        &self,
        part_selections: &PartSelections,
        skeleton: SkeletonFamily,
        culture_tags: &[String],
    ) -> Result<HashMap<PartId, PartRecord>, CatalogError> {
        let selections = selected_parts(part_selections);
        let selected_ids = selections
            .iter()
            .map(|(id, _)| (*id).clone())
            .collect::<HashSet<_>>();
        let mut parts = HashMap::with_capacity(selections.len());

        for (id, expected_region) in selections {
            let part = self
                .part(id)
                .ok_or_else(|| CatalogError::UnknownPart(id.clone()))?;
            validate_part_content(part)?;
            if part.region != expected_region {
                return Err(CatalogError::WrongRegion {
                    part_id: id.clone(),
                    expected: expected_region,
                    actual: part.region,
                });
            }
            if !part.skeletons.contains(&skeleton) {
                return Err(CatalogError::IncompatibleSkeleton {
                    part_id: id.clone(),
                    skeleton,
                });
            }
            if !culture_tags
                .iter()
                .any(|tag| part.cultural_tags.contains(tag))
            {
                return Err(CatalogError::IncompatibleCulture {
                    part_id: id.clone(),
                    culture_tags: culture_tags.to_vec(),
                });
            }
            for companion in &part.companions {
                if !selected_ids.contains(companion) {
                    return Err(CatalogError::MissingSelectedCompanion {
                        part_id: id.clone(),
                        companion: companion.clone(),
                    });
                }
            }
            for exclusion in &part.exclusions {
                if selected_ids.contains(exclusion) {
                    return Err(CatalogError::ExcludedPartCombination {
                        part_id: id.clone(),
                        excluded: exclusion.clone(),
                    });
                }
            }

            parts.insert(id.clone(), part.clone());
        }

        Ok(parts)
    }

    const REQUIRED_HUMAN_REGIONS: [BodyRegion; 6] = [
        BodyRegion::Body,
        BodyRegion::Face,
        BodyRegion::Hair,
        BodyRegion::Torso,
        BodyRegion::Legs,
        BodyRegion::Feet,
    ];
}

fn validate_part_content(part: &PartRecord) -> Result<(), CatalogError> {
    let Some(asset_attachment) = runtime_asset_attachment(&part.asset_path) else {
        return Err(CatalogError::UnregisteredAssetPath {
            part_id: part.id.clone(),
            asset_path: part.asset_path.clone(),
        });
    };
    if !attachment_compatible_with_region(part.region, &part.attachment.point) {
        return Err(CatalogError::IncompatibleAttachmentPoint {
            part_id: part.id.clone(),
            region: part.region,
            point: part.attachment.point.clone(),
        });
    }
    if asset_attachment != part.attachment.point {
        return Err(CatalogError::AssetAttachmentMismatch {
            part_id: part.id.clone(),
            asset_path: part.asset_path.clone(),
            expected: asset_attachment.to_owned(),
            actual: part.attachment.point.clone(),
        });
    }
    validate_material_content(part)?;
    Ok(())
}

fn validate_material_content(part: &PartRecord) -> Result<(), CatalogError> {
    for (channel, asset_path) in [
        ("mask_path", part.material.mask_path.as_deref()),
        ("normal_path", part.material.normal_path.as_deref()),
        ("shadow_path", part.material.shadow_path.as_deref()),
    ] {
        let Some(asset_path) = asset_path else {
            continue;
        };
        if !is_human_material_channel_path(asset_path) {
            return Err(CatalogError::InvalidMaterialChannelPath {
                part_id: part.id.clone(),
                channel: channel.to_owned(),
                asset_path: asset_path.to_owned(),
            });
        }
    }

    validate_material_number(part, "depth_offset", part.material.depth_offset, -1.0..=1.0)?;
    validate_material_number(part, "highlight", part.material.highlight, 0.0..=1.0)?;
    Ok(())
}

/// Keeps runtime catalog data inside the human runtime asset namespace without
/// coupling the game crate to individual sidecar records. Asset CI owns the
/// stronger registration and rig-attachment checks, so new authored channels
/// can be added with their sidecar records without a Rust code change.
fn is_human_material_channel_path(path: &str) -> bool {
    let Some(file_name) = path.strip_prefix("fighters/human/runtime/") else {
        return false;
    };
    !file_name.is_empty()
        && file_name.ends_with(".png")
        && file_name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.')
        })
}

fn validate_material_number(
    part: &PartRecord,
    setting: &str,
    value: Option<f32>,
    range: std::ops::RangeInclusive<f32>,
) -> Result<(), CatalogError> {
    if value.is_some_and(|value| !value.is_finite() || !range.contains(&value)) {
        return Err(CatalogError::InvalidMaterialNumeric {
            part_id: part.id.clone(),
            setting: setting.to_owned(),
        });
    }
    Ok(())
}

fn attachment_compatible_with_region(region: BodyRegion, point: &str) -> bool {
    match region {
        BodyRegion::Body => matches!(
            point,
            "upper_arm_back"
                | "forearm_back"
                | "hand_back"
                | "upper_arm_front"
                | "forearm_front"
                | "hand_front"
        ),
        BodyRegion::Face | BodyRegion::FacialHair => point == "head",
        BodyRegion::Hair => point == "hair",
        BodyRegion::Torso | BodyRegion::Waist => point == "torso",
        BodyRegion::Legs => matches!(
            point,
            "thigh_back" | "shin_back" | "thigh_front" | "shin_front"
        ),
        BodyRegion::Feet => matches!(point, "foot_back" | "foot_front"),
        BodyRegion::Accessory => matches!(
            point,
            "hair"
                | "head"
                | "torso"
                | "upper_arm_back"
                | "forearm_back"
                | "hand_back"
                | "thigh_back"
                | "shin_back"
                | "foot_back"
                | "upper_arm_front"
                | "forearm_front"
                | "hand_front"
                | "thigh_front"
                | "shin_front"
                | "foot_front"
        ),
    }
}

fn runtime_asset_attachment(path: &str) -> Option<&'static str> {
    Some(match path {
        "fighters/human/runtime/hair.png" => "hair",
        "fighters/human/runtime/head.png" => "head",
        "fighters/human/runtime/torso.png" => "torso",
        "fighters/human/runtime/upper_arm_back.png" => "upper_arm_back",
        "fighters/human/runtime/forearm_back.png" => "forearm_back",
        "fighters/human/runtime/hand_back.png" => "hand_back",
        "fighters/human/runtime/thigh_back.png" => "thigh_back",
        "fighters/human/runtime/shin_back.png" => "shin_back",
        "fighters/human/runtime/foot_back.png" => "foot_back",
        "fighters/human/runtime/upper_arm_front.png" => "upper_arm_front",
        "fighters/human/runtime/forearm_front.png" => "forearm_front",
        "fighters/human/runtime/hand_front.png" => "hand_front",
        "fighters/human/runtime/thigh_front.png" => "thigh_front",
        "fighters/human/runtime/shin_front.png" => "shin_front",
        "fighters/human/runtime/foot_front.png" => "foot_front",
        _ => return None,
    })
}

fn selected_parts(selections: &PartSelections) -> Vec<(&PartId, BodyRegion)> {
    let mut parts = vec![
        (&selections.body, BodyRegion::Body),
        (&selections.face, BodyRegion::Face),
        (&selections.hair, BodyRegion::Hair),
        (&selections.torso, BodyRegion::Torso),
        (&selections.legs, BodyRegion::Legs),
        (&selections.feet, BodyRegion::Feet),
    ];
    if let Some(facial_hair) = &selections.facial_hair {
        parts.push((facial_hair, BodyRegion::FacialHair));
    }
    if let Some(waist) = &selections.waist {
        parts.push((waist, BodyRegion::Waist));
    }
    parts.extend(
        selections
            .accessories
            .iter()
            .map(|accessory| (accessory, BodyRegion::Accessory)),
    );
    parts
}

/// A definition whose selected parts were looked up in a validated catalog.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCharacter {
    definition: CharacterDefinition,
    parts: HashMap<PartId, PartRecord>,
}

impl ResolvedCharacter {
    /// Returns the definition that supplied these resolved records.
    pub fn definition(&self) -> &CharacterDefinition {
        &self.definition
    }

    /// Returns the selected records keyed by their stable catalog IDs.
    pub fn parts(&self) -> &HashMap<PartId, PartRecord> {
        &self.parts
    }
}

/// The reason an authored catalog or requested definition cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    InvalidJson(String),
    UnsupportedCatalogVersion {
        found: u32,
        supported: u32,
    },
    UnsupportedDefinitionVersion {
        found: u32,
        supported: u32,
    },
    UnregisteredAssetPath {
        part_id: PartId,
        asset_path: String,
    },
    IncompatibleAttachmentPoint {
        part_id: PartId,
        region: BodyRegion,
        point: String,
    },
    AssetAttachmentMismatch {
        part_id: PartId,
        asset_path: String,
        expected: String,
        actual: String,
    },
    InvalidMaterialChannelPath {
        part_id: PartId,
        channel: String,
        asset_path: String,
    },
    InvalidMaterialNumeric {
        part_id: PartId,
        setting: String,
    },
    DuplicatePartId(PartId),
    MissingRequiredRegion(BodyRegion),
    MissingCompanionPart {
        part_id: PartId,
        companion: PartId,
    },
    MissingExclusionPart {
        part_id: PartId,
        exclusion: PartId,
    },
    MissingSelectedCompanion {
        part_id: PartId,
        companion: PartId,
    },
    UnknownPart(PartId),
    IncompatibleSkeleton {
        part_id: PartId,
        skeleton: SkeletonFamily,
    },
    IncompatibleCulture {
        part_id: PartId,
        culture_tags: Vec<String>,
    },
    WrongRegion {
        part_id: PartId,
        expected: BodyRegion,
        actual: BodyRegion,
    },
    ExcludedPartCombination {
        part_id: PartId,
        excluded: PartId,
    },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(error) => {
                write!(formatter, "invalid character catalog JSON: {error}")
            }
            Self::UnsupportedCatalogVersion { found, supported } => write!(
                formatter,
                "unsupported character catalog version {found} (expected {supported})"
            ),
            Self::UnsupportedDefinitionVersion { found, supported } => write!(
                formatter,
                "unsupported character definition version {found} (expected {supported})"
            ),
            Self::UnregisteredAssetPath {
                part_id,
                asset_path,
            } => write!(
                formatter,
                "part `{part_id}` references unregistered runtime asset `{asset_path}`"
            ),
            Self::IncompatibleAttachmentPoint {
                part_id,
                region,
                point,
            } => write!(
                formatter,
                "part `{part_id}` in `{region:?}` cannot attach at `{point}`"
            ),
            Self::AssetAttachmentMismatch {
                part_id,
                asset_path,
                expected,
                actual,
            } => write!(
                formatter,
                "part `{part_id}` asset `{asset_path}` is registered for `{expected}`, not `{actual}`"
            ),
            Self::InvalidMaterialChannelPath {
                part_id,
                channel,
                asset_path,
            } => write!(
                formatter,
                "part `{part_id}` material `{channel}` must be a human runtime PNG path, not `{asset_path}`"
            ),
            Self::InvalidMaterialNumeric { part_id, setting } => write!(
                formatter,
                "part `{part_id}` material `{setting}` must be finite and within its supported range"
            ),
            Self::DuplicatePartId(id) => write!(formatter, "duplicate character part ID `{id}`"),
            Self::MissingRequiredRegion(region) => {
                write!(
                    formatter,
                    "human catalog is missing required region `{region:?}`"
                )
            }
            Self::MissingCompanionPart { part_id, companion } => {
                write!(
                    formatter,
                    "part `{part_id}` references missing companion `{companion}`"
                )
            }
            Self::MissingExclusionPart { part_id, exclusion } => {
                write!(
                    formatter,
                    "part `{part_id}` references missing exclusion `{exclusion}`"
                )
            }
            Self::MissingSelectedCompanion { part_id, companion } => {
                write!(
                    formatter,
                    "part `{part_id}` requires selected companion `{companion}`"
                )
            }
            Self::UnknownPart(id) => write!(formatter, "unknown character part `{id}`"),
            Self::IncompatibleSkeleton { part_id, skeleton } => {
                write!(
                    formatter,
                    "part `{part_id}` is incompatible with skeleton `{skeleton:?}`"
                )
            }
            Self::IncompatibleCulture {
                part_id,
                culture_tags,
            } => write!(
                formatter,
                "part `{part_id}` shares no cultural tag with {culture_tags:?}"
            ),
            Self::WrongRegion {
                part_id,
                expected,
                actual,
            } => write!(
                formatter,
                "part `{part_id}` occupies `{actual:?}`, not required region `{expected:?}`"
            ),
            Self::ExcludedPartCombination { part_id, excluded } => {
                write!(formatter, "part `{part_id}` excludes `{excluded}`")
            }
        }
    }
}

impl std::error::Error for CatalogError {}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::character::{
        CHARACTER_DEFINITION_VERSION, CharacterDefinition, CulturalProfile, PartId,
        PlayerAppearance, SkeletonFamily,
    };

    use super::{
        BodyRegion, CHARACTER_CATALOG_VERSION, CatalogError, CharacterCatalog, MaterialMetadata,
        PaletteRegion, validate_part_content,
    };

    fn fixture() -> CharacterCatalog {
        CharacterCatalog::from_json(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON")
    }

    fn fixture_from(value: Value) -> CharacterCatalog {
        CharacterCatalog::from_json(&value.to_string()).expect("fixture remains valid JSON")
    }

    fn fixture_without(id: &str) -> CharacterCatalog {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .retain(|part| part["id"] != id);
        fixture_from(value)
    }

    #[test]
    fn human_catalog_rejects_duplicate_ids() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        let duplicate = value["parts"][0].clone();
        value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .push(duplicate);
        let catalog = fixture_from(value);

        assert_eq!(
            catalog.validate(),
            Err(CatalogError::DuplicatePartId(
                PartId::new("human.body.foundation.v1").unwrap()
            ))
        );
    }

    #[test]
    fn catalog_rejects_an_unsupported_future_schema_version() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        value["version"] = serde_json::json!(CHARACTER_CATALOG_VERSION + 1);

        assert_eq!(
            CharacterCatalog::from_json(&value.to_string()).expect_err("future schema is rejected"),
            CatalogError::UnsupportedCatalogVersion {
                found: CHARACTER_CATALOG_VERSION + 1,
                supported: CHARACTER_CATALOG_VERSION,
            }
        );
    }

    #[test]
    fn resolution_rejects_an_unsupported_character_definition_version() {
        let catalog = fixture();
        let mut definition = CharacterDefinition::legacy_human(PlayerAppearance::default());
        definition.version = CHARACTER_DEFINITION_VERSION + 1;

        assert_eq!(
            catalog.resolve(&definition),
            Err(CatalogError::UnsupportedDefinitionVersion {
                found: CHARACTER_DEFINITION_VERSION + 1,
                supported: CHARACTER_DEFINITION_VERSION,
            })
        );
    }

    #[test]
    fn catalog_rejects_an_asset_path_that_is_not_runtime_registered() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        let part = value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .iter_mut()
            .find(|part| part["id"] == "human.hair.long.v1")
            .expect("fixture has long hair");
        part["asset_path"] = serde_json::json!("fighters/human/runtime/missing.png");
        let catalog = fixture_from(value);

        assert_eq!(
            catalog.validate(),
            Err(CatalogError::UnregisteredAssetPath {
                part_id: PartId::new("human.hair.long.v1").unwrap(),
                asset_path: "fighters/human/runtime/missing.png".to_owned(),
            })
        );
    }

    #[test]
    fn catalog_accepts_typed_optional_material_metadata() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        let part = value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .iter_mut()
            .find(|part| part["id"] == "human.hair.long.v1")
            .expect("fixture has long hair");
        part["material"] = serde_json::json!({
            "mask_path": "fighters/human/runtime/hair.png",
            "normal_path": "fighters/human/runtime/hair.png",
            "shadow_path": "fighters/human/runtime/hair.png",
            "palette": ["hair"],
            "depth_offset": 0.25,
            "highlight": 0.75
        });

        let catalog = fixture_from(value);
        let material = &catalog
            .part(&PartId::new("human.hair.long.v1").unwrap())
            .expect("fixture has long hair")
            .material;

        assert_eq!(
            material,
            &MaterialMetadata {
                mask_path: Some("fighters/human/runtime/hair.png".to_owned()),
                normal_path: Some("fighters/human/runtime/hair.png".to_owned()),
                shadow_path: Some("fighters/human/runtime/hair.png".to_owned()),
                palette: vec![PaletteRegion::Hair],
                depth_offset: Some(0.25),
                highlight: Some(0.75),
            }
        );
        assert_eq!(catalog.validate(), Ok(()));
    }

    #[test]
    fn catalog_rejects_a_material_channel_outside_the_runtime_directory() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        let part = value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .iter_mut()
            .find(|part| part["id"] == "human.hair.long.v1")
            .expect("fixture has long hair");
        part["material"] = serde_json::json!({
            "mask_path": "fighters/human/source/hair.png"
        });
        let catalog = fixture_from(value);

        assert_eq!(
            catalog.validate(),
            Err(CatalogError::InvalidMaterialChannelPath {
                part_id: PartId::new("human.hair.long.v1").unwrap(),
                channel: "mask_path".to_owned(),
                asset_path: "fighters/human/source/hair.png".to_owned(),
            })
        );
    }

    #[test]
    fn catalog_rejects_out_of_range_material_numeric_settings() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        let part = value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .iter_mut()
            .find(|part| part["id"] == "human.hair.long.v1")
            .expect("fixture has long hair");
        part["material"] = serde_json::json!({ "depth_offset": 1.01 });
        let catalog = fixture_from(value);

        assert_eq!(
            catalog.validate(),
            Err(CatalogError::InvalidMaterialNumeric {
                part_id: PartId::new("human.hair.long.v1").unwrap(),
                setting: "depth_offset".to_owned(),
            })
        );
    }

    #[test]
    fn catalog_rejects_non_finite_material_numeric_settings() {
        let mut part = fixture()
            .part(&PartId::new("human.hair.long.v1").unwrap())
            .expect("fixture has long hair")
            .clone();
        part.material.highlight = Some(f32::NAN);

        assert_eq!(
            validate_part_content(&part),
            Err(CatalogError::InvalidMaterialNumeric {
                part_id: PartId::new("human.hair.long.v1").unwrap(),
                setting: "highlight".to_owned(),
            })
        );
    }

    #[test]
    fn catalog_rejects_an_attachment_point_incompatible_with_its_region() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        let part = value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .iter_mut()
            .find(|part| part["id"] == "human.hair.long.v1")
            .expect("fixture has long hair");
        part["attachment"]["point"] = serde_json::json!("torso");
        let catalog = fixture_from(value);

        assert_eq!(
            catalog.validate(),
            Err(CatalogError::IncompatibleAttachmentPoint {
                part_id: PartId::new("human.hair.long.v1").unwrap(),
                region: BodyRegion::Hair,
                point: "torso".to_owned(),
            })
        );
    }

    #[test]
    fn every_bundled_catalog_asset_exists_on_disk() {
        let catalog = fixture();
        for record in catalog.parts.values() {
            assert!(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("assets")
                    .join(&record.asset_path)
                    .is_file(),
                "{} must exist for {}",
                record.asset_path,
                record.id
            );
        }
    }

    #[test]
    fn human_catalog_rejects_a_missing_torso() {
        let catalog = fixture_without("human.torso.linen.v1");

        assert_eq!(
            catalog.validate(),
            Err(CatalogError::MissingRequiredRegion(BodyRegion::Torso))
        );
    }

    #[test]
    fn resolution_rejects_a_part_incompatible_with_the_human_skeleton() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .iter_mut()
            .find(|part| part["id"] == "human.body.foundation.v1")
            .expect("fixture has a body part")["skeletons"] = serde_json::json!([]);
        let catalog = fixture_from(value);

        assert_eq!(
            catalog.resolve(&CharacterDefinition::legacy_human(
                PlayerAppearance::default()
            )),
            Err(CatalogError::IncompatibleSkeleton {
                part_id: PartId::new("human.body.foundation.v1").unwrap(),
                skeleton: SkeletonFamily::Human,
            })
        );
    }

    #[test]
    fn human_catalog_rejects_a_missing_companion_part() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .iter_mut()
            .find(|part| part["id"] == "human.torso.linen.v1")
            .expect("fixture has a torso part")["companions"] =
            serde_json::json!(["human.accessory.missing.v1"]);
        let catalog = fixture_from(value);

        assert_eq!(
            catalog.validate(),
            Err(CatalogError::MissingCompanionPart {
                part_id: PartId::new("human.torso.linen.v1").unwrap(),
                companion: PartId::new("human.accessory.missing.v1").unwrap(),
            })
        );
    }

    #[test]
    fn human_catalog_rejects_a_dangling_exclusion_id() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .iter_mut()
            .find(|part| part["id"] == "human.torso.linen.v1")
            .expect("fixture has a torso part")["exclusions"] =
            serde_json::json!(["human.accessory.missing.v1"]);
        let catalog = fixture_from(value);

        assert_eq!(
            catalog.validate(),
            Err(CatalogError::MissingExclusionPart {
                part_id: PartId::new("human.torso.linen.v1").unwrap(),
                exclusion: PartId::new("human.accessory.missing.v1").unwrap(),
            })
        );
    }

    #[test]
    fn resolution_rejects_parts_without_a_shared_cultural_tag() {
        let catalog = fixture();
        let mut definition = CharacterDefinition::legacy_human(PlayerAppearance::default());
        definition.culture.tags = vec!["generic_fantasy".to_owned()];

        assert_eq!(
            catalog.resolve(&definition),
            Err(CatalogError::IncompatibleCulture {
                part_id: PartId::new("human.body.foundation.v1").unwrap(),
                culture_tags: vec!["generic_fantasy".to_owned()],
            })
        );
    }

    #[test]
    fn validation_resolves_the_catalog_fallback_under_full_human_rules() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../assets/fighters/catalog/human-foundation.json"
        ))
        .expect("human foundation fixture is valid JSON");
        value["parts"]
            .as_array_mut()
            .expect("fixture has parts")
            .iter_mut()
            .find(|part| part["id"] == "human.body.foundation.v1")
            .expect("fixture has a body part")["cultural_tags"] =
            serde_json::json!(["generic_fantasy"]);
        let catalog = fixture_from(value);

        assert_eq!(
            catalog.validate(),
            Err(CatalogError::IncompatibleCulture {
                part_id: PartId::new("human.body.foundation.v1").unwrap(),
                culture_tags: vec!["romanian".to_owned()],
            })
        );
    }

    #[test]
    fn known_good_human_resolves_all_selected_parts() {
        let catalog = fixture();
        let definition = CharacterDefinition {
            version: CHARACTER_DEFINITION_VERSION,
            seed: None,
            skeleton: SkeletonFamily::Human,
            culture: CulturalProfile {
                tags: catalog.known_good_human_cultural_tags().to_vec(),
            },
            parts: catalog.known_good_human().clone(),
            appearance: PlayerAppearance::default(),
        };

        assert_eq!(catalog.validate(), Ok(()));

        let resolved = catalog.resolve(&definition).unwrap();
        assert_eq!(resolved.definition(), &definition);
        assert_eq!(resolved.parts().len(), 6);
    }
}
